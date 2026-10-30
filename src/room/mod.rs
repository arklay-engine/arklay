//! Room definitions: the `room/<id>/` folder, or a raw RDT.
//!
//! Everything a room is made of sits in one folder named by its three-digit
//! room id (`room/10b/`), under conventional names, so a room can be copied
//! to another pack or renamed by renaming its folder. The folder holds every
//! variant of the room: both players and, for the mansion's stages 1 and 2,
//! both visits. `room.xml` marks what only some variants have with `when`
//! ([`xml`]); where the variants' models, effects or player animation
//! differ, each differing file has a number of its own in the kind's folder
//! and `room.xml`'s `<slots>` names the files each variant's slots read
//! ([`source::Slots`]). An RE2 pack keeps one folder per RDT (`room/1001/`),
//! each player's room its own.
//!
//! | entry | content |
//! |---|---|
//! | `room.xml` | the room definition ([`xml`], modelled by [`RoomSource`]) |
//! | `room.scd` | the room's init, per-frame and event scripts |
//! | `cut/<camera>.bmp` | each camera's background |
//! | `mask/<camera>.bmp` | each camera's mask page |
//! | `anim/<file>.emr` + `.edd` | the room's player-animation pair |
//! | `object/<file>.tmd` + `.bmp` | an object model's mesh and texture |
//! | `item/<file>.tmd` + `.bmp` | an item model's mesh and texture |
//! | `effect/<file>.xml` + `.bmp` | an effect sprite's definition and sheet |
//!
//! Camera and file numbers are two lower-case hex digits (`cut/0a.bmp`);
//! slot `n` reads file `n` unless `<slots>` says otherwise.
//!
//! RE1's definitions are checked in ([`RE1_FILES`]); `convert-game` takes only
//! the models, effect sheets and player animation from an install's RDTs
//! ([`source::rdt_assets`]).
//!
//! The room's display names and message texts live in `room.xml`'s
//! `<strings>` block and reach the game as `ROOM_<room3>_NAME`,
//! `ROOM_<room3>_NAME_SAVE` and `ROOM_<room3>_TEXT_XX` language keys
//! ([`language_layers`]); a language file's own key still wins.
//!
//! A raw RDT in the folder still loads in place of `room.xml`, so a mod that
//! ships an RDT keeps working: when a pack holds both, the one from the
//! higher layer wins, and the XML wins a tie. An RDT is one variant's, so it
//! is named for its variant ([`RoomId::rdt_entry`]: `jill-mansion2.rdt`); a
//! `room.rdt` serves every variant without one.

pub mod source;
pub mod xml;

use anyhow::{Context, Result};

use crate::format::Dialect;
use crate::pack::Pack;
use crate::scd;
use crate::state::{RoomId, RoomState};

pub use source::{Converted, RoomSource};

impl RoomId {
    /// Pack path of the room definition, e.g. `room/100/room.xml`.
    pub fn xml_entry(self) -> String {
        format!("{}/{}", self.dir(), source::XML_FILE)
    }
}

/// The folder one variant of a room reads its files from in a pack: the
/// room's own, or for a pack that keeps one folder per RDT (RE2) that RDT's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    dir: String,
    /// The raw RDT the variant would read: its own, else the folder's
    /// `room.rdt`, whether the pack has it or not.
    rdt: String,
}

impl Folder {
    /// The folder of room variant `id` in `pack`: the room folder
    /// ([`RoomId::dir`]), or, for a pack that keeps one folder per RDT (RE2),
    /// that RDT's folder.
    pub fn of(pack: &Pack, id: RoomId) -> Self {
        let dir = id.dir();
        let variant_rdt = id.rdt_entry();
        let room_rdt = format!("{dir}/{RDT_FILE}");
        let defines = |dir: &str| {
            [source::XML_FILE, RDT_FILE]
                .iter()
                .any(|file| pack.contains(&format!("{dir}/{file}")))
        };
        let rdt_dir = id.rdt_dir();
        if !defines(&dir) && !pack.contains(&variant_rdt) && defines(&rdt_dir) {
            return Self {
                rdt: format!("{rdt_dir}/{RDT_FILE}"),
                dir: rdt_dir,
            };
        }
        let rdt = if pack.contains(&variant_rdt) {
            variant_rdt
        } else {
            room_rdt
        };
        Self { dir, rdt }
    }

