//! The RE1 map that `scripts/biorand-convert-rdt.py` wrote reads back with the
//! counts the staging data has.

use std::collections::HashSet;
use std::fs;

use arklay_biorand::config::{self, Config};
use arklay_biorand::enemies::{self, EnemyPlan, Placement};
use arklay_biorand::inventory::{self, StartInventory};
use arklay_biorand::map::{Enemies, Map, Player, Spot};
use arklay_biorand::rng::Rng;

const RE1: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../games/re1");

/// Each room directory's name and the text of its `random.xml`, in name order.
fn room_files() -> Vec<(String, String)> {
    let mut files: Vec<(String, String)> = fs::read_dir(format!("{RE1}/room"))
        .expect("games/re1/room exists")
        .map(|entry| {
            let entry = entry.expect("a readable room entry");
            let dir = entry.file_name().into_string().expect("a UTF-8 room name");
            let text = fs::read_to_string(entry.path().join("random.xml"))
                .expect("a random.xml in every room directory");
            (dir, text)
        })
        .collect();
    files.sort();
    files
}

#[test]
fn re1_map_reads_with_the_staging_counts() {
    let files = room_files();
    assert_eq!(files.len(), 116, "room directories");
    let map_xml =
        fs::read_to_string(format!("{RE1}/biorand/map.xml")).expect("biorand/map.xml exists");
    let rooms = files
        .iter()
        .map(|(dir, text)| (dir.as_str(), text.as_str()));
    let map = Map::parse(&map_xml, rooms).unwrap_or_else(|err| panic!("{err:#}"));

    assert_eq!(map.begin_end.len(), 1, "begin-end");
    assert_eq!(map.items.len(), 76, "catalogue items");
    assert_eq!(map.enemies.len(), 10, "enemy groups");
    let entries: Vec<_> = map
        .enemies
        .iter()
        .flat_map(|group| &group.entries)
        .collect();
    assert_eq!(entries.len(), 11, "enemy entries");
    let poses: usize = entries.iter().map(|entry| entry.poses.len()).sum();
    assert_eq!(poses, 10, "enemy poses");

    assert_eq!(map.rooms.len(), 122, "logical rooms");
    let doors: Vec<_> = map.rooms.values().flat_map(|room| &room.doors).collect();
    assert_eq!(doors.len(), 283, "doors");
    let never_locked = doors
        .iter()
        .filter(|door| door.allowed_locks == Some(Vec::new()))
        .count();
    assert_eq!(never_locked, 76, "doors that may not be locked");
    let items: usize = map.rooms.values().map(|room| room.items.len()).sum();
    assert_eq!(items, 200, "room items");
    let flags: usize = map.rooms.values().map(|room| room.flags.len()).sum();
    assert_eq!(flags, 9, "graph flags");
    let rules: usize = map.rooms.values().map(|room| room.enemies.len()).sum();
    assert_eq!(rules, 140, "enemy rules");
    let spots: usize = map.spots.values().map(Vec::len).sum();
    assert_eq!(spots, 799, "enemy spots");

    // Door 8 of 303: its entrance is the one doors.csv gives, not the PoC's.
    let fountain = map.rooms["303"]
        .doors
        .iter()
        .find(|door| door.id == Some(8))
        .expect("303 door 8");
    assert_eq!(fountain.target.as_deref(), Some("305:0"));
    assert_eq!(fountain.entrance.map(|e| (e.x, e.z)), Some((8700, 5300)));
    // 207's door 1 is locked with the armor key.
    let armor_door = map.rooms["207"]
        .doors
        .iter()
        .find(|door| door.id == Some(1))
        .expect("207 door 1");
    assert_eq!(armor_door.key, Some(52));
    // The FLARE slot in 303 takes any item.
    assert!(
        map.rooms["303"]
            .items
            .iter()
            .any(|item| item.id == 2 && item.group == u32::MAX)
    );
    assert_eq!(map.spots["507"].len(), 9, "507 spots");
}

/// The seed every stage of the default game is planned with.
const SEED: u64 = 1;

