//! The seed's game on paper: which rooms, how their doors connect, what each
//! item slot gives and which enemies stand where. Planned from the rooms'
//! `random.xml` alone.
//!
//! The door graph is a tree grown from the start room, so a door's far side
//! leads back to where the player came from and a locked door's far side
//! needs no key. Gates are locks on tree doors plus the door to the goal room;
//! each key goes in a slot the player can reach before that lock.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, anyhow, bail};

use super::ids;
use super::rng::Rng;
use super::rules::{Entrance, RoomRules};

/// A generated game holds this many rooms, the goal room included.
const MIN_ROOMS: usize = 8;
const MAX_ROOMS: usize = 12;
/// How many gates a game has, besides the lock on the door to the goal.
const MIN_GATES: usize = 1;
const MAX_GATES: usize = 2;
/// How many enemy types and how many enemies a room can hold.
const MAX_ENEMY_TYPES: usize = 3;
const MAX_ROOM_ENEMIES: usize = 6;

/// One side of a connected door.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DoorPlan {
    pub target: u16,
    pub entrance: Entrance,
    pub lock: Option<Lock>,
}

/// A door that a key opens once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lock {
    /// The bit in the lock flag bank that records the door as open.
    pub flag: u8,
    pub key: u8,
}

/// What an item slot gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pickup {
    pub item: u8,
    pub quantity: u8,
}

/// One enemy of a room.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spawn {
    pub enemy: u8,
    pub behaviour: u8,
    /// The bit in the enemy flag bank that records the enemy as dead.
    pub flag: u8,
    pub slot: u8,
    pub x: i16,
    pub y: i16,
    pub z: i16,
    pub d: i16,
}

/// What one room of the game becomes. A door the plan does not name is sealed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RoomPlan {
    pub doors: BTreeMap<u8, DoorPlan>,
    pub items: BTreeMap<u8, Pickup>,
    /// `None` keeps the room's own enemies.
    pub spawns: Option<Vec<Spawn>>,
    /// The room's scripts test for the second mansion visit, which a generated
    /// game never makes.
    pub second_visit: bool,
}

/// A planned game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub seed: u32,
    /// `0` Chris, `1` Jill.
    pub character: u8,
    pub start: u16,
    pub start_entrance: Entrance,
    pub rooms: BTreeMap<u16, RoomPlan>,
}

/// A door of the tree, seen from the room nearer the start.
#[derive(Clone, Copy, Debug)]
struct Edge {
    from: (u16, u8),
    to: (u16, u8),
}

/// Plan the game of `seed` from the pool's rules, which name one start and one
/// goal room.
pub fn plan(seed: u32, pool: &[RoomRules]) -> Result<Plan> {
    let mut rng = Rng::new(seed);
    let by_room: BTreeMap<u16, &RoomRules> = pool.iter().map(|rules| (rules.room, rules)).collect();
    let unique = |tag: &str, found: Vec<&RoomRules>| -> Result<u16> {
        match found.as_slice() {
            [rules] => Ok(rules.room),
            _ => bail!("the pool has {} {tag} rooms, not one", found.len()),
        }
    };
    let start = unique("start", pool.iter().filter(|rules| rules.start).collect())?;
    let goal = unique("goal", pool.iter().filter(|rules| rules.goal).collect())?;

    let character = rng.below(2) as u8;
    let (edges, goal_edge) = grow(&mut rng, pool, &by_room, start, goal)?;
    let reached: BTreeSet<u16> = std::iter::once(start)
        .chain(edges.iter().map(|edge| edge.to.0))
        .chain([goal])
        .collect();

    let gates = pick_gates(&mut rng, &edges, goal_edge, start, &by_room)?;
    let mut rooms: BTreeMap<u16, RoomPlan> = reached
        .iter()
        .map(|&room| {
            let plan = RoomPlan {
                second_visit: by_room[&room].second_visit,
                ..RoomPlan::default()
            };
            (room, plan)
        })
        .collect();

    // Connect both sides of each door, with a lock on the near side.
    let mut keys = ids::GATE_KEYS.to_vec();
    rng.shuffle(&mut keys);
    let mut flags = ids::GATE_LOCK_FLAGS.iter().copied();
    let mut locks: BTreeMap<(u16, u8), Lock> = BTreeMap::new();
    for gate in gates.iter().chain([&goal_edge]) {
        let lock = Lock {
            flag: flags.next().ok_or_else(|| anyhow!("too many gates"))?,
            key: keys.pop().ok_or_else(|| anyhow!("too many gates"))?,
        };
        locks.insert(gate.from, lock);
    }
    for edge in edges.iter().chain([&goal_edge]) {
        let entrance = |side: (u16, u8)| -> Result<Entrance> {
            by_room[&side.0]
                .door(side.1)
                .and_then(|door| door.entrance)
                .ok_or_else(|| anyhow!("room {:03x} door {} has no entrance", side.0, side.1))
        };
        let near = DoorPlan {
            target: edge.to.0,
            entrance: entrance(edge.to)?,
            lock: locks.get(&edge.from).copied(),
        };
        let far = DoorPlan {
            target: edge.from.0,
            entrance: entrance(edge.from)?,
            lock: None,
        };
        room_mut(&mut rooms, edge.from.0)?
            .doors
            .insert(edge.from.1, near);
        room_mut(&mut rooms, edge.to.0)?
            .doors
            .insert(edge.to.1, far);
    }

    place_items(
        &mut rng, &edges, goal_edge, &gates, &locks, start, &by_room, &mut rooms,
    )?;
    place_enemies(&mut rng, &by_room, start, &mut rooms)?;

    let start_entrance = by_room[&start]
        .connectable_doors()
        .find_map(|door| door.entrance)
        .ok_or_else(|| anyhow!("the start room has no entrance"))?;
    Ok(Plan {
        seed,
        character,
        start,
        start_entrance,
        rooms,
    })
}

