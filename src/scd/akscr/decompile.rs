//! `.akscr` decompiler: structured code from the decoded SCD IR.
//!
//! Every section is rendered structurally where the bytecode has the shape the
//! compiler emits, and falls back to the escape hatches (`noreturn`,
//! `single(N)`, `raw(...)`, `asm { ... }`) where it does not. Each rendered
//! section is then compiled on its own and compared with the original bytes;
//! a section that does not reproduce exactly is re-rendered as one `asm`
//! block, so the output always compiles back to identical bytecode.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use super::engine::{self, Family};
use super::layout;
use super::names::Names;
use super::preprocess::{merged_id, room_variants};
use crate::scd::akscr::compile;
use crate::scd::decomp::constants;
use crate::scd::disasm::{COMMAND_SIG, EventState, format_insn, next_event_state, render_run};
use crate::scd::ir::{Block, Decoded, Insn, Scripts, Stream, StreamKind};
use crate::scd::opcode::command_width;
use crate::sfx;

/// Render the `.akscr` source of every stream, with numbers for game data.
pub fn decompile(scripts: &Scripts) -> String {
    decompile_with(scripts, &Names::default(), 0x1000)
}

/// Render the `.akscr` source of every stream of RDT `room`, naming game
/// data with `names` and `#include`-ing the files they come from.
pub fn decompile_with(scripts: &Scripts, names: &Names, room: u16) -> String {
    let mut defines = names.defines().clone();
    for bit in 0..=u8::MAX {
        defines.insert(room_flag(bit), i64::from(bit));
    }
    for (id, name) in names.room_message_names(room) {
        defines.insert(name.to_string(), i64::from(id));
    }
    let aots = aot_names(scripts);
    for (&index, name) in &aots {
        defines.insert(name.clone(), i64::from(index));
    }
    let enemies = enemy_names(scripts, names);
    for (&slot, name) in &enemies {
        defines.insert(name.clone(), i64::from(slot));
    }
    let voices = voice_names(scripts, room);
    for (&id, name) in &voices {
        defines.insert(name.clone(), i64::from(id));
    }
    let voice_messages = voice_message_names(scripts, names, room, &voices);
    for (&id, name) in &voice_messages {
        defines.insert(name.clone(), i64::from(id));
    }
    for value in 0..=u8::MAX {
        for prefix in ROOM_SLOTS {
            defines.insert(format!("{prefix}{value:02X}"), i64::from(value));
        }
    }
    for slot in 0..0x7E {
        defines.insert(fx_at_enemy(slot), i64::from(slot) + 2);
    }
    let sounds = sound_names(room);
    for (&(_, id), name) in &sounds {
        defines.insert(name.clone(), i64::from(id));
    }
    let ctx = Ctx {
        names,
        room,
        aots,
        enemies,
        voices,
        voice_messages,
        stage: (merged_id(room) >> 8) as u8,
        defines,
        room_flags: RefCell::default(),
        room_messages: RefCell::default(),
        used_voices: RefCell::default(),
        locals: RefCell::default(),
        sounds,
    };
    let mut sections: Vec<String> = Vec::new();
    for block in &scripts.init {
        sections.push(block_section(&ctx, "init", block));
    }
    for block in &scripts.main {
        sections.push(block_section(&ctx, "main", block));
    }
    let first_event = sections.len();
    for stream in &scripts.events {
        sections.push(event_section(&ctx, stream));
    }
    name_events(&mut sections, first_event);

    let mut out = String::from("#version 1\n");
    if !names.includes().is_empty() {
        out.push('\n');
        for path in names.includes() {
            out.push_str(&format!("#include \"{path}\"\n"));
        }
    }
    let room_flags = ctx.room_flags.borrow();
    let room_messages = ctx.room_messages.borrow();
    let used_voices = ctx.used_voices.borrow();
    let locals = ctx.locals.borrow();
    if !room_flags.is_empty()
        || !locals.is_empty()
        || !ctx.aots.is_empty()
        || !ctx.enemies.is_empty()
        || !room_messages.is_empty()
        || !used_voices.is_empty()
    {
        out.push('\n');
        for (&index, name) in &ctx.aots {
            out.push_str(&format!("#define {name} 0x{index:02X}\n"));
        }
        for (&slot, name) in &ctx.enemies {
            out.push_str(&format!("#define {name} 0x{slot:02X}\n"));
        }
        for (name, value) in locals.iter() {
            out.push_str(&format!("#define {name} 0x{value:02X}\n"));
        }
        for &id in used_voices.iter() {
            out.push_str(&format!("#define {} 0x{id:02X}\n", ctx.voices[&id]));
        }
        for &bit in room_flags.iter() {
            out.push_str(&format!("#define {} 0x{bit:02X}\n", room_flag(bit)));
        }
        for (&id, name) in room_messages.iter() {
            out.push_str(&format!("#define {name} 0x{id:02X}\n"));
        }
    }
    for section in sections {
        out.push('\n');
        out.push_str(&section);
    }
    out
}

/// The prefixes of a room's object, camera, effect, item model, collision,
/// camera zone and sprite slot names, and of the AOTs and enemy slots that
/// have no name of their own.
const ROOM_SLOTS: [&str; 9] = [
    "OBJ_", "CAM_", "FX_", "IMDL_", "COL_", "ZONE_", "SPR_", "AOT_", "ENEMY_",
];

/// The effect parent byte of enemy slot `slot`.
fn fx_at_enemy(slot: u8) -> String {
    format!("FX_AT_ENEMY_{slot:02X}")
}

