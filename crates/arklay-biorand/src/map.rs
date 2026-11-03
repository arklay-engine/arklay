//! The randomizer's map of the game: every room's doors, pickups, graph flags
//! and enemy rules, and the map-wide item and enemy catalogues. A port of
//! staging's `Map.cs`, read from XML instead of `rdt.json`.
//!
//! # Files
//!
//! The map-wide file is `biorand/map.xml`:
//!
//! ```xml
//! <map>
//!   <begin-end start="106" end="501"/>
//!   <items>
//!     <item id="2" name="Beretta" kind="weapon/handgun" max="15" ammo="11"/>
//!     <item id="52" name="Armor Key" kind="key/reusuable" group="1" discard="true"/>
//!   </items>
//!   <enemies>
//!     <group name="Zombie" background="#66f" foreground="#fff">
//!       <entry key="zombie" name="Zombie" id="0 1 17" esp="3 4">
//!         <pose name="Walking" pose="0" probability="0.4"/>
//!       </entry>
//!     </group>
//!   </enemies>
//! </map>
//! ```
//!
//! Each room's `room/<id>/random.xml` holds one or more logical rooms. A
//! logical room is the whole room, or a part of it the route treats as its own
//! node (`10C/BACK`):
//!
//! ```xml
//! <random>
//!   <room key="10D" name="1F WEST TIGER" tags="mansion mansion2able">
//!     <door id="0" target="103:3" kind="locked" lock="11" key="52"
//!           allowed-locks="" no-unlock="true" randomize="false"
//!           requires="item(33) flag(LIFT)" tags="segment-end"
//!           door-rando="true" player="jill">
//!       <entrance x="3000" y="0" z="3600" d="3072" cut="7"/>
//!     </door>
//!     <flag name="COURTYARD_LIFT_POWER" requires="item(57)"/>
//!     <item id="4" flag="175" type="5" amount="6" group="1" name="RED EYE"
//!           requires="item(34)" document="true" hidden="true" optional="true"
//!           lock="35" player="chris" door-rando="false"/>
//!     <enemies id="0" flag="28" kind="npc" include="11 12" exclude="4 12"
//!              max-difficulty="0" restricted="true" player="chris"/>
//!     <spot x="11764" y="0" z="11442" d="2014" include="zombie" exclude=""/>
//!   </room>
//! </random>
//! ```
//!
//! Attribute meanings follow staging's `rdt.json` field for field
//! (`docs/biorand/research/staging-data.md` §1 and
//! `docs/biorand/research/port-map.md` §3). The differences:
//!
//! - A room's RDTs are its directory: arklay's rooms hold both mansion visits.
//! - `player` is `chris` or `jill` instead of 0 or 1.
//! - Lists are space-separated; `requires` holds requirement atoms.
//! - An item's `flag` is staging's `globalId`, the pickup flag in `FG_ITEM`.
//!   `typeOffsets` is gone: scripts use `picked()` instead.
//! - An enemy rule has no `condition`: the room's script says where added
//!   enemies go with `random_enemies()` (`docs/biorand/plan.md`).
//! - `<spot>` holds the room's candidate enemy positions (staging's
//!   `enemy.csv`), and a door's `<entrance>` its arrival point (staging's
//!   `doors.csv` and inline entrances).

use indexmap::IndexMap;

/// Who the player is, for entries that exist for one character only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Player {
    Chris,
    Jill,
}

/// The filter fields every map entry may carry (staging's `MapFilterable`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    pub tags: Vec<String>,
    /// Present only when door randomization is (or isn't) on.
    pub door_rando: Option<bool>,
    /// Present only for one player.
    pub player: Option<Player>,
}

impl Filter {
    /// Whether an entry with these fields is part of a game with `door_rando`
    /// for `player` (staging's `IsIncludedInFilter`).
    pub fn includes(&self, door_rando: bool, player: Player) -> bool {
        self.door_rando.is_none_or(|value| value == door_rando)
            && self.player.is_none_or(|value| value == player)
    }

    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }
}

/// Room and door tags with a meaning to the generator (staging's `MapTags`).
pub mod tags {
    pub const BOX: &str = "box";
    pub const SAFE: &str = "safe";
    pub const SAVE: &str = "save";
    pub const PROLOGUE: &str = "prologue";
    pub const SEGMENT_END: &str = "segment-end";
    pub const BEGIN: &str = "begin";
    pub const END: &str = "end";
    /// The door must be connected out from this room: it can't be the way the
    /// player first reaches it.
    pub const CONNECT_OUT: &str = "connect-out";
    /// The door must connect back to a room with a shared ancestor, so a
    /// player can return after a one-way door.
    pub const CONNECT_BACK: &str = "connect-back";
    /// The lock randomizer prefers to lock this door.
    pub const LOCK_PRIORITY: &str = "lock-priority";
}

