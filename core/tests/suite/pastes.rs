//! Normal pastes, with Cloudflare replaced by a fake.

use magi_core::paste::{DEFAULT_PASTE_SECONDS, MAX_PASTE_BYTES, check_text, find_urls};
use magi_core::platform::{Body, Config, FormValue, Method};
use magi_core::responses::ResponseKey;
use magi_core::store::{LinkKind, LinkMeta, clicks_key, get_link};
use serde_json::Value;

use crate::support::{
    Fake, answer, block, body, from_ip, get, head, manage_key_of, md5_hex, parts, plain_post, post,
    request, run, sha256_hex, visit,
};

const TEXT: &str = "first line\nsecond line\n<script>alert('orb')</script>";

fn stored_paste(meta: LinkMeta) -> LinkMeta {
    LinkMeta {
        kind: LinkKind::Paste,
        ..meta
    }
}

mod checks {
    use super::*;

    #[test]
    fn needs_some_text_within_the_cap() {
        assert_eq!(check_text("hi"), Ok(()));
        assert_eq!(check_text(" \n\t"), Err(ResponseKey::EmptyPaste));
        assert_eq!(check_text(""), Err(ResponseKey::EmptyPaste));
        assert_eq!(check_text(&"a".repeat(MAX_PASTE_BYTES)), Ok(()));
        assert_eq!(
            check_text(&"a".repeat(MAX_PASTE_BYTES + 1)),
            Err(ResponseKey::PasteTooBig)
        );
    }

    #[test]
    fn finds_web_addresses_in_text() {
        let text = "see https://a.example/x, (http://b.example) and https://a.example/x.\n\
                    not httpd or ftp://c.example or http:// or https://bad host";
        assert_eq!(
            find_urls(text),
            ["https://a.example/x", "http://b.example", "https://bad"]
        );
        assert!(find_urls("nothing here").is_empty());
    }

    #[test]
    fn stops_at_what_safe_browsing_accepts() {
        let text: String = (0..600)
            .map(|index| format!("https://a{index}.example\n"))
            .collect();
        assert_eq!(find_urls(&text).len(), 500);
    }
}

mod creating {
    use super::*;

