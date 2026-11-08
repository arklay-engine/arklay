//! RE2's partner: `Spl_Auto` and `Sherry_move`, for `splayer_work`.
//!
//! When a script places Sherry (0x45 or 0x4F) as the partner (`Type` 0x400,
//! with `fg_status`'s `0x10000000`), `_Spl` runs her own routine: each tick
//! `Spl_Root_ck_sys` picks a point beside Claire to follow ([`root_ck_sys`]),
//! and `Sherry_move` (`spl_auto`) stands, walks, runs or turns toward it.
//! Enemies, climbing and water are not ported yet: the nearest-enemy list is
//! always empty, `Oma_Spl_updown_ck` finds no step and `Water` is 0.

use crate::player2::{Bank, Mover, direction_ck, goto};
use crate::state::{Guide, ScaRect};
use crate::zombie2::{Rnd, RootCk, RootWorld, root_ck, turn_within};

/// `Spl_r_pos` (`0x53cda0`): the follow points' length and two angles from
/// Claire's facing, beside her then behind her, Ada's row then Sherry's.
pub const SPL_R_POS: [i16; 12] = [
    1000, 1300, -1300, 800, 1920, 1920, 700, 1100, -1100, 600, 2176, 1920,
];
/// `Sherry_St_pos` (`0x53d148`): the follow point's kind for each answer of
/// `Spl_em_pos_ck`, 0 beside and 2 behind.
pub const SHERRY_ST_POS: [u8; 12] = [0, 2, 2, 0, 0, 2, 2, 0, 0, 2, 2, 0];
/// `Spl_pos_ck_timer`'s start (`0x53cdef`): `Sherry_move` checks the enemies
/// every sixteenth tick.
pub const SPL_POS_CK_START: u8 = 0x0F;

/// The clips by `D_life_u`, healthy, hurt and badly hurt: the stand, the walk
/// (and `Aida_mv04`'s turn) and the run of the first sub bank, and
/// `Aida_mv03`'s turn of the main bank.
const STAND_CLIPS: [u8; 3] = [2, 5, 8];
const WALK_CLIPS: [u8; 3] = [0, 3, 6];
const RUN_CLIPS: [u8; 3] = [1, 4, 7];
const MV03_CLIPS: [u8; 3] = [0, 9, 9];
/// `Sherry_mv00`'s clips: the sidestep of the first sub bank, to one side and
/// the other, and the call of the main bank when she is lost.
const SIDESTEP_CLIP: u8 = 9;
const SIDESTEP_OTHER_CLIP: u8 = 0xB;
const CALL_CLIP: u8 = 6;

/// `Hokan_flg` of a new clip: the `0x70000` of `Move`'s dword.
const CLIP_BLEND: u8 = 7;
/// `Joint_move`'s blend step for every one of her clips.
const STEP: u16 = 0x200;

/// The state word's bits (`+0x21C`): the follow point's kind, a wall turn,
/// lost, and the kind changed (cleared every check).
pub const STATE_KIND: u16 = 3;
pub const STATE_WALL_TURN: u16 = 0x1000;
pub const STATE_LOST: u16 = 0x2000;
pub const STATE_CHANGED: u16 = 0x8000;

/// The distances the states decide by.
const LOST: u16 = 8000;
const CALL_NECK: u16 = 6000;
const LOST_ENEMY_NEAR: u16 = 6000;
const LOST_WALK_BACK: u16 = 4000;
const FOUND: u16 = 3000;
const RUN: u16 = 2000;
const WALK: u16 = 1500;
const STOP_RUN: u16 = 1000;
const STEP_BACK: u16 = 400;
const SIDESTEP: u16 = 200;
const ARRIVED: u16 = 150;
const TURN_ROUND_ARRIVED: u16 = 100;
/// The ticks against a wall before the wall turn, and the turn's length.
const WALL_TICKS: i16 = 60;
const WALL_TURN_TICKS: i16 = 15;
const WALL_TURN_ENEMY_TICKS: i16 = 30;
/// `Aida_mv01`'s count to a stop near Claire, or near the point at a wall.
const WALK_STOP_TICKS: i16 = 60;
/// The ticks `Aida_mv03` turns before it may stop, and at most.
const DODGE_MIN_TICKS: i8 = 5;
const DODGE_MAX_TICKS: i8 = 120;

/// `Routine_1` of `Sherry_Move_type`'s tables (`0x53d154`, `0x53ce28`).
const MV_STAND: u8 = 0;
const MV_WALK: u8 = 1;
const MV_RUN: u8 = 2;
const MV_DODGE: u8 = 3;
const MV_FACE: u8 = 4;
const MV_FACE_AHEAD: u8 = 0xB;
const MV_TURN_ROUND: u8 = 0xC;
const MV_FACE_POINT: u8 = 0xD;

/// The partner's own words of `PLAYER_WORK`, past the player's (`+0x218` up)
/// and those of `+0x150` to `+0x16B` that `Spl_Auto`'s states use. The
/// timers are kept here, not in the player's [`crate::player2::Work`]: in the
/// original they are the same words a hold on her would use.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Partner {
    /// `+0x218`, `+0x21A`: the follow point.
    pub soba: [i16; 2],
    /// `+0x21C`: the [`STATE_KIND`] and the other `STATE_*` bits.
    pub state: u16,
    /// `+0x21E`: `SquareRoot0` from her to the follow point.
    pub dist: u16,
    /// `+0x220`: the wall turn's heading.
    pub turn: u16,
    /// `+0x223`: ticks touching (`At_em_flg`'s `0x8000`), with `0x80` past 60.
    pub touch: u8,
    /// `+0x224`: ticks against a wall, then the wall turn's ticks left.
    pub stuck: i16,
    /// `+0x22A`: counts down to 0; Sherry doesn't read it.
    pub wait: u8,
    /// `Timer0` (`+0x158`), `Timer1` (`+0x15A`), `Timer2` (`+0x16A`) and
    /// `Timer3` (`+0x16B`). `Spl_in45` leaves the last two as the work's
    /// memory held them.
    pub timer0: i16,
    pub timer1: i16,
    pub timer2: i8,
    pub timer3: u8,
    /// `Root_ck`'s turn (`+0x150`) and `Status_flg`'s sight bit. `Sce_em_set`
    /// seeds the turn with the slot's index, 1 for `splayer_work`.
    pub root_cnt: u8,
    pub status: u16,
    /// `Life` (`+0x156`) and `Max_life` (`+0x162`): -1 for a scene character.
    pub life: i16,
    pub max_life: i16,
    /// `Sca_info`'s 1: `Sca_ck_em` pushed her out of a wall last tick.
    pub sca_hit: bool,
}

