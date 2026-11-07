//! The whole site: what each path answers.

use serde_json::{Value, json};

use crate::alias::Alias;
use crate::guards::{
    CREATE_LIMIT, UNLOCK_LIMIT, count_attempt, is_dangerous, is_limited, limit_key,
    passes_turnstile,
};
use crate::live::{
    ROOM_INIT, ROOM_LOCK, ROOM_RESTORE, ROOM_SNAPSHOTS, ROOM_TEXT, ROOM_UNLOCK, ROOM_WIPE, Role,
    RoomInit,
};
use crate::pages::{
    LivePage, PreviewDetails, UnlockState, countdown_page, live_page, not_found_page, paste_page,
    preview_page, raw_text, unlock_page,
};
use crate::paste::{DEFAULT_PASTE_SECONDS, MAX_PASTE_BYTES, check_text, find_urls, normalize_text};
use crate::platform::{
    Body, Config, Form, Method, Outcome, Platform, PlatformError, Req, Res, Result,
};
use crate::responses::{
    CreatedLink, ORB_LINK_NOT_FOUND_MESSAGE, ORB_WRONG_KEY_MESSAGE, ResponseKey, get_response,
    link_created,
};
use crate::secrets::{hash_password, random_token, sha256_hex, timing_safe_equal, verify_password};
use crate::spells::{parse_fortunes, parse_spells, parse_target};
use crate::store::{LinkKind, LinkMeta, LinkRecord, delete_link, get_clicks, get_link, put_link};
use crate::telegram::{TelegramCommand, parse_command, send_message};
use crate::text::js_trim;
use crate::visits::{Background, Visit, needs_every_visit, pick_destination, redirect_to};
use crate::words::summon_wizard_words;

const RANDOM_ALIAS_ATTEMPTS: u32 = 5;
const MANAGE_KEY_BYTES: usize = 16;

/// Stands in for the destination in visit alerts: the text stays out of Telegram.
const PASTE_VISIT_LABEL: &str = "📋 the paste";
const LIVE_SOCKET_PREFIX: &str = "paste/ws/";
const LIVE_VISIT_LABEL: &str = "✍️ the live paste";
/// How unguessable a view-only link is.
const VIEW_ALIAS_BYTES: usize = 16;

const TELEGRAM_HELP: &str = "hi, i'm the orb's snitch 🔮\nsend /alert <alias> <manage key> and i'll ping you when someone opens that link.\nsend it again to make me stop.";

fn json_reply(body: &Value) -> Res {
    Res::new(200)
        .with_header("Content-Type", "application/json; charset=utf-8")
        .with_header("Cache-Control", "no-store")
        .with_body(body.to_string())
}

struct Claimed {
    alias: String,
    alias_hash: String,
}

async fn claim_custom_alias<P: Platform>(
    platform: &P,
    raw: &str,
) -> Result<std::result::Result<Claimed, ResponseKey>> {
    let alias = Alias::new(raw);
    if alias.is_unclaimable() {
        return Ok(Err(ResponseKey::InvalidAlias));
    }
    let alias_hash = alias.hash();
    if get_link(platform, &alias_hash).await?.is_some() {
        return Ok(Err(ResponseKey::AliasLocked));
    }
    Ok(Ok(Claimed {
        alias: alias.as_str().to_owned(),
        alias_hash,
    }))
}

async fn claim_random_alias<P: Platform>(
    platform: &P,
) -> Result<std::result::Result<Claimed, ResponseKey>> {
    for _ in 0..RANDOM_ALIAS_ATTEMPTS {
        let words = summon_wizard_words(|| platform.random());
        if let Ok(claimed) = claim_custom_alias(platform, &words).await? {
            return Ok(Ok(claimed));
        }
    }
    Ok(Err(ResponseKey::NoAliasLeft))
}

/// `GET /orb?alias=x` tells the form if an alias is free while you type.
/// `GET /orb` hands the page its public settings.
async fn orb_get<P: Platform>(platform: &P, config: &Config, req: &Req) -> Result<Res> {
    let Some(alias) = req.query("alias") else {
        return Ok(json_reply(
            &json!({ "turnstileSiteKey": config.turnstile_site_key }),
        ));
    };
    if js_trim(&alias).is_empty() {
        return Ok(json_reply(&json!({ "status": "invalid" })));
    }
    Ok(json_reply(
        &match claim_custom_alias(platform, &alias).await? {
            Ok(claimed) => json!({ "status": "free", "alias": claimed.alias }),
            Err(ResponseKey::AliasLocked) => json!({ "status": "taken" }),
            Err(_) => json!({ "status": "invalid" }),
        },
    ))
}

