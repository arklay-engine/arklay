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
use crate::htexture::{self, Replacement};
use crate::ivm::Ivm;
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
}

impl Asset {
    fn size(&self) -> (u32, u32) {
        match self {
            Self::Picture(image) => (image.width, image.height),
            Self::Page(art) => (art.width(), art.height()),
            Self::Model(model, _) => (model.texture.width, model.texture.height),
        }
    }

    /// The bytes the asset holds, against the screen's art budget.
    fn bytes(&self) -> usize {
        let (width, height) = self.size();
        let pixels = (width as usize)
            .saturating_mul(height as usize)
            .saturating_mul(4);
        match self {
            Self::Model(_, file) => pixels.saturating_add(*file),
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
    /// The named HD keys (`htexture/<stem>`) a handle's art answers to, most
    /// specific first.
    hd_names: RefCell<HashMap<usize, Vec<String>>>,
    /// The texel indices of each indexed page a recoloured HD draw
    /// (`api:page_lut`) used, shared by its draws.
    indices: RefCell<HashMap<usize, Arc<htexture::Indices>>>,
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

    /// The handle `key`'s art was loaded under, if it is still live.
    fn handle(&self, key: &str) -> Option<usize> {
        self.by_key.get(key).copied()
    }

    /// The texel indices of indexed page `handle`, copied once.
    fn indices(&self, handle: usize) -> mlua::Result<Arc<htexture::Indices>> {
        if let Some(found) = self.indices.borrow().get(&handle) {
            return Ok(found.clone());
        }
        let Artwork::Indexed(texture) = self.page(handle)? else {
            return Err(mlua::Error::RuntimeError(format!(
                "art handle {handle} is not an indexed page"
            )));
        };
        let indices = Arc::new(htexture::Indices {
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

    fn picture(&self, handle: usize) -> mlua::Result<&Image> {
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
                .find_map(|name| htexture::named_replacement(pack, name, [size.0, size.1]))
        });
        let found = match named {
            Some(found) => Some(found),
            None => {
                // The frame is captured without an HD pack too (for the
                // viewer's model), so skip hashing the art when there is
                // nothing to find.
                let candidates = match asset {
                    _ if pack.htexture().is_empty() => Vec::new(),
                    Asset::Picture(image) | Asset::Page(Artwork::Rgba(image)) => {
                        htexture::image_candidates(image)
                    }
                    Asset::Page(Artwork::Indexed(texture)) => {
                        htexture::page_candidates(texture, row)
                    }
                    Asset::Model(model, _) => htexture::page_candidates(&model.texture, row),
                };
                if candidates.is_empty() {
                    None
                } else {
                    let (width, height) = size;
                    htexture::replacement(pack, &candidates, || {
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
            Ok(Asset::Picture(image)) if !pack.htexture().is_empty() => {
                crop(image, cell).and_then(|crop| {
                    let candidates = htexture::image_candidates(&crop);
                    let [x, y, w, h] = cell;
                    htexture::replacement(pack, &candidates, || {
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
            _ if pack.htexture().is_empty() => Vec::new(),
            Decoded::Rgba(image) => htexture::image_candidates(image),
            Decoded::Indexed(texture) => htexture::page_candidates(texture, row),
        };
        let found = if candidates.is_empty() {
            None
        } else {
            htexture::replacement(pack, &candidates, || format!("{entry} (row {row})"))
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

/// A `(handle)` or `(nil, message)` load result.
fn loaded(result: anyhow::Result<usize>) -> (Option<usize>, Option<String>) {
    match result {
        Ok(handle) => (Some(handle), None),
        Err(err) => (None, Some(format!("{err:#}"))),
    }
}

/// A clamped `[x, y, w, h]`.
/// The `page_lut` `blend` option.
fn lut_blend(name: Option<&str>) -> mlua::Result<SpriteBlend> {
    match name {
        None | Some("opaque") => Ok(SpriteBlend::Opaque),
        Some("half") => Ok(SpriteBlend::Half),
        Some("add") => Ok(SpriteBlend::Add),
        Some(other) => Err(mlua::Error::RuntimeError(format!(
            "unknown page_lut blend {other:?}"
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

/// A screen camera's projection offset: the rasterizer projects about the
/// 4:3 frame's centre, so a shorter layout band (a wider aspect) moves the
/// centre up by half the rows it lost, and a wider canvas moves it right by
/// half the columns it gained, keeping the model centred in the layout.
fn band_centre(framebuffer: &Framebuffer, screen: [i32; 2]) -> [i32; 2] {
    let gained = framebuffer.width as i32 - crate::render::WIDTH as i32;
    let lost = crate::render::HEIGHT as i32 - framebuffer.height as i32;
    [screen[0] + gained / 2, screen[1] - lost / 2]
}

#[cfg(test)]
mod tests {
    use super::*;

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
