//! RE 2's typewriter: `Sce_Save` (`0x4E98F0`), the area kind 9 that
//! `Sce_at_check` fires, and its back routine `Sce_Save_backfunc`
//! (`0x4E9930`), which asks whether to use an ink ribbon.
//!
//! `Game_loop` runs two back routine slots in play. Slot 2 (`0x98E794`,
//! `pG+0x82B4`) runs right after `Sce_at`, the same tick the area fires.
//! `Sce_Save` and `Sce_Itembox` both set it, so the two overwrite each
//! other ([`BackRoutine`]). They also share the routine's state byte
//! (`0x98E542`) and the saved `fg_stop` (`0x991FC4`), which arklay keeps in
//! the item box's [`super::itembox2::Entrance`].
//!
//! | step | what the original does |
//! |---|---|
//! | `Sce_Save` | slot 2 = the back routine, state 0, saved = `fg_stop`, `fg_stop \|= 0xFF000000`, the place (`0x98E9A7`) = the area's first data byte |
//! | state 0, a ribbon | door message 1, the question; state 1 |
//! | state 0, no ribbon | `fg_stop` = saved, door message 0; slot 2 cleared |
//! | state 1 | waits while the message shows (`fg_message` bit 7); then No puts `fg_stop` back, Yes sets game mode 1 and `fg_system \|= 0x40000` and keeps the hold; both clear slot 2 |

use super::GameState;
use crate::scd2::host::{ITEM_INK_RIBBON, door_message};

/// Slot 2 of `Game_loop`'s back routines (`0x98E794`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BackRoutine {
    #[default]
    None,
    /// `Item_box_entrance` (`0x4E9A20`), which `Sce_Itembox` sets.
    ItemBox,
    /// `Sce_Save_backfunc` (`0x4E9930`), which `Sce_Save` sets.
    Save,
}

impl BackRoutine {
    /// The routine's address, as the oracle's `tw` field shows it.
    pub fn address(self) -> u32 {
        match self {
            Self::None => 0,
            Self::ItemBox => 0x4E_9A20,
            Self::Save => 0x4E_9930,
        }
    }
}

/// Door message 0: "It's an old typewriter. If I had an Ink Ribbon, ..."
const MESSAGE_NO_RIBBON: u8 = 0;
/// Door message 1: "... Will you use the Ink Ribbon?", with its yes/no.
const MESSAGE_PROMPT: u8 = 1;

/// The save place a new game starts with (`0x98E9A7`), by the scenario's
/// index (`0x989E7E`: Leon A, Claire A, Leon B, Claire B, then the two
/// extras), from the new game's set-up (`0x4B73C0`).
const NEW_GAME_PLACES: [u8; 6] = [0x0F, 0x10, 0x0F, 0x10, 0x11, 0x12];

/// The save place before any typewriter fires, for a new game as
/// `character` (0 Leon, 1 Claire) in scenario A or B.
pub fn new_game_place(character: u8, scenario_b: bool) -> u8 {
    NEW_GAME_PLACES[usize::from(character & 1) + 2 * usize::from(scenario_b)]
}

/// The stop word the two messages hold the world with (`Stop_data`
/// `0xFF000000`).
const MESSAGE_STOP: u16 = 0xFF00;

impl GameState {
    /// `Sce_Save` (`0x4E98F0`): set the back routine, hold the player, the
    /// enemies and the scripts, and keep the area's place for the save.
    /// `place` is the area's first data byte (`aot + 0xC`).
    pub fn sce_save(&mut self, place: u8) {
        self.re2_back = BackRoutine::Save;
        let stop = self.re2.fg_stop_mut();
        let saved = *stop;
        *stop |= 0xFF00_0000;
        let entrance = &mut self.re2_box_entrance;
        entrance.state = 0;
        entrance.saved_stop = saved;
        self.re2_save_place = place;
    }

    /// `Search_item_id` (`0x502660`): the first of the inventory's
    /// `_inventory_size` slots that holds `id`, whatever its count.
    pub fn re2_item_slot(&self, id: u8) -> Option<usize> {
        self.inventory
            .iter()
            .take(self.inventory_capacity())
            .position(|stack| stack.id == id)
    }