#[test]
fn re1_plans_the_default_game_for_both_players() {
    let files = room_files();
    let map_xml =
        fs::read_to_string(format!("{RE1}/biorand/map.xml")).expect("biorand/map.xml exists");
    let rooms = files
        .iter()
        .map(|(dir, text)| (dir.as_str(), text.as_str()));
    let map = Map::parse(&map_xml, rooms).unwrap_or_else(|err| panic!("{err:#}"));
    let definition = config::definition(&map);
    let game_rooms: HashSet<String> = map.rooms.keys().cloned().collect();

    for (player, name, dropped) in [(Player::Chris, "Chris", 3), (Player::Jill, "Jill", 0)] {
        let mut defaults = Config::defaults(&definition);
        defaults.set("player", name);
        let settings = config::resolve(&definition, &defaults, &mut Rng::named(SEED, "config"));

        let start =
            inventory::randomize(&map, &settings, player, &mut Rng::named(SEED, "inventory"));
        check_inventory(&map, &start);

        let plan = enemies::randomize(
            &map,
            &settings,
            player,
            &game_rooms,
            &mut Rng::named(SEED, "enemy"),
        );
        check_plan(&map, &settings, player, &plan);
        // Half of the rooms with rules get enemies (`enemies/rooms` is 0.5 by
        // default), whatever the seed.
        assert_eq!(plan.rooms.len(), 48, "{name}: rooms with enemies");
        // In RE1 the only shortage is spots: a room asks for more enemies than its
        // directory's spots that take the kind allow. The seed gives Chris three.
        assert_eq!(plan.dropped, dropped, "{name}: creates with no spot");
        let again = enemies::randomize(
            &map,
            &settings,
            player,
            &game_rooms,
            &mut Rng::named(SEED, "enemy"),
        );
        assert_eq!(plan, again, "{name}: the same seed gives the same plan");
    }
}

/// The default config gives the knife first (`inventory/weapon/knife` is
/// `Always`), and no slot holds more than its item's maximum.
fn check_inventory(map: &Map, start: &StartInventory) {
    let knife = map
        .items
        .iter()
        .find(|(_, def)| def.kind == "weapon/knife")
        .map(|(id, _)| *id)
        .expect("a knife in the catalogue");
    assert_eq!(start.weapons.first(), Some(&knife), "the knife comes first");
    for slot in &start.slots {
        let def = &map.items[&slot.item];
        assert!(
            slot.amount <= def.max,
            "item {} holds at most {}",
            slot.item,
            def.max
        );
    }
    for weapon in &start.weapons {
        assert!(
            map.items[weapon].kind.starts_with("weapon/"),
            "item {weapon} is a weapon"
        );
    }
}

/// Each change fits the room: a retype changes one of its fixed enemies, and a
/// create takes a spot of the room's directory, a slot no rule uses and a kill
/// flag no rule in the game uses. Kill flags are distinct across the plan.
fn check_plan(map: &Map, settings: &Config, player: Player, plan: &EnemyPlan) {
    let door_rando = settings.get_bool("doors/random", false);
    let used_flags: HashSet<u16> = map
        .rooms
        .values()
        .filter(|room| room.filter.includes(door_rando, player))
        .flat_map(|room| &room.enemies)
        .filter(|rule| rule.filter.includes(door_rando, player))
        .filter_map(|rule| rule.flag)
        .collect();
    let mut kill_flags = HashSet::new();
    for (key, planned) in &plan.rooms {
        let room = &map.rooms[key.as_str()];
        assert!(
            room.filter.includes(door_rando, player),
            "{key} is in the game"
        );
        assert_eq!(
            planned.directory,
            room.room_id().to_ascii_lowercase(),
            "{key}"
        );
        let entry = map
            .enemies
            .iter()
            .flat_map(|group| &group.entries)
            .find(|entry| entry.key == planned.kind)
            .expect("the kind is in the catalogue");
        assert_eq!(planned.esp, entry.esp, "{key}: the kind's effect sheets");
        let rules: Vec<&Enemies> = room
            .enemies
            .iter()
            .filter(|rule| rule.filter.includes(door_rando, player))
            .collect();
        let rule_ids: Vec<u8> = rules.iter().filter_map(|rule| rule.id).collect();
        let mut slots = Vec::new();
        let mut spots: Vec<&Spot> = Vec::new();
        for placement in &planned.placements {
            match placement {
                Placement::Retype { slot, enemy } => {
                    assert!(
                        rules
                            .iter()
                            .any(|rule| rule.id == Some(*slot)
                                && rule.kind.as_deref() != Some("npc")),
                        "{key}: retypes a rule of the room"
                    );
                    assert!(entry.ids.contains(enemy), "{key}: an id of the kind");
                }
                Placement::Create {
                    slot,
                    enemy,
                    pose,
                    kill_flag,
                    spot,
                } => {
                    assert!(*slot < 16 && !rule_ids.contains(slot), "{key}: a free slot");
                    assert!(entry.ids.contains(enemy), "{key}: an id of the kind");
                    assert!(
                        *pose == 0 || entry.poses.iter().any(|p| p.pose == *pose),
                        "{key}: a pose of the kind"
                    );
                    assert!(
                        map.spots_for(room).contains(spot),
                        "{key}: a spot of its directory"
                    );
                    assert!(
                        !used_flags.contains(&u16::from(*kill_flag)),
                        "{key}: a kill flag no rule uses"
                    );
                    assert!(
                        kill_flags.insert(*kill_flag),
                        "kill flag {kill_flag} is used once"
                    );
                    assert!(!spots.contains(&spot), "{key}: a spot is used once");
                    spots.push(spot);
                    slots.push(*slot);
                }
            }
        }
        slots.sort_unstable();
        assert!(
            slots.windows(2).all(|pair| pair[0] != pair[1]),
            "{key}: a slot is used once"
        );
    }
}
