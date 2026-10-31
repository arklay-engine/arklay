//! RE2's room collision: pushing a body out of the RDT's SCA records.
//!
//! This follows `Sca_ck_info` in bio2's 1.10 PC executable: a record applies
//! when its area bits hold the body's quadrant around the header's centre,
//! its id passes the body's mask and its floor bits hold the body's floor.
//! The box test grows each record by the body's radius, and each shape then
//! has its own push, ported handler by handler from the table at `0x53AD00`
//! with the original's integer rounding and its 16-bit and 32-bit wrapping.

use crate::state::{Sca, ScaRect};

/// The record id bit the player's collision mask tests (`Sc_id 0x80 << 8`).
pub const PLAYER_MASK: u16 = 0x8000;
/// `_Spl`'s mask: the records that stop a script's character.
pub const CHARACTER_MASK: u16 = 0x1000;
/// The mask of the two characters `_Spl` treats apart, ids 0x45 and 0x4F.
pub const CHARACTER_MASK_LOW: u16 = 0x0800;
/// The record id bit that exempts a record from the body push.
const NO_PUSH: u16 = 0x0100;
/// The player's collision radius (`Player_set`).
pub const PLAYER_RADIUS: i32 = 0x1C2;
/// The collision radius of Claire's costumes and Sherry (`Pl_init`).
const SMALL_PLAYER_RADIUS: i32 = 0x15E;
/// How far ahead of the player the reach point stands (`Sce_at`).
pub const REACH_DISTANCE: i32 = 0x26C;
/// The height of one floor of a room.
pub const FLOOR_HEIGHT: i32 = 0x708;

/// The collision radius of the player character `id`.
pub fn player_radius(id: u8) -> i32 {
    if matches!(id, 1 | 3 | 5 | 7 | 9 | 0xE) {
        SMALL_PLAYER_RADIUS
    } else {
        PLAYER_RADIUS
    }
}

/// The floor a height stands on.
pub fn floor_of(y: i32) -> u8 {
    ((900 - y) / FLOOR_HEIGHT).clamp(0, 31) as u8
}

/// The result of one collision pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resolved {
    pub pos: [i32; 3],
    /// Whether a record pushed the body.
    pub hit: bool,
    /// The last push's axis: `Some(true)` for one that moved Z.
    pub push_z: Option<bool>,
}

/// The bit of the quadrant `(x, z)` lies in around the records' centre.
fn area(sca: &Sca, x: i32, z: i32) -> u16 {
    let quadrant = u32::from(x < i32::from(sca.cell_x)) + 2 * u32::from(z < i32::from(sca.cell_z));
    1 << quadrant
}

/// Whether the record applies to a player at `(x, z)` on `floor`, inside the
/// outline grown by `radius`.
fn applies(record: &ScaRect, mask: u16, area: u16, pos: [i32; 3], radius: i32, floor: u8) -> bool {
    record.kind & area != 0
        && record.id & mask != 0
        && record.floor & (1u32 << floor) != 0
        && inside_grown(record, pos[0], pos[2], radius)
}

fn inside_grown(record: &ScaRect, x: i32, z: i32, radius: i32) -> bool {
    let dx = x + radius - i32::from(record.x);
    let dz = z + radius - i32::from(record.z);
    (0..i32::from(record.width) + radius * 2).contains(&dx)
        && (0..i32::from(record.depth) + radius * 2).contains(&dz)
}

/// Move a body from `prev` to `proposed`, pushing it out of the records.
pub fn resolve(sca: &Sca, prev: [i32; 3], proposed: [i32; 3], radius: i32) -> Resolved {
    resolve_masked(sca, PLAYER_MASK, prev, proposed, radius)
}

/// [`resolve`] against the records for `mask`, `Sca_ck_em`'s second argument.
pub fn resolve_masked(
    sca: &Sca,
    mask: u16,
    prev: [i32; 3],
    proposed: [i32; 3],
    radius: i32,
) -> Resolved {
    let mut pos = proposed;
    let floor = floor_of(pos[1]);
    // `Sca_ck_info` picks the quadrant once, before anything pushes the body.
    let area = area(sca, pos[0], pos[2]);
    let mut hit = false;
    let mut push_z = None;
    for record in &sca.records {
        if !applies(record, mask, area, pos, radius, floor) || record.id & NO_PUSH != 0 {
            continue;
        }
        let before = pos;
        let pushed = match record.shape() {
            0 | 9 | 10 | 12 => push_box(record, prev, &mut pos, radius),
            1 => push_naname_a(record, prev, &mut pos, radius),
            2 => push_naname_b(record, prev, &mut pos, radius),
            3 => push_naname_c(record, prev, &mut pos, radius),
            4 => push_naname_d(record, prev, &mut pos, radius),
            5 => push_hishi(record, prev, &mut pos, radius),
            6 => push_circle(record, &mut pos, radius),
            7 => push_koban_x(record, prev, &mut pos, radius),
            8 => push_koban_z(record, prev, &mut pos, radius),
            11 => push_slope(record, prev, &mut pos, radius),
            13 => push_curve(record, &mut pos),
            _ => false,
        };
        if pushed {
            hit = true;
            if pos[2] != before[2] {
                push_z = Some(true);
            } else if pos[0] != before[0] {
                push_z = Some(false);
            }
        }
    }
    Resolved { pos, hit, push_z }
}

