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
//! The metrics are the executable's `width_tbl` ([`super::exe`]), which
//! `mess_trans` and `Name_disp` place the first 0x57 cells by: a cell is
//! drawn its left edge's columns before the cursor, which then moves on by
//! its advance. Any other cell is a full one, the `0xEE` page's ampersand
//! among them. Mostly a glyph's ink starts at the cursor and the next starts
//! a pixel after it ends, which is what a sheet converted without the
//! executable gets (with an apostrophe's two pixels on each side); the
//! table's digits are all 12 wide from a pixel in, and its `f` a pixel wider
//! than its ink.
//!
//! The sheet's palette is the PC version's: each 5-bit level with its top
//! bits again below it (`v << 3 | v >> 2`), as its frames show the font.

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
/// 150, each cursor cell 18 to the left. That is the status screen's and the
/// get-item window's place (`Mess_set`'s `0xAF0010`); a message in play is
/// placed at `0xB90022` instead, by the game (`GameState::show_re2_message`).
const HEADER: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!--
  RE2's message font (Common/Data/font0.tim), its glyphs moved onto the
  Western sheet's cells. Converted from the install; see src/convert_re2/font.rs.
-->
<font format="1" script="latin" advance="8" cellWidth="14"
      margin="16" textY="175" menuY="175" yesX="100" noX="150" cursorX="-18">
"#;

/// The pack's font `00`: the converted sheet as an indexed BMP and its
/// description, with the executable's `widths` (`[left, advance]` by RE2
/// cell) when there are any.
pub(super) fn sheet(font0: &[u8], widths: Option<&[[u8; 2]]>) -> Result<(Vec<u8>, String)> {
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
            Ok(index) if index < 16 => {
                let [r, g, b, a] = re2.palette(0, index);
                let [r, g, b] = [r, g, b].map(pc_level);
                [r, g, b, a]
            }
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
        // The RE2 cell the glyph came from says how the original places it.
        let from = moves
            .iter()
            .find(|&&(_, to)| to == cell)
            .map(|&(from, _)| from);
        let table = widths
            .zip(from)
            .map(|(widths, from)| match widths.get(from) {
                Some(&[left, advance]) => (i32::from(left), i32::from(advance)),
                None => (0, CELL as i32),
            });
        let (indent, advance) = table.unwrap_or_else(|| {
            let (indent, advance) = (left, right - left + 2);
            if Some(cell) == western_cell('\'') {
                (indent - 1, advance + 2)
            } else {
                (indent, advance)
            }
        });
        xml.push_str(&format!(
            "  <glyph cell=\"{cell:#04X}\" advance=\"{advance}\" indent=\"{indent}\"/>\n"
        ));
    }
    xml.push_str("</font>\n");
    Ok((bmp, xml))
}

