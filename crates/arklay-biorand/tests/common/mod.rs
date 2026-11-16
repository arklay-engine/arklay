//! What the RE1 integration tests share: loading the real map, and a
//! play-through of a key placement that walks the map itself rather than the
//! routing graph.

#![allow(dead_code)]

use std::fs;

use arklay_biorand::map::Map;

pub const RE1: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../games/re1");

/// The RE1 map as the room files and `biorand/map.xml` give it.
pub fn load_map() -> Map {
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
    let map_xml =
        fs::read_to_string(format!("{RE1}/biorand/map.xml")).expect("biorand/map.xml exists");
    let rooms = files
        .iter()
        .map(|(dir, text)| (dir.as_str(), text.as_str()));
    Map::parse(&map_xml, rooms).unwrap_or_else(|err| panic!("{err:#}"))
}

#[allow(unused_imports)]
pub use arklay_biorand::audit::*;
