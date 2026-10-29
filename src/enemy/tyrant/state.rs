//! Split out of the parent module so that edits there do not move these
//! definitions: types inside `GameState` are inlined into many codegen
//! units, and moving one recompiles all of them (#266).

use super::*;

/// One ribbon history slot: the matrix pair, their element-wise midpoint and
/// the near/far blade endpoints (the original's `+0x90`/`+0xb0`/`+0xd0`/
/// `+0xf0`/`+0xf8`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TrailSlot {
    /// Blade A (the previous frame's claw world matrix at store time).
    pub mat_a: Mat4x3,
    /// Blade B (the freshly computed matrix).
    pub mat_b: Mat4x3,
    /// `mid`: per-element `(b - a) / 2 + a`, truncating toward zero.
    pub mid: Mat4x3,
    /// Near endpoint in blade B's local frame.
    pub near: [i16; 3],
    /// Far endpoint in blade B's local frame.
    pub far: [i16; 3],
}

/// The ribbon's persistent state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trail {
    /// The nine history slots.
    pub slots: [TrailSlot; TRAIL_SLOTS],
    /// `0x004ba264`: bit `0x8000` arms the sweep, the low bits count the tail
    /// down.
    pub timer: u16,
    /// `0x004ba268`: the number of live history segments.
    pub segments: i32,
    /// `0x004ba270`: the near blade endpoint (0, -300, 0).
    pub near: [i16; 3],
    /// `0x004ba278`: the far blade endpoint (0, 1500, 0), swept by the arm.
    pub far: [i16; 3],
    /// `DAT_008fc41c`: the segment tint byte (0x70 red).
    pub tint: u8,
    /// The alloc ran (the arm and update paths are gated on this in the port;
    /// the original gates on the pool pointer).
    pub allocated: bool,
}

/// The two claw-ghost copies and their oscillation counters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ghost {
    /// `0x004ba254`: the X/Z scale counter, a signed 16-bit word.
    pub scale_a: i16,
    /// `0x004ba258`: the Y scale counter, a signed 16-bit word.
    pub scale_b: i16,
    /// `0x004ba25c`: the per-frame step, a signed byte (0xC8 = -56).
    pub step: i8,
    /// The copy matrices: `[0]` scaled, `[1]` unscaled (the original's
    /// `block + i * 0x7c + 0x44`, copy index 0 first).
    pub matrices: [Mat4x3; 2],
    /// Both copies were refreshed at least once (the draw gate).
    pub live: bool,
}

/// One severed limb: a free joint body.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Limb {
    /// The joint's world matrix, mutated directly by the physics.
    pub world: Mat4x3,
    /// The tumble rotation (`joint + 4`), a signed rotation triple.
    pub rotation: [i16; 3],
    /// The X velocity (`joint + 0x72`).
    pub vel_x: i16,
    /// The Y velocity (`joint + 0x70`), the one gravity accumulates into.
    pub vel_y: i16,
    /// The Z velocity (`joint + 0x74`).
    pub vel_z: i16,
    /// The per-frame gravity step (`joint + 0x76`).
    pub gravity: i16,
    /// The remaining bounces (`joint + 0x78`); zero stops the physics.
    pub bounces: u16,
    /// The limb was launched (the port's live gate for free-body rendering).
    pub live: bool,
}

/// The Tyrant scratch block.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct TyrantState {
    /// The slash ribbon.
    pub trail: Trail,
    /// The two claw ghosts.
    pub ghost: Ghost,
    /// The five free limbs, in launch order (joints 2, 4, 7, 10 and 12).
    pub limbs: [Limb; LIMB_COUNT],
}
