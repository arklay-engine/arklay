//! RE2's player machine: `Player_move` in bio2's 1.10 PC executable.
//!
//! The port keeps the original's structure and names. Every tick
//! `routine[0]` picks a mode: 1 is the pad (`pl_move`, whose `pl_br_*` read the
//! keys and whose `pl_mv_*` move), and 4 is a script's (`Spl_Scenario`, driven
//! by `plc_motion` and `plc_dest`). Speeds aren't constants: `Spd_get` reads
//! them from the root motion of the frame about to play, which is why a walk
//! changes speed through its stride.
//!
//! What isn't ported is in `docs/re2-deviations.md`.

use crate::model::{Clip, ClipFrame, Keyframe};
use crate::player::{self, ClipSource, Input, Options, PlayerState};
use crate::state::RoomState;

/// `Key`, the mapped pad word `pl_br_*` test.
const KEY_FORWARD: u16 = 0x0001;
const KEY_RIGHT: u16 = 0x0002;
const KEY_BACKWARD: u16 = 0x0004;
const KEY_LEFT: u16 = 0x0008;
const KEY_TURN: u16 = KEY_RIGHT | KEY_LEFT;
const KEY_RUN: u16 = 0x0200;

/// `routine[0]`: the pad's mode and the scripts' mode.
const MODE_MOVE: u8 = 1;
const MODE_DAMAGE: u8 = 2;
const MODE_DEAD: u8 = 3;
const MODE_SCENARIO: u8 = 4;
/// `routine[1]` under [`MODE_MOVE`]: the `pl_br_*` and `pl_mv_*` tables.
const MOVE_IDLE: u8 = 0;
const MOVE_FORWARD: u8 = 1;
const MOVE_RUN: u8 = 2;
const MOVE_BACKWARD: u8 = 3;
const MOVE_ROTATE: u8 = 4;
/// The port's optional quick turn, past the original's table.
const MOVE_QUICK_TURN: u8 = 0x40;
/// `Plc_stop`'s routine, which no table entry animates.
const STOPPED_ROUTINE: [u8; 4] = [MODE_SCENARIO, 0x14, 0, 0];

/// A clip by health (`D_life_u`), `pl_mv_00`..`04`.
const IDLE_CLIPS: [u8; 3] = [2, 5, 8];
const WALK_CLIPS: [u8; 3] = [0, 3, 6];
const RUN_CLIPS: [u8; 3] = [1, 4, 7];
const BACK_CLIPS: [u8; 3] = [0, 9, 9];
const ROTATE_CLIPS: [u8; 3] = [0, 3, 6];
/// A turn per tick by health, `pl_br_01`..`04`.
const FORWARD_TURN: [u16; 3] = [0x28, 0x20, 0x16];
const RUN_TURN: [u16; 3] = [0x38, 0x30, 0x18];
const BACK_TURN: [u16; 3] = [0x28, 0x10, 0x10];
const ROTATE_TURN: [u16; 3] = [0x50, 0x30, 0x20];
/// A run only steers above this speed (`pl_br_02`).
const RUN_TURN_MIN_SPEED: i16 = 0x32;
/// What `pl_mv_02` takes off a run's speed unless badly hurt.
const RUN_SLOWDOWN: i16 = 0x11;
/// The health below which `D_life_u` rises.
const CAUTION_LIFE: i32 = 0x65;
const DANGER_LIFE: i32 = 0x15;
/// `J_seq`'s blend step, `Late_flg` in every player `Joint_move`.
const BLEND_STEP: u16 = 0x200;
/// A motion change's blend: `Hokan_flg` in the `0x70000` the routines write.
const FULL_BLEND: u8 = 7;
/// The blend step of `Pl_damage`'s `Joint_move`s.
const DAMAGE_BLEND_STEP: u16 = 0x400;
/// The blend `Pl_damage` starts its clips with (`0x30000` in `Move_no`'s word).
const DAMAGE_BLEND: u8 = 3;
/// `Pl_damage`'s clips, all in the character's own bank.
const DAMAGE_CLIP: u8 = 5;
const STAGGER_CLIPS: [u8; 2] = [3, 4];
const FALL_CLIP: u8 = 2;
/// How far `Pl_damage` knocks the player in a stagger.
const STAGGER_SPEED: i16 = 200;
/// `Damage_cnt`'s top bit: the player can't be damaged. The rest counts down.
const DAMAGE_LOCK: u8 = 0x80;
/// `Status_flg`'s bit: damage just ended, so the next area hit is forgiven.
const STATUS_RECOVERED: u16 = 2;
/// The EDD frame bits: a footfall, and that it's the second kind of floor.
pub const FRAME_FOOTSTEP: u16 = 0x4000;
pub const FRAME_FOOTSTEP_ALT: u16 = 0x2000;

/// The scripted walks' constant speeds by character id (`Spl_sce04`..`08`).
const SCE_WALK_SPEED: [i16; 16] = [
    0x4B, 0x46, 0x4B, 0x46, 0x4B, 0x46, 0x4B, 0x46, 0x4B, 0x46, 0x4B, 0x4B, 0x4B, 0x4B, 0x46, 0x32,
];
const SCE_RUN_SPEED: [i16; 16] = [
    0xC8, 0xD2, 0xC8, 0xD2, 0xC8, 0xD2, 0xC8, 0xD2, 0xC8, 0xD2, 0xC8, 0xC8, 0xC8, 0xC8, 0xDC, 0x78,
];
const SCE_BACK_SPEED: [i16; 16] = [
    0x46, 0x41, 0x46, 0x41, 0x46, 0x41, 0x46, 0x41, 0x46, 0x41, 0x46, 0x46, 0x46, 0x46, 0x37, 0x23,
];
const SCE_BACK_RUN_SPEED: [i16; 16] = [
    0x28, 0x2D, 0x28, 0x2D, 0x28, 0x2D, 0x28, 0x2D, 0x28, 0x2D, 0x28, 0x28, 0x28, 0x28, 0x2D, 0x1E,
];
/// `Plc_dest`'s turn rates for the walks, by routine; the character only
/// changes the last.
const DEST_WALK_TURN: i16 = 0x30;
const DEST_RUN_TURN: i16 = 0x48;
const DEST_BACK_TURN: i16 = 0x30;
const DEST_ROTATE_TURN: [i16; 16] = [
    0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x58, 0x50,
];
/// How close to a destination counts as being there (`Spl_sce_mv02`).
const ARRIVE_DISTANCE: i64 = 100;
/// How close the facing must be before a walk starts (`Spl_sce_mv01`).
const FACING_TOLERANCE: i32 = 0x15E;
/// How close the facing must be for `Spl_sce15`'s turn to end.
const TURN_TOLERANCE: i32 = 0x80;

