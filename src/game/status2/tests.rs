//! The item page against the original's runs (`oracle/re2.md`, "Items"):
//! each test ticks the page with the recording's presses, on its ticks, and
//! checks the `gi` and `inv` values the original traced.

use super::*;
use crate::game::{GameState, InventoryItem};
use crate::state::{RoomId, RoomState};

fn game(slots: &[(u8, u8)]) -> GameState {
    let mut game = GameState::new(RoomId::parse("1020").unwrap(), &RoomState::default());
    game.is_re2_room = true;
    game.inventory = slots
        .iter()
        .map(|&(id, quantity)| InventoryItem { id, quantity })
        .collect();
    game.re2.personal_item = [0x2F, 1];
    game.entities[0].health = 200;
    game.open_re2_items();
    game.init_re2_status();
    game
}

const DOWN: Input = Input { down: true, ..NONE };
const UP: Input = Input { up: true, ..NONE };
const RIGHT: Input = Input {
    right: true,
    ..NONE
};
const CONFIRM: Input = Input {
    confirm: true,
    ..NONE
};
const CANCEL: Input = Input {
    cancel: true,
    ..NONE
};
const NONE: Input = Input {
    up: false,
    down: false,
    left: false,
    right: false,
    confirm: false,
    cancel: false,
    start: false,
    page_up: false,
    page_down: false,
    held: 0,
};

/// Ticks the page through `from..=to` with `presses` on their ticks. A
/// message the page shows closes on the next press, as the original's does,
/// with the answer `answer` for a question.
struct Run {
    game: GameState,
    tick: u32,
    log: Vec<(u32, String, String)>,
}

impl Run {
    fn new(game: GameState, tick: u32) -> Self {
        Self {
            game,
            tick,
            log: Vec::new(),
        }
    }

    fn until(&mut self, to: u32, presses: &[(u32, Input)]) -> &mut Self {
        while self.tick < to {
            self.tick += 1;
            let input = presses
                .iter()
                .find(|(tick, _)| *tick == self.tick)
                .map_or(NONE, |(_, input)| *input);
            let up = self.game.message.active;
            let outcome = self
                .game
                .tick_re2_items(if up { NONE } else { input })
                .unwrap();
            if let Some(item) = outcome.describe {
                self.game.show_re2_description(item, &[0x01]);
            }
            // The message's tick, after the page's: a press closes it.
            if up && input.confirm {
                self.game.message.active = false;
                let choice = self.game.message.menu_choice_id() & 1;
                self.game.message.set_menu_choice_id(choice);
            }
            let page = self.game.re2_items.unwrap();
            self.log.push((self.tick, page.trace(), self.inventory()));
        }
        self
    }

    fn inventory(&self) -> String {
        (0..8)
            .map(|slot| {
                let [id, count] = self.game.re2_slot(slot);
                format!("{id:x}:{count}")
            })
            .collect::<Vec<_>>()
            .join(",")
    }

    fn at(&self, tick: u32) -> (&str, &str) {
        let (_, page, inventory) = self
            .log
            .iter()
            .find(|(t, ..)| *t == tick)
            .unwrap_or_else(|| panic!("tick {tick} not run"));
        (page.as_str(), inventory.as_str())
    }

    /// The first tick whose page trace is `page`.
    fn first(&self, page: &str) -> u32 {
        self.log
            .iter()
            .find(|(_, p, _)| p == page)
            .unwrap_or_else(|| panic!("no tick with {page}"))
            .0
    }
}

/// `inv_use`: down to the green herb, open the menu, USE it with 100 life.
#[test]
fn a_green_herb_heals_after_the_panel_and_its_slot_closes() {
    let mut start = game(&[(0x02, 18), (0x01, 1), (0x26, 1), (0x27, 1)]);
    start.entities[0].health = 100;
    let mut run = Run::new(start, 399);
    run.until(500, &[(400, DOWN), (420, CONFIRM), (440, CONFIRM)]);
    // The menu opens over five ticks with its cursor on USE.
    assert_eq!(run.at(420).0, "3,0,0,0,2,0,3,0");
    assert_eq!(run.at(424).0, "3,4,0,0,2,0,3,0");
    assert_eq!(run.at(425).0, "1,0,0,0,2,0,3,0");
    // USE: the panel sweeps for 40 ticks, then the life changes.
    assert_eq!(run.at(441).0, "2,3,4,0,2,2,3,1");
    assert_eq!(run.at(481).0, "2,3,4,0,2,2,3,0");
    assert_eq!(run.game.entities[0].health, 150);
    assert_eq!(run.at(482).0, "2,3,2,0,2,2,3,0");
    // The herb's slot empties, then the red herb moves into it.
    assert_eq!(
        run.at(483),
        ("2,1,2,0,2,2,3,0", "2:18,1:1,0:0,27:1,0:0,0:0,0:0,0:0")
    );
    assert_eq!(run.at(486).1, "2:18,1:1,0:0,27:1,0:0,0:0,0:0,0:0");
    assert_eq!(
        run.at(487),
        ("2,1,2,0,2,2,3,0", "2:18,1:1,27:1,0:0,0:0,0:0,0:0,0:0")
    );
    assert_eq!(run.at(491).0, "4,0,0,0,2,2,3,0");
    assert_eq!(run.at(496).0, "0,0,0,0,2,2,3,0");
}

/// `inv_use_full`: the same at full life. The herb is used up all the same.
#[test]
fn a_herb_used_at_full_life_is_used_up() {
    let mut run = Run::new(game(&[(0x02, 18), (0x01, 1), (0x26, 1), (0x27, 1)]), 399);
    run.until(500, &[(400, DOWN), (420, CONFIRM), (440, CONFIRM)]);
    assert_eq!(run.game.entities[0].health, 200);
    assert_eq!(run.at(487).1, "2:18,1:1,27:1,0:0,0:0,0:0,0:0,0:0");
    assert_eq!(run.at(496).0, "0,0,0,0,2,2,3,0");
}

/// USE of a key item reads `fg_use` (bank 10), not `fg_system`, and gives
/// the scripts the item in variable 2.
#[test]
fn a_key_item_is_used_where_fg_use_has_it() {
    const MEDAL: u8 = 0x47;
    let used = |bank: u8| {
        let mut start = game(&[(0x02, 18), (0x01, 1), (MEDAL, 1)]);
        start.re2.set_flag(bank, MEDAL, 1);
        let mut run = Run::new(start, 399);
        run.until(460, &[(400, DOWN), (420, CONFIRM), (440, CONFIRM)]);
        run
    };
    let run = used(crate::scd2::host::FG_USE_BANK);
    assert_eq!(
        run.game.re2.vars[crate::scd2::host::VAR_USED_ITEM],
        u16::from(MEDAL)
    );
    assert!(!run.game.message.active);
    assert_eq!(run.game.re2_items.unwrap().page, 0, "the screen closes");
    // Bank 0's bit is `fg_system`'s: "not now", and no item for the scripts.
    let run = used(0);
    assert_eq!(run.game.re2.vars[crate::scd2::host::VAR_USED_ITEM], 0);
    assert!(run.game.message.active);
}

/// `inv_use`'s second USE: the red herb, which needs a mix.
#[test]
fn a_red_herb_is_not_needed_now() {
    let mut run = Run::new(game(&[(0x02, 18), (0x01, 1), (0x27, 1)]), 399);
    run.until(
        700,
        &[(400, DOWN), (560, CONFIRM), (580, CONFIRM), (640, CONFIRM)],
    );
    assert_eq!(run.at(581).0, "2,3,1,0,2,2,3,0");
    assert!(run.at(640).0.starts_with("2,3,1"));
    assert_eq!(run.at(641).0, "4,0,1,0,2,2,3,0");
    assert_eq!(run.at(646).0, "0,0,1,0,2,2,3,0");
    assert_eq!(run.at(646).1, "2:18,1:1,27:1,0:0,0:0,0:0,0:0,0:0");
}

