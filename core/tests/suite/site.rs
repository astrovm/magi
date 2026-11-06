//! The site end to end, with Cloudflare replaced by a fake.

use crate::support::{
    Fake, answer, between, block, body, from_ip, get, head, manage_key_of, md5_hex, parts,
    plain_post, post, request, run, sha256_hex, visit,
};
use magi_core::guards::{CREATE_LIMIT, UNLOCK_LIMIT};
use magi_core::pages::COUNTDOWN_SECONDS;
use magi_core::platform::{Body, Config, FormValue, Method, Outcome, Place};
use magi_core::routes::handle;
use magi_core::secrets::hash_password;
use magi_core::store::{LinkMeta, clicks_key};
use magi_core::visits::{RICKROLL_URL, run_background};
use serde_json::{Value, json};

fn bot_config() -> Config {
    Config {
        telegram_bot_token: Some("token".to_owned()),
        telegram_webhook_secret: Some("webhook-secret".to_owned()),
        ..Config::default()
    }
}

fn json_of(fake: &Fake, config: &Config, path: &str) -> Value {
    let response = visit(fake, config, &get(path));
    assert_eq!(response.header("Cache-Control"), Some("no-store"));
    serde_json::from_str(body(&response)).expect("json")
}

mod routing {
    use super::*;

    #[test]
    fn leaves_the_homepage_to_the_static_site() {
        let fake = Fake::new();
        assert_eq!(run(&fake, &Config::default(), &get("/")), Outcome::Asset);
        assert_eq!(run(&fake, &Config::default(), &head("/")), Outcome::Asset);
    }

    #[test]
    fn serves_dotted_paths_as_files_without_a_lookup() {
        let fake = Fake::new();
        assert_eq!(
            run(&fake, &Config::default(), &get("/style.css")),
            Outcome::Asset
        );
        assert!(fake.gets.borrow().is_empty());
    }

    #[test]
    fn refuses_other_methods_everywhere() {
        let fake = Fake::new();
        for path in ["/", "/orb", "/some-alias", "/a/b"] {
            assert_eq!(answer(&fake, &request(Method::Other, path)).status, 405);
        }
        assert_eq!(answer(&fake, &post("/", &[])).status, 405);
        assert_eq!(answer(&fake, &post("/a/b", &[])).status, 405);
    }

    #[test]
    fn answers_multi_segment_paths_with_the_lost_page() {
        let fake = Fake::new();
        let response = answer(&fake, &get("/a/b"));
        assert_eq!(response.status, 404);
        assert!(body(&response).contains("the orb didn't found this link."));
    }

    #[test]
    fn treats_function_paths_as_ghosts_for_other_methods() {
        let fake = Fake::new();
        for path in ["/manage", "/telegram"] {
            assert_eq!(answer(&fake, &get(path)).status, 404);
        }
        assert_eq!(answer(&fake, &head("/orb")).status, 404);
    }

    #[test]
    fn accepts_a_trailing_slash() {
        let fake = Fake::new();
        fake.store("slash", "https://example.com", None);
        assert_eq!(answer(&fake, &get("/slash/")).status, 301);
        assert_eq!(answer(&fake, &get("/orb/")).status, 200);
    }

    #[test]
    fn passes_kv_failures_up() {
        let fake = Fake::new();
        fake.broken_kv.set(true);
        let result = block(handle(&fake, &Config::default(), &get("/anything")));
        assert_eq!(result.expect_err("kv is down").0, "kv down");
    }

    #[test]
    fn reports_background_failures() {
        let fake = Fake::new();
        fake.store(
            "counted",
            "https://example.com",
            Some(LinkMeta {
                count: true,
                ..LinkMeta::default()
            }),
        );
        let (_, background) = parts(run(&fake, &Config::default(), &get("/counted")));
        fake.broken_kv.set(true);
        let job = background.expect("a job");
        assert!(block(run_background(&fake, &Config::default(), &job)).is_err());
    }
}

mod visiting {
    use super::*;

    #[test]
    fn redirects_a_plain_link_permanently_with_a_short_cache() {
        let fake = Fake::new();
        fake.store("my-link", "https://example.com/target", None);
        let (response, background) = parts(run(&fake, &Config::default(), &get("/My%20Link")));
        assert_eq!(response.status, 301);
        assert_eq!(
            response.header("Location"),
            Some("https://example.com/target")
        );
        assert_eq!(
            response.header("Cache-Control"),
            Some("public, max-age=300")
        );
        assert_eq!(background, None);
    }

    #[test]
    fn shows_the_lost_page_for_a_missing_link() {
        let fake = Fake::new();
        let response = answer(&fake, &get("/nothing"));
        assert_eq!(response.status, 404);
        assert_eq!(response.header("Cache-Control"), Some("no-store"));
        assert!(body(&response).contains("the orb didn't found this link."));
    }

    #[test]
    fn previews_a_link_with_its_stats() {
        let fake = Fake::new();
        fake.store(
            "peek",
            "https://example.com/<b>",
            Some(LinkMeta {
                created: Some(1_791_417_600_000),
                max: Some(5),
                exp: Some(1_791_462_600),
                cursed: true,
                tg: Some(42),
                ..LinkMeta::default()
            }),
        );
        fake.kv
            .borrow_mut()
            .insert(clicks_key(&md5_hex("peek")), ("2".to_owned(), None));
        let response = answer(&fake, &get("/peek%2B"));
        let html = body(&response);
        assert_eq!(response.header("Cache-Control"), Some("no-store"));
        assert!(html.contains("s.4st.li/peek"));
        assert!(html.contains("https://example.com/&lt;b&gt;"));
        assert!(html.contains("<dt>born</dt><dd>2026-10-08</dd>"));
        assert!(html.contains("<dt>visits</dt><dd>2</dd>"));
        assert!(html.contains("after 3 more visits"));
        assert!(html.contains("2026-10-08 12:30 UTC"));
        assert!(html.contains("might rickroll you"));
        assert!(html.contains("visit alerts"));
    }

