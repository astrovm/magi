use crate::support::{Fake, block, body, form, get, md5_hex, request, sha256_hex};
use magi_core::alias::{Alias, MAX_ALIAS_LENGTH, RESERVED_ALIASES};
use magi_core::guards::{
    CREATE_LIMIT, SAFE_BROWSING_URL, TURNSTILE_VERIFY_URL, UNLOCK_LIMIT, count_attempt,
    is_dangerous, is_limited, limit_key, passes_turnstile,
};
use magi_core::html::{PageOptions, render_page};
use magi_core::platform::{Body, Form, FormValue, Method, Place, PlatformError, PutOptions, Res};
use magi_core::qr::render_svg;
use magi_core::responses::{
    CreatedLink, ResponseKey, Tone, get_response, link_created, orb_message,
};
use magi_core::secrets::{
    PASSWORD_ITERATIONS, hash_password, random_token, timing_safe_equal, verify_password,
};
use magi_core::spells::{MAX_FORTUNES, parse_fortunes, parse_spells, parse_target};
use magi_core::store::{
    LINK_CACHE_TTL, LinkKind, LinkMeta, LinkRecord, add_click, clicks_key, delete_link, get_clicks,
    get_link, put_link,
};
use magi_core::target::{MAX_URL_LENGTH, Target, header_value, with_scheme};
use magi_core::telegram::{describe_device, describe_place, parse_command};
use magi_core::text::{
    decode_uri_component, escape_html, format_date, format_date_time, has_special_chars, js_trim,
    to_hex, utf16_len,
};
use magi_core::visits::{
    CURSE_CHANCE, RICKROLL_URL, needs_every_visit, pick_destination, redirect_to,
};
use magi_core::words::{ADJECTIVES, NOUNS, random_index, summon_wizard_words};
use serde_json::json;

fn record(url: &str, meta: LinkMeta) -> LinkRecord {
    LinkRecord {
        url: url.to_owned(),
        more: Vec::new(),
        meta,
    }
}

mod text {
    use super::*;

    #[test]
    fn writes_hex() {
        assert_eq!(to_hex(&[0, 15, 16, 255]), "000f10ff");
    }

    #[test]
    fn trims_like_javascript() {
        assert_eq!(js_trim(" \t\u{FEFF}\u{A0}a b\u{3000}\n"), "a b");
        assert_eq!(js_trim("\u{2003}\u{1680}a\u{2028}\u{202F}\u{205F}"), "a");
        // JavaScript keeps U+0085, even though Unicode calls it whitespace.
        assert_eq!(js_trim("\u{85}a"), "\u{85}a");
    }

    #[test]
    fn measures_like_javascript() {
        assert_eq!(utf16_len("abc"), 3);
        assert_eq!(utf16_len("ñ"), 1);
        assert_eq!(utf16_len("🪱"), 2);
    }

    #[test]
    fn detects_characters_that_need_url_encoding() {
        assert!(!has_special_chars("plain-alias_1.!~*'()"));
        assert!(has_special_chars("with space"));
        assert!(has_special_chars("ñandú"));
        assert!(has_special_chars("a/b"));
        assert!(has_special_chars("a+b"));
    }

    #[test]
    fn decodes_like_decode_uri_component() {
        assert_eq!(decode_uri_component("a%20b%2B"), Some("a b+".to_owned()));
        assert_eq!(decode_uri_component("%C3%B1"), Some("ñ".to_owned()));
        assert_eq!(
            decode_uri_component("plain+text"),
            Some("plain+text".to_owned())
        );
        for broken in ["%", "%2", "%ZZ", "%2Z", "bad%E0%A4%A", "%FF"] {
            assert_eq!(decode_uri_component(broken), None, "{broken}");
        }
    }

    #[test]
    fn escapes_html() {
        assert_eq!(
            escape_html(r#"<a href="x">'&'</a>"#),
            "&lt;a href=&quot;x&quot;&gt;&#39;&amp;&#39;&lt;/a&gt;"
        );
    }

    #[test]
    fn formats_dates_in_utc() {
        assert_eq!(format_date(0), "1970-01-01");
        assert_eq!(format_date(1_791_417_600_000), "2026-10-08");
        // Leap day, the last day of a year and the first of the next.
        assert_eq!(format_date(1_709_164_800_000), "2024-02-29");
        assert_eq!(format_date(1_735_603_200_000), "2024-12-31");
        assert_eq!(format_date(1_735_689_600_000), "2025-01-01");
        assert_eq!(format_date(951_782_400_000), "2000-02-29");
        // Around the century rules of the calendar.
        for (ms, date) in [
            (946_598_400_000, "1999-12-31"),
            (951_868_800_000, "2000-03-01"),
            (3_981_312_000_000, "2096-02-29"),
            (4_107_456_000_000, "2100-02-28"),
            (4_107_542_400_000, "2100-03-01"),
            (4_133_894_400_000, "2100-12-31"),
            (13_574_563_200_000, "2400-02-29"),
            (13_574_649_600_000, "2400-03-01"),
            (13_569_379_200_000, "2399-12-31"),
        ] {
            assert_eq!(format_date(ms), date);
        }
        assert_eq!(format_date_time(1_791_462_600_000), "2026-10-08 12:30 UTC");
    }
}

mod alias {
    use super::*;

