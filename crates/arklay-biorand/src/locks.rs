//! The lock stage: staging's `LockRandomizer` (`LockRandomizer.cs`), on the
//! original door layout.
//!
//! Under `locks/random`, [`randomize`] takes the map [`crate::prepare`] made
//! (original locks stripped, each door told which keys may lock it) and:
//!
//! 1. keeps one keyless route from the start to an item box clear
//!    (`KeepBoxRouteClear`);
//! 2. keeps a few doors near the start of each segment unlocked, until two
//!    pickups are in reach (`KeepRouteItemful`);
//! 3. locks a share (`locks/ratio`) of the door pairs that may take a key,
//!    each with a new lock flag and a key drawn from the keys both sides
//!    allow, `lock-priority` doors first (`RandomizeLocks`);
//! 4. reads every door's lock flag and key the way the game will
//!    (`SetDoorLocks`), as a [`DoorLock`] per door.
//!
//! The returned map carries the new locks as each door's `lock`, `key` and
//! `requires`, which the key stage routes on.
//!
//! Where arklay differs from staging, and why:
//!
//! - New lock ids are allocated from 64 up ([`lock_id_order`]). The original
//!   room scripts raise lock flags below 64 by number (11A, 308, 40E, 50C and
//!   others), so a new lock must not reuse one. Staging allocates from 1.
//! - A door leading nowhere (a gate to an area the game leaves out) is
//!   locked for good with lock 255, which nothing raises. Staging writes no
//!   lock for it, as it has no lock id, which leaves the gate open.
//! - A door marked `no-unlock` (its side can't be unlocked: the B1
//!   elevator's doors to 213 and 21C) takes no lock, nor does its far side.
//!   Staging may lock such a pair; the game then shows no key on the
//!   `no-unlock` side, while the graph routes through it with the key.
//! - A door left without a lock loses its key requirements (`drop_stale_keys`).
//! - With `locks/random` off the stage does nothing: the pack keeps the
//!   original lock bytes, and the engine reads them the original's way.

use std::collections::{BTreeSet, HashSet, VecDeque};

use anyhow::{Result, bail};
use indexmap::IndexMap;

use crate::config::Config;
use crate::map::{DoorKind, Map, Requirement, tags};
use crate::rng::Rng;

/// The key byte of a door its lock keeps shut from this side for good.
pub const KEY_LOCKED: u8 = 255;
/// The key byte of a door that unlocks from this side.
pub const KEY_UNLOCK: u8 = 254;
/// The key byte of a door no key opens: unlocked, or opened by a script.
pub const KEY_NONE: u8 = 0;
/// The lock flag of a door leading nowhere. No lock is allocated it.
pub const LOCK_NOWHERE: u8 = 255;

/// The order new lock ids are taken in: 64 to 254, then 1 to 63. The
/// original room scripts raise lock flags below 64 by number, so the low ids
/// are used last; 255 is [`LOCK_NOWHERE`].
pub fn lock_id_order() -> impl Iterator<Item = u8> {
    (64..=254).chain(1..64)
}

/// One door's lock as the game reads it, with the whole lock byte as the
/// flag (`BR_LOCK_FULL_BYTE`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DoorLock {
    /// The logical room the door is in (`10C/BACK`).
    pub room: String,
    /// The room's directory in lowercase (`10c`): the script declaring it.
    pub dir: String,
    /// The AOT id of the door's `door_aot_set`.
    pub id: u8,
    /// The room directory the door leads to, in lowercase, or `None` for a
    /// door that leads nowhere in this game. An emitter matches the
    /// `door_aot_set` by AOT id and this.
    pub target: Option<String>,
    /// The lock flag in bank 2, the whole byte; 0 is unlocked.
    pub lock: u8,
    /// What opens it: an item, [`KEY_NONE`] (no key: a script raises the
    /// flag), [`KEY_UNLOCK`] or [`KEY_LOCKED`]. [`KEY_NONE`] when unlocked.
    pub key: u8,
    /// Whether the lock randomizer drew this door's lock (locked or left
    /// unlocked); otherwise its lock comes from the map as prepared.
    pub drawn: bool,
    /// The other door of its pair, as `room:id`: the door it leads to, or
    /// the door leading to it, paired as staging pairs them. Both doors of a
    /// pair share the lock flag. `None` for a door in no pair.
    pub other: Option<String>,
}

