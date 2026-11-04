//! Places keys in item slots so that every edge can be opened (common's
//! `RouteFinder`, `Route` and `DeadEndInfo`).
//!
//! The finder walks the graph from the start, taking every edge it can. When
//! it is stuck, it chooses a locked edge, places the keys it needs in item
//! slots already reached, and carries on. Each choice is tried a few times
//! with other slots, depth first; a branch that ends with locked edges left is
//! a dead end, and the finder backs up to the last choice.
//!
//! One-way edges fork the walk: the player crosses with only the keys they
//! are sure to hold, and the fork joins the main walk again when it reaches a
//! node the main walk has visited. No-return edges start a new walk with only
//! those keys, and nothing behind is reachable again.

use std::collections::BTreeSet;
use std::fmt;
use std::rc::Rc;

use indexmap::IndexMap;

use super::bits::BitSet;
use super::graph::{EdgeId, EdgeKind, Graph, KeyId, KeyKind, NodeId, Requirement};
use super::solver::{self, SolveResult};
use crate::rng::Rng;

/// How many slot choices the finder tries for each locked edge before it
/// moves on to the next edge.
const RETRIES: usize = 10;

/// Limits and hooks for a search (common's `RouteFinderOptions`).
#[derive(Default)]
pub struct RouteFinderOptions<'a> {
    /// Give up when the search nests this deep.
    pub depth_limit: Option<usize>,
    /// Give up after this many dead ends. Staging's `KeyRandomizer` stops at
    /// 1000.
    pub max_dead_ends: Option<usize>,
    /// Called at every dead end.
    pub on_dead_end: Option<&'a mut dyn FnMut(&DeadEndInfo)>,
}

/// Why a search gave up.
#[derive(Clone, Debug)]
pub enum RouteError {
    DepthLimit,
    /// [`RouteFinderOptions::max_dead_ends`] was reached; this is the last.
    TooManyDeadEnds(DeadEndInfo),
}

impl fmt::Display for RouteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DepthLimit => write!(f, "the route finder reached its depth limit"),
            Self::TooManyDeadEnds(info) => write!(
                f,
                "failed to find a route: too many dead ends, the last with {} locked edge(s)",
                info.stuck_edges.len()
            ),
        }
    }
}

impl std::error::Error for RouteError {}

/// A key placement (common's `Route`).
#[derive(Clone, Debug)]
pub struct Route<'g> {
    graph: &'g Graph,
    all_nodes_visited: bool,
    item_to_key: Vec<Option<KeyId>>,
    dead_ends: usize,
    log: Vec<String>,
}

impl<'g> Route<'g> {
    pub fn graph(&self) -> &'g Graph {
        self.graph
    }

    /// Whether the search ended with every edge open. Staging's
    /// `KeyRandomizer` never checks it: a partial route is used as it is.
    pub fn all_nodes_visited(&self) -> bool {
        self.all_nodes_visited
    }

    /// The key placed in an item slot.
    pub fn item_contents(&self, item: NodeId) -> Option<KeyId> {
        self.item_to_key[item.index()]
    }

    /// The slots holding `key`, in node order.
    pub fn items_with_key(&self, key: KeyId) -> Vec<NodeId> {
        self.placements()
            .filter(|&(_, k)| k == key)
            .map(|(item, _)| item)
            .collect()
    }

    /// Every slot given a key, in node order.
    pub fn placements(&self) -> impl Iterator<Item = (NodeId, KeyId)> + '_ {
        self.item_to_key
            .iter()
            .enumerate()
            .filter_map(|(i, key)| Some((NodeId::from_index(i), (*key)?)))
    }

    /// How many dead ends the search backed out of.
    pub fn dead_ends(&self) -> usize {
        self.dead_ends
    }

    /// What the finder did, one step per line, indented by fork depth.
    pub fn log(&self) -> &[String] {
        &self.log
    }

    /// Plays the placement through: see [`SolveResult`].
    pub fn solve(&self) -> SolveResult {
        solver::solve(self.graph, &self.item_to_key)
    }
}