/// `inv_check`: CHECK the green herb, then COMBN the knife with the red herb.
#[test]
fn check_shows_the_description_and_returns_to_the_grid() {
    let mut run = Run::new(game(&[(0x02, 18), (0x01, 1), (0x26, 1), (0x27, 1)]), 399);
    run.until(
        900,
        &[
            (400, DOWN),
            (420, CONFIRM),
            (440, DOWN),
            (460, CONFIRM),
            (600, CONFIRM),
            (680, UP),
            (700, RIGHT),
            (720, CONFIRM),
            (740, DOWN),
            (760, DOWN),
            (780, CONFIRM),
            (800, DOWN),
            (820, CONFIRM),
            (840, CANCEL),
            (860, CANCEL),
        ],
    );
    assert_eq!(run.at(461).0, "2,2,1,1,2,2,2,0");
    assert_eq!(run.at(465).0, "2,2,2,0,2,2,2,0");
    assert_eq!(run.at(470).0, "2,2,3,0,2,2,2,0");
    assert_eq!(run.at(477).0, "2,2,4,0,2,2,2,0");
    assert_eq!(run.at(487).0, "2,2,6,0,2,2,2,0");
    // The description opens 36 ticks after the confirm.
    assert_eq!(run.at(495).0, "2,2,6,8,2,2,2,0");
    assert_eq!(run.at(496).0, "2,2,7,9,2,2,2,0");
    assert_eq!(run.at(600).0, "2,2,7,9,2,2,2,0");
    assert_eq!(run.at(601).0, "2,2,8,0,2,2,2,0");
    assert_eq!(run.at(618).0, "2,2,11,0,2,2,2,0");
    assert_eq!(run.at(628).0, "2,2,13,0,2,2,2,0");
    assert_eq!(run.at(629).0, "0,0,0,0,2,2,2,0");
    // Up, right to the knife, COMBN onto the red herb: no record, a buzz.
    assert_eq!(run.at(780).0, "2,1,0,0,1,1,1,0");
    assert_eq!(run.at(800).0, "2,1,0,0,1,3,1,0");
    assert_eq!(run.at(820).0, "2,1,0,0,1,3,1,0");
    assert_eq!(run.at(840).0, "1,0,0,0,1,3,1,0");
    assert_eq!(run.at(865).0, "0,0,0,0,1,3,1,0");
    assert_eq!(run.at(865).1, "2:18,1:1,26:1,27:1,0:0,0:0,0:0,0:0");
}

/// CHECK names an item with two names as its description closes
/// (`St_com_inspect`'s step 7, `inv_check_key`): the Spade Key's class in
/// the checked flags (`0x98EBE4`), and nothing for an item with one name.
#[test]
fn check_names_a_key_when_its_description_closes() {
    let spade = (0x8000_0000u32 >> 1).to_le_bytes();
    let mut run = Run::new(game(&[(0x02, 18), (0x01, 1), (0x59, 1), (0x26, 1)]), 399);
    let presses = [
        (400, DOWN),
        (420, CONFIRM),
        (440, DOWN),
        (460, CONFIRM),
        (600, CONFIRM),
    ];
    run.until(600, &presses);
    assert_eq!(run.at(600).0, "2,2,7,9,2,2,2,0", "the description is up");
    assert_eq!(run.game.examined_flags(), [0; 4]);
    run.until(601, &presses);
    assert_eq!(run.at(601).0, "2,2,8,0,2,2,2,0");
    assert_eq!(run.game.examined_flags(), spade);
    // The name is the game's from then on: through the next room's load and
    // a save.
    run.game
        .enter_room(RoomId::parse("1030").unwrap(), &RoomState::default());
    assert_eq!(run.game.examined_flags(), spade);
    let save = crate::save::SaveFile::from_state(&run.game);
    assert_eq!(save.examined, spade);

    // The green herb, in slot 3: right of the key.
    let mut run = Run::new(game(&[(0x02, 18), (0x01, 1), (0x59, 1), (0x26, 1)]), 399);
    run.until(
        640,
        &[
            (400, DOWN),
            (410, RIGHT),
            (420, CONFIRM),
            (440, DOWN),
            (460, CONFIRM),
            (600, CONFIRM),
        ],
    );
    // It has no class: `Flg_on` sets class 0's flag, which names nothing.
    assert_eq!(run.game.examined_flags(), 0x8000_0000u32.to_le_bytes());
}

/// `inv_mix`: COMBN the green herb with the red, yes to the question.
#[test]
fn a_mix_asks_then_combines_and_closes_the_gap() {
    let mut run = Run::new(
        game(&[(0x02, 18), (0x01, 1), (0x26, 1), (0x27, 1), (0x14, 30)]),
        399,
    );
    run.until(
        700,
        &[
            (400, DOWN),
            (420, CONFIRM),
            (440, DOWN),
            (460, DOWN),
            (480, CONFIRM),
            (500, RIGHT),
            (520, CONFIRM),
            (600, CONFIRM),
        ],
    );
    assert_eq!(run.at(520).0, "2,1,11,0,2,3,1,0");
    assert_eq!(run.at(521).0, "2,1,11,1,2,3,1,0");
    assert_eq!(run.at(601).0, "2,1,4,1,2,3,1,0");
    assert_eq!(
        run.at(602),
        ("2,1,1,0,2,3,1,0", "2:18,1:1,2a:1,0:0,14:30,0:0,0:0,0:0")
    );
    assert_eq!(run.at(612).0, "2,1,1,10,2,3,1,0");
    assert_eq!(run.at(613).0, "2,1,2,0,2,3,1,0");
    assert_eq!(run.at(617).1, "2:18,1:1,2a:1,14:30,0:0,0:0,0:0,0:0");
    assert_eq!(run.first("4,0,0,0,2,3,1,0"), 621);
    assert_eq!(run.at(626).0, "0,0,0,0,2,3,1,0");
}

/// A no to the question goes back to choosing the target.
#[test]
fn a_mix_refused_returns_to_the_target() {
    let mut run = Run::new(game(&[(0x26, 1), (0x26, 1)]), 0);
    run.game.message.set_menu_choice_id(0);
    run.until(
        12,
        &[
            (1, CONFIRM),
            (7, DOWN),
            (8, DOWN),
            (9, CONFIRM),
            (10, RIGHT),
            (11, CONFIRM),
        ],
    );
    assert_eq!(run.at(12).0, "2,1,11,1,0,1,1,0");
    run.game.message.active = false;
    run.game.message.set_menu_choice_id(1);
    run.until(13, &[]);
    assert_eq!(run.at(13).0, "2,1,0,0,0,1,1,0");
    assert_eq!(run.inventory(), "26:1,26:1,0:0,0:0,0:0,0:0,0:0,0:0");
}

/// `inv_reload`: COMBN the ammunition into the handgun, then the full
/// handgun with the rest, then EQUIP.
#[test]
fn ammunition_loads_the_weapon_and_a_full_one_says_so() {
    let mut start = game(&[(0x02, 5), (0x01, 1), (0x14, 30)]);
    start.set_equipped(Some(0x02));
    let mut run = Run::new(start, 399);
    run.until(
        960,
        &[
            (400, DOWN),
            (420, CONFIRM),
            (440, DOWN),
            (460, DOWN),
            (480, CONFIRM),
            (500, UP),
            (520, CONFIRM),
            (600, CONFIRM),
            (620, DOWN),
            (640, DOWN),
            (660, CONFIRM),
            (680, DOWN),
            (700, CONFIRM),
            (780, CONFIRM),
            (880, CONFIRM),
            (900, CONFIRM),
        ],
    );
    // The rounds go into the handgun, the cursor follows it.
    assert_eq!(
        run.at(521),
        ("2,1,1,0,2,0,1,0", "2:18,1:1,14:17,0:0,0:0,0:0,0:0,0:0")
    );
    assert_eq!(run.at(536).0, "4,0,0,0,0,0,1,0");
    assert_eq!(run.at(541).0, "0,0,0,0,0,0,1,0");
    // A full handgun: "It's already fully loaded.", then the grid.
    assert_eq!(run.at(702).0, "2,1,3,1,0,2,1,0");
    assert_eq!(run.at(780).0, "2,1,3,79,0,2,1,0");
    assert_eq!(run.at(781).0, "0,0,0,0,0,2,1,0");
    // EQUIP on the equipped handgun takes it off.
    assert_eq!(run.at(901).0, "4,0,0,0,0,0,3,0");
    assert_eq!(run.game.equipped, None);
    assert_eq!(run.at(906).0, "0,0,0,0,0,0,3,0");
}

