//! Door locks in a generated pack's room scripts.
//!
//! Under `BR_LOCK_FULL_BYTE` the engine reads a door's whole lock byte as its
//! lock flag, 0 being unlocked. A pack that raises it therefore writes a lock
//! byte the new way into every `door_aot_set` of every room script
//! ([`patch_doors`]):
//!
//! - a door the plan has (matched by its room, AOT id and the room its
//!   script names, [`DoorLock::script_target`]) gets the plan's lock flag and key byte ([`DoorLock`]), in every
//!   branch and variant that declares it;
//! - any other door keeps its key and gets its original lock flag: `byte &
//!   0x3F` when the original locks it (`0x80`), otherwise 0. Its `0x40` bit
//!   (refused to Rebecca) becomes a room-script check under the same flag.
//!
//! The bytes are patched where they are, so nothing else of a script moves.

use std::collections::BTreeMap;

use anyhow::{Result, bail, ensure};
use arklay_biorand::locks::DoorLock;
use arklay_biorand::plan::{Game, system_flags};

use super::emit::commands;
use crate::scd::ir::Scripts;

/// `door_aot_set`, the instruction that declares a door.
pub(super) const DOOR_AOT_SET: u8 = 0x0C;
/// The byte offsets of a `door_aot_set`'s fields.
pub(super) const DOOR_ID: usize = 1;
pub(super) const DOOR_LOCK: usize = 14;
pub(super) const DOOR_NEXT_ROOM: usize = 15;
pub(super) const DOOR_KEY: usize = 24;
/// The original's lock bits: checked, and its flag.
const ORIGINAL_LOCKED: u8 = 0x80;
const ORIGINAL_FLAG: u8 = 0x3F;

/// Whether `game`'s pack writes whole lock bytes.
pub(super) fn full_byte(game: &Game) -> bool {
    game.system_flags.contains(&system_flags::BR_LOCK_FULL_BYTE)
}

/// The room directory a door in room `dir` leads to, from its next-room byte
/// (as `GameState::door_destination` decodes it), or `None` for a door that
/// leads nowhere.
pub(super) fn target_dir(dir: &str, next_room: u8) -> Option<String> {
    if next_room == 0xFF {
        return None;
    }
    let stage = if next_room < 0x20 {
        dir.chars().next()?.to_digit(10)? as u8
    } else {
        let row = (next_room >> 5) - 1;
        if row >= 5 { row - 4 } else { row + 1 }
    };
    Some(format!("{stage}{:02x}", next_room & 0x1F))
}

/// What one `door_aot_set` should hold: `(lock, key)`, and the plan's door
/// when it has one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Expected<'a> {
    pub lock: u8,
    pub key: u8,
    pub planned: Option<&'a DoorLock>,
}

/// The plan's doors of room `dir`, by AOT id.
pub(super) fn planned<'a>(game: &'a Game, dir: &str) -> BTreeMap<u8, Vec<&'a DoorLock>> {
    let mut doors: BTreeMap<u8, Vec<&DoorLock>> = BTreeMap::new();
    for lock in game.locks.iter().filter(|lock| lock.dir == dir) {
        doors.entry(lock.id).or_default().push(lock);
    }
    doors
}

/// What the `door_aot_set` `bytes` in room `dir` should hold.
pub(super) fn expected<'a>(
    dir: &str,
    bytes: &[u8],
    planned: &BTreeMap<u8, Vec<&'a DoorLock>>,
) -> Result<Expected<'a>> {
    ensure!(bytes.len() > DOOR_KEY, "a short door_aot_set");
    let id = bytes[DOOR_ID] & 0x7F;
    let target = target_dir(dir, bytes[DOOR_NEXT_ROOM]);
    let candidates = planned.get(&id).map_or(&[][..], Vec::as_slice);
    // By the target the script names; a door the plan has naming none
    // matches by id.
    let mut matches: Vec<&DoorLock> = candidates
        .iter()
        .copied()
        .filter(|lock| target.is_some() && lock.script_target == target)
        .collect();
    if matches.is_empty() {
        matches = candidates
            .iter()
            .copied()
            .filter(|lock| lock.script_target.is_none())
            .collect();
    }
    if let Some(&first) = matches.first() {
        if let Some(other) = matches
            .iter()
            .find(|lock| (lock.lock, lock.key) != (first.lock, first.key))
        {
            bail!(
                "AOT {id} is both {}:{} (lock {}, key {}) and {}:{} (lock {}, key {})",
                first.room,
                first.id,
                first.lock,
                first.key,
                other.room,
                other.id,
                other.lock,
                other.key
            );
        }
        return Ok(Expected {
            lock: first.lock,
            key: first.key,
            planned: Some(first),
        });
    }
    let original = bytes[DOOR_LOCK];
    let lock = if original & ORIGINAL_LOCKED != 0 {
        original & ORIGINAL_FLAG
    } else {
        0
    };
    ensure!(
        original & ORIGINAL_LOCKED == 0 || lock != 0,
        "AOT {id} is locked with flag 0, which a whole lock byte can't say"
    );
    Ok(Expected {
        lock,
        key: bytes[DOOR_KEY],
        planned: None,
    })
}

