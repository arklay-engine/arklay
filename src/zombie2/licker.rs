//! `Em_22`: the licker, on the floor.
//!
//! Its behaviour is the script `enemy/22/enemy.lua`; this is what the script
//! stands on: the fields the engine reads, the tongue's blend and the pushes at
//! the end of its tick. The walls and the ceiling need the body's `Super_matrix`,
//! which isn't ported: a licker the scripts start there doesn't spawn, and one
//! that would climb stays down (`docs/re2-deviations.md`).

use super::{Atd, Em, NO_BODY};
use crate::player2;

/// `Be_flg`: no pushes, no `Sca_ck_em`.
const BE_NO_OBA: u32 = 2;
const BE_NO_SCA: u32 = 8;

/// The tongue's part, which isn't in the skeleton: `Em22_Init` hangs it from
/// the part below, turned this way.
pub const TONGUE_PART: u8 = 0x16;
pub const TONGUE_PARENT: usize = 8;
pub const TONGUE_TURN: [i16; 3] = [0x400, 0, 0x400];

/// `+0x22c`: its feet make no sound, and `Sca_ck_em` is off for a tick.
const FLAG_QUIET: u16 = 4;
const FLAG_NO_SCA: u16 = 0x100;

/// How many of the work's fields the script keeps for itself.
pub(super) const VARS: usize = 13;

/// What `Em_22` keeps at the end of the work, and in its `Timer`s.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Licker {
    /// `Atd[1]`, the head.
    pub head: Atd,
    /// The script's fields (`e:var`), which only it reads.
    pub(super) vars: [i32; VARS],
    /// `+0x22c`.
    pub(super) flags: u16,
    /// `+0x21a`: how far along the tongue is.
    pub(super) tongue: u8,
    /// `+0x240`.
    tongue_count: u8,
    /// The weights of the tongue's nine shapes (`pSa_dat_head[3..]`).
    tongue_shape: [i16; 9],
    /// `+0x23a`: how far ahead of its chest `Sca_ck_em` checks.
    pub(super) reach: i16,
    /// `+0x21b`: its colour goes once it's dead.
    pub(super) fading: u8,
    /// `Timer1`.
    pub(super) timer1: i16,
    /// `Sca_old_x`, `Sca_old_z`.
    sca_old: [i32; 2],
    /// `Workm.t` of parts 0 and 1, which `Oba_set_ofs` reads from the last
    /// `Joint_trans2`.
    pub(super) part0: [i32; 3],
    pub(super) part1: [i32; 3],
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
    /// `tongue_move`'s states and shapes, and whether it has just gone in.
    fn step_tongue(&mut self) -> bool {
        let l = self;
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
                l.tongue_shape[0] += 0x800;
                l.tongue_count += 1;
                if l.tongue_count == 2 {
                    l.tongue = 4;
                    l.tongue_count = 0;
                }
            }
            4 => {
                l.tongue_shape[1] += 0x400;
                l.tongue_count += 1;
                if l.tongue_count == 4 {
                    l.tongue = 5;
                    l.tongue_count = 4;
                }
            }
            5 => {
                l.tongue_shape[usize::from(l.tongue_count) - 2] += 0x1000;
                l.tongue_count += 1;
                if l.tongue_count == 11 {
                    l.tongue = 6;
                    l.tongue_count = 0;
                    l.tongue_shape[..8].fill(0);
                }
            }
            6 => {
                l.tongue_shape[8] = 0;
                l.tongue = 0;
                return true;
            }
            _ => {}
        }
        false
    }

    /// The tongue's shape while it's out: `Be_flg` is up from the first shape
    /// `tongue_move` blends to its last.
    pub fn tongue_weights(&self) -> Option<&[i16; 9]> {
        (self.tongue_shape != [0; 9]).then_some(&self.tongue_shape)
    }

    /// Where `MulKage` puts the shadow, `Off_x` ahead of the third part.
    pub fn shadow_at(&self, angle: u16) -> [i32; 3] {
        let (x, z) = player2::rotate(angle, 0xFA, 0);
        [self.part2[0] + x, self.part2[1], self.part2[2] + z]
    }
}

/// `Em_22`: the script runs the tick's behaviour, and the rest follows.
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
        self.script();
        self.tongue_move();
        if self.l.flags & FLAG_QUIET == 0 {
            let seq = self.em.mover.now_seq();
            if seq & super::SEQ_SOUND != 0 {
                self.em.z.sounds.push((seq >> 28) as u8);
            }
        }
        self.em.z.last_routine = self.em.work().routine;
        self.oba_and_sca();
        self.joint_trans();
    }

    /// `update` of `enemy/22/enemy.lua`. A pack without it leaves the licker
    /// standing.
    fn script(&mut self) {
        let Some(lua) = self.em.world.lua.take() else {
            return;
        };
        let pack = self.em.world.pack;
        let id = self.em.mover.tick.id;
        let mut api = super::script::Api::new(self.em, self.l);
        lua.update_re2(pack, id, &mut api);
        self.em.world.lua = Some(lua);
    }

    /// `tongue_move`.
    fn tongue_move(&mut self) {
        if self.l.step_tongue() {
            *self.em.world.tongue_owner = 0;
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
        let floor = self.em.z.floor;
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
    fn the_tongue_blends_out_shape_by_shape_and_goes_at_the_end() {
        let mut licker = Licker {
            tongue: 3,
            ..Licker::default()
        };
        let mut steps = 0;
        let mut seen = Vec::new();
        assert_eq!(licker.tongue_weights(), None);
        while licker.tongue != 0 {
            licker.step_tongue();
            seen.push(licker.tongue_shape);
            steps += 1;
            assert!(steps < 20);
        }
        // Two ticks, four, seven, and the one that puts it away.
        assert_eq!(steps, 14);
        assert_eq!(seen[0][0], 0x800);
        assert_eq!(seen[1][0], 0x1000);
        assert_eq!(seen[5][1], 0x1000);
        assert_eq!(seen[6][2], 0x1000);
        assert_eq!(seen[12], [0, 0, 0, 0, 0, 0, 0, 0, 0x1000]);
        assert_eq!(licker.tongue_weights(), None);
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
