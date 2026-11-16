//! Randomized doors in a generated pack.
//!
//! Under `doors/random` the plan gives every room of the game a new id
//! ([`DoorPlan::rooms`]): segment `n` is stage `n`, its rooms numbered from 0.
//! The pack moves each room's folder to its new id ([`entry_path`]), and:
//!
//! - its `room.xml` names the original as its `origin`, so the engine plays
//!   it as that room, and lists in `<doors>` every door whose target byte
//!   would now lead elsewhere than the plan says ([`patch_room_xml`]);
//! - each `door_aot_set` of a planned door puts the player where the far
//!   door's entrance says: position, facing and camera ([`patch_script`]). Its
//!   animation and sound stay the source door's. A door the plan does not
//!   have that leads to a room the game leaves out is locked for good.
//!
//! The rooms the game leaves out are dropped, but room 311 (which 303's death
//! replay enters by its id) keeps its folder, and no room takes its id. Every
//! room's `random.xml` moves to `biorand/rooms/<room>.xml`, so the pack still
//! carries the whole map a check plans the game again from.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, bail, ensure};
use arklay_biorand::doors::{self as plan_doors, DoorPlan, PlannedDoor};
use arklay_biorand::plan::Game;

use super::locks::{self, DOOR_ID, DOOR_KEY, DOOR_LOCK, DOOR_NEXT_ROOM};
use crate::scd::ir::Scripts;

/// Where a door-randomized pack keeps each room's `random.xml`.
pub(super) const MAP_ROOMS: &str = "biorand/rooms/";

/// The byte offsets of a `door_aot_set`'s camera byte and the player's
/// arrival: position (x, y, z) and facing, each a little-endian word.
const DOOR_CAMERA: usize = 13;
const DOOR_X: usize = 16;
const DOOR_Y: usize = 18;
const DOOR_Z: usize = 20;
const DOOR_ANGLE: usize = 22;
/// The camera byte's camera-only bit (the door re-aims the camera without
/// leaving the room) and the arrival camera's bits.
const CAMERA_ONLY: u8 = 0x80;
const CAMERA_CUT: u8 = 0x3F;
/// A lock byte and key that never open.
const LOCKED_FOR_GOOD: u8 = 0xFF;

/// The door plan of `game`, when its doors are randomized.
pub(super) fn plan(game: &Game) -> Option<&DoorPlan> {
    game.doors.as_ref()
}

/// A room entry `room/<dir>/<rest>`: its directory, lowercase, and the rest.
fn split_room_path(path: &str) -> Option<(String, &str)> {
    let rest = path.strip_prefix("room/")?;
    let (dir, rest) = rest.split_once('/')?;
    Some((dir.to_ascii_lowercase(), rest))
}

/// Where the base's entry `path` goes in `game`'s pack, or `None` when the
/// game leaves it out. Every entry stays where it is unless doors are
/// randomized.
pub(super) fn entry_path(game: &Game, path: &str) -> Option<String> {
    let Some(plan) = plan(game) else {
        return Some(path.to_string());
    };
    let Some((dir, rest)) = split_room_path(path) else {
        return Some(path.to_string());
    };
    if rest.eq_ignore_ascii_case("random.xml") {
        return Some(format!("{MAP_ROOMS}{dir}.xml"));
    }
    if let Some(room) = plan.rooms.get(&dir) {
        return Some(format!("room/{}/{rest}", room.id));
    }
    plan_doors::RESERVED_IDS
        .contains(&dir.as_str())
        .then(|| path.to_string())
}

/// The plan's door that a `door_aot_set` (`bytes`) in room `dir` (the
/// original's directory) declares: by its AOT id and the room its target
/// byte names, else a planned door of that id naming none.
pub(super) fn planned_door<'a>(
    plan: &'a DoorPlan,
    dir: &str,
    bytes: &[u8],
) -> Option<&'a PlannedDoor> {
    let id = bytes[DOOR_ID] & 0x7F;
    let target = locks::target_dir(dir, bytes[DOOR_NEXT_ROOM]);
    let candidates = || {
        plan.doors
            .iter()
            .filter(move |door| door.dir == dir && door.id == id)
    };
    candidates()
        .find(|door| target.is_some() && door.script_target == target)
        .or_else(|| candidates().find(|door| door.script_target.is_none()))
}

/// Whether a `door_aot_set` leads anywhere: not camera-only, and with a
/// target byte.
fn leaves_room(bytes: &[u8]) -> bool {
    bytes[DOOR_CAMERA] & CAMERA_ONLY == 0 && bytes[DOOR_NEXT_ROOM] != 0xFF
}

