//! RE 2's save screen (`games/re2/ui/save_load.lua`, `Card_access` in save
//! mode) over a pack holding only the script, against the original's
//! traces (`oracle/re2.md`, "The save screen").

use std::path::{Path, PathBuf};

use super::*;

use crate::game::GameState;
use crate::pack::Pack;
use crate::render::Framebuffer;
use crate::save::SaveFile;
use crate::state::{RoomId, RoomState};
use crate::ui::{Screen, ScreenResult, UiCue, UiInput};

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
                "ui/save_load.lua",
                include_str!("../../../games/re2/ui/save_load.lua")
                    .as_bytes()
                    .to_vec(),
            )
            .unwrap();
        Pack::from_bytes(writer.to_bytes().unwrap()).unwrap()
    })
}

/// A fresh, empty save folder for one test, removed when it drops.
struct Folder(PathBuf);

impl std::ops::Deref for Folder {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for Folder {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn folder(name: &str) -> Folder {
    let dir = std::env::temp_dir().join(format!("arklay-re2-save-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    Folder(dir)
}

fn context(dir: &Path) -> UiContext<'_> {
    UiContext {
        statics: None,
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

/// Room 200's block with `saves` saves made.
fn block(saves: u8) -> SaveFile {
    re_block(saves, true)
}

/// A block with `saves` saves made, RE 2's (with its trailer) or RE 1's.
fn re_block(saves: u8, re2: bool) -> SaveFile {
    let mut game = GameState::new(RoomId::parse("2000").unwrap(), &RoomState::default());
    game.is_re2_room = re2;
    game.state_bytes[usize::from(crate::game::STATE_BYTE_SAVES)] = saves;
    SaveFile::from_state(&game)
}

/// `count` saves in `dir`, the first the oldest.
fn saves_in(dir: &Path, count: u8) {
    for index in 0..count {
        write_at(
            dir,
            &format!("save{index:02}.dat"),
            &block(index + 1),
            index,
        );
    }
}

/// `file` written as `name`, `minute` minutes into the test's clock.
fn write_at(dir: &Path, name: &str, file: &SaveFile, minute: u8) {
    let path = dir.join(name);
    crate::save::save_to_path(&path, file).unwrap();
    let time = std::time::SystemTime::UNIX_EPOCH
        + std::time::Duration::from_secs(1_000_000 + u64::from(minute) * 60);
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(time)
        .unwrap();
}

fn open(dir: &Path) -> LuaScreen {
    let mut cx = context(dir);
    LuaScreen::open_save("ui/save_load.lua", &mut cx, block(0), true).unwrap()
}

fn open_load(dir: &Path) -> LuaScreen {
    let mut cx = context(dir);
    LuaScreen::open_load("ui/save_load.lua", &mut cx).unwrap()
}

/// One tick, with its cues.
fn tick(screen: &mut LuaScreen, dir: &Path, input: UiInput) -> (ScreenResult, Vec<UiCue>) {
    let cx = context(dir);
    let result = screen.update(&cx, input);
    (result, cx.cues.into_inner())
}

/// The trace's `sv` from the values the script publishes, without the
/// engine's save count: step, init/exit step, state, mode, cursor, top,
/// rows, timer, choice, typed, saves.
fn sv(screen: &LuaScreen) -> Vec<i64> {
    [
        "sv_sub",
        "sv_step",
        "sv_state",
        "sv_mode",
        "sv_cursor",
        "sv_top",
        "sv_rows",
        "sv_timer",
        "sv_choice",
        "sv_typed",
        "sv_saves",
    ]
    .iter()
    .map(|key| screen.value(key))
    .collect()
}

fn held_down() -> UiInput {
    UiInput {
        down: true,
        held_down: true,
        ..UiInput::default()
    }
}

/// From the open to the list: `typewriter_200_yes_cancel`'s 547 to 555.
fn to_the_list(screen: &mut LuaScreen, dir: &Path) {
    for _ in 0..8 {
        tick(screen, dir, UiInput::default());
    }
}

#[test]
fn the_save_screen_steps_one_state_a_tick() {
    let dir = folder("steps");
    saves_in(&dir, 3);
    let mut screen = open(&dir);
    assert_eq!(screen.value("game_active"), 1, "the game's limiter");
    // 547: `Cardaccess_init`'s step 0 ran with the open.
    assert_eq!(sv(&screen), [3, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    // 548 to 551: the fade in; 552 its end.
    for _ in 0..4 {
        tick(&mut screen, &dir, UiInput::default());
        assert_eq!(sv(&screen)[..2], [3, 1]);
    }
    tick(&mut screen, &dir, UiInput::default());
    assert_eq!(sv(&screen)[..2], [3, 2], "552");
    // 553: `Task_sleep(2)`'s skipped tick.
    tick(&mut screen, &dir, UiInput::default());
    assert_eq!(sv(&screen)[..3], [3, 2, 0], "553");
    // 554: state 0, 555: state 1 lists the three saves.
    tick(&mut screen, &dir, UiInput::default());
    assert_eq!(sv(&screen)[..3], [2, 0, 1], "554");
    tick(&mut screen, &dir, UiInput::default());
    assert_eq!(sv(&screen), [2, 0, 2, 0, 0, 0, 5, 0, 0, 0, 3], "555");
    // `Card_write`'s order: the newest first.
    let first: String = screen.test_eval("return M.state.saves[1].name");
    assert_eq!(first, "save02.dat");
}

#[test]
fn down_moves_every_fourth_tick_while_held_and_wraps() {
    let dir = folder("down");
    saves_in(&dir, 3);
    let mut screen = open(&dir);
    to_the_list(&mut screen, &dir);
    let mut cursors = Vec::new();
    for at in 0..20 {
        let (_, cues) = tick(&mut screen, &dir, held_down());
        let moved = at % 4 == 0;
        assert_eq!(
            cues,
            if moved { vec![UiCue::Core(4)] } else { vec![] },
            "tick {at}"
        );
        if moved {
            cursors.push(sv(&screen)[4]);
        }
        assert_eq!(
            sv(&screen)[7],
            [3, 2, 1, 0][at % 4],
            "the timer on tick {at}"
        );
    }
    // New file, three saves, leave: down from the new file wraps past leave.
    assert_eq!(cursors, [1, 2, 3, 4, 0]);
}

#[test]
fn decide_on_new_file_writes_and_reveals() {
    let dir = folder("new");
    let mut screen = open(&dir);
    to_the_list(&mut screen, &dir);
    let confirm = UiInput {
        confirm: true,
        ..UiInput::default()
    };
    // P: the decide cue and state 3.
    let (_, cues) = tick(&mut screen, &dir, confirm);
    assert_eq!(cues, [UiCue::Core(6)]);
    assert_eq!(sv(&screen)[2], 3);
    assert!(screen.take_outcome().is_none());
    // P+1: the write, once.
    tick(&mut screen, &dir, UiInput::default());
    assert_eq!(sv(&screen)[2], 0x22);
    assert!(
        screen
            .take_outcome()
            .is_some_and(|outcome| outcome.ink_ribbon)
    );
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    // From P+2, a character on each tick the timer is 0: 5 ticks a
    // character, 9 after a space or `_`.
    let name: String = screen.test_eval("return M.state.name");
    assert!(name.starts_with("Leon A_01_"), "{name}");
    let mut next = 0;
    for typed in 1..=name.len() {
        for _ in 0..next {
            let (_, cues) = tick(&mut screen, &dir, UiInput::default());
            assert!(cues.is_empty());
        }
        let (_, cues) = tick(&mut screen, &dir, UiInput::default());
        assert_eq!(sv(&screen)[9], typed as i64);
        let space = matches!(name.as_bytes()[typed - 1], b' ' | b'_');
        assert_eq!(cues, [UiCue::RoomSound(if space { 0x24 } else { 0x22 })]);
        next = if space { 8 } else { 4 };
    }
    for _ in 0..next {
        tick(&mut screen, &dir, UiInput::default());
    }
    // The reveal's end: 0x24 with the timer at 60, then 61 ticks of wait.
    tick(&mut screen, &dir, UiInput::default());
    assert_eq!(
        sv(&screen)[2..],
        [0x24, 0, 0, 0, 2, 60, 0, name.len() as i64, 0]
    );
    for _ in 0..60 {
        tick(&mut screen, &dir, UiInput::default());
        assert_eq!(sv(&screen)[2], 0x24);
    }
    tick(&mut screen, &dir, UiInput::default());
    assert_eq!(sv(&screen)[2], 0x61);
    // 0x61, then the exit's fade over six ticks and the black.
    tick(&mut screen, &dir, UiInput::default());
    assert_eq!(sv(&screen)[..2], [4, 1]);
    for _ in 0..6 {
        let (result, _) = tick(&mut screen, &dir, UiInput::default());
        assert_eq!(result, ScreenResult::Continue);
    }
    assert_eq!(sv(&screen)[..2], [4, 2]);
    let (result, _) = tick(&mut screen, &dir, UiInput::default());
    assert_ne!(result, ScreenResult::Continue, "Mem_card ends");
}

/// One press of a button.
fn press(set: impl Fn(&mut UiInput)) -> UiInput {
    let mut input = UiInput::default();
    set(&mut input);
    input
}

/// From the list to the overwrite choice of the folder's only save:
/// `save_200_overwrite`'s 1140 to 1180.
fn to_the_choice(screen: &mut LuaScreen, dir: &Path) {
    let (_, cues) = tick(screen, dir, held_down());
    assert_eq!(cues, [UiCue::Core(4)]);
    for _ in 0..3 {
        tick(screen, dir, UiInput::default());
    }
    let (_, cues) = tick(screen, dir, press(|input| input.confirm = true));
    assert_eq!(cues, [UiCue::Core(6)]);
    assert_eq!(sv(screen)[2..], [0x1E, 0, 1, 0, 3, 0, 0, 0, 1]);
}

#[test]
fn the_overwrite_choice_moves_and_cancels() {
    let dir = folder("choice");
    saves_in(&dir, 1);
    let mut screen = open(&dir);
    to_the_list(&mut screen, &dir);
    assert_eq!(sv(&screen)[2..], [2, 0, 0, 0, 3, 0, 0, 0, 1]);
    to_the_choice(&mut screen, &dir);
    // Cancel: the cancel cue, and the list again on the save's row.
    let (_, cues) = tick(&mut screen, &dir, press(|input| input.cancel = true));
    assert_eq!(cues, [UiCue::Core(5)]);
    assert_eq!(sv(&screen)[2..], [2, 0, 1, 0, 3, 0, 0, 0, 1]);
    // Right and left are edges: each moves the choice, with the move cue
    // and no timer, and it has two answers.
    let (_, cues) = tick(&mut screen, &dir, press(|input| input.confirm = true));
    assert_eq!(cues, [UiCue::Core(6)]);
    for (set, choice) in [
        (press(|input| input.right = true), 1),
        (press(|input| input.left = true), 0),
        (press(|input| input.left = true), 1),
        (press(|input| input.right = true), 0),
        (press(|input| input.right = true), 1),
    ] {
        let (_, cues) = tick(&mut screen, &dir, set);
        assert_eq!(cues, [UiCue::Core(4)]);
        assert_eq!(sv(&screen)[2..], [0x1E, 0, 1, 0, 3, 0, choice, 0, 1]);
    }
    // A held direction is no move, and unlike the list's it doesn't hide
    // a decide. Decide on Cancel is a cancel: the save stays.
    let held = UiInput {
        held_right: true,
        ..UiInput::default()
    };
    let (_, cues) = tick(&mut screen, &dir, held);
    assert!(cues.is_empty());
    assert_eq!(sv(&screen)[8], 1);
    let decide = UiInput {
        confirm: true,
        ..held
    };
    let (_, cues) = tick(&mut screen, &dir, decide);
    assert_eq!(cues, [UiCue::Core(5)]);
    assert_eq!(sv(&screen)[2], 2);
    assert!(dir.join("save00.dat").exists());
    assert!(screen.take_outcome().is_none());
    // The next decide starts on Overwrite again.
    tick(&mut screen, &dir, press(|input| input.confirm = true));
    assert_eq!(sv(&screen)[2..], [0x1E, 0, 1, 0, 3, 0, 0, 0, 1]);
    // A direction's press comes before a decide on the same tick, and a
    // decide before a cancel.
    let both = UiInput {
        right: true,
        confirm: true,
        ..UiInput::default()
    };
    let (_, cues) = tick(&mut screen, &dir, both);
    assert_eq!(cues, [UiCue::Core(4)]);
    assert_eq!(sv(&screen)[2..], [0x1E, 0, 1, 0, 3, 0, 1, 0, 1]);
    tick(&mut screen, &dir, press(|input| input.left = true));
    let both = UiInput {
        confirm: true,
        cancel: true,
        ..UiInput::default()
    };
    let (_, cues) = tick(&mut screen, &dir, both);
    assert_eq!(cues, [UiCue::Core(6)]);
    assert_eq!(sv(&screen)[2], 0x1F);
}

/// With the list scrolled, the save overwritten is the one under the
/// cursor: the row's index is the cursor's and the top row's together.
#[test]
fn a_scrolled_lists_overwrite_takes_the_row_under_the_cursor() {
    let dir = folder("overwrite-scrolled");
    saves_in(&dir, 12);
    let mut screen = open(&dir);
    to_the_list(&mut screen, &dir);
    // Eleven rows down: the cursor on the last of ten rows, the list two
    // down. Row 0 is the new file, so the row is the eleventh newest save.
    for _ in 0..11 {
        tick(&mut screen, &dir, held_down());
        for _ in 0..3 {
            tick(&mut screen, &dir, UiInput::default());
        }
    }
    assert_eq!(sv(&screen)[4..6], [9, 2]);
    tick(&mut screen, &dir, press(|input| input.confirm = true));
    assert_eq!(sv(&screen)[2], 0x1E);
    tick(&mut screen, &dir, press(|input| input.confirm = true));
    tick(&mut screen, &dir, UiInput::default());
    assert_eq!(sv(&screen)[2], 0x20);
    let left: Vec<bool> = (0..12)
        .map(|index| dir.join(format!("save{index:02}.dat")).exists())
        .collect();
    let mut expected = [true; 12];
    expected[1] = false;
    assert_eq!(left, expected);
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 12);
}

/// A write that fails loses no save: the row's file is deleted only once
/// the new one is written. The screen carries on as after a save, as a new
/// file's failed write does.
#[test]
fn a_failed_overwrite_keeps_the_old_save() {
    let dir = folder("overwrite-failed");
    saves_in(&dir, 1);
    let mut screen = open(&dir);
    // A folder stands where the new file would go.
    screen.test_eval::<()>("M.save_name = function() return 'blocked' end");
    std::fs::create_dir(dir.join("blocked.dat")).unwrap();
    to_the_list(&mut screen, &dir);
    to_the_choice(&mut screen, &dir);
    tick(&mut screen, &dir, press(|input| input.confirm = true));
    tick(&mut screen, &dir, UiInput::default());
    assert_eq!(sv(&screen)[2], 0x20);
    assert!(dir.join("save00.dat").exists());
}

/// A save whose name is the new one's but for its case is one file with it
/// where the folder folds case, and another where it doesn't: either way
/// the overwrite leaves the new save, alone.
#[test]
fn an_overwrite_of_a_name_differing_in_case_leaves_the_new_save() {
    let dir = folder("overwrite-case");
    write_at(&dir, "FIXED.DAT", &block(1), 0);
    let mut screen = open(&dir);
    screen.test_eval::<()>("M.save_name = function() return 'fixed' end");
    to_the_list(&mut screen, &dir);
    to_the_choice(&mut screen, &dir);
    tick(&mut screen, &dir, press(|input| input.confirm = true));
    tick(&mut screen, &dir, UiInput::default());
    assert_eq!(sv(&screen)[2], 0x20);
    let left = crate::save::scan_dir(&dir);
    assert_eq!(left.len(), 1, "{:?}", left[0].name);
    assert!(left[0].name.eq_ignore_ascii_case("fixed.dat"));
    assert!(left[0].info.is_some(), "a whole save");
}

/// A font sheet whose every cell is a solid white block, so a drawn glyph
/// shows in a frame without the pack's art.
fn solid_font() -> crate::font::Font {
    let (width, height) = (256u32, 224u32);
    let texture = crate::model::Texture8 {
        width,
        height,
        indices: vec![1; (width * height) as usize],
        palettes: vec![[255, 255, 255, 255]; 256],
        stp: vec![false; 256],
    };
    crate::font::Font::new(texture, crate::font::FontDef::western())
}

/// A frame of the list, the choice and the retyped row, over a solid-cell
/// font: a save whose file name isn't ASCII draws, the answers' arrowhead
/// shows only while the blink is under 15, and the new label types in the
/// save's own row.
#[test]
fn the_overwrite_draws_its_arrowhead_and_types_in_the_row() {
    let dir = folder("overwrite-draw");
    write_at(&dir, "Caf\u{e9} \u{4e16}.dat", &block(1), 0);
    let font = solid_font();
    let frame = |screen: &mut LuaScreen| {
        let mut cx = context(&dir);
        cx.font = Some(&font);
        let mut framebuffer = Framebuffer::new();
        screen.draw(&cx, &mut framebuffer);
        assert!(!screen.failed(), "the draw ran");
        framebuffer
    };
    let pixel = |framebuffer: &Framebuffer, x: usize, y: usize| {
        let at = (y * framebuffer.width as usize + x) * 4;
        [
            framebuffer.rgba[at],
            framebuffer.rgba[at + 1],
            framebuffer.rgba[at + 2],
        ]
    };
    // Whether any pixel of the row at `y` (15 high) isn't black.
    let row_drawn = |framebuffer: &Framebuffer, y: usize| {
        (y..y + 14).any(|y| (20..300).any(|x| pixel(framebuffer, x, y) != [0, 0, 0]))
    };
    let mut screen = open(&dir);
    to_the_list(&mut screen, &dir);
    let list = frame(&mut screen);
    assert!(row_drawn(&list, 0x31 + 15), "the save's row");
    // The label starts where a line of its six characters centres, which
    // a count of its bytes would move, and its accented letter is drawn:
    // four cells of ink, then the space and a character the font lacks.
    let inked: Vec<usize> = (20..300)
        .filter(|&x| (0x31 + 15..0x31 + 29).any(|y| pixel(&list, x, y) != [0, 0, 0]))
        .collect();
    assert_eq!((inked[0], *inked.last().unwrap()), (137, 166));
    to_the_choice(&mut screen, &dir);
    // The arrowhead's body, 7 left of the lit answer and 4 down its row.
    const BODY: [u8; 3] = [206, 206, 189];
    let arrow = |framebuffer: &Framebuffer, answer_x: usize| {
        pixel(framebuffer, answer_x - 7 + 1, 0xDE + 4 + 4) == BODY
    };
    let lit = frame(&mut screen);
    assert!(arrow(&lit, 0x38) && !arrow(&lit, 0xCC));
    for _ in 0..14 {
        tick(&mut screen, &dir, UiInput::default());
    }
    let out = frame(&mut screen);
    assert!(!arrow(&out, 0x38), "out from the blink's 15th tick");
    tick(&mut screen, &dir, press(|input| input.right = true));
    let moved = frame(&mut screen);
    assert!(arrow(&moved, 0xCC) && !arrow(&moved, 0x38));
    // The overwrite: the row is empty on the write's tick and fills as the
    // label types; the new file's row keeps "Create New".
    tick(&mut screen, &dir, press(|input| input.left = true));
    tick(&mut screen, &dir, press(|input| input.confirm = true));
    tick(&mut screen, &dir, UiInput::default());
    assert_eq!(sv(&screen)[2], 0x20);
    let empty = frame(&mut screen);
    assert!(row_drawn(&empty, 0x31));
    let bar_only = |framebuffer: &Framebuffer| {
        let first = pixel(framebuffer, 20, 0x31 + 15 + 3);
        (20..300).all(|x| pixel(framebuffer, x, 0x31 + 15 + 3) == first)
    };
    assert!(bar_only(&empty), "nothing typed yet");
    tick(&mut screen, &dir, UiInput::default());
    let typed = frame(&mut screen);
    assert!(!bar_only(&typed), "the first character, in the save's row");
}

#[test]
fn the_choices_cursor_blinks_15_of_32_ticks_and_restarts_on_a_move() {
    let dir = folder("blink");
    saves_in(&dir, 1);
    let mut screen = open(&dir);
    to_the_list(&mut screen, &dir);
    let count = |screen: &LuaScreen| screen.test_eval::<i64>("return M.state.blink");
    assert_eq!(count(&screen), 0, "not stepped outside the choice");
    to_the_choice(&mut screen, &dir);
    // `save_200_overwrite`: lit from 1180 to 1193, out to 1210, lit to
    // the cancel on 1220.
    assert_eq!(count(&screen), 1);
    for tick_no in 1..40 {
        tick(&mut screen, &dir, UiInput::default());
        assert_eq!(count(&screen), (1 + tick_no) & 0x1F);
    }
    // The list doesn't step it, and the choice carries on from it (1260:
    // six more ticks lit).
    tick(&mut screen, &dir, press(|input| input.cancel = true));
    tick(&mut screen, &dir, UiInput::default());
    assert_eq!(count(&screen), 8);
    tick(&mut screen, &dir, press(|input| input.confirm = true));
    assert_eq!(count(&screen), 9);
    // A move starts it again, lit (1300 and 1320: 15 ticks each).
    tick(&mut screen, &dir, press(|input| input.right = true));
    assert_eq!(count(&screen), 0);
    tick(&mut screen, &dir, UiInput::default());
    tick(&mut screen, &dir, press(|input| input.left = true));
    assert_eq!(count(&screen), 0);
}

#[test]
fn decide_on_overwrite_replaces_the_save_and_types_in_its_row() {
    let dir = folder("overwrite");
    saves_in(&dir, 1);
    let mut screen = open(&dir);
    to_the_list(&mut screen, &dir);
    to_the_choice(&mut screen, &dir);
    // P: the decide cue and state 0x1F; the old file is still there.
    let (_, cues) = tick(&mut screen, &dir, press(|input| input.confirm = true));
    assert_eq!(cues, [UiCue::Core(6)]);
    assert_eq!(sv(&screen)[2..], [0x1F, 0, 1, 0, 3, 0, 0, 0, 1]);
    assert!(dir.join("save00.dat").exists());
    assert!(screen.take_outcome().is_none());
    // P+1: the write, once, under the new name, and the old file goes;
    // the folder's count isn't read again.
    tick(&mut screen, &dir, UiInput::default());
    assert!(!dir.join("save00.dat").exists());
    assert_eq!(sv(&screen)[2..], [0x20, 0, 1, 0, 3, 0, 0, 0, 1]);
    assert!(
        screen
            .take_outcome()
            .is_some_and(|outcome| outcome.ink_ribbon)
    );
    let name: String = screen.test_eval("return M.state.name");
    assert!(name.starts_with("Leon A_01_"), "{name}");
    assert!(dir.join(format!("{name}.dat")).exists());
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    // From P+2 the label types as a new file's does: 5 ticks a character,
    // 9 after a space or `_`.
    let mut next = 0;
    for typed in 1..=name.len() {
        for _ in 0..next {
            let (_, cues) = tick(&mut screen, &dir, UiInput::default());
            assert!(cues.is_empty());
        }
        let (_, cues) = tick(&mut screen, &dir, UiInput::default());
        assert_eq!(sv(&screen)[9], typed as i64);
        let space = matches!(name.as_bytes()[typed - 1], b' ' | b'_');
        assert_eq!(cues, [UiCue::RoomSound(if space { 0x24 } else { 0x22 })]);
        next = if space { 8 } else { 4 };
        assert_eq!(sv(&screen)[7], next);
    }
    for _ in 0..next {
        tick(&mut screen, &dir, UiInput::default());
    }
    // The reveal's end: 0x21 with the timer at 60, 61 ticks of wait, 0x61.
    tick(&mut screen, &dir, UiInput::default());
    assert_eq!(
        sv(&screen)[2..],
        [0x21, 0, 1, 0, 3, 60, 0, name.len() as i64, 1]
    );
    for _ in 0..60 {
        tick(&mut screen, &dir, UiInput::default());
        assert_eq!(sv(&screen)[2], 0x21);
    }
    tick(&mut screen, &dir, UiInput::default());
    assert_eq!(sv(&screen)[2], 0x61);
    tick(&mut screen, &dir, UiInput::default());
    assert_eq!(sv(&screen)[..2], [4, 1]);
}

#[test]
fn a_press_ends_the_overwrites_wait() {
    let dir = folder("overwrite-press");
    saves_in(&dir, 1);
    let mut screen = open(&dir);
    to_the_list(&mut screen, &dir);
    to_the_choice(&mut screen, &dir);
    tick(&mut screen, &dir, press(|input| input.start = true));
    assert_eq!(sv(&screen)[2], 0x1F, "START decides too");
    // The write, then 29 characters at 9 ticks each at most.
    let mut ticks = 0;
    while sv(&screen)[2] != 0x21 {
        tick(&mut screen, &dir, UiInput::default());
        ticks += 1;
        assert!(ticks < 300, "the reveal ends");
    }
    let (_, cues) = tick(&mut screen, &dir, press(|input| input.cancel = true));
    assert!(cues.is_empty());
    assert_eq!(sv(&screen)[2], 0x61);
    assert_eq!(sv(&screen)[7], 0);
}

/// The work the original keeps in static data: a screen opens with the rows
/// and save count of the last listing, the name scroll and the question's
/// blink as the last screen left them (`save_200_scroll`'s 1177 and
/// `save_200_ask_twice`'s 1600), and a context that keeps nothing starts
/// each screen over.
#[test]
fn a_screen_finds_the_work_the_last_one_kept() {
    for kept in [true, false] {
        let dir = folder(if kept { "kept" } else { "not-kept" });
        saves_in(&dir, 1);
        let statics = crate::ui::ScreenStatics::default();
        let context_of = || {
            let mut cx = context(&dir);
            if kept {
                cx.statics = Some(&statics);
            }
            cx
        };
        let open_one = || {
            let mut cx = context_of();
            LuaScreen::open_save("ui/save_load.lua", &mut cx, block(0), true).unwrap()
        };
        let step = |screen: &mut LuaScreen, input: UiInput| {
            let cx = context_of();
            screen.update(&cx, input)
        };
        let mut screen = open_one();
        assert_eq!(sv(&screen)[6], 0, "no listing yet");
        for _ in 0..8 {
            step(&mut screen, UiInput::default());
        }
        assert_eq!((sv(&screen)[6], sv(&screen)[10]), (3, 1));
        // The scroll to 8, then the question on the save's row for five
        // ticks.
        step(
            &mut screen,
            UiInput {
                held_right: true,
                ..UiInput::default()
            },
        );
        for _ in 0..3 {
            step(&mut screen, UiInput::default());
        }
        step(&mut screen, held_down());
        for _ in 0..3 {
            step(&mut screen, UiInput::default());
        }
        step(&mut screen, press(|input| input.confirm = true));
        assert_eq!(sv(&screen)[2], 0x1E);
        for _ in 0..4 {
            step(&mut screen, UiInput::default());
        }
        assert_eq!(screen.value("sv_blink"), 5);
        step(&mut screen, press(|input| input.cancel = true));
        step(&mut screen, press(|input| input.cancel = true));
        while step(&mut screen, UiInput::default()) == ScreenResult::Continue {}
        // The next screen, before its own listing.
        let mut screen = open_one();
        let found = (
            sv(&screen)[6],
            sv(&screen)[10],
            screen.value("sv_scroll"),
            screen.value("sv_blink"),
        );
        assert_eq!(found, if kept { (3, 1, 8, 5) } else { (0, 0, 0, 0) });
        for _ in 0..8 {
            step(&mut screen, UiInput::default());
        }
        step(&mut screen, held_down());
        for _ in 0..3 {
            step(&mut screen, UiInput::default());
        }
        step(&mut screen, press(|input| input.confirm = true));
        assert_eq!(
            screen.value("sv_blink"),
            if kept { 6 } else { 1 },
            "the question's blink goes on"
        );
    }
}

/// The choice the question last drew is static too (`Mwork_pc2`): a
/// question left on Cancel starts the next screen's blink over, since that
/// one opens on Overwrite, where a question left on Overwrite lets it go on
/// (`a_screen_finds_the_work_the_last_one_kept`).
#[test]
fn a_question_left_on_cancel_starts_the_next_screens_blink_over() {
    for kept in [true, false] {
        let dir = folder(if kept {
            "kept-choice"
        } else {
            "no-kept-choice"
        });
        saves_in(&dir, 1);
        let statics = crate::ui::ScreenStatics::default();
        let context_of = || {
            let mut cx = context(&dir);
            if kept {
                cx.statics = Some(&statics);
            }
            cx
        };
        let step = |screen: &mut LuaScreen, input: UiInput| {
            let cx = context_of();
            screen.update(&cx, input)
        };
        let to_the_question = |screen: &mut LuaScreen| {
            for _ in 0..8 {
                step(screen, UiInput::default());
            }
            step(screen, held_down());
            for _ in 0..3 {
                step(screen, UiInput::default());
            }
            step(screen, press(|input| input.confirm = true));
            assert_eq!(sv(screen)[2], 0x1E);
        };
        let open_one = || {
            let mut cx = context_of();
            LuaScreen::open_save("ui/save_load.lua", &mut cx, block(0), true).unwrap()
        };
        let mut screen = open_one();
        to_the_question(&mut screen);
        // To Cancel, three more ticks of its blink, and out on it.
        step(&mut screen, press(|input| input.right = true));
        for _ in 0..3 {
            step(&mut screen, UiInput::default());
        }
        assert_eq!((sv(&screen)[8], screen.value("sv_blink")), (1, 3));
        step(&mut screen, press(|input| input.confirm = true));
        step(&mut screen, press(|input| input.cancel = true));
        while step(&mut screen, UiInput::default()) == ScreenResult::Continue {}
        let mut screen = open_one();
        assert_eq!(screen.value("sv_blink"), if kept { 3 } else { 0 });
        to_the_question(&mut screen);
        assert_eq!(sv(&screen)[8], 0, "the question opens on Overwrite");
        assert_eq!(
            screen.value("sv_blink"),
            if kept { 0 } else { 1 },
            "from the start: the last question drew Cancel"
        );
    }
}

/// `api:keep` and `api:kept`: integers and booleans by key, `nil` to forget
/// one, shared by every screen of the context's store, 64 keys at most, and
/// nothing kept by a context without a store.
#[test]
fn the_api_keeps_values_past_a_screen() {
    let dir = folder("keep-api");
    let statics = crate::ui::ScreenStatics::default();
    let mut screen = open(&dir);
    let mut eval = |kept: bool, body: &str| -> Option<i64> {
        let mut cx = context(&dir);
        if kept {
            cx.statics = Some(&statics);
        }
        screen.test_eval_with_game(&cx, None, body)
    };
    assert_eq!(eval(true, "return api:kept('a')"), None);
    assert_eq!(eval(true, "api:keep('a', 7) return api:kept('a')"), Some(7));
    assert_eq!(
        statics.get("ui/save_load.lua", "a"),
        Some(7),
        "the store's, under the script's own scope"
    );
    assert_eq!(statics.get("UI/Save_Load.lua", "a"), Some(7), "in any case");
    assert_eq!(statics.get("ui/title.lua", "a"), None);
    assert_eq!(
        eval(true, "api:keep('b', true) return api:kept('b')"),
        Some(1)
    );
    assert_eq!(eval(true, "api:keep('a', nil) return api:kept('a')"), None);
    assert_eq!(
        eval(true, "return pcall(api.keep, api, 'c', 1.5) and 1 or 0"),
        Some(0),
        "an integer, a boolean or nil"
    );
    // 64 keys: `b` and 63 more, then no new one, though a kept one changes.
    assert_eq!(
        eval(
            true,
            "for i = 1, 63 do api:keep('k' .. i, i) end
            local ok = pcall(api.keep, api, 'one-more', 1)
            api:keep('k63', 99)
            return (ok and 1000 or 0) + api:kept('k63')"
        ),
        Some(99)
    );
    assert_eq!(eval(false, "api:keep('z', 1) return api:kept('z')"), None);
    assert_eq!(statics.get("ui/save_load.lua", "z"), None);
    // Another script's keys don't count against this one's 64.
    for i in 0..64 {
        assert!(statics.set("ui/other.lua", &format!("k{i}"), Some(i)));
    }
    assert!(!statics.set("ui/other.lua", "one-more", Some(0)));
    assert_eq!(
        eval(true, "api:keep('k1', 5) return api:kept('k1')"),
        Some(5)
    );
}

/// The load screen left without a load has `fg_system`'s 0x400 up from its
/// exit's first tick (`title_load_empty`'s 821), for the title to find.
#[test]
fn a_load_screen_left_without_a_load_says_so() {
    let dir = folder("no-load");
    let mut screen = open_load(&dir);
    to_the_list(&mut screen, &dir);
    assert_eq!(sv(&screen)[3], 1, "load mode");
    let cancel = UiInput {
        cancel: true,
        ..UiInput::default()
    };
    tick(&mut screen, &dir, cancel);
    assert_eq!(sv(&screen)[2], 99);
    assert_eq!(screen.value("sv_fg_system"), 0);
    tick(&mut screen, &dir, UiInput::default());
    assert_eq!(sv(&screen)[..2], [4, 1]);
    assert_eq!(screen.value("sv_fg_system"), 0x400);
}

/// Classic REbirth's print as a save's label shows it (`title_load_game`):
/// a glyph a pixel closer than its width, the digits, `/` and `-` a pixel
/// later and a row up, and `.` and `:` a pixel later on the letters' row.
#[test]
fn classic_rebirths_print_sets_the_digits_later_and_a_row_up() {
    let dir = folder("cr-print");
    let screen = open(&dir);
    let placed: String = screen.test_eval(
        r#"local print = require("ui/cr_text").print
        local out, api = {}, {}
        function api:text_width() return 8 end
        function api:text(x, y, glyph) out[#out + 1] = glyph .. "@" .. x .. "," .. y end
        print(api, 0, 10, "nA1/-.:9n")
        return table.concat(out, " ")"#,
    );
    assert_eq!(
        placed,
        "n@0,10 A@7,10 1@15,9 /@22,9 -@29,9 .@36,10 :@43,10 9@50,9 n@56,10"
    );
}

/// The load (`title_load_game`'s 800 to 809): the decide's cue and state
/// 10, the read on the tick after (state 0x61, with the save's count), the
/// exit's fade as after a save, and the load on the tick the screen ends.
#[test]
fn a_decide_on_a_save_loads_it_as_the_screen_ends() {
    let dir = folder("load-game");
    saves_in(&dir, 1);
    let mut screen = open_load(&dir);
    to_the_list(&mut screen, &dir);
    assert_eq!(sv(&screen)[2..], [2, 1, 0, 0, 2, 0, 0, 0, 1]);
    let (result, cues) = tick(&mut screen, &dir, press(|input| input.confirm = true));
    assert_eq!(
        (result, cues),
        (ScreenResult::Continue, vec![UiCue::Core(6)])
    );
    assert_eq!((sv(&screen)[2], screen.value("sv_count")), (10, 0), "800");
    let (result, cues) = tick(&mut screen, &dir, UiInput::default());
    assert_eq!((result, cues), (ScreenResult::Continue, vec![]));
    assert_eq!((sv(&screen)[2], screen.value("sv_count")), (0x61, 1), "801");
    // 702 to 708 of the recording's shape: six steps of the fade and the
    // tick it stands full, with nothing for the title to clear.
    for step in [1, 1, 1, 1, 1, 1, 2] {
        let (result, _) = tick(&mut screen, &dir, UiInput::default());
        assert_eq!(result, ScreenResult::Continue);
        assert_eq!(sv(&screen)[..3], [4, step, 0x61]);
        assert_eq!(screen.value("sv_fg_system"), 0);
    }
    let (result, _) = tick(&mut screen, &dir, UiInput::default());
    assert_eq!(
        result,
        ScreenResult::Done(ScreenAction::LoadFile(dir.join("save00.dat"))),
        "809"
    );
}

/// Left and right held step the name scroll by 8 between 0 and 21, each
/// step with the move cue and the cursor's wait, and nothing at either end;
/// a held direction keeps a decide from being taken (`save_200_scroll`'s
/// 600 to 720).
#[test]
fn left_and_right_step_the_name_scroll_between_0_and_21() {
    let dir = folder("name-scroll");
    let mut screen = open(&dir);
    to_the_list(&mut screen, &dir);
    let left = UiInput {
        held_left: true,
        ..UiInput::default()
    };
    let right = UiInput {
        held_right: true,
        ..UiInput::default()
    };
    let (_, cues) = tick(&mut screen, &dir, left);
    assert!(cues.is_empty(), "nothing below 0");
    assert_eq!(sv(&screen)[7], 0, "and no wait");
    for (held, expected) in [(right, [8, 16, 21]), (left, [13, 5, 0])] {
        let mut steps = Vec::new();
        for at in 0..20 {
            let (_, cues) = tick(&mut screen, &dir, held);
            if cues.is_empty() {
                continue;
            }
            assert_eq!(cues, [UiCue::Core(4)]);
            assert_eq!(sv(&screen)[7], 3);
            steps.push((at, screen.value("sv_scroll")));
        }
        // A step every fourth tick.
        assert_eq!(steps, [0, 4, 8].map(|at| (at, expected[at / 4])));
        assert_eq!(sv(&screen)[7], 0, "no wait once it stands");
    }
    // The decide on "Create New" would write; with a direction held it
    // isn't read.
    let (_, cues) = tick(
        &mut screen,
        &dir,
        UiInput {
            confirm: true,
            ..left
        },
    );
    assert!(cues.is_empty());
    assert_eq!(sv(&screen)[2], 2);
    // Up and down come first.
    let (_, cues) = tick(
        &mut screen,
        &dir,
        UiInput {
            held_right: true,
            ..held_down()
        },
    );
    assert_eq!(cues, [UiCue::Core(4)]);
    assert_eq!((sv(&screen)[4], screen.value("sv_scroll")), (1, 0));
}

/// The leave row's footer says what leaving does: "Do not save" here, as
/// the load screen's says "Do not load"; the new file's row has none.
#[test]
fn the_leave_row_has_its_footer_in_save_mode() {
    let dir = folder("leave-footer");
    let font = solid_font();
    let footer_drawn = |screen: &mut LuaScreen| {
        let mut cx = context(&dir);
        cx.font = Some(&font);
        let mut framebuffer = Framebuffer::new();
        screen.draw(&cx, &mut framebuffer);
        assert!(!screen.failed(), "the draw ran");
        (216..230usize).any(|y| {
            (20..300usize).any(|x| {
                let at = (y * framebuffer.width as usize + x) * 4;
                framebuffer.rgba[at..at + 3] != [0, 0, 0]
            })
        })
    };
    let mut screen = open(&dir);
    to_the_list(&mut screen, &dir);
    assert!(!footer_drawn(&mut screen), "the new file's row");
    tick(&mut screen, &dir, held_down());
    assert_eq!(sv(&screen)[4], 1, "on the leave row");
    assert!(footer_drawn(&mut screen));
}

#[test]
fn cancel_and_leave_both_exit_without_a_write() {
    let cancel = UiInput {
        cancel: true,
        ..UiInput::default()
    };
    let confirm = UiInput {
        confirm: true,
        ..UiInput::default()
    };
    for leave in [false, true] {
        let dir = folder(if leave { "leave" } else { "cancel" });
        let mut screen = open(&dir);
        to_the_list(&mut screen, &dir);
        if leave {
            tick(&mut screen, &dir, held_down());
            for _ in 0..3 {
                tick(&mut screen, &dir, UiInput::default());
            }
            assert_eq!(sv(&screen)[4], 1, "on the leave row");
        }
        let (_, cues) = tick(&mut screen, &dir, if leave { confirm } else { cancel });
        assert_eq!(cues, [UiCue::Core(5)]);
        assert_eq!(sv(&screen)[2], 99);
        assert_eq!(screen.value("sv_fade3_level"), 0, "fade 3 stands");
        tick(&mut screen, &dir, UiInput::default());
        assert_eq!(sv(&screen)[..2], [4, 1]);
        assert_eq!(screen.value("sv_fade3_level"), -1, "the exit takes it");
        assert_eq!(screen.value("sv_fg_system"), 0, "the bit is load mode's");
        let mut ticks = 0;
        while tick(&mut screen, &dir, UiInput::default()).0 == ScreenResult::Continue {
            ticks += 1;
        }
        assert_eq!(ticks, 6);
        assert!(screen.take_outcome().is_none());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    }
}

#[test]
fn a_failed_write_still_reveals_the_label() {
    // A save folder that can't be written: a file stands in its place.
    let parent = folder("failed");
    let dir = parent.join("not-a-folder");
    std::fs::write(&dir, b"").unwrap();
    let mut screen = open(&dir);
    to_the_list(&mut screen, &dir);
    tick(
        &mut screen,
        &dir,
        UiInput {
            confirm: true,
            ..UiInput::default()
        },
    );
    tick(&mut screen, &dir, UiInput::default());
    // `string` is 0 in save mode, so the rollback doesn't run: the label
    // types, and the ribbon and the count go as on a written save.
    assert_eq!(sv(&screen)[2], 0x22);
    assert!(
        screen
            .take_outcome()
            .is_some_and(|outcome| outcome.ink_ribbon)
    );
    let (_, cues) = tick(&mut screen, &dir, UiInput::default());
    assert_eq!(cues, [UiCue::RoomSound(0x22)]);
}

#[test]
fn the_save_list_reads_player_count_and_place() {
    let dir = folder("list");
    saves_in(&dir, 2);
    let mut screen = open(&dir);
    to_the_list(&mut screen, &dir);
    // Newest first: the second save (count 2), then the first (count 1),
    // each with its player, count and room for the footer.
    let rows: (u8, u8, u8, u8, u8, u8) = screen.test_eval(
        "local a, b = M.state.saves[1], M.state.saves[2]
         return a.character, a.saves, a.stage, a.room, b.character, b.saves",
    );
    assert_eq!(rows, (0, 2, 2, 0, 0, 1));
}

fn held_up() -> UiInput {
    UiInput {
        up: true,
        held_up: true,
        ..UiInput::default()
    }
}

#[test]
fn more_saves_than_rows_scroll_and_up_wraps_to_the_bottom() {
    let dir = folder("scroll");
    // Eleven saves: the new file, the saves and leave are 13 rows, 10 shown.
    saves_in(&dir, 11);
    let mut screen = open(&dir);
    to_the_list(&mut screen, &dir);
    assert_eq!(sv(&screen)[6], 10, "the rows shown");
    // Up from the top wraps to the last row: the cursor on the bottom one
    // shown, the list scrolled to its end.
    tick(&mut screen, &dir, held_up());
    assert_eq!(sv(&screen)[4..6], [9, 3]);
    for _ in 0..3 {
        tick(&mut screen, &dir, UiInput::default());
    }
    // Down from the last row wraps to the top.
    tick(&mut screen, &dir, held_down());
    assert_eq!(sv(&screen)[4..6], [0, 0]);
    for _ in 0..3 {
        tick(&mut screen, &dir, UiInput::default());
    }
    // Down past the bottom one shown scrolls the list a row at a time.
    let mut places = Vec::new();
    for _ in 0..12 {
        tick(&mut screen, &dir, held_down());
        places.push((sv(&screen)[4], sv(&screen)[5]));
        for _ in 0..3 {
            tick(&mut screen, &dir, UiInput::default());
        }
    }
    let expected: Vec<(i64, i64)> = (1..=9)
        .map(|cursor| (cursor, 0))
        .chain([(9, 1), (9, 2), (9, 3)])
        .collect();
    assert_eq!(places, expected);
}

#[test]
fn only_re2_saves_are_listed_and_load_mode_adds_the_autosaves() {
    let dir = folder("kinds");
    saves_in(&dir, 2);
    write_at(&dir, "savedat1.dat", &re_block(4, false), 5);
    write_at(&dir, "autosave1.dat", &block(3), 6);
    write_at(&dir, "autosave2.dat", &block(3), 7);
    let names = |screen: &LuaScreen| -> Vec<String> {
        screen.test_eval(
            "local out = {}
             for i, entry in ipairs(M.state.saves) do out[i] = entry.name end
             return out",
        )
    };
    // Save mode: the original's list, the RE 2 saves alone.
    let mut screen = open(&dir);
    to_the_list(&mut screen, &dir);
    assert_eq!(names(&screen), ["save01.dat", "save00.dat"]);
    assert_eq!(
        sv(&screen)[6..],
        [4, 0, 0, 0, 2],
        "new, 2 saves, leave; cnt0 2"
    );
    // Load mode: the autosaves follow, newest first, before the leave row.
    let mut screen = open_load(&dir);
    to_the_list(&mut screen, &dir);
    assert_eq!(
        names(&screen),
        ["save01.dat", "save00.dat", "autosave1.dat", "autosave2.dat"]
    );
    assert_eq!(sv(&screen)[3], 1, "load mode");
    assert_eq!(
        sv(&screen)[6..],
        [5, 0, 0, 0, 2],
        "4 rows and leave; cnt0 2"
    );
}