/// `Sca_hit_box`: the axis the previous position was already clear on is the
/// one pushed, out of the side the body came from. Its far edge rounds an odd
/// size down, as the original halves it.
fn push_box(record: &ScaRect, prev: [i32; 3], pos: &mut [i32; 3], radius: i32) -> bool {
    let (x, z) = (i32::from(record.x), i32::from(record.z));
    let (width, depth) = (i32::from(record.width), i32::from(record.depth));
    let old_x = prev[0] as i16 as i32 + radius - x;
    let old_z = prev[2] as i16 as i32 + radius - z;
    let span_x = width + radius * 2;
    let span_z = depth + radius * 2;
    let outside = |old: i32, span: i32| old < 0 || old >= span;
    let mut clear_x = outside(old_x, span_x);
    let mut clear_z = outside(old_z, span_z);
    if old_x == -1 || old_x == span_x + 1 {
        (clear_x, clear_z) = (true, false);
    }
    if old_z == -1 || old_z == span_z + 1 {
        (clear_x, clear_z) = (false, true);
    } else if !clear_x && !clear_z {
        leave_box(record, prev, pos, radius);
        return true;
    }
    if clear_x {
        let half = width >> 1;
        let reach = half + 1 + radius;
        let reach = if pos[0] - prev[0] as i16 as i32 >= 0 {
            -reach
        } else {
            reach
        };
        pos[0] = reach + half + x;
    }
    if clear_z {
        let half = depth >> 1;
        let reach = half + 1 + radius;
        let reach = if pos[2] - prev[2] as i16 as i32 >= 0 {
            -reach
        } else {
            reach
        };
        pos[2] = reach + z + half;
    }
    true
}

/// How far `Sca_hit_box` lets a body that's inside a box move on one axis,
/// when the way out agrees with how it was moving, and on either otherwise.
const LEAVE_WITH_MOTION: i32 = 0x191;
const LEAVE_AGAINST_MOTION: i32 = 0x321;

/// A body already inside a box (`FUN_004e0980`, `FUN_004e0b40`) leaves by the
/// nearer side on each axis. It takes the one shorter way out, and if that's
/// too far for how it was moving it goes back to where it was.
fn leave_box(record: &ScaRect, prev: [i32; 3], pos: &mut [i32; 3], radius: i32) {
    let (x, z) = (i32::from(record.x), i32::from(record.z));
    let nearer = |low: i32, high: i32| if -low < high { low } else { high };
    let out_x = nearer(
        x - pos[0] - radius - 1,
        i32::from(record.width) - pos[0] + x + 1 + radius,
    );
    let out_z = nearer(
        z - pos[2] - radius - 1,
        i32::from(record.depth) - pos[2] + z + 1 + radius,
    );
    let moved_x = pos[0] - prev[0] as i16 as i32;
    let moved_z = pos[2] - prev[2] as i16 as i32;
    let (cost_x, cost_z) = (out_x.abs(), out_z.abs());
    let against_x = (moved_x ^ out_x) < 0;
    let against_z = (moved_z ^ out_z) < 0;
    match (against_z, against_x) {
        (false, true) if cost_x < LEAVE_WITH_MOTION => {
            pos[0] += out_x;
            return;
        }
        (true, false) if cost_z < LEAVE_WITH_MOTION => {
            pos[2] += out_z;
            return;
        }
        (false, false) => {
            if cost_x < cost_z {
                pos[0] += out_x;
            } else {
                pos[2] += out_z;
            }
            return;
        }
        _ => {}
    }
    if cost_x < cost_z {
        if cost_x < LEAVE_AGAINST_MOTION {
            pos[0] += out_x;
            return;
        }
    } else if cost_z < LEAVE_AGAINST_MOTION {
        pos[2] += out_z;
        return;
    }
    pos[0] = prev[0] as i16 as i32;
    pos[2] = prev[2] as i16 as i32;
}

/// C's integer product, wrapping as the original's 32 bits do.
fn mul(a: i32, b: i32) -> i32 {
    a.wrapping_mul(b)
}

/// C's integer division. The original faults on a zero divisor; a body that
/// gets there here simply doesn't move.
fn div(a: i32, b: i32) -> i32 {
    if b == 0 { 0 } else { a.wrapping_div(b) }
}

/// `SquareRoot0`: a table of `sqrt(i) * 512` for the value's top bits, scaled
/// back by its exponent, so it rounds down coarsely for big values.
pub(crate) fn square_root0(value: i32) -> i32 {
    if value == 0 || value == -1 {
        return 0;
    }
    let shift = value.unsigned_abs().leading_zeros() & !1;
    let index = if shift < 24 {
        value >> (24 - shift)
    } else {
        value << (shift - 24)
    };
    let entry = (index.unsigned_abs() * 512 * 512).isqrt();
    ((entry << ((31 - shift) >> 1)) >> 12) as i32
}

/// The most a diagonal's slide moves a body on either axis in one pass.
const SLIDE_LIMIT: i32 = 0x191;
/// The diagonals compare sides on a coarse grid of this size.
const SIDE_GRID: i32 = 0x12;

/// The diagonals' slide for a body `a` past the edge, with `b` its counterpart
/// along the other axis. `None` when it's too far to slide.
fn slide(a: i32, b: i32) -> Option<(i32, i32)> {
    let length = mul(a, a).wrapping_add(mul(b, b));
    let along_x = div(mul(mul(b, a), a), length);
    let along_z = div(mul(mul(b, b), a), length);
    (along_x.wrapping_abs() < SLIDE_LIMIT && along_z.wrapping_abs() < SLIDE_LIMIT)
        .then_some((along_x, along_z))
}

/// A record's corners as the handlers read them: `(x, z, width, depth)`.
fn outline(record: &ScaRect) -> (i32, i32, i32, i32) {
    (
        i32::from(record.x),
        i32::from(record.z),
        i32::from(record.width),
        i32::from(record.depth),
    )
}

