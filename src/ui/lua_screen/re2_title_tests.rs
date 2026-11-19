//! RE 2's title (`games/re2/ui/title.lua`) over a pack holding only the
//! script and its backgrounds, against the original's trace
//! (`oracle/re2.md`, "The title").

use super::*;

use crate::pack::Pack;
use crate::ui::{Screen, ScreenAction, ScreenResult, UiCue, UiInput};

/// The title's pack; `prologue_art` adds what the prologue shows, which is
/// what makes a new game run it.
fn build_pack(prologue_art: bool) -> Pack {
    let mut writer = crate::pack::PackWriter::new();
    for (path, source) in [
        (
            "ui/cr_text.lua",
            include_str!("../../../games/re2/ui/cr_text.lua"),
        ),
        (
            "ui/prologue.lua",
            include_str!("../../../games/re2/ui/prologue.lua"),
        ),
        (
            "ui/title.lua",
            include_str!("../../../games/re2/ui/title.lua"),
        ),
        (
            "ui/char_select.lua",
            include_str!("../../../games/re2/ui/char_select.lua"),
        ),
    ] {
        writer.add(path, source.as_bytes().to_vec()).unwrap();
    }
    // The backgrounds, so a test can tell which one is up.
    for (index, stem) in ["title/title_bg", "option/room", "option/color2"]
        .into_iter()
        .enumerate()
    {
        let picture = super::tests::solid_bmp(4, 4, [index as u8; 3]);
        writer.add(&format!("ui/{stem}.bmp"), picture).unwrap();
    }
    if prologue_art {
        for stem in ["open10", "open11"] {
            let picture = super::tests::solid_bmp(4, 4, [0x40; 3]);
            writer
                .add(&format!("ui/prologue/{stem}.bmp"), picture)
                .unwrap();
        }
        for (sheet, count) in [("open00", 4), ("open01", 4), ("open02", 2)] {
            for index in 0..count {
                writer
                    .add(
                        &format!("ui/prologue/{sheet}_{index}.tim"),
                        super::tests::test_tim(),
                    )
                    .unwrap();
            }
        }
    }
    Pack::from_bytes(writer.to_bytes().unwrap()).unwrap()
}

fn pack() -> &'static Pack {
    use std::sync::OnceLock;
    static PACK: OnceLock<Pack> = OnceLock::new();
    PACK.get_or_init(|| build_pack(false))
}

fn prologue_pack() -> &'static Pack {
    use std::sync::OnceLock;
    static PACK: OnceLock<Pack> = OnceLock::new();
    PACK.get_or_init(|| build_pack(true))
}

fn context<'a>(dir: &'a std::path::Path) -> UiContext<'a> {
    context_over(pack(), dir)
}