/// Finds a key placement for `graph`.
pub fn find<'g>(
    graph: &'g Graph,
    rng: &mut Rng,
    options: RouteFinderOptions<'_>,
) -> Result<Route<'g>, RouteError> {
    let mut finder = Finder {
        graph,
        rng,
        options,
        dead_ends: 0,
    };
    let state = State::new(graph);
    let state = finder.do_subgraph(state, graph.start(), false, 0)?;
    Ok(Route {
        graph,
        all_nodes_visited: state.next.is_empty(),
        item_to_key: state.item_to_key,
        dead_ends: finder.dead_ends,
        log: state.log.lines(),
    })
}

impl Graph {
    /// A key placement drawn from `rng`, with no limits on the search.
    pub fn generate_route(&self, rng: &mut Rng) -> Route<'_> {
        find(self, rng, RouteFinderOptions::default()).expect("a search with no limits finishes")
    }
}

/// Why the finder was stuck at a dead end (common's `DeadEndInfo`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeadEndInfo {
    pub visited_nodes: usize,
    pub spare_items: usize,
    pub placed_keys: usize,
    pub stuck_edges: Vec<StuckEdge>,
    pub key_shortages: Vec<KeyShortage>,
}

/// A locked edge and the keys it still needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StuckEdge {
    pub edge: EdgeId,
    pub missing_keys: Vec<KeyId>,
}

/// A key the stuck edges are missing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyShortage {
    pub key: KeyId,
    /// The stuck edges that need it.
    pub edge_count: usize,
    /// Copies still needed: one for a reusable key, one per missing use
    /// otherwise.
    pub copies_needed: usize,
    /// Item slots in the whole graph that may take it.
    pub compatible_items: usize,
    /// Those slots reached and still empty.
    pub spare_compatible_items: usize,
}

impl KeyShortage {
    /// Whether the graph has fewer slots for the key than it needs copies.
    pub fn is_structurally_impossible(&self) -> bool {
        self.copies_needed > self.compatible_items
    }
}

impl DeadEndInfo {
    /// A report naming the graph's nodes and keys.
    pub fn describe(&self, graph: &Graph) -> String {
        use std::fmt::Write;
        let mut s = String::new();
        let _ = writeln!(
            s,
            "Dead end: {} nodes visited, {} keys placed, {} spare items",
            self.visited_nodes, self.placed_keys, self.spare_items
        );
        if !self.stuck_edges.is_empty() {
            let _ = writeln!(s, "  {} stuck edge(s):", self.stuck_edges.len());
            for stuck in &self.stuck_edges {
                let keys: Vec<String> = stuck
                    .missing_keys
                    .iter()
                    .map(|&k| format!("{} ({:?})", graph.key(k).label, graph.key(k).kind))
                    .collect();
                let edge = graph.edge(stuck.edge);
                let _ = writeln!(
                    s,
                    "    {} --> {}: needs [{}]",
                    graph.node(edge.source).label,
                    graph.node(edge.destination).label,
                    keys.join(", ")
                );
            }
        }
        if !self.key_shortages.is_empty() {
            let _ = writeln!(s, "  Key shortage(s):");
            for shortage in &self.key_shortages {
                let key = graph.key(shortage.key);
                let _ = writeln!(
                    s,
                    "    {} ({:?}): needed by {} edge(s), {} copy needed, {} compatible item(s) in graph, {} currently spare",
                    key.label,
                    key.kind,
                    shortage.edge_count,
                    shortage.copies_needed,
                    shortage.compatible_items,
                    shortage.spare_compatible_items
                );
                if shortage.is_structurally_impossible() {
                    let _ = writeln!(
                        s,
                        "      *** STRUCTURALLY IMPOSSIBLE: not enough compatible items ***"
                    );
                } else if shortage.spare_compatible_items == 0 {
                    let _ = writeln!(
                        s,
                        "      *** NO SPARE COMPATIBLE ITEMS: all compatible items already have keys placed ***"
                    );
                }
            }
        }
        s.trim_end().to_string()
    }
}

