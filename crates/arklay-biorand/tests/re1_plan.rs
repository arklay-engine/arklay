//! Whole games planned on the real RE1 map, for both players, many seeds and
//! a few configurations, each checked against the map it was planned on.

mod common;

use std::collections::{HashMap, HashSet};

use arklay_biorand::config::{self, Config};
use arklay_biorand::doors;
use arklay_biorand::items::{Pickup, Role};
use arklay_biorand::keys::KeyOptions;
use arklay_biorand::map::Map;
use arklay_biorand::plan::{Game, Players, generate};
use arklay_biorand::prepare;
use arklay_biorand::rng::Rng;

use common::{check_playthrough, load_map, with_doors, with_locks};

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

/// The plan's locks hold together: every door of the map is planned once,
/// both sides of a pair share a lock, a new lock takes a lock flag no
/// original room script raises and a key that may go there, and a door
/// leading nowhere never opens.
fn check_locks(map: &Map, game: &Game) -> Result<(), String> {
    let prepared = game_map(map, game)?;
    let doors: usize = prepared
        .rooms
        .values()
        .map(|room| room.doors.iter().filter(|door| door.id.is_some()).count())
        .sum();
    if game.locks.len() != doors {
        return Err(format!("{} door locks for {doors} doors", game.locks.len()));
    }
    let by_name: HashMap<String, &arklay_biorand::locks::DoorLock> = game
        .locks
        .iter()
        .map(|lock| (format!("{}:{}", lock.room, lock.id), lock))
        .collect();
    let mut flags: HashMap<u8, HashSet<String>> = HashMap::new();
    for lock in &game.locks {
        let name = format!("{}:{}", lock.room, lock.id);
        if lock.lock == 0 && lock.key != 0 {
            return Err(format!("{name} is unlocked with key {}", lock.key));
        }
        if lock.target.is_none() && (lock.lock, lock.key) != (255, 255) {
            return Err(format!("{name} leads nowhere but opens"));
        }
        if let Some(key) = lock.key_item() {
            let def = map
                .items
                .get(&key)
                .ok_or_else(|| format!("{name}: key {key}"))?;
            if lock.drawn && !def.discard {
                return Err(format!(
                    "{name}: drawn key {} is not a discard key",
                    def.name
                ));
            }
        }
        if lock.drawn && lock.lock != 0 {
            let other = lock
                .other
                .as_ref()
                .and_then(|other| by_name.get(other))
                .ok_or_else(|| format!("{name} is drawn without a far side"))?;
            if other.lock != lock.lock
                || (other.key_item() != lock.key_item() && other.key != 255 && lock.key != 255)
            {
                return Err(format!("{name} {lock:?} and its far side {other:?} differ"));
            }
            flags.entry(lock.lock).or_default().insert(name.clone());
        }
    }
    for (flag, doors) in &flags {
        if doors.len() > 2 {
            return Err(format!("lock flag {flag} is shared by {doors:?}"));
        }
        if !game.config.get_bool("locks/preserve", false) && *flag < 64 {
            return Err(format!(
                "new lock flag {flag} (of {doors:?}) is one a room script may raise"
            ));
        }
    }
    Ok(())
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

/// The map prepared for a game's configuration, with the original doors.
fn prepared(map: &Map, game: &Game) -> Result<Map, String> {
    let options = prepare::Options::from_config(&game.config, game.player);
    prepare::prepare(map, &options, &mut Rng::named(game.spoiler.seed, "re1map"))
        .map_err(|err| format!("{err:#}"))
}

/// The map a game was planned on: prepared for its configuration, and
/// rewired as its door plan says.
fn game_map(map: &Map, game: &Game) -> Result<Map, String> {
    let prepared = prepared(map, game)?;
    match &game.doors {
        Some(plan) => with_doors(&prepared, plan),
        None => Ok(prepared),
    }
}

/// Checks one game against the map it was planned on.
fn check_game(map: &Map, game: &Game) -> Result<(), String> {
    let prepared = game_map(map, game)?;

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
    let walked = with_locks(&prepared, &game.locks)?;
    check_playthrough(&walked, &key_options, keys)
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

/// The door plan holds together: every room has one new id, numbered from 0
/// in each stage, none is 311's; every door of the game is planned once and
/// leads to a door of the game that leads back; the far side's entrance is
/// the map's; the start room is in the first stage.
fn check_doors(map: &Map, game: &Game) -> Result<(), String> {
    let plan = game
        .doors
        .as_ref()
        .ok_or("doors are randomized, but no plan")?;
    let original = prepared(map, game)?;
    let walked = game_map(map, game)?;
    let mut ids = HashSet::new();
    let mut numbers: HashMap<u8, Vec<u8>> = HashMap::new();
    for (dir, room) in &plan.rooms {
        if !std::path::Path::new(&format!("{}/room/{dir}", common::RE1)).is_dir() {
            return Err(format!("room {dir} has no folder"));
        }
        if room.id != doors::room_id(room.stage, room.room) {
            return Err(format!("{dir}: id {} for {room:?}", room.id));
        }
        if room.id == "311" || !ids.insert(room.id.clone()) {
            return Err(format!("{dir}: id {} is taken", room.id));
        }
        if room.stage == 0 || room.stage > plan.segments {
            return Err(format!("{dir}: stage {} of {}", room.stage, plan.segments));
        }
        numbers.entry(room.stage).or_default().push(room.room);
    }
    for (stage, mut rooms) in numbers {
        rooms.sort_unstable();
        let expected: Vec<u8> = (0..=u8::MAX)
            .filter(|&n| doors::room_id(stage, n) != "311")
            .take(rooms.len())
            .collect();
        if rooms != expected {
            return Err(format!("stage {stage} numbers its rooms {rooms:?}"));
        }
    }
    let start = &map.begin_end[0].start;
    match plan.rooms.get(&start.to_ascii_lowercase()) {
        Some(room) if room.stage == 1 => {}
        other => return Err(format!("the start room {start} is {other:?}")),
    }

    let planned_doors: usize = walked
        .rooms
        .values()
        .map(|room| room.doors.iter().filter(|door| door.id.is_some()).count())
        .sum();
    if planned_doors != plan.doors.len() {
        return Err(format!(
            "{} planned doors for {planned_doors}",
            plan.doors.len()
        ));
    }
    let by_name: HashMap<(&str, u8), &doors::PlannedDoor> = plan
        .doors
        .iter()
        .map(|door| ((door.room.as_str(), door.id), door))
        .collect();
    if by_name.len() != plan.doors.len() {
        return Err("a door is planned twice".into());
    }
    for door in &plan.doors {
        let name = format!("{}:{}", door.room, door.id);
        if !plan.rooms.contains_key(&door.dir) {
            return Err(format!("{name}: room {} has no new id", door.dir));
        }
        let Some(target) = &door.target else {
            if door.no_return {
                return Err(format!("{name} ends a segment but leads nowhere"));
            }
            continue;
        };
        let far = by_name
            .get(&(target.room.as_str(), target.door))
            .ok_or_else(|| {
                format!(
                    "{name} leads to {}:{}, not in the game",
                    target.room, target.door
                )
            })?;
        if plan.new_id(&target.dir) != Some(target.id.as_str()) {
            return Err(format!(
                "{name}: target id {} for {}",
                target.id, target.dir
            ));
        }
        let back = far.target.as_ref().map(|t| (t.room.as_str(), t.door));
        if back != Some((door.room.as_str(), door.id)) {
            return Err(format!(
                "{name} leads to {}:{}, which leads to {back:?}",
                far.room, far.id
            ));
        }
        let entrance = original.rooms[&target.room]
            .doors
            .iter()
            .find(|d| d.id == Some(target.door))
            .and_then(|d| d.entrance);
        if entrance != Some(target.entrance) {
            return Err(format!(
                "{name}: entrance {:?} for {entrance:?}",
                target.entrance
            ));
        }
        if !door.randomized {
            let was = original.rooms[&door.room]
                .doors
                .iter()
                .find(|d| d.id == Some(door.id))
                .and_then(|d| d.target.clone());
            if was != Some(format!("{}:{}", target.room, target.door)) {
                return Err(format!("{name} is fixed but leads elsewhere than {was:?}"));
            }
        }
    }
    Ok(())
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
