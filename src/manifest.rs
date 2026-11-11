//! Pack manifest (`manifest.toml`).
//!
//! The manifest is an ordinary pack entry that describes its pack: the format
//! version, a stable id, a human name, version and description, whether the pack is a base
//! or a mod, the base id a mod shadows, the layer order mods sort by, the
//! language of its own room text, the region of its room scripts, a
//! free-form engine note and the explicit Lua hook list.
//!
//! A manifest names no file-format dialect: a pack may mix RE1- and
//! RE2-format files, and each loader detects a file's format from the file
//! itself ([`crate::format`]). The `rdt` and `scd` keys older packs carry are
//! accepted and ignored.
//!
//! The parser is a deliberately small, dependency-free TOML subset: comments,
//! blank lines, `key = value` with strings, integers, booleans and string
//! arrays, plus `[section]` headers. Every malformed value names its line, so
//! an authoring typo is reported where it was written. Booleans are accepted by
//! the parser even though no current manifest field is one, so the grammar the
//! tests pin is the grammar the parser implements.

use std::collections::HashSet;

use anyhow::{Context, Result, anyhow, bail};

use crate::budget;
use crate::scd::akscr::Region;
use crate::scd::opcode::system;

/// The pack entry holding a rendered manifest.
pub const ENTRY: &str = "manifest.toml";

/// The manifest format version this module writes and accepts.
pub const FORMAT: u32 = 1;

/// The most inventory slots a character can have (`chris_slots`,
/// `jill_slots`, `start_items`).
pub const MAX_SLOTS: u8 = 8;

/// The base a generated pack without a `base` key was made from: BioRand,
/// the only generator so far, writes RE 1 games and records no `base`.
pub const GENERATED_BASE: &str = "re1";

/// Whether a pack is a base game pack or a mod layered over one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackKind {
    /// The base pack: the full converted game.
    Base,
    /// A mod pack that layers over a base pack.
    Mod,
}

impl PackKind {
    /// The manifest spelling of this kind.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Base => "base",
            Self::Mod => "mod",
        }
    }
}

impl std::fmt::Display for PackKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A parsed pack manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// Manifest format version; always [`FORMAT`].
    pub format: u32,
    /// Stable pack id, e.g. `re1` or `demo`.
    pub id: String,
    /// Human-readable pack name.
    pub name: Option<String>,
    /// Pack version string.
    pub version: Option<String>,
    /// Free-form description shown to players, e.g. credits or a seed.
    pub description: Option<String>,
    /// The mod catalog's category for the pack, e.g. `High Definition`.
    pub category: Option<String>,
    /// The people or teams who made the pack's content.
    pub authors: Vec<String>,
    /// Further acknowledgements: sources, contributors, thanks.
    pub credits: Vec<String>,
    /// Free-form labels, e.g. [`TAG_OFFICIAL`] for a pack converted from a
    /// retail game or [`TAG_RANDOMIZER`] for a generated one. The launcher
    /// lists official game packs first.
    pub tags: Vec<String>,
    /// The pack's home page.
    pub homepage: Option<String>,
    /// The pack entry holding a 4:3 preview image, e.g. `thumbnail.webp`.
    pub thumbnail: Option<String>,
    /// Whether this is the base pack or a mod.
    pub kind: PackKind,
    /// Mods: the base pack id this mod layers over. A generated base pack
    /// (one with a [`Self::seed`]) may name the pack it was generated from
    /// here, so that pack's mods layer over it too
    /// ([`Self::generated_from`]).
    pub base: Option<String>,
    /// Mods: layer order; lower layers are applied first.
    pub load_order: i32,
    /// Language of the pack's own room text, e.g. `ja-JP`.
    ///
    /// The converted install's RDT messages are in this language; the
    /// `language/<tag>.txt` files a pack carries are separate.
    pub language: Option<String>,
    /// Optional note about the engine build this pack targets.
    pub engine: Option<String>,
    /// The conversion revision that produced the pack; a launcher compares
    /// it with [`crate::convert::CONVERT_VERSION`]. Older packs do not carry
    /// it.
    pub convert_version: Option<u32>,
    /// A fingerprint of the engine-owned scripts and language sources that
    /// were packed; see [`crate::convert::packed_source_hash`].
    pub source_hash: Option<String>,
    /// Explicit Lua hook paths, in load order.
    pub lua: Vec<String>,
    /// Base packs: the three-digit room a new game starts in (`100`), the
    /// character picking its player variant. Absent, a new game starts in
    /// RE1's main hall.
    pub start_room: Option<String>,
    /// Base packs: where a new game places the player in the start room, and
    /// the facing and health it starts with. Absent, the player enters the
    /// room through its first door, with RE1's health.
    pub start_pos: Option<[i32; 3]>,
    pub start_angle: Option<u16>,
    pub start_health: Option<i16>,
    /// Base packs: the region whose room scripts the converted install
    /// ships (`convert-game` records it), which picks a mod's per-region
    /// script builds. Absent, a mod's scripts run their USA build.
    pub region: Option<Region>,
    /// Base packs: the randomizer seed that generated the pack. Its presence
    /// raises `RANDOMIZER` for scripts; [`Self::biorand_flags`] names the rest.
    pub seed: Option<u32>,
    /// Base packs: the revision of the randomizer that generated the pack.
    pub generator_version: Option<u32>,
    /// Base packs: the character a new game plays (`0` Chris, `1` Jill),
    /// without the character select screen or the prologue film. RE 1's
    /// select screen offers nothing else, so no new-game choice is lost.
    pub character: Option<u8>,
    /// Base packs: the bank-5 system flags the pack raises, by their script
    /// names (see [`crate::scd::opcode::system`]). A seed raises `RANDOMIZER`
    /// on its own; the list names the others. Absent, the pack predates the
    /// list, so a seeded one also raises the proof of concept's
    /// `DOOR_RANDOMIZER` and `ENEMY_RANDOMIZER`. Parsing refuses a name that is
    /// not a system flag and one listed twice.
    pub biorand_flags: Option<Vec<String>>,
    /// Base packs: the revision of the `arklay-biorand` plan the pack was
    /// written from (`arklay_biorand::plan::GENERATOR_VERSION`), beside the
    /// emitter's own [`Self::generator_version`].
    pub plan_version: Option<u32>,
    /// Base packs: what a new game sets up beyond the shipped start.
    pub setup: GameSetup,
}

/// What a base pack's new game sets up beyond the shipped start: the
/// options a generated game records (`scenario_flags`, `start_items`,
/// `chris_slots`, `jill_slots`). Every field is empty in an ordinary pack.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GameSetup {
    /// Scenario flags (bank 0) a new game raises (`true`) or clears, after
    /// the shipped start: `scenario_flags = ["122=1", "123=0"]`. A save keeps
    /// bank 0, so a load needs nothing more.
    pub scenario_flags: Vec<(u8, bool)>,
    /// The player's start inventory as `(item, amount)`, in place of what
    /// `game/new_game.lua` gives the player (Rebecca's slots are left as the
    /// script sets them): `start_items = ["1:0", "2:15"]`.
    pub start_items: Option<Vec<(u8, u8)>>,
    /// Chris's and Jill's inventory slot counts, in place of the original's
    /// six and eight (1 to 8), in RE 1 rooms: `chris_slots = 8`. Not saved,
    /// so every path into a game applies them
    /// ([`crate::game::GameState::pack_inventory_slots`]).
    pub chris_slots: Option<u8>,
    pub jill_slots: Option<u8>,
}

