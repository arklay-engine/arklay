//! `.akscr` decompiler: structured code from the decoded SCD IR.
//!
//! Every section is rendered structurally where the bytecode has the shape the
//! compiler emits, and falls back to the escape hatches (`noreturn`,
//! `single(N)`, `raw(...)`, `asm { ... }`) where it does not. Each rendered
//! section is then compiled on its own and compared with the original bytes;
//! a section that does not reproduce exactly is re-rendered as one `asm`
//! block, so the output always compiles back to identical bytecode.

use crate::scd::akscr::compile;
use crate::scd::disasm::{EventState, format_insn, next_event_state, render_run};
use crate::scd::ir::{Block, Decoded, Insn, Scripts, Stream, StreamKind};
use crate::scd::opcode::command_width;

/// Render the `.akscr` source of every stream.
pub fn decompile(scripts: &Scripts) -> String {
    let mut sections: Vec<String> = Vec::new();
    for block in &scripts.init {
        sections.push(block_section("init", block));
    }
    for block in &scripts.main {
        sections.push(block_section("main", block));
    }
    for stream in &scripts.events {
        sections.push(event_section(stream));
    }

    let mut out = String::from("#version 1\n");
    for section in sections {
        out.push('\n');
        out.push_str(&section);
    }
    out
}

/// One `proc init`/`proc main` section, verified against the block's bytes.
fn block_section(name: &str, block: &Block) -> String {
    let original = stream_bytes(&block.insns, &block.trailing);
    let mut writer = Writer::default();
    let header = format!("proc {name}");
    if block.trailing.is_empty()
        && let Some(body) = structure_block(&block.insns)
    {
        writer.command_block(&header, &body);
    } else {
        writer.asm_block(&header, &block.insns, &block.trailing, block.offset + 2);
    }
    let text = writer.finish();
    if compile::section_bytes(&text).ok().as_deref() == Some(original.as_slice()) {
        return text;
    }
    let mut writer = Writer::default();
    writer.asm_block(&header, &block.insns, &block.trailing, block.offset + 2);
    writer.finish()
}

/// One `event event_XX` section, verified against the stream's bytes.
fn event_section(stream: &Stream) -> String {
    let StreamKind::Event(index) = stream.kind else {
        return String::new();
    };
    let mut header = format!("event event_{index:02X}");
    if needs_nopad(stream) {
        header.push_str(" nopad");
    }
    let original = stream_bytes(&stream.insns, &stream.trailing);
    let mut writer = Writer::default();
    if let Some(body) = EventParser::new(&stream.insns).parse_all() {
        writer.line(&header);
        writer.open();
        writer.events(&body);
        if !is_alignment_padding(stream) {
            writer.raw(&stream.trailing);
        }
        writer.close();
    } else {
        writer.event_asm(&header, stream);
    }
    let text = writer.finish();
    if compile::section_bytes(&text).ok().as_deref() == Some(original.as_slice()) {
        return text;
    }
    let mut writer = Writer::default();
    writer.event_asm(&header, stream);
    writer.finish()
}

/// The exact bytes of one stream: every instruction plus the trailing data.
fn stream_bytes(insns: &[Insn], trailing: &[u8]) -> Vec<u8> {
    let mut bytes: Vec<u8> = insns
        .iter()
        .flat_map(|insn| insn.bytes.iter().copied())
        .collect();
    bytes.extend_from_slice(trailing);
    bytes
}

/// True when an event's trailing bytes are exactly the zero padding the
/// compiler adds after a final `evt_finish` (none when it is `nopad`).
fn is_alignment_padding(stream: &Stream) -> bool {
    if !ends_finished(stream) || needs_nopad(stream) {
        return stream.trailing.is_empty();
    }
    let padding = (4 - instruction_bytes(stream) % 4) % 4;
    stream.trailing.len() == padding && stream.trailing.iter().all(|&byte| byte == 0)
}

