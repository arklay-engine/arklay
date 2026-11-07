//! The mod catalog: the hosted mods the launcher's Mods > Available tab
//! lists, downloads and installs.
//!
//! A catalog is an `index.json` (written by `scripts/publish-mods.py`) at a
//! URL or a local path. Every pack and thumbnail it lists is named relative
//! to the index, so a hosted tree can move as a whole:
//!
//! ```json
//! {
//!   "format": 1,
//!   "generated": "2026-11-07T15:00:00Z",
//!   "mods": [{
//!     "id": "re1-shdp", "name": "Seamless HD Project", "version": "1.1",
//!     "game": "re1", "kind": "mod", "category": "High Definition",
//!     "description": "...", "authors": ["MoArtis"], "credits": ["..."],
//!     "homepage": "https://...",
//!     "url": "re1-shdp/1.1/re1-shdp.akpak", "size": 529808808,
//!     "sha256": "...", "thumbnail": "re1-shdp/1.1/thumbnail.webp"
//!   }]
//! }
//! ```
//!
//! The catalog location is `ARKLAY_MOD_CATALOG`, else `[mods] catalog` in
//! `arklay.toml`, else [`DEFAULT_URL`]. A download is written beside its
//! target, checked against the listed size, SHA-256 and the pack's own
//! manifest, and only then renamed over the installed pack, so a failed or
//! cancelled download never leaves a broken mod behind.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::gui::canvas::Picture;

/// The hosted catalog, when no other is configured.
///
/// `None` until the public mods URL is settled; set `[mods] catalog` in
/// `arklay.toml` (or `ARKLAY_MOD_CATALOG`) to a catalog meanwhile.
pub const DEFAULT_URL: Option<&str> = None;

/// The environment variable that overrides the configured catalog.
pub const URL_ENV: &str = "ARKLAY_MOD_CATALOG";

/// The `index.json` format this build reads.
pub const FORMAT: u32 = 1;

/// The largest `index.json` accepted.
const MAX_INDEX_BYTES: u64 = 4 << 20;
/// The largest thumbnail accepted.
const MAX_THUMBNAIL_BYTES: u64 = 4 << 20;

/// A parsed catalog.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Catalog {
    pub format: u32,
    #[serde(default)]
    pub generated: String,
    pub mods: Vec<CatalogMod>,
}

/// One hosted mod. `url` and `thumbnail` are resolved against the index
/// once [`fetch`] returns.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CatalogMod {
    pub id: String,
    pub name: String,
    pub version: String,
    /// The base pack id it layers over (`re1`).
    pub game: String,
    /// `mod`, or `base` for a game pack.
    #[serde(default = "mod_kind")]
    pub kind: String,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub credits: Vec<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    pub url: String,
    pub size: u64,
    pub sha256: String,
    #[serde(default)]
    pub thumbnail: Option<String>,
}

fn mod_kind() -> String {
    "mod".to_string()
}

impl CatalogMod {
    /// The file name the mod installs as.
    pub fn file_name(&self) -> String {
        format!("{}.akpak", self.id)
    }

    /// The thumbnail's cache file: one per mod version, so a new version's
    /// picture replaces the old one.
    /// `None` without a thumbnail, or one in a format the launcher does not
    /// decode. The id and version are checked by [`parse`].
    fn thumbnail_cache(&self, dir: &Path) -> Option<PathBuf> {
        let extension = thumbnail_extension(self.thumbnail.as_deref()?)?;
        Some(dir.join(format!("{}-{}.{extension}", self.id, self.version)))
    }
}

/// Where the catalog is: the environment, else `configured`, else
/// [`DEFAULT_URL`].
pub fn location(configured: Option<&str>) -> Option<String> {
    std::env::var(URL_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| configured.map(str::to_string))
        .or_else(|| DEFAULT_URL.map(str::to_string))
}

/// The directory thumbnails are cached in.
pub fn cache_dir() -> PathBuf {
    crate::paths::primary_data_dir().join("cache").join("mods")
}

fn is_remote(location: &str) -> bool {
    let lower = location.to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://")
}

