//! Writing a planned game's enemies into the room scripts.
//!
//! A room script marks where its enemies are set with `random_enemies()`
//! (`docs/modding.md`), which the runtime build compiles to a four-byte no-op
//! ([`MARKER_OP`], [`MARKER_TAG`] and the marker's id). The original enemies
//! sit under `if (!ENEMY_RANDOMIZER)`. A pack that randomizes enemies raises
//! `ENEMY_RANDOMIZER`, so the originals are skipped, and [`patch_room`] puts
//! the plan's enemies in their place:
//!
//! - Every `enemy` of the plan's `Retype` placements, found by its slot, gets
//!   the new type and the reset state staging gives it.
//! - The `Create` placements of a marker become an event script of one
//!   `enemy` per placement. The marker, four bytes, is rewritten in place to
//!   an `evt_exec` of that script, so nothing else of the room moves. The
//!   scripts are appended to the event table.
//! - A room the plan leaves alone has the sense of every `ENEMY_RANDOMIZER`
//!   test flipped, so it runs as if the flag were clear: its original enemies
//!   stay and the conditions staging adds for randomized rooms do not apply.
//!   The same goes for a room whose plan creates nothing.

use std::collections::BTreeSet;

use anyhow::{Context, Result, bail};
use arklay_biorand::enemies::{Placement, RoomEnemies};
use arklay_biorand::map::Spot;
use arklay_biorand::plan::Game;

use crate::scd::akscr::{
    ENEMY_MARKER_OP as MARKER_OP, ENEMY_MARKER_TAG as MARKER_TAG, placeholder_id,
};
use crate::scd::asm::Assembled;
use crate::scd::ir::{Insn, Scripts, Stream, StreamKind};
use crate::scd::opcode::system::ENEMY_RANDOMIZER;
use crate::scd::reader;

/// `ck`, and the byte offsets of its bank, flag and expected value.
const CK: u8 = 0x04;
const CK_BANK: usize = 1;
const CK_FLAG: usize = 2;
const CK_VALUE: usize = 3;
/// The bank the system flags are in.
const SYSTEM_BANK: u8 = 5;
/// `enemy`, the byte offsets of its fields, and its length.
const ENEMY: u8 = 0x1B;
const ENEMY_LEN: usize = 22;
const ENEMY_TYPE: usize = 1;
const ENEMY_STATE: usize = 2;
const ENEMY_KILL_FLAG: usize = 3;
const ENEMY_D: usize = 8;
const ENEMY_X: usize = 12;
const ENEMY_Y: usize = 14;
const ENEMY_Z: usize = 16;
const ENEMY_SLOT: usize = 18;
/// `evt_exec`, with its task: the first free slot from 8.
const EVT_EXEC: u8 = 0x14;
const EVT_TASK: u8 = 9;
/// `evt_single` and its operand: its own two bytes and the command.
const EVT_SINGLE: u8 = 0x07;
/// `evt_finish`.
const EVT_FINISH: u8 = 0xFF;
/// The slots a room has for enemies.
const SLOTS: u8 = 16;
/// Snake and web spinner, which a retype resets to state 0.
const RESET_TYPES: [u8; 2] = [0x03, 0x0A];
/// The enemy types the plan can replace (staging's `CanRemoveEnemy`): the
/// others, props and NPCs and bosses, keep their slots.
const REMOVABLE: [u8; 14] = [
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x09, 0x0A, 0x0B, 0x0C, 0x0E, 0x11,
];

/// The `enemy` instructions of a room, in the command streams.
fn enemy_insns(scripts: &Scripts) -> impl Iterator<Item = &Insn> {
    super::emit::commands(scripts).filter(|insn| insn.op == ENEMY && insn.bytes.len() >= ENEMY_LEN)
}

/// The `random_enemies()` markers of a room, with their ids.
pub(super) fn markers(scripts: &Scripts) -> Vec<(&Insn, u16)> {
    super::emit::commands(scripts)
        .filter(|insn| insn.op == MARKER_OP && insn.bytes.len() == 4 && insn.bytes[1] == MARKER_TAG)
        .map(|insn| (insn, u16::from_le_bytes([insn.bytes[2], insn.bytes[3]])))
        .collect()
}

/// The `ck`s of `ENEMY_RANDOMIZER` in a room.
pub(super) fn randomizer_tests(scripts: &Scripts) -> Vec<&Insn> {
    super::emit::commands(scripts)
        .filter(|insn| {
            insn.op == CK
                && insn.bytes.len() == 4
                && insn.bytes[CK_BANK] == SYSTEM_BANK
                && insn.bytes[CK_FLAG] == ENEMY_RANDOMIZER
        })
        .collect()
}

/// The placeholder id a spot belongs to.
fn spot_marker(spot: &Spot) -> u16 {
    placeholder_id(spot.placeholder.as_deref())
}