fn context_over<'a>(pack: &'static Pack, dir: &'a std::path::Path) -> UiContext<'a> {
    UiContext {
        statics: None,
        authentic: false,
        pack,
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

/// The title up to its hand-off to the load screen: the confirm at 260 and
/// the hand-off on 531, with the task's step at 1.
fn asleep_under_the_load_screen(dir: &std::path::Path) -> LuaScreen {
    let cx = context(dir);
    let mut cx_open = context(dir);
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
    screen
}

/// A load (`title_load_game`): the title wakes with nothing to clear, its
/// step goes to 3 on that tick (810) and it hands the game over on the
/// next, as a new game's choice does.
#[test]
fn a_load_wakes_the_title_into_its_hand_over() {
    let dir = std::env::temp_dir().join(format!("arklay-re2-loaded-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cx = context(&dir);
    let mut screen = asleep_under_the_load_screen(&dir);
    let asleep = ti(&screen);
    assert!(screen.continues());
    screen.resume_loaded(&cx);
    assert_eq!(ti(&screen), asleep, "809: its work stands");
    assert_eq!(screen.value("ti_fgsys") & 0x400, 0);
    assert_eq!(
        screen.update(&cx, UiInput::default()),
        ScreenResult::Continue
    );
    assert_eq!(ti(&screen)[..4], [2, 3, 1, 0], "810");
    assert_eq!(screen.value("ti_fgsys") & 0x20000, 0);
    assert_eq!(
        screen.update(&cx, UiInput::default()),
        ScreenResult::Done(ScreenAction::Continue)
    );
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
    let mut screen = asleep_under_the_load_screen(&dir);
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

/// A title opened with what `cx` keeps, and its OPTION confirmed, to the
/// option task's first tick (black, with the fade in set).
fn open_option_to_its_first_tick(
    cx: &UiContext<'_>,
    name: &str,
) -> (LuaScreen, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("arklay-re2-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut cx_open = context(&dir);
    cx_open.statics = cx.statics;
    let mut screen =
        LuaScreen::open_screen("title", &[], ScreenAction::Quit, &mut cx_open).unwrap();
    run(&mut screen, cx, UiInput::default(), 70);
    for _ in 0..4 {
        run(&mut screen, cx, press("down"), 20);
    }
    assert_eq!(screen.value("ti_cur"), 4, "OPTION's row");
    // `title_option_menu`: the confirm at 400, the task's first tick at 409
    // and the menu from 414.
    run(&mut screen, cx, press("confirm"), 9);
    assert_eq!(screen.value("opt_on"), 0);
    run(&mut screen, cx, UiInput::default(), 1);
    assert_eq!(screen.value("opt_on"), 1);
    (screen, dir)
}

/// The title's OPTION confirmed, to the first tick its menu reads the pad.
fn open_option(cx: &UiContext<'_>, name: &str) -> (LuaScreen, std::path::PathBuf) {
    let (mut screen, dir) = open_option_to_its_first_tick(cx, name);
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

/// The option screen's work and its sound values are the process's in the
/// original: a title opened again (after a game) finds them as the last one
/// left them, where a context with nowhere to keep them starts over.
#[test]
fn the_option_screens_work_outlasts_the_title() {
    let dir = std::env::temp_dir();
    let statics = crate::ui::ScreenStatics::default();
    for kept in [true, false] {
        let mut cx = context(&dir);
        if kept {
            cx.statics = Some(&statics);
        }
        let (mut screen, folder) = open_option(&cx, "option-kept");
        run(&mut screen, &cx, press("down"), 2);
        run(&mut screen, &cx, press("down"), 2);
        run(&mut screen, &cx, press("confirm"), 4);
        // MONO, and the mode's arrows four ticks into their blink.
        run(&mut screen, &cx, press("left"), 7);
        let left = opt(&screen);
        assert_eq!(left[5], 1, "MONO");
        let blink = screen.value("opt_blink_mode");
        assert_ne!(blink, 0);
        let _ = std::fs::remove_dir_all(&folder);
        // A title opened anew: its option screen's black tick.
        let (again, folder) = open_option_to_its_first_tick(&cx, "option-kept-again");
        let found = opt(&again);
        if kept {
            assert_eq!(found[2], left[2], "the row, until `Config_main` clears it");
            assert_eq!(found[5..], left[5..], "the sound values");
            assert_eq!(again.value("opt_blink_mode"), blink);
        } else {
            assert_eq!(found, [1, 0, 0, 0, 0, 2, 0, 100, 100]);
            assert_eq!(again.value("opt_blink_mode"), 0);
        }
        let _ = std::fs::remove_dir_all(&folder);
    }
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

/// The SOUND page's two volumes are the settings' music and effects volumes
/// when the engine gives them (`on_volumes`), held to the page's 1 to 100,
/// and a change on the page is published for the engine to take. A title
/// told nothing publishes neither.
#[test]
fn the_sound_pages_volumes_are_the_settings_when_they_are_given() {
    let dir = std::env::temp_dir();
    let cx = context(&dir);
    let (mut screen, dir) = open_option(&cx, "option-volumes");
    let volumes = |screen: &LuaScreen| {
        ["music_volume", "effects_volume", "opt_bgm", "opt_se"].map(|key| screen.value(key))
    };
    assert_eq!(volumes(&screen), [0, 0, 100, 100], "its own, unpublished");
    screen.signal_with("on_volumes", (40i64, 0i64), &cx);
    assert_eq!(volumes(&screen)[..2], [40, 1], "no page volume is 0");
    // SOUND, then down to the music's volume and left by one.
    run(&mut screen, &cx, press("down"), 2);
    run(&mut screen, &cx, press("down"), 2);
    run(&mut screen, &cx, press("confirm"), 1);
    assert_eq!(volumes(&screen), [40, 1, 40, 1]);
    run(&mut screen, &cx, press("down"), 2);
    run(&mut screen, &cx, press("down"), 2);
    let left = UiInput {
        held_left: true,
        ..UiInput::default()
    };
    screen.update(&cx, left);
    assert_eq!(volumes(&screen), [39, 1, 39, 1]);
    // The settings changed elsewhere: the page follows.
    screen.signal_with("on_volumes", (100i64, 55i64), &cx);
    assert_eq!(volumes(&screen)[..2], [100, 55]);
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(volumes(&screen), [100, 55, 100, 55]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The new-game choice's entry runs the title's script, so it takes what
/// the engine tells the title: the settings' volumes and a film's end.
#[test]
fn the_choice_entry_takes_the_titles_hooks() {
    let dir = prologue_dir("choice-hooks");
    let cx = context(&dir);
    let mut cx_open = context(&dir);
    let mut screen =
        LuaScreen::open_screen("char_select", &[], ScreenAction::Quit, &mut cx_open).unwrap();
    assert!(screen.plays_films());
    screen.signal_with("on_volumes", (70i64, 30i64), &cx);
    assert_eq!(screen.value("music_volume"), 70);
    assert_eq!(screen.value("effects_volume"), 30);
    assert!(!screen.failed());
    let _ = std::fs::remove_dir_all(&dir);
}

// ------------------------------------------------------- the prologue

/// The trace's `pro` field's step, count and skip, with the sequence and the
/// routine (`mv`, `r`).
fn pro(screen: &LuaScreen) -> [i64; 5] {
    ["pro_mv", "pro_step", "pro_count", "pro_r", "pro_skip"].map(|key| screen.value(key))
}

fn confirm() -> UiInput {
    UiInput {
        confirm: true,
        ..UiInput::default()
    }
}

/// What a run of the prologue left: the tick the title handed the game over
/// on, its action, and the films it asked for on the way (tick, id, Claire's).
struct Prologue {
    end: u32,
    action: ScreenAction,
    films: Vec<(u32, u8, bool)>,
}

/// A title over the prologue's art, run through `prologue_leon.txt`'s choice
/// (down on 260, confirms on 300, 360 and 420; with `claire`, right on 340)
/// and then `presses`, from `Title_init` (the original's tick 189) until it
/// hands the game over. A film it asks for is over at once, as in the
/// recordings, by a press when it is `skipped_film`. `each` sees every tick
/// after its update, with the cues it played.
fn run_prologue(
    dir: &std::path::Path,
    claire: bool,
    presses: &[(u32, UiInput)],
    skipped_film: Option<u8>,
    mut each: impl FnMut(u32, &LuaScreen, &[UiCue]),
) -> Prologue {
    let cx = context_over(prologue_pack(), dir);
    let mut cx_open = context_over(prologue_pack(), dir);
    let mut screen =
        LuaScreen::open_screen("title", &[], ScreenAction::Quit, &mut cx_open).unwrap();
    assert!(screen.plays_films());
    let down = UiInput {
        down: true,
        held_down: true,
        ..UiInput::default()
    };
    let right = UiInput {
        right: true,
        held_right: true,
        ..UiInput::default()
    };
    let mut films = Vec::new();
    for tick in 189..5000 {
        let input = match tick {
            260 => down,
            340 if claire => right,
            300 | 360 | 420 => confirm(),
            _ => presses
                .iter()
                .find(|(at, _)| *at == tick)
                .map_or_else(UiInput::default, |(_, input)| *input),
        };
        cx.cues.borrow_mut().clear();
        let result = screen.update(&cx, input);
        each(tick, &screen, &cx.cues.borrow());
        match result {
            ScreenResult::Continue => {}
            ScreenResult::Done(ScreenAction::Film { id, claire }) => {
                films.push((tick, id, claire));
                screen.resume_film(&cx, skipped_film == Some(id));
            }
            ScreenResult::Done(action) => {
                return Prologue {
                    end: tick,
                    action,
                    films,
                };
            }
        }
    }
    panic!("the title never handed over");
}

fn prologue_dir(label: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("arklay-re2-{label}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// `prologue_leon`: `Title_game_init` starts the task two ticks after the
/// hand-over (692), its first tick is 693, the narration's lines start on
/// their counts, the two films are asked for on 4025 and 4026, and the
/// game starts when the task ends on 4028. A press under the last fade
/// (3800) skips nothing.
#[test]
fn a_new_game_runs_the_prologue_before_it_starts() {
    let dir = prologue_dir("prologue");
    let mut voices = Vec::new();
    let mut music = Vec::new();
    let presses = [(3800, confirm())];
    let run = run_prologue(&dir, false, &presses, None, |tick, screen, cues| {
        match tick {
            690 => assert_eq!(screen.value("pro_on"), 0),
            691 => assert_eq!(ti(screen)[..4], [2, 3, 1, 0]),
            692 => {
                assert_eq!(ti(screen)[..4], [2, 3, 0, 2]);
                assert_eq!(screen.value("ti_fade_level"), -1, "fade 0 is off");
            }
            693 => {
                assert_eq!(ti(screen)[..4], [2, 3, 0, 3]);
                assert_eq!(pro(screen), [0, 1, 1, 1, 0]);
                assert_eq!(screen.value("pro_slot"), 3);
            }
            // Step 1's last count starts step 2, and so on to the films'.
            1516 => assert_eq!(pro(screen), [0, 2, 1, 2, 0]),
            2421 => assert_eq!(pro(screen)[..3], [0, 3, 1]),
            2772 => assert_eq!(pro(screen)[..3], [0, 4, 1]),
            3352 => assert_eq!(pro(screen)[..3], [0, 5, 1]),
            3797 => {
                assert_eq!(screen.value("pro_skip_ok"), 0);
                assert_eq!(screen.value("pro_slot"), 0);
            }
            3801 => assert_eq!(pro(screen)[4], 2, "no skip under the last fade"),
            3925 => assert_eq!(pro(screen)[..3], [0, 6, 1]),
            4025 => assert_eq!(pro(screen)[..3], [0, 7, 1]),
            4026 => assert_eq!(pro(screen)[..3], [0, 9, 1]),
            4027 => assert_eq!(pro(screen), [4, 9, 2, 3, 2]),
            _ => {}
        }
        for cue in cues {
            match cue {
                UiCue::Voice { player, line } => voices.push((tick, *player, *line)),
                UiCue::VoiceStop => voices.push((tick, 0xFF, 0)),
                UiCue::Music(_) | UiCue::MusicFade | UiCue::MusicStop => music.push((tick, *cue)),
                _ => {}
            }
        }
    });
    assert_eq!(run.end, 4028);
    assert_eq!(
        run.action,
        ScreenAction::NewGame {
            character: 0,
            choice: Default::default(),
        }
    );
    // `Init_movie_work(3)` and `(0xF)`, each over on its own tick.
    assert_eq!(run.films, [(4025, 3, false), (4026, 0xF, false)]);
    // `Xa_play(1, n)` on each step's count, and `Xa_stop` at the end.
    assert_eq!(
        voices,
        [
            (693 + 420, 0, 1),
            (1516 + 64, 0, 2),
            (2421 + 94, 0, 3),
            (2772 + 94, 0, 4),
            (3352 + 96, 0, 5),
            (4028, 0xFF, 0),
        ]
    );
    assert_eq!(
        music,
        [
            (693, UiCue::Music(0x2B)),
            (3352 + 445, UiCue::MusicFade),
            (4028, UiCue::MusicStop),
        ]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A press that ends the first film (`0x9814B5`) leaves the second out:
/// step 7 goes to the task's end, a tick sooner.
#[test]
fn a_skipped_first_film_takes_the_second_with_it() {
    let dir = prologue_dir("pro-film-skip");
    let run = run_prologue(&dir, false, &[], Some(3), |tick, screen, _| {
        if tick == 4026 {
            assert_eq!(pro(screen), [4, 9, 1, 3, 2]);
        }
    });
    assert_eq!(run.films, [(4025, 3, false)]);
    assert_eq!(run.end, 4027);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Claire's first scenario is `Prologue_mv`'s entry 1, the same routine:
/// her bank's lines, her films, and the entry a skip comes back to.
#[test]
fn claires_prologue_is_entry_1_with_her_lines_and_films() {
    let dir = prologue_dir("pro-claire");
    let mut voices = Vec::new();
    let presses = [(1300, confirm())];
    let run = run_prologue(&dir, true, &presses, None, |tick, screen, cues| {
        match tick {
            693 => assert_eq!(pro(screen), [1, 1, 1, 1, 0]),
            1300 => assert_eq!(pro(screen)[..2], [4, 6]),
            1428 => assert_eq!(pro(screen), [1, 6, 0x5A, 1, 2]),
            _ => {}
        }
        voices.extend(cues.iter().filter_map(|cue| match cue {
            UiCue::Voice { player, line } => Some((tick, *player, *line)),
            _ => None,
        }));
    });
    assert_eq!(voices, [(693 + 420, 1, 1)]);
    assert_eq!(run.films, [(1439, 3, true), (1440, 0xF, true)]);
    assert_eq!(run.end, 1442);
    assert_eq!(
        run.action,
        ScreenAction::NewGame {
            character: 1,
            choice: Default::default(),
        }
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// `prologue_leon_skip`: a confirm on 1300 stops the line, fades the music
/// and the frame out and takes the sequence to its films' step, and the
/// game starts on 1442. The pad's other skip keys do the same.
#[test]
fn a_press_skips_the_prologue_to_its_films() {
    let dir = prologue_dir("pro-skip");
    let keys: [fn(&mut UiInput); 4] = [
        |input| input.confirm = true,
        |input| input.cancel = true,
        |input| input.start = true,
        |input| input.map = true,
    ];
    for key in keys {
        let mut press = UiInput::default();
        key(&mut press);
        let run = run_prologue(&dir, false, &[(1300, press)], None, |tick, screen, cues| {
            match tick {
                1299 => assert_eq!(pro(screen)[4], 0),
                1300 => {
                    assert_eq!(pro(screen)[..2], [4, 6]);
                    assert_eq!(pro(screen)[4], 1);
                    assert_eq!(screen.value("pro_slot"), 0);
                    assert!(cues.contains(&UiCue::VoiceStop));
                    assert!(cues.contains(&UiCue::MusicFade));
                }
                // Slot 0's fade is over: the sequence comes back in at 0x5A.
                1428 => assert_eq!(pro(screen), [0, 6, 0x5A, 1, 2]),
                1439 => assert_eq!(pro(screen)[..2], [0, 7]),
                1441 => assert_eq!(pro(screen)[..2], [4, 9]),
                _ => {}
            }
        });
        assert_eq!(run.films, [(1439, 3, false), (1440, 0xF, false)]);
        assert_eq!(run.end, 1442);
    }
    // A direction is no skip key.
    let down = UiInput {
        down: true,
        held_down: true,
        ..UiInput::default()
    };
    assert_eq!(
        run_prologue(&dir, false, &[(1300, down)], None, |_, _, _| {}).end,
        4028
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A second press, once the frame is black and the sequence is back, ends
/// the task on that tick: no film is asked for. So does a first press after
/// the last fade, while step 6 counts to its films.
#[test]
fn a_press_on_black_ends_the_prologue_with_no_film() {
    let dir = prologue_dir("pro-skip2");
    let presses = [(1300, confirm()), (1430, confirm())];
    let run = run_prologue(&dir, false, &presses, None, |tick, screen, _| {
        if tick == 1429 {
            assert_eq!(pro(screen), [0, 6, 0x5B, 1, 2]);
        }
    });
    assert_eq!(run.end, 1430);
    assert_eq!(run.films, []);

    let run = run_prologue(
        &dir,
        false,
        &[(3950, confirm())],
        None,
        |tick, screen, _| {
            if tick == 3949 {
                assert_eq!(pro(screen)[..2], [0, 6]);
                assert_eq!(screen.value("pro_skip_ok"), 1);
            }
        },
    );
    assert_eq!(run.end, 3950);
    assert_eq!(run.films, []);
    assert!(matches!(run.action, ScreenAction::NewGame { .. }));
    let _ = std::fs::remove_dir_all(&dir);
}

/// The frame of step 2 (tick 1566): the cells' pieces at their full colour,
/// three quarters over what is under them, between the scene's black bands.
#[test]
fn the_prologue_draws_its_pieces_between_the_bands() {
    let dir = prologue_dir("pro-draw");
    let cx = context_over(prologue_pack(), &dir);
    let mut cx_open = context_over(prologue_pack(), &dir);
    let mut screen =
        LuaScreen::open_screen("title", &[], ScreenAction::Quit, &mut cx_open).unwrap();
    let down = UiInput {
        down: true,
        held_down: true,
        ..UiInput::default()
    };
    for tick in 189..=1566 {
        let input = match tick {
            260 => down,
            300 | 360 | 420 => confirm(),
            _ => UiInput::default(),
        };
        screen.update(&cx, input);
    }
    // Pieces 2 to 5 scroll up from 10 and 170, a line every 11 ticks.
    let (top, bottom): (i32, i32) =
        screen.test_eval("local p = M.state.pro return p.sprites[2].y, p.sprites[4].y");
    assert_eq!((top, bottom), (5, 165));
    assert_eq!(
        screen.test_eval::<i64>("return M.state.pro.sprites[4].colour"),
        0x60
    );
    let mut framebuffer = Framebuffer::new();
    screen.draw(&cx, &mut framebuffer);
    let pixel = |x: i32, y: i32| {
        let at = (y as usize * framebuffer.width as usize + x as usize) * 4;
        <[u8; 3]>::try_from(&framebuffer.rgba[at..at + 3]).unwrap()
    };
    // The first full-screen picture is fading out under them (slot 3).
    let under = pixel(8, bottom);
    assert!(under[0] > 0 && under[0] < 0x40, "{under:?}");
    assert_eq!(pixel(0, bottom + 4), under);
    // The sheet's 4 x 4 blue texels at alpha 191 over it.
    let over = |texel: u32, under: u8| ((texel * 191 + u32::from(under) * 64 + 127) / 255) as u8;
    let blue = [over(0, under[0]), over(0, under[1]), over(255, under[2])];
    assert_eq!(pixel(0, bottom), blue);
    assert_eq!(pixel(259, bottom + 3), blue);
    // The upper pieces are behind the top band.
    assert_eq!(pixel(0, top), [0, 0, 0]);
    assert_eq!(pixel(8, 23), [0, 0, 0]);
    assert_eq!(pixel(8, 24), under);
    assert_eq!(pixel(8, 216), [0, 0, 0]);
    let _ = std::fs::remove_dir_all(&dir);
}
