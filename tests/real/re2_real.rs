//! RE2 conversion against the retail PC install: `ARKLAY_RE2_ROOT` names the
//! directory holding `data/` (or `data/` itself). Without it every test here
//! skips. The RE1 half of the detection census also reads `ARKLAY_RE1_ROOT`
//! when it is set.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use arklay::convert::{Game, convert_game_as};
use arklay::format::{self, Detection, Dialect};
use arklay::pack::Pack;
use arklay::verify;

fn re2_root() -> Option<PathBuf> {
    std::env::var_os("ARKLAY_RE2_ROOT").map(PathBuf::from)
}

/// The retail tree converted once for the whole module.
fn pack() -> Option<&'static Pack> {
    static PACK: OnceLock<Pack> = OnceLock::new();
    let root = re2_root()?;
    Some(PACK.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("arklay-re2-real-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let out = dir.join("re2.akpak");
        let jobs = std::thread::available_parallelism().map_or(1, usize::from);
        convert_game_as(&root, &out, Some(Game::Re2), jobs, false).unwrap();
        let pack = Pack::open(&out).unwrap();
        // The pack is memory-mapped or read whole; either way the file can go.
        let _ = fs::remove_dir_all(&dir);
        pack
    }))
}

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files(&path, out);
        } else {
            out.push(path);
        }
    }
}

fn with_extension(root: &Path, extension: &str) -> Vec<PathBuf> {
    let mut all = Vec::new();
    files(root, &mut all);
    all.retain(|path| {
        path.extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case(extension))
    });
    all.sort();
    all
}

fn name(path: &Path) -> String {
    path.file_name()
        .unwrap()
        .to_string_lossy()
        .to_ascii_uppercase()
}

#[test]
#[ignore]
fn the_retail_tree_converts_every_category() {
    let Some(pack) = pack() else {
        return;
    };
    let mut categories = BTreeMap::<&str, usize>::new();
    for path in pack.paths() {
        let category = match path.split('/').collect::<Vec<_>>().as_slice() {
            ["room", _, "cut", _] => "cut",
            [first, ..] => first.trim_end_matches(".toml"),
            [] => unreachable!(),
        };
        *categories.entry(category).or_default() += 1;
    }
    let expected = [
        ("bgm", 118),
        ("cut", 5028),
        ("demo", 6),
        ("door", 55),
        ("enemy", 234),
        ("manifest", 1),
        ("movie", 26),
        ("player", 221),
        ("room", 501),
        ("voice", 599),
    ];
    assert_eq!(categories, BTreeMap::from(expected));

    // `roomcut.bin` holds 1242 distinct streams for the 5028 in-range
    // cameras; some of those differ only in encoding and decode to the same
    // 1205 images.
    let mut distinct = HashSet::new();
    for path in pack.paths().filter(|path| path.contains("/cut/")) {
        let data = pack.read(path).unwrap();
        let mut hasher = DefaultHasher::new();
        data.hash(&mut hasher);
        distinct.insert(hasher.finish());
    }
    assert_eq!(distinct.len(), 1205);
    let image = arklay::bmp::decode(&pack.read("room/1000/cut/00.bmp").unwrap()).unwrap();
    assert_eq!((image.width, image.height), (320, 240));

    let manifest = pack.manifest().unwrap();
    assert_eq!(manifest.id, "re2");
    assert_eq!(manifest.language, None);
}

#[test]
#[ignore]
fn the_converted_pack_verifies() {
    let Some(pack) = pack() else {
        return;
    };
    let report = verify::verify_pack(pack, false);
    assert!(report.valid(), "{:?}", report.failures);
    let count = |format: verify::Format| {
        report
            .formats
            .get(&format)
            .map_or(0, |stats| stats.entries as usize)
    };
    assert_eq!(count(verify::Format::Re2Rdt), 501);
    assert_eq!(count(verify::Format::Re2Emd), 116);
    assert_eq!(count(verify::Format::AdpcmWav), 717);
    assert_eq!(count(verify::Format::Mpeg), 26);
    // Strict mode fails exactly the formats nothing parses yet, plus the
    // opaque demo reels.
    let strict = verify::verify_pack(pack, true);
    assert!(
        strict
            .failures
            .iter()
            .all(|failure| failure.format.unsupported() || failure.path.starts_with("demo/")),
        "{:?}",
        strict.failures
    );
}

