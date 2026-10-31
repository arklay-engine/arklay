//! `Em_22`: the licker, on the floor.
//!
//! The walls and the ceiling (`Routine_1` 1, 7, 0xB, 0xD, 0x10-0x12, 0x15-0x17,
//! `Em22_Damage` 2 and 5, `Em22_Die` 2-4) need the body's `Super_matrix`, which
//! isn't ported: a licker the scripts start there doesn't spawn, and one that
//! would climb stays down (`docs/re2-deviations.md`).

use super::{Atd, Em, NO_BODY, PLAYER_BODY, square_root0};
use crate::player2::{self, Bank, direction_ck, goto};

/// `Be_flg`: no pushes, none given, no `Sca_ck_em`.
const BE_NO_OBA: u32 = 2;
const BE_NO_PUSH: u32 = 4;
const BE_NO_SCA: u32 = 8;
const BE_SUPER: u32 = 0x200_0000;

/// The tongue's part, which is hidden as its drawing isn't ported.
const TONGUE_PART: u32 = 0x16;

/// `+0x22c`: the shot that hurt it wasn't a surprise, it stands on its hind
/// legs, its feet make no sound, it fell off a wall, and it's in the air.
const FLAG_KNOWS: u16 = 1;
const FLAG_UP: u16 = 2;
const FLAG_QUIET: u16 = 4;
const FLAG_FALLING: u16 = 0x40;
const FLAG_NO_SCA: u16 = 0x100;
const FLAG_JUMPING: u16 = 0x200;
/// What `Em22_Damage` clears when the hit does more than hurt.
const FLAGS_KEPT: u16 = 0xFD0F;

/// `Em22_Life` and its easy table.
const LIFE: [i16; 16] = [
    87, 101, 81, 75, 110, 115, 99, 101, 101, 99, 81, 101, 115, 110, 87, 101,
];
const LIFE_EASY: [i16; 16] = [
    65, 65, 70, 70, 75, 69, 62, 90, 80, 75, 82, 97, 89, 101, 90, 100,
];

/// `WORD_00527c46`, by `Routine_3`: how far the tongue reaches as it goes out.
const TONGUE_REACH: [u32; 8] = [3500, 4600, 4500, 4200, 3500, 3000, 2500, 2000];

/// `DAT_00527c90`, by weapon: how many hits of blood, which points they pick
/// from from the front and from behind, the mask, and the effect.
const BLOOD: [(u8, u8, u8, u8, u8, u8, u16); 20] = [
    (0, 0, 0, 0, 0, 6, 0x2000),
    (0, 0, 12, 12, 0, 6, 0x2000),
    (1, 0, 12, 12, 0, 6, 0x2000),
    (1, 0, 12, 12, 0, 6, 0x2000),
    (1, 0, 12, 12, 0, 6, 0x2000),
    (1, 0, 12, 12, 0, 6, 0x2000),
    (1, 0, 12, 12, 0, 6, 0x2000),
    (5, 0, 0, 24, 0, 6, 0x2000),
    (8, 0, 0, 24, 0, 6, 0x2000),
    (4, 0, 12, 12, 5, 4, 0x1400),
    (6, 0, 0, 24, 5, 4, 0x1800),
    (4, 0, 0, 24, 4, 9, 0x2800),
    (0, 0, 12, 12, 0, 6, 0x2000),
    (1, 0, 12, 12, 0, 6, 0x2000),
    (0, 0, 12, 12, 6, 0, 0x1000),
    (1, 0, 0, 24, 0, 6, 0x2000),
    (6, 0, 0, 24, 5, 4, 0x1800),
    (1, 0, 8, 12, 5, 6, 0x2000),
    (1, 0, 8, 12, 0, 6, 0x2000),
    (1, 0, 8, 12, 0, 6, 0x2000),
];

/// `pVec`: the points on the body that bleed, and their parts.
const BLOOD_POINTS: [([i32; 3], usize); 24] = [
    ([-300, 0, 0], 7),
    ([0, -200, 0], 4),
    ([0, 200, 200], 6),
    ([0, -300, 0], 8),
    ([-300, 0, 0], 10),
    ([300, 100, 0], 11),
    ([0, 200, 200], 6),
    ([0, -300, 0], 13),
    ([300, -100, 0], 5),
    ([-200, -600, 0], 6),
    ([300, 400, 0], 12),
    ([0, 0, -100], 13),
    ([0, 0, -400], 0),
    ([-400, 0, -300], 0),
    ([-300, 0, 200], 2),
    ([-300, 0, -200], 2),
    ([-500, -300, 200], 2),
    ([-200, -600, -400], 2),
    ([-200, 600, -200], 14),
    ([0, 600, 200], 18),
    ([0, 0, 0], 16),
    ([0, 0, 0], 20),
    ([-400, 200, 0], 18),
    ([-100, 400, -200], 18),
];

/// `FUN_00466100`'s points on the player, `y` and `z`.
const BITE_POINTS: [[i32; 2]; 4] = [[-400, 400], [-300, 250], [-200, 100], [-100, -50]];

/// `DAT_00527ba8`: the entries of its sixteen that catch, one a tick.
const FLAMES: [(u8, [i32; 3], usize); 5] = [
    (4, [0, 100, 100], 6),
    (6, [-100, 0, -100], 15),
    (8, [-200, 0, 200], 1),
    (11, [-300, -600, -200], 2),
    (15, [0, 0, 0], 20),
];

/// `+0x23c`: what runs after each tick's move.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum After {
    #[default]
    Nothing,
    /// `FUN_00466100`: the jump's bite bleeds.
    Bite,
    /// `FUN_00467370`: a burning weapon's flames.
    Flames,
}

/// What `Em_22` keeps at the end of the work, and in its `Timer`s.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Licker {
    /// `Atd[1]`, the head.
    pub head: Atd,
    /// `+0x228`: it has someone to go after.
    target: bool,
    /// `+0x22c`.
    flags: u16,
    /// `+0x219`: 1 has heard, 2 knows where.
    aware: u8,
    /// `+0x21a`: how far along the tongue is.
    tongue: u8,
    /// `+0x240`.
    tongue_count: u8,
    /// `+0x22e`: the wait before the next attack.
    wait: u8,
    /// `+0x22f`: the weapon of the last hit.
    weapon: u8,
    /// `+0x230`: which way it's turning along a wall.
    turn: i8,
    /// `+0x232`: where a shot can aim.
    gun_area: u8,
    /// `+0x233`.
    count: u8,
    /// `+0x23a`: how far ahead of its chest `Sca_ck_em` checks.
    reach: i16,
    /// `+0x23c`.
    after: After,
    /// `+0x241`, `+0x242` and `+0x244`: the jump's slowing, gravity, and the
    /// height it left from.
    slowing: i8,
    gravity: i16,
    left_y: i32,
    /// `+0x21b`: its colour goes once it's dead.
    fading: u8,
    /// `Timer0`, `Timer1`.
    timer0: i16,
    timer1: i16,
    /// `FUN_00466100`'s turn: which way the bite that killed came from.
    bite_behind: bool,
    /// `Sca_old_x`, `Sca_old_z`.
    sca_old: [i32; 2],
    /// `Workm.t` of parts 0 and 1, which `Oba_set_ofs` reads from the last
    /// `Joint_trans2`.
    part0: [i32; 3],
    part1: [i32; 3],
    /// `Workm.t` of the part `Kage_work_set` follows.
    part2: [i32; 3],
    /// `Workm.t` of part 9, where `Em22_before` starts its drool.
    drool: [i16; 3],
}

