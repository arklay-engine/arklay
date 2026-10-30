//! Lifting a merged script's variant `#if` lines into run-time tests.
//!
//! The exporter ([`super::merge`]) lines up a room's RDT variants and keeps
//! each variant's lines with `#if CHRIS`-style blocks. [`lift`] turns that
//! form into the one the checked-in scripts use, which keeps every variant in
//! one bytecode:
//!
//! - a block inside a section becomes `if (CHRIS) { ... } else { ... }`;
//! - a section only some RDTs carry gets `when (...)`, and a header whose
//!   `noreturn` or `nopad` differs per variant gets `noreturn (...)`;
//! - every event gets one name. The exporter names an event per RDT, so one
//!   name may stand for different events in different variants; each event
//!   keeps a name one of its variants used where it can, and a reference
//!   whose event differs per variant is split into a variant test.
//!
//! Built for any one variant, the lifted script gives the same bytecode as
//! the merged one.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::{Context, Result, bail};

use super::preprocess::{Region, Variant};

/// A set of variants, one bit per index of [`Lift::variants`].
type Set = u32;

/// A merged script's lines, with the variant `#if` blocks as nodes.
#[derive(Debug, Clone)]
enum Node {
    Line(String),
    Cond {
        /// The `#if` symbol, with any `!`.
        symbol: String,
        then: Vec<Node>,
        otherwise: Vec<Node>,
    },
}

/// One open variant `#if` while reading the lines: its symbol, its branches
/// and whether `#else` was seen.
struct Frame {
    symbol: String,
    then: Vec<Node>,
    otherwise: Vec<Node>,
    in_else: bool,
}

/// Read `text` into nodes.
fn parse(text: &str) -> Result<Vec<Node>> {
    let mut root = Vec::new();
    let mut stack: Vec<Frame> = Vec::new();
    fn current<'a>(root: &'a mut Vec<Node>, stack: &'a mut [Frame]) -> &'a mut Vec<Node> {
        match stack.last_mut() {
            Some(frame) if frame.in_else => &mut frame.otherwise,
            Some(frame) => &mut frame.then,
            None => root,
        }
    }
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        let line_no = index + 1;
        if let Some(rest) = trimmed.strip_prefix("#if")
            && (rest.is_empty() || rest.starts_with(char::is_whitespace))
        {
            stack.push(Frame {
                symbol: rest.trim().to_string(),
                then: Vec::new(),
                otherwise: Vec::new(),
                in_else: false,
            });
            continue;
        }
        match trimmed.split_whitespace().next() {
            Some("#else") => match stack.last_mut() {
                Some(frame) => frame.in_else = true,
                None => bail!("line {line_no}: `#else` without `#if`"),
            },
            Some("#endif") => match stack.pop() {
                Some(Frame {
                    symbol,
                    then,
                    otherwise,
                    ..
                }) => current(&mut root, &mut stack).push(Node::Cond {
                    symbol,
                    then,
                    otherwise,
                }),
                None => bail!("line {line_no}: `#endif` without `#if`"),
            },
            _ => current(&mut root, &mut stack).push(Node::Line(line.to_string())),
        }
    }
    if !stack.is_empty() {
        bail!("`#if` is not closed by `#endif`");
    }
    Ok(root)
}

/// The change in brace depth across `line`, outside a `//` comment.
fn brace_delta(line: &str) -> i32 {
    let code = line.split("//").next().unwrap_or("");
    code.chars()
        .map(|c| match c {
            '{' => 1,
            '}' => -1,
            _ => 0,
        })
        .sum()
}

/// The brace depth change across `nodes`, checking every variant block is
/// balanced.
fn nodes_delta(nodes: &[Node]) -> Result<i32> {
    let mut delta = 0;
    for node in nodes {
        match node {
            Node::Line(line) => delta += brace_delta(line),
            Node::Cond {
                symbol,
                then,
                otherwise,
            } => {
                if nodes_delta(then)? != 0 || nodes_delta(otherwise)? != 0 {
                    bail!("an `#if {symbol}` block inside a section is not balanced");
                }
            }
        }
    }
    Ok(delta)
}

