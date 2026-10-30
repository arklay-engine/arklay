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
    /// The camera `cut_old` returns to.
    pub cut_old: u8,
    /// Interaction-area events waiting for the VM (`Sce_event`): task, event.
    pub pending_events: Vec<(u8, u8)>,
    /// `Sce_Damage` areas the player's position probe hit this tick: the box,
    /// the kind and the damage. The tick applies them to the player.
    pub pending_damage: Vec<([i16; 4], u8, i16)>,
    /// Camera-table edits waiting for the room (`cut_be_set`, `cut_replace`).
    pub pending_cuts: Vec<CutEdit>,
    /// A door has just handed back, and `fg_stop` hasn't come down yet.
    pub door_settling: bool,
    /// What each task's `work_set` selected.
    pub work: [Work; TASK_SLOTS],
    /// A scripted move of the player, applied at the next tick.
    pub pending_player: PendingPlayer,
    /// The player's `PLAYER_WORK`.
    pub player: crate::player2::Work,
    /// The room's running scripts.
    pub vm: Option<Box<super::vm::Vm>>,
    /// Scenario B (`FG_STATUS_SCENARIO`).
    pub scenario_b: bool,
    /// The room music table (`bgm_tbl`).
    pub bgm: crate::bgm2::BgmTable,
    /// The main music playing.
    pub main_music: Option<u8>,
}

/// `pos_set` and `dir_set` on the player.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PendingPlayer {
    pub pos: Option<[i32; 3]>,
    pub angle: Option<u16>,
}

/// An edit of the room's camera switch zones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CutEdit {
    /// `cut_be_set`: entry `index` of camera `cut`'s zones gets gate `value`.
    SetGate { cut: u8, index: u8, value: u8 },
    /// `cut_replace`: swap two values across every zone's gate and floor.
    Replace { a: u8, b: u8 },
}

/// A `work_set` target. Only the player is modelled so far.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Work {
    #[default]
    Player,
    Other,
}

/// The bytes of [`Re2State::to_save_bytes`].
const SAVE_LEN: usize = (BANKS * 8 + 2) * 4 + 512 + crate::bgm2::ROOMS * 2 + 5;

/// The script's task slots.
const TASK_SLOTS: usize = super::vm::TASKS;
/// `set 2, 7`: `FG_STOP_DISABLE_INPUT`, a cutscene holding the player's input.
const STOP_BANK: u8 = 2;
const STOP_INPUT: u8 = 7;
/// The item-collected flag bank (`FlagGroup::Item`).
pub const ITEM_BANK: u8 = 8;
/// The ink ribbon's item id.
pub const ITEM_INK_RIBBON: u8 = 0x1E;

/// Whether picked-up `item` joins a stack: the ammunition and ink ribbons.
pub fn item_stacks(item: u8) -> bool {
    (0x14..=ITEM_INK_RIBBON).contains(&item)
}

/// `Sce_at`'s door lock key values.
pub const KEY_UNLOCK: u8 = 0xFE;
pub const KEY_LOCKED: u8 = 0xFF;

/// The messages the executable draws itself; RE2's own are not read yet, so
/// the pack's language file spells them (`MESSAGE_<nn>`).
pub const MESSAGE_PICKED_UP: u8 = 0;
pub const MESSAGE_LOCKED: u8 = 1;
pub const MESSAGE_LOCKED_OTHER_SIDE: u8 = 2;
pub const MESSAGE_UNLOCKED: u8 = 3;
pub const MESSAGE_USED_KEY: u8 = 4;
pub const MESSAGE_FULL: u8 = 5;

/// The script variables the engine owns (`scd_var_*`).
pub const VAR_FORWARD_AREA: usize = 0;
pub const VAR_BODY_AREA: usize = 1;
const VAR_STAGE: u8 = 24;
const VAR_ROOM: u8 = 25;
const VAR_CUT: u8 = 26;

