//! Room message text: an encoded message stream as text plus control
//! elements, and back.
//!
//! | element | bytes | meaning |
//! |---|---|---|
//! | text | glyphs | characters through [`crate::charset`] |
//! | `<end action="N"/>` | `0x01 N` | end; `N` is 0 (wait) or a frame count |
//! | `<br/>` | `0x02` | newline |
//! | `<page n="N"/>` | `0x03 N` | page break with settle `N` |
//! | `<speed n="N"/>` | `0x04 N` | character delay; `n="0"` opens a skip block that a second `<speed>` closes |
//! | `<color n="N"/>` | `0x05 N` | CLUT colour |
//! | `<item n="N"/>` | `0x06 N` | item-name substitution, 0 = the selected item |
//! | `<itemEnd/>` | `0x07` | return from the item name |
//! | `<yesno a="A" b="B" c="C"/>` | `0x08 A B C` | yes/no prompt and its post-action |
//! | `<glyph page="0xF8" n="N"/>` | `0xF8`..`0xFA N` | an extended glyph the script does not name |
//! | `<s/>` | `0xFB` | glyph spacing |
//! | `<byte v="0xNN"/>` | `NN` | any other byte, raw |
//!
//! [`encode`] only produces markup that [`decode`] turns back into the same
//! bytes; a stream it cannot spell that way is the caller's to keep raw.

use anyhow::{Result, bail};

use crate::charset::{self, Script};

/// One piece of a marked-up message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    /// A run of characters.
    Text(String),
    /// A control element: its name and byte-valued attributes.
    Tag(&'static str, Vec<(&'static str, u8)>),
}

/// The control elements: name, lead byte and operand attribute names.
const TAGS: &[(&str, u8, &[&str])] = &[
    ("end", 0x01, &["action"]),
    ("br", 0x02, &[]),
    ("page", 0x03, &["n"]),
    ("speed", 0x04, &["n"]),
    ("color", 0x05, &["n"]),
    ("item", 0x06, &["n"]),
    ("itemEnd", 0x07, &[]),
    ("yesno", 0x08, &["a", "b", "c"]),
    ("s", 0xFB, &[]),
];

/// Mark up an encoded message stream. `None` when the markup would not
/// decode back to exactly `bytes`.
pub fn encode(bytes: &[u8], script: Script) -> Option<Vec<Piece>> {
    let mut pieces = Vec::new();
    let mut text = String::new();
    // Where the current text run's bytes start.
    let mut run_start = 0;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(&(name, _, operands)) = TAGS.iter().find(|(_, lead, _)| *lead == byte)
            && let Some(values) = bytes.get(index + 1..index + 1 + operands.len())
        {
            flush(&mut text, &mut pieces);
            pieces.push(Piece::Tag(
                name,
                operands
                    .iter()
                    .copied()
                    .zip(values.iter().copied())
                    .collect(),
            ));
            index += 1 + operands.len();
            continue;
        }
        let unit = if (0xF8..=0xFA).contains(&byte) {
            bytes.get(index..index + 2)
        } else {
            Some(&bytes[index..=index])
        };
        let cell = unit.and_then(|unit| spell(unit, script));
        match (unit, cell) {
            (Some(unit), Some(cell)) => {
                if text.is_empty() {
                    run_start = index;
                }
                // A cell that would fuse with the run before it (a
                // two-character cell spelled apart) is written as its bytes:
                // adjacent text runs read back as one.
                let mut joined = Vec::new();
                charset::encode(&format!("{text}{cell}"), script, &mut joined);
                if joined.strip_suffix(unit) != Some(&bytes[run_start..index]) {
                    flush(&mut text, &mut pieces);
                    pieces.push(raw_unit(unit));
                } else {
                    text.push_str(cell);
                }
                index += unit.len();
            }
            (Some(unit), None) => {
                flush(&mut text, &mut pieces);
                pieces.push(raw_unit(unit));
                index += unit.len();
            }
            (None, _) => {
                flush(&mut text, &mut pieces);
                pieces.push(raw_unit(&[byte]));
                index += 1;
            }
        }
    }
    flush(&mut text, &mut pieces);
    (decode(&pieces, script).ok()? == bytes).then_some(pieces)
}

/// The text a glyph unit reads as, when it encodes back to that unit and is
/// plain text an XML document carries unchanged.
fn spell(unit: &[u8], script: Script) -> Option<&'static str> {
    let cell = match unit {
        [byte] => charset::plain_cell(*byte, script)?,
        [page, operand] => charset::extended_cell(*page, *operand, script)?,
        _ => return None,
    };
    if cell.is_empty() || cell.chars().any(|ch| ch < ' ' || ch == '\u{7F}') {
        return None;
    }
    let mut encoded = Vec::new();
    charset::encode(cell, script, &mut encoded);
    (encoded == unit).then_some(cell)
}