/// The finder's log: a shared list, so that cloning a state doesn't copy it.
#[derive(Clone, Default)]
struct Log(Option<Rc<LogLine>>);

struct LogLine {
    text: String,
    previous: Log,
}

impl Log {
    fn push(&mut self, depth: usize, text: String) {
        let previous = std::mem::take(self);
        let text = format!("{}{text}", "  ".repeat(depth));
        *self = Log(Some(Rc::new(LogLine { text, previous })));
    }

    fn lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        let mut at = self.0.as_deref();
        while let Some(line) = at {
            lines.push(line.text.clone());
            at = line.previous.0.as_deref();
        }
        lines.reverse();
        lines
    }
}

impl Drop for Log {
    /// Unlinks the list one line at a time: dropping a long list recursively
    /// would overflow the stack.
    fn drop(&mut self) {
        let mut next = self.0.take();
        while let Some(line) = next {
            next = match Rc::try_unwrap(line) {
                Ok(mut line) => line.previous.0.take(),
                Err(_) => None,
            };
        }
    }
}

/// A point in the search (common's `RouteFinder.State`). Cloned at every
/// choice, so that a failed branch can be dropped.
#[derive(Clone)]
struct State {
    /// The walk this fork split from, joined again when the fork reaches a
    /// node it has visited.
    parent: Option<Rc<State>>,
    /// The frontier: edges taken from a visited node, not yet opened.
    next: BitSet,
    /// One-way and no-return edges found, not yet followed.
    one_way: BitSet,
    /// Item slots reached with no key placed.
    spare_items: BitSet,
    visited: BitSet,
    /// The keys held, by count.
    keys: Vec<u32>,
    item_to_key: Vec<Option<KeyId>>,
    /// How many slots hold each key.
    placed: Vec<u32>,
    log: Log,
}

impl State {
    fn new(graph: &Graph) -> Self {
        Self {
            parent: None,
            next: BitSet::default(),
            one_way: BitSet::default(),
            spare_items: BitSet::default(),
            visited: BitSet::default(),
            keys: vec![0; graph.keys().len()],
            item_to_key: vec![None; graph.nodes().len()],
            placed: vec![0; graph.keys().len()],
            log: Log::default(),
        }
    }

    fn depth(&self) -> usize {
        let mut depth = 0;
        let mut parent = self.parent.as_deref();
        while let Some(p) = parent {
            depth += 1;
            parent = p.parent.as_deref();
        }
        depth
    }

    fn add_log(&mut self, text: String) {
        let depth = self.depth();
        self.log.push(depth, text);
    }

    fn placed_count(&self) -> usize {
        self.placed.iter().map(|&n| n as usize).sum()
    }

    fn visit_node(&mut self, graph: &Graph, node: NodeId) {
        // A fork that reaches its parent's ground joins it.
        let mut parent = self.parent.clone();
        while let Some(p) = parent {
            if p.visited.contains(node.index()) {
                self.join(&p);
                return;
            }
            parent = p.parent.clone();
        }

        self.visited.insert(node.index());
        if graph.node(node).is_item() {
            match self.item_to_key[node.index()] {
                Some(key) => self.keys[key.index()] += 1,
                None => {
                    self.spare_items.insert(node.index());
                }
            }
        }
        for edge in graph.applicable_edges_from(node) {
            let e = graph.edge(edge);
            if !(self.visited.contains(e.source.index())
                && self.visited.contains(e.destination.index()))
            {
                self.next.insert(edge.index());
            }
        }
        self.add_log(format!("Visit node: {}", graph.node_name(node)));
    }

