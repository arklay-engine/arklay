//! The handle-based art and drawing half of the screen `api`.
//!
//! `api:picture(stem)` and `api:page(stem)` load pack art once and return a
//! small integer handle; the draw methods take that handle and map one to one
//! onto the [`Framebuffer`] primitives the original screens use, so a ported
//! screen draws exactly the pixels its Rust predecessor did. Rectangles are
//! flat `x, y, w, h` arguments in the layout space `api:width()` and
//! `api:height()` report.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use mlua::{Table, UserDataMethods, Value};

use crate::anim::{self, Mat4x3};
use crate::assets::{Artwork, Geometry};
use crate::budget;
use crate::font::{GLYPH_HEIGHT, Tint};
use crate::hires::{self, Replacement};
use crate::ivm::Ivm;
use crate::model::{Emd, Keyframe};
use crate::pack::{AssetClass, Pack};
use crate::render::{Camera, EntityLight, EntityLightSource, Framebuffer, SpriteBlend};
use crate::state::Image;

use super::{Api, Decoded, colour_rgb, colour_tint, coord, no_draw};

/// One loaded piece of art.
enum Asset {
    /// A decoded picture (straight RGBA), drawn as a backdrop or RGBA sprite.
    Picture(Image),
    /// A sprite page in its native shape: indexed with CLUT rows, or RGBA.
    Page(Artwork),
    /// An item-view model and the size of the file it was parsed from.
    Model(Arc<Ivm>, usize),
    /// A character's model and the bytes it holds
    /// ([`Emd::decoded_bytes`]).
    Character(Arc<Emd>, usize),
}

impl Asset {
    fn size(&self) -> (u32, u32) {
        match self {
            Self::Picture(image) => (image.width, image.height),
            Self::Page(art) => (art.width(), art.height()),
            Self::Model(model, _) => (model.texture.width, model.texture.height),
            Self::Character(model, _) => (model.texture.width, model.texture.height),
        }
    }

    /// The bytes the asset holds, against the screen's art budget.
    fn bytes(&self) -> usize {
        let (width, height) = self.size();
        let pixels = (width as usize)
            .saturating_mul(height as usize)
            .saturating_mul(4);
        match self {
            Self::Model(_, held) | Self::Character(_, held) => pixels.saturating_add(*held),
            _ => pixels,
        }
    }
}

/// The art a screen loaded, by handle.
#[derive(Default)]
pub(super) struct Assets {
    /// The art by handle - 1; a released handle's slot stays empty, so a
    /// stale handle fails instead of drawing later art.
    list: Vec<Option<Asset>>,
    /// Loaded (unreleased) art, against [`budget::MAX_SCREEN_IMAGES`].
    live: usize,
    /// Whether each handle counts against [`Assets::live`]. Per-item icons are
    /// exempt: the item table bounds them and a converted pack has one per
    /// item, while the screen's own art still has its full budget.
    counted: Vec<bool>,
    /// Handle by load key, so asking twice returns the same art.
    by_key: HashMap<String, usize>,
    /// Decoded bytes held, against [`budget::MAX_SCREEN_IMAGE_BYTES`].
    bytes: usize,
    /// The HD replacement of each `(handle, CLUT row)` and of each
    /// `(handle, picture cell)` looked up so far. It is pack art rather than
    /// the script's, so it is not budgeted.
    hd: RefCell<HashMap<(usize, usize), HdArt>>,
    hd_cells: RefCell<HashMap<(usize, [i32; 4]), HdArt>>,
    /// The HD replacement of each `(entry, CLUT row)` the entry-based draws
    /// (`api:background`, `api:sprite`, `api:sprite_region`) looked up.
    hd_entries: RefCell<HashMap<(String, usize), HdArt>>,
    /// The named HD keys (`hires/<stem>`) a handle's art answers to, most
    /// specific first.
    hd_names: RefCell<HashMap<usize, Vec<String>>>,
    /// The texel indices of each indexed page a recoloured HD draw
    /// (`api:page_lut`) used, shared by its draws.
    indices: RefCell<HashMap<usize, Arc<hires::Indices>>>,
}

/// A looked-up HD replacement of screen art; `None` when the pack has none.
pub(super) type HdArt = Option<Arc<Replacement>>;

impl Assets {
    /// Load `key`'s art with `load` once, returning its 1-based handle. The
    /// set is bounded by count and by the pixels it holds.
    fn load(
        &mut self,
        key: String,
        load: impl FnOnce() -> anyhow::Result<Asset>,
    ) -> anyhow::Result<usize> {
        if let Some(&handle) = self.by_key.get(&key) {
            return Ok(handle);
        }
        budget::check_len(self.live + 1, budget::MAX_SCREEN_IMAGES, "screen art count")?;
        let asset = load()?;
        let bytes = asset.bytes();
        budget::check_len(
            self.bytes.saturating_add(bytes),
            budget::MAX_SCREEN_IMAGE_BYTES,
            "screen art bytes",
        )?;
        self.bytes = self.bytes.saturating_add(bytes);
        self.live += 1;
        self.list.push(Some(asset));
        self.counted.push(true);
        let handle = self.list.len();
        self.by_key.insert(key, handle);
        Ok(handle)
    }

    /// [`Assets::load`] for a per-item icon: the count cap is skipped because
    /// the item table bounds the set to 94 ids and a converted pack has one
    /// small icon each. The byte cap still applies.
    fn load_icon(
        &mut self,
        key: String,
        load: impl FnOnce() -> anyhow::Result<Asset>,
    ) -> anyhow::Result<usize> {
        if let Some(&handle) = self.by_key.get(&key) {
            return Ok(handle);
        }
        let asset = load()?;
        let bytes = asset.bytes();
        budget::check_len(
            self.bytes.saturating_add(bytes),
            budget::MAX_SCREEN_IMAGE_BYTES,
            "screen art bytes",
        )?;
        self.bytes = self.bytes.saturating_add(bytes);
        self.list.push(Some(asset));
        self.counted.push(false);
        let handle = self.list.len();
        self.by_key.insert(key, handle);
        Ok(handle)
    }