impl GameSetup {
    /// Whether the setup holds nothing.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// The tag of a pack converted from a retail game.
pub const TAG_OFFICIAL: &str = "official";
/// The tag of a pack a randomizer generated.
pub const TAG_RANDOMIZER: &str = "randomizer";

impl Manifest {
    /// Whether the pack carries `tag` (compared without case).
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|own| own.eq_ignore_ascii_case(tag))
    }

    /// A base manifest for `id`.
    pub fn base(id: impl Into<String>) -> Self {
        Self {
            format: FORMAT,
            id: id.into(),
            name: None,
            version: None,
            description: None,
            category: None,
            authors: Vec::new(),
            credits: Vec::new(),
            tags: Vec::new(),
            homepage: None,
            thumbnail: None,
            kind: PackKind::Base,
            base: None,
            load_order: 0,
            language: None,
            engine: None,
            convert_version: None,
            source_hash: None,
            lua: Vec::new(),
            start_room: None,
            start_pos: None,
            start_angle: None,
            start_health: None,
            seed: None,
            generator_version: None,
            biorand_flags: None,
            plan_version: None,
            setup: GameSetup::default(),
            character: None,
            region: None,
        }
    }

    /// Whether this pack declares itself a mod.
    pub fn is_mod(&self) -> bool {
        self.kind == PackKind::Mod
    }

    /// The id of the pack a generated base pack (one with a [`Self::seed`])
    /// was made from, whose mods layer over it: its `base`, else
    /// [`GENERATED_BASE`] for a pack written before the generator recorded
    /// it. `None` for any other pack.
    pub fn generated_from(&self) -> Option<&str> {
        if self.kind != PackKind::Base || self.seed.is_none() {
            return None;
        }
        Some(self.base.as_deref().unwrap_or(GENERATED_BASE))
    }

    /// The bank-5 system flags this manifest raises: `RANDOMIZER` for a seed,
    /// then the names in `biorand_flags`. A seeded pack without the list
    /// predates it, so it also raises the proof of concept's
    /// `DOOR_RANDOMIZER` and `ENEMY_RANDOMIZER`, and the bits its scripts
    /// test for them ([`system::PROOF_OF_CONCEPT`]).
    pub fn system_flags(&self) -> Vec<u8> {
        let mut flags = Vec::new();
        if self.seed.is_some() {
            flags.push(system::RANDOMIZER);
        }
        match &self.biorand_flags {
            Some(names) => flags.extend(names.iter().filter_map(|name| system::bit_named(name))),
            None if self.seed.is_some() => {
                flags.extend([system::DOOR_RANDOMIZER, system::ENEMY_RANDOMIZER]);
                // Its room scripts test the bits the flags had then.
                flags.extend(system::PROOF_OF_CONCEPT);
            }
            None => {}
        }
        flags
    }

    /// Parse a manifest, naming the line of every malformed value.
    ///
    /// The source, every line and the statement count are capped by
    /// [`budget::MAX_TEXT_BYTES`], [`budget::MAX_MANIFEST_LINE`] and
    /// [`budget::MAX_MANIFEST_ITEMS`].
    pub fn parse(text: &str) -> Result<Self> {
        budget::check_len(text.len(), budget::MAX_TEXT_BYTES, "manifest size")?;
        let mut format = None;
        let mut id = None;
        let mut name = None;
        let mut version = None;
        let mut description = None;
        let mut category = None;
        let mut authors = None;
        let mut credits = None;
        let mut tags = None;
        let mut homepage = None;
        let mut thumbnail = None;
        let mut kind = None;
        let mut base = None;
        let mut load_order = None;
        let mut language = None;
        let mut engine = None;
        let mut convert_version = None;
        let mut source_hash = None;
        let mut lua = None;
        let mut start_room = None;
        let mut start_pos = None;
        let mut start_angle = None;
        let mut seed = None;
        let mut generator_version = None;
        let mut character = None;
        let mut start_health = None;
        let mut region = None;
        let mut biorand_flags = None;
        let mut plan_version = None;
        let mut setup = GameSetup::default();
        let mut seen: HashSet<String> = HashSet::new();

        for item in parse_items(text)? {
            let section = item.section.as_deref().unwrap_or("pack");
            if section != "pack" {
                bail!(
                    "manifest.toml line {}: unknown section [{section}]",
                    item.line
                );
            }
            if !seen.insert(item.key.clone()) {
                bail!(
                    "manifest.toml line {}: duplicate key `{}`",
                    item.line,
                    item.key
                );
            }
            let line = item.line;
            match item.key.as_str() {
                "format" => {
                    let Value::Integer(value) = item.value else {
                        bail!("manifest.toml line {line}: `format` must be an integer");
                    };
                    let value = u32::try_from(value)
                        .map_err(|_| anyhow!("manifest.toml line {line}: bad format {value}"))?;
                    if value != FORMAT {
                        bail!(
                            "manifest.toml line {line}: unsupported manifest format {value} (expected {FORMAT})"
                        );
                    }
                    format = Some(value);
                }
                "id" => id = Some(non_empty_string(item, "id")?),
                "name" => name = Some(string_value(item, "name")?),
                "version" => version = Some(string_value(item, "version")?),
                "description" => description = Some(string_value(item, "description")?),
                "category" => category = Some(non_empty_string(item, "category")?),
                "authors" => authors = Some(string_array(item, "authors")?),
                "credits" => credits = Some(string_array(item, "credits")?),
                "tags" => tags = Some(string_array(item, "tags")?),
                "homepage" => homepage = Some(non_empty_string(item, "homepage")?),
                "thumbnail" => thumbnail = Some(non_empty_string(item, "thumbnail")?),
                "kind" => {
                    let text = string_value(item, "kind")?;
                    kind = Some(match text.as_str() {
                        "base" => PackKind::Base,
                        "mod" => PackKind::Mod,
                        other => bail!(
                            "manifest.toml line {line}: `kind` must be \"base\" or \"mod\", found \"{other}\""
                        ),
                    });
                }
                "base" => base = Some(non_empty_string(item, "base")?),
                "load_order" => {
                    let Value::Integer(value) = item.value else {
                        bail!("manifest.toml line {line}: `load_order` must be an integer");
                    };
                    load_order = Some(i32::try_from(value).map_err(|_| {
                        anyhow!("manifest.toml line {line}: `load_order` does not fit an i32")
                    })?);
                }
                // Older packs name a pack-wide dialect; formats are now
                // detected per file, so the keys are read and ignored.
                "rdt" | "scd" => {
                    let key = item.key.clone();
                    string_value(item, &key)?;
                }
                "language" => language = Some(non_empty_string(item, "language")?),
                "engine" => engine = Some(string_value(item, "engine")?),
                "convert_version" => {
                    let Value::Integer(value) = item.value else {
                        bail!("manifest.toml line {line}: `convert_version` must be an integer");
                    };
                    convert_version = Some(u32::try_from(value).map_err(|_| {
                        anyhow!("manifest.toml line {line}: bad convert_version {value}")
                    })?);
                }
                "source_hash" => source_hash = Some(non_empty_string(item, "source_hash")?),
                "lua" => {
                    let Value::Array(values) = item.value else {
                        bail!("manifest.toml line {line}: `lua` must be a string array");
                    };
                    budget::check_len(
                        values.len(),
                        budget::MAX_MANIFEST_LUA,
                        "manifest lua list length",
                    )?;
                    if values.iter().any(String::is_empty) {
                        bail!("manifest.toml line {line}: `lua` entries must not be empty");
                    }
                    lua = Some(values);
                }
                "start_room" => {
                    let room = non_empty_string(item, "start_room")?;
                    crate::state::RoomId::from_room_and_player(&room, 0).map_err(|error| {
                        anyhow!("manifest.toml line {line}: bad `start_room`: {error}")
                    })?;
                    start_room = Some(room);
                }
                "start_pos" => {
                    let text = non_empty_string(item, "start_pos")?;
                    let values: Vec<i32> = text
                        .split(',')
                        .map(|value| value.trim().parse())
                        .collect::<Result<_, _>>()
                        .map_err(|error| {
                            anyhow!("manifest.toml line {line}: bad `start_pos`: {error}")
                        })?;
                    let [x, y, z] = values[..] else {
                        bail!("manifest.toml line {line}: `start_pos` must be \"x,y,z\"");
                    };
                    start_pos = Some([x, y, z]);
                }
                "seed" => seed = Some(parsed_field(item, "seed")?),
                "generator_version" => {
                    generator_version = Some(parsed_field(item, "generator_version")?)
                }
                "character" => {
                    let value: u8 = parsed_field(item, "character")?;
                    if value > 1 {
                        bail!("manifest.toml line {line}: bad `character`: {value}");
                    }
                    character = Some(value);
                }
                "biorand_flags" => {
                    let Value::Array(names) = item.value else {
                        bail!("manifest.toml line {line}: `biorand_flags` must be a string array");
                    };
                    for (index, name) in names.iter().enumerate() {
                        if system::bit_named(name).is_none() {
                            bail!(
                                "manifest.toml line {line}: unknown system flag `{name}` in `biorand_flags`"
                            );
                        }
                        if names[..index].contains(name) {
                            bail!("manifest.toml line {line}: `biorand_flags` repeats `{name}`");
                        }
                    }
                    biorand_flags = Some(names);
                }
                "plan_version" => plan_version = Some(parsed_field(item, "plan_version")?),
                "scenario_flags" => {
                    let Value::Array(values) = item.value else {
                        bail!("manifest.toml line {line}: `scenario_flags` must be a string array");
                    };
                    for value in &values {
                        let parsed = value.split_once('=').and_then(|(flag, raised)| {
                            let raised = match raised.trim() {
                                "1" => true,
                                "0" => false,
                                _ => return None,
                            };
                            Some((flag.trim().parse::<u8>().ok()?, raised))
                        });
                        let Some((flag, raised)) = parsed else {
                            bail!(
                                "manifest.toml line {line}: bad `scenario_flags` entry `{value}` (expected \"<flag>=0\" or \"<flag>=1\")"
                            );
                        };
                        if setup.scenario_flags.iter().any(|(seen, _)| *seen == flag) {
                            bail!("manifest.toml line {line}: `scenario_flags` repeats {flag}");
                        }
                        setup.scenario_flags.push((flag, raised));
                    }
                }
                "start_items" => {
                    let Value::Array(values) = item.value else {
                        bail!("manifest.toml line {line}: `start_items` must be a string array");
                    };
                    if values.len() > usize::from(MAX_SLOTS) {
                        bail!(
                            "manifest.toml line {line}: `start_items` lists more than {MAX_SLOTS} items"
                        );
                    }
                    let mut items = Vec::with_capacity(values.len());
                    for value in &values {
                        let parsed = value.split_once(':').and_then(|(id, amount)| {
                            Some((
                                id.trim().parse::<u8>().ok()?,
                                amount.trim().parse::<u8>().ok()?,
                            ))
                        });
                        match parsed {
                            Some((id, amount)) if id != 0 => items.push((id, amount)),
                            _ => bail!(
                                "manifest.toml line {line}: bad `start_items` entry `{value}` (expected \"<item>:<amount>\")"
                            ),
                        }
                    }
                    setup.start_items = Some(items);
                }
                "chris_slots" | "jill_slots" => {
                    let Value::Integer(value) = item.value else {
                        bail!(
                            "manifest.toml line {line}: `{}` must be an integer",
                            item.key
                        );
                    };
                    let Some(slots) = u8::try_from(value)
                        .ok()
                        .filter(|slots| (1..=MAX_SLOTS).contains(slots))
                    else {
                        bail!(
                            "manifest.toml line {line}: `{}` must be 1 to {MAX_SLOTS}, found {value}",
                            item.key
                        );
                    };
                    if item.key == "chris_slots" {
                        setup.chris_slots = Some(slots);
                    } else {
                        setup.jill_slots = Some(slots);
                    }
                }
                "start_angle" => {
                    let text = non_empty_string(item, "start_angle")?;
                    start_angle = Some(text.parse().map_err(|error| {
                        anyhow!("manifest.toml line {line}: bad `start_angle`: {error}")
                    })?);
                }
                "start_health" => {
                    let text = non_empty_string(item, "start_health")?;
                    start_health = Some(text.parse().map_err(|error| {
                        anyhow!("manifest.toml line {line}: bad `start_health`: {error}")
                    })?);
                }
                "region" => {
                    let name = string_value(item, "region")?;
                    region = Some(Region::parse(&name).ok_or_else(|| {
                        anyhow!(
                            "manifest.toml line {line}: `region` must be \"USA\", \"JPN\" \
                             or \"EUR\", found \"{name}\""
                        )
                    })?);
                }
                unknown => bail!("manifest.toml line {line}: unknown key `{unknown}`"),
            }
        }

        let format = format.context("manifest.toml: missing `format`")?;
        let id = id.context("manifest.toml: missing `id`")?;
        let kind = kind.context("manifest.toml: missing `kind`")?;
        if kind == PackKind::Mod && base.is_none() {
            bail!("manifest.toml: a mod manifest must name a `base`");
        }
        if kind == PackKind::Mod
            && (start_room.is_some()
                || start_pos.is_some()
                || start_angle.is_some()
                || start_health.is_some()
                || seed.is_some()
                || generator_version.is_some()
                || character.is_some())
        {
            bail!("manifest.toml: `start_room` belongs to a base pack, not a mod");
        }
        if kind == PackKind::Mod && region.is_some() {
            bail!("manifest.toml: `region` belongs to a base pack, not a mod");
        }
        if kind == PackKind::Mod && biorand_flags.is_some() {
            bail!("manifest.toml: `biorand_flags` belongs to a base pack, not a mod");
        }
        if kind == PackKind::Mod && (plan_version.is_some() || !setup.is_empty()) {
            bail!(
                "manifest.toml: `plan_version`, `scenario_flags`, `start_items` and the slot counts belong to a base pack, not a mod"
            );
        }

        Ok(Self {
            format,
            id,
            name,
            version,
            description,
            category,
            authors: authors.unwrap_or_default(),
            credits: credits.unwrap_or_default(),
            tags: tags.unwrap_or_default(),
            homepage,
            thumbnail,
            kind,
            base,
            load_order: load_order.unwrap_or(0),
            language,
            engine,
            convert_version,
            source_hash,
            lua: lua.unwrap_or_default(),
            start_room,
            start_pos,
            start_angle,
            start_health,
            region,
            seed,
            generator_version,
            character,
            biorand_flags,
            plan_version,
            setup,
        })
    }

    /// Render the manifest deterministically.
    ///
    /// The output is the canonical checked-in form: a `[pack]` section, one
    /// `key = value` line per set field in a fixed order, and a trailing
    /// newline. Parsing the result yields the same manifest.
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str("[pack]\n");
        write_pair(&mut out, "format", &self.format.to_string());
        write_pair(&mut out, "id", &quote(&self.id));
        if let Some(name) = &self.name {
            write_pair(&mut out, "name", &quote(name));
        }
        if let Some(version) = &self.version {
            write_pair(&mut out, "version", &quote(version));
        }
        if let Some(description) = &self.description {
            write_pair(&mut out, "description", &quote(description));
        }
        if let Some(category) = &self.category {
            write_pair(&mut out, "category", &quote(category));
        }
        for (key, values) in [
            ("authors", &self.authors),
            ("credits", &self.credits),
            ("tags", &self.tags),
        ] {
            if !values.is_empty() {
                let entries: Vec<String> = values.iter().map(|value| quote(value)).collect();
                write_pair(&mut out, key, &format!("[{}]", entries.join(", ")));
            }
        }
        if let Some(homepage) = &self.homepage {
            write_pair(&mut out, "homepage", &quote(homepage));
        }
        if let Some(thumbnail) = &self.thumbnail {
            write_pair(&mut out, "thumbnail", &quote(thumbnail));
        }
        write_pair(&mut out, "kind", &quote(self.kind.as_str()));
        if let Some(base) = &self.base {
            write_pair(&mut out, "base", &quote(base));
        }
        if self.kind == PackKind::Mod || self.load_order != 0 {
            write_pair(&mut out, "load_order", &self.load_order.to_string());
        }
        if let Some(language) = &self.language {
            write_pair(&mut out, "language", &quote(language));
        }
        if let Some(engine) = &self.engine {
            write_pair(&mut out, "engine", &quote(engine));
        }
        if let Some(version) = self.convert_version {
            write_pair(&mut out, "convert_version", &version.to_string());
        }
        if let Some(hash) = &self.source_hash {
            write_pair(&mut out, "source_hash", &quote(hash));
        }
        if !self.lua.is_empty() {
            let entries: Vec<String> = self.lua.iter().map(|path| quote(path)).collect();
            write_pair(&mut out, "lua", &format!("[{}]", entries.join(", ")));
        }
        if let Some(room) = &self.start_room {
            write_pair(&mut out, "start_room", &quote(room));
        }
        if let Some([x, y, z]) = self.start_pos {
            write_pair(&mut out, "start_pos", &quote(&format!("{x},{y},{z}")));
        }
        if let Some(angle) = self.start_angle {
            write_pair(&mut out, "start_angle", &quote(&angle.to_string()));
        }
        if let Some(health) = self.start_health {
            write_pair(&mut out, "start_health", &quote(&health.to_string()));
        }
        if let Some(region) = self.region {
            write_pair(&mut out, "region", &quote(region.symbol()));
        }
        if let Some(seed) = self.seed {
            write_pair(&mut out, "seed", &quote(&seed.to_string()));
        }
        if let Some(version) = self.generator_version {
            write_pair(&mut out, "generator_version", &quote(&version.to_string()));
        }
        if let Some(character) = self.character {
            write_pair(&mut out, "character", &quote(&character.to_string()));
        }
        if let Some(names) = &self.biorand_flags {
            let entries: Vec<String> = names.iter().map(|name| quote(name)).collect();
            write_pair(
                &mut out,
                "biorand_flags",
                &format!("[{}]", entries.join(", ")),
            );
        }
        if let Some(version) = self.plan_version {
            write_pair(&mut out, "plan_version", &quote(&version.to_string()));
        }
        let setup = &self.setup;
        if !setup.scenario_flags.is_empty() {
            let entries: Vec<String> = setup
                .scenario_flags
                .iter()
                .map(|(flag, raised)| quote(&format!("{flag}={}", u8::from(*raised))))
                .collect();
            write_pair(
                &mut out,
                "scenario_flags",
                &format!("[{}]", entries.join(", ")),
            );
        }
        if let Some(items) = &setup.start_items {
            let entries: Vec<String> = items
                .iter()
                .map(|(id, amount)| quote(&format!("{id}:{amount}")))
                .collect();
            write_pair(
                &mut out,
                "start_items",
                &format!("[{}]", entries.join(", ")),
            );
        }
        if let Some(slots) = setup.chris_slots {
            write_pair(&mut out, "chris_slots", &slots.to_string());
        }
        if let Some(slots) = setup.jill_slots {
            write_pair(&mut out, "jill_slots", &slots.to_string());
        }
        out
    }
}