/// `Em22_before`, which `Game_loop` calls before `Joint_trans2`: its drool is
/// always a draw while it's alive.
pub fn before(
    licker: &mut Licker,
    life: i16,
    be_flg: &mut u32,
    rnd: &mut super::Rnd,
    effects: &mut crate::esp2::Effects,
) {
    if licker.fading == 1 {
        *be_flg |= BE_NO_OBA | BE_NO_SCA;
        licker.timer1 = 0;
        licker.fading = 2;
    }
    if life > 0 && rnd.draw() & 0x1F == 0 {
        effects.call(0, 0x15, 0x800, 0, licker.drool, false);
    }
}

impl Licker {
    /// Where `MulKage` puts the shadow, `Off_x` ahead of the third part.
    pub fn shadow_at(&self, angle: u16) -> [i32; 3] {
        let (x, z) = player2::rotate(angle, 0xFA, 0);
        [self.part2[0] + x, self.part2[1], self.part2[2] + z]
    }
}

/// `Em_22`.
pub(super) fn run(em: &mut Em<'_, '_, '_>, licker: &mut Licker) {
    Em22 { em, l: licker }.run();
}

struct Em22<'e, 'a, 'b, 'c> {
    em: &'e mut Em<'a, 'b, 'c>,
    l: &'e mut Licker,
}

impl Em22<'_, '_, '_, '_> {
    fn run(&mut self) {
        self.em.root_ck();
        if self.em.world.stopped {
            return;
        }
        if self.em.z.damage_cnt & 0x7F != 0 {
            self.em.z.damage_cnt -= 1;
        }
        self.l.wait = self.l.wait.saturating_sub(1);
        match self.em.work().routine[0] {
            0 => self.init(),
            1 | 4 => self.mv(),
            2 => self.damage(),
            3 => self.die(),
            _ => {}
        }
        self.gunarea_chg();
        self.tongue_move();
        if self.l.flags & FLAG_QUIET == 0 {
            let seq = self.em.mover.now_seq();
            if seq & super::SEQ_SOUND != 0 {
                self.em.z.sounds.push((seq >> 28) as u8);
            }
        }
        match self.l.after {
            After::Nothing => {}
            After::Bite => self.bite_blood(),
            After::Flames => self.flames(),
        }
        self.em.z.last_routine = self.em.work().routine;
        self.oba_and_sca();
        self.joint_trans();
    }

    /// `Em22_Init`.
    fn init(&mut self) {
        self.em.z.damage_flg = 0;
        self.em.z.damage_cnt = 0;
        self.em.clip(1, 0, 0);
        self.joint(0x100);
        *self.em.be_flg |= BE_SUPER;
        self.em.work().neck.flags = 0;
        *self.em.world.tongue_owner = 0;
        let table = if self.em.world.easy { LIFE_EASY } else { LIFE };
        let down = match (self.em.world.easy, self.em.world.common_a4) {
            (_, false) => 0,
            (false, true) => 10,
            (true, true) => 20,
        };
        self.em.z.life = table[usize::from(self.em.rnd() & 0xF)] - down;
        self.l.target = true;
        self.l.reach = -500;
        self.em.work().hidden_parts |= 1 << TONGUE_PART;
        *self.em.be_flg &= !(BE_NO_OBA | BE_NO_SCA);
        self.em.work().routine = [1, 0, 0, 0];
        self.atd_set();
        *self.em.be_flg = (*self.em.be_flg & 0xEFFF_FFFF) | 0xC00_0000;
        let pos = self.em.mover.player.pos;
        self.l.part0 = pos;
        self.l.part1 = pos;
    }

    /// `Em22_Atd_set_1`.
    fn atd_set(&mut self) {
        self.em.z.atd = Atd {
            ofs_y: 0,
            at_w: 0x250,
            at_h: 600,
            w: 1000,
            at: [0; 3],
        };
        self.l.head = Atd {
            ofs_y: -0x180,
            at_w: 0x180,
            at_h: 0x180,
            w: 0x180,
            at: [0; 3],
        };
    }

    /// `Gunarea_chg`.
    fn gunarea_chg(&mut self) {
        let shotgun = matches!(self.em.world.weapon, 7 | 8);
        let be_flg = &mut *self.em.be_flg;
        *be_flg = match self.l.gun_area {
            0 => (*be_flg & 0xF3FF_FFFF) | 0x1000_0000,
            1 => (*be_flg & 0xFBFF_FFFF) | 0x1800_0000,
            2 => (*be_flg & 0xEBFF_FFFF) | 0x800_0000,
            3 => (*be_flg & 0xEFFF_FFFF) | 0xC00_0000,
            4 if shotgun => (*be_flg & 0xEFFF_FFFF) | 0xC00_0000,
            4 => (*be_flg & 0xE7FF_FFFF) | 0x400_0000,
            5 => *be_flg & 0xE3FF_FFFF,
            _ => *be_flg,
        };
    }

    /// `tongue_move`, without its shape.
    fn tongue_move(&mut self) {
        let l = &mut *self.l;
        match l.tongue {
            1 => {
                l.tongue_count = 1;
                l.tongue = 2;
            }
            2 => {
                l.tongue_count += 1;
                if l.tongue_count == 0x11 {
                    l.tongue = 3;
                    l.tongue_count = 0;
                }
            }
            3 => {
                // Two ticks of `+0x800`.
                l.tongue_count += 1;
                if l.tongue_count == 2 {
                    l.tongue = 4;
                    l.tongue_count = 0;
                }
            }
            4 => {
                // Four of `+0x400`.
                l.tongue_count += 1;
                if l.tongue_count == 4 {
                    l.tongue = 5;
                    l.tongue_count = 4;
                }
            }
            5 => {
                l.tongue_count += 1;
                if l.tongue_count == 11 {
                    l.tongue = 6;
                    l.tongue_count = 0;
                }
            }
            6 => {
                l.tongue = 0;
                *self.em.world.tongue_owner = 0;
            }
            _ => {}
        }
    }

    /// The end of `Em_22`: the pushes from `Atd`, which it sets from the parts
    /// and not from the body.
    fn oba_and_sca(&mut self) {
        let at = self.l.part0;
        self.oba_set_ofs(at);
        self.em.z.at_em_no = NO_BODY;
        for other in self.em.world.bodies {
            let body = &mut self.em.mover.player.pos;
            let be_flg = *self.em.be_flg;
            if super::oba_hit(other, be_flg, &mut self.em.z.atd, body) {
                self.em.z.at_em_no = other.slot;
            }
            if super::oba_hit(other, be_flg, &mut self.l.head, body) {
                self.em.z.at_em_no = other.slot;
            }
        }
        let (x, z) = player2::rotate(self.em.mover.player.angle, i32::from(self.l.reach), 0);
        let chest = self.l.part1;
        self.oba_set_ofs([chest[0] + x, chest[1], chest[2] + z]);
        if self.l.flags & FLAG_NO_SCA == 0 {
            self.sca_ck_em();
        } else {
            self.l.flags &= !FLAG_NO_SCA;
        }
        let at = self.em.z.atd.at;
        self.l.sca_old = [at[0], at[2]];
        self.oba_set_ofs(self.l.part0);
        let body = &mut *self.em.mover.player;
        body.collision_mirror = [body.pos[0], body.pos[2]];
    }

    /// `Oba_set_ofs`: both boxes, the first reaching forward from its middle.
    fn oba_set_ofs(&mut self, at: [i32; 3]) {
        let angle = self.em.mover.player.angle;
        let (x, z) = player2::rotate(angle, 0x2A0, 0);
        self.em.z.atd.at = [at[0] + x, at[1], at[2] + z];
        let (x, z) = player2::rotate(angle, -0x60, 0);
        self.l.head.at = [at[0] + x, at[1] + i32::from(self.l.head.ofs_y), at[2] + z];
    }

