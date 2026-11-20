//! RE 2's gallery (`games/re2/ui/gallery.lua`) under the title, against the
//! original's trace and frames (`oracle/re2.md`, "The gallery").

use super::re2_title_tests::{
    GALLERY_WORDS, context, context_over, frame, gallery_words, open_gallery_with,
    pack_with_every_sketch, pack_without_gallery_art, pack_without_gallery_pictures, press,
    press_all, run, ti, words,
};
use super::*;

use crate::ui::{Screen, ScreenAction, ScreenResult, UiContext, UiCue, UiInput};

/// The trace's `gal`: the task's step, the screen, its step and row, the lit
/// row's level, the fade's level and the flags; nothing while no gallery
/// runs.
fn gal(screen: &LuaScreen) -> Option<[i64; 7]> {
    (screen.value("gal_on") != 0).then(|| {
        [
            "gal_task",
            "gal_screen",
            "gal_step",
            "gal_row",
            "gal_pulse",
            "gal_fade",
            "gal_flags",
        ]
        .map(|key| screen.value(key))
    })
}

/// The levels the last tick drew the menu's rows at, top to bottom; nothing
/// on a tick that drew no menu.
fn rows(screen: &LuaScreen) -> Option<Vec<i64>> {
    screen.test_eval("return M.state.gal and M.state.gal.words")
}

/// The fade's black over the last tick's menu, if any.
fn black(screen: &LuaScreen) -> Option<i64> {
    screen.test_eval("return M.state.gal and M.state.gal.black")
}

/// The trace's `galv`: the picture's index and the count of its textures.
fn galv(screen: &LuaScreen) -> [i64; 2] {
    ["gal_index", "gal_tiles"].map(|key| screen.value(key))
}

/// The colour the last tick's pictures screen is cleared to; nothing on a
/// tick that drew none.
fn cleared(screen: &LuaScreen) -> Option<i64> {
    screen.test_eval("local g = M.state.gal return g and g.view and g.view.clear")
}

/// Whether the last tick's pictures screen has its picture.
fn pictured(screen: &LuaScreen) -> Option<bool> {
    screen.test_eval("local g = M.state.gal return g and g.view and g.view.picture")
}

/// Where the last tick's pictures screen has a lit key, down the panel.
fn lit_key(screen: &LuaScreen) -> Option<i64> {
    screen.test_eval("local g = M.state.gal return g and g.view and g.view.key and g.view.key[3]")
}

/// The movie number the last tick's movies screen shows; nothing on a tick
/// that drew none.
fn listed(screen: &LuaScreen) -> Option<i64> {
    screen.test_eval("local g = M.state.gal return g and g.list and g.list.index")
}

/// What the last tick's movies screen has lit, if anything.
fn lit_word(screen: &LuaScreen) -> Option<String> {
    screen.test_eval("local g = M.state.gal return g and g.list and g.list.lit")
}

/// The title taken into the gallery as LEON or CLAIRE: the tick the title
/// hands off on, which is the gallery task's first.
fn start_gallery(cx: &UiContext<'_>, dir: &std::path::Path, claire: bool) -> LuaScreen {
    start_gallery_with(cx, dir, claire, None)
}

/// [`start_gallery`], with config.ini's `GALLERY0` to `GALLERY4` at `opened`.
fn start_gallery_with(
    cx: &UiContext<'_>,
    dir: &std::path::Path,
    claire: bool,
    opened: Option<[u32; 5]>,
) -> LuaScreen {
    let mut screen = open_gallery_with(cx, dir, opened);
    run(&mut screen, cx, UiInput::default(), 20);
    if claire {
        run(&mut screen, cx, press("right"), 5);
    }
    run(&mut screen, cx, press("confirm"), 18);
    assert_eq!(gal(&screen), Some([1, 0, 0, 0, 0, 0, 0]));
    screen
}

/// The gallery with its menu faded in and the cursor on the first row.
fn open_menu(cx: &UiContext<'_>, dir: &std::path::Path, claire: bool) -> LuaScreen {
    open_menu_with(cx, dir, claire, None)
}

fn open_menu_with(
    cx: &UiContext<'_>,
    dir: &std::path::Path,
    claire: bool,
    opened: Option<[u32; 5]>,
) -> LuaScreen {
    let mut screen = start_gallery_with(cx, dir, claire, opened);
    run(&mut screen, cx, UiInput::default(), 40);
    assert_eq!(screen.value("gal_fade"), 0);
    assert_eq!(screen.value("gal_flags") & 3, 0);
    screen
}

/// The ticks from a button on the menu's first row to the last of its fade:
/// LEON's starts at 0xFF and CLAIRE's at 0x80.
fn first_row_fade(claire: bool) -> usize {
    if claire { 16 } else { 32 }
}

/// The gallery's pictures, faded in on the first, with config.ini's
/// `GALLERY0` to `GALLERY4` at `opened`.
fn open_pictures(
    cx: &UiContext<'_>,
    dir: &std::path::Path,
    claire: bool,
    opened: Option<[u32; 5]>,
) -> LuaScreen {
    let mut screen = open_menu_with(cx, dir, claire, opened);
    run(&mut screen, cx, press("confirm"), first_row_fade(claire));
    // The screen is set, its art loads, and its fade from 0xFF is over on
    // the 33rd tick after.
    run(&mut screen, cx, UiInput::default(), 2 + 32);
    assert_eq!(screen.value("gal_screen"), 1);
    assert_eq!(screen.value("gal_step"), 1);
    assert_eq!(screen.value("gal_flags") & 3, 0);
    assert_eq!(pictured(&screen), Some(true));
    cx.cues.borrow_mut().clear();
    screen
}

fn test_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("arklay-re2-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn render(screen: &mut LuaScreen, cx: &UiContext<'_>) -> Framebuffer {
    let mut framebuffer = Framebuffer::new();
    screen.draw(cx, &mut framebuffer);
    framebuffer
}

fn pixel(framebuffer: &Framebuffer, x: usize, y: usize) -> [u8; 3] {
    let at = (y * framebuffer.width as usize + x) * 4;
    <[u8; 3]>::try_from(&framebuffer.rgba[at..at + 3]).unwrap()
}

