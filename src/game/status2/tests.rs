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
