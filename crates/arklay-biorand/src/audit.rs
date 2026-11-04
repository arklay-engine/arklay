//! Static checks of a planned game, shared by the verifier, the seed sweep
//! and the integration tests: [`check_game`] walks the plan on the map it was
//! planned on, and [`check_locks`] and [`check_doors`] check the plan's own
//! consistency.
//!
//! The walk ([`World`]) is independent of the routing graph. It plays the
//! map itself from the start room: doors with their kinds, locks and
//! requirements, graph flags raised by room events, pickups, consumable keys,
//! one-way and no-return doors. [`World::search`] tries every order of
//! spending consumable keys; [`World::first_come`] spends each on the first
//! lock found, which is the "greedy spend" a careless player makes (common
//! 1.6.1 does not rule it out; see `docs/biorand/plan.md`, Known issues).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::doors::{self, DoorPlan};
use crate::items::{Pickup, Role};
use crate::keys::{KeyOptions, KeyPlacement};
use crate::locks::{self, DoorLock};
use crate::map::{DoorKind, Filter, Map, Requirement};
use crate::plan::Game;
use crate::prepare;
use crate::rng::Rng;

/// `map` (prepared, with the original doors) rewired as the door `plan`
/// says, for a walk: only the rooms the plan's doors are in or lead to, each
/// door to its planned far side, a segment end no-return with its far door
/// blocked, and a sealed door shut. Worked out from the plan alone, not from
/// the map the door stage returned.
pub fn with_doors(map: &Map, plan: &DoorPlan) -> Result<Map, String> {
    let mut map = map.clone();
    // The rooms the plan's doors are in or lead to, and those reached from
    // them through exits with no door AOT (`20C/GRAVE`).
    let mut kept: HashSet<String> = plan
        .doors
        .iter()
        .flat_map(|door| {
            std::iter::once(door.room.clone()).chain(door.target.as_ref().map(|t| t.room.clone()))
        })
        .collect();
    loop {
        let more: Vec<String> = map
            .rooms
            .values()
            .filter(|room| kept.contains(&room.key))
            .flat_map(|room| room.doors.iter().filter(|door| door.id.is_none()))
            .filter_map(|door| door.target_room().map(str::to_string))
            .filter(|to| !kept.contains(to))
            .collect();
        if more.is_empty() {
            break;
        }
        kept.extend(more);
    }
    map.rooms.retain(|key, _| kept.contains(key));
    let mut blocked = HashSet::new();
    for room in map.rooms.values_mut() {
        let key = room.key.clone();
        room.doors.retain(|door| {
            door.id.is_some() || door.target_room().is_none_or(|to| kept.contains(to))
        });
        for door in &mut room.doors {
            let Some(id) = door.id else {
                continue;
            };
            let planned = plan
                .doors
                .iter()
                .find(|d| d.room == key && d.id == id)
                .ok_or_else(|| format!("{key}:{id} is in the game but not in the plan"))?;
            match &planned.target {
                Some(target) => {
                    door.target = Some(format!("{}:{}", target.room, target.door));
                    if planned.no_return {
                        door.kind = Some(DoorKind::NoReturn);
                        blocked.insert((target.room.clone(), target.door));
                    }
                }
                None => {
                    door.target = None;
                    door.kind = Some(if door.no_unlock {
                        DoorKind::Blocked
                    } else {
                        DoorKind::Locked
                    });
                    door.requires.clear();
                }
            }
        }
    }
    for (room, id) in blocked {
        let door = map
            .rooms
            .get_mut(&room)
            .and_then(|room| room.doors.iter_mut().find(|door| door.id == Some(id)))
            .ok_or_else(|| format!("a segment ends at {room}:{id}, not in the game"))?;
        door.kind = Some(DoorKind::Blocked);
    }
    Ok(map)
}

/// `map` with the planned `locks` as the game will read them, for a walk:
/// worked out from each door's lock flag and key byte alone, not from what
/// the lock stage gave the routing map. A door that no key opens loses its
/// key requirements; a door a key opens needs that key (and keeps any other
/// requirement, such as a puzzle's); a door locked for good from this side
/// opens only from the other; a door with a lock and no key is a script's
/// and stays as the map has it.
pub fn with_locks(map: &Map, locks: &[DoorLock]) -> Result<Map, String> {
    let mut map = map.clone();
    let discard: HashSet<u8> = map
        .items
        .iter()
        .filter(|(_, def)| def.discard)
        .map(|(&id, _)| id)
        .collect();
    for lock in locks {
        let door = map
            .rooms
            .get_mut(&lock.room)
            .and_then(|room| room.doors.iter_mut().find(|door| door.id == Some(lock.id)))
            .ok_or_else(|| {
                format!(
                    "the plan locks {}:{}, which the map has not",
                    lock.room, lock.id
                )
            })?;
        let keyless = |requires: &[Requirement]| -> Vec<Requirement> {
            requires
                .iter()
                .filter(|r| !matches!(r, Requirement::Item(item) if discard.contains(item)))
                .cloned()
                .collect()
        };
        let unkind = |kind: Option<DoorKind>| match kind {
            Some(DoorKind::Locked | DoorKind::Unlock) => None,
            other => other,
        };
        if lock.lock == 0 {
            door.requires = keyless(&door.requires);
            door.kind = unkind(door.kind);
        } else if let Some(key) = lock.key_item() {
            door.requires = keyless(&door.requires);
            door.requires.push(Requirement::Item(key));
            door.kind = unkind(door.kind);
        } else if lock.key == locks::KEY_LOCKED {
            if !matches!(
                door.kind,
                Some(DoorKind::Blocked | DoorKind::NoReturn | DoorKind::OneWay)
            ) {
                door.kind = Some(DoorKind::Locked);
            }
        } else if lock.key == locks::KEY_UNLOCK {
            door.requires = keyless(&door.requires);
        }
    }
    Ok(map)
}

