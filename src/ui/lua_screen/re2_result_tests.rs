//! RE 2's results (`games/re2/ui/result.lua`) over a pack holding the script
//! and synthetic `end00.adt` sheets, drawn from a
//! [`crate::result2::Result`] showing chosen pieces: the words Classic
//! REbirth gives them in English.

use super::*;

use crate::pack::Pack;
use crate::render::Framebuffer;
use crate::result2::Result;
use crate::ui::ScreenAction;

const GREY: u8 = 100;

/// An 8 bpp 256 by 256 sheet whose texels `(u, v)` take index
/// `index(u, v)`, the CLUT's colour `n` being `n` itself as a 15-bit colour
/// (red its low five bits, green the next three), so a drawn pixel gives its
/// index back ([`index`]); 0 is transparent.
fn sheet(index: impl Fn(usize, usize) -> u8) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&0x10u32.to_le_bytes());
    out.extend_from_slice(&(1u32 | 8).to_le_bytes());
    out.extend_from_slice(&(12u32 + 256 * 2).to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&256u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    for colour in 0..256u16 {
        out.extend_from_slice(&colour.to_le_bytes());
    }
    out.extend_from_slice(&(12u32 + 256 * 256).to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&128u16.to_le_bytes());
    out.extend_from_slice(&256u16.to_le_bytes());
    for v in 0..256 {
        out.extend((0..256).map(|u| index(u, v)));
    }
    out
}

/// The sheets: the letters' sheet (3) by `u` (`255 - u`) over its rows of
/// glyphs, the lines' sheets (1 and 2) by `v` (`255 - v`).
fn pack() -> &'static Pack {
    use std::sync::OnceLock;
    static PACK: OnceLock<Pack> = OnceLock::new();
    PACK.get_or_init(|| {
        let mut writer = crate::pack::PackWriter::new();
        writer
            .add(
                "ui/result.lua",
                include_str!("../../../games/re2/ui/result.lua")
                    .as_bytes()
                    .to_vec(),
            )
            .unwrap();
        let by_v = |_: usize, v: usize| (255 - v) as u8;
        let sheets = [
            sheet(by_v),
            sheet(by_v),
            sheet(by_v),
            sheet(|u, v| if v < 62 { (255 - u) as u8 } else { 0 }),
        ];
        for (n, sheet) in sheets.into_iter().enumerate() {
            writer
                .add(&format!("ui/result/end00_{n}.tim"), sheet)
                .unwrap();
        }
        Pack::from_bytes(writer.to_bytes().unwrap()).unwrap()
    })
}

fn context<'a>(
    dir: &'a std::path::Path,
    game: Option<&'a crate::game::GameState>,
) -> UiContext<'a> {
    UiContext {
        statics: None,
        authentic: false,
        pack: pack(),
        save_dir: dir,
        font: None,
        text: None,
        ticks: 0,
        cues: Default::default(),
        game,
    }
}