    /// The room's folder, `room/100`.
    pub fn dir(&self) -> &str {
        &self.dir
    }

    /// The path of `name` in the folder.
    pub fn file(&self, name: &str) -> String {
        format!("{}/{name}", self.dir)
    }

    /// Pack stem of a camera's background, `room/100/cut/03`.
    pub fn cut_stem(&self, camera: usize) -> String {
        format!("{}/cut/{camera:02x}", self.dir)
    }

    /// Pack stem of a camera's mask page, `room/100/mask/03`.
    pub fn mask_stem(&self, camera: usize) -> String {
        format!("{}/mask/{camera:02x}", self.dir)
    }

    /// The raw RDT of the variant: its own (`room/100/jill.rdt`), else the
    /// folder's `room.rdt`.
    pub fn rdt(&self) -> &str {
        &self.rdt
    }
}

/// The file name of a raw RDT in a room folder.
pub const RDT_FILE: &str = "room.rdt";
/// Which file a room's definition comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// `room/<id>/room.xml`.
    Xml,
    /// `room/<id>/room.rdt`.
    Rdt,
}

/// The pack entry of room variant `id`'s `room.xml`.
pub fn xml_entry(pack: &Pack, id: RoomId) -> String {
    format!("{}/{}", Folder::of(pack, id).dir(), source::XML_FILE)
}

/// The pack entry of the raw RDT room variant `id` takes ([`Folder::rdt`]).
pub fn rdt_entry(pack: &Pack, id: RoomId) -> String {
    Folder::of(pack, id).rdt().to_string()
}

/// The file `pack` defines room `id` with, or `None` when it has neither.
pub fn format(pack: &Pack, id: RoomId) -> Option<Format> {
    match (
        pack.depth_of(&xml_entry(pack, id)),
        pack.depth_of(&rdt_entry(pack, id)),
    ) {
        (Some(xml), Some(rdt)) if rdt > xml => Some(Format::Rdt),
        (Some(_), _) => Some(Format::Xml),
        (None, Some(_)) => Some(Format::Rdt),
        (None, None) => None,
    }
}

/// The game room `id`'s definition comes from: an RDT's detected dialect,
/// or a `room.xml`'s `game`. A missing or unreadable room is RE1's.
pub fn dialect(pack: &Pack, id: RoomId) -> Dialect {
    match format(pack, id) {
        Some(Format::Rdt) => pack
            .read(&rdt_entry(pack, id))
            .ok()
            .and_then(|data| crate::format::rdt(&data).dialect())
            .unwrap_or_default(),
        Some(Format::Xml) => pack
            .read(&xml_entry(pack, id))
            .ok()
            .and_then(|data| xml::parse_for(std::str::from_utf8(&data).ok()?, id).ok())
            .map(|source| source.dialect)
            .unwrap_or_default(),
        None => Dialect::Re1,
    }
}

/// Whether `pack` defines room `id` at all.
pub fn exists(pack: &Pack, id: RoomId) -> bool {
    format(pack, id).is_some()
}

/// The pack entry room `id` is defined by, for messages and layer checks.
pub fn entry(pack: &Pack, id: RoomId) -> String {
    match format(pack, id) {
        Some(Format::Rdt) => rdt_entry(pack, id),
        _ => xml_entry(pack, id),
    }
}

/// The layer depth of room `id`'s definition (see [`Pack::depth_of`]).
pub fn depth(pack: &Pack, id: RoomId) -> Option<usize> {
    pack.depth_of(&entry(pack, id))
}

