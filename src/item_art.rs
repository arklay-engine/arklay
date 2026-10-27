//! Per-item inventory art: the icon, the item-view model and its texture.
//!
//! The original keys inventory art by *image type*: one row of
//! `DATA/ITEM_ALL.PIX` and one `ITEM_M2/I<imageType>V.IVM` serve every item
//! that shares a class. A pack built by `convert-game` stores the art in each
//! item's own folder instead, so an item's icon and model can diverge from
//! its class and the folder can be copied to another id or pack whole:
//!
//! ```text
//! item/<id>/item.bmp     the 40x30 inventory icon
//! item/<id>/model.tmd    the inventory 3D model
//! item/<id>/model.bmp    its 256x256 indexed texture page
//! ```
//!
//! `<id>` is the two-digit lower-case hex item id (`0b`, `6f`). The files are
//! optional: an item without `item.bmp` draws no icon, a `model.tmd` without
//! its `model.bmp` is an untextured model, and an item without `model.tmd`
//! has no model.
//!
//! HD replacements keep working through [`crate::htexture`]:
//!
//! - `htexture/item/<id>/item` and `htexture/item/<id>/model` are the named
//!   per-item keys.
//! - `htexture/data/item_all/<imageType - 1:02>` and
//!   `htexture/data/item_m2/<model name>` are the legacy CR file names, so an
//!   existing HD pack keeps replacing the shared art.
//! - the content hashes of the icon cell and of the model page remain the
//!   last route.

use std::sync::Arc;

use anyhow::{Context, Result, bail};

use crate::bmp;
use crate::items;
use crate::ivm::{self, Ivm};
use crate::model::Texture8;
use crate::pack::Pack;

/// Pack directory of the per-item folders.
pub const PREFIX: &str = "item/";
/// Number of bytes in one 40x30 8bpp icon.
pub const ICON_BYTES: usize = (items::ICON_WIDTH * items::ICON_HEIGHT) as usize;

/// The pack folder of `item`, with its trailing slash: `item/0b/`.
pub fn folder(item: u8) -> String {
    format!("{PREFIX}{item:02x}/")
}

/// The pack entry of `item`'s own icon, when it has one.
pub fn icon_entry(item: u8) -> Option<String> {
    (items::icon_row(item).is_some() || matches!(item, items::ITEM_INGRAM | items::ITEM_MINIMI))
        .then(|| format!("{}item.bmp", folder(item)))
}

/// The pack entry of `item`'s own 3D model, when its class has a model.
pub fn model_tmd_entry(item: u8) -> Option<String> {
    items::model_name(item).map(|_| format!("{}model.tmd", folder(item)))
}

/// The pack entry of `item`'s own model texture.
pub fn model_texture_entry(item: u8) -> Option<String> {
    items::model_name(item).map(|_| format!("{}model.bmp", folder(item)))
}

/// The named HD keys that replace `item`'s icon, most specific first.
///
/// The legacy key is the CR inventory path for the item's image type, so an
/// HD pack made for the shared atlas still overlays the right icon.
pub fn icon_hd_names(item: u8) -> Vec<String> {
    let mut names = Vec::with_capacity(2);
    if icon_entry(item).is_some() {
        names.push(format!("{}item", folder(item)));
    }
    if let Some(row) = items::icon_row(item) {
        names.push(format!("data/item_all/{row:02x}"));
    }
    names
}

/// The named HD keys that replace `item`'s model texture, most specific
/// first: the per-item texture, then the legacy `ITEM_M2` TIM name.
pub fn model_hd_names(item: u8) -> Vec<String> {
    let Some(name) = items::model_name(item) else {
        return Vec::new();
    };
    vec![
        format!("{}model", folder(item)),
        format!("data/item_m2/{}", name.to_ascii_lowercase()),
    ]
}

/// An item-view model loaded from a pack and where its bytes came from.
#[derive(Debug, Clone)]
pub struct LoadedModel {
    /// The parsed model.
    pub ivm: Arc<Ivm>,
    /// Bytes the pack entries held.
    pub bytes: usize,
    /// Whether the TMD had no per-item texture and got the blank page.
    pub untextured: bool,
}