/// Plays `placement` through on the map. From the start room, the player
/// takes whatever needs nothing or only keys held; a consumable key is used
/// up by each door, slot or flag that takes one. Fails unless some order of
/// spending the consumable keys reaches the end room, picks up every placed
/// key and opens every locked door, slot and flag met on the way.
pub fn check_playthrough(
    map: &Map,
    options: &KeyOptions,
    placement: &KeyPlacement,
) -> Result<(), String> {
    World::new(map, options, placement).search()
}

/// The map as the walk sees it, for one placement.
pub struct World<'a> {
    options: &'a KeyOptions,
    /// Slot (room, AOT id) to key item.
    keys: HashMap<(&'a str, u8), u8>,
    consumable: HashSet<u8>,
    /// Every door, slot and flag of every room, in map order.
    actions: Vec<Action<'a>>,
    /// Each door's kind, by its room and target room.
    door_kinds: HashMap<(&'a str, &'a str), Option<DoorKind>>,
    start: &'a str,
    /// `None` when the game leaves the end room out (the lab off).
    end: Option<&'a str>,
}

/// Something the player can do once, in room `at`.
#[derive(Clone, Copy, Debug)]
struct Action<'a> {
    at: &'a str,
    what: What<'a>,
    requires: &'a [Requirement],
}

#[derive(Clone, Copy, Debug)]
enum What<'a> {
    Door { to: &'a str, kind: Option<DoorKind> },
    Slot { item: u8 },
    Flag { name: &'a str },
}

/// How far a play-through has got.
#[derive(Clone, Default, PartialEq, Eq, Hash)]
struct Progress<'a> {
    reached: BTreeSet<&'a str>,
    flags: BTreeSet<&'a str>,
    picked: BTreeSet<(&'a str, u8)>,
    /// Doors opened, by their two rooms in order.
    opened: BTreeSet<(&'a str, &'a str)>,
    held: BTreeMap<u8, u32>,
    /// What was left undone in the segments behind a no-return door.
    stranded: BTreeSet<String>,
}

fn pair<'a>(a: &'a str, b: &'a str) -> (&'a str, &'a str) {
    if a <= b { (a, b) } else { (b, a) }
}

/// Give up a search after this many play-throughs.
const SEARCH_BUDGET: usize = 100_000;

impl<'a> World<'a> {
    pub fn new(map: &'a Map, options: &'a KeyOptions, placement: &'a KeyPlacement) -> Self {
        let included = |f: &Filter| f.includes(options.door_rando, options.player);
        let begin_end = map
            .begin_end
            .iter()
            .find(|b| included(&b.filter))
            .expect("a start");
        let rooms: HashSet<&str> = map
            .rooms
            .values()
            .filter(|room| included(&room.filter))
            .map(|room| room.key.as_str())
            .collect();
        let mut actions = Vec::new();
        let mut door_kinds = HashMap::new();
        for room in map.rooms.values().filter(|room| included(&room.filter)) {
            let at = room.key.as_str();
            for door in room.doors.iter().filter(|d| included(&d.filter)) {
                let Some(to) = door.target_room() else {
                    continue;
                };
                if door.ignore_in_graph || !rooms.contains(to) {
                    continue;
                }
                door_kinds.insert((at, to), door.kind);
                let what = What::Door {
                    to,
                    kind: door.kind,
                };
                actions.push(Action {
                    at,
                    what,
                    requires: &door.requires,
                });
            }
            for item in room.items.iter().filter(|i| included(&i.filter)) {
                // The slots the randomizer uses; a locked one must open even
                // if it holds no key.
                if item.optional
                    || (item.document && !options.include_documents)
                    || (item.hidden && !options.include_hidden)
                {
                    continue;
                }
                let what = What::Slot { item: item.id };
                actions.push(Action {
                    at,
                    what,
                    requires: &item.requires,
                });
            }
            for flag in room.flags.iter().filter(|f| included(&f.filter)) {
                let what = What::Flag { name: &flag.name };
                actions.push(Action {
                    at,
                    what,
                    requires: &flag.requires,
                });
            }
        }
        Self {
            options,
            keys: placement
                .keys
                .iter()
                .map(|placed| ((placed.slot.room.as_str(), placed.slot.item), placed.key))
                .collect(),
            consumable: map
                .items
                .iter()
                .filter(|(_, def)| def.kind == "key/consumable")
                .map(|(&id, _)| id)
                .collect(),
            actions,
            door_kinds,
            start: &begin_end.start,
            end: rooms
                .contains(begin_end.end.as_str())
                .then_some(begin_end.end.as_str()),
        }
    }