/// A string field holding a number, like `start_angle`.
fn parsed_field<T>(item: Item, name: &str) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    let line = item.line;
    non_empty_string(item, name)?
        .parse()
        .map_err(|error| anyhow!("manifest.toml line {line}: bad `{name}`: {error}"))
}

/// The value kinds the subset grammar can represent.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Value {
    /// A double-quoted string.
    String(String),
    /// A signed integer.
    Integer(i64),
    /// `true` or `false`.
    Bool(bool),
    /// A bracketed list of strings.
    Array(Vec<String>),
}

/// One parsed `key = value` statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Item {
    /// Enclosing section, if any.
    section: Option<String>,
    /// The key.
    key: String,
    /// The parsed value.
    value: Value,
    /// 1-based source line.
    line: usize,
}

/// Parse the subset grammar into a flat list of statements.
pub(crate) fn parse_items(text: &str) -> Result<Vec<Item>> {
    let mut items = Vec::new();
    let mut seen = HashSet::new();
    let mut section: Option<String> = None;

    let mut lines = text.lines().enumerate();
    while let Some((index, raw)) = lines.next() {
        let line = index + 1;
        budget::check_len(
            raw.len(),
            budget::MAX_MANIFEST_LINE,
            &format!("manifest.toml line {line} length"),
        )?;
        let content = strip_comment(raw).trim();
        if content.is_empty() {
            continue;
        }

        if let Some(inner) = content.strip_prefix('[') {
            let Some(name) = inner.strip_suffix(']') else {
                bail!("manifest.toml line {line}: malformed section header");
            };
            let name = name.trim();
            if name.is_empty()
                || !name
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
            {
                bail!("manifest.toml line {line}: invalid section name `{name}`");
            }
            section = Some(name.to_string());
            continue;
        }

        let (key, value) = split_key_value(content, line)?;
        // An array may continue over the following lines up to its `]`.
        let mut joined;
        let mut value = value;
        if value.starts_with('[') && !array_closed(value) {
            joined = value.to_string();
            loop {
                let Some((_, raw)) = lines.next() else {
                    bail!("manifest.toml line {line}: unterminated array");
                };
                budget::check_len(
                    joined.len() + raw.len(),
                    budget::MAX_MANIFEST_LINE,
                    &format!("manifest.toml line {line} array length"),
                )?;
                joined.push(' ');
                joined.push_str(strip_comment(raw).trim());
                if array_closed(&joined) {
                    break;
                }
            }
            value = &joined;
        }
        if !key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
        {
            bail!("manifest.toml line {line}: invalid key `{key}`");
        }
        if !seen.insert((section.clone(), key.to_string())) {
            bail!("manifest.toml line {line}: duplicate key `{key}`");
        }
        budget::check_len(
            items.len() + 1,
            budget::MAX_MANIFEST_ITEMS,
            "manifest statement count",
        )?;
        items.push(Item {
            section: section.clone(),
            key: key.to_string(),
            value: parse_value(value, line)?,
            line,
        });
    }

    Ok(items)
}

