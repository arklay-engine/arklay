//! The debug menu's pictures, loaded on first use and kept while the menu
//! lives: room thumbnails from each room's first camera cut, the item icons
//! (their HD replacements when the pack has them) and the status-screen
//! faces.

use std::collections::HashMap;
use std::sync::Arc;

use crate::assets::{self, Artwork, Geometry};
use crate::gui::canvas::Picture;
use crate::gui::save_screen;
use crate::htexture;
use crate::item_art;
use crate::items;
use crate::pack::{AssetClass, Pack};
use crate::state::{Image, RoomId};

/// The widest a room thumbnail is kept.
const THUMBNAIL_WIDTH: u32 = 320;

#[derive(Default)]
pub(super) struct Art {
    thumbnails: HashMap<RoomId, Option<Arc<Picture>>>,
    icons: HashMap<u8, Option<Arc<Picture>>>,
    faces: Option<Option<Image>>,
    portraits: HashMap<u8, Option<Arc<Picture>>>,
}

impl Art {
    /// Whether `room`'s thumbnail has been looked up.
    pub fn has_thumbnail(&self, room: RoomId) -> bool {
        self.thumbnails.contains_key(&room)
    }

    /// `room`'s first camera cut, scaled down; `None` until loaded or when
    /// the room has none.
    pub fn thumbnail(&self, room: RoomId) -> Option<Arc<Picture>> {
        self.thumbnails.get(&room).cloned().flatten()
    }

    /// Load `room`'s thumbnail.
    pub fn load_thumbnail(&mut self, pack: &Pack, room: RoomId) {
        let picture = (|| {
            let stem = room.cut_stem(0);
            let image = crate::ui::load_picture(pack, &stem, Geometry::Free, None).ok()?;
            Some(Arc::new(save_screen::downscale(
                opaque(image),
                THUMBNAIL_WIDTH,
            )))
        })();
        self.thumbnails.insert(room, picture);
    }

    /// `item`'s 40x30 icon: the named HD replacement, else the one its
    /// content hash names, else the pack's own.
    pub fn icon(&mut self, pack: &Pack, item: u8) -> Option<Arc<Picture>> {
        self.icons
            .entry(item)
            .or_insert_with(|| load_icon(pack, item).map(Arc::new))
            .clone()
    }

    /// Character `character`'s status-screen face (0 Chris, 1 Jill, 2 Barry,
    /// 3 Rebecca).
    pub fn portrait(&mut self, pack: &Pack, character: u8) -> Option<Arc<Picture>> {
        if let Some(face) = self.portraits.get(&character) {
            return face.clone();
        }
        let sheet = self
            .faces
            .get_or_insert_with(|| {
                crate::ui::load_page(pack, "ui/statface")
                    .ok()
                    .map(|page| match page {
                        Artwork::Indexed(texture) => assets::texture_to_rgba(&texture),
                        Artwork::Rgba(image) => image,
                    })
            })
            .as_ref();
        let face = sheet.and_then(|sheet| {
            let (x0, y0) = (u32::from(character & 1) * 32, u32::from(character & 2) * 16);
            let side = 32;
            if x0 + side > sheet.width || y0 + side > sheet.height {
                return None;
            }
            let mut rgba = Vec::with_capacity((side * side * 4) as usize);
            for y in y0..y0 + side {
                let at = ((y * sheet.width + x0) * 4) as usize;
                rgba.extend_from_slice(&sheet.rgba[at..at + (side * 4) as usize]);
            }
            Some(Arc::new(Picture {
                width: side,
                height: side,
                rgba,
            }))
        });
        self.portraits.insert(character, face.clone());
        face
    }
}

/// Load `item`'s icon, preferring an HD replacement.
fn load_icon(pack: &Pack, item: u8) -> Option<Picture> {
    let frame = [items::ICON_WIDTH, items::ICON_HEIGHT];
    for name in item_art::icon_hd_names(item) {
        if let Some(hd) = htexture::named_replacement(pack, &name, frame) {
            return Some(crop(&hd));
        }
    }
    let entry = item_art::icon_entry(item)?;
    let stem = assets::stem(&entry);
    if !pack.has_asset(stem, AssetClass::Image) {
        return None;
    }
    let image = crate::ui::load_picture(pack, stem, Geometry::Shipped, Some([0, 0, 0])).ok()?;
    let candidates = htexture::image_candidates(&image);
    if !candidates.is_empty()
        && let Some(hd) =
            htexture::replacement(pack, &candidates, || format!("item icon {item:02x}"))
    {
        return Some(crop(&hd));
    }
    Some(Picture {
        width: image.width,
        height: image.height,
        rgba: image.rgba,
    })
}

/// The part of an HD replacement that covers the 40x30 icon at its origin.
fn crop(hd: &htexture::Replacement) -> Picture {
    let image = &hd.image;
    let width = (image.width * items::ICON_WIDTH / hd.frame[0].max(1)).clamp(1, image.width);
    let height = (image.height * items::ICON_HEIGHT / hd.frame[1].max(1)).clamp(1, image.height);
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        let at = ((y * image.width) * 4) as usize;
        rgba.extend_from_slice(&image.rgba[at..at + (width * 4) as usize]);
    }
    Picture {
        width,
        height,
        rgba,
    }
}

/// A decoded picture made opaque.
fn opaque(image: Image) -> Picture {
    let mut rgba = image.rgba;
    for px in rgba.as_chunks_mut::<4>().0 {
        px[3] = 255;
    }
    Picture {
        width: image.width,
        height: image.height,
        rgba,
    }
}