impl Partner {
    /// The words `Sce_em_set` leaves on `splayer_work`.
    pub fn placed() -> Self {
        Self {
            root_cnt: 1,
            life: -1,
            max_life: -1,
            ..Self::default()
        }
    }

    /// `Spl_in45` (`0x4effd0`): the partner's life and the words its states
    /// start from. `Timer2` and `Timer3` are not reset.
    pub fn spl_in45(&mut self, partner: bool, life: i16) {
        if partner {
            self.max_life = 200;
            self.life = life;
        }
        self.state = 0;
        self.stuck = 0;
        self.turn = 0;
        self.touch = 0;
        self.wait = 0;
    }
}

/// What `Sherry_move` reads of the rest of the game.
pub struct PartnerWorld<'a> {
    /// The player's position, `Cdir_y` and `Routine_1`.
    pub player: [i32; 3],
    pub player_angle: u16,
    pub player_routine: u8,
    /// `L_pl` (`+0x1F0`): her distance from the player.
    pub distance: u32,
    /// The nearest seen live enemy (`0x692fe0`): none until part 3.
    pub enemy: Option<[i32; 3]>,
    /// `0x692fc8`: the first entry of the nearest-enemy cache, which only a
    /// room with enemies rewrites, so it can be stale.
    pub enemy_dist: u16,
    pub rnd: &'a mut Rnd,
    /// `Spl_pos_ck_timer`.
    pub pos_ck_timer: &'a mut u8,
    /// `Water` and `Ground`: the water's level she stands in, and the floor's.
    pub water: i16,
    pub ground: i32,
    /// `BYTE_0098ee7b`'s bit 0: the partner is poisoned.
    pub poisoned: bool,
}

/// `Spl_soba_pos_set` (`0x4f5a00`): the nearer to the body of the two points
/// `ll` from the player at `dir` and `dir2` from its facing. The first wins a
/// tie.
pub fn soba_pos_set(
    player: [i32; 3],
    angle: u16,
    body: [i32; 3],
    ll: i16,
    dir: i16,
    dir2: i16,
) -> [i16; 2] {
    let point = |turn: i16| {
        let (dx, dz) = crate::player2::rotate(angle.wrapping_add(turn as u16), i32::from(ll), 0);
        let distance = {
            let x = i32::from(((body[0] - player[0]) - dx) as i16);
            let z = i32::from(((body[2] - player[2]) - dz) as i16);
            crate::sca::square_root0(z.wrapping_mul(z).wrapping_add(x.wrapping_mul(x))) as u32
        };
        ([(player[0] + dx) as i16, (player[2] + dz) as i16], distance)
    };
    let (first, near) = point(dir);
    let (second, far) = point(dir2);
    if far < near { second } else { first }
}

/// What [`root_ck_sys`] reads.
pub struct RootSys<'a> {
    pub guides: &'a [Guide],
    pub sca: &'a [ScaRect],
    pub player: [i32; 3],
    pub player_angle: u16,
    pub player_neck: Option<[i32; 3]>,
    /// `fg_status`'s `0x10000000` and the work is `splayer_work`.
    pub partner: bool,
}

/// `Spl_Root_ck_sys` (`0x4f21f0`), which `_Spl` runs every tick, stopped or
/// not. For the partner it works out the follow point, routes to it with
/// `Root_ck` and measures her distance from it, and looks at the player.
/// Any other `_Spl` work has `Type`'s `0x1000`, so its `Root_ck` only copies
/// `Sce_free2` and `Sce_free3` into `Dest`, which `plc_dest` writes in their
/// place: only its turn counts here.
pub fn root_ck_sys(
    partner: &mut Partner,
    work: &mut crate::player2::Work,
    body: [i32; 3],
    id: u8,
    kind: u16,
    world: &RootSys<'_>,
) {
    let mut state = RootCk {
        cnt: partner.root_cnt,
        status: partner.status,
        dest: work.dest,
    };
    let root_world = RootWorld {
        guides: world.guides,
        sca: world.sca,
        player_neck: world.player_neck,
    };
    if !world.partner {
        if state.cnt & 0x7F == 0 {
            state.cnt |= 7;
        }
        partner.root_cnt = state.cnt.wrapping_sub(1);
        return;
    }
    let row = if matches!(id, 0x45 | 0x4F) { 6 } else { 0 };
    let r = |n: usize| SPL_R_POS[row + n];
    let soba = match partner.state & STATE_KIND {
        0 => soba_pos_set(world.player, world.player_angle, body, r(0), r(1), r(2)),
        2 => soba_pos_set(world.player, world.player_angle, body, r(3), r(4), r(5)),
        _ => partner.soba,
    };
    partner.soba = soba;
    if crate::zombie2::guide_check(world.guides, soba[0], soba[1]).is_none() {
        partner.soba = [world.player[0] as i16, world.player[2] as i16];
    }
    let neck = work.neck.position;
    root_ck(
        &mut state,
        kind,
        &root_world,
        body,
        neck,
        partner.soba,
        None,
    );
    partner.root_cnt = state.cnt;
    partner.status = state.status;
    work.dest = state.dest;
    // `pEnemy_neck = &pPl`.
    work.neck.target = Some(0);
    let dx = body[0] - i32::from(partner.soba[0]);
    let dz = body[2] - i32::from(partner.soba[1]);
    partner.dist =
        crate::sca::square_root0(dz.wrapping_mul(dz).wrapping_add(dx.wrapping_mul(dx))) as u16;
}

/// `Spl_Auto` (`0x4f23b0`) for Sherry: the touch and wait counts, then
/// `Sherry_move` (`0x4f4aa0`). `kind` is her live `Type`, whose low nibble
/// holds `Spl_em_pos_ck`'s answer.
pub(crate) fn spl_auto(
    mover: &mut Mover<'_, '_>,
    partner: &mut Partner,
    kind: &mut u16,
    world: &mut PartnerWorld<'_>,
) {
    // `At_em_flg`'s `0x8000`: `Sce_em_set` leaves it as the work's memory
    // held it, and only `Oba_ck_em2`, the player's, writes it.
    partner.touch = 0;
    partner.wait = partner.wait.saturating_sub(1);
    let mut auto = Auto {
        m: mover,
        p: partner,
        kind,
        w: world,
    };
    auto.sherry_move();
}