#[test]
#[ignore]
fn detection_separates_the_dialects() {
    let Some(re2) = re2_root() else {
        return;
    };
    let mut wrong = Vec::new();
    for path in with_extension(&re2, "rdt") {
        let detection = format::rdt(&fs::read(&path).unwrap());
        // The retail ROOM7051.RDT is a broken Claire-only stub.
        let expected = if name(&path) == "ROOM7051.RDT" {
            detection
        } else {
            Detection::Dialect(Dialect::Re2)
        };
        if detection != expected {
            wrong.push((path, detection));
        }
    }
    for path in with_extension(&re2, "emd") {
        let detection = format::emd(&fs::read(&path).unwrap());
        if detection != Detection::Dialect(Dialect::Re2) {
            wrong.push((path, detection));
        }
    }
    if let Some(re1) = std::env::var_os("ARKLAY_RE1_ROOT").map(PathBuf::from) {
        for path in with_extension(&re1, "rdt") {
            let data = fs::read(&path).unwrap();
            // RE1 ships four-byte placeholder RDTs; those are nothing at all.
            let expected = if data.len() <= 4 {
                Detection::Unknown
            } else {
                Detection::Dialect(Dialect::Re1)
            };
            let detection = format::rdt(&data);
            if detection != expected {
                wrong.push((path, detection));
            }
        }
        for path in with_extension(&re1, "emd") {
            let detection = format::emd(&fs::read(&path).unwrap());
            if detection != Detection::Dialect(Dialect::Re1) {
                wrong.push((path, detection));
            }
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Every converted RE2 room reads into a room with cameras and collision,
/// and every camera but the retail data's known gaps has a background.
#[test]
#[ignore]
fn every_room_loads_with_its_backgrounds() {
    let Some(pack) = pack() else {
        return;
    };
    let ids = arklay::room::ids(pack);
    assert_eq!(ids.len(), 501, "every converted RDT");
    let (mut cameras, mut missing) = (0, Vec::new());
    for id in ids {
        let loaded = arklay::room::load(pack, id)
            .unwrap_or_else(|error| panic!("room {}: {error:#}", id.code()));
        assert_eq!(loaded.dialect, Dialect::Re2, "room {}", id.code());
        let room = &loaded.state;
        assert!(!room.cuts.is_empty(), "room {} has cameras", id.code());
        assert!(room.sca.is_some(), "room {} has collision", id.code());
        assert!(
            !room.zones.is_empty(),
            "room {} has switch zones",
            id.code()
        );
        // Not every zone names one of the room's cameras: 11C0 keeps groups
        // for cameras 13 and 14 it does not have.
        assert!(
            room.zones.iter().any(|zone| zone.cam_from == 0),
            "room {} has camera 0's switch zones",
            id.code()
        );
        assert!(
            loaded.own_scripts(pack).unwrap().init.is_empty(),
            "an RE2 room runs no scripts yet"
        );
        for cut in &room.cuts {
            cameras += 1;
            if pack
                .read_asset(&id.cut_stem(cut.index), arklay::pack::AssetClass::Image)
                .unwrap()
                .is_none()
            {
                missing.push(format!("{}/{:02x}", id.code(), cut.index));
            }
        }
    }
    eprintln!("{cameras} cameras, no background for {missing:?}");
    // Room 7071's first eight cameras point past the end of `roomcut.bin`,
    // and the 17-camera rooms' 17th has no pointer slot.
    let mut expected: Vec<String> = ["30c0/10", "30c1/10", "c0c0/10", "c0c1/10"]
        .map(String::from)
        .into();
    expected.extend((0..8).map(|cut| format!("7071/{cut:02x}")));
    expected.sort();
    missing.sort();
    assert_eq!(missing, expected);
}
