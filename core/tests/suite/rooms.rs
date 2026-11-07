//! The room logic, with real Yjs documents standing in for browsers.

use magi_core::live::{
    Bucket, CLOSE_BAD_FRAME, IDLE_LIFETIME_MS, MAX_AWARENESS_BYTES, MAX_STATE_BYTES, NOTICE_FULL,
    Reply, Role, Room, SNAPSHOT_EVERY_MS, awareness_query, expires_at, notice, snapshot_due,
};
use magi_core::live_frames::{SyncFrame, is_acceptable_update, parse_sync_frame};
use std::cell::Cell;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use yrs::sync::{Message, SyncMessage};
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;
use yrs::{Doc, GetString, ReadTxn, StateVector, Text, Transact, Update};

/// A browser tab: its own document, synced by exchanging frames with the room.
struct Client {
    doc: Doc,
}

impl Client {
    fn new() -> Self {
        Self { doc: Doc::new() }
    }

    fn text(&self) -> String {
        let text = self.doc.get_or_insert_text("codemirror");
        text.get_string(&self.doc.transact())
    }

    /// Types and returns the frame the browser would send.
    fn type_text(&self, index: u32, typed: &str) -> Vec<u8> {
        let text = self.doc.get_or_insert_text("codemirror");
        let mut txn = self.doc.transact_mut();
        text.insert(&mut txn, index, typed);
        Message::Sync(SyncMessage::Update(txn.encode_update_v1())).encode_v1()
    }

    fn hear(&self, frame: &[u8]) {
        match Message::decode_v1(frame).expect("a valid frame") {
            Message::Sync(SyncMessage::SyncStep2(update) | SyncMessage::Update(update)) => {
                let update = Update::decode_v1(&update).expect("a valid update");
                self.doc
                    .transact_mut()
                    .apply_update(update)
                    .expect("applies");
            }
            other => panic!("unexpected frame: {other:?}"),
        }
    }

    fn sync_step_1(&self) -> Vec<u8> {
        let state_vector = self.doc.transact().state_vector();
        Message::Sync(SyncMessage::SyncStep1(state_vector)).encode_v1()
    }

    /// What this tab answers when the room asks what it's missing.
    fn answer(&self, request: &[u8]) -> Vec<u8> {
        let Message::Sync(SyncMessage::SyncStep1(state_vector)) =
            Message::decode_v1(request).expect("a valid frame")
        else {
            panic!("expected a sync request");
        };
        let update = self.doc.transact().encode_diff_v1(&state_vector);
        Message::Sync(SyncMessage::SyncStep2(update)).encode_v1()
    }

    fn sync_with(&self, room: &mut Room) {
        let reply = room.receive(Role::Edit, false, &self.sync_step_1());
        for frame in reply.to_sender {
            self.hear(&frame);
        }
    }
}

fn saved(room: &mut Room) -> Room {
    Room::load(&room.save()).expect("loads")
}

#[test]
fn a_new_client_gets_the_starting_text() {
    let mut room = Room::create("hello");
    assert_eq!(room.text(), "hello");
    let client = Client::new();
    client.sync_with(&mut room);
    assert_eq!(client.text(), "hello");
}

#[test]
fn an_edit_reaches_the_others_and_is_marked_for_saving() {
    let mut room = Room::create("");
    let (ana, bo) = (Client::new(), Client::new());
    let reply = room.receive(Role::Edit, false, &ana.type_text(0, "hi"));
    assert!(reply.changed);
    assert!(reply.to_sender.is_empty());
    bo.hear(&reply.to_others[0]);
    assert_eq!(bo.text(), "hi");
    assert_eq!(room.text(), "hi");
}

#[test]
fn concurrent_typing_converges_everywhere() {
    let mut room = Room::create("");
    let clients: Vec<Client> = (0..5).map(|_| Client::new()).collect();
    let mut frames = Vec::new();
    for round in 0..20u32 {
        for (index, client) in clients.iter().enumerate() {
            frames.push((index, client.type_text(round, &format!("{index}"))));
        }
    }
    // Nobody heard anybody while typing. Everything arrives late and interleaved.
    for (from, frame) in &frames {
        let reply = room.receive(Role::Edit, false, frame);
        for (index, client) in clients.iter().enumerate() {
            if index != *from {
                client.hear(&reply.to_others[0]);
            }
        }
    }
    let text = room.text();
    assert_eq!(text.len(), 100);
    for client in &clients {
        assert_eq!(client.text(), text);
    }
}

