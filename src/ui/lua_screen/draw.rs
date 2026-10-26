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
use crate::ivm::{self, Ivm};
use crate::pack::{AssetClass, Pack};
use crate::render::{Camera, EntityLight, EntityLightSource, Framebuffer};
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
    /// The shared icon sheets, decoded once: the `ITEM_ALL` picture and the
    /// `STAITEM` page. An old pack has no per-item icons, so every item crops
    /// the same sheet instead of decoding it again.
    item_sources: RefCell<HashMap<String, Arc<Image>>>,
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

    /// The decoded shared icon sheet `stem` (`item/item_all` or
    /// `ui/staitem`), cached for the screen's lifetime.
    fn item_source(&self, pack: &Pack, stem: &str) -> anyhow::Result<Arc<Image>> {
        if let Some(image) = self.item_sources.borrow().get(stem) {
            return Ok(Arc::clone(image));
        }
        let image = if stem == "ui/staitem" {
            crate::ui::load_page(pack, stem)?.to_rgba()
        } else {
            crate::ui::load_picture(pack, stem, Geometry::Shipped, Some([0, 0, 0]))?
        };
        let image = Arc::new(image);
        self.item_sources
            .borrow_mut()
            .insert(stem.to_string(), Arc::clone(&image));
        Ok(image)
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

/// The 40x30 RGBA icon of `item`.
///
/// A pack converted since the per-item split has `item/<id>.bmp`; an older
/// pack, or an item the split skips, falls back to the shared `ITEM_ALL` row
/// (and `STAITEM` for the two PC-only machineguns) cut out here.
fn load_item_icon(item: u8, source: &Image) -> anyhow::Result<Image> {
    let y = if let Some(row) = crate::items::icon_row(item) {
        usize::from(row) * crate::items::ICON_HEIGHT as usize
    } else if matches!(item, crate::items::ITEM_INGRAM | crate::items::ITEM_MINIMI) {
        usize::from(item - crate::items::ITEM_INGRAM) * crate::items::ICON_HEIGHT as usize
    } else {
        anyhow::bail!("item {item:#04x} has no icon");
    };
    crop(
        source,
        [
            0,
            y as i32,
            crate::items::ICON_WIDTH as i32,
            crate::items::ICON_HEIGHT as i32,
        ],
    )
    .ok_or_else(|| anyhow::anyhow!("the shared icon sheet has no row at y={y}"))
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

/// Register the handle-based art and drawing methods.
pub(super) fn add_methods<'a, M: UserDataMethods<Api<'a>>>(methods: &mut M) {
    // ---- layout --------------------------------------------------------
    methods.add_method("width", |_lua, api, ()| Ok(api.host.layout.0));
    methods.add_method("height", |_lua, api, ()| Ok(api.host.layout.1));

    // ---- art -----------------------------------------------------------
    methods.add_method("has_image", |_lua, api, stem: String| {
        Ok(api.cx.pack.has_asset(&stem, AssetClass::Image))
    });

    methods.add_method_mut(
        "picture",
        |_lua, api, (stem, key, geometry): (String, Option<Value>, Option<String>)| {
            let key = match key {
                Some(Value::Nil) | None => None,
                Some(value) => Some(colour_rgb(&value)?),
            };
            // "shipped" pins the art to the original sheet's size; the
            // default lets a replacement be authored at any size.
            let geometry = match geometry.as_deref() {
                None | Some("free") => Geometry::Free,
                Some("shipped") => Geometry::Shipped,
                Some(other) => {
                    return Err(mlua::Error::RuntimeError(format!(
                        "unknown picture geometry {other:?}"
                    )));
                }
            };
            let pack: &Pack = api.cx.pack;
            let cache_key = format!("picture:{stem}:{key:?}:{geometry:?}");
            Ok(loaded(api.host.assets.load(cache_key, || {
                crate::ui::load_picture(pack, &stem, geometry, key).map(Asset::Picture)
            })))
        },
    );

    methods.add_method_mut("page", |_lua, api, stem: String| {
        let pack: &Pack = api.cx.pack;
        let cache_key = format!("page:{stem}");
        Ok(loaded(api.host.assets.load(cache_key, || {
            crate::ui::load_page(pack, &stem).map(Asset::Page)
        })))
    });

    // `item_icon(item)`: the item's 40x30 inventory icon as a picture handle.
    //
    // The per-item `item/<id>.bmp` wins; an older pack falls back to the
    // shared atlas row. The handle's HD names are registered so a named
    // `htexture/item/<id>` (or the older `htexture/data/item_all/<row>`)
    // replacement wins over the content hash while the frame is captured.
    methods.add_method_mut("item_icon", |_lua, api, item: u8| {
        let pack: &Pack = api.cx.pack;
        let source_stem = if crate::items::icon_row(item).is_some() {
            "item/item_all"
        } else if matches!(item, crate::items::ITEM_INGRAM | crate::items::ITEM_MINIMI) {
            "ui/staitem"
        } else {
            return Ok((None, Some(format!("item {item:#04x} has no icon"))));
        };
        let cache_key = format!("icon:{item:02x}");
        if let Some(handle) = api.host.assets.handle(&cache_key) {
            return Ok((Some(handle), None));
        }
        let icon = (|| -> anyhow::Result<Image> {
            if let Some(stem) = crate::item_art::icon_entry(item)
                .map(|entry| crate::assets::stem(&entry).to_string())
                .filter(|stem| pack.has_asset(stem, AssetClass::Image))
                // A corrupt per-item icon is an authoring error `verify`
                // reports; the inventory still shows the shared row instead
                // of a hole.
                && let Ok(image) =
                    crate::ui::load_picture(pack, &stem, Geometry::Shipped, Some([0, 0, 0]))
            {
                return Ok(image);
            }
            let source = api.host.assets.item_source(pack, source_stem)?;
            load_item_icon(item, &source)
        })();
        let result = match icon {
            Ok(image) => api
                .host
                .assets
                .load_icon(cache_key, || Ok(Asset::Picture(image))),
            Err(err) => Err(err),
        };
        if let Ok(handle) = result {
            api.host
                .assets
                .set_hd_names(handle, crate::item_art::icon_hd_names(item));
        }
        Ok(loaded(result))
    });

    // `indexed_page(stem)`: a page that is always indexed - a true-colour
    // replacement is re-indexed colour for colour - for the screens that
    // composite by texel index (`page_lut`).
    methods.add_method_mut("indexed_page", |_lua, api, stem: String| {
        let pack: &Pack = api.cx.pack;
        let cache_key = format!("indexed:{stem}");
        Ok(loaded(api.host.assets.load(cache_key, || {
            let asset = pack
                .read_asset(&stem, AssetClass::Image)?
                .ok_or_else(|| anyhow::anyhow!("missing {stem}"))?;
            crate::assets::decode_texture(&asset, Geometry::Shipped)
                .map(|texture| Asset::Page(Artwork::Indexed(texture)))
                .map_err(|err| err.context(format!("invalid {}", asset.path())))
        })))
    });

    methods.add_method_mut("ivm", |_lua, api, entry: String| {
        let pack: &Pack = api.cx.pack;
        let cache_key = format!("ivm:{entry}");
        let result = api.host.assets.load(cache_key, || {
            let bytes = pack.read(&entry)?;
            let model = ivm::parse(&bytes)?;
            Ok(Asset::Model(Arc::new(model), bytes.len()))
        });
        if let Ok(handle) = result
            && let Some(name) = entry
                .strip_prefix(crate::item_art::PREFIX)
                .and_then(|rest| rest.strip_suffix(".ivm"))
        {
            api.host
                .assets
                .set_hd_names(handle, crate::item_art::model_hd_names_for_name(name));
        }
        Ok(loaded(result))
    });

    // `item_model(item)`: the item viewer's model for `item`; nil and a
    // message when the item has none or its file is missing.
    //
    // A pack converted since the per-item split carries `item/<id>_model.tmd`
    // and `_model.bmp`; older packs fall back to the shared `.ivm`.
    methods.add_method_mut("item_model", |_lua, api, item: u8| {
        let pack: &Pack = api.cx.pack;
        let cache_key = format!("item-model:{item:02x}");
        let result = api.host.assets.load(cache_key, || {
            let Some(model) = crate::item_art::load_model(pack, item)? else {
                anyhow::bail!("item {item:#04x} has no model");
            };
            Ok(Asset::Model(model.ivm, model.bytes))
        });
        if let Ok(handle) = result {
            api.host
                .assets
                .set_hd_names(handle, crate::item_art::model_hd_names(item));
        }
        Ok(loaded(result))
    });

    // `model_objects(model)`: the number of objects a model carries.
    methods.add_method("model_objects", |_lua, api, handle: usize| {
        Ok(api.host.assets.model(handle)?.objects.len())
    });

    // `release(handle)`: drop art the screen no longer draws, so a screen
    // that pages through more art than the cache holds stays in budget.
    methods.add_method_mut("release", |_lua, api, handle: usize| {
        api.host.assets.release(handle);
        Ok(())
    });

    methods.add_method("art_size", |_lua, api, handle: usize| {
        Ok(api.host.assets.get(handle)?.size())
    });

    // `page_palette(page, row)`: an indexed page's CLUT row as a table of
    // `0xRRGGBB` colours keyed by texel index (`0..=255`); nil for an RGBA
    // page.
    methods.add_method(
        "page_palette",
        |lua, api, (handle, row): (usize, Option<usize>)| {
            let Artwork::Indexed(texture) = api.host.assets.page(handle)? else {
                return Ok(None);
            };
            let row = row.unwrap_or(0);
            let table = lua.create_table()?;
            for index in 0..=255u8 {
                let [r, g, b, _] = texture.palette(row, index);
                table.set(
                    index,
                    (i64::from(r) << 16) | (i64::from(g) << 8) | i64::from(b),
                )?;
            }
            Ok(Some(table))
        },
    );

    // ---- primitives ----------------------------------------------------
    methods.add_method_mut("clear", |_lua, api, ()| {
        api.framebuffer.as_deref_mut().ok_or_else(no_draw)?.clear();
        Ok(())
    });

    methods.add_method_mut(
        "fill_rect",
        |_lua, api, (x, y, w, h, r, g, b, a): (i32, i32, i32, i32, i64, i64, i64, Option<i64>)| {
            let framebuffer = api.framebuffer.as_deref_mut().ok_or_else(no_draw)?;
            framebuffer.fill_rect(
                rect(x, y, w, h),
                [byte(r), byte(g), byte(b), byte(a.unwrap_or(255))],
            );
            Ok(())
        },
    );

    // `line(x0, y0, x1, y1, r, g, b)`: a one-pixel line as the original's
    // GPU primitive draws it - a half-pixel quad, so the far end of each axis
    // is excluded, and each colour byte through the primitive's
    // `byte / 256 * 255` conversion (0xD8 draws as 0xD7).
    methods.add_method_mut(
        "line",
        |_lua, api, (x0, y0, x1, y1, r, g, b): (i32, i32, i32, i32, i64, i64, i64)| {
            let framebuffer = api.framebuffer.as_deref_mut().ok_or_else(no_draw)?;
            framebuffer.draw_line(
                [coord(x0), coord(y0)],
                [coord(x1), coord(y1)],
                [
                    line_colour(byte(r)),
                    line_colour(byte(g)),
                    line_colour(byte(b)),
                ],
            );
            Ok(())
        },
    );

    // `draw_ivm(model, {joints = {...}, objects = {{...}, ...}, camera =
    // {from, to, fov, screen = {x, y}}, light})`: object `i` takes the
    // joint chain `objects[i]`, and every object past the list the one
    // composed `joints` chain. `screen` offsets the projection centre,
    // which sits at the layout's centre (see [`band_centre`]).
    // While the GPU renderer draws the frame, the model renders on the GPU,
    // cut to the clip rect ([`Framebuffer::draw_ivm_captured`]).
    methods.add_method_mut("draw_ivm", |_lua, api, (handle, spec): (usize, Table)| {
        let Api {
            host, framebuffer, ..
        } = api;
        let framebuffer = framebuffer.as_deref_mut().ok_or_else(no_draw)?;
        let model = host.assets.model(handle)?;
        let joint = joint_matrix(spec.get("joints")?)?;
        let camera: Table = spec.get("camera")?;
        let screen = match camera.get::<Option<Table>>("screen")? {
            Some(offset) => [offset.get(1)?, offset.get(2)?],
            None => [0, 0],
        };
        let mut camera = Camera::from_points(
            triple(&camera, "from")?,
            triple(&camera, "to")?,
            field(&camera, "fov")?,
        );
        camera.screen = band_centre(framebuffer, screen);
        let light = match spec.get::<Option<Table>>("light")? {
            Some(table) => Some(light(&table)?),
            None => None,
        };
        let mut joints = vec![joint; model.objects.len()];
        if let Some(objects) = spec.get::<Option<Table>>("objects")? {
            for (slot, chain) in joints.iter_mut().zip(objects.sequence_values::<Table>()) {
                *slot = joint_matrix(Some(chain?))?;
            }
        }
        let page = hd_art(&host.assets, api.cx.pack, framebuffer, handle, 0);
        framebuffer.draw_ivm_captured(model, &joints, &camera, light.as_ref(), page);
        Ok(())
    });

    // `draw_actor(name, {from, to, fov})`: an engine-owned actor through
    // the screen's camera; false when the engine has no such actor.
    methods.add_method_mut(
        "draw_actor",
        |_lua, api, (name, camera): (String, Table)| {
            let mut camera = Camera::from_points(
                triple(&camera, "from")?,
                triple(&camera, "to")?,
                field(&camera, "fov")?,
            );
            let Api {
                framebuffer,
                actors,
                ..
            } = api;
            let framebuffer = framebuffer.as_deref_mut().ok_or_else(no_draw)?;
            camera.screen = band_centre(framebuffer, [0, 0]);
            Ok(actors
                .as_deref_mut()
                .is_some_and(|draw| draw(&name, &camera, framebuffer)))
        },
    );

    // `grid_text(x, y, scale, bytes)`: raw font-sheet cells, see
    // [`draw_grid_text`]. Without a font nothing draws.
    methods.add_method_mut(
        "grid_text",
        |_lua, api, (x, y, scale, text): (i32, i32, i64, mlua::String)| {
            let Some(font) = api.cx.font else {
                return Ok(());
            };
            let framebuffer = api.framebuffer.as_deref_mut().ok_or_else(no_draw)?;
            draw_grid_text(
                framebuffer,
                font,
                [coord(x), coord(y)],
                byte(scale),
                &text.as_bytes(),
            );
            Ok(())
        },
    );

    // `clip(x, y, w, h)`: clip the rest of this draw to a screen rect;
    // `clip()` lifts it.
    methods.add_method_mut(
        "clip",
        |_lua, api, (x, y, w, h): (Option<i32>, Option<i32>, Option<i32>, Option<i32>)| {
            let framebuffer = api.framebuffer.as_deref_mut().ok_or_else(no_draw)?;
            framebuffer.set_clip_rect(match (x, y, w, h) {
                (Some(x), Some(y), Some(w), Some(h)) => Some(rect(x, y, w, h)),
                _ => None,
            });
            Ok(())
        },
    );

    // `darken(alpha)`: blend the whole frame towards black by `alpha`/255
    // (the screens' black fade rect).
    methods.add_method_mut("darken", |_lua, api, alpha: i64| {
        let framebuffer = api.framebuffer.as_deref_mut().ok_or_else(no_draw)?;
        framebuffer.fade_to_black_weighted(byte(alpha));
        Ok(())
    });

    // `page_ramp(page, sx, sy, w, h, dx, dy, ramp)`: an unscaled page rect
    // modulated by `2 * ramp` and half-blended over the frame (the FILE
    // reader's backdrop pieces).
    methods.add_method_mut(
        "page_ramp",
        |_lua,
         api,
         (handle, sx, sy, w, h, dx, dy, ramp): (usize, i32, i32, i32, i32, i32, i32, i64)| {
            let Api {
                host, framebuffer, ..
            } = api;
            let framebuffer = framebuffer.as_deref_mut().ok_or_else(no_draw)?;
            let art = host.assets.page(handle)?;
            let (src, dst, ramp) = (rect(sx, sy, w, h), rect(dx, dy, w, h), byte(ramp));
            let draw = |framebuffer: &mut Framebuffer| {
                framebuffer.draw_page_sprite_ramp(art, src, dst, 0, ramp);
            };
            match hd_art(&host.assets, api.cx.pack, framebuffer, handle, 0) {
                Some(hd) => {
                    let (corners, uv) = sprite_quad(&hd, src, dst, [false; 2]);
                    // `2 * ramp`, unclamped like the software modulation.
                    let colour = [f32::from(ramp) * 2.0 / 255.0; 3];
                    framebuffer.draw_replaced(hd, corners, uv, colour, draw);
                }
                None => draw(framebuffer),
            }
            Ok(())
        },
    );

    methods.add_method_mut(
        "blend_black_rect",
        |_lua, api, (x, y, w, h, alpha): (i32, i32, i32, i32, i64)| {
            let framebuffer = api.framebuffer.as_deref_mut().ok_or_else(no_draw)?;
            framebuffer.blend_black_rect(rect(x, y, w, h), byte(alpha));
            Ok(())
        },
    );

    #[allow(clippy::type_complexity)]
    methods.add_method_mut(
        "picture_sprite_scaled",
        |_lua,
         api,
         (handle, sx, sy, sw, sh, dx, dy, dw, dh, scale, cell): (
            usize,
            i32,
            i32,
            i32,
            i32,
            i32,
            i32,
            i32,
            i32,
            i64,
            Option<Table>,
        )| {
            let Api {
                host, framebuffer, ..
            } = api;
            let framebuffer = framebuffer.as_deref_mut().ok_or_else(no_draw)?;
            let image = host.assets.picture(handle)?;
            let (src, dst, scale) = (rect(sx, sy, sw, sh), rect(dx, dy, dw, dh), byte(scale));
            let cell = match cell {
                Some(cell) => Some(rect(cell.get(1)?, cell.get(2)?, cell.get(3)?, cell.get(4)?)),
                None => None,
            };
            let draw = |framebuffer: &mut Framebuffer| {
                framebuffer.draw_rgba_sprite_scaled(image, src, dst, scale);
            };
            // A cell is its own upload: its replacement covers the cell, so
            // the source rect is taken relative to the cell's corner.
            let hd = match cell {
                Some(cell) if framebuffer.records() => host
                    .assets
                    .cell_replacement(api.cx.pack, handle, cell)
                    .map(|hd| (hd, [src[0] - cell[0], src[1] - cell[1], src[2], src[3]])),
                Some(_) => None,
                None => {
                    hd_art(&host.assets, api.cx.pack, framebuffer, handle, 0).map(|hd| (hd, src))
                }
            };
            match hd {
                Some((hd, src)) => {
                    let (corners, uv) = sprite_quad(&hd, src, dst, [false; 2]);
                    let colour = sprite_colour(Tint::White, scale);
                    framebuffer.draw_replaced(hd, corners, uv, colour, |framebuffer| {
                        // A cell is drawn as one quad, so its HD art covers
                        // the cell's keyed-clear texels too.
                        if cell.is_some() {
                            framebuffer.punch_rect(dst);
                        }
                        draw(framebuffer);
                    });
                }
                None => draw(framebuffer),
            }
            Ok(())
        },
    );

    methods.add_method_mut(
        "draw_background",
        |_lua, api, (handle, x, y, r, g, b): (usize, i32, i32, Option<i64>, Option<i64>, Option<i64>)| {
            let Api {
                host, framebuffer, ..
            } = api;
            let framebuffer = framebuffer.as_deref_mut().ok_or_else(no_draw)?;
            let image = host.assets.picture(handle)?;
            let colour = [
                byte(r.unwrap_or(255)),
                byte(g.unwrap_or(255)),
                byte(b.unwrap_or(255)),
            ];
            let origin = [coord(x), coord(y)];
            let draw = |framebuffer: &mut Framebuffer| {
                framebuffer.draw_background(image, origin, colour);
            };
            match hd_art(&host.assets, api.cx.pack, framebuffer, handle, 0) {
                Some(hd) => {
                    let (corners, uv) = background_quad(&hd, image, framebuffer, origin);
                    let colour = colour.map(|channel| f32::from(channel) / 255.0);
                    // A background ignores the clip, so its quad must too.
                    let clip = framebuffer.clip.take();
                    framebuffer.draw_replaced(hd, corners, uv, colour, draw);
                    framebuffer.clip = clip;
                }
                None => draw(framebuffer),
            }
            Ok(())
        },
    );

    // `page_lut(page, sx, sy, w, h, dx, dy, lut[, whole])`: an unscaled
    // indexed-page rect drawn through `lut`, a table of opaque `0xRRGGBB`
    // colours keyed by texel index; an index without an entry is transparent,
    // index 0 included. The screens composite remapped palettes with it (the
    // map's tinted and room-highlighted plans). With `whole`, a lookup that
    // tints every index alike lets an HD replacement cover the whole rect,
    // as `page_sprite_mirrored`'s does, for art the HD page redraws.
    #[allow(clippy::type_complexity)]
    methods.add_method_mut(
        "page_lut",
        |_lua,
         api,
         (handle, sx, sy, w, h, dx, dy, lut, whole): (
            usize,
            i32,
            i32,
            i32,
            i32,
            i32,
            i32,
            Table,
            Option<bool>,
        )| {
            let Api {
                host, framebuffer, ..
            } = api;
            let Artwork::Indexed(texture) = host.assets.page(handle)? else {
                return Err(mlua::Error::RuntimeError(format!(
                    "art handle {handle} is not an indexed page"
                )));
            };
            let mut colours = [None; 256];
            for (index, colour) in colours.iter_mut().enumerate() {
                if let Some(rgb) = lut.get::<Option<i64>>(index)? {
                    *colour = Some([(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8]);
                }
            }
            let framebuffer = framebuffer.as_deref_mut().ok_or_else(no_draw)?;
            let (src, dst) = (rect(sx, sy, w, h), rect(dx, dy, w, h));
            let draw = |framebuffer: &mut Framebuffer| {
                framebuffer.draw_indexed_sprite_lut(texture, src, dst, &colours);
            };
            // The HD page through CLUT row 0, recoloured on the GPU by the
            // original's indices, so the lookup applies at its resolution.
            match hd_art(&host.assets, api.cx.pack, framebuffer, handle, 0) {
                Some(art) => {
                    let (corners, uv) = sprite_quad(&art, src, dst, [false; 2]);
                    let palette = std::array::from_fn(|index| {
                        let [r, g, b, _] = texture.palette(0, index as u8);
                        [r, g, b]
                    });
                    let whole = match whole {
                        Some(true) => htexture::Recolour::whole(&palette, &colours),
                        _ => None,
                    };
                    let (lut, whole) = match whole {
                        Some(lut) => (lut, true),
                        None => (htexture::Recolour::lut(&palette, &colours), false),
                    };
                    let draw = |framebuffer: &mut Framebuffer| {
                        if whole {
                            framebuffer.punch_rect(dst);
                        }
                        draw(framebuffer);
                    };
                    let recoloured = htexture::Recoloured {
                        art,
                        indices: host.assets.indices(handle)?,
                        lut,
                    };
                    framebuffer.draw_replaced(
                        htexture::QuadImage::Recoloured(Arc::new(recoloured)),
                        corners,
                        uv,
                        [1.0; 3],
                        draw,
                    );
                }
                None => draw(framebuffer),
            }
            Ok(())
        },
    );

    // `page_sprite_mirrored(page, sx, sy, sw, sh, dx, dy, dw, dh, clut,
    // brightness, mirror_u, mirror_v[, whole])`: a sheet region flipped per
    // axis. With `whole`, an HD replacement covers the region's keyed-clear
    // texels too and its own alpha says what shows. That suits art the HD
    // page redraws rather than upscales: cut to the original's drawn texels,
    // it would come out in pieces.
    #[allow(clippy::type_complexity)]
    methods.add_method_mut(
        "page_sprite_mirrored",
        |_lua,
         api,
         (handle, sx, sy, sw, sh, dx, dy, dw, dh, clut, brightness, mirror_u, mirror_v, whole): (
            usize,
            i32,
            i32,
            i32,
            i32,
            i32,
            i32,
            i32,
            i32,
            usize,
            i64,
            bool,
            bool,
            Option<bool>,
        )| {
            let Api {
                host, framebuffer, ..
            } = api;
            let framebuffer = framebuffer.as_deref_mut().ok_or_else(no_draw)?;
            let art = host.assets.page(handle)?;
            let (src, dst, row) = (rect(sx, sy, sw, sh), rect(dx, dy, dw, dh), clut.min(255));
            let (brightness, mirror) = (byte(brightness), [mirror_u, mirror_v]);
            let draw = |framebuffer: &mut Framebuffer| {
                framebuffer.draw_page_sprite_mirrored(
                    art,
                    src,
                    dst,
                    row,
                    brightness,
                    Tint::White,
                    mirror,
                );
            };
            match hd_art(&host.assets, api.cx.pack, framebuffer, handle, row) {
                Some(hd) => {
                    let (corners, uv) = sprite_quad(&hd, src, dst, mirror);
                    let scale = crate::render::brightness_scale(brightness);
                    let colour = sprite_colour(Tint::White, scale);
                    framebuffer.draw_replaced(hd, corners, uv, colour, |framebuffer| {
                        if whole == Some(true) {
                            framebuffer.punch_rect(dst);
                        }
                        draw(framebuffer);
                    });
                }
                None => draw(framebuffer),
            }
            Ok(())
        },
    );

    #[allow(clippy::type_complexity)]
    methods.add_method_mut(
        "page_sprite_scaled",
        |_lua,
         api,
         (handle, sx, sy, sw, sh, dx, dy, dw, dh, clut, scale, tint): (
            usize,
            i32,
            i32,
            i32,
            i32,
            i32,
            i32,
            i32,
            i32,
            usize,
            i64,
            Option<Value>,
        )| {
            let tint = match tint {
                Some(Value::Nil) | None => crate::font::Tint::White,
                Some(value) => colour_tint(&value)?,
            };
            let Api {
                host, framebuffer, ..
            } = api;
            let framebuffer = framebuffer.as_deref_mut().ok_or_else(no_draw)?;
            let art = host.assets.page(handle)?;
            let (src, dst, row) = (rect(sx, sy, sw, sh), rect(dx, dy, dw, dh), clut.min(255));
            let scale = byte(scale);
            let draw = |framebuffer: &mut Framebuffer| {
                framebuffer.draw_page_sprite_scaled(art, src, dst, row, scale, tint);
            };
            match hd_art(&host.assets, api.cx.pack, framebuffer, handle, row) {
                Some(hd) => {
                    let (corners, uv) = sprite_quad(&hd, src, dst, [false; 2]);
                    let colour = sprite_colour(tint, scale);
                    framebuffer.draw_replaced(hd, corners, uv, colour, draw);
                }
                None => draw(framebuffer),
            }
            Ok(())
        },
    );
}

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
