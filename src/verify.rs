//! `arklay verify`: parse every known entry of a game pack and report a
//! per-format table.
//!
//! The classifier is path-driven: a pack path prefix or extension selects the
//! parser family the engine would use for that entry. Where a family spans
//! more than one game's format (an RDT, an EMD, a WAV), the entry's own bytes
//! decide which ([`crate::format`]), so RE2 data is never fed to an RE1
//! parser. A format the engine cannot load yet is *recognised*: it gets its
//! own row and never fails a normal run, but `--strict` fails it like an
//! opaque entry. An entry whose format is not in
//! the table is *opaque*: it is counted and never fails, so a pack carrying a
//! future or third-party asset cannot break the command. That includes a
//! `text/*.bin` name with no table parser and a `room/` folder not named by a
//! four-digit room id. `--strict` turns an opaque entry into a failure.
//!
//! For every room the command checks that the room has at least one camera
//! cut, that each cut's background (`room/<id>/cut/<camera>.bmp`) is
//! present in the merged pack, and that the embedded init/main/event scripts
//! parse, so a room the engine cannot load fails even when every present entry
//! parses.
//!
//! No failure aborts the walk: the report lists every failing path with its
//! error, and the caller exits non-zero when any known-format entry failed.

use std::collections::BTreeMap;
use std::fmt;
use std::io::Write;

use anyhow::{Context, Result, bail};

use crate::pack::Pack;
use crate::state::RoomId;

/// Bytes of a `map/tables.bin` blob: the magic plus the seven tables.
const MAP_TABLES_BYTES: usize = 4 + 6 + 6 + 16 + 16 + 16 + 4 + 4;

/// The parser family a pack entry is validated with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Format {
    /// `manifest.toml`, the pack manifest.
    Manifest,
    /// `room/<id>/room.rdt`, a room plus its embedded SCD streams.
    Rdt,
    /// `room/<id>/room.xml`, a room definition.
    RoomXml,
    /// `room/<id>/effect/*.xml`, one effect slot's sprite table.
    RoomEffect,
    /// `item/<id>/item.xml`, an item definition.
    ItemXml,
    /// `room/<id>/anim/*.emr` / `.edd`, one half of a room player-animation pair.
    RoomAnimation,
    /// `room/<id>/*.scd`, a room's script container.
    Scd,
    /// `room/<id>/cut/*`, a camera background.
    Cut,
    /// `room/<id>/mask/*`, a camera mask page.
    Mask,
    /// Any other `*.bmp`.
    Bmp,
    /// `*.tim`, mode-dispatched.
    Tim,
    /// `*.png`, a replacement image.
    Png,
    /// `*.webp`, a replacement image.
    Webp,
    /// `*.ogg`, a replacement audio stream.
    Ogg,
    /// `*.ivm`, an item-view model.
    Ivm,
    /// `*.emd`, a character model.
    Emd,
    /// `*.emw`, a weapon model.
    Emw,
    /// `*.tmd`, a held-weapon mesh.
    Tmd,
    /// `*.dor`, a door animation.
    Dor,
    /// `*.wav`, a RIFF/WAVE sample.
    Wav,
    /// `language/*.txt`, a language string file.
    Language,
    /// `map/tables.bin`, the map tables blob.
    MapTables,
    /// `data/combat.bin`, the shared combat table blob.
    Combat,
    /// `movie/*.avi`, a film: the shipped Cinepak/PCM or an H.264/AAC
    /// replacement.
    Avi,
    /// `movie/*.mp4`, an H.264/AAC replacement film.
    Mp4,
    /// `*.lua`, a sandboxed script source.
    Lua,
    /// An RE2 room (`*.rdt` with an RE2 header). TODO(re2): parse it.
    Re2Rdt,
    /// An RE2 enemy model (`*.emd` with an RE2 directory). TODO(re2): parse it.
    Re2Emd,
    /// An RE2 player model (`*.pld`). TODO(re2): parse it.
    Pld,
    /// An RE2 player weapon model (`*.plw`). TODO(re2): parse it.
    Plw,
    /// An RE2 door archive (`*.do2`). TODO(re2): parse it.
    Do2,
    /// A Microsoft ADPCM WAV (RE2 voice and music). TODO(re2): decode it.
    AdpcmWav,
    /// An MPEG program stream film (`movie/*.mpg`). TODO(re2): decode it.
    Mpeg,
    /// Any entry outside the known table.
    Opaque,
}