/// True for an event that ends in `evt_finish` short of a four-byte boundary
/// with nothing after it, which only the `nopad` form reproduces.
fn needs_nopad(stream: &Stream) -> bool {
    ends_finished(stream)
        && stream.trailing.is_empty()
        && !instruction_bytes(stream).is_multiple_of(4)
}

fn ends_finished(stream: &Stream) -> bool {
    stream
        .insns
        .last()
        .is_some_and(|insn| matches!(insn.decoded, Decoded::Control(op) if op.op == 0xFF))
}

fn instruction_bytes(stream: &Stream) -> usize {
    stream.insns.iter().map(|insn| insn.bytes.len()).sum()
}

// ---------------------------------------------------------------------------
// Command blocks
// ---------------------------------------------------------------------------

/// A structured command-VM statement.
enum Cmd<'a> {
    /// A plain command.
    Call(&'a Insn),
    /// A condition outside an `if` head: abandons the enclosing `if` body (or
    /// the whole block) when false.
    Require(&'a Insn),
    /// `end 0`.
    Return,
    /// An `endif` that closes no `if`, kept as a one-line `asm` block.
    StrayEndif(&'a Insn),
    /// `if`/`else`/`endif`.
    If {
        conditions: Vec<&'a Insn>,
        then: Vec<Cmd<'a>>,
        otherwise: Option<Vec<Cmd<'a>>>,
    },
}

/// A structured command block and whether its final `end 0` is missing.
struct CommandBody<'a> {
    noreturn: bool,
    body: Vec<Cmd<'a>>,
}

/// Structure one command block. The final top-level `end 0` is implicit; a
/// block without one is marked `noreturn`.
fn structure_block(insns: &[Insn]) -> Option<CommandBody<'_>> {
    let last = insns.last();
    let implicit_end = last.is_some_and(|insn| is_command(insn, 0x00) && insn.bytes[1] == 0);
    let (end, count) = match last {
        Some(insn) if implicit_end => (insn.offset, insns.len() - 1),
        Some(insn) => (insn.offset + insn.bytes.len(), insns.len()),
        None => (0, 0),
    };
    let mut index = 0usize;
    let body = structure_commands(insns, &mut index, end)?;
    (index == count).then_some(CommandBody {
        noreturn: !implicit_end,
        body,
    })
}

/// Structure the commands from `*index` up to the instruction at offset `end`.
fn structure_commands<'a>(
    insns: &'a [Insn],
    index: &mut usize,
    end: usize,
) -> Option<Vec<Cmd<'a>>> {
    let mut body = Vec::new();
    while let Some(insn) = insns.get(*index)
        && insn.offset < end
    {
        let Decoded::Command(op) = insn.decoded else {
            return None;
        };
        match op.op {
            0x00 => {
                if insn.bytes[1] != 0 {
                    return None;
                }
                body.push(Cmd::Return);
                *index += 1;
            }
            0x01 => body.push(structure_if(insns, index, end)?),
            // The original data pops the branch stack with an unpaired
            // `endif`; it carries no jump, so it can sit in place.
            0x03 => {
                body.push(Cmd::StrayEndif(insn));
                *index += 1;
            }
            0x02 => return None,
            _ if op.condition => {
                body.push(Cmd::Require(insn));
                *index += 1;
            }
            _ => {
                body.push(Cmd::Call(insn));
                *index += 1;
            }
        }
    }
    Some(body)
}

