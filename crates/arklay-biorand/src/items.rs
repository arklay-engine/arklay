//! Staging's `ItemRandomizer` (`ItemRandomizer.cs`), with the key amounts
//! staging's `KeyRandomizer` sets (`KeyRandomizer.cs:149-195`): what every
//! item slot holds once the keys are placed.
//!
//! A slot is known by its pickup flag (staging's `globalId`), which both
//! mansion visits share. After the keys, the stage places one weapon of each
//! weapon group the player doesn't start with, then fills the slots left with
//! ammunition, healing and ink in the configured proportions, and empties the
//! rest. Documents and maps are slots like any other when `items/documents` is
//! on; otherwise they keep their document. That covers the maps, the MO disc
//! readers' pass codes and the pickups Chris's game has that Jill's lacks: the
//! room files hold them as slots, and the emitter turns each one into an
//! ordinary pickup when the plan gives it an item.
//!
//! Where arklay differs from staging, and why:
//!
//! - The weapons the player starts with make their ammunition available even
//!   when `items/weapon/enabled/<weapon>` is off. Staging also requires the
//!   world-placement switch for them (`ItemRandomizer.cs:68-70`), so starting
//!   with a shotgun the world doesn't place left no shells anywhere.
//! - A key whose slot is in no segment (a room the start can't reach by
//!   doors) keeps its catalogue amount. Staging skips it (`:167-168`), which
//!   leaves the slot unassigned, so the item stage would put something else
//!   there and lose the key.
//! - A stack is never more than the item's maximum, nor less than one, even
//!   when the configured minimum is above the maximum; staging's
//!   `Next(min, max + 1)` returns the minimum then.
//! - A slot with no pickup flag is left alone, as staging leaves it; none of
//!   RE1's slots lack one. A key placed in one is an error.

use std::collections::{HashSet, VecDeque};

use anyhow::{Result, anyhow, bail};
use indexmap::IndexMap;

use crate::config::Config;
use crate::inventory::StartInventory;
use crate::keys::KeyPlacement;
use crate::map::{DoorKind, ItemDef, Map, Requirement, Room};
use crate::rng::Rng;

/// Why a slot holds what it holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    /// A key the route needs, placed by the key stage.
    Key,
    /// A weapon of a group the player doesn't start with.
    Weapon,
    /// Ammunition, healing or ink, by the configured distribution.
    Supply,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Key => "key",
            Self::Weapon => "weapon",
            Self::Supply => "supply",
        }
    }
}

/// What a slot holds in the generated game.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pickup {
    /// The original's pickup, unchanged: items aren't randomized, or the
    /// slot is a document and documents aren't replaced.
    Original,
    /// Nothing: the pickup is removed (staging's `Item(0, 0)`).
    Empty,
    /// `amount` of item `item`.
    Item { item: u8, amount: u16, role: Role },
}

/// The item stage's result.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemPlan {
    /// Every pickup flag of the map's slots, in map order, with what its
    /// slots hold.
    pub slots: IndexMap<u16, Pickup>,
}

/// How eagerly a kind of slot is filled (staging's `ItemPriority`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Priority {
    /// Left as the original has it.
    Disabled,
    /// Filled only by the distribution, in proportion to the slots left.
    Low,
    /// Filled first: weapons go here before anywhere low.
    Normal,
}

/// What the stage reads from the configuration besides the amounts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemOptions {
    /// `items/documents` and `items/documents/keys` (`:18-22`).
    pub documents: Priority,
    /// `items/hidden/keys` (`:23-25`). Hidden slots are never left alone.
    pub hidden: Priority,
}

impl ItemOptions {
    pub fn from_config(config: &Config) -> Self {
        let documents = match (
            config.get_bool("items/documents", false),
            config.get_bool("items/documents/keys", false),
        ) {
            (false, _) => Priority::Disabled,
            (true, false) => Priority::Low,
            (true, true) => Priority::Normal,
        };
        let hidden = if config.get_bool("items/hidden/keys", false) {
            Priority::Normal
        } else {
            Priority::Low
        };
        Self { documents, hidden }
    }
}

