//! Room definitions: `room/<id>.xml` and its sidecars, or a raw RDT.
//!
//! `convert-game` writes every room as an XML definition ([`xml`], modelled
//! by [`RoomSource`]) that names shared sidecar files:
//!
//! | entry | content |
//! |---|---|
//! | `room/<id>.xml` | the room definition |
//! | `room/<id>.scd` | the room's own init, per-frame and event scripts |
//! | `prop/<pid>.tmd` + `.bmp` | one model pair an object or item slot names |
//! | `prop/props.toml` | which rooms and slots use each prop |
//! | `effect/<eid>.tim` | one room effect sprite sheet |
//! | `animation/<aid>.emr` + `.edd` | one room player-animation pair |
//!
//! The camera backgrounds and mask pages stay `roomcut/` and `roommask/`
//! images, which the XML names by stem.
//!
//! A raw `room/<id>.rdt` still loads, so a pack converted before the XML
//! format and a mod that ships an RDT keep working: when a pack holds both,
//! the one from the higher layer wins, and the XML wins a tie.

pub mod markup;
pub mod source;
pub mod xml;

use anyhow::{Context, Result};

use crate::pack::Pack;
use crate::scd;
use crate::state::{RoomId, RoomState};

pub use source::{RoomSource, Sidecars, scripts_entry};

impl RoomId {
    /// Pack path of the room definition, e.g. `room/1001.xml`.
    pub fn xml_entry(self) -> String {
        format!("room/{:04x}.xml", self.rdt_number())
    }
}

/// Which file a room's definition comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// `room/<id>.xml`.
    Xml,
    /// `room/<id>.rdt`.
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

/// The layer depth of room `id`'s own scripts: the RDT that embeds them,
/// or the container the XML names (the XML itself when it names none).
///
/// A mod that only edits a room's XML keeps naming the base container, so
/// it does not shadow a `scd/<id>.scd` override of the same layer.
pub fn scripts_depth(pack: &Pack, id: RoomId) -> Option<usize> {
    let definition = depth(pack, id);
    if format(pack, id) != Some(Format::Xml) {
        return definition;
    }
    match read_xml(pack, id).map(|source| source.scripts) {
        Ok(Some(container)) => pack.depth_of(&container).or(definition),
        _ => definition,
    }
}

/// Read and parse room `id`'s XML definition.
fn read_xml(pack: &Pack, id: RoomId) -> Result<RoomSource> {
    let entry = id.xml_entry();
    let data = pack.read(&entry)?;
    let text = std::str::from_utf8(&data).with_context(|| format!("{entry} is not UTF-8"))?;
    xml::parse(text, id).with_context(|| format!("invalid {entry}"))
}

/// A loaded room definition: the room state, plus where its own scripts are.
#[derive(Debug)]
pub struct Loaded {
    pub state: RoomState,
    scripts: OwnScripts,
}

/// Where a room's own scripts come from.
#[derive(Debug)]
enum OwnScripts {
    /// Embedded in the RDT (its bytes).
    Rdt(Vec<u8>),
    /// The SCD container the XML names, if any.
    Container(Option<String>),
}

impl Loaded {
    /// A room parsed from RDT bytes `data`, whose scripts are embedded.
    pub fn from_rdt(state: RoomState, data: Vec<u8>) -> Self {
        Self {
            state,
            scripts: OwnScripts::Rdt(data),
        }
    }

    /// Build room `id` from XML definition `text`, reading its sidecars
    /// from `pack`.
    pub fn from_xml(pack: &Pack, id: RoomId, text: &str) -> Result<Self> {
        let source = xml::parse(text, id)?;
        let state = source.build(id, pack)?;
        Ok(Self {
            state,
            scripts: OwnScripts::Container(source.scripts),
        })
    }

    /// The room's own scripts: the RDT's SCD sections or the container the
    /// XML names. A definition without scripts has none.
    pub fn own_scripts(&self, pack: &Pack) -> Result<scd::ir::Scripts> {
        match &self.scripts {
            OwnScripts::Rdt(data) => scd::reader::parse(data).context("invalid room SCD scripts"),
            OwnScripts::Container(None) => Ok(scd::ir::Scripts::default()),
            OwnScripts::Container(Some(entry)) => {
                let data = pack
                    .read(entry)
                    .with_context(|| format!("missing {entry}"))?;
                scd::reader::parse(&data).with_context(|| format!("invalid room scripts {entry}"))
            }
        }
    }
}