#[test]
fn survives_a_save_and_load() {
    let mut room = Room::create("start");
    let client = Client::new();
    client.sync_with(&mut room);
    room.receive(Role::Edit, false, &client.type_text(5, " more"));
    let mut reloaded = saved(&mut room);
    assert_eq!(reloaded.text(), "start more");

    // A client that was connected the whole time keeps editing on top of it.
    let reply = reloaded.receive(Role::Edit, false, &client.type_text(10, "!"));
    assert!(reply.changed);
    assert_eq!(reloaded.text(), "start more!");
}

#[test]
fn a_room_that_lost_its_memory_gets_it_back_from_open_tabs() {
    let mut room = Room::create("saved");
    let client = Client::new();
    client.sync_with(&mut room);
    let state = room.save();
    // Typed after the last save, then the room forgets everything.
    room.receive(Role::Edit, false, &client.type_text(5, " unsaved"));
    let mut rebooted = Room::load(&state).expect("loads");
    assert_eq!(rebooted.text(), "saved");

    let answer = client.answer(&rebooted.sync_request());
    let reply = rebooted.receive(Role::Edit, false, &answer);
    assert!(reply.changed);
    assert_eq!(rebooted.text(), "saved unsaved");
}

#[test]
fn viewers_can_read_but_never_change_anything() {
    let mut room = Room::create("read me");
    let viewer = Client::new();
    viewer.sync_with(&mut room);
    assert_eq!(viewer.text(), "read me");

    assert_eq!(
        room.receive(Role::View, false, &viewer.type_text(0, "x")),
        Reply::default()
    );
    let sneaky = viewer.answer(&room.sync_request());
    assert_eq!(room.receive(Role::View, false, &sneaky), Reply::default());
    assert_eq!(room.text(), "read me");

    // But they still hear everything others do.
    let editor = Client::new();
    editor.sync_with(&mut room);
    let frames = room
        .receive(Role::Edit, false, &editor.type_text(0, "new "))
        .to_others;
    assert_eq!(frames.len(), 1);
}

#[test]
fn a_locked_room_refuses_edits_but_still_syncs() {
    let mut room = Room::create("frozen");
    let client = Client::new();
    let reply = room.receive(Role::Edit, true, &client.sync_step_1());
    client.hear(&reply.to_sender[0]);
    assert_eq!(client.text(), "frozen");
    assert_eq!(
        room.receive(Role::Edit, true, &client.type_text(0, "x")),
        Reply::default()
    );
    assert_eq!(room.text(), "frozen");
}

#[test]
fn unlocking_lets_refused_edits_back_in() {
    let mut room = Room::create("ab");
    let client = Client::new();
    client.sync_with(&mut room);
    room.receive(Role::Edit, true, &client.type_text(2, "c"));
    assert_eq!(room.text(), "ab");
    let answer = client.answer(&room.sync_request());
    assert!(room.receive(Role::Edit, false, &answer).changed);
    assert_eq!(room.text(), "abc");
}

#[test]
fn passes_cursors_along_from_editors_only() {
    let mut room = Room::create("");
    let cursor = vec![1, 3, 7, 7, 7];
    assert_eq!(
        room.receive(Role::Edit, false, &cursor).to_others,
        vec![cursor.clone()]
    );
    assert_eq!(
        room.receive(Role::Edit, true, &cursor).to_others,
        vec![cursor.clone()]
    );
    assert_eq!(room.receive(Role::View, false, &cursor), Reply::default());
    let mut huge = vec![1];
    huge.resize(MAX_AWARENESS_BYTES, 0);
    assert_eq!(room.receive(Role::Edit, false, &huge).to_others.len(), 1);
    huge.push(0);
    assert_eq!(room.receive(Role::Edit, false, &huge), Reply::default());
}

#[test]
fn ignores_message_types_it_has_no_use_for() {
    let mut room = Room::create("safe");
    for frame in [Vec::new(), vec![2, 0], vec![3], vec![9, 9, 9]] {
        assert_eq!(
            room.receive(Role::Edit, false, &frame),
            Reply::default(),
            "{frame:?}"
        );
    }
    assert_eq!(room.text(), "safe");
}

