//! Mod discovery and the mod pack builder.
//!
//! A mod source directory is convention over configuration: it must hold a
//! `manifest.toml` declaring `kind = "mod"`, and every other file maps to a
//! pack entry named after its directory-relative path with `/` separators.
//! `room/<id>/<stem>.s` and `.akscr` sources are built into
//! `room/<id>/<stem>.scd` entries, and a merged `room/<room>.akscr` into
//! `room/<id>/room.scd` for every variant of the room; the text source itself
//! is not packed. Every other file, including other `.toml` files, is packed
//! verbatim.
//!
//! [`discover_mods`] scans a pack's sibling `mods/` directory and the per-user
//! `<config dir>/mods` directory for top-level `*.akpak` files and mod source
//! directories in stable sorted order; the engine then filters and orders the
//! layers from the user configuration when it opens them.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::manifest;
use crate::pack::{Pack, PackWriter};

/// The result of building a mod pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModSummary {
    /// The written pack entries in table-of-contents order: path and size.
    pub entries: Vec<(String, usize)>,
    /// Total bytes held by the entries.
    pub bytes: usize,
    /// Aggregated non-fatal problems, e.g. overrides that are new files.
    pub warnings: Vec<String>,
}

/// Whether `path` overrides an entry of `base` in the merged view.
///
/// A mod entry whose stem matches a base entry of the same class (for example
/// `room/1000/cut/00.png` over `room/1000/cut/00.bmp`) is an override even
/// though its exact path is new, so the new-file warning only names entries
/// the base could never serve.
fn is_override(base: &Pack, path: &str) -> bool {
    if base.contains(path) {
        return true;
    }
    use crate::pack::AssetClass;
    let extension = crate::pack::extension_of(path);
    let class = [AssetClass::Audio, AssetClass::Movie]
        .into_iter()
        .find(|class| class.extensions().contains(&extension.as_str()))
        .unwrap_or(AssetClass::Image);
    base.has_asset(crate::assets::stem(path), class)
}

