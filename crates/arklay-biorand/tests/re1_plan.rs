//! Whole games planned on the real RE1 map, for both players, many seeds and
//! a few configurations, each checked against the map it was planned on.

mod common;

use std::collections::HashMap;

use arklay_biorand::config::{self, Config};
use arklay_biorand::items::{Pickup, Role};
use arklay_biorand::keys::KeyOptions;
use arklay_biorand::map::Map;
use arklay_biorand::plan::{Players, generate};
use arklay_biorand::prepare;
use arklay_biorand::rng::Rng;

use common::{
    check_doors, check_game, check_locks, check_playthrough, game_map, load_map, with_locks,
};

const SEEDS: u64 = 50;

/// The configurations the plans are checked under, by name.
fn variants(map: &Map) -> Vec<(&'static str, Config)> {
    let definition = config::definition(map);
    let mut defaults = Config::defaults(&definition);
    // The door variants are below.
    defaults.set("doors/random", false);
    let ammo_kinds: Vec<String> = map
        .items
        .values()
        .filter(|def| def.kind.starts_with("ammo/"))
        .map(|def| def.kind.clone())
        .collect();

    let mut documents_off = defaults.clone();
    documents_off.set("items/documents", false);
    documents_off.set("items/hidden/keys", false);

    let mut max_ammo = defaults.clone();
    let mut min_ammo = defaults.clone();
    for kind in &ammo_kinds {
        max_ammo.set(format!("items/distribution/{kind}"), 1.0);
        max_ammo.set(format!("items/stack/min/{kind}"), 100);
        max_ammo.set(format!("items/stack/max/{kind}"), 100);
        min_ammo.set(format!("items/distribution/{kind}"), 0.0);
    }
    for def in map
        .items
        .values()
        .filter(|def| def.kind.starts_with("weapon/"))
    {
        let kind = &def.kind;
        max_ammo.set(
            format!("items/weapon/initial/min/{kind}"),
            i64::from(def.max),
        );
        min_ammo.set(format!("items/weapon/initial/max/{kind}"), 0);
    }

    // Every restriction staging has on the original doors.
    let mut strict = defaults.clone();
    strict.set("progression/tyrant1", "Always");
    strict.set("progression/tyrant2", "Always");
    strict.set("inventory/special/lockpick", "Never");
    strict.set("enemies/random", false);

    // The guardhouse and the lab left out.
    let mut small = defaults.clone();
    small.set("progression/guardhouse", "Never");
    small.set("progression/lab", "Never");
    small.set("ink/enable", "Always");
    small.set("hard", true);

    vec![
        ("default", defaults),
        ("documents off", documents_off),
        ("max ammo", max_ammo),
        ("min ammo", min_ammo),
        ("strict", strict),
        ("small", small),
    ]
}

/// The configurations with locks randomized, by name.
fn lock_variants(map: &Map) -> Vec<(&'static str, Config)> {
    let variants: HashMap<&str, Config> = variants(map).into_iter().collect();
    let locked = |name: &str, ratio: f64| {
        let mut config = variants[name].clone();
        config.set("locks/random", true);
        config.set("locks/ratio", ratio);
        config
    };
    let mut preserve = locked("default", 0.25);
    preserve.set("locks/preserve", true);
    vec![
        ("locks", locked("default", 0.25)),
        ("locks strict", locked("strict", 0.5)),
        ("locks small", locked("small", 0.25)),
        ("locks all", locked("documents off", 1.0)),
        ("locks preserve", preserve),
    ]
}

#[test]
fn re1_plans_hold_for_both_players() {
    let map = load_map();
    let mut games = 0;
    for (name, config) in variants(&map) {
        for seed in 0..SEEDS {
            let what = format!("{name} seed {seed}");
            let plan = generate(&map, &config, seed, Players::Both)
                .unwrap_or_else(|err| panic!("{what}: {err:#}"));
            assert_eq!(plan.games.len(), 2, "{what}");
            for game in &plan.games {
                let what = format!("{what} {:?}", game.player);
                check_game(&map, game).unwrap_or_else(|err| panic!("{what}: {err}"));
                games += 1;
            }
            if seed < 3 {
                let again = generate(&map, &config, seed, Players::Both).unwrap();
                assert_eq!(plan, again, "{what}: the same seed gives the same plan");
            }
        }
    }
    assert_eq!(games, 6 * SEEDS * 2);
}

