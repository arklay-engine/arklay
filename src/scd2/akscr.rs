//! RE2 `.akscr` scripts: the procedure lists as structured source that
//! compiles back to identical bytes.
//!
//! ```text
//! #version 1
//! #game re2
//!
//! proc init_00
//! {
//!     if (ck(1, 25, 0) && cmp(0, 24, 0, 3))
//!     {
//!         set(5, 3, 1);
//!     }
//!     else
//!     {
//!         gosub(2);
//!     }
//!     evt_end(0);
//! }
//!
//! proc main_00
//! {
//!     while (ck(5, 3, 0))
//!     {
//!         evt_next();
//!     }
//!     do
//!     {
//!         evt_next();
//!     }
//!     while (ck(5, 4, 1));
//!     for (30)
//!     {
//!         evt_next();
//!     }
//!     switch (16)
//!     {
//!         case (1)
//!         {
//!             break(0);
//!         }
//!         default
//!         {
//!         }
//!     }
//!     evt_end(0);
//! }
//! ```
//!
//! - `proc init_NN` / `proc main_NN` is procedure `NN` (hex) of a list; the
//!   procedures of each list are dense and ascending, `init` first.
//! - `name(args);` is one instruction with every operand field, as in the
//!   `.s` form.
//! - `if (a && b) { ... } else { ... }`: the conditions are the instructions
//!   right after the `if` opcode; any of them failing skips to the `else`
//!   body or past the block. An `else` body ends in two `nop` bytes, which
//!   is how the original scripts close it.
//! - `while (c) { ... }`, `do { ... } while (c);`, `for (count) { ... }`,
//!   `for2 (variable) { ... }` and `switch (variable) { case (value) { ... }
//!   default { ... } }` are the block opcodes with their lengths computed.
//!   A `while` or `do` condition may join several conditions with `&&` and
//!   `||`.
//! - `@name(args);` writes a block opcode (`if`, `else`, `endif`, `for`,
//!   `next`, `while`, `ewhile`, `do`, `edwhile`, `switch`, `case`,
//!   `default`, `eswitch`, `for2`) as a plain instruction with explicit
//!   operands. The decompiler uses it for a block whose stored lengths or
//!   padding differ from what the structured form would compile to.
//! - `raw(0x.., ...);` as a procedure's last statement gives its trailing
//!   bytes.

use std::collections::HashMap;
use std::fmt::Write as _;

use anyhow::{Result, anyhow, bail};

use crate::scd2::asm::parse_int;
use crate::scd2::opcode::{
    self, CASE, DEFAULT, DO, EDWHILE, ELSE, ENDIF, ESWITCH, EWHILE, FOR, FOR2, IF, NEXT, NOP, Op,
    SWITCH, WHILE,
};
use crate::scd2::reader::decode_procedure;
use crate::scd2::{Insn, ProcList, Procedure, Scripts};

/// Opcodes that have a structured form, written `@name(...)` as plain
/// instructions.
const BLOCK_OPS: [u8; 14] = [
    IF, ELSE, ENDIF, FOR, NEXT, WHILE, EWHILE, DO, EDWHILE, SWITCH, CASE, DEFAULT, ESWITCH, FOR2,
];

fn is_block_op(op: u8) -> bool {
    BLOCK_OPS.contains(&op)
}

// ---- decompiler ------------------------------------------------------------

/// Decompile scripts into `.akscr` source.
pub fn decompile(scripts: &Scripts) -> String {
    let mut out = String::from("#version 1\n#game re2\n");
    for (name, list) in [("init", &scripts.init), ("main", &scripts.main)] {
        for (index, procedure) in list.procs.iter().enumerate() {
            let _ = write!(out, "\nproc {name}_{index:02X}\n{{\n");
            let decompiler = Decompiler::new(&procedure.insns);
            decompiler.range(0, procedure.insns.len(), 1, &mut out);
            if !procedure.trailing.is_empty() {
                let bytes: Vec<String> = procedure
                    .trailing
                    .iter()
                    .map(|byte| format!("0x{byte:02X}"))
                    .collect();
                let _ = writeln!(out, "    raw({});", bytes.join(", "));
            }
            out.push_str("}\n");
        }
    }
    out
}

struct Decompiler<'a> {
    insns: &'a [Insn],
    /// Instruction index by offset.
    index: HashMap<usize, usize>,
}

