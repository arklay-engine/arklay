//! The door stage: staging's new `DoorRandomizer` (`DoorRandomizer.cs`), not
//! the legacy `DoorRandomiser`.
//!
//! Under `doors/random`, [`randomize`] takes the map [`crate::prepare`] made
//! for door randomization and rewires it:
//!
//! 1. Rooms joined by doors that are never randomized (`randomize="false"`)
//!    become one *piece* (`GetPieces`), and every other door loses its target.
//! 2. The game is cut into `doors/segments` segments. The first starts at the
//!    begin room and the last ends at the end room; between them, a shuffled
//!    choice of the pieces with a `segment-end` door each ends one segment and
//!    starts the next (a *bridge* room). Its `segment-end` door becomes a
//!    no-return door into the next segment.
//! 3. Pieces that depend on each other (a door requiring another room or a
//!    flag it raises) always go together. Box rooms, then the pieces with the
//!    most doors, then a share of the rest (`doors/rooms`) are dealt to the
//!    segments.
//! 4. Each segment is wired from its start: doors are connected one by one to
//!    unused pieces, keeping a free door open, then to the end piece through
//!    the deepest open door, then to the rest; spare doors are paired back
//!    to each other behind one-sided locks, and the rest are sealed.
//! 5. A door one side of which unblocks or must connect back gets a one-sided
//!    lock, so the route can't enter it the wrong way.
//!
//! The returned map has the new targets, kinds and locks, and only the rooms
//! the game uses; the [`DoorPlan`] says where each room and door ends up.
//! Segment *n* becomes stage *n*, and its rooms are numbered from 0 in the
//! order the randomizer reached them.
//!
//! Where arklay differs from staging, and why:
//!
//! - New lock ids are allocated from 64 up ([`crate::locks::lock_id_order`]),
//!   as the lock stage allocates them, and never one an item lock uses: the
//!   original room scripts raise lock flags below 64 by number.
//! - When a door that already has a lock is connected, its far side takes
//!   the lock *and the mirror of its kind*: the far side of a door that
//!   unlocks from its own side is locked, and the reverse. Staging clears the
//!   far side's kind, which leaves it open in the route while the game keeps
//!   it locked with no key.
//! - A step that staging would crash on (no door free to reach the end room,
//!   a segment without its start) is an error, so the seed fails cleanly.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

use anyhow::{Result, anyhow, bail};
use indexmap::IndexMap;

use crate::config::Config;
use crate::map::{DoorKind, Entrance, Map, Requirement, tags};
use crate::rng::{EndlessBag, Rng};

/// Where a door of the game leads, and the rooms' new ids.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DoorPlan {
    /// Each room directory the game uses, by its original name in lowercase
    /// (`10c`), in the order the rooms are numbered.
    pub rooms: IndexMap<String, PlannedRoom>,
    /// Every door with an AOT id in the game's rooms, in map order.
    pub doors: Vec<PlannedDoor>,
    /// How many segments, and so stages, the game has.
    pub segments: u8,
    /// The directory, lowercase, of the room a new game starts in (`106`).
    pub start: String,
    /// The directory, lowercase, of the room the game ends in (`501`).
    pub end: String,
}

impl DoorPlan {
    /// The new id of the room directory `dir` (lowercase), if the game uses
    /// it.
    pub fn new_id(&self, dir: &str) -> Option<&str> {
        self.rooms.get(dir).map(|room| room.id.as_str())
    }
}

/// A room directory's place in the generated game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedRoom {
    /// The new stage: the segment the randomizer first used the room in.
    pub stage: u8,
    /// The room's number in its stage.
    pub room: u8,
    /// The room folder's name: `10a`, or `01-20` for an id the short form
    /// can't spell ([`room_id`]).
    pub id: String,
}

/// One door of a room in the game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedDoor {
    /// The logical room the door is in (`10C/BACK`).
    pub room: String,
    /// The room's directory in lowercase (`10c`).
    pub dir: String,
    /// The AOT id of its `door_aot_set`.
    pub id: u8,
    /// The room directory, lowercase, the original door leads to: what the
    /// room script's target byte names, by which an emitter finds the door.
    /// `None` for a door the original leads nowhere.
    pub script_target: Option<String>,
    /// Whether the door randomizer could rewire it (`randomize` is not false).
    pub randomized: bool,
    /// Where it leads now; `None` for a door sealed in this game.
    pub target: Option<PlannedTarget>,
    /// A segment's end: once through, the player can't come back, and the
    /// far door counts as blocked.
    pub no_return: bool,
}

/// The far side of a door.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedTarget {
    /// The logical room it leads to (`10C/BACK`).
    pub room: String,
    /// That room's directory, lowercase, as the original names it.
    pub dir: String,
    /// The new id of that directory ([`PlannedRoom::id`]).
    pub id: String,
    /// The door it arrives through.
    pub door: u8,
    /// Where the player arrives and faces: the far door's entrance.
    pub entrance: Entrance,
}

/// The folder name of room `room` of stage `stage`: the short form (`10a`)
/// for a stage of 1–7 and a room up to 0x1F, else the long one (`01-20`), as
/// the engine's `RoomId` spells it.
pub fn room_id(stage: u8, room: u8) -> String {
    if (1..=7).contains(&stage) && room <= 0x1F {
        format!("{stage:x}{room:02x}")
    } else {
        format!("{stage:02x}-{room:02x}")
    }
}