struct Auto<'r, 'a, 'b, 'w> {
    m: &'r mut Mover<'a, 'b>,
    p: &'r mut Partner,
    kind: &'r mut u16,
    w: &'r mut PartnerWorld<'w>,
}

impl Auto<'_, '_, '_, '_> {
    /// `Sherry_move`: the enemy check every sixteenth tick, then the
    /// decision of `Routine_1` and the state it leaves, both this tick.
    fn sherry_move(&mut self) {
        // `Spl_enemy_ck`: nothing to list in a room without enemies.
        if *self.w.pos_ck_timer == 0 {
            let was = *self.kind & 0xFF;
            *self.kind &= 0xFF00;
            *self.kind |= u16::from(self.spl_em_pos_ck());
            self.p.state &= !STATE_CHANGED;
            if was & 0xF != *self.kind & 0xF {
                let kind = SHERRY_ST_POS[usize::from(*self.kind & 0xF)];
                self.p.state = (self.p.state & !STATE_KIND) | u16::from(kind) | STATE_CHANGED;
            }
            *self.w.pos_ck_timer = SPL_POS_CK_START;
        } else {
            *self.w.pos_ck_timer -= 1;
        }
        self.move_type();
    }

    /// `Spl_em_pos_ck(w, 3000)`: where the nearest enemies are, 0 to 10. With
    /// none listed it is 0.
    fn spl_em_pos_ck(&self) -> u8 {
        0
    }

    /// `Sherry_Move_type`'s body, the same for every answer.
    fn move_type(&mut self) {
        match self.routine1() {
            MV_STAND => self.sherry_mv00_br(),
            MV_WALK => self.sherry_mv01_br(),
            MV_RUN => self.sherry_mv02_br(),
            MV_DODGE => self.aida_mv03_br(),
            MV_FACE => self.aida_mv04_br(),
            MV_FACE_AHEAD => self.aida_mv0b_br(),
            MV_TURN_ROUND => self.sherry_mv0c_br(),
            MV_FACE_POINT => self.aida_mv0d_br(),
            _ => {}
        }
        match self.routine1() {
            MV_STAND => self.sherry_mv00(),
            MV_WALK => self.aida_mv01(),
            MV_RUN => self.aida_mv02(),
            MV_DODGE | MV_TURN_ROUND => self.aida_mv03(),
            MV_FACE | MV_FACE_AHEAD | MV_FACE_POINT => self.aida_mv04(),
            routine => log_once(routine),
        }
    }

    fn routine1(&self) -> u8 {
        self.m.work.routine[1]
    }

    fn routine2(&self) -> u8 {
        self.m.work.routine[2]
    }

    /// `Routine = 0x(two)(one)01`.
    fn set(&mut self, one: u8, two: u8) {
        self.m.work.routine = [1, one, two, 0];
    }

    fn lost(&self) -> bool {
        self.p.state & STATE_LOST != 0
    }

    /// Lost: too far from the point.
    fn get_lost(&mut self) {
        self.p.timer0 = 0;
        self.p.state = (self.p.state & !STATE_WALL_TURN) | STATE_LOST;
    }

    fn sca_hit(&self) -> bool {
        self.p.sca_hit
    }

    fn pos(&self) -> [i32; 3] {
        self.m.player.pos
    }

    fn direction_to(&self, x: i32, z: i32) -> u16 {
        let pos = self.pos();
        direction_ck(pos[0], pos[2], x, z)
    }

    /// `Goto00_ck`: 0 once she faces within `rate` of the point.
    fn goto00_ck(&self, x: i32, z: i32, rate: i32) -> i32 {
        turn_within(self.m.player.angle, self.direction_to(x, z), rate)
    }

    fn ahead(&self, x: i32, z: i32, rate: i32) -> bool {
        self.goto00_ck(x, z, rate) == 0
    }

    /// `Goto00`: turn by `rate` toward the point, or away with a negative rate.
    fn goto00(&mut self, x: i32, z: i32, rate: i32) {
        let target = self.direction_to(x, z);
        self.goto01(target, rate);
    }

    /// `Goto01`: turn by `rate` toward a heading.
    fn goto01(&mut self, dir: u16, rate: i32) {
        self.m.player.angle = goto(self.m.player.angle, dir, rate);
    }

    fn goto_dest(&mut self, rate: i32) {
        let [x, z] = self.m.work.dest;
        self.goto00(i32::from(x), i32::from(z), rate);
    }

    fn goto_soba(&mut self, rate: i32) {
        let [x, z] = self.p.soba;
        self.goto00(i32::from(x), i32::from(z), rate);
    }

    fn player_xz(&self) -> (i32, i32) {
        (self.w.player[0], self.w.player[2])
    }

    fn enemy_near(&self, within: u16) -> bool {
        self.w.enemy.is_some() && self.w.enemy_dist < within
    }

    fn clip(&mut self, bank: Bank, clip: u8) {
        let work = &mut *self.m.work;
        work.bank = bank;
        work.move_no = clip;
        work.move_cnt = 0;
        work.hokan = CLIP_BLEND;
    }

    fn joint(&mut self, bank: Bank) -> bool {
        self.m.work.bank = bank;
        self.m.joint_move_late_reversed(STEP, false)
    }

    fn spd(&mut self, bank: Bank) {
        self.m.work.bank = bank;
        self.m.spd_get_reversed(false);
    }

    fn life_column(&self) -> usize {
        usize::from(self.m.work.d_life_u.min(2))
    }

    /// The wall check at the top of the walk's and the run's decisions: 60
    /// ticks pushed out of a wall start a wall turn on the run.
    fn wall_turn(&mut self) -> bool {
        if self.sca_hit() && self.p.state & STATE_WALL_TURN == 0 {
            self.p.stuck = self.p.stuck.wrapping_add(1);
            if self.p.stuck > WALL_TICKS {
                self.p.state |= STATE_WALL_TURN;
                let a = self.w.rnd.draw();
                let b = self.w.rnd.draw();
                self.p.stuck = WALL_TURN_TICKS;
                let bit = u16::from((b >> (a & 3)) & 1);
                self.p.turn = self
                    .m
                    .player
                    .angle
                    .wrapping_sub(bit * 0x100)
                    .wrapping_add(0x880);
                if let Some(enemy) = self.w.enemy.filter(|_| self.w.enemy_dist < FOUND) {
                    self.p.turn = self.direction_to(enemy[0], enemy[2]);
                    self.p.stuck = WALL_TURN_ENEMY_TICKS;
                }
                self.set(MV_RUN, 0);
                return true;
            }
        }
        if !self.sca_hit() && self.p.state & STATE_WALL_TURN == 0 {
            self.p.stuck = 0;
        }
        false
    }

