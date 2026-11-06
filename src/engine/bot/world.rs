//! The bot's read-only world model: what the player has observed this run.
//!
//! Built only from the per-tick [`SessionSnapshot`] (room, doors, actions,
//! room items, inventory counts, flags) and the pad input the route asked
//! for. Nothing here writes game state; Lua reads it through `bot:world()`.
//! The planner uses it to pick goals; every rule below is a bot-side
//! inference from observations, not engine bookkeeping.
//!
//! Rules:
//! - A room is *seen* when the bot enters it or a seen door leads to it;
//!   *visited* when the bot has entered it. Entries count room changes.
//! - A door is *locked* when, the first time the bot sees it, its lock byte
//!   has bit `0x80` and its bank-2 lock flag (`lock & 0x3F`) is clear. It is
//!   *unlocked* once that flag is observed set. It is *traversed* when the
//!   room changes to its target right after the bot pressed action on it, or
//!   when it is the only door of the old room that leads to the new one. On
//!   arrival, the new room's door back to the old room (the one nearest the
//!   player, if several) is traversed too: the bot came through its far side.
//! - A *press* is a tick whose input has the action edge while the room is
//!   controllable; it is credited to the first action-key entry (flags `0x80`
//!   and `0x01`, live handler) whose zone holds the engine's reach point
//!   (RE1 reach offset). The press's *result* is the first message that opens
//!   and the flag bits (bank 5 excepted) that rise within
//!   [`RESULT_WINDOW`] ticks in the same room. A new press clears it.
//! - A room item is *taken* when its slot stops being that item action while
//!   the bot stays in the room, and either the item's inventory count rose
//!   that tick or the bot pressed the slot within [`TAKE_WINDOW`] ticks. An
//!   item gone for any other reason (a script, or absent on re-entry) is only
//!   marked not *present*.
//! - The frontier is every present, untraversed door and every present item
//!   not taken, in room then slot order.
//! - Each action keeps its zone centre (`x`, `z`) as last seen, so the
//!   planner can price walks between a room's doors and item boxes.

use std::collections::{BTreeMap, BTreeSet};

use mlua::{Lua, Table};

use super::{BotInput, SessionSnapshot};
use crate::game::{self, FlagBank};

/// Ticks after a press in which a message or rising flag is its result.
pub(super) const RESULT_WINDOW: u64 = 60;
/// Ticks after a press in which a vanished item counts as picked up.
pub(super) const TAKE_WINDOW: u64 = 120;
/// The most rising flags one press records.
const MAX_RESULT_FLAGS: usize = 8;
/// The bank-2 door lock flags.
const BANK_LOCKS: u8 = 2;
/// The main-state bank, which changes every few ticks and is not progress.
const BANK_MAIN_STATE: usize = 5;

/// One remembered door.
#[derive(Clone, Debug, PartialEq, Eq)]
struct DoorRecord {
    target: Option<String>,
    key: u8,
    /// The bank-2 lock bit when the door was locked at first sight.
    lock_bit: Option<u8>,
    present: bool,
    traversed: bool,
    x: i32,
    z: i32,
}

/// One remembered room item.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ItemRecord {
    item: u8,
    quantity: u8,
    present: bool,
    taken: Option<u64>,
    x: i32,
    z: i32,
}

/// One remembered room action and the bot's presses on it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ActionRecord {
    kind: &'static str,
    /// The zone centre when last seen: the mental map's box positions.
    x: i32,
    z: i32,
    presses: u32,
    last_press: Option<u64>,
    message: Option<u16>,
    flags: Vec<(u8, u8)>,
}

/// One room the bot has entered.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct RoomRecord {
    entries: u32,
    last_entry: Option<u64>,
    doors: BTreeMap<u8, DoorRecord>,
    items: BTreeMap<u8, ItemRecord>,
    actions: BTreeMap<u8, ActionRecord>,
}

/// The last press whose result is still being watched.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Pending {
    room: String,
    slot: u8,
    tick: u64,
}

/// The world model. Reset with the bot's memory on a new game.
#[derive(Clone, Debug, Default)]
pub(super) struct World {
    rooms: BTreeMap<String, RoomRecord>,
    current: Option<String>,
    /// The action-key slot the reach point is in this tick, if controllable.
    probe: Option<u8>,
    prev_flags: Option<[FlagBank; game::FLAG_BANK_COUNT]>,
    prev_counts: BTreeMap<u8, u32>,
    pending: Option<Pending>,
}

/// The zone centre of an `x, z, width, depth` box.
fn centre(x: i32, z: i32, width: i32, depth: i32) -> (i32, i32) {
    (x + width / 2, z + depth / 2)
}

/// The engine's unsigned zone test ([`crate::game::RoomAction::contains`]).
fn zone_contains(origin: (i32, i32), size: (i32, i32), x: i32, z: i32) -> bool {
    let inside = |value: i32, origin: i32, size: i32| {
        (value as u32).wrapping_sub(u32::from(origin as i16 as u16))
            <= u32::from(size as i16 as u16)
    };
    inside(x, origin.0, size.0) && inside(z, origin.1, size.1)
}