/// RE2's `Sce_at_jump_tbl` indices (`SceKind`).
pub const SCE_AUTO: u8 = 0;
const SCE_DOOR: u8 = 1;
const SCE_ITEM: u8 = 2;
const SCE_MESSAGE: u8 = 4;
const SCE_EVENT: u8 = 5;
const SCE_SAVE: u8 = 9;
const SCE_ITEMBOX: u8 = 10;
pub const SCE_DAMAGE: u8 = 11;

impl Default for Re2State {
    fn default() -> Self {
        Self {
            banks: [[0; 8]; BANKS],
            vars: [0; 256],
            gates: [ActionGate::default(); ROOM_ACTION_SLOTS],
            door_locks: [0; 2],
            random: 0x1382_01C3,
            cut_old: 0,
            pending_events: Vec::new(),
            pending_damage: Vec::new(),
            pending_cuts: Vec::new(),
            door_settling: false,
            work: [Work::Player; TASK_SLOTS],
            pending_player: PendingPlayer::default(),
            player: crate::player2::Work::default(),
            vm: None,
            scenario_b: false,
            bgm: Default::default(),
            main_music: None,
        }
    }
}

impl Re2State {
    /// The state a save keeps: the flags, variables, door locks and music
    /// table. The rest belongs to the room, and the room's init rebuilds it.
    pub fn to_save_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(SAVE_LEN);
        for word in self.banks.iter().flatten().chain(&self.door_locks) {
            out.extend_from_slice(&word.to_le_bytes());
        }
        for var in &self.vars {
            out.extend_from_slice(&var.to_le_bytes());
        }
        for room in self.bgm.words() {
            out.extend_from_slice(&room.to_le_bytes());
        }
        out.extend_from_slice(&self.random.to_le_bytes());
        out.push(u8::from(self.scenario_b));
        out
    }

    /// Restore [`Self::to_save_bytes`]; `None` for bytes of another length.
    pub fn load_save_bytes(&mut self, bytes: &[u8]) -> Option<()> {
        if bytes.len() != SAVE_LEN {
            return None;
        }
        let mut words = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|raw| u32::from_le_bytes(*raw));
        for word in self.banks.iter_mut().flatten().chain(&mut self.door_locks) {
            *word = words.next()?;
        }
        let rest = &bytes[(BANKS * 8 + 2) * 4..];
        let (vars, rest) = rest.split_at(512);
        for (var, raw) in self.vars.iter_mut().zip(vars.as_chunks::<2>().0) {
            *var = u16::from_le_bytes(*raw);
        }
        let (rooms, rest) = rest.split_at(crate::bgm2::ROOMS * 2);
        for (room, raw) in self
            .bgm
            .words_mut()
            .iter_mut()
            .zip(rooms.as_chunks::<2>().0)
        {
            *room = u16::from_le_bytes(*raw);
        }
        self.random = u32::from_le_bytes(rest[..4].try_into().ok()?);
        self.scenario_b = rest[4] != 0;
        Some(())
    }

    /// Whether a script holds the player's input.
    pub fn input_stopped(&self) -> bool {
        self.flag(STOP_BANK, STOP_INPUT)
    }

    /// `fg_stop`'s top bit, which keeps `Player_move` from running.
    pub fn player_stopped(&self) -> bool {
        self.flag(STOP_BANK, 0)
    }

    /// `Flg_on(&fg_room, ...)`, a finished scripted walk's.
    pub fn raise_room_flag(&mut self, flag: u8) {
        self.set_flag(ROOM_BANK, flag, 1);
    }

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

/// `work_set`'s kind byte for the player (`WK_PLAYER`).
const WK_PLAYER: i64 = 1;

/// The work members holding the position (`Member_set_branch`).
const MEMBER_POS_X: usize = 0x0B;
/// The member that reads and writes `cdir.y`.
const MEMBER_ANGLE: u8 = 0x0F;
/// The room's flag bank, which `plc_dest` raises its arrival flag in.
const ROOM_BANK: u8 = 5;

/// `Member_cmp`'s comparison.
fn compare_member(op: u8, value: i32, operand: i32) -> bool {
    match op {
        0 => value == operand,
        1 => operand < value,
        2 => operand <= value,
        3 => value < operand,
        4 => value <= operand,
        5 => value != operand,
        6 => value & operand != 0,
        _ => false,
    }
}

