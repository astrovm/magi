//! Destination URLs.

use url::Url;

use crate::text::{js_trim, utf16_len};

/// Most characters accepted for a destination.
pub const MAX_URL_LENGTH: usize = 2048;

/// Lets people paste `example.com` without typing the scheme.
pub fn with_scheme(input: &str) -> String {
    let trimmed = js_trim(input);
    if trimmed.contains("://") {
        trimmed.to_owned()
    } else {
        format!("https://{trimmed}")
    }
}

fn parse_web_url(text: &str) -> Option<Url> {
    // URL parsers quietly drop line breaks, but links are stored one per line.
    if text.chars().any(char::is_control) {
        return None;
    }
    Url::parse(text)
        .ok()
        .filter(|url| matches!(url.scheme(), "http" | "https"))
}

/// A destination exactly as it was typed, known to be a valid web URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    text: String,
    host: String,
}

impl Target {
    pub fn from_input(input: &str) -> Option<Self> {
        let text = with_scheme(input);
        let url = parse_web_url(&text)?;
        let host = match (url.host_str(), url.port()) {
            (Some(host), Some(port)) => format!("{host}:{port}"),
            (Some(host), None) => host.to_owned(),
            (None, _) => return None,
        };
        Some(Self { text, host })
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn is_too_long(&self) -> bool {
        utf16_len(&self.text) > MAX_URL_LENGTH
    }

    pub fn into_string(self) -> String {
        self.text
    }
}

/// HTTP headers can't carry non-ASCII text, so those destinations go out
/// percent-encoded. Everything else stays exactly as it was typed.
pub fn header_value(target: &str) -> String {
    if target.is_ascii() {
        return target.to_owned();
    }
    parse_web_url(target).map_or_else(
        || target.chars().filter(char::is_ascii).collect(),
        |url| url.to_string(),
    )
}
