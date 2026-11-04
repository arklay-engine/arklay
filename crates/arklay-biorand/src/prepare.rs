//! Staging's RE1 map preparation (`Re1ClassicRandomizerController.GetMap`,
//! `:141-524`): the map as one game sees it, before any stage runs.
//!
//! It keeps only what the game's player and door mode include, leaves out
//! the areas the progression options turn off, and resets every slot's
//! placement groups so that only chosen slots can hold the keys that split
//! the game into parts: the Helmet Key behind plant 42, the Battery in the
//! second mansion visit, the Flare with Tyrant 1. The segmented caves and lab
//! become no-return doors, after which the route finder starts afresh.
//!
//! Every later stage reads the map this returns. Staging mutates one map in
//! place through all its stages; here each stage that changes the map (doors
//! and locks, when they come) will return a new one in the same way.
//!
//! Where arklay differs from staging, and why:
//!
//! - Mansion 2 without the guardhouse is not an error. Staging throws
//!   (`:225-228`), but the option's own description says it applies only with
//!   the guardhouse, and `Random` can pick that pair; here mansion 2 is
//!   simply off.
//! - Under door randomization ([`prepare_for_doors`]) the areas, the second
//!   mansion visit and the segmented caves and lab are left to the door
//!   stage's segments, as staging leaves them.
//! - Lock randomization ([`prepare_locks`], `:290-445`) allocates the second
//!   mansion visit's new lock ids from 64 up, as [`crate::locks`] does, so
//!   that they never meet a lock flag an original room script raises.
//! - The entrances from `doors.csv` (`:162-182`) are already in the room
//!   files, and the lockpick's free locks (`:185-190`) are
//!   [`KeyOptions::implicit_keys`](crate::keys::KeyOptions::implicit_keys).

use anyhow::{Context, Result, anyhow};

use crate::config::Config;
use crate::map::{Door, DoorKind, Filter, Map, Player, Requirement, Room, tags};
use crate::rng::Rng;

/// The placement group bits staging's preparation uses (`:205-215`). A slot
/// takes a key whose group bits are all set in the slot's group.
pub mod groups {
    /// Every bit: the slot takes any key.
    pub const ALL: u32 = u32::MAX;
    pub const MANSION_1: u32 = 1;
    pub const MANSION_2: u32 = 2;
    pub const COURTYARD: u32 = 4;
    pub const GUARDHOUSE: u32 = 8;
    pub const CAVES: u32 = 16;
    pub const LAB: u32 = 32;
    pub const PLANT_42: u32 = 64;
    pub const SMALL_KEY: u32 = 128;
    pub const BATTERY: u32 = 256;
    pub const LAB_TYRANT: u32 = 512;
}

/// The key that opens the second mansion visit's doors on the original
/// locks (staging's `Re1ItemIds.HelmetKey`).
const HELMET_KEY: u8 = 54;
/// The Sword Key, which the lockpick opens: never the second visit's key
/// while the lockpick is in the game.
const SWORD_KEY: u8 = 51;
/// Restricted to the second mansion visit and the lab under mansion 2.
const BATTERY: u8 = 39;
/// Restricted to Tyrant 1's room when Tyrant 1 is mandatory.
const FLARE: u8 = 42;

/// The progression options of a game on the original doors, read as staging
/// reads them (`:146-155`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progression {
    /// `progression/guardhouse` is `Always`.
    pub guardhouse: bool,
    /// `progression/mansion2` is `Always`, and the guardhouse is in.
    pub mansion2: bool,
    /// `progression/caves` is `Always (Segmented)`.
    pub segmented_caves: bool,
    /// `progression/lab` is not `Never`.
    pub lab: bool,
    /// `progression/lab` is `Always (Segmented)`.
    pub segmented_lab: bool,
    /// `progression/tyrant1` is `Always`.
    pub tyrant1: bool,
}

