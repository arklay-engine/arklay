//! Author-authored room metadata from `room/<id>/bot.xml`.
//!
//! The restricted bot VM sees this read-only through `bot:room_meta()`; the
//! Lua route still owns execution policy. The file is optional: a room without
//! one has no metadata. The parser refuses unknown elements and attributes, so
//! a typo fails loudly instead of silently dropping a puzzle step.
//!
//! Format 2 adds room facts (never coordinates or walk paths) a generic
//! planner reads: `<interact>` (an AOT slot and the flag that shows it is
//! done), `<camera>` (a camera cut to enter, which starts a room scene),
//! `<use>` (an item to USE from the menu while standing in an AOT slot's
//! use-item zone), `<answer>` (the choice to make at a yes/no prompt) and
//! `<avoid>` (an AOT slot the planner should not use). `<interact>` and `<camera>` share one
//! ordered step list. A format-1 document may not use them.
//!
//! The merged pack resolves the file next to the room's other entries, so a
//! mod's `room/117/bot.xml` overlays the base pack's copy (or adds one when the
//! base has none).

use anyhow::{Context, Result, bail};
use roxmltree::{Document, Node};

use crate::budget;
use crate::game::FLAG_BANK_COUNT;
use crate::pack::Pack;
use crate::state::RoomId;

/// The newest `<bot format="...">` version this parser reads. Format 1 (the
/// default when the attribute is absent) has only `<puzzle>`; format 2 adds
/// the fact elements.
pub const FORMAT: u32 = 2;

/// The format a document without a `format` attribute is read as.
const DEFAULT_FORMAT: u32 = 1;

/// The largest metadata file accepted, matching the room XML budget.
const MAX_BYTES: usize = 4 << 20;

/// One room's bot metadata: a format version, its puzzles and its facts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RoomMeta {
    pub format: u32,
    pub puzzles: Vec<PuzzleMeta>,
    pub interacts: Vec<Interact>,
    pub uses: Vec<Use>,
    pub answers: Vec<Answer>,
    pub avoids: Vec<Avoid>,
}

/// One step of the room's ordered sequence: `<interact>` (press the AOT
/// slot; its flag set means done) or `<camera>` (enter the camera cut; done
/// once the optional flag is set, or on entering the cut without one).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Interact {
    pub target: Target,
    pub flag: Option<(u8, u8)>,
    pub label: Option<String>,
}

/// What an [`Interact`] step goes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// Press action in this AOT slot's zone.
    Slot(u8),
    /// Walk into a camera switch zone that leads to this cut.
    Camera(u8),
}

/// `<use>`: stand where the room's use-item probe in AOT `slot` (a
/// `flag_bank_set` action) arms `item`, then USE `item` from the inventory
/// menu; the flag `bank`/`bit` set means done.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Use {
    pub slot: u8,
    pub item: u8,
    pub flag: (u8, u8),
    pub label: Option<String>,
}

/// `<answer>`: the choice to make at a yes/no prompt, identified by the AOT
/// `slot` that opens it, the flag `bank`/`bit`, or both.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Answer {
    /// `true` for yes, `false` for no.
    pub yes: bool,
    pub slot: Option<u8>,
    pub flag: Option<(u8, u8)>,
    pub label: Option<String>,
}

/// `<avoid>`: an AOT slot the planner should not use, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Avoid {
    pub slot: u8,
    pub reason: String,
}

/// One named, ordered puzzle in a room.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PuzzleMeta {
    pub id: String,
    pub title: Option<String>,
    pub steps: Vec<PuzzleStep>,
}

/// One step of a puzzle: the action slot and flag it proves, the points to
/// walk through, and the point to face before pressing action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PuzzleStep {
    pub slot: u8,
    pub bank: u8,
    pub bit: u8,
    pub label: String,
    pub walks: Vec<[i32; 2]>,
    pub face: Option<[i32; 2]>,
}

/// Load `room/<id>/bot.xml` from the merged pack.
///
/// An absent file is `Ok(None)`. A present file that is not UTF-8, not
/// well-formed XML, or does not match the schema is an error naming the path.
pub fn load(pack: &Pack, id: RoomId) -> Result<Option<RoomMeta>> {
    let path = format!("{}/bot.xml", id.dir());
    if !pack.contains(&path) {
        return Ok(None);
    }
    let bytes = pack
        .read(&path)
        .with_context(|| format!("cannot read {path}"))?;
    let text = std::str::from_utf8(&bytes).with_context(|| format!("{path} is not valid UTF-8"))?;
    parse(text).map(Some).with_context(|| path)
}