#[test]
fn the_grid_reaches_the_personal_item_and_the_tabs() {
    let mut run = Run::new(game(&[(0x02, 18), (0x01, 1)]), 0);
    run.until(4, &[(1, UP), (2, DOWN), (3, UP), (4, UP)]);
    assert_eq!(run.at(1).0.split(',').nth(4), Some("10"));
    assert_eq!(run.at(2).0.split(',').nth(4), Some("1"));
    assert_eq!(run.at(3).0.split(',').nth(4), Some("10"));
    assert_eq!(run.game.re2_items.unwrap().page, PAGE_TABS);
    // The personal slot is the lighter (here the save's 0x2F).
    assert_eq!(run.game.re2_slot(PERSONAL_SLOT), [0x2F, 1]);
}

#[test]
fn the_command_menu_wraps_three_two_one() {
    let mut run = Run::new(game(&[(0x02, 18)]), 0);
    run.until(
        14,
        &[(1, CONFIRM), (7, DOWN), (8, DOWN), (9, DOWN), (10, UP)],
    );
    let menu = |tick| run.at(tick).0.split(',').nth(6).unwrap().to_string();
    assert_eq!(menu(7), "2");
    assert_eq!(menu(8), "1");
    assert_eq!(menu(9), "3");
    assert_eq!(menu(10), "1");
}

/// The custom handgun's menu (`St_menu0_item` for item 4) has command 0
/// below the others: the cursor goes 3, 2, 1, 0 and wraps both ways, and
/// command 0 turns `fg_common`'s 0x7E over and goes back to the menu on
/// it.
#[test]
fn the_custom_handguns_command_0_turns_its_flag_over() {
    use crate::scd2::host::COMMON_BANK;
    let mut run = Run::new(game(&[(0x04, 18)]), 0);
    let presses = [
        (1, CONFIRM),
        (7, DOWN),
        (8, DOWN),
        (9, DOWN),
        (10, DOWN),
        (11, UP),
        (12, CONFIRM),
        (20, CONFIRM),
    ];
    run.until(13, &presses);
    assert!(run.game.re2_items.unwrap().burst);
    let menu = |run: &Run, tick| run.at(tick).0.split(',').nth(6).unwrap().to_string();
    assert_eq!(
        [7, 8, 9, 10, 11].map(|tick| menu(&run, tick)),
        ["2", "1", "0", "3", "0"]
    );
    // The decide on 12, the command on 13: back to the menu (sub 1) on
    // command 0, the flag up; and down again by the second.
    assert_eq!(run.at(13).0, "1,0,0,0,0,0,0,0");
    assert!(run.game.re2.flag(COMMON_BANK, BURST_FLAG));
    // The command's tick draws the menu before the flag turns over
    // (`status_burst_102`'s 561 and 621): its label changes on the next.
    let label = |run: &Run| run.game.re2_items.unwrap().burst_label;
    assert!(!label(&run));
    run.until(14, &presses);
    assert!(label(&run));
    run.until(21, &presses);
    assert_eq!(run.at(21).0, "1,0,0,0,0,0,0,0");
    assert!(!run.game.re2.flag(COMMON_BANK, BURST_FLAG));
    assert!(label(&run));
    run.until(22, &presses);
    assert!(!label(&run));
}

#[test]
fn a_part_upgrades_the_equipped_weapon() {
    let mut start = game(&[(0x02, 9), (0x20, 1)]);
    start.set_equipped(Some(0x02));
    let mut run = Run::new(start, 0);
    run.until(
        40,
        &[
            (1, CONFIRM),
            (7, DOWN),
            (8, DOWN),
            (9, CONFIRM),
            (10, RIGHT),
            (11, CONFIRM),
        ],
    );
    assert_eq!(run.inventory(), "4:18,0:0,0:0,0:0,0:0,0:0,0:0,0:0");
    assert_eq!(run.game.equipped, Some(0x04));
}

#[test]
fn closing_compacts_the_inventory() {
    let mut game = game(&[(0x02, 18), (0x01, 1)]);
    assert_eq!(game.inventory.len(), 8);
    game.close_re2_items();
    assert_eq!(game.inventory.len(), 2);
    assert!(game.re2_items.is_none());
}

/// `Status`'s start clears the screen's work (`inv_cursor_reopen`, 606):
/// the cursors a close left are gone on the tick before the page's first,
/// and the page starts at the first slot again.
#[test]
fn the_screens_start_clears_the_work_the_last_close_left() {
    let mut run = Run::new(game(&[(0x02, 18), (0x01, 1), (0x26, 1), (0x27, 1)]), 0);
    // Down to slot 2, right to the red herb in slot 3, its commands and
    // CHECK's row.
    run.until(24, &[(4, DOWN), (8, RIGHT), (12, CONFIRM), (22, DOWN)]);
    let page = run.game.re2_items.unwrap();
    assert_eq!((page.cursor, page.command_cursor), (3, 2));
    let left = run.game.re2_items_trace.clone();
    assert_ne!(left, ItemPage::CLEARED_TRACE);
    run.game.close_re2_items();
    assert_eq!(run.game.re2_items_trace, left, "a close leaves the work");
    run.game.open_re2_items();
    assert_eq!(run.game.re2_items_trace, left, "until the screen starts");
    run.game.init_re2_status();
    assert_eq!(run.game.re2_items_trace, ItemPage::CLEARED_TRACE);
    // The page's first tick, at the first slot and the first command.
    run.until(25, &[]);
    assert_eq!(run.at(25).0, ItemPage::CLEARED_TRACE);
    let page = run.game.re2_items.unwrap();
    assert_eq!((page.cursor, page.command_cursor), (0, 0));
}

#[test]
fn every_record_names_a_step_the_page_has() {
    for recipe in RECIPES {
        assert!(
            matches!(recipe.step, 4..=8 | 11),
            "{:x} onto {:x}",
            recipe.item,
            recipe.target
        );
        assert_eq!(recipe.result != 0, matches!(recipe.step, 4 | 11));
    }
    assert_eq!(recipe(0x26, 0x27).map(|r| r.result), Some(0x2A));
    assert_eq!(recipe(0x01, 0x27), None);
}

#[test]
fn the_cues_are_the_core_banks_slots_4_to_7() {
    assert_eq!(Sound::Cursor.id(), 0x0404_0000);
    assert_eq!(Sound::Cancel.id(), 0x0405_0000);
    assert_eq!(Sound::Decide.id(), 0x0406_0000);
    assert_eq!(Sound::Buzzer.id(), 0x0407_0000);
}

/// Ticks the screen with `input` on the first tick until `done`, returning
/// the ticks it took and the sounds it played.
fn ticks_until(
    game: &mut GameState,
    input: Input,
    done: impl Fn(&ItemPage, &files::FileWork) -> bool,
) -> (u32, Vec<Sound>) {
    let mut sounds = Vec::new();
    let mut ticks = 0;
    while !done(&game.re2_items.unwrap(), &game.re2_file) {
        assert!(ticks < 100, "stuck at {:?}", game.re2_items);
        let out = game
            .tick_re2_items(if ticks == 0 { input } else { NONE })
            .unwrap();
        sounds.extend(out.sounds);
        ticks += 1;
    }
    (ticks, sounds)
}

/// One tick with `input`, its sounds.
fn tick(game: &mut GameState, input: Input) -> Vec<Sound> {
    game.tick_re2_items(input).unwrap().sounds
}

fn page(game: &GameState) -> ItemPage {
    game.re2_items.unwrap()
}

/// The screen on `page_byte`'s sub `sub`.
fn on(game: &mut GameState, page_byte: u8, sub: u8) {
    let page = game.re2_items.as_mut().unwrap();
    page.page = page_byte;
    page.sub = sub;
    page.command = 0;
}

const LEFT: Input = Input { left: true, ..NONE };

/// `status_map`: the map loads in a tick and slides in over 14, the last
/// with the map's cue; the cancel key slides it out over 14 more.
#[test]
fn the_map_slides_in_and_out_over_fourteen_ticks() {
    let mut game = game(&[(0x02, 18)]);
    on(&mut game, PAGE_MAP, MAP_LOAD);
    let (ticks, sounds) = ticks_until(&mut game, NONE, |p, _| p.sub == MAP_SHOWN);
    assert_eq!((ticks, sounds), (15, vec![Sound::Map]));
    // Shown, only the cancel key does anything.
    let (ticks, sounds) = ticks_until(&mut game, CANCEL, |p, _| p.page == PAGE_TABS);
    assert_eq!((ticks, sounds), (15, vec![Sound::Cancel]));
}

