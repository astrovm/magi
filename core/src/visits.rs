use crate::platform::{Config, Place, Platform, Res, Result};
use crate::store::{LinkRecord, add_click, delete_link};
use crate::target::header_value;
use crate::telegram::{send_message, visit_alert_text};
use crate::words::random_index;

pub const RICKROLL_URL: &str = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";

/// Chance that a cursed link sends the visitor to Rick instead.
pub const CURSE_CHANCE: f64 = 0.1;

pub fn pick_destination(record: &LinkRecord, mut random: impl FnMut() -> f64) -> String {
    if record.meta.cursed && random() < CURSE_CHANCE {
        return RICKROLL_URL.to_owned();
    }
    let index = random_index(random(), record.more.len() + 1);
    match index.checked_sub(1) {
        Some(extra) => record.more.get(extra).unwrap_or(&record.url).clone(),
        None => record.url.clone(),
    }
}

/// Links with these spells must hit the worker on every visit, so browsers
/// can't cache their redirects.
pub fn needs_every_visit(record: &LinkRecord) -> bool {
    let meta = &record.meta;
    meta.count
        || meta.max.is_some()
        || meta.cursed
        || meta.tg.is_some()
        || meta.pw.is_some()
        || meta.wait
        || !record.more.is_empty()
}

/// What a visit still has to do after the visitor got their answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Visit {
    pub alias_hash: String,
    pub record: LinkRecord,
    pub short_link: String,
    pub target: String,
    pub user_agent: String,
    pub place: Place,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Background {
    RecordVisit(Box<Visit>),
    DeleteLink(String),
}

async fn count_visit<P: Platform>(platform: &P, visit: &Visit) -> Result<()> {
    let meta = &visit.record.meta;
    if !meta.count && meta.max.is_none() {
        return Ok(());
    }
    let clicks = add_click(platform, &visit.alias_hash, meta).await?;
    if meta.max.is_some_and(|max| clicks >= max) {
        delete_link(platform, &visit.alias_hash).await?;
    }
    Ok(())
}

async fn snitch_visit<P: Platform>(platform: &P, config: &Config, visit: &Visit) -> Result<()> {
    let (Some(chat_id), Some(token)) = (visit.record.meta.tg, &config.telegram_bot_token) else {
        return Ok(());
    };
    let text = visit_alert_text(
        &visit.short_link,
        &visit.target,
        &visit.user_agent,
        &visit.place,
    );
    send_message(platform, token, chat_id, &text).await
}

/// Runs after the response went out. One failing doesn't stop the other.
pub async fn run_background<P: Platform>(
    platform: &P,
    config: &Config,
    job: &Background,
) -> Result<()> {
    match job {
        Background::RecordVisit(visit) => {
            let counted = count_visit(platform, visit).await;
            let snitched = snitch_visit(platform, config, visit).await;
            counted.and(snitched)
        }
        Background::DeleteLink(alias_hash) => delete_link(platform, alias_hash).await,
    }
}

pub fn redirect_to(url: &str, cacheable: bool) -> Res {
    Res::new(if cacheable { 301 } else { 302 })
        .with_header("Location", header_value(url))
        .with_header(
            "Cache-Control",
            if cacheable {
                "public, max-age=300"
            } else {
                "no-store"
            },
        )
}
