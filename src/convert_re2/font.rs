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
//! The metrics are the executable's `width_tbl` ([`WIDTHS`]), which
//! `mess_trans` and `Name_disp` place the first 0x57 cells by: a cell is
//! drawn its bearing's columns before the cursor, which then moves on by its
//! advance. Any other cell is a full one, the `0xEE` page's ampersand among
//! them. Mostly a glyph's ink starts at the cursor and the next starts a
//! pixel after it ends, but not always: the digits are all 12 wide from a
//! pixel in, and `f` moves on a pixel farther than its ink's end and one. A
//! Western cell RE2 has no glyph for is placed by its ink.
//!
//! The text's colours are the original's `font_rgb` ([`FONT_RGB`]): the PC
//! version draws every glyph from the sheet's first CLUT row under a
//! primitive colour, which the description gives each row as a `<tint>`.
//!
//! The sheet's palette is the PC version's: each 5-bit level with its top
//! bits again below it (`v << 3 | v >> 2`), as its frames show the font.

use std::collections::BTreeMap;

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

/// `width_tbl` (`0x52E478` in `bio2 1.10.exe`): each of the sheet's first
/// 0x57 cells' bearing (the columns of the cell left of the cursor, which
/// needn't be where its ink starts) and advance.
#[rustfmt::skip]
const WIDTHS: [[u8; 2]; 0x57] = [
    [0, 8], [2, 5], [4, 7], [4, 6], [5, 6], [5, 7], [3, 7], [3, 8],
    [4, 7], [6, 8], [1, 8], [2, 10], [1, 12], [1, 12], [1, 12], [1, 12],
    [1, 12], [1, 12], [1, 12], [1, 12], [1, 12], [1, 12], [4, 7], [0, 14],
    [2, 5], [2, 11], [4, 6], [2, 10], [1, 13], [0, 14], [1, 13], [1, 12],
    [1, 12], [1, 12], [2, 11], [1, 13], [0, 14], [4, 7], [1, 13], [1, 13],
    [1, 13], [0, 15], [0, 14], [1, 12], [1, 12], [0, 13], [0, 14], [1, 12],
    [1, 13], [1, 13], [1, 13], [0, 15], [1, 13], [1, 13], [1, 13], [3, 8],
    [0, 15], [3, 8], [4, 7], [0, 14], [0, 14], [2, 10], [2, 10], [3, 9],
    [2, 10], [3, 9], [3, 9], [2, 10], [2, 11], [4, 6], [4, 6], [2, 11],
    [4, 6], [0, 14], [2, 11], [2, 10], [2, 10], [2, 10], [3, 8], [3, 9],
    [3, 8], [2, 11], [2, 11], [0, 14], [2, 11], [2, 11], [3, 9],
];