/// `(a + b)` kept to the 16 bits the handlers hold it in.
fn short(value: i32) -> i32 {
    i32::from(value as i16)
}

/// Whether `(x, z)` relative to a box lies inside its `(width, depth)`, by the
/// unsigned compare that also rejects negatives.
fn within(x: i32, z: i32, width: i32, depth: i32) -> bool {
    (x as u32) < width as u32 && (z as u32) < depth as u32
}

/// The end of each diagonal's slide: take it if it's short enough and the body
/// started on the open side, and otherwise go back to where the body was.
fn finish_slide(
    pos: &mut [i32; 3],
    prev: [i32; 3],
    target: Option<(i32, i32)>,
    start_is_open: bool,
) -> bool {
    if let Some((x, z)) = target {
        pos[0] = x;
        pos[2] = z;
        if start_is_open {
            return true;
        }
    }
    pos[0] = short(prev[0]);
    pos[2] = short(prev[2]);
    true
}

/// `Sca_hit_naname_a`: the triangle in the box's far corner.
fn push_naname_a(record: &ScaRect, prev: [i32; 3], pos: &mut [i32; 3], radius: i32) -> bool {
    let (x, z, width, depth) = outline(record);
    let (body_x, body_z) = (pos[0], pos[2]);
    let cut = div(
        mul(body_x - x + radius, depth + radius * 2),
        width + radius * 2,
    );
    let past = body_z - z - depth;
    if past <= -cut {
        return false;
    }
    let left = x / SIDE_GRID;
    let far = (z + depth) / SIDE_GRID;
    let right = (width + x) / SIDE_GRID;
    let side_z = z / SIDE_GRID - far;
    let side_x = right - left;
    let old = mul(short(prev[0]) / SIDE_GRID, side_z)
        - mul(short(prev[2]) / SIDE_GRID, side_x)
        - mul(z / SIDE_GRID, left)
        + mul(right, far);
    if old < 0 {
        if within(body_x - x, body_z - z, width + radius, depth + radius) {
            return push_box(record, prev, pos, radius);
        }
        return true;
    }
    let a = past + cut;
    let b = div(mul(width + radius, a), depth + radius);
    let open = mul(body_x / SIDE_GRID - left, side_z) - mul(body_z / SIDE_GRID - far, side_x) >= 0;
    let target = slide(a, b).map(|(dx, dz)| (body_x - dx, body_z - dz));
    finish_slide(pos, prev, target, open)
}

/// `Sca_hit_naname_b`: the triangle in the box's corner at low x, high z.
fn push_naname_b(record: &ScaRect, prev: [i32; 3], pos: &mut [i32; 3], radius: i32) -> bool {
    let (x, z, width, depth) = outline(record);
    let (body_x, body_z) = (pos[0], pos[2]);
    let far = short(z + depth);
    let right = short(width + x);
    let span_z = far - z + radius;
    let span_x = radius + right - x;
    let cut = div(mul(body_x - x, span_z), span_x);
    let into = body_z - z;
    if radius + into <= cut {
        return false;
    }
    let (left_grid, near_grid) = (x / SIDE_GRID, z / SIDE_GRID);
    let (right_grid, far_grid) = (right / SIDE_GRID, far / SIDE_GRID);
    let side_z = far_grid - near_grid;
    let side_x = right_grid - left_grid;
    let old = mul(short(prev[0]) / SIDE_GRID, side_z)
        - mul(short(prev[2]) / SIDE_GRID, side_x)
        - mul(far_grid, left_grid)
        + mul(right_grid, near_grid);
    if old < 0 {
        if within(body_x - x + radius, into, width + radius, depth + radius) {
            return push_box(record, prev, pos, radius);
        }
        return true;
    }
    let a = into - cut + radius;
    let b = div(mul(span_x, a), span_z);
    let open = mul(body_x / SIDE_GRID - left_grid, side_z)
        - mul(body_z / SIDE_GRID - near_grid, side_x)
        >= 0;
    let target = slide(a, b).map(|(dx, dz)| (body_x + dx, body_z - dz));
    finish_slide(pos, prev, target, open)
}

/// `Sca_hit_naname_c`: the triangle in the box's corner at high x, low z.
fn push_naname_c(record: &ScaRect, prev: [i32; 3], pos: &mut [i32; 3], radius: i32) -> bool {
    let (x, z, width, depth) = outline(record);
    let (body_x, body_z) = (pos[0], pos[2]);
    let far = short(z + depth);
    let right = short(width + x);
    let span_z = far - z + radius;
    let span_x = right - x + radius;
    let cut = div(mul(radius + body_x - x, span_z), span_x);
    let into = body_z - z;
    if cut <= into {
        return false;
    }
    let (left_grid, near_grid) = (x / SIDE_GRID, z / SIDE_GRID);
    let (right_grid, far_grid) = (right / SIDE_GRID, far / SIDE_GRID);
    let side_z = far_grid - near_grid;
    let side_x = right_grid - left_grid;
    let old = mul(short(prev[0]) / SIDE_GRID, side_z)
        - mul(short(prev[2]) / SIDE_GRID, side_x)
        - mul(far_grid, left_grid)
        + mul(right_grid, near_grid);
    if old < 1 {
        let a = into - cut;
        let b = div(mul(span_x, a), span_z);
        let open = mul(body_x / SIDE_GRID - left_grid, side_z)
            - mul(body_z / SIDE_GRID - near_grid, side_x)
            < 1;
        let target = slide(a, b).map(|(dx, dz)| (body_x + dx, body_z - dz));
        return finish_slide(pos, prev, target, open);
    }
    if within(body_x - x, into + radius, width + radius, depth + radius) {
        return push_box(record, prev, pos, radius);
    }
    true
}

