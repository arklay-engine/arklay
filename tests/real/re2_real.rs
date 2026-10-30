//! RE2 conversion against the retail PC install: `ARKLAY_RE2_ROOT` names the
//! directory holding `data/` (or `data/` itself). Without it every test here
//! skips. The RE1 half of the detection census also reads `ARKLAY_RE1_ROOT`
//! when it is set.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use arklay::adpcm;
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
            ["room", _, "sound.edh"] => "sound",
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
        ("enemy", 384),
        ("manifest", 1),
        ("movie", 26),
        ("player", 221),
        ("room", 501),
        ("sound", 2768),
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

    // Every room is a room definition, every texture a BMP and every track
    // a numbered `bgm/<id:03x>`.
    assert!(
        !pack
            .paths()
            .any(|path| path.ends_with(".rdt") || path.ends_with(".tim"))
    );
    assert_eq!(
        pack.paths()
            .filter(|path| path.ends_with("/room.xml"))
            .count(),
        501
    );
    for path in pack.paths().filter(|path| path.starts_with("bgm/")) {
        let id = path
            .strip_prefix("bgm/")
            .and_then(|name| name.strip_suffix(".wav"))
            .unwrap();
        assert!(
            id.len() == 3 && u16::from_str_radix(id, 16).is_ok(),
            "{path}"
        );
    }
    for track in [
        "bgm/000.wav",
        "bgm/144.wav",
        "bgm/200.wav",
        "bgm/32f.wav",
        "bgm/400.wav",
    ] {
        assert!(pack.contains(track), "{track}");
    }
}