fn room_mut(rooms: &mut BTreeMap<u16, RoomPlan>, room: u16) -> Result<&mut RoomPlan> {
    rooms
        .get_mut(&room)
        .ok_or_else(|| anyhow!("room {room:03x} is not in the plan"))
}

/// Grow the tree from `start`, always keeping a door open for the goal room,
/// and connect the goal to the door deepest in the tree.
fn grow(
    rng: &mut Rng,
    pool: &[RoomRules],
    by_room: &BTreeMap<u16, &RoomRules>,
    start: u16,
    goal: u16,
) -> Result<(Vec<Edge>, Edge)> {
    let mut candidates: Vec<u16> = pool
        .iter()
        .filter(|rules| !rules.start && !rules.goal && rules.connectable_doors().next().is_some())
        .map(|rules| rules.room)
        .collect();
    rng.shuffle(&mut candidates);
    // The start room and the rooms grown from it; the goal makes one more.
    let wanted = rng.range(MIN_ROOMS, MAX_ROOMS) - 2;

    let doors_of = |room: u16| -> Vec<u8> {
        by_room[&room]
            .connectable_doors()
            .map(|door| door.aot)
            .collect()
    };
    let mut open: Vec<(u16, u8, usize)> = doors_of(start)
        .into_iter()
        .map(|door| (start, door, 0))
        .collect();
    let mut edges = Vec::new();
    for _ in 0..wanted {
        if open.is_empty() {
            break;
        }
        // A room with one door would use up the last open door.
        let last_door = open.len() == 1;
        let Some(position) = candidates
            .iter()
            .position(|&room| !last_door || doors_of(room).len() > 1)
        else {
            break;
        };
        let room = candidates.remove(position);
        let (parent, door, depth) = open.swap_remove(rng.below(open.len()));
        let mut doors = doors_of(room);
        let entry = doors.swap_remove(rng.below(doors.len()));
        edges.push(Edge {
            from: (parent, door),
            to: (room, entry),
        });
        open.extend(doors.into_iter().map(|door| (room, door, depth + 1)));
    }
    if edges.len() + 2 < MIN_ROOMS {
        bail!("the pool only makes a game of {} rooms", edges.len() + 2);
    }

    let deepest = open
        .iter()
        .map(|&(_, _, depth)| depth)
        .max()
        .ok_or_else(|| anyhow!("no door is left for the goal room"))?;
    let far: Vec<_> = open.iter().filter(|door| door.2 == deepest).collect();
    let &&(room, door, _) = rng.pick(&far);
    let entry = by_room[&goal]
        .connectable_doors()
        .next()
        .ok_or_else(|| anyhow!("the goal room has no door"))?
        .aot;
    let goal_edge = Edge {
        from: (room, door),
        to: (goal, entry),
    };
    Ok((edges, goal_edge))
}