    #[test]
    fn previews_plain_links_and_keeps_passwords_secret() {
        let fake = Fake::new();
        fake.store("old", "https://example.com/old", None);
        fake.store(
            "locked",
            "https://example.com/hidden",
            Some(LinkMeta {
                pw: Some("x.y".to_owned()),
                count: true,
                ..LinkMeta::default()
            }),
        );
        let plain = answer(&fake, &get("/old+"));
        assert!(body(&plain).contains("https://example.com/old"));
        assert!(!body(&plain).contains("<dt>visits</dt>"));
        let locked = answer(&fake, &get("/locked+"));
        assert!(!body(&locked).contains("https://example.com/hidden"));
        assert!(body(&locked).contains("sworn to silence"));
        assert!(body(&locked).contains("<dt>visits</dt><dd>0</dd>"));
    }

    #[test]
    fn asks_for_the_password_of_a_guarded_link() {
        let fake = Fake::new();
        fake.store(
            "vault",
            "https://example.com/secret",
            Some(LinkMeta {
                pw: Some("x.y".to_owned()),
                ..LinkMeta::default()
            }),
        );
        let response = answer(&fake, &get("/vault"));
        assert_eq!(response.status, 200);
        assert!(body(&response).contains(r#"<form method="post" action="/vault""#));
        assert!(!body(&response).contains("https://example.com/secret"));
    }

    #[test]
    fn counts_visits_without_letting_browsers_cache_the_redirect() {
        let fake = Fake::new();
        fake.store(
            "counted",
            "https://example.com",
            Some(LinkMeta {
                count: true,
                exp: Some(2_000_000_000),
                ..LinkMeta::default()
            }),
        );
        for _ in 0..2 {
            let response = answer(&fake, &get("/counted"));
            assert_eq!(response.status, 302);
            assert_eq!(response.header("Cache-Control"), Some("no-store"));
        }
        let key = clicks_key(&md5_hex("counted"));
        assert_eq!(fake.value(&key).as_deref(), Some("2"));
        assert_eq!(
            fake.put_options(&key)
                .and_then(|options| options.expiration),
            Some(2_000_000_000)
        );
    }

    #[test]
    fn self_destructs_after_the_last_allowed_visit() {
        let fake = Fake::new();
        fake.store(
            "boom",
            "https://example.com",
            Some(LinkMeta {
                max: Some(2),
                ..LinkMeta::default()
            }),
        );
        assert_eq!(answer(&fake, &get("/boom")).status, 302);
        assert!(fake.link_value("boom").is_some());
        assert_eq!(answer(&fake, &get("/boom")).status, 302);
        assert!(fake.link_value("boom").is_none());
        assert!(fake.value(&clicks_key(&md5_hex("boom"))).is_none());
        assert_eq!(answer(&fake, &get("/boom")).status, 404);
    }

    #[test]
    fn treats_a_used_up_link_as_gone_even_if_the_lookup_is_cached() {
        let fake = Fake::new();
        fake.store(
            "boom",
            "https://example.com",
            Some(LinkMeta {
                max: Some(2),
                ..LinkMeta::default()
            }),
        );
        fake.kv
            .borrow_mut()
            .insert(clicks_key(&md5_hex("boom")), ("2".to_owned(), None));
        assert_eq!(answer(&fake, &get("/boom")).status, 404);
        assert!(fake.link_value("boom").is_none());
    }

    #[test]
    fn curses_sometimes_rickroll() {
        let fake = Fake::new();
        fake.store(
            "cursed",
            "https://example.com",
            Some(LinkMeta {
                cursed: true,
                ..LinkMeta::default()
            }),
        );
        fake.set_randoms(&[0.05]);
        assert_eq!(
            answer(&fake, &get("/cursed")).header("Location"),
            Some(RICKROLL_URL)
        );
        fake.set_randoms(&[0.5, 0.5]);
        assert_eq!(
            answer(&fake, &get("/cursed")).header("Location"),
            Some("https://example.com")
        );
    }

    #[test]
    fn snitches_visits_to_telegram_without_the_visitor_ip() {
        let fake = Fake::new();
        fake.store(
            "watched",
            "https://example.com",
            Some(LinkMeta {
                tg: Some(42),
                ..LinkMeta::default()
            }),
        );
        let mut req = from_ip(get("/watched"), "203.0.113.9");
        req.user_agent =
            "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) Safari/604.1".to_owned();
        req.place = Place {
            city: Some("Rosario".to_owned()),
            country: Some("AR".to_owned()),
        };
        assert_eq!(visit(&fake, &bot_config(), &req).status, 302);

        let calls = fake.calls.borrow();
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0].url,
            "https://api.telegram.org/bottoken/sendMessage"
        );
        let sent: Value = serde_json::from_str(&calls[0].body).expect("json");
        assert_eq!(sent["chat_id"], json!(42));
        assert_eq!(sent["disable_web_page_preview"], json!(true));
        assert_eq!(
            sent["text"],
            json!(
                "🔮 someone opened s.4st.li/watched\n🌍 Rosario, AR\n🧭 Safari on iOS\n➡️ https://example.com"
            )
        );
        assert!(!calls[0].body.contains("203.0.113.9"));
    }

