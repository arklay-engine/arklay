//! Item definitions: each item's `item/<id>/item.xml`.
//!
//! An item's folder holds everything about it, so the folder can be copied to
//! another id or pack whole ([`crate::item_art`] covers its art). The XML
//! carries the item's behaviour and its strings:
//!
//! ```xml
//! <item kind="heal" stack="1">
//!     <effects>
//!         <effect kind="heal" amount="third" />
//!         <effect kind="cure_poison" />
//!     </effects>
//!     <strings>
//!         <language lang="en-US">
//!             <string key="name">Mixed Herbs</string>
//!             <string key="description">It's a mixture of green\nand blue herbs.</string>
//!         </language>
//!     </strings>
//! </item>
//! ```
//!
//! - `kind` is the item's use category: `weapon`, `ammo`, `bottle`,
//!   `chemical`, `special`, `key`, `book`, `heal`, `file` or `other`. It
//!   picks what USE does and which refusal message a failed use shows.
//! - `stack` is the most the item holds: a weapon's loaded rounds or an ammo
//!   box's count (0, the default, for an item whose quantity is not shown).
//! - `merge="true"` merges a picked-up item into a stack the player already
//!   carries.
//! - `<effect kind="heal" amount="third|two_thirds|full">` restores health;
//!   `cure_poison` clears the `0x02` poison and `cure_serious_poison` the
//!   `0x20` one.
//! - `<string key="name|name_unchecked|description">` are the language-file
//!   `ITEM_<id>_NAME`, `_NAME_UNCHECKED` and `_DESCRIPTION` entries, with the
//!   same value syntax (`\n`, `{color 1}`, ...). A `language/<tag>.txt` that
//!   declares the key itself still wins.
//!
//! The folder name is the item id; the XML does not repeat it. A mod replaces
//! an item by shipping its whole `item.xml`. An id with no `item.xml` is an
//! [`ItemKind::Other`] with no stack and no strings. The checked-in
//! definitions are embedded in the binary, and a pack's own files replace
//! them per item.

use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock};

use anyhow::{Context, Result, bail};
use roxmltree::{Document, Node};

use crate::budget;
use crate::item_art::PREFIX;
use crate::language::Language;
use crate::pack::Pack;

mod sources {
    include!(concat!(env!("OUT_DIR"), "/item_files.rs"));
}

/// The checked-in item definitions as `(pack entry, text)` pairs, generated
/// by `build.rs`; `convert-game` packs exactly this set.
pub use sources::ITEM_FILES;

/// File name of an item's definition inside its folder.
pub const FILE_NAME: &str = "item.xml";

/// The pack entry of `item`'s definition: `item/0b/item.xml`.
pub fn entry(item: u8) -> String {
    format!("{}{FILE_NAME}", crate::item_art::folder(item))
}

/// The item id an `item/<id>/item.xml` entry names, matched
/// case-insensitively like every pack lookup.
pub fn entry_item(entry: &str) -> Option<u8> {
    let entry = entry.to_ascii_lowercase();
    let rest = entry.strip_prefix(PREFIX)?;
    let (id, file) = rest.split_once('/')?;
    if file != FILE_NAME || id.len() != 2 {
        return None;
    }
    u8::from_str_radix(id, 16).ok()
}

/// An item's use category, in the original's dispatch-table order.
///
/// [`ItemKind::dispatch_index`] is the slot the menu's USE refusal message is
/// picked by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ItemKind {
    /// Weapons: cannot be used.
    Weapon,
    /// Ammunition: cannot be used.
    Ammo,
    /// The empty bottle: cannot be used.
    Bottle,
    /// Chemicals and fluids: usable when their per-item flag is set.
    Chemical,
    /// Quest items: usable when their per-item flag is set.
    Special,
    /// Keys: usable when their per-item flag is set.
    Key,
    /// Books: only the red book is usable.
    Book,
    /// Healing items: apply their [`HealEffect`].
    Heal,
    /// Documents, read from the FILE tab: cannot be used.
    File,
    /// Anything else: cannot be used.
    #[default]
    Other,
}