/// A top-level item in file order.
#[derive(Debug)]
enum Top {
    /// A line outside every section, for the variants of the set.
    Line(Set, String),
    /// A section header line, for the variants of the set.
    Header(Set, String),
    /// A section body, `{` through `}`, for the variants of the set.
    Body(Set, Vec<Node>),
}

/// A section: its header per variant and its body.
#[derive(Debug)]
struct Section {
    /// Each header line and the variants it heads the body for.
    headers: Vec<(Set, String)>,
    /// The variants whose RDT carries the section.
    set: Set,
    body: Vec<Node>,
}

/// A parsed section header.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Header {
    /// `proc` or `event`.
    kind: String,
    name: String,
    /// `noreturn` for a proc, `nopad` for an event.
    flag: bool,
}

fn parse_header(line: &str) -> Result<Header> {
    let words: Vec<&str> = line.split_whitespace().collect();
    match words.as_slice() {
        [kind @ ("proc" | "event"), name] => Ok(Header {
            kind: kind.to_string(),
            name: name.to_string(),
            flag: false,
        }),
        [kind @ "proc", name, "noreturn"] | [kind @ "event", name, "nopad"] => Ok(Header {
            kind: kind.to_string(),
            name: name.to_string(),
            flag: true,
        }),
        _ => bail!("unexpected section header `{}`", line.trim()),
    }
}

/// One entry of the lifted file.
enum Entry {
    Line(String),
    Section(Section),
}

/// The lifting state of one script.
struct Lift {
    /// The variants the script covers.
    variants: Vec<Variant>,
}

impl Lift {
    fn all(&self) -> Set {
        (1 << self.variants.len()) - 1
    }

    /// The variants for which `#if symbol` holds.
    fn symbol_set(&self, symbol: &str) -> Result<Set> {
        let (negated, name) = match symbol.strip_prefix('!') {
            Some(name) => (true, name),
            None => (false, symbol),
        };
        let mut set = 0;
        for (index, variant) in self.variants.iter().enumerate() {
            let Some(defined) = variant.defines(name) else {
                bail!("unknown symbol `{symbol}`");
            };
            if defined != negated {
                set |= 1 << index;
            }
        }
        Ok(set)
    }