    #[test]
    fn answers_head_like_get_without_counting_a_visit() {
        let fake = Fake::new();
        fake.store("plain", "https://example.com/plain", None);
        fake.store(
            "tracked",
            "https://example.com/tracked",
            Some(LinkMeta {
                count: true,
                tg: Some(42),
                ..LinkMeta::default()
            }),
        );
        for (alias, status) in [("plain", 301), ("tracked", 302)] {
            let (response, background) =
                parts(run(&fake, &bot_config(), &head(&format!("/{alias}"))));
            assert_eq!(response.status, status);
            assert_eq!(
                response.header("Location"),
                Some(format!("https://example.com/{alias}").as_str())
            );
            assert_eq!(background, None);
        }
        assert!(fake.value(&clicks_key(&md5_hex("tracked"))).is_none());
        assert!(fake.calls.borrow().is_empty());
    }

    #[test]
    fn stays_quiet_when_the_bot_is_not_configured() {
        let fake = Fake::new();
        fake.store(
            "watched",
            "https://example.com",
            Some(LinkMeta {
                tg: Some(42),
                ..LinkMeta::default()
            }),
        );
        answer(&fake, &get("/watched"));
        assert!(fake.calls.borrow().is_empty());
    }

    #[test]
    fn counts_the_visit_even_when_telegram_is_down() {
        let fake = Fake::new();
        fake.store(
            "both",
            "https://example.com",
            Some(LinkMeta {
                tg: Some(42),
                count: true,
                ..LinkMeta::default()
            }),
        );
        fake.reply_failure();
        let (_, background) = parts(run(&fake, &bot_config(), &get("/both")));
        let job = background.expect("a job");
        assert!(block(run_background(&fake, &bot_config(), &job)).is_err());
        assert_eq!(
            fake.value(&clicks_key(&md5_hex("both"))).as_deref(),
            Some("1")
        );
    }

    #[test]
    fn redirects_to_destinations_with_non_ascii_characters() {
        let fake = Fake::new();
        fake.store("japan", "https://example.com/日本", None);
        assert_eq!(
            answer(&fake, &get("/japan")).header("Location"),
            Some("https://example.com/%E6%97%A5%E6%9C%AC")
        );
    }
}

mod countdown {
    use super::*;

