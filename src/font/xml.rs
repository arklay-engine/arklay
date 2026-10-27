//! `font/<id>.xml`: a font sheet's script and metrics.
//!
//! Every font is a pair of entries under `font/` sharing a two-digit hex id:
//! the glyph sheet (`00.bmp`, or any image a mod replaces it with) and this
//! description of it. The root carries the script the sheet's cells are
//! encoded in and the sheet-wide metrics; each `<glyph>` overrides the
//! default advance of one left-grid cell.
//!
//! ```xml
//! <font format="1" script="latin" advance="8" cellWidth="14"
//!       margin="0x30" textY="181" menuY="186">
//!   <glyph cell="0x05" advance="5" indent="3"/>
//! </font>
//! ```
//!
//! `cell` is the left grid's index: a plain byte, or `0xEA + nn` for
//! `0xF8 nn`; every other cell, and the whole right page, takes the default
//! `advance` with no indent. Integers are decimal or `0x` hex. Unknown
//! elements and attributes are refused, so a typo in a hand-edited font fails
//! loudly instead of silently drawing at the wrong width.

use anyhow::{Context, Result, bail};
use roxmltree::{Document, Node};

use super::{FontMetrics, GlyphMetric};
use crate::charset::Script;

/// The format version `<font format="...">` declares.
pub const FORMAT: u32 = 1;

/// The pack directory every font lives in.
pub const DIR: &str = "font/";

/// The checked-in description of Classic REbirth's Western sheet, font `00`.
pub const WESTERN: &str = include_str!("../../games/re1/font/00.xml");
/// The checked-in description of the Japanese sheet, font `01`.
pub const JAPANESE: &str = include_str!("../../games/re1/font/01.xml");

/// One parsed `font/<id>.xml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontDef {
    /// The byte space the sheet's cells are encoded in.
    pub script: Script,
    /// The sheet-wide metrics.
    pub metrics: FontMetrics,
    /// The left-grid cells whose metric is not the default advance.
    pub glyphs: Vec<(usize, GlyphMetric)>,
}

impl FontDef {
    /// Parse a `font/<id>.xml` document.
    pub fn parse(text: &str) -> Result<Self> {
        let doc = Document::parse(text).context("invalid font XML")?;
        let root = doc.root_element();
        if root.tag_name().name() != "font" {
            bail!("expected a <font> root, found <{}>", root.tag_name().name());
        }
        check_attributes(
            root,
            &[
                "format",
                "script",
                "advance",
                "cellWidth",
                "margin",
                "textY",
                "menuY",
            ],
        )?;
        let format: u32 = attr(root, "format")?;
        if format != FORMAT {
            bail!("unsupported font format {format} (expected {FORMAT})");
        }
        let script = match attr_text(root, "script")? {
            "latin" => Script::Latin,
            "japanese" => Script::Japanese,
            other => bail!("<font> `script` value `{other}` is not latin or japanese"),
        };
        let metrics = FontMetrics {
            glyph_w: positive(root, "advance")?,
            cell_w: positive(root, "cellWidth")?,
            left_margin: attr(root, "margin")?,
            text_y: attr(root, "textY")?,
            menu_y: attr(root, "menuY")?,
        };
        let mut glyphs = Vec::new();
        for node in root.children().filter(Node::is_element) {
            if node.tag_name().name() != "glyph" {
                bail!("unknown <font> child <{}>", node.tag_name().name());
            }
            check_attributes(node, &["cell", "advance", "indent"])?;
            let cell: usize = attr(node, "cell")?;
            if glyphs.iter().any(|&(seen, _)| seen == cell) {
                bail!("<glyph> cell {cell:#04X} is listed twice");
            }
            let indent: i32 = match node.attribute("indent") {
                Some(_) => attr(node, "indent")?,
                None => 0,
            };
            if indent < 0 {
                bail!("<glyph> cell {cell:#04X} `indent` must not be negative");
            }
            glyphs.push((
                cell,
                GlyphMetric {
                    indent,
                    advance: positive(node, "advance")?,
                },
            ));
        }
        Ok(Self {
            script,
            metrics,
            glyphs,
        })
    }

    /// Classic REbirth's Western sheet ([`WESTERN`]).
    pub fn western() -> Self {
        Self::parse(WESTERN).expect("the checked-in Western font XML parses")
    }

    /// The Japanese sheet ([`JAPANESE`]).
    pub fn japanese() -> Self {
        Self::parse(JAPANESE).expect("the checked-in Japanese font XML parses")
    }
}