impl DoorLock {
    /// Whether a key item opens it.
    pub fn key_item(&self) -> Option<u8> {
        (self.lock != 0 && ![KEY_NONE, KEY_UNLOCK, KEY_LOCKED].contains(&self.key))
            .then_some(self.key)
    }
}

/// A door: its room's key and its index in the room.
type DoorRef = (String, usize);

/// The lock stage. `rng` should be `Rng::named(seed, "lock")`. Returns the
/// map with its locks set and every door's lock, in map order; with
/// `locks/random` off, the map as it is and no doors.
pub fn randomize(map: &Map, config: &Config, rng: &mut Rng) -> Result<(Map, Vec<DoorLock>)> {
    if !config.get_bool("locks/random", false) {
        return Ok((map.clone(), Vec::new()));
    }
    let mut map = map.clone();
    let doors = doors(&map)?;
    let pairs = pairs(&map, &doors);

    if config.get_bool("locks/preserve", false) {
        for (room, index) in &doors {
            let door = &mut map.rooms[room.as_str()].doors[*index];
            if door.lock.is_some() {
                door.allowed_locks = Some(Vec::new());
            }
        }
    }

    // A door that can't be unlocked from its side shows no key there, so
    // the player could only open it from the other: the graph would route
    // through it with the key. Its pair takes no lock.
    for (room, index) in &doors {
        if door(&map, &(room.clone(), *index)).no_unlock {
            force_unlocked(&mut map, &(room.clone(), *index), false);
        }
    }

    keep_box_route_clear(&mut map, rng)?;
    keep_route_itemful(&mut map, rng);
    let drawn = randomize_locks(&mut map, config, rng, &pairs);
    drop_stale_keys(&mut map, &doors);
    let locks = door_locks(&map, &doors, &pairs, &drawn);
    Ok((map, locks))
}

/// `GetDoors`: every door with an id, in map order.
fn doors(map: &Map) -> Result<Vec<DoorRef>> {
    let mut seen = HashSet::new();
    let mut doors = Vec::new();
    for room in map.rooms.values() {
        for (index, door) in room.doors.iter().enumerate() {
            let Some(id) = door.id else {
                continue;
            };
            if !seen.insert((room.key.as_str(), id)) {
                bail!("room {} has two doors with id {id}", room.key);
            }
            doors.push((room.key.clone(), index));
        }
    }
    Ok(doors)
}

/// `GetDoorPairs`: each door with the door it leads to, when that door leads
/// back. A door is in one pair at most, the first found in map order.
fn pairs(map: &Map, doors: &[DoorRef]) -> Vec<(DoorRef, DoorRef)> {
    let mut by_identity: IndexMap<String, DoorRef> = doors
        .iter()
        .map(|door| (identity(map, door), door.clone()))
        .collect();
    let mut pairs = Vec::new();
    while let Some((_, a)) = by_identity.shift_remove_index(0) {
        let target = door(map, &a).target.clone().unwrap_or_default();
        if let Some(b) = by_identity.shift_remove(&target) {
            pairs.push((a, b));
        }
    }
    pairs
}

fn identity(map: &Map, (room, index): &DoorRef) -> String {
    let id = map.rooms[room.as_str()].doors[*index]
        .id
        .unwrap_or_default();
    format!("{room}:{id}")
}

fn door<'a>(map: &'a Map, (room, index): &DoorRef) -> &'a crate::map::Door {
    &map.rooms[room.as_str()].doors[*index]
}

