//! Named HD replacements for a room's effect pages.
//!
//! A room's effect sprites have no pages of their own: the engine composites
//! each sprite's TIM onto the room's `effect/NN` sheet at the sprite's
//! `page_v` offset, plus one variant page per extra CLUT row, and the GPU
//! renderer resolves HD art by hashing that composite. A pack can also name
//! the pieces:
//!
//! - `htexture/room/<id>/effect/<slot>` covers sprite slot `<slot>`'s rows of
//!   its page under CLUT row 0, and `htexture/room/<id>/effect/<slot>_<row>`
//!   its rows of the variant page for CLUT row `<row>` (1 to 3);
//! - `htexture/effect/NN` covers the shipped sheet `effect/NN` under the
//!   sprites.
//!
//! [`composite`] builds the HD version of one page from those images, at the
//! largest scale among them. A page no name covers is left to the content
//! hash, so a pack made for the composite keeps resolving exactly as before.

use crate::htexture::{self, Replacement};
use crate::model::Texture8;
use crate::pack::{AssetClass, Pack};
use crate::state::{Image, RoomId};

/// The widest or tallest HD page [`composite`] builds, in texels.
const MAX_SIDE: u32 = 8192;

/// The named stem of sprite slot `slot`'s art under CLUT row `row`, relative
/// to [`htexture::ROOT`] (`room/1000/effect/00`, `room/1000/effect/00_1`).
pub fn sprite_stem(id: RoomId, slot: usize, row: u8) -> String {
    let stem = id.effect_stem(slot);
    if row == 0 {
        stem
    } else {
        format!("{stem}_{row}")
    }
}

/// The HD image a pack names under [`htexture::ROOT`] (`stem` is
/// `effect/1f`), decoded at its own size. A missing name is not reported;
/// one that fails to decode is, on stderr.
pub fn named_image(pack: &Pack, stem: &str) -> Option<Image> {
    let path = format!("{}{stem}", htexture::ROOT);
    let asset = pack.read_asset(&path, AssetClass::Image).ok().flatten()?;
    match htexture::load(pack, asset.path()) {
        Ok(image) if usable(&image) => Some(image),
        Ok(_) => None,
        Err(err) => {
            eprintln!("htexture: {err:#}");
            None
        }
    }
}

/// One sprite composited onto an effect page.
pub struct PageSprite<'a> {
    /// The sprite's art; texels with index zero show the page beneath.
    pub texture: &'a Texture8,
    /// The page row its top edge sits on.
    pub v: u32,
    /// The named HD image covering its rows, if the pack has one.
    pub hd: Option<Image>,
}

