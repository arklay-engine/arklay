//! `Em_22` and its copy `Em_24`: the lickers.
//!
//! Their behaviour is the script `lib/licker.lua`, behind a thin
//! `enemy/{id:02x}/enemy.lua` for each id; this is what the script
//! stands on: the fields the engine reads, the tongue's blend, the turn of the
//! surface it's on and the pushes at the end of its tick.

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
pub(super) const VARS: usize = 20;

/// What `Em_22` keeps at the end of the work, and in its `Timer`s.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Licker {
    /// `Atd[1]`, the head.
    pub head: Atd,
    /// `Atd[2]`, its back, which only counts on a wall or the ceiling
    /// (`Em22_Atd_set_2`).
    pub back: Option<Atd>,
    /// `DAT_006894f0`: the turn the script asks of the surface this tick.
    pub(super) super_add: [i16; 3],
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
    shadow_scale: Option<i32>,
    /// `Workm.t` of parts 0 and 1, which `Oba_set_ofs` reads from the last
    /// `Joint_trans2`.
    pub(super) part0: [i32; 3],
    pub(super) part1: [i32; 3],
    /// `Workm.t` of the part `Kage_work_set` follows.
    part2: [i32; 3],
    /// `Workm.t` of part 9 as the last draw left it, where `Em22_before`
    /// starts its drool.
    drool: [i16; 3],
    /// Every part's `Workm` from the last `Joint_trans2`, which a part the
    /// script hides keeps.
    parts: Vec<Option<player2::Matrix>>,
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
    /// `Em22_before` runs after the move and before this tick's
    /// `Joint_trans2`: its drool is on part 9 as the last draw left it, or
    /// the body before it has been drawn.
    fn latch_drool(&mut self, pos: [i32; 3]) {
        self.drool = self
            .parts
            .get(9)
            .copied()
            .flatten()
            .map_or(pos, |part| part.t)
            .map(|axis| axis as i16);
    }

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

    /// `Em22_before`: how much of the shadow's size is left, the higher the
    /// body the less, in 9000ths.
    pub fn shadow_scale(&self) -> i32 {
        self.shadow_scale.unwrap_or(9000)
    }

    /// Where `MulKage` puts the shadow, `Off_x` ahead of the third part.
    pub fn shadow_at(&self, angle: u16) -> [i32; 3] {
        let (x, z) = player2::rotate(angle, 0xFA, 0);
        [self.part2[0] + x, self.part2[1], self.part2[2] + z]
    }
}

/// `Em_24`'s id: its init (`FUN_0046cfa0`) poses it through `Joint_move`.
const EM24_ID: u8 = 0x24;

