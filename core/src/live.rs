//! Live pastes: one shared Yjs document per room, spoken over WebSockets with
//! the standard y-protocols sync messages. Nothing here knows about Cloudflare.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use yrs::sync::{Message, SyncMessage};
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;
use yrs::{Doc, GetString, ReadTxn, StateVector, Text, TextRef, Transact, Update};

use crate::live_frames::{SyncFrame, is_acceptable_update, parse_sync_frame};

/// The field the browser editor binds to.
const TEXT_FIELD: &str = "codemirror";

const FRAME_SYNC: u8 = 0;
const FRAME_AWARENESS: u8 = 1;
const FRAME_AWARENESS_QUERY: u8 = 3;
/// Our own message type: a one-byte notice for the page.
const FRAME_NOTICE: u8 = 4;

pub const NOTICE_LOCKED: u8 = 1;
pub const NOTICE_UNLOCKED: u8 = 2;
pub const NOTICE_FULL: u8 = 3;

/// Saved document size, deleted text included. Leaves room for edit history on
/// top of the same 512 KB a normal paste can hold.
pub const MAX_STATE_BYTES: usize = 2 * 512 * 1024;
/// Cursors and names are small. Anything bigger isn't one.
pub const MAX_AWARENESS_BYTES: usize = 4096;
pub const MAX_SOCKETS: usize = 50;
pub const MAX_SNAPSHOTS: u32 = 20;
/// Edits reach storage at most this often.
pub const SAVE_DELAY_MS: u64 = 2_000;
pub const SNAPSHOT_EVERY_MS: u64 = 300_000;
/// A room with no edits for this long deletes itself.
pub const IDLE_LIFETIME_MS: u64 = 30 * 24 * 60 * 60 * 1000;

/// Close code for a socket that sends too fast. Its client reconnects and syncs
/// again, so nothing it typed gets lost.
pub const CLOSE_TOO_FAST: u16 = 4429;
/// Close code for a frame the room can't use.
pub const CLOSE_BAD_FRAME: u16 = 4400;
/// Close code for a room that no longer exists.
pub const CLOSE_GONE: u16 = 4404;

/// What the server can ask of a room, besides opening a socket.
pub const ROOM_INIT: &str = "/init";
pub const ROOM_WIPE: &str = "/wipe";
pub const ROOM_LOCK: &str = "/lock";
pub const ROOM_UNLOCK: &str = "/unlock";
pub const ROOM_SNAPSHOTS: &str = "/snapshots";
pub const ROOM_RESTORE: &str = "/restore/";
pub const ROOM_TEXT: &str = "/text";
pub const ROOM_SOCKET: &str = "/socket";

/// How a room starts. It keeps the hashes so it can delete itself from KV.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomInit {
    pub text: String,
    pub link_hash: String,
    pub view_hash: String,
}

const BUCKET_SIZE: f64 = 300.0;
const BUCKET_REFILL_PER_MS: f64 = 0.03;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Edit,
    View,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Edit => "edit",
            Self::View => "view",
        }
    }

    pub fn parse(text: &str) -> Self {
        if text == "edit" {
            Self::Edit
        } else {
            Self::View
        }
    }
}

/// What to do after a socket sent a frame.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reply {
    pub to_sender: Vec<Vec<u8>>,
    pub to_others: Vec<Vec<u8>>,
    /// The document changed and has to be saved.
    pub changed: bool,
    /// Close this socket with this code.
    pub close: Option<u16>,
}