/// A structured statement matched at an instruction.
enum Matched {
    If {
        conds: (usize, usize),
        body: (usize, usize),
        otherwise: Option<(usize, usize)>,
    },
    While {
        cond: usize,
        body: (usize, usize),
    },
    Do {
        body: (usize, usize),
        cond: usize,
    },
    For {
        keyword: &'static str,
        value: i64,
        body: (usize, usize),
    },
    Switch {
        value: i64,
        sections: Vec<(Option<i64>, (usize, usize))>,
    },
}

impl<'a> Decompiler<'a> {
    fn new(insns: &'a [Insn]) -> Self {
        let index = insns
            .iter()
            .enumerate()
            .map(|(i, insn)| (insn.offset, i))
            .collect();
        Self { insns, index }
    }

    /// The instruction starting at `offset` with opcode `op`, if it lies
    /// within `[lo, hi)`.
    fn at(&self, offset: usize, op: u8, lo: usize, hi: usize) -> Option<usize> {
        let &i = self.index.get(&offset)?;
        (lo <= i && i < hi && self.insns[i].bytes[0] == op).then_some(i)
    }

    /// The index just past the instruction ending at `offset`, if it lies
    /// within `[lo, hi]`.
    fn boundary(&self, offset: usize, lo: usize, hi: usize) -> Option<usize> {
        let i = if offset == self.insns.last().map_or(0, Insn::end) {
            self.insns.len()
        } else {
            *self.index.get(&offset)?
        };
        (lo <= i && i <= hi).then_some(i)
    }

    fn range(&self, lo: usize, hi: usize, depth: usize, out: &mut String) {
        let mut i = lo;
        while i < hi {
            match self.structure(i, hi) {
                Some((matched, next)) => {
                    self.render(&matched, depth, out);
                    i = next;
                }
                None => {
                    line(out, depth, &call(&self.insns[i], true));
                    i += 1;
                }
            }
        }
    }

    fn render(&self, matched: &Matched, depth: usize, out: &mut String) {
        match matched {
            Matched::If {
                conds,
                body,
                otherwise,
            } => {
                let conds: Vec<String> = (conds.0..conds.1)
                    .map(|i| call(&self.insns[i], false))
                    .collect();
                line(out, depth, &format!("if ({})", conds.join(" && ")));
                self.braced(*body, depth, out);
                if let Some(otherwise) = otherwise {
                    line(out, depth, "else");
                    self.braced(*otherwise, depth, out);
                }
            }
            Matched::While { cond, body } => {
                line(
                    out,
                    depth,
                    &format!("while ({})", call(&self.insns[*cond], false)),
                );
                self.braced(*body, depth, out);
            }
            Matched::Do { body, cond } => {
                line(out, depth, "do");
                self.braced(*body, depth, out);
                line(
                    out,
                    depth,
                    &format!("while ({});", call(&self.insns[*cond], false)),
                );
            }
            Matched::For {
                keyword,
                value,
                body,
            } => {
                line(out, depth, &format!("{keyword} ({value})"));
                self.braced(*body, depth, out);
            }
            Matched::Switch { value, sections } => {
                line(out, depth, &format!("switch ({value})"));
                line(out, depth, "{");
                for (value, body) in sections {
                    match value {
                        Some(value) => line(out, depth + 1, &format!("case ({value})")),
                        None => line(out, depth + 1, "default"),
                    }
                    self.braced(*body, depth + 1, out);
                }
                line(out, depth, "}");
            }
        }
    }

    fn braced(&self, (lo, hi): (usize, usize), depth: usize, out: &mut String) {
        line(out, depth, "{");
        self.range(lo, hi, depth + 1, out);
        line(out, depth, "}");
    }

