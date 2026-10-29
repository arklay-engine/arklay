//! The shipped engine screens (`games/re1/ui/*.lua`), driven through
//! [`LuaScreen`] over an empty pack so the checked-in scripts run.

use super::*;

use crate::game::InventoryItem;
use crate::items;
use crate::message::{MessageInput, MessagePhase};
use crate::pack::Pack;
use crate::save::{self, SaveFile};
use crate::transition::FadeColor;
use crate::ui::save_load::SaveOutcome;
use crate::ui::{Screen, ScreenAction, ScreenResult, UiCue, UiInput};

/// Self-deleting save directory unique to this process and label.
struct TempDir(std::path::PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("arklay-ui-screen-{}-{label}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A pack with no screen art and no `ui/` scripts, only the converted
/// rooms' names.
fn empty_pack() -> &'static Pack {
    use std::sync::OnceLock;
    static PACK: OnceLock<Pack> = OnceLock::new();
    PACK.get_or_init(|| {
        let mut writer = crate::pack::PackWriter::new();
        writer.add("unused", Vec::new()).unwrap();
        for (entry, data) in crate::room::re1_name_entries() {
            writer.add(&entry, data).unwrap();
        }
        Pack::from_bytes(writer.to_bytes().unwrap()).unwrap()
    })
}

struct Harness {
    dir: TempDir,
}

impl Harness {
    fn new(label: &str) -> Self {
        Self {
            dir: TempDir::new(label),
        }
    }

    fn context(&self) -> UiContext<'_> {
        UiContext {
            authentic: false,
            pack: empty_pack(),
            save_dir: &self.dir.0,
            font: None,
            text: None,
            ticks: 0,
            cues: Default::default(),
            game: None,
        }
    }

    fn open(&self, id: &str, args: &[(&str, ScreenArg)]) -> LuaScreen {
        let mut cx = self.context();
        LuaScreen::open_screen(id, args, ScreenAction::Quit, &mut cx).unwrap()
    }
}

fn neutral() -> UiInput {
    UiInput::default()
}

fn press(build: impl FnOnce(&mut UiInput)) -> UiInput {
    let mut input = UiInput::default();
    build(&mut input);
    input
}

fn stage(screen: &LuaScreen) -> String {
    screen.test_eval("return M.state.stage")
}

/// Tick until the screen leaves, at most `limit` ticks; the action and the
/// number of ticks taken.
fn run_to_result(
    screen: &mut LuaScreen,
    cx: &UiContext<'_>,
    limit: usize,
) -> (ScreenAction, usize) {
    for tick in 1..=limit {
        if let ScreenResult::Done(action) = screen.update(cx, neutral()) {
            return (action, tick);
        }
    }
    panic!("the screen never finished");
}

// ---- title -----------------------------------------------------------------

/// A title in the option menu.
fn title_menu(harness: &Harness, any_saves: bool) -> LuaScreen {
    let screen = harness.open("title", &[]);
    screen.test_eval::<()>(&format!(
        "M.state.stage = 'menu' M.state.any_saves = {any_saves} \
         M.state.selection = {}",
        if any_saves { 2 } else { 1 }
    ));
    screen
}

#[test]
fn title_any_button_opens_the_option_menu() {
    let harness = Harness::new("title-any");
    let mut screen = harness.open("title", &[]);
    screen.update(&harness.context(), press(|i| i.any = true));
    assert_eq!(stage(&screen), "menu");
}

#[test]
fn title_selection_starts_on_new_game_without_saves() {
    let harness = Harness::new("title-nosaves");
    let screen = harness.open("title", &[]);
    assert!(!screen.test_eval::<bool>("return M.state.any_saves"));
    assert_eq!(screen.test_eval::<u8>("return M.state.selection"), 1);
}

#[test]
fn title_the_folder_probe_follows_a_registered_save_screen() {
    let harness = Harness::new("title-folder");
    // A save that is not one of the eight slots.
    crate::save::save_to_path(
        &harness.dir.0.join("save_01.dat"),
        &crate::save::SaveFile::default(),
    )
    .unwrap();
    let any_saves = |folder: bool| -> (bool, u8) {
        let screen = harness.open("title", &[("folder_saves", ScreenArg::Bool(folder))]);
        (
            screen.test_eval("return M.state.any_saves"),
            screen.test_eval("return M.state.selection"),
        )
    };
    // The ported original's probe only looks at `savedat1..8.dat` and the
    // autosaves its load screen lists.
    assert_eq!(any_saves(false), (false, 1));
    crate::save::write_autosave(&harness.dir.0, &crate::save::SaveFile::default()).unwrap();
    assert_eq!(any_saves(false), (true, 2));
    std::fs::remove_file(harness.dir.0.join("autosave1.dat")).unwrap();
    // A registered save screen lists the folder, like CR's replacement for
    // the original's "does a save exist" check.
    assert_eq!(any_saves(true), (true, 2));
    // A junk file alone does not count.
    std::fs::remove_file(harness.dir.0.join("save_01.dat")).unwrap();
    std::fs::write(harness.dir.0.join("notes.txt"), b"junk").unwrap();
    assert_eq!(any_saves(true), (false, 1));
}

#[test]
fn title_load_is_unreachable_without_saves() {
    let harness = Harness::new("title-noload");
    let cx = harness.context();
    let mut screen = title_menu(&harness, false);
    screen.update(&cx, press(|i| i.right = true));
    assert_eq!(screen.test_eval::<u8>("return M.state.selection"), 1);
    screen.update(&cx, press(|i| i.left = true));
    assert_eq!(screen.test_eval::<u8>("return M.state.selection"), 1);
    // Even forced onto the row, confirming LOAD does nothing.
    screen.test_eval::<()>("M.state.selection = 2");
    let result = screen.update(&cx, press(|i| i.confirm = true));
    assert_eq!(result, ScreenResult::Continue);
    assert_eq!(stage(&screen), "menu");
}

#[test]
fn title_cursor_selects_load_and_confirms_when_saves_exist() {
    let harness = Harness::new("title-load");
    let cx = harness.context();
    let mut screen = title_menu(&harness, true);
    screen.update(&cx, press(|i| i.left = true));
    assert_eq!(screen.test_eval::<u8>("return M.state.selection"), 1);
    screen.update(&cx, press(|i| i.right = true));
    assert_eq!(screen.test_eval::<u8>("return M.state.selection"), 2);
    screen.update(&cx, press(|i| i.confirm = true));
    assert_eq!(stage(&screen), "fade_out");
    assert_eq!(screen.value("game_active"), 1, "the flash runs at 30 Hz");
    assert_eq!(
        run_to_result(&mut screen, &cx, 256).0,
        ScreenAction::SaveLoad
    );
}

#[test]
fn title_only_the_confirm_plays_the_title_voice() {
    let harness = Harness::new("title-voice");
    let cx = harness.context();
    let mut screen = harness.open("title", &[]);
    // The PRESS dismissal and the option-menu moves are silent.
    screen.update(&cx, press(|i| i.any = true));
    for input in [
        press(|i| i.left = true),
        press(|i| i.right = true),
        press(|i| i.up = true),
        press(|i| i.down = true),
    ] {
        screen.update(&cx, input);
    }
    assert!(
        cx.cues.borrow().is_empty(),
        "PRESS and moves must be silent"
    );
    screen.update(&cx, press(|i| i.confirm = true));
    assert_eq!(cx.cues.borrow().as_slice(), &[UiCue::Title]);
}

#[test]
fn title_confirm_runs_the_white_flash_then_black_out() {
    let harness = Harness::new("title-flash");
    let cx = harness.context();
    let mut screen = title_menu(&harness, false);
    screen.update(&cx, press(|i| i.confirm = true));
    assert_eq!(screen.overlay(), Some((FadeColor::White, 0)));
    let step = |screen: &mut LuaScreen, expected| {
        screen.update(&cx, neutral());
        assert_eq!(screen.overlay(), expected);
    };
    // Leg 6 reaches white in two ticks; legs 7, 8 and 9 start from full.
    step(&mut screen, Some((FadeColor::White, 254)));
    step(&mut screen, Some((FadeColor::White, 255)));
    step(&mut screen, Some((FadeColor::White, 127)));
    step(&mut screen, Some((FadeColor::White, 255)));
    step(&mut screen, Some((FadeColor::White, 255)));
    // Leg 9 decays 16 alpha steps a tick over 15 ticks.
    step(&mut screen, Some((FadeColor::White, 239)));
    for _ in 0..13 {
        screen.update(&cx, neutral());
    }
    step(&mut screen, Some((FadeColor::White, 15)));
    // Leg 4: the black-out rises to full.
    step(&mut screen, Some((FadeColor::Black, 0)));
    step(&mut screen, Some((FadeColor::Black, 4)));
    let (action, ticks) = run_to_result(&mut screen, &cx, 128);
    assert_eq!(action, ScreenAction::CharSelect);
    assert_eq!(ticks, 52, "the black-out holds for 52 more drawn ticks");
}

#[test]
fn title_appears_clear_and_ramps_the_press_text() {
    let harness = Harness::new("title-ramp");
    let cx = harness.context();
    let mut screen = harness.open("title", &[]);
    // No entry fade: the first frame is clear, only the PRESS text ramps.
    screen.update(&cx, neutral());
    assert_eq!(screen.overlay(), None);
    for _ in 0..64 {
        screen.update(&cx, neutral());
    }
    assert_eq!(screen.overlay(), None);
    assert_eq!(
        screen.test_eval::<u8>("return M.state.press_brightness"),
        0x80
    );
    // The blink alternates between two non-black scales.
    let scales: Vec<u8> = (0..64)
        .map(|_| {
            screen.update(&cx, neutral());
            screen.test_eval("return M.press_scale()")
        })
        .collect();
    assert!(scales.contains(&255));
    assert!(scales.iter().any(|&scale| scale < 255));
}

#[test]
fn title_the_idle_title_times_out_into_the_attract_demo() {
    let harness = Harness::new("title-attract");
    let cx = harness.context();
    let mut screen = harness.open("title", &[]);
    // 32 ramp ticks + the 0x708 countdown + the 0x400 fade (8 alpha a tick;
    // the wrap tick ends it).
    let (action, ticks) = run_to_result(&mut screen, &cx, 4000);
    assert_eq!(action, ScreenAction::Attract);
    assert_eq!(ticks, 1863);
    assert_eq!(stage(&screen), "attract_fade");
}

#[test]
fn title_a_button_cancels_the_attract_fade_back_to_press() {
    let harness = Harness::new("title-cancel");
    let cx = harness.context();
    let mut screen = harness.open("title", &[]);
    screen.test_eval::<()>("M.state.attract_countdown = 1 M.state.press_brightness = 0x80");
    screen.update(&cx, neutral());
    assert_eq!(stage(&screen), "attract_fade");
    screen.update(&cx, press(|i| i.any = true));
    assert_eq!(stage(&screen), "press");
    assert_eq!(screen.overlay(), None, "the fade clears instantly");
    assert_eq!(
        screen.test_eval::<i32>("return M.state.attract_countdown"),
        0x708
    );
}

// ---- character select ------------------------------------------------------

/// Ticks one card swap takes to reach its end poses.
const SWAP_TICKS: usize = 34;

/// A card's (x, y, scale, brightness, tpage).
type Pose = (i32, i32, i32, u8, u8);

/// Card `card`'s pose; 0 Chris, 1 Jill.
fn pose(screen: &LuaScreen, card: usize) -> Pose {
    screen.test_eval(&format!(
        "local p = M.state.poses[{}] return p.x, p.y, p.scale, p.bright, p.tpage",
        card + 1
    ))
}

fn selected(screen: &LuaScreen) -> u8 {
    screen.test_eval("return M.state.selected")
}

fn settle(screen: &mut LuaScreen, cx: &UiContext<'_>) {
    for _ in 0..SWAP_TICKS {
        screen.update(cx, neutral());
    }
}

#[test]
fn select_left_and_right_swap_the_pick_when_the_slide_lands() {
    let harness = Harness::new("select-swap");
    let cx = harness.context();
    let mut screen = harness.open("char_select", &[]);
    assert_eq!(selected(&screen), 0);
    screen.update(&cx, press(|i| i.right = true));
    assert_eq!(stage(&screen), "swapping", "right starts the slide");
    assert_eq!(selected(&screen), 0, "the pick flips only at the end");
    settle(&mut screen, &cx);
    assert_eq!(stage(&screen), "idle");
    assert_eq!(selected(&screen), 1);
    screen.update(&cx, press(|i| i.left = true));
    settle(&mut screen, &cx);
    assert_eq!(selected(&screen), 0);
}

#[test]
fn select_the_slide_follows_the_original_frame_trace() {
    let harness = Harness::new("select-trace");
    let cx = harness.context();
    let mut screen = harness.open("char_select", &[]);
    screen.update(&cx, press(|i| i.right = true));
    // Frame 1: card 0 left the front pose by one acceleration step.
    assert_eq!(pose(&screen, 0), (138, 73, 0xF0, 0x80, 2));
    assert_eq!(pose(&screen, 1), (198, 103, 0x110, 0x50, 3));
    let steps: [(usize, Pose, Option<Pose>); 5] = [
        // Frames 2..: the overshoot builds toward x = 312.
        (1, (142, 75, 0xF0, 0x80, 2), None),
        // Frame 10.
        (8, (240, 124, 0xF0, 0x80, 2), None),
        // Frame 15: the scale/brightness ramp is running.
        (5, (300, 154, 0xF8, 0x74, 2), None),
        // Frame 20: the velocity crosses zero and the tpage flips.
        (5, (310, 159, 0x102, 0x65, 3), Some((26, 17, 0xFE, 0x6B, 2))),
        // Frame 33: the cards sit on their final poses.
        (
            13,
            (200, 104, 0x110, 0x50, 3),
            Some((136, 72, 0xF0, 0x80, 2)),
        ),
    ];
    for (ticks, card0, card1) in steps {
        for _ in 0..ticks {
            screen.update(&cx, neutral());
        }
        assert_eq!(pose(&screen, 0), card0);
        if let Some(card1) = card1 {
            assert_eq!(pose(&screen, 1), card1);
        }
    }
    // Frame 34 flips the pick and holds the same poses.
    screen.update(&cx, neutral());
    assert_eq!(stage(&screen), "idle");
    assert_eq!(selected(&screen), 1);
    assert_eq!(pose(&screen, 0), (200, 104, 0x110, 0x50, 3));
    assert_eq!(pose(&screen, 1), (136, 72, 0xF0, 0x80, 2));
}

#[test]
fn select_confirm_starts_a_new_game_for_the_picked_character() {
    let harness = Harness::new("select-confirm");
    let cx = harness.context();
    let mut screen = harness.open("char_select", &[]);
    screen.update(&cx, press(|i| i.right = true));
    settle(&mut screen, &cx);
    assert_eq!(selected(&screen), 1);
    screen.update(&cx, press(|i| i.confirm = true));
    // 128 fade-in + 128 fade-out + the 240-tick g_selTimer hold, matching the
    // reference's case 6 at frame 496.
    let (action, ticks) = run_to_result(&mut screen, &cx, 700);
    assert_eq!(ticks, 496, "the confirm chain's fixed length");
    assert_eq!(action, ScreenAction::NewGame { character: 1 });
}

#[test]
fn select_cues_follow_the_input_edges() {
    let harness = Harness::new("select-cues");
    let cx = harness.context();
    let mut screen = harness.open("char_select", &[]);
    screen.update(&cx, press(|i| i.right = true));
    assert_eq!(cx.cues.borrow().as_slice(), &[UiCue::SelectCursor]);
    cx.cues.borrow_mut().clear();
    settle(&mut screen, &cx);
    screen.update(&cx, press(|i| i.confirm = true));
    assert_eq!(cx.cues.borrow().as_slice(), &[UiCue::SelectConfirm]);

    // Cancel is silent in the original.
    let mut screen = harness.open("char_select", &[]);
    cx.cues.borrow_mut().clear();
    screen.update(&cx, press(|i| i.cancel = true));
    assert!(cx.cues.borrow().is_empty(), "cancel must not queue a cue");
}

/// The white overlay alpha at the current tick, or `None` when clear.
fn white(screen: &LuaScreen) -> Option<u8> {
    match screen.overlay() {
        Some((FadeColor::White, alpha)) => Some(alpha),
        Some((FadeColor::Black, _)) => panic!("expected a white overlay"),
        None => None,
    }
}

#[test]
fn select_confirm_runs_the_white_flash_chain_before_the_new_game() {
    let harness = Harness::new("select-flash");
    let cx = harness.context();
    let mut screen = harness.open("char_select", &[]);
    // Skip the opening black fade.
    for _ in 0..40 {
        screen.update(&cx, neutral());
    }
    screen.update(&cx, press(|i| i.confirm = true));
    assert_eq!(white(&screen), Some(0), "the confirm tick starts clear");
    assert_eq!(stage(&screen), "confirm_fade_in");

    // The flash rises two alpha steps a tick; the 128th tick wraps the 16-bit
    // accumulator and hands over to the fade-out.
    screen.update(&cx, neutral());
    assert_eq!(white(&screen), Some(2));
    for _ in 0..126 {
        screen.update(&cx, neutral());
    }
    assert_eq!(white(&screen), Some(254));
    assert_eq!(stage(&screen), "confirm_fade_in");
    screen.update(&cx, neutral());
    assert_eq!(stage(&screen), "confirm_fade_out");
    assert_eq!(white(&screen), Some(255), "the fade-out starts from full");

    // The white decays two alpha steps a tick for 128 ticks.
    screen.update(&cx, neutral());
    assert_eq!(white(&screen), Some(253));
    for _ in 0..126 {
        screen.update(&cx, neutral());
    }
    assert_eq!(white(&screen), Some(1));
    assert_eq!(stage(&screen), "confirm_fade_out");

    // The 240-tick hold runs with the frame clear: g_selTimer starts at -16
    // and decrements to zero through the signed-byte wrap.
    screen.update(&cx, neutral());
    assert_eq!(stage(&screen), "confirm_hold");
    assert_eq!(screen.overlay(), None);
    let (action, ticks) = run_to_result(&mut screen, &cx, 240);
    assert_eq!(ticks, 240);
    assert_eq!(action, ScreenAction::NewGame { character: 0 });
}

#[test]
fn select_cancel_black_fades_in_eleven_ticks_back_to_the_title() {
    let harness = Harness::new("select-cancel");
    let cx = harness.context();
    let mut screen = harness.open("char_select", &[]);
    for _ in 0..40 {
        screen.update(&cx, neutral());
    }
    screen.update(&cx, press(|i| i.cancel = true));
    assert_eq!(stage(&screen), "cancel_fade_out");
    assert_eq!(screen.overlay(), Some((FadeColor::Black, 0)));
    screen.update(&cx, neutral());
    assert_eq!(screen.overlay(), Some((FadeColor::Black, 24)));
    for _ in 0..9 {
        screen.update(&cx, neutral());
    }
    assert_eq!(screen.overlay(), Some((FadeColor::Black, 240)));
    assert_eq!(
        screen.update(&cx, neutral()),
        ScreenResult::Done(ScreenAction::Title)
    );
}

#[test]
fn select_the_confirm_and_cancel_fades_draw_over_black() {
    let harness = Harness::new("select-black");
    let cx = harness.context();
    let mut screen = harness.open("char_select", &[]);
    for stage in ["confirm_fade_out", "confirm_hold", "cancel_fade_out"] {
        screen.test_eval::<()>(&format!("M.state.stage = '{stage}'"));
        let mut framebuffer = crate::render::Framebuffer::new();
        framebuffer.fill_rect([0, 0, 320, 240], [9, 9, 9, 255]);
        screen.draw(&cx, &mut framebuffer);
        assert!(
            framebuffer
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| pixel[..3] == [0, 0, 0]),
            "{stage} must blank the frame"
        );
    }
}

// ---- ending ----------------------------------------------------------------

fn result_screen(harness: &Harness, ending_id: i64, no_save: bool) -> LuaScreen {
    harness.open(
        "ending_result",
        &[
            ("ending_id", ScreenArg::Int(ending_id)),
            ("no_save", ScreenArg::Bool(no_save)),
            ("background", ScreenArg::Bool(ending_id >= 4)),
        ],
    )
}

fn step(screen: &LuaScreen) -> i64 {
    screen.test_eval("return M.state.step")
}

fn active(screen: &LuaScreen, slot: usize) -> bool {
    screen.test_eval(&format!("return M.state.slots[{slot}].active"))
}

/// Tick neutrally until the RESULT machine reaches `target`, with a cap.
fn drive_to(screen: &mut LuaScreen, cx: &UiContext<'_>, target: i64, cap: usize) {
    for _ in 0..cap {
        if step(screen) == target {
            return;
        }
        screen.update(cx, neutral());
    }
    assert_eq!(step(screen), target, "the machine reaches step {target}");
}

#[test]
fn ending_result_runs_the_reference_step_machine() {
    let harness = Harness::new("result-steps");
    let cx = harness.context();
    let mut screen = result_screen(&harness, 1, false);
    assert_eq!(step(&screen), 0);
    // Step 0 arms the background fade and builds the hidden slots.
    screen.update(&cx, neutral());
    assert_eq!(step(&screen), 1);
    assert!(screen.test_eval::<i64>("return M.state.background.fade_vel") > 0);
    assert!((1..=5).all(|slot| !active(&screen, slot)));
    // Steps 1-2 reveal the plates in pairs.
    drive_to(&mut screen, &cx, 2, 0x80);
    assert!(active(&screen, 1) && active(&screen, 2));
    drive_to(&mut screen, &cx, 3, 0x80);
    assert!(active(&screen, 3) && active(&screen, 4));
    // Step 3 is the reveal hold; step 4 waits for input.
    drive_to(&mut screen, &cx, 4, 0x80);
    screen.update(&cx, neutral());
    assert_eq!(step(&screen), 4, "step 4 holds without input");

    // A button press leaves step 4 (ending 1 -> step 5 -> step 10 -> done).
    screen.update(&cx, press(|i| i.any = true));
    assert_eq!(step(&screen), 5);
    let (action, _) = run_to_result(&mut screen, &cx, 0x400);
    assert_eq!(action, ScreenAction::EndingEpilogue);
}

#[test]
fn ending_result_step_four_times_out_without_input() {
    let harness = Harness::new("result-timeout");
    let cx = harness.context();
    let mut screen = result_screen(&harness, 2, false);
    drive_to(&mut screen, &cx, 4, 0x100);
    drive_to(&mut screen, &cx, 5, 0x710);
}

#[test]
fn ending_six_takes_the_extra_plate_steps() {
    let harness = Harness::new("result-six");
    let cx = harness.context();
    let mut screen = result_screen(&harness, 6, false);
    drive_to(&mut screen, &cx, 4, 0x100);
    screen.update(&cx, press(|i| i.any = true));
    assert_eq!(step(&screen), 6, "ending 6 diverts to the fifth plate");
    // The fifth plate activates after the 0x5A hold.
    drive_to(&mut screen, &cx, 8, 0x100);
    assert!(active(&screen, 5));
    assert!(screen.test_eval::<bool>("return M.state.slots[5].additive"));
    let (action, _) = run_to_result(&mut screen, &cx, 0x400);
    assert_eq!(action, ScreenAction::EndingEpilogue);
}

#[test]
fn ending_result_a_no_save_run_enables_the_confetti_bonus() {
    for no_save in [false, true] {
        let harness = Harness::new("result-confetti");
        let cx = harness.context();
        let mut screen = result_screen(&harness, 1, no_save);
        screen.update(&cx, neutral());
        screen.test_eval::<()>("M.state.step = 10 M.state.timer = 0x1E");
        screen.update(&cx, neutral());
        assert_eq!(step(&screen), 11);
        assert_eq!(screen.test_eval::<bool>("return M.state.effects"), no_save);
        assert_eq!(
            screen.test_eval::<i64>("return #M.state.confetti"),
            if no_save { 48 } else { 0 }
        );
        let cues = cx.cues.borrow();
        if no_save {
            assert_eq!(cues.as_slice(), &[UiCue::Decide]);
        } else {
            assert!(cues.is_empty());
        }
    }
}

