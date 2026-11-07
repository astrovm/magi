//! Reading what people typed into the link form.

use crate::platform::Form;
use crate::platform::is_our_address;
use crate::responses::ResponseKey;
use crate::store::LinkMeta;
use crate::target::Target;
use crate::text::utf16_len;

/// Allowed lifetimes in seconds: 1 hour, 1 day, 1 week, 30 days.
pub const EXPIRY_OPTIONS: [&str; 4] = ["3600", "86400", "604800", "2592000"];
pub const MAX_SELF_DESTRUCT: u64 = 1_000_000;
pub const MAX_PASSWORD_LENGTH: usize = 256;
/// Extra fortune cookie destinations on top of the main link.
pub const MAX_FORTUNES: usize = 9;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Spells {
    pub meta: LinkMeta,
    pub password: Option<String>,
}

/// A whole number in range, written any way JavaScript's `Number` reads it
/// (`3`, `3.0`, `3e0`).
fn parse_self_destruct(text: &str) -> Option<u64> {
    let number: f64 = crate::text::js_trim(text).parse().ok()?;
    let in_range = number >= 1.0 && number <= MAX_SELF_DESTRUCT as f64;
    (in_range && number.fract() == 0.0).then_some(number as u64)
}

pub fn parse_spells(form: &Form, now_ms: u64) -> Option<Spells> {
    let burn = form.text("burn").unwrap_or_default();
    let expires = form.text("expires").unwrap_or_default();
    let password = form.text("password").unwrap_or_default();
    let mut spells = Spells::default();

    if !burn.is_empty() {
        spells.meta.max = Some(parse_self_destruct(burn)?);
    }

    if !expires.is_empty() {
        if !EXPIRY_OPTIONS.contains(&expires) {
            return None;
        }
        spells.meta.exp = Some(now_ms / 1000 + expires.parse::<u64>().ok()?);
    }

    if utf16_len(password) > MAX_PASSWORD_LENGTH {
        return None;
    }
    if !password.is_empty() {
        spells.password = Some(password.to_owned());
    }
    spells.meta.cursed = form.is_on("cursed");
    spells.meta.count = form.is_on("count");
    spells.meta.wait = form.is_on("wait");

    Some(spells)
}

pub fn parse_target(input: &str, own_host: &str) -> Result<Target, ResponseKey> {
    let target = Target::from_input(input).ok_or(ResponseKey::InvalidUrl)?;
    if target.is_too_long() {
        return Err(ResponseKey::InvalidUrl);
    }
    if target.host() == own_host || is_our_address(target.host()) {
        return Err(ResponseKey::OwnTail);
    }
    Ok(target)
}

/// Reads the fortune cookie list: extra links, one per line.
pub fn parse_fortunes(text: &str, own_host: &str) -> Result<Vec<String>, ResponseKey> {
    let lines: Vec<&str> = text
        .split('\n')
        .map(crate::text::js_trim)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.len() > MAX_FORTUNES {
        return Err(ResponseKey::BadSpell);
    }
    lines
        .into_iter()
        .map(|line| parse_target(line, own_host).map(Target::into_string))
        .collect()
}