#[test]
fn the_gallery_loads_on_its_second_tick_and_fades_in_over_sixteen() {
    let dir = test_dir("gallery-open");
    let cx = context(&dir);
    let mut screen = start_gallery(&cx, &dir, false);
    let asleep = ti(&screen);
    // The first tick is the title's hand-off, its frame held.
    assert_eq!(frame(&screen).as_deref(), Some("held"));
    assert_eq!(words(&screen), gallery_words(0, 0x20 + asleep[8] as u8));
    // The load: the fade is set to 0x80 and steps once, over a black frame.
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(gal(&screen), Some([1, 0, 1, 0, 0, 120, 1]));
    assert_eq!(rows(&screen), None);
    let shown = render(&mut screen, &cx);
    assert!(shown.rgba.chunks(4).all(|pixel| pixel[..3] == [0, 0, 0]));
    // The menu from the next tick, the lit row rising from nothing under
    // the fade's black at the level before its step.
    for (tick, level) in (0..15).map(|tick| (tick, 5 * tick)) {
        run(&mut screen, &cx, UiInput::default(), 1);
        assert_eq!(
            gal(&screen),
            Some([1, 0, 1, 0, level + 5, 112 - 8 * tick, 1]),
            "tick {tick}"
        );
        assert_eq!(rows(&screen), Some(vec![level, 0x40, 0x40]));
        assert_eq!(black(&screen), Some(120 - 8 * tick));
    }
    // The fade is over on the tick that finds it at 0.
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(gal(&screen), Some([1, 0, 1, 0, 80, 0, 0]));
    assert_eq!(black(&screen), None);
    // The title's task hasn't moved.
    assert_eq!(ti(&screen), asleep);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_lit_row_rises_and_falls_by_five() {
    let dir = test_dir("gallery-pulse");
    let cx = context(&dir);
    let mut screen = start_gallery(&cx, &dir, false);
    let mut levels = Vec::new();
    for _ in 0..60 {
        run(&mut screen, &cx, UiInput::default(), 1);
        levels.push((screen.value("gal_pulse"), screen.value("gal_flags") & 0x20));
    }
    // Up by 5 to 125, then 0x80 with the falling flag; down by 5 to 3,
    // then 0 with the flag cleared.
    assert_eq!(levels[0], (0, 0), "the load's tick");
    assert_eq!(levels[1..4], [(5, 0), (10, 0), (15, 0)]);
    assert_eq!(
        levels[25..29],
        [(125, 0), (128, 0x20), (123, 0x20), (118, 0x20)]
    );
    assert_eq!(levels[50..54], [(8, 0x20), (3, 0x20), (0, 0), (5, 0)]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn leons_rows_wrap_and_a_move_lights_the_row_it_leaves() {
    let dir = test_dir("gallery-leon");
    let cx = context(&dir);
    let mut screen = open_menu(&cx, &dir, false);
    for (key, row) in [
        ("down", 1),
        ("down", 2),
        ("down", 0),
        ("up", 2),
        ("up", 1),
        ("up", 0),
    ] {
        let from = screen.value("gal_row");
        cx.cues.borrow_mut().clear();
        run(&mut screen, &cx, press(key), 1);
        assert_eq!(*cx.cues.borrow(), [UiCue::Core(4)], "{key}");
        assert_eq!(screen.value("gal_row"), row, "{key}");
        // The move's tick still draws the row it left, at full.
        let lit: Vec<i64> = (0..3)
            .map(|k| if k == from { 0x80 } else { 0x40 })
            .collect();
        assert_eq!(rows(&screen), Some(lit), "{key}");
        // The level is put back to 0x80 before its step.
        assert!([123, 128].contains(&screen.value("gal_pulse")), "{key}");
        run(&mut screen, &cx, UiInput::default(), 3);
        let shown = rows(&screen).unwrap();
        assert_eq!(
            shown.iter().position(|level| *level != 0x40),
            Some(row as usize)
        );
    }
    // Left and right aren't the gallery's keys.
    cx.cues.borrow_mut().clear();
    run(&mut screen, &cx, press("left"), 3);
    run(&mut screen, &cx, press("right"), 3);
    assert!(cx.cues.borrow().is_empty());
    assert_eq!(screen.value("gal_row"), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn claires_rows_have_3d_model_above_exit() {
    let dir = test_dir("gallery-claire");
    let cx = context(&dir);
    let mut screen = open_menu(&cx, &dir, true);
    // The rows as drawn: ILLUSTRATION (0), MOVIE (1), 3D MODEL (5), EXIT (2).
    let drawn = [0, 1, 5, 2];
    for (key, row) in [
        ("down", 1),
        ("down", 5),
        ("down", 2),
        ("down", 0),
        ("up", 2),
        ("up", 5),
        ("up", 1),
        ("up", 0),
    ] {
        cx.cues.borrow_mut().clear();
        run(&mut screen, &cx, press(key), 4);
        assert_eq!(*cx.cues.borrow(), [UiCue::Core(4)], "{key}");
        assert_eq!(screen.value("gal_row"), row, "{key}");
        let shown = rows(&screen).unwrap();
        assert_eq!(shown.len(), 4);
        let lit = shown.iter().position(|level| *level != 0x40).unwrap();
        assert_eq!(drawn[lit], row, "{key}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn keys_together_act_in_the_originals_order() {
    let dir = test_dir("gallery-keys");
    let cx = context(&dir);
    // Up, then down, each from the row the tick began on.
    let mut screen = open_menu(&cx, &dir, false);
    cx.cues.borrow_mut().clear();
    run(&mut screen, &cx, press_all(&["up", "down"]), 1);
    assert_eq!(*cx.cues.borrow(), [UiCue::Core(4), UiCue::Core(4)]);
    assert_eq!(screen.value("gal_row"), 1);
    // A button after them: it is the first row's that is taken.
    let mut screen = open_menu(&cx, &dir, false);
    cx.cues.borrow_mut().clear();
    run(&mut screen, &cx, press_all(&["down", "confirm"]), 1);
    assert_eq!(*cx.cues.borrow(), [UiCue::Core(4), UiCue::Core(6)]);
    assert_eq!(screen.value("gal_row"), 3);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn exit_ends_the_gallery_and_the_title_wakes_two_ticks_on() {
    let dir = test_dir("gallery-exit");
    let cx = context(&dir);
    for (claire, button) in [(false, "confirm"), (true, "cancel"), (false, "start")] {
        let mut screen = open_menu(&cx, &dir, claire);
        let asleep = ti(&screen);
        run(&mut screen, &cx, press("up"), 4);
        assert_eq!(screen.value("gal_row"), 2);
        let level = screen.value("gal_pulse");
        cx.cues.borrow_mut().clear();
        run(&mut screen, &cx, press(button), 1);
        assert_eq!(*cx.cues.borrow(), [UiCue::Core(6)], "{button}");
        assert_eq!(screen.value("gal_task"), 2, "{button}");
        // The tick still draws the menu: LEON's with EXIT lit, CLAIRE's
        // with every row black.
        let shown = rows(&screen).unwrap();
        if claire {
            assert_eq!(shown, [0, 0, 0, 0]);
        } else {
            assert_eq!(shown, [0x40, 0x40, level]);
        }
        // The tick after closes the task: the title's picture alone, its
        // task still asleep.
        run(&mut screen, &cx, UiInput::default(), 1);
        assert_eq!(gal(&screen), None);
        assert_eq!(rows(&screen), None);
        assert_eq!(screen.test_eval::<i64>("return #M.state.sprites"), 0);
        assert_eq!(ti(&screen), asleep);
        let closed = render(&mut screen, &cx);
        assert_eq!(pixel(&closed, 0, 0), [0xFF; 3], "the title's picture");
        // Then the page runs again where it was left.
        run(&mut screen, &cx, UiInput::default(), 1);
        let awake = ti(&screen);
        assert_eq!((awake[4], awake[5], awake[7]), (7, 2, 0));
        assert_eq!(awake[10], asleep[10] + 1);
        let lit = usize::from(claire);
        assert_eq!(words(&screen), gallery_words(lit, 0x20 + awake[8] as u8));
        assert_eq!(screen.test_eval::<bool>("return M.state.claire"), claire);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_button_on_a_row_fades_the_menu_out_to_the_rows_screen() {
    let dir = test_dir("gallery-row");
    let cx = context(&dir);
    // The character, the way to the row, the row it turns to, the level its
    // fade starts at, the ticks to the one that finds it done and the
    // screen that one sets.
    for (claire, way, turned, fade, ticks, opens) in [
        (false, &[][..], 3, 0xFF, 32, 1),
        (false, &["down"][..], 4, 0x80, 16, 2),
        (true, &[][..], 3, 0x80, 16, 1),
        (true, &["down"][..], 6, 0x80, 16, 2),
        (true, &["down", "down"][..], 7, 0x80, 16, 3),
    ] {
        let mut screen = open_menu(&cx, &dir, claire);
        for key in way {
            run(&mut screen, &cx, press(key), 4);
        }
        cx.cues.borrow_mut().clear();
        run(&mut screen, &cx, press("confirm"), 1);
        let name = format!("claire {claire} row {turned}");
        assert_eq!(*cx.cues.borrow(), [UiCue::Core(6)], "{name}");
        assert_eq!(screen.value("gal_row"), turned, "{name}");
        assert_eq!(screen.value("gal_flags") & 3, 3, "{name}");
        // The black goes out from 0xFF less the level, which steps by 8.
        assert_eq!(screen.value("gal_fade"), fade - 8, "{name}");
        assert_eq!(black(&screen), Some(0xFF - fade), "{name}");
        // The rows are black while it fades, and the keys do nothing.
        cx.cues.borrow_mut().clear();
        run(&mut screen, &cx, press_all(&["down", "confirm"]), 1);
        assert!(cx.cues.borrow().is_empty(), "{name}");
        assert_eq!(screen.value("gal_row"), turned, "{name}");
        assert!(rows(&screen).unwrap().iter().all(|level| *level == 0));
        assert_eq!(black(&screen), Some(0xFF - fade + 8), "{name}");
        // Each tick of it: the black 8 deeper, the rows black, to the last.
        for tick in 2..ticks {
            run(&mut screen, &cx, UiInput::default(), 1);
            assert_eq!(screen.value("gal_row"), turned, "{name} tick {tick}");
            assert_eq!(
                black(&screen),
                Some(0xFF - fade + 8 * tick),
                "{name} tick {tick}"
            );
            assert_eq!(
                screen.value("gal_fade"),
                fade - 8 * (tick + 1),
                "{name} tick {tick}"
            );
            assert!(rows(&screen).unwrap().iter().all(|level| *level == 0));
        }
        assert!(screen.value("gal_fade") < 1, "{name}");
        // The tick that finds the fade done draws nothing, and sets the
        // row's screen: the pictures, the movies or the models.
        run(&mut screen, &cx, UiInput::default(), 1);
        let state = gal(&screen).unwrap();
        assert_eq!(state[..4], [1, opens, 0, 0], "{name}");
        assert_eq!(rows(&screen), None, "{name}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A sheet level `v` as the original shows it, at `level` of 0x80.
fn shown(v: u32, level: u32) -> u8 {
    (((8 * v + 7) * level + 64) >> 7) as u8
}

#[test]
fn the_menu_draws_at_half_size_in_the_originals_colours() {
    let dir = test_dir("gallery-draw");
    let cx = context(&dir);
    let mut screen = open_menu(&cx, &dir, false);
    run(&mut screen, &cx, press("down"), 1);
    // The move's tick: ROUGH SKETCH at full, MOVIE and EXIT dim.
    assert_eq!(rows(&screen), Some(vec![0x80, 0x40, 0x40]));
    let frame = render(&mut screen, &cx);
    // The picture's six pieces, each sheet's red at 8v + 7 and the fifth's
    // right half (green) under its left.
    for (x, y, colour) in [
        (0, 0, [shown(1, 0x80), 0, 0]),
        (127, 127, [shown(1, 0x80), 0, 0]),
        (128, 0, [shown(2, 0x80), 0, 0]),
        (255, 127, [shown(2, 0x80), 0, 0]),
        (0, 128, [shown(3, 0x80), 0, 0]),
        (127, 239, [shown(3, 0x80), 0, 0]),
        (128, 128, [shown(4, 0x80), 0, 0]),
        (255, 239, [shown(4, 0x80), 0, 0]),
        (256, 0, [shown(5, 0x80), 0, 0]),
        (319, 127, [shown(5, 0x80), 0, 0]),
        (256, 128, [0, shown(5, 0x80), 0]),
        (319, 239, [0, shown(5, 0x80), 0]),
    ] {
        assert_eq!(pixel(&frame, x, y), colour, "{x}, {y}");
    }
    // The rows where the original has them at 640 x 480, halved: each word
    // white at its level over the picture.
    let [sketch, movie, .., exit] = GALLERY_WORDS;
    for (word, x, y, level) in [
        (sketch, 198, 342, 0x80),
        (movie, 270, 386, 0x40),
        (exit, 280, 430, 0x40),
    ] {
        let lit = shown(31, level);
        let (x, y) = (x / 2, y / 2);
        let (w, h) = ((word[2] / 2) as usize, (word[3] / 2) as usize);
        assert_eq!(pixel(&frame, x, y), [lit; 3], "{word:?}");
        assert_eq!(pixel(&frame, x + w - 1, y + h - 1), [lit; 3], "{word:?}");
        // Outside it the picture shows.
        assert_eq!(pixel(&frame, x - 1, y)[1..], [0, 0], "{word:?}");
        assert_eq!(pixel(&frame, x, y - 1)[1..], [0, 0], "{word:?}");
    }
    // A word with an odd side ends on half a pixel: MOVIE's last column
    // and EXIT's last row are the word and the picture, averaged.
    let half = |word: u8, picture: u8| (u32::from(word) + u32::from(picture)).div_ceil(2) as u8;
    let dim = shown(31, 0x40);
    assert_eq!(
        pixel(&frame, 135 + 51, 193),
        [half(dim, shown(4, 0x80)), half(dim, 0), half(dim, 0)]
    );
    assert_eq!(pixel(&frame, 135 + 52, 193), [shown(4, 0x80), 0, 0]);
    assert_eq!(
        pixel(&frame, 140, 215 + 10),
        [half(dim, shown(4, 0x80)), half(dim, 0), half(dim, 0)]
    );
    // The fade's black over all of it, rounded: a button on MOVIE starts
    // it at 0x7F.
    run(&mut screen, &cx, press("confirm"), 1);
    assert_eq!(black(&screen), Some(0x7F));
    let faded = render(&mut screen, &cx);
    let under = |level: u8| ((u32::from(level) * 0x80 + 127) / 255) as u8;
    assert_eq!(pixel(&faded, 0, 0), [under(shown(1, 0x80)), 0, 0]);
    // Each of the original's pixels fades before the frame halves them: a
    // half-covered pixel is the mean of the faded word and faded picture.
    // MOVIE, the button's row, is at the lit level.
    let movie = under(shown(31, rows(&screen).unwrap()[1] as u32));
    assert_eq!(
        pixel(&faded, 135 + 51, 193),
        [
            half(movie, under(shown(4, 0x80))),
            half(movie, 0),
            half(movie, 0)
        ]
    );
    // The button's own tick still draws its rows; they are black from the
    // next.
    assert_eq!(pixel(&faded, 99, 171), [under(dim); 3]);
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(pixel(&render(&mut screen, &cx), 99, 171), [0, 0, 0]);
    // CLAIRE's four rows at their own places, ILLUSTRATION lit.
    let mut screen = open_menu(&cx, &dir, true);
    run(&mut screen, &cx, press("down"), 1);
    assert_eq!(rows(&screen), Some(vec![0x80, 0x40, 0x40, 0x40]));
    let frame = render(&mut screen, &cx);
    let [_, movie, illustration, model, exit] = GALLERY_WORDS;
    for (word, x, y, level) in [
        (illustration, 208, 326, 0x80),
        (movie, 270, 362, 0x40),
        (model, 242, 398, 0x40),
        (exit, 280, 434, 0x40),
    ] {
        let lit = shown(31, level);
        let (x, y) = (x / 2, y / 2);
        let (w, h) = ((word[2] / 2) as usize, (word[3] / 2) as usize);
        assert_eq!(pixel(&frame, x, y), [lit; 3], "{word:?}");
        assert_eq!(pixel(&frame, x + w - 1, y + h - 1), [lit; 3], "{word:?}");
        assert_eq!(pixel(&frame, x - 1, y)[1..], [0, 0], "{word:?}");
        assert_eq!(pixel(&frame, x, y - 1)[1..], [0, 0], "{word:?}");
        assert_eq!(pixel(&frame, x, y + h + 1)[1..], [0, 0], "{word:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_pack_short_of_the_gallerys_art_closes_the_gallery_as_it_opens() {
    let dir = test_dir("gallery-bare");
    let cx = context_over(pack_without_gallery_art(), &dir);
    let mut screen = start_gallery(&cx, &dir, false);
    let asleep = ti(&screen);
    // The load's tick ends the task, with no cue and nothing drawn.
    cx.cues.borrow_mut().clear();
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(gal(&screen), Some([2, 0, 0, 0, 0, 0, 0]));
    assert_eq!(rows(&screen), None);
    assert!(cx.cues.borrow().is_empty());
    // It closes on the next, and the title's page is back on the one after.
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(gal(&screen), None);
    assert_eq!(ti(&screen), asleep);
    run(&mut screen, &cx, press("down"), 1);
    assert_eq!(*cx.cues.borrow(), [UiCue::Core(4)]);
    assert_eq!(screen.value("ti_cur"), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_first_rows_button_opens_the_pictures_which_fade_in_from_black() {
    let dir = test_dir("gallery-pictures");
    let cx = context(&dir);
    // The first picture's textures: LEON's is 300 x 200, CLAIRE's 20 x 10.
    for (claire, tiles) in [(false, 2), (true, 1)] {
        let mut screen = open_menu(&cx, &dir, claire);
        run(&mut screen, &cx, press("confirm"), first_row_fade(claire));
        // The tick that finds the menu's fade done sets the screen.
        run(&mut screen, &cx, UiInput::default(), 1);
        let pulse = screen.value("gal_pulse");
        assert_eq!(gal(&screen).unwrap()[..4], [1, 1, 0, 0]);
        // The panel's art loads: the fade is set to 0xFF and steps once,
        // the first picture waits (the row), and nothing is drawn.
        run(&mut screen, &cx, UiInput::default(), 1);
        assert_eq!(gal(&screen), Some([1, 1, 1, 1, pulse, 247, 1]));
        assert_eq!(galv(&screen), [0, 0]);
        assert_eq!(cleared(&screen), None);
        let shown = render(&mut screen, &cx);
        assert!(shown.rgba.chunks(4).all(|pixel| pixel[..3] == [0, 0, 0]));
        // The picture loads on the next, which shows neither it nor the
        // white behind it: the frame is still cleared to black.
        run(&mut screen, &cx, UiInput::default(), 1);
        assert_eq!(gal(&screen), Some([1, 1, 1, 0, pulse, 239, 1]));
        assert_eq!(galv(&screen), [0, tiles]);
        assert_eq!(cleared(&screen), Some(0));
        assert_eq!(pictured(&screen), Some(false));
        assert_eq!(black(&screen), Some(247));
        // Then the picture over white, the black 8 lighter a tick.
        for tick in 2..32 {
            run(&mut screen, &cx, UiInput::default(), 1);
            assert_eq!(cleared(&screen), Some(0xFF), "tick {tick}");
            assert_eq!(pictured(&screen), Some(true), "tick {tick}");
            assert_eq!(black(&screen), Some(255 - 8 * tick), "tick {tick}");
            assert_eq!(screen.value("gal_flags"), 1, "tick {tick}");
        }
        // The fade is over on the tick that finds it under 1. The menu's
        // lit level has stood still.
        run(&mut screen, &cx, UiInput::default(), 1);
        assert_eq!(gal(&screen), Some([1, 1, 1, 0, pulse, -1, 0]));
        assert_eq!(black(&screen), None);
        assert_eq!(lit_key(&screen), None);
        assert!(cx.cues.borrow().iter().all(|cue| *cue != UiCue::Core(10)));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn left_and_right_turn_the_pictures_round_their_ends() {
    let dir = test_dir("gallery-turn");
    let cx = context(&dir);
    let mut screen = open_pictures(&cx, &dir, false, None);
    // A turn loads the picture on its own tick, which shows none; the key
    // is lit while it is held.
    run(&mut screen, &cx, press("right"), 1);
    assert_eq!(*cx.cues.borrow(), [UiCue::Core(10)]);
    assert_eq!(gal(&screen).unwrap()[..4], [1, 1, 1, 0]);
    assert_eq!(galv(&screen), [1, 1]);
    assert_eq!(pictured(&screen), Some(false));
    assert_eq!(cleared(&screen), Some(0xFF));
    assert_eq!(lit_key(&screen), Some(285));
    let held = UiInput {
        held_right: true,
        ..UiInput::default()
    };
    for _ in 0..3 {
        screen.update(&cx, held);
        assert_eq!(galv(&screen), [1, 1], "a held key turns once");
        assert_eq!(pictured(&screen), Some(true));
        assert_eq!(lit_key(&screen), Some(285));
    }
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(lit_key(&screen), None);
    // Back, and round the start to the last, whose 258 x 258 is cut in
    // four; then round the end to the first.
    cx.cues.borrow_mut().clear();
    for (key, lit, picture) in [
        ("left", 321, [0, 2]),
        ("left", 321, [59, 4]),
        ("right", 285, [0, 2]),
    ] {
        run(&mut screen, &cx, press(key), 1);
        assert_eq!(galv(&screen), picture, "{key}");
        assert_eq!(lit_key(&screen), Some(lit), "{key}");
        run(&mut screen, &cx, UiInput::default(), 1);
    }
    assert_eq!(*cx.cues.borrow(), [UiCue::Core(10); 3]);
    // A picture whose file the pack lacks shows as none.
    run(&mut screen, &cx, press("right"), 2);
    run(&mut screen, &cx, press("right"), 2);
    assert_eq!(galv(&screen), [2, 0]);
    assert_eq!(pictured(&screen), Some(false));
    assert_eq!(cleared(&screen), Some(0xFF));
    // Right is asked before left, and up before either.
    run(&mut screen, &cx, press_all(&["left", "right"]), 2);
    assert_eq!(galv(&screen), [3, 1]);
    // The buttons do nothing here.
    cx.cues.borrow_mut().clear();
    for button in ["confirm", "cancel", "start"] {
        run(&mut screen, &cx, press(button), 2);
    }
    assert!(cx.cues.borrow().is_empty());
    assert_eq!(gal(&screen).unwrap()[..4], [1, 1, 1, 0]);
    assert_eq!(galv(&screen), [3, 1]);
    run(&mut screen, &cx, press_all(&["up", "right"]), 1);
    assert_eq!(*cx.cues.borrow(), [UiCue::Core(6)]);
    assert_eq!(screen.value("gal_step"), 2);
    assert_eq!(galv(&screen), [3, 1]);
    // The lit key is the first held of right, left, up and down.
    assert_eq!(lit_key(&screen), Some(285));
    let _ = std::fs::remove_dir_all(&dir);
}

/// The pictures are more than a screen's art may hold at once: each is let
/// go as the next loads, so every one of them shows, on any lap.
#[test]
fn every_picture_shows_however_many_were_turned_through() {
    let dir = test_dir("gallery-every-picture");
    let cx = context_over(pack_with_every_sketch(), &dir);
    let mut screen = open_pictures(&cx, &dir, false, Some([!0; 5]));
    for turn in 1..=130 {
        let key = if turn <= 70 { "right" } else { "left" };
        run(&mut screen, &cx, press(key), 2);
        let [index, tiles] = galv(&screen);
        assert_eq!(
            index,
            if turn <= 70 {
                turn % 60
            } else {
                (140 - turn) % 60
            }
        );
        assert_eq!(
            pictured(&screen),
            Some(true),
            "turn {turn}, picture {index}"
        );
        assert_eq!(
            tiles,
            match index {
                0 => 2,
                59 => 4,
                _ => 1,
            },
            "turn {turn}, picture {index}"
        );
    }
    // The eleventh, 20 x 10 at 177, 58 of 640 x 480, is still drawn after
    // the others were let go.
    assert_eq!(galv(&screen), [10, 1]);
    let framebuffer = render(&mut screen, &cx);
    assert_eq!(pixel(&framebuffer, 92, 31), [0, shown(10, 0x80), 0]);
    assert_eq!(pixel(&framebuffer, 92, 36), [0xFF; 3]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_frame_is_cleared_to_what_the_tick_before_asked_for() {
    let dir = test_dir("gallery-clear");
    let cx = context(&dir);
    let mut screen = open_menu(&cx, &dir, false);
    run(&mut screen, &cx, press("confirm"), first_row_fade(false));
    // The screen, its art, and the first picture's load, on a frame still
    // cleared to black.
    run(&mut screen, &cx, UiInput::default(), 3);
    assert_eq!(cleared(&screen), Some(0));
    assert_eq!(pictured(&screen), Some(false));
    // A turn on the next tick loads again: white, as the first load's tick
    // asked, and no picture yet.
    run(&mut screen, &cx, press("right"), 1);
    assert_eq!(galv(&screen), [1, 1]);
    assert_eq!(cleared(&screen), Some(0xFF));
    assert_eq!(pictured(&screen), Some(false));
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(cleared(&screen), Some(0xFF));
    assert_eq!(pictured(&screen), Some(true));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_turn_skips_the_pictures_play_hasnt_opened() {
    let dir = test_dir("gallery-opened");
    let cx = context(&dir);
    let turns = |claire: bool, opened: [u32; 5], keys: &[(&str, i64)]| {
        let mut screen = open_pictures(&cx, &dir, claire, Some(opened));
        for (n, (key, index)) in keys.iter().enumerate() {
            run(&mut screen, &cx, press(key), 2);
            assert_eq!(
                screen.value("gal_index"),
                *index,
                "claire {claire} {opened:x?} turn {n}"
            );
        }
        assert_eq!(cx.cues.borrow().len(), keys.len());
        screen
    };
    // LEON's sketches have a bit each from the first word's top: the
    // first, the fourth and the sixtieth.
    turns(
        false,
        [0x9000_0000, 0x10, 0, 0, 0],
        &[
            ("right", 3),
            ("right", 59),
            ("right", 0),
            ("left", 59),
            ("left", 3),
            ("left", 0),
        ],
    );
    // CLAIRE's are in the third word, where her first five share the top
    // bit: those and the last.
    turns(
        true,
        [0, 0, 0x8000_0002, 0, 0],
        &[
            ("right", 1),
            ("right", 2),
            ("right", 3),
            ("right", 4),
            ("right", 34),
            ("right", 0),
            ("left", 34),
            ("left", 4),
        ],
    );
    // Her sixth alone: the first shows as the screen opens, opened or not,
    // and isn't come back to.
    turns(
        true,
        [0, 0, 0x4000_0000, 0, 0],
        &[("right", 5), ("right", 5), ("left", 5)],
    );
    // The other character's words aren't hers.
    turns(true, [!0, !0, 0, !0, !0], &[("right", 0), ("left", 0)]);
    // With nothing opened a turn comes back to the picture it left, which
    // loads again.
    let mut screen = turns(false, [0; 5], &[("right", 0)]);
    run(&mut screen, &cx, press("left"), 1);
    assert_eq!(galv(&screen), [0, 2]);
    assert_eq!(pictured(&screen), Some(false));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn up_and_down_fade_the_pictures_out_to_the_menu_and_the_movies() {
    let dir = test_dir("gallery-leave");
    let cx = context(&dir);
    for (key, step, lit) in [("up", 2, 357), ("down", 4, 393)] {
        let mut screen = open_pictures(&cx, &dir, false, None);
        run(&mut screen, &cx, press("right"), 2);
        let pulse = screen.value("gal_pulse");
        cx.cues.borrow_mut().clear();
        run(&mut screen, &cx, press(key), 1);
        assert_eq!(*cx.cues.borrow(), [UiCue::Core(6)], "{key}");
        assert_eq!(gal(&screen), Some([1, 1, step, 0, pulse, 247, 3]), "{key}");
        assert_eq!(black(&screen), Some(0), "{key}");
        assert_eq!(lit_key(&screen), Some(lit), "{key}");
        // The keys do nothing while it fades.
        run(&mut screen, &cx, press_all(&["right", "down", "up"]), 1);
        assert_eq!(cx.cues.borrow().len(), 1, "{key}");
        assert_eq!(galv(&screen), [1, 1], "{key}");
        assert_eq!(screen.value("gal_step"), step, "{key}");
        assert_eq!(black(&screen), Some(8), "{key}");
        // The black 8 deeper a tick, over white until the tick after the
        // one that finds the level under 0x20, then over black.
        for tick in 2..32 {
            run(&mut screen, &cx, UiInput::default(), 1);
            assert_eq!(black(&screen), Some(8 * tick), "{key} tick {tick}");
            let white = 255 - 8 * (tick - 1) >= 0x20;
            assert_eq!(
                cleared(&screen),
                Some(if white { 0xFF } else { 0 }),
                "{key} tick {tick}"
            );
            assert_eq!(pictured(&screen), Some(true), "{key} tick {tick}");
        }
        assert_eq!(screen.value("gal_fade"), -1, "{key}");
        // The tick that finds it done draws nothing and sets the screen:
        // the menu from up, the movies from down.
        run(&mut screen, &cx, UiInput::default(), 1);
        let to = if key == "up" { 0 } else { 2 };
        assert_eq!(gal(&screen), Some([1, to, 0, 0, pulse, -1, 3]), "{key}");
        assert_eq!(galv(&screen), [1, 0], "{key}");
        assert_eq!(cleared(&screen), None, "{key}");
        assert_eq!(rows(&screen), None, "{key}");
        run(&mut screen, &cx, UiInput::default(), 1);
        if key == "down" {
            // The movies load, from their first.
            assert_eq!(gal(&screen), Some([1, 2, 1, 0, pulse, 247, 1]));
            assert_eq!(galv(&screen), [0, 0]);
            continue;
        }
        assert_eq!(gal(&screen), Some([1, 0, 1, 0, pulse, 120, 1]), "{key}");
        run(&mut screen, &cx, UiInput::default(), 20);
        assert_eq!(rows(&screen).map(|rows| rows.len()), Some(3), "{key}");
        // The pictures start over on the first.
        run(&mut screen, &cx, press("confirm"), first_row_fade(false));
        run(&mut screen, &cx, UiInput::default(), 3);
        assert_eq!(galv(&screen), [0, 2], "{key}");
        assert_eq!(cleared(&screen), Some(0), "{key}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A colour of the pictures' screen at half weight over `under`.
fn halved(over: [u8; 3], under: [u8; 3]) -> [u8; 3] {
    std::array::from_fn(|k| {
        ((u32::from(over[k]) * 128 + u32::from(under[k]) * 127 + 127) / 255) as u8
    })
}

#[test]
fn the_pictures_draw_at_half_size_in_the_originals_colours() {
    let dir = test_dir("gallery-view");
    let cx = context(&dir);
    let white = [0xFF; 3];
    // The lower panel's rows are two blues in turn, and a pixel their mean.
    let panel = [shown(10, 0x80), 0, 0];
    let (even_row, odd_row) = (shown(14, 0x80), shown(12, 0x80));
    let low_panel = [0, 0, ((u16::from(even_row) + u16::from(odd_row)) / 2) as u8];
    let grey = |level: u32| [shown(level, 0x80); 3];
    let mut screen = open_pictures(&cx, &dir, false, None);
    let frame = render(&mut screen, &cx);
    // The first sketch, 300 x 200 at 270, 13 of 640 x 480: its first and
    // last rows share their pixels with the white behind.
    let sketch = [0, 0, shown(20, 0x80)];
    for (x, y, colour) in [
        (310, 5, white),
        (134, 50, white),
        (135, 50, sketch),
        (284, 50, sketch),
        (285, 50, white),
        (140, 5, white),
        (140, 6, halved(sketch, white)),
        (140, 7, sketch),
        (140, 105, sketch),
        (140, 106, halved(sketch, white)),
        (140, 107, white),
        // The panel's two sheets, 154 columns of each.
        (0, 0, panel),
        (76, 127, panel),
        (77, 127, white),
        (0, 128, low_panel),
        (76, 239, low_panel),
        (77, 239, white),
        // No key is lit.
        (20, 150, low_panel),
        // The count, "01/ 60", each digit 12 x 20 at 455 down: its first
        // and last rows are halves too.
        (280, 226, white),
        (280, 227, halved(grey(1), white)),
        (280, 228, grey(1)),
        (285, 236, grey(1)),
        (285, 237, halved(grey(1), white)),
        (285, 238, white),
        (286, 230, grey(2)),
        (292, 230, grey(15)),
        (297, 230, grey(15)),
        (298, 230, white),
        (300, 230, grey(7)),
        (306, 230, grey(1)),
        (311, 230, grey(1)),
        (312, 230, white),
    ] {
        assert_eq!(pixel(&frame, x, y), colour, "{x}, {y}");
    }
    // The second, 20 x 10 at 162, 67, with right held: its key is lit, 100
    // x 25 at 18, 285, over the panel.
    run(&mut screen, &cx, press("right"), 1);
    let turning = render(&mut screen, &cx);
    assert_eq!(
        pixel(&turning, 85, 35),
        white,
        "the turn's tick has no picture"
    );
    assert_eq!(pixel(&turning, 140, 50), white);
    assert_eq!(pixel(&turning, 286, 230), grey(3), "02");
    let held = UiInput {
        held_right: true,
        ..UiInput::default()
    };
    screen.update(&cx, held);
    let frame = render(&mut screen, &cx);
    let small = [0, shown(10, 0x80), 0];
    for (x, y, colour) in [
        (140, 50, white),
        (80, 35, white),
        (81, 33, halved(small, white)),
        (81, 34, small),
        (90, 37, small),
        (90, 38, halved(small, white)),
        (91, 35, white),
        (8, 150, low_panel),
        (9, 141, low_panel),
        // The key's first row shares its pixels with the panel's row above
        // it alone, not with the panel's pixel.
        (9, 142, halved(grey(20), [0, 0, even_row])),
        (58, 142, halved(grey(20), [0, 0, even_row])),
        (59, 142, low_panel),
        (9, 143, grey(20)),
        (58, 154, grey(20)),
        (58, 155, low_panel),
        (59, 150, low_panel),
        (286, 230, grey(3)),
    ] {
        assert_eq!(pixel(&frame, x, y), colour, "{x}, {y}");
    }
    // Each of the other keys' own.
    for (key, y, level) in [("left", 321, 22), ("up", 357, 24), ("down", 393, 26)] {
        let mut screen = open_pictures(&cx, &dir, false, None);
        run(&mut screen, &cx, press(key), 1);
        let frame = render(&mut screen, &cx);
        assert_eq!(pixel(&frame, 30, y / 2 + 5), grey(level), "{key}");
        assert_eq!(pixel(&frame, 30, y / 2 + 13), low_panel, "{key}");
    }
    // The fade's black over all of it, the white as well, and on the fade
    // out's last ticks over black.
    let under = |colour: [u8; 3], black: u32| {
        colour.map(|c| ((u32::from(c) * (255 - black) + 127) / 255) as u8)
    };
    let mut screen = open_pictures(&cx, &dir, false, None);
    run(&mut screen, &cx, press("up"), 1);
    let held = UiInput {
        held_up: true,
        ..UiInput::default()
    };
    screen.update(&cx, held);
    assert_eq!(black(&screen), Some(8));
    let faded = render(&mut screen, &cx);
    assert_eq!(pixel(&faded, 310, 5), under(white, 8));
    assert_eq!(pixel(&faded, 0, 0), under(panel, 8));
    assert_eq!(pixel(&faded, 140, 50), under(sketch, 8));
    assert_eq!(
        pixel(&faded, 140, 6),
        halved(under(sketch, 8), under(white, 8))
    );
    assert_eq!(pixel(&faded, 30, 357 / 2 + 5), under(grey(24), 8));
    run(&mut screen, &cx, UiInput::default(), 27);
    assert_eq!(black(&screen), Some(224));
    let late = render(&mut screen, &cx);
    assert_eq!(pixel(&late, 310, 5), under(white, 224));
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(black(&screen), Some(232));
    let last = render(&mut screen, &cx);
    assert_eq!(pixel(&last, 310, 5), [0, 0, 0]);
    assert_eq!(pixel(&last, 0, 0), under(panel, 232));
    assert_eq!(pixel(&last, 140, 50), under(sketch, 232));
    // The first load's tick: the panel and the count under the black, over
    // a frame still cleared to black.
    let mut screen = open_menu(&cx, &dir, false);
    run(&mut screen, &cx, press("confirm"), first_row_fade(false));
    run(&mut screen, &cx, UiInput::default(), 3);
    assert_eq!(black(&screen), Some(247));
    let loading = render(&mut screen, &cx);
    assert_eq!(pixel(&loading, 310, 5), [0, 0, 0]);
    assert_eq!(pixel(&loading, 140, 50), [0, 0, 0]);
    assert_eq!(pixel(&loading, 0, 0), under(panel, 247));
    assert_eq!(pixel(&loading, 292, 230), under(grey(15), 247));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_pack_short_of_the_pictures_panel_closes_the_gallery_as_they_open() {
    let dir = test_dir("gallery-panel");
    let cx = context_over(pack_without_gallery_pictures(), &dir);
    let mut screen = open_menu(&cx, &dir, false);
    let asleep = ti(&screen);
    run(
        &mut screen,
        &cx,
        press("confirm"),
        first_row_fade(false) + 1,
    );
    assert_eq!(gal(&screen).unwrap()[..4], [1, 1, 0, 0]);
    // The load's tick ends the task, with nothing drawn.
    cx.cues.borrow_mut().clear();
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(gal(&screen).unwrap()[..4], [2, 1, 0, 0]);
    assert_eq!(cleared(&screen), None);
    assert!(cx.cues.borrow().is_empty());
    // It closes on the next, and the title's page is back on the one after.
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(gal(&screen), None);
    assert_eq!(ti(&screen), asleep);
    run(&mut screen, &cx, press("down"), 1);
    assert_eq!(*cx.cues.borrow(), [UiCue::Core(4)]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The gallery's movies, faded in on number 0, with config.ini's `GALLERY0`
/// to `GALLERY4` at `opened`.
fn open_movies(
    cx: &UiContext<'_>,
    dir: &std::path::Path,
    claire: bool,
    opened: Option<[u32; 5]>,
) -> LuaScreen {
    let mut screen = open_menu_with(cx, dir, claire, opened);
    run(&mut screen, cx, press("down"), 4);
    // MOVIE's fade starts at 0x80; then the screen is set, its art loads,
    // and its own fade from 0xFF is over on the 33rd tick after.
    run(&mut screen, cx, press("confirm"), 16);
    run(&mut screen, cx, UiInput::default(), 2 + 32);
    assert_eq!(screen.value("gal_screen"), 2);
    assert_eq!(screen.value("gal_step"), 1);
    assert_eq!(screen.value("gal_flags") & 3, 0);
    assert_eq!(listed(&screen), Some(0));
    cx.cues.borrow_mut().clear();
    screen
}

#[test]
fn the_movie_rows_button_opens_the_movies_which_fade_in() {
    let dir = test_dir("gallery-movies");
    let cx = context(&dir);
    for claire in [false, true] {
        let mut screen = open_menu(&cx, &dir, claire);
        run(&mut screen, &cx, press("down"), 4);
        run(&mut screen, &cx, press("confirm"), 16);
        run(&mut screen, &cx, UiInput::default(), 1);
        // The menu's lit level stands still, with its flag as it was.
        let pulse = screen.value("gal_pulse");
        let falling = screen.value("gal_flags") & 0x20;
        assert_eq!(gal(&screen).unwrap()[..4], [1, 2, 0, 0]);
        // The art loads: the fade is set to 0xFF and steps once, and
        // nothing is drawn.
        run(&mut screen, &cx, UiInput::default(), 1);
        assert_eq!(gal(&screen), Some([1, 2, 1, 0, pulse, 247, 1 | falling]));
        assert_eq!(galv(&screen), [0, 0]);
        assert_eq!(listed(&screen), None);
        let shown = render(&mut screen, &cx);
        assert!(shown.rgba.chunks(4).all(|pixel| pixel[..3] == [0, 0, 0]));
        // The words from the next tick, the black 8 lighter a tick.
        for tick in 1..32 {
            run(&mut screen, &cx, UiInput::default(), 1);
            assert_eq!(listed(&screen), Some(0), "tick {tick}");
            assert_eq!(lit_word(&screen), None, "tick {tick}");
            assert_eq!(black(&screen), Some(255 - 8 * tick), "tick {tick}");
        }
        run(&mut screen, &cx, UiInput::default(), 1);
        assert_eq!(gal(&screen), Some([1, 2, 1, 0, pulse, -1, falling]));
        assert_eq!(black(&screen), None);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_movies_keys_turn_play_and_leave() {
    let dir = test_dir("gallery-movie-keys");
    let cx = context(&dir);
    let mut screen = open_movies(&cx, &dir, false, None);
    // Right and left turn with the cursor's cue, round the sixteen, and the
    // held one's arrow is lit.
    for (key, number) in [
        ("right", 1),
        ("right", 2),
        ("left", 1),
        ("left", 0),
        ("left", 15),
    ] {
        cx.cues.borrow_mut().clear();
        run(&mut screen, &cx, press(key), 1);
        assert_eq!(*cx.cues.borrow(), [UiCue::Core(4)], "{key}");
        assert_eq!(galv(&screen), [number, 0], "{key}");
        assert_eq!(listed(&screen), Some(number), "{key}");
        assert_eq!(lit_word(&screen).as_deref(), Some(key));
        run(&mut screen, &cx, UiInput::default(), 1);
        assert_eq!(lit_word(&screen), None);
    }
    let held = UiInput {
        held_right: true,
        ..UiInput::default()
    };
    run(&mut screen, &cx, press("right"), 1);
    for _ in 0..3 {
        screen.update(&cx, held);
        assert_eq!(galv(&screen), [0, 0], "a held key turns once");
        assert_eq!(lit_word(&screen).as_deref(), Some("right"));
    }
    // A button plays the movie's film, each of the three, and lights the
    // button for its tick; a button comes before the directions.
    let films = [0, 3, 15, 16, 10, 5, 4, 6, 7, 11, 8, 9, 12, 17, 13, 14];
    for (number, keys) in [
        (0, &["confirm"][..]),
        (1, &["cancel"][..]),
        (2, &["start", "left", "up"][..]),
    ] {
        cx.cues.borrow_mut().clear();
        let result = screen.update(&cx, press_all(keys));
        assert_eq!(*cx.cues.borrow(), [UiCue::Core(6)], "{keys:?}");
        assert!(
            matches!(
                result,
                ScreenResult::Done(ScreenAction::Film { id, claire: false, gallery: true }) if id == films[number]
            ),
            "{keys:?}"
        );
        assert_eq!(gal(&screen).unwrap()[..4], [1, 2, 1, 0], "{keys:?}");
        assert_eq!(galv(&screen), [number as i64, 0], "{keys:?}");
        // A held direction's arrow is lit before the button.
        let lit = if keys.len() > 1 { "left" } else { "button" };
        assert_eq!(lit_word(&screen).as_deref(), Some(lit), "{keys:?}");
        // The film over, the movies run on.
        screen.resume_film(&cx, false, true);
        assert!(matches!(
            screen.update(&cx, UiInput::default()),
            ScreenResult::Continue
        ));
        assert_eq!(listed(&screen), Some(number as i64));
        run(&mut screen, &cx, press("right"), 2);
    }
    // Up fades out to the pictures, and down to the menu.
    for (key, step, to) in [("up", 3, 1), ("down", 2, 0)] {
        let mut screen = open_movies(&cx, &dir, false, None);
        run(&mut screen, &cx, press("right"), 2);
        let pulse = screen.value("gal_pulse");
        let out = 3 | (screen.value("gal_flags") & 0x20);
        cx.cues.borrow_mut().clear();
        run(&mut screen, &cx, press(key), 1);
        assert_eq!(*cx.cues.borrow(), [UiCue::Core(6)], "{key}");
        assert_eq!(
            gal(&screen),
            Some([1, 2, step, 0, pulse, 247, out]),
            "{key}"
        );
        assert_eq!(lit_word(&screen).as_deref(), Some(key));
        assert_eq!(black(&screen), Some(0), "{key}");
        // The keys do nothing while it fades.
        let result = screen.update(&cx, press_all(&["confirm", "right"]));
        assert!(matches!(result, ScreenResult::Continue), "{key}");
        assert_eq!(cx.cues.borrow().len(), 1, "{key}");
        assert_eq!(galv(&screen), [1, 0], "{key}");
        for tick in 2..32 {
            run(&mut screen, &cx, UiInput::default(), 1);
            assert_eq!(black(&screen), Some(8 * tick), "{key} tick {tick}");
            assert_eq!(listed(&screen), Some(1), "{key} tick {tick}");
        }
        run(&mut screen, &cx, UiInput::default(), 1);
        assert_eq!(gal(&screen), Some([1, to, 0, 0, pulse, -1, out]), "{key}");
        assert_eq!(galv(&screen), [1, 0], "{key}");
        assert_eq!(listed(&screen), None, "{key}");
        run(&mut screen, &cx, UiInput::default(), 1);
        let state = gal(&screen).unwrap();
        assert_eq!(state[..4], [1, to, 1, if to == 1 { 1 } else { 0 }], "{key}");
        // The pictures' first frame is still cleared to black.
        run(&mut screen, &cx, UiInput::default(), 1);
        if to == 1 {
            assert_eq!(galv(&screen), [0, 2]);
            assert_eq!(cleared(&screen), Some(0));
        }
    }
    // CLAIRE's films are her own, and her way down is to the models.
    let mut screen = open_movies(&cx, &dir, true, None);
    for _ in 0..7 {
        run(&mut screen, &cx, press("right"), 2);
    }
    let result = screen.update(&cx, press("confirm"));
    assert!(matches!(
        result,
        ScreenResult::Done(ScreenAction::Film {
            id: 11,
            claire: true,
            gallery: true
        })
    ));
    screen.resume_film(&cx, true, true);
    run(&mut screen, &cx, press("down"), 33);
    assert_eq!(gal(&screen).unwrap()[..4], [1, 3, 0, 0]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_movies_keys_wait_for_the_art_and_take_one_way_at_a_time() {
    let dir = test_dir("gallery-movie-order");
    let cx = context(&dir);
    let to_the_load = |cx: &UiContext<'_>| {
        let mut screen = open_menu(cx, &dir, false);
        run(&mut screen, cx, press("down"), 4);
        run(&mut screen, cx, press("confirm"), 16);
        run(&mut screen, cx, UiInput::default(), 1);
        assert_eq!(gal(&screen).unwrap()[..4], [1, 2, 0, 0]);
        cx.cues.borrow_mut().clear();
        screen
    };
    // The tick the art loads on reads no key.
    let mut screen = to_the_load(&cx);
    let result = screen.update(&cx, press_all(&["confirm", "right", "up"]));
    assert!(matches!(result, ScreenResult::Continue));
    assert!(cx.cues.borrow().is_empty());
    assert_eq!(gal(&screen).unwrap()[..4], [1, 2, 1, 0]);
    assert_eq!(galv(&screen), [0, 0]);
    assert_eq!(listed(&screen), None);
    // The first tick that draws does, under the fade in: a turn, or the
    // film.
    run(&mut screen, &cx, press("right"), 1);
    assert_eq!(*cx.cues.borrow(), [UiCue::Core(4)]);
    assert_eq!(galv(&screen), [1, 0]);
    assert_eq!(listed(&screen), Some(1));
    assert_eq!(lit_word(&screen).as_deref(), Some("right"));
    assert_eq!(black(&screen), Some(247));
    let mut screen = to_the_load(&cx);
    run(&mut screen, &cx, UiInput::default(), 1);
    let result = screen.update(&cx, press("confirm"));
    assert!(matches!(
        result,
        ScreenResult::Done(ScreenAction::Film { id: 0, .. })
    ));
    assert_eq!(black(&screen), Some(247));
    // Of two directions pressed together one is taken, up before down
    // before right before left, and one is lit, the first held of right,
    // left, up and down.
    for (keys, step, number, cue, lit) in [
        (["up", "down"], 3, 0, 6, "up"),
        (["right", "left"], 1, 1, 4, "right"),
        (["up", "right"], 3, 0, 6, "right"),
        (["down", "left"], 2, 0, 6, "left"),
        (["down", "right"], 2, 0, 6, "right"),
    ] {
        let mut screen = open_movies(&cx, &dir, false, None);
        run(&mut screen, &cx, press_all(&keys), 1);
        assert_eq!(*cx.cues.borrow(), [UiCue::Core(cue)], "{keys:?}");
        assert_eq!(screen.value("gal_step"), step, "{keys:?}");
        assert_eq!(galv(&screen), [number, 0], "{keys:?}");
        assert_eq!(lit_word(&screen).as_deref(), Some(lit), "{keys:?}");
    }
    // A fade out still lights what is held, or the button a press is of,
    // through its last drawn tick.
    for key in ["up", "down"] {
        let mut screen = open_movies(&cx, &dir, false, None);
        run(&mut screen, &cx, press(key), 2);
        assert_eq!(lit_word(&screen), None, "{key}");
        let result = screen.update(&cx, press("left"));
        assert!(matches!(result, ScreenResult::Continue), "{key}");
        assert_eq!(lit_word(&screen).as_deref(), Some("left"), "{key}");
        let result = screen.update(&cx, press("start"));
        assert!(matches!(result, ScreenResult::Continue), "{key}");
        assert_eq!(lit_word(&screen).as_deref(), Some("button"), "{key}");
        let result = screen.update(&cx, press_all(&["cancel", "down"]));
        assert!(matches!(result, ScreenResult::Continue), "{key}");
        assert_eq!(lit_word(&screen).as_deref(), Some("down"), "{key}");
        assert_eq!(cx.cues.borrow().len(), 1, "{key}");
        assert_eq!(galv(&screen), [0, 0], "{key}");
        run(&mut screen, &cx, UiInput::default(), 26);
        screen.update(&cx, press("right"));
        assert_eq!(black(&screen), Some(8 * 31), "{key}");
        assert_eq!(lit_word(&screen).as_deref(), Some("right"), "{key}");
        // The tick the fade is over on draws nothing.
        screen.update(&cx, press("right"));
        assert_eq!(listed(&screen), None, "{key}");
        assert_eq!(galv(&screen), [0, 0], "{key}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_direction_held_through_a_film_is_no_new_press() {
    let dir = test_dir("gallery-movie-held");
    let cx = context(&dir);
    let held_down = UiInput {
        held_down: true,
        ..UiInput::default()
    };
    let mut screen = open_movies(&cx, &dir, false, None);
    let result = screen.update(&cx, press("confirm"));
    assert!(matches!(
        result,
        ScreenResult::Done(ScreenAction::Film { id: 0, .. })
    ));
    cx.cues.borrow_mut().clear();
    // Down went down under the film: the pad ran through it, so the
    // movies wake to a held key, which lights its arrow and no more.
    screen.resume_film(&cx, false, true);
    for _ in 0..3 {
        let result = screen.update(&cx, held_down);
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(gal(&screen).unwrap()[..4], [1, 2, 1, 0]);
        assert_eq!(lit_word(&screen).as_deref(), Some("down"));
    }
    assert!(cx.cues.borrow().is_empty());
    // Let go and pressed again, it leaves.
    run(&mut screen, &cx, UiInput::default(), 1);
    run(&mut screen, &cx, press("down"), 1);
    assert_eq!(*cx.cues.borrow(), [UiCue::Core(6)]);
    assert_eq!(screen.value("gal_step"), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn playing_a_movie_opens_it_by_its_place_in_leons_table() {
    let dir = test_dir("gallery-movie-unlock");
    let cx = context(&dir);
    let word =
        |screen: &LuaScreen| -> Option<i64> { screen.test_eval("return M.state.gal.open[4]") };
    let play = |screen: &mut LuaScreen, film: u8, claire: bool| {
        let result = screen.update(&cx, press("confirm"));
        assert_eq!(
            result,
            ScreenResult::Done(ScreenAction::Film {
                id: film,
                claire,
                gallery: true
            })
        );
        // Opened as it is asked for, played or not.
        screen.resume_film(&cx, false, false);
        run(screen, &cx, UiInput::default(), 1);
    };
    let turn = |screen: &mut LuaScreen, key: &str, number: i64| {
        run(screen, &cx, press(key), 2);
        assert_eq!(screen.value("gal_index"), number, "{key}");
    };
    // CLAIRE's number 7 alone opened. It is film 11, LEON's number 9, and
    // so it opens her number 9: film 7, LEON's number 8, which opens her
    // number 8: film 6, LEON's number 7, which is opened already.
    let mut screen = open_movies(&cx, &dir, true, Some([0, 0, 0, 0x0000_0100, 0]));
    turn(&mut screen, "right", 7);
    turn(&mut screen, "right", 7);
    play(&mut screen, 11, true);
    assert_eq!(word(&screen), Some(0x0000_0140));
    turn(&mut screen, "right", 9);
    turn(&mut screen, "right", 7);
    turn(&mut screen, "left", 9);
    play(&mut screen, 7, true);
    assert_eq!(word(&screen), Some(0x0000_01C0));
    turn(&mut screen, "left", 8);
    play(&mut screen, 6, true);
    assert_eq!(word(&screen), Some(0x0000_01C0));
    turn(&mut screen, "left", 7);
    turn(&mut screen, "left", 9);
    // The title's words are the ones set: the gallery finds them so again.
    assert_eq!(
        screen.test_eval::<Option<i64>>("return M.state.gallery[4]"),
        Some(0x0000_01C0)
    );
    // Number 0 is shown opened or not, and playing it opens it: a bit of
    // LEON's half, or of CLAIRE's.
    for (claire, bit) in [(false, 0x8000_0000), (true, 0x0000_8000)] {
        let mut screen = open_movies(&cx, &dir, claire, Some([0; 5]));
        assert_eq!(word(&screen), Some(0));
        play(&mut screen, 0, claire);
        assert_eq!(word(&screen), Some(bit), "claire {claire}");
    }
    // A gallery with everything opened stays as it is.
    let mut screen = open_movies(&cx, &dir, false, None);
    play(&mut screen, 0, false);
    assert_eq!(word(&screen), Some(0xFFFF_FFFF));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_turn_skips_the_movies_play_hasnt_opened() {
    let dir = test_dir("gallery-movie-opened");
    let cx = context(&dir);
    // The fourth word has a bit each from its top, LEON's sixteen and then
    // CLAIRE's: numbers 0, 3 and 15 of each here, and LEON's 7 alone.
    let opened = Some([0, 0, 0, 0x9101_9001, 0]);
    for (claire, turns) in [(false, &[3, 7, 15, 0][..]), (true, &[3, 15, 0][..])] {
        let mut screen = open_movies(&cx, &dir, claire, opened);
        for number in turns {
            run(&mut screen, &cx, press("right"), 2);
            assert_eq!(screen.value("gal_index"), *number, "claire {claire}");
        }
        for number in turns.iter().rev().skip(1) {
            run(&mut screen, &cx, press("left"), 2);
            assert_eq!(screen.value("gal_index"), *number, "claire {claire}");
        }
    }
    // With none opened a turn stays, with its cue; the pictures' words
    // aren't the movies'.
    let mut screen = open_movies(&cx, &dir, false, Some([!0, !0, !0, 0, !0]));
    run(&mut screen, &cx, press("right"), 2);
    run(&mut screen, &cx, press("left"), 2);
    assert_eq!(*cx.cues.borrow(), [UiCue::Core(4); 2]);
    assert_eq!(galv(&screen), [0, 0]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_movies_draw_at_half_size_in_the_originals_colours() {
    let dir = test_dir("gallery-movie-draw");
    let cx = context(&dir);
    let grey = |level: u32, at: u32| [shown(level, at); 3];
    let mut screen = open_movies(&cx, &dir, false, None);
    let frame = render(&mut screen, &cx);
    for (x, y, colour) in [
        (0, 0, [0, 0, 0]),
        (310, 230, [0, 0, 0]),
        // The arrows and the button, dim.
        (86, 120, grey(11, 0x40)),
        (114, 120, grey(12, 0x40)),
        (160, 112, grey(13, 0x40)),
        (160, 127, grey(14, 0x40)),
        (144, 202, grey(22, 0x40)),
        // MOVIE, LEON's two ways and PLAY, at their own level.
        (130, 120, grey(15, 0x80)),
        (200, 112, grey(16, 0x80)),
        (200, 126, grey(17, 0x80)),
        (160, 202, grey(21, 0x80)),
        // "00": each digit's rows 0 to 9 and 11 to 20, with no row 10. The
        // last pixel row is the mean of rows 19 and 20.
        (93, 115, grey(1, 0x80)),
        (93, 119, grey(1, 0x80)),
        (93, 120, grey(1, 0x80)),
        (93, 123, grey(1, 0x80)),
        (
            93,
            124,
            [((u16::from(shown(1, 0x80)) + u16::from(shown(23, 0x80))) / 2) as u8; 3],
        ),
        (93, 125, [0, 0, 0]),
        (105, 119, grey(1, 0x80)),
        (105, 120, grey(1, 0x80)),
        (
            105,
            124,
            [((u16::from(shown(1, 0x80)) + u16::from(shown(23, 0x80))) / 2) as u8; 3],
        ),
        // A word at an odd place: MOVIE's first column and row are halves.
        (121, 120, halved(grey(15, 0x80), [0, 0, 0])),
        (130, 116, halved(grey(15, 0x80), [0, 0, 0])),
        (120, 120, [0, 0, 0]),
    ] {
        assert_eq!(pixel(&frame, x, y), colour, "{x}, {y}");
    }
    // Number 15 with left held: its arrow at its own level.
    run(&mut screen, &cx, press("left"), 1);
    let frame = render(&mut screen, &cx);
    assert_eq!(pixel(&frame, 86, 120), grey(11, 0x80));
    assert_eq!(pixel(&frame, 114, 120), grey(12, 0x40));
    assert_eq!(pixel(&frame, 93, 117), grey(2, 0x80));
    assert_eq!(pixel(&frame, 105, 117), grey(6, 0x80));
    // Each of the others lit in its turn.
    for (key, x, y, level) in [
        ("right", 114, 120, 12),
        ("up", 160, 112, 13),
        ("down", 160, 127, 14),
        ("confirm", 144, 202, 22),
    ] {
        let mut screen = open_movies(&cx, &dir, false, None);
        screen.update(&cx, press(key));
        let frame = render(&mut screen, &cx);
        assert_eq!(pixel(&frame, x, y), grey(level, 0x80), "{key}");
        assert_eq!(pixel(&frame, 86, 120), grey(11, 0x40), "{key}");
    }
    // The fade's black over the words.
    let mut screen = open_movies(&cx, &dir, false, None);
    run(&mut screen, &cx, press("up"), 2);
    assert_eq!(black(&screen), Some(8));
    let faded = render(&mut screen, &cx);
    let under = |colour: [u8; 3]| colour.map(|c| ((u32::from(c) * 247 + 127) / 255) as u8);
    assert_eq!(pixel(&faded, 130, 120), under(grey(15, 0x80)));
    assert_eq!(pixel(&faded, 86, 120), under(grey(11, 0x40)));
    // CLAIRE's ways are ILLUSTRATION and 3D MODEL.
    let mut screen = open_movies(&cx, &dir, true, None);
    let frame = render(&mut screen, &cx);
    assert_eq!(pixel(&frame, 200, 112), grey(18, 0x80));
    assert_eq!(pixel(&frame, 200, 126), grey(19, 0x80));
    assert_eq!(pixel(&frame, 176, 112), [0, 0, 0], "left of ILLUSTRATION");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_pack_short_of_the_movies_words_closes_the_gallery_as_they_open() {
    let dir = test_dir("gallery-movie-bare");
    let cx = context_over(pack_without_gallery_pictures(), &dir);
    let mut screen = open_menu(&cx, &dir, false);
    run(&mut screen, &cx, press("down"), 4);
    run(&mut screen, &cx, press("confirm"), 17);
    assert_eq!(gal(&screen).unwrap()[..4], [1, 2, 0, 0]);
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(gal(&screen).unwrap()[..4], [2, 2, 0, 0]);
    assert_eq!(listed(&screen), None);
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(gal(&screen), None);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Whether the last tick's models screen shows its model; nothing on a
/// tick that drew no such screen.
fn modelled(screen: &LuaScreen) -> Option<bool> {
    screen.test_eval("local g = M.state.gal return g and g.view and g.view.model")
}

/// Whether the last tick left anything to draw: a menu's rows, a panel or
/// the movies' words.
fn drew(screen: &LuaScreen) -> bool {
    screen.test_eval("local g = M.state.gal return (g.words or g.view or g.list) ~= nil")
}

/// CLAIRE's models, faded in on number 0, with config.ini's `GALLERY0` to
/// `GALLERY4` at `opened`.
fn open_models(cx: &UiContext<'_>, dir: &std::path::Path, opened: Option<[u32; 5]>) -> LuaScreen {
    let mut screen = open_menu_with(cx, dir, true, opened);
    run(&mut screen, cx, press("down"), 4);
    run(&mut screen, cx, press("down"), 4);
    // 3D MODEL's fade starts at 0x80; then the screen is set, its art
    // loads, and its own fade from 0xFF is over on the 33rd tick after.
    run(&mut screen, cx, press("confirm"), 16);
    run(&mut screen, cx, UiInput::default(), 2 + 32);
    assert_eq!(screen.value("gal_screen"), 3);
    assert_eq!(screen.value("gal_step"), 1);
    assert_eq!(screen.value("gal_flags") & 0x13, 0x10);
    assert_eq!(modelled(&screen), Some(true));
    cx.cues.borrow_mut().clear();
    screen
}

#[test]
fn the_model_rows_button_opens_the_models_which_fade_in() {
    let dir = test_dir("gallery-models");
    let cx = context(&dir);
    let mut screen = open_menu(&cx, &dir, true);
    run(&mut screen, &cx, press("down"), 4);
    run(&mut screen, &cx, press("down"), 4);
    run(&mut screen, &cx, press("confirm"), 16);
    run(&mut screen, &cx, UiInput::default(), 1);
    let pulse = screen.value("gal_pulse");
    let falling = screen.value("gal_flags") & 0x20;
    assert_eq!(gal(&screen).unwrap()[..4], [1, 3, 0, 0]);
    // The art loads: a model waits (the row), the fade is set to 0xFF and
    // steps once, and nothing is drawn. The keys aren't read.
    cx.cues.borrow_mut().clear();
    run(&mut screen, &cx, press_all(&["right", "up"]), 1);
    assert_eq!(gal(&screen), Some([1, 3, 1, 1, pulse, 247, 1 | falling]));
    assert_eq!(galv(&screen), [0, 0]);
    assert!(!drew(&screen));
    assert!(cx.cues.borrow().is_empty());
    let shown = render(&mut screen, &cx);
    assert!(shown.rgba.chunks(4).all(|pixel| pixel[..3] == [0, 0, 0]));
    // The model is loaded on the next tick, which draws the panel without
    // it, and shows from the one after.
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(gal(&screen), Some([1, 3, 1, 0, pulse, 239, 0x11 | falling]));
    assert_eq!(modelled(&screen), Some(false));
    assert_eq!(cleared(&screen), Some(0));
    assert_eq!(black(&screen), Some(247));
    for tick in 2..32 {
        run(&mut screen, &cx, UiInput::default(), 1);
        assert_eq!(modelled(&screen), Some(true), "tick {tick}");
        assert_eq!(black(&screen), Some(255 - 8 * tick), "tick {tick}");
    }
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(gal(&screen), Some([1, 3, 1, 0, pulse, -1, 0x10 | falling]));
    assert_eq!(black(&screen), None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_models_keys_turn_and_leave() {
    let dir = test_dir("gallery-model-keys");
    let cx = context(&dir);
    let mut screen = open_models(&cx, &dir, None);
    // Right and left turn with the cursor's cue, round the thirty-eight. A
    // turn's tick loads the new model and shows none; the next shows it.
    for (key, number) in [
        ("right", 1),
        ("right", 2),
        ("left", 1),
        ("left", 0),
        ("left", 37),
    ] {
        cx.cues.borrow_mut().clear();
        run(&mut screen, &cx, press(key), 1);
        assert_eq!(*cx.cues.borrow(), [UiCue::Core(4)], "{key}");
        assert_eq!(gal(&screen).unwrap()[..4], [1, 3, 1, 0], "{key}");
        assert_eq!(galv(&screen), [number, 0], "{key}");
        assert_eq!(modelled(&screen), Some(false), "{key}");
        assert_eq!(screen.value("gal_flags") & 0x10, 0x10, "{key}");
        run(&mut screen, &cx, UiInput::default(), 1);
        assert_eq!(gal(&screen).unwrap()[..4], [1, 3, 1, 0], "{key}");
        assert_eq!(modelled(&screen), Some(true), "{key}");
    }
    // A held key turns once, and the buttons do nothing.
    let held = UiInput {
        held_right: true,
        ..UiInput::default()
    };
    run(&mut screen, &cx, press("right"), 1);
    for _ in 0..3 {
        screen.update(&cx, held);
        assert_eq!(galv(&screen), [0, 0]);
    }
    cx.cues.borrow_mut().clear();
    let result = screen.update(&cx, press_all(&["confirm", "cancel", "start"]));
    assert!(matches!(result, ScreenResult::Continue));
    assert!(cx.cues.borrow().is_empty());
    assert_eq!(gal(&screen).unwrap()[..4], [1, 3, 1, 0]);
    // Of two directions one is taken: up before down before right before
    // left. The first held of right, left, up and down is lit.
    for (keys, step, number, cue, lit) in [
        (["up", "down"], 2, 0, 6, 357),
        (["right", "left"], 1, 1, 4, 285),
        (["up", "right"], 2, 0, 6, 285),
        (["down", "left"], 4, 0, 6, 321),
    ] {
        let mut screen = open_models(&cx, &dir, None);
        run(&mut screen, &cx, press_all(&keys), 1);
        assert_eq!(*cx.cues.borrow(), [UiCue::Core(cue)], "{keys:?}");
        assert_eq!(screen.value("gal_step"), step, "{keys:?}");
        assert_eq!(galv(&screen), [number, 0], "{keys:?}");
        assert_eq!(lit_key(&screen), Some(lit), "{keys:?}");
    }
    // Up fades out to the movies, and down to the menu, with the model
    // shown to the last drawn tick and let go on the next.
    for (key, step, to) in [("up", 2, 2), ("down", 4, 0)] {
        let mut screen = open_models(&cx, &dir, None);
        run(&mut screen, &cx, press("right"), 2);
        let pulse = screen.value("gal_pulse");
        let falling = screen.value("gal_flags") & 0x20;
        cx.cues.borrow_mut().clear();
        run(&mut screen, &cx, press(key), 1);
        assert_eq!(*cx.cues.borrow(), [UiCue::Core(6)], "{key}");
        assert_eq!(
            gal(&screen),
            Some([1, 3, step, 0, pulse, 247, 0x13 | falling]),
            "{key}"
        );
        assert_eq!(black(&screen), Some(0), "{key}");
        // The keys do nothing while it fades.
        let result = screen.update(&cx, press_all(&["confirm", "right"]));
        assert!(matches!(result, ScreenResult::Continue), "{key}");
        assert_eq!(cx.cues.borrow().len(), 1, "{key}");
        assert_eq!(galv(&screen), [1, 0], "{key}");
        assert_eq!(lit_key(&screen), Some(285), "{key}");
        for tick in 2..32 {
            run(&mut screen, &cx, UiInput::default(), 1);
            assert_eq!(black(&screen), Some(8 * tick), "{key} tick {tick}");
            assert_eq!(modelled(&screen), Some(true), "{key} tick {tick}");
        }
        run(&mut screen, &cx, UiInput::default(), 1);
        assert_eq!(
            gal(&screen),
            Some([1, to, 0, 0, pulse, -1, 3 | falling]),
            "{key}"
        );
        assert_eq!(galv(&screen), [1, 0], "{key}");
        assert!(!drew(&screen), "{key}");
        assert_eq!(black(&screen), None, "{key}");
        // The next screen loads: the movies' words, or the menu.
        run(&mut screen, &cx, UiInput::default(), 1);
        assert_eq!(gal(&screen).unwrap()[..4], [1, to, 1, 0], "{key}");
    }
    // The movies' way down comes back to the models, on number 0 again.
    let mut screen = open_models(&cx, &dir, None);
    run(&mut screen, &cx, press("right"), 2);
    run(&mut screen, &cx, press("up"), 33 + 1 + 32);
    assert_eq!(gal(&screen).unwrap()[..4], [1, 2, 1, 0]);
    run(&mut screen, &cx, press("down"), 33);
    assert_eq!(gal(&screen).unwrap()[..4], [1, 3, 0, 0]);
    run(&mut screen, &cx, UiInput::default(), 3);
    assert_eq!(gal(&screen).unwrap()[..4], [1, 3, 1, 0]);
    assert_eq!(galv(&screen), [0, 0]);
    assert_eq!(modelled(&screen), Some(true));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_turn_skips_the_models_play_hasnt_opened() {
    let dir = test_dir("gallery-model-opened");
    let cx = context(&dir);
    // The fifth word has the models' bits from its top: the first two
    // models share bit 0, and six of the zombies bit 1. Here bits 1, 3 and
    // 31: models 5, 7 to 10 and 13, model 2 and model 37.
    let opened = Some([!0, !0, !0, !0, 0x5000_0001]);
    let mut screen = open_models(&cx, &dir, opened);
    assert_eq!(galv(&screen), [0, 0], "number 0 is shown, opened or not");
    for number in [2, 5, 7, 8, 9, 10, 13, 37, 2] {
        run(&mut screen, &cx, press("right"), 2);
        assert_eq!(screen.value("gal_index"), number);
    }
    for number in [37, 13, 10] {
        run(&mut screen, &cx, press("left"), 2);
        assert_eq!(screen.value("gal_index"), number);
    }
    // Bit 0 alone opens the first two.
    let mut screen = open_models(&cx, &dir, Some([0, 0, 0, 0, 0x8000_0000]));
    for number in [1, 0, 1] {
        run(&mut screen, &cx, press("right"), 2);
        assert_eq!(screen.value("gal_index"), number);
    }
    // Each bit alone opens its models and no others (kind 4's table).
    let bits: Vec<u32> = [0, 0, 3, 4, 5, 1, 2, 1, 1, 1, 1, 6, 7, 1]
        .into_iter()
        .chain(8..32)
        .collect();
    assert_eq!(bits.len(), 38);
    for bit in 0..32 {
        let models: Vec<i64> = (0..38).filter(|n| bits[*n as usize] == bit).collect();
        let mut screen = open_models(&cx, &dir, Some([!0, !0, !0, !0, 0x8000_0000 >> bit]));
        let mut reached: Vec<i64> = (0..models.len())
            .map(|_| {
                run(&mut screen, &cx, press("right"), 2);
                screen.value("gal_index")
            })
            .collect();
        reached.sort_unstable();
        assert_eq!(reached, models, "bit {bit}");
    }
    // With none opened a turn stays, with its cue, and loads the model
    // again.
    let mut screen = open_models(&cx, &dir, Some([!0, !0, !0, !0, 0]));
    run(&mut screen, &cx, press("right"), 1);
    assert_eq!(*cx.cues.borrow(), [UiCue::Core(4)]);
    assert_eq!(modelled(&screen), Some(false));
    assert_eq!(galv(&screen), [0, 0]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_models_panel_draws_at_half_size_in_the_originals_colours() {
    let dir = test_dir("gallery-model-draw");
    let cx = context(&dir);
    let black = [0, 0, 0];
    let grey = |level: u32| [shown(level, 0x80); 3];
    let panel = [shown(9, 0x80), 0, 0];
    let (even_row, odd_row) = (shown(13, 0x80), shown(11, 0x80));
    let low_panel = [((u16::from(even_row) + u16::from(odd_row)) / 2) as u8, 0, 0];
    let mut screen = open_models(&cx, &dir, None);
    let frame = render(&mut screen, &cx);
    for (x, y, colour) in [
        // The panel's two sheets, 154 columns of each, over black.
        (0, 0, panel),
        (76, 127, panel),
        (77, 127, black),
        (0, 128, low_panel),
        (76, 239, low_panel),
        (77, 239, black),
        (200, 100, black),
        // No key is lit.
        (20, 150, low_panel),
        // The count, "01/ 38", in the models' own digits.
        (280, 226, black),
        (280, 227, halved(grey(11), black)),
        (280, 228, grey(11)),
        (286, 230, grey(12)),
        (292, 230, grey(25)),
        (300, 230, grey(14)),
        (306, 230, grey(19)),
        (312, 230, black),
    ] {
        assert_eq!(pixel(&frame, x, y), colour, "{x}, {y}");
    }
    // Number 37 with left held: its key is lit, 100 x 25 at 18, 321.
    run(&mut screen, &cx, press("left"), 1);
    let frame = render(&mut screen, &cx);
    assert_eq!(lit_key(&screen), Some(321));
    assert_eq!(pixel(&frame, 20, 165), grey(23));
    assert_eq!(pixel(&frame, 57, 165), grey(23));
    assert_eq!(pixel(&frame, 20, 150), low_panel, "right's key isn't");
    // The key's last column is see-through: its last pixels are the mean of
    // its last but one and the panel's own column beside it, not of the
    // panel's halved pixel. On its first row, half a pixel down, the
    // panel's row above is the other half.
    let beside = shown(20, 0x80);
    let mean = |a: u8, b: u8| (u16::from(a) + u16::from(b)).div_ceil(2) as u8;
    let last = [
        mean(grey(23)[0], beside),
        mean(grey(23)[0], 0),
        mean(grey(23)[0], 0),
    ];
    assert_eq!(pixel(&frame, 58, 165), last);
    assert_eq!(pixel(&frame, 58, 172), last);
    let above = [mean(shown(13, 0x80), beside), 0, 0];
    assert_eq!(pixel(&frame, 58, 160), halved(last, above));
    assert_eq!(pixel(&frame, 59, 165), low_panel, "past the key");
    assert_eq!(pixel(&frame, 280, 230), grey(14), "38");
    assert_eq!(pixel(&frame, 286, 230), grey(19));
    // The fade's black over the panel.
    let mut screen = open_models(&cx, &dir, None);
    run(&mut screen, &cx, press("down"), 2);
    let faded = render(&mut screen, &cx);
    let under = |colour: [u8; 3]| colour.map(|c| ((u32::from(c) * 247 + 127) / 255) as u8);
    assert_eq!(pixel(&faded, 0, 0), under(panel));
    assert_eq!(pixel(&faded, 286, 230), under(grey(12)));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_pack_short_of_the_models_panel_closes_the_gallery_as_they_open() {
    let dir = test_dir("gallery-model-bare");
    let cx = context_over(pack_without_gallery_pictures(), &dir);
    let mut screen = open_menu(&cx, &dir, true);
    run(&mut screen, &cx, press("down"), 4);
    run(&mut screen, &cx, press("down"), 4);
    run(&mut screen, &cx, press("confirm"), 17);
    assert_eq!(gal(&screen).unwrap()[..4], [1, 3, 0, 0]);
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(gal(&screen).unwrap()[..4], [2, 3, 0, 0]);
    assert_eq!(modelled(&screen), None);
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(gal(&screen), None);
    let _ = std::fs::remove_dir_all(&dir);
}