    /// Drop `handle`'s art, returning its bytes and count to the budget. The
    /// handle is invalid afterwards and never handed out again; releasing an
    /// unknown or already released handle does nothing.
    fn release(&mut self, handle: usize) {
        let Some(asset) = handle
            .checked_sub(1)
            .and_then(|index| self.list.get_mut(index))
            .and_then(Option::take)
        else {
            return;
        };
        self.bytes = self.bytes.saturating_sub(asset.bytes());
        if handle
            .checked_sub(1)
            .and_then(|index| self.counted.get_mut(index))
            .is_some_and(std::mem::take)
        {
            self.live -= 1;
        }
        self.by_key.retain(|_, &mut held| held != handle);
        self.hd.get_mut().retain(|&(held, _), _| held != handle);
        self.hd_names.get_mut().remove(&handle);
        self.indices.get_mut().remove(&handle);
    }

    /// Register the named HD keys `handle`'s art answers to, most specific
    /// first. A lookup prefers these over the content hashes.
    fn set_hd_names(&self, handle: usize, names: Vec<String>) {
        if !names.is_empty() {
            self.hd_names.borrow_mut().insert(handle, names);
        }
    }

    /// How much art counts against the screen's budget just now.
    #[cfg(test)]
    pub(super) fn live(&self) -> usize {
        self.live
    }

    /// The handle `key`'s art was loaded under, if it is still live.
    fn handle(&self, key: &str) -> Option<usize> {
        self.by_key.get(key).copied()
    }

    /// The texel indices of indexed page `handle`, copied once.
    fn indices(&self, handle: usize) -> mlua::Result<Arc<hires::Indices>> {
        if let Some(found) = self.indices.borrow().get(&handle) {
            return Ok(found.clone());
        }
        let Artwork::Indexed(texture) = self.page(handle)? else {
            return Err(mlua::Error::RuntimeError(format!(
                "art handle {handle} is not an indexed page"
            )));
        };
        let indices = Arc::new(hires::Indices {
            width: texture.width,
            height: texture.height,
            texels: texture.indices.clone(),
        });
        self.indices.borrow_mut().insert(handle, indices.clone());
        Ok(indices)
    }

    fn get(&self, handle: usize) -> mlua::Result<&Asset> {
        handle
            .checked_sub(1)
            .and_then(|index| self.list.get(index))
            .and_then(Option::as_ref)
            .ok_or_else(|| mlua::Error::RuntimeError(format!("invalid art handle {handle}")))
    }

    pub(super) fn picture(&self, handle: usize) -> mlua::Result<&Image> {
        match self.get(handle)? {
            Asset::Picture(image) => Ok(image),
            _ => Err(mlua::Error::RuntimeError(format!(
                "art handle {handle} is not a picture"
            ))),
        }
    }

    fn page(&self, handle: usize) -> mlua::Result<&Artwork> {
        match self.get(handle)? {
            Asset::Page(art) => Ok(art),
            _ => Err(mlua::Error::RuntimeError(format!(
                "art handle {handle} is not a page"
            ))),
        }
    }

    /// The HD replacement `pack` carries for `handle`'s art drawn through
    /// CLUT row `row` (ignored but for an indexed page), looked up once.
    ///
    /// A named per-item key registered through [`Assets::set_hd_names`] wins
    /// over the content hashes, so a pack can replace art that is composited
    /// per frame (the inventory icons).
    fn replacement(&self, pack: &Pack, handle: usize, row: usize) -> Option<Arc<Replacement>> {
        let asset = self.get(handle).ok()?;
        let row = match asset {
            Asset::Page(Artwork::Indexed(_)) => row,
            _ => 0,
        };
        if let Some(found) = self.hd.borrow().get(&(handle, row)) {
            return found.clone();
        }
        let size = asset.size();
        let named = self.hd_names.borrow().get(&handle).and_then(|names| {
            names
                .iter()
                .find_map(|name| hires::named_replacement(pack, name, [size.0, size.1]))
        });
        let found = match named {
            Some(found) => Some(found),
            None => {
                // The frame is captured without an HD pack too (for the
                // viewer's model), so skip hashing the art when there is
                // nothing to find.
                let candidates = match asset {
                    _ if pack.hires().is_empty() => Vec::new(),
                    Asset::Picture(image) | Asset::Page(Artwork::Rgba(image)) => {
                        hires::image_candidates(image)
                    }
                    Asset::Page(Artwork::Indexed(texture)) => hires::page_candidates(texture, row),
                    Asset::Model(model, _) => hires::page_candidates(&model.texture, row),
                    // A character's polygons pick their own CLUT rows, so
                    // one row's replacement can't stand for its page.
                    Asset::Character(..) => Vec::new(),
                };
                if candidates.is_empty() {
                    None
                } else {
                    let (width, height) = size;
                    hires::replacement(pack, &candidates, || {
                        format!("{width}x{height} screen art (row {row})")
                    })
                }
            }
        };
        self.hd.borrow_mut().insert((handle, row), found.clone());
        found
    }

    /// The HD replacement `pack` carries for the `cell` (`[x, y, w, h]`) of
    /// picture `handle`, hashed as an upload of that cell alone, looked up
    /// once.
    fn cell_replacement(&self, pack: &Pack, handle: usize, cell: [i32; 4]) -> HdArt {
        if let Some(found) = self.hd_cells.borrow().get(&(handle, cell)) {
            return found.clone();
        }
        let found = match self.get(handle) {
            Ok(Asset::Picture(image)) if !pack.hires().is_empty() => {
                crop(image, cell).and_then(|crop| {
                    let candidates = hires::image_candidates(&crop);
                    let [x, y, w, h] = cell;
                    hires::replacement(pack, &candidates, || {
                        format!("{w}x{h} screen art cell at ({x}, {y})")
                    })
                })
            }
            _ => None,
        };
        self.hd_cells
            .borrow_mut()
            .insert((handle, cell), found.clone());
        found
    }

