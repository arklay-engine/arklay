//! `.akscr` checks over every room of an installation.
//!
//! Needs `ARKLAY_RE1_ROOT` and `ARKLAY_RE1_PACK`; without them the tests
//! skip. The checked-in `games/re1/room/<room>/room.akscr` scripts are
//! edited by hand: each covers rooms of the install, and its runtime build
//! and its build for each RDT variant and region compile. Separately, the
//! decompiler and merge must round-trip every room, and with `ARKLAY_RE1_JPN_ROOT` and `ARKLAY_RE1_EUR_ROOT` set
//! to installs of those regions beside a USA `ARKLAY_RE1_ROOT`, the merge of
//! the three regions must too.

use crate::common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

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

/// Every room RDT with scripts in the install under `root`, by id.
fn original_rooms(root: &Path) -> BTreeMap<u16, Scripts> {
    let rooms = arklay::convert::installed_room_scripts(root).unwrap();
    assert!(rooms.len() > 300, "only {} rooms with scripts", rooms.len());
    rooms
}

#[test]
#[ignore = "requires a real RE1 installation via ARKLAY_RE1_ROOT and ARKLAY_RE1_PACK"]
fn checked_in_room_scripts_cover_the_installs_rooms_and_compile() {
    let Some((root, _pack_path)) = common::asset_env() else {
        return;
    };
    let checked_in = room_dir();
    let originals = original_rooms(&root);
    let mut bases = BTreeSet::new();
    for &id in originals.keys() {
        let base = akscr::merged_id(id);
        let script = checked_in.join(akscr::room_script_file(base));
        let text = std::fs::read_to_string(&script)
            .unwrap_or_else(|error| panic!("{}: {error}", script.display()));
        for region in akscr::Region::ALL {
            compile(&text, Some(akscr::Variant::for_room(id).in_region(region)));
        }
        if bases.insert(format!("{base:03x}")) {
            // The runtime build, which ships.
            compile(&text, None);
        }
    }

    for entry in std::fs::read_dir(&checked_in).unwrap() {
        let path = entry.unwrap().path().join(akscr::ROOM_SCRIPT_FILE);
        if !path.is_file() {
            continue;
        }
        let folder = path
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy();
        let stem = folder.to_string();
        assert!(
            bases.contains(&stem),
            "{} has no room in the install",
            path.display()
        );
    }
}

#[test]
#[ignore = "requires a real RE1 installation via ARKLAY_RE1_ROOT and ARKLAY_RE1_PACK"]
fn every_room_decompiles_and_merges_to_identical_bytecode() {
    let Some((root, _pack_path)) = common::asset_env() else {
        return;
    };
    let names = akscr::load_names(&room_dir()).unwrap();
    let originals = original_rooms(&root);

    let mut texts = BTreeMap::new();
    for (&id, original) in &originals {
        let text = akscr::decompile_with(original, &names, id);
        assert_identical(
            &format!("room {id:04X}"),
            original,
            &compile(&text, Some(akscr::Variant::for_room(id))),
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
            &compile(text, Some(akscr::Variant::for_room(id))),
        );
    }
    // The merged scripts hold every variant in one runtime build.
    for text in merged.values() {
        compile(text, None);
    }
}

#[test]
#[ignore = "requires every RE1 region via ARKLAY_RE1_ROOT, ARKLAY_RE1_PACK, ARKLAY_RE1_JPN_ROOT and ARKLAY_RE1_EUR_ROOT"]
fn every_region_merges_to_identical_bytecode() {
    let Some((root, _pack_path)) = common::asset_env() else {
        return;
    };
    let (Some(jpn), Some(eur)) = (
        std::env::var_os("ARKLAY_RE1_JPN_ROOT"),
        std::env::var_os("ARKLAY_RE1_EUR_ROOT"),
    ) else {
        return;
    };
    let names = akscr::load_names(&room_dir()).unwrap();
    let regions: Vec<(akscr::Region, BTreeMap<u16, Scripts>)> = [
        (akscr::Region::Usa, root.as_os_str()),
        (akscr::Region::Jpn, &jpn),
        (akscr::Region::Eur, &eur),
    ]
    .into_iter()
    .map(|(region, path)| (region, original_rooms(Path::new(path))))
    .collect();
    let texts: Vec<(akscr::Region, BTreeMap<u16, String>)> = regions
        .iter()
        .map(|(region, rooms)| {
            let texts = rooms
                .iter()
                .map(|(&id, scripts)| (id, akscr::decompile_with(scripts, &names, id)))
                .collect();
            (*region, texts)
        })
        .collect();

    let merged = akscr::merge_regions(
        &texts
            .iter()
            .map(|(region, texts)| (*region, texts))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    for (region, originals) in &regions {
        for (&id, original) in originals {
            let base = akscr::merged_id(id);
            let variant = akscr::Variant::for_room(id).in_region(*region);
            assert_identical(
                &format!("merged {base:03x} as {id:04X} for the {region} region"),
                original,
                &compile(&merged[&base], Some(variant)),
            );
        }
    }
}