/// The new id of the room a `door_aot_set` (`bytes`) in room `dir` leads
/// to in the game, or `None` when it leads nowhere: the plan's far side for
/// a planned door, else the new id of the room its byte names.
pub(super) fn new_target(plan: &DoorPlan, dir: &str, bytes: &[u8]) -> Option<String> {
    if !leaves_room(bytes) {
        return None;
    }
    match planned_door(plan, dir, bytes) {
        Some(door) => door.target.as_ref().map(|target| target.id.clone()),
        None => {
            let target = locks::target_dir(dir, bytes[DOOR_NEXT_ROOM])?;
            plan.new_id(&target).map(str::to_string)
        }
    }
}

/// Where the engine sends a door with target byte `next_room` from a room of
/// stage `stage` when `<doors>` says nothing (`GameState::door_destination`),
/// as a folder name.
fn byte_target(stage: u8, next_room: u8) -> Option<String> {
    if next_room == 0xFF {
        return None;
    }
    if next_room < 0x20 {
        Some(plan_doors::room_id(stage, next_room))
    } else {
        locks::target_dir("100", next_room)
    }
}

/// The `<doors>` entries of room `dir` (the original's directory) in the
/// game: `(aot, next, target)` for every door whose byte would lead
/// elsewhere than the plan says. An entry names its byte when the script
/// arms the slot with more than one.
pub(super) fn door_targets(
    plan: &DoorPlan,
    dir: &str,
    scripts: &Scripts,
) -> Result<Vec<(u8, Option<u8>, String)>> {
    let stage = plan
        .rooms
        .get(dir)
        .with_context(|| format!("room {dir} has no new id"))?
        .stage;
    let mut bytes_of: BTreeMap<u8, BTreeSet<u8>> = BTreeMap::new();
    let mut wanted: BTreeMap<(u8, u8), String> = BTreeMap::new();
    for insn in locks::doors(scripts) {
        let bytes = &insn.bytes;
        ensure!(
            bytes.len() > DOOR_KEY,
            "a short door_aot_set at {:#x}",
            insn.offset
        );
        if !leaves_room(bytes) {
            continue;
        }
        let aot = bytes[DOOR_ID] & 0x7F;
        let next = bytes[DOOR_NEXT_ROOM];
        bytes_of.entry(aot).or_default().insert(next);
        let Some(target) = new_target(plan, dir, bytes) else {
            continue;
        };
        if let Some(other) = wanted.get(&(aot, next))
            && *other != target
        {
            bail!("AOT {aot} with target byte {next:#04x} leads both to {other} and {target}");
        }
        wanted.insert((aot, next), target);
    }
    Ok(wanted
        .into_iter()
        .filter(|((_, next), target)| byte_target(stage, *next).as_deref() != Some(target))
        .map(|((aot, next), target)| {
            let shared = bytes_of[&aot].len() > 1;
            (aot, shared.then_some(next), target)
        })
        .collect())
}

/// Room `dir`'s `room.xml` `text` as the game's room `plan` numbers it: its
/// `origin`, and its `<doors>`.
pub(super) fn patch_room_xml(
    text: &str,
    dir: &str,
    targets: &[(u8, Option<u8>, String)],
) -> Result<String> {
    ensure!(
        crate::room::xml::origin_of(text)?.is_none(),
        "the base's room already names an origin"
    );
    ensure!(
        !text.contains("<doors"),
        "the base's room already has <doors>"
    );
    let start = text.find("<room").context("no <room> element")? + "<room".len();
    let end = text.rfind("</room>").context("no </room>")?;
    let mut out = String::with_capacity(text.len() + 64 * (targets.len() + 1));
    out.push_str(&text[..start]);
    out.push_str(&format!(" origin=\"{dir}\""));
    out.push_str(&text[start..end]);
    if !targets.is_empty() {
        out.push_str("  <doors>\n");
        for (aot, next, target) in targets {
            match next {
                Some(next) => out.push_str(&format!(
                    "    <door aot=\"{aot}\" next=\"0x{next:02X}\" target=\"{target}\"/>\n"
                )),
                None => out.push_str(&format!("    <door aot=\"{aot}\" target=\"{target}\"/>\n")),
            }
        }
        out.push_str("  </doors>\n");
    }
    out.push_str(&text[end..]);
    Ok(out)
}

/// What a `door_aot_set` of room `dir` holds in the game, where it differs
/// from the script: the arrival `(x, y, z, angle, camera)` of a planned door
/// leading somewhere, or that a door the plan lacks leads nowhere now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DoorBytes {
    Arrival {
        x: i16,
        y: i16,
        z: i16,
        angle: i16,
        camera: u8,
    },
    LockedForGood,
    Unchanged,
}