/// `Sca_hit_naname_d`: the triangle in the box's near corner.
fn push_naname_d(record: &ScaRect, prev: [i32; 3], pos: &mut [i32; 3], radius: i32) -> bool {
    let (x, z, width, depth) = outline(record);
    let (body_x, body_z) = (pos[0], pos[2]);
    let far = short(depth + z);
    let right = short(width + x);
    let span_x = radius + right - x;
    let cut = div(mul(z - far - radius, body_x - x), span_x);
    let past = body_z - far - radius;
    if cut <= past {
        return false;
    }
    let left_grid = x / SIDE_GRID;
    let far_grid = far / SIDE_GRID;
    let right_grid = right / SIDE_GRID;
    let side_z = z / SIDE_GRID - far_grid;
    let side_x = right_grid - left_grid;
    let old = mul(short(prev[0]) / SIDE_GRID, side_z)
        - mul(short(prev[2]) / SIDE_GRID, side_x)
        - mul(z / SIDE_GRID, left_grid)
        + mul(right_grid, far_grid);
    if old < 1 {
        let a = past - cut;
        let b = div(mul(span_x, a), far - z + radius);
        let open = mul(body_x / SIDE_GRID - left_grid, side_z)
            - mul(body_z / SIDE_GRID - far_grid, side_x)
            < 1;
        let target = slide(a, b).map(|(dx, dz)| (body_x - dx, body_z - dz));
        return finish_slide(pos, prev, target, open);
    }
    if within(
        body_x - x + radius,
        body_z - z + radius,
        width + radius,
        depth + radius,
    ) {
        return push_box(record, prev, pos, radius);
    }
    true
}

/// `Sca_hit_hishi`: a diamond, handled by the quadrant of its centre the body
/// is in. Unlike the triangles, it judges the side after the slide.
fn push_hishi(record: &ScaRect, prev: [i32; 3], pos: &mut [i32; 3], radius: i32) -> bool {
    let (x, z, width, depth) = outline(record);
    let right = short(x + width);
    let far = short(z + depth);
    let mid_z = short((depth >> 1) + z);
    let mid_x = short((width >> 1) + x);
    let (body_x, body_z) = (pos[0], pos[2]);
    let grid = |value: i32| value / SIDE_GRID;
    let (a, b, moved);
    let open: fn(i32) -> bool;
    let (corner_x, corner_z, side_z, side_x);
    match (body_z < mid_z, body_x < mid_x) {
        (false, false) => {
            let span_x = right - mid_x + radius;
            let cut = div(mul(mid_z - far - radius, body_x - mid_x), span_x);
            let past = body_z - far - radius;
            if cut <= past {
                return false;
            }
            a = past - cut;
            b = div(mul(span_x, a), far - mid_z + radius);
            moved = (-1, -1);
            open = |side| side < 1;
            (corner_x, corner_z) = (grid(mid_x), grid(far));
            (side_z, side_x) = (grid(mid_z) - grid(far), grid(right) - grid(mid_x));
        }
        (false, true) => {
            let span_z = far - mid_z + radius;
            let span_x = mid_x - x + radius;
            let cut = div(mul(body_x - x + radius, span_z), span_x);
            let past = body_z - mid_z;
            if cut <= past {
                return false;
            }
            a = past - cut;
            b = div(mul(span_x, a), span_z);
            moved = (1, -1);
            open = |side| side < 1;
            (corner_x, corner_z) = (grid(x), grid(mid_z));
            (side_z, side_x) = (grid(far) - grid(mid_z), grid(mid_x) - grid(x));
        }
        (true, false) => {
            let span_z = mid_z - z + radius;
            let span_x = right - mid_x + radius;
            let cut = div(mul(body_x - mid_x, span_z), span_x);
            let into = body_z - z;
            if radius + into <= cut {
                return false;
            }
            a = into - cut + radius;
            b = div(mul(span_x, a), span_z);
            moved = (1, -1);
            open = |side| side >= 0;
            (corner_x, corner_z) = (grid(mid_x), grid(z));
            (side_z, side_x) = (grid(mid_z) - grid(z), grid(right) - grid(mid_x));
        }
        (true, true) => {
            let span_x = mid_x - x + radius;
            let cut = div(mul(body_x - x + radius, z - mid_z - radius), span_x);
            let past = body_z - mid_z;
            if past <= cut {
                return false;
            }
            a = past - cut;
            b = div(mul(span_x, a), mid_z - z + radius);
            moved = (-1, -1);
            open = |side| side >= 0;
            (corner_x, corner_z) = (grid(x), grid(mid_z));
            (side_z, side_x) = (grid(z) - grid(mid_z), grid(mid_x) - grid(x));
        }
    }
    if let Some((dx, dz)) = slide(a, b) {
        pos[0] = body_x + moved.0 * dx;
        pos[2] = body_z + moved.1 * dz;
        let side = mul(grid(pos[0]) - corner_x, side_z) - mul(grid(pos[2]) - corner_z, side_x);
        if open(side) {
            return true;
        }
    }
    pos[0] = short(prev[0]);
    pos[2] = short(prev[2]);
    true
}

/// Push out of a disc of diameter `size` whose box starts at `(left, near)`.
/// The original's `+ 8` leans every push the same way.
fn push_disc(pos: &mut [i32; 3], left: i32, near: i32, size: i32, radius: i32) -> bool {
    let half = size >> 1;
    let dx = pos[0] - left - half;
    let dz = pos[2] - near - half;
    let distance = square_root0(mul(dz, dz).wrapping_add(mul(dx, dx)));
    let reach = half - distance + radius;
    if reach < 1 {
        return false;
    }
    pos[0] += div(mul(dx + 8, reach), distance);
    pos[2] += div(mul(dz + 8, reach), distance);
    true
}