/// The animation bank a motion plays from: `pKan_t_ptr` and `pSub0_kan_t_ptr`.
/// A PLD's own clips, the held weapon's, or the room's.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Bank {
    Pld,
    #[default]
    Weapon,
    Room,
}

/// The player's part of `PLAYER_WORK`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Work {
    /// `Routine_0`..`3`.
    pub routine: [u8; 4],
    pub move_no: u8,
    pub move_cnt: u8,
    pub hokan: u8,
    pub bank: Bank,
    pub d_life_u: u8,
    pub spd: [i16; 3],
    spd_base: [i16; 3],
    timer0: u16,
    /// The flags of `pNow_seq`, the frame `Joint_move` played last.
    now_seq: u16,
    /// `Type`: the weapon, and the bit that stops the player being moved.
    pub kind: u16,
    /// `Sce_flg`: a script's motion options.
    pub sce_flg: u16,
    pub sce_flg_no: u8,
    /// `Sce_free0` and `Sce_free1`: a turn rate, or a turn at the motion's end.
    pub sce_free: [i16; 2],
    /// `Dest_x` and `Dest_z`.
    pub dest: [i16; 2],
    /// `Plc_stop`'s saved routine.
    stopped: [u8; 4],
    /// `Damage_cnt` and `Status_flg`, for `Pl_damage` and `Sce_Damage`.
    damage_cnt: u8,
    status_flg: u16,
}

impl Default for Work {
    fn default() -> Self {
        Self {
            // `Pl_init`, then the first tick of `pl_mv_00`.
            routine: [MODE_MOVE, MOVE_IDLE, 1, 0],
            move_no: IDLE_CLIPS[0],
            move_cnt: 1,
            hokan: FULL_BLEND - 1,
            bank: Bank::Weapon,
            d_life_u: 0,
            spd: [0; 3],
            spd_base: [0; 3],
            timer0: 0,
            now_seq: 0,
            kind: 0,
            sce_flg: 0,
            sce_flg_no: 0,
            sce_free: [0; 2],
            dest: [0; 2],
            stopped: [0; 4],
            damage_cnt: 0,
            status_flg: 0,
        }
    }
}

impl Work {
    /// `Pl_init`: what a room's first tick of play starts from.
    pub fn entering_room(&self) -> Self {
        Self {
            d_life_u: self.d_life_u,
            kind: self.kind,
            ..Self::default()
        }
    }

    /// `Pl_init`, which leaves the routines' first tick to the room's first
    /// `Player_move`.
    pub fn after_pl_init(mut self) -> Self {
        self.routine = [MODE_MOVE, MOVE_IDLE, 0, 0];
        self.hokan = 0;
        self
    }

    /// The trace's `beh` and `as`, and its `mot` field: `Routine_2`, `Routine_3`,
    /// `Move_no`, `Move_cnt`, `Hokan_flg`, `D_life_u` and the speed.
    pub fn trace_fields(&self) -> ([u8; 2], [i32; 9]) {
        (
            [self.routine[0], self.routine[1]],
            [
                i32::from(self.routine[2]),
                i32::from(self.routine[3]),
                i32::from(self.move_no),
                i32::from(self.move_cnt),
                i32::from(self.hokan),
                i32::from(self.d_life_u),
                i32::from(self.spd[0]),
                i32::from(self.spd[1]),
                i32::from(self.spd[2]),
            ],
        )
    }
}

/// The clips and keyframes behind each [`Bank`].
#[derive(Clone, Copy)]
pub struct Banks<'a> {
    pub pld: (&'a [Clip], &'a [Keyframe]),
    pub weapon: (&'a [Clip], &'a [Keyframe]),
    pub room: (&'a [Clip], &'a [Keyframe]),
}

impl<'a> Banks<'a> {
    fn get(&self, bank: Bank) -> (&'a [Clip], &'a [Keyframe]) {
        match bank {
            Bank::Pld => self.pld,
            Bank::Weapon => self.weapon,
            Bank::Room => self.room,
        }
    }
}

/// One tick's inputs from the rest of the game.
pub struct Tick<'a> {
    pub room: &'a RoomState,
    pub banks: Banks<'a>,
    pub input: Input,
    pub options: Options,
    /// The character id, `Id`.
    pub id: u8,
    pub life: i32,
    pub poisoned: bool,
    /// `fg_stop`'s top bit, which freezes the player's whole routine.
    pub stopped: bool,
}

/// `Player_move`. The result is the room flag a finished scripted walk raises.
pub fn player_move(work: &mut Work, player: &mut PlayerState, tick: &Tick<'_>) -> Option<u8> {
    if tick.stopped {
        return None;
    }
    if work.damage_cnt & !DAMAGE_LOCK != 0 {
        work.damage_cnt -= 1;
    }
    let mut mover = Mover {
        work,
        player,
        tick,
        arrived: None,
    };
    match mover.work.routine[0] {
        MODE_MOVE => mover.pl_move(),
        MODE_DAMAGE => mover.pl_damage(),
        MODE_SCENARIO => mover.scenario(),
        _ => {}
    }
    mover.arrived
}

/// A tick of [`player_move`]: the state the original keeps in globals.
struct Mover<'a, 'b> {
    work: &'a mut Work,
    player: &'a mut PlayerState,
    tick: &'a Tick<'b>,
    arrived: Option<u8>,
}

fn key_word(input: &Input) -> u16 {
    let bit = |set: bool, value: u16| if set { value } else { 0 };
    bit(input.up, KEY_FORWARD)
        | bit(input.right, KEY_RIGHT)
        | bit(input.down, KEY_BACKWARD)
        | bit(input.left, KEY_LEFT)
        | bit(input.run, KEY_RUN)
}

impl Mover<'_, '_> {
    fn set_routine(&mut self, routine: u8) {
        self.work.routine = [MODE_MOVE, routine, 0, 0];
    }

    /// The health check at the top of every `pl_br_*`.
    fn update_life_clips(&mut self) {
        if self.work.routine[2] != 0 && self.work.move_cnt != 0 {
            return;
        }
        let before = self.work.d_life_u;
        let mut level = 0;
        if self.tick.life < CAUTION_LIFE || self.tick.poisoned {
            level = 1;
        }
        if self.tick.life < DANGER_LIFE {
            level = 2;
        }
        self.work.d_life_u = level;
        if before != level {
            self.work.routine[2] = 0;
        }
    }