/// Structure the `if` at `*index`: its conditions, its body up to the
/// `endif`/`else` its skip lands on, and the `else` body up to its target.
fn structure_if<'a>(insns: &'a [Insn], index: &mut usize, end: usize) -> Option<Cmd<'a>> {
    let head = &insns[*index];
    let label = head.operands.first()?.target?.checked_sub(2)?;
    if label <= head.offset || label >= end {
        return None;
    }
    *index += 1;
    let mut conditions = Vec::new();
    while let Some(insn) = insns.get(*index)
        && insn.offset < label
        && matches!(insn.decoded, Decoded::Command(op) if op.condition)
    {
        conditions.push(insn);
        *index += 1;
    }
    if conditions.is_empty() {
        return None;
    }
    let then = structure_commands(insns, index, label)?;
    let tail = insns.get(*index).filter(|insn| insn.offset == label)?;
    *index += 1;
    if is_command(tail, 0x03) {
        if tail.bytes[1] != 0 {
            return None;
        }
        return Some(Cmd::If {
            conditions,
            then,
            otherwise: None,
        });
    }
    if !is_command(tail, 0x02) {
        return None;
    }
    let target = tail.operands.first()?.target?;
    if target > end {
        return None;
    }
    let otherwise = structure_commands(insns, index, target)?;
    // The compiler labels the instruction after the `else` body, so one must
    // start exactly at the target.
    insns.get(*index).filter(|insn| insn.offset == target)?;
    Some(Cmd::If {
        conditions,
        then,
        otherwise: Some(otherwise),
    })
}

fn is_command(insn: &Insn, opcode: u8) -> bool {
    matches!(insn.decoded, Decoded::Command(op) if op.op == opcode)
}

// ---------------------------------------------------------------------------
// Event streams
// ---------------------------------------------------------------------------

/// A structured event-VM statement.
enum Evt<'a> {
    /// One rendered statement line.
    Line(String),
    /// `evt_block` with a single command sub-block.
    Run {
        insns: &'a [Insn],
        body: Option<CommandBody<'a>>,
        start: usize,
    },
    /// `evt_for 0, N` ... `evt_fornext`.
    For { count: u16, body: Vec<Evt<'a>> },
    /// `evt_do` + inline condition ... `evt_dountil`.
    DoWhile {
        condition: &'a Insn,
        body: Vec<Evt<'a>>,
    },
}

/// Recursive-descent structurer over one event stream.
struct EventParser<'a> {
    insns: &'a [Insn],
    /// The sub-ISA state before each instruction.
    states: Vec<EventState>,
    index: usize,
}

impl<'a> EventParser<'a> {
    fn new(insns: &'a [Insn]) -> Self {
        let mut states = Vec::with_capacity(insns.len());
        let mut state = EventState::Top;
        for insn in insns {
            states.push(state);
            state = next_event_state(state, insn);
        }
        Self {
            insns,
            states,
            index: 0,
        }
    }

