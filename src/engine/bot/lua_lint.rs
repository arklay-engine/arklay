//! A lint over the bot mod's Lua (`mods/bot/bot/*.lua`): the planner must be
//! generic, so room facts live in `room/<id>/bot.xml` and the Rust side, not
//! in the Lua.
//!
//! The rule, outside comments and outside `constants.lua`:
//! - no string literal that looks like a room id: exactly three characters,
//!   a decimal digit followed by two hex digits (`"10F"`, `"117"`, `"20D"`);
//! - no hex number literal of `0x10` or more;
//! - no decimal number literal of `100` or more.
//!
//! `constants.lua` is the one named exception. It must stay small (at most
//! [`MAX_CONSTANTS`] assignments and [`MAX_CONSTANT_LINES`] lines), and every
//! assignment in it needs a `--` comment, on the same line or the line above,
//! saying which engine rule it reflects. It may not hold room-id strings
//! either.

use std::path::{Path, PathBuf};

/// The file the number rule exempts.
const CONSTANTS_FILE: &str = "constants.lua";
/// The most assignments `constants.lua` may hold.
const MAX_CONSTANTS: usize = 40;
/// The most lines `constants.lua` may have.
const MAX_CONSTANT_LINES: usize = 140;

/// One literal the lexer found.
#[derive(Debug, PartialEq)]
enum Literal {
    Str(String),
    Hex(String),
    Dec(String),
}

/// Lex `source` into its string and number literals with their 1-based
/// line, skipping comments. Enough Lua 5.4 for the lint: short and long
/// strings, short and long comments, hex and decimal numerals.
fn literals(source: &str) -> Vec<(usize, Literal)> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let mut line = 1;
    // `[[`, `[=[`, ...: the level, if a long bracket opens at `at`.
    let long_open = |at: usize| -> Option<usize> {
        if bytes.get(at) != Some(&b'[') {
            return None;
        }
        let mut level = 0;
        while bytes.get(at + 1 + level) == Some(&b'=') {
            level += 1;
        }
        (bytes.get(at + 1 + level) == Some(&b'[')).then_some(level)
    };
    // Skip a long bracket body opened at `at` with `level`; returns the end.
    let long_close = |at: usize, level: usize, line: &mut usize| -> usize {
        let mut j = at + level + 2;
        while j < bytes.len() {
            if bytes[j] == b'\n' {
                *line += 1;
            }
            if bytes[j] == b']'
                && bytes[j + 1..].iter().take(level).all(|&b| b == b'=')
                && bytes.get(j + 1 + level) == Some(&b']')
            {
                return j + level + 2;
            }
            j += 1;
        }
        bytes.len()
    };
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'\n' {
            line += 1;
            i += 1;
        } else if c == b'-' && bytes.get(i + 1) == Some(&b'-') {
            if let Some(level) = long_open(i + 2) {
                i = long_close(i + 2, level, &mut line);
            } else {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
        } else if c == b'"' || c == b'\'' {
            let start_line = line;
            let mut text = String::new();
            i += 1;
            while i < bytes.len() && bytes[i] != c {
                if bytes[i] == b'\\' {
                    i += 1;
                }
                if bytes.get(i) == Some(&b'\n') {
                    line += 1;
                }
                if let Some(&b) = bytes.get(i) {
                    text.push(b as char);
                }
                i += 1;
            }
            i += 1;
            out.push((start_line, Literal::Str(text)));
        } else if let Some(level) = long_open(i) {
            let start_line = line;
            let end = long_close(i, level, &mut line);
            let open = i + level + 2;
            let close = end.saturating_sub(level + 2).max(open).min(source.len());
            out.push((
                start_line,
                Literal::Str(source[open.min(close)..close].to_string()),
            ));
            i = end;
        } else if c.is_ascii_digit()
            || (c == b'.' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit))
        {
            let ident_before =
                i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
            let start = i;
            let hex = c == b'0' && matches!(bytes.get(i + 1), Some(b'x' | b'X'));
            if hex {
                i += 2;
                while i < bytes.len() && (bytes[i].is_ascii_hexdigit() || bytes[i] == b'.') {
                    i += 1;
                }
            } else {
                while i < bytes.len() {
                    let b = bytes[i];
                    let exponent_sign =
                        matches!(b, b'+' | b'-') && matches!(bytes[i - 1], b'e' | b'E');
                    if b.is_ascii_digit() || b == b'.' || b == b'e' || b == b'E' || exponent_sign {
                        i += 1;
                    } else {
                        break;
                    }
                }
            }
            if !ident_before {
                let text = source[start..i].to_string();
                out.push((
                    line,
                    if hex {
                        Literal::Hex(text)
                    } else {
                        Literal::Dec(text)
                    },
                ));
            }
        } else if c.is_ascii_alphabetic() || c == b'_' {
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    out
}