/// One room's `<use>` fact, with the room it belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomUse {
    /// The room id as `bot:room().id` spells it (`"10C"`).
    pub room: String,
    pub fact: Use,
}

/// Every room's `<use>` facts, by room id then document order: the
/// planner's index of where a held item has a use (`bot:use_facts()`).
/// A file that does not parse is an error, as for [`load`].
pub fn load_uses(pack: &Pack) -> Result<Vec<RoomUse>> {
    let mut rooms: Vec<String> = pack
        .paths()
        .filter_map(|path| {
            let lower = path.to_ascii_lowercase();
            let room = lower.strip_prefix("room/")?.strip_suffix("/bot.xml")?;
            (room.len() == 3 && room.bytes().all(|b| b.is_ascii_hexdigit()))
                .then(|| room.to_string())
        })
        .collect();
    rooms.sort();
    rooms.dedup();
    let mut uses = Vec::new();
    for room in rooms {
        let path = format!("room/{room}/bot.xml");
        let bytes = pack
            .read(&path)
            .with_context(|| format!("cannot read {path}"))?;
        let text =
            std::str::from_utf8(&bytes).with_context(|| format!("{path} is not valid UTF-8"))?;
        let meta = parse(text).with_context(|| path.clone())?;
        let id = room.to_ascii_uppercase();
        uses.extend(meta.uses.into_iter().map(|fact| RoomUse {
            room: id.clone(),
            fact,
        }));
    }
    Ok(uses)
}

/// Parse one `bot.xml` document.
pub fn parse(text: &str) -> Result<RoomMeta> {
    budget::check_len(text.len(), MAX_BYTES, "bot XML size")?;
    let document = Document::parse(text).map_err(|err| {
        anyhow::anyhow!(
            "bot XML is not well-formed at line {}: {err}",
            err.pos().row
        )
    })?;
    let root = document.root_element();
    if root.tag_name().name() != "bot" {
        bail!(
            "line {} <{}>: bot XML root must be <bot>",
            line(root),
            root.tag_name().name()
        );
    }
    check_attributes(root, &["format"])?;
    let format: u32 = attr_or(root, "format", DEFAULT_FORMAT)?;
    if !(1..=FORMAT).contains(&format) {
        bail!(
            "{}: bot XML format {format} is not supported (expected 1..={FORMAT})",
            at(root)
        );
    }

    let mut meta = RoomMeta {
        format,
        ..RoomMeta::default()
    };
    budget::check_len(
        elements(root).count(),
        budget::MAX_RECORDS,
        "bot XML element count",
    )?;
    for child in elements(root) {
        let name = child.tag_name().name();
        let fact = matches!(name, "interact" | "camera" | "use" | "answer" | "avoid");
        if fact && format < 2 {
            bail!("{}: <{name}> needs <bot format=\"2\">", at(child));
        }
        match name {
            "puzzle" => {
                let puzzle = parse_puzzle(child)?;
                if meta.puzzles.iter().any(|held| held.id == puzzle.id) {
                    bail!("{}: duplicate puzzle id `{}`", at(child), puzzle.id);
                }
                meta.puzzles.push(puzzle);
            }
            "interact" => meta.interacts.push(parse_interact(child)?),
            "camera" => meta.interacts.push(parse_camera(child)?),
            "use" => meta.uses.push(parse_use(child)?),
            "answer" => meta.answers.push(parse_answer(child)?),
            "avoid" => meta.avoids.push(parse_avoid(child)?),
            other => bail!("{}: unknown <bot> element <{other}>", at(child)),
        }
    }
    Ok(meta)
}

/// A fact element has no children.
pub(super) fn check_leaf(node: Node<'_, '_>) -> Result<()> {
    if let Some(child) = elements(node).next() {
        bail!(
            "{}: unknown <{}> element <{}>",
            at(child),
            node.tag_name().name(),
            child.tag_name().name()
        );
    }
    Ok(())
}

