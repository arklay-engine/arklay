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

use crate::model::{Clip, ClipFrame, Keyframe, Skeleton};
use crate::player::{self, ClipSource, Input, Options, PlayerState};
use crate::state::{RoomState, ScaRect};

/// `Key`, the mapped pad word `pl_br_*` test.
const KEY_FORWARD: u16 = 0x0001;
const KEY_RIGHT: u16 = 0x0002;
const KEY_BACKWARD: u16 = 0x0004;
const KEY_LEFT: u16 = 0x0008;
const KEY_TURN: u16 = KEY_RIGHT | KEY_LEFT;
const KEY_RUN: u16 = 0x0200;
/// The aim tilts: the up and down pad bits have a `Key` bit of their own too.
const KEY_TILT_UP: u16 = 0x0010;
const KEY_TILT_DOWN: u16 = 0x0020;
const KEY_FIRE: u16 = 0x0040;
const KEY_ACTION: u16 = 0x0080;
const KEY_AIM: u16 = 0x0100;
const KEY_SIDE_LEFT: u16 = 0x0400;
const KEY_SIDE_RIGHT: u16 = 0x0800;
const KEY_CONFIRM: u16 = 0x1000;
const KEY_CANCEL: u16 = 0x2000;
/// What `Pad_set` leaves of the word while the input is stopped.
pub const KEYS_IN_A_STOP: u16 = KEY_SIDE_LEFT | KEY_SIDE_RIGHT | KEY_CONFIRM | KEY_CANCEL;

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
const MOVE_AIM: u8 = 5;
const MOVE_STAIRS: u8 = 7;
const MOVE_CLIMB: u8 = 8;
const MOVE_JUMP_DOWN: u8 = 9;
/// The port's optional quick turn, past the original's table.
const MOVE_QUICK_TURN: u8 = 0x40;
/// `Plc_stop`'s routine, which no table entry animates.
/// `Type`'s bit that gives a character to `Spl_Auto`, not to the scripts.
const SPL_AUTO: u16 = 0x400;
/// `Spl_Init`'s `Joint_move` late step.
const SPL_INIT_STEP: u16 = 0x100;
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
/// The first of `pRoom`'s climbing clips, going up and down: a first step,
/// the step repeated, and the last.
const STAIRS_CLIPS: [u8; 2] = [0, 3];
const STAIRS_TURN: i32 = 0x50;
const STAIRS_TURN_CLOSE: i32 = 0x28;
/// The walk off the stairs lasts until this frame.
const STAIRS_WALK_OFF_FRAME: u8 = 9;
const STAIRS_FIRST_BLEND: u8 = 3;
const STAIRS_FIRST_STEP: u16 = 0x400;
/// `Oma_pl_updown_ck`'s reach to a box to climb and to a drop, by `Id + 4`
/// (`0x533370`, `0x533398`).
const CLIMB_REACH: [i32; 20] = [
    884, 1251, 581, 971, 800, 800, 800, 800, 800, 800, 800, 800, 800, 800, 800, 800, 800, 800, 800,
    600,
];
const DROP_REACH: [i32; 20] = [
    884, 653, 854, 581, 1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000,
    1000, 1000, 1000, 600,
];
/// `Routine_2`'s bit for a jump down, in `pl_mv_08`'s turn to the edge.
const CLIMB_IS_DROP: u8 = 2;
/// The room bank's clips: the climb, the jump down and its fall and landing.
const CLIMB_CLIP: u8 = 6;
const JUMP_CLIP: u8 = 7;
const JUMP_FALL_CLIP: u8 = 8;
const LAND_CLIP: u8 = 9;
/// The small character (id 0xF) has its own timing, on the same clips.
const SMALL_ID: u8 = 0xF;
/// `pl_mv05_w1`'s clips: raising; holding, then firing, at each tilt.
const AIM_RAISE_CLIP: u8 = 9;
const AIM_HOLD_CLIPS: [u8; 3] = [0x0F, 0x0B, 0x0D];
const AIM_FIRE_CLIPS: [u8; 3] = [0x0E, 0x0A, 0x0C];
const AIM_RELOAD_CLIP: u8 = 0x10;
/// `Status_flg`'s tilt.
const TILT_DOWN: u16 = 0x2000;
const TILT_LEVEL: u16 = 0x4000;
const TILT_UP: u16 = 0x8000;
/// `Cdir_y` per tick in each part of the stance.
const AIM_RAISE_TURN: u16 = 0x20;
const AIM_HOLD_TURN: u16 = 0x40;
const AIM_FIRE_TURN: u16 = 8;
/// The handguns' row of the table at `0x53A305`: the frame from which the
/// raise can tilt, and from which a released shot is cut short.
const AIM_TILT_DOWN_FRAME: u8 = 5;
const AIM_TILT_UP_FRAME: u8 = 7;
const AIM_FIRE_CANCEL_FRAME: u8 = 8;
const AIM_LOWER_BLEND: u8 = 0x0F;
/// The parts from the root to the foot the handguns' stance plants.
const AIM_FOOT_CHAIN: [usize; 5] = [0, 1, 5, 6, 7];
const AIM_LOWER_STEP: u16 = 0x100;
/// The weapon bank's slot of a shot (`Snd_se_on(0x1010001)`).
const SOUND_SHOT: u16 = 1;
/// The frame flag `pl_mv05_w1_t4` plays a sound of the weapon bank on; the
/// slot is the frame's [`FRAME_SOUND_SLOT`] bits.
const FRAME_SOUND: u32 = 0x10_0000;
const FRAME_SOUND_SLOT: u32 = 0xf_0000;
/// The frame flag `pl_mv05_w1_t4` fills the magazine on.
const FRAME_RELOAD: u32 = 0x40_0000;
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
pub const FRAME_FOOTSTEP: u32 = 0x4000;
pub const FRAME_FOOTSTEP_ALT: u32 = 0x2000;

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
// The same tables' other half, for the characters from id 0x40 up.
/// `Spl_sce04`, `12`.
const SPL_WALK_SPEED: [i16; 32] = [
    0x14, 0x46, 0x14, 0x46, 0x3C, 0x32, 0x3C, 0x41, 0x3C, 0x41, 0x1E, 0x00, 0x00, 0x00, 0x00, 0x32,
    0x4B, 0x46, 0x4B, 0x46, 0x4B, 0x46, 0x4B, 0x46, 0x4B, 0x46, 0x4B, 0x4B, 0x4B, 0x4B, 0x46, 0x32,
];
/// `Spl_sce05`.
const SPL_RUN_SPEED: [i16; 32] = [
    0x1E, 0xDC, 0x1E, 0xDC, 0xBE, 0x78, 0xBE, 0xAA, 0xA0, 0xAA, 0x1E, 0x00, 0x00, 0x00, 0x00, 0x78,
    0xC8, 0xD2, 0xC8, 0xD2, 0xC8, 0xD2, 0xC8, 0xD2, 0xC8, 0xD2, 0xC8, 0xC8, 0xC8, 0xC8, 0xDC, 0x78,
];
/// `Spl_sce07`.
const SPL_BACK_SPEED: [i16; 32] = [
    0x19, 0x37, 0x19, 0x37, 0x37, 0x23, 0x37, 0x37, 0x37, 0x37, 0x19, 0x00, 0x00, 0x00, 0x00, 0x23,
    0x46, 0x41, 0x46, 0x41, 0x46, 0x41, 0x46, 0x41, 0x46, 0x41, 0x46, 0x46, 0x46, 0x46, 0x37, 0x23,
];
/// `Spl_sce08`.
const SPL_BACK_RUN_SPEED: [i16; 32] = [
    0x0A, 0x2D, 0x0A, 0x2D, 0x28, 0x1E, 0x28, 0x28, 0x28, 0x28, 0x0A, 0x00, 0x00, 0x00, 0x00, 0x1E,
    0x28, 0x2D, 0x28, 0x2D, 0x28, 0x2D, 0x28, 0x2D, 0x28, 0x2D, 0x28, 0x28, 0x28, 0x28, 0x2D, 0x1E,
];
/// `Plc_dest`'s routines 4 and 0x12.
const SPL_DEST_WALK_TURN: [i16; 32] = [
    0x10, 0x30, 0x10, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x10, 0x00, 0x00, 0x00, 0x00, 0x30,
    0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30,
];
/// `Plc_dest`'s routine 5.
const SPL_DEST_RUN_TURN: [i16; 32] = [
    0x1E, 0x48, 0x1E, 0x48, 0x48, 0x40, 0x48, 0x48, 0x3C, 0x48, 0x1E, 0x00, 0x00, 0x00, 0x00, 0x40,
    0x48, 0x48, 0x48, 0x48, 0x48, 0x48, 0x48, 0x48, 0x48, 0x48, 0x48, 0x48, 0x48, 0x48, 0x48, 0x40,
];
/// `Plc_dest`'s routine 7.
const SPL_DEST_BACK_TURN: [i16; 32] = [
    0x10, 0x30, 0x10, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x10, 0x00, 0x00, 0x00, 0x00, 0x30,
    0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30,
];
/// `Plc_dest`'s routine 8.
const SPL_DEST_BACK_RUN_TURN: [i16; 32] = [
    0x08, 0x30, 0x08, 0x30, 0x30, 0x28, 0x30, 0x28, 0x28, 0x28, 0x08, 0x00, 0x00, 0x00, 0x00, 0x28,
    0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x28,
];
/// `Plc_dest`'s routine 9.
const SPL_DEST_ROTATE_TURN: [i16; 32] = [
    0x20, 0x58, 0x20, 0x58, 0x58, 0x50, 0x58, 0x58, 0x58, 0x58, 0x20, 0x00, 0x00, 0x00, 0x00, 0x50,
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
    /// `Timer1` and `Timer2`: the climb's steps left and whether it goes down.
    timer1: i16,
    timer2: u8,
    /// The flags of `pNow_seq`, the frame `Joint_move` played last.
    now_seq: u32,
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
    /// A round was spent this tick (`Tama_decrease_i`), for the inventory.
    pub fired: bool,
    /// The magazine was filled this tick (`Tama_reload`), for the inventory.
    pub reloaded: bool,
    /// The head's `Rot_neck` state.
    pub neck: Neck,
    /// The parts whose `Be_flg` `parts_set` cleared, by bit: they aren't drawn.
    pub hidden_parts: u32,
    /// `Parts0_pos_y`: the scale of the first part's height, `0x1000` being 1.
    parts0_y: i32,
}

