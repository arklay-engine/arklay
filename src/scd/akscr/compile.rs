//! `.akscr` compiler: source text to the SCD IR.
//!
//! The source is tokenised, parsed into a small statement tree and lowered to
//! the assembler's item stream (mnemonic plus operands, with generated labels
//! for the jumps), which [`asm::assemble_source`] encodes, lays out and
//! re-decodes. Errors name the source line and column.

use anyhow::{Result, bail};

use crate::budget;
use crate::scd::asm::{self, Arg, Assembled, InsnSrc, Item, ItemKind, Source};
use crate::scd::disasm::{ACTOR_SIG, CONTROL_SIG, EVENT_TOP_SIG, TWEEN_SIG};

/// Compile `.akscr` source into the SCD IR.
pub fn compile(text: &str) -> Result<Assembled> {
    let sections = parse(text)?;
    let mut source = Source::default();
    let mut pad_events = Vec::new();
    let mut init_done = false;
    let mut main_done = false;
    for section in &sections {
        let items = lower_section(section)?;
        match section.kind {
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
                pad_events.push(!section.nopad);
            }
        }
    }
    asm::assemble_source(&source, &pad_events)
}

/// Compile a single section (with or without the `#version` line) and return
/// the bytes of its block or event stream.
pub(super) fn section_bytes(text: &str) -> Result<Vec<u8>> {
    let text = if text.trim_start().starts_with('#') {
        text.to_string()
    } else {
        format!("#version 1\n{text}")
    };
    let sections = parse(&text)?;
    let [section] = sections.as_slice() else {
        bail!("expected exactly one section");
    };
    let items = lower_section(section)?;
    let event = matches!(section.kind, SectionKind::Event(_));
    asm::assemble_stream(&items, event, !section.nopad)
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

const PUNCTUATION: [&str; 10] = ["&&", "(", ")", "{", "}", ",", ";", "|", "-", "#"];

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
    Event(u8),
}

#[derive(Debug)]
struct Section {
    kind: SectionKind,
    noreturn: bool,
    /// An event without the four-byte padding after its final `evt_finish`.
    nopad: bool,
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
            | Stmt::Run { line, col, .. }
            | Stmt::For { line, col, .. }
            | Stmt::DoWhile { line, col, .. }
            | Stmt::Raw { line, col, .. } => (*line, *col),
            Stmt::Asm(lines) => (lines.first().map_or(0, |(line, _)| *line), 1),
        }
    }
}

fn parse(text: &str) -> Result<Vec<Section>> {
    budget::check_len(text.len(), budget::MAX_TEXT_BYTES, "script size")?;
    let tokens = lex(text)?;
    let mut parser = Parser { tokens, pos: 0 };
    parser.version()?;
    let mut sections = Vec::new();
    while parser.peek().tok != Tok::Eof {
        sections.push(parser.section()?);
    }
    Ok(sections)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
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
        match token.tok {
            Tok::Num(value) => Ok(if negative { -value } else { value }),
            _ => self.error(&token, "a number"),
        }
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
                    _ => return self.error(&name_token, "`init` or `main`"),
                }
            }
            "event" => {
                let (name, name_token) = self.expect_ident()?;
                let index = name
                    .strip_prefix("event_")
                    .filter(|hex| hex.len() == 2)
                    .and_then(|hex| u8::from_str_radix(hex, 16).ok());
                match index {
                    Some(index) => SectionKind::Event(index),
                    None => return self.error(&name_token, "an event name `event_XX`"),
                }
            }
            _ => return self.error(&token, "`proc` or `event`"),
        };
        let event = matches!(kind, SectionKind::Event(_));
        let noreturn = !event && self.eat_ident("noreturn");
        let nopad = event && self.eat_ident("nopad");
        let body = self.block()?;
        Ok(Section {
            kind,
            noreturn,
            nopad,
            body,
            line: token.line,
        })
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
            Tok::Asm(lines) => return Ok(Stmt::Asm(lines.clone())),
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
        let mut conditions = vec![self.call()?];
        while self.eat_punct("&&") {
            conditions.push(self.call()?);
        }
        self.expect_punct(")")?;
        let then = self.block()?;
        let otherwise = if self.eat_ident("else") {
            let next = self.peek().clone();
            if self.eat_ident("if") {
                Some(vec![self.if_statement(next.line, next.col)?])
            } else {
                Some(self.block()?)
            }
        } else {
            None
        };
        Ok(Stmt::If {
            conditions,
            then,
            otherwise,
            line,
            col,
        })
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
        if let Tok::Ident(name) = &self.peek().tok {
            let name = name.clone();
            self.pos += 1;
            return Ok(Arg::Symbol(name));
        }
        let value = self.expect_int()?;
        if self.eat_punct("|") {
            let low = self.expect_int()?;
            return Ok(Arg::Bits(value, low));
        }
        Ok(Arg::Num(value))
    }
}

// ---------------------------------------------------------------------------
// Lowering
// ---------------------------------------------------------------------------

/// Lower one section to assembler items.
fn lower_section(section: &Section) -> Result<Vec<Item>> {
    let mut lowerer = Lowerer::default();
    match section.kind {
        SectionKind::Init | SectionKind::Main => {
            lowerer.command_block(&section.body, section.noreturn, section.line)?;
        }
        SectionKind::Event(_) => lowerer.events(&section.body)?,
    }
    if let Some(label) = lowerer.pending.first() {
        bail!(
            "line {}: the jump to `{label}` has no instruction to land on; end the block \
             with a statement",
            section.line
        );
    }
    Ok(lowerer.items)
}

#[derive(Default)]
struct Lowerer {
    items: Vec<Item>,
    /// Labels for the next item.
    pending: Vec<String>,
    next_label: usize,
}

impl Lowerer {
    fn label(&mut self) -> String {
        self.next_label += 1;
        format!("__akscr_{}", self.next_label)
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

    fn push_call(&mut self, call: &Call) {
        self.push(&call.name, call.args.clone(), call.line, call.col);
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
                    self.push_call(call);
                }
                Stmt::If {
                    conditions,
                    then,
                    otherwise,
                    line,
                    col,
                } => {
                    let skip = self.label();
                    self.push("if", vec![Arg::Symbol(skip.clone())], *line, *col);
                    for condition in conditions {
                        self.condition(condition)?;
                        self.push_call(condition);
                    }
                    self.commands(then)?;
                    self.pending.push(skip);
                    match otherwise {
                        None => self.push("endif", vec![Arg::Num(0)], *line, *col),
                        Some(otherwise) => {
                            let end = self.label();
                            self.push("else", vec![Arg::Symbol(end.clone())], *line, *col);
                            self.commands(otherwise)?;
                            self.pending.push(end);
                        }
                    }
                }
                Stmt::Asm(lines) => self.asm(lines)?,
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

    /// A plain command statement; flow opcodes must use the statement forms.
    fn command_call(&mut self, call: &Call) -> Result<()> {
        if matches!(call.name.as_str(), "end" | "if" | "else" | "endif") {
            bail!(
                "line {}, column {}: write `{}` with the `if`/`else`/`return` statements",
                call.line,
                call.col,
                call.name
            );
        }
        self.push_call(call);
        Ok(())
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
                    self.push_call(condition);
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
                Stmt::Return { .. } | Stmt::Require(_) | Stmt::If { .. } => {
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
            name if is_event_mnemonic(name) => self.push_call(call),
            _ => {
                self.push("evt_single", Vec::new(), call.line, call.col);
                self.command_call(call)?;
            }
        }
        Ok(())
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