/// Names for the sounds `se_play_3d` can play in RDT `room`, by bank and
/// id: `SE_<id>_<file>` from the room's sound table when every variant of
/// the room plays the same file, `SE_<id>` when not, and a placeholder for
/// the other banks, whose sound depends on the door taken, the weapon or the
/// character.
fn sound_names(room: u16) -> BTreeMap<(u8, u8), String> {
    let rows: Vec<usize> = room_variants(merged_id(room))
        .into_iter()
        .filter_map(crate::state::RoomId::from_original)
        .map(|variant| sfx::room_row(variant.table_row(), variant.room))
        .collect();
    let mut names = BTreeMap::new();
    for id in 0..=u8::MAX {
        let files: BTreeSet<Option<&str>> = rows
            .iter()
            .map(|&row| sfx::room_sound(row, usize::from(id)))
            .collect();
        let name = match files.into_iter().collect::<Vec<_>>().as_slice() {
            [Some(file)] => {
                let file: String = file
                    .chars()
                    .map(|ch| {
                        if ch.is_ascii_alphanumeric() {
                            ch.to_ascii_uppercase()
                        } else {
                            '_'
                        }
                    })
                    .collect();
                format!("SE_{id:02X}_{file}")
            }
            _ => format!("SE_{id:02X}"),
        };
        names.insert((2, id), name);
        for (bank, prefix) in [
            (0, "SE_DOOR_"),
            (1, "SE_WEAPON_"),
            (3, "SE_PL_"),
            (4, "SE_BGM_"),
        ] {
            names.insert((bank, id), format!("{prefix}{id:02X}"));
        }
    }
    names
}

/// The opcodes whose first operand is an AOT (room trigger) index.
const AOT_OPS: [u8; 5] = [0x0C, 0x0D, 0x12, 0x13, 0x24];

/// Names for the room's AOT indices, `AOT_<index>_<kind>`, the kind from
/// the first command that sets each one up as anything but `SCE_NONE`.
fn aot_names(scripts: &Scripts) -> BTreeMap<u8, String> {
    let mut names: BTreeMap<u8, String> = BTreeMap::new();
    for insn in room_insns(scripts) {
        let Decoded::Command(op) = insn.decoded else {
            continue;
        };
        let Some(&index) = insn.bytes.get(1) else {
            continue;
        };
        // `item_aot_set`'s top bit is the item's alternate rotation.
        let index = if op.op == 0x18 { index & 0x7F } else { index };
        let kind = match op.op {
            0x0C => "DOOR".to_string(),
            0x18 => "ITEM".to_string(),
            // `aot_set` and `aot_reset` carry the room action type.
            0x0D | 0x12 => {
                let at = if op.op == 0x0D { 10 } else { 2 };
                match insn.bytes.get(at).and_then(|&sce| constants::sce(sce)) {
                    Some(sce) => sce.trim_start_matches("SCE_").to_string(),
                    None => continue,
                }
            }
            _ => continue,
        };
        let entry = names.entry(index).or_insert_with(|| kind.clone());
        if entry == "NONE" {
            *entry = kind;
        }
    }
    names
        .into_iter()
        .map(|(index, kind)| (index, format!("AOT_{index:02X}_{kind}")))
        .collect()
}

/// Every instruction of the room, in stream order.
fn room_insns(scripts: &Scripts) -> impl Iterator<Item = &Insn> {
    scripts
        .init
        .iter()
        .chain(&scripts.main)
        .flat_map(|block| &block.insns)
        .chain(scripts.events.iter().flat_map(|stream| &stream.insns))
}

/// The enemy slot an `enemy` command fills, and its type.
fn enemy_slot(insn: &Insn) -> Option<(u8, u8)> {
    let Decoded::Command(op) = insn.decoded else {
        return None;
    };
    if op.op != 0x1B {
        return None;
    }
    let slot = *insn.bytes.get(18)?;
    (slot < 0x10).then_some((slot, *insn.bytes.get(1)?))
}

/// Names for the room's enemy slots, `ENEMY_<slot>_<type>` from the
/// `enemy` commands that fill them; a slot filled with different types
/// is just `ENEMY_<slot>`.
fn enemy_names(scripts: &Scripts, names: &Names) -> BTreeMap<u8, String> {
    let mut types: BTreeMap<u8, BTreeSet<u8>> = BTreeMap::new();
    for (slot, kind) in room_insns(scripts).filter_map(enemy_slot) {
        types.entry(slot).or_default().insert(kind);
    }
    types
        .into_iter()
        .map(|(slot, kinds)| {
            let kind = match kinds.iter().copied().collect::<Vec<_>>().as_slice() {
                &[kind] => Some(names.enemy(kind).map_or_else(
                    || format!("{kind:02X}"),
                    // `EM_11_ZOMBIE_LAB` is `ZOMBIE_LAB`.
                    |name| name.splitn(3, '_').nth(2).unwrap_or(name).to_string(),
                )),
                _ => None,
            };
            let name = match kind {
                Some(kind) => format!("ENEMY_{slot:02X}_{kind}"),
                None => format!("ENEMY_{slot:02X}"),
            };
            (slot, name)
        })
        .collect()
}

/// The id a `voice_play(VOICE_PLAY, id, ..)` plays.
fn voice_id(insn: &Insn) -> Option<u16> {
    let Decoded::Command(op) = insn.decoded else {
        return None;
    };
    (op.op == 0x1E && insn.bytes.len() == 4 && insn.bytes[1] == 1)
        .then(|| u16::from_le_bytes([insn.bytes[2], insn.bytes[3]]))
}

/// Names for the voice lines RDT `room` plays, `VO_<file>` after the
/// line's file (`V104_02` is `VO_104_02`), with the id as a suffix where a
/// stage's table lists one file twice.
fn voice_names(scripts: &Scripts, room: u16) -> BTreeMap<u16, String> {
    let stage = ((room >> 12) as u8).saturating_sub(1);
    let Some(row) = crate::voice::row_for_stage(stage) else {
        return BTreeMap::new();
    };
    let records = crate::voice_table::ROWS[row];
    room_insns(scripts)
        .filter_map(voice_id)
        .filter_map(|id| {
            let file = crate::voice_table::name(stage, id)?;
            let base = file.strip_prefix('V').unwrap_or(file).to_uppercase();
            let name = if records.iter().filter(|&&other| other == file).count() > 1 {
                format!("VO_{base}_{id:02X}")
            } else {
                format!("VO_{base}")
            };
            Some((id, name))
        })
        .collect()
}