/// Room 303's death replay draws room 311 relative to the origin, so 311
/// keeps its folder and no other room takes its id.
pub const RESERVED_IDS: [&str; 1] = ["311"];

/// The door stage. `rng` should be `Rng::named(seed, "sourceDoor")`. Returns
/// the map rewired, with the rooms the game leaves out removed, and the plan
/// of rooms and doors.
pub fn randomize(map: &Map, config: &Config, rng: &mut Rng) -> Result<(Map, DoorPlan)> {
    let original = map.clone();
    let mut state = State::new(map.clone(), rng);
    let segments = state.run(config)?;
    let plan = state.plan(&original, &segments)?;
    check_reachable(&state.map)?;
    Ok((state.map, plan))
}

/// Whether every room of the rewired map can be walked into from the begin
/// room, keys aside: through doors that open from the side walked from.
/// Staging doesn't check: its key stage counts a room nothing leads into as
/// visited, and ships the layout.
fn check_reachable(map: &Map) -> Result<()> {
    let begin = map
        .rooms
        .values()
        .find(|room| room.filter.has_tag(tags::BEGIN))
        .ok_or_else(|| anyhow!("no begin room"))?;
    let mut reached = HashSet::from([begin.key.as_str()]);
    let mut queue = VecDeque::from([begin.key.as_str()]);
    while let Some(key) = queue.pop_front() {
        for door in &map.rooms[key].doors {
            if door.ignore_in_graph
                || matches!(door.kind, Some(DoorKind::Blocked | DoorKind::Locked))
            {
                continue;
            }
            if let Some(to) = door.target_room()
                && map.rooms.contains_key(to)
                && reached.insert(to)
            {
                queue.push_back(to);
            }
        }
    }
    let unreached: Vec<&str> = map
        .rooms
        .keys()
        .map(String::as_str)
        .filter(|key| !reached.contains(key))
        .collect();
    if unreached.is_empty() {
        Ok(())
    } else {
        bail!("nothing leads into {unreached:?}")
    }
}

/// A door: its room's key and its index in the room's doors.
type DoorRef = (String, usize);

/// Usually one room, but sometimes rooms always joined together (staging's
/// `RoomPiece`).
#[derive(Clone, Debug)]
struct Piece {
    /// Its logical rooms' keys, in the order the piece was gathered.
    rooms: Vec<String>,
    /// Its randomizable doors, as indices into [`State::doors`].
    doors: Vec<usize>,
    parent: Option<usize>,
}

/// A randomizable door of a piece (staging's `RoomPieceDoor`).
#[derive(Clone, Debug)]
struct PieceDoor {
    owner: usize,
    at: DoorRef,
    target: Option<usize>,
    sealed: bool,
}

#[derive(Clone, Debug, Default)]
struct Segment {
    number: u8,
    start: Option<usize>,
    end: Option<usize>,
    used: Vec<usize>,
    unused: Vec<usize>,
    available_locks: BTreeSet<u8>,
    has_items: bool,
    has_box: bool,
}

struct State<'r> {
    map: Map,
    rng: &'r mut Rng,
    pieces: Vec<Piece>,
    doors: Vec<PieceDoor>,
    /// The pieces not yet dealt to a segment (`_allRooms`).
    all: Vec<usize>,
    /// The pieces dealt (`_includedRooms`).
    included: Vec<usize>,
    groups: Groups,
    lock_ids: VecDeque<u8>,
}

impl<'r> State<'r> {
    fn new(map: Map, rng: &'r mut Rng) -> Self {
        let reserved: HashSet<u8> = map
            .rooms
            .values()
            .flat_map(|room| {
                let doors = room.doors.iter().filter_map(|door| door.lock);
                doors.chain(room.items.iter().filter_map(|item| item.lock))
            })
            .collect();
        let lock_ids = crate::locks::lock_id_order()
            .filter(|id| !reserved.contains(id))
            .collect();
        Self {
            map,
            rng,
            pieces: Vec::new(),
            doors: Vec::new(),
            all: Vec::new(),
            included: Vec::new(),
            groups: Groups::default(),
            lock_ids,
        }
    }

    fn door(&self, door: usize) -> &crate::map::Door {
        let (room, index) = &self.doors[door].at;
        &self.map.rooms[room.as_str()].doors[*index]
    }

    fn door_mut(&mut self, door: usize) -> &mut crate::map::Door {
        let (room, index) = &self.doors[door].at;
        &mut self.map.rooms[room.as_str()].doors[*index]
    }

