//! Turn a [`Plan`] into pack entries: each room's script is rewritten so the
//! room is the planned one, and the rest of the pack comes from the base.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, bail};

use super::ids;
use super::plan::{DoorPlan, Plan, RoomPlan, Spawn};
use crate::scd::asm::Assembled;
use crate::scd::ir::{Block, Decoded, Insn};
use crate::scd::opcode::variant;
use crate::scd::reader;

pub(super) const OP_DOOR: u8 = 0x0C;
pub(super) const OP_ITEM: u8 = 0x18;
pub(super) const OP_ENEMY: u8 = 0x1B;
const OP_POS_SET: u8 = 0x21;
const OP_EML_STATE: u8 = 0x28;
const OP_NOP: u8 = 0x0E;
pub(super) const OP_VARIANT_IF: u8 = 0x51;
pub(super) const OP_VARIANT_JUMP: u8 = 0x52;
const OP_EVT_VARIANT_IF: u8 = 0xF4;

const DOOR_AOT: usize = 1;
pub(super) const DOOR_CUT: usize = 13;
pub(super) const DOOR_LOCK: usize = 14;
pub(super) const DOOR_NEXT_ROOM: usize = 15;
pub(super) const DOOR_ARRIVAL: usize = 16;
pub(super) const DOOR_KEY: usize = 24;
/// The lock bit of a door's lock byte.
pub(super) const DOOR_LOCKED: u8 = 0x80;
/// The key byte of a door no key opens.
pub(super) const DOOR_KEY_NONE: u8 = 0xFF;

pub(super) const ITEM_ITEM: usize = 10;
pub(super) const ITEM_QUANTITY: usize = 11;

pub(super) const ENEMY_FLAG: usize = 3;
pub(super) const ENEMY_SLOT: usize = 18;
/// The original's second record byte for every enemy: its hit words.
const ENEMY_SCA: u8 = 2;
const SPAWN_FORCE: u8 = 1;
const ENEMY_RECORD: usize = 22;

