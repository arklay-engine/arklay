//! `.akscr` compiler: source text to the SCD IR.
//!
//! The source is tokenised, parsed into a small statement tree and lowered to
//! the assembler's item stream (mnemonic plus operands, with generated labels
//! for the jumps), which [`asm::assemble_source`] encodes, lays out and
//! re-decodes. Errors name the source line and column.

use anyhow::{Result, bail};

use std::collections::{HashMap, HashSet};

use super::layout;
use super::preprocess::{Build, Loader, Region, Variant, no_includes, preprocess};
use crate::budget;
use crate::scd::asm::{self, Arg, Assembled, InsnSrc, Item, ItemKind, Source};
use crate::scd::disasm::{ACTOR_SIG, CONTROL_SIG, EVENT_TOP_SIG, TWEEN_SIG};

/// Compile `.akscr` source into the SCD IR: the runtime build, which tests
/// the room variant through the engine's status flags. The script must not use `#include`; see
/// [`compile_with`].
pub fn compile(text: &str) -> Result<Assembled> {
    compile_with(text, None, &no_includes)
}

/// Compile the `variant` of a merged `.akscr` script into the SCD IR: its
/// variant tests fold away, giving that RDT's own bytecode.
pub fn compile_variant(text: &str, variant: Variant) -> Result<Assembled> {
    compile_with(text, Some(variant), &no_includes)
}

/// Compile a script for `variant`, or the runtime build for every variant
/// when `None`, reading its `#include` files through `load`.
pub fn compile_with(text: &str, variant: Option<Variant>, load: Loader) -> Result<Assembled> {
    let build = variant.map_or(Build::Runtime, Build::Variant);
    compile_build(text, build, load)
}

/// Compile a script for `build`, reading its `#include` files through
/// `load`.
pub fn compile_build(text: &str, build: Build, load: Loader) -> Result<Assembled> {
    let preprocessed = preprocess(text, build, load)?;
    let Parsed { sections, procs } = parse(&preprocessed.text, &preprocessed.defines, build)?;
    check_recursion(&sections, &procs)?;
    relaxed(|relax| {
        let (source, pad_events) = lower_sections(&sections, build, &procs, relax)?;
        asm::assemble_source(&source, &pad_events)
    })
}

/// Lower and assemble with `attempt` until every jump fits: a command `if`
/// whose 1-byte jump does not reach takes the long form, and the script is
/// lowered again.
fn relaxed<T>(mut attempt: impl FnMut(&mut Relax) -> Result<T>) -> Result<T> {
    let mut long = HashSet::new();
    loop {
        let mut relax = Relax {
            long: std::mem::take(&mut long),
            ..Relax::default()
        };
        match attempt(&mut relax) {
            Ok(done) => return Ok(done),
            Err(error) => {
                let owner = short_jump_label(&format!("{error:#}"))
                    .and_then(|label| relax.owner.get(label))
                    .filter(|owner| !relax.long.contains(*owner))
                    .cloned();
                let Some(owner) = owner else {
                    return Err(error);
                };
                long = relax.long;
                long.insert(owner);
            }
        }
    }
}

/// The label of a 1-byte jump the assembler reports as too far.
fn short_jump_label(error: &str) -> Option<&str> {
    let (_, rest) = error.split_once("jump to `")?;
    let (label, rest) = rest.split_once('`')?;
    let (_, rest) = rest.split_once(" bytes and does not fit")?;
    (!rest.starts_with(" 16 bits")).then_some(label)
}

/// Lower every section of `build` to the assembler's streams, and say which
/// events take the padding.
fn lower_sections(
    sections: &[Section],
    build: Build,
    procs: &HashMap<String, u8>,
    relax: &mut Relax,
) -> Result<(Source, Vec<bool>)> {
    let mut source = Source::default();
    let mut pad_events = Vec::new();
    let mut init_done = false;
    let mut main_done = false;
    for section in sections {
        // A section another RDT variant carries is not in this one's
        // streams; the runtime build carries every section.
        if let Some(variant) = build.variant()
            && !variant.matches(section.when)
        {
            continue;
        }
        let items = lower_section(section, build, procs, relax)?;
        match section.kind {
            // Procedures may stand anywhere; they are numbered in order.
            SectionKind::Proc(_) => source.procs.push(items),
            SectionKind::Init => {
                if init_done {
                    bail!(
                        "line {}: `proc init` must come before `proc main` and the events",
                        section.line
                    );
                }
                source.init.push(items);
            }
            SectionKind::Main => {
                if main_done {
                    bail!(
                        "line {}: `proc main` must come before the events",
                        section.line
                    );
                }
                init_done = true;
                source.main.push(items);
            }
            SectionKind::Event(index) => {
                init_done = true;
                main_done = true;
                if usize::from(index) != source.events.len() {
                    bail!(
                        "line {}: expected `event event_{:02X}`, events must be dense and \
                         ascending from event_00",
                        section.line,
                        source.events.len()
                    );
                }
                source.events.push(items);
                pad_events.push(!holds(section.nopad, build));
            }
        }
    }
    Ok((source, pad_events))
}

/// Compile a single section (with or without the `#version` line), naming
/// with `defines`, and return the bytes of its block or event stream. The
/// script's procedures are `procs`, by number: a call to one compiles to its
/// `gosub`, and a section that is one of them is that procedure.
pub(super) fn section_bytes(
    text: &str,
    defines: &HashMap<String, i64>,
    procs: &[String],
) -> Result<Vec<u8>> {
    let build = Build::Variant(Variant {
        jill: false,
        mansion: None,
        // The decompiler checks one RDT's section, which holds no region.
        region: Region::Usa,
    });
    let text = text.trim_start().strip_prefix("#version 1").unwrap_or(text);
    // Empty procedures stand for the others, in their places, so that the
    // procedures keep their numbers.
    let own = procs.iter().position(|name| {
        text.trim_start()
            .strip_prefix("proc ")
            .and_then(|rest| rest.strip_prefix(name.as_str()))
            .is_some_and(|rest| {
                !rest.starts_with(|ch: char| ch.is_ascii_alphanumeric() || ch == '_')
            })
    });
    let mut script = String::from("#version 1\n");
    for (index, name) in procs.iter().enumerate() {
        if Some(index) == own {
            script.push_str(text);
            script.push('\n');
        } else {
            script.push_str(&format!("proc {name}\n{{\n}}\n"));
        }
    }
    if own.is_none() {
        script.push_str(text);
    }
    let Parsed { sections, procs } = parse(&script, defines, build)?;
    let position = own.unwrap_or(procs.len());
    let (Some(section), true) = (
        sections.get(position),
        sections.len() == procs.len() + usize::from(own.is_none()),
    ) else {
        bail!("expected exactly one section");
    };
    let event = matches!(section.kind, SectionKind::Event(_));
    relaxed(|relax| {
        let items = lower_section(section, build, &procs, relax)?;
        asm::assemble_stream(&items, event, !holds(section.nopad, build))
    })
}

/// Whether a section flag held for the variants of `mask` (`None` when the
/// section does not carry it) holds in `build`. The runtime build takes a
/// flag only when every variant holds it.
fn holds(flag: Option<u8>, build: Build) -> bool {
    match (flag, build.variant()) {
        (None, _) => false,
        (Some(mask), Some(variant)) => variant.matches(mask),
        (Some(mask), None) => mask == 0,
    }
}

