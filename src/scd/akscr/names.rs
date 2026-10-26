//! The names a decompiled script uses for game data, read from include files.
//!
//! The compiler knows only the engine's own constants. Items, enemies, rooms
//! and flags are a game's data, so a game names them in `#define` include
//! files (`inc/items.akinc` and so on) and the decompiler reads those files
//! back to name the values it meets. A define is recognised by its prefix:
//!
//! - `ITEM_*` an item id, `ENEMY_*` an enemy type;
//! - `STAGE_*` a door target's stage bits (`0x40` for stage 2);
//! - `ROOM_<stage><room>*` a room within its stage (`ROOM_106_MAIN_HALL` is
//!   `0x06`), for door targets;
//! - `FLAG_<group>_*` a bit of flag group `FG_<group>`.

use std::collections::HashMap;

use anyhow::{Context, Result, bail};

use super::preprocess::parse_number;
use crate::scd::decomp::constants;

/// Game data names, by value.
#[derive(Debug, Default, Clone)]
pub struct Names {
    /// The include paths a script names these with, in order.
    includes: Vec<String>,
    /// Every define, for compiling the scripts that use them.
    defines: HashMap<String, i64>,
    items: HashMap<u8, String>,
    enemies: HashMap<u8, String>,
    stages: HashMap<u8, String>,
    rooms: HashMap<(u8, u8), String>,
    flags: HashMap<(u8, u8), String>,
    /// Message text by language key, as one plain line, for comments.
    messages: HashMap<String, String>,
}

impl Names {
    /// Read `(include path, text)` files of `#define` lines. The first name
    /// for a value wins.
    pub fn parse(files: &[(String, String)]) -> Result<Self> {
        let mut names = Self::default();
        for (path, text) in files {
            names.includes.push(path.clone());
            for (index, line) in text.lines().enumerate() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with("//") {
                    continue;
                }
                let mut parts = trimmed.split_whitespace();
                let (Some("#define"), Some(name), Some(value), None) =
                    (parts.next(), parts.next(), parts.next(), parts.next())
                else {
                    bail!("{path}:{}: expected `#define NAME value`", index + 1);
                };
                let value = parse_number(value)
                    .with_context(|| format!("{path}:{}: `{value}` is not a number", index + 1))?;
                names.add(name, value);
            }
        }
        Ok(names)
    }

    fn add(&mut self, name: &str, value: i64) {
        self.defines.insert(name.to_string(), value);
        let Ok(byte) = u8::try_from(value) else {
            return;
        };
        let name = name.to_string();
        if name.starts_with("ITEM_") {
            self.items.entry(byte).or_insert(name);
        } else if name.starts_with("ENEMY_") {
            self.enemies.entry(byte).or_insert(name);
        } else if name.starts_with("STAGE_") {
            self.stages.entry(byte >> 5).or_insert(name);
        } else if let Some(stage) = name
            .strip_prefix("ROOM_")
            .and_then(|rest| rest.get(..1))
            .and_then(|digit| u8::from_str_radix(digit, 16).ok())
        {
            self.rooms.entry((stage, byte)).or_insert(name);
        } else if let Some(rest) = name.strip_prefix("FLAG_") {
            // The longest group name that prefixes the rest.
            let group = (0..=u8::MAX)
                .filter_map(|group| {
                    let suffix = constants::flag_group(group)?.strip_prefix("FG_")?;
                    let tail = rest.strip_prefix(suffix)?;
                    tail.starts_with('_').then_some((group, suffix.len()))
                })
                .max_by_key(|&(_, len)| len);
            if let Some((group, _)) = group {
                self.flags.entry((group, byte)).or_insert(name);
            }
        }
    }

    /// Read message text for comments from a language file
    /// (`KEY = "text"` lines); control codes are dropped.
    pub fn add_messages(&mut self, language: &str) {
        for line in language.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            if !(key.starts_with("MESSAGE_") || key.contains("_TEXT_")) {
                continue;
            }
            let Some(value) = value
                .trim()
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
            else {
                continue;
            };
            let text = plain_text(value);
            if !text.is_empty() {
                self.messages.insert(key.to_string(), text);
            }
        }
    }

    /// The text `message(_, id)` shows in RDT `room`: a global message, or
    /// the room's own text for this player, then for either.
    pub(super) fn message(&self, room: u16, id: u16) -> Option<&str> {
        if id & 0x40 != 0 {
            return self
                .messages
                .get(&format!("MESSAGE_{:02X}", id & 0x3F))
                .map(String::as_str);
        }
        let room3 = format!("{:X}", room >> 4);
        let player = room & 0xF;
        self.messages
            .get(&format!("ROOM_{room3}{player}_TEXT_{id:02X}"))
            .or_else(|| self.messages.get(&format!("ROOM_{room3}_TEXT_{id:02X}")))
            .map(String::as_str)
    }

    pub(super) fn includes(&self) -> &[String] {
        &self.includes
    }

    pub(super) fn defines(&self) -> &HashMap<String, i64> {
        &self.defines
    }

    pub(super) fn item(&self, value: u8) -> Option<&str> {
        self.items.get(&value).map(String::as_str)
    }

    pub(super) fn enemy(&self, value: u8) -> Option<&str> {
        self.enemies.get(&value).map(String::as_str)
    }

    pub(super) fn flag(&self, group: u8, bit: u8) -> Option<&str> {
        self.flags.get(&(group, bit)).map(String::as_str)
    }

    /// A door target in a room of `stage`: a room of the same stage alone,
    /// any other as `STAGE_n | ROOM_n..`.
    pub(super) fn door_target(&self, stage: u8, value: u8) -> Option<String> {
        if value < 0x20 {
            return self.rooms.get(&(stage, value)).cloned();
        }
        let target = value >> 5;
        let stage_name = self.stages.get(&target)?;
        let room = value & 0x1F;
        let room = self
            .rooms
            .get(&(target, room))
            .cloned()
            .unwrap_or_else(|| room.to_string());
        Some(format!("{stage_name} | {room}"))
    }
}

/// A language value as one plain line: escapes resolved, `{codes}` dropped
/// and whitespace collapsed.
fn plain_text(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => match chars.next() {
                Some('n') => out.push(' '),
                Some(other) => out.push(other),
                None => {}
            },
            '{' => {
                for ch in chars.by_ref() {
                    if ch == '}' {
                        break;
                    }
                }
            }
            _ => out.push(ch),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Read every `*.akinc` in the `inc/` directory found in `dir` or the
/// nearest directory above it, as `#include "inc/<file>"` names.
pub fn load_names(dir: &std::path::Path) -> Result<Names> {
    let Some(inc) = dir
        .ancestors()
        .map(|base| base.join("inc"))
        .find(|inc| inc.is_dir())
    else {
        return Ok(Names::default());
    };
    let mut names = Names::default();
    let mut paths: Vec<_> = std::fs::read_dir(&inc)
        .with_context(|| format!("failed to read {}", inc.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "akinc")
        })
        .collect();
    paths.sort();
    let mut files = Vec::new();
    for path in paths {
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        files.push((format!("inc/{name}"), text));
    }
    if !files.is_empty() {
        names = Names::parse(&files)?;
    }
    // Message comments read the English text beside the includes.
    if let Some(base) = inc.parent() {
        let language = base.join("language/en-US.txt");
        if language.is_file() {
            let text = std::fs::read_to_string(&language)
                .with_context(|| format!("failed to read {}", language.display()))?;
            names.add_messages(&text);
        }
    }
    Ok(names)
}