#[test]
fn closes_the_socket_of_whoever_sends_a_broken_frame() {
    let mut room = Room::create("safe");
    for frame in [
        vec![0],
        vec![0, 0],
        vec![0, 0, 200],
        vec![0, 1, 3, 1, 2, 3],
        vec![0, 2, 200, 1],
        vec![0, 9, 0],
        vec![0, 2, 0, 7],
        Message::Sync(SyncMessage::Update(vec![255, 255, 255])).encode_v1(),
    ] {
        let reply = room.receive(Role::Edit, false, &frame);
        assert_eq!(reply.close, Some(CLOSE_BAD_FRAME), "{frame:?}");
        assert!(!reply.changed);
    }
    assert_eq!(room.text(), "safe");
}

#[test]
fn refuses_an_update_with_a_gap_and_takes_it_after_a_resync() {
    let mut room = Room::create("");
    let client = Client::new();
    let first = client.type_text(0, "a");
    let second = client.type_text(1, "b");
    // The second edit depends on the first, which never arrived.
    assert_eq!(
        room.receive(Role::Edit, false, &second).close,
        Some(CLOSE_BAD_FRAME)
    );
    assert_eq!(room.text(), "");
    assert!(room.receive(Role::Edit, false, &first).changed);
    assert!(room.receive(Role::Edit, false, &second).changed);
    assert_eq!(room.text(), "ab");
}

#[test]
fn takes_the_same_edit_twice_without_changing_anything() {
    let mut room = Room::create("");
    let client = Client::new();
    let frame = client.type_text(0, "once");
    assert!(room.receive(Role::Edit, false, &frame).changed);
    assert!(room.receive(Role::Edit, false, &frame).changed);
    assert_eq!(room.text(), "once");
}

/// Writes updates by hand, including ones no editor would ever send.
#[derive(Default)]
struct Wire(Vec<u8>);

impl Wire {
    fn number(&mut self, mut value: u64) -> &mut Self {
        while value >= 0x80 {
            self.0.push((value as u8 & 0x7f) | 0x80);
            value >>= 7;
        }
        self.0.push(value as u8);
        self
    }

    fn text(&mut self, text: &str) -> &mut Self {
        self.number(text.len() as u64);
        self.0.extend_from_slice(text.as_bytes());
        self
    }

    fn byte(&mut self, byte: u8) -> &mut Self {
        self.0.push(byte);
        self
    }

    fn sent(&self) -> Vec<u8> {
        Message::Sync(SyncMessage::Update(self.0.clone())).encode_v1()
    }
}

/// One client, one string block at clock 0 in the shared field, no deletes.
fn plain_update(field: &str, text: &str) -> Vec<u8> {
    let mut wire = Wire::default();
    wire.number(1).number(1).number(7).number(0);
    wire.byte(4).number(1).text(field).text(text).number(0);
    wire.sent()
}

#[test]
fn accepts_a_handwritten_update_so_the_refusals_below_mean_something() {
    let mut room = Room::create("");
    assert!(
        room.receive(Role::Edit, false, &plain_update("codemirror", "hi"))
            .changed
    );
    assert_eq!(room.text(), "hi");
}

