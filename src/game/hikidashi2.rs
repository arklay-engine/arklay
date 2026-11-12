//! RE 2's drawers: `Sce_Hikidashi` (`0x4E9CB0`, `Sce_at_jump_tbl[13]`) and
//! the hook it sets, `Hikidashi_entrance` (`0x4E9EA0`).
//!
//! A drawer's area (`aot_set ..., 13, ...`) holds three words: the `fg_key`
//! bit that unlocks it, the slot of the `item_aot_set` area that holds its
//! item, and the cut of its close-up. Room 201's desk, 10E's, 113's and
//! 115's are the game's four.
//!
//! The lock: the press's checks, the messages "The desk is locked.", the
//! question, "There's nothing more." and "You have used the item.", and the
//! hook's cases 1 and 2, which ask and unlock.
//!
//! The open, on a press on an unlocked drawer with its item still there:
//!
//! | tick | what the original does |
//! |---|---|
//! | the press | sound `0x28`; the item's model `Be_flg \|= 0x11` (drawn, and past the zone cull); `Cut_change` to the close-up; `fg_stop` saved (`0x991FC4`) and `\|= 0xFF000000`; `fg_status \|= 0x40`; the close-up's view quad (`Ccut_searc`'s entry, `0x988634`) overwritten with a far square; the hook's state `0x23` |
//! | the next 30 | the state counts down to 5 (the default case), the close-up on screen and the player and the enemies outside its quad |
//! | case 5 | the hand-off: the item's area becomes the get-item window's (`0x9888D0`, the item in `0x98504F`); `fg_status` `&~0x40 \| 0x8000`, game mode 1 with `St` mode 2; `Cut_change` and `scd_var_cut` back to the cut before the open (`0x98EB1C`), with no load, so the close-up stays up until the window takes the cut; `fg_status \|= 0x100`, which holds `Cut_check`; then the state goes down to 4 |
//! | case 4 | once the window has let `0x8000` go: `fg_status &= ~0x100`, the model's `Be_flg &= ~0x11`, the hook cleared |
//!
//! The window is `Sce_Item`'s (`GameState::open_re2_item_get`), with no
//! difficulty's count. Its `Status` start (the sixth tick) zeroes `fg_stop`
//! and its return puts the saved word back. The view quad stays far for the
//! rest of the stay, until the room's next load reads its RDT again; the
//! window's return loads the cut before the open, whose quad is whole.

use super::{GameState, RoomAction, STATE_BYTE_SELECTED_ITEM};
use crate::scd2::host::{CutEdit, DOOR_USED_KEY, door_message};

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
pub const OPEN_COUNT: u8 = 0x23;
/// The hook's state that hands the item to the get-item window.
pub const HAND_OFF: u8 = 5;
/// The hook's state that waits for the window, then closes the drawer.
pub const CLOSE: u8 = 4;

/// `fg_status`'s bits the drawer drives: the close-up's hold (`0x40`) and
/// `Cut_check`'s (`0x100`). Case 5's `0x8000` is the get-item window's,
/// which arklay models as the window itself (`GameState::re2_item_get`), as
/// it does for `Sce_Item`'s.
const STATUS_HOLD: u32 = 0x40;
const STATUS_NO_CUT_CHECK: u32 = 0x100;

/// The item model's `Be_flg` bits the open raises and the close drops:
/// drawn (1), and drawn whatever the zones (`0x10`).
pub const MODEL_SHOWN: u32 = 0x11;

/// What the open writes over the close-up's view quad (`+4` to `+0x12`,
/// `0x8044` and `0x80A8` as signed words): a square far off every map, so
/// `Hit_ck_point4` finds nobody inside it.
pub const FAR_VIEW_QUAD: [[i16; 2]; 4] = [
    [-32700, -32700],
    [-32700, -32600],
    [-32600, -32600],
    [-32600, -32700],
];

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
    /// The open's: the item's area (the get-item window's from case 5).
    pub item: Option<RoomAction>,
    /// The cut on screen before the open (`0x98EB1C`), which case 5 goes
    /// back to.
    pub old_cut: u8,
    /// `fg_stop` before the open (`0x991FC4`).
    pub saved_stop: u32,
}

impl Hikidashi {
    /// The drawer's `fg_key` bit.
    fn key(&self) -> u8 {
        self.area[0]
    }

    /// The close-up's cut (the area's third word).
    fn close_up(&self) -> u8 {
        self.area[4]
    }

