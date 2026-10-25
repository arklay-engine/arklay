//! The GUI's own strings in every language the game ships.
//!
//! The in-game menu and the options screen belong to the engine, not to a
//! game, so their text lives here rather than in a pack's `language/` files.
//! Each `locale/<tag>.txt` holds `key = "value"` lines (`#` starts a comment
//! line; `\n`, `\"` and `\\` escape). A key a file leaves out falls back to
//! the same language in another region, then to English, then to the key
//! itself, so a partial translation still draws everything.

use std::collections::HashMap;
use std::sync::Arc;

use crate::language::{FALLBACK_TAG, normalize_tag};

/// The embedded string files, by tag.
const FILES: [(&str, &str); 8] = [
    ("en-US", include_str!("locale/en-US.txt")),
    ("ja-JP", include_str!("locale/ja-JP.txt")),
    ("fr-FR", include_str!("locale/fr-FR.txt")),
    ("de-DE", include_str!("locale/de-DE.txt")),
    ("it-IT", include_str!("locale/it-IT.txt")),
    ("es-ES", include_str!("locale/es-ES.txt")),
    ("ru-RU", include_str!("locale/ru-RU.txt")),
    ("fi-FI", include_str!("locale/fi-FI.txt")),
];

/// The strings for one language, resolved through its fallback chain.
#[derive(Debug, Clone)]
pub struct Locale {
    tag: String,
    strings: Arc<HashMap<String, String>>,
}

impl Default for Locale {
    fn default() -> Self {
        Self::new(FALLBACK_TAG)
    }
}

impl Locale {
    /// The strings for `tag` (any form [`normalize_tag`] accepts).
    pub fn new(tag: &str) -> Self {
        let tag = normalize_tag(tag);
        let language = tag.split('-').next().unwrap_or("").to_string();
        let mut strings = HashMap::new();
        // Lowest priority first: English, then the language in any region,
        // then the exact tag.
        let layers = FILES
            .iter()
            .filter(|(file, _)| file.eq_ignore_ascii_case(FALLBACK_TAG))
            .chain(FILES.iter().filter(|(file, _)| {
                !file.eq_ignore_ascii_case(&tag)
                    && file
                        .split('-')
                        .next()
                        .is_some_and(|part| part.eq_ignore_ascii_case(&language))
            }))
            .chain(
                FILES
                    .iter()
                    .filter(|(file, _)| file.eq_ignore_ascii_case(&tag)),
            );
        for (_, source) in layers {
            strings.extend(parse(source));
        }
        Self {
            tag,
            strings: Arc::new(strings),
        }
    }

    /// The tag these strings were resolved for.
    pub fn tag(&self) -> &str {
        &self.tag
    }

    /// The string for `key`, or the key itself when no language has it.
    pub fn get<'a>(&'a self, key: &'a str) -> &'a str {
        self.strings.get(key).map_or(key, String::as_str)
    }

    /// The string for `key` with its `{}` replaced by `value`.
    pub fn format(&self, key: &str, value: impl std::fmt::Display) -> String {
        self.get(key).replacen("{}", &value.to_string(), 1)
    }

    /// Every character the strings use, for warming the fallback faces.
    pub fn characters(&self) -> String {
        let mut chars: Vec<char> = self.strings.values().flat_map(|s| s.chars()).collect();
        chars.sort_unstable();
        chars.dedup();
        chars.into_iter().collect()
    }
}

/// Parse one file's `key = "value"` lines; a malformed line is skipped (the
/// tests hold the shipped files to the format).
fn parse(source: &str) -> impl Iterator<Item = (String, String)> + '_ {
    source.lines().filter_map(parse_line)
}

fn parse_line(line: &str) -> Option<(String, String)> {
    let line = line.trim().trim_start_matches('\u{FEFF}');
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let (key, value) = line.split_once('=')?;
    let value = value.trim().strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next()? {
            'n' => out.push('\n'),
            other => out.push(other),
        }
    }
    Some((key.trim().to_string(), out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_file_parses_and_knows_only_english_keys() {
        let english: HashMap<String, String> = parse(FILES[0].1).collect();
        for (tag, source) in FILES {
            for (index, line) in source.lines().enumerate() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }
                let (key, _) = parse_line(line)
                    .unwrap_or_else(|| panic!("{tag} line {}: malformed", index + 1));
                assert!(english.contains_key(&key), "{tag}: unknown key `{key}`");
            }
        }
    }

    #[test]
    fn every_language_translates_every_key() {
        let english: HashMap<String, String> = parse(FILES[0].1).collect();
        for (tag, source) in &FILES[1..] {
            let strings: HashMap<String, String> = parse(source).collect();
            let mut missing: Vec<_> = english
                .keys()
                .filter(|k| !strings.contains_key(*k))
                .collect();
            missing.sort();
            assert!(missing.is_empty(), "{tag} lacks {missing:?}");
        }
    }

    #[test]
    fn falls_back_by_language_then_english_then_key() {
        assert_eq!(Locale::new("de-DE").get("menu.quit"), "Beenden");
        assert_eq!(Locale::new("de_AT.UTF-8").get("menu.quit"), "Beenden");
        assert_eq!(Locale::new("xx").get("menu.quit"), "Quit");
        assert_eq!(Locale::new("de-DE").get("no.such.key"), "no.such.key");
        assert_eq!(
            Locale::new("en-US").format("options.default", "On"),
            "(Default: On)"
        );
    }
}