#[test]
fn refuses_anything_a_text_editor_would_never_send() {
    let refused = |name: &str, update: Vec<u8>| {
        let mut room = Room::create("");
        let reply = room.receive(Role::Edit, false, &update);
        assert_eq!(reply.close, Some(CLOSE_BAD_FRAME), "{name}");
        assert_eq!(room.text(), "", "{name}");
    };
    let build = |write: &dyn Fn(&mut Wire)| {
        let mut wire = Wire::default();
        write(&mut wire);
        wire.sent()
    };
    let block_header = |wire: &mut Wire| {
        wire.number(1).number(1).number(7).number(0);
    };

    refused("another field", plain_update("somewhere-else", "hi"));
    refused("empty string", plain_update("codemirror", ""));
    refused(
        "a client id too big for JavaScript",
        build(&|wire| {
            wire.number(1).number(1).number(1 << 53).number(0);
            wire.byte(4)
                .number(1)
                .text("codemirror")
                .text("hi")
                .number(0);
        }),
    );
    refused(
        "a number that never ends",
        build(&|wire| {
            wire.0.extend_from_slice(&[0xff; 16]);
        }),
    );
    refused(
        "a clock at the limit",
        build(&|wire| {
            wire.number(1).number(1).number(7).number(1 << 31);
            wire.byte(4)
                .number(1)
                .text("codemirror")
                .text("hi")
                .number(0);
        }),
    );
    refused(
        "a block that goes past the limit",
        build(&|wire| {
            block_header(wire);
            wire.byte(0).number((1 << 31) + 1).number(0);
        }),
    );
    refused(
        "a parent given by id",
        build(&|wire| {
            block_header(wire);
            wire.byte(4)
                .number(0)
                .number(1)
                .number(0)
                .text("hi")
                .number(0);
        }),
    );
    refused(
        "a map key",
        build(&|wire| {
            block_header(wire);
            wire.byte(4 | 0x20)
                .number(1)
                .text("codemirror")
                .text("key")
                .text("hi")
                .number(0);
        }),
    );
    for content in [2, 3, 5, 6, 7, 8, 9, 10, 31] {
        refused(
            "another kind of content",
            build(&|wire| {
                block_header(wire);
                wire.byte(content)
                    .number(1)
                    .text("codemirror")
                    .number(2)
                    .number(0);
            }),
        );
    }
    refused(
        "text that isn't UTF-8",
        build(&|wire| {
            block_header(wire);
            wire.byte(4)
                .number(1)
                .text("codemirror")
                .number(1)
                .byte(0xff)
                .number(0);
        }),
    );
    refused(
        "a left neighbor nobody has",
        build(&|wire| {
            block_header(wire);
            wire.byte(4 | 0x80).number(9).number(0).text("hi").number(0);
        }),
    );
    refused(
        "a right neighbor nobody has",
        build(&|wire| {
            block_header(wire);
            wire.byte(4 | 0x40).number(9).number(0).text("hi").number(0);
        }),
    );
    refused(
        "a block that points at itself",
        build(&|wire| {
            block_header(wire);
            wire.byte(4 | 0x80).number(7).number(0).text("hi").number(0);
        }),
    );
    refused(
        "two clients waiting on each other",
        build(&|wire| {
            wire.number(2);
            wire.number(1)
                .number(7)
                .number(0)
                .byte(4 | 0x80)
                .number(8)
                .number(0)
                .text("a");
            wire.number(1)
                .number(8)
                .number(0)
                .byte(4 | 0x80)
                .number(7)
                .number(0)
                .text("b");
            wire.number(0);
        }),
    );
    refused(
        "a gap in a client's history",
        build(&|wire| {
            wire.number(1).number(1).number(7).number(5);
            wire.byte(4)
                .number(1)
                .text("codemirror")
                .text("hi")
                .number(0);
        }),
    );
    refused(
        "the same client twice",
        build(&|wire| {
            wire.number(2);
            wire.number(1)
                .number(7)
                .number(0)
                .byte(4)
                .number(1)
                .text("codemirror")
                .text("a");
            wire.number(1)
                .number(7)
                .number(1)
                .byte(4)
                .number(1)
                .text("codemirror")
                .text("b");
            wire.number(0);
        }),
    );
    refused(
        "deleting what doesn't exist",
        build(&|wire| {
            wire.number(0)
                .number(1)
                .number(7)
                .number(1)
                .number(0)
                .number(1);
        }),
    );
    refused(
        "deleting nothing",
        build(&|wire| {
            block_header(wire);
            wire.byte(4).number(1).text("codemirror").text("hi");
            wire.number(1).number(7).number(1).number(0).number(0);
        }),
    );
    refused(
        "trailing bytes",
        build(&|wire| {
            block_header(wire);
            wire.byte(4)
                .number(1)
                .text("codemirror")
                .text("hi")
                .number(0)
                .byte(0);
        }),
    );
}

#[test]
fn accepts_what_editors_do_send() {
    let mut room = Room::create("");
    let client = Client::new();
    let frame = client.type_text(0, "hello 🪱");
    assert!(room.receive(Role::Edit, false, &frame).changed);
    // Deletes, and a block between two others.
    let text = client.doc.get_or_insert_text("codemirror");
    let removed = {
        let mut txn = client.doc.transact_mut();
        text.remove_range(&mut txn, 0, 2);
        Message::Sync(SyncMessage::Update(txn.encode_update_v1())).encode_v1()
    };
    assert!(room.receive(Role::Edit, false, &removed).changed);
    assert!(
        room.receive(Role::Edit, false, &client.type_text(1, "x"))
            .changed
    );
    assert_eq!(room.text(), client.text());
    // A whole document, the way a tab sends it when it reconnects, with the
    // client's garbage-collected history.
    let mut empty = Room::create("");
    let state = client.answer(&empty.sync_request());
    assert!(empty.receive(Role::Edit, false, &state).changed);
    assert_eq!(empty.text(), client.text());
}