    #[test]
    fn makes_visitors_wait_before_redirecting() {
        let fake = Fake::new();
        fake.store(
            "slow",
            "https://example.com/slow",
            Some(LinkMeta {
                wait: true,
                ..LinkMeta::default()
            }),
        );
        let page = answer(&fake, &get("/slow"));
        assert_eq!(page.status, 200);
        assert_eq!(
            page.header("Refresh"),
            Some(format!("{COUNTDOWN_SECONDS}; url=https://example.com/slow").as_str())
        );
        assert_eq!(page.header("Cache-Control"), Some("no-store"));
        assert!(body(&page).contains(r#"data-target="https://example.com/slow""#));
        assert!(body(&page).contains("charging the orb"));
    }

    #[test]
    fn answers_head_with_the_redirect() {
        let fake = Fake::new();
        fake.store(
            "slow",
            "https://example.com/slow",
            Some(LinkMeta {
                wait: true,
                ..LinkMeta::default()
            }),
        );
        let response = answer(&fake, &head("/slow"));
        assert_eq!(response.status, 302);
        assert_eq!(
            response.header("Location"),
            Some("https://example.com/slow")
        );
    }

    #[test]
    fn shows_up_on_the_preview() {
        let fake = Fake::new();
        fake.store(
            "slowpoke",
            "https://example.com",
            Some(LinkMeta {
                wait: true,
                ..LinkMeta::default()
            }),
        );
        assert!(body(&answer(&fake, &get("/slowpoke+"))).contains("seconds of suspense"));
    }
}

mod unlocking {
    use super::*;

    fn store_vault(fake: &Fake, count: bool) {
        let pw = block(hash_password(fake, "hunter2")).expect("hash");
        fake.store(
            "vault",
            "https://example.com/secret",
            Some(LinkMeta {
                pw: Some(pw),
                count,
                ..LinkMeta::default()
            }),
        );
    }

    #[test]
    fn redirects_with_the_right_password() {
        let fake = Fake::new();
        store_vault(&fake, true);
        let response = answer(&fake, &plain_post("/vault", &[("password", "hunter2")]));
        assert_eq!(response.status, 302);
        assert_eq!(
            response.header("Location"),
            Some("https://example.com/secret")
        );
        assert_eq!(
            fake.value(&clicks_key(&md5_hex("vault"))).as_deref(),
            Some("1")
        );
    }

    #[test]
    fn scolds_wrong_passwords() {
        let fake = Fake::new();
        store_vault(&fake, false);
        let response = answer(&fake, &plain_post("/vault", &[("password", "nope")]));
        assert_eq!(response.status, 403);
        assert!(body(&response).contains("the worm is disappointed in you"));
    }

    #[test]
    fn just_redirects_links_without_a_password() {
        let fake = Fake::new();
        fake.store("open", "https://example.com", None);
        assert_eq!(
            answer(&fake, &plain_post("/open", &[("password", "whatever")])).status,
            301
        );
    }

    #[test]
    fn rejects_requests_without_a_password() {
        let fake = Fake::new();
        store_vault(&fake, false);
        assert_eq!(answer(&fake, &plain_post("/vault", &[])).status, 400);
        let mut no_form = request(Method::Post, "/vault");
        no_form.body = Body::Unreadable;
        assert_eq!(answer(&fake, &no_form).status, 400);
    }

    #[test]
    fn treats_missing_links_previews_and_files_as_lost() {
        let fake = Fake::new();
        store_vault(&fake, false);
        for path in ["/ghost", "/vault+", "/file.txt"] {
            assert_eq!(
                answer(&fake, &plain_post(path, &[("password", "x")])).status,
                404,
                "{path}"
            );
        }
    }

    #[test]
    fn locks_a_visitor_out_after_too_many_wrong_guesses() {
        let fake = Fake::new();
        store_vault(&fake, false);
        let guess = |password: &str, ip: &str| {
            answer(
                &fake,
                &from_ip(plain_post("/vault", &[("password", password)]), ip),
            )
        };
        for _ in 0..UNLOCK_LIMIT.max {
            assert_eq!(guess("nope", "198.51.100.1").status, 403);
        }
        let locked = guess("hunter2", "198.51.100.1");
        assert_eq!(locked.status, 429);
        assert!(body(&locked).contains("too many wrong guesses"));
        assert_eq!(guess("hunter2", "203.0.113.7").status, 302);
    }
}

mod creating {
    use super::*;

    const LINK: (&str, &str) = ("url", "https://example.com");

    #[test]
    fn stores_a_new_link_with_a_manage_key() {
        let fake = Fake::new();
        let response = answer(&fake, &post("/orb", &[("alias", " Cool%20 Link "), LINK]));
        let html = body(&response);
        assert_eq!(response.status, 201);
        assert_eq!(
            response.header("Content-Type"),
            Some("text/html; charset=utf-8")
        );
        assert!(!html.contains("<!DOCTYPE html>"));
        assert!(html.contains("the worm summoned your link"));
        assert!(html.contains(
            r#"<a class="short-link" href="https://s.4st.li/Cool-Link">s.4st.li/Cool-Link</a>"#
        ));
        assert!(html.contains(r#"href="https://s.4st.li/Cool-Link+""#));
        assert!(!html.contains("t.me"));

        let key = manage_key_of(html);
        assert_eq!(key.len(), 22);
        assert_eq!(
            fake.link_value("cool-link").as_deref(),
            Some("https://example.com")
        );
        let meta = fake.meta("cool-link").expect("meta");
        assert_eq!(meta.key, Some(sha256_hex(&key)));
        assert_eq!(meta.created, Some(fake.now.get()));
        assert_eq!(meta.pw, None);
    }

    #[test]
    fn renders_a_full_page_for_plain_form_posts() {
        let fake = Fake::new();
        let response = answer(
            &fake,
            &plain_post("/orb", &[("alias", "page"), ("url", "example.com")]),
        );
        assert!(body(&response).starts_with("<!DOCTYPE html>"));
        assert!(body(&response).contains("the worm summoned your link"));
        assert_eq!(
            fake.link_value("page").as_deref(),
            Some("https://example.com")
        );
    }

    #[test]
    fn adds_the_telegram_hint_when_a_bot_is_configured() {
        let fake = Fake::new();
        let config = Config {
            telegram_bot_username: Some("MagiOrbBot".to_owned()),
            ..Config::default()
        };
        let response = visit(&fake, &config, &post("/orb", &[("alias", "tg"), LINK]));
        let html = body(&response);
        let key = manage_key_of(html);
        assert!(html.contains(&format!("<code>/alert tg {key}</code>")));
        assert!(html.contains(r#"href="https://t.me/MagiOrbBot""#));
    }

    #[test]
    fn casts_every_extra_spell() {
        let fake = Fake::new();
        let response = answer(
            &fake,
            &post(
                "/orb",
                &[
                    ("alias", "spells"),
                    LINK,
                    ("password", "hunter2"),
                    ("burn", "3"),
                    ("expires", "3600"),
                    ("cursed", "on"),
                    ("count", "on"),
                ],
            ),
        );
        assert_eq!(response.status, 201);
        let meta = fake.meta("spells").expect("meta");
        assert_eq!(meta.max, Some(3));
        assert_eq!(meta.exp, Some(1_800_003_600));
        assert!(meta.cursed && meta.count);
        assert_eq!(meta.created, Some(1_800_000_000_000));
        let pw = meta.pw.as_deref().expect("hash");
        assert!(pw.contains('.') && !pw.contains("hunter2"));
        assert_eq!(
            fake.put_options(&md5_hex("spells"))
                .and_then(|options| options.expiration),
            Some(1_800_003_600)
        );
    }

    #[test]
    fn rejects_weird_spells() {
        for spell in [("burn", "0"), ("burn", "1.5"), ("expires", "42")] {
            let fake = Fake::new();
            let response = answer(&fake, &post("/orb", &[("alias", "x"), LINK, spell]));
            assert_eq!(response.status, 400);
            assert!(body(&response).contains("the orb rejected your weird spell settings."));
            assert!(fake.kv.borrow().is_empty());
        }
    }

    #[test]
    fn lets_the_worm_pick_an_alias() {
        let fake = Fake::new();
        fake.default_random.set(0.0);
        let response = answer(&fake, &post("/orb", &[LINK]));
        assert!(body(&response).contains("s.4st.li/sneaky-worm-0"));
        assert_eq!(
            fake.link_value("sneaky-worm-0").as_deref(),
            Some("https://example.com")
        );
    }

    #[test]
    fn tries_again_when_a_random_alias_is_taken() {
        let fake = Fake::new();
        fake.store("sneaky-worm-0", "https://taken.example", None);
        fake.default_random.set(0.0);
        // number, adjective and noun of the first try, then a different one.
        fake.set_randoms(&[0.0, 0.0, 0.0, 0.5, 0.0, 0.0]);
        let response = answer(&fake, &post("/orb", &[LINK]));
        assert_eq!(response.status, 201);
        assert!(body(&response).contains("s.4st.li/sneaky-worm-50"));
    }

    #[test]
    fn gives_up_when_every_random_alias_is_taken() {
        let fake = Fake::new();
        fake.store("sneaky-worm-0", "https://taken.example", None);
        fake.default_random.set(0.0);
        let response = answer(&fake, &post("/orb", &[("alias", "  "), LINK]));
        assert_eq!(response.status, 503);
        assert!(body(&response).contains("the worm ran out of ideas"));
    }

    #[test]
    fn refuses_to_overwrite_an_existing_alias() {
        let fake = Fake::new();
        fake.store("taken", "https://old.example", None);
        let response = answer(
            &fake,
            &post(
                "/orb",
                &[("alias", "TAKEN"), ("url", "https://new.example")],
            ),
        );
        assert_eq!(response.status, 409);
        assert!(body(&response).contains("the orb rejected your access to rewrite this link."));
        assert_eq!(
            fake.link_value("taken").as_deref(),
            Some("https://old.example")
        );
    }

    #[test]
    fn rejects_requests_without_a_url_or_with_files() {
        let fake = Fake::new();
        let mut unreadable = request(Method::Post, "/orb");
        unreadable.body = Body::Unreadable;
        let with_fields = |fields: Vec<(&str, FormValue)>| {
            let mut req = request(Method::Post, "/orb");
            let fields = fields
                .into_iter()
                .map(|(name, value)| (name.to_owned(), value))
                .collect();
            req.body = Body::Form(magi_core::platform::Form::new(fields));
            req
        };
        let url = || FormValue::Text("https://example.com".to_owned());
        for req in [
            post("/orb", &[("alias", "a")]),
            unreadable,
            with_fields(vec![("url", FormValue::File)]),
            with_fields(vec![("url", url()), ("alias", FormValue::File)]),
            with_fields(vec![("url", url()), ("fortune", FormValue::File)]),
        ] {
            let response = answer(&fake, &req);
            assert_eq!(response.status, 400);
            assert!(body(&response).contains("the orb rejected your request."));
        }
        assert!(fake.kv.borrow().is_empty());
    }

    #[test]
    fn rejects_bad_aliases() {
        let too_long = "a".repeat(13313);
        for alias in ["file.txt", "ñandú", too_long.as_str(), "Manage", "%20"] {
            let fake = Fake::new();
            let response = answer(&fake, &post("/orb", &[("alias", alias), LINK]));
            assert!(
                body(&response).contains("the orb rejected your alias."),
                "{alias}"
            );
            assert!(fake.kv.borrow().is_empty());
        }
    }

    #[test]
    fn rejects_bad_urls() {
        let too_long = format!("https://example.com/{}", "a".repeat(2048));
        for url in ["ftp://example.com", "not a url", too_long.as_str()] {
            let fake = Fake::new();
            let response = answer(&fake, &post("/orb", &[("alias", "ok"), ("url", url)]));
            assert!(
                body(&response).contains("the orb rejected your invalid url."),
                "{url}"
            );
            assert!(fake.kv.borrow().is_empty());
        }
    }

    #[test]
    fn refuses_to_shorten_its_own_links() {
        let fake = Fake::new();
        let response = answer(
            &fake,
            &post("/orb", &[("alias", "loop"), ("url", "s.4st.li/loop")]),
        );
        assert!(body(&response).contains("the orb refuses to eat its own tail."));
    }

    #[test]
    fn slows_down_spam_per_visitor() {
        let fake = Fake::new();
        let create = |alias: String, ip: &str| {
            answer(
                &fake,
                &from_ip(
                    post("/orb", &[("url", "https://example.com"), ("alias", &alias)]),
                    ip,
                ),
            )
        };
        for index in 0..CREATE_LIMIT.max {
            assert_eq!(create(format!("spam-{index}"), "198.51.100.1").status, 201);
        }
        let blocked = create("one-more".to_owned(), "198.51.100.1");
        assert_eq!(blocked.status, 429);
        assert!(body(&blocked).contains("the worm is exhausted"));
        assert_eq!(create("someone-else".to_owned(), "203.0.113.7").status, 201);
        let puts = fake.puts.borrow();
        let limit = puts
            .iter()
            .find(|(key, _, _)| key.starts_with("limit:create:"))
            .expect("a limit");
        assert_eq!(limit.2.expiration_ttl, Some(CREATE_LIMIT.window));
        assert!(!limit.0.contains("198.51.100.1"));
    }

    #[test]
    fn stops_people_turnstile_rejects() {
        let fake = Fake::new();
        let config = Config {
            turnstile_secret_key: Some("secret".to_owned()),
            ..Config::default()
        };
        let missing = visit(&fake, &config, &post("/orb", &[LINK, ("alias", "bot")]));
        assert_eq!(missing.status, 403);
        assert!(body(&missing).contains("couldn't tell if you're human"));

        fake.reply_with(r#"{"success":true}"#);
        let human = visit(
            &fake,
            &config,
            &post(
                "/orb",
                &[LINK, ("alias", "human"), ("cf-turnstile-response", "t")],
            ),
        );
        assert_eq!(human.status, 201);
    }

    #[test]
    fn refuses_links_safe_browsing_flags() {
        let fake = Fake::new();
        let config = Config {
            safe_browsing_api_key: Some("key".to_owned()),
            ..Config::default()
        };
        fake.reply_with(r#"{"matches":[{}]}"#);
        let response = visit(
            &fake,
            &config,
            &post(
                "/orb",
                &[("url", "https://phish.example"), ("alias", "phish")],
            ),
        );
        assert_eq!(response.status, 400);
        assert!(body(&response).contains("the orb smells something fishy"));
        assert!(fake.link_value("phish").is_none());

        fake.reply_with(r#"{"matches":[{}]}"#);
        let fortune = visit(
            &fake,
            &config,
            &post(
                "/orb",
                &[
                    ("url", "https://fine.example"),
                    ("fortune", "https://evil.example"),
                    ("alias", "mixed"),
                ],
            ),
        );
        assert_eq!(fortune.status, 400);
        let calls = fake.calls.borrow();
        assert!(
            calls
                .last()
                .expect("a call")
                .body
                .contains("https://evil.example")
        );
    }

    #[test]
    fn stores_every_fortune_and_picks_one_per_visit() {
        let fake = Fake::new();
        let response = answer(
            &fake,
            &post(
                "/orb",
                &[
                    ("url", "https://a.example"),
                    ("fortune", "b.example\nc.example"),
                    ("alias", "cookie"),
                ],
            ),
        );
        assert_eq!(response.status, 201);
        assert_eq!(
            fake.link_value("cookie").as_deref(),
            Some("https://a.example\nhttps://b.example\nhttps://c.example")
        );
        let visit = answer(&fake, &get("/cookie"));
        assert_eq!(visit.status, 302);
        assert_eq!(visit.header("Location"), Some("https://c.example"));
    }

    #[test]
    fn rejects_a_bad_fortune_list() {
        let fake = Fake::new();
        let response = answer(
            &fake,
            &post(
                "/orb",
                &[
                    ("url", "https://a.example"),
                    ("fortune", "nope nope"),
                    ("alias", "x"),
                ],
            ),
        );
        assert_eq!(response.status, 400);
        assert!(fake.kv.borrow().is_empty());
    }

    #[test]
    fn previews_every_destination() {
        let fake = Fake::new();
        fake.store("cookie", "https://a.example\nhttps://b.example", None);
        let html = answer(&fake, &get("/cookie+"));
        assert!(body(&html).contains("🥠 goes to one of"));
        assert!(body(&html).contains("https://a.example</a><br /><a"));
        assert!(body(&html).contains("https://b.example"));
    }

    #[test]
    fn comes_with_a_qr_code_and_what_my_links_needs() {
        let fake = Fake::new();
        let response = answer(&fake, &post("/orb", &[LINK, ("alias", "qr")]));
        let html = body(&response);
        assert!(html.contains(r#"<details class="qr">"#));
        assert!(html.contains(r#"<svg xmlns="http://www.w3.org/2000/svg""#));
        assert!(html.contains(r#"data-alias="qr" data-short="https://s.4st.li/qr" data-key=""#));
        assert_eq!(between(html, "data-key=\"", "\""), manage_key_of(html));
    }
}

mod orb_settings {
    use super::*;

    #[test]
    fn hands_out_the_turnstile_site_key() {
        let fake = Fake::new();
        assert_eq!(
            json_of(&fake, &Config::default(), "/orb"),
            json!({ "turnstileSiteKey": null })
        );
        let config = Config {
            turnstile_site_key: Some("site-key".to_owned()),
            ..Config::default()
        };
        assert_eq!(
            json_of(&fake, &config, "/orb"),
            json!({ "turnstileSiteKey": "site-key" })
        );
    }

    #[test]
    fn checks_aliases_while_you_type() {
        let fake = Fake::new();
        fake.store("taken", "https://example.com", None);
        let config = Config::default();
        assert_eq!(
            json_of(&fake, &config, "/orb?alias=Fresh%20Spell"),
            json!({ "status": "free", "alias": "Fresh-Spell" })
        );
        assert_eq!(
            json_of(&fake, &config, "/orb?alias=TAKEN"),
            json!({ "status": "taken" })
        );
        assert_eq!(
            json_of(&fake, &config, "/orb?alias=no.dots"),
            json!({ "status": "invalid" })
        );
        assert_eq!(
            json_of(&fake, &config, "/orb?alias=%20%20"),
            json!({ "status": "invalid" })
        );
    }
}

mod managing {
    use super::*;

    const KEY: &str = "manage-key";

    fn seed(fake: &Fake, meta: LinkMeta) {
        fake.store(
            "mine",
            "https://example.com/old",
            Some(LinkMeta {
                key: Some(sha256_hex(KEY)),
                ..meta
            }),
        );
        fake.kv
            .borrow_mut()
            .insert(clicks_key(&md5_hex("mine")), ("7".to_owned(), None));
    }

    #[test]
    fn banishes_a_link_and_its_counter() {
        let fake = Fake::new();
        seed(&fake, LinkMeta::default());
        let response = answer(
            &fake,
            &post(
                "/manage",
                &[
                    ("alias", "MINE"),
                    ("key", " manage-key "),
                    ("action", "banish"),
                ],
            ),
        );
        assert!(body(&response).contains("the worm banished your link to the shadow realm."));
        assert!(fake.kv.borrow().is_empty());
    }

    #[test]
    fn transmutes_a_link_and_keeps_its_spells() {
        let fake = Fake::new();
        seed(
            &fake,
            LinkMeta {
                exp: Some(2_000_000_000),
                cursed: true,
                ..LinkMeta::default()
            },
        );
        let response = answer(
            &fake,
            &post(
                "/manage",
                &[
                    ("alias", "mine"),
                    ("key", KEY),
                    ("action", "transmute"),
                    ("url", "example.org/new"),
                ],
            ),
        );
        assert!(body(&response).contains("the worm transmuted your link."));
        assert_eq!(
            fake.link_value("mine").as_deref(),
            Some("https://example.org/new")
        );
        assert_eq!(
            fake.meta("mine"),
            Some(LinkMeta {
                key: Some(sha256_hex(KEY)),
                exp: Some(2_000_000_000),
                cursed: true,
                ..LinkMeta::default()
            })
        );
        assert_eq!(
            fake.put_options(&md5_hex("mine"))
                .and_then(|options| options.expiration),
            Some(2_000_000_000)
        );
    }

    #[test]
    fn keeps_the_fortunes_when_transmuting() {
        let fake = Fake::new();
        fake.store(
            "cookie",
            "https://a.example\nhttps://b.example",
            Some(LinkMeta {
                key: Some(sha256_hex("k")),
                ..LinkMeta::default()
            }),
        );
        let response = answer(
            &fake,
            &post(
                "/manage",
                &[
                    ("alias", "cookie"),
                    ("key", "k"),
                    ("action", "transmute"),
                    ("url", "z.example"),
                ],
            ),
        );
        assert_eq!(response.status, 200);
        assert_eq!(
            fake.link_value("cookie").as_deref(),
            Some("https://z.example\nhttps://b.example")
        );
    }

    #[test]
    fn validates_the_new_link() {
        let fake = Fake::new();
        seed(&fake, LinkMeta::default());
        let base = [("alias", "mine"), ("key", KEY), ("action", "transmute")];
        let missing = answer(&fake, &post("/manage", &base));
        assert!(body(&missing).contains("invalid url"));
        for (url, message) in [
            ("ftp://example.com", "invalid url"),
            ("https://s.4st.li/mine", "its own tail"),
        ] {
            let mut fields = base.to_vec();
            fields.push(("url", url));
            assert!(body(&answer(&fake, &post("/manage", &fields))).contains(message));
        }
        assert_eq!(
            fake.link_value("mine").as_deref(),
            Some("https://example.com/old")
        );
    }

    #[test]
    fn refuses_dangerous_transmutations() {
        let fake = Fake::new();
        let config = Config {
            safe_browsing_api_key: Some("key".to_owned()),
            ..Config::default()
        };
        seed(&fake, LinkMeta::default());
        fake.reply_with(r#"{"matches":[{}]}"#);
        let response = visit(
            &fake,
            &config,
            &post(
                "/manage",
                &[
                    ("alias", "mine"),
                    ("key", KEY),
                    ("action", "transmute"),
                    ("url", "evil.example"),
                ],
            ),
        );
        assert_eq!(response.status, 400);
        assert_eq!(
            fake.link_value("mine").as_deref(),
            Some("https://example.com/old")
        );
    }

    #[test]
    fn rejects_wrong_keys_and_links_without_one() {
        let fake = Fake::new();
        seed(&fake, LinkMeta::default());
        fake.store("legacy", "https://example.com", None);
        for alias in ["mine", "legacy"] {
            let response = answer(
                &fake,
                &post(
                    "/manage",
                    &[("alias", alias), ("key", "nope"), ("action", "banish")],
                ),
            );
            assert_eq!(response.status, 403);
            assert!(body(&response).contains("the orb doesn't recognize that manage key."));
        }
        assert!(fake.link_value("mine").is_some());
    }

    #[test]
    fn reports_missing_links() {
        let fake = Fake::new();
        let response = answer(
            &fake,
            &post(
                "/manage",
                &[("alias", "ghost"), ("key", KEY), ("action", "banish")],
            ),
        );
        assert_eq!(response.status, 404);
    }

    #[test]
    fn rejects_incomplete_requests() {
        let fake = Fake::new();
        let mut unreadable = request(Method::Post, "/manage");
        unreadable.body = Body::Unreadable;
        for req in [
            post("/manage", &[("key", KEY), ("action", "banish")]),
            post("/manage", &[("alias", "mine"), ("action", "banish")]),
            post(
                "/manage",
                &[("alias", "mine"), ("key", KEY), ("action", "explode")],
            ),
            post(
                "/manage",
                &[("alias", ""), ("key", KEY), ("action", "banish")],
            ),
            post(
                "/manage",
                &[("alias", "mine"), ("key", ""), ("action", "banish")],
            ),
            unreadable,
        ] {
            assert_eq!(answer(&fake, &req).status, 400);
        }
    }
}

mod telegram_bot {
    use super::*;

    const KEY: &str = "manage-key";

    fn update(text: &str, secret: &str) -> magi_core::platform::Req {
        let mut req = request(Method::Post, "/telegram");
        req.telegram_secret = secret.to_owned();
        req.body =
            Body::Text(json!({ "message": { "text": text, "chat": { "id": 99 } } }).to_string());
        req
    }

    fn say(fake: &Fake, text: &str) -> u16 {
        visit(fake, &bot_config(), &update(text, "webhook-secret")).status
    }

    fn replies(fake: &Fake) -> Vec<String> {
        fake.calls
            .borrow()
            .iter()
            .map(|call| {
                let sent: Value = serde_json::from_str(&call.body).expect("json");
                sent["text"].as_str().expect("text").to_owned()
            })
            .collect()
    }

    #[test]
    fn hides_when_the_bot_is_not_configured_or_the_secret_is_wrong() {
        let fake = Fake::new();
        let only_token = Config {
            telegram_bot_token: Some("token".to_owned()),
            ..Config::default()
        };
        assert_eq!(
            visit(
                &fake,
                &Config::default(),
                &update("/start", "webhook-secret")
            )
            .status,
            404
        );
        assert_eq!(
            visit(&fake, &only_token, &update("/start", "webhook-secret")).status,
            404
        );
        assert_eq!(
            visit(&fake, &bot_config(), &update("/start", "wrong")).status,
            404
        );
        assert_eq!(
            visit(&fake, &bot_config(), &update("/start", "")).status,
            404
        );
        assert!(fake.calls.borrow().is_empty());
    }

    #[test]
    fn ignores_updates_it_cannot_read() {
        let fake = Fake::new();
        let mut not_json = update("", "webhook-secret");
        not_json.body = Body::Text("nope".to_owned());
        assert_eq!(visit(&fake, &bot_config(), &not_json).status, 200);
        let mut no_body = update("", "webhook-secret");
        no_body.body = Body::Unreadable;
        assert_eq!(visit(&fake, &bot_config(), &no_body).status, 200);
        assert!(fake.calls.borrow().is_empty());
    }

    #[test]
    fn answers_start_and_getid() {
        let fake = Fake::new();
        assert_eq!(say(&fake, "/start"), 200);
        assert_eq!(say(&fake, "/GetID@MagiOrbBot"), 200);
        let answers = replies(&fake);
        assert!(answers[0].starts_with("hi, i'm the orb's snitch 🔮"));
        assert_eq!(answers[1], "chat id: 99");
        assert_eq!(
            fake.calls.borrow()[0].url,
            "https://api.telegram.org/bottoken/sendMessage"
        );
    }

    #[test]
    fn toggles_visit_alerts_with_the_manage_key() {
        let fake = Fake::new();
        let base = LinkMeta {
            key: Some(sha256_hex(KEY)),
            exp: Some(2_000_000_000),
            ..LinkMeta::default()
        };
        fake.store("mine", "https://example.com", Some(base.clone()));
        say(&fake, &format!("/alert Mine {KEY}"));
        assert_eq!(
            fake.meta("mine"),
            Some(LinkMeta {
                tg: Some(99),
                ..base.clone()
            })
        );
        assert_eq!(
            fake.put_options(&md5_hex("mine"))
                .and_then(|options| options.expiration),
            Some(2_000_000_000)
        );
        say(&fake, &format!("/alert mine {KEY}"));
        assert_eq!(fake.meta("mine"), Some(base));
        assert_eq!(
            replies(&fake),
            [
                "👀 watching Mine. i'll snitch on every visit. give me a few minutes to wake up.",
                "😴 stopped watching mine."
            ]
        );
    }

    #[test]
    fn keeps_every_fortune_cookie_destination_when_toggling_alerts() {
        let fake = Fake::new();
        let destinations = "https://one.example\nhttps://two.example\nhttps://three.example";
        fake.store(
            "lucky",
            destinations,
            Some(LinkMeta {
                key: Some(sha256_hex(KEY)),
                ..LinkMeta::default()
            }),
        );
        say(&fake, &format!("/alert lucky {KEY}"));
        assert_eq!(fake.link_value("lucky").as_deref(), Some(destinations));
        say(&fake, &format!("/alert lucky {KEY}"));
        assert_eq!(fake.link_value("lucky").as_deref(), Some(destinations));
    }

    #[test]
    fn answers_200_when_replying_fails_so_telegram_does_not_retry_the_toggle() {
        let fake = Fake::new();
        fake.store(
            "mine",
            "https://example.com",
            Some(LinkMeta {
                key: Some(sha256_hex(KEY)),
                ..LinkMeta::default()
            }),
        );
        fake.reply_failure();
        assert_eq!(say(&fake, &format!("/alert mine {KEY}")), 200);
        assert_eq!(fake.meta("mine").and_then(|meta| meta.tg), Some(99));
    }

    #[test]
    fn refuses_bad_alert_requests() {
        let fake = Fake::new();
        fake.store(
            "mine",
            "https://example.com",
            Some(LinkMeta {
                key: Some(sha256_hex(KEY)),
                ..LinkMeta::default()
            }),
        );
        fake.store("legacy", "https://example.com", None);
        for text in [
            "/alert",
            "/alert mine",
            "/alert ghost key",
            "/alert mine nope",
            "/alert legacy nope",
        ] {
            say(&fake, text);
        }
        assert_eq!(
            replies(&fake),
            [
                "usage: /alert <alias> <manage key>",
                "usage: /alert <alias> <manage key>",
                "the orb didn't found this link.",
                "the orb doesn't recognize that manage key.",
                "the orb doesn't recognize that manage key."
            ]
        );
        assert_eq!(fake.meta("mine").and_then(|meta| meta.tg), None);
    }
}