/// `Sca_hit_circle`: a disc whose diameter is the record's width.
fn push_circle(record: &ScaRect, pos: &mut [i32; 3], radius: i32) -> bool {
    let (x, z, width, _) = outline(record);
    push_disc(pos, x, z, width, radius)
}

/// `Sca_hit_koban_x`: a box with rounded ends along x.
fn push_koban_x(record: &ScaRect, prev: [i32; 3], pos: &mut [i32; 3], radius: i32) -> bool {
    let (x, z, width, depth) = outline(record);
    let half = depth >> 1;
    let before_right = pos[0] < short(x - half + width);
    let before_left = pos[0] < short(x + half);
    let end = match (before_right, before_left) {
        (false, false) => short(x - depth + width),
        (true, true) => x,
        (true, false) => return push_box(record, prev, pos, radius),
        (false, true) => return false,
    };
    push_disc(pos, end, z, depth, radius)
}

/// `Sca_hit_koban_z`: a box with rounded ends along z.
fn push_koban_z(record: &ScaRect, prev: [i32; 3], pos: &mut [i32; 3], radius: i32) -> bool {
    let (x, z, width, depth) = outline(record);
    let half = width >> 1;
    let before_far = pos[2] < short(z - half + depth);
    let before_near = pos[2] < short(z + half);
    let end = match (before_far, before_near) {
        (false, false) => short(depth - width + z),
        (true, true) => z,
        (true, false) => return push_box(record, prev, pos, radius),
        (false, true) => return false,
    };
    push_disc(pos, x, end, width, radius)
}

/// `Sca_hit_slope`: a body on the slope takes its height from its place along
/// it, and the slope's long sides block like a box.
fn push_slope(record: &ScaRect, prev: [i32; 3], pos: &mut [i32; 3], radius: i32) -> bool {
    let along_z = record.kind & 0x20 != 0;
    let (x, z, width, depth) = outline(record);
    let (across_prev, across_low, across_high) = if along_z {
        (short(prev[0]), x - radius, x + width + radius)
    } else {
        (short(prev[2]), z - radius, z + depth + radius)
    };
    if !(across_low..=across_high).contains(&across_prev) {
        return push_box(record, prev, pos, radius);
    }
    let on_slope = if along_z {
        (z..=z + depth).contains(&pos[2])
    } else {
        (x..=x + width).contains(&pos[0])
    };
    if on_slope {
        pos[1] = slope_height(record, pos[0], pos[2]);
    } else {
        // Off either end: settle on the floor there.
        pos[1] = i32::from(floor_of(pos[1])) * -FLOOR_HEIGHT;
    }
    true
}

/// The height of a slope record at `(x, z)` (`FUN_004e2930`).
fn slope_height(record: &ScaRect, x: i32, z: i32) -> i32 {
    let (left, near, width, depth) = outline(record);
    let rise = match (record.kind & 0x30) >> 4 {
        0 => div((left - x + width) * FLOOR_HEIGHT, width),
        1 => div((x - left) * FLOOR_HEIGHT, width),
        2 => div((near - z + depth) * FLOOR_HEIGHT, depth),
        _ => div((z - near) * FLOOR_HEIGHT, depth),
    };
    rise.clamp(0, FLOOR_HEIGHT) - i32::from((record.kind & 0x7C0) >> 6) * FLOOR_HEIGHT
}

/// `Sca_hit_curve`: a half pipe. The body's height and z follow a circle whose
/// diameter is the record's depth, standing on the record's lowest floor.
fn push_curve(record: &ScaRect, pos: &mut [i32; 3]) -> bool {
    let half = i32::from(record.depth >> 1);
    let near = i32::from(record.z);
    let from_middle = (near - pos[2] + half).abs();
    if half / 2 < from_middle {
        pos[1] += div((from_middle * 2 - half) * 0xA0, half);
    }
    let dz = pos[2] - near - half;
    let bottom = record.floor.trailing_zeros().min(31) as i32 * -FLOOR_HEIGHT;
    let dy = pos[1] + half - bottom;
    let distance = square_root0(mul(dy, dy).wrapping_add(mul(dz, dz)));
    let error = half - distance;
    if error.abs() > 10 {
        pos[2] += div(mul(dz, error), distance + 1);
        pos[1] += div(mul(dy, error), distance + 1);
    }
    true
}

/// `Sca_hit_stairs`'s and the front point's reach.
const STAIRS_REACH: i32 = 0x1C2;

/// The stairs a player at `pos` can climb with the action key, if any:
/// `Set_front_pos`'s point in front of the player, in `Sca_ck_info`'s reach.
pub fn stairs_ahead(sca: &Sca, pos: [i32; 3], angle: u16, radius: i32) -> Option<&ScaRect> {
    let (dx, dz) = crate::player2::rotate(angle, (radius * 0xB3) >> 7, 0);
    let (x, z) = (pos[0] + dx, pos[2] + dz);
    let floor = floor_of(pos[1]);
    let area = area(sca, x, z);
    // `Sca_ck_info` doesn't stop at a hit, so the last stairs in reach win.
    sca.records.iter().rev().find(|record| {
        record.shape() == 12
            && applies(
                record,
                PLAYER_MASK,
                area,
                [x, pos[1], z],
                STAIRS_REACH,
                floor,
            )
    })
}

/// `Sca_ck_info`'s bit for a record the player can climb onto (shape 9).
pub const INFO_UP: u16 = 2;
/// `Sca_ck_info`'s bit for a record the player can jump down from (shape 10).
pub const INFO_DOWN: u16 = 4;