    /// `Sherry_mv00_br` (`0x4f4ed0`): from the stand, walk or turn round to a
    /// point over 1500 away, sidestep under 200, get lost over 8000, and
    /// while lost come back under 3000.
    fn sherry_mv00_br(&mut self) {
        let [x, z] = self.p.soba;
        let ahead = self.ahead(i32::from(x), i32::from(z), 0x400);
        if self.routine2() == 0 {
            let fraction = life_fraction(self.p.life, self.p.max_life);
            let mut d_life_u = u8::from(self.w.poisoned);
            if fraction < 2000 {
                d_life_u = 1;
            }
            if fraction < 800 {
                d_life_u = 2;
            }
            self.m.work.d_life_u = d_life_u;
        }
        let dist = self.p.dist;
        if !self.lost() {
            if dist < LOST {
                if dist > WALK {
                    self.set(if ahead { MV_WALK } else { MV_TURN_ROUND }, 0);
                    return;
                }
            } else {
                self.get_lost();
            }
            if dist < SIDESTEP && self.routine2() < 2 {
                self.set(MV_STAND, 2);
            }
            if dist > STEP_BACK && self.routine2() > 1 {
                self.set(MV_STAND, 0);
            }
        } else {
            if self.enemy_near(LOST_ENEMY_NEAR) {
                self.set(MV_WALK, 0);
            }
            if dist < FOUND {
                if self.routine2() == 5 {
                    self.m.work.routine[2] = 6;
                }
                if self.routine2() == 1 {
                    self.m.work.routine[2] = 0;
                    self.p.state &= !STATE_LOST;
                }
            }
        }
    }

    /// `Sherry_mv00` (`0x4f5050`): the stand, the sidestep, and while lost the
    /// call: its clip, held at its start, then played back.
    fn sherry_mv00(&mut self) {
        loop {
            match self.routine2() {
                0 => {
                    self.m.work.routine[2] = 1;
                    let clip = STAND_CLIPS[self.life_column()];
                    self.clip(Bank::Weapon, clip);
                    self.m.work.neck.flags &= 0xED;
                    if self.lost() {
                        self.clip(Bank::Pld, CALL_CLIP);
                        self.m.work.neck.flags |= 0x12;
                        self.m.work.routine[2] = 4;
                        self.m.work.spd[0] = 0;
                        self.m.work.spd[1] = 0;
                        let water = i32::from(self.w.water);
                        if water != 0 && water < self.w.ground - 500 {
                            self.clip(Bank::Weapon, clip);
                            self.m.work.routine[2] = 1;
                        }
                        return;
                    }
                }
                1 => {
                    self.joint(Bank::Weapon);
                    return;
                }
                2 => {
                    let (x, z) = self.player_xz();
                    let clip = if self.goto00_ck(x, z, 1) < 0 {
                        SIDESTEP_OTHER_CLIP
                    } else {
                        SIDESTEP_CLIP
                    };
                    self.clip(Bank::Weapon, clip);
                    self.m.work.routine[2] = 3;
                    self.p.timer3 = 0;
                }
                3 => {
                    if self.joint(Bank::Weapon) && self.p.timer3 == 0 {
                        self.p.timer3 = 1;
                        self.m.work.move_no += 1;
                    }
                    return;
                }
                4 => {
                    let ended = self.joint(Bank::Pld);
                    self.m.work.routine[2] += u8::from(ended);
                    return;
                }
                5 => {
                    self.m.work.neck.flags &= 0xED;
                    if self.p.dist > CALL_NECK {
                        self.m.work.neck.flags |= 0x12;
                    }
                    self.clip(Bank::Pld, CALL_CLIP);
                    return;
                }
                6 => {
                    self.m.work.bank = Bank::Pld;
                    let ended = self.m.joint_move_late_reversed(STEP, true);
                    self.m.work.routine[2] += u8::from(ended);
                    return;
                }
                7 => {
                    self.p.state &= !STATE_LOST;
                    self.m.work.routine[2] = 0;
                    return;
                }
                _ => return,
            }
        }
    }

    /// `Sherry_mv01_br` (`0x4f5250`): the walk's decision.
    fn sherry_mv01_br(&mut self) {
        if self.wall_turn() {
            return;
        }
        let dist = self.p.dist;
        if !self.lost() {
            if dist > LOST {
                self.set(MV_STAND, 0);
                self.get_lost();
                return;
            }
            if dist < ARRIVED {
                self.set(MV_FACE_AHEAD, 0);
                return;
            }
            if dist > RUN {
                self.set(MV_RUN, 0);
                return;
            }
            if dist < RUN && self.w.player_routine == 2 && !self.cdir_ck2() {
                self.set(MV_DODGE, 0);
                return;
            }
            match self.w.enemy {
                Some(enemy)
                    if self.w.enemy_dist < WALK
                        && self.ahead(enemy[0], enemy[2], 0x4B0)
                        && !self.sca_hit() =>
                {
                    self.goto00(enemy[0], enemy[2], -0x28);
                }
                _ => self.goto_dest(0x28),
            }
            self.updown_ck();
        } else {
            if let Some(enemy) = self.w.enemy {
                if self.w.enemy_dist < 5000 {
                    self.set(MV_RUN, 0);
                    return;
                }
                if self.w.enemy_dist > 7000 {
                    self.set(MV_STAND, 0);
                    return;
                }
                self.goto00(enemy[0], enemy[2], -0x28);
            }
            if dist < LOST_WALK_BACK {
                self.p.state &= !STATE_LOST;
            }
        }
    }

    /// `Cdir_ck2(w, pl)` (`0x4b26a0`): Claire faces away from her.
    fn cdir_ck2(&self) -> bool {
        let (x, z) = self.player_xz();
        let pos = self.pos();
        let to_player = direction_ck(pos[0], pos[2], x, z);
        (self
            .w
            .player_angle
            .wrapping_sub(to_player)
            .wrapping_add(0x400))
            & 0xFFF
            < 0x800
    }

