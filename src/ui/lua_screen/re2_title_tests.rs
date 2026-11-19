//! RE 2's title (`games/re2/ui/title.lua`) over a pack holding only the
//! script and its backgrounds, against the original's trace
//! (`oracle/re2.md`, "The title").

use super::*;

use crate::pack::Pack;
use crate::ui::{Screen, ScreenAction, ScreenResult, UiCue, UiInput};

fn pack() -> &'static Pack {
    use std::sync::OnceLock;
    static PACK: OnceLock<Pack> = OnceLock::new();
    PACK.get_or_init(|| {
        let mut writer = crate::pack::PackWriter::new();
        writer
            .add(
                "ui/cr_text.lua",
                include_str!("../../../games/re2/ui/cr_text.lua")
                    .as_bytes()
                    .to_vec(),
            )
            .unwrap();
        writer
            .add(
                "ui/title.lua",
                include_str!("../../../games/re2/ui/title.lua")
                    .as_bytes()
                    .to_vec(),
            )
            .unwrap();
        writer
            .add(
                "ui/char_select.lua",
                include_str!("../../../games/re2/ui/char_select.lua")
                    .as_bytes()
                    .to_vec(),
            )
            .unwrap();
        // The backgrounds, so a test can tell which one is up.
        for (index, stem) in ["title/title_bg", "option/room", "option/color2"]
            .into_iter()
            .enumerate()
        {
            let picture = super::tests::solid_bmp(4, 4, [index as u8; 3]);
            writer.add(&format!("ui/{stem}.bmp"), picture).unwrap();
        }
        Pack::from_bytes(writer.to_bytes().unwrap()).unwrap()
    })
}

fn context<'a>(dir: &'a std::path::Path) -> UiContext<'a> {
    UiContext {
        authentic: false,
        pack: pack(),
        save_dir: dir,
        font: None,
        text: None,
        ticks: 0,
        cues: Default::default(),
        game: None,
    }
}

/// The trace's `ti` field from the values the script publishes.
fn ti(screen: &LuaScreen) -> Vec<i64> {
    [
        "ti_mv", "ti_disp", "ti_init", "ti_sub", "ti_menu", "ti_step", "ti_sel", "ti_cur",
        "ti_pulse", "ti_slide", "ti_demo",
    ]
    .iter()
    .map(|key| screen.value(key))
    .collect()
}

fn logo(screen: &LuaScreen) -> [i64; 3] {
    [
        screen.value("ti_flash"),
        screen.value("ti_logo"),
        screen.value("ti_logo_count"),
    ]
}