    fn done(&self, p: &Progress, action: &Action) -> bool {
        match action.what {
            What::Door { to, kind } => {
                p.reached.contains(to)
                    && (matches!(kind, Some(DoorKind::OneWay | DoorKind::NoReturn))
                        || p.opened.contains(&pair(action.at, to)))
            }
            What::Slot { item } => p.picked.contains(&(action.at, item)),
            What::Flag { name } => p.flags.contains(name),
        }
    }

    /// The consumable keys `action` would use up, or `None` if it can't be
    /// done now.
    fn try_action(&self, p: &Progress, action: &Action) -> Option<Vec<u8>> {
        if !p.reached.contains(action.at) || self.done(p, action) {
            return None;
        }
        if let What::Door { to, kind } = action.what {
            if p.opened.contains(&pair(action.at, to)) {
                return Some(Vec::new());
            }
            let far = self.door_kinds.get(&(to, action.at)).copied().flatten();
            if matches!(kind, Some(DoorKind::Blocked | DoorKind::Locked))
                || far == Some(DoorKind::Unblock)
            {
                // Only opened from the other side.
                return None;
            }
        }
        let mut uses = Vec::new();
        for requirement in action.requires {
            match requirement {
                Requirement::Item(item) => {
                    if self.options.implicit_keys.contains(item) {
                        continue;
                    }
                    let need = 1 + uses.iter().filter(|&&u| u == *item).count() as u32;
                    if p.held.get(item).copied().unwrap_or(0) < need {
                        return None;
                    }
                    if self.consumable.contains(item) {
                        uses.push(*item);
                    }
                }
                Requirement::Flag(name) => {
                    if !p.flags.contains(name.as_str()) {
                        return None;
                    }
                }
                Requirement::Room(room) => {
                    if !p.reached.contains(room.as_str()) {
                        return None;
                    }
                }
            }
        }
        Some(uses)
    }

    fn apply(&self, p: &mut Progress<'a>, action: &Action<'a>, uses: &[u8]) {
        for item in uses {
            *p.held.get_mut(item).expect("a held key") -= 1;
        }
        match action.what {
            What::Door { to, kind } => {
                if kind == Some(DoorKind::NoReturn) {
                    // Everything behind is out of reach for good.
                    let left = self.stuck(p);
                    p.stranded.extend(left);
                    p.reached.clear();
                } else if kind != Some(DoorKind::OneWay) {
                    p.opened.insert(pair(action.at, to));
                }
                p.reached.insert(to);
            }
            What::Slot { item } => {
                p.picked.insert((action.at, item));
                if let Some(&key) = self.keys.get(&(action.at, item)) {
                    *p.held.entry(key).or_default() += 1;
                }
            }
            What::Flag { name } => {
                p.flags.insert(name);
            }
        }
    }

    /// Whether spending `uses` can't hurt: as many of each key are held as
    /// every lock not yet opened anywhere takes.
    fn is_safe(&self, p: &Progress, uses: &[u8]) -> bool {
        uses.iter().all(|&key| {
            let demand: usize = self
                .actions
                .iter()
                .filter(|action| !self.done(p, action))
                .map(|action| {
                    action
                        .requires
                        .iter()
                        .filter(|r| **r == Requirement::Item(key))
                        .count()
                })
                .sum();
            p.held.get(&key).copied().unwrap_or(0) as usize >= demand
        })
    }

    /// Does everything that uses up no key, or only keys that can be spared,
    /// until nothing is left of either.
    fn settle(&self, p: &mut Progress<'a>) {
        loop {
            let next = self.actions.iter().find_map(|action| {
                if is_no_return(action) {
                    return None;
                }
                let uses = self.try_action(p, action)?;
                (uses.is_empty() || self.is_safe(p, &uses)).then_some((action, uses))
            });
            match next {
                Some((action, uses)) => self.apply(p, action, &uses),
                None => return,
            }
        }
    }

    /// What is wrong with a finished play-through.
    fn problems(&self, p: &Progress) -> Vec<String> {
        let mut problems = Vec::new();
        if let Some(end) = self.end
            && !p.reached.contains(end)
        {
            problems.push(format!(
                "the end room {end} is never reached; reached {:?}",
                p.reached
            ));
        }
        let mut unpicked: Vec<_> = self
            .keys
            .iter()
            .filter(|(slot, _)| !p.picked.contains(slot))
            .collect();
        unpicked.sort();
        if !unpicked.is_empty() {
            problems.push(format!("keys never picked up: {unpicked:?}"));
        }
        let mut stuck: Vec<String> = p.stranded.iter().cloned().collect();
        stuck.extend(self.stuck(p));
        if !stuck.is_empty() {
            let held: Vec<_> = p.held.iter().filter(|(_, n)| **n > 0).collect();
            problems.push(format!(
                "locks that never open: {stuck:?}, holding {held:?}"
            ));
        }
        problems
    }