    /// The whole of `DoorRandomizer.Randomize` up to the map update. Returns
    /// the segments, in order.
    fn run(&mut self, config: &Config) -> Result<Vec<Segment>> {
        self.gather_pieces();
        self.all = (0..self.pieces.len()).collect();

        let ratio = config.get_f64("doors/rooms", 0.5);
        let total = self.all.len();
        let wanted = ((total as f64 * ratio).round_ties_even().max(0.0) as usize).min(total);
        let count = usize::try_from(config.get_int("doors/segments", 2).max(1)).unwrap_or(1);

        let enders: Vec<usize> = self
            .all
            .iter()
            .copied()
            .filter(|&piece| self.is_segment_end(piece))
            .collect();
        let mut enders = self.rng.shuffled(&enders);
        if count == 1 {
            for &piece in &enders {
                for door in self.pieces[piece].doors.clone() {
                    self.door_mut(door)
                        .filter
                        .tags
                        .retain(|tag| tag != tags::SEGMENT_END);
                }
            }
            enders.clear();
        }
        if enders.len() < count - 1 {
            bail!(
                "{count} segments need {} segment ends, and the map has {}",
                count - 1,
                enders.len()
            );
        }
        let mut used_enders: VecDeque<usize> = enders[..count - 1].iter().copied().collect();
        let unused_enders = enders[count - 1..].to_vec();
        let mut left_out: Vec<usize> = unused_enders.clone();
        self.all.retain(|piece| !unused_enders.contains(piece));

        for &piece in &self.all.clone() {
            let coupled: Vec<usize> = self
                .all
                .iter()
                .copied()
                .filter(|&other| self.is_coupled(piece, other))
                .collect();
            self.groups.add(&coupled);
        }

        let mut segments = self.create_segments(count)?;
        for index in 0..segments.len() {
            if segments[index].end.is_none() {
                let end = used_enders
                    .pop_front()
                    .ok_or_else(|| anyhow!("no segment end is left"))?;
                let taken = self.take(end);
                segments[index]
                    .unused
                    .extend(taken.into_iter().filter(|&piece| piece != end));
                segments[index].end = Some(end);
                if let Some(next) = segments.get_mut(index + 1) {
                    next.start = Some(end);
                }
            }
        }

        // Some box rooms, some hubs, then the rest of the rooms wanted.
        let boxes: Vec<usize> = self
            .all
            .iter()
            .copied()
            .filter(|&piece| self.is_box(piece))
            .collect();
        self.distribute(&mut segments, &boxes);
        let mut hubs = self.rng.shuffled(&self.all);
        hubs.sort_by_key(|&piece| std::cmp::Reverse(self.pieces[piece].doors.len()));
        hubs.truncate(count * 4);
        self.distribute(&mut segments, &hubs);
        let remaining = wanted.saturating_sub(self.included.len());
        let rest: Vec<usize> = self
            .all
            .iter()
            .copied()
            .filter(|&piece| !self.is_segment_end(piece))
            .collect();
        let mut rest = self.rng.shuffled(&rest);
        rest.truncate(remaining);
        self.distribute(&mut segments, &rest);

        for index in 0..segments.len() {
            self.randomize_segment(&mut segments[index])?;
            let unused = std::mem::take(&mut segments[index].unused);
            match segments.get_mut(index + 1) {
                Some(next) => next.unused.extend(unused),
                None => self.all.extend(unused),
            }
        }

        left_out.extend(self.all.iter().copied());
        for piece in left_out {
            for room in self.pieces[piece].rooms.clone() {
                self.map.rooms.shift_remove(&room);
            }
        }

        if config.get_bool("locks/random", false) {
            self.distribute_locks(&mut segments);
        }
        self.update_targets(&segments)?;
        Ok(segments)
    }

    /// `GetPieces`: the rooms joined by doors that are never randomized; every
    /// other door loses its target.
    fn gather_pieces(&mut self) {
        let mut visited: HashSet<String> = HashSet::new();
        let keys: Vec<String> = self.map.rooms.keys().cloned().collect();
        for key in keys {
            if visited.contains(&key) {
                continue;
            }
            let mut group: Vec<String> = Vec::new();
            let mut queue = VecDeque::from([key]);
            while let Some(current) = queue.pop_front() {
                if group.contains(&current) {
                    continue;
                }
                group.push(current.clone());
                visited.insert(current.clone());
                let room = &mut self.map.rooms[current.as_str()];
                let mut next = Vec::new();
                for door in &mut room.doors {
                    if door.randomize == Some(false) {
                        if let Some(target) = door.target_room() {
                            next.push(target.to_string());
                        }
                    } else {
                        door.target = None;
                    }
                }
                for target in next {
                    if self.map.rooms.contains_key(&target) {
                        queue.push_back(target);
                    }
                }
            }
            let piece = self.pieces.len();
            let mut doors = Vec::new();
            for room in &group {
                for (index, door) in self.map.rooms[room.as_str()].doors.iter().enumerate() {
                    if door.randomize != Some(false) {
                        doors.push(self.doors.len());
                        self.doors.push(PieceDoor {
                            owner: piece,
                            at: (room.clone(), index),
                            target: None,
                            sealed: false,
                        });
                    }
                }
            }
            self.pieces.push(Piece {
                rooms: group,
                doors,
                parent: None,
            });
        }
    }

    fn piece_has_room_tag(&self, piece: usize, tag: &str) -> bool {
        self.pieces[piece]
            .rooms
            .iter()
            .any(|room| self.map.rooms[room.as_str()].filter.has_tag(tag))
    }

    fn is_segment_end(&self, piece: usize) -> bool {
        self.pieces[piece]
            .doors
            .iter()
            .any(|&door| self.is_segment_end_door(door))
    }

    fn is_box(&self, piece: usize) -> bool {
        self.piece_has_room_tag(piece, tags::BOX)
    }