/// Build the mod source directory `dir` into the pack `out`.
///
/// When `base` is given, the source manifest's declared base id must match the
/// base pack's id (or its stem when the base has no manifest), and every
/// override path is checked against the base: paths the base does not hold are
/// reported together as one warning and never fail the build.
pub fn build_mod(dir: &Path, out: &Path, base: Option<&Path>) -> Result<ModSummary> {
    if !dir.is_dir() {
        bail!("mod source directory {} does not exist", dir.display());
    }
    let manifest_path = dir.join(manifest::ENTRY);
    let text = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read mod manifest {}", manifest_path.display()))?;
    let manifest = manifest::Manifest::parse(&text)
        .with_context(|| format!("invalid mod manifest {}", manifest_path.display()))?;
    if manifest.kind != manifest::PackKind::Mod {
        bail!(
            "{}: a mod source must declare kind = \"mod\", found \"{}\"",
            manifest_path.display(),
            manifest.kind
        );
    }

    let mut warnings = Vec::new();
    let mut entries: Vec<(String, Vec<u8>)> =
        vec![(manifest::ENTRY.to_string(), manifest.render().into_bytes())];
    let mut outputs: HashMap<String, String> = HashMap::new();
    outputs.insert(manifest::ENTRY.to_string(), manifest::ENTRY.to_string());

    let mut files = Vec::new();
    collect_sources(dir, dir, "", &mut files)?;
    files.sort_by(|a, b| {
        a.0.to_ascii_lowercase()
            .cmp(&b.0.to_ascii_lowercase())
            .then_with(|| a.0.cmp(&b.0))
    });
    let source_paths: HashSet<String> = files
        .iter()
        .map(|(relative, _)| relative.to_ascii_lowercase())
        .collect();
    let out_canonical = fs::canonicalize(out).ok();

    for (relative, source) in &files {
        // The manifest lookup above is case-resolved by the filesystem; skip
        // whatever case variant was actually found so it is never packed
        // twice (and never collides with the rendered lowercase entry).
        if relative.eq_ignore_ascii_case(manifest::ENTRY) {
            continue;
        }
        if let Some(canonical) = &out_canonical
            && fs::canonicalize(source).ok().as_ref() == Some(canonical)
        {
            continue;
        }
        // Include files only feed the scripts that `#include` them.
        if relative
            .rsplit_once('.')
            .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("akinc"))
        {
            continue;
        }
        if let Some(base) = merged_scd_source(relative) {
            let text = fs::read_to_string(source)
                .with_context(|| format!("failed to read {}", source.display()))?;
            let dir = source.parent().unwrap_or(Path::new("."));
            let load = crate::scd::akscr::includes_from(dir);
            let mut by_region = Vec::new();
            for id in crate::scd::akscr::room_variants(base) {
                let variant = crate::scd::akscr::Variant::for_room(id);
                let compile = |variant| {
                    crate::scd::akscr::compile_with(&text, Some(variant), &load)
                        .and_then(|assembled| assembled.to_container())
                        .with_context(|| format!("failed to compile {relative} for room {id:04X}"))
                };
                let container = compile(variant)?;
                // `variant` is the USA build; check every other region.
                for region in crate::scd::akscr::Region::ALL {
                    if region != crate::scd::akscr::Region::Usa
                        && !by_region.contains(&region)
                        && compile(variant.in_region(region))? != container
                    {
                        by_region.push(region);
                    }
                }
                let output = format!("room/{id:04x}/room.scd");
                push_entry(&mut entries, &mut outputs, output, container, relative)?;
            }
            if !by_region.is_empty() {
                let regions: Vec<&str> = by_region.iter().map(|region| region.symbol()).collect();
                warnings.push(format!(
                    "{relative} builds differently for {}; the mod packs the USA build, which \
                     runs on every region",
                    regions.join(" and ")
                ));
            }
            continue;
        }
        if let Some(assembled) = scd_source_output(relative) {
            let container = assemble_scd(source)?;
            push_entry(&mut entries, &mut outputs, assembled, container, relative)?;
            continue;
        }
        let data =
            fs::read(source).with_context(|| format!("failed to read {}", source.display()))?;
        push_entry(&mut entries, &mut outputs, relative.clone(), data, relative)?;
    }

    if let Some(base_path) = base {
        let base_pack = Pack::open(base_path)
            .with_context(|| format!("failed to open base pack {}", base_path.display()))?;
        let declared = base_pack.manifest().map(|manifest| manifest.id.clone());
        let base_id = match declared {
            Some(id) => id,
            None => {
                let id = pack_stem(base_path);
                warnings.push(format!(
                    "base pack {} has no {}; checking against id \"{id}\"",
                    base_path.display(),
                    manifest::ENTRY
                ));
                id
            }
        };
        if manifest.base.as_deref() != Some(base_id.as_str()) {
            bail!(
                "mod declares base {:?} but {} declares id \"{base_id}\"",
                manifest.base.as_deref().unwrap_or_default(),
                base_path.display()
            );
        }
        let new_paths: Vec<&str> = entries
            .iter()
            .map(|(path, _)| path.as_str())
            // HD textures replace by pixel hash, not by path.
            .filter(|path| {
                *path != manifest::ENTRY
                    && !path.starts_with(crate::htexture::ROOT)
                    && !is_override(&base_pack, path)
            })
            .collect();
        if !new_paths.is_empty() {
            warnings.push(format!(
                "{} override(s) are new files not present in {}: {}",
                new_paths.len(),
                base_path.display(),
                new_paths.join(", ")
            ));
        }
    }

    let missing_lua: Vec<&str> = manifest
        .lua
        .iter()
        .map(String::as_str)
        .filter(|path| !source_paths.contains(&path.to_ascii_lowercase()))
        .collect();
    if !missing_lua.is_empty() {
        warnings.push(format!(
            "{} declared lua hook(s) are missing from {}: {}",
            missing_lua.len(),
            dir.display(),
            missing_lua.join(", ")
        ));
    }

    let mut summary_entries: Vec<(String, usize)> = entries
        .iter()
        .map(|(path, data)| (path.clone(), data.len()))
        .collect();
    summary_entries.sort_by(|a, b| {
        a.0.to_ascii_lowercase()
            .cmp(&b.0.to_ascii_lowercase())
            .then_with(|| a.0.cmp(&b.0))
    });
    let bytes = summary_entries.iter().map(|(_, size)| size).sum();

    let mut writer = PackWriter::new();
    for (path, data) in entries {
        writer
            .add(&path, data)
            .with_context(|| format!("failed to add pack entry {path}"))?;
    }
    writer
        .write(out)
        .with_context(|| format!("failed to write mod pack {}", out.display()))?;

    Ok(ModSummary {
        entries: summary_entries,
        bytes,
        warnings,
    })
}