    /// `Sca_ck_em`, on `Atd[0]`.
    fn sca_ck_em(&mut self) {
        self.em.z.sca_info = 0;
        let room = self.em.mover.tick.room;
        if *self.em.be_flg & BE_NO_SCA != 0 {
            return;
        }
        let Some(sca) = &room.sca else {
            return;
        };
        let at = self.em.z.atd.at;
        let [old_x, old_z] = self.l.sca_old;
        let prev = [old_x, self.em.mover.player.pos[1], old_z];
        let radius = i32::from(self.em.z.atd.w);
        let floor = crate::sca::floor_of(prev[1]);
        let resolved = crate::sca::resolve_on_floor(sca, super::MASK, prev, at, radius, floor);
        let (dx, dz) = (resolved.pos[0] - at[0], resolved.pos[2] - at[2]);
        let body = &mut *self.em.mover.player;
        body.pos[0] += dx;
        body.pos[2] += dz;
        self.em.z.atd.at[0] += dx;
        self.em.z.atd.at[2] += dz;
        self.em.z.sca_info = u32::from(resolved.hit);
    }

    /// What `Joint_trans2` leaves for the next tick.
    fn joint_trans(&mut self) {
        let t = |em: &Em<'_, '_, '_>, part| em.mover.part_matrix(part).map(|matrix| matrix.t);
        let pos = self.em.mover.player.pos;
        self.l.part0 = t(self.em, 0).unwrap_or(pos);
        self.l.part1 = t(self.em, 1).unwrap_or(pos);
        self.l.part2 = t(self.em, 2).unwrap_or(pos);
        self.l.drool = t(self.em, 9).unwrap_or(pos).map(|axis| axis as i16);
        self.em.work().neck.position = t(self.em, 8);
    }

    /// `FUN_00466100`.
    fn bite_blood(&mut self) {
        let count = self.l.count;
        if count & 1 == 0 {
            let [y, z] = BITE_POINTS[usize::from(count >> 1)];
            let turn = if self.l.bite_behind { 0x800 } else { 0 };
            let (x, z) = player2::rotate(turn, 0x15E, z);
            let at = self.em.world.pl.root.as_ref().map(|root| {
                let point = [x, y, z];
                std::array::from_fn(|axis| {
                    let turned: i32 = (0..3)
                        .map(|index| i32::from(root.m[axis][index]) * point[index])
                        .sum();
                    (root.t[axis] + (turned >> 12)) as i16
                })
            });
            if let Some(at) = at {
                self.em.world.effects.call(0, 2, 0x2400, 0, at, false);
            }
        }
        self.l.count += 1;
        if self.l.count == 7 {
            self.l.after = After::Nothing;
        }
    }

    /// `FUN_00467370`: the first tick only sets it going.
    fn flames(&mut self) {
        let count = self.l.count;
        if count != 0 {
            let entry = FLAMES.iter().find(|flame| flame.0 == (count - 1) % 16);
            if let Some(&(_, point, part)) = entry {
                let size = u16::from(self.em.rnd()) * 2 + 0x1000;
                self.em.effect(6, 0, size, 0, part, point);
            }
            if count == 0x10 {
                self.l.after = After::Nothing;
                self.l.count = 0x11;
                return;
            }
        }
        self.l.count += 1;
    }

    /// `FUN_004671f0`: the blood of a hit.
    fn hit_blood(&mut self, behind: bool) {
        let table = BLOOD
            .get(usize::from(self.l.weapon))
            .copied()
            .unwrap_or(BLOOD[0]);
        let (count, front, back, mask, kind, row, size) = table;
        let from = if behind { back } else { front };
        for _ in 0..count {
            let pick = self.em.rnd() & mask.wrapping_sub(1);
            let (point, part) = BLOOD_POINTS[usize::from(pick + from)];
            self.em.effect(kind, row, size, 0, part, point);
        }
        self.l.count = 0;
        if self.l.weapon == 0xE {
            self.l.after = After::Flames;
        }
    }

    fn joint_reversed(&mut self, late: u16, reverse: bool) -> bool {
        self.em.work().bank = Bank::Pld;
        self.em.mover.joint_move_late_reversed(late, reverse)
    }

    fn joint(&mut self, late: u16) -> bool {
        self.joint_reversed(late, false)
    }

    fn spd(&mut self) {
        self.em.spd(Bank::Pld);
    }

    /// `Add_speedXYZsuper` on the floor.
    fn add_speed_xyz(&mut self) {
        self.em.mover.add_speed_xyz();
    }

    /// `Mot_play2`.
    fn mot_play(&mut self, clip: u8, hokan: u8, reverse: bool) -> bool {
        if self.em.work().move_no != clip {
            self.em.clip(clip, 0, hokan);
        }
        let late = (0x1000 / (u32::from(hokan) + 1)) as u16;
        self.joint_reversed(late, reverse)
    }

    /// `Mot_play2_sp`.
    fn mot_play_sp(&mut self, clip: u8, hokan: u8) -> bool {
        if self.em.work().move_no != clip {
            self.em.clip(clip, 0, hokan);
        }
        self.spd();
        let late = (0x1000 / (u32::from(hokan) + 1)) as u16;
        if self.joint(late) {
            return true;
        }
        self.add_speed_xyz();
        false
    }

    fn r2(&mut self) -> u8 {
        self.em.work().routine[2]
    }

    fn set_r2(&mut self, value: u8) {
        self.em.work().routine[2] = value;
    }

    fn r3(&mut self) -> u8 {
        self.em.work().routine[3]
    }

    fn set_r3(&mut self, value: u8) {
        self.em.work().routine[3] = value;
    }

    fn go(&mut self, state: u8) {
        self.em.state(state, 0);
    }

    fn player_life(&self) -> i16 {
        *self.em.world.player_life
    }

    /// `FUN_00463490`: whether it notices the player, who is the only target.
    fn spots(&mut self) -> bool {
        if self.player_life() < 0 {
            return false;
        }
        if self.em.work().routine[0] == 7 {
            self.l.aware = 0;
            return false;
        }
        if self.l.flags & FLAG_KNOWS != 0 {
            return true;
        }
        if self.touching() {
            self.l.aware = 1;
            return true;
        }
        let noise = self.em.world.noise as u8;
        let heard = match branch(self.em.distance, 3000, 9000, 0x238C) {
            0 => noise & 0x17,
            _ => noise & 5,
        };
        if heard == 0 {
            return false;
        }
        self.l.aware = 1;
        true
    }

    /// `FUN_004641f0`.
    fn spots_close(&mut self) -> bool {
        if self.player_life() < 0 {
            return false;
        }
        if self.em.work().routine[0] == 7 {
            self.l.aware = 0;
            return false;
        }
        if self.l.flags & FLAG_KNOWS != 0 {
            self.l.aware = 1;
            return true;
        }
        if self.touching() {
            self.l.aware = 2;
            return true;
        }
        let noise = self.em.world.noise as u8;
        let noticed = match branch(self.em.distance, 3000, 5000, 7000) {
            0 => self.em.world.pl.routine[1] != 0,
            1 => noise & 0x17 != 0,
            _ => noise & 5 != 0,
        };
        if noticed {
            self.l.aware = 2;
        }
        noticed
    }

    /// `At_em_no` is the player or their partner.
    fn touching(&self) -> bool {
        let at = self.em.z.at_em_no;
        at == PLAYER_BODY || at == crate::scd2::host::SPLAYER_SLOT as u8
    }