    fn turn(&mut self, key: u16, rates: &[u16; 3]) {
        let rate = rates[usize::from(self.work.d_life_u)];
        if key & KEY_RIGHT != 0 {
            self.player.angle = self.player.angle.wrapping_add(rate) & 0x0FFF;
        }
        if key & KEY_LEFT != 0 {
            self.player.angle = self.player.angle.wrapping_sub(rate) & 0x0FFF;
        }
    }

    /// `Pl_move`: the branch on the keys, then the movement it chose.
    fn pl_move(&mut self) {
        let (input, target) = player::analog_input(
            self.player,
            self.tick.room,
            self.tick.input,
            self.tick.options,
            self.work.routine[1] == MOVE_RUN,
        );
        self.player.input = input;
        let mut key = key_word(&input);
        if target.is_some() {
            key &= !KEY_TURN;
        }
        if self.tick.options.quick_turn
            && self.work.routine[1] <= MOVE_ROTATE
            && ((input.down && !input.up && input.run_pressed) || input.quick_turn_pressed)
        {
            self.set_routine(MOVE_QUICK_TURN);
        }
        match self.work.routine[1] {
            MOVE_IDLE => self.pl_br_idle(key),
            MOVE_FORWARD => self.pl_br_forward(key),
            MOVE_RUN => self.pl_br_run(key),
            MOVE_BACKWARD => self.pl_br_backward(key),
            MOVE_ROTATE => self.pl_br_rotate(key),
            _ => {}
        }
        if let Some(target) = target {
            self.player.angle = player::analog_turned(self.player.angle, target);
        }
        match self.work.routine[1] {
            MOVE_IDLE => self.pl_mv_idle(),
            MOVE_FORWARD => self.pl_mv_forward(),
            MOVE_RUN => self.pl_mv_run(),
            MOVE_BACKWARD => self.pl_mv_backward(),
            MOVE_ROTATE => self.pl_mv_rotate(),
            MOVE_QUICK_TURN => self.mv_quick_turn(),
            _ => {}
        }
    }

    fn pl_br_idle(&mut self, key: u16) {
        self.update_life_clips();
        if key & KEY_TURN != 0 {
            self.set_routine(MOVE_ROTATE);
        }
        if key & KEY_BACKWARD != 0 {
            self.set_routine(MOVE_BACKWARD);
        }
        if key & KEY_FORWARD != 0 {
            self.set_routine(if key & KEY_RUN != 0 {
                MOVE_RUN
            } else {
                MOVE_FORWARD
            });
        }
    }

    fn pl_br_forward(&mut self, key: u16) {
        self.update_life_clips();
        if key & KEY_FORWARD == 0 {
            self.set_routine(MOVE_IDLE);
            if key & KEY_TURN != 0 {
                self.set_routine(MOVE_ROTATE);
            }
            if key & KEY_BACKWARD != 0 {
                self.set_routine(MOVE_BACKWARD);
            }
            return;
        }
        self.turn(key, &FORWARD_TURN);
        if key & KEY_RUN != 0 {
            self.set_routine(MOVE_RUN);
        }
    }

    fn pl_br_run(&mut self, key: u16) {
        self.update_life_clips();
        if key & KEY_RUN == 0 || key & KEY_FORWARD == 0 {
            self.set_routine(MOVE_IDLE);
            if key & KEY_TURN != 0 {
                self.set_routine(MOVE_ROTATE);
            }
            if key & KEY_BACKWARD != 0 {
                self.set_routine(MOVE_BACKWARD);
            }
            if key & KEY_FORWARD != 0 {
                self.set_routine(MOVE_FORWARD);
            }
            return;
        }
        if self.work.spd[0] > RUN_TURN_MIN_SPEED {
            self.turn(key, &RUN_TURN);
        }
    }

    fn pl_br_backward(&mut self, key: u16) {
        self.update_life_clips();
        if key & KEY_BACKWARD == 0 {
            self.set_routine(MOVE_IDLE);
            if key & KEY_TURN != 0 {
                self.set_routine(MOVE_ROTATE);
            }
            if key & KEY_FORWARD != 0 {
                self.set_routine(MOVE_FORWARD);
            }
            return;
        }
        self.turn(key, &BACK_TURN);
    }

    fn pl_br_rotate(&mut self, key: u16) {
        self.update_life_clips();
        if key & KEY_TURN == 0 {
            self.set_routine(MOVE_IDLE);
        }
        if key & KEY_BACKWARD != 0 {
            self.set_routine(MOVE_BACKWARD);
        }
        if key & KEY_FORWARD != 0 {
            self.set_routine(if key & KEY_RUN != 0 {
                MOVE_RUN
            } else {
                MOVE_FORWARD
            });
        }
        self.turn(key, &ROTATE_TURN);
    }

    /// The first tick of a motion: its clip, from the first frame, blended in.
    fn start_motion(&mut self, bank: Bank, clip: u8) {
        self.work.routine[2] = 1;
        self.work.bank = bank;
        self.work.move_no = clip;
        self.work.move_cnt = 0;
        self.work.hokan = FULL_BLEND;
    }

    fn life_clip(&self, clips: &[u8; 3]) -> u8 {
        clips[usize::from(self.work.d_life_u)]
    }

    fn pl_mv_idle(&mut self) {
        if self.work.routine[2] == 0 {
            self.start_motion(Bank::Weapon, self.life_clip(&IDLE_CLIPS));
            self.work.timer0 = 0;
        }
        self.joint_move();
    }

    fn pl_mv_forward(&mut self) {
        if self.work.routine[2] == 0 {
            self.start_motion(Bank::Weapon, self.life_clip(&WALK_CLIPS));
        }
        self.spd_get();
        self.joint_move();
        self.footstep(0);
        self.add_speed_xz();
    }

    fn pl_mv_run(&mut self) {
        if self.work.routine[2] == 0 {
            self.start_motion(Bank::Weapon, self.life_clip(&RUN_CLIPS));
        }
        self.spd_get();
        self.joint_move();
        self.footstep(1);
        if self.work.d_life_u < 2 {
            self.work.spd[0] = self.work.spd[0].wrapping_sub(RUN_SLOWDOWN);
        }
        self.add_speed_xz();
    }

    fn pl_mv_backward(&mut self) {
        if self.work.routine[2] == 0 {
            self.start_motion(Bank::Pld, self.life_clip(&BACK_CLIPS));
        }
        self.spd_get();
        self.joint_move();
        self.footstep(0);
        self.add_speed_xz();
    }