#[test]
fn ending_fade_shade_and_scale_ramp_to_full() {
    let harness = Harness::new("ending-shade");
    let screen = result_screen(&harness, 1, false);
    let eval = |body: &str| -> i64 {
        screen.test_eval(&format!("local E = require('lib/ending') return {body}"))
    };
    assert_eq!(eval("E.fade_shade(0)"), 0);
    assert_eq!(eval("E.fade_shade(E.FADE_FULL)"), 0x80);
    assert_eq!(eval("E.fade_shade(-1)"), 0);
    assert_eq!(eval("E.shade_scale(0)"), 0);
    assert_eq!(eval("E.shade_scale(0x80)"), 255);
    assert_eq!(eval("E.shade_scale(0x40)"), 127);
    // The epilogue's fixed backdrop shade (`s_bgSlot->fadeLevel` 0x6000).
    assert_eq!(eval("E.fade_shade(0x6000)"), 0x60);
    assert_eq!(eval("E.shade_scale(E.fade_shade(0x6000))"), 191);
}

#[test]
fn ending_result_lays_the_slots_out_from_the_credit_table() {
    let harness = Harness::new("result-layout");
    let cx = harness.context();
    let slot = |screen: &LuaScreen, n: usize| -> (String, i64, i64) {
        screen.test_eval(&format!(
            "local s = M.state.slots[{n}] return s.kind, s.x, s.y"
        ))
    };
    // The plain rows (endings 1-3) use entries 8-12.
    let mut plain = result_screen(&harness, 1, false);
    plain.update(&cx, neutral());
    assert_eq!(slot(&plain, 2), ("clear_time".into(), 0x80, 0x58));
    assert_eq!(slot(&plain, 4), ("save_count".into(), 0x98, 0xB8));
    // The background rows shift right for Jill.
    let mut jill = harness.open(
        "ending_result",
        &[
            ("ending_id", ScreenArg::Int(4)),
            ("character", ScreenArg::Int(1)),
            ("background", ScreenArg::Bool(true)),
        ],
    );
    jill.update(&cx, neutral());
    assert_eq!(slot(&jill, 1), ("sprite".into(), 0x9C, 0x30));
    assert_eq!(slot(&jill, 2), ("clear_time".into(), 0x30 + 0x9C, 0x50));
}

#[test]
fn ending_epilogue_runs_and_spins_the_model() {
    let harness = Harness::new("epilogue");
    let cx = harness.context();
    let mut screen = harness.open("ending_epilogue", &[]);
    assert_eq!(
        screen.test_eval::<i64>("return M.state.captions[1].fade_level"),
        0x8000
    );
    let (action, ticks) = run_to_result(&mut screen, &cx, 0x1000);
    assert_eq!(action, ScreenAction::EndingSave);
    // The 0x10D hold (plus the build tick and the step past it), then the
    // 16-step fade out.
    assert_eq!(ticks, 1 + 0x10E + 16);
    assert_ne!(
        screen.test_eval::<i64>("return M.state.spin"),
        0,
        "the model spins"
    );
}

// ---- save/load -------------------------------------------------------------

const SAVE_LOAD: &str = "ui/save_load.lua";
/// The assembled reveal's length (`name/count/location`).
const REVEAL_LEN: usize = 31;
/// The reveal slice grows two bytes per step.
const REVEAL_STEP: usize = 2;
/// Ticks between reveal steps.
const REVEAL_TICKS: usize = 5;

fn open_load(cx: &mut UiContext<'_>) -> LuaScreen {
    LuaScreen::open_load(SAVE_LOAD, cx).unwrap()
}

fn open_save(cx: &mut UiContext<'_>, file: SaveFile, ink_ribbon: bool) -> LuaScreen {
    LuaScreen::open_save(SAVE_LOAD, cx, file, ink_ribbon).unwrap()
}

fn confirm() -> UiInput {
    press(|i| i.confirm = true)
}

fn cancel() -> UiInput {
    press(|i| i.cancel = true)
}

fn cursor(screen: &LuaScreen) -> i64 {
    screen.test_eval("return M.state.cursor")
}

fn reveal_bytes(screen: &LuaScreen) -> Vec<u8> {
    screen
        .test_eval::<mlua::String>("return M.state.reveal.bytes")
        .as_bytes()
        .to_vec()
}

fn reveal_len(screen: &LuaScreen) -> usize {
    screen.test_eval("return M.state.reveal and M.state.reveal.length or 0")
}

fn lua_bytes(screen: &LuaScreen, body: &str) -> Vec<u8> {
    screen.test_eval::<mlua::String>(body).as_bytes().to_vec()
}

#[test]
fn save_load_empty_slots_refuse_and_exit_returns_to_title() {
    let harness = Harness::new("sl-empty");
    let mut cx = harness.context();
    let mut screen = open_load(&mut cx);
    assert_eq!(cursor(&screen), 0, "the original starts on slot 1");
    assert!(screen.test_eval::<bool>(
        "for _, slot in ipairs(M.state.slots) do if slot then return false end end return true"
    ));

    screen.update(&cx, confirm());
    assert_eq!(stage(&screen), "idle", "an empty slot refuses");

    screen.test_eval::<()>("M.state.cursor = 8");
    screen.update(&cx, confirm());
    assert_eq!(stage(&screen), "fade_out");
    // The load screen fades its own overlay out, 16 a tick.
    let (action, ticks) = run_to_result(&mut screen, &cx, 64);
    assert_eq!(action, ScreenAction::Title);
    assert_eq!(ticks, 16);
    assert_eq!(screen.fade(), 255);
}

#[test]
fn save_load_load_mode_cancel_returns_to_title_and_confirm_returns_the_slot() {
    let harness = Harness::new("sl-load");
    let file = SaveFile {
        character: 1,
        stage: 1,
        room: 1,
        ..SaveFile::default()
    };
    save::save(&harness.dir.0, 2, &file).unwrap();
    let mut cx = harness.context();
    let mut screen = open_load(&mut cx);
    screen.update(&cx, confirm());
    assert_eq!(stage(&screen), "idle", "an empty slot refuses");

    screen.test_eval::<()>("M.state.cursor = 2");
    screen.update(&cx, confirm());
    assert_eq!(stage(&screen), "fade_out");
    let (action, _) = run_to_result(&mut screen, &cx, 64);
    assert_eq!(action, ScreenAction::LoadGame { slot: 2 });

    let mut screen = open_load(&mut cx);
    screen.update(&cx, cancel());
    let (action, _) = run_to_result(&mut screen, &cx, 64);
    assert_eq!(action, ScreenAction::Title);
}

#[test]
fn save_load_overwrite_confirm_yes_and_no_flow() {
    let harness = Harness::new("sl-overwrite");
    save::save(&harness.dir.0, 0, &SaveFile::default()).unwrap();
    let mut cx = harness.context();
    let mut screen = open_save(&mut cx, SaveFile::default(), false);

    // Confirm the occupied slot: the dialog opens with YES selected and the
    // cursor hidden.
    screen.update(&cx, confirm());
    assert_eq!(stage(&screen), "confirm_overwrite");
    assert_eq!(screen.test_eval::<i64>("return M.state.confirm_choice"), 0);
    assert!(screen.test_eval::<bool>("return M.state.blink_state"));

    // NO with the right arrow, confirm: back to navigation, nothing saved.
    screen.update(&cx, press(|i| i.right = true));
    assert_eq!(screen.test_eval::<i64>("return M.state.confirm_choice"), 1);
    screen.update(&cx, confirm());
    assert_eq!(stage(&screen), "idle");
    assert!(screen.take_outcome().is_none());

    // Cancel out of the dialog.
    screen.update(&cx, confirm());
    assert_eq!(stage(&screen), "confirm_overwrite");
    screen.update(&cx, cancel());
    assert_eq!(stage(&screen), "idle");

    // YES (the default) writes and records the outcome.
    screen.update(&cx, confirm());
    screen.update(&cx, confirm());
    assert_eq!(stage(&screen), "save_anim");
    assert_eq!(
        screen.take_outcome(),
        Some(SaveOutcome {
            slot: Some(0),
            ink_ribbon: false
        })
    );
    assert!(save::read_slot_info(&harness.dir.0, 0).is_some());
}

#[test]
fn save_load_saving_an_empty_slot_skips_the_confirm_and_consumes_the_ribbon() {
    let harness = Harness::new("sl-save-empty");
    let mut file = SaveFile {
        character: 0,
        stage: 1,
        room: 0,
        saves: 4,
        total_held: 1,
        ..SaveFile::default()
    };
    file.player_slots[0] = InventoryItem {
        id: items::ITEM_INK_RIBBONS,
        quantity: 1,
    };
    let mut cx = harness.context();
    let mut screen = open_save(&mut cx, file, true);
    screen.update(&cx, confirm());
    assert_eq!(stage(&screen), "save_anim");
    assert_eq!(
        screen.take_outcome(),
        Some(SaveOutcome {
            slot: Some(0),
            ink_ribbon: true
        }),
        "the outcome is available while the reveal runs"
    );

    // The block was written without the ribbon and the slot rescanned.
    let written = save::load(&harness.dir.0, 0).unwrap();
    assert_eq!(written.total_held, 0);
    assert_eq!(written.player_slots[0].id, 0);
    assert_eq!(written.used_item, items::ITEM_INK_RIBBONS);
    assert_eq!(screen.test_eval::<i64>("return M.state.slots[1].saves"), 4);

    // The reveal starts two bytes long and finishes back in gameplay, with
    // no fade of its own.
    assert_eq!(reveal_len(&screen), REVEAL_STEP);
    let (action, _) = run_to_result(&mut screen, &cx, 600);
    assert_eq!(action, ScreenAction::Resume);
    assert_eq!(screen.fade(), 0);
}

#[test]
fn save_load_the_reveal_grows_two_bytes_per_step_and_names_the_location() {
    let harness = Harness::new("sl-reveal");
    let file = SaveFile {
        character: 1,
        stage: 3,
        room: 7,
        saves: 7,
        ..SaveFile::default()
    };
    let mut cx = harness.context();
    let mut screen = open_save(&mut cx, file, false);
    screen.update(&cx, confirm());
    assert_eq!(stage(&screen), "save_anim");

    let reveal = reveal_bytes(&screen);
    assert_eq!(reveal.len(), REVEAL_LEN);
    assert_eq!(
        &reveal[6..14],
        &[0x38, 0xFB, 0x0C, 0xFB, 0x13, 0xFB, 0x38, 0xFB],
        "separator, count 07, separator"
    );
    let blank_cells = |bytes: &[u8]| bytes.chunks(2).all(|cell| cell == [0x00, 0xFB]);
    assert!(
        blank_cells(&reveal[..6]) && blank_cells(&reveal[14..30]),
        "the test context has no text, so the name and location are blank cells"
    );
    assert_eq!(reveal[30], 0x01, "the slice takes the terminator too");

    // The assembled layout against real-shaped strings.
    let assembled = lua_bytes(
        &screen,
        "return M.build_reveal('\\xDB\\xFB\\xCF\\xFB\\x00\\xFB\\x01', 7, \
         '\\xF9\\x09\\x00\\xFB\\x01')",
    );
    assert_eq!(&assembled[..6], &[0xDB, 0xFB, 0xCF, 0xFB, 0x00, 0xFB]);
    assert_eq!(
        &assembled[6..14],
        &[0x38, 0xFB, 0x0C, 0xFB, 0x13, 0xFB, 0x38, 0xFB]
    );
    assert_eq!(&assembled[14..18], &[0xF9, 0x09, 0x00, 0xFB]);
    assert_eq!(assembled.len(), REVEAL_LEN);
    assert!(blank_cells(&assembled[18..30]));
    assert_eq!(assembled[30], 0x01);

    // Steps advance every five ticks.
    let mut lengths = vec![reveal_len(&screen)];
    for _ in 0..8 {
        for _ in 0..=REVEAL_TICKS {
            screen.update(&cx, neutral());
        }
        if reveal_len(&screen) != *lengths.last().unwrap() {
            lengths.push(reveal_len(&screen));
        }
    }
    assert_eq!(&lengths[..4], &[2, 4, 6, 8]);
    assert!(lengths.iter().all(|length| length % 2 == 0));
    assert!(lengths.iter().all(|&length| length <= REVEAL_LEN));
}

#[test]
fn save_load_the_reveal_plays_the_typewriter_tick_for_every_non_space_slice() {
    let harness = Harness::new("sl-reveal-tick");
    let mut cx = harness.context();
    let mut screen = open_save(&mut cx, SaveFile::default(), false);

    // The write's own tick plays the slot-confirm cue (the same bank slot as
    // the first slice's tick) and draws the first slice; the steps that
    // follow cue as they reveal.
    screen.update(&cx, confirm());
    assert_eq!(stage(&screen), "save_anim");
    assert_eq!(cx.cues.borrow().as_slice(), &[UiCue::SaveConfirm]);

    let bytes = reveal_bytes(&screen);
    let mut lengths = Vec::new();
    for _ in 0..(REVEAL_LEN + REVEAL_STEP) * (REVEAL_TICKS + 1) {
        if stage(&screen) != "save_anim" {
            break;
        }
        cx.cues.borrow_mut().clear();
        screen.update(&cx, neutral());
        if !cx.cues.borrow().is_empty() {
            lengths.push(reveal_len(&screen));
        }
    }
    assert_ne!(stage(&screen), "save_anim", "the reveal finished");
    // Only the separators and the count digits are non-space here. The
    // steps that cue are the ones whose slice revealed a non-space byte two
    // back from the copy end (`REVEAL_LEN - 1` is the last drawn slice).
    let cued: Vec<usize> = (2 * REVEAL_STEP..=REVEAL_LEN - 1)
        .step_by(REVEAL_STEP)
        .filter(|&length| bytes[length - REVEAL_STEP] != 0)
        .collect();
    assert_eq!(cued, [8, 10, 12, 14]);
    assert_eq!(lengths, cued);
    assert_eq!(reveal_len(&screen), REVEAL_LEN - 1);
}

#[test]
fn save_load_the_reveal_tick_reads_the_byte_two_back_from_the_copy_end() {
    let harness = Harness::new("sl-reveal-offset");
    let mut cx = harness.context();
    let mut screen = open_save(&mut cx, SaveFile::default(), false);
    // The last byte and the last-but-two byte of the next slice are not
    // spaces, but the byte two back from the copy end is: the step to four
    // bytes stays silent (a `length - 1` or `length - 3` check would cue).
    screen.test_eval::<()>(
        "M.state.reveal = { bytes = '\\0\\x38\\0\\x38' .. string.rep('\\0', 27), \
         length = 2, timer = 0 } M.state.stage = 'save_anim'",
    );
    screen.update(&cx, neutral());
    assert_eq!(reveal_len(&screen), 2 * REVEAL_STEP, "the step ran");
    assert!(cx.cues.borrow().is_empty());
}

#[test]
fn save_load_the_write_tick_cues_the_first_slice_once_on_both_paths() {
    let harness = Harness::new("sl-first-slice");
    let text = crate::text::Text::embedded();
    let mut cx = harness.context();
    cx.text = Some(&text);

    // An empty slot: the navigation confirm already plays the slot's confirm
    // cue, which shares the first slice's bank slot.
    let mut screen = open_save(&mut cx, SaveFile::default(), false);
    screen.update(&cx, confirm());
    assert_eq!(stage(&screen), "save_anim");
    assert_ne!(
        reveal_bytes(&screen)[0],
        0,
        "the embedded name is not a space"
    );
    assert_eq!(cx.cues.borrow().as_slice(), &[UiCue::SaveConfirm]);

    // The overwrite dialog's YES is silent, so the write's own tick carries
    // the first slice's tick. (The write above occupied slot 0.)
    let mut screen = open_save(&mut cx, SaveFile::default(), false);
    screen.update(&cx, confirm());
    assert_eq!(stage(&screen), "confirm_overwrite");
    cx.cues.borrow_mut().clear();
    screen.update(&cx, confirm());
    assert_eq!(stage(&screen), "save_anim");
    assert_eq!(cx.cues.borrow().as_slice(), &[UiCue::SaveConfirm]);
}

#[test]
fn save_load_the_overwrite_write_stays_silent_when_the_first_slice_is_a_space() {
    let harness = Harness::new("sl-blank-slice");
    save::save(&harness.dir.0, 0, &SaveFile::default()).unwrap();
    let mut cx = harness.context();
    let mut screen = open_save(&mut cx, SaveFile::default(), false);
    screen.update(&cx, confirm());
    assert_eq!(stage(&screen), "confirm_overwrite");
    cx.cues.borrow_mut().clear();
    screen.update(&cx, confirm());
    assert_eq!(stage(&screen), "save_anim");
    assert_eq!(reveal_bytes(&screen)[0], 0, "no text, so the name is blank");
    assert!(cx.cues.borrow().is_empty(), "a space never cues");
}

#[test]
fn save_load_count_digits_follow_the_original_table() {
    let harness = Harness::new("sl-count");
    let mut cx = harness.context();
    let screen = open_load(&mut cx);
    let count = |saves: u32| lua_bytes(&screen, &format!("return M.count_bytes({saves})"));
    assert_eq!(count(0), [0x0C, 0xFB, 0x0C, 0xFB]);
    assert_eq!(count(7), [0x0C, 0xFB, 0x13, 0xFB]);
    assert_eq!(count(42), [0x10, 0xFB, 0x0E, 0xFB]);
    assert_eq!(count(107), [0x0C, 0xFB, 0x13, 0xFB]);
    assert_eq!(
        screen.test_eval::<usize>("return #M.build_reveal('', 0, '')"),
        REVEAL_LEN
    );
}

/// The Western sheet (8px grid) and a blank 768-wide Japanese sheet (14px
/// grid), as the session fonts the save screen's column tables key on.
fn western_font() -> crate::font::Font {
    crate::font::western()
}

fn japanese_font() -> crate::font::Font {
    let (width, height) = (crate::font::JPN_SHEET_WIDTH, 256);
    crate::font::Font::new(
        crate::model::Texture8 {
            width,
            height,
            indices: vec![0; (width * height) as usize],
            palettes: vec![[0; 4]; crate::model::PALETTE_ROW_LEN],
            stp: Vec::new(),
        },
        crate::font::FontDef::japanese(),
    )
}

/// The layout field `key` of the screen's running column table.
fn layout_field(screen: &LuaScreen, key: &str) -> i64 {
    screen.test_eval(&format!("return M.layout.{key}"))
}

#[test]
fn save_load_picks_the_column_table_of_the_session_fonts_grid() {
    let harness = Harness::new("sl-layout");
    let western = western_font();
    let japanese = japanese_font();

    // SaveLoadScreen.cpp's GetSaveScreenLayout, per release.
    let cases: [(Option<&crate::font::Font>, [i64; 7]); 3] = [
        (Some(&japanese), [14, 0x6F, 0x99, 0x61, 0x29, 56, 3]),
        (Some(&western), [8, 103, 127, 124, 47, 40, 5]),
        // No font draws no text; the Japanese table stands.
        (None, [14, 0x6F, 0x99, 0x61, 0x29, 56, 3]),
    ];
    for (font, expected) in cases {
        let mut cx = harness.context();
        cx.font = font;
        let screen = open_load(&mut cx);
        let fields = [
            "glyph_w",
            "count_x",
            "loc_x",
            "header_x",
            "cursor_x",
            "confirm_stride",
            "name_cells",
        ]
        .map(|key| layout_field(&screen, key));
        assert_eq!(fields, expected);
    }

    // Every column is a whole cell of its grid from the row start, as the
    // reference's comments derive them.
    let mut cx = harness.context();
    for font in [&japanese, &western] {
        cx.font = Some(font);
        let screen = open_load(&mut cx);
        let w = layout_field(&screen, "glyph_w");
        let name = layout_field(&screen, "name_cells");
        assert_eq!(layout_field(&screen, "count_x"), 55 + (name + 1) * w);
        assert_eq!(layout_field(&screen, "loc_x"), 55 + (name + 4) * w);
        assert_eq!(layout_field(&screen, "cursor_x"), 55 - w);
        // The 9-cell "SAVE GAME" centred on the 320 frame.
        assert_eq!(layout_field(&screen, "header_x"), 160 - 9 * w / 2);
    }
}

#[test]
fn save_load_slot_art_matches_the_releases_templates() {
    let harness = Harness::new("sl-art");
    let mut cx = harness.context();
    let decode = |bytes: Vec<u8>| -> String {
        bytes
            .iter()
            .map(|&byte| match byte {
                0x00 => ' ',
                0x38 => '/',
                0x3B => '-',
                _ => '?',
            })
            .collect()
    };
    let japanese = japanese_font();
    cx.font = Some(&japanese);
    let screen = open_load(&mut cx);
    // 0x004b0fa0 / 0x004b0fc0: the 15-cell Japanese row.
    assert_eq!(
        decode(lua_bytes(&screen, "return M.layout.filled")),
        "   /  /     /  "
    );
    assert_eq!(
        decode(lua_bytes(&screen, "return M.layout.empty")),
        "---/--/-----/--"
    );

    let western = western_font();
    cx.font = Some(&western);
    let screen = open_load(&mut cx);
    // DAT_004d4058 / DAT_004d4070: the 28-cell USA row (0x38 is the
    // backslash on the Western sheets).
    assert_eq!(
        decode(lua_bytes(&screen, "return M.layout.filled")),
        "     /  /"
    );
    let empty = decode(lua_bytes(&screen, "return M.layout.empty"));
    assert_eq!(empty, "-----/--/  -----------------");
    assert_eq!(empty.len(), 28);
}

#[test]
fn save_load_usa_reveal_spans_the_28_cell_row() {
    let harness = Harness::new("sl-usa-reveal");
    let western = western_font();
    let text = crate::text::Text::embedded();
    let mut cx = harness.context();
    cx.font = Some(&western);
    cx.text = Some(&text);
    let file = SaveFile {
        character: 1,
        stage: 0,
        room: 6,
        saves: 3,
        ..SaveFile::default()
    };
    let mut screen = open_save(&mut cx, file, false);
    screen.update(&cx, confirm());
    assert_eq!(stage(&screen), "save_anim");

    let reveal = reveal_bytes(&screen);
    // name(5) \ count(2) \ location(19) + the terminator: the reference's
    // 57-byte USA reveal string.
    assert_eq!(reveal.len(), 57);
    let mut name = Vec::new();
    crate::charset::encode("Jill", crate::charset::Script::Latin, &mut name);
    let mut expected = Vec::new();
    for byte in name {
        expected.extend([byte, 0xFB]);
    }
    expected.extend([0x00, 0xFB]);
    assert_eq!(&reveal[..10], expected.as_slice(), "Jill as five cells");
    assert_eq!(
        &reveal[10..18],
        &[0x38, 0xFB, 0x0C, 0xFB, 0x0F, 0xFB, 0x38, 0xFB]
    );
    // The location field opens with the USA names' two blank cells.
    assert_eq!(&reveal[18..22], &[0x00, 0xFB, 0x00, 0xFB]);
    let mut label = Vec::new();
    crate::charset::encode("Hall 1F", crate::charset::Script::Latin, &mut label);
    let cells: Vec<u8> = reveal[22..22 + label.len() * 2]
        .chunks(2)
        .map(|cell| {
            assert_eq!(cell[1], 0xFB);
            cell[0]
        })
        .collect();
    assert_eq!(cells, label);
    assert_eq!(reveal[56], 0x01);

    // Every slice of the reveal draws, and the screen finishes back in
    // gameplay.
    let mut framebuffer = crate::render::Framebuffer::new();
    let mut result = ScreenResult::Continue;
    for _ in 0..1000 {
        screen.draw(&cx, &mut framebuffer);
        assert!(!screen.failed(), "the reveal draws");
        result = screen.update(&cx, neutral());
        if result != ScreenResult::Continue {
            break;
        }
    }
    assert_eq!(result, ScreenResult::Done(ScreenAction::Resume));

    // The overwrite dialog over the now-filled slot draws too.
    let mut screen = open_save(&mut cx, SaveFile::default(), false);
    screen.update(&cx, confirm());
    assert_eq!(stage(&screen), "confirm_overwrite");
    screen.draw(&cx, &mut framebuffer);
    assert!(!screen.failed());
}

