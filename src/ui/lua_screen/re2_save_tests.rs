//! RE 2's save screen (`games/re2/ui/save_load.lua`, `Card_access` in save
//! mode) over a pack holding only the script, against the original's
//! traces (`oracle/re2.md`, "The save screen").

use std::path::{Path, PathBuf};

use super::*;

use crate::game::GameState;
use crate::pack::Pack;
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
                "ui/save_load.lua",
                include_str!("../../../games/re2/ui/save_load.lua")
                    .as_bytes()
                    .to_vec(),
            )
            .unwrap();
        Pack::from_bytes(writer.to_bytes().unwrap()).unwrap()
    })
}

/// A fresh, empty save folder for one test.
fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("arklay-re2-save-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn context(dir: &Path) -> UiContext<'_> {
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

/// Room 200's block with `saves` saves made.
fn block(saves: u8) -> SaveFile {
    let mut game = GameState::new(RoomId::parse("2000").unwrap(), &RoomState::default());
    game.is_re2_room = true;
    game.state_bytes[usize::from(crate::game::STATE_BYTE_SAVES)] = saves;
    SaveFile::from_state(&game)
}

/// `count` saves in `dir`, the first the oldest.
fn saves_in(dir: &Path, count: u8) {
    for index in 0..count {
        let path = dir.join(format!("save{index}.dat"));
        crate::save::save_to_path(&path, &block(index + 1)).unwrap();
        let time = std::time::SystemTime::UNIX_EPOCH
            + std::time::Duration::from_secs(1_000_000 + u64::from(index) * 60);
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(time)
            .unwrap();
    }
}

fn open(dir: &Path) -> LuaScreen {
    let mut cx = context(dir);
    LuaScreen::open_save("ui/save_load.lua", &mut cx, block(0), true).unwrap()
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
    assert_eq!(first, "save2.dat");
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
        tick(&mut screen, &dir, UiInput::default());
        assert_eq!(sv(&screen)[..2], [4, 1]);
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
