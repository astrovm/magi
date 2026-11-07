//! The site answers on several addresses, but only hands out the short one.

use magi_core::platform::{Config, HttpReply, Outcome, Req};
use magi_core::store::{LinkKind, LinkMeta};
use magi_core::visits::Background;
use url::Url;

use crate::support::{Fake, HOST, answer, body, get, md5_hex, parts, post, run, sha256_hex};

const OTHER_ADDRESSES: [&str; 2] = ["https://s.4st.li", "https://r.4st.li"];
const EVERY_ADDRESS: [&str; 3] = [HOST, "https://s.4st.li", "https://r.4st.li"];
const PREVIEW: &str = "https://magi-try.example.workers.dev";
const VIEW: &str = "CTpOm7FFiy7ol1qbAAFvcw";

fn at(site: &str, mut req: Req) -> Req {
    let moved = req.url.as_str().replacen(HOST, site, 1);
    req.url = Url::parse(&moved).expect("test url");
    req
}

fn seed_room(fake: &Fake) {
    fake.store(
        "room",
        VIEW,
        Some(LinkMeta {
            kind: LinkKind::Live,
            key: Some(sha256_hex("key")),
            ..LinkMeta::default()
        }),
    );
    // Aliases are looked up in lowercase.
    fake.store(
        &VIEW.to_lowercase(),
        &md5_hex("room"),
        Some(LinkMeta {
            kind: LinkKind::View,
            ..LinkMeta::default()
        }),
    );
}