    fn pl_mv_rotate(&mut self) {
        if self.work.routine[2] == 0 {
            self.start_motion(Bank::Weapon, self.life_clip(&ROTATE_CLIPS));
            self.work.spd = [0; 3];
        }
        self.joint_move();
        self.footstep(0);
    }

    fn mv_quick_turn(&mut self) {
        if self.work.routine[2] == 0 {
            self.start_motion(Bank::Weapon, WALK_CLIPS[0]);
            self.work.timer0 = 0;
        }
        if self.work.timer0 < 8 {
            self.work.timer0 += 1;
            self.player.angle = self.player.angle.wrapping_add(0x100) & 0x0FFF;
        } else {
            self.set_routine(MOVE_IDLE);
        }
        self.joint_move();
        self.footstep(0);
    }

    /// `Spd_get`: this tick's speed, the root motion since the previous frame.
    fn spd_get(&mut self) {
        let reverse = self.work.sce_flg & 0x80 != 0 && self.work.routine[0] == MODE_SCENARIO;
        let (clips, keyframes) = self.tick.banks.get(self.work.bank);
        let Some(clip) = clips.get(usize::from(self.work.move_no)) else {
            return;
        };
        let count = usize::from(self.work.move_cnt);
        let index = if reverse {
            clip.frames.len().wrapping_sub(count + 1)
        } else {
            count
        };
        let vertex = |frame: Option<&ClipFrame>| {
            frame
                .and_then(|frame| keyframes.get(usize::from(frame.keyframe)))
                .map(|keyframe| keyframe.vertex)
        };
        let Some(current) = vertex(clip.frames.get(index)) else {
            return;
        };
        if count == 0 {
            self.work.spd_base = [0; 3];
            if reverse && let Some(first) = vertex(clip.frames.first()) {
                for axis in 0..3 {
                    self.work.spd_base[axis] = first[axis].wrapping_add(current[axis]);
                }
            }
        }
        let base = self.work.spd_base;
        self.work.spd = std::array::from_fn(|axis| current[axis].wrapping_sub(base[axis]));
        self.work.spd_base = current;
        // The original packs the base's x and y in one word, and x's sign
        // extension spills over y: a backward step gets a base y of -1.
        self.work.spd_base[1] |= current[0] >> 15;
    }

    /// `Pl_damage`: the hit reactions a `Sce_Damage` or an enemy starts.
    /// `Routine_1` 4 and 5, the falls that read `pSub1`, play as the plain hit
    /// (`docs/re2-deviations.md`).
    fn pl_damage(&mut self) {
        match self.work.routine[1] {
            2 | 3 => {
                let back = self.work.routine[1] & 1;
                let speed = [
                    if back == 0 {
                        -STAGGER_SPEED
                    } else {
                        STAGGER_SPEED
                    },
                    // The original writes the x speed as a 32-bit word.
                    i16::from(back) - 1,
                    0,
                ];
                if !self.damage_step(STAGGER_CLIPS[usize::from(back)], speed) {
                    return;
                }
                if self.work.hokan != 0 {
                    self.add_speed_xz();
                }
                self.damage_joint(DAMAGE_BLEND_STEP);
            }
            6 => {
                if self.work.routine[2] == 0 {
                    self.work.routine[2] = 1;
                    self.work.bank = Bank::Pld;
                    self.work.move_no = FALL_CLIP;
                    self.work.move_cnt = 0;
                    self.work.hokan = FULL_BLEND;
                    self.work.damage_cnt |= DAMAGE_LOCK;
                    self.work.spd = [0; 3];
                } else if self.work.routine[2] != 1 {
                    if self.work.routine[2] == 2 {
                        self.work.routine[0] = 7;
                    }
                    return;
                }
                self.damage_joint(BLEND_STEP);
            }
            _ => {
                if self.damage_step(DAMAGE_CLIP, [0; 3]) {
                    self.damage_joint(DAMAGE_BLEND_STEP);
                }
            }
        }
    }

    /// The opening of `Pl_damage`'s plain cases: start `clip`, or at the end
    /// of it hand back to the pad. False when no frame plays this tick.
    fn damage_step(&mut self, clip: u8, speed: [i16; 3]) -> bool {
        match self.work.routine[2] {
            0 => {
                self.work.routine[2] = 1;
                self.work.bank = Bank::Pld;
                self.work.move_no = clip;
                self.work.move_cnt = 0;
                self.work.hokan = DAMAGE_BLEND;
                self.work.spd = speed;
                self.work.damage_cnt |= DAMAGE_LOCK;
                true
            }
            1 => true,
            2 => {
                self.work.routine = [MODE_MOVE, MOVE_IDLE, 0, 0];
                self.work.status_flg |= STATUS_RECOVERED;
                self.work.damage_cnt = 0;
                false
            }
            _ => false,
        }
    }

    /// `Routine_2` advances when a damage clip ends.
    fn damage_joint(&mut self, late: u16) {
        if self.joint_move_late(late) {
            self.work.routine[2] += 1;
        }
    }

    /// `Joint_move` and `J_seq`: play one frame. True when the clip ended.
    fn joint_move(&mut self) -> bool {
        self.joint_move_late(BLEND_STEP)
    }

    fn joint_move_late(&mut self, late: u16) -> bool {
        let (clips, _) = self.tick.banks.get(self.work.bank);
        let Some(clip) = clips.get(usize::from(self.work.move_no)) else {
            return true;
        };
        if clip.frames.is_empty() {
            return true;
        }
        let reverse = self.work.routine[0] == MODE_SCENARIO && self.work.sce_flg & 0x80 != 0;
        let count = usize::from(self.work.move_cnt) % clip.frames.len();
        let index = if reverse {
            clip.frames.len() - 1 - count
        } else {
            count
        };
        self.work.now_seq = clip.frames[index].flags;
        self.player.clip_source = match self.work.bank {
            Bank::Pld => ClipSource::Emd,
            Bank::Weapon => ClipSource::Emw,
            Bank::Room => ClipSource::Room,
        };
        let anim = &mut self.player.anim;
        anim.clip = usize::from(self.work.move_no);
        anim.frame = count;
        anim.blend_counter = u16::from(self.work.hokan);
        anim.blend_step = late;
        anim.reverse = reverse;
        anim.timing = 0;
        let ended = anim.update(clips);
        self.work.move_cnt = anim.frame as u8;
        self.work.hokan = anim.blend_counter as u8;
        ended
    }

    /// `Snd_se_walk` on a frame marked as a footfall.
    fn footstep(&mut self, sound_type: u8) {
        if self.work.now_seq & FRAME_FOOTSTEP != 0 {
            self.player.emit_footstep(self.work.move_cnt, sound_type);
        }
    }

