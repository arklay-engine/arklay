//! The game side of the RE2 script VM: the flag banks and script variables,
//! and the instructions that build the room's interaction areas.

use super::opcode::Op;
use super::vm::{Host, Step};
use crate::game::{Door, GameState, ROOM_ACTION_SLOTS, RoomAction, RoomActionKind};
use crate::state::Zone;

/// The flag banks `ck` and `set` index (`Flg_addr`).
const BANKS: usize = 32;

/// RE2's script state, which has no RE1 counterpart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Re2State {
    /// The flag banks; a bit is `0x80000000 >> (index & 31)` of word
    /// `(index >> 5) & 7`.
    pub banks: [[u32; 8]; BANKS],
    /// The script variables (`scd_variables`).
    pub vars: [u16; 256],
    /// The interaction areas' gates, by action slot.
    pub gates: [ActionGate; ROOM_ACTION_SLOTS],
    /// `door_locks`: the unlocked doors' bits.
    pub door_locks: [u32; 2],
    /// `sce_rnd`'s state.
    pub random: u32,
}

impl Default for Re2State {
    fn default() -> Self {
        Self {
            banks: [[0; 8]; BANKS],
            vars: [0; 256],
            gates: [ActionGate::default(); ROOM_ACTION_SLOTS],
            door_locks: [0; 2],
            random: 0x1382_01C3,
        }
    }
}

impl Re2State {
    pub fn flag(&self, bank: u8, index: u8) -> bool {
        let Some(words) = self.banks.get(usize::from(bank)) else {
            return false;
        };
        words[usize::from(index >> 5) & 7] & (0x8000_0000 >> (index & 31)) != 0
    }

    pub fn set_flag(&mut self, bank: u8, index: u8, op: u8) {
        let Some(words) = self.banks.get_mut(usize::from(bank)) else {
            return;
        };
        let word = &mut words[usize::from(index >> 5) & 7];
        let bit = 0x8000_0000 >> (index & 31);
        match op {
            0 => *word &= !bit,
            1 => *word |= bit,
            7 => *word ^= bit,
            _ => {}
        }
    }
}

/// The gate on one interaction area, on top of its [`RoomAction`] box.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ActionGate {
    /// The four corners of a `*_4p` area, which replace the box.
    pub quad: Option<[[i16; 2]; 4]>,
    /// The floor the area is on; `None` for every floor.
    pub floor: Option<u8>,
}

impl ActionGate {
    /// Whether `(x, z)` on `floor` is in the area whose box is `zone`, with
    /// RE2's signed coordinates (`Sce_at_check`).
    pub fn contains(&self, zone: [i16; 4], x: i32, z: i32, floor: u8) -> bool {
        if self.floor.is_some_and(|gate| gate != floor) {
            return false;
        }
        match self.quad {
            Some(corners) => Zone {
                corners,
                ..Zone::default()
            }
            .contains_signed(x, z),
            None => {
                let [left, near, width, depth] = zone.map(i32::from);
                let (width, depth) = (width & 0xFFFF, depth & 0xFFFF);
                (0..width).contains(&(x - left)) && (0..depth).contains(&(z - near))
            }
        }
    }
}

/// The RE2 half of a door record.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Re2Door {
    /// The destination's RoomId stage, already 1-based.
    pub stage: u8,
    pub room: u8,
    /// The camera the player arrives under.
    pub cut: u8,
}

/// The probe flags an RE2 `sat` byte means to [`GameState`]'s area scan: its
/// bit `0x10` is the action key, `0x20` probes the reach point, and the low
/// nibble selects who can trigger it.
fn probe_flags(sat: u8) -> u8 {
    let mut flags = sat & 0x0F;
    if sat & 0x10 != 0 {
        flags = 0x81;
    }
    if sat & 0x20 == 0 {
        flags |= 0x40;
    }
    flags
}

/// The script host over the game state.
pub struct GameHost<'a> {
    pub game: &'a mut GameState,
}

