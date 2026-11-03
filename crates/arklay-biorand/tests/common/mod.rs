//! What the RE1 integration tests share: loading the real map, and a
//! play-through of a key placement that walks the map itself rather than the
//! routing graph.

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;

use arklay_biorand::keys::{KeyOptions, KeyPlacement};
use arklay_biorand::locks::{self, DoorLock};
use arklay_biorand::map::{DoorKind, Filter, Map, Requirement};

pub const RE1: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../games/re1");

/// The RE1 map as the room files and `biorand/map.xml` give it.
pub fn load_map() -> Map {
    let mut files: Vec<(String, String)> = fs::read_dir(format!("{RE1}/room"))
        .expect("games/re1/room exists")
        .map(|entry| {
            let entry = entry.expect("a readable room entry");
            let dir = entry.file_name().into_string().expect("a UTF-8 room name");
            let text = fs::read_to_string(entry.path().join("random.xml"))
                .expect("a random.xml in every room directory");
            (dir, text)
        })
        .collect();
    files.sort();
    let map_xml =
        fs::read_to_string(format!("{RE1}/biorand/map.xml")).expect("biorand/map.xml exists");
    let rooms = files
        .iter()
        .map(|(dir, text)| (dir.as_str(), text.as_str()));
    Map::parse(&map_xml, rooms).unwrap_or_else(|err| panic!("{err:#}"))
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
            problems.push(format!("the end room {end} is never reached"));
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
                What::Door { to, .. } => {
                    !is_no_return(action) && !action.requires.is_empty() && !p.reached.contains(to)
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
            first_failure.get_or_insert(problems);
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
        Err(first_failure.unwrap_or_default().join("\n"))
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