/// The condition names a variant test may use, with their mask bits.
const VARIANT_ATOMS: [(&str, u8); 7] = {
    use crate::scd::opcode::variant::*;
    [
        ("CHRIS", CHRIS),
        ("JILL", JILL),
        ("MANSION1", MANSION1),
        ("MANSION2", MANSION2),
        ("USA", USA),
        ("JPN", JPN),
        ("EUR", EUR),
    ]
};

/// The mask bit of variant condition name `name`.
fn variant_atom(name: &str) -> Option<u8> {
    VARIANT_ATOMS
        .iter()
        .find(|(atom, _)| *atom == name)
        .map(|(_, bit)| *bit)
}

/// The flag tests that hold for the room variants of `mask` at run time.
/// The engine keeps a status bit for the player and the release, and the
/// scenario flag for the mansion visit, in step with the room
/// ([`crate::game::GameState::sync_variant_flags`]). Outside rooms leave the
/// visit flag alone, so a visit test means nothing there.
///
/// The second list, when there is one, must not all pass as well: no flag
/// says `!USA` (the Japanese or a European release), but it is where the
/// USA test, both release bits clear, fails.
fn variant_tests(mask: u8, line: usize, col: usize) -> (Vec<Call>, Option<Vec<Call>>) {
    use crate::scd::opcode::variant::*;
    let ck = |group: u8, flag: u8, set: bool| Call {
        name: "ck".to_string(),
        args: vec![
            Arg::Num(i64::from(group)),
            Arg::Num(i64::from(flag)),
            // `ck` passes on a set flag for 0 (`TRUE`).
            Arg::Num(if set { 0 } else { 1 }),
        ],
        line,
        col,
    };
    let mut tests = Vec::new();
    match mask & PLAYER {
        CHRIS => tests.push(ck(FLAG_BANK_STATUS, FLAG_JILL, false)),
        JILL => tests.push(ck(FLAG_BANK_STATUS, FLAG_JILL, true)),
        _ => {}
    }
    match mask & (MANSION1 | MANSION2) {
        MANSION1 => tests.push(ck(FLAG_BANK_SCENARIO, FLAG_MANSION2, false)),
        MANSION2 => tests.push(ck(FLAG_BANK_SCENARIO, FLAG_MANSION2, true)),
        _ => {}
    }
    let usa = || {
        vec![
            ck(FLAG_BANK_STATUS, FLAG_REGION_JPN, false),
            ck(FLAG_BANK_STATUS, FLAG_REGION_EUR, false),
        ]
    };
    let region = mask & REGION;
    if region == JPN {
        tests.push(ck(FLAG_BANK_STATUS, FLAG_REGION_JPN, true));
    } else if region == EUR {
        tests.push(ck(FLAG_BANK_STATUS, FLAG_REGION_EUR, true));
    } else if region == JPN | EUR {
        return (tests, Some(usa()));
    } else if region == USA {
        tests.extend(usa());
    } else if region == USA | JPN {
        tests.push(ck(FLAG_BANK_STATUS, FLAG_REGION_EUR, false));
    } else if region == USA | EUR {
        tests.push(ck(FLAG_BANK_STATUS, FLAG_REGION_JPN, false));
    }
    (tests, None)
}

/// Parse a variant condition from `tokens` (`JILL`, `!JPN`, `CHRIS &&
/// MANSION2`), up to the closing `)`, into a mask
/// ([`crate::scd::opcode::variant_matches`]); returns the mask and the
/// tokens used.
fn variant_condition(tokens: &[Token]) -> Result<(u8, usize)> {
    use crate::scd::opcode::variant::DIMENSIONS;
    // The allowed values of each dimension, narrowed by every term.
    let mut allowed = DIMENSIONS;
    let mut pos = 0usize;
    loop {
        let negated = matches!(tokens.get(pos).map(|t| &t.tok), Some(Tok::Punct("!")));
        if negated {
            pos += 1;
        }
        let Some(token) = tokens.get(pos) else {
            bail!("the variant condition is not closed");
        };
        let bit = match &token.tok {
            Tok::Ident(name) => variant_atom(name),
            _ => None,
        };
        let Some(bit) = bit else {
            bail!(
                "line {}, column {}: expected a variant (one of {}), found {}; a variant test \
                 cannot mix with other conditions",
                token.line,
                token.col,
                VARIANT_ATOMS.map(|(atom, _)| atom).join(", "),
                describe(&token.tok)
            );
        };
        pos += 1;
        for dimension in allowed.iter_mut().zip(DIMENSIONS) {
            let (allowed, all) = dimension;
            if all & bit != 0 {
                *allowed &= if negated { all & !bit } else { bit };
                if *allowed == 0 {
                    bail!(
                        "line {}, column {}: this variant condition never holds",
                        token.line,
                        token.col
                    );
                }
            }
        }
        match tokens.get(pos).map(|token| &token.tok) {
            Some(Tok::Punct("&&")) => pos += 1,
            _ => break,
        }
    }
    let mask = allowed
        .iter()
        .zip(DIMENSIONS)
        .filter(|(allowed, all)| *allowed != all)
        .fold(0u8, |mask, (allowed, _)| mask | allowed);
    Ok((mask, pos))
}

/// Whether the tokens at the start of an `if` condition are a variant test.
fn starts_variant_condition(tokens: &[Token]) -> bool {
    match tokens.first().map(|token| &token.tok) {
        Some(Tok::Punct("!")) => true,
        Some(Tok::Ident(name)) => {
            variant_atom(name).is_some()
                && !matches!(tokens.get(1).map(|t| &t.tok), Some(Tok::Punct("(")))
        }
        _ => false,
    }
}

/// A token as an error message names it.
fn describe(tok: &Tok) -> String {
    match tok {
        Tok::Ident(name) => format!("`{name}`"),
        Tok::Num(value) => format!("`{value}`"),
        Tok::Punct(punct) => format!("`{punct}`"),
        Tok::Asm(_) => "`asm`".to_string(),
        Tok::Eof => "end of file".to_string(),
    }
}

/// The `when (...)` mask of a section header starting at `tokens` (the
/// tokens after the section's name, up to its body); 0 when it has none.
fn header_when(tokens: &[Token]) -> Result<u8> {
    let mut pos = 0usize;
    while let Some(token) = tokens.get(pos) {
        match &token.tok {
            Tok::Punct("{") | Tok::Eof => break,
            Tok::Ident(word) if word == "when" => {
                if !matches!(tokens.get(pos + 1).map(|t| &t.tok), Some(Tok::Punct("("))) {
                    bail!(
                        "line {}, column {}: expected `(` after `when`",
                        token.line,
                        token.col
                    );
                }
                return Ok(variant_condition(&tokens[pos + 2..])?.0);
            }
            _ => pos += 1,
        }
    }
    Ok(0)
}

// ---------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Ident(String),
    Num(i64),
    Punct(&'static str),
    /// The raw lines of an `asm { ... }` block, with their line numbers.
    Asm(Vec<(usize, String)>),
    Eof,
}

#[derive(Debug, Clone)]
struct Token {
    tok: Tok,
    line: usize,
    col: usize,
}