/// Names for the room's messages that have no text but follow a voice line:
/// `MSG_VO_104_02` for the line they caption, with the id as a suffix where
/// one line has two.
fn voice_message_names(
    scripts: &Scripts,
    names: &Names,
    room: u16,
    voices: &BTreeMap<u16, String>,
) -> BTreeMap<u8, String> {
    let mut by_id: BTreeMap<u8, String> = BTreeMap::new();
    let mut last_voice: Option<u16> = None;
    for insn in room_insns(scripts) {
        let Decoded::Command(op) = insn.decoded else {
            continue;
        };
        if let Some(voice) = voice_id(insn) {
            last_voice = Some(voice);
            continue;
        }
        if op.op == 0x0B
            && insn.bytes.len() == 4
            && let Some(voice) = last_voice
            && let Some(name) = voices.get(&voice)
        {
            let id = insn.bytes[1] & 0x7F;
            if names.message_id(room, id).is_none() {
                by_id.entry(id).or_insert_with(|| format!("MSG_{name}"));
            }
        }
        last_voice = None;
    }
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for name in by_id.values() {
        *counts.entry(name).or_default() += 1;
    }
    let shared: HashSet<String> = counts
        .into_iter()
        .filter(|&(_, count)| count > 1)
        .map(|(name, _)| name.to_string())
        .collect();
    by_id
        .into_iter()
        .map(|(id, name)| {
            let name = if shared.contains(&name) {
                format!("{name}_{id:02X}")
            } else {
                name
            };
            (id, name)
        })
        .collect()
}

/// Name each event by what starts it: `on_aot_05_message` for one an AOT
/// triggers, `on_enter` for one `proc init` runs, `from_main` for one
/// `proc main` runs and `<parent>_sub<n>` for the events another event
/// starts.
/// Events nothing names keep `event_XX`. The `event_XX` references in
/// `sections` (the events from `first_event` on) are renamed in place.
fn name_events(sections: &mut [String], first_event: usize) {
    let count = sections.len() - first_event;
    // (priority, name) per event; the lowest priority wins.
    let mut found: Vec<Option<(u8, String)>> = vec![None; count];
    let mut parents: Vec<Vec<usize>> = vec![Vec::new(); count];
    for (position, section) in sections.iter().enumerate() {
        let owner = position.checked_sub(first_event);
        let from_main = section.starts_with("proc main");
        for line in section.lines() {
            let trimmed = line.trim_start();
            let mnemonic: String = trimmed
                .chars()
                .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
                .collect();
            for target in event_refs(line) {
                if target >= count {
                    continue;
                }
                let candidate = if mnemonic == "aot_set" || mnemonic == "aot_reset" {
                    trimmed[mnemonic.len()..]
                        .trim_start_matches(['(', ' '])
                        .split([',', ' '])
                        .next()
                        .map(|aot| {
                            let aot = aot.to_ascii_lowercase();
                            (
                                0,
                                format!("on_{}", aot.strip_suffix("_event").unwrap_or(&aot)),
                            )
                        })
                } else {
                    match owner {
                        None if from_main => Some((2, "from_main".to_string())),
                        None => Some((1, "on_enter".to_string())),
                        Some(parent) if parent != target => {
                            parents[target].push(parent);
                            None
                        }
                        Some(_) => None,
                    }
                };
                if let Some(candidate) = candidate
                    && found[target]
                        .as_ref()
                        .is_none_or(|best| candidate.0 < best.0)
                {
                    found[target] = Some(candidate);
                }
            }
        }
    }

    // Name the triggered events first, then the forks after their parents;
    // a fork of a fork waits a round, and a cycle keeps `event_XX`.
    let mut used = HashSet::new();
    let mut unique = |base: &str| {
        let mut name = base.to_string();
        let mut suffix = 2;
        while !used.insert(name.clone()) {
            name = format!("{base}_{suffix}");
            suffix += 1;
        }
        name
    };
    let mut names: Vec<Option<String>> = found
        .iter()
        .map(|found| found.as_ref().map(|(_, base)| unique(base)))
        .collect();
    let mut children = vec![0usize; count];
    for _ in 0..count {
        for index in 0..count {
            if names[index].is_some() {
                continue;
            }
            if let Some(parent) = parents[index]
                .iter()
                .copied()
                .find(|&parent| names[parent].is_some())
            {
                children[parent] += 1;
                let base = format!(
                    "{}_sub{}",
                    names[parent].as_deref().unwrap_or(""),
                    children[parent]
                );
                names[index] = Some(unique(&base));
            }
        }
    }
    let names: Vec<String> = names
        .into_iter()
        .enumerate()
        .map(|(index, name)| name.unwrap_or_else(|| format!("event_{index:02X}")))
        .collect();
    for section in sections.iter_mut() {
        *section = rename_events(section, &names);
    }
}

