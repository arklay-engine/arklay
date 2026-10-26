//! HD texture replacement by content hash (`htexture/`).
//!
//! A mod pack may carry any number of images anywhere under `htexture/`,
//! each named by the [`crate::hash`] CRC of the texture it replaces
//! (`htexture/bgd/0259741A.webp`, `htexture/a/b/C5157461_3B.png`): the first
//! eight hex digits of the file stem are the hash and anything after an `_`
//! is ignored. The directories carry no meaning, so a TeamX-style
//! `hires/<class>/` tree can be dropped in as it is.
//!
//! The lookup is the last step of loading a texture. The usual routing has
//! already picked the file and decoded it; when that texture reaches the GPU
//! renderer its texels are hashed, and only a hash that names a file in the
//! table swaps in the HD image. A texture a mod has replaced by path
//! therefore keeps its own art, and an HD image can never land on the wrong
//! texels.
//!
//! Art that is composited per frame (the inventory icons) has no stable hash.
//! A pack may name such a replacement by its own stem under [`ROOT`] instead,
//! e.g. `htexture/item/0b.webp` for item `0x0B`'s icon; the caller supplies
//! the source extent the image covers ([`named_replacement`]). The named
//! route is preferred when it exists and the content hashes remain the
//! fallback, so a pack made before the name existed keeps working.
//!
//! The PC game uploaded each texture in its own conversion, so one texture can
//! carry any of a few hashes ([`image_candidates`], [`page_candidates`]); the
//! first one in the table wins. Each candidate also states its frame: the
//! extent, in source texels, that the HD image covers. A room mask hashes a
//! 256 x 256 window whatever its page size, so a 1024 x 1024 replacement of a
//! 128 x 128 page draws its top-left quarter.
//!
//! Only the GPU renderer draws replacements; the software rasterizer stays
//! the 320 x 240 reference. The 2D screens (files, item pictures, the status
//! screen, maps, the select and epilogue backgrounds) are drawn as the GPU
//! backend's 2D pass ([`crate::draw2d`]), where a replaced piece of their
//! art draws its HD image in the original's place, kept to the texels the
//! original draws.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex, Weak};

use anyhow::{Context, Result};

use crate::hash::{Expand, Surface, texel};
use crate::model::{PALETTE_ROW_LEN, Texture8};
use crate::pack::{AssetClass, Pack};
use crate::state::Image;

/// The pack directory replacements are found under, at any depth.
pub const ROOT: &str = "htexture/";

/// One hash a texture may be known by, and the source-texel extent an HD
/// replacement found by it covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub hash: u32,
    pub frame: [u32; 2],
}

/// A replacement found for a texture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    /// The pack entry holding the HD image.
    pub path: String,
    /// The source-texel extent the HD image covers.
    pub frame: [u32; 2],
}

/// A decoded replacement's file and frame.
type DecodedKey = (String, [u32; 2]);

/// The `htexture/` replacements of a pack and its mod layers, by hash.
#[derive(Debug, Default)]
pub struct HdTable {
    files: HashMap<u32, String>,
    /// Images with a meaningful stem rather than a hash: the named per-item
    /// keys. They are not hash candidates, but their presence still makes the
    /// pack an HD pack.
    named: usize,
    /// Room masks by the hash of their camera's background, from a TeamX
    /// loader's `cache/ROOM*.dat` lists ([`HdTable::paired_masks`]).
    paired: HashMap<u32, u32>,
    /// Textures already reported as having no replacement.
    reported: Mutex<HashSet<u32>>,
    /// Decoded 2D replacements by path and frame, held weakly: while any
    /// screen or the GPU's texture cache keeps one alive, every lookup gets
    /// that same [`Replacement`], so a reopened screen is neither decoded
    /// nor uploaded again.
    decoded: Mutex<HashMap<DecodedKey, Weak<Replacement>>>,
}

impl HdTable {
    /// Index every image under [`ROOT`] in the merged view of `pack`. A stem
    /// with a suffix (`C5157461_3B`) only fills a hash no bare stem names.
    pub fn scan(pack: &Pack) -> Self {
        Self::scan_with(pack, true)
    }

    /// [`HdTable::scan`], pairing masks by shape only when `shapes`.
    fn scan_with(pack: &Pack, shapes: bool) -> Self {
        let mut files = HashMap::new();
        let mut suffixed = HashMap::new();
        let mut lists = Vec::new();
        let mut named = 0usize;
        for path in pack.paths() {
            let lower = path.to_ascii_lowercase();
            if !lower.starts_with(ROOT) {
                continue;
            }
            if is_room_list(&lower) {
                lists.push(path.to_string());
                continue;
            }
            let Some((stem, extension)) = lower.rsplit('/').next().and_then(|n| n.rsplit_once('.'))
            else {
                continue;
            };
            if !matches!(extension, "webp" | "png" | "bmp") {
                continue;
            }
            let (digits, bare) = match stem.split_once('_') {
                Some((digits, _)) => (digits, false),
                None => (stem, true),
            };
            if digits.len() != 8 {
                named += 1;
                continue;
            }
            let Ok(hash) = u32::from_str_radix(digits, 16) else {
                named += 1;
                continue;
            };
            let slot = if bare { &mut files } else { &mut suffixed };
            slot.entry(hash).or_insert_with(|| path.to_string());
        }
        for (hash, path) in suffixed {
            files.entry(hash).or_insert(path);
        }
        let is_mask = |hash: &u32| files.get(hash).is_some_and(|path| in_mask_dir(path));
        let mut paired = HashMap::new();
        for path in &lists {
            let Ok(data) = pack.read(path) else {
                continue;
            };
            let hashes: Vec<u32> = data
                .as_chunks::<4>()
                .0
                .iter()
                .map(|&bytes| u32::from_le_bytes(bytes))
                .collect();
            for pair in hashes.windows(2) {
                if is_mask(&pair[0]) && !is_mask(&pair[1]) {
                    paired.entry(pair[1]).or_insert(pair[0]);
                }
            }
        }
        if shapes {
            match_mask_shapes(pack, &files, &mut paired);
        }
        HdTable {
            files,
            named,
            paired,
            reported: Mutex::new(HashSet::new()),
            decoded: Mutex::new(HashMap::new()),
        }
    }

    /// The number of HD names: content hashes and named entries.
    pub fn len(&self) -> usize {
        self.files.len() + self.named
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.named == 0
    }

    /// The replacement of the first candidate the table names.
    ///
    /// With a table of content hashes a texture none of whose candidates
    /// match is reported once on stderr, naming `what` and its hashes, so a
    /// pack's coverage gaps show up while playing. Named-only packs do not
    /// report hash misses.
    pub fn find(&self, candidates: &[Candidate], what: impl FnOnce() -> String) -> Option<Found> {
        let found = self.find_quietly(candidates);
        if found.is_none()
            && !self.files.is_empty()
            && let Some(first) = candidates.first()
            && self
                .reported
                .lock()
                .is_ok_and(|mut reported| reported.insert(first.hash))
        {
            let hashes: Vec<String> = candidates
                .iter()
                .map(|candidate| format!("{:08X}", candidate.hash))
                .collect();
            eprintln!(
                "htexture: no replacement for {}: {}",
                what(),
                hashes.join(" ")
            );
        }
        found
    }

    /// The room masks paired with a camera background known by any of
    /// `background`, as candidates covering the mask's 256 x 256 window.
    ///
    /// Many masks' hashes depend on what the loader's surface held before
    /// the mask was written, so their names can't be worked out from the
    /// mask alone. The loader lists each room's hashes in load order
    /// (`cache/ROOM0000.dat`, little-endian words), and a camera's mask is
    /// loaded just before its background: an entry under a `mask/` directory
    /// followed by one that isn't pairs the two.
    pub fn paired_masks(&self, background: &[Candidate]) -> Vec<Candidate> {
        background
            .iter()
            .filter_map(|candidate| self.paired.get(&candidate.hash))
            .map(|&hash| Candidate {
                hash,
                frame: [256, 256],
            })
            .collect()
    }

    /// [`HdTable::find`] without reporting a miss.
    pub fn find_quietly(&self, candidates: &[Candidate]) -> Option<Found> {
        candidates.iter().find_map(|candidate| {
            self.files.get(&candidate.hash).map(|path| Found {
                path: path.clone(),
                frame: candidate.frame,
            })
        })
    }
}

/// Whether `path` sits directly in a `mask/` directory, as a TeamX pack's
/// room masks do.
fn in_mask_dir(path: &str) -> bool {
    path.rsplit('/')
        .nth(1)
        .is_some_and(|dir| dir.eq_ignore_ascii_case("mask"))
}