/// A glyph unit as its element: `<glyph>` for an extended pair, else
/// `<byte>`.
fn raw_unit(unit: &[u8]) -> Piece {
    match unit {
        [page, operand] => Piece::Tag("glyph", vec![("page", *page), ("n", *operand)]),
        _ => Piece::Tag("byte", vec![("v", unit[0])]),
    }
}

fn flush(text: &mut String, pieces: &mut Vec<Piece>) {
    if !text.is_empty() {
        pieces.push(Piece::Text(std::mem::take(text)));
    }
}

/// Encode marked-up pieces back into a message stream.
pub fn decode(pieces: &[Piece], script: Script) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    for piece in pieces {
        match piece {
            Piece::Text(text) => charset::encode(text, script, &mut out),
            Piece::Tag(name, values) => {
                let value = |attribute: &str| -> Result<u8> {
                    values
                        .iter()
                        .find(|(key, _)| *key == attribute)
                        .map(|(_, value)| *value)
                        .ok_or_else(|| anyhow::anyhow!("<{name}> needs a `{attribute}` attribute"))
                };
                match *name {
                    "glyph" => {
                        let page = value("page")?;
                        if !(0xF8..=0xFA).contains(&page) {
                            bail!("<glyph> page must be F8, F9 or FA, not {page:02X}");
                        }
                        out.extend_from_slice(&[page, value("n")?]);
                    }
                    "byte" => out.push(value("v")?),
                    _ => {
                        let Some(&(_, lead, operands)) =
                            TAGS.iter().find(|(tag, _, _)| tag == name)
                        else {
                            bail!("unknown message element <{name}>");
                        };
                        out.push(lead);
                        for operand in operands {
                            out.push(value(operand)?);
                        }
                    }
                }
            }
        }
    }
    Ok(out)
}

/// The attribute names element `name` carries, in byte order.
pub fn attributes(name: &str) -> &'static [&'static str] {
    match name {
        "glyph" => &["page", "n"],
        "byte" => &["v"],
        _ => TAGS
            .iter()
            .find(|(tag, _, _)| *tag == name)
            .map_or(&[], |(_, _, operands)| operands),
    }
}

/// The element name `name` as the static string the pieces hold.
pub fn element(name: &str) -> Option<&'static str> {
    ["glyph", "byte"]
        .into_iter()
        .chain(TAGS.iter().map(|(tag, _, _)| *tag))
        .find(|tag| *tag == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controls_and_text_round_trip() {
        let mut bytes = Vec::new();
        charset::encode("Hello", Script::Latin, &mut bytes);
        bytes.extend_from_slice(&[0x02, 0x03, 0x00, 0x05, 0x02, 0x06, 0x00, 0x07]);
        bytes.extend_from_slice(&[0x08, 0x01, 0x09, 0x02, 0x01, 0x00, 0xFB]);
        let pieces = encode(&bytes, Script::Latin).unwrap();
        assert_eq!(pieces[0], Piece::Text("Hello".to_string()));
        assert!(pieces.contains(&Piece::Tag("br", vec![])));
        assert!(pieces.contains(&Piece::Tag("end", vec![("action", 0)])));
        assert_eq!(decode(&pieces, Script::Latin).unwrap(), bytes);
    }

    #[test]
    fn a_truncated_control_is_a_raw_byte() {
        let pieces = encode(&[0x08, 0x01], Script::Latin).unwrap();
        assert_eq!(pieces[0], Piece::Tag("byte", vec![("v", 0x08)]));
        assert_eq!(decode(&pieces, Script::Latin).unwrap(), [0x08, 0x01]);
    }

    #[test]
    fn text_runs_are_never_adjacent() {
        // Adjacent runs merge when the XML is read back, so a cell that would
        // fuse with the run before it must not start a second run.
        for script in [Script::Latin, Script::Japanese] {
            for first in 0..=0xF7u8 {
                for second in 0..=0xF7u8 {
                    let Some(pieces) = encode(&[first, second], script) else {
                        continue;
                    };
                    assert!(
                        !pieces
                            .windows(2)
                            .any(|pair| matches!(pair, [Piece::Text(_), Piece::Text(_)])),
                        "{first:02X} {second:02X} in {script:?}: {pieces:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn every_byte_round_trips_in_both_scripts() {
        for script in [Script::Latin, Script::Japanese] {
            for byte in 0..=255u8 {
                for next in [0x00, 0x41, 0xFF] {
                    let bytes = [byte, next, 0x01, 0x00];
                    let pieces = encode(&bytes, script)
                        .unwrap_or_else(|| panic!("{byte:02X} {next:02X} in {script:?}"));
                    assert_eq!(decode(&pieces, script).unwrap(), bytes);
                }
            }
        }
    }
}