    /// The HD replacement `pack` carries for `image`, decoded from pack
    /// `entry`, drawn through CLUT row `row` (ignored but for an indexed
    /// image), looked up once.
    pub(super) fn entry_replacement(
        &self,
        pack: &Pack,
        entry: &str,
        image: &Decoded,
        row: usize,
    ) -> HdArt {
        let row = match image {
            Decoded::Indexed(_) => row,
            Decoded::Rgba(_) => 0,
        };
        let key = (entry.to_string(), row);
        if let Some(found) = self.hd_entries.borrow().get(&key) {
            return found.clone();
        }
        let candidates = match image {
            _ if pack.hires().is_empty() => Vec::new(),
            Decoded::Rgba(image) => hires::image_candidates(image),
            Decoded::Indexed(texture) => hires::page_candidates(texture, row),
        };
        let found = if candidates.is_empty() {
            None
        } else {
            hires::replacement(pack, &candidates, || format!("{entry} (row {row})"))
        };
        self.hd_entries.borrow_mut().insert(key, found.clone());
        found
    }

    fn model(&self, handle: usize) -> mlua::Result<&Arc<Ivm>> {
        match self.get(handle)? {
            Asset::Model(model, _) => Ok(model),
            _ => Err(mlua::Error::RuntimeError(format!(
                "art handle {handle} is not a model"
            ))),
        }
    }

    fn character(&self, handle: usize) -> mlua::Result<&Arc<Emd>> {
        match self.get(handle)? {
            Asset::Character(model, _) => Ok(model),
            _ => Err(mlua::Error::RuntimeError(format!(
                "art handle {handle} is not a character's model"
            ))),
        }
    }
}

/// The HD replacement of `handle`'s art through CLUT row `row`, when the
/// frame being drawn captures replacements.
fn hd_art(
    assets: &Assets,
    pack: &Pack,
    framebuffer: &Framebuffer,
    handle: usize,
    row: usize,
) -> Option<Arc<Replacement>> {
    if framebuffer.records() {
        assets.replacement(pack, handle, row)
    } else {
        None
    }
}

/// The `[x, y, w, h]` region of `image`, or `None` when it is empty or not
/// wholly inside the image.
fn crop(image: &Image, [x, y, w, h]: [i32; 4]) -> Option<Image> {
    let [x, y, w, h] = [x, y, w, h].map(|v| u32::try_from(v).ok());
    let (x, y, w, h) = (x?, y?, w?, h?);
    if w == 0 || h == 0 || x + w > image.width || y + h > image.height {
        return None;
    }
    let stride = image.width as usize * 4;
    let rgba = (y..y + h)
        .flat_map(|row| {
            let start = row as usize * stride + x as usize * 4;
            image
                .rgba
                .get(start..start + w as usize * 4)
                .unwrap_or(&[])
                .iter()
                .copied()
        })
        .collect::<Vec<u8>>();
    (rgba.len() == (w * h * 4) as usize).then_some(Image {
        width: w,
        height: h,
        rgba,
    })
}

/// The destination corners and normalised texture coordinates of a sprite
/// drawing the `src` texels of art `hd` replaces into `dst`, flipped per
/// axis by `mirror`.
pub(super) fn sprite_quad(
    hd: &Replacement,
    src: [i32; 4],
    dst: [i32; 4],
    mirror: [bool; 2],
) -> ([f32; 4], [f32; 4]) {
    let [sx, sy, sw, sh] = src.map(|v| v as f32);
    let [dx, dy, dw, dh] = dst.map(|v| v as f32);
    let [fw, fh] = hd.frame.map(|v| v as f32);
    let (mut u0, mut u1) = (sx / fw, (sx + sw) / fw);
    let (mut v0, mut v1) = (sy / fh, (sy + sh) / fh);
    if mirror[0] {
        std::mem::swap(&mut u0, &mut u1);
    }
    if mirror[1] {
        std::mem::swap(&mut v0, &mut v1);
    }
    ([dx, dy, dx + dw, dy + dh], [u0, v0, u1, v1])
}

/// A sprite's colour multiplier: `tint` and then `scale`, as the software
/// sprite applies them.
pub(super) fn sprite_colour(tint: Tint, scale: u8) -> [f32; 3] {
    tint.rgb()
        .map(|channel| f32::from(channel) / 255.0 * f32::from(scale) / 255.0)
}

/// How a page's 15-bit colours became 8-bit ones in RE 2's PC version, which
/// differs from sheet to sheet, by its frames. The pack's are the decoder's,
/// `v * 255 / 31` of a 5-bit level `v`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Widen {
    /// As the pack has them.
    Kept,
    /// The level's top bits again below it, `v << 3 | v >> 2`.
    Bits,
    /// `8v + 7`, and 0 for 0.
    Seven,
}

impl Widen {
    pub(super) fn named(name: Option<&str>) -> mlua::Result<Self> {
        match name {
            None => Ok(Self::Kept),
            Some("bits") => Ok(Self::Bits),
            Some("seven") => Ok(Self::Seven),
            Some(other) => Err(mlua::Error::RuntimeError(format!(
                "unknown widening `{other}` (bits, seven)"
            ))),
        }
    }

    /// The pack colour `colour` as the PC version holds it. One with a
    /// channel that isn't a 5-bit level's (a mod's own page) is kept whole.
    pub(super) fn colour(self, colour: [u8; 3]) -> [u8; 3] {
        let level = |channel: u8| {
            let level = (u32::from(channel) * 31).div_ceil(255);
            (level * 255 / 31 == u32::from(channel)).then_some(level)
        };
        let [Some(r), Some(g), Some(b)] = colour.map(level) else {
            return colour;
        };
        match self {
            Self::Kept => colour,
            Self::Bits => [r, g, b].map(|level| (level << 3 | level >> 2) as u8),
            Self::Seven => [r, g, b].map(|level| (level * 8 + 7 * level.min(1)) as u8),
        }
    }
}

