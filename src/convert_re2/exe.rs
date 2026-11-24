//! The English strings `bio2 1.10.exe` carries: the item names, the system
//! messages (the pick-up questions, the item page's answers and the item
//! descriptions) and the door messages.
//!
//! The executable holds each language's tables; Classic REbirth repoints the
//! readers at the English ones (`0x4C9965` for the names, `0x4C8B1B` and
//! `0x4C8C68` for the system messages, `0x4C8ABF` and `0x4C8C4A` for the door
//! messages). Each table is a run of `u16` offsets from its base address to
//! one string: a name ends at `0xF7`, a message at `0xFE`.

use anyhow::{Context, Result, ensure};

/// The address the executable's image is loaded at; a file offset is its
/// virtual address less this.
const IMAGE_BASE: usize = 0x40_0000;

/// One string table: its offsets' address, the address they are relative to,
/// and how many strings it holds.
struct Table {
    offsets: usize,
    base: usize,
    len: usize,
}

/// The item names, ids `0..=0x80`.
const NAMES: Table = Table {
    offsets: 0x52_F1F8,
    base: 0x52_EB98,
    len: 0x81,
};

/// The names CHECK gives the items with two (`name_ptr_set`, `0x4C9909`):
/// the entries after the names by id, by the item's class, `1..=0xA`. Entry
/// 0 is no item's.
const CHECKED_NAMES: Table = Table {
    offsets: NAMES.offsets + 2 * NAMES.len,
    base: NAMES.base,
    len: 0xB,
};

/// The item table (`0x53DE28`): eight bytes an item, the second its class
/// in [`CHECKED_NAMES`].
const ITEM_TABLE: usize = 0x53_DE28;
/// The table's rows: ids `0..=0x71`.
const ITEM_ROWS: usize = 0x72;

/// The system messages: `0..=0x10`, then each item's description at
/// `0x10 + id`, ids `1..=0x63`.
const SYSTEM: Table = Table {
    offsets: 0x53_24D8,
    base: 0x53_0DB0,
    len: 0x74,
};

/// The door messages, `0..=0x17`.
const DOOR: Table = Table {
    offsets: 0x52_F9B4,
    base: 0x52_F618,
    len: 0x18,
};

/// The first system message's opening, which tells the English tables are
/// where [`SYSTEM`] says: `Will you take`.
const FIRST_SYSTEM: &[u8] = &[
    0x33, 0x45, 0x48, 0x48, 0x00, 0x55, 0x4B, 0x51, 0x00, 0x50, 0x3D, 0x47, 0x41,
];

/// Room 200's computer's strings (`Computer200`, `0x4D1170`): ASCII with
/// `Cprint`'s control codes, each ending at a NUL. In the order of
/// [`crate::game::computer2::Strings`]: the title, `RELEASED`, `LOCKED`, the
/// card's line, the newline, `Checking up ID CARD` and the `OK!`.
const COMPUTER: [usize; 7] = [
    0x53_3AD0, 0x53_3AAC, 0x53_3A88, 0x53_3A58, 0x53_3A54, 0x53_3A28, 0x53_3A04,
];

/// The computer's title, which tells [`COMPUTER`] points at its strings.
const COMPUTER_TITLE: &[u8] = b"DOOR LOCK SERVICE]";

/// The executable's English strings, each the raw bytes of one entry with its
/// terminator.
pub(super) struct ExeText {
    /// Item names by id.
    pub names: Vec<Vec<u8>>,
    /// The names CHECK gives, by the item's class ([`CHECKED_NAMES`]);
    /// empty when [`ExeText::checked_error`] says why.
    pub checked_names: Vec<Vec<u8>>,
    /// Each item's class, by id ([`ITEM_TABLE`]); empty with the names.
    pub name_classes: Vec<u8>,
    /// Why the pack gets no second names: the executable's classes aren't
    /// the engine's, or its table doesn't read.
    pub checked_error: Option<String>,
    /// System messages by id.
    pub system: Vec<Vec<u8>>,
    /// Door messages by id.
    pub door: Vec<Vec<u8>>,
    /// Room 200's computer's strings, without their NULs ([`COMPUTER`]).
    pub computer: Vec<Vec<u8>>,
}