    /// Merges every walk from this fork's parent up to `target` into this
    /// one, which takes `target`'s place.
    fn join(&mut self, target: &Rc<State>) {
        let mut depth = self.depth();
        let mut current = self.parent.clone();
        loop {
            let c = current.expect("the parent to join is an ancestor");
            self.visited.union_with(&c.visited);
            for (held, theirs) in self.keys.iter_mut().zip(&c.keys) {
                *held += theirs;
            }
            self.next.union_with(&c.next);
            self.one_way.union_with(&c.one_way);
            self.spare_items.union_with(&c.spare_items);
            self.log.push(depth, "Join back to parent".to_string());
            debug_assert!(
                self.spare_items
                    .iter()
                    .all(|item| self.item_to_key[item].is_none()),
                "a spare slot holds a key"
            );
            depth = depth.saturating_sub(1);
            if Rc::ptr_eq(&c, target) {
                break;
            }
            current = c.parent.clone();
        }
        self.parent = target.parent.clone();
    }

    fn place_key(&mut self, graph: &Graph, item: NodeId, key: KeyId) {
        debug_assert!(self.item_to_key[item.index()].is_none());
        self.spare_items.remove(item.index());
        self.item_to_key[item.index()] = Some(key);
        self.placed[key.index()] += 1;
        self.keys[key.index()] += 1;
        self.add_log(format!(
            "Place {} at {}",
            graph.key_name(key),
            graph.node_name(item)
        ));
    }

    /// Opens `edge`, using up its consumable keys.
    fn use_edge(&mut self, graph: &Graph, edge: EdgeId) {
        self.next.remove(edge.index());
        for key in graph.edge(edge).required_keys() {
            if graph.key(key).kind == KeyKind::Consumable {
                let count = &mut self.keys[key.index()];
                *count = count.saturating_sub(1);
            }
        }
    }
}

struct Finder<'g, 'r, 'o> {
    graph: &'g Graph,
    rng: &'r mut Rng,
    options: RouteFinderOptions<'o>,
    dead_ends: usize,
}

