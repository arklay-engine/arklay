//! `convert-game` for Resident Evil 2 (the PC release, `bio2 1.10`).
//!
//! The install is split into two disc trees, `Pl0` (Leon) and `Pl1`
//! (Claire), and a shared `Common` tree. Everything lands in one pack with
//! the RE1 pack's folder conventions; `<s>` below is the scenario disc, `0`
//! for `Pl0` and `1` for `Pl1`. Where both discs ship a file, both copies
//! are written: identical bytes share one blob in the pack.
//!
//! | source | entry |
//! |---|---|
//! | `Pl{0,1}/Rdt/ROOM####.RDT` | `room/<id>/room.xml` |
//! | ADT streams in `Common/bin/roomcut.bin` | `room/<id>/cut/<nn>.bmp` |
//! | `Pl0/emd0`, `Pl1/emd1` `EM<s><id>.EMD` | `enemy/<id>/enemy_<s>.emd` |
//! | `Pl0/emd0`, `Pl1/emd1` `EM<s><id>.TIM` | `enemy/<id>/enemy_<s>.bmp`, `enemy_<s>_clut_<nn>.bmp` |
//! | `Pl{0,1}/PLD/PL<id>.PLD` | `player/<id>/model_<s>.pld` |
//! | `Pl{0,1}/PLD/PL<id>CH.PLD` | `player/<id>/model_ch_<s>.pld` |
//! | `Pl{0,1}/PLD/PL<id>W<nn>.PLW` | `player/<id>/weapon/<nn>_<s>.plw` |
//! | `Pl{0,1}/Voice/stageN/vNNN.sap` | `voice/<s>/<NN>_<id>.wav` |
//! | `Common/Sound/BGM/{main,sub}<nn>[_<k>].sap` | `bgm/<track:03x>.wav` |
//! | `Common/DOOR/DOOR<nn>.DO2` | `door/<nn>.do2` |
//! | `Zmovie`, `Pl{0,1}/Zmovie` `*.bin` | `movie/<stem>.mpg` |
//! | `Pl0/PLD/PDEMO0n.DAT`, `Pl1/PLD/PDEMO1n.DAT` | `demo/<nn>.dat` |
//!
//! Every name is matched case-insensitively (the retail tree mixes
//! `em010.emd` with `EM012.EMD`, `Pl0/VOICE` with `Pl1/Voice`) and every
//! entry name is lower case. Room ids are the four characters after `ROOM`:
//! `<stage><room:02x><disc>`, where the stage is `1`-`7` or one of the letter
//! stages `A`-`G`, whose backgrounds are stage 1-7's groups by letter order.
//!
//! A room is converted into its folder's `room.xml` from what [`crate::rdt2`]
//! reads of the RDT; one the XML cannot carry exactly stays a raw
//! `room.rdt`, which the engine still loads. An enemy texture is written as
//! an indexed BMP page plus a palette swatch per further CLUT row
//! ([`emd2::texture_bmps`]). Music tracks are numbered as RE1's are
//! ([`bgm_track`]).
//!
//! The six `Pdemo**p.dat` files are deliberately not packed: they are
//! alternate takes of the six reels (three byte-identical, three differing
//! only in their recorded inputs) that the executable's single demo path
//! template, `pl0\pld\pdemo00.dat` with its digits patched from the demo
//! index, can never name.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};

use crate::convert::{finish_pack, parallel_map, read_dir_sorted};
use crate::format::{self, Dialect};
use crate::manifest;
use crate::pack::PackWriter;
use crate::progress::Progress;
use crate::state::RoomId;
use crate::{adt, bmp, emd2, sap};

/// The pack id of a converted RE2 install.
pub const BASE_PACK_ID: &str = "re2";

/// How deep under the conversion root the data directory may sit.
const MAX_DEPTH: usize = 3;

/// Camera cuts a room's `roomcut.bin` pointer group holds.
const CUTS_PER_ROOM: usize = 16;

/// Room slots per stage in the `roomcut.bin` pointer table.
const ROOMS_PER_STAGE: usize = 32;

/// Phases [`convert`] reports: scan, room, cut, enemy, player, voice, bgm,
/// door, movie, demo, manifest, compress, write.
const PHASES: u64 = 13;

