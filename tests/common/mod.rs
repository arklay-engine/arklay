//! Shared helpers for the ignored real-asset integration tests.

use std::path::{Path, PathBuf};

use arklay::manifest;
use arklay::pack::{Pack, PackWriter};

/// Resolve the asset environment the real-asset tests share.
///
/// Both `ARKLAY_RE1_ROOT` and `ARKLAY_RE1_PACK` must be configured together:
/// with neither set the ignored test skips, but a partial configuration fails
/// loudly instead of silently passing.
pub fn asset_env() -> Option<(PathBuf, PathBuf)> {
    let root = std::env::var("ARKLAY_RE1_ROOT").ok();
    let pack = std::env::var("ARKLAY_RE1_PACK").ok();
    match (root, pack) {
        (None, None) => None,
        (Some(_), None) => panic!(
            "ARKLAY_RE1_ROOT is set but ARKLAY_RE1_PACK is not; set both to run the real-asset tests"
        ),
        (None, Some(_)) => panic!(
            "ARKLAY_RE1_PACK is set but ARKLAY_RE1_ROOT is not; set both to run the real-asset tests"
        ),
        (Some(root), Some(pack)) => Some((PathBuf::from(root), PathBuf::from(pack))),
    }
}

/// Open the shipped pack with the checked-in enemy scripts layered over it.
///
/// The shared pack may have been converted from an older tree, so a test that
/// must exercise the repository's Lua shadows the pack's `enemy/**.lua` and
/// `lib/**.lua` copies with the `ENEMY_SCRIPTS` the build embeds. Rooms and models still come from
/// the real pack.
// The soak binary includes this module without calling it.
#[allow(dead_code)]
pub fn enemy_scripts_pack(pack_path: &Path) -> Pack {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "arklay-enemy-scripts-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create the script overlay dir");
    let mod_path = dir.join("enemy-scripts.akpak");
    let mut writer = PackWriter::new();
    let manifest = manifest::Manifest {
        kind: manifest::PackKind::Mod,
        base: Some("re1".to_string()),
        load_order: 10,
        ..manifest::Manifest::base("enemy-scripts")
    }
    .render()
    .into_bytes();
    writer
        .add(manifest::ENTRY, manifest)
        .expect("write manifest");
    for (path, source) in arklay::enemy::ENEMY_SCRIPTS {
        writer
            .add(path, source.as_bytes().to_vec())
            .expect("write script entry");
    }
    writer.write(&mod_path).expect("write overlay pack");
    Pack::open_layered(pack_path, std::slice::from_ref(&mod_path)).expect("open layered pack")
}

/// RDT `id`'s own scripts: the checked-in room script built for that RDT
/// variant, as `AUTHENTIC`, in the region `pack` was converted from. A pack
/// carries each room's runtime build, which holds every variant's lines;
/// this is the one RDT's. `None` for a room without a script.
#[allow(dead_code)]
pub fn rdt_scripts(pack: &Pack, id: arklay::state::RoomId) -> Option<arklay::scd::ir::Scripts> {
    use arklay::scd::akscr;
    let number = u16::try_from(id.rdt_number()).ok()?;
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/re1/room");
    let text = std::fs::read_to_string(dir.join(akscr::room_script_file(akscr::merged_id(number))))
        .ok()?;
    let region = pack
        .manifest()
        .and_then(|manifest| manifest.region)
        .unwrap_or(akscr::Region::Usa);
    let variant = akscr::Variant::for_room(number)
        .in_region(region)
        .authentic();
    let container = akscr::compile_with(&text, Some(variant), &akscr::includes_from(&dir))
        .and_then(|assembled| assembled.to_container())
        .unwrap_or_else(|error| panic!("{id:?}: {error:#}"));
    Some(arklay::scd::reader::parse(&container).expect("a compiled container parses"))
}

/// The RE1 room the original game names `name` (`6051`, or three digits
/// for player 0, `70c`): its stages 6 and 7 are the second mansion visit.
#[allow(dead_code)]
pub fn original_room(name: &str) -> arklay::state::RoomId {
    let digits = if name.len() == 3 {
        format!("{name}0")
    } else {
        name.to_string()
    };
    let number = u16::from_str_radix(&digits, 16).unwrap_or_else(|_| panic!("room `{name}`"));
    arklay::state::RoomId::from_original(number).unwrap_or_else(|| panic!("room `{name}`"))
}