fn is_word(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

/// The index of a whole-word `event_XX` at the start of `text`, if `text`
/// starts with one and `before` does not continue a word into it.
fn event_ref_at(before: &str, text: &str) -> Option<usize> {
    if before.chars().next_back().is_some_and(is_word) {
        return None;
    }
    let hex = text.strip_prefix("event_")?.get(..2)?;
    if text[8..].chars().next().is_some_and(is_word) {
        return None;
    }
    usize::from_str_radix(hex, 16).ok()
}

/// The `event_XX` indices referenced on `line`.
fn event_refs(line: &str) -> Vec<usize> {
    line.match_indices("event_")
        .filter_map(|(at, _)| event_ref_at(&line[..at], &line[at..]))
        .collect()
}

/// `text` with each whole-word `event_XX` replaced by its name.
fn rename_events(text: &str, names: &[String]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some(found) = text[at..].find("event_") {
        let start = at + found;
        out.push_str(&text[at..start]);
        match event_ref_at(&text[..start], &text[start..]).and_then(|index| names.get(index)) {
            Some(name) => {
                out.push_str(name);
                at = start + 8;
            }
            None => {
                out.push_str("event_");
                at = start + 6;
            }
        }
    }
    out.push_str(&text[at..]);
    out
}

/// The generated name of a room's own flag bit.
fn room_flag(bit: u8) -> String {
    format!("F_ROOM_{bit:02X}")
}

/// The naming context of one room.
struct Ctx<'n> {
    names: &'n Names,
    /// The room's AOT index names, defined at the top of the script.
    aots: BTreeMap<u8, String>,
    /// The room's enemy slot names, defined at the top of the script.
    enemies: BTreeMap<u8, String>,
    /// The names of the voice lines the room plays, by id.
    voices: BTreeMap<u16, String>,
    /// Names for the textless messages that caption a voice line, by id.
    voice_messages: BTreeMap<u8, String>,
    /// The RDT id, for its message text.
    room: u16,
    /// The room's stage (stages 6/7 as 1/2), for stage-relative door targets.
    stage: u8,
    /// Every name a section may use, for verifying it.
    defines: HashMap<String, i64>,
    /// The room flag bits named so far, defined at the top of the script.
    room_flags: RefCell<BTreeSet<u8>>,
    /// The room's own message names used so far, by id.
    room_messages: RefCell<BTreeMap<u8, String>>,
    /// The voice lines named so far, defined at the top of the script.
    used_voices: RefCell<BTreeSet<u16>>,
    /// The room's object, camera, effect and sound names used so far,
    /// defined at the top of the script.
    locals: RefCell<BTreeMap<String, u8>>,
    /// The room's sound names, by `se_play_3d` bank and id.
    sounds: BTreeMap<(u8, u8), String>,
}

impl Ctx<'_> {
    /// `name`, defined at the top of the script as `value`.
    fn local(&self, name: String, value: u8) -> Option<String> {
        self.locals.borrow_mut().insert(name.clone(), value);
        Some(name)
    }
}

/// The flag group of the room's own flags.
const FG_ROOM: u8 = 4;

/// The flag group of the door lock flags.
const FG_LOCK: u8 = 2;

/// The flag group of the enemy flags.
const FG_ENEMY: u8 = 3;

/// The flag group of the room item flags.
const FG_ITEM: u8 = 7;

/// One `proc init`/`proc main` section, verified against the block's bytes.
fn block_section(ctx: &Ctx, name: &str, block: &Block) -> String {
    let original = stream_bytes(&block.insns, &block.trailing);
    let mut writer = Writer::new(ctx);
    let header = format!("proc {name}");
    if block.trailing.is_empty()
        && let Some(body) = structure_block(&block.insns)
    {
        writer.command_block(&header, &body);
    } else {
        writer.asm_block(&header, &block.insns, &block.trailing, block.offset + 2);
    }
    let text = writer.finish();
    if verified(ctx, &text, &original) {
        return text;
    }
    let mut writer = Writer::new(ctx);
    writer.asm_block(&header, &block.insns, &block.trailing, block.offset + 2);
    writer.finish()
}

