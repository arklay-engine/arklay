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
//! - Door randomization and lock randomization are later milestones. Under
//!   them staging prepares the map differently (`:217`, `:290-445`, `:449`):
//!   the hooks are marked below, and [`prepare`] fails if either is asked for.
//!   [`crate::plan::generate`] turns both off before calling it.
//! - The entrances from `doors.csv` (`:162-182`) are already in the room
//!   files, and the lockpick's free locks (`:185-190`) are
//!   [`KeyOptions::implicit_keys`](crate::keys::KeyOptions::implicit_keys).

use anyhow::{Context, Result, anyhow, bail};

use crate::config::Config;
use crate::map::{Door, DoorKind, Filter, Map, Player, Room, tags};
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
    /// `doors/random`. Not supported yet.
    pub door_rando: bool,
    /// `locks/random`. Not supported yet.
    pub lock_rando: bool,
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
            cutscenes_disabled: config.get_bool("cutscenes/disable", false),
            progression: Progression::from_config(config),
        }
    }
}

/// The map of one game. `rng` should be `Rng::named(seed, "re1map")`; only
/// lock randomization draws from it.
pub fn prepare(map: &Map, options: &Options, rng: &mut Rng) -> Result<Map> {
    let _ = rng;
    if options.door_rando {
        // Hook (door milestone): with door randomization, staging skips the
        // area and lock set-up below, lets any slot take any key (`:449-454`)
        // and leaves the areas to the door randomizer's segments.
        bail!("door randomization is not supported yet");
    }
    if options.lock_rando {
        // Hook (lock milestone): staging's `:290-445`, which strips the
        // original locks, picks the second mansion visit's key and rooms, and
        // gives each door the keys it may take, by area.
        bail!("lock randomization is not supported yet");
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

    restrict_groups(&mut map, progression)?;
    segment(&mut map, progression)?;
    Ok(map)
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
fn restrict_groups(map: &mut Map, progression: &Progression) -> Result<()> {
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
        // The Helmet Key is behind plant 42.
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
        item_def(map, HELMET_KEY)?.group = groups::PLANT_42;

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
fn other_side_mut<'a>(map: &'a mut Map, room: &str, index: usize) -> Option<&'a mut Door> {
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