#[test]
fn links_use_the_short_address_wherever_they_are_made() {
    for site in EVERY_ADDRESS {
        let fake = Fake::new();
        let req = post("/orb", &[("alias", "cool"), ("url", "https://example.com")]);
        let response = answer(&fake, &at(site, req));
        let html = body(&response);
        assert_eq!(response.status, 201, "{site}");
        assert!(
            html.contains(
                r#"<a class="short-link" href="https://s.4st.li/cool">s.4st.li/cool</a>"#
            ),
            "{site}: {html}"
        );
        assert!(html.contains(r#"href="https://s.4st.li/cool+""#), "{site}");
    }
}

#[test]
fn pastes_use_the_short_address_wherever_they_are_made() {
    for site in OTHER_ADDRESSES {
        let fake = Fake::new();
        let req = post("/paste", &[("alias", "note"), ("text", "hi")]);
        let response = answer(&fake, &at(site, req));
        assert!(
            body(&response).contains(r#"href="https://s.4st.li/note""#),
            "{site}"
        );
    }
}

#[test]
fn live_pastes_use_the_short_address_wherever_they_are_made() {
    for site in EVERY_ADDRESS {
        let fake = Fake::new();
        let req = post(
            "/paste",
            &[("live", "on"), ("alias", "room"), ("text", "hi")],
        );
        let response = answer(&fake, &at(site, req));
        let html = body(&response);
        assert!(html.contains(r#"href="https://s.4st.li/room""#), "{site}");
        assert!(!html.contains("magi.4st.li"), "{site}: {html}");
        assert!(!html.contains("r.4st.li"), "{site}: {html}");
    }
}

#[test]
fn the_editor_shares_the_short_address_but_connects_to_its_own() {
    let fake = Fake::new();
    seed_room(&fake);
    let response = answer(&fake, &at("https://magi.4st.li", get("/room")));
    let html = body(&response);
    assert!(html.contains(r#"data-copy="https://s.4st.li/room""#));
    assert!(html.contains(&format!(r#"data-copy="https://s.4st.li/{VIEW}""#)));
    let policy = response
        .header("Content-Security-Policy")
        .expect("a policy");
    assert!(policy.contains("connect-src 'self' ws://magi.4st.li wss://magi.4st.li"));
}

#[test]
fn an_old_address_can_still_connect_to_a_room() {
    let fake = Fake::new();
    seed_room(&fake);
    let mut socket = at("https://magi.4st.li", get("/paste/ws/room"));
    socket.is_websocket = true;
    socket.origin_header = Some("https://magi.4st.li".to_owned());
    assert!(matches!(
        run(&fake, &Config::default(), &socket),
        Outcome::LiveSocket { .. }
    ));
}

#[test]
fn live_pages_open_at_home_where_the_owner_key_is() {
    for site in OTHER_ADDRESSES {
        for alias in ["room", VIEW] {
            let fake = Fake::new();
            seed_room(&fake);
            let response = answer(&fake, &at(site, get(&format!("/{alias}"))));
            assert_eq!(response.status, 302, "{site}/{alias}");
            assert_eq!(
                response.header("Location"),
                Some(format!("{HOST}/{alias}").as_str()),
                "{site}/{alias}"
            );
        }
    }
}

#[test]
fn live_pages_stay_put_at_home_and_on_a_try_out_deploy() {
    for site in [HOST, PREVIEW] {
        let fake = Fake::new();
        seed_room(&fake);
        let response = answer(&fake, &at(site, get("/room")));
        assert_eq!(response.status, 200, "{site}");
        assert!(body(&response).contains(r#"id="live-owner""#), "{site}");
    }
}

#[test]
fn a_protected_live_page_asks_for_its_password_only_at_home() {
    let fake = Fake::new();
    fake.store(
        "room",
        VIEW,
        Some(LinkMeta {
            kind: LinkKind::Live,
            pw: Some("stored".to_owned()),
            ..LinkMeta::default()
        }),
    );
    let response = answer(&fake, &at("https://s.4st.li", get("/room")));
    assert_eq!(response.status, 302);
    assert_eq!(
        response.header("Location"),
        Some(format!("{HOST}/room").as_str())
    );
    assert!(body(&answer(&fake, &get("/room"))).contains(r#"name="password""#));
}

#[test]
fn raw_text_never_moves() {
    let fake = Fake::new();
    seed_room(&fake);
    fake.room_replies.borrow_mut().push_back(HttpReply {
        status: 200,
        body: "typed text".to_owned(),
    });
    let response = answer(&fake, &at("https://s.4st.li", get("/room?raw")));
    assert_eq!(response.status, 200);
    assert_eq!(body(&response), "typed text");
}

#[test]
fn links_and_pastes_stay_on_the_address_they_were_opened_on() {
    let fake = Fake::new();
    fake.store(
        "note",
        "some text",
        Some(LinkMeta {
            kind: LinkKind::Paste,
            ..LinkMeta::default()
        }),
    );
    let paste = answer(&fake, &at("https://s.4st.li", get("/note")));
    assert_eq!(paste.status, 200);
    assert!(body(&paste).contains("some text"));
}

#[test]
fn old_links_keep_working_on_old_addresses() {
    for site in EVERY_ADDRESS {
        let fake = Fake::new();
        fake.store("old", "https://example.com", None);
        let response = answer(&fake, &at(site, get("/old")));
        assert_eq!(response.header("Location"), Some("https://example.com"));
    }
}

#[test]
fn peeking_shows_the_short_address() {
    let fake = Fake::new();
    fake.store("peek", "https://example.com", None);
    let response = answer(&fake, &at("https://magi.4st.li", get("/peek%2B")));
    let html = body(&response);
    assert!(html.contains("s.4st.li/peek"), "{html}");
    assert!(!html.contains("magi.4st.li"), "{html}");
}

#[test]
fn alerts_name_the_short_address() {
    let fake = Fake::new();
    fake.store(
        "watched",
        "https://example.com",
        Some(LinkMeta {
            tg: Some(42),
            ..LinkMeta::default()
        }),
    );
    let req = at("https://magi.4st.li", get("/watched"));
    let (_, background) = parts(run(&fake, &Config::default(), &req));
    let Some(Background::RecordVisit(visit)) = background else {
        panic!("expected a recorded visit");
    };
    assert_eq!(visit.short_link, "s.4st.li/watched");
}

#[test]
fn no_address_can_point_at_any_address() {
    for site in EVERY_ADDRESS {
        for target in ["s.4st.li/loop", "magi.4st.li/loop", "r.4st.li/loop"] {
            let fake = Fake::new();
            let req = post("/orb", &[("alias", "loop"), ("url", target)]);
            let response = answer(&fake, &at(site, req));
            assert!(
                body(&response).contains("its own tail"),
                "{site} to {target}"
            );
            assert_eq!(fake.link_value("loop"), None, "{site} to {target}");
        }
    }
}

#[test]
fn fortunes_cannot_point_at_any_address_either() {
    let fake = Fake::new();
    let req = post(
        "/orb",
        &[
            ("alias", "loop"),
            ("url", "https://example.com"),
            ("fortune", "https://magi.4st.li/loop"),
        ],
    );
    assert!(body(&answer(&fake, &req)).contains("its own tail"));
}

#[test]
fn a_try_out_deploy_keeps_its_own_address() {
    let fake = Fake::new();
    let req = post("/orb", &[("alias", "cool"), ("url", "https://example.com")]);
    let response = answer(&fake, &at(PREVIEW, req));
    let expected = format!(r#"href="{PREVIEW}/cool""#);
    assert!(body(&response).contains(&expected));

    let own = format!("{PREVIEW}/loop");
    let req = post("/orb", &[("alias", "loop"), ("url", own.as_str())]);
    assert!(body(&answer(&fake, &at(PREVIEW, req))).contains("its own tail"));
}
