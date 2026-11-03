//! Common's routing tests (`TestRouting.cs` and `TestGraphBuilder.cs`).
//!
//! Common runs most of them over seeds 0 to 99 of .NET's `Random`; these run
//! over seeds 0 to 99 of [`Rng`]. The few common runs once with an unseeded
//! `Random` run over the same seeds here too, as they assert what every seed
//! must give.

use super::*;
use crate::rng::Rng;

const SEEDS: u64 = 100;

fn route(graph: &Graph, seed: u64) -> Route<'_> {
    graph.generate_route(&mut Rng::new(seed))
}

fn k(key: KeyId) -> Requirement {
    key.into()
}

fn n(node: NodeId) -> Requirement {
    node.into()
}

/// The slot holds one of `expected` (`None` for nothing).
fn assert_item(route: &Route, item: NodeId, expected: &[Option<KeyId>]) {
    let actual = route.item_contents(item);
    assert!(
        expected.contains(&actual),
        "expected {} to be one of {expected:?} but was {actual:?}\n{}",
        route.graph().node_name(item),
        route.log().join("\n")
    );
}

/// `key` is placed exactly once, in one of `expected`.
fn assert_key_once(route: &Route, key: KeyId, expected: &[NodeId]) {
    let items = route.items_with_key(key);
    for item in &items {
        assert!(
            expected.contains(item),
            "expected {} to be at one of {expected:?} but was at {}\n{}",
            route.graph().key_name(key),
            route.graph().node_name(*item),
            route.log().join("\n")
        );
    }
    assert_eq!(
        items.len(),
        1,
        "expected {} to be placed once\n{}",
        route.graph().key_name(key),
        route.log().join("\n")
    );
}

fn assert_key_quantity(route: &Route, key: KeyId, count: usize) {
    assert_eq!(
        route.items_with_key(key).len(),
        count,
        "copies of {}\n{}",
        route.graph().key_name(key),
        route.log().join("\n")
    );
}

/// An OR gate: two ways into a room.
#[test]
fn alt_ways_in_same_room() {
    let mut b = DependencyGraphBuilder::new();
    let room0 = b.root("ROOM 0");
    let room1 = b.and_gate("ROOM 1", room0, &[]);
    let room2 = b.and_gate("ROOM 2", room0, &[]);
    b.or_gate("ROOM 3", &[room1, room2]);
    let graph = b.build();
    for seed in 0..SEEDS {
        assert!(route(&graph, seed).all_nodes_visited());
    }
}

/// Three rooms with items and keys.
#[test]
fn basic() {
    let mut b = DependencyGraphBuilder::new();
    let key0 = b.reusable_key(1, "KEY 0");
    let key1 = b.reusable_key(1, "KEY 1");
    let room0 = b.root("ROOM 0");
    let item0a = b.item(1, "ITEM 0.A", room0, &[]);
    let item0b = b.item(1, "ITEM 0.B", room0, &[]);
    let room1 = b.and_gate("ROOM 1", room0, &[k(key0)]);
    let item1a = b.item(1, "ITEM 1.A", room1, &[]);
    b.and_gate("ROOM 2", room1, &[k(key1)]);
    let graph = b.build();
    for seed in 0..SEEDS {
        let route = route(&graph, seed);
        assert_key_once(&route, key0, &[item0a, item0b]);
        assert_key_once(&route, key1, &[item0a, item0b, item1a]);
        assert_eq!(route.solve(), SolveResult::Ok);
    }
}

/// A door that can only be opened from one side.
#[test]
fn blocked_door() {
    let mut b = GraphBuilder::new();
    let key0 = b.key("KEY 0", 1, KeyKind::Reusable);
    let key1 = b.key("KEY 1", 1, KeyKind::Reusable);
    let room0 = b.room("ROOM 0");
    let room1 = b.room("ROOM 1");
    let room2 = b.room("ROOM 2");
    let room3 = b.room("ROOM 3");
    let item1 = b.item("ITEM 1", 1, room1, &[]);
    let item2 = b.item("ITEM 2", 1, room2, &[]);
    b.blocked_door(room0, room1, &[k(key0)]).unwrap();
    b.blocked_door(room1, room2, &[]).unwrap();
    b.door(room1, room3, &[k(key1)]).unwrap();
    b.door(room2, room0, &[]).unwrap();
    let graph = b.build().unwrap();
    for seed in 0..SEEDS {
        let route = route(&graph, seed);
        assert_key_once(&route, key0, &[item2]);
        assert_key_once(&route, key1, &[item1]);
        assert_eq!(route.solve(), SolveResult::Ok);
    }
}

#[test]
fn key_behind_key() {
    let mut b = DependencyGraphBuilder::new();
    let key0 = b.reusable_key(1, "KEY 0");
    let key1 = b.reusable_key(1, "KEY 1");
    let room0 = b.root("ROOM 0");
    let item0a = b.item(1, "ITEM 0.A", room0, &[]);
    let item0b = b.item(1, "ITEM 0.B", room0, &[]);
    let item0c = b.item(1, "ITEM 0.C", room0, &[k(key0)]);
    b.and_gate("ROOM 1", room0, &[k(key0), k(key1)]);
    let graph = b.build();
    for seed in 0..SEEDS {
        let route = route(&graph, seed);
        assert_key_once(&route, key0, &[item0a, item0b]);
        assert_key_once(&route, key1, &[item0a, item0b, item0c]);
        assert!(route.all_nodes_visited());
    }
}

/// The keys must be placed in a certain order, or the reachable slots run
/// out.
#[test]
fn key_order_matters() {
    let mut b = DependencyGraphBuilder::new();
    let key0 = b.reusable_key(1, "KEY 0");
    let key1 = b.reusable_key(1, "KEY 1");
    let room0 = b.root("ROOM 0");
    let item0a = b.item(1, "ITEM 0.A", room0, &[]);
    let item0b = b.item(1, "ITEM 0.B", room0, &[k(key0)]);
    b.and_gate("ROOM 1", room0, &[k(key0), k(key1)]);
    let graph = b.build();
    for seed in 0..SEEDS {
        let route = route(&graph, seed);
        assert_key_once(&route, key0, &[item0a]);
        assert_key_once(&route, key1, &[item0b]);
        assert!(route.all_nodes_visited());
    }
}