impl Progression {
    /// The options of a resolved configuration, with staging's fallbacks.
    pub fn from_config(config: &Config) -> Self {
        let guardhouse = config.get_str("progression/guardhouse", "Never") == "Always";
        let lab = config.get_str("progression/lab", "Never");
        Self {
            guardhouse,
            mansion2: guardhouse && config.get_str("progression/mansion2", "Never") == "Always",
            segmented_caves: config.get_str("progression/caves", "Always") == "Always (Segmented)",
            lab: lab != "Never",
            segmented_lab: lab == "Always (Segmented)",
            tyrant1: config.get_str("progression/tyrant1", "Never") == "Always",
        }
    }
}

/// Everything the preparation reads from the configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    pub player: Player,
    /// `doors/random`.
    pub door_rando: bool,
    /// `locks/random`.
    pub lock_rando: bool,
    /// `locks/preserve`: under `locks/random`, the original locks stay.
    pub lock_preserve: bool,
    /// `inventory/special/lockpick` is `Always`.
    pub lockpick: bool,
    /// `cutscenes/disable`: when off, the prologue's doors may not be locked.
    pub cutscenes_disabled: bool,
    pub progression: Progression,
}

impl Options {
    pub fn from_config(config: &Config, player: Player) -> Self {
        Self {
            player,
            door_rando: config.get_bool("doors/random", false),
            lock_rando: config.get_bool("locks/random", false),
            lock_preserve: config.get_bool("locks/preserve", false),
            lockpick: config.get_str("inventory/special/lockpick", "Always") == "Always",
            cutscenes_disabled: config.get_bool("cutscenes/disable", false),
            progression: Progression::from_config(config),
        }
    }
}

/// The map of one game. `rng` should be `Rng::named(seed, "re1map")`; only
/// lock randomization draws from it.
pub fn prepare(map: &Map, options: &Options, rng: &mut Rng) -> Result<Map> {
    if options.door_rando {
        return prepare_for_doors(map, options, rng);
    }

    let mut map = for_game(map, options.door_rando, options.player);

    // :193-203. The prologue's cutscenes walk through these doors.
    if !options.cutscenes_disabled {
        for door in doors_mut(&mut map).filter(|door| door.filter.has_tag(tags::PROLOGUE)) {
            door.allowed_locks = Some(Vec::new());
        }
    }

    let progression = &options.progression;
    leave_out_areas(&mut map, progression)?;

    // :277-288. Doors whose lock could strand the player stay unlocked.
    let safe: Vec<(String, usize)> = map
        .rooms
        .values()
        .flat_map(|room| {
            let doors = room.doors.iter().enumerate();
            doors
                .filter(|(_, door)| door.filter.has_tag("softlock-safe"))
                .map(|(index, _)| (room.key.clone(), index))
        })
        .collect();
    for (room, index) in safe {
        let door = &mut map.rooms[room.as_str()].doors[index];
        door.allowed_locks = Some(Vec::new());
        if let Some(other) = other_side_mut(&mut map, &room, index) {
            other.allowed_locks = Some(Vec::new());
        }
    }

    let mansion2_key = if options.lock_rando {
        prepare_locks(&mut map, options, rng)?
    } else {
        HELMET_KEY
    };

    restrict_groups(&mut map, progression, mansion2_key)?;
    segment(&mut map, progression)?;
    Ok(map)
}

/// The map of a game with door randomization (`doors/random`). Staging
/// skips the areas, the softlock-safe doors, the second mansion visit and
/// the segmented caves and lab (`:217`, `:277`): the door randomizer's
/// segments take their place. Under `locks/random` the original locks the
/// randomizer may replace go and every door may take any discard key
/// (`:290-326`); the door stage then narrows each to its segment's keys.
/// Any slot takes any key (`:449-454`).
///
/// Unlike staging, an original one-sided lock (a door locked or unlocked
/// from one side, with no key) on a door the randomizer may rewire goes
/// whatever the lock options: on a new layout it would seal off what it now
/// leads to, from the side that has to open it.
fn prepare_for_doors(map: &Map, options: &Options, rng: &mut Rng) -> Result<Map> {
    let mut map = for_game(map, true, options.player);
    if !options.cutscenes_disabled {
        for door in doors_mut(&mut map).filter(|door| door.filter.has_tag(tags::PROLOGUE)) {
            door.allowed_locks = Some(Vec::new());
        }
    }
    for door in doors_mut(&mut map).filter(|door| {
        door.randomize != Some(false)
            && door.allowed_locks.is_none()
            && door.key.is_none()
            && matches!(door.kind, Some(DoorKind::Locked | DoorKind::Unlock))
    }) {
        door.kind = None;
        door.lock = None;
    }
    if options.lock_rando {
        if !options.lock_preserve {
            for door in doors_mut(&mut map).filter(|door| door.allowed_locks.is_none()) {
                if matches!(door.kind, Some(DoorKind::Locked | DoorKind::Unlock)) {
                    door.kind = None;
                }
                door.lock = None;
            }
        }
        let discard: Vec<u8> = map
            .items
            .iter()
            .filter(|(_, def)| def.discard)
            .map(|(&id, _)| id)
            .collect();
        let generic_keys = rng.shuffled(&discard);
        for door in doors_mut(&mut map).filter(|door| door.allowed_locks.is_none()) {
            door.allowed_locks = Some(generic_keys.clone());
        }
    }
    for_each_slot(&mut map, |group| *group = groups::ALL);
    Ok(map)
}