/// One enemy a room is to create.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Created {
    pub marker: u16,
    pub enemy: u8,
    pub pose: u8,
    pub kill_flag: u8,
    pub slot: u8,
    pub x: i16,
    pub y: i16,
    pub z: i16,
    pub d: i16,
}

impl Created {
    /// The 22-byte `enemy` record.
    pub fn record(&self) -> [u8; ENEMY_LEN] {
        let mut record = [0u8; ENEMY_LEN];
        record[0] = ENEMY;
        record[ENEMY_TYPE] = self.enemy;
        record[ENEMY_STATE] = self.pose;
        record[ENEMY_KILL_FLAG] = self.kill_flag;
        record[4] = 1;
        record[5] = 2;
        record[ENEMY_D..ENEMY_D + 2].copy_from_slice(&self.d.to_le_bytes());
        record[ENEMY_X..ENEMY_X + 2].copy_from_slice(&self.x.to_le_bytes());
        record[ENEMY_Y..ENEMY_Y + 2].copy_from_slice(&self.y.to_le_bytes());
        record[ENEMY_Z..ENEMY_Z + 2].copy_from_slice(&self.z.to_le_bytes());
        record[ENEMY_SLOT] = self.slot;
        record
    }
}

/// The enemies `room` creates in a room whose scripts are `scripts`, with
/// their final slots. A created enemy takes the slot the plan gave it unless a
/// fixed enemy that stays (a type the plan cannot replace) holds it, and then
/// the lowest slot nothing holds. One with no slot left, or a position that
/// does not fit 16 bits, is dropped.
pub(super) fn created(scripts: &Scripts, room: &RoomEnemies) -> Vec<Created> {
    let fixed: BTreeSet<u8> = enemy_insns(scripts)
        .filter(|insn| !REMOVABLE.contains(&insn.bytes[ENEMY_TYPE]))
        .map(|insn| insn.bytes[ENEMY_SLOT] & 0x0F)
        .collect();
    // Every slot a fixed enemy or the plan holds.
    let mut used = fixed.clone();
    for placement in &room.placements {
        match placement {
            Placement::Retype { slot, .. } | Placement::Create { slot, .. } => {
                used.insert(*slot);
            }
        }
    }
    let mut out = Vec::new();
    for placement in &room.placements {
        let Placement::Create {
            slot,
            enemy,
            pose,
            kill_flag,
            spot,
        } = placement
        else {
            continue;
        };
        let slot = if fixed.contains(slot) {
            match (0..SLOTS).find(|s| !used.contains(s)) {
                Some(free) => free,
                None => continue,
            }
        } else {
            *slot
        };
        let (Ok(x), Ok(y), Ok(z), Ok(d)) = (
            i16::try_from(spot.x),
            i16::try_from(spot.y),
            i16::try_from(spot.z),
            i16::try_from(spot.d),
        ) else {
            continue;
        };
        used.insert(slot);
        out.push(Created {
            marker: spot_marker(spot),
            enemy: *enemy,
            pose: *pose,
            kill_flag: *kill_flag,
            slot,
            x,
            y,
            z,
            d,
        });
    }
    out
}

/// The new state byte of an `enemy` retyped from `old` to `new`
/// (`Re1CrModBuilder.cs:602-610`).
pub(super) fn retyped_state(new: u8, state: u8) -> u8 {
    if RESET_TYPES.contains(&new) {
        0
    } else {
        state & 0x80
    }
}

/// The event script that creates `enemies`: an `evt_single` of each `enemy`,
/// and `evt_finish`.
fn event_bytes(enemies: &[&Created]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for enemy in enemies {
        bytes.push(EVT_SINGLE);
        bytes.push(u8::try_from(ENEMY_LEN + 2).expect("a short instruction"));
        bytes.extend_from_slice(&enemy.record());
    }
    bytes.push(EVT_FINISH);
    bytes
}

