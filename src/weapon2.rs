//! RE2's handgun shots: `Em_ck`, `hit_scan_ck` and `Weapon_at_ck`.
//!
//! The player's routine finds its target here, from a view of the enemy slots
//! taken before it ran, and leaves a [`Hit`] for `zombie2` to take. The other
//! weapons aren't ported: `player2` has only the starting handguns.

use crate::player2::rotate;
use crate::sca::square_root0;
use crate::state::ScaRect;
use crate::zombie2::sca_ck_line;

/// What `Sca_ck_line` is asked for in both.
const LINE_MASK: u16 = 0x80;
/// `Type`'s bits for out of reach, and for someone only aimed at if nobody else is.
const TYPE_UNTARGETED: u16 = 0xC000;
const TYPE_ON_FLOOR: u16 = 0x2000;

/// What the player's routine sees of an enemy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Target {
    pub slot: usize,
    pub id: u8,
    pub be_flg: u32,
    pub kind: u16,
    pub life: i16,
    pub damage_cnt: u8,
    pub pos: [i32; 3],
    /// `Atd[0]`'s `X` and `Z`, and its `At_w`.
    pub at: [i32; 2],
    pub at_w: u16,
    /// `Atd[1]`'s `At_w`.
    pub at_w1: u16,
    /// `Workm.t[1]` of its neck.
    pub neck_y: i32,
}

impl Target {
    fn in_play(&self) -> bool {
        self.be_flg & 1 != 0
    }
}

/// What `Weapon_at_ck` does to the enemy it hits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hit {
    pub slot: usize,
    /// `Routine_1`: the player's `Type`.
    pub weapon: u8,
    /// Which record of the shot's three: the damage word's field.
    pub band: u8,
    /// Which of `D_life_d`, `D_life_c` and `D_life_u`.
    pub region: u8,
    pub damage: i16,
    /// What the region's count loses.
    pub stun: u8,
    /// `Damage_cnt`'s new count.
    pub count: u8,
    /// `Damage_flg`'s bits for what the shot crossed.
    pub flags: u16,
}

/// `Em_ck`: the slot to aim at, the nearest in view, but one on the floor only
/// when there's nobody else. A `range` of 0 is no limit.
pub fn em_ck(
    targets: &[Target],
    sca: &[ScaRect],
    pos: [i32; 3],
    neck: [i32; 3],
    range: u32,
) -> Option<usize> {
    let mut standing = (if range == 0 { i32::MAX as u32 } else { range }, None);
    let mut floored = (i32::MAX as u32, None);
    for target in targets.iter().filter(|target| target.in_play()) {
        if target.kind & TYPE_UNTARGETED != 0 || target.life < 0 {
            continue;
        }
        let (dx, dz) = (pos[0] - target.pos[0], pos[2] - target.pos[2]);
        let distance = square_root0(dz.wrapping_mul(dz).wrapping_add(dx.wrapping_mul(dx))) as u32;
        let to = [target.at[0], target.neck_y, target.at[1]];
        if sca_ck_line(sca, neck, to, LINE_MASK) {
            continue;
        }
        let best = if target.kind & TYPE_ON_FLOOR == 0 {
            &mut standing
        } else {
            &mut floored
        };
        if distance < best.0 {
            *best = (distance, Some(target.slot));
        }
    }
    standing.1.or(floored.1)
}

/// `hit_scan_data` for the handguns: how far under and over the shoulder the
/// hand is, which picks the level of the shot.
const HIT_SCAN: (i32, i32) = (-200, 300);
/// `DAT_0053a35c` for the handguns, by `pld_to_player_map`.
const SHOULDER: [i32; 2] = [-2427, -2120];

/// `hit_scan_ck`: `Be_flg`'s level of the shot, as `BYTE_0053a520` has it: 0 for
/// low, 1 level and 2 up. `y` is the hand's.
pub fn hit_scan_ck(id: u8, pos_y: i32, parts0_y: i32, y: i32) -> usize {
    let map = usize::from(id & 1);
    let shoulder = ((SHOULDER[map] + pos_y) * parts0_y) >> 12;
    if y < HIT_SCAN.0 + shoulder {
        2
    } else if HIT_SCAN.1 + shoulder < y {
        0
    } else {
        1
    }
}

/// A record of `At_tbl_top`: what each of its three rectangles may reach, and
/// `Attack_r_ck0`'s `x`, `z`, width and length of each.
struct Record {
    reach: [u8; 3],
    rects: [[i16; 4]; 3],
}

/// `At_tbl_top`'s handguns, by the shot's level.
const HANDGUN: [Record; 3] = [
    Record {
        reach: [0x4, 0x2, 0x1],
        rects: [
            [100, 0, 1000, 375],
            [4100, 0, 1000, 500],
            [8100, 0, 2500, 625],
        ],
    },
    Record {
        reach: [0x8, 0x8, 0x8],
        rects: [
            [200, 0, 1000, 375],
            [4200, 0, 1000, 500],
            [8200, 0, 7500, 625],
        ],
    },
    Record {
        reach: [0x10, 0x20, 0x40],
        rects: [
            [100, 0, 1000, 375],
            [4100, 0, 1000, 500],
            [8200, 0, 2500, 625],
        ],
    },
];