impl Format {
    /// The stable report name of the format.
    pub fn name(self) -> &'static str {
        match self {
            Self::Manifest => "manifest",
            Self::Rdt => "rdt",
            Self::RoomXml => "room-xml",
            Self::RoomAnimation => "animation",
            Self::RoomEffect => "room-effect",
            Self::ItemXml => "item-xml",
            Self::Scd => "scd",
            Self::Cut => "cut",
            Self::Mask => "mask",
            Self::Bmp => "bmp",
            Self::Tim => "tim",
            Self::Png => "png",
            Self::Webp => "webp",
            Self::Ogg => "ogg",
            Self::Ivm => "ivm",
            Self::Emd => "emd",
            Self::Emw => "emw",
            Self::Tmd => "tmd",
            Self::Dor => "dor",
            Self::Wav => "wav",
            Self::Language => "language",
            Self::MapTables => "maptables",
            Self::Combat => "combat",
            Self::Avi => "avi",
            Self::Mp4 => "mp4",
            Self::Lua => "lua",
            Self::Re2Rdt => "re2-rdt",
            Self::Re2Emd => "re2-emd",
            Self::Pld => "pld",
            Self::Plw => "plw",
            Self::Do2 => "do2",
            Self::AdpcmWav => "adpcm-wav",
            Self::Mpeg => "mpeg",
            Self::Opaque => "opaque",
        }
    }

    /// Whether the format has a parser and a failure is a verification error.
    pub fn known(self) -> bool {
        self != Self::Opaque && !self.unsupported()
    }

    /// Whether the format is recognised but has no parser yet: it never
    /// fails a normal run and fails `--strict` like an opaque entry.
    pub fn unsupported(self) -> bool {
        matches!(
            self,
            Self::Re2Rdt
                | Self::Re2Emd
                | Self::Pld
                | Self::Plw
                | Self::Do2
                | Self::AdpcmWav
                | Self::Mpeg
        )
    }
}

/// The item id and file name of a path inside an item folder
/// (`item/0b/item.bmp`), if `path` is in one.
fn item_file(path: &str) -> Option<(u8, String)> {
    let lower = path.to_ascii_lowercase();
    let (id, file) = lower.strip_prefix("item/")?.split_once('/')?;
    (id.len() == 2 && id.bytes().all(|byte| byte.is_ascii_hexdigit()) && !file.contains('/'))
        .then(|| Some((u8::from_str_radix(id, 16).ok()?, file.to_owned())))
        .flatten()
}

/// The item id of a per-item icon path (`item/0b/item.bmp`), if `path` is one.
fn item_icon_id(path: &str) -> Option<u8> {
    let (id, file) = item_file(path)?;
    let (stem, _) = file.rsplit_once('.')?;
    (stem == "item").then_some(id)
}

/// Whether `path` is a per-item model page (`item/0b/model.bmp`).
fn is_item_model_page(path: &str) -> bool {
    item_file(path).is_some_and(|(_, file)| file == "model.bmp")
}

/// The lowercased extension of a pack path.
fn path_extension(path: &str) -> &str {
    path.rsplit_once('.').map_or("", |(_, extension)| extension)
}

/// Whether a lowercased path ends in one of the image class's extensions.
fn is_image_extension(lower: &str) -> bool {
    crate::pack::AssetClass::Image
        .extensions()
        .iter()
        .any(|extension| lower.ends_with(&format!(".{extension}")))
}

/// The path of a lowercased entry inside its room folder (`room.xml`,
/// `cut/00.bmp`), when it is in one. A folder not named by a four-digit room
/// id is not a room the loader can ever ask for, so its entries stay opaque
/// rather than failing third-party packs.
fn room_file(lower: &str) -> Option<&str> {
    let (id, file) = lower.strip_prefix("room/")?.split_once('/')?;
    (id.len() == 4 && RoomId::parse(id).is_ok()).then_some(file)
}

/// Whether a lowercased path is a room's effect sheet (`effect/<slot>.bmp`).
fn is_effect_sheet(lower: &str) -> bool {
    room_file(lower).is_some_and(|file| file.starts_with("effect/") && file.ends_with(".bmp"))
}

/// Select the format for one pack path.
pub fn classify(path: &str) -> Format {
    let lower = path.to_ascii_lowercase();
    if lower == crate::manifest::ENTRY {
        return Format::Manifest;
    }
    if lower == "map/tables.bin" {
        return Format::MapTables;
    }
    if lower == "data/combat.bin" {
        return Format::Combat;
    }
    if let Some(file) = room_file(&lower) {
        match file {
            "room.rdt" => return Format::Rdt,
            "room.xml" => return Format::RoomXml,
            _ if file.ends_with(".scd") => return Format::Scd,
            _ if file.starts_with("effect/") && file.ends_with(".xml") => {
                return Format::RoomEffect;
            }
            _ if file.starts_with("anim/")
                && (file.ends_with(".emr") || file.ends_with(".edd")) =>
            {
                return Format::RoomAnimation;
            }
            _ if file.starts_with("cut/") && is_image_extension(file) => return Format::Cut,
            _ if file.starts_with("mask/") && is_image_extension(file) => return Format::Mask,
            _ => {}
        }
    }
    if crate::item_catalog::entry_item(&lower).is_some() {
        return Format::ItemXml;
    }
    if lower.starts_with("language/") && lower.ends_with(".txt") {
        return Format::Language;
    }
    if lower.starts_with("movie/") && lower.ends_with(".avi") {
        return Format::Avi;
    }
    if lower.starts_with("movie/") && lower.ends_with(".mp4") {
        return Format::Mp4;
    }
    match lower.rsplit_once('.') {
        Some((_, "bmp")) => Format::Bmp,
        Some((_, "tim")) => Format::Tim,
        Some((_, "png")) => Format::Png,
        Some((_, "webp")) => Format::Webp,
        Some((_, "ogg")) => Format::Ogg,
        Some((_, "ivm")) => Format::Ivm,
        Some((_, "emd")) => Format::Emd,
        Some((_, "emw")) => Format::Emw,
        Some((_, "tmd")) => Format::Tmd,
        Some((_, "dor")) => Format::Dor,
        Some((_, "wav")) => Format::Wav,
        Some((_, "avi")) => Format::Avi,
        Some((_, "lua")) => Format::Lua,
        Some((_, "pld")) => Format::Pld,
        Some((_, "plw")) => Format::Plw,
        Some((_, "do2")) => Format::Do2,
        Some((_, "mpg")) => Format::Mpeg,
        _ => Format::Opaque,
    }
}

