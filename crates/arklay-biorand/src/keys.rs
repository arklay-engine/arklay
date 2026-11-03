//! Staging's `KeyRandomizer` (`KeyRandomizer.cs`), graph and placement:
//! which item slot gets which key, so the game can be finished.
//!
//! The map becomes a routing [`Graph`]: a `START` node, a node per room, an
//! edge per door, a node per item slot and per graph flag. The route finder
//! then places a key in slots until every edge can be opened. Each key
//! item's amount (`KeyRandomizer.cs:149-195`) is left to the item stage.
//!
//! The graph follows the map as it is given. Staging first reshapes the map
//! for the configuration (`Re1ClassicRandomizerController.GetMap`: the areas
//! left out, the slot groups, the segmented caves and lab); that is the
//! caller's job, as door randomization will be when it rewires the doors.

use anyhow::{Context, Result, anyhow, bail};
use indexmap::IndexMap;

use crate::config::Config;
use crate::map::{Door, DoorKind, Map, Player, Requirement as MapRequirement, Room};
use crate::rng::Rng;
use crate::routing::{
    self, EdgeKind, Graph, GraphBuilder, KeyId, KeyKind, NodeId, Requirement, RouteFinderOptions,
};

/// Staging gives up after this many dead ends (`KeyRandomizer.cs:136-147`).
const MAX_DEAD_ENDS: usize = 1000;

/// The Sword Key and the Small Key: with the lockpick, their locks need no
/// key (`Re1ClassicRandomizerController.cs:185-190`).
const LOCKPICK_KEYS: [u8; 2] = [51, 61];

/// What goes into the graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyOptions {
    pub player: Player,
    /// Whether doors are randomized, for the map's `door-rando` filters.
    pub door_rando: bool,
    /// Document slots may hold keys (`items/documents` and
    /// `items/documents/keys`).
    pub include_documents: bool,
    /// Hidden slots may hold keys (`items/hidden/keys`).
    pub include_hidden: bool,
    /// Key items whose locks need no key: requirements on them are dropped
    /// (staging's `Implicit`).
    pub implicit_keys: Vec<u8>,
}

impl KeyOptions {
    /// Only what the map holds for `player`, on the original doors.
    pub fn new(player: Player) -> Self {
        Self {
            player,
            door_rando: false,
            include_documents: false,
            include_hidden: false,
            implicit_keys: Vec::new(),
        }
    }

    /// The options a resolved configuration gives, read as staging reads
    /// them.
    pub fn from_config(config: &Config, player: Player) -> Self {
        let lockpick = config.get_str("inventory/special/lockpick", "Always") == "Always";
        Self {
            player,
            door_rando: config.get_bool("doors/random", false),
            include_documents: config.get_bool("items/documents", false)
                && config.get_bool("items/documents/keys", false),
            include_hidden: config.get_bool("items/hidden/keys", false),
            implicit_keys: if lockpick {
                LOCKPICK_KEYS.to_vec()
            } else {
                Vec::new()
            },
        }
    }
}

/// An item slot of the map that is a node of the graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slot {
    /// The logical room it is in.
    pub room: String,
    /// The AOT id of its `item_aot_set`.
    pub item: u8,
    /// Its pickup flag (staging's `globalId`).
    pub flag: Option<u16>,
}

/// The map's routing graph, and what its keys and item nodes stand for.
#[derive(Clone, Debug)]
pub struct KeyGraph {
    pub graph: Graph,
    /// The key item each key of the graph is.
    pub key_items: IndexMap<KeyId, u8>,
    /// The slot each item node is.
    pub slots: IndexMap<NodeId, Slot>,
}