/// A `bank` attribute: below [`FLAG_BANK_COUNT`].
fn bank_attr(node: Node<'_, '_>) -> Result<u8> {
    let bank: u8 = attr(node, "bank")?;
    if usize::from(bank) >= FLAG_BANK_COUNT {
        bail!(
            "{}: bank {bank} is out of range 0..={}",
            at(node),
            FLAG_BANK_COUNT - 1
        );
    }
    Ok(bank)
}

fn parse_interact(node: Node<'_, '_>) -> Result<Interact> {
    check_attributes(node, &["slot", "bank", "bit", "label"])?;
    check_leaf(node)?;
    Ok(Interact {
        target: Target::Slot(attr(node, "slot")?),
        flag: Some((bank_attr(node)?, attr(node, "bit")?)),
        label: node.attribute("label").map(str::to_string),
    })
}

fn parse_use(node: Node<'_, '_>) -> Result<Use> {
    check_attributes(node, &["slot", "item", "bank", "bit", "label"])?;
    check_leaf(node)?;
    let item: u8 = attr(node, "item")?;
    if item == 0 {
        bail!("{}: <use> `item` must not be 0 (no item)", at(node));
    }
    Ok(Use {
        slot: attr(node, "slot")?,
        item,
        flag: (bank_attr(node)?, attr(node, "bit")?),
        label: node.attribute("label").map(str::to_string),
    })
}

fn parse_camera(node: Node<'_, '_>) -> Result<Interact> {
    check_attributes(node, &["cut", "bank", "bit", "label"])?;
    check_leaf(node)?;
    let flag = match (node.attribute("bank"), node.attribute("bit")) {
        (Some(_), Some(_)) => Some((bank_attr(node)?, attr(node, "bit")?)),
        (None, None) => None,
        _ => bail!(
            "{}: <camera> needs both `bank` and `bit`, or neither",
            at(node)
        ),
    };
    Ok(Interact {
        target: Target::Camera(attr(node, "cut")?),
        flag,
        label: node.attribute("label").map(str::to_string),
    })
}

fn parse_answer(node: Node<'_, '_>) -> Result<Answer> {
    check_attributes(node, &["choice", "slot", "bank", "bit", "label"])?;
    check_leaf(node)?;
    let yes = match attr_text(node, "choice")? {
        "yes" => true,
        "no" => false,
        other => bail!(
            "{}: <answer> `choice` value `{other}` is not `yes` or `no`",
            at(node)
        ),
    };
    let slot = match node.attribute("slot") {
        Some(_) => Some(attr(node, "slot")?),
        None => None,
    };
    let flag = match (node.attribute("bank"), node.attribute("bit")) {
        (Some(_), Some(_)) => Some((bank_attr(node)?, attr(node, "bit")?)),
        (None, None) => None,
        _ => bail!(
            "{}: <answer> needs both `bank` and `bit`, or neither",
            at(node)
        ),
    };
    if slot.is_none() && flag.is_none() {
        bail!(
            "{}: <answer> needs a `slot` or a `bank`/`bit` flag",
            at(node)
        );
    }
    Ok(Answer {
        yes,
        slot,
        flag,
        label: node.attribute("label").map(str::to_string),
    })
}

fn parse_avoid(node: Node<'_, '_>) -> Result<Avoid> {
    check_attributes(node, &["slot", "reason"])?;
    check_leaf(node)?;
    let reason = attr_text(node, "reason")?.to_string();
    if reason.trim().is_empty() {
        bail!("{}: <avoid> reason must not be empty", at(node));
    }
    Ok(Avoid {
        slot: attr(node, "slot")?,
        reason,
    })
}

fn parse_puzzle(node: Node<'_, '_>) -> Result<PuzzleMeta> {
    check_attributes(node, &["id", "title"])?;
    let id = attr_text(node, "id")?.to_string();
    if id.trim().is_empty() {
        bail!("{}: puzzle id must not be empty", at(node));
    }
    let title = node.attribute("title").map(str::to_string);
    let count = elements(node).count();
    budget::check_len(count, budget::MAX_RECORDS, "bot XML step count")?;
    let mut steps = Vec::with_capacity(count);
    for child in elements(node) {
        if child.tag_name().name() != "step" {
            bail!(
                "{}: unknown <puzzle> element <{}>",
                at(child),
                child.tag_name().name()
            );
        }
        steps.push(parse_step(child)?);
    }
    Ok(PuzzleMeta { id, title, steps })
}