/// :290-445, under `locks/random`. Unless the original locks are preserved,
/// every door the lock randomizer may lock loses its original lock, the
/// second mansion visit gets a key of its own (any discard key) and a few
/// rooms locked with it, and each door is told which keys may lock it, by
/// the area each key is drawn for. Returns the second visit's key: the
/// Helmet Key on the original locks.
///
/// A door's original `requires` stay, as staging leaves them: the lock
/// stage ([`crate::locks`]) rewrites those of every door it decides.
fn prepare_locks(map: &mut Map, options: &Options, rng: &mut Rng) -> Result<u8> {
    let progression = &options.progression;
    let mut mansion2_key = HELMET_KEY;
    if !options.lock_preserve {
        // Remove every original lock the randomizer may replace: a door
        // whose two sides take any key.
        let mut free = Vec::new();
        for room in map.rooms.values() {
            for (index, door) in room.doors.iter().enumerate() {
                let opposite_free = other_side(map, &room.key, index)
                    .is_none_or(|other| other.allowed_locks.is_none());
                if door.allowed_locks.is_none() && opposite_free {
                    free.push((room.key.clone(), index));
                }
            }
        }
        for (room, index) in free {
            let door = &mut map.rooms[room.as_str()].doors[index];
            if matches!(door.kind, Some(DoorKind::Locked | DoorKind::Unlock)) {
                door.kind = None;
            }
            door.lock = None;
        }
    }

    let discard: Vec<u8> = map
        .items
        .iter()
        .filter(|(_, def)| def.discard)
        .map(|(&id, _)| id)
        .collect();
    let mut generic_keys = rng.shuffled(&discard);
    if options.lock_preserve {
        generic_keys.retain(|&key| key != HELMET_KEY);
    } else {
        for_each_slot(map, |group| {
            if *group == groups::MANSION_2 {
                *group = groups::MANSION_1;
            }
        });
        if progression.mansion2 {
            mansion2_key = generic_keys
                .iter()
                .copied()
                .find(|&key| !options.lockpick || key != SWORD_KEY)
                .ok_or_else(|| anyhow!("no discard key is left for the second mansion visit"))?;
            generic_keys.retain(|&key| key != mansion2_key);
            lock_mansion2_rooms(map, mansion2_key, rng)?;
        }
    }

    // The areas a key may be drawn for: the mansion with the courtyard, the
    // caves, then the guardhouse and the lab when the game has them.
    let mut areas: Vec<&[&str]> = vec![&["mansion", "courtyard"], &["caves"]];
    if progression.guardhouse {
        areas.push(&["guardhouse"]);
    }
    if progression.lab {
        areas.push(&["lab"]);
    }
    let locks: Vec<(u8, Vec<&str>)> = generic_keys
        .iter()
        .map(|&key| {
            let bits = rng.next_in(0, 0b1111);
            let tags = (0..4)
                .filter(|&i| bits & (1 << i) != 0)
                .filter_map(|i| areas.get(i))
                .flat_map(|tags| tags.iter().copied())
                .collect();
            (key, tags)
        })
        .collect();
    for room in map.rooms.values_mut() {
        let allowed: Vec<u8> = locks
            .iter()
            .filter(|(_, tags)| tags.is_empty() || tags.iter().any(|tag| room.filter.has_tag(tag)))
            .map(|(key, _)| *key)
            .collect();
        for door in room
            .doors
            .iter_mut()
            .filter(|door| door.allowed_locks.is_none())
        {
            door.allowed_locks = Some(allowed.clone());
        }
    }
    Ok(mansion2_key)
}