    /// The locked doors, slots and flags of the rooms reached that are not
    /// done.
    fn stuck(&self, p: &Progress) -> Vec<String> {
        self.actions
            .iter()
            .filter(|action| p.reached.contains(action.at) && !self.done(p, action))
            .filter(|action| match action.what {
                // A blocked door is never opened from this side, key or not.
                What::Door { to, kind } => {
                    !is_no_return(action)
                        && kind != Some(DoorKind::Blocked)
                        && !action.requires.is_empty()
                        && !p.reached.contains(to)
                }
                What::Slot { .. } | What::Flag { .. } => true,
            })
            .map(|action| format!("{action:?}"))
            .collect()
    }

    fn begin(&self) -> Progress<'a> {
        let mut p = Progress::default();
        p.reached.insert(self.start);
        p
    }

    /// Searches the orders of spending keys that can't be spared.
    fn search(&self) -> Result<(), String> {
        let mut seen = HashSet::new();
        let mut budget = SEARCH_BUDGET;
        let mut first_failure = None;
        let mut stack = vec![self.begin()];
        while let Some(mut p) = stack.pop() {
            self.settle(&mut p);
            let problems = self.problems(&p);
            if problems.is_empty() {
                return Ok(());
            }
            // Report the play-through that got furthest: the most slots
            // emptied (a no-return door forgets the rooms reached).
            let progress = (p.picked.len(), p.reached.len());
            if first_failure
                .as_ref()
                .is_none_or(|(furthest, _)| progress > *furthest)
            {
                first_failure = Some((progress, problems));
            }
            if budget == 0 {
                return Err("the search ran out of budget".to_string());
            }
            budget -= 1;
            // Push in reverse, so the first choice is tried first.
            for action in self.actions.iter().rev() {
                if let Some(uses) = self.try_action(&p, action) {
                    let mut child = p.clone();
                    self.apply(&mut child, action, &uses);
                    if seen.insert(child.clone()) {
                        stack.push(child);
                    }
                }
            }
        }
        Err(first_failure.unwrap_or_default().1.join("\n"))
    }

    /// Plays through spending each consumable key on the first lock found.
    pub fn first_come(&self) -> Result<(), String> {
        let mut p = self.begin();
        loop {
            self.settle(&mut p);
            let mut doable = self
                .actions
                .iter()
                .filter_map(|action| Some((action, self.try_action(&p, action)?)));
            let next = doable
                .clone()
                .find(|(action, _)| !is_no_return(action))
                .or_else(|| doable.find(|(action, _)| is_no_return(action)));
            match next {
                Some((action, uses)) => self.apply(&mut p, action, &uses),
                None => break,
            }
        }
        let problems = self.problems(&p);
        if problems.is_empty() {
            Ok(())
        } else {
            Err(problems.join("\n"))
        }
    }
}

fn is_no_return(action: &Action) -> bool {
    matches!(
        action.what,
        What::Door {
            kind: Some(DoorKind::NoReturn),
            ..
        }
    )
}

/// The plan's locks hold together: every door of the map is planned once,
/// both sides of a pair share a lock, a new lock takes a lock flag no
/// original room script raises and a key that may go there, and a door
/// leading nowhere never opens.
pub fn check_locks(map: &Map, game: &Game) -> Result<(), String> {
    let prepared = game_map(map, game)?;
    let doors: usize = prepared
        .rooms
        .values()
        .map(|room| room.doors.iter().filter(|door| door.id.is_some()).count())
        .sum();
    if game.locks.len() != doors {
        return Err(format!("{} door locks for {doors} doors", game.locks.len()));
    }
    let by_name: HashMap<String, &DoorLock> = game
        .locks
        .iter()
        .map(|lock| (format!("{}:{}", lock.room, lock.id), lock))
        .collect();
    let mut flags: HashMap<u8, HashSet<String>> = HashMap::new();
    for lock in &game.locks {
        let name = format!("{}:{}", lock.room, lock.id);
        if lock.lock == 0 && lock.key != 0 {
            return Err(format!("{name} is unlocked with key {}", lock.key));
        }
        if lock.target.is_none() && (lock.lock, lock.key) != (255, 255) {
            return Err(format!("{name} leads nowhere but opens"));
        }
        if let Some(key) = lock.key_item() {
            let def = map
                .items
                .get(&key)
                .ok_or_else(|| format!("{name}: key {key}"))?;
            if lock.drawn && !def.discard {
                return Err(format!(
                    "{name}: drawn key {} is not a discard key",
                    def.name
                ));
            }
        }
        if lock.drawn && lock.lock != 0 {
            let other = lock
                .other
                .as_ref()
                .and_then(|other| by_name.get(other))
                .ok_or_else(|| format!("{name} is drawn without a far side"))?;
            if other.lock != lock.lock
                || (other.key_item() != lock.key_item() && other.key != 255 && lock.key != 255)
            {
                return Err(format!("{name} {lock:?} and its far side {other:?} differ"));
            }
            flags.entry(lock.lock).or_default().insert(name.clone());
        }
    }
    for (flag, doors) in &flags {
        if doors.len() > 2 {
            return Err(format!("lock flag {flag} is shared by {doors:?}"));
        }
        if !game.config.get_bool("locks/preserve", false) && *flag < 64 {
            return Err(format!(
                "new lock flag {flag} (of {doors:?}) is one a room script may raise"
            ));
        }
    }
    Ok(())
}