/// The child of `dir` named `name`, matched case-insensitively.
fn child(dir: &Path, name: &str) -> Option<PathBuf> {
    let direct = dir.join(name);
    if direct.exists() {
        return Some(direct);
    }
    fs::read_dir(dir).ok()?.flatten().find_map(|entry| {
        entry
            .file_name()
            .to_str()
            .is_some_and(|found| found.eq_ignore_ascii_case(name))
            .then(|| entry.path())
    })
}

/// The child reached by following `names` from `dir`, case-insensitively.
fn path_of(dir: &Path, names: &[&str]) -> Option<PathBuf> {
    names
        .iter()
        .try_fold(dir.to_path_buf(), |dir, name| child(&dir, name))
}

/// Whether `dir` is an RE2 data directory: it holds `Pl0/Rdt`.
fn is_data_dir(dir: &Path) -> bool {
    path_of(dir, &["Pl0", "Rdt"]).is_some_and(|rdt| rdt.is_dir())
}

/// The RE2 data directory at or under `root` (breadth-first, at most
/// three levels down), if there is one.
pub fn find_data_dir(root: &Path) -> Option<PathBuf> {
    let mut queue = std::collections::VecDeque::from([(root.to_path_buf(), 0usize)]);
    while let Some((dir, depth)) = queue.pop_front() {
        if is_data_dir(&dir) {
            return Some(dir);
        }
        if depth >= MAX_DEPTH {
            continue;
        }
        // An unreadable directory is skipped rather than ending the search.
        let Ok(entries) = read_dir_sorted(&dir) else {
            continue;
        };
        for entry in entries {
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                queue.push_back((entry.path(), depth + 1));
            }
        }
    }
    None
}

/// The files of `dir` (case-insensitive path), sorted by name; empty when
/// the directory is missing.
fn files_in(dir: Option<PathBuf>) -> Result<Vec<PathBuf>> {
    let Some(dir) = dir.filter(|dir| dir.is_dir()) else {
        return Ok(Vec::new());
    };
    Ok(read_dir_sorted(&dir)?
        .into_iter()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .map(|entry| entry.path())
        .collect())
}

/// The upper-case stem and extension of a file name.
fn upper_parts(path: &Path) -> Option<(String, String)> {
    let name = path.file_name()?.to_str()?.to_ascii_uppercase();
    let (stem, extension) = name.rsplit_once('.')?;
    Some((stem.to_owned(), extension.to_owned()))
}

/// Two ASCII hex digits as a byte. Stricter than `u8::from_str_radix`,
/// which also takes a leading `+`.
fn hex_byte(digits: &str) -> Option<u8> {
    (digits.len() == 2 && digits.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| u8::from_str_radix(digits, 16).ok())
        .flatten()
}

fn read(path: &Path) -> Result<Vec<u8>> {
    fs::read(path).with_context(|| format!("failed to read {}", path.display()))
}

/// A room id parsed from `ROOM<id>.RDT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Re2Room {
    /// The stage character, upper case: `1`-`7` or `A`-`G`.
    pub stage: char,
    /// The room within the stage.
    pub room: u8,
    /// The disc digit.
    pub disc: u8,
}

impl Re2Room {
    /// Parse the four characters after `ROOM` (e.g. `100A` is not a room,
    /// `1000` and `G010` are).
    pub fn parse(id: &str) -> Option<Self> {
        let id = id.to_ascii_uppercase();
        let bytes = id.as_bytes();
        if bytes.len() != 4 || !id.is_ascii() {
            return None;
        }
        let stage = bytes[0] as char;
        if !matches!(stage, '1'..='7' | 'A'..='G') {
            return None;
        }
        let room = hex_byte(&id[1..3])?;
        let disc = (bytes[3] as char).to_digit(10)? as u8;
        Some(Self { stage, room, disc })
    }

    /// The zero-based stage group in `roomcut.bin`: stages `1`-`7` are 0-6
    /// and the letter stages `A`-`G` share them by letter order.
    pub fn stage_group(self) -> usize {
        match self.stage {
            '1'..='7' => self.stage as usize - '1' as usize,
            _ => self.stage as usize - 'A' as usize,
        }
    }

    /// The pack folder id, e.g. `1000` or `g010`.
    pub fn folder(self) -> String {
        format!(
            "{}{:02x}{}",
            self.stage.to_ascii_lowercase(),
            self.room,
            self.disc
        )
    }