fn parse_step(node: Node<'_, '_>) -> Result<PuzzleStep> {
    check_attributes(node, &["slot", "bank", "bit", "label"])?;
    let slot: u8 = attr(node, "slot")?;
    let bank = bank_attr(node)?;
    let bit: u8 = attr(node, "bit")?;
    let label = attr_text(node, "label")?.to_string();
    let count = elements(node).count();
    budget::check_len(count, budget::MAX_RECORDS, "bot XML action count")?;
    let mut walks = Vec::new();
    let mut face = None;
    for child in elements(node) {
        match child.tag_name().name() {
            "walk" => {
                check_attributes(child, &["x", "z"])?;
                walks.push([attr(child, "x")?, attr(child, "z")?]);
            }
            "face" => {
                if face.is_some() {
                    bail!("{}: step has more than one <face>", at(child));
                }
                check_attributes(child, &["x", "z"])?;
                face = Some([attr(child, "x")?, attr(child, "z")?]);
            }
            other => bail!("{}: unknown <step> element <{other}>", at(child)),
        }
    }
    Ok(PuzzleStep {
        slot,
        bank,
        bit,
        label,
        walks,
        face,
    })
}

/// The element children of `node`.
pub(super) fn elements<'a, 'input>(
    node: Node<'a, 'input>,
) -> impl Iterator<Item = Node<'a, 'input>> {
    node.children().filter(Node::is_element)
}

/// Refuse any attribute of `node` not in `known`.
pub(super) fn check_attributes(node: Node<'_, '_>, known: &[&str]) -> Result<()> {
    for attribute in node.attributes() {
        if !known.contains(&attribute.name()) {
            bail!("{}: unknown attribute `{}`", at(node), attribute.name());
        }
    }
    Ok(())
}

pub(super) fn attr_text<'a>(node: Node<'a, '_>, name: &str) -> Result<&'a str> {
    node.attribute(name).with_context(|| {
        format!(
            "{}: <{}> needs a `{name}` attribute",
            at(node),
            node.tag_name().name()
        )
    })
}

/// Parse a decimal or `0x` hex integer.
pub(super) fn integer(text: &str) -> Option<i64> {
    let text = text.trim();
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let value = match digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        Some(hex) => i64::from_str_radix(hex, 16).ok()?,
        None => digits.parse().ok()?,
    };
    Some(if negative { -value } else { value })
}

pub(super) fn attr<T: TryFrom<i64>>(node: Node<'_, '_>, name: &str) -> Result<T> {
    let raw = attr_text(node, name)?;
    integer(raw)
        .and_then(|value| T::try_from(value).ok())
        .with_context(|| {
            format!(
                "{}: <{}> `{name}` value `{raw}` is not a valid {}",
                at(node),
                node.tag_name().name(),
                std::any::type_name::<T>()
            )
        })
}

fn attr_or<T: TryFrom<i64>>(node: Node<'_, '_>, name: &str, default: T) -> Result<T> {
    match node.attribute(name) {
        Some(_) => attr(node, name),
        None => Ok(default),
    }
}

/// The source line of `node`, for parse errors.
fn line(node: Node<'_, '_>) -> u32 {
    node.document().text_pos_at(node.range().start).row
}