/// What kind of door a door is (staging's `DoorKinds`). `None` on a door is
/// an ordinary two-way door.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DoorKind {
    /// Locked and unlocked by the room's script; its lock id is kept.
    Dynamic,
    /// Permanently or temporarily blocked: a wall.
    Blocked,
    /// Can be unblocked into a two-way passage, perhaps after a puzzle.
    Unblock,
    /// Locked, but unlockable without a key from the other side.
    Locked,
    /// Unlockable without a key from this side.
    Unlock,
    /// One way, but there is another way back.
    OneWay,
    /// One way, with no way back.
    NoReturn,
}

impl DoorKind {
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "dynamic" => Self::Dynamic,
            "blocked" => Self::Blocked,
            "unblock" => Self::Unblock,
            "locked" => Self::Locked,
            "unlock" => Self::Unlock,
            "oneway" => Self::OneWay,
            "noreturn" => Self::NoReturn,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dynamic => "dynamic",
            Self::Blocked => "blocked",
            Self::Unblock => "unblock",
            Self::Locked => "locked",
            Self::Unlock => "unlock",
            Self::OneWay => "oneway",
            Self::NoReturn => "noreturn",
        }
    }
}

/// One requirement of an edge: `item(52)`, `flag(LIFT)` or `room(513)`
/// (staging's `MapRequirement`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Requirement {
    Flag(String),
    Item(u8),
    Room(String),
}

impl Requirement {
    pub fn parse(text: &str) -> Option<Self> {
        let (kind, rest) = text.split_once('(')?;
        let value = rest.strip_suffix(')')?;
        if value.contains(['(', ')']) {
            return None;
        }
        Some(match kind.to_ascii_lowercase().as_str() {
            "flag" => Self::Flag(value.to_string()),
            "item" => Self::Item(value.parse().ok()?),
            "room" => Self::Room(value.to_string()),
            _ => return None,
        })
    }
}

impl std::fmt::Display for Requirement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Flag(value) => write!(f, "flag({value})"),
            Self::Item(value) => write!(f, "item({value})"),
            Self::Room(value) => write!(f, "room({value})"),
        }
    }
}

/// Where a player arriving through a door stands, and the cut they see.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Entrance {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub d: i32,
    pub cut: Option<u8>,
}

/// A door of a room (staging's `MapRoomDoor`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Door {
    /// The AOT id of its `door_aot_set`. `None` for a fixed exit with no door
    /// AOT (a scripted move to another room).
    pub id: Option<u8>,
    pub name: Option<String>,
    /// The original's far side: a room key, and the door there if any.
    pub target: Option<String>,
    pub kind: Option<DoorKind>,
    pub lock: Option<u8>,
    /// The key item that opens it.
    pub key: Option<u8>,
    /// `None`: undecided. `Some([])`: may not be locked. Otherwise the keys
    /// it may be given.
    pub allowed_locks: Option<Vec<u8>>,
    /// Can't be a one-sided lock; a dead end through it becomes blocked.
    pub no_unlock: bool,
    /// `Some(false)`: door randomization keeps its connection.
    pub randomize: Option<bool>,
    pub requires: Vec<Requirement>,
    pub entrance: Option<Entrance>,
    pub ignore_in_graph: bool,
    pub filter: Filter,
}

impl Door {
    /// The room key the original leads to.
    pub fn target_room(&self) -> Option<&str> {
        self.target
            .as_deref()
            .map(|t| t.split(':').next().unwrap_or(t))
    }

    /// The door id on the far side the original leads to.
    pub fn target_id(&self) -> Option<u8> {
        self.target.as_deref()?.split(':').nth(1)?.parse().ok()
    }
}

/// A named graph flag a room raises once its requirements are met (staging's
/// `MapEdge` in `flags`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GraphFlag {
    pub name: String,
    pub requires: Vec<Requirement>,
    pub filter: Filter,
}

/// A pickup slot (staging's `MapRoomItem`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Item {
    /// The AOT id of its `item_aot_set`.
    pub id: u8,
    /// Its pickup flag in `FG_ITEM` (staging's `globalId`): the slot's
    /// identity, shared by both mansion visits.
    pub flag: Option<u16>,
    /// The original's item type and amount.
    pub kind: Option<u8>,
    pub amount: Option<u8>,
    pub name: Option<String>,
    /// The item box or drawer lock it sits behind.
    pub lock: Option<u8>,
    /// Placement groups, as a bit mask; 0 places anywhere.
    pub group: u32,
    pub requires: Vec<Requirement>,
    pub document: bool,
    pub hidden: bool,
    pub optional: bool,
    pub filter: Filter,
}