/// The map prepared for a game's configuration, with the original doors.
pub fn prepared(map: &Map, game: &Game) -> Result<Map, String> {
    let options = prepare::Options::from_config(&game.config, game.player);
    prepare::prepare(map, &options, &mut Rng::named(game.spoiler.seed, "re1map"))
        .map_err(|err| format!("{err:#}"))
}

/// The map a game was planned on: prepared for its configuration, and
/// rewired as its door plan says.
pub fn game_map(map: &Map, game: &Game) -> Result<Map, String> {
    let prepared = prepared(map, game)?;
    match &game.doors {
        Some(plan) => with_doors(&prepared, plan),
        None => Ok(prepared),
    }
}

/// Checks one game against the map it was planned on.
pub fn check_game(map: &Map, game: &Game) -> Result<(), String> {
    let prepared = game_map(map, game)?;

    // The start fits the player's slots.
    let size = usize::from(game.inventory_sizes.of(game.player));
    if game.inventory.slots.len() > size {
        return Err(format!(
            "{} start slots for {size}",
            game.inventory.slots.len()
        ));
    }

    // Every slot of the game is planned once, and nothing else is.
    let mut expected = HashSet::new();
    let mut by_flag: HashMap<u16, Pickup> = HashMap::new();
    for room in prepared.rooms.values() {
        let dir = room.room_id().to_ascii_lowercase();
        for item in &room.items {
            let flag = item.flag.ok_or("a slot without a pickup flag")?;
            expected.insert((dir.clone(), item.id));
            let slot = game
                .items
                .get(&dir)
                .and_then(|slots| slots.get(&item.id))
                .ok_or_else(|| format!("{} AOT {} is not planned", room.key, item.id))?;
            if slot.flag != flag {
                return Err(format!(
                    "{} AOT {}: flag {} for {flag}",
                    room.key, item.id, slot.flag
                ));
            }
            // Slots sharing a flag hold the same.
            if let Some(other) = by_flag.insert(flag, slot.pickup)
                && other != slot.pickup
            {
                return Err(format!("flag {flag} holds two things"));
            }
            check_pickup(&prepared, game, item, slot.pickup)
                .map_err(|err| format!("{} AOT {} ({:?}): {err}", room.key, item.id, item.name))?;
        }
    }
    let planned: usize = game.items.values().map(|slots| slots.len()).sum();
    if planned != expected.len() {
        return Err(format!("{planned} slots planned for {}", expected.len()));
    }

    // One world weapon per group, none of a group the player starts with.
    let group = |item: u8| {
        map.items[&item]
            .kind
            .split('/')
            .nth(1)
            .unwrap_or("")
            .to_string()
    };
    let mut groups: HashSet<String> = game.inventory.weapons.iter().map(|&w| group(w)).collect();
    for pickup in by_flag.values() {
        if let Pickup::Item {
            item,
            role: Role::Weapon,
            ..
        } = *pickup
            && !groups.insert(group(item))
        {
            return Err(format!("a second weapon of group {}", group(item)));
        }
    }

    // The keys are where the key stage put them, and the game can be played
    // through with them.
    let Some(keys) = &game.keys else {
        return Err("items are randomized, but no keys were placed".into());
    };
    for placed in &keys.keys {
        let dir = placed
            .slot
            .room
            .split('/')
            .next()
            .unwrap()
            .to_ascii_lowercase();
        let slot = &game.items[&dir][&placed.slot.item];
        match slot.pickup {
            Pickup::Item {
                item,
                role: Role::Key,
                ..
            } if item == placed.key => {}
            other => return Err(format!("key {} became {other:?}", placed.key)),
        }
    }
    let key_slots = by_flag
        .values()
        .filter(|pickup| {
            matches!(
                pickup,
                Pickup::Item {
                    role: Role::Key,
                    ..
                }
            )
        })
        .count();
    if key_slots != keys.keys.len() {
        return Err(format!(
            "{key_slots} key slots for {} keys",
            keys.keys.len()
        ));
    }
    let key_options = KeyOptions::from_config(&game.config, game.player);
    let walked = with_locks(&prepared, &game.locks)?;
    check_playthrough(&walked, &key_options, keys)
}

/// What a slot holds fits it: its group takes the item, the amount is within
/// the item's limit, and only a document left alone keeps the original.
fn check_pickup(
    map: &Map,
    game: &Game,
    slot: &crate::map::Item,
    pickup: Pickup,
) -> Result<(), String> {
    match pickup {
        Pickup::Original => {
            let documents = game.config.get_bool("items/documents", false);
            if slot.document && !documents {
                Ok(())
            } else {
                Err("left as the original".into())
            }
        }
        Pickup::Empty => Ok(()),
        Pickup::Item { item, amount, role } => {
            let def = map.items.get(&item).ok_or("not in the catalogue")?;
            if slot.group & def.group != def.group {
                return Err(format!(
                    "group {:#x} doesn't take {} ({:#x})",
                    slot.group, def.name, def.group
                ));
            }
            match role {
                Role::Key => {
                    if !def.kind.starts_with("key/") {
                        return Err(format!("{} as a key", def.name));
                    }
                    if def.discard && amount == 0 {
                        return Err(format!("{} with no lock to open", def.name));
                    }
                    if !def.discard && amount != def.amount.map_or(1, u16::from) {
                        return Err(format!("{} x{amount}", def.name));
                    }
                }
                Role::Weapon | Role::Supply => {
                    if amount > def.max {
                        return Err(format!("{} x{amount}, above {}", def.name, def.max));
                    }
                    if role == Role::Supply && amount == 0 {
                        return Err(format!("{} x0", def.name));
                    }
                }
            }
            Ok(())
        }
    }
}

