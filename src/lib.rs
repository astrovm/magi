//! Plugs the site into Cloudflare. Everything it does lives in `magi-core`.

use js_sys::{Array, Math, Object, Reflect, Uint8Array};
use magi_core::platform::{
    Body, Config, FORM_FIELDS, Form, FormValue, HttpReply, Method as CoreMethod, Outcome, Place,
    Platform, PlatformError, PutOptions, Req, Res, Result as CoreResult,
};
use magi_core::responses::{ResponseKey, get_response};
use magi_core::routes::handle;
use magi_core::store::LinkMeta;
use magi_core::visits::run_background;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use worker::kv::KvStore;
use worker::{
    Context, Env, Fetch, FormEntry, Headers, Method, Request, RequestInit, Response, Result, event,
};

fn platform_error(error: impl std::fmt::Debug) -> PlatformError {
    PlatformError(format!("{error:?}"))
}

/// Same shape as the rest of the Workers logs, for searching them later.
fn log_error(context: &str, error: &str) {
    let line = serde_json::json!({ "level": "error", "context": context, "error": error });
    worker::console_error!("{line}");
}

struct Cloudflare {
    links: KvStore,
}

fn web_crypto() -> CoreResult<web_sys::Crypto> {
    let crypto =
        Reflect::get(&js_sys::global(), &JsValue::from_str("crypto")).map_err(platform_error)?;
    crypto.dyn_into().map_err(platform_error)
}

impl Platform for Cloudflare {
    async fn kv_get(&self, key: &str) -> CoreResult<Option<String>> {
        self.links.get(key).text().await.map_err(platform_error)
    }

    async fn kv_get_with_metadata(
        &self,
        key: &str,
        cache_ttl: u64,
    ) -> CoreResult<Option<(String, Option<LinkMeta>)>> {
        let (value, meta) = self
            .links
            .get(key)
            .cache_ttl(cache_ttl)
            .text_with_metadata::<LinkMeta>()
            .await
            .map_err(platform_error)?;
        Ok(value.map(|value| (value, meta)))
    }

    async fn kv_put(&self, key: &str, value: &str, options: PutOptions) -> CoreResult<()> {
        let mut put = self.links.put(key, value).map_err(platform_error)?;
        if let Some(expiration) = options.expiration {
            put = put.expiration(expiration);
        }
        if let Some(ttl) = options.expiration_ttl {
            put = put.expiration_ttl(ttl);
        }
        if let Some(metadata) = options.metadata {
            put = put.metadata(metadata).map_err(platform_error)?;
        }
        put.execute().await.map_err(platform_error)
    }

    async fn kv_delete(&self, key: &str) -> CoreResult<()> {
        self.links.delete(key).await.map_err(platform_error)
    }

    async fn http_post(
        &self,
        url: &str,
        content_type: &str,
        body: String,
    ) -> CoreResult<HttpReply> {
        let headers = Headers::new();
        headers
            .set("Content-Type", content_type)
            .map_err(platform_error)?;
        let mut init = RequestInit::new();
        init.with_method(Method::Post)
            .with_headers(headers)
            .with_body(Some(JsValue::from_str(&body)));
        let request = Request::new_with_init(url, &init).map_err(platform_error)?;
        let mut response = Fetch::Request(request)
            .send()
            .await
            .map_err(platform_error)?;
        Ok(HttpReply {
            status: response.status_code(),
            body: response.text().await.map_err(platform_error)?,
        })
    }

    async fn pbkdf2(&self, password: &str, salt: &[u8], iterations: u32) -> CoreResult<Vec<u8>> {
        let subtle = web_crypto()?.subtle();
        let usages = Array::of1(&JsValue::from_str("deriveBits"));
        let key_material: Object = Uint8Array::from(password.as_bytes()).into();
        let imported = subtle
            .import_key_with_str("raw", &key_material, "PBKDF2", false, &usages)
            .map_err(platform_error)?;
        let key = JsFuture::from(imported).await.map_err(platform_error)?;
        let salt: Object = Uint8Array::from(salt).into();
        let params =
            web_sys::Pbkdf2Params::new("PBKDF2", &JsValue::from_str("SHA-256"), iterations, &salt);
        let derived = subtle
            .derive_bits_with_object(&params, &key.unchecked_into(), 256)
            .map_err(platform_error)?;
        let bits = JsFuture::from(derived).await.map_err(platform_error)?;
        Ok(Uint8Array::new(&bits).to_vec())
    }