    fn parse_all(mut self) -> Option<Vec<Evt<'a>>> {
        let body = self.parse(None)?;
        (self.index == self.insns.len()).then_some(body)
    }

    /// Parse statements until the control opcode `close` (left unconsumed) or
    /// the end of the stream when `close` is `None`.
    fn parse(&mut self, close: Option<u8>) -> Option<Vec<Evt<'a>>> {
        let mut body = Vec::new();
        while let Some(insn) = self.insns.get(self.index) {
            let state = self.states[self.index];
            match insn.decoded {
                Decoded::Control(op) => match op.op {
                    0xFB | 0xFD => {
                        return (close == Some(op.op)).then_some(body);
                    }
                    0xFA => {
                        if insn.bytes[1] != 0 {
                            return None;
                        }
                        let count = u16::from_le_bytes([insn.bytes[2], insn.bytes[3]]);
                        self.index += 1;
                        let inner = self.parse(Some(0xFB))?;
                        self.index += 1;
                        body.push(Evt::For { count, body: inner });
                    }
                    0xFC => body.push(self.parse_do(insn)?),
                    0xF8 if insn.bytes.len() == 4 && insn.bytes[1] == 0xF9 => {
                        let count = u16::from_le_bytes([insn.bytes[2], insn.bytes[3]]);
                        body.push(Evt::Line(format!("sleep({count});")));
                        self.index += 1;
                    }
                    0xF6 if self.insns.get(self.index + 1).is_some_and(
                        |next| matches!(next.decoded, Decoded::Control(op) if op.op == 0xF7),
                    ) =>
                    {
                        body.push(Evt::Line("wait_message();".to_string()));
                        self.index += 2;
                    }
                    _ => {
                        body.push(Evt::Line(statement(insn, state)));
                        self.index += 1;
                    }
                },
                Decoded::Event(op) if op.op == 0x06 => body.push(self.parse_run()?),
                Decoded::Event(op) if op.op == 0x07 => body.push(self.parse_single(insn)?),
                Decoded::Event(_) | Decoded::Actor(_) | Decoded::Tween(_) => {
                    body.push(Evt::Line(statement(insn, state)));
                    self.index += 1;
                }
                _ => return None,
            }
        }
        close.is_none().then_some(body)
    }

    /// `evt_do` whose jump skips exactly one whole condition command.
    fn parse_do(&mut self, head: &'a Insn) -> Option<Evt<'a>> {
        let target = head.operands.first()?.target?;
        let condition = self.insns.get(self.index + 1)?;
        let Decoded::Command(op) = condition.decoded else {
            return None;
        };
        if !op.condition
            || condition.offset + condition.bytes.len() != target
            || command_width(&condition.bytes) != Some(condition.bytes.len())
        {
            return None;
        }
        self.index += 2;
        let body = self.parse(Some(0xFD))?;
        self.index += 1;
        Some(Evt::DoWhile { condition, body })
    }

    /// `evt_block` with one sub-block, its terminator and the `evt_nop` that
    /// is the terminator's second byte.
    fn parse_run(&mut self) -> Option<Evt<'a>> {
        let header = self.insns.get(self.index + 1)?;
        if !matches!(header.decoded, Decoded::SubBlockHeader { size } if size != 0) {
            return None;
        }
        let start = self.index + 2;
        let mut end = start;
        while let Some(insn) = self.insns.get(end)
            && matches!(insn.decoded, Decoded::Command(_))
        {
            end += 1;
        }
        let terminator = self.insns.get(end)?;
        if !matches!(terminator.decoded, Decoded::SubBlockHeader { size: 0 }) {
            return None;
        }
        let nop = self.insns.get(end + 1)?;
        if !matches!(nop.decoded, Decoded::Event(op) if op.op == 0x00) {
            return None;
        }
        let insns = &self.insns[start..end];
        self.index = end + 2;
        Some(Evt::Run {
            insns,
            body: structure_block(insns),
            start: header.offset + 2,
        })
    }

    /// `evt_single`: the inline command as a statement, with the explicit
    /// payload form when the payload does not match the command's width.
    fn parse_single(&mut self, head: &'a Insn) -> Option<Evt<'a>> {
        let command = self.insns.get(self.index + 1)?;
        let Decoded::Command(op) = command.decoded else {
            return None;
        };
        if op.op <= 0x03 {
            return None;
        }
        let call = call_text(command, EventState::Top);
        let exact = command_width(&command.bytes) == Some(command.bytes.len())
            && usize::from(head.bytes[1]) == command.bytes.len() + 2;
        self.index += 2;
        Some(Evt::Line(if exact {
            format!("{call};")
        } else {
            format!("single({}) {call};", head.bytes[1])
        }))
    }
}

/// `name(args)` for one instruction.
fn call_text(insn: &Insn, state: EventState) -> String {
    let (name, args) = format_insn(insn, state, None);
    format!("{name}({})", args.join(", "))
}

