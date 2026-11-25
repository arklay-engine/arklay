//! RE 2's status screen (`games/re2/ui/main_menu.lua`) over a pack holding
//! the script and a synthetic frame sheet.

use super::*;

use crate::game::{GameState, Re2ItemGet, Re2ItemGetPhase, RoomAction, RoomActionKind};
use crate::pack::Pack;
use crate::render::Framebuffer;
use crate::ui::{Screen, ScreenAction};

/// The frame's sheet (texture 0x12) with only the item grid's pieces and the
/// command menu's top on it: the grid's bottom (u 0, v 143, 90 by 4) and its
/// two sides (u 113 and 118, from v 92), and the menu's top piece (u 0,
/// v 176, 56 by 13), index 1; index 0, transparent, elsewhere.
fn sheet() -> Vec<u8> {
    sheet_marking(|u, v| {
        let bottom = u < 90 && (143..147).contains(&v);
        let side = (113..123).contains(&u) && v >= 92;
        let menu = u < 56 && (176..189).contains(&v);
        bottom || side || menu
    })
}

/// The portrait's sheet (texture 0x13) with only the columns of the faces
/// beside the scenario's (Ada's or Sherry's from u 0x2C, HUNK's from 0x58,
/// 39 wide, from v 0x48) on it.
fn portrait_sheet() -> Vec<u8> {
    sheet_marking(|u, v| ((0x2C..0x53).contains(&u) || (0x58..0x7F).contains(&u)) && v >= 0x48)
}

/// A 256 by 256 8-bit TIM whose texels are index 1 (white) where `marked`
/// and index 0 (transparent) elsewhere.
fn sheet_marking(marked: impl Fn(usize, usize) -> bool) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&0x10u32.to_le_bytes());
    out.extend_from_slice(&(1u32 | 8).to_le_bytes());
    out.extend_from_slice(&(12u32 + 256 * 2).to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&256u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    for index in 0..256u16 {
        out.extend_from_slice(&(if index == 1 { 0x7FFFu16 } else { 0 }).to_le_bytes());
    }
    out.extend_from_slice(&(12u32 + 256 * 256).to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&128u16.to_le_bytes());
    out.extend_from_slice(&256u16.to_le_bytes());
    for v in 0..256usize {
        for u in 0..256usize {
            out.push(u8::from(marked(u, v)));
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
                "ui/main_menu.lua",
                include_str!("../../../games/re2/ui/main_menu.lua")
                    .as_bytes()
                    .to_vec(),
            )
            .unwrap();
        writer.add("ui/status/st0_jp_0.tim", sheet()).unwrap();
        writer
            .add("ui/status/st0_jp_1.tim", portrait_sheet())
            .unwrap();
        Pack::from_bytes(writer.to_bytes().unwrap()).unwrap()
    })
}

