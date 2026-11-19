//! RE 2's gallery (`games/re2/ui/gallery.lua`) under the title, against the
//! original's trace and frames (`oracle/re2.md`, "The gallery").

use super::re2_title_tests::{
    GALLERY_WORDS, context, context_over, frame, gallery_words, open_gallery,
    pack_without_gallery_art, press, press_all, run, ti, words,
};
use super::*;

use crate::ui::{Screen, UiContext, UiCue, UiInput};

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

/// The title taken into the gallery as LEON or CLAIRE: the tick the title
/// hands off on, which is the gallery task's first.
fn start_gallery(cx: &UiContext<'_>, dir: &std::path::Path, claire: bool) -> LuaScreen {
    let mut screen = open_gallery(cx, dir);
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
    let mut screen = start_gallery(cx, dir, claire);
    run(&mut screen, cx, UiInput::default(), 40);
    assert_eq!(screen.value("gal_fade"), 0);
    assert_eq!(screen.value("gal_flags") & 3, 0);
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
fn a_button_on_a_row_fades_the_menu_out_and_the_menu_loads_again() {
    let dir = test_dir("gallery-row");
    let cx = context(&dir);
    // The character, the way to the row, the row it turns to, the level its
    // fade starts at and the ticks to the one that finds it done.
    for (claire, way, turned, fade, ticks) in [
        (false, &[][..], 3, 0xFF, 32),
        (false, &["down"][..], 4, 0x80, 16),
        (true, &[][..], 3, 0x80, 16),
        (true, &["down"][..], 6, 0x80, 16),
        (true, &["down", "down"][..], 7, 0x80, 16),
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
        // The tick that finds the fade done draws nothing. The row's
        // screen isn't ported: the menu starts again from its load.
        run(&mut screen, &cx, UiInput::default(), 1);
        let state = gal(&screen).unwrap();
        assert_eq!(state[..4], [1, 0, 0, 0], "{name}");
        assert_eq!(rows(&screen), None, "{name}");
        run(&mut screen, &cx, UiInput::default(), 1);
        let state = gal(&screen).unwrap();
        assert_eq!((state[2], state[3], state[5]), (1, 0, 120), "{name}");
        assert_eq!(state[6] & 3, 1, "{name}");
        run(&mut screen, &cx, UiInput::default(), 20);
        assert_eq!(
            rows(&screen).map(|rows| rows.len()),
            Some(if claire { 4 } else { 3 })
        );
        assert_eq!(black(&screen), None, "{name}");
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