fn put_i16(bytes: &mut [u8], at: usize, value: i16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn seal(room: u16, bytes: &mut [u8]) {
    bytes[DOOR_LOCK] = DOOR_LOCKED | ids::SEAL_LOCK_FLAG;
    bytes[DOOR_KEY] = DOOR_KEY_NONE;
    bytes[DOOR_NEXT_ROOM] = ids::next_room(room);
}

fn connect(door: &DoorPlan, bytes: &mut [u8]) {
    bytes[DOOR_CUT] = door.entrance.cut;
    bytes[DOOR_NEXT_ROOM] = ids::next_room(door.target);
    let arrival = [
        door.entrance.x,
        door.entrance.y,
        door.entrance.z,
        door.entrance.d,
    ];
    for (index, value) in arrival.into_iter().enumerate() {
        put_i16(bytes, DOOR_ARRIVAL + index * 2, value);
    }
    match door.lock {
        Some(lock) => {
            bytes[DOOR_LOCK] = DOOR_LOCKED | lock.flag;
            bytes[DOOR_KEY] = lock.key;
        }
        None => {
            bytes[DOOR_LOCK] = 0;
            bytes[DOOR_KEY] = 0;
        }
    }
}

/// The `enemy` record of one planned enemy.
fn spawn_record(spawn: &Spawn) -> [u8; ENEMY_RECORD] {
    let mut bytes = [0u8; ENEMY_RECORD];
    bytes[0] = OP_ENEMY;
    bytes[1] = spawn.enemy;
    bytes[2] = spawn.behaviour;
    bytes[ENEMY_FLAG] = spawn.flag;
    bytes[4] = SPAWN_FORCE;
    bytes[5] = ENEMY_SCA;
    put_i16(&mut bytes, 8, spawn.d);
    put_i16(&mut bytes, 12, spawn.x);
    put_i16(&mut bytes, 14, spawn.y);
    put_i16(&mut bytes, 16, spawn.z);
    bytes[ENEMY_SLOT] = spawn.slot;
    bytes
}

fn command_insns(blocks: &[Block]) -> impl Iterator<Item = &Insn> {
    blocks
        .iter()
        .flat_map(|block| &block.insns)
        .filter(|insn| matches!(insn.decoded, Decoded::Command(_)))
}

/// Let a second-visit room's scripts run on the first visit: clear the mansion
/// dimension from the tests that ask for the second.
fn open_second_visit(scripts: &crate::scd::ir::Scripts, out: &mut [u8]) -> Result<()> {
    let blocks = scripts
        .init
        .iter()
        .chain(&scripts.main)
        .map(|block| &block.insns);
    let events = scripts.events.iter().map(|event| &event.insns);
    for insn in blocks.chain(events).flatten() {
        let test = match insn.decoded {
            Decoded::Command(_) => insn.op == OP_VARIANT_IF,
            Decoded::Control(_) => insn.op == OP_EVT_VARIANT_IF,
            _ => false,
        };
        if !test {
            continue;
        }
        let mask = &mut out[insn.offset + 1];
        match *mask & variant::MANSION {
            0 => {}
            variant::MANSION2 => *mask &= !variant::MANSION,
            other => bail!("a second-visit room tests mansion bits {other:#x}"),
        }
    }
    Ok(())
}

/// Rewrite one room's script: every door connected or sealed, every item slot
/// its planned pickup, and the room's enemies replaced by the planned ones.
pub fn patch_room(room: u16, scd: &[u8], plan: &RoomPlan) -> Result<Vec<u8>> {
    let scripts = reader::parse(scd).context("the room's script does not parse")?;
    for event in &scripts.events {
        let found = event.insns.iter().find(|insn| {
            matches!(insn.decoded, Decoded::Command(_))
                && matches!(insn.op, OP_DOOR | OP_ITEM)
                && (insn.op == OP_DOOR
                    || plan
                        .items
                        .contains_key(insn.bytes.get(DOOR_AOT).unwrap_or(&0xFF)))
        });
        if let Some(insn) = found {
            bail!("an event script sets up a door or slot: op {:#x}", insn.op);
        }
    }

    let mut out = scd.to_vec();
    if plan.second_visit {
        open_second_visit(&scripts, &mut out)?;
    }
    let mut seen_items = BTreeSet::new();
    for insn in command_insns(&scripts.init).chain(command_insns(&scripts.main)) {
        let bytes = &mut out[insn.offset..insn.offset + insn.bytes.len()];
        match insn.op {
            OP_DOOR => match plan.doors.get(&bytes[DOOR_AOT]) {
                Some(door) => connect(door, bytes),
                None => seal(room, bytes),
            },
            OP_ITEM => {
                if let Some(pickup) = plan.items.get(&bytes[DOOR_AOT]) {
                    seen_items.insert(bytes[DOOR_AOT]);
                    bytes[ITEM_ITEM] = pickup.item;
                    bytes[ITEM_QUANTITY] = pickup.quantity;
                }
            }
            OP_ENEMY | OP_POS_SET | OP_EML_STATE if plan.spawns.is_some() => {
                for nop in bytes.as_chunks_mut::<2>().0 {
                    *nop = [OP_NOP, 0];
                }
            }
            _ => {}
        }
    }
    for slot in plan.items.keys() {
        if !seen_items.contains(slot) {
            bail!("the room's script has no item slot {slot}");
        }
    }

    let spawns = plan.spawns.as_deref().unwrap_or_default();
    if spawns.is_empty() {
        return Ok(out);
    }
    // The new records go first in the init script. Jumps are relative, so the
    // room's instructions keep working when they move.
    let mut scripts = reader::parse(&out).context("the patched script does not parse")?;
    let Some(first) = scripts.init.first_mut() else {
        bail!("the room has no init script");
    };
    let mut records = Vec::with_capacity(spawns.len() * ENEMY_RECORD);
    for spawn in spawns {
        records.extend_from_slice(&spawn_record(spawn));
    }
    let (insns, trailing) = reader::decode_command_block(&records, 0, records.len());
    if !trailing.is_empty() || insns.len() != spawns.len() {
        bail!("the enemy records do not decode");
    }
    first.insns.splice(0..0, insns);
    Assembled {
        init: scripts.init,
        main: scripts.main,
        events: scripts.events,
    }
    .to_container()
}

/// Every script's enemy ids, for keeping just the enemy files the game needs.
pub fn enemies_in(scd: &[u8]) -> Result<BTreeSet<u8>> {
    let scripts = reader::parse(scd)?;
    let events = scripts.events.iter().flat_map(|event| &event.insns);
    Ok(command_insns(&scripts.init)
        .chain(command_insns(&scripts.main))
        .chain(events.filter(|insn| matches!(insn.decoded, Decoded::Command(_))))
        .filter(|insn| insn.op == OP_ENEMY)
        .filter_map(|insn| insn.bytes.get(1).copied())
        .collect())
}

/// The start inventory, in place of the base's `game/new_game.lua`.
pub fn new_game_lua() -> &'static str {
    "-- Generated by BioRand: the start inventory of either character.\n\
     local api, character = ...\n\
     \n\
     api:set_item_slot(0, 0x01, 0)\n\
     api:set_item_slot(1, 0x02, 15)\n\
     api:set_item_slot(2, 0x41, 1)\n"
}

/// The rewritten scripts of every room of `plan`, by pack entry.
pub fn room_scripts(
    plan: &Plan,
    read: &dyn Fn(&str) -> Result<Vec<u8>>,
) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut scripts = BTreeMap::new();
    for (&room, room_plan) in &plan.rooms {
        let entry = format!("room/{room:03x}/room.scd");
        let scd = read(&entry)?;
        let patched = patch_room(room, &scd, room_plan)
            .with_context(|| format!("failed to rewrite room {room:03x}"))?;
        scripts.insert(entry, patched);
    }
    Ok(scripts)
}