/// Scan the sibling `mods/` directory of `pack` and the per-user `global`
/// directory for mod candidates.
///
/// Only top-level entries are considered: an `.akpak` file, or a directory
/// holding a `manifest.toml` mod source. Nested directories are not scanned.
/// Returns the paths in stable sorted order (lowercased file name, then full
/// path) and an empty list when a directory is absent. A candidate present in
/// both locations (for example through a symlink) is returned once.
pub fn discover_mods(pack: &Path, globals: &[PathBuf]) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::with_capacity(1 + globals.len());
    if let Some(dir) = pack.parent().map(|parent| parent.join("mods")) {
        dirs.push(dir);
    }
    dirs.extend(globals.iter().cloned());

    let mut mods: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    for dir in dirs {
        let Ok(read) = fs::read_dir(&dir) else {
            continue;
        };
        let mut candidates: Vec<PathBuf> = read
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| is_mod_candidate(path))
            .collect();
        candidates.sort_by(|a, b| {
            let a_name = a.file_name().unwrap_or_default().to_ascii_lowercase();
            let b_name = b.file_name().unwrap_or_default().to_ascii_lowercase();
            a_name.cmp(&b_name).then_with(|| a.cmp(b))
        });
        for candidate in candidates {
            let canonical = fs::canonicalize(&candidate).unwrap_or_else(|_| candidate.clone());
            if seen.insert(canonical) {
                mods.push(candidate);
            }
        }
    }
    mods.sort_by(|a, b| {
        let a_name = a.file_name().unwrap_or_default().to_ascii_lowercase();
        let b_name = b.file_name().unwrap_or_default().to_ascii_lowercase();
        a_name.cmp(&b_name).then_with(|| a.cmp(b))
    });
    mods
}

/// Whether a top-level directory entry is a mod candidate.
///
/// A file must be an `.akpak` image; a directory must hold a `manifest.toml`,
/// so an unrelated folder next to the packs is not treated as a mod. Both
/// checks follow symlinks, so a linked pack or mod directory is discovered.
fn is_mod_candidate(path: &Path) -> bool {
    if path.is_file() {
        return path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("akpak"));
    }
    path.is_dir() && path.join(manifest::ENTRY).is_file()
}

/// Assemble a text `.s` source, or compile an `.akscr` script, into a
/// standalone `.scd` container.
pub fn assemble_scd(source: &Path) -> Result<Vec<u8>> {
    let text = fs::read_to_string(source)
        .with_context(|| format!("failed to read {}", source.display()))?;
    let script = source
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("akscr"));
    let assembled = if script {
        let dir = source.parent().unwrap_or(Path::new("."));
        crate::scd::akscr::compile_with(&text, None, &crate::scd::akscr::includes_from(dir))
            .with_context(|| format!("failed to compile {}", source.display()))?
    } else {
        crate::scd::asm::assemble(&text)
            .with_context(|| format!("failed to assemble {}", source.display()))?
    };
    assembled
        .to_container()
        .with_context(|| format!("failed to lay out {}", source.display()))
}

/// The path below a leading `room/` directory, matched whatever its case.
fn under_room(relative: &str) -> Option<&str> {
    let (prefix, rest) = relative.split_once('/')?;
    prefix.eq_ignore_ascii_case("room").then_some(rest)
}

/// The pack entry a room script source `room/<id>/<stem>.s` or
/// `room/<id>/<stem>.akscr` builds into (`room/<id>/<stem>.scd`), if it is one.
fn scd_source_output(relative: &str) -> Option<String> {
    let (folder, name) = under_room(relative)?.split_once('/')?;
    if folder.len() != 4 || !folder.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let stem = [".s", ".akscr"].iter().find_map(|suffix| {
        name.get(..name.len().checked_sub(suffix.len())?)
            .filter(|stem| name[stem.len()..].eq_ignore_ascii_case(suffix))
    })?;
    if stem.is_empty() || stem.contains('/') {
        return None;
    }
    Some(format!(
        "{}{stem}.scd",
        &relative[..relative.len() - name.len()]
    ))
}

/// A merged room script `room/<room>.akscr` (three hex digits, e.g.
/// `room/103.akscr`): the room id. It builds `room/<id>/room.scd` for every
/// player and mansion variant of the room, for the USA region (`#if USA`): a
/// mod's script has no build per region, so the build warns when another
/// region's would differ.
fn merged_scd_source(relative: &str) -> Option<u16> {
    let name = under_room(relative)?;
    let (stem, extension) = name.split_at_checked(3)?;
    if !extension.eq_ignore_ascii_case(".akscr") || !stem.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u16::from_str_radix(stem, 16).ok()
}