fn context<'a>(dir: &'a std::path::Path, game: Option<&'a GameState>) -> UiContext<'a> {
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

/// The get-item window's frame with an inventory of `before` slots when it
/// opened and `slots` when it is drawn.
fn get_window(before: u8, slots: u8) -> Framebuffer {
    use crate::state::{RoomId, RoomState};
    let dir = std::env::temp_dir().join(format!("arklay-re2-status-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut game = GameState::new(RoomId::parse("1020").unwrap(), &RoomState::default());
    game.is_re2_room = true;
    game.re2.inventory_size = before;
    game.re2_item_get = Some(Re2ItemGet {
        // Handgun rounds, as room 102's.
        action: RoomAction {
            slot: 0,
            kind: RoomActionKind::Item,
            zone: [0; 4],
            sce: 2,
            handler: 1,
            flags: 0,
            params: [0x14, 30, 0, 0, 0, 0, 0, 0],
            item_data: None,
            room_items_flag: 0xFF,
            reach_animation: false,
        },
        phase: Re2ItemGetPhase::Open,
        ticks: 0,
        fly: Default::default(),
        slot: None,
        file: None,
    });
    let mut screen = {
        let mut cx = context(&dir, None);
        let args = [("mode", ScreenArg::Str("get".to_owned()))];
        LuaScreen::open_screen_with_game(
            "main_menu",
            &args,
            ScreenAction::Resume,
            &mut cx,
            &mut game,
        )
        .unwrap()
    };
    // `Status`'s start, on the task's sixth tick: the screen is already up.
    game.re2.inventory_size = slots;
    let mut framebuffer = Framebuffer::new();
    let cx = context(&dir, Some(&game));
    screen.draw(&cx, &mut framebuffer);
    framebuffer
}

/// Whether the frame's sheet drew at a pixel: the window's backdrop is dark.
fn framed(framebuffer: &Framebuffer, x: usize, y: usize) -> bool {
    let at = (y * framebuffer.width as usize + x) * 4;
    framebuffer.rgba[at] > 100
}

/// `St_disp_itemlist` (`0x4FF360`) sizes the grid's frame by
/// `inventory_size` on every draw: 30 a row of slots down the sides and the
/// bottom under them (`status_start_102`'s frames: the Side Pack's ten slots
/// reach 223, over the message box's pipe). The size is the one at the
/// draw, not the one the screen opened with.
#[test]
fn the_item_grids_frame_is_as_long_as_the_inventory() {
    for (before, slots, bottom) in [(8, 8, 193), (8, 10, 223), (10, 10, 223), (10, 8, 193)] {
        let frame = get_window(before, slots);
        for y in [193, 223] {
            assert_eq!(
                framed(&frame, 260, y + 1),
                y == bottom,
                "{slots} slots (opened with {before}): the bottom at {y}"
            );
        }
        // The sides run from 73 to the bottom's top.
        for x in [222, 307] {
            assert!(
                framed(&frame, x, bottom - 1),
                "{slots} slots: the side at {x}"
            );
            assert!(
                !framed(&frame, x, bottom + 5),
                "{slots} slots: under the side at {x}"
            );
        }
    }
}

/// The item page's logic draws the command menu, so the menu shows on the
/// tick a command takes the screen off the page: USE of a key item closes
/// the screen from the menu, and `crank_111`'s frame of the close's first
/// tick still has the menu where it was. The tick after has none.
#[test]
fn the_command_menu_draws_on_the_tick_a_command_leaves_the_item_page() {
    use crate::game::status2::{ItemPage, SUB_COMMAND};
    use crate::state::{RoomId, RoomState};
    let dir = std::env::temp_dir().join(format!("arklay-re2-status-menu-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut game = GameState::new(RoomId::parse("1110").unwrap(), &RoomState::default());
    game.is_re2_room = true;
    // USE, chosen: the menu at rest.
    let page = ItemPage {
        sub: SUB_COMMAND,
        command: 3,
        ..ItemPage::default()
    };
    game.re2_items = Some(page);
    let mut screen = {
        let mut cx = context(&dir, None);
        LuaScreen::open_screen_with_game("main_menu", &[], ScreenAction::Resume, &mut cx, &mut game)
            .unwrap()
    };
    let mut menu_drawn = |game: &GameState, open: u8, close: Option<u8>| {
        let cx = context(&dir, Some(game));
        screen.signal_with("animate", (open, close), &cx);
        let mut framebuffer = Framebuffer::new();
        screen.draw(&cx, &mut framebuffer);
        // The menu's top piece at its rest: x 162 to 217, y 70 to 82.
        framed(&framebuffer, 180, 75)
    };
    assert!(menu_drawn(&game, 20, None), "the menu on the item page");
    // The close's page: the screen is off the item page.
    game.re2_items = Some(ItemPage { page: 0, ..page });
    assert!(
        menu_drawn(&game, 21, Some(0)),
        "the tick USE closes the screen"
    );
    assert!(!menu_drawn(&game, 22, Some(1)), "the tick after");
}

/// `St_init_disp_face` (`0x4FE8C0`) picks the portrait's texels by the
/// player's id when `Status` starts: Ada (0xE) and Sherry (0xF) show the
/// faces at u 0x2C of the scenario's sheet and HUNK (0xC) the one at 0x58;
/// Leon and Claire the ones at u 0, and TO-FU (0xD) rows of his own, from
/// v 0x34, which the marked columns don't reach.
#[test]
fn the_portrait_is_the_face_of_the_character_the_player_is() {
    use crate::game::status2::ItemPage;
    use crate::state::{RoomId, RoomState};
    let dir = std::env::temp_dir().join(format!("arklay-re2-status-face-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut game = GameState::new(RoomId::parse("3060").unwrap(), &RoomState::default());
    game.is_re2_room = true;
    game.re2_items = Some(ItemPage::default());
    let mut screen = {
        let mut cx = context(&dir, None);
        LuaScreen::open_screen_with_game("main_menu", &[], ScreenAction::Resume, &mut cx, &mut game)
            .unwrap()
    };
    for (id, face) in [
        (0, false),
        (1, false),
        (0xC, true),
        (0xD, false),
        (0xE, true),
        (0xF, true),
    ] {
        game.re2.pl_id = id;
        let cx = context(&dir, Some(&game));
        screen.signal_with("animate", (20u8, None::<u8>), &cx);
        let mut framebuffer = Framebuffer::new();
        screen.draw(&cx, &mut framebuffer);
        // The three rows: y 20 to 29, 31 to 50 and 51 to 70, from x 11.
        for y in [25, 40, 60] {
            assert_eq!(
                framed(&framebuffer, 30, y),
                face,
                "id {id:#x}: the row at y {y}"
            );
        }
    }
}