/// A game in room 201 (floor 2) with the MAP page shown, as
/// `map_floors_201` has it once the map has slid in.
fn map_shown(floors: &[u8]) -> GameState {
    let mut game = game(&[(0x02, 18)]);
    game.id = RoomId::parse("2010").unwrap();
    game.re2_room_set();
    for &floor in floors {
        game.re2.set_flag(map::FLOOR_BANK, floor, 1);
    }
    on(&mut game, PAGE_MAP, MAP_LOAD);
    ticks_until(&mut game, NONE, |p, _| p.sub == MAP_SHOWN);
    game
}

/// `map_floors_201`'s tick 520 and 521: up changes to floor 3 on the press
/// with the cursor cue and sub 4, and the next tick loads it, with the
/// blink at 2, rising.
#[test]
fn up_changes_floor_when_its_bit_is_set() {
    let mut game = map_shown(&[3]);
    assert_eq!(game.re2_file.map_floor, 2);
    let level = game.re2_file.blink.level;
    assert_eq!(tick(&mut game, UP), vec![Sound::Cursor]);
    assert_eq!((game.re2_file.map_floor, page(&game).sub), (3, MAP_FLOOR));
    // The blink steps before the change.
    assert_eq!(game.re2_file.blink.level, level + 2);
    assert_eq!(game.re2_map_trace(), format!("3,{},0", level + 2));
    assert_eq!(tick(&mut game, NONE), vec![]);
    assert_eq!(page(&game).sub, MAP_SHOWN);
    assert_eq!(game.re2_file.blink, Blink::RESET);
    assert_eq!(game.re2_map_trace(), "3,2,0");
    // And back down: floor 2's bit is the room load's.
    assert_eq!(tick(&mut game, DOWN), vec![Sound::Cursor]);
    assert_eq!((game.re2_file.map_floor, page(&game).sub), (2, MAP_FLOOR));
}

#[test]
fn no_floor_change_without_the_bit_or_below_floor_2() {
    // Floor 2 without floor 3's bit, and with no floor 5 below.
    let mut game = map_shown(&[]);
    assert_eq!(tick(&mut game, UP), vec![]);
    assert_eq!(tick(&mut game, DOWN), vec![]);
    assert_eq!((game.re2_file.map_floor, page(&game).sub), (2, MAP_SHOWN));
    // Floor 4 has none above it.
    let mut game = map_shown(&[3, 4]);
    tick(&mut game, UP);
    tick(&mut game, NONE);
    tick(&mut game, UP);
    tick(&mut game, NONE);
    assert_eq!(game.re2_file.map_floor, 4);
    assert_eq!(tick(&mut game, UP), vec![]);
    assert_eq!((game.re2_file.map_floor, page(&game).sub), (4, MAP_SHOWN));
    // Floors 0 and 1 change to none, whatever is set.
    let every = |_: u8, _: u8| true;
    assert_eq!(map::floor_after(0, true, true, every), 0);
    assert_eq!(map::floor_after(1, true, true, every), 1);
    assert_eq!(map::floor_after(5, true, false, every), 2);
    assert_eq!(map::floor_after(8, true, false, every), 7);
    assert_eq!(map::floor_after(7, false, true, every), 8);
}

/// Up is taken first and down from where it left the floor, so both
/// together come back to the floor shown: no load and no cue.
#[test]
fn up_then_down_in_one_tick() {
    let mut game = map_shown(&[3]);
    let both = Input { down: true, ..UP };
    assert_eq!(tick(&mut game, both), vec![]);
    assert_eq!((game.re2_file.map_floor, page(&game).sub), (2, MAP_SHOWN));
}

/// The cancel test comes after the floor change: both cues play, the
/// floor has changed and the map slides out.
#[test]
fn cancel_wins_over_a_floor_change() {
    let mut game = map_shown(&[3]);
    let both = Input { cancel: true, ..UP };
    assert_eq!(tick(&mut game, both), vec![Sound::Cursor, Sound::Cancel]);
    assert_eq!((game.re2_file.map_floor, page(&game).sub), (3, MAP_OUT));
}

#[test]
fn each_opening_of_the_map_starts_on_the_players_floor() {
    let mut game = map_shown(&[3]);
    tick(&mut game, UP);
    tick(&mut game, NONE);
    ticks_until(&mut game, CANCEL, |p, _| p.page == PAGE_TABS);
    // The floor byte keeps the floor left on until the page loads again.
    assert_eq!(game.re2_file.map_floor, 3);
    assert_eq!(game.re2_map_trace(), "-");
    // The cancel's tick stepped the blink once more.
    on(&mut game, PAGE_MAP, MAP_LOAD);
    assert_eq!(game.re2_map_trace(), "3,4,0");
    tick(&mut game, NONE);
    assert_eq!(game.re2_file.map_floor, 2);
}

/// `status_file`: the binders slide in over 13 ticks, turn over 13 with
/// cue 10 (wrapping 0 to 2), and slide out on the cancel key.
#[test]
fn the_binders_slide_in_turn_and_slide_out() {
    let mut game = game(&[(0x02, 18)]);
    on(&mut game, PAGE_FILE, FILE_LOAD);
    let (ticks, sounds) = ticks_until(&mut game, NONE, |p, _| p.sub == FILE_SHOWN);
    assert_eq!((ticks, sounds), (13, vec![]));
    // The decide key does nothing: no binder holds a file.
    tick(&mut game, CONFIRM);
    assert_eq!(page(&game).sub, FILE_SHOWN);
    let (ticks, sounds) = ticks_until(&mut game, LEFT, |p, w| p.sub == FILE_SHOWN && w.binder != 0);
    assert_eq!(
        (ticks, sounds, game.re2_file.binder),
        (14, vec![Sound::Turn], BINDERS - 1)
    );
    let (ticks, sounds) = ticks_until(&mut game, CANCEL, |p, _| p.page == PAGE_TABS);
    assert_eq!((ticks, sounds), (14, vec![Sound::Cancel]));
}

/// A screen on FILE's carousel, shown, with `held` in binder 0's slots.
fn file_game(held: &[u8]) -> GameState {
    let mut game = game(&[(0x02, 18)]);
    for (slot, &file) in held.iter().enumerate() {
        game.re2.files[slot] = file;
    }
    on(&mut game, PAGE_FILE, FILE_LOAD);
    ticks_until(&mut game, NONE, |p, _| p.sub == FILE_SHOWN);
    game
}

/// From the carousel, open binder 0 and wait for its list.
fn open_list(game: &mut GameState) {
    assert_eq!(tick(game, CONFIRM), vec![Sound::Decide]);
    ticks_until(game, NONE, |p, _| p.sub == FILE_LIST);
}

/// From the list, open the file under the cursor and wait for its page.
fn open_file(game: &mut GameState) {
    assert_eq!(tick(game, CONFIRM), vec![Sound::Decide]);
    ticks_until(game, NONE, |p, _| p.sub == FILE_PAGE);
}

#[test]
fn move_file_tag_wraps_and_skips_empty_slots() {
    let mut held = [NO_FILE; crate::scd2::host::FILE_SLOTS];
    held[0] = 1;
    held[1] = 3;
    let mut slot = 1;
    files::move_file_tag(&held, 0, &mut slot, false);
    assert_eq!(slot, 0);
    files::move_file_tag(&held, 0, &mut slot, true);
    assert_eq!(slot, 1);
    // One file: neither way moves.
    held[1] = NO_FILE;
    let mut slot = 0;
    files::move_file_tag(&held, 0, &mut slot, true);
    files::move_file_tag(&held, 0, &mut slot, false);
    assert_eq!(slot, 0);
    // An empty binder leaves the slot as it was.
    let mut slot = 5;
    files::move_file_tag(&held, 1, &mut slot, false);
    assert_eq!(slot, 5);
    // A file in a later slot only: down finds it.
    held[0] = NO_FILE;
    held[6] = 2;
    let mut slot = 0;
    files::move_file_tag(&held, 0, &mut slot, false);
    assert_eq!(slot, 6);
}

#[test]
fn file_sub0_resets_the_binder_slot_and_zoom() {
    let mut game = game(&[(0x02, 18)]);
    game.re2_file.binder = 2;
    game.re2_file.slot = 4;
    game.re2_file.zoom = 100;
    game.re2_file.turn = 7;
    on(&mut game, PAGE_FILE, FILE_LOAD);
    tick(&mut game, NONE);
    let work = game.re2_file;
    assert_eq!(
        (work.binder, work.slot, work.zoom, work.turn),
        (0, 0, 0, 0x400)
    );
    // Binder 0's first slot empty but a later one held: the slot finds it.
    game.re2.files[3] = 5;
    game.re2_file.slot = 1;
    on(&mut game, PAGE_FILE, FILE_LOAD);
    tick(&mut game, NONE);
    assert_eq!(game.re2_file.slot, 3);
}