/// One `event event_XX` section, verified against the stream's bytes.
fn event_section(ctx: &Ctx, stream: &Stream) -> String {
    let StreamKind::Event(index) = stream.kind else {
        return String::new();
    };
    let mut header = format!("event event_{index:02X}");
    if needs_nopad(stream) {
        header.push_str(" nopad");
    }
    let original = stream_bytes(&stream.insns, &stream.trailing);
    let mut writer = Writer::new(ctx);
    if let Some(body) = EventParser::new(ctx, &stream.insns).parse_all() {
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
    if verified(ctx, &text, &original) {
        return text;
    }
    let mut writer = Writer::new(ctx);
    writer.event_asm(&header, stream);
    writer.finish()
}

/// Whether section `text` compiles back to `original`.
fn verified(ctx: &Ctx, text: &str, original: &[u8]) -> bool {
    compile::section_bytes(text, &ctx.defines).ok().as_deref() == Some(original)
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
    /// An `endif` that closes no `if`. The original's `end` resets the
    /// branch stack it unbalances, so it does nothing: kept as `asm`.
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
    ctx: &'a Ctx<'a>,
    insns: &'a [Insn],
    /// The sub-ISA state before each instruction.
    states: Vec<EventState>,
    index: usize,
}

impl<'a> EventParser<'a> {
    fn new(ctx: &'a Ctx<'a>, insns: &'a [Insn]) -> Self {
        let mut states = Vec::with_capacity(insns.len());
        let mut state = EventState::Top;
        for insn in insns {
            states.push(state);
            state = next_event_state(state, insn);
        }
        Self {
            ctx,
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
                        body.push(Evt::Line(statement(self.ctx, insn, state)));
                        self.index += 1;
                    }
                },
                Decoded::Event(op) if op.op == 0x06 => body.push(self.parse_run()?),
                Decoded::Event(op) if op.op == 0x07 => body.push(self.parse_single(insn)?),
                Decoded::Event(_) | Decoded::Actor(_) | Decoded::Tween(_) => {
                    body.push(Evt::Line(statement(self.ctx, insn, state)));
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
        let call = call_text(self.ctx, command, EventState::Top);
        let exact = command_width(&command.bytes) == Some(command.bytes.len())
            && usize::from(head.bytes[1]) == command.bytes.len() + 2;
        self.index += 2;
        let line = if exact {
            format!("{call};")
        } else {
            format!("single({}) {call};", head.bytes[1])
        };
        Some(Evt::Line(with_comment(self.ctx, command, line)))
    }
}

/// `name(args)` for one instruction, naming game data through `ctx`.
fn call_text(ctx: &Ctx, insn: &Insn, state: EventState) -> String {
    let (name, mut args) = format_insn(insn, state, None);
    if let Some(fields) = layout::layout(&name) {
        let values = layout::unpack(fields, insn.bytes.get(1..).unwrap_or(&[]));
        let args: Vec<String> = values
            .iter()
            .enumerate()
            .map(|(index, &value)| {
                name_field(ctx, &name, index, &values).unwrap_or_else(|| value.to_string())
            })
            .collect();
        return format!("{name}({})", args.join(", "));
    }
    match insn.decoded {
        Decoded::Command(op) => name_command_args(ctx, insn, op.op, &mut args),
        // `evt_work_set(WK_ENEMY, slot)` / `evt_work_set(WK_AOT, index)`.
        Decoded::Event(op) if op.op == 0x04 && insn.bytes.len() == 3 => {
            let value = insn.bytes[2];
            let named = match insn.bytes[1] {
                1 => ctx.enemy(i64::from(value)),
                2 => ctx.slot("OBJ_", i64::from(value)),
                // The item model record, not an AOT.
                3 => ctx.slot("IMDL_", i64::from(value)),
                _ => None,
            };
            if let Some(named) = named
                && let Some(arg) = args.get_mut(1)
            {
                *arg = named;
            }
        }
        // `evt_fork(slot, ..)` / `evt_disable(slot)`.
        Decoded::Event(op) if matches!(op.op, 0x05 | 0x09) && insn.bytes.len() > 1 => {
            if let (Some(named), Some(arg)) =
                (engine::TASK.name(i64::from(insn.bytes[1])), args.get_mut(0))
            {
                *arg = named;
            }
        }
        _ => {}
    }
    format!("{name}({})", args.join(", "))
}

/// Name the arguments of command `op`, `args` as the disassembler renders
/// them: by its signature, or one per byte for a command without one.
fn name_command_args(ctx: &Ctx, insn: &Insn, op: u8, args: &mut [String]) {
    let kinds: Vec<u8> = match COMMAND_SIG
        .get(usize::from(op))
        .map(|signature| signature.split_once(':'))
    {
        Some(Some((_, params))) => params.bytes().collect(),
        Some(None) => vec![b'u'; insn.bytes.len().saturating_sub(1)],
        None => return,
    };
    let byte = |position: usize| insn.bytes.get(position).copied().unwrap_or(0);
    let mut position = 1;
    let mut positions = Vec::new();
    for kind in kinds {
        let size = match kind {
            b'U' | b'I' => 2,
            _ => 1,
        };
        if position + size > insn.bytes.len() {
            break;
        }
        positions.push((kind, position));
        position += size;
    }
    let at = |index: usize| {
        positions
            .get(index)
            .map_or(0, |&(_, position)| byte(position))
    };
    for (index, &(kind, position)) in positions.iter().enumerate() {
        let value = byte(position);
        let word = i64::from(value) | (i64::from(byte(position + 1)) << 8);
        let byte_value = i64::from(value);
        let named = match (op, index, kind) {
            // `ck` / `set`: the flag bit within its group, and the state.
            (0x04 | 0x05, 1, _) => {
                let group = byte(1);
                if group == FG_ROOM {
                    ctx.room_flags.borrow_mut().insert(value);
                    Some(room_flag(value))
                } else {
                    ctx.names.flag(group, value).map(str::to_string)
                }
            }
            (0x04, 2, _) if value < 2 => engine::TRUTH.name(byte_value),
            (0x05, 2, _) => engine::TRUTH.name(byte_value),
            // `cmpb` / `setb`: the state byte, the comparison and the value.
            (0x06 | 0x08, 0, _) => engine::BYTE.name(byte_value),
            (0x06, 1, _) | (0x07, 2, _) => engine::CMP.name(byte_value),
            (0x06, 2, _) | (0x08, 1, _) => state_value(ctx, at(0), value),
            // `cmpw` / `setw`: the state word.
            (0x07, 1, _) => engine::WORD.name(byte_value),
            // The AOT trigger flags.
            (0x0C, 16, _) | (0x0D, 6, _) | (0x12 | 0x13, 2, _) => engine::SAT.bits(byte_value),
            // The camera to cut to.
            (0x09, 0, _) => ctx.local(format!("CAM_{value:02X}"), value),
            // `door_aot_set`: its sound pair, animation and lock.
            (0x0C, 6, _) => ctx.names.door_sound(value).map(str::to_string),
            (0x0C, 7, _) => ctx.names.door(value).map(str::to_string),
            (0x0C, 9, _) => door_lock(ctx, value),
            // Event slots.
            (0x14, 1, _) => engine::TASK.name(byte_value),
            // Music channels.
            (0x15 | 0x16, 0, _) => engine::BGM_CHANNEL.name(byte_value),
            (0x29, 0, _) => ctx.names.movie(value).map(str::to_string),
            (0x2D, 0, _) => ctx.aot(byte_value),
            // `sys_multi`: the sub-command and its value.
            (0x33, 0, _) => engine::SYS.name(byte_value),
            (0x33, 1, b'U') => sys_value(at(0), word),
            (0x0B, 1, b'U') => engine::RUN.bits(word),
            (_, _, b't') if !matches!(value, 0 | 254 | 255) => Some(
                ctx.names
                    .item(value)
                    .map_or_else(|| value.to_string(), str::to_string),
            ),
            (_, _, b'e') => Some(
                ctx.names
                    .enemy(value)
                    .map_or_else(|| value.to_string(), str::to_string),
            ),
            (op, 0, _) if AOT_OPS.contains(&op) => ctx.aot(byte_value),
            // `message`: the text id, its top bit asking for the fast reveal.
            (0x0B, 0, _) => {
                let id = value & 0x7F;
                let named = match ctx.names.message_id(ctx.room, id) {
                    Some((name, local)) => {
                        if local {
                            ctx.room_messages.borrow_mut().insert(id, name.to_string());
                        }
                        Some(name.to_string())
                    }
                    None => ctx.voice_messages.get(&id).map(|name| {
                        ctx.room_messages.borrow_mut().insert(id, name.clone());
                        name.clone()
                    }),
                };
                match (named, value & 0x80 != 0) {
                    (Some(name), true) => Some(format!("{name} | MSG_FAST")),
                    (Some(name), false) => Some(name),
                    (None, true) => Some(format!("{id} | MSG_FAST")),
                    (None, false) => None,
                }
            }
            // `se_play_3d`: the bank and sound, where it plays, and the enemy
            // it plays on.
            (0x17, 0, _) => engine::SND_BANK.name(byte_value),
            (0x17, 1, _) => ctx
                .sounds
                .get(&(at(0), value))
                .and_then(|name| ctx.local(name.clone(), value)),
            (0x17, 3, _) => engine::SND_AT.name(byte_value),
            (0x17, 4, _) if at(3) == 2 => ctx.enemy(byte_value),
            // Commands on an enemy slot.
            (0x21 | 0x39, 0, _) | (0x28, 1, _) => ctx.enemy(byte_value),
            // `eml_state`: the sub-command, and how the status bits apply.
            (0x28, 2, _) => engine::EML.name(byte_value),
            (0x28, 3, b'U') if at(2) == 0 => engine::ENEMY_BEHAVIOUR.name(word),
            (0x28, 3, b'U') if at(2) == 2 => engine::ACTION.name(word),
            (0x28, 3, b'U') if at(2) == 9 => engine::JOINT.bits(word),
            (0x23, 0, _) => engine::CUT.name(byte_value),
            (0x28, 3, b'U') if at(2) == 3 => engine::EML_STATUS
                .name(word & 0xFF00)
                .map(|mode| format!("{mode} | {}", word & 0xFF)),
            // `room_bgm_state_set`: the room in the 0-based stage, and the
            // music state.
            (0x37, 1, _) if value < 0x20 => ctx.names.door_target(at(0) + 1, value),
            (0x37, 2, _) => engine::BGM_OFF
                .name(byte_value)
                .or_else(|| engine::BGM_STATE.bits(byte_value)),
            (_, _, b'r') => Some(
                ctx.names
                    .door_target(ctx.stage, value)
                    .unwrap_or_else(|| value.to_string()),
            ),
            _ => None,
        };
        if let Some(named) = named
            && let Some(arg) = args.get_mut(index)
        {
            *arg = named;
        }
    }
}

/// `sys_multi`'s value for sub-command `sub`: how the player or health
/// flags apply and which, the hit, the joints or the switch.
fn sys_value(sub: u8, word: i64) -> Option<String> {
    let bits = |family: &Family| {
        let mode = engine::SYS_MODE.name(word & 0xFF00)?;
        Some(match family.bits(word & 0xFF) {
            Some(bits) => format!("{mode} | {bits}"),
            None => mode,
        })
    };
    match sub {
        1 => engine::HIT.name(word),
        3 => bits(&engine::PLAYER_FLAG),
        8 => bits(&engine::HEALTH_STATUS),
        9 => engine::JOINT.bits(word),
        10 => engine::SYS_SWITCH.name(word),
        _ => None,
    }
}

/// `parts` ORed together.
fn or(parts: &[String]) -> String {
    parts.join(" | ")
}

/// `value` as `family`'s name for its `mask` bits ORed with the rest, which
/// `rest` names.
fn split(
    value: i64,
    mask: i64,
    family: &Family,
    rest: impl FnOnce(i64) -> Option<String>,
) -> Option<String> {
    let low = rest(value & !mask).unwrap_or_else(|| (value & !mask).to_string());
    if value & mask == 0 {
        return Some(low);
    }
    Some(or(&[family.bits(value & mask)?, low]))
}

impl Ctx<'_> {
    /// A room flag bit, defined at the top of the script.
    fn room_flag(&self, value: i64) -> Option<String> {
        let bit = u8::try_from(value).ok()?;
        self.room_flags.borrow_mut().insert(bit);
        Some(room_flag(bit))
    }

    /// A room-local slot name, `<prefix><value>`.
    fn slot(&self, prefix: &str, value: i64) -> Option<String> {
        let value = u8::try_from(value).ok()?;
        self.local(format!("{prefix}{value:02X}"), value)
    }

    /// An enemy slot.
    fn enemy(&self, value: i64) -> Option<String> {
        match self.enemies.get(&u8::try_from(value).ok()?) {
            Some(name) => Some(name.clone()),
            None => self.slot("ENEMY_", value),
        }
    }

    /// An AOT index.
    fn aot(&self, value: i64) -> Option<String> {
        match self.aots.get(&u8::try_from(value).ok()?) {
            Some(name) => Some(name.clone()),
            None => self.slot("AOT_", value),
        }
    }

    /// An effect parent byte: the world, the player, an enemy slot or an
    /// object.
    fn fx_parent(&self, value: i64) -> Option<String> {
        match value {
            0 | 1 => engine::FX_AT.name(value),
            2..=0x7F => {
                let slot = value as u8 - 2;
                self.local(fx_at_enemy(slot), value as u8)
            }
            _ => split(value, 0x80, &engine::FX_AT, |index| {
                self.slot("OBJ_", index)
            }),
        }
    }

    /// An attachment byte: nothing, the player, an enemy or an object.
    fn sca(&self, value: i64) -> Option<String> {
        match value {
            0xFE | 0xFF => engine::SCA.name(value),
            0x80.. => split(value, 0x80, &engine::SCA, |slot| self.enemy(slot)),
            _ => self.slot("OBJ_", value),
        }
    }
}