impl Reply {
    /// The frame can't be used. Its sender reconnects, syncs again and loses
    /// nothing, while a sender that wrote it on purpose gets nowhere.
    fn refused() -> Self {
        Self {
            close: Some(CLOSE_BAD_FRAME),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomError(pub String);

impl RoomError {
    fn from_cause(cause: impl std::fmt::Display) -> Self {
        Self(cause.to_string())
    }
}

pub struct Room {
    doc: Doc,
    text: TextRef,
    /// Size of the last encoded state plus the updates applied since.
    size: usize,
}

pub fn notice(code: u8) -> Vec<u8> {
    vec![FRAME_NOTICE, code]
}

/// Asks everyone else to send their cursor again, for whoever just joined.
pub fn awareness_query() -> Vec<u8> {
    vec![FRAME_AWARENESS_QUERY]
}

impl Room {
    fn empty() -> Self {
        let doc = Doc::new();
        let text = doc.get_or_insert_text(TEXT_FIELD);
        Self { doc, text, size: 0 }
    }

    /// A brand new room holding `initial`.
    pub fn create(initial: &str) -> Self {
        let room = Self::empty();
        room.text.insert(&mut room.doc.transact_mut(), 0, initial);
        room
    }

    /// A room as it was saved.
    pub fn load(state: &[u8]) -> Result<Self, RoomError> {
        let mut room = Self::empty();
        let update = Update::decode_v1(state).map_err(RoomError::from_cause)?;
        room.doc
            .transact_mut()
            .apply_update(update)
            .map_err(RoomError::from_cause)?;
        room.size = state.len();
        Ok(room)
    }

    /// Everything needed to rebuild the room. Also resets the size estimate,
    /// since the estimate only ever goes up between saves.
    pub fn save(&mut self) -> Vec<u8> {
        let state = self
            .doc
            .transact()
            .encode_state_as_update_v1(&StateVector::default());
        self.size = state.len();
        state
    }

    pub fn text(&self) -> String {
        self.text.get_string(&self.doc.transact())
    }

    /// "Tell me what you have that I don't": the first thing a socket hears,
    /// and how a room that lost its memory gets it back from open sockets.
    pub fn sync_request(&self) -> Vec<u8> {
        let state_vector = self.doc.transact().state_vector();
        Message::Sync(SyncMessage::SyncStep1(state_vector)).encode_v1()
    }

    /// Replaces the whole text as one edit, so open editors just follow along.
    /// Returns the frame to send to everyone.
    pub fn replace_text(&mut self, text: &str) -> Vec<u8> {
        let update = {
            let mut txn = self.doc.transact_mut();
            let length = self.text.len(&txn);
            self.text.remove_range(&mut txn, 0, length);
            self.text.insert(&mut txn, 0, text);
            txn.encode_update_v1()
        };
        self.size = self.size.saturating_add(update.len());
        Message::Sync(SyncMessage::Update(update)).encode_v1()
    }

    pub fn receive(&mut self, role: Role, locked: bool, frame: &[u8]) -> Reply {
        match frame.first() {
            Some(&FRAME_SYNC) => self.receive_sync(role, locked, frame),
            // Viewers stay invisible, and their cursors go nowhere.
            Some(&FRAME_AWARENESS) if role == Role::Edit && frame.len() <= MAX_AWARENESS_BYTES => {
                Reply {
                    to_others: vec![frame.to_vec()],
                    ..Reply::default()
                }
            }
            _ => Reply::default(),
        }
    }

    fn receive_sync(&mut self, role: Role, locked: bool, frame: &[u8]) -> Reply {
        match parse_sync_frame(frame) {
            None => Reply::refused(),
            Some(SyncFrame::Request(state_vector)) => {
                let missing = self.doc.transact().encode_diff_v1(&state_vector);
                Reply {
                    to_sender: vec![Message::Sync(SyncMessage::SyncStep2(missing)).encode_v1()],
                    ..Reply::default()
                }
            }
            Some(SyncFrame::Update(update)) if role == Role::Edit && !locked => self.apply(update),
            Some(SyncFrame::Update(_)) => Reply::default(),
        }
    }

    fn known_clocks(&self) -> HashMap<u64, u64> {
        self.doc
            .transact()
            .state_vector()
            .iter()
            .map(|(client, clock)| (client.get(), u64::from(*clock)))
            .collect()
    }

    fn apply(&mut self, bytes: &[u8]) -> Reply {
        if self.size.saturating_add(bytes.len()) > MAX_STATE_BYTES {
            // The estimate might be high: only the real size can refuse an edit.
            self.save();
            if self.size.saturating_add(bytes.len()) > MAX_STATE_BYTES {
                return Reply {
                    to_sender: vec![notice(NOTICE_FULL)],
                    ..Reply::default()
                };
            }
        }
        let applied = is_acceptable_update(bytes, TEXT_FIELD, &self.known_clocks())
            && Update::decode_v1(bytes)
                .is_ok_and(|update| self.doc.transact_mut().apply_update(update).is_ok());
        if !applied {
            return Reply::refused();
        }
        self.size = self.size.saturating_add(bytes.len());
        Reply {
            to_others: vec![Message::Sync(SyncMessage::Update(bytes.to_vec())).encode_v1()],
            changed: true,
            ..Reply::default()
        }
    }
}

/// Keeps one socket from flooding a room.
#[derive(Debug, Clone, PartialEq)]
pub struct Bucket {
    tokens: f64,
    last_ms: u64,
}

impl Bucket {
    pub fn new(now_ms: u64) -> Self {
        Self {
            tokens: BUCKET_SIZE,
            last_ms: now_ms,
        }
    }

    /// Takes one frame's worth. `false` means this socket is going too fast.
    pub fn allow(&mut self, now_ms: u64) -> bool {
        let elapsed = now_ms.saturating_sub(self.last_ms) as f64;
        self.tokens = (self.tokens + elapsed * BUCKET_REFILL_PER_MS).min(BUCKET_SIZE);
        self.last_ms = now_ms;
        if self.tokens < 1.0 {
            return false;
        }
        self.tokens -= 1.0;
        true
    }
}

pub fn snapshot_due(last_snapshot_ms: u64, now_ms: u64) -> bool {
    now_ms.saturating_sub(last_snapshot_ms) >= SNAPSHOT_EVERY_MS
}

pub fn expires_at(last_edit_ms: u64) -> u64 {
    last_edit_ms.saturating_add(IDLE_LIFETIME_MS)
}