    fn members(&self, set: Set) -> impl Iterator<Item = (usize, Variant)> + '_ {
        self.variants
            .iter()
            .copied()
            .enumerate()
            .filter(move |(index, _)| set & (1 << index) != 0)
    }

    /// Flatten the top level of `nodes` (at brace depth 0) for `set`.
    fn flatten(&self, nodes: &[Node], set: Set, out: &mut Vec<Top>) -> Result<()> {
        let mut index = 0;
        while index < nodes.len() {
            match &nodes[index] {
                Node::Cond {
                    symbol,
                    then,
                    otherwise,
                } => {
                    let cond = self.symbol_set(symbol)?;
                    self.flatten(then, set & cond, out)?;
                    self.flatten(otherwise, set & !cond, out)?;
                    index += 1;
                }
                Node::Line(line) => {
                    let trimmed = line.trim();
                    if trimmed.starts_with("proc ") || trimmed.starts_with("event ") {
                        out.push(Top::Header(set, line.clone()));
                        index += 1;
                    } else if brace_delta(line) > 0 {
                        // A body: through the line that closes it.
                        let mut depth = 0;
                        let start = index;
                        loop {
                            let Some(node) = nodes.get(index) else {
                                bail!("a section body is not closed in its `#if` block");
                            };
                            depth += nodes_delta(std::slice::from_ref(node))?;
                            index += 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        out.push(Top::Body(set, nodes[start..index].to_vec()));
                    } else {
                        out.push(Top::Line(set, line.clone()));
                        index += 1;
                    }
                }
            }
        }
        Ok(())
    }

    /// The file's entries: the lines outside sections and the sections.
    fn entries(&self, nodes: &[Node]) -> Result<Vec<Entry>> {
        let mut top = Vec::new();
        self.flatten(nodes, self.all(), &mut top)?;
        let mut entries = Vec::new();
        let mut headers: Vec<(Set, String)> = Vec::new();
        for item in top {
            match item {
                Top::Header(set, line) => {
                    if set != 0 {
                        headers.push((set, line));
                    }
                }
                Top::Body(set, body) => {
                    if set == 0 {
                        continue;
                    }
                    // The body's headers are the pending ones of its
                    // variants; another variant's header may still wait for
                    // a later body.
                    let (own, rest): (Vec<_>, Vec<_>) =
                        headers.drain(..).partition(|(header, _)| header & set != 0);
                    headers = rest;
                    let covered = own.iter().fold(0, |acc, (set, _)| acc | set);
                    let overlap = own
                        .iter()
                        .enumerate()
                        .any(|(i, (a, _))| own[i + 1..].iter().any(|(b, _)| a & b != 0));
                    if covered != set || overlap {
                        bail!(
                            "the section before `{}` does not have exactly one header per variant",
                            body_first_line(&body)
                        );
                    }
                    entries.push(Entry::Section(Section {
                        headers: own,
                        set,
                        body,
                    }));
                }
                Top::Line(set, line) => {
                    let trimmed = line.trim();
                    if !headers.is_empty() && !trimmed.is_empty() && !trimmed.starts_with("//") {
                        bail!("a line between a section header and its body: `{line}`");
                    }
                    if set != self.all() && !trimmed.is_empty() && !trimmed.starts_with("//") {
                        bail!("a line outside the sections is variant-dependent: `{trimmed}`");
                    }
                    if set != 0 {
                        entries.push(Entry::Line(line));
                    }
                }
            }
        }
        if !headers.is_empty() {
            bail!("a section header has no body");
        }
        Ok(entries)
    }

    /// The header `section` has for `variant`.
    fn header_for(&self, section: &Section, index: usize) -> Result<Header> {
        let (_, line) = section
            .headers
            .iter()
            .find(|(set, _)| set & (1 << index) != 0)
            .context("a variant has no header")?;
        parse_header(line)
    }

    /// One name per event, and each variant's map from its own event names
    /// to them.
    #[allow(clippy::type_complexity)]
    fn name_events(
        &self,
        entries: &[Entry],
    ) -> Result<(Vec<Option<String>>, Vec<HashMap<String, String>>)> {
        let sections: Vec<&Section> = entries
            .iter()
            .filter_map(|entry| match entry {
                Entry::Section(section) => Some(section),
                Entry::Line(_) => None,
            })
            .collect();
        // Each variant's own name and index for each event section.
        let mut own: Vec<BTreeMap<usize, (String, usize)>> = Vec::new();
        for (index, _) in self.variants.iter().enumerate() {
            let mut events = BTreeMap::new();
            let mut count = 0;
            for (position, section) in sections.iter().enumerate() {
                if section.set & (1 << index) == 0 {
                    continue;
                }
                let header = self.header_for(section, index)?;
                if header.kind == "event" {
                    events.insert(position, (header.name, count));
                    count += 1;
                }
            }
            own.push(events);
        }
        let mut taken: BTreeSet<String> = BTreeSet::new();
        let every_name: BTreeSet<String> = own
            .iter()
            .flat_map(|events| events.values().map(|(name, _)| name.clone()))
            .collect();
        let mut names: Vec<Option<String>> = Vec::new();
        for (position, section) in sections.iter().enumerate() {
            if self
                .header_for(section, section.set.trailing_zeros() as usize)?
                .kind
                != "event"
            {
                names.push(None);
                continue;
            }
            // The section's index and name in each variant carrying it.
            let uses: Vec<(String, usize)> = own
                .iter()
                .filter_map(|events| events.get(&position).cloned())
                .collect();
            // A generic `event_XX` name is index XX in a variant's build, so
            // it stays only where every variant has the event there.
            let fits = |name: &str| {
                let generic = name
                    .strip_prefix("event_")
                    .filter(|hex| hex.len() == 2)
                    .and_then(|hex| usize::from_str_radix(hex, 16).ok());
                generic.is_none_or(|index| uses.iter().all(|(_, at)| *at == index))
            };
            let mut chosen = None;
            for (name, _) in &uses {
                if !taken.contains(name) && fits(name) {
                    chosen = Some(name.clone());
                    break;
                }
            }
            let name = match chosen {
                Some(name) => name,
                None => {
                    // The first variant's name, marked with the variants
                    // carrying the event: `on_enter_jill`.
                    let base = &uses[0].0;
                    let tag = self
                        .condition(section.set, self.all())
                        .unwrap_or_default()
                        .to_ascii_lowercase()
                        .replace(" && ", "_")
                        .replace('!', "not_");
                    let tagged = (!tag.is_empty()).then(|| format!("{base}_{tag}"));
                    tagged
                        .into_iter()
                        .chain((2..).map(|n| format!("{base}_v{n}")))
                        .find(|name| {
                            !taken.contains(name) && !every_name.contains(name) && fits(name)
                        })
                        .expect("an unused name exists")
                }
            };
            taken.insert(name.clone());
            names.push(Some(name));
        }
        // Each variant's own names, and its event numbers as `#N` (an
        // `evt_chain` names its event by number), to the new names.
        let maps = own
            .iter()
            .map(|events| {
                events
                    .iter()
                    .flat_map(|(position, (name, index))| {
                        let new = names[*position].clone().expect("an event has a name");
                        [(name.clone(), new.clone()), (format!("#{index}"), new)]
                    })
                    .collect()
            })
            .collect();
        Ok((names, maps))
    }

    /// `line` with each event name replaced by its new name in `map`, and an
    /// `evt_chain`'s event number by its name.
    fn rename(line: &str, map: &HashMap<String, String>) -> String {
        let (code, comment) = match line.find("//") {
            Some(at) => line.split_at(at),
            None => (line, ""),
        };

        let mut out = String::with_capacity(line.len());
        let mut word = String::new();
        let flush = |word: &mut String, out: &mut String| {
            match map.get(word.as_str()) {
                Some(name) => out.push_str(name),
                None => out.push_str(word),
            }
            word.clear();
        };
        for c in code.chars() {
            if c.is_ascii_alphanumeric() || c == '_' {
                word.push(c);
            } else {
                flush(&mut word, &mut out);
                out.push(c);
            }
        }
        flush(&mut word, &mut out);
        // A number is no name, so the chained event's is put in last.
        if let Some(at) = out.find("evt_chain(")
            && let Some(close) = out[at..].find(')')
        {
            let number = &out[at + "evt_chain(".len()..at + close];
            let index = super::preprocess::parse_number(number.trim());
            if let Some(name) = index.and_then(|index| map.get(&format!("#{index}"))) {
                out = format!("{}evt_chain({name}){}", &out[..at], &out[at + close + 1..]);
            }
        }
        out.push_str(comment);
        out
    }

    /// The body `nodes` with event references renamed for the variants of
    /// `set`, splitting a statement whose event differs per variant.
    fn rename_nodes(
        &self,
        nodes: &[Node],
        set: Set,
        maps: &[HashMap<String, String>],
        in_asm: &mut bool,
    ) -> Result<Vec<Node>> {
        let mut out = Vec::new();
        for node in nodes {
            match node {
                Node::Cond {
                    symbol,
                    then,
                    otherwise,
                } => {
                    let cond = self.symbol_set(symbol)?;
                    out.push(Node::Cond {
                        symbol: symbol.clone(),
                        then: self.rename_nodes(then, set & cond, maps, in_asm)?,
                        otherwise: self.rename_nodes(otherwise, set & !cond, maps, in_asm)?,
                    });
                }
                Node::Line(line) => {
                    let trimmed = line.trim();
                    let texts: BTreeMap<usize, String> = self
                        .members(set)
                        .map(|(index, _)| (index, Self::rename(line, &maps[index])))
                        .collect();
                    let distinct: BTreeSet<&String> = texts.values().collect();
                    if distinct.len() <= 1 {
                        out.push(Node::Line(
                            distinct
                                .into_iter()
                                .next()
                                .cloned()
                                .unwrap_or_else(|| line.clone()),
                        ));
                    } else if *in_asm || !trimmed.ends_with(';') {
                        bail!(
                            "`{trimmed}` names a different event per variant and cannot be split"
                        );
                    } else {
                        out.extend(self.split(set, &texts));
                    }
                    if trimmed == "asm" || trimmed == "asm {" {
                        *in_asm = true;
                    } else if *in_asm && trimmed == "}" {
                        *in_asm = false;
                    }
                }
            }
        }
        Ok(out)
    }

    /// Variant blocks giving each variant of `set` its line of `texts`.
    fn split(&self, set: Set, texts: &BTreeMap<usize, String>) -> Vec<Node> {
        let distinct: BTreeSet<&String> = self
            .members(set)
            .filter_map(|(index, _)| texts.get(&index))
            .collect();
        if distinct.len() <= 1 {
            return distinct
                .into_iter()
                .map(|text| Node::Line(text.clone()))
                .collect();
        }
        let tests: [(&str, Set); 4] = [
            ("CHRIS", self.symbol_set("CHRIS").unwrap_or(0)),
            ("MANSION1", self.symbol_set("MANSION1").unwrap_or(0)),
            ("USA", self.symbol_set("USA").unwrap_or(0)),
            ("JPN", self.symbol_set("JPN").unwrap_or(0)),
        ];
        for (symbol, cond) in tests {
            if set & cond != 0 && set & !cond != 0 {
                return vec![Node::Cond {
                    symbol: symbol.to_string(),
                    then: self.split(set & cond, texts),
                    otherwise: self.split(set & !cond, texts),
                }];
            }
        }
        unreachable!("variants with different lines differ in some symbol")
    }

    /// The condition holding for `subset` of `within`, as `&&`-joined
    /// variant names; `None` when no such condition exists. An empty string
    /// is a condition every variant of `within` meets.
    fn condition(&self, subset: Set, within: Set) -> Option<String> {
        // Each dimension's values among `subset`, and the variants of
        // `within` that product picks.
        let player = |v: &Variant| v.jill;
        let mansion = |v: &Variant| v.mansion;
        let region = |v: &Variant| v.region;
        let players: BTreeSet<bool> = self.members(subset).map(|(_, v)| player(&v)).collect();
        let mansions: BTreeSet<Option<u8>> =
            self.members(subset).map(|(_, v)| mansion(&v)).collect();
        let regions: BTreeSet<Region> = self.members(subset).map(|(_, v)| region(&v)).collect();
        let product: Set = self
            .members(within)
            .filter(|(_, v)| {
                players.contains(&player(v))
                    && mansions.contains(&mansion(v))
                    && regions.contains(&region(v))
            })
            .fold(0, |acc, (index, _)| acc | 1 << index);
        if product != subset {
            return None;
        }
        let mut terms = Vec::new();
        let all_players: BTreeSet<bool> = self.members(within).map(|(_, v)| player(&v)).collect();
        if players != all_players {
            terms.push(
                if players.contains(&true) {
                    "JILL"
                } else {
                    "CHRIS"
                }
                .to_string(),
            );
        }
        let all_mansions: BTreeSet<Option<u8>> =
            self.members(within).map(|(_, v)| mansion(&v)).collect();
        if mansions != all_mansions {
            if mansions.len() != 1 {
                return None;
            }
            terms.push(match mansions.first() {
                Some(Some(1)) => "MANSION1".to_string(),
                Some(Some(_)) => "MANSION2".to_string(),
                _ => return None,
            });
        }
        let all_regions: BTreeSet<Region> = self.members(within).map(|(_, v)| region(&v)).collect();
        if regions != all_regions {
            match regions.len() {
                1 => terms.push(regions.first().expect("one region").symbol().to_string()),
                _ => {
                    let missing: Vec<&Region> = all_regions.difference(&regions).collect();
                    for region in missing {
                        terms.push(format!("!{}", region.symbol()));
                    }
                }
            }
        }
        Some(terms.join(" && "))
    }

    /// The lifted lines of body `nodes`, for the variants of `set`.
    fn emit(&self, nodes: &[Node], set: Set, out: &mut Vec<String>) -> Result<()> {
        let mut index = 0;
        while index < nodes.len() {
            match &nodes[index] {
                Node::Line(line) => out.push(line.clone()),
                Node::Cond {
                    symbol,
                    then,
                    otherwise,
                } => {
                    let cond = self.symbol_set(symbol)?;
                    let (then_set, else_set) = (set & cond, set & !cond);
                    if then_set == 0 {
                        self.emit(otherwise, set, out)?;
                    } else if else_set == 0 {
                        self.emit(then, set, out)?;
                    } else {
                        self.emit_if(symbol, then, otherwise, set, out)?;
                    }
                }
            }
            index += 1;
        }
        Ok(())
    }

    /// One variant block as an `if`, for the variants of `set` (both
    /// branches have some).
    fn emit_if(
        &self,
        symbol: &str,
        then: &[Node],
        otherwise: &[Node],
        set: Set,
        out: &mut Vec<String>,
    ) -> Result<()> {
        let cond = self.symbol_set(symbol)?;
        let mut then_lines = Vec::new();
        self.emit(then, set & cond, &mut then_lines)?;
        let mut else_lines = Vec::new();
        self.emit(otherwise, set & !cond, &mut else_lines)?;
        let has = |lines: &[String]| lines.iter().any(|line| !line.trim().is_empty());
        let (mut test, mut then_lines, else_lines) = match (has(&then_lines), has(&else_lines)) {
            (false, false) => return Ok(()),
            (true, _) => (symbol.to_string(), then_lines, else_lines),
            (false, true) => (negate(symbol), else_lines, Vec::new()),
        };
        let indent = indent_of(then_lines.iter().chain(&else_lines));
        // `if (A) { if (B) { ... } }` with nothing else reads as one test.
        if !has(&else_lines)
            && let Some((inner, body)) = sole_if(&then_lines, &indent)
        {
            test = format!("{test} && {inner}");
            then_lines = body;
        }
        out.push(format!("{indent}if ({test})"));
        out.push(format!("{indent}{{"));
        out.extend(then_lines.iter().map(|line| indented(line)));
        out.push(format!("{indent}}}"));
        if has(&else_lines) {
            out.push(format!("{indent}else"));
            // `else { if (B) { ... } }` with nothing else reads as `else if`.
            match sole_chain(&else_lines, &indent) {
                Some(chain) => {
                    let last = out.len() - 1;
                    out[last] = format!("{indent}else {}", chain[0].trim_start());
                    out.extend(chain[1..].iter().cloned());
                }
                None => {
                    out.push(format!("{indent}{{"));
                    out.extend(else_lines.iter().map(|line| indented(line)));
                    out.push(format!("{indent}}}"));
                }
            }
        }
        Ok(())
    }

    /// The lifted header line of `section`, named `name` for an event.
    fn header_line(&self, section: &Section, name: Option<&str>) -> Result<String> {
        let mut header = None;
        let mut flagged = 0;
        for (index, _) in self.members(section.set) {
            let variant_header = self.header_for(section, index)?;
            if variant_header.flag {
                flagged |= 1 << index;
            }
            match &header {
                None => header = Some(variant_header),
                Some(first) => {
                    if first.kind != variant_header.kind
                        || (first.kind == "proc" && first.name != variant_header.name)
                    {
                        bail!(
                            "a section is `{} {}` in one variant and `{} {}` in another",
                            first.kind,
                            first.name,
                            variant_header.kind,
                            variant_header.name
                        );
                    }
                }
            }
        }
        let header = header.context("a section without variants")?;
        let mut line = format!("{} {}", header.kind, name.unwrap_or(&header.name));
        if flagged != 0 {
            let flag = if header.kind == "proc" {
                "noreturn"
            } else {
                "nopad"
            };
            let Some(condition) = self.condition(flagged, section.set) else {
                bail!("`{line}` is `{flag}` for variants no condition names");
            };
            line.push(' ');
            line.push_str(flag);
            if !condition.is_empty() {
                line.push_str(&format!(" ({condition})"));
            }
        }
        if section.set != self.all() {
            let Some(condition) = self.condition(section.set, self.all()) else {
                bail!("`{line}` is in RDT variants no condition names");
            };
            line.push_str(&format!(" when ({condition})"));
        }
        Ok(line)
    }
}

