//! RE2's change of player character: a room script's `save(32, N)` writes
//! `scd_var_next_pld`, and the next room's `Set_room` (`0x4DE7B0`, case 10)
//! makes the player that character when it differs from `pPl.Id`. Leon A's
//! sewer hands the player to Ada (0xE) this way and back.
//!
//! What's here is the block's state: the costume remap, `St_chenge_pl`'s
//! inventory swap (`0x502190`), `Player_set`'s `Max_life` and the life and
//! poison stash. The engine reloads the model, the weapon's clips and the
//! core sound bank from [`crate::scd2::host::Re2State::pl_id`].

use super::{GameState, InventoryItem};

/// `scd_variables[32]`, `scd_var_next_pld` (`0x98EB24`): the character the
/// next room's load makes the player.
pub const VAR_NEXT_PLD: usize = 32;
/// Ada's and Sherry's ids.
pub const ADA: u8 = 0xE;
pub const SHERRY: u8 = 0xF;
/// `fg_common`'s `0x7F` (flag 4:127): the scenario's character is stashed.
const FG_COMMON: u8 = 4;
const FG_COMMON_STASHED: u8 = 0x7F;
/// `fg_zapping` (`0x98ED24`) is `Flg_addr[29]`; its bits 5, 6 and 0xF pick
/// the costumes.
const FG_ZAPPING: u8 = 29;
/// No slot equipped (`St+8`).
pub const NO_SLOT: u8 = 0x80;
/// `poison_status` while poisoned: every writer sets 0x1518.
pub const POISON_STATUS: u16 = 0x1518;
/// The personal items `St_chenge_pl` gives back: the lighter (Leon's ids)
/// and the lockpick (Claire's).
const LIGHTER: u8 = 0x2F;
const LOCKPICK: u8 = 0x30;

/// The table at `0x540328`, row 0: Ada's handgun (item 3) with 13 rounds,
/// 45 handgun bullets, a first aid spray, and her photo as the personal item.
pub const ADA_ITEMS: [[u8; 3]; 11] = [
    [0x03, 0x0D, 0],
    [0x14, 0x2D, 0],
    [0x23, 0x01, 0],
    [0; 3],
    [0; 3],
    [0; 3],
    [0; 3],
    [0; 3],
    [0; 3],
    [0; 3],
    [0x57, 0x0F, 0],
];
/// The same table's next row: a first aid spray and Sherry's photo.
pub const SHERRY_ITEMS: [[u8; 3]; 11] = [
    [0x23, 0x01, 0],
    [0; 3],
    [0; 3],
    [0; 3],
    [0; 3],
    [0; 3],
    [0; 3],
    [0; 3],
    [0; 3],
    [0; 3],
    [0x31, 0x01, 0],
];

impl GameState {
    /// `St+8`'s low byte: the slot of the equipped item, 0x80 for none.
    pub fn re2_equip_slot(&self) -> u8 {
        self.equipped
            .and_then(|item| self.inventory.iter().position(|stack| stack.id == item))
            .map_or(NO_SLOT, |slot| slot as u8)
    }

    /// `Item_work`'s eleven entries, `[id, count, mark]`.
    fn re2_item_work(&self) -> [[u8; 3]; 11] {
        let mut work = [[0; 3]; 11];
        for (slot, stack) in work.iter_mut().zip(self.inventory.iter().take(10)) {
            *slot = [stack.id, stack.quantity, 0];
        }
        let [id, count] = self.re2.personal_item;
        work[10] = [id, count, 0];
        work
    }

    /// Fill `Item_work`'s first ten entries.
    fn set_re2_item_work(&mut self, slots: &[[u8; 3]]) {
        self.inventory = slots
            .iter()
            .take(10)
            .map(|&[id, quantity, _]| InventoryItem { id, quantity })
            .collect();
        self.rebuild_slots();
    }