const PUNCTUATION: [&str; 11] = ["&&", "(", ")", "{", "}", ",", ";", "|", "-", "#", "!"];

fn lex(text: &str) -> Result<Vec<Token>> {
    let lines: Vec<&str> = text.lines().collect();
    budget::check_len(lines.len(), budget::MAX_ASM_LINES, "script line count")?;
    let mut tokens = Vec::new();
    let mut row = 0usize;
    let mut col = 0usize;
    while row < lines.len() {
        let line = lines[row];
        let bytes = line.as_bytes();
        if col >= bytes.len() {
            row += 1;
            col = 0;
            continue;
        }
        let ch = bytes[col];
        if ch.is_ascii_whitespace() {
            col += 1;
            continue;
        }
        if line[col..].starts_with("//") {
            row += 1;
            col = 0;
            continue;
        }
        let (line_no, column) = (row + 1, col + 1);
        if ch.is_ascii_alphabetic() || ch == b'_' {
            let end = line[col..]
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .map_or(bytes.len(), |offset| col + offset);
            let word = &line[col..end];
            col = end;
            if word == "asm" {
                let (block, next_row) = asm_block(&lines, row, col, line_no, column)?;
                tokens.push(Token {
                    tok: Tok::Asm(block),
                    line: line_no,
                    col: column,
                });
                row = next_row;
                col = 0;
                continue;
            }
            tokens.push(Token {
                tok: Tok::Ident(word.to_string()),
                line: line_no,
                col: column,
            });
            continue;
        }
        if ch.is_ascii_digit() {
            let end = line[col..]
                .find(|c: char| !c.is_ascii_alphanumeric())
                .map_or(bytes.len(), |offset| col + offset);
            let word = &line[col..end];
            let value = match word.strip_prefix("0x").or_else(|| word.strip_prefix("0X")) {
                Some(hex) => i64::from_str_radix(hex, 16).ok(),
                None => word.parse::<i64>().ok(),
            };
            let Some(value) = value else {
                bail!("line {line_no}, column {column}: invalid number `{word}`");
            };
            tokens.push(Token {
                tok: Tok::Num(value),
                line: line_no,
                col: column,
            });
            col = end;
            continue;
        }
        let Some(punct) = PUNCTUATION
            .iter()
            .find(|punct| line[col..].starts_with(**punct))
        else {
            bail!(
                "line {line_no}, column {column}: unexpected character `{}`",
                line[col..].chars().next().unwrap_or(' ')
            );
        };
        tokens.push(Token {
            tok: Tok::Punct(punct),
            line: line_no,
            col: column,
        });
        col += punct.len();
    }
    tokens.push(Token {
        tok: Tok::Eof,
        line: lines.len() + 1,
        col: 1,
    });
    Ok(tokens)
}

/// Capture the raw lines of an `asm` block: `{` (on the keyword's line or the
/// next one) ends its line, and a line holding only `}` closes the block.
/// Returns the lines and the row after the closing brace.
fn asm_block(
    lines: &[&str],
    row: usize,
    col: usize,
    line_no: usize,
    column: usize,
) -> Result<(Vec<(usize, String)>, usize)> {
    let mut row = row;
    let mut rest = lines[row][col..].trim();
    if rest.is_empty() {
        row += 1;
        rest = lines.get(row).map_or("", |line| line.trim());
    }
    if rest != "{" {
        bail!("line {line_no}, column {column}: `asm` must be followed by `{{` on its own line");
    }
    let mut block = Vec::new();
    row += 1;
    while let Some(line) = lines.get(row) {
        if line.trim() == "}" {
            return Ok((block, row + 1));
        }
        block.push((row + 1, line.to_string()));
        row += 1;
    }
    bail!("line {line_no}, column {column}: `asm` block is not closed")
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SectionKind {
    Init,
    Main,
    /// A user procedure (`proc NAME`), which `NAME();` calls: `gosub N`.
    Proc(u8),
    Event(u8),
}

#[derive(Debug)]
struct Section {
    kind: SectionKind,
    /// A block without the implicit final `end 0`, for the variants of the
    /// mask (`noreturn` alone is mask 0, every variant).
    noreturn: Option<u8>,
    /// An event without the four-byte padding after its final `evt_finish`,
    /// for the variants of the mask.
    nopad: Option<u8>,
    /// The variants whose RDT carries the section (`when (...)`); 0 for
    /// every variant.
    when: u8,
    body: Vec<Stmt>,
    line: usize,
}

#[derive(Debug, Clone)]
struct Call {
    name: String,
    args: Vec<Arg>,
    line: usize,
    col: usize,
}

#[derive(Debug)]
enum Stmt {
    Call(Call),
    Return {
        line: usize,
        col: usize,
    },
    Require(Call),
    If {
        conditions: Vec<Call>,
        then: Vec<Stmt>,
        otherwise: Option<Vec<Stmt>>,
        line: usize,
        col: usize,
    },
    /// `if (JILL) { ... } else { ... }`: a test of the room variant, which
    /// tests the engine's flags at run time ([`variant_tests`]) and folds
    /// away in a variant's build.
    Variant {
        mask: u8,
        then: Vec<Stmt>,
        otherwise: Option<Vec<Stmt>>,
        line: usize,
        col: usize,
    },
    Asm(Vec<(usize, String)>),
    Run {
        noreturn: bool,
        body: Vec<Stmt>,
        line: usize,
        col: usize,
    },
    For {
        count: i64,
        body: Vec<Stmt>,
        line: usize,
        col: usize,
    },
    DoWhile {
        body: Vec<Stmt>,
        condition: Call,
        line: usize,
        col: usize,
    },
    Single {
        payload: i64,
        call: Call,
    },
    Raw {
        bytes: Vec<u8>,
        line: usize,
        col: usize,
    },
}

impl Stmt {
    fn position(&self) -> (usize, usize) {
        match self {
            Stmt::Call(call) | Stmt::Require(call) | Stmt::Single { call, .. } => {
                (call.line, call.col)
            }
            Stmt::Return { line, col }
            | Stmt::If { line, col, .. }
            | Stmt::Variant { line, col, .. }
            | Stmt::Run { line, col, .. }
            | Stmt::For { line, col, .. }
            | Stmt::DoWhile { line, col, .. }
            | Stmt::Raw { line, col, .. } => (*line, *col),
            Stmt::Asm(lines) => (lines.first().map_or(0, |(line, _)| *line), 1),
        }
    }
}

/// Names the compiler itself knows: the engine's flag groups, room action
/// types, work variables and door lock states, and `event_XX`.
fn is_engine_name(name: &str) -> bool {
    asm::is_engine_constant(name)
        || name
            .strip_prefix("event_")
            .is_some_and(|hex| hex.len() == 2 && u8::from_str_radix(hex, 16).is_ok())
}

/// Replace each defined name in an `asm` line with its value.
fn substitute(line: &str, defines: &HashMap<String, i64>) -> String {
    let mut out = String::with_capacity(line.len());
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        match defines.get(word.as_str()) {
            Some(value) => out.push_str(&value.to_string()),
            None => out.push_str(word),
        }
        word.clear();
    };
    for ch in line.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            word.push(ch);
        } else {
            flush(&mut word, &mut out);
            out.push(ch);
        }
    }
    flush(&mut word, &mut out);
    out
}