/// `Sca_ck_info` at a point, with no reach: the climb bits of the records
/// holding it.
pub fn info_at(sca: &Sca, mask: u16, pos: [i32; 3], floor: u8) -> u16 {
    let area = area(sca, pos[0], pos[2]);
    sca.records
        .iter()
        .filter(|record| applies(record, mask, area, pos, 0, floor))
        .fold(0, |info, record| match record.shape() {
            9 => info | 3,
            10 => info | INFO_DOWN,
            _ => info,
        })
}

/// What `Sca_ck_hit` finds at a body's position.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ground {
    /// The highest top the body stands at or above: its return value.
    pub height: i32,
    /// A record rises past the body's height, or the body is under the floor
    /// (`DWORD_00695E84`'s bit 0).
    pub blocked: bool,
    /// The lowest top rising past the body (`DWORD_00695E74`).
    pub ceiling: i32,
}

/// `Sca_ck_hit`, without the object models.
pub fn ground_at(sca: &Sca, pos: [i32; 3], radius: i32, mask: u16) -> Ground {
    let mut ground = Ground {
        blocked: pos[1] > 0,
        ..Ground::default()
    };
    let area = area(sca, pos[0], pos[2]);
    for record in &sca.records {
        if record.kind & area == 0
            || record.id & mask == 0
            || record.shape() == 10
            || !inside_grown(record, pos[0], pos[2], radius)
            || !inside_shape(record, pos[0], pos[2], radius)
        {
            continue;
        }
        let top_floor = record.floor.trailing_zeros().min(32) as i32 * -FLOOR_HEIGHT;
        let height = height_at(record, pos, top_floor);
        if pos[1] <= top_floor {
            if height < pos[1] {
                ground.blocked = true;
                ground.ceiling = ground.ceiling.min(height);
            } else {
                ground.height = ground.height.min(height);
            }
        }
    }
    ground
}

/// Within a record's own shape, given it's within its outline: `Sca_ck_naname_a`
/// to `Sca_ck_circle`, and `FUN_004e2250` and `FUN_004e23e0`.
fn inside_shape(record: &ScaRect, x: i32, z: i32, r: i32) -> bool {
    let (left, near, width, depth) = outline(record);
    let (dx, dz) = (x - left, z - near);
    let (across, along) = (width + r * 2, depth + r * 2);
    let circle = |left: i32, near: i32, size: i32| {
        let half = size / 2;
        let (dx, dz) = (x - left - half, z - near - half);
        let distance = square_root0(mul(dz, dz).wrapping_add(mul(dx, dx)));
        half + r - distance > 0
    };
    match record.shape() {
        1 => div(mul(-along, dx + r), across) < dz - depth - r,
        2 => div(mul(dx - width - r, along), across) < dz - depth - r,
        3 => dz + r < div(mul(dx + r, along), across),
        4 => dz + r < div(mul(-along, dx - width - r), across),
        5 => {
            let (right, far) = (left + width, near + depth);
            let (middle_x, middle_z) = (left + width / 2, near + depth / 2);
            let (from_x, from_z) = (x - middle_x, z - middle_z);
            match (from_z < 0, from_x < 0) {
                (false, false) => {
                    z - far - r < div(mul(middle_z - far - r, from_x), right - middle_x + r)
                }
                (false, true) => {
                    from_z < div(mul(far - middle_z + r, x - left + r), middle_x - left + r)
                }
                (true, false) => {
                    div(mul(middle_z - near + r, from_x), right - middle_x + r) < z - near + r
                }
                (true, true) => {
                    div(mul(near - middle_z - r, x - left + r), middle_x - left + r) < from_z
                }
            }
        }
        6 => circle(left, near, width),
        7 => {
            let half = depth / 2;
            if x < left + half {
                circle(left, near, depth)
            } else if x < left - half + width {
                true
            } else {
                circle(left + width - depth, near, depth)
            }
        }
        8 => {
            let half = width / 2;
            if z < near + half {
                circle(left, near, width)
            } else if z < near - half + depth {
                true
            } else {
                circle(left, near + depth - width, width)
            }
        }
        _ => true,
    }
}

/// The height `Sca_ck_hit` takes from a record at a point: flat with a raised
/// top, or a slope's, stairs' or half pipe's there.
fn height_at(record: &ScaRect, pos: [i32; 3], top_floor: i32) -> i32 {
    let base = i32::from((record.kind & 0x7C0) >> 6) * -FLOOR_HEIGHT;
    let (left, near, width, depth) = outline(record);
    let [x, _, z] = pos;
    match record.shape() {
        0xB => slope_height(record, x, z),
        // `FUN_004e29f0`.
        0xC => {
            let run = match (record.kind & 0x30) >> 4 {
                0 => width - x + left,
                1 => x - left,
                2 => depth - z + near,
                _ => z - near,
            };
            let rise = (run / 0x21C * STAIRS_REACH).min(top_floor - base).max(0);
            base + rise
        }
        // `FUN_004e2aa0`.
        0xD => {
            let half = depth / 2;
            let from_middle = near - z + half;
            let across = mul(half, half).wrapping_sub(mul(from_middle, from_middle));
            square_root0(across) + top_floor - half
        }
        _ => base + i32::from(record.kind >> 11) * -100,
    }
}

