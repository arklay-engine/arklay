//! RE2 `.s` assembly: one line per instruction with every operand field
//! written out, block lengths included, so it reassembles byte for byte.
//!
//! ```text
//! .version 1
//! .game re2
//!
//! .init
//! .proc
//!     if                      0, 8
//!     ck                      1, 25, 0
//!     set                     5, 3, 1
//!     endif                   0
//!     evt_end                 0
//! .main
//! .proc
//!     ...
//!     .db 0xED, 0x47
//! ```
//!
//! `.proc` starts the next procedure of the current list; `.db` gives a
//! procedure's trailing bytes and must come after its instructions. `;`
//! starts a comment.

use anyhow::{Context, Result, bail};

use crate::scd2::opcode;
use crate::scd2::{Insn, ProcList, Procedure, Scripts};

/// Column where mnemonics start.
const MNEMONIC_COLUMN: usize = 4;
/// Column where operands start.
const ARGS_COLUMN: usize = 28;
/// Trailing bytes per `.db` line.
const DB_CHUNK: usize = 16;

/// Render scripts as `.s` assembly.
pub fn render(scripts: &Scripts) -> String {
    let mut out = String::from(".version 1\n.game re2\n");
    for (name, list) in [("init", &scripts.init), ("main", &scripts.main)] {
        out.push_str(&format!("\n.{name}\n"));
        for procedure in &list.procs {
            out.push_str(".proc\n");
            for insn in &procedure.insns {
                out.push_str(&render_insn(insn));
                out.push('\n');
            }
            for chunk in procedure.trailing.chunks(DB_CHUNK) {
                out.push_str(&" ".repeat(MNEMONIC_COLUMN));
                out.push_str(".db ");
                out.push_str(&hex_list(chunk));
                out.push('\n');
            }
        }
    }
    out
}

fn hex_list(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("0x{byte:02X}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_insn(insn: &Insn) -> String {
    let mut line = " ".repeat(MNEMONIC_COLUMN);
    line.push_str(insn.op().mnemonic);
    let operands = insn.operands();
    if !operands.is_empty() {
        let pad = ARGS_COLUMN.saturating_sub(line.len()).max(1);
        line.push_str(&" ".repeat(pad));
        let args: Vec<String> = operands.iter().map(i64::to_string).collect();
        line.push_str(&args.join(", "));
    }
    line
}

/// Assemble `.s` source.
pub fn assemble(text: &str) -> Result<Scripts> {
    let mut scripts = Scripts::default();
    let mut list: Option<&mut ProcList> = None;
    let mut version = false;
    let mut game = false;
    for (index, raw) in text.lines().enumerate() {
        let line_no = index + 1;
        let line = raw.split(';').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }
        let (word, rest) = line
            .split_once(char::is_whitespace)
            .map_or((line, ""), |(word, rest)| (word, rest.trim()));
        match word {
            ".version" => {
                if rest != "1" {
                    bail!("line {line_no}: unsupported `.version {rest}`; expected 1");
                }
                version = true;
                continue;
            }
            ".game" => {
                if rest != "re2" {
                    bail!("line {line_no}: `.game {rest}` is not re2");
                }
                game = true;
                continue;
            }
            _ => {}
        }
        if !version || !game {
            bail!("line {line_no}: the source must start with `.version 1` and `.game re2`");
        }
        match word {
            ".init" => list = Some(&mut scripts.init),
            ".main" => list = Some(&mut scripts.main),
            ".proc" => list
                .as_deref_mut()
                .with_context(|| format!("line {line_no}: `.proc` before `.init` or `.main`"))?
                .procs
                .push(Procedure::default()),
            ".db" => {
                let procedure = current(&mut list, line_no)?;
                for piece in rest.split(',') {
                    let value = parse_int(piece.trim())
                        .filter(|value| (0..=0xFF).contains(value))
                        .with_context(|| {
                            format!("line {line_no}: `{}` is not a byte", piece.trim())
                        })?;
                    procedure.trailing.push(value as u8);
                }
            }
            mnemonic => {
                let op = opcode::by_mnemonic(mnemonic)
                    .with_context(|| format!("line {line_no}: unknown mnemonic `{mnemonic}`"))?;
                let values = if rest.is_empty() {
                    Vec::new()
                } else {
                    rest.split(',')
                        .map(|piece| {
                            parse_int(piece.trim()).with_context(|| {
                                format!("line {line_no}: `{}` is not a number", piece.trim())
                            })
                        })
                        .collect::<Result<Vec<_>>>()?
                };
                let bytes = opcode::encode(op, &values)
                    .map_err(|error| anyhow::anyhow!("line {line_no}: {error}"))?;
                let procedure = current(&mut list, line_no)?;
                if !procedure.trailing.is_empty() {
                    bail!("line {line_no}: an instruction after the procedure's `.db` bytes");
                }
                let offset = procedure.insns.last().map_or(0, Insn::end);
                procedure.insns.push(Insn { offset, bytes });
            }
        }
    }
    scripts.check()?;
    Ok(scripts)
}

fn current<'a>(list: &'a mut Option<&mut ProcList>, line_no: usize) -> Result<&'a mut Procedure> {
    list.as_deref_mut()
        .and_then(|list| list.procs.last_mut())
        .with_context(|| format!("line {line_no}: an instruction outside a `.proc`"))
}

/// Parse a decimal or `0x` hex integer, optionally negative.
pub(crate) fn parse_int(text: &str) -> Option<i64> {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    if digits.starts_with(['-', '+']) {
        return None;
    }
    let value = match digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        Some(hex) if !hex.starts_with(['-', '+']) => i64::from_str_radix(hex, 16).ok()?,
        Some(_) => return None,
        None => digits.parse().ok()?,
    };
    if negative {
        value.checked_neg()
    } else {
        Some(value)
    }
}