impl ItemKind {
    const NAMES: [(&str, ItemKind); 10] = [
        ("weapon", Self::Weapon),
        ("ammo", Self::Ammo),
        ("bottle", Self::Bottle),
        ("chemical", Self::Chemical),
        ("special", Self::Special),
        ("key", Self::Key),
        ("book", Self::Book),
        ("heal", Self::Heal),
        ("file", Self::File),
        ("other", Self::Other),
    ];

    fn parse(name: &str) -> Option<Self> {
        Self::NAMES
            .iter()
            .find(|(known, _)| *known == name)
            .map(|(_, kind)| *kind)
    }

    /// The original use-category dispatch index (weapons through
    /// unusable); files and other items share the unusable slot.
    pub fn dispatch_index(self) -> u8 {
        match self {
            Self::Weapon => 0,
            Self::Ammo => 1,
            Self::Bottle => 2,
            Self::Chemical => 3,
            Self::Special => 4,
            Self::Key => 5,
            Self::Book => 6,
            Self::Heal => 7,
            Self::File | Self::Other => 9,
        }
    }
}

/// Health restored by a heal effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HealAmount {
    /// No health restored.
    #[default]
    None,
    /// One third of the maximum.
    Third,
    /// Two thirds of the maximum.
    TwoThirds,
    /// Full health.
    Full,
}

impl HealAmount {
    /// Apply the amount: `health` is raised by the third/two-thirds/full step
    /// and clamped to `max_health`; at or past the cap nothing changes.
    pub fn apply(self, health: i16, max_health: i16) -> i16 {
        if health >= max_health {
            return health;
        }
        let max = i32::from(max_health);
        let raised = match self {
            Self::None => return health,
            Self::Third => i32::from(health) + max / 3,
            Self::TwoThirds => i32::from(health) + (max * 2) / 3,
            Self::Full => max,
        };
        raised.min(max) as i16
    }
}

/// Status cured by a heal effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatusCure {
    /// No status change.
    #[default]
    None,
    /// Clears the serious (`0x20`) poison: `cure_serious_poison`.
    Poison20,
    /// Clears the `0x02` poison: `cure_poison`.
    Poison02,
}

impl StatusCure {
    /// Apply the cure to a health-status byte: only a poisoned (`0x22`)
    /// status changes.
    pub fn apply(self, status: u8) -> u8 {
        if status & 0x22 == 0 {
            return status;
        }
        match self {
            Self::None => status,
            Self::Poison20 => status & !0x20,
            Self::Poison02 => status & !0x02,
        }
    }
}

/// An item's heal effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HealEffect {
    /// Health restored.
    pub amount: HealAmount,
    /// Status cured.
    pub cure: StatusCure,
}

/// One `<string>`: `(language tag, key, value)`.
type ItemString = (String, String, String);

/// One item's definition.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ItemDef {
    /// Use category.
    pub kind: ItemKind,
    /// Most the item holds (0 when its quantity is not shown).
    pub stack: u8,
    /// Whether a pickup merges into a carried stack.
    pub merge: bool,
    /// Heal effects (none for a non-heal item).
    pub heal: HealEffect,
    /// Strings in declaration order.
    strings: Vec<ItemString>,
}

impl ItemDef {
    /// Parse an `item.xml`.
    pub fn parse(source: &str) -> Result<Self> {
        budget::check_len(source.len(), budget::MAX_TEXT_BYTES, "item.xml size")?;
        let document = Document::parse(source).context("invalid XML")?;
        let root = document.root_element();
        if root.tag_name().name() != "item" {
            bail!("the root element must be <item>");
        }
        let mut def = Self::default();
        let (mut effects, mut strings) = (false, false);
        for attribute in root.attributes() {
            let value = attribute.value();
            match attribute.name() {
                "kind" => {
                    def.kind = ItemKind::parse(value)
                        .with_context(|| format!("unknown item kind `{value}`"))?;
                }
                "stack" => {
                    def.stack = value
                        .parse()
                        .with_context(|| format!("invalid stack `{value}`"))?;
                }
                "merge" => def.merge = parse_bool(value)?,
                other => bail!("unknown <item> attribute `{other}`"),
            }
        }
        for child in elements(root) {
            let seen = match child.tag_name().name() {
                "effects" => {
                    def.heal = parse_effects(child)?;
                    &mut effects
                }
                "strings" => {
                    def.strings = parse_strings(child)?;
                    &mut strings
                }
                other => bail!("unknown <item> element <{other}>"),
            };
            if std::mem::replace(seen, true) {
                bail!("<item> has more than one <{}>", child.tag_name().name());
            }
        }
        Ok(def)
    }

