//! RE2's message font, `Common/Data/font0.tim`, as the pack's font `00`.
//!
//! The original draws its names and messages with `font0.tim`: 14px cells,
//! 18 to a row from v 28, the layout of RE1's Japanese sheet's left page,
//! with the letters at the bytes of RE2's message table. Arklay's Latin
//! script addresses Classic REbirth's Western sheet, which lays the same
//! cells out by another byte assignment, so the conversion moves each of
//! RE2's glyphs onto the Western cell of the same character. Cells RE2's
//! sheet has no letter for (the accented Latin) keep the Western glyph.
//!
//! The metrics are the ones measured in the original's frames: a glyph's ink
//! starts at the cursor and the next starts one pixel after its ink ends,
//! a space is 8 pixels, and an apostrophe keeps two pixels on each side.

use anyhow::{Context, Result, ensure};

use crate::charset::Script;
use crate::font::GLYPH_HEIGHT;
use crate::model::{PALETTE_ROW_LEN, Texture8};

/// The cell pitch.
const CELL: usize = 14;
/// Cells per sheet row.
const COLUMNS: usize = 18;
/// The y of the first cell row; the rows above hold the menu font.
const TOP: usize = 28;
/// Cells on the sheet's grid (16 rows).
const CELLS: usize = 16 * COLUMNS;

/// RE2's message bytes as characters, the order of `font0.tim`'s cells
/// ([`super::text`]'s table); `_` is a cell with no letter.
const RE2_CELLS: &str =
    " .___()__“”_0123456789:_,\"!?_ABCDEFGHIJKLMNOPQRSTUVWXYZ[/]'-_abcdefghijklmnopqrstuvwxyz";
/// `0xEE 0x12`'s cell, the ampersand.
const AMPERSAND: usize = 0xEA + 0x12;
/// The symbol cells both sheets keep at the same byte: the choice cursor and
/// the page wait's arrow.
const SYMBOLS: [usize; 2] = [0x02, 0x0B];

/// The description's sheet-wide part: the Latin script at the original's
/// places. The first line's cells start at x 16 and y 175, so the glyphs'
/// ink tops sit at y 177 and each line 16 lower; the item page draws in the
/// same box. A question's choice is on its last line: `Yes` at 100, `No` at
/// 150, each cursor cell 18 to the left.
const HEADER: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!--
  RE2's message font (Common/Data/font0.tim), its glyphs moved onto the
  Western sheet's cells. Converted from the install; see src/convert_re2/font.rs.
-->
<font format="1" script="latin" advance="8" cellWidth="14"
      margin="16" textY="175" menuY="175" yesX="100" noX="150" cursorX="-18">
"#;

/// The pack's font `00`: the converted sheet as an indexed BMP and its
/// description.
pub(super) fn sheet(font0: &[u8]) -> Result<(Vec<u8>, String)> {
    let re2 = crate::tim::decode_4bpp(font0).context("font0.tim")?;
    ensure!(
        re2.width as usize >= COLUMNS * CELL && re2.height as usize >= TOP + CELLS / COLUMNS * CELL,
        "font0.tim is {}x{}, too small for the message grid",
        re2.width,
        re2.height
    );
    let mut sheet =
        crate::bmp::decode_indexed_page(crate::font::WESTERN_SHEET).context("the Western sheet")?;
    let mut moves: Vec<(usize, usize)> = SYMBOLS.iter().map(|&cell| (cell, cell)).collect();
    for (cell, ch) in RE2_CELLS.chars().enumerate() {
        if ch != '_' && ch != ' ' {
            moves.extend(western_cell(ch).map(|target| (cell, target)));
        }
    }
    moves.extend(western_cell('&').map(|target| (AMPERSAND, target)));
    for &(from, to) in &moves {
        copy_cell(&re2, from, &mut sheet, to);
    }
    // RE2's own grey ramp, which the text's tint colours.
    sheet.palettes = (0..PALETTE_ROW_LEN)
        .map(|index| match u8::try_from(index) {
            Ok(index) if index < 16 => re2.palette(0, index),
            _ => [0, 0, 0, 255],
        })
        .collect();
    sheet.stp = vec![false; PALETTE_ROW_LEN];
    let bmp = crate::bmp::encode_indexed_page(&sheet, 0)?;

    let mut xml = HEADER.to_owned();
    for cell in 1..CELLS {
        if SYMBOLS.contains(&cell) {
            continue;
        }
        let Some((left, right)) = ink(&sheet, cell) else {
            continue;
        };
        let (mut indent, mut advance) = (left, right - left + 2);
        if Some(cell) == western_cell('\'') {
            indent -= 1;
            advance += 2;
        }
        xml.push_str(&format!(
            "  <glyph cell=\"{cell:#04X}\" advance=\"{advance}\" indent=\"{indent}\"/>\n"
        ));
    }
    xml.push_str("</font>\n");
    Ok((bmp, xml))
}

