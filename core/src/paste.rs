//! Pastes: text kept under an alias, with the same spells as a link.

use crate::responses::ResponseKey;
use crate::target::Target;
use crate::text::js_trim;

/// Most bytes a paste can hold.
pub const MAX_PASTE_BYTES: usize = 512 * 1024;
/// A paste with no chosen lifetime lasts 30 days.
pub const DEFAULT_PASTE_SECONDS: u64 = 2_592_000;
/// What Safe Browsing accepts in one request.
const MAX_CHECKED_URLS: usize = 500;

/// Forms send every line break as `\r\n`, whatever was typed. Pastes keep `\n`,
/// so the raw text comes back the way it went in.
pub fn normalize_text(text: &str) -> String {
    text.replace("\r\n", "\n")
}

pub fn check_text(text: &str) -> Result<(), ResponseKey> {
    if js_trim(text).is_empty() {
        return Err(ResponseKey::EmptyPaste);
    }
    if text.len() > MAX_PASTE_BYTES {
        return Err(ResponseKey::PasteTooBig);
    }
    Ok(())
}

/// Characters that end a URL written inside normal text.
fn ends_url(ch: char) -> bool {
    ch.is_whitespace()
        || matches!(
            ch,
            '<' | '>' | '"' | '\'' | '`' | '(' | ')' | '[' | ']' | '{' | '}'
        )
}

/// The web addresses mentioned in a text, each one once.
pub fn find_urls(text: &str) -> Vec<&str> {
    let mut urls: Vec<&str> = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("http") {
        let (_, from_scheme) = rest.split_at(start);
        let end = from_scheme.find(ends_url).unwrap_or(from_scheme.len());
        let (candidate, after) = from_scheme.split_at(end);
        let url = candidate.trim_end_matches(['.', ',', ';', ':', '!', '?']);
        // Safe Browsing could refuse a whole request over one broken address,
        // and a refusal lets the paste through.
        let is_web = url.contains("://") && Target::from_input(url).is_some();
        if is_web && !urls.contains(&url) {
            urls.push(url);
        }
        if urls.len() == MAX_CHECKED_URLS {
            break;
        }
        rest = after;
    }
    urls
}