/// Whether `text` holds a `]` outside a string.
fn array_closed(text: &str) -> bool {
    let mut in_string = false;
    let mut escaped = false;
    for ch in text.chars() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
        } else if ch == '"' {
            in_string = true;
        } else if ch == ']' {
            return true;
        }
    }
    false
}

/// Remove a `#` comment that starts outside a string.
fn strip_comment(line: &str) -> &str {
    let mut in_string = false;
    let mut escaped = false;
    for (index, ch) in line.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
        } else if ch == '"' {
            in_string = true;
        } else if ch == '#' {
            return &line[..index];
        }
    }
    line
}

/// Split `key = value` at the first unquoted `=`.
fn split_key_value(text: &str, line: usize) -> Result<(&str, &str)> {
    let mut in_string = false;
    let mut escaped = false;
    for (index, ch) in text.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
        } else if ch == '"' {
            in_string = true;
        } else if ch == '=' {
            let key = text[..index].trim();
            let value = text[index + 1..].trim();
            if key.is_empty() {
                bail!("manifest.toml line {line}: missing key before `=`");
            }
            if value.is_empty() {
                bail!("manifest.toml line {line}: missing value for `{key}`");
            }
            return Ok((key, value));
        }
    }
    bail!("manifest.toml line {line}: expected `key = value`")
}