    /// Match a structured statement at instruction `i`, nested within
    /// `[.., hi)`: the statement and the index past it. `None` when the
    /// stored lengths or padding differ from what the structured form
    /// compiles to.
    fn structure(&self, i: usize, hi: usize) -> Option<(Matched, usize)> {
        let insn = &self.insns[i];
        let ops = insn.operands();
        match insn.bytes[0] {
            IF if ops[0] == 0 => {
                let target = insn.end() + ops[1] as usize;
                let mut j = i + 1;
                while j < hi && self.insns[j].op().condition && self.insns[j].end() <= target {
                    j += 1;
                }
                if let Some(e) = target
                    .checked_sub(4)
                    .and_then(|at| self.at(at, ELSE, j, hi))
                {
                    let other = &self.insns[e];
                    let other_ops = other.operands();
                    if other_ops[0] != 0 {
                        return None;
                    }
                    let end = usize::try_from(other.offset as i64 + other_ops[1]).ok()?;
                    let first = self.at(end.checked_sub(2)?, NOP, e + 1, hi)?;
                    self.at(end - 1, NOP, first + 1, hi)?;
                    let next = self.boundary(end, first + 2, hi)?;
                    return Some((
                        Matched::If {
                            conds: (i + 1, j),
                            body: (j, e),
                            otherwise: Some((e + 1, first)),
                        },
                        next,
                    ));
                }
                let x = self.at(target.checked_sub(2)?, ENDIF, j, hi)?;
                (self.insns[x].bytes[1] == 0).then_some((
                    Matched::If {
                        conds: (i + 1, j),
                        body: (j, x),
                        otherwise: None,
                    },
                    x + 1,
                ))
            }
            WHILE => {
                let cond = i + 1;
                if cond >= hi || self.insns[cond].bytes.len() != ops[0] as usize {
                    return None;
                }
                let target = insn.end() + ops[1] as usize;
                let x = self.at(target.checked_sub(2)?, EWHILE, cond + 1, hi)?;
                (self.insns[x].bytes[1] == 0).then_some((
                    Matched::While {
                        cond,
                        body: (cond + 1, x),
                    },
                    x + 1,
                ))
            }
            DO if ops[0] == 0 => {
                let target = insn.end() + ops[1] as usize;
                let next = self.boundary(target, i + 3, hi)?;
                let cond = next - 1;
                let x = cond - 1;
                let edwhile = &self.insns[x];
                (edwhile.bytes[0] == EDWHILE
                    && usize::from(edwhile.bytes[1]) == self.insns[cond].bytes.len())
                .then_some((
                    Matched::Do {
                        body: (i + 1, x),
                        cond,
                    },
                    next,
                ))
            }
            FOR | FOR2 => {
                let (keyword, value) = if insn.bytes[0] == FOR {
                    ("for", ops[2])
                } else {
                    if ops[2] != 0 {
                        return None;
                    }
                    ("for2", ops[3])
                };
                if ops[0] != 0 {
                    return None;
                }
                let target = insn.end() + ops[1] as usize;
                let x = self.at(target.checked_sub(2)?, NEXT, i + 1, hi)?;
                (self.insns[x].bytes[1] == 0).then_some((
                    Matched::For {
                        keyword,
                        value,
                        body: (i + 1, x),
                    },
                    x + 1,
                ))
            }
            SWITCH => {
                let target = insn.end() + ops[1] as usize;
                let x = self.at(target.checked_sub(2)?, ESWITCH, i + 1, hi)?;
                if self.insns[x].bytes[1] != 0 {
                    return None;
                }
                let mut sections = Vec::new();
                let mut k = i + 1;
                while k < x {
                    let section = &self.insns[k];
                    let section_ops = section.operands();
                    match section.bytes[0] {
                        CASE if section_ops[0] == 0 => {
                            let end = section.end() + section_ops[1] as usize;
                            let next = self.boundary(end, k + 1, x)?;
                            if next != x && !matches!(self.insns[next].bytes[0], CASE | DEFAULT) {
                                return None;
                            }
                            sections.push((Some(section_ops[2]), (k + 1, next)));
                            k = next;
                        }
                        DEFAULT if section_ops[0] == 0 => {
                            sections.push((None, (k + 1, x)));
                            k = x;
                        }
                        _ => return None,
                    }
                }
                Some((
                    Matched::Switch {
                        value: ops[0],
                        sections,
                    },
                    x + 1,
                ))
            }
            _ => None,
        }
    }
}

fn line(out: &mut String, depth: usize, text: &str) {
    let _ = writeln!(out, "{}{text}", "    ".repeat(depth));
}

/// `name(args)`, with `@` for a block opcode and `;` when a statement.
fn call(insn: &Insn, statement: bool) -> String {
    let op = insn.op();
    let args: Vec<String> = insn.operands().iter().map(i64::to_string).collect();
    let at = if is_block_op(op.op) { "@" } else { "" };
    let end = if statement { ";" } else { "" };
    format!("{at}{}({}){end}", op.mnemonic, args.join(", "))
}

