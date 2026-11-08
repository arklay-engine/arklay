//! RE 2's drawers: `Sce_Hikidashi` (`0x4E9CB0`, `Sce_at_jump_tbl[13]`) and
//! the hook it sets, `Hikidashi_entrance` (`0x4E9EA0`).
//!
//! A drawer's area (`aot_set ..., 13, ...`) holds three words: the `fg_key`
//! bit that unlocks it, the slot of the `item_aot_set` area that holds its
//! item, and the cut of its close-up. Room 201's desk, 10E's, 113's and
//! 115's are the game's four.
//!
//! The lock is modelled: the press's checks, the messages "The desk is
//! locked.", the question, "There's nothing more." and "You have used the
//! item.", and the hook's cases 1 and 2, which ask and unlock. The open (the
//! close-up's cut, its thirty-tick count, the view-quad clobber and the
//! hand-off to the get-item window, cases 4 and 5) is not: a press on an
//! unlocked drawer only notes that it isn't modelled.

use super::{GameState, STATE_BYTE_SELECTED_ITEM};
use crate::scd2::host::{DOOR_USED_KEY, door_message};

/// `Snd_se_on`'s ids from the room's bank: the drawer's open and the key's
/// turn.
pub const SOUND_OPEN: u32 = 0x0228_0000;
pub const SOUND_UNLOCK: u32 = 0x0227_0000;

/// The door messages the drawer shows: "The desk is locked.", the same with
/// "Will you use the item?" and a yes/no, and "There's nothing more.". The
/// unlock's "You have used the item." is the doors' own
/// ([`DOOR_USED_KEY`]).
pub const DESK_LOCKED: u8 = 2;
pub const DESK_ASK: u8 = 3;
pub const DESK_EMPTY: u8 = 4;

/// The small key, and Claire's lockpick in the personal slot.
const SMALL_KEY: u8 = 0x1F;
const LOCKPICK: u8 = 0x30;

/// The hook's state for the open's close-up (`0x23`, counted down to 5).
#[allow(dead_code)]
const OPEN_COUNT: u8 = 0x23;

/// `Hikidashi_entrance`'s routine while it is the hook (`pG + 0x82B0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Hikidashi {
    /// Whether `Hikidashi_entrance` is the hook.
    pub active: bool,
    /// The hook's state byte (`0x98E541`).
    pub state: u8,
    /// The drawer's area data (`0x9888D0`'s `+0x0C`): the key bit, the item
    /// area's slot and the close-up's cut, as words.
    pub area: [u8; 6],
    /// The small key's slot that the question found (`0x689CA4`).
    pub key_slot: Option<usize>,
}

impl Hikidashi {
    /// The drawer's `fg_key` bit.
    fn key(&self) -> u8 {
        self.area[0]
    }
}

/// The `fg_key` word and bit of `bit`, MSB first, as `Flg_ck` reads it.
fn key_bit(bit: u8) -> (usize, u32) {
    (usize::from(bit >> 5) & 1, 0x8000_0000 >> (bit & 31))
}

impl GameState {
    /// Whether the per-tick hook `pG + 0x82B0` is set: Sce_Door's
    /// `Key_lost` or a drawer's `Hikidashi_entrance`.
    pub fn re2_hook_set(&self) -> bool {
        self.re2_hikidashi.active || self.re2_key_lost.is_some()
    }

    fn re2_key_set(&self, bit: u8) -> bool {
        let (word, mask) = key_bit(bit);
        self.re2.door_locks[word] & mask != 0
    }

