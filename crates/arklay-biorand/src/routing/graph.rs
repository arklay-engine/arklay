//! The routing graph: keys, nodes and the edges between nodes (common's
//! `Key`, `Node`, `Edge`, `Requirement`, `Graph` and `GraphBuilder`).

use std::fmt;

/// A key: something the player carries that opens edges. Ids index
/// [`Graph::keys`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KeyId(u32);

/// A node: a room, a flag, a gate or an item slot. Ids index
/// [`Graph::nodes`]; node 0 is where the player starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(u32);

/// An edge of a built [`Graph`]. Ids index [`Graph::edges`], which are sorted
/// by (source, destination).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EdgeId(u32);

macro_rules! id_index {
    ($($id:ident),*) => {$(
        impl $id {
            pub fn index(self) -> usize {
                self.0 as usize
            }

            pub(crate) fn from_index(index: usize) -> Self {
                Self(u32::try_from(index).expect("fewer than 2^32 entries"))
            }
        }
    )*};
}
id_index!(KeyId, NodeId, EdgeId);

/// How a key behaves once it has opened something.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyKind {
    /// Kept: one copy opens every edge that needs it.
    Reusable,
    /// Used up: each edge needing it takes a copy.
    Consumable,
    /// Left behind in the edge, but the player can take it back out, so the
    /// copies needed are the most any one path from the start uses at once.
    Removable,
}

impl KeyKind {
    /// Parses the kind part of an item's `key/<kind>`. Staging spells the
    /// first `reusuable`, after common's enum; both spellings are accepted.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text.to_ascii_lowercase().as_str() {
            "reusuable" | "reusable" => Self::Reusable,
            "consumable" => Self::Consumable,
            "removable" => Self::Removable,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Key {
    pub label: String,
    /// The placement groups: the key goes only in an item slot whose group
    /// has every one of these bits.
    pub group: u32,
    pub kind: KeyKind,
}

/// Whether a node is a place or an item slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NodeKind {
    /// A room, flag or gate: anything the player reaches but can't hold.
    Room,
    /// A slot a key may be placed in.
    Item,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub label: String,
    /// For an item slot, the groups of keys it takes (see [`Key::group`]).
    pub group: u32,
    pub kind: NodeKind,
}

impl Node {
    pub fn is_item(&self) -> bool {
        self.kind == NodeKind::Item
    }
}

/// What an edge needs: a key held, or a node already visited.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Requirement {
    Key(KeyId),
    Node(NodeId),
}

impl From<KeyId> for Requirement {
    fn from(key: KeyId) -> Self {
        Self::Key(key)
    }
}

impl From<NodeId> for Requirement {
    fn from(node: NodeId) -> Self {
        Self::Node(node)
    }
}

/// How the player may pass an edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EdgeKind {
    /// Either way. Its requirements can be met from either side, and once
    /// met the edge stays open both ways: a door whose key works from both
    /// sides.
    TwoWay,
    /// Either way once open, but it can only be opened from the source: a
    /// chained door cut from one side.
    UnlockTwoWay,
    /// Source to destination only, but the player can get back to the source
    /// some other way: a ladder that breaks behind them.
    OneWay,
    /// Source to destination only, with no way back: everything behind it is
    /// left for good.
    NoReturn,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edge {
    pub source: NodeId,
    pub destination: NodeId,
    pub requires: Vec<Requirement>,
    pub kind: EdgeKind,
}

impl Edge {
    /// The keys it needs, one entry per copy.
    pub fn required_keys(&self) -> impl Iterator<Item = KeyId> + '_ {
        self.requires.iter().filter_map(|r| match r {
            Requirement::Key(key) => Some(*key),
            Requirement::Node(_) => None,
        })
    }

    /// The nodes that must be visited before it opens.
    pub fn required_nodes(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.requires.iter().filter_map(|r| match r {
            Requirement::Node(node) => Some(*node),
            Requirement::Key(_) => None,
        })
    }

    /// The end that isn't `node`.
    pub fn other_end(&self, node: NodeId) -> NodeId {
        if self.source == node {
            self.destination
        } else {
            debug_assert_eq!(self.destination, node, "edge does not contain node");
            self.source
        }
    }
}

/// A mistake in building a graph: edges added twice in ways that disagree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GraphError {
    NoNodes,
    /// The same source and destination again, with another kind.
    DuplicateKind {
        source: String,
        destination: String,
    },
    /// The same source and destination again, with other requirements.
    DuplicateRequirements {
        source: String,
        destination: String,
    },
    /// The reverse of an existing edge, and the two can't be reconciled.
    ConflictingInverse {
        source: String,
        destination: String,
        reason: &'static str,
    },
}

