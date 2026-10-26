//! `.akscr` checks over every room of a converted pack.
//!
//! Needs `ARKLAY_RE1_ROOT` and `ARKLAY_RE1_PACK`; without them the tests
//! skip. The checked-in `games/re1/room/<room>.akscr` scripts are edited by
//! hand: built as `AUTHENTIC` for each variant they must give that RDT's
//! init, main and event streams byte for byte, and their fixed build must
//! compile. Separately, the decompiler and merge must round-trip every room.

use crate::common;

use std::collections::{BTreeMap, BTreeSet};
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

/// The checked-in room scripts, beside their `inc/` include files.
fn room_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("games/re1/room")
}

/// Compile `text` (for `variant`, if given) into a standalone container and
/// decode it back, reading includes as a script in `games/re1/room` would.
fn compile(text: &str, variant: Option<akscr::Variant>) -> Scripts {
    let container = akscr::compile_with(text, variant, &akscr::includes_from(&room_dir()))
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

/// Every room RDT with scripts, by id, read from the pack.
fn original_rooms(pack: &Pack) -> BTreeMap<u16, Scripts> {
    let mut rooms = BTreeMap::new();
    for entry in pack.entries() {
        let path = entry.path();
        let Some(id) = path
            .strip_prefix("room/")
            .and_then(|name| name.strip_suffix(".rdt"))
            .and_then(|stem| u16::from_str_radix(stem, 16).ok())
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
        rooms.insert(id, original);
    }
    assert!(rooms.len() > 300, "only {} rooms with scripts", rooms.len());
    rooms
}

#[test]
#[ignore = "requires a real RE1 installation via ARKLAY_RE1_ROOT and ARKLAY_RE1_PACK"]
fn checked_in_room_scripts_build_the_original_bytecode() {
    let Some((_root, pack_path)) = common::asset_env() else {
        return;
    };
    let pack = Pack::open(&pack_path).unwrap();
    let checked_in = room_dir();
    let originals = original_rooms(&pack);

    let mut bases = BTreeSet::new();
    for (&id, original) in &originals {
        let base = akscr::merged_id(id);
        bases.insert(format!("{base:03x}"));
        let script = checked_in.join(format!("{base:03x}.akscr"));
        let text = std::fs::read_to_string(&script)
            .unwrap_or_else(|error| panic!("{}: {error}", script.display()));
        let compiled = compile(&text, Some(akscr::Variant::for_room(id).authentic()));
        assert_identical(
            &format!("{base:03x}.akscr as {id:04X}"),
            original,
            &compiled,
        );
        // The fixed build compiles too.
        compile(&text, Some(akscr::Variant::for_room(id)));
    }

    for entry in std::fs::read_dir(&checked_in).unwrap() {
        let path = entry.unwrap().path();
        if path
            .extension()
            .is_none_or(|extension| extension != "akscr")
        {
            continue;
        }
        let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
        assert!(
            bases.contains(&stem),
            "{} has no room in the pack",
            path.display()
        );
    }
}

#[test]
#[ignore = "requires a real RE1 installation via ARKLAY_RE1_ROOT and ARKLAY_RE1_PACK"]
fn every_room_decompiles_and_merges_to_identical_bytecode() {
    let Some((_root, pack_path)) = common::asset_env() else {
        return;
    };
    let pack = Pack::open(&pack_path).unwrap();
    let names = akscr::load_names(&room_dir()).unwrap();
    let originals = original_rooms(&pack);

    let mut texts = BTreeMap::new();
    for (&id, original) in &originals {
        let text = akscr::decompile_with(original, &names, id);
        assert_identical(
            &format!("room {id:04X}"),
            original,
            &compile(&text, Some(akscr::Variant::for_room(id).authentic())),
        );
        texts.insert(id, text);
    }

    let merged = akscr::merge_rooms(&texts).unwrap();
    for (&id, original) in &originals {
        let base = akscr::merged_id(id);
        let text = &merged[&base];
        assert_identical(
            &format!("merged {base:03x} as {id:04X}"),
            original,
            &compile(text, Some(akscr::Variant::for_room(id).authentic())),
        );
        compile(text, Some(akscr::Variant::for_room(id)));
    }
}