fn door_mut<'a>(map: &'a mut Map, (room, index): &DoorRef) -> &'a mut crate::map::Door {
    &mut map.rooms[room.as_str()].doors[*index]
}

/// The far side of a door, as a reference.
fn other_ref(map: &Map, (room, index): &DoorRef) -> Option<DoorRef> {
    let door = &map.rooms.get(room.as_str())?.doors[*index];
    let target_room = door.target_room()?;
    let target_id = door.target_id()?;
    let index = map
        .rooms
        .get(target_room)?
        .doors
        .iter()
        .position(|other| other.id == Some(target_id))?;
    Some((target_room.to_string(), index))
}

/// `AssignDoorLock`: lock a door with `lock` opened by `key`, or with `None`
/// leave it unlocked. A door the key can't open from this side needs nothing
/// to pass in the graph: its far side is the way through.
fn assign(map: &mut Map, door: &DoorRef, lock: Option<(u8, u8)>) {
    let door = door_mut(map, door);
    match lock {
        None => {
            door.lock = None;
            door.key = None;
            door.requires.clear();
        }
        Some((lock, key)) => {
            door.lock = Some(lock);
            door.key = Some(key);
            door.requires = if key == KEY_LOCKED {
                Vec::new()
            } else {
                vec![Requirement::Item(key)]
            };
        }
    }
}

/// Clears a door's lock and keeps it from being locked, both sides.
fn force_unlocked(map: &mut Map, first: &DoorRef, clear_lock: bool) {
    let other = other_ref(map, first);
    for side in std::iter::once(first.clone()).chain(other) {
        door_mut(map, &side).allowed_locks = Some(Vec::new());
        if clear_lock && door(map, &side).lock.is_some() {
            assign(map, &side, None);
        }
    }
}

/// `KeepBoxRouteClear`: of the routes from the start through doors that need
/// nothing, each going on until it can't, one that ends in a room with an
/// item box, and doesn't leave the start's keyless area and come back, has
/// its doors kept unlocked.
fn keep_box_route_clear(map: &mut Map, rng: &mut Rng) -> Result<()> {
    /// Staging enumerates every route; this bounds the work on a bad map.
    const ROUTE_LIMIT: usize = 1_000_000;

    let Some(start) = map.begin_end.first().map(|entry| entry.start.clone()) else {
        return Ok(());
    };
    if !map.rooms.contains_key(&start) {
        return Ok(());
    }
    let together = keyless_area(map, &start);

    let mut queue: VecDeque<(Vec<String>, Vec<DoorRef>)> =
        VecDeque::from([(vec![start.clone()], Vec::new())]);
    let mut finished = Vec::new();
    let mut seen = 0usize;
    while let Some((rooms, doors)) = queue.pop_front() {
        seen += 1;
        if seen > ROUTE_LIMIT {
            bail!("more than {ROUTE_LIMIT} keyless routes from {start}");
        }
        let tail = rooms.last().expect("a route has a room");
        let next: Vec<(String, usize)> = map.rooms[tail.as_str()]
            .doors
            .iter()
            .enumerate()
            .filter(|(_, door)| !door.filter.has_tag(tags::SEGMENT_END) && door.requires.is_empty())
            .filter_map(|(index, door)| {
                let to = door.target_room()?;
                (map.rooms.contains_key(to) && !rooms.iter().any(|room| room == to))
                    .then(|| (to.to_string(), index))
            })
            .collect();
        if next.is_empty() {
            if map.rooms[tail.as_str()].filter.has_tag(tags::BOX) {
                finished.push((rooms, doors));
            }
            continue;
        }
        for (to, index) in next {
            let mut rooms = rooms.clone();
            let mut doors = doors.clone();
            doors.push((tail.clone(), index));
            rooms.push(to);
            queue.push_back((rooms, doors));
        }
    }

    finished.retain(|(rooms, _)| !leaves_and_returns(rooms, &together));
    rng.shuffle(&mut finished);
    let Some((_, doors)) = finished.into_iter().next() else {
        return Ok(());
    };
    for door in &doors {
        force_unlocked(map, door, true);
    }
    Ok(())
}