/// The name of field `index` of `name`, a command or event instruction
/// written by field ([`layout`]), whose fields hold `values`.
fn name_field(ctx: &Ctx, name: &str, index: usize, values: &[i64]) -> Option<String> {
    let value = values[index];
    let at = |index: usize| values.get(index).copied().unwrap_or(0);
    let item = |value: i64| {
        let id = u8::try_from(value).ok()?;
        (!matches!(id, 0 | 254 | 255)).then(|| {
            ctx.names
                .item(id)
                .map_or_else(|| id.to_string(), str::to_string)
        })
    };
    let flag = |group: u8, value: i64| {
        ctx.names
            .flag(group, u8::try_from(value).ok()?)
            .map(str::to_string)
    };
    match (name, index) {
        // `item_aot_set`: its AOT, the item, its model and parent, the flag
        // that marks it taken, its trigger and its pick-up flags.
        ("item_aot_set", 0) => split(value, 0x80, &engine::ITEM, |aot| ctx.aot(aot)),
        ("item_aot_set", 5) => item(value),
        ("item_aot_set", 7) => ctx.slot("IMDL_", value),
        ("item_aot_set", 8) | ("obj", 2) => ctx.sca(value),
        ("item_aot_set", 13) => flag(FG_ITEM, value),
        ("item_aot_set", 14) => engine::SAT.bits(value),
        // The sparkle, its effect, its height (a number) and the reach.
        ("item_aot_set", 15) if value != 0 && value & 0x700E == 0 => {
            let parts: Vec<String> = [0x8000, 0x0F00, 0x00F0, 0x0001]
                .into_iter()
                .filter(|&mask| value & mask != 0)
                .map(|mask| {
                    engine::ITEM
                        .name(value & mask)
                        .unwrap_or_else(|| (value & mask).to_string())
                })
                .collect();
            Some(or(&parts))
        }
        // `enemy`: its type and behaviour, the flag that keeps it away, how
        // it starts, its slot, animation and variant.
        ("enemy", 0) => Some(
            ctx.names
                .enemy(u8::try_from(value).ok()?)
                .map_or_else(|| value.to_string(), str::to_string),
        ),
        ("enemy", 1) => engine::ENEMY_BEHAVIOUR.name(value),
        ("enemy", 2) => engine::NO_FLAG
            .name(value)
            .or_else(|| flag(FG_ENEMY, value)),
        ("enemy", 3) => engine::SPAWN.name(value),
        ("enemy", 11) => {
            let slot = ctx.enemy(value & 0x0F)?;
            Some(match value & !0x0F {
                0 => slot,
                rest => format!("{slot} | {rest}"),
            })
        }
        ("enemy", 12) | ("plw_anim", 1) | ("act_anim_flags", 0) | ("act_anim_set", 1) => {
            engine::ANIM.name(value)
        }
        ("enemy", 14) => engine::ENEMY_VARIANT.name(value),
        // `timer_setup`: the light's mode and channels.
        ("timer_setup", 0) => engine::LIGHT_FADE.name(value),
        ("timer_setup", 2) => engine::FADE_CHANNEL.bits(value),
        ("ck_last_item", 0) => item(value),
        // `voice_play`: play or stop, and the line.
        ("voice_play", 0) => engine::VOICE.name(value),
        ("voice_play", 1) if at(0) == 1 => {
            let id = u16::try_from(value).ok()?;
            ctx.voices.get(&id).map(|name| {
                ctx.used_voices.borrow_mut().insert(id);
                name.clone()
            })
        }
        // `obj`: the object and how it behaves, and its flags.
        ("obj", 0) => split(value, 0xC0, &engine::OBJ_SLOT, |slot| {
            ctx.slot("OBJ_", slot)
        }),
        ("obj", 1) => engine::OBJ_FLAG.bits(value),
        ("ck_item_count", 0) => engine::ITEM_GROUP.name(value),
        ("ck_item_count" | "ck_anim", 1) => engine::CMP.name(value),
        ("aot_switch", 1) => ctx.slot("SPR_", value),
        ("aot_switch", 2) => engine::SPRITE.name(value),
        // Effects: the type, the animation row and what it is attached to.
        ("effect" | "effect_tracked" | "effect_kill_b", 0) => ctx.slot("FX_", value),
        ("effect" | "effect_tracked" | "effect_kill_b", 1) => engine::FX_ROW.name(value),
        ("effect" | "effect_tracked", 2) => ctx.fx_parent(value),
        ("plw_anim", 0) => engine::PLW.name(value),
        ("snd_pan_vol_set" | "bgm_volume_ramp", 0) => engine::BGM_CHANNEL.name(value),
        // `inst_cfg`: the collision list, its record and flags.
        ("inst_cfg", 0) => engine::COL_LIST.name(value),
        ("inst_cfg", 1) => ctx.slot("COL_", value),
        ("inst_cfg", 2) => engine::COL_FLAG.bits(value),
        ("setw", 0) => engine::WORD.name(value),
        ("model_op", 0) => engine::MODEL_OP.name(value),
        // `objtbl_b_set`: the table, the object or item model, and its flags.
        ("objtbl_b_set", 0) => engine::TABLE.name(value),
        ("objtbl_b_set", 1) => match at(0) {
            0 => ctx.slot("OBJ_", value),
            1 => ctx.slot("IMDL_", value),
            _ => None,
        },
        ("objtbl_b_set", 2) => engine::OBJ_FLAG.bits(value),
        ("ck_anim" | "eml_pos", 0) => ctx.slot("OBJ_", value),
        ("ck_bits", 0) => engine::TRUTH.name(value),
        ("ck_bits", 1) => engine::PAD.bits(value),
        ("msgnode_set", 0) => ctx.slot("ZONE_", value),
        ("msgnode_set", 1 | 2) => ctx.slot("CAM_", value),
        // `eml_rot`: an object (`ROT_OBJ | n`) or an item model.
        ("eml_rot", 0) => match value {
            0x80.. => split(value, 0x80, &engine::ROT, |index| ctx.slot("OBJ_", index)),
            _ => ctx.slot("IMDL_", value),
        },
        // `ck_counter`: the kind of target and which.
        ("ck_counter", 1) => engine::DIST.name(value),
        ("ck_counter", 2) => match at(1) {
            0 => ctx.enemy(value),
            1 => ctx.slot("OBJ_", value),
            2 => ctx.slot("IMDL_", value),
            _ => None,
        },
        ("obj_xfm", 0) => engine::LIGHT.name(value),
        ("spd_set", 0) => engine::ENTITY.name(value),
        ("task_kill", 0) => engine::TASK.name(value),
        ("msg_list", 7 | 15 | 23) => engine::LIGHT_KIND.name(value),
        // `swap_var`: which way, the AOT and the state byte.
        ("swap_var", 0) => engine::XFER.name(value),
        ("swap_var", 1) => ctx.aot(value),
        ("swap_var", 2) => engine::BYTE.name(value),
        ("mass_mask", 0) => engine::MASK.name(value),
        ("mass_mask", 1) => engine::FX_FLAG.bits(value),
        ("costume_set" | "costume_ck", 0) => engine::COSTUME.name(value),
        // Actors: the motion and its target, the behaviour, the animation,
        // the room flag raised when it is done and the entity flags.
        ("act_motion" | "act_motion_path", 0) => engine::MOTION.name(value),
        ("act_motion_path", 1) if at(0) == 0x93 => engine::LOOK_AT.name(value),
        ("act_motion_path", 2) if at(0) == 0x93 => match at(1) {
            1 => ctx.enemy(value),
            2 => ctx.slot("OBJ_", value),
            3 => ctx.slot("IMDL_", value),
            _ => None,
        },
        ("act_anim_seq", 0) => split(value, 0x10, &engine::ACTION, |behaviour| {
            engine::ACTION.name(behaviour)
        }),
        ("act_anim_set", 0) => engine::ACTION.name(value),
        ("act_anim_seq", 3) | ("act_anim_flags", 1) | ("act_anim_set", 2) => ctx.room_flag(value),
        ("act_anim_flags", 2) => engine::ANIM_FLAG.bits(value),
        ("act_flag_op", 0) => engine::ENTITY_FLAG_MODE.name(value),
        ("act_flag_op", 1) => engine::ENTITY_FLAG.bits(value),
        // Tweens: the state byte or value to write.
        ("tw_set_field", 0) => engine::TWEEN_FIELD.name(value),
        ("tw_set_sel", 0) => engine::TWEEN_SEL.name(value),
        _ => None,
    }
}