/// Placing a key doesn't add the edges it would open to the frontier, so
/// keys aren't placed before they are needed: here there are too few slots.
#[test]
fn no_key() {
    let mut b = DependencyGraphBuilder::new();
    let key0 = b.reusable_key(1, "KEY 0");
    let key1 = b.reusable_key(1, "KEY 1");
    let key2 = b.reusable_key(1, "KEY 2");
    let room0 = b.root("ROOM 0");
    b.item(1, "ITEM 0.A", room0, &[]);
    b.item(1, "ITEM 0.B", room0, &[]);
    let room1 = b.and_gate("ROOM 1", room0, &[k(key0)]);
    let room2 = b.and_gate("ROOM 2", room1, &[k(key1)]);
    b.and_gate("ROOM 3", room2, &[k(key0), k(key2)]);
    let graph = b.build();
    for seed in 0..SEEDS {
        assert!(!route(&graph, seed).all_nodes_visited());
    }
}

/// Two segments: the key is placed in both, so a player who skipped it in
/// the first isn't stuck in the second.
#[test]
fn ensure_key_placed_again() {
    let mut b = DependencyGraphBuilder::new();
    let key0 = b.reusable_key(1, "KEY 0");
    let room0 = b.root("ROOM 0");
    let item0a = b.item(1, "ITEM 0.A", room0, &[]);
    b.and_gate("ROOM 1", room0, &[k(key0)]);
    let room2 = b.no_return("ROOM 2", room0, &[]);
    let item2a = b.item(1, "ITEM 2.A", room2, &[]);
    b.and_gate("ROOM 3", room2, &[k(key0)]);
    let graph = b.build();
    for seed in 0..SEEDS {
        let route = route(&graph, seed);
        assert_item(&route, item0a, &[Some(key0)]);
        assert_item(&route, item2a, &[Some(key0)]);
        assert!(route.all_nodes_visited());
    }
}

/// Two segments, where the key is needed to reach the second, so it is
/// placed once.
#[test]
fn ensure_key_not_placed_again() {
    let mut b = DependencyGraphBuilder::new();
    let key0 = b.reusable_key(1, "KEY 0");
    let room0 = b.root("ROOM 0");
    let item0a = b.item(1, "ITEM 0.A", room0, &[]);
    let room1 = b.no_return("ROOM 1", room0, &[k(key0)]);
    let item1a = b.item(1, "ITEM 1.A", room1, &[]);
    b.and_gate("ROOM 2", room1, &[k(key0)]);
    let graph = b.build();
    for seed in 0..SEEDS {
        let route = route(&graph, seed);
        assert_item(&route, item0a, &[Some(key0)]);
        assert_item(&route, item1a, &[None]);
        assert!(route.all_nodes_visited());
    }
}

/// Two segments, where the key to another key is placed once.
#[test]
fn ensure_nested_keys_not_placed_again() {
    let mut b = DependencyGraphBuilder::new();
    let key0 = b.reusable_key(1, "KEY 0");
    let key1 = b.reusable_key(1, "KEY 1");
    let room0 = b.root("ROOM 0");
    let item0a = b.item(1, "ITEM 0.A", room0, &[]);
    let room1 = b.and_gate("ROOM 1", room0, &[]);
    let item1a = b.item(1, "ITEM 1.A", room1, &[k(key0)]);
    let room2 = b.no_return("ROOM 2", room1, &[k(key1)]);
    b.item(1, "ITEM 2.A", room2, &[]);
    b.item(1, "ITEM 2.B", room2, &[]);
    let room3 = b.and_gate("ROOM 3", room2, &[k(key0)]);
    b.and_gate("ROOM 4", room3, &[k(key1)]);
    let graph = b.build();
    for seed in 0..SEEDS {
        let route = route(&graph, seed);
        assert_key_once(&route, key0, &[item0a]);
        assert_key_once(&route, key1, &[item1a]);
        assert!(route.all_nodes_visited());
    }
}

/// Two segments, where key 1 is placed again because there are two ways to
/// the second segment's door, only one through key 1's door.
#[test]
fn ensure_semi_nested_keys() {
    let mut b = DependencyGraphBuilder::new();
    let key0 = b.reusable_key(1, "KEY 0");
    let key1 = b.reusable_key(1, "KEY 1");
    let room0 = b.root("ROOM 0");
    let item0a = b.item(1, "ITEM 0.A", room0, &[]);
    let room1 = b.and_gate("ROOM 1", room0, &[k(key1)]);
    let item1a = b.item(1, "ITEM 1.A", room1, &[]);
    let room2 = b.or_gate("ROOM 2", &[room0, room1]);
    let room3 = b.no_return("ROOM 3", room2, &[k(key0)]);
    let item3a = b.item(1, "ITEM 3.A", room3, &[]);
    let item3b = b.item(1, "ITEM 3.B", room3, &[]);
    b.and_gate("ROOM 5", room3, &[k(key0), k(key1)]);
    let graph = b.build();
    for seed in 0..SEEDS {
        let route = route(&graph, seed);
        assert_key_once(&route, key0, &[item0a, item1a]);
        assert_key_quantity(&route, key1, 2);
        assert_item(&route, item3a, &[Some(key1), None]);
        assert_item(&route, item3b, &[Some(key1), None]);
        assert!(route.all_nodes_visited());
    }
}

/// A key needed to reach a node the no-return edge requires is placed once.
#[test]
fn ensure_key_to_required_node_not_placed_again() {
    let mut b = DependencyGraphBuilder::new();
    let key0 = b.reusable_key(1, "KEY 0");
    let room0 = b.root("ROOM 0");
    let item0a = b.item(1, "ITEM 0.A", room0, &[]);
    let room1 = b.and_gate("ROOM 1", room0, &[]);
    let room2 = b.and_gate("ROOM 2", room1, &[k(key0)]);
    let room3 = b.no_return("ROOM 3", room1, &[n(room2)]);
    b.item(1, "ITEM 3.A", room3, &[]);
    b.and_gate("ROOM 4", room3, &[k(key0)]);
    let graph = b.build();
    for seed in 0..SEEDS {
        let route = route(&graph, seed);
        assert_key_once(&route, key0, &[item0a]);
        assert!(route.all_nodes_visited());
    }
}