    /// `FUN_00463540`: turn away from what's ahead.
    fn turn_at_wall(&mut self, step: i8) {
        let body = &*self.em.mover.player;
        let (x, z) = player2::rotate(body.angle, 0x708, 0);
        let ahead = [body.pos[0] + x, body.pos[1], body.pos[2] + z];
        let [here_x, _, here_z] = body.pos;
        let angle = body.angle;
        let floor = self.em.z.floor;
        let hit = self
            .em
            .mover
            .tick
            .room
            .sca
            .as_ref()
            .and_then(|sca| crate::sca::first_hit(sca, super::MASK, ahead, 400, floor));
        let Some(record) = hit else {
            self.l.turn = 0;
            return;
        };
        if self.l.turn == 0 {
            let along = (here_x - i32::from(record.x)) * i32::from(record.depth)
                / i32::from(record.width.max(1));
            let over = here_z - i32::from(record.z);
            let beyond = over - i32::from(record.depth);
            let side = if over <= along {
                if -beyond == along || -along < beyond {
                    0
                } else {
                    1
                }
            } else if -along < beyond {
                3
            } else {
                2
            };
            let mut off = i32::from(angle & 0xFFF) - (((side - 2) * 0x400) & 0xFFF);
            if off.abs() > 0x800 {
                off -= 0x1000;
            }
            self.l.turn = if off == 0 { -step } else { step };
        }
        let body = &mut *self.em.mover.player;
        body.angle = body.angle.wrapping_add_signed(i16::from(self.l.turn));
    }

    fn goto_player(&mut self, rate: i32) {
        self.em.goto_player(rate);
    }

    fn distance_now(&self) -> i32 {
        let body = self.em.mover.player.pos;
        let [x, _, z] = self.em.world.player;
        let (dx, dz) = (x - body[0], z - body[2]);
        square_root0(dx.wrapping_mul(dx).wrapping_add(dz.wrapping_mul(dz)))
    }

    /// `Em22_Move`.
    fn mv(&mut self) {
        match self.em.work().routine[1] {
            0 => self.idle(),
            2 => self.turn_to(),
            3 => self.get_up(),
            4 => self.get_down(),
            5 => self.hesitate(),
            6 => self.back_off(),
            8 => self.tongue_attack(),
            9 => self.slash(),
            0xA => self.jump(),
            0xC => self.stalk(),
            0xE => self.listen(),
            0xF => self.decide(),
            0x13 => self.chase(),
            0x14 => self.run_at(),
            // A wall or the ceiling, which isn't ported: it stays and listens.
            _ => self.go(0xE),
        }
    }

    fn idle(&mut self) {
        self.l.gun_area = 4;
        self.l.target = self.spots();
        if self.l.target {
            match self.l.aware {
                1 => self.go(0xE),
                2 => self.go(0xF),
                _ => {}
            }
            return;
        }
        let mut r2 = self.r2();
        if matches!(r2, 0 | 3) {
            let r3 = self.r3() + 1;
            self.l.flags |= FLAG_UP;
            self.l.turn = 0;
            self.em.clip(3, 0, 0x2F);
            self.set_r3(r3);
            if r3 < 2 {
                self.set_r2(1);
                r2 = 1;
            } else {
                self.set_r2(4);
                self.set_r3(0);
                self.em.clip(1, 0, 0x1F);
                r2 = 4;
            }
        }
        match r2 {
            4 => {
                self.turn_at_wall(0xC);
                self.spd();
                if self.joint(0x80) && self.em.rnd() & 3 == 0 {
                    self.set_r2(0);
                    self.set_r3(0);
                }
                self.add_speed_xyz();
            }
            1 => {
                self.spd();
                if self.joint(0x55) {
                    self.set_r2(3);
                }
                self.em.mover.add_speed_xz();
            }
            2 => {
                let ended = self.joint_reversed(0x400, true);
                self.em.step(ended);
            }
            _ => {}
        }
    }

    fn turn_to(&mut self) {
        let mut r2 = self.r2();
        if r2 == 0 {
            self.l.flags &= !FLAG_UP;
            self.em.clip(2, 0, 0xF);
            self.l.gun_area = 3;
            self.set_r2(1);
            r2 = 1;
        }
        if r2 == 1 {
            self.goto_player(0x60);
            self.spd();
            let spd = &mut self.em.work().spd;
            spd[0] /= 4;
            spd[2] /= 4;
            let ended = self.joint(0x100);
            self.em.step(ended);
            self.add_speed_xyz();
            return;
        }
        if r2 == 2 {
            self.em.clip(4, 0, 0xF);
            self.set_r2(3);
            self.set_r3(0);
            r2 = 3;
        }
        if r2 == 3 {
            let r3 = self.r3() + 1;
            self.set_r3(r3);
            self.joint(0x100);
            if r3 > 4 {
                self.go(0xF);
            }
        }
    }

    fn get_up(&mut self) {
        match self.r2() {
            0 => {
                self.l.flags |= FLAG_UP;
                self.em.clip(6, 0, 0xF);
                self.l.gun_area = 4;
                self.set_r2(1);
            }
            1 => {}
            _ => return,
        }
        if self.l.target {
            self.goto_player(0x20);
        }
        if self.joint_reversed(0x100, true) {
            self.l.flags &= !FLAG_UP;
            self.l.gun_area = 3;
            self.go(5);
        }
    }

    fn get_down(&mut self) {
        match self.r2() {
            0 => {
                self.l.flags |= FLAG_UP;
                self.em.clip(5, 0, 0xF);
                self.l.gun_area = 4;
                self.set_r2(1);
            }
            1 => {}
            _ => return,
        }
        if self.joint(0x100) {
            self.go(0);
        }
    }

    fn hesitate(&mut self) {
        let r2 = self.r2();
        let mut waiting = r2 == 1;
        if r2 == 0 {
            self.l.flags &= !FLAG_UP;
            self.l.gun_area = 3;
            if !self.l.target {
                self.em.clip(4, 0, 0xF);
                self.set_r2(1);
                waiting = true;
            }
        } else if r2 > 2 {
            return;
        }
        if waiting {
            self.l.target = self.spots();
            let r3 = self.r3() + 1;
            self.set_r3(r3);
            self.joint(0x100);
            if !self.l.target {
                if r3 >= 0x79 {
                    self.go(4);
                    self.em.clip(2, 0, 0xF);
                }
                return;
            }
        }
        if r2 != 2 {
            self.em.clip(2, 0, 0xF);
            self.set_r2(2);
        }
        self.hesitate_step();
    }

    /// `LAB_00463918`.
    fn hesitate_step(&mut self) {
        let mut turn = 1;
        if self.em.turn_to_player(0x200) == 0 {
            turn = if self.distance_now() < 4000 {
                self.em.turn_to_player(0x40)
            } else {
                let turn = self.em.turn_to_player(0x10);
                let r3 = self.r3() + 1;
                self.set_r3(r3);
                turn
            };
            let body = &mut *self.em.mover.player;
            body.angle = body.angle.wrapping_add_signed(turn as i16);
            let spd = &mut self.em.work().spd;
            spd[0] = -40;
            spd[2] = 0;
            self.em.mover.add_speed_xz();
        } else {
            self.go(0x13);
        }
        self.joint(0x100);
        if turn == 0 {
            self.go(2);
        }
        if self.r3() > 0x20 {
            self.go(5);
        }
    }

    fn back_off(&mut self) {
        match self.r2() {
            0 => {
                self.l.flags |= FLAG_UP;
                self.em.clip(1, 0, 0xF);
                self.l.gun_area = 4;
                self.set_r2(1);
            }
            1 => {}
            _ => return,
        }
        if self.l.wait == 0 && self.em.distance < 2000 {
            if self.em.turn_to_player(0x300) == 0 {
                self.go(9);
            } else {
                self.go(2);
            }
            return;
        }
        if self.em.distance > 4000 {
            if self.player_life() > 99 {
                if self.em.rnd() & 7 < 3 {
                    self.go(0xA);
                } else if self.em.rnd() & 2 == 0 {
                    self.go(8);
                } else {
                    self.go(0x14);
                }
            }
            if self.player_life() < 100 {
                if self.em.rnd() & 7 < 2 {
                    self.go(0xA);
                } else {
                    self.go(0x14);
                }
            }
        }
        self.em.work().bank = Bank::Pld;
        self.em.mover.spd_get_reversed(true);
        self.em.mover.add_speed_xz();
        self.joint_reversed(0x100, true);
        if self.em.work().move_cnt == 0x2E {
            self.go(0xF);
        }
    }

