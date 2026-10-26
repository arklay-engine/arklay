//! Real-asset inventory/status screen tests.
//!
//! Run with:
//! `ARKLAY_RE1_ROOT=assets/re1 ARKLAY_RE1_PACK=re1.akpak cargo test --profile test-release --test real inventory_real:: -- --ignored --nocapture`
//!
//! Both `ARKLAY_RE1_ROOT` and `ARKLAY_RE1_PACK` must be set together; with
//! neither the tests skip, and with only one they fail. The pack must contain
//! the menu art (`ui/status.tim`, `ui/blue.tim`, `ui/statface.tim`,
//! `ui/staitem.tim`, `item/item_all.bmp`, `font/font.tim`); a stale pack fails
//! the test rather than silently skipping.
//!
//! The test renders the `--ui menu` capture (`ui/main_menu.lua` over the
//! room boot's inventory) twice and requires it to be deterministic and to
//! draw the panel art. The menu's rules are covered by the screen tests over
//! the checked-in script.

use crate::common;

use arklay::assets::Geometry;
use arklay::bmp;
use arklay::engine;
use arklay::items;
use arklay::pack::Pack;
use arklay::ui;

#[test]
#[ignore = "requires both ARKLAY_RE1_ROOT and ARKLAY_RE1_PACK"]
fn menu_capture_is_deterministic_and_draws_the_panel_art() {
    let Some((_root, path)) = common::asset_env() else {
        return;
    };
    let dir = std::env::temp_dir().join(format!("arklay-menu-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let saves = dir.join("saves");
    let first = dir.join("menu_a.bmp");
    let second = dir.join("menu_b.bmp");

    engine::run_ui_with_options(&path, "menu", Some(&first), &saves, 0).unwrap();
    engine::run_ui_with_options(&path, "menu", Some(&second), &saves, 0).unwrap();

    let first_bytes = std::fs::read(&first).unwrap();
    let second_bytes = std::fs::read(&second).unwrap();
    assert_eq!(first_bytes, second_bytes, "two menu captures differ");
    let image = bmp::decode(&first_bytes).unwrap();
    assert_eq!((image.width, image.height), (320, 240));

    // The centre viewport is the original's pure-black screen clear, not a
    // grey: sample points away from the panel art and the item slots.
    let pixel = |x: usize, y: usize| -> [u8; 4] {
        let offset = (y * image.width as usize + x) * 4;
        image.rgba[offset..offset + 4].try_into().unwrap()
    };
    assert_eq!(pixel(35, 60), [0, 0, 0, 255]);
    assert_eq!(pixel(170, 100), [0, 0, 0, 255]);

    let painted = image
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[..3] != [0, 0, 0])
        .count();
    println!("pixels painted by the menu: {painted}");
    assert!(painted > 5000, "the menu only painted {painted} pixels");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "requires both ARKLAY_RE1_ROOT and ARKLAY_RE1_PACK"]
fn per_item_icons_match_the_shared_atlas_cells() {
    let Some((_root, path)) = common::asset_env() else {
        return;
    };
    let pack = Pack::open(&path).unwrap();
    let atlas =
        ui::load_picture(&pack, "item/item_all", Geometry::Shipped, Some([0, 0, 0])).unwrap();
    let row = items::icon_row(0x02).unwrap();
    let y = usize::from(row) * items::ICON_HEIGHT as usize;
    let per_item = ui::load_picture(&pack, "item/02", Geometry::Shipped, Some([0, 0, 0])).unwrap();
    assert_eq!(
        (per_item.width, per_item.height),
        (items::ICON_WIDTH, items::ICON_HEIGHT)
    );
    for row in 0..items::ICON_HEIGHT as usize {
        let atlas_row =
            &atlas.rgba[(y + row) * atlas.width as usize * 4..][..items::ICON_WIDTH as usize * 4];
        let icon_row = &per_item.rgba[row * items::ICON_WIDTH as usize * 4..]
            [..items::ICON_WIDTH as usize * 4];
        assert_eq!(atlas_row, icon_row, "row {row}");
    }
}

#[test]
#[ignore = "requires both ARKLAY_RE1_ROOT and ARKLAY_RE1_PACK"]
fn per_item_icons_match_the_staitem_rows_for_the_machineguns() {
    let Some((_root, path)) = common::asset_env() else {
        return;
    };
    let pack = Pack::open(&path).unwrap();
    let arklay::assets::Artwork::Indexed(page) = ui::load_page(&pack, "ui/staitem").unwrap() else {
        panic!("ui/staitem is an indexed page");
    };
    for (item, row) in [(items::ITEM_INGRAM, 0usize), (items::ITEM_MINIMI, 1)] {
        let icon = ui::load_picture(
            &pack,
            &format!("item/{item:02x}"),
            Geometry::Shipped,
            Some([0, 0, 0]),
        )
        .unwrap();
        assert_eq!(
            (icon.width, icon.height),
            (items::ICON_WIDTH, items::ICON_HEIGHT)
        );
        for y in 0..items::ICON_HEIGHT as usize {
            for x in 0..items::ICON_WIDTH as usize {
                let index =
                    page.indices[(row * items::ICON_HEIGHT as usize + y) * page.width as usize + x];
                let expected = if index == 0 {
                    [0, 0, 0, 0]
                } else {
                    page.palette(0, index)
                };
                let at = (y * items::ICON_WIDTH as usize + x) * 4;
                assert_eq!(
                    &icon.rgba[at..at + 4],
                    &expected,
                    "item {item:#04x} at ({x}, {y})"
                );
            }
        }
    }
}