#[test]
fn basic_one_way() {
    let mut b = GraphBuilder::new();
    let room0 = b.room("ROOM 0");
    let room1 = b.room("ROOM 1");
    let room2 = b.room("ROOM 2");
    b.door(room0, room1, &[]).unwrap();
    b.door(room1, room2, &[]).unwrap();
    b.one_way(room0, room2, &[]).unwrap();
    let graph = b.build().unwrap();
    for seed in 0..SEEDS {
        assert!(route(&graph, seed).all_nodes_visited());
    }
}

/// A mini segment that leads back to the main one.
#[test]
fn mini_segment() {
    let mut b = GraphBuilder::new();
    let key0 = b.key("KEY 0", 1, KeyKind::Reusable);
    let key1 = b.key("KEY 1", 1, KeyKind::Reusable);
    let room1 = b.room("ROOM 1");
    let room2 = b.room("ROOM 2");
    let room3 = b.room("ROOM 3");
    let room4 = b.room("ROOM 4");
    let room5 = b.room("ROOM 5");
    let room6 = b.room("ROOM 6");
    let item2 = b.item("Item 2", 1, room2, &[]);
    let item3 = b.item("Item 3", 1, room3, &[]);
    b.door(room1, room2, &[]).unwrap();
    b.door(room2, room5, &[]).unwrap();
    b.door(room3, room4, &[k(key1)]).unwrap();
    b.door(room3, room6, &[k(key0)]).unwrap();
    b.blocked_door(room4, room5, &[]).unwrap();
    b.one_way(room1, room3, &[]).unwrap();
    let graph = b.build().unwrap();
    for seed in 0..SEEDS {
        let route = route(&graph, seed);
        assert!(route.all_nodes_visited());
        assert_key_once(&route, key0, &[item2]);
        assert_key_once(&route, key1, &[item3]);
    }
}

/// Two segments the player can go between. Common skips this one as
/// failing (`[Fact(Skip = "Failing")]`); it passes for every seed here.
#[test]
fn circular_segments() {
    let mut b = DependencyGraphBuilder::new();
    let key0 = b.reusable_key(1, "KEY 0");
    let key1 = b.reusable_key(2, "KEY 1");
    let room0 = b.root("ROOM 0");
    let room1 = b.one_way("ROOM 1", room0, &[]);
    let room2 = b.and_gate("ROOM 2", room1, &[]);
    let room3 = b.and_gate("ROOM 3", room0, &[]);
    let room4 = b.and_gate("ROOM 4", room2, &[]);
    let back = b.one_way("", room4, &[]);
    b.or_gate("ROOM 5", &[room3, back]);
    b.and_gate("ROOM 6", room2, &[k(key1)]);
    b.and_gate("ROOM 7", room3, &[k(key0)]);
    let item2 = b.item(1, "Item 2", room2, &[]);
    let item3 = b.item(2, "Item 3", room3, &[]);
    let graph = b.build();
    for seed in 0..SEEDS {
        let route = route(&graph, seed);
        assert!(route.all_nodes_visited());
        assert_key_once(&route, key0, &[item2]);
        assert_key_once(&route, key1, &[item3]);
    }
}

/// A mini segment done once and never returned to.
#[test]
fn single_time_mini_segment() {
    let mut b = GraphBuilder::new();
    let key0 = b.key("KEY 0", 1, KeyKind::Reusable);
    let key1 = b.key("KEY 1", 1, KeyKind::Reusable);
    let room0 = b.room("ROOM 0");
    let room1 = b.room("ROOM 1");
    let room2 = b.room("ROOM 2");
    let room3 = b.room("ROOM 3");
    let room4 = b.room("ROOM 4");
    let room5 = b.room("ROOM 5");
    let item0 = b.item("ITEM 0", 1, room0, &[]);
    let item2 = b.item("ITEM 2", 1, room2, &[]);
    b.item("ITEM 3", 1, room3, &[k(key1)]);
    b.door(room0, room1, &[]).unwrap();
    b.door(room1, room4, &[]).unwrap();
    b.door(room4, room5, &[k(key0)]).unwrap();
    b.one_way(room1, room2, &[]).unwrap();
    b.door(room2, room3, &[]).unwrap();
    b.no_return(room3, room4, &[]).unwrap();
    let graph = b.build().unwrap();
    for seed in 0..SEEDS {
        let route = route(&graph, seed);
        assert!(route.all_nodes_visited());
        assert_key_once(&route, key0, &[item0]);
        assert_key_once(&route, key1, &[item2]);
    }
}

/// A one-way edge into an area with a door needing a key the main area
/// already has: the key isn't placed twice.
#[test]
fn one_way_key_not_placed_twice() {
    let mut b = GraphBuilder::new();
    let key0 = b.key("KEY 0", 1, KeyKind::Reusable);
    let key1 = b.key("KEY 1", 1, KeyKind::Reusable);
    let key2 = b.key("KEY 2", 1, KeyKind::Reusable);
    let room0 = b.room("ROOM 0");
    let room1 = b.room("ROOM 1");
    let room2 = b.room("ROOM 2");
    let room3 = b.room("ROOM 3");
    let room4 = b.room("ROOM 4");
    let room5 = b.room("ROOM 5");
    let item0 = b.item("ITEM 0", 1, room0, &[]);
    let item1 = b.item("ITEM 1", 1, room1, &[]);
    let item2 = b.item("ITEM 2", 1, room2, &[]);
    let item4 = b.item("ITEM 4", 1, room4, &[]);
    b.door(room0, room1, &[]).unwrap();
    b.one_way(room1, room2, &[k(key1)]).unwrap();
    b.door(room1, room5, &[k(key0)]).unwrap();
    b.door(room2, room4, &[]).unwrap();
    b.blocked_door(room4, room1, &[k(key2)]).unwrap();
    b.door(room2, room3, &[k(key0)]).unwrap();
    let graph = b.build().unwrap();
    for seed in 0..SEEDS {
        let route = route(&graph, seed);
        assert!(route.all_nodes_visited());
        assert_key_once(&route, key0, &[item0, item1, item2, item4]);
    }
}