    fn tongue_attack(&mut self) {
        let mut r2 = self.r2();
        if r2 == 0 {
            self.set_r3(0);
            if self.l.flags & FLAG_UP != 0 {
                self.goto_player(0x10);
                let ended = self.mot_play(6, 0xF, true);
                self.em.step(ended);
                r2 = 0xFF;
            } else {
                self.set_r2(1);
                r2 = 1;
            }
        }
        if r2 == 1 {
            if *self.em.world.tongue_owner == 0 {
                *self.em.world.tongue_owner = self.em.world.slot as u8 + 2;
                self.l.flags &= !FLAG_UP;
                self.l.tongue = 1;
                self.em.clip(7, 0, 3);
                self.l.gun_area = 3;
                self.l.timer0 = 0;
                self.set_r2(2);
                self.set_r3(0);
                r2 = 2;
            } else {
                self.go(0xF);
                r2 = 0xFF;
            }
        }
        if r2 == 2 {
            self.spd();
            self.goto_player(8);
            let ended = self.joint(0x400);
            self.em.step(ended);
            self.em.mover.add_speed_xz();
            if self.em.work().move_cnt == 0x11 {
                self.em.effect(0, 0x14, 0x1800, 0, 9, [200, 0, 0]);
                self.em.z.sounds.push(5);
            }
        }
        if r2 == 3 && self.l.tongue == 0 {
            self.go(0xF);
        }
        let r3 = self.r3().wrapping_add(1);
        self.set_r3(r3);
        if r3 > 0x11
            && r3 < 0x1A
            && self.l.timer0 == 0
            && self.em.world.pl.damage_cnt == 0
            && self.em.turn_to_player(0x100) == 0
            && (self.em.distance as u32) < TONGUE_REACH[usize::from(r3 - 0x12)]
        {
            self.hurt_player(0x19);
        }
    }

    /// The tongue's and the slash's end: `Pl_damage`, `Routine_1` 0 or 1.
    fn hurt_player(&mut self, down: i32) {
        let behind = self.em.cdir_player();
        self.em.world.pl.held_by = Some(self.em.world.slot);
        self.em.world.pl.routine = [2, u8::from(behind), 0, 0];
        let result = self.em.pl_life_down(down, false);
        if result & 2 != 0 {
            self.em.world.pl.routine = [3, 0, 0, 0];
        }
        self.l.wait = 0x28;
        self.l.timer0 = 1;
    }

    fn slash(&mut self) {
        let r2 = self.r2();
        if r2 == 0 {
            self.set_r3(0);
            if self.l.flags & FLAG_UP == 0 {
                let ended = self.mot_play(6, 0xF, false);
                self.em.step(ended);
                return;
            }
            self.set_r2(1);
        } else if r2 > 2 {
            return;
        }
        if self.r2() == 1 {
            self.l.flags |= FLAG_UP;
            self.em.clip(0x15, 0, 3);
            self.l.gun_area = 4;
            self.set_r2(2);
            self.l.timer0 = 0;
        }
        self.spd();
        self.goto_player(8);
        if self.joint(0x400) {
            self.go(0xF);
        }
        self.em.mover.add_speed_xz();
        let r3 = self.r3().wrapping_add(1);
        self.set_r3(r3);
        if self.em.world.pl.damage_cnt == 0
            && self.l.timer0 == 0
            && (2..=6).contains(&r3)
            && self.em.turn_to_player(0x154) == 0
            && self.em.distance <= 0xAEF
            && self.player_life() >= 0
        {
            let at = self.em.mover.part_point(13, [0, 200, 0]);
            if let Some(at) = at {
                let at = at.map(|axis| axis as i16);
                self.em.world.effects.call(0, 0, 0x2400, 0, at, false);
            }
            self.hurt_player(0xF);
        }
    }

    fn jump(&mut self) {
        let mut r2 = self.r2();
        if r2 == 0 {
            self.l.left_y = self.em.mover.player.pos[1];
            self.em.work().spd = [0; 3];
            self.l.slowing = 0;
            self.l.gravity = 0;
            self.l.timer0 = 0;
            if self.l.flags & FLAG_UP != 0 {
                self.em.clip(6, 0, 3);
                self.set_r2(1);
                r2 = 1;
            } else {
                self.set_r2(2);
                r2 = 2;
            }
        }
        if r2 == 1 {
            let ended = self.joint_reversed(0x400, true);
            self.em.step(ended);
        }
        if r2 == 2 {
            self.l.flags &= !FLAG_UP;
            self.l.gun_area = 3;
            self.l.slowing = -10;
            self.l.gravity = 0;
            self.em.z.sounds.push(8);
            self.set_r2(3);
            r2 = 3;
        }
        if r2 == 3 {
            let ended = self.mot_play(0x1F, 0, false);
            self.em.step(ended);
            self.goto_player(6);
        }
        if r2 == 4 {
            self.l.flags |= FLAG_JUMPING;
            let spd = &mut self.em.work().spd;
            spd[0] = 0x226;
            spd[1] = -0xF5;
            self.l.slowing = -8;
            self.l.gravity = 0x23;
            self.em.mover.player.pos[1] -= 0x72F;
            self.set_r2(5);
            r2 = 5;
        }
        if r2 == 5 {
            let ended = self.mot_play(0x20, 0, false);
            self.em.step(ended);
            self.goto_player(0x20);
            let count = self.em.work().move_cnt;
            let angle = self.angle_to_player();
            if self.em.world.pl.damage_cnt == 0
                && (0xB..=0xE).contains(&count)
                && (-199..=0x2C7).contains(&angle)
                && self.em.distance <= 0x897
                && self.l.timer0 == 0
            {
                self.bite();
            }
        }
        if r2 == 6 {
            self.em.clip(0x20, 0x1C, 0);
            if self.em.mover.player.pos[1] > self.l.left_y - 0x60F {
                self.l.flags &= !FLAG_JUMPING;
                self.l.slowing = -60;
                self.em.mover.player.pos[1] = self.l.left_y;
                self.em.work().spd[1] = 0;
                self.l.gravity = 0;
                self.set_r2(7);
                r2 = 7;
            }
        }
        if r2 == 7 && self.mot_play(0x21, 0, false) {
            self.go(0xF);
        }
        self.em.mover.player.pos[1] += i32::from(self.em.work().spd[1]);
        self.em.mover.add_speed_xz();
        let slowing = i16::from(self.l.slowing);
        let gravity = self.l.gravity;
        let spd = &mut self.em.work().spd;
        spd[0] = spd[0].wrapping_add(slowing);
        spd[1] = spd[1].wrapping_add(gravity);
        if spd[0] < 0 {
            spd[0] = 0;
            self.l.slowing = 0;
        }
    }

    /// The jump's bite, which takes the most of anything.
    fn bite(&mut self) {
        self.l.after = After::Bite;
        self.l.count = 0;
        self.l.timer0 = 1;
        self.em.world.pl.held_by = Some(self.em.world.slot);
        let down = if self.player_life() > 0x6E { 100 } else { 0x50 };
        self.em.pl_life_down(down, false);
        self.em.z.sounds.push(10);
        let behind = self.em.cdir_player();
        if self.player_life() < 0 {
            self.l.bite_behind = behind;
            self.em.world.pl.routine = [3, 0, 0, 0];
            *self.em.world.player_life = -1;
        } else {
            if self.player_life() < 0x28 {
                *self.em.world.player_life = 0x28;
            }
            self.em.world.pl.routine = [2, 2 + u8::from(behind), 0, 0];
        }
    }