/// The index an event a variant's build leaves out stands for: no event,
/// so a reference that survives the fold fails to encode.
const ABSENT_EVENT: i64 = -1;

/// A parsed script: its sections and the user procedures' numbers by name.
struct Parsed {
    sections: Vec<Section>,
    procs: HashMap<String, u8>,
}

fn parse(text: &str, defines: &HashMap<String, i64>, build: Build) -> Result<Parsed> {
    budget::check_len(text.len(), budget::MAX_TEXT_BYTES, "script size")?;
    let tokens = lex(text)?;
    let mut defines = defines.clone();
    for (name, value) in super::engine::engine_names() {
        defines.entry(name.clone()).or_insert(*value);
    }
    // Event names stand for their index, so any event may name any other.
    // A variant's build numbers only the events its RDT carries; the
    // runtime build numbers every event in order.
    // A user procedure is numbered in the order it stands in.
    let mut depth = 0usize;
    let mut events = 0i64;
    let mut procs = HashMap::new();
    for (pos, token) in tokens.iter().enumerate() {
        match &token.tok {
            Tok::Punct("{") => depth += 1,
            Tok::Punct("}") => depth = depth.saturating_sub(1),
            Tok::Ident(word) if depth == 0 && word == "proc" => {
                let Some(Token {
                    tok: Tok::Ident(name),
                    line,
                    ..
                }) = tokens.get(pos + 1)
                else {
                    continue;
                };
                if name == "init" || name == "main" {
                    continue;
                }
                if defines.contains_key(name) || is_engine_name(name) {
                    bail!("line {line}: procedure name `{name}` is already defined");
                }
                let Ok(index) = u8::try_from(procs.len()) else {
                    bail!("line {line}: a script has at most 256 procedures");
                };
                if procs.insert(name.clone(), index).is_some() {
                    bail!("line {line}: procedure `{name}` is already defined");
                }
            }
            Tok::Ident(word) if depth == 0 && word == "event" => {
                let Some(Token {
                    tok: Tok::Ident(name),
                    line,
                    ..
                }) = tokens.get(pos + 1)
                else {
                    continue;
                };
                let when = header_when(&tokens[pos + 2..])?;
                let present = build.variant().is_none_or(|variant| variant.matches(when));
                // In a variant's build a generic `event_XX` is index XX
                // wherever it stands.
                let generic = build
                    .variant()
                    .and(name.strip_prefix("event_"))
                    .filter(|hex| hex.len() == 2)
                    .and_then(|hex| i64::from_str_radix(hex, 16).ok());
                let index = if present {
                    generic.unwrap_or(events)
                } else {
                    ABSENT_EVENT
                };
                if defines.insert(name.clone(), index).is_some() || procs.contains_key(name) {
                    bail!("line {line}: event name `{name}` is already defined");
                }
                if present {
                    events += 1;
                }
            }
            _ => {}
        }
    }
    let mut parser = Parser {
        tokens,
        pos: 0,
        defines,
        procs,
    };
    parser.version()?;
    let mut sections = Vec::new();
    while parser.peek().tok != Tok::Eof {
        sections.push(parser.section()?);
    }
    Ok(Parsed {
        sections,
        procs: parser.procs,
    })
}

/// Reject a procedure that calls itself, directly or through others: the
/// calls nest without end.
fn check_recursion(sections: &[Section], procs: &HashMap<String, u8>) -> Result<()> {
    fn calls(body: &[Stmt], procs: &HashMap<String, u8>, out: &mut Vec<(u8, usize)>) {
        for stmt in body {
            match stmt {
                Stmt::Call(call) | Stmt::Single { call, .. } => {
                    if let Some(&index) = procs.get(&call.name) {
                        out.push((index, call.line));
                    }
                }
                Stmt::If {
                    then, otherwise, ..
                }
                | Stmt::Variant {
                    then, otherwise, ..
                } => {
                    calls(then, procs, out);
                    calls(otherwise.as_deref().unwrap_or_default(), procs, out);
                }
                Stmt::Run { body, .. } | Stmt::For { body, .. } | Stmt::DoWhile { body, .. } => {
                    calls(body, procs, out);
                }
                Stmt::Return { .. } | Stmt::Require(_) | Stmt::Asm(_) | Stmt::Raw { .. } => {}
            }
        }
    }
    let mut graph: HashMap<u8, Vec<(u8, usize)>> = HashMap::new();
    for section in sections {
        if let SectionKind::Proc(index) = section.kind {
            let mut out = Vec::new();
            calls(&section.body, procs, &mut out);
            graph.insert(index, out);
        }
    }
    let name = |index: u8| {
        procs
            .iter()
            .find(|(_, value)| **value == index)
            .map_or("?", |(name, _)| name.as_str())
    };
    // Depth-first from every procedure, keeping the path to report.
    fn visit(
        index: u8,
        graph: &HashMap<u8, Vec<(u8, usize)>>,
        path: &mut Vec<u8>,
        done: &mut Vec<u8>,
    ) -> Option<(Vec<u8>, usize)> {
        if done.contains(&index) {
            return None;
        }
        path.push(index);
        for &(callee, line) in graph.get(&index).map_or(&[][..], Vec::as_slice) {
            if path.contains(&callee) {
                return Some((path.clone(), line));
            }
            if let Some(found) = visit(callee, graph, path, done) {
                return Some(found);
            }
        }
        path.pop();
        done.push(index);
        None
    }
    let mut done = Vec::new();
    let mut indices: Vec<u8> = graph.keys().copied().collect();
    indices.sort_unstable();
    for index in indices {
        if let Some((path, line)) = visit(index, &graph, &mut Vec::new(), &mut done) {
            let chain: Vec<&str> = path.iter().map(|&index| name(index)).collect();
            bail!(
                "line {line}: procedure `{}` calls itself ({} -> {})",
                chain[0],
                chain.join(" -> "),
                chain[0]
            );
        }
    }
    Ok(())
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    /// `#define`s, the builtins and the event names.
    defines: HashMap<String, i64>,
    /// The user procedures' numbers, by name.
    procs: HashMap<String, u8>,
}

impl Parser {
    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn next(&mut self) -> Token {
        let token = self.tokens[self.pos].clone();
        if token.tok != Tok::Eof {
            self.pos += 1;
        }
        token
    }

    fn error<T>(&self, token: &Token, expected: &str) -> Result<T> {
        let found = match &token.tok {
            Tok::Ident(name) => format!("`{name}`"),
            Tok::Num(value) => format!("`{value}`"),
            Tok::Punct(punct) => format!("`{punct}`"),
            Tok::Asm(_) => "`asm`".to_string(),
            Tok::Eof => "end of file".to_string(),
        };
        bail!(
            "line {}, column {}: expected {expected}, found {found}",
            token.line,
            token.col
        )
    }