impl Finder<'_, '_, '_> {
    /// Starts a walk at `start`: a fork of the current one past a one-way
    /// edge, or a fresh one past a no-return edge. Either begins with only
    /// what every way to `start` is sure to have collected.
    fn do_subgraph(
        &mut self,
        mut state: State,
        start: NodeId,
        fork: bool,
        depth: usize,
    ) -> Result<State, RouteError> {
        let graph = self.graph;
        let guaranteed = guaranteed_requirements(graph, &state, start);
        let what = if fork { "fork" } else { "subgraph" };
        state.add_log(format!("Begin {what} {}", graph.node_name(start)));
        if fork {
            state.parent = Some(Rc::new(state.clone()));
        }
        state.visited.clear();
        state.keys.iter_mut().for_each(|n| *n = 0);
        for requirement in guaranteed {
            match requirement {
                Requirement::Key(key) => state.keys[key.index()] = 1,
                Requirement::Node(node) => {
                    state.visited.insert(node.index());
                }
            }
        }
        state.next.clear();
        state.one_way.clear();
        state.spare_items.clear();
        state.visit_node(graph, start);
        self.fulfill(state, depth)
    }

    fn fulfill(&mut self, mut state: State, depth: usize) -> Result<State, RouteError> {
        if self.options.depth_limit.is_some_and(|limit| depth >= limit) {
            return Err(RouteError::DepthLimit);
        }

        self.expand(&mut state);
        // Go down one-way paths first.
        let state = self.follow_one_way_exits(state, depth)?;

        // Choose a locked edge to open.
        for required in self.edges_with_requirements(&state) {
            for _ in 0..RETRIES {
                let Some(slots) = self.find_available_slots(&state, &required) else {
                    continue;
                };
                let mut attempt = state.clone();
                for (&slot, &key) in slots.iter().zip(&required) {
                    attempt.place_key(self.graph, slot, key);
                }
                // Common validates each result with the solver here, and
                // rejects it on `PotentialSoftlock`. Since 1.6.1 the solver
                // never reports that, so the check is left out.
                let result = self.fulfill(attempt, depth + 1)?;
                if result.next.is_empty() && result.one_way.is_empty() {
                    return Ok(result);
                }
            }
        }

        // Common also keeps the attempt with the most keys placed and goes on
        // from it, but only when no locked edge is left, which is when there
        // was nothing to attempt: so it always goes on from `state`.
        if !state.next.is_empty() {
            self.dead_end(&state)?;
            return Ok(state);
        }
        self.follow_no_return_exits(state, depth)
    }

    fn dead_end(&mut self, state: &State) -> Result<(), RouteError> {
        self.dead_ends += 1;
        let limit_reached = self
            .options
            .max_dead_ends
            .is_some_and(|limit| self.dead_ends >= limit);
        if self.options.on_dead_end.is_none() && !limit_reached {
            return Ok(());
        }
        let info = self.dead_end_info(state);
        if let Some(callback) = self.options.on_dead_end.as_mut() {
            callback(&info);
        }
        if limit_reached {
            return Err(RouteError::TooManyDeadEnds(info));
        }
        Ok(())
    }

    /// Takes every frontier edge it can, until none is left that it can.
    fn expand(&mut self, state: &mut State) {
        let graph = self.graph;
        loop {
            let satisfied = self.take_next_nodes(state);
            if satisfied.is_empty() {
                return;
            }
            for edge_id in satisfied {
                let edge = graph.edge(edge_id);
                if state.visited.contains(edge.source.index()) {
                    if state.visited.contains(edge.destination.index()) {
                        continue;
                    }
                    if matches!(edge.kind, EdgeKind::OneWay | EdgeKind::NoReturn) {
                        state.one_way.insert(edge_id.index());
                    } else {
                        state.visit_node(graph, edge.destination);
                    }
                } else {
                    state.visit_node(graph, edge.source);
                }
            }
        }
    }

    /// Opens the frontier edges whose requirements are met, in edge order.
    /// Using up keys only ever closes edges, so one pass finds them all.
    fn take_next_nodes(&mut self, state: &mut State) -> Vec<EdgeId> {
        let mut taken = Vec::new();
        for edge in state
            .next
            .iter()
            .map(EdgeId::from_index)
            .collect::<Vec<_>>()
        {
            if self.is_satisfied(state, edge) {
                taken.push(edge);
                state.use_edge(self.graph, edge);
            }
        }
        taken
    }

    fn is_satisfied(&self, state: &State, edge: EdgeId) -> bool {
        self.missing_keys(&state.keys, edge).is_empty()
            && self
                .graph
                .edge(edge)
                .required_nodes()
                .all(|node| state.visited.contains(node.index()))
    }

    /// The keys `edge` still needs, one entry per copy, given `held`.
    fn missing_keys(&self, held: &[u32], edge: EdgeId) -> Vec<KeyId> {
        let mut missing = Vec::new();
        for (key, count) in key_counts(self.graph, edge) {
            let need = if self.graph.key(key).kind == KeyKind::Removable {
                removable_key_count(self.graph, key, edge)
            } else {
                count as i64
            };
            let short = need - i64::from(held[key.index()]);
            for _ in 0..short.max(0) {
                missing.push(key);
            }
        }
        missing
    }

    /// The keys to place to open `edge`. When those would also open another
    /// frontier edge, that edge's consumable keys are placed too, so that
    /// taking it on the way doesn't use up the copies meant for this one.
    fn required_keys(&self, state: &State, edge: EdgeId) -> Vec<KeyId> {
        let mut required = self.missing_keys(&state.keys, edge);
        let mut with_new = state.keys.clone();
        for key in &required {
            with_new[key.index()] += 1;
        }
        for other in state.next.iter().map(EdgeId::from_index) {
            if other == edge || !self.missing_keys(&with_new, other).is_empty() {
                continue;
            }
            for key in self.missing_keys(&state.keys, other) {
                if self.graph.key(key).kind == KeyKind::Consumable {
                    required.push(key);
                }
            }
        }
        required
    }

    /// The key sets of the locked frontier edges, in the order to try them:
    /// shuffled, then the ones needing fewest reusable keys already placed
    /// elsewhere first.
    fn edges_with_requirements(&mut self, state: &State) -> Vec<Vec<KeyId>> {
        let mut lists: Vec<Vec<KeyId>> = state
            .next
            .iter()
            .map(|e| self.required_keys(state, EdgeId::from_index(e)))
            .filter(|keys| !keys.is_empty())
            .collect();
        self.rng.shuffle(&mut lists);
        let graph = self.graph;
        lists.sort_by_key(|keys| {
            keys.iter()
                .filter(|&&k| {
                    graph.key(k).kind == KeyKind::Reusable && state.placed[k.index()] != 0
                })
                .count()
        });
        lists
    }

    /// A random spare slot for each key that its group allows, or `None` if
    /// some key has none.
    fn find_available_slots(&mut self, state: &State, keys: &[KeyId]) -> Option<Vec<NodeId>> {
        if state.spare_items.len() < keys.len() {
            return None;
        }
        let mut available: Vec<NodeId> = state.spare_items.iter().map(NodeId::from_index).collect();
        self.rng.shuffle(&mut available);
        let mut slots = Vec::with_capacity(keys.len());
        for &key in keys {
            let group = self.graph.key(key).group;
            let at = available
                .iter()
                .position(|&item| self.graph.node(item).group & group == group)?;
            slots.push(available.remove(at));
        }
        Some(slots)
    }

    /// Follows one of the one-way edges found, at random, as a fork.
    fn follow_one_way_exits(
        &mut self,
        mut state: State,
        depth: usize,
    ) -> Result<State, RouteError> {
        let graph = self.graph;
        let mut exits: Vec<EdgeId> = state
            .one_way
            .iter()
            .map(EdgeId::from_index)
            .filter(|&e| graph.edge(e).kind == EdgeKind::OneWay)
            .collect();
        self.rng.shuffle(&mut exits);
        if let Some(&exit) = exits.first() {
            state.one_way.remove(exit.index());
            state = self.do_subgraph(state, graph.edge(exit).destination, true, depth)?;
        }
        Ok(state)
    }

    /// Follows every no-return edge found, in random order, each as a fresh
    /// walk.
    ///
    /// As in common, each walk starts from the state the previous one ended
    /// in, and a fresh walk clears the frontier: with more than one exit, the
    /// result only says whether the last walk was completed.
    fn follow_no_return_exits(
        &mut self,
        mut state: State,
        depth: usize,
    ) -> Result<State, RouteError> {
        let graph = self.graph;
        let mut exits: Vec<EdgeId> = state
            .one_way
            .iter()
            .map(EdgeId::from_index)
            .filter(|&e| graph.edge(e).kind == EdgeKind::NoReturn)
            .collect();
        self.rng.shuffle(&mut exits);
        for exit in exits {
            state.one_way.remove(exit.index());
            state = self.do_subgraph(state, graph.edge(exit).destination, false, depth)?;
        }
        Ok(state)
    }

    fn dead_end_info(&self, state: &State) -> DeadEndInfo {
        let graph = self.graph;
        let mut stuck_edges = Vec::new();
        // Per key: (edges needing it, copies missing in total).
        let mut needs: IndexMap<KeyId, (usize, usize)> = IndexMap::new();
        for edge in state.next.iter().map(EdgeId::from_index) {
            let missing = self.missing_keys(&state.keys, edge);
            let mut counted: Vec<(KeyId, usize)> = Vec::new();
            for &key in &missing {
                match counted.iter_mut().find(|(k, _)| *k == key) {
                    Some((_, n)) => *n += 1,
                    None => counted.push((key, 1)),
                }
            }
            for (key, n) in counted {
                let entry = needs.entry(key).or_insert((0, 0));
                entry.0 += 1;
                entry.1 += n;
            }
            stuck_edges.push(StuckEdge {
                edge,
                missing_keys: missing,
            });
        }
        let fits = |item: NodeId, key: KeyId| {
            let group = graph.key(key).group;
            graph.node(item).group & group == group
        };
        let key_shortages = needs
            .into_iter()
            .map(|(key, (edge_count, copies))| KeyShortage {
                key,
                edge_count,
                copies_needed: if graph.key(key).kind == KeyKind::Reusable {
                    1
                } else {
                    copies
                },
                compatible_items: graph
                    .node_ids()
                    .filter(|&n| graph.node(n).is_item() && fits(n, key))
                    .count(),
                spare_compatible_items: state
                    .spare_items
                    .iter()
                    .filter(|&n| fits(NodeId::from_index(n), key))
                    .count(),
            })
            .collect();
        DeadEndInfo {
            visited_nodes: state.visited.len(),
            spare_items: state.spare_items.len(),
            placed_keys: state.placed_count(),
            stuck_edges,
            key_shortages,
        }
    }
}

