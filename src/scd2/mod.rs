//! RE2 SCD scripts: opcode table, reader, `.s` disassembler and assembler,
//! and the `.akscr` decompiler and compiler.
//!
//! An RE2 RDT holds two procedure lists: `init` (section slot 16), run once
//! when the room loads, and `main` (slot 17), whose procedures the room's
//! events and `gosub` call. A list starts with a table of `u16` offsets
//! from the list start, one per procedure, so the first offset is twice
//! the procedure count. A procedure runs from its offset to the next one.
//! The last procedure's end is not stored: it ends at the first `evt_end`
//! outside every block, as biohazard-utils' reader decides it. The bytes
//! after it up to the next section are padding or other data and are not
//! part of the script.
//!
//! The reader keeps every instruction's exact bytes, so
//! [`ProcList::to_bytes`] rebuilds a list byte for byte. Bytes that do not
//! decode, such as data after a procedure's last instruction, stay as the
//! procedure's trailing bytes.

pub mod akscr;
pub mod asm;
pub mod opcode;
pub mod reader;

#[cfg(test)]
mod tests;

use anyhow::{Result, bail};
use opcode::Op;

/// One decoded instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Insn {
    /// Offset of the opcode byte from the start of its procedure.
    pub offset: usize,
    /// The exact bytes, opcode first.
    pub bytes: Vec<u8>,
}

impl Insn {
    /// The opcode's table row.
    pub fn op(&self) -> &'static Op {
        opcode::lookup(self.bytes[0]).expect("an Insn holds a known opcode")
    }

    /// The decoded operands, one per signature character.
    pub fn operands(&self) -> Vec<i64> {
        opcode::operands(self.op(), &self.bytes)
    }

    /// Offset just past the instruction.
    pub fn end(&self) -> usize {
        self.offset + self.bytes.len()
    }
}

/// One procedure.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Procedure {
    /// Decoded instructions.
    pub insns: Vec<Insn>,
    /// Bytes after the last instruction that do not decode.
    pub trailing: Vec<u8>,
}

impl Procedure {
    /// The procedure's bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out: Vec<u8> = self
            .insns
            .iter()
            .flat_map(|insn| insn.bytes.clone())
            .collect();
        out.extend_from_slice(&self.trailing);
        out
    }
}

/// One procedure list (`init` or `main`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProcList {
    /// The procedures in table order.
    pub procs: Vec<Procedure>,
}

impl ProcList {
    /// Check that the list can be stored: every procedure starts within
    /// reach of the `u16` offset table, and the last procedure has no
    /// trailing bytes (the reader ends it at its closing `evt_end`).
    pub fn check(&self, name: &str) -> Result<()> {
        let mut at = self.procs.len() * 2;
        for (index, procedure) in self.procs.iter().enumerate() {
            if at > usize::from(u16::MAX) {
                bail!("{name} procedure {index:02X} starts at byte {at}, past the list's 64 KiB");
            }
            at += procedure.to_bytes().len();
        }
        if self
            .procs
            .last()
            .is_some_and(|last| !last.trailing.is_empty())
        {
            bail!("the last {name} procedure cannot have trailing bytes");
        }
        Ok(())
    }

    /// The list's bytes: the offset table, then each procedure. Offsets
    /// past 64 KiB wrap; [`ProcList::check`] rejects such a list.
    pub fn to_bytes(&self) -> Vec<u8> {
        if self.procs.is_empty() {
            return Vec::new();
        }
        let bodies: Vec<Vec<u8>> = self.procs.iter().map(Procedure::to_bytes).collect();
        let mut out = Vec::new();
        let mut at = self.procs.len() * 2;
        for body in &bodies {
            out.extend_from_slice(&(at as u16).to_le_bytes());
            at += body.len();
        }
        for body in &bodies {
            out.extend_from_slice(body);
        }
        out
    }
}

/// An RE2 room's scripts.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Scripts {
    /// The init procedures.
    pub init: ProcList,
    /// The main procedures.
    pub main: ProcList,
}

impl Scripts {
    /// [`ProcList::check`] both lists.
    pub fn check(&self) -> Result<()> {
        self.init.check("init")?;
        self.main.check("main")
    }
}
