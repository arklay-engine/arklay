//! Turn a game's `inc/flags.akinc` into the `game/flags.lua` its pack carries.
//!
//! Shared with `build.rs`, so it uses only `std`. The room scripts take their
//! flag names from `flags.akinc` when the pack is built; the pack keeps the
//! game banks' names as a Lua chunk of `api:set_flag_name` calls for the
//! debug menu. The system banks (the engine's room, state, run, map and
//! item-use bits) are named by the engine and are left out.

use std::fmt::Write as _;

/// The game banks, by the group prefix their defines take.
pub const GAME_BANKS: [(&str, u8); 5] = [
    ("SCENARIO", 0),
    ("COMMON", 1),
    ("LOCK", 2),
    ("ENEMY", 3),
    ("ITEM", 7),
];

/// One named bit of a game bank.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlagDefine {
    pub bank: u8,
    pub bit: u8,
    /// The name less its `F_<group>_` prefix.
    pub name: String,
    /// The define's trailing `//` comment, if any.
    pub description: String,
}

/// The named game-bank bits of `akinc`, in file order. A placeholder name,
/// the bit in two hex digits (`F_COMMON_A4`), is not a name and is skipped.
pub fn flag_defines(akinc: &str) -> Vec<FlagDefine> {
    let mut defines = Vec::new();
    for line in akinc.lines() {
        let (define, comment) = match line.split_once("//") {
            Some((define, comment)) => (define, comment.trim()),
            None => (line, ""),
        };
        let mut parts = define.split_whitespace();
        let (Some("#define"), Some(name), Some(value), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let Some(bit) = parse_byte(value) else {
            continue;
        };
        let Some((bank, short)) = GAME_BANKS.iter().find_map(|(group, bank)| {
            let short = name
                .strip_prefix("F_")?
                .strip_prefix(group)?
                .strip_prefix('_')?;
            Some((*bank, short))
        }) else {
            continue;
        };
        if short == format!("{bit:02X}") {
            continue;
        }
        defines.push(FlagDefine {
            bank,
            bit,
            name: short.to_string(),
            description: comment.to_string(),
        });
    }
    defines
}

fn parse_byte(text: &str) -> Option<u8> {
    match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        Some(hex) => u8::from_str_radix(hex, 16).ok(),
        None => text.parse().ok(),
    }
}

/// The `game/flags.lua` chunk for `akinc`, read from `source` (named in its
/// header).
pub fn flags_lua(akinc: &str, source: &str) -> String {
    let mut out = format!(
        "-- @generated from {source} when the pack is built: the names of the\n\
         -- game's flag bits, for the debug menu's FLAGS screen. Edit the names\n\
         -- there; a mod can ship its own game/flags.lua.\n\
         --\n\
         -- api:set_flag_name(bank, bit, name, description); a bit may take\n\
         -- more than one name (one per player).\n\n\
         local api = ...\n"
    );
    let mut bank = None;
    for define in flag_defines(akinc) {
        if bank != Some(define.bank) {
            bank = Some(define.bank);
            let group = GAME_BANKS
                .iter()
                .find(|(_, number)| *number == define.bank)
                .map_or("", |(group, _)| group);
            let _ = write!(out, "\n-- FG_{group}\n");
        }
        let _ = writeln!(
            out,
            "api:set_flag_name({}, 0x{:02X}, {}, {})",
            define.bank,
            define.bit,
            lua_string(&define.name),
            lua_string(&define.description)
        );
    }
    out
}

/// `text` as a Lua string literal.
fn lua_string(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