    /// `FUN_0047dbb0`.
    fn angle_to_player(&self) -> i32 {
        let body = self.em.mover.player.pos;
        let [x, _, z] = self.em.world.player;
        let to = direction_ck(body[0], body[2], x, z);
        let off = to.wrapping_sub(self.em.mover.player.angle) & 0xFFF;
        if off > 0x800 {
            i32::from(off) - 0x1000
        } else {
            i32::from(off)
        }
    }

    fn stalk(&mut self) {
        let mut r2 = self.r2();
        if r2 == 0 {
            self.l.flags |= FLAG_UP;
            self.em.clip(6, 5, 3);
            self.l.gun_area = 4;
            self.set_r2(1);
            self.set_r3(0);
            r2 = 1;
        }
        match r2 {
            1 => {
                if self.joint(0x400) {
                    self.em.clip(0, 0, 0);
                    self.set_r2(2);
                    self.set_r3(0);
                }
                self.goto_player(0x40);
            }
            2 => self.stalk_on(),
            3 => {
                let r3 = self.r3() + 1;
                self.set_r3(r3);
                if r3 > 5 {
                    self.em.clip(0, 0, 0);
                    self.set_r2(2);
                    self.set_r3(0);
                }
            }
            _ => {}
        }
    }

    fn stalk_on(&mut self) {
        if self.em.distance < 0x9C4 {
            self.go(0xF);
        }
        self.goto_player(0x40);
        self.spd();
        if self.joint(0x1000) {
            if self.em.distance > 8000 {
                self.l.aware = 1;
                self.go(0);
                return;
            }
            if self.em.distance < 2000 {
                self.go(0xF);
                self.add_speed_xyz();
                return;
            }
            let draw = self.em.rnd();
            self.set_r2(2);
            self.set_r3(0);
            if draw & 3 == 0 {
                self.em.clip(0, 0, 0);
            }
        }
        self.add_speed_xyz();
    }

    fn listen(&mut self) {
        let r2 = self.r2();
        if r2 == 0 {
            self.l.flags &= !FLAG_UP;
            self.em.clip(4, 0, 0xF);
            self.l.gun_area = 3;
            self.set_r2(1);
            self.set_r3(0);
            self.l.timer0 = 0;
        }
        if r2 <= 1 {
            if self.joint(0x100) {
                self.em.clip(4, 0, 3);
                let next = self.r2() + 1;
                self.set_r2(next);
            }
        } else if r2 == 2 && self.joint(0x400) {
            self.go(4);
        }
        let r3 = self.r3().wrapping_add(1);
        self.set_r3(r3);
        if r3 > 10 {
            self.l.target = self.spots_close();
            if self.l.target {
                self.go(0xF);
            }
        }
    }

    fn decide(&mut self) {
        let life = self.player_life();
        if life < 0 || self.em.work().routine[0] == 7 {
            self.go(0xE);
        } else if self.em.work().kind & 0x10 != 0 && self.em.z.life < 0x32 && self.em.rnd() & 3 == 0
        {
            // `Routine_1` 1, up a wall.
            self.go(0xE);
        } else {
            let mut off = i32::from(self.angle_to_player_raw());
            if off > 0x800 {
                off = 0x1000 - off;
            }
            let side = branch(off, 0x100, 0x200, 0x600);
            match branch(self.em.distance, 0x9C4, 0x1194, 0x1964) {
                0 => self.decide_near(side),
                1 => self.decide_middle(side, life),
                2 => self.decide_far(side, life),
                _ => self.decide_farthest(side, life),
            }
        }
        if self.em.work().routine[1] != 0xF {
            self.mv();
        }
        self.l.flags &= !FLAG_KNOWS;
    }

    fn angle_to_player_raw(&self) -> u16 {
        let body = self.em.mover.player.pos;
        let [x, _, z] = self.em.world.player;
        direction_ck(body[0], body[2], x, z).wrapping_sub(self.em.mover.player.angle) & 0xFFF
    }

    fn decide_near(&mut self, side: u8) {
        if self.l.wait != 0 {
            self.go(6);
        } else if self.em.rnd() & 3 != 0 {
            self.go(if side < 2 { 6 } else { 2 });
        } else {
            self.go(9);
        }
    }

    fn decide_middle(&mut self, side: u8, life: i16) {
        match side {
            0 => {
                if self.l.wait != 0 {
                    self.go(0x13);
                    return;
                }
                if life > 99 {
                    if self.em.rnd() & 7 < 3 {
                        self.go(0xA);
                    } else if self.em.rnd() & 2 != 0 {
                        self.go(0x13);
                    } else {
                        self.go(8);
                    }
                }
                if life < 100 {
                    let state = if self.em.rnd() & 7 > 1 { 8 } else { 0xA };
                    self.go(state);
                }
            }
            1 => {
                if self.l.wait == 0 {
                    self.go(8);
                } else if self.em.rnd() & 1 != 0 {
                    self.go(6);
                } else {
                    self.go(0x13);
                }
            }
            _ => self.go(2),
        }
    }

    fn decide_far(&mut self, side: u8, life: i16) {
        match side {
            0 => {
                if self.l.flags & FLAG_KNOWS != 0 {
                    self.go(0x13);
                    return;
                }
                if life > 99 {
                    let state = if self.em.rnd() & 7 > 4 { 0x13 } else { 0xA };
                    self.go(state);
                }
                if life < 100 {
                    let state = if self.em.rnd() & 7 > 1 { 0x13 } else { 0xA };
                    self.go(state);
                }
            }
            1 => {
                let state = if self.em.rnd() & 3 != 0 { 0x13 } else { 2 };
                self.go(state);
            }
            _ => self.go(0x14),
        }
    }

    fn decide_farthest(&mut self, side: u8, life: i16) {
        if self.em.distance < 10000 || self.l.aware != 2 {
            self.l.aware = 2;
            if side < 3 {
                if life > 99 {
                    let state = if self.em.rnd() & 7 > 4 { 0x14 } else { 0xA };
                    self.go(state);
                }
                if life < 100 {
                    let state = if self.em.rnd() & 7 > 1 { 0x14 } else { 0xA };
                    self.go(state);
                }
            } else {
                self.go(0xC);
            }
        } else {
            self.go(0xC);
        }
    }

    fn chase(&mut self) {
        match self.r2() {
            0 => {
                self.l.flags |= FLAG_UP;
                self.l.gun_area = 4;
                self.em.clip(1, 0, 0xF);
                self.set_r2(1);
            }
            1 => {}
            _ => return,
        }
        self.goto_player(0x10);
        self.spd();
        self.em.mover.add_speed_xz();
        if self.joint(0x100) {
            self.go(0xF);
        }
        if self.em.distance > 4000 {
            self.go(0x14);
            return;
        }
        if self.l.wait == 0 && self.em.distance < 2000 {
            self.go(9);
        }
    }

    fn run_at(&mut self) {
        match self.r2() {
            0 => {
                self.l.flags |= FLAG_UP;
                self.l.gun_area = 4;
                self.em.clip(0x1E, 0, 0xF);
                self.l.timer0 = 0;
                self.l.timer1 = 0;
                self.l.turn = 0;
                self.set_r2(1);
            }
            1 => {}
            _ => return,
        }
        if self.em.z.sca_info & 1 == 0 {
            self.l.timer1 = 0;
        } else {
            self.l.timer1 += 1;
        }
        let stuck = self.l.timer1;
        if stuck < 6 {
            if self.em.rnd() & 2 == 0 {
                self.em.goto_dest(0x32);
            } else {
                self.goto_player(0x32);
            }
        } else {
            self.l.timer0 += 1;
            if stuck > 0x14 {
                if stuck == 0x15 {
                    self.l.turn = 0;
                }
                self.turn_at_wall(0x32);
            } else {
                self.em.goto_dest(0x32);
            }
        }
        self.spd();
        self.joint(0x100);
        self.em.mover.add_speed_xz();
        self.l.timer0 += 1;
        if self.l.timer0 > 0xB3 {
            self.go(0);
            return;
        }
        if self.em.distance < 0x898 {
            self.go(0xF);
        }
    }

