//! Check a generated game from what its pack holds, not from what the planner
//! meant: the rooms' finished scripts decide where the doors go and what the
//! slots give.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, anyhow, bail};

use super::emit::{
    DOOR_ARRIVAL, DOOR_CUT, DOOR_KEY, DOOR_KEY_NONE, DOOR_LOCK, DOOR_LOCKED, DOOR_NEXT_ROOM,
    ENEMY_FLAG, ENEMY_SLOT, ITEM_ITEM, OP_DOOR, OP_ENEMY, OP_ITEM, OP_VARIANT_IF, OP_VARIANT_JUMP,
};
use super::rules::{self, Entrance, RoomRules};
use super::{ids, script_room};
use crate::manifest::Manifest;
use crate::scd::akscr::Region;
use crate::scd::ir::{Block, Decoded, Insn};
use crate::scd::opcode::{system, variant, variant_matches};
use crate::scd::reader;

const OP_IF: u8 = 0x01;
const OP_ELSE: u8 = 0x02;
const OP_CK: u8 = 0x04;
/// The flag bank that holds the randomizer's system flags.
const SYSTEM_BANK: u8 = 5;

/// A door as a room's script sets it up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Door {
    sealed: bool,
    target: u16,
    key: Option<u8>,
    lock_flag: Option<u8>,
    arrival: Entrance,
}

/// One room as its pack entries describe it.
#[derive(Debug)]
struct Room {
    rules: RoomRules,
    doors: BTreeMap<u8, Door>,
    /// `(slot, item)` of the room's listed item slots.
    items: BTreeMap<u8, u8>,
    /// `(id, guard flag, slot)` of the room's enemies.
    enemies: Vec<(u8, u8, u8)>,
}