    /// One tick of `Sce_Save_backfunc` (`0x4E9930`), while slot 2 holds it.
    /// Returns true on the tick a Yes sets game mode 1.
    pub fn tick_re2_save(&mut self) -> bool {
        if self.re2_back != BackRoutine::Save {
            return false;
        }
        match self.re2_box_entrance.state {
            0 => {
                if self.re2_item_slot(ITEM_INK_RIBBON).is_some() {
                    self.show_re2_door_message(door_message(MESSAGE_PROMPT));
                    self.hold_re2_message_stop(MESSAGE_STOP);
                    self.re2_box_entrance.state = 1;
                } else {
                    *self.re2.fg_stop_mut() = self.re2_box_entrance.saved_stop;
                    self.show_re2_door_message(door_message(MESSAGE_NO_RIBBON));
                    self.hold_re2_message_stop(MESSAGE_STOP);
                    self.re2_back = BackRoutine::None;
                }
                false
            }
            _ => {
                if self.message.active {
                    return false;
                }
                self.re2_back = BackRoutine::None;
                self.re2_box_entrance.state = 0;
                if self.message.menu_choice_id() & 1 == 0 {
                    self.re2_save_requested = true;
                    true
                } else {
                    *self.re2.fg_stop_mut() = self.re2_box_entrance.saved_stop;
                    false
                }
            }
        }
    }