    /// The `(language tag, key, value)` strings, `key` being `name`,
    /// `name_unchecked` or `description`.
    pub fn strings(&self) -> impl Iterator<Item = (&str, &str, &str)> {
        self.strings
            .iter()
            .map(|(tag, key, value)| (tag.as_str(), key.as_str(), value.as_str()))
    }

    /// Whether a heal item with no effect this use is still used up outside
    /// authentic mode: one that restores health while the player is at full
    /// health, and a `cure_poison`-only item while the player is not
    /// poisoned.
    pub fn accepts_ineffective_use(&self, full_health: bool, poisoned: bool) -> bool {
        if self.heal.amount != HealAmount::None {
            return full_health;
        }
        self.heal.cure == StatusCure::Poison02 && !poisoned
    }
}

fn parse_bool(value: &str) -> Result<bool> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        other => bail!("expected `true` or `false`, found `{other}`"),
    }
}

fn elements<'a, 'input>(node: Node<'a, 'input>) -> impl Iterator<Item = Node<'a, 'input>> {
    node.children().filter(Node::is_element)
}

fn no_attributes(node: Node) -> Result<()> {
    if let Some(attribute) = node.attributes().next() {
        bail!(
            "unknown <{}> attribute `{}`",
            node.tag_name().name(),
            attribute.name()
        );
    }
    Ok(())
}

fn parse_effects(node: Node) -> Result<HealEffect> {
    no_attributes(node)?;
    let mut heal = HealEffect::default();
    for effect in elements(node) {
        if effect.tag_name().name() != "effect" {
            bail!("unknown <effects> element <{}>", effect.tag_name().name());
        }
        let kind = effect
            .attribute("kind")
            .context("an <effect> needs a kind")?;
        let mut amount = None;
        for attribute in effect.attributes() {
            match attribute.name() {
                "kind" => {}
                "amount" if kind == "heal" => amount = Some(attribute.value()),
                other => bail!("unknown `{kind}` effect attribute `{other}`"),
            }
        }
        match kind {
            "heal" => {
                if heal.amount != HealAmount::None {
                    bail!("an item can only have one heal effect");
                }
                heal.amount = match amount.context("a heal effect needs an amount")? {
                    "third" => HealAmount::Third,
                    "two_thirds" => HealAmount::TwoThirds,
                    "full" => HealAmount::Full,
                    other => bail!("unknown heal amount `{other}`"),
                };
            }
            "cure_poison" | "cure_serious_poison" => {
                if heal.cure != StatusCure::None {
                    bail!("an item can only cure one poison");
                }
                heal.cure = if kind == "cure_poison" {
                    StatusCure::Poison02
                } else {
                    StatusCure::Poison20
                };
            }
            other => bail!("unknown effect kind `{other}`"),
        }
    }
    Ok(heal)
}