    /// `Sherry_mv02_br` (`0x4f54b0`): the run's decision, and the wall turn.
    fn sherry_mv02_br(&mut self) {
        if self.wall_turn() {
            return;
        }
        if self.p.state & STATE_WALL_TURN != 0 && self.p.stuck != 0 {
            self.goto01(self.p.turn, 0x50);
            self.p.stuck -= 1;
            if self.p.stuck == 0 {
                self.p.state &= !STATE_WALL_TURN;
            }
            return;
        }
        let dist = self.p.dist;
        if self.lost() {
            if let Some(enemy) = self.w.enemy {
                if self.w.enemy_dist > 7000 {
                    self.set(MV_STAND, 0);
                    return;
                }
                self.goto00(enemy[0], enemy[2], -0x38);
            }
            if dist < FOUND {
                self.p.state &= !STATE_LOST;
            }
            return;
        }
        if dist > LOST {
            self.set(MV_STAND, 0);
            self.get_lost();
            return;
        }
        if dist < STOP_RUN {
            self.set(MV_WALK, 0);
            return;
        }
        match self.w.enemy {
            Some(enemy)
                if self.w.enemy_dist < RUN
                    && self.ahead(enemy[0], enemy[2], 0x400)
                    && !self.sca_hit() =>
            {
                self.goto00(enemy[0], enemy[2], -0x48);
            }
            _ => self.goto_dest(0x48),
        }
        self.updown_ck();
    }

    /// `Sherry_mv0c_br` (`0x4f56f0`): the turn round. The nearest-enemy cache
    /// is read without a test for an enemy, so a stale distance under 1500
    /// (0 in a process that has never seen one) turns her toward her own
    /// position, which `Direction_ck` makes 0x400.
    fn sherry_mv0c_br(&mut self) {
        if self.routine2() == 0 {
            self.p.timer0 = 0x32;
        }
        if self.p.timer0 != 0 {
            self.p.timer0 -= 1;
        }
        let dist = self.p.dist;
        if dist < TURN_ROUND_ARRIVED {
            self.set(MV_FACE_AHEAD, 0);
            return;
        }
        if self.p.timer0 < 0x1E && dist > STOP_RUN {
            self.set(MV_FACE_POINT, 0);
            return;
        }
        if self.p.timer0 == 0 {
            self.set(MV_FACE_POINT, 0);
        }
        if self.w.enemy_dist < WALK && !self.sca_hit() {
            let [x, _, z] = self.pos();
            self.goto00(x, z, 0x30);
        } else {
            self.goto_dest(-0x30);
        }
    }

    /// `Aida_mv03_br` (`0x4f3710`): turn out of a running Claire's way, until
    /// she faces as Claire does.
    fn aida_mv03_br(&mut self) {
        if self.routine2() == 0 {
            self.p.timer1 = self.player_turn_to_her() * 0x60;
            self.p.timer2 = 0;
        }
        if turn_within(self.m.player.angle, self.w.player_angle, 0x200) == 0
            && self.p.timer2 > DODGE_MIN_TICKS
        {
            self.set(MV_STAND, 0);
        }
        if self.p.timer2 > DODGE_MAX_TICKS {
            self.set(MV_STAND, 0);
        }
        self.m.player.angle = self.m.player.angle.wrapping_add(self.p.timer1 as u16);
    }

    /// `Goto00_ck(player_work, w.Pos, 1)`: which way Claire would turn to her,
    /// 1 when she faces her already.
    fn player_turn_to_her(&self) -> i16 {
        let (x, z) = self.player_xz();
        let pos = self.pos();
        let to_her = direction_ck(x, z, pos[0], pos[2]);
        match turn_within(self.w.player_angle, to_her, 1) {
            0 => 1,
            turn => turn as i16,
        }
    }

    /// `Aida_mv04_br` (`0x4f38a0`): turn on the spot to face Claire.
    fn aida_mv04_br(&mut self) {
        let (x, z) = self.player_xz();
        if self.goto00_ck(x, z, 0x50) == 0 {
            self.set(MV_STAND, 0);
        } else {
            self.goto00(x, z, 0x50);
        }
    }

    /// `Aida_mv0b_br` (`0x4f4840`): turn to Claire's facing.
    fn aida_mv0b_br(&mut self) {
        let facing = self.w.player_angle;
        if turn_within(self.m.player.angle, facing, 0x50) == 0 {
            self.set(MV_STAND, 0);
        } else {
            self.goto01(facing, 0xA0);
        }
    }

    /// `Aida_mv0d_br` (`0x4f4a50`): turn on the spot to the follow point.
    fn aida_mv0d_br(&mut self) {
        let [x, z] = self.p.soba;
        if self.goto00_ck(i32::from(x), i32::from(z), 0x50) == 0 {
            self.set(MV_WALK, 0);
        } else {
            self.goto_soba(0xA0);
        }
    }

    /// `Aida_mv01` (`0x4f3250`): the walk. Sixty ticks near the point at a
    /// wall, or near Claire, stop her.
    fn aida_mv01(&mut self) {
        if self.routine2() == 0 {
            self.m.work.routine[2] = 1;
            self.p.timer0 = WALK_STOP_TICKS;
            self.p.timer1 = WALK_STOP_TICKS;
            self.clip(Bank::Weapon, WALK_CLIPS[self.life_column()]);
        }
        self.spd(Bank::Weapon);
        self.joint(Bank::Weapon);
        self.m.footstep(0);
        if self.p.dist < WALK && self.sca_hit() {
            self.p.timer0 -= 1;
            if self.p.timer0 == 0 {
                self.set(MV_FACE, 0);
            }
        } else {
            self.p.timer0 = WALK_STOP_TICKS;
        }
        if self.w.distance < u32::from(WALK) {
            self.p.timer1 -= 1;
            if self.p.timer1 == 0 {
                self.set(MV_FACE, 0);
            }
        } else {
            self.p.timer1 = WALK_STOP_TICKS;
        }
        self.slow_in_water();
        self.m.add_speed_xz();
    }

    /// `Aida_mv01` and `Aida_mv02` slow down in deep water. Unlike the stand,
    /// they don't test that there is water.
    fn slow_in_water(&mut self) {
        if i32::from(self.w.water) < self.w.ground - 500 {
            let spd = self.m.work.spd[0];
            self.m.work.spd[0] = spd - spd / 3;
        }
    }

    /// `Aida_mv02` (`0x4f3620`): the run.
    fn aida_mv02(&mut self) {
        if self.routine2() == 0 {
            self.m.work.routine[2] = 1;
            self.clip(Bank::Weapon, RUN_CLIPS[self.life_column()]);
        }
        self.spd(Bank::Weapon);
        self.joint(Bank::Weapon);
        self.m.footstep(1);
        if self.m.work.hokan != 0 {
            self.m.work.spd[0] >>= 1;
        }
        self.slow_in_water();
        self.m.add_speed_xz();
    }

