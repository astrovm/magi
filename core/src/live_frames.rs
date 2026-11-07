//! Checks every frame a browser sends before Yjs code sees it.
//!
//! `yrs` trusts its input: fuzzing it with damaged updates panics and, in
//! release builds, crashes on invalid memory. A live paste is open to anyone
//! with the link, so only what a text editor really produces gets through:
//! text in the one shared field, whose references all exist already. This also
//! means the room never has to hold updates it can't apply yet.

use std::collections::{BTreeMap, HashMap, HashSet};
use yrs::StateVector;
use yrs::updates::decoder::Decode;

/// Yjs client ids are JavaScript safe integers.
const MAX_CLIENT: u64 = (1 << 53) - 1;
/// Keeps every position far from where `yrs` counts in 32 bits.
const MAX_CLOCK: u64 = 1 << 31;

const CONTENT_GC: u8 = 0;
const CONTENT_DELETED: u8 = 1;
const CONTENT_STRING: u8 = 4;
const INFO_CONTENT_MASK: u8 = 0b0001_1111;
const INFO_PARENT_SUB: u8 = 0b0010_0000;
const INFO_RIGHT_ORIGIN: u8 = 0b0100_0000;
const INFO_ORIGIN: u8 = 0b1000_0000;

const SYNC_STEP_1: u64 = 0;
const SYNC_STEP_2: u64 = 1;
const SYNC_UPDATE: u64 = 2;

/// A frame of the sync protocol, with its payload known to be safe to use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncFrame<'a> {
    /// What the sender already has.
    Request(StateVector),
    /// Encoded update.
    Update(&'a [u8]),
}

struct Cursor<'a> {
    bytes: &'a [u8],
}

impl<'a> Cursor<'a> {
    fn byte(&mut self) -> Option<u8> {
        let (&first, rest) = self.bytes.split_first()?;
        self.bytes = rest;
        Some(first)
    }

    /// An unsigned LEB128 number, up to what a JavaScript number can hold.
    fn number(&mut self) -> Option<u64> {
        let mut value = 0u64;
        for shift in (0..=49).step_by(7) {
            let byte = self.byte()?;
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return (value <= MAX_CLIENT).then_some(value);
            }
        }
        None
    }

    fn take(&mut self, length: u64) -> Option<&'a [u8]> {
        let length = usize::try_from(length).ok()?;
        if length > self.bytes.len() {
            return None;
        }
        let (taken, rest) = self.bytes.split_at(length);
        self.bytes = rest;
        Some(taken)
    }

    fn text(&mut self) -> Option<&'a str> {
        let length = self.number()?;
        std::str::from_utf8(self.take(length)?).ok()
    }

    fn id(&mut self) -> Option<Id> {
        let client = self.number()?;
        let clock = self.number()?;
        (clock < MAX_CLOCK).then_some(Id { client, clock })
    }

    fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Id {
    client: u64,
    clock: u64,
}

struct Block {
    clock: u64,
    length: u64,
    origin: Option<Id>,
    right_origin: Option<Id>,
}

struct Section {
    client: u64,
    blocks: Vec<Block>,
    /// Blocks already known to fit, during the dependency check.
    done: usize,
}

/// One message of the sync protocol, or `None` if it isn't exactly that.
pub fn parse_sync_frame(frame: &[u8]) -> Option<SyncFrame<'_>> {
    let mut cursor = Cursor { bytes: frame };
    if cursor.number()? != 0 {
        return None;
    }
    let kind = cursor.number()?;
    let length = cursor.number()?;
    let payload = cursor.take(length)?;
    if !cursor.is_empty() {
        return None;
    }
    match kind {
        SYNC_STEP_1 if is_valid_state_vector(payload) => {
            StateVector::decode_v1(payload).ok().map(SyncFrame::Request)
        }
        SYNC_STEP_2 | SYNC_UPDATE => Some(SyncFrame::Update(payload)),
        _ => None,
    }
}

fn is_valid_state_vector(payload: &[u8]) -> bool {
    let mut cursor = Cursor { bytes: payload };
    let Some(clients) = cursor.number() else {
        return false;
    };
    for _ in 0..clients {
        if cursor.id().is_none() {
            return false;
        }
    }
    cursor.is_empty()
}

/// Length of a string the way Yjs counts it.
fn utf16_length(text: &str) -> u64 {
    text.encode_utf16().count() as u64
}

