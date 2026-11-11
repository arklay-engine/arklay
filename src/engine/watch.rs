//! Hot reload for mod source directories.
//!
//! `--watch` polls the filesystem pack and every layered mod source directory
//! for changed `.lua` files, `bot.xml` metadata and the bot's
//! `knowledge/*.xml`. [`Watch::classify`] sorts each change by what it
//! belongs to, so the engine reloads only that: the pack's Lua hooks, the
//! restricted `bot/**.lua` route (with its knowledge), or the bot's room
//! metadata. Room XML, room scripts, assets and `.akpak` layers are
//! deliberately out of scope and need a room reload or a restart.
//!
//! A directory pack's entries are read lazily from disk ([`Pack::read`]), so a
//! changed file is visible to a fresh [`crate::lua::LuaVm`] or to the bot VM
//! without reopening the pack. The watcher itself only tracks file metadata:
//! it scans at [`POLL_INTERVAL`] and compares each file's modification time and
//! length with the baseline from the previous scan.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use crate::pack::Pack;

/// How often [`Watch::poll`] rescans the source directories.
pub const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// One file's metadata, the unit the baseline compares.
type Stamp = (Option<SystemTime>, u64);

/// What a changed file belongs to, which decides what the engine reloads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Changed {
    /// Any other `.lua` file: the pack's hooks and screens, which run in the
    /// session's VM.
    Hooks,
    /// The bot's `bot/**.lua` route or its `knowledge/*.xml`.
    BotRoute,
    /// A room's `bot.xml` metadata.
    BotMeta,
}

/// The watched source directories and the last scan's file stamps.
pub struct Watch {
    dirs: Vec<PathBuf>,
    baseline: BTreeMap<PathBuf, Stamp>,
    interval: Duration,
    last_poll: Option<Instant>,
}

impl Watch {
    /// Collect the filesystem source directories of `pack` and its layers.
    ///
    /// `enabled` is the CLI's `--watch`; a disabled watcher is not created.
    /// `extra` adds directories beyond the pack's own sources (the tests use
    /// it). A source that is an `.akpak` file is not watched: its entries live
    /// in memory and a change needs a restart. When no directory is left, a
    /// warning says so and `None` is returned.
    pub fn new(enabled: bool, pack: &Pack, extra: &[PathBuf]) -> Option<Self> {
        if !enabled {
            return None;
        }
        let mut dirs = Vec::new();
        let mut seen = HashSet::new();
        for source in std::iter::once(pack.source())
            .chain(pack.overrides().map(Pack::source))
            .flatten()
        {
            add_source(source, &mut dirs, &mut seen);
        }
        for source in extra {
            add_source(source, &mut dirs, &mut seen);
        }
        if dirs.is_empty() {
            eprintln!(
                "[watch] no script/metadata source directories to watch; \
                 .akpak layers need a restart to change"
            );
            return None;
        }
        Some(Self {
            dirs,
            baseline: BTreeMap::new(),
            interval: POLL_INTERVAL,
            last_poll: None,
        })
    }

    /// Scan for changed, added or removed `.lua` and `bot.xml` files.
    ///
    /// The first call establishes the baseline and returns nothing. Later calls
    /// return the paths whose modification time or length changed, plus files
    /// added or removed since the previous scan. The scan only runs once per
    /// [`Self::interval`], so calling this every frame is cheap.
    pub fn poll(&mut self) -> Vec<PathBuf> {
        if let Some(last) = self.last_poll
            && last.elapsed() < self.interval
        {
            return Vec::new();
        }
        let current = scan(&self.dirs);
        let first = self.last_poll.is_none();
        let mut changed = Vec::new();
        if !first {
            for (path, stamp) in &current {
                if self.baseline.get(path) != Some(stamp) {
                    changed.push(path.clone());
                }
            }
            for path in self.baseline.keys() {
                if !current.contains_key(path) {
                    changed.push(path.clone());
                }
            }
            changed.sort();
            changed.dedup();
        }
        if first {
            eprintln!(
                "[watch] watching {} script/metadata file(s) in {} source director{}",
                current.len(),
                self.dirs.len(),
                if self.dirs.len() == 1 { "y" } else { "ies" }
            );
        }
        self.baseline = current;
        self.last_poll = Some(Instant::now());
        changed
    }

    /// What `path`, one of [`Self::poll`]'s changes, belongs to. The top
    /// directory is read relative to the watched source that holds it.
    pub fn classify(&self, path: &Path) -> Changed {
        if is_bot_meta(path) {
            return Changed::BotMeta;
        }
        let relative = self
            .dirs
            .iter()
            .find_map(|dir| path.strip_prefix(dir).ok())
            .unwrap_or(path);
        let top = relative
            .components()
            .next()
            .and_then(|component| component.as_os_str().to_str());
        let bot = top.is_some_and(|top| top.eq_ignore_ascii_case("bot"))
            || (top.is_some_and(|top| top.eq_ignore_ascii_case("knowledge"))
                && has_extension(path, "xml"));
        if bot {
            Changed::BotRoute
        } else {
            Changed::Hooks
        }
    }
}