    /// The byte offset of the room's 16 cut pointers in `roomcut.bin`, as
    /// the executable's `bg_load` computes it. Each stage has 32 room slots;
    /// a room number of `0x20` or more would read the next stage's pointers
    /// (see [`Re2Room::has_pointer_slot`]).
    pub fn pointer_offset(self) -> usize {
        (self.room as usize + ROOMS_PER_STAGE * self.stage_group()) << 6
    }

    /// Whether the room number fits its stage's 32 pointer slots (retail
    /// rooms stop at `0x1D`).
    pub fn has_pointer_slot(self) -> bool {
        usize::from(self.room) < ROOMS_PER_STAGE
    }
}

/// One room to pack.
struct Room {
    id: Re2Room,
    bytes: Vec<u8>,
}

/// The discovered install.
struct Layout {
    data: PathBuf,
    /// `Pl0` and `Pl1`, either may be missing (a single-disc install).
    discs: [Option<PathBuf>; 2],
    common: Option<PathBuf>,
}

impl Layout {
    fn discover(root: &Path) -> Result<Self> {
        let Some(data) = find_data_dir(root) else {
            bail!(
                "no RE2 data directory (one holding Pl0/Rdt) within {MAX_DEPTH} levels of {}",
                root.display()
            );
        };
        Ok(Self {
            discs: [child(&data, "Pl0"), child(&data, "Pl1")],
            common: child(&data, "Common"),
            data,
        })
    }

    fn disc(&self, disc: usize, names: &[&str]) -> Option<PathBuf> {
        path_of(self.discs[disc].as_ref()?, names)
    }

    fn common(&self, names: &[&str]) -> Option<PathBuf> {
        path_of(self.common.as_ref()?, names)
    }
}

/// Category counters for the verbose summary.
#[derive(Default)]
struct Tally {
    entries: BTreeMap<&'static str, (usize, usize)>,
    notes: Vec<String>,
}

impl Tally {
    fn note(&mut self, line: String) {
        self.notes.push(line);
    }

    fn add(
        &mut self,
        writer: &mut PackWriter,
        category: &'static str,
        entry: &str,
        data: Vec<u8>,
    ) -> Result<()> {
        self.add_shared(writer, category, entry, Arc::new(data))
    }

    /// Add an entry whose bytes other entries share (see
    /// [`PackWriter::add_shared`]).
    fn add_shared(
        &mut self,
        writer: &mut PackWriter,
        category: &'static str,
        entry: &str,
        data: Arc<Vec<u8>>,
    ) -> Result<()> {
        let counter = self.entries.entry(category).or_default();
        counter.0 += 1;
        counter.1 += data.len();
        writer
            .add_shared(entry, data)
            .with_context(|| format!("failed to add {entry}"))
    }

    fn summary(&self) -> Vec<u8> {
        let mut out = String::new();
        for (category, (count, bytes)) in &self.entries {
            out.push_str(&format!("{category}: {count} entries, {bytes} bytes\n"));
        }
        for note in &self.notes {
            out.push_str(note);
            out.push('\n');
        }
        out.into_bytes()
    }
}