/// What a visitor has to pass before creating anything.
async fn check_visitor<P: Platform>(
    platform: &P,
    config: &Config,
    req: &Req,
    form: &Form,
) -> Result<std::result::Result<String, ResponseKey>> {
    let create_key = limit_key("create", req);
    if is_limited(platform, &create_key, CREATE_LIMIT).await? {
        return Ok(Err(ResponseKey::TooMany));
    }
    let token = form.text("cf-turnstile-response");
    if !passes_turnstile(platform, config.turnstile_secret_key.as_deref(), token, req).await {
        return Ok(Err(ResponseKey::NotHuman));
    }
    Ok(Ok(create_key))
}

async fn claim_from_form<P: Platform>(
    platform: &P,
    form: &Form,
) -> Result<std::result::Result<Claimed, ResponseKey>> {
    let alias_field = form.text("alias").unwrap_or_default();
    if js_trim(alias_field).is_empty() {
        claim_random_alias(platform).await
    } else {
        claim_custom_alias(platform, alias_field).await
    }
}

async fn orb_post<P: Platform>(platform: &P, config: &Config, req: &Req) -> Result<Res> {
    let Some(form) = req.form() else {
        return Ok(get_response(req, ResponseKey::InvalidRequest));
    };
    let Some(url_field) = form.text("url") else {
        return Ok(get_response(req, ResponseKey::InvalidRequest));
    };
    if form.is_file("alias") || form.is_file("fortune") {
        return Ok(get_response(req, ResponseKey::InvalidRequest));
    }

    let create_key = match check_visitor(platform, config, req, form).await? {
        Ok(create_key) => create_key,
        Err(key) => return Ok(get_response(req, key)),
    };

    let now = platform.now_ms();
    let Some(spells) = parse_spells(form, now) else {
        return Ok(get_response(req, ResponseKey::BadSpell));
    };

    let host = req.host();
    let target = match parse_target(url_field, &host) {
        Ok(target) => target,
        Err(key) => return Ok(get_response(req, key)),
    };
    let fortunes = match parse_fortunes(form.text("fortune").unwrap_or_default(), &host) {
        Ok(fortunes) => fortunes,
        Err(key) => return Ok(get_response(req, key)),
    };
    let mut destinations = vec![target.as_str()];
    destinations.extend(fortunes.iter().map(String::as_str));
    if is_dangerous(
        platform,
        config.safe_browsing_api_key.as_deref(),
        &destinations,
    )
    .await
    {
        return Ok(get_response(req, ResponseKey::DangerUrl));
    }

    let claimed = match claim_from_form(platform, form).await? {
        Ok(claimed) => claimed,
        Err(key) => return Ok(get_response(req, key)),
    };

    let manage_key = random_token(platform, MANAGE_KEY_BYTES)?;
    let pw = match &spells.password {
        Some(password) => Some(hash_password(platform, password).await?),
        None => None,
    };
    count_attempt(platform, &create_key, CREATE_LIMIT).await?;
    put_link(
        platform,
        &claimed.alias_hash,
        &LinkRecord {
            url: target.as_str().to_owned(),
            more: fortunes,
            meta: LinkMeta {
                created: Some(now),
                key: Some(sha256_hex(&manage_key)),
                pw,
                ..spells.meta
            },
        },
    )
    .await?;

    Ok(link_created(
        req,
        &CreatedLink {
            short_url: &format!("{}/{}", req.short_origin(), claimed.alias),
            manage_key: &manage_key,
            alias: &claimed.alias,
            telegram_bot: config.telegram_bot_username.as_deref(),
            kind: LinkKind::Link,
            view_url: None,
        },
    ))
}