/// The rooms open without passing the door of a locked edge, and the locks they
/// reach, starting from what is already `open`.
fn flood(
    edges: &[Edge],
    goal_edge: Edge,
    locked: &BTreeSet<(u16, u8)>,
    open: &BTreeSet<(u16, u8)>,
    start: u16,
) -> (BTreeSet<u16>, Vec<(u16, u8)>) {
    let mut rooms = BTreeSet::from([start]);
    let mut frontier = Vec::new();
    loop {
        let mut grew = false;
        for edge in edges.iter().chain([&goal_edge]) {
            if !rooms.contains(&edge.from.0) || rooms.contains(&edge.to.0) {
                continue;
            }
            if locked.contains(&edge.from) && !open.contains(&edge.from) {
                if !frontier.contains(&edge.from) {
                    frontier.push(edge.from);
                }
            } else {
                rooms.insert(edge.to.0);
                grew = true;
            }
        }
        if !grew {
            return (rooms, frontier);
        }
    }
}

/// Whether every lock can get its key from a slot before the lock, and from
/// outside the goal room.
fn keys_fit(
    edges: &[Edge],
    goal_edge: Edge,
    gates: &[Edge],
    start: u16,
    by_room: &BTreeMap<u16, &RoomRules>,
) -> bool {
    let locked: BTreeSet<(u16, u8)> = gates
        .iter()
        .chain([&goal_edge])
        .map(|edge| edge.from)
        .collect();
    let mut open = BTreeSet::new();
    let mut used = 0;
    loop {
        let (rooms, frontier) = flood(edges, goal_edge, &locked, &open, start);
        let Some(&next) = frontier.first() else {
            return open.len() == locked.len();
        };
        let slots: usize = rooms.iter().map(|room| by_room[room].items.len()).sum();
        if used == slots {
            return false;
        }
        used += 1;
        open.insert(next);
    }
}

/// Choose the gates: tree doors whose keys can be placed along with the goal's.
fn pick_gates(
    rng: &mut Rng,
    edges: &[Edge],
    goal_edge: Edge,
    start: u16,
    by_room: &BTreeMap<u16, &RoomRules>,
) -> Result<Vec<Edge>> {
    let wanted = rng.range(MIN_GATES, MAX_GATES);
    let mut shuffled = edges.to_vec();
    rng.shuffle(&mut shuffled);
    let mut gates: Vec<Edge> = Vec::new();
    for edge in shuffled {
        if gates.len() == wanted {
            break;
        }
        gates.push(edge);
        if !keys_fit(edges, goal_edge, &gates, start, by_room) {
            gates.pop();
        }
    }
    if gates.len() < MIN_GATES {
        bail!("no door can be a gate");
    }
    Ok(gates)
}

/// Fill the item slots: each key before its lock, the flare anywhere the player
/// can reach, then the pool's supplies.
#[allow(clippy::too_many_arguments)]
fn place_items(
    rng: &mut Rng,
    edges: &[Edge],
    goal_edge: Edge,
    gates: &[Edge],
    locks: &BTreeMap<(u16, u8), Lock>,
    start: u16,
    by_room: &BTreeMap<u16, &RoomRules>,
    rooms: &mut BTreeMap<u16, RoomPlan>,
) -> Result<()> {
    let locked: BTreeSet<(u16, u8)> = gates
        .iter()
        .chain([&goal_edge])
        .map(|edge| edge.from)
        .collect();
    let mut open = BTreeSet::new();
    let mut placed: BTreeSet<(u16, u8)> = BTreeSet::new();
    let free_slots = |reached: &BTreeSet<u16>, placed: &BTreeSet<(u16, u8)>| -> Vec<(u16, u8)> {
        reached
            .iter()
            .flat_map(|&room| by_room[&room].items.iter().map(move |&slot| (room, slot)))
            .filter(|slot| !placed.contains(slot))
            .collect()
    };
    loop {
        let (reached, frontier) = flood(edges, goal_edge, &locked, &open, start);
        let Some(&next) = frontier.first() else {
            break;
        };
        let slots = free_slots(&reached, &placed);
        if slots.is_empty() {
            bail!("no slot is left for a key");
        }
        let &slot = rng.pick(&slots);
        let key = locks[&next].key;
        room_mut(rooms, slot.0)?.items.insert(
            slot.1,
            Pickup {
                item: key,
                quantity: 1,
            },
        );
        placed.insert(slot);
        open.insert(next);
    }

    let (reached, _) = flood(edges, goal_edge, &locked, &open, start);
    let slots = free_slots(&reached, &placed);
    if slots.is_empty() {
        bail!("no slot is left for the flare");
    }
    let &slot = rng.pick(&slots);
    room_mut(rooms, slot.0)?.items.insert(
        slot.1,
        Pickup {
            item: ids::FLARE,
            quantity: 1,
        },
    );
    placed.insert(slot);

    let mut slots = free_slots(&reached, &placed);
    rng.shuffle(&mut slots);
    let mut supplies = ids::SUPPLIES.iter().cycle();
    for slot in slots {
        // `cycle` on a non-empty list never ends.
        let &(item, quantity) = supplies.next().expect("the supply list is not empty");
        room_mut(rooms, slot.0)?
            .items
            .insert(slot.1, Pickup { item, quantity });
    }
    Ok(())
}