fn parse_strings(node: Node) -> Result<Vec<ItemString>> {
    no_attributes(node)?;
    let mut strings = Vec::new();
    let mut tags = Vec::new();
    for language in elements(node) {
        if language.tag_name().name() != "language" {
            bail!("unknown <strings> element <{}>", language.tag_name().name());
        }
        let mut tag = None;
        for attribute in language.attributes() {
            match attribute.name() {
                "lang" => tag = Some(attribute.value()),
                other => bail!("unknown <language> attribute `{other}`"),
            }
        }
        let tag = tag.context("a <language> needs a lang")?;
        if !crate::language::valid_tag(tag) {
            bail!("invalid language tag `{tag}`");
        }
        if tags
            .iter()
            .any(|seen: &&str| seen.eq_ignore_ascii_case(tag))
        {
            bail!("more than one <language lang=\"{tag}\">");
        }
        tags.push(tag);
        let mut values = Vec::new();
        for string in elements(language) {
            if string.tag_name().name() != "string" {
                bail!("unknown <language> element <{}>", string.tag_name().name());
            }
            let mut key = None;
            for attribute in string.attributes() {
                match attribute.name() {
                    "key" => key = Some(attribute.value()),
                    other => bail!("unknown <string> attribute `{other}`"),
                }
            }
            let key = key.context("a <string> needs a key")?;
            let suffix =
                language_key_suffix(key).with_context(|| format!("unknown string key `{key}`"))?;
            if string.children().any(|child| !child.is_text()) {
                bail!("<string key=\"{key}\"> must hold only text");
            }
            let value = string.text().unwrap_or_default();
            values.push((format!("ITEM_00_{suffix}"), value));
            strings.push((tag.to_string(), key.to_string(), value.to_string()));
        }
        // The values use the language files' syntax; check it here so a bad
        // string fails this item rather than the whole language's layer.
        Language::from_values(values, &format!("<language lang=\"{tag}\">"))?;
    }
    Ok(strings)
}

/// The language-file key suffix of an `item.xml` string key.
fn language_key_suffix(key: &str) -> Option<&'static str> {
    match key {
        "name" => Some("NAME"),
        "name_unchecked" => Some("NAME_UNCHECKED"),
        "description" => Some("DESCRIPTION"),
        _ => None,
    }
}

/// Every item's definition, by id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ItemCatalog {
    items: BTreeMap<u8, ItemDef>,
}

/// The definition an id without an `item.xml` reads as.
static UNDEFINED: ItemDef = ItemDef {
    kind: ItemKind::Other,
    stack: 0,
    merge: false,
    heal: HealEffect {
        amount: HealAmount::None,
        cure: StatusCure::None,
    },
    strings: Vec::new(),
};

/// The catalog of the checked-in definitions alone.
static EMBEDDED: LazyLock<Arc<ItemCatalog>> =
    LazyLock::new(|| Arc::new(ItemCatalog::assemble(std::iter::empty(), true)));

impl ItemCatalog {
    /// The checked-in definitions, shared.
    pub fn embedded() -> Arc<Self> {
        EMBEDDED.clone()
    }

    /// The pack's definitions: every `item/<id>/item.xml` (the top mod layer
    /// for each), over the checked-in ones. A file that does not parse is
    /// reported and leaves that item undefined, so a broken mod item is
    /// visible instead of silently reverting to the shipped one.
    pub fn load(pack: &Pack) -> Self {
        Self::assemble(read_pack(pack).into_iter(), true)
    }

    fn assemble(sources: impl Iterator<Item = (String, String)>, warn: bool) -> Self {
        let mut items = BTreeMap::new();
        for (path, source) in ITEM_FILES
            .iter()
            .map(|(entry, source)| (entry.to_string(), source.to_string()))
            .chain(sources)
        {
            let Some(item) = entry_item(&path) else {
                continue;
            };
            match ItemDef::parse(&source) {
                Ok(def) => {
                    items.insert(item, def);
                }
                Err(error) => {
                    if warn {
                        eprintln!("warning: invalid {path}: {error:#}");
                    }
                    items.remove(&item);
                }
            }
        }
        Self { items }
    }

    /// `item`'s definition; an undefined id reads as [`ItemKind::Other`]
    /// with no stack.
    pub fn get(&self, item: u8) -> &ItemDef {
        self.items.get(&item).unwrap_or(&UNDEFINED)
    }

    /// Whether `item` has an `item.xml`.
    pub fn contains(&self, item: u8) -> bool {
        self.items.contains_key(&item)
    }