/// Add one built entry, rejecting two sources that map to the same path.
fn push_entry(
    entries: &mut Vec<(String, Vec<u8>)>,
    outputs: &mut HashMap<String, String>,
    output: String,
    data: Vec<u8>,
    source: &str,
) -> Result<()> {
    if let Some(previous) = outputs.insert(output.to_ascii_lowercase(), source.to_string()) {
        bail!("`{previous}` and `{source}` both produce pack entry `{output}`");
    }
    entries.push((output, data));
    Ok(())
}

/// Collect every regular file under `dir` as `(relative path, absolute path)`.
fn collect_sources(
    root: &Path,
    dir: &Path,
    prefix: &str,
    out: &mut Vec<(String, PathBuf)>,
) -> Result<()> {
    let mut entries: Vec<fs::DirEntry> = fs::read_dir(dir)
        .with_context(|| format!("failed to read {}", dir.display()))?
        .collect::<std::io::Result<_>>()
        .with_context(|| format!("failed to read {}", dir.display()))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            bail!(
                "file name {:?} under {} is not valid UTF-8",
                name,
                root.display()
            );
        };
        let relative = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}/{name}")
        };
        let file_type = entry
            .file_type()
            .with_context(|| format!("failed to inspect {}", entry.path().display()))?;
        if file_type.is_dir() {
            collect_sources(root, &entry.path(), &relative, out)?;
        } else if file_type.is_file() {
            out.push((relative, entry.path()));
        }
    }
    Ok(())
}