#[test]
fn decide_on_a_binder_with_a_file_opens_it() {
    let mut game = file_game(&[1]);
    game.re2_file.page = 9;
    assert_eq!(tick(&mut game, CONFIRM), vec![Sound::Decide]);
    assert_eq!((page(&game).sub, game.re2_file.page), (FILE_OPEN, 0));
}

#[test]
fn the_binder_opens_spins_and_lists() {
    let mut game = file_game(&[1]);
    tick(&mut game, CONFIRM);
    let mut zooms = Vec::new();
    for _ in 0..11 {
        tick(&mut game, NONE);
        zooms.push(game.re2_file.zoom);
    }
    assert_eq!(zooms, [24, 52, 84, 120, 160, 204, 252, 304, 360, 420, 484]);
    // The view lags the zoom by a tick.
    assert_eq!(game.re2_file.view_zoom, 420);
    tick(&mut game, NONE);
    assert_eq!(page(&game).sub, FILE_SPIN_OUT);
    for _ in 0..11 {
        tick(&mut game, NONE);
        assert_eq!(page(&game).sub, FILE_SPIN_OUT);
    }
    tick(&mut game, NONE);
    assert_eq!(page(&game).sub, FILE_LIST);
    assert_eq!(game.re2_file.spin, 0x26 * 12);
    assert_eq!(game.re2_file.slot, 0);
}

#[test]
fn the_binder_closes_back_to_the_carousel() {
    let mut game = file_game(&[1]);
    open_list(&mut game);
    assert_eq!(tick(&mut game, CANCEL), vec![Sound::Cancel]);
    assert_eq!(page(&game).sub, FILE_SPIN_BACK);
    let (ticks, _) = ticks_until(&mut game, NONE, |p, _| p.sub == FILE_CLOSE);
    assert_eq!(ticks, 12);
    assert_eq!(game.re2_file.spin, 0);
    let mut zooms = Vec::new();
    for _ in 0..11 {
        tick(&mut game, NONE);
        zooms.push(game.re2_file.zoom);
    }
    assert_eq!(zooms, [460, 432, 400, 364, 324, 280, 232, 180, 124, 64, 0]);
    tick(&mut game, NONE);
    assert_eq!(page(&game).sub, FILE_SHOWN);
}

#[test]
fn the_list_moves_with_the_cursor_cue_only_on_a_change() {
    let mut game = file_game(&[1, 3]);
    open_list(&mut game);
    assert_eq!(tick(&mut game, UP), vec![Sound::Cursor]);
    assert_eq!(game.re2_file.slot, 1);
    assert_eq!(tick(&mut game, DOWN), vec![Sound::Cursor]);
    assert_eq!(game.re2_file.slot, 0);
    // Both in one tick: up then down, back where it was, no cue.
    assert_eq!(
        tick(
            &mut game,
            Input {
                up: true,
                down: true,
                ..NONE
            }
        ),
        vec![]
    );
    let mut one = file_game(&[1]);
    open_list(&mut one);
    assert_eq!(tick(&mut one, DOWN), vec![]);
}

#[test]
fn a_file_opens_after_fourteen_steps_and_loads_on_the_fifteenth() {
    let mut game = file_game(&[1]);
    open_list(&mut game);
    tick(&mut game, CONFIRM);
    assert_eq!((page(&game).sub, game.re2_file.x), (FILE_READ_IN, 25));
    for count in 1..=14 {
        assert_eq!(tick(&mut game, NONE), vec![]);
        assert_eq!(
            (page(&game).sub, page(&game).command),
            (FILE_READ_IN, count)
        );
    }
    assert_eq!(tick(&mut game, NONE), vec![Sound::Page]);
    assert_eq!((page(&game).sub, game.re2_file.x), (FILE_NEXT_IN, 312));
}

#[test]
fn the_page_slides_in_to_25() {
    let mut game = file_game(&[1]);
    open_list(&mut game);
    tick(&mut game, CONFIRM);
    ticks_until(&mut game, NONE, |p, _| p.sub == FILE_NEXT_IN);
    game.re2_file.blink = Blink {
        level: 40,
        falling: true,
    };
    let mut xs = Vec::new();
    for _ in 0..11 {
        tick(&mut game, NONE);
        xs.push(game.re2_file.x);
    }
    assert_eq!(xs, [216, 152, 104, 72, 56, 46, 38, 32, 28, 26, 25]);
    tick(&mut game, NONE);
    assert_eq!(page(&game).sub, FILE_PAGE);
    assert_eq!(game.re2_file.blink, Blink::RESET);
}

/// The page's x over a slide's 11 moving ticks, then the sub after its
/// 12th.
fn slide(game: &mut GameState) -> (Vec<i16>, u8) {
    let mut xs = Vec::new();
    for _ in 0..11 {
        tick(game, NONE);
        xs.push(game.re2_file.x);
    }
    tick(game, NONE);
    (xs, page(game).sub)
}

#[test]
fn next_and_previous_pages_follow_st_mv() {
    let mut game = file_game(&[1]);
    open_list(&mut game);
    open_file(&mut game);
    assert_eq!(tick(&mut game, RIGHT), vec![Sound::Page]);
    assert_eq!((page(&game).sub, game.re2_file.page), (FILE_NEXT_OUT, 1));
    let (xs, sub) = slide(&mut game);
    assert_eq!(xs, [23, 19, 13, 5, -5, -21, -53, -101, -165, -261, -373]);
    assert_eq!((sub, game.re2_file.x), (FILE_NEXT_IN, 312));
    slide(&mut game);
    assert_eq!(page(&game).sub, FILE_PAGE);
    assert_eq!(tick(&mut game, LEFT), vec![Sound::Page]);
    assert_eq!((page(&game).sub, game.re2_file.page), (FILE_PREV_OUT, 0));
    let (xs, sub) = slide(&mut game);
    assert_eq!(xs, [27, 31, 37, 45, 55, 71, 103, 151, 215, 311, 423]);
    assert_eq!((sub, game.re2_file.x), (FILE_PREV_IN, -262));
    let (xs, sub) = slide(&mut game);
    assert_eq!(xs, [-166, -102, -54, -22, -6, 4, 12, 18, 22, 24, 25]);
    assert_eq!(sub, FILE_PAGE);
    // Left on the first page does nothing.
    assert_eq!(tick(&mut game, LEFT), vec![]);
    assert_eq!(page(&game).sub, FILE_PAGE);
}

#[test]
fn right_on_the_last_page_lights_the_end_and_left_puts_it_out() {
    // File 2 has two pages.
    let mut game = file_game(&[2]);
    open_list(&mut game);
    open_file(&mut game);
    tick(&mut game, RIGHT);
    ticks_until(&mut game, NONE, |p, _| p.sub == FILE_PAGE);
    assert_eq!(game.re2_file.page, 1);
    assert_eq!(tick(&mut game, RIGHT), vec![Sound::Cursor]);
    assert_eq!(page(&game).sub, FILE_END);
    // Right again does nothing, and the blink holds.
    game.re2_file.blink.level = 30;
    assert_eq!(tick(&mut game, RIGHT), vec![]);
    assert_eq!(game.re2_file.blink.level, 30);
    assert_eq!(tick(&mut game, LEFT), vec![]);
    assert_eq!(page(&game).sub, FILE_PAGE);
    assert_eq!((game.re2_file.page, game.re2_file.blink), (1, Blink::RESET));
}

#[test]
fn both_directions_on_the_last_page() {
    let mut game = file_game(&[2]);
    open_list(&mut game);
    open_file(&mut game);
    tick(&mut game, RIGHT);
    ticks_until(&mut game, NONE, |p, _| p.sub == FILE_PAGE);
    let both = Input {
        left: true,
        right: true,
        ..NONE
    };
    assert_eq!(tick(&mut game, both), vec![Sound::Cursor, Sound::Page]);
    assert_eq!((page(&game).sub, game.re2_file.page), (FILE_PREV_OUT, 0));
}

