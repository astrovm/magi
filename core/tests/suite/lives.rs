//! Live paste routes, with Cloudflare replaced by a fake.

use magi_core::alias::Alias;
use magi_core::live::{
    ROOM_INIT, ROOM_LOCK, ROOM_SNAPSHOTS, ROOM_TEXT, ROOM_UNLOCK, ROOM_WIPE, Role, RoomInit,
};
use magi_core::platform::{Config, HttpReply, Outcome, Res};
use magi_core::store::{LinkKind, LinkMeta};

use crate::support::{
    Fake, answer, between, block, body, get, head, manage_key_of, md5_hex, parts, post, run,
    sha256_hex, visit,
};

const KEY: &str = "manage-key";
const VIEW: &str = "view-token";

fn create(fake: &Fake, fields: &[(&str, &str)]) -> Res {
    let mut all = fields.to_vec();
    for default in [("live", "on"), ("alias", "room"), ("text", "")] {
        if !fields.iter().any(|(name, _)| *name == default.0) {
            all.push(default);
        }
    }
    answer(fake, &post("/paste", &all))
}

fn seed(fake: &Fake) {
    fake.store(
        "room",
        VIEW,
        Some(LinkMeta {
            kind: LinkKind::Live,
            key: Some(sha256_hex(KEY)),
            ..LinkMeta::default()
        }),
    );
    fake.store(
        VIEW,
        &md5_hex("room"),
        Some(LinkMeta {
            kind: LinkKind::View,
            ..LinkMeta::default()
        }),
    );
}

fn reply_status(fake: &Fake, status: u16, body: &str) {
    fake.room_replies.borrow_mut().push_back(HttpReply {
        status,
        body: body.to_owned(),
    });
}

fn manage(fake: &Fake, action: &str, extra: &[(&str, &str)]) -> Res {
    let mut all = vec![("alias", "room"), ("key", KEY), ("action", action)];
    all.extend_from_slice(extra);
    answer(fake, &post("/manage", &all))
}

fn room_paths(fake: &Fake) -> Vec<String> {
    fake.room_calls
        .borrow()
        .iter()
        .map(|(_, path, _)| path.clone())
        .collect()
}

mod creating {
    use super::*;

    #[test]
    fn makes_a_room_and_two_aliases() {
        let fake = Fake::new();
        let response = create(&fake, &[("text", "start\r\nhere")]);
        let html = body(&response);
        assert_eq!(response.status, 201);
        assert!(html.contains("the worm summoned your live paste"));
        let view_url = between(html, "view-only link</p>", "</div>");
        let view_alias = between(view_url, "href=\"https://s.4st.li/", "\"");
        assert_eq!(view_alias.len(), 22);
        assert!(view_url.contains(&format!(">s.4st.li/{view_alias}<")));

        let calls = fake.room_calls.borrow();
        let (room, path, sent) = &calls[0];
        assert_eq!(room, &md5_hex("room"));
        assert_eq!(path, ROOM_INIT);
        let init: RoomInit = serde_json::from_str(sent).expect("json");
        assert_eq!(init.text, "start\nhere");
        assert_eq!(init.link_hash, md5_hex("room"));
        assert_eq!(init.view_hash, Alias::new(view_alias).hash());

        assert_eq!(fake.link_value("room").as_deref(), Some(view_alias));
        let meta = fake.meta("room").expect("meta");
        assert_eq!(meta.kind, LinkKind::Live);
        assert_eq!(meta.key, Some(sha256_hex(&manage_key_of(html))));
        assert_eq!(meta.exp, None);

        let view_key = Alias::new(view_alias).hash();
        let stored = fake
            .kv
            .borrow()
            .get(&view_key)
            .cloned()
            .expect("a view record");
        assert_eq!(stored.0, md5_hex("room"));
        let view_meta = stored.1.expect("meta");
        assert_eq!(view_meta.kind, LinkKind::View);
        assert_eq!(view_meta.key, None);
    }

    #[test]
    fn ignores_the_spells_of_normal_pastes() {
        let fake = Fake::new();
        create(
            &fake,
            &[
                ("password", "x"),
                ("burn", "1"),
                ("expires", "3600"),
                ("count", "on"),
            ],
        );
        let meta = fake.meta("room").expect("meta");
        assert_eq!(
            (meta.pw, meta.max, meta.exp, meta.count),
            (None, None, None, false)
        );
    }