/// Each key an edge needs and how many copies, in first-appearance order.
fn key_counts(graph: &Graph, edge: EdgeId) -> Vec<(KeyId, u32)> {
    let mut counts: Vec<(KeyId, u32)> = Vec::new();
    for key in graph.edge(edge).required_keys() {
        match counts.iter_mut().find(|(k, _)| *k == key) {
            Some((_, n)) => *n += 1,
            None => counts.push((key, 1)),
        }
    }
    counts
}

/// The fewest copies of the removable `key` held at once on a way from the
/// start to `edge`'s destination, or -1 if there is no way.
///
/// As in common, the search shares one visited set across all its branches,
/// so it counts the first way it finds into each node, not the best.
fn removable_key_count(graph: &Graph, key: KeyId, edge: EdgeId) -> i64 {
    fn search(graph: &Graph, key: KeyId, node: NodeId, visited: &mut BitSet, count: i64) -> i64 {
        if !visited.insert(node.index()) {
            return -1;
        }
        if node == graph.start() {
            return count;
        }
        let mut best: Option<i64> = None;
        for e in graph.applicable_edges_to(node) {
            let edge = graph.edge(e);
            let uses = edge.required_keys().filter(|&k| k == key).count() as i64;
            let found = search(graph, key, edge.other_end(node), visited, count + uses);
            if found != -1 {
                best = Some(best.map_or(found, |b| b.min(found)));
            }
        }
        best.unwrap_or(-1)
    }
    search(
        graph,
        key,
        graph.edge(edge).destination,
        &mut BitSet::default(),
        0,
    )
}

