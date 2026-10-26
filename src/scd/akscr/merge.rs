//! Merge the scripts of a room's variants into one `#if` script.
//!
//! The variants are aligned progressively, section by section first (a
//! `proc` or `event` pairs with the most alike section of the other
//! variants, whatever its number) and then line by line within each section
//! pair (a weighted longest common subsequence), so every merged line knows
//! which variants hold it. Lines every variant in scope holds are written
//! plainly. Each run of the others is grown to whole statements (balanced
//! braces, an `if` with its `else`, a header with its block) and split on
//! the mansion, the player or a region, whichever writes fewer lines (on a
//! tie, in that order), into `#if` / `#else` / `#endif` blocks. A section
//! header alone may differ.

use std::collections::{BTreeMap, HashMap};

use anyhow::{Context, Result, bail, ensure};

use super::preprocess::{Region, Variant, kept_lines_with, merged_id};

/// One bit per merge input.
type Mask = u16;

/// One merged line and the variants (bit per input) that hold it.
#[derive(Clone, Copy)]
struct Entry<'a> {
    line: &'a str,
    mask: Mask,
    /// A section header (`event event_03`), which may differ on its own.
    header: bool,
}

impl<'a> Entry<'a> {
    fn new(line: &'a str, mask: Mask) -> Self {
        Self {
            line,
            mask,
            header: false,
        }
    }
}

/// A top-level piece of a script: a section (its header line, then its
/// braced body) or one line between sections.
struct Unit<'a> {
    header: Option<&'a str>,
    lines: Vec<&'a str>,
}

/// Cut a script into its top-level units.
fn units(text: &str) -> Vec<Unit<'_>> {
    let lines: Vec<&str> = text.lines().collect();
    let mut units = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        if lines.get(index + 1).is_some_and(|next| brace(next) == 1) && brace(line) == 0 {
            let mut end = index + 1;
            let mut depth = 0;
            while end < lines.len() {
                depth += brace(lines[end]);
                end += 1;
                if depth <= 0 {
                    break;
                }
            }
            units.push(Unit {
                header: Some(line),
                lines: lines[index + 1..end].to_vec(),
            });
            index = end;
        } else {
            units.push(Unit {
                header: None,
                lines: vec![line],
            });
            index += 1;
        }
    }
    units
}

/// Merged units: the alternative headers of a section and its merged lines.
struct Group<'a> {
    headers: Vec<Entry<'a>>,
    body: Vec<Entry<'a>>,
}

/// How alike a merged group and a new unit are, if they may pair at all: a
/// line pairs with the same line, a `proc` with a `proc` of the same kind and
/// an event with an event sharing at least half the weight of the smaller.
fn likeness(group: &Group, unit: &Unit) -> Option<u32> {
    let Some(header) = unit.header else {
        return (group.headers.is_empty() && group.body[0].line == unit.lines[0]).then_some(1);
    };
    let kind = |header: &str| {
        let mut words = header.split_whitespace();
        match words.next() {
            Some("event") => "event".to_string(),
            first => format!("{} {}", first.unwrap_or(""), words.next().unwrap_or("")),
        }
    };
    let first = group.headers.first()?;
    if kind(first.line) != kind(header) {
        return None;
    }
    let mut counts: HashMap<&str, i32> = HashMap::new();
    for entry in &group.body {
        *counts.entry(entry.line).or_default() += 1;
    }
    let mut shared = 0;
    for line in &unit.lines {
        if let Some(count) = counts.get_mut(line)
            && *count > 0
        {
            *count -= 1;
            shared += weight(line);
        }
    }
    let total = |lines: &mut dyn Iterator<Item = &str>| lines.map(weight).sum::<u32>();
    let smallest = total(&mut group.body.iter().map(|entry| entry.line))
        .min(total(&mut unit.lines.iter().copied()));
    let proc = !header.starts_with("event");
    (proc || shared * 2 >= smallest).then_some(shared + 1)
}