/// Mark the door of the room just entered that leads back to `from`, the
/// room just left, as traversed: the bot came through its other side, so
/// both ends are known. With several such doors, the one nearest the
/// player's arrival point `(x, z)`.
fn mark_arrival(room: &mut RoomRecord, from: &str, x: i32, z: i32) {
    let distance = |door: &DoorRecord| {
        let (dx, dz) = (i64::from(door.x - x), i64::from(door.z - z));
        dx * dx + dz * dz
    };
    if let Some(door) = room
        .doors
        .values_mut()
        .filter(|door| door.present && door.target.as_deref() == Some(from))
        .min_by_key(|door| distance(door))
    {
        door.traversed = true;
    }
}

fn counts(snapshot: &SessionSnapshot) -> BTreeMap<u8, u32> {
    let mut counts = BTreeMap::new();
    for stack in snapshot.inventory.iter().filter(|stack| stack.id != 0) {
        *counts.entry(stack.id).or_insert(0) += u32::from(stack.quantity);
    }
    counts
}

impl World {
    /// Fold one tick's snapshot into the model. Call before the route runs.
    pub(super) fn observe(&mut self, tick: u64, snapshot: &SessionSnapshot) {
        // Mid door load the room id is the destination's but the room state
        // is not yet; the entry is counted on the first booted tick.
        if !snapshot.booted {
            return;
        }
        let key = snapshot.room.room3();
        let entered = self.current.as_deref() != Some(key.as_str());
        let mut arrived_from = None;
        if entered {
            if let Some(previous) = self.current.take() {
                self.mark_traversed(&previous, &key);
                arrived_from = Some(previous);
            }
            let room = self.rooms.entry(key.clone()).or_default();
            room.entries += 1;
            room.last_entry = Some(tick);
            self.current = Some(key.clone());
            self.pending = None;
        }
        let counts = counts(snapshot);
        let pending = self
            .pending
            .clone()
            .filter(|pending| pending.room == key && tick - pending.tick <= RESULT_WINDOW);
        let room = self.rooms.entry(key.clone()).or_default();

        for door in room.doors.values_mut() {
            door.present = false;
        }
        for door in &snapshot.doors {
            let (x, z) = centre(door.x, door.z, door.width, door.depth);
            let record = room.doors.entry(door.slot).or_insert_with(|| DoorRecord {
                target: None,
                key: door.key,
                lock_bit: (door.lock & 0x80 != 0 && !snapshot.flag(BANK_LOCKS, door.lock & 0x3F))
                    .then_some(door.lock & 0x3F),
                present: true,
                traversed: false,
                x,
                z,
            });
            record.target = door.target.clone();
            record.key = door.key;
            // A room script can rewrite the lock byte: a door seen without
            // its lock bit is unlocked outright, and one that gains a lock
            // while its flag is clear is locked again.
            if door.lock & 0x80 == 0 {
                record.lock_bit = None;
            } else if record.lock_bit.is_none() && !snapshot.flag(BANK_LOCKS, door.lock & 0x3F) {
                record.lock_bit = Some(door.lock & 0x3F);
            }
            record.present = true;
            record.x = x;
            record.z = z;
        }
        if let Some(from) = arrived_from {
            mark_arrival(room, &from, snapshot.player.x, snapshot.player.z);
        }

        let mut present = BTreeSet::new();
        for action in &snapshot.actions {
            let record = room.actions.entry(action.slot).or_default();
            record.kind = action.kind;
            (record.x, record.z) = centre(action.x, action.z, action.width, action.depth);
            let (Some(item), Some(quantity)) = (action.item, action.quantity) else {
                continue;
            };
            present.insert(action.slot);
            let (x, z) = centre(action.x, action.z, action.width, action.depth);
            let record = room.items.entry(action.slot).or_insert(ItemRecord {
                item,
                quantity,
                present: true,
                taken: None,
                x,
                z,
            });
            if record.item != item {
                // The slot was re-armed with another item: a new record.
                record.taken = None;
            }
            record.item = item;
            record.quantity = quantity;
            record.present = true;
            record.x = x;
            record.z = z;
        }
        for (slot, record) in &mut room.items {
            if present.contains(slot) || !record.present {
                continue;
            }
            record.present = false;
            if entered {
                continue;
            }
            let rose = counts.get(&record.item).copied().unwrap_or(0)
                > self.prev_counts.get(&record.item).copied().unwrap_or(0);
            let pressed = room
                .actions
                .get(slot)
                .and_then(|action| action.last_press)
                .is_some_and(|last| tick - last <= TAKE_WINDOW);
            if rose || pressed {
                record.taken = Some(tick);
            }
        }

        if let Some(pending) = pending {
            let action = room.actions.entry(pending.slot).or_default();
            if action.message.is_none() {
                action.message = snapshot.scene.message;
            }
            if let Some(prev) = &self.prev_flags {
                record_rising(action, prev, &snapshot.flags);
            }
        }

        self.probe = snapshot
            .scene
            .controllable
            .then(|| probe_slot(snapshot))
            .flatten();
        self.prev_flags = Some(snapshot.flags);
        self.prev_counts = counts;
    }