/// The door plan holds together: every room has one new id, numbered from 0
/// in each stage, none is 311's; every door of the game is planned once and
/// leads to a door of the game that leads back; the far side's entrance is
/// the map's; the start room is in the first stage.
pub fn check_doors(map: &Map, game: &Game) -> Result<(), String> {
    let plan = game
        .doors
        .as_ref()
        .ok_or("doors are randomized, but no plan")?;
    let original = prepared(map, game)?;
    let walked = game_map(map, game)?;
    let mut ids = HashSet::new();
    let mut numbers: HashMap<u8, Vec<u8>> = HashMap::new();
    for (dir, room) in &plan.rooms {
        if room.id != doors::room_id(room.stage, room.room) {
            return Err(format!("{dir}: id {} for {room:?}", room.id));
        }
        if room.id == "311" || !ids.insert(room.id.clone()) {
            return Err(format!("{dir}: id {} is taken", room.id));
        }
        if room.stage == 0 || room.stage > plan.segments {
            return Err(format!("{dir}: stage {} of {}", room.stage, plan.segments));
        }
        numbers.entry(room.stage).or_default().push(room.room);
    }
    for (stage, mut rooms) in numbers {
        rooms.sort_unstable();
        let expected: Vec<u8> = (0..=u8::MAX)
            .filter(|&n| doors::room_id(stage, n) != "311")
            .take(rooms.len())
            .collect();
        if rooms != expected {
            return Err(format!("stage {stage} numbers its rooms {rooms:?}"));
        }
    }
    let start = &map.begin_end[0].start;
    match plan.rooms.get(&start.to_ascii_lowercase()) {
        Some(room) if room.stage == 1 => {}
        other => return Err(format!("the start room {start} is {other:?}")),
    }

    let planned_doors: usize = walked
        .rooms
        .values()
        .map(|room| room.doors.iter().filter(|door| door.id.is_some()).count())
        .sum();
    if planned_doors != plan.doors.len() {
        return Err(format!(
            "{} planned doors for {planned_doors}",
            plan.doors.len()
        ));
    }
    let by_name: HashMap<(&str, u8), &doors::PlannedDoor> = plan
        .doors
        .iter()
        .map(|door| ((door.room.as_str(), door.id), door))
        .collect();
    if by_name.len() != plan.doors.len() {
        return Err("a door is planned twice".into());
    }
    for door in &plan.doors {
        let name = format!("{}:{}", door.room, door.id);
        if !plan.rooms.contains_key(&door.dir) {
            return Err(format!("{name}: room {} has no new id", door.dir));
        }
        let Some(target) = &door.target else {
            if door.no_return {
                return Err(format!("{name} ends a segment but leads nowhere"));
            }
            continue;
        };
        let far = by_name
            .get(&(target.room.as_str(), target.door))
            .ok_or_else(|| {
                format!(
                    "{name} leads to {}:{}, not in the game",
                    target.room, target.door
                )
            })?;
        if plan.new_id(&target.dir) != Some(target.id.as_str()) {
            return Err(format!(
                "{name}: target id {} for {}",
                target.id, target.dir
            ));
        }
        let back = far.target.as_ref().map(|t| (t.room.as_str(), t.door));
        if back != Some((door.room.as_str(), door.id)) {
            return Err(format!(
                "{name} leads to {}:{}, which leads to {back:?}",
                far.room, far.id
            ));
        }
        let entrance = original.rooms[&target.room]
            .doors
            .iter()
            .find(|d| d.id == Some(target.door))
            .and_then(|d| d.entrance);
        if entrance != Some(target.entrance) {
            return Err(format!(
                "{name}: entrance {:?} for {entrance:?}",
                target.entrance
            ));
        }
        if !door.randomized {
            let was = original.rooms[&door.room]
                .doors
                .iter()
                .find(|d| d.id == Some(door.id))
                .and_then(|d| d.target.clone());
            if was != Some(format!("{}:{}", target.room, target.door)) {
                return Err(format!("{name} is fixed but leads elsewhere than {was:?}"));
            }
        }
    }
    Ok(())
}

