//! `.akscr` preprocessor: `#define` and `#include`, and the older merged
//! form's `#if` / `#else` / `#endif` lines.
//!
//! One script covers every variant of a room, but the variants are not
//! preprocessor symbols: the player (`CHRIS`, `JILL`), the mansion visit
//! (`MANSION1`, `MANSION2`) and the release (`USA`, `JPN`, `EUR`) are tested
//! at run time by `if (JILL) { ... }` statements (see [`super::compile`]).
//! A [`Build`] for one RDT variant folds those tests away, giving that RDT's
//! own bytecode; the runtime build keeps them as variant jumps.
//!
//! The older merged form, which picked the variants with `#if CHRIS`-style
//! lines (`#if !JPN` takes the block when the symbol is not defined; blocks
//! nest), is still read by [`Build::Legacy`]: the exporter writes it before
//! [`super::lift`] turns it into `if` statements. Any other build has no
//! `#if` lines.
//!
//! `#define NAME value` names a number (or another name) for the arguments
//! that follow, and `#include "inc/items.akinc"` reads defines from a file;
//! an include file holds only directives, comments and blank lines, and each
//! file is read once.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// The symbols a [`Build::Legacy`] script's `#if` may test.
pub const SYMBOLS: [&str; 7] = ["CHRIS", "JILL", "MANSION1", "MANSION2", "USA", "JPN", "EUR"];

/// The room variant a script is compiled for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Variant {
    /// The player digit is 1 (Jill) rather than 0 (Chris).
    pub jill: bool,
    /// 1 for the first mansion visit (stages 1/2), 2 for the second (stages
    /// 6/7), `None` outside the mansion.
    pub mansion: Option<u8>,
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

/// What a script is compiled for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Build {
    /// One RDT variant: every `if (JILL)`-style test folds to the branch the
    /// variant takes, so the bytecode is that RDT's own.
    Variant(Variant),
    /// Every variant at once: the tests stay in the bytecode as variant
    /// jumps the engine resolves for the room it runs. This is what ships.
    Runtime,
    /// One RDT variant of a script in the older merged form, whose `#if`
    /// lines may also test the variant symbols ([`SYMBOLS`]).
    Legacy(Variant),
}

impl Build {
    /// The variant a build folds to, `None` for the runtime build.
    pub fn variant(self) -> Option<Variant> {
        match self {
            Build::Variant(variant) | Build::Legacy(variant) => Some(variant),
            Build::Runtime => None,
        }
    }

    /// Whether `#if symbol` holds, `None` for a symbol the build does not
    /// know.
    fn defines(self, symbol: &str) -> Option<bool> {
        match self {
            Build::Legacy(variant) => variant.defines(symbol),
            _ => None,
        }
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
            region: Region::Usa,
        }
    }

    /// This variant for `region`.
    pub fn in_region(self, region: Region) -> Self {
        Self { region, ..self }
    }

    /// The variant's bits for a variant jump's mask
    /// ([`crate::scd::opcode::variant_matches`]): one per dimension.
    pub fn bits(self) -> u8 {
        use crate::scd::opcode::variant;
        let player = if self.jill {
            variant::JILL
        } else {
            variant::CHRIS
        };
        let mansion = match self.mansion {
            Some(1) => variant::MANSION1,
            Some(_) => variant::MANSION2,
            None => variant::OUTSIDE,
        };
        let region = match self.region {
            Region::Usa => variant::USA,
            Region::Jpn => variant::JPN,
            Region::Eur => variant::EUR,
        };
        player | mansion | region
    }

    /// Whether a variant jump's `mask` takes this variant.
    pub fn matches(self, mask: u8) -> bool {
        crate::scd::opcode::variant_matches(mask, self.bits())
    }

    /// Whether `symbol` is defined, `None` for an unknown symbol.
    pub fn defines(self, symbol: &str) -> Option<bool> {
        match symbol {
            "CHRIS" => Some(!self.jill),
            "JILL" => Some(self.jill),
            "MANSION1" => Some(self.mansion == Some(1)),
            "MANSION2" => Some(self.mansion == Some(2)),
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

/// Where the checked-in script of room `base` (`0x103`) sits under
/// `games/re1/room`: in the room's folder, `103/room.akscr`.
pub fn room_script_file(base: u16) -> String {
    format!("{base:03x}/{ROOM_SCRIPT_FILE}")
}

/// The file name of a checked-in room script inside its room folder.
pub const ROOM_SCRIPT_FILE: &str = "room.akscr";

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

/// The lines `build` keeps, with their 1-based line numbers. Without a
/// build the script must not use any directive.
pub(super) fn kept_lines(text: &str, build: Option<Build>) -> Result<Vec<(usize, &str)>> {
    // One entry per open `#if`: (taking the `#if` branch, now in `#else`).
    let mut stack: Vec<(bool, bool)> = Vec::new();
    let mut kept = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line_no = index + 1;
        let live = stack.iter().all(|&(taken, in_else)| taken != in_else);
        let Some(directive) = directive(line) else {
            if live {
                kept.push((line_no, line));
            }
            continue;
        };
        match directive {
            Directive::If(symbol) => {
                let (negated, name) = match symbol.strip_prefix('!') {
                    Some(name) => (true, name),
                    None => (false, symbol),
                };
                let Some(build) = build else {
                    bail!("line {line_no}: this script may not use `#if {symbol}`");
                };
                let Some(defined) = build.defines(name) else {
                    if SYMBOLS.contains(&name) {
                        bail!(
                            "line {line_no}: `#if {symbol}` picks a room variant, which is a \
                             run-time test: write `if ({symbol}) {{ ... }}`"
                        );
                    }
                    bail!("line {line_no}: unknown symbol `{symbol}`");
                };
                stack.push((defined != negated, false));
            }
            Directive::Else => match stack.last_mut() {
                Some((_, in_else @ false)) => *in_else = true,
                Some(_) => bail!("line {line_no}: second `#else` for one `#if`"),
                None => bail!("line {line_no}: `#else` without `#if`"),
            },
            Directive::Endif => {
                if stack.pop().is_none() {
                    bail!("line {line_no}: `#endif` without `#if`");
                }
            }
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

/// Resolve the directives for `build`. Dropped and directive lines become
/// blank, so line numbers in later errors still point into the source.
pub fn preprocess(text: &str, build: Build, load: Loader) -> Result<Preprocessed> {
    let mut state = State {
        defines: HashMap::new(),
        included: HashSet::new(),
        build,
        load,
    };
    let kept = kept_lines(text, Some(build))?;
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
    build: Build,
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
        for (line_no, line) in kept_lines(text, Some(self.build))? {
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