#[test]
fn cancel_and_close_run_the_first_return_step_in_the_same_tick() {
    let mut game = file_game(&[2]);
    open_list(&mut game);
    open_file(&mut game);
    assert_eq!(tick(&mut game, CANCEL), vec![Sound::Cancel]);
    assert_eq!((page(&game).sub, page(&game).command), (FILE_READ_OUT, 1));
    let (ticks, _) = ticks_until(&mut game, NONE, |p, _| p.sub == FILE_LIST);
    assert_eq!(ticks, 13);
    // From the end marker, the decide key closes it with the decide cue.
    open_file(&mut game);
    tick(&mut game, RIGHT);
    ticks_until(&mut game, NONE, |p, _| p.sub == FILE_PAGE);
    tick(&mut game, RIGHT);
    assert_eq!(page(&game).sub, FILE_END);
    assert_eq!(tick(&mut game, CONFIRM), vec![Sound::Decide]);
    assert_eq!((page(&game).sub, page(&game).command), (FILE_READ_OUT, 1));
}

#[test]
fn the_blink_shows_rising_25_ticks_then_48_tick_periods() {
    let mut blink = Blink::RESET;
    let mut ways = Vec::new();
    for _ in 0..121 {
        blink.step_reader();
        ways.push(blink.falling);
    }
    let first_falling = ways.iter().position(|&way| way).unwrap();
    assert_eq!(first_falling, 25);
    let back = ways[25..].iter().position(|&way| !way).unwrap() + 25;
    assert_eq!(back, 49);
    let again = ways[49..].iter().position(|&way| way).unwrap() + 49;
    assert_eq!(again, 73);
}

#[test]
fn the_shared_blink_steps_each_way() {
    // The item cursor: by 3 between 0x32 and 0x78.
    let mut blink = Blink::OPEN;
    blink.step_cursor();
    assert_eq!(
        blink,
        Blink {
            level: 0x35,
            falling: false
        }
    );
    let mut top = Blink {
        level: 0x79,
        falling: false,
    };
    top.step_cursor();
    assert_eq!(
        top,
        Blink {
            level: 0x7C,
            falling: true
        }
    );
    let mut bottom = Blink {
        level: 0x31,
        falling: true,
    };
    bottom.step_cursor();
    assert_eq!(
        bottom,
        Blink {
            level: 0x2E,
            falling: false
        }
    );
    // MAP's marker: by 2 between 10 and 0x50.
    let mut map = Blink {
        level: 0x51,
        falling: false,
    };
    map.step_map();
    assert_eq!(
        map,
        Blink {
            level: 0x53,
            falling: true
        }
    );
    // The screen's opening resets it; the grid steps it and the menu holds
    // it at 0x3C on the frames the screen draws.
    let mut game = game(&[(0x02, 18)]);
    game.re2_file.blink = Blink {
        level: 9,
        falling: true,
    };
    game.open_re2_items();
    assert_eq!(game.re2_file.blink.level, 9);
    game.init_re2_status();
    assert_eq!(game.re2_file.blink, Blink::OPEN);
    game.animate_re2_items(true);
    assert_eq!(game.re2_file.blink.level, 0x35);
    game.animate_re2_items(false);
    assert_eq!(game.re2_file.blink.level, 0x35);
    tick(&mut game, CONFIRM);
    game.animate_re2_items(true);
    assert_eq!(game.re2_file.blink.level, Blink::HELD);
    // The tab bar leaves it alone, on the tick the grid leaves for it too.
    let mut tabs = self::game(&[(0x02, 18)]);
    tabs.re2_items.as_mut().unwrap().cursor = PERSONAL_SLOT;
    tick(&mut tabs, UP);
    assert_eq!(page(&tabs).page, PAGE_TABS);
    tabs.animate_re2_items(true);
    assert_eq!(tabs.re2_file.blink.level, 0x35);
    tick(&mut tabs, NONE);
    tabs.animate_re2_items(true);
    assert_eq!(tabs.re2_file.blink.level, 0x35);
}

#[test]
fn the_slot_survives_closing_and_reopening_one_binder() {
    let mut game = file_game(&[1, 3]);
    open_list(&mut game);
    tick(&mut game, UP);
    tick(&mut game, CANCEL);
    ticks_until(&mut game, NONE, |p, _| p.sub == FILE_SHOWN);
    open_list(&mut game);
    assert_eq!(game.re2_file.slot, 1);
}

#[test]
fn turning_to_an_empty_binder_keeps_the_slot() {
    let mut game = file_game(&[1, 3]);
    open_list(&mut game);
    tick(&mut game, UP);
    tick(&mut game, CANCEL);
    ticks_until(&mut game, NONE, |p, _| p.sub == FILE_SHOWN);
    tick(&mut game, RIGHT);
    ticks_until(&mut game, NONE, |p, _| p.sub == FILE_SHOWN);
    assert_eq!((game.re2_file.binder, game.re2_file.slot), (1, 1));
    // The empty binder doesn't open.
    assert_eq!(tick(&mut game, CONFIRM), vec![]);
    tick(&mut game, LEFT);
    ticks_until(&mut game, NONE, |p, _| p.sub == FILE_SHOWN);
    assert_eq!((game.re2_file.binder, game.re2_file.slot), (0, 0));
}

#[test]
fn the_file_trace_is_the_work_then_the_files() {
    let mut held = [NO_FILE; crate::scd2::host::FILE_SLOTS];
    held[0] = 1;
    held[1] = 0x13;
    let work = files::FileWork {
        slot: 1,
        page: 3,
        zoom: 484,
        turn: 0x400,
        x: -262,
        ..Default::default()
    };
    assert_eq!(
        work.trace(&held),
        format!("0,1,3,0,0,484,1024,-262/0113{}", "ff".repeat(22))
    );
}

/// One tick with `bits` of the raw pad held, the way the engine builds the
/// page's input: the directions gated on the repeat flag, the raw pad on the
/// side.
fn held_tick(game: &mut GameState, bits: u16) -> (Vec<Sound>, Input) {
    use crate::game::pad_repeat as pad;
    game.pad_repeat.step(bits);
    let repeat = game.pad_repeat;
    let input = Input {
        up: repeat.gated(pad::UP),
        down: repeat.gated(pad::DOWN),
        left: repeat.gated(pad::LEFT),
        right: repeat.gated(pad::RIGHT),
        held: repeat.raw,
        ..NONE
    };
    (tick(game, input), input)
}

#[test]
fn a_held_right_moves_the_item_cursor_at_11_then_every_7() {
    use crate::game::pad_repeat as pad;
    let items = [(2, 18), (1, 1), (0x14, 30), (0x26, 1), (0x27, 1), (0x28, 1)];
    let mut game = game(&items);
    let mut moves = Vec::new();
    for t in 0..40 {
        let before = page(&game).cursor;
        let (sounds, _) = held_tick(&mut game, pad::RIGHT);
        if page(&game).cursor != before {
            moves.push((t, page(&game).cursor));
            assert_eq!(sounds, vec![Sound::Cursor], "tick {t}");
        }
    }
    // The press, then +11 and every 7 after.
    assert_eq!(moves, [(0, 1), (11, 2), (18, 3), (25, 4), (32, 5), (39, 6)]);
    // Held on, it stops at the last of the eight slots (no wrap).
    for _ in 0..30 {
        held_tick(&mut game, pad::RIGHT);
    }
    assert_eq!(page(&game).cursor, 7);
}