#[test]
fn save_load_autosave_rows_use_the_selected_grid() {
    // An autosave row draws exactly like a filled slot row of the same block,
    // on either grid, and pushes the exit row down one row.
    let harness = Harness::new("sl-autosave-grid");
    let file = SaveFile {
        character: 0,
        stage: 0,
        room: 6,
        saves: 12,
        ..SaveFile::default()
    };
    save::save(&harness.dir.0, 0, &file).unwrap();
    crate::save::write_autosave(&harness.dir.0, &file).unwrap();
    let text = crate::text::Text::embedded();
    let western = western_font();
    let japanese = japanese_font();
    for font in [&western, &japanese] {
        let mut cx = harness.context();
        cx.font = Some(font);
        cx.text = Some(&text);
        let mut screen = open_load(&mut cx);
        assert_eq!(screen.test_eval::<i64>("return #M.state.autosaves"), 1);
        // Park the cursor on the exit row so neither compared row has it.
        screen.test_eval::<()>("M.state.cursor = 9 M.state.blink_state = true");
        let mut framebuffer = crate::render::Framebuffer::new();
        screen.draw(&cx, &mut framebuffer);
        assert!(!screen.failed());
        let row = |y0: usize| -> Vec<u8> {
            let stride = 320 * 4;
            (y0..y0 + 14)
                .flat_map(|y| framebuffer.rgba[y * stride..(y + 1) * stride].to_vec())
                .collect()
        };
        let slot = row(45);
        assert!(slot.iter().any(|&byte| byte != 0), "the slot row has ink");
        assert_eq!(row(45 + 8 * 16), slot, "the autosave row matches slot 1");
        assert!(
            row(45 + 9 * 16).iter().any(|&byte| byte != 0),
            "the exit row"
        );
    }
}

#[test]
fn save_load_japanese_fields_keep_their_authored_cells() {
    // The ja-JP names and save labels are authored cell-encoded (the
    // reference's STR_JP_CELL), so the reveal's cell padding leaves them
    // byte-identical: three and eight cells.
    let harness = Harness::new("sl-jpn-cells");
    let text = crate::text::Text::load(empty_pack(), Some("ja-JP"));
    let japanese = japanese_font();
    let mut cx = harness.context();
    cx.font = Some(&japanese);
    cx.text = Some(&text);
    let screen = open_load(&mut cx);
    let cells = |bytes: &[u8], count: usize| {
        let literal: String = bytes.iter().map(|byte| format!("\\x{byte:02X}")).collect();
        lua_bytes(&screen, &format!("return M.cells('{literal}', {count})"))
    };
    let strip = |bytes: &[u8]| bytes.strip_suffix(&[0x01]).unwrap_or(bytes).to_vec();
    for character in 0..2 {
        let authored = strip(text.save().character_name(character));
        assert_eq!(authored.len(), 6);
        assert_eq!(cells(&authored, 3), authored);
    }
    // One room of every save location the reference names.
    for (stage, room) in [
        (0, 0x00),
        (0, 0x06),
        (1, 0x00),
        (2, 0x00),
        (3, 0x00),
        (4, 0x00),
    ] {
        let authored = strip(text.save().location(stage, room));
        assert_eq!(authored.len(), 16, "stage {stage} room {room:02X}");
        assert_eq!(cells(&authored, 8), authored);
    }
}

/// A pack carrying the Western sheet, so a non-Japanese language's text is
/// encoded for it.
fn western_pack() -> &'static Pack {
    use std::sync::OnceLock;
    static PACK: OnceLock<Pack> = OnceLock::new();
    PACK.get_or_init(|| {
        let mut writer = crate::pack::PackWriter::new();
        writer
            .add("font/00.bmp", crate::font::WESTERN_SHEET.to_vec())
            .unwrap();
        writer
            .add("font/00.xml", crate::font::xml::WESTERN.as_bytes().to_vec())
            .unwrap();
        Pack::from_bytes(writer.to_bytes().unwrap()).unwrap()
    })
}

/// The Western languages the save screen draws on the USA grid.
const WESTERN_LANGUAGES: [&str; 7] = [
    "en-US", "de-DE", "es-ES", "fr-FR", "it-IT", "fi-FI", "ru-RU",
];

/// `M.header_x(api, mode)` and `M.confirm_line(api, bytes)`'s NO offset
/// with the API bound.
fn header_x(screen: &mut LuaScreen, cx: &UiContext<'_>, mode: i64) -> i64 {
    screen.test_eval_with_game(cx, None, &format!("return M.header_x(api, {mode})"))
}

fn width_of(screen: &mut LuaScreen, cx: &UiContext<'_>, bytes: &[u8]) -> i64 {
    let literal: String = bytes.iter().map(|byte| format!("\\x{byte:02X}")).collect();
    screen.test_eval_with_game(cx, None, &format!("return api:glyphs_width('{literal}')"))
}

#[test]
fn save_load_header_is_centred_on_the_reference_column() {
    let harness = Harness::new("sl-header");
    let western = western_font();
    // en-US on the USA grid: the nine cells of "Save game" start at the
    // reference's headerX.
    let text = crate::text::Text::load(western_pack(), Some("en-US"));
    let mut cx = harness.context();
    cx.font = Some(&western);
    cx.text = Some(&text);
    let mut screen = open_load(&mut cx);
    assert_eq!(header_x(&mut screen, &cx, 0), 124);
    assert_eq!(header_x(&mut screen, &cx, 1), 124);

    // ja-JP on the Japanese grid: "SAVE" under "     GAME", nine full
    // cells from 0x61.
    let japanese = japanese_font();
    let text = crate::text::Text::load(empty_pack(), Some("ja-JP"));
    cx.font = Some(&japanese);
    cx.text = Some(&text);
    let mut screen = open_load(&mut cx);
    assert_eq!(header_x(&mut screen, &cx, 0), 0x61);
    assert_eq!(header_x(&mut screen, &cx, 1), 0x61);

    // Every Western language's label is centred on the same column (x 160)
    // by its measured width; no suffix is inherited from en-US.
    for language in WESTERN_LANGUAGES {
        let text = crate::text::Text::load(western_pack(), Some(language));
        assert!(
            text.ui("UI_SAVE_HEADER_SUFFIX").is_empty(),
            "{language} draws its header as one label"
        );
        let mut cx = harness.context();
        cx.font = Some(&western);
        cx.text = Some(&text);
        let mut screen = open_load(&mut cx);
        for mode in 0..2 {
            let label = text.ui(&format!("UI_SAVE_HEADER_{mode}")).to_vec();
            let width = width_of(&mut screen, &cx, &label);
            let x = header_x(&mut screen, &cx, mode);
            assert!(x >= 0, "{language} header {mode} fits the frame");
            let centre2 = 2 * x + width;
            assert!(
                (319..=320).contains(&centre2),
                "{language} header {mode} centred: x {x}, width {width}"
            );
        }
    }
}

#[test]
fn save_load_yes_no_cursors_sit_on_the_blanks_before_each_answer() {
    let harness = Harness::new("sl-yes-no");
    let western = western_font();
    let mut cx = harness.context();

    // en-US keeps the reference exactly: " Yes  No" and a 40px stride.
    let text = crate::text::Text::load(western_pack(), Some("en-US"));
    cx.font = Some(&western);
    cx.text = Some(&text);
    let screen = open_load(&mut cx);
    assert_eq!(
        lua_bytes(&screen, "return M.state.confirm.bytes"),
        text.ui("UI_SAVE_YES_NO")
    );
    assert_eq!(screen.test_eval::<i64>("return M.state.confirm.no_dx"), 40);

    // ja-JP keeps the Japanese stride of four 14px cells.
    let japanese = japanese_font();
    let jtext = crate::text::Text::load(empty_pack(), Some("ja-JP"));
    cx.font = Some(&japanese);
    cx.text = Some(&jtext);
    let screen = open_load(&mut cx);
    assert_eq!(screen.test_eval::<i64>("return M.state.confirm.no_dx"), 56);

    // Every Western language: the line opens with a blank for the YES
    // cursor, and the NO cursor's arrow (7px of ink from where it is
    // drawn) lies between the two answers.
    for language in WESTERN_LANGUAGES {
        let text = crate::text::Text::load(western_pack(), Some(language));
        let mut cx = harness.context();
        cx.font = Some(&western);
        cx.text = Some(&text);
        let mut screen = open_load(&mut cx);
        let line = lua_bytes(&screen, "return M.state.confirm.bytes");
        let no_dx: i64 = screen.test_eval("return M.state.confirm.no_dx");
        let cells: Vec<u8> = line.iter().copied().filter(|&b| b != 0xFB).collect();
        assert_eq!(cells[0], 0x00, "{language}: a leading blank");
        let first_end = cells[1..].iter().position(|&b| b == 0).unwrap() + 1;
        let second = first_end + cells[first_end..].iter().position(|&b| b != 0).unwrap();
        let first_w = width_of(&mut screen, &cx, &cells[..first_end]);
        let second_x = width_of(&mut screen, &cx, &cells[..second]);
        assert!(
            no_dx >= first_w && no_dx + 7 <= second_x,
            "{language}: NO cursor {no_dx} between {first_w} and {second_x}"
        );
    }

    // The long Finnish YES moves the NO cursor past the reference stride;
    // a line without the leading blank gets one.
    let text = crate::text::Text::load(western_pack(), Some("fi-FI"));
    let mut cx = harness.context();
    cx.font = Some(&western);
    cx.text = Some(&text);
    let mut screen = open_load(&mut cx);
    assert!(screen.test_eval::<i64>("return M.state.confirm.no_dx") > 40);
    let mut line = |body: &str| -> (Vec<u8>, i64) {
        let (bytes, no_dx): (mlua::String, i64) =
            screen.test_eval_with_game(&cx, None, &format!("return M.confirm_line(api, '{body}')"));
        (bytes.as_bytes().to_vec(), no_dx)
    };
    // "YES  NO" without its leading blank: the blank is added, and the
    // reference stride lands between the answers, as in the USA original.
    let (bytes, no_dx) = line("\\x35\\x21\\x2F\\x00\\x00\\x2A\\x2B");
    assert_eq!(bytes, [0x00, 0x35, 0x21, 0x2F, 0x00, 0x00, 0x2A, 0x2B]);
    assert_eq!(no_dx, 40, "a reference-length pair keeps the stride");
    // " T  U": the stride would land on U, so the NO cursor takes the
    // blank before it (8 + 8 + 8).
    let (_, no_dx) = line("\\x00\\x30\\x00\\x00\\x31");
    assert_eq!(no_dx, 24, "a short pair puts the cursor on its blank");
}

#[test]
fn save_load_usa_cursor_ink_matches_the_fontus_arrow() {
    // The reference blits fontus.tim's 8px arrow cell at cursorX 47, its ink
    // one pixel in (x 48..52) and clear of the row text at 55. The Western
    // sheet's arrow is drawn so its ink starts at that same x and still ends
    // before 55.
    let harness = Harness::new("sl-cursor");
    let western = western_font();
    let text = crate::text::Text::load(western_pack(), Some("en-US"));
    let mut cx = harness.context();
    cx.font = Some(&western);
    cx.text = Some(&text);
    let mut screen = open_load(&mut cx);
    let frame = |screen: &mut LuaScreen, visible: bool| {
        screen.test_eval::<()>(&format!("M.state.blink_state = {}", !visible));
        let mut framebuffer = crate::render::Framebuffer::new();
        screen.draw(&cx, &mut framebuffer);
        assert!(!screen.failed());
        framebuffer.rgba
    };
    let shown = frame(&mut screen, true);
    let hidden = frame(&mut screen, false);
    let columns: Vec<usize> = (0..shown.len() / 4)
        .filter(|&i| shown[i * 4..i * 4 + 4] != hidden[i * 4..i * 4 + 4])
        .map(|i| i % 320)
        .collect();
    assert!(!columns.is_empty(), "the cursor draws");
    assert_eq!(columns.iter().min(), Some(&48));
    assert!(*columns.iter().max().unwrap() < 55, "{columns:?}");
}

#[test]
fn save_load_save_mode_cancel_returns_to_gameplay_without_writing() {
    let harness = Harness::new("sl-save-cancel");
    let mut cx = harness.context();
    let mut screen = open_save(&mut cx, SaveFile::default(), true);
    // The typewriter screen leaves on the cancel tick itself.
    assert_eq!(
        screen.update(&cx, cancel()),
        ScreenResult::Done(ScreenAction::Resume)
    );
    assert!(screen.take_outcome().is_none());
    assert!(save::read_slot_info(&harness.dir.0, 0).is_none());
}

#[test]
fn save_load_a_failed_write_shows_the_error_line_until_a_key() {
    let harness = Harness::new("sl-save-error");
    // A file where the save directory should be makes the write fail.
    let dir = harness.dir.0.join("saves");
    std::fs::write(&dir, b"not a directory").unwrap();
    let mut cx = harness.context();
    cx.save_dir = &dir;
    let mut screen = open_save(&mut cx, SaveFile::default(), false);
    screen.update(&cx, confirm());
    assert_eq!(stage(&screen), "error");
    assert!(screen.take_outcome().is_none());
    screen.update(&cx, press(|i| i.any = true));
    assert_eq!(stage(&screen), "idle");
}

#[test]
fn save_load_cues_follow_the_original_bank_slots() {
    let harness = Harness::new("sl-cues");
    save::save(&harness.dir.0, 0, &SaveFile::default()).unwrap();
    let mut cx = harness.context();
    let mut screen = open_load(&mut cx);

    screen.update(&cx, press(|i| i.down = true));
    assert_eq!(cursor(&screen), 1);
    assert_eq!(cx.cues.borrow().as_slice(), &[UiCue::SaveCursor]);
    cx.cues.borrow_mut().clear();

    // Slot 2 is empty, but the confirm slot still plays.
    screen.update(&cx, confirm());
    assert_eq!(cx.cues.borrow().as_slice(), &[UiCue::SaveConfirm]);
    assert_eq!(stage(&screen), "idle", "an empty slot refuses");
    cx.cues.borrow_mut().clear();

    // Up from the first row wraps to the exit row, which confirms with the
    // cancel slot.
    screen.update(&cx, press(|i| i.up = true));
    screen.update(&cx, press(|i| i.up = true));
    assert_eq!(cursor(&screen), 8);
    cx.cues.borrow_mut().clear();
    screen.update(&cx, confirm());
    assert_eq!(cx.cues.borrow().as_slice(), &[UiCue::Cancel]);
}

#[test]
fn save_load_the_overwrite_dialog_is_silent() {
    let harness = Harness::new("sl-silent");
    save::save(&harness.dir.0, 0, &SaveFile::default()).unwrap();
    let mut cx = harness.context();
    let mut screen = open_save(&mut cx, SaveFile::default(), false);
    screen.update(&cx, confirm());
    assert_eq!(stage(&screen), "confirm_overwrite");
    cx.cues.borrow_mut().clear();
    for input in [
        press(|i| i.left = true),
        press(|i| i.right = true),
        cancel(),
    ] {
        screen.update(&cx, input);
        assert!(cx.cues.borrow().is_empty(), "the dialog must be silent");
    }
}

// ---- return to title -------------------------------------------------------

#[test]
fn return_title_maps_confirm_cancel_and_idle() {
    let harness = Harness::new("return-title");
    let cx = harness.context();
    let mut screen = harness.open("return_title", &[]);
    assert_eq!(screen.update(&cx, neutral()), ScreenResult::Continue);
    // The F9 edge wins over the same tick's `any`.
    let f9 = press(|i| {
        i.return_title = true;
        i.any = true;
    });
    assert_eq!(
        screen.update(&cx, f9),
        ScreenResult::Done(ScreenAction::Title)
    );
    for other in [
        press(|i| {
            i.cancel = true;
            i.any = true;
        }),
        press(|i| {
            i.confirm = true;
            i.any = true;
        }),
    ] {
        let mut screen = harness.open("return_title", &[]);
        assert_eq!(
            screen.update(&cx, other),
            ScreenResult::Done(ScreenAction::Resume),
            "any other key cancels"
        );
    }
}

#[test]
fn return_title_dims_the_whole_frame_without_a_font() {
    let harness = Harness::new("return-title-dim");
    let cx = harness.context();
    let mut screen = harness.open("return_title", &[]);
    let mut framebuffer = crate::render::Framebuffer::new();
    framebuffer.fill_rect([0, 0, 320, 240], [255, 255, 255, 255]);
    screen.draw(&cx, &mut framebuffer);
    let pixel = |x: usize, y: usize| -> [u8; 4] {
        let offset = (y * 320 + x) * 4;
        framebuffer.rgba[offset..offset + 4].try_into().unwrap()
    };
    // 255 * (255 - 100) / 255 = 155.
    assert_eq!(pixel(0, 0), [155, 155, 155, 255]);
    assert_eq!(pixel(319, 239), [155, 155, 155, 255]);
}

#[test]
fn return_title_text_comes_from_the_language_files() {
    let text = crate::text::Text::embedded();
    for key in [
        "UI_PROMPT_RETURN_TITLE_1",
        "UI_PROMPT_RETURN_TITLE_2",
        "UI_PROMPT_RETURN_TITLE_3",
    ] {
        assert!(!text.ui(key).is_empty(), "{key}");
    }
}

// ---- death -----------------------------------------------------------------

fn death_screen(harness: &Harness) -> LuaScreen {
    harness.open(
        "death",
        &[
            ("x", ScreenArg::Int(1000)),
            ("y", ScreenArg::Int(0)),
            ("z", ScreenArg::Int(2000)),
            ("angle", ScreenArg::Int(0x100)),
            ("fov", ScreenArg::Int(221)),
        ],
    )
}

fn death_state(screen: &LuaScreen) -> i64 {
    screen.test_eval("return M.state.state")
}

fn death_strip(screen: &LuaScreen) -> Option<i64> {
    screen.test_eval("return M.state.strip")
}

fn death_rect(screen: &LuaScreen) -> i64 {
    screen.test_eval("return M.state.rect")
}

/// The screen's fade overlay: colour name and alpha.
fn death_overlay(screen: &LuaScreen) -> (Option<String>, Option<i64>) {
    screen.test_eval("return M.state.fade:overlay()")
}

#[test]
fn death_the_screen_machine_runs_its_states_in_order() {
    let harness = Harness::new("death-machine");
    let cx = harness.context();
    let mut screen = death_screen(&harness);
    let camera: (Vec<i64>, Vec<i64>) = screen.test_eval("return M.state.from, M.state.to");
    assert_eq!(camera, (vec![6000, -7000, 2000], vec![1000, -1000, 2000]));

    // The entry arms the white flash: the first frame is fully white.
    assert_eq!(death_overlay(&screen), (Some("white".into()), Some(255)));
    assert_eq!(death_state(&screen), 1);

    // The flash decays over 86 ticks, then the reveal starts on the 87th.
    assert_eq!(death_rect(&screen), 0);
    let mut reveal = 0;
    while death_state(&screen) == 1 {
        assert_eq!(screen.update(&cx, neutral()), ScreenResult::Continue);
        reveal += 1;
        assert!(reveal < 200, "the flash must clear");
    }
    assert_eq!(reveal, 87);

    // The black ramp starts at zero; the strip starts stepping once the ramp
    // passes the threshold, 21 ticks later.
    let mut first_strip = None;
    let mut stepped = 0;
    for tick in 0..200 {
        screen.update(&cx, neutral());
        if death_strip(&screen).is_some() {
            first_strip.get_or_insert((tick, death_rect(&screen)));
            stepped += 1;
        }
        if death_state(&screen) == 3 {
            break;
        }
    }
    let (tick, alpha) = first_strip.expect("the strip draws");
    assert_eq!(alpha, 63, "the first strip frame is past the threshold");
    assert_eq!(tick, 21);
    assert_eq!(stepped, 56, "59 down to 4 is 56 stepped frames");

    // The hold runs 49 frames at the settled amplitude.
    for _ in 0..=0x30 {
        assert_eq!(screen.update(&cx, neutral()), ScreenResult::Continue);
    }
    assert_eq!(death_state(&screen), 4);
    // The black fade-out arms and re-waves the strip; the overlay holds the
    // last ramped value.
    screen.update(&cx, neutral());
    assert_eq!(death_rect(&screen), 228);
    assert_eq!(death_overlay(&screen).0.as_deref(), Some("black"));
    let mut rewave = Vec::new();
    let mut finished = None;
    for _ in 0..200 {
        if let ScreenResult::Done(action) = screen.update(&cx, neutral()) {
            finished = Some(action);
            break;
        }
        rewave.extend(death_strip(&screen));
    }
    assert_eq!(finished, Some(ScreenAction::Title));
    assert_eq!(
        rewave[..2],
        [4, 5],
        "the re-wave starts at the settled value"
    );
}

#[test]
fn death_the_corpse_spins_eight_units_a_tick() {
    let harness = Harness::new("death-spin");
    let cx = harness.context();
    let mut screen = death_screen(&harness);
    screen.update(&cx, neutral());
    assert_eq!(screen.value("spin"), 0x108);
    screen.update(&cx, neutral());
    assert_eq!(screen.value("spin"), 0x110);
}

#[test]
fn death_the_confirm_button_skips_the_screen() {
    let harness = Harness::new("death-skip");
    let cx = harness.context();
    let mut screen = death_screen(&harness);
    assert_eq!(screen.update(&cx, neutral()), ScreenResult::Continue);
    assert_eq!(
        screen.update(&cx, press(|i| i.confirm = true)),
        ScreenResult::Done(ScreenAction::Title)
    );
}

#[test]
fn death_the_strip_math_matches_the_original() {
    let harness = Harness::new("death-strip");
    let screen = death_screen(&harness);
    let base = |param: i64| -> i64 { screen.test_eval(&format!("return M.strip_base({param})")) };
    let wave = |param: i64, column: i64| -> i64 {
        screen.test_eval(&format!("return M.strip_wave({param}, {column})"))
    };
    assert_eq!(base(59), -152);
    assert_eq!(base(3), -44);
    assert_eq!(wave(59, 1), 665);
    assert_eq!(wave(59, 2), 1230);
    assert_eq!(wave(3, 1), 1);
    // The unsigned shift wraps negative sine products below the baseline.
    assert_eq!(wave(59, 255), -666);
    assert_eq!(wave(3, 255), -2);
    // The 16-entry sine table is the original's fix12 sine at every column
    // phase: `(int)(sin(phase * 2^-12 * 2pi) * 4096)`.
    let fsin = |angle: i64| -> i64 {
        ((angle as f64 * (1.0 / 4096.0) * 6.283_185_482_025_146_5).sin() * 4096.0) as i64
    };
    for column in 0..256 {
        let expected = ((fsin(column * 0x100) * 59 * 59) as u32 >> 13) as u16 as i16;
        assert_eq!(wave(59, column), i64::from(expected), "column {column}");
    }
}

// ---- computer_lab ----------------------------------------------------------

/// The lab terminal over the embedded language file (its credentials).
struct Lab {
    harness: Harness,
    text: crate::text::Text,
    screen: LuaScreen,
}

impl Lab {
    fn new(label: &str) -> Self {
        let harness = Harness::new(label);
        let text = crate::text::Text::embedded();
        let mut cx = UiContext {
            authentic: false,
            text: Some(&text),
            ..harness.context()
        };
        let screen =
            LuaScreen::open_screen("computer_lab", &[], ScreenAction::Quit, &mut cx).unwrap();
        Self {
            harness,
            text,
            screen,
        }
    }

    fn step(&mut self, input: UiInput) -> ScreenResult {
        let cx = UiContext {
            authentic: false,
            text: Some(&self.text),
            ..self.harness.context()
        };
        self.screen.update(&cx, input)
    }

    fn state(&self, field: &str) -> String {
        self.screen
            .test_eval(&format!("return tostring(M.state.{field})"))
    }

    fn cell(&self) -> u8 {
        self.state("cell").parse().unwrap()
    }

    /// Navigate to `cell` (rows first, like the covered cells need) and
    /// commit it.
    fn press_cell(&mut self, cell: u8) {
        for _ in 0..64 {
            let current = self.cell();
            if current == cell {
                break;
            }
            let (row, col) = (current / 8, current % 8);
            self.step(press(|i| {
                if row < cell / 8 {
                    i.down = true;
                } else if row > cell / 8 {
                    i.up = true;
                } else if col < cell % 8 {
                    i.right = true;
                } else {
                    i.left = true;
                }
            }));
        }
        assert_eq!(self.cell(), cell, "cursor reached cell {cell:#x}");
        self.step(press(|i| i.confirm = true));
    }

    /// Type char codes (`1` = `A`) and submit them with ENTER.
    fn type_codes(&mut self, codes: &[u8]) {
        for &code in codes {
            self.press_cell(if code <= 0x16 { code } else { code + 1 });
        }
        self.press_cell(0x17);
    }