/// An area operand block's box and, for a `*_4p` area, its corners.
fn area_shape(ops: &[i64], four_points: bool) -> ([i16; 4], Option<[[i16; 2]; 4]>) {
    let at = |index: usize| ops.get(index).copied().unwrap_or(0);
    if four_points {
        let corner = |index: usize| [at(5 + index * 2) as i16, at(6 + index * 2) as i16];
        ([0; 4], Some([corner(0), corner(1), corner(2), corner(3)]))
    } else {
        (
            [at(5) as i16, at(6) as i16, at(7) as i16, at(8) as i16],
            None,
        )
    }
}

/// An interaction area's table entry. Every kind but `SCE_AUTO` records the
/// probe's hit in the script variables, so the others stay live even with
/// nothing to do.
fn area_action(slot: u8, sce: u8, sat: u8, zone: [i16; 4], params: [u8; 8]) -> RoomAction {
    let kind = match sce {
        SCE_DOOR => RoomActionKind::Door,
        SCE_ITEM => RoomActionKind::Item,
        SCE_MESSAGE => RoomActionKind::Message,
        SCE_EVENT => RoomActionKind::Event,
        SCE_SAVE => RoomActionKind::Typewriter,
        SCE_ITEMBOX => RoomActionKind::ItemBox,
        _ => RoomActionKind::Other,
    };
    RoomAction {
        slot,
        kind,
        zone,
        sce,
        handler: u8::from(sce != SCE_AUTO),
        flags: probe_flags(sat),
        params,
        item_data: None,
        room_items_flag: 0xFF,
        reach_animation: false,
    }
}

/// Note a script instruction with no game side yet, once per run.
fn log_unmodelled(mnemonic: &'static str) {
    use std::collections::BTreeSet;
    use std::sync::Mutex;
    static SEEN: Mutex<BTreeSet<&'static str>> = Mutex::new(BTreeSet::new());
    if SEEN.lock().is_ok_and(|mut seen| seen.insert(mnemonic)) {
        crate::logging::write(&format!("RE2 script: {mnemonic} is not modelled"));
    }
}

/// The script host over the game state.
pub struct GameHost<'a> {
    pub game: &'a mut GameState,
}