/// Where a room's SCD scripts are loaded from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptSource {
    /// The scripts embedded in the room's `room.rdt`.
    Rdt,
    /// The folder's `room.scd` (none at all when it is missing).
    Room,
}

/// Where room `id` takes its scripts from: of the RDT's own scripts (when
/// the RDT defines the room) and `room.scd`, the one from the highest layer,
/// `room.scd` winning a tie.
///
/// So a mod that ships its own `room.rdt` runs that RDT's scripts over the
/// base pack's `room.scd`, one that ships a `room.scd` runs it, and one that
/// only edits `room.xml` keeps the base scripts.
pub fn script_source(pack: &Pack, id: RoomId) -> ScriptSource {
    let folder = Folder::of(pack, id);
    let rdt = (format(pack, id) == Some(Format::Rdt))
        .then(|| pack.depth_of(folder.rdt()))
        .flatten();
    [
        (rdt, ScriptSource::Rdt),
        (pack.depth_of(&folder.file(SCD_FILE)), ScriptSource::Room),
    ]
    .into_iter()
    .filter_map(|(depth, source)| Some((depth?, source)))
    // `max_by_key` keeps the last of equal keys: the later source.
    .max_by_key(|(depth, _)| *depth)
    .map_or(ScriptSource::Room, |(_, source)| source)
}

/// The file name of a room folder's scripts.
pub const SCD_FILE: &str = "room.scd";

/// The scripts embedded in RDT `data`: none for an RE2 RDT, whose script
/// dialect the engine does not run yet.
fn rdt_scripts(data: &[u8]) -> Result<scd::ir::Scripts> {
    if crate::format::rdt(data).dialect() == Some(Dialect::Re2) {
        return Ok(scd::ir::Scripts::default());
    }
    scd::reader::parse(data).context("invalid room SCD scripts")
}

/// Read and parse SCD container `entry`; a missing `room.scd` is no scripts.
fn read_scripts(pack: &Pack, entry: &str) -> Result<scd::ir::Scripts> {
    if !pack.contains(entry) {
        return Ok(scd::ir::Scripts::default());
    }
    let data = pack.read(entry)?;
    scd::reader::parse(&data).with_context(|| format!("invalid room scripts {entry}"))
}

/// A loaded room definition: the room state, plus the RDT bytes its scripts
/// may come from.
#[derive(Debug)]
pub struct Loaded {
    pub id: RoomId,
    pub state: RoomState,
    /// The game the room's files are in. An RE2 room has no scripts the
    /// engine can run yet, so it runs none.
    pub dialect: Dialect,
    rdt: Option<Vec<u8>>,
}

impl Loaded {
    /// Room `id` parsed from RDT bytes `data`, whose scripts are embedded.
    ///
    /// The RDT's dialect is detected from the file ([`crate::format::rdt`]):
    /// an RE2 RDT goes through [`crate::rdt2`]'s room definition, anything
    /// else (including the undetectable stub RDTs) through the RE1 parser.
    pub fn from_rdt(pack: &Pack, id: RoomId, data: Vec<u8>) -> Result<Self> {
        if crate::format::rdt(&data).dialect() == Some(Dialect::Re2) {
            let state = crate::rdt2::read(&data)?.build(id, pack)?;
            return Ok(Self {
                id,
                state,
                dialect: Dialect::Re2,
                rdt: Some(data),
            });
        }
        let state = crate::rdt::parse(&data, id)?;
        let script = crate::text::pack_script(pack);
        Ok(Self {
            id,
            state: drawn_messages(state, script),
            dialect: Dialect::Re1,
            rdt: Some(data),
        })
    }

    /// Build room `id` from XML definition `text`, reading the rest of its
    /// folder from `pack`.
    pub fn from_xml(pack: &Pack, id: RoomId, text: &str) -> Result<Self> {
        let source = xml::parse_for(text, id)?;
        let state = source.build(id, pack)?;
        Ok(Self {
            id,
            state,
            dialect: source.dialect,
            rdt: None,
        })
    }