#[test]
fn single_use_key_door_after_door() {
    let mut b = DependencyGraphBuilder::new();
    let key0 = b.consumable_key(1, "KEY 0");
    let room0 = b.root("ROOM 0");
    let item0 = b.item(1, "ITEM 0", room0, &[]);
    let room1 = b.and_gate("ROOM 1", room0, &[k(key0)]);
    let item1 = b.item(1, "ITEM 1", room1, &[]);
    b.and_gate("ROOM 2", room1, &[k(key0)]);
    let graph = b.build();
    for seed in 0..SEEDS {
        let route = route(&graph, seed);
        assert_item(&route, item0, &[Some(key0)]);
        assert_item(&route, item1, &[Some(key0)]);
        assert!(route.all_nodes_visited());
    }
}

#[test]
fn single_use_key_two_doors() {
    let mut b = DependencyGraphBuilder::new();
    let key0 = b.consumable_key(1, "KEY 0");
    let room0 = b.root("ROOM 0");
    let item0 = b.item(1, "ITEM 0", room0, &[]);
    let item1 = b.item(1, "ITEM 1", room0, &[]);
    b.and_gate("ROOM 1", room0, &[k(key0)]);
    b.and_gate("ROOM 2", room0, &[k(key0)]);
    let graph = b.build();
    for seed in 0..SEEDS {
        let route = route(&graph, seed);
        assert_item(&route, item0, &[Some(key0)]);
        assert_item(&route, item1, &[Some(key0)]);
        assert!(route.all_nodes_visited());
    }
}

#[test]
fn single_use_key_two_doors_no_possible_softlock() {
    let mut b = DependencyGraphBuilder::new();
    let key0 = b.consumable_key(1, "KEY 0");
    let room0 = b.root("ROOM 0");
    let item0a = b.item(1, "ITEM 0A", room0, &[]);
    let item0b = b.item(1, "ITEM 0B", room0, &[]);
    let room1 = b.and_gate("ROOM 1", room0, &[k(key0)]);
    let item1 = b.item(1, "ITEM 1", room1, &[]);
    b.and_gate("ROOM 2", room0, &[k(key0)]);
    let graph = b.build();
    for seed in 0..SEEDS {
        let route = route(&graph, seed);
        assert_item(&route, item0a, &[Some(key0)]);
        assert_item(&route, item0b, &[Some(key0)]);
        assert_item(&route, item1, &[None]);
        assert!(route.all_nodes_visited());
    }
}

#[test]
fn single_use_key_route_order_matters() {
    let mut b = DependencyGraphBuilder::new();
    let key0 = b.consumable_key(1, "KEY 0");
    let key1 = b.reusable_key(1, "KEY 1");
    let room0 = b.root("ROOM 0");
    let item0 = b.item(1, "ITEM 0", room0, &[]);
    let room1 = b.and_gate("ROOM 1", room0, &[k(key1)]);
    b.and_gate("ROOM 2", room1, &[k(key0)]);
    let room3 = b.and_gate("ROOM 3", room0, &[k(key0)]);
    let item3a = b.item(1, "ITEM 3.A", room3, &[]);
    let item3b = b.item(1, "ITEM 3.B", room3, &[]);
    let graph = b.build();
    for seed in 0..SEEDS {
        let route = route(&graph, seed);
        assert_item(&route, item0, &[Some(key0)]);
        assert_key_once(&route, key1, &[item3a, item3b]);
        assert_key_quantity(&route, key0, 2);
        assert!(route.all_nodes_visited());
    }
}

#[test]
fn single_use_key_route_order_matters_flexible() {
    let mut b = DependencyGraphBuilder::new();
    let key0 = b.consumable_key(1, "KEY 0");
    let key1 = b.reusable_key(1, "KEY 1");
    let room0 = b.root("ROOM 0");
    let item0a = b.item(1, "ITEM 0.A", room0, &[]);
    let item0b = b.item(1, "ITEM 0.B", room0, &[]);
    let room1 = b.and_gate("ROOM 1", room0, &[k(key1)]);
    b.and_gate("ROOM 2", room1, &[k(key0)]);
    let room3 = b.and_gate("ROOM 3", room0, &[k(key0)]);
    let item3a = b.item(1, "ITEM 3.A", room3, &[]);
    let item3b = b.item(1, "ITEM 3.B", room3, &[]);
    let graph = b.build();
    for seed in 0..SEEDS {
        let route = route(&graph, seed);
        assert!(route.all_nodes_visited());
        assert_key_once(&route, key1, &[item0a, item0b, item3a, item3b]);
    }
}

#[test]
fn single_use_key_two_one_way_doors() {
    let mut b = DependencyGraphBuilder::new();
    let key0 = b.consumable_key(1, "KEY 0");
    let room0 = b.root("ROOM 0");
    let item0 = b.item(1, "ITEM 0", room0, &[]);
    let item1 = b.item(1, "ITEM 1", room0, &[]);
    b.one_way("ROOM 1", room0, &[k(key0)]);
    b.one_way("ROOM 2", room0, &[k(key0)]);
    let graph = b.build();
    for seed in 0..SEEDS {
        let route = route(&graph, seed);
        assert_item(&route, item0, &[Some(key0)]);
        assert_item(&route, item1, &[Some(key0)]);
        assert!(route.all_nodes_visited());
    }
}

#[test]
fn single_use_key_two_key_door() {
    let mut b = DependencyGraphBuilder::new();
    let key0 = b.consumable_key(1, "KEY 0");
    let key1 = b.reusable_key(2, "KEY 1");
    let room0 = b.root("ROOM 0");
    let item0 = b.item(1, "ITEM 0", room0, &[]);
    let room1 = b.and_gate("ROOM 1", room0, &[k(key0)]);
    let item1a = b.item(1, "ITEM 1A", room1, &[]);
    let item1b = b.item(2, "ITEM 1B", room1, &[]);
    b.and_gate("ROOM 2", room0, &[k(key0), k(key1)]);
    let graph = b.build();
    for seed in 0..SEEDS {
        let route = route(&graph, seed);
        assert_item(&route, item0, &[Some(key0)]);
        assert_item(&route, item1a, &[Some(key0)]);
        assert_item(&route, item1b, &[Some(key1)]);
        assert!(route.all_nodes_visited());
    }
}