    /// Whether an open is under way: the close-up's count, or the wait for
    /// the window (the lock's states are 1 and 2).
    pub fn opening(&self) -> bool {
        self.active && self.state >= CLOSE
    }

    /// `fg_status & 0x100`, from case 5 to case 4: `Game_loop` skips
    /// `Cut_check`.
    pub fn holds_cut(&self) -> bool {
        self.active && self.state == CLOSE
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
            self.open_re2_drawer(area, item);
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
            ..Hikidashi::default()
        };
    }

    /// `Sce_Hikidashi`'s open: the press on an unlocked drawer whose item is
    /// still there.
    fn open_re2_drawer(&mut self, area: [u8; 6], item: RoomAction) {
        self.push_re2_sound(SOUND_OPEN);
        self.set_re2_model_shown(item, true);
        let mut hook = Hikidashi {
            active: true,
            state: OPEN_COUNT,
            area,
            item: Some(item),
            old_cut: self.camera.current_cut as u8,
            ..Hikidashi::default()
        };
        // `Cut_change`: the live cut only; `Swap_Cbuff` loads it at the
        // tick's end, as the cut differs from `scd_var_cut`.
        self.set_camera_cut(usize::from(hook.close_up()));
        let fg_stop = self.re2.fg_stop_mut();
        hook.saved_stop = *fg_stop;
        *fg_stop |= 0xFF00_0000;
        *self.re2_fg_status() |= STATUS_HOLD;
        self.re2.pending_cuts.push(CutEdit::ViewQuad {
            cut: hook.close_up(),
            corners: FAR_VIEW_QUAD,
        });
        self.re2_hikidashi = hook;
    }

    /// The item model's `Be_flg |= 0x11` (or `&= ~0x11`): the item's area's
    /// model byte (`+0x12`).
    fn set_re2_model_shown(&mut self, item: RoomAction, shown: bool) {
        if let Some(record) = self.objects.record_mut(usize::from(item.params[3])) {
            if shown {
                record.be_flg |= MODEL_SHOWN;
            } else {
                record.be_flg &= !MODEL_SHOWN;
            }
            // The draw reads `Be_flg` bit 0, which arklay keeps as the
            // record's active flag.
            record.sync_active_from_be_flg();
        }
    }

    /// Case 5: the item goes to the get-item window, and the cut back to the
    /// one before the open, with no load.
    fn hand_off_re2_drawer(&mut self, hook: &Hikidashi) {
        if let Some(item) = hook.item {
            self.open_re2_item_get(item);
        }
        let status = self.re2_fg_status();
        *status = (*status & !STATUS_HOLD) | STATUS_NO_CUT_CHECK;
        // The original reads `WORD_0098eb1c`, the `scd_var_cut` that the
        // last `Bg_loadPC` replaced. The press's gate
        // (`BYTE_00989eea == scd_var_cut`: no cut change waiting) makes that
        // the cut on screen at the press, the one the open saved.
        let cut = hook.old_cut;
        self.set_camera_cut(usize::from(cut));
        self.re2.var_cut = Some(cut);
        self.re2.cut_no_load = true;
    }

    /// `fg_status`, the status bank's word.
    fn re2_fg_status(&mut self) -> &mut u32 {
        &mut self.re2.banks[usize::from(crate::scd2::host::STATUS_BANK)][0]
    }

    /// The get-item window's `Status` start (its sixth tick) zeroes
    /// `fg_stop`, and its return puts back the word saved before the open.
    /// The return's tick is a `Game_loop` frame (game modes 3 and 5) whose
    /// hook sees `fg_status`'s `0x8000` gone, so case 4 closes the drawer on
    /// it.
    pub fn re2_drawer_window_stop(&mut self, start: bool) {
        if !self.re2_hikidashi.holds_cut() {
            return;
        }
        let saved = self.re2_hikidashi.saved_stop;
        let fg_stop = self.re2.fg_stop_mut();
        *fg_stop = if start { 0 } else { *fg_stop | saved };
        if !start {
            self.close_re2_drawer();
        }
    }

    /// Case 4's close: `fg_status &= ~0x100`, the model's `Be_flg &=
    /// ~0x11`, and the hook cleared.
    fn close_re2_drawer(&mut self) {
        *self.re2_fg_status() &= !STATUS_NO_CUT_CHECK;
        if let Some(item) = self.re2_hikidashi.item {
            self.set_re2_model_shown(item, false);
        }
        self.re2_hikidashi = Hikidashi::default();
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
            CLOSE => {
                // `fg_status & 0x8000`: the window is still up.
                if self.re2_item_get.is_some() {
                    return;
                }
                self.close_re2_drawer();
                return;
            }
            HAND_OFF => {
                self.hand_off_re2_drawer(&hook);
                hook.state -= 1;
            }
            // The close-up's count.
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

    /// The oracle's `hk` field: the hook (0 none, 1 `Hikidashi_entrance`,
    /// 2 `Key_lost`), its state, then each of the room's `objects` objects'
    /// `Be_flg & 0x11`. The two hooks share the state byte (`0x98E541`):
    /// `Key_lost` holds its slot + 1 there until the key's message closes,
    /// and 0 while the discard question waits.
    pub fn re2_hikidashi_trace(&self, objects: usize) -> String {
        let hook = self.re2_hikidashi;
        let (kind, state) = match self.re2_key_lost {
            Some(key) => (2, if key.first { key.slot + 1 } else { 0 }),
            None => (u8::from(hook.active), hook.state),
        };
        let flags: Vec<String> = (0..objects.min(32))
            .map(|index| {
                let be_flg = self.objects.records.get(index).map_or(0, |r| r.be_flg);
                format!("{:x}", be_flg & 0x11)
            })
            .collect();
        format!(
            "hk={},{},{}",
            kind,
            state,
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

    /// An unlocked drawer with its item still there, room 201's desk shown
    /// from cut 2, the item's model 4 in the room.
    fn unlocked() -> GameState {
        let mut game = game();
        game.re2.door_locks[0] = 0x4000_0000;
        game.set_camera_cut(2);
        game.re2.shown_cut = 2;
        game.objects
            .records
            .resize(5, crate::objects::ObjectRecord::default());
        game.objects.records[4].be_flg = 0x8000_0000;
        game
    }

    fn fg_status(game: &GameState) -> u32 {
        game.re2.banks[usize::from(crate::scd2::host::STATUS_BANK)][0]
    }

    /// `Sce_Hikidashi`'s open: sound 0x28, the item model's `Be_flg |=
    /// 0x11`, the close-up's cut 6 (live only), `fg_stop |= 0xFF000000`
    /// with the word saved, `fg_status |= 0x40`, the close-up's view quad
    /// overwritten with `0x8044`/`0x80A8`, and the hook at `0x23`.
    #[test]
    fn an_unlocked_drawer_opens_on_its_close_up() {
        let mut game = unlocked();
        *game.re2.fg_stop_mut() = 0x10;
        press(&mut game);
        let sounds: Vec<_> = game.re2.sounds.iter().map(|sound| sound.id).collect();
        assert_eq!(
            sounds,
            vec![crate::se2::SoundId::decode(SOUND_OPEN).unwrap()]
        );
        assert_eq!(SOUND_OPEN, 0x0228_0000);
        assert_eq!(game.objects.records[4].be_flg, 0x8000_0011);
        assert!(game.objects.records[4].active(), "the item's model draws");
        assert_eq!(game.camera.current_cut, 6);
        assert_eq!(game.re2.var_cut, None, "`Cut_change` leaves `scd_var_cut`");
        assert_eq!(*game.re2.fg_stop_mut(), 0xFF00_0010);
        assert_eq!(fg_status(&game), 0x40);
        assert_eq!(
            game.re2.pending_cuts,
            vec![CutEdit::ViewQuad {
                cut: 6,
                corners: FAR_VIEW_QUAD
            }]
        );
        assert_eq!(
            FAR_VIEW_QUAD.map(|corner| corner.map(|word| word as u16)),
            [
                [0x8044, 0x8044],
                [0x8044, 0x80A8],
                [0x80A8, 0x80A8],
                [0x80A8, 0x8044]
            ]
        );
        let hook = game.re2_hikidashi;
        assert_eq!(
            (hook.active, hook.state, hook.old_cut, hook.saved_stop),
            (true, 0x23, 2, 0x10)
        );
        assert!(hook.opening() && hook.state > HAND_OFF && !hook.holds_cut());
        assert!(game.re2_item_get.is_none());
    }

    /// The default case counts the close-up down from `0x23`: thirty ticks
    /// to 5, the hand-off on the thirty-first. Case 5 gives the item's area
    /// to the get-item window, puts the cut before the open back (live and
    /// `scd_var_cut`, with no load), swaps `fg_status`'s `0x40` for
    /// `0x8100` (the `0x8000` is the window here) and goes down to 4. Case
    /// 4 waits for the window, then drops `0x100` and the model's `0x11`.
    #[test]
    fn the_close_up_counts_thirty_ticks_then_hands_the_item_to_the_window() {
        let mut game = unlocked();
        press(&mut game);
        for _ in 0..30 {
            game.tick_re2_hikidashi();
        }
        assert_eq!(game.re2_hikidashi.state, HAND_OFF);
        assert!(game.re2_item_get.is_none());
        assert_eq!(game.camera.current_cut, 6);
        game.tick_re2_hikidashi();
        let get = game.re2_item_get.expect("the window opens on case 5");
        assert_eq!(get.action, game.room_actions[9].unwrap());
        assert_eq!(game.re2_hikidashi.state, CLOSE);
        assert_eq!(game.camera.current_cut, 2);
        assert_eq!(game.re2.var_cut, Some(2));
        assert!(game.re2.cut_no_load, "the close-up stays up");
        // The original's `0x8100`: the window's `0x8000` is the window.
        assert_eq!(fg_status(&game), 0x100);
        let hook = game.re2_hikidashi;
        assert!(hook.opening() && hook.state == CLOSE && hook.holds_cut());
        // The window is up: case 4 waits.
        game.tick_re2_hikidashi();
        assert_eq!(game.re2_hikidashi.state, CLOSE);
        assert_eq!(game.objects.records[4].be_flg & MODEL_SHOWN, MODEL_SHOWN);
        game.re2_item_get = None;
        game.tick_re2_hikidashi();
        assert!(!game.re2_hook_set());
        assert_eq!(fg_status(&game), 0);
        assert_eq!(game.objects.records[4].be_flg, 0x8000_0000);
        assert!(!game.objects.records[4].active());
    }

    /// The window's `Status` start zeroes `fg_stop`, and its return puts the
    /// word saved before the open back and closes the drawer (the
    /// `hikidashi_open_201` recording's 1111); neither touches it outside a
    /// drawer's hand-off.
    #[test]
    fn the_window_zeroes_fg_stop_and_its_return_puts_the_saved_word_back() {
        let mut game = unlocked();
        *game.re2.fg_stop_mut() = 0x10;
        press(&mut game);
        game.re2_drawer_window_stop(true);
        assert_eq!(
            *game.re2.fg_stop_mut(),
            0xFF00_0010,
            "not before the hand-off"
        );
        for _ in 0..31 {
            game.tick_re2_hikidashi();
        }
        game.re2_drawer_window_stop(true);
        assert_eq!(*game.re2.fg_stop_mut(), 0);
        game.re2_drawer_window_stop(false);
        assert_eq!(*game.re2.fg_stop_mut(), 0x10);
        // The return's frame is case 4's: the drawer closes on it.
        assert!(!game.re2_hook_set());
        assert_eq!(game.objects.records[4].be_flg, 0x8000_0000);
    }

    /// The press is refused while an open is under way (the hook is set),
    /// and a drawer whose item is gone says "There's nothing more." instead.
    #[test]
    fn an_open_drawer_takes_no_second_press() {
        let mut game = unlocked();
        press(&mut game);
        game.re2.sounds.clear();
        game.re2.pending_cuts.clear();
        press(&mut game);
        assert!(game.re2.sounds.is_empty());
        assert!(game.re2.pending_cuts.is_empty());
        assert_eq!(game.re2_hikidashi.state, OPEN_COUNT);
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

    #[test]
    fn hk_trace_reads_key_lost_and_its_slot() {
        use crate::game::door2::KeyLost;
        let mut game = game();
        // A key turned from slot 5: the hook holds slot + 1 until the
        // message closes (`Key_lost`, `0x98E541`), as leon_a_200_203's
        // Spade Key at 204's door 2 has it.
        game.re2_key_lost = Some(KeyLost {
            slot: 5,
            first: true,
        });
        assert_eq!(game.re2_hikidashi_trace(0), "hk=2,6,-");
        // The discard question's phase: the byte is 0.
        game.re2_key_lost = Some(KeyLost {
            slot: 5,
            first: false,
        });
        assert_eq!(game.re2_hikidashi_trace(0), "hk=2,0,-");
        game.re2_key_lost = None;
        assert_eq!(game.re2_hikidashi_trace(0), "hk=0,0,-");
    }
}