/// The id a `font/<id>.<ext>` entry names: two hex digits.
pub fn entry_id(path: &str) -> Option<u8> {
    let name = path
        .get(..DIR.len())?
        .eq_ignore_ascii_case(DIR)
        .then(|| &path[DIR.len()..])?;
    let (stem, _) = name.split_once('.')?;
    if stem.len() != 2 || !stem.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u8::from_str_radix(stem, 16).ok()
}

/// The pack stem of font `id` (`font/01`), the key its sheet resolves by.
pub fn stem(id: u8) -> String {
    format!("{DIR}{id:02x}")
}

/// The pack entry of font `id`'s description.
pub fn entry(id: u8) -> String {
    format!("{}.xml", stem(id))
}

fn check_attributes(node: Node<'_, '_>, known: &[&str]) -> Result<()> {
    for attribute in node.attributes() {
        if !known.contains(&attribute.name()) {
            bail!(
                "unknown <{}> attribute `{}`",
                node.tag_name().name(),
                attribute.name()
            );
        }
    }
    Ok(())
}

fn attr_text<'a>(node: Node<'a, '_>, name: &str) -> Result<&'a str> {
    node.attribute(name)
        .with_context(|| format!("<{}> needs a `{name}` attribute", node.tag_name().name()))
}

/// Parse a decimal or `0x` hex integer attribute.
fn attr<T: TryFrom<i64>>(node: Node<'_, '_>, name: &str) -> Result<T> {
    let raw = attr_text(node, name)?;
    let text = raw.trim();
    let value = match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        Some(hex) => i64::from_str_radix(hex, 16).ok(),
        None => text.parse().ok(),
    };
    value
        .and_then(|value| T::try_from(value).ok())
        .with_context(|| {
            format!(
                "<{}> `{name}` value `{raw}` is not a valid {}",
                node.tag_name().name(),
                std::any::type_name::<T>()
            )
        })
}

fn positive(node: Node<'_, '_>, name: &str) -> Result<i32> {
    let value: i32 = attr(node, name)?;
    if value <= 0 {
        bail!("<{}> `{name}` must be positive", node.tag_name().name());
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_checked_in_fonts_parse() {
        let western = FontDef::western();
        assert_eq!(western.script, Script::Latin);
        assert_eq!(western.metrics.glyph_w, 8);
        assert_eq!(western.metrics.left_margin, 0x30);
        assert_eq!(western.glyphs.len(), 47);
        assert!(western.glyphs.contains(&(
            0x05,
            GlyphMetric {
                indent: 3,
                advance: 5
            }
        )));

        let japanese = FontDef::japanese();
        assert_eq!(japanese.script, Script::Japanese);
        assert_eq!(japanese.metrics.glyph_w, 14);
        assert_eq!(japanese.metrics.left_margin, 0x22);
        assert_eq!(japanese.glyphs.len(), 85);
    }

    #[test]
    fn typos_are_refused() {
        let base = r#"<font format="1" script="latin" advance="8" cellWidth="14" margin="0" textY="0" menuY="0">"#;
        assert!(FontDef::parse(&format!("{base}</font>")).is_ok());
        assert!(FontDef::parse(&format!("{base}<glyf cell=\"1\" advance=\"2\"/></font>")).is_err());
        assert!(FontDef::parse(&format!("{base}<glyph cell=\"1\" width=\"2\"/></font>")).is_err());
        assert!(
            FontDef::parse(&format!(
                "{base}<glyph cell=\"1\" advance=\"2\"/><glyph cell=\"0x01\" advance=\"3\"/></font>"
            ))
            .is_err()
        );
        assert!(
            FontDef::parse(&format!("{base}<glyph cell=\"1\" advance=\"0\"/></font>")).is_err()
        );
        assert!(
            FontDef::parse(&format!(
                "{base}<glyph cell=\"1\" advance=\"2\" indent=\"-1\"/></font>"
            ))
            .is_err()
        );
        assert!(FontDef::parse(&base.replace("latin", "greek")).is_err());
        assert!(FontDef::parse(&base.replace("format=\"1\"", "format=\"2\"")).is_err());
    }

    #[test]
    fn entries_name_two_hex_digits() {
        assert_eq!(entry_id("font/00.xml"), Some(0));
        assert_eq!(entry_id("font/1F.bmp"), Some(0x1F));
        assert_eq!(entry_id("FONT/1f.png"), Some(0x1F));
        assert_eq!(entry_id("font/font.tim"), None);
        assert_eq!(entry_id("font/100.xml"), None);
        assert_eq!(entry_id("ui/00.xml"), None);
        assert_eq!(entry(1), "font/01.xml");
        assert_eq!(stem(0xAB), "font/ab");
    }
}