/// `KeylessRoute.Together`: the rooms reached from `start` through doors that
/// need nothing.
fn keyless_area(map: &Map, start: &str) -> BTreeSet<String> {
    let mut area = BTreeSet::new();
    let mut queue = VecDeque::from([start.to_string()]);
    while let Some(room) = queue.pop_front() {
        for door in &map.rooms[room.as_str()].doors {
            if !door.requires.is_empty() {
                continue;
            }
            let Some(to) = door.target_room().filter(|to| map.rooms.contains_key(*to)) else {
                continue;
            };
            if area.insert(to.to_string()) {
                queue.push_back(to.to_string());
            }
        }
    }
    area
}

/// `ContainsButNotAtStart`: whether a route leaves `area` and comes back.
fn leaves_and_returns(rooms: &[String], area: &BTreeSet<String>) -> bool {
    let mut left = false;
    for room in rooms {
        if area.contains(room) {
            if left {
                return true;
            }
        } else {
            left = true;
        }
    }
    false
}

/// `KeepRouteItemful`: from the start and from the far side of each
/// no-return door, walks the doors that need nothing; whenever the walk runs
/// out before two pickups are in reach, one door it met that could be locked
/// is kept unlocked, both sides, and the walk goes through it.
fn keep_route_itemful(map: &mut Map, rng: &mut Rng) {
    const ITEMS_WANTED: usize = 2;

    let Some(start) = map.begin_end.first().map(|entry| entry.start.clone()) else {
        return;
    };
    if !map.rooms.contains_key(&start) {
        return;
    }
    let mut starts = vec![start];
    for room in map.rooms.values() {
        for door in &room.doors {
            if door.kind != Some(DoorKind::NoReturn) {
                continue;
            }
            if let Some(to) = door.target_room().filter(|to| map.rooms.contains_key(*to)) {
                starts.push(to.to_string());
            }
        }
    }

    for start in starts {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::from([start]);
        let mut items = 0;
        let mut candidates: Vec<DoorRef> = Vec::new();
        while let Some(room_key) = queue.pop_front() {
            if visited.insert(room_key.clone()) {
                let room = &map.rooms[room_key.as_str()];
                items += room
                    .items
                    .iter()
                    .filter(|item| item.requires.is_empty() && !item.optional)
                    .count();
                if items >= ITEMS_WANTED {
                    break;
                }
                for (index, door) in room.doors.iter().enumerate() {
                    if matches!(door.kind, Some(DoorKind::NoReturn | DoorKind::Blocked)) {
                        continue;
                    }
                    if door.allowed_locks.as_ref().is_some_and(Vec::is_empty) {
                        if !door.requires.is_empty() {
                            continue;
                        }
                    } else {
                        candidates.push((room_key.clone(), index));
                        continue;
                    }
                    if let Some(to) = door.target_room().filter(|to| map.rooms.contains_key(*to)) {
                        queue.push_back(to.to_string());
                    }
                }
            }

            while queue.is_empty() && !candidates.is_empty() {
                let pick = candidates.remove(rng.index(candidates.len()));
                let Some(to) = door(map, &pick)
                    .target_room()
                    .filter(|to| map.rooms.contains_key(*to))
                    .map(str::to_string)
                else {
                    continue;
                };
                if visited.contains(&to) {
                    continue;
                }
                force_unlocked(map, &pick, false);
                queue.push_back(to);
            }
        }
    }
}

/// The keys both sides of a pair allow, in the first side's order.
fn allowed(map: &Map, (a, b): &(DoorRef, DoorRef)) -> Vec<u8> {
    let a = door(map, a).allowed_locks.clone().unwrap_or_default();
    let b = door(map, b).allowed_locks.clone().unwrap_or_default();
    let mut both: Vec<u8> = Vec::new();
    for key in a {
        if b.contains(&key) && !both.contains(&key) {
            both.push(key);
        }
    }
    both
}

