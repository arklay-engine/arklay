//! SCD scripts: opcode tables, reader, IR, disassembler, decompiler and VMs.
//!
//! An RDT stores three kinds of script data: the `init` procedure container,
//! the `main` procedure container (both run by the command VM) and a table of
//! event scripts (run by the event VM). The reader decodes the raw bytes into
//! an instruction IR that keeps every original byte and absolute file offset.

pub mod akscr;
pub mod asm;
pub mod decomp;
pub mod disasm;
pub mod host;
pub mod ir;
pub mod opcode;
pub mod reader;
pub mod vm;

use akscr::Region;

/// One checked-in room script (`games/re1/room/<room>.akscr`) compiled for
/// one RDT of one region.
#[derive(Debug)]
pub struct RoomScript {
    /// The RDT, e.g. `0x1030` for stage 1, room 03, player 0.
    pub id: u16,
    /// The regions whose RDT this build is for: every region whose
    /// `AUTHENTIC` build has the same digest.
    pub regions: &'static [Region],
    /// The [`ir::Scripts::digest`] of the script's `AUTHENTIC` build: the
    /// RDT's own scripts have it when the script was written from them.
    pub original: u64,
    /// The fixed build, an SCD container.
    pub container: &'static [u8],
}

/// The checked-in room scripts, compiled by the build script for every RDT
/// variant they cover: the USA region's build, then each other region's
/// whose `AUTHENTIC` build differs (`#if JPN`, `#if EUR`).
pub static ROOM_SCRIPTS: &[RoomScript] = include!(concat!(env!("OUT_DIR"), "/room_scripts.rs"));