impl ExeText {
    /// Read the English tables of `exe`, a copy of `bio2 1.10.exe`.
    pub fn read(exe: &[u8]) -> Result<Self> {
        // The second names are the engine's to show by its own classes: an
        // executable that has others keeps its first names alone.
        let (checked_names, name_classes, checked_error) = match checked_names(exe) {
            Ok((names, classes)) => (names, classes, None),
            Err(error) => (Vec::new(), Vec::new(), Some(format!("{error:#}"))),
        };
        let text = Self {
            names: strings(exe, &NAMES, 0xF7).context("the item names")?,
            checked_names,
            name_classes,
            checked_error,
            system: strings(exe, &SYSTEM, 0xFE).context("the system messages")?,
            door: strings(exe, &DOOR, 0xFE).context("the door messages")?,
            computer: computer_strings(exe).context("the computer's strings")?,
        };
        ensure!(
            text.computer[0].starts_with(COMPUTER_TITLE),
            "the computer's strings do not start with its title"
        );
        ensure!(
            text.system[0].starts_with(FIRST_SYSTEM),
            "the system messages do not start with the pick-up question"
        );
        // `Knife`, item 1.
        ensure!(
            text.names[1] == [0x27, 0x4A, 0x45, 0x42, 0x41, 0xF7],
            "item 1 is not the knife"
        );
        Ok(text)
    }
}

/// The names CHECK gives ([`CHECKED_NAMES`]) and each item's class
/// ([`ITEM_TABLE`]), when the classes are the ones the engine sets and reads
/// (`crate::message::re2_name_class`).
fn checked_names(exe: &[u8]) -> Result<(Vec<Vec<u8>>, Vec<u8>)> {
    let classes = (0..ITEM_ROWS)
        .map(|item| {
            exe.get(ITEM_TABLE - IMAGE_BASE + item * 8 + 1)
                .copied()
                .context("the item table is outside the executable")
        })
        .collect::<Result<Vec<u8>>>()?;
    for (item, &class) in classes.iter().enumerate() {
        let known = crate::message::re2_name_class(item as u8);
        ensure!(
            class == known,
            "item {item:#x} has name class {class}, not the engine's {known}"
        );
    }
    let names = strings(exe, &CHECKED_NAMES, 0xF7).context("the checked names")?;
    Ok((names, classes))
}

/// [`COMPUTER`]'s strings, each up to its NUL. They are printable ASCII, so
/// they can go one a line.
fn computer_strings(exe: &[u8]) -> Result<Vec<Vec<u8>>> {
    COMPUTER
        .iter()
        .map(|&address| {
            let start = address
                .checked_sub(IMAGE_BASE)
                .filter(|&at| at < exe.len())
                .context("a string is outside the executable")?;
            let tail = &exe[start..];
            let len = tail
                .iter()
                .position(|&byte| byte == 0)
                .filter(|&len| len < 0x100)
                .context("a string has no end")?;
            let string = tail[..len].to_vec();
            ensure!(
                string.iter().all(|byte| (0x20..0x7F).contains(byte)),
                "a string at {address:#x} isn't printable ASCII"
            );
            Ok(string)
        })
        .collect()
}

/// The strings of `table` in `exe`, each up to and including its `end` byte
/// (and the byte after `0xFE`, its operand).
fn strings(exe: &[u8], table: &Table, end: u8) -> Result<Vec<Vec<u8>>> {
    let at = |address: usize| address.checked_sub(IMAGE_BASE).filter(|&at| at < exe.len());
    let offsets = at(table.offsets).context("the table is outside the executable")?;
    (0..table.len)
        .map(|index| {
            let entry = offsets + 2 * index;
            let offset = exe
                .get(entry..entry + 2)
                .map(|bytes| usize::from(u16::from_le_bytes([bytes[0], bytes[1]])))
                .with_context(|| format!("entry {index:#x} is truncated"))?;
            let start = at(table.base + offset)
                .with_context(|| format!("entry {index:#x} points outside the executable"))?;
            let tail = &exe[start..];
            let len = tail
                .iter()
                .position(|&byte| byte == end)
                .filter(|&len| len < 0x400)
                .with_context(|| format!("entry {index:#x} has no end"))?;
            let len = len + 1 + usize::from(end == 0xFE);
            Ok(tail[..len.min(tail.len())].to_vec())
        })
        .collect()
}