// ---- compiler --------------------------------------------------------------

/// Compile `.akscr` source into scripts.
pub fn compile(text: &str) -> Result<Scripts> {
    let tokens = lex(text)?;
    let mut parser = Parser { tokens, pos: 0 };
    let mut scripts = Scripts::default();
    while !parser.done() {
        let line = parser.line();
        parser.keyword("proc")?;
        let name = parser.ident()?;
        let (list, index) = name
            .split_once('_')
            .and_then(|(list, index)| Some((list, usize::from_str_radix(index, 16).ok()?)))
            .ok_or_else(|| anyhow!("line {line}: `{name}` is not `init_NN` or `main_NN`"))?;
        let target: &mut ProcList = match list {
            "init" if scripts.main.procs.is_empty() => &mut scripts.init,
            "init" => bail!("line {line}: `proc {name}` must come before the main procedures"),
            "main" => &mut scripts.main,
            _ => bail!("line {line}: `{name}` is not `init_NN` or `main_NN`"),
        };
        if index != target.procs.len() {
            bail!(
                "line {line}: expected `proc {list}_{:02X}`; procedures must be dense and \
                 ascending",
                target.procs.len()
            );
        }
        let (body, raw) = parser.proc_body()?;
        let mut bytes = Vec::new();
        for stmt in &body {
            emit(stmt, &mut bytes)?;
        }
        let mut procedure = decode_procedure(&bytes, false);
        procedure.trailing.extend(raw);
        target.procs.push(procedure);
    }
    Ok(scripts)
}

/// Compile one procedure body (the statements between its braces) to its
/// bytes.
pub fn compile_procedure(text: &str) -> Result<Procedure> {
    let compiled = compile(&format!(
        "#version 1\n#game re2\nproc main_00\n{{\n{text}\n}}\n"
    ))?;
    Ok(compiled.main.procs.into_iter().next().unwrap_or_default())
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    Num(i64),
    Punct(char),
    And,
    Or,
}

#[derive(Debug, Clone)]
struct Token {
    tok: Tok,
    line: usize,
}

fn lex(text: &str) -> Result<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut version = false;
    let mut game = false;
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let code = raw.split("//").next().unwrap_or_default().trim();
        if let Some(directive) = code.strip_prefix('#') {
            match directive.split_whitespace().collect::<Vec<_>>().as_slice() {
                ["version", "1"] => version = true,
                ["game", "re2"] => game = true,
                _ => bail!("line {line}: unknown directive `#{directive}`"),
            }
            continue;
        }
        let chars: Vec<char> = code.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c.is_whitespace() {
                i += 1;
                continue;
            }
            if !(version && game) {
                bail!("line {line}: the script must start with `#version 1` and `#game re2`");
            }
            let start = i;
            let tok = if c.is_ascii_alphabetic() || c == '_' || c == '@' {
                i += 1;
                while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                Tok::Ident(chars[start..i].iter().collect())
            } else if c.is_ascii_digit()
                || (c == '-' && chars.get(i + 1).is_some_and(char::is_ascii_digit))
            {
                i += 1;
                while i < chars.len() && chars[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                let text: String = chars[start..i].iter().collect();
                Tok::Num(
                    parse_int(&text)
                        .ok_or_else(|| anyhow!("line {line}: `{text}` is not a number"))?,
                )
            } else if c == '&' && chars.get(i + 1) == Some(&'&') {
                i += 2;
                Tok::And
            } else if c == '|' && chars.get(i + 1) == Some(&'|') {
                i += 2;
                Tok::Or
            } else if "(){},;".contains(c) {
                i += 1;
                Tok::Punct(c)
            } else {
                bail!("line {line}: unexpected `{c}`");
            };
            tokens.push(Token { tok, line });
        }
    }
    if !(version && game) {
        bail!("the script must start with `#version 1` and `#game re2`");
    }
    Ok(tokens)
}

/// One instruction call: its opcode and operand values.
#[derive(Debug, Clone)]
struct Call {
    op: &'static Op,
    args: Vec<i64>,
    line: usize,
}

/// Conditions joined by `&&` (`false`) or `||` (`true`) before each one
/// after the first.
type Conds = Vec<(bool, Call)>;

