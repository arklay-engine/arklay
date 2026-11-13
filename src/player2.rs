//! RE2's player machine: `Player_move` in bio2's 1.10 PC executable.
//!
//! The port keeps the original's structure and names. Every tick
//! `routine[0]` picks a mode: 1 is the pad (`pl_move`, whose `pl_br_*` read the
//! keys and whose `pl_mv_*` move), and 4 is a script's (`Spl_Scenario`, driven
//! by `plc_motion` and `plc_dest`). Speeds aren't constants: `Spd_get` reads
//! them from the root motion of the frame about to play, which is why a walk
//! changes speed through its stride.
//!
//! What isn't ported is in `docs/internals/deviations/re2.md`.

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
/// `Pl_dead`.
const MODE_DEAD_BODY: u8 = 7;
pub(crate) const MODE_SCENARIO: u8 = 4;
/// `Pl_Em_damage` and `Pl_Em_die`: an enemy has hold of the player.
pub(crate) const MODE_HELD: u8 = 5;
pub(crate) const MODE_HELD_DIE: u8 = 6;
/// `Be_flg`'s 8, which keeps `Sca_ck_em` from running.
pub(crate) const BE_NO_SCA: u32 = 8;
/// `Be_flg`'s 4: `Oba_ck_hit` doesn't push this body out of the others (the
/// climb and the jump down).
pub(crate) const BE_NO_PUSH: u32 = 4;
/// `routine[1]` under [`MODE_MOVE`]: the `pl_br_*` and `pl_mv_*` tables.
const MOVE_IDLE: u8 = 0;
const MOVE_FORWARD: u8 = 1;
const MOVE_RUN: u8 = 2;
const MOVE_BACKWARD: u8 = 3;
const MOVE_ROTATE: u8 = 4;
const MOVE_AIM: u8 = 5;
/// `pl_mv06`, the crouching pick-up `Sce_Item` starts.
pub(crate) const MOVE_ITEM: u8 = 6;
const MOVE_STAIRS: u8 = 7;
const MOVE_CLIMB: u8 = 8;
const MOVE_JUMP_DOWN: u8 = 9;
/// `pl_mv_0a`: pushing a room object.
const MOVE_PUSH: u8 = 10;
/// `pl_mv_0b`, which raises `fg_status`'s `0x40` too ([`Work::holds_game`]).
const MOVE_0B: u8 = 11;
/// The port's optional quick turn, past the original's table.
const MOVE_QUICK_TURN: u8 = 0x40;
/// `Spl_Init`'s `Joint_move` late step.
const SPL_INIT_STEP: u16 = 0x100;
/// `Plc_stop`'s routine, which no table entry animates.
const STOPPED_ROUTINE: [u8; 4] = [MODE_SCENARIO, 0x14, 0, 0];
/// A frame word's top bit, which `get_null_pos` passes over.
const FRAME_SKIPPED: u32 = 0x8000;

/// A clip by health (`D_life_u`), `pl_mv_00`..`04`.
const IDLE_CLIPS: [u8; 3] = [2, 5, 8];
/// `pl_mv06`'s crouch, a clip of the PLD's own.
const ITEM_CLIP: u8 = 6;
const WALK_CLIPS: [u8; 3] = [0, 3, 6];
const RUN_CLIPS: [u8; 3] = [1, 4, 7];
const BACK_CLIPS: [u8; 3] = [0, 9, 9];
const ROTATE_CLIPS: [u8; 3] = [0, 3, 6];
/// A turn per tick by health, `pl_br_01`..`04`.
const FORWARD_TURN: [u16; 3] = [0x28, 0x20, 0x10];
const RUN_TURN: [u16; 3] = [0x38, 0x30, 0x18];
const BACK_TURN: [u16; 3] = [0x28, 0x10, 0x10];
/// `pl_mv03`'s clip that walks back looking at an enemy (`D_life_u` 0 only),
/// how often it looks, and its `FUN_004daa60`'s reach and half field of view.
const BACK_LOOK_CLIP: u8 = 1;
const BACK_LOOK_WAIT: u8 = 15;
const BACK_LOOK_REACH: u32 = 5000;
const BACK_LOOK_VIEW: u16 = 800;
/// The plain back walk takes over on a multiple of this frame.
const BACK_LOOK_FRAME_STEP: u8 = 7;
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
/// `Sce_flg`'s bits that pin a foot through a script's clip, and the part:
/// `FUN_004b2b00` composes parts 0, 1, 2, 3, 4 or 0, 1, 5, 6, 7.
const FEET: [(u16, usize); 2] = [(1, 4), (2, 7)];
/// The first of `pRoom`'s climbing clips, going up and down: a first step,
/// the step repeated, and the last.
const STAIRS_CLIPS: [u8; 2] = [0, 3];
const STAIRS_TURN: i32 = 0x50;
const STAIRS_TURN_CLOSE: i32 = 0x28;
/// The walk off the stairs lasts until this frame.
const STAIRS_WALK_OFF_FRAME: u8 = 9;
const STAIRS_FIRST_BLEND: u8 = 3;
const STAIRS_FIRST_STEP: u16 = 0x400;
/// How far the shadow leads the body on the stairs, and how fast it gets
/// there and comes back.
const STAIRS_KAGE_LEAD: i16 = 200;
const STAIRS_KAGE_STEP: i16 = 0x14;
const STAIRS_KAGE_BACK: i16 = 0x32;
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
/// `pl_mv_0a`'s clips of the player's own: the brace, played forward to
/// start and backward to end, and the push.
const PUSH_BRACE_CLIP: u8 = 7;
const PUSH_CLIP: u8 = 8;
/// The room bank's sounds 4, the push's step, and 0, its end
/// (`Snd_se_on(0x2040001)`, `Snd_se_on(0x2000001)`).
const PUSH_SOUND: u32 = 0x204_0001;
const PUSH_END_SOUND: u32 = 0x200_0001;
const JUMP_CLIP: u8 = 7;
const JUMP_FALL_CLIP: u8 = 8;
const LAND_CLIP: u8 = 9;
/// `pl_mv_09`'s `Timer3` once the fall is in water: five splashes, and the
/// top bit keeps it from starting again.
const JUMP_SPLASH: u8 = 0x85;
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
const AIM_LOWER_BLEND: u8 = 0x0F;
/// The parts from the root to the foot the handguns' stance plants.
const AIM_FOOT_CHAIN: [usize; 5] = [0, 1, 5, 6, 7];
/// `Foot_set_pl`'s other leg, for the shotgun's stance.
const AIM_OTHER_FOOT_CHAIN: [usize; 5] = [0, 1, 2, 3, 4];
const AIM_LOWER_STEP: u16 = 0x100;
/// The knife's id, and `pl_mv05_w0_t2`'s parts: the root, then the arm down
/// to the hand that holds it.
const KNIFE: u8 = 1;
const KNIFE_CHAIN: [usize; 4] = [0, 9, 10, 11];
/// The frame past which a slash with the aim let go stops.
const KNIFE_RELEASE_FRAME: u8 = 0x14;
/// The frames between which the blade finds a wall, and the bounce's clip,
/// blend and step (`pl_mv05_w0_t6`).
const KNIFE_WALL_FRAMES: std::ops::RangeInclusive<u8> = 7..=12;
const KNIFE_BOUNCE_CLIP: u8 = 0x10;
const KNIFE_BOUNCE_BLEND: u8 = 3;
const KNIFE_BOUNCE_STEP: u16 = 0x400;
/// `Sca_ck_hit`'s radius and mask for the blade's tip.
const KNIFE_WALL_RADIUS: i32 = 10;
const KNIFE_WALL_MASK: u16 = 0x2080;
/// The weapon bank's slot the bounce plays (`Snd_se_on(0x1000001)`).
const SOUND_KNIFE_WALL: u16 = 0;
/// `Em_ck`'s reach for the knife's raise, and in the rooms with a wider one
/// (`Stage_no` 3 room 0xA and `Stage_no` 6 room 4, by the room's digits).
const KNIFE_REACH: u32 = 5000;
const KNIFE_WIDE_REACH: u32 = 30000;
const KNIFE_WIDE_ROOMS: [(u8, u8); 2] = [(4, 0xA), (7, 4)];
/// The weapon bank's slot of an empty gun's click (`Snd_se_on(0x1010001)`).
const SOUND_EMPTY: u16 = 1;
/// The frame flag `pl_mv05_w1_t4` plays a sound of the weapon bank on; the
/// slot is the frame's [`FRAME_SOUND_SLOT`] bits.
const FRAME_SOUND: u32 = 0x10_0000;
const FRAME_SOUND_SLOT: u32 = 0xf_0000;
/// The frame flag `pl_mv05_w1_t4` fills the magazine on.
const FRAME_RELOAD: u32 = 0x40_0000;
/// `pNow_seq`: the hand lets the magazine go.
const FRAME_MAGAZINE: u32 = 0x20_0000;
/// The blend step of `Pl_damage`'s `Joint_move`s.
const DAMAGE_BLEND_STEP: u16 = 0x400;
/// The blend `Pl_damage` starts its clips with (`0x30000` in `Move_no`'s word).
const DAMAGE_BLEND: u8 = 3;
/// `Pl_damage`'s clips, all in the character's own bank.
const DAMAGE_CLIP: u8 = 5;
const STAGGER_CLIPS: [u8; 2] = [3, 4];
const FALL_CLIP: u8 = 2;
/// How long `Pl_dead` grows the shadow.
const DEAD_POOL_TICKS: u16 = 0x3C;
/// How far `Pl_damage` knocks the player in a stagger.
const STAGGER_SPEED: i16 = 200;
/// `Damage_cnt`'s top bit: the player can't be damaged. The rest counts down.
const DAMAGE_LOCK: u8 = 0x80;
/// `Status_flg`'s bit: damage just ended, so the next area hit is forgiven.
const STATUS_RECOVERED: u16 = 2;
/// The EDD frame bits: a footfall, and that it's the second kind of floor.
pub const FRAME_FOOTSTEP: u32 = 0x4000;
/// A walking step's noise in `0x989EEE`, which listening enemies hear.
const NOISE_STEP: u8 = 2;
/// A running step's noise: the run, the stairs and the climb.
const NOISE_RUN: u8 = 4;
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
    /// `pSub1_kan_t_ptr`, which an enemy's grab points at its own model's.
    Held,
}

/// `J_seq2`'s part matrices for a step to `target`, with `Hokan` as it is
/// before the step: `RotMatrix` of the frame's angles, then with no blend left
/// `NormalMatrix_2`, and in a blend `HMatrix` from the last matrices,
/// `0xfff - late * Hokan` of the new, then `NormalMatrix_0`. Nothing for a
/// body that plays its clips through `J_seq`.
fn jseq2_step(work: &mut Work, target: Option<&crate::model::Keyframe>, late: u16) {
    let kept = i32::from(late).wrapping_mul(i32::from(work.hokan));
    let blend = work.hokan != 0;
    let (Some(parts), Some(target)) = (work.jseq2.as_mut(), target) else {
        return;
    };
    let fresh = parts.is_empty();
    parts.resize(target.rotations.len(), [[0; 3]; 3]);
    for (part, rotation) in parts.iter_mut().zip(&target.rotations) {
        // A body with no matrices yet eases from the frame itself.
        let last = if fresh { rot_matrix(*rotation) } else { *part };
        *part = jseq2_matrix(last, *rotation, blend.then_some(kept));
    }
}

/// One part's `J_seq2` matrix from `last`: `RotMatrix(rotation)` made unit
/// (`NormalMatrix_2`) with no blend, or with `kept` (`late * Hokan`) left
/// of `last` (`HMatrix`, each term `>> 12` apart) made unit (`NormalMatrix_0`).
pub(crate) fn jseq2_matrix(
    last: [[i16; 3]; 3],
    rotation: [i16; 3],
    kept: Option<i32>,
) -> [[i16; 3]; 3] {
    let new = rot_matrix(rotation);
    let Some(kept) = kept else {
        return normal_matrix(&new, 2);
    };
    let weight = 0xfff_i32.wrapping_sub(kept);
    let mixed: [[i16; 3]; 3] = std::array::from_fn(|row| {
        std::array::from_fn(|column| {
            // The original's `IMUL`s wrap.
            let old = i32::from(last[row][column]).wrapping_mul(0x1000 - weight) >> 12;
            let new = i32::from(new[row][column]).wrapping_mul(weight) >> 12;
            (old as i16).wrapping_add(new as i16)
        })
    });
    normal_matrix(&mixed, 1)
}

/// `NormalMatrix_0` (`0x450BC0`, rows 0 and 1) and `NormalMatrix_2`
/// (`0x450BF0`, rows 0 and 2): `NormalMatrix` (`0x450B00`) makes each row a
/// unit vector on its own: scaled by 1/4096, `1 / sqrt` of its squares' sum
/// (1e-9 for a zero length), times 4096 and that, truncated. The third row is
/// left as it is.
pub(crate) fn normal_matrix(m: &[[i16; 3]; 3], second: usize) -> [[i16; 3]; 3] {
    let mut out = *m;
    for index in [0, second] {
        let row = &mut out[index];
        let [x, y, z] = row.map(|v| f64::from(v) * (1.0 / 4096.0));
        let mut length = (z * z + y * y + x * x).sqrt();
        if length == 0.0 {
            length = 1e-9;
        }
        let inverse = 1.0 / length;
        *row = [x, y, z].map(|v| (v * 4096.0 * inverse) as i32 as i16);
    }
    out
}

/// The player's part of `PLAYER_WORK`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Work {
    /// `Routine_0`..`3`.
    pub routine: [u8; 4],
    /// `Floor` (`+0x106`), the floor `Sca_ck_em` checks against. Only the
    /// stairs, slopes, climbs and drops change it, so a script that moves the
    /// player to another height leaves it behind.
    pub floor: u8,
    /// The joints hold a pose of play ([`seed_pose`]); a door keeps it.
    pub posed: bool,
    /// [`pl_init`] ran and the engine hasn't posed its frame yet ([`pose_init`]).
    pub init_pose: bool,
    /// The routines hold the first tick of `pl_mv_00` that a room started on
    /// its own assumes ([`Work::default`]), and no tick has run them yet.
    pub first_tick_assumed: bool,
    /// `fg_status`'s `0x40`. The aim (`pl_mv_05`), a hit (`Pl_damage`), the
    /// death (`Pl_die`), `pl_mv_0b` and an enemy's hold (`Pl_Em_damage`,
    /// `Pl_Em_die`) raise it and leave it to the next routine's case 0, so it
    /// outlasts each by a tick. The pick-up (`pl_mv_06`), the stairs
    /// (`pl_mv_07`) and the climb (`pl_mv_08`) raise it on their first tick and
    /// drop it themselves on their last. While it is up START doesn't open the
    /// status screen (`Game_loop`, `0x4C02C7`).
    pub holds_game: bool,
    /// `Water` (`+0x10c`): the level of the water the player stands in, or 0.
    /// `Sce_at` clears it each tick after `Player_move` and a water area
    /// (`Sce_Water`, `aot_set` kind 7) sets it again, so a footfall reads the
    /// previous tick's.
    pub water: i16,
    /// `At_sce_no` (`+0xB`, script member 9): the last area `Sce_at_check`
    /// found the player in, `0xFF` once `Sce_work_clr_at` has cleared it.
    pub at_sce: u8,
    /// `Super_matrix`, the surface a body walks on when it isn't the floor.
    pub super_matrix: Option<[[i16; 3]; 3]>,
    /// `Cdir_x` and `Cdir_z`, which only the giant spider on a wall or the
    /// ceiling turns its body by.
    pub(crate) tilt: [i16; 2],
    pub move_no: u8,
    pub move_cnt: u8,
    pub hokan: u8,
    pub bank: Bank,
    pub d_life_u: u8,
    pub spd: [i16; 3],
    pub(crate) spd_base: [i16; 3],
    timer0: u16,
    /// `Sca_info`'s 1: whether `Sca_ck_em` pushed the player out of a wall
    /// on the last tick, which `pl_dm02`'s slide reads.
    pub sca_hit: bool,
    /// `Timer1` and `Timer2`: the climb's steps left and whether it goes down.
    timer1: i16,
    timer2: u8,
    /// `pl_mv03`'s `+0x16A` and `+0x16B`: whether the back walk plays its clip
    /// that looks at an enemy, and the ticks until it looks again.
    back_look: bool,
    back_wait: u8,
    /// `pl_mv06` reached sub 2 this tick: the get-item window opens
    /// (`0x4DB035`). The game takes it.
    pub item_window: bool,
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
    /// Part 0's `Workm`, as the last tick left it, for what an enemy has handed over.
    pub(crate) root: Option<Matrix>,
    /// Every part's `Workm`, as the last tick left it, at the same point as
    /// `root`: where a cockroach (`Em_29`) aims at an attach point.
    pub(crate) parts: Vec<Option<Matrix>>,
    /// Where the body stood when `parts` were composed, so they can follow
    /// what moves it before the draw ([`move_parts`]).
    parts_at: Option<[i32; 3]>,
    /// `Kage_ofs`: how far along the body `Game_loop` probes the shadow's ground.
    kage_ofs: i16,
    /// The point `pl_mv_07` aims `pKage_work->pPos` at in place of part 0.
    kage_pos: Option<[i32; 3]>,
    /// Where `Game_loop`'s `Sca_ck_hit` finds the shadow's ground this tick:
    /// `pPos` before this tick's `Joint_trans2`, `Kage_ofs` along `Cdir_y`.
    pub kage_probe: Option<[i32; 3]>,
    /// `Damage_cnt`, and `Status_flg`, for `Pl_damage`, `Sce_Damage` and the
    /// enemies: its top bit is also an enemy having hold of them.
    pub(crate) damage_cnt: u8,
    pub(crate) status_flg: u16,
    /// `FUN_004f6800`'s blood this tick, whose `Rnd` the caller holds: whether
    /// it's the spray, and where.
    pub(crate) sce_blood: Option<(bool, [i16; 3])>,
    /// The part `Mag_set` drops.
    pub(crate) magazine: Option<Magazine>,
    /// The bowgun's bolts, while it's held (`Plw_init_tbl` sets them up).
    pub quiver: Option<crate::bow2::Quiver>,
    /// A round was spent this tick (`Tama_decrease_i`), for the inventory.
    pub fired: bool,
    /// The rounds a burst's `Tama_decrease_i` took this tick, besides
    /// [`Work::fired`]'s.
    pub spent: u8,
    /// `WORD_00691f8c`: the burst weapons' calls of `Tama_decrease_i` since
    /// one last took rounds. Only the new game clears it.
    pub tama_count: i16,
    /// `Attw_seq_no` and `Attw_timer`: where a burst is in its
    /// [`Weapon::burst_sequence`].
    attw: (u8, u8),
    /// The magazine was filled this tick (`Tama_reload`), for the inventory.
    pub reloaded: bool,
    /// The head's `Rot_neck` state.
    pub neck: Neck,
    /// The parts whose `Be_flg` `parts_set` cleared, by bit: they aren't drawn.
    pub hidden_parts: u32,
    /// Each part's `Poly_rgb`, `0x00BBGGRR`: the colour its polygons are
    /// lit by, `0x80` a channel at full light. `PartsWork_link` (0x4C12B0)
    /// starts every part at `0x808080`; `parts_set` cases 0 and 1 write it.
    pub part_rgb: [u32; 32],
    /// Parts placed under their parent other than the skeleton says: the
    /// `Pos` that `Sa_parts_allmv` blends for a morphing part's children.
    pub joint_offsets: Vec<(u8, [i16; 3])>,
    /// `Parts0_pos_y`: the scale of the first part's height, `0x1000` being 1.
    parts0_y: i32,
    /// How far a hit has the body leaning: the extra turn `Em10Dm00` gives the
    /// first part, and takes from the second.
    pub lean: i16,
    /// The lean's turn about y: `FUN_00457e60`'s first tick leans by the
    /// vector its `Esp_call` left (`vy` 300), as `RotMatrix` of both.
    pub lean_yaw: i16,
    /// `J_seq2`'s part matrices (the licker's `Joint_move2`), when the body
    /// plays its clips that way: each part's `M` is the last one eased toward
    /// the frame's `RotMatrix` and made unit again, not the matrix of eased
    /// angles. `None` for a body that plays them through `J_seq`.
    pub jseq2: Option<Vec<[[i16; 3]; 3]>>,
    /// `pEnemy_ptr`: the enemy slot the aim turns to.
    pub aim_target: Option<usize>,
    /// `Timer3` of the raise: it has been aimed again already.
    aimed_again: bool,
    /// `Key_trg`, as this tick's `Pad_set` left it ([`Tick::key_trg`]).
    key_trg: u16,
    /// `fg_status`'s `0x200000`: a `pl_br_*` saw the action key go down with
    /// no stairs or climb to take it, so this tick's `Sce_at` checks the
    /// action areas (`Sce_at_check(pPl, 1, 0x10)`). `Psx_main` clears it at
    /// the frame's end.
    pub check: bool,
    /// `Be_flg`'s bits that `Oba_ck_hit` reads.
    pub be_flg: u32,
    /// `DAT_0098a0a4`: the slot of the enemy that has hold of the player.
    pub held_by: Option<usize>,
    /// `fg_system`'s `0x100` when no hold of `MODE_HELD_DIE` set it: a bite that
    /// killed outright.
    pub held_kill: bool,
    /// A dog's bite took the last life, but `fg_system`'s `0x4000000`, which
    /// starts `Die`, waits until the dog has the player down.
    pub die_held_back: bool,
    /// The angle an enemy turned the player to this tick.
    pub turn_to: Option<u16>,
    /// Where an enemy's `Approach` moved the player this tick, `x` and `z`.
    pub move_to: Option<[i32; 2]>,
    /// `+0x164`, which `Null_pos_set` adds a clip's movement to.
    pub base: [i16; 3],
    /// `Timer0`, `Timer1` and `Timer2` of the held states.
    pub timers: [u16; 3],
    /// `At_em_flg`: the enemy slots `Oba_ck_em2` found the player touching,
    /// a bit each.
    pub at_em_flg: u32,
    /// `At_em_flg`'s `0x4000`: `Oba_ck_em2` found the player touching the
    /// body in `splayer_work`, which the enemy slots' shove leaves alone.
    pub at_em_partner: bool,
    /// `Atd[0]`, as the last `Oba_ck_em2` left it: the box the bodies push
    /// against, which does not follow a hit that moves the player afterwards.
    pub atd: Option<crate::zombie2::Atd>,
    /// Where the player stood before `Player_move` this tick, for `Old_x2`,
    /// `Old_y2` and `Old_z2`.
    pub old: Option<[i32; 3]>,
    /// A character's `Old_x2`, `Old_y2` and `Old_z2`: its `Atd[0]` box as
    /// `Game_loop`'s draw left it last tick, which `Om_move` reads.
    pub old_box: Option<[i32; 3]>,
    /// `pOn_om` (`+0x1E4`): the room object the player stands on, or is
    /// climbing onto. `Set_room` clears it.
    pub on_object: Option<u8>,
    /// `+0x154`'s `0x1000`: `Pl_life_down` has already let the player live once.
    pub near_death: bool,
    /// `poison_status`: an ivy's spit poisoned the player. The life level
    /// and [`poison_tick`] read it; the screen's tint isn't ported.
    pub poisoned: bool,
    /// `poison_timer`: the ticks to the next life poison takes.
    pub poison_timer: u8,
    /// `Snd_se_on`s of the held states, for the caller.
    pub sounds: Vec<u32>,
    /// What `Weapon_at_ck` found this tick, for the enemy to take.
    pub hit: Option<crate::weapon2::Hit>,
    /// `WORD_00989eee`'s low bits: what the player did this tick, for enemies to
    /// hear. A shot is 1, a step 2, a running step 4, and aiming 0x18.
    pub noise: u8,
    /// `Esp_call`s this tick made, in order.
    pub effects: Vec<EffectCall>,
    /// The `Rnd` draws the aim's splashes (`Pl_water` and the hands') still
    /// owe their `effects`, made by [`Work::water_draws`] after the move.
    water_draws: Vec<WaterDraw>,
    /// `Be_flg`'s level of the shot, and a bit for each tick left of it that
    /// looks for a body.
    shot_level: usize,
    shot_ticks: u8,
    /// The gun's line, for `Shoot_camera` to look along.
    pub shot_line: Option<crate::weapon2::ShotLine>,
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
        // `Em22_Init`'s, and `Em_24`'s copy.
        0x22 | 0x24 => (0x44C, 0x352),
        // `MrX_init`'s.
        0x2A => (0x226, 0x2EE),
        0x45 | 0x4F => (0x172, 0x1CC),
        0x4D => (500, 600),
        id if id & 1 != 0 => (0x1A4, 0x208),
        _ => (500, 600),
    }
}

fn sca_records<'a>(tick: &Tick<'a>) -> &'a [ScaRect] {
    tick.room
        .sca
        .as_ref()
        .map_or(&[], |sca| sca.records.as_slice())
}

/// An `Esp_call` for the tick's owner to make, at a point worked out already.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectCall {
    pub kind: u8,
    pub row: u8,
    pub size: u16,
    pub dir: i16,
    pub at: [i16; 3],
    /// What the caller writes over the slot's first block once it's made.
    pub motion: Option<crate::esp2::Launch>,
    /// The part's `Workm` and the vector on it `Esp_call` was given, for an
    /// effect that follows the part ([`crate::esp2::Effects::call_on_part`]).
    pub(crate) on: Option<(u8, Matrix, [i16; 3])>,
    /// `Esp_call3D`'s turn of what's drawn (`+0x44`), for the slot and those
    /// that die with it.
    pub turn: Option<[i16; 3]>,
}

/// `Type & 0xFFF` of the grenade launcher with each of its rounds.
const GRENADES: u8 = 9;
/// The Colt S.A.A. and the Beretta, which `pl_mv05_w1` fires as it does the
/// handguns.
pub const COLT: u8 = 0xD;
pub const BERETTA: u8 = 0x13;
/// The sub-machine gun and the flamethrower, which `pl_mv05_w2` fires for as
/// long as the trigger is held.
pub const SMG: u8 = 0xF;
pub const FLAMETHROWER: u8 = 0x10;
/// `pl_mv05_w2_t1`'s hold, `pl_mv05_w2_t2`'s burst and the end of it, by tilt
/// (down, level, up), and `pl_mv05_w2_t4`'s reload.
const BURST_HOLD_CLIPS: [u8; 3] = [0x12, 0x0C, 0x0F];
const BURST_CLIPS: [u8; 3] = [0x10, 0x0A, 0x0D];
const BURST_END_CLIPS: [u8; 3] = [0x11, 0x0B, 0x0E];
const BURST_RELOAD_CLIP: u8 = 0x13;
/// `pl_mv05_w2_t2`'s turn a tick.
const BURST_TURN: u16 = 0x20;
/// `pl_w0f_t2_move`'s flash and smoke on every third frame, and its case on
/// every sixth, along the body's facing turned by the second.
const SMG_FLASH: [u32; 2] = [0x101_1000, 0x30C_1000];
const SMG_CASE: (u32, i16, [i16; 3]) = (0x201_0900, 0x4B0, [0xAA, 0x118, 0x50]);
/// `pl_w10_t2_move`'s flame on every third frame, along the body's facing.
const FLAME: (u32, [i16; 3]) = (0x31D_1200, [0x96, 0x4B0, 0]);
/// The flamethrower's sounds: the burst's first frame and its eleventh, and a
/// change of tilt.
const SOUND_FLAME_START: u16 = 0;
const SOUND_FLAME_GO_ON: u16 = 0xB;
const SOUND_FLAME_TILT: u16 = 0xD;
/// `FUN_004d6560`'s last call, `Esp_call3D` 0x1073000 0x38E up the hand,
/// turned a quarter about z.
const COLT_SMOKE_3D: (u32, [i32; 3], [i16; 3]) = (0x107_3000, [0x82, 0x38E, 0], [0, 0, 0x400]);
/// The reload's `Esp_call(0x2070800)` on the hand, which throws the Colt's
/// cases out where the others let their magazine go.
const COLT_CASES: (u32, [i16; 3]) = (0x207_0800, [0x64, 0xDC, 0]);
const ACID_ROUNDS: u8 = 0xB;
/// `FUN_004d6150`'s and `FUN_004d6460`'s point on the hand.
const GRENADE_MUZZLE: [i32; 3] = [0x78, 0x4B0, 0];

/// A splash in [`Work::effects`] whose `Rnd` draws are still to be made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WaterDraw {
    effect: usize,
    /// `Pl_water`'s ring, 0 to 3, or `None` for a hand's splash.
    ring: Option<u16>,
}

/// One tick of `Weapon_at_ck`'s walk of `sequence` from `Attw_seq_no` and
/// `Attw_timer`: where it is after, and whether it looks for a body.
fn attw_step(sequence: &[(u8, u8)], (mut index, mut timer): (u8, u8)) -> ((u8, u8), bool) {
    let at = |index: u8| {
        sequence
            .get(usize::from(index))
            .copied()
            .unwrap_or((0xFF, 0xFF))
    };
    if timer == 0 {
        index = index.wrapping_add(1);
        timer = at(index).1;
    }
    let mut record = at(index).0;
    if record == 0xFF {
        return ((index, timer.wrapping_sub(1)), false);
    }
    if record == 0xFE {
        index = 0;
        timer = at(0).1;
        record = at(0).0;
        if record == 0xFF {
            return ((index, timer), false);
        }
    }
    if timer == 0xFF {
        return ((index, timer), false);
    }
    ((index, timer - 1), true)
}

/// `pl_mv05_w1_t0`'s `Goto00` to who it's aiming at, and the Colt's quicker
/// one.
const AIM_ENEMY_TURN: i32 = 0xD8;
const COLT_ENEMY_TURN: i32 = 0x120;
/// The part `pl_mv05_w1_t2` gives `hit_scan_ck`: the gun's hand.
const HAND_PART: usize = 14;
/// The part `pl_w02_t2_move` works from.
const SHOT_PART: usize = 11;
/// [`Mover::drawn_parts`]: parts 0, 11 and 14.
type DrawnParts = [Option<Matrix>; 3];

/// `Neck_no`: the head's joint, the same for every player.
pub const NECK_JOINT: usize = 8;
/// `FUN_0045f5b0`: the dog turns two joints, and leaves `Neck_no` on the first.
const DOG_NECK: [usize; 2] = [2, 3];
/// `Pl_init`'s turn rates and limits of [`NECK_JOINT`], yaw then pitch.
const NECK_RATES: [i32; 2] = [0x60, 0x60];
/// The alligator's jaws, which [`Neck::jaw`] rolls.
const ALLIGATOR_JAWS: [usize; 2] = [6, 7];
const NECK_LIMITS: [i32; 2] = [0x200, 0x138];
/// What `Pl_neck` counts down from.
const NECK_WAIT: u8 = 0x2D;
/// Its count when play begins, three `Player_move`s after the static's `0x2D`.
const NECK_WAIT_AT_START: u8 = NECK_WAIT - 3;
/// `Player_move`'s `Pl_neck`: how far it looks, and half its field of view.
const NECK_REACH: u32 = 7000;
const NECK_VIEW: u16 = 0x5DC;
/// `Sca_ck_line`'s mask between `Pl_neck`'s two heads.
const NECK_LINE_MASK: u16 = 0x2080;
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
    /// `+0x1B8`: whose head, in [`Necks`], `Pl_neck` has it look at. `None` is
    /// its own, so it follows its pose.
    pub target: Option<usize>,
    /// The second joint's yaw, when the neck has two (`DOG_NECK`).
    pub turn_two: Option<i16>,
    /// The alligator's jaws (`Em_23`'s `+0x22a` and `+0x228`): the roll of
    /// parts 6 and 7 on top of their pose.
    pub jaw: [i16; 2],
    /// `BYTE_0053a7d8`: the ticks until `Pl_neck` looks again. It's a static,
    /// so a new room doesn't restart it.
    pub wait: u8,
    /// `Neck_no` when it isn't [`NECK_JOINT`]: the joint the yaw turns.
    pub joint: Option<u8>,
    /// Where the player's body was when `position` was last placed, so the
    /// head can follow the tick's pushes ([`follow_pushes`]).
    pub body: Option<[i32; 3]>,
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

    /// The draw's locals from the work: `J_seq2`'s matrices for a body that
    /// plays through it (the first `len()` parts, with y flipped as the draw
    /// has them), then the lean.
    pub fn pose_locals(&self, local: &mut [crate::anim::Mat4x3]) {
        if let Some(parts) = &self.jseq2 {
            for (joint, m) in local.iter_mut().zip(parts) {
                joint.r = flip_y(m.map(|row| row.map(i32::from)));
            }
        }
        self.lean_locals(local);
    }

    /// `Em10Dm00`'s lean on the joints' matrices on their parents', as
    /// `lean` turns the pose's `Workm`: what the draw composes. The draw's
    /// matrices turn a flipped Y (`anim::rotation_matrix`), so each is
    /// flipped to the game's turn for the lean and back.
    pub fn lean_locals(&self, local: &mut [crate::anim::Mat4x3]) {
        let mut parts: Vec<Option<Matrix>> = local
            .iter()
            .take(2)
            .map(|joint| {
                Some(Matrix {
                    m: flip_y(joint.r).map(|row| row.map(|cell| cell as i16)),
                    t: joint.t,
                })
            })
            .collect();
        lean(&mut parts, self.lean, self.lean_yaw);
        for (joint, part) in local.iter_mut().zip(parts.into_iter().flatten()) {
            joint.r = flip_y(part.m.map(|row| row.map(i32::from)));
        }
    }

    fn scaled_height(&self, height: i16) -> i16 {
        ((i32::from(height) * self.parts0_y) >> 12) as i16
    }
}

impl Neck {
    /// Add the turn to `pose`, as `Rot_neck` does around its `RotMatrix`.
    pub fn apply(&self, pose: &mut Keyframe) {
        if self.jaw != [0, 0] {
            for (joint, roll) in ALLIGATOR_JAWS.into_iter().zip(self.jaw) {
                if let Some(rotation) = pose.rotations.get_mut(joint) {
                    rotation[2] = rotation[2].wrapping_add(roll);
                }
            }
        }
        if let Some(turn_two) = self.turn_two {
            for (joint, turn) in DOG_NECK.into_iter().zip([self.turn[0], turn_two]) {
                if let Some(rotation) = pose.rotations.get_mut(joint) {
                    rotation[1] = rotation[1].wrapping_add(turn);
                }
            }
            return;
        }
        if let Some(joint) = self.joint {
            if let Some(rotation) = pose.rotations.get_mut(usize::from(joint)) {
                rotation[1] = rotation[1].wrapping_add(self.turn[0]);
            }
            return;
        }
        if let Some(rotation) = pose.rotations.get_mut(NECK_JOINT) {
            rotation[1] = rotation[1].wrapping_add(self.turn[0]);
            rotation[2] = rotation[2].wrapping_add(self.turn[1]);
        }
    }
}

/// A part's `Poly_rgb` as `PartsWork_link` leaves it: full light.
pub const DEFAULT_PART_RGB: u32 = 0x0080_8080;

/// `Parts_set`'s case 2 on `work`: `Be_flg`, whose first bit draws the part.
pub fn set_part_flags(work: &mut Work, part: u8, flags: u16) {
    parts_set(work, part as i8, 2, flags as i16, false);
}

/// `Parts_set` (0x4E5F60) on `work`: one field of the part's `PARTS_W`.
///
/// - Case 0 writes `Poly_rgb`'s first two bytes (red and green), case 1 its
///   third (blue). In room 702 (`room_702`) the original writes `0x0505` and
///   `5` instead, whatever the script says.
/// - Case 2 writes the low word of `Be_flg`, whose bit 0 has `Joint_trans2`
///   transform and draw the part. The original writes the whole word, which
///   also clears the `0x8`, `0x10`, `0x20` and `0x9000` bits; arklay keeps
///   only bit 0, as [`Work::hidden_parts`].
/// - Case 3 writes `Be_flg`'s high word, which nothing that draws reads
///   (`ko_joint_trans2` takes the low word only), so it has no field here.
/// - Cases 4 to 6 write the part's `Cdir_x/y/z`, which the animated path never
///   reads (`J_seq2` builds the part's matrix from its `Rot_x/y/z`), so they
///   have no field either. No room script uses cases 3 to 6.
///
/// The part is the operand's signed byte: the original writes out of bounds
/// for a negative part or one past the work's parts; here a part outside
/// `0..32` does nothing.
pub fn parts_set(work: &mut Work, part: i8, case: u8, value: i16, room_702: bool) {
    let Ok(index) = usize::try_from(part) else {
        return;
    };
    if index >= 32 {
        return;
    }
    let rgb = &mut work.part_rgb[index];
    match case {
        0 => {
            let word = if room_702 {
                0x0505
            } else {
                u32::from(value as u16)
            };
            *rgb = *rgb & 0x00FF_0000 | word;
        }
        1 => {
            let byte = if room_702 { 5 } else { u32::from(value as u8) };
            *rgb = *rgb & 0x0000_FFFF | byte << 16;
        }
        2 => {
            let bit = 1 << index;
            if value & 1 == 0 {
                work.hidden_parts |= bit;
            } else {
                work.hidden_parts &= !bit;
            }
        }
        _ => {}
    }
}

/// A part's `Poly_rgb` as a tint on arklay's lit colour: the original
/// multiplies the vertex light by each byte `>> 8`, so `0x80` is the colour
/// every part has unless a script changes it, and draws as 255 here.
pub fn part_tint(rgb: u32) -> [u8; 3] {
    let channel = |shift: u32| (((rgb >> shift) & 0xFF) * 2).min(255) as u8;
    [channel(0), channel(8), channel(16)]
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
    let level = crate::sca::square_root0(z.wrapping_mul(z).wrapping_add(x.wrapping_mul(x)));
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

/// The part `Mag_set` starts the magazine under.
const MAGAZINE_HAND: usize = 11;
/// `M.t` of the magazine's part under the hand.
const MAGAZINE_OFFSET: i32 = 150;
/// `BYTE_0053a422`.
const MAGAZINE_SPEED: i32 = 0x1E;
/// `Status_flg`'s bits that go with `pOn_om`.
const STATUS_ON_OBJECT: u16 = 0x0600;
/// `Status_flg`: there's a magazine in the gun to drop.
const STATUS_MAGAZINE: u16 = 1;

/// The magazine a reload drops: `Mag_down` keeps it falling straight down to
/// its first bounce, sends it spinning along the ground after that, and takes
/// it away at its second landing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Magazine {
    /// `Workm`.
    pub(crate) matrix: Matrix,
    /// `+0x3a` and `+0x3c`.
    speed: [i16; 2],
    /// `+0x38`: `Ground` as it left the hand.
    ground: i16,
    /// `+0x84`.
    fall: i16,
    /// `+0x79`.
    bounces: u8,
}

impl Work {
    /// The first tick of `pl_mv_00` chose its clip by the life it began with.
    pub(crate) fn begin_at_life(&mut self, life: i32) {
        let level = life_level(life, false);
        self.d_life_u = level;
        self.move_no = IDLE_CLIPS[usize::from(level)];
    }

    /// `Om_move`'s end with no `pOn_om`: `Status_flg` loses `0x0600`, which
    /// `Set_room` clears with `pOn_om` too.
    pub(crate) fn leave_object(&mut self) {
        self.status_flg &= !STATUS_ON_OBJECT;
    }

    /// `Player_set`: the magazine is gone, and the next reload drops one if
    /// there's a round in the gun.
    pub(crate) fn enter_room(&mut self, loaded: bool) {
        self.status_flg = if loaded { STATUS_MAGAZINE } else { 0 };
        self.magazine = None;
        self.on_object = None;
    }

    /// The aim's splashes' `Rnd`, in the order the original draws it: two for
    /// each of `Pl_water`'s rings (its turn, then its size) and one for each
    /// hand's splash (its size). Nothing else in the move draws, so drawing
    /// after it keeps the order.
    pub(crate) fn water_draws(&mut self, rnd: &mut crate::zombie2::Rnd) {
        for draw in std::mem::take(&mut self.water_draws) {
            let Some(effect) = self.effects.get_mut(draw.effect) else {
                continue;
            };
            match draw.ring {
                Some(ring) => {
                    let turn = u16::from(rnd.draw()) * 4;
                    effect.dir = (effect.dir as u16).wrapping_sub(turn) as i16;
                    let base = if ring < 2 { 0x818 } else { 0xC00 };
                    effect.size = u16::from(rnd.draw()) * 4 + base;
                }
                None => effect.size = u16::from(rnd.draw()) * 4 + 0x60C,
            }
        }
    }

    /// `Mag_down`, and where the magazine bounced this tick, for its sound.
    pub(crate) fn mag_down(&mut self, rnd: &mut crate::zombie2::Rnd) -> Option<[i32; 3]> {
        let magazine = self.magazine.as_mut()?;
        magazine.fall = magazine.fall.wrapping_add(0x14);
        magazine.matrix.t[1] += i32::from(magazine.fall);
        let ground = i32::from(magazine.ground);
        let mut bounced = None;
        if ground < magazine.matrix.t[1] {
            if magazine.bounces == 0 {
                self.magazine = None;
                return None;
            }
            magazine.matrix.t[1] = ground;
            magazine.bounces -= 1;
            magazine.fall = -(magazine.fall / 2);
            if rnd.draw() & 1 != 0 {
                magazine.speed = magazine.speed.map(|speed| -speed);
            }
            bounced = Some(magazine.matrix.t);
        }
        if magazine.bounces == 0 {
            magazine.matrix.m = mul_matrix(&rot_matrix([0x100, 0, 0]), &magazine.matrix.m);
            magazine.matrix.t[0] += i32::from(magazine.speed[0]);
            magazine.matrix.t[2] += i32::from(magazine.speed[1]);
        }
        bounced
    }
}

impl Work {
    /// The root of `Joint_trans2`: `Cdir_y`, on the surface it walks on.
    pub(crate) fn body_rotation(&self, angle: u16) -> [[i16; 3]; 3] {
        let [x, z] = self.tilt;
        let turn = rot_matrix([x, angle as i16, z]);
        self.super_matrix
            .map_or(turn, |surface| mul_matrix(&surface, &turn))
    }

    /// `pKage_work->pPos`: part 0's place, unless the stairs moved it.
    pub fn kage_pos(&self) -> Option<[i32; 3]> {
        self.kage_pos.or(self.root.map(|root| root.t))
    }

    /// `Game_loop`'s probe for the shadow's ground, taken before this tick's
    /// part 0 is composed.
    fn latch_kage_probe(&mut self, angle: u16) {
        let turn = rot_matrix([0, angle as i16, 0]);
        let ofs = i32::from(self.kage_ofs);
        self.kage_probe = self.kage_pos().map(|at| {
            [
                at[0] + i32::from(((i32::from(turn[0][0]) * ofs) >> 12) as i16),
                at[1],
                at[2] + i32::from(((i32::from(turn[2][0]) * ofs) >> 12) as i16),
            ]
        });
    }
}

impl Default for Work {
    fn default() -> Self {
        Self {
            // `Pl_init`, then the first tick of `pl_mv_00`.
            shot_level: 0,
            shot_ticks: 0,
            attw: (0, 0),
            spent: 0,
            tama_count: 0,
            shot_line: None,
            super_matrix: None,
            tilt: [0, 0],
            floor: 0,
            posed: false,
            init_pose: false,
            first_tick_assumed: true,
            holds_game: false,
            routine: [MODE_MOVE, MOVE_IDLE, 1, 0],
            move_no: IDLE_CLIPS[0],
            move_cnt: 1,
            hokan: FULL_BLEND - 1,
            bank: Bank::Weapon,
            hidden_parts: 0,
            parts_at: None,
            part_rgb: [DEFAULT_PART_RGB; 32],
            joint_offsets: Vec::new(),
            parts0_y: 0x1000,
            lean: 0,
            lean_yaw: 0,
            jseq2: None,
            aim_target: None,
            aimed_again: false,
            key_trg: 0,
            check: false,
            be_flg: 1,
            held_by: None,
            held_kill: false,
            die_held_back: false,
            turn_to: None,
            move_to: None,
            base: [0; 3],
            timers: [0; 3],
            at_em_flg: 0,
            at_em_partner: false,
            atd: None,
            old: None,
            old_box: None,
            on_object: None,
            near_death: false,
            poisoned: false,
            poison_timer: 0,
            sounds: Vec::new(),
            hit: None,
            noise: 0,
            effects: Vec::new(),
            water_draws: Vec::new(),
            water: 0,
            at_sce: crate::scd2::host::NO_AREA,
            d_life_u: 0,
            spd: [0; 3],
            spd_base: [0; 3],
            timer0: 0,
            sca_hit: false,
            timer1: 0,
            timer2: 0,
            back_look: false,
            back_wait: 0,
            item_window: false,
            now_seq: 0,
            kind: 0,
            sce_flg: 0,
            sce_flg_no: 0,
            sce_free: [0; 2],
            dest: [0; 2],
            stopped: [0; 4],
            root: None,
            parts: Vec::new(),
            kage_ofs: 0,
            kage_pos: None,
            kage_probe: None,
            damage_cnt: 0,
            status_flg: 0,
            sce_blood: None,
            magazine: None,
            quiver: None,
            fired: false,
            reloaded: false,
            neck: Neck {
                wait: NECK_WAIT_AT_START,
                ..Neck::default()
            },
        }
    }
}

impl Work {
    /// `Pl_init`: what a room's first tick of play starts from.
    pub fn entering_room(&self) -> Self {
        Self {
            d_life_u: self.d_life_u,
            kind: self.kind,
            posed: self.posed,
            tama_count: self.tama_count,
            // The head's world position is the joints' last matrix, which a
            // room load leaves alone until the new room is drawn.
            neck: Neck {
                wait: self.neck.wait,
                position: self.neck.position,
                ..Neck::default()
            },
            ..Self::default()
        }
    }

    /// `Pl_init`, which leaves the routines' first tick to the room's first
    /// `Player_move`.
    pub fn after_pl_init(mut self) -> Self {
        self.routine = [MODE_MOVE, MOVE_IDLE, 0, 0];
        self.hokan = 0;
        self.first_tick_assumed = false;
        self
    }

    /// Whether the player is aiming (`pl_mv_05`), which raises `fg_status`'s
    /// `0x80` and so holds the cut on screen (`Re2State::view_held`).
    pub fn holds_view(&self) -> bool {
        self.routine[0] == MODE_MOVE && self.routine[1] == MOVE_AIM
    }

    /// The routines that raise `fg_status`'s `0x40` and leave it up for the
    /// next routine to drop ([`Work::holds_game`]).
    fn holds_game_now(&self) -> bool {
        self.holds_view()
            || matches!(
                self.routine[0],
                MODE_DAMAGE | MODE_DEAD | MODE_HELD | MODE_HELD_DIE
            )
            || (self.routine[0] == MODE_MOVE && self.routine[1] == MOVE_0B)
    }

    /// The routines that raise `fg_status`'s `0x40` and drop it themselves on
    /// their last tick ([`Work::holds_game`]).
    fn holds_game_in_routine(&self) -> bool {
        self.routine[0] == MODE_MOVE
            && matches!(self.routine[1], MOVE_ITEM | MOVE_STAIRS | MOVE_CLIMB)
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
    pub held: (&'a [Clip], &'a [Keyframe]),
}

impl<'a> Banks<'a> {
    pub fn get(&self, bank: Bank) -> (&'a [Clip], &'a [Keyframe]) {
        match bank {
            Bank::Pld => self.pld,
            Bank::Weapon => self.weapon,
            Bank::Room => self.room,
            Bank::Held => self.held,
        }
    }
}

/// One tick's inputs from the rest of the game.
pub struct Tick<'a> {
    pub room: &'a RoomState,
    pub banks: Banks<'a>,
    pub skeleton: &'a Skeleton,
    /// The model's `pSa_dat` morphs, which move a part's children.
    pub morphs: &'a [crate::model::PartMorph],
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
    /// What the shots and the aim see of the enemy slots.
    pub targets: &'a [crate::weapon2::Target],
    /// Who `Pl_neck` can look at, in its order: none while the room has no
    /// enemies.
    pub neck_targets: &'a [NeckTarget],
    /// `BYTE_00989eeb`: the room's enemies so far.
    pub enemies: u8,
    /// `fg_status`'s `0x2000`: last tick's `Om_move` pushed an object.
    pub pushing: bool,
    /// The room's objects (`pOm`), as last tick's `Om_move` left them: the
    /// climb onto one, standing on it, and the ground their tops make.
    pub objects: &'a [crate::objects::ObjectRecord],
    /// `Key_trg`: the keys `Pad_set` saw go down this tick
    /// ([`crate::scd2::host::Re2State::set_key`]). Under an input stop it
    /// keeps only [`KEYS_IN_A_STOP`], and the tick the stop ends raises
    /// nothing, so a key held through the stop is no new press.
    pub key_trg: u16,
}

/// A body `Pl_neck` considers.
#[derive(Debug, Clone, Copy)]
pub struct NeckTarget {
    /// Its place in [`Necks`].
    pub index: usize,
    /// `Type`.
    pub kind: u16,
    pub pos: [i32; 3],
    pub neck: [i32; 3],
    /// `+0x1F0`: how far it was from the player when it last ran.
    pub distance: u32,
    /// `Life` (`+0x156`) and `Cdir_y`.
    pub life: i16,
    pub angle: u16,
}

/// The necks of [`Tick::necks`].
pub type Necks = [Option<[i32; 3]>; NECK_WORKS];
/// `player_work`, `splayer_work` and `enemy_work`'s 32 (room 40B's swarm
/// fills 24).
pub const NECK_WORKS: usize = 34;

/// What the player's `Type` and the inventory say about the held weapon.
#[derive(Debug, Default, Clone, Copy)]
pub struct Weapon {
    /// `Type & 0xFFF`.
    pub id: u8,
    /// `Tama_ck_i`: there is a round in it.
    pub loaded: bool,
    /// `Tama_reload_ck`: the inventory has its ammunition.
    pub spare: bool,
    /// The rounds in it, for the bursts' `Tama_decrease_i`.
    pub rounds: u8,
}

impl Weapon {
    /// Whether [`Mover::pl_mv_aim`] handles it: the knife, the starting
    /// handguns, the magnums, the shotguns, the grenade launcher, the bowgun,
    /// the Colt S.A.A. and the Beretta.
    fn aims(&self) -> bool {
        matches!(
            self.id,
            KNIFE | 2 | 3 | 5..=8 | GRENADES..=crate::bow2::BOW | COLT | SMG | FLAMETHROWER | BERETTA
        )
    }

    /// `pl_mv05_w_tbl`'s `pl_mv05_w2`: the sub-machine gun and the
    /// flamethrower share the handguns' raise, lowering and lock, but hold,
    /// fire and reload on their own clips.
    fn bursts(&self) -> bool {
        matches!(self.id, SMG | FLAMETHROWER)
    }

    /// The hold's clips, by tilt.
    fn hold_clips(&self) -> [u8; 3] {
        if self.bursts() {
            BURST_HOLD_CLIPS
        } else {
            AIM_HOLD_CLIPS
        }
    }

    fn reload_clip(&self) -> u8 {
        if self.bursts() {
            BURST_RELOAD_CLIP
        } else {
            AIM_RELOAD_CLIP
        }
    }

    /// `At_tbl_top`'s sequence for a burst: the record and the ticks it
    /// lasts. A record of 0xFF waits, 0xFE starts again, and a count of 0xFF
    /// ends it. The sub-machine gun looks on every third tick; the
    /// flamethrower's never looks (its flames do).
    fn burst_sequence(&self) -> &'static [(u8, u8)] {
        if self.id == SMG {
            &[(0, 1), (0xFF, 2), (0xFE, 0xFF)]
        } else {
            &[(0, 0xFF)]
        }
    }

    /// `pl_mv05_w1_t2` looks for no body itself with the launchers and the
    /// bowgun: what they fire does.
    fn shoots_effects(&self) -> bool {
        matches!(self.id, GRENADES..=0xC)
    }

    /// `pl_mv05_tbl`'s `pl_mv05_w0` (`0x4d4b20`): the knife shares the
    /// handguns' routines but the attack.
    fn is_knife(&self) -> bool {
        self.id == KNIFE
    }

    /// `DAT_0053A305`'s row of its three, each frame past which something is
    /// allowed: tilting down, tilting up, and stopping a shot. The first two
    /// keep a bit that chooses `Foot_set_pl`'s leg.
    fn frames(&self) -> [u8; 3] {
        match self.id {
            KNIFE => [0x85, 0x85, 0x32],
            5 => [0x85, 0x87, 25],
            6 => [0x85, 0x87, 35],
            7 => [7, 10, 28],
            8 => [0x88, 0x8A, 35],
            GRENADES..=ACID_ROUNDS => [0x8A, 0x8C, 26],
            crate::bow2::BOW => [0x86, 0x87, 10],
            COLT => [0x85, 0x86, 5],
            SMG | FLAMETHROWER => [0x87, 0x88, 0],
            // The handguns' and the Beretta's.
            _ => [0x85, 0x87, 8],
        }
    }

    /// `pl_mv05_w1_t0`'s step of the turn to who it's aiming at.
    fn enemy_turn(&self) -> i32 {
        if self.id == COLT {
            COLT_ENEMY_TURN
        } else {
            AIM_ENEMY_TURN
        }
    }

    fn tilt_down_frame(&self) -> u8 {
        self.frames()[0] & 0x7F
    }

    fn tilt_up_frame(&self) -> u8 {
        self.frames()[1] & 0x7F
    }

    fn fire_cancel_frame(&self) -> u8 {
        self.frames()[2]
    }

    /// `Foot_set_pl`'s chain for this weapon, for a character of id `id`.
    fn foot_chain(&self, id: u8) -> &'static [usize; 5] {
        if self.frames()[usize::from(id & 1)] & 0x80 != 0 {
            &AIM_FOOT_CHAIN
        } else {
            &AIM_OTHER_FOOT_CHAIN
        }
    }

    /// `At_tbl_top`'s sequence: a bit for each tick of a shot, the lowest first,
    /// on which it looks for a body.
    fn shot_ticks(&self) -> u8 {
        match self.id {
            6 => 0b111,
            7 => 0b1_0101,
            8 => 0b101_0101,
            _ => 1,
        }
    }

    /// The hand's point and calls of `pl_wNN_t2_move` on a frame of its clip.
    fn shot_effects(&self, frame: u8) -> &'static [ShotEffect] {
        const CASE: [ShotCall; 3] = [
            (0x308_0900, None),
            (0x308_0900, None),
            (0x200_0900, Some(0x694)),
        ];
        const MAGNUM_CASE: &[ShotEffect] = &[ShotEffect {
            point: [0x14A, 0x32, 0],
            calls: &CASE,
        }];
        const SHOTGUN_CASE: &[ShotEffect] = &[ShotEffect {
            point: [0xB8, 0x22E, -0x50],
            calls: &[(0x308_0D00, None), (0x202_0900, Some(0x514))],
        }];
        const SHOTGUN_SMOKE: &[ShotEffect] = &[ShotEffect {
            point: [0xB8, 0x21A, -0x50],
            calls: &[(0x308_0D00, None)],
        }];
        match (self.id, frame) {
            // `pl_w02_t2_move`, which the Beretta shares.
            (2 | 3 | BERETTA, 1) => &[ShotEffect {
                point: [0x8A, 0x267, 8],
                calls: &[(0x30A_0E00, None), (0x100_0E00, None)],
            }],
            // `FUN_004d6560`: the flash, the smoke and their glow up the
            // barrel, then [`COLT_SMOKE_3D`].
            (COLT, 2) => &[
                ShotEffect {
                    point: [0x82, 0x2DA, 0],
                    calls: &[(0x100_1000, None)],
                },
                ShotEffect {
                    point: [0x82, 0x118, 0],
                    calls: &[
                        (0x104_1000, None),
                        (0x106_1000, Some(0x800)),
                        (0x30A_0C00, None),
                    ],
                },
                ShotEffect {
                    point: [0x82, 0x33E, 0],
                    calls: &[(0x30C_1000, None)],
                },
            ],
            (2 | BERETTA, 3) => &[ShotEffect {
                point: [0x15E, 0x28, -15],
                calls: &[
                    (0x308_0900, None),
                    (0x308_0900, None),
                    (0x200_0700, Some(0x514)),
                ],
            }],
            (3, 3) => &[ShotEffect {
                point: [0xA0, 200, 0x3C],
                calls: &[
                    (0x308_0900, None),
                    (0x308_0900, None),
                    (0x200_0700, Some(0x514)),
                ],
            }],
            (5, 1) => &[ShotEffect {
                point: [0x6E, 0x29E, -10],
                calls: &[(0x30A_1400, None), (0x100_1D00, None)],
            }],
            (6, 1) => &[
                ShotEffect {
                    point: [0x82, 900, -10],
                    calls: &[(0x30A_1800, None), (0x100_2000, None)],
                },
                ShotEffect {
                    point: [0x82, 0x44C, -10],
                    calls: &[(0x102_3000, None)],
                },
            ],
            (5 | 6, 4) => MAGNUM_CASE,
            (7, 1) => &[
                ShotEffect {
                    point: [0x96, 0x514, 0],
                    calls: &[(0x100_1D00, None), (0x104_1000, None), (0x30C_1800, None)],
                },
                ShotEffect {
                    point: [0x96, 0x708, 0],
                    calls: &[(0x30E_1000, None), (0x102_5000, None)],
                },
                ShotEffect {
                    point: [0x96, 0x8FC, 0],
                    calls: &[(0x30E_1D00, None)],
                },
            ],
            (0xA | ACID_ROUNDS, 1) => &[ShotEffect {
                point: GRENADE_MUZZLE,
                calls: &[
                    (0x100_2000, None),
                    (0x20C_1000, Some(0)),
                    (0x308_1200, None),
                ],
            }],
            (7, 0x1A) | (8, 5) => SHOTGUN_CASE,
            (7, 0x1B) | (8, 6) => SHOTGUN_SMOKE,
            (8, 1) => &[
                ShotEffect {
                    point: [0x30C, 0x532, 0],
                    calls: &[
                        (0x100_2900, None),
                        (0x104_1600, None),
                        (0x105_4000, None),
                        (0x30C_1600, None),
                    ],
                },
                ShotEffect {
                    point: [0x3BF, 0x65E, 0],
                    calls: &[(0x30E_0E00, None), (0x104_1800, None)],
                },
                ShotEffect {
                    point: [0x4E5, 0x852, 0],
                    calls: &[(0x30E_1600, None), (0x103_5900, None), (0x104_1A00, None)],
                },
                ShotEffect {
                    point: [0x646, 0xAAA, 0],
                    calls: &[(0x30E_2000, None)],
                },
            ],
            _ => &[],
        }
    }
}

/// One point of a `pl_wNN_t2_move`, and the `Esp_call`s made at it.
struct ShotEffect {
    point: [i32; 3],
    calls: &'static [ShotCall],
}

/// An `Esp_call`'s word of kind, row and size, and what to add to the player's
/// facing for its direction.
type ShotCall = (u32, Option<i16>);

/// The head where the render's joint matrices put it on the pose on screen,
/// with the routines not run: a door's last frame draws the new room.
pub fn place_neck(work: &mut Work, player: &mut PlayerState, tick: &Tick<'_>) {
    let pos = player.pos;
    Mover {
        work,
        player,
        tick,
        arrived: None,
        stairs: None,
    }
    .place_neck();
    work.neck.body = Some(pos);
}

/// The pose on screen before the player's first tick of play: the frame its
/// clip last showed (`Move_cnt - 1`), as the original's joints hold it. A
/// new game's first tick is set up as already run (`Work::default`) with the
/// joints never posed, and the clip blend that starts on the next tick eases
/// out of this pose: the new game's idle frame. Once per player; a door keeps
/// the last room's pose ([`Work::entering_room`]).
pub fn seed_pose(work: &mut Work, player: &mut PlayerState, tick: &Tick<'_>) {
    if std::mem::replace(&mut work.posed, true) || work.move_cnt == 0 {
        return;
    }
    let (clips, keyframes) = tick.banks.get(work.bank);
    let pose = clips
        .get(usize::from(work.move_no))
        .and_then(|clip| clip.frames.get(usize::from(work.move_cnt) - 1))
        .and_then(|frame| keyframes.get(usize::from(frame.keyframe)));
    if let Some(pose) = pose {
        player.anim.set_applied_pose(Some(pose.clone()));
    }
}

/// `Player_move`. The result is the room flag a finished scripted walk raises.
pub fn player_move(work: &mut Work, player: &mut PlayerState, tick: &Tick<'_>) -> Option<u8> {
    work.key_trg = tick.key_trg;
    work.check = false;
    if tick.stopped {
        // The head isn't placed on a stopped tick, so it doesn't follow what
        // moves the body either (a door's new position, for one).
        work.neck.body = None;
        work.latch_kage_probe(player.angle);
        latch_stopped(work, player, tick);
        return None;
    }
    work.first_tick_assumed = false;
    let held_game = work.holds_game_now();
    if work.damage_cnt & !DAMAGE_LOCK != 0 {
        work.damage_cnt -= 1;
    }
    work.noise &= 0xE0;
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
        MODE_DEAD => mover.pl_die(),
        MODE_DEAD_BODY => mover.pl_dead(),
        MODE_SCENARIO => mover.scenario(),
        _ => {}
    }
    if mover.work.routine[0] == MODE_MOVE && mover.work.routine[1] == MOVE_JUMP_DOWN {
        mover.work.floor = crate::sca::floor_toward_zero(mover.player.pos[1]);
    }
    mover.pl_neck();
    mover.rot_neck();
    mover.work.neck.body = Some(mover.player.pos);
    mover.work.latch_kage_probe(mover.player.angle);
    mover.work.root = mover.part_matrix(0);
    mover.latch_parts();
    mover.work.holds_game =
        held_game || mover.work.holds_game_now() || mover.work.holds_game_in_routine();
    mover.arrived
}

/// `Oba_ck_em` and `Sca_ck_em` push the player after `Rot_neck`, and the
/// head's `+0x5C` is on the pushed body: move the head by what the pushes
/// moved the body since it was placed, a slope's height (`Sca_hit_slope`)
/// included. `Game_loop`'s `Joint_trans2` draws the parts after the pushes
/// too, so the parts' `Workm` (`Work::parts`) moves with them.
pub fn follow_pushes(work: &mut Work, pos: [i32; 3]) {
    let Some(body) = work.neck.body else {
        return;
    };
    let by: [i32; 3] = std::array::from_fn(|axis| pos[axis] - body[axis]);
    if let Some(head) = work.neck.position.as_mut() {
        for (axis, by) in head.iter_mut().zip(by) {
            *axis += by;
        }
    }
    work.neck.body = Some(pos);
    move_parts(work, pos);
}

/// `Joint_trans2` draws the player on a stopped tick too: the pose holds, and
/// the parts' `Workm` are on where the body is.
fn latch_stopped(work: &mut Work, player: &mut PlayerState, tick: &Tick<'_>) {
    Mover {
        work,
        player,
        tick,
        arrived: None,
        stairs: None,
    }
    .latch_parts();
}

/// The parts' `Workm` move with the body to `pos`, as the draw after the
/// pushes and the scripts composes them there: what an effect on a part
/// follows (`Effects::anchors`).
pub fn move_parts(work: &mut Work, pos: [i32; 3]) {
    let Some(at) = work.parts_at else {
        return;
    };
    let by: [i32; 3] = std::array::from_fn(|axis| pos[axis] - at[axis]);
    for part in work.parts.iter_mut().flatten() {
        for (axis, by) in part.t.iter_mut().zip(by) {
            *axis += by;
        }
    }
    work.parts_at = Some(pos);
}

/// `Pl_init` (`0x4D9940`), as game mode 3 runs it when the status screen
/// closes (`0x4C000D` zeroes the routines first): the move starts again from
/// routine 1, 0, 0, 0 and clip 2 of the player's own bank, whose first frame
/// `Joint_move` plays at once, unblended, so `Move_cnt` is 1. The head looks
/// ahead again. The engine poses that frame on the same tick ([`pose_init`]).
pub fn pl_init(work: &mut Work) {
    if let Some(quiver) = &mut work.quiver {
        quiver.clear();
    }
    work.routine = [MODE_MOVE, 0, 0, 0];
    work.bank = Bank::Pld;
    work.move_no = 2;
    work.move_cnt = 1;
    work.hokan = 0;
    work.neck.flags = 0;
    work.neck.joint = None;
    work.neck.turn = [0; 2];
    work.neck.target = None;
    work.init_pose = true;
}

/// `Pl_init`'s `Joint_move`: the joints take clip 2's first frame, which the
/// next clip's blend then starts from, and the head is placed on it.
pub fn pose_init(work: &mut Work, player: &mut PlayerState, tick: &Tick<'_>) {
    let (clips, keyframes) = tick.banks.get(Bank::Weapon);
    if let Some(pose) = idle_pose(clips, keyframes) {
        player.anim.set_applied_pose(Some(pose.clone()));
    }
    place_neck(work, player, tick);
}

/// The idle's first frame, clip 2 of the weapon's bank (`pSub0_kan_t_ptr`),
/// where `Pl_init` stands the player: at a new game, through a door and when
/// the get-item window closes. Leon's handgun has his skeleton's own root
/// and no turn on the parts down to the head; Claire's (`PL01W03`'s keyframe
/// 50) stands her 14 units higher than her skeleton's root and rolls her hips.
pub fn idle_pose<'k>(clips: &[Clip], keyframes: &'k [Keyframe]) -> Option<&'k Keyframe> {
    let frame = clips.get(2)?.frames.first()?;
    keyframes.get(usize::from(frame.keyframe))
}

/// `Player_move`'s poison, before its routine: while `poison_status` is set,
/// a life each time `poison_timer` runs out, never the last one.
pub fn poison_tick(work: &mut Work, life: &mut i16) {
    if !work.poisoned {
        return;
    }
    let left = work.poison_timer;
    work.poison_timer = left.wrapping_sub(1);
    if left == 0 {
        work.poison_timer = 0x1E;
        if *life > 1 {
            *life -= 1;
        }
    }
}

/// `Player_move` while an enemy has hold of the player (`Pl_Em_damage`,
/// `Pl_Em_die`), whose states draw `Rnd`.
pub fn player_held(
    work: &mut Work,
    player: &mut PlayerState,
    tick: &Tick<'_>,
    held: &mut Held<'_>,
) {
    if tick.stopped {
        work.neck.body = None;
        work.latch_kage_probe(player.angle);
        latch_stopped(work, player, tick);
        return;
    }
    if work.damage_cnt & !DAMAGE_LOCK != 0 {
        work.damage_cnt -= 1;
    }
    work.noise &= 0xE0;
    // `Pl_Em_damage` and `Pl_Em_die` raise `fg_status`'s `0x40` on every
    // held tick, the free tick included, so START is refused from the tick
    // after the grab until the tick after the free ([`Work::holds_game`]).
    work.holds_game = true;
    work.be_flg &= !4;
    let mut mover = Mover::new(work, player, tick);
    mover.em10_held(held);
    mover.pl_neck();
    mover.rot_neck();
    mover.work.neck.body = Some(mover.player.pos);
    mover.work.latch_kage_probe(mover.player.angle);
    mover.work.root = mover.part_matrix(0);
    mover.latch_parts();
}

/// Whether [`player_held`] runs the player, not [`player_move`].
pub fn is_held(work: &Work) -> bool {
    matches!(work.routine[0], MODE_HELD | MODE_HELD_DIE)
}

/// What [`player_held`] sees of the rest of the game.
pub struct Held<'a> {
    pub holder: [i32; 3],
    pub holder_angle: u16,
    /// Where an ivy's head points (`FUN_004887d0`'s turn): its `Cdir_y`, its
    /// neck's turn and pose, less its side.
    pub holder_aim: u16,
    /// Its id, whose table says what becomes of the player.
    pub holder_id: u8,
    /// Its `Move_cnt`, which G3's kill starts the player's clip from.
    pub holder_move_cnt: u8,
    /// Where G3's frame grips a knocked-back player, x and z.
    pub holder_grip: Option<[i32; 2]>,
    /// `DWORD_009888d8`'s low byte, which some of the frames wait on.
    pub draw_buffer: u8,
    pub rnd: &'a mut crate::zombie2::Rnd,
    pub life: &'a mut i16,
    /// `Be_flg`'s bits to clear from the holder: the hold has ended.
    pub released: bool,
    /// The slots a shove threw back.
    pub knocks: u32,
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
/// pad with. The table's row 0 (`0x5338D8`) gives the one button for the
/// action, the shot and the confirm (raw `0x80`) all three bits, so a press
/// of the action key also raises `KEY_FIRE`, which `Gacha_ch` counts.
pub fn key_word(input: &Input) -> u16 {
    let bit = |set: bool, value: u16| if set { value } else { 0 };
    let action = input.action_held || input.action_pressed;
    bit(input.up, KEY_FORWARD | KEY_TILT_UP)
        | bit(input.right, KEY_RIGHT | KEY_SIDE_RIGHT)
        | bit(input.down, KEY_BACKWARD | KEY_TILT_DOWN)
        | bit(input.left, KEY_LEFT | KEY_SIDE_LEFT)
        | bit(input.run, KEY_RUN | KEY_CANCEL)
        | bit(action || input.fire, KEY_CONFIRM | KEY_FIRE | KEY_ACTION)
        | bit(input.aim, KEY_AIM)
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

/// `D_life_u`: 0 fine, 1 caution, 2 danger.
fn life_level(life: i32, poisoned: bool) -> u8 {
    if life < DANGER_LIFE {
        2
    } else if life < CAUTION_LIFE || poisoned {
        1
    } else {
        0
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
        let level = life_level(self.tick.life, self.tick.poisoned);
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
        // `Sca_ck_em` leaves `Sca_info` empty while the player stands on an
        // object.
        self.stairs = self
            .tick
            .room
            .sca
            .as_ref()
            .filter(|_| self.work.on_object.is_none())
            .and_then(|sca| {
                crate::sca::stairs_ahead(
                    sca,
                    self.player.pos,
                    self.player.angle,
                    self.player.radius,
                )
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
        // The hands as the last tick drew them (`Workm`), before the
        // branches and the analog stick turn the player.
        let wading = self.wading_parts();
        match self.work.routine[1] {
            MOVE_IDLE => self.pl_br_idle(key),
            MOVE_FORWARD => self.pl_br_forward(key),
            MOVE_RUN => self.pl_br_run(key),
            MOVE_BACKWARD => self.pl_br_backward(key),
            MOVE_ROTATE => self.pl_br_rotate(key),
            MOVE_JUMP_DOWN => self.pl_br_jump_down(key),
            MOVE_PUSH => self.pl_br_push(key),
            _ => {}
        }
        // `pl_mv_05` reads the keys itself.
        if let Some(target) = target {
            self.player.angle = player::analog_turned(self.player.angle, target);
        }
        match self.work.routine[1] {
            MOVE_IDLE => self.pl_mv_idle(),
            MOVE_FORWARD => self.pl_mv_forward(wading.as_ref()),
            MOVE_RUN => self.pl_mv_run(wading.as_ref()),
            MOVE_BACKWARD => self.pl_mv_backward(wading.as_ref()),
            MOVE_ROTATE => self.pl_mv_rotate(wading.as_ref()),
            MOVE_AIM => self.pl_mv_aim(key),
            MOVE_ITEM => self.pl_mv_item(),
            MOVE_STAIRS => self.pl_mv_stairs(),
            MOVE_CLIMB => self.pl_mv_climb(),
            MOVE_JUMP_DOWN => self.pl_mv_jump_down(),
            MOVE_PUSH => self.pl_mv_push(),
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
    /// Held or new, it tries the stairs and then a climb; new and taken by
    /// neither, it asks for this tick's check ([`Work::check`]).
    fn stairs_branch(&mut self, key: u16) -> bool {
        if key & KEY_ACTION == 0 {
            return false;
        }
        if let Some(stairs) = self.stairs {
            self.sca_hit_stairs(stairs);
            return true;
        }
        if self.updown_ck() {
            return true;
        }
        self.raise_check();
        false
    }

    /// `fg_status |= 0x200000` on `Key_trg`'s action bit.
    fn raise_check(&mut self) {
        if self.work.key_trg & KEY_ACTION != 0 {
            self.work.check = true;
        }
    }

    /// `Oma_pl_updown_ck` (`0x4CDE00`): a box to climb in front of the
    /// player, a climb record's or a room object's, or a drop, off a drop
    /// record or off the object the player stands on (`pOn_om`). The other
    /// characters at the point (`Oma_obj_ck_all`) aren't looked at.
    fn updown_ck(&mut self) -> bool {
        use crate::sca::{FLOOR_HEIGHT, INFO_DOWN, INFO_UP, PLAYER_MASK};
        let Some(sca) = self.tick.room.sca.as_ref() else {
            return false;
        };
        // A quarter turn, the nearest to the player's facing.
        let facing = self.player.angle.wrapping_add(0x200) & 0xFC00;
        let [x, y, z] = self.player.pos;
        let radius = self.player.radius;
        let floor = self.work.floor;
        let reach = |table: &[i32; 20]| table[(usize::from(self.tick.id) + 4) % table.len()];
        let (dx, dz) = rotate(facing, reach(&CLIMB_REACH), 0);
        let up = [x + dx, y - FLOOR_HEIGHT, z + dz];
        if floor < 31 {
            let info = crate::sca::info_at(sca, PLAYER_MASK, up, floor);
            // `Oma_on_check` faces the object with the player's own heading.
            let object = crate::objects::oma_on_check(
                self.tick.objects,
                floor,
                self.work.on_object,
                self.player.pos,
                self.player.angle,
                radius as u16,
            );
            if info & INFO_UP != 0 || object.is_some() {
                let ground = self.object_ground(up, radius);
                if ground.height <= up[1] && !ground.blocked {
                    self.work.routine = [MODE_MOVE, MOVE_CLIMB, 0, 0];
                    self.work.on_object = object;
                    return true;
                }
            }
        }
        let (dx, dz) = rotate(facing, reach(&DROP_REACH), 0);
        let mut down = [x + dx, y, z + dz];
        let ground = self.object_ground(down, radius);
        let landing = ground.height.min(ground.ceiling);
        if landing <= y
            || (crate::sca::info_at(sca, PLAYER_MASK, down, floor) & INFO_DOWN == 0
                && self.work.on_object.is_none())
        {
            return false;
        }
        // No other object may stand where the player lands, nor at the
        // player's own height there. `Sca_ck_hit` has just put every
        // object's box on its position (`Oma_set_ofs`).
        down[1] = landing;
        let in_the_way = self
            .tick
            .objects
            .iter()
            .enumerate()
            .filter(|&(index, obj)| {
                obj.be_flg & 1 != 0
                    && obj.be_flg & 2 == 0
                    && self.work.on_object != Some(index as u8)
            })
            .any(|(_, obj)| {
                let mut obj = *obj;
                obj.set_ofs();
                crate::objects::omd_in_check(down, &obj, radius, true)
                    || crate::objects::omd_in_check([down[0], y, down[2]], &obj, radius, true)
            });
        if in_the_way {
            return false;
        }
        self.work.on_object = None;
        self.work.routine = [MODE_MOVE, MOVE_CLIMB, CLIMB_IS_DROP, 0];
        true
    }

    /// `Sca_ck_hit` with the room objects' tops (`param_4` 0) at `pos`.
    fn object_ground(&self, pos: [i32; 3], radius: i32) -> crate::sca::Ground {
        crate::objects::sca_ck_hit(
            self.tick.room.sca.as_ref(),
            self.tick.objects,
            pos,
            radius,
            crate::sca::PLAYER_MASK,
        )
    }

    /// `Oma_ob_pull2` toward the object the player stands on, at most `max`
    /// on each axis.
    fn pull_to_object(&mut self, max: u32) {
        let Some(obj) = self
            .work
            .on_object
            .and_then(|index| self.tick.objects.get(usize::from(index)))
        else {
            return;
        };
        let height = crate::character2::PLAYER_HEIGHT;
        let pos = self.player.pos;
        let mut chr = crate::objects::CharBox::standing(
            pos,
            pos,
            -(height as i16),
            self.player.radius as u16,
            height,
        );
        chr.be_flg = self.work.be_flg;
        crate::objects::oma_ob_pull2(&mut chr, obj, max);
        self.player.pos = chr.pos;
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
        // `pl_br_09` asks for the check first and then tries the stairs, with
        // no climb.
        if self.work.key_trg & KEY_ACTION != 0 {
            self.work.check = true;
            if let Some(stairs) = self.stairs {
                self.sca_hit_stairs(stairs);
            }
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
            // Not pushed out of the other bodies (`Oba_ck_hit`) until the
            // climb is done.
            self.work.be_flg |= BE_NO_PUSH;
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
                self.pull_to_object(1000);
                self.set_routine(MOVE_IDLE);
                return;
            }
        }
        // Drawn in to the object being climbed, 4 a frame on each axis.
        self.pull_to_object(4);
        // The body lifts on the clip's frame: the move and the height together.
        let small = self.tick.id == SMALL_ID;
        let (lift, reach, end) = if small {
            (99, 0x224, 0x77)
        } else {
            (0x1D, 1000, 0x31)
        };
        let frame = self.work.move_cnt;
        // `Floor` goes up a floor here, a dozen frames before the body does
        // (`0x4DB79A`), and the camera's zones and collision see it from then.
        if frame == 0x11 {
            self.work.damage_cnt |= DAMAGE_LOCK;
            self.work.floor = self.work.floor.wrapping_add(1);
            self.work.be_flg |= BE_NO_SCA;
        }
        if frame == lift {
            self.work.spd[0] = reach;
            self.add_speed_xz();
            self.player.pos[1] -= crate::sca::FLOOR_HEIGHT;
        }
        if frame == end {
            self.work.damage_cnt &= !DAMAGE_LOCK;
            self.work.be_flg &= !(BE_NO_PUSH | BE_NO_SCA);
        }
        if self.joint_move_late(STAIRS_FIRST_STEP) {
            self.work.routine[3] = 4;
        }
        // Out of water: `Pl_water` on the clip's odd frames below 0xF.
        let frame = self.work.move_cnt;
        if i32::from(self.work.water) < self.drawn_ground() && frame < 0xF && frame & 1 != 0 {
            let root = self.part_matrix(0);
            self.pl_water(root);
        }
        // `pl_mv_08`'s footfalls are a run's noise.
        if self.work.now_seq & FRAME_FOOTSTEP != 0 {
            self.work.noise |= NOISE_RUN;
            self.footstep(1);
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
                self.work.be_flg |= BE_NO_PUSH;
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
                // `Timer3` (`+0x16B`, the back walk's wait): the splash's
                // ticks.
                self.work.back_wait = 0;
                self.fall();
            }
            3 => self.fall(),
            4 => {
                self.work.routine[3] = 5;
                self.work.move_no = LAND_CLIP;
                self.work.move_cnt = 4;
                self.work.hokan = FULL_BLEND;
                // `Pos_y = (short)Ground`: the shadow probe's, objects
                // included, as `Game_loop` left it.
                self.player.pos[1] = self.drawn_ground();
                self.work.damage_cnt &= !DAMAGE_LOCK;
                self.work.be_flg &= !BE_NO_PUSH;
                // `Snd_se_walk(1, 4, w)`, with its ripples in water.
                self.foot_down(1, false);
                // The landing is heard as loud as a running step.
                self.work.noise |= 4;
                self.land();
            }
            5 => self.land(),
            _ => {
                self.pull_to_object(1000);
                self.set_routine(MOVE_IDLE);
            }
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

    /// `Ground` as `Game_loop` left it after the last tick: `Sca_ck_hit`
    /// with no reach at the shadow's probe, `Kage_ofs` along the body.
    fn drawn_ground(&self) -> i32 {
        let (Some(_), Some(probe)) = (self.tick.room.sca.as_ref(), self.work.kage_probe) else {
            return self.ground();
        };
        i32::from(self.object_ground(probe, 0).height as i16)
    }

    fn jump_step(&mut self) {
        let ended = self.joint_move();
        let last = if self.tick.id == SMALL_ID { 0x1A } else { 0xE };
        self.work.routine[3] = if ended { 2 } else { self.work.routine[3] };
        if self.work.move_cnt == last {
            self.work.routine[3] = 2;
        }
    }

    /// The fall, and into water a splash (`Pl_water`) on each of its first
    /// five ticks there (`Timer3` 0x85).
    fn fall(&mut self) {
        self.player.pos[1] += i32::from(self.work.timer0 as i16) * 0x1E;
        self.work.timer0 += 1;
        let ground = self
            .object_ground(self.player.pos, self.player.radius)
            .height;
        // Landing on an object's top: the player stands on it.
        let pos = self.player.pos;
        let radius = self.player.radius;
        if let Some(index) = self.tick.objects.iter().position(|obj| {
            let mut obj = *obj;
            obj.set_ofs();
            obj.be_flg & 1 != 0
                && obj.be_flg & 2 == 0
                && crate::objects::omd_in_check(pos, &obj, radius, false)
                && i32::from(ground as i16) == crate::objects::object_top(&obj)
        }) {
            self.work.on_object = Some(index as u8);
        }
        self.pull_to_object(0x30);
        self.joint_move();
        if i32::from(self.work.water) < ground && self.work.back_wait == 0 {
            self.work.back_wait = JUMP_SPLASH;
        }
        if self.work.back_wait & 0x7F != 0 {
            self.work.back_wait -= 1;
            let root = self.part_matrix(0);
            self.pl_water(root);
        }
        if ground - 300 < self.player.pos[1] {
            self.work.routine[3] = 4;
        }
    }

    fn land(&mut self) {
        if self.work.move_cnt == 6 {
            self.foot_down(1, true);
        }
        self.pull_to_object(0x30);
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

    /// `pl_mv_07`: turn to the stairs, climb them step by step, walk off. The
    /// shadow leads the body by `Timer0` meanwhile, and probes its ground
    /// `Kage_ofs` further on.
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
            2 | 3 => {
                if self.work.routine[2] == 2 {
                    self.work.routine[2] = 3;
                    // `Floor` steps the whole flight here, `Timer1` floors up or
                    // down, before the first step (`0x4DB1B7`).
                    let flight = self.work.timer1 as u8;
                    self.work.floor = if self.work.timer2 == 0 {
                        self.work.floor.wrapping_add(flight)
                    } else {
                        self.work.floor.wrapping_sub(flight)
                    };
                    self.work.bank = Bank::Room;
                    self.work.move_no = climb;
                    self.work.move_cnt = 0;
                    self.work.hokan = STAIRS_FIRST_BLEND;
                    self.work.timer0 = 0;
                }
                self.stairs_step(climb, STAIRS_FIRST_STEP);
                let down = self.work.timer2 != 0;
                let step = if down {
                    STAIRS_KAGE_STEP
                } else {
                    -STAIRS_KAGE_STEP
                };
                let mut lead = (self.work.timer0 as i16).wrapping_add(step);
                self.work.kage_ofs = self.work.kage_ofs.wrapping_add(STAIRS_KAGE_STEP);
                if (down && lead > STAIRS_KAGE_LEAD) || (!down && lead < -STAIRS_KAGE_LEAD) {
                    lead = if down {
                        STAIRS_KAGE_LEAD
                    } else {
                        -STAIRS_KAGE_LEAD
                    };
                    self.work.kage_ofs = STAIRS_KAGE_LEAD;
                }
                self.work.timer0 = lead as u16;
                self.stairs_kage(lead);
            }
            4 => {
                self.stairs_step(climb, BLEND_STEP);
                self.stairs_kage(self.stairs_full_lead());
            }
            5 => {
                self.spd_get();
                if self.joint_move() {
                    self.work.routine[2] = 6;
                }
                self.add_speed_xyz();
                self.stairs_kage(self.stairs_full_lead());
            }
            6 | 7 => {
                let down = self.work.timer2 != 0;
                if self.work.routine[2] == 6 {
                    self.work.routine[2] = 7;
                    self.work.bank = Bank::Weapon;
                    self.work.move_no = 0;
                    self.work.move_cnt = 0;
                    self.work.hokan = FULL_BLEND;
                    self.work.timer0 = self.stairs_full_lead() as u16;
                }
                // The lead and the probe both shrink back to the body.
                let step = if down {
                    -STAIRS_KAGE_BACK
                } else {
                    STAIRS_KAGE_BACK
                };
                let lead = (self.work.timer0 as i16).wrapping_add(step);
                self.work.kage_ofs = self.work.kage_ofs.wrapping_sub(STAIRS_KAGE_BACK);
                let lead = if (down && lead < 0) || (!down && lead > 0) {
                    self.work.kage_ofs = 0;
                    0
                } else {
                    lead
                };
                self.work.timer0 = lead as u16;
                self.stairs_kage(lead);
                self.spd_get();
                self.joint_move();
                self.add_speed_xz();
                if self.work.move_cnt > STAIRS_WALK_OFF_FRAME {
                    self.set_routine(MOVE_IDLE);
                    self.work.damage_cnt &= !DAMAGE_LOCK;
                    self.work.kage_pos = None;
                }
            }
            _ => {}
        }
        // `pl_mv_07`'s footfalls are a run's noise.
        self.footfall(0, NOISE_RUN);
    }

    /// The shadow's whole lead on the stairs, ahead going down and behind
    /// going up.
    fn stairs_full_lead(&self) -> i16 {
        if self.work.timer2 != 0 {
            STAIRS_KAGE_LEAD
        } else {
            -STAIRS_KAGE_LEAD
        }
    }

    /// `pl_mv_07`'s `DWORD_00689be0`, the shadow's `pPos`: `lead` along `M`
    /// from part 0 as the last tick left it.
    fn stairs_kage(&mut self, lead: i16) {
        let Some(root) = self.work.root else {
            return;
        };
        let m = self.work.body_rotation(self.player.angle);
        let lead = i32::from(lead);
        self.work.kage_pos = Some(std::array::from_fn(|axis| {
            root.t[axis] + i32::from(((i32::from(m[axis][0]) * lead) >> 12) as i16)
        }));
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
        // An object gave way: push it, run or not. The aim still wins.
        if self.tick.pushing {
            self.set_routine(MOVE_PUSH);
        }
        self.end_branch(key);
    }

    /// `pl_br_0a`: forward let go, or the object stopped giving way, ends the
    /// push's walk (state 5) with motion 7 backward.
    fn pl_br_push(&mut self, key: u16) {
        if (key & KEY_TILT_UP == 0 || !self.tick.pushing) && self.work.routine[2] == 5 {
            self.work.routine[2] = 6;
            self.work.move_no = PUSH_BRACE_CLIP;
            self.work.move_cnt = 0;
            self.work.hokan = FULL_BLEND;
            self.work.sounds.push(PUSH_END_SOUND);
        }
    }

    /// `pl_mv_0a`: square up to the object, brace (motion 7), push (motion 8)
    /// until `pl_br_0a` ends it, let go (motion 7 backward), and stand.
    fn pl_mv_push(&mut self) {
        match self.work.routine[2] {
            0 | 1 => {
                if self.work.routine[2] == 0 {
                    self.work.routine[2] = 1;
                    self.work.bank = Bank::Weapon;
                    self.work.move_no = self.life_clip(&WALK_CLIPS);
                    self.work.move_cnt = 0;
                    self.work.hokan = FULL_BLEND;
                }
                // The step of the frame before.
                if self.work.now_seq & FRAME_FOOTSTEP != 0 {
                    self.footstep(1);
                    self.work.noise |= 4;
                }
                if self.square_up() {
                    self.work.routine[2] = 2;
                }
            }
            2 | 3 => {
                if self.work.routine[2] == 2 {
                    self.work.routine[2] = 3;
                    self.push_clip(PUSH_BRACE_CLIP);
                }
                if self.joint_move() {
                    self.work.routine[2] += 1;
                }
            }
            4 | 5 => {
                if self.work.routine[2] == 4 {
                    self.work.routine[2] = 5;
                    self.push_clip(PUSH_CLIP);
                }
                if self.work.move_cnt == 1 {
                    self.work.sounds.push(PUSH_SOUND);
                }
                // `Foot_set_pl`'s second mode: the step is how far the foot
                // moved, from last tick's pose to this one's.
                let chain = if self.tick.id == SMALL_ID {
                    &AIM_FOOT_CHAIN
                } else {
                    &AIM_OTHER_FOOT_CHAIN
                };
                let planted = self.chain_position(chain);
                self.joint_move();
                if self.tick.id == SMALL_ID {
                    self.work.spd[0] = 5;
                }
                if self.work.now_seq & FRAME_FOOTSTEP_ALT != 0
                    && let (Some(planted), Some(foot)) = (planted, self.chain_position(chain))
                {
                    let (dx, dz) = (foot[0] - planted[0], foot[2] - planted[2]);
                    self.work.spd[0] = crate::sca::square_root0(
                        dz.wrapping_mul(dz).wrapping_add(dx.wrapping_mul(dx)),
                    ) as i16;
                }
                self.work.spd[2] = 0;
                self.add_speed_xz();
            }
            6 => {
                if self.joint_move_late_reversed(BLEND_STEP, true) {
                    self.work.routine[2] += 1;
                }
            }
            _ => self.set_routine(MOVE_IDLE),
        }
    }

    /// One of the push's clips of the player's own, from its first frame,
    /// blended in.
    fn push_clip(&mut self, clip: u8) {
        self.work.bank = Bank::Pld;
        self.work.move_no = clip;
        self.work.move_cnt = 0;
        self.work.hokan = FULL_BLEND;
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
        // `pl_br_03` doesn't try the stairs or a climb.
        self.raise_check();
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

    /// `pl_mv05_w1`: the handguns' stance, and `pl_mv05_w2`, the bursts'.
    /// `Status_flg`'s top bits hold the tilt.
    fn pl_mv_aim(&mut self, key: u16) {
        // `Foot_set_pl` reads last tick's world matrix from the parts.
        let chain = self.tick.weapon.foot_chain(self.tick.id);
        let planted = self.chain_position(chain);
        let drawn = self.drawn_parts();
        match self.work.routine[2] {
            0 => self.aim_raise(key, &drawn),
            1 => self.aim_hold(key),
            2 if self.tick.weapon.is_knife() => self.aim_slash(key),
            2 if self.tick.weapon.bursts() => self.aim_burst(key, &drawn),
            2 => self.aim_fire(key, &drawn),
            3 => self.aim_lower(key),
            4 => self.aim_reload(),
            5 => {
                // `pl_mv05_w1_t5`: a lock on nothing goes straight back.
                self.work.routine[2] = 1;
                self.work.routine[3] = 0;
            }
            6 if self.tick.weapon.is_knife() => self.knife_bounce(),
            _ => {}
        }
        if self.work.spd[0] == 0
            && (self.work.routine[2] != 0 || self.work.hokan == 0)
            && let (Some(planted), Some(foot)) = (planted, self.chain_position(chain))
        {
            self.player.pos[0] += planted[0] - foot[0];
            self.player.pos[2] += planted[2] - foot[2];
        }
    }

    /// The handguns' foot in the world, on the current pose: `Foot_set_pl`'s
    /// chain, the root, the hips and that leg's three parts.
    pub(crate) fn foot_position(&self) -> Option<[i32; 3]> {
        self.chain_position(&AIM_FOOT_CHAIN)
    }

    /// `pSin_parts_ptr[0].Pos_y`: how far the pose has the root off the ground.
    pub(crate) fn root_y(&self) -> i32 {
        let (clips, keyframes) = self.tick.banks.get(self.work.bank);
        self.player
            .anim
            .pose_keyframe(clips, keyframes)
            .map_or(0, |pose| i32::from(pose.offset[1]))
    }

    /// `Rot_neck`: turn the head toward what `Plc_neck` asked for, or back to
    /// its pose.
    /// `Rot_neck_em`: the head turns toward `target`, within its limit, and only
    /// that way. `Neck_Dir_ZY` aims from the neck's `Workm.t` (`+0x5C`) as the
    /// last draw left it (`drawn`), not from this tick's pose; with no draw yet,
    /// from the pose.
    pub(crate) fn rot_neck_em(&mut self, target: Option<[i32; 3]>, drawn: Option<[i32; 3]>) {
        if self.work.neck.flags & 1 != 0 {
            // The head still moves with the body, for another's aim at it.
            self.work.neck.position = self.chain_position(&[0, NECK_JOINT]);
            return;
        }
        let from = self.chain_position(&[0, NECK_JOINT]);
        let aim_from = drawn.or(from);
        let old = self.work.neck.turn[0];
        if let Some(turn) = self.rot_joint_em(NECK_JOINT, aim_from, old, target) {
            self.work.neck.position = from;
            self.work.neck.turn[0] = turn;
        }
    }

    /// Where the head is once `Oba_ck_em` and `Sca_ck_em` have pushed the
    /// body: the neck joint's `+0x5C` that the next tick's `Root_ck` looks
    /// from. The original's world matrices are made after the pushes, so the
    /// head moves with them.
    pub(crate) fn place_neck(&mut self) {
        if let Some(position) = self.chain_position(&[0, NECK_JOINT]) {
            self.work.neck.position = Some(position);
        }
    }

    /// `FUN_0045f5b0`: `Rot_neck_em` on both of the dog's neck joints.
    pub(crate) fn rot_neck_dog(&mut self, target: Option<[i32; 3]>) {
        let [first, second] = DOG_NECK;
        let from = self.part_point(first, [0, 0, 0]);
        if self.work.neck.flags & 1 == 0 {
            let old = self.work.neck.turn[0];
            if let Some(turn) = self.rot_joint_em(first, from, old, target) {
                self.work.neck.turn[0] = turn;
            }
            let old = self.work.neck.turn_two.unwrap_or(0);
            let from_second = self.part_point(second, [0, 0, 0]);
            if let Some(turn) = self.rot_joint_em(second, from_second, old, target) {
                self.work.neck.turn_two = Some(turn);
            }
        }
        self.work.neck.position = from;
    }

    /// One joint of `Rot_neck_em`: its new yaw, starting from `old`.
    fn rot_joint_em(
        &mut self,
        joint: usize,
        from: Option<[i32; 3]>,
        old: i16,
        target: Option<[i32; 3]>,
    ) -> Option<i16> {
        let angle = self.player.angle as i16;
        self.rot_joint_with(
            joint,
            from,
            old,
            target,
            angle,
            NECK_RATES[0],
            NECK_LIMITS[0],
        )
    }

    /// `Rot_neck_em` on `Neck_no` `joint`, given the work's direction and the
    /// joint's rate and limit (its `+0x9C` and `+0xA0`).
    pub(crate) fn rot_neck_joint(
        &mut self,
        joint: usize,
        from: Option<[i32; 3]>,
        target: Option<[i32; 3]>,
        angle: i16,
        rate: i32,
        limit: i32,
    ) {
        if self.work.neck.flags & 1 != 0 {
            return;
        }
        let old = self.work.neck.turn[0];
        if let Some(turn) = self.rot_joint_with(joint, from, old, target, angle, rate, limit) {
            self.work.neck.turn[0] = turn;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn rot_joint_with(
        &mut self,
        joint: usize,
        from: Option<[i32; 3]>,
        old: i16,
        target: Option<[i32; 3]>,
        angle: i16,
        rate: i32,
        limit: i32,
    ) -> Option<i16> {
        let (clips, keyframes) = self.tick.banks.get(self.work.bank);
        let pose = self.player.anim.pose_keyframe(clips, keyframes)?;
        let bone = pose.rotations.get(joint).copied()?;
        let root = pose.rotations.first().copied()?;
        let bone = i32::from(bone[1]);
        let facing = i32::from(angle) + i32::from(root[1]);
        let neck = &self.work.neck;
        let mut aim = match (from, target) {
            (Some(from), Some(target)) => neck_direction(from, target)[0],
            _ => 0,
        };
        if neck.flags & 2 != 0 {
            aim = bone + facing;
        }
        let rate = if neck.flags & 0x80 != 0 {
            i32::from(neck.rates[0])
        } else {
            rate
        };
        if limit * 2 < (aim - facing + limit) & 0xFFF {
            aim = if (aim - facing) & 0x800 == 0 {
                limit + facing
            } else {
                facing - limit
            };
        }
        let old = i32::from(old);
        let error = rate - old - bone - facing + aim;
        let mut turn = old - rate;
        if error & 0x800 == 0 {
            turn += rate * 2;
        }
        if error & 0xFFF < rate * 2 {
            turn = aim - bone - facing;
        }
        Some(turn as i16)
    }

    /// `Pl_neck`: every so often, choose the nearest body the head can see in
    /// front of it, standing bodies before those on the floor.
    fn pl_neck(&mut self) {
        if self.work.neck.flags & 1 != 0 {
            return;
        }
        if self.work.neck.wait != 0 {
            self.work.neck.wait -= 1;
            return;
        }
        self.work.neck.wait = NECK_WAIT;
        let sca = self.tick.room.sca.as_ref();
        let from = self.work.neck.position.unwrap_or(self.player.pos);
        let [x, _, z] = self.player.pos;
        let angle = self.player.angle;
        let mut nearest = None;
        let mut nearest_down = None;
        let mut reach = NECK_REACH;
        let mut reach_down = i32::MAX as u32;
        for target in self.tick.neck_targets {
            if target.kind & 0xC000 != 0
                || sca.is_some_and(|sca| {
                    crate::zombie2::sca_ck_line(&sca.records, from, target.neck, NECK_LINE_MASK)
                })
                || direction_ck(x, z, target.pos[0], target.pos[2])
                    .wrapping_sub(angle)
                    .wrapping_add(NECK_VIEW)
                    & 0xFFF
                    >= NECK_VIEW * 2
            {
                continue;
            }
            if target.kind & 0x2000 == 0 {
                if target.distance < reach {
                    reach = target.distance;
                    nearest = Some(target.index);
                }
            } else if target.distance < reach_down {
                reach_down = target.distance;
                nearest_down = Some(target.index);
            }
        }
        self.work.neck.target = nearest.or(nearest_down);
    }

    pub(crate) fn rot_neck(&mut self) {
        if self.work.neck.flags & 1 != 0 {
            self.work.neck.position = self.chain_position(&[0, NECK_JOINT]);
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
        let looked_at = neck.target.and_then(|index| *self.tick.necks.get(index)?);
        if let (Some(from), Some(to)) = (from, looked_at) {
            aim = neck_direction(from, to);
        }
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
        if neck.target.is_none() && flags & 0x80 == 0 {
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
        let world = self.part_matrix(part)?;
        Some(std::array::from_fn(|axis| {
            let turned: i32 = (0..3)
                .map(|index| i32::from(world.m[axis][index]) * point[index])
                .sum();
            world.t[axis] + (turned >> 12)
        }))
    }

    pub(crate) fn part_matrix(&self, part: usize) -> Option<Matrix> {
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
        self.chain_matrix(&chain)
    }

    /// `Work::parts`: each part's `Workm` on the current pose, the head on
    /// its neck's turn.
    fn latch_parts(&mut self) {
        self.work.parts = self.posed_parts();
        self.work.parts_at = Some(self.player.pos);
    }

    /// Every part's `Workm` on the current pose, with the neck's turn on its
    /// joint, as the skeleton composes them from the root: what `FUN_004886f0`
    /// leaves for the ivy's vines.
    pub(crate) fn posed_parts(&self) -> Vec<Option<Matrix>> {
        self.posed_parts_turned(self.work.body_rotation(self.player.angle))
    }

    /// [`Self::posed_parts`] from a root turned by `turn`.
    pub(crate) fn posed_parts_turned(&self, turn: [[i16; 3]; 3]) -> Vec<Option<Matrix>> {
        self.pose_tree(turn).compose(&[])
    }

    /// The current pose as each part's matrix on its parent's, from a root
    /// turned by `turn`, for [`PoseTree::compose`].
    pub(crate) fn pose_tree(&self, turn: [[i16; 3]; 3]) -> PoseTree {
        let (clips, keyframes) = self.tick.banks.get(self.work.bank);
        let skeleton = &self.tick.skeleton;
        let mut tree = PoseTree {
            root: Matrix {
                m: turn,
                t: self.player.pos,
            },
            local: vec![None; skeleton.relative.len()],
            children: skeleton.children.clone(),
        };
        let Some(mut pose) = self.player.anim.pose_keyframe(clips, keyframes) else {
            tree.local.clear();
            return tree;
        };
        self.work.neck.apply(&mut pose);
        for (part, local) in tree.local.iter_mut().enumerate() {
            let Some(rotation) = pose.rotations.get(part) else {
                continue;
            };
            let t = if part == 0 {
                let [x, y, z] = pose.offset;
                Some([x, self.work.scaled_height(y), z])
            } else {
                self.relative(part)
            };
            *local = t.map(|t| Matrix {
                m: self.part_rotation(part, *rotation),
                t: t.map(i32::from),
            });
        }
        lean(&mut tree.local, self.work.lean, self.work.lean_yaw);
        tree
    }

    /// The parts' own rotations on the current pose: `+0x68`, which
    /// `Rot_neck_em` puts back after it builds the neck's matrix.
    pub(crate) fn posed_rotations(&self) -> Vec<[i16; 3]> {
        let (clips, keyframes) = self.tick.banks.get(self.work.bank);
        self.player
            .anim
            .pose_keyframe(clips, keyframes)
            .map_or_else(Vec::new, |pose| pose.rotations)
    }

    /// A part's `M`: its own rotation and its place under its parent.
    pub(crate) fn part_local(&self, part: usize) -> Option<Matrix> {
        let (clips, keyframes) = self.tick.banks.get(self.work.bank);
        let pose = self.player.anim.pose_keyframe(clips, keyframes)?;
        Some(Matrix {
            m: self.part_rotation(part, *pose.rotations.get(part)?),
            t: self.relative(part)?.map(i32::from),
        })
    }

    /// Where a part sits under its parent: the skeleton's, or what the work
    /// put in its place.
    fn relative(&self, part: usize) -> Option<[i16; 3]> {
        let skeleton = self.tick.skeleton.relative.get(part).copied()?;
        Some(
            self.work
                .joint_offsets
                .iter()
                .find(|(index, _)| usize::from(*index) == part)
                .map_or(skeleton, |(_, at)| *at),
        )
    }

    /// `Joint_trans2`'s `Workm`s, in `parts`, from last tick's: the parts
    /// go in index order, each on its parent's `Workm` (this tick's for a
    /// parent before it, last tick's for one after it), and a part whose
    /// `Be_flg` bit 0 is clear (`hidden`) isn't transformed at all, so it
    /// keeps last tick's matrix and its children compose with that.
    pub(crate) fn joint_trans2(&self, hidden: u32, parts: &mut Vec<Option<Matrix>>) {
        let skeleton = &self.tick.skeleton;
        let count = skeleton.relative.len();
        parts.resize(count, None);
        let (clips, keyframes) = self.tick.banks.get(self.work.bank);
        let Some(pose) = self.player.anim.pose_keyframe(clips, keyframes) else {
            return;
        };
        let mut parents = vec![None; count];
        for (parent, children) in skeleton.children.iter().enumerate() {
            for &child in children {
                if let Some(slot) = parents.get_mut(usize::from(child)) {
                    *slot = Some(parent);
                }
            }
        }
        let root = Matrix {
            m: self.work.body_rotation(self.player.angle),
            t: self.player.pos,
        };
        for part in 0..count {
            if part < 32 && hidden & 1 << part != 0 {
                continue;
            }
            let parent = if part == 0 {
                Some(root)
            } else {
                parents[part].and_then(|parent| parts[parent])
            };
            let t = if part == 0 {
                let [x, y, z] = pose.offset;
                Some([x, self.work.scaled_height(y), z])
            } else {
                self.relative(part)
            };
            parts[part] = match (parent, t, pose.rotations.get(part)) {
                (Some(parent), Some(t), Some(rotation)) => Some(parent.comp(&Matrix {
                    m: self.part_rotation(part, *rotation),
                    t: t.map(i32::from),
                })),
                _ => None,
            };
        }
    }

    fn chain_matrix(&self, chain: &[usize]) -> Option<Matrix> {
        let (clips, keyframes) = self.tick.banks.get(self.work.bank);
        let pose = self.player.anim.pose_keyframe(clips, keyframes)?;
        let mut world = Matrix {
            m: self.work.body_rotation(self.player.angle),
            t: self.player.pos,
        };
        for &part in chain {
            let t = if part == 0 {
                let [x, y, z] = pose.offset;
                [x, self.work.scaled_height(y), z]
            } else {
                self.relative(part)?
            };
            world = world.comp(&Matrix {
                m: self.part_rotation(part, *pose.rotations.get(part)?),
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
    fn aim_raise(&mut self, key: u16, drawn: &DrawnParts) {
        match self.work.routine[3] {
            0 => {
                self.aim_clip(AIM_RAISE_CLIP);
                self.set_tilt(TILT_LEVEL);
                self.em_ck(self.raise_reach());
                self.work.aimed_again = false;
                self.work.neck.flags |= 0x12;
                self.work.spd[0] = 0;
            }
            1 => {}
            2 => {
                self.aim_stage(1);
                return;
            }
            _ => return,
        }
        self.aim_splashes(300, drawn);
        if let Some(target) = self
            .work
            .aim_target
            .and_then(|slot| self.tick.targets.iter().find(|target| target.slot == slot))
        {
            let to = direction_ck(
                self.player.pos[0],
                self.player.pos[2],
                target.at[0],
                target.at[1],
            );
            self.player.angle = goto(self.player.angle, to, self.tick.weapon.enemy_turn());
        }
        // A tilt down looks again, for the nearest within reach (`Key_trg`'s 0x20).
        if self.work.key_trg & KEY_TILT_DOWN != 0 && !self.work.aimed_again {
            self.em_ck(3000);
            self.work.aimed_again = true;
        }
        self.aim_turn(key, AIM_RAISE_TURN);
        if self.joint_move() {
            self.work.routine[3] += 1;
        }
        if self.work.move_cnt > self.tick.weapon.tilt_down_frame() && key & KEY_TILT_DOWN != 0 {
            self.aim_stage(1);
            self.set_tilt(TILT_DOWN);
            return;
        }
        if self.work.move_cnt > self.tick.weapon.tilt_up_frame() && key & KEY_TILT_UP != 0 {
            self.aim_stage(1);
            self.set_tilt(TILT_UP);
        }
    }

    /// `Em_ck`, from the neck.
    fn em_ck(&mut self, range: u32) {
        let neck = self.work.neck.position.unwrap_or(self.player.pos);
        let sca = sca_records(self.tick);
        self.work.aim_target =
            crate::weapon2::em_ck(self.tick.targets, sca, self.player.pos, neck, range);
    }

    /// `hit_scan_ck` and `Weapon_at_ck`, on a tick the shot looks for a body.
    fn shoot(&mut self) {
        let weapon = self.tick.weapon.id;
        let level = self.work.shot_level;
        let shooter = crate::weapon2::Shooter {
            pos: self.player.pos,
            neck: self.work.neck.position.unwrap_or(self.player.pos),
            angle: self.player.angle,
            weapon,
            level,
        };
        let sca = sca_records(self.tick);
        self.work.hit = crate::weapon2::weapon_at_ck(&shooter, self.tick.targets, sca);
    }

    /// The clip of `clips` for the tilt.
    fn tilt_clip(&self, clips: [u8; 3]) -> u8 {
        match self.work.status_flg >> 13 {
            1 => clips[0],
            2 => clips[1],
            4 => clips[2],
            _ => 0,
        }
    }

    /// `pl_mv05_w1_t1`, and `pl_mv05_w2_t1` on its own clips.
    fn aim_hold(&mut self, key: u16) {
        if self.work.routine[3] == 0 {
            self.aim_clip(self.tilt_clip(self.tick.weapon.hold_clips()));
            // The knife makes no noise aiming but the stance's own.
            self.work.noise |= if self.tick.weapon.is_knife() {
                0x08
            } else {
                0x18
            };
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
            if self.tick.input.fire_pressed {
                // The reload starts next tick: this one still turns and
                // steps the hold (`pl_mv05_w1` falls through to `Joint_move`).
                if self.tick.weapon.spare {
                    self.aim_stage(4);
                } else {
                    self.player.queue_sound(SOUND_EMPTY);
                }
            }
        }
        self.aim_turn(key, AIM_HOLD_TURN);
        self.joint_move();
    }

    /// `pl_mv05_w1_t2`.
    fn aim_fire(&mut self, key: u16, drawn: &DrawnParts) {
        if self.work.routine[3] == 0 {
            // The matrix `Shoot_camera` reads is the last tick's, so the new
            // clip mustn't be set yet.
            self.work.shot_line =
                self.part_matrix(SHOT_PART)
                    .map(|hand| crate::weapon2::ShotLine {
                        from: hand.t,
                        matrix: hand.m,
                    });
            let bow_hand = self.part_matrix(SHOT_PART);
            let clip = match self.work.status_flg >> 13 {
                1 => AIM_FIRE_CLIPS[0],
                2 => AIM_FIRE_CLIPS[1],
                4 => AIM_FIRE_CLIPS[2],
                _ => 0,
            };
            self.aim_clip(clip);
            self.work.noise |= 1;
            if self.tick.weapon.id == crate::bow2::BOW {
                // `FUN_004d58c0`: a silent shot of three bolts.
                self.work.noise &= !1;
                if let Some(hand) = bow_hand {
                    let quiver = self.work.quiver.get_or_insert_with(Default::default);
                    for side in 0..3 {
                        quiver.launch(side, &hand, self.player.angle);
                    }
                }
            }
            let hand = self
                .part_point(HAND_PART, [0; 3])
                .unwrap_or(self.player.pos);
            self.work.shot_level = crate::weapon2::hit_scan_ck(
                self.tick.id,
                self.tick.weapon.id,
                self.player.pos[1],
                self.work.parts0_y,
                hand[1],
            );
            self.work.shot_ticks = self.tick.weapon.shot_ticks();
            self.work.fired = true;
            self.work.spd = [0; 3];
        }
        if self.work.shot_ticks & 1 != 0 && !self.tick.weapon.shoots_effects() {
            self.shoot();
        }
        self.work.shot_ticks >>= 1;
        self.aim_turn(key, AIM_FIRE_TURN);
        if key & KEY_AIM == 0 && self.work.move_cnt > self.tick.weapon.fire_cancel_frame() {
            self.aim_stage(1);
            self.work.spd = [0; 3];
            return;
        }
        let shot_from = self.work.effects.len();
        self.shot_effects();
        let kicks = matches!(self.tick.weapon.id, 6 | 8) && self.tick.id != 0xB;
        if kicks {
            self.spd_get();
        }
        let ended = self.joint_move();
        if ended {
            self.aim_stage(1);
            self.work.spd = [0; 3];
        }
        // The splashes come before `pl_wp02_tbl`'s calls in the original,
        // which takes the effects' draws in the order they were called.
        let splash_from = self.work.effects.len();
        let draws_from = self.work.water_draws.len();
        self.aim_splashes(200, drawn);
        let splashes = self.work.effects.split_off(splash_from);
        self.work.effects.splice(shot_from..shot_from, splashes);
        for draw in &mut self.work.water_draws[draws_from..] {
            draw.effect = draw.effect - splash_from + shot_from;
        }
        if self.tick.weapon.shoots_effects() && self.work.move_cnt == 8 && !self.tick.weapon.loaded
        {
            // `FUN_004d6150`: an empty launcher stops the shot to reload.
            self.aim_stage(1);
            self.work.spd = [0; 3];
        }
        // `pl_w01_t2_move`, the bowgun's, only plays them.
        let sounds = matches!(self.tick.weapon.id, 7 | GRENADES..=crate::bow2::BOW);
        if sounds && self.work.now_seq & FRAME_SOUND != 0 {
            self.player
                .queue_sound(((self.work.now_seq & FRAME_SOUND_SLOT) >> 16) as u16);
        }
        if kicks {
            self.add_speed_xz();
        }
    }

    /// `pl_mv05_w2_t2`: the burst, for as long as the trigger is held and the
    /// gun has rounds. `hit_scan_ck` looks at the hand on every tick, and
    /// `Weapon_at_ck` follows the weapon's sequence; a change of tilt goes on
    /// from the frame the burst had reached. Letting go of the trigger plays
    /// the burst's end, from which a press fires again.
    fn aim_burst(&mut self, key: u16, drawn: &DrawnParts) {
        let hand = self
            .part_point(HAND_PART, [0; 3])
            .unwrap_or(self.player.pos);
        self.work.shot_level = crate::weapon2::hit_scan_ck(
            self.tick.id,
            self.tick.weapon.id,
            self.player.pos[1],
            self.work.parts0_y,
            hand[1],
        );
        let weapon = self.tick.weapon;
        match self.work.routine[3] {
            0..=2 => {
                if self.work.routine[3] == 0 {
                    self.aim_clip(self.tilt_clip(BURST_CLIPS));
                }
                if self.work.routine[3] == 1 {
                    // A new tilt keeps the frame.
                    self.work.move_no = self.tilt_clip(BURST_CLIPS);
                    self.work.hokan = FULL_BLEND;
                    self.init_weapon_at();
                    self.work.routine[3] = 2;
                }
                let spends = weapon.id != SMG || self.work.move_cnt.is_multiple_of(3);
                if spends && !self.tama_decrease() {
                    self.work.routine[3] = 3;
                    return;
                }
                self.work.noise |= 1;
                if self.weapon_at_seq() {
                    self.shoot();
                }
                let shot_hand = self.drawn_part(SHOT_PART);
                self.joint_move();
                self.burst_effects(shot_hand);
                if key & KEY_AIM == 0 {
                    self.aim_stage(3);
                    return;
                }
                let tilt = if key & KEY_TILT_UP != 0 && self.work.status_flg & TILT_UP == 0 {
                    Some(TILT_UP)
                } else if key & KEY_TILT_DOWN != 0 && self.work.status_flg & TILT_DOWN == 0 {
                    Some(TILT_DOWN)
                } else if key & (KEY_TILT_UP | KEY_TILT_DOWN) == 0
                    && self.work.status_flg & TILT_LEVEL == 0
                {
                    Some(TILT_LEVEL)
                } else {
                    None
                };
                if let Some(tilt) = tilt {
                    self.work.routine[3] = 1;
                    self.set_tilt(tilt);
                    if weapon.id == FLAMETHROWER {
                        self.player.queue_sound(SOUND_FLAME_TILT);
                    }
                    return;
                }
                if key & KEY_FIRE == 0 {
                    self.work.routine[3] = 3;
                    return;
                }
                let water = self.work.water;
                if water != 0 && i32::from(water) < self.ground() && self.work.move_cnt & 3 == 0 {
                    self.pl_water(drawn[0]);
                }
            }
            3 | 4 => {
                if self.work.routine[3] == 3 {
                    self.aim_clip(self.tilt_clip(BURST_END_CLIPS));
                    self.work.routine[3] = 4;
                }
                if self.joint_move() {
                    self.work.routine[3] += 1;
                }
                if key & KEY_FIRE != 0 {
                    if weapon.loaded {
                        self.work.routine[2] = 2;
                        self.work.routine[3] = 1;
                        return;
                    }
                    if self.tick.input.fire_pressed {
                        if weapon.spare {
                            self.aim_stage(4);
                        } else {
                            self.player.queue_sound(SOUND_EMPTY);
                        }
                    }
                }
            }
            5 => self.aim_stage(1),
            _ => {}
        }
        self.aim_turn(key, BURST_TURN);
    }

    /// `init_Weapon_at`: the sequence from its start.
    fn init_weapon_at(&mut self) {
        let sequence = self.tick.weapon.burst_sequence();
        self.work.attw = (0, sequence[0].1);
    }

    /// `Weapon_at_ck`'s walk of the sequence: whether this tick looks for a
    /// body.
    fn weapon_at_seq(&mut self) -> bool {
        let (attw, looks) = attw_step(self.tick.weapon.burst_sequence(), self.work.attw);
        self.work.attw = attw;
        looks
    }

    /// `Tama_decrease_i` for the bursts: seven calls in eight take nothing
    /// ([`Work::tama_count`]), and the eighth takes two rounds, or finds the
    /// gun empty after one.
    fn tama_decrease(&mut self) -> bool {
        let count = self.work.tama_count.wrapping_add(1);
        if count < 8 {
            self.work.tama_count = count;
            return true;
        }
        self.work.tama_count = 0;
        let left = self.tick.weapon.rounds.saturating_sub(self.work.spent);
        if left == 0 {
            return false;
        }
        self.work.spent += 1;
        if left == 1 {
            return false;
        }
        self.work.spent += 1;
        true
    }

    /// `pl_w0f_t2_move` and `pl_w10_t2_move`, on the frame `Joint_move`
    /// reached, from the hand as the last draw left it.
    fn burst_effects(&mut self, hand: Option<Matrix>) {
        let frame = self.work.move_cnt;
        let angle = self.player.angle as i16;
        let mut calls: Vec<(u32, i16, [i16; 3])> = Vec::new();
        if self.tick.weapon.id == SMG {
            if frame % 3 == 1 {
                let point = [0x46, 1000, if self.tick.id > 3 { -100 } else { -60 }];
                calls.extend(SMG_FLASH.map(|word| (word, 0, point)));
            }
            if frame % 6 == 1 {
                let (word, turn, point) = SMG_CASE;
                calls.push((word, angle.wrapping_add(turn), point));
            }
        } else {
            if frame % 3 == 1 {
                let (word, point) = FLAME;
                calls.push((word, angle, point));
            }
            // At `+0x790`: part 11's `Pos_x` to `Pos_z`, its place under its
            // parent, which `Snd_se_on` takes as a point in the room.
            let at = self
                .relative(SHOT_PART)
                .map_or(self.player.pos, |pos| pos.map(i32::from));
            if frame == 1 {
                self.player.queue_sound_at(SOUND_FLAME_START, at);
            }
            if frame == 0xB {
                self.player.queue_sound_at(SOUND_FLAME_GO_ON, at);
            }
        }
        let Some(hand) = hand else {
            return;
        };
        for (word, dir, point) in calls {
            self.work.effects.push(EffectCall {
                kind: (word >> 24) as u8,
                row: (word >> 16) as u8,
                size: word as u16,
                dir,
                at: crate::esp2::apply_matrix(&hand, point),
                motion: None,
                on: Some((SHOT_PART as u8, hand, point)),
                turn: None,
            });
        }
    }

    /// `Em_ck`'s reach as the raise starts: none for the guns, and the knife's.
    fn raise_reach(&self) -> u32 {
        if !self.tick.weapon.is_knife() {
            return 0;
        }
        let room = (self.tick.room.stage, self.tick.room.room);
        if KNIFE_WIDE_ROOMS.contains(&room) {
            KNIFE_WIDE_REACH
        } else {
            KNIFE_REACH
        }
    }

    /// `pl_mv05_w0_t6` (`0x4486c0`): the blade's bounce off a wall, until its
    /// clip ends.
    fn knife_bounce(&mut self) {
        if self.joint_move_late(KNIFE_BOUNCE_STEP) {
            self.aim_stage(1);
        }
    }

    /// `pl_mv05_w0_t2` (`0x4d4b40`): the knife's slash. It doesn't turn, spend
    /// anything or make a noise. From the seventh frame to the twelfth the
    /// blade's tip, ahead of the hand by what's left of the clip's 25 frames,
    /// finds a wall through `Sca_ck_hit` and bounces off it. Its search for a
    /// body (`0x4d7e30`), the hit's sound and blood, the splashes in water and
    /// id 0xD's sparks aren't ported.
    fn aim_slash(&mut self, key: u16) {
        if self.work.routine[3] == 0 {
            let clip = match self.work.status_flg >> 13 {
                1 => AIM_FIRE_CLIPS[0],
                2 => AIM_FIRE_CLIPS[1],
                4 => AIM_FIRE_CLIPS[2],
                _ => 0,
            };
            self.aim_clip(clip);
            // `Be_flg`'s top bits take the tilt.
            self.work.be_flg = self.work.be_flg & 0x1FFF_FFFF
                | (u32::from(self.work.status_flg) << 16) & 0xE000_0000;
        }
        let held = key & KEY_AIM != 0;
        if !held && self.work.move_cnt > self.tick.weapon.fire_cancel_frame() {
            self.aim_stage(1);
            return;
        }
        if self.joint_move() {
            self.aim_stage(1);
        }
        if !held && self.work.move_cnt > KNIFE_RELEASE_FRAME {
            self.aim_stage(1);
            self.work.spd = [0; 3];
            return;
        }
        let frame = self.work.move_cnt;
        if let (Some(hand), Some(sca)) =
            (self.chain_matrix(&KNIFE_CHAIN), self.tick.room.sca.as_ref())
        {
            // `ApplyMatrixSV`: the reach along the body's facing.
            let reach = (25 - i32::from(frame)) * 15;
            let rotation = self.work.body_rotation(self.player.angle);
            let ahead =
                |axis: usize| (i32::from(rotation[axis][2]) * i32::from(reach as i16)) >> 12;
            let tip = [
                hand.t[0] + i32::from(ahead(0) as i16),
                hand.t[1],
                hand.t[2] + i32::from(ahead(2) as i16),
            ];
            let ground = crate::sca::ground_at(sca, tip, KNIFE_WALL_RADIUS, KNIFE_WALL_MASK);
            if KNIFE_WALL_FRAMES.contains(&frame) && (ground.blocked || ground.under_record) {
                self.work.routine[2] = 6;
                self.work.routine[3] = 0;
                self.work.move_no = KNIFE_BOUNCE_CLIP;
                self.work.move_cnt = 0;
                self.work.hokan = KNIFE_BOUNCE_BLEND;
                self.joint_move_late(KNIFE_BOUNCE_STEP);
                self.player.queue_sound(SOUND_KNIFE_WALL);
                return;
            }
        }
        // `pl_w01_t2_move` (`0x4d5930`): the frame's sound.
        if self.work.now_seq & FRAME_SOUND != 0 {
            self.player
                .queue_sound(((self.work.now_seq & FRAME_SOUND_SLOT) >> 16) as u16);
        }
    }

    /// A part's `Workm` as the last draw left it, after the last tick's
    /// pushes ([`Work::parts`]), or on the pose before this tick's move when
    /// nothing was latched: the matrix an `Esp_call` on the part copies.
    fn drawn_part(&self, part: usize) -> Option<Matrix> {
        self.work
            .parts
            .get(part)
            .copied()
            .flatten()
            .or_else(|| self.part_matrix(part))
    }

    /// The parts' `Workm` as the last `Joint_trans2` left them: the pose,
    /// place and turn from before this tick's move.
    fn drawn_parts(&self) -> DrawnParts {
        [0, SHOT_PART, HAND_PART].map(|part| self.part_matrix(part))
    }

    /// The aim's splashes in water: `Pl_water`'s four rings on Move_cnt 1, 3
    /// and 5 while the water is above `Ground`, then one off each hand
    /// (parts 11 and 14) the water reaches, within 300 of part 11 and
    /// `hand14` of part 14. Their `Rnd` waits for [`Work::water_draws`].
    /// The original reads the parts' `Workm`, which is last tick's pose even
    /// where `Joint_move` has already run (`pl_mv05_w1_t2`), so `drawn` is
    /// [`Mover::drawn_parts`] from before the move.
    fn aim_splashes(&mut self, hand14: i32, drawn: &DrawnParts) {
        let water = self.work.water;
        if water == 0 {
            return;
        }
        if i32::from(water) < self.ground() && self.work.move_cnt < 6 && self.work.move_cnt & 1 != 0
        {
            self.pl_water(drawn[0]);
        }
        self.hand_splashes(hand14, drawn);
    }

    /// `Pl_water` (`0x4d49c0`): four rings (`Esp_call` kind 0x1a) a quarter
    /// turn apart on part 0's `Workm`, at the water's level. Their `Rnd`
    /// waits for [`Work::water_draws`].
    fn pl_water(&mut self, root: Option<Matrix>) {
        let dir = self.player.angle as i16;
        let root = root.map_or(self.player.pos, |part| part.t);
        let at = [root[0] as i16, self.work.water, root[2] as i16];
        for ring in 0..4u16 {
            self.work.water_draws.push(WaterDraw {
                effect: self.work.effects.len(),
                ring: Some(ring),
            });
            self.work.effects.push(EffectCall {
                kind: 0x1A,
                row: 0,
                size: 0,
                dir: dir.wrapping_add((ring * 0x400) as i16),
                at,
                motion: None,
                on: None,
                turn: None,
            });
        }
    }

    /// The splashes off the hands (parts 11 and 14) the water reaches: one
    /// `Esp_call` kind 0x1a, row 0, 300 along each hand's matrix, while
    /// `Water` is within 300 of part 11 and `hand14` of part 14. Each one's
    /// size is a `Rnd` (`* 4 + 0x60C`), which waits for
    /// [`Work::water_draws`]. `drawn` is [`Mover::drawn_parts`] from before
    /// the move, since the original reads the parts' `Workm`.
    fn hand_splashes(&mut self, hand14: i32, drawn: &DrawnParts) {
        let water = self.work.water;
        let dir = self.player.angle as i16;
        for (hand, reach) in [(drawn[1], 300), (drawn[2], hand14)] {
            let Some(hand) = hand else {
                continue;
            };
            if i32::from(water) < hand.t[1] + reach {
                let at = hand
                    .comp(&Matrix {
                        m: rot_matrix([0; 3]),
                        t: [0, 300, 0],
                    })
                    .t;
                self.work.water_draws.push(WaterDraw {
                    effect: self.work.effects.len(),
                    ring: None,
                });
                self.work.effects.push(EffectCall {
                    kind: 0x1A,
                    row: 0,
                    size: 0,
                    dir,
                    at: at.map(|axis| axis as i16),
                    motion: None,
                    on: None,
                    turn: None,
                });
            }
        }
    }

    /// `pl_w02_t2_move` and `pl_w03_t2_move`: the flash, then the smoke and the
    /// case thrown out, off the hand. These run after `Joint_move` on the frame
    /// it reached, but the hand's matrix is still last tick's, which this is
    /// called early to see.
    fn shot_effects(&mut self) {
        let frame = self.work.move_cnt.wrapping_add(1);
        let angle = self.player.angle as i16;
        if self.tick.weapon.id == GRENADES && frame == 1 {
            self.grenades();
            return;
        }
        for effect in self.tick.weapon.shot_effects(frame) {
            let Some(hand) = self.drawn_part(SHOT_PART) else {
                continue;
            };
            let point = effect.point.map(|axis| axis as i16);
            let at = crate::esp2::apply_matrix(&hand, point);
            for &(word, dir) in effect.calls {
                self.work.effects.push(EffectCall {
                    kind: (word >> 24) as u8,
                    row: (word >> 16) as u8,
                    size: word as u16,
                    dir: dir.map_or(0, |dir| angle.wrapping_add(dir)),
                    at,
                    motion: None,
                    on: Some((SHOT_PART as u8, hand, point)),
                    turn: None,
                });
            }
        }
        if self.tick.weapon.id == COLT
            && frame == 2
            && let Some(hand) = self.drawn_part(SHOT_PART)
        {
            let (word, point, turn) = COLT_SMOKE_3D;
            let point = point.map(|axis| axis as i16);
            self.work.effects.push(EffectCall {
                kind: (word >> 24) as u8,
                row: (word >> 16) as u8,
                size: word as u16,
                dir: 0,
                at: crate::esp2::apply_matrix(&hand, point),
                motion: None,
                on: Some((SHOT_PART as u8, hand, point)),
                turn: Some(turn),
            });
        }
    }

    /// `FUN_004d6150`'s first frame: a flash and five grenades out of the
    /// muzzle, spread across and up by their velocities. Aimed up, they take
    /// their speeds across from the second row of `ACROSS`.
    fn grenades(&mut self) {
        const ACROSS: [[i16; 3]; 2] = [[0, -120, -240], [250, 200, 150]];
        /// The grenades' `+8` and `+0xe`, `+0x10`, by which of the three
        /// columns of `ACROSS` each takes.
        const SPREAD: [(i8, i16, i16, usize); 5] = [
            (-10, 600, 0, 0),
            (-20, 400, 100, 1),
            (-20, 400, -100, 1),
            (-40, 250, 50, 2),
            (-40, 250, -50, 2),
        ];
        let Some(hand) = self.drawn_part(SHOT_PART) else {
            return;
        };
        let at = crate::esp2::apply_matrix(&hand, GRENADE_MUZZLE.map(|axis| axis as i16));
        let call = |word: u32, dir: i16, motion| EffectCall {
            kind: (word >> 24) as u8,
            row: (word >> 16) as u8,
            size: word as u16,
            dir,
            at,
            motion,
            on: None,
            turn: None,
        };
        self.work.effects.push(EffectCall {
            on: Some((
                SHOT_PART as u8,
                hand,
                GRENADE_MUZZLE.map(|axis| axis as i16),
            )),
            ..call(0x100_2000, 0, None)
        });
        let across = ACROSS[usize::from(self.work.status_flg & TILT_UP != 0)];
        let angle = self.player.angle as i16;
        for (accel, up, side, column) in SPREAD {
            let motion = crate::esp2::Launch {
                accel,
                velocity: [across[column], up, side],
                matrix: hand.m,
                t: hand.t,
                point: GRENADE_MUZZLE.map(|axis| axis as i16),
            };
            self.work
                .effects
                .push(call(0x20C_0A00, angle, Some(motion)));
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
            self.work.neck.flags &= 0xED;
        }
    }

    /// `pl_mv05_w1_t4`: the frame marked for it fills the magazine. The
    /// bursts' `pl_mv05_w2_t4` is the same on its own clip.
    fn aim_reload(&mut self) {
        if self.work.routine[3] == 0 {
            self.aim_clip(self.tick.weapon.reload_clip());
        }
        // `Workm` is the last `Joint_trans2`'s, from before this tick's frame.
        let hand = self.part_matrix(MAGAZINE_HAND);
        if self.joint_move() {
            self.aim_stage(1);
        }
        if self.work.now_seq & FRAME_SOUND != 0 {
            self.player
                .queue_sound(((self.work.now_seq & FRAME_SOUND_SLOT) >> 16) as u16);
        }
        if self.work.now_seq & FRAME_MAGAZINE != 0 && self.work.status_flg & STATUS_MAGAZINE != 0 {
            if self.tick.weapon.id == COLT {
                self.colt_cases(hand);
            } else {
                self.mag_set(hand);
            }
            self.work.status_flg &= !STATUS_MAGAZINE;
        }
        if self.work.now_seq & FRAME_RELOAD != 0 {
            self.work.reloaded = true;
            self.work.status_flg |= STATUS_MAGAZINE;
        }
    }

    /// `pl_mv05_w1_t4`'s call in place of `Mag_set` for the Colt: its spent
    /// cases out of the hand, along the body's facing.
    fn colt_cases(&mut self, hand: Option<Matrix>) {
        let Some(hand) = hand else {
            return;
        };
        let (word, point) = COLT_CASES;
        self.work.effects.push(EffectCall {
            kind: (word >> 24) as u8,
            row: (word >> 16) as u8,
            size: word as u16,
            dir: self.player.angle as i16,
            at: crate::esp2::apply_matrix(&hand, point),
            motion: None,
            on: Some((MAGAZINE_HAND as u8, hand, point)),
            turn: None,
        });
    }

    /// `Mag_set`: a gun with a magazine lets it go from the hand. The weapons
    /// that have one all share a row of `BYTE_0053a420`.
    fn mag_set(&mut self, hand: Option<Matrix>) {
        let Some(hand) = hand else {
            return;
        };
        if !matches!(self.tick.weapon.id, 2..=6 | 0xF | 0x10 | 0x13) {
            return;
        }
        let (x, z) = rotate(self.player.angle.wrapping_add(0x400), MAGAZINE_SPEED, 0);
        self.work.magazine = Some(Magazine {
            matrix: hand.comp(&Matrix {
                m: rot_matrix([0; 3]),
                t: [0, MAGAZINE_OFFSET, 0],
            }),
            speed: [x as i16, z as i16],
            ground: self.ground() as i16,
            fall: 0,
            bounces: 1,
        });
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

    fn pl_mv_forward(&mut self, drawn: Option<&DrawnParts>) {
        if self.work.routine[2] == 0 {
            self.start_motion(Bank::Weapon, self.life_clip(&WALK_CLIPS));
            self.work.noise |= 2;
        }
        self.spd_get();
        self.joint_move();
        if self.work.now_seq & FRAME_FOOTSTEP != 0 {
            self.work.noise |= 2;
        }
        self.footstep(0);
        self.wading_splashes(drawn);
        self.slow_in_deep_water();
        self.add_speed_xz();
    }

    /// `pl_mv_01` to `pl_mv_03`: in water more than 500 over the ground, a
    /// third off the step.
    fn slow_in_deep_water(&mut self) {
        if i32::from(self.work.water) < self.drawn_ground() - 500 {
            let spd = self.work.spd[0];
            self.work.spd[0] = spd - spd / 3;
        }
    }

    fn pl_mv_run(&mut self, drawn: Option<&DrawnParts>) {
        if self.work.routine[2] == 0 {
            self.start_motion(Bank::Weapon, self.life_clip(&RUN_CLIPS));
            self.work.noise |= 4;
        }
        self.spd_get();
        self.joint_move();
        if self.work.now_seq & FRAME_FOOTSTEP != 0 {
            self.work.noise |= 4;
        }
        self.footstep(1);
        self.wading_splashes(drawn);
        if self.work.d_life_u < 2 {
            self.work.spd[0] = self.work.spd[0].wrapping_sub(RUN_SLOWDOWN);
        }
        self.slow_in_deep_water();
        self.add_speed_xz();
    }

    /// `pl_mv03` (`0x4da7c0`). Every 15 ticks, unhurt, it swaps to the clip
    /// that looks at an enemy while one is in view, and back when none is,
    /// keeping the frame (the plain clip's on a multiple of 7).
    fn pl_mv_backward(&mut self, drawn: Option<&DrawnParts>) {
        if self.work.routine[2] == 0 {
            self.start_motion(Bank::Pld, self.life_clip(&BACK_CLIPS));
            self.work.back_look = false;
            self.work.back_wait = 0;
            self.work.noise |= 2;
        }
        if self.work.back_wait == 0 && self.work.d_life_u == 0 {
            let seen = self.enemy_in_view(BACK_LOOK_REACH, BACK_LOOK_VIEW);
            if seen != self.work.back_look {
                let frame = if seen {
                    self.work.move_cnt
                } else {
                    self.work.move_cnt / BACK_LOOK_FRAME_STEP * BACK_LOOK_FRAME_STEP
                };
                self.work.move_no = if seen { BACK_LOOK_CLIP } else { BACK_CLIPS[0] };
                self.work.back_look = seen;
                // Two `Spd_get`s move the speed's base onto the new clip's
                // frame, so this tick's own one finds no step.
                self.work.move_cnt = 0;
                self.work.hokan = FULL_BLEND;
                self.spd_get();
                self.work.move_cnt = frame;
                self.spd_get();
            }
            self.work.back_wait = BACK_LOOK_WAIT;
        }
        self.work.back_wait = self.work.back_wait.wrapping_sub(1);
        self.spd_get();
        self.joint_move();
        if self.work.now_seq & FRAME_FOOTSTEP != 0 {
            self.work.noise |= 2;
        }
        self.footstep(0);
        self.wading_splashes(drawn);
        self.slow_in_deep_water();
        self.add_speed_xz();
    }

    /// `FUN_004daa60`: whether an enemy in play (not the second player) stands
    /// within `reach` of the player, alive, upright, in the view of `view`
    /// either side, in clear sight head to head, and facing the player's way
    /// back rather than away. None while the room has no enemies.
    fn enemy_in_view(&self, reach: u32, view: u16) -> bool {
        if self.tick.enemies == 0 {
            return false;
        }
        let sca = self.tick.room.sca.as_ref();
        let from = self.work.neck.position.unwrap_or(self.player.pos);
        let [x, _, z] = self.player.pos;
        let angle = self.player.angle;
        self.tick.neck_targets.iter().any(|target| {
            if target.index < 2 || target.kind & 0xE000 != 0 || target.life < 0 {
                return false;
            }
            let (dx, dz) = (x - target.pos[0], z - target.pos[2]);
            let distance =
                crate::sca::square_root0(dz.wrapping_mul(dz).wrapping_add(dx.wrapping_mul(dx)))
                    as u32;
            distance < reach
                && !sca.is_some_and(|sca| {
                    crate::zombie2::sca_ck_line(&sca.records, from, target.neck, NECK_LINE_MASK)
                })
                && direction_ck(x, z, target.pos[0], target.pos[2])
                    .wrapping_sub(angle)
                    .wrapping_add(view)
                    & 0xFFF
                    < view * 2
                && target.angle.wrapping_sub(angle).wrapping_add(0x400) & 0xFFF >= 0x800
        })
    }

    /// `pl_mv_04` (`0x4DAE70`): the turn on the spot. Its footfalls are a
    /// step's noise (`0x989EEE |= 2`), which a listening licker hears.
    fn pl_mv_rotate(&mut self, drawn: Option<&DrawnParts>) {
        if self.work.routine[2] == 0 {
            self.start_motion(Bank::Weapon, self.life_clip(&ROTATE_CLIPS));
            self.work.spd = [0; 3];
        }
        self.joint_move();
        if self.work.now_seq & FRAME_FOOTSTEP != 0 {
            self.work.noise |= 2;
        }
        self.footstep(0);
        self.wading_splashes(drawn);
    }

    /// The walk's, run's, back step's and turn's splashes in water
    /// (`pl_mv_01`, `_02`, `_03` and `_04`, after `Snd_se_walk`): off the
    /// hands, within 300 of both, on odd `Move_cnt` (the frame `Joint_move`
    /// has just moved to).
    fn wading_splashes(&mut self, drawn: Option<&DrawnParts>) {
        if let Some(drawn) = drawn
            && self.work.move_cnt & 1 != 0
        {
            self.hand_splashes(300, drawn);
        }
    }

    /// [`Mover::drawn_parts`] for [`Mover::wading_splashes`], taken before
    /// the tick turns or moves the player, and only in water.
    fn wading_parts(&self) -> Option<DrawnParts> {
        (self.work.water != 0).then(|| self.drawn_parts())
    }

    /// `pl_mv06` (`0x4DAFF0`): the crouching pick-up. Sub 0 and 1 play the
    /// crouch and sub 2 opens the get-item window, which the game shows
    /// with the player held in sub 3. The window's close runs `Pl_init`, so
    /// the stand-up in sub 4 to 6 (the crouch played backwards, then
    /// routine 1, 0) is reached only if something sets sub 4, which the
    /// pick-up doesn't. `pl_br06` (`0x4DBFD0`) reads no keys.
    fn pl_mv_item(&mut self) {
        let reverse = match self.work.routine[2] {
            0 => {
                self.start_motion(Bank::Pld, ITEM_CLIP);
                self.work.spd[..2].fill(0);
                false
            }
            1 => false,
            2 => {
                self.work.item_window = true;
                self.work.routine[2] = 3;
                return;
            }
            4 => {
                self.start_motion(Bank::Pld, ITEM_CLIP);
                self.work.routine[2] = 5;
                true
            }
            5 => true,
            6 => {
                self.work.routine = [MODE_MOVE, MOVE_IDLE, 0, 0];
                return;
            }
            _ => return,
        };
        if self.joint_move_late_reversed(BLEND_STEP, reverse) {
            self.work.routine[2] += 1;
        }
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
        self.spd_get_reversed(reverse);
    }

    pub(crate) fn spd_get_reversed(&mut self, reverse: bool) {
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
    /// `Routine_1` 4 and 5, the falls on `pSub1`'s clips, play as the plain
    /// hit when no enemy has handed the player its clips
    /// (`docs/internals/deviations/re2.md`).
    fn pl_damage(&mut self) {
        let has_sub1 = !self.tick.banks.held.0.is_empty();
        match self.work.routine[1] {
            4 if has_sub1 => self.pl_dm02(),
            5 if has_sub1 => self.pl_dm03(),
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
                let sound = (u32::from(back) + 1) << 16 | 0x400_0001;
                if !self.damage_step(STAGGER_CLIPS[usize::from(back)], speed, sound) {
                    return;
                }
                if self.work.hokan != 0 {
                    self.add_speed_xz();
                }
                self.damage_joint(DAMAGE_BLEND_STEP);
            }
            6 => self.pl_die(),
            _ => {
                let sound = if self.work.routine[1] == 0 {
                    0x400_0001
                } else {
                    0x401_0001
                };
                if self.damage_step(DAMAGE_CLIP, [0; 3], sound) {
                    self.damage_joint(DAMAGE_BLEND_STEP);
                }
            }
        }
    }

    /// `Gacha_ch`: a button pressed this tick, which shortens a fall's wait.
    fn gacha(&self) -> u16 {
        u16::from(self.work.key_trg & 0x34F != 0)
    }

    /// The wait on the floor before a fall's getting up: longer the less life
    /// is left.
    fn floor_wait(&self) -> u16 {
        let life = self.tick.life;
        let mut wait: u16 = 0x1E;
        if life < 0x96 {
            wait = 0x3C;
        }
        if life < 0x46 {
            wait += 0x32;
        }
        if life < 0x1E {
            wait += 0x32;
        }
        if life == 0 {
            wait += 100;
        }
        wait
    }

    /// Start a fall's clip on `pSub1`'s bank.
    fn sub1_clip(&mut self, clip: u8) {
        self.work.bank = Bank::Held;
        self.work.move_no = clip;
        self.work.move_cnt = 0;
        self.work.hokan = DAMAGE_BLEND;
    }

    /// `pl_dm02`, `Pl_damage`'s `Routine_1` 4: knocked down backwards on the
    /// enemy's clips (`pSub1`), sliding, onto a wall if one stops the slide
    /// (`Sca_info`'s 1), the wait on the floor (shorter for each button
    /// pressed) and the getting up, ending in the player's own clip 6 played
    /// backwards.
    fn pl_dm02(&mut self) {
        const LATE: u16 = 0x400;
        let mut sub = self.work.routine[2];
        if sub == 0 {
            self.work.spd = [-800, 0, 0];
            self.sub1_clip(2);
            self.work.neck.flags |= 0x12;
            self.work.routine[2] = 1;
            self.work.damage_cnt |= DAMAGE_LOCK;
            self.work.sounds.push(0x401_0001);
            self.work.be_flg &= !4;
            sub = 1;
        }
        match sub {
            1 => {
                if self.work.hokan == 0 {
                    self.work.routine[2] = 2;
                }
                self.add_speed_xz();
                self.work.bank = Bank::Held;
                self.joint_move_late(LATE);
                self.work.spd[0] = self.work.spd[0].wrapping_add(0x96);
                if self.work.sca_hit {
                    self.work.routine[2] = 5;
                }
            }
            2 | 3 => {
                if sub == 2 {
                    self.sub1_clip(3);
                    self.work.routine[2] = 3;
                    self.work.sounds.push(0x22A_0001);
                }
                self.add_speed_xz();
                self.work.bank = Bank::Held;
                let ended = self.joint_move_late(LATE);
                self.work.spd[0] = self.work.spd[0].wrapping_add(0xF);
                self.work.routine[2] += u8::from(ended);
                if self.work.spd[0] > 0 {
                    self.work.spd[0] = 0;
                }
            }
            4 => {
                self.work.routine = [MODE_MOVE, 0, 0, 0];
                self.work.damage_cnt = 0;
                self.work.neck.flags &= 0xED;
            }
            5 | 6 => {
                if sub == 5 {
                    self.sub1_clip(0);
                    self.work.routine[2] = 6;
                    self.work.sounds.push(0x229_0001);
                    self.work.sounds.push(0x402_0001);
                }
                self.work.bank = Bank::Held;
                let ended = self.joint_move_late(LATE);
                self.work.routine[2] += u8::from(ended);
            }
            7 | 8 => {
                if sub == 7 {
                    self.sub1_clip(4);
                    self.work.routine[2] = 8;
                    self.work.timer0 = self.floor_wait();
                }
                self.work.bank = Bank::Held;
                self.joint_move_late_reversed(LATE, true);
                let gacha = self.gacha();
                self.work.timer0 = self.work.timer0.wrapping_sub(gacha * 5 + 1);
                if (self.work.timer0 as i16) < 0 {
                    self.work.routine[2] = 9;
                }
            }
            9 | 10 => {
                if sub == 9 {
                    self.sub1_clip(1);
                    self.work.routine[2] = 10;
                    self.work.damage_cnt = 0;
                }
                self.work.bank = Bank::Held;
                if self.joint_move_late(LATE) {
                    self.work.routine[2] = 0xB;
                    self.work.bank = Bank::Pld;
                    self.work.move_no = 6;
                    self.work.move_cnt = 0;
                    self.work.hokan = DAMAGE_BLEND;
                }
            }
            0xB => {
                self.work.bank = Bank::Pld;
                if self.joint_move_late_reversed(LATE, true) {
                    self.work.routine[2] = 4;
                }
            }
            _ => {}
        }
    }

    /// `pl_dm03`, `Pl_damage`'s `Routine_1` 5: knocked down forwards on the
    /// enemy's clips (`pSub1`), the wait on the floor (shorter for each
    /// button pressed) and the getting up, ending in the player's own clip 6
    /// played backwards.
    fn pl_dm03(&mut self) {
        const LATE: u16 = 0x400;
        let mut sub = self.work.routine[2];
        if sub == 0 {
            self.work.damage_cnt |= DAMAGE_LOCK;
            self.sub1_clip(5);
            self.work.routine[2] = 1;
            self.work.spd = [400, 0, 0];
            self.work.neck.flags |= 0x12;
            self.work.sounds.push(0x402_0001);
            self.work.be_flg &= !4;
            sub = 1;
        }
        match sub {
            1 => {
                if self.work.move_cnt == 5 && self.work.hokan == 3 {
                    self.work.sounds.push(0x402_0001);
                }
                self.add_speed_xz();
                self.work.bank = Bank::Held;
                let ended = self.joint_move_late(LATE);
                self.work.routine[2] += u8::from(ended);
                let slow = if self.work.hokan == 0 { 0x32 } else { 0x14 };
                self.work.spd[0] = self.work.spd[0].wrapping_sub(slow);
                if self.work.spd[0] < 0 {
                    self.work.spd[0] = 0;
                }
            }
            2 | 3 => {
                if sub == 2 {
                    self.sub1_clip(4);
                    self.work.routine[2] = 3;
                    self.work.timer0 = self.floor_wait();
                }
                self.work.bank = Bank::Held;
                self.joint_move_late_reversed(LATE, true);
                let gacha = self.gacha();
                self.work.timer0 = self.work.timer0.wrapping_sub(gacha * 3 + 1);
                if (self.work.timer0 as i16) < 0 {
                    self.work.routine[2] = 4;
                }
            }
            4 | 5 => {
                if sub == 4 {
                    self.sub1_clip(1);
                    self.work.routine[2] = 5;
                    self.work.damage_cnt = 0;
                }
                self.work.bank = Bank::Held;
                if self.joint_move_late(LATE) {
                    self.work.routine[2] = 6;
                    self.work.bank = Bank::Pld;
                    self.work.move_no = 6;
                    self.work.move_cnt = 0;
                    self.work.hokan = DAMAGE_BLEND;
                }
            }
            6 => {
                self.work.bank = Bank::Pld;
                if self.joint_move_late_reversed(LATE, true) {
                    self.work.routine = [MODE_MOVE, 0, 0, 0];
                    self.work.damage_cnt = 0;
                    self.work.neck.flags &= 0xED;
                }
            }
            _ => {}
        }
    }

    /// `Pl_die`, and `pl_die00` for `Pl_damage`'s `Routine_1` 6: the cry and the
    /// fall, then `Pl_dead`, which only grows the shadow into a pool of blood
    /// that the game over's `Die_vcut_set` culls.
    fn pl_die(&mut self) {
        // `Foot_set_pl` reads last tick's world matrix from the parts.
        let planted = self.foot_position();
        match self.work.routine[2] {
            0 => {
                self.work.routine[2] = 1;
                self.work.bank = Bank::Pld;
                self.work.move_no = FALL_CLIP;
                self.work.move_cnt = 0;
                self.work.hokan = FULL_BLEND;
                self.work.damage_cnt |= DAMAGE_LOCK;
                self.work.spd = [0; 3];
                self.work.neck.flags = 1;
                self.work.sounds.push(0x403_0001);
                self.damage_joint(BLEND_STEP);
            }
            1 => self.damage_joint(BLEND_STEP),
            2 => self.work.routine = [MODE_DEAD_BODY, 0, 0, 0],
            _ => {}
        }
        if self.work.hokan == 0 {
            self.foot_set(planted);
        }
    }

    /// `Pl_dead`, without the shadow's pool (see [`Self::pl_die`]).
    fn pl_dead(&mut self) {
        if self.work.routine[1] == 0 {
            self.work.routine[1] = 1;
            self.work.timer0 = DEAD_POOL_TICKS;
        } else if self.work.routine[1] != 1 {
            return;
        }
        if self.work.timer0 == 0 {
            self.work.routine[1] = 2;
        }
        self.work.timer0 = self.work.timer0.wrapping_sub(1);
    }

    /// The opening of `Pl_damage`'s plain cases: start `clip`, or at the end
    /// of it hand back to the pad. False when no frame plays this tick.
    fn damage_step(&mut self, clip: u8, speed: [i16; 3], sound: u32) -> bool {
        match self.work.routine[2] {
            0 => {
                self.work.routine[2] = 1;
                self.work.bank = Bank::Pld;
                self.work.move_no = clip;
                self.work.move_cnt = 0;
                self.work.hokan = DAMAGE_BLEND;
                self.work.spd = speed;
                self.work.damage_cnt |= DAMAGE_LOCK;
                self.work.neck.flags |= 0x12;
                self.work.be_flg &= !4;
                self.work.sounds.push(sound);
                true
            }
            1 => true,
            2 => {
                self.work.routine = [MODE_MOVE, MOVE_IDLE, 0, 0];
                self.work.status_flg |= STATUS_RECOVERED;
                self.work.damage_cnt = 0;
                self.work.neck.flags &= 0xED;
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
        let (clips, keyframes) = self.tick.banks.get(self.work.bank);
        let Some(clip) = clips.get(usize::from(self.work.move_no)) else {
            return true;
        };
        if clip.frames.is_empty() {
            return true;
        }
        if !reverse
            && let Some((next, frame)) =
                frame_past_end(clips, usize::from(self.work.move_no), self.work.move_cnt)
        {
            return self.joint_move_past_end(next, frame, late);
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
            Bank::Held => ClipSource::Damage,
        };
        jseq2_step(
            self.work,
            keyframes.get(usize::from(clip.frames[index].keyframe)),
            late,
        );
        let anim = &mut self.player.anim;
        anim.clip = usize::from(self.work.move_no);
        anim.frame = count;
        anim.blend_counter = u16::from(self.work.hokan);
        anim.blend_step = late;
        anim.re2_blend = true;
        anim.reverse = reverse;
        anim.timing = 0;
        let ended = anim.update(clips);
        // `J_seq` writes the pose into the parts at once. The queued step names
        // a keyframe of this bank, and a hold's end switches the bank after its
        // last frame (`FUN_00463210`'s release), so fold it now.
        anim.pose_keyframe(clips, keyframes);
        self.work.move_cnt = anim.frame as u8;
        self.work.hokan = anim.blend_counter as u8;
        ended
    }

    /// A part's `M` before `Joint_trans2` turns it into a `Workm`: `J_seq2`'s
    /// matrix, or `RotMatrix` of the pose's angles. A `J_seq2` body's
    /// `rotation` is not read, so a turn added to the pose's angles (the
    /// neck's, `Neck::apply`) would be lost: no `J_seq2` body (the lickers)
    /// calls `Rot_neck_em`.
    fn part_rotation(&self, part: usize, rotation: [i16; 3]) -> [[i16; 3]; 3] {
        debug_assert!(
            self.work.jseq2.is_none() || self.work.neck.turn == [0; 2],
            "a J_seq2 body's neck turn is not drawn"
        );
        self.work
            .jseq2
            .as_ref()
            .and_then(|parts| parts.get(part))
            .copied()
            .unwrap_or_else(|| rot_matrix(rotation))
    }

    /// Play through `J_seq2` from the pose `J_seq` left: each part's `M` the
    /// raw `RotMatrix` of its angles, as `Em_24`'s init (`FUN_0046cfa0`,
    /// `Joint_move`) leaves them.
    pub(crate) fn start_jseq2(&mut self) {
        let parts = self.posed_rotations().into_iter().map(rot_matrix).collect();
        self.work.jseq2 = Some(parts);
    }

    /// `Joint_move` with `Move_cnt` at or past its clip's end: `pSeq_ptr` reads
    /// on into the clips after it (`frame` of clip `next`), and `J_seq` ends
    /// the clip there, leaving `Move_cnt` 0.
    fn joint_move_past_end(&mut self, next: usize, frame: usize, late: u16) -> bool {
        let (clips, keyframes) = self.tick.banks.get(self.work.bank);
        self.work.now_seq = clips[next].frames[frame].flags;
        jseq2_step(
            self.work,
            keyframes.get(usize::from(clips[next].frames[frame].keyframe)),
            late,
        );
        let anim = &mut self.player.anim;
        anim.clip = next;
        anim.frame = frame;
        anim.blend_counter = u16::from(self.work.hokan);
        anim.blend_step = late;
        anim.re2_blend = true;
        anim.reverse = false;
        anim.timing = 0;
        anim.update(clips);
        anim.pose_keyframe(clips, keyframes);
        anim.clip = usize::from(self.work.move_no);
        anim.frame = 0;
        self.work.move_cnt = 0;
        self.work.hokan = anim.blend_counter as u8;
        true
    }

    /// `Set_base_pos`: where this body's clip must start for it to be here now.
    pub(crate) fn set_base_pos(&mut self) -> [i16; 3] {
        let [x, y, z] = self.null_pos(0);
        let pos = self.player.pos;
        self.work.base = [
            pos[0].wrapping_sub(x) as i16,
            pos[1].wrapping_sub(y) as i16,
            pos[2].wrapping_sub(z) as i16,
        ];
        self.work.base
    }

    /// `Null_pos_set`: the body stands where the clip's frame puts it from its
    /// base, turned by `turn` more than it faces.
    pub(crate) fn null_pos_set(&mut self, turn: u16) {
        let [x, y, z] = self.null_pos(turn);
        let base = self.work.base.map(i32::from);
        self.player.pos = [base[0] + x, base[1] + y, base[2] + z];
    }

    /// `get_null_pos`, turned by `RotMatrixY`.
    fn null_pos(&self, turn: u16) -> [i32; 3] {
        let (clips, keyframes) = self.tick.banks.get(self.work.bank);
        let (move_no, move_cnt) = (self.work.move_no, self.work.move_cnt);
        let index = match clips.get(usize::from(move_no)) {
            Some(clip) => clip
                .frames
                .iter()
                .skip(usize::from(move_cnt))
                .find(|frame| frame.flags & FRAME_SKIPPED == 0)
                .map(|frame| frame.keyframe),
            None => stray_keyframe(clips, move_no, move_cnt),
        };
        let Some(keyframe) = index.and_then(|index| keyframes.get(usize::from(index))) else {
            return [0; 3];
        };
        let [x, y, z] = keyframe.vertex.map(i32::from);
        let (x, z) = rotate(self.player.angle.wrapping_add(turn), x, z);
        [x, y, z]
    }

    /// `FUN_0045a820`, `FUN_0045b000`: the zombie's table of what it does to the
    /// player it holds.
    fn em10_held(&mut self, held: &mut Held<'_>) {
        if held.holder_id == crate::zombie2::DOG_ID {
            self.em20_pldie();
            return;
        }
        if held.holder_id == crate::zombie2::CROW_ID {
            self.em21_held(held);
            return;
        }
        if held.holder_id == crate::zombie2::MRX_ID {
            self.em2a_held(held);
            return;
        }
        if held.holder_id == crate::zombie2::ALLIGATOR_ID {
            self.em23_held(held);
            return;
        }
        if held.holder_id == crate::zombie2::G1_ID {
            self.em30_killed(held);
            return;
        }
        if held.holder_id == crate::zombie2::G3_ID {
            if self.work.routine[0] == MODE_HELD_DIE {
                self.em33_killed(held);
            } else {
                self.em33_held(held);
            }
            return;
        }
        if held.holder_id == crate::zombie2::G2_ID {
            if self.work.routine[0] == MODE_HELD_DIE {
                self.em31_killed(held);
            } else {
                self.em31_held(held);
            }
            return;
        }
        if matches!(
            held.holder_id,
            crate::zombie2::IVY_ID | crate::zombie2::POISON_IVY_ID
        ) {
            self.em2e_held(held);
            return;
        }
        if self.work.routine[0] == MODE_HELD_DIE {
            self.held_dying(held);
            return;
        }
        match self.work.routine[1] {
            0 | 1 => self.held_standing(held),
            2 | 3 => self.held_low(held),
            _ => self.held_dying(held),
        }
    }

    /// `FUN_00463210`: with a crow on the head, until it lets go.
    fn em21_held(&mut self, held: &mut Held<'_>) {
        const LATE: u16 = 0x200;
        match self.work.routine[2] {
            0 | 1 => {
                if self.work.routine[2] == 0 {
                    self.work.bank = Bank::Held;
                    self.held_clip(0, 7);
                    self.work.routine[2] = 1;
                }
                if self.joint_move_late(LATE) {
                    self.held_clip(1, 7);
                    self.work.routine[2] = 2;
                }
            }
            2 => {
                self.joint_move_late(LATE);
            }
            3 | 4 => {
                if self.work.routine[2] == 3 {
                    if *held.life < 0 {
                        self.work.routine = [MODE_DEAD, 0, 0, 0];
                        return;
                    }
                    self.held_clip(2, 7);
                    self.work.routine[2] = 4;
                }
                if self.joint_move_late(LATE) {
                    self.held_clip(1, 7);
                    self.held_end(held);
                }
            }
            _ => {}
        }
    }

    /// `FUN_00487c70`: the ivy's table, on its `pSub1` clips. `Routine_1` 0
    /// pulls the player in, 1 holds them while it squeezes, 2 lets them go.
    fn em2e_held(&mut self, held: &mut Held<'_>) {
        const LATE: u16 = 0x40;
        self.work.bank = Bank::Held;
        match self.work.routine[1] {
            0 => {
                if self.work.routine[2] == 0 {
                    self.work.routine[2] = 1;
                    self.held_clip(0, 0x3F);
                    self.work.neck.flags |= 1;
                    let [x, _, z] = held.holder;
                    let (dx, dz) = (self.player.pos[0] - x, self.player.pos[2] - z);
                    let distance = crate::sca::square_root0(
                        dz.wrapping_mul(dz).wrapping_add(dx.wrapping_mul(dx)),
                    );
                    self.work.timers[1] = (distance as i16).wrapping_sub(0x5BF) as u16;
                    self.work.timers[2] = 7;
                    self.work.sounds.push(0x401_0001);
                }
                let left = self.work.timers[2] as u8;
                let reach = if left == 0 {
                    0
                } else {
                    self.work.timers[2] = u16::from(left - 1);
                    (i32::from(self.work.timers[1] as i16) >> 3) * i32::from(left as i8)
                };
                self.em2e_place(held, reach + 0x5BF);
                if self.joint_move_late(LATE) {
                    self.work.routine[1] = 1;
                    self.work.routine[2] = 0;
                    self.work.routine[3] = 0;
                }
            }
            1 => {
                if self.work.routine[2] == 0 {
                    self.work.routine[2] = 1;
                    self.held_clip(1, 0x1F);
                    self.work.timers[2] = 0x1F;
                } else if self.work.routine[2] == 2 {
                    // `Timer3` starts at 1, so the squeeze shows this tick.
                    self.work.routine[2] = 1;
                    self.work.routine[3] = 0;
                    self.work.sounds.push(0x402_0001);
                    self.em2e_blood();
                }
                let left = self.work.timers[2] as u8;
                let reach = if left == 0 {
                    0
                } else {
                    self.work.timers[2] = u16::from(left - 1);
                    (i32::from(left as i8) * 0x11C) / 0x1F
                };
                self.em2e_place(held, reach + 0x4A3);
                if self.joint_move_late(0x80) {
                    self.work.hokan = 0x1F;
                }
            }
            2 => {
                if self.work.routine[2] == 0 {
                    self.work.routine[2] = 1;
                    self.held_clip(4, 0x1F);
                    self.spd_get();
                    self.joint_move_late(LATE);
                    self.work.timers[0] = 0x4A3;
                    return;
                }
                self.spd_get();
                if self.joint_move_late(LATE) {
                    self.work.routine = [MODE_MOVE, 0, 0, 0];
                    self.work.damage_cnt &= 0x7F;
                    self.work.be_flg &= !0x42;
                    self.work.bank = Bank::Weapon;
                    self.work.neck.flags &= 0xED;
                    held.released = true;
                }
                self.add_speed_xz_at(0x800);
                self.work.timers[0] = self.work.timers[0].wrapping_add(self.work.spd[0] as u16);
                if self.work.timers[0] as i16 > 0x739 {
                    self.work.be_flg &= !0x42;
                }
            }
            3 => {
                if self.work.routine[2] == 0 {
                    self.work.routine[2] = 1;
                    self.held_clip(3, 0x3F);
                    self.work.neck.flags |= 1;
                }
                self.joint_move_late(LATE);
            }
            5 => {
                if self.work.routine[2] == 0 {
                    self.work.routine[2] = 1;
                    self.held_clip(2, 0x3F);
                    self.work.be_flg |= 8;
                } else if self.work.routine[2] != 1 {
                    return;
                }
                self.null_pos_set(0x800);
                let ended = self.joint_move_late(LATE);
                self.work.routine[2] += u8::from(ended);
            }
            6..=8 => {}
            _ => match self.work.routine[2] {
                0 => {
                    self.work.routine[2] = 1;
                    self.held_clip(0, 0x3F);
                    self.work.neck.flags |= 1;
                }
                1 => {
                    self.null_pos_set(0x800);
                    if self.joint_move_late(LATE) {
                        self.work.routine[2] = 2;
                        self.held_clip(1, 0x3F);
                    }
                }
                2 => {
                    self.null_pos_set(0x800);
                    if self.joint_move_late(LATE) {
                        self.work.routine[1] = 2;
                        self.work.routine[2] = 0;
                        self.work.routine[3] = 0;
                    }
                }
                _ => {}
            },
        }
    }

    /// `FUN_004887d0`: the player `reach` out along the ivy's head.
    fn em2e_place(&mut self, held: &Held<'_>, reach: i32) {
        let m = rot_matrix([0, held.holder_aim as i16, 0]);
        let turned = |row: usize| (i32::from(m[row][0]) * reach) >> 12;
        self.player.pos[0] = held.holder[0] + i32::from(turned(0) as i16);
        self.player.pos[2] = held.holder[2] + i32::from(turned(2) as i16);
    }

    /// `FUN_00488570(player, 0, 8)`: blood at the player's first part.
    fn em2e_blood(&mut self) {
        let [x, _, z] = self.player.pos;
        self.part_effect(4, 0x0F, 0x1400, direction_ck(x, z, x, z) as i16, 0, [0; 3]);
    }

    /// `FUN_004805e0`: Mr. X's table, on his `pSub1` clips. `Routine_1` 0 and
    /// 1 are the blow that throws the player back or turns them round, and 2
    /// is the kill.
    fn em2a_held(&mut self, held: &mut Held<'_>) {
        self.work.bank = Bank::Held;
        match self.work.routine[1] {
            0 => self.em2a_thrown(),
            2 => self.em2a_killed(held),
            _ => self.em2a_turned(),
        }
    }

    /// `FUN_0046a710`, `em_damage_table_real[0x13]`: the alligator's jaws.
    /// `Routine_1` 1 drags the player to it, 2 throws them back from the
    /// front and 3 from behind, on the room's clips.
    fn em23_held(&mut self, held: &mut Held<'_>) {
        match self.work.routine[1] {
            1 => self.em23_dragged(held),
            2 => self.em23_thrown(),
            3 => self.em23_thrown_round(),
            _ => self.em23_killed(),
        }
    }

    /// `FUN_0046a710`'s other `Routine_1`s: caught at the corridor's end. It
    /// stands on its own clip until the alligator sets its `Timer0`, then
    /// plays the alligator's clip from the base it set. `fg_stop`'s
    /// `0x1000000` isn't ported.
    fn em23_killed(&mut self) {
        match self.work.routine[2] {
            0 | 1 => {
                if self.work.routine[2] == 0 {
                    self.work.timers[0] = 0;
                    self.work.routine[2] = 1;
                }
                self.work.bank = Bank::Weapon;
                self.work.move_no = 2;
                self.work.move_cnt = 0;
                self.work.hokan = 3;
                self.joint_move_late(0x400);
                if self.work.timers[0] != 0 {
                    self.work.move_no = 1;
                    self.work.move_cnt = 0;
                    self.work.hokan = 0;
                    self.work.routine[2] = 2;
                }
            }
            2 => {
                self.work.bank = Bank::Held;
                self.null_pos_set(0);
                if self.work.move_cnt == 3 {
                    self.work.sounds.push(0x402_0001);
                }
                let ended = self.joint_move_late(0x1000);
                self.work.routine[2] += u8::from(ended);
            }
            3 => self.work.damage_cnt = 0x80,
            _ => {}
        }
    }

    /// `FUN_0046acf0(step)`: the drag's blood, its first three ticks. Each step
    /// takes three of the player's parts from its first and a point on each,
    /// as `Esp_call`'s matrix and vector: blood rows 1, 2 and 1 toward the
    /// step's way, then rows 2, 0 and 0 a quarter round from it. The parts
    /// are posed as the last draw left them, before this tick's move.
    fn em23_drag_blood(&mut self, step: u8) {
        /// The way, the first part, and the points on it and the two after.
        const STEPS: [(i16, usize, [[i32; 3]; 3]); 3] = [
            (0x850, 4, [[0, 0, 0], [0, 0, 0], [0, 200, 0]]),
            (0x536, 3, [[0, 200, 0], [0, 300, 0], [0, 200, 0]]),
            (0x9E6, 1, [[-300, -500, 200], [0, 0, 200], [-200, 600, 400]]),
        ];
        /// Row and size of each call, and whether it turns a quarter.
        const CALLS: [(u8, u16, bool, usize); 6] = [
            (1, 0x1800, false, 0),
            (2, 0x1000, false, 1),
            (1, 0x1800, false, 2),
            (2, 0x2000, true, 0),
            (0, 0x1800, true, 1),
            (0, 0x1000, true, 2),
        ];
        let Some(&(dir, first, points)) = STEPS.get(usize::from(step)) else {
            return;
        };
        for (row, size, turned, index) in CALLS {
            let dir = if turned { dir.wrapping_add(0x400) } else { dir };
            self.part_effect(0, row, size, dir, first + index, points[index]);
        }
    }

    /// `FUN_0046a710`'s `Routine_1` 1: caught by its reach, and pulled in,
    /// bleeding over its first three ticks. `fg_system`'s `0x4000000` isn't
    /// ported.
    fn em23_dragged(&mut self, held: &mut Held<'_>) {
        if self.work.routine[2] == 0 {
            self.work.sounds.push(0x403_0001);
            self.work.move_no = 0;
            self.work.move_cnt = 3;
            self.work.hokan = 0;
            self.work.routine[3] = 0;
            self.work.routine[2] = 1;
        } else if self.work.routine[2] != 1 {
            return;
        }
        if self.work.routine[3] < 3 {
            self.em23_drag_blood(self.work.routine[3]);
            self.work.routine[3] += 1;
        }
        let [x, _, z] = held.holder;
        let to = direction_ck(x, z, self.player.pos[0], self.player.pos[2]);
        self.player.angle = goto(self.player.angle, to, 0x80);
        self.work.bank = Bank::Weapon;
        let ended = self.joint_move_late(0x400);
        self.work.routine[2] += u8::from(ended);
    }

    /// `FUN_0046a710`'s `Routine_1` 2: thrown back, sliding to a stop.
    fn em23_thrown(&mut self) {
        const LATE: u16 = 0x400;
        self.work.bank = Bank::Room;
        match self.work.routine[2] {
            0 | 1 => {
                if self.work.routine[2] == 0 {
                    self.work.sounds.push(0x401_0001);
                    self.work.spd = [-800, 0, 0];
                    self.work.move_no = 0x12;
                    self.work.move_cnt = 0;
                    self.work.hokan = 3;
                    self.work.routine[2] = 1;
                    self.work.damage_cnt |= 0x80;
                    self.work.neck.flags |= 0x12;
                }
                if self.work.hokan == 0 {
                    self.work.routine[2] = 2;
                }
                self.add_speed_xz();
                self.joint_move_late(LATE);
                self.work.spd[0] = self.work.spd[0].wrapping_add(0x96);
            }
            2 | 3 => {
                if self.work.routine[2] == 2 {
                    self.work.move_no = 0x13;
                    self.work.move_cnt = 0;
                    self.work.hokan = 3;
                    self.work.routine[2] = 3;
                }
                self.add_speed_xz();
                let first = self.joint_move_late(LATE);
                let second = self.joint_move_late(LATE);
                self.work.spd[0] = self.work.spd[0].wrapping_add(0xF);
                self.work.routine[2] += u8::from(first) + u8::from(second);
                if self.work.spd[0] > 0 {
                    self.work.spd[0] = 0;
                }
            }
            4 => {
                self.work.routine = [MODE_MOVE, 0, 0, 0];
                self.work.damage_cnt = 0;
                self.work.neck.flags &= 0xED;
            }
            _ => {}
        }
    }

    /// `FUN_0046a710`'s `Routine_1` 3: thrown forward from behind, then
    /// back to the player's own clip.
    fn em23_thrown_round(&mut self) {
        const LATE: u16 = 0x400;
        match self.work.routine[2] {
            0 | 1 => {
                self.work.bank = Bank::Room;
                if self.work.routine[2] == 0 {
                    self.work.sounds.push(0x402_0001);
                    self.work.move_no = 0x14;
                    self.work.move_cnt = 0;
                    self.work.hokan = 3;
                    self.work.routine[2] = 1;
                    self.work.spd = [800, 0, 0];
                    self.work.damage_cnt |= 0x80;
                    self.work.neck.flags |= 0x12;
                }
                self.add_speed_xz();
                let ended = self.joint_move_late(LATE);
                self.work.spd[0] = self.work.spd[0].wrapping_sub(0x78);
                self.work.routine[2] += u8::from(ended);
                if self.work.spd[0] < 0 {
                    self.work.spd[0] = 0;
                }
            }
            2 | 4 | 5 => {
                self.work.bank = Bank::Room;
                if self.work.routine[2] != 5 {
                    self.work.move_no = 0x11;
                    self.work.move_cnt = 0;
                    self.work.hokan = 3;
                    self.work.routine[2] = 5;
                    self.work.damage_cnt = 0;
                }
                let first = self.joint_move_late(LATE);
                let second = self.joint_move_late(LATE);
                if first || second {
                    self.work.routine[2] = 6;
                    self.work.move_no = 6;
                    self.work.move_cnt = 0;
                    self.work.hokan = 3;
                }
            }
            6 => {
                self.work.bank = Bank::Pld;
                if self.joint_move_late_reversed(0x400, true) {
                    self.work.routine = [MODE_MOVE, 0, 0, 0];
                    self.work.damage_cnt = 0;
                    self.work.neck.flags &= 0xED;
                }
            }
            _ => {}
        }
    }

    /// `FUN_004805e0`'s `Routine_1` 0: thrown back, sliding to a stop.
    fn em2a_thrown(&mut self) {
        const LATE: u16 = 0x400;
        match self.work.routine[2] {
            0 | 1 => {
                if self.work.routine[2] == 0 {
                    self.work.sounds.push(0x401_0001);
                    self.work.spd = [-0x258, 0, 0];
                    self.work.move_no = 2;
                    self.work.move_cnt = 0;
                    self.work.hokan = 3;
                    self.work.routine[2] = 1;
                    self.work.damage_cnt |= 0x80;
                    self.work.neck.flags |= 0x12;
                }
                if self.work.hokan == 0 {
                    self.work.routine[2] = 2;
                }
                self.add_speed_xz();
                self.joint_move_late(LATE);
                self.work.spd[0] = self.work.spd[0].wrapping_add(0x70);
            }
            2 | 3 => {
                if self.work.routine[2] == 2 {
                    self.work.move_no = 3;
                    self.work.move_cnt = 0;
                    self.work.hokan = 3;
                    self.work.routine[2] = 3;
                }
                self.add_speed_xz();
                let first = self.joint_move_late(LATE);
                let second = self.joint_move_late(LATE);
                self.work.spd[0] = self.work.spd[0].wrapping_add(0xF);
                self.work.routine[2] += u8::from(first) + u8::from(second);
                if self.work.spd[0] > 0 {
                    self.work.spd[0] = 0;
                }
            }
            4 => {
                self.work.routine = [MODE_MOVE, 0, 0, 0];
                self.work.damage_cnt = 0;
                self.work.neck.flags &= 0xED;
            }
            _ => {}
        }
    }

    /// `FUN_004805e0`'s `Routine_1` 1: hit from behind, turned round, and
    /// back to the player's own clip.
    fn em2a_turned(&mut self) {
        const LATE: u16 = 0x400;
        match self.work.routine[2] {
            0 | 1 => {
                if self.work.routine[2] == 0 {
                    self.work.sounds.push(0x402_0001);
                    self.work.move_no = 4;
                    self.work.move_cnt = 0;
                    self.work.hokan = 3;
                    self.work.routine[2] = 1;
                    self.work.spd = [500, 0, 0];
                    self.work.damage_cnt |= 0x80;
                    self.work.neck.flags |= 0x12;
                }
                self.add_speed_xz();
                let ended = self.joint_move_late(LATE);
                self.work.routine[2] += u8::from(ended);
                let slow = if self.work.hokan == 0 { 0x3C } else { 0x1E };
                self.work.spd[0] = self.work.spd[0].wrapping_sub(slow);
                if self.work.spd[0] < 0 {
                    self.work.spd[0] = 0;
                }
            }
            2 | 4 | 5 => {
                if self.work.routine[2] != 5 {
                    self.work.move_no = 1;
                    self.work.move_cnt = 0;
                    self.work.hokan = 3;
                    self.work.routine[2] = 5;
                    self.work.damage_cnt = 0;
                }
                let first = self.joint_move_late(LATE);
                let second = self.joint_move_late(LATE);
                if first || second {
                    self.work.routine[2] = 6;
                    self.work.move_no = 6;
                    self.work.move_cnt = 0;
                    self.work.hokan = 3;
                }
            }
            6 => {
                self.work.bank = Bank::Pld;
                if self.joint_move_late_reversed(0x400, true) {
                    self.work.routine = [MODE_MOVE, 0, 0, 0];
                    self.work.damage_cnt = 0;
                    self.work.neck.flags &= 0xED;
                }
            }
            _ => {}
        }
    }

    /// `FUN_004805e0`'s `Routine_1` 2: lifted and killed. It turns to face him,
    /// or away if it faces his way, and some of its frames wait for the
    /// other draw buffer.
    fn em2a_killed(&mut self, held: &mut Held<'_>) {
        const LATE: u16 = 0x400;
        let same_way = held
            .holder_angle
            .wrapping_sub(self.player.angle)
            .wrapping_add(0x400)
            & 0xFFF
            < 0x800;
        let [x, _, z] = held.holder;
        let sound = (u32::from(same_way) + 1) << 16 | 0x400_0001;
        let turn = |mover: &mut Self, rate: i32| {
            let target = direction_ck(mover.player.pos[0], mover.player.pos[2], x, z);
            let rate = if same_way { -rate } else { rate };
            mover.player.angle = goto(mover.player.angle, target, rate);
        };
        match self.work.routine[2] {
            0 | 1 => {
                if self.work.routine[2] == 0 {
                    self.work.routine[2] = 1;
                    self.work.move_no = 3 + (self.work.routine[1] & 1);
                    self.work.move_cnt = 0;
                    self.work.hokan = 3;
                    self.work.be_flg |= 0xA;
                    self.work.timers[0] = 0;
                    self.work.sounds.push(sound);
                    self.work.neck.flags |= 0x12;
                }
                turn(self, 0x40);
                if self.work.move_cnt < 6 || held.draw_buffer != 0 {
                    self.joint_move_late(LATE);
                }
                if self.work.move_cnt == 0x1A {
                    self.work.routine[2] = 2;
                }
            }
            2 | 3 => {
                if self.work.routine[2] == 2 {
                    self.work.sounds.push(sound);
                    self.work.move_no = 0;
                    self.work.move_cnt = 0x10;
                    self.work.hokan = 3;
                    self.work.routine[2] = 3;
                }
                turn(self, 0x10);
                let ended = self.joint_move_late(LATE);
                self.work.routine[2] += u8::from(ended);
            }
            4 => {
                self.work.timers[0] = self.work.timers[0].wrapping_add(1);
                if self.work.timers[0] as i16 > 0x23 {
                    self.work.routine[2] = 5;
                }
            }
            5 | 6 => {
                if self.work.routine[2] == 5 {
                    self.work.sounds.push(sound);
                    self.work.move_no = 5;
                    self.work.move_cnt = 0;
                    self.work.hokan = 3;
                    self.work.routine[2] = 6;
                }
                let ended = self.joint_move_late(LATE);
                self.work.routine[2] += u8::from(ended);
                if self.work.move_cnt == 0xC {
                    self.work.routine[2] += 1;
                }
            }
            _ => {}
        }
    }

    /// `Em20_pldie`: thrown down, behind the dog.
    fn em20_pldie(&mut self) {
        match self.work.routine[1] {
            0 => {
                self.work.routine[1] = 1;
                self.work.bank = Bank::Held;
                self.held_clip(0, 0x1F);
                self.work.neck.flags |= 1;
            }
            1 => {
                self.null_pos_set(0x800);
                if let Some(sca) = &self.tick.room.sca {
                    let at = self.player.pos;
                    let ground = crate::sca::ground_at(sca, at, 0, crate::sca::PLAYER_MASK);
                    self.player.pos[1] = ground.height;
                }
                if self.joint_move_late(0x80) {
                    self.work.routine[1] = 2;
                }
            }
            _ => {}
        }
    }

    /// `Esp_call`'s blood from a body part, on a turned offset.
    fn held_blood(&mut self, size: u16, part: usize, point: [i32; 3], rnd: u8) {
        let back = u16::from(self.work.routine[1] & 1);
        let dir = (u16::from(rnd).wrapping_add(back * 0x400) * 2)
            .wrapping_sub(0x100)
            .wrapping_add(self.player.angle);
        self.part_effect(0, 0, size, dir as i16, part, point);
    }

    /// `Esp_call` on a part's `Workm` as the last draw left it
    /// ([`Mover::drawn_part`]) and a vector on it: the effect keeps the part,
    /// and follows it if its blocks say so.
    fn part_effect(
        &mut self,
        kind: u8,
        row: u8,
        size: u16,
        dir: i16,
        part: usize,
        point: [i32; 3],
    ) {
        let point = point.map(|axis| axis as i16);
        let call = match self.drawn_part(part) {
            Some(matrix) => EffectCall {
                kind,
                row,
                size,
                dir,
                at: crate::esp2::apply_matrix(&matrix, point),
                motion: None,
                on: Some((part as u8, matrix, point)),
                turn: None,
            },
            None => EffectCall {
                kind,
                row,
                size,
                dir,
                at: self.player.pos.map(|axis| axis as i16),
                motion: None,
                on: None,
                turn: None,
            },
        };
        self.work.effects.push(call);
    }

    /// The held states' first tick: the clip, facing the enemy, and the turn
    /// that the clip's movement takes if it comes from behind.
    fn held_start(&mut self, held: &Held<'_>, clip: u8) {
        self.work.routine[2] = 1;
        self.work.timers[0] = 0x800;
        self.work.bank = Bank::Held;
        self.work.move_no = clip;
        self.work.move_cnt = 0;
        self.work.hokan = 0xF;
        let [x, _, z] = held.holder;
        let target = direction_ck(self.player.pos[0], self.player.pos[2], x, z);
        self.player.angle = goto(self.player.angle, target, 0x800);
        if self.work.routine[1] & 1 != 0 {
            self.player.angle = self.player.angle.wrapping_add(0x800) & 0xFFF;
            self.work.timers[0] = 0;
        }
    }

    fn held_clip(&mut self, clip: u8, hokan: u8) {
        self.work.move_no = clip;
        self.work.move_cnt = 0;
        self.work.hokan = hokan;
    }

    fn held_null_pos(&mut self) {
        let turn = self.work.timers[0];
        self.null_pos_set(turn);
    }

    /// The hold ending: `Be_flg`'s bits on the enemy go with the caller's
    /// [`released`].
    fn held_end(&mut self, held: &mut Held<'_>) {
        self.work.routine = [MODE_MOVE, 0, 0, 0];
        self.work.damage_cnt &= 0x7F;
        self.work.neck.flags &= 0xED;
        self.work.be_flg &= !0x1004;
        self.work.bank = Bank::Weapon;
        held.released = true;
    }

    /// `em_die_table[0x30]`'s `Routine_1` 0 (`0x48f7d0`): killed by G1's
    /// charge, on its clip 7. Its blood at the player's parts (two effects a
    /// frame below frame 10, each with a `Rnd`) and the parts its frame 5
    /// hides aren't drawn, but their `Rnd`s are. Its `Routine_1` 2 and 3,
    /// which the charge doesn't start, aren't ported.
    fn em30_killed(&mut self, held: &mut Held<'_>) {
        const LATE: u16 = 0x400;
        if self.work.routine[1] != 0 {
            return;
        }
        match self.work.routine[2] {
            0 => {
                self.work.bank = Bank::Held;
                self.work.move_no = 7;
                self.work.move_cnt = 0;
                self.work.hokan = DAMAGE_BLEND;
                self.work.routine[2] = 1;
                self.work.neck.flags = 1;
                self.work.damage_cnt |= DAMAGE_LOCK;
                *held.life = -1;
                self.work.sounds.push(0x402_0001);
            }
            1 => {}
            _ => return,
        }
        if self.work.move_cnt == 0x17 {
            self.work.sounds.push(0x403_0001);
        }
        if self.work.move_cnt < 10 {
            held.rnd.draw();
            held.rnd.draw();
        }
        self.work.bank = Bank::Held;
        if self.joint_move_late(LATE) {
            self.work.be_flg |= 2;
            self.work.routine[2] = 2;
        }
    }

    /// `FUN_004960d0`, `em_damage_table[0x31]`'s `Routine_1` 0: in G2's hand
    /// on its clip 0, turning to it, while G2's grab carries the player.
    /// Its `Routine_1` 2 to 4, the kill, aren't ported.
    fn em31_held(&mut self, held: &mut Held<'_>) {
        if matches!(self.work.routine[1], 2..=4) {
            return;
        }
        match self.work.routine[2] {
            0 => {
                self.work.bank = Bank::Held;
                self.held_clip(0, 3);
                self.work.routine[2] = 1;
                self.work.neck.flags = 1;
                self.work.sounds.push(0x400_0001);
            }
            1 => {}
            _ => return,
        }
        self.work.bank = Bank::Held;
        self.work.move_cnt = 0;
        self.joint_move_late(0x400);
        let [x, _, z] = held.holder;
        let target = direction_ck(self.player.pos[0], self.player.pos[2], x, z);
        self.player.angle = goto(self.player.angle, target, 0x100);
    }

    /// `0x49a790`, `em_damage_table[0x33]`'s `Routine_1` 0 and 1: knocked
    /// back by G3's swipe on the player's own clip 3 or 4 (`+0x108` and
    /// `+0x17c`, the model's bank, not G3's), sliding at 600 and slowing by
    /// 90 a tick, toward `0x800` for `Routine_1` 0 and 0 for 1 (`Add_speedXZ`
    /// adds the turn to `Cdir_y`, so `Timer0` is the heading). `Routine_1`
    /// 2 and 3 are [`Self::em33_gripped`].
    fn em33_held(&mut self, held: &mut Held<'_>) {
        let side = self.work.routine[1];
        if side > 1 {
            self.em33_gripped(held);
            return;
        }
        match self.work.routine[2] {
            0 => {
                self.work.neck.flags |= 1;
                self.work.spd[1] = 0;
                self.work.spd[2] = 0;
                self.work.routine[2] = 1;
                self.work.bank = Bank::Pld;
                self.held_clip(3 + side, 0xF);
                self.work.timers[0] = if side == 0 { 0x800 } else { 0 };
                self.work.spd[0] = 600;
                self.work
                    .sounds
                    .push(0x400_0001 | (u32::from(side & 1) + 1) << 16);
            }
            1 => {}
            _ => return,
        }
        self.work.bank = Bank::Pld;
        if self.joint_move_late(0x100) {
            self.work.neck.flags &= !1;
            self.work.damage_cnt &= 0x7F;
            self.work.routine = [MODE_MOVE, 0, 0, 0];
            self.work.bank = Bank::Weapon;
        }
        let (dx, dz) = rotate(self.work.timers[0], i32::from(self.work.spd[0]), 0);
        self.player.pos[0] += dx;
        self.player.pos[2] += dz;
        self.work.spd[0] = (self.work.spd[0] - 0x5A).max(0);
    }

    /// `0x49a890`, `em_damage_table[0x33]`'s `Routine_1` 2 and 3: struck by
    /// G3's second swipes, on the player's own clip 3 or 4, held where G3's
    /// frame grips them (the part `BYTE_0052a817` names, as it was drawn).
    fn em33_gripped(&mut self, held: &mut Held<'_>) {
        match self.work.routine[2] {
            0 => {
                self.work.routine[2] = 1;
                self.work.neck.flags |= 1;
                self.work.bank = Bank::Pld;
                self.held_clip(self.work.routine[1] + 1, 0xF);
                self.joint_move_late(0x100);
                let side = u32::from(self.work.timers[0].wrapping_add(1));
                self.work.sounds.push(0x400_0001 | side << 16);
            }
            1 => {
                self.work.bank = Bank::Pld;
                if self.joint_move_late(0x100) {
                    self.work.neck.flags &= !1;
                    self.work.damage_cnt &= 0x7F;
                    self.work.routine = [MODE_MOVE, 0, 0, 0];
                    self.work.bank = Bank::Weapon;
                    return;
                }
            }
            _ => return,
        }
        if let Some([x, z]) = held.holder_grip {
            self.player.pos[0] = x;
            self.player.pos[2] = z;
        }
    }

    /// `FUN_0049a980`, `em_die_table[0x33]`: killed in G3's grip, on its
    /// clip 6 from its own frame less one, from `Null_pos_set` turned half
    /// round, with a life of -1. Its blood at part 8 and the parts its frame
    /// 0x10000 hides aren't ported.
    fn em33_killed(&mut self, held: &mut Held<'_>) {
        match self.work.routine[1] {
            0 => {
                self.work.routine[1] = 1;
                self.work.neck.flags |= 1;
                *held.life = -1;
                self.work.bank = Bank::Held;
                let (cnt, carry) = held.holder_move_cnt.overflowing_add(0xFF);
                self.work.move_no = 6;
                self.work.move_cnt = cnt;
                self.work.hokan = 0xE + u8::from(carry);
            }
            1 => {}
            _ => return,
        }
        self.work.bank = Bank::Held;
        self.null_pos_set(0x800);
        let ended = self.joint_move_late(0x100);
        self.work.routine[1] += u8::from(ended);
    }

    /// `FUN_00496180`, `em_die_table[0x31]`'s `Routine_1` 0: killed in G2's
    /// claws, on its clip 0 from `Null_pos_set` turned half round, then 30
    /// ticks' wait. Its blood at the player's parts (an `Esp_call` each fourth
    /// frame from frame 8 and two at frames 0x59 and 0x62, with their `Rnd`s,
    /// which are drawn), the parts its frame 10 hides and frame 0x59's and
    /// 0x62's footfall sound aren't ported.
    fn em31_killed(&mut self, held: &mut Held<'_>) {
        if self.work.routine[1] != 0 {
            return;
        }
        match self.work.routine[2] {
            0 | 1 => {
                if self.work.routine[2] == 0 {
                    self.work.bank = Bank::Held;
                    self.held_clip(0, 7);
                    self.work.damage_cnt |= DAMAGE_LOCK;
                    self.work.neck.flags = 1;
                    self.work.routine[2] = 1;
                }
                if self.work.move_cnt == 10 {
                    self.work.sounds.push(0x403_0001);
                }
                self.work.bank = Bank::Held;
                self.null_pos_set(0x800);
                let ended = self.joint_move_late(0x200);
                self.work.routine[2] += u8::from(ended);
                let count = self.work.move_cnt;
                if count > 6 && count & 3 == 0 {
                    for _ in 0..3 {
                        held.rnd.draw();
                    }
                }
                if count == 0x59 || count == 0x62 {
                    for _ in 0..10 {
                        held.rnd.draw();
                    }
                }
            }
            2 | 3 => {
                if self.work.routine[2] == 2 {
                    self.work.timers[0] = 0x1E;
                    self.work.routine[2] = 3;
                }
                let was = self.work.timers[0];
                self.work.timers[0] = was.wrapping_sub(1);
                if was == 0 {
                    self.work.routine[2] = 4;
                }
            }
            _ => {}
        }
    }

    /// `FUN_0045a880`: held from the front or the back, standing.
    fn held_standing(&mut self, held: &mut Held<'_>) {
        const CLIPS: [u8; 2] = [0, 3];
        // `FUN_0045a880`'s `awStack_c`: from the front, then from behind.
        const POINTS: [[i32; 3]; 2] = [[0, 200, 100], [-100, 100, -200]];
        let back = usize::from(self.work.routine[1] & 1);
        let clip = CLIPS[back];
        if self.work.routine[2] == 0 {
            self.held_start(held, clip);
            self.work.timers[2] = 0;
            let rnd = held.rnd.draw();
            self.work.sounds.push(u32::from(rnd & 1) << 17 | 0x400_0001);
            self.work.timers[1] = 0;
            self.work.neck.flags |= 0x12;
        }
        match self.work.routine[2] {
            1 => {
                self.held_null_pos();
                let ended = self.joint_move_late(0x100);
                self.work.routine[2] += u8::from(ended);
            }
            2 | 3 => {
                if self.work.routine[2] == 2 {
                    self.work.routine[2] = 3;
                    self.held_clip(clip + 1, 0);
                }
                if self.work.move_cnt == 0xF {
                    let rnd = held.rnd.draw();
                    self.held_blood(0x17D0, 8, POINTS[back], rnd);
                    self.work.timers[2] = 0;
                }
                self.held_null_pos();
                self.joint_move_late(0x100);
            }
            4 | 5 => {
                if self.work.routine[2] == 4 {
                    self.work.routine[2] = 5;
                    self.held_clip(clip + 2, 7);
                    if self.work.timers[2] != 0 {
                        let rnd = held.rnd.draw();
                        self.held_blood(0x17D0, 8, POINTS[back], rnd);
                    }
                }
                self.held_null_pos();
                let ended = self.joint_move_late(0x200);
                self.work.routine[2] += u8::from(ended);
                if (9..0xE).contains(&self.work.move_cnt) {
                    let flags = self.work.at_em_flg;
                    held.knocks |= flags & !u32::from(self.work.timers[1]);
                    self.work.timers[1] |= flags as u16;
                }
            }
            6 => self.held_end(held),
            _ => {}
        }
    }

    /// `FUN_0045aca0`: a crawling zombie has hold of the player's legs.
    fn held_low(&mut self, held: &mut Held<'_>) {
        const CLIPS: [u8; 2] = [6, 10];
        const POINTS: [[i32; 3]; 2] = [[-400, -100, 0], [200, -100, 0]];
        let back = usize::from(self.work.routine[1] & 1);
        let clip = CLIPS[back];
        self.work.be_flg |= 4;
        if self.work.routine[2] == 0 {
            self.held_start(held, clip);
            self.work.sounds.push(0x400_0001);
            self.work.neck.flags |= 0x12;
        }
        match self.work.routine[2] {
            1 => {
                self.held_null_pos();
                let ended = self.joint_move_late(0x100);
                self.work.routine[2] += u8::from(ended);
            }
            2 | 3 => {
                if self.work.routine[2] == 2 {
                    self.work.routine[2] = 3;
                    self.held_clip(clip + 1, 0);
                }
                if self.work.move_cnt == 0 {
                    let rnd = held.rnd.draw();
                    self.held_blood(0xFD0, 3, POINTS[back], rnd);
                }
                if self.work.move_cnt == 3 {
                    let rnd = held.rnd.draw();
                    self.held_blood(0x17D0, 3, POINTS[back], rnd);
                }
                self.held_null_pos();
                self.joint_move_late(0x100);
            }
            4 | 5 => {
                if self.work.routine[2] == 4 {
                    self.work.routine[2] = 5;
                    self.held_clip(clip + 2, 7);
                }
                self.held_null_pos();
                let ended = self.joint_move_late(0x200);
                self.work.routine[2] += u8::from(ended);
            }
            6 | 7 => {
                if self.work.routine[2] == 6 {
                    self.held_clip(9, 0);
                    self.work.routine[2] = 7;
                    if back != 0 {
                        self.player.angle = self.player.angle.wrapping_add(0x800) & 0xFFF;
                        self.work.timers[0] = 0x800;
                    }
                }
                self.held_null_pos();
                let ended = self.joint_move_late(0x200);
                self.work.routine[2] += u8::from(ended);
            }
            8 => self.held_end(held),
            _ => {}
        }
    }

    /// `FUN_0045b070`: the player dies in the enemy's hands.
    fn held_dying(&mut self, held: &mut Held<'_>) {
        const CLIPS: [u8; 2] = [0xD, 0xF];
        match self.work.routine[2] {
            0 | 1 => {
                if self.work.routine[2] == 0 {
                    let back = usize::from(self.work.routine[1] & 1);
                    self.work.routine[2] = 1;
                    self.work.timers[0] = if back != 0 { 0 } else { 0x800 };
                    self.work.bank = Bank::Held;
                    self.held_clip(CLIPS[back], 0xF);
                    self.work.sounds.push(0x403_0001);
                    self.work.neck.flags |= 0x13;
                }
                self.held_null_pos();
                let ended = self.joint_move_late(0x100);
                let count = self.work.move_cnt;
                self.work.routine[2] += u8::from(ended);
                if count > 0x28 && count < 99 && count & 1 == 0 {
                    let dir = u16::from(held.rnd.draw()) << 4;
                    let size = u16::from(held.rnd.draw()) * 8 + 0xBD0;
                    self.held_part_blood(size, dir);
                }
                if count > 99 && count < 0x69 {
                    let dir = u16::from(held.rnd.draw()) << 4;
                    self.held_part_blood(0x17D0, dir);
                    let dir = u16::from(held.rnd.draw()) << 4;
                    self.held_part_blood(0x1BD0, dir);
                }
            }
            2 | 3 => {
                if self.work.routine[2] == 2 {
                    self.work.routine[2] = 3;
                    self.work.move_cnt = 0;
                    self.work.move_no += 1;
                    *held.life = i16::MIN;
                }
                self.work.hokan = 10;
                let ended = self.joint_move_late(0x100);
                self.work.routine[2] += u8::from(ended);
            }
            4 | 5 => {
                if self.work.routine[2] == 4 {
                    self.work.routine[2] = 5;
                    self.work.timers[2] = u16::from(held.rnd.draw() & 0x1F) + 3;
                }
                let left = self.work.timers[2];
                self.work.timers[2] = left.wrapping_sub(1);
                if left == 0 {
                    self.work.routine[2] = 3;
                }
            }
            6 => self.work.routine = [7, 0, 0, 0],
            _ => {}
        }
    }

    fn held_part_blood(&mut self, size: u16, dir: u16) {
        self.part_effect(0, 0, size, dir as i16, 8, [0; 3]);
    }

    /// `Snd_se_walk` on a frame marked as a footfall, with the noise
    /// (`0x989EEE`) its caller makes there: [`NOISE_STEP`] or [`NOISE_RUN`].
    fn footfall(&mut self, sound_type: u8, noise: u8) {
        if self.work.now_seq & FRAME_FOOTSTEP != 0 {
            self.work.noise |= noise;
        }
        self.footstep(sound_type);
    }

    /// `Snd_se_walk` on a frame marked as a footfall.
    pub(crate) fn footstep(&mut self, sound_type: u8) {
        if self.work.now_seq & FRAME_FOOTSTEP != 0 {
            // The callers' foot: part 4 or 7 by the frame's other bit.
            let right = self.work.now_seq & FRAME_FOOTSTEP_ALT != 0;
            self.foot_down(sound_type, right);
        }
    }

    /// `Snd_se_walk(sound_type, 4 or 7, w)`: a footfall of the left foot
    /// (part 4) or the right (part 7), with its ripples in water.
    pub(crate) fn foot_down(&mut self, sound_type: u8, right: bool) {
        let water = self.work.water != 0;
        self.player
            .emit_footstep_in(self.work.move_cnt, sound_type, water);
        if water {
            self.water_ripples(sound_type, right);
        }
    }

    /// `Snd_se_walk`'s `Water` path: three ripples (`Esp_call` kind 0x1a) on
    /// part 1's matrix, at the water's level beside the foot.
    fn water_ripples(&mut self, sound_type: u8, right: bool) {
        /// `Snd_se_walk`'s smaller character, whose ripples are smaller.
        const SMALL_ID: u8 = 0x0F;
        let small = self.tick.id == SMALL_ID;
        // `[first size, other size, first vx]` of the walk and the run.
        let (first, other, ahead): (u16, u16, i32) = match (sound_type, small) {
            (0, false) => (0xE00, 0xC00, 200),
            (0, true) => (0xE00, 0xC00, 100),
            (1, false) => (0x1600, 0x1100, 600),
            (1, true) => (0x1200, 0xE00, 300),
            _ => return,
        };
        // `Workm` as the last draw left it, not this tick's pose.
        let Some(part) = self.drawn_part(1) else {
            return;
        };
        let mut up = i32::from(self.work.water) - part.t[1];
        if small {
            up += 100;
        }
        let side = if right { 0xA0 } else { -0xA0 };
        let dir = self.player.angle;
        for (size, ahead, turn) in [(first, ahead, 0), (other, 0, 0x400), (other, 0, 0xC00)] {
            self.part_effect(
                0x1A,
                0,
                size,
                dir.wrapping_add(turn) as i16,
                1,
                [ahead, up, side],
            );
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
    pub(crate) fn add_speed_xyz(&mut self) {
        self.add_speed_xz();
        self.player.pos[1] += i32::from(self.work.spd[1]);
    }

    /// `Spl_Scenario`: a script's motion. A motion with `Sce_flg` bit `0x10`
    /// plays every other tick, which bit `0x20` counts. Bits 1 and 2 then
    /// keep a foot (part 4 or 7) where it was, so the clip walks the body.
    pub(crate) fn scenario(&mut self) {
        let feet = FEET.map(|(_, part)| self.part_point(part, [0; 3]));
        if self.work.sce_flg & 0x30 != 0x30 {
            self.spl_sce();
        }
        self.work.sce_flg ^= 0x20;
        for ((bit, part), was) in FEET.into_iter().zip(feet) {
            if self.work.sce_flg & bit == 0 {
                continue;
            }
            if let (Some(was), Some(now)) = (was, self.part_point(part, [0; 3])) {
                self.player.pos[0] += was[0] - now[0];
                self.player.pos[2] += was[2] - now[2];
            }
        }
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
            0x20 => self.spl_sce20(),
            _ => {}
        }
    }

    /// `FUN_004f6800`: in a zombie's hands, on the clips of its grab, losing
    /// blood. The height it changes, `Null_pos_set` overwrites.
    fn spl_sce20(&mut self) {
        match self.work.routine[2] {
            0 => {
                self.work.routine[2] = 1;
                self.work.bank = Bank::Held;
                self.held_clip(0xD, 0xF);
                self.work.neck.flags |= 1;
                self.work.timers[0] = 0x800;
            }
            1 => {}
            _ => return,
        }
        self.held_null_pos();
        let ended = self.joint_move_late(0x100);
        self.work.routine[2] += u8::from(ended);
        let count = self.work.move_cnt;
        let spray = match count {
            0x29..=98 if count & 1 == 0 => false,
            100..=104 => true,
            _ => return,
        };
        let at = self.part_point(8, [0; 3]).unwrap_or(self.player.pos);
        self.work.sce_blood = Some((spray, at.map(|axis| axis as i16)));
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
        self.footfall(0, NOISE_STEP);
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
        // `Spl_sce_mv03`.
        self.player.angle = goto(self.player.angle, target, i32::from(self.work.dest[1]));
        self.footfall(0, NOISE_STEP);
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
        self.footfall(0, NOISE_STEP);
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
        self.footfall(u8::from(run), if run { NOISE_RUN } else { NOISE_STEP });
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
pub(crate) fn mul_matrix(a: &[[i16; 3]; 3], b: &[[i16; 3]; 3]) -> [[i16; 3]; 3] {
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
pub(crate) fn rot_matrix(angle: [i16; 3]) -> [[i16; 3]; 3] {
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

/// A turn seen with Y flipped (`F·M·F`, `F` = diag(1, -1, 1)): between the
/// game's matrices and the draw's.
fn flip_y(m: [[i32; 3]; 3]) -> [[i32; 3]; 3] {
    std::array::from_fn(|row| {
        std::array::from_fn(|column| {
            let value = m[row][column];
            if (row == 1) != (column == 1) {
                -value
            } else {
                value
            }
        })
    })
}

/// `Em10Dm00`'s and `FUN_00457e60`'s lean after `Joint_move`: `R`, the
/// `RotMatrix` of `(0, yaw, lean)`, turns the first part's `M` on its own side
/// (`MulMatrix`) and its transpose the second's on its parent's
/// (`MulMatrix2`). `Em10Dm00` hands the second `RotMatrix` of `-lean`, which
/// for a turn about z alone is the same transpose.
pub(crate) fn lean(local: &mut [Option<Matrix>], lean: i16, yaw: i16) {
    if lean == 0 && yaw == 0 {
        return;
    }
    let turn = rot_matrix([0, yaw, lean]);
    let back: [[i16; 3]; 3] =
        std::array::from_fn(|row| std::array::from_fn(|column| turn[column][row]));
    if let Some(Some(first)) = local.get_mut(0) {
        first.m = mul_matrix(&first.m, &turn);
    }
    if let Some(Some(second)) = local.get_mut(1) {
        second.m = mul_matrix(&back, &second.m);
    }
}

/// A pose as the skeleton holds it: each part's matrix on its parent's.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PoseTree {
    root: Matrix,
    /// By part, `None` for a part the pose doesn't reach.
    local: Vec<Option<Matrix>>,
    children: Vec<Vec<u8>>,
}

impl PoseTree {
    /// Every part's `Workm`, as `Joint_trans2` composes them from the root,
    /// with some parts off the skeleton at their own (`ko_joint_trans2`'s
    /// parts down, a pulled arm, a kicked head): those and the parts under
    /// them are composed from it.
    pub(crate) fn compose(&self, detached: &[(u8, Matrix)]) -> Vec<Option<Matrix>> {
        self.compose_drawn(detached, 0, &[])
    }

    /// [`Self::compose`] for a draw: a part that isn't drawn (`hidden`, bit 0
    /// of its `Be_flg` clear) keeps the `Workm` the last draw left it
    /// (`previous`), and the parts under it are composed from that.
    pub(crate) fn compose_drawn(
        &self,
        detached: &[(u8, Matrix)],
        hidden: u32,
        previous: &[Option<Matrix>],
    ) -> Vec<Option<Matrix>> {
        // No pose (a body with no clip yet): the last draw's stay.
        if self.local.is_empty() {
            return previous.to_vec();
        }
        let mut parts = vec![None; self.local.len()];
        let mut stack = vec![(0usize, self.root)];
        while let Some((part, parent)) = stack.pop() {
            let Some(Some(local)) = self.local.get(part) else {
                continue;
            };
            let kept = (hidden.checked_shr(part as u32).unwrap_or(0) & 1 != 0)
                .then(|| previous.get(part).copied().flatten())
                .flatten();
            let world = kept.unwrap_or_else(|| {
                detached
                    .iter()
                    .find(|(own, _)| usize::from(*own) == part)
                    .map_or_else(|| parent.comp(local), |(_, own)| *own)
            });
            parts[part] = Some(world);
            for &child in self.children.get(part).map_or(&[][..], Vec::as_slice) {
                stack.push((usize::from(child), world));
            }
        }
        parts
    }
}

/// A `MATRIX`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Matrix {
    pub(crate) m: [[i16; 3]; 3],
    pub(crate) t: [i32; 3],
}

impl Matrix {
    /// `MulLMatrix`: `self` as seen from `root`.
    pub(crate) fn relative_to(&self, root: &Matrix) -> Matrix {
        let transposed: [[i16; 3]; 3] =
            std::array::from_fn(|row| std::array::from_fn(|column| root.m[column][row]));
        let by: [i32; 3] = std::array::from_fn(|axis| self.t[axis] - root.t[axis]);
        Matrix {
            m: mul_matrix(&transposed, &self.m),
            t: std::array::from_fn(|row| {
                let sum: i32 = (0..3)
                    .map(|k| i32::from(transposed[row][k]).wrapping_mul(by[k]))
                    .fold(0, i32::wrapping_add);
                sum >> 12
            }),
        }
    }

    /// `RotMatrix`, then `MulMatrix`.
    pub(crate) fn turned(&self, angle: [i16; 3]) -> Matrix {
        Matrix {
            m: mul_matrix(&self.m, &rot_matrix(angle)),
            t: self.t,
        }
    }

    /// `CompMatrix`, whose `ApplyMatrixLV` floors.
    pub(crate) fn comp(&self, child: &Matrix) -> Matrix {
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

/// `HMatrix` and its `NormalMatrix_0`: `from`, `weight` of 0x1000 of the way to
/// `to`.
pub(crate) fn blend_matrix(from: &[[i16; 3]; 3], to: &[[i16; 3]; 3], weight: i32) -> [[i16; 3]; 3] {
    let mut out = [[0f64; 3]; 3];
    for (row, line) in out.iter_mut().enumerate() {
        for (column, cell) in line.iter_mut().enumerate() {
            *cell = f64::from(
                ((i32::from(from[row][column]) * (0x1000 - weight)) >> 12)
                    + ((i32::from(to[row][column]) * weight) >> 12),
            );
        }
    }
    let unit = |v: [f64; 3]| {
        let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1.0);
        v.map(|axis| axis / length)
    };
    let cross = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let first = unit(out[0]);
    let third = unit(cross(first, unit(out[1])));
    let second = cross(third, first);
    [first, second, third].map(|line| line.map(|axis| (axis * 4096.0) as i16))
}

/// `RotMatrix(0, angle, 0)` then `ApplyMatrixSV`: its products floor rather
/// than truncate, and the result is narrowed to a `short`.
/// The clip and frame `Joint_move` reads for frame `count` of clip `clip`
/// when that is past the clip's end: the clips' frames lie one after another,
/// so it is a frame of a later clip. `None` within the clip, or past the last.
fn frame_past_end(clips: &[Clip], clip: usize, count: u8) -> Option<(usize, usize)> {
    let mut left = usize::from(count).checked_sub(clips.get(clip)?.frames.len())?;
    for (next, after) in clips.iter().enumerate().skip(clip + 1) {
        if left < after.frames.len() {
            return Some((next, left));
        }
        left -= after.frames.len();
    }
    None
}

/// `get_null_pos` on a clip past the table (`Mot_play_init`'s `0xFF`): the
/// original reads the EDD's bytes as they lie, so a frame's word is taken as
/// the clip's count and offset. The parsed clips stand in for those bytes,
/// laid end to end after their table as the shipped models have them.
fn stray_keyframe(clips: &[Clip], move_no: u8, move_cnt: u8) -> Option<u16> {
    let table = clips.len();
    let word = |index: usize| -> Option<u32> {
        if index < table {
            let before: usize = clips[..index].iter().map(|clip| clip.frames.len()).sum();
            let offset = (table + before) * 4;
            return Some(clips[index].frames.len() as u32 | (offset as u32) << 16);
        }
        let frame = clips
            .iter()
            .flat_map(|clip| &clip.frames)
            .nth(index - table)?;
        Some((u32::from(frame.keyframe) | frame.flags) | u32::from(frame.timing) << 16)
    };
    let mut at = (word(usize::from(move_no))? >> 16) as usize / 4 + usize::from(move_cnt);
    while word(at)? & FRAME_SKIPPED != 0 {
        at += 1;
    }
    Some(word(at)? as u16 & 0xFFF)
}

pub(crate) fn rotate(angle: u16, x: i32, z: i32) -> (i32, i32) {
    let (cos, sin) = (rcos(i32::from(angle)), rsin(i32::from(angle)));
    (
        i32::from(((cos * x + sin * z) >> 12) as i16),
        i32::from(((cos * z - sin * x) >> 12) as i16),
    )
}

/// `catan`, on the original's x87: `atan(v / 4096) * 2048 / 3.14`, truncated.
/// The divisor is the executable's own constant, not a full-precision pi.
pub(crate) fn catan(value: i32) -> i32 {
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

/// `Spl_Init`'s `Atd[0]` heights in units of 18 (`At_h`, and `-Ofs_y`), by id
/// from 0x40: the tables it builds on its stack (`0x4ef3c6` to `0x4ef7cf`).
pub const CHARACTER_HEIGHTS: [u8; 28] = [
    85, 75, 85, 75, 86, 45, 86, 75, 85, 75, 85, 75, 0, 75, 0, 45, 85, 75, 85, 75, 85, 75, 85, 75,
    85, 75, 85, 85,
];

/// `Atd[0]`'s `At_h` of character `id`: 810 for Sherry, 0x5FA for most.
pub fn character_height(id: u8) -> u16 {
    id.checked_sub(0x40)
        .and_then(|index| CHARACTER_HEIGHTS.get(usize::from(index)))
        .map_or(0x5FA, |&height| u16::from(height) * 18)
}

/// `Spl_Init`: a script's character, whose first pose is the first frame of
/// its model's first clip, and which then waits for the script. The partner
/// (`partner`, `Type`'s `0x400` with `fg_status`'s partner bit) keeps
/// `Spl_Auto`'s routine, and `Spl_in45` has her look at the player before the
/// head turns. `kind` is the `Type` after `Spl_Init` raised its bits: `0x4000`
/// sends the character to `Spl_Scenario`.
pub fn spl_init(
    work: &mut Work,
    player: &mut PlayerState,
    tick: &Tick<'_>,
    kind: u16,
    partner: bool,
) {
    work.routine = if kind & 0x4000 != 0 {
        [MODE_SCENARIO, 6, 0, 0]
    } else {
        [MODE_MOVE, 0, 0, 0]
    };
    work.bank = Bank::Pld;
    work.move_no = 0;
    work.move_cnt = 0;
    work.hokan = 0;
    work.neck.flags = 0x92;
    if partner {
        work.neck.flags = 0;
        work.neck.target = Some(0);
    }
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
    // `_Spl` turns the head after whichever routine ran.
    mover.rot_neck();
}

/// `_Spl`'s routine for the partner, `Spl_Auto`, then its `Rot_neck`.
pub(crate) fn spl_auto(
    work: &mut Work,
    player: &mut PlayerState,
    tick: &Tick<'_>,
    run: impl FnOnce(&mut Mover<'_, '_>),
) {
    let mut mover = Mover::new(work, player, tick);
    run(&mut mover);
    mover.rot_neck();
    mover.work.neck.body = Some(mover.player.pos);
}

/// Whether `_Spl`'s routine is `Spl_Auto`'s.
pub fn in_auto(work: &Work) -> bool {
    work.routine[0] == MODE_MOVE
}

/// `Workm` of a work's part, as the last tick left it.
pub(crate) fn part_world(
    work: &mut Work,
    player: &mut PlayerState,
    tick: &Tick<'_>,
    part: usize,
) -> Option<Matrix> {
    let mover = Mover {
        work,
        player,
        tick,
        arrived: None,
        stairs: None,
    };
    mover.part_matrix(part)
}

/// The player is in `pl_mv06`, the crouching pick-up.
pub fn crouching(work: &Work) -> bool {
    work.routine[..2] == [MODE_MOVE, MOVE_ITEM]
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

/// `Plc_stop` on the player. A scene that stops the player on a room's
/// first tick, before `pl_mv_00` has run (room 200's entry), leaves
/// `Pl_init`'s blend, 0, which nothing writes until the scene hands the
/// player back.
pub fn plc_stop_player(work: &mut Work) {
    if std::mem::take(&mut work.first_tick_assumed) {
        work.hokan = 0;
    }
    plc_stop(work);
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
        // `Be_flg`'s low and high words.
        0 => i32::from(work.be_flg as u16),
        1 => i32::from((work.be_flg >> 16) as u16),
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
        0 => work.be_flg = (work.be_flg & 0xFFFF_0000) | u32::from(value as u16),
        1 => work.be_flg = (work.be_flg & 0xFFFF) | u32::from(value as u16) << 16,
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
mod weapon_tests {
    use super::*;

    fn weapon(id: u8) -> Weapon {
        Weapon {
            id,
            loaded: true,
            spare: true,
            rounds: 100,
        }
    }

    #[test]
    fn the_grenade_launcher_fires_effects_rather_than_a_shot() {
        for id in [GRENADES, 0xA, ACID_ROUNDS] {
            let launcher = weapon(id);
            assert!(launcher.aims());
            assert!(launcher.shoots_effects());
            assert_eq!(launcher.frames(), [0x8A, 0x8C, 26]);
        }
        assert!(!weapon(8).shoots_effects());
        // The grenade rounds' five come from `grenades`; the other rounds make
        // their three calls from the table.
        assert!(weapon(GRENADES).shot_effects(1).is_empty());
        assert_eq!(weapon(0xA).shot_effects(1)[0].calls.len(), 3);
    }

    #[test]
    fn each_gun_has_its_own_timing_and_leg() {
        assert!(
            [KNIFE, 2, 3, 5, 6, 7, 8, 9, 0xC, COLT, BERETTA]
                .iter()
                .all(|&id| weapon(id).aims())
        );
        assert!(![0, 4, 0xE, 0x12].iter().any(|&id| weapon(id).aims()));
        // The knife's row: a slash let go of past frame 0x32 stops at once.
        assert!(weapon(KNIFE).is_knife() && !weapon(2).is_knife());
        assert_eq!(weapon(KNIFE).fire_cancel_frame(), 0x32);
        assert_eq!(weapon(2).fire_cancel_frame(), 8);
        assert_eq!(weapon(7).fire_cancel_frame(), 28);
        assert_eq!(weapon(8).tilt_down_frame(), 8);
        assert_eq!(weapon(5).foot_chain(0), &AIM_FOOT_CHAIN);
        assert_eq!(weapon(7).foot_chain(0), &AIM_OTHER_FOOT_CHAIN);
    }

    #[test]
    fn a_matrix_comes_back_from_under_another() {
        let parent = Matrix {
            m: rot_matrix([0x123, 0x456, 0x789]),
            t: [1000, -2000, 3000],
        };
        let child = Matrix {
            m: rot_matrix([0x321, 0x654, 0x87]),
            t: [100, 569, -40],
        };
        let back = parent.comp(&child).relative_to(&parent);
        for axis in 0..3 {
            assert!((back.t[axis] - child.t[axis]).abs() <= 3, "{back:?}");
        }
        assert_eq!(child.turned([0, 0, 0]).m, child.m);
    }

    #[test]
    fn the_big_magnum_looks_for_a_body_longer() {
        assert_eq!(weapon(5).shot_ticks(), 1);
        assert_eq!(weapon(6).shot_ticks(), 0b111);
        assert_eq!(weapon(7).shot_ticks(), 0b1_0101);
        assert_eq!(weapon(8).shot_ticks(), 0b101_0101);
    }

    /// `DAT_0053a305`'s rows: the Colt tilts up past frame 6 and a shot let
    /// go of stops past 5; the Beretta's are the handguns'. Both look for a
    /// body on the shot's first tick alone, as the handguns do.
    #[test]
    fn the_colt_and_the_beretta_have_the_tables_rows() {
        assert_eq!(weapon(COLT).frames(), [0x85, 0x86, 5]);
        // The raise turns to its target 0x120 a tick with the Colt.
        assert_eq!(weapon(COLT).enemy_turn(), 0x120);
        assert_eq!(weapon(BERETTA).enemy_turn(), 0xD8);
        assert_eq!(weapon(2).enemy_turn(), 0xD8);
        assert_eq!(weapon(BERETTA).frames(), weapon(2).frames());
        for id in [COLT, BERETTA] {
            assert_eq!(weapon(id).shot_ticks(), 1);
            assert!(!weapon(id).shoots_effects());
            assert_eq!(weapon(id).foot_chain(0), &AIM_FOOT_CHAIN);
        }
    }

    /// `pl_mv05_w_tbl` sends the sub-machine gun and the flamethrower to
    /// `pl_mv05_w2`, with `DAT_0053a305`'s rows and their own clips.
    #[test]
    fn the_bursts_have_their_own_clips_and_rows() {
        for id in [SMG, FLAMETHROWER] {
            let weapon = weapon(id);
            assert!(weapon.aims() && weapon.bursts());
            assert_eq!(weapon.frames(), [0x87, 0x88, 0]);
            assert_eq!(weapon.hold_clips(), [0x12, 0x0C, 0x0F]);
            assert_eq!(weapon.reload_clip(), 0x13);
            assert_eq!(weapon.enemy_turn(), 0xD8);
        }
        assert_eq!(weapon(2).hold_clips(), AIM_HOLD_CLIPS);
        assert_eq!(weapon(2).reload_clip(), AIM_RELOAD_CLIP);
        assert!(
            ![2, COLT, 0x11, BERETTA]
                .iter()
                .any(|&id| weapon(id).bursts())
        );
    }

    /// `Weapon_at_ck`'s walk: the sub-machine gun's sequence looks on the
    /// first tick and every third after; the flamethrower's never does.
    #[test]
    fn the_smgs_sequence_looks_every_third_tick_and_the_flames_never() {
        let walk = |id: u8| -> Vec<bool> {
            let sequence = weapon(id).burst_sequence();
            let mut attw = (0, sequence[0].1);
            (0..10)
                .map(|_| {
                    let (next, looks) = attw_step(sequence, attw);
                    attw = next;
                    looks
                })
                .collect()
        };
        let every_third: Vec<bool> = (0..10).map(|tick| tick % 3 == 0).collect();
        assert_eq!(walk(SMG), every_third);
        assert_eq!(walk(FLAMETHROWER), [false; 10]);
        // The handguns' one look, then none.
        let handgun = [(0, 1), (0, 0xFF)];
        let mut attw = (0, 1);
        let looks: Vec<bool> = (0..3)
            .map(|_| {
                let (next, looks) = attw_step(&handgun, attw);
                attw = next;
                looks
            })
            .collect();
        assert_eq!(looks, [true, false, false]);
    }

    /// `FUN_004d6560` makes its calls on the shot's second frame, at three
    /// points up the barrel; the Beretta's are `pl_w02_t2_move`'s.
    #[test]
    fn the_colt_flashes_on_its_second_frame_and_the_beretta_as_the_handgun() {
        let words = |id: u8, frame: u8| -> Vec<([i32; 3], u32, Option<i16>)> {
            weapon(id)
                .shot_effects(frame)
                .iter()
                .flat_map(|effect| {
                    effect
                        .calls
                        .iter()
                        .map(|&(word, dir)| (effect.point, word, dir))
                })
                .collect()
        };
        assert!(words(COLT, 1).is_empty());
        assert_eq!(
            words(COLT, 2),
            [
                ([0x82, 0x2DA, 0], 0x100_1000, None),
                ([0x82, 0x118, 0], 0x104_1000, None),
                ([0x82, 0x118, 0], 0x106_1000, Some(0x800)),
                ([0x82, 0x118, 0], 0x30A_0C00, None),
                ([0x82, 0x33E, 0], 0x30C_1000, None),
            ]
        );
        assert!(words(COLT, 3).is_empty(), "no case: the Colt keeps them");
        for frame in 1..6 {
            assert_eq!(words(BERETTA, frame), words(2, frame), "frame {frame}");
        }
        assert_eq!(words(BERETTA, 3).len(), 3);
    }

    #[test]
    fn each_gun_has_its_own_flash_and_case() {
        let calls = |id: u8, frame: u8| -> usize {
            weapon(id)
                .shot_effects(frame)
                .iter()
                .map(|effect| effect.calls.len())
                .sum()
        };
        assert_eq!(calls(2, 1), 2);
        assert_eq!(calls(5, 1), 2);
        assert_eq!(calls(6, 1), 3);
        assert_eq!(calls(7, 1), 6);
        assert_eq!(calls(8, 1), 10);
        assert_eq!(calls(7, 0x1A), 2);
        assert_eq!(calls(8, 5), 2);
        assert_eq!(calls(2, 5), 0);
    }
}

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
        neck_targets: Vec<NeckTarget>,
        /// What the shots see of the enemy slots.
        targets: Vec<crate::weapon2::Target>,
        /// `BYTE_00989eeb`, else as many as `neck_targets`.
        enemies: Option<u8>,
        /// `fg_status`'s `0x2000`.
        pushing: bool,
        /// `Pad_set`'s last `Key`, for [`Self::tick`]'s `Key_trg`.
        last_key: u16,
        /// The room's objects.
        objects: Vec<crate::objects::ObjectRecord>,
    }

    /// `angle` from the player, as far as `distance`.
    fn neck_target(index: usize, kind: u16, angle: f64, distance: u32) -> NeckTarget {
        let pos = [
            (angle.cos() * f64::from(distance)) as i32,
            0,
            (-angle.sin() * f64::from(distance)) as i32,
        ];
        NeckTarget {
            index,
            kind,
            pos,
            neck: pos,
            distance,
            life: 100,
            angle: 0,
        }
    }

    #[test]
    fn poison_takes_a_life_every_31_ticks_but_never_the_last() {
        let mut work = Work {
            poisoned: true,
            poison_timer: 0x1E,
            ..Work::default()
        };
        let mut life = 2;
        for _ in 0..0x1E {
            poison_tick(&mut work, &mut life);
        }
        assert_eq!(life, 2);
        poison_tick(&mut work, &mut life);
        assert_eq!((life, work.poison_timer), (1, 0x1E));
        for _ in 0..0x1F {
            poison_tick(&mut work, &mut life);
        }
        assert_eq!(life, 1);
    }

    #[test]
    fn the_life_levels_read_danger_before_poison() {
        assert_eq!(life_level(DANGER_LIFE - 1, false), 2);
        assert_eq!(life_level(DANGER_LIFE - 1, true), 2);
        assert_eq!(life_level(CAUTION_LIFE - 1, false), 1);
        assert_eq!(life_level(CAUTION_LIFE, true), 1);
        assert_eq!(life_level(CAUTION_LIFE, false), 0);
    }

    #[test]
    fn pl_neck_looks_every_so_often_at_the_nearest_it_can_see() {
        let mut setup = Setup::new(0);
        setup.player.pos = [0; 3];
        setup.player.angle = 0;
        let ahead = 0.0;
        let behind = std::f64::consts::PI;
        setup.neck_targets = vec![
            neck_target(2, 0, behind, 500),
            neck_target(3, 0, ahead, 3000),
            neck_target(4, 0, ahead, 2000),
            neck_target(5, 0xC000, ahead, 100),
            neck_target(6, 0x2000, ahead, 100),
            neck_target(7, 0, ahead, 8000),
        ];
        setup.work.neck.wait = 1;
        setup.tick(Input::default(), 200);
        assert_eq!(setup.work.neck.target, None, "not time to look yet");
        setup.tick(Input::default(), 200);
        assert_eq!(setup.work.neck.target, Some(4), "the nearest standing");
        assert_eq!(setup.work.neck.wait, NECK_WAIT);

        setup
            .neck_targets
            .retain(|target| target.index != 3 && target.index != 4);
        setup.work.neck.wait = 0;
        setup.tick(Input::default(), 200);
        assert_eq!(setup.work.neck.target, Some(6), "else someone on the floor");

        setup.neck_targets.clear();
        setup.work.neck.wait = 0;
        setup.tick(Input::default(), 200);
        assert_eq!(setup.work.neck.target, None, "else its own");
    }

    /// `pl_br_00` (0x4D9D60): the button going down, with no stairs and no
    /// climb to take it, raises `fg_status`'s 0x200000 for `Sce_at`; held,
    /// it doesn't, and the aim (`pl_mv05`) never does.
    #[test]
    fn a_new_press_while_standing_asks_for_the_check() {
        let mut setup = Setup::new(0);
        setup.work.routine = [MODE_MOVE, MOVE_IDLE, 1, 0];
        let fire = Input {
            fire: true,
            ..Input::default()
        };
        setup.tick(fire, 200);
        assert!(setup.work.check);
        setup.tick(fire, 200);
        assert!(!setup.work.check, "held");
        setup.tick(Input::default(), 200);
        assert!(!setup.work.check);
        setup.work.routine = [MODE_MOVE, MOVE_AIM, 1, 0];
        setup.tick(fire, 200);
        assert!(!setup.work.check, "aiming");
    }

    /// The check takes `Pad_set`'s (0x4D0F30) `Key_trg`. Under
    /// `fg_stop & 0x1000000` (a door, the get-item close, a check's message)
    /// `Pad_set` keeps only `Key & 0x3C00`, and the first frame after the stop
    /// sets the last key to this one's (`DWORD_0098860c = Key`), so fire held
    /// through the stop is no new press. One released and pressed again is.
    #[test]
    fn a_button_held_through_a_stop_asks_for_no_check() {
        let mut setup = Setup::new(0);
        setup.work.routine = [MODE_MOVE, MOVE_IDLE, 1, 0];
        let mut pad = crate::scd2::host::Re2State::default();
        // One of `tick_room_timed`'s ticks: `Pad_set`, then the input a stop
        // blanks, then `Player_move`.
        let tick = |setup: &mut Setup, pad: &mut crate::scd2::host::Re2State, held: bool| {
            let raw = Input {
                fire: held,
                ..Input::default()
            };
            pad.set_key(key_word(&raw));
            let input = if pad.input_stopped() {
                Input::default()
            } else {
                raw
            };
            pad.door_key_stop = false;
            setup.tick_trg(input, pad.key_trg & key_word(&input), 200);
            setup.work.check
        };
        assert!(tick(&mut setup, &mut pad, true));
        // A door's stop, then a message's stop word, each held through.
        pad.door_key_stop = true;
        assert!(!tick(&mut setup, &mut pad, true));
        assert!(!tick(&mut setup, &mut pad, true), "the door's stop ended");
        *pad.fg_stop_mut() |= 0xFFFF << 16;
        assert!(!tick(&mut setup, &mut pad, true));
        assert!(!tick(&mut setup, &mut pad, true));
        *pad.fg_stop_mut() &= 0xFFFF;
        assert!(
            !tick(&mut setup, &mut pad, true),
            "the message's stop ended"
        );
        assert!(!tick(&mut setup, &mut pad, true));
        // Let go during the stop, and a press after it is new. The
        // first frame after it raises nothing, even for a press on that frame.
        *pad.fg_stop_mut() |= 0xFFFF << 16;
        assert!(!tick(&mut setup, &mut pad, false));
        *pad.fg_stop_mut() &= 0xFFFF;
        assert!(
            !tick(&mut setup, &mut pad, true),
            "pressed as the stop ended"
        );
        assert!(!tick(&mut setup, &mut pad, false));
        assert!(
            tick(&mut setup, &mut pad, true),
            "a new press after the stop"
        );
    }

    /// `pl_mv_01`: in water more than 500 over the ground (room 402's channel,
    /// -700) each step is a third shorter; room 302's -180 isn't deep enough.
    #[test]
    fn deep_water_takes_a_third_off_the_walk() {
        let walked = |water: i16| {
            let mut setup = Setup::new(30);
            setup.player.pos = [0; 3];
            setup.player.angle = 0;
            setup.work.water = water;
            let up = Input {
                up: true,
                ..Input::default()
            };
            for _ in 0..12 {
                setup.tick(up, 200);
            }
            setup.player.pos[0]
        };
        let dry = walked(0);
        assert!(dry > 0);
        assert_eq!(walked(-180), dry);
        let deep = walked(-700);
        assert!(deep < dry && deep * 3 >= dry * 2 - 30, "{deep} of {dry}");
    }

    /// `pl_mv_09`'s fall with the water over the ground: `Pl_water`'s four
    /// rings on each of its first five ticks (`Timer3` 0x85), then none.
    #[test]
    fn a_fall_into_water_splashes_on_five_ticks() {
        let mut setup = Setup::new(0);
        (setup.clips, setup.keyframes) = bank(10, 40, 0);
        setup.player.pos = [0, -3600, 0];
        setup.work.water = -700;
        setup.work.routine = [MODE_MOVE, MOVE_JUMP_DOWN, 2, 2];
        let mut rings = Vec::new();
        while setup.work.routine[3] < 4 {
            setup.tick(Input::default(), 200);
            let effects = std::mem::take(&mut setup.work.effects);
            rings.push(effects.iter().filter(|call| call.kind == 0x1A).count());
        }
        assert!(rings.len() > 6, "{rings:?}");
        assert_eq!(rings[..5], [4; 5]);
        assert!(rings[5..].iter().all(|&count| count == 0), "{rings:?}");
        // Out of water, none.
        let mut setup = Setup::new(0);
        (setup.clips, setup.keyframes) = bank(10, 40, 0);
        setup.player.pos = [0, -3600, 0];
        setup.work.routine = [MODE_MOVE, MOVE_JUMP_DOWN, 2, 2];
        setup.tick(Input::default(), 200);
        assert!(setup.work.effects.is_empty());
    }

    /// `pl_mv_08` raises `Be_flg`'s 4 (no push out of the other bodies) as it
    /// starts and 8 on frame 0x11, and drops both on frame 0x31; the jump down
    /// holds the 4 until it lands.
    #[test]
    fn the_climb_and_the_jump_are_not_pushed_out_of_bodies() {
        let mut setup = Setup::new(0);
        (setup.clips, setup.keyframes) = bank(10, 60, 0);
        setup.player.angle = 0;
        setup.work.routine = [MODE_MOVE, MOVE_CLIMB, 0, 0];
        let mut seen = Vec::new();
        while setup.work.routine[1] == MOVE_CLIMB && seen.len() < 200 {
            setup.tick(Input::default(), 200);
            seen.push(setup.work.be_flg & (BE_NO_PUSH | BE_NO_SCA));
        }
        assert_eq!(seen[0], BE_NO_PUSH);
        assert!(seen.contains(&(BE_NO_PUSH | BE_NO_SCA)));
        assert_eq!(seen.last(), Some(&0));
        let mut setup = Setup::new(0);
        (setup.clips, setup.keyframes) = bank(10, 40, 0);
        setup.player.pos = [0, -1800, 0];
        setup.work.routine = [MODE_MOVE, MOVE_JUMP_DOWN, 2, 0];
        let mut pushed = Vec::new();
        while setup.work.routine[1] == MOVE_JUMP_DOWN && pushed.len() < 200 {
            setup.tick(Input::default(), 200);
            pushed.push((setup.work.routine[3], setup.work.be_flg & BE_NO_PUSH));
        }
        // Up from the first step to the landing's (`Routine_3` 4 runs next).
        let held = |step: u8| (1..=4).contains(&step);
        assert!(
            pushed.iter().all(|&(step, bit)| (bit != 0) == held(step)),
            "{pushed:?}"
        );
    }

    /// `pl_br_03` (0x4DA6C0) asks for the check while backing up, and
    /// `pl_br_09` (0x4DB930) once the landing has run four frames.
    #[test]
    fn backing_up_and_landing_ask_for_the_check_too() {
        let mut setup = Setup::new(0);
        setup.work.routine = [MODE_MOVE, MOVE_BACKWARD, 1, 0];
        let input = Input {
            down: true,
            fire: true,
            ..Input::default()
        };
        setup.tick(input, 200);
        assert!(setup.work.check);
        let mut setup = Setup::new(0);
        setup.work.routine = [MODE_MOVE, MOVE_JUMP_DOWN, 0, 5];
        setup.tick(
            Input {
                fire: true,
                ..Input::default()
            },
            200,
        );
        assert!(setup.work.check);
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
    fn the_neck_is_placed_on_the_pushed_body() {
        let mut setup = Setup::new(0);
        for keyframe in &mut setup.keyframes {
            keyframe.rotations = vec![[0; 3]; NECK_JOINT + 1];
        }
        setup.skeleton.relative = vec![[0; 3]; NECK_JOINT + 1];
        setup.skeleton.relative[NECK_JOINT] = [0, -700, 0];
        setup.player.pos = [1000, 0, 2000];
        setup.player.angle = 0;
        let bank = (setup.clips.as_slice(), setup.keyframes.as_slice());
        let tick = Tick {
            room: &setup.room,
            skeleton: &setup.skeleton,
            morphs: &[],
            banks: Banks {
                pld: bank,
                weapon: bank,
                room: bank,
                held: bank,
            },
            input: Input::default(),
            options: Options::default(),
            id: 0,
            life: 200,
            poisoned: false,
            stopped: false,
            weapon: setup.weapon,
            necks: [None; NECK_WORKS],
            targets: &[],
            neck_targets: &setup.neck_targets,
            enemies: setup.neck_targets.len() as u8,
            pushing: false,
            objects: &[],
            key_trg: 0,
        };
        let mut mover = Mover::new(&mut setup.work, &mut setup.player, &tick);
        mover.rot_neck_em(None, None);
        let before = mover.work.neck.position.expect("a head on the pose");
        // `Oba_ck_em` and `Sca_ck_em` push the body after `Rot_neck_em`.
        mover.player.pos[0] += 18;
        mover.player.pos[2] -= 14;
        mover.place_neck();
        assert_eq!(
            mover.work.neck.position,
            Some([before[0] + 18, before[1], before[2] - 14])
        );
    }

    /// `Neck_Dir_ZY` aims from the neck's `Workm.t` (`+0x5C`) as the last
    /// draw left it: a head drawn 600 units off its pose turns as a body
    /// standing there would, not as the pose would. In `leon_a_106_108` this
    /// is the crawler's -144 to -135 where the pose gave -158.
    #[test]
    fn the_neck_aims_from_where_it_was_last_drawn() {
        fn turn(pos: [i32; 3], drawn: Option<[i32; 3]>) -> i16 {
            let mut setup = Setup::new(0);
            for keyframe in &mut setup.keyframes {
                keyframe.rotations = vec![[0; 3]; NECK_JOINT + 1];
            }
            setup.skeleton.relative = vec![[0; 3]; NECK_JOINT + 1];
            setup.skeleton.relative[NECK_JOINT] = [0, -700, 0];
            setup.player.pos = pos;
            setup.player.angle = 0;
            let bank = (setup.clips.as_slice(), setup.keyframes.as_slice());
            let tick = Tick {
                room: &setup.room,
                skeleton: &setup.skeleton,
                morphs: &[],
                banks: Banks {
                    pld: bank,
                    weapon: bank,
                    room: bank,
                    held: bank,
                },
                input: Input::default(),
                options: Options::default(),
                id: 0,
                life: 200,
                poisoned: false,
                stopped: false,
                weapon: setup.weapon,
                necks: [None; NECK_WORKS],
                targets: &[],
                neck_targets: &[],
                enemies: 0,
                pushing: false,
                objects: &[],
                key_trg: 0,
            };
            let mut mover = Mover::new(&mut setup.work, &mut setup.player, &tick);
            mover.rot_neck_em(Some([3000, -700, 2000]), drawn);
            mover.work.neck.turn[0]
        }
        let posed = turn([1000, 0, 2000], None);
        let drawn = turn([1000, 0, 2000], Some([1000, -700, 2600]));
        assert_ne!(drawn, posed);
        assert_eq!(drawn, turn([1000, 0, 2600], None));
    }

    /// `Pl_init` stands the player on the weapon bank's idle, not the body's:
    /// Claire's handgun's (keyframe 50) has her hips 15 units higher than
    /// her PLD's clip 2 and rolled, which puts her head where the original's
    /// is at a new game, through a door and after the get-item window.
    #[test]
    fn pl_init_poses_the_idle_of_the_weapons_bank() {
        let mut setup = Setup::new(0);
        setup.skeleton.relative = vec![[0; 3]; NECK_JOINT + 1];
        setup.skeleton.relative[0] = [0, -1709, 0];
        setup.skeleton.relative[NECK_JOINT] = [-21, -623, 0];
        setup.skeleton.children = vec![vec![NECK_JOINT as u8]];
        setup.player.pos = [18802, 0, -2932];
        setup.player.angle = 2048;
        let idle = |offset: i16, roll: i16, keyframe: u16| {
            let mut rotations = vec![[0; 3]; NECK_JOINT + 1];
            rotations[0] = [0, 0, roll];
            let clip = Clip {
                frames: vec![ClipFrame {
                    keyframe,
                    ..ClipFrame::default()
                }],
            };
            let pose = Keyframe {
                offset: [0, offset, 0],
                vertex: [0; 3],
                rotations,
            };
            (vec![Clip::default(), Clip::default(), clip], pose)
        };
        let (pld_clips, pld_pose) = idle(-1708, 4077, 0);
        let (weapon_clips, weapon_pose) = idle(-1723, 4077, 1);
        let keyframes = vec![pld_pose, weapon_pose.clone()];
        assert_eq!(idle_pose(&weapon_clips, &keyframes), Some(&weapon_pose));
        let tick = Tick {
            room: &setup.room,
            skeleton: &setup.skeleton,
            morphs: &[],
            banks: Banks {
                pld: (&pld_clips, &keyframes),
                weapon: (&weapon_clips, &keyframes),
                room: (&pld_clips, &keyframes),
                held: (&pld_clips, &keyframes),
            },
            input: Input::default(),
            options: Options::default(),
            id: 1,
            life: 200,
            poisoned: false,
            stopped: false,
            weapon: setup.weapon,
            necks: [None; NECK_WORKS],
            targets: &[],
            neck_targets: &[],
            enemies: 0,
            pushing: false,
            objects: &[],
            key_trg: 0,
        };
        pose_init(&mut setup.work, &mut setup.player, &tick);
        let [x, y, z] = setup.work.neck.position.expect("a head on the pose");
        // The original's at Claire's new game: 18841, -2346, -2932.
        assert_eq!((x - 18802, y, z), (39, -2346, -2932));
    }

    /// `FUN_0046acf0`'s second step: six bloods on parts 3 to 5, rows 1, 2
    /// and 1 toward 0x536 and rows 2, 0 and 0 a quarter round, each at its
    /// part's point; there is no fourth step.
    #[test]
    fn the_alligators_drag_bleeds_on_three_parts_a_step() {
        let mut setup = Setup::new(0);
        for keyframe in &mut setup.keyframes {
            keyframe.rotations = vec![[0; 3]; 7];
        }
        setup.skeleton.relative = std::iter::once([0; 3])
            .chain(std::iter::repeat_n([0, -100, 0], 6))
            .collect();
        setup.skeleton.children = (1..7u8).map(|child| vec![child]).collect();
        setup.skeleton.children.push(Vec::new());
        setup.player.pos = [1000, 0, 2000];
        setup.player.angle = 0;
        let bank = (setup.clips.as_slice(), setup.keyframes.as_slice());
        let tick = Tick {
            room: &setup.room,
            skeleton: &setup.skeleton,
            morphs: &[],
            banks: Banks {
                pld: bank,
                weapon: bank,
                room: bank,
                held: bank,
            },
            input: Input::default(),
            options: Options::default(),
            id: 0,
            life: 200,
            poisoned: false,
            stopped: false,
            weapon: setup.weapon,
            necks: [None; NECK_WORKS],
            targets: &[],
            neck_targets: &setup.neck_targets,
            enemies: setup.neck_targets.len() as u8,
            pushing: false,
            objects: &[],
            key_trg: 0,
        };
        let mut mover = Mover::new(&mut setup.work, &mut setup.player, &tick);
        mover.em23_drag_blood(1);
        let calls: Vec<_> = std::mem::take(&mut mover.work.effects)
            .iter()
            .map(|call| (call.kind, call.row, call.size, call.dir, call.at))
            .collect();
        let (near, far) = ([1000, -100, 2000], [1000, -300, 2000]);
        assert_eq!(
            calls,
            [
                (0, 1, 0x1800, 0x536, near),
                (0, 2, 0x1000, 0x536, near),
                (0, 1, 0x1800, 0x536, far),
                (0, 2, 0x2000, 0x936, near),
                (0, 0, 0x1800, 0x936, near),
                (0, 0, 0x1000, 0x936, far),
            ]
        );
        mover.em23_drag_blood(3);
        assert!(mover.work.effects.is_empty());
    }

    /// `Snd_se_walk` in water: the footfall plays the water's sound, and three
    /// ripples (`Esp_call` kind 0x1a) start on part 1 at the water's level.
    #[test]
    fn a_footfall_in_water_starts_three_ripples() {
        let mut setup = Setup::new(0);
        for keyframe in &mut setup.keyframes {
            keyframe.rotations = vec![[0; 3]; 2];
        }
        setup.skeleton.relative = vec![[0; 3], [0, -900, 0]];
        setup.skeleton.children = vec![vec![1], Vec::new()];
        setup.player.pos = [1000, 0, 2000];
        setup.player.angle = 0x100;
        setup.work.water = -180;
        setup.work.now_seq = FRAME_FOOTSTEP;
        setup.player.take_footsteps();
        let bank = (setup.clips.as_slice(), setup.keyframes.as_slice());
        let tick = Tick {
            room: &setup.room,
            skeleton: &setup.skeleton,
            morphs: &[],
            banks: Banks {
                pld: bank,
                weapon: bank,
                room: bank,
                held: bank,
            },
            input: Input::default(),
            options: Options::default(),
            id: 0,
            life: 200,
            poisoned: false,
            stopped: false,
            weapon: setup.weapon,
            necks: [None; NECK_WORKS],
            targets: &[],
            neck_targets: &setup.neck_targets,
            enemies: setup.neck_targets.len() as u8,
            pushing: false,
            objects: &[],
            key_trg: 0,
        };
        let mut mover = Mover::new(&mut setup.work, &mut setup.player, &tick);
        mover.footstep(0);
        let effects = std::mem::take(&mut mover.work.effects);
        let calls: Vec<_> = effects
            .iter()
            .map(|call| (call.kind, call.row, call.size, call.dir, call.at[1]))
            .collect();
        assert_eq!(
            calls,
            [
                (0x1A, 0, 0xE00, 0x100, -180),
                (0x1A, 0, 0xC00, 0x500, -180),
                (0x1A, 0, 0xC00, 0xD00, -180),
            ]
        );
        let steps = mover.player.take_footsteps();
        assert_eq!(steps.len(), 1);
        assert!(steps[0].water);
        // Out of the water the same footfall starts nothing.
        mover.work.water = 0;
        mover.footstep(0);
        assert!(mover.work.effects.is_empty());
        assert!(!mover.player.take_footsteps()[0].water);
    }

    /// `Pl_water` in the aim: on odd frames up to 5, while the water is above
    /// the ground, four rings at the water's level, each drawing `Rnd` for
    /// its turn and its size.
    #[test]
    fn the_aim_splashes_four_rings_on_odd_frames_in_water() {
        let mut setup = Setup::new(0);
        setup.player.pos = [1000, 0, 2000];
        setup.player.angle = 0x100;
        setup.work.water = -180;
        let bank = (setup.clips.as_slice(), setup.keyframes.as_slice());
        let tick = Tick {
            room: &setup.room,
            skeleton: &setup.skeleton,
            morphs: &[],
            banks: Banks {
                pld: bank,
                weapon: bank,
                room: bank,
                held: bank,
            },
            input: Input::default(),
            options: Options::default(),
            id: 0,
            life: 200,
            poisoned: false,
            stopped: false,
            weapon: setup.weapon,
            necks: [None; NECK_WORKS],
            targets: &[],
            neck_targets: &setup.neck_targets,
            enemies: setup.neck_targets.len() as u8,
            pushing: false,
            objects: &[],
            key_trg: 0,
        };
        let mut mover = Mover::new(&mut setup.work, &mut setup.player, &tick);
        mover.work.move_cnt = 2;
        let drawn = mover.drawn_parts();
        mover.aim_splashes(300, &drawn);
        assert!(
            mover.work.effects.is_empty(),
            "an even frame splashes nothing"
        );
        mover.work.move_cnt = 3;
        let drawn = mover.drawn_parts();
        mover.aim_splashes(300, &drawn);
        let mut rnd = crate::zombie2::Rnd::default();
        let mut expected = rnd;
        let draws: Vec<u16> = (0..8).map(|_| u16::from(expected.draw())).collect();
        mover.work.water_draws(&mut rnd);
        assert_eq!(rnd, expected);
        let calls: Vec<_> = mover
            .work
            .effects
            .iter()
            .map(|call| (call.kind, call.row, call.size, call.dir as u16, call.at[1]))
            .collect();
        let ring = |n: usize, base: u16| {
            let dir = (0x100 + n as u16 * 0x400).wrapping_sub(draws[n * 2] * 4);
            (0x1A, 0, draws[n * 2 + 1] * 4 + base, dir, -180)
        };
        assert_eq!(
            calls,
            [
                ring(0, 0x818),
                ring(1, 0x818),
                ring(2, 0xC00),
                ring(3, 0xC00)
            ]
        );
        mover.work.move_cnt = 7;
        mover.work.effects.clear();
        let drawn = mover.drawn_parts();
        mover.aim_splashes(300, &drawn);
        assert!(mover.work.effects.is_empty(), "nor does a frame past 5");
    }

    #[test]
    fn part_tint_scales_from_0x80() {
        assert_eq!(part_tint(0x0080_8080), [255; 3]);
        assert_eq!(part_tint(0x0070_7070), [224; 3]);
        assert_eq!(part_tint(0), [0; 3]);
        assert_eq!(part_tint(0x0000_00FF), [255, 0, 0]);
        assert_eq!(part_tint(0x0001_207F), [254, 64, 2]);
    }

    #[test]
    fn parts_set_writes_a_parts_colour() {
        let mut work = Work::default();
        parts_set(&mut work, 3, 0, 0x7070, false);
        assert_eq!(work.part_rgb[3], 0x0080_7070);
        parts_set(&mut work, 3, 1, 0x70, false);
        assert_eq!(work.part_rgb[3], 0x0070_7070);
        assert!(
            work.part_rgb
                .iter()
                .enumerate()
                .all(|(n, &rgb)| n == 3 || rgb == DEFAULT_PART_RGB)
        );
        parts_set(&mut work, 4, 0, 0x1234, true);
        parts_set(&mut work, 4, 1, 0x77, true);
        assert_eq!(work.part_rgb[4], 0x0005_0505);
        parts_set(&mut work, -1, 0, 0, false);
        parts_set(&mut work, 40, 2, 0, false);
        for case in 3..=6 {
            parts_set(&mut work, 5, case, 0, false);
        }
        assert_eq!(work.hidden_parts, 0);
        assert_eq!(work.part_rgb[5], DEFAULT_PART_RGB);
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
    fn the_push_routine_squares_the_heading_then_walks() {
        let up = Input {
            up: true,
            ..Input::default()
        };
        let mut setup = Setup::new(0);
        setup.work.routine = [MODE_MOVE, MOVE_FORWARD, 1, 0];
        setup.player.angle = 1100;
        setup.pushing = true;
        // `pl_br_01` takes the bit with forward held, over a held run too.
        setup.tick(Input { run: true, ..up }, 200);
        assert_eq!(setup.work.routine[..2], [MODE_MOVE, MOVE_PUSH]);
        assert_eq!(
            (setup.work.bank, setup.work.move_no),
            (Bank::Weapon, WALK_CLIPS[0])
        );
        // A quarter of `Cdir_y`'s low bits a tick, until square.
        let mut angles = vec![setup.player.angle];
        while setup.work.routine[2] == 1 {
            setup.tick(up, 200);
            angles.push(setup.player.angle);
        }
        assert_eq!(angles, [1081, 1067, 1057, 1024]);
        // The brace, motion 7 of the player's own, to its end.
        let mut ticks = 0;
        while setup.work.routine[2] < 4 {
            setup.tick(up, 200);
            ticks += 1;
            assert_eq!(
                (setup.work.bank, setup.work.move_no),
                (Bank::Pld, PUSH_BRACE_CLIP)
            );
            assert!(ticks < 20);
        }
        // Then motion 8, with the room's sound 4 on its second tick.
        setup.tick(up, 200);
        assert_eq!(setup.work.routine[2], 5);
        assert_eq!(setup.work.move_no, PUSH_CLIP);
        assert!(!setup.work.sounds.contains(&PUSH_SOUND));
        setup.tick(up, 200);
        assert!(setup.work.sounds.contains(&PUSH_SOUND));
        // Forward let go: motion 7 backward and sound 0, then the stand.
        setup.work.sounds.clear();
        setup.tick(Input::default(), 200);
        assert_eq!(setup.work.routine[2], 6);
        assert!(setup.work.sounds.contains(&PUSH_END_SOUND));
        assert_eq!(setup.work.move_no, PUSH_BRACE_CLIP);
        let mut ticks = 0;
        while setup.work.routine[1] == MOVE_PUSH {
            setup.tick(Input::default(), 200);
            ticks += 1;
            assert!(ticks < 20);
        }
        assert_eq!(setup.work.routine[1], MOVE_IDLE);
    }

    #[test]
    fn the_push_ends_when_the_object_stops_giving_way() {
        let up = Input {
            up: true,
            ..Input::default()
        };
        let mut setup = Setup::new(0);
        setup.work.routine = [MODE_MOVE, MOVE_PUSH, 5, 0];
        setup.work.bank = Bank::Pld;
        setup.work.move_no = PUSH_CLIP;
        setup.pushing = true;
        setup.tick(up, 200);
        assert_eq!(setup.work.routine[2], 5);
        setup.pushing = false;
        setup.tick(up, 200);
        assert_eq!(setup.work.routine[2], 6);
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
                neck_targets: Vec::new(),
                targets: Vec::new(),
                enemies: None,
                pushing: false,
                last_key: 0,
                objects: Vec::new(),
            }
        }

        /// A tick with no stop: `Key_trg` is what `input` adds to the last
        /// tick's keys.
        fn tick(&mut self, input: Input, life: i32) -> Option<u8> {
            let key = key_word(&input);
            let key_trg = key & !std::mem::replace(&mut self.last_key, key);
            self.tick_trg(input, key_trg, life)
        }

        /// A tick whose `Pad_set` gave `key_trg`.
        fn tick_trg(&mut self, input: Input, key_trg: u16, life: i32) -> Option<u8> {
            let bank = (self.clips.as_slice(), self.keyframes.as_slice());
            let tick = Tick {
                room: &self.room,
                skeleton: &self.skeleton,
                morphs: &[],
                banks: Banks {
                    pld: bank,
                    weapon: bank,
                    room: bank,
                    held: bank,
                },
                input,
                options: Options::default(),
                id: 0,
                life,
                poisoned: false,
                stopped: false,
                weapon: self.weapon,
                necks: [None; NECK_WORKS],
                targets: &self.targets,
                neck_targets: &self.neck_targets,
                enemies: self.enemies.unwrap_or(self.neck_targets.len() as u8),
                pushing: self.pushing,
                objects: &self.objects,
                key_trg,
            };
            player_move(&mut self.work, &mut self.player, &tick)
        }

        /// [`seed_pose`] in this setup's banks.
        fn seed(&mut self) {
            let bank = (self.clips.as_slice(), self.keyframes.as_slice());
            let tick = Tick {
                room: &self.room,
                skeleton: &self.skeleton,
                morphs: &[],
                banks: Banks {
                    pld: bank,
                    weapon: bank,
                    room: bank,
                    held: bank,
                },
                input: Input::default(),
                options: Options::default(),
                id: 0,
                life: 200,
                poisoned: false,
                stopped: false,
                weapon: self.weapon,
                necks: [None; NECK_WORKS],
                targets: &[],
                neck_targets: &self.neck_targets,
                enemies: 0,
                pushing: false,
                objects: &[],
                key_trg: 0,
            };
            seed_pose(&mut self.work, &mut self.player, &tick);
        }

        /// [`Mover::joint_trans2`] in this setup.
        fn joint_trans2(&mut self, hidden: u32, parts: &mut Vec<Option<Matrix>>) {
            let bank = (self.clips.as_slice(), self.keyframes.as_slice());
            let tick = Tick {
                room: &self.room,
                skeleton: &self.skeleton,
                morphs: &[],
                banks: Banks {
                    pld: bank,
                    weapon: bank,
                    room: bank,
                    held: bank,
                },
                input: Input::default(),
                options: Options::default(),
                id: 0,
                life: 200,
                poisoned: false,
                stopped: false,
                weapon: self.weapon,
                necks: [None; NECK_WORKS],
                targets: &[],
                neck_targets: &self.neck_targets,
                enemies: 0,
                pushing: false,
                objects: &[],
                key_trg: 0,
            };
            let mover = Mover {
                work: &mut self.work,
                player: &mut self.player,
                tick: &tick,
                arrived: None,
                stairs: None,
            };
            mover.joint_trans2(hidden, parts);
        }
    }

    #[test]
    fn a_hidden_part_keeps_last_ticks_matrix() {
        let mut setup = Setup::new(0);
        setup.skeleton.relative = vec![[0; 3], [100, 0, 0], [0, 50, 0]];
        setup.skeleton.children = vec![vec![1], vec![2], vec![]];
        for keyframe in &mut setup.keyframes {
            keyframe.rotations = vec![[0; 3]; 3];
        }
        setup.seed();
        let start = setup.player.pos;
        let at = |parts: &[Option<Matrix>], part: usize| parts[part].unwrap().t;
        let mut parts = Vec::new();
        setup.joint_trans2(0, &mut parts);
        assert_eq!(parts.len(), 3);
        let part1 = at(&parts, 1);
        assert_eq!(at(&parts, 2), [part1[0], part1[1] + 50, part1[2]]);
        setup.player.pos[0] += 1000;
        setup.joint_trans2(1 << 1, &mut parts);
        assert_eq!(at(&parts, 0)[0], start[0] + 1000, "the root moves");
        assert_eq!(at(&parts, 1), part1, "the hidden part stays");
        assert_eq!(
            at(&parts, 2),
            [part1[0], part1[1] + 50, part1[2]],
            "on the stale parent"
        );
        setup.joint_trans2(0, &mut parts);
        assert_eq!(at(&parts, 1)[0], part1[0] + 1000);
    }

    #[test]
    fn a_new_games_joints_start_on_the_idle_frame_it_shows() {
        let mut setup = Setup::new(0);
        (setup.clips, setup.keyframes) = bank(IDLE_CLIPS[0] as usize + 1, 4, 0);
        for (index, keyframe) in setup.keyframes.iter_mut().enumerate() {
            keyframe.rotations = vec![[index as i16 * 10, 0, 0]; 2];
        }
        assert!(setup.player.anim.applied_pose().is_none());
        setup.seed();
        // `Work::default` has shown the idle clip's first frame (`Move_cnt` 1).
        assert_eq!(
            setup.player.anim.applied_pose(),
            Some(setup.keyframes[0].clone())
        );
        // A door keeps the pose the last room left.
        setup.work = setup.work.entering_room();
        setup
            .player
            .anim
            .set_applied_pose(Some(setup.keyframes[3].clone()));
        setup.seed();
        assert_eq!(
            setup.player.anim.applied_pose(),
            Some(setup.keyframes[3].clone())
        );
    }

    /// Runs a dead player's hold until `done`, or panics.
    fn run_held(setup: &mut Setup, life: &mut i16, done: impl Fn(&Work) -> bool) {
        let bank = (setup.clips.as_slice(), setup.keyframes.as_slice());
        let tick = Tick {
            room: &setup.room,
            skeleton: &setup.skeleton,
            morphs: &[],
            banks: Banks {
                pld: bank,
                weapon: bank,
                room: bank,
                held: bank,
            },
            input: Input::default(),
            options: Options::default(),
            id: 0,
            life: 0,
            poisoned: false,
            stopped: false,
            weapon: setup.weapon,
            necks: [None; NECK_WORKS],
            targets: &[],
            neck_targets: &[],
            enemies: 0,
            pushing: false,
            objects: &[],
            key_trg: 0,
        };
        let mut rnd = crate::zombie2::Rnd::default();
        for _ in 0..200 {
            let mut held = Held {
                holder: [0; 3],
                holder_angle: 0,
                holder_aim: 0,
                holder_id: 0x10,
                holder_move_cnt: 0,
                holder_grip: None,
                draw_buffer: 0,
                rnd: &mut rnd,
                life,
                released: false,
                knocks: 0,
            };
            player_held(&mut setup.work, &mut setup.player, &tick, &mut held);
            if done(&setup.work) {
                return;
            }
        }
        panic!("never got there: {:?}", setup.work.routine);
    }

    /// `Pl_Em_damage` raises `fg_status`'s `0x40` on each held tick, so a hold
    /// that starts from a free stance refuses START too, not only one that
    /// catches the player aiming. `zombies_207_grabs` checks it against the
    /// original: START opens the status screen on a grab's tick, not ten
    /// ticks after it or on the tick after the free, and opens it again two
    /// ticks after the free.
    #[test]
    fn a_zombies_hold_holds_the_game_on_every_held_tick() {
        let mut setup = Setup::new(0);
        setup.tick(Input::default(), 190);
        assert!(!setup.work.holds_game);
        (setup.clips, setup.keyframes) = bank(0x11, 4, 0);
        setup.work.routine = [MODE_HELD, 0, 0, 0];
        let mut life = 190;
        run_held(&mut setup, &mut life, |_| true);
        assert!(setup.work.holds_game);
        // The free tick's hold raises it once more; the next tick, free of
        // the hold from its start, drops it.
        setup.work.routine = [MODE_MOVE, MOVE_IDLE, 0, 0];
        setup.tick(Input::default(), 190);
        assert!(!setup.work.holds_game);
    }

    #[test]
    fn a_player_dying_in_a_zombies_hands_ends_with_lifes_mark() {
        let mut setup = Setup::new(0);
        (setup.clips, setup.keyframes) = bank(0x11, 4, 0);
        setup.work.routine = [MODE_HELD_DIE, 1, 0, 0];
        let mut life = -20;
        run_held(&mut setup, &mut life, |work| work.routine[2] == 1);
        assert_eq!(setup.work.move_no, 0xF);
        assert_eq!(life, -20);
        run_held(&mut setup, &mut life, |work| work.routine[2] == 3);
        assert_eq!(setup.work.move_no, 0x10);
        assert_eq!(life, i16::MIN);
    }

    #[test]
    fn a_crows_release_keeps_the_last_frame_of_its_own_bank() {
        let keyframes = |rotation: i16, count: usize| -> Vec<Keyframe> {
            (0..count)
                .map(|index| Keyframe {
                    offset: [0; 3],
                    vertex: [0; 3],
                    rotations: vec![[rotation + index as i16, 0, 0]],
                })
                .collect()
        };
        let setup = Setup::new(0);
        let (held_clips, _) = bank(3, 2, 0);
        let held_keyframes = keyframes(100, 2);
        let weapon_keyframes = keyframes(3000, 2);
        let tick = Tick {
            room: &setup.room,
            skeleton: &setup.skeleton,
            morphs: &[],
            banks: Banks {
                pld: (&held_clips, &weapon_keyframes),
                weapon: (&held_clips, &weapon_keyframes),
                room: (&held_clips, &weapon_keyframes),
                held: (&held_clips, &held_keyframes),
            },
            input: Input::default(),
            options: Options::default(),
            id: 0,
            life: 200,
            poisoned: false,
            stopped: false,
            weapon: setup.weapon,
            necks: [None; NECK_WORKS],
            targets: &[],
            neck_targets: &[],
            enemies: 0,
            pushing: false,
            objects: &[],
            key_trg: 0,
        };
        let Setup {
            mut work,
            mut player,
            ..
        } = setup;
        // `FUN_00463210`'s peck, on its last frame.
        work.routine = [MODE_HELD, 0, 4, 0];
        work.bank = Bank::Held;
        work.move_no = 2;
        work.move_cnt = 1;
        work.hokan = 0;
        let mut rnd = crate::zombie2::Rnd::default();
        let mut life = 200;
        let mut held = Held {
            holder: [0; 3],
            holder_angle: 0,
            holder_aim: 0,
            holder_id: crate::zombie2::CROW_ID,
            holder_move_cnt: 0,
            holder_grip: None,
            draw_buffer: 0,
            rnd: &mut rnd,
            life: &mut life,
            released: false,
            knocks: 0,
        };
        player_held(&mut work, &mut player, &tick, &mut held);
        assert!(held.released);
        assert_eq!(work.bank, Bank::Weapon);
        let pose = player
            .anim
            .pose_keyframe(&held_clips, &weapon_keyframes)
            .unwrap();
        assert_eq!(pose.rotations, vec![[101, 0, 0]]);
    }

    #[test]
    fn a_dead_players_wait_starts_counting_on_the_tick_it_draws() {
        let mut setup = Setup::new(0);
        setup.work.routine = [MODE_HELD_DIE, 0, 4, 0];
        let mut life = i16::MIN;
        run_held(&mut setup, &mut life, |work| work.routine[2] == 5);
        // `Rnd`'s first draw, 0x7D, gives 0x1D + 3, less the tick just run.
        assert_eq!(setup.work.timers[2], 0x1F);
    }

    /// A back walk from a stand at the origin, facing +x, with `targets`.
    fn backing(targets: Vec<NeckTarget>) -> Setup {
        let mut setup = Setup::new(10);
        (setup.clips, setup.keyframes) = bank(10, 40, 10);
        setup.player.pos = [0; 3];
        setup.player.angle = 0;
        setup.neck_targets = targets;
        setup
    }

    fn back() -> Input {
        Input {
            down: true,
            ..Input::default()
        }
    }

    /// An enemy ahead at `distance`, facing the player.
    fn facing_enemy(distance: u32) -> NeckTarget {
        NeckTarget {
            angle: 0x800,
            ..neck_target(2, 0, 0.0, distance)
        }
    }

    #[test]
    fn a_turns_footfall_is_a_steps_noise() {
        let mut setup = Setup::new(10);
        (setup.clips, setup.keyframes) = bank(10, 40, 10);
        for clip in &mut setup.clips {
            for frame in &mut clip.frames {
                frame.flags = FRAME_FOOTSTEP;
            }
        }
        let left = Input {
            left: true,
            ..Input::default()
        };
        setup.tick(left, 200);
        assert_eq!(setup.work.routine[1], MOVE_ROTATE);
        assert_ne!(setup.work.noise & 2, 0, "a footfall's noise");
    }

    /// A setup in water `water` whose skeleton has parts 11 and 14 (the
    /// hands) on the root, at heights `hand11` and `hand14`.
    fn wading(water: i16, hand11: i16, hand14: i16) -> Setup {
        let mut setup = Setup::new(10);
        (setup.clips, setup.keyframes) = bank(10, 40, 10);
        for keyframe in &mut setup.keyframes {
            keyframe.rotations = vec![[0; 3]; 15];
        }
        setup.skeleton.relative = vec![[0; 3]; 15];
        setup.skeleton.relative[11] = [-200, hand11, 0];
        setup.skeleton.relative[14] = [200, hand14, 0];
        setup.skeleton.children = vec![Vec::new(); 15];
        setup.skeleton.children[0] = vec![11, 14];
        setup.work.water = water;
        setup.seed();
        setup
    }

    /// Runs `ticks` ticks of `input` and gives, for each, `Move_cnt` after
    /// the move, the `Esp_call`s it made (with their `Rnd` drawn from `rnd`)
    /// and the `Rnd` draws they took.
    fn wade(
        setup: &mut Setup,
        input: Input,
        ticks: usize,
        rnd: &mut crate::zombie2::Rnd,
    ) -> Vec<(u8, Vec<EffectCall>, Vec<u16>)> {
        (0..ticks)
            .map(|_| {
                setup.tick(input, 200);
                let before = *rnd;
                setup.work.water_draws(rnd);
                let effects = std::mem::take(&mut setup.work.effects);
                let mut replay = before;
                let mut draws = Vec::new();
                while replay != *rnd && draws.len() < 16 {
                    draws.push(u16::from(replay.draw()));
                }
                (setup.work.move_cnt, effects, draws)
            })
            .collect()
    }

    fn turn_left() -> Input {
        Input {
            left: true,
            ..Input::default()
        }
    }

    fn walk_forward() -> Input {
        Input {
            up: true,
            ..Input::default()
        }
    }

    /// `pl_mv_01`'s, `_02`'s, `_03`'s and `_04`'s splashes: in water within
    /// 300 of a hand, on each odd `Move_cnt`, one `Esp_call(Rnd() * 4 +
    /// 0x60C | 0x1A000000)` off part 11, then one off part 14, 300 down each
    /// hand's matrix as the last tick drew it, and nothing on even ones.
    #[test]
    fn the_walk_and_the_turn_splash_off_the_hands_on_odd_frames_in_deep_water() {
        let run = Input {
            up: true,
            run: true,
            ..Input::default()
        };
        for (input, routine) in [
            (walk_forward(), MOVE_FORWARD),
            (turn_left(), MOVE_ROTATE),
            (run, MOVE_RUN),
            (back(), MOVE_BACKWARD),
        ] {
            let mut setup = wading(-800, -900, -900);
            setup.player.pos = [1000, 0, 2000];
            setup.seed();
            let mut rnd = crate::zombie2::Rnd::default();
            let mut odd = 0;
            for tick in 0..12 {
                let angle = setup.player.angle;
                let hands = {
                    let bank = (setup.clips.as_slice(), setup.keyframes.as_slice());
                    let tick = Tick {
                        room: &setup.room,
                        skeleton: &setup.skeleton,
                        morphs: &[],
                        banks: Banks {
                            pld: bank,
                            weapon: bank,
                            room: bank,
                            held: bank,
                        },
                        input,
                        options: Options::default(),
                        id: 0,
                        life: 200,
                        poisoned: false,
                        stopped: false,
                        weapon: setup.weapon,
                        necks: [None; NECK_WORKS],
                        targets: &[],
                        neck_targets: &[],
                        enemies: 0,
                        pushing: false,
                        objects: &[],
                        key_trg: 0,
                    };
                    let mover = Mover::new(&mut setup.work, &mut setup.player, &tick);
                    [
                        mover.part_point(11, [0, 300, 0]),
                        mover.part_point(14, [0, 300, 0]),
                    ]
                };
                let [(move_cnt, effects, draws)] =
                    <[_; 1]>::try_from(wade(&mut setup, input, 1, &mut rnd)).unwrap();
                assert_eq!(setup.work.routine[1], routine, "tick {tick}");
                if move_cnt & 1 == 0 {
                    assert!(
                        effects.is_empty(),
                        "{routine}: an even frame splashes nothing"
                    );
                    assert!(draws.is_empty(), "{routine}: nor draws");
                    continue;
                }
                odd += 1;
                assert_eq!(draws.len(), 2, "{routine}: a draw for each hand");
                let dir = setup.player.angle as i16;
                let expected: Vec<_> = (0..2)
                    .map(|hand| {
                        // The hands the last tick drew, before this
                        // tick's move.
                        let at = hands[hand].expect("a drawn hand");
                        EffectCall {
                            kind: 0x1A,
                            row: 0,
                            size: draws[hand] * 4 + 0x60C,
                            dir,
                            at: at.map(|axis| axis as i16),
                            motion: None,
                            on: None,
                            turn: None,
                        }
                    })
                    .collect();
                assert_eq!(effects, expected, "{routine}, tick {tick}");
                if routine != MOVE_ROTATE {
                    assert_eq!(angle as i16, dir, "{routine}: the walks don't turn");
                }
            }
            assert!(odd >= 5, "{routine}: {odd} odd frames");
        }
    }

    /// Each hand splashes only while the water is within 300 of it.
    #[test]
    fn a_wading_splash_wants_the_water_within_300_of_the_hand() {
        // Part 11 at -500 (within 300 from -200), part 14 at -900 (from -600).
        for (water, splashes) in [(-400, 1), (-700, 2), (-180, 0), (-150, 0)] {
            let mut setup = wading(water, -500, -900);
            let mut rnd = crate::zombie2::Rnd::default();
            for (move_cnt, effects, draws) in wade(&mut setup, walk_forward(), 6, &mut rnd) {
                let expected = if move_cnt & 1 != 0 { splashes } else { 0 };
                assert_eq!(effects.len(), expected, "water {water}, frame {move_cnt}");
                assert_eq!(draws.len(), expected, "water {water}, frame {move_cnt}");
            }
        }
    }

    /// Out of the water (`Water` 0) the walk and the turn splash nothing and
    /// draw nothing, though `0 < hand + 300` would hold.
    #[test]
    fn out_of_the_water_the_walk_and_the_turn_splash_nothing() {
        for input in [walk_forward(), turn_left()] {
            let mut setup = wading(0, 0, 0);
            let mut rnd = crate::zombie2::Rnd::default();
            for (_, effects, draws) in wade(&mut setup, input, 6, &mut rnd) {
                assert!(effects.is_empty());
                assert!(draws.is_empty());
            }
            assert_eq!(rnd, crate::zombie2::Rnd::default());
        }
    }

    #[test]
    fn the_back_walk_looks_at_an_enemy_in_view_every_15_ticks() {
        let mut setup = backing(vec![facing_enemy(2000)]);
        setup.tick(back(), 200);
        assert_eq!(setup.work.routine[1], MOVE_BACKWARD);
        assert_eq!(setup.work.move_no, BACK_LOOK_CLIP, "seen on the first tick");
        assert_eq!(setup.work.move_cnt, 1, "on the frame it had");
        assert_eq!(setup.work.spd, [10, 0, 0], "a first frame steps as ever");
        assert_ne!(setup.work.noise & 2, 0, "a step's noise");
        assert_eq!(setup.work.back_wait, BACK_LOOK_WAIT - 1);

        setup.neck_targets.clear();
        for _ in 1..BACK_LOOK_WAIT {
            setup.tick(back(), 200);
            assert_eq!(setup.work.move_no, BACK_LOOK_CLIP, "not time to look yet");
        }
        assert_eq!(setup.work.back_wait, 0);
        let frame = setup.work.move_cnt;
        assert_eq!(frame, 15);
        setup.tick(back(), 200);
        assert_eq!(setup.work.move_no, BACK_CLIPS[0], "nobody in view");
        assert_eq!(
            setup.work.move_cnt,
            frame / BACK_LOOK_FRAME_STEP * BACK_LOOK_FRAME_STEP + 1,
            "back on a multiple of 7, then a frame on"
        );
        assert_eq!(setup.work.spd, [0; 3], "a swap mid-clip loses its step");
        assert!(!setup.work.back_look);
        assert_eq!(setup.work.back_wait, BACK_LOOK_WAIT - 1);
    }

    #[test]
    fn the_back_walk_looks_only_at_an_enemy_alive_upright_near_in_view_and_facing() {
        let behind = std::f64::consts::PI;
        let cases = [
            (
                "the second player",
                NeckTarget {
                    index: 1,
                    ..facing_enemy(2000)
                },
            ),
            (
                "dead",
                NeckTarget {
                    life: -1,
                    ..facing_enemy(2000)
                },
            ),
            (
                "on the floor",
                NeckTarget {
                    kind: 0x2000,
                    ..facing_enemy(2000)
                },
            ),
            // `SquareRoot0` rounds down: 5000 itself comes out under the reach.
            ("too far", facing_enemy(BACK_LOOK_REACH + 100)),
            (
                "facing away",
                NeckTarget {
                    angle: 0,
                    ..facing_enemy(2000)
                },
            ),
            (
                "behind",
                NeckTarget {
                    angle: 0,
                    ..neck_target(2, 0, behind, 2000)
                },
            ),
        ];
        for (what, target) in cases {
            let mut setup = backing(vec![target]);
            setup.tick(back(), 200);
            assert_eq!(setup.work.move_no, BACK_CLIPS[0], "{what}");
        }
        let mut setup = backing(vec![facing_enemy(BACK_LOOK_REACH - 10)]);
        setup.tick(back(), 200);
        assert_eq!(setup.work.move_no, BACK_LOOK_CLIP, "just within reach");

        let mut setup = backing(vec![facing_enemy(2000)]);
        setup.enemies = Some(0);
        setup.tick(back(), 200);
        assert_eq!(setup.work.move_no, BACK_CLIPS[0], "the room has no enemies");

        let mut setup = backing(vec![facing_enemy(2000)]);
        setup.work.d_life_u = 1;
        setup.tick(back(), 50);
        assert_eq!(
            (setup.work.d_life_u, setup.work.move_no),
            (1, BACK_CLIPS[1]),
            "hurt, it doesn't look"
        );
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
            rounds: u8::from(loaded) * 18,
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

    /// Room 203's box at rest against the shelf (`box_203`): its box's
    /// centre at z -25186, 600 by 675, its top at -1800.
    fn box_203() -> crate::objects::ObjectRecord {
        let mut record = crate::objects::ObjectRecord {
            flag: crate::objects::OBJECT_FLAG_ACTIVE,
            pos: [-18673, 0, -25236],
            rotation: [0, -1024, 0],
            be_flg: 0x181,
            id: 2,
            atd2: crate::objects::ObjectAtd {
                ofs: [50, -900, 0],
                at_w: 675,
                at_h: 900,
                at_d: 600,
            },
            ..crate::objects::ObjectRecord::default()
        };
        record.set_ofs();
        record
    }

    /// Leon in a room with no walls and 203's box, at `pos`, heading `angle`.
    fn by_the_box(pos: [i32; 3], angle: u16) -> Setup {
        let mut setup = Setup::new(0);
        (setup.clips, setup.keyframes) = bank(10, 60, 0);
        setup.room.sca = Some(crate::state::Sca::default());
        setup.objects = vec![box_203()];
        setup.player.pos = pos;
        setup.player.angle = angle;
        setup.player.radius = 450;
        setup.work.routine = [MODE_MOVE, MOVE_IDLE, 0, 0];
        setup
    }

    /// `box_203`: the action key at the box's +z face, facing it.
    #[test]
    fn the_action_key_climbs_onto_a_box_it_faces() {
        let mut setup = by_the_box([-18700, 0, -24060], 1024);
        setup.tick(action(), 200);
        assert_eq!(setup.work.routine[..3], [MODE_MOVE, MOVE_CLIMB, 0]);
        assert_eq!(setup.work.on_object, Some(0));
        // `Oma_front_ck`'s arm takes no more than 38 degrees.
        let mut setup = by_the_box([-18700, 0, -24060], 1024 + 0x1C0);
        setup.tick(action(), 200);
        assert_ne!(setup.work.routine[1], MOVE_CLIMB);
        // Nor an object that can't be climbed (10C's statues, 216's van).
        let mut setup = by_the_box([-18700, 0, -24060], 1024);
        setup.objects[0].be_flg = 0x81;
        setup.tick(action(), 200);
        assert_ne!(setup.work.routine[1], MOVE_CLIMB);
        assert_eq!(setup.work.on_object, None);
    }

    /// `pl_mv_08`'s `Oma_ob_pull2`: 4 a frame toward the box, nothing once
    /// the player is on its top.
    #[test]
    fn a_climb_onto_an_object_draws_the_player_in() {
        let mut setup = by_the_box([-18700, 0, -24060], 1024);
        setup.work.on_object = Some(0);
        setup.work.routine = [MODE_MOVE, MOVE_CLIMB, 0, 2];
        setup.tick(Input::default(), 200);
        assert_eq!(setup.player.pos, [-18700, 0, -24064]);
        let mut setup = by_the_box([-18700, -1800, -25064], 1024);
        setup.work.on_object = Some(0);
        setup.work.routine = [MODE_MOVE, MOVE_CLIMB, 0, 4];
        setup.tick(Input::default(), 200);
        assert_eq!(setup.player.pos, [-18700, -1800, -25064]);
        assert_eq!(setup.work.routine[1], MOVE_IDLE);
    }

    /// `box_203`'s way down: off the top's +z edge, with no drop record.
    #[test]
    fn the_action_key_jumps_down_off_the_object_stood_on() {
        // As `box_203` presses it, a little off square.
        let mut setup = by_the_box([-18574, -1800, -24736], 3120);
        setup.work.floor = 1;
        setup.work.on_object = Some(0);
        setup.tick(action(), 200);
        assert_eq!(
            setup.work.routine[..3],
            [MODE_MOVE, MOVE_CLIMB, CLIMB_IS_DROP]
        );
        assert_eq!(setup.work.on_object, None);
        // From the middle the point ahead is still over the box.
        let mut setup = by_the_box([-18673, -1800, -25186], 3072);
        setup.work.floor = 1;
        setup.work.on_object = Some(0);
        setup.tick(action(), 200);
        assert_ne!(setup.work.routine[1], MOVE_CLIMB);
        // Off the floor with no object stood on and no drop record, nothing.
        let mut setup = by_the_box([0, -1800, 0], 3072);
        setup.tick(action(), 200);
        assert_ne!(setup.work.routine[1], MOVE_CLIMB);
    }

    /// No jump down onto another object: one 900 high under the landing
    /// point holds it. Its box is stale (`Atd` at the origin), as
    /// `Sca_ck_hit`'s `Oma_set_ofs` finds it.
    #[test]
    fn no_jump_down_onto_another_object() {
        let mut setup = by_the_box([-18574, -1800, -24736], 3120);
        setup.work.floor = 1;
        setup.work.on_object = Some(0);
        setup.objects.push(crate::objects::ObjectRecord {
            flag: crate::objects::OBJECT_FLAG_ACTIVE,
            pos: [-18574, 0, -23736],
            be_flg: 0x81,
            atd2: crate::objects::ObjectAtd {
                ofs: [0, -900, 0],
                at_w: 675,
                at_h: 900,
                at_d: 600,
            },
            ..crate::objects::ObjectRecord::default()
        });
        setup.tick(action(), 200);
        assert_ne!(setup.work.routine[1], MOVE_CLIMB);
        assert_eq!(setup.work.on_object, Some(0));
    }

    /// `Sca_ck_em` leaves `Sca_info` empty on an object: no stairs.
    #[test]
    fn no_stairs_from_an_object() {
        let mut setup = at_stairs([1500, 0, 700], 0xC00);
        setup.objects = vec![box_203()];
        setup.work.on_object = Some(0);
        setup.tick(action(), 200);
        assert_ne!(setup.work.routine[1], MOVE_STAIRS);
    }

    /// `pl_mv_09` lands on an object's top and stands on it. The landing
    /// takes `Ground` from the shadow's probe (`Game_loop`'s), here the
    /// body.
    #[test]
    fn a_jump_down_lands_on_an_objects_top() {
        let mut setup = by_the_box([-18673, -3600, -25186 + 1000], 1024);
        setup.work.routine = [MODE_MOVE, MOVE_JUMP_DOWN, 2, 2];
        for _ in 0..100 {
            if setup.work.routine[1] != MOVE_JUMP_DOWN {
                break;
            }
            setup.work.kage_pos = Some(setup.player.pos);
            setup.tick(Input::default(), 200);
        }
        assert_eq!(setup.work.on_object, Some(0));
        assert_eq!(setup.player.pos, [-18673, -1800, -25186]);
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
    fn a_climb_steps_floor_the_whole_flight_before_its_first_step() {
        for (pos, angle, start) in [([1500, 0, 700], 0xC00, 0), ([1500, 0, 2300], 0x400, 5)] {
            let mut setup = at_stairs(pos, angle);
            setup.work.floor = start;
            setup.tick(action(), 200);
            let flight = setup.work.timer1 as u8;
            let up = setup.work.timer2 == 0;
            assert!(flight > 0);
            while setup.work.routine[2] < 3 {
                assert_eq!(setup.work.floor, start, "the turn keeps it");
                setup.tick(Input::default(), 200);
            }
            let want = if up { start + flight } else { start - flight };
            assert_eq!(setup.work.floor, want);
            for _ in 0..200 {
                if setup.work.routine[1] != MOVE_STAIRS {
                    break;
                }
                setup.tick(Input::default(), 200);
                assert_eq!(setup.work.floor, want, "not with the height");
            }
        }
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

    /// `pl_mv_07` leads the shadow off the body on the stairs, probing its
    /// ground further on, and brings both back to the body for the walk off.
    #[test]
    fn a_climb_leads_the_shadow_and_hands_it_back() {
        let mut setup = at_stairs([1500, 0, 700], 0xC00);
        setup.tick(action(), 200);
        let mut most = 0;
        for _ in 0..200 {
            setup.tick(Input::default(), 200);
            if setup.work.routine[1] != MOVE_STAIRS {
                break;
            }
            most = most.max(setup.work.kage_ofs);
        }
        assert_eq!(most, STAIRS_KAGE_LEAD);
        assert_eq!(setup.work.kage_ofs, 0);
        assert_eq!(setup.work.kage_pos, None);
        // Back on part 0, probed where it is.
        assert_eq!(setup.work.kage_pos(), setup.work.root.map(|root| root.t));
    }

    #[test]
    fn only_a_handgun_raises_into_the_stance() {
        // The spark shot (`pl_mv05_w3`) isn't ported.
        let mut setup = armed(true, false);
        setup.weapon.id = 0xE;
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
        assert!(
            setup.player.take_sounds().is_empty(),
            "a shot doesn't click"
        );

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
        let clicks = setup.player.take_sounds();
        assert_eq!(clicks.len(), 1, "an empty gun clicks");
        assert_eq!(clicks[0].id, SOUND_EMPTY);

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

    /// [`armed`] with a chain of 15 parts, so that the hand (11) has a
    /// matrix.
    fn armed_with_hands(loaded: bool, spare: bool) -> Setup {
        let mut setup = armed(loaded, spare);
        for keyframe in &mut setup.keyframes {
            keyframe.rotations = vec![[0; 3]; 15];
        }
        setup.skeleton.relative = std::iter::once([0; 3])
            .chain(std::iter::repeat_n([0, -100, 0], 14))
            .collect();
        setup.skeleton.children = (1..15u8).map(|child| vec![child]).collect();
        setup.skeleton.children.push(Vec::new());
        setup
    }

    /// The Colt's shot, from the hold: its five calls and `Esp_call3D`'s
    /// turned smoke on the second frame, on the hand.
    #[test]
    fn the_colts_second_frame_makes_its_calls_and_the_turned_smoke() {
        let mut setup = armed_with_hands(true, true);
        setup.weapon.id = COLT;
        setup.work.routine = [MODE_MOVE, MOVE_AIM, 1, 1];
        setup.work.status_flg = TILT_LEVEL;
        setup.tick(aiming(true, true), 200);
        assert_eq!(setup.work.routine[2], 2);
        setup.tick(aiming(true, false), 200);
        assert!(setup.work.fired);
        assert!(setup.work.effects.is_empty(), "nothing on the first frame");
        setup.tick(aiming(true, false), 200);
        let angle = setup.player.angle as i16;
        let calls: Vec<_> = std::mem::take(&mut setup.work.effects)
            .iter()
            .map(|call| {
                (
                    u32::from(call.kind) << 24 | u32::from(call.row) << 16 | u32::from(call.size),
                    call.dir,
                    call.on.map(|(part, _, point)| (part, point)),
                    call.turn,
                )
            })
            .collect();
        let on = |point: [i16; 3]| Some((SHOT_PART as u8, point));
        assert_eq!(
            calls,
            [
                (0x100_1000, 0, on([0x82, 0x2DA, 0]), None),
                (0x104_1000, 0, on([0x82, 0x118, 0]), None),
                (
                    0x106_1000,
                    angle.wrapping_add(0x800),
                    on([0x82, 0x118, 0]),
                    None
                ),
                (0x30A_0C00, 0, on([0x82, 0x118, 0]), None),
                (0x30C_1000, 0, on([0x82, 0x33E, 0]), None),
                (0x107_3000, 0, on([0x82, 0x38E, 0]), Some([0, 0, 0x400])),
            ]
        );
    }

    /// `pl_mv05_w1_t4` with the Colt: the frame the others let the
    /// magazine go on throws the cases out of the hand instead.
    #[test]
    fn the_colts_reload_throws_out_its_cases_rather_than_a_magazine() {
        for (id, cases) in [(COLT, true), (2, false)] {
            let mut setup = armed_with_hands(false, true);
            setup.weapon.id = id;
            setup.work.routine = [MODE_MOVE, MOVE_AIM, 4, 0];
            setup.work.status_flg = TILT_LEVEL | STATUS_MAGAZINE;
            setup.clips[usize::from(AIM_RELOAD_CLIP)].frames[1].flags = FRAME_MAGAZINE;
            setup.tick(aiming(false, false), 200);
            setup.tick(aiming(false, false), 200);
            let thrown: Vec<_> = setup
                .work
                .effects
                .iter()
                .map(|call| {
                    (
                        call.kind,
                        call.row,
                        call.size,
                        call.dir,
                        call.on.map(|on| (on.0, on.2)),
                    )
                })
                .collect();
            if cases {
                let dir = setup.player.angle as i16;
                assert_eq!(
                    thrown,
                    [(
                        2,
                        7,
                        0x800,
                        dir,
                        Some((MAGAZINE_HAND as u8, [0x64, 0xDC, 0]))
                    )]
                );
                assert!(setup.work.magazine.is_none());
            } else {
                assert!(thrown.is_empty());
                assert!(setup.work.magazine.is_some(), "the handgun's magazine");
            }
            assert_eq!(setup.work.status_flg & STATUS_MAGAZINE, 0);
        }
    }

    /// [`armed_with_hands`] holding `id` with `rounds`, on clips long enough
    /// for a burst, aimed level.
    fn bursting(id: u8, rounds: u8) -> Setup {
        let mut setup = armed_with_hands(rounds != 0, true);
        (setup.clips, setup.keyframes) = bank(0x14, 40, 0);
        for keyframe in &mut setup.keyframes {
            keyframe.rotations = vec![[0; 3]; 15];
        }
        setup.weapon.id = id;
        setup.weapon.rounds = rounds;
        setup.work.routine = [MODE_MOVE, MOVE_AIM, 1, 1];
        setup.work.status_flg = TILT_LEVEL;
        setup.work.move_no = BURST_HOLD_CLIPS[1];
        // A press from the hold starts the burst.
        setup.tick(aiming(true, true), 200);
        assert_eq!(setup.work.routine[2], 2);
        setup
    }

    /// The words of this tick's effects, with their turn from the body's.
    fn burst_calls(setup: &mut Setup) -> Vec<(u32, i16, [i16; 3])> {
        let angle = setup.player.angle as i16;
        std::mem::take(&mut setup.work.effects)
            .iter()
            .map(|call| {
                (
                    u32::from(call.kind) << 24 | u32::from(call.row) << 16 | u32::from(call.size),
                    call.dir.wrapping_sub(angle),
                    call.on.map_or([0; 3], |(_, _, point)| point),
                )
            })
            .collect()
    }

    /// `pl_mv05_w2_t2` with the sub-machine gun: the burst on its level clip,
    /// `Tama_decrease_i` on every third frame, the eighth of which takes two
    /// rounds, and `pl_w0f_t2_move`'s flash and smoke on every third frame
    /// and a case on every sixth.
    #[test]
    fn the_smg_bursts_and_spends_two_rounds_in_eight_calls() {
        let mut setup = bursting(SMG, 50);
        let mut spent_at = None;
        for tick in 1..=24u8 {
            setup.tick(aiming(true, false), 200);
            assert_eq!(setup.work.routine[2..], [2, 2], "tick {tick}");
            assert_eq!(setup.work.move_no, BURST_CLIPS[1]);
            assert_eq!(setup.work.noise & 1, 1);
            let frame = setup.work.move_cnt;
            let mut expected = Vec::new();
            // The flash and smoke go out at 0, not along the body.
            let at_zero = (setup.player.angle as i16).wrapping_neg();
            if frame % 3 == 1 {
                for word in SMG_FLASH {
                    expected.push((word, at_zero, [0x46, 1000, -60]));
                }
            }
            if frame % 6 == 1 {
                expected.push((0x201_0900, 0x4B0, [0xAA, 0x118, 0x50]));
            }
            assert_eq!(burst_calls(&mut setup), expected, "frame {frame}");
            if spent_at.is_none() && setup.work.spent != 0 {
                spent_at = Some(tick);
            }
        }
        // Calls on ticks 1, 4, ... 22: the eighth takes two.
        assert_eq!(spent_at, Some(22));
        assert_eq!(setup.work.spent, 2);
        assert_eq!(setup.work.tama_count, 0);
    }

    /// `pl_mv05_w2_t2` with the flamethrower: `Tama_decrease_i` on every
    /// tick, `pl_w10_t2_move`'s flame along the body on every third frame,
    /// and its sounds on the first and the eleventh.
    #[test]
    fn the_flamethrower_spends_every_tick_and_throws_a_flame_every_third() {
        let mut setup = bursting(FLAMETHROWER, 50);
        setup.player.take_sounds();
        let mut sounds = Vec::new();
        for tick in 1..=12u8 {
            setup.tick(aiming(true, false), 200);
            let frame = setup.work.move_cnt;
            let expected: Vec<_> = if frame % 3 == 1 {
                vec![(0x31D_1200, 0, [0x96, 0x4B0, 0])]
            } else {
                Vec::new()
            };
            assert_eq!(burst_calls(&mut setup), expected, "frame {frame}");
            sounds.extend(
                setup
                    .player
                    .take_sounds()
                    .into_iter()
                    .map(|sound| (frame, sound.id)),
            );
            if tick == 7 {
                assert_eq!(setup.work.spent, 0);
            }
            if tick == 8 {
                assert_eq!(setup.work.spent, 2);
            }
        }
        assert_eq!(sounds, [(1, SOUND_FLAME_START), (0xB, SOUND_FLAME_GO_ON)]);
    }

    /// `pl_mv05_w2_t2`'s `Weapon_at_ck` with a body in front: the
    /// sub-machine gun's shot looks for it, and hits it, on the burst's first
    /// tick and every third after; the flamethrower's never looks (nor has
    /// it records to look with), as its flames burn what they reach instead.
    #[test]
    fn the_smgs_burst_hits_on_every_third_tick_and_the_flames_never() {
        for id in [SMG, FLAMETHROWER] {
            let mut setup = bursting(id, 50);
            let [x, _, z] = setup.player.pos;
            // Straight ahead of the body at angle 0, in reach.
            let (dx, dz) = (1500, 0);
            setup.targets = vec![crate::weapon2::Target {
                slot: 3,
                id: 0x1F,
                be_flg: 1 | 0x400_0000 | 0x800_0000,
                life: 50,
                pos: [x + dx, 0, z + dz],
                at: [x + dx, z + dz],
                at_w: 500,
                neck_y: -1500,
                ..Default::default()
            }];
            setup.player.angle = 0;
            setup.work.hit = None;
            let mut hits = Vec::new();
            let mut looks = Vec::new();
            for tick in 1..=12u8 {
                // The step this tick's `Weapon_at_ck` takes, after the
                // first tick's `init_Weapon_at` (`Routine_3` 0's clip
                // moves it on to 1, which starts the sequence).
                let sequence = setup.weapon.burst_sequence();
                let attw = if setup.work.routine[3] <= 1 {
                    (0, sequence[0].1)
                } else {
                    setup.work.attw
                };
                if attw_step(sequence, attw).1 {
                    looks.push(tick);
                }
                setup.tick(aiming(true, false), 200);
                assert_eq!(setup.work.routine[2..], [2, 2], "tick {tick}");
                if let Some(hit) = setup.work.hit.take() {
                    assert_eq!((hit.slot, hit.weapon), (3, id));
                    hits.push(tick);
                }
            }
            if id == SMG {
                assert_eq!(hits, [1, 4, 7, 10]);
                assert_eq!(looks, hits);
            } else {
                assert!(looks.is_empty() && hits.is_empty(), "{looks:?} {hits:?}");
            }
        }
    }

    /// Letting go of the trigger plays the burst's end; a press there fires
    /// again from `pl_mv05_w2_t2`'s second step, and a tilt mid-burst keeps
    /// the frame, the flamethrower with a sound.
    #[test]
    fn a_burst_ends_on_its_own_clip_and_a_tilt_keeps_the_frame() {
        let mut setup = bursting(FLAMETHROWER, 50);
        for _ in 0..4 {
            setup.tick(aiming(true, false), 200);
        }
        let frame = setup.work.move_cnt;
        setup.player.take_sounds();
        setup.tick(
            Input {
                up: true,
                ..aiming(true, false)
            },
            200,
        );
        assert_eq!(setup.work.routine[3], 1);
        assert_eq!(setup.work.status_flg & 0xE000, TILT_UP);
        assert!(
            setup
                .player
                .take_sounds()
                .iter()
                .any(|sound| sound.id == SOUND_FLAME_TILT)
        );
        setup.tick(
            Input {
                up: true,
                ..aiming(true, false)
            },
            200,
        );
        assert_eq!(setup.work.move_no, BURST_CLIPS[2]);
        assert_eq!(setup.work.move_cnt, frame + 2, "the frame goes on");
        assert_eq!(setup.work.routine[3], 2);
        // Let go of the trigger: the end, up.
        setup.tick(
            Input {
                up: true,
                ..aiming(false, false)
            },
            200,
        );
        assert_eq!(setup.work.routine[3], 3);
        setup.tick(
            Input {
                up: true,
                ..aiming(false, false)
            },
            200,
        );
        assert_eq!(setup.work.routine[3], 4);
        assert_eq!(setup.work.move_no, BURST_END_CLIPS[2]);
        // A press fires again.
        setup.tick(
            Input {
                up: true,
                ..aiming(true, true)
            },
            200,
        );
        assert_eq!(setup.work.routine[2..], [2, 1]);
        // Letting go of the aim lowers the gun.
        setup.tick(Input::default(), 200);
        assert_eq!(setup.work.routine[2], 3);
    }

    /// `Tama_decrease_i` that finds one round left takes it and stops the
    /// burst; the reload is the bursts' own clip.
    #[test]
    fn a_burst_stops_on_its_last_round_and_reloads_on_its_own_clip() {
        let mut setup = bursting(SMG, 1);
        setup.work.tama_count = 7;
        setup.tick(aiming(true, false), 200);
        assert_eq!(setup.work.spent, 1);
        assert_eq!(setup.work.routine[2..], [2, 3]);
        assert!(setup.work.effects.is_empty(), "no shot");
        setup.weapon.loaded = false;
        setup.weapon.rounds = 0;
        setup.tick(aiming(false, false), 200);
        assert_eq!(setup.work.move_no, BURST_END_CLIPS[1]);
        setup.tick(aiming(true, true), 200);
        assert_eq!(setup.work.routine[2], 4);
        setup.tick(aiming(true, false), 200);
        assert_eq!(setup.work.move_no, BURST_RELOAD_CLIP);
    }

    #[test]
    fn the_crouching_pick_up_crouches_then_opens_the_window() {
        let mut setup = Setup::new(0);
        setup.work.routine = [MODE_MOVE, MOVE_ITEM, 0, 0];
        // `pl_br06` reads no keys: a turn held through the crouch does nothing.
        let turn = Input {
            right: true,
            ..Input::default()
        };
        let angle = setup.player.angle;
        setup.tick(turn, 200);
        assert_eq!(
            (setup.work.bank, setup.work.move_no, setup.work.move_cnt),
            (Bank::Pld, ITEM_CLIP, 1)
        );
        assert_eq!(setup.work.routine[2], 1);
        // The clip's eight frames, then sub 2 opens the window on the next
        // tick, as room 103's 25 do on the 26th.
        let mut ticks = 1;
        while !setup.work.item_window {
            setup.tick(turn, 200);
            ticks += 1;
            assert!(ticks < 20, "the window never opened");
        }
        assert_eq!(ticks, 9);
        assert_eq!(setup.work.routine, [MODE_MOVE, MOVE_ITEM, 3, 0]);
        assert_eq!(setup.player.angle, angle);
        assert!(crouching(&setup.work));
        // Sub 3 holds while the window is up.
        setup.work.item_window = false;
        setup.tick(Input::default(), 200);
        assert_eq!(setup.work.routine, [MODE_MOVE, MOVE_ITEM, 3, 0]);
        assert!(!setup.work.item_window);
        // Sub 4 stands back up: the crouch backwards, then routine 1, 0.
        setup.work.routine[2] = 4;
        setup.tick(Input::default(), 200);
        assert_eq!(setup.work.routine[2], 5);
        assert!(setup.player.anim.reverse);
        let mut ticks = 1;
        while crouching(&setup.work) {
            setup.tick(Input::default(), 200);
            ticks += 1;
            assert!(ticks < 20, "the stand-up never ended");
        }
        assert_eq!(setup.work.routine, [MODE_MOVE, MOVE_IDLE, 0, 0]);
    }

    fn dropped(y: i32, bounces: u8) -> Work {
        Work {
            magazine: Some(Magazine {
                matrix: Matrix {
                    m: rot_matrix([0; 3]),
                    t: [100, y, 200],
                },
                speed: [3, 30],
                ground: 0,
                fall: 0,
                bounces,
            }),
            ..Work::default()
        }
    }

    #[test]
    fn a_magazine_falls_straight_down_until_it_bounces() {
        let mut work = dropped(-1000, 1);
        let mut rnd = crate::zombie2::Rnd::default();
        let start = rnd;
        assert_eq!(work.mag_down(&mut rnd), None);
        let magazine = work.magazine.unwrap();
        assert_eq!(magazine.fall, 0x14);
        assert_eq!(magazine.matrix.t, [100, -980, 200]);
        assert_eq!(rnd, start, "only a bounce draws");
    }

    #[test]
    fn a_magazines_bounce_draws_sounds_and_sends_it_spinning() {
        let mut work = dropped(-10, 1);
        let mut rnd = crate::zombie2::Rnd::default();
        let start = rnd;
        assert_eq!(work.mag_down(&mut rnd), Some([100, 0, 200]));
        assert_ne!(rnd, start);
        let magazine = work.magazine.unwrap();
        assert_eq!(magazine.fall, -10);
        assert_eq!(magazine.bounces, 0);
        // Either way along the floor, and turned about its own x.
        assert!(matches!(magazine.matrix.t, [103, 0, 230] | [97, 0, 170]));
        assert_eq!(magazine.matrix.m, rot_matrix([0x100, 0, 0]));
    }

    #[test]
    fn a_magazine_goes_at_its_second_landing_with_no_draw() {
        let mut work = dropped(-10, 0);
        let mut rnd = crate::zombie2::Rnd::default();
        let start = rnd;
        assert_eq!(work.mag_down(&mut rnd), None);
        assert_eq!(work.magazine, None);
        assert_eq!(rnd, start);
    }

    #[test]
    fn a_new_room_takes_the_magazine_and_arms_the_next_by_the_gun() {
        let mut work = dropped(-1000, 1);
        work.enter_room(true);
        assert_eq!(work.magazine, None);
        assert_eq!(work.status_flg, STATUS_MAGAZINE);
        work.enter_room(false);
        assert_eq!(work.status_flg, 0);
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
    fn an_enemys_hold_holds_the_game_until_a_tick_starts_free() {
        let mut setup = Setup::new(0);
        setup.tick(Input::default(), 190);
        assert!(!setup.work.holds_game);
        setup.work.routine = [MODE_HELD, 0, 0, 0];
        setup.tick(Input::default(), 190);
        assert!(setup.work.holds_game);
        // The next tick that starts free of the hold drops it.
        setup.work.routine = [MODE_MOVE, MOVE_IDLE, 0, 0];
        setup.tick(Input::default(), 190);
        assert!(!setup.work.holds_game);
        // A hit holds it as the hold does.
        setup.work.routine = [MODE_DAMAGE, 0, 0, 0];
        setup.tick(Input::default(), 190);
        assert!(setup.work.holds_game);
    }

    #[test]
    fn the_stairs_hold_the_game_only_while_they_run() {
        let mut setup = Setup::new(0);
        setup.work.routine = [MODE_MOVE, MOVE_STAIRS, 1, 0];
        assert!(setup.work.holds_game_in_routine() && !setup.work.holds_game_now());
        // Their last tick drops it itself, so a tick that ends idle is free
        // whatever it started in.
        setup.work.routine = [MODE_MOVE, MOVE_IDLE, 0, 0];
        assert!(!setup.work.holds_game_in_routine() && !setup.work.holds_game_now());
    }

    #[test]
    fn each_hit_plays_its_own_cry() {
        for (kind, cry) in [
            (0, 0x400_0001),
            (1, 0x401_0001),
            (2, 0x401_0001),
            (3, 0x402_0001),
            (6, 0x403_0001),
        ] {
            let mut setup = Setup::new(0);
            setup.work.routine = [MODE_DAMAGE, kind, 0, 0];
            setup.tick(Input::default(), 190);
            assert_eq!(setup.work.sounds, [cry], "kind {kind}");
            assert_eq!(setup.work.be_flg & 4, 0);
        }
    }

    #[test]
    fn dying_cries_falls_and_lies_still() {
        let mut setup = Setup::new(0);
        setup.work.routine = [MODE_DEAD, 0, 0, 0];
        setup.tick(Input::default(), 0);
        assert_eq!(setup.work.sounds, [0x403_0001]);
        assert_eq!(setup.work.move_no, FALL_CLIP);
        assert_eq!(setup.work.neck.flags & 0x12, 0);
        for _ in 0..200 {
            setup.tick(Input::default(), 0);
        }
        assert_eq!(setup.work.routine[..2], [MODE_DEAD_BODY, 2]);
        assert_eq!(setup.work.sounds.len(), 1);
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
    fn the_action_key_raises_the_shot_bit_too() {
        // `Pad_set`'s row 0 gives raw `0x80` the shot, the action and the
        // confirm, so `Gacha_ch`'s mask sees an action press.
        // The fire key is the same button, so it raises the action bit that
        // `pl_br_*` checks with.
        for input in [
            Input {
                action_pressed: true,
                ..Input::default()
            },
            Input {
                fire: true,
                ..Input::default()
            },
        ] {
            let key = key_word(&input);
            assert_eq!(
                key & (KEY_FIRE | KEY_ACTION | KEY_CONFIRM),
                KEY_FIRE | KEY_ACTION | KEY_CONFIRM
            );
            assert_ne!(key & 0x34F, 0);
        }
    }

    /// Only `pl_br_00` to `pl_br_04`, and `pl_br_09` past its fourth step,
    /// raise `fg_status`'s `0x200000`: aiming or hurt, a press checks
    /// nothing. (A held player runs [`player_held`], with no `pl_br_*`.)
    #[test]
    fn only_the_pads_branches_take_the_check_key() {
        let fire = Input {
            fire: true,
            ..Input::default()
        };
        for (routine, checks) in [
            ([MODE_MOVE, MOVE_IDLE, 1, 0], true),
            ([MODE_MOVE, MOVE_ROTATE, 1, 0], true),
            ([MODE_MOVE, MOVE_AIM, 1, 0], false),
            ([MODE_MOVE, MOVE_JUMP_DOWN, 0, 4], false),
            ([MODE_MOVE, MOVE_JUMP_DOWN, 0, 5], true),
            ([MODE_DAMAGE, 0, 0, 0], false),
        ] {
            let mut setup = Setup::new(0);
            setup.work.routine = routine;
            setup.tick(fire, 200);
            assert_eq!(setup.work.check, checks, "{routine:?}");
        }
    }

    #[test]
    fn a_walk_turns_by_pl_br_01s_rate_for_its_health() {
        for (life, rate) in [(200, 0x28), (100, 0x20), (20, 0x10)] {
            let mut setup = Setup::new(50);
            setup.work.routine[2] = 0;
            setup.player.angle = 0;
            setup.tick(input(true, false, false), life);
            assert_eq!(setup.work.routine[1], MOVE_FORWARD);
            setup.tick(input(true, true, false), life);
            assert_eq!(setup.player.angle, rate, "life {life}");
        }
    }

    #[test]
    fn raising_the_gun_looks_again_on_a_tilt_down_not_up() {
        for (down, again) in [(false, false), (true, true)] {
            let mut setup = armed(true, false);
            setup.tick(aiming(false, false), 200);
            assert_eq!(setup.work.routine, [MODE_MOVE, MOVE_AIM, 0, 1]);
            let tilt = Input {
                up: !down,
                down,
                ..aiming(false, false)
            };
            setup.tick(tilt, 200);
            assert_eq!(setup.work.aimed_again, again, "down {down}");
        }
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
            morphs: &[],
            banks: Banks {
                pld: bank,
                weapon: bank,
                room: bank,
                held: bank,
            },
            input: input(true, false, false),
            options: Options::default(),
            id: 0,
            life: 200,
            poisoned: false,
            stopped: true,
            weapon: Weapon::default(),
            necks: [None; NECK_WORKS],
            targets: &[],
            neck_targets: &[],
            enemies: 0,
            pushing: false,
            objects: &[],
            key_trg: KEY_FORWARD | KEY_TILT_UP,
        };
        player_move(&mut setup.work, &mut setup.player, &tick);
        // `Key_trg` comes from the pad, which goes on being read.
        assert_eq!(setup.work.key_trg, KEY_FORWARD | KEY_TILT_UP);
        setup.work.key_trg = before.key_trg;
        // `Joint_trans2` draws the held pose again, where the body is.
        assert_eq!(setup.work.parts_at, Some(setup.player.pos));
        setup.work.parts_at = before.parts_at;
        assert_eq!(setup.work, before);
    }

    #[test]
    fn the_head_follows_the_bodys_push_and_a_slopes_height() {
        let mut work = Work::default();
        work.neck.position = Some([100, -1700, 200]);
        work.neck.body = Some([0, 0, 0]);
        // A push out in x and z, and a slope's `Sca_hit_slope` raising the
        // body 40: the head is on the moved body.
        follow_pushes(&mut work, [30, -40, -50]);
        assert_eq!(work.neck.position, Some([130, -1740, 150]));
        assert_eq!(work.neck.body, Some([30, -40, -50]));
        // A second push moves it by what the body moved since the first.
        follow_pushes(&mut work, [35, -40, -50]);
        assert_eq!(work.neck.position, Some([135, -1740, 150]));
    }

    /// `Joint_trans2` draws the parts after the pushes, so the `Workm` an
    /// `Esp_call` copies on the next tick is on the pushed body. In
    /// `spider_shots_302` a wall's push of 15 in x on the shot's tick put the
    /// original's flash at x -15532 where the unpushed hand gave -15517.
    #[test]
    fn the_parts_follow_the_bodys_push() {
        let mut work = Work::default();
        work.neck.body = Some([-14908, 0, -12022]);
        work.parts_at = Some([-14908, 0, -12022]);
        let hand = Matrix {
            m: [[285, -2206, -3440], [-4060, -546, 15], [-470, 3406, -2227]],
            t: [-15188, -2490, -11302],
        };
        work.parts = vec![None, Some(hand)];
        follow_pushes(&mut work, [-14923, 0, -12022]);
        assert_eq!(
            work.parts[1].map(|part| part.t),
            Some([-15203, -2490, -11302])
        );
        assert_eq!(work.parts[1].map(|part| part.m), Some(hand.m));
    }

    /// `Em10Dm00` turns the first part by the lean on its own side
    /// (`MulMatrix(M, Rz)`) and the second by the opposite on its parent's
    /// (`MulMatrix2(Rz', M)`), which for a part turned about more than z isn't
    /// the lean added to its angle.
    #[test]
    fn the_lean_turns_the_first_two_parts_on_either_side() {
        let first = rot_matrix([0x100, 0x200, 0]);
        let second = rot_matrix([0x80, 0, 0x40]);
        let mut local = vec![
            Some(Matrix {
                m: first,
                t: [0; 3],
            }),
            Some(Matrix {
                m: second,
                t: [0, -400, 0],
            }),
            Some(Matrix {
                m: second,
                t: [0; 3],
            }),
        ];
        lean(&mut local, -0x40, 0);
        assert_eq!(
            local[0].unwrap().m,
            mul_matrix(&first, &rot_matrix([0, 0, -0x40]))
        );
        assert_eq!(
            local[1].unwrap().m,
            mul_matrix(&rot_matrix([0, 0, 0x40]), &second)
        );
        assert_eq!(local[1].unwrap().t, [0, -400, 0]);
        assert_eq!(local[2].unwrap().m, second, "only the first two parts");
        assert_ne!(local[1].unwrap().m, rot_matrix([0x80, 0, 0]));
        assert_ne!(local[0].unwrap().m, rot_matrix([0x100, 0x200, -0x40]));
    }

    /// `FUN_00457e60`'s first tick leans by `RotMatrix(0, 300, lean)`, the
    /// blood's vector left in `local_48`: the first part on its own side, and
    /// the second by its transpose (`TransposeMatrix`, `MulMatrix2`), which
    /// isn't `RotMatrix` of the opposite angles once it turns about y too.
    #[test]
    fn the_rising_lean_turns_about_y_and_takes_the_transpose_back() {
        let first = rot_matrix([0x100, 0x200, 0]);
        let second = rot_matrix([0x80, 0, 0x40]);
        let part = |m| Some(Matrix { m, t: [0; 3] });
        let mut local = vec![part(first), part(second)];
        lean(&mut local, -0x40, 300);
        let turn = rot_matrix([0, 300, -0x40]);
        let back = std::array::from_fn(|row| std::array::from_fn(|column| turn[column][row]));
        assert_eq!(local[0].unwrap().m, mul_matrix(&first, &turn));
        assert_eq!(local[1].unwrap().m, mul_matrix(&back, &second));
        assert_ne!(
            local[1].unwrap().m,
            mul_matrix(&rot_matrix([0, -300, 0x40]), &second)
        );
        let mut still = vec![part(first), part(second)];
        lean(&mut still, 0, 300);
        assert_ne!(still[0].unwrap().m, first, "a yaw alone still leans");
    }

    /// `Em10Dm00`'s lean in `street_shoot`: zombie slot 1's part 0 on tick
    /// 45, the root and posed `M` under a lean of -0x240, as arklay's draw
    /// has them. With the lean as `MulMatrix(M, Rz(l))` the blood the hit
    /// started on part 0 at (-140, -483, 0) (slot 62, three ticks old, its
    /// offset (150, -180, 42) turned by 972) is where the original's is on
    /// that tick, (18386, -2519, 2637); without it, it isn't.
    #[test]
    fn a_leaning_zombies_blood_is_where_the_originals_is() {
        let tree = |lean_by: i16| {
            let mut local = vec![Some(Matrix {
                m: [[3852, -1352, 326], [1375, 3844, -319], [-202, 408, 4070]],
                t: [0, -1998, 0],
            })];
            lean(&mut local, lean_by, 0);
            PoseTree {
                root: Matrix {
                    m: [[333, 0, 4082], [0, 4096, 0], [-4082, 0, 333]],
                    t: [18360, 0, 2416],
                },
                local,
                children: vec![vec![]],
            }
        };
        let blood = |part: Matrix| {
            let at = crate::esp2::apply_matrix(&part, [-140, -483, 0]);
            let (sin, cos) = (rsin(972), rcos(972));
            let (x, y, z) = (150, -180, 42);
            [
                at[0].wrapping_add(((cos * x + sin * z) >> 12) as i16),
                at[1].wrapping_add(y as i16),
                at[2].wrapping_add(((cos * z - sin * x) >> 12) as i16),
            ]
        };
        let part = tree(-0x240).compose(&[])[0].unwrap();
        assert_eq!(
            part.m,
            [[-159, 273, 4082], [-2100, 3500, -319], [-3513, -2104, 6]]
        );
        assert_eq!(blood(part), [18386, -2519, 2637]);
        let upright = tree(0).compose(&[])[0].unwrap();
        assert_ne!(blood(upright), [18386, -2519, 2637]);
    }

    /// A `J_seq2` body is drawn from its eased matrices, not from the
    /// pose's angles: its first parts' drawn `M` are those, y flipped, with
    /// the lean after; a part past them keeps the pose's.
    #[test]
    fn a_jseq2_body_is_drawn_from_its_eased_matrices() {
        let eased = [[3886, 0, 1293], [0, 4096, 0], [-1023, 0, 3073]];
        let posed = crate::anim::rotation_matrix(0x100, 0x200, -0x80);
        let mut local = vec![
            crate::anim::Mat4x3 {
                r: posed,
                t: [0; 3]
            };
            2
        ];
        let mut work = Work {
            jseq2: Some(vec![eased]),
            ..Work::default()
        };
        work.pose_locals(&mut local);
        assert_eq!(local[0].r, flip_y(eased.map(|row| row.map(i32::from))));
        assert_eq!(local[1].r, posed);
        work.lean = -0x40;
        let mut leaning = vec![
            crate::anim::Mat4x3 {
                r: posed,
                t: [0; 3]
            };
            2
        ];
        work.pose_locals(&mut leaning);
        let turned = mul_matrix(&eased, &rot_matrix([0, 0, -0x40]));
        assert_eq!(leaning[0].r, flip_y(turned.map(|row| row.map(i32::from))));
    }

    /// `Em_24`'s init poses through `J_seq`, so its `J_seq2` starts from
    /// each part's raw `RotMatrix`, not one `NormalMatrix_2` made unit.
    #[test]
    fn em24s_jseq2_starts_from_the_raw_rot_matrix() {
        let rotation = [0x100, 0x200, 0x80];
        let raw = rot_matrix(rotation);
        assert_ne!(raw, normal_matrix(&raw, 2), "the two differ here");
        let mut setup = Setup::new(0);
        setup.keyframes = vec![Keyframe {
            offset: [0; 3],
            vertex: [0; 3],
            rotations: vec![rotation],
        }];
        setup.clips = vec![Clip {
            frames: vec![ClipFrame {
                keyframe: 0,
                timing: 0,
                flags: 0,
            }],
        }];
        let bank = (setup.clips.as_slice(), setup.keyframes.as_slice());
        let tick = Tick {
            room: &setup.room,
            skeleton: &setup.skeleton,
            morphs: &[],
            banks: Banks {
                pld: bank,
                weapon: bank,
                room: bank,
                held: bank,
            },
            input: Input::default(),
            options: Options::default(),
            id: 0x24,
            life: 200,
            poisoned: false,
            stopped: false,
            weapon: setup.weapon,
            necks: [None; NECK_WORKS],
            targets: &[],
            neck_targets: &[],
            enemies: 0,
            pushing: false,
            objects: &[],
            key_trg: 0,
        };
        let mut mover = Mover::new(&mut setup.work, &mut setup.player, &tick);
        mover.work.bank = Bank::Pld;
        mover.work.move_no = 0;
        mover.work.move_cnt = 0;
        mover.work.hokan = 0;
        mover.joint_move_late_reversed(0x1000, false);
        assert_eq!(mover.work.jseq2, None, "the init's step is `J_seq`'s");
        mover.start_jseq2();
        assert_eq!(mover.work.jseq2, Some(vec![raw]));
    }

    /// The draw's joint matrices are the game's seen with Y flipped, so the
    /// lean the draw composes is the `Workm`'s: the drawn part 0 and 1 under a
    /// lean are the game's flipped, as without one.
    #[test]
    fn the_drawn_lean_is_the_workms_with_y_flipped() {
        let rotations = [[0x100, 0x200, -0x80], [0x80, -0x300, 0x40]];
        for lean_by in [0, -0x240, 0x180] {
            let mut local: Vec<crate::anim::Mat4x3> = rotations
                .iter()
                .map(|[x, y, z]| crate::anim::Mat4x3 {
                    r: crate::anim::rotation_matrix(i32::from(*x), i32::from(*y), i32::from(*z)),
                    t: [0; 3],
                })
                .collect();
            let work = Work {
                lean: lean_by,
                ..Work::default()
            };
            work.lean_locals(&mut local);
            let mut game: Vec<Option<Matrix>> = rotations
                .iter()
                .map(|rotation| {
                    Some(Matrix {
                        m: rot_matrix(*rotation),
                        t: [0; 3],
                    })
                })
                .collect();
            lean(&mut game, lean_by, 0);
            for (drawn, game) in local.iter().zip(game) {
                let game = flip_y(game.unwrap().m.map(|row| row.map(i32::from)));
                for (a, b) in drawn.r.iter().flatten().zip(game.iter().flatten()) {
                    assert!(
                        (a - b).abs() <= 4,
                        "lean {lean_by}: {:?} against {game:?}",
                        drawn.r
                    );
                }
            }
        }
    }

    /// A part `Joint_trans2` doesn't draw (bit 0 of its `Be_flg` clear)
    /// keeps the `Workm` the last draw left it, and the parts under it are
    /// composed from that: a stomped crawler bleeds from its hidden head
    /// where the head last was.
    #[test]
    fn a_hidden_part_keeps_its_last_workm() {
        let local = |t: [i32; 3]| {
            Some(Matrix {
                m: rot_matrix([0; 3]),
                t,
            })
        };
        let tree = PoseTree {
            root: Matrix {
                m: rot_matrix([0; 3]),
                t: [1000, 0, 0],
            },
            local: vec![
                local([0, -500, 0]),
                local([0, -400, 0]),
                local([0, -100, 0]),
            ],
            children: vec![vec![1], vec![2], vec![]],
        };
        let drawn = tree.compose(&[]);
        assert_eq!(drawn[2].map(|part| part.t), Some([1000, -1000, 0]));
        let old = Matrix {
            m: rot_matrix([0; 3]),
            t: [900, -800, 50],
        };
        let previous = vec![None, Some(old), None];
        let kept = tree.compose_drawn(&[], 1 << 1, &previous);
        assert_eq!(kept[0].map(|part| part.t), Some([1000, -500, 0]));
        assert_eq!(kept[1], Some(old));
        assert_eq!(kept[2].map(|part| part.t), Some([900, -900, 50]));
    }

    /// A body with no pose yet (no clip: `local` empty) draws nothing, so its
    /// parts keep the `Workm` the last draw left them.
    #[test]
    fn a_body_without_a_pose_keeps_its_last_workm() {
        let old = Matrix {
            m: rot_matrix([0; 3]),
            t: [900, -800, 50],
        };
        let previous = vec![Some(old), None, Some(old)];
        let tree = PoseTree::default();
        assert_eq!(tree.compose_drawn(&[], 0, &previous), previous);
        assert_eq!(tree.compose_drawn(&[], u32::MAX, &previous), previous);
    }

    /// `FUN_0045a880`'s `awStack_c`: the blood of a hold from the front is at
    /// (0, 200, 100) on part 8, and from behind at (-100, 100, -200).
    #[test]
    fn a_standing_holds_blood_is_on_its_sides_point() {
        for (back, point) in [(0, [0, 200, 100]), (1, [-100, 100, -200])] {
            let mut setup = Setup::new(0);
            (setup.clips, setup.keyframes) = bank(0x11, 24, 0);
            for keyframe in &mut setup.keyframes {
                keyframe.rotations = vec![[0; 3]; 9];
            }
            setup.skeleton = Skeleton {
                relative: vec![[0, -100, 0]; 9],
                children: (0..9u8)
                    .map(|part| (part < 8).then_some(part + 1).into_iter().collect())
                    .collect(),
            };
            setup.work.routine = [MODE_HELD, back, 0, 0];
            let mut life = 190;
            run_held(&mut setup, &mut life, |work| {
                work.effects.iter().any(|call| call.size == 0x17D0)
            });
            let call = setup
                .work
                .effects
                .iter()
                .find(|call| call.size == 0x17D0)
                .unwrap();
            let (part, _, on) = call.on.expect("on part 8");
            assert_eq!((part, on), (8, point), "back {back}");
        }
    }

    #[test]
    fn the_head_stays_without_a_placed_body() {
        let mut work = Work::default();
        work.neck.position = Some([100, -1700, 200]);
        follow_pushes(&mut work, [30, 0, -50]);
        assert_eq!(work.neck.position, Some([100, -1700, 200]));
        assert_eq!(work.neck.body, None);
    }

    #[test]
    fn a_stopped_tick_forgets_the_body() {
        let mut setup = Setup::new(50);
        setup.work.neck.position = Some([100, -1700, 200]);
        setup.work.neck.body = Some(setup.player.pos);
        let bank = (setup.clips.as_slice(), setup.keyframes.as_slice());
        let tick = Tick {
            room: &setup.room,
            skeleton: &setup.skeleton,
            morphs: &[],
            banks: Banks {
                pld: bank,
                weapon: bank,
                room: bank,
                held: bank,
            },
            input: Input::default(),
            options: Options::default(),
            id: 0,
            life: 200,
            poisoned: false,
            stopped: true,
            weapon: Weapon::default(),
            necks: [None; NECK_WORKS],
            targets: &[],
            neck_targets: &[],
            enemies: 0,
            pushing: false,
            objects: &[],
            key_trg: 0,
        };
        player_move(&mut setup.work, &mut setup.player, &tick);
        assert_eq!(setup.work.neck.body, None);
        follow_pushes(&mut setup.work, [5000, 0, 5000]);
        assert_eq!(setup.work.neck.position, Some([100, -1700, 200]));
    }

    /// The one frame of each clip marked a footfall in the noise tests.
    const FOOTFALL_FRAME: usize = 3;

    /// Mark frame [`FOOTFALL_FRAME`] of every clip a footfall, and no other.
    fn one_footfall(setup: &mut Setup) {
        for clip in &mut setup.clips {
            for (index, frame) in clip.frames.iter_mut().enumerate() {
                frame.flags = if index == FOOTFALL_FRAME {
                    FRAME_FOOTSTEP
                } else {
                    0
                };
            }
        }
    }

    /// Tick once: whether the frame the routine tests is the footfall (the
    /// one `Joint_move` leaves when `after`, else the one it starts from),
    /// and the noise the tick leaves.
    fn footfall_tick(setup: &mut Setup, after: bool) -> (bool, u8) {
        let before = setup.work.now_seq & FRAME_FOOTSTEP != 0;
        setup.tick(Input::default(), 200);
        let after_move = setup.work.now_seq & FRAME_FOOTSTEP != 0;
        (if after { after_move } else { before }, setup.work.noise)
    }

    /// [`footfall_tick`] while `running` holds, for up to 200 ticks.
    fn footfall_ticks(
        setup: &mut Setup,
        after: bool,
        running: impl Fn(&Work) -> bool,
    ) -> Vec<(bool, u8)> {
        let mut seen = Vec::new();
        for _ in 0..200 {
            if !running(&setup.work) {
                break;
            }
            seen.push(footfall_tick(setup, after));
        }
        seen
    }

    /// `noise` is made on the ticks of `seen` whose frame is the footfall,
    /// and only on them; `seen` has ticks of both kinds.
    fn assert_noise_on_footfalls(seen: &[(bool, u8)], noise: u8, what: &str) {
        assert!(
            seen.iter().any(|&(footfall, _)| footfall),
            "{what}: no footfall: {seen:?}"
        );
        assert!(
            seen.iter().any(|&(footfall, _)| !footfall),
            "{what}: only footfalls: {seen:?}"
        );
        for (tick, &(footfall, made)) in seen.iter().enumerate() {
            assert_eq!(
                made & noise != 0,
                footfall,
                "{what}: tick {tick} of {seen:?}"
            );
        }
    }

    /// `pl_mv_07` tests the frame after `Joint_move`.
    #[test]
    fn a_stairs_footfall_is_a_runs_noise() {
        let mut setup = at_stairs([1500, 0, 700], 0xC00);
        one_footfall(&mut setup);
        setup.tick(action(), 200);
        assert_eq!(setup.work.routine[1], MOVE_STAIRS);
        let seen = footfall_ticks(&mut setup, true, |work| work.routine[1] == MOVE_STAIRS);
        assert_noise_on_footfalls(&seen, NOISE_RUN, "the stairs");
    }

    /// `pl_mv_08` tests the frame after `Joint_move`.
    #[test]
    fn a_climbs_footfall_is_a_runs_noise() {
        let mut setup = Setup::new(0);
        (setup.clips, setup.keyframes) = bank(10, 60, 0);
        one_footfall(&mut setup);
        setup.work.routine = [MODE_MOVE, MOVE_CLIMB, 0, 2];
        let seen = footfall_ticks(&mut setup, true, |work| work.routine[1] == MOVE_CLIMB);
        assert_noise_on_footfalls(&seen, NOISE_RUN, "the climb");
    }

    /// `Spl_sce_mv01`'s turn and `Spl_sce_mv02`'s walk make a step's noise,
    /// and `Spl_sce_mv02`'s run a run's, testing the frame before
    /// `Joint_move`.
    #[test]
    fn a_scripted_walks_footfalls_are_steps_and_a_runs_are_runs() {
        for (routine, noise) in [(4, NOISE_STEP), (5, NOISE_RUN)] {
            let mut setup = Setup::new(50);
            (setup.clips, setup.keyframes) = bank(10, 40, 10);
            one_footfall(&mut setup);
            setup.player.pos = [0, 0, 0];
            setup.player.angle = 0x800;
            plc_dest(&mut setup.work, 0, routine, 7, 4000, 0);
            let mut turn = Vec::new();
            let mut walk = Vec::new();
            for _ in 0..200 {
                if setup.work.routine[0] != MODE_SCENARIO || setup.work.routine[1] != routine {
                    break;
                }
                let phase = setup.work.routine[2];
                let seen = footfall_tick(&mut setup, false);
                if phase == 2 { &mut walk } else { &mut turn }.push(seen);
            }
            assert_noise_on_footfalls(&turn, NOISE_STEP, "the turn");
            assert!(
                walk.iter()
                    .all(|(_, made)| made & (NOISE_STEP | NOISE_RUN) & !noise == 0),
                "the walk makes only its own noise: {walk:?}"
            );
            assert_noise_on_footfalls(&walk, noise, "the walk");
        }
    }

    /// `Spl_sce_mv04`'s clip and `Spl_sce15`'s turn (`Spl_sce_mv03`) make a
    /// step's noise, testing the frame before `Joint_move`.
    #[test]
    fn a_scripted_clip_and_turn_make_a_steps_noise() {
        for routine in [0x0A, 0x15] {
            let mut setup = Setup::new(50);
            (setup.clips, setup.keyframes) = bank(10, 40, 10);
            one_footfall(&mut setup);
            setup.player.angle = 0;
            setup.work.routine = [MODE_SCENARIO, routine, 0, 0];
            setup.work.dest = [0x800, 64];
            let seen = footfall_ticks(&mut setup, false, |work| work.routine[1] == routine);
            assert_noise_on_footfalls(&seen, NOISE_STEP, &format!("{routine:#x}"));
        }
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

    /// Clips of 8, 16 and 4 frames, each frame's flags its clip and index.
    fn uneven_clips() -> Vec<Clip> {
        [8u32, 16, 4]
            .iter()
            .enumerate()
            .map(|(clip, &len)| Clip {
                frames: (0..len)
                    .map(|index| ClipFrame {
                        keyframe: index as u16,
                        timing: 0,
                        flags: (clip as u32) << 20 | index << 8,
                    })
                    .collect(),
            })
            .collect()
    }

    #[test]
    fn a_frame_past_a_clips_end_is_in_the_clips_after_it() {
        let clips = uneven_clips();
        assert_eq!(frame_past_end(&clips, 0, 7), None);
        assert_eq!(frame_past_end(&clips, 0, 8), Some((1, 0)));
        assert_eq!(frame_past_end(&clips, 0, 9), Some((1, 1)));
        assert_eq!(frame_past_end(&clips, 0, 24), Some((2, 0)));
        assert_eq!(frame_past_end(&clips, 0, 28), None, "past the last clip");
        assert_eq!(frame_past_end(&clips, 2, 4), None);
    }

    /// The alligator's kill does a `Null_pos_set` while `Mot_play_init` has
    /// its clip at `0xFF`: the original reads a frame's word as the clip's
    /// count and offset, and the frame it lands on skips a held frame.
    #[test]
    fn a_clip_past_the_table_reads_the_frames_as_its_header() {
        let frame = |keyframe, timing, flags| ClipFrame {
            keyframe,
            timing,
            flags,
        };
        let clips = vec![
            Clip {
                frames: vec![frame(10, 0, 0), frame(11, 0x10, 0), frame(12, 0, 0)],
            },
            Clip {
                frames: vec![
                    frame(20, 0, FRAME_SKIPPED),
                    frame(42, 0, 0),
                    frame(22, 0, 0),
                ],
            },
        ];
        // Word 3 is clip 0's frame 1: offset 0x10 is word 4, and one frame on
        // is clip 1's held frame 0, so its frame 1.
        assert_eq!(stray_keyframe(&clips, 3, 1), Some(42));
        assert_eq!(stray_keyframe(&clips, 3, 0), Some(12));
        assert_eq!(stray_keyframe(&clips, 0xFF, 0), None, "past the bytes");
    }

    /// `Em_25`'s death on a wall starts its 8-frame clip 1 at `Move_cnt` 9:
    /// `Joint_move` reads clip 2's frame 1, and `J_seq` ends the clip.
    #[test]
    fn joint_move_past_a_clips_end_reads_the_next_clip_and_ends() {
        let mut setup = Setup::new(0);
        setup.clips = uneven_clips();
        let bank = (setup.clips.as_slice(), setup.keyframes.as_slice());
        let tick = Tick {
            room: &setup.room,
            skeleton: &setup.skeleton,
            morphs: &[],
            banks: Banks {
                pld: bank,
                weapon: bank,
                room: bank,
                held: bank,
            },
            input: Input::default(),
            options: Options::default(),
            id: 0,
            life: 200,
            poisoned: false,
            stopped: false,
            weapon: setup.weapon,
            necks: [None; NECK_WORKS],
            targets: &[],
            neck_targets: &[],
            enemies: 0,
            pushing: false,
            objects: &[],
            key_trg: 0,
        };
        let mut mover = Mover::new(&mut setup.work, &mut setup.player, &tick);
        mover.work.bank = Bank::Pld;
        mover.work.move_no = 0;
        mover.work.move_cnt = 9;
        assert!(mover.joint_move_late_reversed(0x400, false));
        assert_eq!(mover.work.move_cnt, 0);
        assert_eq!(mover.work.now_seq, 1 << 20 | 1 << 8);
        assert_eq!(mover.player.anim.clip, 0);
        assert!(!mover.joint_move_late_reversed(0x400, false));
        assert_eq!(mover.work.move_cnt, 1);
        assert_eq!(mover.work.now_seq, 0, "then the clip from its start");
    }

    /// `J_seq2` eases the matrices: from the identity toward a quarter turn
    /// about y with `late * Hokan` 0xC00 kept, `HMatrix` gives row 0
    /// `(3073, 0, 1023)`, which `NormalMatrix_0` makes `(3886, 0, 1293)`, and
    /// row 2 stays `(-1023, 0, 3073)`. Eased angles would give a 0x100 turn,
    /// `(3784, 0, 1567)`. With no blend left `NormalMatrix_2` keeps row 1.
    #[test]
    fn jseq2_eases_the_matrix_and_makes_rows_0_and_1_unit() {
        let identity = rot_matrix([0; 3]);
        let quarter = [0, 0x400, 0];
        assert_eq!(
            jseq2_matrix(identity, quarter, Some(0xC00)),
            [[3886, 0, 1293], [0, 4096, 0], [-1023, 0, 3073]]
        );
        assert_eq!(
            jseq2_matrix(identity, quarter, None),
            [[0, 0, 4096], [0, 4096, 0], [-4096, 0, 0]]
        );
        let skewed = [[3000, 0, 1000], [10, 20, 30], [0, 3000, 1000]];
        assert_eq!(
            normal_matrix(&skewed, 2),
            [[3885, 0, 1295], [10, 20, 30], [0, 3885, 1295]]
        );
    }

    /// A body that plays through `J_seq2` keeps its eased matrices from one
    /// step to the next, so its part's `M` is never `RotMatrix` of an angle:
    /// the second step of a 4-step blend eases from the first's matrix.
    #[test]
    fn a_jseq2_body_poses_its_parts_from_the_eased_matrices() {
        let mut setup = Setup::new(0);
        let still = Keyframe {
            offset: [0; 3],
            vertex: [0; 3],
            rotations: vec![[0; 3]],
        };
        let turned = Keyframe {
            rotations: vec![[0, 0x400, 0]],
            ..still.clone()
        };
        setup.keyframes = vec![still, turned.clone(), turned];
        setup.clips = vec![Clip {
            frames: (0..3)
                .map(|keyframe| ClipFrame {
                    keyframe,
                    timing: 0,
                    flags: 0,
                })
                .collect(),
        }];
        setup.work.jseq2 = Some(Vec::new());
        let bank = (setup.clips.as_slice(), setup.keyframes.as_slice());
        let tick = Tick {
            room: &setup.room,
            skeleton: &setup.skeleton,
            morphs: &[],
            banks: Banks {
                pld: bank,
                weapon: bank,
                room: bank,
                held: bank,
            },
            input: Input::default(),
            options: Options::default(),
            id: 0,
            life: 200,
            poisoned: false,
            stopped: false,
            weapon: setup.weapon,
            necks: [None; NECK_WORKS],
            targets: &[],
            neck_targets: &[],
            enemies: 0,
            pushing: false,
            objects: &[],
            key_trg: 0,
        };
        let mut mover = Mover::new(&mut setup.work, &mut setup.player, &tick);
        mover.work.bank = Bank::Pld;
        mover.work.move_no = 0;
        mover.work.move_cnt = 0;
        mover.work.hokan = 0;
        mover.joint_move_late_reversed(0x400, false);
        assert_eq!(mover.part_rotation(0, [0; 3]), rot_matrix([0; 3]));
        mover.work.hokan = 3;
        mover.joint_move_late_reversed(0x400, false);
        assert_eq!(
            mover.part_rotation(0, [0; 3]),
            [[3886, 0, 1293], [0, 4096, 0], [-1023, 0, 3073]]
        );
        mover.joint_move_late_reversed(0x400, false);
        assert_eq!(
            mover.part_rotation(0, [0; 3]),
            [[2396, 0, 3321], [0, 4096, 0], [-2559, 0, 1537]],
            "row 2 is never made unit again"
        );
        mover.work.jseq2 = None;
        assert_eq!(
            mover.part_rotation(0, [0, 0x200, 0]),
            rot_matrix([0, 0x200, 0]),
            "a `J_seq` body turns by its pose's angles"
        );
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
    fn a_scene_that_stops_the_player_before_the_first_tick_keeps_pl_init_s_blend() {
        // Room 200's entry: the scene stops the player on the room's first
        // tick, before `pl_mv_00` has stepped the blend.
        let mut work = Work::default();
        assert_eq!(work.hokan, FULL_BLEND - 1);
        plc_stop_player(&mut work);
        assert_eq!(work.routine, STOPPED_ROUTINE);
        assert_eq!(work.hokan, 0);

        // Once a tick has run the routines, the blend is that tick's.
        let mut setup = Setup::new(0);
        setup.tick(Input::default(), 200);
        let hokan = setup.work.hokan;
        plc_stop_player(&mut setup.work);
        assert_eq!(setup.work.hokan, hokan);
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
