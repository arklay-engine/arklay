//! RE2 RDT script reader.
//!
//! Malformed data never panics. A bad section offset or a procedure table
//! that does not fit is an error; an unknown opcode, or an instruction that
//! runs past its procedure, ends the procedure's instructions and the rest
//! of its bytes become trailing data.

use anyhow::{Context, Result, bail};

use crate::budget;
use crate::scd2::opcode::{self, EVT_END};
use crate::scd2::{Insn, ProcList, Procedure, Scripts};

/// Offset of the RDT's section offset table.
const OFFSETS: usize = 0x08;
/// Number of section offsets.
const OFFSET_COUNT: usize = 23;
/// Section slot of the init procedure list.
pub const INIT_SLOT: usize = 16;
/// Section slot of the main procedure list.
pub const MAIN_SLOT: usize = 17;

/// Where a script section lies in an RDT: `start` and its byte length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub len: usize,
}

/// Parse an RE2 RDT's init and main procedure lists.
pub fn parse(data: &[u8]) -> Result<Scripts> {
    Ok(parse_with_spans(data)?.0)
}

/// [`parse`], also returning where each list lies in the RDT (`None` for
/// an absent list). A list's span covers what [`ProcList::to_bytes`]
/// rebuilds.
pub fn parse_with_spans(data: &[u8]) -> Result<(Scripts, [Option<Span>; 2])> {
    let table = data
        .get(OFFSETS..OFFSETS + OFFSET_COUNT * 4)
        .context("RDT is too short for the section offset table")?;
    let offsets: Vec<usize> = table
        .as_chunks::<4>()
        .0
        .iter()
        .map(|word| u32::from_le_bytes(*word) as usize)
        .collect();
    let mut spans = [None; 2];
    let mut lists = [ProcList::default(), ProcList::default()];
    for (index, slot) in [INIT_SLOT, MAIN_SLOT].into_iter().enumerate() {
        let start = offsets[slot];
        if start == 0 {
            continue;
        }
        if start >= data.len() {
            bail!(
                "section {slot} offset 0x{start:x} is past the end of the {}-byte RDT",
                data.len()
            );
        }
        // The list can run up to the next section in the file.
        let bound = offsets
            .iter()
            .copied()
            .filter(|&offset| offset > start)
            .min()
            .unwrap_or(data.len())
            .min(data.len());
        let (list, len) = parse_list(&data[start..bound])
            .with_context(|| format!("procedure list in section {slot}"))?;
        spans[index] = Some(Span { start, len });
        lists[index] = list;
    }
    let [init, main] = lists;
    Ok((Scripts { init, main }, spans))
}

/// Parse one procedure list from `data` (from the list start up to the
/// furthest it can run), returning it and the bytes it covers.
pub fn parse_list(data: &[u8]) -> Result<(ProcList, usize)> {
    let u16_at = |at: usize| -> Result<usize> {
        let bytes = data
            .get(at..at + 2)
            .with_context(|| format!("procedure table is truncated at 0x{at:x}"))?;
        Ok(usize::from(u16::from_le_bytes([bytes[0], bytes[1]])))
    };
    let first = u16_at(0)?;
    if first == 0 || !first.is_multiple_of(2) {
        bail!("procedure table's first offset 0x{first:x} is not a non-zero even number");
    }
    let count = first / 2;
    let starts = (0..count)
        .map(|index| u16_at(index * 2))
        .collect::<Result<Vec<_>>>()?;
    for (index, pair) in starts.windows(2).enumerate() {
        if pair[1] < pair[0] {
            bail!(
                "procedure {} starts at 0x{:x}, before procedure {index} at 0x{:x}",
                index + 1,
                pair[1],
                pair[0]
            );
        }
    }
    if let Some(&last) = starts.last()
        && last > data.len()
    {
        bail!("procedure {} starts past the list's end", count - 1);
    }
    let mut procs = Vec::with_capacity(count);
    let mut len = first;
    for (index, &start) in starts.iter().enumerate() {
        let last = index + 1 == count;
        let end = if last { data.len() } else { starts[index + 1] };
        let procedure = decode_procedure(&data[start..end], last);
        len = start + procedure.insns.last().map_or(0, Insn::end) + procedure.trailing.len();
        procs.push(procedure);
    }
    Ok((ProcList { procs }, len))
}

/// Decode one procedure. The last procedure of a list has no stored end,
/// so it stops at the first `evt_end` past every block it has opened and
/// keeps no trailing bytes.
pub fn decode_procedure(data: &[u8], last: bool) -> Procedure {
    let mut insns = Vec::new();
    let mut pc = 0;
    // The furthest any block or jump seen so far reaches.
    let mut reach = 0;
    while pc < data.len() && insns.len() < budget::MAX_SCD_INSTRUCTIONS {
        let Some(op) = opcode::lookup(data[pc]) else {
            break;
        };
        let Some(bytes) = data.get(pc..pc + op.width) else {
            break;
        };
        let insn = Insn {
            offset: pc,
            bytes: bytes.to_vec(),
        };
        if let Some(target) = block_target(&insn) {
            reach = reach.max(target);
        }
        pc = insn.end();
        let ends = last && insn.bytes[0] == EVT_END && insn.offset >= reach;
        insns.push(insn);
        if ends {
            return Procedure {
                insns,
                trailing: Vec::new(),
            };
        }
    }
    Procedure {
        insns,
        trailing: if last {
            Vec::new()
        } else {
            data[pc..].to_vec()
        },
    }
}

/// Where a block or jump instruction's stored length or offset leads, as
/// an offset in the procedure (`None` for other instructions or a target
/// before the start).
pub fn block_target(insn: &Insn) -> Option<usize> {
    let op = insn.op();
    let operands = insn.operands();
    op.operands
        .bytes()
        .zip(operands)
        .find_map(|(kind, value)| match kind {
            b'L' => Some(insn.end() as i64 + value),
            b'J' => Some(insn.offset as i64 + value),
            _ => None,
        })
        .and_then(|target| usize::try_from(target).ok())
}