/// The grid's step (`0x4F8177`) and COMBN's (`0x4F88B0`) over a two-slot
/// weapon, whose slots `Item_work` marks 1 and 2.
#[test]
fn the_cursor_steps_over_a_two_slot_weapon() {
    let step = |from: u8, marks: &[u8], input: Input, tabs: bool| {
        let mut cursor = from;
        move_cursor(&mut cursor, 8, marks, input, tabs);
        cursor
    };
    for tabs in [true, false] {
        // The weapon in the first two slots, as a pick-up leaves it.
        let first = [1, 2, 0, 0, 0, 0, 0, 0];
        assert_eq!(step(0, &first, RIGHT, tabs), 2);
        assert_eq!(step(2, &first, LEFT, tabs), 1);
        // Left from its right half has nowhere to go.
        assert_eq!(step(1, &first, LEFT, tabs), 1);
        assert_eq!(step(1, &first, RIGHT, tabs), 2);
        // Down and up are the rows', marks or not.
        assert_eq!(step(1, &first, DOWN, tabs), 3);
        assert_eq!(step(3, &first, UP, tabs), 1);
        // In the second row: right goes over it, left from its right half
        // leaves it, and left from the slot after it lands on that half.
        let second = [0, 0, 1, 2, 0, 0, 0, 0];
        assert_eq!(step(1, &second, RIGHT, tabs), 2);
        assert_eq!(step(2, &second, RIGHT, tabs), 4);
        assert_eq!(step(3, &second, LEFT, tabs), 1);
        assert_eq!(step(4, &second, LEFT, tabs), 3);
        assert_eq!(step(2, &second, LEFT, tabs), 1);
        // No marks: the plain steps.
        assert_eq!(step(2, &[0; 8], RIGHT, tabs), 3);
        assert_eq!(step(3, &[0; 8], LEFT, tabs), 2);
    }
    // In the last two slots the grid's right goes to the right half; COMBN's
    // has no such test and leaves the slots (`0x4F88F5`).
    let last = [0, 0, 0, 0, 0, 0, 1, 2];
    assert_eq!(step(6, &last, RIGHT, true), 7);
    assert_eq!(step(6, &last, RIGHT, false), 8);
    assert_eq!(step(7, &last, RIGHT, true), 7);
}

/// Presses 20 ticks apart from tick 400: `select` to the item, its COMBN,
/// `aim` for the target, then the decide key with `last`.
fn combine(start: GameState, select: &[Input], aim: &[Input], last: Input) -> Run {
    let mut presses = Vec::new();
    let mut tick = 400;
    let mut press = |input: Input| {
        presses.push((tick, input));
        tick += 20;
    };
    select.iter().copied().for_each(&mut press);
    [CONFIRM, DOWN, DOWN, CONFIRM]
        .into_iter()
        .for_each(&mut press);
    aim.iter().copied().for_each(&mut press);
    press(Input {
        confirm: true,
        ..last
    });
    let mut run = Run::new(start, 399);
    run.until(tick + 40, &presses);
    run
}

/// The item page with the sub-machine gun as a pick-up leaves it: bullets
/// combined onto either of its slots, or from either, count in both
/// (`Set_iwork_num`, `0x502500`).
#[test]
fn rounds_load_into_both_slots_of_a_two_slot_weapon() {
    let smg = crate::player2::SMG;
    let cases: [(&[Input], &[Input], u8, u8); 4] = [
        // From the bullets onto the left half, and onto the right; the
        // cursor follows the rounds to the weapon.
        (&[DOWN], &[UP], 0, 0),
        (&[DOWN], &[LEFT], 1, 1),
        // From the weapon's left half, and from its right, onto the bullets.
        (&[], &[RIGHT], 0, 2),
        (&[DOWN, LEFT], &[RIGHT], 1, 2),
    ];
    for (select, aim, cursor, target) in cases {
        let start = game(&[(smg, 40), (smg, 40), (0x1B, 100), (0x02, 18)]);
        let run = combine(start, select, aim, NONE);
        let page = run.game.re2_items.unwrap();
        assert_eq!((page.cursor, page.target), (cursor, target));
        assert_eq!(
            run.inventory(),
            "f:100,f:100,1b:40,2:18,0:0,0:0,0:0,0:0",
            "from {cursor} onto {target}"
        );
    }
}

/// `St_com_mix` moves the target and then asks `Check_item_mix`, in one
/// tick: a direction pressed with the decide key combines with the slot the
/// target lands on, not the one it left.
#[test]
fn a_combine_reads_the_target_a_move_on_its_tick_left() {
    let smg = crate::player2::SMG;
    let items = [(smg, 40), (smg, 40), (0x1B, 100), (0x02, 18)];
    // Right over the weapon to its bullets, then right and decide: the
    // handgun, which the weapon takes nothing from.
    let run = combine(game(&items), &[], &[RIGHT], RIGHT);
    let page = run.game.re2_items.unwrap();
    assert_eq!((page.target, page.step), (3, 0));
    assert_eq!(run.inventory(), "f:40,f:40,1b:100,2:18,0:0,0:0,0:0,0:0");
    // The other way: left and decide from the handgun lands on the bullets.
    let run = combine(game(&items), &[], &[RIGHT, RIGHT], LEFT);
    assert_eq!(run.inventory(), "f:100,f:100,1b:40,2:18,0:0,0:0,0:0,0:0");
}

/// COMBN's step 2 moves the equipped slot with its item as a gap closes,
/// and the close's compaction keeps it there.
#[test]
fn the_equipped_slot_follows_its_item_up_a_closing_gap() {
    let mut game = game(&[(0x26, 1), (0x02, 18), (0x01, 1)]);
    game.set_equipped(Some(0x02));
    assert_eq!(game.equipped_slot, Some(1));
    game.inventory[0] = InventoryItem::default();
    let mut page = game.re2_items.unwrap();
    game.re2_compact_step(&mut page);
    assert_eq!(game.re2_slot(0), [0x02, 18]);
    assert_eq!(game.equipped_slot, Some(0));
    game.re2_compact_step(&mut page);
    assert_eq!(game.re2_slot(1), [0x01, 1]);
    assert_eq!(game.equipped_slot, Some(0));

    // A screen closed with the gap still open.
    let mut game = self::game(&[(0x26, 1), (0x01, 1), (0x02, 18)]);
    game.set_equipped(Some(0x02));
    game.inventory[0] = InventoryItem::default();
    game.close_re2_items();
    assert_eq!(game.inventory.len(), 2);
    assert_eq!(game.equipped_slot, Some(1));
    assert_eq!(game.weapon_slot(0x02), Some(1));
}

#[test]
fn a_held_left_in_the_carousel_turns_again_at_the_end_of_a_turn() {
    use crate::game::pad_repeat as pad;
    let mut game = file_game(&[1, 3]);
    // The press turns the binders (sub 4): the flag and the key.
    let (_, _) = held_tick(&mut game, pad::LEFT);
    assert_eq!(page(&game).sub, FILE_LEFT);
    // Each turn is 13 ticks: 12 counting ticks and the end tick, whose raw
    // test starts the next turn at once, flag or not.
    let mut ends = Vec::new();
    for t in 1..40 {
        let before = work_binder(&game);
        held_tick(&mut game, pad::LEFT);
        if work_binder(&game) != before {
            ends.push(t);
            assert_eq!(page(&game).sub, FILE_LEFT, "tick {t}: the next turn");
        }
    }
    assert_eq!(ends, [13, 26, 39]);
}

fn work_binder(game: &GameState) -> u8 {
    game.re2_file.binder
}

#[test]
fn a_held_left_in_sub_3_moves_only_on_flag_ticks() {
    use crate::game::pad_repeat as pad;
    let mut game = file_game(&[1, 3]);
    // Left goes down on a tick the page doesn't see, so the carousel sits
    // in sub 3 with the key held and the flag down: the raw pad alone must
    // not turn it.
    game.pad_repeat.step(pad::LEFT);
    for t in 1..11 {
        let (_, input) = held_tick(&mut game, pad::LEFT);
        assert!(!input.left, "tick {t}");
        assert_eq!(input.held, pad::LEFT);
        assert_eq!(page(&game).sub, FILE_SHOWN, "tick {t}: no flag, no turn");
    }
    // The counter ran out: the flag's tick turns the binders.
    let (sounds, input) = held_tick(&mut game, pad::LEFT);
    assert!(input.left);
    assert_eq!(page(&game).sub, FILE_LEFT);
    assert_eq!(sounds, vec![Sound::Turn]);
}

/// `Stage_init_item`'s EASY steps: Leon's hint file or Claire's in slot 0
/// at last page 1 (none off EASY or in scenario B), the bullets and the
/// sprays; `St_init00` then grows the character's from the flags on each
/// opening.
#[test]
fn easy_new_game_holds_the_hint_file() {
    use crate::scd2::host::NO_FILE;
    for (room, file) in [("1020", 0x17), ("1021", 0x18)] {
        let mut game = GameState::new(RoomId::parse(room).unwrap(), &RoomState::default());
        game.re2.start_new_game(true, false, false);
        game.seed_re2_easy();
        assert_eq!(game.re2.files[0], file, "room {room}");
        assert_eq!(game.re2_slot(3), [0x14, 120]);
        assert_eq!(game.re2.item_box[1..4], [[0x23, 1, 0]; 3]);
        assert_eq!(game.re2_file.pages_last(file), 1);
        game.re2.set_flag(4, 0x0F, 1);
        game.init_re2_status();
        assert_eq!(game.re2_file.pages_last(file), 2);
    }
    let mut normal = GameState::new(RoomId::parse("1020").unwrap(), &RoomState::default());
    normal.re2.start_new_game(false, false, false);
    normal.seed_re2_easy();
    assert_eq!(normal.re2.files[0], NO_FILE);
    let mut scenario_b = GameState::new(RoomId::parse("1020").unwrap(), &RoomState::default());
    scenario_b.re2.scenario_b = true;
    scenario_b.re2.start_new_game(true, false, false);
    scenario_b.seed_re2_easy();
    assert_eq!(scenario_b.re2.files[0], NO_FILE);
}