async fn paste_post<P: Platform>(platform: &P, config: &Config, req: &Req) -> Result<Res> {
    let Some(form) = req.form() else {
        return Ok(get_response(req, ResponseKey::InvalidRequest));
    };
    let Some(text) = form.text("text") else {
        return Ok(get_response(req, ResponseKey::InvalidRequest));
    };
    if form.is_file("alias") {
        return Ok(get_response(req, ResponseKey::InvalidRequest));
    }

    let create_key = match check_visitor(platform, config, req, form).await? {
        Ok(create_key) => create_key,
        Err(key) => return Ok(get_response(req, key)),
    };

    if form.is_on("live") {
        return live_post(platform, config, req, form, text, &create_key).await;
    }

    let now = platform.now_ms();
    let Some(spells) = parse_spells(form, now) else {
        return Ok(get_response(req, ResponseKey::BadSpell));
    };
    let text = normalize_text(text);
    if let Err(key) = check_text(&text) {
        return Ok(get_response(req, key));
    }
    let urls = find_urls(&text);
    if is_dangerous(platform, config.safe_browsing_api_key.as_deref(), &urls).await {
        return Ok(get_response(req, ResponseKey::DangerUrl));
    }
    let claimed = match claim_from_form(platform, form).await? {
        Ok(claimed) => claimed,
        Err(key) => return Ok(get_response(req, key)),
    };

    let manage_key = random_token(platform, MANAGE_KEY_BYTES)?;
    let pw = match &spells.password {
        Some(password) => Some(hash_password(platform, password).await?),
        None => None,
    };
    // Leaving the choice out means the default. Choosing "never" sends it empty.
    let exp = if form.get("expires").is_none() {
        Some(now / 1000 + DEFAULT_PASTE_SECONDS)
    } else {
        spells.meta.exp
    };
    count_attempt(platform, &create_key, CREATE_LIMIT).await?;
    put_link(
        platform,
        &claimed.alias_hash,
        &LinkRecord {
            url: text,
            more: Vec::new(),
            meta: LinkMeta {
                kind: LinkKind::Paste,
                created: Some(now),
                key: Some(sha256_hex(&manage_key)),
                pw,
                max: spells.meta.max,
                count: spells.meta.count,
                exp,
                ..LinkMeta::default()
            },
        },
    )
    .await?;

    Ok(link_created(
        req,
        &CreatedLink {
            short_url: &format!("{}/{}", req.short_origin(), claimed.alias),
            manage_key: &manage_key,
            alias: &claimed.alias,
            telegram_bot: config.telegram_bot_username.as_deref(),
            kind: LinkKind::Paste,
            view_url: None,
        },
    ))
}

/// A live paste is two aliases and a room: one alias edits, the other is a
/// random one that can only watch and doesn't reveal the first.
async fn live_post<P: Platform>(
    platform: &P,
    config: &Config,
    req: &Req,
    form: &Form,
    text: &str,
    create_key: &str,
) -> Result<Res> {
    let text = normalize_text(text);
    if text.len() > MAX_PASTE_BYTES {
        return Ok(get_response(req, ResponseKey::PasteTooBig));
    }
    let urls = find_urls(&text);
    if is_dangerous(platform, config.safe_browsing_api_key.as_deref(), &urls).await {
        return Ok(get_response(req, ResponseKey::DangerUrl));
    }
    let claimed = match claim_from_form(platform, form).await? {
        Ok(claimed) => claimed,
        Err(key) => return Ok(get_response(req, key)),
    };

    let manage_key = random_token(platform, MANAGE_KEY_BYTES)?;
    let view_alias = random_token(platform, VIEW_ALIAS_BYTES)?;
    let view_hash = Alias::new(&view_alias).hash();
    let now = platform.now_ms();
    count_attempt(platform, create_key, CREATE_LIMIT).await?;

    // The room comes first. If the rest fails, it cleans up after itself.
    let init = RoomInit {
        text,
        link_hash: claimed.alias_hash.clone(),
        view_hash: view_hash.clone(),
    };
    // Strings always serialize. An empty body would only get the room to refuse.
    let body = serde_json::to_string(&init).unwrap_or_default();
    let reply = platform
        .live_call(&claimed.alias_hash, ROOM_INIT, body)
        .await?;
    if reply.status != 200 {
        return Err(PlatformError(format!("room init: {}", reply.status)));
    }
    let base = LinkMeta {
        created: Some(now),
        ..LinkMeta::default()
    };
    put_link(
        platform,
        &view_hash,
        &LinkRecord {
            url: claimed.alias_hash.clone(),
            more: Vec::new(),
            meta: LinkMeta {
                kind: LinkKind::View,
                ..base.clone()
            },
        },
    )
    .await?;
    put_link(
        platform,
        &claimed.alias_hash,
        &LinkRecord {
            url: view_alias.clone(),
            more: Vec::new(),
            meta: LinkMeta {
                kind: LinkKind::Live,
                key: Some(sha256_hex(&manage_key)),
                ..base
            },
        },
    )
    .await?;

    let origin = req.short_origin();
    Ok(link_created(
        req,
        &CreatedLink {
            short_url: &format!("{origin}/{}", claimed.alias),
            manage_key: &manage_key,
            alias: &claimed.alias,
            telegram_bot: config.telegram_bot_username.as_deref(),
            kind: LinkKind::Live,
            view_url: Some(&format!("{origin}/{view_alias}")),
        },
    ))
}