/// Every check of a planned game, in one list of problems (empty when it
/// passes): the plan's own consistency ([`check_doors`], [`check_locks`]),
/// the walk ([`check_game`]), and the static checks ([`check_start`],
/// [`check_enemies`], [`check_map_data`]).
pub fn check_all(map: &Map, game: &Game) -> Vec<String> {
    let mut problems = Vec::new();
    let mut run = |name: &str, result: Result<(), String>| {
        if let Err(err) = result {
            problems.push(format!("{name}: {err}"));
        }
    };
    run("start inventory", check_start(map, game));
    run("enemies", check_enemies(map, game));
    run("flags", check_flags(game));
    if game.doors.is_some() {
        run("doors", check_doors(map, game));
    }
    if !game.locks.is_empty() {
        run("locks", check_locks(map, game));
    }
    if game.keys.is_some() {
        run("walk", check_game(map, game));
    }
    problems
}

/// Whether a player who spends each consumable key on the first lock found
/// (the "greedy spend") still finishes the game. The route finder does not
/// rule a stranded player out (plan.md, Known issues), so a failure here is a
/// measurement, not a bug in the plan. `Ok` also when the game has no keys.
pub fn greedy_spend(map: &Map, game: &Game) -> Result<(), String> {
    let Some(keys) = &game.keys else {
        return Ok(());
    };
    let prepared = game_map(map, game)?;
    let walked = with_locks(&prepared, &game.locks)?;
    let options = KeyOptions::from_config(&game.config, game.player);
    World::new(&walked, &options, keys).first_come()
}

/// The start inventory is made of catalogue items in amounts they allow, and
/// fits the player's slots with its weapons among them.
pub fn check_start(map: &Map, game: &Game) -> Result<(), String> {
    let size = usize::from(game.inventory_sizes.of(game.player));
    if game.inventory.slots.len() > size {
        return Err(format!(
            "{} start slots for {size}",
            game.inventory.slots.len()
        ));
    }
    for slot in &game.inventory.slots {
        let def = map
            .items
            .get(&slot.item)
            .ok_or_else(|| format!("item {} is not in the catalogue", slot.item))?;
        if slot.amount > def.max.max(1) {
            return Err(format!("{} x{}, above {}", def.name, slot.amount, def.max));
        }
        // A weapon's amount is the rounds loaded, and may be none.
        if slot.amount == 0 && !def.kind.starts_with("weapon/") {
            return Err(format!("{} x0", def.name));
        }
    }
    for weapon in &game.inventory.weapons {
        if !game.inventory.slots.iter().any(|slot| slot.item == *weapon) {
            return Err(format!("weapon {weapon} is not in a start slot"));
        }
    }
    Ok(())
}

/// The planned enemies are of the kind their room names (the type is one of
/// the kind's ids, the effect sheets are the kind's), sit in free slots of
/// a room that is in the game, and take kill flags that no other enemy of
/// their room directory uses.
pub fn check_enemies(map: &Map, game: &Game) -> Result<(), String> {
    use crate::enemies::Placement;
    let kinds: HashMap<&str, &crate::map::EnemyEntry> = map
        .enemies
        .iter()
        .flat_map(|group| &group.entries)
        .map(|entry| (entry.key.as_str(), entry))
        .collect();
    if !game.config.get_bool("enemies/random", false) {
        return if game.enemies.rooms.is_empty() {
            Ok(())
        } else {
            Err("enemies are not randomized, but the plan changes some".into())
        };
    }
    let in_game: HashSet<String> = prepared(map, game)?
        .rooms
        .values()
        .map(|room| room.key.clone())
        .collect();
    let mut kill_flags: HashMap<String, HashSet<u8>> = HashMap::new();
    for (key, room) in &game.enemies.rooms {
        if !in_game.contains(key) {
            return Err(format!("{key} gets enemies but is not in the game"));
        }
        let entry = kinds
            .get(room.kind.as_str())
            .ok_or_else(|| format!("{key}: kind {:?} is not in the catalogue", room.kind))?;
        if room.esp != entry.esp {
            return Err(format!(
                "{key}: effect sheets {:?} for {} ({:?})",
                room.esp, room.kind, entry.esp
            ));
        }
        let mut slots = HashSet::new();
        for placement in &room.placements {
            let (slot, enemy) = match placement {
                Placement::Retype { slot, enemy } => (*slot, *enemy),
                Placement::Create {
                    slot,
                    enemy,
                    kill_flag,
                    pose,
                    ..
                } => {
                    if !entry.poses.iter().any(|p| p.pose == *pose) && !entry.poses.is_empty() {
                        return Err(format!("{key}: pose {pose} is not {}'s", room.kind));
                    }
                    if !kill_flags
                        .entry(room.directory.clone())
                        .or_default()
                        .insert(*kill_flag)
                    {
                        return Err(format!(
                            "{key}: kill flag {kill_flag} is used twice in {}",
                            room.directory
                        ));
                    }
                    (*slot, *enemy)
                }
            };
            if !entry.ids.contains(&enemy) {
                return Err(format!(
                    "{key}: enemy type {enemy:#04x} is not a {} ({:?})",
                    room.kind, entry.ids
                ));
            }
            if slot >= 16 {
                return Err(format!("{key}: enemy slot {slot}"));
            }
            if !slots.insert(slot) {
                return Err(format!("{key}: two enemies in slot {slot}"));
            }
        }
    }
    Ok(())
}