    /// The room's scripts and where they came from ([`script_source`]).
    pub fn scripts(&self, pack: &Pack) -> Result<(scd::ir::Scripts, ScriptSource)> {
        let source = script_source(pack, self.id);
        if self.dialect == Dialect::Re2 {
            return Ok((scd::ir::Scripts::default(), source));
        }
        let folder = Folder::of(pack, self.id);
        let scripts = match source {
            ScriptSource::Rdt => match &self.rdt {
                Some(data) => rdt_scripts(data)?,
                None => rdt_scripts(&pack.read(folder.rdt())?)?,
            },
            ScriptSource::Room => read_scripts(pack, &folder.file(SCD_FILE))?,
        };
        Ok((scripts, source))
    }

    /// The scripts the room runs, wherever they come from.
    pub fn own_scripts(&self, pack: &Pack) -> Result<scd::ir::Scripts> {
        self.scripts(pack).map(|(scripts, _)| scripts)
    }
}

/// Load room `id` from whichever definition `pack` resolves it to.
pub fn load(pack: &Pack, id: RoomId) -> Result<Loaded> {
    match format(pack, id) {
        Some(Format::Rdt) => {
            let data = pack.read(&rdt_entry(pack, id))?.into_owned();
            Loaded::from_rdt(pack, id, data)
        }
        Some(Format::Xml) => {
            let entry = xml_entry(pack, id);
            let data = pack.read(&entry)?;
            let text =
                std::str::from_utf8(&data).with_context(|| format!("{entry} is not UTF-8"))?;
            Loaded::from_xml(pack, id, text).with_context(|| format!("invalid {entry}"))
        }
        None => anyhow::bail!("{}", missing(id)),
    }
}

/// `state` with its RDT message block as the game draws it in `script`: a
/// Japanese stage-5 room's text moves onto the stage-five kanji rows
/// ([`crate::charset::stage_five_text`]), the same form the room's XML holds.
fn drawn_messages(mut state: RoomState, script: crate::charset::Script) -> RoomState {
    if let Some(block) = state.messages.as_mut() {
        *block = source::drawn_message_block(block, script, state.stage);
    }
    state
}

/// The error of a room the pack does not define.
fn missing(id: RoomId) -> String {
    format!(
        "room {}{} is missing: no {} or {}",
        id.room3(),
        variant_label(id),
        id.xml_entry(),
        id.rdt_entry()
    )
}

/// How a message names room variant `id`: ` (jill, second visit)`.
pub fn variant_label(id: RoomId) -> String {
    if id.mansion2 {
        format!(" ({}, second visit)", id.player_tag())
    } else {
        format!(" ({})", id.player_tag())
    }
}

/// Load just room `id`'s state.
pub fn load_state(pack: &Pack, id: RoomId) -> Result<RoomState> {
    load(pack, id).map(|loaded| loaded.state)
}

/// Room `id`'s own scripts ([`Loaded::own_scripts`]), without building the
/// rest of the room.
pub fn own_scripts(pack: &Pack, id: RoomId) -> Result<scd::ir::Scripts> {
    if !exists(pack, id) {
        anyhow::bail!("{}", missing(id));
    }
    Loaded {
        id,
        state: RoomState::default(),
        dialect: dialect(pack, id),
        rdt: None,
    }
    .own_scripts(pack)
}