    fn hold(&mut self) {
        for _ in 0..90 {
            assert_eq!(self.step(neutral()), ScreenResult::Continue);
        }
    }
}

#[test]
fn the_lab_login_accepts_john_and_ada_then_opens_the_menu() {
    let mut lab = Lab::new("lab-login");
    lab.type_codes(&[10, 15, 8, 14]);
    assert_eq!(lab.state("sub"), "secret");
    lab.type_codes(&[1, 4, 1]);
    assert_eq!(lab.state("sub"), "accepted");
    lab.hold();
    assert_eq!(lab.state("stage"), "menu");
}

#[test]
fn the_lab_rejects_a_wrong_password_and_restarts_the_login() {
    let mut lab = Lab::new("lab-reject");
    lab.type_codes(&[10, 15, 8, 14]);
    lab.type_codes(&[1, 4, 2]);
    assert_eq!(lab.state("sub"), "rejected");
    lab.hold();
    assert_eq!(lab.state("stage"), "login");
    assert_eq!(lab.state("sub"), "user");
    assert_eq!(lab.cell(), 1);
}

#[test]
fn mole_releases_the_selected_lab_door() {
    let mut lab = Lab::new("lab-mole");
    lab.type_codes(&[10, 15, 8, 14]);
    lab.type_codes(&[1, 4, 1]);
    lab.hold();
    lab.step(press(|i| i.down = true));
    lab.step(press(|i| i.confirm = true));
    assert_eq!(lab.state("stage"), "command");
    lab.type_codes(&[13, 15, 12, 5]);
    assert_eq!(lab.state("sub"), "accepted");
    assert_eq!(lab.screen.value("released_door"), 0);
    lab.hold();
    assert_eq!(lab.state("stage"), "unlocked");
    assert_eq!(lab.screen.value("released_door"), 2);
    // The unlock hold then closes the terminal.
    for _ in 0..89 {
        assert_eq!(lab.step(neutral()), ScreenResult::Continue);
    }
    assert_eq!(lab.step(neutral()), ScreenResult::Done(ScreenAction::Quit));
}

#[test]
fn the_lab_keyboard_skips_the_covered_cells() {
    let mut lab = Lab::new("lab-cursor");
    // Row 3 right of BACKSPACE (0x1d) is covered: the cursor steps back.
    lab.press_cell(0x1d);
    lab.step(press(|i| i.right = true));
    assert_eq!(lab.cell(), 0x1d);
    // Down from row 2's last column steps back up out of 0x1f.
    lab.step(press(|i| i.up = true));
    lab.step(press(|i| i.right = true));
    lab.step(press(|i| i.right = true));
    assert_eq!(lab.cell(), 0x17);
    lab.step(press(|i| i.down = true));
    assert_eq!(lab.cell(), 0x17);
}

#[test]
fn cancel_leaves_the_lab_terminal_from_the_menu() {
    let mut lab = Lab::new("lab-cancel");
    lab.type_codes(&[10, 15, 8, 14]);
    lab.type_codes(&[1, 4, 1]);
    lab.hold();
    assert_eq!(
        lab.step(press(|i| i.cancel = true)),
        ScreenResult::Done(ScreenAction::Quit)
    );
    assert_eq!(lab.screen.value("released_door"), 0);
}

// ---- map -----------------------------------------------------------------

/// An 8bpp TIM of one row: `palette` maps index to a BGR555 colour.
fn map_tim(pixels: &[u8], palette: impl Fn(u16) -> u16) -> Vec<u8> {
    map_tim_rows(pixels.len(), pixels, palette)
}

/// An 8bpp TIM of `width`-texel rows.
fn map_tim_rows(width: usize, pixels: &[u8], palette: impl Fn(u16) -> u16) -> Vec<u8> {
    let height = (pixels.len() / width) as u16;
    let mut data = Vec::new();
    data.extend_from_slice(&0x10u32.to_le_bytes());
    data.extend_from_slice(&9u32.to_le_bytes());
    data.extend_from_slice(&524u32.to_le_bytes());
    data.extend_from_slice(&[0, 0, 0, 0]);
    data.extend_from_slice(&256u16.to_le_bytes());
    data.extend_from_slice(&1u16.to_le_bytes());
    for index in 0..256u16 {
        data.extend_from_slice(&palette(index).to_le_bytes());
    }
    data.extend_from_slice(&(12u32 + pixels.len() as u32).to_le_bytes());
    data.extend_from_slice(&[0, 0, 0, 0]);
    data.extend_from_slice(&(width as u16 / 2).to_le_bytes());
    data.extend_from_slice(&height.to_le_bytes());
    data.extend_from_slice(pixels);
    data
}

/// The map tab over a game whose room-flags bank has `bits` set.
struct MapTab {
    harness: Harness,
    pack: Pack,
    game: crate::game::GameState,
    screen: LuaScreen,
    /// The cues the last [`MapTab::step`] queued.
    cues: Vec<UiCue>,
}

impl MapTab {
    fn new(label: &str, pack: Pack, at: (u8, u8, usize), bits: &[u8]) -> Self {
        Self::with_flags(label, pack, at, bits, &[])
    }

    /// Like [`MapTab::new`], plus `(bank, bit)` scenario flags.
    fn with_flags(
        label: &str,
        pack: Pack,
        at: (u8, u8, usize),
        bits: &[u8],
        flags: &[(u8, u8)],
    ) -> Self {
        Self::with_game(label, pack, map_game(at, bits, flags))
    }

    fn with_game(label: &str, pack: Pack, game: crate::game::GameState) -> Self {
        let harness = Harness::new(label);
        let screen = {
            let mut cx = UiContext {
                authentic: false,
                pack: &pack,
                game: Some(&game),
                ..harness.context()
            };
            LuaScreen::open_screen("map", &[], ScreenAction::Quit, &mut cx).unwrap()
        };
        Self {
            harness,
            pack,
            game,
            screen,
            cues: Vec::new(),
        }
    }

    fn step(&mut self, input: UiInput) -> ScreenResult {
        let cx = UiContext {
            authentic: false,
            pack: &self.pack,
            game: Some(&self.game),
            ..self.harness.context()
        };
        let result = self.screen.update(&cx, input);
        self.cues = cx.cues.borrow().clone();
        result
    }

    fn draw(&mut self) -> crate::render::Framebuffer {
        let cx = UiContext {
            authentic: false,
            pack: &self.pack,
            game: Some(&self.game),
            ..self.harness.context()
        };
        let mut framebuffer = crate::render::Framebuffer::new();
        self.screen.draw(&cx, &mut framebuffer);
        framebuffer
    }

    fn state(&self, expr: &str) -> i64 {
        self.screen.test_eval(&format!("return {expr}"))
    }
}

/// A game in `(stage, room, cut)` with room-flags `bits` and `(bank, bit)`
/// flags set.
fn map_game(
    (stage, room, cut): (u8, u8, usize),
    bits: &[u8],
    flags: &[(u8, u8)],
) -> crate::game::GameState {
    let mut game = crate::game::GameState::default();
    game.id.stage = stage;
    game.id.room = room;
    game.camera.current_cut = cut;
    for &bit in bits {
        game.flags[usize::from(crate::game::BANK_ROOM_FLAGS)].apply(bit, 0);
    }
    for &(bank, bit) in flags {
        game.flags[usize::from(bank)].apply(bit, 0);
    }
    game
}

fn no_map_pack() -> Pack {
    let mut writer = crate::pack::PackWriter::new();
    writer.add("unused", Vec::new()).unwrap();
    Pack::from_bytes(writer.to_bytes().unwrap()).unwrap()
}