/// `Wp_dmg_tbl`'s entry for a weapon on a zombie: the damages by band, in ten
/// bits each; what the region loses in three bits each from the bottom, and
/// `Damage_cnt` in seven from the ninth; and the heights `Set_MinMax` reads.
struct Damage {
    damage: u32,
    stun: u32,
    heights: [i32; 6],
}

const HANDGUN_HEIGHTS: [i32; 6] = [-5000, -2000, -3000, 2000, -500, 3000];
/// `Type & 0xFFF` of 2, 3 and 4.
const DAMAGE: [Damage; 3] = [
    Damage {
        damage: 0x00E0_3C10,
        stun: 0x0285_0A14,
        heights: HANDGUN_HEIGHTS,
    },
    Damage {
        damage: 0x00E0_3C10,
        stun: 0x0285_0A14,
        heights: HANDGUN_HEIGHTS,
    },
    Damage {
        damage: 0x00E0_3C10,
        stun: 0x0081_0214,
        heights: HANDGUN_HEIGHTS,
    },
];

/// `W_at_ck`, read at three offsets: what a body's height band can be hit in.
const W_AT_CK: [u8; 0x14] = [4, 2, 1, 2, 1, 4, 1, 2, 4, 4, 2, 0, 2, 0, 0, 1, 2, 0, 0, 0];

/// `Set_MinMax`: the heights of a reach's lowest bit, and a third of them.
fn set_min_max(heights: &[i32; 6], reach: u8) -> (i32, i32, i32) {
    let bit = reach.trailing_zeros();
    let third = |low: i32, high: i32| (low - high) / 3;
    let [a, b, c, d, e, f] = *heights;
    match bit {
        1 => {
            let step = third(e, f);
            (e - step, f - step, step)
        }
        2 => (e, f, third(e, f)),
        3 => (c, d, third(c, d)),
        4 => (a, b, third(a, b)),
        5 => {
            let step = third(a, b);
            (a + step, b + step, step)
        }
        6 => {
            let step = third(a, b);
            (a + step * 2, b + step * 2, step)
        }
        _ => {
            let step = third(e, f);
            (e - step * 2, f - step * 2, step)
        }
    }
}

/// `Attack_r_ck0`: `at` is inside the rectangle of `rect`, which turns with the
/// shooter. The original scales by four to fit the GTE's sixteen bits.
fn attack_r_ck0(pos: [i32; 3], angle: u16, rect: [i16; 4], at: [i32; 2]) -> bool {
    let [x, z, width, length] = rect.map(i32::from);
    let (ox, oz) = rotate(angle, x, -(z + length * 4));
    let (origin_x, origin_z) = ((ox + pos[0]) >> 2, (oz + pos[2]) >> 2);
    let (wide_x, wide_z) = rotate(angle, width, 0);
    let (long_x, long_z) = rotate(angle, 0, length * 2);
    let clamp = |value: i32| value.clamp(-0x8000, 0x7FFF);
    let (ex, ez) = (at[0] >> 2, at[1] >> 2);
    let corners = [
        (ex - origin_x, ez - origin_z),
        (ex - (origin_x + wide_x), ez - (origin_z + wide_z)),
        (ex - (origin_x + long_x), ez - (origin_z + long_z)),
    ];
    let along =
        |axis: (i32, i32), corner: (i32, i32)| clamp((axis.0 * corner.0 + axis.1 * corner.1) >> 12);
    let width_axis = (wide_x, wide_z);
    let length_axis = (long_x, long_z);
    (along(width_axis, corners[0]) ^ along(width_axis, corners[1])) < 0
        && (along(length_axis, corners[0]) ^ along(length_axis, corners[2])) < 0
}

/// What `Weapon_at_ck` needs of the shooter.
pub struct Shooter {
    pub pos: [i32; 3],
    pub neck: [i32; 3],
    pub angle: u16,
    /// `Type & 0xFFF`, from 2 to 4.
    pub weapon: u8,
    /// [`hit_scan_ck`]'s level.
    pub level: usize,
}