/// The default id of a pack without a manifest: its file stem, or `pack`.
fn pack_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("pack")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Self-deleting temporary directory unique to this process and label.
    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("arklay-modding-{}-{label}", std::process::id()));
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

    fn write(path: &Path, data: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, data).unwrap();
    }

    fn mod_manifest_text(id: &str, base: &str) -> String {
        mod_manifest_text_with_lua(id, base, &[])
    }

    fn mod_manifest_text_with_lua(id: &str, base: &str, lua: &[&str]) -> String {
        manifest::Manifest {
            kind: manifest::PackKind::Mod,
            base: Some(base.to_string()),
            load_order: 10,
            lua: lua.iter().map(|path| path.to_string()).collect(),
            ..manifest::Manifest::base(id)
        }
        .render()
    }

    #[test]
    fn directory_convention_maps_and_packs_files() {
        let dir = TempDir::new("convention");
        let source = dir.path.join("source");
        write(
            &source.join("manifest.toml"),
            mod_manifest_text("demo", "re1").as_bytes(),
        );
        write(&source.join("lua/demo.lua"), b"return 1");
        write(&source.join("room/1000/cut/00.bmp"), b"bmp-bytes");
        write(&source.join("room/1000/room.scd"), b"scd-bytes");
        write(&source.join("sub/data.bin"), b"data");
        write(&source.join("notes.toml"), b"other = true");

        let out = dir.path.join("demo.akpak");
        let summary = build_mod(&source, &out, None).unwrap();

        assert_eq!(summary.warnings, Vec::<String>::new());
        assert_eq!(
            summary
                .entries
                .iter()
                .map(|(path, _)| path.as_str())
                .collect::<Vec<_>>(),
            [
                "lua/demo.lua",
                "manifest.toml",
                "notes.toml",
                "room/1000/cut/00.bmp",
                "room/1000/room.scd",
                "sub/data.bin",
            ]
        );
        assert_eq!(
            summary.bytes,
            summary.entries.iter().map(|(_, size)| size).sum::<usize>()
        );

        let pack = Pack::open(&out).unwrap();
        assert_eq!(
            pack.read("room/1000/room.scd").unwrap().as_ref(),
            b"scd-bytes"
        );
        assert_eq!(pack.read("notes.toml").unwrap().as_ref(), b"other = true");
        assert_eq!(pack.read("sub/data.bin").unwrap().as_ref(), b"data");
        let manifest = pack.manifest().unwrap();
        assert_eq!(manifest.id, "demo");
        assert_eq!(manifest.kind, manifest::PackKind::Mod);
        assert_eq!(manifest.base.as_deref(), Some("re1"));
        assert_eq!(manifest.load_order, 10);
    }

    #[test]
    fn scd_sources_are_assembled_and_not_packed() {
        let dir = TempDir::new("scd-source");
        let source = dir.path.join("source");
        write(
            &source.join("manifest.toml"),
            mod_manifest_text("demo", "re1").as_bytes(),
        );
        write(
            &source.join("room/1000/room.s"),
            b".version 1\n\n.main\n.block\n    nop                     0\n",
        );

        let out = dir.path.join("demo.akpak");
        let summary = build_mod(&source, &out, None).unwrap();
        assert_eq!(
            summary
                .entries
                .iter()
                .map(|(path, _)| path.as_str())
                .collect::<Vec<_>>(),
            ["manifest.toml", "room/1000/room.scd"]
        );

        let pack = Pack::open(&out).unwrap();
        assert!(!pack.contains("room/1000/room.s"));
        let scripts = crate::scd::reader::parse(&pack.read("room/1000/room.scd").unwrap()).unwrap();
        assert_eq!(scripts.main.len(), 1);
        assert_eq!(scripts.main[0].insns[0].bytes, [0x0E, 0x00]);
    }

    #[test]
    fn merged_room_scripts_build_every_variant() {
        let dir = TempDir::new("scd-merged");
        let source = dir.path.join("source");
        write(
            &source.join("manifest.toml"),
            mod_manifest_text("demo", "re1").as_bytes(),
        );
        write(
            &source.join("room/103.akscr"),
            b"#version 1\nproc main noreturn\n{\n#if JILL\n    nop(1);\n#endif\n#if MANSION2\n    nop(2);\n#endif\n}\n",
        );

        let out = dir.path.join("demo.akpak");
        let summary = build_mod(&source, &out, None).unwrap();
        assert_eq!(
            summary
                .entries
                .iter()
                .map(|(path, _)| path.as_str())
                .collect::<Vec<_>>(),
            [
                "manifest.toml",
                "room/1030/room.scd",
                "room/1031/room.scd",
                "room/6030/room.scd",
                "room/6031/room.scd"
            ]
        );

        let pack = Pack::open(&out).unwrap();
        for (path, expected) in [
            ("room/1030/room.scd", &[][..]),
            ("room/1031/room.scd", &[0x0E, 0x01][..]),
            ("room/6030/room.scd", &[0x0E, 0x02][..]),
            ("room/6031/room.scd", &[0x0E, 0x01, 0x0E, 0x02][..]),
        ] {
            let scripts = crate::scd::reader::parse(&pack.read(path).unwrap()).unwrap();
            let bytes: Vec<u8> = scripts.main[0]
                .insns
                .iter()
                .flat_map(|insn| insn.bytes.iter().copied())
                .collect();
            assert_eq!(bytes, expected, "{path}");
        }
        assert_eq!(summary.warnings, Vec::<String>::new());
    }

    #[test]
    fn a_merged_room_script_by_region_warns() {
        let dir = TempDir::new("scd-region");
        let source = dir.path.join("source");
        write(
            &source.join("manifest.toml"),
            mod_manifest_text("demo", "re1").as_bytes(),
        );
        write(
            &source.join("room/514.akscr"),
            b"#version 1\nproc main noreturn\n{\n#if JPN\n    nop(1);\n#endif\n}\n",
        );

        let summary = build_mod(&source, &dir.path.join("demo.akpak"), None).unwrap();
        assert_eq!(summary.warnings.len(), 1, "{:?}", summary.warnings);
        assert!(
            summary.warnings[0].contains("room/514.akscr")
                && summary.warnings[0].contains("for JPN;"),
            "{:?}",
            summary.warnings
        );
    }

    #[test]
    fn a_broken_scd_source_fails_the_build() {
        let dir = TempDir::new("scd-broken");
        let source = dir.path.join("source");
        write(
            &source.join("manifest.toml"),
            mod_manifest_text("demo", "re1").as_bytes(),
        );
        write(
            &source.join("room/1000/room.s"),
            b".version 1\n\n.init\n    frob 1\n",
        );

        let out = dir.path.join("demo.akpak");
        let err = build_mod(&source, &out, None).unwrap_err().to_string();
        assert!(err.contains("room.s"), "{err}");
        assert!(!out.exists(), "a failed build must not leave a pack behind");
    }

    #[test]
    fn scd_source_outputs_are_recognised_case_insensitively() {
        assert_eq!(
            scd_source_output("room/1000/room.s").as_deref(),
            Some("room/1000/room.scd")
        );
        assert_eq!(
            scd_source_output("room/1000/ROOM.S").as_deref(),
            Some("room/1000/ROOM.scd")
        );
        assert_eq!(
            scd_source_output("room/1000/room.AKSCR").as_deref(),
            Some("room/1000/room.scd")
        );
        assert_eq!(
            scd_source_output("Room/1000/room.s").as_deref(),
            Some("Room/1000/room.scd")
        );
        assert_eq!(scd_source_output("lua/demo.s"), None);
        assert_eq!(scd_source_output("room/1000/sub/room.s"), None);
        assert_eq!(scd_source_output("room/100/room.s"), None);
        assert_eq!(scd_source_output("room/1000/.akscr"), None);
        assert_eq!(scd_source_output("room/1000/.s"), None);
        assert_eq!(scd_source_output("room/1000/room.scd"), None);
    }

    #[test]
    fn merged_room_scripts_are_recognised() {
        assert_eq!(merged_scd_source("room/103.akscr"), Some(0x103));
        assert_eq!(merged_scd_source("ROOM/20A.AKSCR"), Some(0x20A));
        assert_eq!(merged_scd_source("room/1030/room.akscr"), None);
        assert_eq!(merged_scd_source("room/10x.akscr"), None);
        assert_eq!(merged_scd_source("room/103.s"), None);
        assert_eq!(merged_scd_source("lua/103.akscr"), None);
    }

    #[test]
    fn build_requires_a_mod_manifest() {
        let dir = TempDir::new("manifest-required");
        let source = dir.path.join("source");
        write(&source.join("lua/demo.lua"), b"return 1");
        let out = dir.path.join("demo.akpak");

        let err = build_mod(&source, &out, None).unwrap_err().to_string();
        assert!(err.contains("manifest.toml"), "{err}");

        write(
            &source.join("manifest.toml"),
            manifest::Manifest::base("demo").render().as_bytes(),
        );
        let err = build_mod(&source, &out, None).unwrap_err().to_string();
        assert!(err.contains("kind = \"mod\""), "{err}");

        write(&source.join("manifest.toml"), b"not = toml\n");
        let err = build_mod(&source, &out, None).unwrap_err().to_string();
        assert!(err.contains("invalid mod manifest"), "{err}");
    }

    #[test]
    fn duplicate_output_paths_are_rejected() {
        let dir = TempDir::new("duplicates");
        let source = dir.path.join("source");
        write(
            &source.join("manifest.toml"),
            mod_manifest_text("demo", "re1").as_bytes(),
        );
        write(&source.join("data.bin"), b"a");
        // A case-insensitive filesystem (Windows, some macOS mounts) resolves
        // both names to the same file, so the duplicate never reaches the
        // builder; the pack writer's own duplicate test covers that platform.
        if source.join("DATA.BIN").exists() {
            return;
        }
        write(&source.join("DATA.BIN"), b"b");
        let out = dir.path.join("demo.akpak");

        let err = build_mod(&source, &out, None).unwrap_err().to_string();
        assert!(err.contains("both produce pack entry"), "{err}");
    }

    #[test]
    fn base_checks_the_id_and_aggregates_new_files() {
        let dir = TempDir::new("base-check");
        let base_path = dir.path.join("re1.akpak");
        let mut base = PackWriter::new();
        base.add(
            manifest::ENTRY,
            manifest::Manifest::base("re1").render().into_bytes(),
        )
        .unwrap();
        base.add("x.txt", b"old".to_vec()).unwrap();
        base.add("lua/keep.lua", b"keep".to_vec()).unwrap();
        base.write(&base_path).unwrap();

        let source = dir.path.join("source");
        write(
            &source.join("manifest.toml"),
            mod_manifest_text_with_lua("demo", "re1", &["lua/demo.lua"]).as_bytes(),
        );
        write(&source.join("x.txt"), b"new");
        write(&source.join("new.bin"), b"new");
        write(&source.join("lua/demo.lua"), b"return 1");

        let out = dir.path.join("demo.akpak");
        let summary = build_mod(&source, &out, Some(&base_path)).unwrap();
        assert_eq!(summary.warnings.len(), 1, "{:?}", summary.warnings);
        assert!(
            summary.warnings[0].contains("2 override(s)"),
            "{:?}",
            summary.warnings
        );
        assert!(
            summary.warnings[0].contains("new.bin"),
            "{:?}",
            summary.warnings
        );
        assert!(
            summary.warnings[0].contains("lua/demo.lua"),
            "{:?}",
            summary.warnings
        );

        let wrong = dir.path.join("wrong");
        write(
            &wrong.join("manifest.toml"),
            mod_manifest_text("demo", "other").as_bytes(),
        );
        let err = build_mod(&wrong, &out, Some(&base_path))
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("\"other\"") && err.contains("\"re1\""),
            "{err}"
        );

        // A base without a manifest is checked against its stem.
        let bare_path = dir.path.join("bare.akpak");
        let mut bare = PackWriter::new();
        bare.add("x.txt", b"old".to_vec()).unwrap();
        bare.write(&bare_path).unwrap();
        let bare_source = dir.path.join("bare-source");
        write(
            &bare_source.join("manifest.toml"),
            mod_manifest_text("demo", "bare").as_bytes(),
        );
        write(&bare_source.join("x.txt"), b"new");
        let summary = build_mod(&bare_source, &out, Some(&bare_path)).unwrap();
        assert_eq!(summary.warnings.len(), 1, "{:?}", summary.warnings);
        assert!(
            summary.warnings[0].contains("no manifest.toml"),
            "{:?}",
            summary.warnings
        );
    }

    #[test]
    fn missing_declared_lua_hooks_warn_together() {
        let dir = TempDir::new("lua-missing");
        let source = dir.path.join("source");
        let manifest = mod_manifest_text_with_lua("demo", "re1", &["lua/a.lua", "lua/b.lua"]);
        write(&source.join("manifest.toml"), manifest.as_bytes());
        write(&source.join("lua/a.lua"), b"return 1");

        let out = dir.path.join("demo.akpak");
        let summary = build_mod(&source, &out, None).unwrap();
        assert_eq!(summary.warnings.len(), 1, "{:?}", summary.warnings);
        assert!(
            summary.warnings[0].contains("1 declared lua hook"),
            "{:?}",
            summary.warnings
        );
        assert!(
            summary.warnings[0].contains("lua/b.lua"),
            "{:?}",
            summary.warnings
        );
    }

    #[test]
    fn build_output_is_deterministic() {
        let dir = TempDir::new("deterministic");
        let source = dir.path.join("source");
        write(
            &source.join("manifest.toml"),
            mod_manifest_text("demo", "re1").as_bytes(),
        );
        write(&source.join("lua/demo.lua"), b"return 1");
        write(&source.join("data/a.bin"), b"aaaa");
        write(&source.join("data/b.bin"), b"bbbb");

        let first = dir.path.join("first.akpak");
        let second = dir.path.join("second.akpak");
        let first_summary = build_mod(&source, &first, None).unwrap();
        let second_summary = build_mod(&source, &second, None).unwrap();

        assert_eq!(first_summary, second_summary);
        assert_eq!(fs::read(&first).unwrap(), fs::read(&second).unwrap());
    }

    #[test]
    fn discover_mods_scans_the_sibling_directory() {
        let dir = TempDir::new("discover");
        let pack = dir.path.join("game.akpak");
        write(&pack, b"pack");
        let mods = dir.path.join("mods");
        write(&mods.join("b.AKPAK"), b"b");
        write(&mods.join("a.akpak"), b"a");
        write(&mods.join("notes.txt"), b"n");
        write(&mods.join("sub/c.akpak"), b"c");
        write(
            &mods.join("folder/manifest.toml"),
            mod_manifest_text("folder", "re1").as_bytes(),
        );

        let found = discover_mods(&pack, &[]);
        assert_eq!(
            found
                .iter()
                .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            ["a.akpak", "b.AKPAK", "folder"]
        );

        assert!(discover_mods(&dir.path.join("elsewhere/missing.akpak"), &[]).is_empty());
    }

    #[test]
    fn discover_mods_scans_the_global_directory_top_level_only() {
        let dir = TempDir::new("discover-global");
        let pack = dir.path.join("game.akpak");
        write(&pack, b"pack");
        let global = dir.path.join("config/mods");
        write(&global.join("two.akpak"), b"two");
        write(
            &global.join("one/manifest.toml"),
            mod_manifest_text("one", "re1").as_bytes(),
        );
        write(&global.join("nested/three.akpak"), b"three");
        write(&global.join("unrelated/data.bin"), b"data");

        let found = discover_mods(&pack, std::slice::from_ref(&global));
        assert_eq!(
            found
                .iter()
                .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            ["one", "two.akpak"],
            "only top-level mod files/directories are candidates"
        );
    }

    #[test]
    fn discover_mods_follows_symlinked_packs() {
        let dir = TempDir::new("discover-links");
        let pack = dir.path.join("game.akpak");
        write(&pack, b"pack");
        let mods = dir.path.join("mods");
        write(&mods.join("real.akpak"), b"mod");
        // The link target lives outside `mods/`; a link to a pack already in
        // the directory is deduplicated by canonical path.
        let target = dir.path.join("elsewhere.akpak");
        write(&target, b"mod");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, mods.join("link.akpak")).unwrap();
        #[cfg(not(unix))]
        write(&mods.join("link.akpak"), b"mod");

        let found = discover_mods(&pack, &[]);
        assert_eq!(
            found
                .iter()
                .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            ["link.akpak", "real.akpak"]
        );
    }

    #[test]
    fn a_case_variant_manifest_is_not_packed_twice() {
        let dir = TempDir::new("manifest-case");
        let source = dir.path.join("source");
        write(
            &source.join("manifest.toml"),
            mod_manifest_text("demo", "re1").as_bytes(),
        );
        // On a case-insensitive filesystem this overwrites the entry above; on
        // a case-sensitive one it is a second directory entry. Either way the
        // uppercase variant must never become a second `manifest.toml` entry.
        write(
            &source.join("MANIFEST.TOML"),
            mod_manifest_text("demo", "re1").as_bytes(),
        );
        let out = dir.path.join("demo.akpak");
        let summary = build_mod(&source, &out, None).unwrap();
        assert_eq!(
            summary
                .entries
                .iter()
                .map(|(path, _)| path.as_str())
                .collect::<Vec<_>>(),
            ["manifest.toml"]
        );
    }

    #[test]
    fn checked_in_cr_loadsave_builds_to_the_planned_entries() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mod_dir = root.join("mods/crloadsave");
        let dir = TempDir::new("crloadsave");
        let out = dir.path.join("crloadsave.akpak");

        let summary = build_mod(&mod_dir, &out, None).unwrap();
        assert_eq!(summary.warnings, Vec::<String>::new());
        assert_eq!(
            summary
                .entries
                .iter()
                .map(|(path, _)| path.as_str())
                .collect::<Vec<_>>(),
            [
                "language/en-US.txt",
                "language/ja-JP.txt",
                "manifest.toml",
                "mod.lua",
                "script/loadsave.lua",
            ]
        );

        let pack = Pack::open(&out).unwrap();
        let manifest = pack.manifest().unwrap();
        assert_eq!(manifest.id, "crloadsave");
        assert_eq!(manifest.kind, manifest::PackKind::Mod);
        assert_eq!(manifest.base.as_deref(), Some("re1"));
        assert_eq!(manifest.lua, ["mod.lua"]);

        // The registration names the screen entry, and the screen script
        // parses as Lua (the same check the pack verifier runs).
        assert!(
            std::str::from_utf8(&pack.read("mod.lua").unwrap())
                .unwrap()
                .contains("register_save_screen")
        );
        crate::lua::verify_source(&pack.read("script/loadsave.lua").unwrap()).unwrap();

        // The Japanese screen text must be spellable with the shipped 14-px
        // sheet; an unsupported character would render as a missing-glyph box.
        let japanese = pack.read("language/ja-JP.txt").unwrap();
        let japanese = std::str::from_utf8(&japanese).unwrap();
        let japanese = crate::language::Language::parse(japanese).unwrap();
        for (key, entry) in japanese.iter() {
            assert_eq!(
                entry.missing_glyphs(crate::charset::Script::Japanese),
                0,
                "{key} wants a glyph the Japanese font lacks"
            );
        }
    }

    #[test]
    fn checked_in_demo_builds_to_the_planned_entries() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let demo = root.join("mods/demo");
        let dir = TempDir::new("demo");
        let out = dir.path.join("demo.akpak");

        let summary = build_mod(&demo, &out, None).unwrap();
        assert_eq!(summary.warnings, Vec::<String>::new());
        assert_eq!(
            summary
                .entries
                .iter()
                .map(|(path, _)| path.as_str())
                .collect::<Vec<_>>(),
            [
                "lua/demo.lua",
                "manifest.toml",
                "room/1000/cut/00.bmp",
                "room/1000/room.scd",
            ]
        );

        let pack = Pack::open(&out).unwrap();
        let manifest = pack.manifest().unwrap();
        assert_eq!(manifest.id, "demo");
        assert_eq!(manifest.kind, manifest::PackKind::Mod);
        assert_eq!(manifest.base.as_deref(), Some("re1"));
        assert_eq!(manifest.load_order, 10);
        assert_eq!(manifest.lua, ["lua/demo.lua"]);

        let scripts = crate::scd::reader::parse(&pack.read("room/1000/room.scd").unwrap()).unwrap();
        assert_eq!(scripts.main.len(), 1);
        assert_eq!(scripts.events.len(), 1);
        assert_eq!(scripts.events[0].kind, crate::scd::ir::StreamKind::Event(0));

        let image = crate::bmp::decode(&pack.read("room/1000/cut/00.bmp").unwrap()).unwrap();
        assert_eq!((image.width, image.height), (320, 240));
        assert!(image.rgba.iter().any(|&byte| byte != 0));

        assert_eq!(
            pack.read("lua/demo.lua").unwrap(),
            fs::read(demo.join("lua/demo.lua")).unwrap()
        );
    }
}
