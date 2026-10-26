//! Merge the scripts of a room's variants into one `#if` script.
//!
//! The variants' lines are aligned progressively (a weighted longest common
//! subsequence against the lines merged so far), so every merged line knows
//! which variants hold it. Lines every variant in scope holds are written
//! plainly; each run of the others is split on the mansion or the player,
//! whichever writes fewer lines, into `#if` / `#else` / `#endif` blocks.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail, ensure};

use super::preprocess::{Variant, kept_lines, merged_id};

/// One merged line and the variants (bit per input) that hold it.
struct Entry<'a> {
    line: &'a str,
    mask: u8,
}

/// Merge the scripts of `variants` (at most eight, each variant once) into a
/// single script whose preprocessed text is each input again.
pub fn merge(variants: &[(Variant, &str)]) -> Result<String> {
    ensure!(
        !variants.is_empty() && variants.len() <= 8,
        "merge takes one to eight variants"
    );
    for (index, (variant, _)) in variants.iter().enumerate() {
        if variants[..index].iter().any(|(other, _)| other == variant) {
            bail!("variant {variant:?} given twice");
        }
    }

    let mut entries: Vec<Entry> = Vec::new();
    for (bit, (_, text)) in variants.iter().enumerate() {
        let lines: Vec<&str> = text.lines().collect();
        entries = align(entries, &lines, 1 << bit);
    }

    let all = (1u8 << variants.len()) - 1;
    let kinds: Vec<Variant> = variants.iter().map(|(variant, _)| *variant).collect();
    let mut out = String::new();
    for line in emit(&entries, all, &kinds) {
        out.push_str(&line);
        out.push('\n');
    }

    for (variant, text) in variants {
        let kept = kept_lines(&out, Some(*variant))?;
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
fn align<'a>(entries: Vec<Entry<'a>>, lines: &[&'a str], bit: u8) -> Vec<Entry<'a>> {
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
            line: entry.line,
            mask: entry.mask | bit,
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
                    line: entry.line,
                    mask: entry.mask | bit,
                });
            }
            (Some(i), None) => out.push(middle[i].take().unwrap()),
            (None, Some(j)) => out.push(Entry {
                line: right[j],
                mask: bit,
            }),
            (None, None) => unreachable!(),
        }
    }
    for entry in entries {
        out.push(Entry {
            line: entry.line,
            mask: entry.mask | bit,
        });
    }
    out
}

/// The symbol pairs a context can split on: (mask of the first half, its
/// symbol, the second half's symbol).
fn splits(ctx: u8, kinds: &[Variant]) -> Vec<(u8, &'static str, &'static str)> {
    let half = |pick: &dyn Fn(Variant) -> bool| -> u8 {
        kinds
            .iter()
            .enumerate()
            .filter(|&(_, &kind)| pick(kind))
            .fold(0, |mask, (bit, _)| mask | (1 << bit))
            & ctx
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
    ] {
        if first != 0 && second != 0 && first | second == ctx {
            splits.push((first, a, b));
        }
    }
    splits
}

/// The lines of `entries` for the variants in `ctx`.
fn emit(entries: &[Entry], ctx: u8, kinds: &[Variant]) -> Vec<String> {
    let live: Vec<&Entry> = entries
        .iter()
        .filter(|entry| entry.mask & ctx != 0)
        .collect();
    let mut out = Vec::new();
    let mut index = 0;
    while index < live.len() {
        if live[index].mask & ctx == ctx {
            out.push(live[index].line.to_string());
            index += 1;
            continue;
        }
        let start = index;
        while index < live.len() && live[index].mask & ctx != ctx {
            index += 1;
        }
        let run: Vec<Entry> = live[start..index]
            .iter()
            .map(|entry| Entry {
                line: entry.line,
                mask: entry.mask,
            })
            .collect();
        out.extend(split(&run, ctx, kinds));
    }
    out
}

/// Write a run of lines not every variant in `ctx` holds as `#if` blocks.
/// The run is cut where its lines change from differing on one symbol pair
/// to the other, and the cuts that give the shortest text are kept.
fn split(run: &[Entry], ctx: u8, kinds: &[Variant]) -> Vec<String> {
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
        if class(&run[index]) != class(&run[index - 1]) {
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
    ctx: u8,
    kinds: &[Variant],
    pairs: &[(u8, &'static str, &'static str)],
) -> Vec<String> {
    let mut best: Option<Vec<String>> = None;
    for &(first, a, b) in pairs {
        let yes = emit(run, first, kinds);
        let no = emit(run, ctx & !first, kinds);
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
    // always differ in the mansion or the player.
    best.expect("a run of differing lines has a symbol to split on")
}

/// Merge per-RDT scripts (keyed by RDT id, e.g. `0x1030`) into one script
/// per room, keyed by [`merged_id`] (e.g. `0x103` for 1030, 1031, 6030 and
/// 6031).
pub fn merge_rooms(rooms: &BTreeMap<u16, String>) -> Result<BTreeMap<u16, String>> {
    let mut groups: BTreeMap<u16, Vec<(Variant, &str)>> = BTreeMap::new();
    for (&id, text) in rooms {
        groups
            .entry(merged_id(id))
            .or_default()
            .push((Variant::for_room(id), text));
    }
    groups
        .into_iter()
        .map(|(base, variants)| {
            let merged = merge(&variants).with_context(|| format!("room {base:03X}"))?;
            Ok((base, merged))
        })
        .collect()
}