    /// `Aida_mv03` (`0x4f37a0`): a turning walk of the main bank, `Timer1` a
    /// tick when `Aida_mv03_br` decides.
    fn aida_mv03(&mut self) {
        if self.routine2() == 0 {
            self.clip(Bank::Pld, MV03_CLIPS[self.life_column()]);
            self.m.work.routine[2] = 1;
            self.p.timer2 = 0;
            self.p.timer1 = self.player_turn_to_her() << 7;
            self.p.timer0 = 0x3C;
        }
        self.spd(Bank::Pld);
        self.joint(Bank::Pld);
        self.m.footstep(0);
        self.p.timer2 = self.p.timer2.wrapping_add(1);
        if self.m.work.hokan != 0 {
            self.m.work.spd[0] >>= 1;
        }
        self.m.add_speed_xz();
    }

    /// `Aida_mv04` (`0x4f38f0`): the walk's clip on the spot, while a
    /// decision turns her.
    fn aida_mv04(&mut self) {
        if self.routine2() == 0 {
            self.m.work.routine[2] = 1;
            self.m.work.spd = [0; 3];
            self.clip(Bank::Weapon, WALK_CLIPS[self.life_column()]);
        }
        self.joint(Bank::Weapon);
        self.m.footstep(0);
    }

    /// `Oma_Spl_updown_ck` (`0x4ce0c0`): a step to climb or drop in front of
    /// her, with Sherry's reach of 600 for both. Not ported: where one is
    /// found it says so once and she walks on.
    fn updown_ck(&mut self) {
        use crate::sca::{FLOOR_HEIGHT, INFO_DOWN, INFO_UP};
        let Some(sca) = self.m.tick.room.sca.as_ref() else {
            return;
        };
        const REACH: i32 = 600;
        let mask = crate::sca::CHARACTER_MASK_LOW;
        let facing = self.m.player.angle.wrapping_add(0x200) & 0xFC00;
        let [x, y, z] = self.pos();
        let floor = self.m.work.floor;
        let (dx, dz) = crate::player2::rotate(facing, REACH, 0);
        let up = [x + dx, y - FLOOR_HEIGHT, z + dz];
        let down = [x + dx, y, z + dz];
        if crate::sca::info_at(sca, mask, up, floor) & INFO_UP != 0
            || crate::sca::info_at(sca, mask, down, floor) & INFO_DOWN != 0
        {
            static LOGGED: std::sync::Once = std::sync::Once::new();
            LOGGED.call_once(|| crate::logging::write("RE2 partner: climb not ported"));
        }
    }
}

/// `Life * 4096 / Max_life`, the stand's measure of her health.
fn life_fraction(life: i16, max_life: i16) -> i32 {
    if max_life == 0 {
        return 0;
    }
    i32::from(life) * 4096 / i32::from(max_life)
}

