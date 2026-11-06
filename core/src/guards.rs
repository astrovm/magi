//! Abuse protection: rate limits, Turnstile and Safe Browsing.

use serde_json::{Value, json};

use crate::platform::{Platform, PutOptions, Req, Result};
use crate::secrets::sha256_hex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limit {
    /// Most attempts allowed inside the window.
    pub max: u64,
    /// Window length in seconds. KV needs at least 60.
    pub window: u64,
}

/// Wrong passwords allowed per link and visitor before a lockout.
pub const UNLOCK_LIMIT: Limit = Limit { max: 5, window: 900 };

/// Links one visitor can create per hour.
pub const CREATE_LIMIT: Limit = Limit { max: 30, window: 3600 };

pub const TURNSTILE_VERIFY_URL: &str = "https://challenges.cloudflare.com/turnstile/v0/siteverify";
pub const SAFE_BROWSING_URL: &str = "https://safebrowsing.googleapis.com/v4/threatMatches:find";

/// Hashed so no raw IPs end up in storage.
pub fn limit_key(scope: &str, req: &Req) -> String {
    let visitor = sha256_hex(req.visitor_ip.as_deref().unwrap_or("unknown"));
    format!("limit:{scope}:{visitor}")
}

async fn attempts<P: Platform>(platform: &P, key: &str) -> Result<u64> {
    let value = platform.kv_get(key).await?;
    Ok(value.and_then(|count| count.parse().ok()).unwrap_or(0))
}

pub async fn is_limited<P: Platform>(platform: &P, key: &str, limit: Limit) -> Result<bool> {
    Ok(attempts(platform, key).await? >= limit.max)
}

/// Counts one attempt. Like the click counter, concurrent attempts can be
/// undercounted, which is fine for slowing down guessers and spammers.
pub async fn count_attempt<P: Platform>(platform: &P, key: &str, limit: Limit) -> Result<()> {
    let count = attempts(platform, key).await?.saturating_add(1);
    let options = PutOptions {
        expiration_ttl: Some(limit.window),
        ..PutOptions::default()
    };
    platform.kv_put(key, &count.to_string(), options).await
}

/// Checks the Turnstile token from the form. Without a secret the check is off.
pub async fn passes_turnstile<P: Platform>(
    platform: &P,
    secret: Option<&str>,
    token: Option<&str>,
    req: &Req,
) -> bool {
    let Some(secret) = secret else {
        return true;
    };
    let Some(token) = token.filter(|token| !token.is_empty()) else {
        return false;
    };
    let body = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("secret", secret)
        .append_pair("response", token)
        .append_pair("remoteip", req.visitor_ip.as_deref().unwrap_or_default())
        .finish();
    let Ok(reply) = platform
        .http_post(TURNSTILE_VERIFY_URL, "application/x-www-form-urlencoded", body)
        .await
    else {
        return false;
    };
    serde_json::from_str::<Value>(&reply.body)
        .is_ok_and(|result| result.get("success") == Some(&Value::Bool(true)))
}

/// Asks Google Safe Browsing about some destinations in one call. Without a
/// key the check is off. Fails open: if Google is down, links still get created.
pub async fn is_dangerous<P: Platform>(
    platform: &P,
    api_key: Option<&str>,
    urls: &[&str],
) -> bool {
    let Some(api_key) = api_key else {
        return false;
    };
    let key: String = url::form_urlencoded::byte_serialize(api_key.as_bytes()).collect();
    let entries: Vec<Value> = urls.iter().map(|url| json!({ "url": url })).collect();
    let body = json!({
        "client": { "clientId": "magi", "clientVersion": "1" },
        "threatInfo": {
            "threatTypes": ["MALWARE", "SOCIAL_ENGINEERING", "UNWANTED_SOFTWARE", "POTENTIALLY_HARMFUL_APPLICATION"],
            "platformTypes": ["ANY_PLATFORM"],
            "threatEntryTypes": ["URL"],
            "threatEntries": entries,
        },
    });
    let Ok(reply) = platform
        .http_post(
            &format!("{SAFE_BROWSING_URL}?key={key}"),
            "application/json",
            body.to_string(),
        )
        .await
    else {
        return false;
    };
    if !(200..300).contains(&reply.status) {
        return false;
    }
    serde_json::from_str::<Value>(&reply.body).is_ok_and(|result| {
        result
            .get("matches")
            .and_then(Value::as_array)
            .is_some_and(|matches| !matches.is_empty())
    })
}