/// :343-410. Between two and eight `mansion2able` rooms become the second
/// mansion visit: their slots are its slots, and each of their doors, both
/// sides, is locked with a new lock that `key` opens.
fn lock_mansion2_rooms(map: &mut Map, key: u8, rng: &mut Rng) -> Result<()> {
    let able: Vec<String> = map
        .rooms
        .values()
        .filter(|room| room.filter.has_tag("mansion2able"))
        .map(|room| room.key.clone())
        .collect();
    let mut rooms = rng.shuffled(&able);
    let count = usize::try_from(rng.next_in(2, 9)).unwrap_or(2);
    rooms.truncate(count);

    let used: std::collections::BTreeSet<u8> = map
        .rooms
        .values()
        .flat_map(|room| {
            let doors = room.doors.iter().filter_map(|door| door.lock);
            doors.chain(room.items.iter().filter_map(|item| item.lock))
        })
        .collect();
    let mut lock_ids = crate::locks::lock_id_order().filter(|id| !used.contains(id));

    for room in rooms {
        for item in &mut map.rooms[room.as_str()].items {
            item.group = groups::MANSION_2;
        }
        for index in 0..map.rooms[room.as_str()].doors.len() {
            if map.rooms[room.as_str()].doors[index].target.is_none() {
                continue;
            }
            let lock = lock_ids
                .next()
                .ok_or_else(|| anyhow!("no lock id is left for the second mansion visit"))?;
            let set = |door: &mut Door| {
                door.allowed_locks = Some(Vec::new());
                door.requires = vec![Requirement::Item(key)];
                door.lock = Some(lock);
                door.key = Some(key);
            };
            set(&mut map.rooms[room.as_str()].doors[index]);
            if let Some(other) = other_side_mut(map, &room, index) {
                set(other);
            }
        }
    }
    Ok(())
}

/// The map with only what a game with `door_rando` for `player` includes
/// (staging's `Map.For`, `Map.cs:131-145` and `:260-275`). The catalogues
/// and enemy spots are kept whole.
pub fn for_game(map: &Map, door_rando: bool, player: Player) -> Map {
    let included = |filter: &Filter| filter.includes(door_rando, player);
    let rooms = map
        .rooms
        .iter()
        .filter(|(_, room)| included(&room.filter))
        .map(|(key, room)| {
            let room = Room {
                doors: keep(&room.doors, |door| included(&door.filter)),
                flags: keep(&room.flags, |flag| included(&flag.filter)),
                items: keep(&room.items, |item| included(&item.filter)),
                enemies: keep(&room.enemies, |rule| included(&rule.filter)),
                ..room.clone()
            };
            (key.clone(), room)
        })
        .collect();
    Map {
        begin_end: keep(&map.begin_end, |entry| included(&entry.filter)),
        rooms,
        ..map.clone()
    }
}

fn keep<T: Clone>(values: &[T], f: impl Fn(&T) -> bool) -> Vec<T> {
    values.iter().filter(|value| f(value)).cloned().collect()
}

/// :217-273. The guardhouse and the lab, when the game leaves them out; and
/// with the lab in, the fountain's way to the heliport goes through it.
fn leave_out_areas(map: &mut Map, progression: &Progression) -> Result<()> {
    if !progression.guardhouse {
        map.rooms
            .retain(|_, room| !room.filter.has_tag("guardhouse"));
        named_door(map, "302", "GATE TO GUARDHOUSE")?.target = None;
    }
    if progression.lab {
        // The heliport door opens only from the heliport, which the player
        // reaches from the lab, so the graph leaves it out.
        let fountain = named_door(map, "305", "DOOR TO HELIPORT")?;
        fountain.kind = Some(DoorKind::Locked);
        fountain.allowed_locks = Some(Vec::new());
        let helipad = named_door(map, "303", "DOOR TO FOUNTAIN")?;
        helipad.kind = Some(DoorKind::Unlock);
        helipad.ignore_in_graph = true;
    } else {
        map.rooms.retain(|_, room| !room.filter.has_tag("lab"));
        let stairs = named_door(map, "305", "FOUNTAIN STAIRS")?;
        stairs.target = None;
        stairs.requires.clear();
        named_door(map, "305", "DOOR TO HELIPORT")?.kind = None;
        named_door(map, "303", "DOOR TO FOUNTAIN")?.kind = None;
        named_door(map, "303", "LIFT TO LAB")?.target = None;
    }
    Ok(())
}

