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

/// The executable's English strings, each the raw bytes of one entry with its
/// terminator.
pub(super) struct ExeText {
    /// Item names by id.
    pub names: Vec<Vec<u8>>,
    /// System messages by id.
    pub system: Vec<Vec<u8>>,
    /// Door messages by id.
    pub door: Vec<Vec<u8>>,
}

impl ExeText {
    /// Read the English tables of `exe`, a copy of `bio2 1.10.exe`.
    pub fn read(exe: &[u8]) -> Result<Self> {
        let text = Self {
            names: strings(exe, &NAMES, 0xF7).context("the item names")?,
            system: strings(exe, &SYSTEM, 0xFE).context("the system messages")?,
            door: strings(exe, &DOOR, 0xFE).context("the door messages")?,
        };
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