fn pair_lock(map: &Map, (a, b): &(DoorRef, DoorRef)) -> Option<u8> {
    door(map, a).lock.or(door(map, b).lock)
}

/// `RandomizeLocks`: locks up to `locks/ratio` of the pairs, counting those
/// whose lock the map fixes. Returns the doors of the pairs it decided.
fn randomize_locks(
    map: &mut Map,
    config: &Config,
    rng: &mut Rng,
    pairs: &[(DoorRef, DoorRef)],
) -> HashSet<DoorRef> {
    let ratio = config.get_f64("locks/ratio", 0.0);
    let (restricted, mut free): (Vec<_>, Vec<_>) = pairs
        .iter()
        .cloned()
        .partition(|pair| allowed(map, pair).is_empty());

    let reserved: HashSet<u8> = restricted
        .iter()
        .filter_map(|pair| pair_lock(map, pair))
        .chain(
            map.rooms
                .values()
                .flat_map(|room| room.items.iter().filter_map(|item| item.lock)),
        )
        .collect();
    let mut available: VecDeque<u8> = lock_id_order()
        .filter(|id| !reserved.contains(id))
        .collect();

    let limit = (pairs.len() as f64 * ratio).round_ties_even() as usize;
    let mut locks = restricted
        .iter()
        .filter(|pair| pair_lock(map, pair).is_some())
        .count();
    rng.shuffle(&mut free);
    let priority = |map: &Map, (a, b): &(DoorRef, DoorRef)| {
        door(map, a).filter.has_tag(tags::LOCK_PRIORITY)
            || door(map, b).filter.has_tag(tags::LOCK_PRIORITY)
    };
    free.sort_by_key(|pair| !priority(map, pair));

    let mut drawn = HashSet::new();
    for pair in free {
        let (a, b) = &pair;
        drawn.insert(a.clone());
        drawn.insert(b.clone());
        if locks >= limit {
            assign(map, a, None);
            assign(map, b, None);
            continue;
        }
        let Some(lock) = pair_lock(map, &pair).or_else(|| available.pop_front()) else {
            drawn.remove(a);
            drawn.remove(b);
            continue;
        };
        let keys = allowed(map, &pair);
        let key = *rng.pick(&keys);
        let (mut key_a, mut key_b) = (key, key);
        // Staging's work-around: the far side of a door that unblocks needs
        // no key in the graph.
        if door(map, a).kind == Some(DoorKind::Unblock) {
            key_b = KEY_LOCKED;
        } else if door(map, b).kind == Some(DoorKind::Unblock) {
            key_a = KEY_LOCKED;
        }
        assign(map, a, Some((lock, key_a)));
        assign(map, b, Some((lock, key_b)));
        locks += 1;
    }
    drawn
}

/// A door left with no lock is unlocked in the game, whatever key the room
/// file says it took: its key requirements go. Staging keeps them on the
/// doors it keeps unlocked for a route, so the graph asks for keys the game
/// doesn't.
fn drop_stale_keys(map: &mut Map, doors: &[DoorRef]) {
    let discard: HashSet<u8> = map
        .items
        .iter()
        .filter(|(_, def)| def.discard)
        .map(|(&id, _)| id)
        .collect();
    for door_ref in doors {
        let door = door_mut(map, door_ref);
        if door.lock.is_none() {
            door.requires
                .retain(|r| !matches!(r, Requirement::Item(item) if discard.contains(item)));
        }
    }
}