/// Convert the RE2 install at or under `root` into the pack `out`.
pub(crate) fn convert(
    root: &Path,
    out: &Path,
    jobs: usize,
    verbose: bool,
    progress: &mut Progress,
) -> Result<()> {
    progress.set_phases(PHASES);
    progress.begin("scan", 1);
    let layout = Layout::discover(root)?;
    let mut warnings = Vec::new();
    for (disc, path) in layout.discs.iter().enumerate() {
        if path.is_none() {
            warnings.push(format!(
                "no Pl{disc} directory under {}",
                layout.data.display()
            ));
        }
    }
    if layout.common.is_none() {
        warnings.push(format!(
            "no Common directory under {}",
            layout.data.display()
        ));
    }
    let rooms = scan_rooms(&layout, &mut warnings)?;
    progress.advance("scan");
    progress.end_phase();

    let mut writer = PackWriter::new();
    let mut tally = Tally::default();

    progress.begin("room", rooms.len() as u64);
    for room in &rooms {
        let folder = room.id.folder();
        let id = RoomId::parse(&folder)?;
        // A room the XML cannot carry exactly stays a raw RDT, which the
        // engine still loads.
        let files =
            crate::room::convert_rdt2(&room.bytes, id, Vec::new()).unwrap_or_else(|error| {
                warnings.push(format!("{}: kept as an RDT: {error:#}", id.rdt_entry()));
                vec![(id.rdt_entry(), room.bytes.clone())]
            });
        for (entry, data) in files {
            tally.add(&mut writer, "room", &entry, data)?;
        }
        progress.advance(&folder);
    }
    progress.end_phase();

    copy_backgrounds(
        &layout,
        &rooms,
        &mut writer,
        &mut tally,
        &mut warnings,
        progress,
        jobs,
    )?;
    copy_enemies(&layout, &mut writer, &mut tally, &mut warnings, progress)?;
    copy_players(&layout, &mut writer, &mut tally, &mut warnings, progress)?;
    copy_voice(&layout, &mut writer, &mut tally, &mut warnings, progress)?;
    copy_bgm(&layout, &mut writer, &mut tally, &mut warnings, progress)?;
    copy_doors(&layout, &mut writer, &mut tally, &mut warnings, progress)?;
    copy_movies(&layout, &mut writer, &mut tally, &mut warnings, progress)?;
    copy_demos(&layout, &mut writer, &mut tally, &mut warnings, progress)?;

    progress.begin("manifest", 1);
    let text = base_manifest().render().into_bytes();
    tally.add(&mut writer, "manifest", manifest::ENTRY, text)?;
    progress.advance(manifest::ENTRY);
    progress.end_phase();

    let summary = if verbose { tally.summary() } else { Vec::new() };
    finish_pack(writer, out, jobs, progress, &warnings, &summary, verbose)
}

/// The RE2 pack's self-description.
fn base_manifest() -> manifest::Manifest {
    let mut manifest = manifest::Manifest::base(BASE_PACK_ID);
    manifest.name = Some("Resident Evil 2".to_string());
    manifest.version = Some(env!("CARGO_PKG_VERSION").to_string());
    manifest.convert_version = Some(crate::convert::CONVERT_VERSION);
    // A new game starts on the street, room 100 (Leon's disc is variant 0,
    // Claire's 1).
    manifest.start_room = Some(START_ROOM.to_string());
    manifest
}

/// The room a new RE2 game starts in.
const START_ROOM: &str = "100";

/// Every `ROOM####.RDT` of both discs, in id order. A file that is not an
/// RE2 RDT (the retail `ROOM7051.RDT` is a broken stub) is skipped with a
/// warning.
fn scan_rooms(layout: &Layout, warnings: &mut Vec<String>) -> Result<Vec<Room>> {
    let mut rooms = Vec::new();
    for disc in 0..2 {
        let Some(dir) = layout.disc(disc, &["Rdt"]) else {
            if layout.discs[disc].is_some() {
                warnings.push(format!(
                    "no Pl{disc}/Rdt directory; its rooms will be missing"
                ));
            }
            continue;
        };
        for path in files_in(Some(dir))? {
            let Some((stem, extension)) = upper_parts(&path) else {
                continue;
            };
            let Some(id) = stem.strip_prefix("ROOM").and_then(Re2Room::parse) else {
                continue;
            };
            if extension != "RDT" {
                continue;
            }
            let bytes = read(&path)?;
            match format::rdt(&bytes).dialect() {
                Some(Dialect::Re2) => rooms.push(Room { id, bytes }),
                _ => warnings.push(format!(
                    "{}: not an RE2 RDT; skipped",
                    path.file_name().unwrap_or_default().to_string_lossy()
                )),
            }
        }
    }
    rooms.sort_by_key(|room| room.id);
    // Two names for one room (`ROOM1000.RDT` and `room1000.rdt` in one
    // folder on a case-sensitive file system): keep the first.
    let before = rooms.len();
    rooms.dedup_by_key(|room| room.id);
    if rooms.len() < before {
        warnings.push(format!(
            "{} duplicate room file(s) differing only in name case; the first of each kept",
            before - rooms.len()
        ));
    }
    Ok(rooms)
}

