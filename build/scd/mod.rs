//! The `.akscr` compiler, built into the build script from the crate's own
//! sources so the room scripts ship as bytecode. Only the compile path is
//! loaded; the module paths match the crate's, so the sources build
//! unchanged.

#![allow(dead_code, unused_imports)]

#[path = "../../src/scd/asm.rs"]
pub mod asm;
#[path = "../../src/scd/decomp.rs"]
pub mod decomp;
#[path = "../../src/scd/disasm.rs"]
pub mod disasm;
#[path = "../../src/scd/ir.rs"]
pub mod ir;
#[path = "../../src/scd/opcode.rs"]
pub mod opcode;
#[path = "../../src/scd/reader.rs"]
pub mod reader;

pub mod akscr;