/// Builds the routing graph of `map` (`KeyRandomizer.cs:23-134`).
pub fn build_graph(map: &Map, options: &KeyOptions) -> Result<KeyGraph> {
    let included =
        |filter: &crate::map::Filter| filter.includes(options.door_rando, options.player);
    let mut builder = GraphBuilder::new();

    // Keys: every catalogue item whose kind is `key/<kind>`.
    let mut item_keys: IndexMap<u8, KeyId> = IndexMap::new();
    for (&id, def) in &map.items {
        let Some(kind) = def.kind.strip_prefix("key/") else {
            continue;
        };
        let kind = KeyKind::parse(kind)
            .ok_or_else(|| anyhow!("item {id} ({}): unknown key kind `{}`", def.name, def.kind))?;
        item_keys.insert(id, builder.key(def.name.clone(), def.group, kind));
    }

    let begin_end = map
        .begin_end
        .iter()
        .find(|entry| included(&entry.filter))
        .ok_or_else(|| anyhow!("the map has no start room for {:?}", options.player))?;
    let rooms: Vec<&Room> = map
        .rooms
        .values()
        .filter(|room| included(&room.filter))
        .collect();

    let start = builder.room("START");
    let mut room_nodes: IndexMap<&str, NodeId> = IndexMap::new();
    for room in &rooms {
        let label = match &room.name {
            Some(name) => format!("{}|{name}", room.key),
            None => room.key.clone(),
        };
        room_nodes.insert(room.key.as_str(), builder.room(label));
    }
    let first = *room_nodes
        .get(begin_end.start.as_str())
        .ok_or_else(|| anyhow!("the start room {} is not in the map", begin_end.start))?;
    builder.door(start, first, &[])?;

    let mut graph = Builder {
        builder,
        options,
        item_keys,
        room_nodes,
        flag_nodes: IndexMap::new(),
    };
    let mut slots = IndexMap::new();
    for room in &rooms {
        let source = graph.room_nodes[room.key.as_str()];
        for door in room.doors.iter().filter(|door| included(&door.filter)) {
            graph
                .add_door(&rooms, source, door)
                .with_context(|| format!("room {} door {:?}", room.key, door.id))?;
        }
        for item in room.items.iter().filter(|item| included(&item.filter)) {
            if item.optional
                || (item.document && !options.include_documents)
                || (item.hidden && !options.include_hidden)
            {
                continue;
            }
            let label = match &item.name {
                Some(name) => format!("{}|{}/{name}", room.key, room.name.as_deref().unwrap_or("")),
                None => room.key.clone(),
            };
            let requires = graph
                .requirements(&item.requires)
                .with_context(|| format!("room {} item {}", room.key, item.id))?;
            let node = graph.builder.item(label, item.group, source, &requires);
            slots.insert(
                node,
                Slot {
                    room: room.key.clone(),
                    item: item.id,
                    flag: item.flag,
                },
            );
        }
        for flag in room.flags.iter().filter(|flag| included(&flag.filter)) {
            let requires = graph
                .requirements(&flag.requires)
                .with_context(|| format!("room {} flag {}", room.key, flag.name))?;
            let node = graph.flag(&flag.name);
            graph
                .builder
                .door(source, node, &requires)
                .with_context(|| format!("room {} flag {}", room.key, flag.name))?;
        }
    }

    let key_items = graph
        .item_keys
        .iter()
        .map(|(&item, &key)| (key, item))
        .collect();
    Ok(KeyGraph {
        graph: graph.builder.build()?,
        key_items,
        slots,
    })
}

struct Builder<'a> {
    builder: GraphBuilder,
    options: &'a KeyOptions,
    item_keys: IndexMap<u8, KeyId>,
    room_nodes: IndexMap<&'a str, NodeId>,
    flag_nodes: IndexMap<String, NodeId>,
}

