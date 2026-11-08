//! An RE2 pack's language file: RE2's item names and the messages its
//! executable draws, read from the executable ([`super::exe`]), the interface
//! strings shared with RE1, and each room's own text decoded from its English
//! message section.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;

use crate::rdt2;
use crate::state::RoomId;

/// The system messages the language file carries as `MESSAGE_00..=10`; the
/// rest are the item descriptions, `0x10 + id`.
const SYSTEM_MESSAGES: usize = 0x11;

/// The strings no RE2 file carries.
const SHARED: &str = include_str!("../../games/re2/language/en-US.txt");

/// The characters of RE2's message bytes, `BioString`'s table; `_` is a byte
/// with no spelling.
const TABLE: &str =
    " .___()__“”_0123456789:_,\"!?_ABCDEFGHIJKLMNOPQRSTUVWXYZ[/]'-_abcdefghijklmnopqrstuvwxyz";

/// A message stream as a language value, in the terms of `Mess_disp`: the
/// printing bytes, the line and page breaks, the character delay and how the
/// message ends. A delay of zero skips to the next `0xFA`, which holds the
/// delay instead; the delays are halved as `Mess_disp` halves them.
pub(super) fn decode_message(stream: &[u8]) -> String {
    let table: Vec<char> = TABLE.chars().collect();
    let mut value = String::new();
    let mut bytes = stream.iter().copied();
    while let Some(byte) = bytes.next() {
        let mut operand = || bytes.next().unwrap_or(0);
        match byte {
            // An item name's end.
            0xF7 => break,
            // The yes/no line's spacing, which the choice's own place covers.
            0xF5 => {}
            0xFE => {
                let ticks = operand();
                if ticks != 0 {
                    let _ = write!(value, "{{timed {ticks}}}");
                }
                break;
            }
            0xFC => value.push_str("\\n"),
            0xFD => {
                let _ = write!(value, "{{clear {}}}", operand());
            }
            0xF9 => {
                let on = operand() == 1;
                let _ = write!(value, "{{color {}}}", u8::from(on));
            }
            0xF8 => {
                let _ = write!(value, "{{string {}}}", operand());
            }
            0xFA => {
                let delay = operand();
                let delay = if delay == 0 { 0 } else { (delay / 2).max(1) };
                let _ = write!(value, "{{scroll {delay}}}");
            }
            // A question: the rest of the bytes are the next message's. Its
            // argument (the cue flags, `& 0x60` and `& 0x20`) is the tag's
            // third byte, behind a branch that runs nothing.
            0xFB => {
                let _ = write!(value, "{{branch 0 0 {}}}", operand());
                break;
            }
            0xEE..=0xF0 => match (byte, operand()) {
                (0xEF, 0xE6) => value.push_str("S."),
                (0xF0, 0x09) => value.push_str("T."),
                (0xF0, 0x0A) => value.push_str("A."),
                (0xF0, 0x0B) => value.push_str("R."),
                (0xEE, 0x12) => value.push('&'),
                _ => {}
            },
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
/// ids are, and how the menu may use them. The amounts are RE2's, measured in
/// the original: a green herb heals 50, two 100, green with blue 50 and two
/// greens with blue 100. Three greens and the first aid spray heal to the
/// maximum (200). The red-herb mixes are taken to heal to the maximum too,
/// and the blue herb's cure is assumed.
pub(super) fn item_definitions() -> Vec<(u8, String)> {
    const HEAL: [(u8, &str); 9] = [
        (0x23, r#"<effect kind="heal" amount="full" />"#),
        (0x26, r#"<effect kind="heal" amount="50" />"#),
        (0x28, r#"<effect kind="cure_poison" />"#),
        (0x29, r#"<effect kind="heal" amount="100" />"#),
        (0x2A, r#"<effect kind="heal" amount="full" />"#),
        (
            0x2B,
            r#"<effect kind="heal" amount="50" /><effect kind="cure_poison" />"#,
        ),
        (0x2C, r#"<effect kind="heal" amount="full" />"#),
        (
            0x2D,
            r#"<effect kind="heal" amount="100" /><effect kind="cure_poison" />"#,
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
                // The stacks are the item table's maximum (`0x53DE28`), and
                // the keys the ids its byte 1 gives a door key to, with the
                // small key.
                let max = crate::scd2::host::item_max(item);
                match item {
                    0x01 => r#"<item kind="weapon" />"#.to_string(),
                    0x02..=0x13 => format!(r#"<item kind="weapon" stack="{max}" />"#),
                    0x14..=0x1D => format!(r#"<item kind="ammo" stack="{max}" merge="true" />"#),
                    0x1E => format!(r#"<item kind="other" stack="{max}" merge="true" />"#),
                    0x1F => format!(r#"<item kind="key" stack="{max}" merge="true" />"#),
                    0x2F | 0x30 => r#"<item kind="special" />"#.to_string(),
                    0x33 | 0x43 | 0x53 | 0x59..=0x5E => r#"<item kind="key" />"#.to_string(),
                    _ => r#"<item kind="other" />"#.to_string(),
                }
            };
            (item, xml)
        })
        .collect()
}

/// The language file: `rooms` lists each room's id and RDT, `exe` holds the
/// executable's English strings and `places` the save screen's name of each
/// room with a typewriter.
pub(super) fn language_file(
    rooms: &[(RoomId, &[u8])],
    exe: Option<&super::exe::ExeText>,
    places: &HashMap<RoomId, String>,
) -> String {
    let mut text = SHARED.to_owned();
    if let Some(exe) = exe {
        exe_lines(exe, &mut text);
    }
    for line in crate::language::LANGUAGE_FILES
        .iter()
        .find(|(entry, _)| entry.eq_ignore_ascii_case("language/en-US.txt"))
        .map_or("", |(_, source)| source)
        .lines()
        .filter(|line| {
            // The interface. The typewriter's messages are the executable's
            // door messages 0 and 1 (`MESSAGE_21`, `MESSAGE_22`), not RE1's.
            line.starts_with("UI_") && !line.starts_with("UI_SAVE_CHARACTER_")
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
        // The save screen's label: the place a room's typewriter names, or
        // the room's number where it has none.
        let room = format!("{:X}{:02X}", id.stage, id.room);
        let label = places
            .get(id)
            .cloned()
            .unwrap_or_else(|| format!("Room {room}"));
        lines.insert(format!("ROOM_{room}_NAME_SAVE"), label);
    }
    for (key, value) in lines {
        let _ = writeln!(text, "{key} = \"{value}\"");
    }
    text
}

/// Where `sys.tex`, Classic REbirth's save-screen strings, keeps its table of
/// `u16` string offsets.
const SYS_TEX_OFFSETS: usize = 0x3B8;
/// The `sys.tex` string of save place 0, "Waiting Room"; place `n` is
/// `SYS_TEX_PLACES + n`.
const SYS_TEX_PLACES: usize = 0x2B;
/// The save places `sys.tex` names.
const PLACE_COUNT: usize = 15;

/// The save screen's place names by room: Classic REbirth's save list
/// (`ddraw.dll`'s `0x10080FC0`) names a save by `sys.tex` string
/// `0x2B + place`, where the place is the first data byte of the room's
/// typewriter (`aot_set`, kind `Sce_Save`) as the save records it. `rooms`
/// lists each room's id and RDT; rooms with no typewriter are left out.
pub(super) fn save_places(sys_tex: &[u8], rooms: &[(RoomId, &[u8])]) -> HashMap<RoomId, String> {
    let place = |index: usize| {
        let at = SYS_TEX_OFFSETS + 2 * (SYS_TEX_PLACES + index);
        let offset = usize::from(u16::from_le_bytes(
            sys_tex.get(at..at + 2)?.try_into().ok()?,
        ));
        let tail = sys_tex.get(offset..SYS_TEX_OFFSETS)?;
        let end = tail.iter().position(|&byte| byte == 0xFE)?;
        Some(decode_message(&tail[..end]))
    };
    let names: Vec<String> = (0..PLACE_COUNT).map_while(place).collect();
    if names.first().map(String::as_str) != Some("Waiting Room") {
        return HashMap::new();
    }
    rooms
        .iter()
        .filter_map(|(id, rdt)| {
            let scripts = crate::scd2::reader::parse(rdt).ok()?;
            let place = [&scripts.init, &scripts.main]
                .into_iter()
                .flat_map(|list| &list.procs)
                .flat_map(|procedure| &procedure.insns)
                .find_map(|insn| {
                    let ops = insn.operands();
                    let data = match insn.op().mnemonic {
                        "aot_set" => 9,
                        "aot_set_4p" => 13,
                        _ => return None,
                    };
                    (ops.get(1) == Some(&i64::from(crate::scd2::host::SCE_SAVE)))
                        .then(|| ops.get(data).copied())
                        .flatten()
                })?;
            let name = names.get(usize::try_from(place).ok()?)?;
            Some((*id, name.clone()))
        })
        .collect()
}

/// The item names and descriptions and the system and door messages of
/// `exe`, as language lines.
fn exe_lines(exe: &super::exe::ExeText, text: &mut String) {
    for (item, name) in exe.names.iter().enumerate().skip(1) {
        let name = decode_message(name);
        if !name.is_empty() {
            let _ = writeln!(text, "ITEM_{item:02X}_NAME = \"{name}\"");
        }
    }
    for (index, message) in exe.system.iter().enumerate() {
        let value = decode_message(message);
        if index < SYSTEM_MESSAGES {
            let _ = writeln!(text, "MESSAGE_{index:02X} = \"{value}\"");
        } else {
            let item = index - 0x10;
            let _ = writeln!(text, "ITEM_{item:02X}_DESCRIPTION = \"{value}\"");
        }
    }
    for (index, message) in exe.door.iter().enumerate() {
        let id = usize::from(crate::scd2::host::DOOR_MESSAGES) + index;
        let _ = writeln!(text, "MESSAGE_{id:02X} = \"{}\"", decode_message(message));
    }
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
        // The lighter, whose id is RE1's ink ribbon.
        assert_eq!(kind(0x2F), ItemKind::Special);
        assert_eq!(kind(0x59), ItemKind::Key);
        assert_eq!(kind(0x1F), ItemKind::Key);
        assert_eq!(kind(0x1E), ItemKind::Other);
        let def =
            |item: u8| crate::item_catalog::ItemDef::parse(&defs[usize::from(item) - 1].1).unwrap();
        assert_eq!(def(0x14).stack, 250);
        assert_eq!(def(0x17).stack, 100);
        assert_eq!(def(0x02).stack, 18);
        assert!(def(0x1E).merge && def(0x1F).merge);
    }

    #[test]
    fn the_item_definitions_heal_re2_amounts() {
        use crate::item_catalog::{HealAmount, ItemDef};
        let defs = item_definitions();
        let amount = |item: u8| {
            ItemDef::parse(&defs[usize::from(item) - 1].1)
                .unwrap()
                .heal
                .amount
        };
        assert_eq!(amount(0x26), HealAmount::Points(50));
        assert_eq!(amount(0x29), HealAmount::Points(100));
        assert_eq!(amount(0x2B), HealAmount::Points(50));
        assert_eq!(amount(0x2D), HealAmount::Points(100));
        for full in [0x23, 0x2A, 0x2C, 0x2E] {
            assert_eq!(amount(full), HealAmount::Full, "{full:#x}");
        }
        assert_eq!(amount(0x28), HealAmount::None);
        // Full means the RE2 maximum.
        assert_eq!(HealAmount::Full.apply(1, crate::game::RE2_MAX_HEALTH), 200);
    }

    #[test]
    fn using_a_herb_in_re2_heals_its_points_and_stops_at_200() {
        use crate::game::{GameState, UseResult};
        let mut writer = crate::pack::PackWriter::new();
        for (item, xml) in item_definitions() {
            writer
                .add(&crate::item_catalog::entry(item), xml.into_bytes())
                .unwrap();
        }
        let pack = crate::pack::Pack::from_bytes(writer.to_bytes().unwrap()).unwrap();
        let catalog = std::sync::Arc::new(crate::item_catalog::ItemCatalog::load(&pack));
        // (item, life before, life after), as measured in the original.
        for (item, before, after) in [
            (0x26, 100, 150),
            (0x26, 190, 200),
            (0x29, 50, 150),
            (0x2B, 1, 51),
            (0x2D, 1, 101),
            (0x2C, 1, 200),
            (0x2A, 50, 200),
            (0x23, 1, 200),
        ] {
            let mut state = GameState::default();
            state.item_defs = catalog.clone();
            state.is_re2_room = true;
            // RE1's maximum for Jill must not be the cap in an RE2 room.
            state.max_health = 96;
            state.entities[0].health = before;
            assert!(
                matches!(state.use_item(item), UseResult::Used { healed: true, .. }),
                "{item:#x} at {before}"
            );
            assert_eq!(state.entities[0].health, after, "{item:#x} at {before}");
        }
        // At the maximum a herb changes nothing.
        let mut state = GameState::default();
        state.item_defs = catalog;
        state.is_re2_room = true;
        state.entities[0].health = 200;
        state.use_item(0x26);
        assert_eq!(state.entities[0].health, 200);
    }

    #[test]
    fn a_message_decodes_its_letters_and_breaks() {
        // "Hi" over two lines, a page, then "?" and the end.
        let stream = [
            0xFA, 2, 0x24, 0x45, 0xFC, 0x24, 0xFD, 0, 0x1B, 0xFE, 0, 0x24,
        ];
        assert_eq!(decode_message(&stream), "{scroll 1}Hi\\nH{clear 0}?");
    }

    #[test]
    fn a_message_keeps_how_it_ends() {
        // A voiced line's subtitle: skipped text, a delay, a timed end.
        let stream = [0xFA, 0, 0x24, 0xFA, 1, 0xFE, 0x14, 0x24];
        assert_eq!(decode_message(&stream), "{scroll 0}H{scroll 1}{timed 20}");
        // A page held for a while.
        assert_eq!(decode_message(&[0xFD, 0x49, 0xFE, 0]), "{clear 73}");
        // A question stops where the next message begins.
        assert_eq!(
            decode_message(&[0x24, 0xFB, 0x44, 0x24]),
            "H{branch 0 0 68}"
        );
    }

    #[test]
    fn a_message_spells_the_stars_ligature() {
        let stream = [0xEF, 0xE6, 0xF0, 0x09, 0xF0, 0x0A, 0xF0, 0x0B, 0xEF, 0xE6];
        assert_eq!(decode_message(&stream), "S.T.A.R.S.");
    }

    #[test]
    fn the_shared_strings_parse() {
        let rooms: [(RoomId, &[u8]); 0] = [];
        crate::language::Language::parse(&language_file(&rooms, None, &HashMap::new())).unwrap();
    }

    #[test]
    fn a_save_place_is_the_typewriters_place_in_sys_tex() {
        let Some(root) = std::env::var_os("ARKLAY_RE2_ROOT").map(std::path::PathBuf::from) else {
            return;
        };
        let ddraw = std::fs::read(root.join("ddraw.dll")).unwrap();
        let archive = crate::sevenz::find(&ddraw).unwrap();
        let sys_tex = crate::sevenz::extract(archive, |name| {
            name.eq_ignore_ascii_case(super::super::SYS_TEX_7Z)
        })
        .unwrap()
        .remove(0)
        .1;
        let rdt = |name: &str| std::fs::read(root.join("data/Pl0/Rdt").join(name)).unwrap();
        let (hall, cabin, street) = (
            rdt("ROOM2000.RDT"),
            rdt("ROOM1060.RDT"),
            rdt("ROOM1000.RDT"),
        );
        let id = |text: &str| RoomId::parse(text).unwrap();
        let rooms = [
            (id("2000"), hall.as_slice()),
            (id("1060"), cabin.as_slice()),
            (id("1000"), street.as_slice()),
        ];
        let places = save_places(&sys_tex, &rooms);
        assert_eq!(places.get(&id("2000")).map(String::as_str), Some("Hall"));
        assert_eq!(places.get(&id("1060")).map(String::as_str), Some("Cabin"));
        // The street has no typewriter.
        assert_eq!(places.get(&id("1000")), None);
    }

    #[test]
    fn a_name_ends_at_its_terminator_and_spells_the_ampersand() {
        // `B&D`, the end, then the next name.
        assert_eq!(decode_message(&[0x1E, 0xEE, 0x12, 0x20, 0xF7, 0x1D]), "B&D");
        // The yes/no line's spacing draws nothing.
        assert_eq!(
            decode_message(&[0xFC, 0xF5, 0xF5, 0xFB, 0x80]),
            "\\n{branch 0 0 128}"
        );
    }

    /// The executable of the install `ARKLAY_RE2_ROOT` names, when it is set.
    fn installed_exe() -> Option<super::super::exe::ExeText> {
        let root = std::path::PathBuf::from(std::env::var_os("ARKLAY_RE2_ROOT")?);
        let exe = std::fs::read(root.join("bio2 1.10.exe")).ok()?;
        Some(super::super::exe::ExeText::read(&exe).expect("the English tables read"))
    }

    #[test]
    fn the_executables_names_and_messages_are_the_originals() {
        let Some(exe) = installed_exe() else {
            return;
        };
        let name = |item: usize| decode_message(&exe.names[item]);
        assert_eq!(name(0x02), "Hand Gun");
        assert_eq!(name(0x14), "H. Gun Bullets");
        assert_eq!(name(0x15), "Shotgun Shells");
        assert_eq!(name(0x23), "F. Aid Spray");
        assert_eq!(name(0x26), "Green Herb");
        assert_eq!(name(0x29), "Mixed Herb");
        assert_eq!(name(0x42), "Bomb & Det.");
        let system = |id: usize| decode_message(&exe.system[id]);
        assert_eq!(
            system(0),
            "Will you take the\\n{color 1}{string 0}{color 0}?\\n{branch 0 0 128}"
        );
        assert_eq!(
            system(6),
            "You've taken the\\n{color 1}{string 0}{color 0}."
        );
        assert_eq!(system(7), "You don't need to\\nuse this right now.");
        assert_eq!(system(0xB), "Will you mix \\nthe herb?\\n{branch 0 0 128}");
        assert_eq!(
            system(0x11),
            "A combat knife.\\nIt could come in\\nhandy..."
        );
        let door = |id: usize| decode_message(&exe.door[id]);
        assert_eq!(door(0xA), "\\nYou unlocked it.");
        assert_eq!(door(0xC), "\\nIt's locked.");
        let mut text = String::new();
        exe_lines(&exe, &mut text);
        crate::language::Language::parse(&text).unwrap();
    }
}
