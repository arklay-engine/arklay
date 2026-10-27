//! Real-asset FILE tab tests.
//!
//! Run with:
//! `ARKLAY_RE1_ROOT=assets/re1 ARKLAY_RE1_PACK=re1.akpak cargo test --profile test-release --test real file_real:: -- --ignored --nocapture`
//!
//! Both `ARKLAY_RE1_ROOT` and `ARKLAY_RE1_PACK` must be set together; with
//! neither the tests skip, and with only one they fail. The pack must carry
//! the entries the `ui/file.lua` reader draws (two `ui/file_book_<n>` covers,
//! a `file/<doc>_bg` backdrop per document and the Japanese half-pages
//! `file/<doc>_<page>_fg_ja-JP`) plus the menu sheets and font. The first
//! test loads every page and checks the sizes the JPN reader indexes; the
//! reader's paging is covered by the Lua screen's unit tests. The second
//! renders the `--ui file` capture twice and requires it to be deterministic
//! and to draw its own layer over the menu.

use crate::common;

use arklay::bmp;
use arklay::engine;
use arklay::pack::Pack;
use arklay::state::Image;

/// The TEXTM sheets each document's Japanese pages were split from; each
/// sheet is two half-pages.
const JPN_SHEETS: [usize; 16] = [2, 3, 6, 2, 1, 3, 4, 4, 4, 3, 3, 2, 1, 1, 1, 3];

fn pack() -> Option<Pack> {
    let (_, path) = common::asset_env()?;
    Some(Pack::open(&path).unwrap())
}

fn non_black(image: &Image) -> usize {
    image
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[0] != 0 || pixel[1] != 0 || pixel[2] != 0)
        .count()
}

#[test]
#[ignore = "requires both ARKLAY_RE1_ROOT and ARKLAY_RE1_PACK"]
fn real_file_art_has_the_sizes_the_reader_indexes() {
    let Some(pack) = pack() else {
        return;
    };
    let size = |stem: &str| {
        let art =
            arklay::ui::load_page(&pack, stem).unwrap_or_else(|err| panic!("{stem}: {err:#}"));
        (art.width(), art.height())
    };
    for cover in ["ui/file_book_0", "ui/file_book_1"] {
        assert_eq!(size(cover), (256, 256), "{cover}");
    }
    for (document, sheets) in JPN_SHEETS.into_iter().enumerate() {
        let backdrop = format!("file/{document:02x}_bg");
        assert_eq!(size(&backdrop), (256, 120), "{backdrop}");
        for page in 0..sheets * 2 {
            let stem = format!("file/{document:02x}_{page:02x}_fg_ja-JP");
            assert_eq!(size(&stem), (256, 128), "{stem}");
        }
        let past = format!("file/{document:02x}_{:02x}_fg_ja-JP.bmp", sheets * 2);
        assert!(!pack.contains(&past), "{past}");
    }
}

#[test]
#[ignore = "requires a converted pack via ARKLAY_RE1_ROOT and ARKLAY_RE1_PACK"]
fn real_file_capture_is_deterministic_and_draws_the_list() {
    let Some((_root, path)) = common::asset_env() else {
        return;
    };
    let dir = std::env::temp_dir().join(format!("arklay-file-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let saves = dir.join("saves");
    let first = dir.join("file_a.bmp");
    let second = dir.join("file_b.bmp");
    let menu_path = dir.join("menu.bmp");

    engine::run_ui_with_options(&path, "file", Some(&first), &saves, 0).unwrap();
    engine::run_ui_with_options(&path, "file", Some(&second), &saves, 0).unwrap();
    // The same capture path without the FILE tab: the room, camera and panel
    // beneath are the same, so the delta is the file selector's own layer.
    engine::run_ui_with_options(&path, "menu", Some(&menu_path), &saves, 0).unwrap();

    let first_bytes = std::fs::read(&first).unwrap();
    let second_bytes = std::fs::read(&second).unwrap();
    assert_eq!(first_bytes, second_bytes, "two file captures differ");
    let image = bmp::decode(&first_bytes).unwrap();
    assert_eq!((image.width, image.height), (320, 240));
    assert!(
        non_black(&image) > 5000,
        "the file capture is mostly black: {} pixels",
        non_black(&image)
    );

    // A missing FILE layer would leave the menu over the same room; require a
    // known delta so that case cannot pass.
    let menu = bmp::decode(&std::fs::read(&menu_path).unwrap()).unwrap();
    let changed = image
        .rgba
        .iter()
        .zip(&menu.rgba)
        .filter(|(a, b)| a != b)
        .count();
    println!("file vs menu: {changed} changed pixels");
    // The selector draws into the panel's `0x28,0x18 0xf0x0xc0` screen area
    // (the original masks it with the panel's chrome), so its own layer can
    // only repaint that much.
    assert!(
        changed > 20000,
        "the FILE tab only repainted {changed} pixels over the menu"
    );
    println!("wrote {}", first.display());
}
