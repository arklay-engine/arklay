//! The 40x30 inventory item icons for the Rust-drawn debug overlay. The Lua
//! screens draw their icons through the screen API instead.

use std::collections::HashMap;

use anyhow::Result;

use crate::assets::{Artwork, Geometry};
use crate::font::Tint;
use crate::item_art;
use crate::items;
use crate::pack::Pack;
use crate::render::Framebuffer;
use crate::state::Image;

/// `ui/staitem.tim`: the Ingram and Minimi icons.
const STAITEM_STEM: &str = "ui/staitem";

/// `item/item_all.bmp`: the 40x30 icon atlas (black keyed transparent).
/// Older packs that predate the per-item split only carry this sheet.
const ITEM_ALL_STEM: &str = "item/item_all";

/// The icon sheets and the per-item icons a converted pack carries.
pub struct ItemIcons {
    item_all: Image,
    staitem: Artwork,
    per_item: HashMap<u8, Image>,
}

impl ItemIcons {
    /// Load both icon sheets from `pack`, plus every per-item icon it has.
    pub fn load(pack: &Pack) -> Result<Self> {
        let mut per_item = HashMap::new();
        for item in items::art_items() {
            let Some(entry) = item_art::icon_entry(item) else {
                continue;
            };
            let stem = crate::assets::stem(&entry);
            if !pack.has_asset(stem, crate::pack::AssetClass::Image) {
                continue;
            }
            match super::load_picture(pack, stem, Geometry::Shipped, Some([0, 0, 0])) {
                Ok(image) => {
                    per_item.insert(item, image);
                }
                Err(err) => eprintln!("item icon {entry}: {err:#}"),
            }
        }
        Ok(Self {
            item_all: super::load_picture(pack, ITEM_ALL_STEM, Geometry::Shipped, Some([0, 0, 0]))?,
            staitem: super::load_page(pack, STAITEM_STEM)?,
            per_item,
        })
    }

    /// Draw `item`'s icon at `position`: its own `item/<id>.bmp` when the
    /// pack has one, else the `item/item_all.bmp` row for ids below `0x6F`
    /// and the `ui/staitem.tim` rows for the Ingram and Minimi.
    pub fn draw(&self, framebuffer: &mut Framebuffer, item: u8, position: [i32; 2]) {
        let dst = [position[0], position[1], 40, 30];
        if let Some(image) = self.per_item.get(&item) {
            framebuffer.draw_rgba_sprite(image, [0, 0, 40, 30], dst, 0);
            return;
        }
        if item < 0x6F {
            let Some(row) = items::icon_row(item) else {
                return;
            };
            let v = (usize::from(row) * items::ICON_HEIGHT as usize) as i32;
            framebuffer.draw_rgba_sprite(&self.item_all, [0, v, 40, 30], dst, 0);
        } else {
            let row = i32::from(item - 0x6F);
            framebuffer.draw_page_sprite(
                &self.staitem,
                [0, row * 30, 40, 30],
                dst,
                0,
                0,
                Tint::White,
            );
        }
    }
}