fn log_once(routine: u8) {
    static LOGGED: std::sync::Once = std::sync::Once::new();
    LOGGED.call_once(|| {
        crate::logging::write(&format!(
            "RE2 partner: Routine_1 {routine:#x} is not ported"
        ))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Clip, ClipFrame, Keyframe, Skeleton};
    use crate::player::{Input, Options, PlayerState};
    use crate::player2::{Banks, Tick, Work, rotate};
    use crate::state::{RoomId, RoomState};

    /// Twelve clips of four frames, each moving 10 along the root.
    fn bank() -> (Vec<Clip>, Vec<Keyframe>) {
        let keyframes = (0..4)
            .map(|index| Keyframe {
                offset: [0; 3],
                vertex: [10 * (index as i16 + 1), 0, 0],
                rotations: Vec::new(),
            })
            .collect();
        let clip = Clip {
            frames: (0..4)
                .map(|index| ClipFrame {
                    keyframe: index,
                    timing: 0,
                    flags: 0,
                })
                .collect(),
        };
        (vec![clip; 12], keyframes)
    }

    /// Sherry at the origin facing 0, Claire 1000 along x facing 0x800.
    struct Rig {
        work: Work,
        body: PlayerState,
        room: RoomState,
        skeleton: Skeleton,
        clips: Vec<Clip>,
        keyframes: Vec<Keyframe>,
        partner: Partner,
        kind: u16,
        rnd: Rnd,
        timer: u8,
        player: [i32; 3],
        player_angle: u16,
        player_routine: u8,
        distance: u32,
        enemy_dist: u16,
        poisoned: bool,
    }

    impl Rig {
        fn new() -> Self {
            let room = RoomState::default();
            let (clips, keyframes) = bank();
            let mut body = crate::player::spawn(RoomId::parse("21B1").unwrap(), &room);
            body.pos = [0; 3];
            body.angle = 0;
            let mut partner = Partner::placed();
            partner.spl_in45(true, 200);
            let mut work = Work::default();
            work.routine = [1, 0, 0, 0];
            Self {
                work,
                body,
                room,
                skeleton: Skeleton::default(),
                clips,
                keyframes,
                partner,
                kind: 0x400,
                rnd: Rnd(0x1234),
                timer: SPL_POS_CK_START,
                player: [1000, 0, 0],
                player_angle: 0x800,
                player_routine: 0,
                distance: 1000,
                enemy_dist: 0xFFFF,
                poisoned: false,
            }
        }

        /// One tick of `Spl_Auto`, or with `decide_only` the stand's decision.
        fn run(&mut self, decide_only: bool) {
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
                id: 0x45,
                life: 200,
                poisoned: false,
                stopped: false,
                weapon: Default::default(),
                necks: [None; crate::player2::NECK_WORKS],
                targets: &[],
                neck_targets: &[],
                enemies: 0,
                pushing: false,
                key_trg: 0,
            };
            let mut world = PartnerWorld {
                player: self.player,
                player_angle: self.player_angle,
                player_routine: self.player_routine,
                distance: self.distance,
                enemy: None,
                enemy_dist: self.enemy_dist,
                rnd: &mut self.rnd,
                pos_ck_timer: &mut self.timer,
                water: 0,
                ground: 0,
                poisoned: self.poisoned,
            };
            let mut mover = Mover::new(&mut self.work, &mut self.body, &tick);
            if decide_only {
                let mut auto = Auto {
                    m: &mut mover,
                    p: &mut self.partner,
                    kind: &mut self.kind,
                    w: &mut world,
                };
                auto.sherry_mv00_br();
            } else {
                spl_auto(&mut mover, &mut self.partner, &mut self.kind, &mut world);
            }
        }

        fn tick(&mut self) {
            self.run(false);
        }

        fn routine(&self) -> [u8; 4] {
            self.work.routine
        }
    }

    #[test]
    fn the_follow_point_tables_are_the_executables() {
        assert_eq!(SPL_R_POS[6..12], [700, 1100, -1100, 600, 2176, 1920]);
        assert_eq!(SHERRY_ST_POS, [0, 2, 2, 0, 0, 2, 2, 0, 0, 2, 2, 0]);
    }

    #[test]
    fn the_follow_point_is_the_nearer_side() {
        let at = |turn: i16, ll: i32| {
            let (x, z) = rotate(turn as u16, ll, 0);
            [x, 0, z]
        };
        let point = |turn: i16| {
            let [x, _, z] = at(turn, 700);
            [x as i16, z as i16]
        };
        let soba = |body| soba_pos_set([0; 3], 0, body, 700, 1100, -1100);
        assert_eq!(soba(at(1100, 2000)), point(1100));
        assert_eq!(soba(at(-1100, 2000)), point(-1100));
        // Equally far: the first.
        assert_eq!(soba([0; 3]), point(1100));
    }

    #[test]
    fn off_the_guides_the_follow_point_is_the_player() {
        let guides = [Guide {
            x0: 900,
            z0: -100,
            x1: 1100,
            z1: 100,
            links: 0,
        }];
        let mut partner = Partner::placed();
        let mut work = Work::default();
        let world = RootSys {
            guides: &guides,
            sca: &[],
            player: [1000, 0, 0],
            player_angle: 0,
            player_neck: None,
            partner: true,
        };
        root_ck_sys(&mut partner, &mut work, [0; 3], 0x45, 0x400, &world);
        assert_eq!(partner.soba, [1000, 0]);
        // `SquareRoot0`'s answer for 1000.
        assert_eq!(partner.dist, 999);
        assert_eq!(partner.root_cnt, 0);
        assert_eq!(work.neck.target, Some(0));
    }

    #[test]
    fn the_root_turn_counts_down_for_a_scene_character_too() {
        let mut partner = Partner::placed();
        let mut work = Work::default();
        let world = RootSys {
            guides: &[],
            sca: &[],
            player: [1000, 0, 0],
            player_angle: 0,
            player_neck: None,
            partner: false,
        };
        let turns: Vec<u8> = (0..9)
            .map(|_| {
                root_ck_sys(&mut partner, &mut work, [0; 3], 0x45, 0, &world);
                partner.root_cnt
            })
            .collect();
        assert_eq!(turns, [0, 6, 5, 4, 3, 2, 1, 0, 6]);
        assert_eq!(partner.soba, [0, 0]);
    }

    #[test]
    fn the_enemy_check_runs_every_sixteenth_tick() {
        let mut rig = Rig::new();
        rig.partner.state = 2;
        rig.partner.dist = 1000;
        for left in (0..SPL_POS_CK_START).rev() {
            rig.tick();
            assert_eq!(rig.timer, left);
        }
        rig.tick();
        assert_eq!(rig.timer, SPL_POS_CK_START);
        assert_eq!(rig.kind & 0xF, 0);
        assert_eq!(rig.partner.state, 2, "the kind stays with no enemies");
    }

    #[test]
    fn the_stand_decides_by_distance() {
        let decide = |dist: u16, ahead: bool, routine2: u8| {
            let mut rig = Rig::new();
            // The point is along x: ahead faces 0, behind faces 0x800.
            rig.partner.soba = [1000, 0];
            rig.body.angle = if ahead { 0 } else { 0x800 };
            rig.partner.dist = dist;
            rig.work.routine = [1, 0, routine2, 0];
            rig.run(true);
            (rig.routine(), rig.partner.state, rig.partner.timer0)
        };
        assert_eq!(decide(1501, true, 1).0, [1, MV_WALK, 0, 0]);
        assert_eq!(decide(1501, false, 1).0, [1, MV_TURN_ROUND, 0, 0]);
        assert_eq!(decide(1500, true, 1).0, [1, 0, 1, 0]);
        assert_eq!(
            decide(1500, true, 3).0,
            [1, 0, 0, 0],
            "past 400 the sidestep ends"
        );
        assert_eq!(decide(199, true, 1).0, [1, 0, 2, 0]);
        assert_eq!(decide(401, true, 3).0, [1, 0, 0, 0]);
        let (routine, state, timer0) = decide(8000, true, 1);
        assert_eq!(routine, [1, 0, 1, 0]);
        assert_eq!((state & STATE_LOST, timer0), (STATE_LOST, 0));
    }

    #[test]
    fn the_stands_clip_follows_life() {
        let clip = |life: i16, max_life: i16, poisoned: bool| {
            let mut rig = Rig::new();
            rig.partner.life = life;
            rig.partner.max_life = max_life;
            rig.partner.dist = 1000;
            rig.poisoned = poisoned;
            rig.tick();
            rig.work.move_no
        };
        assert_eq!(clip(200, 200, false), 2);
        assert_eq!(clip(99, 200, false), 2);
        assert_eq!(clip(90, 200, false), 5);
        assert_eq!(clip(30, 200, false), 8);
        assert_eq!(clip(200, 200, true), 5);
        assert_eq!(clip(-1, -1, false), 2, "a scene Sherry's -1 of -1");
    }

    #[test]
    fn lost_sherry_calls_then_comes_back() {
        let mut rig = Rig::new();
        rig.partner.state = STATE_LOST;
        rig.partner.dist = 9000;
        rig.tick();
        assert_eq!(rig.routine(), [1, 0, 4, 0]);
        let motion = (rig.work.move_no, rig.work.move_cnt, rig.work.hokan);
        assert_eq!(
            motion,
            (CALL_CLIP, 0, 7),
            "no pose on the call's first tick"
        );
        assert_eq!(rig.work.bank, Bank::Pld);
        assert_eq!(rig.work.neck.flags & 0x12, 0x12);
        for _ in 0..4 {
            rig.tick();
        }
        assert_eq!(rig.routine()[2], 5, "the call's end holds it");
        rig.tick();
        let held = (rig.routine()[2], rig.work.move_cnt, rig.work.hokan);
        assert_eq!(held, (5, 0, 7));
        rig.partner.dist = 2999;
        rig.tick();
        assert_eq!(rig.routine()[2], 6);
        while rig.routine()[2] == 6 {
            rig.tick();
        }
        assert_eq!(rig.routine()[2], 7, "the call played back");
        rig.tick();
        assert_eq!(rig.routine(), [1, 0, 0, 0]);
        assert_eq!(rig.partner.state & STATE_LOST, 0);
    }

    #[test]
    fn the_walk_decides() {
        let walk = |dist: u16, running_at_her: Option<bool>| {
            let mut rig = Rig::new();
            rig.work.routine = [1, MV_WALK, 1, 0];
            rig.partner.dist = dist;
            if let Some(at_her) = running_at_her {
                rig.player_routine = 2;
                // Claire is along +x: facing 0x800 faces her.
                rig.player_angle = if at_her { 0x800 } else { 0 };
            }
            rig.tick();
            (rig.routine(), rig.partner.state)
        };
        let (routine, state) = walk(8001, None);
        assert_eq!(routine[..2], [1, MV_STAND]);
        assert_eq!(state & STATE_LOST, STATE_LOST);
        assert_eq!(walk(149, None).0[..2], [1, MV_FACE_AHEAD]);
        assert_eq!(walk(2001, None).0[..2], [1, MV_RUN]);
        assert_eq!(walk(1999, Some(true)).0[..2], [1, MV_DODGE]);
        assert_eq!(walk(1999, Some(false)).0[..2], [1, MV_WALK]);
    }

    #[test]
    fn the_run_slows_to_a_walk_under_1000() {
        let mut rig = Rig::new();
        rig.work.routine = [1, MV_RUN, 1, 0];
        rig.partner.dist = 1000;
        rig.tick();
        assert_eq!(rig.routine()[1], MV_RUN);
        rig.partner.dist = 999;
        rig.tick();
        assert_eq!(rig.routine(), [1, MV_WALK, 1, 0]);
    }

    #[test]
    fn a_wall_turns_her_after_60_ticks() {
        let mut rig = Rig::new();
        rig.work.routine = [1, MV_WALK, 1, 0];
        rig.partner.dist = 1600;
        rig.partner.sca_hit = true;
        rig.distance = 3000;
        for _ in 0..60 {
            rig.tick();
            assert_eq!(rig.routine()[1], MV_WALK);
        }
        assert_eq!(rig.partner.stuck, 60);
        let mut rnd = rig.rnd;
        let angle = rig.body.angle;
        rig.tick();
        let (a, b) = (rnd.draw(), rnd.draw());
        assert_eq!(rig.rnd, rnd, "two draws");
        let bit = u16::from((b >> (a & 3)) & 1);
        let turn = angle.wrapping_sub(bit * 0x100).wrapping_add(0x880);
        assert_eq!(rig.partner.turn, turn);
        assert_eq!(rig.partner.stuck, 15);
        assert_eq!(rig.routine(), [1, MV_RUN, 1, 0]);
        assert_ne!(rig.partner.state & STATE_WALL_TURN, 0);
        for left in (0..15).rev() {
            let before = rig.body.angle;
            rig.tick();
            assert_eq!(rig.body.angle, goto(before, turn, 0x50));
            assert_eq!(rig.partner.stuck, left);
        }
        assert_eq!(rig.partner.state & STATE_WALL_TURN, 0);
    }

    #[test]
    fn the_walk_stops_after_60_ticks_near_claire() {
        let mut rig = Rig::new();
        rig.work.routine = [1, MV_WALK, 1, 0];
        rig.partner.dist = 1000;
        rig.distance = 1499;
        rig.partner.timer1 = 60;
        for _ in 0..59 {
            rig.tick();
            assert_eq!(rig.routine()[1], MV_WALK);
        }
        rig.distance = 1500;
        rig.tick();
        assert_eq!(rig.partner.timer1, 60, "1500 starts the count again");
        rig.distance = 1499;
        for _ in 0..59 {
            rig.tick();
        }
        assert_eq!(rig.routine()[1], MV_WALK);
        rig.tick();
        assert_eq!(rig.routine(), [1, MV_FACE, 0, 0]);
    }

    #[test]
    fn the_turn_round_counts_from_60() {
        let mut rig = Rig::new();
        rig.work.routine = [1, MV_TURN_ROUND, 0, 0];
        rig.partner.dist = 500;
        rig.tick();
        assert_eq!(rig.partner.timer0, 0x3C);
        assert_eq!(rig.routine(), [1, MV_TURN_ROUND, 1, 0]);
    }

    #[test]
    fn a_stale_enemy_distance_turns_her_round_toward_0x400() {
        let mut rig = Rig::new();
        rig.work.routine = [1, MV_TURN_ROUND, 1, 0];
        rig.partner.timer0 = 0x3C;
        rig.partner.dist = 500;
        rig.enemy_dist = 0;
        rig.body.angle = 0;
        rig.tick();
        assert_eq!(rig.body.angle, 0x30);
        // A cache past 1500: away from `Dest` instead.
        let mut rig = Rig::new();
        rig.work.routine = [1, MV_TURN_ROUND, 1, 0];
        rig.partner.timer0 = 0x3C;
        rig.partner.dist = 500;
        rig.work.dest = [1000, 0];
        rig.body.angle = 0x700;
        rig.tick();
        assert_eq!(rig.body.angle, 0x730);
    }

    #[test]
    fn the_new_state_runs_in_the_same_tick() {
        let mut rig = Rig::new();
        rig.work.routine = [1, 0, 1, 0];
        rig.partner.soba = [1000, 0];
        rig.partner.dist = 1501;
        rig.tick();
        assert_eq!(rig.routine(), [1, MV_WALK, 1, 0]);
        assert_eq!((rig.work.move_no, rig.work.move_cnt), (WALK_CLIPS[0], 1));
        assert_eq!(rig.partner.timer0, 60);
    }

    #[test]
    fn the_dodge_turns_0x60_then_0x80() {
        let mut rig = Rig::new();
        rig.work.routine = [1, MV_DODGE, 0, 0];
        rig.partner.dist = 1000;
        // Claire faces her exactly: `Goto00_ck` is 0, so the turn is +.
        rig.player_angle = 0x800;
        rig.body.angle = 0x100;
        rig.tick();
        assert_eq!(rig.body.angle, 0x160);
        rig.tick();
        assert_eq!(rig.body.angle, 0x1E0);
    }

    #[test]
    fn spl_in45_gives_the_partner_her_saved_life() {
        let mut partner = Partner::placed();
        partner.timer2 = 35;
        partner.spl_in45(false, 150);
        assert_eq!((partner.life, partner.max_life), (-1, -1));
        partner.spl_in45(true, 150);
        let words = (partner.life, partner.max_life, partner.timer2);
        assert_eq!(words, (150, 200, 35));
    }
}