/// The first line of a section body, for an error message.
fn body_first_line(body: &[Node]) -> String {
    body.iter()
        .find_map(|node| match node {
            Node::Line(line) if !line.trim().is_empty() && line.trim() != "{" => {
                Some(line.trim().to_string())
            }
            _ => None,
        })
        .unwrap_or_default()
}

/// The opposite test of `symbol`.
fn negate(symbol: &str) -> String {
    match symbol {
        "CHRIS" => "JILL".to_string(),
        "JILL" => "CHRIS".to_string(),
        _ => match symbol.strip_prefix('!') {
            Some(name) => name.to_string(),
            None => format!("!{symbol}"),
        },
    }
}

/// The common leading whitespace of the non-blank `lines`.
fn indent_of<'a>(lines: impl Iterator<Item = &'a String>) -> String {
    lines
        .filter(|line| !line.trim().is_empty())
        .map(|line| &line[..line.len() - line.trim_start().len()])
        .min_by_key(|indent| indent.len())
        .unwrap_or("")
        .to_string()
}

/// `line` one level deeper; a blank line stays blank.
fn indented(line: &str) -> String {
    if line.trim().is_empty() {
        String::new()
    } else {
        format!("    {line}")
    }
}

/// When `lines` are exactly one variant `if` with no `else`, at `indent`:
/// its test and its body lines (one level shallower).
fn sole_if(lines: &[String], indent: &str) -> Option<(String, Vec<String>)> {
    let first = lines.first()?;
    let test = first
        .strip_prefix(indent)?
        .strip_prefix("if (")?
        .strip_suffix(')')?;
    if !is_variant_test(test) {
        return None;
    }
    if lines.get(1)? != &format!("{indent}{{") || lines.last()? != &format!("{indent}}}") {
        return None;
    }
    let body = &lines[2..lines.len() - 1];
    // The body's own lines are deeper: no other line at `indent`.
    if body
        .iter()
        .any(|line| !line.trim().is_empty() && !line.starts_with(&format!("{indent}    ")))
    {
        return None;
    }
    let body = body
        .iter()
        .map(|line| line.strip_prefix("    ").unwrap_or(line).to_string())
        .collect();
    Some((test.to_string(), body))
}