/// Refine a path's [`classify`] with the entry's bytes where the path alone
/// cannot tell one game's format from another's: an RDT or EMD with an RE2
/// layout, a WAV holding MS-ADPCM. An `*.rdt` outside a numbered room folder
/// (an RE2 letter-stage room, `room/g010/room.rdt`) is recognised when its
/// bytes are an RE2 RDT and stays opaque otherwise.
pub fn classify_entry(path: &str, data: &[u8]) -> Format {
    use crate::format::{self, Dialect};
    let format = classify(path);
    let is_rdt = path.to_ascii_lowercase().ends_with(".rdt");
    match format {
        Format::Rdt if format::rdt(data).dialect() == Some(Dialect::Re2) => Format::Re2Rdt,
        Format::Opaque if is_rdt && format::rdt(data).dialect() == Some(Dialect::Re2) => {
            Format::Re2Rdt
        }
        Format::Emd if format::emd(data).dialect() == Some(Dialect::Re2) => Format::Re2Emd,
        Format::Wav if format::wav_codec(data) == Some(format::WAV_MS_ADPCM) => Format::AdpcmWav,
        format => format,
    }
}

/// Whether a path's family needs its bytes to pick a format.
fn needs_bytes(path: &str, format: Format) -> bool {
    matches!(format, Format::Rdt | Format::Emd | Format::Wav)
        || (format == Format::Opaque && path.to_ascii_lowercase().ends_with(".rdt"))
}

/// Per-format counters in the report.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FormatStats {
    /// Entries classified as this format.
    pub entries: u64,
    /// Entries that parsed.
    pub ok: u64,
    /// Entries that failed to parse.
    pub failed: u64,
    /// Total bytes of the format's entries.
    pub bytes: u64,
}

/// One failed entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// Entry path.
    pub path: String,
    /// Classified format.
    pub format: Format,
    /// The parser's error text.
    pub error: String,
}

/// The full verification result.
#[derive(Debug, Default, Clone)]
pub struct VerifyReport {
    /// Counters per format, present only when the pack carried one.
    pub formats: BTreeMap<Format, FormatStats>,
    /// Every failed path, in pack order.
    pub failures: Vec<Failure>,
    /// Total entries walked.
    pub entries: u64,
    /// Entries that parsed (opaque entries count as ok when not strict).
    pub ok: u64,
    /// Entries that failed.
    pub failed: u64,
    /// Opaque entries, whether or not they failed strict mode.
    pub opaque: u64,
    /// Total entry bytes.
    pub bytes: u64,
}

impl VerifyReport {
    /// Whether the pack is valid: no known-format entry failed, and no opaque
    /// entry failed either (which only happens under `--strict`).
    pub fn valid(&self) -> bool {
        self.failed == 0
    }

    /// Write the stable report: per-format table, failures, summary.
    pub fn write(&self, pack_path: &std::path::Path, out: &mut impl Write) -> std::io::Result<()> {
        writeln!(out, "pack: {}", pack_path.display())?;
        writeln!(out, "{} entries, {} bytes", self.entries, self.bytes)?;
        writeln!(
            out,
            "{:<13} {:>9} {:>9} {:>9} {:>14}",
            "format", "entries", "ok", "failed", "bytes"
        )?;
        for (format, stats) in &self.formats {
            writeln!(
                out,
                "{:<13} {:>9} {:>9} {:>9} {:>14}",
                format.name(),
                stats.entries,
                stats.ok,
                stats.failed,
                stats.bytes
            )?;
        }
        if !self.failures.is_empty() {
            writeln!(out, "failures:")?;
            for failure in &self.failures {
                writeln!(out, "  {}: {}", failure.path, failure.error)?;
            }
        }
        writeln!(
            out,
            "verify: {} entries, {} ok, {} failed, {} opaque",
            self.entries, self.ok, self.failed, self.opaque
        )
    }
}

