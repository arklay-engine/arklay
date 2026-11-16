//! Key placement through routing: a port of biorand/common's `Routing`
//! (release 1.6.1).
//!
//! A [`Graph`] holds the places the player can be ([`Node`]s: rooms, flags
//! and item slots), the [`Edge`]s between them and the [`Key`]s that open
//! edges. [`find`] places keys in item slots so that the player can open
//! every edge, drawing its choices from a [`RouteRng`]. The result is a
//! [`Route`]; [`Route::solve`] plays it through as a check.
//!
//! The port keeps common's algorithm, including where it depends on the order
//! it looks at things in. Its `HashSet` orders are replaced by id order, so a
//! seed always gives the same route, though not the route common gives.
//!
//! As in the C#, where the routing is part of BioRand.Common, this crate knows
//! nothing of any game: building the graph from the RE1 map is
//! `arklay_biorand::keys`.

mod bits;
mod finder;
mod graph;
mod rng;
mod solver;
#[cfg(test)]
mod tests;

// The tests' seeded stream (`tests/common`) names this crate as the
// integration tests see it.
#[cfg(test)]
extern crate self as arklay_routing;
#[cfg(test)]
#[path = "../tests/common/mod.rs"]
mod seeded;

pub use finder::{
    DeadEndInfo, KeyShortage, Route, RouteError, RouteFinderOptions, StuckEdge, find,
};
pub use graph::{
    DependencyGraphBuilder, Edge, EdgeId, EdgeKind, Graph, GraphBuilder, GraphError, Key, KeyId,
    KeyKind, Node, NodeId, NodeKind, Requirement,
};
pub use rng::RouteRng;
pub use solver::SolveResult;