fn i16_at(bytes: &[u8], at: usize) -> i16 {
    i16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn read_door(room: u16, bytes: &[u8]) -> Door {
    let lock = bytes[DOOR_LOCK];
    let locked = lock & DOOR_LOCKED != 0;
    let sealed = locked && bytes[DOOR_KEY] == DOOR_KEY_NONE;
    Door {
        sealed,
        target: ids::room_of(bytes[DOOR_NEXT_ROOM], (room >> 8) as u8),
        key: (locked && !sealed).then_some(bytes[DOOR_KEY]),
        lock_flag: locked.then_some(lock & 0x3F),
        // A sealed door keeps whatever arrival its record had: nobody arrives.
        arrival: Entrance {
            x: if sealed {
                0
            } else {
                i16_at(bytes, DOOR_ARRIVAL)
            },
            y: if sealed {
                0
            } else {
                i16_at(bytes, DOOR_ARRIVAL + 2)
            },
            z: if sealed {
                0
            } else {
                i16_at(bytes, DOOR_ARRIVAL + 4)
            },
            d: if sealed {
                0
            } else {
                i16_at(bytes, DOOR_ARRIVAL + 6)
            },
            cut: if sealed { 0 } else { bytes[DOOR_CUT] },
        },
    }
}

/// The instructions of `block` that run for the room variant `current`, with
/// no flag tests decided: what the variant tests alone leave.
fn live_insns(block: &Block, current: u8) -> Vec<&Insn> {
    let mut live = Vec::new();
    let mut skip_to = 0;
    for insn in &block.insns {
        if insn.offset < skip_to || !matches!(insn.decoded, Decoded::Command(_)) {
            continue;
        }
        let target = |index: usize| insn.operands.get(index).and_then(|operand| operand.target);
        match insn.op {
            OP_VARIANT_IF if !variant_matches(insn.bytes[1], current) => {
                skip_to = target(1).unwrap_or(usize::MAX);
            }
            OP_VARIANT_JUMP => skip_to = target(0).unwrap_or(usize::MAX),
            _ => live.push(insn),
        }
    }
    live
}

/// The slots of the door and item instructions in `insns` that run on every
/// entry to the room: outside story-flag tests, or in both of a test's arms. A
/// randomizer flag is known, so only its live arm counts.
fn always_set(insns: &[&Insn]) -> BTreeSet<u8> {
    let index_of = |target: Option<usize>| {
        target.map_or(insns.len(), |target| {
            insns
                .iter()
                .position(|insn| insn.offset >= target)
                .unwrap_or(insns.len())
        })
    };
    let mut set = BTreeSet::new();
    let mut at = 0;
    while at < insns.len() {
        let insn = insns[at];
        if matches!(insn.op, OP_DOOR | OP_ITEM) {
            set.insert(insn.bytes[1]);
        }
        at += 1;
        if insn.op != OP_IF {
            continue;
        }
        let split = index_of(insn.operands.first().and_then(|operand| operand.target)).max(at);
        let then = &insns[at..split];
        let (then, otherwise, end): (_, &[&Insn], _) = match then.last() {
            Some(last) if last.op == OP_ELSE => {
                let end = index_of(last.operands.first().and_then(|operand| operand.target));
                let end = end.max(split);
                (&then[..then.len() - 1], &insns[split..end], end)
            }
            _ => (then, &[], split),
        };
        let then_set = always_set(then);
        let otherwise_set = always_set(otherwise);
        let known = then.first().filter(|test| {
            test.op == OP_CK && test.bytes[1] == SYSTEM_BANK && test.bytes[2] >= system::RANDOMIZER
        });
        match known {
            Some(test) if test.bytes[3] == 0 => set.extend(then_set),
            Some(_) => set.extend(otherwise_set),
            None => set.extend(then_set.intersection(&otherwise_set)),
        }
        at = end;
    }
    set
}

fn read_room(room: u16, text: &str, scd: &[u8], game_variant: u8) -> Result<Room> {
    let rules = rules::parse(room, text).context("bad random.xml")?;
    let scripts = reader::parse(scd).context("bad script")?;
    let mut doors = BTreeMap::new();
    let mut items = BTreeMap::new();
    let mut enemies = Vec::new();
    let current = game_variant
        | if room >> 8 > 2 {
            variant::OUTSIDE
        } else {
            variant::MANSION1
        };
    let mut always = BTreeSet::new();
    let blocks = scripts.init.iter().chain(&scripts.main);
    for insn in blocks.flat_map(|block| {
        let live = live_insns(block, current);
        always.extend(always_set(&live));
        live
    }) {
        let bytes = &insn.bytes;
        match insn.op {
            OP_DOOR => {
                let door = read_door(room, bytes);
                if doors.insert(bytes[1], door).is_some_and(|old| old != door) {
                    bail!("door {} is set up two ways", bytes[1]);
                }
            }
            OP_ITEM if rules.items.contains(&bytes[1]) => {
                let item = bytes[ITEM_ITEM];
                if items.insert(bytes[1], item).is_some_and(|old| old != item) {
                    bail!("slot {} gives two items", bytes[1]);
                }
            }
            OP_ENEMY => enemies.push((bytes[1], bytes[ENEMY_FLAG], bytes[ENEMY_SLOT])),
            _ => {}
        }
    }
    for slot in &rules.items {
        if !items.contains_key(slot) {
            bail!("slot {slot} of random.xml is not in the script");
        }
        if !always.contains(slot) {
            bail!("slot {slot} of random.xml depends on a story flag");
        }
    }
    for (aot, door) in &doors {
        if !door.sealed && !always.contains(aot) {
            bail!("door {aot} depends on a story flag");
        }
    }
    Ok(Room {
        rules,
        doors,
        items,
        enemies,
    })
}

/// Check that a door's far side leads back to it and sets the player down at
/// the entrance the rules give for the far door.
fn check_door(id: u16, aot: u8, door: &Door, rooms: &BTreeMap<u16, Room>) -> Result<()> {
    let far = rooms.get(&door.target).ok_or_else(|| {
        anyhow!(
            "door {aot} leads to room {:03x}, not in the pack",
            door.target
        )
    })?;
    let matching = far.doors.iter().find(|(far_aot, far_door)| {
        !far_door.sealed
            && far_door.target == id
            && far.rules.door(**far_aot).and_then(|rule| rule.entrance) == Some(door.arrival)
    });
    let Some((far_aot, far_door)) = matching else {
        bail!("door {aot} to room {:03x} has no door back", door.target);
    };
    let own = rooms[&id].rules.door(aot).and_then(|rule| rule.entrance);
    if own != Some(far_door.arrival) {
        bail!(
            "door {aot} and door {far_aot} of room {:03x} disagree",
            door.target
        );
    }
    Ok(())
}

/// Walk the game from the start room, picking up keys and opening what they
/// open, and return the rooms and items reached.
fn walk(start: u16, rooms: &BTreeMap<u16, Room>) -> (BTreeSet<u16>, BTreeSet<u8>) {
    let mut reached = BTreeSet::from([start]);
    let mut items = BTreeSet::new();
    loop {
        let before = (reached.len(), items.len());
        for id in reached.clone() {
            let room = &rooms[&id];
            items.extend(room.items.values().copied());
            for door in room.doors.values().filter(|door| !door.sealed) {
                if door.key.is_none_or(|key| items.contains(&key)) {
                    reached.insert(door.target);
                }
            }
        }
        if before == (reached.len(), items.len()) {
            return (reached, items);
        }
    }
}

fn check_rooms(start: u16, rooms: &BTreeMap<u16, Room>) -> Result<()> {
    let goals: Vec<u16> = rooms
        .iter()
        .filter(|(_, room)| room.rules.goal)
        .map(|(&id, _)| id)
        .collect();
    let [goal] = goals.as_slice() else {
        bail!("the pack holds {} goal rooms", goals.len());
    };
    if !rooms.contains_key(&start) {
        bail!("the start room {start:03x} is not in the pack");
    }
    if goals.contains(&start) {
        bail!("the start room is the goal");
    }

    let mut guards = BTreeSet::new();
    let mut lock_flags: BTreeMap<u8, u8> = BTreeMap::new();
    let mut key_doors: BTreeMap<u8, usize> = BTreeMap::new();
    for (&id, room) in rooms {
        let context = || format!("room {id:03x}");
        for (&aot, door) in &room.doors {
            if door.sealed {
                if door.lock_flag != Some(ids::SEAL_LOCK_FLAG) {
                    bail!("room {id:03x} door {aot} is sealed by another flag");
                }
                continue;
            }
            check_door(id, aot, door, rooms).with_context(context)?;
            if let (Some(flag), Some(key)) = (door.lock_flag, door.key) {
                if !ids::GATE_LOCK_FLAGS.contains(&flag) {
                    bail!("room {id:03x} door {aot} locks with stray flag {flag:#x}");
                }
                if lock_flags.insert(flag, key).is_some() {
                    bail!("two doors share lock flag {flag:#x}");
                }
                *key_doors.entry(key).or_default() += 1;
            }
        }
        if room.rules.enemies.fixed {
            continue;
        }
        if id == start && !room.enemies.is_empty() {
            bail!("the start room has enemies");
        }
        let mut slots = BTreeSet::new();
        for &(enemy, guard, slot) in &room.enemies {
            if !ids::ENEMIES.iter().any(|known| known.id == enemy) {
                bail!("room {id:03x} has enemy {enemy:#x} from outside the pool");
            }
            if !ids::SPAWN_FLAGS.contains(&guard) || !guards.insert(guard) {
                bail!("room {id:03x} reuses enemy flag {guard:#x}");
            }
            if !slots.insert(slot) {
                bail!("room {id:03x} uses enemy slot {slot} twice");
            }
        }
    }

    let (reached, items) = walk(start, rooms);
    if let Some(lost) = rooms.keys().find(|room| !reached.contains(room)) {
        bail!("room {lost:03x} cannot be reached");
    }
    if !reached.contains(goal) {
        bail!("the goal room cannot be reached");
    }
    if !items.contains(&ids::FLARE) {
        bail!("the flare cannot be reached");
    }
    let placed: Vec<u8> = rooms
        .values()
        .flat_map(|room| room.items.values().copied())
        .collect();
    let count = |item: u8| placed.iter().filter(|&&placed| placed == item).count();
    if count(ids::FLARE) != 1 {
        bail!("the pack holds {} flares", count(ids::FLARE));
    }
    for (key, doors) in key_doors {
        if doors != 1 || count(key) != 1 {
            bail!("key {key:#x} has {} copies and {doors} doors", count(key));
        }
    }
    Ok(())
}

/// Check a generated game: its manifest, and the rooms `read` returns.
pub fn verify_entries(
    manifest: &Manifest,
    paths: &[String],
    read: &dyn Fn(&str) -> Result<Vec<u8>>,
) -> Result<()> {
    if manifest.seed.is_none() || manifest.generator_version.is_none() {
        bail!("the pack's manifest does not name a seed and generator version");
    }
    let start = manifest
        .start_room
        .as_deref()
        .ok_or_else(|| anyhow!("the pack names no start room"))?;
    let start = u16::from_str_radix(start, 16).with_context(|| format!("bad room `{start}`"))?;
    let player = match manifest.character {
        Some(0) => variant::CHRIS,
        Some(1) => variant::JILL,
        other => bail!("the pack's character is {other:?}"),
    };
    let region = match manifest.region {
        Some(Region::Jpn) => variant::JPN,
        Some(Region::Eur) => variant::EUR,
        Some(Region::Usa) | None => variant::USA,
    };
    let mut rooms = BTreeMap::new();
    for id in paths.iter().filter_map(|path| script_room(path)) {
        let context = || format!("room {id:03x}");
        let text = read(&format!("room/{id:03x}/random.xml")).with_context(context)?;
        let text = String::from_utf8(text).with_context(context)?;
        let scd = read(&format!("room/{id:03x}/room.scd")).with_context(context)?;
        rooms.insert(
            id,
            read_room(id, &text, &scd, player | region).with_context(context)?,
        );
    }
    check_rooms(start, &rooms)
}