/// The colours RE 2's PC version draws CLUT row `row` of `texture` in as a
/// sprite under the colour `rgb`, by its frames: each channel of the texel
/// as `widen` has it, times twice the colour's, capped at 255, over 255 and
/// rounded. So 0x80 is the texel as it is, and nothing brightens it. Index
/// 0 draws nothing, as a sprite's doesn't, and nor does black without the
/// STP bit, the PlayStation's clear colour.
pub(super) fn modulated_colours(
    texture: &crate::model::Texture8,
    row: usize,
    rgb: [u8; 3],
    widen: Widen,
) -> [Option<[u8; 3]>; 256] {
    std::array::from_fn(|index| {
        let texel = texture.palette(row, index as u8);
        let clear = texel[..3] == [0; 3] && !texture.palette_stp(row, index as u8);
        (index != 0 && texel[3] != 0 && !clear).then(|| {
            let wide = widen.colour([texel[0], texel[1], texel[2]]);
            std::array::from_fn(|channel| {
                let by = (u32::from(rgb[channel]) * 2).min(255);
                ((u32::from(wide[channel]) * by + 127) / 255) as u8
            })
        })
    })
}

/// A `(handle)` or `(nil, message)` load result.
fn loaded(result: anyhow::Result<usize>) -> (Option<usize>, Option<String>) {
    match result {
        Ok(handle) => (Some(handle), None),
        Err(err) => (None, Some(format!("{err:#}"))),
    }
}

/// The error an effect call raises before `api:effect_scene`.
fn no_scene() -> mlua::Error {
    mlua::Error::RuntimeError("the screen has no effect scene (call api:effect_scene first)".into())
}

/// A clamped `[x, y, w, h]`.
/// A page draw's `blend` name: `opaque` (the default), `half`, `variant`
/// (the original's variant-2 half alpha) or `add`.
/// A sprite's blend: a mode's name, or an integer, the alpha the sprite
/// goes over the frame at.
fn sprite_blend_value(value: Option<&Value>) -> mlua::Result<SpriteBlend> {
    match value {
        None | Some(Value::Nil) => Ok(SpriteBlend::Opaque),
        Some(Value::Integer(alpha)) => Ok(SpriteBlend::Alpha(byte(*alpha))),
        Some(Value::String(name)) => sprite_blend(Some(&name.to_str()?)),
        Some(other) => Err(mlua::Error::RuntimeError(format!(
            "a blend is a name or an alpha, got {}",
            other.type_name()
        ))),
    }
}

fn sprite_blend(name: Option<&str>) -> mlua::Result<SpriteBlend> {
    match name {
        None | Some("opaque") => Ok(SpriteBlend::Opaque),
        Some("half") => Ok(SpriteBlend::Half),
        Some("variant") => Ok(SpriteBlend::HalfVariant),
        Some("add") => Ok(SpriteBlend::Add),
        Some(other) => Err(mlua::Error::RuntimeError(format!(
            "unknown blend {other:?} (opaque, half, variant, add)"
        ))),
    }
}

fn rect(x: i32, y: i32, w: i32, h: i32) -> [i32; 4] {
    [coord(x), coord(y), coord(w), coord(h)]
}

/// A colour channel or alpha byte.
fn byte(value: i64) -> u8 {
    value.clamp(0, 255) as u8
}

/// A line-primitive colour channel. The original hands its line primitives a
/// colour byte, which the reference converts to a float by `/ 256` and back by
/// `* 255`, so every channel draws one step down (0xD8 as 0xD7, 0xFF as 0xFE).
fn line_colour(value: u8) -> u8 {
    (u16::from(value) * 255 / 256) as u8
}

/// An integer field of `table`, `0` when absent.
fn field(table: &Table, key: &str) -> mlua::Result<i32> {
    Ok(table.get::<Option<i32>>(key)?.unwrap_or(0))
}

/// A whole-number field of `table`, `0` when absent. Unlike [`field`] it
/// takes nothing else for one: a number with a fraction is an error.
fn whole(table: &Table, key: &str) -> mlua::Result<i64> {
    match table.get::<Value>(key)? {
        Value::Nil => Ok(0),
        Value::Integer(value) => Ok(value),
        _ => Err(mlua::Error::RuntimeError(format!(
            "{key} is not a whole number"
        ))),
    }
}

/// A `{x, y, z}` triple.
fn triple(table: &Table, key: &str) -> mlua::Result<[i32; 3]> {
    let value: Table = table.get(key)?;
    Ok([value.get(1)?, value.get(2)?, value.get(3)?])
}

/// The joint matrix a `joints` list composes, root first. Each joint is
/// `{rx, ry, rz, tx, ty, tz}` in the engine's 12-bit angles.
fn joint_matrix(joints: Option<Table>) -> mlua::Result<Mat4x3> {
    // The chain starts at the root joint itself: composing onto the table's
    // zero rotation would skew it, as that is not an exact 4.12 identity.
    let mut matrix: Option<Mat4x3> = None;
    if let Some(joints) = joints {
        for joint in joints.sequence_values::<Table>() {
            let joint = joint?;
            let local = Mat4x3 {
                r: anim::rotation_matrix(
                    field(&joint, "rx")?,
                    field(&joint, "ry")?,
                    field(&joint, "rz")?,
                ),
                t: [
                    field(&joint, "tx")?,
                    field(&joint, "ty")?,
                    field(&joint, "tz")?,
                ],
            };
            matrix = Some(match matrix {
                Some(parent) => anim::compose(&parent, &local),
                None => local,
            });
        }
    }
    Ok(matrix.unwrap_or(Mat4x3 {
        r: [[4096, 0, 0], [0, 4096, 0], [0, 0, 4096]],
        t: [0; 3],
    }))
}