/// Valid-looking updates with references that are random, crossed or missing.
fn crafted_update(next: &mut impl FnMut() -> u64) -> Vec<u8> {
    let mut wire = Wire::default();
    let clients = 1 + next() % 3;
    wire.number(clients);
    for _ in 0..clients {
        let blocks = next() % 4;
        wire.number(blocks).number(next() % 4).number(next() % 12);
        for _ in 0..blocks {
            match next() % 8 {
                0 => {
                    wire.byte(0).number(1 + next() % 8);
                }
                1 => {
                    wire.byte(1)
                        .number(1)
                        .text("codemirror")
                        .number(1 + next() % 8);
                }
                2 => {
                    wire.byte(1 | 0x80)
                        .number(next() % 4)
                        .number(next() % 12)
                        .number(1 + next() % 8);
                }
                3 => {
                    wire.byte(4).number(1).text("codemirror").text("🪱é");
                }
                4 => {
                    wire.byte(4 | 0x40)
                        .number(next() % 4)
                        .number(next() % 12)
                        .text("right");
                }
                5 => {
                    wire.byte(4 | 0x80 | 0x40);
                    wire.number(next() % 4).number(next() % 12);
                    wire.number(next() % 4).number(next() % 12);
                    wire.text("both");
                }
                _ => {
                    wire.byte(4 | 0x80)
                        .number(next() % 4)
                        .number(next() % 12)
                        .text("left");
                }
            }
        }
    }
    let delete_clients = next() % 3;
    wire.number(delete_clients);
    for _ in 0..delete_clients {
        wire.number(next() % 4)
            .number(1)
            .number(next() % 12)
            .number(1 + next() % 4);
    }
    wire.sent()
}

/// How many frames to throw at the room. Raise it for a longer run.
fn fuzz_rounds() -> u64 {
    std::env::var("MAGI_FUZZ_ROUNDS")
        .ok()
        .and_then(|rounds| rounds.parse().ok())
        .unwrap_or(20_000)
}

/// Anyone with the link can send anything. `yrs` alone panics or crashes on
/// this input, so every frame has to stop at the check in front of it.
#[test]
fn survives_random_damaged_and_crafted_frames() {
    let client = Client::new();
    let mut seeds = vec![
        client.type_text(0, "hello there"),
        client.type_text(3, "xx"),
    ];
    let text = client.doc.get_or_insert_text("codemirror");
    {
        let mut txn = client.doc.transact_mut();
        text.remove_range(&mut txn, 1, 4);
        seeds.push(Message::Sync(SyncMessage::Update(txn.encode_update_v1())).encode_v1());
    }
    seeds.push(client.answer(&Client::new().sync_step_1()));

    let mut seed = 0x2545_F491_4F6C_DD1D_u64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let mut room = Room::create("safe");
    let mut changed = 0u64;
    for round in 0..fuzz_rounds() {
        let base = &seeds[next() as usize % seeds.len()];
        let mut frame = base.clone();
        match next() % 5 {
            0 => {
                let spot = 2 + next() as usize % (frame.len() - 2);
                frame[spot] = next() as u8;
            }
            1 => {
                for _ in 0..3 {
                    let spot = 2 + next() as usize % (frame.len() - 2);
                    frame[spot] = next() as u8;
                }
            }
            2 => frame.truncate(1 + next() as usize % (frame.len() - 1)),
            3 => {
                let length = next() as usize % 48;
                frame = vec![0, next() as u8 % 3, length as u8];
                frame.extend((0..length).map(|_| next() as u8));
            }
            _ => frame = crafted_update(&mut next),
        }
        if room.receive(Role::Edit, false, &frame).changed {
            changed += 1;
            room.text();
            room.save();
        }
        // A fresh room now and then, so references stay likely to line up.
        if round % 200 == 0 {
            room = Room::load(&room.save()).expect("what a room saved always loads");
        }
        if round % 2000 == 0 {
            room = Room::create("safe");
        }
    }
    // A fuzz run that gets nothing through proves nothing.
    assert!(
        changed * 100 > fuzz_rounds(),
        "only {changed} frames got through"
    );
    println!("{} frames, {changed} changed the room", fuzz_rounds());
}