/// Parse one value.
fn parse_value(text: &str, line: usize) -> Result<Value> {
    if text.starts_with('"') {
        return Ok(Value::String(parse_string(text, line)?));
    }
    if text.starts_with('[') {
        return Ok(Value::Array(parse_array(text, line)?));
    }
    match text {
        "true" => Ok(Value::Bool(true)),
        "false" => Ok(Value::Bool(false)),
        _ => {
            let value = text.parse::<i64>().map_err(|_| {
                anyhow!(
                    "manifest.toml line {line}: expected a string, integer, bool or string array, found `{text}`"
                )
            })?;
            Ok(Value::Integer(value))
        }
    }
}

/// Parse a standalone double-quoted string.
fn parse_string(text: &str, line: usize) -> Result<String> {
    let (value, consumed) = parse_quoted(text, line)?;
    let trailing = text[consumed..].trim();
    if !trailing.is_empty() {
        bail!("manifest.toml line {line}: unexpected trailing text `{trailing}` after string");
    }
    Ok(value)
}

/// Parse a double-quoted string at the start of `text`, returning the decoded
/// value and the number of bytes consumed.
fn parse_quoted(text: &str, line: usize) -> Result<(String, usize)> {
    let mut chars = text.char_indices().peekable();
    if !matches!(chars.peek(), Some((_, '"'))) {
        bail!("manifest.toml line {line}: expected a double-quoted string");
    }
    chars.next();
    let mut value = String::new();
    while let Some((index, ch)) = chars.next() {
        match ch {
            '"' => return Ok((value, index + 1)),
            '\\' => {
                let Some((_, escaped)) = chars.next() else {
                    bail!("manifest.toml line {line}: unterminated string");
                };
                match escaped {
                    '"' => value.push('"'),
                    '\\' => value.push('\\'),
                    'n' => value.push('\n'),
                    'r' => value.push('\r'),
                    't' => value.push('\t'),
                    other => {
                        bail!("manifest.toml line {line}: unsupported escape `\\{other}`")
                    }
                }
            }
            other => value.push(other),
        }
    }
    bail!("manifest.toml line {line}: unterminated string")
}

/// Parse a bracketed string array.
fn parse_array(text: &str, line: usize) -> Result<Vec<String>> {
    let mut rest = text
        .strip_prefix('[')
        .expect("callers only pass `[`-prefixed text")
        .trim_start();
    let mut values = Vec::new();
    loop {
        if let Some(after) = rest.strip_prefix(']') {
            let trailing = after.trim();
            if !trailing.is_empty() {
                bail!(
                    "manifest.toml line {line}: unexpected trailing text `{trailing}` after array"
                );
            }
            return Ok(values);
        }
        if rest.is_empty() {
            bail!("manifest.toml line {line}: unterminated array");
        }
        let (value, consumed) = parse_quoted(rest, line)?;
        budget::check_len(
            values.len() + 1,
            budget::MAX_MANIFEST_ITEMS,
            "manifest array length",
        )?;
        values.push(value);
        rest = rest[consumed..].trim_start();
        if let Some(after) = rest.strip_prefix(',') {
            rest = after.trim_start();
        } else if !rest.starts_with(']') {
            bail!("manifest.toml line {line}: expected `,` or `]` in array");
        }
    }
}

/// Extract a required string manifest field.
/// A string array of at most [`budget::MAX_MANIFEST_LUA`] values.
fn string_array(item: Item, key: &str) -> Result<Vec<String>> {
    let line = item.line;
    let Value::Array(values) = item.value else {
        bail!("manifest.toml line {line}: `{key}` must be a string array");
    };
    budget::check_len(
        values.len(),
        budget::MAX_MANIFEST_LUA,
        "manifest list length",
    )?;
    Ok(values)
}

fn string_value(item: Item, key: &str) -> Result<String> {
    match item.value {
        Value::String(value) => Ok(value),
        _ => bail!("manifest.toml line {}: `{key}` must be a string", item.line),
    }
}

/// Extract a required non-empty string manifest field.
fn non_empty_string(item: Item, key: &str) -> Result<String> {
    let line = item.line;
    let value = string_value(item, key)?;
    if value.is_empty() {
        bail!("manifest.toml line {line}: `{key}` must not be empty");
    }
    Ok(value)
}

/// Append one rendered `key = value` line.
fn write_pair(out: &mut String, key: &str, value: &str) {
    out.push_str(key);
    out.push_str(" = ");
    out.push_str(value);
    out.push('\n');
}