/// `door_aot_set`'s lock byte: locked, and the `FG_LOCK` bit that unlocks.
fn door_lock(ctx: &Ctx, value: u8) -> Option<String> {
    if value == 0 {
        return None;
    }
    let mut parts: Vec<String> = [0x80, 0x40]
        .into_iter()
        .filter(|&bit| value & bit != 0)
        .filter_map(|bit| engine::DOOR_LOCK.name(i64::from(bit)))
        .collect();
    let bit = value & 0x3F;
    if bit != 0 || value & 0x80 != 0 {
        parts.push(
            ctx.names
                .flag(FG_LOCK, bit)
                .map_or_else(|| bit.to_string(), str::to_string),
        );
    }
    Some(parts.join(" | "))
}

/// A value a state byte holds: a room of this stage, a camera, or an item.
fn state_value(ctx: &Ctx, index: u8, value: u8) -> Option<String> {
    match index {
        1 | 3 if value < 0x20 => ctx.names.door_target(ctx.stage, value),
        2 | 4 => ctx.local(format!("CAM_{value:02X}"), value),
        6 | 18 | 19 | 41 if value != 0 => ctx.names.item(value).map(str::to_string),
        _ => None,
    }
}

/// `name(args);` for one instruction.
fn statement(ctx: &Ctx, insn: &Insn, state: EventState) -> String {
    with_comment(ctx, insn, format!("{};", call_text(ctx, insn, state)))
}