/// When `lines` are exactly one variant `if` chain at `indent`: the lines.
fn sole_chain(lines: &[String], indent: &str) -> Option<Vec<String>> {
    let first = lines.first()?;
    let test = first
        .strip_prefix(indent)?
        .strip_prefix("if (")?
        .strip_suffix(')')?;
    if !is_variant_test(test) {
        return None;
    }
    // Every other line at `indent` belongs to the chain.
    for line in &lines[1..] {
        let Some(rest) = line.strip_prefix(indent) else {
            if line.trim().is_empty() {
                continue;
            }
            return None;
        };
        if rest.starts_with(char::is_whitespace) || rest.is_empty() {
            continue;
        }
        if !matches!(rest, "{" | "}" | "else") && !rest.starts_with("else if (") {
            return None;
        }
    }
    Some(lines.to_vec())
}

/// Whether `test` is a variant test the lift wrote.
fn is_variant_test(test: &str) -> bool {
    test.split(" && ").all(|term| {
        matches!(
            term.trim_start_matches('!'),
            "CHRIS" | "JILL" | "MANSION1" | "MANSION2" | "USA" | "JPN" | "EUR"
        )
    })
}

/// The variants a merged script for a mansion room (stage 1 or 2) or
/// another room covers, Chris on the first visit in the USA release first.
pub fn variants(mansion: bool) -> Vec<Variant> {
    let visits: &[Option<u8>] = if mansion {
        &[Some(1), Some(2)]
    } else {
        &[None]
    };
    let mut out = Vec::new();
    for &visit in visits {
        for jill in [false, true] {
            for region in Region::ALL {
                out.push(Variant {
                    jill,
                    mansion: visit,
                    region,
                });
            }
        }
    }
    out
}