/// Every retail enemy texture is stored exactly: the page BMP and its CLUT
/// swatches read back as the TIM.
#[test]
#[ignore]
fn every_enemy_texture_round_trips_through_its_bmps() {
    let (Some(pack), Some(root)) = (pack(), re2_root()) else {
        return;
    };
    let mut checked = 0;
    for path in with_extension(&root, "tim") {
        let name = name(&path);
        let Some(rest) = name
            .strip_prefix("EM")
            .and_then(|rest| rest.strip_suffix(".TIM"))
        else {
            continue;
        };
        let disc = &rest[..1];
        // The stray disc-1 copy under `Pl0/emd0` is not packed.
        let folder = path
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy();
        if !folder.eq_ignore_ascii_case(&format!("emd{disc}")) {
            continue;
        }
        let stem = format!("enemy/{}/enemy_{disc}", rest[1..].to_ascii_lowercase());
        let mut swatches = Vec::new();
        for row in 1.. {
            let entry = arklay::emd2::clut_entry(&stem, row);
            if !pack.contains(&entry) {
                break;
            }
            swatches.push(pack.read(&entry).unwrap().to_vec());
        }
        let page = pack.read(&format!("{stem}.bmp")).unwrap();
        let texture = arklay::emd2::texture_from_bmps(&page, &swatches).unwrap();
        let expected = arklay::tim::decode_8bpp(&fs::read(&path).unwrap()).unwrap();
        assert!(texture == expected, "{stem}");
        checked += 1;
    }
    assert_eq!(checked, 118);
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
    assert_eq!(count(verify::Format::RoomXml), 501);
    assert_eq!(count(verify::Format::Re2Rdt), 0);
    assert_eq!(count(verify::Format::Re2Emd), 116);
    assert_eq!(count(verify::Format::Pld), 40);
    assert_eq!(count(verify::Format::Plw), 181);
    assert_eq!(count(verify::Format::AdpcmWav), 2893);
    assert_eq!(count(verify::Format::Wav), 4);
    assert_eq!(count(verify::Format::Edh), 588);
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

/// Every converted RE2 room reads into a room with cameras, collision and
/// each camera's lights, and every camera but the retail data's known gaps has a background.
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
            assert!(
                cut.light.is_some(),
                "room {} camera {} has its LIT lights",
                id.code(),
                cut.index
            );
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

/// The TeamX RE2 HD pack's backgrounds are named by `bio2hd.asi`'s hash of
/// each camera's main page; `ARKLAY_RE2_HD` names the extracted pack (the
/// directory holding `hires/`). The pack has no file for the rooms and
/// camera states it skipped, so a few cameras stay unmatched.
#[test]
#[ignore]
fn the_hd_pack_names_most_backgrounds() {
    let (Some(pack), Some(hd)) = (pack(), std::env::var_os("ARKLAY_RE2_HD").map(PathBuf::from))
    else {
        return;
    };
    let names: HashSet<u32> = with_extension(&hd.join("hires/bgd"), "webp")
        .iter()
        .filter_map(|path| u32::from_str_radix(path.file_stem()?.to_str()?.get(..8)?, 16).ok())
        .collect();
    let (mut matched, mut unmatched) = (0, Vec::new());
    for path in pack.paths().filter(|path| path.contains("/cut/")) {
        let image = arklay::bmp::decode(&pack.read(path).unwrap()).unwrap();
        let candidates = arklay::htexture::image_candidates(&image);
        if candidates
            .iter()
            .any(|candidate| names.contains(&candidate.hash))
        {
            matched += 1;
        } else {
            unmatched.push(path.to_string());
        }
    }
    eprintln!(
        "{matched} of {} backgrounds have an HD file; unmatched: {unmatched:#?}",
        matched + unmatched.len()
    );
    assert!(
        matched * 10 >= (matched + unmatched.len()) * 9,
        "{matched} matched"
    );
}

/// Every RE2 model of the pack: the enemy EMDs, the player PLDs (body and
/// `ch` variants) and the weapon PLWs.
fn models(pack: &Pack) -> Vec<String> {
    let mut models: Vec<String> = pack
        .paths()
        .filter(|path| {
            (path.starts_with("enemy/") && path.ends_with(".emd"))
                || (path.starts_with("player/")
                    && (path.ends_with(".pld") || path.ends_with(".plw")))
        })
        .map(str::to_owned)
        .collect();
    models.sort();
    models
}

/// Each model parses through the viewer, every clip frame names one of its
/// bank's keyframes and every primitive its object's vertices, and it draws
/// textured and animates when stood in room 100: the frame differs from the
/// empty room in many colours, and a clip with more than one keyframe draws
/// differently eight ticks on. `ARKLAY_RE2_MODEL_SHOTS` names a directory
/// to write each model's frame to as a BMP (the contact sheet's tiles), with
/// the empty room as `room.bmp`.
#[test]
#[ignore]
fn every_model_parses_draws_textured_and_animates() {
    use arklay::engine::{ModelView, model_view_frames};
    use arklay::model_viewer;

    let Some(pack) = pack() else {
        return;
    };
    let shots = std::env::var_os("ARKLAY_RE2_MODEL_SHOTS").map(PathBuf::from);
    if let Some(dir) = &shots {
        fs::create_dir_all(dir).unwrap();
    }
    let room = arklay::state::RoomId::from_room_and_player("100", 0).unwrap();
    let empty = model_view_frames(pack, room, None, &[0], None)
        .unwrap()
        .remove(0);
    if let Some(dir) = &shots {
        let data = arklay::bmp::encode_to_vec(&empty).unwrap();
        fs::write(dir.join("room.bmp"), data).unwrap();
    }
    let models = models(pack);
    assert_eq!(models.len(), 116 + 40 + 181, "{models:?}");
    let mut failures = Vec::new();
    for entry in &models {
        let model = match model_viewer::load(pack, entry) {
            Ok(model) => model,
            Err(error) => {
                failures.push(format!("{entry}: {error:#}"));
                continue;
            }
        };
        let banks = std::iter::once((&model.keyframes, &model.clips)).chain(
            model
                .banks
                .iter()
                .map(|bank| (&bank.keyframes, &bank.clips)),
        );
        for (index, (keyframes, clips)) in banks.enumerate() {
            let bad = clips
                .iter()
                .flat_map(|clip| &clip.frames)
                .find(|frame| usize::from(frame.keyframe) >= keyframes.len());
            if let Some(frame) = bad {
                failures.push(format!(
                    "{entry}: bank {index} names keyframe {} of {}",
                    frame.keyframe,
                    keyframes.len()
                ));
            }
        }
        for (index, object) in model.mesh.objects.iter().enumerate() {
            let bad = object.prims.iter().any(|prim| {
                prim.vertices
                    .iter()
                    .any(|&vertex| usize::from(vertex) >= object.vertices.len())
            });
            if bad {
                failures.push(format!("{entry}: object {index} names a missing vertex"));
            }
        }
        let view = ModelView {
            entry,
            bank: 0,
            clip: 0,
        };
        let frames = model_view_frames(pack, room, Some(view), &[0, 8], None).unwrap();
        let changed: HashSet<[u8; 4]> = frames[0]
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .zip(empty.rgba.as_chunks::<4>().0)
            .filter(|(drawn, room)| drawn != room)
            .map(|(drawn, _)| *drawn)
            .collect();
        if changed.len() < 8 {
            failures.push(format!(
                "{entry}: draws {} distinct colours over the room",
                changed.len()
            ));
        }
        let moving = model.clips.first().is_some_and(|clip| {
            clip.frames.iter().any(|frame| {
                model.keyframes.get(usize::from(frame.keyframe)) != model.keyframes.first()
            })
        });
        if moving && frames[0].rgba == frames[1].rgba {
            failures.push(format!("{entry}: clip 0 does not animate"));
        }
        if let Some(dir) = &shots {
            let name = entry.replace('/', "_");
            let data = arklay::bmp::encode_to_vec(&frames[0]).unwrap();
            fs::write(dir.join(format!("{name}.bmp")), data).unwrap();
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The TeamX RE2 HD pack's model skins are named by `bio2hd.asi`'s hash of
/// each 128-texel page through its own CLUT row; `ARKLAY_RE2_HD` names the
/// extracted pack. Every enemy and player page has a skin but a few the
/// pack skipped (the weapon composites are hashed as the character's own
/// texture, so only the bodies are counted).
#[test]
#[ignore]
fn the_hd_pack_names_most_model_pages() {
    let (Some(pack), Some(hd)) = (pack(), std::env::var_os("ARKLAY_RE2_HD").map(PathBuf::from))
    else {
        return;
    };
    let mut names = HashSet::new();
    for class in ["skin", "skin2"] {
        for entry in fs::read_dir(hd.join("hires").join(class)).unwrap() {
            let name = entry.unwrap().file_name().to_string_lossy().into_owned();
            if let Some(hash) = name
                .get(..8)
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
            {
                names.insert(hash);
            }
        }
    }
    let (mut matched, mut unmatched) = (0, Vec::new());
    for entry in models(pack).iter().filter(|entry| !entry.ends_with(".plw")) {
        let model = arklay::model_viewer::load(pack, entry).unwrap();
        let texture = &model.texture;
        for page in 0..(texture.width as usize / 128).min(texture.palettes.len() / 256) {
            let candidates = arklay::htexture::page_candidates(texture, page);
            if candidates.iter().any(|candidate| {
                candidate.origin == [page as u32 * 128, 0] && names.contains(&candidate.hash)
            }) {
                matched += 1;
            } else {
                unmatched.push(format!("{entry} page {page}"));
            }
        }
    }
    eprintln!(
        "{matched} of {} model pages have an HD skin; unmatched: {unmatched:#?}",
        matched + unmatched.len()
    );
    assert!(
        matched * 10 >= (matched + unmatched.len()) * 9,
        "{matched} matched"
    );
}

/// Every room's init and main scripts parse, rebuild byte for byte, and
/// round-trip through `.s` and `.akscr` to the same bytes.
#[test]
#[ignore]
fn every_room_script_round_trips() {
    use arklay::scd2::{akscr, asm, reader};

    let Some(root) = re2_root() else {
        return;
    };
    let mut rooms = 0;
    let mut procedures = 0;
    let mut fallbacks = 0;
    let mut failures = Vec::new();
    for path in with_extension(&root, "rdt") {
        let data = fs::read(&path).unwrap();
        if format::rdt(&data).dialect() != Some(Dialect::Re2) {
            continue;
        }
        let (scripts, spans) = match reader::parse_with_spans(&data) {
            Ok(parsed) => parsed,
            Err(error) => {
                failures.push(format!("{}: {error:#}", path.display()));
                continue;
            }
        };
        rooms += 1;
        let lists = [&scripts.init, &scripts.main];
        for (list, span) in lists.iter().zip(spans) {
            procedures += list.procs.len();
            if let Some(span) = span
                && list.to_bytes() != data[span.start..span.start + span.len]
            {
                failures.push(format!("{}: a list does not rebuild", path.display()));
            }
        }
        match asm::assemble(&asm::render(&scripts)) {
            Ok(back) if back == scripts => {}
            Ok(_) => failures.push(format!("{}: .s round trip differs", path.display())),
            Err(error) => failures.push(format!("{}: .s: {error:#}", path.display())),
        }
        let text = akscr::decompile(&scripts);
        fallbacks += text.matches('@').count();
        match akscr::compile(&text) {
            Ok(back)
                if back.init.to_bytes() == scripts.init.to_bytes()
                    && back.main.to_bytes() == scripts.main.to_bytes() => {}
            Ok(_) => failures.push(format!("{}: .akscr round trip differs", path.display())),
            Err(error) => failures.push(format!("{}: .akscr: {error:#}", path.display())),
        }
    }
    eprintln!("{rooms} rooms, {procedures} procedures, {fallbacks} raw block instructions");
    assert!(failures.is_empty(), "{failures:#?}");
    assert!(rooms > 0);
    // Every retail block decompiles to its structured form.
    assert_eq!(fallbacks, 0);
}

/// The packed WAVs: every voice line, BGM track and sound-bank slot.
fn wav_entries(pack: &Pack) -> Vec<String> {
    let mut entries: Vec<String> = pack
        .paths()
        .filter(|path| {
            path.ends_with(".wav")
                && ["voice/", "bgm/", "sound/"]
                    .iter()
                    .any(|prefix| path.starts_with(prefix))
        })
        .map(str::to_owned)
        .collect();
    entries.sort();
    entries
}

/// ffmpeg's decode of `wav` as interleaved 16-bit samples.
fn ffmpeg_decode(dir: &Path, name: &str, wav: &[u8]) -> Vec<i16> {
    let input = dir.join(format!("{name}.wav"));
    fs::write(&input, wav).unwrap();
    let output = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(&input)
        .args(["-f", "s16le", "-acodec", "pcm_s16le", "pipe:1"])
        .output()
        .unwrap();
    fs::remove_file(&input).unwrap();
    assert!(
        output.status.success(),
        "{name}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
        .stdout
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&pair| i16::from_le_bytes(pair))
        .collect()
}

/// A WAV's `fmt ` and `data` chunk bodies.
fn wav_chunks(wav: &[u8]) -> (&[u8], &[u8]) {
    let (mut fmt, mut data) = (None, None);
    let mut at = 12;
    while at + 8 <= wav.len() {
        let size = u32::from_le_bytes(wav[at + 4..at + 8].try_into().unwrap()) as usize;
        let body = &wav[at + 8..(at + 8 + size).min(wav.len())];
        match &wav[at..at + 4] {
            b"fmt " => fmt = Some(body),
            b"data" => data = Some(body),
            _ => {}
        }
        at += 8 + size + (size & 1);
    }
    (fmt.unwrap(), data.unwrap())
}

/// Every converted WAV decodes, and the MS-ADPCM decoder agrees with
/// ffmpeg's `adpcm_ms` on every distinct one. With ffmpeg's rounding (a
/// division) the samples are identical; with the port's (a shift, what the
/// engine plays) they differ only where a negative prediction rounds the
/// other way and the filter carries that through the block.
#[test]
#[ignore]
fn every_wav_decodes_and_matches_ffmpeg() {
    let Some(pack) = pack() else {
        return;
    };
    let entries = wav_entries(pack);
    let mut distinct = BTreeMap::new();
    let mut adpcm = 0;
    for entry in &entries {
        let data = pack.read(entry).unwrap().to_vec();
        let wav =
            arklay::audio::parse_wav(&data).unwrap_or_else(|error| panic!("{entry}: {error:#}"));
        assert!(!wav.data.is_empty(), "{entry}");
        // The few PCM slots pass through undecoded.
        if format::wav_codec(&data) != Some(format::WAV_MS_ADPCM) {
            continue;
        }
        adpcm += 1;
        let mut hasher = DefaultHasher::new();
        data.hash(&mut hasher);
        distinct
            .entry(hasher.finish())
            .or_insert((entry.clone(), data, wav));
    }
    eprintln!(
        "{} WAVs ({adpcm} MS-ADPCM), {} distinct",
        entries.len(),
        distinct.len()
    );
    assert!(entries.len() > 2000);

    if std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_err()
    {
        eprintln!("ffmpeg is not installed: skipping the cross-check");
        return;
    }
    let dir = std::env::temp_dir().join(format!("arklay-re2-ffmpeg-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let work: Vec<_> = distinct.values().collect();
    let jobs = std::thread::available_parallelism().map_or(1, usize::from);
    let results: Vec<(String, usize, u32, usize)> = std::thread::scope(|scope| {
        let handles: Vec<_> = work
            .chunks(work.len().div_ceil(jobs))
            .enumerate()
            .map(|(job, chunk)| {
                let dir = &dir;
                scope.spawn(move || {
                    chunk
                        .iter()
                        .enumerate()
                        .map(|(index, (entry, data, wav))| {
                            let ours: Vec<i16> = wav
                                .data
                                .as_chunks::<2>()
                                .0
                                .iter()
                                .map(|&pair| i16::from_le_bytes(pair))
                                .collect();
                            let theirs = ffmpeg_decode(dir, &format!("{job}_{index}"), data);
                            assert_eq!(ours.len(), theirs.len(), "{entry}: sample count");
                            let (fmt, body) = wav_chunks(data);
                            let format = adpcm::Format::parse(fmt).unwrap();
                            let divided =
                                adpcm::decode_with(&format, body, adpcm::Rounding::Divide).unwrap();
                            assert!(divided == theirs, "{entry}: differs from ffmpeg");
                            let mut max = 0u32;
                            let mut differing = 0;
                            for (a, b) in ours.iter().zip(&theirs) {
                                let diff = (i32::from(*a) - i32::from(*b)).unsigned_abs();
                                max = max.max(diff);
                                differing += usize::from(diff != 0);
                            }
                            (entry.clone(), ours.len(), max, differing)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    });
    let _ = fs::remove_dir_all(&dir);
    let samples: usize = results.iter().map(|r| r.1).sum();
    let differing: usize = results.iter().map(|r| r.3).sum();
    let worst = results.iter().max_by_key(|r| r.2).unwrap();
    eprintln!(
        "{} files, {samples} samples: {differing} differ, worst {} by {}",
        results.len(),
        worst.0,
        worst.2
    );
    assert!(worst.2 <= MAX_FFMPEG_DIFFERENCE, "{worst:?}");
}

/// The largest sample difference the shift's rounding makes against
/// ffmpeg's (79 in the retail data).
const MAX_FFMPEG_DIFFERENCE: u32 = 128;

/// Every room's sound banks load through `se2` the way `SsLoadBanks` fills
/// them. Some table entries name slots no bank fills; the game plays nothing
/// for those, so their count is pinned rather than required to be zero.
#[test]
#[ignore]
fn every_room_sound_bank_resolves() {
    use arklay::se2::{Banks, Edh, Kind};
    let Some(pack) = pack() else {
        return;
    };
    let mut rooms = 0;
    let mut sounds = 0;
    let mut tables = 0;
    let mut named_but_empty = Vec::new();
    for path in pack.paths().filter(|path| path.ends_with("/room.xml")) {
        let folder = path.split('/').nth(1).unwrap();
        let (stage, room) = (folder.as_bytes()[0], &folder[1..3]);
        // The letter stages are stages 1-7 by letter order.
        let stage = match stage {
            b'1'..=b'7' => stage - b'0',
            b'a'..=b'g' => stage - b'a' + 1,
            _ => panic!("{folder}"),
        };
        let room = u8::from_str_radix(room, 16).unwrap();
        let table = pack
            .read(&format!("room/{folder}/{}", arklay::se2::ROOM_EDH_NAME))
            .ok()
            .map(|data| Edh::parse(&data).unwrap());
        tables += usize::from(table.is_some());
        let mut banks = Banks::new();
        banks.load_room(pack, stage - 1, room, table.clone());
        rooms += 1;
        for slot in 0..48 {
            if banks.stem(Kind::Room, slot).is_some() {
                sounds += 1;
            } else if table
                .as_ref()
                .and_then(|table| table.volume(slot))
                .is_some()
            {
                named_but_empty.push(format!("{folder}:{slot:02x}"));
            }
        }
    }
    eprintln!(
        "{rooms} rooms, {tables} sound tables, {sounds} room sounds, {} table slots without a sound",
        named_but_empty.len()
    );
    assert_eq!(rooms, 501);
    assert_eq!(tables, 476);
    assert_eq!(sounds, ROOM_SOUNDS);
    assert_eq!(named_but_empty.len(), ROOM_TABLE_SLOTS_WITHOUT_SOUND);
}

/// The room sounds the retail rooms load, over all 501 rooms.
const ROOM_SOUNDS: usize = 5256;
/// The room table entries that name a slot no bank fills.
const ROOM_TABLE_SLOTS_WITHOUT_SOUND: usize = 304;

/// Every BGM track the executable's music tables name is a packed track,
/// apart from the names the retail install ships no `.sap` for.
#[test]
#[ignore]
fn every_music_table_track_is_packed() {
    use arklay::bgm2;
    let Some(pack) = pack() else {
        return;
    };
    let mut missing = std::collections::BTreeSet::new();
    let mut found = 0;
    let names = (0..bgm2::MAIN_TRACKS.len() as u8)
        .flat_map(|id| bgm2::main_tracks(id).into_iter().flatten())
        .chain(
            (0..bgm2::SUB_TRACKS.len() as u8)
                .flat_map(|id| bgm2::sub_tracks(id).into_iter().flatten()),
        );
    for name in names {
        let stem = bgm2::track_stem(name).unwrap();
        if pack.contains(&format!("{stem}.wav")) {
            found += 1;
        } else {
            missing.insert(name);
        }
    }
    eprintln!("{found} track references packed; no file for {missing:?}");
    assert_eq!(missing, ["main26"].into_iter().collect());
}