/// :456-506. Every slot takes any key, then the progression options clear
/// bits so that a few keys can only go in chosen slots. The room files'
/// groups say which area each slot is in.
fn restrict_groups(map: &mut Map, progression: &Progression, mansion2_key: u8) -> Result<()> {
    let area = |group: u32| -> Vec<(String, usize)> {
        map.rooms
            .values()
            .flat_map(|room| {
                room.items
                    .iter()
                    .enumerate()
                    .filter(move |(_, item)| item.group == group)
                    .map(|(index, _)| (room.key.clone(), index))
            })
            .collect()
    };
    let mansion2_slots = area(groups::MANSION_2);
    let lab_slots = area(groups::LAB);
    let tyrant_slots = area(groups::LAB_TYRANT);

    for_each_slot(map, |group| *group = groups::ALL);

    // Staging's segmented guardhouse (`:465-472`) is hard-wired off (`:150`).

    if progression.mansion2 {
        // The second visit's key (the Helmet Key on the original locks) is
        // behind plant 42.
        for_each_slot(map, |group| *group &= !groups::PLANT_42);
        let fireplace = map
            .rooms
            .get_mut("40C")
            .and_then(|room| {
                room.items
                    .iter_mut()
                    .find(|item| item.name.as_deref() == Some("KEY IN FIREPLACE"))
            })
            .ok_or_else(|| anyhow!("room 40C has no slot KEY IN FIREPLACE"))?;
        fireplace.group = groups::PLANT_42;
        item_def(map, mansion2_key)?.group = groups::PLANT_42;

        // The Battery is in a room of the second visit, or the lab.
        for_each_slot(map, |group| *group &= !groups::BATTERY);
        item_def(map, BATTERY)?.group = groups::BATTERY;
        for (room, index) in mansion2_slots.iter().chain(&lab_slots) {
            map.rooms[room.as_str()].items[*index].group |= groups::BATTERY;
        }
    }

    if progression.lab && progression.tyrant1 {
        // The Flare is where Tyrant 1 is, so the player has to fight it.
        item_def(map, FLARE)?.group = groups::LAB_TYRANT;
        for_each_slot(map, |group| *group &= !groups::LAB_TYRANT);
        for (room, index) in &tyrant_slots {
            map.rooms[room.as_str()].items[*index].group = groups::LAB_TYRANT;
        }
    }
    Ok(())
}

/// :508-520. A segmented area is entered through a no-return door, whose far
/// side is blocked; the route finder needs no key from before it.
fn segment(map: &mut Map, progression: &Progression) -> Result<()> {
    let mut entries = Vec::new();
    if progression.segmented_caves {
        entries.push(("302", "LADDER TO CAVES"));
    }
    if progression.lab && progression.segmented_lab {
        entries.push(("305", "FOUNTAIN STAIRS"));
    }
    for (room, name) in entries {
        let door = named_door(map, room, name)?;
        door.kind = Some(DoorKind::NoReturn);
        let index = map.rooms[room]
            .doors
            .iter()
            .position(|d| d.name.as_deref() == Some(name));
        let other = index
            .and_then(|index| other_side_mut(map, room, index))
            .ok_or_else(|| anyhow!("room {room}: {name} has no far side"))?;
        other.kind = Some(DoorKind::Blocked);
    }
    Ok(())
}

fn for_each_slot(map: &mut Map, mut f: impl FnMut(&mut u32)) {
    for room in map.rooms.values_mut() {
        for item in &mut room.items {
            f(&mut item.group);
        }
    }
}

