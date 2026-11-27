//! RE 2's title (`games/re2/ui/title.lua`) over a pack holding only the
//! script and its backgrounds, against the original's trace
//! (`oracle/re2.md`, "The title").

use super::*;

use crate::pack::Pack;
use crate::ui::{Screen, ScreenAction, ScreenResult, UiCue, UiInput};

/// How much of the gallery's art a pack has: none, the menu's sheets,
/// those with the pictures' panel, some of the pictures and the movies'
/// words, or those with every one of LEON's sketches.
#[derive(Clone, Copy, PartialEq, PartialOrd)]
enum GalleryArt {
    None,
    Menu,
    All,
    Sketches,
}

/// The title's pack; `prologue_art` adds what the prologue shows, which is
/// what makes a new game run it, and `gallery_art` the gallery's sheets.
fn build_pack(prologue_art: bool, gallery_art: GalleryArt) -> Pack {
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
            "ui/gallery.lua",
            include_str!("../../../games/re2/ui/gallery.lua"),
        ),
        (
            "ui/exbattle.lua",
            include_str!("../../../games/re2/ui/exbattle.lua"),
        ),
        (
            "ui/ex_result.lua",
            include_str!("../../../games/re2/ui/ex_result.lua"),
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
    // The backgrounds, each a 4 x 4 grey of its own from white down, so a
    // test can tell which one is up.
    for (index, stem) in [
        "title/title_bg",
        "option/room",
        "option/color2",
        "title/warning",
    ]
    .into_iter()
    .enumerate()
    {
        let picture = super::tests::solid_bmp(4, 4, [0xFF - 0x10 * index as u8; 3]);
        writer.add(&format!("ui/{stem}.bmp"), picture).unwrap();
    }
    // The gallery's sheets (`re2_gallery_tests`): sheet `n` of the picture
    // is all index 0, a red of level `n`, but for the right half of the
    // fifth, a green; the words' sheet is white where a word is and
    // see-through black elsewhere.
    for n in (1..=5u16).filter(|_| gallery_art >= GalleryArt::Menu) {
        let sheet = sheet_tim(&[n, 5 << 5], |u, _| u8::from(n == 5 && u >= 128));
        writer
            .add(&format!("ui/gallery/title{n}.tim"), sheet)
            .unwrap();
    }
    let words = sheet_tim(&[0, 0x7FFF], |u, v| {
        u8::from(
            GALLERY_WORDS
                .iter()
                .any(|&[u0, v0, w, h]| (u0..u0 + w).contains(&u) && (v0..v0 + h).contains(&v)),
        )
    });
    if gallery_art >= GalleryArt::Menu {
        writer.add("ui/gallery/moji.tim", words).unwrap();
    }
    if gallery_art >= GalleryArt::All {
        let mut art: Vec<(String, Vec<u8>)> = gallery_pictures_art()
            .into_iter()
            .map(|(stem, sheet)| (stem.to_owned(), sheet))
            .collect();
        if gallery_art == GalleryArt::Sketches {
            // The sketches the others lack, each a 20 x 10 green.
            for n in 1..=60 {
                let stem = format!("rough{n:02}");
                if art.iter().all(|(held, _)| *held != stem) {
                    art.push((stem, tim_8bit(20, 10, &[0, 10 << 5], |_, _| 1)));
                }
            }
        }
        for (stem, sheet) in art {
            writer
                .add(&format!("ui/gallery/{stem}.tim"), sheet)
                .unwrap();
        }
        // Two of the gallery's models, each a triangle 800 wide and 800
        // tall on its point, facing along X (`re2_gallery_tests`): its
        // first, an enemy's file with its page and its second row's
        // swatch, 1600 up, and the first player's.
        let mut texture = crate::emd2::texture_bmps(&crate::emd2::tests::paged_tim())
            .unwrap()
            .into_iter();
        let triangle =
            crate::emd2::tests::emd_of([[0, -2400, -400], [0, -2400, 400], [0, -1600, 0], [0; 3]]);
        writer.add("enemy/50/enemy_1.emd", triangle).unwrap();
        writer
            .add("enemy/50/enemy_1.bmp", texture.next().unwrap())
            .unwrap();
        writer
            .add(
                &crate::emd2::clut_entry("enemy/50/enemy_1", 1),
                texture.next().unwrap(),
            )
            .unwrap();
        // Zombie 0x12 as a mod might replace it: seventeen parts, each the
        // enemy's triangle, on a two-page texture, short of the pages its
        // skins move parts to.
        let triangle = crate::emd2::tests::emd_parts(
            [[0, -2400, -400], [0, -2400, 400], [0, -1600, 0], [0; 3]],
            17,
        );
        writer.add("enemy/12/enemy_1.emd", triangle).unwrap();
        for (row, bmp) in crate::emd2::texture_bmps(&crate::emd2::tests::paged_tim())
            .unwrap()
            .into_iter()
            .enumerate()
        {
            let entry = match row {
                0 => "enemy/12/enemy_1.bmp".to_owned(),
                row => crate::emd2::clut_entry("enemy/12/enemy_1", row),
            };
            writer.add(&entry, bmp).unwrap();
        }
        let triangle =
            crate::emd2::tests::pld_of([[0, -800, -400], [0, -800, 400], [0, 0, 0], [0; 3]]);
        writer.add("player/0b/model_1.pld", triangle).unwrap();
    }
    if prologue_art {
        for stem in ["open10", "open11", "open12", "open13", "open14"] {
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

/// The words of the gallery's sheet, each `[u, v, w, h]`: ROUGH SKETCH,
/// MOVIE, ILLUSTRATION, 3D MODEL and EXIT.
pub(super) const GALLERY_WORDS: [[u32; 4]; 5] = [
    [0, 0, 244, 22],
    [0, 22, 103, 22],
    [0, 44, 226, 22],
    [0, 66, 157, 22],
    [0, 110, 79, 21],
];

/// The grey of 5-bit level `level`, as a TIM colour.
const fn grey(level: u16) -> u16 {
    level | level << 5 | level << 10
}

/// The art of the gallery's pictures (`re2_gallery_tests`), each sheet a
/// few flat colours of its own.
///
/// The panel: `left01` a red of level 10 and `left02` a blue of level 14
/// on its even rows and 12 on its odd ones, each with a green right of its
/// 154 columns that must never show; `left02`'s four lit keys there are
/// greys of levels 20, 22, 24 and 26.
/// `gnum2`: digit `d` a grey of level `d + 1`, the stroke one of level 15.
///
/// The pictures: LEON's first (300 x 200, a blue of level 20), second and
/// fourth (20 x 10, a green of level 10) and last (258 x 258), and
/// CLAIRE's first and last (20 x 10). The rest are missing.
fn gallery_pictures_art() -> Vec<(&'static str, Vec<u8>)> {
    let keys = |u: u32, v: u32| match (u, v) {
        (0..154, _) => {
            if v & 1 == 0 {
                6
            } else {
                1
            }
        }
        (154..254, 0..100) => 2 + (v / 25) as u8,
        _ => 0,
    };
    let digits = |u: u32, v: u32| match (u, v) {
        (35..47, 40..60) => 11,
        (0..60, 0..40) => 1 + (v / 20 * 5 + u / 12) as u8,
        _ => 0,
    };
    let mut digit_colours = vec![0];
    digit_colours.extend((1..=10).map(grey));
    digit_colours.push(grey(15));
    let mut model_digit_colours = vec![0];
    model_digit_colours.extend((11..=20).map(grey));
    model_digit_colours.push(grey(25));
    let green = [0, 10 << 5];
    let small = || tim_8bit(20, 10, &green, |_, _| 1);
    vec![
        (
            "left01",
            sheet_tim(&[0, 10, 5 << 5], |u, _| 1 + u8::from(u >= 154)),
        ),
        (
            "left02",
            sheet_tim(
                &[
                    0,
                    12 << 10,
                    grey(20),
                    grey(22),
                    grey(24),
                    grey(26),
                    14 << 10,
                ],
                keys,
            ),
        ),
        ("gnum2", tim_8bit(64, 64, &digit_colours, digits)),
        ("rough01", tim_8bit(300, 200, &[0, 20 << 10], |_, _| 1)),
        ("rough02", small()),
        ("rough04", small()),
        ("rough60", tim_8bit(258, 258, &green, |_, _| 1)),
        ("illust01", small()),
        ("illust35", small()),
        ("movie64", movie_words_art()),
        // The models' panel, in reds where the pictures' is blue, with its
        // own greys for the keys, whose last column is see-through, and a
        // red of its own down the panel's column beside them; and its
        // digits: digit `d` level `d + 11` and the stroke 25.
        (
            "3dleft1",
            sheet_tim(&[0, 9, 5 << 5], |u, _| 1 + u8::from(u >= 154)),
        ),
        (
            "3dleft2",
            sheet_tim(
                &[0, 11, grey(21), grey(23), grey(25), grey(27), 13, 20],
                |u, v| match (u, v) {
                    (253, 0..100) => 0,
                    (117, _) => 7,
                    _ => keys(u, v),
                },
            ),
        ),
        ("gnum", tim_8bit(64, 64, &model_digit_colours, digits)),
    ]
}

/// The movies' sheet (`re2_gallery_tests`), each word a grey of its own
/// level over see-through black: digit `d` level `d + 1` on its rows 0 to 9
/// and 11 to 19, with a green on row 10, which is never drawn, and nothing
/// on row 20; then the left arrow 11, the right 12, up 13, down 14, MOVIE
/// 15, ROUGH SKETCH 16, TO HOME 17, ILLUSTRATION 18, 3D MODEL 19, PLAY 21
/// and the button 22.
fn movie_words_art() -> Vec<u8> {
    let mut colours: Vec<u16> = (0..=23).map(grey).collect();
    colours[20] = 31 << 5;
    sheet_tim(&colours, |u, v| match (u, v) {
        // A digit's last row, which the left arrow's piece starts on too.
        (0..120, 20) => 23,
        // The rows between the words.
        (_, 20 | 150 | 164) => 0,
        (0..120, 0..10 | 11..20) => 1 + (u / 12) as u8,
        (0..120, 10) => 20,
        (0..11, 21..39) => 11,
        (0..11, 39..58) => 12,
        (0..19, 58..69) => 13,
        (0..19, 69..80) => 14,
        (0..59, 80..94) => 15,
        (0..139, 94..108) => 16,
        (0..81, 108..122) => 17,
        (0..127, 122..136) => 18,
        (0..90, 136..150) => 19,
        (0..48, 151..164) => 21,
        (0..16, 165..181) => 22,
        _ => 0,
    })
}

/// An 8-bit 256 x 256 TIM: `palette`'s 15-bit colours from index 0, and
/// each texel's index from `texel(u, v)`.
fn sheet_tim(palette: &[u16], texel: impl Fn(u32, u32) -> u8) -> Vec<u8> {
    tim_8bit(256, 256, palette, texel)
}

/// An 8-bit TIM of an even `width`.
fn tim_8bit(width: u16, height: u16, palette: &[u16], texel: impl Fn(u32, u32) -> u8) -> Vec<u8> {
    let mut tim = vec![0x10, 0, 0, 0, 9, 0, 0, 0];
    tim.extend((12u32 + 512).to_le_bytes());
    tim.extend([0, 0, 0, 0, 0, 1, 1, 0]);
    for index in 0..256 {
        tim.extend(palette.get(index).copied().unwrap_or(0).to_le_bytes());
    }
    tim.extend((12u32 + u32::from(width) * u32::from(height)).to_le_bytes());
    tim.extend([0, 0, 0, 0]);
    tim.extend((width / 2).to_le_bytes());
    tim.extend(height.to_le_bytes());
    for v in 0..u32::from(height) {
        tim.extend((0..u32::from(width)).map(|u| texel(u, v)));
    }
    tim
}

fn pack() -> &'static Pack {
    use std::sync::OnceLock;
    static PACK: OnceLock<Pack> = OnceLock::new();
    PACK.get_or_init(|| build_pack(false, GalleryArt::All))
}

fn prologue_pack() -> &'static Pack {
    use std::sync::OnceLock;
    static PACK: OnceLock<Pack> = OnceLock::new();
    PACK.get_or_init(|| build_pack(true, GalleryArt::All))
}

/// The title's pack with none of the gallery's sheets.
pub(super) fn pack_without_gallery_art() -> &'static Pack {
    use std::sync::OnceLock;
    static PACK: OnceLock<Pack> = OnceLock::new();
    PACK.get_or_init(|| build_pack(false, GalleryArt::None))
}

/// The title's pack with the gallery's menu and none of its pictures' art.
pub(super) fn pack_without_gallery_pictures() -> &'static Pack {
    use std::sync::OnceLock;
    static PACK: OnceLock<Pack> = OnceLock::new();
    PACK.get_or_init(|| build_pack(false, GalleryArt::Menu))
}

/// The title's pack with every one of LEON's sketches.
pub(super) fn pack_with_every_sketch() -> &'static Pack {
    use std::sync::OnceLock;
    static PACK: OnceLock<Pack> = OnceLock::new();
    PACK.get_or_init(|| build_pack(false, GalleryArt::Sketches))
}

pub(super) fn context<'a>(dir: &'a std::path::Path) -> UiContext<'a> {
    context_over(pack(), dir)
}