/// `Weapon_at_ck`, for a handgun's one frame: the nearest enemy the shot
/// reaches. Its three rectangles run out from the gun and take in a body by
/// its width, once the height of the body picks a line of `W_AT_CK`.
pub fn weapon_at_ck(shooter: &Shooter, targets: &[Target], sca: &[ScaRect]) -> Option<Hit> {
    let table = DAMAGE.get(usize::from(shooter.weapon).checked_sub(2)?)?;
    let record = HANDGUN.get(shooter.level)?;
    let mut nearest = i32::MAX as u32;
    let mut found = None;
    let mut region = 0;
    for target in targets.iter().filter(|target| target.in_play()) {
        if target.damage_cnt != 0 || target.life < 0 || target.kind & TYPE_UNTARGETED != 0 {
            continue;
        }
        let to = [target.at[0], target.neck_y, target.at[1]];
        if sca_ck_line(sca, shooter.neck, to, LINE_MASK) {
            continue;
        }
        let height = target.pos[1] - shooter.pos[1];
        let stance = (target.be_flg >> 26 & 7) as u8;
        let mut result = 0u8;
        if stance != 0 {
            for (index, (&reach, rect)) in record.reach.iter().zip(&record.rects).enumerate() {
                if reach == 0 {
                    continue;
                }
                let (low, high, step) = set_min_max(&table.heights, reach);
                if (height.wrapping_sub(low) as u32) > (high.wrapping_sub(low) as u32) {
                    continue;
                }
                let mut line = 6;
                if high + step * 2 <= height {
                    line = 3;
                }
                if high + step <= height {
                    line = 0;
                }
                let line = line + usize::from(reach & 8 != 0) * 9;
                let mut rect = *rect;
                rect[3] += (target.at_w >> 2) as i16;
                if index == 0 {
                    rect[2] += (target.at_w1 >> 2) as i16;
                }
                if !attack_r_ck0(shooter.pos, shooter.angle, rect, target.at) {
                    continue;
                }
                result = 0;
                for offset in [2, 1, 0] {
                    let bits = W_AT_CK[line + offset];
                    if stance & bits != 0 {
                        region = bits >> 1;
                        result = region * 8 + (1 << index);
                    }
                }
            }
        }
        if result != 0 {
            let (dx, dz) = (shooter.pos[0] - target.at[0], shooter.pos[2] - target.at[1]);
            let distance =
                square_root0(dz.wrapping_mul(dz).wrapping_add(dx.wrapping_mul(dx))) as u32;
            if distance < nearest {
                nearest = distance;
                found = Some((target.slot, result, region));
            }
        }
    }
    let (slot, result, region) = found?;
    let band = if result & 4 != 0 {
        2
    } else {
        u8::from(result & 2 != 0)
    };
    Some(Hit {
        slot,
        weapon: shooter.weapon,
        band,
        region,
        damage: (table.damage >> (u32::from(band) * 10) & 0x3FF) as i16,
        stun: (table.stun >> (u32::from(band) * 3) & 7) as u8,
        count: (table.stun >> (u32::from(band) * 7 + 9) & 0x7F) as u8,
        flags: u16::from(result),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zombie(slot: usize, x: i32, z: i32) -> Target {
        Target {
            slot,
            id: 0x1F,
            be_flg: 1 | 0x400_0000 | 0x800_0000,
            life: 50,
            pos: [x, 0, z],
            at: [x, z],
            at_w: 500,
            neck_y: -1500,
            ..Target::default()
        }
    }

    #[test]
    fn the_hand_picks_the_shots_level() {
        assert_eq!(hit_scan_ck(0, 0, 0x1000, -2427), 1);
        assert_eq!(hit_scan_ck(0, 0, 0x1000, -2700), 2);
        assert_eq!(hit_scan_ck(0, 0, 0x1000, -2000), 0);
        assert_eq!(hit_scan_ck(1, 0, 0x1000, -2120), 1);
    }

    #[test]
    fn aiming_prefers_who_stands_and_then_who_is_nearest() {
        let targets = [zombie(0, 0, 3000), zombie(1, 0, 2000)];
        assert_eq!(em_ck(&targets, &[], [0; 3], [0; 3], 0), Some(1));
        let mut floored = targets;
        floored[1].kind = TYPE_ON_FLOOR;
        assert_eq!(em_ck(&floored, &[], [0; 3], [0; 3], 0), Some(0));
        floored[0].life = -1;
        assert_eq!(em_ck(&floored, &[], [0; 3], [0; 3], 0), Some(1));
        assert_eq!(em_ck(&targets, &[], [0; 3], [0; 3], 1000), None);
    }

    #[test]
    fn a_shot_finds_the_nearest_in_front_and_skips_who_is_recovering() {
        let shooter = Shooter {
            pos: [0; 3],
            neck: [0, -2000, 0],
            // Facing +x.
            angle: 0,
            weapon: 2,
            level: 1,
        };
        let mut targets = [zombie(0, 1600, 200), zombie(1, 900, 200)];
        let hit = weapon_at_ck(&shooter, &targets, &[]).expect("a hit");
        assert_eq!(hit.slot, 1);
        targets[1].damage_cnt = 3;
        assert_eq!(
            weapon_at_ck(&shooter, &targets, &[]).map(|hit| hit.slot),
            Some(0)
        );
        targets[0].pos[0] = -1600;
        targets[0].at[0] = -1600;
        assert_eq!(weapon_at_ck(&shooter, &targets, &[]), None);
    }
}