    /// Credit this tick's action press, if any. Call after the route ran.
    pub(super) fn observe_input(&mut self, tick: u64, input: &BotInput) {
        if !input.input.action_pressed {
            return;
        }
        let (Some(slot), Some(room)) = (self.probe, self.current.clone()) else {
            return;
        };
        let action = self
            .rooms
            .entry(room.clone())
            .or_default()
            .actions
            .entry(slot)
            .or_default();
        action.presses += 1;
        action.last_press = Some(tick);
        action.message = None;
        action.flags.clear();
        self.pending = Some(Pending { room, slot, tick });
    }

    /// Mark the door of `from` the bot left through to reach `to`.
    fn mark_traversed(&mut self, from: &str, to: &str) {
        let pressed = self
            .pending
            .as_ref()
            .filter(|pending| pending.room == from)
            .map(|pending| pending.slot);
        let Some(room) = self.rooms.get_mut(from) else {
            return;
        };
        let leads = |door: &DoorRecord| door.target.as_deref() == Some(to);
        let slot = match pressed {
            Some(slot) if room.doors.get(&slot).is_some_and(leads) => Some(slot),
            _ => {
                let mut matching = room.doors.iter().filter(|(_, door)| leads(door));
                match (matching.next(), matching.next()) {
                    (Some((slot, _)), None) => Some(*slot),
                    _ => None,
                }
            }
        };
        if let Some(door) = slot.and_then(|slot| room.doors.get_mut(&slot)) {
            door.traversed = true;
        }
    }

    fn visited(&self, room: &str) -> bool {
        self.rooms.get(room).is_some_and(|room| room.entries > 0)
    }

    /// The `bot:world()` table (see the module docs and mods/bot/README.md).
    pub(super) fn to_lua(&self, lua: &Lua, snapshot: &SessionSnapshot) -> mlua::Result<Table> {
        let locked = |door: &DoorRecord| {
            door.lock_bit
                .is_some_and(|bit| !snapshot.flag(BANK_LOCKS, bit))
        };
        // Every room entered plus every door target, sorted by id.
        let mut seen: BTreeSet<&str> = self.rooms.keys().map(String::as_str).collect();
        for room in self.rooms.values() {
            seen.extend(
                room.doors
                    .values()
                    .filter_map(|door| door.target.as_deref()),
            );
        }
        let table = lua.create_table()?;
        table.set("room", self.current.as_deref())?;
        let rooms = lua.create_table()?;
        let frontier = lua.create_table()?;
        for id in seen {
            let row = lua.create_table()?;
            row.set("id", id)?;
            let record = self.rooms.get(id);
            row.set("visited", self.visited(id))?;
            row.set("entries", record.map_or(0, |room| room.entries))?;
            row.set("last_entry", record.and_then(|room| room.last_entry))?;
            let doors = lua.create_table()?;
            let items = lua.create_table()?;
            let actions = lua.create_table()?;
            for (slot, door) in record.iter().flat_map(|room| &room.doors) {
                let target_visited = door.target.as_deref().is_some_and(|t| self.visited(t));
                let door_row = lua.create_table()?;
                door_row.set("slot", *slot)?;
                door_row.set("target", door.target.as_deref())?;
                door_row.set("target_visited", target_visited)?;
                door_row.set("key", door.key)?;
                door_row.set("locked", door.lock_bit.is_some())?;
                door_row.set("unlocked", door.lock_bit.is_some() && !locked(door))?;
                door_row.set("traversed", door.traversed)?;
                door_row.set("present", door.present)?;
                door_row.set("x", door.x)?;
                door_row.set("z", door.z)?;
                doors.push(door_row)?;
                if door.present && !door.traversed {
                    let goal = lua.create_table()?;
                    goal.set("kind", "door")?;
                    goal.set("room", id)?;
                    goal.set("slot", *slot)?;
                    goal.set("target", door.target.as_deref())?;
                    goal.set("target_visited", target_visited)?;
                    goal.set("key", door.key)?;
                    goal.set("locked", locked(door))?;
                    goal.set("x", door.x)?;
                    goal.set("z", door.z)?;
                    frontier.push(goal)?;
                }
            }
            for (slot, item) in record.iter().flat_map(|room| &room.items) {
                let item_row = lua.create_table()?;
                item_row.set("slot", *slot)?;
                item_row.set("item", item.item)?;
                item_row.set("item_kind", snapshot.item_kind(item.item))?;
                item_row.set("quantity", item.quantity)?;
                item_row.set("present", item.present)?;
                item_row.set("taken", item.taken.is_some())?;
                item_row.set("taken_tick", item.taken)?;
                item_row.set("x", item.x)?;
                item_row.set("z", item.z)?;
                items.push(item_row)?;
                if item.present && item.taken.is_none() {
                    let goal = lua.create_table()?;
                    goal.set("kind", "item")?;
                    goal.set("room", id)?;
                    goal.set("slot", *slot)?;
                    goal.set("item", item.item)?;
                    goal.set("item_kind", snapshot.item_kind(item.item))?;
                    goal.set("quantity", item.quantity)?;
                    goal.set("x", item.x)?;
                    goal.set("z", item.z)?;
                    frontier.push(goal)?;
                }
            }
            for (slot, action) in record.iter().flat_map(|room| &room.actions) {
                let action_row = lua.create_table()?;
                action_row.set("slot", *slot)?;
                action_row.set("kind", action.kind)?;
                action_row.set("x", action.x)?;
                action_row.set("z", action.z)?;
                action_row.set("presses", action.presses)?;
                action_row.set("last_press", action.last_press)?;
                action_row.set("message", action.message)?;
                let flags = lua.create_table()?;
                for (bank, bit) in &action.flags {
                    let flag = lua.create_table()?;
                    flag.set("bank", *bank)?;
                    flag.set("bit", *bit)?;
                    flags.push(flag)?;
                }
                action_row.set("flags", flags)?;
                actions.push(action_row)?;
            }
            row.set("doors", doors)?;
            row.set("items", items)?;
            row.set("actions", actions)?;
            rooms.push(row)?;
        }
        table.set("rooms", rooms)?;
        table.set("frontier", frontier)?;
        Ok(table)
    }
}

