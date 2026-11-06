//! String helpers that behave like the JavaScript ones the stored data and
//! the site's behavior were built on.

pub fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        for nibble in [byte >> 4, byte & 15] {
            out.extend(char::from_digit(u32::from(nibble), 16));
        }
    }
    out
}

/// Same set as the JavaScript `\s` and `trim()`. Rust's own whitespace check
/// disagrees on U+0085 and U+FEFF.
pub fn is_js_space(ch: char) -> bool {
    matches!(
        ch,
        '\t' | '\n'
            | '\u{0B}'
            | '\u{0C}'
            | '\r'
            | ' '
            | '\u{A0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200A}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}'
            | '\u{FEFF}'
    )
}

pub fn js_trim(text: &str) -> &str {
    text.trim_matches(is_js_space)
}

/// JavaScript string length, which is what the size limits were written for.
pub fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// True when `encodeURIComponent` would change the text.
pub fn has_special_chars(text: &str) -> bool {
    !text
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || "-_.!~*'()".contains(ch))
}

/// Like `decodeURIComponent`: `None` for a broken escape or invalid UTF-8.
pub fn decode_uri_component(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut rest = bytes;
    while let Some((&byte, after)) = rest.split_first() {
        if byte != b'%' {
            out.push(byte);
            rest = after;
            continue;
        }
        let (&high, after) = after.split_first()?;
        let (&low, after) = after.split_first()?;
        let high = char::from(high).to_digit(16)?;
        let low = char::from(low).to_digit(16)?;
        out.push(u8::try_from(high * 16 + low).ok()?);
        rest = after;
    }
    String::from_utf8(out).ok()
}

pub fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

/// Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// `2026-10-03`
pub fn format_date(epoch_ms: u64) -> String {
    let days = i64::try_from(epoch_ms / 86_400_000).unwrap_or(0);
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

/// `2026-10-04 12:30 UTC`
pub fn format_date_time(epoch_ms: u64) -> String {
    let minutes = epoch_ms / 60_000 % 1440;
    format!(
        "{} {:02}:{:02} UTC",
        format_date(epoch_ms),
        minutes / 60,
        minutes % 60
    )
}