/// A light record: `{ambient = 12-bit level, sources = {{pos = {x, y, z},
/// color = {r, g, b}}, ...}}`. As the reference's `FUN_0040ac80` builds it,
/// each position is a direction, normalized to 4.12 with Y negated, and each
/// colour byte caps at the `0x80` full scale; the ambient converts by
/// `(value * 255) / 4096`. Missing sources are dark.
fn light(table: &Table) -> mlua::Result<EntityLight> {
    let ambient = f64::from(field(table, "ambient")?.clamp(0, 0xFFFF) * 255 / 4096);
    let dark = EntityLightSource {
        dir: [0; 3],
        color: [0; 3],
    };
    let mut sources = [dark, dark, dark];
    if let Some(list) = table.get::<Option<Table>>("sources")? {
        for (slot, source) in sources.iter_mut().zip(list.sequence_values::<Table>()) {
            let source = source?;
            let pos = triple(&source, "pos")?.map(f64::from);
            let length = pos.iter().map(|v| v * v).sum::<f64>().sqrt().max(1.0);
            let color = triple(&source, "color")?;
            *slot = EntityLightSource {
                dir: [
                    (pos[0] / length * 4096.0) as i32,
                    (-pos[1] / length * 4096.0) as i32,
                    (pos[2] / length * 4096.0) as i32,
                ],
                color: color.map(|c| c.clamp(0, 0x80) as u8),
            };
        }
    }
    Ok(EntityLight {
        ambient: [ambient; 3],
        sources,
    })
}

/// A character's pose: the keyframe of `{bank, clip, frame}`, bank 0 the
/// model's own animations and bank `n` its `n`th further one, or with no
/// `pose` the model as its file has it, every joint unturned. `None` when
/// the model has no such frame, as it has none of a negative number;
/// anything but a whole number is an error.
///
/// A pose's `height` scales the root's height in 4.12, as a short zombie's
/// `Parts0_pos_y` does, and its `turn` adds `{x, y, z}` to the angles of the
/// joints it is keyed by, numbered from 0, as a neck's turn does. A key
/// that isn't an integer is an error; a joint the model lacks is skipped.
fn character_pose(model: &Emd, pose: Option<Table>) -> mlua::Result<Option<Keyframe>> {
    let Some(pose) = pose else {
        return Ok(Some(Keyframe {
            offset: model.skeleton.relative.first().copied().unwrap_or_default(),
            vertex: [0; 3],
            rotations: vec![anim::IDENTITY_ROTATION; model.skeleton.relative.len()],
        }));
    };
    let (Ok(bank), Ok(clip), Ok(frame)) = (
        usize::try_from(whole(&pose, "bank")?),
        usize::try_from(whole(&pose, "clip")?),
        usize::try_from(whole(&pose, "frame")?),
    ) else {
        return Ok(None);
    };
    let (keyframes, clips) = match bank.checked_sub(1) {
        None => (&model.keyframes, &model.clips),
        Some(bank) => match model.banks.get(bank) {
            Some(bank) => (&bank.keyframes, &bank.clips),
            None => return Ok(None),
        },
    };
    let Some(mut keyframe) = clips
        .get(clip)
        .and_then(|clip| clip.frames.get(frame))
        .and_then(|frame| keyframes.get(usize::from(frame.keyframe)))
        .cloned()
    else {
        return Ok(None);
    };
    if let Some(height) = pose.get::<Option<i32>>("height")? {
        let scaled = (i64::from(keyframe.offset[1]) * i64::from(height)) >> 12;
        keyframe.offset[1] = scaled as i16;
    }
    if let Some(turn) = pose.get::<Option<Table>>("turn")? {
        for pair in turn.pairs::<Value, Table>() {
            let (joint, by) = pair?;
            let Value::Integer(joint) = joint else {
                return Err(mlua::Error::RuntimeError(format!(
                    "a pose's turn is keyed by {}, not a joint's number",
                    joint.type_name()
                )));
            };
            let Some(angles) = usize::try_from(joint)
                .ok()
                .and_then(|joint| keyframe.rotations.get_mut(joint))
            else {
                continue;
            };
            if *angles == anim::IDENTITY_ROTATION {
                *angles = [0; 3];
            }
            for (axis, angle) in angles.iter_mut().enumerate() {
                *angle = angle.wrapping_add(by.get::<i32>(axis + 1)? as i16);
            }
        }
    }
    Ok(Some(keyframe))
}

/// The matrices of a character's parts beyond its skeleton's, each `{parent,
/// x, y, z}`: a part unturned at that place on part `parent`'s, numbered
/// from 0, as an enemy's routine hangs one. They follow `joints` in order;
/// a part whose parent isn't there yet is an error.
fn extra_parts(joints: &mut Vec<Mat4x3>, parts: Option<Table>) -> mlua::Result<()> {
    let Some(parts) = parts else {
        return Ok(());
    };
    for part in parts.sequence_values::<Table>() {
        let part = part?;
        let parent: usize = part.get(1)?;
        let parent = joints.get(parent).ok_or_else(|| {
            mlua::Error::RuntimeError(format!("part {} has no part {parent}", joints.len()))
        })?;
        let local = Mat4x3 {
            r: anim::IDENTITY_MATRIX,
            t: [part.get(2)?, part.get(3)?, part.get(4)?],
        };
        let world = anim::compose(parent, &local);
        budget::check_len(joints.len() + 1, budget::MAX_EMD_JOINTS, "character parts")
            .map_err(mlua::Error::external)?;
        joints.push(world);
    }
    Ok(())
}

