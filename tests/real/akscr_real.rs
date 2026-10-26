//! `.akscr` round trip over every room of a converted pack.
//!
//! Needs `ARKLAY_RE1_ROOT` and `ARKLAY_RE1_PACK`; without them the test skips.
//! Every room RDT with scripts must decompile to a script that compiles back
//! to byte-identical init, main and event streams, and the checked-in
//! `games/re1/room/<id>.akscr` must be exactly that script.

use crate::common;

use std::collections::BTreeSet;
use std::path::Path;

use arklay::pack::Pack;
use arklay::scd::akscr;
use arklay::scd::ir::{Block, Scripts, Stream};
use arklay::scd::reader;

fn block_bytes(block: &Block) -> Vec<u8> {
    let mut bytes: Vec<u8> = block
        .insns
        .iter()
        .flat_map(|insn| insn.bytes.iter().copied())
        .collect();
    bytes.extend_from_slice(&block.trailing);
    bytes
}

fn event_bytes(stream: &Stream) -> Vec<u8> {
    let mut bytes: Vec<u8> = stream
        .insns
        .iter()
        .flat_map(|insn| insn.bytes.iter().copied())
        .collect();
    bytes.extend_from_slice(&stream.trailing);
    bytes
}

/// Compile `text` into a standalone container and decode it back.
fn compile(text: &str) -> Scripts {
    let container = akscr::compile(text)
        .and_then(|assembled| assembled.to_container())
        .unwrap();
    reader::parse(&container).unwrap()
}

/// Assert both script sets hold identical streams.
fn assert_identical(entry: &str, original: &Scripts, compiled: &Scripts) {
    for (name, left, right) in [
        ("init", &original.init, &compiled.init),
        ("main", &original.main, &compiled.main),
    ] {
        assert_eq!(left.len(), right.len(), "{entry}: {name} block count");
        for (index, (left, right)) in left.iter().zip(right).enumerate() {
            assert_eq!(
                block_bytes(left),
                block_bytes(right),
                "{entry}: {name} block {index}"
            );
        }
    }
    assert_eq!(
        original.events.len(),
        compiled.events.len(),
        "{entry}: event count"
    );
    for (index, (left, right)) in original.events.iter().zip(&compiled.events).enumerate() {
        assert_eq!(
            event_bytes(left),
            event_bytes(right),
            "{entry}: event_{index:02X}"
        );
    }
}

#[test]
#[ignore = "requires a real RE1 installation via ARKLAY_RE1_ROOT and ARKLAY_RE1_PACK"]
fn every_room_script_compiles_to_identical_bytecode() {
    let Some((_root, pack_path)) = common::asset_env() else {
        return;
    };
    let pack = Pack::open(&pack_path).unwrap();
    let checked_in = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/re1/room");

    let mut rooms = BTreeSet::new();
    for entry in pack.entries() {
        let path = entry.path();
        let Some(stem) = path
            .strip_prefix("room/")
            .and_then(|name| name.strip_suffix(".rdt"))
        else {
            continue;
        };
        let data = pack.read(path).unwrap();
        // The stub RDTs of unused room slots carry no script pointers.
        let Ok(original) = reader::parse(&data) else {
            assert!(
                data.len() < 0x94,
                "{path} has a header but no readable scripts"
            );
            continue;
        };
        let text = akscr::decompile(&original);
        assert_identical(path, &original, &compile(&text));

        let script = checked_in.join(format!("{stem}.akscr"));
        let stored = std::fs::read_to_string(&script)
            .unwrap_or_else(|error| panic!("{}: {error}", script.display()));
        assert!(
            stored == text,
            "{} is out of date; re-export it with `arklay scd export`",
            script.display()
        );
        rooms.insert(stem.to_string());
    }
    assert!(rooms.len() > 300, "only {} rooms with scripts", rooms.len());

    for entry in std::fs::read_dir(&checked_in).unwrap() {
        let path = entry.unwrap().path();
        let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
        assert!(
            rooms.contains(&stem),
            "{} has no room in the pack",
            path.display()
        );
    }
}