    fn eat_punct(&mut self, punct: &str) -> bool {
        if matches!(&self.peek().tok, Tok::Punct(found) if *found == punct) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect_punct(&mut self, punct: &str) -> Result<Token> {
        let token = self.next();
        match &token.tok {
            Tok::Punct(found) if *found == punct => Ok(token),
            _ => self.error(&token, &format!("`{punct}`")),
        }
    }

    fn eat_ident(&mut self, word: &str) -> bool {
        if matches!(&self.peek().tok, Tok::Ident(found) if found == word) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect_ident(&mut self) -> Result<(String, Token)> {
        let token = self.next();
        match &token.tok {
            Tok::Ident(name) => Ok((name.clone(), token)),
            _ => self.error(&token, "a name"),
        }
    }

    fn expect_int(&mut self) -> Result<i64> {
        let negative = self.eat_punct("-");
        let token = self.next();
        let value = match &token.tok {
            Tok::Num(value) => *value,
            Tok::Ident(name) if self.defines.contains_key(name) => self.defines[name],
            _ => return self.error(&token, "a number"),
        };
        Ok(if negative { -value } else { value })
    }

    fn version(&mut self) -> Result<()> {
        let token = self.peek().clone();
        if !self.eat_punct("#") || !self.eat_ident("version") {
            bail!(
                "line {}, column {}: the script must start with `#version 1`",
                token.line,
                token.col
            );
        }
        let version = self.expect_int()?;
        if version != 1 {
            bail!(
                "line {}, column {}: unsupported script version {version}",
                token.line,
                token.col
            );
        }
        Ok(())
    }

    fn section(&mut self) -> Result<Section> {
        let (word, token) = self.expect_ident()?;
        let kind = match word.as_str() {
            "proc" => {
                let (name, name_token) = self.expect_ident()?;
                match name.as_str() {
                    "init" => SectionKind::Init,
                    "main" => SectionKind::Main,
                    _ => match self.procs.get(&name) {
                        Some(&index) => SectionKind::Proc(index),
                        None => return self.error(&name_token, "a procedure name"),
                    },
                }
            }
            "event" => {
                let (name, name_token) = self.expect_ident()?;
                match self.defines[&name] {
                    // Left out of this variant's build: skipped unlowered.
                    ABSENT_EVENT => SectionKind::Event(u8::MAX),
                    index => match u8::try_from(index) {
                        Ok(index) => SectionKind::Event(index),
                        Err(_) => return self.error(&name_token, "at most 256 events"),
                    },
                }
            }
            _ => return self.error(&token, "`proc` or `event`"),
        };
        let event = matches!(kind, SectionKind::Event(_));
        let mut noreturn = None;
        let mut nopad = None;
        let mut when = 0;
        loop {
            if !event && self.eat_ident("noreturn") {
                noreturn = Some(self.optional_condition()?);
            } else if event && self.eat_ident("nopad") {
                nopad = Some(self.optional_condition()?);
            } else if self.eat_ident("when") {
                self.expect_punct("(")?;
                when = self.condition_mask()?;
            } else {
                break;
            }
        }
        if matches!(kind, SectionKind::Proc(_)) && (noreturn.is_some() || when != 0) {
            bail!(
                "line {}: a procedure takes no `noreturn` or `when`; test the variant with `if` \
                 inside it",
                token.line
            );
        }
        let body = self.block()?;
        Ok(Section {
            kind,
            noreturn,
            nopad,
            when,
            body,
            line: token.line,
        })
    }

    /// A section flag's optional `(variants)`: the mask it holds for, 0
    /// (every variant) without one.
    fn optional_condition(&mut self) -> Result<u8> {
        if self.eat_punct("(") {
            self.condition_mask()
        } else {
            Ok(0)
        }
    }

    /// A variant condition after its `(`, through the closing `)`.
    fn condition_mask(&mut self) -> Result<u8> {
        let (mask, used) = variant_condition(&self.tokens[self.pos..])?;
        self.pos += used;
        self.expect_punct(")")?;
        Ok(mask)
    }

    fn block(&mut self) -> Result<Vec<Stmt>> {
        self.expect_punct("{")?;
        let mut body = Vec::new();
        while !self.eat_punct("}") {
            budget::check_len(body.len() + 1, budget::MAX_ASM_LINES, "statement count")?;
            body.push(self.statement()?);
        }
        Ok(body)
    }

    fn statement(&mut self) -> Result<Stmt> {
        let token = self.next();
        let (line, col) = (token.line, token.col);
        let word = match &token.tok {
            Tok::Asm(lines) => {
                let lines = lines
                    .iter()
                    .map(|(line_no, line)| (*line_no, substitute(line, &self.defines)))
                    .collect();
                return Ok(Stmt::Asm(lines));
            }
            Tok::Ident(word) => word.clone(),
            _ => return self.error(&token, "a statement"),
        };
        match word.as_str() {
            "return" => {
                self.expect_punct(";")?;
                Ok(Stmt::Return { line, col })
            }
            "if" => self.if_statement(line, col),
            "require" => {
                self.expect_punct("(")?;
                let call = self.call()?;
                self.expect_punct(")")?;
                self.expect_punct(";")?;
                Ok(Stmt::Require(call))
            }
            "run" => {
                let noreturn = self.eat_ident("noreturn");
                let body = self.block()?;
                Ok(Stmt::Run {
                    noreturn,
                    body,
                    line,
                    col,
                })
            }
            "for" => {
                self.expect_punct("(")?;
                let count = self.expect_int()?;
                self.expect_punct(")")?;
                let body = self.block()?;
                Ok(Stmt::For {
                    count,
                    body,
                    line,
                    col,
                })
            }
            "do" => {
                let body = self.block()?;
                if !self.eat_ident("while") {
                    let next = self.peek().clone();
                    return self.error(&next, "`while`");
                }
                self.expect_punct("(")?;
                let condition = self.call()?;
                self.expect_punct(")")?;
                self.expect_punct(";")?;
                Ok(Stmt::DoWhile {
                    body,
                    condition,
                    line,
                    col,
                })
            }
            "single" => {
                self.expect_punct("(")?;
                let payload = self.expect_int()?;
                self.expect_punct(")")?;
                let call = self.call()?;
                self.expect_punct(";")?;
                Ok(Stmt::Single { payload, call })
            }
            "raw" => {
                self.expect_punct("(")?;
                let mut bytes = Vec::new();
                loop {
                    let value = self.expect_int()?;
                    let Ok(byte) = u8::try_from(value) else {
                        bail!("line {line}, column {col}: raw byte {value} is out of range");
                    };
                    bytes.push(byte);
                    if !self.eat_punct(",") {
                        break;
                    }
                }
                self.expect_punct(")")?;
                self.expect_punct(";")?;
                Ok(Stmt::Raw { bytes, line, col })
            }
            _ => {
                self.pos -= 1;
                let call = self.call()?;
                self.expect_punct(";")?;
                Ok(Stmt::Call(call))
            }
        }
    }

    fn if_statement(&mut self, line: usize, col: usize) -> Result<Stmt> {
        self.expect_punct("(")?;
        if starts_variant_condition(&self.tokens[self.pos..]) {
            let mask = self.condition_mask()?;
            let then = self.block()?;
            let otherwise = self.else_branch()?;
            return Ok(Stmt::Variant {
                mask,
                then,
                otherwise,
                line,
                col,
            });
        }
        let mut conditions = vec![self.call()?];
        while self.eat_punct("&&") {
            conditions.push(self.call()?);
        }
        self.expect_punct(")")?;
        let then = self.block()?;
        let otherwise = self.else_branch()?;
        Ok(Stmt::If {
            conditions,
            then,
            otherwise,
            line,
            col,
        })
    }

    /// An `if`'s optional `else { ... }` or `else if ...`.
    fn else_branch(&mut self) -> Result<Option<Vec<Stmt>>> {
        if !self.eat_ident("else") {
            return Ok(None);
        }
        let next = self.peek().clone();
        if self.eat_ident("if") {
            Ok(Some(vec![self.if_statement(next.line, next.col)?]))
        } else {
            Ok(Some(self.block()?))
        }
    }

    /// `name(arg, ...)`; an argument is a number, a constant name or a
    /// `hi | lo` bit field.
    fn call(&mut self) -> Result<Call> {
        let (name, token) = self.expect_ident()?;
        self.expect_punct("(")?;
        let mut args = Vec::new();
        if !self.eat_punct(")") {
            loop {
                args.push(self.argument()?);
                if self.eat_punct(")") {
                    break;
                }
                self.expect_punct(",")?;
            }
        }
        Ok(Call {
            name,
            args,
            line: token.line,
            col: token.col,
        })
    }

    fn argument(&mut self) -> Result<Arg> {
        let token = self.peek().clone();
        if let Tok::Ident(name) = &token.tok
            && !self.defines.contains_key(name)
        {
            self.pos += 1;
            if !is_engine_name(name) {
                bail!(
                    "line {}, column {}: unknown name `{name}` (is an `#include` missing?)",
                    token.line,
                    token.col
                );
            }
            return Ok(Arg::Symbol(name.clone()));
        }
        let value = self.expect_int()?;
        if !self.eat_punct("|") {
            return Ok(Arg::Num(value));
        }
        // `hi | lo`, or any longer run of bits ORed together.
        let low = self.expect_int()?;
        let mut rest = 0;
        let mut more = false;
        while self.eat_punct("|") {
            rest |= self.expect_int()?;
            more = true;
        }
        Ok(if more {
            Arg::Num(value | low | rest)
        } else {
            Arg::Bits(value, low)
        })
    }
}

// ---------------------------------------------------------------------------
// Lowering
// ---------------------------------------------------------------------------

/// What lowering a script carries from section to section, and from one
/// attempt to the next.
#[derive(Default)]
struct Relax {
    /// The command `if`s, by skip label, that take the long form.
    long: HashSet<String>,
    /// The command `if` each jump label belongs to, by its skip label.
    owner: HashMap<String, String>,
    /// The last label number used; labels are unique across the sections.
    next_label: usize,
}

/// Lower one section to assembler items.
fn lower_section(
    section: &Section,
    build: Build,
    procs: &HashMap<String, u8>,
    relax: &mut Relax,
) -> Result<Vec<Item>> {
    let mut lowerer = Lowerer {
        build,
        procs,
        items: Vec::new(),
        pending: Vec::new(),
        relax,
    };
    match section.kind {
        SectionKind::Proc(_) => lowerer.command_block(&section.body, false, section.line)?,
        SectionKind::Init | SectionKind::Main if build.variant().is_some() => {
            let noreturn = holds(section.noreturn, build);
            lowerer.command_block(&section.body, noreturn, section.line)?;
        }
        // The runtime build carries every variant's blocks: the variants
        // whose RDT lacks this one end it at once.
        SectionKind::Init | SectionKind::Main => {
            let (line, col) = (section.line, 1);
            let mut end = |lowerer: &mut Lowerer| {
                lowerer.push("end", vec![Arg::Num(0)], line, col);
                Ok(())
            };
            let mut body = |lowerer: &mut Lowerer| {
                lowerer.commands(&section.body)?;
                // A `noreturn` block runs off its end, which ends the block
                // list too; the other variants end it plainly.
                match section.noreturn {
                    None => {
                        lowerer.push("end", vec![Arg::Num(0)], line, col);
                        Ok(())
                    }
                    Some(0) => Ok(()),
                    Some(mask) => {
                        let mut end = |lowerer: &mut Lowerer| {
                            lowerer.push("end", vec![Arg::Num(0)], line, col);
                            Ok(())
                        };
                        lowerer.variant_if(mask, &mut |_| Ok(()), Some(&mut end), false, line, col)
                    }
                }
            };
            lowerer.variant_if(section.when, &mut body, Some(&mut end), false, line, col)?;
        }
        SectionKind::Event(_) => lowerer.events(&section.body)?,
    }
    if !lowerer.pending.is_empty() {
        // A jump past the last statement lands on the end of the stream.
        lowerer.items.push(Item {
            labels: std::mem::take(&mut lowerer.pending),
            kind: ItemKind::Db(Vec::new()),
            line: section.line,
            col: 1,
        });
    }
    Ok(lowerer.items)
}

struct Lowerer<'a> {
    build: Build,
    /// The user procedures' numbers, by name.
    procs: &'a HashMap<String, u8>,
    items: Vec<Item>,
    /// Labels for the next item.
    pending: Vec<String>,
    relax: &'a mut Relax,
}

/// A part of an `if` statement to lower.
type Branch<'b> = &'b mut dyn FnMut(&mut Lowerer) -> Result<()>;

impl Lowerer<'_> {
    fn label(&mut self) -> String {
        self.relax.next_label += 1;
        format!("__akscr_{}", self.relax.next_label)
    }