/// What the `door_aot_set` `bytes` in room `dir` should hold in the game.
pub(super) fn expected_door(plan: &DoorPlan, dir: &str, bytes: &[u8]) -> Result<DoorBytes> {
    ensure!(bytes.len() > DOOR_KEY, "a short door_aot_set");
    if !leaves_room(bytes) {
        return Ok(DoorBytes::Unchanged);
    }
    match planned_door(plan, dir, bytes) {
        Some(door) => {
            let Some(target) = &door.target else {
                return Ok(DoorBytes::Unchanged);
            };
            let entrance = target.entrance;
            let word = |value: i32, what: &str| {
                i16::try_from(value).with_context(|| {
                    format!(
                        "{}:{}'s entrance {what} {value} is out of range",
                        target.room, target.door
                    )
                })
            };
            let camera = match entrance.cut {
                Some(cut) => {
                    ensure!(
                        cut <= CAMERA_CUT,
                        "{}:{}'s entrance camera {cut}",
                        target.room,
                        target.door
                    );
                    (bytes[DOOR_CAMERA] & !CAMERA_CUT) | cut
                }
                None => bytes[DOOR_CAMERA],
            };
            Ok(DoorBytes::Arrival {
                x: word(entrance.x, "x")?,
                y: word(entrance.y, "y")?,
                z: word(entrance.z, "z")?,
                angle: word(entrance.d, "facing")?,
                camera,
            })
        }
        None if new_target(plan, dir, bytes).is_none() => Ok(DoorBytes::LockedForGood),
        None => Ok(DoorBytes::Unchanged),
    }
}

/// Room `dir`'s script `scd` with every door's arrival written for the
/// game ([`expected_door`]). Unchanged when doors aren't randomized.
pub(super) fn patch_script(scd: &[u8], dir: &str, game: &Game) -> Result<Vec<u8>> {
    let mut out = scd.to_vec();
    let Some(plan) = plan(game) else {
        return Ok(out);
    };
    let scripts = crate::scd::reader::parse(scd)?;
    for insn in locks::doors(&scripts) {
        let expected = expected_door(plan, dir, &insn.bytes)
            .with_context(|| format!("the door_aot_set at {:#x}", insn.offset))?;
        let at = insn.offset;
        match expected {
            DoorBytes::Arrival {
                x,
                y,
                z,
                angle,
                camera,
            } => {
                out[at + DOOR_CAMERA] = camera;
                for (offset, value) in [(DOOR_X, x), (DOOR_Y, y), (DOOR_Z, z), (DOOR_ANGLE, angle)]
                {
                    out[at + offset..at + offset + 2].copy_from_slice(&value.to_le_bytes());
                }
            }
            DoorBytes::LockedForGood => {
                out[at + DOOR_LOCK] = LOCKED_FOR_GOOD;
                out[at + DOOR_KEY] = LOCKED_FOR_GOOD;
            }
            DoorBytes::Unchanged => {}
        }
    }
    Ok(out)
}

/// What a `door_aot_set` holds as the arrival bytes, to compare with
/// [`DoorBytes::Arrival`].
pub(super) fn held_arrival(bytes: &[u8]) -> DoorBytes {
    let word = |at: usize| i16::from_le_bytes([bytes[at], bytes[at + 1]]);
    DoorBytes::Arrival {
        x: word(DOOR_X),
        y: word(DOOR_Y),
        z: word(DOOR_Z),
        angle: word(DOOR_ANGLE),
        camera: bytes[DOOR_CAMERA],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_targets_decode_like_the_engine() {
        assert_eq!(byte_target(1, 0x0C).as_deref(), Some("10c"));
        assert_eq!(byte_target(9, 0x05).as_deref(), Some("09-05"));
        assert_eq!(byte_target(1, 0x40 | 0x0B).as_deref(), Some("20b"));
        assert_eq!(byte_target(1, 0xFF), None);
    }

    #[test]
    fn room_xml_gains_its_origin_and_doors() {
        let text = "<?xml version=\"1.0\"?>\n<room format=\"3\">\n  <ambient r=\"1\" g=\"2\" b=\"3\"/>\n</room>\n";
        let out = patch_room_xml(
            text,
            "10c",
            &[(1, None, "205".into()), (3, Some(0x45), "01-20".into())],
        )
        .unwrap();
        assert!(out.contains("<room origin=\"10c\" format=\"3\">"), "{out}");
        assert!(out.contains("<door aot=\"1\" target=\"205\"/>"), "{out}");
        assert!(
            out.contains("<door aot=\"3\" next=\"0x45\" target=\"01-20\"/>"),
            "{out}"
        );
        assert!(out.ends_with("  </doors>\n</room>\n"), "{out}");
        assert!(patch_room_xml(&out, "10c", &[]).is_err());
    }
}