    /// `Add_speedXZ`.
    fn add_speed_xz(&mut self) {
        let (dx, dz) = rotate(
            self.player.angle,
            i32::from(self.work.spd[0]),
            i32::from(self.work.spd[2]),
        );
        self.player.pos[0] += dx;
        self.player.pos[2] += dz;
    }

    /// `Add_speedXYZ` for a body that doesn't lean.
    fn add_speed_xyz(&mut self) {
        self.add_speed_xz();
        self.player.pos[1] += i32::from(self.work.spd[1]);
    }

    /// `Spl_Scenario`: a script's motion. A motion with `Sce_flg` bit `0x10`
    /// plays every other tick, which bit `0x20` counts.
    fn scenario(&mut self) {
        if self.work.sce_flg & 0x30 != 0x30 {
            self.spl_sce();
        }
        self.work.sce_flg ^= 0x20;
    }

    fn spl_sce(&mut self) {
        let id = usize::from(self.tick.id & 0x0F);
        match self.work.routine[1] {
            0x00 => self.spl_sce_mv00(Bank::Room),
            0x01 => self.spl_sce_mv00(Bank::Pld),
            0x02 | 0x03 => self.spl_sce_mv00(Bank::Weapon),
            0x04 | 0x12 => self.spl_walk(SCE_WALK_SPEED[id], 0),
            0x05 => self.spl_walk(SCE_RUN_SPEED[id], 1),
            0x06 => {
                if self.work.routine[2] == 0 {
                    self.start_motion(Bank::Weapon, IDLE_CLIPS[0]);
                }
                self.joint_move();
            }
            0x07 => self.spl_back(SCE_BACK_SPEED[id], 0),
            0x08 => self.spl_back(SCE_BACK_RUN_SPEED[id], 1),
            0x09 => self.spl_sce09(),
            0x0A => self.spl_sce_mv04(Bank::Room),
            0x0B => self.spl_sce_mv04(Bank::Pld),
            0x0C | 0x0D => self.spl_sce_mv04(Bank::Weapon),
            0x15 => self.spl_sce15(),
            _ => {}
        }
    }

    /// `Spl_sce_mv00`: a clip on the spot.
    fn spl_sce_mv00(&mut self, bank: Bank) {
        if self.work.routine[2] == 0 {
            self.work.routine[2] = 1;
            self.work.hokan = if self.work.sce_flg & 0x40 != 0 {
                0
            } else {
                FULL_BLEND
            };
        } else if self.work.routine[2] != 1 {
            return;
        }
        self.work.bank = bank;
        if self.joint_move() {
            self.motion_ended();
            return;
        }
        if self.work.sce_flg & 8 != 0 && self.joint_move() {
            self.motion_ended();
        }
        self.player.angle = turned(self.player.angle, self.work.sce_free[0]);
    }

    /// The end of a script's clip: it holds, or loops with `Sce_flg` bit 4, and
    /// `Sce_free1` turns the player.
    fn motion_ended(&mut self) {
        self.work.routine[2] = if self.work.sce_flg & 4 != 0 { 1 } else { 2 };
        self.player.angle = turned(self.player.angle, self.work.sce_free[1]);
    }

    /// `Spl_sce_mv04`: a clip that moves the player by its own root motion.
    fn spl_sce_mv04(&mut self, bank: Bank) {
        if self.work.routine[2] == 0 {
            self.work.routine[2] = 1;
            self.work.bank = bank;
            self.work.hokan = if self.work.sce_flg & 0x40 != 0 {
                0
            } else {
                FULL_BLEND
            };
            if self.work.sce_flg & 0x100 != 0 {
                self.spd_get();
            }
        } else if self.work.routine[2] != 1 {
            return;
        }
        self.footstep(0);
        self.spd_get();
        if self.joint_move() {
            self.motion_ended();
            self.add_speed_xyz();
            return;
        }
        self.add_speed_xyz();
        if self.work.sce_flg & 8 != 0 {
            if self.joint_move() {
                self.motion_ended();
            }
            self.add_speed_xyz();
        }
        self.player.angle = turned(self.player.angle, self.work.sce_free[0]);
    }

    /// `Spl_sce04`, `05` and `12`: turn to face the destination on the walk's
    /// clip, then go there on `clip`, and `Flg_on` at the end.
    fn spl_walk(&mut self, speed: i16, clip: u8) {
        if self.work.routine[2] == 2 {
            if self.spl_sce_mv02() {
                self.arrive();
            }
            return;
        }
        if self.work.routine[2] == 0 {
            self.work.spd[0] = speed;
            self.work.spd[2] = 0;
            self.start_motion(Bank::Weapon, 0);
        } else if self.work.routine[2] != 1 {
            return;
        }
        if self.spl_sce_mv01() {
            self.work.routine[2] = 2;
            if clip != 0 {
                self.work.move_no = clip;
                self.work.move_cnt = 0;
                self.work.hokan = FULL_BLEND;
            }
        }
    }

    /// `Spl_sce07` and `08`: a scripted walk backwards.
    fn spl_back(&mut self, speed: i16, clip: u8) {
        if self.work.routine[2] == 0 {
            self.work.spd[0] = speed.wrapping_neg();
            self.work.spd[2] = 0;
            self.start_motion(Bank::Pld, clip);
        } else if self.work.routine[2] != 1 {
            return;
        }
        if self.spl_sce_mv02() {
            self.arrive();
        }
    }

    fn spl_sce09(&mut self) {
        if self.work.routine[2] == 0 {
            self.start_motion(Bank::Weapon, 0);
        } else if self.work.routine[2] != 1 {
            return;
        }
        if self.spl_sce_mv01() {
            self.arrive();
        }
    }

    fn spl_sce15(&mut self) {
        if self.work.routine[2] == 0 {
            self.start_motion(Bank::Weapon, 0);
        } else if self.work.routine[2] != 1 {
            return;
        }
        let target = self.work.dest[0] as u16;
        let error = (target.wrapping_sub(self.player.angle) as i32 + TURN_TOLERANCE) & 0x0FFF;
        if error < TURN_TOLERANCE * 2 {
            self.arrive();
            return;
        }
        self.player.angle = goto(self.player.angle, target, i32::from(self.work.dest[1]));
        self.footstep(0);
        self.joint_move();
    }

    /// `Flg_on`, and the end of the routine: `Sce_flg` bit 4 keeps its last
    /// state running, otherwise the player stands (`Spl_sce06`).
    fn arrive(&mut self) {
        self.arrived = Some(self.work.sce_flg_no);
        if self.work.sce_flg & 4 != 0 {
            self.work.routine[2] = 2;
        } else {
            self.work.routine[1] = 6;
            self.work.routine[2] = 0;
        }
    }