/// `relative` against the index at `base`: a URL is kept, anything else is
/// joined to the index's folder.
pub fn resolve(base: &str, relative: &str) -> String {
    if is_remote(relative) {
        return relative.to_string();
    }
    if is_remote(base) {
        let folder = base.rsplit_once('/').map_or(base, |(folder, _)| folder);
        return format!("{folder}/{}", relative.trim_start_matches('/'));
    }
    let folder = Path::new(base).parent().unwrap_or(Path::new(""));
    folder.join(relative).to_string_lossy().into_owned()
}

/// The picture formats a thumbnail may be in.
const THUMBNAIL_EXTENSIONS: [&str; 3] = ["webp", "png", "bmp"];

/// A thumbnail link's extension, lowercased, when it is one of
/// [`THUMBNAIL_EXTENSIONS`].
fn thumbnail_extension(location: &str) -> Option<&'static str> {
    let extension = extension(location)?;
    THUMBNAIL_EXTENSIONS
        .into_iter()
        .find(|known| known.eq_ignore_ascii_case(extension))
}

/// An HTTP agent with connect and read timeouts but no limit on the whole
/// transfer, so a large pack on a slow link still completes.
fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(30))
        .timeout_read(Duration::from_secs(60))
        .build()
}

fn extension(location: &str) -> Option<&str> {
    let name = location.rsplit(['/', '\\']).next()?;
    let name = name.split(['?', '#']).next()?;
    name.rsplit_once('.').map(|(_, extension)| extension)
}

/// A reader over `location` and its length, when known.
fn open(location: &str) -> Result<(Box<dyn Read + Send>, Option<u64>)> {
    if is_remote(location) {
        let response = agent()
            .get(location)
            .call()
            .with_context(|| format!("failed to download {location}"))?;
        let length = response
            .header("Content-Length")
            .and_then(|length| length.parse().ok());
        Ok((Box::new(response.into_reader()), length))
    } else {
        let file = File::open(location).with_context(|| format!("failed to open {location}"))?;
        let length = file.metadata().ok().map(|metadata| metadata.len());
        Ok((Box::new(file), length))
    }
}

/// All of `location`, refusing more than `limit` bytes.
fn read(location: &str, limit: u64) -> Result<Vec<u8>> {
    let (reader, _) = open(location)?;
    let mut data = Vec::new();
    reader
        .take(limit + 1)
        .read_to_end(&mut data)
        .with_context(|| format!("failed to read {location}"))?;
    if data.len() as u64 > limit {
        bail!("{location} is larger than {limit} bytes");
    }
    Ok(data)
}

/// Whether `id` is safe as a file name and a URL path segment.
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && !id.starts_with('.')
}

/// Parse an `index.json` read from `location`, resolving its links. Mods
/// that are malformed are left out, so one bad entry does not hide the
/// rest.
pub fn parse(location: &str, text: &str) -> Result<Catalog> {
    let mut catalog: Catalog =
        serde_json::from_str(text).context("the mod catalog is not valid JSON")?;
    if catalog.format != FORMAT {
        bail!(
            "the mod catalog is format {}; this build reads format {FORMAT} (update Arklay)",
            catalog.format
        );
    }
    catalog.mods.retain(|entry| {
        // The id and version name cached files, so they are kept to plain
        // file-name characters.
        let ok = valid_id(&entry.id)
            && valid_id(&entry.version)
            && entry.sha256.len() == 64
            && entry.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
            && !entry.url.is_empty();
        if !ok {
            crate::logging::write(&format!(
                "mod catalog: skipping malformed entry {:?}",
                entry.id
            ));
        }
        ok
    });
    for entry in &mut catalog.mods {
        entry.url = resolve(location, &entry.url);
        entry.thumbnail = entry
            .thumbnail
            .take()
            .map(|thumbnail| resolve(location, &thumbnail));
    }
    Ok(catalog)
}

/// Read and parse the catalog at `location`.
pub fn fetch(location: &str) -> Result<Catalog> {
    let data = read(location, MAX_INDEX_BYTES)?;
    let text = String::from_utf8(data).context("the mod catalog is not UTF-8")?;
    parse(location, &text)
}