#[test]
fn refuses_to_grow_past_the_cap_and_says_so() {
    let mut room = Room::create("");
    let client = Client::new();
    let chunk = "a".repeat(400 * 1024);
    let mut index = 0;
    for _ in 0..2 {
        assert!(
            room.receive(Role::Edit, false, &client.type_text(index, &chunk))
                .changed
        );
        index += chunk.len() as u32;
    }
    let reply = room.receive(Role::Edit, false, &client.type_text(index, &chunk));
    assert_eq!(reply.to_sender, vec![notice(NOTICE_FULL)]);
    assert!(!reply.changed);
    assert!(room.save().len() <= MAX_STATE_BYTES);
    assert_eq!(room.text().len(), 800 * 1024);
}

#[test]
fn an_edit_that_only_looks_too_big_still_goes_in() {
    let mut room = Room::create("");
    let client = Client::new();
    // Many small edits overstate the size: each one is more bytes alone than it
    // is merged into the saved document.
    let mut index = 0;
    for _ in 0..30_000 {
        room.receive(
            Role::Edit,
            false,
            &client.type_text(index, "aaaaaaaaaaaaaaaaaaaa"),
        );
        index += 20;
    }
    let before = room.text().len();
    let reply = room.receive(Role::Edit, false, &client.type_text(index, "more"));
    assert!(reply.changed);
    assert_eq!(room.text().len(), before + 4);
}

#[test]
fn replacing_the_text_reaches_open_tabs_and_saves() {
    let mut room = Room::create("old text");
    let client = Client::new();
    client.sync_with(&mut room);
    client.hear(&room.replace_text("restored"));
    assert_eq!(client.text(), "restored");
    assert_eq!(room.text(), "restored");
    assert_eq!(saved(&mut room).text(), "restored");
}

#[test]
fn refuses_to_load_garbage() {
    assert!(Room::load(&[255, 255, 255]).is_err());
    assert!(
        Room::load(
            &Doc::new()
                .transact()
                .encode_state_as_update_v1(&StateVector::default())
        )
        .is_ok()
    );
}

#[test]
fn slows_down_a_flood_and_recovers() {
    let mut bucket = Bucket::new(1000);
    for _ in 0..300 {
        assert!(bucket.allow(1000));
    }
    assert!(!bucket.allow(1000));
    // 30 frames per second come back.
    assert!(bucket.allow(1100));
    assert!(bucket.allow(1100));
    assert!(bucket.allow(1100));
    assert!(!bucket.allow(1100));
    // It never saves up more than a full bucket.
    let mut rested = Bucket::new(0);
    assert!(rested.allow(u64::from(u32::MAX)));
    assert_eq!(rested, {
        let mut full = Bucket::new(u64::from(u32::MAX));
        assert!(full.allow(u64::from(u32::MAX)));
        full
    });
    // A clock that jumps back is harmless.
    let mut back = Bucket::new(5000);
    assert!(back.allow(0));
    assert!(back.allow(5000));
}

#[test]
fn knows_when_to_snapshot_and_when_to_expire() {
    assert!(!snapshot_due(1000, 1000 + SNAPSHOT_EVERY_MS - 1));
    assert!(snapshot_due(1000, 1000 + SNAPSHOT_EVERY_MS));
    assert!(!snapshot_due(5000, 1000));
    assert_eq!(expires_at(5), 5 + IDLE_LIFETIME_MS);
    assert_eq!(expires_at(u64::MAX), u64::MAX);
}

#[test]
fn names_roles_and_frames() {
    assert_eq!(Role::parse(Role::Edit.as_str()), Role::Edit);
    assert_eq!(Role::parse(Role::View.as_str()), Role::View);
    assert_eq!(Role::parse("anything else"), Role::View);
    assert_eq!(awareness_query(), [3]);
    assert_eq!(notice(1), [4, 1]);
}

/// Two clients that each need the other's last block, over and over, so the
/// check moves one block per pass, with `waiting` more clients stuck behind them.
fn tangled_update(chain: u64, waiting: u64) -> Vec<u8> {
    const WITH_ORIGIN: u8 = 4 | 0x80;
    let mut wire = Wire::default();
    wire.number(2 + waiting);
    wire.number(chain).number(1).number(0);
    wire.byte(4).number(1).text("codemirror").text("x");
    for clock in 1..chain {
        wire.byte(WITH_ORIGIN).number(2).number(clock - 1).text("x");
    }
    wire.number(chain).number(2).number(0);
    for clock in 0..chain {
        wire.byte(WITH_ORIGIN).number(1).number(clock).text("x");
    }
    for client in 0..waiting {
        wire.number(1).number(client + 3).number(0);
        wire.byte(WITH_ORIGIN).number(2).number(chain - 1).text("x");
    }
    wire.number(0);
    wire.sent()
}

