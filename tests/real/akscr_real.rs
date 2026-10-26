//! `.akscr` checks over every room of a converted pack.
//!
//! Needs `ARKLAY_RE1_ROOT` and `ARKLAY_RE1_PACK`; without them the tests
//! skip. The checked-in `games/re1/room/<room>.akscr` scripts are edited by
//! hand: built as `AUTHENTIC` for each variant they must give that RDT's
//! init, main and event streams byte for byte (unless the RDT is another
//! release's, see [`OTHER_RELEASES`]), and their fixed build must compile. Separately, the decompiler and merge must round-trip every room.

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

/// RDTs of another release whose scripts differ from the ones the checked-in
/// scripts were written from (the Steam release fixes a few rooms), by id
/// and [`Scripts::digest`]. Such a room keeps its own scripts, so its script
/// is not compared with it; any other mismatch is a failure.
const OTHER_RELEASES: &[(u16, u64)] = &[
    (0x1001, 0xB8BE24E418F124E9),
    (0x1061, 0xACF7413031CAB705),
    (0x10F0, 0xF60A04CBAC0D16F3),
    (0x1120, 0x26906221B4B6771D),
    (0x1121, 0x26906221B4B6771D),
    (0x20A0, 0xEB18BDCA6FCE01BF),
    (0x20A1, 0xF3C57FE2B338FCCC),
    (0x20F1, 0x658A11DADC1D4E67),
    (0x3010, 0x53E61CBE377A786E),
    (0x3011, 0x53E61CBE377A786E),
    (0x30C0, 0x28DFFC3A8A2DE02F),
    (0x30C1, 0x28DFFC3A8A2DE02F),
    (0x30E0, 0xD6558389A1D3631F),
    (0x30E1, 0xD6558389A1D3631F),
    (0x30F0, 0x366830B1E5FD4922),
    (0x30F1, 0x366830B1E5FD4922),
    (0x4040, 0x1EDDC48B92ED4AEC),
    (0x4041, 0x1EDDC48B92ED4AEC),
    (0x4050, 0x0467E9E7064F7BAD),
    (0x4051, 0xD38259763CDEF0D1),
    (0x5091, 0xA3AF68EA738FF0CE),
    (0x50E0, 0x73DEAB3853388CBB),
    (0x50E1, 0x73DEAB3853388CBB),
    (0x5131, 0x9CD81B650EFF7850),
    (0x6001, 0xFA8B618BF9619F34),
    (0x6061, 0x7A262D06463C92C0),
    (0x60A0, 0x66DF0DD733D68694),
    (0x60C0, 0x271228BB3E38096A),
    (0x60C1, 0xAD7543D56BB4E87A),
    (0x6120, 0x26906221B4B6771D),
    (0x6121, 0x26906221B4B6771D),
    (0x6191, 0xBCB1A5BBB254E37B),
    (0x70A0, 0xEB18BDCA6FCE01BF),
    (0x70A1, 0xF3C57FE2B338FCCC),
    (0x70F1, 0x658A11DADC1D4E67),
    (0x7180, 0x21727B2872FD642A),
    (0x7181, 0x21727B2872FD642A),
];

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
    let mut unknown = Vec::new();
    let mut first_mismatch = None;
    for (&id, original) in &originals {
        let base = akscr::merged_id(id);
        bases.insert(format!("{base:03x}"));
        let script = checked_in.join(format!("{base:03x}.akscr"));
        let text = std::fs::read_to_string(&script)
            .unwrap_or_else(|error| panic!("{}: {error}", script.display()));
        let compiled = compile(&text, Some(akscr::Variant::for_room(id).authentic()));
        // The fixed build compiles too.
        compile(&text, Some(akscr::Variant::for_room(id)));
        if compiled.digest() == original.digest()
            || OTHER_RELEASES.contains(&(id, original.digest()))
        {
            continue;
        }
        unknown.push(format!("    (0x{id:04X}, 0x{:016X}),", original.digest()));
        first_mismatch.get_or_insert((id, base, compiled));
    }
    if let Some((id, base, compiled)) = first_mismatch {
        eprintln!(
            "RDTs that do not build from their script; if they are another \
             release's, list them in OTHER_RELEASES:\n{}",
            unknown.join("\n")
        );
        assert_identical(
            &format!("{base:03x}.akscr as {id:04X}"),
            &originals[&id],
            &compiled,
        );
        unreachable!("the digests differ, so the bytes do");
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