/// The game's flags agree with its configuration: each option that raises a
/// system flag does, no flag is raised twice, and the scenario flags are the
/// options'.
pub fn check_flags(game: &Game) -> Result<(), String> {
    use crate::plan::system_flags::*;
    let config = &game.config;
    let expected = [
        (RANDOMIZER, true),
        (DOOR_RANDOMIZER, config.get_bool("doors/random", false)),
        (ENEMY_RANDOMIZER, config.get_bool("enemies/random", false)),
        (ITEM_RANDOMIZER, config.get_bool("items/random", false)),
        (LOCK_RANDOMIZER, config.get_bool("locks/random", false)),
        (
            BR_LOCK_FULL_BYTE,
            config.get_bool("locks/random", false) || config.get_bool("doors/random", false),
        ),
        (
            CUTSCENES_DISABLED,
            config.get_bool("cutscenes/disable", false),
        ),
        (
            FORCE_TYRANT2,
            config.get_str("progression/tyrant2", "Never") == "Always",
        ),
    ];
    for (flag, on) in expected {
        if game.system_flags.contains(&flag) != on {
            return Err(format!("{flag} is {}raised", if on { "not " } else { "" }));
        }
    }
    let mut seen = HashSet::new();
    for flag in &game.system_flags {
        if !seen.insert(flag) {
            return Err(format!("{flag} is raised twice"));
        }
    }
    let lockpick = config.get_str("inventory/special/lockpick", "Always") == "Always";
    let raised = |name: &str| {
        game.scenario_flags
            .iter()
            .find(|flag| flag.name == name)
            .map(|flag| flag.raised)
    };
    if raised("lockpick") != Some(lockpick) {
        return Err(format!(
            "the lockpick flag is {:?}, not {lockpick}",
            raised("lockpick")
        ));
    }
    if raised("hard") != Some(config.get_bool("hard", false)) {
        return Err("the hard flag differs from the option".into());
    }
    let mut numbers = HashSet::new();
    for flag in &game.scenario_flags {
        if !numbers.insert(flag.flag) {
            return Err(format!("scenario flag {} is set twice", flag.flag));
        }
    }
    Ok(())
}

/// The map's own data holds together, before any game is planned from it:
/// every requirement names an item of the catalogue, a flag some room
/// raises or a room of the map; every door's target is a door of the map
/// with an entrance; every slot has a pickup flag, and slots that share one
/// agree on what they were.
pub fn check_map_data(map: &Map) -> Result<(), String> {
    let flags: HashSet<&str> = map
        .rooms
        .values()
        .flat_map(|room| room.flags.iter().map(|flag| flag.name.as_str()))
        .collect();
    let check = |at: &str, requires: &[Requirement]| -> Result<(), String> {
        for requirement in requires {
            let ok = match requirement {
                Requirement::Item(item) => map.items.contains_key(item),
                Requirement::Flag(name) => flags.contains(name.as_str()),
                Requirement::Room(room) => map.rooms.contains_key(room),
            };
            if !ok {
                return Err(format!("{at}: {requirement} names nothing"));
            }
        }
        Ok(())
    };
    type Sharer<'a> = ((Option<u8>, Option<u8>), &'a Filter);
    let mut pickups: HashMap<u16, Vec<Sharer>> = HashMap::new();
    for room in map.rooms.values() {
        for door in &room.doors {
            let at = format!("{}:{:?}", room.key, door.id);
            check(&at, &door.requires)?;
            if let Some(target) = door.target_room() {
                let far = map
                    .rooms
                    .get(target)
                    .ok_or_else(|| format!("{at} leads to {target}, not in the map"))?;
                if let (Some(id), Some(_)) = (door.target_id(), door.id) {
                    let far_door = far
                        .doors
                        .iter()
                        .find(|d| d.id == Some(id) && d.filter == door.filter)
                        .or_else(|| far.doors.iter().find(|d| d.id == Some(id)))
                        .ok_or_else(|| format!("{at} leads to {target}:{id}, not in the map"))?;
                    if far_door.entrance.is_none() {
                        return Err(format!(
                            "{at} leads to {target}:{id}, which has no entrance"
                        ));
                    }
                }
            }
        }
        for flag in &room.flags {
            check(&format!("{} flag {}", room.key, flag.name), &flag.requires)?;
        }
        for item in &room.items {
            let at = format!("{} AOT {}", room.key, item.id);
            check(&at, &item.requires)?;
            let flag = item
                .flag
                .ok_or_else(|| format!("{at} has no pickup flag"))?;
            let was = (item.kind, item.amount);
            // Slots of one game share a flag only when they hold the same;
            // a slot for the other player or layout is not in that game.
            let overlaps = |other: &Filter| {
                (other.player.is_none()
                    || item.filter.player.is_none()
                    || other.player == item.filter.player)
                    && (other.door_rando.is_none()
                        || item.filter.door_rando.is_none()
                        || other.door_rando == item.filter.door_rando)
            };
            let seen = pickups.entry(flag).or_default();
            if seen.iter().any(|(other, f)| *other != was && overlaps(f)) {
                return Err(format!(
                    "{at}: pickup flag {flag} is shared by different items"
                ));
            }
            seen.push((was, &item.filter));
        }
    }
    Ok(())
}
