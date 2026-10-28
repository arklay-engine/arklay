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
