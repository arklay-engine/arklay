//! Room 200's computer console (`games/re2/ui/computer.lua`) over a pack
//! holding the script and a synthetic console page, drawn from a running
//! [`crate::game::computer2::Computer`]'s view.

use super::*;

use crate::game::computer2::{Computer, test_strings};
use crate::pack::Pack;
use crate::render::Framebuffer;
use crate::ui::{Screen, ScreenAction};

const GREY: u8 = 100;
const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];

/// An 8 bpp 256 by 256 page shaped like room 200's: every glyph cell (rows
/// 0 to 29) red, the cursor (u 224, v 10) green, the panel (v 117 and on)
/// blue around its transparent cell area (u 4..244, v 128..248), and index
/// 0 transparent elsewhere.
fn page() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&0x10u32.to_le_bytes());
    out.extend_from_slice(&(1u32 | 8).to_le_bytes());
    out.extend_from_slice(&(12u32 + 256 * 2).to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&256u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    for index in 0..256u16 {
        let colour: u16 = match index {
            1 => 0x001F,
            2 => 0x03E0,
            3 => 0x7C00,
            _ => 0,
        };
        out.extend_from_slice(&colour.to_le_bytes());
    }
    out.extend_from_slice(&(12u32 + 256 * 256).to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&128u16.to_le_bytes());
    out.extend_from_slice(&256u16.to_le_bytes());
    for v in 0..256usize {
        for u in 0..256usize {
            let cell_area = (4..244).contains(&u) && (128..248).contains(&v);
            out.push(match (u, v) {
                (224..232, 10..20) => 2,
                (_, 0..30) => 1,
                (0..252, 117..256) if !cell_area => 3,
                _ => 0,
            });
        }
    }
    out
}

fn pack() -> &'static Pack {
    use std::sync::OnceLock;
    static PACK: OnceLock<Pack> = OnceLock::new();
    PACK.get_or_init(|| {
        let mut writer = crate::pack::PackWriter::new();
        writer
            .add(
                "ui/computer.lua",
                include_str!("../../../games/re2/ui/computer.lua")
                    .as_bytes()
                    .to_vec(),
            )
            .unwrap();
        writer.add("ui/computer/rc200.tim", page()).unwrap();
        Pack::from_bytes(writer.to_bytes().unwrap()).unwrap()
    })
}

fn context<'a>(
    dir: &'a std::path::Path,
    game: Option<&'a crate::game::GameState>,
) -> UiContext<'a> {
    UiContext {
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

/// The computer's frame drawn over a grey frame.
fn draw(computer: &Computer) -> Framebuffer {
    use crate::state::{RoomId, RoomState};
    let dir = std::env::temp_dir().join(format!("arklay-re2-computer-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut game =
        crate::game::GameState::new(RoomId::parse("2000").unwrap(), &RoomState::default());
    game.re2_computer = Some(computer.clone());
    let mut screen = {
        let mut cx = context(&dir, None);
        LuaScreen::open_screen_with_game("computer", &[], ScreenAction::Resume, &mut cx, &mut game)
            .unwrap()
    };
    let mut framebuffer = Framebuffer::new();
    framebuffer.fill_rect([0, 0, 320, 240], [GREY, GREY, GREY, 255]);
    let cx = context(&dir, Some(&game));
    screen.draw(&cx, &mut framebuffer);
    framebuffer
}

fn pixel(framebuffer: &Framebuffer, x: i32, y: i32) -> [u8; 4] {
    let at = (y as usize * framebuffer.width as usize + x as usize) * 4;
    framebuffer.rgba[at..at + 4].try_into().unwrap()
}

/// Tick a computer at the counter until `done` holds for its view.
fn computer_until(done: impl Fn(&crate::game::computer2::View) -> bool) -> Computer {
    let mut computer = Computer::new(0xF0, test_strings());
    for _ in 0..200 {
        computer.tick(&crate::game::computer2::World::default());
        if done(&computer.view()) {
            return computer;
        }
    }
    panic!("the view never came");
}

#[test]
fn computer_lua_draws_the_frame_inclusive_and_alone() {
    let computer = computer_until(|view| view.frame == Some([12, 10, 33, 21]));
    let framebuffer = draw(&computer);
    for (x, y) in [
        (12, 10),
        (33, 10),
        (12, 21),
        (33, 21),
        (20, 10),
        (20, 21),
        (12, 15),
        (33, 15),
    ] {
        assert_eq!(pixel(&framebuffer, x, y), WHITE, "edge pixel ({x}, {y})");
    }
    for (x, y) in [(13, 11), (32, 20), (34, 10), (12, 22), (100, 100)] {
        assert_eq!(
            pixel(&framebuffer, x, y),
            [GREY, GREY, GREY, 255],
            "({x}, {y})"
        );
    }
}

#[test]
fn computer_lua_draws_a_zero_length_frame_as_one_pixel() {
    // The open's step 0, `Wframe_move`'s first call.
    let computer = computer_until(|view| view.frame == Some([12, 10, 12, 10]));
    let framebuffer = draw(&computer);
    assert_eq!(pixel(&framebuffer, 12, 10), WHITE);
    for (x, y) in [(13, 10), (12, 11), (11, 10)] {
        assert_eq!(pixel(&framebuffer, x, y), [GREY, GREY, GREY, 255]);
    }
}

#[test]
fn computer_lua_draws_tile_cells_panel_and_cursor() {
    let computer =
        computer_until(|view| view.drawn && view.cursor.is_some() && view.cells.len() >= 2);
    let view = computer.view();
    assert_eq!(view.frame, None);
    let framebuffer = draw(&computer);
    // The glyphs, each 8 by 10 at its place.
    for cell in &view.cells {
        assert_eq!(pixel(&framebuffer, cell[0], cell[1]), RED);
        assert_eq!(pixel(&framebuffer, cell[0] + 7, cell[1] + 9), RED);
    }
    // The cursor over them.
    let [x, y] = view.cursor.unwrap();
    assert_eq!(pixel(&framebuffer, x, y), GREEN);
    assert_eq!(pixel(&framebuffer, x + 7, y + 9), GREEN);
    // The panel's border, and its empty cell area halved by the black tile.
    assert_eq!(pixel(&framebuffer, 12, 10), BLUE);
    assert_eq!(pixel(&framebuffer, 263, 148), BLUE);
    let half = GREY / 2;
    assert_eq!(pixel(&framebuffer, 250, 135), [half, half, half, 255]);
    // Outside the console nothing changes.
    assert_eq!(pixel(&framebuffer, 264, 149), [GREY, GREY, GREY, 255]);
    assert_eq!(pixel(&framebuffer, 11, 9), [GREY, GREY, GREY, 255]);
}