/// Fills every slot of `map`, which should be the prepared map of the game.
/// `keys` is the key stage's placement, `inventory` the player's start, and
/// `rng` should be `Rng::named(seed, "item")`.
pub fn randomize(
    map: &Map,
    config: &Config,
    inventory: &StartInventory,
    keys: &KeyPlacement,
    rng: &mut Rng,
) -> Result<ItemPlan> {
    let options = ItemOptions::from_config(config);
    let mut plan = ItemPlan {
        slots: slots(map)
            .map(|(_, item)| (flag_of(item), Pickup::Original))
            .collect(),
    };

    // KeyRandomizer.cs:157-194.
    let segments = Segments::new(map);
    for placed in &keys.keys {
        let flag = placed.slot.flag.ok_or_else(|| {
            anyhow!(
                "{} {}: a key in a slot with no pickup flag",
                placed.slot.room,
                placed.slot.item
            )
        })?;
        let amount = segments.key_amount(map, placed.key, flag);
        assign(&mut plan, flag, placed.key, amount, Role::Key)?;
    }

    let mut queue = SlotQueue::new(map, &plan, &options, rng);

    // :28-59. One weapon of each group the player has none of.
    let start: Vec<Weapon> = inventory
        .weapons
        .iter()
        .filter_map(|&id| Weapon::new(map, config, id))
        .collect();
    let mut groups: HashSet<&str> = start.iter().map(|weapon| weapon.group).collect();
    let mut weapons: Vec<Weapon> = map
        .items
        .keys()
        .filter_map(|&id| Weapon::new(map, config, id))
        .collect();
    rng.shuffle(&mut weapons);
    let mut placed: Vec<&Weapon> = Vec::new();
    for weapon in &weapons {
        if !weapon.enabled || !groups.insert(weapon.group) {
            continue;
        }
        let Some(flag) = queue.next_normal_first() else {
            break;
        };
        let amount = rng.next_in(weapon.min_initial, weapon.max_initial + 1);
        let amount = clamp_amount(amount, weapon.def.max);
        assign(&mut plan, flag, weapon.id, amount, Role::Weapon)?;
        placed.push(weapon);
    }

    // :61-99. The rest, in proportion to each item's weight.
    let usable = |ammo: u8| {
        start
            .iter()
            .chain(placed.iter().copied())
            .any(|weapon| weapon.def.ammo.contains(&ammo))
    };
    let weights: Vec<(u8, &ItemDef, f64)> = map
        .items
        .iter()
        .map(|(&id, def)| {
            let weight = config.get_f64(&format!("items/distribution/{}", def.kind), 0.0);
            let weight = if def.kind.starts_with("ammo/") && !usable(id) {
                0.0
            } else {
                weight
            };
            (id, def, weight)
        })
        .filter(|(_, _, weight)| *weight > 0.0)
        .collect();
    let total: f64 = weights.iter().map(|(_, _, weight)| weight).sum();
    let slots = queue.len() as f64;
    let mut counts: Vec<(u8, &ItemDef, usize)> = weights
        .iter()
        .map(|&(id, def, weight)| (id, def, (weight / total * slots).ceil() as usize))
        .collect();
    // Rarer items first, so that rounding up never squeezes them out.
    counts.sort_by_key(|&(_, _, count)| count);
    for (id, def, count) in counts {
        let max = i64::from(def.max);
        let min_stack = config
            .get_int(&format!("items/stack/min/{}", def.kind), 1)
            .max(1);
        let max_stack = config
            .get_int(&format!("items/stack/max/{}", def.kind), max)
            .min(max);
        for _ in 0..count {
            let Some(flag) = queue.next_any(rng) else {
                break;
            };
            let amount = rng.next_in(to_i32(min_stack), to_i32(max_stack) + 1);
            let amount = clamp_amount(amount, def.max).max(1);
            assign(&mut plan, flag, id, amount, Role::Supply)?;
        }
    }

    // :101-105.
    while let Some(flag) = queue.next_any(rng) {
        plan.slots.insert(flag, Pickup::Empty);
    }
    Ok(plan)
}