impl fmt::Display for GraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoNodes => write!(f, "graph contains no nodes"),
            Self::DuplicateKind {
                source,
                destination,
            } => write!(
                f,
                "duplicate edge {source} -> {destination} added with a different kind"
            ),
            Self::DuplicateRequirements {
                source,
                destination,
            } => write!(
                f,
                "duplicate edge {source} -> {destination} added with different requirements"
            ),
            Self::ConflictingInverse {
                source,
                destination,
                reason,
            } => write!(
                f,
                "edge {source} -> {destination} conflicts with its inverse: {reason}"
            ),
        }
    }
}

impl std::error::Error for GraphError {}

/// A finished graph. Node 0 is the start.
#[derive(Clone, Debug)]
pub struct Graph {
    keys: Vec<Key>,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    /// Each node's outgoing and incoming edges, in edge order.
    from: Vec<Vec<EdgeId>>,
    to: Vec<Vec<EdgeId>>,
}

impl Graph {
    pub fn new(keys: Vec<Key>, nodes: Vec<Node>, mut edges: Vec<Edge>) -> Result<Self, GraphError> {
        if nodes.is_empty() {
            return Err(GraphError::NoNodes);
        }
        // Common sorts each node's edges by (source, destination) when it
        // builds its edge maps, and sorts its frontier the same way. Ids in
        // that order make every set of edge ids iterate in it.
        edges.sort_by_key(|edge| (edge.source, edge.destination));
        let mut from = vec![Vec::new(); nodes.len()];
        let mut to = vec![Vec::new(); nodes.len()];
        for (i, edge) in edges.iter().enumerate() {
            from[edge.source.index()].push(EdgeId::from_index(i));
            to[edge.destination.index()].push(EdgeId::from_index(i));
        }
        Ok(Self {
            keys,
            nodes,
            edges,
            from,
            to,
        })
    }

    pub fn keys(&self) -> &[Key] {
        &self.keys
    }

    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    pub fn key_ids(&self) -> impl Iterator<Item = KeyId> + use<> {
        (0..self.keys.len()).map(KeyId::from_index)
    }

    pub fn node_ids(&self) -> impl Iterator<Item = NodeId> + use<> {
        (0..self.nodes.len()).map(NodeId::from_index)
    }

    pub fn key(&self, id: KeyId) -> &Key {
        &self.keys[id.index()]
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.index()]
    }

    pub fn edge(&self, id: EdgeId) -> &Edge {
        &self.edges[id.index()]
    }

    pub fn start(&self) -> NodeId {
        NodeId(0)
    }

    pub fn edges_from(&self, node: NodeId) -> &[EdgeId] {
        &self.from[node.index()]
    }

    pub fn edges_to(&self, node: NodeId) -> &[EdgeId] {
        &self.to[node.index()]
    }

    /// The edges the player may leave `node` by: its outgoing edges, then
    /// the two-way edges coming in.
    pub fn applicable_edges_from(&self, node: NodeId) -> impl Iterator<Item = EdgeId> + '_ {
        let back = self.edges_to(node).iter().copied();
        self.edges_from(node)
            .iter()
            .copied()
            .chain(back.filter(move |&id| self.edge(id).kind == EdgeKind::TwoWay))
    }

    /// The edges the player may arrive at `node` by: its two-way outgoing
    /// edges, then its incoming edges.
    pub fn applicable_edges_to(&self, node: NodeId) -> impl Iterator<Item = EdgeId> + '_ {
        let out = self.edges_from(node).iter().copied();
        out.filter(move |&id| self.edge(id).kind == EdgeKind::TwoWay)
            .chain(self.edges_to(node).iter().copied())
    }

    /// `N5 (label)`, for logs.
    pub fn node_name(&self, id: NodeId) -> String {
        format!("N{} ({})", id.0, self.node(id).label)
    }

    /// `K5 (label)`, for logs.
    pub fn key_name(&self, id: KeyId) -> String {
        format!("K{} ({})", id.0, self.key(id).label)
    }

    /// One line per edge, in the shape of common's `Edge.ToString`: `<=>` is
    /// two-way, `==>` unlocks two-way, `-->` one-way and `|>` no return.
    pub fn edge_name(&self, id: EdgeId) -> String {
        let edge = self.edge(id);
        let requires: Vec<String> = edge
            .requires
            .iter()
            .map(|r| match *r {
                Requirement::Key(key) => self.key_name(key),
                Requirement::Node(node) => self.node_name(node),
            })
            .collect();
        let requires = if requires.is_empty() {
            String::new()
        } else {
            format!(" [{}] ", requires.join(", "))
        };
        let (left, right) = match edge.kind {
            EdgeKind::TwoWay => ("<=", "=>"),
            EdgeKind::UnlockTwoWay => ("==", "=>"),
            EdgeKind::OneWay => ("--", "->"),
            EdgeKind::NoReturn => ("|", "|>"),
        };
        format!(
            "{} {left}{requires}{right} {}",
            self.node_name(edge.source),
            self.node_name(edge.destination)
        )
    }
}