/// Room `dir`'s script `scd` (already holding its planned pickups) with
/// every door's lock byte and key written the whole-byte way. Returns it
/// unchanged when `game` keeps the original locks.
pub(super) fn patch_doors(scd: &[u8], dir: &str, game: &Game) -> Result<Vec<u8>> {
    let mut out = scd.to_vec();
    if !full_byte(game) {
        return Ok(out);
    }
    let scripts = crate::scd::reader::parse(scd)?;
    let planned = planned(game, dir);
    for insn in doors(&scripts) {
        let expected = expected(dir, &insn.bytes, &planned)
            .map_err(|err| anyhow::anyhow!("the door_aot_set at {:#x}: {err:#}", insn.offset))?;
        out[insn.offset + DOOR_LOCK] = expected.lock;
        out[insn.offset + DOOR_KEY] = expected.key;
    }
    Ok(out)
}

/// Every `door_aot_set` of a room's scripts.
pub(super) fn doors(scripts: &Scripts) -> impl Iterator<Item = &crate::scd::ir::Insn> {
    commands(scripts).filter(|insn| insn.op == DOOR_AOT_SET)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_dirs_decode_like_the_engine() {
        assert_eq!(target_dir("201", 0x14).as_deref(), Some("214"));
        assert_eq!(target_dir("201", 0x21).as_deref(), Some("101"));
        assert_eq!(target_dir("201", 0xC1).as_deref(), Some("101"));
        assert_eq!(target_dir("10a", 0x40 | 0x0B).as_deref(), Some("20b"));
        assert_eq!(target_dir("303", 0xA0 | 0x0D).as_deref(), Some("50d"));
        assert_eq!(target_dir("303", 0xFF), None);
    }

    fn door_bytes(id: u8, lock: u8, next_room: u8, key: u8) -> Vec<u8> {
        let mut bytes = vec![0u8; 26];
        bytes[0] = DOOR_AOT_SET;
        bytes[DOOR_ID] = id;
        bytes[DOOR_LOCK] = lock;
        bytes[DOOR_NEXT_ROOM] = next_room;
        bytes[DOOR_KEY] = key;
        bytes
    }

    fn lock(id: u8, target: Option<&str>, lock: u8, key: u8) -> DoorLock {
        DoorLock {
            room: "201".into(),
            dir: "201".into(),
            id,
            target: target.map(str::to_string),
            script_target: target.map(str::to_string),
            lock,
            key,
            drawn: true,
            other: None,
        }
    }

    #[test]
    fn planned_doors_take_the_plan_and_others_their_original_flag() {
        let locks = [lock(1, Some("214"), 81, 58), lock(7, None, 255, 255)];
        let mut planned: BTreeMap<u8, Vec<&DoorLock>> = BTreeMap::new();
        for l in &locks {
            planned.entry(l.id).or_default().push(l);
        }
        let got = expected("201", &door_bytes(1, 0x80 | 0x40 | 0x3F, 0x14, 0), &planned).unwrap();
        assert_eq!((got.lock, got.key), (81, 58));
        assert!(got.planned.is_some());
        // The same id to another room is not the plan's door.
        let got = expected("201", &door_bytes(1, 0x80 | 0x0C, 0x15, 54), &planned).unwrap();
        assert_eq!((got.lock, got.key, got.planned), (12, 54, None));
        // Unlocked, refused to Rebecca: unlocked.
        let got = expected("201", &door_bytes(3, 0x40 | 0x05, 0x02, 0), &planned).unwrap();
        assert_eq!((got.lock, got.key), (0, 0));
        // A door leading nowhere in this game matches by id.
        let got = expected("201", &door_bytes(7, 0, 0x16, 0), &planned).unwrap();
        assert_eq!((got.lock, got.key), (255, 255));
        // Flag 0 can't be said.
        assert!(expected("201", &door_bytes(4, 0x80, 0x02, 0), &planned).is_err());
    }
}