    #[test]
    fn refuses_what_a_normal_paste_refuses() {
        let fake = Fake::new();
        let huge = "a".repeat(512 * 1024 + 1);
        assert_eq!(create(&fake, &[("text", &huge)]).status, 413);
        assert_eq!(create(&fake, &[("alias", "no.dots")]).status, 400);
        fake.store("taken", "https://example.com", None);
        assert_eq!(create(&fake, &[("alias", "taken")]).status, 409);
        assert!(fake.room_calls.borrow().is_empty());

        let config = Config {
            safe_browsing_api_key: Some("key".to_owned()),
            ..Config::default()
        };
        fake.reply_with(r#"{"matches":[{}]}"#);
        let danger = post(
            "/paste",
            &[
                ("live", "on"),
                ("alias", "evil"),
                ("text", "https://evil.example"),
            ],
        );
        assert_eq!(visit(&fake, &config, &danger).status, 400);
        assert!(fake.room_calls.borrow().is_empty());
    }

    #[test]
    fn gives_up_cleanly_when_the_room_cannot_start() {
        let fake = Fake::new();
        reply_status(&fake, 500, "nope");
        let req = post("/paste", &[("live", "on"), ("alias", "room"), ("text", "")]);
        let result = block(magi_core::routes::handle(&fake, &Config::default(), &req));
        assert_eq!(result.expect_err("no room").0, "room init: 500");
        assert_eq!(fake.link_value("room"), None);
    }
}

mod visiting {
    use super::*;