#[test]
fn single_use_key_key_order_matters() {
    let mut b = DependencyGraphBuilder::new();
    let key0 = b.consumable_key(1, "KEY 0");
    let key1 = b.reusable_key(1, "KEY 1");
    let key2 = b.reusable_key(1, "KEY 2");
    let room0 = b.root("ROOM 0");
    b.item(1, "ITEM 0.A", room0, &[]);
    b.item(1, "ITEM 0.B", room0, &[]);
    let room1 = b.and_gate("ROOM 1", room0, &[k(key0)]);
    b.item(1, "ITEM 1.A", room1, &[]);
    let room2 = b.and_gate("ROOM 2", room0, &[k(key1)]);
    b.item(1, "ITEM 2.A", room2, &[]);
    let room3 = b.and_gate("ROOM 3", room2, &[k(key2)]);
    b.and_gate("ROOM 4", room3, &[k(key0)]);
    let graph = b.build();
    for seed in 0..SEEDS {
        assert!(route(&graph, seed).all_nodes_visited());
    }
}

/// Validating a route before all keys were placed after expanding made the
/// finder back up too far. Common checks seed 0 reaches no dead end; that
/// depends on its random stream, so here every seed must finish.
#[test]
fn single_use_key_validation_edge_case() {
    let mut b = DependencyGraphBuilder::new();
    let key0 = b.consumable_key(1, "KEY 0");
    let key1 = b.reusable_key(1, "KEY 1");
    let room0 = b.root("ROOM 0");
    let item0 = b.item(1, "ITEM 0", room0, &[]);
    let item1 = b.item(1, "ITEM 1", room0, &[]);
    let item2 = b.item(1, "ITEM 2", room0, &[]);
    b.and_gate("ROOM 1", room0, &[k(key0)]);
    let room2 = b.and_gate("ROOM 2", room0, &[k(key1)]);
    b.and_gate("ROOM 3", room2, &[k(key0)]);
    let graph = b.build();
    let mut without_dead_end = 0;
    for seed in 0..SEEDS {
        let route = route(&graph, seed);
        for item in [item0, item1, item2] {
            assert_item(&route, item, &[Some(key0), Some(key1)]);
        }
        assert!(route.all_nodes_visited());
        if route.dead_ends() == 0 {
            without_dead_end += 1;
        }
    }
    assert!(
        without_dead_end > 0,
        "some seed finds the route straight away"
    );
}

#[test]
fn key_2x_required() {
    let mut b = DependencyGraphBuilder::new();
    let key0 = b.reusable_key(1, "KEY 0");
    let room0 = b.root("ROOM 0");
    let item0a = b.item(1, "ITEM 0.A", room0, &[]);
    let item0b = b.item(1, "ITEM 0.B", room0, &[]);
    b.and_gate("ROOM 1", room0, &[k(key0), k(key0)]);
    let graph = b.build();
    for seed in 0..SEEDS {
        let route = route(&graph, seed);
        assert_item(&route, item0a, &[Some(key0)]);
        assert_item(&route, item0b, &[Some(key0)]);
        assert!(route.all_nodes_visited());
    }
}

#[test]
fn two_routes() {
    let mut b = GraphBuilder::new();
    let key_top = b.key("KEY TOP", 1, KeyKind::Reusable);
    let key_bottom = b.key("KEY BOTTOM", 1, KeyKind::Reusable);
    let key_end = b.key("KEY END", 1, KeyKind::Reusable);
    let start = b.room("ROOM START");
    let top1 = b.room("ROOM TOP 1");
    let top2 = b.room("ROOM TOP 2");
    let bottom1 = b.room("ROOM BOTTOM 1");
    let bottom2 = b.room("ROOM BOTTOM 2");
    let merge = b.room("ROOM MERGE");
    let end = b.room("ROOM END");
    let item_top1 = b.item("ITEM TOP 1", 1, top1, &[]);
    let item_bottom1 = b.item("ITEM BOTTOM 1", 1, bottom1, &[]);
    let item_merge = b.item("ITEM MERGE", 1, merge, &[]);
    b.no_return(start, top1, &[]).unwrap();
    b.no_return(start, bottom1, &[]).unwrap();
    b.blocked_door(top1, top2, &[k(key_top)]).unwrap();
    b.door(top2, merge, &[]).unwrap();
    b.blocked_door(bottom1, bottom2, &[k(key_bottom)]).unwrap();
    b.door(bottom2, merge, &[]).unwrap();
    b.door(merge, end, &[k(key_end)]).unwrap();
    let graph = b.build().unwrap();
    for seed in 0..SEEDS {
        let route = route(&graph, seed);
        assert_item(&route, item_top1, &[Some(key_top)]);
        assert_item(&route, item_bottom1, &[Some(key_bottom)]);
        assert_item(&route, item_merge, &[Some(key_end)]);
        assert!(route.all_nodes_visited());
    }
}

#[test]
fn removable_single_key_required() {
    let mut b = DependencyGraphBuilder::new();
    let key0 = b.removable_key(1, "KEY 0");
    let key1 = b.reusable_key(1, "KEY 1");
    let room0 = b.root("ROOM 0");
    let item0 = b.item(1, "ITEM 0", room0, &[]);
    let room1 = b.and_gate("ROOM 1", room0, &[k(key0)]);
    let item1 = b.item(1, "ITEM 1", room1, &[]);
    let room2 = b.and_gate("ROOM 2", room0, &[k(key0)]);
    let item2 = b.item(1, "ITEM 1", room2, &[]);
    b.and_gate("ROOM 2", room2, &[k(key1)]);
    let graph = b.build();
    for seed in 0..SEEDS {
        let route = route(&graph, seed);
        assert_key_once(&route, key0, &[item0]);
        assert_key_once(&route, key1, &[item1, item2]);
        assert!(route.all_nodes_visited());
    }
}