impl fmt::Display for Graph {
    /// Every edge, one per line.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for i in 0..self.edges.len() {
            writeln!(f, "{}", self.edge_name(EdgeId::from_index(i)))?;
        }
        Ok(())
    }
}

/// Builds a graph room by room and door by door (common's `GraphBuilder`).
/// The first node added is the start.
#[derive(Clone, Debug, Default)]
pub struct GraphBuilder {
    keys: Vec<Key>,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
}

impl GraphBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn key(&mut self, label: impl Into<String>, group: u32, kind: KeyKind) -> KeyId {
        self.keys.push(Key {
            label: label.into(),
            group,
            kind,
        });
        KeyId::from_index(self.keys.len() - 1)
    }

    pub fn room(&mut self, label: impl Into<String>) -> NodeId {
        self.node(label.into(), 0, NodeKind::Room)
    }

    /// An item slot in `room`, reached from it by a two-way edge with
    /// `requires`.
    pub fn item(
        &mut self,
        label: impl Into<String>,
        group: u32,
        room: NodeId,
        requires: &[Requirement],
    ) -> NodeId {
        let item = self.node(label.into(), group, NodeKind::Item);
        self.edge(room, item, EdgeKind::TwoWay, requires)
            .expect("a new node has no edges to conflict with");
        item
    }

    fn node(&mut self, label: String, group: u32, kind: NodeKind) -> NodeId {
        self.nodes.push(Node { label, group, kind });
        NodeId::from_index(self.nodes.len() - 1)
    }

    /// Adds an edge, or returns the existing one it duplicates.
    ///
    /// Adding an edge twice is allowed when both agree. Adding the reverse of
    /// an existing edge (each side of a door adds its own) is reconciled as
    /// common does:
    ///
    /// - two two-way edges: the one with requirements wins; both having
    ///   different requirements is an error;
    /// - an `UnlockTwoWay` meeting a two-way edge without requirements, or an
    ///   `UnlockTwoWay` without requirements, replaces it; an `UnlockTwoWay`
    ///   meeting an existing two-way edge with requirements is an error;
    /// - a two-way edge without requirements meeting an existing
    ///   `UnlockTwoWay` is dropped, and is an error if it has requirements;
    /// - anything meeting a one-way or no-return edge is an error.
    pub fn edge(
        &mut self,
        source: NodeId,
        destination: NodeId,
        kind: EdgeKind,
        requires: &[Requirement],
    ) -> Result<Edge, GraphError> {
        use EdgeKind::*;

        let names = |this: &Self| {
            (
                this.nodes[source.index()].label.clone(),
                this.nodes[destination.index()].label.clone(),
            )
        };
        let same = self
            .edges
            .iter()
            .position(|e| e.source == source && e.destination == destination);
        if let Some(i) = same {
            let existing = &self.edges[i];
            let (source, destination) = names(self);
            if existing.kind != kind {
                return Err(GraphError::DuplicateKind {
                    source,
                    destination,
                });
            }
            if existing.requires != requires {
                return Err(GraphError::DuplicateRequirements {
                    source,
                    destination,
                });
            }
            return Ok(existing.clone());
        }

        let edge = Edge {
            source,
            destination,
            requires: requires.to_vec(),
            kind,
        };
        let inverse = self
            .edges
            .iter()
            .position(|e| e.source == destination && e.destination == source);
        let Some(i) = inverse else {
            self.edges.push(edge.clone());
            return Ok(edge);
        };
        let existing = &self.edges[i];
        let conflict = |reason| {
            let (source, destination) = names(self);
            Err(GraphError::ConflictingInverse {
                source,
                destination,
                reason,
            })
        };
        match (existing.kind, kind) {
            (TwoWay, TwoWay) => {
                if existing.requires == requires {
                    return Ok(existing.clone());
                }
                if !existing.requires.is_empty() {
                    if !requires.is_empty() {
                        return conflict("both sides have different requirements");
                    }
                    return Ok(existing.clone());
                }
                // An inverse with no requirements is replaced.
            }
            (NoReturn, _) | (_, NoReturn) => return conflict("one of them is no-return"),
            (OneWay, _) | (_, OneWay) => return conflict("one of them is one-way"),
            (TwoWay, UnlockTwoWay) | (UnlockTwoWay, UnlockTwoWay) => {
                if !existing.requires.is_empty() {
                    return conflict("the inverse has requirements and this is an unblock");
                }
            }
            (UnlockTwoWay, TwoWay) => {
                if !requires.is_empty() {
                    return conflict("the inverse is an unblock and this side has requirements");
                }
                return Ok(existing.clone());
            }
        }
        self.edges[i] = edge.clone();
        Ok(edge)
    }

    pub fn door(
        &mut self,
        source: NodeId,
        destination: NodeId,
        requires: &[Requirement],
    ) -> Result<Edge, GraphError> {
        self.edge(source, destination, EdgeKind::TwoWay, requires)
    }

    pub fn blocked_door(
        &mut self,
        source: NodeId,
        destination: NodeId,
        requires: &[Requirement],
    ) -> Result<Edge, GraphError> {
        self.edge(source, destination, EdgeKind::UnlockTwoWay, requires)
    }

    pub fn one_way(
        &mut self,
        source: NodeId,
        destination: NodeId,
        requires: &[Requirement],
    ) -> Result<Edge, GraphError> {
        self.edge(source, destination, EdgeKind::OneWay, requires)
    }

    pub fn no_return(
        &mut self,
        source: NodeId,
        destination: NodeId,
        requires: &[Requirement],
    ) -> Result<Edge, GraphError> {
        self.edge(source, destination, EdgeKind::NoReturn, requires)
    }

    pub fn build(self) -> Result<Graph, GraphError> {
        Graph::new(self.keys, self.nodes, self.edges)
    }
}