#[test]
fn closes_the_socket_of_whoever_asks_with_a_broken_state_vector() {
    let mut room = Room::create("text");
    let frame = [0, 0, 1, 255];
    assert_eq!(
        room.receive(Role::Edit, false, &frame).close,
        Some(CLOSE_BAD_FRAME)
    );
}

#[test]
fn only_reads_sync_frames_as_sync_frames() {
    let frame = Client::new().sync_step_1();
    assert!(parse_sync_frame(&frame).is_some());
    let mut other = frame.clone();
    other[0] = 1;
    assert!(parse_sync_frame(&other).is_none());
    assert!(parse_sync_frame(&[]).is_none());
}

#[test]
fn takes_any_order_of_dependencies_however_long() {
    let mut room = Room::create("");
    let frame = tangled_update(5_000, 0);
    assert!(room.receive(Role::Edit, false, &frame).changed);
    assert_eq!(room.text().len(), 10_000);
}

#[test]
fn a_frame_built_to_keep_the_check_busy_stays_cheap() {
    let frame = tangled_update(4_000, 80_000);
    let Some(SyncFrame::Update(update)) = parse_sync_frame(&frame) else {
        panic!("expected an update");
    };
    let started = Instant::now();
    assert!(is_acceptable_update(update, "codemirror", &HashMap::new()));
    // This took 13 s in release mode before the check stopped going round in
    // passes, so 5 s in any build leaves room for a slow machine.
    assert!(started.elapsed() < Duration::from_secs(5));
}

/// Random blocks that lean on each other, and the slow, obvious way to tell if
/// they all fit: keep going round until nothing moves.
#[test]
fn agrees_with_going_round_until_nothing_moves() {
    const WITH_ORIGIN: u8 = 4 | 0x80;
    let seed = Cell::new(0x9E37_79B9_7F4A_7C15_u64);
    let next = |below: u64| {
        let mut value = seed.get();
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        seed.set(value);
        value % below
    };
    let (mut accepted, mut refused) = (0, 0);
    for _ in 0..20_000 {
        let known: HashMap<u64, u64> = (1..=4)
            .filter(|_| next(3) == 0)
            .map(|client| (client, 1 + next(3)))
            .collect();
        let mut sections = Vec::new();
        for client in (1..=4).filter(|_| next(4) != 0) {
            let at_start = known.get(&client).copied().unwrap_or(0);
            let start = if next(16) == 0 {
                at_start + 1
            } else {
                at_start
            };
            let origins: Vec<Option<(u64, u64)>> = (0..1 + next(4))
                .map(|_| (next(2) == 0).then(|| (1 + next(4), next(2))))
                .collect();
            sections.push((client, start, origins));
        }

        let mut wire = Wire::default();
        wire.number(sections.len() as u64);
        for (client, start, origins) in &sections {
            wire.number(origins.len() as u64)
                .number(*client)
                .number(*start);
            for origin in origins {
                match origin {
                    Some((client, clock)) => {
                        wire.byte(WITH_ORIGIN).number(*client).number(*clock);
                    }
                    None => {
                        wire.byte(4).number(1).text("codemirror");
                    }
                }
                wire.text("x");
            }
        }
        wire.number(0);
        let frame = wire.sent();
        let Some(SyncFrame::Update(update)) = parse_sync_frame(&frame) else {
            panic!("expected an update");
        };

        let mut reached = known.clone();
        let mut done = vec![0; sections.len()];
        loop {
            let mut moved = false;
            for (index, (client, start, origins)) in sections.iter().enumerate() {
                while let Some(origin) = origins.get(done[index]) {
                    let clock = start + done[index] as u64;
                    let here = reached.get(client).copied().unwrap_or(0);
                    let origin_known =
                        origin.is_none_or(|(of, at)| at < reached.get(&of).copied().unwrap_or(0));
                    if clock > here || !origin_known {
                        break;
                    }
                    reached.insert(*client, here.max(clock + 1));
                    done[index] += 1;
                    moved = true;
                }
            }
            if !moved {
                break;
            }
        }
        let fits = sections
            .iter()
            .zip(&done)
            .all(|((_, _, origins), done)| origins.len() == *done);

        assert_eq!(
            is_acceptable_update(update, "codemirror", &known),
            fits,
            "{sections:?} with {known:?}"
        );
        if fits {
            accepted += 1;
        } else {
            refused += 1;
        }
    }
    assert!(
        accepted > 1_000 && refused > 1_000,
        "{accepted} / {refused}"
    );
}