    /// `Sce_Hikidashi`: the press on a drawer's area.
    pub fn sce_hikidashi(&mut self, action: crate::game::RoomAction) {
        if self.re2_hook_set() || self.re2.player.damage_cnt != 0 {
            return;
        }
        // `fg_status & 0x8000`: the get-item window, the status screen or
        // the box is due or up. (`Sce_Item`'s crouch raises it only when
        // the crouch ends, and no press reaches the areas mid-crouch.)
        if self.re2_item_get.is_some()
            || self.re2_box.is_some()
            || self.re2_back == super::save2::BackRoutine::ItemBox
        {
            return;
        }
        // `fg_status & 0x40`: a message holds the game.
        if self.message.active {
            return;
        }
        // A cut `Cut_check` changed this tick isn't on screen yet.
        if self.camera.current_cut != usize::from(self.re2.shown_cut) {
            return;
        }
        let area: [u8; 6] = std::array::from_fn(|index| action.params[index]);
        let item_slot = usize::from(u16::from_le_bytes([area[2], area[3]]));
        // The item area's flag word (`+0x10`). Bank 8 serves the stages
        // below 4; `fg_item_2`, for the later ones, isn't modelled, and no
        // drawer is past stage 2.
        let Some(Some(item)) = self.room_actions.get(item_slot).copied() else {
            return;
        };
        if self.re2.flag(crate::scd2::host::ITEM_BANK, item.params[2]) {
            self.show_re2_door_message(door_message(DESK_EMPTY));
            return;
        }
        if self.re2_key_set(area[0]) {
            crate::scd2::host::log_unmodelled("Sce_Hikidashi open");
            return;
        }
        if self.re2_key_slot(SMALL_KEY).is_none() && self.re2.personal_item[0] != LOCKPICK {
            self.show_re2_door_message(door_message(DESK_LOCKED));
            return;
        }
        self.re2_hikidashi = Hikidashi {
            active: true,
            state: 1,
            area,
            key_slot: None,
        };
    }

    /// One tick of `Hikidashi_entrance`, at the head of the play body:
    /// case 1 asks, case 2 unlocks on a yes.
    pub fn tick_re2_hikidashi(&mut self) {
        let mut hook = self.re2_hikidashi;
        if !hook.active {
            return;
        }
        let lockpick = self.re2.personal_item[0] == LOCKPICK;
        match hook.state {
            0 => {}
            1 => {
                let item = if lockpick {
                    LOCKPICK
                } else {
                    hook.key_slot = self.re2_key_slot(SMALL_KEY);
                    SMALL_KEY
                };
                self.state_bytes[usize::from(STATE_BYTE_SELECTED_ITEM)] = item;
                self.show_re2_door_message(door_message(DESK_ASK));
                hook.state = 2;
            }
            2 => {
                // `fg_message & 0x80`: the window is still up.
                if self.message.menu_choice_id() & 0x80 != 0 {
                    return;
                }
                if self.message.menu_choice_id() & 1 == 0 {
                    let (word, mask) = key_bit(hook.key());
                    self.re2.door_locks[word] |= mask;
                    self.push_re2_sound(SOUND_UNLOCK);
                    let item = if lockpick {
                        LOCKPICK
                    } else {
                        self.use_re2_key(hook.key_slot);
                        SMALL_KEY
                    };
                    self.state_bytes[usize::from(STATE_BYTE_SELECTED_ITEM)] = item;
                    self.show_re2_door_message(door_message(DOOR_USED_KEY));
                }
                hook = Hikidashi::default();
            }
            // Cases 4 and 5 are the open's.
            _ => hook.state -= 1,
        }
        self.re2_hikidashi = hook;
    }

    /// Case 2's use of the small key: `Item_work`'s count is a byte, and
    /// only an exact 0 clears the slot; `Sort_item` runs whatever the count.
    fn use_re2_key(&mut self, slot: Option<usize>) {
        if let Some(stack) = slot.and_then(|slot| self.inventory.get_mut(slot)) {
            stack.quantity = stack.quantity.wrapping_sub(1);
            if stack.quantity == 0 {
                stack.id = 0;
            }
        }
        self.rebuild_slots();
    }

