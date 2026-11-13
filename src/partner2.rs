//! RE2's partner: `Spl_Auto` and `Sherry_move`, for `splayer_work`.
//!
//! When a script places Sherry (0x45 or 0x4F) as the partner (`Type` 0x400,
//! with `fg_status`'s `0x10000000`), `_Spl` runs her own routine: each tick
//! `Spl_Root_ck_sys` picks a point beside Claire to follow ([`root_ck_sys`]),
//! and `Sherry_move` (`spl_auto`) stands, walks, runs or turns toward it.
//! Every sixteenth tick she looks where the nearest enemies are
//! ([`spl_enemy_ck`], `Spl_em_pos_ck`) to choose a point beside Claire or
//! behind her, and her states turn her away from the nearest. Her walk and
//! run look for a box to climb or an edge to jump down from in front of her
//! (`Oma_Spl_updown_ck`), which `Aida_mv08` and `Aida_mv09` climb and jump,
//! and the water she stands in (`Water`, from `Sce_at`'s water areas) slows
//! them, keeps her from calling and makes her footfalls ripple.

use crate::player2::{Bank, Mover, direction_ck, goto};
use crate::state::{Guide, ScaRect};
use crate::zombie2::{Rnd, RootCk, RootWorld, root_ck, sca_ck_line, turn_within};

/// `Spl_r_pos` (`0x53cda0`): the follow points' length and two angles from
/// Claire's facing, beside her then behind her, Ada's row then Sherry's.
pub const SPL_R_POS: [i16; 12] = [
    1000, 1300, -1300, 800, 1920, 1920, 700, 1100, -1100, 600, 2176, 1920,
];
/// `Sherry_St_pos` (`0x53d148`): the follow point's kind for each answer of
/// `Spl_em_pos_ck`, 0 beside and 2 behind.
pub const SHERRY_ST_POS: [u8; 12] = [0, 2, 2, 0, 0, 2, 2, 0, 0, 2, 2, 0];
/// `Ada_St_pos` (`0x53cde4`): the same for Ada.
pub const ADA_ST_POS: [u8; 12] = [0, 0, 0, 2, 2, 0, 0, 2, 2, 2, 2, 15];
/// `Spl_em_pos_ck`'s reach: the enemies nearer than this count as close.
const EM_POS_NEAR: u16 = 3000;
/// `L_spl`'s top two bits: which of the nearest three an enemy is (1, 2, 3).
pub const L_SPL_LISTED: u32 = 0xC000_0000;
/// The mask `Spl_enemy_ck`'s line of sight is blocked by.
const SIGHT_MASK: u16 = 0x2080;

/// What [`spl_enemy_ck`] reads of an enemy work, and the two words it writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnemyView {
    pub be_flg: u32,
    /// `Type`: a high byte with `0xC0` keeps it off the list.
    pub kind: u16,
    pub life: i16,
    pub pos: [i32; 3],
    /// The part `+0x1C1` names, where the line of sight ends.
    pub neck: Option<[i32; 3]>,
    /// `L_spl` (`+0x1F4`): its distance from her, with [`L_SPL_LISTED`].
    pub l_spl: u32,
    /// `Dir_spl` (`+0x1F8`): the way from it to her.
    pub dir_spl: u16,
}

/// `Spl_enemy_ck` (`0x4f5b70`): with enemies in the room (`count`, the room's
/// `BYTE_00989eeb`), every enemy in play and not held off (`Type`'s
/// `0xC000`) takes its distance from her in `L_spl`, and the three nearest
/// she sees (`Sca_ck_line` from her head to its with `0x2080`) and that are
/// alive go on the list, nearest first, a tie after the one already there.
/// Each listed enemy has its place in `L_spl`'s top bits and the way from it
/// to her in `Dir_spl`. The list's distances go to `cache`, which a room
/// without enemies leaves alone. The answer is the listed indices.
pub fn spl_enemy_ck(
    enemies: &mut [EnemyView],
    count: u8,
    body: [i32; 3],
    neck: Option<[i32; 3]>,
    sca: &[ScaRect],
    cache: &mut [u16; 3],
) -> [Option<usize>; 3] {
    let mut listed: [Option<usize>; 3] = [None; 3];
    if count == 0 {
        return listed;
    }
    let mut dist = [u32::MAX >> 1; 3];
    for (index, enemy) in enemies.iter_mut().enumerate() {
        if enemy.be_flg & 1 == 0 || (enemy.kind >> 8) & 0xC0 != 0 {
            continue;
        }
        let dx = enemy.pos[0].wrapping_sub(body[0]);
        let dz = enemy.pos[2].wrapping_sub(body[2]);
        let d =
            crate::sca::square_root0(dz.wrapping_mul(dz).wrapping_add(dx.wrapping_mul(dx))) as u32;
        enemy.l_spl = d;
        let seen = match (neck, enemy.neck) {
            (Some(from), Some(to)) => !sca_ck_line(sca, from, to, SIGHT_MASK),
            _ => false,
        };
        if !seen || enemy.life < 0 {
            continue;
        }
        if let Some(place) = (0..3).find(|&place| d < dist[place]) {
            for later in (place + 1..3).rev() {
                dist[later] = dist[later - 1];
                listed[later] = listed[later - 1];
            }
            dist[place] = d;
            listed[place] = Some(index);
        }
    }
    for place in 0..3 {
        cache[place] = dist[place] as u16;
        if let Some(index) = listed[place] {
            let enemy = &mut enemies[index];
            let to = direction_ck(body[0], body[2], enemy.pos[0], enemy.pos[2]);
            enemy.l_spl |= (place as u32 + 1) << 30;
            enemy.dir_spl = to.wrapping_sub(0x800) & 0xFFF;
        }
    }
    listed
}

/// `Spl_em_pos_ck(w, 3000)` (`0x4f5df0`): where the listed enemies are, 0 to
/// 10. With none it is 0. Each close one (under 3000) counts, with whether she
/// faces it within 0x400 (bit `0x10000`) or not (`0x20000`); every one has
/// whether Claire faces it (bit 1) or not (2). With none close the answer is
/// twice the low bits, 4 more for two or more listed; otherwise it is the
/// high bits over `0x8000` less 1, or 3 more for two or more close.
fn em_pos_ck(
    nearest: &[Option<[i32; 3]>; 3],
    dist: &[u16; 3],
    body: [i32; 3],
    angle: u16,
    player: [i32; 3],
    player_angle: u16,
) -> u8 {
    let listed = nearest.iter().filter(|enemy| enemy.is_some()).count();
    if listed == 0 {
        return 0;
    }
    let faces = |from: [i32; 3], facing: u16, to: [i32; 3]| {
        turn_within(facing, direction_ck(from[0], from[2], to[0], to[2]), 0x400) == 0
    };
    let mut bits = 0u32;
    let mut close = 0;
    for (place, enemy) in nearest.iter().take(listed).enumerate() {
        // The count is of the three, so a gap would read a null work; the
        // list is always filled from the front.
        let Some(enemy) = *enemy else { continue };
        if dist[place] < EM_POS_NEAR {
            bits |= if faces(body, angle, enemy) {
                0x10000
            } else {
                0x20000
            };
            close += 1;
        }
        bits |= if faces(player, player_angle, enemy) {
            1
        } else {
            2
        };
    }
    let answer = if bits & 0xFFFF_0000 == 0 {
        bits * 2 + if listed < 2 { 0 } else { 4 }
    } else if close < 2 {
        (bits >> 15) - 1
    } else {
        (bits >> 15) + 3
    };
    answer as u8
}

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
/// Ada's distances (`Ada_mv00_br`, `Ada_mv01_br`, `Ada_mv0c_br`): from the
/// point, the walk's stop, its run and the turn round's stop, and the stand's
/// walk off with `Spl_em_pos_ck`'s answer 10; from the player (`L_pl`), a
/// running and a walking player she turns out of the way of; and the turn
/// round's turn to the point when the player runs off.
const ADA_ARRIVED: u16 = 300;
const ADA_RUN: u16 = 5000;
const ADA_TURN_ROUND_ARRIVED: u16 = 200;
const ADA_KIND_10_WALK: u16 = 500;
const ADA_DODGE_RUNNING: u32 = 4000;
const ADA_DODGE_WALKING: u32 = 1000;
const ADA_TURN_ROUND_PLAYER_RUNS: u16 = 700;
/// Ada's walk turns from a wall for 30 ticks, her run for Sherry's 15.
const ADA_WALK_WALL_TURN_TICKS: i16 = 30;
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
/// The climb (`Aida_mv08`) and the jump down (`Aida_mv09`), which have no
/// decision (`MarniOut` in the first table).
const MV_CLIMB: u8 = 8;
const MV_JUMP_DOWN: u8 = 9;