/// `line` with the text a `message` shows as a trailing comment.
fn with_comment(ctx: &Ctx, insn: &Insn, line: String) -> String {
    if let Decoded::Command(op) = insn.decoded
        && op.op == 0x0B
        && insn.bytes.len() == 4
        // The id's top bit asks for the fast reveal; the rest picks the text.
        && let Some(text) = ctx.names.message(ctx.room, insn.bytes[1] & 0x7F)
    {
        return format!("{line} // \"{text}\"");
    }
    line
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

/// Indented line writer.
struct Writer<'a> {
    ctx: &'a Ctx<'a>,
    lines: Vec<String>,
    indent: usize,
}

impl<'a> Writer<'a> {
    fn new(ctx: &'a Ctx<'a>) -> Self {
        Self {
            ctx,
            lines: Vec::new(),
            indent: 0,
        }
    }

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
                Cmd::Call(insn) => self.line(&statement(self.ctx, insn, EventState::Top)),
                Cmd::Require(insn) => self.line(&format!(
                    "require({});",
                    call_text(self.ctx, insn, EventState::Top)
                )),
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
            .map(|insn| call_text(self.ctx, insn, EventState::Top))
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
                        call_text(self.ctx, condition, EventState::Top)
                    ));
                }
            }
        }
    }
}