/// Lift merged script `text`, whose variant `#if` lines cover the variants
/// of a mansion room (`mansion`) or another room, into run-time tests.
pub fn lift(text: &str, mansion: bool) -> Result<String> {
    let lift = Lift {
        variants: variants(mansion),
    };
    let nodes = parse(text)?;
    let mut entries = lift.entries(&nodes)?;
    // Every variant's procs come before its events, but one variant's proc
    // may follow another's events; the runtime build carries them all, so
    // the procs move up before the first event (in order: inits, then
    // mains). Only the order of the events numbers them.
    let is_event = |entry: &Entry| match entry {
        Entry::Section(section) => section
            .headers
            .first()
            .is_some_and(|(_, line)| line.trim_start().starts_with("event ")),
        Entry::Line(_) => false,
    };
    let proc_rank = |entry: &Entry| match entry {
        Entry::Section(section) => section.headers.first().map(|(_, line)| {
            if line.trim_start().starts_with("proc init") {
                0
            } else {
                1
            }
        }),
        Entry::Line(_) => None,
    };
    let ranks: Vec<Option<usize>> = entries
        .iter()
        .map(|entry| {
            if is_event(entry) {
                Some(2)
            } else {
                proc_rank(entry)
            }
        })
        .collect();
    let ordered = ranks.iter().flatten().is_sorted();
    if !ordered && let Some(first_event) = entries.iter().position(is_event) {
        let mut procs = Vec::new();
        let mut kept = Vec::new();
        for (index, entry) in entries.into_iter().enumerate() {
            match ranks[index] {
                Some(rank) if rank < 2 => procs.push((rank, entry)),
                _ => kept.push(entry),
            }
        }
        procs.sort_by_key(|(rank, _)| *rank);
        // The first event's place among the entries left.
        let at = kept.iter().position(is_event).unwrap_or(first_event);
        let mut moved = Vec::new();
        for (_, entry) in procs {
            moved.push(entry);
            moved.push(Entry::Line(String::new()));
        }
        kept.splice(at..at, moved);
        entries = kept;
    }
    let (names, maps) = lift.name_events(&entries)?;
    let mut out = Vec::new();
    let mut position = 0;
    for entry in &entries {
        match entry {
            Entry::Line(line) => out.push(line.clone()),
            Entry::Section(section) => {
                let name = names[position].as_deref();
                position += 1;
                out.push(lift.header_line(section, name)?);
                let mut in_asm = false;
                let body = lift.rename_nodes(&section.body, section.set, &maps, &mut in_asm)?;
                lift.emit(&body, section.set, &mut out)?;
            }
        }
    }
    // No run of blank lines longer than one.
    let mut text = String::with_capacity(out.iter().map(|line| line.len() + 1).sum());
    let mut blank = false;
    for line in out {
        let is_blank = line.trim().is_empty();
        if is_blank && blank {
            continue;
        }
        blank = is_blank;
        text.push_str(if is_blank { "" } else { line.trim_end() });
        text.push('\n');
    }
    Ok(text)
}