/// `font_rgb` (`0x985080`), which `Init_main` copies from the fifteen bytes
/// after `Common/Data/font0.adt`'s picture: the primitive colour of each of
/// the text's five colour rows (white, green, red, grey and blue), which
/// `mess_trans` and `Name_disp` draw a glyph under.
const FONT_RGB: [[u8; 3]; 5] = [
    [128, 128, 128],
    [0, 100, 5],
    [95, 0, 35],
    [54, 53, 55],
    [15, 50, 128],
];

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
    // The RE2 cell each Western cell takes its glyph, and so its place,
    // from. Two of RE2's characters can share a Western cell (the closing
    // and the straight quote); the later one has it.
    let mut sources: BTreeMap<usize, usize> = SYMBOLS.iter().map(|&cell| (cell, cell)).collect();
    for (cell, ch) in RE2_CELLS.chars().enumerate() {
        if ch != '_' && ch != ' ' {
            sources.extend(western_cell(ch).map(|target| (target, cell)));
        }
    }
    sources.extend(western_cell('&').map(|target| (target, AMPERSAND)));
    for (&to, &from) in &sources {
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
    for (row, [r, g, b]) in FONT_RGB.iter().enumerate().skip(1) {
        xml.push_str(&format!(
            "  <tint row=\"{row}\" colour=\"#{r:02X}{g:02X}{b:02X}\"/>\n"
        ));
    }
    for cell in 1..CELLS {
        if SYMBOLS.contains(&cell) {
            continue;
        }
        let (indent, advance) = match sources.get(&cell) {
            // The original's place for the glyph, inked or not; a cell past
            // the table is a full one at the cursor.
            Some(&from) => match WIDTHS.get(from) {
                Some(&[bearing, advance]) => (i32::from(bearing), i32::from(advance)),
                None => (0, CELL as i32),
            },
            // The Western sheet's own glyph: its ink at the cursor, and
            // the next a pixel after it ends.
            None => match ink(&sheet, cell) {
                Some((left, right)) => (left, right - left + 2),
                None => continue,
            },
        };
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

    /// A 256 x 256 4bpp `font0.tim` whose cell `c` has ink (index 1, a white
    /// of levels 25, 25 and 23) in the columns `ink(c)` says, first and last.
    fn font0(ink: impl Fn(usize) -> Option<(usize, usize)>) -> Vec<u8> {
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
        let inked = |x: usize, y: usize| {
            y >= TOP
                && x < COLUMNS * CELL
                && ink((y - TOP) / CELL * COLUMNS + x / CELL)
                    .is_some_and(|(first, last)| (first..=last).contains(&(x % CELL)))
        };
        for y in 0..256usize {
            for pair in 0..128usize {
                data.push(u8::from(inked(pair * 2, y)) | u8::from(inked(pair * 2 + 1, y)) << 4);
            }
        }
        data
    }

    /// The description's metric of `ch`'s cell.
    fn metric(xml: &str, ch: char) -> Option<(i32, i32)> {
        let def = crate::font::xml::FontDef::parse(xml).unwrap();
        def.glyphs
            .iter()
            .find(|(cell, _)| Some(*cell) == western_cell(ch))
            .map(|(_, metric)| (metric.indent, metric.advance))
    }

    #[test]
    fn the_executables_table_places_the_glyphs() {
        let re2: Vec<char> = RE2_CELLS.chars().collect();
        let cell = |ch: char| re2.iter().position(|&c| c == ch).unwrap();
        // Every cell inked in columns 3 to 9, but the closing quote's (5 to
        // 6, where the straight one's is 2 to 3) and `z`'s, which is blank.
        let ink = |c: usize| match c {
            c if c == cell('\u{201d}') => Some((5, 6)),
            c if c == cell('"') => Some((2, 3)),
            c if c == cell('z') => None,
            _ => Some((3, 9)),
        };
        let (bmp, xml) = sheet(&font0(ink)).unwrap();
        // The table's place, whatever the ink: `f` moves on by 9 where its
        // ink's end and one would be 8, the digits are 12 wide from a pixel
        // in, and a blank cell has its place too.
        assert_eq!(metric(&xml, 'f'), Some((3, 9)));
        assert_eq!(metric(&xml, 'a'), Some((2, 10)));
        assert_eq!(metric(&xml, 'O'), Some((1, 12)));
        assert_eq!(metric(&xml, '\''), Some((4, 7)));
        assert_eq!(metric(&xml, 'z'), Some((3, 9)));
        for digit in '0'..='9' {
            assert_eq!(metric(&xml, digit), Some((1, 12)), "{digit}");
        }
        // The `0xEE` page's cell is past the table: a full cell at the cursor.
        assert_eq!(metric(&xml, '&'), Some((0, 14)));
        // The two quotes share a Western cell, which has the straight one's
        // glyph and place, not the closing one's.
        assert_eq!(western_cell('\u{201d}'), western_cell('"'));
        assert_eq!(metric(&xml, '"'), Some((2, 11)));
        let page = crate::bmp::decode_indexed_page(&bmp).unwrap();
        let inked = |ch: char, column: usize| {
            let (u, v) = origin(western_cell(ch).unwrap());
            page.indices[v * page.width as usize + u + column] != 0
        };
        assert!(inked('"', 2) && !inked('"', 5));
        assert!(inked('f', 3) && !inked('f', 2));
        // A Western glyph RE2 has none for goes by its own ink.
        let (left, right) = super::ink(&page, western_cell('\u{e9}').unwrap()).unwrap();
        assert_eq!(metric(&xml, '\u{e9}'), Some((left, right - left + 2)));
        // The text's colours are the original's, the first being the sheet's.
        let def = crate::font::xml::FontDef::parse(&xml).unwrap();
        assert_eq!(
            def.tints,
            [
                None,
                Some([0, 100, 5]),
                Some([95, 0, 35]),
                Some([54, 53, 55]),
                Some([15, 50, 128])
            ]
        );
        // The palette is the PC version's: level 25 is 206 and 23 is 189,
        // not the decoder's 205.
        assert_eq!(page.palette(0, 1), [206, 206, 189, 255]);
        assert_eq!(pc_level(0), 0);
        assert_eq!(pc_level(255), 255);
        assert_eq!(pc_level(164), 165);
    }

    #[test]
    fn the_installed_font_and_table_are_the_ones_converted() {
        let Some(root) = std::env::var_os("ARKLAY_RE2_ROOT").map(std::path::PathBuf::from) else {
            return;
        };
        // `width_tbl`, at its address less the image's base.
        let exe = std::fs::read(root.join("bio2 1.10.exe")).unwrap();
        let table = &exe[0x12_E478..0x12_E478 + 2 * WIDTHS.len()];
        assert_eq!(table, WIDTHS.as_flattened());
        // `font_rgb`, after the 16-bit picture and its header.
        let adt = std::fs::read(root.join("data/Common/Data/font0.adt")).unwrap();
        let adt = crate::adt::decode(&adt).unwrap();
        assert_eq!(adt[0x2_0014..], *FONT_RGB.as_flattened());
        let font0 = std::fs::read(root.join("data/Common/Data/font0.tim")).unwrap();
        let (bmp, xml) = sheet(&font0).unwrap();
        let def = crate::font::xml::FontDef::parse(&xml).unwrap();
        assert_eq!(def.metrics.left_margin, 16);
        // `W` fills its cell and `t` starts three columns in.
        assert_eq!(metric(&xml, 'W'), Some((0, 15)));
        assert_eq!(metric(&xml, 't'), Some((3, 8)));
        crate::bmp::decode_indexed_page(&bmp).unwrap();
    }
}