    /// `Em22_Damage`.
    fn damage(&mut self) {
        if self.em.work().routine[1] != 0xFF {
            self.l.weapon = self.em.work().routine[1];
            self.l.target = true;
            if *self.em.world.tongue_owner == self.em.world.slot as u8 + 2 {
                *self.em.world.tongue_owner = 0;
            }
            self.l.aware = 2;
            self.em.work().routine[1] = 0xFF;
            self.pick_damage();
            if self.em.z.damage_no != 0 {
                self.l.flags &= FLAGS_KEPT;
                *self.em.be_flg &= !(BE_NO_OBA | BE_NO_PUSH | BE_NO_SCA);
            }
        }
        match self.em.z.damage_no {
            0 => self.hurt(),
            1 | 6 => self.knocked(),
            3 => self.flinch(),
            4 => self.reared(),
            7 => self.shot_down(),
            _ => {}
        }
    }

    /// `FUN_00467040`, on the floor.
    fn pick_damage(&mut self) {
        let flags = self.l.flags;
        if flags & FLAG_FALLING != 0 {
            self.em.z.damage_no = 0;
            return;
        }
        if flags & FLAG_JUMPING != 0 {
            self.em.z.damage_no = 7;
            return;
        }
        let behind = self.em.cdir_player();
        match self.l.weapon {
            1 | 2 | 3 | 0xD | 0x10 | 0x12 | 0x13 => self.em.z.damage_no = 0,
            5..=9 | 0xB => {
                self.em.z.damage_no = if self.em.rnd() & 0xF > 4 {
                    if behind { 4 } else { 3 }
                } else {
                    1
                };
            }
            4 | 0xC | 0xE | 0xF => self.em.z.damage_no = 1,
            0xA | 0x11 => {
                self.em.z.damage_flg &= !0x20;
                if behind {
                    self.em.z.damage_flg |= 0x20;
                }
                self.em.z.damage_no = 6;
            }
            _ => {}
        }
    }

    /// Case 0: it barely notices.
    fn hurt(&mut self) {
        if self.em.z.life < 0 {
            self.em.work().routine = [3, 0, 0, 0];
            return;
        }
        self.em.work().routine = self.em.z.last_routine;
        self.em.z.sounds.push(7);
        self.hit_blood(false);
        if self.em.work().routine[1] != 0xFF {
            self.l.flags |= FLAG_KNOWS;
            self.mv();
            self.l.flags &= !FLAG_KNOWS;
        }
    }

    /// `Spd_x`, as a speed along the shooter's facing.
    fn along_shot(&mut self, speed: i16) {
        let shooter = self.em.world.player_angle;
        let (x, z) = player2::rotate(shooter, i32::from(speed), 0);
        self.em.work().spd = [x as i16, 0, z as i16];
    }

    /// Cases 1 and 6, which go by the side the shot came from.
    fn knocked(&mut self) {
        match self.em.work().routine[3] {
            0 => {
                let behind = self.em.z.damage_flg & 0x20 != 0;
                self.em.z.sounds.push(7);
                self.hit_blood(behind);
                self.em.work().spd = [100, 0, 0];
                let shooter = self.em.world.player_angle;
                let to = if behind {
                    shooter
                } else {
                    shooter.wrapping_add(0x800)
                };
                let body = &mut *self.em.mover.player;
                body.angle = goto(body.angle, to, 0x80);
                self.l.flags &= !FLAG_UP;
                self.em.work().routine[3] = 1;
                self.em.clip(8, 0, 3);
            }
            1 => {}
            2 => {
                if self.em.z.life < 0 {
                    self.em.work().routine = [3, 0, 0, 0];
                } else {
                    self.after_hurt();
                }
                return;
            }
            _ => return,
        }
        let speed = self.em.work().spd[0];
        if speed > 0 {
            self.em.work().spd[0] = speed - 6;
        }
        let speed = self.em.work().spd[0];
        let shooter = self.em.world.player_angle;
        let (x, z) = player2::rotate(shooter, i32::from(speed), 0);
        let body = &mut *self.em.mover.player;
        body.pos[0] += x;
        body.pos[2] += z;
        let ended = self.joint(0x400);
        self.em.work().routine[3] += u8::from(ended);
    }

    /// Case 3: thrown on its back.
    fn flinch(&mut self) {
        match self.em.work().routine[3] {
            0 => {
                self.em.z.damage_cnt |= 0x80;
                self.em.z.sounds.push(7);
                self.hit_blood(false);
                self.along_shot(0x32);
                let to = self.em.world.player_angle.wrapping_add(0x800);
                let body = &mut *self.em.mover.player;
                body.angle = goto(body.angle, to, 0x80);
                let draw = self.em.rnd();
                self.l.flags |= FLAG_UP;
                self.l.timer0 = i16::from(draw & 0x11);
                self.em.clip(9, 0, 0);
                self.em.work().routine[3] = 1;
                self.flinch_slide();
            }
            1 => self.flinch_slide(),
            2 => {
                if self.em.z.life < 0 {
                    self.em.work().routine[3] = 0;
                    self.em.work().routine[2] += 1;
                    return;
                }
                self.l.timer0 -= 1;
                if self.l.timer0 < 1 {
                    self.em.clip(0x16, 0, 3);
                    self.l.timer0 = 0;
                    self.em.work().routine[3] = 3;
                }
            }
            3 => {
                self.l.timer0 += 1;
                if self.l.reach < -500 {
                    self.l.reach += 0x50;
                }
                self.spd();
                if self.joint(0x400) {
                    self.l.reach = -500;
                    self.em.z.damage_cnt &= 0x7F;
                    self.l.gun_area = 3;
                    self.after_hurt();
                }
                self.em.mover.add_speed_xz();
            }
            _ => {}
        }
    }

    fn flinch_slide(&mut self) {
        let ended = self.joint(0x1000);
        self.em.work().routine[3] += u8::from(ended);
        if self.l.reach > -800 {
            self.l.reach -= 0x28;
        }
        self.l.gun_area = 3;
        let [x, _, z] = self.em.work().spd;
        let body = &mut *self.em.mover.player;
        body.pos[0] += i32::from(x);
        body.pos[2] += i32::from(z);
    }

    /// Case 4: back on its hind legs.
    fn reared(&mut self) {
        match self.em.work().routine[3] {
            0 => {
                self.em.z.damage_cnt |= 0x80;
                self.em.z.sounds.push(7);
                self.hit_blood(true);
                self.along_shot(100);
                let to = self.em.world.player_angle;
                let body = &mut *self.em.mover.player;
                body.angle = goto(body.angle, to, 0x80);
                self.l.flags |= FLAG_UP;
                self.l.timer0 = 0;
                self.em.clip(0xA, 0, 3);
                self.em.work().routine[3] = 1;
                self.reared_slide();
            }
            1 => self.reared_slide(),
            2 => {
                if self.em.z.life < 0 {
                    self.em.work().routine[3] = 0;
                    self.em.work().routine[2] += 1;
                    return;
                }
                self.l.flags |= FLAG_UP;
                self.em.clip(5, 0, 0xF);
                self.em.work().routine[3] = 3;
                self.reared_up();
            }
            3 => self.reared_up(),
            _ => {}
        }
    }

    fn reared_slide(&mut self) {
        let ended = self.joint(0x400);
        self.l.timer0 += 1;
        self.em.work().routine[3] += u8::from(ended);
        if self.l.timer0 == 4 {
            self.along_shot(0x32);
        }
        let [x, _, z] = self.em.work().spd;
        let body = &mut *self.em.mover.player;
        body.pos[0] += i32::from(x);
        body.pos[2] += i32::from(z);
    }

