//! Whole games planned on the real RE1 map, for both players, many seeds and
//! a few configurations, each checked against the map it was planned on.

mod common;

use std::collections::{HashMap, HashSet};

use arklay_biorand::config::{self, Config};
use arklay_biorand::items::{Pickup, Role};
use arklay_biorand::keys::KeyOptions;
use arklay_biorand::map::Map;
use arklay_biorand::plan::{Game, Players, generate};
use arklay_biorand::prepare;
use arklay_biorand::rng::Rng;

use common::{check_playthrough, load_map};

const SEEDS: u64 = 50;

/// The configurations the plans are checked under, by name.
fn variants(map: &Map) -> Vec<(&'static str, Config)> {
    let definition = config::definition(map);
    let defaults = Config::defaults(&definition);
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
    assert_eq!(
        game.deferred,
        ["doors/random"],
        "the default turns door randomization on"
    );
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

/// Checks one game against the map it was planned on.
fn check_game(map: &Map, game: &Game) -> Result<(), String> {
    let options = prepare::Options::from_config(&game.config, game.player);
    let prepared = prepare::prepare(map, &options, &mut Rng::named(0, "re1map"))
        .map_err(|err| format!("{err:#}"))?;

    // The start fits the player's slots.
    let size = usize::from(game.inventory_sizes.of(game.player));
    if game.inventory.slots.len() > size {
        return Err(format!(
            "{} start slots for {size}",
            game.inventory.slots.len()
        ));
    }

    // Every slot of the game is planned once, and nothing else is.
    let mut expected = HashSet::new();
    let mut by_flag: HashMap<u16, Pickup> = HashMap::new();
    for room in prepared.rooms.values() {
        let dir = room.room_id().to_ascii_lowercase();
        for item in &room.items {
            let flag = item.flag.ok_or("a slot without a pickup flag")?;
            expected.insert((dir.clone(), item.id));
            let slot = game
                .items
                .get(&dir)
                .and_then(|slots| slots.get(&item.id))
                .ok_or_else(|| format!("{} AOT {} is not planned", room.key, item.id))?;
            if slot.flag != flag {
                return Err(format!(
                    "{} AOT {}: flag {} for {flag}",
                    room.key, item.id, slot.flag
                ));
            }
            // Slots sharing a flag hold the same.
            if let Some(other) = by_flag.insert(flag, slot.pickup)
                && other != slot.pickup
            {
                return Err(format!("flag {flag} holds two things"));
            }
            check_pickup(&prepared, game, item, slot.pickup)
                .map_err(|err| format!("{} AOT {} ({:?}): {err}", room.key, item.id, item.name))?;
        }
    }
    let planned: usize = game.items.values().map(|slots| slots.len()).sum();
    if planned != expected.len() {
        return Err(format!("{planned} slots planned for {}", expected.len()));
    }

    // One world weapon per group, none of a group the player starts with.
    let group = |item: u8| {
        map.items[&item]
            .kind
            .split('/')
            .nth(1)
            .unwrap_or("")
            .to_string()
    };
    let mut groups: HashSet<String> = game.inventory.weapons.iter().map(|&w| group(w)).collect();
    for pickup in by_flag.values() {
        if let Pickup::Item {
            item,
            role: Role::Weapon,
            ..
        } = *pickup
            && !groups.insert(group(item))
        {
            return Err(format!("a second weapon of group {}", group(item)));
        }
    }

    // The keys are where the key stage put them, and the game can be played
    // through with them.
    let Some(keys) = &game.keys else {
        return Err("items are randomized, but no keys were placed".into());
    };
    for placed in &keys.keys {
        let dir = placed
            .slot
            .room
            .split('/')
            .next()
            .unwrap()
            .to_ascii_lowercase();
        let slot = &game.items[&dir][&placed.slot.item];
        match slot.pickup {
            Pickup::Item {
                item,
                role: Role::Key,
                ..
            } if item == placed.key => {}
            other => return Err(format!("key {} became {other:?}", placed.key)),
        }
    }
    let key_slots = by_flag
        .values()
        .filter(|pickup| {
            matches!(
                pickup,
                Pickup::Item {
                    role: Role::Key,
                    ..
                }
            )
        })
        .count();
    if key_slots != keys.keys.len() {
        return Err(format!(
            "{key_slots} key slots for {} keys",
            keys.keys.len()
        ));
    }
    let key_options = KeyOptions::from_config(&game.config, game.player);
    check_playthrough(&prepared, &key_options, keys)
}

/// What a slot holds fits it: its group takes the item, the amount is within
/// the item's limit, and only a document left alone keeps the original.
fn check_pickup(
    map: &Map,
    game: &Game,
    slot: &arklay_biorand::map::Item,
    pickup: Pickup,
) -> Result<(), String> {
    match pickup {
        Pickup::Original => {
            let documents = game.config.get_bool("items/documents", false);
            if slot.document && !documents {
                Ok(())
            } else {
                Err("left as the original".into())
            }
        }
        Pickup::Empty => Ok(()),
        Pickup::Item { item, amount, role } => {
            let def = map.items.get(&item).ok_or("not in the catalogue")?;
            if slot.group & def.group != def.group {
                return Err(format!(
                    "group {:#x} doesn't take {} ({:#x})",
                    slot.group, def.name, def.group
                ));
            }
            match role {
                Role::Key => {
                    if !def.kind.starts_with("key/") {
                        return Err(format!("{} as a key", def.name));
                    }
                    if def.discard && amount == 0 {
                        return Err(format!("{} with no lock to open", def.name));
                    }
                    if !def.discard && amount != def.amount.map_or(1, u16::from) {
                        return Err(format!("{} x{amount}", def.name));
                    }
                }
                Role::Weapon | Role::Supply => {
                    if amount > def.max {
                        return Err(format!("{} x{amount}, above {}", def.name, def.max));
                    }
                    if role == Role::Supply && amount == 0 {
                        return Err(format!("{} x0", def.name));
                    }
                }
            }
            Ok(())
        }
    }
}

/// On the segmented default layout too, the walk is a real check: without
/// any one placed key, the play-through fails.
#[test]
fn re1_walk_catches_a_missing_key_across_segments() {
    let map = load_map();
    let config = Config::defaults(&config::definition(&map));
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