/// The smallest image [`ExeText::read`] reads: every name the knife, every
/// system message the pick-up question's opening, every door message one
/// letter, and the computer's title and one-letter strings.
#[cfg(test)]
pub(super) fn readable_image() -> Vec<u8> {
    let mut exe = vec![0u8; ITEM_TABLE - IMAGE_BASE + ITEM_ROWS * 8];
    let mut put = |address: usize, bytes: &[u8]| {
        exe[address - IMAGE_BASE..address - IMAGE_BASE + bytes.len()].copy_from_slice(bytes);
    };
    // Each table's offsets are all 0, so every entry is the string at its base.
    put(NAMES.base, &[0x27, 0x4A, 0x45, 0x42, 0x41, 0xF7]);
    put(SYSTEM.base, &[FIRST_SYSTEM, &[0xFE, 0x00]].concat());
    put(DOOR.base, &[0x2D, 0xFE, 0x00]);
    put(COMPUTER[0], &[COMPUTER_TITLE, b"\0"].concat());
    for &address in &COMPUTER[1..] {
        put(address, b"a\0");
    }
    for item in 0..ITEM_ROWS {
        exe[ITEM_TABLE - IMAGE_BASE + item * 8 + 1] = crate::message::re2_name_class(item as u8);
    }
    exe
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An executable's image up to the item table's end, with the engine's
    /// classes and every checked name the one string at the names' base.
    fn image() -> Vec<u8> {
        let mut exe = vec![0u8; ITEM_TABLE - IMAGE_BASE + ITEM_ROWS * 8];
        for item in 0..ITEM_ROWS {
            exe[ITEM_TABLE - IMAGE_BASE + item * 8 + 1] =
                crate::message::re2_name_class(item as u8);
        }
        let base = CHECKED_NAMES.base - IMAGE_BASE;
        exe[base..base + 3].copy_from_slice(&[0x2D, 0x4B, 0xF7]);
        exe
    }

    #[test]
    fn the_smallest_readable_image_reads() {
        let text = ExeText::read(&readable_image()).unwrap();
        assert_eq!(text.names.len(), NAMES.len);
        assert_eq!(text.system.len(), SYSTEM.len);
        assert_eq!(text.door.len(), DOOR.len);
        assert!(text.checked_error.is_none());
    }

    #[test]
    fn the_checked_names_read_with_the_engines_classes() {
        let (names, classes) = checked_names(&image()).unwrap();
        assert_eq!(names.len(), CHECKED_NAMES.len);
        assert!(names.iter().all(|name| name == &[0x2D, 0x4B, 0xF7]));
        assert_eq!(classes.len(), ITEM_ROWS);
        assert_eq!((classes[0x59], classes[0x5E], classes[0x58]), (1, 9, 0));
    }

    /// Another build's classes would name the wrong items: it gets none.
    #[test]
    fn other_classes_or_a_broken_table_give_no_checked_names() {
        // A class the engine doesn't set, and one it sets that is gone.
        for (item, class) in [(0x58, 3), (0x59, 0), (0x5A, 1)] {
            let mut exe = image();
            exe[ITEM_TABLE - IMAGE_BASE + item * 8 + 1] = class;
            let error = checked_names(&exe).unwrap_err();
            assert!(format!("{error:#}").contains("name class"), "{error:#}");
        }
        // A name with no end.
        let mut exe = image();
        let base = CHECKED_NAMES.base - IMAGE_BASE;
        exe[base + 2] = 0;
        assert!(checked_names(&exe).is_err());
        // An image that stops short of the item table.
        let exe = image();
        assert!(checked_names(&exe[..ITEM_TABLE - IMAGE_BASE]).is_err());
    }
}