/// Every room variant `pack` defines, sorted: each player, and each visit
/// of a mansion room, of every room folder (`room/100/`), and every RDT of a
/// pack that keeps one folder per RDT (`room/1001/`, RE2).
pub fn ids(pack: &Pack) -> Vec<RoomId> {
    let mut ids: Vec<RoomId> = Vec::new();
    for path in pack.paths() {
        let Some(rest) = path.strip_prefix("room/") else {
            continue;
        };
        let Some((stem, file)) = rest.split_once('/') else {
            continue;
        };
        if file.contains('/') {
            continue;
        }
        // A raw RDT may be one variant's (`jill-mansion2.rdt`).
        let rdt = if stem.len() == 3 {
            file.to_ascii_lowercase().ends_with(".rdt")
        } else {
            file.eq_ignore_ascii_case(RDT_FILE)
        };
        if !(file.eq_ignore_ascii_case(source::XML_FILE) || rdt) {
            continue;
        }
        match stem.len() {
            3 => {
                let Ok(room) = RoomId::parse(stem) else {
                    continue;
                };
                for player_flag in 0..2 {
                    let id = RoomId {
                        player_flag,
                        ..room
                    };
                    ids.push(id);
                    if id.is_mansion() {
                        ids.push(id.with_visit(true));
                    }
                }
            }
            4 => ids.extend(RoomId::parse(stem).ok()),
            _ => {}
        }
    }
    ids.sort_by_key(|id| (id.mansion2, id.rdt_number()));
    ids.dedup();
    ids
}

/// Every room `pack` defines, as its lowest variant (Chris on the first
/// visit, where the pack has it), sorted.
pub fn rooms(pack: &Pack) -> Vec<RoomId> {
    let mut rooms: Vec<RoomId> = Vec::new();
    for id in ids(pack) {
        if !rooms
            .iter()
            .any(|room| (room.stage, room.room) == (id.stage, id.room))
        {
            rooms.push(id);
        }
    }
    rooms.sort_by_key(|id| (id.stage, id.room));
    rooms
}

/// Every room's `<strings>` as language layers, `(tag, file)` per language,
/// keyed `ROOM_<room3>_NAME`, `ROOM_<room3>_NAME_SAVE` and
/// `ROOM_<room3>_TEXT_XX`, and a string only some variants have (its
/// `when`) as `ROOM_<room3>_<VARIANT>_TEXT_XX` (`ROOM_100_JILL_MANSION2_TEXT_0A`,
/// [`xml::string_variant`]), which [`crate::text::Text::message`] reads
/// first for those variants.
///
/// The strings come from the room's `room.xml` even when a mod's raw RDT
/// defines the room over it, so a mod that ships RDTs keeps the names; an
/// unreadable `room.xml` warns and is skipped here (loading the room reports
/// it properly).
pub fn language_layers(pack: &Pack) -> Vec<(String, crate::language::Language)> {
    use std::collections::BTreeMap;
    let mut by_tag: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut entries: Vec<(RoomId, String)> = rooms(pack)
        .into_iter()
        .map(|id| (id, xml_entry(pack, id)))
        .collect();
    // A per-RDT folder (RE2) names its room as the first player's does.
    entries.dedup_by(|(_, a), (_, b)| a == b);
    for (id, entry) in entries {
        if !pack.contains(&entry) {
            continue;
        }
        let strings = match pack.read(&entry).and_then(|data| room_strings(&data)) {
            Ok(strings) => strings,
            Err(error) => {
                eprintln!("warning: {entry}: {error:#}");
                continue;
            }
        };
        let room3 = id.room3();
        for (tag, key, value, condition) in strings {
            let Some(suffix) = xml::string_suffix(&key) else {
                continue;
            };
            let variant = match condition.as_deref().map(xml::string_variant) {
                None => String::new(),
                Some(Ok(variant)) => format!("{variant}_"),
                Some(Err(error)) => {
                    eprintln!("warning: {entry}: {error:#}");
                    continue;
                }
            };
            let values = by_tag.entry(tag.to_ascii_lowercase()).or_default();
            // A per-RDT folder's later player keeps the first's text.
            values
                .entry(format!("ROOM_{room3}_{variant}{suffix}"))
                .or_insert(value);
        }
    }
    let mut layers = Vec::new();
    for (tag, values) in &by_tag {
        let values = values
            .iter()
            .map(|(key, value)| (key.clone(), value.as_str()));
        match crate::language::Language::from_values(values, &format!("room strings for {tag}")) {
            Ok(language) => layers.push((tag.clone(), language)),
            Err(error) => eprintln!("warning: {error:#}"),
        }
    }
    layers
}