/// Walk every entry of `pack` through its format's parser.
///
/// The walk is exhaustive and never stops at the first failure. `strict` turns
/// an unknown entry into a failure.
pub fn verify_pack(pack: &Pack, strict: bool) -> VerifyReport {
    let mut report = VerifyReport::default();
    for entry in pack.entries() {
        let path = entry.path();
        let format = match classify(path) {
            format if needs_bytes(path, format) => match pack.read(path) {
                Ok(data) => classify_entry(path, &data),
                Err(_) => format,
            },
            format => format,
        };
        let stats = report.formats.entry(format).or_default();
        stats.entries += 1;
        stats.bytes += entry.size() as u64;
        report.entries += 1;
        report.bytes += entry.size() as u64;

        let opaque = format == Format::Opaque;
        if opaque {
            report.opaque += 1;
        }
        let result = match format {
            Format::Opaque if strict => {
                Err(anyhow::anyhow!("unknown entry format (--strict): {}", path))
            }
            Format::Opaque => Ok(()),
            _ if format.unsupported() && strict => Err(anyhow::anyhow!(
                "{} is recognised but not supported yet (--strict): {}",
                format.name(),
                path
            )),
            _ if format.unsupported() => Ok(()),
            _ => verify_entry(pack, path, format)
                .with_context(|| format!("{} ({})", path, format.name())),
        };
        match result {
            Ok(()) => {
                stats.ok += 1;
                report.ok += 1;
            }
            Err(err) => {
                stats.failed += 1;
                report.failed += 1;
                report.failures.push(Failure {
                    path: path.to_owned(),
                    format,
                    error: format!("{err:#}"),
                });
            }
        }
    }
    report
}

/// Parse one entry with the parser for `format`.
fn verify_entry(pack: &Pack, path: &str, format: Format) -> Result<()> {
    let data = &*pack.read(path)?;
    match format {
        Format::Manifest => {
            let text = std::str::from_utf8(data).context("manifest.toml is not valid UTF-8")?;
            crate::manifest::Manifest::parse(text)?;
        }
        Format::Rdt => {
            let id = room_id_from_path(path)?;
            let loaded = crate::room::Loaded::from_rdt(pack, id, data.to_vec())?;
            verify_room(pack, id, &loaded)?;
        }
        Format::RoomXml => {
            let id = room_id_from_path(path)?;
            let text = std::str::from_utf8(data).context("room XML is not valid UTF-8")?;
            let loaded = crate::room::Loaded::from_xml(pack, id, text)?;
            // The engine skips a model, animation or effect sheet that does
            // not load with a warning; a pack that ships a broken one is still
            // broken.
            let warnings = &loaded.state;
            if let Some(warning) = warnings
                .model_warnings
                .iter()
                .chain(&warnings.effects.warnings)
                .next()
            {
                bail!("{warning}");
            }
            verify_room(pack, id, &loaded)?;
        }
        Format::RoomEffect => {
            let text = std::str::from_utf8(data).context("effect XML is not valid UTF-8")?;
            crate::room::xml::parse_effect(text)?;
        }
        Format::Scd => {
            crate::scd::reader::parse(data)?;
        }
        Format::RoomAnimation => {
            // Each half is checked as the pair it belongs to.
            let stem = path.rsplit_once('.').map_or(path, |(stem, _)| stem);
            let half = |extension: &str| -> Result<std::borrow::Cow<'_, [u8]>> {
                let entry = format!("{stem}.{extension}");
                pack.read(&entry)
                    .with_context(|| format!("the pair's {entry} is missing"))
            };
            crate::emd::parse_room_anim_files(&half("emr")?, &half("edd")?)?;
        }
        Format::Cut => {
            crate::assets::decode_image_bytes(path_extension(path), data)?;
        }
        Format::Mask => {
            crate::assets::decode_image_bytes(path_extension(path), data)?;
        }
        Format::Bmp if is_effect_sheet(&path.to_ascii_lowercase()) => {
            crate::bmp::decode_clut_sheet(data)?;
        }
        Format::Bmp if is_item_model_page(path) => {
            let texture = crate::bmp::decode_indexed_page(data)?;
            if (texture.width, texture.height) != (256, 256) {
                bail!(
                    "item model page is {}x{}, expected 256x256",
                    texture.width,
                    texture.height
                );
            }
        }
        Format::Bmp if crate::bmp::is_indexed_page(data) && item_icon_id(path).is_none() => {
            crate::bmp::decode_indexed_page(data)?;
        }
        Format::Bmp => {
            let image = crate::bmp::decode(data)?;
            if item_icon_id(path).is_some() && (image.width, image.height) != (40, 30) {
                bail!(
                    "item icon is {}x{}, expected 40x30",
                    image.width,
                    image.height
                );
            }
        }
        Format::Png | Format::Webp => {
            let image = crate::assets::decode_image_bytes(path_extension(path), data)?;
            if item_icon_id(path).is_some() && (image.width, image.height) != (40, 30) {
                bail!(
                    "item icon is {}x{}, expected 40x30",
                    image.width,
                    image.height
                );
            }
        }
        Format::Ogg => {
            crate::assets::decode_audio_bytes("ogg", data)?;
        }
        Format::Tim => {
            let flags = data
                .get(4..8)
                .context("TIM is truncated before its flags")?;
            let flags = u32::from_le_bytes(flags.try_into().unwrap());
            match flags & 7 {
                0 => {
                    crate::tim::decode_4bpp(data)?;
                }
                1 => {
                    crate::tim::decode_8bpp(data)?;
                }
                2 => {
                    crate::tim::decode(data)?;
                }
                mode => bail!("unsupported TIM color mode {mode}"),
            }
        }
        Format::ItemXml => {
            let text = std::str::from_utf8(data).context("item XML is not valid UTF-8")?;
            crate::item_catalog::ItemDef::parse(text)?;
        }
        Format::Ivm => {
            crate::ivm::parse(data)?;
        }
        Format::Emd => {
            crate::emd::parse(data)?;
        }
        Format::Emw => {
            crate::emd::parse_emw(data)?;
        }
        Format::Tmd => {
            crate::tmd::parse(data)?;
        }
        Format::Dor => {
            crate::door::parse(data)?;
        }
        Format::Wav => {
            crate::audio::parse_wav(data)?;
        }
        Format::Language => {
            let source = std::str::from_utf8(data)
                .map_err(|error| anyhow::anyhow!("language file is not UTF-8: {error}"))?;
            crate::language::Language::parse(source)?;
        }
        Format::MapTables => {
            // The map tab (`ui/map.lua`) reads the magic and the seven
            // tables.
            anyhow::ensure!(
                data.len() >= MAP_TABLES_BYTES && data.starts_with(b"MAP1"),
                "map table blob needs the MAP1 magic and {MAP_TABLES_BYTES} bytes"
            );
        }
        Format::Combat => {
            crate::combat::CombatTables::parse(data)?;
        }
        Format::Avi => {
            let avi = crate::avi::Avi::parse_any(data.to_vec())?;
            if avi.format().video_codec.eq_ignore_ascii_case(b"cvid") {
                crate::avi::Avi::parse(avi.into_data())?;
            } else {
                crate::media::Media::from_avi(avi)?.validate()?;
            }
        }
        Format::Mp4 => {
            crate::mp4::parse(data.to_vec())?.validate()?;
        }
        Format::Lua => {
            crate::lua::verify_source(data)?;
        }
        Format::Opaque
        | Format::Re2Rdt
        | Format::Re2Emd
        | Format::Pld
        | Format::Plw
        | Format::Do2
        | Format::AdpcmWav
        | Format::Mpeg => {
            unreachable!("opaque and unsupported entries are handled by the caller")
        }
    }
    Ok(())
}