/// Align the units of one variant (bit `bit`) against the merged groups,
/// pairing the alike ones in order.
fn align_units<'a>(groups: Vec<Group<'a>>, units: Vec<Unit<'a>>, bit: Mask) -> Vec<Group<'a>> {
    let (rows, cols) = (groups.len(), units.len());
    let width = cols + 1;
    let mut likeness_of = vec![None; rows * cols];
    for (i, group) in groups.iter().enumerate() {
        for (j, unit) in units.iter().enumerate() {
            likeness_of[i * cols + j] = likeness(group, unit);
        }
    }
    let mut score = vec![0u32; (rows + 1) * width];
    for i in (0..rows).rev() {
        for j in (0..cols).rev() {
            let mut best = score[(i + 1) * width + j].max(score[i * width + j + 1]);
            if let Some(like) = likeness_of[i * cols + j] {
                best = best.max(score[(i + 1) * width + j + 1] + like);
            }
            score[i * width + j] = best;
        }
    }

    let mut groups: Vec<Option<Group>> = groups.into_iter().map(Some).collect();
    let mut units: Vec<Option<Unit>> = units.into_iter().map(Some).collect();
    let mut out = Vec::with_capacity(rows + cols);
    let (mut i, mut j) = (0, 0);
    while i < rows || j < cols {
        let here = score[i * width + j];
        let paired = (i < rows && j < cols)
            .then(|| likeness_of[i * cols + j])
            .flatten()
            .filter(|like| here == score[(i + 1) * width + j + 1] + like);
        if paired.is_some() {
            let mut group = groups[i].take().unwrap();
            let unit = units[j].take().unwrap();
            if let Some(header) = unit.header {
                match group.headers.iter_mut().find(|entry| entry.line == header) {
                    Some(entry) => entry.mask |= bit,
                    None => group.headers.push(Entry {
                        line: header,
                        mask: bit,
                        header: true,
                    }),
                }
            }
            group.body = align(group.body, &unit.lines, bit);
            out.push(group);
            i += 1;
            j += 1;
        } else if i < rows && here == score[(i + 1) * width + j] {
            out.push(groups[i].take().unwrap());
            i += 1;
        } else {
            let unit = units[j].take().unwrap();
            out.push(Group {
                headers: unit
                    .header
                    .map(|line| Entry {
                        line,
                        mask: bit,
                        header: true,
                    })
                    .into_iter()
                    .collect(),
                body: unit
                    .lines
                    .iter()
                    .map(|line| Entry::new(line, bit))
                    .collect(),
            });
            j += 1;
        }
    }
    out
}

/// Merge the scripts of `variants` (each variant once) into a single script
/// whose preprocessed text is each input again.
///
/// A variant whose script is another region's, word for word, shares that
/// region's input, so the regions differ only where their scripts do. At
/// most sixteen inputs remain.
pub fn merge(variants: &[(Variant, &str)]) -> Result<String> {
    ensure!(!variants.is_empty(), "merge takes at least one variant");
    for (index, (variant, _)) in variants.iter().enumerate() {
        if variants[..index].iter().any(|(other, _)| other == variant) {
            bail!("variant {variant:?} given twice");
        }
    }
    let mut inputs: Vec<(Vec<Variant>, &str)> = Vec::new();
    for &(variant, text) in variants {
        let twin = |kind: &Variant| kind.in_region(variant.region) == variant;
        match inputs
            .iter_mut()
            .find(|(kinds, other)| *other == text && kinds.iter().any(twin))
        {
            Some((kinds, _)) => kinds.push(variant),
            None => inputs.push((vec![variant], text)),
        }
    }
    ensure!(
        inputs.len() <= Mask::BITS as usize,
        "merge takes at most {} distinct scripts, not {}",
        Mask::BITS,
        inputs.len()
    );

    let mut groups: Vec<Group> = Vec::new();
    for (bit, (_, text)) in inputs.iter().enumerate() {
        groups = align_units(groups, units(text), 1 << bit);
    }
    let entries: Vec<Entry> = groups
        .into_iter()
        .flat_map(|group| group.headers.into_iter().chain(group.body))
        .collect();

    let all = Mask::MAX >> (Mask::BITS as usize - inputs.len());
    let kinds: Vec<Vec<Variant>> = inputs.into_iter().map(|(kinds, _)| kinds).collect();
    let mut out = String::new();
    for line in emit(&entries, all, &kinds) {
        out.push_str(&line);
        out.push('\n');
    }

    for (variant, text) in variants {
        let kept = kept_lines_with(&out, Some(*variant), Some("AUTHENTIC"))?;
        let same = kept.len() == text.lines().count()
            && kept
                .iter()
                .zip(text.lines())
                .all(|((_, kept), line)| *kept == line);
        ensure!(same, "the merged script does not reproduce {variant:?}");
    }
    Ok(out)
}