    /// The oracle's `tw` field: slot 2's routine, its state byte and the
    /// save place.
    pub fn re2_typewriter_trace(&self) -> String {
        format!(
            "tw={:x},{},{}",
            self.re2_back.address(),
            self.re2_box_entrance.state,
            self.re2_save_place
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::InventoryItem;
    use crate::state::{RoomId, RoomState};

    const HELD: u32 = 0x0100_0000;

    /// Room 200 with Leon's handgun and knife, and `fg_stop` with one bit
    /// already held.
    fn game(ribbon: Option<(usize, u8)>) -> GameState {
        let mut game = GameState::new(RoomId::parse("2000").unwrap(), &RoomState::default());
        game.is_re2_room = true;
        game.inventory = [(2, 18), (1, 1)]
            .iter()
            .map(|&(id, quantity)| InventoryItem { id, quantity })
            .collect();
        if let Some((slot, quantity)) = ribbon {
            game.inventory.resize(slot + 1, InventoryItem::default());
            game.inventory[slot] = InventoryItem {
                id: ITEM_INK_RIBBON,
                quantity,
            };
        }
        *game.re2.fg_stop_mut() = HELD;
        game
    }

    fn fg_stop(game: &mut GameState) -> u32 {
        *game.re2.fg_stop_mut()
    }

    /// Close the window with `answer` in `fg_message`'s bit 0.
    fn answer(game: &mut GameState, answer: u8) {
        game.message.active = false;
        game.message.set_menu_choice_id(answer);
        game.release_re2_message_stop();
    }

    #[test]
    fn a_new_game_starts_on_its_scenarios_place() {
        assert_eq!(new_game_place(0, false), 0x0F);
        assert_eq!(new_game_place(1, false), 0x10);
        assert_eq!(new_game_place(0, true), 0x0F);
        assert_eq!(new_game_place(1, true), 0x10);
    }

    #[test]
    fn sce_save_holds_the_world_and_records_the_place() {
        let mut game = game(None);
        game.sce_save(1);
        assert_eq!(game.re2_back, BackRoutine::Save);
        assert_eq!(game.re2_box_entrance.state, 0);
        assert_eq!(game.re2_box_entrance.saved_stop, HELD);
        assert_eq!(fg_stop(&mut game), 0xFF00_0000 | HELD);
        assert_eq!(game.re2_save_place, 1);
        assert!(!game.message.active);
    }

    #[test]
    fn a_ribbon_shows_the_prompt() {
        let mut game = game(Some((2, 3)));
        game.sce_save(1);
        assert!(!game.tick_re2_save());
        assert!(game.message.active);
        assert_eq!(game.message.id, Some(door_message(1) | 0x40));
        assert_eq!(game.message.re2_timing_flags(), (true, true, false));
        assert_eq!(game.re2_box_entrance.state, 1);
        assert_eq!(fg_stop(&mut game), 0xFF00_0000 | HELD);
    }

    #[test]
    fn no_ribbon_releases_the_hold_and_says_so() {
        let mut game = game(None);
        game.sce_save(1);
        assert!(!game.tick_re2_save());
        assert_eq!(game.message.id, Some(door_message(0) | 0x40));
        assert_eq!(game.re2_back, BackRoutine::None);
        assert_eq!(game.re2_box_entrance.state, 0);
        // `Mess_disp` holds the world again until the window closes.
        assert_eq!(fg_stop(&mut game), 0xFF00_0000 | HELD);
        answer(&mut game, 0);
        assert_eq!(fg_stop(&mut game), HELD);
    }

    #[test]
    fn a_ribbon_past_the_inventory_size_does_not_count() {
        let capacity = game(None).inventory_capacity();
        let mut game = game(Some((capacity, 3)));
        game.sce_save(1);
        game.tick_re2_save();
        assert_eq!(game.message.id, Some(door_message(0) | 0x40));
        // `Search_item_id` reads only the id: an empty stack still counts.
        let mut game = super::tests::game(Some((2, 0)));
        game.sce_save(1);
        game.tick_re2_save();
        assert_eq!(game.message.id, Some(door_message(1) | 0x40));
    }

    #[test]
    fn the_prompt_waits_for_its_answer() {
        let mut game = game(Some((2, 3)));
        game.sce_save(1);
        game.tick_re2_save();
        for _ in 0..5 {
            assert!(!game.tick_re2_save());
            assert_eq!(game.re2_back, BackRoutine::Save);
            assert_eq!(game.re2_box_entrance.state, 1);
        }
    }

    #[test]
    fn no_puts_fg_stop_back() {
        let mut game = game(Some((2, 3)));
        game.sce_save(1);
        game.tick_re2_save();
        answer(&mut game, 1);
        // The window's close leaves the hold `Sce_Save` set.
        assert_eq!(fg_stop(&mut game), 0xFF00_0000 | HELD);
        assert!(!game.tick_re2_save());
        assert_eq!(game.re2_back, BackRoutine::None);
        assert_eq!(game.re2_box_entrance.state, 0);
        assert_eq!(fg_stop(&mut game), HELD);
        assert!(!game.re2_save_requested);
    }

    #[test]
    fn yes_requests_the_save_and_keeps_the_hold() {
        let mut game = game(Some((2, 3)));
        game.sce_save(1);
        game.tick_re2_save();
        answer(&mut game, 0);
        assert!(game.tick_re2_save());
        assert!(game.re2_save_requested);
        assert_eq!(game.re2_back, BackRoutine::None);
        assert_eq!(fg_stop(&mut game), 0xFF00_0000 | HELD);
    }

    #[test]
    fn the_item_box_and_the_typewriter_share_one_slot() {
        let mut game = game(Some((2, 3)));
        game.open_re2_box_entrance(3);
        assert_eq!(game.re2_back, BackRoutine::ItemBox);
        game.sce_save(1);
        assert_eq!(game.re2_back, BackRoutine::Save);
        assert!(!game.tick_re2_box_entrance());
        game.tick_re2_save();
        assert!(game.re2_box_trace().starts_with("ib=1,"));
    }

    #[test]
    fn the_trace_field() {
        let mut game = game(Some((2, 3)));
        assert_eq!(game.re2_typewriter_trace(), "tw=0,0,0");
        game.sce_save(1);
        game.tick_re2_save();
        assert_eq!(game.re2_typewriter_trace(), "tw=4e9930,1,1");
        answer(&mut game, 1);
        game.tick_re2_save();
        assert_eq!(game.re2_typewriter_trace(), "tw=0,0,1");
    }
}