/// Load room `id` from whichever definition `pack` resolves it to.
pub fn load(pack: &Pack, id: RoomId) -> Result<Loaded> {
    match format(pack, id) {
        Some(Format::Rdt) => {
            let data = pack.read(&id.rdt_entry())?.into_owned();
            let state = crate::rdt::parse(&data, id)?;
            Ok(Loaded::from_rdt(state, data))
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

/// The error of a room the pack does not define.
fn missing(id: RoomId) -> String {
    format!(
        "room {:04X} is missing: no {} or {}",
        id.rdt_number(),
        id.xml_entry(),
        id.rdt_entry()
    )
}

/// Load just room `id`'s state.
pub fn load_state(pack: &Pack, id: RoomId) -> Result<RoomState> {
    load(pack, id).map(|loaded| loaded.state)
}

/// Room `id`'s own scripts (not a `scd/<id>.scd` override), without
/// building the rest of the room.
pub fn own_scripts(pack: &Pack, id: RoomId) -> Result<scd::ir::Scripts> {
    let scripts = match format(pack, id) {
        Some(Format::Xml) => OwnScripts::Container(read_xml(pack, id)?.scripts),
        Some(Format::Rdt) => OwnScripts::Rdt(pack.read(&id.rdt_entry())?.into_owned()),
        None => anyhow::bail!("{}", missing(id)),
    };
    Loaded {
        state: RoomState::default(),
        scripts,
    }
    .own_scripts(pack)
}

/// Every room id `pack` defines, sorted, with either format.
pub fn ids(pack: &Pack) -> Vec<RoomId> {
    let mut ids: Vec<RoomId> = pack
        .paths()
        .filter_map(|path| {
            let name = path.strip_prefix("room/")?;
            let stem = name
                .strip_suffix(".xml")
                .or_else(|| name.strip_suffix(".rdt"))?;
            if stem.len() != 4 {
                return None;
            }
            RoomId::parse(stem).ok()
        })
        .collect();
    ids.sort_by_key(|id| id.rdt_number());
    ids.dedup();
    ids
}

/// Convert one RDT into its XML definition, adding its sidecars to
/// `sidecars`.
///
/// The written XML, built against its own sidecars, must give the same room
/// state and scripts as the RDT parser, so a conversion never ships a room
/// that loads differently from its RDT. A room that fails adds nothing.
pub fn convert_rdt(
    data: &[u8],
    id: RoomId,
    script: crate::charset::Script,
    sidecars: &mut Sidecars,
) -> Result<String> {
    let mut own = Sidecars::default();
    let source = RoomSource::from_rdt(data, id, script, &mut own)?;
    let text = xml::write(&source, id);
    let back = xml::parse(&text, id).context("the written room XML does not parse")?;
    if back != source {
        bail_difference("does not read back as the same definition", &source, &back)?;
    }

    let mut writer = crate::pack::PackWriter::new();
    for (path, bytes) in own.clone().into_entries() {
        writer.add(&path, bytes)?;
    }
    let pack = Pack::from_bytes(writer.to_bytes()?)?;
    let loaded = Loaded::from_xml(&pack, id, &text).context("the written room does not build")?;
    let expected = crate::rdt::parse(data, id)?;
    let mut built = loaded.state.clone();
    // The prop stems only name HD replacements, which an RDT has none of.
    for asset in built.object_models.iter_mut().chain(&mut built.item_models) {
        asset.prop = None;
    }
    if format!("{built:?}") != format!("{expected:?}") {
        bail_difference("builds a different room from its RDT", &expected, &built)?;
    }
    let scripts = loaded.own_scripts(&pack)?;
    let original = scd::reader::parse(data).unwrap_or_default();
    if scripts.digest() != original.digest() {
        anyhow::bail!("the written room's scripts differ from its RDT's");
    }

    sidecars.absorb(own)?;
    Ok(text)
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