/// The source line and element name of `node`, for parse errors.
pub(super) fn at(node: Node<'_, '_>) -> String {
    format!("line {} <{}>", line(node), node.tag_name().name())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("arklay-bot-meta-{}-{label}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn room_117() -> RoomId {
        RoomId {
            stage: 1,
            room: 0x17,
            player_flag: 1,
            mansion2: false,
        }
    }

    fn checked_in() -> RoomMeta {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mods/bot/room/117/bot.xml");
        let text = fs::read_to_string(&path).unwrap();
        parse(&text).unwrap()
    }

    #[test]
    fn checked_in_117_metadata_has_the_seven_paintings() {
        let meta = checked_in();
        assert_eq!(meta.format, 2);
        assert!(meta.puzzles.is_empty(), "format 2 has no walk steps");
        let interact = |slot, bank, bit, label: &str| Interact {
            target: Target::Slot(slot),
            flag: Some((bank, bit)),
            label: Some(label.to_string()),
        };
        let expected = vec![
            interact(3, 4, 0x1A, "painting 1/7 newborn"),
            interact(5, 4, 0x1C, "painting 2/7 infant"),
            interact(6, 4, 0x1D, "painting 3/7 lively boy"),
            interact(4, 4, 0x1B, "painting 4/7 young man"),
            interact(2, 4, 0x19, "painting 5/7 tired middle-aged man"),
            interact(7, 4, 0x1E, "painting 6/7 bold old man"),
            interact(8, 0, 0x03, "painting 7/7 people mourning"),
        ];
        assert_eq!(meta.interacts, expected);
    }

    #[test]
    fn unknown_elements_and_attributes_are_rejected() {
        let unknown_step = r#"<bot format="1">
            <puzzle id="paintings"><step slot="1" bank="0" bit="1" label="x">
                <jump x="1" z="2"/>
            </step></puzzle>
        </bot>"#;
        let error = parse(unknown_step).unwrap_err().to_string();
        assert!(error.contains("unknown <step> element <jump>"), "{error}");
        assert!(error.contains("line 3"), "{error}");

        let unknown_root = r#"<bot format="1"><record/></bot>"#;
        let error = parse(unknown_root).unwrap_err().to_string();
        assert!(error.contains("unknown <bot> element <record>"), "{error}");

        let unknown_attribute = r#"<bot format="1" version="1"/>"#;
        let error = parse(unknown_attribute).unwrap_err().to_string();
        assert!(error.contains("unknown attribute `version`"), "{error}");
    }

    #[test]
    fn step_ranges_are_validated() {
        let bad_bank = r#"<bot format="1">
            <puzzle id="paintings"><step slot="1" bank="10" bit="1" label="x"/></puzzle>
        </bot>"#;
        let error = parse(bad_bank).unwrap_err().to_string();
        assert!(error.contains("bank 10 is out of range"), "{error}");

        let bad_bit = r#"<bot format="1">
            <puzzle id="paintings"><step slot="1" bank="0" bit="0x100" label="x"/></puzzle>
        </bot>"#;
        let error = parse(bad_bit).unwrap_err().to_string();
        assert!(error.contains("is not a valid u8"), "{error}");
    }

    #[test]
    fn format_2_reads_the_fact_elements() {
        let text = r#"<bot format="2">
            <puzzle id="p"><step slot="1" bank="0" bit="1" label="x"/></puzzle>
            <interact slot="4" bank="1" bit="0x22" label="push the statue"/>
            <interact slot="5" bank="0" bit="3"/>
            <camera cut="3" label="north hall"/>
            <camera cut="2" bank="1" bit="0x05"/>
            <answer choice="no" slot="6" label="do not drink"/>
            <answer choice="yes" bank="4" bit="0x10"/>
            <avoid slot="7" reason="the plant stuns the player"/>
        </bot>"#;
        let meta = parse(text).unwrap();
        assert_eq!(meta.format, 2);
        assert_eq!(meta.puzzles.len(), 1);
        assert_eq!(
            meta.interacts,
            vec![
                Interact {
                    target: Target::Slot(4),
                    flag: Some((1, 0x22)),
                    label: Some("push the statue".to_string()),
                },
                Interact {
                    target: Target::Slot(5),
                    flag: Some((0, 3)),
                    label: None,
                },
                Interact {
                    target: Target::Camera(3),
                    flag: None,
                    label: Some("north hall".to_string()),
                },
                Interact {
                    target: Target::Camera(2),
                    flag: Some((1, 0x05)),
                    label: None,
                },
            ]
        );
        assert_eq!(
            meta.answers,
            vec![
                Answer {
                    yes: false,
                    slot: Some(6),
                    flag: None,
                    label: Some("do not drink".to_string()),
                },
                Answer {
                    yes: true,
                    slot: None,
                    flag: Some((4, 0x10)),
                    label: None,
                },
            ]
        );
        assert_eq!(
            meta.avoids,
            vec![Avoid {
                slot: 7,
                reason: "the plant stuns the player".to_string(),
            }]
        );
    }

    #[test]
    fn format_2_reads_use_facts_in_order() {
        let text = r#"<bot format="2">
            <interact slot="4" bank="1" bit="0x22"/>
            <use slot="2" item="0x26" bank="1" bit="0xA9" label="herbicide at the pump"/>
            <use slot="11" item="32" bank="0" bit="0x6A"/>
        </bot>"#;
        let meta = parse(text).unwrap();
        assert_eq!(meta.interacts.len(), 1, "a use is not an interact step");
        assert_eq!(
            meta.uses,
            vec![
                Use {
                    slot: 2,
                    item: 0x26,
                    flag: (1, 0xA9),
                    label: Some("herbicide at the pump".to_string()),
                },
                Use {
                    slot: 11,
                    item: 0x20,
                    flag: (0, 0x6A),
                    label: None,
                },
            ]
        );
    }

    #[test]
    fn use_facts_are_validated() {
        let error = |text: &str| parse(text).unwrap_err().to_string();
        let v1 = r#"<bot format="1"><use slot="2" item="0x26" bank="1" bit="1"/></bot>"#;
        assert!(error(v1).contains("<use> needs <bot format=\"2\">"));
        let no_item = r#"<bot format="2"><use slot="2" bank="1" bit="1"/></bot>"#;
        assert!(error(no_item).contains("<use> needs a `item` attribute"));
        let zero_item = r#"<bot format="2"><use slot="2" item="0" bank="1" bit="1"/></bot>"#;
        assert!(error(zero_item).contains("`item` must not be 0"));
        let big_item = r#"<bot format="2"><use slot="2" item="0x100" bank="1" bit="1"/></bot>"#;
        assert!(error(big_item).contains("`item` value `0x100` is not a valid u8"));
        let no_flag = r#"<bot format="2"><use slot="2" item="0x26"/></bot>"#;
        assert!(error(no_flag).contains("<use> needs a `bank` attribute"));
        let half_flag = r#"<bot format="2"><use slot="2" item="0x26" bank="1"/></bot>"#;
        assert!(error(half_flag).contains("<use> needs a `bit` attribute"));
        let bad_bank = r#"<bot format="2"><use slot="2" item="0x26" bank="10" bit="1"/></bot>"#;
        assert!(error(bad_bank).contains("bank 10 is out of range"));
        let unknown = r#"<bot format="2"><use slot="2" item="0x26" bank="1" bit="1" x="1"/></bot>"#;
        assert!(error(unknown).contains("unknown attribute `x`"));
        let child = r#"<bot format="2"><use slot="2" item="0x26" bank="1" bit="1"><walk x="1" z="2"/></use></bot>"#;
        assert!(error(child).contains("unknown <use> element <walk>"));
    }

    #[test]
    fn checked_in_use_facts_parse() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mods/bot/room");
        let mut seen = 0;
        for entry in fs::read_dir(&root).unwrap() {
            let path = entry.unwrap().path().join("bot.xml");
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let meta = parse(&text).unwrap_or_else(|err| panic!("{}: {err:#}", path.display()));
            seen += meta.uses.len();
        }
        assert!(seen > 0, "no checked-in <use> facts were found");
    }

    #[test]
    fn the_use_index_lists_every_rooms_use_facts_by_room() {
        let pack = Pack::open(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mods/bot")).unwrap();
        let uses = load_uses(&pack).unwrap();
        let rows: Vec<(&str, u8, u8)> = uses
            .iter()
            .map(|entry| (entry.room.as_str(), entry.fact.slot, entry.fact.item))
            .collect();
        // Sorted by room id, document order inside a room.
        assert_eq!(
            rows,
            vec![
                ("105", 11, 0x20),
                ("10C", 2, 0x26),
                ("10F", 6, 0x1F),
                ("11A", 5, 0x2D),
                ("11A", 3, 0x2E),
                ("11A", 4, 0x2C),
                ("11A", 6, 0x29),
            ]
        );
        assert_eq!(uses[1].fact.flag, (1, 0xA9));
    }

    #[test]
    fn format_2_facts_are_validated() {
        let error = |text: &str| parse(text).unwrap_err().to_string();

        let v1 = r#"<bot format="1"><avoid slot="1" reason="x"/></bot>"#;
        assert!(error(v1).contains("<avoid> needs <bot format=\"2\">"));
        let v1_default = r#"<bot><interact slot="1" bank="0" bit="1"/></bot>"#;
        assert!(error(v1_default).contains("needs <bot format=\"2\">"));
        assert!(error(r#"<bot format="3"/>"#).contains("format 3 is not supported"));

        let unknown = r#"<bot format="2"><prompt slot="1"/></bot>"#;
        assert!(error(unknown).contains("unknown <bot> element <prompt>"));
        let child =
            r#"<bot format="2"><avoid slot="1" reason="x"><walk x="1" z="2"/></avoid></bot>"#;
        assert!(error(child).contains("unknown <avoid> element <walk>"));

        let bad_attribute = r#"<bot format="2"><interact slot="1" bank="0" bit="1" x="5"/></bot>"#;
        assert!(error(bad_attribute).contains("unknown attribute `x`"));
        let bad_choice = r#"<bot format="2"><answer choice="maybe" slot="1"/></bot>"#;
        assert!(error(bad_choice).contains("is not `yes` or `no`"));
        let no_target = r#"<bot format="2"><answer choice="yes"/></bot>"#;
        assert!(error(no_target).contains("needs a `slot` or a `bank`/`bit` flag"));
        let half_flag = r#"<bot format="2"><answer choice="yes" bank="1"/></bot>"#;
        assert!(error(half_flag).contains("both `bank` and `bit`"));
        let no_reason = r#"<bot format="2"><avoid slot="1"/></bot>"#;
        assert!(error(no_reason).contains("needs a `reason` attribute"));
        let blank_reason = r#"<bot format="2"><avoid slot="1" reason=" "/></bot>"#;
        assert!(error(blank_reason).contains("reason must not be empty"));

        let bad_bit = r#"<bot format="2"><interact slot="1" bank="0" bit="256"/></bot>"#;
        assert!(error(bad_bit).contains("`bit` value `256` is not a valid u8"));
        let bad_bank = r#"<bot format="2"><interact slot="1" bank="10" bit="1"/></bot>"#;
        assert!(error(bad_bank).contains("bank 10 is out of range"));
        let bad_answer_bank = r#"<bot format="2"><answer choice="no" bank="12" bit="1"/></bot>"#;
        assert!(error(bad_answer_bank).contains("bank 12 is out of range"));
        let bad_slot = r#"<bot format="2"><avoid slot="-1" reason="x"/></bot>"#;
        assert!(error(bad_slot).contains("`slot` value `-1` is not a valid u8"));
    }

    #[test]
    fn load_reads_the_room_file_and_ignores_an_absent_one() {
        let temp = TempDir::new("load");
        let room = temp.path.join("room/117");
        fs::create_dir_all(&room).unwrap();
        fs::write(
            room.join("bot.xml"),
            br#"<bot format="1">
                <puzzle id="paintings"><step slot="8" bank="0" bit="0x03" label="last">
                    <walk x="2900" z="4100"/><face x="1850" z="4100"/>
                </step></puzzle>
            </bot>"#,
        )
        .unwrap();

        let pack = Pack::open(&temp.path).unwrap();
        let meta = load(&pack, room_117()).unwrap().expect("the room metadata");
        assert_eq!(meta.puzzles[0].steps[0].slot, 8);
        assert_eq!(meta.puzzles[0].steps[0].walks, vec![[2900, 4100]]);
        assert_eq!(meta.puzzles[0].steps[0].face, Some([1850, 4100]));

        // A directory pack reads lazily, so a rewrite is visible to the next
        // load without reopening the pack (the --watch path).
        fs::write(
            room.join("bot.xml"),
            br#"<bot format="1">
                <puzzle id="paintings"><step slot="1" bank="0" bit="0x02" label="changed">
                    <face x="1" z="2"/>
                </step></puzzle>
            </bot>"#,
        )
        .unwrap();
        let changed = load(&pack, room_117()).unwrap().unwrap();
        assert_eq!(changed.puzzles[0].steps[0].label, "changed");
        assert_eq!(changed.puzzles[0].steps[0].walks, Vec::<[i32; 2]>::new());
        assert_eq!(changed.puzzles[0].steps[0].face, Some([1, 2]));

        let absent = RoomId {
            stage: 1,
            room: 0x18,
            player_flag: 1,
            mansion2: false,
        };
        assert_eq!(load(&pack, absent).unwrap(), None);
    }
}