/// The floor heights at `stairs`' two ends: `FUN_004E3910` and `FUN_004E3940`.
pub fn stairs_ends(stairs: &ScaRect) -> (i32, i32) {
    let low = (-i32::from((stairs.kind >> 6) & 0x1F) * 0x12 - i32::from(stairs.kind >> 11)) * 100;
    let mut bits = stairs.floor;
    let mut high = 0;
    while bits & 1 == 0 && bits != 0 {
        high -= FLOOR_HEIGHT;
        bits >>= 1;
    }
    (low, high)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_climbable_box_answers_its_height_and_bits() {
        // Room 118's bin and the edge of its top.
        let sca = room(vec![
            ScaRect {
                x: -23361,
                z: -7975,
                width: 1930,
                depth: 3088,
                id: 0xFC09,
                kind: 0x900F,
                floor: 1,
            },
            ScaRect {
                x: -25131,
                z: -7975,
                width: 1769,
                depth: 2798,
                id: 0xFC0A,
                kind: 0x904F,
                floor: 2,
            },
        ]);
        let on_bin = [-22500, -FLOOR_HEIGHT, -6000];
        let ground = ground_at(&sca, on_bin, 450, PLAYER_MASK);
        assert_eq!(ground.height, -FLOOR_HEIGHT);
        assert!(!ground.blocked);
        assert_eq!(info_at(&sca, PLAYER_MASK, [-22500, 0, -6000], 0), 3);
        assert_eq!(info_at(&sca, PLAYER_MASK, [-22500, 0, -6000], 1), 0);
        // Off the top, the ground is the street again.
        let beside = [-24000, -FLOOR_HEIGHT, -6000];
        assert_eq!(info_at(&sca, PLAYER_MASK, beside, 1), INFO_DOWN);
        assert_eq!(ground_at(&sca, beside, 450, PLAYER_MASK).height, 0);
    }

    fn room(records: Vec<ScaRect>) -> Sca {
        Sca {
            records,
            ..Sca::default()
        }
    }

    fn record(shape: u16) -> ScaRect {
        ScaRect {
            x: 1000,
            z: 1000,
            width: 1000,
            depth: 1000,
            id: PLAYER_MASK | shape,
            kind: 0xF,
            floor: 1,
        }
    }

    #[test]
    fn walking_into_a_box_stops_at_its_grown_edge() {
        let sca = room(vec![record(0)]);
        let hit = resolve(&sca, [0, 0, 1500], [900, 0, 1500], PLAYER_RADIUS);
        assert_eq!(hit.pos, [1000 - PLAYER_RADIUS - 1, 0, 1500]);
        assert!(hit.hit);
        assert_eq!(hit.push_z, Some(false));
    }

    #[test]
    fn a_box_lets_the_body_slide_along_its_side() {
        let sca = room(vec![record(0)]);
        let start = 1000 - PLAYER_RADIUS - 1;
        let hit = resolve(&sca, [start, 0, 1500], [start + 50, 0, 1600], PLAYER_RADIUS);
        assert_eq!(hit.pos, [start, 0, 1600]);
    }

    #[test]
    fn a_character_answers_to_its_own_records() {
        let mut sca = crate::state::Sca::default();
        sca.records.push(ScaRect {
            kind: 0xFFFF,
            floor: u32::MAX,
            id: PLAYER_MASK,
            x: 1000,
            z: 1000,
            width: 1000,
            depth: 1000,
        });
        let (prev, proposed) = ([0, 0, 1500], [900, 0, 1500]);
        let blocked = resolve_masked(&sca, PLAYER_MASK, prev, proposed, PLAYER_RADIUS);
        assert!(blocked.hit);
        let through = resolve_masked(&sca, CHARACTER_MASK, prev, proposed, PLAYER_RADIUS);
        assert!(!through.hit);
        assert_eq!(through.pos, proposed);
    }

    #[test]
    fn a_record_for_another_mask_or_floor_or_quadrant_does_not_block() {
        let mut other_mask = record(0);
        other_mask.id = 0x2000;
        let mut other_floor = record(0);
        other_floor.floor = 2;
        let mut other_area = record(0);
        other_area.kind = 0x4;
        let sca = room(vec![other_mask, other_floor, other_area]);
        let free = resolve(&sca, [0, 0, 1500], [1500, 0, 1500], PLAYER_RADIUS);
        assert_eq!(free.pos, [1500, 0, 1500]);
        assert!(!free.hit);
    }

    #[test]
    fn a_record_on_the_second_floor_blocks_only_up_there() {
        let mut upstairs = record(0);
        upstairs.floor = 2;
        let sca = room(vec![upstairs]);
        let hit = resolve(&sca, [0, -1800, 1500], [900, -1800, 1500], PLAYER_RADIUS);
        assert!(hit.hit);
    }

    #[test]
    fn the_body_leaves_a_disc_radially() {
        let sca = room(vec![record(6)]);
        let hit = resolve(&sca, [0, 0, 1500], [1000, 0, 1500], PLAYER_RADIUS);
        // The coarse root makes the distance 499, and pushes lean by `+ 8`.
        assert_eq!(
            hit.pos,
            [1000 + (-492 * 451) / 499, 0, 1500 + (8 * 451) / 499]
        );
    }

    #[test]
    fn each_triangle_blocks_only_its_solid_corner() {
        // (solid corner of the box, clear corner) for shapes 1 to 4.
        let far = 2000 + PLAYER_RADIUS - 10;
        let near = 1000 - PLAYER_RADIUS + 10;
        let cases = [
            (1, [far, far], [near, near]),
            (2, [near, far], [far, near]),
            (3, [far, near], [near, far]),
            (4, [near, near], [far, far]),
        ];
        for (shape, solid, clear) in cases {
            let sca = room(vec![record(shape)]);
            let hit = resolve(&sca, [0, 0, 0], [solid[0], 0, solid[1]], PLAYER_RADIUS);
            assert!(hit.hit, "shape {shape} should block {solid:?}");
            let again = resolve(&sca, hit.pos, hit.pos, PLAYER_RADIUS);
            assert!(!again.hit, "shape {shape} should leave the body clear");
            let free = resolve(&sca, [0, 0, 0], [clear[0], 0, clear[1]], PLAYER_RADIUS);
            assert!(!free.hit, "shape {shape} should not block {clear:?}");
        }
    }

    #[test]
    fn the_ground_is_only_within_a_records_own_shape() {
        let near = 1000 + 10;
        let far = 2000 - 10;
        for (shape, solid, clear) in [
            (1, [far, far], [near, near]),
            (2, [near, far], [far, near]),
            (3, [far, near], [near, far]),
            (4, [near, near], [far, far]),
            (5, [1500, 1500], [near, near]),
            (6, [1500, 1500], [near, near]),
        ] {
            let mut raised = record(shape);
            raised.kind |= 1 << 6;
            let sca = room(vec![raised]);
            let at = |[x, z]: [i32; 2]| ground_at(&sca, [x, 0, z], 2, PLAYER_MASK);
            assert!(at(solid).blocked, "shape {shape} at {solid:?}");
            assert!(!at(clear).blocked, "shape {shape} at {clear:?}");
        }
    }

    #[test]
    fn a_diamond_blocks_its_middle_but_not_its_box_corners() {
        let sca = room(vec![record(5)]);
        let near = 1000 - PLAYER_RADIUS + 10;
        assert!(!resolve(&sca, [0, 0, 0], [near, 0, near], PLAYER_RADIUS).hit);
        let hit = resolve(&sca, [0, 0, 1500], [1100, 0, 1500], PLAYER_RADIUS);
        assert!(hit.hit);
        assert!(!resolve(&sca, hit.pos, hit.pos, PLAYER_RADIUS).hit);
    }

    #[test]
    fn a_slope_lifts_the_body_along_it() {
        let mut slope = record(11);
        slope.kind = 0xF | (3 << 4);
        let sca = room(vec![slope]);
        let half = resolve(&sca, [1500, 0, 1400], [1500, 0, 1500], PLAYER_RADIUS);
        assert_eq!(half.pos[1], 900);
        assert!(half.hit);
    }

    #[test]
    fn square_root0_follows_the_tables_coarse_rounding() {
        assert_eq!(square_root0(0), 0);
        assert_eq!(square_root0(-1), 0);
        assert_eq!(square_root0(4), 2);
        // The table holds only eight bits of the value, and rounds down.
        assert_eq!(square_root0(250_000), 499);
        assert_eq!(square_root0(1_000_001), 999);
        assert_eq!(square_root0(0x7FFF_FFFF), 46_152);
    }

    #[test]
    fn a_rounded_box_pushes_radially_past_its_ends_and_flat_along_its_sides() {
        let mut koban = record(7);
        koban.width = 3000;
        koban.depth = 1000;
        let sca = room(vec![koban]);
        // Off the low end: out of the end's disc.
        let end = resolve(&sca, [0, 0, 1500], [800, 0, 1500], PLAYER_RADIUS);
        assert_eq!(
            end.pos,
            [800 + (-692 * 252) / 698, 0, 1500 + (8 * 252) / 698]
        );
        // Along the middle: `Sca_hit_box`.
        let side = resolve(&sca, [2000, 0, 0], [2000, 0, 800], PLAYER_RADIUS);
        assert_eq!(side.pos, [2000, 0, 1000 - PLAYER_RADIUS - 1]);
    }

    #[test]
    fn a_diagonal_slides_a_body_along_it() {
        // Shape 1: walking straight in from the open side moves along x too.
        let sca = room(vec![record(1)]);
        let start = [1400, 0, 1400];
        let hit = resolve(&sca, start, [1600, 0, 1600], PLAYER_RADIUS);
        assert!(hit.hit);
        assert_ne!(hit.pos[0], 1600);
        assert_ne!(hit.pos[2], 1600);
        // A slide too long to take goes back to where the body was.
        let far = resolve(&sca, [0, 0, 0], [2000, 0, 2000], PLAYER_RADIUS);
        assert!(far.hit);
        assert_eq!(far.pos, [0, 0, 0]);
    }

    #[test]
    fn stairs_are_found_at_their_foot_and_know_their_ends() {
        let mut stairs = record(12);
        stairs.floor = 2;
        stairs.kind = 0xF;
        let sca = room(vec![stairs]);
        // Angle 0 faces +x: in front of the player, and not behind.
        let foot = [1000 - PLAYER_RADIUS - 1, -1800, 1500];
        let found = stairs_ahead(&sca, foot, 0, PLAYER_RADIUS).unwrap();
        let away = [1000 - 1079 - 1, -1800, 1500];
        assert!(stairs_ahead(&sca, away, 0x800, PLAYER_RADIUS).is_none());
        assert_eq!(stairs_ends(found), (0, -1800));
        // Three floors' worth, as room 116's second set of stairs is.
        stairs.floor = 30;
        stairs.kind = 0x9113;
        assert_eq!(stairs_ends(&stairs), (-9000, -1800));
    }

    #[test]
    fn the_last_stairs_in_reach_are_the_ones_taken() {
        let mut first = record(12);
        first.floor = 2;
        first.kind = 0xF;
        let mut last = first;
        last.kind = 0x10F;
        let sca = room(vec![first, last]);
        let foot = [1000 - PLAYER_RADIUS - 1, -1800, 1500];
        let found = stairs_ahead(&sca, foot, 0, PLAYER_RADIUS).unwrap();
        assert_eq!(found.kind, 0x10F);
    }
}