    fn dest_position(&self) -> (i32, i32) {
        (i32::from(self.work.dest[0]), i32::from(self.work.dest[1]))
    }

    /// `Spl_sce_mv01`: turn to face the destination. True once it does.
    fn spl_sce_mv01(&mut self) -> bool {
        let (x, z) = self.dest_position();
        let target = direction_ck(self.player.pos[0], self.player.pos[2], x, z);
        let error = (i32::from(target.wrapping_sub(self.player.angle)) + FACING_TOLERANCE) & 0x0FFF;
        if error < FACING_TOLERANCE * 2 {
            return true;
        }
        self.player.angle = goto(self.player.angle, target, i32::from(self.work.sce_free[0]));
        self.footstep(0);
        self.joint_move();
        false
    }

    /// `Spl_sce_mv02`: walk at the set speed, steering to the destination.
    /// True on arriving.
    fn spl_sce_mv02(&mut self) -> bool {
        let (x, z) = self.dest_position();
        let target = direction_ck(self.player.pos[0], self.player.pos[2], x, z);
        let sign = if self.work.spd[0] < 0 { -1 } else { 1 };
        self.player.angle = goto(
            self.player.angle,
            target,
            sign * i32::from(self.work.sce_free[0]),
        );
        self.add_speed_xz();
        let run = self.work.routine[1] == 5;
        self.footstep(u8::from(run));
        self.joint_move();
        let dx = i64::from(self.player.pos[0] - x);
        let dz = i64::from(self.player.pos[2] - z);
        dx * dx + dz * dz < ARRIVE_DISTANCE * ARRIVE_DISTANCE
    }
}

fn turned(angle: u16, by: i16) -> u16 {
    angle.wrapping_add(by as u16) & 0x0FFF
}

/// `Sce_Damage`: the player stands in a damage area whose box is
/// `[x, z, width, depth]`, with its `kind` and `damage` operands. Dying leaves
/// the player in `Pl_dead`; otherwise it's pushed away from the box's centre
/// and `Pl_damage` starts.
pub fn sce_damage(
    work: &mut Work,
    player: &mut PlayerState,
    zone: [i16; 4],
    kind: u8,
    damage: i16,
    life: &mut i16,
) {
    if work.damage_cnt & DAMAGE_LOCK != 0 {
        return;
    }
    if work.status_flg & STATUS_RECOVERED != 0 {
        work.status_flg &= !STATUS_RECOVERED;
        return;
    }
    work.damage_cnt |= DAMAGE_LOCK;
    *life = life.wrapping_sub(damage);
    if *life < 0 {
        if work.routine[0] != MODE_DEAD {
            work.routine = [MODE_DEAD, 0, 0, 0];
        }
        return;
    }
    work.routine = [MODE_DAMAGE, kind, 0, 0];
    let centre_x = i32::from((zone[2] as u16) >> 1) + i32::from(zone[0]);
    let centre_z = i32::from((zone[3] as u16) >> 1) + i32::from(zone[1]);
    let away = direction_ck(centre_x, centre_z, player.pos[0], player.pos[2]);
    work.spd[0] = STAGGER_SPEED;
    let (dx, dz) = rotate(away, i32::from(work.spd[0]), i32::from(work.spd[2]));
    player.pos[0] += dx;
    player.pos[2] += dz;
}

/// `rsin`: this executable's sine table, `4096 * sin` rounded to the nearest
/// unit. It reaches a full 4096, which RE1's table doesn't.
fn rsin(angle: i32) -> i32 {
    let step = angle & 0x7FF;
    let step = if step < 0x400 { step } else { 0x800 - step };
    let value = (f64::from(step) * std::f64::consts::PI / 2048.0).sin() * 4096.0;
    let value = value.round() as i32;
    if angle & 0x800 != 0 { -value } else { value }
}

/// `rcos`.
fn rcos(angle: i32) -> i32 {
    rsin(angle + 0x400)
}

/// `RotMatrix(0, angle, 0)` then `ApplyMatrixSV`: its products floor rather
/// than truncate, and the result is narrowed to a `short`.
pub(crate) fn rotate(angle: u16, x: i32, z: i32) -> (i32, i32) {
    let (cos, sin) = (rcos(i32::from(angle)), rsin(i32::from(angle)));
    (
        i32::from(((cos * x + sin * z) >> 12) as i16),
        i32::from(((cos * z - sin * x) >> 12) as i16),
    )
}

/// `catan`, on the original's x87: `atan(v / 4096) * 2048 / 3.14`, truncated.
/// The divisor is the executable's own constant, not a full-precision pi.
fn catan(value: i32) -> i32 {
    const ONE_OVER_PI: f64 = 0.318_471_337_579_617_8;
    ((f64::from(value) * 0.000_244_140_625).atan() * 2048.0 * ONE_OVER_PI) as i32
}

/// `Direction_ck`: the 12-bit yaw from one point to another, as `i16`s.
fn direction_ck(from_x: i32, from_z: i32, to_x: i32, to_z: i32) -> u16 {
    let dx = i32::from(to_x as i16) - i32::from(from_x as i16);
    let dz = i32::from(to_z as i16) - i32::from(from_z as i16);
    if dx == 0 {
        return if dz > 0 { 0xC00 } else { 0x400 };
    }
    let half_turn = if dx < 0 { 0x800 } else { 0 };
    ((-(catan(dz.wrapping_mul(0x1000) / dx) + half_turn)) & 0x0FFF) as u16
}

/// The turn in `Goto00` and `Goto01`: step `rate` toward `target`, landing on
/// it when close. A negative rate faces away from it.
fn goto(angle: u16, target: u16, rate: i32) -> u16 {
    let (target, rate) = if rate < 0 {
        (target.wrapping_sub(0x800) & 0x0FFF, -rate)
    } else {
        (target, rate)
    };
    let error = (i32::from(target) - i32::from(angle) + rate) & 0x0FFF;
    if error < rate * 2 {
        return target;
    }
    let stepped = i32::from(angle) - rate;
    let stepped = if error <= 0x800 {
        stepped + rate * 2
    } else {
        stepped
    };
    (stepped & 0x0FFF) as u16
}