/// `scd` with the enemies of `room` written in; `None` is a room the plan
/// does not touch. See the module docs.
pub(super) fn patch_room(scd: &[u8], room: Option<&RoomEnemies>) -> Result<Vec<u8>> {
    let scripts = reader::parse(scd)?;
    let mut out = scd.to_vec();
    let made = room.map(|room| created(&scripts, room)).unwrap_or_default();
    if made.is_empty() {
        for insn in randomizer_tests(&scripts) {
            out[insn.offset + CK_VALUE] ^= 1;
        }
    }
    let Some(room) = room else {
        return Ok(out);
    };
    for placement in &room.placements {
        let Placement::Retype { slot, enemy } = placement else {
            continue;
        };
        for insn in enemy_insns(&scripts).filter(|insn| insn.bytes[ENEMY_SLOT] & 0x0F == *slot) {
            let old = insn.bytes[ENEMY_TYPE];
            if old != *enemy {
                out[insn.offset + ENEMY_TYPE] = *enemy;
                out[insn.offset + ENEMY_STATE] = retyped_state(*enemy, insn.bytes[ENEMY_STATE]);
            }
        }
    }
    if made.is_empty() {
        return Ok(out);
    }

    // One new event script per marker the plan creates at.
    let ids: BTreeSet<u16> = made.iter().map(|enemy| enemy.marker).collect();
    let first = scripts.events.len();
    let mut events: Vec<Vec<u8>> = Vec::new();
    for id in &ids {
        let here: Vec<&Created> = made.iter().filter(|enemy| enemy.marker == *id).collect();
        let marks: Vec<&Insn> = markers(&scripts)
            .into_iter()
            .filter(|(_, marker)| marker == id)
            .map(|(insn, _)| insn)
            .collect();
        if marks.is_empty() {
            bail!(
                "the room has no random_enemies() marker {id:#06x} for {} enemies",
                here.len()
            );
        }
        let index = u8::try_from(first + events.len())
            .ok()
            .filter(|index| *index < u8::MAX)
            .context("the room has too many events for another")?;
        for mark in marks {
            out[mark.offset..mark.offset + 4].copy_from_slice(&[EVT_EXEC, 0, EVT_TASK, index]);
        }
        events.push(event_bytes(&here));
    }

    let mut assembled = reader::parse(&out)?;
    for bytes in events {
        let (insns, trailing) = reader::decode_event_stream(&bytes, 0, bytes.len());
        assembled.events.push(Stream {
            kind: StreamKind::Event(u8::try_from(assembled.events.len()).unwrap_or(u8::MAX)),
            offset: 0,
            insns,
            trailing,
        });
    }
    Assembled {
        init: assembled.init,
        main: assembled.main,
        events: assembled.events,
    }
    .to_container()
}

/// `scd`, room `dir`'s script, with `game`'s enemies written in. A game that
/// does not randomize enemies leaves the script as it is.
pub(super) fn patch_for_game(scd: &[u8], dir: &str, game: &Game) -> Result<Vec<u8>> {
    if !game
        .system_flags
        .contains(&arklay_biorand::plan::system_flags::ENEMY_RANDOMIZER)
    {
        return Ok(scd.to_vec());
    }
    let room = game
        .enemies
        .rooms
        .values()
        .find(|room| room.directory == dir);
    patch_room(scd, room)
}