/// Draw `bytes` through the font sheet's own grid addressing:
/// `u = (c % 18) * cell_w`, `v = (c / 18) * 14`, no header offset, advancing
/// `glyph_w` (the reference's `ending_draw_text`, `EndingScreen.cpp:296-324`). The
/// bytes are raw sheet cells, so a `0x5E` lands on the sheet's own separator
/// cell.
fn draw_grid_text(
    framebuffer: &mut Framebuffer,
    font: &crate::font::Font,
    origin: [i32; 2],
    scale: u8,
    bytes: &[u8],
) {
    let glyph_w = font.metrics.glyph_w;
    let cell_w = font.metrics.cell_w;
    if glyph_w <= 0 {
        return;
    }
    let [mut pen, y] = origin;
    for &byte in bytes {
        if byte != b' ' {
            let u = i32::from(byte % 18) * cell_w;
            let v = i32::from(byte / 18) * GLYPH_HEIGHT;
            framebuffer.draw_indexed_sprite_scaled(
                &font.texture,
                [u, v, cell_w, GLYPH_HEIGHT],
                [pen, y, cell_w, GLYPH_HEIGHT],
                0,
                scale,
                Tint::White,
            );
        }
        pen = pen.saturating_add(glyph_w);
    }
}

mod methods;
pub(super) use methods::add_methods;

/// The destination corners and texture coordinates of a background drawn at
/// display `origin`: [`Framebuffer::draw_background`] stretches the image over
/// the logical 320x240 quad centred in the frame and repeats its edges past
/// it, so the quad covers the whole framebuffer and samples past `0..=1`
/// there.
pub(super) fn background_quad(
    hd: &Replacement,
    image: &Image,
    framebuffer: &Framebuffer,
    origin: [i32; 2],
) -> ([f32; 4], [f32; 4]) {
    let (width, height) = (framebuffer.width as f32, framebuffer.height as f32);
    // Centred as `draw_background` centres it, in whole pixels.
    let left = (origin[0] + (framebuffer.width as i32 - crate::render::WIDTH as i32) / 2) as f32;
    let top = (origin[1] + (framebuffer.height as i32 - crate::render::HEIGHT as i32) / 2) as f32;
    // Logical-quad position to source texels, then to the replacement's frame.
    let u =
        |x: f32| (x - left) / crate::render::WIDTH as f32 * image.width as f32 / hd.frame[0] as f32;
    let v = |y: f32| {
        (y - top) / crate::render::HEIGHT as f32 * image.height as f32 / hd.frame[1] as f32
    };
    (
        [0.0, 0.0, width, height],
        [u(0.0), v(0.0), u(width), v(height)],
    )
}

/// The frame's `rect` of picture `handle` drawn again as `draw_background`
/// draws the whole at `origin`: the same placement and sampling, and the
/// same HD replacement over just that part.
pub(super) fn draw_background_rect(
    assets: &Assets,
    pack: &Pack,
    framebuffer: &mut Framebuffer,
    handle: usize,
    origin: [i32; 2],
    rect: [i32; 4],
) -> mlua::Result<()> {
    let image = assets.picture(handle)?;
    let draw = |framebuffer: &mut Framebuffer| {
        framebuffer.draw_background_within(image, origin, [255; 3], rect);
    };
    match hd_art(assets, pack, framebuffer, handle, 0) {
        Some(hd) => {
            let (_, [u0, v0, u1, v1]) = background_quad(&hd, image, framebuffer, origin);
            let (width, height) = (framebuffer.width as f32, framebuffer.height as f32);
            let [x0, y0] = [rect[0] as f32, rect[1] as f32];
            let [x1, y1] = [(rect[0] + rect[2]) as f32, (rect[1] + rect[3]) as f32];
            let at = |from: f32, to: f32, t: f32, extent: f32| from + (to - from) * t / extent;
            let uv = [
                at(u0, u1, x0, width),
                at(v0, v1, y0, height),
                at(u0, u1, x1, width),
                at(v0, v1, y1, height),
            ];
            let clip = framebuffer.clip.take();
            framebuffer.draw_replaced(hd, [x0, y0, x1, y1], uv, [1.0; 3], draw);
            framebuffer.clip = clip;
        }
        None => draw(framebuffer),
    }
    Ok(())
}