    #[test]
    fn stores_the_text_whole_and_expires_in_30_days() {
        let fake = Fake::new();
        let response = answer(&fake, &post("/paste", &[("alias", "Note"), ("text", TEXT)]));
        let html = body(&response);
        assert_eq!(response.status, 201);
        assert!(html.contains("the worm summoned your paste"));
        assert!(html.contains(r#"<a class="short-link" href="https://s.4st.li/Note">"#));
        assert_eq!(fake.link_value("note").as_deref(), Some(TEXT));

        let key = manage_key_of(html);
        let meta = fake.meta("note").expect("meta");
        let exp = fake.now.get() / 1000 + DEFAULT_PASTE_SECONDS;
        assert_eq!(
            meta,
            LinkMeta {
                kind: LinkKind::Paste,
                created: Some(fake.now.get()),
                key: Some(sha256_hex(&key)),
                exp: Some(exp),
                ..LinkMeta::default()
            }
        );
        assert_eq!(
            fake.put_options(&md5_hex("note"))
                .and_then(|options| options.expiration),
            Some(exp)
        );
    }

    #[test]
    fn stores_line_breaks_the_way_they_were_typed() {
        let fake = Fake::new();
        answer(
            &fake,
            &post("/paste", &[("alias", "crlf"), ("text", "a\r\nb\nc")]),
        );
        assert_eq!(fake.link_value("crlf").as_deref(), Some("a\nb\nc"));
    }

    #[test]
    fn reads_back_as_one_text_not_fortune_cookie_destinations() {
        let fake = Fake::new();
        answer(&fake, &post("/paste", &[("alias", "note"), ("text", TEXT)]));
        let record = block(get_link(&fake, &md5_hex("note")))
            .expect("get")
            .expect("a paste");
        assert_eq!(record.url, TEXT);
        assert!(record.more.is_empty());
    }

    #[test]
    fn can_pick_a_lifetime_or_none_at_all() {
        let fake = Fake::new();
        answer(
            &fake,
            &post(
                "/paste",
                &[("alias", "hour"), ("text", "x"), ("expires", "3600")],
            ),
        );
        answer(
            &fake,
            &post(
                "/paste",
                &[("alias", "never"), ("text", "x"), ("expires", "")],
            ),
        );
        assert_eq!(
            fake.meta("hour").and_then(|meta| meta.exp),
            Some(fake.now.get() / 1000 + 3600)
        );
        assert_eq!(fake.meta("never").and_then(|meta| meta.exp), None);
    }

    #[test]
    fn keeps_the_spells_that_fit_and_ignores_the_others() {
        let fake = Fake::new();
        let response = answer(
            &fake,
            &post(
                "/paste",
                &[
                    ("alias", "spells"),
                    ("text", TEXT),
                    ("password", "hunter2"),
                    ("burn", "3"),
                    ("count", "on"),
                    ("cursed", "on"),
                    ("wait", "on"),
                    ("fortune", "https://example.com"),
                ],
            ),
        );
        assert_eq!(response.status, 201);
        let meta = fake.meta("spells").expect("meta");
        assert_eq!(meta.max, Some(3));
        assert!(meta.count);
        assert!(!meta.cursed && !meta.wait);
        assert!(meta.pw.is_some_and(|hash| !hash.contains("hunter2")));
        assert_eq!(fake.link_value("spells").as_deref(), Some(TEXT));
    }

    #[test]
    fn picks_an_alias_when_none_is_given() {
        let fake = Fake::new();
        fake.default_random.set(0.0);
        assert!(body(&answer(&fake, &post("/paste", &[("text", "x")]))).contains("/sneaky-worm-0"));
    }

    #[test]
    fn shares_aliases_with_links() {
        let fake = Fake::new();
        fake.store("taken", "https://example.com", None);
        let response = answer(&fake, &post("/paste", &[("alias", "TAKEN"), ("text", "x")]));
        assert_eq!(response.status, 409);
        assert_eq!(
            fake.link_value("taken").as_deref(),
            Some("https://example.com")
        );
    }

    #[test]
    fn owns_its_path() {
        let fake = Fake::new();
        let link = answer(
            &fake,
            &post("/orb", &[("alias", "paste"), ("url", "example.com")]),
        );
        assert!(body(&link).contains("the orb rejected your alias."));
        assert_eq!(answer(&fake, &get("/paste")).status, 404);
    }

    #[test]
    fn rejects_bad_requests() {
        let fake = Fake::new();
        let mut unreadable = request(Method::Post, "/paste");
        unreadable.body = Body::Unreadable;
        let with_file = |name: &str, fields: Vec<(String, FormValue)>| {
            let mut req = request(Method::Post, "/paste");
            let mut fields = fields;
            fields.push((name.to_owned(), FormValue::File));
            req.body = Body::Form(magi_core::platform::Form::new(fields));
            req
        };
        let text = vec![("text".to_owned(), FormValue::Text("x".to_owned()))];
        for req in [
            post("/paste", &[("alias", "a")]),
            unreadable,
            with_file("text", Vec::new()),
            with_file("alias", text),
        ] {
            let response = answer(&fake, &req);
            assert_eq!(response.status, 400);
            assert!(body(&response).contains("the orb rejected your request."));
        }
        assert!(fake.kv.borrow().is_empty());
    }

    #[test]
    fn rejects_empty_huge_and_badly_spelled_pastes() {
        let huge = "a".repeat(MAX_PASTE_BYTES + 1);
        for (fields, status, message) in [
            (vec![("text", "  \n")], 400, "paste some text"),
            (vec![("text", huge.as_str())], 413, "512 KB at most"),
            (
                vec![("text", "x"), ("burn", "0")],
                400,
                "weird spell settings",
            ),
            (
                vec![("text", "x"), ("expires", "42")],
                400,
                "weird spell settings",
            ),
            (
                vec![("text", "x"), ("alias", "no.dots")],
                400,
                "rejected your alias",
            ),
        ] {
            let fake = Fake::new();
            let response = answer(&fake, &post("/paste", &fields));
            assert_eq!(response.status, status);
            assert!(body(&response).contains(message), "{message}");
            assert!(fake.kv.borrow().is_empty());
        }
    }

    #[test]
    fn goes_through_the_same_guards_as_links() {
        let fake = Fake::new();
        for index in 0..30 {
            let alias = format!("spam-{index}");
            let req = from_ip(
                post("/paste", &[("alias", &alias), ("text", "x")]),
                "198.51.100.1",
            );
            assert_eq!(answer(&fake, &req).status, 201);
        }
        let req = from_ip(
            post("/paste", &[("alias", "more"), ("text", "x")]),
            "198.51.100.1",
        );
        assert_eq!(answer(&fake, &req).status, 429);

        let human = Config {
            turnstile_secret_key: Some("secret".to_owned()),
            ..Config::default()
        };
        assert_eq!(
            visit(&fake, &human, &post("/paste", &[("text", "x")])).status,
            403
        );
    }

    #[test]
    fn refuses_text_that_links_to_something_dangerous() {
        let fake = Fake::new();
        let config = Config {
            safe_browsing_api_key: Some("key".to_owned()),
            ..Config::default()
        };
        fake.reply_with(r#"{"matches":[{}]}"#);
        let text = "click https://phish.example/login now";
        let response = visit(
            &fake,
            &config,
            &post("/paste", &[("alias", "bad"), ("text", text)]),
        );
        assert_eq!(response.status, 400);
        assert!(body(&response).contains("smells something fishy"));
        assert!(fake.link_value("bad").is_none());
        let sent: Value = serde_json::from_str(&fake.calls.borrow()[0].body).expect("json");
        assert_eq!(
            sent["threatInfo"]["threatEntries"][0]["url"],
            "https://phish.example/login"
        );
    }

    #[test]
    fn skips_safe_browsing_when_the_text_has_no_links() {
        let fake = Fake::new();
        let config = Config {
            safe_browsing_api_key: Some("key".to_owned()),
            ..Config::default()
        };
        assert_eq!(
            visit(&fake, &config, &post("/paste", &[("text", "x")])).status,
            201
        );
        assert!(fake.calls.borrow().is_empty());
    }
}

mod reading {
    use super::*;

    fn seed(fake: &Fake, alias: &str, meta: LinkMeta) {
        fake.store(alias, TEXT, Some(stored_paste(meta)));
    }

    #[test]
    fn shows_the_text_escaped_and_locked_down() {
        let fake = Fake::new();
        seed(&fake, "note", LinkMeta::default());
        let response = answer(&fake, &get("/note"));
        let html = body(&response);
        assert_eq!(response.status, 200);
        assert_eq!(response.header("Cache-Control"), Some("no-store"));
        assert_eq!(response.header("X-Content-Type-Options"), Some("nosniff"));
        let policy = response
            .header("Content-Security-Policy")
            .expect("a policy");
        assert!(policy.contains("default-src 'none'") && policy.contains("script-src 'self'"));
        assert!(!policy.contains("unsafe-inline"));
        assert!(html.contains("first line\nsecond line\n&lt;script&gt;alert(&#39;orb&#39;)"));
        assert!(!html.contains("<script>alert"));
        assert!(html.contains(r#"<a class="chip" href="/note?raw">raw</a>"#));
    }

    #[test]
    fn serves_raw_text_that_cannot_run() {
        let fake = Fake::new();
        seed(&fake, "note", LinkMeta::default());
        let response = answer(&fake, &get("/note?raw"));
        assert_eq!(response.status, 200);
        assert_eq!(body(&response), TEXT);
        assert_eq!(
            response.header("Content-Type"),
            Some("text/plain; charset=utf-8")
        );
        assert_eq!(response.header("X-Content-Type-Options"), Some("nosniff"));
        assert_eq!(
            response.header("Content-Security-Policy"),
            Some("default-src 'none'; sandbox")
        );
        assert_eq!(response.header("Cache-Control"), Some("no-store"));
    }

    #[test]
    fn asks_for_the_password_first() {
        let fake = Fake::new();
        let pw = block(magi_core::secrets::hash_password(&fake, "hunter2")).expect("hash");
        seed(
            &fake,
            "vault",
            LinkMeta {
                pw: Some(pw),
                ..LinkMeta::default()
            },
        );

        for path in ["/vault", "/vault?raw"] {
            assert!(
                !body(&answer(&fake, &get(path))).contains("first line"),
                "{path}"
            );
        }
        assert!(body(&answer(&fake, &get("/vault?raw"))).contains(r#"action="/vault?raw""#));
        assert_eq!(
            answer(&fake, &plain_post("/vault", &[("password", "nope")])).status,
            403
        );
        let wrong = answer(&fake, &plain_post("/vault?raw", &[("password", "nope")]));
        assert!(body(&wrong).contains(r#"action="/vault?raw""#));

        let page = answer(&fake, &plain_post("/vault", &[("password", "hunter2")]));
        assert!(body(&page).contains("first line"));
        let raw = answer(&fake, &plain_post("/vault?raw", &[("password", "hunter2")]));
        assert_eq!(body(&raw), TEXT);
    }

    #[test]
    fn counts_and_self_destructs() {
        let fake = Fake::new();
        seed(
            &fake,
            "burn",
            LinkMeta {
                max: Some(2),
                count: true,
                ..LinkMeta::default()
            },
        );
        let first = answer(&fake, &get("/burn"));
        assert!(!body(&first).contains("?raw"));
        assert_eq!(
            fake.value(&clicks_key(&md5_hex("burn"))).as_deref(),
            Some("1")
        );
        assert_eq!(answer(&fake, &get("/burn?raw")).status, 200);
        assert!(fake.link_value("burn").is_none());
        assert_eq!(answer(&fake, &get("/burn")).status, 404);
    }

    #[test]
    fn head_is_not_a_visit() {
        let fake = Fake::new();
        seed(
            &fake,
            "burn",
            LinkMeta {
                max: Some(1),
                ..LinkMeta::default()
            },
        );
        let (response, background) = parts(run(&fake, &Config::default(), &head("/burn")));
        assert_eq!(response.status, 200);
        assert_eq!(background, None);
    }

    #[test]
    fn peek_shows_the_size_but_not_the_text() {
        let fake = Fake::new();
        seed(
            &fake,
            "note",
            LinkMeta {
                count: true,
                ..LinkMeta::default()
            },
        );
        let response = answer(&fake, &get("/note+"));
        let html = body(&response);
        assert!(html.contains("<dt>holds</dt><dd>📋 a paste, 1 KB</dd>"));
        assert!(html.contains("<dt>visits</dt>"));
        assert!(!html.contains("first line"));
    }

    #[test]
    fn keeps_the_text_out_of_telegram() {
        let fake = Fake::new();
        seed(
            &fake,
            "watched",
            LinkMeta {
                tg: Some(42),
                ..LinkMeta::default()
            },
        );
        let config = Config {
            telegram_bot_token: Some("token".to_owned()),
            ..Config::default()
        };
        visit(&fake, &config, &get("/watched"));
        let calls = fake.calls.borrow();
        let sent: Value = serde_json::from_str(&calls[0].body).expect("json");
        let text = sent["text"].as_str().expect("text");
        assert!(text.contains("📋 the paste"));
        assert!(!text.contains("first line"));
    }
}

mod managing {
    use super::*;

    const KEY: &str = "manage-key";

    fn seed(fake: &Fake) {
        fake.store(
            "mine",
            TEXT,
            Some(stored_paste(LinkMeta {
                key: Some(sha256_hex(KEY)),
                exp: Some(2_000_000_000),
                ..LinkMeta::default()
            })),
        );
    }

    fn transmute(text: &str) -> Vec<(&'static str, String)> {
        vec![
            ("alias", "mine".to_owned()),
            ("key", KEY.to_owned()),
            ("action", "transmute".to_owned()),
            ("text", text.to_owned()),
        ]
    }

    fn manage(
        fake: &Fake,
        config: &Config,
        fields: &[(&'static str, String)],
    ) -> magi_core::platform::Res {
        let fields: Vec<(&str, &str)> = fields
            .iter()
            .map(|(name, value)| (*name, value.as_str()))
            .collect();
        visit(fake, config, &post("/manage", &fields))
    }

    #[test]
    fn transmute_edits_the_text_and_keeps_the_rest() {
        let fake = Fake::new();
        seed(&fake);
        let before = fake.meta("mine");
        let response = manage(&fake, &Config::default(), &transmute("new\ntext"));
        assert!(body(&response).contains("the worm transmuted your paste."));
        assert_eq!(fake.link_value("mine").as_deref(), Some("new\ntext"));
        manage(&fake, &Config::default(), &transmute("crlf\r\ntext"));
        assert_eq!(fake.link_value("mine").as_deref(), Some("crlf\ntext"));
        assert_eq!(fake.meta("mine"), before);
        assert_eq!(
            fake.put_options(&md5_hex("mine"))
                .and_then(|options| options.expiration),
            Some(2_000_000_000)
        );
    }

    #[test]
    fn transmute_checks_the_new_text() {
        let fake = Fake::new();
        seed(&fake);
        let config = Config {
            safe_browsing_api_key: Some("key".to_owned()),
            ..Config::default()
        };
        let huge = "a".repeat(MAX_PASTE_BYTES + 1);
        assert_eq!(manage(&fake, &config, &transmute(" ")).status, 400);
        assert_eq!(manage(&fake, &config, &transmute(&huge)).status, 413);
        let no_text = &transmute("x")[..3];
        assert_eq!(manage(&fake, &config, no_text).status, 400);
        fake.reply_with(r#"{"matches":[{}]}"#);
        assert_eq!(
            manage(&fake, &config, &transmute("https://evil.example")).status,
            400
        );
        assert_eq!(fake.link_value("mine").as_deref(), Some(TEXT));
    }

    #[test]
    fn banish_needs_the_key_and_removes_it() {
        let fake = Fake::new();
        seed(&fake);
        let wrong = post(
            "/manage",
            &[("alias", "mine"), ("key", "nope"), ("action", "banish")],
        );
        assert_eq!(answer(&fake, &wrong).status, 403);
        let right = post(
            "/manage",
            &[("alias", "mine"), ("key", KEY), ("action", "banish")],
        );
        assert_eq!(answer(&fake, &right).status, 200);
        assert_eq!(answer(&fake, &get("/mine")).status, 404);
    }

    #[test]
    fn telegram_alerts_keep_the_whole_text() {
        let fake = Fake::new();
        seed(&fake);
        let config = Config {
            telegram_bot_token: Some("token".to_owned()),
            telegram_webhook_secret: Some("secret".to_owned()),
            ..Config::default()
        };
        let mut req = request(Method::Post, "/telegram");
        req.telegram_secret = "secret".to_owned();
        let update = serde_json::json!({ "message": { "text": format!("/alert mine {KEY}"), "chat": { "id": 9 } } });
        req.body = Body::Text(update.to_string());
        assert_eq!(visit(&fake, &config, &req).status, 200);
        assert_eq!(fake.link_value("mine").as_deref(), Some(TEXT));
        assert_eq!(fake.meta("mine").and_then(|meta| meta.tg), Some(9));
    }
}
