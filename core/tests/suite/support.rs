use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, VecDeque};

use magi_core::platform::{
    Body, Config, Form, FormValue, HttpReply, Method, Outcome, Platform, PlatformError, PutOptions,
    Req, Res, Result,
};
use magi_core::routes::handle;
use magi_core::store::LinkMeta;
use magi_core::visits::{Background, run_background};
use md5::{Digest, Md5};
use sha2::Sha256;
use url::Url;

pub const HOST: &str = "https://s.4st.li";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpCall {
    pub url: String,
    pub content_type: String,
    pub body: String,
}

/// Cloudflare with the network and the database swapped for memory.
pub struct Fake {
    pub kv: RefCell<BTreeMap<String, (String, Option<LinkMeta>)>>,
    pub gets: RefCell<Vec<(String, Option<u64>)>>,
    pub puts: RefCell<Vec<(String, String, PutOptions)>>,
    pub randoms: RefCell<VecDeque<f64>>,
    pub default_random: Cell<f64>,
    pub calls: RefCell<Vec<HttpCall>>,
    pub replies: RefCell<VecDeque<Result<HttpReply>>>,
    pub now: Cell<u64>,
    pub broken_kv: Cell<bool>,
    pub room_calls: RefCell<Vec<(String, String, String)>>,
    pub room_replies: RefCell<VecDeque<HttpReply>>,
    next_byte: Cell<u8>,
}

impl Fake {
    pub fn new() -> Self {
        Self {
            kv: RefCell::default(),
            gets: RefCell::default(),
            puts: RefCell::default(),
            randoms: RefCell::default(),
            default_random: Cell::new(0.99),
            calls: RefCell::default(),
            replies: RefCell::default(),
            now: Cell::new(1_800_000_000_000),
            broken_kv: Cell::new(false),
            room_calls: RefCell::default(),
            room_replies: RefCell::default(),
            next_byte: Cell::new(1),
        }
    }

    pub fn store(&self, alias: &str, value: &str, meta: Option<LinkMeta>) {
        self.kv
            .borrow_mut()
            .insert(md5_hex(alias), (value.to_owned(), meta));
    }

    pub fn value(&self, key: &str) -> Option<String> {
        self.kv.borrow().get(key).map(|(value, _)| value.clone())
    }

    pub fn link_value(&self, alias: &str) -> Option<String> {
        self.value(&md5_hex(alias))
    }

    pub fn meta(&self, alias: &str) -> Option<LinkMeta> {
        self.kv
            .borrow()
            .get(&md5_hex(alias))
            .and_then(|(_, meta)| meta.clone())
    }

    pub fn put_options(&self, key: &str) -> Option<PutOptions> {
        self.puts
            .borrow()
            .iter()
            .rev()
            .find(|(put_key, _, _)| put_key == key)
            .map(|(_, _, options)| options.clone())
    }

    pub fn set_randoms(&self, values: &[f64]) {
        *self.randoms.borrow_mut() = values.iter().copied().collect();
    }

    pub fn reply_with(&self, body: &str) {
        self.replies.borrow_mut().push_back(Ok(HttpReply {
            status: 200,
            body: body.to_owned(),
        }));
    }

    pub fn reply_status(&self, status: u16) {
        self.replies.borrow_mut().push_back(Ok(HttpReply {
            status,
            body: "nope".to_owned(),
        }));
    }

    pub fn reply_failure(&self) {
        self.replies
            .borrow_mut()
            .push_back(Err(PlatformError("offline".to_owned())));
    }

    fn check_kv(&self) -> Result<()> {
        if self.broken_kv.get() {
            Err(PlatformError("kv down".to_owned()))
        } else {
            Ok(())
        }
    }
}

impl Platform for Fake {
    async fn kv_get(&self, key: &str) -> Result<Option<String>> {
        self.check_kv()?;
        self.gets.borrow_mut().push((key.to_owned(), None));
        Ok(self.value(key))
    }

    async fn kv_get_with_metadata(
        &self,
        key: &str,
        cache_ttl: u64,
    ) -> Result<Option<(String, Option<LinkMeta>)>> {
        self.check_kv()?;
        self.gets
            .borrow_mut()
            .push((key.to_owned(), Some(cache_ttl)));
        Ok(self.kv.borrow().get(key).cloned())
    }

    async fn kv_put(&self, key: &str, value: &str, options: PutOptions) -> Result<()> {
        self.check_kv()?;
        self.puts
            .borrow_mut()
            .push((key.to_owned(), value.to_owned(), options.clone()));
        self.kv
            .borrow_mut()
            .insert(key.to_owned(), (value.to_owned(), options.metadata));
        Ok(())
    }

