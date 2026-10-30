//! RE2's room collision: pushing a body out of the RDT's SCA records.
//!
//! This follows `Sca_ck_info` in bio2's 1.10 PC executable: a record applies
//! when its area bits hold the body's quadrant around the header's centre,
//! its id passes the body's mask and its floor bits hold the body's floor.
//! The box test grows each record by the body's radius, and each shape then
//! has its own push. The boxes follow `Sca_hit_box` exactly; the other shapes
//! push straight out of the same outline the original tests, which differs
//! from its hand-tuned slides in small ways (`docs/re2-deviations.md`).

use crate::state::{Sca, ScaRect};

/// The record id bit the player's collision mask tests (`Sc_id 0x80 << 8`).
pub const PLAYER_MASK: u16 = 0x8000;
/// The record id bit that exempts a record from the body push.
const NO_PUSH: u16 = 0x0100;
/// The player's collision radius (`Player_set`).
pub const PLAYER_RADIUS: i32 = 0x1C2;
/// How far ahead of the player the reach point stands (`Sce_at`).
pub const REACH_DISTANCE: i32 = 0x26C;
/// The height of one floor of a room.
pub const FLOOR_HEIGHT: i32 = 0x708;

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
fn applies(sca: &Sca, record: &ScaRect, pos: [i32; 3], radius: i32, floor: u8) -> bool {
    record.kind & area(sca, pos[0], pos[2]) != 0
        && record.id & PLAYER_MASK != 0
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
    let mut pos = proposed;
    let floor = floor_of(pos[1]);
    let mut hit = false;
    let mut push_z = None;
    for record in &sca.records {
        if !applies(sca, record, pos, radius, floor) || record.id & NO_PUSH != 0 {
            continue;
        }
        let before = pos;
        let pushed = match record.shape() {
            0 | 9 | 12 => push_box(record, prev, &mut pos, radius),
            shape @ 1..=4 => push_triangle(record, shape, &mut pos, radius),
            5 => push_diamond(record, &mut pos, radius),
            6 => push_circle(record, &mut pos, radius),
            7 => push_capsule(record, true, &mut pos, radius),
            8 => push_capsule(record, false, &mut pos, radius),
            11 => push_slope(record, prev, &mut pos, radius),
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
/// one pushed, out of the side the body came from.
fn push_box(record: &ScaRect, prev: [i32; 3], pos: &mut [i32; 3], radius: i32) -> bool {
    let left = i32::from(record.x) - radius - 1;
    let right = i32::from(record.x) + i32::from(record.width) + radius + 1;
    let near = i32::from(record.z) - radius - 1;
    let far = i32::from(record.z) + i32::from(record.depth) + radius + 1;
    let clear_x = !(left < prev[0] && prev[0] < right);
    let clear_z = !(near < prev[2] && prev[2] < far);
    if clear_x {
        pos[0] = if pos[0] >= prev[0] { left } else { right };
    }
    if clear_z {
        pos[2] = if pos[2] >= prev[2] { near } else { far };
    }
    if !clear_x && !clear_z {
        // Already inside: leave by the nearest side.
        let to_x = if pos[0] - left < right - pos[0] {
            left
        } else {
            right
        };
        let to_z = if pos[2] - near < far - pos[2] {
            near
        } else {
            far
        };
        if (to_x - pos[0]).abs() < (to_z - pos[2]).abs() {
            pos[0] = to_x;
        } else {
            pos[2] = to_z;
        }
    }
    true
}

/// Push along `normal` by `depth / |normal|²` of it, rounding away from the
/// surface so the body ends up clear of it.
fn push_along(pos: &mut [i32; 3], normal: [i64; 2], depth: i64) {
    let length = normal[0] * normal[0] + normal[1] * normal[1];
    if length == 0 {
        return;
    }
    let step = |axis: i64| {
        let scaled = axis * depth;
        let rounded = if scaled >= 0 {
            (scaled + length - 1) / length
        } else {
            (scaled - length + 1) / length
        };
        rounded as i32
    };
    pos[0] += step(normal[0]);
    pos[2] += step(normal[1]);
}

/// `Sca_ck_naname_a`..`d`: a right triangle filling one corner of the grown
/// box, cut by a diagonal; a body on the solid side leaves across it.
fn push_triangle(record: &ScaRect, shape: u8, pos: &mut [i32; 3], radius: i32) -> bool {
    let width = i64::from(record.width) + i64::from(radius) * 2;
    let depth = i64::from(record.depth) + i64::from(radius) * 2;
    let x = i64::from(pos[0]) - i64::from(record.x) + i64::from(radius);
    let z = i64::from(pos[2]) - i64::from(record.z) + i64::from(radius);
    // Each case is `solid > 0` on the solid side, scaled by the box's width.
    let (solid, normal) = match shape {
        1 => (z * width + depth * x - depth * width, [-depth, -width]),
        2 => (z * width - depth * x, [depth, -width]),
        3 => (depth * x - z * width, [-depth, width]),
        _ => (depth * width - depth * x - z * width, [depth, width]),
    };
    if solid <= 0 {
        return false;
    }
    push_along(pos, normal, solid + 1);
    true
}

/// `Sca_ck_hishi`: a diamond inscribed in the grown box.
fn push_diamond(record: &ScaRect, pos: &mut [i32; 3], radius: i32) -> bool {
    let half_x = i64::from(record.width) / 2 + i64::from(radius);
    let half_z = i64::from(record.depth) / 2 + i64::from(radius);
    let dx = i64::from(pos[0]) - i64::from(record.x) - i64::from(record.width) / 2;
    let dz = i64::from(pos[2]) - i64::from(record.z) - i64::from(record.depth) / 2;
    let solid = half_x * half_z - half_z * dx.abs() - half_x * dz.abs();
    if solid <= 0 {
        return false;
    }
    let sign = |value: i64| if value < 0 { -1 } else { 1 };
    push_along(pos, [sign(dx) * half_z, sign(dz) * half_x], solid + 1);
    true
}

/// Push radially out of a disc of `reach` around `centre`.
fn push_disc(pos: &mut [i32; 3], centre: [i32; 2], reach: i32) -> bool {
    let dx = i64::from(pos[0]) - i64::from(centre[0]);
    let dz = i64::from(pos[2]) - i64::from(centre[1]);
    let squared = dx * dx + dz * dz;
    if squared >= i64::from(reach) * i64::from(reach) {
        return false;
    }
    let distance = (squared as f64).sqrt();
    if distance < 1.0 {
        pos[2] = centre[1] + reach;
    } else {
        let scale = f64::from(reach) / distance;
        pos[0] = centre[0] + (dx as f64 * scale).round() as i32;
        pos[2] = centre[1] + (dz as f64 * scale).round() as i32;
    }
    true
}

/// `Sca_ck_circle`: a disc whose diameter is the record's width.
fn push_circle(record: &ScaRect, pos: &mut [i32; 3], radius: i32) -> bool {
    let half = i32::from(record.width) / 2;
    push_disc(
        pos,
        [i32::from(record.x) + half, i32::from(record.z) + half],
        half + radius,
    )
}

/// `Sca_hit_koban_x` and `_z`: a box with rounded ends along one axis.
fn push_capsule(record: &ScaRect, along_x: bool, pos: &mut [i32; 3], radius: i32) -> bool {
    let (x, z) = (i32::from(record.x), i32::from(record.z));
    let (width, depth) = (i32::from(record.width), i32::from(record.depth));
    let centre = if along_x {
        let half = depth / 2;
        [
            pos[0].clamp(x + half, (x + width - half).max(x + half)),
            z + half,
        ]
    } else {
        let half = width / 2;
        [
            x + half,
            pos[2].clamp(z + half, (z + depth - half).max(z + half)),
        ]
    };
    let half = if along_x { depth } else { width } / 2;
    push_disc(pos, centre, half + radius)
}

/// `Sca_hit_slope`: a body on the slope takes its height from its place along
/// it, and the slope's long sides block like a box.
fn push_slope(record: &ScaRect, prev: [i32; 3], pos: &mut [i32; 3], radius: i32) -> bool {
    let along_z = record.kind & 0x20 != 0;
    let (x, z) = (i32::from(record.x), i32::from(record.z));
    let (width, depth) = (i32::from(record.width), i32::from(record.depth));
    let (across_prev, across_low, across_high) = if along_z {
        (prev[0], x - radius, x + width + radius)
    } else {
        (prev[2], z - radius, z + depth + radius)
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
        pos[1] = i32::from(floor_of(pos[1] - 900)) * -FLOOR_HEIGHT;
    }
    false
}

/// The height of a slope record at `(x, z)` (`FUN_004e2930`).
fn slope_height(record: &ScaRect, x: i32, z: i32) -> i32 {
    let (left, near) = (i32::from(record.x), i32::from(record.z));
    let (width, depth) = (
        i32::from(record.width).max(1),
        i32::from(record.depth).max(1),
    );
    let rise = match (record.kind & 0x30) >> 4 {
        0 => (left - x + width) * FLOOR_HEIGHT / width,
        1 => (x - left) * FLOOR_HEIGHT / width,
        2 => (near - z + depth) * FLOOR_HEIGHT / depth,
        _ => (z - near) * FLOOR_HEIGHT / depth,
    };
    rise.clamp(0, FLOOR_HEIGHT) - i32::from((record.kind & 0x7C0) >> 6) * FLOOR_HEIGHT
}

/// The stairs a player at `pos` can climb with the action key, if any.
pub fn stairs_at(sca: &Sca, pos: [i32; 3], radius: i32) -> Option<&ScaRect> {
    let floor = floor_of(pos[1]);
    sca.records
        .iter()
        .find(|record| record.shape() == 12 && applies(sca, record, pos, radius + 1, floor))
}

/// Where climbing `stairs` from `pos` ends: past the far end along its axis
/// and on the floor at that end (`Sca_hit_stairs`, without the climb itself).
pub fn stairs_exit(stairs: &ScaRect, pos: [i32; 3], radius: i32) -> [i32; 3] {
    let low =
        -i32::from((stairs.kind >> 6) & 0x1F) * 0x12 * 100 - i32::from(stairs.kind >> 11) * 100;
    let mut bits = stairs.floor;
    let mut high = 0;
    while bits & 1 == 0 && bits != 0 {
        high -= FLOOR_HEIGHT;
        bits >>= 1;
    }
    let target = if floor_of(pos[1]) == floor_of(low) {
        high
    } else {
        low
    };
    let mut exit = pos;
    exit[1] = target;
    let (x, z) = (i32::from(stairs.x), i32::from(stairs.z));
    let (width, depth) = (i32::from(stairs.width), i32::from(stairs.depth));
    if (stairs.kind >> 4) & 2 == 0 {
        exit[0] = if pos[0] - x < width / 2 {
            x + width + radius + 1
        } else {
            x - radius - 1
        };
    } else {
        exit[2] = if pos[2] - z < depth / 2 {
            z + depth + radius + 1
        } else {
            z - radius - 1
        };
    }
    exit
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(hit.pos, [1500 - 500 - PLAYER_RADIUS, 0, 1500]);
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
        assert!(!half.hit);
    }

    #[test]
    fn stairs_block_and_climbing_them_changes_floor() {
        let mut stairs = record(12);
        stairs.floor = 2;
        stairs.kind = 0xF;
        let sca = room(vec![stairs]);
        let foot = [1000 - PLAYER_RADIUS - 1, -1800, 1500];
        let found = stairs_at(&sca, foot, PLAYER_RADIUS).unwrap();
        let exit = stairs_exit(found, foot, PLAYER_RADIUS);
        assert_eq!(exit[0], 2000 + PLAYER_RADIUS + 1);
        assert_eq!(exit[1], 0);
    }
}