#[test]
fn map_tables_blob_overrides_need_the_magic_and_size() {
    let tab = MapTab::new("map-tables", no_map_pack(), (1, 1, 0), &[]);
    let parsed: i64 = tab.state(r#"M.parse_tables("MAP1" .. string.rep("\7", 68)).area[15]"#);
    assert_eq!(parsed, 7);
    let short: bool = tab
        .screen
        .test_eval(r#"return M.parse_tables("MAP1" .. string.rep("\7", 67)) == nil"#);
    assert!(short);
    let magic: bool = tab
        .screen
        .test_eval(r#"return M.parse_tables("MAP2" .. string.rep("\7", 68)) == nil"#);
    assert!(magic);
}

#[test]
fn map_area_table_and_initial_view_follow_the_reference() {
    let tab = MapTab::new("map-initial", no_map_pack(), (1, 1, 0), &[]);
    for (layout, room, area) in [
        (0, 0, 2),
        (0, 1, 0),
        (0, 2, 1),
        (0, 3, 0xFF),
        (3, 0, 10),
        (3, 3, 7),
    ] {
        assert_eq!(tab.state(&format!("M.area_at({layout}, {room})")), area);
    }
    for ((group, room, cut), area) in [
        ((0, 0x01, 0), 0),
        ((0, 0x07, 4), 0),
        ((0, 0x10, 0), 2),
        ((1, 0x0C, 5), 0),
        ((1, 0x0F, 2), 1),
        ((2, 0x0B, 5), 4),
        ((3, 0x0C, 0), 5),
        ((3, 0x0D, 0), 6),
        ((4, 0x13, 0), 10),
        ((4, 0x07, 0), 9),
        ((4, 0x01, 0), 7),
        ((4, 0x03, 0), 8),
    ] {
        assert_eq!(
            tab.state(&format!("M.initial_area({group}, {room}, {cut})")),
            area,
            "group {group} room {room:#x} cut {cut}"
        );
    }
    // Room 1001 opens on Mansion 1F, layout 0's room slot 1.
    assert_eq!(tab.state("M.state.area"), 0);
    assert_eq!(tab.state("M.state.layout"), 0);
    assert_eq!(tab.state("M.state.room[0]"), 1);
}

#[test]
fn map_navigation_steps_rooms_and_zooms() {
    // Room 1 of group 0 (area 0) and a group-1 room in area 1 make the
    // layout-0 up step reachable; room 0 (area 2) is not.
    let mut tab = MapTab::new("map-nav", no_map_pack(), (1, 1, 0), &[1, 32]);
    assert_eq!(tab.step(neutral()), ScreenResult::Continue);
    tab.step(press(|i| i.down = true));
    assert_eq!(tab.state("M.state.room[0]"), 1, "area 2 is not reachable");
    tab.step(press(|i| i.up = true));
    assert_eq!(tab.state("M.state.room[0]"), 2);
    assert_eq!(tab.state("M.state.area"), 1);
    tab.step(press(|i| i.up = true));
    assert_eq!(
        tab.state("M.state.room[0]"),
        2,
        "room 2 is the last real room"
    );
    tab.step(press(|i| i.down = true));
    assert_eq!(tab.state("M.state.room[0]"), 1);
    // Confirm enters the full display; cancel shrinks it back to the
    // browser over several frames, then cancel closes.
    tab.step(press(|i| i.confirm = true));
    assert_eq!(tab.screen.value("zoomed"), 1);
    tab.step(press(|i| i.up = true));
    assert_eq!(tab.state("M.state.room[0]"), 1, "the display ignores steps");
    tab.step(press(|i| i.cancel = true));
    let mut frames = 0;
    while tab.screen.value("zoomed") == 1 {
        assert_eq!(tab.step(neutral()), ScreenResult::Continue);
        frames += 1;
        assert!(frames < 40, "the display never closed");
    }
    assert!(frames > 10, "the close is animated: {frames} frames");
    assert_eq!(
        tab.step(press(|i| i.cancel = true)),
        ScreenResult::Done(ScreenAction::Quit)
    );
    // A cancel while the display shrinks leaves the tab outright.
    let mut tab = MapTab::new("map-nav-close", no_map_pack(), (1, 1, 0), &[1, 32]);
    tab.step(neutral());
    tab.step(press(|i| i.confirm = true));
    tab.step(neutral());
    tab.step(press(|i| i.cancel = true));
    assert_eq!(
        tab.step(press(|i| i.cancel = true)),
        ScreenResult::Done(ScreenAction::Quit)
    );
}

#[test]
fn map_owned_maps_unlock_the_layout_steps() {
    let base = crate::game::ROOM_FLAG_MAP_BASE;
    // Only Mansion 1F is known: there is no other layout to step to.
    let mut tab = MapTab::new("map-layout-1f", no_map_pack(), (1, 1, 0), &[base]);
    tab.step(neutral());
    tab.step(press(|i| i.left = true));
    assert_eq!(tab.state("M.state.layout"), 0);
    // The laboratory map (areas 8/9, layout 3) makes all four layouts wrap in
    // pairs: left from layout 0 lands on layout 3 and right returns.
    let mut tab = MapTab::new(
        "map-layout-lab",
        no_map_pack(),
        (1, 1, 0),
        &[base, base + 5],
    );
    tab.step(neutral());
    tab.step(press(|i| i.left = true));
    assert_eq!(tab.state("M.state.layout"), 3);
    assert_eq!(tab.state("M.state.area"), 7, "layout 3's untouched slot 3");
    tab.step(press(|i| i.right = true));
    assert_eq!(tab.state("M.state.layout"), 0);
}

#[test]
fn map_display_reddens_the_current_room_over_the_grid() {
    const WHITE: u16 = 0x7FFF;
    const GREEN: u16 = 0x03E0;
    const BLUE: u16 = 0x7C00;
    // Room 1110's first camera opens on area 2 (layout 0's room slot 0),
    // map group 1; the player's room is visited.
    let probe = MapTab::new("map-display-probe", no_map_pack(), (1, 0x10, 0), &[]);
    assert_eq!(probe.state("M.state.area"), 2);
    let room = probe.state("M.state.map_room");
    let visited =
        probe.state("M.state.tables.stage_room_offsets[M.state.map_group] + M.state.map_room");
    // The player's room's fill (its six entries' second); another room's
    // door marker - unvisited on an unknown map, so cleared.
    let fill = u8::try_from(13 + room * 6 + 1).unwrap();
    let other = if room == 0 { 19 } else { 13 };
    let mut writer = crate::pack::PackWriter::new();
    // The grid's index 0 is a solid colour; index 1 is the grid line, under
    // the plan's fourth texel (the grid's left half rests at (32, 24)).
    let mut grid = [0u8; 80 * 5];
    grid[4 * 80 + 16 + 3] = 1;
    writer
        .add(
            "map/bg.tim",
            map_tim_rows(80, &grid, |index| match index {
                0 => BLUE,
                1 => GREEN,
                _ => 0,
            }),
        )
        .unwrap();
    // The 2F plan: the keyed index 0 (a black CLUT word), the player's
    // fill, an index past the room table (200) and the other room's marker.
    writer
        .add(
            "map/03.tim",
            map_tim(
                &[0, fill, 200, other],
                |index| if index == 0 { 0 } else { WHITE },
            ),
        )
        .unwrap();
    let pack = Pack::from_bytes(writer.to_bytes().unwrap()).unwrap();
    let mut tab = MapTab::new(
        "map-display",
        pack,
        (1, 0x10, 0),
        &[u8::try_from(visited).unwrap()],
    );
    tab.step(neutral());
    tab.step(press(|i| i.confirm = true));
    // Let the zoom settle at rest (state 6).
    for _ in 0..40 {
        tab.step(neutral());
    }
    assert_eq!(tab.state("M.state.zoom"), 6);
    let framebuffer = tab.draw();
    let pixel = |x: u32| {
        let offset = ((28 * framebuffer.width + 48 + x) * 4) as usize;
        [
            framebuffer.rgba[offset],
            framebuffer.rgba[offset + 1],
            framebuffer.rgba[offset + 2],
        ]
    };
    let blue = pixel(0);
    assert!(
        blue[2] > 0 && blue[0] == 0 && blue[1] == 0,
        "the keyed texel shows the half-blended grid: {blue:?}"
    );
    let red = pixel(1);
    assert!(
        red[0] > 0 && red[1] == 0,
        "the current room's fill glows red over the grid: {red:?}"
    );
    let white = pixel(2);
    assert!(
        white.iter().all(|&c| c > 0xC0),
        "a plain entry draws: {white:?}"
    );
    let green = pixel(3);
    assert!(
        green[1] > 0 && green[0] == 0 && green[2] == 0,
        "unvisited markers show the grid: {green:?}"
    );
}

#[test]
fn map_cues_follow_the_reference_play_sfx_calls() {
    // Room 1001 with only Mansion 1F's rooms visited: the down step is
    // refused, the up step is not.
    let mut tab = MapTab::new("map-cues", no_map_pack(), (1, 1, 0), &[1, 32]);
    tab.step(neutral());
    tab.step(press(|i| i.down = true));
    assert!(tab.cues.is_empty(), "a refused step is silent");
    tab.step(press(|i| i.up = true));
    assert_eq!(tab.cues, [UiCue::Cursor]);
    tab.step(press(|i| i.left = true));
    assert!(tab.cues.is_empty(), "no other layout to step to");
    // Confirm is silent; the display's first frame plays the zoom.
    tab.step(press(|i| i.confirm = true));
    assert!(tab.cues.is_empty());
    tab.step(neutral());
    assert_eq!(tab.cues, [UiCue::MapZoom]);
    for _ in 0..40 {
        tab.step(neutral());
    }
    assert!(tab.cues.is_empty(), "no objective, no label cue");
    tab.step(press(|i| i.cancel = true));
    assert_eq!(tab.cues, [UiCue::Cancel]);
    // A confirm during the shrink re-opens with the zoom cue.
    tab.step(press(|i| i.confirm = true));
    assert_eq!(tab.cues, [UiCue::MapZoom]);
    assert_eq!(tab.state("M.state.zoom"), 3);
    tab.step(press(|i| i.cancel = true));
    assert_eq!(
        tab.step(press(|i| i.cancel = true)),
        ScreenResult::Done(ScreenAction::Quit)
    );
    assert_eq!(tab.cues, [UiCue::Cancel]);
}

#[test]
fn map_objective_label_blinks_over_its_area() {
    const RED: u16 = 0x001F;
    // Mansion 1F's plan with the label's 8x8 frames at row 104: frame `n`
    // fills its first `n` rows.
    let mut plan = vec![0u8; 128 * 112];
    for frame in 0..8 {
        for row in 0..frame {
            for col in 0..8 {
                plan[(104 + row) * 128 + frame * 8 + col] = 214;
            }
        }
    }
    let mut writer = crate::pack::PackWriter::new();
    writer
        .add(
            "map/01.tim",
            map_tim_rows(128, &plan, |index| if index == 214 { RED } else { 0 }),
        )
        .unwrap();
    let pack = Pack::from_bytes(writer.to_bytes().unwrap()).unwrap();
    // Chris's first serum objective is outstanding (second scenario bank).
    let flags = [(crate::game::BANK_SCENARIO2, 0x2D)];
    let mut tab = MapTab::with_flags("map-label", pack, (1, 1, 0), &[1], &flags);
    assert_eq!(tab.state("M.state.objective"), 2);
    tab.step(neutral());
    tab.step(press(|i| i.confirm = true));
    let mut label_frame = None;
    for frame in 0..60 {
        tab.step(neutral());
        if tab.cues.contains(&UiCue::MapLabel) {
            label_frame = Some(frame);
            break;
        }
    }
    let first = label_frame.expect("the label cue played");
    assert_eq!(tab.state("M.state.zoom"), 6);
    // The label starts on frame 0 (empty) and grows two counts a frame.
    for _ in 0..8 {
        tab.step(neutral());
    }
    assert_eq!(tab.state("M.state.label_state"), 3);
    let frame = tab.state("M.state.label_frame");
    assert!(frame > 0, "the square has grown: frame {frame}");
    let framebuffer = tab.draw();
    // The menu origin sits at the 320x240 frame's (0, 0); the label's
    // first row is at (0x38, 0x34).
    let offset = ((0x34 * framebuffer.width + 0x38) * 4) as usize;
    assert!(
        framebuffer.rgba[offset] > 0xC0 && framebuffer.rgba[offset + 1] == 0,
        "the label draws red: {:?}",
        &framebuffer.rgba[offset..offset + 3]
    );
    // The blink repeats with its cue: grow, shrink, then 20 hidden frames.
    let mut again = None;
    for frame in 0..80 {
        tab.step(neutral());
        if tab.cues.contains(&UiCue::MapLabel) {
            again = Some(frame);
            break;
        }
    }
    assert!(
        again.is_some(),
        "the label blinks again after {first} frames"
    );
    // The close parks the label until the plan is back at rest.
    tab.step(press(|i| i.cancel = true));
    tab.step(neutral());
    assert_eq!(tab.state("M.state.label_state"), 1);
    assert_eq!(tab.state("M.state.label_frame == nil and 1 or 0"), 1);
}

#[test]
fn map_objective_follows_the_scenario_flags() {
    use crate::game::{BANK_SCENARIO, BANK_SCENARIO2};
    for (label, (stage, player), flags, objective) in [
        ("none", (1, 0), &[][..], 0),
        ("serum1", (1, 0), &[(BANK_SCENARIO2, 0x2D)][..], 2),
        (
            "serum1-acked",
            (1, 0),
            &[(BANK_SCENARIO2, 0x2D), (BANK_SCENARIO, 0x49)][..],
            0,
        ),
        ("serum2", (2, 0), &[(BANK_SCENARIO2, 0x2E)][..], 2),
        ("serum2-outside", (3, 0), &[(BANK_SCENARIO2, 0x2E)][..], 0),
        ("plant42", (3, 0), &[(BANK_SCENARIO2, 0x38)][..], 4),
        ("jill-serum", (1, 1), &[(BANK_SCENARIO2, 0x23)][..], 2),
        ("jill-plant42", (3, 1), &[(BANK_SCENARIO2, 0x38)][..], 0),
    ] {
        let mut game = map_game((stage, 1, 0), &[], flags);
        game.id.player_flag = player;
        let tab = MapTab::with_game(label, no_map_pack(), game);
        assert_eq!(tab.state("M.state.objective"), objective, "{label}");
    }
}

#[test]
fn map_area_plans_use_the_reference_file_numbers() {
    let tab = MapTab::new("map-stems", no_map_pack(), (1, 1, 0), &[]);
    for (area, stem) in [
        (0, "01"),
        (4, "05"),
        (5, "0a"),
        (6, "0b"),
        (7, "06"),
        (10, "09"),
    ] {
        let found: String = tab.screen.test_eval(&format!("return M.area_stem({area})"));
        assert_eq!(found, format!("map/{stem}"), "area {area}");
    }
}

#[test]
fn map_without_art_stays_blank() {
    let mut tab = MapTab::new("map-blank", no_map_pack(), (1, 1, 0), &[1]);
    tab.step(neutral());
    let framebuffer = tab.draw();
    assert!(framebuffer.rgba.iter().all(|&c| c == 0));
    tab.step(press(|i| i.confirm = true));
    let framebuffer = tab.draw();
    assert!(framebuffer.rgba.iter().all(|&c| c == 0));
}

/// The FILE tab over a game state whose FILE-collected bits are `files`.
struct FileTab {
    harness: Harness,
    pack: &'static Pack,
    text: Option<&'static crate::text::Text>,
    game: crate::game::GameState,
    screen: LuaScreen,
    /// The cues the last [`FileTab::step`] queued.
    cues: Vec<UiCue>,
}

impl FileTab {
    fn new(label: &str, character: u8, files: &[u8], args: &[(&str, ScreenArg)]) -> Self {
        Self::with_pack(label, character, files, args, empty_pack(), None)
    }

    /// The tab over `pack` with a session in `language` (no text when None).
    fn with_pack(
        label: &str,
        character: u8,
        files: &[u8],
        args: &[(&str, ScreenArg)],
        pack: &'static Pack,
        language: Option<&str>,
    ) -> Self {
        let text: Option<&'static crate::text::Text> =
            language.map(|tag| &*Box::leak(Box::new(crate::text::Text::load(pack, Some(tag)))));
        let harness = Harness::new(label);
        let mut game = crate::game::GameState::default();
        game.id.player_flag = character;
        for &file in files {
            game.flags[usize::from(crate::game::BANK_ROOM_FLAGS)].apply(0x82 + file, 0);
        }
        let screen = {
            let mut cx = UiContext {
                authentic: false,
                pack,
                text,
                game: Some(&game),
                ..harness.context()
            };
            LuaScreen::open_screen("file", args, ScreenAction::Quit, &mut cx).unwrap()
        };
        Self {
            harness,
            pack,
            text,
            game,
            screen,
            cues: Vec::new(),
        }
    }

    fn step(&mut self, input: UiInput) -> ScreenResult {
        let cx = UiContext {
            authentic: false,
            pack: self.pack,
            text: self.text,
            game: Some(&self.game),
            ..self.harness.context()
        };
        let result = self.screen.update(&cx, input);
        self.cues = cx.cues.borrow().clone();
        result
    }

    fn tick(&mut self) {
        assert!(matches!(self.step(neutral()), ScreenResult::Continue));
    }

    fn draw(&mut self) -> crate::render::Framebuffer {
        let cx = UiContext {
            authentic: false,
            pack: self.pack,
            text: self.text,
            game: Some(&self.game),
            ..self.harness.context()
        };
        let mut framebuffer = crate::render::Framebuffer::new();
        framebuffer.fill_rect([0, 0, 320, 240], [9, 9, 9, 255]);
        self.screen.draw(&cx, &mut framebuffer);
        framebuffer
    }

    fn int(&self, expr: &str) -> i64 {
        self.screen.test_eval(&format!("return {expr}"))
    }

    fn flag(&self, expr: &str) -> bool {
        self.screen.test_eval(&format!("return {expr}"))
    }

    fn state(&self) -> String {
        self.screen.test_eval("return M.state.reader_state")
    }

    fn mode(&self) -> String {
        self.screen.test_eval("return M.state.mode")
    }

    /// Advance the selector until its entry slide settles.
    fn settle_list(&mut self) {
        for _ in 0..16 {
            if self.flag("M.state.list_ready") {
                return;
            }
            self.tick();
        }
        panic!("the selector entry slide did not settle");
    }

    /// Advance until the reader (or the selector it returned to) reads the
    /// pad again; a pick-up reader's finished exit is reported as `Done`.
    fn settle_reader(&mut self) -> ScreenResult {
        for _ in 0..128 {
            let settled = self.screen.value("locked") == 0
                && (self.mode() == "list" || self.state() == "idle");
            if settled {
                return ScreenResult::Continue;
            }
            let result = self.step(neutral());
            if !matches!(result, ScreenResult::Continue) {
                return result;
            }
        }
        panic!("the reader slide did not settle");
    }
}

#[test]
fn file_tables_match_the_shipped_jpn_tables() {
    let tab = FileTab::new("file-tables", 0, &[], &[]);
    let chris: Vec<i64> = tab.screen.test_eval(
        "local t = {} for i = 0, 15 do t[i + 1] = M.tables.key_items[0][i] end return t",
    );
    assert_eq!(
        chris,
        [
            0x0F, 0x02, 0xFE, 0x03, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0C, 0x0D, 0x0E, 0xFF,
            0xFF, 0xFF
        ]
    );
    let jill: Vec<i64> = tab.screen.test_eval(
        "local t = {} for i = 0, 15 do t[i + 1] = M.tables.key_items[1][i] end return t",
    );
    assert_eq!(
        jill,
        [
            0x0F, 0x02, 0xFE, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D,
            0x0E, 0xFF
        ]
    );
    let stem: String = tab.screen.test_eval("return M.page_stem(0x0F, 5, 'ja-JP')");
    assert_eq!(stem, "item/6e/file_fg_05_ja-JP");
    assert_eq!(tab.int("M.tables.title_x[0]"), 42);
    assert_eq!(tab.int("M.tables.title_x[15]"), 63);
}

#[test]
fn file_list_entries_resolve_against_the_file_flags() {
    // Nothing collected: the book has no selection.
    assert_eq!(
        FileTab::new("file-none", 0, &[], &[]).int("M.state.slot"),
        0xFF
    );
    // The 0xFE marker (slot 2) lists document 0 or 1 once either is seen.
    assert_eq!(
        FileTab::new("file-fe0", 0, &[0], &[]).int("M.state.slot"),
        2
    );
    assert_eq!(
        FileTab::new("file-fe1", 0, &[1], &[]).int("M.state.slot"),
        2
    );
    assert_eq!(
        FileTab::new("file-0f", 0, &[0x0F, 1], &[]).int("M.state.slot"),
        0
    );
    // Jill's list puts document 4 in book 0 slot 4, where Chris has 5.
    let mut jill = FileTab::new("file-jill", 1, &[4], &[]);
    assert_eq!(jill.int("M.state.slot"), 4);
    let chris = FileTab::new("file-chris", 0, &[4], &[]);
    assert_eq!(chris.int("M.state.slot"), 0xFF);
    jill.settle_list();
    jill.step(press(|i| i.right = true));
    assert_eq!(jill.int("M.state.book"), 1);
    assert_eq!(jill.int("M.state.slot"), 0xFF);
}

#[test]
fn file_selector_navigation_skips_uncollected_documents() {
    // Book 1: slots 0 (file 0x0F) and 3 (file 0x03) are collected.
    let mut tab = FileTab::new("file-nav", 0, &[0x0F, 0x03], &[]);
    assert_eq!(tab.int("M.state.book"), 0);
    assert_eq!(tab.int("M.state.slot"), 0);
    // The tab's first frame draws only the panel under it; the cover's entry
    // slide starts on the next tick at `-0x7f` with speed `0x80`, and only
    // the settled state reads the pad.
    assert!(tab.flag("M.state.list_pending"));
    assert_eq!(tab.screen.value("locked"), 1);
    tab.tick();
    assert_eq!(tab.int("M.state.slide_y"), -0x7f + 0x40);
    tab.settle_list();
    assert_eq!(tab.int("M.state.slide_y"), 0);

    tab.step(press(|i| i.down = true));
    assert_eq!(
        tab.int("M.state.slot"),
        3,
        "down jumps to the next collected row"
    );
    tab.step(press(|i| i.down = true));
    assert_eq!(tab.int("M.state.slot"), 3, "nowhere else to go");
    tab.step(press(|i| i.up = true));
    assert_eq!(tab.int("M.state.slot"), 0);

    tab.step(press(|i| i.right = true));
    assert_eq!(tab.int("M.state.book"), 1);
    assert_eq!(
        tab.int("M.state.slot"),
        0xFF,
        "book 2 has nothing collected"
    );
    assert_eq!(tab.int("M.state.slide_x"), 0x78);
    tab.step(press(|i| i.left = true));
    assert_eq!(tab.int("M.state.book"), 0);
    assert_eq!(tab.int("M.state.slot"), 0);
    assert_eq!(tab.int("M.state.slide_x"), -0x78);

    // Confirm on a listed document starts the fade hand-off; the reader opens
    // at page 0 when the ramp completes.
    assert!(matches!(
        tab.step(press(|i| i.confirm = true)),
        ScreenResult::Continue
    ));
    assert!(tab.flag("M.state.fade_in"));
    while tab.flag("M.state.fade_in") {
        tab.tick();
    }
    assert_eq!(tab.mode(), "reader");
    assert_eq!(tab.screen.value("reader"), 1);
    assert_eq!(tab.int("M.state.page"), 0);
}

#[test]
fn file_reader_pages_through_and_closes_on_the_last_page() {
    let mut tab = FileTab::new("file-pages", 0, &[0x0F], &[]);
    tab.settle_list();
    tab.step(press(|i| i.confirm = true));
    tab.settle_reader();
    assert_eq!(tab.mode(), "reader");

    // List slot 0 is document 0x0F, five half-pages long. Walk to the last
    // page; a further page-down only acknowledges it (FILE_SAVEFLAG).
    let count = tab.int("M.tables.page_count[0x0F]");
    assert_eq!(count, 5);
    for page in 1..count {
        tab.step(press(|i| i.right = true));
        tab.settle_reader();
        assert_eq!(tab.int("M.state.page"), page);
    }
    assert!(!tab.flag("M.state.reader_last_page"));
    tab.step(press(|i| i.right = true));
    tab.settle_reader();
    assert_eq!(tab.int("M.state.page"), count - 1, "the last page holds");
    assert!(
        tab.flag("M.state.reader_last_page"),
        "the last page is acknowledged"
    );
    // A page-up first withdraws the acknowledgement.
    tab.step(press(|i| i.left = true));
    assert!(!tab.flag("M.state.reader_last_page"));
    assert_eq!(tab.int("M.state.page"), count - 1);
    tab.step(press(|i| i.right = true));
    assert!(tab.flag("M.state.reader_last_page"));

    // Up/down and the paging buttons are not read by the reader.
    for input in [
        press(|i| i.down = true),
        press(|i| i.up = true),
        press(|i| i.page_right = true),
    ] {
        tab.step(input);
        tab.settle_reader();
        assert_eq!(tab.int("M.state.page"), count - 1);
    }

    tab.step(press(|i| i.confirm = true));
    tab.settle_reader();
    assert_eq!(tab.mode(), "list", "confirm exits on the last page");

    // Cancel from the reader returns to the list too, and a cancel from the
    // settled list closes the tab.
    tab.step(press(|i| i.confirm = true));
    tab.settle_reader();
    assert_eq!(tab.mode(), "reader");
    tab.step(press(|i| i.cancel = true));
    tab.settle_reader();
    assert_eq!(tab.mode(), "list");
    assert!(matches!(
        tab.step(press(|i| i.cancel = true)),
        ScreenResult::Done(_)
    ));
}

#[test]
fn file_pages_turn_with_the_character_bank_page_turn_cue() {
    // The reference's `jpn_file_list_input` plays `play_sfx(3, 8)` (the
    // character table's "Chris10") on each half-page turn and `play_sfx(3, 4)`
    // (the shared "cursor") when the last-page acknowledgement is raised or
    // withdrawn; the selector's moves blip and its confirm chimes.
    let mut tab = FileTab::new("file-cues", 0, &[0x0F, 0x03], &[]);
    tab.settle_list();
    assert!(tab.cues.is_empty(), "the entry slide is silent");

    // A move that lands blips; the ends of the list and the empty second book
    // play nothing.
    tab.step(press(|i| i.down = true));
    assert_eq!(tab.cues, [UiCue::Cursor]);
    tab.step(press(|i| i.down = true));
    assert!(tab.cues.is_empty(), "nowhere left to move");
    tab.step(press(|i| i.up = true));
    assert_eq!(tab.cues, [UiCue::Cursor]);
    tab.step(press(|i| i.right = true));
    assert_eq!(tab.cues, [UiCue::Cursor], "the book change blips");
    tab.step(press(|i| i.left = true));
    assert_eq!(tab.cues, [UiCue::Cursor]);

    // Confirm opens the reader with the decide chime; the fade and the
    // reader's entry ramp are silent.
    let count = tab.int("M.tables.page_count[0x0F]");
    assert_eq!(count, 5);
    tab.step(press(|i| i.confirm = true));
    assert_eq!(tab.cues, [UiCue::Decide]);
    tab.settle_reader();
    assert!(tab.cues.is_empty());

    // Page down turns the half-page with the page-turn sample. The reader's
    // busy lock drops the next press without a cue.
    tab.step(press(|i| i.right = true));
    assert_eq!(tab.cues, [UiCue::PageTurn]);
    tab.step(press(|i| i.right = true));
    assert!(tab.cues.is_empty(), "the busy lock swallows the input");
    tab.settle_reader();
    assert_eq!(tab.int("M.state.page"), 1);

    for _ in 1..count - 1 {
        tab.step(press(|i| i.right = true));
        assert_eq!(tab.cues, [UiCue::PageTurn]);
        tab.settle_reader();
    }
    assert_eq!(tab.int("M.state.page"), count - 1);

    // The last half-page is acknowledged and withdrawn with the cursor blip,
    // not the page-turn sample; an acknowledged last page holds silently.
    tab.step(press(|i| i.right = true));
    assert_eq!(tab.cues, [UiCue::Cursor]);
    assert!(tab.flag("M.state.reader_last_page"));
    tab.step(press(|i| i.right = true));
    assert!(tab.cues.is_empty());
    tab.step(press(|i| i.left = true));
    assert_eq!(tab.cues, [UiCue::Cursor]);
    assert!(!tab.flag("M.state.reader_last_page"));

    // A page up and the follow-up page down turn pages with the page-turn
    // sample again.
    tab.step(press(|i| i.left = true));
    assert_eq!(tab.cues, [UiCue::PageTurn]);
    tab.settle_reader();
    assert_eq!(tab.int("M.state.page"), count - 2);
    tab.step(press(|i| i.right = true));
    assert_eq!(tab.cues, [UiCue::PageTurn]);
    tab.settle_reader();
    tab.step(press(|i| i.right = true));
    assert_eq!(tab.cues, [UiCue::Cursor], "re-acknowledge the last page");

    // Confirm on the last page and the selector's cancel both play the cancel
    // cue; the exit slides and return to the list in silence.
    tab.step(press(|i| i.confirm = true));
    assert_eq!(tab.cues, [UiCue::Cancel]);
    tab.settle_reader();
    assert_eq!(tab.mode(), "list");
    assert!(matches!(
        tab.step(press(|i| i.cancel = true)),
        ScreenResult::Done(_)
    ));
    assert_eq!(tab.cues, [UiCue::Cancel]);
}

#[test]
fn file_reader_slides_the_page_on_and_off() {
    let mut tab = FileTab::new("file-slides", 0, &[0x0F], &[]);
    tab.settle_list();

    // Opening the reader fades the selector out first; the reader then spends
    // sixteen black frames ramping its backdrop in before the page even pages
    // in, and only then starts the 0x128 entry slide.
    tab.step(press(|i| i.confirm = true));
    while tab.flag("M.state.fade_in") {
        tab.tick();
    }
    assert_eq!(tab.mode(), "reader");
    assert_eq!(tab.state(), "backdrop");
    assert_eq!(tab.int("M.state.reader_slide"), 0);
    for _ in 0..15 {
        tab.tick();
        assert_eq!(tab.int("M.state.reader_slide"), 0, "the page is not up yet");
        assert!(!tab.flag("M.state.reader_page_loaded"));
    }
    // The sixteenth ramp frame completes the backdrop.
    tab.tick();
    assert_eq!(tab.state(), "page_load");
    assert_eq!(tab.int("M.state.reader_backdrop"), 0x80);
    assert!(
        !tab.flag("M.state.reader_page_loaded"),
        "the load frame draws black"
    );
    // The half-page's TIM pages in and falls into the slide's first step.
    tab.tick();
    assert_eq!(tab.state(), "slide_in");
    assert!(tab.flag("M.state.reader_page_loaded"));
    assert_eq!(
        tab.int("M.state.reader_slide"),
        146,
        "300 / 2 slides the first step"
    );
    let mut positions = vec![146];
    while tab.state() != "idle" {
        tab.tick();
        positions.push(tab.int("M.state.reader_slide"));
    }
    assert_eq!(positions.last(), Some(&0), "the page settles on the box");
    assert_eq!(positions.len(), 8, "state 2 plus the seven state-3 frames");

    // The busy lock releases one frame after the slide lands, and the input
    // is only read the frame after that.
    assert!(tab.flag("M.state.reader_busy"));
    tab.tick();
    assert!(!tab.flag("M.state.reader_busy"));
    assert_eq!(
        tab.screen.value("locked"),
        1,
        "the release frame still locks"
    );
    tab.tick();
    assert_eq!(tab.screen.value("locked"), 0);

    // A page-down first slides the current page off to the left, then eases
    // the next one in from the right; the page advances only once the old one
    // is clear.
    tab.step(press(|i| i.right = true));
    assert_eq!(tab.state(), "slide_out_left");
    assert_eq!(
        tab.int("M.state.reader_slide"),
        -1,
        "the input frame steps the slide"
    );
    let mut min_slide = 0;
    while tab.state() == "slide_out_left" {
        tab.tick();
        min_slide = min_slide.min(tab.int("M.state.reader_slide"));
        if tab.state() == "slide_out_left" {
            assert_eq!(
                tab.int("M.state.page"),
                0,
                "the old page holds through the slide-out"
            );
        }
    }
    assert!(min_slide < -0x80, "the old page left through the left edge");
    assert_eq!(tab.int("M.state.page"), 1);
    // One load frame, then the right-hand entry slide.
    tab.tick();
    assert_eq!(tab.state(), "page_load");
    tab.tick();
    assert_eq!(tab.state(), "slide_in");
    assert_eq!(tab.int("M.state.reader_slide"), 146);
    tab.settle_reader();

    // A page-up mirrors it: out to the right, then in from the left.
    tab.step(press(|i| i.left = true));
    assert_eq!(tab.state(), "slide_out_right");
    assert_eq!(tab.int("M.state.reader_slide"), 1);
    let mut max_slide = 0;
    while tab.state() == "slide_out_right" {
        tab.tick();
        max_slide = max_slide.max(tab.int("M.state.reader_slide"));
    }
    assert!(max_slide > 0x80, "the page left through the right edge");
    assert_eq!(tab.int("M.state.page"), 0);
    // The unpack frame (0xa) holds the +511 slide; the next frame is the
    // load, which falls into the left entry's first step.
    assert_eq!(tab.state(), "unpack_left");
    assert_eq!(tab.int("M.state.reader_slide"), 511);
    tab.tick();
    assert_eq!(tab.state(), "prev_page");
    assert_eq!(
        tab.int("M.state.reader_slide"),
        511,
        "the load frame holds the slide"
    );
    tab.tick();
    assert_eq!(tab.state(), "slide_in_left");
    assert_eq!(
        tab.int("M.state.reader_slide"),
        -146,
        "state 0xb halves the speed in"
    );
    tab.settle_reader();
    assert_eq!(tab.int("M.state.reader_slide"), 0);

    // Cancel draws the settled page on its own frame (the original notices
    // the cancel after the reader's advance), then slides out.
    tab.step(press(|i| i.cancel = true));
    assert_eq!(
        tab.int("M.state.reader_slide"),
        0,
        "the cancel frame still holds"
    );
    tab.tick();
    assert_eq!(tab.state(), "exit_slide");
    assert_eq!(tab.int("M.state.reader_slide"), -1);
    let mut exit_frames = 1;
    while tab.mode() != "list" {
        tab.tick();
        exit_frames += 1;
    }
    // One hold frame, one cover-load frame and the hand-off, then the
    // eight-frame fade back over the selector. The selector inherits the exit
    // slide's offset (its cover stays off-screen) until the ramp completes.
    assert_eq!(exit_frames, 9 + 16 + 3);
    assert_eq!(tab.int("M.state.fade"), 0xFF);
    assert_eq!(
        tab.int("M.state.slide_x"),
        -511,
        "the cover stays parked clear"
    );
    assert_eq!(tab.int("M.state.slide_y"), 0);
    while tab.flag("M.state.fade_out") {
        tab.tick();
    }
    assert_eq!(tab.int("M.state.fade"), 0);
    assert_eq!(
        tab.int("M.state.slide_x"),
        0,
        "the ramp completion resets the slide"
    );
}

#[test]
fn file_pickup_reader_exits_instead_of_returning_to_the_list() {
    // Item 0x5F is document 0; its list slot is the 0xFE marker, resolved to
    // 0 once the document is collected.
    let mut tab = FileTab::new(
        "file-pickup",
        0,
        &[0, 1],
        &[("pickup", ScreenArg::Int(0x5F))],
    );
    assert_eq!(tab.mode(), "reader");
    assert_eq!(tab.screen.value("reader"), 1);
    assert_eq!(tab.int("M.state.book"), 0);
    assert_eq!(
        tab.int("M.state.slot"),
        2,
        "the 0xFE list slot resolves to file 0"
    );
    assert!(matches!(tab.settle_reader(), ScreenResult::Continue));

    // Walking past document 0's last half-page ends the flow through the
    // exit slide.
    let count = tab.int("M.tables.page_count[0]");
    for _ in 1..count {
        tab.step(press(|i| i.right = true));
        assert_eq!(tab.cues, [UiCue::PageTurn]);
        tab.settle_reader();
    }
    assert_eq!(tab.int("M.state.page"), count - 1);
    tab.step(press(|i| i.right = true));
    tab.settle_reader();
    assert!(tab.flag("M.state.reader_last_page"));
    tab.step(press(|i| i.confirm = true));
    assert!(
        matches!(tab.settle_reader(), ScreenResult::Done(_)),
        "the pick-up reader exits once its slide-out finishes"
    );
    assert_eq!(
        tab.mode(),
        "reader",
        "a pick-up reader never falls back to the selector"
    );
}

#[test]
fn file_reader_blacks_out_the_screen_and_the_first_list_frame_draws_nothing() {
    let mut tab = FileTab::new("file-draw", 0, &[0x0F], &[]);
    // The tab's first frame leaves the panel underneath alone.
    let framebuffer = tab.draw();
    assert!(framebuffer.rgba.chunks(4).all(|px| px == [9, 9, 9, 255]));
    // The pick-up reader's own layer is an opaque black over everything.
    let mut tab = FileTab::new(
        "file-draw-reader",
        0,
        &[0],
        &[("pickup", ScreenArg::Int(0x5F))],
    );
    let framebuffer = tab.draw();
    assert!(framebuffer.rgba.chunks(4).all(|px| px == [0, 0, 0, 255]));
}

/// The Western build's page count per document.
const WEST_COUNTS: [usize; 16] = [4, 6, 11, 4, 2, 6, 7, 7, 10, 5, 5, 4, 3, 3, 3, 4];

/// The palette of the Western test pages: index 0 is the converter's black
/// key, index 15 the white text and every other index its own red.
fn west_palette(index: u16) -> u16 {
    match index {
        0 => 0,
        15 => 0x7FFF,
        index => index & 0x1F,
    }
}

/// An 8bpp `width` x `height` TIM over [`west_palette`].
fn sheet_tim(width: u16, height: u16, pixels: &[u8]) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&0x10u32.to_le_bytes());
    data.extend_from_slice(&9u32.to_le_bytes());
    data.extend_from_slice(&524u32.to_le_bytes());
    data.extend_from_slice(&[0, 0, 0, 0]);
    data.extend_from_slice(&256u16.to_le_bytes());
    data.extend_from_slice(&1u16.to_le_bytes());
    for index in 0..256u16 {
        data.extend_from_slice(&west_palette(index).to_le_bytes());
    }
    data.extend_from_slice(&(12u32 + pixels.len() as u32).to_le_bytes());
    data.extend_from_slice(&[0, 0, 0, 0]);
    data.extend_from_slice(&(width / 2).to_le_bytes());
    data.extend_from_slice(&height.to_le_bytes());
    data.extend_from_slice(pixels);
    data
}

/// A pack of Western document pages: under each `(language, documents)`,
/// every page of the listed documents (all sixteen for None). Page `n` is a
/// one-row sheet of palette index `n + 1` with a white texel at x 1. The
/// arrow sheet's 8-pixel columns are indices 20..=23, and `jpn` adds the
/// first Japanese page of those documents.
fn west_pack(languages: &[(&str, Option<&[usize]>)], jpn: &[usize]) -> &'static Pack {
    let mut writer = crate::pack::PackWriter::new();
    for &(language, documents) in languages {
        for (entry, count) in WEST_COUNTS.into_iter().enumerate() {
            if documents.is_some_and(|documents| !documents.contains(&entry)) {
                continue;
            }
            for page in 0..count {
                let mut pixels = vec![page as u8 + 1; 256];
                pixels[1] = 15;
                writer
                    .add(
                        &format!(
                            "item/{:02x}/file_fg_{page:02x}_{language}.tim",
                            0x5F + entry
                        ),
                        sheet_tim(256, 1, &pixels),
                    )
                    .unwrap();
            }
        }
    }
    let arrows: Vec<u8> = (0..32 * 8).map(|i| 20 + (i % 32) as u8 / 8).collect();
    writer
        .add("ui/file_arrows.tim", sheet_tim(32, 8, &arrows))
        .unwrap();
    for entry in jpn {
        writer
            .add(
                &format!("item/{:02x}/file_fg_00_ja-JP.tim", 0x5F + entry),
                map_tim(&[1; 256], west_palette),
            )
            .unwrap();
    }
    Box::leak(Box::new(
        Pack::from_bytes(writer.to_bytes().unwrap()).unwrap(),
    ))
}

/// Every document's Western pages under each of `languages`.
fn west_file_pack(languages: &[&str]) -> &'static Pack {
    let languages: Vec<_> = languages.iter().map(|&language| (language, None)).collect();
    west_pack(&languages, &[])
}