/// Decode each room's camera backgrounds from `roomcut.bin`.
///
/// The cut count is byte 1 of the RDT. Each unique stream is decoded once and
/// written under every room folder that points at it, sharing one buffer. A
/// zero pointer, one past the end of the file, or a stream that fails to
/// decode warns and produces no entry.
fn copy_backgrounds(
    layout: &Layout,
    rooms: &[Room],
    writer: &mut PackWriter,
    tally: &mut Tally,
    warnings: &mut Vec<String>,
    progress: &mut Progress,
    jobs: usize,
) -> Result<()> {
    let Some(path) = layout.common(&["bin", "roomcut.bin"]) else {
        warnings.push("no Common/bin/roomcut.bin; every room background will be missing".into());
        progress.begin("cut", 0);
        progress.end_phase();
        return Ok(());
    };
    let bin = read(&path)?;
    let mut streams: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    let mut zero = 0usize;
    let mut outside = 0usize;
    for room in rooms {
        if !room.id.has_pointer_slot() {
            warnings.push(format!(
                "room {}: room number past roomcut.bin's {ROOMS_PER_STAGE} slots per stage; \
                 no background packed",
                room.id.folder()
            ));
            continue;
        }
        let cuts = usize::from(room.bytes[1]);
        if cuts > CUTS_PER_ROOM {
            // The executable would read the next room's first pointer.
            let dropped = if cuts - 1 == CUTS_PER_ROOM {
                format!("cut {CUTS_PER_ROOM:02x} has")
            } else {
                format!("cuts {CUTS_PER_ROOM:02x}-{:02x} have", cuts - 1)
            };
            warnings.push(format!(
                "room {}: {cuts} camera cuts but roomcut.bin holds {CUTS_PER_ROOM} per room; \
                 {dropped} no background",
                room.id.folder()
            ));
        }
        let cuts = cuts.min(CUTS_PER_ROOM);
        let base = room.id.pointer_offset();
        let folder = room.id.folder();
        for cut in 0..cuts {
            let entry = format!("room/{folder}/cut/{cut:02x}.bmp");
            let pointer = bin
                .get(base + cut * 4..base + cut * 4 + 4)
                .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()) as usize);
            match pointer {
                Some(0) => zero += 1,
                Some(pointer) if pointer < bin.len() => {
                    streams.entry(pointer).or_default().push(entry)
                }
                _ => outside += 1,
            }
        }
    }
    if zero > 0 {
        warnings.push(format!(
            "{zero} camera cut(s) have a zero roomcut.bin pointer; no background packed"
        ));
    }
    if outside > 0 {
        warnings.push(format!(
            "{outside} camera cut(s) point past the end of roomcut.bin; no background packed"
        ));
    }

    let streams: Vec<(usize, Vec<String>)> = streams.into_iter().collect();
    tally.note(format!(
        "cut: {} unique roomcut.bin streams decoded",
        streams.len()
    ));
    let total = streams
        .iter()
        .map(|(_, entries)| entries.len() as u64)
        .sum();
    progress.begin("cut", total);
    let decoded = parallel_map(streams.len(), jobs, progress, |index| {
        let (pointer, entries) = &streams[index];
        // A corrupt stream costs only its own cameras.
        let bmp = adt::background(&bin[*pointer..])
            .and_then(|image| bmp::encode_to_vec(&image))
            .map_err(|error| format!("{error:#}"));
        Ok((entries[0].clone(), entries.len() as u64, bmp))
    })?;
    progress.end_phase();
    for ((pointer, entries), (_, bmp)) in streams.iter().zip(decoded) {
        match bmp {
            Ok(bmp) => {
                let bmp = Arc::new(bmp);
                for entry in entries {
                    tally.add_shared(writer, "cut", entry, Arc::clone(&bmp))?;
                }
            }
            Err(error) => warnings.push(format!(
                "roomcut.bin stream at {pointer:#x} does not decode ({error}); \
                 no background for {}",
                entries.join(", ")
            )),
        }
    }
    Ok(())
}

/// The pack entry for an enemy file name, e.g. `EM13A.TIM` is
/// `enemy/3a/enemy_1.tim`, and the disc its name says it belongs to.
fn enemy_entry(stem: &str, extension: &str) -> Option<(String, usize)> {
    let name = stem
        .strip_prefix("EM")
        .filter(|name| name.len() == 3 && name.is_ascii())?;
    let disc = match &name[..1] {
        "0" => 0,
        "1" => 1,
        _ => return None,
    };
    let id = hex_byte(&name[1..])?;
    let extension = extension.to_ascii_lowercase();
    matches!(extension.as_str(), "emd" | "tim")
        .then(|| (format!("enemy/{id:02x}/enemy_{disc}.{extension}"), disc))
}