/// What the owner of a live paste can do, with the manage key already checked.
async fn manage_live<P: Platform>(
    platform: &P,
    req: &Req,
    alias_hash: &str,
    record: &LinkRecord,
    action: &str,
) -> Result<Res> {
    match action {
        "banish" => {
            platform
                .live_call(alias_hash, ROOM_WIPE, String::new())
                .await?;
            delete_link(platform, &Alias::new(&record.url).hash()).await?;
            delete_link(platform, alias_hash).await?;
            Ok(get_response(req, ResponseKey::PasteBanished))
        }
        "lock" => {
            platform
                .live_call(alias_hash, ROOM_LOCK, String::new())
                .await?;
            Ok(get_response(req, ResponseKey::LiveLocked))
        }
        "unlock" => {
            platform
                .live_call(alias_hash, ROOM_UNLOCK, String::new())
                .await?;
            Ok(get_response(req, ResponseKey::LiveUnlocked))
        }
        "snapshots" => {
            let reply = platform
                .live_call(alias_hash, ROOM_SNAPSHOTS, String::new())
                .await?;
            Ok(Res::new(reply.status)
                .with_header("Content-Type", "application/json; charset=utf-8")
                .with_header("Cache-Control", "no-store")
                .with_body(reply.body))
        }
        "restore" => {
            let id = req
                .form()
                .and_then(|form| form.text("snapshot"))
                .and_then(|snapshot| snapshot.parse::<u32>().ok());
            let Some(id) = id else {
                return Ok(get_response(req, ResponseKey::InvalidRequest));
            };
            let path = format!("{ROOM_RESTORE}{id}");
            let reply = platform.live_call(alias_hash, &path, String::new()).await?;
            Ok(get_response(
                req,
                if reply.status == 200 {
                    ResponseKey::LiveRestored
                } else {
                    ResponseKey::InvalidRequest
                },
            ))
        }
        _ => Ok(get_response(req, ResponseKey::LiveNoTransmute)),
    }
}

async fn transmute_paste<P: Platform>(
    platform: &P,
    config: &Config,
    req: &Req,
    alias_hash: &str,
    record: LinkRecord,
) -> Result<Res> {
    let field = req.form().and_then(|form| form.text("text"));
    let text = normalize_text(field.unwrap_or_default());
    if let Err(key) = check_text(&text) {
        return Ok(get_response(req, key));
    }
    let urls = find_urls(&text);
    if is_dangerous(platform, config.safe_browsing_api_key.as_deref(), &urls).await {
        return Ok(get_response(req, ResponseKey::DangerUrl));
    }
    put_link(
        platform,
        alias_hash,
        &LinkRecord {
            url: text,
            ..record
        },
    )
    .await?;
    Ok(get_response(req, ResponseKey::PasteTransmuted))
}

fn holds_key(record: &LinkRecord, key: &str) -> bool {
    record
        .meta
        .key
        .as_deref()
        .is_some_and(|stored| timing_safe_equal(&sha256_hex(key), stored))
}