#[test]
fn removable_multiple_keys_required() {
    let mut b = DependencyGraphBuilder::new();
    let key0 = b.removable_key(1, "KEY 0");
    let room0 = b.root("ROOM 0");
    let item0 = b.item(1, "ITEM 0", room0, &[]);
    let room1 = b.and_gate("ROOM 1", room0, &[k(key0)]);
    b.item(1, "ITEM 1", room1, &[]);
    let room2 = b.and_gate("ROOM 2", room1, &[k(key0)]);
    b.item(1, "ITEM 2", room2, &[]);
    b.and_gate("ROOM 3", room2, &[k(key0)]);
    let graph = b.build();
    for seed in 0..SEEDS {
        let route = route(&graph, seed);
        assert!(route.all_nodes_visited());
        assert_item(&route, item0, &[Some(key0)]);
        assert_key_quantity(&route, key0, 3);
    }
}

#[test]
fn removable_multiple_keys_required_once() {
    let mut b = DependencyGraphBuilder::new();
    let key0 = b.removable_key(1, "KEY 0");
    let key1 = b.reusable_key(1, "KEY 0");
    let room0 = b.root("ROOM 0");
    let item0 = b.item(1, "ITEM 0", room0, &[]);
    let room1 = b.and_gate("ROOM 1", room0, &[k(key0)]);
    let item1 = b.item(1, "ITEM 1", room1, &[]);
    let room2 = b.and_gate("ROOM 2", room0, &[k(key0)]);
    let item2 = b.item(1, "ITEM 2", room2, &[]);
    b.and_gate("ROOM 3", room1, &[k(key0)]);
    let room4 = b.and_gate("ROOM 4", room2, &[k(key0)]);
    b.and_gate("ROOM 5", room4, &[k(key1)]);
    let graph = b.build();
    for seed in 0..SEEDS {
        let route = route(&graph, seed);
        assert!(route.all_nodes_visited());
        assert_key_once(&route, key1, &[item1, item2]);
        assert_item(&route, item0, &[Some(key0)]);
        assert_item(&route, item1, &[Some(key0), Some(key1)]);
        assert_item(&route, item2, &[Some(key0), Some(key1)]);
        assert_key_quantity(&route, key0, 2);
    }
}

/// Keys only go in slots whose group has every bit of the key's.
#[test]
fn keys_restricted_to_zones() {
    let mut b = DependencyGraphBuilder::new();
    let key1 = b.reusable_key(1, "KEY 1");
    let key2 = b.reusable_key(2, "KEY 2");
    let key3 = b.reusable_key(3, "KEY 3");
    let room0 = b.root("ROOM 0");
    let room1 = b.and_gate("ROOM 3", room0, &[k(key1)]);
    let room2 = b.and_gate("ROOM 4", room0, &[k(key2)]);
    let room3 = b.and_gate("ROOM 5", room0, &[k(key3)]);
    b.or_gate("ROOM 6", &[room1, room2, room3]);
    let item1 = b.item(1, "ITEM 1", room0, &[]);
    let item2 = b.item(2, "ITEM 2", room0, &[]);
    let item3 = b.item(3, "ITEM 3", room0, &[]);
    let item7 = b.item(7, "ITEM 7", room0, &[]);
    let graph = b.build();
    for seed in 0..SEEDS {
        let route = route(&graph, seed);
        assert!(route.all_nodes_visited());
        assert_key_once(&route, key1, &[item1, item3, item7]);
        assert_key_once(&route, key2, &[item2, item3, item7]);
        assert_key_once(&route, key3, &[item3, item7]);
    }
}

#[test]
fn requires_non_existent_room() {
    let mut b = DependencyGraphBuilder::new();
    let room0 = b.root("ROOM 0");
    let room1 = b.root("ROOM 1");
    b.and_gate("ROOM 2", room0, &[n(room1)]);
    let graph = b.build();
    for seed in 0..SEEDS {
        assert!(!route(&graph, seed).all_nodes_visited());
    }
}

#[test]
fn requires_room_previous_segment() {
    let mut b = DependencyGraphBuilder::new();
    let room0 = b.root("ROOM 0");
    let room1 = b.and_gate("ROOM 1", room0, &[]);
    let room2 = b.no_return("ROOM 2", room0, &[n(room1)]);
    b.and_gate("ROOM 3", room2, &[n(room1)]);
    let graph = b.build();
    for seed in 0..SEEDS {
        assert!(route(&graph, seed).all_nodes_visited());
    }
}

/// The solver follows two-way edges into a room from either end, or it
/// misses rooms, slots and keys (fixed in common 1.6.1).
#[test]
fn solver_handles_reversed_two_way_edges() {
    let mut b = GraphBuilder::new();
    let key0 = b.key("K0", 1, KeyKind::Consumable);
    let room_a = b.room("ROOM A");
    let room_b = b.room("ROOM B");
    let room_c = b.room("ROOM C");
    let room_d = b.room("ROOM D");
    let room_e = b.room("ROOM E");
    b.item("ITEM 1", 1, room_a, &[]);
    b.item("ITEM 2", 1, room_c, &[]);
    b.door(room_a, room_b, &[]).unwrap();
    b.door(room_c, room_b, &[]).unwrap();
    b.door(room_b, room_d, &[k(key0)]).unwrap();
    b.door(room_a, room_e, &[k(key0)]).unwrap();
    let graph = b.build().unwrap();
    for seed in 0..SEEDS {
        let route = route(&graph, seed);
        assert!(route.all_nodes_visited());
        assert_eq!(route.solve(), SolveResult::Ok);
    }
}

/// The solver opens consumable doors one at a time, so keys found behind
/// one pay for the next (fixed in common 1.6.1).
#[test]
fn solver_handles_sequential_consumable_keys() {
    let mut b = GraphBuilder::new();
    let coin = b.key("COIN", 1, KeyKind::Consumable);
    let lobby = b.room("LOBBY");
    b.item("ITEM 0", 1, lobby, &[]);
    b.item("ITEM 1", 1, lobby, &[]);
    let room_a = b.room("ROOM A");
    b.item("ITEM A1", 1, room_a, &[]);
    b.item("ITEM A2", 1, room_a, &[]);
    b.door(lobby, room_a, &[k(coin)]).unwrap();
    let room_b = b.room("ROOM B");
    b.door(lobby, room_b, &[k(coin), k(coin)]).unwrap();
    let graph = b.build().unwrap();
    for seed in 0..SEEDS {
        let route = route(&graph, seed);
        assert!(route.all_nodes_visited());
        assert_eq!(route.solve(), SolveResult::Ok);
    }
}