mod checked_in {
    include!(concat!(env!("OUT_DIR"), "/room_files.rs"));
}

/// The checked-in RE1 room definitions (`games/re1/room/`), `(path, text)`
/// by path under that directory: each `<id>/` folder's `room.xml` and
/// `effect/<slot>.xml`. Every release takes the same definitions; the French
/// and German RDTs' leftover data in rooms the others ship as stubs (and their
/// unreachable rooms 313 and 412) is not kept.
///
/// `convert-game` packs these in place of anything read from the install's
/// RDTs, which only supply the room's models and animation.
pub use checked_in::ROOM_FILES as RE1_FILES;

/// The checked-in definition of RE1 room `id` (any variant of it), as
/// `(pack entry, text)`: its folder's `room.xml` and effect definitions.
/// Empty when the repository does not define the room.
pub fn re1_definition(id: RoomId) -> Vec<(String, &'static str)> {
    let prefix = format!("{}/", id.room3().to_ascii_lowercase());
    RE1_FILES
        .iter()
        .filter_map(|(path, text)| {
            let file = path.strip_prefix(&prefix)?;
            Some((format!("{}/{file}", id.dir()), *text))
        })
        .collect()
}

/// Test helper: each checked-in room's `room.xml` at its pack entry, for
/// tests that need the room names and message texts a converted pack
/// carries.
#[cfg(test)]
pub(crate) fn re1_string_entries() -> Vec<(String, Vec<u8>)> {
    RE1_FILES
        .iter()
        .filter_map(|(path, text)| {
            let room = path.strip_suffix("/room.xml")?;
            (room.len() == 3).then(|| (format!("room/{room}/room.xml"), text.as_bytes().to_vec()))
        })
        .collect()
}

/// Just the `<strings>` of a `room.xml`, without building the rest of it.
///
/// Startup reads every room's strings, so only the `<strings>` element is
/// parsed: its text cannot hold a literal `</strings>` (a `<` is escaped),
/// and a room without the tag has none. Anything unusual (the tag inside a
/// comment, say) falls back to parsing the whole file.
fn room_strings(data: &[u8]) -> Result<Vec<xml::ConditionalString>> {
    const CLOSE: &str = "</strings>";
    let text = std::str::from_utf8(data).context("not UTF-8")?;
    let Some(start) = text.find("<strings") else {
        return Ok(Vec::new());
    };
    if let Some(end) = text[start..].find(CLOSE)
        && let Ok(fragment) = roxmltree::Document::parse(&text[start..start + end + CLOSE.len()])
    {
        return xml::conditional_strings(fragment.root_element());
    }
    let document = roxmltree::Document::parse(text).context("not well-formed XML")?;
    match document
        .root_element()
        .children()
        .find(|node| node.has_tag_name("strings"))
    {
        Some(node) => xml::conditional_strings(node),
        None => Ok(Vec::new()),
    }
}