impl Builder<'_> {
    fn add_door(&mut self, rooms: &[&Room], source: NodeId, door: &Door) -> Result<()> {
        // A blocked or locked door is passed from the other side only, and
        // that side adds the edge.
        if door.ignore_in_graph || matches!(door.kind, Some(DoorKind::Blocked | DoorKind::Locked)) {
            return Ok(());
        }
        let Some(target_room) = door.target_room() else {
            return Ok(());
        };
        let target = *self
            .room_nodes
            .get(target_room)
            .ok_or_else(|| anyhow!("target room {target_room} is not in the map"))?;
        let mut requires = self.requirements(&door.requires)?;
        // "The graph library hates it when you have unblock[key] <--> [key]":
        // the side facing an unblock adds no requirements.
        if self.other_side(rooms, door).and_then(|other| other.kind) == Some(DoorKind::Unblock) {
            requires.clear();
        }
        let kind = match door.kind {
            Some(DoorKind::OneWay) => EdgeKind::OneWay,
            Some(DoorKind::NoReturn) => EdgeKind::NoReturn,
            Some(DoorKind::Unblock | DoorKind::Unlock) => EdgeKind::UnlockTwoWay,
            _ => EdgeKind::TwoWay,
        };
        self.builder.edge(source, target, kind, &requires)?;
        Ok(())
    }

    /// The door on the far side of `door` (staging's `Map.GetOtherSide`).
    fn other_side<'r>(&self, rooms: &[&'r Room], door: &Door) -> Option<&'r Door> {
        let target_room = door.target_room()?;
        let target_id = door.target_id()?;
        let room = rooms.iter().find(|room| room.key == target_room)?;
        room.doors.iter().find(|other| {
            other.id == Some(target_id)
                && other
                    .filter
                    .includes(self.options.door_rando, self.options.player)
        })
    }

    fn requirements(&mut self, requires: &[MapRequirement]) -> Result<Vec<Requirement>> {
        let mut result = Vec::new();
        for requirement in requires {
            match requirement {
                MapRequirement::Item(item) => {
                    if self.options.implicit_keys.contains(item) {
                        continue;
                    }
                    let key = self
                        .item_keys
                        .get(item)
                        .ok_or_else(|| anyhow!("requirement item({item}) is not a key item"))?;
                    result.push(Requirement::Key(*key));
                }
                MapRequirement::Flag(name) => result.push(Requirement::Node(self.flag(name))),
                MapRequirement::Room(room) => {
                    let node = self
                        .room_nodes
                        .get(room.as_str())
                        .ok_or_else(|| anyhow!("requirement room({room}) is not in the map"))?;
                    result.push(Requirement::Node(*node));
                }
            }
        }
        Ok(result)
    }

    /// The node of a graph flag, created when first named.
    fn flag(&mut self, name: &str) -> NodeId {
        if let Some(&node) = self.flag_nodes.get(name) {
            return node;
        }
        let node = self.builder.room(name);
        self.flag_nodes.insert(name.to_string(), node);
        node
    }
}

/// A key given to a slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlacedKey {
    pub slot: Slot,
    /// The key item.
    pub key: u8,
}

/// Where the keys went.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyPlacement {
    /// In graph order: the map's room order, then each room's item order.
    pub keys: Vec<PlacedKey>,
    /// Whether every edge of the graph could be opened. Staging doesn't
    /// check it: a partial placement is used as it is.
    pub all_nodes_visited: bool,
    /// The dead ends the route finder backed out of.
    pub dead_ends: usize,
    /// The route finder's log.
    pub log: Vec<String>,
}

/// Places the keys of `map` in its slots (`KeyRandomizer.cs:136-147`).
/// Fails when the map can't be made into a graph, or the route finder gives
/// up after staging's 1000 dead ends.
pub fn randomize(map: &Map, options: &KeyOptions, rng: &mut Rng) -> Result<KeyPlacement> {
    let key_graph = build_graph(map, options)?;
    place(&key_graph, rng)
}

/// Places the keys of an already built graph.
pub fn place(key_graph: &KeyGraph, rng: &mut Rng) -> Result<KeyPlacement> {
    let graph = &key_graph.graph;
    let options = RouteFinderOptions {
        max_dead_ends: Some(MAX_DEAD_ENDS),
        ..RouteFinderOptions::default()
    };
    let route = match routing::find(graph, rng, options) {
        Ok(route) => route,
        Err(routing::RouteError::TooManyDeadEnds(info)) => bail!(
            "failed to find a route, possibly for lack of items to place keys in\n{}",
            info.describe(graph)
        ),
        Err(error) => return Err(error.into()),
    };
    let keys = route
        .placements()
        .filter_map(|(node, key)| {
            Some(PlacedKey {
                slot: key_graph.slots.get(&node)?.clone(),
                key: key_graph.key_items[&key],
            })
        })
        .collect();
    Ok(KeyPlacement {
        keys,
        all_nodes_visited: route.all_nodes_visited(),
        dead_ends: route.dead_ends(),
        log: route.log().to_vec(),
    })
}