#[derive(Debug, Clone)]
enum Stmt {
    Call(Call),
    If {
        conds: Conds,
        body: Vec<Stmt>,
        otherwise: Option<Vec<Stmt>>,
    },
    While {
        conds: Conds,
        body: Vec<Stmt>,
    },
    Do {
        body: Vec<Stmt>,
        conds: Conds,
    },
    For {
        op: u8,
        value: i64,
        body: Vec<Stmt>,
        line: usize,
    },
    Switch {
        value: i64,
        sections: Vec<(Option<i64>, Vec<Stmt>)>,
        line: usize,
    },
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn done(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn line(&self) -> usize {
        self.tokens
            .get(self.pos)
            .or(self.tokens.last())
            .map_or(0, |token| token.line)
    }

    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.pos).map(|token| &token.tok)
    }

    fn next(&mut self) -> Result<Tok> {
        let token = self
            .tokens
            .get(self.pos)
            .ok_or_else(|| anyhow!("unexpected end of script"))?;
        self.pos += 1;
        Ok(token.tok.clone())
    }

    fn expect(&mut self, c: char) -> Result<()> {
        let line = self.line();
        match self.next()? {
            Tok::Punct(p) if p == c => Ok(()),
            other => bail!("line {line}: expected `{c}`, found {}", describe(&other)),
        }
    }

    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(&Tok::Punct(c)) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn ident(&mut self) -> Result<String> {
        let line = self.line();
        match self.next()? {
            Tok::Ident(name) => Ok(name),
            other => bail!("line {line}: expected a name, found {}", describe(&other)),
        }
    }

    fn keyword(&mut self, word: &str) -> Result<()> {
        let line = self.line();
        let name = self.ident()?;
        if name != word {
            bail!("line {line}: expected `{word}`, found `{name}`");
        }
        Ok(())
    }

    fn number(&mut self) -> Result<i64> {
        let line = self.line();
        match self.next()? {
            Tok::Num(value) => Ok(value),
            other => bail!("line {line}: expected a number, found {}", describe(&other)),
        }
    }

    /// `( number )`.
    fn paren_number(&mut self) -> Result<i64> {
        self.expect('(')?;
        let value = self.number()?;
        self.expect(')')?;
        Ok(value)
    }

    /// A procedure's braced statements and its trailing `raw` bytes.
    fn proc_body(&mut self) -> Result<(Vec<Stmt>, Vec<u8>)> {
        self.expect('{')?;
        let mut stmts = Vec::new();
        let mut raw = Vec::new();
        while !self.eat('}') {
            if !raw.is_empty() {
                bail!(
                    "line {}: `raw(...)` must be the procedure's last statement",
                    self.line()
                );
            }
            if self.peek() == Some(&Tok::Ident("raw".into())) {
                let line = self.line();
                self.pos += 1;
                self.expect('(')?;
                loop {
                    let value = self.number()?;
                    let byte = u8::try_from(value)
                        .map_err(|_| anyhow!("line {line}: `{value}` is not a byte"))?;
                    raw.push(byte);
                    if !self.eat(',') {
                        break;
                    }
                }
                self.expect(')')?;
                self.expect(';')?;
                continue;
            }
            stmts.push(self.stmt()?);
        }
        Ok((stmts, raw))
    }

    fn block(&mut self) -> Result<Vec<Stmt>> {
        self.expect('{')?;
        let mut stmts = Vec::new();
        while !self.eat('}') {
            stmts.push(self.stmt()?);
        }
        Ok(stmts)
    }

    fn stmt(&mut self) -> Result<Stmt> {
        let line = self.line();
        let name = self.ident()?;
        match name.as_str() {
            "if" => {
                self.expect('(')?;
                let conds = if self.eat(')') {
                    Vec::new()
                } else {
                    let conds = self.conds()?;
                    self.expect(')')?;
                    conds
                };
                if conds.iter().any(|(or, _)| *or) {
                    bail!("line {line}: an `if` condition joins with `&&` only");
                }
                let body = self.block()?;
                let otherwise = if self.peek() == Some(&Tok::Ident("else".into())) {
                    self.pos += 1;
                    Some(self.block()?)
                } else {
                    None
                };
                Ok(Stmt::If {
                    conds,
                    body,
                    otherwise,
                })
            }
            "while" => {
                self.expect('(')?;
                let conds = self.conds()?;
                self.expect(')')?;
                Ok(Stmt::While {
                    conds,
                    body: self.block()?,
                })
            }
            "do" => {
                let body = self.block()?;
                self.keyword("while")?;
                self.expect('(')?;
                let conds = self.conds()?;
                self.expect(')')?;
                self.expect(';')?;
                Ok(Stmt::Do { body, conds })
            }
            "for" | "for2" => {
                let value = self.paren_number()?;
                Ok(Stmt::For {
                    op: if name == "for" { FOR } else { FOR2 },
                    value,
                    body: self.block()?,
                    line,
                })
            }
            "switch" => {
                let value = self.paren_number()?;
                self.expect('{')?;
                let mut sections = Vec::new();
                while !self.eat('}') {
                    let section_line = self.line();
                    match self.ident()?.as_str() {
                        "case" => {
                            let value = self.paren_number()?;
                            sections.push((Some(value), self.block()?));
                        }
                        "default" => sections.push((None, self.block()?)),
                        other => bail!(
                            "line {section_line}: expected `case` or `default`, found `{other}`"
                        ),
                    }
                }
                Ok(Stmt::Switch {
                    value,
                    sections,
                    line,
                })
            }
            _ => {
                self.pos -= 1;
                let call = self.call()?;
                self.expect(';')?;
                Ok(Stmt::Call(call))
            }
        }
    }

    fn conds(&mut self) -> Result<Conds> {
        let mut conds = vec![(false, self.call()?)];
        loop {
            let or = match self.peek() {
                Some(Tok::And) => false,
                Some(Tok::Or) => true,
                _ => return Ok(conds),
            };
            self.pos += 1;
            conds.push((or, self.call()?));
        }
    }

    fn call(&mut self) -> Result<Call> {
        let line = self.line();
        let name = self.ident()?;
        let (explicit, mnemonic) = match name.strip_prefix('@') {
            Some(rest) => (true, rest),
            None => (false, name.as_str()),
        };
        let op = opcode::by_mnemonic(mnemonic)
            .ok_or_else(|| anyhow!("line {line}: unknown instruction `{mnemonic}`"))?;
        if is_block_op(op.op) && !explicit {
            bail!(
                "line {line}: `{mnemonic}` is a block opcode; use its structured form or \
                 `@{mnemonic}(...)`"
            );
        }
        self.expect('(')?;
        let mut args = Vec::new();
        if !self.eat(')') {
            loop {
                args.push(self.number()?);
                if self.eat(')') {
                    break;
                }
                self.expect(',')?;
            }
        }
        Ok(Call { op, args, line })
    }
}