async fn manage_post<P: Platform>(platform: &P, config: &Config, req: &Req) -> Result<Res> {
    let Some(form) = req.form() else {
        return Ok(get_response(req, ResponseKey::InvalidRequest));
    };
    let alias_field = form.text("alias").filter(|field| !field.is_empty());
    let key_field = form.text("key").filter(|field| !field.is_empty());
    let action = form.text("action");
    let (
        Some(alias_field),
        Some(key_field),
        Some(action @ ("transmute" | "banish" | "lock" | "unlock" | "snapshots" | "restore")),
    ) = (alias_field, key_field, action)
    else {
        return Ok(get_response(req, ResponseKey::InvalidRequest));
    };

    let alias_hash = Alias::new(alias_field).hash();
    let Some(record) = get_link(platform, &alias_hash).await? else {
        return Ok(get_response(req, ResponseKey::LinkNotFound));
    };
    if !holds_key(&record, js_trim(key_field)) {
        return Ok(get_response(req, ResponseKey::WrongKey));
    }

    if record.meta.kind == LinkKind::Live {
        return manage_live(platform, req, &alias_hash, &record, action).await;
    }
    if !matches!(action, "transmute" | "banish") {
        return Ok(get_response(req, ResponseKey::InvalidRequest));
    }
    if action == "banish" {
        delete_link(platform, &alias_hash).await?;
        let banished = if record.meta.kind == LinkKind::Paste {
            ResponseKey::PasteBanished
        } else {
            ResponseKey::LinkBanished
        };
        return Ok(get_response(req, banished));
    }
    if record.meta.kind == LinkKind::Paste {
        return transmute_paste(platform, config, req, &alias_hash, record).await;
    }

    let target = match parse_target(form.text("url").unwrap_or_default(), &req.host()) {
        Ok(target) => target,
        Err(key) => return Ok(get_response(req, key)),
    };
    if is_dangerous(
        platform,
        config.safe_browsing_api_key.as_deref(),
        &[target.as_str()],
    )
    .await
    {
        return Ok(get_response(req, ResponseKey::DangerUrl));
    }

    put_link(
        platform,
        &alias_hash,
        &LinkRecord {
            url: target.into_string(),
            ..record
        },
    )
    .await?;
    Ok(get_response(req, ResponseKey::LinkTransmuted))
}

async fn toggle_alerts<P: Platform>(platform: &P, command: &TelegramCommand) -> Result<String> {
    let (Some(alias_arg), Some(key)) = (command.args.first(), command.args.get(1)) else {
        return Ok("usage: /alert <alias> <manage key>".to_owned());
    };
    let alias = Alias::new(alias_arg);
    let alias_hash = alias.hash();
    let Some(mut record) = get_link(platform, &alias_hash).await? else {
        return Ok(ORB_LINK_NOT_FOUND_MESSAGE.to_owned());
    };
    if !holds_key(&record, key) {
        return Ok(ORB_WRONG_KEY_MESSAGE.to_owned());
    }

    let enable = record.meta.tg != Some(command.chat_id);
    record.meta.tg = enable.then_some(command.chat_id);
    put_link(platform, &alias_hash, &record).await?;
    let name = alias.as_str();
    Ok(if enable {
        format!("👀 watching {name}. i'll snitch on every visit. give me a few minutes to wake up.")
    } else {
        format!("😴 stopped watching {name}.")
    })
}

async fn telegram_post<P: Platform>(platform: &P, config: &Config, req: &Req) -> Result<Res> {
    let (Some(token), Some(secret)) = (&config.telegram_bot_token, &config.telegram_webhook_secret)
    else {
        return Ok(Res::new(404));
    };
    if !timing_safe_equal(&req.telegram_secret, secret) {
        return Ok(Res::new(404));
    }

    let Body::Text(text) = &req.body else {
        return Ok(Res::new(200));
    };
    let update = serde_json::from_str::<Value>(text).unwrap_or(Value::Null);
    let Some(command) = parse_command(&update) else {
        return Ok(Res::new(200));
    };
    let answer = match command.command.as_str() {
        "alert" => toggle_alerts(platform, &command).await?,
        "getid" => format!("chat id: {}", command.chat_id),
        _ => TELEGRAM_HELP.to_owned(),
    };
    // The change is already made. Answering 500 here would make Telegram send
    // the update again, and a toggle sent twice undoes itself.
    let _ = send_message(platform, token, command.chat_id, &answer).await;
    Ok(Res::new(200))
}

struct Lookup {
    alias: String,
    alias_hash: String,
    peek: bool,
    record: Option<LinkRecord>,
}

