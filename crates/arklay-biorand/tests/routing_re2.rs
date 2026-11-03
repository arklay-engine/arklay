//! Common's `TestRoutingExamples`: Leon A of RE2 as a routing graph, with
//! how many copies of each key it should get.
//!
//! `data/re2-routing.json` is common's `test/.../data/re2.json` (MIT, Ted
//! John) with its commented-out second test case removed, as JSON has no
//! comments.

use std::collections::HashMap;

use arklay_biorand::rng::Rng;
use arklay_biorand::routing::{
    EdgeKind, Graph, GraphBuilder, KeyId, KeyKind, NodeId, Requirement, SolveResult,
};
use serde_json::Value;

const DATA: &str = include_str!("data/re2-routing.json");

/// A filter field that is absent or matches.
fn matches(entry: &Value, field: &str, value: i64) -> bool {
    entry
        .get(field)
        .and_then(Value::as_i64)
        .is_none_or(|v| v == value)
}

fn strings(entry: &Value, field: &str) -> Vec<String> {
    entry
        .get(field)
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .map(|s| s.as_str().expect("a string").to_string())
                .collect()
        })
        .unwrap_or_default()
}

struct Example {
    graph: Graph,
    /// Each key's least and most copies.
    expectations: HashMap<KeyId, (usize, usize)>,
}

/// Builds the graph of the test case for `player` and `scenario` as
/// common's `GetTestCase` does.
fn example(player: i64, scenario: i64) -> Example {
    let data: Value = serde_json::from_str(DATA).expect("valid JSON");
    let case = data["testCases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["player"] == player && case["scenario"] == scenario)
        .expect("the test case");

    let mut b = GraphBuilder::new();
    let start = b.room("START");

    let mut keys: HashMap<String, KeyId> = HashMap::new();
    for (id, key) in data["keys"].as_object().unwrap() {
        let kind = KeyKind::parse(key["kind"].as_str().unwrap()).expect("a key kind");
        let name = key["name"].as_str().unwrap();
        keys.insert(id.clone(), b.key(name, 1, kind));
    }

    let rooms = data["rooms"].as_object().unwrap();
    let mut nodes: HashMap<String, NodeId> = HashMap::new();
    let requirement = |b: &mut GraphBuilder, nodes: &mut HashMap<String, NodeId>, text: &str| {
        let (kind, rest) = text.split_once('(').expect("kind(value)");
        let value = rest.strip_suffix(')').expect("kind(value)");
        match kind {
            "item" => Requirement::Key(keys[value]),
            "room" => Requirement::Node(nodes[value]),
            "flag" => Requirement::Node(
                *nodes
                    .entry(value.to_string())
                    .or_insert_with(|| b.room(value)),
            ),
            _ => panic!("failed to parse requirement {text}"),
        }
    };

    for (id, room) in rooms {
        let name = data["roomNames"][id].as_str().unwrap_or(id);
        let node = b.room(format!("ROOM:{name}"));
        nodes.insert(id.clone(), node);
        for item in room["items"].as_array().into_iter().flatten() {
            if !matches(item, "player", player) || !matches(item, "scenario", scenario) {
                continue;
            }
            let group = if item["kind"] == "low" { 2 } else { 1 };
            let requires: Vec<Requirement> = strings(item, "requires")
                .iter()
                .map(|r| requirement(&mut b, &mut nodes, r))
                .collect();
            let global = item["globalId"].as_i64().unwrap().to_string();
            let item_name = data["itemNames"][&global].as_str().unwrap_or(&global);
            b.item(format!("ITEM:{name}/{item_name}"), group, node, &requires);
        }
    }

    for (id, room) in rooms {
        let source = nodes[id];
        for door in room["doors"].as_array().into_iter().flatten() {
            if !matches(door, "player", player) || !matches(door, "scenario", scenario) {
                continue;
            }
            let kind = door["kind"].as_str();
            if matches!(kind, Some("locked" | "side")) {
                continue;
            }
            let target = door["target"].as_str().unwrap();
            let target = nodes[target.split(':').next().unwrap()];
            let requires: Vec<Requirement> = strings(door, "requires")
                .iter()
                .map(|r| requirement(&mut b, &mut nodes, r))
                .collect();
            let kind = match kind {
                Some("oneway") => EdgeKind::OneWay,
                Some("noreturn") => EdgeKind::NoReturn,
                Some("unblock") => EdgeKind::UnlockTwoWay,
                _ => EdgeKind::TwoWay,
            };
            b.edge(source, target, kind, &requires).unwrap();
        }
        for flag in room["flags"].as_array().into_iter().flatten() {
            if !matches(flag, "player", player) || !matches(flag, "scenario", scenario) {
                continue;
            }
            let name = flag["name"].as_str().unwrap();
            let node = *nodes
                .entry(name.to_string())
                .or_insert_with(|| b.room(format!("FLAG:{name}")));
            let requires: Vec<Requirement> = strings(flag, "requires")
                .iter()
                .map(|r| requirement(&mut b, &mut nodes, r))
                .collect();
            b.edge(source, node, EdgeKind::TwoWay, &requires).unwrap();
        }
    }

    let first = nodes[case["start"].as_str().unwrap()];
    b.door(start, first, &[]).unwrap();

    let expectations = case["expectations"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(id, range)| {
            let range = range.as_array().unwrap();
            let bound = |i: usize| range[i].as_u64().unwrap() as usize;
            (keys[id], (bound(0), bound(1)))
        })
        .collect();
    Example {
        graph: b.build().unwrap(),
        expectations,
    }
}

/// Common checks seed 0 of its own random stream; here a range of seeds
/// must all meet the expectations.
#[test]
fn example_re2_leon_a() {
    let example = example(0, 0);
    let graph = &example.graph;
    let seeds = 100;
    let mut greedy_short = 0;
    for seed in 0..seeds {
        let route = graph.generate_route(&mut Rng::new(seed));
        assert!(route.all_nodes_visited(), "seed {seed}");
        for key in graph.key_ids() {
            let count = route.items_with_key(key).len();
            let (least, most) = example.expectations.get(&key).copied().unwrap_or((0, 0));
            assert!(
                (least..=most).contains(&count),
                "seed {seed}: {} was placed {count} times, expected {least}..={most}",
                graph.key_name(key)
            );
        }
        // The solver spends consumable keys on the first lock it finds, so
        // it can strand itself where another order gets through. Common
        // doesn't check it here; run with `--nocapture` to see how often.
        if route.solve() != SolveResult::Ok {
            greedy_short += 1;
        }
    }
    println!("the solver leaves nodes in {greedy_short} of {seeds} routes");
}