    /// A command-VM `if` of `conditions`. The short `if`/`else` jump a byte;
    /// an `if` [`Relax`] marks takes `if_long`/`else_long`, which jump two.
    ///
    /// The short `if` names the `else` or `endif` it resumes after; the long
    /// one names where it resumes.
    fn command_if(
        &mut self,
        conditions: &[Call],
        then: Branch,
        otherwise: Option<Branch>,
        line: usize,
        col: usize,
    ) -> Result<()> {
        let skip = self.label();
        let long = self.relax.long.contains(&skip);
        self.relax.owner.insert(skip.clone(), skip.clone());
        let (test, jump) = if long {
            ("if_long", "else_long")
        } else {
            ("if", "else")
        };
        self.push(test, vec![Arg::Symbol(skip.clone())], line, col);
        for condition in conditions {
            self.condition(condition)?;
            self.push_call(condition)?;
        }
        then(self)?;
        if !long {
            self.pending.push(skip.clone());
        }
        match otherwise {
            None => {
                self.push("endif", vec![Arg::Num(0)], line, col);
                if long {
                    self.pending.push(skip);
                }
            }
            Some(otherwise) => {
                let end = self.label();
                self.relax.owner.insert(end.clone(), skip.clone());
                self.push(jump, vec![Arg::Symbol(end.clone())], line, col);
                if long {
                    self.pending.push(skip);
                }
                otherwise(self)?;
                self.pending.push(end);
            }
        }
        Ok(())
    }

    /// An event-VM `if` of `conditions`: `evt_if` runs the conditions up to
    /// its body and jumps past it when one fails; `evt_jump` skips the
    /// `else`.
    fn event_if(
        &mut self,
        conditions: &[Call],
        then: Branch,
        otherwise: Option<Branch>,
        line: usize,
        col: usize,
    ) -> Result<()> {
        let body = self.label();
        let skip = self.label();
        self.push(
            "evt_if",
            vec![Arg::Symbol(body.clone()), Arg::Symbol(skip.clone())],
            line,
            col,
        );
        for condition in conditions {
            self.condition(condition)?;
            self.push_call(condition)?;
        }
        self.pending.push(body);
        then(self)?;
        match otherwise {
            None => self.pending.push(skip),
            Some(otherwise) => {
                let end = self.label();
                self.push("evt_jump", vec![Arg::Symbol(end.clone())], line, col);
                self.pending.push(skip);
                otherwise(self)?;
                self.pending.push(end);
            }
        }
        Ok(())
    }

