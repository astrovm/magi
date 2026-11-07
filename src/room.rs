//! The Durable Object behind a live paste. It hibernates between messages, so
//! memory can vanish at any time: storage is the truth, and open sockets help
//! fill in whatever hadn't been saved yet.

use std::cell::RefCell;
use std::collections::HashMap;

use magi_core::live::{
    Bucket, CLOSE_GONE, CLOSE_TOO_FAST, MAX_SNAPSHOTS, MAX_SOCKETS, NOTICE_LOCKED, NOTICE_UNLOCKED,
    ROOM_INIT, ROOM_LOCK, ROOM_RESTORE, ROOM_SNAPSHOTS, ROOM_SOCKET, ROOM_TEXT, ROOM_UNLOCK,
    ROOM_WIPE, Role, Room, RoomInit, SAVE_DELAY_MS, awareness_query, expires_at, notice,
    snapshot_due,
};
use magi_core::store::clicks_key;
use serde::Deserialize;
use worker::{
    DurableObject, Env, Error, Request, Response, Result, SqlStorageValue, State, WebSocket,
    WebSocketIncomingMessage, WebSocketPair, durable_object,
};

const CREATE_TABLES: &str = "
CREATE TABLE IF NOT EXISTS room (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  state BLOB NOT NULL,
  link_hash TEXT NOT NULL,
  view_hash TEXT NOT NULL,
  locked INTEGER NOT NULL,
  last_edit INTEGER NOT NULL,
  last_snapshot INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS snapshots (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  at INTEGER NOT NULL,
  text TEXT NOT NULL
);";

/// Characters of a snapshot shown in its list.
const PREVIEW_CHARS: usize = 80;

struct Loaded {
    room: Room,
    locked: bool,
    /// Holds edits storage doesn't have yet.
    dirty: bool,
    /// A save is already on its way.
    save_scheduled: bool,
    buckets: HashMap<String, Bucket>,
}

#[derive(Deserialize)]
struct Hashes {
    link_hash: String,
    view_hash: String,
}

#[derive(Deserialize)]
struct Times {
    last_edit: f64,
    last_snapshot: f64,
}

#[derive(Deserialize, serde::Serialize)]
struct Snapshot {
    id: f64,
    at: f64,
    preview: String,
}

#[derive(Deserialize)]
struct SnapshotText {
    text: String,
}

fn now_ms() -> u64 {
    js_sys::Date::now() as u64
}

fn number(value: u64) -> SqlStorageValue {
    SqlStorageValue::Integer(i64::try_from(value).unwrap_or(i64::MAX))
}

fn log_error(context: &str, error: &str) {
    let line = serde_json::json!({ "level": "error", "context": context, "error": error });
    worker::console_error!("{line}");
}

fn send(socket: &WebSocket, frame: &[u8]) {
    // A socket that just went away isn't a problem: it gets its close event.
    if let Err(error) = socket.send_with_bytes(frame) {
        log_error("room send", &error.to_string());
    }
}

#[durable_object]
pub struct LiveRoom {
    state: State,
    env: Env,
    loaded: RefCell<Option<Loaded>>,
}

impl LiveRoom {
    fn has_tables(&self) -> Result<bool> {
        #[derive(Deserialize)]
        struct Count {
            found: f64,
        }
        let row: Count = self
            .state
            .storage()
            .sql()
            .exec(
                "SELECT COUNT(*) AS found FROM sqlite_master WHERE type = 'table' AND name = 'room'",
                None,
            )?
            .one()?;
        Ok(row.found > 0.0)
    }

    /// Brings the room into memory. `None` if it doesn't exist; otherwise,
    /// whether memory was empty and may have lost something.
    fn ensure_loaded(&self) -> Result<Option<bool>> {
        if self.loaded.borrow().is_some() {
            return Ok(Some(false));
        }
        if !self.has_tables()? {
            return Ok(None);
        }
        let sql = self.state.storage().sql();
        let row = sql
            .exec("SELECT state, locked FROM room", None)?
            .raw()
            .next()
            .transpose()?;
        let Some(row) = row else {
            return Ok(None);
        };
        let (Some(SqlStorageValue::Blob(state)), Some(locked)) = (row.first(), row.get(1)) else {
            return Err(Error::RustError("room row has the wrong shape".to_owned()));
        };
        let room = Room::load(state).map_err(|error| Error::RustError(error.0))?;
        *self.loaded.borrow_mut() = Some(Loaded {
            room,
            locked: matches!(
                locked,
                SqlStorageValue::Integer(1) | SqlStorageValue::Float(1.0)
            ),
            dirty: false,
            save_scheduled: false,
            buckets: HashMap::new(),
        });
        Ok(Some(true))
    }

    fn broadcast(&self, frame: &[u8], except: Option<&WebSocket>) {
        for socket in self.state.get_websockets() {
            if except != Some(&socket) {
                send(&socket, frame);
            }
        }
    }

    fn snapshot_now(&self, text: &str, at: u64) -> Result<()> {
        let sql = self.state.storage().sql();
        sql.exec(
            "INSERT INTO snapshots (at, text) VALUES (?, ?)",
            vec![number(at), SqlStorageValue::String(text.to_owned())],
        )?;
        sql.exec(
            "DELETE FROM snapshots WHERE id NOT IN (SELECT id FROM snapshots ORDER BY id DESC LIMIT ?)",
            vec![number(u64::from(MAX_SNAPSHOTS))],
        )?;
        sql.exec("UPDATE room SET last_snapshot = ?", vec![number(at)])?;
        Ok(())
    }

    /// Writes pending edits to storage, if there are any.
    fn save(&self) -> Result<()> {
        let mut loaded = self.loaded.borrow_mut();
        let Some(loaded) = loaded.as_mut().filter(|loaded| loaded.dirty) else {
            return Ok(());
        };
        let now = now_ms();
        let state = loaded.room.save();
        let sql = self.state.storage().sql();
        sql.exec(
            "UPDATE room SET state = ?, last_edit = ?",
            vec![SqlStorageValue::Blob(state), number(now)],
        )?;
        let times: Times = sql
            .exec("SELECT last_edit, last_snapshot FROM room", None)?
            .one()?;
        if snapshot_due(times.last_snapshot as u64, now) {
            self.snapshot_now(&loaded.room.text(), now)?;
        }
        loaded.dirty = false;
        Ok(())
    }

    /// Next wake-up: a save soon, or the end of the room's life if idle.
    async fn schedule_save(&self) -> Result<()> {
        let schedule = {
            let mut loaded = self.loaded.borrow_mut();
            loaded.as_mut().is_some_and(|loaded| {
                let schedule = !loaded.save_scheduled;
                loaded.save_scheduled = true;
                loaded.dirty = true;
                schedule
            })
        };
        if schedule {
            let delay = i64::try_from(SAVE_DELAY_MS).unwrap_or(i64::MAX);
            self.state.storage().set_alarm(delay).await?;
        }
        Ok(())
    }

    async fn schedule_expiry(&self, last_edit: u64) -> Result<()> {
        let delay = expires_at(last_edit).saturating_sub(now_ms());
        self.state
            .storage()
            .set_alarm(i64::try_from(delay).unwrap_or(i64::MAX))
            .await
    }

    /// Removes the room: sockets, memory and storage.
    async fn wipe(&self) -> Result<()> {
        for socket in self.state.get_websockets() {
            if let Err(error) = socket.close(Some(CLOSE_GONE), Some("gone")) {
                log_error("room close", &error.to_string());
            }
        }
        *self.loaded.borrow_mut() = None;
        let storage = self.state.storage();
        storage.delete_alarm().await?;
        storage.delete_all().await
    }

    /// The room outlived its 30 days: its aliases go too.
    async fn expire(&self) -> Result<()> {
        let hashes: Hashes = self
            .state
            .storage()
            .sql()
            .exec("SELECT link_hash, view_hash FROM room", None)?
            .one()?;
        let links = self.env.kv("links")?;
        for key in [
            hashes.link_hash.clone(),
            clicks_key(&hashes.link_hash),
            hashes.view_hash.clone(),
            clicks_key(&hashes.view_hash),
        ] {
            links.delete(&key).await?;
        }
        self.wipe().await
    }

    async fn init(&self, body: &str) -> Result<Response> {
        let init: RoomInit = serde_json::from_str(body)?;
        let sql = self.state.storage().sql();
        sql.exec(CREATE_TABLES, None)?;
        let mut room = Room::create(&init.text);
        let now = now_ms();
        sql.exec(
            "INSERT INTO room (id, state, link_hash, view_hash, locked, last_edit, last_snapshot) VALUES (1, ?, ?, ?, 0, ?, 0)",
            vec![
                SqlStorageValue::Blob(room.save()),
                SqlStorageValue::String(init.link_hash),
                SqlStorageValue::String(init.view_hash),
                number(now),
            ],
        )?;
        self.schedule_expiry(now).await?;
        Response::ok("")
    }

    fn accept_socket(&self, role: Role) -> Result<Response> {
        if self.state.get_websockets().len() >= MAX_SOCKETS {
            return Response::error("full", 429);
        }
        let Some(fresh) = self.ensure_loaded()? else {
            return Response::error("gone", 404);
        };
        let pair = WebSocketPair::new()?;
        let id = format!("socket:{}", js_sys::Math::random());
        self.state
            .accept_websocket_with_tags(&pair.server, &[role.as_str(), &id]);
        let loaded = self.loaded.borrow();
        if let Some(loaded) = loaded.as_ref() {
            let request = loaded.room.sync_request();
            send(&pair.server, &request);
            if loaded.locked {
                send(&pair.server, &notice(NOTICE_LOCKED));
            }
            if fresh {
                self.broadcast(&request, Some(&pair.server));
            }
        }
        self.broadcast(&awareness_query(), Some(&pair.server));
        Response::from_websocket(pair.client)
    }

    fn set_locked(&self, locked: bool) -> Result<Response> {
        if self.ensure_loaded()?.is_none() {
            return Response::error("gone", 404);
        }
        self.state.storage().sql().exec(
            "UPDATE room SET locked = ?",
            vec![number(u64::from(locked))],
        )?;
        let mut loaded = self.loaded.borrow_mut();
        if let Some(loaded) = loaded.as_mut() {
            loaded.locked = locked;
            self.broadcast(
                &notice(if locked {
                    NOTICE_LOCKED
                } else {
                    NOTICE_UNLOCKED
                }),
                None,
            );
            // Whatever got refused while locked comes back now.
            if !locked {
                self.broadcast(&loaded.room.sync_request(), None);
            }
        }
        Response::ok("")
    }

    fn snapshots(&self) -> Result<Response> {
        if !self.has_tables()? {
            return Response::error("gone", 404);
        }
        self.save()?;
        let query = format!(
            "SELECT id, at, substr(text, 1, {PREVIEW_CHARS}) AS preview FROM snapshots ORDER BY id DESC"
        );
        let rows: Vec<Snapshot> = self.state.storage().sql().exec(&query, None)?.to_array()?;
        Response::from_json(&rows)
    }

    async fn restore(&self, id: &str) -> Result<Response> {
        let Ok(id) = id.parse::<u32>() else {
            return Response::error("no such version", 404);
        };
        if self.ensure_loaded()?.is_none() {
            return Response::error("gone", 404);
        }
        self.save()?;
        let sql = self.state.storage().sql();
        let rows: Vec<SnapshotText> = sql
            .exec(
                "SELECT text FROM snapshots WHERE id = ?",
                vec![number(u64::from(id))],
            )?
            .to_array()?;
        let Some(version) = rows.first() else {
            return Response::error("no such version", 404);
        };
        let frame = {
            let mut loaded = self.loaded.borrow_mut();
            let Some(loaded) = loaded.as_mut() else {
                return Response::error("gone", 404);
            };
            // So a restore can be undone with another restore.
            self.snapshot_now(&loaded.room.text(), now_ms())?;
            loaded.dirty = true;
            loaded.room.replace_text(&version.text)
        };
        self.broadcast(&frame, None);
        self.save()?;
        Response::ok("")
    }

    fn text(&self) -> Result<Response> {
        if self.ensure_loaded()?.is_none() {
            return Response::error("gone", 404);
        }
        let loaded = self.loaded.borrow();
        let text = loaded.as_ref().map(|loaded| loaded.room.text());
        Response::ok(text.unwrap_or_default())
    }

    fn role_and_id(&self, socket: &WebSocket) -> (Role, String) {
        let tags = self.state.get_tags(socket);
        let role = tags.first().map_or(Role::View, |tag| Role::parse(tag));
        let id = tags.get(1).cloned().unwrap_or_default();
        (role, id)
    }
}

impl DurableObject for LiveRoom {
    fn new(state: State, env: Env) -> Self {
        Self {
            state,
            env,
            loaded: RefCell::new(None),
        }
    }

    async fn fetch(&self, mut request: Request) -> Result<Response> {
        let url = request.url()?;
        let path = url.path();
        if path == ROOM_SOCKET {
            let role = url
                .query_pairs()
                .find(|(key, _)| key == "role")
                .map_or(Role::View, |(_, value)| Role::parse(&value));
            return self.accept_socket(role);
        }
        if let Some(id) = path.strip_prefix(ROOM_RESTORE) {
            return self.restore(id).await;
        }
        match path {
            ROOM_INIT => {
                let body = request.text().await?;
                self.init(&body).await
            }
            ROOM_WIPE => {
                self.wipe().await?;
                Response::ok("")
            }
            ROOM_LOCK => self.set_locked(true),
            ROOM_UNLOCK => self.set_locked(false),
            ROOM_SNAPSHOTS => self.snapshots(),
            ROOM_TEXT => self.text(),
            _ => Response::error("nope", 404),
        }
    }

    async fn websocket_message(
        &self,
        socket: WebSocket,
        message: WebSocketIncomingMessage,
    ) -> Result<()> {
        let WebSocketIncomingMessage::Binary(frame) = message else {
            return Ok(());
        };
        let Some(fresh) = self.ensure_loaded()? else {
            return socket.close(Some(CLOSE_GONE), Some("gone"));
        };
        let (role, id) = self.role_and_id(&socket);
        let now = now_ms();
        let reply = {
            let mut loaded = self.loaded.borrow_mut();
            let Some(loaded) = loaded.as_mut() else {
                return Ok(());
            };
            // Memory was empty, so saved edits are all it has. Any open tab
            // can fill in what the last moments lost.
            if fresh {
                self.broadcast(&loaded.room.sync_request(), None);
            }
            let bucket = loaded.buckets.entry(id).or_insert_with(|| Bucket::new(now));
            if !bucket.allow(now) {
                return socket.close(Some(CLOSE_TOO_FAST), Some("too fast"));
            }
            loaded.room.receive(role, loaded.locked, &frame)
        };
        for frame in &reply.to_sender {
            send(&socket, frame);
        }
        for frame in &reply.to_others {
            self.broadcast(frame, Some(&socket));
        }
        if let Some(code) = reply.close {
            socket.close(Some(code), Some("bad frame"))?;
        }
        if reply.changed {
            self.schedule_save().await?;
        }
        Ok(())
    }

    async fn websocket_close(
        &self,
        socket: WebSocket,
        _code: usize,
        _reason: String,
        _was_clean: bool,
    ) -> Result<()> {
        let (_, id) = self.role_and_id(&socket);
        if let Some(loaded) = self.loaded.borrow_mut().as_mut() {
            loaded.buckets.remove(&id);
        }
        Ok(())
    }

    async fn websocket_error(&self, _socket: WebSocket, error: Error) -> Result<()> {
        log_error("room socket", &error.to_string());
        Ok(())
    }

    async fn alarm(&self) -> Result<Response> {
        if let Some(loaded) = self.loaded.borrow_mut().as_mut() {
            loaded.save_scheduled = false;
        }
        self.save()?;
        if !self.has_tables()? {
            return Response::ok("");
        }
        let times: Times = self
            .state
            .storage()
            .sql()
            .exec("SELECT last_edit, last_snapshot FROM room", None)?
            .one()?;
        let last_edit = times.last_edit as u64;
        if now_ms() >= expires_at(last_edit) {
            self.expire().await?;
        } else {
            self.schedule_expiry(last_edit).await?;
        }
        Response::ok("")
    }
}