/// The smallest IoU at which a replacement mask's shape names a camera
/// ([`match_mask_shapes`]).
const SHAPE_MIN_IOU: f32 = 0.9;
/// How far the best camera's IoU must lead the runner-up's.
const SHAPE_MIN_MARGIN: f32 = 0.05;

/// The coverage of a room mask in its 256 x 256 hash window: which texels
/// are drawn, within the extent the original page fills.
#[derive(Clone, Debug, PartialEq, Eq)]
struct MaskShape {
    extent: [usize; 2],
    covered: Vec<bool>,
}

impl MaskShape {
    const SIZE: usize = 256;

    /// The drawn (non-transparent) texels of an original mask page.
    fn of_page(page: &Image) -> Self {
        let extent = [
            (page.width as usize).min(Self::SIZE),
            (page.height as usize).min(Self::SIZE),
        ];
        let mut covered = vec![false; Self::SIZE * Self::SIZE];
        if page.rgba.len() < page.width as usize * page.height as usize * 4 {
            return MaskShape { extent, covered };
        }
        for y in 0..extent[1] {
            for x in 0..extent[0] {
                let alpha = page.rgba[(y * page.width as usize + x) * 4 + 3];
                covered[y * Self::SIZE + x] = alpha != 0;
            }
        }
        MaskShape { extent, covered }
    }

    /// The coverage of an HD mask spanning the whole window, brought down
    /// to one texel per original texel: a texel is drawn when most of the
    /// HD texels over it are at least half opaque.
    fn of_replacement(hd: &Image) -> Self {
        let (width, height) = (hd.width as usize, hd.height as usize);
        let mut covered = vec![false; Self::SIZE * Self::SIZE];
        if width >= Self::SIZE && height >= Self::SIZE && hd.rgba.len() >= width * height * 4 {
            for y in 0..Self::SIZE {
                let rows = y * height / Self::SIZE..(y + 1) * height / Self::SIZE;
                for x in 0..Self::SIZE {
                    let columns = x * width / Self::SIZE..(x + 1) * width / Self::SIZE;
                    let mut drawn = 0;
                    for row in rows.clone() {
                        drawn += columns
                            .clone()
                            .filter(|&column| hd.rgba[(row * width + column) * 4 + 3] >= 128)
                            .count();
                    }
                    covered[y * Self::SIZE + x] = drawn * 2 > rows.len() * columns.len();
                }
            }
        }
        MaskShape {
            extent: [Self::SIZE; 2],
            covered,
        }
    }

    /// Intersection over union of `self`'s drawn texels and `other`'s,
    /// within `self`'s extent. A replacement's texels past the original
    /// page held whatever the loader's surface did, so they don't count.
    fn iou(&self, other: &MaskShape) -> f32 {
        let (mut both, mut either) = (0u32, 0u32);
        for y in 0..self.extent[1] {
            let row = y * Self::SIZE..y * Self::SIZE + self.extent[0];
            for (&a, &b) in self.covered[row.clone()].iter().zip(&other.covered[row]) {
                both += u32::from(a && b);
                either += u32::from(a || b);
            }
        }
        if either == 0 {
            0.0
        } else {
            both as f32 / either as f32
        }
    }
}

/// The index of the clear winner among `scores`: at least
/// [`SHAPE_MIN_IOU`], and ahead of every other by [`SHAPE_MIN_MARGIN`].
fn clear_winner(scores: impl IntoIterator<Item = f32>) -> Option<(usize, f32)> {
    let mut best: Option<(usize, f32)> = None;
    let mut second = 0.0f32;
    for (index, score) in scores.into_iter().enumerate() {
        match best {
            Some((_, top)) if score <= top => second = second.max(score),
            _ => {
                if let Some((_, top)) = best {
                    second = second.max(top);
                }
                best = Some((index, score));
            }
        }
    }
    best.filter(|&(_, top)| top >= SHAPE_MIN_IOU && top - second >= SHAPE_MIN_MARGIN)
}

/// A mask page's width, height and the CRC of its texels.
type PageId = (u32, u32, u32);

/// A camera of the base game: the stems of its mask page and background.
struct RoomCamera {
    mask: String,
    background: String,
}

/// Every camera with a mask page in `pack` (`roommask/100_000`), with the
/// background stem it is drawn over (`roomcut/100_000`).
fn room_cameras(pack: &Pack) -> Vec<RoomCamera> {
    let mut stems: Vec<&str> = pack
        .paths()
        .filter_map(|path| path.strip_prefix("roommask/"))
        .map(|name| name.rsplit_once('.').map_or(name, |(stem, _)| stem))
        .filter(|name| !name.contains('/'))
        .collect();
    stems.sort_unstable();
    stems.dedup();
    stems
        .into_iter()
        .map(|name| RoomCamera {
            mask: format!("roommask/{name}"),
            background: format!("roomcut/{name}"),
        })
        .collect()
}

/// Pair the room masks no hash names with their cameras by shape.
///
/// A camera's mask is drawn from its own hashes or through a room list
/// ([`HdTable::paired_masks`]), but some cameras miss both: their room is
/// absent from the lists, or their background has no replacement to pair
/// with. Each replacement under a `mask/` directory that no camera already
/// uses is brought down to one texel per original texel and compared with
/// the mask page of every camera still missing one. A replacement whose
/// coverage overlaps one camera's page with an IoU of at least
/// [`SHAPE_MIN_IOU`], [`SHAPE_MIN_MARGIN`] ahead of any other camera, is
/// paired with that camera's background as a room list would pair it.
/// Cameras whose pages are identical are one candidate and share the match,
/// and one whose page is identical to a camera that has its mask takes
/// that camera's.
fn match_mask_shapes(pack: &Pack, files: &HashMap<u32, String>, paired: &mut HashMap<u32, u32>) {
    if !files.values().any(|path| in_mask_dir(path)) {
        return;
    }
    // Decoded as the runtime decodes them: a mask page keeps the shipped
    // page's size, a background is any size.
    let decode = |stem: &str, geometry, key| -> Option<Image> {
        let asset = pack
            .read_asset(stem, crate::pack::AssetClass::Image)
            .ok()??;
        crate::assets::decode_picture(&asset, geometry, key).ok()
    };
    // Each camera's page (identified by its size and texels' CRC) and
    // either the replacement its mask already has, or its shape and
    // background hash.
    let cameras = parallel_map(&room_cameras(pack), |camera| {
        let page = decode(
            &camera.mask,
            crate::assets::Geometry::Shipped,
            Some([0, 0, 0]),
        )?;
        let id = (page.width, page.height, crate::hash::crc32(&page.rgba));
        if let Some(own) = image_candidates(&page)
            .iter()
            .find(|candidate| files.contains_key(&candidate.hash))
        {
            return Some((id, Err(own.hash)));
        }
        let background = image_candidates(&decode(
            &camera.background,
            crate::assets::Geometry::Free,
            None,
        )?);
        match background
            .iter()
            .filter_map(|candidate| paired.get(&candidate.hash))
            .find(|hash| files.contains_key(hash))
        {
            Some(&hash) => Some((id, Err(hash))),
            None => Some((
                id,
                Ok((MaskShape::of_page(&page), background.first()?.hash)),
            )),
        }
    });
    let mut used = HashMap::new();
    // Cameras still missing a mask, grouped by identical page, with the
    // background hashes each group is known by.
    let mut missing: Vec<(PageId, MaskShape, Vec<u32>)> = Vec::new();
    for (id, camera) in cameras.into_iter().flatten() {
        match camera {
            Err(hash) => {
                used.entry(id).or_insert(hash);
            }
            Ok((shape, background)) => match missing.iter_mut().find(|(known, ..)| *known == id) {
                Some((.., backgrounds)) => backgrounds.push(background),
                None => missing.push((id, shape, vec![background])),
            },
        }
    }
    // A camera whose page is identical to a camera that has its mask takes
    // the same one.
    missing.retain(|(id, _, backgrounds)| {
        let Some(&hash) = used.get(id) else {
            return true;
        };
        for &background in backgrounds {
            paired.entry(background).or_insert(hash);
        }
        false
    });
    if missing.is_empty() {
        return;
    }
    let used: HashSet<u32> = used.into_values().collect();
    let mut unused: Vec<(u32, &String)> = files
        .iter()
        .filter(|&(hash, path)| in_mask_dir(path) && !used.contains(hash))
        .map(|(&hash, path)| (hash, path))
        .collect();
    unused.sort_unstable();
    let matches = parallel_map(&unused, |&(hash, path)| {
        let shape = MaskShape::of_replacement(&load(pack, path).ok()?);
        let (group, score) = clear_winner(missing.iter().map(|(_, page, _)| page.iou(&shape)))?;
        Some((group, score, hash))
    });
    // The best replacement for each group, by IoU.
    let mut best: BTreeMap<usize, (f32, u32)> = BTreeMap::new();
    for (group, score, hash) in matches.into_iter().flatten() {
        let entry = best.entry(group).or_insert((score, hash));
        if score > entry.0 {
            *entry = (score, hash);
        }
    }
    for (group, (_, hash)) in best {
        for &background in &missing[group].2 {
            paired.entry(background).or_insert(hash);
        }
    }
}

