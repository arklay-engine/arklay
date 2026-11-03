//! The RE1 map that `scripts/biorand-convert-rdt.py` wrote reads back with the
//! counts the staging data has.

use std::fs;

use arklay_biorand::map::Map;

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
    let spots: usize = map.rooms.values().map(|room| room.spots.len()).sum();
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
    assert_eq!(map.rooms["507/STORE"].spots.len(), 9, "507/STORE spots");
}