    /// The oracle's `hk` field: the hook (0 none, 1 `Hikidashi_entrance`),
    /// its state, then each of the room's `objects` objects' `Be_flg & 0x11`.
    pub fn re2_hikidashi_trace(&self, objects: usize) -> String {
        let hook = self.re2_hikidashi;
        let flags: Vec<String> = (0..objects.min(32))
            .map(|index| {
                let be_flg = self.objects.records.get(index).map_or(0, |r| r.be_flg);
                format!("{:x}", be_flg & 0x11)
            })
            .collect();
        format!(
            "hk={},{},{}",
            u8::from(hook.active),
            hook.state,
            if flags.is_empty() {
                "-".to_owned()
            } else {
                flags.join("/")
            }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::InventoryItem;
    use crate::scd2::host::run_op_for_test as exec;
    use crate::state::{RoomId, RoomState};

    /// Room 201 for Leon A: the desk's area 8 and its item area 9, the
    /// handgun bullets that are model 4.
    fn game() -> GameState {
        let mut game = GameState::new(RoomId::parse("2010").unwrap(), &RoomState::default());
        game.is_re2_room = true;
        game.inventory = [(2, 18), (1, 1)]
            .iter()
            .map(|&(id, quantity)| InventoryItem { id, quantity })
            .collect();
        exec(
            &mut game,
            "aot_set",
            &[
                8, 13, 49, 0, 0, -26631, -18161, 2500, 1800, 1, 0, 9, 0, 6, 0,
            ],
        );
        exec(
            &mut game,
            "item_aot_set",
            &[9, 2, 0, 0, 0, 0, 0, 10, 10, 20, 15, 92, 4, 0],
        );
        game
    }

    fn press(game: &mut GameState) {
        let action = game.room_actions[8].unwrap();
        game.fire_re2_area(action);
    }

    fn message(game: &GameState) -> Option<u8> {
        game.message.id.map(|id| id & !0x40)
    }

    /// The window closes with this answer (0 yes, 1 no).
    fn answer(game: &mut GameState, choice: u8) {
        game.message.active = false;
        game.message.id = None;
        game.message.set_menu_choice_id(choice);
    }

    fn give(game: &mut GameState, id: u8, quantity: u8) {
        game.inventory.push(InventoryItem { id, quantity });
    }

    /// Fire, ask, answer: the hook's whole unlock.
    fn unlock(game: &mut GameState, choice: u8) {
        press(game);
        game.tick_re2_hikidashi();
        answer(game, choice);
        game.tick_re2_hikidashi();
    }

    #[test]
    fn the_desk_is_a_drawer_area() {
        let game = game();
        let action = game.room_actions[8].unwrap();
        assert_eq!(action.sce, crate::scd2::host::SCE_HIKIDASHI);
        assert_eq!(&action.params[..6], &[1, 0, 9, 0, 6, 0]);
    }

    #[test]
    fn a_taken_drawer_says_nothing_more() {
        let mut game = game();
        game.re2.set_flag(crate::scd2::host::ITEM_BANK, 92, 1);
        press(&mut game);
        assert_eq!(message(&game), Some(door_message(DESK_EMPTY)));
        assert!(!game.re2_hikidashi.active);
        assert!(game.re2.sounds.is_empty());
    }

    #[test]
    fn a_locked_drawer_without_a_key_says_locked() {
        let mut game = game();
        press(&mut game);
        assert_eq!(message(&game), Some(door_message(DESK_LOCKED)));
        assert!(!game.re2_hikidashi.active);
        // `Mess_disp` raises `fg_stop`'s top byte at the end of the tick,
        // after the moves.
        assert_eq!(game.re2_message_stop_pending, 0xFF00);
    }

    #[test]
    fn a_small_key_asks_on_the_next_tick() {
        let mut game = game();
        give(&mut game, SMALL_KEY, 1);
        press(&mut game);
        assert!(game.re2_hikidashi.active);
        assert_eq!(game.re2_hikidashi.state, 1);
        assert_eq!(message(&game), None);
        game.tick_re2_hikidashi();
        assert_eq!(message(&game), Some(door_message(DESK_ASK)));
        assert_eq!(
            game.state_bytes[usize::from(STATE_BYTE_SELECTED_ITEM)],
            SMALL_KEY
        );
        assert_eq!(game.re2_hikidashi.state, 2);
        assert_eq!(game.re2_hikidashi.key_slot, Some(2));
        // The window is up: case 2 waits.
        game.tick_re2_hikidashi();
        assert_eq!(game.re2_hikidashi.state, 2);
    }

    #[test]
    fn yes_unlocks_plays_0x227_and_uses_the_key() {
        let mut game = game();
        give(&mut game, SMALL_KEY, 1);
        unlock(&mut game, 0);
        assert_ne!(game.re2.door_locks[0] & 0x4000_0000, 0);
        let sounds: Vec<_> = game.re2.sounds.iter().map(|sound| sound.id).collect();
        assert_eq!(
            sounds,
            vec![crate::se2::SoundId::decode(SOUND_UNLOCK).unwrap()]
        );
        assert!(!game.has_item(SMALL_KEY));
        assert_eq!(message(&game), Some(door_message(DOOR_USED_KEY)));
        assert!(!game.re2_hook_set());
        // The drawer stays shut on this press; a later one finds it open.
        assert!(game.re2_key_set(1));
    }

    #[test]
    fn no_leaves_the_lock_and_the_key() {
        let mut game = game();
        give(&mut game, SMALL_KEY, 1);
        unlock(&mut game, 1);
        assert_eq!(game.re2.door_locks, [0, 0]);
        assert_eq!(game.item_count(SMALL_KEY), 1);
        assert_eq!(message(&game), None);
        assert!(game.re2.sounds.is_empty());
        assert!(!game.re2_hook_set());
    }

    #[test]
    fn two_keys_leave_one() {
        let mut game = game();
        give(&mut game, SMALL_KEY, 2);
        unlock(&mut game, 0);
        assert_eq!(
            game.inventory[2],
            InventoryItem {
                id: SMALL_KEY,
                quantity: 1
            }
        );
    }

    #[test]
    fn claires_lockpick_unlocks_without_a_key() {
        let mut game = game();
        game.re2.personal_item = [LOCKPICK, 1];
        press(&mut game);
        game.tick_re2_hikidashi();
        assert_eq!(
            game.state_bytes[usize::from(STATE_BYTE_SELECTED_ITEM)],
            LOCKPICK
        );
        answer(&mut game, 0);
        game.tick_re2_hikidashi();
        assert!(game.re2_key_set(1));
        assert_eq!(game.re2.personal_item, [LOCKPICK, 1]);
        assert_eq!(
            game.state_bytes[usize::from(STATE_BYTE_SELECTED_ITEM)],
            LOCKPICK
        );
        assert_eq!(message(&game), Some(door_message(DOOR_USED_KEY)));
    }

    #[test]
    fn the_lockpick_wins_over_a_small_key() {
        let mut game = game();
        game.re2.personal_item = [LOCKPICK, 1];
        give(&mut game, SMALL_KEY, 1);
        press(&mut game);
        game.tick_re2_hikidashi();
        assert_eq!(
            game.state_bytes[usize::from(STATE_BYTE_SELECTED_ITEM)],
            LOCKPICK
        );
        answer(&mut game, 0);
        game.tick_re2_hikidashi();
        assert_eq!(game.item_count(SMALL_KEY), 1);
    }

    #[test]
    fn an_unlocked_drawer_is_not_opened_yet() {
        let mut game = game();
        game.re2.door_locks[0] = 0x4000_0000;
        press(&mut game);
        assert!(!game.re2_hook_set());
        assert_eq!(message(&game), None);
    }

    #[test]
    fn the_gates_hold_the_drawer() {
        let gates: [fn(&mut GameState); 5] = [
            |game| game.message.active = true,
            |game| game.re2_back = crate::game::save2::BackRoutine::ItemBox,
            |game| game.re2.player.damage_cnt = 0x80,
            |game| game.re2_hikidashi.active = true,
            |game| game.camera.current_cut = 1,
        ];
        for gate in gates {
            let mut game = game();
            gate(&mut game);
            let before = game.re2_hikidashi;
            press(&mut game);
            assert_eq!(game.re2_hikidashi, before);
            assert_eq!(message(&game), None);
        }
    }

    #[test]
    fn hk_trace_reads_the_hook_and_the_bits() {
        let mut game = game();
        assert_eq!(game.re2_hikidashi_trace(0), "hk=0,0,-");
        give(&mut game, SMALL_KEY, 1);
        press(&mut game);
        game.tick_re2_hikidashi();
        game.objects
            .records
            .resize(2, crate::objects::ObjectRecord::default());
        game.objects.records[0].be_flg = 0x0A;
        game.objects.records[1].be_flg = 0x1B;
        assert_eq!(game.re2_hikidashi_trace(2), "hk=1,2,0/11");
        // Past the records the room declares, a slot reads 0.
        assert_eq!(game.re2_hikidashi_trace(3), "hk=1,2,0/11/0");
    }
}