/// Add one pack source if it is a directory. A file source is an `.akpak` (or
/// another single-file pack) and cannot be watched in place.
fn add_source(source: &Path, dirs: &mut Vec<PathBuf>, seen: &mut HashSet<PathBuf>) {
    let Ok(metadata) = fs::metadata(source) else {
        return;
    };
    if metadata.is_dir() {
        let canonical = fs::canonicalize(source).unwrap_or_else(|_| source.to_path_buf());
        if seen.insert(canonical.clone()) {
            dirs.push(canonical);
        }
    } else if metadata.is_file() {
        eprintln!(
            "[watch] {} is a pack file; changes to it need a restart",
            source.display()
        );
    }
}

/// Every `.lua` file below `dirs`, keyed by path with its metadata.
fn scan(dirs: &[PathBuf]) -> BTreeMap<PathBuf, Stamp> {
    let mut files = BTreeMap::new();
    for dir in dirs {
        scan_dir(dir, &mut files);
    }
    files
}

fn scan_dir(dir: &Path, files: &mut BTreeMap<PathBuf, Stamp>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            scan_dir(&path, files);
        } else if file_type.is_file() && is_watched(&path) {
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            files.insert(path, (metadata.modified().ok(), metadata.len()));
        }
    }
}

fn is_watched(path: &Path) -> bool {
    has_extension(path, "lua") || is_bot_meta(path) || is_bot_knowledge(path)
}

fn is_bot_meta(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("bot.xml"))
}

/// A `knowledge/<game>.xml`, the bot's combat knowledge.
fn is_bot_knowledge(path: &Path) -> bool {
    has_extension(path, "xml")
        && path
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("knowledge"))
}

fn has_extension(path: &Path, wanted: &str) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(wanted))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("arklay-watch-{}-{label}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn poll_reports_changed_bot_xml_but_not_room_xml() {
        let temp = TempDir::new("bot-meta");
        let room = temp.path.join("room/117");
        fs::create_dir_all(&room).unwrap();
        let metadata = room.join("bot.xml");
        let room_xml = room.join("room.xml");
        let other_xml = room.join("not-bot.xml");
        fs::write(&metadata, b"<bot format=\"1\"/>\n").unwrap();
        fs::write(&room_xml, b"<room format=\"1\"/>\n").unwrap();
        fs::write(&other_xml, b"<other/>\n").unwrap();

        let pack = Pack::open(&temp.path).unwrap();
        let mut watch = Watch::new(true, &pack, &[]).expect("a directory source");
        assert!(watch.poll().is_empty());

        fs::write(&metadata, b"<bot format=\"1\"><puzzle id=\"p\"/></bot>\n").unwrap();
        fs::write(&room_xml, b"<room format=\"1\"><lights/></room>\n").unwrap();
        fs::write(&other_xml, b"<other changed=\"true\"/>\n").unwrap();

        watch.interval = Duration::ZERO;
        assert_eq!(watch.poll(), vec![metadata]);
    }

    #[test]
    fn poll_reports_changed_added_and_removed_lua_files() {
        let temp = TempDir::new("poll");
        let lua = temp.path.join("lua");
        fs::create_dir_all(&lua).unwrap();
        let changed = lua.join("hook.lua");
        let removed = lua.join("old.lua");
        fs::write(&changed, b"local value = 1\n").unwrap();
        fs::write(&removed, b"local old = true\n").unwrap();

        let pack = Pack::open(&temp.path).unwrap();
        let mut watch = Watch::new(true, &pack, &[]).expect("a directory source");
        // The first poll establishes the baseline.
        assert!(watch.poll().is_empty());

        // A length change is visible even when the filesystem's mtime
        // granularity is coarse; the other two cover added/removed files.
        fs::write(&changed, b"local value = 2 -- changed\n").unwrap();
        fs::remove_file(&removed).unwrap();
        let added = lua.join("new.lua");
        fs::write(&added, b"local new = true\n").unwrap();

        // The default interval is 250 ms; the test has no sleeps.
        watch.interval = Duration::ZERO;
        let mut changed_paths = watch.poll();
        changed_paths.sort();
        let mut expected = vec![added, changed, removed];
        expected.sort();
        assert_eq!(changed_paths, expected);
    }

    #[test]
    fn classify_sorts_changes_by_what_they_reload() {
        let temp = TempDir::new("classify");
        for dir in ["lua", "bot/sub", "knowledge", "room/117"] {
            fs::create_dir_all(temp.path.join(dir)).unwrap();
        }
        let hook = temp.path.join("lua/hook.lua");
        let route = temp.path.join("bot/sub/planner.lua");
        let knowledge = temp.path.join("knowledge/re1.xml");
        let meta = temp.path.join("room/117/bot.xml");
        for path in [&hook, &route, &knowledge, &meta] {
            fs::write(path, b"-- 1\n").unwrap();
        }

        let pack = Pack::open(&temp.path).unwrap();
        let mut watch = Watch::new(true, &pack, &[]).expect("a directory source");
        assert!(watch.poll().is_empty());
        for path in [&hook, &route, &knowledge, &meta] {
            fs::write(path, b"-- 2, changed\n").unwrap();
        }
        watch.interval = Duration::ZERO;
        let changed = watch.poll();
        assert_eq!(changed.len(), 4, "every kind is watched: {changed:?}");
        let canonical = |path: &Path| fs::canonicalize(path).unwrap();
        assert_eq!(watch.classify(&canonical(&hook)), Changed::Hooks);
        assert_eq!(watch.classify(&canonical(&route)), Changed::BotRoute);
        assert_eq!(watch.classify(&canonical(&knowledge)), Changed::BotRoute);
        assert_eq!(watch.classify(&canonical(&meta)), Changed::BotMeta);
    }
}