/// Load `item`'s viewer model from its folder's `model.tmd` and `model.bmp`.
///
/// `Ok(None)` means the item has no model (or the pack has no `model.tmd`),
/// not that a file is corrupt.
pub fn load_model(pack: &Pack, item: u8) -> Result<Option<LoadedModel>> {
    let (Some(tmd_entry), Some(texture_entry)) = (model_tmd_entry(item), model_texture_entry(item))
    else {
        return Ok(None);
    };
    if !pack.contains(&tmd_entry) {
        return Ok(None);
    }
    let tmd = pack
        .read(&tmd_entry)
        .with_context(|| format!("failed to read {tmd_entry}"))?;
    let (texture, untextured, mut bytes) = if pack.contains(&texture_entry) {
        let data = pack
            .read(&texture_entry)
            .with_context(|| format!("failed to read {texture_entry}"))?;
        (
            bmp::decode_indexed_page(&data).with_context(|| format!("invalid {texture_entry}"))?,
            false,
            data.len(),
        )
    } else {
        (ivm::blank_texture(), true, 0)
    };
    bytes += tmd.len();
    let ivm = ivm::from_parts(texture, &tmd)
        .with_context(|| format!("invalid model for item {item:#04x}"))?;
    Ok(Some(LoadedModel {
        ivm: Arc::new(ivm),
        bytes,
        untextured,
    }))
}

/// Bake one 40x30 icon row of `ITEM_ALL.PIX` through a STATUS.TIM CLUT row.
///
/// `row` is zero-based. Index 0 is the BMP's transparent magenta entry, cut
/// out by [`crate::ui::load_picture`].
pub fn bake_icon(pix: &[u8], row: usize, palette: &Texture8) -> Result<Vec<u8>> {
    let start = row
        .checked_mul(ICON_BYTES)
        .context("item icon row overflows")?;
    let end = start
        .checked_add(ICON_BYTES)
        .context("item icon end overflows")?;
    let pixels = pix
        .get(start..end)
        .with_context(|| format!("item sheet has no row {row}"))?;
    if palette.palettes.len() < 3 * crate::model::PALETTE_ROW_LEN {
        bail!("item icon palette has no third CLUT row");
    }
    let texture = icon_texture(pixels.to_vec(), palette, 2);
    bmp::encode_indexed_page(&texture, 0).context("failed to encode an item icon as BMP")
}

/// Bake one 40x30 row of an already-decoded icon page (`STAITEM.TIM`) through
/// its first CLUT row.
///
/// `row` is zero-based and `page_y` is the row's first scanline. Index 0 is
/// the BMP's transparent magenta entry, cut out by the inventory loader.
pub fn bake_page_icon(page: &Texture8, page_y: usize) -> Result<Vec<u8>> {
    let width = items::ICON_WIDTH as usize;
    let height = items::ICON_HEIGHT as usize;
    let page_width = page.width as usize;
    if page_width < width || page_y.saturating_add(height) > page.height as usize {
        bail!(
            "icon page is {}x{}; a {width}x{height} icon at y={page_y} does not fit",
            page.width,
            page.height
        );
    }
    let needed = page_width
        .checked_mul(page.height as usize)
        .context("icon page size overflows")?;
    if page.indices.len() < needed {
        bail!(
            "icon page has {} indices but {}x{} needs {needed}",
            page.indices.len(),
            page.width,
            page.height
        );
    }
    if page.palettes.len() < crate::model::PALETTE_ROW_LEN {
        bail!("icon page has no full CLUT row");
    }
    let mut indices = Vec::with_capacity(width * height);
    for y in page_y..page_y + height {
        for x in 0..width {
            indices.push(page.indices[y * page_width + x]);
        }
    }
    let texture = icon_texture(indices, page, 0);
    bmp::encode_indexed_page(&texture, 0).context("failed to encode an item icon as BMP")
}

