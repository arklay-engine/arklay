//! The 40x30 inventory item icons for the Rust-drawn debug overlay. The Lua
//! screens draw their icons through the screen API instead.

use std::collections::HashMap;

use anyhow::Result;

use crate::assets::Geometry;
use crate::item_art;
use crate::items;
use crate::pack::Pack;
use crate::render::Framebuffer;
use crate::state::Image;

/// Every item icon a converted pack carries: each item folder's `item.bmp`.
pub struct ItemIcons {
    per_item: HashMap<u8, Image>,
}

impl ItemIcons {
    /// Load every per-item icon `pack` has.
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
        Ok(Self { per_item })
    }

    /// Draw `item`'s icon at `position`; an item without one draws nothing.
    pub fn draw(&self, framebuffer: &mut Framebuffer, item: u8, position: [i32; 2]) {
        if let Some(image) = self.per_item.get(&item) {
            let dst = [position[0], position[1], 40, 30];
            framebuffer.draw_rgba_sprite(image, [0, 0, 40, 30], dst, 0);
        }
    }
}