/// The results showing `pieces`, drawn over a grey 320 by 240 frame.
fn draw(pieces: &[usize]) -> Framebuffer {
    use crate::state::{RoomId, RoomState};
    let dir = std::env::temp_dir().join(format!("arklay-re2-result-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut game =
        crate::game::GameState::new(RoomId::parse("100").unwrap(), &RoomState::default());
    game.re2_result = Some(Box::new(Result::showing(pieces)));
    let mut screen = {
        let mut cx = context(&dir, None);
        LuaScreen::open_screen_with_game("result", &[], ScreenAction::Resume, &mut cx, &mut game)
            .unwrap()
    };
    let mut framebuffer = Framebuffer::new();
    framebuffer.fill_rect([0, 0, 320, 240], [GREY, GREY, GREY, 255]);
    let cx = context(&dir, Some(&game));
    screen.draw(&cx, &mut framebuffer);
    framebuffer
}

/// The frame's pixels in `x0..x1` by `y0..y1` (the screen's 320 by 240
/// place, centred on the frame).
fn pixels(framebuffer: &Framebuffer, [x0, y0, x1, y1]: [i32; 4]) -> Vec<[u8; 4]> {
    let (ox, oy) = (
        framebuffer.width as i32 / 2 - 160,
        framebuffer.height as i32 / 2 - 120,
    );
    let mut out = Vec::new();
    for y in y0..y1 {
        for x in x0..x1 {
            let at = ((oy + y) as usize * framebuffer.width as usize + (ox + x) as usize) * 4;
            out.push(framebuffer.rgba[at..at + 4].try_into().unwrap());
        }
    }
    out
}

/// The sheet index a drawn pixel shows, or `None` for the grey under it.
fn index(pixel: &[u8; 4]) -> Option<u8> {
    (pixel[..3] != [GREY; 3]).then(|| (pixel[0] >> 3) | ((pixel[1] >> 3) << 5))
}

/// The letters' glyphs (`glyph_map`'s `u` and width), by the letters and
/// signs `text_result` spells with.
fn letter(c: char) -> (usize, usize) {
    match c {
        'A' => (0, 13),
        'B' => (15, 13),
        'C' => (29, 13),
        'D' => (43, 13),
        'E' => (58, 11),
        'F' => (71, 11),
        'H' => (113, 12),
        'I' => (102, 6),
        'K' => (112, 13),
        'L' => (127, 11),
        'N' => (154, 13),
        'O' => (168, 14),
        'R' => (197, 13),
        'T' => (225, 12),
        'U' => (238, 13),
        'W' => (14, 14),
        'Y' => (42, 14),
        ' ' => (241, 8),
        _ => unreachable!("{c}"),
    }
}

/// Piece `piece` spells `word` from `x` on the row at `y`: each column the
/// letter's own, and nothing either side.
fn assert_spelt(piece: usize, word: &str, x: i32, y: i32) {
    let frame = draw(&[piece]);
    let expected: Vec<Option<u8>> = word
        .chars()
        .flat_map(|c| {
            let (u, width) = letter(c);
            (u..u + width).map(|u| Some((255 - u) as u8))
        })
        .collect();
    let width = expected.len() as i32;
    let row: Vec<_> = pixels(&frame, [x - 1, y + 5, x + width + 1, y + 6])
        .iter()
        .map(index)
        .collect();
    assert_eq!(row[0], None, "piece {piece}");
    assert_eq!(row[1..row.len() - 1], expected, "piece {piece}");
    assert_eq!(row[row.len() - 1], None, "piece {piece}");
}

#[test]
fn the_weapons_note_is_the_english_one() {
    // Piece 22, string 22, glyph 0x31: Classic REbirth gives it 0x30's rect,
    // sheet 1's rows 192 to 223.
    let frame = draw(&[22]);
    for y in 0..32 {
        let row = pixels(&frame, [31, 150 + y, 287, 151 + y]);
        assert_eq!(index(&row[0]), None);
        assert_eq!(index(&row[255]), None);
        let v = 192 + y as usize;
        assert!(
            row[1..255]
                .iter()
                .all(|p| index(p) == Some((255 - v) as u8)),
            "{v}"
        );
    }
}

#[test]
fn the_scenarios_names_are_spelt_in_letters_at_classic_rebirths_places() {
    assert_spelt(9, "LEON A", 210, 108);
    assert_spelt(10, "CLAIRE A", 201, 108);
    assert_spelt(11, "LEON B", 210, 108);
    assert_spelt(12, "CLAIRE B", 201, 108);
}

#[test]
fn the_extras_names_are_spelt_in_letters() {
    assert_spelt(31, "HUNK", 224, 108);
    assert_spelt(32, "TOFU", 217, 108);
}

#[test]
fn your_reward_replaces_weapon_get() {
    assert_spelt(23, "YOUR REWARD", 99, 30);
}

#[test]
fn the_japanese_lines_draw_nothing() {
    // Pieces 2 and 3 (glyphs 0x2D, 0x2E), 25 and 26 (0x2F), 29 and 30
    // (0x34, 0x35): zeroed glyphs.
    let frame = draw(&[2, 3, 25, 26, 29, 30]);
    assert!(
        pixels(&frame, [0, 0, 320, 240])
            .iter()
            .all(|p| index(p).is_none())
    );
}