/// The solver doesn't walk an edge that needs a node not yet visited.
#[test]
fn solver_waits_for_required_nodes() {
    let mut b = GraphBuilder::new();
    let room0 = b.room("ROOM 0");
    let room1 = b.room("ROOM 1");
    let room2 = b.room("ROOM 2");
    b.door(room0, room2, &[n(room1)]).unwrap();
    let graph = b.build().unwrap();
    let route = route(&graph, 0);
    assert!(!route.all_nodes_visited());
    assert_eq!(route.solve(), SolveResult::NodesRemaining);
}

#[test]
fn dead_ends_are_reported_and_limited() {
    let mut b = DependencyGraphBuilder::new();
    let key0 = b.reusable_key(1, "KEY 0");
    let key1 = b.reusable_key(1, "KEY 1");
    let room0 = b.root("ROOM 0");
    b.item(1, "ITEM 0", room0, &[]);
    let room1 = b.and_gate("ROOM 1", room0, &[k(key0)]);
    b.and_gate("ROOM 2", room1, &[k(key1)]);
    let graph = b.build();

    let mut reports = Vec::new();
    let mut record = |info: &DeadEndInfo| reports.push(info.clone());
    let options = RouteFinderOptions {
        on_dead_end: Some(&mut record),
        ..RouteFinderOptions::default()
    };
    let route = find(&graph, &mut Rng::new(1), options).unwrap();
    assert!(!route.all_nodes_visited());
    assert_eq!(reports.len(), route.dead_ends());
    // Key 0 goes in the only slot, and room 1 is stuck without key 1, ten
    // times over; backed up to the start, the door to room 1 is stuck too.
    assert_eq!(reports.len(), 11);
    let inner = &reports[0];
    assert_eq!(inner.stuck_edges.len(), 1);
    assert_eq!(inner.stuck_edges[0].missing_keys, [key1]);
    assert_eq!(inner.key_shortages[0].spare_compatible_items, 0);
    assert!(inner.describe(&graph).contains("NO SPARE COMPATIBLE ITEMS"));
    assert_eq!(reports[10].stuck_edges[0].missing_keys, [key0]);

    let options = RouteFinderOptions {
        max_dead_ends: Some(1),
        ..RouteFinderOptions::default()
    };
    let error = find(&graph, &mut Rng::new(1), options).unwrap_err();
    assert!(matches!(error, RouteError::TooManyDeadEnds(_)));

    let options = RouteFinderOptions {
        depth_limit: Some(0),
        ..RouteFinderOptions::default()
    };
    let error = find(&graph, &mut Rng::new(1), options).unwrap_err();
    assert!(matches!(error, RouteError::DepthLimit));
}

#[test]
fn a_seed_always_gives_the_same_route() {
    let mut b = DependencyGraphBuilder::new();
    let key0 = b.reusable_key(1, "KEY 0");
    let key1 = b.consumable_key(1, "KEY 1");
    let room0 = b.root("ROOM 0");
    for i in 0..6 {
        b.item(1, &format!("ITEM {i}"), room0, &[]);
    }
    let room1 = b.and_gate("ROOM 1", room0, &[k(key0)]);
    b.and_gate("ROOM 2", room1, &[k(key1)]);
    b.and_gate("ROOM 3", room0, &[k(key1)]);
    let graph = b.build();
    let placements = |seed| route(&graph, seed).placements().collect::<Vec<_>>();
    assert_eq!(placements(7), placements(7));
    assert!(
        (0..20).any(|seed| placements(seed) != placements(7)),
        "seeds vary the placement"
    );
}

// TestGraphBuilder.cs

#[test]
fn no_duplicate_edge_simple() {
    let mut b = GraphBuilder::new();
    let room_a = b.room("ROOM A");
    let room_b = b.room("ROOM B");
    let edge_a = b.door(room_a, room_b, &[]).unwrap();
    let edge_b = b.door(room_a, room_b, &[]).unwrap();
    let edge_c = b.door(room_b, room_a, &[]).unwrap();
    assert_eq!(edge_a, edge_b);
    assert_eq!(edge_a, edge_c);
    assert_eq!(b.build().unwrap().edges().len(), 1);
}

#[test]
fn no_duplicate_edge_new_requirements() {
    let mut b = GraphBuilder::new();
    let key = b.key("KEY 0", 1, KeyKind::Reusable);
    let room_a = b.room("ROOM A");
    let room_b = b.room("ROOM B");
    let edge_a = b.door(room_a, room_b, &[]).unwrap();
    let edge_b = b.door(room_b, room_a, &[k(key)]).unwrap();
    assert_ne!(edge_a, edge_b);
    let graph = b.build().unwrap();
    assert_eq!(graph.edges(), [edge_b]);
}

/// Edges added twice must agree, and an edge's reverse is reconciled with
/// it as `GraphBuilder::edge` describes.
#[test]
fn inverse_edges_are_reconciled() {
    let mut b = GraphBuilder::new();
    let key0 = b.key("KEY 0", 1, KeyKind::Reusable);
    let key1 = b.key("KEY 1", 1, KeyKind::Reusable);
    let rooms: Vec<NodeId> = (0..12).map(|i| b.room(format!("ROOM {i}"))).collect();
    let r = |i: usize| rooms[i];

    // The same edge again with another kind or other requirements.
    b.door(r(0), r(1), &[]).unwrap();
    assert!(matches!(
        b.one_way(r(0), r(1), &[]),
        Err(GraphError::DuplicateKind { .. })
    ));
    assert!(matches!(
        b.door(r(0), r(1), &[k(key0)]),
        Err(GraphError::DuplicateRequirements { .. })
    ));

    // Two-way both sides: requirements on one side win, on both conflict.
    let locked = b.door(r(2), r(3), &[k(key0)]).unwrap();
    assert_eq!(b.door(r(3), r(2), &[]).unwrap(), locked);
    assert!(matches!(
        b.door(r(3), r(2), &[k(key1)]),
        Err(GraphError::ConflictingInverse { .. })
    ));

    // One-way and no-return can't have a reverse.
    b.one_way(r(4), r(5), &[]).unwrap();
    assert!(b.door(r(5), r(4), &[]).is_err());
    b.door(r(6), r(7), &[]).unwrap();
    assert!(b.no_return(r(7), r(6), &[]).is_err());

    // A free two-way edge added after an unblock is dropped, and one with
    // requirements conflicts; an unblock can't replace a locked reverse.
    let unblock = b.blocked_door(r(9), r(8), &[k(key0)]).unwrap();
    b.door(r(8), r(9), &[]).unwrap();
    assert_eq!(b.door(r(8), r(9), &[]).unwrap(), unblock);
    assert!(b.door(r(8), r(9), &[k(key1)]).is_err());
    b.door(r(10), r(11), &[k(key0)]).unwrap();
    assert!(b.blocked_door(r(11), r(10), &[]).is_err());

    let graph = b.build().unwrap();
    let unblocks: Vec<&Edge> = graph
        .edges()
        .iter()
        .filter(|e| e.kind == EdgeKind::UnlockTwoWay)
        .collect();
    assert_eq!(unblocks, [&unblock]);
}

