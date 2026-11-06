//! Links in KV. The format is shared with every link already stored, so it
//! can't change: the destinations are the value, the spells are the metadata.

use serde::{Deserialize, Serialize};

use crate::platform::{Platform, PutOptions, Result};

/// Seconds KV may cache an alias lookup. Kept short so edits, banishments and
/// self-destructs show up quickly.
pub const LINK_CACHE_TTL: u64 = 300;

/// Extra spells stored as KV metadata next to the destination. Links created
/// before these existed have no metadata at all. Keys are short because KV
/// metadata is capped at 1024 bytes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkMeta {
    /// Creation time in epoch milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<u64>,
    /// SHA-256 of the manage key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// PBKDF2 password hash as `salt.hash`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pw: Option<String>,
    /// Self-destruct after this many visits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<u64>,
    /// Count visits.
    #[serde(default, skip_serializing_if = "is_false")]
    pub count: bool,
    /// Sometimes rickroll visitors.
    #[serde(default, skip_serializing_if = "is_false")]
    pub cursed: bool,
    /// Expiration in epoch seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exp: Option<u64>,
    /// Telegram chat that gets visit alerts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tg: Option<i64>,
    /// Show the visitor a countdown before redirecting.
    #[serde(default, skip_serializing_if = "is_false")]
    pub wait: bool,
}

// Serde's `skip_serializing_if` hands over a reference.
fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkRecord {
    pub url: String,
    /// Extra fortune cookie destinations, picked at random with `url`.
    pub more: Vec<String>,
    pub meta: LinkMeta,
}

pub fn clicks_key(alias_hash: &str) -> String {
    format!("clicks:{alias_hash}")
}

pub async fn get_link<P: Platform>(platform: &P, alias_hash: &str) -> Result<Option<LinkRecord>> {
    let Some((value, meta)) = platform.kv_get_with_metadata(alias_hash, LINK_CACHE_TTL).await?
    else {
        return Ok(None);
    };
    // Fortune cookie links store every destination, one per line.
    let mut lines = value.split('\n').map(str::to_owned);
    let url = lines.next().unwrap_or_default();
    Ok(Some(LinkRecord {
        url,
        more: lines.collect(),
        meta: meta.unwrap_or_default(),
    }))
}

pub async fn put_link<P: Platform>(
    platform: &P,
    alias_hash: &str,
    record: &LinkRecord,
) -> Result<()> {
    let mut lines = vec![record.url.as_str()];
    lines.extend(record.more.iter().map(String::as_str));
    let options = PutOptions {
        expiration: record.meta.exp,
        expiration_ttl: None,
        metadata: Some(record.meta.clone()),
    };
    platform.kv_put(alias_hash, &lines.join("\n"), options).await
}

pub async fn delete_link<P: Platform>(platform: &P, alias_hash: &str) -> Result<()> {
    let link = platform.kv_delete(alias_hash).await;
    let clicks = platform.kv_delete(&clicks_key(alias_hash)).await;
    link.and(clicks)
}

pub async fn get_clicks<P: Platform>(platform: &P, alias_hash: &str) -> Result<u64> {
    let value = platform.kv_get(&clicks_key(alias_hash)).await?;
    Ok(value.and_then(|clicks| clicks.parse().ok()).unwrap_or(0))
}

/// Bumps the visit counter. KV has no atomic increment, so concurrent visits
/// can be undercounted. The orb is bad at math and has made peace with it.
pub async fn add_click<P: Platform>(platform: &P, alias_hash: &str, meta: &LinkMeta) -> Result<u64> {
    let clicks = get_clicks(platform, alias_hash).await?.saturating_add(1);
    let options = PutOptions {
        expiration: meta.exp,
        ..PutOptions::default()
    };
    platform
        .kv_put(&clicks_key(alias_hash), &clicks.to_string(), options)
        .await?;
    Ok(clicks)
}