/// Put zero to three enemy types in each room that takes enemies.
fn place_enemies(
    rng: &mut Rng,
    by_room: &BTreeMap<u16, &RoomRules>,
    start: u16,
    rooms: &mut BTreeMap<u16, RoomPlan>,
) -> Result<()> {
    let mut flags = ids::SPAWN_FLAGS;
    for (&room, plan) in rooms.iter_mut() {
        let rules = by_room[&room];
        if rules.enemies.fixed {
            plan.spawns = None;
            continue;
        }
        let mut spawns = Vec::new();
        if room != start {
            let mut pool: Vec<_> = ids::ENEMIES
                .iter()
                .filter(|enemy| rules.enemies.spots_for(enemy.id).next().is_some())
                .collect();
            rng.shuffle(&mut pool);
            let types = rng.below(MAX_ENEMY_TYPES + 1).min(pool.len());
            let chosen = &pool[..types];
            let mut taken = BTreeSet::new();
            let total = if types == 0 {
                0
            } else {
                rng.range(types, MAX_ROOM_ENEMIES)
            };
            for index in 0..total {
                let enemy = chosen[index % types];
                let open: Vec<_> = rules
                    .enemies
                    .spots_for(enemy.id)
                    .filter(|(spot, _)| !taken.contains(spot))
                    .collect();
                if open.is_empty() {
                    continue;
                }
                let &(position, spot) = rng.pick(&open);
                taken.insert(position);
                let flag = flags.next().ok_or_else(|| anyhow!("out of enemy flags"))?;
                spawns.push(Spawn {
                    enemy: enemy.id,
                    behaviour: enemy.behaviour,
                    flag,
                    slot: spawns.len() as u8,
                    x: spot.x,
                    y: spot.y,
                    z: spot.z,
                    d: spot.d,
                });
            }
        }
        plan.spawns = Some(spawns);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::biorand::embedded_pool;

    #[test]
    fn plans_are_a_pure_function_of_the_seed() {
        let pool = embedded_pool();
        assert_eq!(plan(4242, &pool).unwrap(), plan(4242, &pool).unwrap());
        assert_ne!(plan(4242, &pool).unwrap(), plan(4243, &pool).unwrap());
    }

    #[test]
    fn every_seed_plans_a_game_of_the_right_size() {
        let pool = embedded_pool();
        for seed in 0..2000 {
            let plan = plan(seed, &pool).unwrap();
            let rooms = plan.rooms.len();
            assert!(
                (MIN_ROOMS..=MAX_ROOMS).contains(&rooms),
                "seed {seed}: {rooms} rooms"
            );
            assert!(plan.rooms[&plan.start].spawns.as_ref().unwrap().is_empty());
        }
    }

    #[test]
    fn seeds_mix_enemy_types_in_one_room() {
        let pool = embedded_pool();
        let mixed = (0..200).any(|seed| {
            plan(seed, &pool).unwrap().rooms.values().any(|room| {
                let types: BTreeSet<u8> = room
                    .spawns
                    .iter()
                    .flatten()
                    .map(|spawn| spawn.enemy)
                    .collect();
                types.len() > 1
            })
        });
        assert!(mixed);
    }
}