#[test]
fn file_reader_follows_the_session_language() {
    // The picked-up document `entry` (0x02 or 0x0F, both on Chris's list).
    let west = |pack, language, entry: u8| -> Option<String> {
        FileTab::with_pack(
            "file-language",
            0,
            &[entry],
            &[("pickup", ScreenArg::Int(0x5F + i64::from(entry)))],
            pack,
            language,
        )
        .screen
        .test_eval("return M.state.west")
    };
    let both = west_file_pack(&["en-US", "fr-FR"]);
    assert_eq!(west(both, Some("fr-FR"), 0x0F).as_deref(), Some("fr-FR"));
    assert_eq!(
        west(both, Some("de-DE"), 0x0F).as_deref(),
        Some("en-US"),
        "a language without pages reads the English ones"
    );
    assert_eq!(west(both, None, 0x0F).as_deref(), Some("en-US"));
    assert_eq!(
        west(both, Some("ja-JP"), 0x0F).as_deref(),
        Some("en-US"),
        "Japanese falls back to the English pages without its own"
    );
    let french = west_file_pack(&["fr-FR"]);
    assert_eq!(
        west(french, Some("en-US"), 0x0F).as_deref(),
        Some("fr-FR"),
        "a French-only tree serves an English session"
    );
    // A partial tree: the German pages of document 0x0F only.
    let partial = west_pack(&[("de-DE", Some(&[0x0F])), ("en-US", None)], &[0x02]);
    assert_eq!(west(partial, Some("de-DE"), 0x0F).as_deref(), Some("de-DE"));
    assert_eq!(
        west(partial, Some("de-DE"), 0x02).as_deref(),
        Some("en-US"),
        "each document falls back on its own"
    );
    assert_eq!(
        west(partial, Some("ja-JP"), 0x02),
        None,
        "a Japanese session reads its own pages"
    );
    assert_eq!(
        west(partial, Some("ja-JP"), 0x0F).as_deref(),
        Some("en-US"),
        "a Japanese document without its pages reads the English ones"
    );
    let japanese = west_pack(&[], &[0x0F]);
    assert_eq!(west(japanese, Some("en-US"), 0x0F), None);
    assert_eq!(
        west(empty_pack(), Some("en-US"), 0x0F),
        None,
        "no Western pages: the Japanese reader"
    );
}

#[test]
fn file_west_tables_match_the_reference() {
    let tab = FileTab::with_pack(
        "file-west-tables",
        0,
        &[],
        &[],
        west_file_pack(&["en-US"]),
        None,
    );
    let counts: Vec<i64> = tab.screen.test_eval(
        "local t = {} for i = 0, 15 do t[i + 1] = M.tables.west_page_count[i] end return t",
    );
    assert_eq!(counts, [4, 6, 11, 4, 2, 6, 7, 7, 10, 5, 5, 4, 3, 3, 3, 4]);
    let title: Vec<i64> = tab.screen.test_eval(
        "local t = {} for i = 0, 15 do t[i + 1] = M.tables.west_title_x[i] end return t",
    );
    assert_eq!(
        title,
        [8, 8, 24, 52, 32, 20, 64, 40, 16, 0, 16, 16, 32, 32, 32, 32]
    );
    assert_eq!(tab.int("M.tables.title_y.west"), 0xC1);
    assert_eq!(tab.int("M.tables.title_y.jpn"), 0xC0);
    assert_eq!(tab.int("M.tables.scroll_x.west"), 0x67);
    assert_eq!(tab.int("M.tables.scroll_x.jpn"), 0x6F);
    let stem: String = tab.screen.test_eval("return M.page_stem(15, 3, 'en-US')");
    assert_eq!(stem, "item/6e/file_fg_03_en-US");
}

#[test]
fn file_west_reader_draws_whole_pages_and_holds_one() {
    let mut tab = FileTab::with_pack(
        "file-west-pages",
        0,
        &[0x0F],
        &[],
        west_file_pack(&["en-US"]),
        Some("en-US"),
    );
    tab.settle_list();
    tab.step(press(|i| i.confirm = true));
    while tab.flag("M.state.fade_in") {
        tab.tick();
    }
    // States 0/1 ramp the FILEI backdrop in over sixteen frames, like the
    // Japanese reader.
    assert_eq!(tab.state(), "backdrop");
    for _ in 0..16 {
        assert_eq!(tab.state(), "backdrop");
        tab.tick();
    }
    assert_eq!(tab.state(), "page_load");
    tab.settle_reader();
    assert_eq!(tab.mode(), "reader");

    // Document 0x0F is four whole pages in the Western build.
    let count = tab.int("M.tables.west_page_count[0x0F]");
    assert_eq!(count, 4);
    let pixel = |framebuffer: &crate::render::Framebuffer, x: u32, y: u32| {
        let offset = ((y * framebuffer.width + x) * 4) as usize;
        [
            framebuffer.rgba[offset],
            framebuffer.rgba[offset + 1],
            framebuffer.rgba[offset + 2],
        ]
    };
    let red = |index: u32| ((index & 0x1F) * 255 / 31) as u8;
    for page in 0..count {
        if page > 0 {
            tab.step(press(|i| i.right = true));
            tab.settle_reader();
        }
        assert_eq!(tab.int("M.state.page"), page);
        let framebuffer = tab.draw();
        // The page's top-left texel sits at (0x18, 0x18), its white text
        // beside it.
        assert_eq!(
            pixel(&framebuffer, 0x18, 0x18),
            [red(page as u32 + 1), 0, 0],
            "page {page}"
        );
        assert_eq!(
            pixel(&framebuffer, 0x19, 0x18),
            [255, 255, 255],
            "page {page}"
        );
        // The next arrow always shows; the previous one from page 1 on.
        // Each blink frame is its own 8-pixel column of `ui/file_arrows`.
        let blink = tab.int("(M.state.blink & 0x30) == 0 and 1 or 0") as u32;
        assert_eq!(
            pixel(&framebuffer, 0x110, 0x74),
            [red(22 + blink), 0, 0],
            "page {page}"
        );
        let previous = pixel(&framebuffer, 0x18, 0x74);
        if page == 0 {
            assert!(previous != [red(20), 0, 0] && previous != [red(21), 0, 0]);
        } else {
            assert_eq!(previous, [red(20 + blink), 0, 0], "page {page}");
        }
        // Only the current page's art is held.
        let held: i64 = tab.screen.test_eval(
            "local n = 0 for stem, h in pairs(M.state.pages) do if stem:match('file_fg_.*_en%-US$') and h then n = n + 1 end end return n",
        );
        assert_eq!(held, 1, "page {page}");
    }
    tab.step(press(|i| i.right = true));
    assert!(tab.flag("M.state.reader_last_page"));
    // The arrows freeze lit on the acknowledged last page.
    let framebuffer = tab.draw();
    assert_eq!(pixel(&framebuffer, 0x110, 0x74), [red(23), 0, 0]);
    tab.step(press(|i| i.confirm = true));
    for _ in 0..32 {
        if tab.state() == "backdrop_out" {
            break;
        }
        tab.tick();
    }
    assert_eq!(tab.state(), "backdrop_out");
    // State 0xf ramps the backdrop out over sixteen frames, like entry.
    for _ in 0..16 {
        assert_eq!(tab.state(), "backdrop_out");
        tab.tick();
    }
    assert_ne!(tab.state(), "backdrop_out");
    tab.settle_reader();
    assert_eq!(tab.mode(), "list", "confirm exits on the last page");
    tab.draw();
    assert!(tab.flag("M.state.held == nil"), "the page is released");
}

/// `ui/item_view.lua` against a game state the test drives (the message
/// window flag and the examined bank).
struct ItemView {
    harness: Harness,
    game: crate::game::GameState,
    screen: LuaScreen,
}

impl ItemView {
    fn new(label: &str, item: u8, args: &[(&str, ScreenArg)]) -> Self {
        let harness = Harness::new(label);
        let game = crate::game::GameState::default();
        let mut all = vec![("item", ScreenArg::Int(i64::from(item)))];
        all.extend(args.iter().cloned());
        let screen = {
            let mut cx = UiContext {
                authentic: false,
                game: Some(&game),
                ..harness.context()
            };
            LuaScreen::open_screen("item_view", &all, ScreenAction::Quit, &mut cx).unwrap()
        };
        Self {
            harness,
            game,
            screen,
        }
    }

    fn step(&mut self, input: UiInput) -> ScreenResult {
        let cx = UiContext {
            authentic: false,
            game: Some(&self.game),
            ..self.harness.context()
        };
        self.screen.update(&cx, input)
    }

    /// One frame; the published event as `(event, id, mark, select)`.
    fn event(&mut self, input: UiInput) -> (i64, i64, i64, i64) {
        assert!(matches!(self.step(input), ScreenResult::Continue));
        (
            self.screen.value("event"),
            self.screen.value("event_id"),
            self.screen.value("event_mark"),
            self.screen.value("event_select"),
        )
    }

    fn int(&self, expr: &str) -> i64 {
        self.screen.test_eval(&format!("return {expr}"))
    }

    fn pose(&self) -> [i64; 3] {
        [
            self.int("M.state.pitch"),
            self.int("M.state.yaw"),
            self.int("M.state.roll"),
        ]
    }

    fn ready(&self) -> bool {
        self.screen.value("ready") != 0
    }

    /// Run with no input until the turntable is up; the frames it took.
    fn settle(&mut self) -> u32 {
        for frame in 1..=0x100 {
            self.event(neutral());
            if self.ready() {
                return frame;
            }
        }
        panic!("the intro never settled");
    }
}

fn held(left: bool, right: bool, up: bool, down: bool) -> UiInput {
    UiInput {
        held_left: left,
        held_right: right,
        held_up: up,
        held_down: down,
        ..UiInput::default()
    }
}

#[test]
fn item_view_setup_frame_and_intro_take_0x40_frames_and_ramp_the_lights() {
    let mut view = ItemView::new("view-intro", 0x01, &[]);
    // The setup runs on the first frame together with the first intro step,
    // so the turntable is up on frame 0x40.
    view.event(neutral());
    assert_eq!(view.int("M.state.light"), 0xFF - (0x3F << 2));
    assert_eq!(view.settle(), 0x3F);
    assert_eq!(view.int("M.state.light"), 0xFF);
    // The intro's yaw and roll wrap back to the resting pose.
    assert_eq!(view.pose(), [0, 0, 0]);
    assert_eq!(view.int("M.state.model_x"), 0x1980);
}

#[test]
fn item_view_left_and_up_spin_the_turntable_the_reference_way() {
    let mut view = ItemView::new("view-spin", 0x01, &[]);
    view.settle();
    let mut poses = Vec::new();
    for input in [
        held(true, false, false, false),
        held(false, true, false, false),
        held(false, false, true, false),
        held(false, false, false, true),
    ] {
        view.event(input);
        poses.push(view.pose());
    }
    assert_eq!(poses, [[0, 0x1F, 0], [0, 0, 0], [0, 0, 0x0FE1], [0, 0, 0]]);
}

#[test]
fn item_view_cancel_runs_the_exit_and_closes_after_0x40_frames() {
    let mut view = ItemView::new("view-cancel", 0x01, &[]);
    view.settle();
    view.event(UiInput {
        cancel: true,
        ..UiInput::default()
    });
    assert!(!view.ready());
    for frame in 1..0x40 {
        view.event(neutral());
        assert_eq!(view.int("M.state.light"), ((0x40 - frame) << 2) - 1);
    }
    assert!(!matches!(view.step(neutral()), ScreenResult::Continue));
}

#[test]
fn item_view_confirm_shows_the_description_and_waits_for_the_window() {
    let mut view = ItemView::new("view-confirm", 0x01, &[]);
    view.settle();
    assert_eq!(
        view.event(UiInput {
            confirm: true,
            ..UiInput::default()
        }),
        (1, 0, 0, 0)
    );
    // Cancel and the arrows are ignored while the description is up.
    view.game.message.active = true;
    view.event(UiInput {
        cancel: true,
        held_left: true,
        ..UiInput::default()
    });
    assert!(!view.ready());
    assert_eq!(view.pose()[1], 0);
    view.game.message.active = false;
    view.event(neutral());
    assert!(view.ready());
}

#[test]
fn item_view_loaded_weapons_name_their_rounds() {
    let mut view = ItemView::new("view-weapon", 0x04, &[("quantity", ScreenArg::Int(6))]);
    view.settle();
    assert_eq!(
        view.event(UiInput {
            confirm: true,
            ..UiInput::default()
        }),
        (
            2,
            i64::from(crate::examine::MESSAGE_WEAPON_LOADED),
            0,
            i64::from(crate::weapons::weapon_ammo_item_id(0x04)),
        )
    );
}

#[test]
fn item_view_red_book_zooms_then_marks_the_read_bit() {
    let mut view = ItemView::new("view-red-book", items::ITEM_RED_BOOK, &[]);
    view.settle();
    view.screen.test_eval::<()>("M.state.yaw = 0x400");
    let confirm = UiInput {
        confirm: true,
        ..UiInput::default()
    };
    assert_eq!(view.event(confirm), (0, 0, 0, 0), "the zoom starts");
    let mut frames = 0;
    let message = loop {
        frames += 1;
        match view.event(neutral()) {
            (0, ..) => {}
            event => break event,
        }
    };
    let class = crate::examine::class_message(items::ITEM_RED_BOOK).unwrap();
    assert_eq!(message, (2, i64::from(class), 0, 0));
    // The step ramps by four up to 0x20: 4+8+..+32 = 144 after eight
    // frames, then 0x20 a frame until object 1 passes 0x300.
    assert_eq!(frames, 8 + (0x300 - 144) / 0x20 + 1);
    view.game.message.active = true;
    view.event(neutral());
    view.game.message.active = false;
    assert_eq!(view.event(neutral()), (3, 0x0D, 0, 0));
    // The spin has run: a second confirm goes straight to the message and
    // marks the book examined.
    assert_eq!(
        view.event(confirm),
        (2, i64::from(class), i64::from(items::ITEM_RED_BOOK), 0)
    );
}

#[test]
fn item_view_doom_book_turns_into_its_medal() {
    let mut view = ItemView::new("view-doom-book", 0x3F, &[]);
    view.settle();
    view.screen.test_eval::<()>("M.state.yaw = 0x400");
    view.event(UiInput {
        confirm: true,
        ..UiInput::default()
    });
    while let (0, ..) = view.event(neutral()) {}
    // The message closes, then the book zooms back out for 0x40 frames.
    assert_eq!(view.event(neutral()), (0, 0, 0, 0));
    for frame in 1..=0x40 {
        let event = view.event(neutral());
        if frame < 0x40 {
            assert_eq!(event, (0, 0, 0, 0), "frame {frame}");
        } else {
            assert_eq!(event, (4, 0x24, 0, 0));
        }
    }
    assert_eq!(view.screen.value("item"), 0x24);
    assert_eq!(view.settle(), 0x40, "the medal plays the intro again");
}

#[test]
fn item_view_pickup_spins_in_and_closes_on_begin_exit() {
    let mut view = ItemView::new("view-pickup", 0x01, &[("pickup", ScreenArg::Bool(true))]);
    for frame in 1..=0x40 {
        view.event(UiInput {
            confirm: true,
            cancel: true,
            ..UiInput::default()
        });
        assert_eq!(view.screen.value("settled") != 0, frame == 0x40);
    }
    // Settled, it waits for the engine without reading the pad.
    view.event(UiInput {
        cancel: true,
        ..UiInput::default()
    });
    assert!(view.screen.value("settled") != 0);
    {
        let cx = UiContext {
            authentic: false,
            game: Some(&view.game),
            ..view.harness.context()
        };
        view.screen.signal("begin_exit", &cx);
    }
    for _ in 1..0x40 {
        view.event(neutral());
    }
    assert!(!matches!(view.step(neutral()), ScreenResult::Continue));
}

/// `ui/item_box.lua` against a game state the harness owns; a published swap
/// is applied the way the engine applies it.
struct ItemBoxScreen {
    harness: Harness,
    game: crate::game::GameState,
    screen: LuaScreen,
    /// The cues the last frame queued.
    cues: Vec<UiCue>,
}

impl ItemBoxScreen {
    fn new(label: &str, game: crate::game::GameState, cursor: u8) -> Self {
        let harness = Harness::new(label);
        let screen = {
            let mut cx = UiContext {
                authentic: false,
                game: Some(&game),
                ..harness.context()
            };
            LuaScreen::open_screen(
                "item_box",
                &[("cursor", ScreenArg::Int(i64::from(cursor)))],
                ScreenAction::Quit,
                &mut cx,
            )
            .unwrap()
        };
        Self {
            harness,
            game,
            screen,
            cues: Vec::new(),
        }
    }

    /// One frame; whether it swapped, panicking if it closed.
    fn step(&mut self, input: UiInput) -> bool {
        assert!(!self.closes(input), "the box closed");
        self.screen.value("swap") != 0
    }

    /// One frame; whether the box closed.
    fn closes(&mut self, input: UiInput) -> bool {
        let cx = UiContext {
            authentic: false,
            game: Some(&self.game),
            ..self.harness.context()
        };
        let result = self.screen.update(&cx, input);
        self.cues = cx.cues.borrow().clone();
        if self.screen.value("swap") != 0 {
            self.game.item_box_swap(
                self.screen.value("swap_box") as usize,
                self.screen.value("swap_slot") as usize,
            );
        }
        !matches!(result, ScreenResult::Continue)
    }

    fn int(&self, expr: &str) -> i64 {
        self.screen.test_eval(&format!("return {expr}"))
    }

    fn armed(&self) -> bool {
        self.int("M.state.mode == 'armed' and 1 or 0") != 0
    }

    fn box_cursor(&self) -> i64 {
        self.int("M.state.box_cursor")
    }

    fn dim(&self) -> bool {
        self.int("M.state.cursor_blink & 0x20") == 0
    }

    /// Run idle frames until the page slide ends.
    fn settle_slide(&mut self) {
        for _ in 0..0x20 {
            if self.int("M.state.slide_dir") == 0 {
                return;
            }
            self.step(neutral());
        }
        panic!("the slide never ended");
    }
}

fn box_game() -> crate::game::GameState {
    let mut game = crate::game::GameState::new(
        crate::state::RoomId::parse("1001").unwrap(),
        &crate::state::RoomState::default(),
    );
    game.max_health = 96;
    game
}

#[test]
fn item_box_confirm_arms_and_a_paged_confirm_swaps() {
    let mut game = box_game();
    game.add_item(0x41, 1); // spray in slot 0
    game.item_box[2] = InventoryItem {
        id: 0x0B,
        quantity: 12,
    };
    let mut item_box = ItemBoxScreen::new("box-swap", game, 8);
    assert!(!item_box.step(confirm()));
    assert!(item_box.armed());
    // Cancel disarms without moving anything or closing.
    assert!(!item_box.step(press(|input| input.cancel = true)));
    assert!(!item_box.armed());
    assert_eq!(item_box.game.inventory[0].id, 0x41);

    // Arm, page to the ammo slot and confirm: deposit the spray and take the
    // ammo.
    item_box.step(confirm());
    for _ in 0..2 {
        item_box.step(press(|input| input.page_right = true));
        item_box.settle_slide();
    }
    assert_eq!(item_box.box_cursor(), 2);
    assert!(item_box.step(confirm()));
    assert!(!item_box.armed());
    assert_eq!(item_box.game.item_box[2].id, 0x41);
    assert_eq!(item_box.game.inventory[0].id, 0x0B);
    assert_eq!(item_box.game.inventory[0].quantity, 12);
}

#[test]
fn item_box_drops_a_confirm_during_the_slide() {
    let mut game = box_game();
    game.add_item(0x41, 1);
    game.item_box[1] = InventoryItem {
        id: 0x0B,
        quantity: 12,
    };
    let mut item_box = ItemBoxScreen::new("box-slide", game, 8);
    item_box.step(confirm());
    // Page forward, then confirm with the slide still running: the
    // original's state 3 reads no buttons.
    item_box.step(press(|input| input.down = true));
    assert!(!item_box.step(confirm()));
    assert!(item_box.armed());
    assert_eq!(item_box.game.item_box[1].id, 0x0B);
    // The same confirm after the slide swaps the slot it landed on.
    item_box.settle_slide();
    assert_eq!(item_box.box_cursor(), 1);
    assert!(item_box.step(confirm()));
    assert_eq!(item_box.game.item_box[1].id, 0x41);
    assert_eq!(item_box.game.inventory[0].id, 0x0B);
}

#[test]
fn item_box_drops_a_confirm_on_the_slide_end_frame() {
    // `menu_itembox_interaction` state 3 hands back to state 2 on the step
    // that ends the slide and breaks without reading the pad, so a confirm on
    // exactly that frame is dropped: no swap and no decide cue.
    let mut game = box_game();
    game.add_item(0x41, 1);
    game.item_box[1] = InventoryItem {
        id: 0x0B,
        quantity: 15,
    };
    let mut item_box = ItemBoxScreen::new("box-slide-end", game, 8);
    item_box.step(confirm());
    item_box.step(press(|input| input.down = true));
    // Run until one step remains: the next frame ends the slide.
    while item_box.int("M.state.slide") != 0x0E {
        item_box.step(neutral());
    }
    assert!(!item_box.step(confirm()));
    assert_eq!(item_box.int("M.state.slide_dir"), 0, "the slide ended");
    assert_eq!(item_box.box_cursor(), 1);
    assert!(item_box.armed());
    assert!(
        item_box.cues.is_empty(),
        "the dropped confirm stayed silent"
    );
    assert_eq!(item_box.game.item_box[1].id, 0x0B);
    // The next frame reads the pad again.
    assert!(item_box.step(confirm()));
    assert_eq!(item_box.game.item_box[1].id, 0x41);
}

#[test]
fn item_box_start_closes_from_every_state_with_the_cancel_cue() {
    // The reference checks START (`pad_held_byte1() & 8`) before its state
    // switch: it plays `play_sfx(3, 5)` and returns 1 in browse, armed and
    // sliding states alike.
    let mut game = box_game();
    game.add_item(0x41, 1);
    let start = || press(|input| input.start = true);

    let mut item_box = ItemBoxScreen::new("box-start-browse", game.clone(), 8);
    assert!(item_box.closes(start()));
    assert_eq!(item_box.cues, [UiCue::Cancel]);

    let mut item_box = ItemBoxScreen::new("box-start-armed", game.clone(), 8);
    item_box.step(confirm());
    assert!(item_box.closes(start()));
    assert_eq!(item_box.cues, [UiCue::Cancel]);

    let mut item_box = ItemBoxScreen::new("box-start-slide", game.clone(), 8);
    item_box.step(confirm());
    item_box.step(press(|input| input.down = true));
    let slide = item_box.int("M.state.slide");
    assert!(item_box.closes(start()));
    assert_eq!(item_box.cues, [UiCue::Cancel]);
    assert_eq!(
        item_box.int("M.state.slide"),
        slide,
        "START returns before the slide steps"
    );

    // START wins over a confirm on the same frame.
    let mut item_box = ItemBoxScreen::new("box-start-confirm", game, 8);
    item_box.step(confirm());
    assert!(item_box.closes(press(|input| {
        input.start = true;
        input.confirm = true;
    })));
    assert_eq!(item_box.cues, [UiCue::Cancel]);
    assert!(item_box.screen.value("swap") == 0);
}

#[test]
fn item_box_page_plays_the_room_bank_slide_sound() {
    // State 2's page branch plays `play_sfx(2, 0x21)` once as it enters the
    // slide; the slide's own steps are silent.
    let mut game = box_game();
    game.add_item(0x41, 1);
    let mut item_box = ItemBoxScreen::new("box-page-sound", game, 8);
    item_box.step(confirm());
    for page in [
        press(|input| input.down = true),
        press(|input| input.up = true),
        press(|input| input.page_right = true),
        press(|input| input.page_left = true),
    ] {
        item_box.step(page);
        assert_eq!(item_box.cues, [UiCue::RoomSound(0x21)]);
        item_box.step(neutral());
        assert!(item_box.cues.is_empty(), "a slide step played a sound");
        item_box.settle_slide();
    }
    // Paging needs the armed state: a browse up/down is a cursor move.
    item_box.step(press(|input| input.cancel = true));
    item_box.step(press(|input| input.down = true));
    assert_eq!(item_box.cues, [UiCue::Cursor]);
}