/// `EM<s><id>.EMD`/`.TIM` of both discs' enemy folders. The name's disc
/// digit decides the entry. A file filed under the other disc's folder (the
/// retail `Pl0/emd0/EM13A.TIM`) yields to the one in its own folder, with a
/// warning.
fn copy_enemies(
    layout: &Layout,
    writer: &mut PackWriter,
    tally: &mut Tally,
    warnings: &mut Vec<String>,
    progress: &mut Progress,
) -> Result<()> {
    let mut files: BTreeMap<String, (PathBuf, bool)> = BTreeMap::new();
    for folder in 0..2 {
        let dir = match layout.disc(folder, &[&format!("emd{folder}")]) {
            Some(dir) => dir,
            None => {
                if layout.discs[folder].is_some() {
                    warnings.push(format!(
                        "no Pl{folder}/emd{folder} directory; its enemies will be missing"
                    ));
                }
                continue;
            }
        };
        for path in files_in(Some(dir))? {
            let Some((entry, disc)) =
                upper_parts(&path).and_then(|(stem, extension)| enemy_entry(&stem, &extension))
            else {
                continue;
            };
            let home = disc == folder;
            match files.get(&entry) {
                Some((kept, true)) if !home => {
                    warnings.push(format!(
                        "{}: filed under the other disc; {} kept",
                        path.display(),
                        kept.display()
                    ));
                }
                Some((stray, false)) if home => {
                    warnings.push(format!(
                        "{}: filed under the other disc; {} kept",
                        stray.display(),
                        path.display()
                    ));
                    files.insert(entry, (path, home));
                }
                Some(_) => bail!("{} duplicates {entry}", path.display()),
                None => {
                    files.insert(entry, (path, home));
                }
            }
        }
    }
    progress.begin("enemy", files.len() as u64);
    for (entry, (path, _)) in files {
        let data = read(&path)?;
        let Some(stem) = entry.strip_suffix(".tim") else {
            tally.add(writer, "enemy", &entry, data)?;
            progress.advance(&entry);
            continue;
        };
        match emd2::texture_bmps(&data) {
            Ok(bmps) => {
                for (row, bmp) in bmps.into_iter().enumerate() {
                    let entry = match row {
                        0 => format!("{stem}.bmp"),
                        row => emd2::clut_entry(stem, row),
                    };
                    tally.add(writer, "enemy", &entry, bmp)?;
                }
            }
            Err(error) => {
                warnings.push(format!("{}: kept as a TIM: {error:#}", path.display()));
                tally.add(writer, "enemy", &entry, data)?;
            }
        }
        progress.advance(&entry);
    }
    progress.end_phase();
    Ok(())
}

/// The pack entry for a player file name, e.g. `PL0BW0A.PLW` on disc 1 is
/// `player/0b/weapon/0a_1.plw`.
fn player_entry(stem: &str, extension: &str, disc: usize) -> Option<String> {
    let rest = stem.strip_prefix("PL").filter(|rest| rest.is_ascii())?;
    let id = hex_byte(rest.get(..2)?)?;
    let tail = &rest[2..];
    match (extension, tail) {
        ("PLD", "") => Some(format!("player/{id:02x}/model_{disc}.pld")),
        ("PLD", "CH") => Some(format!("player/{id:02x}/model_ch_{disc}.pld")),
        ("PLW", weapon) if weapon.len() == 3 && weapon.starts_with('W') => {
            let weapon = hex_byte(&weapon[1..])?;
            Some(format!("player/{id:02x}/weapon/{weapon:02x}_{disc}.plw"))
        }
        _ => None,
    }
}

/// Every `PL*.PLD`, `PL*CH.PLD` and `PL*W*.PLW` of both discs.
fn copy_players(
    layout: &Layout,
    writer: &mut PackWriter,
    tally: &mut Tally,
    warnings: &mut Vec<String>,
    progress: &mut Progress,
) -> Result<()> {
    let mut files = Vec::new();
    for disc in 0..2 {
        match layout.disc(disc, &["PLD"]) {
            Some(dir) => files.extend(files_in(Some(dir))?.into_iter().map(|path| (disc, path))),
            None if layout.discs[disc].is_some() => warnings.push(format!(
                "no Pl{disc}/PLD directory; its players will be missing"
            )),
            None => {}
        }
    }
    let files: Vec<(String, PathBuf)> = files
        .into_iter()
        .filter_map(|(disc, path)| {
            let (stem, extension) = upper_parts(&path)?;
            Some((player_entry(&stem, &extension, disc)?, path))
        })
        .collect();
    progress.begin("player", files.len() as u64);
    for (entry, path) in files {
        tally.add(writer, "player", &entry, read(&path)?)?;
        progress.advance(&entry);
    }
    progress.end_phase();
    Ok(())
}