    /// `IsCoupled`: whether a requirement of `piece` names a room of `other`
    /// or a flag it raises.
    fn is_coupled(&self, piece: usize, other: usize) -> bool {
        if piece == other {
            return true;
        }
        let requirements: BTreeSet<&Requirement> = self.pieces[piece]
            .rooms
            .iter()
            .flat_map(|room| {
                let room = &self.map.rooms[room.as_str()];
                let doors = room.doors.iter().flat_map(|door| &door.requires);
                let flags = room.flags.iter().flat_map(|flag| &flag.requires);
                doors
                    .chain(flags)
                    .chain(room.items.iter().flat_map(|item| &item.requires))
            })
            .collect();
        let others = &self.pieces[other].rooms;
        requirements.iter().any(|requirement| match requirement {
            Requirement::Room(key) => others.iter().any(|room| room == key),
            Requirement::Flag(name) => others.iter().any(|room| {
                self.map.rooms[room.as_str()]
                    .flags
                    .iter()
                    .any(|flag| &flag.name == name)
            }),
            Requirement::Item(_) => false,
        })
    }

    /// `TakeRoom`: `piece` and the pieces it goes with, out of the pool;
    /// empty if any of them is already taken.
    fn take(&mut self, piece: usize) -> Vec<usize> {
        let group = self.groups.get(piece);
        if group.iter().any(|piece| !self.all.contains(piece)) {
            return Vec::new();
        }
        self.all.retain(|piece| !group.contains(piece));
        self.included.extend(group.iter().copied());
        group
    }

    fn create_segments(&mut self, count: usize) -> Result<Vec<Segment>> {
        let mut segments = Vec::new();
        for i in 0..count {
            let mut segment = Segment {
                number: u8::try_from(i + 1)?,
                ..Segment::default()
            };
            if i == 0 {
                let start = self
                    .all
                    .iter()
                    .copied()
                    .find(|&piece| self.piece_has_room_tag(piece, tags::BEGIN))
                    .ok_or_else(|| anyhow!("no room is tagged begin"))?;
                let taken = self.take(start);
                segment
                    .unused
                    .extend(taken.into_iter().filter(|&piece| piece != start));
                segment.start = Some(start);
            }
            if i == count - 1 {
                let end = self
                    .all
                    .iter()
                    .copied()
                    .find(|&piece| self.piece_has_room_tag(piece, tags::END))
                    .ok_or_else(|| anyhow!("no room is tagged end"))?;
                let taken = self.take(end);
                segment
                    .unused
                    .extend(taken.into_iter().filter(|&piece| piece != end));
                segment.end = Some(end);
            }
            segments.push(segment);
        }
        Ok(segments)
    }

    /// `Distribute`: each piece (and the pieces it goes with) to a segment
    /// drawn from an endless bag.
    fn distribute(&mut self, segments: &mut [Segment], pieces: &[usize]) {
        let mut bag = EndlessBag::new((0..segments.len()).collect::<Vec<_>>());
        let shuffled = self.rng.shuffled(pieces);
        for piece in shuffled {
            let group = self.take(piece);
            if group.is_empty() {
                continue;
            }
            let segment = bag.next(self.rng).expect("a segment");
            segments[segment].unused.extend(group);
        }
    }

    fn use_piece(&self, segment: &mut Segment, piece: usize) {
        segment.unused.retain(|&p| p != piece);
        if !segment.used.contains(&piece) {
            segment.used.push(piece);
        }
        if self.is_box(piece) {
            segment.has_box = true;
        }
        if self.pieces[piece].rooms.iter().any(|room| {
            self.map.rooms[room.as_str()]
                .items
                .iter()
                .any(|item| item.requires.is_empty())
        }) {
            segment.has_items = true;
        }
    }

    // Door properties (`RoomPieceDoor`).

    fn is_segment_end_door(&self, door: usize) -> bool {
        self.door(door).filter.has_tag(tags::SEGMENT_END)
    }

    fn is_free(&self, door: usize) -> bool {
        self.door(door).requires.is_empty()
    }

    fn must_connect_out(&self, door: usize) -> bool {
        let d = self.door(door);
        d.kind == Some(DoorKind::Unblock)
            || d.filter.has_tag(tags::CONNECT_OUT)
            || d.filter.has_tag(tags::CONNECT_BACK)
            || !self.is_free(door)
    }

    fn can_be_locked(&self, door: usize) -> bool {
        !self.door(door).no_unlock
    }

    fn is_connectable(&self, door: usize) -> bool {
        let d = self.door(door);
        !self.doors[door].sealed && d.randomize != Some(false) && d.kind != Some(DoorKind::Blocked)
    }

    fn seal(&mut self, door: usize) -> Result<()> {
        if self.doors[door].sealed {
            bail!("door {:?} is already sealed", self.doors[door].at);
        }
        self.doors[door].sealed = true;
        Ok(())
    }

    fn next_lock_id(&mut self) -> Result<u8> {
        self.lock_ids
            .pop_front()
            .ok_or_else(|| anyhow!("no lock id is left for a door"))
    }