    #[test]
    fn exposes_the_limits() {
        assert_eq!(MAX_ALIAS_LENGTH, 13312);
        assert_eq!(MAX_URL_LENGTH, 2048);
        assert_eq!(RESERVED_ALIASES, ["orb", "manage", "paste", "telegram"]);
    }

    #[test]
    fn normalizes_whitespace_and_percent_encoding() {
        assert_eq!(
            Alias::new("  my%20%20cool \t link ").as_str(),
            "my-cool-link"
        );
        assert_eq!(Alias::new("hello%20big world").as_str(), "hello-big-world");
    }

    #[test]
    fn keeps_malformed_percent_encoding() {
        assert_eq!(Alias::new("bad%E0%A4%A").as_str(), "bad%E0%A4%A");
    }

    #[test]
    fn notices_dots() {
        assert!(Alias::new("style.css").has_dot);
        assert!(Alias::new("style%2Ecss").has_dot);
        assert!(!Alias::new("hello").has_dot);
    }

    #[test]
    fn strips_the_peek_marker_once() {
        let mut alias = Alias::new("cool+");
        assert!(alias.strip_peek());
        assert_eq!(alias.as_str(), "cool");
        assert!(!alias.strip_peek());
        assert_eq!(alias.as_str(), "cool");
    }

    #[test]
    fn hashes_case_insensitively() {
        assert_eq!(Alias::new("HeLLo").hash(), md5_hex("hello"));
        assert_eq!(Alias::new("").hash(), "d41d8cd98f00b204e9800998ecf8427e");
    }

    #[test]
    fn knows_what_cannot_be_claimed() {
        for alias in [
            "", "%20", "file.txt", "ñandú", "a+b", "ORB", "Manage", "telegram",
        ] {
            assert!(Alias::new(alias).is_unclaimable(), "{alias:?}");
        }
        assert!(Alias::new(&"a".repeat(MAX_ALIAS_LENGTH + 1)).is_unclaimable());
        assert!(!Alias::new(&"a".repeat(MAX_ALIAS_LENGTH)).is_unclaimable());
        assert!(!Alias::new("orbs").is_unclaimable());
    }
}

mod target {
    use super::*;

    #[test]
    fn adds_https_to_input_without_a_scheme() {
        assert_eq!(with_scheme("  example.com/a "), "https://example.com/a");
        assert_eq!(with_scheme("http://example.com"), "http://example.com");
        assert_eq!(with_scheme("ftp://example.com"), "ftp://example.com");
    }

    #[test]
    fn keeps_the_text_as_typed_and_knows_its_host() {
        let target = Target::from_input("Example.com:8080/x").expect("valid");
        assert_eq!(target.as_str(), "https://Example.com:8080/x");
        assert_eq!(target.host(), "example.com:8080");
        assert_eq!(
            Target::from_input("https://example.com")
                .expect("valid")
                .host(),
            "example.com"
        );
        assert_eq!(
            Target::from_input("https://example.com:443/")
                .expect("valid")
                .host(),
            "example.com"
        );
        assert_eq!(target.into_string(), "https://Example.com:8080/x");
    }

    #[test]
    fn accepts_only_web_urls() {
        for url in [
            "https://example.com/path",
            "http://example.com",
            "example.com",
        ] {
            assert!(Target::from_input(url).is_some(), "{url}");
        }
        for url in [
            "ftp://example.com",
            "javascript:alert(1)://",
            "not a url",
            "mailto://x",
        ] {
            assert!(Target::from_input(url).is_none(), "{url}");
        }
    }

    #[test]
    fn refuses_line_breaks_because_links_are_stored_one_per_line() {
        assert!(Target::from_input("https://a.example/x\ny").is_none());
        assert!(Target::from_input("https://a.example/x\ty").is_none());
    }

    #[test]
    fn notices_long_urls() {
        let exact = format!("https://example.com/{}", "a".repeat(MAX_URL_LENGTH - 20));
        assert!(!Target::from_input(&exact).expect("valid").is_too_long());
        let over = format!("{exact}a");
        assert!(Target::from_input(&over).expect("valid").is_too_long());
    }

    #[test]
    fn makes_header_values_ascii() {
        assert_eq!(
            header_value("https://Example.com/x?y=1"),
            "https://Example.com/x?y=1"
        );
        assert_eq!(
            header_value("https://example.com/日本?q=ñ"),
            "https://example.com/%E6%97%A5%E6%9C%AC?q=%C3%B1"
        );
        assert_eq!(header_value("nonsense ñ"), "nonsense ");
    }
}

mod secrets {
    use super::*;

    #[test]
    fn makes_url_safe_random_tokens() {
        let fake = Fake::new();
        let token = random_token(&fake, 16).expect("token");
        assert_eq!(token.len(), 22);
        assert!(
            token
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || "-_".contains(ch))
        );
        assert_ne!(random_token(&fake, 16).expect("token"), token);
        assert_eq!(random_token(&fake, 3).expect("token").len(), 4);
    }