#[test]
fn the_title_opens_as_the_boot_reaches_it() {
    let dir = std::env::temp_dir().join(format!("arklay-re2-title-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cx = context(&dir);
    let mut cx_open = context(&dir);
    let mut screen =
        LuaScreen::open_screen("title", &[], ScreenAction::Quit, &mut cx_open).unwrap();
    // The original's ticks 189 to 191 (`title_menu`): `Title_init`'s white,
    // step 5's logo slide, then the menu's first tick.
    screen.update(&cx, UiInput::default());
    assert_eq!(ti(&screen), [2, 5, 1, 0, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(logo(&screen), [0, 240, -1]);
    assert_eq!(screen.value("ti_fade_level"), 0x7F80);
    assert_eq!(screen.value("ti_fade_mode"), 1);
    screen.update(&cx, UiInput::default());
    assert_eq!(ti(&screen), [2, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(logo(&screen), [52, -5, 21]);
    screen.update(&cx, UiInput::default());
    assert_eq!(ti(&screen), [2, 1, 1, 0, 0, 1, 0, 0, 2, 0, 1]);
    assert_eq!(logo(&screen), [50, -8, 20]);
    assert_eq!(screen.value("ti_fade_level"), 0);
    // A press of down moves the cursor with the core bank's cursor cue.
    for _ in 0..60 {
        screen.update(&cx, UiInput::default());
    }
    let held = UiInput {
        down: true,
        held_down: true,
        ..UiInput::default()
    };
    screen.update(&cx, held);
    assert_eq!(screen.value("ti_cur"), 1);
    assert!(cx.cues.borrow().contains(&UiCue::Core(4)));
    let _ = std::fs::remove_dir_all(&dir);
}

/// LOAD GAME (`Title_main_load`, `title_load_empty`): the confirm at 260,
/// the hand-off to the load screen on 531 with the task's step at 1, and,
/// once that screen comes back without a load, the title's picture and
/// `Title_init`'s branch from black on the two ticks after (709 and 710).
#[test]
fn load_game_sleeps_under_the_load_screen_and_starts_again_after_it() {
    let dir = std::env::temp_dir().join(format!("arklay-re2-load-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cx = context(&dir);
    let mut cx_open = context(&dir);
    let mut screen =
        LuaScreen::open_screen("title", &[], ScreenAction::Quit, &mut cx_open).unwrap();
    assert!(screen.resumes());
    for _ in 189..260 {
        screen.update(&cx, UiInput::default());
    }
    let confirm = UiInput {
        confirm: true,
        ..UiInput::default()
    };
    assert_eq!(screen.update(&cx, confirm), ScreenResult::Continue);
    for tick in 261..531 {
        assert_eq!(
            screen.update(&cx, UiInput::default()),
            ScreenResult::Continue,
            "{tick}"
        );
    }
    assert_eq!(ti(&screen)[..4], [2, 2, 1, 0], "530");
    assert_eq!(
        screen.update(&cx, UiInput::default()),
        ScreenResult::Done(ScreenAction::SaveLoad),
        "531"
    );
    assert_eq!(ti(&screen)[..4], [2, 2, 1, 1]);
    let asleep = ti(&screen);
    screen.resume(&cx);
    assert_eq!(ti(&screen), asleep, "708: its work stands");
    // What the load screen left: nothing loaded, and fade 3 off in its
    // mode.
    assert_eq!(screen.value("ti_fgsys") & 0x400, 0x400);
    assert_eq!(
        ["level", "add", "mode"].map(|key| screen.value(&format!("ti_fade3_{key}"))),
        [-1, 0, 1]
    );
    assert_eq!(screen.value("game_active"), 1, "the game's limiter still");
    assert_eq!(
        screen.update(&cx, UiInput::default()),
        ScreenResult::Continue
    );
    assert_eq!(ti(&screen)[..4], [1, 2, 1, 0], "709");
    assert_eq!(screen.value("ti_fgsys") & 0x400, 0);
    assert_eq!(screen.value("ti_fade_level"), 0x7FFF, "still black");
    screen.update(&cx, UiInput::default());
    assert_eq!(ti(&screen), [2, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0], "710");
    assert_eq!(screen.value("ti_kido"), 0x7C00);
    assert_eq!(screen.value("game_active"), 0, "the title's limiter");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_new_game_choice_opens_on_the_character_and_hands_off_leon() {
    let dir = std::env::temp_dir().join(format!("arklay-re2-choice-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cx = context(&dir);
    let mut cx_open = context(&dir);
    let mut screen =
        LuaScreen::open_screen("char_select", &[], ScreenAction::Quit, &mut cx_open).unwrap();
    for _ in 0..80 {
        screen.update(&cx, UiInput::default());
    }
    // ORIGINAL MODE's character rows (menu 5), with no white from the boot.
    assert_eq!(screen.value("ti_menu"), 5);
    let confirm = UiInput {
        confirm: true,
        ..UiInput::default()
    };
    // LEON, then NORMAL: the difficulty (menu 1), then the hand-off.
    screen.update(&cx, confirm);
    for _ in 0..40 {
        screen.update(&cx, UiInput::default());
    }
    assert_eq!(screen.value("ti_menu"), 1);
    screen.update(&cx, confirm);
    let mut action = None;
    for _ in 0..400 {
        if let crate::ui::ScreenResult::Done(done) = screen.update(&cx, UiInput::default()) {
            action = Some(done);
            break;
        }
    }
    assert_eq!(
        action,
        Some(ScreenAction::NewGame {
            character: 0,
            choice: Default::default(),
        })
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn arrange_mode_on_hard_hands_off_its_choice() {
    let dir = std::env::temp_dir().join(format!("arklay-re2-arrange-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cx = context(&dir);
    let mut cx_open = context(&dir);
    let mut screen =
        LuaScreen::open_screen("title", &[], ScreenAction::Quit, &mut cx_open).unwrap();
    // `newgame_arrange_hard.txt` from `Title_init` (the original's tick 189):
    // down twice to ARRANGE MODE, LEON, down to HARD, and its confirm.
    let down = UiInput {
        down: true,
        held_down: true,
        ..UiInput::default()
    };
    let confirm = UiInput {
        confirm: true,
        ..UiInput::default()
    };
    let presses = [
        (71, down),
        (111, down),
        (141, confirm),
        (191, confirm),
        (231, down),
        (261, confirm),
    ];
    let mut action = None;
    for tick in 0..700 {
        let input = presses
            .iter()
            .find(|(at, _)| *at == tick)
            .map_or_else(UiInput::default, |(_, input)| *input);
        if let crate::ui::ScreenResult::Done(done) = screen.update(&cx, input) {
            action = Some(done);
            break;
        }
    }
    assert_eq!(
        action,
        Some(ScreenAction::NewGame {
            character: 0,
            choice: crate::ui::NewGameChoice {
                easy: false,
                hard_mode: true,
                censorship_off: true,
            },
        })
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The title opened with config.ini's `SPECIAL` at `special`, run through
/// `title_special.txt`'s presses up to SPECIAL's first tick (the original's
/// 260, 290 and 320 down and 380 confirm, from `Title_init` at 189).
fn open_special(cx: &UiContext<'_>, dir: &std::path::Path, special: i64) -> LuaScreen {
    let mut cx_open = context(dir);
    let mut screen = LuaScreen::open_screen(
        "title",
        &[("special", ScreenArg::Int(special))],
        ScreenAction::Quit,
        &mut cx_open,
    )
    .unwrap();
    let down = UiInput {
        down: true,
        held_down: true,
        ..UiInput::default()
    };
    let confirm = UiInput {
        confirm: true,
        ..UiInput::default()
    };
    for tick in 0..=192 {
        let input = match tick {
            71 | 101 | 131 => down,
            191 => confirm,
            _ => UiInput::default(),
        };
        screen.update(cx, input);
    }
    screen
}

fn run(screen: &mut LuaScreen, cx: &UiContext<'_>, input: UiInput, ticks: usize) {
    screen.update(cx, input);
    for _ in 1..ticks {
        screen.update(cx, UiInput::default());
    }
}

fn press(direction: &str) -> UiInput {
    let mut input = UiInput::default();
    match direction {
        "down" => (input.down, input.held_down) = (true, true),
        "up" => (input.up, input.held_up) = (true, true),
        "left" => (input.left, input.held_left) = (true, true),
        "right" => (input.right, input.held_right) = (true, true),
        "confirm" => input.confirm = true,
        "start" => input.start = true,
        _ => unreachable!(),
    }
    input
}

#[test]
fn special_layouts_open_on_the_original_cursor() {
    let dir = std::env::temp_dir().join(format!("arklay-re2-special-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cx = context(&dir);
    for (special, cur) in [(0, 3), (1, 4), (2, 2), (5, 0), (15, 0), (0xFF, 0)] {
        let screen = open_special(&cx, &dir, special);
        assert_eq!(screen.value("ti_menu"), 3, "SPECIAL {special}");
        assert_eq!(screen.value("ti_cur"), cur, "SPECIAL {special}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn special_case_moves() {
    let dir = std::env::temp_dir().join(format!("arklay-re2-moves-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cx = context(&dir);
    for (special, moves) in [
        (2, &[("down", 3), ("up", 2), ("right", 4), ("left", 2)][..]),
        (
            5,
            &[("right", 2), ("down", 4), ("down", 3), ("up", 4), ("up", 0)][..],
        ),
        (1, &[("down", 3), ("up", 4)][..]),
    ] {
        let mut screen = open_special(&cx, &dir, special);
        run(&mut screen, &cx, UiInput::default(), 20);
        for (direction, cur) in moves {
            cx.cues.borrow_mut().clear();
            run(&mut screen, &cx, press(direction), 10);
            assert_eq!(
                screen.value("ti_cur"),
                *cur,
                "SPECIAL {special} {direction}"
            );
            assert_eq!(*cx.cues.borrow(), [UiCue::Core(4)]);
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_4th_survivors_title_builds_and_its_start_restarts_the_title() {
    let dir = std::env::temp_dir().join(format!("arklay-re2-4th-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cx = context(&dir);
    let mut screen = open_special(&cx, &dir, 15);
    run(&mut screen, &cx, UiInput::default(), 20);
    // The confirm on 4TH: the decide cue, then slot 2's fade, done on the
    // 17th tick, when `title_mv_state` goes to 3.
    cx.cues.borrow_mut().clear();
    run(&mut screen, &cx, press("confirm"), 1);
    assert_eq!(*cx.cues.borrow(), [UiCue::Core(6)]);
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(screen.value("ti_fade2_level"), 0x200 + 0x800);
    run(&mut screen, &cx, UiInput::default(), 16);
    assert_eq!(screen.value("ti_mv"), 3);
    assert_eq!(screen.value("ti_fgsys") & 9, 8);
    assert_eq!(screen.value("ti_fgstatus"), 0x0200_0000);
    // `Load_title_survivor` in one tick: core bank 0x12, slot 3 at 0x7C00.
    cx.cues.borrow_mut().clear();
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(screen.value("ti_mv"), 4);
    assert_eq!(screen.value("ti_disp"), 0);
    assert_eq!(screen.value("ti_fade3_level"), 0x7C00);
    assert_eq!(*cx.cues.borrow(), [UiCue::CoreBank(0x12)]);
    // The name over 64 ticks, the logo over 128, slot 3 over 91.
    run(&mut screen, &cx, UiInput::default(), 64);
    assert_eq!(screen.value("ti_menu"), 1);
    run(&mut screen, &cx, UiInput::default(), 128);
    assert_eq!(screen.value("ti_menu"), 2);
    cx.cues.borrow_mut().clear();
    run(&mut screen, &cx, UiInput::default(), 91);
    assert_eq!(screen.value("ti_disp"), 1);
    assert_eq!(*cx.cues.borrow(), [UiCue::Core(6)]);
    // START: the confirm's two cues and the white flash on slot 0, then the
    // fade to the hand-off on the 270th tick, where the title starts again.
    run(&mut screen, &cx, UiInput::default(), 10);
    cx.cues.borrow_mut().clear();
    run(&mut screen, &cx, press("start"), 1);
    assert_eq!(*cx.cues.borrow(), [UiCue::Core(0), UiCue::Core(1)]);
    assert_eq!(screen.value("ti_fade_level"), 0x7800 - 0x800);
    run(&mut screen, &cx, UiInput::default(), 268);
    assert_eq!(screen.value("ti_sub2"), 3);
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!((screen.value("ti_mv"), screen.value("ti_disp")), (2, 3));
    cx.cues.borrow_mut().clear();
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(screen.value("ti_mv"), 1);
    assert_eq!(screen.value("ti_fgsys") & 9, 0);
    assert_eq!(*cx.cues.borrow(), [UiCue::CoreBank(0x10)]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_4th_survivors_exit_goes_back_to_the_title_from_black() {
    let dir = std::env::temp_dir().join(format!("arklay-re2-4th-exit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cx = context(&dir);
    let mut screen = open_special(&cx, &dir, 15);
    run(&mut screen, &cx, UiInput::default(), 20);
    run(&mut screen, &cx, press("confirm"), 30);
    assert_eq!(screen.value("ti_mv"), 4);
    // A button in the build-up skips it: `Ti_Disp_add` 1, slot 3 lifting
    // from 0x5A00 by 0x1000 a tick.
    run(&mut screen, &cx, press("confirm"), 1);
    assert_eq!(screen.value("ti_disp"), 1);
    assert_eq!(screen.value("ti_add"), 0xF000);
    run(&mut screen, &cx, UiInput::default(), 20);
    run(&mut screen, &cx, press("down"), 5);
    assert_eq!(screen.value("ti_cur"), 1);
    cx.cues.borrow_mut().clear();
    run(&mut screen, &cx, press("confirm"), 1);
    assert_eq!(*cx.cues.borrow(), [UiCue::Core(5)]);
    let mut ticks = 1;
    while screen.value("ti_mv") != 1 {
        run(&mut screen, &cx, UiInput::default(), 1);
        ticks += 1;
        assert!(ticks < 20);
    }
    assert!(cx.cues.borrow().contains(&UiCue::CoreBank(0x10)));
    // `Title_init`'s branch from black.
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(screen.value("ti_disp"), 1);
    assert_eq!(screen.value("ti_kido"), 0x7C00);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The trace's `opt` field, less the arrows' blink counts: the task's step,
/// the state, the menu's row, the sound page's row, the wait, the speaker
/// mode, the quality and the two volumes.
fn opt(screen: &LuaScreen) -> [i64; 9] {
    [
        "opt_step",
        "opt_state",
        "opt_cursor",
        "opt_type",
        "opt_wait",
        "opt_speaker",
        "opt_quality",
        "opt_bgm",
        "opt_se",
    ]
    .map(|key| screen.value(key))
}

/// What the tick left of the frame: `"black"`, `"held"` (the frame before
/// stays up) or nothing for an ordinary draw.
fn frame(screen: &LuaScreen) -> Option<String> {
    screen.test_eval("return M.state.frame")
}

/// The background's picture handle, and the title's own.
fn backgrounds(screen: &LuaScreen) -> (Option<i64>, Option<i64>) {
    screen.test_eval("return M.state.background, M.state.title_background")
}

/// The arrow sprites the tick drew from `parts.tim`: each one's x and the
/// u of its cell.
fn arrow_sprites(screen: &LuaScreen) -> Vec<(i64, i64)> {
    let listed: String = screen.test_eval(
        "local out = {}
        for _, sprite in ipairs(M.state.sprites) do
            if sprite.page == 5 then out[#out + 1] = sprite.x .. ':' .. sprite.u end
        end
        return table.concat(out, ',')",
    );
    listed
        .split(',')
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            let (x, u) = entry.split_once(':').unwrap();
            (x.parse().unwrap(), u.parse().unwrap())
        })
        .collect()
}

/// One tick, and the cues it played.
fn tick_cues(screen: &mut LuaScreen, cx: &UiContext<'_>, input: UiInput) -> Vec<UiCue> {
    cx.cues.borrow_mut().clear();
    screen.update(cx, input);
    cx.cues.borrow().clone()
}

/// The title's OPTION confirmed, to the first tick its menu reads the pad.
fn open_option(cx: &UiContext<'_>, name: &str) -> (LuaScreen, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("arklay-re2-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut cx_open = context(&dir);
    let mut screen =
        LuaScreen::open_screen("title", &[], ScreenAction::Quit, &mut cx_open).unwrap();
    run(&mut screen, cx, UiInput::default(), 70);
    for _ in 0..4 {
        run(&mut screen, cx, press("down"), 20);
    }
    assert_eq!(screen.value("ti_cur"), 4, "OPTION's row");
    // `title_option_menu`: the confirm at 400, the task's first tick at 409
    // (black, with the fade in set) and the menu from 414.
    run(&mut screen, cx, press("confirm"), 9);
    assert_eq!(screen.value("opt_on"), 0);
    run(&mut screen, cx, UiInput::default(), 1);
    assert_eq!(screen.value("opt_on"), 1);
    assert_eq!(opt(&screen), [1, 0, 0, 0, 0, 2, 0, 100, 100]);
    assert_eq!(screen.value("game_active"), 1, "the game's limiter");
    assert_eq!(frame(&screen).as_deref(), Some("black"));
    let (room, title) = backgrounds(&screen);
    assert!(room.is_some() && title.is_some());
    assert_ne!(room, title, "the room's picture is loaded");
    assert_eq!(ti(&screen)[..2], [1, 1], "the title waits at its start");
    assert_eq!(screen.value("ti_fade_level"), 0x5000);
    assert_eq!(screen.value("ti_fade_add"), -0x1800);
    run(&mut screen, cx, UiInput::default(), 4);
    assert_eq!(screen.value("opt_state"), 0, "under the fade in");
    run(&mut screen, cx, UiInput::default(), 1);
    assert_eq!(screen.value("opt_state"), 1);
    assert_eq!(frame(&screen), None);
    (screen, dir)
}

/// The option menu's rows move on the keys' presses alone, stop at both
/// ends, and KEYCONFIG, whose page isn't ported, takes no decide.
#[test]
fn the_option_menu_moves_between_its_four_rows() {
    let dir = std::env::temp_dir();
    let cx = context(&dir);
    let (mut screen, dir) = open_option(&cx, "option-menu");
    assert!(tick_cues(&mut screen, &cx, press("up")).is_empty());
    assert_eq!(screen.value("opt_cursor"), 0);
    assert!(tick_cues(&mut screen, &cx, press("confirm")).is_empty());
    assert_eq!(screen.value("opt_state"), 1, "KEYCONFIG");
    // Down held: one row, on its press.
    let held = UiInput {
        held_down: true,
        ..UiInput::default()
    };
    assert_eq!(tick_cues(&mut screen, &cx, press("down")), [UiCue::Core(4)]);
    for _ in 0..30 {
        assert!(tick_cues(&mut screen, &cx, held).is_empty());
    }
    assert_eq!(screen.value("opt_cursor"), 1);
    run(&mut screen, &cx, UiInput::default(), 2);
    for row in 2..=3 {
        run(&mut screen, &cx, press("down"), 2);
        assert_eq!(screen.value("opt_cursor"), row);
    }
    assert!(tick_cues(&mut screen, &cx, press("down")).is_empty());
    assert_eq!(screen.value("opt_cursor"), 3);
    let _ = std::fs::remove_dir_all(&dir);
}

/// EXIT plays the cancel cue, fades out over 32 ticks, and two ticks after
/// the fade the title starts again from black (`title_option_menu`'s 660 to
/// 696).
#[test]
fn the_option_menus_exit_goes_back_to_the_title() {
    let dir = std::env::temp_dir();
    let cx = context(&dir);
    let (mut screen, dir) = open_option(&cx, "option-exit");
    for _ in 0..3 {
        run(&mut screen, &cx, press("down"), 2);
    }
    assert_eq!(
        tick_cues(&mut screen, &cx, press("confirm")),
        [UiCue::Core(5)]
    );
    assert_eq!(screen.value("opt_state"), 6);
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(screen.value("opt_state"), 0x16);
    assert_eq!(screen.value("ti_fade_add"), 0x400);
    run(&mut screen, &cx, UiInput::default(), 31);
    assert_eq!(screen.value("opt_state"), 0x16);
    assert_eq!(frame(&screen), None, "the fade's last frame is drawn");
    // The two ticks after the fade present nothing, and the one after
    // them is black.
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(screen.value("opt_state"), 0x17);
    assert_eq!(frame(&screen).as_deref(), Some("held"));
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(opt(&screen)[..2], [2, 0]);
    assert_eq!(frame(&screen).as_deref(), Some("held"));
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(screen.value("opt_on"), 0, "the task has ended");
    assert_eq!(frame(&screen).as_deref(), Some("black"));
    assert_eq!(ti(&screen)[..2], [1, 1]);
    let (room, title) = backgrounds(&screen);
    assert!(room.is_some() && room != title, "the room's still");
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(ti(&screen)[..2], [2, 1], "`Title_init`'s branch from black");
    assert_eq!(screen.value("ti_kido"), 0x7C00);
    let (background, title) = backgrounds(&screen);
    assert_eq!(background, title, "the title's picture is back");
    let _ = std::fs::remove_dir_all(&dir);
}

/// START and the cancel key leave the menu the same way, and a decide on
/// START's tick is not read.
#[test]
fn start_and_cancel_leave_the_option_menu() {
    let dir = std::env::temp_dir();
    let cx = context(&dir);
    for key in ["start", "cancel"] {
        let (mut screen, dir) = open_option(&cx, "option-leave");
        run(&mut screen, &cx, press("down"), 2);
        run(&mut screen, &cx, press("down"), 2);
        let mut input = UiInput {
            confirm: true,
            ..UiInput::default()
        };
        match key {
            "start" => input.start = true,
            _ => input.cancel = true,
        }
        let cues = tick_cues(&mut screen, &cx, input);
        assert_eq!(cues.last(), Some(&UiCue::Core(5)), "{key}");
        assert_eq!(screen.value("opt_state"), 0x16, "{key}");
        assert_eq!(cues.len(), if key == "start" { 1 } else { 2 }, "{key}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// MONITOR TUNING shows its picture until the decide key or the cancel key,
/// each with its cue.
#[test]
fn monitor_tuning_shows_its_picture_until_a_key() {
    let dir = std::env::temp_dir();
    let cx = context(&dir);
    let (mut screen, dir) = open_option(&cx, "option-monitor");
    let (room, title) = backgrounds(&screen);
    run(&mut screen, &cx, press("down"), 2);
    for (key, cue) in [("confirm", 6), ("cancel", 5)] {
        assert_eq!(
            tick_cues(&mut screen, &cx, press("confirm")),
            [UiCue::Core(6)]
        );
        assert_eq!(screen.value("opt_state"), 3);
        run(&mut screen, &cx, UiInput::default(), 1);
        assert_eq!(screen.value("opt_state"), 7);
        let (picture, _) = backgrounds(&screen);
        assert!(picture.is_some() && picture != room && picture != title);
        assert!(tick_cues(&mut screen, &cx, press("up")).is_empty());
        let mut input = UiInput::default();
        match key {
            "confirm" => input.confirm = true,
            _ => input.cancel = true,
        }
        assert_eq!(tick_cues(&mut screen, &cx, input), [UiCue::Core(cue)]);
        assert_eq!(opt(&screen)[1..3], [1, 1], "back on its row");
        assert_eq!(backgrounds(&screen).0, room, "and the room's picture");
        run(&mut screen, &cx, UiInput::default(), 1);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// The SOUND page: left and right change a row's value while they are held,
/// once every six ticks; a volume steps by one, or by ten with the decide
/// key held too, within 1 to 100; and the values outlast the page.
#[test]
fn the_sound_page_changes_its_values_on_the_held_keys() {
    let dir = std::env::temp_dir();
    let cx = context(&dir);
    let (mut screen, dir) = open_option(&cx, "option-sound");
    run(&mut screen, &cx, press("down"), 2);
    run(&mut screen, &cx, press("down"), 2);
    assert_eq!(
        tick_cues(&mut screen, &cx, press("confirm")),
        [UiCue::Core(6)]
    );
    assert_eq!(opt(&screen)[1..4], [4, 2, 0]);
    let left = UiInput {
        held_left: true,
        ..UiInput::default()
    };
    let right = UiInput {
        held_right: true,
        ..UiInput::default()
    };
    let with_decide = |input: UiInput| UiInput {
        held_confirm: true,
        ..input
    };
    // The mode: monaural on the press, the wait's five ticks, stereo again
    // on the sixth.
    assert_eq!(tick_cues(&mut screen, &cx, left), [UiCue::Core(4)]);
    assert_eq!(opt(&screen)[1..6], [8, 2, 0, 5, 1]);
    for wait in (0..5).rev() {
        assert!(tick_cues(&mut screen, &cx, left).is_empty());
        assert_eq!(screen.value("opt_wait"), wait);
    }
    assert_eq!(screen.value("opt_state"), 4);
    assert_eq!(tick_cues(&mut screen, &cx, left), [UiCue::Core(4)]);
    assert_eq!(screen.value("opt_speaker"), 2);
    run(&mut screen, &cx, UiInput::default(), 6);
    // The quality: right goes high, medium, low and round.
    run(&mut screen, &cx, press("down"), 2);
    for quality in [1, 2, 0] {
        run(&mut screen, &cx, right, 1);
        assert_eq!(screen.value("opt_quality"), quality);
        run(&mut screen, &cx, UiInput::default(), 6);
    }
    for quality in [2, 1, 0] {
        run(&mut screen, &cx, left, 1);
        assert_eq!(screen.value("opt_quality"), quality);
        run(&mut screen, &cx, UiInput::default(), 6);
    }
    // The music's volume: at 100 right does nothing; left by one, then by
    // ten with the decide key, down to 1 and no lower.
    run(&mut screen, &cx, press("down"), 2);
    assert!(tick_cues(&mut screen, &cx, right).is_empty());
    assert_eq!(screen.value("opt_state"), 4);
    for _ in 0..12 {
        screen.update(&cx, left);
    }
    assert_eq!(screen.value("opt_bgm"), 98);
    run(&mut screen, &cx, UiInput::default(), 6);
    for _ in 0..12 * 6 {
        screen.update(&cx, with_decide(left));
    }
    assert_eq!(screen.value("opt_bgm"), 1);
    assert!(tick_cues(&mut screen, &cx, with_decide(left)).is_empty());
    run(&mut screen, &cx, with_decide(right), 1);
    assert_eq!(screen.value("opt_bgm"), 11);
    run(&mut screen, &cx, UiInput::default(), 6);
    // The effects' volume has the same rule.
    run(&mut screen, &cx, press("down"), 2);
    assert!(tick_cues(&mut screen, &cx, right).is_empty());
    assert_eq!(opt(&screen)[1..5], [4, 2, 3, 0], "no wait at 100");
    run(&mut screen, &cx, with_decide(left), 1);
    assert_eq!(screen.value("opt_se"), 90);
    run(&mut screen, &cx, UiInput::default(), 6);
    run(&mut screen, &cx, with_decide(right), 1);
    run(&mut screen, &cx, UiInput::default(), 6);
    run(&mut screen, &cx, right, 1);
    assert_eq!(screen.value("opt_se"), 100, "no higher");
    run(&mut screen, &cx, UiInput::default(), 6);
    // EXIT takes the decide; the rows above don't.
    run(&mut screen, &cx, press("up"), 2);
    assert!(tick_cues(&mut screen, &cx, press("confirm")).is_empty());
    run(&mut screen, &cx, press("down"), 2);
    run(&mut screen, &cx, press("down"), 2);
    assert_eq!(screen.value("opt_type"), 4);
    assert!(tick_cues(&mut screen, &cx, press("down")).is_empty());
    assert_eq!(
        tick_cues(&mut screen, &cx, press("confirm")),
        [UiCue::Core(6)]
    );
    assert_eq!(opt(&screen), [1, 1, 2, 0, 0, 2, 0, 11, 100]);
    // The page again, left by the cancel key.
    run(&mut screen, &cx, UiInput::default(), 1);
    run(&mut screen, &cx, press("confirm"), 2);
    assert_eq!(screen.value("opt_state"), 4);
    let cancel = UiInput {
        cancel: true,
        ..UiInput::default()
    };
    assert_eq!(tick_cues(&mut screen, &cx, cancel), [UiCue::Core(5)]);
    assert_eq!(opt(&screen)[1..4], [1, 2, 0]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The arrows about the cursor's row show 20 ticks of each 30, or of each
/// 31 on EXIT, and a row's count stands while the cursor is elsewhere
/// (`title_option_sound`'s 480 to 509).
#[test]
fn the_sound_pages_arrows_blink_20_ticks_of_a_period() {
    let dir = std::env::temp_dir();
    let cx = context(&dir);
    let (mut screen, dir) = open_option(&cx, "option-blink");
    run(&mut screen, &cx, press("down"), 2);
    run(&mut screen, &cx, press("down"), 2);
    run(&mut screen, &cx, press("confirm"), 1);
    let counts: Vec<i64> = (0..31)
        .map(|_| {
            let count = screen.value("opt_blink_mode");
            screen.update(&cx, UiInput::default());
            count
        })
        .collect();
    assert_eq!(counts[..3], [1, 2, 3]);
    assert_eq!(counts[28..], [29, 0, 1]);
    run(&mut screen, &cx, press("down"), 5);
    let stood = screen.value("opt_blink_mode");
    run(&mut screen, &cx, UiInput::default(), 10);
    assert_eq!(screen.value("opt_blink_mode"), stood);
    assert_eq!(screen.value("opt_blink_quality"), 15);
    // EXIT's arrows are sprites, and its count keeps the original's 31
    // ticks (`title_option_sound`'s 1160 to 1221).
    for _ in 0..3 {
        run(&mut screen, &cx, press("down"), 2);
    }
    assert_eq!(screen.value("opt_type"), 4);
    let mut counts = Vec::new();
    for _ in 0..40 {
        screen.update(&cx, UiInput::default());
        let count = screen.value("opt_blink_exit");
        // The tick drew by the count before its step.
        let shown = (1..=20).contains(&count);
        // About EXIT at 132: a cell before it and after its 53 pixels.
        let arrows: &[(i64, i64)] = if shown {
            &[(118, 0xD1), (185, 0xDF)]
        } else {
            &[]
        };
        assert_eq!(arrow_sprites(&screen), arrows, "{count}");
        counts.push(count);
    }
    let wrap = counts.iter().position(|&count| count == 0).unwrap();
    assert_eq!(counts[wrap - 1], 30);
    assert_eq!(counts[wrap + 1], 1);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The page's first and last rows end the tick on a key past them, as the
/// original's early returns do; both directions held at a volume's end step
/// it down and back up; and the cancel key with a direction held leaves for
/// the menu's state, where the direction changes the mode and so stays on
/// the page.
#[test]
fn the_sound_pages_keys_together() {
    let dir = std::env::temp_dir();
    let cx = context(&dir);
    let (mut screen, dir) = open_option(&cx, "option-together");
    run(&mut screen, &cx, press("down"), 2);
    run(&mut screen, &cx, press("down"), 2);
    run(&mut screen, &cx, press("confirm"), 2);
    let up_and_left = UiInput {
        held_left: true,
        ..press("up")
    };
    assert!(tick_cues(&mut screen, &cx, up_and_left).is_empty());
    assert_eq!(opt(&screen)[1..6], [4, 2, 0, 0, 2], "MODE stands");
    run(&mut screen, &cx, UiInput::default(), 1);
    run(&mut screen, &cx, press("down"), 2);
    run(&mut screen, &cx, press("down"), 2);
    let both = UiInput {
        held_left: true,
        held_right: true,
        ..UiInput::default()
    };
    assert_eq!(
        tick_cues(&mut screen, &cx, both),
        [UiCue::Core(4), UiCue::Core(4)]
    );
    assert_eq!(opt(&screen)[1..5], [8, 2, 2, 5]);
    assert_eq!(screen.value("opt_bgm"), 100);
    run(&mut screen, &cx, UiInput::default(), 6);
    run(&mut screen, &cx, press("down"), 2);
    run(&mut screen, &cx, press("down"), 2);
    assert_eq!(screen.value("opt_type"), 4);
    let down_and_cancel = UiInput {
        cancel: true,
        ..press("down")
    };
    assert!(tick_cues(&mut screen, &cx, down_and_cancel).is_empty());
    assert_eq!(opt(&screen)[1..4], [4, 2, 4], "EXIT stands");
    run(&mut screen, &cx, UiInput::default(), 1);
    let cancel_and_left = UiInput {
        cancel: true,
        held_left: true,
        ..UiInput::default()
    };
    assert_eq!(
        tick_cues(&mut screen, &cx, cancel_and_left),
        [UiCue::Core(5), UiCue::Core(4)]
    );
    assert_eq!(opt(&screen)[1..6], [8, 2, 0, 5, 1], "the page, on MODE");
    run(&mut screen, &cx, UiInput::default(), 5);
    assert_eq!(screen.value("opt_state"), 4);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The screen's work is static in the original: a second visit opens with
/// the row the first one left, until `Config_main`'s first tick clears it,
/// and the arrows' counts go on from where they stood
/// (`title_option_sound`'s 1439, 1440 and 1480).
#[test]
fn a_second_visit_keeps_the_first_ones_work() {
    let dir = std::env::temp_dir();
    let cx = context(&dir);
    let (mut screen, dir) = open_option(&cx, "option-again");
    run(&mut screen, &cx, press("down"), 2);
    run(&mut screen, &cx, press("down"), 2);
    run(&mut screen, &cx, press("confirm"), 8);
    let count = screen.value("opt_blink_mode");
    assert_ne!(count, 0);
    let cancel = UiInput {
        cancel: true,
        ..UiInput::default()
    };
    run(&mut screen, &cx, cancel, 2);
    assert_eq!(opt(&screen)[1..3], [1, 2]);
    run(&mut screen, &cx, cancel, 90);
    assert_eq!(screen.value("opt_on"), 0);
    for _ in 0..4 {
        run(&mut screen, &cx, press("down"), 10);
    }
    assert_eq!(screen.value("ti_cur"), 4, "OPTION's row");
    screen.update(&cx, press("confirm"));
    for _ in 0..20 {
        if screen.value("opt_on") == 1 {
            break;
        }
        screen.update(&cx, UiInput::default());
    }
    assert_eq!(opt(&screen)[..3], [1, 0, 2], "the first visit's row");
    assert_eq!(frame(&screen).as_deref(), Some("black"));
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(opt(&screen)[..3], [1, 0, 0]);
    run(&mut screen, &cx, UiInput::default(), 5);
    assert_eq!(screen.value("opt_state"), 1);
    run(&mut screen, &cx, press("down"), 2);
    run(&mut screen, &cx, press("down"), 2);
    assert_eq!(screen.value("opt_blink_mode"), count);
    run(&mut screen, &cx, press("confirm"), 1);
    assert_eq!(screen.value("opt_blink_mode"), count + 1);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A volume's digit is printed where the original's whole cell puts its
/// glyph: in the middle of the cell, but the 6, a pixel left of it (the
/// print's trims, from `title_option_sound`'s frames).
#[test]
fn a_volumes_digit_sits_where_its_cell_puts_it() {
    let dir = std::env::temp_dir();
    let cx = context(&dir);
    let (screen, dir) = open_option(&cx, "option-digit");
    let place = |digit: &str, width: i64| -> i64 {
        screen.test_eval(&format!(
            "return M.option_digit_x(0x100, '{digit}', {width})"
        ))
    };
    assert_eq!(place("0", 9), 0x103);
    assert_eq!(place("1", 6), 0x104);
    assert_eq!(place("6", 9), 0x102);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The original's pad has run since boot, so a direction held as the title
/// opens is no press; one pressed after is.
#[test]
fn a_direction_held_as_the_title_opens_is_no_press() {
    let dir = std::env::temp_dir().join(format!("arklay-re2-title-held-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut cx_open = context(&dir);
    let mut screen =
        LuaScreen::open_screen("title", &[], ScreenAction::Quit, &mut cx_open).unwrap();
    let cx = context(&dir);
    let held = UiInput {
        held_down: true,
        ..UiInput::default()
    };
    screen.update(&cx, held);
    assert_eq!(screen.test_eval::<i64>("return M.state.edge"), 0);
    screen.update(&cx, UiInput::default());
    screen.update(&cx, held);
    assert_eq!(screen.test_eval::<i64>("return M.state.edge"), 0x4000);
    let _ = std::fs::remove_dir_all(&dir);
}