/// How much matching `line` is worth: structural lines that appear
/// everywhere count for less, so they do not pull unrelated code together.
fn weight(line: &str) -> u32 {
    match line.trim() {
        "" | "{" | "}" => 1,
        _ => 4,
    }
}

/// Align `lines` (bit `bit`) against the merged `entries`.
fn align<'a>(entries: Vec<Entry<'a>>, lines: &[&'a str], bit: Mask) -> Vec<Entry<'a>> {
    let (n, m) = (entries.len(), lines.len());
    // Common prefix and suffix first: they keep the table small.
    let mut prefix = 0;
    while prefix < n && prefix < m && entries[prefix].line == lines[prefix] {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < n - prefix
        && suffix < m - prefix
        && entries[n - 1 - suffix].line == lines[m - 1 - suffix]
    {
        suffix += 1;
    }
    let left = &entries[prefix..n - suffix];
    let right = &lines[prefix..m - suffix];
    let (rows, cols) = (left.len(), right.len());

    // score[i][j]: best weight aligning left[i..] with right[j..].
    let width = cols + 1;
    let mut score = vec![0u32; (rows + 1) * width];
    for i in (0..rows).rev() {
        for j in (0..cols).rev() {
            let mut best = score[(i + 1) * width + j].max(score[i * width + j + 1]);
            if left[i].line == right[j] {
                best = best.max(score[(i + 1) * width + j + 1] + weight(right[j]));
            }
            score[i * width + j] = best;
        }
    }

    // Walk forward; between matches the merged lines come before the new.
    let mut ops = Vec::with_capacity(rows + cols);
    let (mut i, mut j) = (0, 0);
    while i < rows || j < cols {
        let here = score[i * width + j];
        if i < rows
            && j < cols
            && left[i].line == right[j]
            && here == score[(i + 1) * width + j + 1] + weight(right[j])
        {
            ops.push((Some(i), Some(j)));
            i += 1;
            j += 1;
        } else if i < rows && here == score[(i + 1) * width + j] {
            ops.push((Some(i), None));
            i += 1;
        } else {
            ops.push((None, Some(j)));
            j += 1;
        }
    }

    let mut entries = entries.into_iter();
    let mut out: Vec<Entry> = Vec::with_capacity(n + m);
    for (index, entry) in entries.by_ref().take(prefix).enumerate() {
        out.push(Entry {
            mask: entry.mask | bit,
            ..entry
        });
        debug_assert_eq!(entry.line, lines[index]);
    }
    let middle: Vec<Entry> = entries.by_ref().take(rows).collect();
    let mut middle = middle.into_iter().map(Some).collect::<Vec<_>>();
    for op in ops {
        match op {
            (Some(i), Some(_)) => {
                let entry = middle[i].take().unwrap();
                out.push(Entry {
                    mask: entry.mask | bit,
                    ..entry
                });
            }
            (Some(i), None) => out.push(middle[i].take().unwrap()),
            (None, Some(j)) => out.push(Entry::new(right[j], bit)),
            (None, None) => unreachable!(),
        }
    }
    for entry in entries {
        out.push(Entry {
            mask: entry.mask | bit,
            ..entry
        });
    }
    out
}

/// The symbol pairs a context can split on: (mask of the first half, its
/// symbol, the second half's symbol). A region splits from the rest. An
/// input in either half has all its variants there, so an input two regions
/// share stays out of a split between them; its variants differ from the
/// others in the mansion, the player or a third region, which split first.
fn splits(ctx: Mask, kinds: &[Vec<Variant>]) -> Vec<(Mask, &'static str, &'static str)> {
    let half = |pick: &dyn Fn(Variant) -> bool| -> Mask {
        kinds
            .iter()
            .enumerate()
            .filter(|&(_, kinds)| kinds.iter().all(|&kind| pick(kind)))
            .fold(0, |mask, (bit, _)| mask | (1 << bit))
            & ctx
    };
    let region = |region: Region, a, b| {
        (
            half(&|kind| kind.region == region),
            half(&|kind| kind.region != region),
            a,
            b,
        )
    };
    let mut splits = Vec::new();
    for (first, second, a, b) in [
        (
            half(&|kind| kind.mansion == Some(1)),
            half(&|kind| kind.mansion == Some(2)),
            "MANSION1",
            "MANSION2",
        ),
        (
            half(&|kind| !kind.jill),
            half(&|kind| kind.jill),
            "CHRIS",
            "JILL",
        ),
        region(Region::Usa, "USA", "!USA"),
        region(Region::Jpn, "JPN", "!JPN"),
        region(Region::Eur, "EUR", "!EUR"),
    ] {
        if first != 0 && second != 0 && first | second == ctx {
            splits.push((first, a, b));
        }
    }
    splits
}

/// The lines of `entries` for the variants in `ctx`.
fn emit(entries: &[Entry], ctx: Mask, kinds: &[Vec<Variant>]) -> Vec<String> {
    let live: Vec<Entry> = entries
        .iter()
        .filter(|entry| entry.mask & ctx != 0)
        .copied()
        .collect();
    let covers = |index: usize| live[index].mask & ctx == ctx;

    // The runs of lines some variant lacks, each grown to whole statements.
    let mut groups: Vec<(usize, usize)> = Vec::new();
    let mut index = 0;
    while index < live.len() {
        if covers(index) {
            index += 1;
            continue;
        }
        let mut end = index;
        while end < live.len() && !covers(end) {
            end += 1;
        }
        let (mut start, mut end) = contain(&live, index, end, ctx);
        while let Some(&(last_start, last_end)) = groups.last()
            && last_end >= start
        {
            groups.pop();
            (start, end) = contain(&live, last_start.min(start), last_end.max(end), ctx);
        }
        groups.push((start, end));
        index = end;
    }

    let mut out = Vec::new();
    let mut next = 0;
    for (start, end) in groups {
        out.extend(live[next..start].iter().map(|entry| entry.line.to_string()));
        out.extend(split(&live[start..end], ctx, kinds));
        next = end;
    }
    out.extend(live[next..].iter().map(|entry| entry.line.to_string()));
    out
}

/// The brace a line opens (1) or closes (-1); the decompiler puts braces on
/// lines of their own. A section's own `#if AUTHENTIC` nests like a brace,
/// so a variant split never cuts through it.
fn brace(line: &str) -> i32 {
    match line.trim() {
        "{" => 1,
        "}" => -1,
        line if line.starts_with("#if ") => 1,
        "#endif" => -1,
        _ => 0,
    }
}

/// Whether `line` continues the statement before it (`else`, or the `while`
/// of a `do`) or opens the block of the header before it.
fn continues(line: &str) -> bool {
    let line = line.trim();
    line == "{" || line == "else" || line.starts_with("else ") || line.starts_with("while (")
}

/// The variant bits in `ctx`.
fn bits(ctx: Mask) -> impl Iterator<Item = Mask> {
    (0..Mask::BITS)
        .map(|bit| 1 << bit)
        .filter(move |bit| ctx & bit != 0)
}

/// For one variant: whether its first line in `live[start..end]` or its
/// first line after it continues an earlier statement, and whether its
/// braces close past `start` or stay open at `end`.
fn breaks(live: &[Entry], start: usize, end: usize, bit: Mask) -> (bool, bool, bool) {
    let held = |entry: &&Entry| entry.mask & bit != 0;
    let continued = live[start..end]
        .iter()
        .find(held)
        .is_some_and(|entry| continues(entry.line));
    // A section header may differ on its own, ahead of its shared `{`.
    let ends_in_header = live[start..end]
        .iter()
        .rev()
        .find(held)
        .is_some_and(|entry| entry.header);
    let continued_after = !ends_in_header
        && live[end..]
            .iter()
            .find(held)
            .is_some_and(|entry| continues(entry.line));
    let mut depth = 0;
    let mut closes_past = false;
    for entry in live[start..end].iter().filter(held) {
        depth += brace(entry.line);
        closes_past |= depth < 0;
    }
    (
        continued || closes_past,
        continued_after,
        depth > 0 && !closes_past,
    )
}

/// Whether a block can start at `live[start]` and end before `live[end]`:
/// each variant in `ctx` sees balanced braces that never close past the
/// start, and no statement crosses either end.
fn contained(live: &[Entry], start: usize, end: usize, ctx: Mask) -> bool {
    bits(ctx).all(|bit| breaks(live, start, end, bit) == (false, false, false))
}

/// Grow `live[start..end]` until it holds whole statements for every
/// variant in `ctx`.
fn contain(live: &[Entry], mut start: usize, mut end: usize, ctx: Mask) -> (usize, usize) {
    loop {
        let (mut back, mut forward) = (false, false);
        for bit in bits(ctx) {
            let (before, after, open) = breaks(live, start, end, bit);
            back |= before;
            forward |= after || open;
        }
        if !back && !forward {
            break;
        }
        if back && start > 0 {
            start -= 1;
        } else if end < live.len() {
            end += 1;
        } else if start > 0 {
            start -= 1;
        } else {
            break;
        }
    }
    (start, end)
}

/// Write a run of lines not every variant in `ctx` holds as `#if` blocks.
/// The run is cut where its lines change from differing on one symbol pair
/// to the other, and the cuts that give the shortest text are kept.
fn split(run: &[Entry], ctx: Mask, kinds: &[Vec<Variant>]) -> Vec<String> {
    let pairs = splits(ctx, kinds);
    // Which pair each line differs on alone, if just one.
    let class = |entry: &Entry| {
        let held = entry.mask & ctx;
        pairs
            .iter()
            .position(|&(first, _, _)| held == first || held == ctx & !first)
    };
    let mut cuts = vec![0];
    for index in 1..run.len() {
        if class(&run[index]) != class(&run[index - 1]) && contained(run, 0, index, ctx) {
            cuts.push(index);
        }
    }
    cuts.push(run.len());
    // Too many cuts would make the search slow; write the run as one block.
    if cuts.len() > 64 {
        cuts = vec![0, run.len()];
    }

    // best[j]: the shortest text for run[..cuts[j]], and its last cut.
    let mut best: Vec<Option<(usize, usize)>> = vec![None; cuts.len()];
    best[0] = Some((0, 0));
    let mut blocks = std::collections::HashMap::new();
    for j in 1..cuts.len() {
        for i in 0..j {
            let Some((before, _)) = best[i] else {
                continue;
            };
            let block = blocks
                .entry((i, j))
                .or_insert_with(|| split_block(&run[cuts[i]..cuts[j]], ctx, kinds, &pairs));
            let total = before + block.len();
            if best[j].is_none_or(|(length, _)| total < length) {
                best[j] = Some((total, i));
            }
        }
    }
    let mut segments = Vec::new();
    let mut j = cuts.len() - 1;
    while j > 0 {
        let (_, i) = best[j].expect("every cut is reachable");
        segments.push(blocks.remove(&(i, j)).expect("the chosen block was built"));
        j = i;
    }
    segments.into_iter().rev().flatten().collect()
}

/// Write `run` as one `#if` block on whichever symbol pair gives the shorter
/// text.
fn split_block(
    run: &[Entry],
    ctx: Mask,
    kinds: &[Vec<Variant>],
    pairs: &[(Mask, &'static str, &'static str)],
) -> Vec<String> {
    let mut best: Option<Vec<String>> = None;
    for &(first, a, b) in pairs {
        let yes = emit(run, first, kinds);
        let no = emit(run, ctx & !first, kinds);
        // Alignment can give one line several entries with different masks;
        // when both halves emit the same text, no `#if` is needed.
        if yes == no {
            return yes;
        }
        let mut lines = Vec::with_capacity(yes.len() + no.len() + 3);
        if yes.is_empty() {
            lines.push(format!("#if {b}"));
            lines.extend(no);
        } else {
            lines.push(format!("#if {a}"));
            lines.extend(yes);
            if !no.is_empty() {
                lines.push("#else".to_string());
                lines.extend(no);
            }
        }
        lines.push("#endif".to_string());
        if best.as_ref().is_none_or(|best| lines.len() < best.len()) {
            best = Some(lines);
        }
    }
    // A run only exists when `ctx` holds variants that differ, and those
    // always differ in the mansion, the player or the region.
    best.expect("a run of differing lines has a symbol to split on")
}

/// Merge per-RDT scripts (keyed by RDT id, e.g. `0x1030`) into one script
/// per room, keyed by [`merged_id`] (e.g. `0x103` for 1030, 1031, 6030 and
/// 6031).
///
/// The variants' `#define` lines are pooled first: a define changes no
/// bytecode, so every variant gets them all and they need no `#if`.
pub fn merge_rooms(rooms: &BTreeMap<u16, String>) -> Result<BTreeMap<u16, String>> {
    merge_variants(
        rooms
            .iter()
            .map(|(&id, text)| (id, Variant::for_room(id), text.as_str())),
    )
}

/// [`merge_rooms`] over several regions, each with its per-RDT scripts: the
/// lines they differ in go under `#if USA`, `#if JPN` or `#if EUR`. A region
/// may hold RDTs the others lack.
pub fn merge_regions(
    regions: &[(Region, &BTreeMap<u16, String>)],
) -> Result<BTreeMap<u16, String>> {
    for (index, (region, _)) in regions.iter().enumerate() {
        ensure!(
            regions[..index].iter().all(|(other, _)| other != region),
            "region {region} given twice"
        );
    }
    merge_variants(regions.iter().flat_map(|&(region, rooms)| {
        rooms
            .iter()
            .map(move |(&id, text)| (id, Variant::for_room(id).in_region(region), text.as_str()))
    }))
}

/// Merge `(RDT id, variant, script)` triples into one script per room.
fn merge_variants<'a>(
    rooms: impl Iterator<Item = (u16, Variant, &'a str)>,
) -> Result<BTreeMap<u16, String>> {
    let mut groups: BTreeMap<u16, Vec<(Variant, &str)>> = BTreeMap::new();
    for (id, variant, text) in rooms {
        groups
            .entry(merged_id(id))
            .or_default()
            .push((variant, text));
    }
    groups
        .into_iter()
        .map(|(base, variants)| {
            let mut defines: Vec<&str> = variants
                .iter()
                .flat_map(|(_, text)| text.lines().filter(|line| line.starts_with("#define ")))
                .collect();
            defines.sort_unstable();
            defines.dedup();
            let texts: Vec<String> = variants
                .iter()
                .map(|(_, text)| with_defines(text, &defines))
                .collect();
            let variants: Vec<(Variant, &str)> = variants
                .iter()
                .zip(&texts)
                .map(|((variant, _), text)| (*variant, text.as_str()))
                .collect();
            let merged = merge(&variants).with_context(|| format!("room {base:03X}"))?;
            Ok((base, merged))
        })
        .collect()
}

/// `text` with its `#define` block replaced by `defines`, after the
/// `#include` lines (or the `#version` line).
fn with_defines(text: &str, defines: &[&str]) -> String {
    let mut lines: Vec<&str> = Vec::new();
    let mut previous_blank = false;
    for line in text.lines() {
        if line.starts_with("#define ") {
            continue;
        }
        // Dropping the block leaves two blank lines where it stood.
        let blank = line.is_empty();
        if blank && previous_blank {
            continue;
        }
        previous_blank = blank;
        lines.push(line);
    }
    if !defines.is_empty() {
        let at = lines
            .iter()
            .rposition(|line| line.starts_with("#include ") || line.starts_with("#version "))
            .map_or(0, |index| index + 1);
        let mut block = vec![""];
        block.extend_from_slice(defines);
        lines.splice(at..at, block);
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}