/// The slots of `map` that have a pickup flag, in map order, each flag once
/// (`DistinctBy(x => x.GlobalId)`, `:120-124`).
fn slots(map: &Map) -> impl Iterator<Item = (&Room, &crate::map::Item)> {
    let mut seen = HashSet::new();
    map.rooms
        .values()
        .flat_map(|room| room.items.iter().map(move |item| (room, item)))
        .filter(move |(_, item)| item.flag.is_some_and(|flag| seen.insert(flag)))
}

fn flag_of(item: &crate::map::Item) -> u16 {
    item.flag.expect("slots() only yields flagged slots")
}

/// Gives a slot its item. Two stages filling one slot is a bug, as staging's
/// `ModBuilder.SetItem` treats it.
fn assign(plan: &mut ItemPlan, flag: u16, item: u8, amount: u16, role: Role) -> Result<()> {
    match plan.slots.get_mut(&flag) {
        Some(slot @ Pickup::Original) => {
            *slot = Pickup::Item { item, amount, role };
            Ok(())
        }
        Some(_) => bail!("pickup flag {flag} is filled twice"),
        None => bail!("pickup flag {flag} is not a slot of the map"),
    }
}

/// An amount drawn for an item that holds at most `max` (0: no limit).
fn clamp_amount(amount: i32, max: u16) -> u16 {
    let amount = u16::try_from(amount.max(0)).unwrap_or(u16::MAX);
    if max > 0 { amount.min(max) } else { amount }
}

fn to_i32(value: i64) -> i32 {
    value.clamp(i64::from(i32::MIN), i64::from(i32::MAX - 1)) as i32
}

/// A weapon of the catalogue as the stage sees it (staging's `WeaponInfo`).
struct Weapon<'a> {
    id: u8,
    def: &'a ItemDef,
    /// The kind's second part: `colt` for both Colt Pythons.
    group: &'a str,
    /// `items/weapon/enabled/<kind>`: the world may hold it.
    enabled: bool,
    min_initial: i32,
    max_initial: i32,
}

impl<'a> Weapon<'a> {
    fn new(map: &'a Map, config: &Config, id: u8) -> Option<Self> {
        let def = map.items.get(&id)?;
        let group = def.kind.strip_prefix("weapon/")?.split('/').next()?;
        let option = |name: &str| config.get_int(&format!("items/weapon/{name}/{}", def.kind), 0);
        Some(Self {
            id,
            def,
            group,
            enabled: config.get_bool(&format!("items/weapon/enabled/{}", def.kind), false),
            min_initial: to_i32(option("initial/min")),
            max_initial: to_i32(option("initial/max")),
        })
    }
}

/// The slots still to fill, shuffled, in two queues (staging's
/// `ItemSlotCollection`, `:109-171`).
struct SlotQueue {
    normal: VecDeque<u16>,
    low: VecDeque<u16>,
}

impl SlotQueue {
    fn new(map: &Map, plan: &ItemPlan, options: &ItemOptions, rng: &mut Rng) -> Self {
        let mut all: Vec<&crate::map::Item> = slots(map).map(|(_, item)| item).collect();
        rng.shuffle(&mut all);
        let mut queue = Self {
            normal: VecDeque::new(),
            low: VecDeque::new(),
        };
        for item in all {
            let flag = flag_of(item);
            if plan.slots[&flag] != Pickup::Original {
                continue;
            }
            let priority = |document: bool, hidden: bool| {
                if document && options.documents == Priority::Disabled {
                    Priority::Disabled
                } else if item.optional
                    || (document && options.documents == Priority::Low)
                    || (hidden && options.hidden == Priority::Low)
                {
                    Priority::Low
                } else {
                    Priority::Normal
                }
            };
            match priority(item.document, item.hidden) {
                Priority::Disabled => {}
                Priority::Low => queue.low.push_back(flag),
                Priority::Normal => queue.normal.push_back(flag),
            }
        }
        queue
    }

    fn len(&self) -> usize {
        self.normal.len() + self.low.len()
    }