/// Whether the licker plays through `J_seq2` (`Mot_play2`'s `Joint_move2`)
/// before this tick's script. `Em22_Init` poses through `Joint_move2` too,
/// but `Em_24`'s init (`FUN_0046cfa0`) poses through `Joint_move` and plain
/// `J_seq`, which leaves each part's `M` a raw `RotMatrix`: its `J_seq2`
/// starts from those, after the init.
fn jseq2_from_the_start(id: u8, routine0: u8) -> bool {
    id != EM24_ID || routine0 != 0
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
        self.l.latch_drool(self.em.mover.player.pos);
        let id = self.em.mover.tick.id;
        if jseq2_from_the_start(id, self.em.work().routine[0]) {
            self.em.work().jseq2.get_or_insert_with(Vec::new);
        }
        self.em.root_ck();
        if self.em.world.stopped {
            return;
        }
        if self.em.z.damage_cnt & 0x7F != 0 {
            self.em.z.damage_cnt -= 1;
        }
        self.l.super_add = [0; 3];
        self.em.z.vars[super::DOG_GROUND] = self.ground();
        self.script();
        if self.em.work().jseq2.is_none() && self.em.work().routine[0] != 0 {
            // `Em_24`'s init has posed it through `J_seq`.
            self.em.mover.start_jseq2();
        }
        if self.l.super_add != [0; 3] {
            let work = self.em.work();
            let (_, surface) = super::surface::turned(work.super_matrix, self.l.super_add);
            work.super_matrix = Some(surface);
        }
        self.tongue_move();
        if self.l.flags & FLAG_QUIET == 0 {
            let seq = self.em.mover.now_seq();
            if seq & super::SEQ_SOUND != 0 {
                self.em
                    .z
                    .sounds
                    .push(((seq >> 28) as u8, self.em.mover.player.pos));
            }
        }
        self.em.z.last_routine = self.em.work().routine;
        self.oba_and_sca();
        self.joint_trans();
        if self.em.work().routine[0] == 1 {
            let above = self.l.part2[1] - self.ground();
            self.l.shadow_scale = Some((above + 9000).max(0));
        }
    }

    /// `Ground`, which `Game_loop` finds under the part the shadow follows.
    fn ground(&mut self) -> i32 {
        let at = self.l.part2;
        let room = self.em.mover.tick.room;
        room.sca.as_ref().map_or(0, |sca| {
            crate::sca::ground_at(sca, at, 0, super::MASK).height
        })
    }

    /// `update` of `enemy/22/enemy.lua` or `enemy/24/enemy.lua`. A pack without it leaves the licker
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
        let old = Some(self.em.old_pos);
        for other in self.em.world.bodies {
            let body = &mut self.em.mover.player.pos;
            let be_flg = *self.em.be_flg;
            if super::oba_hit(other, be_flg, &mut self.em.z.atd, body, old) {
                self.em.z.at_em_no = other.slot;
            }
            if super::oba_hit(other, be_flg, &mut self.l.head, body, old) {
                self.em.z.at_em_no = other.slot;
            }
            if let Some(back) = &mut self.l.back
                && super::oba_hit(other, be_flg, back, body, old)
            {
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

    /// `Oba_set_ofs`: every box, each from its own offset on the surface.
    fn oba_set_ofs(&mut self, at: [i32; 3]) {
        let angle = self.em.mover.player.angle;
        let m = self.em.work().body_rotation(angle);
        let place = |atd: &mut Atd| {
            let ofs = super::surface::apply(&m, [i32::from(atd.ofs_x), i32::from(atd.ofs_y), 0]);
            atd.at = std::array::from_fn(|axis| at[axis] + ofs[axis]);
        };
        place(&mut self.em.z.atd);
        place(&mut self.l.head);
        if let Some(back) = &mut self.l.back {
            place(back);
        }
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
        let hidden = self.em.work().hidden_parts;
        self.em.mover.joint_trans2(hidden, &mut self.l.parts);
        let parts = &self.l.parts;
        let t = |part: usize| parts.get(part).copied().flatten().map(|matrix| matrix.t);
        let pos = self.em.mover.player.pos;
        self.l.part0 = t(0).unwrap_or(pos);
        self.l.part1 = t(1).unwrap_or(pos);
        self.l.part2 = t(2).unwrap_or(pos);
        self.em.work().neck.position = t(8);
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

    /// The drool starts on part 9's `Workm.t` from the last draw: in
    /// `esp_licker24_fight` the tick's own pose put it up to 70 units off
    /// in a long blend.
    #[test]
    fn its_drool_is_on_the_last_draws_ninth_part() {
        let mut licker = Licker::default();
        licker.latch_drool([10, 20, 30]);
        assert_eq!(licker.drool, [10, 20, 30], "not drawn yet: the body");
        licker.parts = vec![None; 10];
        licker.parts[9] = Some(player2::Matrix {
            m: player2::rot_matrix([0; 3]),
            t: [-11827, -282, -15767],
        });
        licker.latch_drool([10, 20, 30]);
        assert_eq!(licker.drool, [-11827, -282, -15767]);
    }

    /// `Em22_Init` poses through `Joint_move2`, `Em_24`'s init through
    /// `Joint_move`; both play through `J_seq2` after.
    #[test]
    fn only_em24s_init_poses_through_j_seq() {
        assert!(jseq2_from_the_start(0x22, 0));
        assert!(jseq2_from_the_start(0x22, 1));
        assert!(!jseq2_from_the_start(EM24_ID, 0));
        assert!(jseq2_from_the_start(EM24_ID, 1));
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