    #[test]
    fn hashes_with_sha256() {
        assert_eq!(
            sha256_hex("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn hashes_and_verifies_passwords() {
        let fake = Fake::new();
        assert_eq!(PASSWORD_ITERATIONS, 50_000);
        let stored = block(hash_password(&fake, "hunter2")).expect("hash");
        assert_ne!(
            stored,
            block(hash_password(&fake, "hunter2")).expect("hash")
        );
        assert!(block(verify_password(&fake, "hunter2", &stored)).expect("verify"));
        assert!(!block(verify_password(&fake, "hunter3", &stored)).expect("verify"));
    }

    #[test]
    fn treats_garbage_hashes_as_wrong() {
        let fake = Fake::new();
        for stored in ["garbage", ".", "abc.", ".abc", "!!.abc"] {
            assert!(
                !block(verify_password(&fake, "hunter2", stored)).expect("verify"),
                "{stored}"
            );
        }
    }

    #[test]
    fn verifies_hashes_made_by_the_old_typescript_site() {
        // Real output of the old site's WebCrypto code, for a fake password.
        let fake = Fake::new();
        let stored = crate_fixture::TS_PASSWORD_HASH;
        assert!(block(verify_password(&fake, crate_fixture::TS_PASSWORD, stored)).expect("verify"));
        assert!(!block(verify_password(&fake, "hunter2", stored)).expect("verify"));
    }

    #[test]
    fn compares_strings_in_constant_time() {
        assert!(timing_safe_equal("abc", "abc"));
        assert!(!timing_safe_equal("abc", "abd"));
        assert!(!timing_safe_equal("abc", "ab"));
        // Two differences that would cancel out if they were only XORed together.
        assert!(!timing_safe_equal("ab", "ba"));
    }
}

/// Fake secrets written by the old site, kept to prove the data still works.
mod crate_fixture {
    pub const TS_PASSWORD: &str = "hunter2 ñ";
    pub const TS_PASSWORD_HASH: &str =
        "UJdf6_DhRD3aDElatft7ug.sFxZwbiYSr2r5bfWDIpeXfCWB4nYijV7n4NOOK0rgTo";
    pub const TS_META: &str = r#"{"count":true,"created":1794000027905,"key":"ad3418bf7de33e8c1c7d698f0073fe95c55c7ffd839a19d67f864d982ff9caa9","pw":"UJdf6_DhRD3aDElatft7ug.sFxZwbiYSr2r5bfWDIpeXfCWB4nYijV7n4NOOK0rgTo"}"#;
}

mod store {
    use super::*;

    #[test]
    fn reads_links_with_metadata_and_the_cache_ttl() {
        let fake = Fake::new();
        fake.kv.borrow_mut().insert(
            "legacy".to_owned(),
            ("https://example.com".to_owned(), None),
        );
        let meta = LinkMeta {
            count: true,
            ..LinkMeta::default()
        };
        fake.kv.borrow_mut().insert(
            "new".to_owned(),
            ("https://example.org".to_owned(), Some(meta.clone())),
        );
        assert_eq!(
            block(get_link(&fake, "legacy")).expect("get"),
            Some(record("https://example.com", LinkMeta::default()))
        );
        assert_eq!(
            block(get_link(&fake, "new")).expect("get"),
            Some(record("https://example.org", meta))
        );
        assert_eq!(block(get_link(&fake, "missing")).expect("get"), None);
        assert_eq!(
            fake.gets.borrow()[0],
            ("legacy".to_owned(), Some(LINK_CACHE_TTL))
        );
        assert_eq!(LINK_CACHE_TTL, 300);
    }

    #[test]
    fn writes_links_with_expiration_only_when_they_expire() {
        let fake = Fake::new();
        block(put_link(
            &fake,
            "a",
            &record("https://a.example", LinkMeta::default()),
        ))
        .expect("put");
        let expiring = LinkMeta {
            exp: Some(123),
            ..LinkMeta::default()
        };
        block(put_link(
            &fake,
            "b",
            &record("https://b.example", expiring.clone()),
        ))
        .expect("put");
        assert_eq!(
            fake.put_options("a"),
            Some(PutOptions {
                metadata: Some(LinkMeta::default()),
                ..PutOptions::default()
            })
        );
        assert_eq!(
            fake.put_options("b"),
            Some(PutOptions {
                expiration: Some(123),
                metadata: Some(expiring),
                ..PutOptions::default()
            })
        );
    }

    #[test]
    fn round_trips_fortune_cookie_links() {
        let fake = Fake::new();
        let link = LinkRecord {
            url: "https://a.example".to_owned(),
            more: vec!["https://b.example".to_owned()],
            meta: LinkMeta::default(),
        };
        block(put_link(&fake, "f", &link)).expect("put");
        assert_eq!(
            fake.value("f").as_deref(),
            Some("https://a.example\nhttps://b.example")
        );
        assert_eq!(block(get_link(&fake, "f")).expect("get"), Some(link));
    }

    #[test]
    fn counts_clicks_and_deletes_everything() {
        let fake = Fake::new();
        assert_eq!(clicks_key("abc"), "clicks:abc");
        assert_eq!(block(get_clicks(&fake, "abc")).expect("get"), 0);
        assert_eq!(
            block(add_click(&fake, "abc", &LinkMeta::default())).expect("add"),
            1
        );
        assert_eq!(fake.put_options("clicks:abc"), Some(PutOptions::default()));
        let expiring = LinkMeta {
            exp: Some(99),
            ..LinkMeta::default()
        };
        assert_eq!(block(add_click(&fake, "abc", &expiring)).expect("add"), 2);
        assert_eq!(
            fake.put_options("clicks:abc")
                .and_then(|options| options.expiration),
            Some(99)
        );

        fake.kv
            .borrow_mut()
            .insert("abc".to_owned(), ("https://example.com".to_owned(), None));
        block(delete_link(&fake, "abc")).expect("delete");
        assert!(fake.kv.borrow().is_empty());
    }

    #[test]
    fn reads_garbage_counters_as_zero() {
        let fake = Fake::new();
        fake.kv
            .borrow_mut()
            .insert("clicks:abc".to_owned(), ("many".to_owned(), None));
        assert_eq!(block(get_clicks(&fake, "abc")).expect("get"), 0);
    }

    #[test]
    fn reads_and_writes_the_metadata_format_of_the_old_site() {
        let meta: LinkMeta = serde_json::from_str(crate_fixture::TS_META).expect("old metadata");
        assert!(meta.count);
        assert_eq!(meta.pw.as_deref(), Some(crate_fixture::TS_PASSWORD_HASH));
        let same: serde_json::Value = serde_json::from_str(crate_fixture::TS_META).expect("json");
        assert_eq!(serde_json::to_value(&meta).expect("json"), same);
    }

    #[test]
    fn leaves_unset_spells_out_of_the_metadata() {
        assert_eq!(
            serde_json::to_value(LinkMeta::default()).expect("json"),
            json!({})
        );
        let full = json!({
            "created": 1, "key": "k", "pw": "p", "max": 2, "count": true, "cursed": true,
            "exp": 3, "tg": -4, "wait": true
        });
        let meta: LinkMeta = serde_json::from_value(full.clone()).expect("meta");
        assert_eq!(serde_json::to_value(meta).expect("json"), full);
    }
}

mod spells {
    use super::*;

    const NOW: u64 = 1_800_000_000_000;

    #[test]
    fn reads_nothing_from_an_empty_form() {
        let spells = parse_spells(&form(&[]), NOW).expect("spells");
        assert_eq!(spells.meta, LinkMeta::default());
        assert_eq!(spells.password, None);
    }

    #[test]
    fn reads_every_spell() {
        let spells = parse_spells(
            &form(&[
                ("burn", "3"),
                ("expires", "3600"),
                ("password", "hunter2"),
                ("cursed", "on"),
                ("count", "on"),
                ("wait", "on"),
            ]),
            NOW,
        )
        .expect("spells");
        assert_eq!(spells.meta.max, Some(3));
        assert_eq!(spells.meta.exp, Some(1_800_003_600));
        assert_eq!(spells.password.as_deref(), Some("hunter2"));
        assert!(spells.meta.cursed && spells.meta.count && spells.meta.wait);
    }

    #[test]
    fn only_on_turns_a_checkbox_on() {
        let spells =
            parse_spells(&form(&[("cursed", "off"), ("count", "true")]), NOW).expect("spells");
        assert!(!spells.meta.cursed && !spells.meta.count);
    }

    #[test]
    fn reads_self_destruct_like_javascript_numbers() {
        for (text, max) in [
            ("1", 1),
            (" 5 ", 5),
            ("3.0", 3),
            ("1e3", 1000),
            ("1000000", 1_000_000),
        ] {
            assert_eq!(
                parse_spells(&form(&[("burn", text)]), NOW)
                    .expect("spells")
                    .meta
                    .max,
                Some(max)
            );
        }
    }

    #[test]
    fn refuses_weird_settings() {
        for fields in [
            [("burn", "0")],
            [("burn", "-1")],
            [("burn", "1.5")],
            [("burn", "1000001")],
            [("burn", "soon")],
            [("burn", "inf")],
            [("burn", "NaN")],
            [("burn", "  ")],
            [("expires", "42")],
        ] {
            assert!(parse_spells(&form(&fields), NOW).is_none(), "{fields:?}");
        }
    }

    #[test]
    fn limits_passwords_in_javascript_length() {
        let longest = "x".repeat(256);
        assert!(parse_spells(&form(&[("password", &longest)]), NOW).is_some());
        let over = "x".repeat(257);
        assert!(parse_spells(&form(&[("password", &over)]), NOW).is_none());
    }

    #[test]
    fn checks_targets() {
        assert_eq!(
            parse_target("example.org", "s.4st.li")
                .expect("ok")
                .as_str(),
            "https://example.org"
        );
        assert_eq!(
            parse_target("ftp://nope", "s.4st.li"),
            Err(ResponseKey::InvalidUrl)
        );
        assert_eq!(
            parse_target("s.4st.li/loop", "s.4st.li"),
            Err(ResponseKey::OwnTail)
        );
        let long = format!("https://example.com/{}", "a".repeat(MAX_URL_LENGTH));
        assert_eq!(
            parse_target(&long, "s.4st.li"),
            Err(ResponseKey::InvalidUrl)
        );
    }

    #[test]
    fn parses_one_fortune_per_line() {
        assert_eq!(
            parse_fortunes(" example.org \n\nhttps://example.net\r\n", "s.4st.li"),
            Ok(vec![
                "https://example.org".to_owned(),
                "https://example.net".to_owned()
            ])
        );
        assert_eq!(parse_fortunes("", "s.4st.li"), Ok(Vec::new()));
        assert_eq!(
            parse_fortunes("ftp://nope", "s.4st.li"),
            Err(ResponseKey::InvalidUrl)
        );
        assert_eq!(
            parse_fortunes("s.4st.li/loop", "s.4st.li"),
            Err(ResponseKey::OwnTail)
        );
        let too_many = ["example.com"; MAX_FORTUNES + 1].join("\n");
        assert_eq!(
            parse_fortunes(&too_many, "s.4st.li"),
            Err(ResponseKey::BadSpell)
        );
        let enough = ["example.com"; MAX_FORTUNES].join("\n");
        assert_eq!(
            parse_fortunes(&enough, "s.4st.li").expect("ok").len(),
            MAX_FORTUNES
        );
    }
}

mod guards {
    use super::*;

    #[test]
    fn counts_attempts_per_visitor_with_an_expiry() {
        let fake = Fake::new();
        let key = limit_key("test", &get("/"));
        assert_eq!(key, format!("limit:test:{}", sha256_hex("unknown")));
        assert!(!block(is_limited(&fake, &key, UNLOCK_LIMIT)).expect("limit"));
        for _ in 0..UNLOCK_LIMIT.max {
            block(count_attempt(&fake, &key, UNLOCK_LIMIT)).expect("count");
        }
        assert!(block(is_limited(&fake, &key, UNLOCK_LIMIT)).expect("limit"));
        assert_eq!(
            fake.put_options(&key)
                .and_then(|options| options.expiration_ttl),
            Some(900)
        );
        assert_eq!((CREATE_LIMIT.max, CREATE_LIMIT.window), (30, 3600));
    }

    #[test]
    fn keeps_raw_ips_out_of_storage() {
        let req = crate::support::from_ip(get("/"), "203.0.113.9");
        let key = limit_key("test", &req);
        assert_eq!(key, format!("limit:test:{}", sha256_hex("203.0.113.9")));
        assert!(!key.contains("203.0.113.9"));
    }

    #[test]
    fn skips_checks_that_are_not_configured() {
        let fake = Fake::new();
        assert!(block(passes_turnstile(&fake, None, None, &get("/"))));
        assert!(!block(is_dangerous(&fake, None, &["https://example.com"])));
        assert!(fake.calls.borrow().is_empty());
    }

    #[test]
    fn turnstile_needs_a_token_and_a_yes() {
        let fake = Fake::new();
        assert!(!block(passes_turnstile(
            &fake,
            Some("secret"),
            None,
            &get("/")
        )));
        assert!(!block(passes_turnstile(
            &fake,
            Some("secret"),
            Some(""),
            &get("/")
        )));
        assert!(fake.calls.borrow().is_empty());

        fake.reply_with(r#"{"success":false}"#);
        assert!(!block(passes_turnstile(
            &fake,
            Some("secret"),
            Some("t"),
            &get("/")
        )));
        fake.reply_with("not json");
        assert!(!block(passes_turnstile(
            &fake,
            Some("secret"),
            Some("t"),
            &get("/")
        )));
        fake.reply_failure();
        assert!(!block(passes_turnstile(
            &fake,
            Some("secret"),
            Some("t"),
            &get("/")
        )));
        fake.reply_with(r#"{"success":true}"#);
        let req = crate::support::from_ip(get("/"), "198.51.100.1");
        assert!(block(passes_turnstile(
            &fake,
            Some("s&cret"),
            Some("t k"),
            &req
        )));

        let calls = fake.calls.borrow();
        let last = calls.last().expect("a call");
        assert_eq!(last.url, TURNSTILE_VERIFY_URL);
        assert_eq!(last.content_type, "application/x-www-form-urlencoded");
        assert_eq!(
            last.body,
            "secret=s%26cret&response=t+k&remoteip=198.51.100.1"
        );
    }

    #[test]
    fn sends_an_empty_remote_ip_when_there_is_none() {
        let fake = Fake::new();
        fake.reply_with(r#"{"success":true}"#);
        assert!(block(passes_turnstile(
            &fake,
            Some("s"),
            Some("t"),
            &get("/")
        )));
        assert!(fake.calls.borrow()[0].body.ends_with("remoteip="));
    }

    #[test]
    fn asks_safe_browsing_about_all_urls_at_once() {
        let fake = Fake::new();
        fake.reply_with(r#"{"matches":[{"threatType":"SOCIAL_ENGINEERING"}]}"#);
        assert!(block(is_dangerous(
            &fake,
            Some("k&y"),
            &["https://a.example", "https://b.example"]
        )));
        let calls = fake.calls.borrow();
        assert_eq!(calls[0].url, format!("{SAFE_BROWSING_URL}?key=k%26y"));
        assert_eq!(calls[0].content_type, "application/json");
        let sent: serde_json::Value = serde_json::from_str(&calls[0].body).expect("json");
        assert_eq!(
            sent["threatInfo"]["threatEntries"],
            json!([{ "url": "https://a.example" }, { "url": "https://b.example" }])
        );
    }

    #[test]
    fn safe_browsing_fails_open() {
        let fake = Fake::new();
        for prepare in [
            (|fake: &Fake| fake.reply_with("{}")) as fn(&Fake),
            |fake| fake.reply_with(r#"{"matches":[]}"#),
            |fake| fake.reply_with("not json"),
            |fake| fake.reply_status(500),
            Fake::reply_failure,
        ] {
            prepare(&fake);
            assert!(!block(is_dangerous(
                &fake,
                Some("key"),
                &["https://a.example"]
            )));
        }
    }
}

mod words {
    use super::*;

    #[test]
    fn summons_adjective_noun_number_aliases() {
        assert_eq!(
            summon_wizard_words(|| 0.0),
            format!("{}-{}-0", ADJECTIVES[0], NOUNS[0])
        );
        assert_eq!(
            summon_wizard_words(|| 0.999),
            format!("{}-{}-99", ADJECTIVES[23], NOUNS[23])
        );
    }

    #[test]
    fn only_uses_url_safe_words() {
        for word in ADJECTIVES.iter().chain(&NOUNS) {
            assert!(word.chars().all(|ch| ch.is_ascii_lowercase()), "{word}");
        }
    }

    #[test]
    fn scales_random_numbers_into_range() {
        assert_eq!(random_index(0.0, 3), 0);
        assert_eq!(random_index(0.5, 3), 1);
        assert_eq!(random_index(0.99, 3), 2);
        assert_eq!(random_index(1.0, 3), 2);
        assert_eq!(random_index(0.5, 0), 0);
    }
}

mod visits {
    use super::*;

    #[test]
    fn picks_the_destination_sometimes_rick() {
        let cursed = record(
            "https://example.com",
            LinkMeta {
                cursed: true,
                ..LinkMeta::default()
            },
        );
        assert_eq!(
            pick_destination(&cursed, || CURSE_CHANCE - 0.01),
            RICKROLL_URL
        );
        assert_eq!(
            pick_destination(&cursed, || CURSE_CHANCE),
            "https://example.com"
        );
        let plain = record("https://example.com", LinkMeta::default());
        assert_eq!(pick_destination(&plain, || 0.0), "https://example.com");
    }

    #[test]
    fn picks_fortune_cookie_destinations_at_random() {
        let link = LinkRecord {
            url: "https://a.example".to_owned(),
            more: vec![
                "https://b.example".to_owned(),
                "https://c.example".to_owned(),
            ],
            meta: LinkMeta::default(),
        };
        assert_eq!(pick_destination(&link, || 0.0), "https://a.example");
        assert_eq!(pick_destination(&link, || 0.5), "https://b.example");
        assert_eq!(pick_destination(&link, || 0.99), "https://c.example");
    }

    #[test]
    fn knows_which_links_need_every_visit() {
        let url = "https://example.com";
        assert!(!needs_every_visit(&record(url, LinkMeta::default())));
        assert!(!needs_every_visit(&record(
            url,
            LinkMeta {
                created: Some(1),
                key: Some("k".to_owned()),
                exp: Some(2),
                ..LinkMeta::default()
            }
        )));
        for meta in [
            LinkMeta {
                count: true,
                ..LinkMeta::default()
            },
            LinkMeta {
                max: Some(1),
                ..LinkMeta::default()
            },
            LinkMeta {
                cursed: true,
                ..LinkMeta::default()
            },
            LinkMeta {
                tg: Some(1),
                ..LinkMeta::default()
            },
            LinkMeta {
                pw: Some("x".to_owned()),
                ..LinkMeta::default()
            },
            LinkMeta {
                wait: true,
                ..LinkMeta::default()
            },
        ] {
            assert!(needs_every_visit(&record(url, meta)));
        }
        let fortune = LinkRecord {
            more: vec!["https://example.org".to_owned()],
            ..record(url, LinkMeta::default())
        };
        assert!(needs_every_visit(&fortune));
    }

    #[test]
    fn builds_cacheable_and_private_redirects() {
        let cached = redirect_to("https://example.com", true);
        assert_eq!(cached.status, 301);
        assert_eq!(cached.header("Cache-Control"), Some("public, max-age=300"));
        let fresh = redirect_to("https://example.com", false);
        assert_eq!(fresh.status, 302);
        assert_eq!(fresh.header("Location"), Some("https://example.com"));
        assert_eq!(fresh.header("Cache-Control"), Some("no-store"));
    }
}

mod telegram {
    use super::*;

    #[test]
    fn describes_devices() {
        for (user_agent, expected) in [
            (
                "Mozilla/5.0 (Windows NT 10.0) Chrome/130.0 Safari/537.36",
                "Chrome on Windows",
            ),
            (
                "Mozilla/5.0 (Windows NT 10.0) Chrome/130.0 Safari/537.36 Edg/130.0",
                "Edge on Windows",
            ),
            (
                "Mozilla/5.0 (X11; Linux x86_64; rv:130.0) Gecko Firefox/130.0",
                "Firefox on Linux",
            ),
            (
                "Mozilla/5.0 (Linux; Android 15) Chrome/130.0 Mobile Safari/537.36 OPR/85",
                "Opera on Android",
            ),
            (
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Safari/605.1.15",
                "Safari on macOS",
            ),
            (
                "Mozilla/5.0 (iPad; CPU OS 18_0 like Mac OS X) Safari/604.1",
                "Safari on iOS",
            ),
            (
                "TelegramBot (like TwitterBot)",
                "🤖 a bot on a mystery device",
            ),
            ("", "a mystery browser on a mystery device"),
        ] {
            assert_eq!(describe_device(user_agent), expected);
        }
    }

    #[test]
    fn describes_places() {
        let place = |city: Option<&str>, country: Option<&str>| Place {
            city: city.map(str::to_owned),
            country: country.map(str::to_owned),
        };
        assert_eq!(
            describe_place(&place(Some("Rosario"), Some("AR"))),
            "Rosario, AR"
        );
        assert_eq!(describe_place(&place(Some(""), Some("AR"))), "AR");
        assert_eq!(describe_place(&place(None, None)), "somewhere in the void");
    }

    #[test]
    fn parses_commands() {
        let parsed = parse_command(&json!({
            "message": { "text": " /Alert@MagiOrbBot  cool  key ", "chat": { "id": -5 } }
        }))
        .expect("command");
        assert_eq!(parsed.chat_id, -5);
        assert_eq!(parsed.command, "alert");
        assert_eq!(parsed.args, ["cool", "key"]);
        let hello = parse_command(&json!({ "message": { "text": "hello", "chat": { "id": 5 } } }));
        assert_eq!(hello.expect("command").command, "hello");
        let empty = parse_command(&json!({ "message": { "text": "", "chat": { "id": 5 } } }));
        assert_eq!(empty.expect("command").command, "");
    }

    #[test]
    fn ignores_updates_that_are_not_commands() {
        for update in [
            json!({}),
            json!({ "message": { "text": "hi" } }),
            json!({ "message": { "chat": { "id": 5 } } }),
            json!({ "message": { "text": 7, "chat": { "id": 5 } } }),
            json!({ "message": { "text": "hi", "chat": { "id": "5" } } }),
        ] {
            assert_eq!(parse_command(&update), None);
        }
    }
}

mod pages_and_responses {
    use super::*;

    #[test]
    fn renders_full_pages() {
        let page = render_page(
            "<p>hi</p>",
            PageOptions {
                title: Some("<lost>"),
                status: Some(418),
                headers: vec![("X-Test", "1".to_owned())],
            },
        );
        assert_eq!(page.status, 418);
        assert_eq!(page.header("X-Test"), Some("1"));
        assert_eq!(
            page.header("Content-Type"),
            Some("text/html; charset=utf-8")
        );
        let html = body(&page);
        assert!(html.contains("<title>&lt;lost&gt; | Magi</title>"));
        assert!(html.contains(r#"<section class="card"><p>hi</p></section>"#));
        let plain = render_page("x", PageOptions::default());
        assert_eq!(plain.status, 200);
        assert!(body(&plain).contains("<title>Magi</title>"));
    }

    #[test]
    fn builds_every_response() {
        let mut req = request(Method::Post, "/orb");
        req.wants_fragment = true;
        for (key, message, status) in [
            (
                ResponseKey::InvalidRequest,
                "the orb rejected your request.",
                400,
            ),
            (
                ResponseKey::InvalidAlias,
                "the orb rejected your alias.",
                400,
            ),
            (
                ResponseKey::InvalidUrl,
                "the orb rejected your invalid url.",
                400,
            ),
            (
                ResponseKey::AliasLocked,
                "the orb rejected your access to rewrite this link.",
                409,
            ),
            (
                ResponseKey::OwnTail,
                "the orb refuses to eat its own tail.",
                400,
            ),
            (
                ResponseKey::WrongKey,
                "the orb doesn't recognize that manage key.",
                403,
            ),
            (
                ResponseKey::BadSpell,
                "the orb rejected your weird spell settings.",
                400,
            ),
            (
                ResponseKey::NoAliasLeft,
                "the worm ran out of ideas. try a custom alias.",
                503,
            ),
            (
                ResponseKey::LinkNotFound,
                "the orb didn't found this link.",
                404,
            ),
            (
                ResponseKey::DangerUrl,
                "the orb smells something fishy. that link looks like phishing or malware.",
                400,
            ),
            (
                ResponseKey::TooMany,
                "the worm is exhausted. try again later.",
                429,
            ),
            (
                ResponseKey::NotHuman,
                "the orb couldn't tell if you're human. try again.",
                403,
            ),
            (
                ResponseKey::LinkTransmuted,
                "the worm transmuted your link.",
                200,
            ),
            (
                ResponseKey::LinkBanished,
                "the worm banished your link to the shadow realm.",
                200,
            ),
            (
                ResponseKey::ServerError,
                "the orb tripped over its own hat. try again.",
                500,
            ),
        ] {
            let response = get_response(&req, key);
            let tone = if status < 300 { "success" } else { "error" };
            assert_eq!(response.status, status);
            assert_eq!(
                body(&response),
                format!(r#"<p class="orb-msg orb-msg--{tone}">{message}</p>"#)
            );
            assert_eq!(
                response.header("Content-Type"),
                Some("text/html; charset=utf-8")
            );
        }
    }

    #[test]
    fn wraps_responses_in_a_page_for_plain_requests() {
        let response = get_response(&request(Method::Post, "/orb"), ResponseKey::InvalidUrl);
        assert!(body(&response).starts_with("<!DOCTYPE html>"));
        assert_eq!(response.status, 400);
    }

    #[test]
    fn writes_messages_in_their_tone() {
        assert_eq!(
            orb_message("x", Tone::Success),
            r#"<p class="orb-msg orb-msg--success">x</p>"#
        );
    }

    #[test]
    fn escapes_the_created_link() {
        let mut req = request(Method::Post, "/orb");
        req.wants_fragment = true;
        let response = link_created(
            &req,
            &CreatedLink {
                short_url: r#"https://s.4st.li/a"b"#,
                manage_key: "<k>",
                alias: r#"a"b"#,
                telegram_bot: Some(r#"Bot""#),
                kind: LinkKind::Link,
            },
        );
        let html = body(&response);
        assert_eq!(response.status, 201);
        assert!(!html.contains(r#"a"b"#));
        assert!(!html.contains("<k>"));
        assert!(html.contains("/alert a&quot;b &lt;k&gt;"));
        assert!(html.contains("@Bot&quot;"));
    }

    #[test]
    fn shows_short_labels_without_the_scheme() {
        let mut req = request(Method::Post, "/orb");
        req.wants_fragment = true;
        for (short_url, label) in [
            ("https://s.4st.li/a", "s.4st.li/a"),
            ("http://localhost:8788/a", "localhost:8788/a"),
            ("weird/a", "weird/a"),
        ] {
            let response = link_created(
                &req,
                &CreatedLink {
                    short_url,
                    manage_key: "k",
                    alias: "a",
                    telegram_bot: None,
                    kind: LinkKind::Link,
                },
            );
            assert!(
                body(&response).contains(&format!(">{label}</a>")),
                "{label}"
            );
            assert!(!body(&response).contains("t.me"));
        }
    }

    #[test]
    fn draws_qr_codes() {
        let svg = render_svg("https://s.4st.li/a");
        assert!(svg.starts_with(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 "#));
        assert!(svg.contains(r#"<rect fill="white""#));
        assert!(svg.contains(r#"<path fill="black" d="M"#));
        assert!(svg.ends_with("</svg>"));
    }

    #[test]
    fn places_every_qr_module_on_the_grid() {
        // Version 1: 21 modules plus a border of 2 on each side, 10 units each.
        let svg = render_svg("a");
        assert!(svg.contains(r#"viewBox="0 0 250 250""#));
        assert!(svg.contains(r#"width="250" height="250""#));
        // Top row and left column of the corner square, past the border.
        assert!(svg.contains(r#"d="M20,20h10v10h-10zM30,20h10v10h-10z"#));
        assert!(svg.contains("M20,30h10v10h-10z"));
    }

    #[test]
    fn leaves_out_qr_codes_that_cannot_fit() {
        assert_eq!(render_svg(&"a".repeat(8000)), "");
    }

    #[test]
    fn exposes_platform_helpers() {
        assert_eq!(PlatformError("boom".to_owned()).to_string(), "boom");
        assert_eq!(Res::new(204).header("X"), None);
        let fields = vec![("a".to_owned(), FormValue::File)];
        let with_file = Form::new(fields);
        assert!(with_file.is_file("a"));
        assert_eq!(with_file.text("a"), None);
        assert!(!with_file.is_on("a"));
        assert_eq!(with_file.get("missing"), None);
        assert_eq!(Method::parse("GET"), Method::Get);
        assert_eq!(Method::parse("HEAD"), Method::Head);
        assert_eq!(Method::parse("POST"), Method::Post);
        assert_eq!(Method::parse("PUT"), Method::Other);
        let mut req = request(Method::Post, "/x?a=1&a=2");
        assert_eq!(req.query("a").as_deref(), Some("1"));
        assert_eq!(req.query("b"), None);
        assert_eq!(req.form(), None);
        req.body = Body::Unreadable;
        assert_eq!(req.form(), None);
        assert_eq!(req.origin(), "https://s.4st.li");
    }

    #[test]
    fn includes_the_port_in_the_host() {
        let url = url::Url::parse("http://localhost:8788/x").expect("url");
        let req = magi_core::platform::Req::new(Method::Get, url);
        assert_eq!(req.host(), "localhost:8788");
        assert_eq!(req.origin(), "http://localhost:8788");
    }
}