/// `Oma_Spl_updown_ck`'s reach to a box and to an edge for Sherry's ids
/// (`0x533368` and `0x5333a8` by `Id`, both 600 for 0x45 and 0x4F), which
/// the climb and the jump also step by.
const CLIMB_REACH: i32 = 600;
/// `Routine_2`'s bit for a jump down: `Aida_mv08` squares up and hands over
/// to `Aida_mv09`.
const CLIMB_IS_DROP: u8 = 2;
/// The room bank's clips: the climb, the jump, its fall and its landing.
const CLIMB_CLIP: u8 = 6;
const JUMP_CLIP: u8 = 7;
const JUMP_FALL_CLIP: u8 = 8;
const LAND_CLIP: u8 = 9;
/// Sherry's climb lifts her on frame 99 of its clip and ends its hold on
/// 0x77; her jump leaves the edge on frame 0x1D.
const CLIMB_LIFT_FRAME: u8 = 99;
const CLIMB_LOCK_FRAME: u8 = 0x11;
const CLIMB_UNLOCK_FRAME: u8 = 0x77;
const JUMP_FRAME: u8 = 0x1D;
/// The climb's blend step on the room bank's clip.
const CLIMB_STEP: u16 = 0x400;
/// The jump's drop off the edge and its fall's first speed.
const JUMP_DROP: i32 = 0x226;
const JUMP_FALL_START: i16 = 5;
/// `Sca_ck_hit`'s reach under the falling body.
const FALL_RADIUS: i32 = 400;
/// `Damage_cnt`'s bit for the climb and the jump.
const DAMAGE_LOCK: u8 = 0x80;
/// `Be_flg`'s bit that keeps her from being pushed out of other bodies
/// (`Oba_ck_hit`) while she climbs or jumps.
pub const BE_CLIMBING: u32 = 4;
/// `WORD_00989eee`'s bit the climb's footfalls and the landing raise: the
/// player's noise, as loud as a running step.
const NOISE_CLIMB: u8 = 4;

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
    /// `Poison_timer` (`+0x10A`): `Oma_Spl_updown_ck` keeps her facing
    /// there, and `Aida_mv08` turns her back to it.
    pub climb_dir: u16,
}