/// `f` of every item, spread over the available cores, in order.
fn parallel_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let threads = std::thread::available_parallelism().map_or(1, usize::from);
    let chunk = items.len().div_ceil(threads).max(1);
    std::thread::scope(|scope| {
        let workers: Vec<_> = items
            .chunks(chunk)
            .map(|chunk| scope.spawn(|| chunk.iter().map(&f).collect::<Vec<_>>()))
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().expect("a mask-matching worker panicked"))
            .collect()
    })
}

/// Whether `path` (lower case) is a loader's room list, `ROOM0000.dat`.
fn is_room_list(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.strip_prefix("room")
        .and_then(|rest| rest.strip_suffix(".dat"))
        .is_some_and(|id| id.len() == 4 && id.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// A decoded HD image and the source-texel extent it covers.
pub struct Replacement {
    pub image: Image,
    pub frame: [u32; 2],
}

impl std::fmt::Debug for Replacement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Replacement({}x{} over {}x{})",
            self.image.width, self.image.height, self.frame[0], self.frame[1]
        )
    }
}

/// The decoded replacement of the first of `candidates` `pack` names; a
/// miss is reported like [`HdTable::find`], a file that fails to decode on
/// stderr.
pub fn replacement(
    pack: &Pack,
    candidates: &[Candidate],
    what: impl FnOnce() -> String,
) -> Option<Arc<Replacement>> {
    let table = pack.htexture();
    let found = table.find(candidates, what)?;
    let frame = [found.frame[0].max(1), found.frame[1].max(1)];
    decoded_replacement(pack, table, found.path, frame)
}

/// The HD image a pack names under [`ROOT`] (`stem` is `item/0b`), decoded at
/// any size.
///
/// This is the per-item name route: the file has a stable path instead of a
/// content hash, so art that is composited per frame (the inventory icons) can
/// still be replaced. A missing name is not reported; the caller has a content
/// hash fallback.
pub fn named_replacement(pack: &Pack, stem: &str, frame: [u32; 2]) -> Option<Arc<Replacement>> {
    let path = format!("{ROOT}{stem}");
    let asset = pack
        .read_asset(path.trim_end_matches('/'), AssetClass::Image)
        .ok()
        .flatten()?;
    let frame = [frame[0].max(1), frame[1].max(1)];
    let table = pack.htexture();
    decoded_replacement(pack, table, asset.path().to_string(), frame)
}

/// Decode and cache `path`'s replacement covering `frame`.
fn decoded_replacement(
    pack: &Pack,
    table: &HdTable,
    path: String,
    frame: [u32; 2],
) -> Option<Arc<Replacement>> {
    let key = (path, frame);
    if let Some(kept) = table
        .decoded
        .lock()
        .ok()
        .and_then(|decoded| decoded.get(&key).and_then(Weak::upgrade))
    {
        return Some(kept);
    }
    match load(pack, &key.0) {
        Ok(image)
            if image.width > 0
                && image.height > 0
                && image.rgba.len() >= (image.width * image.height * 4) as usize =>
        {
            let replacement = Arc::new(Replacement { image, frame });
            if let Ok(mut decoded) = table.decoded.lock() {
                decoded.retain(|_, kept| kept.strong_count() > 0);
                decoded.insert(key, Arc::downgrade(&replacement));
            }
            Some(replacement)
        }
        Ok(_) => None,
        Err(err) => {
            eprintln!("htexture: {err:#}");
            None
        }
    }
}

/// The HD image a replaced 2D draw shows
/// ([`crate::render::Framebuffer::draw_replaced`]).
#[derive(Clone, Debug)]
pub enum QuadImage {
    /// A pack's HD image of 2D art.
    Art(Arc<Replacement>),
    /// A model the GPU renders at its own resolution first: the examine
    /// viewer's item ([`crate::render::Framebuffer::draw_ivm_captured`]).
    Model(Arc<Model>),
    /// A pack's HD image of an indexed page, recoloured per texel by the
    /// original's index (the map's room highlighting).
    Recoloured(Arc<Recoloured>),
}

/// An indexed page's texel indices, shared by its recoloured draws.
#[derive(Debug)]
pub struct Indices {
    pub width: u32,
    pub height: u32,
    /// Row-major, `width * height`.
    pub texels: Vec<u8>,
}

impl Indices {
    /// The index at original texel `(x, y)`, clamped to the page.
    pub fn at(&self, x: i32, y: i32) -> u8 {
        if self.width == 0 || self.height == 0 {
            return 0;
        }
        let x = x.clamp(0, self.width as i32 - 1) as usize;
        let y = y.clamp(0, self.height as i32 - 1) as usize;
        self.texels
            .get(y * self.width as usize + x)
            .copied()
            .unwrap_or(0)
    }
}

/// How an HD texel whose original index draws through a palette lookup
/// takes the lookup's colour: `hd * scale + luma(hd) * luma + flat`, each
/// channel clamped to `0..=1`.
///
/// A lookup that scales the palette colour (a tint, or the colour kept as it
/// is) scales the HD texel alike, keeping its own colours. One that swaps in
/// another colour (the map's red current room) keeps the HD texel's
/// brightness relative to the palette colour's, so its detail shows through
/// in the new colour. Either way an HD texel of the palette colour comes out
/// as the lookup's colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Recolour {
    pub scale: [f32; 3],
    pub luma: [f32; 3],
    pub flat: [f32; 3],
}

/// The Rec. 601 luma weights the recolouring measures brightness with.
pub const LUMA: [f32; 3] = [0.299, 0.587, 0.114];

/// The furthest a channel of a scaled palette colour may land from the
/// lookup's and still count as a tint: the lookups round each channel.
const TINT_TOLERANCE: f32 = 2.5 / 255.0;

/// The dimmest palette colour a swapped colour keeps an HD texel's
/// brightness against; a darker one would turn the slightest HD detail into a
/// large gain, so it draws the colour flat.
const MIN_LUMA: f32 = 2.0 / 255.0;

impl Recolour {
    /// The recolouring that draws palette colour `palette` as `colour`.
    pub fn of(palette: [u8; 3], colour: [u8; 3]) -> Self {
        let p = palette.map(|channel| f32::from(channel) / 255.0);
        let c = colour.map(|channel| f32::from(channel) / 255.0);
        let none = [0.0; 3];
        let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let pp = dot(p, p);
        if pp == 0.0 {
            // Nothing of a black texel's colour to keep.
            return Self {
                scale: none,
                luma: none,
                flat: c,
            };
        }
        let k = dot(c, p) / pp;
        if (0..3).all(|channel| (k * p[channel] - c[channel]).abs() <= TINT_TOLERANCE) {
            return Self {
                scale: [k; 3],
                luma: none,
                flat: none,
            };
        }
        let brightness = dot(p, LUMA);
        if brightness < MIN_LUMA {
            // Too dark to measure an HD texel's brightness against.
            return Self {
                scale: none,
                luma: none,
                flat: c,
            };
        }
        Self {
            scale: none,
            luma: c.map(|channel| channel / brightness),
            flat: none,
        }
    }

    /// The recolourings of a lookup drawing `palette` as `colours`, by
    /// index.
    pub fn lut(
        palette: &[[u8; 3]; 256],
        colours: &[Option<[u8; 3]>; 256],
    ) -> [Option<Recolour>; 256] {
        std::array::from_fn(|index| colours[index].map(|colour| Self::of(palette[index], colour)))
    }