    fn reared_up(&mut self) {
        if self.joint_reversed(0x100, true) {
            self.l.gun_area = 3;
            self.em.z.damage_cnt &= 0x7F;
            self.after_hurt();
        }
    }

    /// Case 7: shot in the air.
    fn shot_down(&mut self) {
        let mut r3 = self.em.work().routine[3];
        if r3 == 0 {
            self.em.z.damage_cnt |= 0x80;
            self.em.z.sounds.push(7);
            self.hit_blood(false);
            self.l.flags |= FLAG_UP;
            self.em.work().spd = [0; 3];
            self.em.clip(0x19, 0, 0);
            self.em.mover.player.pos[1] += 0x546;
            self.em.work().routine[3] = 1;
            r3 = 1;
        }
        if r3 == 1 {
            self.joint(0x1000);
            self.em.work().move_cnt = 0;
            if self.l.left_y <= self.em.mover.player.pos[1] {
                self.atd_set();
                *self.em.be_flg &= !BE_NO_SCA;
                self.em.work().routine[3] = 2;
                self.em.clip(0x19, 0, 0);
                r3 = 2;
            }
        }
        match r3 {
            2 => {
                let ended = self.joint(0x400);
                self.em.work().routine[3] += u8::from(ended);
            }
            3 => {
                if self.em.z.life < 0 {
                    self.em.z.damage_cnt &= 0x7F;
                    self.l.gun_area = 3;
                    *self.em.be_flg &= !(BE_NO_PUSH | BE_NO_SCA);
                    self.em.work().routine[2] += 1;
                } else {
                    self.em.clip(0x16, 0, 0);
                    self.em.work().routine[3] = 4;
                    let ended = self.joint(0x1000);
                    self.em.work().routine[3] += u8::from(ended);
                }
            }
            4 => {
                let ended = self.joint(0x1000);
                self.em.work().routine[3] += u8::from(ended);
            }
            5 => {
                self.em.z.damage_cnt &= 0x7F;
                self.l.gun_area = 3;
                *self.em.be_flg &= !(BE_NO_PUSH | BE_NO_SCA);
                self.after_hurt();
            }
            _ => {}
        }
        let ground = self.l.left_y;
        let pos_y = self.em.mover.player.pos[1];
        let speed = i32::from(self.em.work().spd[1]);
        if speed + 0x48 + pos_y < ground {
            let speed = self.em.work().spd[1] + 0x30;
            self.em.work().spd[1] = speed;
            let [x, _, z] = self.em.work().spd;
            let body = &mut *self.em.mover.player;
            body.pos[1] += i32::from(speed);
            body.pos[0] += i32::from(x);
            body.pos[2] += i32::from(z);
        } else {
            self.em.mover.player.pos[1] = ground;
        }
    }

    /// `FUN_004675f0`.
    fn after_hurt(&mut self) {
        let state = if self.em.distance < 0x9C4 {
            if self.player_turn_here(0x140) == 0 {
                self.l.flags |= FLAG_KNOWS;
                0x13
            } else if self.em.rnd() & 1 == 0 {
                0xF
            } else {
                0xC
            }
        } else if self.player_turn_here(0x1E0) == 0 {
            0x14
        } else {
            0xC
        };
        self.go(state);
    }

    /// `Goto00_ck` with the player as its subject: how far the player must turn
    /// to face the licker.
    fn player_turn_here(&self, rate: i32) -> i32 {
        let [x, _, z] = self.em.mover.player.pos;
        let [from_x, _, from_z] = self.em.world.player;
        let target = super::direction_ck(from_x, from_z, x, z);
        super::turn_within(self.em.world.player_angle, target, rate)
    }

    /// `Em22_Die`.
    fn die(&mut self) {
        if self.em.work().routine[1] != 0xFF {
            self.l.weapon = self.em.work().routine[1];
            self.em.work().routine[1] = 0xFF;
            self.l.target = true;
            self.em.z.died = true;
            self.em.z.damage_no = if self.l.flags & FLAG_JUMPING != 0 {
                5
            } else {
                u8::from(self.em.cdir_player())
            };
        }
        match self.em.z.damage_no {
            0 => self.die_front(),
            1 => self.die_back(),
            5 => self.die_in_air(),
            _ => {}
        }
    }

    fn die_front(&mut self) {
        let mut r2 = self.r2();
        if r2 == 0 {
            self.em.z.sounds.push(8);
            self.hit_blood(false);
            *self.em.be_flg |= BE_NO_OBA;
            self.set_r2(1);
            r2 = 1;
        }
        if r2 == 1 {
            let ended = self.mot_play_sp(0x1C, 0);
            self.em.step(ended);
            if self.l.reach > -1000 {
                self.l.reach -= 0x32;
            }
        }
        if r2 == 2 {
            self.set_r2(3);
            r2 = 3;
        }
        if r2 == 3 && self.mot_play(0xB, 0xF, false) {
            self.l.fading = 1;
            self.set_r2(4);
        }
    }

    fn die_back(&mut self) {
        let mut r2 = self.r2();
        if r2 == 0 {
            self.em.z.sounds.push(8);
            self.hit_blood(true);
            *self.em.be_flg |= BE_NO_OBA;
            self.set_r2(1);
            r2 = 1;
        }
        if r2 == 1 {
            let ended = self.mot_play_sp(0x1D, 0xF);
            self.em.step(ended);
            if self.l.reach < -100 {
                self.l.reach += 0x19;
            }
        } else if r2 == 2 {
            self.l.fading = 1;
            self.set_r2(3);
        }
    }

    fn die_in_air(&mut self) {
        match self.r2() {
            0 => self.shot_down(),
            1 => {
                self.em.z.sounds.push(8);
                self.set_r2(2);
            }
            2 if self.mot_play(0xB, 0xF, false) => {
                self.l.fading = 1;
                self.set_r2(3);
            }
            _ => {}
        }
    }
}

/// `Branch`.
fn branch(value: i32, first: i32, second: i32, third: i32) -> u8 {
    if value < first {
        0
    } else if value < second {
        1
    } else if value < third {
        2
    } else {
        3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn its_shadow_lies_ahead_of_the_third_part() {
        let licker = Licker {
            part2: [1000, -50, 2000],
            ..Licker::default()
        };
        // Angle 0 faces +x.
        assert_eq!(licker.shadow_at(0), [1000 + 0xFA, -50, 2000]);
        assert_eq!(licker.shadow_at(0x800), [1000 - 0xFA, -50, 2000]);
    }

    #[test]
    fn it_drools_on_one_draw_in_thirty_two_while_it_lives() {
        let mut licker = Licker::default();
        let mut be_flg = 0;
        let mut rnd = super::super::Rnd::default();
        let mut effects = crate::esp2::Effects::default();
        let start = rnd;
        before(&mut licker, 0, &mut be_flg, &mut rnd, &mut effects);
        assert_eq!(rnd, start, "a dead licker draws nothing");
        before(&mut licker, 1, &mut be_flg, &mut rnd, &mut effects);
        assert_ne!(rnd, start);
    }

    #[test]
    fn a_fading_body_stops_colliding_once() {
        let mut licker = Licker {
            fading: 1,
            timer1: 9,
            ..Licker::default()
        };
        let mut be_flg = 0;
        let mut rnd = super::super::Rnd::default();
        let mut effects = crate::esp2::Effects::default();
        before(&mut licker, 0, &mut be_flg, &mut rnd, &mut effects);
        assert_eq!(be_flg, BE_NO_OBA | BE_NO_SCA);
        assert_eq!((licker.fading, licker.timer1), (2, 0));
    }
}
