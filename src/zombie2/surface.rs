//! `Super_matrix`: the surface an enemy walks on when it isn't the floor. A body
//! without one stands on the floor, so nothing here costs the others anything.

use crate::player2::{direction_ck, mul_matrix, rot_matrix};

pub type Rotation = [[i16; 3]; 3];

const IDENTITY: Rotation = [[0x1000, 0, 0], [0, 0x1000, 0], [0, 0, 0x1000]];

/// `ApplyMatrix`.
pub(crate) fn apply(m: &Rotation, v: [i32; 3]) -> [i32; 3] {
    std::array::from_fn(|row| {
        let sum: i64 = (0..3).map(|k| i64::from(m[row][k]) * i64::from(v[k])).sum();
        (sum >> 12) as i32
    })
}

/// `Rot_vector_super`: `v` turned by `angle`, on the surface.
pub(super) fn rotate(surface: Option<Rotation>, angle: u16, v: [i32; 3]) -> [i32; 3] {
    let turn = rot_matrix([0, angle as i16, 0]);
    let m = surface.map_or(turn, |surface| mul_matrix(&surface, &turn));
    apply(&m, v).map(|axis| i32::from(axis as i16))
}

/// `Get_axis2`: which way the surface's up points. 0 is up, 1 down, 2 and 3 `x`
/// each way, 4 and 5 `z`.
pub(super) fn axis(surface: Option<Rotation>) -> u8 {
    let [x, y, z] = rotate(surface, 0, [0, 0x7FFF, 0]);
    let mut axis = u8::from(y < -0x4000);
    if x > 0x4000 {
        axis = 2;
    }
    if x < -0x4000 {
        axis = 3;
    }
    if z > 0x4000 {
        axis = 4;
    }
    if z < -0x4000 {
        axis = 5;
    }
    axis
}

/// `FUN_004e3150`: 0 for a floor or a ceiling, 1 for a wall across `x` and 2 for
/// one across `z`.
pub(super) fn plane(surface: Option<Rotation>) -> u8 {
    let [x, _, z] = rotate(surface, 0, [0, -0x7FFF, 0]);
    if z.abs() > 0x4000 {
        2
    } else {
        u8::from(x.abs() > 0x4000)
    }
}

/// `ratan2`, whose truncation never shows: the angles here are whole quarters.
fn ratan2(y: i32, x: i32) -> i16 {
    (f64::from(y).atan2(f64::from(x)) * 2048.0 / std::f64::consts::PI) as i16
}

/// `Rot_add_matrix`, with the rounding `Em_22` does after it: the surface turned
/// by `add`, and the vector that makes it.
pub(super) fn turned(surface: Option<Rotation>, add: [i16; 3]) -> ([i16; 3], Rotation) {
    let m = mul_matrix(&rot_matrix(add), &surface.unwrap_or(IDENTITY));
    let vx = ratan2(-i32::from(m[1][2]), i32::from(m[2][2]));
    let vy = ratan2(
        i32::from(m[0][2]),
        ((0x1000 * 0x1000 - i32::from(m[0][2]) * i32::from(m[0][2])) as f64).sqrt() as i32,
    );
    let undo = rot_matrix([vx, vy, 0]);
    let undo: Rotation = std::array::from_fn(|r| std::array::from_fn(|c| undo[c][r]));
    let column = [m[0][0], m[1][0], m[2][0]].map(i32::from);
    let [x, y, _] = apply(&undo, column);
    let vector = [vx, vy, ratan2(y, x)].map(|axis| (axis.wrapping_add(0x40)) & !0x7F);
    (vector, rot_matrix(vector))
}

/// The turn at the end of `Goto00_super` and `Goto01_super`.
fn turn(angle: u16, difference: i32, rate: i16) -> u16 {
    let rate32 = i32::from(rate);
    let off = (rate32 + difference) & 0xFFF;
    if rate32 * 2 > off {
        angle
    } else if off < 0x801 {
        angle.wrapping_add(rate as u16)
    } else {
        angle.wrapping_sub(rate as u16)
    }
}

/// Its forward direction and how to read two axes of a point, in the plane the
/// surface has it turning in: which of `pos` are across and along, and whether
/// the turn runs the other way.
fn facing(surface: Option<Rotation>, angle: u16) -> (i32, usize, usize, bool) {
    let forward = rotate(surface, angle, [0x7FFF, 0, 0]);
    let (a, b, flipped) = match axis(surface) {
        0 => (0, 2, false),
        1 => (0, 2, true),
        2 => (2, 1, false),
        3 => (2, 1, true),
        4 => (1, 0, false),
        _ => (1, 0, true),
    };
    let direction = i32::from(direction_ck(0, 0, forward[a], forward[b]) as i16);
    (direction, a, b, flipped)
}

/// `Goto00_super`.
pub(super) fn goto_point(
    surface: Option<Rotation>,
    angle: u16,
    pos: [i32; 3],
    to: [i32; 3],
    rate: i16,
) -> u16 {
    let (now, a, b, flipped) = facing(surface, angle);
    let target = i32::from(direction_ck(pos[a], pos[b], to[a], to[b]) as i16);
    turn(
        angle,
        if flipped { now - target } else { target - now },
        rate,
    )
}

/// `Goto01_super`.
pub(super) fn goto_angle(surface: Option<Rotation>, angle: u16, to: i32, rate: i16) -> u16 {
    let (now, _, _, flipped) = facing(surface, angle);
    turn(angle, if flipped { now - to } else { to - now }, rate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_body_without_a_surface_stands_on_the_floor() {
        assert_eq!(axis(None), 0);
        assert_eq!(plane(None), 0);
        assert_eq!(rotate(None, 0, [100, 0, 0]), [100, 0, 0]);
        assert_eq!(rotate(None, 0x400, [100, 0, 0]), [0, 0, -100]);
    }

    #[test]
    fn a_quarter_turn_puts_it_on_a_wall_and_back() {
        let (vector, wall) = turned(None, [0, 0, 0x400]);
        assert_eq!(vector, [0, 0, 0x400]);
        assert_eq!(wall, rot_matrix([0, 0, 0x400]));
        assert_eq!(axis(Some(wall)), 3);
        assert_eq!(plane(Some(wall)), 1);
        let (vector, floor) = turned(Some(wall), [0, 0, -0x400]);
        assert_eq!(vector, [0, 0, 0]);
        assert_eq!(axis(Some(floor)), 0);
        let (_, wall) = turned(None, [0x400, 0, 0]);
        assert_eq!(plane(Some(wall)), 2);
        let (_, ceiling) = turned(None, [0x800, 0x800, 0]);
        assert_eq!(axis(Some(ceiling)), 1);
    }

    #[test]
    fn a_turn_on_the_surface_steps_toward_its_target() {
        // Facing +x on the floor, a target at +z is at 0xC00.
        let angle = goto_point(None, 0, [0, 0, 0], [0, 0, 1000], 0x40);
        assert_eq!(angle, 0u16.wrapping_sub(0x40));
        let angle = goto_angle(None, 0, 0x400, 0x20);
        assert_eq!(angle, 0x20);
        // Near enough, it stays.
        assert_eq!(goto_angle(None, 0x400, 0x400, 0x20), 0x400);
    }
}