    /// `RoomPieceDoor.Connect`.
    fn connect(&mut self, from: usize, to: usize, lock: Option<u8>) -> Result<()> {
        if self.doors[from].target.is_some() || self.doors[to].target.is_some() {
            bail!("a door to connect is already connected");
        }
        self.doors[from].target = Some(to);
        self.doors[to].target = Some(from);
        self.seal(from)?;
        self.seal(to)?;
        let owner = self.doors[from].owner;
        let target_owner = self.doors[to].owner;
        if self.pieces[target_owner].parent.is_none() {
            self.pieces[target_owner].parent = Some(owner);
        }

        let source = self.door(from).clone();
        if let Some(existing) = source.lock {
            // The far side shares the lock, from its side the mirror of this
            // one's (see the module notes).
            let kind = match source.kind {
                Some(DoorKind::Unlock) => Some(DoorKind::Locked),
                Some(DoorKind::Locked) => Some(DoorKind::Unlock),
                _ => None,
            };
            let target = self.door_mut(to);
            target.kind = kind;
            target.requires = if kind == Some(DoorKind::Locked) {
                Vec::new()
            } else {
                source.requires.clone()
            };
            target.lock = Some(existing);
            target.key = source.key;
            target.allowed_locks = source.allowed_locks.clone();
        } else if let Some(lock) = lock {
            let door = self.door_mut(from);
            door.kind = Some(DoorKind::Unlock);
            door.lock = Some(lock);
            door.allowed_locks = Some(Vec::new());
            let target = self.door_mut(to);
            target.kind = Some(DoorKind::Locked);
            target.lock = Some(lock);
            target.allowed_locks = Some(Vec::new());
        }
        Ok(())
    }

    // Segment views (`Segment`).

    fn segment_doors(&self, segment: &Segment) -> Vec<usize> {
        segment
            .used
            .iter()
            .flat_map(|&piece| self.pieces[piece].doors.iter().copied())
            .collect()
    }

    fn unconnected_doors(&self, segment: &Segment) -> Vec<usize> {
        self.segment_doors(segment)
            .into_iter()
            .filter(|&door| {
                if !self.is_connectable(door) {
                    return false;
                }
                if self.is_segment_end_door(door) {
                    return Some(self.doors[door].owner) == segment.start;
                }
                self.requirements_met(segment, &self.door(door).requires, 0)
            })
            .collect()
    }

    fn available_doors(&self, segment: &Segment) -> Vec<usize> {
        segment
            .unused
            .iter()
            .flat_map(|&piece| self.pieces[piece].doors.iter().copied())
            .filter(|&door| self.is_connectable(door) && !self.must_connect_out(door))
            .collect()
    }