    fn random(&self) -> f64 {
        Math::random()
    }

    fn random_bytes(&self, length: usize) -> CoreResult<Vec<u8>> {
        let mut bytes = vec![0; length];
        web_crypto()?
            .get_random_values_with_u8_array(&mut bytes)
            .map_err(platform_error)?;
        Ok(bytes)
    }

    fn now_ms(&self) -> u64 {
        // JavaScript time is a whole, positive number of milliseconds.
        js_sys::Date::now() as u64
    }
}

/// Secrets and plain variables look the same to the site.
fn setting(env: &Env, name: &str) -> Option<String> {
    let value = env
        .secret(name)
        .map(|secret| secret.to_string())
        .or_else(|_| env.var(name).map(|var| var.to_string()))
        .ok()?;
    (!value.is_empty()).then_some(value)
}

fn read_config(env: &Env) -> Config {
    Config {
        telegram_bot_token: setting(env, "TELEGRAM_BOT_TOKEN"),
        telegram_bot_username: setting(env, "TELEGRAM_BOT_USERNAME"),
        telegram_webhook_secret: setting(env, "TELEGRAM_WEBHOOK_SECRET"),
        turnstile_site_key: setting(env, "TURNSTILE_SITE_KEY"),
        turnstile_secret_key: setting(env, "TURNSTILE_SECRET_KEY"),
        safe_browsing_api_key: setting(env, "SAFE_BROWSING_API_KEY"),
    }
}

async fn read_form(request: &mut Request) -> Body {
    let Ok(data) = request.form_data().await else {
        return Body::Unreadable;
    };
    let fields = FORM_FIELDS
        .iter()
        .filter_map(|name| {
            let value = match data.get(name)? {
                FormEntry::Field(text) => FormValue::Text(text),
                FormEntry::File(_) => FormValue::File,
            };
            Some(((*name).to_owned(), value))
        })
        .collect();
    Body::Form(Form::new(fields))
}

fn header(request: &Request, name: &str) -> Option<String> {
    request.headers().get(name).ok().flatten()
}

async fn read_request(request: &mut Request) -> Result<Req> {
    let url = request.url()?;
    let method = CoreMethod::parse(request.method().as_ref());
    let is_telegram = url.path().trim_end_matches('/') == "/telegram";
    let body = match method {
        CoreMethod::Post if is_telegram => match request.text().await {
            Ok(text) => Body::Text(text),
            Err(_) => Body::Unreadable,
        },
        CoreMethod::Post => read_form(request).await,
        _ => Body::None,
    };
    let cf = request.cf();
    let mut req = Req::new(method, url);
    req.body = body;
    req.wants_fragment = header(request, "X-Orb").as_deref() == Some("fragment");
    req.visitor_ip = header(request, "CF-Connecting-IP");
    req.user_agent = header(request, "User-Agent").unwrap_or_default();
    req.telegram_secret = header(request, "X-Telegram-Bot-Api-Secret-Token").unwrap_or_default();
    req.place = Place {
        city: cf.and_then(worker::Cf::city),
        country: cf.and_then(worker::Cf::country),
    };
    Ok(req)
}

fn to_response(res: Res) -> Result<Response> {
    let headers = Headers::new();
    for (name, value) in &res.headers {
        headers.set(name, value)?;
    }
    let response = match res.body {
        Some(body) => Response::from_bytes(body.into_bytes())?,
        None => Response::empty()?,
    };
    Ok(response.with_status(res.status).with_headers(headers))
}

#[event(fetch)]
async fn fetch(mut request: Request, env: Env, context: Context) -> Result<Response> {
    let config = read_config(&env);
    let platform = Cloudflare {
        links: env.kv("links")?,
    };
    let req = read_request(&mut request).await?;

    match handle(&platform, &config, &req).await {
        Ok(Outcome::Asset) => env.assets("ASSETS")?.fetch_request(request).await,
        Ok(Outcome::Reply {
            response,
            background,
        }) => {
            if let Some(job) = background {
                context.wait_until(async move {
                    if let Err(error) = run_background(&platform, &config, &job).await {
                        log_error("visit", &error.0);
                    }
                });
            }
            to_response(response)
        }
        Err(error) => {
            log_error("request", &error.0);
            to_response(get_response(&req, ResponseKey::ServerError))
        }
    }
}