/// Hold a paging key for `frames` frames after its press frame; the frames
/// (counted from the press, 0) that started a page slide, read from the
/// slide's room-bank sound.
fn item_box_hold_page(
    item_box: &mut ItemBoxScreen,
    press_frame: UiInput,
    held: UiInput,
    frames: usize,
) -> Vec<usize> {
    let mut starts = Vec::new();
    for frame in 0..=frames {
        item_box.step(if frame == 0 { press_frame } else { held });
        if item_box.cues.contains(&UiCue::RoomSound(0x21)) {
            starts.push(frame);
        }
    }
    starts
}

#[test]
fn item_box_held_down_keeps_paging_and_speeds_up() {
    // `menu_itembox_interaction` state 2: once the arm has paged
    // (`DAT_00ae9f28`), a held D-pad (`g_button_pressed_id & 0x5000`) starts
    // the next slide with no fresh press. `DAT_00ae9f26` counts the frames the
    // same key stays held from `0x0F`; a page that starts after it reaches 0
    // slides three steps a frame. The first two pages take 15 frames each,
    // then every page takes 5.
    let mut game = box_game();
    game.add_item(0x41, 1);
    let mut item_box = ItemBoxScreen::new("box-held-down", game, 8);
    item_box.step(confirm());
    let pressed = press(|input| {
        input.down = true;
        input.held_down = true;
    });
    let held = press(|input| input.held_down = true);
    let starts = item_box_hold_page(&mut item_box, pressed, held, 43);
    assert_eq!(starts, [0, 15, 30, 35, 40]);
    assert_eq!(item_box.box_cursor(), 4, "the fifth slide is running");
    assert_eq!(item_box.int("M.state.slide_dir"), 3);
    item_box.settle_slide();
    assert_eq!(item_box.box_cursor(), 5);
}

#[test]
fn item_box_fast_page_trace_steps_three_at_a_time() {
    let mut game = box_game();
    game.add_item(0x41, 1);
    let mut item_box = ItemBoxScreen::new("box-held-trace", game, 8);
    item_box.step(confirm());
    let pressed = press(|input| {
        input.down = true;
        input.held_down = true;
    });
    let held = press(|input| input.held_down = true);
    item_box_hold_page(&mut item_box, pressed, held, 29);
    assert_eq!(item_box.box_cursor(), 2);
    assert_eq!(item_box.int("M.state.slide_dir"), 0);
    let mut trace = Vec::new();
    for _ in 0..5 {
        item_box.step(held);
        trace.push(item_box.int("M.state.slide"));
    }
    assert_eq!(trace, [3, 6, 9, 12, 0]);
    assert_eq!(item_box.box_cursor(), 3);
}

#[test]
fn item_box_held_l1_pages_back_fast() {
    // L1 aliases up: backward pages move the box cursor at once and the fast
    // slide counts down 15, 12, 9, 6, 3, 0.
    let mut game = box_game();
    game.add_item(0x41, 1);
    let mut item_box = ItemBoxScreen::new("box-held-l1", game, 8);
    item_box.step(confirm());
    let pressed = press(|input| {
        input.page_left = true;
        input.held_page_left = true;
    });
    let held = press(|input| input.held_page_left = true);
    let starts = item_box_hold_page(&mut item_box, pressed, held, 29);
    assert_eq!(starts, [0, 15]);
    assert_eq!(item_box.box_cursor(), 46);
    item_box.step(held);
    assert_eq!(item_box.box_cursor(), 45);
    assert_eq!(item_box.int("M.state.slide_dir"), -3);
    assert_eq!(item_box.int("M.state.slide"), 12);
    item_box.settle_slide();
    assert_eq!(item_box.box_cursor(), 45);
}

#[test]
fn item_box_held_paging_needs_a_first_press_and_stops_on_release() {
    let mut game = box_game();
    game.add_item(0x41, 1);
    let mut item_box = ItemBoxScreen::new("box-held-gate", game, 8);
    item_box.step(confirm());
    let held = press(|input| input.held_down = true);
    // A key already held when the arm starts pages nothing until pressed.
    for _ in 0..20 {
        item_box.step(held);
    }
    assert_eq!(item_box.int("M.state.slide_dir"), 0);
    assert_eq!(item_box.box_cursor(), 0);
    // One tap pages once: released, the box stays put.
    item_box.step(press(|input| input.down = true));
    item_box.settle_slide();
    for _ in 0..20 {
        item_box.step(neutral());
    }
    assert_eq!(item_box.box_cursor(), 1);
    // Once this arm has paged, holding again pages without a press, at the
    // slow speed (the release restarted the repeat count).
    item_box.step(held);
    assert_eq!(item_box.cues, [UiCue::RoomSound(0x21)]);
    assert_eq!(item_box.int("M.state.slide_dir"), 1);
    item_box.settle_slide();
    assert_eq!(item_box.box_cursor(), 2);
    // A new arm needs a fresh press again.
    item_box.step(cancel());
    item_box.step(confirm());
    assert!(item_box.armed());
    for _ in 0..20 {
        item_box.step(held);
    }
    assert_eq!(item_box.box_cursor(), 2);
}

#[test]
fn item_box_confirm_beats_a_held_page() {
    // State 2 checks confirm before the page branch, so a confirm pressed
    // while a paging key is held swaps instead of paging.
    let mut game = box_game();
    game.add_item(0x41, 1);
    game.item_box[1] = InventoryItem {
        id: 0x0B,
        quantity: 12,
    };
    let mut item_box = ItemBoxScreen::new("box-held-confirm", game, 8);
    item_box.step(confirm());
    item_box.step(press(|input| {
        input.down = true;
        input.held_down = true;
    }));
    while item_box.int("M.state.slide_dir") != 0 {
        item_box.step(press(|input| input.held_down = true));
    }
    assert_eq!(item_box.box_cursor(), 1);
    assert!(item_box.step(press(|input| {
        input.confirm = true;
        input.held_down = true;
    })));
    assert_eq!(item_box.cues, [UiCue::Decide]);
    assert_eq!(item_box.box_cursor(), 1);
    assert_eq!(item_box.game.item_box[1].id, 0x41);
}

#[test]
fn item_box_armed_up_and_down_page_and_wrap_the_box_cursor() {
    let mut game = box_game();
    game.add_item(0x41, 1);
    let mut item_box = ItemBoxScreen::new("box-page", game, 8);
    item_box.step(confirm());
    for expected in [1, 2] {
        item_box.step(press(|input| input.down = true));
        item_box.settle_slide();
        assert_eq!(item_box.box_cursor(), expected);
    }
    assert_eq!(
        item_box.screen.value("cursor"),
        8,
        "the inventory cursor is fixed"
    );
    // Up moves the box cursor back at once and wraps past the first slot.
    item_box.step(press(|input| input.up = true));
    assert_eq!(item_box.box_cursor(), 1);
    item_box.settle_slide();
    for _ in 0..2 {
        item_box.step(press(|input| input.up = true));
        item_box.settle_slide();
    }
    assert_eq!(item_box.box_cursor(), 47, "past the first slot wraps");
    assert!(item_box.armed(), "paging keeps the arm");
    item_box.step(press(|input| input.down = true));
    item_box.settle_slide();
    assert_eq!(item_box.box_cursor(), 0, "past the last slot wraps");
}

#[test]
fn item_box_cursor_blinks_in_step_with_the_box_state() {
    let mut game = box_game();
    game.add_item(0x41, 1);
    let mut item_box = ItemBoxScreen::new("box-blink", game, 8);
    // The counter starts at 0; the first browse frame's count-down lands on
    // 0xFF, the bright frame.
    assert!(item_box.dim());
    item_box.step(neutral());
    assert!(!item_box.dim());
    // The arm resets the counter and the armed and sliding states leave it
    // alone.
    item_box.step(confirm());
    assert!(item_box.dim());
    item_box.step(press(|input| input.down = true));
    for _ in 0..0x40 {
        item_box.step(neutral());
    }
    assert!(item_box.dim());
    // Back in browse the count-down resumes; a D-pad move resets it.
    item_box.step(press(|input| input.cancel = true));
    item_box.step(neutral());
    assert!(!item_box.dim());
    item_box.step(press(|input| input.down = true));
    assert!(item_box.dim());
}

#[test]
fn item_box_swap_clears_the_equipped_marker_when_the_item_leaves() {
    let mut game = box_game();
    game.add_item(0x02, 15); // Beretta
    game.add_item(0x41, 1);
    game.set_equipped(Some(0x02));
    let mut item_box = ItemBoxScreen::new("box-equipped", game, 8);
    item_box.step(confirm());
    assert!(item_box.step(confirm()));
    assert_eq!(item_box.game.item_box[0].id, 0x02);
    assert_eq!(item_box.game.inventory[0].id, 0x41);
    assert_eq!(item_box.game.equipped, None);
}

#[test]
fn item_box_cancel_and_a_tab_confirm_close_the_box() {
    let mut game = box_game();
    game.add_item(0x41, 1);
    let mut item_box = ItemBoxScreen::new("box-cancel", game.clone(), 8);
    assert!(item_box.closes(press(|input| input.cancel = true)));

    // A cursor on the tab row closes on confirm, as the original's
    // `(cursor & 0xf8) == 0` check does.
    let mut item_box = ItemBoxScreen::new("box-tab", game, 6);
    assert!(item_box.closes(confirm()));
}

#[test]
fn item_box_an_empty_armed_slot_stays_armed() {
    let mut item_box = ItemBoxScreen::new("box-empty", box_game(), 8);
    item_box.step(confirm());
    assert!(!item_box.step(confirm()));
    assert!(item_box.armed());
    assert!(item_box.game.item_box.iter().all(|slot| slot.id == 0));
}

#[test]
fn item_box_walks_the_shared_cursor_and_swaps_under_it() {
    let mut game = box_game();
    game.add_item(0x41, 1); // slot 0
    game.add_item(0x44, 1); // slot 1
    game.add_item(0x02, 15); // slot 2
    let mut item_box = ItemBoxScreen::new("box-walk", game, 10);
    assert_eq!(
        item_box.screen.value("cursor"),
        10,
        "open adopts the cursor"
    );
    let mut cursors = Vec::new();
    for input in [
        press(|input| input.left = true),
        press(|input| input.down = true),
        press(|input| input.right = true),
        press(|input| input.up = true),
    ] {
        item_box.step(input);
        cursors.push(item_box.screen.value("cursor"));
    }
    assert_eq!(cursors, [8, 12, 14, 10]);
    item_box.step(confirm());
    assert!(item_box.step(confirm()));
    assert_eq!(
        item_box.game.item_box[0].id, 0x44,
        "the slot under the cursor moved"
    );
    assert!(!item_box.game.has_item(0x44));
    assert_eq!(
        item_box.screen.value("cursor"),
        10,
        "a swap does not move the cursor"
    );
}

#[test]
fn item_box_tab_row_only_reaches_exit() {
    let mut game = box_game();
    game.id.player_flag = 0;
    game.add_item(0x41, 1);
    // Six slots put cursor 16 on the first column's last row. Down wraps to
    // the original's tab-row cursor 4, which the box and the menu's tab
    // highlight both draw on EXIT (cursor | 2 = 6).
    let mut item_box = ItemBoxScreen::new("box-exit-tab", game, 16);
    item_box.step(press(|input| input.down = true));
    assert_eq!(item_box.int("M.state.player_cursor"), 4);
    assert_eq!(
        item_box.screen.value("cursor"),
        6,
        "the drawn cursor is EXIT"
    );
    assert_eq!(
        item_box.screen.value("on_slot"),
        0,
        "the menu draws the tab cursor"
    );
    // Left/right do nothing on the tab row.
    item_box.step(press(|input| input.left = true));
    assert_eq!(item_box.int("M.state.player_cursor"), 4);
    item_box.step(press(|input| input.right = true));
    assert_eq!(item_box.int("M.state.player_cursor"), 4);
    // Down returns to the first slot row's same column, never to Map/File.
    item_box.step(press(|input| input.down = true));
    assert_eq!(item_box.int("M.state.player_cursor"), 8);
    // Up from the first row lands on the tab row again; up from there wraps
    // to the last slot row of the same column.
    item_box.step(press(|input| input.up = true));
    assert_eq!(item_box.int("M.state.player_cursor"), 4);
    item_box.step(press(|input| input.up = true));
    assert_eq!(item_box.int("M.state.player_cursor"), 16);

    // Jill's eight slots wrap the same way (last row cursors 20/22).
    let mut game = box_game();
    game.id.player_flag = 1;
    game.add_item(0x41, 1);
    let mut item_box = ItemBoxScreen::new("box-exit-jill", game, 20);
    item_box.step(press(|input| input.down = true));
    assert_eq!(item_box.int("M.state.player_cursor"), 4);
    assert_eq!(item_box.screen.value("cursor"), 6);
    item_box.step(press(|input| input.up = true));
    assert_eq!(item_box.int("M.state.player_cursor"), 20);

    // The right column wraps to cursor 6 and back to the last row's right
    // column.
    let mut game = box_game();
    game.id.player_flag = 1;
    game.add_item(0x41, 1);
    let mut item_box = ItemBoxScreen::new("box-exit-jill-right", game, 22);
    item_box.step(press(|input| input.down = true));
    assert_eq!(item_box.int("M.state.player_cursor"), 6);
    assert_eq!(item_box.screen.value("cursor"), 6);
    item_box.step(press(|input| input.up = true));
    assert_eq!(item_box.int("M.state.player_cursor"), 22);
}

#[test]
fn item_box_tab_row_wrap_keeps_the_column() {
    let mut game = box_game();
    game.id.player_flag = 0;
    game.add_item(0x41, 1);
    // The right column's last row wraps to cursor 6, still drawn on EXIT.
    let mut item_box = ItemBoxScreen::new("box-exit-column", game, 18);
    item_box.step(press(|input| input.down = true));
    assert_eq!(item_box.int("M.state.player_cursor"), 6);
    assert_eq!(item_box.screen.value("cursor"), 6);
    // Up returns to the last row's right column, not the left one.
    item_box.step(press(|input| input.up = true));
    assert_eq!(item_box.int("M.state.player_cursor"), 18);
    // Left/right are ignored on the right column's tab cursor too, and down
    // returns to the first row's right column.
    item_box.step(press(|input| input.down = true));
    assert_eq!(item_box.int("M.state.player_cursor"), 6);
    item_box.step(press(|input| input.left = true));
    assert_eq!(item_box.int("M.state.player_cursor"), 6);
    item_box.step(press(|input| input.right = true));
    assert_eq!(item_box.int("M.state.player_cursor"), 6);
    item_box.step(press(|input| input.down = true));
    assert_eq!(item_box.int("M.state.player_cursor"), 10);

    // The wrapped-to tab closes the box on confirm, as the original's
    // `(cursor & 0xf8) == 0` check does.
    let mut game = box_game();
    game.id.player_flag = 0;
    game.add_item(0x41, 1);
    let mut item_box = ItemBoxScreen::new("box-exit-close", game, 16);
    item_box.step(press(|input| input.down = true));
    assert!(item_box.closes(confirm()));
}

#[test]
fn item_box_opening_on_a_disabled_tab_uses_exit() {
    // Map (0) and File (2) are not rows the box travels, so a caller that
    // hands the box one still opens on the EXIT pair, keeping the column:
    // 0 -> the left tab cursor (4), 2 -> the right one (6).
    for (cursor, row_cursor, last_row) in [(0, 4, 16), (2, 6, 18)] {
        let mut game = box_game();
        game.id.player_flag = 0;
        game.add_item(0x41, 1);
        let mut item_box = ItemBoxScreen::new("box-open-tab", game, cursor);
        assert_eq!(
            item_box.int("M.state.player_cursor"),
            row_cursor,
            "cursor {cursor} adopts the box's tab row"
        );
        assert_eq!(
            item_box.screen.value("cursor"),
            6,
            "the drawn cursor is EXIT"
        );
        // Up from the tab row wraps to the last slot row, never a negative
        // cursor.
        item_box.step(press(|input| input.up = true));
        assert_eq!(item_box.int("M.state.player_cursor"), last_row);
        assert!(!item_box.closes(neutral()));
    }
}

#[test]
fn item_box_without_art_draws_nothing() {
    let mut item_box = ItemBoxScreen::new("box-draw", box_game(), 8);
    let cx = UiContext {
        authentic: false,
        game: Some(&item_box.game),
        ..item_box.harness.context()
    };
    item_box.screen.signal_with("cursor_shown", true, &cx);
    let mut framebuffer = Framebuffer::new();
    item_box.screen.draw(&cx, &mut framebuffer);
    assert!(framebuffer.rgba == Framebuffer::new().rgba);
}

// ---- main menu -------------------------------------------------------------

const MENU_NONE: (i64, i64) = (0, 0);
const MENU_CLOSE: (i64, i64) = (1, 0);
const MENU_CLOSE_SILENT: (i64, i64) = (2, 0);

/// The pause menu over a lent game state, like the engine drives it.
struct Menu {
    harness: Harness,
    game: crate::game::GameState,
    screen: LuaScreen,
    /// The cues the last [`Menu::input`] queued.
    cues: Vec<UiCue>,
}

/// A session with a 96-health player and nothing held.
fn menu_game() -> crate::game::GameState {
    use crate::state::{RoomId, RoomState};
    let mut game =
        crate::game::GameState::new(RoomId::parse("1001").unwrap(), &RoomState::default());
    game.max_health = 96;
    game.entities[0].health = 96;
    game
}

impl Menu {
    fn open(mut game: crate::game::GameState) -> Self {
        let harness = Harness::new("main-menu");
        let screen = {
            let mut cx = harness.context();
            LuaScreen::open_screen_with_game(
                "main_menu",
                &[],
                ScreenAction::Quit,
                &mut cx,
                &mut game,
            )
            .unwrap()
        };
        Self {
            harness,
            game,
            screen,
            cues: Vec::new(),
        }
    }

    /// Open the menu again over the current game (the original re-runs its
    /// state 0 on every START).
    fn reopen(&mut self) {
        self.game.cancel_message();
        let mut cx = self.harness.context();
        self.screen = LuaScreen::open_screen_with_game(
            "main_menu",
            &[],
            ScreenAction::Quit,
            &mut cx,
            &mut self.game,
        )
        .unwrap();
    }

    fn event(&self) -> (i64, i64) {
        (self.screen.value("event"), self.screen.value("event_id"))
    }

    fn value(&self, key: &str) -> i64 {
        self.screen.value(key)
    }

    fn tick(&mut self) -> (i64, i64) {
        let cx = self.harness.context();
        self.screen
            .signal_with_game("tick", (), &cx, &mut self.game);
        self.event()
    }

    /// One frame's input, once the action submenu accepts it: the engine
    /// keeps ticking the menu while the box grows.
    fn input(&mut self, build: impl FnOnce(&mut UiInput)) -> (i64, i64) {
        for _ in 0..20 {
            if self.mode() != MODE_ACTIONS || self.value("ready") != 0 && self.value("closing") == 0
            {
                break;
            }
            self.tick();
        }
        let cx = self.harness.context();
        let result = self
            .screen
            .update_with_game(&cx, press(build), &mut self.game);
        assert_eq!(result, ScreenResult::Continue);
        self.cues = cx.cues.borrow().clone();
        self.event()
    }

    fn up(&mut self) -> (i64, i64) {
        self.input(|i| i.up = true)
    }

    fn down(&mut self) -> (i64, i64) {
        self.input(|i| i.down = true)
    }

    fn right(&mut self) -> (i64, i64) {
        self.input(|i| i.right = true)
    }

    fn confirm(&mut self) -> (i64, i64) {
        self.input(|i| i.confirm = true)
    }

    fn cancel(&mut self) -> (i64, i64) {
        self.input(|i| i.cancel = true)
    }

    /// Tick until the action submenu's close animation ends; the last
    /// event it published.
    fn finish_close(&mut self) -> (i64, i64) {
        let mut event = MENU_NONE;
        for _ in 0..20 {
            if self.value("closing") == 0 {
                return event;
            }
            let next = self.tick();
            if next != MENU_NONE {
                event = next;
            }
        }
        panic!("the submenu never closed");
    }

    /// Confirm the selected action, then run its one-frame hand-off and
    /// the submenu's close animation; the last event published.
    fn confirm_action(&mut self) -> (i64, i64) {
        self.confirm();
        let event = self.tick();
        let after = self.finish_close();
        if after != MENU_NONE { after } else { event }
    }

    fn mode(&self) -> i64 {
        self.value("mode")
    }

    fn eval<T: mlua::FromLuaMulti>(&self, body: &str) -> T {
        self.screen.test_eval(body)
    }

    /// Evaluate `body` with `api` bound to the menu's API over the lent game.
    fn eval_api<T: mlua::FromLuaMulti>(&mut self, body: &str) -> T {
        let cx = self.harness.context();
        self.screen
            .test_eval_with_game(&cx, Some(&mut self.game), body)
    }
}

const MODE_NAVIGATION: i64 = 0;
const MODE_ACTIONS: i64 = 1;
const MODE_MOVE: i64 = 2;

#[test]
fn main_menu_cursor_navigation_uses_the_layout_rules() {
    use crate::game::STATE_BYTE_SELECTED_ITEM;
    let mut game = menu_game();
    game.add_item(0x44, 1);
    game.add_item(0x43, 1);
    let mut menu = Menu::open(game);
    assert_eq!(menu.value("cursor"), 8);
    assert_eq!(menu.value("slot"), 0);
    assert_eq!(menu.value("selected_item"), 0x44);
    assert_eq!(menu.game.selected_item, Some(0x44));
    assert_eq!(
        menu.game.state_bytes[usize::from(STATE_BYTE_SELECTED_ITEM)],
        0x44
    );

    menu.right();
    assert_eq!(menu.value("cursor"), 10);
    assert_eq!(menu.value("selected_item"), 0x43);
    menu.up();
    assert_eq!(
        menu.value("cursor"),
        6,
        "up from the top slot row reaches the tabs"
    );
    assert_eq!(menu.value("slot"), -1);
    assert_eq!(menu.value("selected_item"), 0);
    assert_eq!(menu.game.selected_item, None);
    menu.up();
    assert_eq!(menu.value("cursor"), 2);
    menu.up();
    assert_eq!(
        menu.value("cursor"),
        22,
        "up from the top tab row wraps to the bottom"
    );

    // Chris carries six slots.
    let mut game = menu_game();
    game.id.player_flag = 0;
    assert_eq!(game.inventory_capacity(), 6);
    let mut chris = Menu::open(game);
    chris.down();
    chris.down();
    assert_eq!(chris.value("cursor"), 16);
    chris.down();
    assert_eq!(
        chris.value("cursor"),
        0,
        "past the sixth slot wraps to the tab row"
    );
}

#[test]
fn main_menu_submenu_transitions_and_check_return() {
    let mut game = menu_game();
    game.add_item(0x44, 1);
    let mut menu = Menu::open(game);

    // Confirm on the item opens the action submenu.
    assert_eq!(menu.confirm(), MENU_NONE);
    assert_eq!(menu.mode(), MODE_ACTIONS);
    assert!(!menu.eval::<bool>("return M.state.equippable"));
    // Down twice selects MOVE, then cancel walks back.
    menu.down();
    menu.down();
    assert_eq!(menu.eval::<i64>("return M.state.action"), 2);
    menu.cancel();
    assert_eq!(menu.value("closing"), 1, "cancel shrinks the submenu away");
    menu.finish_close();
    assert_eq!(menu.mode(), MODE_NAVIGATION);

    // CHECK slides the options out, then starts the viewer.
    menu.confirm();
    while menu.value("ready") == 0 {
        menu.tick();
    }
    menu.down();
    assert_eq!(menu.eval::<i64>("return M.state.action"), 1);
    menu.confirm();
    let mut frames = 0;
    let event = loop {
        frames += 1;
        assert!(frames < 20, "the slide never finished");
        let event = menu.tick();
        if event != MENU_NONE {
            break event;
        }
    };
    assert_eq!(event, (3, 0x44));
    assert_eq!(menu.value("viewing"), 1);
    // The viewer closing slides the options back in.
    let cx = menu.harness.context();
    menu.screen
        .signal_with_game("view_closed", (), &cx, &mut menu.game);
    while menu.value("viewing") != 0 {
        menu.tick();
    }
    assert_eq!(menu.mode(), MODE_ACTIONS);

    // An empty slot refuses to open the submenu, silently.
    let mut empty = Menu::open(menu_game());
    let cx = empty.harness.context();
    let result = empty
        .screen
        .update_with_game(&cx, confirm(), &mut empty.game);
    assert_eq!(result, ScreenResult::Continue);
    assert!(
        cx.cues.borrow().is_empty(),
        "an empty-slot confirm is silent"
    );
    assert_eq!(empty.event(), MENU_NONE);
    assert_eq!(empty.mode(), MODE_NAVIGATION);
}