/// The Western sheet's cell for `ch`, a plain one-byte cell.
fn western_cell(ch: char) -> Option<usize> {
    let mut bytes = Vec::new();
    crate::charset::encode(&ch.to_string(), Script::Latin, &mut bytes);
    match bytes.as_slice() {
        [byte] if usize::from(*byte) < CELLS => Some(usize::from(*byte)),
        _ => None,
    }
}

/// The top-left texel of grid cell `cell`.
fn origin(cell: usize) -> (usize, usize) {
    ((cell % COLUMNS) * CELL, TOP + (cell / COLUMNS) * CELL)
}

/// Copy cell `from` of `source` over cell `to` of `sheet`.
fn copy_cell(source: &Texture8, from: usize, sheet: &mut Texture8, to: usize) {
    let (su, sv) = origin(from);
    let (du, dv) = origin(to);
    for y in 0..GLYPH_HEIGHT as usize {
        for x in 0..CELL {
            let texel = source.indices[(sv + y) * source.width as usize + su + x];
            sheet.indices[(dv + y) * sheet.width as usize + du + x] = texel;
        }
    }
}

/// The first and last columns of `cell` with ink (any texel but the
/// transparent 0), or `None` for a blank cell.
fn ink(sheet: &Texture8, cell: usize) -> Option<(i32, i32)> {
    let (u, v) = origin(cell);
    let inked = |x: usize| {
        (0..GLYPH_HEIGHT as usize)
            .any(|y| sheet.indices[(v + y) * sheet.width as usize + u + x] != 0)
    };
    let left = (0..CELL).find(|&x| inked(x))?;
    let right = (0..CELL).rev().find(|&x| inked(x))?;
    Some((left as i32, right as i32))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn re2s_letters_land_on_the_western_cells() {
        assert_eq!(western_cell('A'), Some(0x1D));
        assert_eq!(western_cell('a'), Some(0x3D));
        assert_eq!(western_cell('.'), Some(6 * 18 + 13));
        assert_eq!(western_cell('&'), Some(7 * 18 + 1));
        // RE2's letters sit where the Western sheet's do.
        let re2: Vec<char> = RE2_CELLS.chars().collect();
        assert_eq!(re2[0x1D], 'A');
        assert_eq!(re2[0x3D], 'a');
    }

    #[test]
    fn the_installed_font_converts() {
        let Some(root) = std::env::var_os("ARKLAY_RE2_ROOT").map(std::path::PathBuf::from) else {
            return;
        };
        let font0 = std::fs::read(root.join("data/Common/Data/font0.tim")).unwrap();
        let (bmp, xml) = sheet(&font0).unwrap();
        let def = crate::font::xml::FontDef::parse(&xml).unwrap();
        assert_eq!(def.metrics.left_margin, 16);
        let metric = |ch: char| {
            def.glyphs
                .iter()
                .find(|(cell, _)| Some(*cell) == western_cell(ch))
                .map(|(_, metric)| *metric)
                .unwrap()
        };
        // `W` fills its cell; `t` starts three columns in.
        assert_eq!((metric('W').indent, metric('W').advance), (0, 15));
        assert_eq!(metric('t').indent, 3);
        crate::bmp::decode_indexed_page(&bmp).unwrap();
    }
}