fn item_def(map: &mut Map, id: u8) -> Result<&mut crate::map::ItemDef> {
    map.items
        .get_mut(&id)
        .ok_or_else(|| anyhow!("item {id} is not in the catalogue"))
}

/// The door of `room` named `name` (staging's `Doors.First(x => x.Name ==
/// name)`).
fn named_door<'a>(map: &'a mut Map, room: &str, name: &str) -> Result<&'a mut Door> {
    map.rooms
        .get_mut(room)
        .with_context(|| format!("room {room} is not in the map"))?
        .doors
        .iter_mut()
        .find(|door| door.name.as_deref() == Some(name))
        .ok_or_else(|| anyhow!("room {room} has no door {name}"))
}

fn doors_mut(map: &mut Map) -> impl Iterator<Item = &mut Door> {
    map.rooms
        .values_mut()
        .flat_map(|room| room.doors.iter_mut())
}

/// The far side of door `index` of `room` (staging's `Map.GetOtherSide`):
/// the first door of the target room with the target's door id.
pub(crate) fn other_side<'a>(map: &'a Map, room: &str, index: usize) -> Option<&'a Door> {
    let door = &map.rooms.get(room)?.doors[index];
    let target_id = door.target_id()?;
    map.rooms
        .get(door.target_room()?)?
        .doors
        .iter()
        .find(|other| other.id == Some(target_id))
}