/// `Plc_dest`: start routine `routine`, walking to `(x, z)`, raising room flag
/// `flag` there. The turn rate depends on the routine and the character.
pub fn plc_dest(work: &mut Work, id: u8, routine: u8, flag: u8, x: i16, z: i16) {
    if work.sce_flg & 4 == 0 || work.routine[1] != routine {
        work.routine = [MODE_SCENARIO, routine, 0, 0];
    }
    match routine {
        0x04 | 0x12 => work.sce_free[0] = DEST_WALK_TURN,
        0x05 => work.sce_free[0] = DEST_RUN_TURN,
        0x07 | 0x08 => work.sce_free[0] = DEST_BACK_TURN,
        0x09 => work.sce_free[0] = DEST_ROTATE_TURN[usize::from(id & 0x0F)],
        _ => {}
    }
    work.sce_flg_no = flag;
    work.sce_flg = 0;
    work.sce_free[1] = 0;
    work.dest = [x, z];
}

/// `Plc_motion`: start clip `clip` of routine `routine`'s bank.
pub fn plc_motion(work: &mut Work, routine: u8, clip: u8, flags: u8) {
    work.routine = [MODE_SCENARIO, routine, 0, 0];
    work.move_no = clip;
    work.move_cnt = 0;
    work.sce_flg = u16::from(flags);
    work.sce_free = [0; 2];
}

/// `Plc_ret`: back to the pad.
pub fn plc_ret(work: &mut Work) {
    work.routine = [MODE_MOVE, MOVE_IDLE, 0, 0];
}

/// `Plc_stop`: hold the routine, or give it back.
pub fn plc_stop(work: &mut Work) {
    if work.routine == STOPPED_ROUTINE {
        work.routine = work.stopped;
    } else {
        work.stopped = work.routine;
        work.routine = STOPPED_ROUTINE;
    }
}

/// `Load_member_branch` for the fields that live in [`Work`].
pub fn member(work: &Work, member: u8) -> Option<i32> {
    Some(match member {
        2..=5 => i32::from(work.routine[usize::from(member) - 2]),
        7 => i32::from(work.kind),
        0x14 | 0x19 => i32::from(work.dest[0]),
        0x15 | 0x1A => i32::from(work.dest[1]),
        0x16 => i32::from(work.sce_flg),
        0x17 => i32::from(work.sce_free[0]),
        0x18 => i32::from(work.sce_free[1]),
        0x1B => i32::from(work.spd[0] as u8),
        0x1C => i32::from(work.spd[0]),
        0x1D => i32::from(work.spd[1]),
        0x1E => i32::from(work.spd[2]),
        0x1F => i32::from(work.hokan),
        _ => return None,
    })
}

/// `Member_set_branch` for the fields that live in [`Work`]. False for another.
pub fn set_member(work: &mut Work, member: u8, value: i32) -> bool {
    match member {
        2..=5 => work.routine[usize::from(member) - 2] = value as u8,
        7 => work.kind = value as u16,
        0x14 | 0x19 => work.dest[0] = value as i16,
        0x15 | 0x1A => work.dest[1] = value as i16,
        0x16 => work.sce_flg = value as u16,
        0x17 => work.sce_free[0] = value as i16,
        0x18 => work.sce_free[1] = value as i16,
        0x1B => work.spd[0] = (work.spd[0] & !0xFF) | i16::from(value as u8),
        0x1C => work.spd[0] = value as i16,
        0x1D => work.spd[1] = value as i16,
        0x1E => work.spd[2] = value as i16,
        0x1F => work.hokan = value as u8,
        _ => return false,
    }
    true
}