pub(super) fn context_over<'a>(pack: &'static Pack, dir: &'a std::path::Path) -> UiContext<'a> {
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
pub(super) fn ti(screen: &LuaScreen) -> Vec<i64> {
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

/// `Init_movie_work` opens the film it plays in the gallery's words: the
/// boot's film of the title is the gallery's number 0, LEON's or CLAIRE's
/// as the film is.
#[test]
fn the_boots_film_opens_the_gallerys_number_0() {
    let dir = std::env::temp_dir().join(format!("arklay-re2-boot-film-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cx = context(&dir);
    let mut cx_open = context(&dir);
    let names = ["gallery0", "gallery1", "gallery2", "gallery3", "gallery4"];
    let args = names.map(|name| (name, ScreenArg::Int(0)));
    let mut screen =
        LuaScreen::open_screen("title", &args, ScreenAction::Quit, &mut cx_open).unwrap();
    screen.signal("on_boot", &cx);
    let film = (0..=188).find_map(|_| match screen.update(&cx, UiInput::default()) {
        ScreenResult::Done(ScreenAction::Film { id: 0, claire, .. }) => Some(claire),
        _ => None,
    });
    let claire = film.expect("the title's film is asked for");
    assert_eq!(
        screen.test_eval::<Option<i64>>("return M.state.gallery[4]"),
        Some(if claire { 0x0000_8000 } else { 0x8000_0000 })
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The boot before the title (`title_boot`, the original's ticks 0 to 188):
/// `Init_main` and `Capcom_logo` step as the trace's `ti` has them, the
/// warning's picture is up under its two fades, the core bank loads on 35,
/// and tick 188 asks for the title's film. With no film played
/// `Capcom_logo` ends on that tick and `Title_init` runs on the next; after
/// one that played, by a press or not, its step 8 takes a black tick first.
#[test]
fn the_boot_shows_the_warning_then_asks_for_the_titles_film() {
    for (played, skipped) in [(false, false), (true, false), (true, true)] {
        let dir = std::env::temp_dir().join(format!(
            "arklay-re2-boot-{}-{played}-{skipped}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let cx = context(&dir);
        let mut cx_open = context(&dir);
        let mut screen =
            LuaScreen::open_screen("title", &[], ScreenAction::Quit, &mut cx_open).unwrap();
        assert!(screen.boots());
        screen.signal("on_boot", &cx);
        assert_eq!(ti(&screen)[..4], [0, 0, 0, 0]);
        let mut film = None;
        for tick in 0..=188i64 {
            cx.cues.borrow_mut().clear();
            // A direction held under the warning repeats nothing: the pad's
            // repeat is the title's from its task's first tick, 35.
            let input = UiInput {
                down: tick == 10,
                held_down: (10..=30).contains(&tick),
                ..UiInput::default()
            };
            let result = screen.update(&cx, input);
            let repeat: (Option<i64>, Option<i64>) =
                screen.test_eval("return M.state.rep_mask, M.state.rep_count");
            let expected = if tick < 35 { (0, 1) } else { (0x5000, 1) };
            assert_eq!(repeat, (Some(expected.0), Some(expected.1)), "tick {tick}");
            let steps = match tick {
                0 => [0, 0, 1, 0],
                1..=34 => [0, 0, 2, 0],
                35..=154 => [0, 0, 1, 5],
                155 => [0, 0, 1, 6],
                156..=187 => [0, 0, 1, 7],
                _ => [0, 0, 1, 8],
            };
            assert_eq!(ti(&screen)[..4], steps, "tick {tick}");
            // `fg_system`'s "from the logo" bit goes up as `Init_main` ends.
            assert_eq!(screen.value("ti_fgsys") & 2 != 0, tick >= 34, "tick {tick}");
            // So is `Rnd`'s state seeded: 0 before.
            let rnd = if tick >= 34 { 0x6CA4 } else { 0 };
            assert_eq!(screen.value("ti_rnd"), rnd, "tick {tick}");
            // The fade after the tick's step: in from 0x7C00, then out to
            // 0x8000.
            let fade = match tick {
                0 => [-1, 0, 0],
                1..=31 => [0x7C00 - 0x400 * tick, -0x400, 2],
                32..=154 => [-0x400, -0x400, 2],
                155..=185 => [0x400 * (tick - 154), 0x400, 2],
                _ => [-0x8000, 0x400, 2],
            };
            let shown =
                ["ti_fade_level", "ti_fade_add", "ti_fade_mode"].map(|key| screen.value(key));
            assert_eq!(shown, fade, "tick {tick}");
            let (background, title) = backgrounds(&screen);
            assert_eq!(
                background.is_some(),
                (1..=186).contains(&tick),
                "tick {tick}"
            );
            assert_ne!(background, title, "tick {tick}");
            let bank = cx.cues.borrow().contains(&UiCue::CoreBank(0x10));
            assert_eq!(bank, tick == 35, "tick {tick}");
            match result {
                ScreenResult::Continue => assert!(tick < 188),
                ScreenResult::Done(ScreenAction::Film { id, .. }) => {
                    assert_eq!(tick, 188);
                    film = Some(id);
                }
                ScreenResult::Done(action) => panic!("tick {tick}: {action:?}"),
            }
        }
        assert_eq!(film, Some(0));
        assert_eq!(frame(&screen).as_deref(), Some("black"));
        screen.resume_film(&cx, skipped, played);
        if played {
            assert_eq!(ti(&screen)[..4], [0, 0, 1, 8], "the task hasn't run yet");
            assert_eq!(
                screen.update(&cx, UiInput::default()),
                ScreenResult::Continue
            );
            assert_eq!(frame(&screen).as_deref(), Some("black"));
        }
        assert_eq!(ti(&screen)[..4], [1, 0, 1, 0]);
        // `Title_init`, as a title opened after the boot starts (the
        // original's tick 189 with no film).
        screen.update(&cx, UiInput::default());
        assert_eq!(ti(&screen), [2, 5, 1, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(screen.value("ti_fade_level"), 0x7F80);
        assert_eq!(screen.value("ti_fgsys") & 2, 0, "`Title_init` took the bit");
        let (background, title) = backgrounds(&screen);
        assert!(background.is_some() && background == title);
        assert!(!screen.failed());
        let _ = std::fs::remove_dir_all(&dir);
    }
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
    screen.resume_loaded(&cx, None);
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
        ScreenResult::Done(ScreenAction::Continue { prologue: false })
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
                extra_player: 0,
                exbattle: None,
            },
        })
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The title opened with config.ini's `SPECIAL` at `special`, run through
/// `title_special.txt`'s presses up to SPECIAL's first tick (the original's
/// 260, 290 and 320 down and 380 confirm, from `Title_init` at 189).
fn open_special(cx: &UiContext<'_>, dir: &std::path::Path, special: i64) -> LuaScreen {
    open_special_with(cx, dir, special, None)
}

/// [`open_special`], with config.ini's `GALLERY0` to `GALLERY4` at `opened`.
fn open_special_with(
    cx: &UiContext<'_>,
    dir: &std::path::Path,
    special: i64,
    opened: Option<[u32; 5]>,
) -> LuaScreen {
    let mut cx_open = context(dir);
    let mut args = vec![("special", ScreenArg::Int(special))];
    let names = ["gallery0", "gallery1", "gallery2", "gallery3", "gallery4"];
    for (name, word) in names.into_iter().zip(opened.into_iter().flatten()) {
        args.push((name, ScreenArg::Int(i64::from(word))));
    }
    let mut screen =
        LuaScreen::open_screen("title", &args, ScreenAction::Quit, &mut cx_open).unwrap();
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

pub(super) fn run(screen: &mut LuaScreen, cx: &UiContext<'_>, input: UiInput, ticks: usize) {
    screen.update(cx, input);
    for _ in 1..ticks {
        screen.update(cx, UiInput::default());
    }
}

pub(super) fn press(direction: &str) -> UiInput {
    let mut input = UiInput::default();
    match direction {
        "down" => (input.down, input.held_down) = (true, true),
        "up" => (input.up, input.held_up) = (true, true),
        "left" => (input.left, input.held_left) = (true, true),
        "right" => (input.right, input.held_right) = (true, true),
        "confirm" => input.confirm = true,
        "cancel" => input.cancel = true,
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
fn the_4th_survivors_title_builds_and_its_start_hands_over_hunks_game() {
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
    // fade to the hand-off on the 270th tick, and on the next HUNK's game
    // (`Title_game_init`'s extras branch).
    run(&mut screen, &cx, UiInput::default(), 10);
    cx.cues.borrow_mut().clear();
    run(&mut screen, &cx, press("start"), 1);
    assert_eq!(*cx.cues.borrow(), [UiCue::Core(0), UiCue::Core(1)]);
    assert_eq!(screen.value("ti_fade_level"), 0x7800 - 0x800);
    run(&mut screen, &cx, UiInput::default(), 268);
    assert_eq!(screen.value("ti_sub2"), 3);
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!((screen.value("ti_mv"), screen.value("ti_disp")), (2, 3));
    assert_eq!(
        done(screen.update(&cx, UiInput::default())),
        Some(extra_game(0xC))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The action an update finished with, if it did.
fn done(result: crate::ui::ScreenResult) -> Option<ScreenAction> {
    match result {
        crate::ui::ScreenResult::Done(action) => Some(action),
        _ => None,
    }
}

/// The new game an extra's START hands over: player `player` (0xC HUNK,
/// 0xD TOFU), with the title's character, Leon from the boot.
fn extra_game(player: u8) -> ScreenAction {
    ScreenAction::NewGame {
        character: 0,
        choice: crate::ui::NewGameChoice {
            extra_player: player,
            exbattle: None,
            ..Default::default()
        },
    }
}

#[test]
fn to_fus_start_hands_over_tofus_game() {
    let dir = std::env::temp_dir().join(format!("arklay-re2-tofu-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cx = context(&dir);
    let mut screen = open_special(&cx, &dir, 15);
    run(&mut screen, &cx, UiInput::default(), 20);
    run(&mut screen, &cx, press("right"), 10);
    run(&mut screen, &cx, press("confirm"), 1);
    run(&mut screen, &cx, UiInput::default(), 17);
    assert_eq!(screen.value("ti_fgsys") & 9, 9);
    // START once the logo is up (as `special_tofu`'s, 580 ticks after the
    // confirm), then the hand-off and TOFU's game.
    run(&mut screen, &cx, UiInput::default(), 562);
    let mut action = done(screen.update(&cx, press("start")));
    for _ in 0..400 {
        if action.is_some() {
            break;
        }
        action = done(screen.update(&cx, UiInput::default()));
    }
    assert_eq!(action, Some(extra_game(0xD)));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A boot whose film picked Claire, then 4TH, EXIT and 4TH's START again:
/// EXIT's `Init_global` clears `fg_status`, the character's bit 31 with it,
/// so the game is HUNK's on Leon's disc.
#[test]
fn an_extras_exit_drops_the_boots_character() {
    let dir = std::env::temp_dir().join(format!("arklay-re2-4th-claire-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cx = context(&dir);
    let mut screen = open_special(&cx, &dir, 15);
    // The boot's film pick, `fg_status`'s bit 31.
    assert!(screen.test_eval::<bool>("M.state.claire = true; return true"));
    run(&mut screen, &cx, UiInput::default(), 20);
    // 4TH, its build-up skipped, and EXIT.
    run(&mut screen, &cx, press("confirm"), 30);
    run(&mut screen, &cx, press("confirm"), 21);
    run(&mut screen, &cx, press("down"), 5);
    assert_eq!(screen.value("ti_cur"), 1);
    run(&mut screen, &cx, press("confirm"), 1);
    let mut ticks = 1;
    while screen.value("ti_mv") != 1 {
        run(&mut screen, &cx, UiInput::default(), 1);
        ticks += 1;
        assert!(ticks < 20);
    }
    assert!(!screen.test_eval::<bool>("return M.state.claire"));
    // The title from black, SPECIAL (the fourth row) and 4TH again.
    run(&mut screen, &cx, UiInput::default(), 200);
    for _ in 0..3 {
        run(&mut screen, &cx, press("down"), 30);
    }
    assert_eq!(screen.value("ti_cur"), 3);
    run(&mut screen, &cx, press("confirm"), 20);
    run(&mut screen, &cx, press("confirm"), 30);
    run(&mut screen, &cx, press("confirm"), 21);
    let mut action = done(screen.update(&cx, press("start")));
    for _ in 0..400 {
        if action.is_some() {
            break;
        }
        action = done(screen.update(&cx, UiInput::default()));
    }
    assert_eq!(action, Some(extra_game(0xC)));
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

/// SPECIAL with every extra, its cursor on EX.BATTLE and confirmed, to the
/// tick `Load_title_extreme` runs on, then through its 62-tick fade in.
fn open_extreme(cx: &UiContext<'_>, dir: &std::path::Path) -> LuaScreen {
    let mut screen = open_special(cx, dir, 15);
    run(&mut screen, cx, UiInput::default(), 20);
    run(&mut screen, cx, press("down"), 10);
    assert_eq!(screen.value("ti_cur"), 2);
    cx.cues.borrow_mut().clear();
    run(&mut screen, cx, press("confirm"), 1);
    assert_eq!(*cx.cues.borrow(), [UiCue::Core(6)]);
    // Slot 2's fade, done on the 17th tick: Extreme Battle's flags and
    // `title_mv_state` 5.
    run(&mut screen, cx, UiInput::default(), 17);
    assert_eq!(screen.value("ti_mv"), 5);
    assert_eq!(screen.value("ti_fgsys") & 0x0100_0009, 0x0100_0000);
    assert_eq!(screen.value("ti_fgstatus"), 0x4000);
    assert_eq!(screen.value("ti_save_count"), 0xFFFF, "`FUN_004B73C0`");
    // `Load_title_extreme` in one tick: core bank 0x14, slot 3 at 0x7C00
    // and `Ti_kido` at 0xF8.
    cx.cues.borrow_mut().clear();
    run(&mut screen, cx, UiInput::default(), 1);
    assert_eq!(*cx.cues.borrow(), [UiCue::CoreBank(0x14)]);
    assert_eq!((screen.value("ti_mv"), screen.value("ti_disp")), (6, 0));
    assert_eq!(screen.value("ti_fade3_level"), 0x7C00);
    assert_eq!(screen.value("ti_kido"), 0xF8);
    screen
}

fn work_x(screen: &LuaScreen, entry: usize) -> i64 {
    screen.test_eval::<i64>(&format!("return M.state.work[{entry}].x"))
}

#[test]
fn extreme_battles_title_fades_in_then_slides_its_logo_in() {
    let dir = std::env::temp_dir().join(format!("arklay-re2-ex-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cx = context(&dir);
    let mut screen = open_extreme(&cx, &dir);
    // Slot 3 lifts by 4 << 7 a tick for 62 ticks, then the logo's count.
    run(&mut screen, &cx, UiInput::default(), 2);
    assert_eq!(screen.value("ti_fade3_level"), 0x7C00 - 0x200);
    run(&mut screen, &cx, UiInput::default(), 59);
    assert_eq!(screen.value("ti_disp"), 0);
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(screen.value("ti_disp"), 1);
    assert_eq!(screen.value("ti_demo"), 0x20);
    assert_eq!(screen.value("ti_fade3_level"), -1);
    // The logo's halves slide in from either side: at n = 31, 31² * 128
    // / 1024 = 120.
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(screen.value("ti_demo"), 31);
    assert_eq!(
        [29, 30, 31, 32].map(|entry| work_x(&screen, entry)),
        [-120, 0xBC - 120, 120, 0xBC + 120]
    );
    run(&mut screen, &cx, UiInput::default(), 31);
    assert_eq!(screen.value("ti_demo"), 0);
    assert_eq!(screen.value("ti_step"), 0);
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(screen.value("ti_step"), 1);
    // The right half's second copy fades by 8 a tick.
    assert_eq!(screen.test_eval::<i64>("return M.state.work[32].r"), 0x78);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_button_in_extreme_battles_fade_in_shows_its_logo_at_once() {
    let dir = std::env::temp_dir().join(format!("arklay-re2-ex-skip-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cx = context(&dir);
    let mut screen = open_extreme(&cx, &dir);
    run(&mut screen, &cx, UiInput::default(), 10);
    // `special_ex_back`'s press on the fade in's twelfth tick: 0xF8 - 44
    // left, by 0x20 a tick, 7 ticks, the logo drawn from the press and its
    // count started on it. The last tick draws nothing: the page is back at
    // 0.
    run(&mut screen, &cx, UiInput::default(), 1);
    run(&mut screen, &cx, press("confirm"), 1);
    assert_eq!(
        (screen.value("ti_menu"), screen.value("ti_demo")),
        (1, 0x1F)
    );
    run(&mut screen, &cx, UiInput::default(), 5);
    assert_eq!(screen.value("ti_disp"), 0);
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!((screen.value("ti_disp"), screen.value("ti_menu")), (1, 0));
    assert_eq!(screen.value("ti_demo"), 0x1F - 5);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Extreme Battle's title with its rows up and the cursor on LOAD GAME.
fn extreme_rows(cx: &UiContext<'_>, dir: &std::path::Path) -> LuaScreen {
    let mut screen = open_extreme(cx, dir);
    run(&mut screen, cx, UiInput::default(), 62 + 33 + 5);
    assert_eq!((screen.value("ti_step"), screen.value("ti_cur")), (1, 0));
    screen
}

#[test]
fn extreme_battles_level_page_chooses_a_level_and_starts() {
    let dir = std::env::temp_dir().join(format!("arklay-re2-ex-level-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cx = context(&dir);
    let mut screen = extreme_rows(&cx, &dir);
    run(&mut screen, &cx, press("down"), 5);
    assert_eq!(screen.value("ti_cur"), 1);
    cx.cues.borrow_mut().clear();
    run(&mut screen, &cx, press("confirm"), 1);
    assert_eq!(*cx.cues.borrow(), [UiCue::Core(6)]);
    assert_eq!(screen.value("ti_step"), 2);
    // The level page's slide: 8 ticks, the digits at 0x60, 0x90 and 0xC0
    // with all three levels open.
    run(&mut screen, &cx, UiInput::default(), 7);
    assert_eq!(screen.value("ti_page2"), 1);
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(screen.value("ti_page2"), 2);
    assert_eq!(
        [33, 34, 35].map(|entry| work_x(&screen, entry)),
        [0x60, 0x90, 0xC0]
    );
    // Right up to level 2 (the third), each with the cursor cue, and no
    // further; left back to 1.
    for (direction, level, cued) in [
        ("right", 1, true),
        ("right", 2, true),
        ("right", 2, false),
        ("left", 1, true),
    ] {
        cx.cues.borrow_mut().clear();
        run(&mut screen, &cx, press(direction), 3);
        assert_eq!(screen.value("ti_ef3"), level, "{direction}");
        assert_eq!(cx.cues.borrow().contains(&UiCue::Core(4)), cued);
    }
    // The confirm on the level: the confirm's two cues and white flash on
    // the next tick, then the fade from 0x1000, and the hand-off.
    cx.cues.borrow_mut().clear();
    run(&mut screen, &cx, press("confirm"), 1);
    assert_eq!(screen.value("ti_page2"), 4);
    assert!(cx.cues.borrow().is_empty());
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(*cx.cues.borrow(), [UiCue::Core(0), UiCue::Core(1)]);
    run(&mut screen, &cx, UiInput::default(), 17);
    assert_eq!(screen.value("ti_ef2"), 3);
    assert_eq!(screen.value("ti_fade_level"), 0x1000 + 0x80);
    run(&mut screen, &cx, UiInput::default(), 224);
    assert_eq!(screen.value("ti_page2"), 5);
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!((screen.value("ti_mv"), screen.value("ti_disp")), (2, 3));
    assert_eq!(screen.value("ti_fgsys") & 0x0102_0000, 0x0102_0000);
    // `Title_game_init` from the next tick: the character's choice, whose
    // confirm on Leon starts Extreme Battle's game on level 2.
    let (choice, _) = choose_extreme(&mut screen, &cx, &[]);
    assert_eq!(
        choice.map(|choice| (choice.player, choice.level, choice.levels)),
        Some((0, 2, 2))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// From the tick after Extreme Battle's hand-off: `Title_game_init`'s
/// first tick, then the character's choice, with `moves` pressed for 3
/// ticks each once the choice reads its keys, a confirm, and the ticks to
/// the new game the title hands over, with the choice's trace on the tick
/// of the first move.
fn choose_extreme(
    screen: &mut LuaScreen,
    cx: &UiContext<'_>,
    moves: &[&str],
) -> (Option<crate::ui::ExBattleChoice>, Option<String>) {
    let exb = |screen: &LuaScreen| screen.state_str("exb_trace");
    // `Title_main`'s step 3, then `Title_game_init`'s default step and step
    // 1, which starts the choice: its init on the same tick.
    assert_eq!(done(screen.update(cx, UiInput::default())), None);
    assert_eq!(exb(screen), None);
    cx.cues.borrow_mut().clear();
    assert_eq!(done(screen.update(cx, UiInput::default())), None);
    // Core bank 0x15 and the music table's stub, main 0x39.
    assert_eq!(
        *cx.cues.borrow(),
        [UiCue::CoreBank(0x15), UiCue::Music(0x39)]
    );
    assert_eq!((screen.value("ti_init"), screen.value("ti_sub")), (0, 2));
    assert_eq!(
        exb(screen).as_deref(),
        Some(
            "1,0,1,1,1,8,0,0,0/0,0,1,50/2048,2048,0,-2250,0,0,0;1024,1024,2250,0,3584,3584,0;\
             0,0,0,2250,3904,3904,0;3072,3072,-2250,0,2048,2048,0"
        ),
        "the choice as `special_ex_leon` has it on its first tick"
    );
    assert_eq!(screen.value("ti_fade_level"), 0x7400 - 0xC00);
    // The fade in is done on the tenth tick, and the keys are read from the
    // eleventh.
    run(screen, cx, UiInput::default(), 9);
    assert!(exb(screen).unwrap().starts_with("1,0,1,"));
    run(screen, cx, UiInput::default(), 1);
    assert!(exb(screen).unwrap().starts_with("1,0,2,"));
    let mut first = None;
    for direction in moves {
        cx.cues.borrow_mut().clear();
        run(screen, cx, press(direction), 1);
        assert!(cx.cues.borrow().contains(&UiCue::Core(4)), "{direction}");
        first.get_or_insert_with(|| exb(screen).unwrap());
        // The four turn for 16 ticks, and a key in that time does nothing.
        run(screen, cx, press(direction), 2);
        run(screen, cx, UiInput::default(), 15);
    }
    cx.cues.borrow_mut().clear();
    run(screen, cx, press("confirm"), 1);
    assert!(cx.cues.borrow().contains(&UiCue::Core(6)));
    for _ in 0..100 {
        if let Some(ScreenAction::NewGame { choice, .. }) =
            done(screen.update(cx, UiInput::default()))
        {
            return (choice.exbattle, first);
        }
    }
    (None, first)
}

#[test]
fn extreme_battles_choice_turns_the_four_and_wraps_past_ada_to_chris() {
    let dir = std::env::temp_dir().join(format!("arklay-re2-ex-choice-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cx = context(&dir);
    let mut screen = extreme_rows(&cx, &dir);
    run(&mut screen, &cx, press("down"), 5);
    run(&mut screen, &cx, press("confirm"), 1);
    run(&mut screen, &cx, UiInput::default(), 8);
    // Level 3.
    run(&mut screen, &cx, press("right"), 3);
    run(&mut screen, &cx, press("right"), 3);
    run(&mut screen, &cx, press("confirm"), 1);
    run(&mut screen, &cx, UiInput::default(), 243);
    assert_eq!((screen.value("ti_mv"), screen.value("ti_disp")), (2, 3));
    let (choice, first) = choose_extreme(&mut screen, &cx, &["left", "left"]);
    // The first move's tick, from Leon to Ada: every player 0x40 round the
    // circle at radius 0x8CA and a sixteenth of its turn toward its target.
    let first = first.unwrap();
    // `special_ex_ada`'s 1250 has them so.
    let players = first.rsplit('/').next().unwrap();
    assert_eq!(
        players,
        "1984,1024,220,-2240,4064,3584,-32;960,0,2239,220,3604,3904,20;\
         4032,3072,-221,2239,3788,2048,-116;3008,2048,-2240,-221,1920,0,-128"
    );
    assert!(first.starts_with("1,0,2,1,0,0,0,1,0/0,0,1,"), "{first}");
    assert_eq!(
        choice.map(|choice| (choice.player, choice.level)),
        Some((0xB, 3)),
        "left from Ada wraps round to Chris"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn extreme_battles_placement_draws_four_of_eight_from_rnd() {
    let dir = std::env::temp_dir().join(format!("arklay-re2-ex-rnd-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cx = context(&dir);
    let screen = open_special(&cx, &dir, 15);
    // `special_ex`'s: `Rnd` at 0x6CA4 gives 0x27 and leaves 9491 (0x2513).
    assert_eq!(
        screen.test_eval::<(i64, i64)>(
            "local ExBattle = require('ui/exbattle') return ExBattle.placement(0x6CA4)"
        ),
        (0x27, 0x2513)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// `FUN_004DE430`: a level's three times in the small digits with their
/// colons, its total in the large, dashes for none and the largest for 100
/// hours or more.
#[test]
fn ex_results_times_are_hours_minutes_and_seconds() {
    let dir = std::env::temp_dir().join(format!("arklay-re2-exr-time-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cx = context(&dir);
    let screen = open_special(&cx, &dir, 15);
    let string = |value: u32, id: u32| {
        screen.test_eval::<Vec<i64>>(&format!(
            "local R = require('ui/ex_result') local c = R.new({{}}, {{}}, 2) \
             R.time_string(c.strings, {value}, {id}) return c.strings[{id}]"
        ))
    };
    assert_eq!(
        string(0, 0),
        [0x40, 0x40, 0x3C, 0x40, 0x40, 0x3C, 0x40, 0x40]
    );
    assert_eq!(
        string(0, 3),
        [0x3F, 0x3F, 0x27, 0x3F, 0x3F, 0x27, 0x3F, 0x3F]
    );
    // 1:02:05 in seconds.
    assert_eq!(
        string(3725, 1),
        [0x32, 0x33, 0x3C, 0x32, 0x34, 0x3C, 0x32, 0x37]
    );
    assert_eq!(
        string(3725, 7),
        [0x1A, 0x1B, 0x27, 0x1A, 0x1C, 0x27, 0x1A, 0x1F]
    );
    assert_eq!(
        string(100 * 3600, 2),
        [0x3B, 0x3B, 0x3C, 0x37, 0x3B, 0x3C, 0x37, 0x3B]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// After an Extreme Battle game (`FUN_004DDFF0`, `special_ex_leon_end`):
/// the page counts to 0xC9; at 0xB4 a level opens when the game's is past
/// those open, and a run better than the level's record (or the first)
/// takes it with NEW RECORD, which pulses; a press past the count fades the
/// music and slot 0, 0x5A ticks pass, and the task ends black.
#[test]
fn ex_results_after_a_game_take_the_record_and_open_a_level() {
    let dir = std::env::temp_dir().join(format!("arklay-re2-exr-after-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cx = context(&dir);
    let screen = open_special(&cx, &dir, 15);
    let script = r#"
        local R = require('ui/ex_result')
        local fade = { level = -1 }
        local fx = {
            set = function(a0, add) fade.add = add; fade.level = add < 0 and add + 0x8000 or 0 end,
            done = function() return fade.level < 0 end,
            level = function(_, level) fade.level = level end,
        }
        local records = {}
        for level = 1, 3 do records[level] = { character = 0, kept = false, times = { 0, 0, 0 } } end
        local c = R.new(fx, records, 1, { level = 2, character = 3, times = { 0, 0, 0 } })
        local out = {}
        local function step(edge)
            R.tick(c, { edge = edge or 0 })
            if fade.level >= 0 then fade.level = fade.level + fade.add end
            if fade.level > 0x7FFF then fade.level = -1 end
        end
        while c.routine < 2 do step() end
        step()
        out[#out + 1] = c.sub
        while c.count < 0xB5 do step() end
        out[#out + 1] = c.levels
        out[#out + 1] = records[2].kept and 1 or 0
        out[#out + 1] = records[2].character
        local record
        for _, slot in pairs(c.slots) do if slot.id == 0x15 then record = slot end end
        out[#out + 1] = record.colour
        local peak, low = 0, 0x100
        for _ = 1, 60 do
            step()
            if record.colour > peak then peak = record.colour end
            if peak >= 0x80 and record.colour < low then low = record.colour end
        end
        out[#out + 1] = peak >= 0x80 and 1 or 0
        out[#out + 1] = low < 0x80 and low >= 0x34 and 1 or 0
        step(0x80)
        out[#out + 1] = c.sub
        step(0x80)
        out[#out + 1] = c.sub
        out[#out + 1] = c.count
        local ticks = 0
        while c.routine == 2 do step() ticks = ticks + 1 end
        out[#out + 1] = ticks
        while not c.ended do step() end
        out[#out + 1] = fade.level
        return out
    "#;
    let out = screen.test_eval::<Vec<i64>>(script);
    // The after-game page, not the title's.
    assert_eq!(out[0], 2);
    // Level 2 opens past level 1, and the first record is the run's, Chris's.
    assert_eq!(&out[1..4], [2, 1, 3]);
    // NEW RECORD fades in slowly from black, then pulses below full.
    assert_eq!(out[4], 4);
    assert_eq!(&out[5..7], [1, 1]);
    // A press past 0xC9 (the count is already past it): sub-step 3 waits
    // 0x5A ticks and one more, then routine 3 ends black.
    assert_eq!(out[7], 3);
    assert_eq!(&out[8..10], [3, 0x59]);
    assert_eq!(out[10], 0x5A);
    assert_eq!(out[11], 0x7FFF);
}

/// RESULT's records (`FUN_004DD4F0`): two loading ticks drawing nothing,
/// slot 0's fade in, then RESULT, the three labels on the count's 0x1E and
/// the times on 0x3C, each fading in from black over 16 ticks, opaque; past 0x5A a
/// press fades out, and once the fade is done the task ends black.
#[test]
fn ex_results_records_show_by_their_count_and_end_on_a_press() {
    let dir = std::env::temp_dir().join(format!("arklay-re2-exr-tick-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cx = context(&dir);
    let screen = open_special(&cx, &dir, 15);
    let script = r#"
        local R = require('ui/ex_result')
        local fade = { level = -1 }
        local fx = {
            set = function(a0, add) fade.add = add; fade.level = add < 0 and add + 0x8000 or 0 end,
            done = function() return fade.level < 0 end,
            level = function(_, level) fade.level = level end,
        }
        local records = {}
        for level = 1, 3 do records[level] = { character = 0, kept = false, times = { 0, 0, 0 } } end
        local c = R.new(fx, records, 2)
        local out = {}
        local function step(edge)
            R.tick(c, { edge = edge or 0 })
            if fade.level >= 0 then fade.level = fade.level + fade.add end
            if fade.level > 0x7FFF then fade.level = -1 end
        end
        step() out[#out + 1] = #c.sprites
        step() out[#out + 1] = #c.sprites
        step() out[#out + 1] = fade.add
        local ticks = 0
        while c.routine == 1 do step() ticks = ticks + 1 end
        out[#out + 1] = ticks
        step() step()
        out[#out + 1] = #c.sprites
        out[#out + 1] = c.sprites[1].colour
        for _ = 1, 0x1E do step() end
        local label = c.sprites[2]
        out[#out + 1] = label.colour
        out[#out + 1] = label.blend == nil and 1 or 0
        for _ = 1, 15 do step() end
        out[#out + 1] = c.sprites[2].colour
        out[#out + 1] = c.sprites[2].blend == nil and 1 or 0
        while c.count < 0x5B do step() end
        out[#out + 1] = #c.sprites
        step(0x80)
        out[#out + 1] = c.routine
        while not c.ended do step() end
        out[#out + 1] = fade.level
        out[#out + 1] = #c.sprites
        return out
    "#;
    let out = screen.test_eval::<Vec<i64>>(script);
    // Two loading ticks with nothing drawn, then the fade in from black.
    assert_eq!(&out[..3], [0, 0, -0x200]);
    // `FUN_004DD670` waits for it: from 0x7E00 by 0x200, 64 ticks to below
    // 0.
    assert_eq!(out[3], 64);
    // RESULT alone, its glyph at full colour.
    assert_eq!(&out[4..6], [1, 0x80]);
    // A label's first tick: 8; 16 ticks on, 0x80; opaque throughout.
    assert_eq!(&out[6..10], [8, 1, 0x80, 1]);
    // Every glyph: RESULT (1), the labels (3 x 3), the times (3 x (3 x 8 +
    // 1 + 8)).
    assert_eq!(out[10], 1 + 9 + 3 * (3 * 8 + 1 + 8));
    assert_eq!(out[11], 3);
    // Black, and nothing drawn.
    assert_eq!(&out[12..], [0x7FFF, 0]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn extreme_battles_level_page_exit_and_cancel_slide_back_to_start() {
    let dir = std::env::temp_dir().join(format!("arklay-re2-ex-back-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cx = context(&dir);
    let mut screen = extreme_rows(&cx, &dir);
    run(&mut screen, &cx, press("down"), 5);
    for leave in [&["down", "confirm"][..], &["cancel"][..]] {
        run(&mut screen, &cx, press("confirm"), 10);
        assert_eq!(screen.value("ti_page2"), 2);
        cx.cues.borrow_mut().clear();
        for key in leave {
            run(&mut screen, &cx, press(key), 1);
        }
        assert_eq!(screen.value("ti_page2"), 3);
        assert_eq!(cx.cues.borrow().last(), Some(&UiCue::Core(5)));
        run(&mut screen, &cx, UiInput::default(), 7);
        assert_eq!(screen.value("ti_page2"), 3);
        run(&mut screen, &cx, UiInput::default(), 1);
        assert_eq!(
            [
                screen.value("ti_page2"),
                screen.value("ti_step"),
                screen.value("ti_cur")
            ],
            [0, 1, 1]
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn extreme_battles_exit_goes_back_to_the_title_from_black() {
    let dir = std::env::temp_dir().join(format!("arklay-re2-ex-exit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cx = context(&dir);
    let mut screen = extreme_rows(&cx, &dir);
    for _ in 0..3 {
        run(&mut screen, &cx, press("down"), 5);
    }
    assert_eq!(screen.value("ti_cur"), 3);
    cx.cues.borrow_mut().clear();
    run(&mut screen, &cx, press("confirm"), 1);
    assert_eq!(*cx.cues.borrow(), [UiCue::Core(5)]);
    // A fade to black by 0x1800 a tick from 0x200 on the next tick: done on
    // the seventh.
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(screen.value("ti_fade_level"), 0x200 + 0x1800);
    run(&mut screen, &cx, UiInput::default(), 5);
    assert_eq!(screen.value("ti_step"), 1);
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(screen.value("ti_step"), 2);
    cx.cues.borrow_mut().clear();
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(screen.value("ti_mv"), 1);
    assert_eq!(*cx.cues.borrow(), [UiCue::CoreBank(0x10)]);
    assert_eq!(screen.value("ti_fgsys") & 0x0100_0000, 0);
    assert_eq!(screen.value("ti_fgstatus"), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn extreme_battles_result_loads_its_title_again() {
    let dir = std::env::temp_dir().join(format!("arklay-re2-ex-result-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cx = context(&dir);
    let mut screen = extreme_rows(&cx, &dir);
    for _ in 0..2 {
        run(&mut screen, &cx, press("down"), 5);
    }
    cx.cues.borrow_mut().clear();
    run(&mut screen, &cx, press("confirm"), 1);
    assert_eq!(*cx.cues.borrow(), [UiCue::Core(6)]);
    run(&mut screen, &cx, UiInput::default(), 7);
    assert_eq!(screen.value("ti_step"), 2);
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(screen.value("ti_mv"), 7);
    // RESULT's records (`special_ex_result`: `title_mv_state` 7 on 708):
    // the task's `+9` up from 709, the cancel on 900, the fade out to 932
    // and `title_mv_state` 5 on 933, the title loaded again on the next.
    run(&mut screen, &cx, UiInput::default(), 191);
    assert_eq!((screen.value("ti_mv"), screen.value("ti_sub")), (7, 1));
    run(&mut screen, &cx, press("cancel"), 1);
    run(&mut screen, &cx, UiInput::default(), 32);
    assert_eq!((screen.value("ti_mv"), screen.value("ti_sub")), (7, 1));
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!((screen.value("ti_mv"), screen.value("ti_sub")), (5, 0));
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!((screen.value("ti_mv"), screen.value("ti_disp")), (6, 0));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn extreme_battles_load_game_comes_back_to_its_title() {
    let dir = std::env::temp_dir().join(format!("arklay-re2-ex-load-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cx = context(&dir);
    let mut screen = extreme_rows(&cx, &dir);
    cx.cues.borrow_mut().clear();
    let mut action = done(screen.update(&cx, press("confirm")));
    for _ in 0..300 {
        if action.is_some() {
            break;
        }
        action = done(screen.update(&cx, UiInput::default()));
    }
    assert_eq!(action, Some(ScreenAction::SaveLoad));
    assert_eq!(*cx.cues.borrow(), [UiCue::Core(0), UiCue::Core(1)]);
    // A load reads a game's save, which clears Extreme Battle's bits.
    {
        let mut other = extreme_rows(&cx, &dir);
        let mut action = done(other.update(&cx, press("confirm")));
        for _ in 0..300 {
            if action.is_some() {
                break;
            }
            action = done(other.update(&cx, UiInput::default()));
        }
        assert_eq!(action, Some(ScreenAction::SaveLoad));
        other.resume_loaded(&cx, None);
        assert_eq!(other.value("ti_fgsys") & 0x0100_0000, 0);
        assert_eq!(other.value("ti_fgstatus") & 0x4000, 0);
    }
    // The load screen cancelled: Extreme Battle's title again.
    screen.resume(&cx);
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(screen.value("ti_mv"), 5);
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!((screen.value("ti_mv"), screen.value("ti_disp")), (6, 0));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A skip on the tick the 4th Survivor's logo is done runs on past its
/// two-entry table into EX.BATTLE's fade in and rows, still in
/// `title_mv_state` 4, where START starts the 4th Survivor's game.
#[test]
fn a_skip_on_the_4th_survivors_last_logo_tick_runs_into_extreme_battles_rows() {
    let dir = std::env::temp_dir().join(format!("arklay-re2-4th-ex-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cx = context(&dir);
    let mut screen = open_special(&cx, &dir, 15);
    run(&mut screen, &cx, UiInput::default(), 20);
    run(&mut screen, &cx, press("confirm"), 19);
    assert_eq!(screen.value("ti_mv"), 4);
    run(&mut screen, &cx, UiInput::default(), 64 + 128 + 90);
    assert_eq!(screen.value("ti_disp"), 0);
    run(&mut screen, &cx, press("confirm"), 1);
    assert_eq!(screen.value("ti_disp"), 2);
    // EX.BATTLE's fade in, done at once, and the logo's count from what the
    // title's idle count left (17, from the ticks of SPECIAL's fade).
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!((screen.value("ti_mv"), screen.value("ti_disp")), (4, 3));
    let count = screen.value("ti_demo");
    assert_eq!(count, 17);
    run(&mut screen, &cx, UiInput::default(), 18);
    assert_eq!(screen.value("ti_step"), 1);
    // The cursor is the 4th Survivor's START (`Ti_Cur` 0): LOAD GAME here.
    run(&mut screen, &cx, press("down"), 5);
    assert_eq!(screen.value("ti_cur"), 1);
    run(&mut screen, &cx, press("confirm"), 1);
    assert_eq!(screen.value("ti_step"), 2);
    run(&mut screen, &cx, UiInput::default(), 10);
    let mut action = done(screen.update(&cx, press("confirm")));
    for _ in 0..300 {
        if action.is_some() {
            break;
        }
        action = done(screen.update(&cx, UiInput::default()));
    }
    assert_eq!(action, Some(extra_game(0xC)));
    let _ = std::fs::remove_dir_all(&dir);
}

/// SPECIAL with every extra, its cursor taken to GALLERY and confirmed: the
/// tick of the confirm, with the cues it played.
pub(super) fn open_gallery(cx: &UiContext<'_>, dir: &std::path::Path) -> LuaScreen {
    open_gallery_with(cx, dir, None)
}

/// [`open_gallery`], with config.ini's `GALLERY0` to `GALLERY4` at `opened`.
pub(super) fn open_gallery_with(
    cx: &UiContext<'_>,
    dir: &std::path::Path,
    opened: Option<[u32; 5]>,
) -> LuaScreen {
    let mut screen = open_special_with(cx, dir, 15, opened);
    run(&mut screen, cx, UiInput::default(), 20);
    run(&mut screen, cx, press("down"), 10);
    run(&mut screen, cx, press("right"), 10);
    assert_eq!(screen.value("ti_cur"), 4);
    cx.cues.borrow_mut().clear();
    run(&mut screen, cx, press("confirm"), 1);
    screen
}

/// The words the tick drew from the title's sheet, less the licence line,
/// each as `u:v:w:h:x:y:colour`. A word is two sprites, the second a line
/// lower with blend 0: one that isn't is listed as `unpaired`.
pub(super) fn words(screen: &LuaScreen) -> Vec<String> {
    let listed: String = screen.test_eval(
        "local out = {}
        local list = M.state.sprites
        for i, sprite in ipairs(list) do
            if sprite.page == 0 and sprite.blend == 1 then
                local low = list[i + 1]
                local paired = low and low.page == 0 and low.blend == 0 and low.y == sprite.y + 1
                for _, key in ipairs({ 'u', 'v', 'w', 'h', 'x', 'colour' }) do
                    paired = paired and low[key] == sprite[key]
                end
                out[#out + 1] = paired and table.concat({
                    sprite.u, sprite.v, sprite.w, sprite.h, sprite.x, sprite.y, sprite.colour,
                }, ':') or 'unpaired'
            end
        end
        return table.concat(out, ',')",
    );
    listed.split(',').map(str::to_owned).collect()
}

/// GALLERY's page as it draws: LEON, CLAIRE, EXIT and the word GALLERY,
/// with `lit` (0 to 2) at `colour`.
pub(super) fn gallery_words(lit: usize, colour: u8) -> Vec<String> {
    [
        "0:32:48:12:104:151",
        "0:44:56:12:168:151",
        "0:123:256:13:32:173",
        "170:96:74:12:123:132",
    ]
    .iter()
    .enumerate()
    .map(|(k, word)| format!("{word}:{}", if k == lit { colour } else { 0x20 }))
    .collect()
}

/// Several keys pressed on one tick.
pub(super) fn press_all(keys: &[&str]) -> UiInput {
    let mut input = UiInput::default();
    for key in keys {
        let one = press(key);
        input.down |= one.down;
        input.up |= one.up;
        input.left |= one.left;
        input.right |= one.right;
        input.held_down |= one.held_down;
        input.held_up |= one.held_up;
        input.held_left |= one.held_left;
        input.held_right |= one.held_right;
        input.confirm |= one.confirm;
        input.cancel |= one.cancel;
        input.start |= one.start;
    }
    input
}

#[test]
fn gallery_opens_its_characters_choice() {
    let dir = std::env::temp_dir().join(format!("arklay-re2-gallery-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cx = context(&dir);
    let mut screen = open_gallery(&cx, &dir);
    // The confirm's tick: the decide cue, page 7 at step 0 with SPECIAL's
    // cursor and its slide still up.
    assert_eq!(*cx.cues.borrow(), [UiCue::Core(6)]);
    assert_eq!(
        ["ti_menu", "ti_step", "ti_cur", "ti_slide"].map(|key| screen.value(key)),
        [7, 0, 4, 8]
    );
    // Step 0 runs step 1 with it, so the slide's 8 ticks end on the eighth.
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(
        ["ti_step", "ti_cur", "ti_slide", "ti_dim", "ti_b8c"].map(|key| screen.value(key)),
        [1, 0, 1, 0x70, 0x10]
    );
    // The page draws its three words and GALLERY's from the first tick, at
    // their own places and unfaded, and none of the rows.
    assert_eq!(words(&screen), gallery_words(0, 0x20));
    run(&mut screen, &cx, UiInput::default(), 7);
    assert_eq!(
        ["ti_step", "ti_slide", "ti_dim", "ti_b8c"].map(|key| screen.value(key)),
        [2, 8, 0, 0x80]
    );
    // Right and left choose the character, each once; LEON is lit first.
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(words(&screen), gallery_words(0, 0x22));
    for (direction, cues, claire) in [
        ("left", 0, false),
        ("right", 1, true),
        ("right", 0, true),
        ("left", 1, false),
        ("right", 1, true),
    ] {
        cx.cues.borrow_mut().clear();
        run(&mut screen, &cx, press(direction), 5);
        assert_eq!(cx.cues.borrow().len(), cues, "{direction}");
        assert!(cx.cues.borrow().iter().all(|cue| *cue == UiCue::Core(4)));
        assert_eq!(
            screen.test_eval::<bool>("return M.state.claire"),
            claire,
            "{direction}"
        );
        assert_eq!(screen.value("ti_cur"), 0);
    }
    let lit = |screen: &LuaScreen| {
        words(screen)
            .iter()
            .position(|word| !word.ends_with(":32"))
            .unwrap()
    };
    assert_eq!(lit(&screen), 1);
    // Down to EXIT, where the sideways keys do nothing, and back up to the
    // character it left.
    cx.cues.borrow_mut().clear();
    run(&mut screen, &cx, press("down"), 5);
    assert_eq!(screen.value("ti_cur"), 1);
    assert_eq!(lit(&screen), 2);
    run(&mut screen, &cx, press("down"), 5);
    run(&mut screen, &cx, press("left"), 5);
    assert_eq!(*cx.cues.borrow(), [UiCue::Core(4)]);
    assert!(screen.test_eval::<bool>("return M.state.claire"));
    run(&mut screen, &cx, press("up"), 5);
    assert_eq!(screen.value("ti_cur"), 0);
    assert_eq!(lit(&screen), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_gallerys_exit_and_cancel_go_back_to_the_rows_on_original_mode() {
    let dir = std::env::temp_dir().join(format!("arklay-re2-gallery-exit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cx = context(&dir);
    for leave in [&["right", "down", "confirm"][..], &["right", "cancel"][..]] {
        let mut screen = open_gallery(&cx, &dir);
        run(&mut screen, &cx, UiInput::default(), 20);
        for key in &leave[..leave.len() - 1] {
            run(&mut screen, &cx, press(key), 5);
        }
        cx.cues.borrow_mut().clear();
        run(&mut screen, &cx, press(leave[leave.len() - 1]), 1);
        assert_eq!(*cx.cues.borrow(), [UiCue::Core(5)], "{leave:?}");
        assert_eq!(screen.value("ti_step"), 3);
        // The slide's count back down over 8 ticks, the page's words still
        // up, then the rows with the cursor on ORIGINAL MODE.
        run(&mut screen, &cx, UiInput::default(), 7);
        assert_eq!(
            ["ti_menu", "ti_step", "ti_slide", "ti_pulse"].map(|key| screen.value(key)),
            [7, 3, 1, 0]
        );
        let lit = leave.len() - 1;
        assert_eq!(words(&screen), gallery_words(lit, 0x20));
        // The tick the count ends still draws the page; the rows draw from
        // the next.
        run(&mut screen, &cx, UiInput::default(), 1);
        assert_eq!(
            [
                "ti_menu", "ti_step", "ti_cur", "ti_slide", "ti_dim", "ti_b8c"
            ]
            .map(|key| screen.value(key)),
            [0, 0, 1, 0, 0x80, 0],
            "{leave:?}"
        );
        assert_eq!(words(&screen), gallery_words(lit, 0x20));
        run(&mut screen, &cx, UiInput::default(), 1);
        let rows = words(&screen);
        assert_eq!(rows.len(), 5);
        assert!(rows.iter().all(|word| word != "unpaired"));
        assert!(!rows.contains(&gallery_words(3, 0)[3]));
        // SPECIAL again, and GALLERY from it: the page starts over on LEON.
        assert!(screen.test_eval::<bool>("return M.state.claire"));
        run(&mut screen, &cx, press("down"), 5);
        run(&mut screen, &cx, press("down"), 5);
        run(&mut screen, &cx, press("confirm"), 20);
        assert_eq!((screen.value("ti_menu"), screen.value("ti_cur")), (3, 0));
        run(&mut screen, &cx, press("down"), 5);
        run(&mut screen, &cx, press("right"), 5);
        run(&mut screen, &cx, press("confirm"), 10);
        assert_eq!(
            ["ti_menu", "ti_step", "ti_cur"].map(|key| screen.value(key)),
            [7, 2, 0]
        );
        assert!(!screen.test_eval::<bool>("return M.state.claire"));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_gallerys_confirm_fades_out_and_starts_the_gallery() {
    let dir = std::env::temp_dir().join(format!("arklay-re2-gallery-in-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cx = context(&dir);
    let mut screen = open_gallery(&cx, &dir);
    run(&mut screen, &cx, UiInput::default(), 20);
    run(&mut screen, &cx, press("right"), 5);
    cx.cues.borrow_mut().clear();
    run(&mut screen, &cx, press("confirm"), 1);
    assert_eq!(*cx.cues.borrow(), [UiCue::Core(6)]);
    assert_eq!(screen.value("ti_step"), 4);
    let pulse = screen.value("ti_pulse");
    // Slot 2 to black from 0x200 by 0x800 a tick; the pulse holds.
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(screen.value("ti_step"), 5);
    assert_eq!(
        ["ti_fade2_level", "ti_fade2_add", "ti_fade2_mode"].map(|key| screen.value(key)),
        [0x200 + 0x800, 0x800, 2]
    );
    run(&mut screen, &cx, UiInput::default(), 15);
    assert_eq!(screen.value("ti_step"), 5);
    assert_eq!(screen.value("ti_pulse"), pulse);
    let mut faded = Framebuffer::new();
    screen.draw(&cx, &mut faded);
    // The title's picture, nearly black under the fade.
    assert_eq!(faded.rgba[..3], [0xFF - 0xF4; 3]);
    // The tick the fade is done hands off to the gallery: the page back at
    // step 2 with slot 2 off, the character kept.
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(
        [
            "ti_menu",
            "ti_step",
            "ti_cur",
            "ti_fade2_level",
            "ti_fade2_add",
            "ti_fade2_mode"
        ]
        .map(|key| screen.value(key)),
        [7, 2, 0, -1, 0, 2]
    );
    assert!(screen.test_eval::<bool>("return M.state.claire"));
    assert_eq!(cx.cues.borrow().len(), 1);
    // That tick's frame isn't presented: the fade's last one stays up.
    assert_eq!(frame(&screen).as_deref(), Some("held"));
    assert_eq!(words(&screen), gallery_words(1, 0x20 + pulse as u8));
    assert_eq!(
        screen.test_eval::<i64>("return M.state.drawn_fades[2].level"),
        0x200 + 15 * 0x800
    );
    let mut held = Framebuffer::new();
    screen.draw(&cx, &mut held);
    assert_eq!(held.rgba, faded.rgba);
    // The gallery's task starts with it, and the title sleeps under it.
    assert_eq!(screen.value("gal_on"), 1);
    let asleep = ti(&screen);
    run(&mut screen, &cx, UiInput::default(), 5);
    assert_eq!(ti(&screen), asleep);
    assert_eq!(screen.test_eval::<i64>("return #M.state.sprites"), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_gallerys_keys_together_take_the_originals_order() {
    let dir = std::env::temp_dir().join(format!("arklay-re2-gallery-keys-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cx = context(&dir);
    let claire = |screen: &LuaScreen| screen.test_eval::<bool>("return M.state.claire");
    let mut screen = open_gallery(&cx, &dir);
    run(&mut screen, &cx, UiInput::default(), 20);
    // Left and right together change the character once, either way.
    for now in [true, false] {
        cx.cues.borrow_mut().clear();
        run(&mut screen, &cx, press_all(&["left", "right"]), 5);
        assert_eq!(claire(&screen), now);
        assert_eq!(*cx.cues.borrow(), [UiCue::Core(4)]);
    }
    // A move down or up is the tick's only one.
    cx.cues.borrow_mut().clear();
    run(&mut screen, &cx, press_all(&["down", "right"]), 5);
    assert_eq!((screen.value("ti_cur"), claire(&screen)), (1, false));
    run(&mut screen, &cx, press_all(&["up", "right"]), 5);
    assert_eq!((screen.value("ti_cur"), claire(&screen)), (0, false));
    assert_eq!(*cx.cues.borrow(), [UiCue::Core(4), UiCue::Core(4)]);
    // The move comes before the confirm, which then reads EXIT.
    cx.cues.borrow_mut().clear();
    run(&mut screen, &cx, press_all(&["down", "confirm"]), 1);
    assert_eq!(*cx.cues.borrow(), [UiCue::Core(4), UiCue::Core(5)]);
    assert_eq!(screen.value("ti_step"), 3);
    // The cancel wins over the confirm on a character.
    let mut screen = open_gallery(&cx, &dir);
    run(&mut screen, &cx, UiInput::default(), 20);
    cx.cues.borrow_mut().clear();
    run(&mut screen, &cx, press_all(&["cancel", "confirm"]), 1);
    assert_eq!(*cx.cues.borrow(), [UiCue::Core(5)]);
    assert_eq!(screen.value("ti_step"), 3);
    // START confirms as the decide key does.
    let mut screen = open_gallery(&cx, &dir);
    run(&mut screen, &cx, UiInput::default(), 20);
    cx.cues.borrow_mut().clear();
    run(&mut screen, &cx, press("start"), 1);
    assert_eq!(*cx.cues.borrow(), [UiCue::Core(6)]);
    assert_eq!(screen.value("ti_step"), 4);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_special_layout_with_gallery_opens_its_page() {
    let dir = std::env::temp_dir().join(format!("arklay-re2-gallery-rows-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cx = context(&dir);
    // The way to GALLERY from each layout's opening cursor.
    for (special, way) in [
        (1, &[][..]),
        (2, &["right"][..]),
        (5, &["down"][..]),
        (15, &["down", "right"][..]),
    ] {
        let mut screen = open_special(&cx, &dir, special);
        run(&mut screen, &cx, UiInput::default(), 20);
        for key in way {
            run(&mut screen, &cx, press(key), 5);
        }
        assert_eq!(screen.value("ti_cur"), 4, "SPECIAL {special}");
        cx.cues.borrow_mut().clear();
        run(&mut screen, &cx, press("confirm"), 1);
        assert_eq!(*cx.cues.borrow(), [UiCue::Core(6)], "SPECIAL {special}");
        assert_eq!(
            (screen.value("ti_menu"), screen.value("ti_step")),
            (7, 0),
            "SPECIAL {special}"
        );
        run(&mut screen, &cx, UiInput::default(), 9);
        assert_eq!(words(&screen), gallery_words(0, 0x22), "SPECIAL {special}");
    }
    // With no extras EXIT is all there is.
    let mut screen = open_special(&cx, &dir, 0);
    run(&mut screen, &cx, UiInput::default(), 20);
    run(&mut screen, &cx, press("confirm"), 1);
    assert_eq!((screen.value("ti_menu"), screen.value("ti_step")), (3, 3));
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
pub(super) fn frame(screen: &LuaScreen) -> Option<String> {
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

/// The option menu's rows move on the keys' presses alone and stop at both
/// ends.
#[test]
fn the_option_menu_moves_between_its_four_rows() {
    let dir = std::env::temp_dir();
    let cx = context(&dir);
    let (mut screen, dir) = open_option(&cx, "option-menu");
    assert!(tick_cues(&mut screen, &cx, press("up")).is_empty());
    assert_eq!(screen.value("opt_cursor"), 0);
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

/// The trace's `kc`, KEYCONFIG's page: state 9's count, the panel's offset
/// and the choice, then the keys being edited.
fn kc(screen: &LuaScreen) -> (String, String) {
    let field = screen.state_str("kc").expect("the option task runs");
    let (page, keys) = field.split_once('/').unwrap();
    (page.to_owned(), keys.to_owned())
}

/// The words the tick's list printed, each with whether it is lit.
fn key_words(screen: &LuaScreen) -> Vec<(String, bool)> {
    let listed: String = screen.test_eval(
        "local out = {}
        for _, t in ipairs(M.state.opt_texts) do
            if not t.key then out[#out + 1] = t.text .. '=' .. (t.on and 1 or 0) end
        end
        return table.concat(out, ',')",
    );
    listed
        .split(',')
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            let (word, on) = entry.split_once('=').unwrap();
            (word.to_owned(), on == "1")
        })
        .collect()
}

/// The keys the tick's list drew, a row each: a letter, or `u:v` of the
/// sheet's glyph, then `r` for the changed key's red or `g` for the lit one.
fn key_glyphs(screen: &LuaScreen) -> Vec<String> {
    let listed: String = screen.test_eval(
        "local out = {}
        for _, t in ipairs(M.state.opt_texts) do
            if t.key then out[#out + 1] = t.y .. ' ' .. t.text .. (t.red and 'r' or t.on and 'g' or '') end
        end
        for _, sprite in ipairs(M.state.sprites) do
            if sprite.page == 5 and sprite.x > 0x9A then
                local colour = sprite.rgb[1] == 0x80 and sprite.rgb[2] == 0x10 and 'r' or sprite.rgb[1] == 0 and 'g' or ''
                out[#out + 1] = sprite.y .. ' ' .. sprite.u .. ':' .. sprite.v .. colour
            end
        end
        table.sort(out, function(a, b) return tonumber(a:match('^%d+')) < tonumber(b:match('^%d+')) end)
        for i, entry in ipairs(out) do out[i] = entry:match(' (.+)$') end
        return table.concat(out, ',')",
    );
    listed
        .split(',')
        .filter(|entry| !entry.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The pad's glyphs the tick's list drew, down the rows: each one's `u:v`.
fn pad_glyphs(screen: &LuaScreen) -> Vec<(i64, i64)> {
    let listed: String = screen.test_eval(
        "local out = {}
        for _, sprite in ipairs(M.state.sprites) do
            if sprite.page == 5 and sprite.x == 0x8C then out[#out + 1] = sprite.u .. ':' .. sprite.v end
        end
        return table.concat(out, ',')",
    );
    listed
        .split(',')
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            let (u, v) = entry.split_once(':').unwrap();
            (u.parse().unwrap(), v.parse().unwrap())
        })
        .collect()
}

/// The executable's own keys (`FUN_0044da40`): the number pad's 8, 2, 4 and
/// 6, then V for the run, A for the map, and X, C, Ctrl and Z.
const DEFAULT_KEYS: &str = "686264660056004100005843115a0000000000000000000000262825270d201b";

/// The option menu's KEYCONFIG decided, to the page's list with its cursor
/// (state 0xB), on the decide key through the walk in and the rows.
fn open_key_page(screen: &mut LuaScreen, cx: &UiContext<'_>) {
    run(screen, cx, press("confirm"), 2);
    assert_eq!(opt(screen)[1], 9);
    run(screen, cx, press("confirm"), 2);
    assert_eq!(opt(screen)[1], 10);
    run(screen, cx, press("confirm"), 2);
    assert_eq!(opt(screen)[1..4], [0xB, 0, 4]);
}

/// KEYCONFIG's decide opens its page: a tick in state 2 that copies the
/// keys, state 9's 30 ticks with the panel alone, state 10's 54 with a row
/// more every five, then the list with its cursor on Get Ready
/// (`title_option_key`'s 440 to 526).
#[test]
fn keyconfig_opens_its_page_a_row_at_a_time() {
    let dir = std::env::temp_dir();
    let cx = context(&dir);
    let (mut screen, dir) = open_option(&cx, "key-open");
    assert_eq!(kc(&screen), ("0,0,0".to_owned(), DEFAULT_KEYS.to_owned()));
    assert_eq!(
        tick_cues(&mut screen, &cx, press("confirm")),
        [UiCue::Core(6)]
    );
    assert_eq!(opt(&screen)[1], 2);
    assert_eq!(
        tick_cues(&mut screen, &cx, UiInput::default()),
        [UiCue::Core(8)]
    );
    assert_eq!((opt(&screen)[1], kc(&screen).0.as_str()), (9, "30,0,0"));
    let panel: String = screen.test_eval(
        "local p = M.state.key_panel return p.x .. ',' .. p.y .. ',' .. p.w .. ',' .. p.h",
    );
    assert_eq!(panel, "136,17,177,209");
    assert!(key_words(&screen).is_empty(), "the panel alone");
    run(&mut screen, &cx, UiInput::default(), 30);
    assert_eq!((opt(&screen)[1], kc(&screen).0.as_str()), (9, "0,0,0"));
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(
        (&opt(&screen)[1..4], kc(&screen).0.as_str()),
        (&[10, 0, 54][..], "-1,0,0")
    );
    assert_eq!(key_words(&screen), [("Forward".to_owned(), false)]);
    run(&mut screen, &cx, UiInput::default(), 4);
    assert_eq!(key_words(&screen).len(), 1, "type 50");
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(key_words(&screen).len(), 2, "type 49");
    run(&mut screen, &cx, UiInput::default(), 45);
    assert_eq!(opt(&screen)[3], 4);
    let words = key_words(&screen);
    assert_eq!(words.len(), 12, "every row, the last one's two words");
    assert!(words.iter().all(|(_, on)| !on), "no cursor yet");
    assert_eq!(
        words[10..],
        [("Exit".to_owned(), false), ("Default".to_owned(), false)]
    );
    run(&mut screen, &cx, UiInput::default(), 4);
    assert_eq!(
        (&opt(&screen)[1..4], kc(&screen).0.as_str()),
        (&[0xB, 0, 4][..], "0,0,0")
    );
    let lit: Vec<_> = key_words(&screen)
        .into_iter()
        .filter(|(_, on)| *on)
        .collect();
    assert_eq!(lit, [("Get Ready".to_owned(), true)]);
    // The default keys: the number pad's digits from the sheet, the letters
    // from the font, Ctrl from the sheet, and the cursor's lit.
    assert_eq!(
        key_glyphs(&screen),
        [
            "230:44", "230:16", "174:44", "202:44", "Xg", "C", "V", "Z", "8:176", "A"
        ]
    );
    // The default pad: the directions, then X, A, B, Y, and the view and
    // the menu buttons, the cursor's from the lit glyphs.
    assert_eq!(
        pad_glyphs(&screen),
        [
            (188, 72),
            (146, 30),
            (174, 72),
            (202, 72),
            (41, 241),
            (160, 30),
            (174, 30),
            (202, 30),
            (230, 30),
            (216, 30)
        ]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The list's cursor moves on the directions' presses and wraps at both
/// ends; the sideways keys change the choice on the last row alone; the
/// pad's other keys do nothing off the last row; and Exit's decide with
/// nothing changed leaves over state 0xD's five ticks, which draw neither
/// the panel nor the list (`title_option_key`'s 540 to 865).
#[test]
fn keyconfigs_cursor_wraps_and_exit_leaves() {
    let dir = std::env::temp_dir();
    let cx = context(&dir);
    let (mut screen, dir) = open_option(&cx, "key-rows");
    open_key_page(&mut screen, &cx);
    assert_eq!(tick_cues(&mut screen, &cx, press("down")), [UiCue::Core(4)]);
    assert_eq!(opt(&screen)[3], 5);
    let held = UiInput {
        held_down: true,
        ..UiInput::default()
    };
    for _ in 0..30 {
        assert!(tick_cues(&mut screen, &cx, held).is_empty());
    }
    assert_eq!(opt(&screen)[3], 5, "a held key moves one row");
    for key in ["left", "right", "confirm", "cancel", "start"] {
        run(&mut screen, &cx, UiInput::default(), 1);
        assert!(tick_cues(&mut screen, &cx, press(key)).is_empty(), "{key}");
        assert_eq!(
            (&opt(&screen)[1..4], kc(&screen).0.as_str()),
            (&[0xB, 0, 5][..], "0,0,0")
        );
    }
    for row in (0..5).rev() {
        run(&mut screen, &cx, UiInput::default(), 1);
        run(&mut screen, &cx, press("up"), 1);
        assert_eq!(opt(&screen)[3], row);
    }
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(tick_cues(&mut screen, &cx, press("up")), [UiCue::Core(4)]);
    assert_eq!(opt(&screen)[3], 10, "up from the first row");
    assert_eq!(
        key_words(&screen)[10..],
        [("Exit".to_owned(), true), ("Default".to_owned(), false)]
    );
    run(&mut screen, &cx, UiInput::default(), 1);
    run(&mut screen, &cx, press("down"), 2);
    assert_eq!(opt(&screen)[3], 0, "down from the last");
    run(&mut screen, &cx, press("up"), 2);
    for (key, choice) in [
        ("right", "0,0,1"),
        ("right", "0,0,0"),
        ("left", "0,0,1"),
        ("left", "0,0,0"),
    ] {
        assert_eq!(tick_cues(&mut screen, &cx, press(key)), [UiCue::Core(4)]);
        assert_eq!(kc(&screen).0, choice);
        let lit = usize::from(choice == "0,0,1");
        assert!(key_words(&screen)[10 + lit].1 && !key_words(&screen)[11 - lit].1);
        run(&mut screen, &cx, UiInput::default(), 1);
    }
    assert_eq!(
        tick_cues(&mut screen, &cx, press("confirm")),
        [UiCue::Core(6)]
    );
    assert_eq!(opt(&screen)[1..5], [0xD, 0, 10, 5]);
    let drawn: bool = screen.test_eval("return M.state.key_panel ~= nil or #M.state.opt_texts > 0");
    assert!(!drawn, "state 0xD draws the room alone");
    run(&mut screen, &cx, UiInput::default(), 4);
    assert_eq!(opt(&screen)[1..5], [0xD, 0, 10, 1]);
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(opt(&screen)[1..5], [1, 0, 0, 0], "the menu, on KEYCONFIG");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Default's decide sets the executable's own keys (state 0xE, a tick) and
/// leaves the cursor on Default; Exit's decide with the keys changed asks
/// (state 0xF), where the sideways keys choose and the decide key or the
/// cancel key answers: Leave keeps the settings, Change takes the edited
/// keys, and a changed key or button is red until then
/// (`title_option_key_default`).
#[test]
fn keyconfigs_default_asks_before_it_changes_the_keys() {
    let dir = std::env::temp_dir();
    let mut cx = context(&dir);
    let statics = crate::ui::ScreenStatics::default();
    cx.statics = Some(&statics);
    let own = crate::player::TitleKeys::parse(RECORDED_KEYS.0, RECORDED_KEYS.1).unwrap();
    let (key_def, joy_def) = own.definitions();
    let folder =
        std::env::temp_dir().join(format!("arklay-re2-key-default-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let mut cx_open = context(&folder);
    cx_open.statics = cx.statics;
    let mut screen = LuaScreen::open_screen(
        "title",
        &[
            ("key_def", ScreenArg::Str(key_def)),
            ("joy_def", ScreenArg::Str(joy_def)),
        ],
        ScreenAction::Quit,
        &mut cx_open,
    )
    .unwrap();
    run(&mut screen, &cx, UiInput::default(), 70);
    for _ in 0..4 {
        run(&mut screen, &cx, press("down"), 20);
    }
    run(&mut screen, &cx, press("confirm"), 16);
    assert_eq!(opt(&screen)[1], 1);
    // Before the page first opens the edited keys are the defaults.
    assert_eq!(kc(&screen).1, DEFAULT_KEYS);
    open_key_page(&mut screen, &cx);
    let recorded = "68626466004d004900004f4a5a4b0000000000000000000000262825270d201b";
    assert_eq!(kc(&screen).1, recorded);
    assert_eq!(
        key_glyphs(&screen),
        [
            "230:44", "230:16", "174:44", "202:44", "Og", "J", "M", "K", "Z", "I"
        ]
    );
    // The recorded pad: the right trigger, A, X, B, the menu and the view.
    assert_eq!(
        pad_glyphs(&screen)[4..],
        [
            (160, 183),
            (160, 30),
            (188, 30),
            (174, 30),
            (216, 30),
            (230, 30)
        ]
    );
    for _ in 0..6 {
        run(&mut screen, &cx, press("down"), 2);
    }
    assert_eq!(opt(&screen)[3], 10);
    run(&mut screen, &cx, press("right"), 2);
    assert_eq!(
        tick_cues(&mut screen, &cx, press("confirm")),
        [UiCue::Core(6)]
    );
    assert_eq!(
        (opt(&screen)[1], kc(&screen)),
        (0xE, ("0,0,1".to_owned(), recorded.to_owned()))
    );
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(
        (opt(&screen)[1], kc(&screen)),
        (0xB, ("0,0,1".to_owned(), DEFAULT_KEYS.to_owned()))
    );
    // The six changed keys are red, and the pad's changed buttons: X, B, Y,
    // the view and the menu, with A as it was.
    assert_eq!(
        key_glyphs(&screen),
        [
            "230:44", "230:16", "174:44", "202:44", "Xr", "Cr", "Vr", "Zr", "8:176r", "Ar"
        ]
    );
    assert_eq!(
        pad_glyphs(&screen)[4..],
        [
            (188, 114),
            (160, 30),
            (174, 114),
            (202, 114),
            (230, 114),
            (216, 114)
        ]
    );
    run(&mut screen, &cx, press("left"), 2);
    assert_eq!(
        tick_cues(&mut screen, &cx, press("confirm")),
        [UiCue::Core(6)]
    );
    assert_eq!(opt(&screen)[1], 0xF);
    assert_eq!(
        key_words(&screen)[10..],
        [("Change".to_owned(), true), ("Leave".to_owned(), false)]
    );
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(
        tick_cues(&mut screen, &cx, press("right")),
        [UiCue::Core(4)]
    );
    assert_eq!(
        key_words(&screen)[10..],
        [("Change".to_owned(), false), ("Leave".to_owned(), true)]
    );
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(
        tick_cues(&mut screen, &cx, press("confirm")),
        [UiCue::Core(6)]
    );
    assert_eq!(opt(&screen)[1..5], [0xD, 0, 10, 5]);
    run(&mut screen, &cx, UiInput::default(), 5);
    assert_eq!(
        (opt(&screen)[1], kc(&screen).0.as_str()),
        (1, "0,0,1"),
        "the choice stands"
    );
    // Leave kept the settings: the page opens with them again, choice 0.
    run(&mut screen, &cx, press("confirm"), 2);
    assert_eq!(kc(&screen), ("30,0,0".to_owned(), recorded.to_owned()));
    run(&mut screen, &cx, press("confirm"), 2);
    run(&mut screen, &cx, press("confirm"), 2);
    for _ in 0..5 {
        run(&mut screen, &cx, press("up"), 2);
    }
    run(&mut screen, &cx, press("right"), 2);
    run(&mut screen, &cx, press("confirm"), 2);
    run(&mut screen, &cx, press("left"), 2);
    run(&mut screen, &cx, press("confirm"), 2);
    assert_eq!(opt(&screen)[1], 0xF);
    // The cancel key answers too, with Change.
    let cancel = UiInput {
        cancel: true,
        ..UiInput::default()
    };
    assert_eq!(tick_cues(&mut screen, &cx, cancel), [UiCue::Core(6)]);
    run(&mut screen, &cx, UiInput::default(), 5);
    assert_eq!(opt(&screen)[1], 1);
    open_key_page(&mut screen, &cx);
    assert_eq!(kc(&screen).1, DEFAULT_KEYS);
    assert_eq!(
        key_glyphs(&screen),
        [
            "230:44", "230:16", "174:44", "202:44", "Xg", "C", "V", "Z", "8:176", "A"
        ],
        "the settings are the defaults now: nothing is red"
    );
    let _ = std::fs::remove_dir_all(&folder);
    // A title opened anew finds the keys the last one changed, not the
    // boot's.
    let mut cx_open = context(&dir);
    cx_open.statics = cx.statics;
    let (key_def, joy_def) = own.definitions();
    let mut again = LuaScreen::open_screen(
        "title",
        &[
            ("key_def", ScreenArg::Str(key_def)),
            ("joy_def", ScreenArg::Str(joy_def)),
        ],
        ScreenAction::Quit,
        &mut cx_open,
    )
    .unwrap();
    run(&mut again, &cx, UiInput::default(), 70);
    for _ in 0..4 {
        run(&mut again, &cx, press("down"), 20);
    }
    run(&mut again, &cx, press("confirm"), 16);
    open_key_page(&mut again, &cx);
    assert_eq!(kc(&again).1, DEFAULT_KEYS);
}

/// config.ini's `KeyDef` and `JoyDef_Emulated` in the install the
/// recordings were made from.
const RECORDED_KEYS: (&str, &str) = (
    "68 62 64 66 00 4D 00 49 00 00 4F 4A 5A 4B 00 00 00 00 00 00 00 00 00 00 00 26 28 25 27 0D 20 1B",
    "00 10 00 00 00 40 00 00 00 80 00 00 00 20 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 \
     80 00 00 00 00 08 00 00 44 00 00 00 00 00 00 00 00 01 00 00 10 00 00 00 02 00 00 00 00 00 00 00 \
     00 00 00 00 08 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 \
     00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00",
);

/// A direction held as the list comes is a press on the list's first tick:
/// state 10 clears the page's own history of the directions (`0x6893D0`),
/// which the title's edges don't share, and the held key moves one row.
#[test]
fn a_direction_held_into_keyconfigs_list_moves_once() {
    let dir = std::env::temp_dir();
    let cx = context(&dir);
    let (mut screen, dir) = open_option(&cx, "key-held");
    run(&mut screen, &cx, press("confirm"), 2);
    run(&mut screen, &cx, press("confirm"), 2);
    assert_eq!(opt(&screen)[1], 10);
    let held = UiInput {
        held_down: true,
        ..UiInput::default()
    };
    assert!(tick_cues(&mut screen, &cx, press("down")).is_empty());
    for _ in 0..10 {
        assert!(tick_cues(&mut screen, &cx, held).is_empty());
    }
    let confirm = UiInput {
        confirm: true,
        held_down: true,
        ..UiInput::default()
    };
    run(&mut screen, &cx, confirm, 1);
    assert_eq!(opt(&screen)[1..4], [0xB, 0, 4]);
    assert_eq!(tick_cues(&mut screen, &cx, held), [UiCue::Core(4)]);
    assert_eq!(opt(&screen)[3], 5);
    for _ in 0..30 {
        assert!(tick_cues(&mut screen, &cx, held).is_empty());
    }
    assert_eq!(
        opt(&screen)[3],
        5,
        "then the held key waits for its release"
    );
    // A tap let go within the tick has no held bit, and still moves.
    let tap = |key: &str| {
        let mut input = UiInput {
            held_down: true,
            ..UiInput::default()
        };
        match key {
            "down" => input.down = true,
            _ => input.up = true,
        }
        input
    };
    let released = UiInput {
        down: true,
        ..UiInput::default()
    };
    assert_eq!(tick_cues(&mut screen, &cx, released), [UiCue::Core(4)]);
    assert_eq!(opt(&screen)[3], 6, "a tap within the tick");
    // Down held again is a press again; then up taps to the last row with
    // it held, and Exit with the history left holding down.
    run(&mut screen, &cx, held, 1);
    assert_eq!(opt(&screen)[3], 7);
    for _ in 0..8 {
        run(&mut screen, &cx, tap("up"), 1);
        run(&mut screen, &cx, held, 1);
    }
    assert_eq!(opt(&screen)[3], 10);
    let held_confirm = UiInput {
        confirm: true,
        held_down: true,
        ..UiInput::default()
    };
    run(&mut screen, &cx, held_confirm, 1);
    run(&mut screen, &cx, held, 5);
    assert_eq!(opt(&screen)[1..3], [1, 0], "the menu, on KEYCONFIG");
    // The next visit's state 10 clears that history: down, held again from
    // the walk in (the menu repeats a held key), moves the list's cursor on
    // its first tick.
    run(&mut screen, &cx, UiInput::default(), 1);
    run(&mut screen, &cx, press("confirm"), 2);
    assert_eq!(opt(&screen)[1], 9);
    run(&mut screen, &cx, held_confirm, 1);
    run(&mut screen, &cx, held, 1);
    assert_eq!(opt(&screen)[1], 10);
    run(&mut screen, &cx, held_confirm, 1);
    assert_eq!(opt(&screen)[1..4], [0xB, 0, 4]);
    assert_eq!(tick_cues(&mut screen, &cx, held), [UiCue::Core(4)]);
    assert_eq!(opt(&screen)[3], 5);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The executable's own pad (`FUN_0044dab0`), a word for each button.
fn default_pad() -> [u32; 32] {
    let mut pad = [0u32; 32];
    pad[..4].copy_from_slice(&[0x1000, 0x4000, 0x8000, 0x2000]);
    pad[8..15].copy_from_slice(&[0x80, 0x44, 0x08, 0x800, 0x10, 0x100, 0x02]);
    pad
}

/// The executable's own pad as config.ini's `JoyDef`, with `extra` in the
/// 16th word.
fn default_pad_with(extra: u32) -> String {
    let mut pad = default_pad();
    pad[16] = extra;
    joy_def(&pad)
}

/// A pad's words as config.ini's `JoyDef`.
fn joy_def(pad: &[u32; 32]) -> String {
    pad.iter()
        .flat_map(|word| word.to_le_bytes())
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The executable's own keys as config.ini's `KeyDef`.
fn default_key_def() -> String {
    (0..32)
        .map(|i| &DEFAULT_KEYS[i * 2..i * 2 + 2])
        .collect::<Vec<_>>()
        .join(" ")
}

/// The pad's words are compared whole: a setting that differs from Default's
/// only in a word's top half still asks on Exit, Leave keeps it, Change
/// takes Default's, and a title opened anew finds what the last one took.
#[test]
fn keyconfig_asks_for_a_change_to_the_pad_alone() {
    let dir = std::env::temp_dir();
    let mut cx = context(&dir);
    let statics = crate::ui::ScreenStatics::default();
    cx.statics = Some(&statics);
    let opened = |joy_def: String| {
        let mut cx_open = context(&dir);
        cx_open.statics = Some(&statics);
        let mut screen = LuaScreen::open_screen(
            "title",
            &[
                ("key_def", ScreenArg::Str(default_key_def())),
                ("joy_def", ScreenArg::Str(joy_def)),
            ],
            ScreenAction::Quit,
            &mut cx_open,
        )
        .unwrap();
        run(&mut screen, &cx, UiInput::default(), 70);
        for _ in 0..4 {
            run(&mut screen, &cx, press("down"), 20);
        }
        run(&mut screen, &cx, press("confirm"), 16);
        assert_eq!(opt(&screen)[1], 1);
        screen
    };
    let mut screen = opened(default_pad_with(0x0001_0000));
    // Default, then Exit: the pad's 16th word differs in its top half.
    let default_then_exit = |screen: &mut LuaScreen| {
        open_key_page(screen, &cx);
        for _ in 0..5 {
            run(screen, &cx, press("up"), 2);
        }
        assert_eq!(opt(screen)[3], 10);
        run(screen, &cx, press("right"), 2);
        run(screen, &cx, press("confirm"), 2);
        assert_eq!(opt(screen)[1], 0xB);
        run(screen, &cx, press("left"), 2);
        run(screen, &cx, press("confirm"), 1);
    };
    default_then_exit(&mut screen);
    assert_eq!(opt(&screen)[1], 0xF, "the changed pad asks");
    run(&mut screen, &cx, UiInput::default(), 1);
    run(&mut screen, &cx, press("right"), 2);
    run(&mut screen, &cx, press("confirm"), 6);
    assert_eq!(opt(&screen)[1], 1, "Leave");
    default_then_exit(&mut screen);
    assert_eq!(opt(&screen)[1], 0xF, "Leave kept the pad as it was");
    run(&mut screen, &cx, press("confirm"), 6);
    assert_eq!(opt(&screen)[1], 1, "Change");
    // Default and Exit leave at once: the settings are Default's now.
    default_then_exit(&mut screen);
    assert_eq!(opt(&screen)[1], 0xD, "Change took Default's pad");
    // A title opened anew with the boot's settings keeps the last one's.
    let mut again = opened(default_pad_with(0x0001_0000));
    default_then_exit(&mut again);
    assert_eq!(opt(&again)[1], 0xD, "the kept pad is Default's");
}

/// The arrow keys are the sheet's thin arrows, and `[`, `]`, `_` and `\`
/// the font's own cells (`Config_draw_key`).
#[test]
fn keyconfig_draws_the_arrows_and_the_fonts_signs() {
    let dir = std::env::temp_dir();
    let mut cx = context(&dir);
    let statics = crate::ui::ScreenStatics::default();
    cx.statics = Some(&statics);
    let mut keys: Vec<String> = default_key_def().split(' ').map(str::to_owned).collect();
    for (row, key) in [
        (0, "25"),
        (1, "DB"),
        (2, "BF"),
        (3, "28"),
        (10, "DD"),
        (11, "BD"),
    ] {
        keys[row] = key.to_owned();
    }
    // Get Ready's word on button 4, which Classic REbirth's table has no
    // place for: its zeros, the sheet's tile at 0, 0.
    let mut pad = default_pad();
    (pad[4], pad[10]) = (pad[10], 0);
    let mut cx_open = context(&dir);
    cx_open.statics = Some(&statics);
    let mut screen = LuaScreen::open_screen(
        "title",
        &[
            ("key_def", ScreenArg::Str(keys.join(" "))),
            ("joy_def", ScreenArg::Str(joy_def(&pad))),
        ],
        ScreenAction::Quit,
        &mut cx_open,
    )
    .unwrap();
    run(&mut screen, &cx, UiInput::default(), 70);
    for _ in 0..4 {
        run(&mut screen, &cx, press("down"), 20);
    }
    run(&mut screen, &cx, press("confirm"), 16);
    open_key_page(&mut screen, &cx);
    assert_eq!(
        key_glyphs(&screen)[..6],
        ["134:16", "[", "\\", "176:16", "]g", "_"]
    );
    assert_eq!(pad_glyphs(&screen)[4], (0, 0));
}

/// Each visit to the option screen draws once from the title's generator
/// for the player KEYCONFIG shows (`FUN_0044b100`): from 0x7FFFFFFF, Leon
/// and then Claire.
#[test]
fn each_option_visit_picks_its_player() {
    let dir = std::env::temp_dir();
    let statics = crate::ui::ScreenStatics::default();
    let (mut screen, cx) = open_kept(&statics, &dir, &[("seed", ScreenArg::Int(0x7FFF_FFFF))]);
    let mut models = Vec::new();
    // The generator's word after each visit: one draw apiece, the first
    // the seed itself.
    for word in [0x7FFF_FFFF, 0x9957_3BA2] {
        run(&mut screen, &cx, UiInput::default(), 70);
        for _ in 0..8 {
            if screen.value("ti_cur") == 4 {
                break;
            }
            run(&mut screen, &cx, press("down"), 20);
        }
        assert_eq!(screen.value("ti_cur"), 4, "OPTION's row");
        run(&mut screen, &cx, press("confirm"), 16);
        assert_eq!(opt(&screen)[1], 1);
        models.push(screen.test_eval::<i64>("return M.state.opt.model"));
        assert_eq!(statics.get("ui/title.lua", "rnd"), Some(word));
        run(&mut screen, &cx, press("cancel"), 1);
        run(&mut screen, &cx, UiInput::default(), 40);
        assert_eq!(screen.value("opt_on"), 0);
    }
    assert_eq!(models, [0, 1]);
}

/// A key's letter is printed with its whole font cell at the original's
/// place (`Config_draw_key` at 0x9C, `title_option_key`'s frames): the print
/// draws a glyph's cell its indent left of the cursor, so the cursor is the
/// place and the indent.
#[test]
fn a_keys_letter_sits_where_the_original_draws_its_cell() {
    let dir = std::env::temp_dir();
    let cx = context(&dir);
    let (screen, dir) = open_option(&cx, "key-letter");
    let place = |indent: i64| -> i64 {
        screen.test_eval(&format!("return M.option_key_x(0x9C, {indent})"))
    };
    // `width_tbl`'s O, J, M and I (bearings 1, 1, 0 and 4), as the
    // recordings place them, and Q, which fills its cell from the left.
    assert_eq!(
        [place(1), place(1), place(0), place(4), place(0)],
        [157, 157, 156, 160, 156]
    );
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
/// glyph (`title_option_sound`'s frames): the print draws a digit's cell a
/// pixel before its place, as the font's table has every digit, 12 wide.
/// Over a font of ink widths it is in the middle of the cell, but the 6, a
/// pixel left of it.
#[test]
fn a_volumes_digit_sits_where_its_cell_puts_it() {
    let dir = std::env::temp_dir();
    let cx = context(&dir);
    let (screen, dir) = open_option(&cx, "option-digit");
    let place = |digit: &str, width: i64, by_table: bool| -> i64 {
        screen.test_eval(&format!(
            "return M.option_digit_x(0x100, '{digit}', {width}, {by_table})"
        ))
    };
    for digit in ["0", "1", "6", "9"] {
        assert_eq!(place(digit, 12, true), 0x101, "{digit}");
    }
    assert_eq!(place("0", 9, false), 0x103);
    assert_eq!(place("1", 6, false), 0x104);
    assert_eq!(place("6", 9, false), 0x102);
    // A mod's font may make its 6 as wide as the table's without being it.
    assert_eq!(place("6", 12, false), 0x100);
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
            ScreenResult::Done(ScreenAction::Film {
                id,
                claire,
                gallery: false,
            }) => {
                films.push((tick, id, claire));
                screen.resume_film(&cx, skipped_film == Some(id), true);
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
                // The task is started, and a trace has it from its first
                // tick.
                assert_eq!(screen.value("pro_on"), 0);
            }
            693 => {
                assert_eq!(ti(screen)[..4], [2, 3, 0, 3]);
                assert_eq!(screen.value("pro_on"), 1);
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

/// A title over the prologue's art, through LOAD GAME (`title_load_b`'s
/// confirms on 260 and, under the load screen, 800) to a save at `place`
/// whose `fg_status` is `status`, then `presses`, until it hands the game
/// over. Films are over at once, a press ending none.
fn run_load(
    dir: &std::path::Path,
    pack: &'static Pack,
    place: u8,
    status: u32,
    presses: &[(u32, UiInput)],
    mut each: impl FnMut(u32, &LuaScreen, &[UiCue]),
) -> Prologue {
    let cx = context_over(pack, dir);
    let mut cx_open = context_over(pack, dir);
    let mut screen =
        LuaScreen::open_screen("title", &[], ScreenAction::Quit, &mut cx_open).unwrap();
    let mut films = Vec::new();
    for tick in 189..9000 {
        let input = if tick == 260 {
            confirm()
        } else {
            presses
                .iter()
                .find(|(at, _)| *at == tick)
                .map_or_else(UiInput::default, |(_, input)| *input)
        };
        cx.cues.borrow_mut().clear();
        let result = screen.update(&cx, input);
        each(tick, &screen, &cx.cues.borrow());
        match result {
            ScreenResult::Continue => {}
            ScreenResult::Done(ScreenAction::SaveLoad) => {
                // `title_load_b`'s load screen chooses on 800 and the title
                // wakes on 809.
                assert_eq!(tick, 531);
                screen.resume_loaded(&cx, Some((place, status)));
                return run_from(screen, &cx, 810, presses, &mut films, each);
            }
            ScreenResult::Done(action) => panic!("{tick}: {action:?}"),
        }
    }
    panic!("the title never opened the load screen");
}

fn run_from(
    mut screen: LuaScreen,
    cx: &UiContext<'_>,
    from: u32,
    presses: &[(u32, UiInput)],
    films: &mut Vec<(u32, u8, bool)>,
    mut each: impl FnMut(u32, &LuaScreen, &[UiCue]),
) -> Prologue {
    for tick in from..9000 {
        let input = presses
            .iter()
            .find(|(at, _)| *at == tick)
            .map_or_else(UiInput::default, |(_, input)| *input);
        cx.cues.borrow_mut().clear();
        let result = screen.update(cx, input);
        each(tick, &screen, &cx.cues.borrow());
        match result {
            ScreenResult::Continue => {}
            ScreenResult::Done(ScreenAction::Film {
                id,
                claire,
                gallery: false,
            }) => {
                films.push((tick, id, claire));
                screen.resume_film(cx, false, true);
            }
            ScreenResult::Done(action) => {
                return Prologue {
                    end: tick,
                    action,
                    films: std::mem::take(films),
                };
            }
        }
    }
    panic!("the title never handed over");
}

/// `title_load_b`: the save a Leon A clear writes is Claire B's start
/// (place 0x10, `fg_status`'s bits 31 and 30), and `Prologue_load` takes
/// it for `Prologue_mv`'s entry 3, `Prologue_mv02`: the task's first tick
/// is 813, its narration's lines are 6 to 14 in Claire's bank, its music
/// main 0x2C, and the films 3 and 0x10 before the game, which then starts
/// as after a new game's prologue.
#[test]
fn a_load_of_claire_bs_start_runs_her_second_scenarios_prologue() {
    let dir = prologue_dir("pro-claire-b");
    let mut voices = Vec::new();
    let mut music = Vec::new();
    let run = run_load(
        &dir,
        prologue_pack(),
        0x10,
        0xC000_0000,
        &[],
        |tick, screen, cues| {
            match tick {
                811 => assert_eq!(screen.value("pro_on"), 0),
                813 => {
                    assert_eq!(pro(screen), [3, 1, 1, 1, 0]);
                    assert_eq!(screen.value("pro_sprites"), 18);
                }
                1336 => assert_eq!(pro(screen)[..3], [3, 2, 1]),
                3943 => assert_eq!(pro(screen)[..3], [3, 9, 1]),
                _ => {}
            }
            for cue in cues {
                match cue {
                    UiCue::Voice { player, line } => voices.push((tick, *player, *line)),
                    UiCue::Music(_) | UiCue::MusicFade => music.push((tick, *cue)),
                    _ => {}
                }
            }
        },
    );
    assert_eq!(
        voices,
        [
            (813 + 240, 1, 6),
            (1336 + 1, 1, 7),
            (1774 + 10, 1, 8),
            (1914 + 1, 1, 9),
            (2236 + 128, 1, 14),
            (2573 + 90, 1, 10),
            (2909 + 1, 1, 11),
            (3175 + 1, 1, 12),
            (3943 + 1, 1, 13),
        ]
    );
    assert_eq!(
        music,
        [(813, UiCue::Music(0x2C)), (3943 + 595, UiCue::MusicFade)]
    );
    let [(film, 3, true), (second, 0x10, true)] = run.films[..] else {
        panic!("{:?}", run.films);
    };
    assert_eq!(second, film + 1);
    assert_eq!(run.end, second + 2);
    assert_eq!(run.action, ScreenAction::Continue { prologue: true });
    let _ = std::fs::remove_dir_all(&dir);
}

/// Leon B's start (place 0x10, `fg_status`'s bit 30) is entry 2,
/// `Prologue_mv01`, with lines 6 to 12 in Leon's bank; a skip takes it to
/// its films' step 7. A save at any other place has no prologue, and one
/// at 0xF (a scenario A's start) runs the first scenarios'.
#[test]
fn a_load_picks_its_prologue_by_the_saves_place() {
    let dir = prologue_dir("pro-load");
    let mut voices = Vec::new();
    let presses = [(1500, confirm())];
    let run = run_load(
        &dir,
        prologue_pack(),
        0x10,
        0x4000_0000,
        &presses,
        |tick, screen, cues| {
            match tick {
                813 => assert_eq!(pro(screen), [2, 1, 1, 1, 0]),
                1500 => assert_eq!(pro(screen)[..2], [4, 7]),
                _ => {}
            }
            voices.extend(cues.iter().filter_map(|cue| match cue {
                UiCue::Voice { player, line } => Some((tick, *player, *line)),
                _ => None,
            }));
        },
    );
    assert_eq!(voices, [(813 + 240, 0, 6), (1336 + 1, 0, 7)]);
    assert!(matches!(run.films[..], [(_, 3, false), (_, 0x10, false)]));
    assert_eq!(run.action, ScreenAction::Continue { prologue: true });

    let run = run_load(
        &dir,
        prologue_pack(),
        0x0B,
        0xC000_0000,
        &[],
        |_, screen, _| {
            assert_eq!(screen.value("pro_on"), 0);
        },
    );
    assert_eq!(run.end, 811);
    assert_eq!(run.action, ScreenAction::Continue { prologue: false });

    let run = run_load(
        &dir,
        prologue_pack(),
        0x0F,
        0x8000_0000,
        &presses,
        |tick, screen, _| {
            if tick == 813 {
                assert_eq!(pro(screen), [1, 1, 1, 1, 0]);
            }
        },
    );
    assert!(matches!(run.films[..], [(_, 3, true), (_, 0xF, true)]));
    assert_eq!(run.action, ScreenAction::Continue { prologue: true });

    // A pack without the sequence's pictures hands the game over at once.
    let run = run_load(&dir, pack(), 0x10, 0xC000_0000, &[], |_, screen, _| {
        assert_eq!(screen.value("pro_on"), 0);
    });
    assert_eq!(run.end, 811);
    assert_eq!(run.action, ScreenAction::Continue { prologue: false });
    let _ = std::fs::remove_dir_all(&dir);
}

/// `prologue_leon_skip`: a confirm on 1300 stops the line, fades the music
/// and the frame out and takes the sequence to its films' step, and the
/// game starts on 1442. The pad's other skip keys do the same.
#[test]
fn a_press_skips_the_prologue_to_its_films() {
    let dir = prologue_dir("pro-skip");
    let keys: [fn(&mut UiInput); 5] = [
        |input| input.confirm = true,
        |input| input.cancel = true,
        |input| input.start = true,
        |input| input.map = true,
        |input| input.option = true,
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

/// A title on `statics`, opened with `args`, in a folder of its own.
fn open_kept<'a>(
    statics: &'a crate::ui::ScreenStatics,
    dir: &'a std::path::Path,
    args: &[(&str, ScreenArg)],
) -> (LuaScreen, UiContext<'a>) {
    let mut cx = context(dir);
    cx.statics = Some(statics);
    let screen = LuaScreen::open_screen("title", args, ScreenAction::Quit, &mut cx).unwrap();
    (screen, cx)
}

/// The title left alone: the reel its idle count hands over to, and the
/// tick it does.
fn idle_to_the_demo(screen: &mut LuaScreen, cx: &UiContext<'_>) -> (Option<u8>, usize) {
    for tick in 0..2200 {
        if let ScreenResult::Done(action) = screen.update(cx, UiInput::default()) {
            let ScreenAction::Attract { reel } = action else {
                panic!("the idle title left for {action:?}");
            };
            return (reel, tick);
        }
    }
    panic!("the idle title never handed over");
}

/// `Title_main_select`'s idle count (`0x503B80`): 0x708 ticks, the fade to
/// black, and the hand-over to the reel it picked. The three reels go in
/// turn (`0x989E7F`), each Leon's or Claire's by the title's generator
/// (`0x50B910`), whose values are the executable's statics: the pack's
/// `demo/<cycle * 2 + character>.dat`.
#[test]
fn the_idle_title_plays_its_three_reels_in_turn() {
    let dir = std::env::temp_dir();
    let statics = crate::ui::ScreenStatics::default();
    // The generator from 0x80000000: Claire, Claire, Leon, Claire.
    let seed = [("seed", ScreenArg::Int(0x8000_0000))];
    let mut reels = Vec::new();
    for _ in 0..4 {
        let (mut screen, cx) = open_kept(&statics, &dir, &seed);
        let (reel, tick) = idle_to_the_demo(&mut screen, &cx);
        // `title_demo`: the title's tick 0 is the original's 189, and the
        // hand-over is on 2119.
        assert_eq!(tick, 2119 - 189);
        reels.push(reel);
    }
    assert_eq!(reels, [Some(1), Some(3), Some(4), Some(1)]);
    // Leon's from 0, and the cycle goes on.
    let statics = crate::ui::ScreenStatics::default();
    let (mut screen, cx) = open_kept(&statics, &dir, &[("seed", ScreenArg::Int(0))]);
    assert_eq!(idle_to_the_demo(&mut screen, &cx).0, Some(0));
    // The clock's, with no seed: one of the first two.
    let statics = crate::ui::ScreenStatics::default();
    let (mut screen, cx) = open_kept(&statics, &dir, &[]);
    assert!(matches!(idle_to_the_demo(&mut screen, &cx).0, Some(0 | 1)));
}

/// The title after a demo played to its end (`Game_loop`'s `0x4BFE5D`,
/// `title_demo_return`): its task starts from `Capcom_logo` with the
/// statics the hand-over left, sleeps 120 ticks on black, and enters the
/// title as from the boot. The film plays on the logo's every sixth pass
/// (`0x689F24`), the boot's being the first.
#[test]
fn the_title_after_a_demo_starts_from_the_logo() {
    let dir = std::env::temp_dir();
    let statics = crate::ui::ScreenStatics::default();
    let seed = ("seed", ScreenArg::Int(0));
    let (mut screen, cx) = open_kept(&statics, &dir, std::slice::from_ref(&seed));
    idle_to_the_demo(&mut screen, &cx);
    let played = [seed.clone(), ("after", ScreenArg::Str("demo".to_owned()))];
    // A return takes the hand-over's statics: each pass is given the one
    // hand-over's again.
    let handed: Vec<_> = [
        "mv",
        "disp",
        "menu",
        "step",
        "sel",
        "cur",
        "pulse",
        "slide",
        "demo",
        "flash",
        "logo",
        "logo_count",
        "kido",
        "add",
        "b8c",
        "fade3",
    ]
    .into_iter()
    .map(|name| format!("handed_{name}"))
    .map(|key| {
        let value = statics.get("ui/title.lua", &key);
        assert!(value.is_some(), "{key}");
        (key, value)
    })
    .collect();
    let mut films = Vec::new();
    for pass in 0..7 {
        for (key, value) in &handed {
            statics.set("ui/title.lua", key, *value);
        }
        let (mut screen, cx) = open_kept(&statics, &dir, &played);
        // As the hand-over left them: the last step, the count and the fade.
        assert_eq!(ti(&screen), [2, 3, 0, 0, 0, 1, 0, 0, 2, 0, 1928]);
        assert_eq!(logo(&screen), [0, -46, -1]);
        assert_eq!(screen.value("ti_fade_level"), -1);
        assert_eq!(screen.value("ti_fade_mode"), 2);
        screen.update(&cx, UiInput::default());
        assert_eq!(ti(&screen)[..4], [0, 3, 1, 5]);
        assert!(cx.cues.borrow().contains(&UiCue::CoreBank(0x10)));
        assert_eq!(backgrounds(&screen).0, None, "black");
        run(&mut screen, &cx, UiInput::default(), 119);
        assert_eq!(ti(&screen)[..4], [0, 3, 1, 5], "the logo's sleep");
        screen.update(&cx, UiInput::default());
        assert_eq!(ti(&screen)[..4], [0, 3, 1, 6]);
        let asked = screen.update(&cx, UiInput::default());
        let film = matches!(asked, ScreenResult::Done(ScreenAction::Film { id: 0, .. }));
        if film {
            // No film plays in a test: it is over on its tick.
            screen.resume_film(&cx, false, false);
        }
        films.push(film);
        assert_eq!(ti(&screen)[..4], [1, 3, 1, 0], "pass {pass}");
        // `Title_init`'s entry from the logo: the white, and the logo down.
        screen.update(&cx, UiInput::default());
        assert_eq!(ti(&screen)[..2], [2, 5]);
        assert_eq!(screen.value("ti_fade_level"), 0x7F80);
        assert_eq!(logo(&screen), [0, 240, -1]);
    }
    // No boot came first here, so the first pass is the film's.
    assert_eq!(films, [true, false, false, false, false, false, true]);
}

/// The title after a demo a key ended (`title_demo_abort_return`), which
/// is how it starts after a game: the logo's sleep, then the title's
/// picture fading in from black, with no film and no entry.
#[test]
fn the_title_after_a_demo_a_key_ended_fades_in() {
    let dir = std::env::temp_dir();
    let statics = crate::ui::ScreenStatics::default();
    let (mut screen, cx) = open_kept(&statics, &dir, &[]);
    idle_to_the_demo(&mut screen, &cx);
    let key = [("after", ScreenArg::Str("game".to_owned()))];
    let (mut screen, cx) = open_kept(&statics, &dir, &key);
    assert_eq!(ti(&screen), [2, 3, 0, 0, 0, 1, 0, 0, 2, 0, 1928]);
    run(&mut screen, &cx, UiInput::default(), 121);
    assert_eq!(ti(&screen)[..4], [0, 3, 1, 6]);
    assert!(matches!(
        screen.update(&cx, UiInput::default()),
        ScreenResult::Continue
    ));
    assert_eq!(ti(&screen)[..4], [1, 3, 1, 0]);
    screen.update(&cx, UiInput::default());
    assert_eq!(ti(&screen)[..2], [2, 1]);
    assert_eq!(screen.value("ti_fade_level"), 0x7C00);
    assert_eq!(screen.value("ti_fade_mode"), 2);
    assert_eq!(logo(&screen), [0, -46, -1]);
    // The logo's count is the played demos': the next one's is the film's.
    let played = [("after", ScreenArg::Str("demo".to_owned()))];
    let (mut screen, cx) = open_kept(&statics, &dir, &played);
    run(&mut screen, &cx, UiInput::default(), 121);
    assert!(matches!(
        screen.update(&cx, UiInput::default()),
        ScreenResult::Done(ScreenAction::Film { id: 0, .. })
    ));
}

/// The title after a game (`title_after_death`): `Game_loop` chains the
/// title's task as after a demo a key ended, with the statics the title
/// had as it handed the game over, a load's here, and fade 0's step as the
/// game left it (a death's `0x200`). The logo sleeps at the game's pace
/// (`Title_game_init`'s limiter stands until `Title_init`), and the title's
/// picture fades in from black. The hand-over's statics are that return's
/// alone: a game no title handed over comes back to a new title's.
#[test]
fn the_title_after_a_game_starts_as_it_handed_over() {
    let dir = std::env::temp_dir().join(format!("arklay-re2-after-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let statics = crate::ui::ScreenStatics::default();
    let (mut screen, cx) = open_kept(&statics, &dir, &[]);
    // LOAD GAME, as `asleep_under_the_load_screen` has it, and a load.
    run(&mut screen, &cx, UiInput::default(), 260 - 189);
    screen.update(&cx, confirm());
    let asleep = (261..=531).find_map(|_| match screen.update(&cx, UiInput::default()) {
        ScreenResult::Done(action) => Some(action),
        ScreenResult::Continue => None,
    });
    assert_eq!(asleep, Some(ScreenAction::SaveLoad));
    screen.resume_loaded(&cx, None);
    screen.update(&cx, UiInput::default());
    assert_eq!(
        screen.update(&cx, UiInput::default()),
        ScreenResult::Done(ScreenAction::Continue { prologue: false })
    );
    let handed = ti(&screen);
    assert_eq!(handed[..2], [2, 3]);
    assert_eq!(statics.get("ui/title.lua", "handed_disp"), Some(3));
    // The new-game page's slide count and the prologue's fade, as a new
    // game's hand-over leaves them.
    statics.set("ui/title.lua", "handed_b8c", Some(0x80));
    statics.set("ui/title.lua", "handed_fade3", Some(2));

    let game = [
        ("after", ScreenArg::Str("game".to_owned())),
        ("fade_add", ScreenArg::Int(0x200)),
    ];
    let (mut screen, cx) = open_kept(&statics, &dir, &game);
    let opened = ti(&screen);
    assert_eq!(opened[..4], [2, 3, 0, 0]);
    assert_eq!(opened[4..], handed[4..], "the hand-over's statics");
    assert_eq!(screen.value("ti_fade_level"), -1);
    assert_eq!(screen.value("ti_fade_add"), 0x200);
    assert_eq!(screen.value("ti_b8c"), 0x80);
    assert_eq!(screen.value("game_active"), 1, "the game's limiter");
    assert_eq!(statics.get("ui/title.lua", "handed_disp"), None, "taken");
    assert_eq!(statics.get("ui/title.lua", "handed_b8c"), None);
    run(&mut screen, &cx, UiInput::default(), 121);
    assert_eq!(ti(&screen)[..4], [0, 3, 1, 6]);
    assert_eq!(ti(&screen)[4..], handed[4..], "through the logo's sleep");
    assert_eq!(screen.value("ti_fade_add"), 0x200);
    assert!(matches!(
        screen.update(&cx, UiInput::default()),
        ScreenResult::Continue
    ));
    assert_eq!(screen.value("game_active"), 1, "to the logo's last tick");
    screen.update(&cx, UiInput::default());
    assert_eq!(ti(&screen)[..2], [2, 1], "`Title_init`, from black");
    assert_eq!(screen.value("ti_fade_level"), 0x7C00);
    assert_eq!(screen.value("ti_fade_add"), 0);
    assert_eq!(screen.value("game_active"), 0, "the title's limiter");
    assert_eq!(screen.value("ti_b8c"), 0x80, "`Title_init` leaves it");

    // A game with no hand-over: a new title's statics through the sleep.
    let (screen, _) = open_kept(&statics, &dir, &game);
    assert_eq!(ti(&screen), [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(screen.value("ti_b8c"), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The title after an extra mode's game (`Game_loop`'s `0x4BFE5D`):
/// `Init_global` keeps `fg_system`'s `0x1000899`, the extra's 8 and
/// To-fu's 1 with it, so after the logo's sleep `Capcom_logo`'s step 9 sets
/// `title_mv_state` 3, that mode's title, whose load runs on the next tick
/// with the mode's core bank. The game's other bits are dropped, and the
/// hand-over's dim count and fade 2's mode are as the extra's START left
/// them. `Title_game_init` left `0x99CF6C` set, so the mode's title plays
/// its cues at their own volume; `Title_init` clears it.
#[test]
fn the_title_after_an_extra_modes_game_is_that_modes_title() {
    let dir = std::env::temp_dir().join(format!("arklay-re2-after-extra-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (fg_system, bank) in [(0x4002_0008i64, 0x12), (0x4002_0009, 0x13)] {
        let statics = crate::ui::ScreenStatics::default();
        // The extra's START left SPECIAL's dim count up and fade 2 in the
        // confirm's mode, which nothing sets again before the game.
        statics.set("ui/title.lua", "handed_dim", Some(0x80));
        statics.set("ui/title.lua", "handed_fade2", Some(2));
        let game = [
            ("after", ScreenArg::Str("game".to_owned())),
            ("fade_add", ScreenArg::Int(0x200)),
            ("fg_system", ScreenArg::Int(fg_system)),
        ];
        let (mut screen, cx) = open_kept(&statics, &dir, &game);
        assert_eq!(screen.value("ti_fgsys"), fg_system & 9, "{fg_system:#x}");
        assert_eq!(screen.value("ti_dim"), 0x80);
        assert_eq!(screen.value("ti_fade2_mode"), 2);
        assert_eq!(screen.value("ti_se_live"), 1, "the game's `0x99CF6C`");
        run(&mut screen, &cx, UiInput::default(), 122);
        assert_eq!(screen.value("ti_mv"), 3, "{fg_system:#x}: step 9");
        cx.cues.borrow_mut().clear();
        screen.update(&cx, UiInput::default());
        assert_eq!(screen.value("ti_mv"), 4, "`Load_title_survivor`");
        assert_eq!(screen.value("ti_disp"), 0);
        assert_eq!(screen.value("ti_fade3_level"), 0x7C00);
        assert_eq!(*cx.cues.borrow(), [UiCue::CoreBank(bank)]);
    }
    // A game of ORIGINAL MODE's comes back to `Title_init`.
    let statics = crate::ui::ScreenStatics::default();
    let game = [
        ("after", ScreenArg::Str("game".to_owned())),
        ("fg_system", ScreenArg::Int(0x4002_0000)),
    ];
    let (mut screen, cx) = open_kept(&statics, &dir, &game);
    assert_eq!(screen.value("ti_se_live"), 1);
    run(&mut screen, &cx, UiInput::default(), 123);
    assert_eq!(screen.value("ti_mv"), 2, "`Title_init`");
    assert_eq!(screen.value("ti_se_live"), 0, "`Title_init` clears it");
    // A demo a key ended: its branch of `Title_game_init` left it clear.
    let demo = [
        ("after", ScreenArg::Str("game".to_owned())),
        ("fg_system", ScreenArg::Int(0x4000_2000)),
    ];
    let (screen, _) = open_kept(&statics, &dir, &demo);
    assert_eq!(screen.value("ti_se_live"), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The boot's film is Leon's or Claire's by the generator's first value,
/// and the boot starts the generator, the film's count and the reels'
/// cycle over.
#[test]
fn the_boots_film_is_the_generators_first_pick() {
    let dir = std::env::temp_dir();
    for (seed, claire) in [(0x8000_0000i64, true), (0, false), (0x7FFF_FFFF, false)] {
        let statics = crate::ui::ScreenStatics::default();
        statics.set("ui/title.lua", "rnd", Some(0x8000_0000));
        statics.set("ui/title.lua", "films", Some(3));
        statics.set("ui/title.lua", "demo_cycle", Some(2));
        let (mut screen, cx) = open_kept(&statics, &dir, &[("seed", ScreenArg::Int(seed))]);
        screen.signal("on_boot", &cx);
        let film = (0..=188).find_map(|_| match screen.update(&cx, UiInput::default()) {
            ScreenResult::Done(ScreenAction::Film { id: 0, claire, .. }) => Some(claire),
            _ => None,
        });
        assert_eq!(film, Some(claire), "seed {seed:#x}");
        // `0x509CB0` sets `fg_status` bit 31 by the pick, which OPTION's
        // model then reads its files by.
        assert_eq!(
            screen.value("ti_claire"),
            i64::from(claire),
            "seed {seed:#x}"
        );
        assert_eq!(statics.get("ui/title.lua", "rnd"), Some(seed));
        assert_eq!(statics.get("ui/title.lua", "films"), Some(1));
        assert_eq!(statics.get("ui/title.lua", "demo_cycle"), None);
    }
}

/// `Title_init` clears `hard_mode` and `censorship_off` as it clears
/// `fg_system`'s ARRANGE bit, so a game played on HARD or in ARRANGE MODE
/// leaves neither to the title it goes back to; `Rnd`'s state it leaves be.
#[test]
fn title_init_clears_hard_mode_and_arrange_mode() {
    let dir = std::env::temp_dir().join(format!("arklay-re2-title-init-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cx = context(&dir);
    let mut cx_open = context(&dir);
    let args = [("rnd", ScreenArg::Int(0x1234))];
    let mut screen =
        LuaScreen::open_screen("title", &args, ScreenAction::Quit, &mut cx_open).unwrap();
    let _: Option<bool> = screen.test_eval(
        "M.state.hard_mode = 1 M.state.censorship_off = 1 M.state.fg_system = 0x40 return nil",
    );
    screen.update(&cx, UiInput::default());
    let flags: (i64, i64, i64) = screen
        .test_eval("return M.state.hard_mode, M.state.censorship_off, M.state.fg_system & 0x40");
    assert_eq!(flags, (0, 0, 0));
    assert_eq!(screen.value("ti_rnd"), 0x1234);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The option screen opened from play (`Config`, `args.config`): its step 0
/// on the first tick, with no title under it and no draw from the title's
/// generator for the page's model, which is the player as play has them;
/// `Config_main` as from the title, and its way out leaves for play.
#[test]
fn the_option_screen_from_play_runs_from_its_step_0_and_resumes_play() {
    let dir = std::env::temp_dir();
    let statics = crate::ui::ScreenStatics::default();
    let args = [
        ("config", ScreenArg::Bool(true)),
        ("id", ScreenArg::Int(1)),
        ("weapon", ScreenArg::Int(3)),
        ("disc", ScreenArg::Int(1)),
    ];
    let (mut screen, cx) = open_kept(&statics, &dir, &args);
    assert_eq!(
        screen.update(&cx, UiInput::default()),
        ScreenResult::Continue
    );
    assert_eq!(opt(&screen), [1, 0, 0, 0, 0, 2, 0, 100, 100]);
    assert_eq!(frame(&screen).as_deref(), Some("black"));
    assert_eq!(screen.value("ti_fade_level"), 0x5000);
    let (room, title) = backgrounds(&screen);
    assert!(room.is_some() && room != title, "the room's picture is up");
    // `FUN_0044af00` replaces `FUN_0044b100`: no pick from the title's
    // generator, which nothing has drawn from.
    assert_eq!(
        screen.test_eval::<Option<i64>>("return M.state.opt.model"),
        None
    );
    assert_eq!(statics.get("ui/title.lua", "rnd"), None);
    assert_eq!(
        screen.test_eval::<i64>("return M.state.opt.player.id * 100 + M.state.opt.player.weapon * 10 + M.state.opt.player.disc"),
        131
    );
    run(&mut screen, &cx, UiInput::default(), 5);
    assert_eq!(screen.value("opt_state"), 1, "the menu");
    // SOUND's page and back, then the cancel key's way out: 32 ticks of
    // fade, 0x17, and step 2's tick resumes play.
    run(&mut screen, &cx, press("down"), 2);
    run(&mut screen, &cx, press("down"), 2);
    run(&mut screen, &cx, press("confirm"), 2);
    assert_eq!(screen.value("opt_state"), 4, "the SOUND page");
    run(&mut screen, &cx, press("cancel"), 2);
    assert_eq!(screen.value("opt_state"), 1);
    assert_eq!(
        tick_cues(&mut screen, &cx, press("cancel")),
        [UiCue::Core(5)]
    );
    assert_eq!(screen.value("opt_state"), 0x16);
    let mut ticks = 0;
    let action = loop {
        ticks += 1;
        assert!(ticks < 60, "the screen never ended");
        if let ScreenResult::Done(action) = screen.update(&cx, UiInput::default()) {
            break action;
        }
        assert_eq!(screen.value("opt_on"), 1, "tick {ticks}");
    };
    assert_eq!(action, ScreenAction::Resume);
    // `title_option_menu`'s 660 to 694: 0x16 for 32 ticks, 0x17, step 2.
    assert_eq!(ticks, 34);
    assert_eq!(screen.value("opt_on"), 0);
    assert_eq!(frame(&screen).as_deref(), Some("black"));
}

/// A key (`key:X`) or a pad source newly down on a tick, as the page reads
/// it (`input.key`, `input.button`).
fn press_source(name: &str) -> UiInput {
    press_sources(&[name])
}

/// The sources newly down together on one tick.
fn press_sources(names: &[&str]) -> UiInput {
    let mut input = UiInput {
        captured: Some(crate::controls::Source::parse(names[0]).unwrap()),
        any: true,
        ..UiInput::default()
    };
    for name in names {
        crate::ui::push_newly_down(
            &mut input.newly_down,
            crate::controls::Source::parse(name).unwrap(),
        );
    }
    input
}

/// The pad table the page edits, by the joypad's button: each word.
fn edit_pad(screen: &LuaScreen) -> Vec<i64> {
    let listed: String = screen.test_eval(
        "local out = {}
        for i = 0, 31 do out[#out + 1] = tostring(M.key_work.edit_pad[i]) end
        return table.concat(out, ',')",
    );
    listed
        .split(',')
        .map(|word| word.parse().unwrap())
        .collect()
}

/// A key newly down on a row is the decide cue and state 0xC, which binds
/// it to the row on the next tick: the row's old key goes to the entry that
/// held the new one, the last of them (`Config_main`'s 0x44A622). A key
/// `Kb_get_state`'s table hasn't (F5, Enter) is no press; the arrows, the
/// page's fixed keys, move the cursor.
#[test]
fn keyconfig_binds_a_key_pressed_on_a_row() {
    let dir = std::env::temp_dir();
    let cx = context(&dir);
    let (mut screen, dir) = open_option(&cx, "key-bind");
    open_key_page(&mut screen, &cx);
    for ignored in ["key:F5", "key:Return"] {
        assert!(tick_cues(&mut screen, &cx, press_source(ignored)).is_empty());
        assert_eq!(opt(&screen)[1..4], [0xB, 0, 4], "{ignored}");
    }
    run(&mut screen, &cx, press_source("key:Down"), 2);
    assert_eq!(opt(&screen)[3], 5);
    run(&mut screen, &cx, press_source("key:Up"), 2);
    assert_eq!(opt(&screen)[3], 4);
    // Q on Get Ready (entry 10), which no row holds.
    assert_eq!(
        tick_cues(&mut screen, &cx, press_source("key:Q")),
        [UiCue::Core(6)]
    );
    assert_eq!(opt(&screen)[1..4], [0xC, 0, 4]);
    assert_eq!(kc(&screen).1, DEFAULT_KEYS, "bound on the next tick");
    assert!(tick_cues(&mut screen, &cx, UiInput::default()).is_empty());
    assert_eq!(opt(&screen)[1..4], [0xB, 0, 4]);
    let bound = "686264660056004100005143115a0000000000000000000000262825270d201b";
    assert_eq!(kc(&screen).1, bound);
    // Get Ready's Q is red: the page changed it.
    assert_eq!(key_glyphs(&screen)[4], "Qr");
    // C, Fire's (entry 11), on Get Ready: Fire takes Q.
    run(&mut screen, &cx, press_source("key:C"), 2);
    let swapped = "686264660056004100004351115a0000000000000000000000262825270d201b";
    assert_eq!(kc(&screen).1, swapped);
    // The number pad's 8, Forward's, on Forward itself: nothing moves.
    for _ in 0..4 {
        run(&mut screen, &cx, press("up"), 2);
    }
    assert_eq!(opt(&screen)[3], 0);
    run(&mut screen, &cx, press_source("key:Keypad 8"), 2);
    assert_eq!(kc(&screen).1, swapped);
    // On the last row a key is the decide: the keys changed, so it asks;
    // and under the question a key answers (Change).
    run(&mut screen, &cx, press("up"), 2);
    assert_eq!(opt(&screen)[3], 10);
    assert_eq!(
        tick_cues(&mut screen, &cx, press_source("key:M")),
        [UiCue::Core(6)]
    );
    assert_eq!(opt(&screen)[1], 0xF);
    run(&mut screen, &cx, UiInput::default(), 2);
    assert_eq!(
        tick_cues(&mut screen, &cx, press_source("key:M")),
        [UiCue::Core(6)]
    );
    assert_eq!(opt(&screen)[1], 0xD);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A pad button newly down binds the row's word to it, and the button that
/// had the word takes the new one's old word (the first from button 8); on
/// the directions' rows it only ends state 0xC, and the directions
/// themselves are no press (`Config_main`'s 0x44A6BF).
#[test]
fn keyconfig_binds_a_pad_button_pressed_on_a_row() {
    let dir = std::env::temp_dir();
    let cx = context(&dir);
    let (mut screen, dir) = open_option(&cx, "key-pad-bind");
    open_key_page(&mut screen, &cx);
    let mut pad = edit_pad(&screen);
    assert_eq!((pad[10], pad[11]), (0x08, 0x800));
    assert!(tick_cues(&mut screen, &cx, press_source("pad:dpdown")).is_empty());
    // Y (button 11, Status's) on Get Ready: X (10) takes Status.
    assert_eq!(
        tick_cues(&mut screen, &cx, press_source("pad:y")),
        [UiCue::Core(6)]
    );
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(opt(&screen)[1], 0xB);
    (pad[10], pad[11]) = (0x800, 0x08);
    assert_eq!(edit_pad(&screen), pad);
    assert_eq!(kc(&screen).1, DEFAULT_KEYS, "the keys stay");
    // The left shoulder, which holds no word, on Fire: A (Fire's) is let go.
    run(&mut screen, &cx, press("down"), 2);
    run(&mut screen, &cx, press_source("pad:leftshoulder"), 2);
    (pad[8], pad[18]) = (0, 0x80);
    assert_eq!(edit_pad(&screen), pad);
    // On Forward the button binds nothing.
    for _ in 0..5 {
        run(&mut screen, &cx, press("up"), 2);
    }
    assert_eq!(opt(&screen)[3], 0);
    assert_eq!(
        tick_cues(&mut screen, &cx, press_source("pad:b")),
        [UiCue::Core(6)]
    );
    run(&mut screen, &cx, UiInput::default(), 1);
    assert_eq!(opt(&screen)[1..4], [0xB, 0, 0]);
    assert_eq!(edit_pad(&screen), pad);
    let _ = std::fs::remove_dir_all(&dir);
}

/// With the player's bindings (`on_bindings`, the interactive game) the
/// rows are arklay's actions: each lists its first key that isn't one of
/// the page's fixed keys and its first pad source, Default is arklay's
/// defaults, and Change asks for the rows the page changed, each key or
/// button replacing the action's others on its device, the fixed keys
/// kept.
#[test]
fn keyconfig_lists_and_changes_the_players_bindings() {
    use crate::controls::{Action, Bindings, Source};
    let dir = std::env::temp_dir();
    let cx = context(&dir);
    let (mut screen, dir) = open_option(&cx, "key-bindings");
    let defaults = Bindings::default();
    let mut bindings = Bindings::default();
    bindings.set(
        Action::Up,
        vec![
            Source::parse("key:Up").unwrap(),
            Source::parse("key:I").unwrap(),
        ],
    );
    bindings.set(Action::Map, vec![Source::parse("key:F5").unwrap()]);
    screen.tell_bindings(&bindings, &defaults, &cx);
    open_key_page(&mut screen, &cx);
    // Forward's first key past the arrow is I; Get Ready (the mouse and the
    // left trigger) has none; Configure's is Ctrl, the map's F5 by name.
    assert_eq!(
        key_glyphs(&screen),
        ["I", "S", "A", "D", "F", "38:176", "23:176", "8:176", "F5"]
    );
    // Q on Get Ready, then up to Forward and Tab (Status's) there.
    run(&mut screen, &cx, press_source("key:Q"), 2);
    for _ in 0..4 {
        run(&mut screen, &cx, press("up"), 2);
    }
    // A fixed key moves the cursor and binds nothing.
    assert!(tick_cues(&mut screen, &cx, press_source("key:Return")).is_empty());
    run(&mut screen, &cx, press_source("key:Tab"), 2);
    // The pad's Y on Fire.
    for _ in 0..5 {
        run(&mut screen, &cx, press("down"), 2);
    }
    run(&mut screen, &cx, press_source("pad:y"), 2);
    // Down to the last row, Exit's decide, and the question's Change.
    for _ in 0..5 {
        run(&mut screen, &cx, press("down"), 2);
    }
    assert_eq!(opt(&screen)[3], 10);
    run(&mut screen, &cx, press("confirm"), 2);
    assert_eq!(opt(&screen)[1], 0xF);
    assert!(screen.take_bindings_request().is_none());
    run(&mut screen, &cx, press("confirm"), 1);
    let applied = screen
        .take_bindings_request()
        .expect("Change asks")
        .apply(&bindings)
        .unwrap();
    let key = |name: &str| Source::parse(name).unwrap();
    assert_eq!(applied.get(Action::Up), [key("key:Tab"), key("key:Up")]);
    // START had Tab and I: it takes Forward's old I, and A, Fire's old
    // button.
    assert_eq!(applied.get(Action::Start), [key("key:I"), key("pad:a")]);
    assert_eq!(
        applied.get(Action::Aim),
        [key("key:Q"), Source::Mouse(3), key("axis:lefttrigger+")]
    );
    // Fire's pad is Y alone.
    assert_eq!(
        applied.get(Action::Fire),
        [
            key("key:F"),
            key("key:Return"),
            Source::Mouse(1),
            key("pad:y")
        ]
    );
    for action in [Action::Down, Action::Run, Action::Map, Action::Option] {
        assert_eq!(applied.get(action), bindings.get(action), "{action:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Default with the player's bindings sets the rows to arklay's own, and
/// Change asks for each changed row's whole shipped binding on its device.
#[test]
fn keyconfigs_default_is_arklays_defaults() {
    use crate::controls::{Action, Bindings, Source};
    let dir = std::env::temp_dir();
    let cx = context(&dir);
    let (mut screen, dir) = open_option(&cx, "key-bindings-default");
    let defaults = Bindings::default();
    let mut bindings = Bindings::default();
    bindings.set(
        Action::Fire,
        vec![
            Source::parse("key:K").unwrap(),
            Source::Mouse(1),
            Source::parse("pad:b").unwrap(),
        ],
    );
    screen.tell_bindings(&bindings, &defaults, &cx);
    open_key_page(&mut screen, &cx);
    for _ in 0..6 {
        run(&mut screen, &cx, press("down"), 2);
    }
    run(&mut screen, &cx, press("right"), 2);
    run(&mut screen, &cx, press("confirm"), 2);
    assert_eq!(key_glyphs(&screen)[4], "Fr", "Default's F, red");
    run(&mut screen, &cx, press("left"), 2);
    run(&mut screen, &cx, press("confirm"), 2);
    run(&mut screen, &cx, press("confirm"), 1);
    let applied = screen
        .take_bindings_request()
        .expect("Change asks")
        .apply(&bindings)
        .unwrap();
    assert_eq!(applied, defaults);
    let _ = std::fs::remove_dir_all(&dir);
}

/// With no arrows or Enter bound (the player moved to WASD), the page's
/// fixed keys still move its cursor and answer it, as the original's do.
#[test]
fn keyconfig_moves_and_answers_on_its_fixed_keys() {
    use crate::controls::{Action, Bindings, Source};
    let dir = std::env::temp_dir();
    let cx = context(&dir);
    let (mut screen, dir) = open_option(&cx, "key-fixed");
    let defaults = Bindings::default();
    let mut bindings = Bindings::default();
    for (action, key) in [
        (Action::Up, "key:W"),
        (Action::Down, "key:S"),
        (Action::Left, "key:A"),
        (Action::Right, "key:D"),
        (Action::Confirm, "key:F"),
    ] {
        bindings.set(action, vec![Source::parse(key).unwrap()]);
    }
    screen.tell_bindings(&bindings, &defaults, &cx);
    open_key_page(&mut screen, &cx);
    for _ in 0..6 {
        run(&mut screen, &cx, press_source("key:Down"), 2);
    }
    assert_eq!(opt(&screen)[1..4], [0xB, 0, 10]);
    run(&mut screen, &cx, press_source("key:Right"), 2);
    run(&mut screen, &cx, press_source("key:Return"), 2);
    // Default, then the question, answered No by Escape.
    run(&mut screen, &cx, press_source("key:Left"), 2);
    run(&mut screen, &cx, press_source("key:Space"), 2);
    assert_eq!(opt(&screen)[1], 0xF);
    run(&mut screen, &cx, press_source("key:Right"), 1);
    run(&mut screen, &cx, press_source("key:Escape"), 1);
    assert_eq!(opt(&screen)[1], 0xD);
    assert!(screen.take_bindings_request().is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Of keys newly down together the page takes the lowest virtual key, as
/// `Kb_get_state` looks through its table; a key that comes while the row
/// binds (state 0xC), when the page doesn't read the keyboard, is lost.
#[test]
fn keyconfig_takes_the_lowest_key_and_none_while_it_binds() {
    let dir = std::env::temp_dir();
    let cx = context(&dir);
    let (mut screen, dir) = open_option(&cx, "key-several");
    open_key_page(&mut screen, &cx);
    run(&mut screen, &cx, press_sources(&["key:Z", "key:Q"]), 1);
    assert_eq!(opt(&screen)[1], 0xC);
    screen.update(&cx, press_source("key:Z"));
    assert_eq!(opt(&screen)[1..4], [0xB, 0, 4]);
    let edit =
        |screen: &LuaScreen| -> String { screen.test_eval("return tostring(M.key_work.edit[10])") };
    assert_eq!(edit(&screen), "81");
    run(&mut screen, &cx, UiInput::default(), 2);
    assert_eq!(opt(&screen)[1..4], [0xB, 0, 4]);
    assert_eq!(edit(&screen), "81");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A fixed key is the page's whatever it is also bound to: the left arrow
/// bound to Backward toggles Exit's choice, and Return bound to Turn left
/// answers the question.
#[test]
fn keyconfigs_fixed_keys_win_over_what_they_are_bound_to() {
    use crate::controls::{Action, Bindings, Source};
    let dir = std::env::temp_dir();
    let cx = context(&dir);
    let (mut screen, dir) = open_option(&cx, "key-fixed-bound");
    let defaults = Bindings::default();
    let bindings = Bindings::default();
    screen.tell_bindings(&bindings, &defaults, &cx);
    open_key_page(&mut screen, &cx);
    run(&mut screen, &cx, press_source("key:Q"), 2);
    for _ in 0..6 {
        run(&mut screen, &cx, press("down"), 2);
    }
    let both = |screen: &mut LuaScreen, name: &str, direction: &str| {
        let mut input = press(direction);
        crate::ui::push_newly_down(&mut input.newly_down, Source::parse(name).unwrap());
        run(screen, &cx, input, 2);
    };
    both(&mut screen, "key:Left", "down");
    assert_eq!(opt(&screen)[3], 10, "still on the last row");
    let choice: i64 = screen.test_eval("return M.key_work.choice");
    assert_eq!(choice, 1);
    both(&mut screen, "key:Right", "down");
    run(&mut screen, &cx, press("confirm"), 2);
    assert_eq!(opt(&screen)[1], 0xF);
    both(&mut screen, "key:Return", "left");
    assert_eq!(opt(&screen)[1], 0xD);
    let applied = screen
        .take_bindings_request()
        .expect("Change asks")
        .apply(&bindings)
        .unwrap();
    assert_eq!(applied.get(Action::Aim)[0], Source::parse("key:Q").unwrap());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Fire/Confirm's and Run/Cancel's keys are the menus' decide and cancel
/// too; a key moved off another row comes off that row's action, which,
/// left with none, keeps its other keys.
#[test]
fn keyconfig_binds_confirm_with_fire_and_takes_a_moved_key_off() {
    use crate::controls::{Action, Bindings, Source};
    let dir = std::env::temp_dir();
    let cx = context(&dir);
    let (mut screen, dir) = open_option(&cx, "key-pairs");
    let defaults = Bindings::default();
    let bindings = Bindings::default();
    screen.tell_bindings(&bindings, &defaults, &cx);
    open_key_page(&mut screen, &cx);
    let before = key_glyphs(&screen);
    let edit = |screen: &LuaScreen, entry: usize| -> String {
        screen.test_eval(&format!("return tostring(M.key_work.edit[{entry}])"))
    };
    // Status's key on Get Ready, which has none: Status is left with none.
    let status = edit(&screen, 13);
    assert_eq!(edit(&screen, 10), "0");
    run(&mut screen, &cx, press_source(&status), 2);
    assert_eq!(edit(&screen, 13), "0");
    // K on Fire.
    run(&mut screen, &cx, press("down"), 2);
    run(&mut screen, &cx, press_source("key:K"), 2);
    assert_ne!(before, key_glyphs(&screen), "the page shows the change");
    for _ in 0..5 {
        run(&mut screen, &cx, press("down"), 2);
    }
    run(&mut screen, &cx, press("confirm"), 2);
    run(&mut screen, &cx, press("confirm"), 1);
    let applied = screen
        .take_bindings_request()
        .expect("Change asks")
        .apply(&bindings)
        .unwrap();
    let key = |name: &str| Source::parse(name).unwrap();
    assert_eq!(applied.get(Action::Fire)[0], key("key:K"));
    assert_eq!(applied.get(Action::Confirm)[0], key("key:K"));
    assert_eq!(applied.get(Action::Aim)[0], key(&status));
    let start: Vec<_> = bindings
        .get(Action::Start)
        .iter()
        .copied()
        .filter(|source| *source != key(&status))
        .collect();
    assert_eq!(applied.get(Action::Start), start);
    assert!(!start.is_empty(), "Status had another source");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A key two rows show, moved off one of them, stays on the other: the
/// player saw it there when they took the change.
#[test]
fn keyconfig_keeps_a_moved_key_on_a_row_that_still_shows_it() {
    use crate::controls::{Action, Bindings, Source};
    let dir = std::env::temp_dir();
    let cx = context(&dir);
    let (mut screen, dir) = open_option(&cx, "key-shared");
    let defaults = Bindings::default();
    let mut bindings = Bindings::default();
    let j = Source::parse("key:J").unwrap();
    bindings.set(Action::Option, vec![j]);
    bindings.set(Action::Map, vec![j]);
    screen.tell_bindings(&bindings, &defaults, &cx);
    open_key_page(&mut screen, &cx);
    run(&mut screen, &cx, press_source("key:J"), 2);
    for _ in 0..6 {
        run(&mut screen, &cx, press("down"), 2);
    }
    run(&mut screen, &cx, press("confirm"), 2);
    run(&mut screen, &cx, press("confirm"), 1);
    let applied = screen
        .take_bindings_request()
        .expect("Change asks")
        .apply(&bindings)
        .unwrap();
    assert_eq!(applied.get(Action::Aim)[0], j);
    assert_eq!(applied.get(Action::Map), [j]);
    assert!(!applied.get(Action::Option).contains(&j));
    let _ = std::fs::remove_dir_all(&dir);
}
