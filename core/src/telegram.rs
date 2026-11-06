use serde_json::{Value, json};

use crate::platform::{Place, Platform, Result};

pub const TELEGRAM_API: &str = "https://api.telegram.org";

pub async fn send_message<P: Platform>(
    platform: &P,
    token: &str,
    chat_id: i64,
    text: &str,
) -> Result<()> {
    let body = json!({ "chat_id": chat_id, "text": text, "disable_web_page_preview": true });
    platform
        .http_post(
            &format!("{TELEGRAM_API}/bot{token}/sendMessage"),
            "application/json",
            body.to_string(),
        )
        .await
        .map(|_| ())
}

fn describe_browser(user_agent: &str) -> &'static str {
    let lower = user_agent.to_lowercase();
    if ["bot", "crawl", "spider", "preview"]
        .iter()
        .any(|word| lower.contains(word))
    {
        return "🤖 a bot";
    }
    [
        ("Firefox/", "Firefox"),
        ("Edg/", "Edge"),
        ("OPR/", "Opera"),
        ("Chrome/", "Chrome"),
        ("Safari/", "Safari"),
    ]
    .iter()
    .find(|(marker, _)| user_agent.contains(marker))
    .map_or("a mystery browser", |(_, name)| name)
}

fn describe_system(user_agent: &str) -> &'static str {
    [
        ("Android", "Android"),
        ("iPhone", "iOS"),
        ("iPad", "iOS"),
        ("Windows", "Windows"),
        ("Mac OS X", "macOS"),
        ("Linux", "Linux"),
    ]
    .iter()
    .find(|(marker, _)| user_agent.contains(marker))
    .map_or("a mystery device", |(_, name)| name)
}

/// Rough device summary for visit alerts. No IPs: the orb snitches, it doesn't dox.
pub fn describe_device(user_agent: &str) -> String {
    format!(
        "{} on {}",
        describe_browser(user_agent),
        describe_system(user_agent)
    )
}

pub fn describe_place(place: &Place) -> String {
    let parts: Vec<&str> = [&place.city, &place.country]
        .into_iter()
        .filter_map(|part| part.as_deref())
        .filter(|part| !part.is_empty())
        .collect();
    if parts.is_empty() {
        "somewhere in the void".to_owned()
    } else {
        parts.join(", ")
    }
}

pub fn visit_alert_text(short_link: &str, target: &str, user_agent: &str, place: &Place) -> String {
    format!(
        "🔮 someone opened {short_link}\n🌍 {}\n🧭 {}\n➡️ {target}",
        describe_place(place),
        describe_device(user_agent),
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelegramCommand {
    pub chat_id: i64,
    pub command: String,
    pub args: Vec<String>,
}

pub fn parse_command(update: &Value) -> Option<TelegramCommand> {
    let message = update.get("message")?;
    let text = message.get("text")?.as_str()?;
    let chat_id = message.get("chat")?.get("id")?.as_i64()?;
    let mut words = text.split(crate::text::is_js_space).filter(|word| !word.is_empty());
    let raw = words.next().unwrap_or_default();
    let without_bot = raw.split('@').next().unwrap_or_default();
    let command = without_bot.strip_prefix('/').unwrap_or(without_bot).to_lowercase();
    Some(TelegramCommand {
        chat_id,
        command,
        args: words.map(str::to_owned).collect(),
    })
}