impl GameHost<'_> {
    /// `door_aot_set` and `door_aot_set_4p`: the area and its door record.
    fn set_door(&mut self, ops: &[i64], four_points: bool) {
        let at = |index: usize| ops.get(index).copied().unwrap_or(0);
        let slot = at(0) as usize;
        if slot >= ROOM_ACTION_SLOTS {
            eprintln!("warning: RE2 door area {slot} is past the action slots");
            return;
        }
        let sat = at(2) as u8;
        let floor = at(3) as u8;
        let (zone, quad, rest) = if four_points {
            let corner = |index: usize| [at(5 + index * 2) as i16, at(6 + index * 2) as i16];
            (
                [0; 4],
                Some([corner(0), corner(1), corner(2), corner(3)]),
                13,
            )
        } else {
            (
                [at(5) as i16, at(6) as i16, at(7) as i16, at(8) as i16],
                None,
                9,
            )
        };
        let next = |index: usize| at(rest + index);
        let flags = probe_flags(sat);
        self.game.re2.gates[slot] = ActionGate {
            quad,
            floor: (floor < 0x80).then_some(floor),
        };
        self.game.room_actions[slot] = Some(RoomAction {
            slot: slot as u8,
            kind: RoomActionKind::Door,
            zone,
            sce: at(1) as u8,
            handler: 1,
            flags,
            params: [0; 8],
            item_data: None,
            room_items_flag: 0xFF,
            reach_animation: false,
        });
        self.game.doors[slot] = Some(Door {
            slot: slot as u8,
            zone,
            // The door type, which `Door_load` hands the DO2 script.
            direction: next(9) as u8,
            sfx: next(10) as u8,
            // The DO2 file.
            door_type: next(8) as u8,
            camera: next(6) as u8 & 0x3F,
            lock: next(11) as u8,
            next_room: next(5) as u8,
            next_pos: [next(0) as i32, next(1) as i32, next(2) as i32],
            next_angle: next(3) as i16,
            key: next(12) as u8,
            sub_type: flags,
            re2: Some(Re2Door {
                stage: next(4) as u8 + 1,
                room: next(5) as u8,
                cut: next(6) as u8,
            }),
        });
    }
}

impl Host for GameHost<'_> {
    fn flag(&self, bank: u8, index: u8) -> bool {
        self.game.re2.flag(bank, index)
    }

    fn set_flag(&mut self, bank: u8, index: u8, op: u8) {
        self.game.re2.set_flag(bank, index, op);
    }

    fn var(&self, index: u8) -> u16 {
        self.game.re2.vars[usize::from(index)]
    }

    fn set_var(&mut self, index: u8, value: u16) {
        self.game.re2.vars[usize::from(index)] = value;
    }

    fn waiting(&self) -> bool {
        self.game.message.active
    }

    fn random(&mut self) -> u16 {
        let state = &mut self.game.re2.random;
        *state = state.wrapping_mul(0x41C6_4E6D).wrapping_add(0x3039);
        (*state >> 16) as u16
    }

    fn exec(&mut self, _task: usize, op: &'static Op, operands: &[i64]) -> Step {
        match op.op {
            0x3B => self.set_door(operands, false),
            0x68 => self.set_door(operands, true),
            _ => {}
        }
        Step::Next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flag_bit_counts_from_the_top_of_its_word() {
        let mut state = Re2State::default();
        state.set_flag(3, 33, 1);
        assert_eq!(state.banks[3][1], 0x4000_0000);
        assert!(state.flag(3, 33));
        state.set_flag(3, 33, 7);
        assert!(!state.flag(3, 33));
    }

    #[test]
    fn a_door_op_builds_its_area_and_record() {
        let mut game = GameState::default();
        let mut host = GameHost { game: &mut game };
        // Room 1000's first door: the action key at the reach point, to room
        // 1010's cut 0 through DO2 file 1.
        let ops = [
            0, 1, 49, 0, 0, -22155, -20156, 1770, 2280, -15332, 0, -11799, 1832, 0, 1, 0, 0, 1, 2,
            3, 4, 5, 6,
        ];
        host.exec(0, crate::scd2::opcode::lookup(0x3B).unwrap(), &ops);
        let action = game.room_actions[0].unwrap();
        assert_eq!(action.flags, 0x81);
        assert!(game.re2.gates[0].contains(action.zone, -21000, -19000, 0));
        assert!(!game.re2.gates[0].contains(action.zone, -21000, -19000, 1));
        assert!(!game.re2.gates[0].contains(action.zone, -23000, -19000, 0));
        let door = game.doors[0].unwrap();
        assert_eq!(door.next_pos, [-15332, 0, -11799]);
        assert_eq!(door.door_type, 1);
        assert_eq!(door.direction, 2);
        assert_eq!(door.lock, 4);
        assert_eq!(door.key, 5);
        assert_eq!(
            door.re2,
            Some(Re2Door {
                stage: 1,
                room: 1,
                cut: 0
            })
        );
    }

    #[test]
    fn an_auto_area_probes_the_body_and_an_action_area_the_reach_point() {
        assert_eq!(probe_flags(0x31), 0x81);
        assert_eq!(probe_flags(0x11), 0xC1);
        assert_eq!(probe_flags(0x01), 0x41);
        assert_eq!(probe_flags(0x21), 0x01);
    }
}
