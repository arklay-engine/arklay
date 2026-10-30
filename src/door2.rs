//! RE2 `.DO2` door-animation container parser.
//!
//! A DO2 is the whole door, stairs or ladder transition of RE2: the sounds it
//! plays, a block of RE2 SCD procedures that animate up to ten door parts, an
//! MD1 mesh of those parts and an 8bpp texture. [`parse`] reads the file and
//! [`vm::Vm`] runs the procedures.
//!
//! Layout:
//!
//! ```text
//! 0x00  EDT     four 4-byte sound entries; byte 1 & 0x7F is the VAB program,
//!               byte 2 >> 4 the tone; 0xFFFFFFFF is no entry
//! 0x10  VAB     a PlayStation `pBAV` sound bank (see vab), size at VAB+0x0C
//! base  block   base = align_up(0x10 + vab size + 8, 0x800)
//! ```
//!
//! The block starts with two `u32` offsets relative to the block, the MD1
//! mesh and the TIM. Between that header and the mesh is an RE2 SCD
//! procedure list (a `u16` offset table, then the procedures). The mesh is an
//! RE2 MD1 (one object per triangle/quad pair, parsed by
//! [`crate::emd2::parse_md1`]) and the texture a 128x256 8bpp TIM with one
//! 256-colour CLUT row, decoded by [`crate::tim::decode_8bpp`].

pub mod vab;
pub mod vm;

use anyhow::{Context, Result, ensure};

use crate::model::{Texture8, Tmd};
use crate::{emd2, scd2, tim};

pub use vab::Vab;

/// Number of entries in the sound table at the start of the file.
pub const SOUND_COUNT: usize = 4;
/// Where the VAB starts.
const VAB_OFFSET: usize = 0x10;
/// The door block is aligned to a CD sector.
const BLOCK_ALIGN: usize = 0x800;
/// The block header: the mesh and texture offsets.
const BLOCK_HEADER_LEN: usize = 8;
/// Number of door files the retail game ships (`DOOR00`-`DOOR36`, hex).
pub const RETAIL_COUNT: u8 = 0x37;

/// One sound table entry: which tone of the VAB a sound plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sound {
    /// VAB program.
    pub program: u8,
    /// Tone within the program.
    pub tone: u8,
}

/// A parsed `.DO2`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Do2 {
    /// The sound table, indexed by the `se_on` sound index.
    pub sounds: [Option<Sound>; SOUND_COUNT],
    /// The sound bank.
    pub vab: Vab,
    /// The procedure list: its `u16` offset table and the procedures, with
    /// offsets relative to the start of this buffer.
    pub scripts: Vec<u8>,
    /// Each procedure's start in `scripts`.
    pub procedures: Vec<usize>,
    /// The door parts, one object each.
    pub mesh: Tmd,
    /// The 8bpp texture.
    pub texture: Texture8,
}

impl Do2 {
    /// The tone sound-table entry `index` plays, if it has one.
    pub fn sound_tone(&self, index: usize) -> Option<&vab::Tone> {
        let sound = (*self.sounds.get(index)?)?;
        self.vab
            .tone(usize::from(sound.program), usize::from(sound.tone))
    }
}

/// The pack entry (`door/<id>.do2`, two hex digits) of a door id.
pub fn entry(id: u8) -> String {
    format!("door/{id:02x}.do2")
}

/// The PC door sound bank (`door%02d.sap`, see [`crate::se2`]) door `id`
/// plays its sounds from. `Door_load` shares a few banks between doors and
/// otherwise loads the door's own; the DO2's sound table still picks each
/// sound's volume.
pub fn sound_bank(id: u8) -> u8 {
    match id {
        0x02 | 0x03 | 0x05 | 0x0B | 0x0D | 0x18 => 0x00,
        0x04 | 0x09 | 0x11 => 0x01,
        0x07 | 0x08 | 0x22 | 0x2F => 0x06,
        0x1A | 0x23 => 0x15,
        0x30 => 0x1B,
        0x29 => 0x24,
        0x31 => 0x26,
        _ => id,
    }
}

/// Parse a `.DO2` file.
pub fn parse(data: &[u8]) -> Result<Do2> {
    ensure!(
        data.len() >= VAB_OFFSET,
        "DO2 is truncated: {} bytes",
        data.len()
    );
    let sounds = std::array::from_fn(|index| {
        let entry = &data[index * 4..index * 4 + 4];
        (entry != [0xFF; 4]).then(|| Sound {
            program: entry[1] & 0x7F,
            tone: entry[2] >> 4,
        })
    });
    let vab = Vab::parse(&data[VAB_OFFSET..]).context("DO2 sound bank")?;
    let base = (VAB_OFFSET + vab.size + 8).next_multiple_of(BLOCK_ALIGN);
    let block = data
        .get(base..)
        .filter(|block| block.len() >= BLOCK_HEADER_LEN)
        .with_context(|| format!("DO2 door block at 0x{base:x} is past the file's end"))?;
    let u32_at = |at: usize| u32::from_le_bytes(block[at..at + 4].try_into().unwrap()) as usize;
    let (mesh_at, texture_at) = (u32_at(0), u32_at(4));
    ensure!(
        BLOCK_HEADER_LEN < mesh_at && mesh_at <= texture_at && texture_at < block.len(),
        "DO2 mesh at 0x{mesh_at:x} and texture at 0x{texture_at:x} do not fit its \
         0x{:x}-byte door block",
        block.len()
    );
    let scripts = block[BLOCK_HEADER_LEN..mesh_at].to_vec();
    let (list, _) = scd2::reader::parse_list(&scripts).context("DO2 procedures")?;
    let procedures = (0..list.procs.len())
        .map(|index| {
            usize::from(u16::from_le_bytes([
                scripts[index * 2],
                scripts[index * 2 + 1],
            ]))
        })
        .collect();
    let mesh = emd2::parse_md1(&block[mesh_at..texture_at]).context("DO2 mesh")?;
    let texture = tim::decode_8bpp(&block[texture_at..]).context("DO2 texture")?;
    Ok(Do2 {
        sounds,
        vab,
        scripts,
        procedures,
        mesh,
        texture,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_doors_take_the_bank_door_load_picks() {
        assert_eq!(sound_bank(0x00), 0x00);
        assert_eq!(sound_bank(0x0B), 0x00);
        assert_eq!(sound_bank(0x11), 0x01);
        assert_eq!(sound_bank(0x2F), 0x06);
        assert_eq!(sound_bank(0x23), 0x15);
        assert_eq!(sound_bank(0x29), 0x24);
        assert_eq!(sound_bank(0x2B), 0x2B);
    }
}