/// [`other_side`], mutable.
pub(crate) fn other_side_mut<'a>(
    map: &'a mut Map,
    room: &str,
    index: usize,
) -> Option<&'a mut Door> {
    let door = &map.rooms.get(room)?.doors[index];
    let target_room = door.target_room()?.to_string();
    let target_id = door.target_id()?;
    map.rooms
        .get_mut(&target_room)?
        .doors
        .iter_mut()
        .find(|door| door.id == Some(target_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{Item, ItemDef};

    fn door(id: u8, name: &str, target: &str) -> Door {
        Door {
            id: Some(id),
            name: Some(name.into()),
            target: Some(target.into()),
            ..Door::default()
        }
    }

    fn slot(id: u8, name: &str, group: u32) -> Item {
        Item {
            id,
            flag: Some(u16::from(id) + 100),
            name: Some(name.into()),
            group,
            ..Item::default()
        }
    }

    fn room(key: &str, tags: &[&str], doors: Vec<Door>, items: Vec<Item>) -> Room {
        Room {
            key: key.into(),
            doors,
            items,
            filter: Filter {
                tags: tags.iter().map(|tag| tag.to_string()).collect(),
                ..Filter::default()
            },
            ..Room::default()
        }
    }

    /// The rooms and doors the preparation names, with a slot in each area.
    fn map() -> Map {
        let mut map = Map::default();
        for id in [HELMET_KEY, BATTERY, FLARE] {
            map.items.insert(
                id,
                ItemDef {
                    kind: "key/consumable".into(),
                    group: 1,
                    ..ItemDef::default()
                },
            );
        }
        let rooms = [
            room(
                "302",
                &["courtyard"],
                vec![
                    door(2, "GATE TO GUARDHOUSE", "304:0"),
                    door(3, "LADDER TO CAVES", "307:0"),
                ],
                vec![slot(1, "COURTYARD", groups::COURTYARD)],
            ),
            room(
                "304",
                &["guardhouse"],
                vec![door(0, "GATE", "302:2")],
                vec![],
            ),
            room(
                "40C",
                &["guardhouse"],
                vec![],
                vec![slot(3, "KEY IN FIREPLACE", groups::PLANT_42)],
            ),
            room("307", &["caves"], vec![door(0, "LADDER", "302:3")], vec![]),
            room(
                "305",
                &["caves"],
                vec![
                    door(0, "DOOR TO HELIPORT", "303:8"),
                    door(1, "FOUNTAIN STAIRS", "310:0"),
                ],
                vec![],
            ),
            room(
                "303",
                &[],
                vec![
                    door(0, "LIFT TO LAB", "50D:1"),
                    door(8, "DOOR TO FOUNTAIN", "305:0"),
                ],
                vec![],
            ),
            room(
                "310",
                &["lab"],
                vec![door(0, "UP", "305:1")],
                vec![slot(4, "LAB", groups::LAB)],
            ),
            room(
                "513",
                &["lab"],
                vec![],
                vec![slot(5, "TYRANT", groups::LAB_TYRANT)],
            ),
            room(
                "21C",
                &["mansion"],
                vec![],
                vec![slot(6, "KITCHEN", groups::MANSION_2)],
            ),
        ];
        for room in rooms {
            map.rooms.insert(room.key.clone(), room);
        }
        map
    }

    fn options(progression: Progression) -> Options {
        Options {
            player: Player::Chris,
            door_rando: false,
            lock_rando: false,
            lock_preserve: false,
            lockpick: true,
            cutscenes_disabled: false,
            progression,
        }
    }

    const EVERYTHING: Progression = Progression {
        guardhouse: true,
        mansion2: true,
        segmented_caves: true,
        lab: true,
        segmented_lab: true,
        tyrant1: true,
    };

    fn group(map: &Map, room: &str) -> u32 {
        map.rooms[room].items[0].group
    }

    fn named<'a>(map: &'a Map, room: &str, name: &str) -> &'a Door {
        map.rooms[room]
            .doors
            .iter()
            .find(|door| door.name.as_deref() == Some(name))
            .unwrap()
    }

    #[test]
    fn every_slot_takes_any_key_without_restrictions() {
        let progression = Progression {
            mansion2: false,
            tyrant1: false,
            ..EVERYTHING
        };
        let map = prepare(&map(), &options(progression), &mut Rng::new(1)).unwrap();
        for room in map.rooms.values() {
            for item in &room.items {
                assert_eq!(item.group, groups::ALL, "{}", room.key);
            }
        }
    }

    #[test]
    fn mansion2_puts_the_helmet_key_behind_plant_42_and_the_battery_late() {
        let map = prepare(&map(), &options(EVERYTHING), &mut Rng::new(1)).unwrap();
        assert_eq!(group(&map, "40C"), groups::PLANT_42);
        assert_eq!(map.items[&HELMET_KEY].group, groups::PLANT_42);
        assert_eq!(map.items[&BATTERY].group, groups::BATTERY);
        assert_ne!(
            group(&map, "21C") & groups::BATTERY,
            0,
            "a second-visit slot"
        );
        assert_ne!(group(&map, "310") & groups::BATTERY, 0, "a lab slot");
        assert_eq!(group(&map, "302") & (groups::BATTERY | groups::PLANT_42), 0);
        // Tyrant 1 holds the flare, and only Tyrant 1's slot takes it.
        assert_eq!(map.items[&FLARE].group, groups::LAB_TYRANT);
        assert_eq!(group(&map, "513"), groups::LAB_TYRANT);
        assert_eq!(group(&map, "310") & groups::LAB_TYRANT, 0);
    }

    #[test]
    fn segmented_areas_are_entered_through_no_return_doors() {
        let map = prepare(&map(), &options(EVERYTHING), &mut Rng::new(1)).unwrap();
        assert_eq!(
            named(&map, "302", "LADDER TO CAVES").kind,
            Some(DoorKind::NoReturn)
        );
        assert_eq!(map.rooms["307"].doors[0].kind, Some(DoorKind::Blocked));
        assert_eq!(
            named(&map, "305", "FOUNTAIN STAIRS").kind,
            Some(DoorKind::NoReturn)
        );
        assert_eq!(map.rooms["310"].doors[0].kind, Some(DoorKind::Blocked));
        // With the lab in, the heliport is reached through it.
        assert_eq!(
            named(&map, "305", "DOOR TO HELIPORT").kind,
            Some(DoorKind::Locked)
        );
        assert!(named(&map, "303", "DOOR TO FOUNTAIN").ignore_in_graph);
    }

    #[test]
    fn areas_left_out_are_gone_and_their_doors_lead_nowhere() {
        let progression = Progression {
            guardhouse: false,
            mansion2: false,
            lab: false,
            segmented_lab: false,
            ..EVERYTHING
        };
        let map = prepare(&map(), &options(progression), &mut Rng::new(1)).unwrap();
        for gone in ["304", "40C", "310", "513"] {
            assert!(!map.rooms.contains_key(gone), "{gone}");
        }
        assert_eq!(named(&map, "302", "GATE TO GUARDHOUSE").target, None);
        assert_eq!(named(&map, "305", "FOUNTAIN STAIRS").target, None);
        assert_eq!(named(&map, "303", "LIFT TO LAB").target, None);
        assert_eq!(named(&map, "305", "DOOR TO HELIPORT").kind, None);
    }

    #[test]
    fn mansion2_needs_the_guardhouse() {
        let mut config = Config::new();
        config.set("progression/guardhouse", "Never");
        config.set("progression/mansion2", "Always");
        assert!(!Progression::from_config(&config).mansion2);
    }

    #[test]
    fn prologue_doors_stay_unlocked_while_cutscenes_play() {
        let mut map = map();
        let mut prologue = door(9, "PROLOGUE", "302:9");
        prologue.filter.tags.push(tags::PROLOGUE.into());
        map.rooms["21C"].doors.push(prologue);
        let prepared = prepare(&map, &options(EVERYTHING), &mut Rng::new(1)).unwrap();
        assert_eq!(
            prepared.rooms["21C"].doors[0].allowed_locks,
            Some(Vec::new())
        );
        let disabled = Options {
            cutscenes_disabled: true,
            ..options(EVERYTHING)
        };
        let prepared = prepare(&map, &disabled, &mut Rng::new(1)).unwrap();
        assert_eq!(prepared.rooms["21C"].doors[0].allowed_locks, None);
    }

    #[test]
    fn lock_randomization_strips_locks_and_locks_the_second_visit() {
        let mut map = map();
        for id in [52, 53, HELMET_KEY] {
            map.items.insert(
                id,
                ItemDef {
                    kind: "key/reusuable".into(),
                    group: 1,
                    discard: true,
                    ..ItemDef::default()
                },
            );
        }
        // A locked door the randomizer may replace, and a second-visit room
        // with a door.
        let gate = &mut map.rooms["302"].doors[0];
        gate.kind = Some(DoorKind::Locked);
        gate.lock = Some(9);
        map.rooms["21C"].filter.tags.push("mansion2able".into());
        map.rooms["21C"]
            .doors
            .push(door(4, "TO COURTYARD", "302:5"));
        map.rooms["302"].doors.push(door(5, "TO KITCHEN", "21C:4"));
        let options = Options {
            lock_rando: true,
            ..options(EVERYTHING)
        };
        for seed in 0..10 {
            let prepared = prepare(&map, &options, &mut Rng::new(seed)).unwrap();
            let gate = named(&prepared, "302", "GATE TO GUARDHOUSE");
            assert_eq!((gate.kind, gate.lock), (None, None), "seed {seed}");
            assert!(gate.allowed_locks.is_some(), "seed {seed}");

            let kitchen = named(&prepared, "21C", "TO COURTYARD");
            let key = kitchen.key.unwrap();
            assert!([52, 53, HELMET_KEY].contains(&key), "seed {seed}");
            assert!(kitchen.lock.unwrap() >= 64);
            assert_eq!(kitchen.requires, vec![Requirement::Item(key)]);
            assert_eq!(kitchen.allowed_locks, Some(Vec::new()));
            let courtyard = named(&prepared, "302", "TO KITCHEN");
            assert_eq!((courtyard.lock, courtyard.key), (kitchen.lock, kitchen.key));
            // The second visit's key is behind plant 42, and its slots are the
            // second visit's.
            assert_eq!(prepared.items[&key].group, groups::PLANT_42, "seed {seed}");
            assert_ne!(group(&prepared, "21C") & groups::BATTERY, 0);
        }
    }

    #[test]
    fn for_game_drops_the_other_players_entries() {
        let mut map = map();
        let mut jill = slot(9, "JILL ONLY", 1);
        jill.filter.player = Some(Player::Jill);
        map.rooms["302"].items.push(jill);
        let chris = for_game(&map, false, Player::Chris);
        assert_eq!(chris.rooms["302"].items.len(), 1);
        let jill = for_game(&map, false, Player::Jill);
        assert_eq!(jill.rooms["302"].items.len(), 2);
    }
}
