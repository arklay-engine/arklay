//! Room definitions: the `room/<id>/` folder, or a raw RDT.
//!
//! Everything a room is made of sits in one folder named by its four-digit
//! RDT number, under conventional names, so a room can be copied to another
//! pack or renamed by renaming its folder:
//!
//! | entry | content |
//! |---|---|
//! | `room.xml` | the room definition ([`xml`], modelled by [`RoomSource`]) |
//! | `room.scd` | the room's init, per-frame and event scripts |
//! | `original.scd` | the original scripts, beside a `room.scd` holding the port's fixes |
//! | `cut/<camera>.bmp` | each camera's background |
//! | `mask/<camera>.bmp` | each camera's mask page |
//! | `anim/00.emr` + `.edd` | the room's player-animation pair |
//! | `object/<slot>.tmd` + `.bmp` | an object-model slot's mesh and texture |
//! | `item/<slot>.tmd` + `.bmp` | an item-model slot's mesh and texture |
//! | `effect/<slot>.xml` + `.bmp` | an effect sprite slot's definition and sheet |
//!
//! Camera and slot numbers are two lower-case hex digits (`cut/0a.bmp`).
//!
//! A raw `room.rdt` in the folder still loads in place of `room.xml`, so a
//! mod that ships an RDT keeps working: when a pack holds both, the one from
//! the higher layer wins, and the XML wins a tie.

pub mod markup;
pub mod source;
pub mod xml;

use anyhow::{Context, Result};

use crate::format::Dialect;
use crate::pack::Pack;
use crate::scd;
use crate::state::{RoomId, RoomState};

pub use source::{Converted, RoomSource};

impl RoomId {
    /// Pack path of the room definition, e.g. `room/1001/room.xml`.
    pub fn xml_entry(self) -> String {
        format!("{}/{}", self.dir(), source::XML_FILE)
    }
}

/// Which file a room's definition comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// `room/<id>/room.xml`.
    Xml,
    /// `room/<id>/room.rdt`.
    Rdt,
}

/// The file `pack` defines room `id` with, or `None` when it has neither.
pub fn format(pack: &Pack, id: RoomId) -> Option<Format> {
    match (
        pack.depth_of(&id.xml_entry()),
        pack.depth_of(&id.rdt_entry()),
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
            .read(&id.rdt_entry())
            .ok()
            .and_then(|data| crate::format::rdt(&data).dialect())
            .unwrap_or_default(),
        Some(Format::Xml) => pack
            .read(&id.xml_entry())
            .ok()
            .and_then(|data| xml::parse(std::str::from_utf8(&data).ok()?).ok())
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
        Some(Format::Rdt) => id.rdt_entry(),
        _ => id.xml_entry(),
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
    /// The folder's `original.scd`: the original bytecode `convert-game`
    /// keeps beside a `room.scd` that holds the port's fixes.
    Original,
}

/// Where room `id` takes its scripts from: of the RDT's own scripts (when
/// the RDT defines the room), `room.scd` and, with `original_scripts` (the
/// `room_script_fixes = false` option), `original.scd`, the one from the
/// highest layer, the later of that list winning a tie.
///
/// So a mod that ships its own `room.rdt` runs that RDT's scripts over the
/// base pack's `room.scd`, one that ships a `room.scd` runs it whatever the
/// option says, and one that only edits `room.xml` keeps the base scripts.
pub fn script_source(pack: &Pack, id: RoomId, original_scripts: bool) -> ScriptSource {
    let rdt = (format(pack, id) == Some(Format::Rdt))
        .then(|| pack.depth_of(&id.rdt_entry()))
        .flatten();
    let original = original_scripts
        .then(|| pack.depth_of(&id.original_scd_entry()))
        .flatten();
    [
        (rdt, ScriptSource::Rdt),
        (pack.depth_of(&id.scd_entry()), ScriptSource::Room),
        (original, ScriptSource::Original),
    ]
    .into_iter()
    .filter_map(|(depth, source)| Some((depth?, source)))
    // `max_by_key` keeps the last of equal keys: the later source.
    .max_by_key(|(depth, _)| *depth)
    .map_or(ScriptSource::Room, |(_, source)| source)
}

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
        let source = xml::parse(text)?;
        let state = source.build(id, pack)?;
        Ok(Self {
            id,
            state,
            dialect: source.dialect,
            rdt: None,
        })
    }

    /// The room's scripts and where they came from ([`script_source`]).
    pub fn scripts(
        &self,
        pack: &Pack,
        original_scripts: bool,
    ) -> Result<(scd::ir::Scripts, ScriptSource)> {
        let source = script_source(pack, self.id, original_scripts);
        if self.dialect == Dialect::Re2 {
            return Ok((scd::ir::Scripts::default(), source));
        }
        let scripts = match source {
            ScriptSource::Rdt => match &self.rdt {
                Some(data) => rdt_scripts(data)?,
                None => rdt_scripts(&pack.read(&self.id.rdt_entry())?)?,
            },
            ScriptSource::Room => read_scripts(pack, &self.id.scd_entry())?,
            ScriptSource::Original => read_scripts(pack, &self.id.original_scd_entry())?,
        };
        Ok((scripts, source))
    }

    /// The room's own scripts, without the port's fixes: `original.scd`
    /// when the pack keeps one beside a fixed `room.scd` (unless a later
    /// layer ships its own `room.scd`), otherwise what the room runs.
    pub fn own_scripts(&self, pack: &Pack) -> Result<scd::ir::Scripts> {
        self.scripts(pack, true).map(|(scripts, _)| scripts)
    }
}

/// Load room `id` from whichever definition `pack` resolves it to.
pub fn load(pack: &Pack, id: RoomId) -> Result<Loaded> {
    match format(pack, id) {
        Some(Format::Rdt) => {
            let data = pack.read(&id.rdt_entry())?.into_owned();
            Loaded::from_rdt(pack, id, data)
        }
        Some(Format::Xml) => {
            let entry = id.xml_entry();
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
        "room {} is missing: no {} or {}",
        id.code().to_ascii_uppercase(),
        id.xml_entry(),
        id.rdt_entry()
    )
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

/// Every room id `pack` defines, sorted, with either format.
pub fn ids(pack: &Pack) -> Vec<RoomId> {
    let mut ids: Vec<RoomId> = pack
        .paths()
        .filter_map(|path| {
            let rest = path.strip_prefix("room/")?;
            let (stem, file) = rest.split_once('/')?;
            if stem.len() != 4
                || !(file.eq_ignore_ascii_case(source::XML_FILE)
                    || file.eq_ignore_ascii_case("room.rdt"))
            {
                return None;
            }
            RoomId::parse(stem).ok()
        })
        .collect();
    ids.sort_by_key(|id| id.rdt_number());
    ids.dedup();
    ids
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
    script: crate::charset::Script,
) -> Result<Vec<(String, Vec<u8>)>> {
    let Converted { source, mut files } = RoomSource::from_rdt(data, id, script)?;
    let text = xml::write(&source)?;
    let back = xml::parse(&text).context("the written room XML does not parse")?;
    if back != source {
        bail_difference("does not read back as the same definition", &source, &back)?;
    }

    let mut writer = crate::pack::PackWriter::new();
    for (path, bytes) in &files {
        writer.add(path, bytes.clone())?;
    }
    let pack = Pack::from_bytes(writer.to_bytes()?)?;
    let loaded = Loaded::from_xml(&pack, id, &text).context("the written room does not build")?;
    let expected = drawn_messages(crate::rdt::parse(data, id)?, script);
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

    files.push((id.xml_entry(), text.into_bytes()));
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