impl GameHost<'_> {
    /// `aot_set` and `aot_set_4p`: an interaction area with no door record.
    fn set_area(&mut self, ops: &[i64], four_points: bool) {
        let at = |index: usize| ops.get(index).copied().unwrap_or(0);
        let rest = if four_points { 13 } else { 9 };
        let (zone, quad) = area_shape(ops, four_points);
        let data: [u8; 6] = std::array::from_fn(|index| at(rest + index) as u8);
        let mut params = [0; 8];
        params[..6].copy_from_slice(&data);
        self.write_area(
            at(0) as usize,
            at(1) as u8,
            at(2) as u8,
            at(3) as u8,
            zone,
            quad,
            params,
        );
    }

    /// `item_aot_set` and `item_aot_set_4p`. A collected item's area is inert
    /// (`Sce` zero), as its collected flag says.
    fn set_item_area(&mut self, ops: &[i64], four_points: bool) {
        let at = |index: usize| ops.get(index).copied().unwrap_or(0);
        let rest = if four_points { 13 } else { 9 };
        let (zone, quad) = area_shape(ops, four_points);
        let (item, amount, flag, model, action) = (
            at(rest) as u8,
            at(rest + 1) as u8,
            at(rest + 2) as u8,
            at(rest + 3) as u8,
            at(rest + 4) as u8,
        );
        let sce = if self.game.re2.flag(ITEM_BANK, flag) {
            SCE_AUTO
        } else {
            at(1) as u8
        };
        let params = [item, amount, flag, model, action, 0, 0, 0];
        self.write_area(
            at(0) as usize,
            sce,
            at(2) as u8,
            at(3) as u8,
            zone,
            quad,
            params,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn write_area(
        &mut self,
        slot: usize,
        sce: u8,
        sat: u8,
        floor: u8,
        zone: [i16; 4],
        quad: Option<[[i16; 2]; 4]>,
        params: [u8; 8],
    ) {
        if slot >= ROOM_ACTION_SLOTS {
            eprintln!("warning: RE2 area {slot} is past the action slots");
            return;
        }
        self.game.re2.gates[slot] = ActionGate {
            quad,
            floor: (floor < 0x80).then_some(floor),
        };
        self.game.doors[slot] = None;
        self.game.room_actions[slot] = Some(area_action(slot as u8, sce, sat, zone, params));
    }

    /// `Aot_reset`: an existing area's kind, trigger and data change.
    fn reset_area(&mut self, ops: &[i64]) {
        let at = |index: usize| ops.get(index).copied().unwrap_or(0);
        let slot = at(0) as usize;
        let Some(Some(old)) = self.game.room_actions.get(slot).copied() else {
            return;
        };
        let data: [u8; 6] = std::array::from_fn(|index| at(3 + index) as u8);
        let mut params = [0; 8];
        params[..6].copy_from_slice(&data);
        self.game.room_actions[slot] = Some(area_action(
            slot as u8,
            at(1) as u8,
            at(2) as u8,
            old.zone,
            params,
        ));
    }

    /// `cut_chg`: a scripted camera, which stays until a `cut_auto` or
    /// `cut_old`.
    fn change_cut(&mut self, cut: u8) {
        self.game.re2.cut_old = self.game.camera.current_cut as u8;
        self.game.camera.locked = true;
        self.game.set_camera_cut(usize::from(cut & 0x7F));
    }

    /// Set the player's position or facing for `pos_set` and `dir_set`.
    fn work_is_player(&self, task: usize) -> bool {
        self.game.re2.work.get(task) == Some(&Work::Player)
    }

    /// `Load_member_branch` for what the engine models: the player.
    fn member(&self, task: usize, member: u8) -> Option<i32> {
        if !self.work_is_player(task) {
            return None;
        }
        if let Some(axis) = usize::from(member)
            .checked_sub(MEMBER_POS_X)
            .filter(|axis| *axis < 3)
        {
            return self.game.entities[0].pos.get(axis).copied();
        }
        match member {
            MEMBER_ANGLE => Some(i32::from(self.game.entities[0].angle & 0x0FFF)),
            _ => crate::player2::member(&self.game.re2.player, member),
        }
    }

    /// `Member_set_branch` on the player.
    fn set_player_member(&mut self, member: u8, value: i32) {
        if let Some(axis) = usize::from(member)
            .checked_sub(MEMBER_POS_X)
            .filter(|axis| *axis < 3)
        {
            let mut pos = self
                .game
                .re2
                .pending_player
                .pos
                .unwrap_or(self.game.entities[0].pos);
            pos[axis] = value;
            self.game.re2.pending_player.pos = Some(pos);
        } else if member == MEMBER_ANGLE {
            self.game.re2.pending_player.angle = Some(value as u16 & 0x0FFF);
        } else {
            crate::player2::set_member(&mut self.game.re2.player, member, value);
        }
    }

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
        match index {
            VAR_CUT => self.game.camera.current_cut as u16,
            VAR_STAGE => u16::from(self.game.id.stage.saturating_sub(1)),
            VAR_ROOM => u16::from(self.game.id.room),
            _ => self.game.re2.vars[usize::from(index)],
        }
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

    fn exec(&mut self, task: usize, op: &'static Op, operands: &[i64]) -> Step {
        let at = |index: usize| operands.get(index).copied().unwrap_or(0);
        match op.mnemonic {
            "door_aot_set" => self.set_door(operands, false),
            "door_aot_set_4p" => self.set_door(operands, true),
            "aot_set" => self.set_area(operands, false),
            "aot_set_4p" => self.set_area(operands, true),
            "item_aot_set" => self.set_item_area(operands, false),
            "item_aot_set_4p" => self.set_item_area(operands, true),
            "aot_reset" => self.reset_area(operands),
            "aot_on" => {
                if let Some(Some(action)) = self.game.room_actions.get(at(0) as usize).copied() {
                    self.game.fire_re2_area(action);
                }
            }
            "message_on" => {
                self.game.show_re2_message(at(1) as u8);
            }
            "cut_chg" => self.change_cut(at(0) as u8),
            "cut_old" => {
                let cut = self.game.re2.cut_old;
                self.game.camera.locked = false;
                self.game.set_camera_cut(usize::from(cut));
            }
            "cut_auto" => self.game.camera.locked = at(0) == 0,
            "cut_be_set" => self.game.re2.pending_cuts.push(CutEdit::SetGate {
                cut: at(0) as u8,
                index: at(1) as u8,
                value: at(2) as u8,
            }),
            "cut_replace" => self.game.re2.pending_cuts.push(CutEdit::Replace {
                a: at(0) as u8,
                b: at(1) as u8,
            }),
            "work_set" => {
                let work = if at(0) == WK_PLAYER {
                    Work::Player
                } else {
                    Work::Other
                };
                if let Some(slot) = self.game.re2.work.get_mut(task) {
                    *slot = work;
                }
            }
            "pos_set" if self.work_is_player(task) => {
                self.game.re2.pending_player.pos = Some([at(1) as i32, at(2) as i32, at(3) as i32]);
            }
            "dir_set" if self.work_is_player(task) => {
                self.game.re2.pending_player.angle = Some(at(2) as u16);
            }
            "sce_bgmtbl_set" => {
                self.game
                    .re2
                    .bgm
                    .set(at(1) as u8, at(2) as u8, at(3) as u16, at(4) as u16);
            }
            "plc_motion" if self.work_is_player(task) => {
                crate::player2::plc_motion(
                    &mut self.game.re2.player,
                    at(0) as u8,
                    at(1) as u8,
                    at(2) as u8,
                );
            }
            "plc_dest" if self.work_is_player(task) => {
                let id = self.game.id.player_flag;
                crate::player2::plc_dest(
                    &mut self.game.re2.player,
                    id,
                    at(1) as u8,
                    at(2) as u8,
                    at(3) as i16,
                    at(4) as i16,
                );
            }
            // A character that isn't modelled is there as soon as it's told
            // to go: the destination's flag goes up for the `edwhile` on it.
            "plc_dest" => {
                let flag = at(2) as u8;
                if flag != 0 {
                    self.game.re2.set_flag(ROOM_BANK, flag, 1);
                }
            }
            "plc_ret" if self.work_is_player(task) => {
                crate::player2::plc_ret(&mut self.game.re2.player);
            }
            "plc_stop" if self.work_is_player(task) => {
                crate::player2::plc_stop(&mut self.game.re2.player);
            }
            "plc_rot" if self.work_is_player(task) => {
                let by = at(1) as i16;
                match at(0) {
                    0 => self.game.re2.player.sce_free[0] = by,
                    1 => self.game.re2.player.sce_free[1] = by,
                    _ => {}
                }
            }
            "plc_flg" if self.work_is_player(task) => {
                let work = &mut self.game.re2.player;
                let value = at(1) as u16;
                match at(0) {
                    0 => work.sce_flg |= value,
                    1 => work.sce_flg = value,
                    2 => work.sce_flg ^= value,
                    _ => {}
                }
            }
            "plc_cnt" if self.work_is_player(task) => {
                self.game.re2.player.move_cnt = at(0) as u8;
            }
            "member_copy" => {
                let value = self.member(task, at(1) as u8).unwrap_or(0);
                self.set_var(at(0) as u8, value as u16);
            }
            // A member with no state behind it ends the loops waiting on it, as
            // a cutscene's moving object has already arrived.
            "member_cmp" => {
                let Some(value) = self.member(task, at(1) as u8) else {
                    return Step::False;
                };
                if !compare_member(at(2) as u8, value, at(3) as i32) {
                    return Step::False;
                }
            }
            "member_set" if self.work_is_player(task) => {
                self.set_player_member(at(0) as u8, at(1) as i32);
            }
            // Any key answers a wait for one: the mask's pad layout isn't mapped.
            "sce_key_ck" | "sce_trg_ck" => {
                let pressed = if op.mnemonic == "sce_key_ck" {
                    self.game.dpad_held != 0
                } else {
                    self.game.dpad_pressed != 0
                };
                if pressed != (at(0) != 0) {
                    return Step::False;
                }
            }
            "keep_item_ck" => {
                if !self.game.has_item(at(0) as u8) {
                    return Step::False;
                }
            }
            "sce_item_get" => self.game.add_item(at(0) as u8, at(1) as u8),
            "sce_item_lost" => {
                self.game.remove_item(at(0) as u8);
            }
            _ => log_unmodelled(op.mnemonic),
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
    fn the_saved_state_round_trips() {
        let mut state = Re2State::default();
        state.set_flag(8, 15, 1);
        state.vars[29] = 0x1234;
        state.door_locks[1] = 7;
        state.bgm.set(1, 1, 0x0102, 0);
        state.scenario_b = true;
        let bytes = state.to_save_bytes();
        let mut restored = Re2State::default();
        restored.load_save_bytes(&bytes).unwrap();
        assert_eq!(restored, state);
        assert!(restored.load_save_bytes(&bytes[1..]).is_none());
    }

    #[test]
    fn a_picked_up_item_stays_collected() {
        let mut game = GameState::default();
        game.is_re2_room = true;
        let ops = [8, 2, 49, 0, 0, -100, -100, 200, 200, 0x14, 15, 3, 1, 0];
        let mut host = GameHost { game: &mut game };
        let item = crate::scd2::opcode::lookup(0x4E).unwrap();
        host.exec(0, item, &ops);
        let action = game.room_actions[8].unwrap();
        assert_eq!(action.kind, RoomActionKind::Item);
        game.fire_re2_area(action);
        assert_eq!(game.item_count(0x14), 15);
        assert!(game.re2.flag(ITEM_BANK, 3));
        assert_eq!(game.room_actions[8].unwrap().handler, 0);
        // The room's next init sees the flag.
        let mut host = GameHost { game: &mut game };
        host.exec(0, item, &ops);
        assert_eq!(game.room_actions[8].unwrap().handler, 0);
    }

    #[test]
    fn a_locked_door_takes_its_key_then_opens() {
        let mut game = GameState::default();
        game.is_re2_room = true;
        let door = Door {
            lock: 0x85,
            key: 0x31,
            re2: Some(Re2Door::default()),
            ..Door::default()
        };
        assert!(!game.try_re2_door_for_test(&door));
        assert_eq!(game.re2.door_locks, [0, 0]);
        game.message.active = false;
        game.add_item(0x31, 1);
        assert!(!game.try_re2_door_for_test(&door));
        assert_ne!(game.re2.door_locks, [0, 0]);
    }

    #[test]
    fn a_member_with_no_state_ends_the_wait_on_it() {
        let mut game = GameState::default();
        let mut host = GameHost { game: &mut game };
        let work = crate::scd2::opcode::lookup(0x2E).unwrap();
        host.exec(0, work, &[4, 0]);
        let cmp = crate::scd2::opcode::lookup(0x3E).unwrap();
        assert_eq!(host.exec(0, cmp, &[0, 12, 3, -5600]), Step::False);
        host.exec(0, work, &[1, 0]);
        // The player's height is 0, not below -5600.
        assert_eq!(host.exec(0, cmp, &[0, 12, 3, -5600]), Step::False);
        assert_eq!(host.exec(0, cmp, &[0, 12, 4, 0]), Step::Next);
    }

    #[test]
    fn an_auto_area_probes_the_body_and_an_action_area_the_reach_point() {
        assert_eq!(probe_flags(0x31), 0x81);
        assert_eq!(probe_flags(0x11), 0xC1);
        assert_eq!(probe_flags(0x01), 0x41);
        assert_eq!(probe_flags(0x21), 0x01);
    }
}