/// Edges into a node just created can't conflict with anything.
fn fresh(result: Result<Edge, GraphError>) {
    result.expect("a new node has no edges to conflict with");
}

/// Builds a graph by naming each node's dependencies as it is created
/// (common's `DependencyGraphBuilder`): an AND gate needs its source and all
/// its requirements, an OR gate any one of its sources.
#[derive(Clone, Debug, Default)]
pub struct DependencyGraphBuilder {
    builder: GraphBuilder,
}

impl DependencyGraphBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reusable_key(&mut self, group: u32, label: &str) -> KeyId {
        self.builder.key(label, group, KeyKind::Reusable)
    }

    pub fn consumable_key(&mut self, group: u32, label: &str) -> KeyId {
        self.builder.key(label, group, KeyKind::Consumable)
    }

    pub fn removable_key(&mut self, group: u32, label: &str) -> KeyId {
        self.builder.key(label, group, KeyKind::Removable)
    }

    pub fn item(
        &mut self,
        group: u32,
        label: &str,
        source: NodeId,
        requires: &[Requirement],
    ) -> NodeId {
        self.builder.item(label, group, source, requires)
    }

    /// A node with no way in: the start, when it is the first.
    pub fn root(&mut self, label: &str) -> NodeId {
        self.builder.room(label)
    }

    pub fn and_gate(&mut self, label: &str, source: NodeId, requires: &[Requirement]) -> NodeId {
        let node = self.builder.room(label);
        fresh(self.builder.door(source, node, requires));
        node
    }

    pub fn or_gate(&mut self, label: &str, sources: &[NodeId]) -> NodeId {
        let node = self.builder.room(label);
        for &source in sources {
            fresh(self.builder.door(source, node, &[]));
        }
        node
    }

    pub fn one_way(&mut self, label: &str, source: NodeId, requires: &[Requirement]) -> NodeId {
        let node = self.builder.room(label);
        fresh(self.builder.one_way(source, node, requires));
        node
    }

    pub fn no_return(&mut self, label: &str, source: NodeId, requires: &[Requirement]) -> NodeId {
        let node = self.builder.room(label);
        fresh(self.builder.no_return(source, node, requires));
        node
    }

    pub fn build(self) -> Graph {
        self.builder.build().expect("a dependency graph has a node")
    }
}