/// Checks room `dir`'s patched `scripts` against the plan: every retyped
/// enemy has its new type, every created enemy is in an event script with the
/// planned record, and no marker the plan creates at is left. Returns the
/// number of planned enemies found, and the problems in `problems`.
pub(super) fn check_room(
    path: &str,
    dir: &str,
    scripts: &Scripts,
    game: &Game,
    problems: &mut Vec<String>,
) -> usize {
    if !game
        .system_flags
        .contains(&arklay_biorand::plan::system_flags::ENEMY_RANDOMIZER)
    {
        return 0;
    }
    let Some(room) = game
        .enemies
        .rooms
        .values()
        .find(|room| room.directory == dir)
    else {
        return 0;
    };
    let mut found = 0;
    for placement in &room.placements {
        let Placement::Retype { slot, enemy } = placement else {
            continue;
        };
        let insns: Vec<&Insn> = enemy_insns(scripts)
            .filter(|insn| insn.bytes[ENEMY_SLOT] & 0x0F == *slot)
            .collect();
        if insns.is_empty() {
            problems.push(format!(
                "{path}: no enemy in slot {slot} to become {enemy:#04x}"
            ));
        }
        for insn in insns {
            if insn.bytes[ENEMY_TYPE] == *enemy {
                found += 1;
            } else {
                problems.push(format!(
                    "{path}: the enemy at {:#x} in slot {slot} is type {:#04x}, the plan {enemy:#04x}",
                    insn.offset, insn.bytes[ENEMY_TYPE]
                ));
            }
        }
    }
    let made = created(scripts, room);
    let events: Vec<&Insn> = scripts
        .events
        .iter()
        .flat_map(|stream| &stream.insns)
        .filter(|insn| insn.op == ENEMY && insn.bytes.len() >= ENEMY_LEN)
        .collect();
    let left = markers(scripts);
    for enemy in &made {
        if events
            .iter()
            .any(|insn| insn.bytes[..ENEMY_LEN] == enemy.record())
        {
            found += 1;
        } else {
            problems.push(format!(
                "{path}: no event creates enemy {:#04x} in slot {} at ({}, {}, {})",
                enemy.enemy, enemy.slot, enemy.x, enemy.y, enemy.z
            ));
        }
        if left.iter().any(|(_, id)| *id == enemy.marker) {
            problems.push(format!(
                "{path}: the random_enemies() marker {:#06x} is still there",
                enemy.marker
            ));
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scd::akscr;

    /// A runtime build of a script: an original enemy under the guard, a
    /// retypable one, a named marker and an unnamed one.
    fn room() -> Vec<u8> {
        let text = "#version 1\nproc init\n{\n\
            if (!ENEMY_RANDOMIZER)\n{\n\
            enemy(0, 0, 1, 1, 2, 0, 0, 0, 100, 0, 200, 0, 0, 0, 0);\n}\n\
            random_enemies();\n\
            random_enemies(side);\n\
            enemy(0, 0x80, 2, 1, 2, 0, 0, 0, 300, 0, 400, 1, 0, 0, 0);\n\
            enemy(0x1F, 0, 3, 1, 2, 0, 0, 0, 500, 0, 600, 2, 0, 0, 0);\n\
            }\n";
        akscr::compile(text).unwrap().to_container().unwrap()
    }

    fn spot(placeholder: Option<&str>, x: i32) -> Spot {
        Spot {
            x,
            y: -10,
            z: 7,
            d: 1024,
            include: Vec::new(),
            exclude: Vec::new(),
            placeholder: placeholder.map(str::to_string),
        }
    }

    fn plan(placements: Vec<Placement>) -> RoomEnemies {
        RoomEnemies {
            directory: "100".into(),
            kind: "hunter".into(),
            esp: Vec::new(),
            placements,
        }
    }

    fn create(slot: u8, placeholder: Option<&str>, x: i32) -> Placement {
        Placement::Create {
            slot,
            enemy: 6,
            pose: 0x12,
            kill_flag: 0x40 + slot,
            spot: spot(placeholder, x),
        }
    }

    #[test]
    fn creates_become_events_at_their_markers() {
        let scd = room();
        let before = reader::parse(&scd).unwrap();
        assert_eq!(markers(&before).len(), 2);
        let plan = plan(vec![
            create(3, None, 1000),
            create(4, Some("side"), 2000),
            create(5, None, 3000),
        ]);
        let out = patch_room(&scd, Some(&plan)).unwrap();
        let after = reader::parse(&out).unwrap();
        assert!(markers(&after).is_empty());
        assert_eq!(after.events.len(), before.events.len() + 2);
        // Every created record is in an event script.
        for made in created(&after, &plan) {
            assert!(
                after
                    .events
                    .iter()
                    .flat_map(|stream| &stream.insns)
                    .any(|insn| insn.bytes == made.record()),
                "{made:?}"
            );
        }
    }

    #[test]
    fn a_created_enemy_avoids_the_slot_of_a_fixed_one() {
        let scripts = reader::parse(&room()).unwrap();
        // Slot 2 holds the prop, which stays.
        let plan = plan(vec![create(2, None, 1)]);
        let made = created(&scripts, &plan);
        assert_eq!(made.len(), 1);
        assert_ne!(made[0].slot, 2);
    }

    #[test]
    fn retypes_reset_the_state_and_marks_without_creates_stay() {
        let scd = room();
        let plan = plan(vec![Placement::Retype {
            slot: 1,
            enemy: 0x0A,
        }]);
        let out = patch_room(&scd, Some(&plan)).unwrap();
        let scripts = reader::parse(&out).unwrap();
        let retyped: Vec<_> = enemy_insns(&scripts)
            .filter(|insn| insn.bytes[ENEMY_SLOT] & 0x0F == 1)
            .collect();
        assert_eq!(retyped.len(), 1);
        assert_eq!(retyped[0].bytes[ENEMY_TYPE], 0x0A);
        assert_eq!(retyped[0].bytes[ENEMY_STATE], 0);
        // No creates: the markers stay and the randomizer test is flipped.
        assert_eq!(markers(&scripts).len(), 2);
        assert_eq!(retyped_state(0x06, 0x92), 0x80);
    }

    #[test]
    fn an_untouched_room_runs_as_if_the_flag_were_clear() {
        let scd = room();
        let before = reader::parse(&scd).unwrap();
        let out = patch_room(&scd, None).unwrap();
        let after = reader::parse(&out).unwrap();
        let tests = randomize_values(&before);
        let flipped = randomize_values(&after);
        assert!(!tests.is_empty());
        assert!(tests.iter().zip(&flipped).all(|(a, b)| a ^ 1 == *b));
    }

    fn randomize_values(scripts: &Scripts) -> Vec<u8> {
        randomizer_tests(scripts)
            .iter()
            .map(|insn| insn.bytes[CK_VALUE])
            .collect()
    }

    #[test]
    fn a_create_without_its_marker_is_an_error() {
        let plan = plan(vec![create(3, Some("nowhere"), 1)]);
        let err = patch_room(&room(), Some(&plan)).unwrap_err();
        assert!(format!("{err:#}").contains("marker"), "{err:#}");
    }
}
