//! RE2's locked doors: `Sce_Door`'s sounds and key use, and `Key_lost`,
//! the per-tick hook (`pG + 0x82B0`) a key turn arms to take the key once
//! its message closes.

use super::GameState;

/// `Sce_Door`'s room-bank sounds (`Snd_se_on`): a door that stays locked,
/// a key turned in it, and a door unlocked from this side.
pub const SOUND_DOOR_LOCKED: u32 = 0x0216_0000;
pub const SOUND_KEY_USED: u32 = 0x0225_0000;
pub const SOUND_UNLOCKED: u32 = 0x0226_0000;

/// Door message 8, "I can't leave Sherry alone."
pub const DOOR_PARTNER: u8 = 8;
/// Door message 9, "This key is useless now. Discard?" (yes/no).
pub const DOOR_DISCARD_KEY: u8 = 9;

/// `Key_lost`'s state while the hook holds it: the slot it works on
/// (`0x98E541` - 1, kept at `0x689CA0`) and whether it is in its first
/// phase (`0x98E541` non-zero: wait for the key's message, then take one).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyLost {
    pub slot: u8,
    pub first: bool,
}

impl GameState {
    /// `Search_item_id` (`0x502660`): the first slot below the inventory's
    /// size that holds `key`.
    pub(super) fn re2_key_slot(&self, key: u8) -> Option<usize> {
        let size = self.inventory_capacity();
        self.inventory
            .iter()
            .take(size)
            .position(|stack| stack.id == key)
    }

    /// `Sce_Door`'s partner test (`0x4E9472`): Claire with Sherry present,
    /// and Sherry's `Be_flg & 1` and her `+0x21D & 0x20` (she is waiting
    /// where Claire left her). arklay does not model Sherry's `+0x21D`, so
    /// this never blocks a door; its call site keeps the original's order.
    pub(super) fn re2_partner_blocks_door(&self) -> bool {
        false
    }

