//! `.akscr` preprocessor: `#if SYMBOL` / `#else` / `#endif` over whole lines.
//!
//! One script covers every variant of a room: the player digit (0 Chris,
//! 1 Jill) and, for the mansion stages, the first visit (stages 1/2) and the
//! second (stages 6/7). The symbols are `CHRIS`, `JILL`, `MANSION1` and
//! `MANSION2`; blocks nest. A room outside the mansion defines neither
//! mansion symbol.
//!
//! `AUTHENTIC` selects the original game's exact bytecode where the shipped
//! data has a quirk: `#if AUTHENTIC` keeps the original, its `#else` holds
//! the fixed version a build uses by default.
//!
//! `USA`, `JPN` and `EUR` pick the region, one defined per build. The PC
//! releases ship three sets of room scripts: the English (USA) one, the
//! Japanese one (the original Mediakite release and the Steam/GOG Japanese
//! build) and the French and German one (EUR), and `#if` blocks on the
//! region hold every difference between them.
//!
//! `#if !SYMBOL` takes the block when `SYMBOL` is not defined.
//!
//! `#define NAME value` names a number (or another name) for the arguments
//! that follow, and `#include "inc/items.akinc"` reads defines from a file;
//! an include file holds only directives, comments and blank lines, and each
//! file is read once.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// The defined symbols.
pub const SYMBOLS: [&str; 8] = [
    "CHRIS",
    "JILL",
    "MANSION1",
    "MANSION2",
    "AUTHENTIC",
    "USA",
    "JPN",
    "EUR",
];

/// The room variant a script is compiled for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Variant {
    /// The player digit is 1 (Jill) rather than 0 (Chris).
    pub jill: bool,
    /// 1 for the first mansion visit (stages 1/2), 2 for the second (stages
    /// 6/7), `None` outside the mansion.
    pub mansion: Option<u8>,
    /// Compile the original game's exact bytecode rather than the fixed
    /// version of its quirks.
    pub authentic: bool,
    /// The region whose scripts to compile.
    pub region: Region,
}

/// The region of a release, which picks its set of room scripts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Region {
    /// The English release, which the checked-in scripts were written from.
    Usa,
    /// The Japanese release: the original Mediakite one and the Steam/GOG
    /// Japanese build.
    Jpn,
    /// The French and German releases.
    Eur,
}

impl Region {
    /// Every region, in the order a merge splits on them.
    pub const ALL: [Region; 3] = [Region::Usa, Region::Jpn, Region::Eur];

    /// The preprocessor symbol the region defines.
    pub fn symbol(self) -> &'static str {
        match self {
            Region::Usa => "USA",
            Region::Jpn => "JPN",
            Region::Eur => "EUR",
        }
    }

    /// The region named `name` (`usa`, `jpn` or `eur`, any case).
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|region| region.symbol().eq_ignore_ascii_case(name))
    }
}

impl std::fmt::Display for Region {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.symbol())
    }
}

impl Variant {
    /// The variant of RDT `id` (`0x1030` is stage 1, room 03, player 0), for
    /// the USA region.
    pub fn for_room(id: u16) -> Self {
        let stage = id >> 12;
        Self {
            jill: id & 0xF == 1,
            mansion: match stage {
                1 | 2 => Some(1),
                6 | 7 => Some(2),
                _ => None,
            },
            authentic: false,
            region: Region::Usa,
        }
    }

    /// This variant for `region`.
    pub fn in_region(self, region: Region) -> Self {
        Self { region, ..self }
    }

    /// This variant compiled to the original game's exact bytecode.
    pub fn authentic(self) -> Self {
        Self {
            authentic: true,
            ..self
        }
    }

    /// Whether `symbol` is defined, `None` for an unknown symbol.
    pub fn defines(self, symbol: &str) -> Option<bool> {
        match symbol {
            "CHRIS" => Some(!self.jill),
            "JILL" => Some(self.jill),
            "MANSION1" => Some(self.mansion == Some(1)),
            "MANSION2" => Some(self.mansion == Some(2)),
            "AUTHENTIC" => Some(self.authentic),
            "USA" => Some(self.region == Region::Usa),
            "JPN" => Some(self.region == Region::Jpn),
            "EUR" => Some(self.region == Region::Eur),
            _ => None,
        }
    }
}

