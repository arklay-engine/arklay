//! An RE2 pack's language file: RE2's item names and the messages its
//! executable draws, the interface strings shared with RE1, and each room's
//! own text decoded from its English message section.

use std::collections::BTreeMap;
use std::fmt::Write;

use crate::rdt2;
use crate::state::RoomId;

/// The strings no RE2 file carries.
const SHARED: &str = include_str!("../../games/re2/language/en-US.txt");

/// The characters of RE2's message bytes, `BioString`'s table; `_` is a byte
/// with no spelling.
const TABLE: &str =
    " .___()__“”_0123456789:_,\"!?_ABCDEFGHIJKLMNOPQRSTUVWXYZ[/]'-_abcdefghijklmnopqrstuvwxyz";

/// A message stream as a language value: the printing bytes and the line and
/// page breaks. `0xFB`'s yes/no prompts have no counterpart yet, so their text
/// reads as a statement.
pub(super) fn decode_message(stream: &[u8]) -> String {
    let table: Vec<char> = TABLE.chars().collect();
    let mut value = String::new();
    let mut bytes = stream.iter().copied();
    while let Some(byte) = bytes.next() {
        match byte {
            0xFE => break,
            0xFC => value.push_str("\\n"),
            0xFD => {
                bytes.next();
                value.push_str("{clear 0}");
            }
            0xF9 => {
                let on = bytes.next().unwrap_or(0) == 1;
                let _ = write!(value, "{{color {}}}", u8::from(on));
            }
            0xFA | 0xFB => {
                bytes.next();
            }
            _ => match table.get(usize::from(byte)) {
                Some('_') | None => {}
                Some('"') => value.push_str("\\\""),
                Some(ch) => value.push(*ch),
            },
        }
    }
    value
}

/// The item definitions, which replace RE1's for the ids they share: what the
/// ids are, and how the menu may use them. Every herb's amount is an
/// approximation in RE1's thirds (`docs/re2-deviations.md`).
pub(super) fn item_definitions() -> Vec<(u8, String)> {
    const HEAL: [(u8, &str); 9] = [
        (0x23, r#"<effect kind="heal" amount="full" />"#),
        (0x26, r#"<effect kind="heal" amount="third" />"#),
        (0x28, r#"<effect kind="cure_poison" />"#),
        (0x29, r#"<effect kind="heal" amount="two_thirds" />"#),
        (0x2A, r#"<effect kind="heal" amount="full" />"#),
        (
            0x2B,
            r#"<effect kind="heal" amount="third" /><effect kind="cure_poison" />"#,
        ),
        (0x2C, r#"<effect kind="heal" amount="full" />"#),
        (
            0x2D,
            r#"<effect kind="heal" amount="two_thirds" /><effect kind="cure_poison" />"#,
        ),
        (
            0x2E,
            r#"<effect kind="heal" amount="full" /><effect kind="cure_poison" />"#,
        ),
    ];
    (1..=0x67)
        .map(|item| {
            let xml = if let Some((_, effect)) = HEAL.iter().find(|(id, _)| *id == item) {
                format!(r#"<item kind="heal" stack="1"><effects>{effect}</effects></item>"#)
            } else {
                match item {
                    0x01 => r#"<item kind="weapon" />"#.to_string(),
                    0x02..=0x13 => r#"<item kind="weapon" stack="255" />"#.to_string(),
                    0x14..=0x1E => r#"<item kind="ammo" stack="255" merge="true" />"#.to_string(),
                    _ => r#"<item kind="other" />"#.to_string(),
                }
            };
            (item, xml)
        })
        .collect()
}

/// The language file: `rooms` lists each room's id and RDT.
pub(super) fn language_file(rooms: &[(RoomId, &[u8])]) -> String {
    let mut text = SHARED.to_owned();
    for line in crate::language::LANGUAGE_FILES
        .iter()
        .find(|(entry, _)| entry.eq_ignore_ascii_case("language/en-US.txt"))
        .map_or("", |(_, source)| source)
        .lines()
        .filter(|line| {
            // The interface, and the typewriter's three prompts.
            (line.starts_with("UI_") && !line.starts_with("UI_SAVE_CHARACTER_"))
                || ["MESSAGE_1E", "MESSAGE_1F", "MESSAGE_20"]
                    .iter()
                    .any(|key| line.starts_with(key))
        })
    {
        text.push_str(line);
        text.push('\n');
    }
    let mut lines = BTreeMap::new();
    for (id, rdt) in rooms {
        let tag = crate::room::xml::string_variants(*id).remove(0);
        for (index, stream) in rdt2::messages(rdt).unwrap_or_default().iter().enumerate() {
            let key = format!("ROOM_{:X}{:02X}_{tag}_TEXT_{index:02X}", id.stage, id.room);
            lines.insert(key, decode_message(stream));
        }
        // The save screen's label: RE2's own place names are not read yet.
        let room = format!("{:X}{:02X}", id.stage, id.room);
        lines.insert(format!("ROOM_{room}_NAME_SAVE"), format!("Room {room}"));
    }
    for (key, value) in lines {
        let _ = writeln!(text, "{key} = \"{value}\"");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_item_definitions_parse_and_give_re2_kinds() {
        let defs = item_definitions();
        for (item, xml) in &defs {
            crate::item_catalog::ItemDef::parse(xml)
                .unwrap_or_else(|error| panic!("{item:#x}: {error:#}"));
        }
        let kind = |item: u8| {
            crate::item_catalog::ItemDef::parse(&defs[usize::from(item) - 1].1)
                .unwrap()
                .kind
        };
        use crate::item_catalog::ItemKind;
        assert_eq!(kind(0x02), ItemKind::Weapon);
        assert_eq!(kind(0x14), ItemKind::Ammo);
        assert_eq!(kind(0x23), ItemKind::Heal);
        // The lighter: RE1's id means the ink ribbon.
        assert_eq!(kind(0x2F), ItemKind::Other);
    }

    #[test]
    fn a_message_decodes_its_letters_and_breaks() {
        // "Hi" over two lines, a page, then "?" and the end.
        let stream = [
            0xFA, 2, 0x24, 0x45, 0xFC, 0x24, 0xFD, 0, 0x1B, 0xFE, 0, 0x24,
        ];
        assert_eq!(decode_message(&stream), "Hi\\nH{clear 0}?");
    }

    #[test]
    fn the_shared_strings_parse() {
        let rooms: [(RoomId, &[u8]); 0] = [];
        crate::language::Language::parse(&language_file(&rooms)).unwrap();
    }
}