mod exact_limits {
    use super::*;

    const LAST_CLOCK: u64 = 1 << 31;

    fn acceptable(wire: &Wire, known: &[(u64, u64)]) -> bool {
        let frame = wire.sent();
        let Some(SyncFrame::Update(update)) = parse_sync_frame(&frame) else {
            panic!("expected an update");
        };
        is_acceptable_update(update, "codemirror", &known.iter().copied().collect())
    }

    #[test]
    fn an_edit_that_exactly_fills_the_room_goes_in() {
        let mut room = Room::create("");
        let empty = room.save().len();
        let update_size = |frame: &[u8]| {
            let Some(SyncFrame::Update(update)) = parse_sync_frame(frame) else {
                panic!("expected an update");
            };
            update.len()
        };
        // The text's length takes the same 3 bytes at any size around here.
        let sample = 100_000;
        let around = "a".repeat(sample);
        let overhead = update_size(&plain_update("codemirror", &around)) - sample;
        let room_for_text = MAX_STATE_BYTES - empty - overhead;

        let exact = plain_update("codemirror", &"a".repeat(room_for_text));
        assert_eq!(empty + update_size(&exact), MAX_STATE_BYTES);
        assert!(room.receive(Role::Edit, false, &exact).changed);

        let mut full = Room::create("");
        full.save();
        let over = plain_update("codemirror", &"a".repeat(room_for_text + 1));
        assert_eq!(
            full.receive(Role::Edit, false, &over).to_sender,
            vec![notice(NOTICE_FULL)]
        );
    }

    #[test]
    fn the_real_size_decides_when_the_estimate_is_over() {
        let client = Client::new();
        let edits: Vec<_> = (0..20).map(|at| client.type_text(at, "a")).collect();
        let mut learned = Room::create("");
        let mut room = Room::create("");
        for edit in &edits {
            learned.receive(Role::Edit, false, edit);
            assert!(room.receive(Role::Edit, false, edit).changed);
        }
        let real_size = learned.save().len();

        let sample = 100_000;
        let frame = plain_update("codemirror", &"a".repeat(sample));
        let Some(SyncFrame::Update(update)) = parse_sync_frame(&frame) else {
            panic!("expected an update");
        };
        let overhead = update.len() - sample;
        let text = "a".repeat(MAX_STATE_BYTES - real_size - overhead);
        let exact = plain_update("codemirror", &text);
        assert!(room.receive(Role::Edit, false, &exact).changed);
    }

    #[test]
    fn a_block_can_end_on_the_last_clock_but_an_origin_cannot_be_there() {
        let mut ends_there = Wire::default();
        ends_there
            .number(1)
            .number(1)
            .number(7)
            .number(LAST_CLOCK - 1);
        ends_there
            .byte(4)
            .number(1)
            .text("codemirror")
            .text("x")
            .number(0);
        assert!(acceptable(&ends_there, &[(7, LAST_CLOCK - 1)]));

        let origin = |clock: u64| {
            let mut wire = Wire::default();
            wire.number(1).number(1).number(7).number(0);
            wire.byte(4 | 0x80).number(8).number(clock).text("x");
            wire.number(0);
            wire
        };
        let known = [(8, LAST_CLOCK + 1)];
        assert!(acceptable(&origin(LAST_CLOCK - 1), &known));
        assert!(!acceptable(&origin(LAST_CLOCK), &known));
    }

    #[test]
    fn a_delete_can_reach_the_end_of_what_is_known_and_no_further() {
        let delete = |length: u64| {
            let mut wire = Wire::default();
            wire.number(0).number(1).number(7).number(1);
            wire.number(1).number(length);
            wire
        };
        assert!(acceptable(&delete(2), &[(7, 3)]));
        assert!(!acceptable(&delete(3), &[(7, 3)]));
    }
}

#[test]
fn a_room_lasts_30_days_without_edits() {
    assert_eq!(IDLE_LIFETIME_MS, 30 * 86_400_000);
}