/// `None` for a dotted path: those are files, never aliases.
async fn look_up<P: Platform>(platform: &P, raw: &str) -> Result<Option<Lookup>> {
    let mut alias = Alias::new(raw);
    if alias.has_dot {
        return Ok(None);
    }
    let peek = alias.strip_peek();
    let alias_hash = alias.hash();
    let record = get_link(platform, &alias_hash).await?;
    Ok(Some(Lookup {
        alias: alias.as_str().to_owned(),
        alias_hash,
        peek,
        record,
    }))
}

/// The live paste an alias belongs to, and what visitors can do with it.
fn live_room(lookup: &Lookup, record: &LinkRecord) -> Option<(String, Role)> {
    match record.meta.kind {
        LinkKind::Live => Some((lookup.alias_hash.clone(), Role::Edit)),
        LinkKind::View => Some((record.url.clone(), Role::View)),
        LinkKind::Link | LinkKind::Paste => None,
    }
}

async fn live_response<P: Platform>(
    platform: &P,
    req: &Req,
    lookup: &Lookup,
    record: &LinkRecord,
    room: &str,
    role: Role,
) -> Result<Res> {
    if req.query("raw").is_some() {
        let reply = platform.live_call(room, ROOM_TEXT, String::new()).await?;
        if reply.status != 200 {
            return Ok(not_found_page());
        }
        return Ok(raw_text(&reply.body));
    }
    let origin = req.short_origin();
    let host = req.host();
    let edit_url = format!("{origin}/{}", lookup.alias);
    let view_url = format!("{origin}/{}", record.url);
    Ok(live_page(&LivePage {
        alias: &lookup.alias,
        host: &host,
        role,
        edit_url: &edit_url,
        view_url: (role == Role::Edit).then_some(view_url.as_str()),
    }))
}

async fn visit<P: Platform>(
    platform: &P,
    req: &Req,
    lookup: &Lookup,
    record: &LinkRecord,
) -> Result<Outcome> {
    let meta = &record.meta;

    // The link lookup is cached, so a self-destructed link can linger for a
    // few minutes. The fresh counter catches it.
    if let Some(max) = meta.max
        && get_clicks(platform, &lookup.alias_hash).await? >= max
    {
        return Ok(Outcome::Reply {
            response: not_found_page(),
            background: Some(Background::DeleteLink(lookup.alias_hash.clone())),
        });
    }

    let tracked = needs_every_visit(record);
    let (target, response) = match meta.kind {
        LinkKind::Paste if req.query("raw").is_some() => {
            (PASTE_VISIT_LABEL.to_owned(), raw_text(&record.url))
        }
        LinkKind::Paste => (
            PASTE_VISIT_LABEL.to_owned(),
            paste_page(&lookup.alias, record),
        ),
        LinkKind::Live => (
            LIVE_VISIT_LABEL.to_owned(),
            live_response(
                platform,
                req,
                lookup,
                record,
                &lookup.alias_hash,
                Role::Edit,
            )
            .await?,
        ),
        LinkKind::View => (
            LIVE_VISIT_LABEL.to_owned(),
            live_response(platform, req, lookup, record, &record.url, Role::View).await?,
        ),
        LinkKind::Link => {
            let target = pick_destination(record, || platform.random());
            let response = if meta.wait && req.method != Method::Head {
                countdown_page(&target)
            } else {
                redirect_to(&target, !tracked)
            };
            (target, response)
        }
    };
    // HEAD is what link checkers send: answer like GET, but it isn't a visit.
    if req.method == Method::Head {
        return Ok(response.into());
    }
    let background = tracked.then(|| {
        Background::RecordVisit(Box::new(Visit {
            alias_hash: lookup.alias_hash.clone(),
            record: record.clone(),
            short_link: format!("{}/{}", req.short_host(), lookup.alias),
            target,
            user_agent: req.user_agent.clone(),
            place: req.place.clone(),
        }))
    });
    Ok(Outcome::Reply {
        response,
        background,
    })
}