/// A requirement while working out what a node is sure to need. A soft node
/// is one passed through on the way, rather than one an edge names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Need {
    Key(KeyId),
    Node(NodeId, bool),
}

impl From<Requirement> for Need {
    fn from(requirement: Requirement) -> Self {
        match requirement {
            Requirement::Key(key) => Need::Key(key),
            Requirement::Node(node) => Need::Node(node, false),
        }
    }
}

type Needs = BTreeSet<Need>;

/// What every way to `root` is sure to have collected: the reusable keys
/// and the nodes named by edges along every path, with each key replaced by
/// what is sure to be needed to reach the slots it was placed in (common's
/// `GetGuaranteedRequirements`).
///
/// The searches here share their visited sets across branches as common's
/// do, so like common's they depend on the order they look at edges in.
fn guaranteed_requirements(graph: &Graph, state: &State, root: NodeId) -> BTreeSet<Requirement> {
    let start = graph.start();
    let mut map: Vec<Option<Needs>> = vec![None; graph.nodes().len()];
    map[start.index()] = Some(Needs::from([Need::Node(start, true)]));
    for node in graph.node_ids() {
        let needs = node_needs(graph, &map, node, &mut BitSet::default())
            .unwrap_or_else(|| Needs::from([Need::Node(node, false)]));
        map[node.index()] = Some(needs);
    }
    let map: Vec<Needs> = map.into_iter().map(Option::unwrap_or_default).collect();

    let mut key_map: Vec<Option<Needs>> = vec![None; graph.keys().len()];
    for key in graph.key_ids() {
        let needs = key_needs(graph, state, &map, &key_map, key, &mut BTreeSet::new());
        key_map[key.index()] = Some(needs);
    }
    let key_map: Vec<Needs> = key_map.into_iter().map(Option::unwrap_or_default).collect();

    let mut result = final_needs(&map, &key_map, root, &mut BitSet::default()).unwrap_or_default();
    // The key in any item slot named counts too. (Common indexes the
    // placement and would throw for an empty slot; one is skipped here.)
    let slot_keys: Vec<Need> = result
        .iter()
        .filter_map(|need| match *need {
            Need::Node(node, _) if graph.node(node).is_item() => {
                state.item_to_key[node.index()].map(Need::Key)
            }
            _ => None,
        })
        .collect();
    result.extend(slot_keys);
    result
        .into_iter()
        .filter_map(|need| match need {
            Need::Node(_, true) => None,
            Need::Node(node, false) => Some(Requirement::Node(node)),
            Need::Key(key) if graph.key(key).kind == KeyKind::Reusable => {
                Some(Requirement::Key(key))
            }
            Need::Key(_) => None,
        })
        .collect()
}

