use md5::{Digest, Md5};

use crate::text::{
    decode_uri_component, has_special_chars as text_has_special_chars, is_js_space, js_trim,
    to_hex, utf16_len,
};

/// Most characters accepted for an alias someone picks.
pub const MAX_ALIAS_LENGTH: usize = 13312;

/// Paths owned by the server, so they can't be used as aliases.
pub const RESERVED_ALIASES: [&str; 4] = ["orb", "manage", "paste", "telegram"];

/// Percent-decodes, leaving text with a broken escape as it was.
fn decode(raw: &str) -> String {
    decode_uri_component(raw).unwrap_or_else(|| raw.to_owned())
}

/// Collapses every run of whitespace into one space and trims the ends.
fn collapse_spaces(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_space = false;
    for ch in text.chars() {
        if is_js_space(ch) {
            if !in_space {
                out.push(' ');
            }
            in_space = true;
        } else {
            out.push(ch);
            in_space = false;
        }
    }
    js_trim(&out).to_owned()
}

/// An alias as it is stored and looked up: decoded, with single dashes
/// where it had whitespace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alias {
    value: String,
    /// Dotted paths are files, never aliases.
    pub has_dot: bool,
}

impl Alias {
    pub fn new(raw: &str) -> Self {
        let collapsed = collapse_spaces(&decode(raw));
        let has_dot = collapsed.contains('.');
        Self {
            value: collapsed.replace(' ', "-"),
            has_dot,
        }
    }

    pub fn as_str(&self) -> &str {
        &self.value
    }

    /// Removes the `+` from `/alias+` and reports if it was there.
    pub fn strip_peek(&mut self) -> bool {
        match self.value.strip_suffix('+') {
            Some(stripped) => {
                self.value = stripped.to_owned();
                true
            }
            None => false,
        }
    }

    /// True when this alias can't be claimed by someone.
    pub fn is_unclaimable(&self) -> bool {
        self.value.is_empty()
            || self.has_dot
            || utf16_len(&self.value) > MAX_ALIAS_LENGTH
            || text_has_special_chars(&self.value)
            || RESERVED_ALIASES.contains(&self.value.to_lowercase().as_str())
    }

    /// Key of the link in KV: aliases are case-insensitive.
    pub fn hash(&self) -> String {
        to_hex(&Md5::digest(self.value.to_lowercase().as_bytes()))
    }
}