    fn push(&mut self, mnemonic: &str, args: Vec<Arg>, line: usize, col: usize) {
        self.items.push(Item {
            labels: std::mem::take(&mut self.pending),
            kind: ItemKind::Insn(InsnSrc {
                mnemonic: mnemonic.to_string(),
                args,
                line,
                col,
            }),
            line,
            col,
        });
    }

    /// Push `call`; one written by field ([`layout`]) is packed into its
    /// bytes and handed to the assembler in the `.s` shape.
    fn push_call(&mut self, call: &Call) -> Result<()> {
        let args = match layout::layout(&call.name) {
            Some(fields) => {
                let values = call
                    .args
                    .iter()
                    .map(|arg| field_value(arg, call))
                    .collect::<Result<Vec<_>>>()?;
                let bytes = layout::pack(fields, &values).map_err(|error| {
                    anyhow::anyhow!(
                        "line {}, column {}: `{}` {error}",
                        call.line,
                        call.col,
                        call.name
                    )
                })?;
                layout::asm_args(&call.name, &bytes)
            }
            None => call.args.clone(),
        };
        self.push(&call.name, args, call.line, call.col);
        Ok(())
    }

    fn asm(&mut self, lines: &[(usize, String)]) -> Result<()> {
        for (line_no, text) in lines {
            if let Some(item) = asm::parse_item_line(text, *line_no, &mut self.pending)? {
                self.items.push(item);
            }
        }
        Ok(())
    }

    /// A command block plus its implicit final `end 0`.
    fn command_block(&mut self, body: &[Stmt], noreturn: bool, line: usize) -> Result<()> {
        self.commands(body)?;
        if !noreturn {
            self.push("end", vec![Arg::Num(0)], line, 1);
        }
        Ok(())
    }

    fn commands(&mut self, body: &[Stmt]) -> Result<()> {
        for stmt in body {
            match stmt {
                Stmt::Call(call) => self.command_call(call)?,
                Stmt::Return { line, col } => self.push("end", vec![Arg::Num(0)], *line, *col),
                Stmt::Require(call) => {
                    self.condition(call)?;
                    self.push_call(call)?;
                }
                Stmt::If {
                    conditions,
                    then,
                    otherwise,
                    line,
                    col,
                } => {
                    self.statement_if(conditions, then, otherwise.as_deref(), false, *line, *col)?
                }
                Stmt::Asm(lines) => self.asm(lines)?,
                Stmt::Variant {
                    mask,
                    then,
                    otherwise,
                    line,
                    col,
                } => self.variant(*mask, then, otherwise.as_deref(), false, *line, *col)?,
                _ => {
                    let (line, col) = stmt.position();
                    bail!(
                        "line {line}, column {col}: this statement is only valid in an event \
                         script"
                    );
                }
            }
        }
        Ok(())
    }

    /// An `if` statement in the command or, for `event`, the event VM.
    fn statement_if(
        &mut self,
        conditions: &[Call],
        then: &[Stmt],
        otherwise: Option<&[Stmt]>,
        event: bool,
        line: usize,
        col: usize,
    ) -> Result<()> {
        let lower = move |lowerer: &mut Lowerer, body: &[Stmt]| {
            if event {
                lowerer.events(body)
            } else {
                lowerer.commands(body)
            }
        };
        let mut then = |lowerer: &mut Lowerer| lower(lowerer, then);
        let mut otherwise = otherwise.map(|body| move |lowerer: &mut Lowerer| lower(lowerer, body));
        let otherwise = otherwise.as_mut().map(|branch| branch as Branch);
        self.flag_if(conditions, &mut then, otherwise, event, line, col)
    }

    /// A variant test: in a variant's build the branch that variant takes,
    /// in the runtime build an `if` of the flags that tell the variant
    /// ([`variant_tests`]).
    fn variant(
        &mut self,
        mask: u8,
        then: &[Stmt],
        otherwise: Option<&[Stmt]>,
        event: bool,
        line: usize,
        col: usize,
    ) -> Result<()> {
        let lower = move |lowerer: &mut Lowerer, body: &[Stmt]| {
            if event {
                lowerer.events(body)
            } else {
                lowerer.commands(body)
            }
        };
        if let Some(variant) = self.build.variant() {
            return match (variant.matches(mask), otherwise) {
                (true, _) => lower(self, then),
                (false, Some(otherwise)) => lower(self, otherwise),
                (false, None) => Ok(()),
            };
        }
        let mut then = |lowerer: &mut Lowerer| lower(lowerer, then);
        let mut otherwise = otherwise.map(|body| move |lowerer: &mut Lowerer| lower(lowerer, body));
        let otherwise = otherwise.as_mut().map(|branch| branch as Branch);
        self.variant_if(mask, &mut then, otherwise, event, line, col)
    }

    /// An `if` of the room variants of `mask` ([`variant_tests`]) in the
    /// command or, for `event`, the event VM. A test of `!USA` is the `else`
    /// of the USA test, nested in the variant's other tests.
    fn variant_if(
        &mut self,
        mask: u8,
        then: Branch,
        otherwise: Option<Branch>,
        event: bool,
        line: usize,
        col: usize,
    ) -> Result<()> {
        let (tests, not) = variant_tests(mask, line, col);
        let Some(usa) = not else {
            if tests.is_empty() {
                return then(self);
            }
            return self.flag_if(&tests, then, otherwise, event, line, col);
        };
        // The `else` body runs both where the other tests fail and in the
        // USA release.
        let has_otherwise = otherwise.is_some();
        let otherwise = std::cell::RefCell::new(otherwise);
        let other = |lowerer: &mut Lowerer| match otherwise.borrow_mut().as_mut() {
            Some(branch) => branch(lowerer),
            None => Ok(()),
        };
        let (mut first, mut second) = (other, other);
        if tests.is_empty() {
            return self.flag_if(&usa, &mut first, Some(then), event, line, col);
        }
        let mut inner = |lowerer: &mut Lowerer| {
            lowerer.flag_if(&usa, &mut first, Some(&mut *then), event, line, col)
        };
        let outer = has_otherwise.then_some(&mut second as Branch);
        self.flag_if(&tests, &mut inner, outer, event, line, col)
    }

    /// An `if` of `conditions` in the command or, for `event`, the event VM.
    fn flag_if(
        &mut self,
        conditions: &[Call],
        then: Branch,
        otherwise: Option<Branch>,
        event: bool,
        line: usize,
        col: usize,
    ) -> Result<()> {
        if event {
            self.event_if(conditions, then, otherwise, line, col)
        } else {
            self.command_if(conditions, then, otherwise, line, col)
        }
    }