    /// Every defined item, in id order.
    pub fn iter(&self) -> impl Iterator<Item = (u8, &ItemDef)> {
        self.items.iter().map(|(&item, def)| (item, def))
    }
}

/// The pack's `item/<id>/item.xml` entries and their text; one that is not
/// UTF-8 reads as empty, which fails to parse.
fn read_pack(pack: &Pack) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for path in pack.paths() {
        if entry_item(path).is_none() {
            continue;
        }
        let Ok(data) = pack.read(path) else {
            eprintln!("warning: failed to read {path}");
            continue;
        };
        match String::from_utf8(data.to_vec()) {
            Ok(source) => out.push((path.to_string(), source)),
            Err(_) => out.push((path.to_string(), String::new())),
        }
    }
    out
}

/// The item strings as language layers, `(tag, file)` per language: the
/// checked-in definitions overlaid by the pack's, keyed
/// `ITEM_<ID>_NAME`/`_NAME_UNCHECKED`/`_DESCRIPTION`.
///
/// [`crate::language::Catalog::load`] merges these under the language
/// files. The game state loads the same catalog and reports its broken files,
/// so this pass stays quiet.
pub fn language_layers(pack: &Pack) -> Vec<(String, Language)> {
    let catalog = ItemCatalog::assemble(read_pack(pack).into_iter(), false);
    let mut by_tag: BTreeMap<String, Vec<(String, &str)>> = BTreeMap::new();
    for (item, def) in catalog.iter() {
        for (tag, key, value) in def.strings() {
            let Some(suffix) = language_key_suffix(key) else {
                continue;
            };
            by_tag
                .entry(tag.to_ascii_lowercase())
                .or_default()
                .push((format!("ITEM_{item:02X}_{suffix}"), value));
        }
    }
    let mut layers = Vec::new();
    for (tag, values) in by_tag {
        match Language::from_values(values, &format!("item strings for {tag}")) {
            Ok(language) => layers.push((tag, language)),
            Err(error) => eprintln!("warning: {error:#}"),
        }
    }
    layers
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::PackWriter;

    fn pack(entries: &[(&str, &str)]) -> Pack {
        let mut writer = PackWriter::new();
        for (path, data) in entries {
            writer.add(path, data.as_bytes().to_vec()).unwrap();
        }
        Pack::from_bytes(writer.to_bytes().unwrap()).unwrap()
    }

    #[test]
    fn entry_names_round_trip() {
        assert_eq!(entry(0x0b), "item/0b/item.xml");
        assert_eq!(entry_item("item/0b/item.xml"), Some(0x0b));
        // Pack lookups ignore case, so a mod's `item/0B/ITEM.XML` counts.
        assert_eq!(entry_item("item/0B/item.xml"), Some(0x0b));
        assert_eq!(entry_item("ITEM/0b/ITEM.XML"), Some(0x0b));
        assert_eq!(entry_item("item/0b/model.tmd"), None);
        assert_eq!(entry_item("item/b/item.xml"), None);
        assert_eq!(entry_item("room/0b/item.xml"), None);
    }

    #[test]
    fn every_checked_in_definition_parses() {
        assert!(!ITEM_FILES.is_empty());
        for (entry, source) in ITEM_FILES {
            assert!(entry_item(entry).is_some(), "{entry}");
            ItemDef::parse(source).unwrap_or_else(|error| panic!("{entry}: {error:#}"));
        }
        let catalog = ItemCatalog::embedded();
        assert_eq!(catalog.iter().count(), ITEM_FILES.len());
    }

    #[test]
    fn parses_every_attribute_and_effect() {
        let def = ItemDef::parse(
            r#"<item kind="heal" stack="3" merge="true">
                <effects>
                    <effect kind="heal" amount="two_thirds" />
                    <effect kind="cure_serious_poison" />
                </effects>
                <strings>
                    <language lang="en-US">
                        <string key="name">Herb &amp; Co</string>
                    </language>
                </strings>
            </item>"#,
        )
        .unwrap();
        assert_eq!(def.kind, ItemKind::Heal);
        assert_eq!(def.stack, 3);
        assert!(def.merge);
        assert_eq!(
            def.heal,
            HealEffect {
                amount: HealAmount::TwoThirds,
                cure: StatusCure::Poison20,
            }
        );
        assert_eq!(
            def.strings().collect::<Vec<_>>(),
            [("en-US", "name", "Herb & Co")]
        );
    }

    #[test]
    fn refuses_unknown_names() {
        for source in [
            r#"<thing />"#,
            r#"<item kind="sword" />"#,
            r#"<item colour="red" />"#,
            r#"<item stack="300" />"#,
            r#"<item merge="yes" />"#,
            r#"<item><crafts /></item>"#,
            r#"<item><effects><effect kind="fly" /></effects></item>"#,
            r#"<item><effects><effect kind="heal" /></effects></item>"#,
            r#"<item><effects><effect kind="heal" amount="half" /></effects></item>"#,
            r#"<item><effects><effect kind="cure_poison" amount="full" /></effects></item>"#,
            r#"<item><strings><language><string key="name">A</string></language></strings></item>"#,
            r#"<item><strings><language lang="en-US"><string key="title">A</string></language></strings></item>"#,
            r#"<item><effects /><effects /></item>"#,
            r#"<item><strings /><strings /></item>"#,
            r#"<item><effects><effect kind="heal" amount="full" /><effect kind="heal" amount="third" /></effects></item>"#,
            r#"<item><strings><language lang="en US"><string key="name">A</string></language></strings></item>"#,
            r#"<item><strings><language lang="en-US" /><language lang="en-US" /></strings></item>"#,
            r#"<item><strings><language lang="en-US"><string key="name">A</string><string key="name">B</string></language></strings></item>"#,
            r#"<item><strings><language lang="en-US"><string key="name">A\q</string></language></strings></item>"#,
            r#"<item><strings><language lang="en-US"><string key="name">{colour 1}A</string></language></strings></item>"#,
            r#"<item><strings><language lang="en-US"><string key="name">A<!-- x -->B</string></language></strings></item>"#,
        ] {
            assert!(ItemDef::parse(source).is_err(), "{source}");
        }
    }

    #[test]
    fn the_shipped_definitions_keep_the_original_tables() {
        let catalog = ItemCatalog::embedded();
        assert_eq!(catalog.get(0x02).kind, ItemKind::Weapon);
        assert_eq!(catalog.get(0x02).stack, 15);
        assert!(!catalog.get(0x02).merge);
        assert!(catalog.get(0x0B).merge);
        assert!(catalog.get(0x2F).merge);
        assert_eq!(catalog.get(0x13).kind, ItemKind::Bottle);
        assert_eq!(catalog.get(0x3E).kind, ItemKind::Book);
        assert_eq!(catalog.get(0x4C).kind, ItemKind::Other);
        assert_eq!(catalog.get(0x5F).kind, ItemKind::File);
        assert_eq!(catalog.get(0x6F).kind, ItemKind::Weapon);
        assert_eq!(
            catalog.get(0x41).heal,
            HealEffect {
                amount: HealAmount::Full,
                cure: StatusCure::None,
            }
        );
        assert_eq!(catalog.get(0x42).heal.cure, StatusCure::Poison20);
        assert_eq!(catalog.get(0x43).heal, HealEffect::default());
        assert_eq!(catalog.get(0x45).heal.cure, StatusCure::Poison02);
        assert_eq!(
            catalog.get(0x4B).heal,
            HealEffect {
                amount: HealAmount::TwoThirds,
                cure: StatusCure::Poison02,
            }
        );
        // An id without a folder is an unusable, unstacked item.
        assert!(!catalog.contains(0xF0));
        assert_eq!(catalog.get(0xF0), &UNDEFINED);
    }

    #[test]
    fn a_pack_definition_replaces_the_shipped_one_whole() {
        let pack = pack(&[
            ("item/02/item.xml", r#"<item kind="ammo" stack="99" />"#),
            ("item/f0/item.xml", r#"<item kind="key" />"#),
            ("item/0b/item.xml", r#"<item kind="nonsense" />"#),
        ]);
        let catalog = ItemCatalog::load(&pack);
        assert_eq!(catalog.get(0x02).kind, ItemKind::Ammo);
        assert_eq!(catalog.get(0x02).stack, 99);
        assert_eq!(catalog.get(0x02).strings().count(), 0);
        assert_eq!(catalog.get(0xF0).kind, ItemKind::Key);
        // A broken file leaves the item undefined rather than shipped.
        assert!(!catalog.contains(0x0B));
        assert_eq!(catalog.get(0x03).kind, ItemKind::Weapon);
    }

    #[test]
    fn strings_become_language_keys_and_a_language_file_still_wins() {
        let pack = pack(&[
            (
                "item/02/item.xml",
                r#"<item kind="weapon"><strings>
                    <language lang="en-US">
                        <string key="name">Pistol</string>
                        <string key="description">A\nB</string>
                    </language>
                    <language lang="fr-FR"><string key="name">Pistolet</string></language>
                </strings></item>"#,
            ),
            ("language/en-US.txt", "ITEM_03_NAME = \"Boomstick\"\n"),
        ]);
        let text = crate::text::Text::load(&pack, Some("en-US"));
        assert_eq!(text.text("ITEM_02_NAME").as_deref(), Some("Pistol"));
        assert_eq!(text.text("ITEM_02_DESCRIPTION").as_deref(), Some("A\nB"));
        assert_eq!(
            text.text_in("fr-FR", "ITEM_02_NAME").as_deref(),
            Some("Pistolet")
        );
        assert_eq!(text.text("ITEM_03_NAME").as_deref(), Some("Boomstick"));
        // An item the pack leaves alone keeps its shipped strings.
        assert_eq!(
            text.text("ITEM_0B_NAME").as_deref(),
            Some("Hand Gun Bullets")
        );
    }

    #[test]
    fn heal_math_follows_the_thirds_and_clamps() {
        assert_eq!(HealAmount::Third.apply(10, 96), 42);
        assert_eq!(HealAmount::TwoThirds.apply(10, 96), 74);
        assert_eq!(HealAmount::Full.apply(10, 96), 96);
        assert_eq!(HealAmount::Third.apply(95, 96), 96);
        assert_eq!(HealAmount::Third.apply(96, 96), 96);
        assert_eq!(HealAmount::None.apply(50, 96), 50);
        assert_eq!(HealAmount::Full.apply(200, 96), 200);
    }

    #[test]
    fn cures_clear_only_the_matching_poison_bits() {
        assert_eq!(StatusCure::Poison20.apply(0x22), 0x02);
        assert_eq!(StatusCure::Poison20.apply(0x20), 0x00);
        assert_eq!(StatusCure::Poison02.apply(0x22), 0x20);
        assert_eq!(StatusCure::Poison02.apply(0x02), 0x00);
        assert_eq!(StatusCure::None.apply(0x22), 0x22);
        assert_eq!(StatusCure::Poison20.apply(0x10), 0x10);
    }

    #[test]
    fn ineffective_heals_follow_the_effects() {
        let catalog = ItemCatalog::embedded();
        // A health item at full health, whatever its cure.
        for item in [0x41, 0x44, 0x46, 0x47, 0x48, 0x49, 0x4A, 0x4B] {
            let def = catalog.get(item);
            assert!(def.accepts_ineffective_use(true, true), "{item:#04x}");
            assert!(!def.accepts_ineffective_use(false, false), "{item:#04x}");
        }
        // The blue herb while not poisoned.
        assert!(catalog.get(0x45).accepts_ineffective_use(false, false));
        assert!(!catalog.get(0x45).accepts_ineffective_use(true, true));
        // The serum and the red herb never.
        for item in [0x42, 0x43] {
            assert!(!catalog.get(item).accepts_ineffective_use(true, false));
        }
    }
}