#[test]
fn re1_lock_plans_hold_for_both_players() {
    let map = load_map();
    let mut games = 0;
    let mut locked_doors = 0;
    for (name, config) in lock_variants(&map) {
        for seed in 0..SEEDS {
            let what = format!("{name} seed {seed}");
            let plan = generate(&map, &config, seed, Players::Both)
                .unwrap_or_else(|err| panic!("{what}: {err:#}"));
            for game in &plan.games {
                let what = format!("{what} {:?}", game.player);
                assert!(!game.deferred.iter().any(|o| o == "locks/random"), "{what}");
                for flag in ["LOCK_RANDOMIZER", "BR_LOCK_FULL_BYTE"] {
                    assert!(game.system_flags.contains(&flag), "{what}: {flag}");
                }
                check_locks(&map, game).unwrap_or_else(|err| panic!("{what}: {err}"));
                check_game(&map, game).unwrap_or_else(|err| panic!("{what}: {err}"));
                locked_doors += game.locks.iter().filter(|l| l.key_item().is_some()).count();
                games += 1;
            }
            if seed < 3 {
                let again = generate(&map, &config, seed, Players::Both).unwrap();
                assert_eq!(plan, again, "{what}: the same seed gives the same plan");
            }
        }
    }
    assert_eq!(games, 5 * SEEDS * 2);
    assert!(
        locked_doors > games as usize * 10,
        "{locked_doors} locked doors"
    );
}

#[test]
fn re1_variants_change_what_they_say() {
    let map = load_map();
    let variants: HashMap<&str, Config> = variants(&map).into_iter().collect();
    let supplies = |config: &Config, seed: u64| -> Vec<(String, u16)> {
        let plan = generate(
            &map,
            config,
            seed,
            Players::Only(arklay_biorand::map::Player::Jill),
        )
        .unwrap();
        plan.games[0]
            .items
            .values()
            .flat_map(|slots| slots.values())
            .filter_map(|slot| match slot.pickup {
                Pickup::Item {
                    item,
                    amount,
                    role: Role::Supply,
                } => Some((map.items[&item].kind.clone(), amount)),
                _ => None,
            })
            .collect()
    };
    for seed in 0..5 {
        let max = supplies(&variants["max ammo"], seed);
        assert!(
            max.iter()
                .filter(|(kind, _)| kind.starts_with("ammo/"))
                .all(|(_, n)| *n == 100)
        );
        assert!(
            max.iter().any(|(kind, _)| kind.starts_with("ammo/")),
            "seed {seed}"
        );
        let min = supplies(&variants["min ammo"], seed);
        assert!(
            min.iter().all(|(kind, _)| !kind.starts_with("ammo/")),
            "seed {seed}"
        );
    }

    let default = generate(&map, &variants["default"], 1, Players::Configured).unwrap();
    let game = &default.games[0];
    assert!(game.deferred.is_empty(), "{:?}", game.deferred);
    for flag in [
        "RANDOMIZER",
        "ITEM_RANDOMIZER",
        "ENEMY_RANDOMIZER",
        "LAB",
        "BR_INK_FLAG",
    ] {
        assert!(game.system_flags.contains(&flag), "{flag}");
    }
    assert!(!game.system_flags.contains(&"DOOR_RANDOMIZER"));
    let json = game.spoiler.to_json();
    let text = serde_json::to_string(&json).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&text).unwrap(),
        json
    );
    assert!(!game.spoiler.keys.is_empty());
    assert!(game.spoiler.to_text().contains("Keys:"));

    let small = generate(&map, &variants["small"], 1, Players::Configured).unwrap();
    let game = &small.games[0];
    assert!(!game.system_flags.contains(&"LAB"));
    assert!(!game.items.contains_key("501") && !game.items.contains_key("401"));
    let raised: Vec<_> = game
        .scenario_flags
        .iter()
        .filter(|f| f.raised)
        .map(|f| f.flag)
        .collect();
    assert!(raised.contains(&122) && raised.contains(&123), "{raised:?}");
}

/// On the segmented default layout too, the walk is a real check: without
/// any one placed key, the play-through fails.
#[test]
fn re1_walk_catches_a_missing_key_across_segments() {
    let map = load_map();
    let mut config = Config::defaults(&config::definition(&map));
    config.set("doors/random", false);
    let plan = generate(
        &map,
        &config,
        0,
        Players::Only(arklay_biorand::map::Player::Chris),
    )
    .unwrap();
    let game = &plan.games[0];
    let options = prepare::Options::from_config(&game.config, game.player);
    assert!(options.progression.segmented_caves && options.progression.segmented_lab);
    let prepared = prepare::prepare(&map, &options, &mut Rng::named(0, "re1map")).unwrap();
    let key_options = KeyOptions::from_config(&game.config, game.player);
    let keys = game.keys.as_ref().unwrap();
    check_playthrough(&prepared, &key_options, keys).unwrap();
    for i in 0..keys.keys.len() {
        let mut broken = keys.clone();
        let removed = broken.keys.remove(i);
        assert!(
            check_playthrough(&prepared, &key_options, &broken).is_err(),
            "the play-through still passes without {removed:?}"
        );
    }
}