async fn alias_get<P: Platform>(platform: &P, req: &Req, raw: &str) -> Result<Outcome> {
    let Some(lookup) = look_up(platform, raw).await? else {
        return Ok(Outcome::Asset);
    };
    let Some(record) = &lookup.record else {
        return Ok(not_found_page().into());
    };
    if lookup.peek {
        let clicks = get_clicks(platform, &lookup.alias_hash).await?;
        let host = req.short_host();
        return Ok(preview_page(&PreviewDetails {
            alias: &lookup.alias,
            host: &host,
            record,
            clicks,
        })
        .into());
    }
    // The owner's key is saved at home, whatever address the link says. Before
    // the password, so nobody has to type it twice.
    if matches!(record.meta.kind, LinkKind::Live | LinkKind::View)
        && req.query("raw").is_none()
        && let Some(home) = req.url_at_home()
    {
        return Ok(redirect_to(&home, false).into());
    }
    if record.meta.pw.is_some() {
        return Ok(unlock_page(&lookup.alias, UnlockState::Ask, req.query("raw").is_some()).into());
    }
    visit(platform, req, &lookup, record).await
}

async fn alias_post<P: Platform>(platform: &P, req: &Req, raw: &str) -> Result<Outcome> {
    let Some(password) = req.form().and_then(|form: &Form| form.text("password")) else {
        return Ok(get_response(req, ResponseKey::InvalidRequest).into());
    };
    let lookup = look_up(platform, raw).await?;
    let Some(lookup) = lookup.filter(|lookup| !lookup.peek) else {
        return Ok(not_found_page().into());
    };
    let Some(record) = &lookup.record else {
        return Ok(not_found_page().into());
    };
    let Some(stored) = &record.meta.pw else {
        return visit(platform, req, &lookup, record).await;
    };

    let raw = req.query("raw").is_some();
    let unlock_key = limit_key(&format!("unlock:{}", lookup.alias_hash), req);
    if is_limited(platform, &unlock_key, UNLOCK_LIMIT).await? {
        return Ok(unlock_page(&lookup.alias, UnlockState::Locked, raw).into());
    }
    if !verify_password(platform, password, stored).await? {
        count_attempt(platform, &unlock_key, UNLOCK_LIMIT).await?;
        return Ok(unlock_page(&lookup.alias, UnlockState::Wrong, raw).into());
    }
    visit(platform, req, &lookup, record).await
}

/// `GET /paste/ws/<alias>` connects a browser to the room behind an alias.
async fn live_socket<P: Platform>(platform: &P, req: &Req, raw: &str) -> Result<Outcome> {
    if !req.is_websocket {
        return Ok(Res::new(426).with_header("Upgrade", "websocket").into());
    }
    // A link is all it takes to get in, but other sites shouldn't get to use
    // their visitors' browsers for it.
    let origin = req.origin();
    if req
        .origin_header
        .as_deref()
        .is_some_and(|sent| sent != origin)
    {
        return Ok(Res::new(403).into());
    }
    let room = match look_up(platform, raw).await? {
        Some(lookup) => lookup
            .record
            .as_ref()
            .and_then(|record| live_room(&lookup, record)),
        None => None,
    };
    Ok(match room {
        Some((room, role)) => Outcome::LiveSocket { room, role },
        None => Res::new(404).into(),
    })
}

/// Answers one request.
pub async fn handle<P: Platform>(platform: &P, config: &Config, req: &Req) -> Result<Outcome> {
    let path = req.url.path();
    let path = path.strip_suffix('/').unwrap_or(path);
    let path = path.strip_prefix('/').unwrap_or(path);

    if let (Method::Get, Some(raw)) = (req.method, path.strip_prefix(LIVE_SOCKET_PREFIX)) {
        return live_socket(platform, req, raw).await;
    }
    let single_segment = !path.is_empty() && !path.contains('/');
    match (req.method, path) {
        (Method::Other, _) => Ok(Res::new(405).into()),
        (Method::Get | Method::Head, "") => Ok(Outcome::Asset),
        (Method::Get, "orb") => Ok(orb_get(platform, config, req).await?.into()),
        (Method::Post, "orb") => Ok(orb_post(platform, config, req).await?.into()),
        (Method::Post, "paste") => Ok(paste_post(platform, config, req).await?.into()),
        (Method::Post, "manage") => Ok(manage_post(platform, config, req).await?.into()),
        (Method::Post, "telegram") => Ok(telegram_post(platform, config, req).await?.into()),
        (Method::Post, _) if single_segment => alias_post(platform, req, path).await,
        (Method::Post, _) => Ok(Res::new(405).into()),
        (_, _) if single_segment => alias_get(platform, req, path).await,
        (_, _) => Ok(not_found_page().into()),
    }
}