/// `Spl_Init`'s `Parts0_pos_y`, by id from 0x40.
const CHARACTER_PARTS0_Y: [u16; 28] = [
    0x107B, 0x1052, 0x107B, 0x1052, 0x119A, 0x1000, 0x119A, 0x1000, 0x10F6, 0x1000, 0xF57, 0x1000,
    0, 0, 0, 0x1000, 0x1000, 0x1000, 0x1000, 0x1000, 0x1000, 0x1000, 0x1000, 0x1000, 0x1000,
    0x1000, 0x1000, 0x1000,
];

/// `Spl_Init`'s `Kage_work_set` half sizes for id `id`: odd ids are smaller,
/// and the children 0x45 and 0x4F smaller still.
pub const fn character_shadow(id: u8) -> (i32, i32) {
    match id {
        // `Em10_Init`'s.
        0x10..0x20 => (0x190, 0x258),
        0x45 | 0x4F => (0x172, 0x1CC),
        0x4D => (500, 600),
        id if id & 1 != 0 => (0x1A4, 0x208),
        _ => (500, 600),
    }
}

/// `Neck_no`: the head's joint, the same for every player.
pub const NECK_JOINT: usize = 8;
/// `Pl_init`'s turn rates and limits of [`NECK_JOINT`], yaw then pitch.
const NECK_RATES: [i32; 2] = [0x60, 0x60];
const NECK_LIMITS: [i32; 2] = [0x200, 0x138];
/// `plc_neck`'s `0x8000` in its second word: aim at another work's neck.
pub(crate) const NECK_AT_WORK: i16 = -0x8000;

/// The state `Plc_neck` leaves for `Rot_neck`, and the turn that adds to the
/// head's pose.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Neck {
    /// `Neck_flg`.
    pub flags: u8,
    /// The script's three words, at `+0x170`, and what a sweep keeps in them.
    pub words: [i16; 3],
    /// `plc_neck`'s rates, used while `flags`' top bit is set.
    pub rates: [u8; 2],
    /// The head's yaw and pitch on top of its pose (the joint's `+0x98`).
    pub turn: [i16; 2],
    /// Where the head was last, in the world (the joint's `+0x5C`), for
    /// another's aim at it.
    pub position: Option<[i32; 3]>,
}

impl Work {
    /// What the work adds to a body's `pose`: its height and its head.
    pub fn set_parts0_y(&mut self, scale: i32) {
        self.parts0_y = scale;
    }

    pub fn apply_to_pose(&self, pose: &mut Keyframe) {
        pose.offset[1] = self.scaled_height(pose.offset[1]);
        self.neck.apply(pose);
    }

    fn scaled_height(&self, height: i16) -> i16 {
        ((i32::from(height) * self.parts0_y) >> 12) as i16
    }
}

impl Neck {
    /// Add the turn to `pose`, as `Rot_neck` does around its `RotMatrix`.
    pub fn apply(&self, pose: &mut Keyframe) {
        if let Some(rotation) = pose.rotations.get_mut(NECK_JOINT) {
            rotation[1] = rotation[1].wrapping_add(self.turn[0]);
            rotation[2] = rotation[2].wrapping_add(self.turn[1]);
        }
    }
}

/// `Parts_set`'s case 2 on `work`: `Be_flg`, whose first bit draws the part.
pub fn set_part_flags(work: &mut Work, part: u8, flags: u16) {
    if part < 32 {
        let bit = 1 << part;
        if flags & 1 == 0 {
            work.hidden_parts |= bit;
        } else {
            work.hidden_parts &= !bit;
        }
    }
}

/// `Plc_neck` on `work`: what to do with the head from now on.
pub fn plc_neck(work: &mut Work, mode: u8, words: [i16; 3], rates: [u8; 2]) {
    let neck = &mut work.neck;
    neck.flags = match mode {
        0 => 0x92,
        1 | 5 => 0x84,
        2 => 0x88,
        3 => 0xAA,
        4 => 0xD8,
        6 => 0,
        _ => 0x80,
    };
    neck.words = [words[0], words[1], words[2]];
    if mode == 5 {
        neck.words[0] = match words[0] {
            1 => 0,
            2 => 1,
            _ => words[1].wrapping_add(2),
        };
        neck.words[1] = NECK_AT_WORK;
    }
    neck.rates = rates;
}

