use crate::html::{PageOptions, render_page};
use crate::platform::Res;
use crate::responses::{ORB_LINK_NOT_FOUND_MESSAGE, Tone, orb_message};
use crate::store::{LinkKind, LinkRecord};
use crate::target::header_value;
use crate::text::{escape_html, format_date, format_date_time};

const WRONG_PASSWORD_MESSAGE: &str = "wrong password. the worm is disappointed in you.";
const LOCKED_OUT_MESSAGE: &str = "too many wrong guesses. the worm needs 15 minutes alone.";

/// Seconds a countdown link makes the visitor wait.
pub const COUNTDOWN_SECONDS: u32 = 5;

fn private_headers() -> Vec<(&'static str, String)> {
    vec![("Cache-Control", "no-store".to_owned())]
}

pub fn not_found_page() -> Res {
    let message = orb_message(ORB_LINK_NOT_FOUND_MESSAGE, Tone::Error);
    render_page(
        &format!(
            r#"<img src="/worm.webp" alt="" class="card-art card-art--sad" />
<h1 class="card-title">404</h1>
{message}
<p class="hint">it expired, self-destructed, got banished, or never existed.</p>
<a href="/" class="btn">summon a new one</a>"#
        ),
        PageOptions {
            status: Some(404),
            title: Some("lost"),
            headers: private_headers(),
        },
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnlockState {
    Ask,
    Wrong,
    Locked,
}

pub fn unlock_page(alias: &str, state: UnlockState, raw: bool) -> Res {
    let safe_alias = escape_html(alias);
    let query = if raw { "?raw" } else { "" };
    let (status, error) = match state {
        UnlockState::Ask => (200, String::new()),
        UnlockState::Wrong => (403, orb_message(WRONG_PASSWORD_MESSAGE, Tone::Error)),
        UnlockState::Locked => (429, orb_message(LOCKED_OUT_MESSAGE, Tone::Error)),
    };
    render_page(
        &format!(
            r#"<img src="/orb.webp" alt="" class="card-art card-art--orb" />
<h1 class="card-title">🔒 this one is guarded</h1>
<p class="hint">say the magic word.</p>
<form method="post" action="/{safe_alias}{query}" class="spell-form">
  <input type="password" name="password" placeholder="magic word" aria-label="Password" required autofocus />
  <button type="submit" class="btn">Unlock</button>
</form>
{error}"#
        ),
        PageOptions {
            status: Some(status),
            title: Some("guarded"),
            headers: private_headers(),
        },
    )
}

fn fact(label: &str, value: &str) -> String {
    format!("<dt>{label}</dt><dd>{value}</dd>")
}

/// A fact that only shows up when its spell is on.
fn fact_if(on: bool, label: &str, value: &str) -> String {
    if on {
        fact(label, value)
    } else {
        String::new()
    }
}

pub struct PreviewDetails<'a> {
    pub alias: &'a str,
    pub host: &'a str,
    pub record: &'a LinkRecord,
    pub clicks: u64,
}

fn destination_fact(record: &LinkRecord) -> String {
    if record.meta.kind == LinkKind::Paste {
        let size = record.url.len().div_ceil(1024);
        return fact("holds", &format!("📋 a paste, {size} KB"));
    }
    if record.meta.pw.is_some() {
        return fact("goes to", "🔒 a secret. the orb is sworn to silence.");
    }
    let destinations: Vec<String> = std::iter::once(&record.url)
        .chain(&record.more)
        .map(|url| {
            let safe = escape_html(url);
            format!(
                r#"<a href="{safe}" rel="noopener noreferrer nofollow" class="long-url">{safe}</a>"#
            )
        })
        .collect();
    let label = if record.more.is_empty() {
        "goes to"
    } else {
        "🥠 goes to one of"
    };
    fact(label, &destinations.join("<br />"))
}

pub fn preview_page(details: &PreviewDetails<'_>) -> Res {
    let PreviewDetails {
        alias,
        host,
        record,
        clicks,
    } = details;
    let meta = &record.meta;
    let safe_alias = escape_html(alias);
    let safe_host = escape_html(host);
    let facts = [
        destination_fact(record),
        meta.created
            .map_or_else(String::new, |created| fact("born", &format_date(created))),
        fact_if(
            meta.count || meta.max.is_some(),
            "visits",
            &clicks.to_string(),
        ),
        meta.max.map_or_else(String::new, |max| {
            let left = max.saturating_sub(*clicks);
            fact("self-destructs", &format!("after {left} more visits"))
        }),
        meta.exp.map_or_else(String::new, |exp| {
            fact("expires", &format_date_time(exp.saturating_mul(1000)))
        }),
        fact_if(meta.cursed, "curse", "☠️ might rickroll you"),
        fact_if(
            meta.tg.is_some(),
            "snitch",
            "📡 the owner gets visit alerts",
        ),
        fact_if(
            meta.wait,
            "countdown",
            &format!("⏳ {COUNTDOWN_SECONDS} seconds of suspense"),
        ),
    ]
    .concat();

    render_page(
        &format!(
            r#"<h1 class="card-title">🔮 the orb reveals</h1>
<p class="short-link">{safe_host}/{safe_alias}</p>
<dl class="facts">{facts}</dl>
<a href="/{safe_alias}" class="btn">take me there</a>"#
        ),
        PageOptions {
            title: Some(alias),
            headers: private_headers(),
            status: None,
        },
    )
}

/// Makes the visitor wait while the worm "charges the orb". Works without JS
/// through the refresh header; the page script animates the count.
pub fn countdown_page(target: &str) -> Res {
    let safe_target = escape_html(target);
    let mut headers = private_headers();
    headers.push((
        "Refresh",
        format!("{COUNTDOWN_SECONDS}; url={}", header_value(target)),
    ));
    render_page(
        &format!(
            r#"<div class="countdown" data-target="{safe_target}">
<img src="/orb.webp" alt="" class="card-art card-art--orb card-art--charging" />
<h1 class="card-title">⚡ charging the orb</h1>
<p class="countdown-number" data-countdown>{COUNTDOWN_SECONDS}</p>
<a href="{safe_target}" rel="noopener noreferrer nofollow" class="hint">skip the suspense</a>
</div>"#
        ),
        PageOptions {
            title: Some("charging"),
            headers,
            status: None,
        },
    )
}

/// Only what the page itself needs. Manage keys live in this origin's browser
/// storage, so nothing in a paste may ever run here.
const PASTE_PAGE_POLICY: &str = "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'; font-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'";

pub fn paste_page(alias: &str, record: &LinkRecord) -> Res {
    let safe_alias = escape_html(alias);
    let safe_text = escape_html(&record.url);
    // Opening the raw text counts as a visit, and could be the last one.
    let raw_link = if record.meta.max.is_none() {
        format!(r#"<a class="chip" href="/{safe_alias}?raw">raw</a>"#)
    } else {
        String::new()
    };
    let mut headers = private_headers();
    headers.push(("Content-Security-Policy", PASTE_PAGE_POLICY.to_owned()));
    headers.push(("X-Content-Type-Options", "nosniff".to_owned()));
    render_page(
        &format!(
            r##"<div class="paste">
<h1 class="card-title">📋 {safe_alias}</h1>
<div class="btn-row">
  <button type="button" class="chip copy-btn" data-copy-from="#paste-text">copy</button>
  {raw_link}
</div>
<pre class="paste-text" id="paste-text" tabindex="0">{safe_text}</pre>
</div>"##
        ),
        PageOptions {
            title: Some(alias),
            headers,
            status: None,
        },
    )
}

/// The text alone. It can't render or run as anything but text.
pub fn raw_text(text: &str) -> Res {
    Res::new(200)
        .with_header("Content-Type", "text/plain; charset=utf-8")
        .with_header("X-Content-Type-Options", "nosniff")
        .with_header("Content-Security-Policy", "default-src 'none'; sandbox")
        .with_header("Cache-Control", "no-store")
        .with_body(text)
}