    /// A plain command statement; flow opcodes must use the statement forms.
    fn command_call(&mut self, call: &Call) -> Result<()> {
        if let Some(&index) = self.procs.get(&call.name) {
            if !call.args.is_empty() {
                bail!(
                    "line {}, column {}: procedure `{}` takes no arguments",
                    call.line,
                    call.col,
                    call.name
                );
            }
            self.push(
                "gosub",
                vec![Arg::Num(i64::from(index))],
                call.line,
                call.col,
            );
            return Ok(());
        }
        if call.name == "gosub" {
            bail!(
                "line {}, column {}: call a procedure by its name: `proc NAME {{ ... }}` and \
                 `NAME();`",
                call.line,
                call.col
            );
        }
        if matches!(
            call.name.as_str(),
            "end" | "if" | "else" | "endif" | "if_long" | "else_long"
        ) {
            bail!(
                "line {}, column {}: write `{}` with the `if`/`else`/`return` statements",
                call.line,
                call.col,
                call.name
            );
        }
        // A message id word past one byte (a room message past 63,
        // `crate::text::message_index`) takes the long form.
        if call.name == "message"
            && let [id, pause] = call.args.as_slice()
            && let Ok(word) = field_value(id, call)
            && word > 0xFF
        {
            if word > 0xFFFF {
                bail!(
                    "line {}, column {}: message id {word:#X} does not fit 16 bits",
                    call.line,
                    call.col
                );
            }
            if word & 0x40 != 0 {
                bail!(
                    "line {}, column {}: global message {word:#X} has no index past 63",
                    call.line,
                    call.col
                );
            }
            self.push(
                "message_long",
                vec![Arg::Num(0), Arg::Num(word), pause.clone()],
                call.line,
                call.col,
            );
            return Ok(());
        }
        self.push_call(call)
    }

    /// Reject a non-condition command in a condition position.
    fn condition(&self, call: &Call) -> Result<()> {
        match asm::resolve_command(&call.name) {
            Some(op) if op.condition => Ok(()),
            Some(_) => bail!(
                "line {}, column {}: `{}` is not a condition",
                call.line,
                call.col,
                call.name
            ),
            None => bail!(
                "line {}, column {}: unknown condition `{}`",
                call.line,
                call.col,
                call.name
            ),
        }
    }

    fn events(&mut self, body: &[Stmt]) -> Result<()> {
        for (index, stmt) in body.iter().enumerate() {
            match stmt {
                Stmt::Call(call) => self.event_call(call)?,
                Stmt::Single { payload, call } => {
                    self.push("evt_single", vec![Arg::Num(*payload)], call.line, call.col);
                    self.command_call(call)?;
                }
                Stmt::Run {
                    noreturn,
                    body,
                    line,
                    col,
                } => {
                    self.push("evt_block", Vec::new(), *line, *col);
                    self.push("evt_block_sub", Vec::new(), *line, *col);
                    self.command_block(body, *noreturn, *line)?;
                    self.push("evt_block_sub", vec![Arg::Num(0)], *line, *col);
                    self.push("evt_nop", Vec::new(), *line, *col);
                }
                Stmt::For {
                    count,
                    body,
                    line,
                    col,
                } => {
                    self.push("evt_for", vec![Arg::Num(0), Arg::Num(*count)], *line, *col);
                    self.events(body)?;
                    self.push("evt_fornext", Vec::new(), *line, *col);
                }
                Stmt::DoWhile {
                    body,
                    condition,
                    line,
                    col,
                } => {
                    let start = self.label();
                    self.push("evt_do", vec![Arg::Symbol(start.clone())], *line, *col);
                    self.condition(condition)?;
                    self.push_call(condition)?;
                    self.pending.push(start);
                    self.events(body)?;
                    self.push("evt_dountil", Vec::new(), *line, *col);
                }
                Stmt::Raw { bytes, line, col } => {
                    if index + 1 != body.len() {
                        bail!("line {line}, column {col}: `raw` must be the last statement");
                    }
                    self.items.push(Item {
                        labels: std::mem::take(&mut self.pending),
                        kind: ItemKind::Db(bytes.clone()),
                        line: *line,
                        col: *col,
                    });
                }
                Stmt::Asm(lines) => self.asm(lines)?,
                Stmt::Variant {
                    mask,
                    then,
                    otherwise,
                    line,
                    col,
                } => self.variant(*mask, then, otherwise.as_deref(), true, *line, *col)?,
                Stmt::If {
                    conditions,
                    then,
                    otherwise,
                    line,
                    col,
                } => {
                    self.statement_if(conditions, then, otherwise.as_deref(), true, *line, *col)?
                }
                Stmt::Return { .. } | Stmt::Require(_) => {
                    let (line, col) = stmt.position();
                    bail!(
                        "line {line}, column {col}: command flow is only valid inside a `run` \
                         block"
                    );
                }
            }
        }
        Ok(())
    }

    /// An event statement: the `sleep`/`wait_message` forms, an event-VM
    /// instruction, or a command wrapped in `evt_single`.
    fn event_call(&mut self, call: &Call) -> Result<()> {
        match call.name.as_str() {
            "sleep" => {
                let [Arg::Num(count)] = call.args.as_slice() else {
                    bail!(
                        "line {}, column {}: `sleep` takes one frame count",
                        call.line,
                        call.col
                    );
                };
                let Ok(count) = u16::try_from(*count) else {
                    bail!(
                        "line {}, column {}: `sleep` count {count} does not fit 16 bits",
                        call.line,
                        call.col
                    );
                };
                let [low, high] = count.to_le_bytes();
                self.push(
                    "evt_sleep",
                    vec![
                        Arg::Num(0xF9),
                        Arg::Num(i64::from(low)),
                        Arg::Num(i64::from(high)),
                    ],
                    call.line,
                    call.col,
                );
            }
            "wait_message" => {
                if !call.args.is_empty() {
                    bail!(
                        "line {}, column {}: `wait_message` takes no arguments",
                        call.line,
                        call.col
                    );
                }
                self.push("evt_push_cond", Vec::new(), call.line, call.col);
                self.push("evt_skip_if", Vec::new(), call.line, call.col);
            }
            name if is_event_mnemonic(name) => self.push_call(call)?,
            _ => {
                self.push("evt_single", Vec::new(), call.line, call.col);
                self.command_call(call)?;
            }
        }
        Ok(())
    }
}

/// The value of an argument to a command written by field.
fn field_value(arg: &Arg, call: &Call) -> Result<i64> {
    match arg {
        Arg::Num(value) => Ok(*value),
        Arg::Bits(high, low) => Ok(high | low),
        Arg::Symbol(name) => asm::engine_constant(name).ok_or_else(|| {
            anyhow::anyhow!(
                "line {}, column {}: `{name}` is not a value `{}` takes",
                call.line,
                call.col,
                call.name
            )
        }),
        Arg::Label(..) => bail!(
            "line {}, column {}: `{}` takes no labels",
            call.line,
            call.col,
            call.name
        ),
    }
}

/// True for an event-VM mnemonic (any state), as opposed to a command.
fn is_event_mnemonic(name: &str) -> bool {
    let named = |signature: &&str| signature.split(':').next() == Some(name);
    EVENT_TOP_SIG.iter().any(named)
        || CONTROL_SIG.iter().any(named)
        || ACTOR_SIG.iter().any(named)
        || TWEEN_SIG.iter().any(named)
        || matches!(name, "act_motion_path" | "act_finish" | "tw_finish")
}