/// What every way into `node` needs, intersected over the ways found, or
/// `None` if no way is found.
fn node_needs(
    graph: &Graph,
    map: &[Option<Needs>],
    node: NodeId,
    visited: &mut BitSet,
) -> Option<Needs> {
    if visited.contains(node.index()) {
        return None;
    }
    if let Some(known) = &map[node.index()] {
        return Some(known.clone());
    }
    visited.insert(node.index());
    let mut result: Option<Needs> = None;
    for e in graph.applicable_edges_to(node) {
        let edge = graph.edge(e);
        let Some(mut way) = node_needs(graph, map, edge.other_end(node), visited) else {
            continue;
        };
        way.extend(edge.requires.iter().copied().map(Need::from));
        result = Some(match result {
            None => way,
            Some(so_far) => so_far.intersection(&way).copied().collect(),
        });
    }
    if let Some(result) = &mut result {
        result.insert(Need::Node(node, true));
    }
    result
}

/// What reaching every slot `key` was placed in is sure to need, with the
/// keys among it expanded the same way.
fn key_needs(
    graph: &Graph,
    state: &State,
    map: &[Needs],
    key_map: &[Option<Needs>],
    key: KeyId,
    visited: &mut BTreeSet<KeyId>,
) -> Needs {
    if let Some(known) = &key_map[key.index()] {
        return known.clone();
    }
    if !visited.insert(key) {
        return Needs::new();
    }
    let mut result: Option<Needs> = None;
    for item in graph.node_ids() {
        if state.item_to_key[item.index()] != Some(key) {
            continue;
        }
        let mut item_needs = Needs::new();
        for &need in &map[item.index()] {
            match need {
                Need::Key(k) => {
                    item_needs.extend(key_needs(graph, state, map, key_map, k, visited))
                }
                other => {
                    item_needs.insert(other);
                }
            }
        }
        result = Some(match result {
            None => item_needs,
            Some(so_far) => so_far.intersection(&item_needs).copied().collect(),
        });
    }
    result.unwrap_or_default()
}

/// `node`'s needs with every key and named node expanded into what it in
/// turn needs.
fn final_needs(
    map: &[Needs],
    key_map: &[Needs],
    node: NodeId,
    visited: &mut BitSet,
) -> Option<Needs> {
    if !visited.insert(node.index()) {
        return None;
    }
    let mut result = Needs::new();
    for &need in &map[node.index()] {
        match need {
            Need::Key(key) => result.extend(key_map[key.index()].iter().copied()),
            Need::Node(other, soft) => {
                if let Some(sub) = final_needs(map, key_map, other, visited) {
                    result.extend(sub);
                }
                if !soft {
                    result.insert(need);
                }
            }
        }
    }
    Some(result)
}