    /// The trace's `pl` field: `pPl.Id`, `scd_var_next_pld`, `Max_life`,
    /// `St+8` and `St+0xA`, the stash's slot and size, the stashed life,
    /// `poison_status` and `poison_timer`.
    pub fn re2_player_trace(&self) -> String {
        let re2 = &self.re2;
        let stash = &re2.pl_stash;
        format!(
            "{},{},{},{},{},{},{},{},{},{}",
            re2.pl_id,
            re2.vars[VAR_NEXT_PLD],
            self.max_health,
            self.re2_equip_slot(),
            self.equipped_weapon_id,
            stash.equip_slot,
            stash.size,
            stash.life,
            if re2.player.poisoned {
                POISON_STATUS
            } else {
                0
            },
            re2.player.poison_timer,
        )
    }

    /// `Player_set`'s `scd_var_next_pld = Id`: at a new game, a load, and
    /// after a setup that names the player.
    pub fn set_re2_player_id(&mut self, id: u8) {
        self.re2.pl_id = id;
        self.re2.vars[VAR_NEXT_PLD] = u16::from(id);
    }
}

/// `St_chenge_pl` (`0x502190`): into Ada or Sherry, stash the scenario's
/// character's items and give the table's; back, restore ten slots, the
/// slot count and the equipped slot, with the lighter or the lockpick as the
/// personal item.
pub fn st_chenge_pl(game: &mut GameState, next: u8) {
    if matches!(next, ADA | SHERRY) {
        game.re2.pl_stash.equip_slot = game.re2_equip_slot();
        game.re2.pl_stash.size = game.re2.inventory_size;
        game.re2.set_flag(FG_COMMON, FG_COMMON_STASHED, 1);
        let (table, equipped) = if next == ADA {
            (&ADA_ITEMS, Some(ADA_ITEMS[0][0]))
        } else {
            (&SHERRY_ITEMS, None)
        };
        game.re2.inventory_size = 8;
        game.re2.pl_stash.slots = game.re2_item_work();
        game.set_re2_item_work(table);
        game.re2.personal_item = [table[10][0], table[10][1]];
        game.set_equipped(equipped);
        return;
    }
    if game.re2.flag(FG_COMMON, FG_COMMON_STASHED) {
        let stash = game.re2.pl_stash;
        game.set_re2_item_work(&stash.slots[..10]);
        game.re2.inventory_size = stash.size;
        // `St+0xA` is what the restored slot holds.
        let equipped = (stash.equip_slot != NO_SLOT)
            .then(|| stash.slots.get(usize::from(stash.equip_slot)))
            .flatten()
            .map(|slot| slot[0])
            .filter(|&id| id != 0);
        game.set_equipped(equipped);
        game.re2.personal_item = [if next & 1 == 0 { LIGHTER } else { LOCKPICK }, 1];
    }
    game.re2.set_flag(FG_COMMON, FG_COMMON_STASHED, 0);
}

/// `Set_room`'s case 10: the player becomes `scd_var_next_pld` when it isn't
/// `pPl.Id`, through the costume remap, `St_chenge_pl`, `Player_set`'s
/// `Max_life` and the life and poison stash. Returns whether the player
/// changed; the caller loads the new model, clips and core bank.
pub fn set_room_change(game: &mut GameState) -> bool {
    let mut next = game.re2.vars[VAR_NEXT_PLD];
    if u16::from(game.re2.pl_id) == next {
        return false;
    }
    if (next as i16) < 0xC {
        if next & 1 == 0 {
            if game.re2.flag(FG_ZAPPING, 5) {
                next = 8;
            }
            if game.re2.flag(FG_ZAPPING, 0xF) {
                next = 10;
            }
        } else if game.re2.flag(FG_ZAPPING, 6) {
            next = 9;
        }
        game.re2.vars[VAR_NEXT_PLD] = next;
    }
    let previous = game.re2.pl_id;
    let next = next as u8;
    game.re2.pl_id = next;
    st_chenge_pl(game, next);
    // `Player_set`: `Rnd`'s seed (`0xD2706CA4`), `Max_life` and
    // `scd_var_next_pld = Id`.
    game.re2.rnd = crate::zombie2::Rnd::default();
    game.max_health = if next == 0xD {
        400
    } else {
        super::RE2_MAX_HEALTH
    };
    game.re2.vars[VAR_NEXT_PLD] = u16::from(next);
    // The new body's parts start zeroed: the neck joint's world position
    // (what a partner's `Root_ck` aims at) reads 0 until the player's
    // routine first moves the joints.
    game.re2.player.neck.position = Some([0, 0, 0]);
    let life = if matches!(next, ADA | SHERRY) {
        let player = &mut game.re2.player;
        let stash = &mut game.re2.pl_stash;
        stash.life = game.entities[0].health;
        stash.poison_timer = player.poison_timer;
        stash.poison_status = if player.poisoned { POISON_STATUS } else { 0 };
        player.poison_timer = 0;
        player.poisoned = false;
        Some(game.max_health)
    } else if previous > 0xB {
        let stash = game.re2.pl_stash;
        let player = &mut game.re2.player;
        player.poison_timer = stash.poison_timer;
        player.poisoned = stash.poison_status != 0;
        Some(stash.life)
    } else {
        None
    };
    if let Some(life) = life {
        game.entities[0].health = life;
        game.set_health_copy(life);
    }
    true
}

