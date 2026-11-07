//! What the server needs from the outside world, and how it sees requests.

use std::future::Future;

use serde::{Deserialize, Serialize};
use url::{Position, Url};

use crate::live::Role;
use crate::store::LinkMeta;
use crate::target::host_with_port;
use crate::visits::Background;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformError(pub String);

impl std::fmt::Display for PlatformError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

pub type Result<T> = std::result::Result<T, PlatformError>;

/// Where short links live.
const SHORT_ADDRESS: &str = "s.4st.li";
/// Where the site lives. A browser keeps manage keys per address, so pages that
/// need them open here.
const HOME_ADDRESS: &str = "magi.4st.li";
/// This one still answers, so links shared with it keep working.
const OLD_ADDRESS: &str = "r.4st.li";

pub fn is_our_address(host: &str) -> bool {
    [SHORT_ADDRESS, HOME_ADDRESS, OLD_ADDRESS].contains(&host)
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PutOptions {
    pub expiration: Option<u64>,
    pub expiration_ttl: Option<u64>,
    pub metadata: Option<LinkMeta>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpReply {
    pub status: u16,
    pub body: String,
}

pub trait Platform {
    fn kv_get(&self, key: &str) -> impl Future<Output = Result<Option<String>>>;
    fn kv_get_with_metadata(
        &self,
        key: &str,
        cache_ttl: u64,
    ) -> impl Future<Output = Result<Option<(String, Option<LinkMeta>)>>>;
    fn kv_put(
        &self,
        key: &str,
        value: &str,
        options: PutOptions,
    ) -> impl Future<Output = Result<()>>;
    fn kv_delete(&self, key: &str) -> impl Future<Output = Result<()>>;
    fn http_post(
        &self,
        url: &str,
        content_type: &str,
        body: String,
    ) -> impl Future<Output = Result<HttpReply>>;
    /// PBKDF2-HMAC-SHA256. The runtime's native version is about five times
    /// faster than compiling one to Wasm, which matters on a 10 ms CPU budget.
    fn pbkdf2(
        &self,
        password: &str,
        salt: &[u8],
        iterations: u32,
    ) -> impl Future<Output = Result<Vec<u8>>>;
    /// Calls a live paste's room. `path` is one of the `ROOM_*` paths.
    fn live_call(
        &self,
        room: &str,
        path: &str,
        body: String,
    ) -> impl Future<Output = Result<HttpReply>>;
    /// Not for secrets: picks silly aliases and curse victims. In `0..1`.
    fn random(&self) -> f64;
    fn random_bytes(&self, length: usize) -> Result<Vec<u8>>;
    fn now_ms(&self) -> u64;
}

/// Optional settings. Without them, each feature stays off.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    pub telegram_bot_token: Option<String>,
    pub telegram_bot_username: Option<String>,
    pub telegram_webhook_secret: Option<String>,
    pub turnstile_site_key: Option<String>,
    pub turnstile_secret_key: Option<String>,
    pub safe_browsing_api_key: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Head,
    Post,
    Other,
}

impl Method {
    pub fn parse(name: &str) -> Self {
        match name {
            "GET" => Self::Get,
            "HEAD" => Self::Head,
            "POST" => Self::Post,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormValue {
    Text(String),
    File,
}

/// Every form field the server reads, so the runtime knows what to copy over.
pub const FORM_FIELDS: [&str; 15] = [
    "action",
    "alias",
    "burn",
    "cf-turnstile-response",
    "count",
    "cursed",
    "expires",
    "fortune",
    "key",
    "live",
    "password",
    "snapshot",
    "text",
    "url",
    "wait",
];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Form {
    fields: Vec<(String, FormValue)>,
}

impl Form {
    pub fn new(fields: Vec<(String, FormValue)>) -> Self {
        Self { fields }
    }

    pub fn get(&self, name: &str) -> Option<&FormValue> {
        self.fields
            .iter()
            .find(|(field, _)| field == name)
            .map(|(_, value)| value)
    }

    /// The text of a field, if it is present and not a file.
    pub fn text(&self, name: &str) -> Option<&str> {
        match self.get(name) {
            Some(FormValue::Text(text)) => Some(text),
            _ => None,
        }
    }

    pub fn is_file(&self, name: &str) -> bool {
        matches!(self.get(name), Some(FormValue::File))
    }

    /// Like an HTML checkbox.
    pub fn is_on(&self, name: &str) -> bool {
        self.text(name) == Some("on")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    None,
    Form(Form),
    Text(String),
    /// A body that promised to be something it wasn't.
    Unreadable,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Place {
    pub city: Option<String>,
    pub country: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Req {
    pub method: Method,
    pub url: Url,
    pub body: Body,
    /// The page script asks for bare HTML with `X-Orb: fragment`.
    pub wants_fragment: bool,
    pub visitor_ip: Option<String>,
    pub user_agent: String,
    pub telegram_secret: String,
    pub place: Place,
    /// The request asks to become a WebSocket.
    pub is_websocket: bool,
    /// The `Origin` header. Browsers always send it on a WebSocket.
    pub origin_header: Option<String>,
}

impl Req {
    pub fn new(method: Method, url: Url) -> Self {
        Self {
            method,
            url,
            body: Body::None,
            wants_fragment: false,
            visitor_ip: None,
            user_agent: String::new(),
            telegram_secret: String::new(),
            place: Place::default(),
            is_websocket: false,
            origin_header: None,
        }
    }

    pub fn host(&self) -> String {
        host_with_port(&self.url)
    }

    pub fn origin(&self) -> String {
        format!("{}://{}", self.url.scheme(), self.host())
    }

    /// The address links should say. A try-out deploy keeps its own.
    pub fn short_host(&self) -> String {
        let host = self.host();
        if is_our_address(&host) {
            SHORT_ADDRESS.to_owned()
        } else {
            host
        }
    }

    pub fn short_origin(&self) -> String {
        format!("{}://{}", self.url.scheme(), self.short_host())
    }

    /// This page at the site's home, if we aren't there. A try-out deploy stays.
    pub fn url_at_home(&self) -> Option<String> {
        let host = self.host();
        (host != HOME_ADDRESS && is_our_address(&host)).then(|| {
            let scheme = self.url.scheme();
            let rest = &self.url[Position::BeforePath..];
            format!("{scheme}://{HOME_ADDRESS}{rest}")
        })
    }

    pub fn query(&self, name: &str) -> Option<String> {
        self.url
            .query_pairs()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.into_owned())
    }

    pub fn form(&self) -> Option<&Form> {
        match &self.body {
            Body::Form(form) => Some(form),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Res {
    pub status: u16,
    pub headers: Vec<(&'static str, String)>,
    pub body: Option<String>,
}

impl Res {
    pub fn new(status: u16) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: None,
        }
    }

    pub fn with_header(mut self, name: &'static str, value: impl Into<String>) -> Self {
        self.headers.push((name, value.into()));
        self
    }

    pub fn with_body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(header, _)| *header == name)
            .map(|(_, value)| value.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Let the static site answer.
    Asset,
    Reply {
        response: Res,
        /// Work to finish after answering.
        background: Option<Background>,
    },
    /// Hand the request over to a live paste's room.
    LiveSocket { room: String, role: Role },
}

impl From<Res> for Outcome {
    fn from(response: Res) -> Self {
        Self::Reply {
            response,
            background: None,
        }
    }
}