/// Quote and escape a string for rendering.
fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_manifest() -> Manifest {
        Manifest {
            format: FORMAT,
            id: "re1".to_string(),
            name: Some("Arklay base".to_string()),
            version: Some("0.1.0".to_string()),
            description: None,
            kind: PackKind::Base,
            base: None,
            load_order: 0,
            language: None,
            engine: Some("arklay 0.1.0".to_string()),
            convert_version: None,
            source_hash: None,
            lua: Vec::new(),
            start_room: None,
            start_pos: None,
            start_angle: None,
            start_health: None,
            seed: None,
            generator_version: None,
            biorand_flags: None,
            plan_version: None,
            setup: Default::default(),
            character: None,
            region: None,
            ..Manifest::base("")
        }
    }

    fn mod_manifest() -> Manifest {
        Manifest {
            format: FORMAT,
            id: "demo".to_string(),
            name: Some("Demo mod".to_string()),
            version: None,
            description: None,
            kind: PackKind::Mod,
            base: Some("re1".to_string()),
            load_order: 40,
            language: None,
            engine: None,
            convert_version: None,
            source_hash: None,
            lua: vec!["lua/demo.lua".to_string(), "lua/extra.lua".to_string()],
            start_room: None,
            start_pos: None,
            start_angle: None,
            start_health: None,
            seed: None,
            generator_version: None,
            biorand_flags: None,
            plan_version: None,
            setup: Default::default(),
            character: None,
            region: None,
            ..Manifest::base("")
        }
    }

    #[test]
    fn region_round_trips_and_is_checked() {
        let mut manifest = Manifest::base("re1");
        manifest.region = Some(Region::Jpn);
        let text = manifest.render();
        assert!(text.contains("region = \"JPN\""), "{text}");
        assert_eq!(Manifest::parse(&text).unwrap(), manifest);
        let error = Manifest::parse(&text.replace("\"JPN\"", "\"STEAM\""))
            .unwrap_err()
            .to_string();
        assert!(error.contains("`region` must be"), "{error}");
        let on_mod = Manifest {
            kind: PackKind::Mod,
            base: Some("re1".to_string()),
            ..manifest
        };
        let error = Manifest::parse(&on_mod.render()).unwrap_err().to_string();
        assert!(error.contains("`region` belongs to a base pack"), "{error}");
    }

    #[test]
    fn a_generated_packs_fields_round_trip() {
        let mut manifest = Manifest::base("biorand-000042");
        manifest.seed = Some(42);
        manifest.generator_version = Some(3);
        manifest.character = Some(1);
        let parsed = Manifest::parse(&manifest.render()).unwrap();
        assert_eq!(parsed.seed, Some(42));
        assert_eq!(parsed.generator_version, Some(3));
        assert_eq!(parsed.character, Some(1));
    }

    #[test]
    fn a_generated_pack_names_the_game_it_was_made_from() {
        let mut manifest = Manifest::base("biorand-000042");
        assert_eq!(manifest.generated_from(), None, "no seed");
        manifest.seed = Some(42);
        assert_eq!(manifest.generated_from(), Some(GENERATED_BASE));
        manifest.base = Some("re2".to_string());
        let parsed = Manifest::parse(&manifest.render()).unwrap();
        assert_eq!(parsed.generated_from(), Some("re2"));
        // A mod's `base` is the pack it layers over, not a source.
        let layer =
            Manifest::parse("format = 1\nid = \"demo\"\nkind = \"mod\"\nbase = \"re1\"\n").unwrap();
        assert_eq!(layer.generated_from(), None);
    }

    #[test]
    fn a_generated_packs_keys_are_checked() {
        let base = "format = 1\nid = \"biorand-000042\"\nkind = \"base\"\n";
        let parse = |line: &str| Manifest::parse(&format!("{base}{line}\n"));
        assert_eq!(parse("character = \"0\"").unwrap().character, Some(0));
        for (line, says) in [
            ("character = \"2\"", "bad `character`"),
            ("seed = \"x\"", "bad `seed`"),
            ("generator_version = \"-1\"", "bad `generator_version`"),
        ] {
            let error = parse(line).unwrap_err().to_string();
            assert!(error.contains(says), "{line}: {error}");
        }
        for line in ["seed = \"42\"", "character = \"1\"", "plan_version = \"3\""] {
            let text =
                format!("format = 1\nid = \"demo\"\nkind = \"mod\"\nbase = \"re1\"\n{line}\n");
            let error = Manifest::parse(&text).unwrap_err().to_string();
            assert!(error.contains("belong"), "{line}: {error}");
        }
    }

    #[test]
    fn biorand_flags_round_trip_and_name_the_raised_flags() {
        let mut manifest = Manifest::base("biorand-000042");
        manifest.seed = Some(42);
        manifest.biorand_flags = Some(vec!["DOOR_RANDOMIZER".to_string(), "LAB".to_string()]);
        let text = manifest.render();
        assert!(
            text.contains("biorand_flags = [\"DOOR_RANDOMIZER\", \"LAB\"]\n"),
            "{text}"
        );
        let parsed = Manifest::parse(&text).unwrap();
        assert_eq!(parsed, manifest);
        assert_eq!(
            parsed.system_flags(),
            [system::RANDOMIZER, system::DOOR_RANDOMIZER, system::LAB]
        );

        // An empty list is written too, and raises only the seed's flag.
        manifest.biorand_flags = Some(Vec::new());
        let text = manifest.render();
        assert!(text.contains("biorand_flags = []\n"), "{text}");
        let parsed = Manifest::parse(&text).unwrap();
        assert_eq!(parsed.biorand_flags, Some(Vec::new()));
        assert_eq!(parsed.system_flags(), [system::RANDOMIZER]);
    }

    #[test]
    fn a_pack_without_the_list_keeps_the_proof_of_concept_flags() {
        let mut manifest = Manifest::base("biorand-000007");
        manifest.seed = Some(7);
        assert_eq!(manifest.biorand_flags, None);
        assert_eq!(
            manifest.system_flags(),
            [
                system::RANDOMIZER,
                system::DOOR_RANDOMIZER,
                system::ENEMY_RANDOMIZER,
                0xF0,
                0xF1,
                0xF2
            ]
        );
        // The list raises its flags with or without a seed.
        manifest.seed = None;
        manifest.biorand_flags = Some(vec!["LAB".to_string()]);
        assert_eq!(manifest.system_flags(), [system::LAB]);
        manifest.biorand_flags = None;
        assert!(manifest.system_flags().is_empty());
    }

    #[test]
    fn biorand_flags_refuse_unknown_repeated_and_mod_declarations() {
        let base = "format = 1\nid = \"biorand-000042\"\nkind = \"base\"\nseed = \"42\"\n";
        let refused = |line: &str| {
            Manifest::parse(&format!("{base}{line}\n"))
                .unwrap_err()
                .to_string()
        };
        let unknown = refused("biorand_flags = [\"DOOR_RANDMIZER\"]");
        assert!(unknown.contains("line 5"), "{unknown}");
        assert!(unknown.contains("DOOR_RANDMIZER"), "{unknown}");
        let repeated = refused("biorand_flags = [\"LAB\", \"LAB\"]");
        assert!(repeated.contains("repeats `LAB`"), "{repeated}");
        let not_an_array = refused("biorand_flags = \"LAB\"");
        assert!(
            not_an_array.contains("must be a string array"),
            "{not_an_array}"
        );

        let mod_with_list = "format = 1\nid = \"demo\"\nkind = \"mod\"\nbase = \"re1\"\nbiorand_flags = [\"LAB\"]\n";
        let error = Manifest::parse(mod_with_list).unwrap_err().to_string();
        assert!(
            error.contains("`biorand_flags` belongs to a base pack"),
            "{error}"
        );
    }

    #[test]
    fn a_generated_games_setup_round_trips_and_is_checked() {
        let mut manifest = Manifest::base("biorand-000042");
        manifest.seed = Some(42);
        manifest.plan_version = Some(3);
        manifest.setup = GameSetup {
            scenario_flags: vec![(122, true), (123, false), (124, true)],
            start_items: Some(vec![(1, 0), (2, 15), (0x41, 1)]),
            chris_slots: Some(8),
            jill_slots: Some(6),
        };
        let text = manifest.render();
        assert!(
            text.contains("scenario_flags = [\"122=1\", \"123=0\", \"124=1\"]\n"),
            "{text}"
        );
        assert!(
            text.contains("start_items = [\"1:0\", \"2:15\", \"65:1\"]\n"),
            "{text}"
        );
        assert!(text.contains("chris_slots = 8\n"), "{text}");
        assert_eq!(Manifest::parse(&text).unwrap(), manifest);
        assert!(
            !Manifest::base("re1").render().contains("slots"),
            "an ordinary pack writes none of it"
        );

        let base = "format = 1\nid = \"biorand-000042\"\nkind = \"base\"\n";
        let refused = |line: &str| {
            Manifest::parse(&format!("{base}{line}\n"))
                .unwrap_err()
                .to_string()
        };
        for (line, says) in [
            ("scenario_flags = [\"122=2\"]", "bad `scenario_flags` entry"),
            ("scenario_flags = [\"122=1\", \"122=0\"]", "repeats 122"),
            ("start_items = [\"0:1\"]", "bad `start_items` entry"),
            ("start_items = [\"2\"]", "bad `start_items` entry"),
            ("chris_slots = 9", "must be 1 to 8"),
            ("jill_slots = 0", "must be 1 to 8"),
        ] {
            let error = refused(line);
            assert!(error.contains(says), "{line}: {error}");
            assert!(error.contains("line 4"), "{line}: {error}");
        }
        let mod_setup =
            "format = 1\nid = \"demo\"\nkind = \"mod\"\nbase = \"re1\"\nchris_slots = 8\n";
        let error = Manifest::parse(mod_setup).unwrap_err().to_string();
        assert!(error.contains("belong to a base pack"), "{error}");
    }

    #[test]
    fn the_start_placement_round_trips() {
        let mut manifest = Manifest::base("re2");
        manifest.start_pos = Some([18802, 0, -3164]);
        manifest.start_angle = Some(0x800);
        manifest.start_health = Some(200);
        let back = Manifest::parse(&manifest.render()).unwrap();
        assert_eq!(back.start_pos, Some([18802, 0, -3164]));
        assert_eq!(back.start_angle, Some(0x800));
        assert_eq!(back.start_health, Some(200));
    }

    #[test]
    fn start_room_round_trips_and_is_checked() {
        let mut manifest = Manifest::base("re2");
        manifest.start_room = Some("100".to_string());
        let text = manifest.render();
        assert!(text.contains("start_room = \"100\""));
        assert_eq!(Manifest::parse(&text).unwrap(), manifest);
        let bad = text.replace("\"100\"", "\"1000\"");
        let error = Manifest::parse(&bad).unwrap_err().to_string();
        assert!(error.contains("start_room"), "{error}");
        let on_mod = Manifest {
            kind: PackKind::Mod,
            base: Some("re2".to_string()),
            ..manifest
        };
        let error = Manifest::parse(&on_mod.render()).unwrap_err().to_string();
        assert!(error.contains("base pack"), "{error}");
    }

    #[test]
    fn parses_every_value_kind() {
        let items = parse_items(
            r#"
# a leading comment
name = "demo"   # trailing comment
count = -3
enabled = true
off = false
files = ["a.lua", "b#c.lua"]   # `#` inside a string survives
empty = []
"#,
        )
        .unwrap();

        let value = |key: &str| items.iter().find(|item| item.key == key).unwrap();
        assert_eq!(value("name").value, Value::String("demo".to_string()));
        assert_eq!(value("count").value, Value::Integer(-3));
        assert_eq!(value("enabled").value, Value::Bool(true));
        assert_eq!(value("off").value, Value::Bool(false));
        assert_eq!(
            value("files").value,
            Value::Array(vec!["a.lua".to_string(), "b#c.lua".to_string()])
        );
        assert_eq!(value("empty").value, Value::Array(Vec::new()));
        assert_eq!(value("name").line, 3);
    }

    #[test]
    fn parses_sections_and_string_escapes() {
        let items = parse_items("[pack]\nid = \"a\\\"b\"\n\n[lua]\npath = \"x\\ny\"\n").unwrap();

        assert_eq!(items[0].section.as_deref(), Some("pack"));
        assert_eq!(items[0].value, Value::String("a\"b".to_string()));
        assert_eq!(items[1].section.as_deref(), Some("lua"));
        assert_eq!(items[1].value, Value::String("x\ny".to_string()));
    }

    #[test]
    fn parse_errors_name_the_line() {
        let cases = [
            ("x = \"unterminated", "line 1"),
            ("x = \"a\" tail", "trailing text"),
            ("x = \"\\q\"", "unsupported escape"),
            ("x = [\"a\" \"b\"]", "expected `,` or `]`"),
            ("x = [\"a\",", "unterminated array"),
            ("x = 12abc", "found `12abc`"),
            ("x =", "missing value"),
            ("= 1", "missing key"),
            ("just words", "expected `key = value`"),
            ("[pack", "malformed section header"),
            ("[]", "invalid section name"),
            ("[weird!]\nx = 1", "invalid section name"),
            ("a = 1\na = 2", "duplicate key"),
            ("x! = 1", "invalid key"),
        ];
        for (text, want) in cases {
            let err = parse_items(text).unwrap_err().to_string();
            assert!(err.contains(want), "{text:?}: {err}");
            assert!(err.contains("line"), "{text:?}: {err}");
        }
    }

    #[test]
    fn parse_manifest_maps_all_fields() {
        let text = "\
[pack]
format = 1
id = \"demo\"
name = \"Demo mod\"
version = \"1.2.3\"
kind = \"mod\"
base = \"re1\"
load_order = -7
rdt = \"re1-jpn\"
scd = \"re1\"
language = \"ja-JP\"
engine = \"arklay 0.1.0\"
lua = [\"lua/a.lua\", \"lua/b.lua\"]
";
        assert_eq!(Manifest::parse(text).unwrap(), mod_manifest_with_extra());
    }

    fn mod_manifest_with_extra() -> Manifest {
        Manifest {
            format: FORMAT,
            id: "demo".to_string(),
            name: Some("Demo mod".to_string()),
            version: Some("1.2.3".to_string()),
            description: None,
            category: None,
            authors: Vec::new(),
            credits: Vec::new(),
            tags: Vec::new(),
            homepage: None,
            thumbnail: None,
            kind: PackKind::Mod,
            base: Some("re1".to_string()),
            load_order: -7,
            language: Some("ja-JP".to_string()),
            engine: Some("arklay 0.1.0".to_string()),
            convert_version: None,
            source_hash: None,
            lua: vec!["lua/a.lua".to_string(), "lua/b.lua".to_string()],
            start_room: None,
            start_pos: None,
            start_angle: None,
            start_health: None,
            seed: None,
            generator_version: None,
            biorand_flags: None,
            plan_version: None,
            setup: Default::default(),
            character: None,
            region: None,
        }
    }

    #[test]
    fn catalog_fields_round_trip_and_arrays_span_lines() {
        let text = "\
[pack]
format = 1
id = \"re1-shdp\"
name = \"Seamless HD Project\"
version = \"1.1\"
category = \"High Definition\"
authors = [
    \"MoArtis\",  # lead
    \"Kayael\",
]
credits = [\"Gemini [Classic REbirth]\", \"TeamX\"]
tags = [\"Official\"]
homepage = \"https://www.reshdp.com/re1/\"
thumbnail = \"thumbnail.webp\"
kind = \"mod\"
base = \"re1\"
";
        let manifest = Manifest::parse(text).unwrap();
        assert_eq!(manifest.category.as_deref(), Some("High Definition"));
        assert_eq!(manifest.authors, ["MoArtis", "Kayael"]);
        assert_eq!(manifest.credits, ["Gemini [Classic REbirth]", "TeamX"]);
        assert!(manifest.has_tag(TAG_OFFICIAL) && !manifest.has_tag(TAG_RANDOMIZER));
        assert_eq!(
            manifest.homepage.as_deref(),
            Some("https://www.reshdp.com/re1/")
        );
        assert_eq!(manifest.thumbnail.as_deref(), Some("thumbnail.webp"));
        assert_eq!(Manifest::parse(&manifest.render()).unwrap(), manifest);

        let open = "format = 1\nid = \"x\"\nkind = \"base\"\nauthors = [\n  \"a\",\n";
        let error = Manifest::parse(open).unwrap_err().to_string();
        assert!(error.contains("line 4: unterminated array"), "{error}");
    }

    #[test]
    fn parse_defaults_the_load_order() {
        let manifest = Manifest::parse("format = 1\nid = \"re1\"\nkind = \"base\"\n").unwrap();
        assert_eq!(manifest.language, None);
        assert_eq!(manifest.load_order, 0);
        assert_eq!(manifest.lua, Vec::<String>::new());
        assert_eq!(manifest.name, None);
        assert_eq!(manifest.version, None);
        assert_eq!(manifest.engine, None);
    }

    #[test]
    fn missing_and_unknown_fields_are_errors() {
        let cases = [
            ("id = \"x\"\nkind = \"base\"", "missing `format`"),
            ("format = 1\nkind = \"base\"", "missing `id`"),
            ("format = 1\nid = \"x\"", "missing `kind`"),
            (
                "format = 2\nid = \"x\"\nkind = \"base\"",
                "unsupported manifest format 2",
            ),
            (
                "format = 1\nid = \"x\"\nkind = \"mod\"",
                "a mod manifest must name a `base`",
            ),
            (
                "format = 1\nid = \"\"\nkind = \"base\"",
                "`id` must not be empty",
            ),
            (
                "format = 1\nid = \"x\"\nkind = \"dll\"",
                "`kind` must be \"base\" or \"mod\"",
            ),
            (
                "format = 1\nid = \"x\"\nkind = \"base\"\nunknown = 1",
                "unknown key `unknown`",
            ),
            (
                "format = 1\nid = \"x\"\nkind = \"base\"\n[other]\nx = 1",
                "unknown section [other]",
            ),
            (
                "format = 1\nid = \"x\"\nkind = \"base\"\nformat = 1",
                "duplicate key `format`",
            ),
            (
                "format = 1\nid = \"x\"\nkind = \"base\"\nlua = [\"\"]",
                "`lua` entries must not be empty",
            ),
            (
                "format = 1\nid = \"x\"\nkind = \"base\"\nload_order = 999999999999",
                "does not fit an i32",
            ),
        ];
        for (text, want) in cases {
            let err = Manifest::parse(text).unwrap_err().to_string();
            assert!(err.contains(want), "{text:?}: {err}");
        }
    }

    #[test]
    fn rejects_text_over_the_caps() {
        let long_line = format!("id = \"{}\"\n", "a".repeat(budget::MAX_MANIFEST_LINE));
        let message = budget::assert_cap_error(Manifest::parse(&long_line));
        assert!(message.contains("line 1 length"), "{message}");

        let mut many = String::new();
        for index in 0..=budget::MAX_MANIFEST_ITEMS {
            many.push_str(&format!("k{index} = 1\n"));
        }
        let message = budget::assert_cap_error(Manifest::parse(&many));
        assert!(message.contains("statement count"), "{message}");

        let entries: Vec<&str> =
            std::iter::repeat_n("\"lua/a.lua\"", budget::MAX_MANIFEST_LUA + 1).collect();
        let text = format!(
            "format = 1\nid = \"x\"\nkind = \"base\"\nlua = [{}]\n",
            entries.join(", ")
        );
        let message = budget::assert_cap_error(Manifest::parse(&text));
        assert!(message.contains("lua list length"), "{message}");
    }

    #[test]
    fn render_matches_the_canonical_form() {
        assert_eq!(
            base_manifest().render(),
            "\
[pack]
format = 1
id = \"re1\"
name = \"Arklay base\"
version = \"0.1.0\"
kind = \"base\"
engine = \"arklay 0.1.0\"
"
        );
        assert_eq!(
            mod_manifest().render(),
            "\
[pack]
format = 1
id = \"demo\"
name = \"Demo mod\"
kind = \"mod\"
base = \"re1\"
load_order = 40
lua = [\"lua/demo.lua\", \"lua/extra.lua\"]
"
        );
    }

    #[test]
    fn render_escapes_special_characters() {
        let manifest = Manifest {
            name: Some("a \"b\" \\ c\nd".to_string()),
            ..base_manifest()
        };
        let rendered = manifest.render();
        assert!(
            rendered.contains("name = \"a \\\"b\\\" \\\\ c\\nd\""),
            "{rendered}"
        );
        assert_eq!(Manifest::parse(&rendered).unwrap(), manifest);
    }

    #[test]
    fn render_round_trips_every_parsed_manifest() {
        for source in [
            "format = 1\nid = \"re1\"\nkind = \"base\"\n",
            "\
[pack]
format = 1
id = \"demo\"
name = \"Demo\"
version = \"2\"
kind = \"mod\"
base = \"re1\"
load_order = 5
lua = [\"lua/a.lua\"]
",
        ] {
            let manifest = Manifest::parse(source).unwrap();
            let rendered = manifest.render();
            assert_eq!(Manifest::parse(&rendered).unwrap(), manifest);
            assert_eq!(rendered, manifest.render(), "render must be deterministic");
        }
    }

    #[test]
    fn pack_kind_renders_its_spelling() {
        assert_eq!(PackKind::Base.as_str(), "base");
        assert_eq!(PackKind::Mod.to_string(), "mod");
    }
}
