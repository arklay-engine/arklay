//! The core pack: the engine's own GUI screens (`ui/*.xml`) and faces
//! (`fonts/*.ttf`), from `games/core/`.
//!
//! `build.rs` packs `games/core/` into `core.akpak`, which is embedded in the
//! binary and always loaded, so the GUI and the launcher work before (and
//! without) any game pack. It is not layered under a game pack: packs and mods
//! cannot replace its entries.

use std::sync::LazyLock;

use crate::pack::Pack;

/// The `core.akpak` image `build.rs` wrote.
static IMAGE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/core.akpak"));

/// The parsed core pack.
static CORE: LazyLock<Pack> = LazyLock::new(|| {
    Pack::from_bytes(IMAGE.to_vec()).unwrap_or_else(|err| panic!("embedded core pack: {err:#}"))
});

/// The always-loaded core pack.
pub fn core() -> &'static Pack {
    &CORE
}

/// The bytes of the core entry `path`.
///
/// Panics when the entry is missing: every caller names a file checked in
/// under `games/core/`, so a miss is a build error, not bad data.
pub fn bytes(path: &str) -> &'static [u8] {
    match core().read(path) {
        Ok(std::borrow::Cow::Borrowed(bytes)) => bytes,
        Ok(std::borrow::Cow::Owned(_)) => panic!("core entry {path} is compressed"),
        Err(err) => panic!("core entry {path}: {err:#}"),
    }
}

/// The text of the core entry `path`, as [`bytes`]; panics unless it is
/// UTF-8.
pub fn text(path: &str) -> &'static str {
    std::str::from_utf8(bytes(path)).unwrap_or_else(|err| panic!("core entry {path}: {err}"))
}

/// The GUI documents (`ui/<name>.xml`) whose name passes `keep`, as
/// `(name, source)` in path order.
pub fn documents(
    keep: impl Fn(&str) -> bool,
) -> impl Iterator<Item = (&'static str, &'static str)> {
    core().paths().filter_map(move |path| {
        let name = path.strip_prefix("ui/")?.strip_suffix(".xml")?;
        (!name.contains('/') && keep(name)).then(|| (name, text(path)))
    })
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::pack::PackWriter;

    fn files(dir: &Path, prefix: &str, out: &mut Vec<(String, Vec<u8>)>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                files(&path, &format!("{prefix}{name}/"), out);
            } else {
                out.push((format!("{prefix}{name}"), std::fs::read(&path).unwrap()));
            }
        }
    }

    fn checked_in() -> Vec<(String, Vec<u8>)> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("games/core");
        let mut out = Vec::new();
        files(&root, "", &mut out);
        out
    }

    #[test]
    fn embeds_every_checked_in_file() {
        let mut expected: Vec<String> = checked_in().into_iter().map(|(path, _)| path).collect();
        expected.sort();
        let mut actual: Vec<String> = core().paths().map(str::to_owned).collect();
        actual.sort();
        assert_eq!(actual, expected);
        for (path, data) in checked_in() {
            assert_eq!(bytes(&path), data.as_slice(), "{path}");
        }
    }

    #[test]
    fn image_matches_the_pack_writer() {
        let mut writer = PackWriter::new();
        for (path, data) in checked_in() {
            writer.add(&path, data).unwrap();
        }
        assert!(
            writer.to_bytes().unwrap() == IMAGE,
            "build.rs and PackWriter disagree"
        );
    }

    #[test]
    fn holds_the_fonts_and_screens() {
        for font in [
            "fonts/barlow_regular.ttf",
            "fonts/barlow_bold.ttf",
            "fonts/departuremono_regular.ttf",
        ] {
            assert!(!bytes(font).is_empty(), "{font}");
        }
        assert!(documents(|name| name == "options").count() == 1);
        assert!(documents(|name| name.starts_with("launcher_")).count() > 0);
    }
}