/// A screen camera's projection offset: the rasterizer projects about the
/// 4:3 frame's centre, so a shorter layout band (a wider aspect) moves the
/// centre up by half the rows it lost, and a wider canvas moves it right by
/// half the columns it gained, keeping the model centred in the layout.
/// `screen` is the script's own offset, held to the range of [`coord`].
pub(super) fn band_centre(framebuffer: &Framebuffer, screen: [i32; 2]) -> [i32; 2] {
    let gained = framebuffer.width as i32 - crate::render::WIDTH as i32;
    let lost = crate::render::HEIGHT as i32 - framebuffer.height as i32;
    [coord(screen[0]) + gained / 2, coord(screen[1]) - lost / 2]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fixture's model: two joints, and one clip whose frames are the
    /// keyframes with roots at `(1, 2, 3)` and `(0, 2, 3)`.
    fn character() -> Emd {
        crate::emd2::parse_pld(&crate::emd2::tests::pld()).unwrap()
    }

    #[test]
    fn a_sheets_levels_widen_and_modulate_as_the_pc_version_shows_them() {
        // The pack's colour of a 5-bit level, and the two ways the PC
        // version has it.
        let pack = |level: u32| (level * 255 / 31) as u8;
        for (level, bits, seven) in [
            (0, 0, 0),
            (1, 8, 15),
            (7, 57, 63),
            (10, 82, 87),
            (20, 165, 167),
            (31, 255, 255),
        ] {
            let colour = [pack(level), 0, pack(31)];
            assert_eq!(Widen::Kept.colour(colour), colour);
            assert_eq!(Widen::Bits.colour(colour), [bits, 0, 255], "{level}");
            assert_eq!(Widen::Seven.colour(colour), [seven, 0, 255], "{level}");
        }
        assert_eq!(pack(20), 164);
        // Every level comes back from the pack's colour of it, and no other
        // byte is one's.
        let levels: Vec<u8> = (0..32).map(pack).collect();
        for channel in 0..=255u8 {
            let [wide, ..] = Widen::Bits.colour([channel, 0, 0]);
            match levels.iter().position(|&level| level == channel) {
                Some(level) => assert_eq!(usize::from(wide), level << 3 | level >> 2),
                None => assert_eq!(wide, channel),
            }
        }
        // A colour with a channel that is no level's is a mod's own, and
        // kept whole: 164 is level 20's, but not beside 100.
        for widen in [Widen::Bits, Widen::Seven] {
            assert_eq!(widen.colour([164, 100, 0]), [164, 100, 0]);
            assert_eq!(widen.colour([254, 255, 255]), [254, 255, 255]);
        }
        assert_eq!(Widen::named(None).unwrap(), Widen::Kept);
        assert_eq!(Widen::named(Some("bits")).unwrap(), Widen::Bits);
        assert_eq!(Widen::named(Some("seven")).unwrap(), Widen::Seven);
        assert!(Widen::named(Some("eight")).is_err());

        // A row whose index `i` is red at level `i`, with black at 5 and 6
        // and the STP bit on 6.
        let mut palettes: Vec<[u8; 4]> = (0..256u32).map(|i| [pack(i & 31), 0, 0, 255]).collect();
        palettes[5] = [0, 0, 0, 255];
        palettes[6] = [0, 0, 0, 255];
        let mut stp = vec![false; 256];
        stp[6] = true;
        let texture = crate::model::Texture8 {
            width: 1,
            height: 1,
            indices: vec![0],
            palettes,
            stp,
        };
        let red = |rgb: u8, widen: Widen| {
            let colours = modulated_colours(&texture, 0, [rgb; 3], widen);
            move |index: usize| colours[index].map(|colour| colour[0])
        };
        // 0x80 is the texel as it is, and more is no brighter.
        let own = red(0x80, Widen::Bits);
        assert_eq!((own(20), own(31)), (Some(165), Some(255)));
        assert_eq!(red(0xFF, Widen::Bits)(20), Some(165));
        // Index 0 is clear, and black without the STP bit.
        assert_eq!((own(0), own(5), own(6)), (None, None, Some(0)));
        // Less dims it, rounded: the status screen's dim tabs (40), a lit
        // label's other channels (64) and the cursor's pulse at 122.
        let dim = red(40, Widen::Bits);
        assert_eq!((dim(3), dim(14), dim(19)), (Some(8), Some(36), Some(49)));
        let half = red(64, Widen::Bits);
        assert_eq!((half(13), half(21)), (Some(54), Some(87)));
        let pulse = red(122, Widen::Bits);
        assert_eq!((pulse(13), pulse(18)), (Some(102), Some(142)));
        assert_eq!(red(122, Widen::Seven)(13), Some(106));
        // Each channel takes its own colour.
        let colours = modulated_colours(&texture, 0, [0x80, 0x40, 0], Widen::Bits);
        assert_eq!(colours[20], Some([165, 0, 0]));
    }

    #[test]
    fn a_pose_is_a_frame_or_the_file_scaled_and_turned_as_asked() {
        let lua = mlua::Lua::new();
        let pose = |source: &str| {
            let table: Table = lua.load(source).eval().unwrap();
            character_pose(&character(), Some(table)).unwrap()
        };
        // With no pose the joints are unturned, the root where the
        // skeleton has it.
        let unposed = character_pose(&character(), None).unwrap().unwrap();
        assert_eq!(unposed.offset, [0, -100, 0]);
        assert_eq!(unposed.rotations, vec![anim::IDENTITY_ROTATION; 2]);

        let frame = pose("return { bank = 0, clip = 0, frame = 0 }").unwrap();
        assert_eq!(frame.offset, [1, 2, 3]);
        assert_eq!(
            frame.rotations,
            vec![[0x123, 0x456, 0x789], [0xABC, 0xDEF, 0x010]]
        );
        assert_eq!(
            pose("return { bank = 0, clip = 0, frame = 1 }")
                .unwrap()
                .offset,
            [0, 2, 3]
        );
        // The root's height alone is scaled, and floored.
        let short = pose("return { bank = 0, clip = 0, frame = 0, height = 0xE66 }").unwrap();
        assert_eq!(short.offset, [1, 1, 3]);
        assert_eq!(short.rotations, frame.rotations);
        // A turn adds to the angles of the joints it names; one the
        // skeleton lacks is nothing.
        let turned = pose(
            "return { bank = 0, clip = 0, frame = 0, turn = { [1] = { 0, -96, 1 }, [7] = { 1, 1, 1 } } }",
        )
        .unwrap();
        assert_eq!(
            turned.rotations,
            vec![[0x123, 0x456, 0x789], [0xABC, 0xDEF - 96, 0x011]]
        );
        for missing in [
            "return { bank = 0, clip = 0, frame = 2 }",
            "return { bank = 0, clip = 1, frame = 0 }",
            "return { bank = 1, clip = 0, frame = 0 }",
            "return { bank = -1, clip = 0, frame = 0 }",
        ] {
            assert_eq!(pose(missing), None, "{missing}");
        }
    }

    #[test]
    fn extra_parts_hang_unturned_on_the_parts_before_them() {
        let lua = mlua::Lua::new();
        let parts = |source: &str| lua.load(source).eval::<Option<Table>>().unwrap();
        let root = Mat4x3 {
            r: anim::rotation_matrix(0, 0x400, 0),
            t: [0, -100, 0],
        };
        let mut joints = vec![root];
        extra_parts(&mut joints, parts("return nil")).unwrap();
        assert_eq!(joints, [root]);
        // The second hangs on the first of them, whose place the root's
        // quarter turn has moved.
        extra_parts(
            &mut joints,
            parts("return { { 0, 10, 20, 30 }, { 1, 0, 5, 0 } }"),
        )
        .unwrap();
        let hung = anim::compose(
            &root,
            &Mat4x3 {
                r: anim::IDENTITY_MATRIX,
                t: [10, 20, 30],
            },
        );
        assert_eq!(joints[1], hung);
        assert_eq!(joints[1].r, root.r);
        assert_ne!(joints[1].t, [10, -80, 30]);
        assert_eq!(
            joints[2],
            anim::compose(
                &hung,
                &Mat4x3 {
                    r: anim::IDENTITY_MATRIX,
                    t: [0, 5, 0],
                },
            )
        );
        // A part can't hang on one that isn't there.
        assert!(extra_parts(&mut joints, parts("return { { 3, 0, 0, 0 } }")).is_err());
        assert_eq!(joints.len(), 3);
    }

    #[test]
    fn released_art_returns_its_count_and_bytes_to_the_budget() {
        let picture = || {
            Ok(Asset::Picture(Image {
                width: 2,
                height: 2,
                rgba: vec![0; 16],
            }))
        };
        let mut assets = Assets::default();
        for index in 0..budget::MAX_SCREEN_IMAGES {
            assert_eq!(
                assets.load(format!("art{index}"), picture).unwrap(),
                index + 1
            );
        }
        assert!(
            assets.load("over".into(), picture).is_err(),
            "the count is capped"
        );
        assert_eq!(assets.bytes, budget::MAX_SCREEN_IMAGES * 16);

        assets.hd.get_mut().insert((3, 0), None);
        assets.hd.get_mut().insert((4, 0), None);
        assets.release(3);
        assert!(assets.get(3).is_err(), "a released handle is invalid");
        assert_eq!(assets.bytes, (budget::MAX_SCREEN_IMAGES - 1) * 16);
        assert!(
            !assets.hd.get_mut().contains_key(&(3, 0)),
            "its HD lookup goes"
        );
        assert!(assets.hd.get_mut().contains_key(&(4, 0)));
        assets.release(3);
        assets.release(0);
        assets.release(budget::MAX_SCREEN_IMAGES + 1);
        assert_eq!(assets.live, budget::MAX_SCREEN_IMAGES - 1);
        // The released key loads afresh under a new handle; the stale one
        // stays invalid.
        let handle = budget::MAX_SCREEN_IMAGES + 1;
        assert_eq!(assets.load("art2".into(), picture).unwrap(), handle);
        assert!(assets.get(3).is_err());
        assert_eq!(
            assets.load("art2".into(), || unreachable!()).unwrap(),
            handle
        );
        assert!(assets.load("over".into(), picture).is_err());
    }

    #[test]
    fn the_epilogue_light_is_the_reference_record() {
        // `ending_epilogue_build`: `FUN_0040ac80(0, (100,100,100 | 0xFFFFFF))`
        // then `setBackColor(0xFFF, 0xFFF, 0xFFF)`. The direction is the
        // normalized position with its Y negated, the colour caps at 0x80 and
        // the 12-bit ambient converts with the reference's `* 255 / 4096`.
        let lua = mlua::Lua::new();
        let table: Table = lua
            .load(
                "return { ambient = 0xFFF, sources = { \
                 { pos = { 100, 100, 100 }, color = { 0xFF, 0xFF, 0xFF } } } }",
            )
            .eval()
            .unwrap();
        let light = light(&table).unwrap();
        assert_eq!(light.ambient, [254.0; 3]);
        assert_eq!(light.sources[0].dir, [2364, -2364, 2364]);
        assert_eq!(light.sources[0].color, [0x80; 3]);
        // The unset slots stay dark.
        assert!(light.sources[1..].iter().all(|s| s.color == [0; 3]));
    }

    #[test]
    fn a_joint_chain_starts_at_its_root_joint() {
        let lua = mlua::Lua::new();
        let joints: Table = lua
            .load("return { { rx = 0x95C, ty = 500 }, { rz = 0x120 } }")
            .eval()
            .unwrap();
        let root = Mat4x3 {
            r: anim::rotation_matrix(0x95C, 0, 0),
            t: [0, 500, 0],
        };
        let spin = Mat4x3 {
            r: anim::rotation_matrix(0, 0, 0x120),
            t: [0; 3],
        };
        let expected = anim::compose(&root, &spin);
        let actual = joint_matrix(Some(joints)).unwrap();
        assert_eq!((actual.r, actual.t), (expected.r, expected.t));
    }

    #[test]
    fn a_mirrored_sprite_quad_swaps_its_texture_coordinates() {
        let hd = Replacement {
            image: Image {
                width: 4,
                height: 4,
                rgba: vec![0; 64],
            },
            frame: [256, 128],
        };
        let (dst, uv) = sprite_quad(&hd, [64, 32, 64, 32], [10, 20, 128, 64], [false; 2]);
        assert_eq!(dst, [10.0, 20.0, 138.0, 84.0]);
        assert_eq!(uv, [0.25, 0.25, 0.5, 0.5]);
        let (_, uv) = sprite_quad(&hd, [64, 32, 64, 32], [10, 20, 128, 64], [true, false]);
        assert_eq!(uv, [0.5, 0.25, 0.25, 0.5]);
        let (_, uv) = sprite_quad(&hd, [64, 32, 64, 32], [10, 20, 128, 64], [false, true]);
        assert_eq!(uv, [0.25, 0.5, 0.5, 0.25]);
    }

    #[test]
    fn a_background_quad_centres_in_whole_pixels_like_the_software() {
        let hd = Replacement {
            image: Image {
                width: 4,
                height: 4,
                rgba: vec![0; 64],
            },
            frame: [320, 240],
        };
        let image = Image {
            width: 320,
            height: 240,
            rgba: Vec::new(),
        };
        // A 427-wide canvas centres the 320 columns 53 in, not 53.5.
        let framebuffer = Framebuffer::with_size(427, 240);
        let (dst, uv) = background_quad(&hd, &image, &framebuffer, [0, 0]);
        assert_eq!(dst, [0.0, 0.0, 427.0, 240.0]);
        assert_eq!(uv[0], -53.0 / 320.0);
        assert_eq!(uv[2], 374.0 / 320.0);
        assert_eq!((uv[1], uv[3]), (0.0, 1.0));
    }
}