#[test]
fn main_menu_tab_and_exit_confirmations() {
    let mut menu = Menu::open(menu_game());
    menu.eval::<()>("M.state.cursor = 4");
    assert_eq!(menu.confirm(), (4, 4));
    // The exit tab closes without the exit cue after its decide
    // (`menu_tab_exit`); the cancel's only cue is the one the engine's close
    // plays.
    menu.eval::<()>("M.state.cursor = 6");
    assert_eq!(menu.confirm(), MENU_CLOSE_SILENT);
    assert_eq!(menu.cues, [UiCue::Decide]);
    assert_eq!(menu.cancel(), MENU_CLOSE);
    assert!(menu.cues.is_empty(), "the cancel queued its own cue");
    // START cycles the tabs: the map, the file, then the exit without the
    // radio.
    assert_eq!(menu.input(|i| i.start = true), (4, 0));
    assert_eq!(menu.input(|i| i.start = true), (4, 2));
    assert_eq!(menu.input(|i| i.start = true), (4, 6));
}

#[test]
fn main_menu_a_herb_heal_updates_health_and_state() {
    use crate::game::{STATE_BYTE_SELECTED_ITEM, STATE_BYTE_USED_ITEM};
    let mut game = menu_game();
    game.entities[0].health = 20;
    game.add_item(0x44, 1);
    let mut menu = Menu::open(game);

    menu.confirm();
    // The action runs one frame after the confirm; check the flush's first
    // frame before the submenu's close animation advances it.
    menu.confirm();
    assert_eq!(menu.tick(), MENU_NONE);
    // A green herb restores exactly one third of the maximum.
    assert_eq!(menu.game.entities[0].health, 20 + 96 / 3);
    assert!(menu.game.inventory.is_empty(), "the herb was consumed");
    assert_eq!(menu.game.last_used_item, Some(0x44));
    assert_eq!(
        menu.game.state_bytes[usize::from(STATE_BYTE_USED_ITEM)],
        0x44
    );
    assert_eq!(
        menu.game.state_bytes[usize::from(STATE_BYTE_SELECTED_ITEM)],
        0
    );
    let flush: Vec<i64> = menu.eval("local f = M.state.bar.flush return {f[1], f[2], f[3]}");
    assert_eq!(flush, [0, 0xFF, 0], "the green flush");
    // The heal's draw is the same frame's monitor update, so the flush has
    // already advanced its first step.
    assert_eq!(menu.value("monitor"), 3);
    menu.finish_close();
    assert_eq!(menu.mode(), MODE_NAVIGATION);
}

#[test]
fn main_menu_heal_refusals_do_not_consume() {
    // Authentic mode refuses a green herb at full health, as the original does.
    let mut game = menu_game();
    game.player_options.authentic = true;
    game.add_item(0x44, 1);
    let mut menu = Menu::open(game);
    menu.confirm();
    menu.confirm_action();
    assert_eq!(menu.game.message.id, Some(0xF7 + 7), "the heal refusal");
    assert_eq!(menu.game.inventory.len(), 1, "the herb was not consumed");
    assert_eq!(menu.game.last_used_item, None);
}

#[test]
fn main_menu_full_health_herbs_are_used_up_outside_authentic_mode() {
    let mut game = menu_game();
    game.add_item(0x44, 1);
    let mut menu = Menu::open(game);
    menu.confirm();
    menu.confirm_action();
    assert_eq!(menu.game.message.id, None, "a used herb raises no message");
    assert!(menu.game.inventory.is_empty(), "the herb was used up");
    assert_eq!(menu.game.last_used_item, Some(0x44));
    assert_eq!(menu.game.entities[0].health, 96, "it heals nothing");
    let started: bool = menu.eval("return M.state.bar.flush ~= nil");
    assert!(!started, "a use with no effect starts no flush");
    menu.finish_close();
    assert_eq!(menu.mode(), MODE_NAVIGATION);
}

#[test]
fn the_menu_api_reads_whether_the_run_is_authentic() {
    let mut menu = Menu::open(menu_game());
    let authentic: bool = menu.eval_api("return api:authentic()");
    assert!(!authentic, "a default run is not authentic");

    let mut game = menu_game();
    game.player_options.authentic = true;
    let mut menu = Menu::open(game);
    let authentic: bool = menu.eval_api("return api:authentic()");
    assert!(authentic, "--authentic reaches the menu API");
}

#[test]
fn a_screen_with_no_session_reads_the_run_mode() {
    let harness = Harness::new("title-authentic");
    let mut screen = harness.open("title", &[]);
    let mut cx = harness.context();
    let default: bool = screen.test_eval_with_game(&cx, None, "return api:authentic()");
    assert!(!default, "a default run is not authentic");
    cx.authentic = true;
    let authentic: bool = screen.test_eval_with_game(&cx, None, "return api:authentic()");
    assert!(
        authentic,
        "an authentic run reaches a screen with no session"
    );
}

#[test]
fn main_menu_poison_cures_clear_the_status_bits() {
    let mut game = menu_game();
    game.set_health_status(0x22);
    game.add_item(0x45, 1); // blue herb cures poison 0x02
    let mut menu = Menu::open(game);
    menu.confirm();
    assert_eq!(menu.confirm_action(), MENU_NONE);
    assert_eq!(menu.game.health_status, 0x20);
    assert_eq!(
        menu.game.state_bytes[usize::from(crate::game::STATE_BYTE_HEALTH_STATUS)],
        0x20
    );

    // Serum cures the 0x20 poison and clears the scenario-2 marker.
    menu.game.set_health_status(0x22);
    menu.game
        .apply_flag(1, crate::game::SCENARIO2_FLAG_YAWN_POISONED, 0);
    menu.game.add_item(0x42, 1);
    menu.reopen();
    menu.confirm();
    assert_eq!(menu.confirm_action(), MENU_NONE);
    assert_eq!(menu.game.health_status, 0x02);
    assert!(
        !menu
            .game
            .flag_test(1, crate::game::SCENARIO2_FLAG_YAWN_POISONED, false)
    );
}

#[test]
fn main_menu_keys_need_their_use_flag_and_are_consumed() {
    let mut game = menu_game();
    game.add_item(0x33, 1); // sword key
    let mut menu = Menu::open(game);
    menu.confirm();
    assert_eq!(menu.confirm_action(), MENU_NONE);
    assert_eq!(menu.game.message.id, Some(0xF7 + 5), "the key refusal");
    assert_eq!(menu.game.inventory.len(), 1);

    menu.game.set_item_use_flag(0x33, true);
    menu.reopen();
    menu.confirm();
    assert_eq!(
        menu.confirm_action(),
        MENU_CLOSE,
        "a key use closes the inventory"
    );
    assert!(menu.game.inventory.is_empty());
    assert_eq!(menu.game.last_used_item, Some(0x33));
}

#[test]
fn main_menu_the_red_book_needs_its_own_flag() {
    let mut game = menu_game();
    game.add_item(0x3E, 1);
    let mut menu = Menu::open(game);
    menu.confirm();
    menu.confirm_action();
    assert_eq!(menu.game.message.id, Some(0xF7 + 6), "the book refusal");
    menu.game.set_item_use_flag(0x3E, true);
    menu.reopen();
    menu.confirm();
    assert_eq!(
        menu.confirm_action(),
        MENU_CLOSE,
        "a book use closes the inventory"
    );
    assert!(menu.game.inventory.is_empty());
}

#[test]
fn main_menu_equipping_a_weapon_toggles_the_state_byte() {
    use crate::game::STATE_BYTE_EQUIPPED;
    let mut game = menu_game();
    game.add_item(0x02, 15);
    let mut menu = Menu::open(game);
    menu.confirm();
    assert!(menu.eval::<bool>("return M.state.equippable"));
    menu.confirm();
    assert_eq!(menu.game.equipped, Some(0x02));
    assert_eq!(
        menu.game.state_bytes[usize::from(STATE_BYTE_EQUIPPED)],
        0x02
    );
    // The equip runs the submenu's close animation; a second confirm once
    // it is back on the inventory removes it again. The ammo is not
    // consumed by an equip.
    menu.finish_close();
    assert_eq!(menu.mode(), MODE_NAVIGATION);
    menu.confirm();
    menu.confirm();
    assert_eq!(menu.game.equipped, None);
    assert_eq!(menu.game.inventory[0].quantity, 15);
}

#[test]
fn main_menu_two_herbs_combine_through_the_move_action() {
    let mut game = menu_game();
    game.add_item(0x44, 1); // green
    game.add_item(0x43, 1); // red
    let mut menu = Menu::open(game);
    menu.confirm();
    menu.down();
    menu.down();
    assert_eq!(menu.confirm(), MENU_NONE);
    assert_eq!(menu.mode(), MODE_MOVE);
    menu.right();
    assert_eq!(menu.eval::<i64>("return M.state.move_cursor"), 10);
    menu.confirm();
    assert_eq!(menu.game.message.id, Some(0xF5), "the herb-mix prompt");
    assert_eq!(menu.game.inventory.len(), 2, "nothing is mixed before Yes");

    // The engine runs the message window before the menu tick. Drive the
    // prompt to YesNo and answer Yes, then let the menu tick apply the mix.
    menu.game.cancel_message();
    assert!(menu.game.message.start(0xF5, 0, &[0x08], true));
    let room = crate::state::RoomState::default();
    let text = crate::text::Text::default();
    for _ in 0..4 {
        menu.game
            .update_message(MessageInput::default(), &room, &text);
    }
    assert_eq!(menu.game.message.phase(), MessagePhase::YesNo);
    menu.game.update_message(
        MessageInput {
            action: true,
            ..MessageInput::default()
        },
        &room,
        &text,
    );
    assert!(!menu.game.message.active);
    // The menu task sees the answer one tick after the message task; the mix
    // then waits the four-tick MOVE-cursor retract before the box closes.
    menu.tick();
    assert_eq!(menu.game.inventory.len(), 2, "the answer is one tick late");
    menu.tick();
    assert_eq!(menu.game.inventory.len(), 1);
    assert_eq!(menu.game.inventory[0].id, 0x46, "green+red mixed herb");
    assert_eq!(menu.mode(), MODE_MOVE, "the prompt applies, then closes");
    for _ in 0..4 {
        menu.tick();
    }
    assert_eq!(menu.value("closing"), 1, "the box starts its close");
    menu.finish_close();
    assert_eq!(menu.mode(), MODE_NAVIGATION);
}

#[test]
fn main_menu_two_herbs_without_a_recipe_show_the_cannot_combine_message() {
    // Red + blue has no recipe, but both are herbs: the original's distinct
    // result 3, which shows `0xF6` rather than refusing silently.
    let mut game = menu_game();
    game.add_item(0x43, 1); // red herb
    game.add_item(0x45, 1); // blue herb
    let mut menu = Menu::open(game);
    menu.confirm();
    menu.down();
    menu.down();
    menu.confirm();
    assert_eq!(menu.mode(), MODE_MOVE);
    menu.right();
    menu.confirm();
    assert_eq!(menu.game.message.id, Some(0xF6));
    assert_eq!(menu.game.inventory.len(), 2, "nothing was consumed");
}

#[test]
fn main_menu_actions_are_visible_to_cmpb() {
    let mut game = menu_game();
    game.entities[0].health = 20;
    game.add_item(0x44, 1);
    game.add_item(0x0B, 30);
    let mut menu = Menu::open(game);
    assert!(menu.game.compare_byte(6, 0, 0x44), "selected item byte");
    assert!(menu.game.compare_byte(7, 0, 2), "two held slots");
    assert!(menu.game.compare_byte(18, 0, 0));
    menu.confirm();
    menu.confirm_action();
    assert!(menu.game.compare_byte(18, 0, 0x44), "used item byte");
    assert!(
        menu.game.compare_byte(7, 0, 1),
        "the green herb was consumed"
    );
    assert!(
        menu.game.compare_byte(6, 0, 0x0B),
        "cursor follows the pack"
    );
}

// ---- the health monitor ------------------------------------------------------

#[test]
fn health_monitor_states_follow_the_thirds_and_clamp() {
    let menu = Menu::open(menu_game());
    let state = |health: i64, max: i64| -> i64 {
        menu.eval(&format!(
            "return require('lib/health_bar').health_state({health}, {max})"
        ))
    };
    assert_eq!(state(140, 140), 3);
    assert_eq!(state(106, 140), 3);
    assert_eq!(state(105, 140), 2);
    assert_eq!(state(71, 140), 2);
    assert_eq!(state(70, 140), 1);
    assert_eq!(state(36, 140), 1);
    assert_eq!(state(35, 140), 0);
    assert_eq!(state(1, 140), 0);
    assert_eq!(state(0, 140), 0);
    // A maximum that is not a multiple of four cannot divide out to 4.
    assert_eq!(state(96, 96), 3);
    assert_eq!(state(150, 150), 3);
    assert_eq!(state(10, 2), 3);
    assert_eq!(state(10, 0), 0);
}

#[test]
fn health_monitor_a_heal_flush_runs_and_ends() {
    let mut game = menu_game();
    game.entities[0].health = 20;
    game.set_health_status(0x22);
    let mut menu = Menu::open(game);
    menu.tick();
    assert_eq!(menu.eval::<i64>("return M.state.bar.health_state"), 0);
    assert!(menu.eval::<bool>("return M.state.bar.poisoned"));
    menu.eval::<()>("require('lib/health_bar').start_flush(M.state.bar, true, true)");
    assert_eq!(menu.eval::<i64>("return M.state.bar.state"), 2);
    menu.game.entities[0].health = 96;
    menu.game.set_health_status(0);
    menu.tick();
    assert_eq!(menu.value("monitor"), 3);
    assert_eq!(menu.eval::<i64>("return M.state.bar.head"), 0xAF);
    for _ in 0..100 {
        if menu.value("monitor") != 3 {
            break;
        }
        menu.tick();
    }
    assert!(
        menu.eval::<bool>("return M.state.bar.flush == nil"),
        "the flush ended"
    );
}

#[test]
fn health_monitor_poison_selects_the_poisoned_waves() {
    let mut game = menu_game();
    game.max_health = 140;
    game.entities[0].health = 140;
    game.set_health_status(0x20);
    let mut menu = Menu::open(game);
    let mut poisoned_group = false;
    for _ in 0..200 {
        menu.tick();
        if menu.eval::<i64>("return M.state.bar.wave") >= 16 {
            poisoned_group = true;
            break;
        }
    }
    assert!(poisoned_group, "poison never selected the poisoned waves");
    assert_eq!(menu.eval::<i64>("return M.state.bar.health_state"), 3);
}

/// Draw the monitor through a recording stand-in for the drawing API and
/// return the queued `api:line` calls as `{x0, y0, x1, y1, r, g, b}`.
fn ekg_lines(menu: &Menu, setup: &str) -> Vec<Vec<i64>> {
    menu.eval(&format!(
        r#"
        local Bar = require('lib/health_bar')
        local calls = {{}}
        local api = {{
            line = function(_, x0, y0, x1, y1, r, g, b)
                calls[#calls + 1] = {{ x0, y0, x1, y1, r, g, b }}
            end,
            page = function() return nil end,
            page_sprite_mirrored = function() end,
        }}
        local bar = Bar.new()
        bar.state = 1
        bar.color = {{ 0xD8, 0xD8, 0x00 }}
        bar.secondary_color = {{ 0xD8, 0xD8, 0x00 }}
        {setup}
        Bar.draw(api, bar, 0, 0)
        return calls
        "#
    ))
}

#[test]
fn health_monitor_walks_the_wave_records() {
    let menu = Menu::open(menu_game());
    // The reference's walk from the head side (`head + 0x0F`) back to the
    // tail: a partial to the first record's right edge, one line per whole
    // record, then a partial to the tail. Wave 15 at head 0x67 rises from
    // (110, 163) to its apex (105, 148) and down to the tail. The colour is
    // the health colour through the line primitive's `/256 * 255` step, so
    // 0xD8 draws as 0xD7.
    let lines = ekg_lines(
        &menu,
        "bar.head = 0x67 bar.wave = 15 bar.secondary_head = 0x35 bar.secondary_wave = 15",
    );
    let expected: [[i64; 7]; 6] = [
        [118, 162, 116, 162, 0xD8, 0xD8, 0],
        [116, 162, 114, 158, 0xD8, 0xD8, 0],
        [114, 158, 111, 164, 0xD8, 0xD8, 0],
        [111, 164, 110, 163, 0xD8, 0xD8, 0],
        [110, 163, 105, 148, 0xD8, 0xD8, 0],
        [105, 148, 103, 156, 0xD8, 0xD8, 0],
    ];
    assert_eq!(lines, expected.map(|line| line.to_vec()));
}

#[test]
fn health_monitor_trail_dims_per_segment() {
    let menu = Menu::open(menu_game());
    // The trailing sweep queues each segment in the chain's near colour and
    // then steps it down by `colour >> 5` (6 for 0xD8) per pixel of that
    // segment's width, so the trail is brightest at its right edge. Wave 0's
    // flat tail at secondary head 0x54 gives one line per record and a
    // 0xD8 -> 0x89 chain.
    let lines = ekg_lines(
        &menu,
        "bar.head = 0x30 bar.secondary_head = 0x54 bar.secondary_wave = 0",
    );
    let expected: [[i64; 7]; 12] = [
        [115, 162, 112, 162, 0xD8, 0xD8, 0],
        [112, 162, 111, 161, 0xC6, 0xC6, 0],
        [111, 161, 110, 159, 0xC0, 0xC0, 0],
        [110, 159, 109, 162, 0xBA, 0xBA, 0],
        [109, 162, 108, 164, 0xB4, 0xB4, 0],
        [108, 164, 107, 163, 0xAE, 0xAE, 0],
        [107, 163, 106, 161, 0xA8, 0xA8, 0],
        [106, 161, 105, 158, 0xA2, 0xA2, 0],
        [105, 158, 104, 162, 0x9C, 0x9C, 0],
        [104, 162, 103, 164, 0x96, 0x96, 0],
        [103, 164, 102, 162, 0x90, 0x90, 0],
        [102, 162, 84, 162, 0x8A, 0x8A, 0],
    ];
    assert_eq!(lines, expected.map(|line| line.to_vec()));
}

#[test]
fn health_monitor_first_sweep_folds_against_the_open_reset() {
    // The menu open resets `DAT_00ae9f48` to 0xFF, so the first sweep's
    // variant draw folds a 3 (rather than a 0) away.
    let menu = Menu::open(menu_game());
    assert_eq!(
        menu.eval::<i64>("return require('lib/health_bar').new().wave_variant"),
        0xFF
    );
}

#[test]
fn health_monitor_face_flash_wraps_its_sweep_threshold() {
    let menu = Menu::open(menu_game());
    let face_tile = |head: i64| -> Option<i64> {
        menu.eval(&format!(
            r#"
            local Bar = require('lib/health_bar')
            local tile = nil
            local api = {{
                line = function() end,
                page = function() return nil end,
                page_sprite_mirrored = function(_, _, _, v) tile = v end,
            }}
            local bar = Bar.new()
            bar.state = 1
            bar.health_state = 0
            bar.head = {head}
            bar.secondary_head = 0x35
            Bar.draw(api, bar, 0, 0)
            return tile
            "#
        ))
    };
    // The threshold is the head's low byte minus 0x35 *in a byte*, so the last
    // four heads of a sweep keep matching the danger span's tail: 0x80/0x81
    // land on the second flash frame, 0x82/0x83 on its trailing blank.
    assert_eq!(face_tile(0x80), Some(0x40));
    assert_eq!(face_tile(0x81), Some(0x40));
    assert_eq!(face_tile(0x82), None);
    assert_eq!(face_tile(0x83), None);
    // A blank middle entry of the flash table, and a nonzero one.
    assert_eq!(face_tile(0x5A), None);
    assert_eq!(face_tile(0x64), Some(0x38));
}

#[test]
fn health_monitor_face_covers_its_whole_tile_for_hd_art() {
    // Classic REbirth's HD status page redraws the condition labels in a
    // different typeface, so the tile asks for its whole rect to be punched
    // rather than only the original's drawn texels (#158).
    let menu = Menu::open(menu_game());
    let whole: Vec<bool> = (0..=4)
        .map(|row: i64| {
            let (state, poisoned) = (row.min(3), row == 4);
            menu.eval(&format!(
                r#"
                local Bar = require('lib/health_bar')
                local whole = nil
                local api = {{
                    line = function() end,
                    page = function() return nil end,
                    page_sprite_mirrored = function(...)
                        whole = select(15, ...)
                    end,
                }}
                local bar = Bar.new()
                bar.state = 1
                bar.health_state = {state}
                bar.poisoned = {poisoned}
                bar.head = 0x64
                bar.secondary_head = 0x35
                Bar.draw(api, bar, 0, 0)
                return whole
                "#
            ))
        })
        .collect();
    assert_eq!(whole, [true; 5]);
}

#[test]
fn health_monitor_trail_starts_pre_faded_past_the_window() {
    let menu = Menu::open(menu_game());
    // A trail whose right edge (secondary head + 0x1F) is past the window is
    // clamped to 0x83 and its chain pre-faded by the overshoot: 0x8F is twelve
    // pixels out, so the 0xD8 chain starts at 0xD8 - 12 * 6 = 0x90.
    let lines = ekg_lines(
        &menu,
        "bar.head = 0x30 bar.secondary_head = 0x70 bar.secondary_wave = 0",
    );
    assert_eq!(lines, vec![vec![131, 162, 112, 162, 0x90, 0x90, 0]]);
}

#[test]
fn health_monitor_flush_uses_the_requested_channels() {
    let menu = Menu::open(menu_game());
    let flush = |setup: &str| -> Vec<Vec<i64>> {
        menu.eval(&format!(
            r#"
            local Bar = require('lib/health_bar')
            local calls = {{}}
            local api = {{
                line = function(_, x0, y0, x1, y1, r, g, b)
                    calls[#calls + 1] = {{ x0, y0, x1, y1, r, g, b }}
                end,
                page = function() return nil end,
                page_sprite_mirrored = function() end,
            }}
            local bar = Bar.new()
            {setup}
            Bar.draw(api, bar, 0, 0)
            return calls
            "#
        ))
    };
    // A cure-only flush is blue, and the first sweep frame draws the one row
    // the window has left at 0xAF.
    assert_eq!(
        flush("bar.state = 3 bar.head = 0xAF bar.flush = { 0, 0, 0xFF }"),
        vec![vec![84, 0xAF, 131, 0xAF, 0, 0, 0xFF]]
    );
    // A heal-and-cure flush is white.
    assert_eq!(
        flush("bar.state = 3 bar.head = 0xAF bar.flush = { 0xFF, 0xFF, 0xFF }"),
        vec![vec![84, 0xAF, 131, 0xAF, 0xFF, 0xFF, 0xFF]]
    );
    // Once the base has left the window the rows start at 0x92 and drop by
    // 0x10 per row, for 12 rows at head 0x8F.
    let rows = flush("bar.state = 3 bar.head = 0x8F bar.flush = { 0, 0xFF, 0 }");
    assert_eq!(rows.len(), 12);
    assert_eq!(rows[0], vec![84, 0x92, 131, 0x92, 0, 0xCF, 0]);
    assert_eq!(rows[11], vec![84, 0x9D, 131, 0x9D, 0, 0x1F, 0]);
}

#[test]
fn health_monitor_the_first_wrap_folds_the_draw_before_it() {
    let menu = Menu::open(menu_game());
    let wave = |variant: i64, roll: i64| -> i64 {
        menu.eval(&format!(
            r#"
            local Bar = require('lib/health_bar')
            local bar = Bar.new()
            bar.head = 0x84
            bar.wave_variant = {variant}
            local api = {{
                player_health = function() return 140, 140, 0 end,
                rand = function() return {roll} end,
            }}
            Bar.update(bar, api)
            return bar.wave
            "#
        ))
    };
    // The menu open leaves 0xFF in the fold byte, so a first draw of 3 folds
    // to variant 0; a 0 against a seeded 0 folds to 1.
    assert_eq!(wave(0xFF, 3), 12);
    assert_eq!(wave(0, 0), 13);
    assert_eq!(wave(0xFF, 0), 12);
}