    #[test]
    fn shows_the_editor_with_both_links() {
        let fake = Fake::new();
        seed(&fake);
        let response = answer(&fake, &get("/room"));
        let html = body(&response);
        assert_eq!(response.status, 200);
        assert!(html.contains(r#"data-alias="room" data-role="edit""#));
        assert!(html.contains(r#"<script src="/live.js" defer></script>"#));
        assert!(html.contains(r#"data-copy="https://s.4st.li/room""#));
        assert!(html.contains(&format!(r#"data-copy="https://s.4st.li/{VIEW}""#)));
        assert!(html.contains(r#"id="live-owner""#));
        assert_eq!(response.header("Cache-Control"), Some("no-store"));
        let policy = response
            .header("Content-Security-Policy")
            .expect("a policy");
        assert!(policy.contains("connect-src 'self' ws://s.4st.li wss://s.4st.li"));
        assert!(policy.contains("script-src 'self';"));
    }

    #[test]
    fn view_only_visitors_never_see_the_edit_link() {
        let fake = Fake::new();
        seed(&fake);
        let response = answer(&fake, &get(&format!("/{VIEW}")));
        let html = body(&response);
        assert!(html.contains(r#"data-role="view""#));
        assert!(!html.contains("/room"));
        assert!(!html.contains(&md5_hex("room")));
        assert!(!html.contains("live-owner"));
        assert!(!html.contains("copy link"));
    }

    #[test]
    fn serves_raw_text_from_the_room() {
        let fake = Fake::new();
        seed(&fake);
        reply_status(&fake, 200, "typed text");
        let edit = answer(&fake, &get("/room?raw"));
        assert_eq!(body(&edit), "typed text");
        assert_eq!(
            edit.header("Content-Type"),
            Some("text/plain; charset=utf-8")
        );
        reply_status(&fake, 200, "typed text");
        let view = answer(&fake, &get(&format!("/{VIEW}?raw")));
        assert_eq!(body(&view), "typed text");
        let calls = fake.room_calls.borrow();
        assert_eq!(calls[0].0, md5_hex("room"));
        assert_eq!(calls[0].1, ROOM_TEXT);
        assert_eq!(calls[1].0, md5_hex("room"));
    }

    #[test]
    fn answers_404_when_the_room_is_gone() {
        let fake = Fake::new();
        seed(&fake);
        reply_status(&fake, 404, "gone");
        assert_eq!(answer(&fake, &get("/room?raw")).status, 404);
    }

    #[test]
    fn head_needs_no_room() {
        let fake = Fake::new();
        seed(&fake);
        let (response, background) = parts(run(&fake, &Config::default(), &head("/room")));
        assert_eq!(response.status, 200);
        assert_eq!(background, None);
    }

    #[test]
    fn peek_describes_it_without_either_link() {
        let fake = Fake::new();
        seed(&fake);
        let edit = answer(&fake, &get("/room+"));
        assert!(body(&edit).contains("a live paste anyone can edit"));
        assert!(!body(&edit).contains(VIEW));
        let view = answer(&fake, &get(&format!("/{VIEW}+")));
        assert!(body(&view).contains("a live paste, view only"));
        assert!(!body(&view).contains("room"));
    }

    #[test]
    fn alerts_say_it_was_opened_but_not_what_it_holds() {
        let fake = Fake::new();
        fake.store(
            "watched",
            VIEW,
            Some(LinkMeta {
                kind: LinkKind::Live,
                tg: Some(42),
                ..LinkMeta::default()
            }),
        );
        let config = Config {
            telegram_bot_token: Some("token".to_owned()),
            ..Config::default()
        };
        visit(&fake, &config, &get("/watched"));
        let calls = fake.calls.borrow();
        assert!(calls[0].body.contains("the live paste"));
        assert!(!calls[0].body.contains(VIEW));
    }
}

mod sockets {
    use super::*;

    fn socket(path: &str) -> magi_core::platform::Req {
        let mut req = get(path);
        req.is_websocket = true;
        req
    }

    #[test]
    fn connects_each_link_with_its_role() {
        let fake = Fake::new();
        seed(&fake);
        let config = Config::default();
        assert_eq!(
            run(&fake, &config, &socket("/paste/ws/room")),
            Outcome::LiveSocket {
                room: md5_hex("room"),
                role: Role::Edit
            }
        );
        assert_eq!(
            run(&fake, &config, &socket(&format!("/paste/ws/{VIEW}"))),
            Outcome::LiveSocket {
                room: md5_hex("room"),
                role: Role::View
            }
        );
    }

    #[test]
    fn answers_requests_that_cannot_connect() {
        let fake = Fake::new();
        seed(&fake);
        fake.store("plain", "https://example.com", None);
        assert_eq!(answer(&fake, &get("/paste/ws/room")).status, 426);
        assert_eq!(answer(&fake, &socket("/paste/ws/ghost")).status, 404);
        assert_eq!(answer(&fake, &socket("/paste/ws/plain")).status, 404);
        assert_eq!(answer(&fake, &socket("/paste/ws/file.txt")).status, 404);
    }

    #[test]
    fn only_lets_the_site_itself_connect_from_a_browser() {
        let fake = Fake::new();
        seed(&fake);
        let mut foreign = socket("/paste/ws/room");
        foreign.origin_header = Some("https://evil.example".to_owned());
        assert_eq!(answer(&fake, &foreign).status, 403);

        let mut own = socket("/paste/ws/room");
        own.origin_header = Some("https://s.4st.li".to_owned());
        assert!(matches!(
            run(&fake, &Config::default(), &own),
            Outcome::LiveSocket { .. }
        ));
    }
}

mod owner {
    use super::*;

    #[test]
    fn locks_and_unlocks() {
        let fake = Fake::new();
        seed(&fake);
        assert!(body(&manage(&fake, "lock", &[])).contains("locked."));
        assert!(body(&manage(&fake, "unlock", &[])).contains("unlocked."));
        assert_eq!(room_paths(&fake), [ROOM_LOCK, ROOM_UNLOCK]);
    }

    #[test]
    fn lists_versions_and_restores_one() {
        let fake = Fake::new();
        seed(&fake);
        reply_status(&fake, 200, r#"[{"id":3,"at":5,"preview":"old"}]"#);
        let list = manage(&fake, "snapshots", &[]);
        assert_eq!(body(&list), r#"[{"id":3,"at":5,"preview":"old"}]"#);
        assert_eq!(list.header("Cache-Control"), Some("no-store"));
        assert_eq!(room_paths(&fake), [ROOM_SNAPSHOTS]);

        let restored = manage(&fake, "restore", &[("snapshot", "3")]);
        assert!(body(&restored).contains("rewound"));
        assert_eq!(room_paths(&fake)[1], "/restore/3");
    }

    #[test]
    fn refuses_versions_that_do_not_exist() {
        let fake = Fake::new();
        seed(&fake);
        assert_eq!(manage(&fake, "restore", &[]).status, 400);
        assert_eq!(manage(&fake, "restore", &[("snapshot", "-1")]).status, 400);
        assert_eq!(manage(&fake, "restore", &[("snapshot", "abc")]).status, 400);
        assert!(fake.room_calls.borrow().is_empty());
        reply_status(&fake, 404, "no such version");
        assert_eq!(manage(&fake, "restore", &[("snapshot", "9")]).status, 400);
    }

    #[test]
    fn banish_wipes_the_room_and_both_aliases() {
        let fake = Fake::new();
        seed(&fake);
        assert!(body(&manage(&fake, "banish", &[])).contains("banished your paste"));
        assert_eq!(room_paths(&fake), [ROOM_WIPE]);
        assert!(fake.kv.borrow().is_empty());
    }

    #[test]
    fn transmute_points_at_the_editor() {
        let fake = Fake::new();
        seed(&fake);
        let response = manage(&fake, "transmute", &[("text", "x")]);
        assert_eq!(response.status, 400);
        assert!(body(&response).contains("edited on their own page"));
        assert!(fake.room_calls.borrow().is_empty());
    }

    #[test]
    fn needs_the_manage_key() {
        let fake = Fake::new();
        seed(&fake);
        for action in ["lock", "unlock", "snapshots", "restore", "banish"] {
            let req = post(
                "/manage",
                &[("alias", "room"), ("key", "nope"), ("action", action)],
            );
            assert_eq!(answer(&fake, &req).status, 403, "{action}");
        }
        assert!(fake.room_calls.borrow().is_empty());
    }

    #[test]
    fn the_view_link_has_no_owner() {
        let fake = Fake::new();
        seed(&fake);
        let req = post(
            "/manage",
            &[("alias", VIEW), ("key", KEY), ("action", "banish")],
        );
        assert_eq!(answer(&fake, &req).status, 403);
    }

    #[test]
    fn other_kinds_refuse_live_actions() {
        let fake = Fake::new();
        fake.store(
            "room",
            "https://example.com",
            Some(LinkMeta {
                key: Some(sha256_hex(KEY)),
                ..LinkMeta::default()
            }),
        );
        for action in ["lock", "unlock", "snapshots", "restore"] {
            assert_eq!(manage(&fake, action, &[]).status, 400, "{action}");
        }
        assert_eq!(manage(&fake, "explode", &[]).status, 400);
    }
}