/// `Neck_Dir_ZY`: the yaw and pitch from `from` to `to`.
fn neck_direction(from: [i32; 3], to: [i32; 3]) -> [i32; 2] {
    let (x, z) = (to[0] - from[0], to[2] - from[2]);
    let y = from[1] - to[1];
    let level = i64::from(z).pow(2) + i64::from(x).pow(2);
    let level = level.isqrt() as i32;
    let mut direction = [0; 2];
    if z != 0 {
        direction[0] = if z < 0 { 0x400 } else { 0xC00 };
    }
    if x != 0 {
        let half = if x < 0 { 0x800 } else { 0 };
        direction[0] = -(catan(z.wrapping_mul(0x1000) / x) + half) & 0xFFF;
    }
    if y != 0 {
        direction[1] = if y < 0 { 0xC00 } else { 0x1400 };
    }
    if level != 0 {
        direction[1] = -catan(y.wrapping_mul(0x1000) / level) & 0xFFF;
    }
    direction
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
            hidden_parts: 0,
            parts0_y: 0x1000,
            d_life_u: 0,
            spd: [0; 3],
            spd_base: [0; 3],
            timer0: 0,
            timer1: 0,
            timer2: 0,
            now_seq: 0,
            kind: 0,
            sce_flg: 0,
            sce_flg_no: 0,
            sce_free: [0; 2],
            dest: [0; 2],
            stopped: [0; 4],
            damage_cnt: 0,
            status_flg: 0,
            fired: false,
            reloaded: false,
            neck: Neck::default(),
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

    /// `Be_flg`'s bits 4 and 8: on the stairs, or going up or down a box and
    /// off the ground, nothing collides.
    pub fn climbing(&self) -> bool {
        self.routine[0] == MODE_MOVE
            && match self.routine[1] {
                MOVE_STAIRS => true,
                MOVE_CLIMB => matches!(self.routine[3], 1..=3),
                MOVE_JUMP_DOWN => self.routine[3] < 5,
                _ => false,
            }
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
    pub fn get(&self, bank: Bank) -> (&'a [Clip], &'a [Keyframe]) {
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
    pub skeleton: &'a Skeleton,
    pub input: Input,
    pub options: Options,
    /// The character id, `Id`.
    pub id: u8,
    pub life: i32,
    pub poisoned: bool,
    /// `fg_stop`'s top bit, which freezes the player's whole routine.
    pub stopped: bool,
    pub weapon: Weapon,
    /// `player_work`'s table of necks a `plc_neck` can aim at: the player, the
    /// second player, then the enemy slots.
    pub necks: Necks,
}

/// The necks of [`Tick::necks`].
pub type Necks = [Option<[i32; 3]>; NECK_WORKS];
/// `player_work`, `splayer_work` and `enemy_work`'s 16.
pub const NECK_WORKS: usize = 18;

/// What the player's `Type` and the inventory say about the held weapon.
#[derive(Debug, Default, Clone, Copy)]
pub struct Weapon {
    /// `Type & 0xFFF`.
    pub id: u8,
    /// `Tama_ck_i`: there is a round in it.
    pub loaded: bool,
    /// `Tama_reload_ck`: the inventory has its ammunition.
    pub spare: bool,
}

impl Weapon {
    /// Whether [`Mover::pl_mv_aim`] handles it: the starting handguns.
    fn aims(&self) -> bool {
        matches!(self.id, 2 | 3)
    }
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
        stairs: None,
    };
    match mover.work.routine[0] {
        MODE_MOVE => mover.pl_move(),
        MODE_DAMAGE => mover.pl_damage(),
        MODE_SCENARIO => mover.scenario(),
        _ => {}
    }
    mover.rot_neck();
    mover.arrived
}

/// A tick of [`player_move`]: the state the original keeps in globals.
pub(crate) struct Mover<'a, 'b> {
    pub(crate) work: &'a mut Work,
    pub(crate) player: &'a mut PlayerState,
    pub(crate) tick: &'a Tick<'b>,
    arrived: Option<u8>,
    /// `Sca_info`'s stairs: those last tick's end left in front of the player.
    stairs: Option<&'b ScaRect>,
}

/// `Pad_set`'s word, whose bits are each a column of the table it maps the
/// pad with.
pub fn key_word(input: &Input) -> u16 {
    let bit = |set: bool, value: u16| if set { value } else { 0 };
    let action = input.action_held || input.action_pressed;
    bit(input.up, KEY_FORWARD | KEY_TILT_UP)
        | bit(input.right, KEY_RIGHT | KEY_SIDE_RIGHT)
        | bit(input.down, KEY_BACKWARD | KEY_TILT_DOWN)
        | bit(input.left, KEY_LEFT | KEY_SIDE_LEFT)
        | bit(input.run, KEY_RUN | KEY_CANCEL)
        | bit(action || input.fire, KEY_CONFIRM)
        | bit(input.aim, KEY_AIM)
        | bit(input.fire, KEY_FIRE)
        | bit(action, KEY_ACTION)
}

impl<'a, 'b> Mover<'a, 'b> {
    pub(crate) fn new(work: &'a mut Work, player: &'a mut PlayerState, tick: &'a Tick<'b>) -> Self {
        Self {
            work,
            player,
            tick,
            arrived: None,
            stairs: None,
        }
    }
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
        self.stairs = self.tick.room.sca.as_ref().and_then(|sca| {
            crate::sca::stairs_ahead(sca, self.player.pos, self.player.angle, self.player.radius)
        });
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
            MOVE_JUMP_DOWN => self.pl_br_jump_down(key),
            _ => {}
        }
        // `pl_mv_05` reads the keys itself.
        if let Some(target) = target {
            self.player.angle = player::analog_turned(self.player.angle, target);
        }
        match self.work.routine[1] {
            MOVE_IDLE => self.pl_mv_idle(),
            MOVE_FORWARD => self.pl_mv_forward(),
            MOVE_RUN => self.pl_mv_run(),
            MOVE_BACKWARD => self.pl_mv_backward(),
            MOVE_ROTATE => self.pl_mv_rotate(),
            MOVE_AIM => self.pl_mv_aim(key),
            MOVE_STAIRS => self.pl_mv_stairs(),
            MOVE_CLIMB => self.pl_mv_climb(),
            MOVE_JUMP_DOWN => self.pl_mv_jump_down(),
            MOVE_QUICK_TURN => self.mv_quick_turn(),
            _ => {}
        }
    }

    /// The end of the `pl_br_*` that can raise the weapon.
    fn aim_branch(&mut self, key: u16) {
        if key & KEY_AIM != 0 && self.tick.weapon.aims() {
            self.set_routine(MOVE_AIM);
        }
    }

    /// The end of the `pl_br_*` with both: the stairs come first.
    fn end_branch(&mut self, key: u16) {
        if !self.stairs_branch(key) {
            self.aim_branch(key);
        }
    }

    /// The action key at stairs, whether or not `Sca_hit_stairs` takes it.
    fn stairs_branch(&mut self, key: u16) -> bool {
        if key & KEY_ACTION == 0 {
            return false;
        }
        let Some(stairs) = self.stairs else {
            return self.updown_ck();
        };
        self.sca_hit_stairs(stairs);
        true
    }

    /// `Oma_pl_updown_ck`, without the object models: a box to climb in front
    /// of the player, or a drop.
    fn updown_ck(&mut self) -> bool {
        use crate::sca::{FLOOR_HEIGHT, INFO_DOWN, INFO_UP, PLAYER_MASK};
        let Some(sca) = self.tick.room.sca.as_ref() else {
            return false;
        };
        // A quarter turn, the nearest to the player's facing.
        let facing = self.player.angle.wrapping_add(0x200) & 0xFC00;
        let [x, y, z] = self.player.pos;
        let radius = self.player.radius;
        let floor = crate::sca::floor_of(y);
        let reach = |table: &[i32; 20]| table[(usize::from(self.tick.id) + 4) % table.len()];
        let (dx, dz) = rotate(facing, reach(&CLIMB_REACH), 0);
        let up = [x + dx, y - FLOOR_HEIGHT, z + dz];
        if floor < 31 && crate::sca::info_at(sca, PLAYER_MASK, up, floor) & INFO_UP != 0 {
            let ground = crate::sca::ground_at(sca, up, radius, PLAYER_MASK);
            if ground.height <= up[1] && !ground.blocked {
                self.work.routine = [MODE_MOVE, MOVE_CLIMB, 0, 0];
                return true;
            }
        }
        let (dx, dz) = rotate(facing, reach(&DROP_REACH), 0);
        let down = [x + dx, y, z + dz];
        let ground = crate::sca::ground_at(sca, down, radius, PLAYER_MASK);
        let landing = ground.height.min(ground.ceiling);
        if landing > y && crate::sca::info_at(sca, PLAYER_MASK, down, floor) & INFO_DOWN != 0 {
            self.work.routine = [MODE_MOVE, MOVE_CLIMB, CLIMB_IS_DROP, 0];
            return true;
        }
        false
    }

    /// The first steps of `pl_mv_08`: turn square to the box. It answers
    /// whether the turn has finished.
    fn square_up(&mut self) -> bool {
        self.joint_move();
        let angle = self.player.angle;
        let step = (angle >> 2) & 0xFF;
        let angle = if angle & 0x200 == 0 {
            angle.wrapping_sub(step)
        } else {
            angle.wrapping_add(step)
        };
        self.player.angle = angle;
        if angle & 0x3E0 != 0 {
            return false;
        }
        self.player.angle = angle & 0xFF00;
        true
    }

    /// The landing's turn back to the pad.
    fn pl_br_jump_down(&mut self, key: u16) {
        if self.work.routine[3] <= 4 {
            return;
        }
        if key & KEY_FORWARD != 0 {
            self.set_routine(if key & KEY_RUN != 0 {
                MOVE_RUN
            } else {
                MOVE_FORWARD
            });
        }
        if key & KEY_BACKWARD != 0 {
            self.set_routine(MOVE_BACKWARD);
        }
        if key & KEY_TURN != 0 {
            self.set_routine(MOVE_ROTATE);
        }
        self.aim_branch(key);
    }

    /// `pl_mv_08`: turn square to a box, then climb it, or hand over to the
    /// jump down.
    fn pl_mv_climb(&mut self) {
        if self.work.routine[3] == 0 {
            self.work.routine[3] = 1;
            self.work.bank = Bank::Weapon;
            self.work.move_no = 0;
            self.work.move_cnt = 0;
            self.work.hokan = FULL_BLEND;
            self.work.spd[0] = 0x32;
        }
        match self.work.routine[3] {
            1 => {
                if self.square_up() {
                    if self.work.routine[2] & CLIMB_IS_DROP != 0 {
                        self.work.routine = [MODE_MOVE, MOVE_JUMP_DOWN, self.work.routine[2], 0];
                    } else {
                        self.work.routine[3] = 2;
                    }
                }
                return;
            }
            2 => {
                self.work.routine[3] = 3;
                self.work.bank = Bank::Room;
                self.work.move_no = CLIMB_CLIP;
                self.work.hokan = STAIRS_FIRST_BLEND;
            }
            3 => {}
            _ => {
                self.set_routine(MOVE_IDLE);
                return;
            }
        }
        // The body lifts on the clip's frame: the move and the height together.
        let small = self.tick.id == SMALL_ID;
        let (lift, reach, end) = if small {
            (99, 0x224, 0x77)
        } else {
            (0x1D, 1000, 0x31)
        };
        let frame = self.work.move_cnt;
        if frame == 0x11 {
            self.work.damage_cnt |= DAMAGE_LOCK;
        }
        if frame == lift {
            self.work.spd[0] = reach;
            self.add_speed_xz();
            self.player.pos[1] -= crate::sca::FLOOR_HEIGHT;
        }
        if frame == end {
            self.work.damage_cnt &= !DAMAGE_LOCK;
        }
        if self.joint_move_late(STAIRS_FIRST_STEP) {
            self.work.routine[3] = 4;
        }
        if self.work.now_seq & FRAME_FOOTSTEP != 0 {
            self.player.emit_footstep(self.work.move_cnt, 1);
        }
    }

    /// `pl_mv_09`: jump off the edge, fall and land.
    fn pl_mv_jump_down(&mut self) {
        match self.work.routine[3] {
            0 => {
                self.work.routine[3] = 1;
                self.work.bank = Bank::Room;
                self.work.move_no = JUMP_CLIP;
                self.work.move_cnt = 0;
                self.work.hokan = FULL_BLEND;
                self.jump_step();
            }
            1 => self.jump_step(),
            2 => {
                self.work.damage_cnt |= DAMAGE_LOCK;
                self.work.routine[3] = 3;
                self.work.move_no = JUMP_FALL_CLIP;
                self.work.move_cnt = 0;
                self.work.hokan = 0;
                let small = self.tick.id == SMALL_ID;
                let (speed, drop, timer) = if small {
                    (600, 0x96, 3)
                } else {
                    (1000, 0x226, 5)
                };
                self.work.spd[0] = speed;
                self.player.pos[1] += drop;
                self.work.timer0 = timer;
                self.work.spd[2] = 0;
                self.add_speed_xz();
                self.fall();
            }
            3 => self.fall(),
            4 => {
                self.work.routine[3] = 5;
                self.work.move_no = LAND_CLIP;
                self.work.move_cnt = 4;
                self.work.hokan = FULL_BLEND;
                self.player.pos[1] = self.ground();
                self.work.damage_cnt &= !DAMAGE_LOCK;
                self.player.emit_footstep(self.work.move_cnt, 1);
                self.land();
            }
            5 => self.land(),
            _ => self.set_routine(MOVE_IDLE),
        }
    }

    /// The ground under the player.
    fn ground(&self) -> i32 {
        self.tick.room.sca.as_ref().map_or(0, |sca| {
            crate::sca::ground_at(
                sca,
                self.player.pos,
                self.player.radius,
                crate::sca::PLAYER_MASK,
            )
            .height
        })
    }

    fn jump_step(&mut self) {
        let ended = self.joint_move();
        let last = if self.tick.id == SMALL_ID { 0x1A } else { 0xE };
        self.work.routine[3] = if ended { 2 } else { self.work.routine[3] };
        if self.work.move_cnt == last {
            self.work.routine[3] = 2;
        }
    }

    fn fall(&mut self) {
        self.player.pos[1] += i32::from(self.work.timer0 as i16) * 0x1E;
        self.work.timer0 += 1;
        self.joint_move();
        if self.ground() - 300 < self.player.pos[1] {
            self.work.routine[3] = 4;
        }
    }

    fn land(&mut self) {
        if self.work.move_cnt == 6 {
            self.player.emit_footstep(self.work.move_cnt, 1);
        }
        if self.joint_move() {
            self.work.routine[3] = 6;
        }
    }

    /// `Sca_hit_stairs`: from either end, in reach of the middle of its width.
    /// It also writes the end's position to `Atd[0]`, which the next tick
    /// replaces from the player's own.
    fn sca_hit_stairs(&mut self, stairs: &ScaRect) {
        let (x, z) = (i32::from(stairs.x), i32::from(stairs.z));
        let (width, depth) = (i32::from(stairs.width), i32::from(stairs.depth));
        let (px, pz) = (self.player.pos[0] - x, self.player.pos[2] - z);
        let axis = stairs.kind >> 4;
        let along_x = axis & 2 == 0;
        let (across, across_size, along, along_size) = if along_x {
            (pz, depth, px, width)
        } else {
            (px, width, pz, depth)
        };
        if ((across_size - 500) as u32) < ((across - 0xFA) as u32)
            || ((along_size + 0x514) as u32) < ((along + 0x28A) as u32)
        {
            return;
        }
        let (low, high) = crate::sca::stairs_ends(stairs);
        let diff = low - high;
        let steps = ((i64::from(diff) * 0x6E5D_4C3B) >> 32) as i32 - diff;
        self.work.timer1 = ((steps >> 10) - (steps >> 31) - 1) as i16;
        self.work.routine[1] = MOVE_STAIRS;
        self.work.routine[2] = 0;
        let near = along < along_size >> 1;
        self.work.timer2 = u8::from((axis & 1 == 1) == near);
        self.work.timer0 = match (along_x, near) {
            (true, true) => 0,
            (true, false) => 0x800,
            (false, true) => 0xC00,
            (false, false) => 0x400,
        };
    }

    /// `pl_mv_07`: turn to the stairs, climb them step by step, walk off.
    /// The shadow's own path isn't ported.
    fn pl_mv_stairs(&mut self) {
        let climb = STAIRS_CLIPS[usize::from(self.work.timer2 != 0)];
        if self.work.routine[2] == 0 {
            self.work.routine[2] = 1;
            self.work.bank = Bank::Weapon;
            self.work.move_no = 0;
            self.work.move_cnt = 0;
            self.work.hokan = FULL_BLEND;
            self.work.damage_cnt |= DAMAGE_LOCK;
        }
        match self.work.routine[2] {
            1 => {
                let target = self.work.timer0;
                self.player.angle = goto(self.player.angle, target, STAIRS_TURN);
                let error =
                    (i32::from(target) - i32::from(self.player.angle) + STAIRS_TURN_CLOSE) & 0x0FFF;
                if error < STAIRS_TURN_CLOSE * 2 {
                    self.work.routine[2] = 2;
                    self.player.angle = goto(self.player.angle, target, STAIRS_TURN);
                }
                self.joint_move();
            }
            2 => {
                self.work.routine[2] = 3;
                self.work.bank = Bank::Room;
                self.work.move_no = climb;
                self.work.move_cnt = 0;
                self.work.hokan = STAIRS_FIRST_BLEND;
                self.stairs_step(climb, STAIRS_FIRST_STEP);
            }
            3 => self.stairs_step(climb, STAIRS_FIRST_STEP),
            4 => self.stairs_step(climb, BLEND_STEP),
            5 => {
                self.spd_get();
                if self.joint_move() {
                    self.work.routine[2] = 6;
                }
                self.add_speed_xyz();
            }
            6 | 7 => {
                if self.work.routine[2] == 6 {
                    self.work.routine[2] = 7;
                    self.work.bank = Bank::Weapon;
                    self.work.move_no = 0;
                    self.work.move_cnt = 0;
                    self.work.hokan = FULL_BLEND;
                }
                self.spd_get();
                self.joint_move();
                self.add_speed_xz();
                if self.work.move_cnt > STAIRS_WALK_OFF_FRAME {
                    self.set_routine(MOVE_IDLE);
                    self.work.damage_cnt &= !DAMAGE_LOCK;
                }
            }
            _ => {}
        }
        self.footstep(0);
    }

    /// A step of the climb's first or repeated clip: the last step's clip
    /// follows when the steps run out.
    fn stairs_step(&mut self, climb: u8, late: u16) {
        self.spd_get();
        if self.joint_move_late(late) {
            let first = self.work.routine[2] == 3;
            self.work.timer1 -= 1;
            if first {
                self.work.routine[2] = 4;
                self.work.move_no = climb + 1;
                self.work.move_cnt = 0;
                self.work.hokan = 0;
            }
            if self.work.timer1 == 0 {
                self.work.routine[2] = 5;
                self.work.move_no = climb + 2;
                self.work.move_cnt = 0;
                self.work.hokan = 0;
            }
        }
        self.add_speed_xyz();
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
        self.end_branch(key);
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
        self.end_branch(key);
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
        self.stairs_branch(key);
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
        self.aim_branch(key);
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
        self.end_branch(key);
    }

    /// `pl_mv05_w1`: the handguns' stance. `Status_flg`'s top bits hold the tilt.
    fn pl_mv_aim(&mut self, key: u16) {
        // `Foot_set_pl` reads last tick's world matrix from the parts.
        let planted = self.foot_position();
        match self.work.routine[2] {
            0 => self.aim_raise(key),
            1 => self.aim_hold(key),
            2 => self.aim_fire(key),
            3 => self.aim_lower(key),
            4 => self.aim_reload(),
            5 => {
                // `pl_mv05_w1_t5`: a lock on nothing goes straight back.
                self.work.routine[2] = 1;
                self.work.routine[3] = 0;
            }
            _ => {}
        }
        if self.work.spd[0] == 0 && (self.work.routine[2] != 0 || self.work.hokan == 0) {
            self.foot_set(planted);
        }
    }

    /// The handguns' foot in the world, on the current pose: `Foot_set_pl`'s
    /// chain, the root, the hips and that leg's three parts.
    pub(crate) fn foot_position(&self) -> Option<[i32; 3]> {
        self.chain_position(&AIM_FOOT_CHAIN)
    }

    /// `Rot_neck`: turn the head toward what `Plc_neck` asked for, or back to
    /// its pose. `Pl_neck`'s aim at the nearest enemy waits for enemies.
    pub(crate) fn rot_neck(&mut self) {
        if self.work.neck.flags & 1 != 0 {
            return;
        }
        let (clips, keyframes) = self.tick.banks.get(self.work.bank);
        let Some(pose) = self.player.anim.pose_keyframe(clips, keyframes) else {
            return;
        };
        let (Some(bone), Some(root)) = (
            pose.rotations.get(NECK_JOINT).copied(),
            pose.rotations.first().copied(),
        ) else {
            return;
        };
        let from = self.chain_position(&[0, NECK_JOINT]);
        let cdir = i32::from(self.player.angle as i16);
        let root = i32::from(root[1]);
        let facing = cdir + root;
        let bone = [i32::from(bone[1]), i32::from(bone[2])];
        let neck = &mut self.work.neck;
        let flags = neck.flags;
        let mut aim = [0; 2];
        neck.position = from;
        if flags & 4 != 0
            && let Some(from) = from
        {
            let to = if neck.words[1] == NECK_AT_WORK {
                let necks = &self.tick.necks;
                necks.get(neck.words[0] as usize).copied().flatten()
            } else {
                Some([neck.words[0], neck.words[1], neck.words[2]].map(i32::from))
            };
            if let Some(to) = to {
                aim = neck_direction(from, to);
            }
        }
        if flags & 8 != 0 {
            aim = [i32::from(neck.words[1]) + facing, i32::from(neck.words[2])];
        }
        if flags & 0x40 != 0 {
            if neck.words[2] == 0 {
                neck.words[2] = 1;
                neck.words[1] = (NECK_LIMITS[0] + facing - 0x80) as i16;
            }
            aim[0] = i32::from(neck.words[1]);
        }
        if flags & 2 != 0 {
            aim[0] = bone[0] + facing;
        }
        if flags & 0x10 != 0 {
            aim[1] = bone[1];
        }
        if flags & 0x80 == 0 {
            aim = [bone[0] + facing, bone[1]];
        }

        let rate = if flags & 0x80 != 0 {
            i32::from(neck.rates[0])
        } else {
            NECK_RATES[0]
        };
        let limit = NECK_LIMITS[0];
        if limit * 2 < (aim[0] - facing + limit) & 0xFFF {
            aim[0] = if (aim[0] - facing) & 0x800 == 0 {
                limit + facing
            } else {
                facing - limit
            };
        }
        let old = i32::from(neck.turn[0]);
        let error = rate - old - bone[0] - facing + aim[0];
        let mut turn = old - rate;
        if error & 0x800 == 0 {
            turn += rate * 2;
        }
        if error & 0xFFF < rate * 2 {
            turn = aim[0] - bone[0] - facing;
            if neck.flags & 0x40 != 0 {
                neck.words[0] = neck.words[0].wrapping_sub(1);
                if neck.words[0] == 0 {
                    neck.flags = 0x12;
                }
                neck.words[1] = (facing * 2 + 0x80 - i32::from(neck.words[1])) as i16;
            }
        }
        neck.turn[0] = turn as i16;

        if neck.flags & 0x20 != 0 {
            if neck.words[2] == 0 {
                neck.words[2] = 1;
                neck.words[1] = NECK_LIMITS[1] as i16;
            }
            aim[1] = i32::from(neck.words[1]);
        }
        let rate = if neck.flags & 0x80 != 0 {
            i32::from(neck.rates[1])
        } else {
            NECK_RATES[1]
        };
        let limit = NECK_LIMITS[1];
        if limit * 2 < (aim[1] + limit) & 0xFFF {
            aim[1] = if aim[1] & 0x800 == 0 { limit } else { -limit };
        }
        let old = i32::from(neck.turn[1]);
        let error = rate - old - bone[1] + aim[1];
        let mut turn = old - rate;
        if error & 0x800 == 0 {
            turn += rate * 2;
        }
        if error & 0xFFF < rate * 2 {
            turn = aim[1] - bone[1];
            if neck.flags & 0x20 != 0 {
                if aim[1] != 0 {
                    neck.words[0] = neck.words[0].wrapping_sub(1);
                    if neck.words[0] == 0 {
                        neck.flags = 0x12;
                    }
                    neck.words[1] = 0;
                } else {
                    neck.words[1] = NECK_LIMITS[1] as i16;
                }
            }
        }
        neck.turn[1] = turn as i16;
    }

    /// Where the last of `chain`'s joints is in the world, on the current pose.
    fn chain_position(&self, chain: &[usize]) -> Option<[i32; 3]> {
        Some(self.chain_matrix(chain)?.t)
    }

    /// Where a part's matrix puts a point of its own: what `Esp_call` is given
    /// a part's matrix and a vector for.
    pub(crate) fn part_point(&self, part: usize, point: [i32; 3]) -> Option<[i32; 3]> {
        fn chain_to(
            children: &[Vec<u8>],
            from: usize,
            part: usize,
            chain: &mut Vec<usize>,
        ) -> bool {
            chain.push(from);
            if from == part {
                return true;
            }
            let found = children.get(from).is_some_and(|next| {
                next.iter()
                    .any(|&child| chain_to(children, usize::from(child), part, chain))
            });
            if !found {
                chain.pop();
            }
            found
        }
        let mut chain = Vec::new();
        if !chain_to(&self.tick.skeleton.children, 0, part, &mut chain) {
            return None;
        }
        let world = self.chain_matrix(&chain)?;
        Some(std::array::from_fn(|axis| {
            let turned: i32 = (0..3)
                .map(|index| i32::from(world.m[axis][index]) * point[index])
                .sum();
            world.t[axis] + (turned >> 12)
        }))
    }

    fn chain_matrix(&self, chain: &[usize]) -> Option<Matrix> {
        let (clips, keyframes) = self.tick.banks.get(self.work.bank);
        let pose = self.player.anim.pose_keyframe(clips, keyframes)?;
        let mut world = Matrix {
            m: rot_matrix([0, self.player.angle as i16, 0]),
            t: self.player.pos,
        };
        for &part in chain {
            let t = if part == 0 {
                let [x, y, z] = pose.offset;
                [x, self.work.scaled_height(y), z]
            } else {
                *self.tick.skeleton.relative.get(part)?
            };
            world = world.comp(&Matrix {
                m: rot_matrix(*pose.rotations.get(part)?),
                t: t.map(i32::from),
            });
        }
        Some(world)
    }

    /// `Foot_set_pl`: move the player so the foot stays where it was planted.
    pub(crate) fn foot_set(&mut self, planted: Option<[i32; 3]>) {
        if let (Some(planted), Some(foot)) = (planted, self.foot_position()) {
            self.player.pos[0] += planted[0] - foot[0];
            self.player.pos[2] += planted[2] - foot[2];
        }
    }

    /// A clip of the stance, from its first frame, blended in.
    fn aim_clip(&mut self, clip: u8) {
        self.work.routine[3] = 1;
        self.work.bank = Bank::Weapon;
        self.work.move_no = clip;
        self.work.move_cnt = 0;
        self.work.hokan = FULL_BLEND;
    }

    fn aim_turn(&mut self, key: u16, rate: u16) {
        if key & KEY_LEFT != 0 {
            self.player.angle = self.player.angle.wrapping_sub(rate) & 0x0FFF;
        }
        if key & KEY_RIGHT != 0 {
            self.player.angle = self.player.angle.wrapping_add(rate) & 0x0FFF;
        }
    }

    fn aim_stage(&mut self, stage: u8) {
        self.work.routine[2] = stage;
        self.work.routine[3] = 0;
    }

    fn set_tilt(&mut self, tilt: u16) {
        self.work.status_flg = self.work.status_flg & 0x1FFF | tilt;
    }

    /// `pl_mv05_w1_t0`.
    fn aim_raise(&mut self, key: u16) {
        match self.work.routine[3] {
            0 => {
                self.aim_clip(AIM_RAISE_CLIP);
                self.set_tilt(TILT_LEVEL);
                self.work.spd[0] = 0;
            }
            1 => {}
            2 => {
                self.aim_stage(1);
                return;
            }
            _ => return,
        }
        self.aim_turn(key, AIM_RAISE_TURN);
        if self.joint_move() {
            self.work.routine[3] += 1;
        }
        if self.work.move_cnt > AIM_TILT_DOWN_FRAME && key & KEY_TILT_DOWN != 0 {
            self.aim_stage(1);
            self.set_tilt(TILT_DOWN);
            return;
        }
        if self.work.move_cnt > AIM_TILT_UP_FRAME && key & KEY_TILT_UP != 0 {
            self.aim_stage(1);
            self.set_tilt(TILT_UP);
        }
    }

    /// `pl_mv05_w1_t1`.
    fn aim_hold(&mut self, key: u16) {
        if self.work.routine[3] == 0 {
            let clip = match self.work.status_flg >> 13 {
                1 => AIM_HOLD_CLIPS[0],
                2 => AIM_HOLD_CLIPS[1],
                4 => AIM_HOLD_CLIPS[2],
                _ => 0,
            };
            self.aim_clip(clip);
        }
        if key & KEY_AIM == 0 {
            self.aim_stage(3);
            return;
        }
        if key & KEY_TILT_UP != 0 && self.work.status_flg & TILT_UP == 0 {
            self.work.routine[3] = 0;
            self.set_tilt(TILT_UP);
            return;
        }
        if key & KEY_TILT_DOWN != 0 && self.work.status_flg & TILT_DOWN == 0 {
            self.work.routine[3] = 0;
            self.set_tilt(TILT_DOWN);
            return;
        }
        if key & (KEY_TILT_UP | KEY_TILT_DOWN) == 0 && self.work.status_flg & TILT_LEVEL == 0 {
            self.work.routine[3] = 0;
            self.set_tilt(TILT_LEVEL);
            return;
        }
        if self.tick.input.lock_drop {
            self.aim_stage(5);
        }
        if key & KEY_FIRE != 0 {
            if self.tick.weapon.loaded {
                self.aim_stage(2);
                return;
            }
            if self.tick.input.fire_pressed && self.tick.weapon.spare {
                self.aim_stage(4);
                return;
            }
        }
        self.aim_turn(key, AIM_HOLD_TURN);
        self.joint_move();
    }

    /// `pl_mv05_w1_t2`. Nothing to hit yet: the shot only spends a round.
    fn aim_fire(&mut self, key: u16) {
        if self.work.routine[3] == 0 {
            let clip = match self.work.status_flg >> 13 {
                1 => AIM_FIRE_CLIPS[0],
                2 => AIM_FIRE_CLIPS[1],
                4 => AIM_FIRE_CLIPS[2],
                _ => 0,
            };
            self.aim_clip(clip);
            self.work.fired = true;
            self.player.queue_sound(SOUND_SHOT);
            self.work.spd = [0; 3];
        }
        self.aim_turn(key, AIM_FIRE_TURN);
        if key & KEY_AIM == 0 && self.work.move_cnt > AIM_FIRE_CANCEL_FRAME {
            self.aim_stage(1);
            self.work.spd = [0; 3];
            return;
        }
        if self.joint_move() {
            self.aim_stage(1);
            self.work.spd = [0; 3];
        }
    }

    /// `pl_mv05_w1_t3`: the raising clip backwards, back to the pad.
    fn aim_lower(&mut self, key: u16) {
        if self.work.routine[3] == 0 {
            self.aim_clip(AIM_RAISE_CLIP);
            self.work.move_cnt = 1;
            self.work.hokan = AIM_LOWER_BLEND;
        }
        self.aim_turn(key, AIM_HOLD_TURN);
        if self.joint_move_late_reversed(AIM_LOWER_STEP, true) {
            self.set_routine(MOVE_IDLE);
        }
    }

    /// `pl_mv05_w1_t4`: the frame marked for it fills the magazine.
    fn aim_reload(&mut self) {
        if self.work.routine[3] == 0 {
            self.aim_clip(AIM_RELOAD_CLIP);
        }
        if self.joint_move() {
            self.aim_stage(1);
        }
        if self.work.now_seq & FRAME_SOUND != 0 {
            self.player
                .queue_sound(((self.work.now_seq & FRAME_SOUND_SLOT) >> 16) as u16);
        }
        if self.work.now_seq & FRAME_RELOAD != 0 {
            self.work.reloaded = true;
        }
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
    pub(crate) fn spd_get(&mut self) {
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

    pub(crate) fn joint_move_late(&mut self, late: u16) -> bool {
        let reverse = self.work.routine[0] == MODE_SCENARIO && self.work.sce_flg & 0x80 != 0;
        self.joint_move_late_reversed(late, reverse)
    }

    pub(crate) fn joint_move_late_reversed(&mut self, late: u16, reverse: bool) -> bool {
        let (clips, _) = self.tick.banks.get(self.work.bank);
        let Some(clip) = clips.get(usize::from(self.work.move_no)) else {
            return true;
        };
        if clip.frames.is_empty() {
            return true;
        }
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
        anim.re2_blend = true;
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
    /// `FUN_004c2bc0`: the clip's next frame, without posing it.
    pub(crate) fn skip_frame(&mut self) {
        let (clips, _) = self.tick.banks.get(self.work.bank);
        let Some(clip) = clips.get(usize::from(self.work.move_no)) else {
            return;
        };
        if let Some(frame) = clip.frames.get(usize::from(self.work.move_cnt)) {
            self.work.now_seq = frame.flags;
        }
        self.work.move_cnt = self.work.move_cnt.wrapping_add(1);
        if usize::from(self.work.move_cnt) >= clip.frames.len() {
            self.work.move_cnt = 0;
        }
    }

    /// The flags of the frame `Joint_move` played last.
    pub(crate) fn now_seq(&self) -> u32 {
        self.work.now_seq
    }

    /// `Add_speedXZ` with its `Add_dir`.
    pub(crate) fn add_speed_xz_at(&mut self, dir: u16) {
        let (dx, dz) = rotate(
            self.player.angle.wrapping_add(dir),
            i32::from(self.work.spd[0]),
            i32::from(self.work.spd[2]),
        );
        self.player.pos[0] += dx;
        self.player.pos[2] += dz;
    }

    pub(crate) fn add_speed_xz(&mut self) {
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
    pub(crate) fn scenario(&mut self) {
        if self.work.sce_flg & 0x30 != 0x30 {
            self.spl_sce();
        }
        self.work.sce_flg ^= 0x20;
    }

    fn spl_sce(&mut self) {
        let id = self.tick.id;
        match self.work.routine[1] {
            0x00 => self.spl_sce_mv00(Bank::Room),
            0x01 => self.spl_sce_mv00(Bank::Pld),
            0x02 | 0x03 => self.spl_sce_mv00(Bank::Weapon),
            0x04 | 0x12 => self.spl_walk(by_id(id, &SCE_WALK_SPEED, &SPL_WALK_SPEED), 0),
            0x05 => self.spl_walk(by_id(id, &SCE_RUN_SPEED, &SPL_RUN_SPEED), 1),
            0x06 => {
                if self.work.routine[2] == 0 {
                    self.start_motion(Bank::Weapon, IDLE_CLIPS[0]);
                }
                self.joint_move();
            }
            0x07 => self.spl_back(by_id(id, &SCE_BACK_SPEED, &SPL_BACK_SPEED), 0),
            0x08 => self.spl_back(by_id(id, &SCE_BACK_RUN_SPEED, &SPL_BACK_RUN_SPEED), 1),
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
pub(crate) fn rsin(angle: i32) -> i32 {
    let step = angle & 0x7FF;
    let step = if step < 0x400 { step } else { 0x800 - step };
    let value = (f64::from(step) * std::f64::consts::PI / 2048.0).sin() * 4096.0;
    let value = value.round() as i32;
    if angle & 0x800 != 0 { -value } else { value }
}

/// `rcos`.
pub(crate) fn rcos(angle: i32) -> i32 {
    rsin(angle + 0x400)
}

/// `MulMatrix0`: every element is narrowed to a `short`.
fn mul_matrix(a: &[[i16; 3]; 3], b: &[[i16; 3]; 3]) -> [[i16; 3]; 3] {
    std::array::from_fn(|row| {
        std::array::from_fn(|column| {
            let sum: i32 = (0..3)
                .map(|k| i32::from(a[row][k]) * i32::from(b[k][column]))
                .sum();
            (sum >> 12) as i16
        })
    })
}

/// `RotMatrix`: Z, then Y, then X, onto the identity.
fn rot_matrix(angle: [i16; 3]) -> [[i16; 3]; 3] {
    let (sin, cos) = (
        |a: i16| rsin(i32::from(a)) as i16,
        |a: i16| rcos(i32::from(a)) as i16,
    );
    let (sx, cx) = (sin(angle[0]), cos(angle[0]));
    let (sy, cy) = (sin(angle[1]), cos(angle[1]));
    let (sz, cz) = (sin(angle[2]), cos(angle[2]));
    let z = [[cz, -sz, 0], [sz, cz, 0], [0, 0, 0x1000]];
    let y = [[cy, 0, sy], [0, 0x1000, 0], [-sy, 0, cy]];
    let x = [[0x1000, 0, 0], [0, cx, -sx], [0, sx, cx]];
    mul_matrix(&x, &mul_matrix(&y, &z))
}

/// A `MATRIX`.
struct Matrix {
    m: [[i16; 3]; 3],
    t: [i32; 3],
}

impl Matrix {
    /// `CompMatrix`, whose `ApplyMatrixLV` floors.
    fn comp(&self, child: &Matrix) -> Matrix {
        Matrix {
            m: mul_matrix(&self.m, &child.m),
            t: std::array::from_fn(|row| {
                let sum: i32 = (0..3)
                    .map(|k| i32::from(self.m[row][k]).wrapping_mul(child.t[k]))
                    .fold(0, i32::wrapping_add);
                (sum >> 12) + self.t[row]
            }),
        }
    }
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
pub(crate) fn direction_ck(from_x: i32, from_z: i32, to_x: i32, to_z: i32) -> u16 {
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
pub(crate) fn goto(angle: u16, target: u16, rate: i32) -> u16 {
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

/// A value of the tables `Spl_sce*` and `Plc_dest` keep by character.
fn by_id(id: u8, low: &[i16; 16], high: &[i16; 32]) -> i16 {
    match id {
        0..0x10 => low[usize::from(id)],
        0x40..0x60 => high[usize::from(id - 0x40)],
        _ => 0,
    }
}

/// `Plc_dest`: start routine `routine`, walking to `(x, z)`, raising room flag
/// `flag` there. The turn rate depends on the routine and the character.
pub fn plc_dest(work: &mut Work, id: u8, routine: u8, flag: u8, x: i16, z: i16) {
    if work.sce_flg & 4 == 0 || work.routine[1] != routine {
        work.routine = [MODE_SCENARIO, routine, 0, 0];
    }
    let low = [DEST_WALK_TURN; 16];
    let run = [DEST_RUN_TURN; 16];
    let back = [DEST_BACK_TURN; 16];
    match routine {
        0x04 | 0x12 => work.sce_free[0] = by_id(id, &low, &SPL_DEST_WALK_TURN),
        0x05 => work.sce_free[0] = by_id(id, &run, &SPL_DEST_RUN_TURN),
        0x07 => work.sce_free[0] = by_id(id, &back, &SPL_DEST_BACK_TURN),
        0x08 => work.sce_free[0] = by_id(id, &back, &SPL_DEST_BACK_RUN_TURN),
        0x09 => work.sce_free[0] = by_id(id, &DEST_ROTATE_TURN, &SPL_DEST_ROTATE_TURN),
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

/// `Spl_Init`: a script's character, whose first pose is the first frame of
/// its model's first clip, and which then waits for the script.
pub fn spl_init(work: &mut Work, player: &mut PlayerState, tick: &Tick<'_>, kind: u16) {
    work.routine = if kind & SPL_AUTO == 0 {
        [MODE_SCENARIO, 6, 0, 0]
    } else {
        [MODE_MOVE, 0, 0, 0]
    };
    work.bank = Bank::Pld;
    work.move_no = 0;
    work.move_cnt = 0;
    work.hokan = 0;
    work.neck.flags = 0x92;
    work.parts0_y = tick
        .id
        .checked_sub(0x40)
        .and_then(|index| CHARACTER_PARTS0_Y.get(usize::from(index)))
        .map_or(0x1000, |&scale| i32::from(scale));
    let mut mover = Mover {
        work,
        player,
        tick,
        arrived: None,
        stairs: None,
    };
    mover.joint_move_late(SPL_INIT_STEP);
}

/// Whether it has no routine yet, which `_Spl` starts with `Spl_Init`.
pub fn uninitialised(work: &Work) -> bool {
    work.routine[0] == 0
}

/// Whether a script is steering it: `_Spl`'s `Spl_Scenario`, the only routine
/// a character has so far.
pub fn in_scenario(work: &Work) -> bool {
    work.routine[0] == MODE_SCENARIO
}

/// `Em_init_move`'s second `_Spl`, for a character the room places mid-clip:
/// the routine and clip it plays, and its `Sce_flg`.
pub fn spl_start(work: &mut Work, start: u16, flags: u16) {
    work.routine = [
        MODE_SCENARIO,
        ((start & 0xFF00).wrapping_sub(0x100) >> 8) as u8,
        0,
        0,
    ];
    work.move_no = start as u8;
    work.sce_flg = flags;
}

/// `Plc_ret`: back to the pad.
pub fn plc_ret(work: &mut Work) {
    work.routine = [MODE_MOVE, MOVE_IDLE, 0, 0];
    work.neck.flags = 0;
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
        skeleton: Skeleton,
        weapon: Weapon,
        clips: Vec<Clip>,
        keyframes: Vec<Keyframe>,
    }

    #[test]
    fn the_neck_turns_to_plc_necks_aim_and_back() {
        let mut setup = Setup::new(0);
        for keyframe in &mut setup.keyframes {
            keyframe.rotations = vec![[0; 3]; NECK_JOINT + 1];
        }
        plc_neck(&mut setup.work, 2, [0, 0, -200], [50, 50]);
        setup.tick(Input::default(), 200);
        assert_eq!(setup.work.neck.turn, [0, -50]);
        for _ in 0..4 {
            setup.tick(Input::default(), 200);
        }
        assert_eq!(setup.work.neck.turn, [0, -200]);

        let mut pose = setup.keyframes[0].clone();
        setup.work.neck.apply(&mut pose);
        assert_eq!(pose.rotations[NECK_JOINT], [0, 0, -200]);

        // With nothing to look at it goes back at the joint's own rate.
        plc_ret(&mut setup.work);
        setup.tick(Input::default(), 200);
        assert_eq!(setup.work.neck.turn, [0, -104]);
        for _ in 0..2 {
            setup.tick(Input::default(), 200);
        }
        assert_eq!(setup.work.neck.turn, [0, 0]);
    }

    #[test]
    fn parts_set_hides_and_shows_a_part() {
        let mut work = Work::default();
        set_part_flags(&mut work, 11, 0);
        assert_eq!(work.hidden_parts, 1 << 11);
        set_part_flags(&mut work, 11, 1);
        assert_eq!(work.hidden_parts, 0);
    }

    #[test]
    fn a_characters_shadow_follows_spl_inits_table() {
        assert_eq!(character_shadow(0x48), (500, 600));
        assert_eq!(character_shadow(0x41), (0x1A4, 0x208));
        assert_eq!(character_shadow(0x4F), (0x172, 0x1CC));
    }

    #[test]
    fn a_bodys_height_scales_its_first_part_rounding_down() {
        let mut work = Work {
            parts0_y: i32::from(CHARACTER_PARTS0_Y[0x48 - 0x40]),
            ..Work::default()
        };
        let mut pose = Keyframe {
            offset: [7, -1681, 9],
            ..Keyframe::default()
        };
        work.neck.turn = [0; 2];
        work.apply_to_pose(&mut pose);
        assert_eq!(pose.offset, [7, -1782, 9]);
    }

    #[test]
    fn the_neck_stays_within_its_limits() {
        let mut setup = Setup::new(0);
        for keyframe in &mut setup.keyframes {
            keyframe.rotations = vec![[0; 3]; NECK_JOINT + 1];
        }
        plc_neck(&mut setup.work, 2, [0, 0x7ff, -0x7ff], [0xff, 0xff]);
        for _ in 0..10 {
            setup.tick(Input::default(), 200);
        }
        assert_eq!(
            setup.work.neck.turn,
            [NECK_LIMITS[0] as i16, -NECK_LIMITS[1] as i16]
        );
    }

    impl Setup {
        fn new(step: i16) -> Self {
            let room = RoomState::default();
            let (clips, keyframes) = bank(10, 8, step);
            Self {
                work: Work::default(),
                player: player::spawn(RoomId::parse("1000").unwrap(), &room),
                room,
                skeleton: Skeleton::default(),
                weapon: Weapon::default(),
                clips,
                keyframes,
            }
        }

        fn tick(&mut self, input: Input, life: i32) -> Option<u8> {
            let bank = (self.clips.as_slice(), self.keyframes.as_slice());
            let tick = Tick {
                room: &self.room,
                skeleton: &self.skeleton,
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
                weapon: self.weapon,
                necks: Default::default(),
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

    /// A setup holding a handgun, in banks with every stance clip.
    fn armed(loaded: bool, spare: bool) -> Setup {
        let mut setup = Setup::new(0);
        (setup.clips, setup.keyframes) = bank(0x11, 4, 0);
        setup.weapon = Weapon {
            id: 2,
            loaded,
            spare,
        };
        setup
    }

    fn aiming(fire: bool, fire_pressed: bool) -> Input {
        Input {
            aim: true,
            fire,
            fire_pressed,
            ..Input::default()
        }
    }

    /// A setup at `pos` facing `angle`, with stairs along z over floor 0.
    fn at_stairs(pos: [i32; 3], angle: u16) -> Setup {
        let mut setup = Setup::new(0);
        (setup.clips, setup.keyframes) = bank(10, 12, 0);
        setup.room.sca = Some(crate::state::Sca {
            records: vec![ScaRect {
                x: 1000,
                z: 1000,
                width: 1000,
                depth: 1000,
                id: 0x8000 | 12,
                kind: 0x906F,
                floor: 3,
            }],
            ..Default::default()
        });
        setup.player.pos = pos;
        setup.player.angle = angle;
        setup
    }

    fn action() -> Input {
        Input {
            action_held: true,
            ..Input::default()
        }
    }

    #[test]
    fn the_action_key_climbs_stairs_from_the_low_end_and_descends_from_the_high() {
        let mut setup = at_stairs([1500, 0, 700], 0xC00);
        setup.tick(action(), 200);
        assert_eq!(setup.work.routine[1], MOVE_STAIRS);
        assert_eq!(setup.work.timer0, 0xC00);
        assert_eq!(setup.work.timer2, 0);
        assert!(setup.work.climbing());

        let mut setup = at_stairs([1500, 0, 2300], 0x400);
        setup.tick(action(), 200);
        assert_eq!(setup.work.routine[1], MOVE_STAIRS);
        assert_eq!(setup.work.timer0, 0x400);
        assert_eq!(setup.work.timer2, 1);
    }

    #[test]
    fn stairs_refuse_a_press_too_near_their_side() {
        let mut setup = at_stairs([1100, 0, 700], 0xC00);
        setup.tick(action(), 200);
        assert_eq!(setup.work.routine[1], MOVE_IDLE);
    }

    #[test]
    fn a_climb_plays_its_steps_then_walks_off_into_idle() {
        let mut setup = at_stairs([1500, 0, 700], 0xC00);
        setup.tick(action(), 200);
        let mut clips = Vec::new();
        for _ in 0..200 {
            setup.tick(Input::default(), 200);
            if setup.work.routine[1] != MOVE_STAIRS {
                break;
            }
            if clips.last() != Some(&setup.work.move_no) {
                clips.push(setup.work.move_no);
            }
        }
        assert_eq!(setup.work.routine[1], MOVE_IDLE);
        // These stairs are one flight, with no repeated step.
        assert_eq!(clips, [0, 2, 0]);
        assert!(!setup.work.climbing());
    }

    #[test]
    fn only_a_handgun_raises_into_the_stance() {
        let mut setup = armed(true, false);
        setup.weapon.id = 0x10;
        setup.tick(aiming(false, false), 200);
        assert_eq!(setup.work.routine[1], MOVE_IDLE);

        let mut setup = armed(true, false);
        setup.tick(aiming(false, false), 200);
        assert_eq!(setup.work.routine, [MODE_MOVE, MOVE_AIM, 0, 1]);
        assert_eq!(setup.work.move_no, AIM_RAISE_CLIP);
        assert_eq!(setup.work.bank, Bank::Weapon);
    }

    #[test]
    fn the_stance_tilts_fires_once_a_press_and_lowers_on_release() {
        let mut setup = armed(true, false);
        for _ in 0..6 {
            setup.tick(aiming(false, false), 200);
        }
        assert_eq!(setup.work.routine[2], 1);
        assert_eq!(setup.work.move_no, AIM_HOLD_CLIPS[1]);

        setup.tick(
            Input {
                up: true,
                ..aiming(false, false)
            },
            200,
        );
        setup.tick(
            Input {
                up: true,
                ..aiming(false, false)
            },
            200,
        );
        assert_eq!(setup.work.move_no, AIM_HOLD_CLIPS[2]);
        setup.tick(aiming(false, false), 200);
        setup.tick(aiming(false, false), 200);
        assert_eq!(setup.work.move_no, AIM_HOLD_CLIPS[1]);

        setup.tick(aiming(true, true), 200);
        assert_eq!(setup.work.routine[2], 2);
        assert!(!setup.work.fired);
        setup.tick(aiming(true, false), 200);
        assert_eq!(setup.work.move_no, AIM_FIRE_CLIPS[1]);
        assert!(setup.work.fired);

        setup.work.routine = [MODE_MOVE, MOVE_AIM, 1, 1];
        setup.tick(Input::default(), 200);
        assert_eq!(setup.work.routine[2], 3);
        for _ in 0..5 {
            setup.tick(Input::default(), 200);
        }
        assert_eq!(setup.work.routine[1], MOVE_IDLE);
    }

    #[test]
    fn an_empty_handgun_reloads_only_on_a_press_with_ammunition() {
        let mut setup = armed(false, false);
        setup.work.routine = [MODE_MOVE, MOVE_AIM, 1, 1];
        setup.work.status_flg = TILT_LEVEL;
        setup.tick(aiming(true, true), 200);
        assert_eq!(setup.work.routine[2], 1, "nothing to reload with");

        let mut setup = armed(false, true);
        setup.work.routine = [MODE_MOVE, MOVE_AIM, 1, 1];
        setup.work.status_flg = TILT_LEVEL;
        setup.tick(aiming(true, false), 200);
        assert_eq!(setup.work.routine[2], 1, "a held button doesn't reload");
        setup.tick(aiming(true, true), 200);
        assert_eq!(setup.work.routine[2], 4);
        // The frame marked for it fills the magazine.
        setup.clips[usize::from(AIM_RELOAD_CLIP)].frames[1].flags = FRAME_RELOAD;
        setup.tick(aiming(false, false), 200);
        assert!(!setup.work.reloaded);
        setup.tick(aiming(false, false), 200);
        assert!(setup.work.reloaded);
    }

    #[test]
    fn rot_matrix_turns_in_the_original_order() {
        assert_eq!(
            rot_matrix([0, 0x400, 0]),
            [[0, 0, 0x1000], [0, 0x1000, 0], [-0x1000, 0, 0]]
        );
        assert_eq!(
            rot_matrix([0x400, 0, 0x400]),
            [[0, -0x1000, 0], [0, 0, -0x1000], [0x1000, 0, 0]]
        );
    }

    #[test]
    fn a_composed_matrix_floors_its_translation() {
        let parent = Matrix {
            m: rot_matrix([0, 0x400, 0]),
            t: [10, 20, 30],
        };
        let child = Matrix {
            m: rot_matrix([0, 0, 0]),
            t: [1, 2, 3],
        };
        assert_eq!(parent.comp(&child).t, [13, 22, 29]);
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
            skeleton: &setup.skeleton,
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
            weapon: Weapon::default(),
            necks: Default::default(),
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
    fn a_character_s_speeds_and_turns_come_by_its_id() {
        const KENDO: u8 = 0x48;
        assert_eq!(by_id(KENDO, &SCE_WALK_SPEED, &SPL_WALK_SPEED), 60);
        assert_eq!(by_id(KENDO, &SCE_RUN_SPEED, &SPL_RUN_SPEED), 160);
        assert_eq!(by_id(KENDO, &SCE_BACK_SPEED, &SPL_BACK_SPEED), 55);
        assert_eq!(by_id(KENDO, &SCE_BACK_RUN_SPEED, &SPL_BACK_RUN_SPEED), 40);
        assert_eq!(by_id(1, &SCE_RUN_SPEED, &SPL_RUN_SPEED), SCE_RUN_SPEED[1]);
        assert_eq!(by_id(0x20, &SCE_RUN_SPEED, &SPL_RUN_SPEED), 0);

        let mut work = Work::default();
        plc_dest(&mut work, KENDO, 9, 33, 0, 0);
        assert_eq!(work.sce_free[0], 88);
        plc_dest(&mut work, KENDO, 5, 33, 0, 0);
        assert_eq!(work.sce_free[0], 60);
        plc_dest(&mut work, KENDO, 8, 33, 0, 0);
        assert_eq!(work.sce_free[0], 40);
    }

    #[test]
    fn a_character_the_room_places_starts_on_its_clip() {
        let mut work = Work::default();
        // The room's `sce_em_set` said 0x10A, flags 4: routine 0, clip 10.
        spl_start(&mut work, 0x10A, 4);
        assert_eq!(work.routine, [MODE_SCENARIO, 0, 0, 0]);
        assert_eq!(work.move_no, 10);
        assert_eq!(work.sce_flg, 4);
        spl_start(&mut work, 0x207, 0);
        assert_eq!(work.routine[1], 1);
        assert_eq!(work.move_no, 7);
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