/// The WAV body of a one-WAV `.sap` file.
fn sap_wav(path: &Path) -> Result<Vec<u8>> {
    let data = read(path)?;
    Ok(sap::single_wav(&data)
        .with_context(|| format!("{}", path.display()))?
        .to_vec())
}

/// `Pl{0,1}/Voice/stageN/vNNN.sap` as `voice/<s>/<NN>_<id>.wav`.
fn copy_voice(
    layout: &Layout,
    writer: &mut PackWriter,
    tally: &mut Tally,
    warnings: &mut Vec<String>,
    progress: &mut Progress,
) -> Result<()> {
    let mut files = Vec::new();
    for disc in 0..2 {
        let Some(voice) = layout.disc(disc, &["Voice"]) else {
            if layout.discs[disc].is_some() {
                warnings.push(format!(
                    "no Pl{disc}/Voice directory; its voice lines will be missing"
                ));
            }
            continue;
        };
        for stage_dir in read_dir_sorted(&voice)? {
            let name = stage_dir.file_name().to_string_lossy().to_ascii_lowercase();
            let Some(stage) = name
                .strip_prefix("stage")
                .and_then(|n| n.parse::<u8>().ok())
            else {
                continue;
            };
            for path in files_in(Some(stage_dir.path()))? {
                let Some((stem, extension)) = upper_parts(&path) else {
                    continue;
                };
                let Some(id) = stem.strip_prefix('V').and_then(|id| id.parse::<u16>().ok()) else {
                    continue;
                };
                if extension == "SAP" {
                    files.push((format!("voice/{disc}/{stage:02}_{id:02x}.wav"), path));
                }
            }
        }
    }
    progress.begin("voice", files.len() as u64);
    for (entry, path) in files {
        tally.add(writer, "voice", &entry, sap_wav(&path)?)?;
        progress.advance(&entry);
    }
    progress.end_phase();
    Ok(())
}

/// The track id of a BGM file stem, which names its `bgm/<id:03x>` entry:
/// `main<nn>` is `0nn`, `sub<nn>` is `1nn`, and an alternate take
/// `<name>_<k>` adds `0x200 * k` (`main00_1` is `200`, `sub2f_1` is `32f`).
///
/// The two-digit numbers are the executable's own hex track numbers (the
/// `common\sound\bgm\%s.sap` names it formats), so `main0f` is `00f`.
pub fn bgm_track(stem: &str) -> Option<u16> {
    let stem = stem.to_ascii_lowercase();
    let (name, take) = match stem.split_once('_') {
        Some((name, take)) => (
            name,
            take.parse::<u16>()
                .ok()
                .filter(|take| (1..=7).contains(take))?,
        ),
        None => (stem.as_str(), 0),
    };
    let (bank, number) = if let Some(number) = name.strip_prefix("main") {
        (0, number)
    } else {
        (1, name.strip_prefix("sub")?)
    };
    Some(take * 0x200 + bank * 0x100 + u16::from(hex_byte(number)?))
}

/// `Common/Sound/BGM/*.sap` as `bgm/<track:03x>.wav` ([`bgm_track`]); a
/// file the numbering does not cover is skipped with a warning.
fn copy_bgm(
    layout: &Layout,
    writer: &mut PackWriter,
    tally: &mut Tally,
    warnings: &mut Vec<String>,
    progress: &mut Progress,
) -> Result<()> {
    let dir = layout.common(&["Sound", "BGM"]);
    if dir.is_none() && layout.common.is_some() {
        warnings.push("no Common/Sound/BGM directory; the music will be missing".into());
    }
    let files: Vec<PathBuf> = files_in(dir)?
        .into_iter()
        .filter(|path| upper_parts(path).is_some_and(|(_, extension)| extension == "SAP"))
        .collect();
    progress.begin("bgm", files.len() as u64);
    for path in files {
        let (stem, _) = upper_parts(&path).unwrap();
        let Some(track) = bgm_track(&stem) else {
            warnings.push(format!(
                "{}: not a numbered BGM track; skipped",
                path.display()
            ));
            progress.advance(&stem);
            continue;
        };
        let entry = format!("bgm/{track:03x}.wav");
        tally.add(writer, "bgm", &entry, sap_wav(&path)?)?;
        progress.advance(&entry);
    }
    progress.end_phase();
    Ok(())
}

