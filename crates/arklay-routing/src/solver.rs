//! Checks a key placement by playing it through greedily (common's
//! `RouteSolver`, with the two-way fix of 1.6.1).

use super::bits::BitSet;
use super::graph::{EdgeId, Graph, KeyId, KeyKind, NodeId};

/// What a play-through of a placement found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SolveResult {
    /// Every edge the player came across was opened.
    Ok,
    /// Some edge could not be opened: a node may be out of reach.
    ///
    /// Common's flags also have `PotentialSoftlock`, but since 1.6.1 the
    /// solver opens consumable edges one at a time and never reports it, so
    /// it is left out here.
    NodesRemaining,
}

/// Plays `item_to_key` through from the start: walk every edge that needs
/// nothing, then open every edge whose reusable or removable keys are held,
/// and when only edges using up consumable keys remain, open the first of
/// them and start again.
pub(crate) fn solve(graph: &Graph, item_to_key: &[Option<KeyId>]) -> SolveResult {
    let mut state = Play {
        graph,
        item_to_key,
        visited: BitSet::default(),
        next: BitSet::default(),
        keys: vec![0; graph.keys().len()],
    };
    let start = graph.start();
    state.visited.insert(start.index());
    for edge in graph.applicable_edges_from(start) {
        state.next.insert(edge.index());
    }
    loop {
        state.expand();
        if !state.use_key() {
            break;
        }
    }
    if state.next.is_empty() {
        SolveResult::Ok
    } else {
        SolveResult::NodesRemaining
    }
}

struct Play<'a> {
    graph: &'a Graph,
    item_to_key: &'a [Option<KeyId>],
    visited: BitSet,
    /// Edges with an unvisited end, or not yet opened.
    next: BitSet,
    keys: Vec<u32>,
}

impl Play<'_> {
    fn edge_ids(&self) -> Vec<EdgeId> {
        self.next.iter().map(EdgeId::from_index).collect()
    }

    fn nodes_visited(&self, edge: EdgeId) -> bool {
        self.graph
            .edge(edge)
            .required_nodes()
            .all(|node| self.visited.contains(node.index()))
    }

    /// Walks every edge that needs no key, until there are none.
    fn expand(&mut self) {
        loop {
            let free: Vec<EdgeId> = self
                .edge_ids()
                .into_iter()
                .filter(|&e| {
                    self.nodes_visited(e) && self.graph.edge(e).required_keys().next().is_none()
                })
                .collect();
            if free.is_empty() {
                return;
            }
            self.visit(&free);
        }
    }

    /// Opens what the keys held allow. Returns whether anything opened.
    fn use_key(&mut self) -> bool {
        // Common only checks the keys here, so an edge that needs no key but
        // an unvisited node would be walked through. It also needs its nodes
        // here, as in `expand`.
        let possible: Vec<EdgeId> = self
            .edge_ids()
            .into_iter()
            .filter(|&e| self.has_all_keys(e) && self.nodes_visited(e))
            .collect();
        let safe: Vec<EdgeId> = possible
            .iter()
            .copied()
            .filter(|&e| {
                self.graph
                    .edge(e)
                    .required_keys()
                    .all(|key| self.graph.key(key).kind != KeyKind::Consumable)
            })
            .collect();
        if !safe.is_empty() {
            for edge in safe {
                self.visit(&[edge]);
            }
            return true;
        }
        // One consumable edge at a time, so that keys found behind it can pay
        // for the next.
        let Some(&edge) = possible.first() else {
            return false;
        };
        for key in self.graph.edge(edge).required_keys() {
            if self.graph.key(key).kind == KeyKind::Consumable {
                let count = &mut self.keys[key.index()];
                *count = count.saturating_sub(1);
            }
        }
        self.visit(&[edge]);
        true
    }

    fn has_all_keys(&self, edge: EdgeId) -> bool {
        let mut need = Vec::<(KeyId, u32)>::new();
        for key in self.graph.edge(edge).required_keys() {
            match need.iter_mut().find(|(k, _)| *k == key) {
                Some((_, n)) => *n += 1,
                None => need.push((key, 1)),
            }
        }
        need.iter().all(|&(key, n)| self.keys[key.index()] >= n)
    }

    fn visit(&mut self, edges: &[EdgeId]) {
        let ends = edges
            .iter()
            .map(|&e| self.graph.edge(e).destination)
            .chain(edges.iter().map(|&e| self.graph.edge(e).source));
        let mut new_nodes: Vec<NodeId> = Vec::new();
        for node in ends {
            if self.visited.insert(node.index()) {
                new_nodes.push(node);
            }
        }
        for &edge in edges {
            self.next.remove(edge.index());
        }
        for &node in &new_nodes {
            for edge in self.graph.applicable_edges_from(node) {
                let e = self.graph.edge(edge);
                if !self.visited.contains(e.source.index())
                    || !self.visited.contains(e.destination.index())
                {
                    self.next.insert(edge.index());
                }
            }
            if let Some(key) = self.item_to_key[node.index()] {
                self.keys[key.index()] += 1;
            }
        }
    }
}
