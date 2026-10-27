//! The converted pack's fonts and the metrics their descriptions declare.
//!
//! Needs `ARKLAY_RE1_ROOT` and `ARKLAY_RE1_PACK`; without them the test skips.
//! Each font's per-glyph metrics come from its `font/<id>.xml`: Classic
//! REbirth's width tables, the indent the left trim and the advance the
//! cursor step.

use crate::common;

use arklay::font::{Font, FontDef, GlyphMetric};
use arklay::pack::Pack;

/// Decode the pack's Japanese sheet.
fn font(pack: &Pack) -> Font {
    arklay::font::load(pack, arklay::font::JAPANESE_ID).expect("the pack's Japanese font")
}

/// The metric of left-grid cell `cell`: a plain byte, or `0xEA + nn` for
/// `0xF8 nn`.
fn cell(font: &Font, cell: usize) -> GlyphMetric {
    font.glyph_table.get(0, 2 + cell / 18, cell % 18)
}

#[test]
#[ignore = "requires a real RE1 installation via ARKLAY_RE1_ROOT and ARKLAY_RE1_PACK"]
fn the_packed_fonts_take_their_descriptions_metrics() {
    let Some((_root, pack_path)) = common::asset_env() else {
        return;
    };
    let pack = Pack::open(&pack_path).expect("open the pack");

    // Both fonts are packed with the checked-in descriptions, and every
    // listed cell draws with its own metric.
    for (id, shipped) in [
        (arklay::font::WESTERN_ID, FontDef::western()),
        (arklay::font::JAPANESE_ID, FontDef::japanese()),
    ] {
        assert_eq!(arklay::font::read_def(&pack, id).as_ref(), Some(&shipped));
        let font = arklay::font::load(&pack, id).expect("the font loads");
        assert_eq!(font.script(), shipped.script);
        assert_eq!(font.metrics, shipped.metrics);
        for &(index, metric) in &shipped.glyphs {
            assert_eq!(
                cell(&font, index),
                metric,
                "font {id:02X} cell {index:#05X}"
            );
        }
    }

    // CR's Japanese-mode anchors: an 8px space, 11px digits trimmed by one,
    // and the kana's full 14px cell.
    let font = font(&pack);
    assert_eq!(
        cell(&font, 0x00),
        GlyphMetric {
            indent: 0,
            advance: 8
        }
    );
    for byte in 0x0C..=0x15 {
        assert_eq!(
            cell(&font, byte),
            GlyphMetric {
                indent: 1,
                advance: 11
            },
            "digit {byte:#04X} must stay monospaced"
        );
    }
    assert_eq!(
        cell(&font, 0x60),
        GlyphMetric {
            indent: 0,
            advance: 14
        }
    );
}

/// Issue #28: the shipped sheet's `0x28` cell is the letter L, and the
/// "You have used the lockpick." message must draw it, not the round
/// parenthesis at the ASCII renderer's remap target.
#[test]
#[ignore = "requires a real RE1 installation via ARKLAY_RE1_ROOT and ARKLAY_RE1_PACK"]
fn the_shipped_lockpick_name_draws_its_letter() {
    let Some((_root, pack_path)) = common::asset_env() else {
        return;
    };
    let pack = Pack::open(&pack_path).expect("open the pack");
    let font = font(&pack);
    let text = arklay::text::Text::load(&pack, Some("en-US"));
    let name = text.item_name(0x31).expect("item 0x31's name");
    assert_eq!(name[0], 0x28, "the charset spells the name's L as 0x28");

    // The glyph samples the left page's letter cell (column 4, row 2), not
    // the parenthesis cell the old remap redirected `0x28` to.
    let glyph = arklay::font::Glyphs::new(name, font.metrics)
        .next()
        .expect("the name's first cell");
    assert_eq!((glyph.u, glyph.v), (4 * 14, 28 + 2 * 14));

    // Draw the name at brightness 0 (full brightness, no shadow pass) and
    // compare the first cell's ink with the sheet's own cells: the drawn
    // pixels must be the letter's, and the two cells must differ so the
    // comparison means something.
    let mut framebuffer = arklay::render::Framebuffer::new();
    font.draw_text(
        &mut framebuffer,
        0,
        0,
        arklay::render::Tint::White,
        0,
        name,
        false,
    );
    let drawn: Vec<bool> = (0..14 * 14)
        .map(|cell| {
            let (x, y) = (cell % 14, cell / 14);
            framebuffer.rgba[(y * 320 + x) * 4 + 3] != 0
        })
        .collect();
    let sheet_cell = |u: u32, v: u32| -> Vec<bool> {
        (0..14 * 14)
            .map(|cell| {
                let (x, y) = (cell % 14, cell / 14);
                font.texture.indices[((v + y) * font.texture.width + u + x) as usize] != 0
            })
            .collect()
    };
    let letter = sheet_cell(56, 56);
    let parenthesis = sheet_cell(56, 224);
    assert_ne!(
        letter, parenthesis,
        "the letter and parenthesis cells must differ"
    );
    assert_eq!(drawn, letter, "the first cell must draw the sheet's L");
}