    fn used_rooms<'a>(&'a self, segment: &'a Segment) -> impl Iterator<Item = &'a String> + 'a {
        segment
            .used
            .iter()
            .flat_map(|&piece| self.pieces[piece].rooms.iter())
    }

    /// `AreRequirementsMet`: the room and flag requirements, met by the rooms
    /// the segment uses. Items don't count.
    fn requirements_met(&self, segment: &Segment, requires: &[Requirement], depth: u32) -> bool {
        if depth > 16 {
            return false;
        }
        requires.iter().all(|requirement| match requirement {
            Requirement::Room(key) => self.used_rooms(segment).any(|room| room == key),
            Requirement::Flag(name) => self.used_rooms(segment).any(|room| {
                self.map.rooms[room.as_str()].flags.iter().any(|flag| {
                    &flag.name == name && self.requirements_met(segment, &flag.requires, depth + 1)
                })
            }),
            Requirement::Item(_) => true,
        })
    }

    /// The other doors of `door`'s piece (`AvailableDoor.OtherDoors`), not
    /// sealed nor connecting back (`OtherAvailableDoors`).
    fn other_available(&self, door: usize) -> Vec<usize> {
        self.pieces[self.doors[door].owner]
            .doors
            .iter()
            .copied()
            .filter(|&other| {
                other != door
                    && !self.doors[other].sealed
                    && !self.door(other).filter.has_tag(tags::CONNECT_BACK)
            })
            .collect()
    }

    /// `RandomizeSegment`.
    fn randomize_segment(&mut self, segment: &mut Segment) -> Result<()> {
        let (Some(start), Some(end)) = (segment.start, segment.end) else {
            bail!("segment {} has no start or end room", segment.number);
        };
        self.use_piece(segment, start);
        while self.connect_another(segment, true)? {}

        let to_end = self
            .pick_door_to_end(segment)
            .ok_or_else(|| anyhow!("segment {} can't reach its end room", segment.number))?;
        let candidates: Vec<usize> = self.pieces[end]
            .doors
            .iter()
            .copied()
            .filter(|&door| {
                self.is_connectable(door) && !self.is_segment_end_door(door) && self.is_free(door)
            })
            .collect();
        let candidates = self.rng.shuffled(&candidates);
        let target = *candidates.first().ok_or_else(|| {
            anyhow!(
                "segment {}'s end room has no free door to arrive by",
                segment.number
            )
        })?;
        self.door_mut(to_end)
            .filter
            .tags
            .push(tags::LOCK_PRIORITY.to_string());
        self.connect(to_end, target, None)?;
        self.use_piece(segment, end);

        // So that the end piece's doors can be connected up too.
        while self.connect_another(segment, false)? {}

        // Connect spare doors back to each other.
        let spare = self.unconnected_doors(segment);
        for door in self.rng.shuffled(&spare) {
            if !self.is_connectable(door) || !self.can_be_locked(door) {
                continue;
            }
            let owner = self.doors[door].owner;
            let potential = self.unconnected_doors(segment).into_iter().find(|&other| {
                other != door
                    && self.is_free(other)
                    && !self.must_connect_out(other)
                    && self.can_be_locked(other)
                    && !self.is_connected_to(self.doors[other].owner, owner)
            });
            if let Some(other) = potential {
                let lock = match self.door(door).lock {
                    Some(lock) => lock,
                    None => self.next_lock_id()?,
                };
                self.connect(door, other, Some(lock))?;
            }
        }

        // Seal the rest.
        for door in self.segment_doors(segment) {
            if !self.doors[door].sealed && !self.is_segment_end_door(door) {
                self.seal(door)?;
            }
        }
        Ok(())
    }

    /// `RoomPiece.IsConnectedTo`.
    fn is_connected_to(&self, piece: usize, other: usize) -> bool {
        piece == other
            || self.pieces[piece]
                .doors
                .iter()
                .any(|&door| self.doors[door].target.map(|t| self.doors[t].owner) == Some(other))
    }

    /// `ConnectAnotherDoor`: connects one open door of the segment to a door
    /// of an unused piece. Returns whether it did.
    fn connect_another(&mut self, segment: &mut Segment, ensure_free: bool) -> Result<bool> {
        let unconnected = self.unconnected_doors(segment);
        let unconnected = self.rng.shuffled(&unconnected);
        let available = self.available_doors(segment);
        let mut available = self.rng.shuffled(&available);
        if !ensure_free {
            available.sort_by_key(|&door| std::cmp::Reverse(self.other_available(door).len()));
        }

        for &source in &unconnected {
            if (!segment.has_box || !segment.has_items)
                && !self.is_segment_end_door(source)
                && !self.is_free(source)
            {
                continue;
            }
            for &target in &available {
                let mut should_lock = false;
                if !self.is_segment_end_door(source)
                    && self.door(source).kind == Some(DoorKind::Unblock)
                    && self.can_be_locked(source)
                {
                    if self.can_be_locked(target) {
                        should_lock = true;
                    } else {
                        // The target must lock, so the unblocking door
                        // can't be entered the wrong way.
                        continue;
                    }
                }

                // Rooms already connected (the key stage would complain).
                let target_room = self.doors[target].at.0.clone();
                let source_room = &self.doors[source].at.0;
                if self.map.rooms[source_room.as_str()]
                    .doors
                    .iter()
                    .any(|door| door.target_room() == Some(target_room.as_str()))
                {
                    continue;
                }

                let target_owner = self.doors[target].owner;
                if ensure_free {
                    let mut remaining: isize = 0;
                    if !segment.has_box || !segment.has_items {
                        // Simple pieces only.
                        if self.pieces[target_owner].rooms.len() != 1 {
                            continue;
                        }
                        remaining += unconnected
                            .iter()
                            .filter(|&&door| door != source && self.is_free(door))
                            .count() as isize;
                        remaining += self
                            .other_available(target)
                            .into_iter()
                            .filter(|&door| self.is_free(door))
                            .count() as isize;
                    } else {
                        remaining +=
                            unconnected.iter().filter(|&&door| door != source).count() as isize;
                        remaining += self.other_available(target).len() as isize;
                    }
                    let connect_back = self.pieces[target_owner]
                        .doors
                        .iter()
                        .filter(|&&door| self.door(door).filter.has_tag(tags::CONNECT_BACK))
                        .count() as isize;
                    remaining -= connect_back;
                    if remaining <= 0 {
                        continue;
                    }
                }

                if !self.connect_back(segment, source, target_owner)? {
                    continue;
                }

                let lock = if should_lock {
                    Some(self.next_lock_id()?)
                } else {
                    None
                };
                self.connect(source, target, lock)?;
                self.use_piece(segment, target_owner);
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// `ConnectBackDoors`: the `connect-back` doors of `piece` connect, each
    /// behind a one-sided lock, to doors on the way from the segment's start
    /// to `source`. Returns false when there aren't enough.
    fn connect_back(&mut self, segment: &Segment, source: usize, piece: usize) -> Result<bool> {
        let back: Vec<usize> = self.pieces[piece]
            .doors
            .iter()
            .copied()
            .filter(|&door| self.door(door).filter.has_tag(tags::CONNECT_BACK))
            .collect();
        if back.is_empty() {
            return Ok(true);
        }
        let Some(head) = segment.start else {
            return Ok(false);
        };
        let suitable = self.connect_back_doors(head, source);
        let suitable = self.rng.shuffled(&suitable);
        if suitable.len() < back.len() {
            return Ok(false);
        }
        for (door, other) in back.into_iter().zip(suitable) {
            let lock = self.next_lock_id()?;
            self.connect(door, other, Some(lock))?;
        }
        Ok(true)
    }

    /// `FindConnectBackDoors`: the open doors of the pieces from `tail`'s up
    /// to `head`, by parent.
    fn connect_back_doors(&self, head: usize, tail: usize) -> Vec<usize> {
        let mut result = Vec::new();
        let mut current = Some(self.doors[tail].owner);
        let mut seen = HashSet::new();
        while let Some(piece) = current {
            if !seen.insert(piece) {
                break;
            }
            for &door in &self.pieces[piece].doors {
                if self.is_connectable(door)
                    && !self.must_connect_out(door)
                    && !self.is_segment_end_door(door)
                    && door != tail
                {
                    result.push(door);
                }
            }
            if piece == head {
                break;
            }
            current = self.pieces[piece].parent;
        }
        result
    }

    /// `PickUnconnectedDoorForSegmentEnd`: an open door as deep from the start
    /// as any.
    fn pick_door_to_end(&mut self, segment: &Segment) -> Option<usize> {
        let unconnected = self.unconnected_doors(segment);
        if unconnected.is_empty() {
            return None;
        }
        let mut depths = vec![0usize; unconnected.len()];
        let mut visited = HashSet::new();
        let mut queue = VecDeque::from([(segment.start?, 0usize)]);
        while let Some((piece, depth)) = queue.pop_front() {
            if !visited.insert(piece) {
                continue;
            }
            for &door in &self.pieces[piece].doors {
                match self.doors[door].target {
                    None => {
                        if let Some(index) = unconnected.iter().position(|&d| d == door) {
                            depths[index] = depth;
                        }
                    }
                    Some(target) => queue.push_back((self.doors[target].owner, depth + 1)),
                }
            }
        }
        let best = *depths.iter().max()?;
        let potential: Vec<usize> = unconnected
            .iter()
            .zip(&depths)
            .filter(|(_, depth)| **depth == best)
            .map(|(door, _)| *door)
            .collect();
        Some(*self.rng.pick(&potential))
    }

    /// `DistributeLocks`: a discard key of its own for each segment, the rest
    /// shared by one to three segments; each door of a segment may then take
    /// only its segment's keys.
    fn distribute_locks(&mut self, segments: &mut [Segment]) {
        let discard: Vec<u8> = self
            .map
            .items
            .iter()
            .filter(|(_, def)| def.discard)
            .map(|(&id, _)| id)
            .collect();
        let mut keys: VecDeque<u8> = self.rng.shuffled(&discard).into();
        for segment in segments.iter_mut() {
            if let Some(key) = keys.pop_front() {
                segment.available_locks.insert(key);
            }
        }
        let mut bag = EndlessBag::new((0..segments.len()).collect::<Vec<_>>());
        while let Some(key) = keys.pop_front() {
            let count = self.rng.next_in(1, (segments.len() as i32 + 1).min(4));
            for _ in 0..count {
                let segment = bag.next(self.rng).expect("a segment");
                segments[segment].available_locks.insert(key);
            }
        }
        for segment in segments.iter() {
            let rooms: Vec<String> = self.used_rooms(segment).cloned().collect();
            for room in rooms {
                let Some(room) = self.map.rooms.get_mut(&room) else {
                    continue;
                };
                for door in &mut room.doors {
                    let mut allowed: Vec<u8> = door
                        .allowed_locks
                        .clone()
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|key| segment.available_locks.contains(key))
                        .collect();
                    allowed.sort_unstable();
                    allowed.dedup();
                    door.allowed_locks = Some(allowed);
                }
            }
        }
    }

    /// The map update after wiring: segment ends become no-return doors,
    /// sealed doors lead nowhere (blocked, or locked for good), and connected
    /// doors name their new far side.
    fn update_targets(&mut self, segments: &[Segment]) -> Result<()> {
        let mut seen = HashSet::new();
        let doors: Vec<usize> = segments
            .iter()
            .flat_map(|segment| self.segment_doors(segment))
            .filter(|door| seen.insert(*door))
            .collect();
        for door in doors {
            if self.is_segment_end_door(door) {
                let target = self.doors[door].target.ok_or_else(|| {
                    anyhow!("segment end {:?} leads nowhere", self.doors[door].at)
                })?;
                self.door_mut(door).kind = Some(DoorKind::NoReturn);
                self.door_mut(target).kind = Some(DoorKind::Blocked);
            }
            let Some(target) = self.doors[door].target else {
                let d = self.door_mut(door);
                if d.no_unlock {
                    d.kind = Some(DoorKind::Blocked);
                } else {
                    d.lock = Some(crate::locks::LOCK_NOWHERE);
                    d.kind = Some(DoorKind::Locked);
                }
                d.requires.clear();
                d.allowed_locks = Some(Vec::new());
                continue;
            };
            let (room, _) = self.doors[target].at.clone();
            let Some(id) = self.door(target).id else {
                continue;
            };
            self.door_mut(door).target = Some(format!("{room}:{id}"));
        }
        for room in self.map.rooms.values_mut() {
            for door in &mut room.doors {
                if door.target.is_none() {
                    door.requires.clear();
                }
            }
        }
        Ok(())
    }

    /// The plan: rooms numbered by segment, in the order each segment used
    /// them, and every door's new far side.
    fn plan(&self, original: &Map, segments: &[Segment]) -> Result<DoorPlan> {
        let mut rooms: IndexMap<String, PlannedRoom> = IndexMap::new();
        for segment in segments {
            let mut number: u16 = 0;
            for &piece in &segment.used {
                for key in &self.pieces[piece].rooms {
                    let Some(room) = self.map.rooms.get(key) else {
                        continue;
                    };
                    let dir = room.room_id().to_ascii_lowercase();
                    if rooms.contains_key(&dir) {
                        continue;
                    }
                    let room_number = loop {
                        let room_number = u8::try_from(number).map_err(|_| {
                            anyhow!("stage {} has more than 256 rooms", segment.number)
                        })?;
                        if !RESERVED_IDS.contains(&room_id(segment.number, room_number).as_str()) {
                            break room_number;
                        }
                        number += 1;
                    };
                    rooms.insert(
                        dir,
                        PlannedRoom {
                            stage: segment.number,
                            room: room_number,
                            id: room_id(segment.number, room_number),
                        },
                    );
                    number += 1;
                }
            }
        }
        for room in self.map.rooms.values() {
            let dir = room.room_id().to_ascii_lowercase();
            if !rooms.contains_key(&dir) {
                bail!("room {} is in the game but in no segment", room.key);
            }
        }

        let original_targets: HashMap<(&str, u8), Option<&str>> = original
            .rooms
            .values()
            .flat_map(|room| {
                room.doors.iter().filter_map(move |door| {
                    Some(((room.key.as_str(), door.id?), door.target_room()))
                })
            })
            .collect();
        let mut doors = Vec::new();
        for room in self.map.rooms.values() {
            for door in &room.doors {
                let Some(id) = door.id else {
                    continue;
                };
                let script_target = original_targets
                    .get(&(room.key.as_str(), id))
                    .copied()
                    .flatten()
                    .map(dir_of);
                let target = match (door.target_room(), door.target_id()) {
                    (Some(to), Some(at)) => {
                        let far = self.map.rooms.get(to).ok_or_else(|| {
                            anyhow!("{}:{id} leads to {to}, not in the game", room.key)
                        })?;
                        let entrance = far
                            .doors
                            .iter()
                            .find(|d| d.id == Some(at))
                            .and_then(|d| d.entrance)
                            .ok_or_else(|| anyhow!("{to}:{at} has no entrance"))?;
                        let dir = dir_of(to);
                        let new_id = rooms
                            .get(&dir)
                            .map(|r| r.id.clone())
                            .ok_or_else(|| anyhow!("room {to} has no new id"))?;
                        Some(PlannedTarget {
                            room: to.to_string(),
                            dir,
                            id: new_id,
                            door: at,
                            entrance,
                        })
                    }
                    _ => None,
                };
                doors.push(PlannedDoor {
                    room: room.key.clone(),
                    dir: room.room_id().to_ascii_lowercase(),
                    id,
                    script_target,
                    randomized: door.randomize != Some(false),
                    target,
                    no_return: door.kind == Some(DoorKind::NoReturn),
                });
            }
        }
        let tagged = |tag: &str| {
            self.map
                .rooms
                .values()
                .find(|room| room.filter.has_tag(tag))
                .map(|room| room.room_id().to_ascii_lowercase())
                .ok_or_else(|| anyhow!("no room is tagged {tag}"))
        };
        Ok(DoorPlan {
            rooms,
            doors,
            segments: u8::try_from(segments.len())?,
            start: tagged(tags::BEGIN)?,
            end: tagged(tags::END)?,
        })
    }
}

/// The directory, lowercase, of a logical room key (`10C/BACK` is `10c`).
fn dir_of(key: &str) -> String {
    key.split('/').next().unwrap_or(key).to_ascii_lowercase()
}

/// Staging's `GroupDictionary`: pieces that go together, merged as they are
/// added.
#[derive(Default)]
struct Groups {
    groups: HashMap<usize, Vec<usize>>,
}

impl Groups {
    fn add(&mut self, items: &[usize]) {
        let Some((&first, rest)) = items.split_first() else {
            return;
        };
        for &next in rest {
            self.join(first, next);
        }
    }

    fn join(&mut self, a: usize, b: usize) {
        let group_a = self.groups.get(&a).cloned();
        let group_b = self.groups.get(&b).cloned();
        let merged = match (group_a, group_b) {
            (Some(ga), Some(gb)) => {
                if ga == gb {
                    return;
                }
                let mut merged = ga;
                for item in gb {
                    if !merged.contains(&item) {
                        merged.push(item);
                    }
                }
                merged
            }
            (Some(mut ga), None) => {
                if !ga.contains(&b) {
                    ga.push(b);
                }
                ga
            }
            (None, Some(mut gb)) => {
                if !gb.contains(&a) {
                    gb.push(a);
                }
                gb
            }
            (None, None) => vec![a, b],
        };
        for &item in &merged {
            self.groups.insert(item, merged.clone());
        }
    }

    fn get(&self, item: usize) -> Vec<usize> {
        self.groups
            .get(&item)
            .cloned()
            .unwrap_or_else(|| vec![item])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_ids_take_the_short_form_where_it_exists() {
        assert_eq!(room_id(1, 0), "100");
        assert_eq!(room_id(3, 0x1F), "31f");
        assert_eq!(room_id(1, 0x20), "01-20");
        assert_eq!(room_id(9, 0), "09-00");
    }

    #[test]
    fn groups_merge_like_staging() {
        let mut groups = Groups::default();
        groups.add(&[1, 2]);
        groups.add(&[3, 4]);
        assert_eq!(groups.get(1), vec![1, 2]);
        assert_eq!(groups.get(5), vec![5]);
        groups.add(&[2, 3]);
        assert_eq!(groups.get(4), vec![1, 2, 3, 4]);
        assert_eq!(groups.get(1), groups.get(3));
    }
}