/// One enemy rule of a room (staging's `MapRoomEnemies`). With an `id` it
/// retypes that fixed enemy; without one the room's enemies are replaced.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Enemies {
    pub id: Option<u8>,
    /// The enemy's kill flag.
    pub flag: Option<u16>,
    /// `npc` rules are left alone.
    pub kind: Option<String>,
    pub include: Option<Vec<u8>>,
    pub exclude: Vec<u8>,
    pub max_difficulty: Option<u8>,
    pub restricted: bool,
    pub filter: Filter,
}

/// A candidate enemy position (staging's `enemy.csv`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Spot {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub d: i32,
    /// Enemy keys allowed here; empty allows all.
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    /// The `random_enemies()` placeholder the spot belongs to, when the room
    /// has more than one.
    pub placeholder: Option<String>,
}

/// One logical room (staging's `MapRoom`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Room {
    /// `106`, or `10C/BACK` for a part of a room.
    pub key: String,
    pub name: Option<String>,
    pub doors: Vec<Door>,
    pub flags: Vec<GraphFlag>,
    pub items: Vec<Item>,
    pub enemies: Vec<Enemies>,
    pub spots: Vec<Spot>,
    pub filter: Filter,
}

impl Room {
    /// The room directory the logical room lives in: `10C/BACK` is in `10c`.
    pub fn room_id(&self) -> &str {
        self.key.split('/').next().unwrap_or(&self.key)
    }
}

/// An item type in the catalogue (staging's `MapItemDefinition`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemDef {
    pub name: String,
    /// `weapon/handgun`, `ammo/shotgun`, `key/consumable`, `health/g`,
    /// `ink`, `map`, `document`, …
    pub kind: String,
    pub ammo: Vec<u8>,
    pub max: u16,
    pub group: u32,
    pub amount: Option<u8>,
    /// A generic key the game discards after its last door; lock
    /// randomization may give it new doors.
    pub discard: bool,
}

/// A pose an enemy may start in.
#[derive(Clone, Debug, PartialEq)]
pub struct EnemyPose {
    pub name: String,
    pub pose: u8,
    pub probability: f64,
}

/// One kind of enemy the randomizer may place (staging's
/// `MapEnemyGroupEntry`).
#[derive(Clone, Debug, PartialEq)]
pub struct EnemyEntry {
    /// The configuration key: `zombie`.
    pub key: String,
    pub name: String,
    /// Enemy type ids; one is chosen.
    pub ids: Vec<u8>,
    /// Effect sheets it needs.
    pub esp: Vec<u8>,
    pub poses: Vec<EnemyPose>,
}

/// A group of enemy kinds, with its colours in the configuration screen.
#[derive(Clone, Debug, PartialEq)]
pub struct EnemyGroup {
    pub name: String,
    pub background: String,
    pub foreground: String,
    pub entries: Vec<EnemyEntry>,
}

/// The game's start and end rooms.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BeginEnd {
    pub start: String,
    pub end: String,
    pub filter: Filter,
}

/// The whole map. Rooms and items keep their file order, which the stages
/// depend on as staging's do.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Map {
    pub begin_end: Vec<BeginEnd>,
    pub items: IndexMap<u8, ItemDef>,
    pub enemies: Vec<EnemyGroup>,
    pub rooms: IndexMap<String, Room>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requirements_round_trip() {
        for text in ["item(52)", "flag(COURTYARD_LIFT_POWER)", "room(513)"] {
            assert_eq!(Requirement::parse(text).unwrap().to_string(), text);
        }
        assert_eq!(
            Requirement::parse("ITEM(3)"),
            Some(Requirement::Item(3)),
            "staging parses the kind case-insensitively"
        );
        assert_eq!(Requirement::parse("item(x)"), None);
        assert_eq!(Requirement::parse("thing(1)"), None);
        assert_eq!(Requirement::parse("item((1))"), None);
    }

    #[test]
    fn filters_match_like_staging() {
        let any = Filter::default();
        assert!(any.includes(true, Player::Jill));
        let jill_doors = Filter {
            door_rando: Some(true),
            player: Some(Player::Jill),
            ..Filter::default()
        };
        assert!(jill_doors.includes(true, Player::Jill));
        assert!(!jill_doors.includes(false, Player::Jill));
        assert!(!jill_doors.includes(true, Player::Chris));
    }

    #[test]
    fn door_targets_split() {
        let door = Door {
            target: Some("10C/BACK:2".into()),
            ..Door::default()
        };
        assert_eq!(door.target_room(), Some("10C/BACK"));
        assert_eq!(door.target_id(), Some(2));
        let exit = Door {
            target: Some("301".into()),
            ..Door::default()
        };
        assert_eq!(exit.target_room(), Some("301"));
        assert_eq!(exit.target_id(), None);
    }
}