/// A no-return edge that never leads back builds.
#[test]
fn no_return_that_does_not_return() {
    let mut b = GraphBuilder::new();
    let rooms: Vec<NodeId> = (0..7).map(|i| b.room(format!("ROOM {i}"))).collect();
    b.door(rooms[0], rooms[1], &[]).unwrap();
    b.door(rooms[1], rooms[2], &[]).unwrap();
    b.door(rooms[1], rooms[3], &[]).unwrap();
    b.no_return(rooms[3], rooms[4], &[]).unwrap();
    b.door(rooms[4], rooms[5], &[]).unwrap();
    b.door(rooms[5], rooms[6], &[]).unwrap();
    b.build().unwrap();
}

#[test]
fn an_empty_graph_is_an_error() {
    assert_eq!(
        GraphBuilder::new().build().unwrap_err(),
        GraphError::NoNodes
    );
}

#[test]
fn example_re2() {
    let mut b = GraphBuilder::new();
    let blue_keycard = b.key("Blue Keycard", 0, KeyKind::Reusable);
    let unicorn = b.key("Unicorn Medal", 0, KeyKind::Reusable);
    let spade = b.key("Spade Key", 0, KeyKind::Reusable);
    let diamond = b.key("Diamond Key", 0, KeyKind::Reusable);
    let small = b.key("Small Key", 0, KeyKind::Consumable);
    let red_jewel = b.key("Red Jewel", 0, KeyKind::Consumable);

    let room103 = b.room("103 - RPD FRONT");
    let room200 = b.room("200 - RPD MAIN HALL");
    let room201 = b.room("201 - RPD WAITING");
    let room202 = b.room("202 - RPD MARVIN");
    let room203 = b.room("203 - RPD LICKER");
    let room204 = b.room("204 - RPD RECORDS");
    let room205 = b.room("205 - RPD ARMS");
    let room208 = b.room("208 - RPD FIREPLACE");
    let room206 = b.room("206 - RPD DARK STAIRS");
    let room207 = b.room("207 - RPD EVIDENCE");

    b.item("103 - PLANT", 0, room103, &[]);
    b.item("200 - DESK 1", 0, room200, &[]);
    b.item("200 - DESK 2", 0, room200, &[]);
    b.item("200 - FOUNTAIN", 0, room200, &[k(unicorn)]);
    b.item("201 - DOCUMENT", 0, room201, &[]);
    b.item("201 - LOCKED DRAWER", 0, room201, &[k(small)]);
    b.item("202 - MARVIN GIFT", 0, room200, &[]);
    b.item("203 - FLOOR", 0, room203, &[]);
    b.item("203 - BODY", 0, room203, &[]);
    b.item("204 - STEPLADDER TOP", 0, room204, &[]);
    b.item("204 - STEPLADDER MID", 0, room204, &[]);
    b.item("204 - DOOR DOCUMENT", 0, room204, &[]);
    b.item("204 - DOOR HIDDEN", 0, room204, &[]);
    b.item("206 - STAIRS 1", 0, room206, &[]);
    b.item("206 - STAIRS 2", 0, room206, &[]);
    b.item("208 - FIREPLACE", 0, room208, &[k(red_jewel), k(red_jewel)]);
    b.item("208 - DOCUMENT", 0, room208, &[]);
    b.item("208 - CORNER", 0, room208, &[]);
    b.item("208 - TABLES", 0, room208, &[]);

    b.door(room103, room200, &[]).unwrap();
    b.door(room200, room201, &[k(blue_keycard)]).unwrap();
    b.door(room201, room203, &[]).unwrap();
    b.door(room203, room204, &[k(spade)]).unwrap();
    b.one_way(room202, room200, &[]).unwrap();
    b.door(room203, room205, &[]).unwrap();
    b.door(room205, room206, &[]).unwrap();
    b.door(room205, room208, &[]).unwrap();
    b.door(room206, room207, &[k(diamond)]).unwrap();
    b.door(room207, room202, &[]).unwrap();
    let graph = b.build().unwrap();

    for seed in 0..SEEDS {
        let route = route(&graph, seed);
        // One of each key, and the two red jewels.
        assert_eq!(route.placements().count(), 7, "seed {seed}");
        assert!(route.all_nodes_visited());
        assert_eq!(route.solve(), SolveResult::Ok);
    }
}

#[test]
fn example_re2r() {
    let mut b = GraphBuilder::new();
    let gas_station_key = b.key("Gas Station Key", 0, KeyKind::Reusable);
    let outside = b.room("GAS STATION OUTSIDE");
    let shop = b.room("GAS STATION SHOP");
    let corridor = b.room("GAS STATION CORRIDOR");
    let back_room = b.room("GAS STATION BACK ROOM");
    b.door(outside, shop, &[]).unwrap();
    b.door(shop, corridor, &[]).unwrap();
    b.one_way(corridor, back_room, &[]).unwrap();
    b.door(back_room, shop, &[k(gas_station_key)]).unwrap();
    b.item("SHOP ITEM 1", 0, shop, &[]);
    b.item("SHOP ITEM 2", 0, shop, &[]);
    b.item("BACK ROOM ITEM 1", 0, back_room, &[]);
    b.item("BACK ROOM ITEM 2", 0, back_room, &[]);
    let graph = b.build().unwrap();
    for seed in 0..SEEDS {
        // Common only checks that it finishes.
        route(&graph, seed);
    }
}