    async fn kv_delete(&self, key: &str) -> Result<()> {
        self.check_kv()?;
        self.kv.borrow_mut().remove(key);
        Ok(())
    }

    async fn http_post(&self, url: &str, content_type: &str, body: String) -> Result<HttpReply> {
        self.calls.borrow_mut().push(HttpCall {
            url: url.to_owned(),
            content_type: content_type.to_owned(),
            body,
        });
        self.replies
            .borrow_mut()
            .pop_front()
            .unwrap_or(Ok(HttpReply {
                status: 200,
                body: "{}".to_owned(),
            }))
    }

    async fn pbkdf2(&self, password: &str, salt: &[u8], iterations: u32) -> Result<Vec<u8>> {
        let mut out = [0u8; 32];
        pbkdf2::pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, iterations, &mut out);
        Ok(out.to_vec())
    }

    async fn live_call(&self, room: &str, path: &str, body: String) -> Result<HttpReply> {
        self.room_calls
            .borrow_mut()
            .push((room.to_owned(), path.to_owned(), body));
        Ok(self
            .room_replies
            .borrow_mut()
            .pop_front()
            .unwrap_or(HttpReply {
                status: 200,
                body: String::new(),
            }))
    }

    fn random(&self) -> f64 {
        self.randoms
            .borrow_mut()
            .pop_front()
            .unwrap_or(self.default_random.get())
    }

    fn random_bytes(&self, length: usize) -> Result<Vec<u8>> {
        let start = self.next_byte.get();
        self.next_byte.set(start.wrapping_add(1));
        Ok((0..length)
            .map(|index| start.wrapping_add(index as u8))
            .collect())
    }

    fn now_ms(&self) -> u64 {
        self.now.get()
    }
}

pub fn md5_hex(text: &str) -> String {
    magi_core::text::to_hex(&Md5::digest(text.as_bytes()))
}

pub fn sha256_hex(text: &str) -> String {
    magi_core::secrets::sha256_hex(text)
}

pub fn block<T>(future: impl std::future::Future<Output = T>) -> T {
    pollster::block_on(future)
}

pub fn request(method: Method, path: &str) -> Req {
    Req::new(
        method,
        Url::parse(&format!("{HOST}{path}")).expect("test url"),
    )
}

pub fn get(path: &str) -> Req {
    request(Method::Get, path)
}

pub fn head(path: &str) -> Req {
    request(Method::Head, path)
}

pub fn form(fields: &[(&str, &str)]) -> Form {
    Form::new(
        fields
            .iter()
            .map(|(name, value)| ((*name).to_owned(), FormValue::Text((*value).to_owned())))
            .collect(),
    )
}

/// A form post from the page script, which asks for a bare fragment.
pub fn post(path: &str, fields: &[(&str, &str)]) -> Req {
    let mut req = plain_post(path, fields);
    req.wants_fragment = true;
    req
}

/// A form post from a browser with no JavaScript.
pub fn plain_post(path: &str, fields: &[(&str, &str)]) -> Req {
    let mut req = request(Method::Post, path);
    req.body = Body::Form(form(fields));
    req
}

pub fn from_ip(mut req: Req, ip: &str) -> Req {
    req.visitor_ip = Some(ip.to_owned());
    req
}

pub fn run(fake: &Fake, config: &Config, req: &Req) -> Outcome {
    block(handle(fake, config, req)).expect("request should not fail")
}

pub fn parts(outcome: Outcome) -> (Res, Option<Background>) {
    match outcome {
        Outcome::Reply {
            response,
            background,
        } => (response, background),
        other => panic!("expected a reply, got {other:?}"),
    }
}

/// Answers a request and does what's left afterwards, like the runtime does.
pub fn visit(fake: &Fake, config: &Config, req: &Req) -> Res {
    let (response, background) = parts(run(fake, config, req));
    if let Some(job) = background {
        block(run_background(fake, config, &job)).expect("background work should not fail");
    }
    response
}

pub fn answer(fake: &Fake, req: &Req) -> Res {
    visit(fake, &Config::default(), req)
}

pub fn body(response: &Res) -> &str {
    response.body.as_deref().unwrap_or_default()
}

pub fn between<'a>(text: &'a str, start: &str, end: &str) -> &'a str {
    let after = text.split_once(start).map_or("", |(_, after)| after);
    after.split_once(end).map_or("", |(inside, _)| inside)
}

pub fn manage_key_of(html: &str) -> String {
    between(html, r#"class="secret">"#, "<").to_owned()
}