/// Whether a string literal spells a room id: a digit and two hex digits.
fn is_room_id(text: &str) -> bool {
    let b = text.as_bytes();
    b.len() == 3 && b[0].is_ascii_digit() && b[1..].iter().all(u8::is_ascii_hexdigit)
}

/// The rule's violations in one file's source.
fn violations(source: &str, constants: bool) -> Vec<String> {
    let mut found = Vec::new();
    for (line, literal) in literals(source) {
        match literal {
            Literal::Str(text) if is_room_id(&text) => {
                found.push(format!("line {line}: room id string \"{text}\""));
            }
            Literal::Hex(text) if !constants => {
                let digits = text[2..].split('.').next().unwrap_or("");
                let value = u64::from_str_radix(digits, 16).unwrap_or(u64::MAX);
                if value >= 0x10 {
                    found.push(format!("line {line}: hex literal {text} (>= 0x10)"));
                }
            }
            Literal::Dec(text) if !constants => {
                let value: f64 = text.parse().unwrap_or(f64::MAX);
                if value >= 100.0 {
                    found.push(format!("line {line}: decimal literal {text} (>= 100)"));
                }
            }
            _ => {}
        }
    }
    found
}

/// `constants.lua`'s own rules: small, and every assignment commented.
fn constants_violations(source: &str) -> Vec<String> {
    let mut found = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    if lines.len() > MAX_CONSTANT_LINES {
        found.push(format!(
            "{} lines (at most {MAX_CONSTANT_LINES})",
            lines.len()
        ));
    }
    let mut count = 0;
    for (index, raw) in lines.iter().enumerate() {
        let code = raw.split("--").next().unwrap_or("").trim();
        // An assignment of a named constant: `Name.FIELD = ...`.
        let is_constant =
            code.starts_with("BotConst.") && code.contains('=') && !code.contains("==");
        if !is_constant {
            continue;
        }
        count += 1;
        let commented =
            raw.contains("--") || index > 0 && lines[index - 1].trim_start().starts_with("--");
        if !commented {
            found.push(format!("line {}: constant without a comment", index + 1));
        }
    }
    if count > MAX_CONSTANTS {
        found.push(format!("{count} constants (at most {MAX_CONSTANTS})"));
    }
    found
}

fn lua_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|err| panic!("read {}: {err}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "lua"))
        .collect();
    files.sort();
    files
}

#[test]
fn the_bot_lua_has_no_room_literals_outside_the_constants_module() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("mods/bot/bot");
    let files = lua_files(&dir);
    assert!(
        files.iter().any(|path| path.ends_with(CONSTANTS_FILE)),
        "{} has no {CONSTANTS_FILE}",
        dir.display()
    );
    let mut report = Vec::new();
    for path in files {
        let source = std::fs::read_to_string(&path).unwrap();
        let constants = path.ends_with(CONSTANTS_FILE);
        let mut found = violations(&source, constants);
        if constants {
            found.extend(constants_violations(&source));
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        report.extend(found.into_iter().map(|item| format!("{name}: {item}")));
    }
    assert!(
        report.is_empty(),
        "room literals in the bot Lua (move them to room/<id>/bot.xml, or a \
         commented engine rule in {CONSTANTS_FILE}):\n{}",
        report.join("\n")
    );
}

#[test]
fn the_lint_finds_room_literals_and_skips_comments() {
    let source = r#"
        -- "10F" 0x7C 1200 in a comment
        --[[ "117" 0xFF ]]
        local a = "20D"
        local b = 0x7C + 0x0F
        local c = 1200 + 99 + 2.5e3
        local d = [[117]]
        local e = "%02X" .. "abc" .. 'x' .. var100
    "#;
    let found = violations(source, false);
    assert_eq!(
        found,
        vec![
            "line 4: room id string \"20D\"",
            "line 5: hex literal 0x7C (>= 0x10)",
            "line 6: decimal literal 1200 (>= 100)",
            "line 6: decimal literal 2.5e3 (>= 100)",
            "line 7: room id string \"117\"",
        ]
    );
    assert!(violations("local x = 1200 + 0xFF", true).is_empty());
    assert_eq!(violations("local x = '117'", true).len(), 1);
    let constants = "-- engine rule\nBotConst.A = 1\nBotConst.B = 2\nBotConst.C = 3 -- why\n";
    assert_eq!(
        constants_violations(constants),
        vec!["line 3: constant without a comment"]
    );
}