/// `Type`'s bit that keeps `Game_loop` from calling `Player_move` at all.
pub const KIND_NOT_MOVED: u16 = 0x8000;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Clip, ClipFrame, Keyframe};
    use crate::state::RoomId;

    /// Clips whose frames each move `step` along the root.
    fn bank(clips: usize, frames: usize, step: i16) -> (Vec<Clip>, Vec<Keyframe>) {
        let keyframes = (0..frames)
            .map(|index| Keyframe {
                offset: [0; 3],
                vertex: [step * (index as i16 + 1), 0, 0],
                rotations: Vec::new(),
            })
            .collect();
        let clip = Clip {
            frames: (0..frames)
                .map(|index| ClipFrame {
                    keyframe: index as u16,
                    timing: 0,
                    flags: 0,
                })
                .collect(),
        };
        (vec![clip; clips], keyframes)
    }

    struct Setup {
        work: Work,
        player: PlayerState,
        room: RoomState,
        clips: Vec<Clip>,
        keyframes: Vec<Keyframe>,
    }

    impl Setup {
        fn new(step: i16) -> Self {
            let room = RoomState::default();
            let (clips, keyframes) = bank(10, 8, step);
            Self {
                work: Work::default(),
                player: player::spawn(RoomId::parse("1000").unwrap(), &room),
                room,
                clips,
                keyframes,
            }
        }

        fn tick(&mut self, input: Input, life: i32) -> Option<u8> {
            let bank = (self.clips.as_slice(), self.keyframes.as_slice());
            let tick = Tick {
                room: &self.room,
                banks: Banks {
                    pld: bank,
                    weapon: bank,
                    room: bank,
                },
                input,
                options: Options::default(),
                id: 0,
                life,
                poisoned: false,
                stopped: false,
            };
            player_move(&mut self.work, &mut self.player, &tick)
        }
    }

    fn input(up: bool, right: bool, run: bool) -> Input {
        Input {
            up,
            right,
            run,
            ..Input::default()
        }
    }

    #[test]
    fn a_walk_starts_on_the_tick_of_the_press_and_moves_by_root_motion() {
        let mut setup = Setup::new(50);
        setup.player.angle = 0;
        setup.tick(input(true, false, false), 200);
        assert_eq!(setup.work.routine[..2], [MODE_MOVE, MOVE_FORWARD]);
        assert_eq!(setup.work.move_no, WALK_CLIPS[0]);
        assert_eq!(setup.work.move_cnt, 1);
        assert_eq!(setup.work.hokan, FULL_BLEND - 1);
        assert_eq!(setup.work.spd[0], 50);
        assert_eq!(setup.player.pos[0], 50);
        setup.tick(input(true, false, false), 200);
        assert_eq!(setup.player.pos[0], 100);
    }

    #[test]
    fn rotation_uses_the_executables_full_scale_table_and_floors() {
        assert_eq!(rsin(0), 0);
        assert_eq!(rsin(0x400), 4096);
        assert_eq!(rcos(0x800), -4096);
        assert_eq!(rsin(1), 6);
        assert_eq!(rotate(0x800, 82, 0), (-82, 0));
        // -82 * 4095 / 4096 floors away from zero.
        assert_eq!(rotate(0x20, 0, -82).1, -82);
    }

    #[test]
    fn a_backward_steps_negative_x_spills_into_the_speed_base() {
        let mut setup = Setup::new(-50);
        setup.player.angle = 0;
        setup.work.routine[2] = 0;
        setup.tick(input(true, false, false), 200);
        setup.tick(input(true, false, false), 200);
        assert_eq!(setup.work.spd[..2], [-50, 1]);
    }

    #[test]
    fn a_damage_area_knocks_the_player_away_and_plays_the_hit_once() {
        let mut setup = Setup::new(0);
        setup.player.pos = [0, 0, 0];
        let zone = [-200, -100, 200, 200];
        let mut life = 200;
        let hit = |setup: &mut Setup, life: &mut i16| {
            sce_damage(&mut setup.work, &mut setup.player, zone, 0, 10, life);
        };
        hit(&mut setup, &mut life);
        assert_eq!(setup.work.routine, [MODE_DAMAGE, 0, 0, 0]);
        assert_eq!(setup.player.pos, [200, 0, 0]);
        assert_eq!(life, 190);
        // It can't hurt again while the clip plays.
        hit(&mut setup, &mut life);
        assert_eq!(life, 190);
        for _ in 0..8 {
            setup.tick(Input::default(), 190);
        }
        assert_eq!(setup.work.move_no, DAMAGE_CLIP);
        assert_eq!(setup.work.routine[..3], [MODE_DAMAGE, 0, 2]);
        setup.tick(Input::default(), 190);
        assert_eq!(setup.work.routine[..2], [MODE_MOVE, MOVE_IDLE]);
        // The first hit after recovering is forgiven, the next isn't.
        hit(&mut setup, &mut life);
        assert_eq!((setup.work.routine[0], life), (MODE_MOVE, 190));
        hit(&mut setup, &mut life);
        assert_eq!((setup.work.routine[0], life), (MODE_DAMAGE, 180));
    }

    #[test]
    fn a_lethal_damage_area_leaves_the_player_dead_where_it_stands() {
        let mut setup = Setup::new(0);
        setup.player.pos = [0, 0, 0];
        let mut life = 5;
        sce_damage(
            &mut setup.work,
            &mut setup.player,
            [-200, -100, 200, 200],
            0,
            10,
            &mut life,
        );
        assert_eq!(setup.work.routine, [MODE_DEAD, 0, 0, 0]);
        assert_eq!(setup.player.pos, [0, 0, 0]);
    }

    #[test]
    fn hurt_clips_follow_health() {
        let mut setup = Setup::new(50);
        setup.work.routine[2] = 0;
        setup.tick(input(true, false, false), 100);
        assert_eq!(setup.work.d_life_u, 1);
        assert_eq!(setup.work.move_no, WALK_CLIPS[1]);
        let mut setup = Setup::new(50);
        setup.work.routine[2] = 0;
        setup.tick(input(true, false, false), 20);
        assert_eq!(setup.work.d_life_u, 2);
        assert_eq!(setup.work.move_no, WALK_CLIPS[2]);
    }

    #[test]
    fn a_run_is_slower_than_its_root_motion_and_only_turns_at_speed() {
        let mut setup = Setup::new(200);
        setup.player.angle = 0;
        setup.tick(input(true, false, true), 200);
        assert_eq!(setup.work.routine[1], MOVE_RUN);
        assert_eq!(setup.work.spd[0], 200 - RUN_SLOWDOWN);
        setup.tick(input(true, true, true), 200);
        assert_eq!(setup.player.angle, RUN_TURN[0]);
        let mut slow = Setup::new(10);
        slow.player.angle = 0;
        slow.tick(input(true, false, true), 200);
        slow.tick(input(true, true, true), 200);
        assert_eq!(slow.player.angle, 0);
    }

    #[test]
    fn a_stopped_player_keeps_its_frame() {
        let mut setup = Setup::new(50);
        let before = setup.work.clone();
        let bank = (setup.clips.as_slice(), setup.keyframes.as_slice());
        let tick = Tick {
            room: &setup.room,
            banks: Banks {
                pld: bank,
                weapon: bank,
                room: bank,
            },
            input: input(true, false, false),
            options: Options::default(),
            id: 0,
            life: 200,
            poisoned: false,
            stopped: true,
        };
        player_move(&mut setup.work, &mut setup.player, &tick);
        assert_eq!(setup.work, before);
    }

    #[test]
    fn a_scripted_walk_faces_the_destination_then_arrives_and_raises_its_flag() {
        let mut setup = Setup::new(50);
        setup.player.pos = [0, 0, 0];
        setup.player.angle = 0x800;
        plc_dest(&mut setup.work, 0, 4, 7, 1000, 0);
        let mut raised = None;
        for _ in 0..200 {
            raised = raised.or(setup.tick(Input::default(), 200));
        }
        assert_eq!(raised, Some(7));
        assert_eq!(setup.work.routine[1], 6, "stands at the end");
        let (dx, dz) = (setup.player.pos[0] - 1000, setup.player.pos[2]);
        assert!(dx * dx + dz * dz < 100 * 100, "{:?}", setup.player.pos);
    }

    #[test]
    fn plc_ret_hands_the_player_back() {
        let mut work = Work::default();
        plc_motion(&mut work, 2, 3, 0);
        assert_eq!(work.routine[..2], [MODE_SCENARIO, 2]);
        assert_eq!(work.move_no, 3);
        plc_ret(&mut work);
        assert_eq!(work.routine, [MODE_MOVE, MOVE_IDLE, 0, 0]);
    }

    #[test]
    fn plc_stop_toggles_and_restores_the_routine() {
        let mut work = Work::default();
        plc_motion(&mut work, 2, 3, 0);
        let running = work.routine;
        plc_stop(&mut work);
        assert_eq!(work.routine, STOPPED_ROUTINE);
        plc_stop(&mut work);
        assert_eq!(work.routine, running);
    }

    #[test]
    fn direction_matches_the_engine_axes() {
        assert_eq!(direction_ck(0, 0, 100, 0), 0);
        assert_eq!(direction_ck(0, 0, -100, 0), 0x800);
        assert_eq!(direction_ck(0, 0, 0, 100), 0xC00);
        assert_eq!(direction_ck(0, 0, 0, -100), 0x400);
    }

    #[test]
    fn goto_steps_the_short_way_and_lands_when_close() {
        assert_eq!(goto(0, 0x100, 0x30), 0x30);
        assert_eq!(goto(0x100, 0, 0x30), 0xD0);
        assert_eq!(goto(0xFF0, 0x10, 0x30), 0x10);
    }
}