/// Convert an RE2 RDT into its room folder's `room.xml`, as `(pack entry,
/// bytes)`, with `strings` as its display strings.
///
/// The written definition must read back as the one [`crate::rdt2`] reads
/// from the RDT, and build the same room state, so a conversion never ships
/// a room that loads differently from its RDT. The RDT sections the engine
/// does not read yet (scripts, masks, models) are not carried over; a later
/// conversion that reads them writes them into the folder.
pub fn convert_rdt2(
    data: &[u8],
    id: RoomId,
    strings: Vec<crate::language::XmlString>,
) -> Result<Vec<(String, Vec<u8>)>> {
    let mut source = crate::rdt2::read(data)?;
    source.strings = strings;
    let text = xml::write(&source)?;
    let back = xml::parse(&text).context("the written room XML does not parse")?;
    if back != source {
        bail_difference("does not read back as the same definition", &source, &back)?;
    }
    let pack = Pack::from_bytes(crate::pack::PackWriter::new().to_bytes()?)?;
    let built = Loaded::from_xml(&pack, id, &text)
        .context("the written room does not build")?
        .state;
    let expected = Loaded::from_rdt(&pack, id, data.to_vec())?.state;
    if format!("{built:?}") != format!("{expected:?}") {
        bail_difference("builds a different room from its RDT", &expected, &built)?;
    }
    // An RE2 room is one RDT's: its folder is the RDT's.
    Ok(vec![(
        format!("{}/{}", id.rdt_dir(), source::XML_FILE),
        text.into_bytes(),
    )])
}

/// Convert one RDT into its room folder: `room.xml` and the other files it
/// is made of, as `(pack entry, bytes)`, besides the camera images.
///
/// The written folder must give the same room state and scripts as the RDT
/// parser, so a conversion never ships a room that loads differently from
/// its RDT.
pub fn convert_rdt(
    data: &[u8],
    id: RoomId,
    strings: Vec<crate::language::XmlString>,
) -> Result<Vec<(String, Vec<u8>)>> {
    let Converted {
        mut source,
        mut files,
    } = RoomSource::from_rdt(data, id)?;
    source.strings = strings;
    let text = xml::write(&source)?;
    let back = xml::parse(&text).context("the written room XML does not parse")?;
    if back != source {
        bail_difference("does not read back as the same definition", &source, &back)?;
    }

    let entry = format!("{}/{}", id.rdt_dir(), source::XML_FILE);
    let mut writer = crate::pack::PackWriter::new();
    for (path, bytes) in &files {
        writer.add(path, bytes.clone())?;
    }
    writer.add(&entry, text.clone().into_bytes())?;
    let pack = Pack::from_bytes(writer.to_bytes()?)?;
    let loaded = Loaded::from_xml(&pack, id, &text).context("the written room does not build")?;
    // The RDT's own message text is left behind: the room's text is its
    // strings.
    let mut expected = crate::rdt::parse(data, id)?;
    expected.messages = None;
    let mut built = loaded.state.clone();
    // The model stems only name HD replacements, which an RDT has none of.
    for asset in built.object_models.iter_mut().chain(&mut built.item_models) {
        asset.hd_name = None;
    }
    if format!("{built:?}") != format!("{expected:?}") {
        bail_difference("builds a different room from its RDT", &expected, &built)?;
    }
    let scripts = loaded.own_scripts(&pack)?;
    let original = scd::reader::parse(data).unwrap_or_default();
    if scripts.digest() != original.digest() {
        anyhow::bail!("the written room's scripts differ from its RDT's");
    }

    files.push((entry, text.into_bytes()));
    Ok(files)
}

/// Fail naming the first line where `expected` and `actual` differ, under the
/// nearest field name above it.
fn bail_difference(
    what: &str,
    expected: &impl std::fmt::Debug,
    actual: &impl std::fmt::Debug,
) -> Result<()> {
    let (expected, actual) = (format!("{expected:#?}"), format!("{actual:#?}"));
    let lines: Vec<(&str, &str)> = expected.lines().zip(actual.lines()).collect();
    let difference = lines
        .iter()
        .position(|(a, b)| a != b)
        .map(|at| {
            let field = lines[..at]
                .iter()
                .rev()
                .find_map(|(line, _)| line.trim().split_once(": ").map(|(name, _)| name))
                .unwrap_or("room");
            let (a, b) = lines[at];
            format!(" ({field}: expected `{}`, got `{}`)", a.trim(), b.trim())
        })
        .unwrap_or_default();
    anyhow::bail!("the written room XML {what}{difference}")
}

#[cfg(test)]
mod tests;