/// The RDT ids a merged script `base` (three hex digits, mansion stages
/// written as 1/2) covers: both players, and stages 6/7 for 1/2.
pub fn room_variants(base: u16) -> Vec<u16> {
    let stage = base >> 8;
    let mut stages = vec![stage];
    if matches!(stage, 1 | 2) {
        stages.push(stage + 5);
    }
    let mut ids = Vec::new();
    for stage in stages {
        for player in 0..2 {
            ids.push((stage << 12) | ((base & 0xFF) << 4) | player);
        }
    }
    ids
}

/// The merged-script id of RDT `id`: stage 6/7 fold onto 1/2 and the player
/// digit is dropped.
pub fn merged_id(id: u16) -> u16 {
    let stage = match id >> 12 {
        6 => 1,
        7 => 2,
        stage => stage,
    };
    (stage << 8) | ((id >> 4) & 0xFF)
}

/// One directive line.
enum Directive<'a> {
    If(&'a str),
    Else,
    Endif,
}

fn directive(line: &str) -> Option<Directive<'_>> {
    let line = line.trim();
    if let Some(rest) = line.strip_prefix("#if")
        && (rest.is_empty() || rest.starts_with(char::is_whitespace))
    {
        return Some(Directive::If(rest.trim()));
    }
    match line.split_whitespace().next() {
        Some("#else") => Some(Directive::Else),
        Some("#endif") => Some(Directive::Endif),
        _ => None,
    }
}

/// The lines `variant` keeps, with their 1-based line numbers. Without a
/// variant the script must not use any directive.
pub(super) fn kept_lines(text: &str, variant: Option<Variant>) -> Result<Vec<(usize, &str)>> {
    kept_lines_with(text, variant, None)
}

/// [`kept_lines`], keeping the `#if keep` blocks as they are, directives
/// and both branches.
pub(super) fn kept_lines_with<'a>(
    text: &'a str,
    variant: Option<Variant>,
    keep: Option<&str>,
) -> Result<Vec<(usize, &'a str)>> {
    // One entry per open `#if`: (taking the `#if` branch, now in `#else`),
    // or `None` for a kept block.
    let mut stack: Vec<Option<(bool, bool)>> = Vec::new();
    let mut kept = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line_no = index + 1;
        let live = stack
            .iter()
            .flatten()
            .all(|&(taken, in_else)| taken != in_else);
        let Some(directive) = directive(line) else {
            if live {
                kept.push((line_no, line));
            }
            continue;
        };
        match directive {
            Directive::If(symbol) if keep == Some(symbol) => {
                stack.push(None);
                if live {
                    kept.push((line_no, line));
                }
            }
            Directive::If(symbol) => {
                let Some(variant) = variant else {
                    bail!(
                        "line {line_no}: `#if {symbol}` needs a room variant to compile for \
                         (e.g. `scd build --room 1030`)"
                    );
                };
                let (negated, name) = match symbol.strip_prefix('!') {
                    Some(name) => (true, name),
                    None => (false, symbol),
                };
                let Some(defined) = variant.defines(name) else {
                    bail!(
                        "line {line_no}: unknown symbol `{symbol}` (expected one of {}, \
                         or `!` before one)",
                        SYMBOLS.join(", ")
                    );
                };
                stack.push(Some((defined != negated, false)));
            }
            Directive::Else => match stack.last_mut() {
                Some(None) => {
                    if live {
                        kept.push((line_no, line));
                    }
                }
                Some(Some((_, in_else @ false))) => *in_else = true,
                Some(Some(_)) => bail!("line {line_no}: second `#else` for one `#if`"),
                None => bail!("line {line_no}: `#else` without `#if`"),
            },
            Directive::Endif => match stack.pop() {
                Some(None) => {
                    if live {
                        kept.push((line_no, line));
                    }
                }
                Some(Some(_)) => {}
                None => bail!("line {line_no}: `#endif` without `#if`"),
            },
        }
    }
    if !stack.is_empty() {
        bail!("`#if` is not closed by `#endif`");
    }
    Ok(kept)
}

/// Reads the text of an `#include` path.
pub type Loader<'a> = &'a dyn Fn(&str) -> Result<String>;

/// A loader for scripts given as text alone: any `#include` is an error.
pub fn no_includes(path: &str) -> Result<String> {
    bail!("`#include \"{path}\"` needs the script's location to resolve it")
}