    /// The one scale every entry of a lookup drawing `palette` as `colours`
    /// tints its colour by, black palette colours aside (they say nothing of
    /// the scale); `None` when an entry swaps its colour.
    pub fn common_tint(palette: &[[u8; 3]; 256], colours: &[Option<[u8; 3]>; 256]) -> Option<f32> {
        let unit = |rgb: [u8; 3]| rgb.map(|channel| f32::from(channel) / 255.0);
        let pairs: Vec<([f32; 3], [f32; 3])> = palette
            .iter()
            .zip(colours)
            .filter(|(p, _)| **p != [0; 3])
            .filter_map(|(p, c)| c.map(|c| (unit(*p), unit(c))))
            .collect();
        // The least-squares scale over every entry, so a dark channel's
        // rounding weighs no more than it moves the colour.
        let (cp, pp) = pairs.iter().fold((0.0, 0.0), |(cp, pp), (p, c)| {
            (
                cp + p[0] * c[0] + p[1] * c[1] + p[2] * c[2],
                pp + p[0] * p[0] + p[1] * p[1] + p[2] * p[2],
            )
        });
        if pp == 0.0 {
            return None;
        }
        let k = cp / pp;
        pairs
            .iter()
            .all(|(p, c)| {
                (0..3).all(|channel| (k * p[channel] - c[channel]).abs() <= TINT_TOLERANCE)
            })
            .then_some(k)
    }

    /// [`Recolour::lut`] for a draw whose HD art covers its whole rect: the
    /// clear entries and black palette colours take the lookup's
    /// [`Recolour::common_tint`]; `None` without one.
    pub fn whole(
        palette: &[[u8; 3]; 256],
        colours: &[Option<[u8; 3]>; 256],
    ) -> Option<[Option<Recolour>; 256]> {
        let k = Self::common_tint(palette, colours)?;
        let tint = Self {
            scale: [k; 3],
            luma: [0.0; 3],
            flat: [0.0; 3],
        };
        Some(std::array::from_fn(|index| match colours[index] {
            Some(colour) if palette[index] != [0; 3] => Some(Self::of(palette[index], colour)),
            _ => Some(tint),
        }))
    }

    /// The colour HD texel `hd` (`0..=1` RGB) takes.
    pub fn apply(&self, hd: [f32; 3]) -> [f32; 3] {
        let brightness = hd[0] * LUMA[0] + hd[1] * LUMA[1] + hd[2] * LUMA[2];
        std::array::from_fn(|channel| {
            (hd[channel] * self.scale[channel]
                + brightness * self.luma[channel]
                + self.flat[channel])
                .clamp(0.0, 1.0)
        })
    }
}

/// An HD image drawn in place of an indexed page through a palette lookup:
/// each pixel takes the recolouring of the original texel under it, and an
/// index without one stays clear.
#[derive(Debug)]
pub struct Recoloured {
    pub art: Arc<Replacement>,
    /// The original page's indices: frame texel `(x, y)` is page texel
    /// `(x, y)`.
    pub indices: Arc<Indices>,
    pub lut: [Option<Recolour>; 256],
}

impl From<Arc<Replacement>> for QuadImage {
    fn from(image: Arc<Replacement>) -> Self {
        Self::Art(image)
    }
}

/// A model draw the GPU renders into its own target, cleared to
/// transparent, and then draws over the frame as its item
/// ([`crate::draw2d::Kind::Model`], whose `uv` spans the target).
#[derive(Debug)]
pub struct Model {
    /// The model whose page the triangles sample.
    pub ivm: Arc<crate::ivm::Ivm>,
    /// Three per triangle, back to front, in the item's clip space.
    pub vertices: Vec<ModelVertex>,
    /// A named or content-hash HD replacement of the model's page, covering
    /// the whole page. The caller resolves it before queuing the model; when
    /// absent the page draws from its own palette.
    pub page: Option<Arc<Replacement>>,
}

/// One projected model corner (40 bytes), the input of `ivm.vert`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelVertex {
    /// Clip-space position over the quad: `x`, `y` and the linear depth,
    /// all times `w`, the view Z.
    pub clip: [f32; 4],
    /// Normalised texture coordinates in the model's page.
    pub uv: [f32; 2],
    /// The lit colour in `0..=1`; `1` keeps the texel as stored.
    pub shade: [f32; 3],
    /// `palette_row << 8 | blend << 24 | untextured << 25`, as `mesh.frag`
    /// reads it.
    pub packed: u32,
}

/// Decode the HD image at `path`.
pub fn load(pack: &Pack, path: &str) -> Result<Image> {
    let data = pack
        .read(path)
        .with_context(|| format!("failed to read {path}"))?;
    let extension = path.rsplit_once('.').map_or("", |(_, ext)| ext);
    crate::assets::decode_image_bytes(&extension.to_ascii_lowercase(), &data)
        .with_context(|| format!("failed to decode {path}"))
}

/// A 15-bit colour back from the decoders' 8-bit channels. Every decoder
/// widens a 5-bit channel to `v * 255 / 31`, `v << 3` or `v << 3 | v >> 2`,
/// all of which keep `v` in the top five bits.
fn colour_of(rgb: [u8; 3]) -> u16 {
    u16::from(rgb[0] >> 3) | u16::from(rgb[1] >> 3) << 5 | u16::from(rgb[2] >> 3) << 10
}

/// The misc/model finish: black (`1`) and transparent (`0`) keep a clear
/// alpha, every other texel is opaque.
fn finish_model(texel: u32) -> u32 {
    if texel <= 1 {
        texel
    } else {
        texel | 0xff00_0000
    }
}

/// The mask finish: everything but a transparent texel is opaque.
fn finish_mask(texel: u32) -> u32 {
    if texel == 0 { 0 } else { texel | 0xff00_0000 }
}

/// Japanese pages Classic REbirth swaps for their English versions before the
/// game uploads them (`DATA/STATUS.TIM`, `DATA/TITLE.PIX`, `DATA/PBFONT0.TIM`),
/// as the hash of the Japanese page and of the English one. A pack made under
/// CR names the English page, so a Japanese install also looks that hash up.
const TRANSLATED: [(u32, u32); 4] = [
    // The status screen's menu page, CLUT row 0.
    (0x440c_318f, 0x97de_3e43),
    // The title screen.
    (0xe0c7_ae8b, 0x3c52_c827),
    // The font's glyph page: `FONT.TIM`'s left page ([`font_candidates`])
    // and CR's Western `PBFONT0.TIM`, whose cells hold the same glyphs but
    // for the hiragana block (see `crate::font`).
    (0xb2dc_eba5, 0xcff9_d54d),
    // The same page from the copy of `PBFONT0.TIM` every pack carries as its
    // Western sheet (`crate::font::WESTERN_SHEET`), which doesn't reproduce
    // CR's hash of it either.
    (0xc428_814d, 0xcff9_d54d),
];

/// Append the English page's hash, with the same frame, after each candidate
/// that names a Japanese page in [`TRANSLATED`].
fn add_translations(candidates: &mut Vec<Candidate>) {
    let english: Vec<Candidate> = candidates
        .iter()
        .filter_map(|candidate| {
            TRANSLATED
                .iter()
                .find(|&&(japanese, _)| japanese == candidate.hash)
                .map(|&(_, hash)| Candidate {
                    hash,
                    frame: candidate.frame,
                })
        })
        .collect();
    candidates.extend(english);
}

/// The hashes an RGBA image (background, mask page, effect sheet) may carry,
/// most specific first.
pub fn image_candidates(image: &Image) -> Vec<Candidate> {
    let (width, height) = (image.width as usize, image.height as usize);
    let pixels = image.rgba.as_chunks::<4>().0;
    if width == 0 || height == 0 || pixels.len() < width * height {
        return Vec::new();
    }
    let pixels = &pixels[..width * height];
    let surface = |expand: Expand, black_clear: bool| Surface {
        width,
        height,
        texels: pixels
            .iter()
            .map(|&[r, g, b, a]| {
                let colour = colour_of([r, g, b]);
                if a == 0 || (black_clear && colour == 0) {
                    0
                } else {
                    texel(colour, expand)
                }
            })
            .collect(),
    };
    let size = [image.width, image.height];
    let mut candidates = Vec::with_capacity(7);
    let replicated = surface(Expand::Replicate, false);
    // A camera background hashes its left 256 x 240 but is drawn whole; a
    // `DATA/*.PIX` screen the same way, but widened like a menu texture.
    if width >= 256 {
        candidates.push(Candidate {
            hash: replicated.window_crc(256, 240, |t| t | 0xff00_0000),
            frame: size,
        });
        candidates.push(Candidate {
            hash: surface(Expand::Shift, false).window_crc(256, 240, |t| t | 0xff00_0000),
            frame: size,
        });
    }
    candidates.push(Candidate {
        hash: replicated.window_crc(256, 256, finish_mask),
        frame: [256, 256],
    });
    for black_clear in [false, true] {
        let shifted = surface(Expand::Shift, black_clear);
        candidates.push(Candidate {
            hash: shifted.window_crc(256, 256, finish_model),
            frame: [256, 256],
        });
        if (width, height) != (256, 256) {
            candidates.push(Candidate {
                hash: shifted.window_crc(width, height, finish_model),
                frame: size,
            });
        }
        // An item icon (one 40 x 30 cell of `ITEM_ALL.PIX`) is uploaded alone
        // in the corner of a square power-of-two texture, 64 x 64.
        let square = width.max(height).next_power_of_two();
        if square < 256 && (width, height) != (square, square) {
            candidates.push(Candidate {
                hash: shifted.window_crc(square, square, finish_model),
                frame: [square as u32; 2],
            });
        }
    }
    add_translations(&mut candidates);
    candidates
}