/// A setup's change to `id` at once (`game:re2_change_player`): `save(32,
/// id)` and `Set_room`'s block.
pub fn change_player(game: &mut GameState, id: u8) -> bool {
    game.re2.vars[VAR_NEXT_PLD] = u16::from(id);
    set_room_change(game)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leon() -> GameState {
        let mut game = GameState {
            is_re2_room: true,
            ..GameState::default()
        };
        game.max_health = super::super::RE2_MAX_HEALTH;
        for (id, quantity) in [(2, 15), (0x14, 30), (0x5A, 1)] {
            game.inventory.push(InventoryItem { id, quantity });
        }
        game.rebuild_slots();
        game.re2.personal_item = [LIGHTER, 1];
        game.set_equipped(Some(2));
        game.set_re2_player_id(0);
        game.entities[0].health = 90;
        game
    }

    #[test]
    fn st_chenge_pl_into_ada_loads_her_table() {
        let mut game = leon();
        game.re2.inventory_size = 10;
        st_chenge_pl(&mut game, ADA);
        assert_eq!(game.re2_item_work(), ADA_ITEMS);
        assert_eq!(game.re2.personal_item, [0x57, 15]);
        assert_eq!(game.equipped_weapon_id, 3);
        assert_eq!(game.re2_equip_slot(), 0);
        assert_eq!(game.inventory_capacity(), 8);
        assert!(game.re2.flag(4, 127));
        let stash = game.re2.pl_stash;
        assert_eq!(stash.slots[..3], [[2, 15, 0], [0x14, 30, 0], [0x5A, 1, 0]]);
        assert_eq!(stash.slots[10], [LIGHTER, 1, 0]);
        assert_eq!((stash.equip_slot, stash.size), (0, 10));
        assert_eq!(
            game.re2_inventory_trace(),
            "3:13,14:45,23:1,0:0,0:0,0:0,0:0,0:0,0:0,0:0,57:15/8"
        );
    }

    #[test]
    fn st_chenge_pl_into_sherry_equips_nothing() {
        let mut game = leon();
        st_chenge_pl(&mut game, SHERRY);
        assert_eq!(game.re2_item_work(), SHERRY_ITEMS);
        assert_eq!(game.re2_equip_slot(), NO_SLOT);
        assert_eq!(game.equipped_weapon_id, 0);
    }

    #[test]
    fn st_chenge_pl_back_restores_ten_slots_and_the_lighter() {
        for (id, personal) in [(0, LIGHTER), (1, LOCKPICK)] {
            let mut game = leon();
            game.set_equipped(Some(0x14));
            st_chenge_pl(&mut game, ADA);
            // The stash's eleventh entry is not what comes back.
            game.re2.pl_stash.slots[10] = [0x99, 9, 0];
            st_chenge_pl(&mut game, id);
            assert_eq!(game.inventory.len(), 3);
            assert_eq!(game.re2.personal_item, [personal, 1]);
            assert_eq!(game.re2_equip_slot(), 1);
            assert_eq!(game.equipped_weapon_id, 0x14);
            assert_eq!(game.inventory_capacity(), 8);
            assert!(!game.re2.flag(4, 127));
        }
        // An unequipped stash comes back with no weapon.
        let mut game = leon();
        game.set_equipped(None);
        st_chenge_pl(&mut game, ADA);
        st_chenge_pl(&mut game, 0);
        assert_eq!(game.re2_equip_slot(), NO_SLOT);
        assert_eq!(game.equipped_weapon_id, 0);
        // Without the stash flag nothing is restored.
        let mut game = leon();
        st_chenge_pl(&mut game, 0);
        assert_eq!(game.re2.personal_item, [LIGHTER, 1]);
        assert_eq!(game.inventory.len(), 3);
    }

    #[test]
    fn set_room_change_stashes_life_and_poison() {
        let mut game = leon();
        game.re2.player.poisoned = true;
        game.re2.player.poison_timer = 12;
        game.re2.vars[VAR_NEXT_PLD] = u16::from(ADA);
        game.re2.rnd = crate::zombie2::Rnd(6114);
        game.re2.player.neck.position = Some([-24756, -7802, -26045]);
        assert!(set_room_change(&mut game));
        assert_eq!(game.re2.pl_id, ADA);
        // The new body's neck reads 0 until its routine runs (30B: 517 to 717).
        assert_eq!(game.re2.player.neck.position, Some([0, 0, 0]));
        assert_eq!(game.entities[0].health, 200);
        // `Player_set` seeds `Rnd` again (room 30B's switch: 6114 to 27812).
        assert_eq!(game.re2.rnd.0, crate::zombie2::RND_SEED);
        assert!(!game.re2.player.poisoned);
        assert_eq!(game.re2.player.poison_timer, 0);
        let stash = game.re2.pl_stash;
        assert_eq!(
            (stash.life, stash.poison_timer, stash.poison_status),
            (90, 12, POISON_STATUS)
        );
        // Ada is hurt; her damage is thrown away on the way back.
        game.entities[0].health = 40;
        game.re2.vars[VAR_NEXT_PLD] = 0;
        assert!(set_room_change(&mut game));
        assert_eq!(game.re2.pl_id, 0);
        assert_eq!(game.entities[0].health, 90);
        assert!(game.re2.player.poisoned);
        assert_eq!(game.re2.player.poison_timer, 12);
        assert_eq!(game.re2.vars[VAR_NEXT_PLD], 0);
    }

    #[test]
    fn set_room_no_change_when_var32_matches() {
        // Claire: id 1 with var 32 at 1. With var 32 left at 0 she'd become
        // Leon on her first door.
        let mut game = leon();
        game.set_re2_player_id(1);
        let before = (
            game.inventory.clone(),
            game.re2.personal_item,
            game.entities[0].health,
        );
        assert!(!set_room_change(&mut game));
        assert_eq!(game.re2.pl_id, 1);
        assert_eq!(
            (
                game.inventory.clone(),
                game.re2.personal_item,
                game.entities[0].health
            ),
            before
        );
        assert!(!game.re2.flag(4, 127));
    }

    #[test]
    fn costume_remap_by_zapping_flags() {
        for (next, flag, remapped) in [(0, 5, 8), (0, 0xF, 10), (1, 6, 9), (2, 6, 2), (1, 5, 1)] {
            let mut game = leon();
            game.set_re2_player_id(ADA);
            game.re2.set_flag(29, flag, 1);
            game.re2.vars[VAR_NEXT_PLD] = next;
            assert!(set_room_change(&mut game));
            assert_eq!(game.re2.pl_id, remapped, "{next} with zapping {flag}");
            assert_eq!(game.re2.vars[VAR_NEXT_PLD], u16::from(remapped));
        }
        // Ada isn't remapped.
        let mut game = leon();
        game.re2.set_flag(29, 5, 1);
        assert!(change_player(&mut game, ADA));
        assert_eq!(game.re2.pl_id, ADA);
    }

    #[test]
    fn player_trace_names_the_id_the_stash_and_the_poison() {
        let mut game = leon();
        assert_eq!(game.re2_player_trace(), "0,0,200,0,2,0,0,200,0,0");
        game.re2.inventory_size = 8;
        game.re2.player.poisoned = true;
        assert!(change_player(&mut game, ADA));
        assert_eq!(game.re2_player_trace(), "14,14,200,0,3,0,8,90,0,0");
    }
}