/// `Common/DOOR/DOOR<nn>.DO2` as `door/<nn>.do2`.
fn copy_doors(
    layout: &Layout,
    writer: &mut PackWriter,
    tally: &mut Tally,
    warnings: &mut Vec<String>,
    progress: &mut Progress,
) -> Result<()> {
    let dir = layout.common(&["DOOR"]);
    if dir.is_none() && layout.common.is_some() {
        warnings.push("no Common/DOOR directory; the doors will be missing".into());
    }
    let mut doors = Vec::new();
    for path in files_in(dir)? {
        let Some((stem, extension)) = upper_parts(&path) else {
            continue;
        };
        let id = stem
            .strip_prefix("DOOR")
            .and_then(|id| u8::from_str_radix(id, 16).ok());
        if let (Some(id), "DO2") = (id, extension.as_str()) {
            doors.push((format!("door/{id:02x}.do2"), path));
        }
    }
    progress.begin("door", doors.len() as u64);
    for (entry, path) in doors {
        tally.add(writer, "door", &entry, read(&path)?)?;
        progress.advance(&entry);
    }
    progress.end_phase();
    Ok(())
}

/// Every `*.bin` film of `Zmovie`, `Pl0/Zmovie` and `Pl1/Zmovie`, as
/// `movie/<stem>.mpg`.
fn copy_movies(
    layout: &Layout,
    writer: &mut PackWriter,
    tally: &mut Tally,
    warnings: &mut Vec<String>,
    progress: &mut Progress,
) -> Result<()> {
    // The shared films sit beside `data` in some installs and inside it in
    // others.
    let shared = child(&layout.data, "Zmovie").or_else(|| {
        layout
            .data
            .parent()
            .and_then(|parent| child(parent, "Zmovie"))
    });
    let dirs = [
        shared,
        layout.disc(0, &["Zmovie"]),
        layout.disc(1, &["Zmovie"]),
    ];
    if dirs.iter().all(Option::is_none) {
        warnings.push("no Zmovie directory; the films will be missing".into());
    }
    let mut films = BTreeMap::new();
    for dir in dirs {
        for path in files_in(dir)? {
            if let Some((stem, extension)) = upper_parts(&path)
                && extension == "BIN"
            {
                match films.entry(format!("movie/{}.mpg", stem.to_ascii_lowercase())) {
                    std::collections::btree_map::Entry::Vacant(slot) => {
                        slot.insert(path);
                    }
                    std::collections::btree_map::Entry::Occupied(kept) => {
                        warnings.push(format!(
                            "{}: same film name as {}; the first kept",
                            path.display(),
                            kept.get().display()
                        ));
                    }
                }
            }
        }
    }
    progress.begin("movie", films.len() as u64);
    for (entry, path) in films {
        let data = read(&path)?;
        if !format::is_mpeg_program(&data) {
            warnings.push(format!(
                "{}: not an MPEG program stream; skipped",
                path.display()
            ));
            progress.advance(&entry);
            continue;
        }
        tally.add(writer, "movie", &entry, data)?;
        progress.advance(&entry);
    }
    progress.end_phase();
    Ok(())
}

/// The six demo reels in interleaved character order:
/// `PDEMO00`, `PDEMO10`, `PDEMO01`, `PDEMO11`, `PDEMO02`, `PDEMO12`.
fn copy_demos(
    layout: &Layout,
    writer: &mut PackWriter,
    tally: &mut Tally,
    warnings: &mut Vec<String>,
    progress: &mut Progress,
) -> Result<()> {
    progress.begin("demo", 6);
    for index in 0..6usize {
        let disc = index % 2;
        let name = format!("PDEMO{disc}{}.DAT", index / 2);
        let entry = format!("demo/{index:02x}.dat");
        match layout.disc(disc, &["PLD", &name]) {
            Some(path) => tally.add(writer, "demo", &entry, read(&path)?)?,
            None if layout.discs[disc].is_some() => {
                warnings.push(format!("no Pl{disc}/PLD/{name}; {entry} will be missing"))
            }
            None => {}
        }
        progress.advance(&entry);
    }
    progress.end_phase();
    Ok(())
}

#[cfg(test)]
mod tests;