/// Which colours of an indexed page the PC upload made transparent.
#[derive(Clone, Copy)]
enum Key {
    /// None: every texel is drawn.
    None,
    /// Colour `0x0000`, the PlayStation's transparent texel.
    Zero,
    /// Any black, with or without the STP bit.
    Black,
}

/// The hashes CLUT row `row` of an indexed model page may carry, most common
/// first.
///
/// The PC game uploads a model page 256 texels wide: a 128-wide page is
/// stretched to twice its width and hashed in a 256 x 256 window, so its
/// replacement covers 128 x 256 source texels. A menu page (the status
/// screen, the files, the item box) is uploaded unstretched with its black
/// clear, in a 256 x 256 window or, below that, at its own size.
pub fn page_candidates(texture: &Texture8, row: usize) -> Vec<Candidate> {
    let (width, height) = (texture.width as usize, texture.height as usize);
    if width == 0 || height == 0 {
        return Vec::new();
    }
    let colours: Vec<u16> = (0..PALETTE_ROW_LEN)
        .map(|index| {
            let [r, g, b, _] = texture.palette(row, index as u8);
            let stp = texture.palette_stp(row, index as u8);
            colour_of([r, g, b]) | if stp { 0x8000 } else { 0 }
        })
        .collect();
    let index_at = |x: usize, y: usize| texture.indices.get(y * width + x).copied().unwrap_or(0);
    let surface = |key: Key, stretch: usize| {
        let mut texels = Vec::with_capacity(width * stretch * height);
        for y in 0..height {
            for x in 0..width * stretch {
                let colour = colours[usize::from(index_at(x / stretch, y))];
                let clear = match key {
                    Key::None => false,
                    Key::Zero => colour == 0,
                    Key::Black => colour & 0x7fff == 0,
                };
                texels.push(if clear {
                    0
                } else {
                    texel(colour, Expand::Shift)
                });
            }
        }
        Surface {
            width: width * stretch,
            height,
            texels,
        }
    };
    let stretch = if width < 256 && 256 % width == 0 {
        256 / width
    } else {
        1
    };
    let window = [(256 / stretch) as u32, 256];
    let mut candidates = Vec::with_capacity(6);
    for key in [Key::Zero, Key::Black, Key::None] {
        candidates.push(Candidate {
            hash: surface(key, stretch).window_crc(256, 256, finish_model),
            frame: window,
        });
    }
    // The original whole-texture hash some item and enemy pages carry.
    candidates.push(Candidate {
        hash: surface(Key::None, 1).window_crc(width, height, finish_model),
        frame: [texture.width, texture.height],
    });
    if stretch > 1 {
        candidates.push(Candidate {
            hash: surface(Key::Black, 1).window_crc(256, 256, finish_model),
            frame: [256, 256],
        });
    }
    if width < 256 && height < 256 {
        candidates.push(Candidate {
            hash: surface(Key::Black, 1).window_crc(width, height, finish_model),
            frame: [texture.width, texture.height],
        });
    }
    add_translations(&mut candidates);
    candidates
}

