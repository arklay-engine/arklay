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

/// One checked-in room script (`games/re1/room/<room>0/room.akscr`), built for
/// every RDT variant and region it covers.
#[derive(Debug)]
pub struct RoomScript {
    /// The room: its stage and room number, e.g. `0x103`.
    pub room: u16,
    /// The `AUTHENTIC` runtime build, an SCD container: the room's own
    /// scripts without the port's fixes. `None` when the fixes leave it
    /// unchanged, so it is the same as [`RoomScript::container`].
    pub authentic: Option<&'static [u8]>,
    /// The runtime build, an SCD container: every variant's scripts, which
    /// the engine picks between for the room it runs.
    pub container: &'static [u8],
    /// The RDTs the script covers, each with the digest of its `AUTHENTIC`
    /// build per group of regions that build it alike.
    pub originals: &'static [RdtOriginal],
}

/// The scripts one RDT variant has in the regions of one build.
#[derive(Debug)]
pub struct RdtOriginal {
    /// The RDT, e.g. `0x1030` for stage 1, room 03, player 0.
    pub id: u16,
    /// The regions whose RDT this is: every region whose `AUTHENTIC` build
    /// of it has the same digest.
    pub regions: &'static [Region],
    /// The [`ir::Scripts::digest`] of the variant's `AUTHENTIC` build: the
    /// RDT's own scripts have it when the script was written from them.
    pub digest: u64,
}

/// The checked-in room scripts, compiled by the build script.
pub static ROOM_SCRIPTS: &[RoomScript] = include!(concat!(env!("OUT_DIR"), "/room_scripts.rs"));