fn read_block(cursor: &mut Cursor<'_>, clock: u64, field: &str) -> Option<Block> {
    let info = cursor.byte()?;
    let content = info & INFO_CONTENT_MASK;
    if content == CONTENT_GC {
        let length = cursor.number()?;
        return Some(Block {
            clock,
            length,
            origin: None,
            right_origin: None,
        });
    }
    if info & INFO_PARENT_SUB != 0 {
        return None;
    }
    let origin = if info & INFO_ORIGIN != 0 {
        Some(cursor.id()?)
    } else {
        None
    };
    let right_origin = if info & INFO_RIGHT_ORIGIN != 0 {
        Some(cursor.id()?)
    } else {
        None
    };
    // With nothing to its left or right, a block has to name its parent, and
    // the only parent there is is the shared text field.
    if origin.is_none()
        && right_origin.is_none()
        && (cursor.number()? != 1 || cursor.text()? != field)
    {
        return None;
    }
    let length = match content {
        CONTENT_DELETED => cursor.number()?,
        CONTENT_STRING => utf16_length(cursor.text()?),
        _ => return None,
    };
    Some(Block {
        clock,
        length,
        origin,
        right_origin,
    })
}

fn read_section(cursor: &mut Cursor<'_>, field: &str) -> Option<Section> {
    let count = cursor.number()?;
    let client = cursor.number()?;
    let mut clock = cursor.number()?;
    let mut blocks = Vec::new();
    for _ in 0..count {
        let block = read_block(cursor, clock, field)?;
        if block.length == 0 {
            return None;
        }
        clock = clock.checked_add(block.length)?;
        if clock > MAX_CLOCK {
            return None;
        }
        blocks.push(block);
    }
    Some(Section {
        client,
        blocks,
        done: 0,
    })
}

/// True if everything in the update can go in right now, in some order.
///
/// A section stuck on something missing is only looked at again once that thing
/// arrives. Going round all of them until nothing moves took 13 s of CPU on a
/// frame built for it, so this has to stay close to linear in the blocks.
fn dependencies_met(sections: &mut [Section], known: &mut HashMap<u64, u64>) -> bool {
    let mut ready: Vec<&mut Section> = sections.iter_mut().collect();
    let mut stuck: HashMap<u64, BTreeMap<u64, Vec<&mut Section>>> = HashMap::new();
    while let Some(section) = ready.pop() {
        let mut missing = None;
        while let Some(block) = section.blocks.get(section.done) {
            missing = first_missing(section.client, block, known);
            if missing.is_some() {
                break;
            }
            let next = known.get(&section.client).copied().unwrap_or(0);
            let reached = next.max(block.clock + block.length);
            known.insert(section.client, reached);
            section.done += 1;
            if let Some(waiting) = stuck.get_mut(&section.client) {
                while let Some(entry) = waiting.first_entry() {
                    if *entry.key() >= reached {
                        break;
                    }
                    ready.extend(entry.remove());
                }
            }
        }
        if let Some(id) = missing {
            let waiting = stuck.entry(id.client).or_default();
            waiting.entry(id.clock).or_default().push(section);
        }
    }
    stuck.values().all(BTreeMap::is_empty)
}

/// The first thing this block needs that isn't there yet.
fn first_missing(client: u64, block: &Block, known: &HashMap<u64, u64>) -> Option<Id> {
    let knows = |id: &Id| id.clock < known.get(&id.client).copied().unwrap_or(0);
    let before = block.clock.checked_sub(1).map(|clock| Id { client, clock });
    [before, block.origin, block.right_origin]
        .into_iter()
        .flatten()
        .find(|id| !knows(id))
}

fn deletes_are_known(cursor: &mut Cursor<'_>, known: &HashMap<u64, u64>) -> Option<()> {
    let clients = cursor.number()?;
    for _ in 0..clients {
        let client = cursor.number()?;
        let ranges = cursor.number()?;
        let end = known.get(&client).copied().unwrap_or(0);
        for _ in 0..ranges {
            let clock = cursor.number()?;
            let length = cursor.number()?;
            if length == 0 || clock.checked_add(length)? > end {
                return None;
            }
        }
    }
    Some(())
}

/// Checks an update against what the room already holds. `known` says how far
/// each client's history goes in the room.
pub fn is_acceptable_update(update: &[u8], field: &str, known: &HashMap<u64, u64>) -> bool {
    let mut cursor = Cursor { bytes: update };
    let Some(clients) = cursor.number() else {
        return false;
    };
    let mut sections = Vec::new();
    let mut seen = HashSet::new();
    for _ in 0..clients {
        let Some(section) = read_section(&mut cursor, field) else {
            return false;
        };
        if !seen.insert(section.client) {
            return false;
        }
        sections.push(section);
    }
    let mut known = known.clone();
    dependencies_met(&mut sections, &mut known)
        && deletes_are_known(&mut cursor, &known).is_some()
        && cursor.is_empty()
}
