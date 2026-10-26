//! SCD intermediate representation.
//!
//! Every instruction keeps the exact original bytes and its absolute offset in
//! the RDT, so the disassembler, the decompiler and a future assembler can all
//! work from one lossless decode.

use crate::scd::opcode::Op;

/// Which script stream an instruction belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StreamKind {
    /// The room initialization procedure container (RDT pointer slot 6).
    #[default]
    Init,
    /// The per-frame procedure container (RDT pointer slot 7).
    Main,
    /// Event script `n` from the event table (RDT pointer slot 8).
    Event(u8),
}

/// One decoded instruction. `bytes` is the exact original byte range, so a
/// future assembler can re-encode losslessly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Insn {
    /// Absolute offset of the opcode byte in the RDT.
    pub offset: usize,
    /// The raw opcode byte.
    pub op: u8,
    /// Exact original bytes, including the opcode byte.
    pub bytes: Vec<u8>,
    /// Which opcode table the instruction came from.
    pub decoded: Decoded,
    /// Decoded operands in order.
    pub operands: Vec<Operand>,
}

/// The opcode table an instruction was decoded with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decoded {
    /// Command VM opcode 0x00-0x50.
    Command(&'static Op),
    /// Event VM state-0 opcode (0x00-0x09).
    Event(&'static Op),
    /// Event VM control opcode (0xF6-0xFF), valid in every state.
    Control(&'static Op),
    /// Event VM actor sub-ISA opcode (state 1).
    Actor(&'static Op),
    /// Event VM tween sub-ISA opcode (state 2).
    Tween(&'static Op),
    /// The size word of one inner block of an `evt_block` chain.
    SubBlockHeader { size: u16 },
    /// Unknown opcode: the stream ends and the rest is trailing data.
    Raw,
}

/// One decoded operand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Operand {
    /// Sign-extended value according to the operand signature.
    pub value: i64,
    /// Absolute RDT offset for jump operands.
    pub target: Option<usize>,
}

/// A command-VM container block: size word plus opcodes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Block {
    /// Offset of the size word.
    pub offset: usize,
    /// The size word, including its own two bytes.
    pub size: u16,
    /// Decoded instructions.
    pub insns: Vec<Insn>,
    /// Undecoded bytes after the last instruction.
    pub trailing: Vec<u8>,
}

/// One event script.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Stream {
    /// Which stream this is.
    pub kind: StreamKind,
    /// Absolute offset of the first instruction.
    pub offset: usize,
    /// Decoded instructions.
    pub insns: Vec<Insn>,
    /// Undecoded bytes after the last instruction.
    pub trailing: Vec<u8>,
}

/// Every SCD stream of an RDT file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Scripts {
    /// Blocks of the init procedure container.
    pub init: Vec<Block>,
    /// Blocks of the main procedure container.
    pub main: Vec<Block>,
    /// Event scripts in table order.
    pub events: Vec<Stream>,
}

impl Scripts {
    /// A 64-bit FNV-1a hash of every stream's bytes, in order: two script
    /// sets with the same digest hold the same bytecode wherever it sits.
    pub fn digest(&self) -> u64 {
        fn feed(hash: &mut u64, bytes: &[u8]) {
            for &byte in bytes {
                *hash = (*hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01B3);
            }
        }
        fn stream(hash: &mut u64, insns: &[Insn], trailing: &[u8]) {
            let len = insns.iter().map(|insn| insn.bytes.len()).sum::<usize>() + trailing.len();
            feed(hash, &(len as u32).to_le_bytes());
            for insn in insns {
                feed(hash, &insn.bytes);
            }
            feed(hash, trailing);
        }
        let mut hash = 0xCBF2_9CE4_8422_2325;
        for blocks in [&self.init, &self.main] {
            feed(&mut hash, &(blocks.len() as u32).to_le_bytes());
            for block in blocks {
                stream(&mut hash, &block.insns, &block.trailing);
            }
        }
        feed(&mut hash, &(self.events.len() as u32).to_le_bytes());
        for event in &self.events {
            stream(&mut hash, &event.insns, &event.trailing);
        }
        hash
    }
}
