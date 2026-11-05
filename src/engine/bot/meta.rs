//! Author-authored room metadata from `room/<id>/bot.xml`.
//!
//! The restricted bot VM sees this read-only through `bot:room_meta()`; the
//! Lua route still owns execution policy. The file is optional: a room without
//! one has no metadata. The parser refuses unknown elements and attributes, so
//! a typo fails loudly instead of silently dropping a puzzle step.
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

/// The `<bot format="...">` version this parser reads.
pub const FORMAT: u32 = 1;

/// The largest metadata file accepted, matching the room XML budget.
const MAX_BYTES: usize = 4 << 20;

/// One room's bot metadata: a format version and its puzzles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomMeta {
    pub format: u32,
    pub puzzles: Vec<PuzzleMeta>,
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
    let format: u32 = attr_or(root, "format", FORMAT)?;
    if format != FORMAT {
        bail!(
            "{}: bot XML format {format} is not supported (expected {FORMAT})",
            at(root)
        );
    }

    let mut puzzles = Vec::new();
    for child in elements(root) {
        if child.tag_name().name() != "puzzle" {
            bail!(
                "{}: unknown <bot> element <{}>",
                at(child),
                child.tag_name().name()
            );
        }
        let puzzle = parse_puzzle(child)?;
        if puzzles.iter().any(|held: &PuzzleMeta| held.id == puzzle.id) {
            bail!("{}: duplicate puzzle id `{}`", at(child), puzzle.id);
        }
        puzzles.push(puzzle);
    }
    budget::check_len(puzzles.len(), budget::MAX_RECORDS, "bot XML puzzle count")?;
    Ok(RoomMeta { format, puzzles })
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
    let bank: u8 = attr(node, "bank")?;
    if usize::from(bank) >= FLAG_BANK_COUNT {
        bail!(
            "{}: bank {bank} is out of range 0..={}",
            at(node),
            FLAG_BANK_COUNT - 1
        );
    }
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
fn elements<'a, 'input>(node: Node<'a, 'input>) -> impl Iterator<Item = Node<'a, 'input>> {
    node.children().filter(Node::is_element)
}

/// Refuse any attribute of `node` not in `known`.
fn check_attributes(node: Node<'_, '_>, known: &[&str]) -> Result<()> {
    for attribute in node.attributes() {
        if !known.contains(&attribute.name()) {
            bail!("{}: unknown attribute `{}`", at(node), attribute.name());
        }
    }
    Ok(())
}

fn attr_text<'a>(node: Node<'a, '_>, name: &str) -> Result<&'a str> {
    node.attribute(name).with_context(|| {
        format!(
            "{}: <{}> needs a `{name}` attribute",
            at(node),
            node.tag_name().name()
        )
    })
}

/// Parse a decimal or `0x` hex integer.
fn integer(text: &str) -> Option<i64> {
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

fn attr<T: TryFrom<i64>>(node: Node<'_, '_>, name: &str) -> Result<T> {
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
fn at(node: Node<'_, '_>) -> String {
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
        assert_eq!(meta.format, 1);
        assert_eq!(meta.puzzles.len(), 1);
        let puzzle = &meta.puzzles[0];
        assert_eq!(puzzle.id, "paintings");
        assert_eq!(
            puzzle.title.as_deref(),
            Some("117 large gallery painting order")
        );
        let expected = vec![
            PuzzleStep {
                slot: 3,
                bank: 4,
                bit: 0x1A,
                label: "117 painting 1/7 newborn".to_string(),
                walks: vec![[19500, 8100]],
                face: Some([19500, 7200]),
            },
            PuzzleStep {
                slot: 5,
                bank: 4,
                bit: 0x1C,
                label: "117 painting 2/7 infant".to_string(),
                walks: vec![[24000, 8100], [24000, 5300], [16000, 5300]],
                face: Some([16000, 6100]),
            },
            PuzzleStep {
                slot: 6,
                bank: 4,
                bit: 0x1D,
                label: "117 painting 3/7 lively boy".to_string(),
                walks: vec![[10500, 5300]],
                face: Some([10500, 6100]),
            },
            PuzzleStep {
                slot: 4,
                bank: 4,
                bit: 0x1B,
                label: "117 painting 4/7 young man".to_string(),
                walks: vec![[19500, 5300]],
                face: Some([19500, 6100]),
            },
            PuzzleStep {
                slot: 2,
                bank: 4,
                bit: 0x19,
                label: "117 painting 5/7 tired middle-aged man".to_string(),
                walks: vec![[24000, 5300], [24000, 8100], [16000, 8100]],
                face: Some([16000, 7200]),
            },
            PuzzleStep {
                slot: 7,
                bank: 4,
                bit: 0x1E,
                label: "117 painting 6/7 bold old man".to_string(),
                walks: vec![[24000, 8100], [24000, 5300], [7250, 5300]],
                face: Some([7250, 6100]),
            },
            PuzzleStep {
                slot: 8,
                bank: 0,
                bit: 0x03,
                label: "117 painting 7/7 people mourning".to_string(),
                walks: vec![[2900, 4100]],
                face: Some([1850, 4100]),
            },
        ];
        assert_eq!(puzzle.steps, expected);
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