/// A 40x30 indexed icon page through `palette`'s `row`, with index 0 forced
/// to black so the BMP writes it as the transparent magenta entry.
fn icon_texture(indices: Vec<u8>, palette: &Texture8, row: usize) -> Texture8 {
    let mut palettes = Vec::with_capacity(crate::model::PALETTE_ROW_LEN);
    let mut stp = Vec::with_capacity(crate::model::PALETTE_ROW_LEN);
    for index in 0..=255u8 {
        if index == 0 {
            palettes.push([0, 0, 0, 255]);
            stp.push(false);
        } else {
            palettes.push(palette.palette(row, index));
            stp.push(palette.palette_stp(row, index));
        }
    }
    Texture8 {
        width: items::ICON_WIDTH,
        height: items::ICON_HEIGHT,
        indices,
        palettes,
        stp,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::PackWriter;
    use crate::tim;

    fn pack(entries: &[(&str, Vec<u8>)]) -> Pack {
        let mut writer = PackWriter::new();
        for (path, data) in entries {
            writer.add(path, data.clone()).unwrap();
        }
        Pack::from_bytes(writer.to_bytes().unwrap()).unwrap()
    }

    fn sample_model() -> (Vec<u8>, Vec<u8>) {
        let ivm = crate::ivm::sample_ivm();
        let (texture, tmd) = crate::ivm::split(&ivm).unwrap();
        let page = crate::bmp::encode_indexed_page(&tim::decode_8bpp(texture).unwrap(), 0).unwrap();
        (tmd.to_vec(), page)
    }

    #[test]
    fn entry_names_cover_shared_types_missing_art_and_the_pc_only_ids() {
        // Beretta and the shared cluster ids all use image class 2, but each
        // gets its own folder.
        assert_eq!(items::image_class(0x02), 2);
        assert_eq!(items::image_class(0x5F), 2);
        assert_eq!(folder(0x0b), "item/0b/");
        assert_eq!(icon_entry(0x02).as_deref(), Some("item/02/item.bmp"));
        assert_eq!(icon_entry(0x5F).as_deref(), Some("item/5f/item.bmp"));
        assert_eq!(model_tmd_entry(0x02).as_deref(), Some("item/02/model.tmd"));
        assert_eq!(
            model_texture_entry(0x02).as_deref(),
            Some("item/02/model.bmp")
        );
        assert_eq!(
            icon_hd_names(0x02),
            vec!["item/02/item".to_string(), "data/item_all/01".to_string()]
        );
        assert_eq!(
            model_hd_names(0x02),
            vec!["item/02/model".to_string(), "data/item_m2/i00v".to_string()]
        );

        // A model-only item has no atlas row but keeps its model.
        assert_eq!(items::icon_row(0x4C), None);
        assert_eq!(icon_entry(0x4C), None);
        assert_eq!(model_tmd_entry(0x4C).as_deref(), Some("item/4c/model.tmd"));

        // Class 0 has no icon and no model.
        assert_eq!(icon_entry(0x6E), None);
        assert_eq!(model_tmd_entry(0x6E), None);
        assert!(icon_hd_names(0x6E).is_empty());
        assert!(model_hd_names(0x6E).is_empty());

        // The PC-only machineguns have both.
        assert_eq!(
            icon_entry(items::ITEM_INGRAM).as_deref(),
            Some("item/6f/item.bmp")
        );
        assert_eq!(
            model_tmd_entry(items::ITEM_MINIMI).as_deref(),
            Some("item/70/model.tmd")
        );
        assert_eq!(items::model_name(items::ITEM_MINIMI), Some("MINI"));
    }

    #[test]
    fn loads_a_split_model_and_preserves_the_texture() {
        let (tmd, page) = sample_model();
        let pack = pack(&[("item/02/model.tmd", tmd), ("item/02/model.bmp", page)]);
        let loaded = load_model(&pack, 0x02).unwrap().unwrap();
        assert!(!loaded.untextured);
        let expected = crate::ivm::parse(&crate::ivm::sample_ivm()).unwrap();
        assert_eq!(loaded.ivm.texture, expected.texture);
        assert_eq!(loaded.ivm.objects, expected.objects);
    }

    #[test]
    fn a_tmd_only_model_gets_the_blank_page() {
        let (tmd, _) = sample_model();
        let pack = pack(&[("item/01/model.tmd", tmd)]);
        let loaded = load_model(&pack, 0x01).unwrap().unwrap();
        assert!(loaded.untextured);
        assert_eq!(
            (loaded.ivm.texture.width, loaded.ivm.texture.height),
            (256, 256)
        );
        assert!(loaded.ivm.texture.indices.iter().all(|&index| index == 1));
        assert!(!loaded.ivm.objects.is_empty());
    }

    #[test]
    fn a_present_but_invalid_page_is_an_error_not_an_untextured_model() {
        let (tmd, _) = sample_model();
        let invalid = pack(&[
            ("item/02/model.tmd", tmd),
            ("item/02/model.bmp", b"not a bmp".to_vec()),
        ]);
        assert!(load_model(&invalid, 0x02).is_err());
        // A missing page, by contrast, is the untextured path.
        let missing = pack(&[("item/02/model.tmd", sample_model().0)]);
        assert!(load_model(&missing, 0x02).unwrap().unwrap().untextured);
    }

    #[test]
    fn an_item_without_a_model_file_has_no_model() {
        // A raw shared `.ivm` is not read: only the item's own folder counts.
        let pack = pack(&[("item/i00v.ivm", crate::ivm::sample_ivm())]);
        assert!(load_model(&pack, 0x02).unwrap().is_none());
        // A class-0 item never resolves the placeholder "m".
        assert!(load_model(&pack, 0x6E).unwrap().is_none());
    }

    #[test]
    fn bakes_the_pc_only_icons_from_the_staitem_page() {
        let mut palettes = vec![[0u8; 4]; crate::model::PALETTE_ROW_LEN];
        palettes[1] = [0, 0, 255, 255];
        palettes[2] = [0, 255, 0, 255];
        let mut indices = vec![0u8; 64 * 64];
        // The second 40x30 row, first two texels.
        indices[30 * 64] = 1;
        indices[30 * 64 + 1] = 2;
        let page = Texture8 {
            width: 64,
            height: 64,
            indices,
            palettes,
            stp: vec![false; crate::model::PALETTE_ROW_LEN],
        };
        let data = bake_page_icon(&page, 30).unwrap();
        let icon = crate::bmp::decode(&data).unwrap();
        assert_eq!((icon.width, icon.height), (40, 30));
        assert_eq!(&icon.rgba[0..4], &[0, 0, 255, 255]);
        assert_eq!(&icon.rgba[4..8], &[0, 255, 0, 255]);
        assert_eq!(
            &icon.rgba[8..12],
            &[0, 0, 0, 0],
            "index 0 is the magenta key"
        );
    }

    #[test]
    fn a_page_without_a_full_icon_row_is_refused() {
        let page = Texture8 {
            width: 64,
            height: 30,
            indices: vec![0; 64 * 30],
            palettes: vec![[0u8; 4]; crate::model::PALETTE_ROW_LEN],
            stp: vec![false; crate::model::PALETTE_ROW_LEN],
        };
        assert!(bake_page_icon(&page, 0).is_ok());
        assert!(bake_page_icon(&page, 1).is_err());
        let short = Texture8 {
            width: 64,
            height: 64,
            indices: vec![0; 64 * 63],
            palettes: vec![[0u8; 4]; crate::model::PALETTE_ROW_LEN],
            stp: vec![false; crate::model::PALETTE_ROW_LEN],
        };
        assert!(bake_page_icon(&short, 0).is_err());
    }

    #[test]
    fn bakes_a_black_keyed_icon_from_the_atlas_row() {
        let mut palettes = vec![[0u8; 4]; crate::model::PALETTE_ROW_LEN * 3];
        palettes[2 * crate::model::PALETTE_ROW_LEN + 1] = [255, 0, 0, 255];
        palettes[2 * crate::model::PALETTE_ROW_LEN + 2] = [0, 255, 0, 255];
        let palette = Texture8 {
            width: 256,
            height: 1,
            indices: vec![0; 256],
            palettes,
            stp: vec![false; crate::model::PALETTE_ROW_LEN * 3],
        };
        let mut pix = vec![0u8; ICON_BYTES * 2];
        pix[ICON_BYTES] = 1;
        pix[ICON_BYTES + 1] = 2;
        let data = bake_icon(&pix, 1, &palette).unwrap();
        let icon = crate::bmp::decode(&data).unwrap();
        assert_eq!((icon.width, icon.height), (40, 30));
        // The BMP keeps the PIX indices and puts the transparent entry at
        // palette index 0.
        let indexed = crate::bmp::decode_indexed_page(&data).unwrap();
        assert_eq!(&indexed.indices[..3], &[1, 2, 0]);
        assert_eq!(indexed.palette(0, 0), [0, 0, 0, 255]);
        assert_eq!(&icon.rgba[0..4], &[255, 0, 0, 255]);
        assert_eq!(&icon.rgba[4..8], &[0, 255, 0, 255]);
        assert_eq!(
            &icon.rgba[8..12],
            &[0, 0, 0, 0],
            "index 0 is the magenta key"
        );
        // The atlas has two rows in this fixture.
        assert!(bake_icon(&pix, 2, &palette).is_err());
    }
}
