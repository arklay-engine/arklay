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