/// With locks randomized too, the walk reads the new locks: without some
/// placed keys the play-through fails, and so it does if the drawn locks are
/// keyed with an item that is no key.
#[test]
fn re1_walk_catches_a_missing_key_with_locks() {
    let map = load_map();
    let (_, config) = lock_variants(&map).remove(0);
    for player in [
        arklay_biorand::map::Player::Chris,
        arklay_biorand::map::Player::Jill,
    ] {
        let plan = generate(&map, &config, 7, Players::Only(player)).unwrap();
        let game = &plan.games[0];
        let options = prepare::Options::from_config(&game.config, game.player);
        let prepared =
            prepare::prepare(&map, &options, &mut Rng::named(plan.seed, "re1map")).unwrap();
        let key_options = KeyOptions::from_config(&game.config, game.player);
        let keys = game.keys.as_ref().unwrap();
        let walked = with_locks(&prepared, &game.locks).unwrap();
        check_playthrough(&walked, &key_options, keys).unwrap();
        // A new lock may guard a shortcut, so not every key is needed; but
        // some are.
        let needed = (0..keys.keys.len())
            .filter(|&i| {
                let mut broken = keys.clone();
                broken.keys.remove(i);
                check_playthrough(&walked, &key_options, &broken).is_err()
            })
            .count();
        assert!(needed > 0, "{player:?}: no key is needed");
        // Rekey the drawn locks with an item that is no key.
        let mut locks = game.locks.clone();
        for lock in locks
            .iter_mut()
            .filter(|lock| lock.drawn && lock.key_item().is_some())
        {
            lock.key = 2;
        }
        let walked = with_locks(&prepared, &locks).unwrap();
        assert!(
            check_playthrough(&walked, &key_options, keys).is_err(),
            "{player:?}: locks nothing opens go unnoticed"
        );
    }
}

/// The configurations with doors randomized, by name.
fn door_variants(map: &Map) -> Vec<(&'static str, Config)> {
    let mut doors = Config::defaults(&config::definition(map));
    doors.set("doors/random", true);
    doors.set("locks/random", false);
    doors.set("enemies/random", false);
    let mut locks = doors.clone();
    locks.set("locks/random", true);
    locks.set("locks/ratio", 0.25);
    let mut enemies = locks.clone();
    enemies.set("enemies/random", true);
    let mut wide = enemies.clone();
    wide.set("doors/segments", 6);
    wide.set("doors/rooms", 1.0);
    let mut narrow = doors.clone();
    narrow.set("doors/segments", 1);
    narrow.set("doors/rooms", 0.0);
    vec![
        ("doors", doors),
        ("doors locks", locks),
        ("doors locks enemies", enemies),
        ("doors wide", wide),
        ("doors narrow", narrow),
    ]
}

#[test]
fn re1_door_plans_hold_for_both_players() {
    let map = load_map();
    let mut games = 0;
    for (name, config) in door_variants(&map) {
        for seed in 0..SEEDS {
            let what = format!("{name} seed {seed}");
            let plan = generate(&map, &config, seed, Players::Both)
                .unwrap_or_else(|err| panic!("{what}: {err:#}"));
            for game in &plan.games {
                let what = format!("{what} {:?}", game.player);
                assert!(game.deferred.is_empty(), "{what}: {:?}", game.deferred);
                for flag in ["DOOR_RANDOMIZER", "BR_LOCK_FULL_BYTE"] {
                    assert!(game.system_flags.contains(&flag), "{what}: {flag}");
                }
                check_doors(&map, game).unwrap_or_else(|err| panic!("{what}: {err}"));
                check_locks(&map, game).unwrap_or_else(|err| panic!("{what}: {err}"));
                check_game(&map, game).unwrap_or_else(|err| panic!("{what}: {err}"));
                games += 1;
            }
            if seed < 3 {
                let again = generate(&map, &config, seed, Players::Both).unwrap();
                assert_eq!(plan, again, "{what}: the same seed gives the same plan");
            }
        }
    }
    assert_eq!(games, 5 * SEEDS * 2);
}

/// With doors randomized, the walk on the rewired map still needs the keys:
/// without some placed key the play-through fails.
#[test]
fn re1_walk_catches_a_missing_key_with_doors() {
    let map = load_map();
    let (_, config) = door_variants(&map).remove(1);
    for player in [
        arklay_biorand::map::Player::Chris,
        arklay_biorand::map::Player::Jill,
    ] {
        let plan = generate(&map, &config, 3, Players::Only(player)).unwrap();
        let game = &plan.games[0];
        let walked = with_locks(&game_map(&map, game).unwrap(), &game.locks).unwrap();
        let key_options = KeyOptions::from_config(&game.config, game.player);
        let keys = game.keys.as_ref().unwrap();
        check_playthrough(&walked, &key_options, keys).unwrap();
        let needed = (0..keys.keys.len())
            .filter(|&i| {
                let mut broken = keys.clone();
                broken.keys.remove(i);
                check_playthrough(&walked, &key_options, &broken).is_err()
            })
            .count();
        assert!(needed > 0, "{player:?}: no key is needed");
    }
}