    /// A normal slot while there is one, then a low one.
    fn next_normal_first(&mut self) -> Option<u16> {
        self.normal.pop_front().or_else(|| self.low.pop_front())
    }

    /// A slot from either queue, in proportion to their lengths.
    fn next_any(&mut self, rng: &mut Rng) -> Option<u16> {
        let len = self.len();
        if len == 0 {
            return None;
        }
        if rng.index(len) >= self.normal.len() {
            self.low.pop_front()
        } else {
            self.normal.pop_front()
        }
    }
}

/// The map split at its no-return doors (staging's `GetSegmentTree`,
/// `KeyRandomizer.cs:237-310`): from the start room, the rooms reached
/// without one are a segment, and each no-return door leads to a child.
#[derive(Debug, Default)]
struct Segments {
    /// Each segment's room keys and child segments; the first is the root.
    segments: Vec<(Vec<String>, Vec<usize>)>,
}

impl Segments {
    fn new(map: &Map) -> Self {
        let mut tree = Self::default();
        let start = map.begin_end.first().map(|entry| entry.start.as_str());
        if let Some(start) = start.filter(|start| map.rooms.contains_key(*start)) {
            let mut visited = HashSet::new();
            tree.visit_segment(map, start, &mut visited);
        }
        tree
    }

    fn visit_segment<'a>(
        &mut self,
        map: &'a Map,
        room: &'a str,
        visited: &mut HashSet<&'a str>,
    ) -> usize {
        let index = self.segments.len();
        self.segments.push((Vec::new(), Vec::new()));
        self.visit_room(map, room, index, visited);
        index
    }

    fn visit_room<'a>(
        &mut self,
        map: &'a Map,
        key: &'a str,
        segment: usize,
        visited: &mut HashSet<&'a str>,
    ) {
        if !visited.insert(key) {
            return;
        }
        self.segments[segment].0.push(key.to_string());
        let room = &map.rooms[key];
        for door in &room.doors {
            if door.kind == Some(DoorKind::Blocked) {
                continue;
            }
            let Some((target, _)) = door.target_room().and_then(|t| map.rooms.get_key_value(t))
            else {
                continue;
            };
            if door.kind == Some(DoorKind::NoReturn) {
                let child = self.visit_segment(map, target, visited);
                self.segments[segment].1.push(child);
            } else {
                self.visit_room(map, target, segment, visited);
            }
        }
    }

    /// The first segment, depth first, with a slot of pickup flag `flag`.
    fn containing(&self, map: &Map, flag: u16) -> Option<usize> {
        self.segments.first()?;
        self.find(map, 0, flag)
    }

    fn find(&self, map: &Map, segment: usize, flag: u16) -> Option<usize> {
        let (rooms, children) = &self.segments[segment];
        let here = rooms.iter().any(|room| {
            map.rooms[room.as_str()]
                .items
                .iter()
                .any(|item| item.flag == Some(flag))
        });
        if here {
            return Some(segment);
        }
        children
            .iter()
            .find_map(|&child| self.find(map, child, flag))
    }

    /// The rooms of a segment and of every segment below it.
    fn descendant_rooms(&self, segment: usize, rooms: &mut Vec<String>) {
        let (own, children) = &self.segments[segment];
        rooms.extend(own.iter().cloned());
        for &child in children {
            self.descendant_rooms(child, rooms);
        }
    }

    /// How many of `key` the slot of pickup flag `flag` gives
    /// (`KeyRandomizer.cs:163-192`). A generic key the game discards after its
    /// last door comes as many times as it has locks to open from where it
    /// is: each door, slot and flag needing it in its segment and the ones
    /// after, a lock shared by two door sides counted once. Any other key
    /// comes in its catalogue amount, or one.
    fn key_amount(&self, map: &Map, key: u8, flag: u16) -> u16 {
        let def = map.items.get(&key);
        let amount = def.and_then(|def| def.amount).map_or(1, u16::from);
        if !def.is_some_and(|def| def.discard) {
            return amount;
        }
        let Some(segment) = self.containing(map, flag) else {
            return amount;
        };
        let mut rooms = Vec::new();
        self.descendant_rooms(segment, &mut rooms);
        let needs = Requirement::Item(key);
        let mut locks = HashSet::new();
        let mut count = 0u16;
        for room in &rooms {
            let room = &map.rooms[room.as_str()];
            for door in room
                .doors
                .iter()
                .filter(|door| door.requires.contains(&needs))
            {
                if door.lock.is_none_or(|lock| locks.insert(lock)) {
                    count += 1;
                }
            }
            count += room
                .flags
                .iter()
                .filter(|flag| flag.requires.contains(&needs))
                .count() as u16;
            count += room
                .items
                .iter()
                .filter(|item| item.requires.contains(&needs))
                .count() as u16;
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::{PlacedKey, Slot};
    use crate::map::{BeginEnd, Door, Item, Player};

    fn def(kind: &str, max: u16, ammo: &[u8]) -> ItemDef {
        ItemDef {
            name: kind.into(),
            kind: kind.into(),
            max,
            ammo: ammo.to_vec(),
            ..ItemDef::default()
        }
    }

    fn slot(id: u8, flag: u16) -> Item {
        Item {
            id,
            flag: Some(flag),
            group: u32::MAX,
            ..Item::default()
        }
    }

    fn door(
        target: &str,
        kind: Option<DoorKind>,
        lock: Option<u8>,
        requires: &[Requirement],
    ) -> Door {
        Door {
            id: Some(0),
            target: Some(target.into()),
            kind,
            lock,
            requires: requires.to_vec(),
            ..Door::default()
        }
    }

    /// Two rooms of ten slots each, the second past a no-return door, and a
    /// catalogue with two weapon groups, their ammo, a herb, ink and a key.
    fn map() -> Map {
        let mut map = Map {
            begin_end: vec![BeginEnd {
                start: "A".into(),
                end: "B".into(),
                ..BeginEnd::default()
            }],
            ..Map::default()
        };
        map.items.insert(1, def("weapon/knife", 1, &[]));
        map.items.insert(2, def("weapon/handgun", 15, &[11]));
        map.items.insert(4, def("weapon/colt/dumdum", 6, &[13]));
        map.items.insert(5, def("weapon/colt/magnum", 6, &[14]));
        map.items.insert(11, def("ammo/handgun", 100, &[]));
        map.items.insert(13, def("ammo/colt/dumdum", 100, &[]));
        map.items.insert(14, def("ammo/colt/magnum", 100, &[]));
        map.items.insert(68, def("health/g", 1, &[]));
        map.items.insert(47, def("ink", 100, &[]));
        map.items.insert(
            52,
            ItemDef {
                kind: "key/reusuable".into(),
                discard: true,
                ..ItemDef::default()
            },
        );
        let armor = Requirement::Item(52);
        let a = Room {
            key: "A".into(),
            doors: vec![
                door("B", Some(DoorKind::NoReturn), None, &[]),
                door("A", None, Some(7), std::slice::from_ref(&armor)),
                door("A", None, Some(7), std::slice::from_ref(&armor)),
            ],
            items: (0..10).map(|i| slot(i, 100 + u16::from(i))).collect(),
            ..Room::default()
        };
        let b = Room {
            key: "B".into(),
            doors: vec![door(
                "A",
                Some(DoorKind::Blocked),
                Some(8),
                std::slice::from_ref(&armor),
            )],
            items: (0..10).map(|i| slot(i, 200 + u16::from(i))).collect(),
            ..Room::default()
        };
        map.rooms.insert("A".into(), a);
        map.rooms.insert("B".into(), b);
        map
    }

    /// Every weapon on, ammo, herbs and ink in equal parts.
    fn config() -> Config {
        let mut config = Config::new();
        for kind in [
            "weapon/knife",
            "weapon/handgun",
            "weapon/colt/dumdum",
            "weapon/colt/magnum",
        ] {
            config.set(format!("items/weapon/enabled/{kind}"), true);
            config.set(format!("items/weapon/initial/min/{kind}"), 1);
            config.set(format!("items/weapon/initial/max/{kind}"), 6);
        }
        for kind in [
            "ammo/handgun",
            "ammo/colt/dumdum",
            "ammo/colt/magnum",
            "health/g",
            "ink",
        ] {
            config.set(format!("items/distribution/{kind}"), 0.5);
            config.set(format!("items/stack/min/{kind}"), 2);
            config.set(format!("items/stack/max/{kind}"), 9);
        }
        config
    }

    fn no_inventory() -> StartInventory {
        StartInventory {
            player: Player::Chris,
            slots: Vec::new(),
            weapons: Vec::new(),
        }
    }

    fn no_keys() -> KeyPlacement {
        KeyPlacement {
            keys: Vec::new(),
            all_nodes_visited: true,
            dead_ends: 0,
            log: Vec::new(),
        }
    }

    fn items(plan: &ItemPlan) -> Vec<(u8, u16, Role)> {
        plan.slots
            .values()
            .filter_map(|pickup| match *pickup {
                Pickup::Item { item, amount, role } => Some((item, amount, role)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn every_slot_is_filled_or_emptied() {
        let map = map();
        for seed in 0..20 {
            let plan = randomize(
                &map,
                &config(),
                &no_inventory(),
                &no_keys(),
                &mut Rng::new(seed),
            )
            .unwrap();
            assert_eq!(plan.slots.len(), 20);
            assert!(
                plan.slots
                    .values()
                    .all(|pickup| *pickup != Pickup::Original)
            );
        }
    }

    #[test]
    fn one_weapon_per_group_and_none_the_player_has() {
        let map = map();
        let mut inventory = no_inventory();
        inventory.weapons.push(2);
        for seed in 0..20 {
            let plan =
                randomize(&map, &config(), &inventory, &no_keys(), &mut Rng::new(seed)).unwrap();
            let mut weapons: Vec<u8> = items(&plan)
                .into_iter()
                .filter(|(_, _, role)| *role == Role::Weapon)
                .map(|(item, _, _)| item)
                .collect();
            weapons.sort_unstable();
            // The knife, and one of the two Colts: they share a group.
            assert_eq!(weapons.len(), 2, "seed {seed}: {weapons:?}");
            assert_eq!(weapons[0], 1);
            assert!([4, 5].contains(&weapons[1]));
        }
    }

    #[test]
    fn only_ammo_for_weapons_the_player_can_have_is_placed() {
        let map = map();
        let mut config = config();
        config.set("items/weapon/enabled/weapon/colt/dumdum", false);
        config.set("items/weapon/enabled/weapon/colt/magnum", false);
        config.set("items/weapon/enabled/weapon/handgun", false);
        let mut inventory = no_inventory();
        // Started with, though the world may not place it.
        inventory.weapons.push(5);
        for seed in 0..20 {
            let plan =
                randomize(&map, &config, &inventory, &no_keys(), &mut Rng::new(seed)).unwrap();
            let placed: HashSet<u8> = items(&plan).iter().map(|(item, _, _)| *item).collect();
            assert!(
                !placed.contains(&11) && !placed.contains(&13),
                "seed {seed}"
            );
            assert!(
                placed.contains(&14),
                "seed {seed}: magnum rounds for the start's magnum"
            );
        }
    }

    #[test]
    fn amounts_stay_within_the_stack_range_and_the_maximum() {
        let map = map();
        let mut config = config();
        config.set("items/stack/min/health/g", 3);
        for seed in 0..20 {
            let plan = randomize(
                &map,
                &config,
                &no_inventory(),
                &no_keys(),
                &mut Rng::new(seed),
            )
            .unwrap();
            for (item, amount, role) in items(&plan) {
                let max = map.items[&item].max;
                assert!(amount <= max, "seed {seed}: {item} x{amount}");
                match role {
                    Role::Weapon => assert!((1..=6).contains(&amount)),
                    Role::Supply if max > 1 => assert!((2..=9).contains(&amount)),
                    Role::Supply => assert_eq!(amount, 1, "a herb is one"),
                    Role::Key => unreachable!(),
                }
            }
        }
    }

    #[test]
    fn distribution_follows_the_weights() {
        let map = map();
        let mut config = config();
        for kind in [
            "ammo/handgun",
            "ammo/colt/dumdum",
            "ammo/colt/magnum",
            "ink",
        ] {
            config.set(format!("items/distribution/{kind}"), 0.0);
        }
        for kind in [
            "weapon/knife",
            "weapon/handgun",
            "weapon/colt/dumdum",
            "weapon/colt/magnum",
        ] {
            config.set(format!("items/weapon/enabled/{kind}"), false);
        }
        let plan = randomize(&map, &config, &no_inventory(), &no_keys(), &mut Rng::new(3)).unwrap();
        assert!(items(&plan).iter().all(|(item, _, _)| *item == 68));
        assert_eq!(items(&plan).len(), 20, "herbs everywhere");

        config.set("items/distribution/health/g", 0.0);
        let plan = randomize(&map, &config, &no_inventory(), &no_keys(), &mut Rng::new(3)).unwrap();
        assert!(plan.slots.values().all(|pickup| *pickup == Pickup::Empty));
    }

    #[test]
    fn documents_keep_their_document_unless_replaced() {
        let mut map = map();
        map.rooms["A"].items[0].document = true;
        let mut config = config();
        let plan = randomize(&map, &config, &no_inventory(), &no_keys(), &mut Rng::new(1)).unwrap();
        assert_eq!(plan.slots[&100], Pickup::Original);
        config.set("items/documents", true);
        let plan = randomize(&map, &config, &no_inventory(), &no_keys(), &mut Rng::new(1)).unwrap();
        assert_ne!(plan.slots[&100], Pickup::Original);
    }

    #[test]
    fn weapons_go_to_normal_slots_first() {
        let mut map = map();
        for item in &mut map.rooms["A"].items {
            item.hidden = true;
        }
        for item in &mut map.rooms["B"].items {
            item.optional = true;
        }
        map.rooms["B"].items[4].optional = false;
        let plan = randomize(
            &map,
            &config(),
            &no_inventory(),
            &no_keys(),
            &mut Rng::new(1),
        )
        .unwrap();
        // Hidden slots are low unless hidden keys are on, so the one normal
        // slot gets the first weapon.
        assert!(matches!(
            plan.slots[&204],
            Pickup::Item {
                role: Role::Weapon,
                ..
            }
        ));
    }

    #[test]
    fn keys_keep_their_slots_and_count_their_locks_from_their_segment() {
        let map = map();
        let key = |room: &str, item: u8, flag: u16| PlacedKey {
            slot: Slot {
                room: room.into(),
                item,
                flag: Some(flag),
            },
            key: 52,
        };
        let keys = KeyPlacement {
            keys: vec![key("A", 3, 103), key("B", 5, 205)],
            ..no_keys()
        };
        let plan = randomize(&map, &config(), &no_inventory(), &keys, &mut Rng::new(1)).unwrap();
        // From A: lock 7 (two sides, once) and B's lock 8. From B: lock 8.
        assert_eq!(
            plan.slots[&103],
            Pickup::Item {
                item: 52,
                amount: 2,
                role: Role::Key
            }
        );
        assert_eq!(
            plan.slots[&205],
            Pickup::Item {
                item: 52,
                amount: 1,
                role: Role::Key
            }
        );
        let keys = KeyPlacement {
            keys: vec![key("A", 3, 103), key("A", 4, 103)],
            ..no_keys()
        };
        assert!(randomize(&map, &config(), &no_inventory(), &keys, &mut Rng::new(1)).is_err());
    }

    #[test]
    fn a_seed_gives_the_same_plan() {
        let map = map();
        let a = randomize(
            &map,
            &config(),
            &no_inventory(),
            &no_keys(),
            &mut Rng::new(9),
        )
        .unwrap();
        let b = randomize(
            &map,
            &config(),
            &no_inventory(),
            &no_keys(),
            &mut Rng::new(9),
        )
        .unwrap();
        assert_eq!(a, b);
    }
}