/// `name(args);` for one instruction.
fn statement(insn: &Insn, state: EventState) -> String {
    format!("{};", call_text(insn, state))
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

/// Indented line writer.
#[derive(Default)]
struct Writer {
    lines: Vec<String>,
    indent: usize,
}

impl Writer {
    fn line(&mut self, text: &str) {
        if text.is_empty() {
            self.lines.push(String::new());
        } else {
            self.lines
                .push(format!("{}{text}", "    ".repeat(self.indent)));
        }
    }

    fn open(&mut self) {
        self.line("{");
        self.indent += 1;
    }

    fn close(&mut self) {
        self.indent = self.indent.saturating_sub(1);
        self.line("}");
    }

    fn finish(self) -> String {
        let mut out = self.lines.join("\n");
        out.push('\n');
        out
    }

    /// A `header [noreturn] { ... }` command block.
    fn command_block(&mut self, header: &str, body: &CommandBody) {
        if body.noreturn {
            self.line(&format!("{header} noreturn"));
        } else {
            self.line(header);
        }
        self.open();
        self.commands(&body.body);
        self.close();
    }

    /// A `header noreturn { asm { ... } }` block holding raw instructions.
    fn asm_block(&mut self, header: &str, insns: &[Insn], trailing: &[u8], start: usize) {
        self.line(&format!("{header} noreturn"));
        self.open();
        self.asm(insns, trailing, start);
        self.close();
    }

    /// A whole event as one `asm` block.
    fn event_asm(&mut self, header: &str, stream: &Stream) {
        self.line(header);
        self.open();
        self.asm(&stream.insns, &stream.trailing, stream.offset);
        self.close();
    }

    fn asm(&mut self, insns: &[Insn], trailing: &[u8], start: usize) {
        self.line("asm");
        self.open();
        for line in render_run(insns, trailing, start) {
            self.line(line.trim_end());
        }
        self.close();
    }

    fn raw(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let text = bytes
            .iter()
            .map(|byte| format!("0x{byte:02X}"))
            .collect::<Vec<_>>()
            .join(", ");
        self.line(&format!("raw({text});"));
    }

    fn commands(&mut self, body: &[Cmd]) {
        for cmd in body {
            match cmd {
                Cmd::Call(insn) => self.line(&statement(insn, EventState::Top)),
                Cmd::Require(insn) => {
                    self.line(&format!("require({});", call_text(insn, EventState::Top)))
                }
                Cmd::Return => self.line("return;"),
                Cmd::StrayEndif(insn) => self.asm(std::slice::from_ref(*insn), &[], insn.offset),
                Cmd::If { .. } => self.if_chain(cmd, "if"),
            }
        }
    }

    /// An `if` statement; an `else` body that is exactly one `if` renders as
    /// `else if`.
    fn if_chain(&mut self, cmd: &Cmd, keyword: &str) {
        let Cmd::If {
            conditions,
            then,
            otherwise,
        } = cmd
        else {
            return;
        };
        let expression = conditions
            .iter()
            .map(|insn| call_text(insn, EventState::Top))
            .collect::<Vec<_>>()
            .join(" && ");
        self.line(&format!("{keyword} ({expression})"));
        self.open();
        self.commands(then);
        self.close();
        match otherwise.as_deref() {
            None => {}
            Some([nested @ Cmd::If { .. }]) => self.if_chain(nested, "else if"),
            Some(body) => {
                self.line("else");
                self.open();
                self.commands(body);
                self.close();
            }
        }
    }

    fn events(&mut self, body: &[Evt]) {
        for evt in body {
            match evt {
                Evt::Line(text) => self.line(text),
                Evt::Run { insns, body, start } => match body {
                    Some(body) => self.command_block("run", body),
                    None => self.asm_block("run", insns, &[], *start),
                },
                Evt::For { count, body } => {
                    self.line(&format!("for ({count})"));
                    self.open();
                    self.events(body);
                    self.close();
                }
                Evt::DoWhile { condition, body } => {
                    self.line("do");
                    self.open();
                    self.events(body);
                    self.close();
                    self.line(&format!(
                        "while ({});",
                        call_text(condition, EventState::Top)
                    ));
                }
            }
        }
    }
}
