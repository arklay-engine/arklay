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

/// One checked-in room script (`games/re1/room/<room>/room.akscr`), built
/// once for every RDT variant and region it covers.
#[derive(Debug)]
pub struct RoomScript {
    /// The room: its stage and room number, e.g. `0x103`.
    pub room: u16,
    /// The runtime build, an SCD container: every variant's scripts, which
    /// the engine picks between for the room it runs.
    pub container: &'static [u8],
}

/// The checked-in room scripts, compiled by the build script.
pub static ROOM_SCRIPTS: &[RoomScript] = include!(concat!(env!("OUT_DIR"), "/room_scripts.rs"));
