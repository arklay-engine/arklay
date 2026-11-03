//! Key placement on the real RE1 map, checked by playing each placement
//! through with a walk over the map itself rather than the routing graph.

mod common;

use std::collections::HashSet;

use arklay_biorand::config::Config;
use arklay_biorand::keys::{self, KeyOptions};
use arklay_biorand::map::{Map, Player};
use arklay_biorand::prepare::{self, Progression};
use arklay_biorand::rng::Rng;

use common::{World, check_playthrough, load_map};

const SEEDS: u64 = 50;

/// The map prepared for `player` with every area in, nothing segmented and
/// no slot restricted: each slot takes any key.
fn original_layout(map: &Map, player: Player) -> Map {
    let mut config = Config::new();
    config.set("progression/guardhouse", "Always");
    config.set("progression/mansion2", "Never");
    config.set("progression/caves", "Always");
    config.set("progression/lab", "Always");
    config.set("progression/tyrant1", "Never");
    let options = prepare::Options::from_config(&config, player);
    assert_eq!(
        options.progression,
        Progression {
            guardhouse: true,
            mansion2: false,
            segmented_caves: false,
            lab: true,
            segmented_lab: false,
            tyrant1: false,
        }
    );
    prepare::prepare(map, &options, &mut Rng::named(0, "re1map"))
        .unwrap_or_else(|err| panic!("{err:#}"))
}

#[test]
fn re1_keys_are_placed_for_both_players() {
    let original = load_map();
    let mut runs = 0;
    let mut first_come_softlocks = 0;
    for player in [Player::Chris, Player::Jill] {
        let map = original_layout(&original, player);
        // Only the plain slots and every lock keyed; then documents and
        // hidden slots too, with the lockpick opening the sword key doors
        // and the small key drawers.
        let plain = KeyOptions::new(player);
        let wide = KeyOptions {
            include_documents: true,
            include_hidden: true,
            implicit_keys: vec![51, 61],
            ..KeyOptions::new(player)
        };
        for options in [plain, wide] {
            for seed in 0..SEEDS {
                let what = format!("{player:?} seed {seed} {options:?}");
                let placement = keys::randomize(&map, &options, &mut Rng::named(seed, "key"))
                    .unwrap_or_else(|err| panic!("{what}: {err:#}"));
                assert!(
                    placement.all_nodes_visited,
                    "{what}: route incomplete\n{}",
                    placement.log.join("\n")
                );
                let mut flags = HashSet::new();
                for placed in &placement.keys {
                    let flag = placed.slot.flag.expect("key slots have pickup flags");
                    assert!(flags.insert(flag), "{what}: two keys in pickup flag {flag}");
                }
                check_playthrough(&map, &options, &placement)
                    .unwrap_or_else(|err| panic!("{what}: {err}"));
                runs += 1;
                if World::new(&map, &options, &placement).first_come().is_err() {
                    first_come_softlocks += 1;
                }
            }
        }
    }
    // Common 1.6.1 promises an order of play that works, not that every
    // order does: spending a consumable key on the wrong lock first can strand
    // the player. Run with `--nocapture` to see how often using each one on
    // the first lock found does.
    println!("first-come play softlocks in {first_come_softlocks} of {runs} placements");
}

/// The walk is a real check: taking away any placed key that some lock
/// needs breaks the play-through.
#[test]
fn re1_walk_catches_a_missing_key() {
    let map = original_layout(&load_map(), Player::Jill);
    let options = KeyOptions::new(Player::Jill);
    let placement = keys::randomize(&map, &options, &mut Rng::named(0, "key")).unwrap();
    check_playthrough(&map, &options, &placement).unwrap();
    assert!(
        placement.keys.len() > 30,
        "{} keys placed",
        placement.keys.len()
    );
    for i in 0..placement.keys.len() {
        let mut broken = placement.clone();
        let removed = broken.keys.remove(i);
        assert!(
            check_playthrough(&map, &options, &broken).is_err(),
            "the play-through still passes without {removed:?}"
        );
    }
}