/// A loader for a script in `dir`: an include path resolves against `dir`
/// and then each directory above it, so `room/103.akscr` finds
/// `inc/items.akinc` beside `room/`.
pub fn includes_from(dir: &Path) -> impl Fn(&str) -> Result<String> + use<> {
    let dir = dir.to_path_buf();
    move |path: &str| {
        let relative = Path::new(path);
        if relative.is_absolute()
            || relative
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            bail!("include path `{path}` must be relative and stay below the script");
        }
        let mut search: Option<&Path> = Some(&dir);
        while let Some(base) = search {
            let candidate: PathBuf = base.join(relative);
            if candidate.is_file() {
                return std::fs::read_to_string(&candidate)
                    .with_context(|| format!("failed to read {}", candidate.display()));
            }
            search = base.parent();
        }
        bail!("include `{path}` not found from {}", dir.display())
    }
}

/// A preprocessed script: the text to parse and the names it defined.
pub struct Preprocessed {
    pub text: String,
    pub defines: HashMap<String, i64>,
}

/// Resolve the directives for `variant`. Dropped and directive lines become
/// blank, so line numbers in later errors still point into the source.
pub fn preprocess(text: &str, variant: Option<Variant>, load: Loader) -> Result<Preprocessed> {
    let mut state = State {
        defines: HashMap::new(),
        included: HashSet::new(),
        variant,
        load,
    };
    let kept = kept_lines(text, variant)?;
    let mut out = String::with_capacity(text.len());
    let mut next = 1usize;
    for (line_no, line) in kept {
        while next < line_no {
            out.push('\n');
            next += 1;
        }
        if !state
            .directive(line)
            .with_context(|| format!("line {line_no}"))?
        {
            out.push_str(line);
        }
        out.push('\n');
        next += 1;
    }
    Ok(Preprocessed {
        text: out,
        defines: state.defines,
    })
}

struct State<'a> {
    defines: HashMap<String, i64>,
    included: HashSet<String>,
    variant: Option<Variant>,
    load: Loader<'a>,
}

impl State<'_> {
    /// Apply an `#include` or `#define` line; `false` for any other line.
    fn directive(&mut self, line: &str) -> Result<bool> {
        let trimmed = line.trim();
        if let Some(rest) = word(trimmed, "#include") {
            let path = rest
                .strip_prefix('"')
                .and_then(|rest| rest.strip_suffix('"'))
                .filter(|path| !path.is_empty())
                .ok_or_else(|| anyhow::anyhow!("expected `#include \"path\"`"))?;
            if self.included.insert(path.to_string()) {
                let text = (self.load)(path)?;
                self.include(&text)
                    .with_context(|| format!("in `{path}`"))?;
            }
            return Ok(true);
        }
        if let Some(rest) = word(trimmed, "#define") {
            let mut parts = rest.split_whitespace();
            let (Some(name), Some(value), None) = (parts.next(), parts.next(), parts.next()) else {
                bail!("expected `#define NAME value`");
            };
            if !crate::scd::asm::is_identifier(name) {
                bail!("`{name}` is not a valid name");
            }
            let value = match parse_number(value) {
                Some(value) => value,
                None => *self.defines.get(value).ok_or_else(|| {
                    anyhow::anyhow!("`{value}` is not a number or a defined name")
                })?,
            };
            if let Some(previous) = self.defines.insert(name.to_string(), value)
                && previous != value
            {
                bail!("`{name}` is already defined as {previous}");
            }
            return Ok(true);
        }
        Ok(false)
    }

    /// Read an include file's defines.
    fn include(&mut self, text: &str) -> Result<()> {
        for (line_no, line) in kept_lines(text, self.variant)? {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("//") {
                continue;
            }
            if !self
                .directive(line)
                .with_context(|| format!("line {line_no}"))?
            {
                bail!("line {line_no}: an include file holds only `#define` and `#include` lines");
            }
        }
        Ok(())
    }
}

/// `rest` after a directive `word` and whitespace.
fn word<'a>(line: &'a str, word: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(word)?;
    (rest.is_empty() || rest.starts_with(char::is_whitespace)).then(|| rest.trim())
}

/// A decimal or `0x` hex number, optionally negative.
pub(super) fn parse_number(text: &str) -> Option<i64> {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(digits) => (true, digits),
        None => (false, text),
    };
    let value = match digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        Some(hex) => i64::from_str_radix(hex, 16).ok()?,
        None => digits.parse().ok()?,
    };
    Some(if negative { -value } else { value })
}