/// The hint file grows in `St_init00`, mode 0's start alone (START's
/// screen and `Sce_Status`'s): the map key's (`St_init03`), a script's
/// map's and the box's leave it.
#[test]
fn only_starts_screen_grows_the_hint_file() {
    let easy = || {
        let mut game = GameState::new(RoomId::parse("1020").unwrap(), &RoomState::default());
        game.re2.start_new_game(true, false, false);
        game.seed_re2_easy();
        game.re2.set_flag(4, 0x0F, 1);
        game
    };
    let mut map_key = easy();
    map_key.request_re2_key_map(false);
    map_key.init_re2_status();
    assert_eq!(map_key.re2_file.pages_last(0x17), 1);
    let mut script_map = easy();
    script_map.request_re2_script_map(2);
    script_map.init_re2_status();
    assert_eq!(script_map.re2_file.pages_last(0x17), 1);
    let mut item_box = easy();
    item_box.open_re2_box();
    item_box.init_re2_status();
    assert_eq!(item_box.re2_file.pages_last(0x17), 1);
    let mut start = easy();
    start.init_re2_status();
    assert_eq!(start.re2_file.pages_last(0x17), 2);
}

/// `Sort_item` at `Status`'s start: the slots after the first empty one move
/// up one and the last is cleared, so EASY's bullets in slot 3 move to slot
/// 2; an inventory with no gap before its end stays as it is.
#[test]
fn the_status_start_closes_the_first_gap() {
    let mut game = GameState::new(RoomId::parse("1020").unwrap(), &RoomState::default());
    game.set_re2_slot(0, [0x01, 1]);
    game.set_re2_slot(1, [0x02, 15]);
    game.set_re2_slot(2, [0, 0]);
    game.set_re2_slot(3, [0x14, 120]);
    game.init_re2_status();
    let slots = |game: &GameState| (0..4).map(|slot| game.re2_slot(slot)).collect::<Vec<_>>();
    assert_eq!(slots(&game), [[0x01, 1], [0x02, 15], [0x14, 120], [0, 0]]);
    game.init_re2_status();
    assert_eq!(
        slots(&game),
        [[0x01, 1], [0x02, 15], [0x14, 120], [0, 0]],
        "compact"
    );
}

/// `St_com_use_equip` (`0x4F8620`) compares slots: with the handgun in hand
/// from slot 0, EQUIP on a second handgun in slot 2 equips that slot, and
/// the weapon fires from its rounds (#515).
#[test]
fn equip_compares_slots_not_ids() {
    let mut start = game(&[(0x03, 13), (0x01, 1), (0x03, 15)]);
    start.set_equipped(Some(0x03));
    assert_eq!(start.equipped_slot, Some(0));
    let mut run = Run::new(start, 399);
    run.until(560, &[(400, DOWN), (420, CONFIRM), (500, CONFIRM)]);
    assert_eq!(run.game.equipped, Some(0x03), "the second copy is equipped");
    assert_eq!(run.game.equipped_slot, Some(2));
    assert_eq!(run.game.equipped_weapon_id, 0x03);
    assert_eq!(
        run.game.re2_weapon().rounds,
        15,
        "the equipped slot's rounds"
    );
}

/// `St_init_disp_face`'s rows by the player's id (`0x4FE8C0`).
#[test]
fn the_portraits_texels_follow_the_players_id() {
    use super::portrait_texels;
    let rows = |u| [(u, 0x48), (u, 0x56), (u, 0x6B)];
    for id in [0, 1, 2, 7, 0xB] {
        assert_eq!(portrait_texels(id), rows(0), "id {id:#x}");
    }
    assert_eq!(portrait_texels(0xC), rows(0x58));
    assert_eq!(portrait_texels(0xD), [(0x2A, 0x34), (0, 0x34), (0, 0x34)]);
    assert_eq!(portrait_texels(0xE), rows(0x2C));
    for id in 0xE..=0xFF {
        assert_eq!(portrait_texels(id), rows(0x2C), "id {id:#x}");
    }
}

/// `St_load_gfx`'s sheet (`0x5016F0`): the scenario's outside Extreme
/// Battle, whatever the character byte's pick; in it the character's index,
/// so Chris draws from ST2 (his portrait and name) and Ada from Leon's.
#[test]
fn the_status_sheet_is_the_scenarios_or_the_extreme_battle_characters() {
    use super::status_sheet;
    use crate::scd2::host::{FG_SYSTEM_EXTREME, STATUS_BANK, STATUS_PLAYER, VAR_EX_CHARACTER};
    assert_eq!(status_sheet(false, None), 0);
    assert_eq!(status_sheet(true, None), 1);
    for claire in [false, true] {
        for (index, sheet) in [(0, 0), (1, 1), (2, 0), (3, 2)] {
            assert_eq!(status_sheet(claire, Some(index)), sheet, "{index}");
        }
    }
    let mut game = GameState::new(RoomId::parse("1020").unwrap(), &RoomState::default());
    assert_eq!(game.re2_status_sheet(), 0);
    game.re2.set_flag(STATUS_BANK, STATUS_PLAYER, 1);
    assert_eq!(game.re2_status_sheet(), 1);
    game.re2.set_flag(0, FG_SYSTEM_EXTREME, 1);
    game.re2.vars[VAR_EX_CHARACTER] = 3;
    assert_eq!(game.re2_status_sheet(), 2);
}

/// The partner's panel (`0x691FA0`) by `inv_check`'s ticks: CHECK's steps 1
/// and 2 move it 6 left a tick, ten ticks in all (`St_com_inspect`'s cases 1
/// and 2), and 11 and 12 bring it back (cases 0xB and 0xC); FILE's slide in
/// moves it 6 left a tick for 13 ticks and its slide out back.
#[test]
fn the_partners_panel_moves_out_for_check_and_file() {
    let mut run = Run::new(game(&[(0x02, 18), (0x01, 1), (0x26, 1), (0x27, 1)]), 399);
    let mut moves = Vec::new();
    let mut last = 0;
    for to in 400..=640 {
        run.until(
            to,
            &[
                (400, DOWN),
                (420, CONFIRM),
                (440, DOWN),
                (460, CONFIRM),
                (600, CONFIRM),
            ],
        );
        let dx = partner_dx(&run.game.re2_items.unwrap());
        if dx != last {
            moves.push((to, dx - last));
            last = dx;
        }
        if [461, 465, 470, 601, 618, 623, 628, 629].contains(&to) {
            let expected = match to {
                461 => -6,
                465 => -30,
                623 => -30,
                628 | 629 => 0,
                _ => -60,
            };
            assert_eq!(dx, expected, "tick {to}");
        }
    }
    let out: Vec<_> = (461..=470).map(|tick| (tick, -6)).collect();
    let back: Vec<_> = (619..=628).map(|tick| (tick, 6)).collect();
    assert_eq!(moves, [out, back].concat());

    let mut game = game(&[(0x02, 18)]);
    on(&mut game, PAGE_FILE, FILE_LOAD);
    let mut xs = Vec::new();
    while page(&game).sub != FILE_SHOWN {
        tick(&mut game, NONE);
        xs.push(partner_dx(&page(&game)));
    }
    assert_eq!(xs, (1..=13).map(|k| -6 * k).collect::<Vec<_>>());
    tick(&mut game, CANCEL);
    assert_eq!(partner_dx(&page(&game)), -78, "the cancel's tick");
    xs.clear();
    while page(&game).page != PAGE_TABS {
        tick(&mut game, NONE);
        xs.push(partner_dx(&page(&game)));
    }
    assert_eq!(xs, (1..=13).map(|k| -78 + 6 * k).collect::<Vec<_>>());
}