/// The hashes the font sheet's glyph page may carry: the sheet's left
/// 256 x 256 page through CLUT row 0, the page the glyphs Classic REbirth
/// replaces live on.
pub fn font_candidates(texture: &Texture8) -> Vec<Candidate> {
    let width = texture.width.min(256);
    let height = texture.height.min(256);
    let stride = texture.width as usize;
    if texture.indices.len() < stride * height as usize {
        return Vec::new();
    }
    let indices = (0..height as usize)
        .flat_map(|y| {
            let start = y * stride;
            texture.indices[start..start + width as usize]
                .iter()
                .copied()
        })
        .collect();
    let page = Texture8 {
        width,
        height,
        indices,
        palettes: texture.palettes.clone(),
        stp: texture.stp.clone(),
    };
    page_candidates(&page, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::{Class, crc32};

    fn pack_of(paths: &[&str]) -> Pack {
        let mut writer = crate::pack::PackWriter::new();
        for path in paths {
            writer.add(path, Vec::new()).unwrap();
        }
        Pack::from_bytes(writer.to_bytes().unwrap()).unwrap()
    }

    #[test]
    fn the_table_indexes_every_hex_stem_under_htexture() {
        let pack = pack_of(&[
            "htexture/bgd/0259741A.webp",
            "htexture/deep/er/C5157461_3B.png",
            "htexture/deep/C5157461.png",
            "htexture/misc/ABCDEF01_8E.webp",
            "htexture/notes.txt",
            "htexture/misc/XYZ.webp",
            "roomcut/12345678.webp",
        ]);
        let table = HdTable::scan(&pack);
        // Three content hashes plus the named `misc/XYZ` image.
        assert_eq!(table.len(), 4);
        let find = |hash| {
            table
                .find(
                    &[Candidate {
                        hash,
                        frame: [1, 1],
                    }],
                    String::new,
                )
                .map(|found| found.path)
        };
        assert_eq!(
            find(0x0259_741a).as_deref(),
            Some("htexture/bgd/0259741A.webp")
        );
        // The bare stem wins over a suffixed copy of the same hash.
        assert_eq!(
            find(0xc515_7461).as_deref(),
            Some("htexture/deep/C5157461.png")
        );
        assert_eq!(
            find(0xabcd_ef01).as_deref(),
            Some("htexture/misc/ABCDEF01_8E.webp")
        );
        assert_eq!(find(0x1234_5678), None);
    }

    #[test]
    fn a_room_list_pairs_each_mask_with_the_background_after_it() {
        let words = |hashes: &[u32]| -> Vec<u8> {
            hashes.iter().flat_map(|hash| hash.to_le_bytes()).collect()
        };
        let mut writer = crate::pack::PackWriter::new();
        for path in [
            "htexture/hires/mask/BE528391.webp",
            "htexture/hires/mask/C5157461.webp",
            "htexture/hires/bgd/F9E129D5.webp",
        ] {
            writer.add(path, Vec::new()).unwrap();
        }
        // A mask, its background (absent from the pack), a mask and its
        // background, then two backgrounds and a trailing mask.
        let list = words(&[
            0xbe52_8391,
            0xdd17_f52f,
            0xc515_7461,
            0xf9e1_29d5,
            0x3c52_c827,
            0xd1d1_44a8,
            0xc515_7461,
        ]);
        writer
            .add("htexture/hires/cache/ROOM0000.dat", list)
            .unwrap();
        // Not a room list: ignored.
        writer
            .add(
                "htexture/hires/cache/notes.dat",
                words(&[0xbe52_8391, 0x1234_5678]),
            )
            .unwrap();
        let pack = Pack::from_bytes(writer.to_bytes().unwrap()).unwrap();
        let table = HdTable::scan(&pack);
        assert_eq!(table.len(), 3);
        let paired = |hash| {
            table
                .paired_masks(&[Candidate {
                    hash,
                    frame: [320, 240],
                }])
                .iter()
                .map(|candidate| (candidate.hash, candidate.frame))
                .collect::<Vec<_>>()
        };
        assert_eq!(paired(0xdd17_f52f), [(0xbe52_8391, [256, 256])]);
        assert_eq!(paired(0xf9e1_29d5), [(0xc515_7461, [256, 256])]);
        assert!(paired(0x3c52_c827).is_empty());
        assert!(paired(0xd1d1_44a8).is_empty());
        assert!(paired(0x1234_5678).is_empty());
    }

    #[test]
    fn the_first_named_candidate_wins_with_its_frame() {
        let pack = pack_of(&["htexture/00000002.webp", "htexture/00000003.webp"]);
        let table = HdTable::scan(&pack);
        let candidates = [
            Candidate {
                hash: 1,
                frame: [1, 1],
            },
            Candidate {
                hash: 3,
                frame: [3, 3],
            },
            Candidate {
                hash: 2,
                frame: [2, 2],
            },
        ];
        let found = table.find(&candidates, String::new).unwrap();
        assert_eq!(found.path, "htexture/00000003.webp");
        assert_eq!(found.frame, [3, 3]);
    }

    #[test]
    fn a_named_only_pack_is_still_an_hd_pack() {
        let pack = pack_of(&["htexture/item/0b.webp", "htexture/notes.txt"]);
        let table = HdTable::scan(&pack);
        assert_eq!(table.len(), 1, "the named icon only");
        assert!(!table.is_empty(), "the named item icon makes it an HD pack");
        // A hash-shaped stem that is not hex is a name, not a hash.
        let pack = pack_of(&["htexture/item/ZZZZZZZZ.webp"]);
        assert!(!HdTable::scan(&pack).is_empty());
    }

    #[test]
    fn a_replacement_is_shared_while_any_holder_keeps_it() {
        let bmp = crate::bmp::encode_to_vec(&Image {
            width: 2,
            height: 2,
            rgba: vec![255; 16],
        })
        .unwrap();
        let mut writer = crate::pack::PackWriter::new();
        writer.add("htexture/misc/00000007.bmp", bmp).unwrap();
        let pack = Pack::from_bytes(writer.to_bytes().unwrap()).unwrap();
        let candidates = [Candidate {
            hash: 7,
            frame: [4, 4],
        }];
        let first = replacement(&pack, &candidates, String::new).unwrap();
        let again = replacement(&pack, &candidates, String::new).unwrap();
        assert!(Arc::ptr_eq(&first, &again));
        drop((first, again));
        // Once dropped, the next lookup decodes afresh.
        let decoded = replacement(&pack, &candidates, String::new).unwrap();
        assert_eq!(decoded.frame, [4, 4]);
        assert_eq!(decoded.image.width, 2);
    }

    #[test]
    fn named_replacements_resolve_by_stem_and_frame() {
        let image = |width: u32, height: u32| {
            crate::bmp::encode_to_vec(&Image {
                width,
                height,
                rgba: [0, 0, 255, 255].repeat((width * height) as usize),
            })
            .unwrap()
        };
        let mut writer = crate::pack::PackWriter::new();
        writer.add("htexture/item/0b.bmp", image(80, 60)).unwrap();
        writer
            .add("htexture/data/item_all/01.bmp", image(160, 120))
            .unwrap();
        // A hash-named file still fills the hash table; a caller that names
        // its full stem can also find it by name.
        writer
            .add("htexture/hash/00000001.bmp", image(4, 4))
            .unwrap();
        let pack = Pack::from_bytes(writer.to_bytes().unwrap()).unwrap();

        let icon = named_replacement(&pack, "item/0b", [40, 30]).unwrap();
        assert_eq!((icon.image.width, icon.image.height), (80, 60));
        assert_eq!(icon.frame, [40, 30]);
        let legacy = named_replacement(&pack, "data/item_all/01", [40, 30]).unwrap();
        assert_eq!((legacy.image.width, legacy.image.height), (160, 120));
        assert!(named_replacement(&pack, "item/0c", [40, 30]).is_none());

        // The decoded replacement is cached across lookups.
        let again = named_replacement(&pack, "item/0b", [40, 30]).unwrap();
        assert!(Arc::ptr_eq(&icon, &again));
    }

    #[test]
    fn a_recolouring_draws_its_palette_colour_as_the_lookups() {
        let close = |got: [f32; 3], want: [u8; 3]| {
            got.iter()
                .zip(want)
                .all(|(&g, w)| (g * 255.0 - f32::from(w)).abs() <= 2.5)
        };
        let unit = |rgb: [u8; 3]| rgb.map(|channel| f32::from(channel) / 255.0);
        // The map's half-bright plan, its red current room, a black texel
        // given a colour, and a colour kept as it is.
        let cases = [
            ([0x20, 0x68, 0x00], [0x10, 0x34, 0x00]),
            ([0x20, 0x68, 0x00], [0x7C, 0x00, 0x00]),
            ([0, 0, 0], [0x10, 0x20, 0x30]),
            ([0xB0, 0xB0, 0xB0], [0xB0, 0xB0, 0xB0]),
        ];
        for (palette, colour) in cases {
            let recolour = Recolour::of(palette, colour);
            assert!(
                close(recolour.apply(unit(palette)), colour),
                "{palette:?} -> {colour:?}: {recolour:?}"
            );
        }
        // A tint scales an HD texel's own colours...
        let tint = Recolour::of([0x20, 0x68, 0x00], [0x10, 0x34, 0x00]);
        assert_eq!(tint.luma, [0.0; 3]);
        assert!(close(
            tint.apply(unit([0x80, 0x40, 0xFF])),
            [0x40, 0x20, 0x80]
        ));
        // ...a swapped colour keeps its brightness: a darker HD outline
        // inside the room draws a darker red.
        let swap = Recolour::of([0x20, 0x68, 0x00], [0x7C, 0x00, 0x00]);
        assert_eq!(swap.scale, [0.0; 3]);
        let outline = swap.apply(unit([0x10, 0x34, 0x00]));
        assert!(close(outline, [0x3E, 0x00, 0x00]), "{outline:?}");
        // ...and a black texel has no brightness to keep.
        let flat = Recolour::of([0, 0, 0], [0x10, 0x20, 0x30]);
        assert!(close(flat.apply([1.0; 3]), [0x10, 0x20, 0x30]));
    }

    #[test]
    fn a_whole_lookup_draws_its_clear_texels_in_its_common_tint() {
        let mut palette = [[0u8; 3]; 256];
        let mut colours = [None; 256];
        // Half of a green, of a grey and of a dark channel that rounds
        // unevenly, with a black palette colour kept black.
        for (index, p) in [
            (1, [0x20, 0x68, 0x00]),
            (2, [0xB0, 0xB0, 0xB0]),
            (3, [9, 0x40, 0x80]),
        ] {
            palette[index] = p;
            colours[index] = Some(p.map(|channel| (u16::from(channel) * 0x80 / 0xFF) as u8));
        }
        colours[4] = Some([0, 0, 0]);
        let k = Recolour::common_tint(&palette, &colours).unwrap();
        assert!((k - 0.5).abs() < 0.02, "{k}");
        let whole = Recolour::whole(&palette, &colours).unwrap();
        assert_eq!(whole[0].unwrap().scale, [k; 3]);
        assert_eq!(whole[4].unwrap().scale, [k; 3]);
        assert_eq!(whole[1], Recolour::lut(&palette, &colours)[1]);
        // A colour drawn black is hidden, not a tint to extend.
        palette[5] = [0x80; 3];
        colours[5] = Some([0, 0, 0]);
        assert!(Recolour::whole(&palette, &colours).is_none());
        // So is a swapped colour.
        colours[5] = Some([0x7C, 0, 0]);
        assert!(Recolour::common_tint(&palette, &colours).is_none());
        assert!(Recolour::common_tint(&[[0; 3]; 256], &[None; 256]).is_none());
    }

    #[test]
    fn a_colour_swapped_for_a_near_black_one_draws_flat() {
        let swap = Recolour::of([0, 0, 8], [0x7C, 0, 0]);
        assert_eq!(swap.flat, [0x7C as f32 / 255.0, 0.0, 0.0]);
        assert_eq!(swap.apply([1.0; 3]), swap.flat);
    }

    #[test]
    fn a_background_image_carries_its_retail_hash() {
        // A 320x240 picture decoded the way the converter does
        // (`v * 255 / 31`) hashes like the 15-bit original.
        let colours: Vec<u16> = (0..320 * 240).map(|i| (i % 0x7fff) as u16).collect();
        let widen = |v: u16| ((v & 31) * 255 / 31) as u8;
        let image = Image {
            width: 320,
            height: 240,
            rgba: colours
                .iter()
                .flat_map(|&c| [widen(c), widen(c >> 5), widen(c >> 10), 255])
                .collect(),
        };
        let retail = Surface::from_bgr555(320, 240, &colours, Expand::Replicate).hash(Class::Bgd);
        let candidates = image_candidates(&image);
        assert_eq!(
            candidates[0],
            Candidate {
                hash: retail,
                frame: [320, 240]
            }
        );
    }

    #[test]
    fn an_item_icon_hashes_in_the_corner_of_a_64_square() {
        // A 40x30 icon: black (keyed clear) on the left half, pure red on
        // the right.
        let image = Image {
            width: 40,
            height: 30,
            rgba: (0..40 * 30)
                .flat_map(|i| {
                    if i % 40 < 20 {
                        [0, 0, 0, 0]
                    } else {
                        [255, 0, 0, 255]
                    }
                })
                .collect(),
        };
        let mut expected = Vec::new();
        for y in 0..64 {
            for x in 0..64 {
                let texel: u32 = if y < 30 && (20..40).contains(&x) {
                    0xfff8_0000
                } else {
                    0
                };
                expected.extend_from_slice(&texel.to_le_bytes());
            }
        }
        assert!(image_candidates(&image).contains(&Candidate {
            hash: crc32(&expected),
            frame: [64, 64],
        }));
    }

    #[test]
    fn a_128_wide_model_page_hashes_stretched_into_a_256_window() {
        // Index 0 is colour 0x0000 (transparent), index 1 is pure red.
        let mut palettes = vec![[0, 0, 0, 255]; PALETTE_ROW_LEN];
        palettes[1] = [255, 0, 0, 255];
        let texture = Texture8 {
            width: 128,
            height: 1,
            indices: (0..128).map(|x| (x % 2) as u8).collect(),
            palettes,
            stp: vec![false; PALETTE_ROW_LEN],
        };
        let mut expected = Vec::new();
        for x in 0..256 {
            let texel: u32 = if (x / 2) % 2 == 1 { 0xfff8_0000 } else { 0 };
            expected.extend_from_slice(&texel.to_le_bytes());
        }
        expected.resize(256 * 256 * 4, 0);
        let candidates = page_candidates(&texture, 0);
        assert_eq!(
            candidates[0],
            Candidate {
                hash: crc32(&expected),
                frame: [128, 256]
            }
        );
    }

    #[test]
    fn a_font_sheet_hashes_its_left_page() {
        let mut palettes = vec![[0u8; 4]; PALETTE_ROW_LEN];
        palettes[1] = [255; 4];
        let sheet = Texture8 {
            width: 768,
            height: 256,
            indices: (0..768 * 256).map(|i| ((i % 768) / 3 % 2) as u8).collect(),
            palettes: palettes.clone(),
            stp: vec![false; PALETTE_ROW_LEN],
        };
        let page = Texture8 {
            width: 256,
            height: 256,
            indices: (0..256 * 256).map(|i| ((i % 256) / 3 % 2) as u8).collect(),
            palettes,
            stp: vec![false; PALETTE_ROW_LEN],
        };
        assert_eq!(font_candidates(&sheet), page_candidates(&page, 0));
        let short = Texture8 {
            indices: vec![0; 768],
            ..sheet
        };
        assert!(font_candidates(&short).is_empty());
    }

    #[test]
    fn the_shipped_western_sheet_names_classic_rebirths_hd_font_page() {
        let sheet = crate::tim::decode_4bpp(crate::font::WESTERN_SHEET).unwrap();
        let hashes: Vec<u32> = font_candidates(&sheet).iter().map(|c| c.hash).collect();
        assert!(hashes.contains(&0xc428_814d), "{hashes:08x?}");
        assert!(hashes.contains(&0xcff9_d54d), "{hashes:08x?}");
    }

    #[test]
    fn a_pix_screen_also_hashes_widened_like_a_menu_texture() {
        let colours: Vec<u16> = (0..320 * 240).map(|i| (i % 0x7fff) as u16).collect();
        let widen = |v: u16| ((v & 31) * 255 / 31) as u8;
        let image = Image {
            width: 320,
            height: 240,
            rgba: colours
                .iter()
                .flat_map(|&c| [widen(c), widen(c >> 5), widen(c >> 10), 255])
                .collect(),
        };
        let shifted =
            Surface::from_bgr555(320, 240, &colours, Expand::Shift)
                .window_crc(256, 240, |t| t | 0xff00_0000);
        assert_eq!(
            image_candidates(&image)[1],
            Candidate {
                hash: shifted,
                frame: [320, 240]
            }
        );
    }

    #[test]
    fn a_page_cr_translates_also_carries_the_english_hash() {
        let mut candidates = vec![
            Candidate {
                hash: 0x440c_318f,
                frame: [256, 256],
            },
            Candidate {
                hash: 5,
                frame: [1, 1],
            },
        ];
        add_translations(&mut candidates);
        assert_eq!(
            candidates,
            [
                Candidate {
                    hash: 0x440c_318f,
                    frame: [256, 256]
                },
                Candidate {
                    hash: 5,
                    frame: [1, 1]
                },
                Candidate {
                    hash: 0x97de_3e43,
                    frame: [256, 256]
                },
            ]
        );
    }

    #[test]
    fn a_menu_page_hashes_unstretched_with_its_black_clear() {
        // Index 0 is black with the STP bit (cleared only by the black key),
        // index 1 pure red.
        let mut palettes = vec![[0, 0, 0, 255]; PALETTE_ROW_LEN];
        palettes[1] = [255, 0, 0, 255];
        let mut stp = vec![false; PALETTE_ROW_LEN];
        stp[0] = true;
        let texture = Texture8 {
            width: 128,
            height: 1,
            indices: (0..128).map(|x| (x % 2) as u8).collect(),
            palettes,
            stp,
        };
        let mut row = Vec::new();
        for x in 0..128 {
            let texel: u32 = if x % 2 == 1 { 0xfff8_0000 } else { 0 };
            row.extend_from_slice(&texel.to_le_bytes());
        }
        let mut window = row.clone();
        window.resize(256 * 256 * 4, 0);
        let candidates = page_candidates(&texture, 0);
        assert!(candidates.contains(&Candidate {
            hash: crc32(&window),
            frame: [256, 256]
        }));
        // Below 256 x 256 the page also hashes at its own size.
        assert_eq!(
            candidates.last(),
            Some(&Candidate {
                hash: crc32(&row),
                frame: [128, 1]
            })
        );
    }

    #[test]
    fn a_shape_match_needs_the_threshold_and_a_margin() {
        assert_eq!(clear_winner([0.95, 0.5]), Some((0, 0.95)));
        assert_eq!(clear_winner([0.5, 0.97, 0.9]), Some((1, 0.97)));
        assert_eq!(clear_winner([0.95, 0.93]), None, "too close to call");
        assert_eq!(clear_winner([0.85, 0.1]), None, "below the threshold");
        assert_eq!(clear_winner([]), None);
    }

    /// An RGBA image whose texel at `(x, y)` is `texel(x, y)`.
    fn image_of(width: u32, height: u32, texel: impl Fn(u32, u32) -> [u8; 4]) -> Image {
        Image {
            width,
            height,
            rgba: (0..width * height)
                .flat_map(|i| texel(i % width, i / width))
                .collect(),
        }
    }

    #[test]
    fn a_replacement_shape_is_compared_within_the_original_page() {
        // A 128 x 128 page drawn in its left half.
        let page = MaskShape::of_page(&image_of(128, 128, |x, _| {
            [9, 9, 9, if x < 64 { 255 } else { 0 }]
        }));
        // Its 4x replacement covers the whole 256 x 256 window: the same
        // left half, and junk past the page that doesn't count.
        let hd = |edge: u32| {
            MaskShape::of_replacement(&image_of(1024, 1024, |x, y| {
                let drawn = x < edge || x >= 512 || y >= 512;
                [9, 9, 9, if drawn { 200 } else { 0 }]
            }))
        };
        assert_eq!(page.iou(&hd(256)), 1.0);
        // A quarter of the page's texels wider: 64 x 128 of 96 x 128.
        assert!((page.iou(&hd(384)) - 2.0 / 3.0).abs() < 1e-6);
        // A texel is drawn when most of the HD texels over it are.
        assert_eq!(page.iou(&hd(255)), 1.0);
        assert!(page.iou(&hd(254)) < 1.0);
    }

    /// A PNG of `image`, alpha kept.
    #[cfg(feature = "hd")]
    fn png_of(image: &Image) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, image.width, image.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&image.rgba).unwrap();
        writer.finish().unwrap();
        bytes
    }

    #[test]
    #[cfg(feature = "hd")]
    fn an_unnamed_mask_pairs_with_the_camera_its_shape_matches() {
        // Each camera's page is drawn in one colour inside a rectangle
        // `[x0, x1, y0, y1)` (black is a hole), and its background is a
        // solid colour of its own.
        let inside = |[x0, x1, y0, y1]: [u32; 4], x: u32, y: u32| {
            (x0..x1).contains(&x) && (y0..y1).contains(&y)
        };
        let page = |rect, red| {
            image_of(256, 256, |x, y| {
                if inside(rect, x, y) {
                    [red, 100, 50, 255]
                } else {
                    [0, 0, 0, 0]
                }
            })
        };
        let cameras = [
            ("100_000", [0, 100, 0, 50], 200),
            // Named by its own hash.
            ("100_001", [50, 250, 100, 200], 200),
            // Two cameras a few texels apart: no replacement can tell them
            // apart.
            ("100_002", [0, 120, 150, 250], 200),
            ("100_003", [0, 121, 150, 250], 200),
            // Paired by a room list, and a camera with the same page.
            ("100_004", [100, 200, 0, 90], 200),
            ("100_005", [100, 200, 0, 90], 200),
            // The same coverage over different art is still two cameras.
            ("100_006", [10, 60, 60, 90], 200),
            ("100_007", [10, 60, 60, 90], 120),
            // The same page as camera 0.
            ("100_008", [0, 100, 0, 50], 200),
        ];
        let replacement = |rect| {
            png_of(&image_of(512, 512, |x, y| {
                [1, 2, 3, if inside(rect, x / 2, y / 2) { 255 } else { 0 }]
            }))
        };
        let mut writer = crate::pack::PackWriter::new();
        let mut backgrounds = Vec::new();
        for (index, &(name, rect, red)) in cameras.iter().enumerate() {
            let background = image_of(320, 240, |_, _| [8 + 16 * index as u8, 0, 0, 255]);
            writer
                .add(
                    &format!("roommask/{name}.bmp"),
                    crate::bmp::encode_to_vec(&page(rect, red)).unwrap(),
                )
                .unwrap();
            writer
                .add(
                    &format!("roomcut/{name}.bmp"),
                    crate::bmp::encode_to_vec(&background).unwrap(),
                )
                .unwrap();
            backgrounds.push(image_candidates(&background));
        }
        // Camera 1's own mask is named by its hash, so its shape is not a
        // candidate even for the replacement drawn like it.
        let own = image_candidates(&page(cameras[1].1, cameras[1].2))
            .into_iter()
            .find(|candidate| candidate.frame == [256, 256])
            .unwrap()
            .hash;
        for (hash, rect) in [
            (own, cameras[1].1),
            (0x1111_1111, cameras[0].1),
            (0x2222_2222, cameras[1].1),
            (0x3333_3333, cameras[2].1),
            (0x4444_4444, [200, 256, 0, 40]),
            (0x5555_5555, cameras[4].1),
            (0x6666_6666, cameras[6].1),
        ] {
            writer
                .add(&format!("htexture/mask/{hash:08X}.png"), replacement(rect))
                .unwrap();
        }
        let list: Vec<u8> = [0x5555_5555, backgrounds[4][0].hash]
            .iter()
            .flat_map(|hash: &u32| hash.to_le_bytes())
            .collect();
        writer.add("htexture/cache/ROOM1000.dat", list).unwrap();
        let pack = Pack::from_bytes(writer.to_bytes().unwrap()).unwrap();
        let table = HdTable::scan(&pack);
        let paired: Vec<Option<u32>> = backgrounds
            .iter()
            .map(|background| {
                let hashes: HashSet<u32> = table
                    .paired_masks(background)
                    .iter()
                    .map(|candidate| candidate.hash)
                    .collect();
                assert!(hashes.len() <= 1);
                hashes.into_iter().next()
            })
            .collect();
        assert_eq!(
            paired,
            [
                Some(0x1111_1111),
                None,
                None,
                None,
                Some(0x5555_5555),
                Some(0x5555_5555),
                None,
                None,
                Some(0x1111_1111),
            ]
        );
        assert_eq!(HdTable::scan_with(&pack, false).paired.len(), 1);
    }

    /// Every camera's mask page, background and whether its mask resolves.
    fn real_cameras(pack: &Pack, table: &HdTable) -> Vec<(String, Image, Option<Found>)> {
        room_cameras(pack)
            .into_iter()
            .filter_map(|camera| {
                let decode = |stem: &str, geometry, key| {
                    let asset = pack
                        .read_asset(stem, crate::pack::AssetClass::Image)
                        .ok()??;
                    crate::assets::decode_picture(&asset, geometry, key).ok()
                };
                let page = decode(
                    &camera.mask,
                    crate::assets::Geometry::Shipped,
                    Some([0, 0, 0]),
                )?;
                let background = decode(&camera.background, crate::assets::Geometry::Free, None)?;
                let mut candidates = image_candidates(&page);
                candidates.extend(table.paired_masks(&image_candidates(&background)));
                let found = table.find_quietly(&candidates);
                Some((camera.mask, page, found))
            })
            .collect()
    }

    #[test]
    #[ignore = "requires ARKLAY_RE1_PACK and ARKLAY_ENHANCE_PACK"]
    fn real_mask_shapes_agree_with_exact_hashes_and_fill_the_gaps() {
        let (Ok(base), Ok(enhance)) = (
            std::env::var("ARKLAY_RE1_PACK"),
            std::env::var("ARKLAY_ENHANCE_PACK"),
        ) else {
            return;
        };
        let pack = Pack::open_layered(
            std::path::Path::new(&base),
            &[std::path::PathBuf::from(enhance)],
        )
        .unwrap();
        let started = std::time::Instant::now();
        let with_shapes = HdTable::scan(&pack);
        let scan_time = started.elapsed();
        let started = std::time::Instant::now();
        let without = HdTable::scan_with(&pack, false);
        let base_time = started.elapsed();
        eprintln!("scan {scan_time:?}, without shapes {base_time:?}");

        // Validate the rule on the masks that name themselves. Each one's
        // clear winner, among the pages of the cameras still missing a mask
        // plus its own camera's page (the choice the scan makes), and among
        // every camera's page (stricter: near-identical cameras then
        // compete), must be its own camera or none.
        let cameras = real_cameras(&pack, &without);
        let mut pages: Vec<&Image> = Vec::new();
        let group_of: Vec<usize> = cameras
            .iter()
            .map(|(_, page, _)| {
                pages
                    .iter()
                    .position(|known| {
                        (known.width, known.height, &known.rgba)
                            == (page.width, page.height, &page.rgba)
                    })
                    .unwrap_or_else(|| {
                        pages.push(page);
                        pages.len() - 1
                    })
            })
            .collect();
        let groups: Vec<MaskShape> = pages.into_iter().map(MaskShape::of_page).collect();
        let mut missing_groups: Vec<usize> = cameras
            .iter()
            .zip(&group_of)
            .filter(|((_, _, found), _)| found.is_none())
            .map(|(_, &group)| group)
            .collect();
        missing_groups.sort_unstable();
        missing_groups.dedup();
        let (mut exact, mut agreed, mut strict, mut wrong) = (0, 0, 0, 0);
        let mut seen = HashSet::new();
        for (index, (name, page, found)) in cameras.iter().enumerate() {
            let Some(found) = found else { continue };
            let own_hit = image_candidates(page)
                .iter()
                .any(|candidate| without.files.get(&candidate.hash) == Some(&found.path));
            if !own_hit || !seen.insert(found.path.clone()) {
                continue;
            }
            exact += 1;
            let own = group_of[index];
            let hd = MaskShape::of_replacement(&load(&pack, &found.path).unwrap());
            let choices: Vec<usize> = missing_groups
                .iter()
                .copied()
                .filter(|&group| group != own)
                .chain([own])
                .collect();
            match clear_winner(choices.iter().map(|&group| groups[group].iou(&hd))) {
                Some((choice, _)) if choices[choice] == own => agreed += 1,
                Some(_) => wrong += 1,
                None => eprintln!("no clear match: {name} {}", found.path),
            }
            match clear_winner(groups.iter().map(|page| page.iou(&hd))) {
                Some((group, _)) if group == own => strict += 1,
                Some(_) => wrong += 1,
                None => {}
            }
        }
        eprintln!(
            "exact {exact}: shape agreed {agreed} (against every camera {strict}), wrong {wrong}"
        );
        let missing_before: HashSet<&str> = cameras
            .iter()
            .filter(|(_, _, found)| found.is_none())
            .map(|(name, _, _)| name.as_str())
            .collect();
        let after = real_cameras(&pack, &with_shapes);
        let missing_after: Vec<&str> = after
            .iter()
            .filter(|(_, _, found)| found.is_none())
            .map(|(name, _, _)| name.as_str())
            .collect();
        let masks = |cameras: &[(String, Image, Option<Found>)]| {
            cameras
                .iter()
                .filter_map(|(_, _, found)| found.as_ref().map(|found| found.path.clone()))
                .collect::<HashSet<_>>()
                .len()
        };
        let total = with_shapes
            .files
            .values()
            .filter(|path| in_mask_dir(path))
            .count();
        eprintln!(
            "cameras missing {} -> {}; masks used {} -> {} of {total}",
            missing_before.len(),
            missing_after.len(),
            masks(&cameras),
            masks(&after),
        );
        eprintln!("still missing: {missing_after:?}");
        assert_eq!(wrong, 0, "the rule never picks another camera");
        assert!(agreed + 1 >= exact, "the rule agrees with the exact hashes");
        // RE-ENHANCE v2.0: every mask it ships now has a camera.
        assert_eq!(masks(&after), total);
    }
}