/// Check a parsed room the way the engine loads it: it needs a camera cut,
/// every cut's background, and scripts that parse.
fn verify_room(pack: &Pack, id: RoomId, loaded: &crate::room::Loaded) -> Result<()> {
    // The engine refuses a room with no camera cuts, so verification must
    // reject it too.
    if loaded.state.cuts.is_empty() {
        bail!("room has no camera cuts");
    }
    for cut in &loaded.state.cuts {
        let stem = id.cut_stem(cut.index);
        if !pack.has_asset(&stem, crate::pack::AssetClass::Image) {
            bail!("room references missing background {stem}.bmp");
        }
    }
    // The scripts the engine runs: a corrupt embedded table is not a failure
    // when the merged pack carries an override it prefers.
    crate::engine::room_scripts(pack, id, loaded, false)?;
    Ok(())
}

/// The [`RoomId`] named by a path inside a room folder (`room/<id>/...`),
/// whatever its case.
fn room_id_from_path(path: &str) -> Result<RoomId> {
    let stem = path
        .split_once('/')
        .filter(|(prefix, _)| prefix.eq_ignore_ascii_case("room"))
        .and_then(|(_, rest)| rest.split_once('/'))
        .map(|(stem, _)| stem)
        .with_context(|| format!("`{path}` is not a room/<id>/ path"))?;
    if stem.len() != 4 {
        bail!("`{path}` does not name a four-digit room id");
    }
    RoomId::parse(stem).with_context(|| format!("`{path}` has no valid room id"))
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest;
    use crate::pack::PackWriter;
    use crate::state::Image;

    fn tiny_tim() -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&0x10u32.to_le_bytes());
        data.extend_from_slice(&2u32.to_le_bytes());
        data.extend_from_slice(&16u32.to_le_bytes());
        data.extend_from_slice(&[0; 8]);
        data.extend_from_slice(&1u16.to_le_bytes());
        data.extend_from_slice(&1u16.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data
    }

    fn tiny_bmp() -> Vec<u8> {
        crate::bmp::encode_to_vec(&Image {
            width: 1,
            height: 1,
            rgba: vec![1, 2, 3, 255],
        })
        .unwrap()
    }

    #[test]
    fn classify_maps_prefixes_and_extensions() {
        assert_eq!(classify("manifest.toml"), Format::Manifest);
        assert_eq!(classify("room/1000/room.rdt"), Format::Rdt);
        assert_eq!(classify("ROOM/1000/ROOM.RDT"), Format::Rdt);
        assert_eq!(
            room_id_from_path("ROOM/1000/ROOM.RDT").unwrap(),
            RoomId::parse("1000").unwrap()
        );
        assert_eq!(classify("room/1000/room.xml"), Format::RoomXml);
        assert_eq!(classify("room/1000/room.scd"), Format::Scd);
        assert_eq!(classify("room/1000/original.scd"), Format::Scd);
        assert_eq!(classify("room/notes.xml"), Format::Opaque);
        assert_eq!(classify("room/1000/notes.xml"), Format::Opaque);
        assert_eq!(classify("room/1000/anim/00.emr"), Format::RoomAnimation);
        assert_eq!(classify("room/1000/anim/00.edd"), Format::RoomAnimation);
        assert_eq!(classify("room/1000/effect/00.xml"), Format::RoomEffect);
        assert_eq!(classify("room/1000/object/00.tmd"), Format::Tmd);
        assert_eq!(classify("room/1000/cut/00.bmp"), Format::Cut);
        assert_eq!(classify("room/1000/cut/00.png"), Format::Cut);
        assert_eq!(classify("room/1000/mask/00.webp"), Format::Mask);
        assert_eq!(classify("room/1000/mask/00.bmp"), Format::Mask);
        assert_eq!(classify("ui/blue.tim"), Format::Tim);
        assert_eq!(classify("ui/blue.png"), Format::Png);
        assert_eq!(classify("ui/blue.webp"), Format::Webp);
        assert_eq!(classify("bgm/013.ogg"), Format::Ogg);
        assert_eq!(classify("room/1000/cut/00.ogg"), Format::Ogg);
        assert_eq!(classify("item/i00v.ivm"), Format::Ivm);
        assert_eq!(classify("item/0b/item.xml"), Format::ItemXml);
        assert_eq!(classify("item/0b/notes.xml"), Format::Opaque);
        assert_eq!(classify("item/0b/model.tmd"), Format::Tmd);
        assert_eq!(classify("enemy/20/enemy.emd"), Format::Emd);
        assert_eq!(classify("player/00/weapon/00.emw"), Format::Emw);
        assert_eq!(classify("enemy/20/weapon/01.tmd"), Format::Tmd);
        assert_eq!(classify("door/00.dor"), Format::Dor);
        assert_eq!(classify("se/a_mcn03.wav"), Format::Wav);
        assert_eq!(classify("language/en-US.txt"), Format::Language);
        assert_eq!(classify("Language/JA-JP.TXT"), Format::Language);
        assert_eq!(classify("map/tables.bin"), Format::MapTables);
        assert_eq!(classify("data/combat.bin"), Format::Combat);
        assert_eq!(classify("movie/00.avi"), Format::Avi);
        assert_eq!(classify("movie/OJ.mp4"), Format::Mp4);
        assert_eq!(classify("enemy/13/enemy.lua"), Format::Lua);
        assert_eq!(classify("lua/demo.lua"), Format::Lua);
        assert_eq!(classify("data/core00.esp"), Format::Opaque);
        // A retired text table stays opaque instead of breaking the command.
        assert_eq!(classify("text/credits.bin"), Format::Opaque);
        assert_eq!(classify("language/notes.md"), Format::Opaque);
        assert_eq!(classify("room/1000.rdt"), Format::Opaque);
        assert_eq!(classify("room/bonus/room.rdt"), Format::Opaque);
        assert_eq!(classify("room/100/room.rdt"), Format::Opaque);
        assert_eq!(classify("room/zzzz/room.rdt"), Format::Opaque);
    }

    #[test]
    fn item_art_entries_are_checked_at_their_required_shapes() {
        let page = |width, height| {
            crate::bmp::encode_indexed_page(
                &crate::model::Texture8 {
                    width,
                    height,
                    indices: vec![1; (width * height) as usize],
                    palettes: vec![[0u8; 4]; crate::model::PALETTE_ROW_LEN],
                    stp: vec![false; crate::model::PALETTE_ROW_LEN],
                },
                0,
            )
            .unwrap()
        };
        let icon = |width, height| {
            crate::bmp::encode_to_vec(&Image {
                width,
                height,
                rgba: [1, 2, 3, 255].repeat((width * height) as usize),
            })
            .unwrap()
        };
        let mut writer = PackWriter::new();
        writer.add("item/01/model.bmp", page(256, 256)).unwrap();
        writer.add("item/01/item.bmp", icon(40, 30)).unwrap();
        writer
            .add(
                "item/01/item.xml",
                crate::item_catalog::ITEM_FILES[0].1.into(),
            )
            .unwrap();
        let pack = crate::pack::Pack::from_bytes(writer.to_bytes().unwrap()).unwrap();
        let report = verify_pack(&pack, true);
        assert!(report.valid(), "{:?}", report.failures);

        // A model page must be the indexed 256x256 page the loader reads.
        let mut writer = PackWriter::new();
        writer.add("item/01/model.bmp", icon(256, 256)).unwrap();
        let pack = crate::pack::Pack::from_bytes(writer.to_bytes().unwrap()).unwrap();
        let report = verify_pack(&pack, true);
        assert!(!report.valid());
        assert!(report.failures[0].error.contains("256"));

        // An icon is exactly 40x30.
        let mut writer = PackWriter::new();
        writer.add("item/01/item.bmp", icon(80, 60)).unwrap();
        let pack = crate::pack::Pack::from_bytes(writer.to_bytes().unwrap()).unwrap();
        let report = verify_pack(&pack, true);
        assert!(!report.valid());
        assert!(report.failures[0].error.contains("40x30"));

        // An item definition must parse.
        let mut writer = PackWriter::new();
        writer
            .add("item/01/item.xml", b"<item kind=\"spoon\" />".to_vec())
            .unwrap();
        let pack = crate::pack::Pack::from_bytes(writer.to_bytes().unwrap()).unwrap();
        let report = verify_pack(&pack, true);
        assert!(!report.valid());
    }

    #[test]
    fn future_text_tables_and_odd_room_names_are_opaque() {
        let mut writer = PackWriter::new();
        writer.add("text/credits.bin", vec![1, 2, 3]).unwrap();
        writer.add("room/bonus.rdt", vec![1, 2, 3]).unwrap();
        let pack = Pack::from_bytes(writer.to_bytes().unwrap()).unwrap();

        let lenient = verify_pack(&pack, false);
        assert!(lenient.valid());
        assert_eq!(lenient.opaque, 2);
        assert_eq!(lenient.failed, 0);
        assert!(!lenient.formats.contains_key(&Format::Language));
        assert!(!lenient.formats.contains_key(&Format::Rdt));

        let strict = verify_pack(&pack, true);
        assert!(!strict.valid());
        assert_eq!(strict.failed, 2);
        assert_eq!(strict.opaque, 2);
    }

    #[test]
    fn a_room_without_camera_cuts_fails_like_the_engine() {
        // Parses as an RDT and its (empty) SCD parses, but the engine refuses
        // a room with no camera cuts on load.
        let rdt = vec![0u8; 0x94];
        let mut writer = PackWriter::new();
        writer.add("room/1000/room.rdt", rdt).unwrap();
        let pack = Pack::from_bytes(writer.to_bytes().unwrap()).unwrap();

        let report = verify_pack(&pack, false);

        assert!(!report.valid());
        assert_eq!(report.failures.len(), 1);
        assert!(
            report.failures[0].error.contains("no camera cuts"),
            "{}",
            report.failures[0].error
        );
    }

    #[test]
    fn a_corrupt_script_pointer_fails_an_otherwise_loadable_rdt() {
        // The room header and its camera parse and the background is present,
        // but the event script pointer is out of bounds: the engine's SCD
        // reader rejects the room on load, so verification must too.
        let mut rdt = vec![0u8; 0x94];
        rdt[0x01] = 1;
        rdt.extend_from_slice(&[0u8; 44]);
        let events = 0x48 + 8 * 4;
        rdt[events..events + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        let mut writer = PackWriter::new();
        writer.add("room/1000/room.rdt", rdt).unwrap();
        writer.add("room/1000/cut/00.bmp", tiny_bmp()).unwrap();
        let pack = Pack::from_bytes(writer.to_bytes().unwrap()).unwrap();

        let report = verify_pack(&pack, false);

        assert!(!report.valid(), "a corrupt script table must fail");
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.failures[0].path, "room/1000/room.rdt");
        assert!(
            report.failures[0].error.contains("event SCD table pointer"),
            "{}",
            report.failures[0].error
        );
    }

    #[test]
    fn a_held_weapon_tmd_entry_parses() {
        // An empty TMD: magic, flags, zero objects.
        let mut tmd = Vec::new();
        tmd.extend_from_slice(&0x41u32.to_le_bytes());
        tmd.extend_from_slice(&0u32.to_le_bytes());
        tmd.extend_from_slice(&0u32.to_le_bytes());
        let mut writer = PackWriter::new();
        writer.add("enemy/20/weapon/01.tmd", tmd).unwrap();
        let pack = Pack::from_bytes(writer.to_bytes().unwrap()).unwrap();
        let report = verify_pack(&pack, false);
        assert!(report.valid(), "failures: {:?}", report.failures);
        assert_eq!(report.formats[&Format::Tmd].ok, 1);

        // A truncated TMD fails.
        let mut writer = PackWriter::new();
        writer
            .add("enemy/20/weapon/01.tmd", vec![0x41, 0, 0])
            .unwrap();
        let pack = Pack::from_bytes(writer.to_bytes().unwrap()).unwrap();
        let report = verify_pack(&pack, false);
        assert!(!report.valid());
        assert_eq!(report.formats[&Format::Tmd].failed, 1);
    }

    #[test]
    fn a_good_pack_reports_every_format_and_zero_failures() {
        let mut writer = PackWriter::new();
        writer
            .add(
                manifest::ENTRY,
                manifest::Manifest::base("re1").render().into_bytes(),
            )
            .unwrap();
        writer.add("ui/blue.tim", tiny_tim()).unwrap();
        writer.add("room/1000/cut/00.bmp", tiny_bmp()).unwrap();
        writer.add("room/1000/mask/00.bmp", tiny_bmp()).unwrap();
        let pack = Pack::from_bytes(writer.to_bytes().unwrap()).unwrap();

        let report = verify_pack(&pack, false);

        assert!(report.valid(), "failures: {:?}", report.failures);
        assert_eq!(report.entries, 4);
        assert_eq!(report.ok, 4);
        assert_eq!(report.failed, 0);
        assert_eq!(report.opaque, 0);
        assert_eq!(report.formats[&Format::Manifest].ok, 1);
        assert_eq!(report.formats[&Format::Tim].entries, 1);
        assert_eq!(report.formats[&Format::Cut].entries, 1);
        assert_eq!(report.formats[&Format::Mask].entries, 1);
    }

    #[test]
    fn a_bad_entry_fails_with_its_path_and_format() {
        let mut writer = PackWriter::new();
        writer.add("ui/blue.tim", b"not a tim".to_vec()).unwrap();
        let pack = Pack::from_bytes(writer.to_bytes().unwrap()).unwrap();

        let report = verify_pack(&pack, false);

        assert!(!report.valid());
        assert_eq!(report.failed, 1);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.failures[0].path, "ui/blue.tim");
        assert_eq!(report.failures[0].format, Format::Tim);
        assert_eq!(report.formats[&Format::Tim].failed, 1);
    }

    #[test]
    fn unknown_entries_are_opaque_unless_strict() {
        let mut writer = PackWriter::new();
        writer.add("data/core00.esp", vec![1, 2, 3]).unwrap();
        writer.add("lua/demo.lua", b"return 1".to_vec()).unwrap();
        let pack = Pack::from_bytes(writer.to_bytes().unwrap()).unwrap();

        let lenient = verify_pack(&pack, false);
        assert!(lenient.valid());
        assert_eq!(lenient.opaque, 1);
        assert_eq!(
            lenient.ok, 2,
            "the Lua source compiles and the opaque entry passes leniently"
        );

        let strict = verify_pack(&pack, true);
        assert!(!strict.valid());
        assert_eq!(strict.failed, 1);
        assert_eq!(strict.opaque, 1);
        assert!(
            strict
                .failures
                .iter()
                .all(|failure| failure.error.contains("--strict"))
        );
    }

    #[test]
    fn an_rdt_with_a_missing_background_fails_the_reference_check() {
        // Build the smallest RDT the parser accepts: one camera cut and no
        // other tables.
        let mut rdt = vec![0u8; 0x94];
        rdt[0x01] = 1;
        rdt.extend_from_slice(&[0u8; 44]);
        while !rdt.len().is_multiple_of(4) {
            rdt.push(0);
        }
        let mut writer = PackWriter::new();
        writer.add("room/1000/room.rdt", rdt).unwrap();
        let pack = Pack::from_bytes(writer.to_bytes().unwrap()).unwrap();
        let report = verify_pack(&pack, false);

        assert!(!report.valid(), "a missing cut must fail verification");
        assert_eq!(report.failures.len(), 1);
        assert!(
            report.failures[0].error.contains("room/1000/cut/00.bmp"),
            "{}",
            report.failures[0].error
        );
    }

    #[test]
    fn an_scd_override_rescues_a_room_with_corrupt_embedded_scripts() {
        // The RDT's embedded event pointer is corrupt, but the merged pack
        // carries a valid standalone override: the engine loads the override,
        // so verification must accept the room.
        let mut rdt = vec![0u8; 0x94];
        rdt[0x01] = 1;
        rdt.extend_from_slice(&[0u8; 44]);
        let events = 0x48 + 8 * 4;
        rdt[events..events + 4].copy_from_slice(&u32::MAX.to_le_bytes());

        let mut scd = vec![0u8; 0x94];
        let init = scd.len();
        scd.extend_from_slice(&4u16.to_le_bytes());
        scd.extend_from_slice(&[0x0E, 0x00]);
        scd.extend_from_slice(&0u16.to_le_bytes());
        scd[0x48 + 6 * 4..0x48 + 7 * 4].copy_from_slice(&(init as u32).to_le_bytes());

        let mut writer = PackWriter::new();
        writer.add("room/1000/room.rdt", rdt).unwrap();
        writer.add("room/1000/cut/00.bmp", tiny_bmp()).unwrap();
        writer.add("room/1000/room.scd", scd).unwrap();
        let pack = Pack::from_bytes(writer.to_bytes().unwrap()).unwrap();

        let report = verify_pack(&pack, false);

        assert!(report.valid(), "failures: {:?}", report.failures);
    }

    #[test]
    fn the_report_writes_the_table_and_failure_lines() {
        let mut writer = PackWriter::new();
        writer.add("ui/blue.tim", tiny_tim()).unwrap();
        writer.add("data/core00.esp", vec![1]).unwrap();
        let pack = Pack::from_bytes(writer.to_bytes().unwrap()).unwrap();
        let report = verify_pack(&pack, false);

        let mut out = Vec::new();
        report
            .write(std::path::Path::new("pack.akpak"), &mut out)
            .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("pack: pack.akpak"), "{text}");
        assert!(text.contains("format"), "{text}");
        let tim: Vec<&str> = text
            .lines()
            .find(|line| line.starts_with("tim "))
            .unwrap()
            .split_whitespace()
            .collect();
        assert_eq!(tim, ["tim", "1", "1", "0", "26"]);
        assert!(
            text.contains("verify: 2 entries, 2 ok, 0 failed, 1 opaque"),
            "{text}"
        );
    }
}