/// `SetDoorLocks`: every door's lock flag and key, as the game will read them.
fn door_locks(
    map: &Map,
    doors: &[DoorRef],
    pairs: &[(DoorRef, DoorRef)],
    drawn: &HashSet<DoorRef>,
) -> Vec<DoorLock> {
    let partner: std::collections::HashMap<&DoorRef, &DoorRef> =
        pairs.iter().flat_map(|(a, b)| [(a, b), (b, a)]).collect();
    doors
        .iter()
        .map(|door_ref| {
            let (room, _) = door_ref;
            let door = door(map, door_ref);
            let key = if door.no_unlock {
                KEY_NONE
            } else if door.target.is_none() || door.kind == Some(DoorKind::Locked) {
                KEY_LOCKED
            } else if door.kind == Some(DoorKind::Unlock) {
                KEY_UNLOCK
            } else {
                door.key.unwrap_or(KEY_NONE)
            };
            let (lock, key) = match door.lock {
                _ if door.target.is_none() => (LOCK_NOWHERE, KEY_LOCKED),
                Some(lock) if lock != 0 => (lock, key),
                _ => (0, KEY_NONE),
            };
            let target = door
                .target_room()
                .map(|to| to.split('/').next().unwrap_or(to).to_ascii_lowercase());
            DoorLock {
                room: room.clone(),
                dir: map.rooms[room.as_str()].room_id().to_ascii_lowercase(),
                id: door.id.unwrap_or_default(),
                target,
                lock,
                key,
                drawn: drawn.contains(door_ref),
                other: partner.get(door_ref).map(|other| identity(map, other)),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{BeginEnd, Door, Filter, Item, Room};

    fn door(id: u8, target: &str) -> Door {
        Door {
            id: Some(id),
            target: Some(target.into()),
            ..Door::default()
        }
    }

    fn room(key: &str, tags: &[&str], doors: Vec<Door>, items: usize) -> Room {
        Room {
            key: key.into(),
            doors,
            items: (0..items)
                .map(|i| Item {
                    id: i as u8,
                    flag: Some(i as u16),
                    ..Item::default()
                })
                .collect(),
            filter: Filter {
                tags: tags.iter().map(|tag| tag.to_string()).collect(),
                ..Filter::default()
            },
            ..Room::default()
        }
    }

    /// A row of rooms A–F, each door free to take keys 52 and 53, the box
    /// and two pickups in C.
    fn map() -> Map {
        let mut map = Map::default();
        map.begin_end.push(BeginEnd {
            start: "A".into(),
            end: "F".into(),
            filter: Filter::default(),
        });
        let names = ["A", "B", "C", "D", "E", "F"];
        for (i, name) in names.iter().enumerate() {
            let mut doors = Vec::new();
            if i > 0 {
                doors.push(door(0, &format!("{}:1", names[i - 1])));
            }
            if i + 1 < names.len() {
                doors.push(door(1, &format!("{}:0", names[i + 1])));
            }
            for door in &mut doors {
                door.allowed_locks = Some(vec![52, 53]);
            }
            let (tags, items): (&[&str], usize) = if *name == "C" {
                (&[tags::BOX], 2)
            } else {
                (&[], 0)
            };
            map.rooms
                .insert(name.to_string(), room(name, tags, doors, items));
        }
        map
    }

    fn config(ratio: f64) -> Config {
        let mut config = Config::new();
        config.set("locks/random", true);
        config.set("locks/ratio", ratio);
        config
    }

    fn lock<'a>(locks: &'a [DoorLock], room: &str, id: u8) -> &'a DoorLock {
        locks.iter().find(|l| l.room == room && l.id == id).unwrap()
    }

    #[test]
    fn off_without_locks_random() {
        let (out, locks) = randomize(&map(), &Config::new(), &mut Rng::new(1)).unwrap();
        assert_eq!(out, map());
        assert!(locks.is_empty());
    }

    #[test]
    fn the_route_to_the_box_stays_unlocked_and_pairs_share_a_lock() {
        for seed in 0..20 {
            let (out, locks) = randomize(&map(), &config(1.0), &mut Rng::new(seed)).unwrap();
            assert_eq!(locks.len(), 10);
            for (room, id) in [("A", 1), ("B", 0), ("B", 1), ("C", 0)] {
                let l = lock(&locks, room, id);
                assert_eq!((l.lock, l.key), (0, KEY_NONE), "seed {seed} {room}:{id}");
            }
            // Everything past the box is locked, both sides alike, with a new
            // lock from 64 up and a key both sides allow.
            for (a, b) in [
                (("C", 1), ("D", 0)),
                (("D", 1), ("E", 0)),
                (("E", 1), ("F", 0)),
            ] {
                let a = lock(&locks, a.0, a.1);
                let b = lock(&locks, b.0, b.1);
                assert!(a.lock >= 64, "seed {seed}: {a:?}");
                assert_eq!((a.lock, a.key), (b.lock, b.key));
                assert!([52, 53].contains(&a.key));
                assert!(a.drawn && b.drawn);
                assert_eq!(
                    a.other.as_deref(),
                    Some(format!("{}:{}", b.room, b.id).as_str())
                );
                let door = &out.rooms[a.room.as_str()]
                    .doors
                    .iter()
                    .find(|d| d.id == Some(a.id))
                    .unwrap();
                assert_eq!(door.requires, vec![Requirement::Item(a.key)]);
                assert_eq!(door.lock, Some(a.lock));
            }
            let ids: HashSet<u8> = locks.iter().map(|l| l.lock).filter(|&l| l != 0).collect();
            assert_eq!(ids.len(), 3, "one lock per pair");
        }
    }

    #[test]
    fn the_ratio_caps_the_locks() {
        let (_, locks) = randomize(&map(), &config(0.0), &mut Rng::new(3)).unwrap();
        assert!(locks.iter().all(|l| l.lock == 0));
        // 5 pairs at 0.4: two locked, of the three not on the box route.
        let (_, locks) = randomize(&map(), &config(0.4), &mut Rng::new(3)).unwrap();
        let locked = locks.iter().filter(|l| l.lock != 0).count();
        assert_eq!(locked, 4);
    }

    #[test]
    fn kept_locks_keep_their_flag_and_kind() {
        let mut map = map();
        // E–F: locked from E, unlocked from F, outside the randomizer.
        let e = &mut map.rooms["E"].doors[1];
        e.kind = Some(DoorKind::Locked);
        e.lock = Some(21);
        e.allowed_locks = Some(Vec::new());
        let f = &mut map.rooms["F"].doors[0];
        f.kind = Some(DoorKind::Unlock);
        f.lock = Some(21);
        f.allowed_locks = Some(Vec::new());
        // A door to an area left out.
        map.rooms["F"].doors.push(Door {
            id: Some(7),
            target: None,
            ..Door::default()
        });
        let (_, locks) = randomize(&map, &config(1.0), &mut Rng::new(1)).unwrap();
        let e = lock(&locks, "E", 1);
        assert_eq!((e.lock, e.key, e.drawn), (21, KEY_LOCKED, false));
        let f = lock(&locks, "F", 0);
        assert_eq!((f.lock, f.key, f.drawn), (21, KEY_UNLOCK, false));
        let nowhere = lock(&locks, "F", 7);
        assert_eq!(
            (nowhere.lock, nowhere.key, nowhere.target.as_deref()),
            (LOCK_NOWHERE, KEY_LOCKED, None)
        );
        // Lock 21 is not handed out again.
        assert_eq!(locks.iter().filter(|l| l.lock == 21).count(), 2);
    }

    #[test]
    fn a_walk_short_of_items_keeps_a_door_open() {
        let mut map = map();
        // Box route ends at C; with the pickups in E instead, the walk from
        // A must keep the doors from C to E unlocked.
        map.rooms["C"].items.clear();
        map.rooms["E"].items = room("E", &[], vec![], 2).items;
        for seed in 0..10 {
            let (out, _) = randomize(&map, &config(1.0), &mut Rng::new(seed)).unwrap();
            for room in ["C", "D"] {
                let door = &out.rooms[room].doors[1];
                assert_eq!(door.allowed_locks, Some(Vec::new()), "seed {seed} {room}");
                assert_eq!(door.lock, None, "seed {seed} {room}");
            }
        }
    }
}