/// `entry`'s thumbnail, from the cache in `dir` or fetched into it.
pub fn thumbnail(entry: &CatalogMod, dir: &Path) -> Result<Option<Picture>> {
    let (Some(url), Some(cache)) = (entry.thumbnail.as_deref(), entry.thumbnail_cache(dir)) else {
        return Ok(None);
    };
    let extension = thumbnail_extension(url).unwrap_or_default();
    let decode = |data: &[u8]| {
        crate::assets::decode_image_bytes(extension, data)
            .with_context(|| format!("thumbnail of {}", entry.id))
    };
    let cached = fs::read(&cache)
        .map_err(anyhow::Error::from)
        .and_then(|data| decode(&data));
    let image = match cached {
        Ok(image) => image,
        Err(_) => {
            let data = read(url, MAX_THUMBNAIL_BYTES)?;
            // Only a picture that decodes is cached.
            let image = decode(&data)?;
            if fs::create_dir_all(dir).is_ok() {
                // A cache that cannot be written only costs a refetch.
                let _ = crate::atomic::write(&cache, &data);
            }
            image
        }
    };
    Ok(Some(Picture {
        width: image.width,
        height: image.height,
        rgba: image.rgba,
    }))
}

/// Download `entry` to `dest`, reporting `(done, total)` bytes. The pack is
/// written to a `.part` file beside `dest` and checked (size, SHA-256, a
/// mod manifest with the listed id) before it replaces `dest`.
pub fn download(
    entry: &CatalogMod,
    dest: &Path,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, u64),
) -> Result<()> {
    let part = dest.with_extension("akpak.part");
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let result = (|| -> Result<()> {
        let (mut reader, length) = open(&entry.url)?;
        let total = length.unwrap_or(entry.size);
        let mut file =
            File::create(&part).with_context(|| format!("failed to create {}", part.display()))?;
        let mut hasher = Sha256::new();
        let mut done = 0u64;
        let mut buffer = vec![0u8; 256 * 1024];
        loop {
            if cancel.load(Ordering::Relaxed) {
                bail!("cancelled");
            }
            let read = reader
                .read(&mut buffer)
                .with_context(|| format!("failed to download {}", entry.url))?;
            if read == 0 {
                break;
            }
            done += read as u64;
            if done > entry.size {
                bail!("{} is larger than the catalog says", entry.url);
            }
            file.write_all(&buffer[..read])
                .with_context(|| format!("failed to write {}", part.display()))?;
            hasher.update(&buffer[..read]);
            progress(done, total);
        }
        file.sync_all()
            .with_context(|| format!("failed to write {}", part.display()))?;
        drop(file);
        if done != entry.size {
            bail!(
                "the download stopped at {done} of {} bytes; try again",
                entry.size
            );
        }
        let digest = format!("{:x}", hasher.finalize());
        if !digest.eq_ignore_ascii_case(&entry.sha256) {
            bail!("the download failed its checksum; try again");
        }
        let manifest = crate::pack::read_manifest(&part)
            .context("the download is not a valid pack")?
            .context("the download has no manifest")?;
        if manifest.id != entry.id || !manifest.is_mod() {
            bail!(
                "the download is pack {:?}, not the mod {:?} the catalog lists",
                manifest.id,
                entry.id
            );
        }
        fs::rename(&part, dest).with_context(|| format!("failed to install {}", dest.display()))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&part);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::PackWriter;

    fn entry_json(id: &str, url: &str, size: u64, sha256: &str) -> String {
        format!(
            r#"{{"id": "{id}", "name": "Name", "version": "1.0", "game": "re1",
                "category": "High Definition", "authors": ["A"],
                "url": "{url}", "size": {size}, "sha256": "{sha256}",
                "thumbnail": "{id}/1.0/thumbnail.webp"}}"#
        )
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("arklay-catalog-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn links_resolve_against_the_index() {
        assert_eq!(
            resolve("https://mods.example/v1/index.json", "a/1.0/a.akpak"),
            "https://mods.example/v1/a/1.0/a.akpak"
        );
        assert_eq!(
            resolve(
                "https://mods.example/index.json",
                "https://cdn.example/a.akpak"
            ),
            "https://cdn.example/a.akpak"
        );
        let local = resolve(r"G:\arklay\mods\index.json", "a/1.0/a.akpak");
        assert_eq!(
            Path::new(&local),
            Path::new(r"G:\arklay\mods").join("a/1.0/a.akpak")
        );
        assert_eq!(extension("https://x/y/thumbnail.webp?v=2"), Some("webp"));
        // Only picture formats name a thumbnail's cache file.
        assert_eq!(thumbnail_extension("a/1.0/thumbnail.WEBP"), Some("webp"));
        assert_eq!(thumbnail_extension("a/1.0/t.bat"), None);
        let entry = CatalogMod {
            thumbnail: Some("a/1.0/t.bat".into()),
            ..parse(
                "index.json",
                &format!(
                    r#"{{"format": 1, "mods": [{}]}}"#,
                    entry_json("a", "a.akpak", 1, &"ab".repeat(32))
                ),
            )
            .unwrap()
            .mods
            .remove(0)
        };
        assert_eq!(entry.thumbnail_cache(Path::new("cache")), None);
    }

    #[test]
    fn a_catalog_parses_and_drops_malformed_entries() {
        let sha = "ab".repeat(32);
        let text = format!(
            r#"{{"format": 1, "mods": [{}, {}, {}, {}]}}"#,
            entry_json("re1-shdp", "re1-shdp/1.0/re1-shdp.akpak", 10, &sha),
            entry_json("../evil", "x", 10, &sha),
            entry_json("short-sha", "x", 10, "abc"),
            // The version names the thumbnail's cache file.
            entry_json("evil-version", "x", 10, &sha)
                .replace(r#""version": "1.0""#, r#""version": "../../Startup/x""#),
        );
        let catalog = parse("https://mods.example/index.json", &text).unwrap();
        assert_eq!(catalog.mods.len(), 1);
        let entry = &catalog.mods[0];
        assert_eq!(entry.kind, "mod");
        assert_eq!(
            entry.url,
            "https://mods.example/re1-shdp/1.0/re1-shdp.akpak"
        );
        assert_eq!(
            entry.thumbnail.as_deref(),
            Some("https://mods.example/re1-shdp/1.0/thumbnail.webp")
        );
        let future = r#"{"format": 2, "mods": []}"#;
        let error = parse("index.json", future).unwrap_err().to_string();
        assert!(error.contains("format 2"), "{error}");
    }

    #[test]
    fn a_download_is_verified_before_it_replaces_the_installed_pack() {
        let dir = temp_dir("download");
        let hosted = dir.join("hosted.akpak");
        let mut writer = PackWriter::new();
        let manifest = "[pack]\nformat = 1\nid = \"hd\"\nkind = \"mod\"\nbase = \"re1\"\n";
        writer
            .add(crate::manifest::ENTRY, manifest.as_bytes().to_vec())
            .unwrap();
        writer
            .add("hires/bgd/0259741A.webp", vec![7; 1000])
            .unwrap();
        writer.write(&hosted).unwrap();
        let bytes = fs::read(&hosted).unwrap();
        let sha = format!("{:x}", Sha256::digest(&bytes));
        let index = dir.join("index.json");
        let text = format!(
            r#"{{"format": 1, "mods": [{}]}}"#,
            entry_json("hd", "hosted.akpak", bytes.len() as u64, &sha)
        );
        let entry = parse(&index.to_string_lossy(), &text)
            .unwrap()
            .mods
            .remove(0);

        let dest = dir.join("mods").join(entry.file_name());
        let cancel = AtomicBool::new(false);
        let mut last = (0, 0);
        download(&entry, &dest, &cancel, |done, total| last = (done, total)).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), bytes);
        assert_eq!(last, (bytes.len() as u64, bytes.len() as u64));

        // A checksum mismatch keeps what is installed and leaves no part file.
        let mut wrong = entry.clone();
        wrong.sha256 = "00".repeat(32);
        fs::write(&dest, b"installed").unwrap();
        let error = download(&wrong, &dest, &cancel, |_, _| {}).unwrap_err();
        assert!(format!("{error:#}").contains("checksum"), "{error:#}");
        assert_eq!(fs::read(&dest).unwrap(), b"installed");
        assert!(!dest.with_extension("akpak.part").exists());

        // So does a pack that is not the listed mod.
        let mut other = entry.clone();
        other.id = "other".into();
        let error = download(&other, &dest, &cancel, |_, _| {}).unwrap_err();
        assert!(format!("{error:#}").contains("not the mod"), "{error:#}");

        // And a cancel.
        cancel.store(true, Ordering::Relaxed);
        assert!(download(&entry, &dest, &cancel, |_, _| {}).is_err());
        assert_eq!(fs::read(&dest).unwrap(), b"installed");
        let _ = fs::remove_dir_all(&dir);
    }
}