/// The first action-key entry whose zone holds the reach point, as
/// `probe_actions` picks it on an action press.
fn probe_slot(snapshot: &SessionSnapshot) -> Option<u8> {
    let player = &snapshot.player;
    let (dx, dz) = crate::player::reach_offset(player.angle);
    let (x, z) = (player.x + dx, player.z + dz);
    snapshot
        .actions
        .iter()
        .filter(|action| action.handler != 0 && action.flags & 0x81 == 0x81)
        .find(|action| zone_contains((action.x, action.z), (action.width, action.depth), x, z))
        .map(|action| action.slot)
}

/// Append the flag selectors that rose from `prev` to `now` (bank 5 is left
/// out), up to [`MAX_RESULT_FLAGS`].
fn record_rising(
    action: &mut ActionRecord,
    prev: &[FlagBank; game::FLAG_BANK_COUNT],
    now: &[FlagBank; game::FLAG_BANK_COUNT],
) {
    for (bank, (before, after)) in prev.iter().zip(now).enumerate() {
        if bank == BANK_MAIN_STATE || before == after {
            continue;
        }
        for bit in 0..=u8::MAX {
            if action.flags.len() >= MAX_RESULT_FLAGS {
                return;
            }
            let rose = !before.bit(bit) && after.bit(bit);
            if rose && !action.flags.contains(&(bank as u8, bit)) {
                action.flags.push((bank as u8, bit));
            }
        }
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::super::{
        ActionSnapshot, DoorSnapshot, MenuSnapshot, PlayerSnapshot, SaveSnapshot, SceneSnapshot,
        WeaponSnapshot,
    };
    use super::*;
    use crate::game::InventoryItem;
    use crate::state::RoomId;

    fn room(code: u16) -> RoomId {
        RoomId {
            stage: (code >> 8) as u8,
            room: (code & 0xFF) as u8,
            player_flag: 1,
            mansion2: false,
        }
    }

    /// A controllable snapshot in room `code`, the player at `(x, z)` facing
    /// angle 0.
    pub(in crate::engine::bot) fn snapshot(code: u16, x: i32, z: i32) -> SessionSnapshot {
        SessionSnapshot {
            room: room(code),
            booted: true,
            camera: 0,
            player: PlayerSnapshot {
                x,
                y: 0,
                z,
                angle: 0,
                health: 100,
                max_health: 100,
                state: 0,
                being_attacked: 0,
                locked: false,
                equipped: None,
                radius: 400,
                collision_flags: 0,
                grabbed: false,
                health_status: 0,
            },
            scene: SceneSnapshot {
                mode: "play",
                skippable: false,
                film: false,
                transition: false,
                cutscene_skip: false,
                message: None,
                message_takes_input: false,
                message_waiting: false,
                prompt: false,
                prompt_yes: true,
                reader: false,
                pickup: false,
                frozen: false,
                controls_locked: false,
                controllable: true,
                dead: false,
                ending: false,
                typewriter: false,
            },
            actions: Vec::new(),
            doors: Vec::new(),
            objects: Vec::new(),
            enemies: Vec::new(),
            weapon: WeaponSnapshot {
                equipped: None,
                weapon: 0,
                clip: 0,
                reserve: 0,
                behavior: 0,
                state: 0,
                aim_state: 0,
                target: None,
            },
            menu: MenuSnapshot::default(),
            flags: [FlagBank::new(); game::FLAG_BANK_COUNT],
            inventory: Vec::new(),
            cameras: Vec::new(),
            save_screen: None,
            saves: SaveSnapshot::default(),
            item_defs: crate::item_catalog::ItemCatalog::embedded(),
            drug_store: false,
            computer: None,
        }
    }

    /// An action-key entry with a 2000-unit zone around `(x, z)`.
    fn action(slot: u8, kind: &'static str, x: i32, z: i32) -> ActionSnapshot {
        ActionSnapshot {
            slot,
            kind,
            x: x - 1000,
            z: z - 1000,
            width: 2000,
            depth: 2000,
            item: None,
            quantity: None,
            next_room: None,
            key: None,
            lock: None,
            handler: 1,
            flags: 0x81,
            desk: None,
        }
    }

    fn item(slot: u8, id: u8, x: i32, z: i32) -> ActionSnapshot {
        ActionSnapshot {
            item: Some(id),
            quantity: Some(1),
            ..action(slot, "item", x, z)
        }
    }

    fn door(slot: u8, target: &str, lock: u8, x: i32, z: i32) -> DoorSnapshot {
        DoorSnapshot {
            slot,
            target: Some(target.to_string()),
            next_room: 0,
            key: 0x30,
            lock,
            x: x - 1000,
            z: z - 1000,
            width: 2000,
            depth: 2000,
        }
    }

    fn press() -> BotInput {
        let mut input = BotInput::default();
        input.press_action();
        input
    }

    fn set_flag(snapshot: &mut SessionSnapshot, bank: u8, bit: u8) {
        snapshot.flags[usize::from(bank)].apply(bit, 0);
    }

    /// The player standing 600 units short (along -Z) of `(x, z)` facing +Z
    /// has the reach point at `(x, z)`; angle 0 points along +Z.
    fn reach_at(x: i32, z: i32) -> (i32, i32) {
        let (dx, dz) = crate::player::reach_offset(0);
        (x - dx, z - dz)
    }

    fn world_table(world: &World, snapshot: &SessionSnapshot) -> (Lua, Table) {
        let lua = Lua::new();
        let table = world.to_lua(&lua, snapshot).unwrap();
        (lua, table)
    }

    fn room_row(table: &Table, id: &str) -> Table {
        let rooms: Table = table.get("rooms").unwrap();
        rooms
            .sequence_values::<Table>()
            .map(Result::unwrap)
            .find(|row| row.get::<String>("id").unwrap() == id)
            .unwrap_or_else(|| panic!("room {id} is not in the world"))
    }

    fn row(rows: &Table, slot: u8) -> Table {
        rows.sequence_values::<Table>()
            .map(Result::unwrap)
            .find(|row| row.get::<u8>("slot").unwrap() == slot)
            .unwrap()
    }

    #[test]
    fn rooms_count_entries_and_include_door_targets() {
        let mut world = World::default();
        let mut a = snapshot(0x106, 5000, 5000);
        a.doors.push(door(1, "107", 0, 9000, 9000));
        world.observe(1, &a);
        world.observe(2, &a);
        world.observe(3, &snapshot(0x107, 5000, 5000));
        world.observe(9, &a);

        let (_lua, table) = world_table(&world, &a);
        assert_eq!(table.get::<String>("room").unwrap(), "106");
        let rooms: Table = table.get("rooms").unwrap();
        assert_eq!(rooms.raw_len(), 2);
        let first = room_row(&table, "106");
        assert!(first.get::<bool>("visited").unwrap());
        assert_eq!(first.get::<u32>("entries").unwrap(), 2);
        assert_eq!(first.get::<u64>("last_entry").unwrap(), 9);

        // A door target the bot never entered is seen but not visited.
        let mut world = World::default();
        world.observe(1, &a);
        let (_lua, table) = world_table(&world, &a);
        let target = room_row(&table, "107");
        assert!(!target.get::<bool>("visited").unwrap());
        assert_eq!(target.get::<u32>("entries").unwrap(), 0);
        assert_eq!(target.get::<Option<u64>>("last_entry").unwrap(), None);
    }

    #[test]
    fn a_door_load_tick_does_not_copy_the_source_rooms_doors() {
        // The engine switches the room id at the door load's placement and
        // boots the destination's doors at its `room_set`, a frame later on a
        // stage change. That frame carries the destination id with the
        // source room's doors and items.
        let mut world = World::default();
        let mut source = snapshot(0x204, 5000, 5000);
        for (slot, target) in ["111", "112", "202", "21A", "219"].iter().enumerate() {
            source
                .doors
                .push(door(slot as u8, target, 0, 2000 * slot as i32, 9000));
        }
        source.actions.push(item(7, 0x44, 20000, 20000));
        world.observe(1, &source);

        let mut loading = snapshot(0x112, 5000, 5000);
        loading.booted = false;
        loading.doors = source.doors.clone();
        loading.actions = source.actions.clone();
        world.observe(2, &loading);

        let mut dest = snapshot(0x112, 5000, 5000);
        dest.doors.push(door(0, "204", 0, 1000, 1000));
        dest.doors.push(door(1, "113", 0, 3000, 3000));
        world.observe(3, &dest);
        world.observe(4, &dest);

        let (_lua, table) = world_table(&world, &dest);
        let room = room_row(&table, "112");
        assert_eq!(room.get::<u32>("entries").unwrap(), 1);
        assert_eq!(
            room.get::<u64>("last_entry").unwrap(),
            3,
            "entered when booted"
        );
        let doors: Table = room.get("doors").unwrap();
        let slots: Vec<u8> = doors
            .sequence_values::<Table>()
            .map(|row| row.unwrap().get::<u8>("slot").unwrap())
            .collect();
        assert_eq!(slots, [0, 1], "only the destination's own doors");
        assert_eq!(row(&doors, 0).get::<String>("target").unwrap(), "204");
        let items: Table = room.get("items").unwrap();
        assert_eq!(items.raw_len(), 0, "no source room item");
        // The source room's doors are untouched, and its only door to 112
        // is credited.
        let doors: Table = room_row(&table, "204").get("doors").unwrap();
        assert_eq!(doors.raw_len(), 5);
        assert!(row(&doors, 1).get::<bool>("traversed").unwrap());
    }

    #[test]
    fn a_locked_door_unlocks_when_its_lock_flag_rises() {
        let mut world = World::default();
        let mut a = snapshot(0x106, 5000, 5000);
        a.doors.push(door(1, "107", 0x80 | 5, 9000, 9000));
        a.doors.push(door(2, "108", 0, 3000, 3000));
        world.observe(1, &a);
        let (_lua, table) = world_table(&world, &a);
        let doors: Table = room_row(&table, "106").get("doors").unwrap();
        let locked = row(&doors, 1);
        assert!(locked.get::<bool>("locked").unwrap());
        assert!(!locked.get::<bool>("unlocked").unwrap());
        assert!(!row(&doors, 2).get::<bool>("locked").unwrap());

        set_flag(&mut a, 2, 5);
        world.observe(2, &a);
        let (_lua, table) = world_table(&world, &a);
        let doors: Table = room_row(&table, "106").get("doors").unwrap();
        let door = row(&doors, 1);
        assert!(door.get::<bool>("locked").unwrap(), "locked at first sight");
        assert!(door.get::<bool>("unlocked").unwrap());

        // A door first seen with its flag already set was never locked.
        let mut world = World::default();
        world.observe(1, &a);
        let (_lua, table) = world_table(&world, &a);
        let doors: Table = room_row(&table, "106").get("doors").unwrap();
        assert!(!row(&doors, 1).get::<bool>("locked").unwrap());
    }

    #[test]
    fn a_door_follows_a_rewritten_lock_byte() {
        let mut world = World::default();
        let mut a = snapshot(0x101, 5000, 5000);
        a.doors.push(door(2, "103", 0, 9000, 9000));
        world.observe(1, &a);
        let locked = |world: &World, a: &SessionSnapshot| {
            let (_lua, table) = world_table(world, a);
            let doors: Table = room_row(&table, "101").get("doors").unwrap();
            let door = row(&doors, 2);
            door.get::<bool>("locked").unwrap() && !door.get::<bool>("unlocked").unwrap()
        };
        assert!(!locked(&world, &a));
        // A room script arms the lock while its flag is clear.
        a.doors[0].lock = 0x80 | 9;
        world.observe(2, &a);
        assert!(locked(&world, &a));
        // ...and later removes the lock bit again.
        a.doors[0].lock = 9;
        world.observe(3, &a);
        assert!(!locked(&world, &a));
    }

    #[test]
    fn the_pressed_door_is_traversed_on_the_room_change() {
        let mut world = World::default();
        let (x, z) = reach_at(9000, 9000);
        let mut a = snapshot(0x106, x, z);
        a.actions.push(action(1, "door", 9000, 9000));
        a.actions.push(action(2, "door", 3000, 3000));
        a.doors.push(door(1, "107", 0, 9000, 9000));
        a.doors.push(door(2, "107", 0, 3000, 3000));
        world.observe(1, &a);
        world.observe_input(1, &press());
        world.observe(2, &snapshot(0x107, 0, 0));

        let (_lua, table) = world_table(&world, &a);
        let doors: Table = room_row(&table, "106").get("doors").unwrap();
        assert!(row(&doors, 1).get::<bool>("traversed").unwrap());
        assert!(!row(&doors, 2).get::<bool>("traversed").unwrap());
        assert!(row(&doors, 2).get::<bool>("target_visited").unwrap());
        let actions: Table = room_row(&table, "106").get("actions").unwrap();
        assert_eq!(row(&actions, 1).get::<u32>("presses").unwrap(), 1);
        assert_eq!(row(&actions, 1).get::<u64>("last_press").unwrap(), 1);
        assert_eq!(row(&actions, 2).get::<u32>("presses").unwrap(), 0);

        // Without a press, the only door to the new room is the one used.
        let mut world = World::default();
        let mut b = snapshot(0x106, 0, 0);
        b.doors.push(door(1, "107", 0, 9000, 9000));
        b.doors.push(door(2, "108", 0, 3000, 3000));
        world.observe(1, &b);
        world.observe(2, &snapshot(0x108, 0, 0));
        let (_lua, table) = world_table(&world, &b);
        let doors: Table = room_row(&table, "106").get("doors").unwrap();
        assert!(!row(&doors, 1).get::<bool>("traversed").unwrap());
        assert!(row(&doors, 2).get::<bool>("traversed").unwrap());
    }

    #[test]
    fn the_door_back_to_the_room_left_is_traversed_on_arrival() {
        let mut world = World::default();
        let mut a = snapshot(0x106, 0, 0);
        a.doors.push(door(1, "107", 0, 9000, 9000));
        world.observe(1, &a);
        // 107 has two doors back to 106 and one onward; the player arrives
        // next to slot 2.
        let mut b = snapshot(0x107, 3200, 3100);
        b.doors.push(door(1, "106", 0, 20000, 20000));
        b.doors.push(door(2, "106", 0, 3000, 3000));
        b.doors.push(door(3, "108", 0, 3500, 3500));
        world.observe(2, &b);

        let (_lua, table) = world_table(&world, &b);
        let doors: Table = room_row(&table, "107").get("doors").unwrap();
        assert!(!row(&doors, 1).get::<bool>("traversed").unwrap());
        assert!(row(&doors, 2).get::<bool>("traversed").unwrap());
        assert!(!row(&doors, 3).get::<bool>("traversed").unwrap());
        let frontier: Table = table.get("frontier").unwrap();
        let slots: Vec<(String, u8)> = frontier
            .sequence_values::<Table>()
            .map(|goal| {
                let goal = goal.unwrap();
                (goal.get("room").unwrap(), goal.get("slot").unwrap())
            })
            .collect();
        assert!(!slots.contains(&("107".to_string(), 2)));
        assert!(slots.contains(&("107".to_string(), 3)));
    }

    #[test]
    fn a_press_outside_every_zone_or_while_not_controllable_is_not_credited() {
        let mut world = World::default();
        let mut a = snapshot(0x106, 20000, 20000);
        a.actions.push(action(1, "event", 9000, 9000));
        world.observe(1, &a);
        world.observe_input(1, &press());

        let (x, z) = reach_at(9000, 9000);
        let mut b = snapshot(0x106, x, z);
        b.actions.push(action(1, "event", 9000, 9000));
        b.scene.controllable = false;
        world.observe(2, &b);
        world.observe_input(2, &press());

        // Holding action without the edge is not a press either.
        b.scene.controllable = true;
        world.observe(3, &b);
        let mut held = BotInput::default();
        held.set("action", true);
        world.observe_input(3, &held);

        let (_lua, table) = world_table(&world, &b);
        let actions: Table = room_row(&table, "106").get("actions").unwrap();
        assert_eq!(row(&actions, 1).get::<u32>("presses").unwrap(), 0);
    }

    #[test]
    fn a_press_records_the_message_and_rising_flags_it_caused() {
        let mut world = World::default();
        let (x, z) = reach_at(9000, 9000);
        let mut a = snapshot(0x117, x, z);
        a.actions.push(action(3, "event", 9000, 9000));
        world.observe(1, &a);
        world.observe_input(1, &press());

        a.scene.message = Some(0x21);
        a.scene.controllable = false;
        set_flag(&mut a, 4, 0x1A);
        set_flag(&mut a, 5, 0x01);
        world.observe(2, &a);
        a.scene.message = Some(0x22);
        world.observe(3, &a);
        // Past the window, a rising flag is not this press's result.
        let mut late = snapshot(0x117, x, z);
        late.actions = a.actions.clone();
        late.flags = a.flags;
        set_flag(&mut late, 0, 0x03);
        world.observe(3 + RESULT_WINDOW, &late);

        let (_lua, table) = world_table(&world, &late);
        let actions: Table = room_row(&table, "117").get("actions").unwrap();
        let painting = row(&actions, 3);
        assert_eq!(painting.get::<String>("kind").unwrap(), "event");
        assert_eq!(painting.get::<u16>("message").unwrap(), 0x21);
        let flags: Table = painting.get("flags").unwrap();
        assert_eq!(flags.raw_len(), 1, "bank 5 is not recorded");
        let flag: Table = flags.get(1).unwrap();
        assert_eq!(flag.get::<u8>("bank").unwrap(), 4);
        assert_eq!(flag.get::<u8>("bit").unwrap(), 0x1A);
    }

    #[test]
    fn an_item_is_taken_when_it_vanishes_after_a_press_or_a_count_rise() {
        let mut world = World::default();
        let (x, z) = reach_at(9000, 9000);
        let mut a = snapshot(0x106, x, z);
        a.actions.push(item(4, 0x30, 9000, 9000));
        a.actions.push(item(5, 0x31, 20000, 20000));
        a.actions.push(item(6, 0x32, 25000, 25000));
        world.observe(1, &a);
        world.observe_input(1, &press());

        // Slot 4 vanishes after the press (a map: no inventory change).
        a.actions.retain(|action| action.slot != 4);
        world.observe(30, &a);
        // Slot 5 vanishes as its item enters the inventory.
        a.actions.retain(|action| action.slot != 5);
        a.inventory.push(InventoryItem {
            id: 0x31,
            quantity: 1,
        });
        world.observe(31, &a);
        // Slot 6 vanishes with neither: a script removed it.
        a.actions.retain(|action| action.slot != 6);
        world.observe(32, &a);

        let (_lua, table) = world_table(&world, &a);
        let items: Table = room_row(&table, "106").get("items").unwrap();
        let taken = |slot| row(&items, slot).get::<bool>("taken").unwrap();
        let present = |slot| row(&items, slot).get::<bool>("present").unwrap();
        assert!(taken(4) && taken(5));
        assert_eq!(row(&items, 4).get::<u64>("taken_tick").unwrap(), 30);
        assert!(!taken(6));
        assert!(!present(4) && !present(5) && !present(6));
        let frontier: Table = table.get("frontier").unwrap();
        assert_eq!(frontier.raw_len(), 0);
    }

    #[test]
    fn an_item_absent_on_reentry_is_not_taken() {
        let mut world = World::default();
        let mut a = snapshot(0x106, 0, 0);
        a.actions.push(item(4, 0x30, 9000, 9000));
        world.observe(1, &a);
        world.observe(2, &snapshot(0x107, 0, 0));
        let mut back = snapshot(0x106, 0, 0);
        back.inventory.push(InventoryItem {
            id: 0x30,
            quantity: 1,
        });
        world.observe(3, &back);

        let (_lua, table) = world_table(&world, &back);
        let items: Table = room_row(&table, "106").get("items").unwrap();
        assert!(!row(&items, 4).get::<bool>("taken").unwrap());
        assert!(!row(&items, 4).get::<bool>("present").unwrap());
    }

    #[test]
    fn the_frontier_lists_untraversed_doors_and_untaken_items_in_order() {
        let mut world = World::default();
        let mut a = snapshot(0x106, 0, 0);
        a.doors.push(door(2, "108", 0x80 | 7, 3000, 3000));
        a.doors.push(door(1, "107", 0, 9000, 9000));
        a.actions.push(item(4, 0x30, 12000, 14000));
        world.observe(1, &a);
        world.observe(2, &snapshot(0x107, 0, 0));
        let mut b = snapshot(0x107, 0, 0);
        b.actions.push(item(3, 0x40, 1000, 1000));
        world.observe(3, &b);

        let (_lua, table) = world_table(&world, &b);
        let frontier: Table = table.get("frontier").unwrap();
        let goals: Vec<(String, String, u8, bool)> = frontier
            .sequence_values::<Table>()
            .map(Result::unwrap)
            .map(|goal| {
                (
                    goal.get("kind").unwrap(),
                    goal.get("room").unwrap(),
                    goal.get("slot").unwrap(),
                    goal.get::<Option<bool>>("locked").unwrap().unwrap_or(false),
                )
            })
            .collect();
        assert_eq!(
            goals,
            vec![
                ("door".to_string(), "106".to_string(), 2, true),
                ("item".to_string(), "106".to_string(), 4, false),
                ("item".to_string(), "107".to_string(), 3, false),
            ]
        );
        let item_goal: Table = frontier.get(2).unwrap();
        assert_eq!(item_goal.get::<i32>("x").unwrap(), 12000);
        assert_eq!(item_goal.get::<i32>("z").unwrap(), 14000);
        assert_eq!(item_goal.get::<u8>("item").unwrap(), 0x30);
        // The catalog kind rides along (0x30 is the lighter, a special item).
        assert_eq!(item_goal.get::<String>("item_kind").unwrap(), "special");
        assert_eq!(item_goal.get::<String>("kind").unwrap(), "item");
    }

    #[test]
    fn the_world_is_deterministic() {
        let build = || {
            let mut world = World::default();
            let mut a = snapshot(0x106, 0, 0);
            a.doors.push(door(1, "107", 0, 9000, 9000));
            a.doors.push(door(2, "105", 0, 3000, 3000));
            a.actions.push(item(4, 0x30, 12000, 14000));
            world.observe(1, &a);
            let lua = Lua::new();
            let table = world.to_lua(&lua, &a).unwrap();
            let rooms: Table = table.get("rooms").unwrap();
            rooms
                .sequence_values::<Table>()
                .map(|row| row.unwrap().get::<String>("id").unwrap())
                .collect::<Vec<_>>()
        };
        assert_eq!(build(), vec!["105", "106", "107"]);
        assert_eq!(build(), build());
    }
}