    /// `Key_lost` (`0x4E95F0`), run every play tick while a key turn armed
    /// it. First phase: once the key's message has closed, one use comes
    /// off the key; at none left it asks whether to discard it. Second
    /// phase: once that question is answered, Yes removes the slot
    /// (`Sort_item`) and No leaves one use.
    pub fn tick_re2_key_lost(&mut self) {
        let Some(state) = self.re2_key_lost else {
            return;
        };
        let slot = usize::from(state.slot);
        let message = self.message.menu_choice_id();
        if message & 0x80 != 0 {
            return;
        }
        if state.first {
            // `DEC CL`: a key already at 0 wraps and asks nothing.
            let count = self.inventory.get_mut(slot).map(|stack| {
                stack.quantity = stack.quantity.wrapping_sub(1);
                stack.quantity
            });
            if count == Some(0) {
                use crate::scd2::host::door_message;
                self.show_re2_door_message(door_message(DOOR_DISCARD_KEY));
                // `fg_stop |= 0xFF000000` (`0x4E964F`): the hook runs before
                // the moves, so the stop holds this tick's, where the
                // message's own stop would only go up after them.
                self.hold_re2_message_stop(0xFF00);
                self.re2_key_lost = Some(KeyLost {
                    slot: state.slot,
                    first: false,
                });
            } else {
                self.re2_key_lost = None;
            }
            return;
        }
        if message & 1 != 0 {
            // No: the key stays, with one use.
            if let Some(stack) = self.inventory.get_mut(slot) {
                stack.quantity = 1;
            }
        } else if slot < self.inventory.len() {
            // Yes: the slot empties and `Sort_item` moves the later ones up.
            // arklay's inventory has no holes, so the first empty slot is
            // the key's own and the sort is a removal.
            self.inventory.remove(slot);
            self.rebuild_slots();
        }
        self.re2_key_lost = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{Door, InventoryItem, STATE_BYTE_SELECTED_ITEM};
    use crate::scd2::host::Re2Door;
    use crate::scd2::host::door_message;
    use crate::se2::SoundId;

    fn game_with(items: &[(u8, u8)]) -> GameState {
        let mut game = GameState {
            is_re2_room: true,
            ..GameState::default()
        };
        for &(id, quantity) in items {
            game.inventory.push(InventoryItem { id, quantity });
        }
        game.rebuild_slots();
        game
    }

    fn door(lock: u8, key: u8) -> Door {
        Door {
            lock,
            key,
            re2: Some(Re2Door::default()),
            ..Door::default()
        }
    }

    fn sounds(game: &GameState) -> Vec<SoundId> {
        game.re2.sounds.iter().map(|request| request.id).collect()
    }

    fn sound(id: u32) -> SoundId {
        SoundId::decode(id).unwrap()
    }

    fn shown(door: u8) -> Option<u8> {
        Some(door_message(door) | 0x40)
    }

    fn close_message(game: &mut GameState, answer: u8) {
        game.message.active = false;
        game.message.set_menu_choice_id(answer);
    }

    #[test]
    fn an_unlock_door_plays_0x226_after_its_message_and_raises_the_bit() {
        let mut game = game_with(&[]);
        assert!(!game.try_re2_door_for_test(&door(0x91, 0xFE)));
        assert!(game.message.active);
        assert_eq!(game.message.id, shown(0x0A));
        assert_eq!(sounds(&game), [sound(SOUND_UNLOCKED)]);
        assert_eq!(game.re2.door_locks, [0x0000_4000, 0]);
    }

    #[test]
    fn a_door_locked_from_inside_plays_0x216_and_keeps_its_bit_down() {
        let mut game = game_with(&[]);
        assert!(!game.try_re2_door_for_test(&door(0x90, 0xFF)));
        assert_eq!(game.message.id, shown(0x0B));
        assert_eq!(sounds(&game), [sound(SOUND_DOOR_LOCKED)]);
        assert_eq!(game.re2.door_locks, [0, 0]);
    }

    #[test]
    fn a_locked_door_without_its_key_names_its_lock() {
        let mut game = game_with(&[(0x14, 30)]);
        assert!(!game.try_re2_door_for_test(&door(0x85, 0x59)));
        assert_eq!(game.message.id, shown(0x0D));
        assert_eq!(sounds(&game), [sound(SOUND_DOOR_LOCKED)]);
        assert_eq!(game.re2.door_locks, [0, 0]);
        assert_eq!(game.re2_key_lost, None);
        // A key past the inventory's size is not found.
        let mut game = game_with(&[(0x14, 30), (0x59, 1)]);
        game.set_inventory_size(Some(1));
        game.inventory.push(InventoryItem {
            id: 0x59,
            quantity: 1,
        });
        assert_eq!(game.re2_key_slot(0x59), None);
    }

    #[test]
    fn a_key_turn_plays_0x225_and_arms_key_lost_on_the_first_slot() {
        let mut game = game_with(&[(1, 0), (2, 15), (0x5A, 2), (0x14, 30), (0x5A, 1)]);
        assert!(!game.try_re2_door_for_test(&door(0x88, 0x5A)));
        assert_eq!(game.message.id, shown(5));
        assert_eq!(
            game.state_bytes[usize::from(STATE_BYTE_SELECTED_ITEM)],
            0x5A
        );
        assert_eq!(sounds(&game), [sound(SOUND_KEY_USED)]);
        assert_eq!(game.re2.door_locks, [0x0080_0000, 0]);
        assert_eq!(
            game.re2_key_lost,
            Some(KeyLost {
                slot: 2,
                first: true
            })
        );
    }

    #[test]
    fn key_lost_waits_for_the_message_then_takes_one() {
        let mut game = game_with(&[(1, 0), (2, 15), (0x5A, 2)]);
        game.try_re2_door_for_test(&door(0x88, 0x5A));
        assert_ne!(game.message.menu_choice_id() & 0x80, 0);
        game.tick_re2_key_lost();
        assert_eq!(game.inventory[2].quantity, 2);
        close_message(&mut game, 0);
        game.tick_re2_key_lost();
        assert_eq!(game.inventory[2].quantity, 1);
        assert_eq!(game.re2_key_lost, None);
        assert!(!game.message.active);
    }

    fn discard(answer: u8) -> GameState {
        let mut game = game_with(&[(1, 0), (2, 15), (0x59, 1), (0x14, 30)]);
        game.try_re2_door_for_test(&door(0x85, 0x59));
        close_message(&mut game, 0);
        game.tick_re2_key_lost();
        assert_eq!(
            game.inventory[2],
            InventoryItem {
                id: 0x59,
                quantity: 0
            }
        );
        assert!(game.message.active);
        assert_eq!(game.message.id, shown(DOOR_DISCARD_KEY));
        // `Key_lost` raises the stop itself, before this tick's moves.
        assert_eq!(*game.re2.fg_stop_mut() & 0xFF00_0000, 0xFF00_0000);
        assert_eq!(
            game.re2_key_lost,
            Some(KeyLost {
                slot: 2,
                first: false
            })
        );
        // Unanswered: nothing moves.
        game.tick_re2_key_lost();
        assert_eq!(game.inventory[2].quantity, 0);
        close_message(&mut game, answer);
        game.tick_re2_key_lost();
        assert_eq!(game.re2_key_lost, None);
        game
    }

    #[test]
    fn the_last_use_asks_to_discard_and_yes_sorts_the_slots() {
        let game = discard(0);
        assert_eq!(game.inventory.len(), 3);
        assert_eq!(
            game.inventory[2],
            InventoryItem {
                id: 0x14,
                quantity: 30
            }
        );
    }

    #[test]
    fn no_keeps_the_key_with_one() {
        let game = discard(1);
        assert_eq!(
            game.inventory[2],
            InventoryItem {
                id: 0x59,
                quantity: 1
            }
        );
        assert_eq!(
            game.inventory[3],
            InventoryItem {
                id: 0x14,
                quantity: 30
            }
        );
    }
}