fn describe(tok: &Tok) -> String {
    match tok {
        Tok::Ident(name) => format!("`{name}`"),
        Tok::Num(value) => format!("`{value}`"),
        Tok::Punct(c) => format!("`{c}`"),
        Tok::And => "`&&`".into(),
        Tok::Or => "`||`".into(),
    }
}

fn emit_call(call: &Call, out: &mut Vec<u8>) -> Result<()> {
    let bytes = opcode::encode(call.op, &call.args)
        .map_err(|error| anyhow!("line {}: {error}", call.line))?;
    out.extend(bytes);
    Ok(())
}

/// Write the length from `from` to the end of `out` as a `u16` at `at`.
fn patch_len(out: &mut [u8], at: usize, from: usize, line: usize) -> Result<()> {
    let value = out.len() - from;
    let value = u16::try_from(value)
        .map_err(|_| anyhow!("line {line}: the block is {value} bytes, over 65535"))?;
    out[at..at + 2].copy_from_slice(&value.to_le_bytes());
    Ok(())
}

/// Emit a `while`/`edwhile` condition region: the first condition, then a
/// `u16` join word (0 `&&`, 1 `||`) before each further one, as the
/// executable's `While_main` reads it. Returns the region's length.
fn emit_region(conds: &Conds, out: &mut Vec<u8>) -> Result<usize> {
    let start = out.len();
    for (index, (or, call)) in conds.iter().enumerate() {
        if index > 0 {
            out.extend_from_slice(&u16::from(*or).to_le_bytes());
        }
        emit_call(call, out)?;
    }
    Ok(out.len() - start)
}