impl Partner {
    /// The words `Sce_em_set` leaves on `splayer_work`: its last two words,
    /// the start a room's load gives a character, also land in `Timer0` and
    /// `Timer1` (room 216's Ada has 777 and 72 there).
    pub fn placed(start: i16, start_flags: i16) -> Self {
        Self {
            root_cnt: 1,
            life: -1,
            max_life: -1,
            timer0: start,
            timer1: start_flags,
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
    /// Where the nearest three seen live enemies stand (`0x692fe0`,
    /// `0x692fe4`, `0x692fe8`), nearest first, as [`spl_enemy_ck`] listed
    /// them this tick.
    pub nearest: [Option<[i32; 3]>; 3],
    /// `0x692fc8`'s three distances: [`spl_enemy_ck`] rewrites them only in a
    /// room with enemies, so they can be stale, and the states read the first
    /// without a test for an enemy.
    pub nearest_dist: [u16; 3],
    pub rnd: &'a mut Rnd,
    /// `Spl_pos_ck_timer`.
    pub pos_ck_timer: &'a mut u8,
    /// `Ground`: the floor under her, which `Game_loop` finds after the
    /// last tick's moves (`Sca_ck_hit` with no reach and her `Sc_id`'s
    /// `0x8000`). Her `Water` is her work's.
    pub ground: i16,
    /// Her `Be_flg`, for [`BE_CLIMBING`].
    pub be_flg: u32,
    /// The bits she raises in the player's noise (`WORD_00989eee`).
    pub noise: u8,
    /// The player's noise as the tick found it, and the player's `Spd_x`
    /// (`+0x144`), which Ada's states read.
    pub player_noise: u8,
    pub player_spd_x: i16,
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

/// `Spl_Auto` (`0x4f23b0`): the touch and wait counts, then `Sherry_move`
/// (`0x4f4aa0`) for Sherry's ids or `Ada_move` (`0x4f2420`) for anyone else
/// (`ada`). `kind` is the live `Type`, whose low nibble holds
/// `Spl_em_pos_ck`'s answer.
pub(crate) fn spl_auto(
    mover: &mut Mover<'_, '_>,
    partner: &mut Partner,
    kind: &mut u16,
    world: &mut PartnerWorld<'_>,
    ada: bool,
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
        ada,
    };
    auto.sherry_move();
}

struct Auto<'r, 'a, 'b, 'w> {
    m: &'r mut Mover<'a, 'b>,
    p: &'r mut Partner,
    kind: &'r mut u16,
    w: &'r mut PartnerWorld<'w>,
    /// `Ada_move`: the same as `Sherry_move` but for its two tables.
    ada: bool,
}

impl Auto<'_, '_, '_, '_> {
    /// `Sherry_move`, and `Ada_move`, which differs only in its tables
    /// (`Ada_St_pos`, `0x53cde4`, and `0x53cdb8`): the enemy check every
    /// sixteenth tick, then the decision of `Routine_1` and the state it
    /// leaves, both this tick.
    fn sherry_move(&mut self) {
        // `Spl_enemy_ck` has listed the enemies: the caller runs it, as it
        // reads the other works.
        if *self.w.pos_ck_timer == 0 {
            let was = *self.kind & 0xFF;
            *self.kind &= 0xFF00;
            *self.kind |= u16::from(self.spl_em_pos_ck());
            self.p.state &= !STATE_CHANGED;
            if was & 0xF != *self.kind & 0xF {
                let st_pos = if self.ada {
                    &ADA_ST_POS
                } else {
                    &SHERRY_ST_POS
                };
                let kind = st_pos[usize::from(*self.kind & 0xF)];
                self.p.state = (self.p.state & !STATE_KIND) | u16::from(kind) | STATE_CHANGED;
            }
            *self.w.pos_ck_timer = SPL_POS_CK_START;
        } else {
            *self.w.pos_ck_timer -= 1;
        }
        self.move_type();
    }

    /// `Spl_em_pos_ck(w, 3000)`: where the nearest enemies are, 0 to 10.
    fn spl_em_pos_ck(&self) -> u8 {
        em_pos_ck(
            &self.w.nearest,
            &self.w.nearest_dist,
            self.pos(),
            self.m.player.angle,
            self.w.player,
            self.w.player_angle,
        )
    }

    /// `0x692fe0`: the nearest listed enemy.
    fn enemy(&self) -> Option<[i32; 3]> {
        self.w.nearest[0]
    }

    /// `0x692fc8`'s first distance, which may be stale.
    fn enemy_dist(&self) -> u16 {
        self.w.nearest_dist[0]
    }

    /// `Sherry_Move_type`'s body, the same for every answer.
    fn move_type(&mut self) {
        if self.ada {
            self.ada_move_type();
            return;
        }
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
            MV_CLIMB => self.aida_mv08(),
            MV_JUMP_DOWN => self.aida_mv09(),
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
        self.enemy().is_some() && self.enemy_dist() < within
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
                if let Some(enemy) = self.enemy().filter(|_| self.enemy_dist() < FOUND) {
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
                        let water = self.m.work.water;
                        if water != 0 && self.deep_water() {
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
            match self.enemy() {
                Some(enemy)
                    if self.enemy_dist() < WALK
                        && self.ahead(enemy[0], enemy[2], 0x4B0)
                        && !self.sca_hit() =>
                {
                    self.goto00(enemy[0], enemy[2], -0x28);
                }
                _ => self.goto_dest(0x28),
            }
            self.updown_ck();
        } else {
            if let Some(enemy) = self.enemy() {
                if self.enemy_dist() < 5000 {
                    self.set(MV_RUN, 0);
                    return;
                }
                if self.enemy_dist() > 7000 {
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
            if let Some(enemy) = self.enemy() {
                if self.enemy_dist() > 7000 {
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
        match self.enemy() {
            Some(enemy)
                if self.enemy_dist() < RUN
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
        if self.enemy_dist() < WALK && !self.sca_hit() {
            let [x, _, z] = self.pos();
            self.goto00(x, z, 0x30);
        } else {
            self.goto_dest(-0x30);
        }
    }

    /// `Ada_Move_type` (`0x4f24e0` for `Spl_em_pos_ck`'s answer 0): the
    /// decision from the answer's table (`0x53cdf0`), then the state from
    /// `0x53ce28`, which is Sherry's but for the stand. The other answers'
    /// tables (`0x53ce60` to `0x53cf78`, with enemies about) have their own
    /// stand, walk, run and turns and aren't ported: they take answer 0's.
    fn ada_move_type(&mut self) {
        if *self.kind & 0xF != 0 {
            log_ada_kind_once(*self.kind & 0xF);
        }
        match self.routine1() {
            MV_STAND => self.ada_mv00_br(),
            MV_WALK => self.ada_mv01_br(),
            MV_RUN => self.ada_mv02_br(),
            MV_DODGE => self.aida_mv03_br(),
            MV_FACE => self.aida_mv04_br(),
            MV_FACE_AHEAD => self.aida_mv0b_br(),
            MV_TURN_ROUND => self.ada_mv0c_br(),
            MV_FACE_POINT => self.aida_mv0d_br(),
            _ => {}
        }
        match self.routine1() {
            MV_STAND => self.ada_mv00(),
            MV_WALK => self.aida_mv01(),
            MV_RUN => self.aida_mv02(),
            MV_DODGE | MV_TURN_ROUND => self.aida_mv03(),
            MV_FACE | MV_FACE_AHEAD | MV_FACE_POINT => self.aida_mv04(),
            MV_CLIMB => self.aida_mv08(),
            MV_JUMP_DOWN => self.aida_mv09(),
            routine => log_once(routine),
        }
    }

    /// `Ada_mv00_br` (`0x4f2ab0`): from the stand, walk (or turn round) to
    /// the point when `Spl_em_pos_ck`'s answer changed or it is over 3000
    /// away, and turn out of the player's way when the player comes at her
    /// (running within 4000, walking within 1000) and she faces the player.
    /// She is never lost and never sidesteps. With an enemy listed and the
    /// player's noise up she turns to it (`0x53d0c0` by its `Id`), which
    /// isn't ported.
    fn ada_mv00_br(&mut self) {
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
        if self.enemy().is_some() && self.w.player_noise & 1 != 0 {
            log_ada_kind_once(0x10);
        }
        let [x, z] = self.p.soba;
        let ahead = self.ahead(i32::from(x), i32::from(z), 0x400);
        let to_point = |auto: &mut Self| auto.set(if ahead { MV_WALK } else { MV_TURN_ROUND }, 0);
        if self.p.state & STATE_CHANGED != 0 {
            to_point(self);
            return;
        }
        if *self.kind & 0xF == 10 && self.p.dist > ADA_KIND_10_WALK {
            to_point(self);
            return;
        }
        if self.p.dist > FOUND {
            to_point(self);
            return;
        }
        let (px, pz) = self.player_xz();
        let coming = |within: u32, routine: u8, auto: &Self| {
            auto.w.distance < within
                && auto.w.player_routine == routine
                && !auto.cdir_ck2()
                && auto.ahead(px, pz, 0x200)
        };
        if coming(ADA_DODGE_RUNNING, 2, self) || coming(ADA_DODGE_WALKING, 1, self) {
            self.set(MV_DODGE, 0);
        }
    }

    /// `Ada_mv00` (`0x4f2d60`): the stand, Sherry's without the call.
    fn ada_mv00(&mut self) {
        if self.routine2() == 0 {
            self.m.work.routine[2] = 1;
            let clip = STAND_CLIPS[self.life_column()];
            self.clip(Bank::Weapon, clip);
        }
        if self.routine2() == 1 {
            self.joint(Bank::Weapon);
        }
    }

    /// The wall check at the top of Ada's walk and run decisions: 60 ticks
    /// pushed out of a wall start a wall turn of `ticks`, in the state she is
    /// in, turning by `rate`. True when the tick's decision is over.
    fn ada_wall_turn(&mut self, ticks: i16, rate: i32) -> bool {
        if self.sca_hit() && self.p.state & STATE_WALL_TURN == 0 {
            self.p.stuck = self.p.stuck.wrapping_add(1);
            if self.p.stuck > WALL_TICKS {
                self.p.state |= STATE_WALL_TURN;
                let a = self.w.rnd.draw();
                let b = self.w.rnd.draw();
                self.p.stuck = ticks;
                let bit = u16::from((b >> (a & 3)) & 1);
                self.p.turn = self
                    .m
                    .player
                    .angle
                    .wrapping_sub(bit * 0x100)
                    .wrapping_add(0x880);
                return true;
            }
        }
        if !self.sca_hit() && self.p.state & STATE_WALL_TURN == 0 {
            self.p.stuck = 0;
        }
        if self.p.state & STATE_WALL_TURN != 0 && self.p.stuck != 0 {
            self.goto01(self.p.turn, rate);
            self.p.stuck -= 1;
            if self.p.stuck == 0 {
                self.p.state &= !STATE_WALL_TURN;
            }
            return true;
        }
        false
    }

    /// `Ada_mv01_br` (`0x4f3110`): the walk's decision. She stops under 300
    /// from the point, runs over 5000, and turns out of a running player's
    /// way within 2000.
    fn ada_mv01_br(&mut self) {
        if self.ada_wall_turn(ADA_WALK_WALL_TURN_TICKS, 0x28) {
            return;
        }
        let dist = self.p.dist;
        if dist < ADA_ARRIVED {
            self.set(MV_FACE_AHEAD, 0);
            return;
        }
        if dist > ADA_RUN {
            self.set(MV_RUN, 0);
            return;
        }
        if dist < RUN && self.w.player_routine == 2 && !self.cdir_ck2() {
            self.set(MV_DODGE, 0);
            return;
        }
        self.goto_dest(0x28);
        self.updown_ck();
    }

    /// `Ada_mv02_br` (`0x4f3510`): the run's decision. She walks under 3000
    /// from the point.
    fn ada_mv02_br(&mut self) {
        if self.ada_wall_turn(WALL_TURN_TICKS, 0x50) {
            return;
        }
        if self.p.dist < FOUND {
            self.set(MV_WALK, 0);
            return;
        }
        self.goto_dest(0x38);
        self.updown_ck();
    }

    /// `Ada_mv0c_br` (`0x4f49b0`): the turn round, Sherry's with other
    /// numbers, and a turn to the point once the player runs off (`Spd_x`
    /// over 100) more than 700 away.
    fn ada_mv0c_br(&mut self) {
        if self.routine2() == 0 {
            self.p.timer0 = 0x3C;
        }
        if self.p.timer0 != 0 {
            self.p.timer0 -= 1;
        }
        let dist = self.p.dist;
        if dist < ADA_TURN_ROUND_ARRIVED {
            self.set(MV_FACE_AHEAD, 0);
            return;
        }
        if self.p.timer0 < 0x1E && dist > STOP_RUN {
            self.set(MV_FACE_POINT, 0);
            return;
        }
        if dist > ADA_TURN_ROUND_PLAYER_RUNS && self.w.player_spd_x > 100 {
            self.set(MV_FACE_POINT, 0);
        }
        if self.p.timer0 == 0 {
            self.set(MV_FACE_POINT, 0);
        }
        self.goto_dest(-0x30);
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
        if self.deep_water() {
            let spd = self.m.work.spd[0];
            self.m.work.spd[0] = spd - spd / 3;
        }
    }

    /// `Water < Ground - 500`: the water's level is over 500 above the floor.
    fn deep_water(&self) -> bool {
        i32::from(self.m.work.water) < i32::from(self.w.ground) - 500
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

    /// `Oma_Spl_updown_ck` (`0x4ce0c0`), without the object models: a box to
    /// climb a quarter turn ahead of her, 600 on and a floor up, or an edge to
    /// jump down from. The climb's and the edge's records are her `0x800`'s,
    /// and the ground under the points is `Sc_id`'s `0x8000`, as the
    /// player's. The edge's ground isn't held under the ceiling, as the
    /// player's is. Either sends her to `Aida_mv08` and keeps her facing.
    fn updown_ck(&mut self) -> bool {
        use crate::sca::{CHARACTER_MASK_LOW, FLOOR_HEIGHT, INFO_DOWN, INFO_UP, PLAYER_MASK};
        let Some(sca) = self.m.tick.room.sca.as_ref() else {
            return false;
        };
        let facing = self.m.player.angle.wrapping_add(0x200) & 0xFC00;
        let [x, y, z] = self.pos();
        let floor = self.m.work.floor;
        let radius = self.m.player.radius;
        let (dx, dz) = crate::player2::rotate(facing, CLIMB_REACH, 0);
        let up = [x + dx, y - FLOOR_HEIGHT, z + dz];
        if crate::sca::info_at(sca, CHARACTER_MASK_LOW, up, floor) & INFO_UP != 0 && floor < 31 {
            let ground = crate::sca::ground_at(sca, up, radius, PLAYER_MASK);
            if i32::from(ground.height as i16) <= up[1] && !ground.blocked {
                self.start_climb(0);
                return true;
            }
        }
        let mut down = [x + dx, y, z + dz];
        down[1] = i32::from(crate::sca::ground_at(sca, down, radius, PLAYER_MASK).height as i16);
        if y < down[1] && crate::sca::info_at(sca, CHARACTER_MASK_LOW, down, floor) & INFO_DOWN != 0
        {
            self.start_climb(CLIMB_IS_DROP);
            return true;
        }
        false
    }

    /// `Routine_1` 8 for the partner (the player's 0x16 and 0x17), `Routine_3`
    /// 0, and her facing in `Poison_timer`.
    fn start_climb(&mut self, two: u8) {
        self.set(MV_CLIMB, two);
        self.p.climb_dir = self.m.player.angle;
    }

    fn joint_step(&mut self, bank: Bank, step: u16) -> bool {
        self.m.work.bank = bank;
        self.m.joint_move_late_reversed(step, false)
    }

    /// `Old_x`, `Old_z` and `Sca_old_*` after a lift or a drop: the walls
    /// push from where she now is.
    fn settle(&mut self) {
        let [x, _, z] = self.m.player.pos;
        self.m.player.collision_mirror = [x, z];
    }

    /// `Snd_se_walk(1, part, w)` on a frame with a footfall, which also
    /// raises the player's noise.
    fn climb_footfall(&mut self) {
        if self.m.now_seq() & crate::player2::FRAME_FOOTSTEP != 0 {
            self.m.footstep(1);
            self.w.noise |= NOISE_CLIMB;
        }
    }

    /// `Aida_mv08` (`0x4f4230`): turn square to the box on the first sub
    /// bank's clip 0, then climb it on the room bank's clip 6, stepping up
    /// and on 600 on its frame 99; a jump down hands over to `Aida_mv09`
    /// once square. The climb ends at the stand.
    fn aida_mv08(&mut self) {
        if self.m.work.routine[3] == 0 {
            self.m.work.routine[3] = 1;
            self.m.player.angle = self.p.climb_dir;
            self.m.work.bank = Bank::Weapon;
            self.m.work.move_no = 0;
            self.m.work.move_cnt = 0;
            self.m.work.hokan = CLIP_BLEND;
            self.m.work.spd[0] = 0x32;
            self.w.be_flg |= BE_CLIMBING;
        }
        match self.m.work.routine[3] {
            1 => {
                self.joint_step(Bank::Weapon, STEP);
                let angle = self.m.player.angle;
                let turn = (angle >> 2) & 0xFF;
                let angle = if angle & 0x200 == 0 {
                    angle.wrapping_sub(turn)
                } else {
                    angle.wrapping_add(turn)
                };
                self.m.player.angle = angle;
                if angle & 0x3E0 == 0 {
                    self.m.work.routine[3] = 2;
                    self.m.player.angle = angle & 0xFF00;
                    if self.routine2() & CLIMB_IS_DROP != 0 {
                        self.m.work.routine[3] = 0;
                        self.m.work.routine[1] += 1;
                    }
                }
            }
            2 | 3 => {
                if self.m.work.routine[3] == 2 {
                    self.m.work.bank = Bank::Room;
                    self.m.work.move_no = CLIMB_CLIP;
                    self.m.work.routine[3] = 3;
                    self.m.work.hokan = 3;
                }
                let frame = self.m.work.move_cnt;
                if frame == CLIMB_LOCK_FRAME {
                    self.m.work.damage_cnt |= DAMAGE_LOCK;
                }
                if frame == CLIMB_LIFT_FRAME {
                    self.m.work.hokan = 0;
                    self.m.work.spd[0] = CLIMB_REACH as i16;
                    self.m.add_speed_xz();
                    self.m.player.pos[1] -= crate::sca::FLOOR_HEIGHT;
                    self.m.work.floor = self.m.work.floor.wrapping_add(1);
                    self.settle();
                }
                if self.m.work.move_cnt == CLIMB_UNLOCK_FRAME {
                    self.m.work.damage_cnt &= !DAMAGE_LOCK;
                    self.w.be_flg &= !BE_CLIMBING;
                }
                let ended = self.joint_step(Bank::Room, CLIMB_STEP);
                self.m.work.routine[3] += u8::from(ended);
                self.climb_footfall();
            }
            4 => self.set(MV_STAND, 0),
            _ => self.climb_footfall(),
        }
    }

    /// `Aida_mv09` (`0x4f4530`): the jump down on the room bank's clips 7 to
    /// 9. She leaves the edge on clip 7's frame 0x1D (600 on, 0x226 down, a
    /// floor down), falls 30 more a tick each tick to within 300 of the
    /// ground under her (`Sca_ck_hit` with a reach of 400), and lands on the
    /// ground `Game_loop` found, with a footfall, to the stand.
    fn aida_mv09(&mut self) {
        match self.m.work.routine[3] {
            0 | 1 => {
                if self.m.work.routine[3] == 0 {
                    self.m.work.routine[3] = 1;
                    self.m.work.bank = Bank::Room;
                    self.m.work.move_no = JUMP_CLIP;
                    self.m.work.hokan = CLIP_BLEND;
                    self.m.work.move_cnt = 0;
                    self.w.be_flg |= BE_CLIMBING;
                }
                self.joint_step(Bank::Room, STEP);
                if self.m.work.move_cnt == JUMP_FRAME {
                    self.m.work.routine[3] = 2;
                }
            }
            2 | 3 => {
                if self.m.work.routine[3] == 2 {
                    self.m.work.routine[3] = 3;
                    self.m.work.damage_cnt |= DAMAGE_LOCK;
                    self.m.work.move_no = JUMP_FALL_CLIP;
                    self.m.work.move_cnt = 0;
                    self.m.work.hokan = 0;
                    self.m.work.spd[0] = CLIMB_REACH as i16;
                    self.m.work.spd[2] = 0;
                    self.m.add_speed_xz();
                    self.m.player.pos[1] += JUMP_DROP;
                    self.m.work.floor = self.m.work.floor.wrapping_sub(1);
                    self.settle();
                    self.p.timer0 = JUMP_FALL_START;
                }
                self.m.player.pos[1] += i32::from(self.p.timer0) * 0x1E;
                self.p.timer0 = self.p.timer0.wrapping_add(1);
                let ground = self.m.tick.room.sca.as_ref().map_or(0, |sca| {
                    crate::sca::ground_at(
                        sca,
                        self.m.player.pos,
                        FALL_RADIUS,
                        crate::sca::PLAYER_MASK,
                    )
                    .height as i16
                });
                self.settle();
                self.m.work.floor = crate::sca::floor_toward_zero(self.m.player.pos[1]);
                self.joint_step(Bank::Room, STEP);
                if i32::from(ground) - 300 < self.m.player.pos[1] {
                    self.m.work.routine[3] = 4;
                }
            }
            4 | 5 => {
                if self.m.work.routine[3] == 4 {
                    self.m.work.routine[3] = 5;
                    self.m.work.move_no = if self.routine2() & 0xC > 4 {
                        JUMP_CLIP
                    } else {
                        LAND_CLIP
                    };
                    self.m.player.pos[1] = i32::from(self.w.ground);
                    self.m.work.move_cnt = 4;
                    self.m.work.hokan = CLIP_BLEND;
                    self.m.work.damage_cnt &= !DAMAGE_LOCK;
                    self.w.be_flg &= !BE_CLIMBING;
                    // `Snd_se_walk(1, 4, w)`: the left foot, part 4.
                    self.m.foot_down(1, false);
                    self.w.noise |= NOISE_CLIMB;
                }
                self.m.work.floor = crate::sca::floor_toward_zero(self.m.player.pos[1]);
                let ended = self.joint_step(Bank::Room, STEP);
                self.m.work.routine[3] += u8::from(ended);
            }
            6 => self.set(MV_STAND, 0),
            _ => {}
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

/// Ada's states with enemies about: the tables for `Spl_em_pos_ck`'s other
/// answers (`what` 1 to 10), and her turn to a listed enemy when the player
/// makes a noise (`what` 0x10).
fn log_ada_kind_once(what: u16) {
    static LOGGED: std::sync::Once = std::sync::Once::new();
    LOGGED.call_once(|| {
        crate::logging::write(&format!(
            "RE2 partner: Ada's states with enemies about ({what:#x}) are not ported"
        ))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Clip, ClipFrame, Keyframe, Skeleton};
    use crate::player::{Input, Options, PlayerState};
    use crate::player2::{Banks, Tick, Work, rotate};
    use crate::state::{RoomId, RoomState, Sca, ScaRect};

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
        nearest: [Option<[i32; 3]>; 3],
        poisoned: bool,
        ground: i16,
        be_flg: u32,
        noise: u8,
        /// `Ada_move` rather than `Sherry_move`.
        ada: bool,
        player_spd_x: i16,
    }

    impl Rig {
        fn new() -> Self {
            let room = RoomState::default();
            let (clips, keyframes) = bank();
            let mut body = crate::player::spawn(RoomId::parse("21B1").unwrap(), &room);
            body.pos = [0; 3];
            body.angle = 0;
            let mut partner = Partner::placed(0, 0);
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
                nearest: [None; 3],
                poisoned: false,
                ground: 0,
                be_flg: 1,
                noise: 0,
                ada: false,
                player_spd_x: 0,
            }
        }

        /// One tick of `Spl_Auto`, or with `decide_only` the stand's decision.
        fn run(&mut self, decide_only: bool) {
            if decide_only {
                self.with_auto(|auto| auto.sherry_mv00_br());
            } else {
                self.with_auto(|auto| {
                    // `spl_auto`'s counts.
                    auto.p.touch = 0;
                    auto.p.wait = auto.p.wait.saturating_sub(1);
                    auto.sherry_move();
                });
            }
        }

        /// `f` on the rig's world, as one tick's `Auto`.
        fn with_auto<T>(&mut self, f: impl FnOnce(&mut Auto<'_, '_, '_, '_>) -> T) -> T {
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
                objects: &[],
                key_trg: 0,
            };
            let mut world = PartnerWorld {
                player: self.player,
                player_angle: self.player_angle,
                player_routine: self.player_routine,
                distance: self.distance,
                nearest: self.nearest,
                nearest_dist: [self.enemy_dist, 0xFFFF, 0xFFFF],
                rnd: &mut self.rnd,
                pos_ck_timer: &mut self.timer,
                ground: self.ground,
                be_flg: self.be_flg,
                noise: 0,
                player_noise: 0,
                player_spd_x: self.player_spd_x,
                poisoned: self.poisoned,
            };
            let mut mover = Mover::new(&mut self.work, &mut self.body, &tick);
            let mut auto = Auto {
                m: &mut mover,
                p: &mut self.partner,
                kind: &mut self.kind,
                w: &mut world,
                ada: self.ada,
            };
            let answer = f(&mut auto);
            self.be_flg = world.be_flg;
            self.noise = world.noise;
            answer
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
        let mut partner = Partner::placed(0, 0);
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
        let mut partner = Partner::placed(0, 0);
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

    fn enemy_at(pos: [i32; 3]) -> EnemyView {
        EnemyView {
            be_flg: 1,
            kind: 0,
            life: 100,
            pos,
            neck: Some([pos[0], -1500, pos[2]]),
            l_spl: 0x1234,
            dir_spl: 7,
        }
    }

    #[test]
    fn the_enemy_check_lists_the_nearest_three_she_sees() {
        let mut enemies = vec![
            enemy_at([4000, 0, 0]),
            enemy_at([1000, 0, 0]),
            enemy_at([0, 0, 2000]),
            enemy_at([-2000, 0, 0]),
            EnemyView {
                life: -1,
                ..enemy_at([500, 0, 0])
            },
            EnemyView {
                kind: 0x4000,
                ..enemy_at([600, 0, 0])
            },
            EnemyView {
                be_flg: 0,
                ..enemy_at([700, 0, 0])
            },
        ];
        let mut cache = [0; 3];
        let neck = Some([0, -1500, 0]);
        let listed = spl_enemy_ck(&mut enemies, 7, [0; 3], neck, &[], &mut cache);
        // A tie goes after the one already listed.
        assert_eq!(listed, [Some(1), Some(2), Some(3)]);
        // `SquareRoot0` is the executable's, a little under.
        let r = |d: i32| crate::sca::square_root0(d * d) as u32;
        assert_eq!(cache, [r(1000), r(2000), r(2000)].map(|d| d as u16));
        let l_spl: Vec<u32> = enemies.iter().map(|enemy| enemy.l_spl).collect();
        assert_eq!(
            l_spl,
            [
                r(4000),
                0x4000_0000 | r(1000),
                0x8000_0000 | r(2000),
                0xC000_0000 | r(2000),
                r(500),
                0x1234,
                0x1234
            ]
        );
        // The way from the enemy to her: from +x, -x.
        assert_eq!(enemies[1].dir_spl, 0x800);
        assert_eq!(enemies[3].dir_spl, 0);
        assert_eq!(enemies[0].dir_spl, 7, "an unlisted enemy keeps its own");
    }

    #[test]
    fn a_room_without_enemies_keeps_the_cache() {
        let mut enemies = vec![enemy_at([1000, 0, 0])];
        let mut cache = [5, 6, 7];
        let listed = spl_enemy_ck(&mut enemies, 0, [0; 3], Some([0; 3]), &[], &mut cache);
        assert_eq!(listed, [None; 3]);
        assert_eq!(cache, [5, 6, 7]);
        assert_eq!(enemies[0].l_spl, 0x1234);
        // With enemies but none seen, the cache is rewritten as unfound.
        let mut unseen = vec![EnemyView {
            neck: None,
            ..enemy_at([1000, 0, 0])
        }];
        spl_enemy_ck(&mut unseen, 1, [0; 3], Some([0; 3]), &[], &mut cache);
        assert_eq!(cache, [0xFFFF; 3]);
        assert_eq!(
            unseen[0].l_spl,
            crate::sca::square_root0(1000 * 1000) as u32
        );
    }

    #[test]
    fn where_the_enemies_are_picks_the_point() {
        // She is at the origin facing +x; Claire 1000 behind her, facing +x.
        let answer = |enemies: &[[i32; 3]]| {
            let mut nearest = [None; 3];
            let mut dist = [0xFFFF; 3];
            for (place, &pos) in enemies.iter().enumerate() {
                nearest[place] = Some(pos);
                dist[place] = crate::sca::square_root0(pos[0] * pos[0] + pos[2] * pos[2]) as u16;
            }
            em_pos_ck(&nearest, &dist, [0; 3], 0, [-1000, 0, 0], 0)
        };
        assert_eq!(answer(&[]), 0);
        // Far: twice Claire's bits, 4 more for two.
        assert_eq!(answer(&[[5000, 0, 0]]), 2);
        assert_eq!(answer(&[[-5000, 0, 0]]), 4);
        assert_eq!(answer(&[[5000, 0, 0], [6000, 0, 0]]), 6);
        assert_eq!(answer(&[[5000, 0, 0], [-6000, 0, 0]]), 10);
        // Close: ahead of her and of Claire, behind her, and two.
        assert_eq!(answer(&[[1000, 0, 0]]), 1);
        assert_eq!(answer(&[[-500, 0, 0]]), 3);
        assert_eq!(answer(&[[1000, 0, 0], [2000, 0, 0]]), 5);
        assert_eq!(answer(&[[1000, 0, 0], [-500, 0, 0]]), 9);
        assert_eq!(
            SHERRY_ST_POS[1], 2,
            "one close ahead puts her behind Claire"
        );
    }

    #[test]
    fn an_enemy_close_by_moves_her_point_behind_claire() {
        let mut rig = Rig::new();
        rig.partner.dist = 1000;
        rig.nearest = [Some([2000, 0, 0]), None, None];
        rig.enemy_dist = 2000;
        for _ in 0..=SPL_POS_CK_START {
            rig.tick();
        }
        assert_eq!(rig.timer, SPL_POS_CK_START);
        // She faces it, Claire (along +x, facing her) doesn't: answer 1.
        assert_eq!(rig.kind & 0xF, 1);
        assert_eq!(
            rig.partner.state & (STATE_KIND | STATE_CHANGED),
            2 | STATE_CHANGED
        );
    }

    #[test]
    fn the_walk_turns_away_from_an_enemy_ahead() {
        let walk = |enemy: Option<[i32; 3]>| {
            let mut rig = Rig::new();
            rig.work.routine = [1, MV_WALK, 1, 0];
            rig.work.dest = [0, 3000];
            rig.partner.dist = 1000;
            rig.distance = 3000;
            rig.nearest = [enemy, None, None];
            rig.enemy_dist = 1000;
            rig.tick();
            rig.body.angle
        };
        let away = goto(0, direction_ck(0, 0, 1000, 300), -0x28);
        assert_eq!(walk(Some([1000, 0, 300])), away);
        assert_eq!(walk(None), goto(0, direction_ck(0, 0, 0, 3000), 0x28));
        assert_ne!(away, walk(None));
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
    fn the_ada_follow_point_table_is_the_executables() {
        assert_eq!(ADA_ST_POS, [0, 0, 0, 2, 2, 0, 0, 2, 2, 2, 2, 15]);
    }

    #[test]
    fn adas_stand_waits_until_3000_and_never_sidesteps_or_gets_lost() {
        let decide = |dist: u16, ahead: bool, state: u16| {
            let mut rig = Rig::new();
            rig.ada = true;
            rig.partner.soba = [1000, 0];
            rig.body.angle = if ahead { 0 } else { 0x800 };
            rig.partner.dist = dist;
            rig.partner.state = state;
            rig.work.routine = [1, 0, 1, 0];
            rig.with_auto(|auto| auto.ada_mv00_br());
            (rig.routine(), rig.partner.state)
        };
        // Sherry walks past 1500, sidesteps under 200 and is lost past 8000.
        assert_eq!(decide(3000, true, 0).0, [1, 0, 1, 0]);
        assert_eq!(decide(150, true, 0).0, [1, 0, 1, 0]);
        assert_eq!(decide(3001, true, 0).0, [1, MV_WALK, 0, 0]);
        assert_eq!(decide(3001, false, 0).0, [1, MV_TURN_ROUND, 0, 0]);
        let (routine, state) = decide(9000, true, 0);
        assert_eq!((routine, state & STATE_LOST), ([1, MV_WALK, 0, 0], 0));
        // A new answer from `Spl_em_pos_ck` sends her to the point, however near.
        assert_eq!(decide(10, true, STATE_CHANGED).0, [1, MV_WALK, 0, 0]);
    }

    #[test]
    fn adas_stand_turns_out_of_the_way_of_a_player_coming_at_her() {
        let decide = |distance: u32, routine: u8, at_her: bool| {
            let mut rig = Rig::new();
            rig.ada = true;
            rig.partner.soba = [0, 0];
            rig.partner.dist = 0;
            rig.work.routine = [1, 0, 1, 0];
            // The player is along +x, and she faces the player.
            rig.player = [1000, 0, 0];
            rig.body.angle = 0;
            rig.distance = distance;
            rig.player_routine = routine;
            rig.player_angle = if at_her { 0x800 } else { 0 };
            rig.with_auto(|auto| auto.ada_mv00_br());
            rig.routine()
        };
        assert_eq!(decide(3999, 2, true), [1, MV_DODGE, 0, 0]);
        assert_eq!(decide(4000, 2, true), [1, 0, 1, 0]);
        assert_eq!(decide(999, 1, true), [1, MV_DODGE, 0, 0]);
        assert_eq!(decide(1000, 1, true), [1, 0, 1, 0]);
        assert_eq!(decide(999, 1, false), [1, 0, 1, 0], "facing away");
    }

    #[test]
    fn adas_stand_plays_its_clip_without_the_call() {
        let mut rig = Rig::new();
        rig.ada = true;
        rig.partner.dist = 0;
        rig.work.neck.flags = 0x12;
        rig.with_auto(|auto| auto.ada_mv00());
        assert_eq!(rig.routine()[2], 1);
        assert_eq!((rig.work.bank, rig.work.move_no), (Bank::Weapon, 2));
        assert_eq!(rig.work.neck.flags, 0x12, "Sherry's stand clears 0x12");
    }

    #[test]
    fn adas_walk_and_run_decide_by_her_own_distances() {
        let decide = |routine1: u8, dist: u16| {
            let mut rig = Rig::new();
            rig.ada = true;
            rig.work.routine = [1, routine1, 1, 0];
            rig.partner.dist = dist;
            rig.with_auto(|auto| match routine1 {
                MV_WALK => auto.ada_mv01_br(),
                _ => auto.ada_mv02_br(),
            });
            rig.routine()
        };
        assert_eq!(decide(MV_WALK, 299), [1, MV_FACE_AHEAD, 0, 0]);
        assert_eq!(decide(MV_WALK, 300), [1, MV_WALK, 1, 0]);
        assert_eq!(decide(MV_WALK, 5001), [1, MV_RUN, 0, 0]);
        assert_eq!(decide(MV_WALK, 5000), [1, MV_WALK, 1, 0]);
        assert_eq!(decide(MV_RUN, 2999), [1, MV_WALK, 0, 0]);
        assert_eq!(decide(MV_RUN, 3000), [1, MV_RUN, 1, 0]);
    }

    #[test]
    fn adas_walk_turns_from_a_wall_for_30_ticks_and_keeps_walking() {
        let mut rig = Rig::new();
        rig.ada = true;
        rig.work.routine = [1, MV_WALK, 1, 0];
        rig.partner.dist = 1000;
        rig.partner.sca_hit = true;
        rig.partner.stuck = WALL_TICKS;
        rig.with_auto(|auto| auto.ada_mv01_br());
        assert_eq!(rig.routine(), [1, MV_WALK, 1, 0], "Sherry's runs");
        assert_eq!(rig.partner.stuck, ADA_WALK_WALL_TURN_TICKS);
        assert_ne!(rig.partner.state & STATE_WALL_TURN, 0);
        rig.with_auto(|auto| auto.ada_mv01_br());
        assert_eq!(rig.partner.stuck, ADA_WALK_WALL_TURN_TICKS - 1);
    }

    #[test]
    fn adas_turn_round_waits_60_ticks_and_turns_once_the_player_runs_off() {
        let decide = |dist: u16, player_spd_x: i16| {
            let mut rig = Rig::new();
            rig.ada = true;
            rig.work.routine = [1, MV_TURN_ROUND, 0, 0];
            rig.partner.dist = dist;
            rig.player_spd_x = player_spd_x;
            rig.with_auto(|auto| auto.ada_mv0c_br());
            (rig.routine(), rig.partner.timer0)
        };
        assert_eq!(decide(500, 0), ([1, MV_TURN_ROUND, 0, 0], 59));
        assert_eq!(decide(199, 0).0, [1, MV_FACE_AHEAD, 0, 0]);
        assert_eq!(decide(701, 101).0, [1, MV_FACE_POINT, 0, 0]);
        assert_eq!(decide(701, 100).0, [1, MV_TURN_ROUND, 0, 0]);
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
        let mut partner = Partner::placed(0, 0);
        partner.timer2 = 35;
        partner.spl_in45(false, 150);
        assert_eq!((partner.life, partner.max_life), (-1, -1));
        partner.spl_in45(true, 150);
        let words = (partner.life, partner.max_life, partner.timer2);
        assert_eq!(words, (150, 200, 35));
    }

    /// Room 402's channel and the walkway west of it: the box a floor-0 body
    /// climbs (record 8), the edge a floor-1 body jumps from (record 9) and
    /// the walkway's wall (record 10).
    fn room_402() -> RoomState {
        let rect = |x, z, width, depth, id, kind, floor| ScaRect {
            x,
            z,
            width,
            depth,
            id,
            kind,
            floor,
        };
        RoomState {
            sca: Some(Sca {
                cell_x: -22900,
                cell_z: -13308,
                ceiling: -6300,
                tail: 0xC5C5_C5C5,
                records: vec![
                    rect(-21093, -11451, 3430, 2458, 0xFC89, 0x9001, 1),
                    rect(-17762, -11451, 3969, 2458, 0xDC0A, 0x9041, 2),
                    rect(-29782, -11451, 8739, 2458, 0xFC80, 0x9003, 1),
                ],
            }),
            ..RoomState::default()
        }
    }

    /// The quarter turn `rotate` takes 600 along to `sign` x.
    fn facing_x(sign: i32) -> u16 {
        (0..4u16)
            .map(|quarter| quarter * 0x400)
            .find(|&angle| rotate(angle, 600, 0).0 * sign > 500)
            .unwrap()
    }

    /// The room bank's clips of the climb and the jump, long enough for
    /// their frames: clip 6 with a footfall on frame 0x40, clip 7 past the
    /// jump's frame 0x1D, the fall and the landing.
    fn climb_rig(pos: [i32; 3], angle: u16, floor: u8) -> Rig {
        let mut rig = Rig::new();
        rig.room = room_402();
        let frames = |count: usize, footfall: Option<usize>| Clip {
            frames: (0..count)
                .map(|index| ClipFrame {
                    keyframe: (index % 4) as u16,
                    timing: 0,
                    flags: if Some(index) == footfall {
                        crate::player2::FRAME_FOOTSTEP
                    } else {
                        0
                    },
                })
                .collect(),
        };
        rig.clips[usize::from(CLIMB_CLIP)] = frames(0x80, Some(0x40));
        rig.clips[usize::from(JUMP_CLIP)] = frames(0x28, None);
        rig.clips[usize::from(JUMP_FALL_CLIP)] = frames(8, None);
        rig.clips[usize::from(LAND_CLIP)] = frames(12, None);
        rig.body.pos = pos;
        rig.body.angle = angle;
        rig.body.collision_mirror = [pos[0], pos[2]];
        rig.body.radius = 250;
        rig.work.floor = floor;
        rig
    }

    #[test]
    fn a_box_ahead_on_the_floor_below_starts_the_climb() {
        // In the channel, facing the walkway 100 off a quarter turn: the
        // point 600 on and a floor up is the box's.
        let west = facing_x(-1);
        let mut rig = climb_rig([-17300, 0, -10000], west.wrapping_add(0x100), 0);
        assert!(rig.with_auto(|auto| auto.updown_ck()));
        assert_eq!(rig.routine(), [1, MV_CLIMB, 0, 0]);
        assert_eq!(rig.partner.climb_dir, west.wrapping_add(0x100));
        // Facing away from it, nothing.
        let mut rig = climb_rig([-17300, 0, -10000], facing_x(1), 0);
        assert!(!rig.with_auto(|auto| auto.updown_ck()));
        assert_eq!(rig.routine(), [1, 0, 0, 0]);
        // On the walkway's floor the box's record doesn't apply.
        let mut rig = climb_rig([-17300, 0, -10000], west, 1);
        assert!(!rig.with_auto(|auto| auto.updown_ck()));
    }

    #[test]
    fn an_edge_ahead_on_the_floor_above_starts_the_jump_down() {
        let east = facing_x(1);
        let mut rig = climb_rig([-18000, -1800, -10000], east, 1);
        assert!(rig.with_auto(|auto| auto.updown_ck()));
        assert_eq!(rig.routine(), [1, MV_CLIMB, CLIMB_IS_DROP, 0]);
        // 300 further back the point is still over the box's top, which
        // isn't below her.
        let mut rig = climb_rig([-18300, -1800, -10000], east, 1);
        assert!(!rig.with_auto(|auto| auto.updown_ck()));
    }

    #[test]
    fn the_climb_squares_up_lifts_her_a_floor_on_frame_99_and_stands() {
        let west = facing_x(-1);
        let mut rig = climb_rig([-17300, 0, -10000], west, 0);
        rig.work.routine = [1, MV_CLIMB, 0, 0];
        rig.partner.climb_dir = west.wrapping_add(0x100);
        rig.with_auto(|auto| auto.aida_mv08());
        assert_eq!(rig.be_flg & BE_CLIMBING, BE_CLIMBING);
        let mut noise = 0;
        let mut lifted = None;
        for tick in 0..400 {
            if rig.routine()[1] != MV_CLIMB {
                break;
            }
            let before = rig.body.pos[1];
            rig.with_auto(|auto| auto.aida_mv08());
            noise |= rig.noise;
            if rig.body.pos[1] != before {
                lifted = Some((tick, rig.work.bank, rig.work.move_no));
            }
        }
        assert_eq!(rig.routine(), [1, MV_STAND, 0, 0]);
        assert_eq!(rig.body.angle, west, "squared to the quarter turn");
        assert!(lifted.is_some_and(|(_, bank, clip)| bank == Bank::Room && clip == CLIMB_CLIP));
        assert_eq!(rig.body.pos, [-17300 - 600, -1800, -10000]);
        assert_eq!(rig.body.collision_mirror, [-17900, -10000]);
        assert_eq!(rig.work.floor, 1);
        assert_eq!(rig.be_flg & BE_CLIMBING, 0);
        assert_eq!(rig.work.damage_cnt & DAMAGE_LOCK, 0);
        assert_eq!(
            noise, NOISE_CLIMB,
            "the clip's footfall is the player's noise"
        );
    }

    #[test]
    fn the_jump_down_falls_to_the_ground_and_lands_on_it() {
        let east = facing_x(1);
        let mut rig = climb_rig([-18000, -1800, -10000], east, 1);
        rig.work.routine = [1, MV_CLIMB, CLIMB_IS_DROP, 0];
        rig.partner.climb_dir = east;
        // The ground `Game_loop` finds under her once she has fallen.
        rig.ground = 0;
        let mut seen = Vec::new();
        let mut noise = 0;
        for _ in 0..200 {
            if rig.routine()[1] == MV_STAND {
                break;
            }
            rig.with_auto(|auto| auto.move_type());
            noise |= rig.noise;
            seen.push((rig.routine(), rig.body.pos[1], rig.work.floor));
        }
        // Square already: one tick of the turn hands over to the jump.
        assert_eq!(seen[0].0, [1, MV_JUMP_DOWN, CLIMB_IS_DROP, 0]);
        // Off the edge at frame 0x1D: 0x226 down and 5 * 30, a floor down.
        let off = seen.iter().find(|(routine, ..)| routine[3] == 3).unwrap();
        assert_eq!((off.1, off.2), (-1800 + 0x226 + 5 * 0x1E, 0));
        assert_eq!(rig.routine(), [1, MV_STAND, 0, 0]);
        assert_eq!(rig.body.pos, [-18000 + 600, 0, -10000]);
        assert_eq!(rig.work.floor, 0);
        assert_eq!(rig.be_flg & BE_CLIMBING, 0);
        assert_eq!(noise, NOISE_CLIMB, "the landing's footfall");
    }

    #[test]
    fn deep_water_slows_her_walk_by_a_third() {
        let mut rig = Rig::new();
        let slowed = |rig: &mut Rig, water: i16, ground: i16| {
            rig.work.water = water;
            rig.ground = ground;
            rig.work.spd[0] = 30;
            rig.with_auto(|auto| auto.slow_in_water());
            rig.work.spd[0]
        };
        // Room 402's water, 700 over the channel's floor.
        assert_eq!(slowed(&mut rig, -700, 0), 20);
        assert_eq!(slowed(&mut rig, 0, 0), 30);
        // A floor up the same level is under her.
        assert_eq!(slowed(&mut rig, -700, -1800), 30);
        // 500 is not over 500.
        assert_eq!(slowed(&mut rig, -500, 0), 30);
    }

    #[test]
    fn lost_in_deep_water_she_stands_instead_of_calling() {
        let lost = |water: i16| {
            let mut rig = Rig::new();
            rig.partner.state |= STATE_LOST;
            rig.partner.dist = LOST + 1;
            rig.work.water = water;
            rig.with_auto(|auto| auto.sherry_mv00());
            (rig.routine()[2], rig.work.bank, rig.work.move_no)
        };
        assert_eq!(lost(0), (4, Bank::Pld, CALL_CLIP));
        assert_eq!(lost(-700), (1, Bank::Weapon, STAND_CLIPS[0]));
    }
}