/// A decoded 5-bit level (`v * 255 / 31`) as the PC version holds it, its
/// top bits again below it.
fn pc_level(channel: u8) -> u8 {
    let level = (u32::from(channel) * 31).div_ceil(255);
    (level << 3 | level >> 2) as u8
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

    /// A 256 x 256 4bpp `font0.tim` whose every cell has ink (index 1, a
    /// white of levels 25, 25 and 23) in columns 3 to 9.
    fn font0() -> Vec<u8> {
        let mut data = Vec::new();
        let put32 = |data: &mut Vec<u8>, value: u32| data.extend_from_slice(&value.to_le_bytes());
        let put16 = |data: &mut Vec<u8>, value: u16| data.extend_from_slice(&value.to_le_bytes());
        put32(&mut data, 0x10);
        put32(&mut data, 0x08);
        put32(&mut data, 12 + 32);
        for value in [0, 480, 16, 1] {
            put16(&mut data, value);
        }
        put16(&mut data, 0);
        put16(&mut data, 25 | 25 << 5 | 23 << 10);
        data.resize(data.len() + 28, 0);
        put32(&mut data, 12 + 128 * 256);
        for value in [0, 0, 64, 256] {
            put16(&mut data, value);
        }
        for y in 0..256usize {
            for pair in 0..128usize {
                let inked =
                    |x: usize| y >= TOP && (3..=9).contains(&(x % CELL)) && x < COLUMNS * CELL;
                data.push(u8::from(inked(pair * 2)) | u8::from(inked(pair * 2 + 1)) << 4);
            }
        }
        data
    }

    /// The description's metric of `ch`'s cell.
    fn metric(xml: &str, ch: char) -> (i32, i32) {
        let def = crate::font::xml::FontDef::parse(xml).unwrap();
        def.glyphs
            .iter()
            .find(|(cell, _)| Some(*cell) == western_cell(ch))
            .map(|(_, metric)| (metric.indent, metric.advance))
            .unwrap()
    }

    #[test]
    fn the_executables_widths_place_the_glyphs_and_the_ink_does_without_them() {
        let re2: Vec<char> = RE2_CELLS.chars().collect();
        let cell = |ch: char| re2.iter().position(|&c| c == ch).unwrap();
        // The table as the executable has these cells.
        let mut widths = vec![[2u8, 10]; 0x57];
        widths[cell('f')] = [3, 9];
        widths[cell('1')] = [1, 12];
        widths[cell('\'')] = [4, 7];
        let (bmp, xml) = sheet(&font0(), Some(&widths)).unwrap();
        assert_eq!(metric(&xml, 'f'), (3, 9));
        assert_eq!(metric(&xml, '1'), (1, 12));
        assert_eq!(metric(&xml, '\''), (4, 7));
        assert_eq!(metric(&xml, 'a'), (2, 10));
        // The `0xEE` page's cell is past the table: a full cell at the cursor.
        assert_eq!(metric(&xml, '&'), (0, 14));
        // The palette is the PC version's: level 25 is 206 and 23 is 189,
        // not the decoder's 205.
        let page = crate::bmp::decode_indexed_page(&bmp).unwrap();
        assert_eq!(page.palette(0, 1), [206, 206, 189, 255]);
        assert_eq!(pc_level(0), 0);
        assert_eq!(pc_level(255), 255);
        assert_eq!(pc_level(164), 165);

        // Without the executable, each glyph's ink starts at the cursor and
        // the next a pixel after it ends; the apostrophe keeps two pixels a
        // side.
        let (_, xml) = sheet(&font0(), None).unwrap();
        assert_eq!(metric(&xml, 'f'), (3, 8));
        assert_eq!(metric(&xml, '1'), (3, 8));
        assert_eq!(metric(&xml, '&'), (3, 8));
        assert_eq!(metric(&xml, '\''), (2, 10));
    }

    #[test]
    fn the_installed_font_converts() {
        let Some(root) = std::env::var_os("ARKLAY_RE2_ROOT").map(std::path::PathBuf::from) else {
            return;
        };
        let font0 = std::fs::read(root.join("data/Common/Data/font0.tim")).unwrap();
        let exe = std::fs::read(root.join("bio2 1.10.exe")).unwrap();
        let exe = super::super::exe::ExeText::read(&exe).unwrap();
        let (bmp, xml) = sheet(&font0, Some(&exe.widths)).unwrap();
        let def = crate::font::xml::FontDef::parse(&xml).unwrap();
        assert_eq!(def.metrics.left_margin, 16);
        // `W` fills its cell and `t` starts three columns in; `f` is a pixel
        // wider than its ink, and a digit is 12 wide from a pixel in.
        assert_eq!(metric(&xml, 'W'), (0, 15));
        assert_eq!(metric(&xml, 't').0, 3);
        assert_eq!(metric(&xml, 'f'), (3, 9));
        assert_eq!(metric(&xml, '1'), (1, 12));
        assert_eq!(metric(&xml, 'O'), (1, 12));
        assert_eq!(metric(&xml, '\''), (4, 7));
        crate::bmp::decode_indexed_page(&bmp).unwrap();
    }
}