fn region_len(len: usize, line: usize) -> Result<u8> {
    u8::try_from(len).map_err(|_| anyhow!("line {line}: the condition is {len} bytes, over 255"))
}

fn emit(stmt: &Stmt, out: &mut Vec<u8>) -> Result<()> {
    match stmt {
        Stmt::Call(call) => emit_call(call, out)?,
        Stmt::If {
            conds,
            body,
            otherwise,
        } => {
            let line = conds.first().map_or(0, |(_, call)| call.line);
            let at = out.len();
            out.extend_from_slice(&[IF, 0, 0, 0]);
            for (_, call) in conds {
                emit_call(call, out)?;
            }
            for stmt in body {
                emit(stmt, out)?;
            }
            match otherwise {
                Some(otherwise) => {
                    let else_at = out.len();
                    out.extend_from_slice(&[ELSE, 0, 0, 0]);
                    patch_len(out, at + 2, at + 4, line)?;
                    for stmt in otherwise {
                        emit(stmt, out)?;
                    }
                    out.extend_from_slice(&[NOP, NOP]);
                    let jump = i16::try_from(out.len() - else_at)
                        .map_err(|_| anyhow!("line {line}: the else block is over 32767 bytes"))?;
                    out[else_at + 2..else_at + 4].copy_from_slice(&jump.to_le_bytes());
                }
                None => {
                    out.extend_from_slice(&[ENDIF, 0]);
                    patch_len(out, at + 2, at + 4, line)?;
                }
            }
        }
        Stmt::While { conds, body } => {
            let line = conds[0].1.line;
            let at = out.len();
            out.extend_from_slice(&[WHILE, 0, 0, 0]);
            let len = emit_region(conds, out)?;
            out[at + 1] = region_len(len, line)?;
            for stmt in body {
                emit(stmt, out)?;
            }
            out.extend_from_slice(&[EWHILE, 0]);
            patch_len(out, at + 2, at + 4, line)?;
        }
        Stmt::Do { body, conds } => {
            let line = conds[0].1.line;
            let at = out.len();
            out.extend_from_slice(&[DO, 0, 0, 0]);
            for stmt in body {
                emit(stmt, out)?;
            }
            let edwhile = out.len();
            out.extend_from_slice(&[EDWHILE, 0]);
            let len = emit_region(conds, out)?;
            out[edwhile + 1] = region_len(len, line)?;
            patch_len(out, at + 2, at + 4, line)?;
        }
        Stmt::For {
            op,
            value,
            body,
            line,
        } => {
            let at = out.len();
            let header = if *op == FOR {
                opcode::encode(&opcode::OPS[usize::from(FOR)], &[0, 0, *value])
            } else {
                opcode::encode(&opcode::OPS[usize::from(FOR2)], &[0, 0, 0, *value])
            }
            .map_err(|error| anyhow!("line {line}: {error}"))?;
            out.extend(header);
            for stmt in body {
                emit(stmt, out)?;
            }
            out.extend_from_slice(&[NEXT, 0]);
            patch_len(out, at + 2, at + 6, *line)?;
        }
        Stmt::Switch {
            value,
            sections,
            line,
        } => {
            let at = out.len();
            let header = opcode::encode(&opcode::OPS[usize::from(SWITCH)], &[*value, 0])
                .map_err(|error| anyhow!("line {line}: {error}"))?;
            out.extend(header);
            for (index, (value, body)) in sections.iter().enumerate() {
                match value {
                    Some(value) => {
                        let case = out.len();
                        let header =
                            opcode::encode(&opcode::OPS[usize::from(CASE)], &[0, 0, *value])
                                .map_err(|error| anyhow!("line {line}: {error}"))?;
                        out.extend(header);
                        for stmt in body {
                            emit(stmt, out)?;
                        }
                        patch_len(out, case + 2, case + 6, *line)?;
                    }
                    None => {
                        if index + 1 != sections.len() {
                            bail!("line {line}: `default` must be the switch's last section");
                        }
                        out.extend_from_slice(&[DEFAULT, 0]);
                        for stmt in body {
                            emit(stmt, out)?;
                        }
                    }
                }
            }
            out.extend_from_slice(&[ESWITCH, 0]);
            patch_len(out, at + 2, at + 4, *line)?;
        }
    }
    Ok(())
}
