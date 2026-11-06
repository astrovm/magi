use crate::html::{HTML_CONTENT_TYPE, PageOptions, render_page};
use crate::platform::{Req, Res};
use crate::qr::render_svg;
use crate::store::LinkKind;
use crate::text::escape_html;

pub const ORB_LINK_NOT_FOUND_MESSAGE: &str = "the orb didn't found this link.";
pub const ORB_WRONG_KEY_MESSAGE: &str = "the orb doesn't recognize that manage key.";
const WORM_LINK_CREATED_MESSAGE: &str = "the worm summoned your link";
const WORM_PASTE_CREATED_MESSAGE: &str = "the worm summoned your paste";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseKey {
    InvalidRequest,
    InvalidAlias,
    InvalidUrl,
    AliasLocked,
    OwnTail,
    WrongKey,
    BadSpell,
    NoAliasLeft,
    LinkNotFound,
    DangerUrl,
    TooMany,
    NotHuman,
    LinkTransmuted,
    LinkBanished,
    ServerError,
    EmptyPaste,
    PasteTooBig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Error,
    Success,
}

impl ResponseKey {
    pub fn details(self) -> (&'static str, u16, Tone) {
        match self {
            Self::EmptyPaste => (
                "the orb can't summon nothing. paste some text.",
                400,
                Tone::Error,
            ),
            Self::PasteTooBig => (
                "that paste is too heavy for the orb. 512 KB at most.",
                413,
                Tone::Error,
            ),
            Self::InvalidRequest => ("the orb rejected your request.", 400, Tone::Error),
            Self::InvalidAlias => ("the orb rejected your alias.", 400, Tone::Error),
            Self::InvalidUrl => ("the orb rejected your invalid url.", 400, Tone::Error),
            Self::AliasLocked => (
                "the orb rejected your access to rewrite this link.",
                409,
                Tone::Error,
            ),
            Self::OwnTail => ("the orb refuses to eat its own tail.", 400, Tone::Error),
            Self::WrongKey => (ORB_WRONG_KEY_MESSAGE, 403, Tone::Error),
            Self::BadSpell => (
                "the orb rejected your weird spell settings.",
                400,
                Tone::Error,
            ),
            Self::NoAliasLeft => (
                "the worm ran out of ideas. try a custom alias.",
                503,
                Tone::Error,
            ),
            Self::LinkNotFound => (ORB_LINK_NOT_FOUND_MESSAGE, 404, Tone::Error),
            Self::DangerUrl => (
                "the orb smells something fishy. that link looks like phishing or malware.",
                400,
                Tone::Error,
            ),
            Self::TooMany => ("the worm is exhausted. try again later.", 429, Tone::Error),
            Self::NotHuman => (
                "the orb couldn't tell if you're human. try again.",
                403,
                Tone::Error,
            ),
            Self::LinkTransmuted => ("the worm transmuted your link.", 200, Tone::Success),
            Self::LinkBanished => (
                "the worm banished your link to the shadow realm.",
                200,
                Tone::Success,
            ),
            Self::ServerError => (
                "the orb tripped over its own hat. try again.",
                500,
                Tone::Error,
            ),
        }
    }
}

pub fn orb_message(message: &str, tone: Tone) -> String {
    let class = match tone {
        Tone::Error => "error",
        Tone::Success => "success",
    };
    format!(r#"<p class="orb-msg orb-msg--{class}">{message}</p>"#)
}

/// The homepage script asks for bare HTML fragments. Plain form posts (no JS)
/// get a full page instead.
pub fn reply(req: &Req, body: &str, status: u16) -> Res {
    if req.wants_fragment {
        return Res::new(status)
            .with_header("Content-Type", HTML_CONTENT_TYPE)
            .with_body(body);
    }
    render_page(
        body,
        PageOptions {
            status: Some(status),
            ..PageOptions::default()
        },
    )
}

pub fn get_response(req: &Req, key: ResponseKey) -> Res {
    let (message, status, tone) = key.details();
    reply(req, &orb_message(message, tone), status)
}

fn copy_row(content: &str, value: &str) -> String {
    format!(
        r#"<div class="copy-row">{content}<button type="button" class="copy-btn" data-copy="{}">copy</button></div>"#,
        escape_html(value)
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedLink<'a> {
    pub short_url: &'a str,
    pub manage_key: &'a str,
    pub alias: &'a str,
    pub telegram_bot: Option<&'a str>,
    pub kind: LinkKind,
}

fn link_created_html(link: &CreatedLink<'_>) -> String {
    let CreatedLink {
        short_url,
        manage_key,
        alias,
        telegram_bot,
        kind,
    } = link;
    let label = short_url
        .strip_prefix("https://")
        .or_else(|| short_url.strip_prefix("http://"))
        .unwrap_or(short_url);
    let short_label = escape_html(label);
    let safe_url = escape_html(short_url);
    let safe_alias = escape_html(alias);
    let safe_key = escape_html(manage_key);
    let alert_command = escape_html(&format!("/alert {alias} {manage_key}"));
    let telegram = telegram_bot.map_or_else(String::new, |bot| {
        let safe_bot = escape_html(bot);
        format!(
            r#"<p class="hint">📡 visit alerts: send <code>{alert_command}</code> to <a href="https://t.me/{safe_bot}">@{safe_bot}</a></p>"#
        )
    });
    let created = orb_message(
        match kind {
            LinkKind::Link => WORM_LINK_CREATED_MESSAGE,
            LinkKind::Paste => WORM_PASTE_CREATED_MESSAGE,
        },
        Tone::Success,
    );
    let short_row = copy_row(
        &format!(r#"<a class="short-link" href="{safe_url}">{short_label}</a>"#),
        short_url,
    );
    let key_row = copy_row(
        &format!(r#"<code class="secret">{safe_key}</code>"#),
        manage_key,
    );
    let qr = render_svg(short_url);

    // The page script reads these to save the link under "my links".
    format!(
        r#"<div class="summoned" data-alias="{safe_alias}" data-short="{safe_url}" data-key="{safe_key}">
  {created}
  {short_row}
  <p class="hint">🔍 peek first: <a href="{safe_url}+">{short_label}+</a></p>
  <details class="qr">
    <summary>📱 QR code</summary>
    <div class="qr-code">{qr}</div>
  </details>
  <div class="scroll">
    <p class="hint">🔑 manage key. shown once, the worm has the memory of a worm.</p>
    {key_row}
  </div>
  {telegram}
</div>"#
    )
}

pub fn link_created(req: &Req, link: &CreatedLink<'_>) -> Res {
    reply(req, &link_created_html(link), 201)
}