/// The HD version of the shipped composite `page`, or `None` when neither
/// `sheet` (the named HD sheet, for a base page) nor any of `sprites` has a
/// named image, which leaves the page to its content hash.
///
/// The page is drawn at the largest scale of its HD inputs. Its base is
/// `sheet` with the shipped texels of the unnamed sprites over it, else the
/// content-hash replacement of `page`, else `page` itself, stretched; each
/// named sprite image then replaces that sprite's rows.
pub fn composite(
    pack: &Pack,
    page: &Image,
    sheet: Option<&Image>,
    sprites: &[PageSprite<'_>],
) -> Option<Replacement> {
    if !usable(page) || (sheet.is_none() && sprites.iter().all(|sprite| sprite.hd.is_none())) {
        return None;
    }
    let hashed = if sheet.is_none() {
        hashed_replacement(pack, page)
    } else {
        None
    };

    let mut scale = [1.0f32, 1.0];
    let mut widen = |image: &Image, frame: [u32; 2]| {
        scale[0] = scale[0].max(image.width as f32 / frame[0].max(1) as f32);
        scale[1] = scale[1].max(image.height as f32 / frame[1].max(1) as f32);
    };
    if let Some(sheet) = sheet {
        widen(sheet, [page.width, page.height]);
    }
    if let Some((image, frame)) = &hashed {
        widen(image, *frame);
    }
    for sprite in sprites {
        if let Some(hd) = &sprite.hd {
            widen(hd, [sprite.texture.width, sprite.texture.height]);
        }
    }

    let mut canvas = Canvas::new(page, scale);
    let whole = [0, 0, page.width, page.height];
    match (sheet, &hashed) {
        (Some(sheet), _) => {
            canvas.cover(whole, sheet);
            for sprite in sprites.iter().filter(|sprite| sprite.hd.is_none()) {
                canvas.copy_texels(page, sprite.texture, sprite.v);
            }
        }
        (None, Some((image, frame))) => canvas.cover([0, 0, frame[0], frame[1]], image),
        (None, None) => canvas.cover(whole, page),
    }
    for sprite in sprites {
        if let Some(hd) = &sprite.hd {
            let rect = [0, sprite.v, sprite.texture.width, sprite.texture.height];
            canvas.cover(rect, hd);
        }
    }
    Some(Replacement {
        image: canvas.image,
        frame: [page.width, page.height],
    })
}

/// The content-hash replacement of `page` and the page texels it covers.
fn hashed_replacement(pack: &Pack, page: &Image) -> Option<(Image, [u32; 2])> {
    let found = pack
        .htexture()
        .find_quietly(&htexture::image_candidates(page))?;
    match htexture::load(pack, &found.path) {
        Ok(image) if usable(&image) => Some((image, found.frame)),
        Ok(_) => None,
        Err(err) => {
            eprintln!("htexture: {err:#}");
            None
        }
    }
}

/// Whether `image` has texels and a buffer that holds them.
fn usable(image: &Image) -> bool {
    image.width > 0
        && image.height > 0
        && image.rgba.len() >= (image.width as usize * image.height as usize * 4)
}

/// An HD page under construction, `scale` of its texels per page texel.
struct Canvas {
    image: Image,
    scale: [f32; 2],
    /// The page's size in its own texels.
    page: [u32; 2],
}

impl Canvas {
    fn new(page: &Image, scale: [f32; 2]) -> Self {
        let side = |texels: u32, scale: f32| {
            ((texels as f32 * scale).round() as u32).clamp(texels.min(MAX_SIDE), MAX_SIDE)
        };
        let width = side(page.width, scale[0]);
        let height = side(page.height, scale[1]);
        Self {
            image: Image {
                width,
                height,
                rgba: vec![0; width as usize * height as usize * 4],
            },
            scale: [
                width as f32 / page.width as f32,
                height as f32 / page.height as f32,
            ],
            page: [page.width, page.height],
        }
    }

    /// The canvas columns or rows covering page texels `start..end` on `axis`.
    fn span(&self, axis: usize, start: u32, end: u32) -> (u32, u32) {
        let limit = [self.image.width, self.image.height][axis];
        let at = |texel: u32| {
            ((texel.min(self.page[axis]) as f32 * self.scale[axis]).round() as u32).min(limit)
        };
        (at(start), at(end))
    }

    /// Replace page texels `rect` (`[x, y, width, height]`) with `art`,
    /// stretched over them nearest-neighbour, alpha and all. A `rect` past
    /// the page is cut at its edge, and `art` with it.
    fn cover(&mut self, rect: [u32; 4], art: &Image) {
        let [x, y, width, height] = rect;
        if width == 0 || height == 0 || !usable(art) {
            return;
        }
        let (x0, x1) = self.span(0, x, x + width);
        let (y0, _) = self.span(1, y, y);
        let (_, y1) = self.span(1, y, y + height);
        // The canvas extent of the whole rectangle, before the cut.
        let full = [width as f32 * self.scale[0], height as f32 * self.scale[1]];
        for cy in y0..y1 {
            let sy = (((cy - y0) as f32 + 0.5) * art.height as f32 / full[1]) as u32;
            let sy = sy.min(art.height - 1);
            for cx in x0..x1 {
                let sx = (((cx - x0) as f32 + 0.5) * art.width as f32 / full[0]) as u32;
                let sx = sx.min(art.width - 1);
                let from = ((sy * art.width + sx) * 4) as usize;
                let to = ((cy * self.image.width + cx) * 4) as usize;
                self.image.rgba[to..to + 4].copy_from_slice(&art.rgba[from..from + 4]);
            }
        }
    }

    /// Paint, from `page`, every texel the non-zero indices of `texture`
    /// cover with its top edge on page row `v`.
    fn copy_texels(&mut self, page: &Image, texture: &Texture8, v: u32) {
        let width = texture.width as usize;
        for y in 0..texture.height {
            let page_y = v + y;
            if page_y >= page.height {
                break;
            }
            for x in 0..texture.width.min(page.width) {
                if texture
                    .indices
                    .get(y as usize * width + x as usize)
                    .copied()
                    .unwrap_or(0)
                    == 0
                {
                    continue;
                }
                let from = ((page_y * page.width + x) * 4) as usize;
                let Some(texel) = page.rgba.get(from..from + 4) else {
                    continue;
                };
                let (x0, x1) = self.span(0, x, x + 1);
                let (y0, y1) = self.span(1, page_y, page_y + 1);
                for cy in y0..y1 {
                    for cx in x0..x1 {
                        let to = ((cy * self.image.width + cx) * 4) as usize;
                        self.image.rgba[to..to + 4].copy_from_slice(texel);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: u32, height: u32, rgba: [u8; 4]) -> Image {
        Image {
            width,
            height,
            rgba: rgba.repeat((width * height) as usize),
        }
    }

    fn texel(image: &Image, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * image.width + x) * 4) as usize;
        image.rgba[at..at + 4].try_into().unwrap()
    }

    fn sprite_texture(width: u32, height: u32) -> Texture8 {
        Texture8 {
            width,
            height,
            indices: vec![1; (width * height) as usize],
            palettes: Vec::new(),
            stp: Vec::new(),
        }
    }

    fn empty_pack() -> Pack {
        let writer = crate::pack::PackWriter::new();
        Pack::from_bytes(writer.to_bytes().unwrap()).unwrap()
    }

    #[test]
    fn a_page_without_named_art_is_left_to_its_hash() {
        let pack = empty_pack();
        let page = solid(256, 256, [9, 9, 9, 255]);
        let texture = sprite_texture(16, 8);
        let sprites = [PageSprite {
            texture: &texture,
            v: 32,
            hd: None,
        }];
        assert!(composite(&pack, &page, None, &sprites).is_none());
    }

    #[test]
    fn a_named_sprite_covers_its_rows_at_its_own_scale() {
        let pack = empty_pack();
        let page = solid(256, 256, [9, 9, 9, 255]);
        let texture = sprite_texture(16, 8);
        let sprites = [PageSprite {
            texture: &texture,
            v: 32,
            hd: Some(solid(64, 32, [200, 10, 10, 128])),
        }];
        let hd = composite(&pack, &page, None, &sprites).unwrap();
        assert_eq!(hd.frame, [256, 256]);
        assert_eq!((hd.image.width, hd.image.height), (1024, 1024));
        // The sprite's rows are the named image, alpha included.
        assert_eq!(texel(&hd.image, 0, 128), [200, 10, 10, 128]);
        assert_eq!(texel(&hd.image, 63, 159), [200, 10, 10, 128]);
        // Outside them the shipped page shows, stretched.
        assert_eq!(texel(&hd.image, 64, 128), [9, 9, 9, 255]);
        assert_eq!(texel(&hd.image, 0, 160), [9, 9, 9, 255]);
        assert_eq!(texel(&hd.image, 0, 127), [9, 9, 9, 255]);
    }

    #[test]
    fn a_named_sheet_keeps_the_unnamed_sprites_shipped_texels() {
        let pack = empty_pack();
        let mut page = solid(256, 256, [9, 9, 9, 255]);
        // The unnamed sprite's shipped texels on the composite.
        for y in 0..4 {
            for x in 0..8 {
                let at = (((10 + y) * 256 + x) * 4) as usize;
                page.rgba[at..at + 4].copy_from_slice(&[1, 2, 3, 255]);
            }
        }
        let mut unnamed = sprite_texture(8, 4);
        unnamed.indices[0] = 0;
        let sheet = solid(512, 512, [50, 60, 70, 255]);
        let sprites = [PageSprite {
            texture: &unnamed,
            v: 10,
            hd: None,
        }];
        let hd = composite(&pack, &page, Some(&sheet), &sprites).unwrap();
        assert_eq!((hd.image.width, hd.image.height), (512, 512));
        assert_eq!(texel(&hd.image, 100, 100), [50, 60, 70, 255]);
        assert_eq!(texel(&hd.image, 2, 20), [1, 2, 3, 255]);
        assert_eq!(texel(&hd.image, 15, 27), [1, 2, 3, 255]);
        // Index zero shows the sheet.
        assert_eq!(texel(&hd.image, 0, 20), [50, 60, 70, 255]);
    }

    #[test]
    fn a_sprite_past_the_page_bottom_is_cut() {
        let pack = empty_pack();
        let page = solid(256, 256, [9, 9, 9, 255]);
        let texture = sprite_texture(8, 16);
        let mut art = solid(16, 32, [1, 1, 1, 255]);
        // The bottom half, below the page, is a different colour.
        for at in (16 * 16 * 4)..art.rgba.len() {
            art.rgba[at] = 2;
        }
        let sprites = [PageSprite {
            texture: &texture,
            v: 248,
            hd: Some(art),
        }];
        let hd = composite(&pack, &page, None, &sprites).unwrap();
        assert_eq!(hd.image.height, 512);
        assert_eq!(texel(&hd.image, 0, 496), [1, 1, 1, 255]);
        assert_eq!(texel(&hd.image, 0, 511), [1, 1, 1, 255]);
    }

    #[test]
    fn sprite_stems_name_the_slot_and_row() {
        let id = RoomId::parse("1000").unwrap();
        assert_eq!(sprite_stem(id, 0, 0), "room/1000/effect/00");
        assert_eq!(sprite_stem(id, 2, 1), "room/1000/effect/02_1");
    }

    #[test]
    fn a_named_image_resolves_by_its_stem_in_any_format() {
        let mut writer = crate::pack::PackWriter::new();
        let bmp = crate::bmp::encode_to_vec(&solid(4, 2, [7, 7, 7, 255])).unwrap();
        writer.add("htexture/room/1000/effect/00.bmp", bmp).unwrap();
        let pack = Pack::from_bytes(writer.to_bytes().unwrap()).unwrap();
        let image = named_image(&pack, "room/1000/effect/00").unwrap();
        assert_eq!((image.width, image.height), (4, 2));
        assert!(named_image(&pack, "room/1000/effect/01").is_none());
    }
}
