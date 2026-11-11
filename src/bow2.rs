//! RE 2's bowgun (`Type & 0xFFF` 0xC): its bolts are six of the weapon's own
//! parts (each with a second part, its tail), which `FUN_004d6e50` lets go of
//! three at a time and `pl_bow` flies, `Player_move`'s call after `Rot_neck`.
//!
//! A bolt flies 800 a tick along its matrix's `y`, steering for its first ten
//! ticks toward the nearest part of the nearest enemy ahead, until a wall, the
//! floor or the room's edge stops it, or `FUN_004d7e30` finds a body in reach:
//! then it sticks in the nearest part of that body, bleeds from it, and is put
//! back when its count runs out.

use crate::player2::{Matrix, catan, mul_matrix, rot_matrix};
use crate::sca::square_root0;

/// `Type & 0xFFF` of the bowgun.
pub const BOW: u8 = 0xC;
const BOLTS: usize = 6;
/// `+0x98`: a bolt the bow holds, put back by state 4.
const READY: u8 = 0xFF;
/// What the parts' `Be_flg` keep: drawn, and flying free of the hand.
const DRAWN: u32 = 1;
const FREE: u32 = 0x40;
const FLYING: u32 = 0x8440;
/// The hand's point a bolt starts at (`+0x2c` to `+0x34`), with its side.
const NOCK: [i32; 2] = [200, 0x390];
/// `Plw_init_tbl`'s bowgun entry (`0x4D30D0`): each bolt's side on the bow.
const RESTING: [i32; BOLTS] = [0, -200, 200, 0, -200, 200];
/// `pl_bow`'s box for `FUN_004d7e30`: `x`, `z`, width and length.
const REACH: [i16; 4] = [-500, 0, 375, 25];
const IDENTITY: [[i16; 3]; 3] = [[0x1000, 0, 0], [0, 0x1000, 0], [0, 0, 0x1000]];

/// What a bolt hangs from: the hand (part 11), or an enemy's part it struck.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parent {
    Hand,
    Body { slot: usize, part: u8 },
}

/// A bolt's part and what `pl_bow` keeps on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bolt {
    /// `+0x98`.
    pub state: u8,
    /// The part's `Be_flg`, and its tail's (`+0xac`).
    pub flags: u32,
    pub tail_flags: u32,
    /// `+0x18`: the matrix on its parent, and `+0x48`, in the world.
    pub(crate) local: Matrix,
    pub(crate) world: Matrix,
    pub parent: Parent,
    /// `+0x84`: the ticks it still steers, wobbles or stays in a body.
    count: i16,
    /// `+0x79`: which way it wobbles; 1 when it left the room, which is silent.
    swing: i8,
    /// `+0x7a`: it has made its splash on the water.
    splashed: bool,
    /// `+0x7e`: the player's facing when it was let go.
    dir: i16,
    /// `+0x9a`: the way it flies.
    yaw: i16,
    /// `+0x99`: the part it's in.
    part: u8,
    /// `+0x9c`: the body it hit.
    target: Option<usize>,
}

impl Bolt {
    fn resting(side: i32) -> Self {
        let local = Matrix {
            m: IDENTITY,
            t: [NOCK[0], NOCK[1], side],
        };
        Self {
            state: READY,
            flags: 0,
            tail_flags: 0,
            local,
            world: local,
            parent: Parent::Hand,
            count: 0,
            swing: 0,
            splashed: false,
            dir: 0,
            yaw: 0,
            part: 0,
            target: None,
        }
    }
}

/// The bowgun's six bolts, and `+0x7e8`, the one last let go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quiver {
    pub bolts: [Bolt; BOLTS],
    last: i16,
}

impl Default for Quiver {
    /// `Plw_init_tbl`'s set-up when the bowgun is taken up.
    fn default() -> Self {
        Self {
            bolts: RESTING.map(Bolt::resting),
            last: 0,
        }
    }
}

impl Quiver {
    /// `pl_bow_clr`, from `Pl_init`: every bolt back on the bow.
    pub fn clear(&mut self) {
        for bolt in &mut self.bolts {
            bolt.state = READY;
            bolt.flags = 0;
            bolt.tail_flags = 0;
            bolt.local.m = IDENTITY;
            bolt.parent = Parent::Hand;
        }
        self.last = 0;
    }

    /// `FUN_004d6e50`: let go of a bolt from the hand's `hand` matrix, on
    /// `side` 0, 1 (100 to the side) or 2 (-100). With none left on the bow,
    /// the first three take turns.
    pub(crate) fn launch(&mut self, side: u8, hand: &Matrix, angle: u16) {
        let side = match side {
            1 => 100,
            2 => -100,
            _ => 0,
        };
        let local = Matrix {
            m: IDENTITY,
            t: [NOCK[0], NOCK[1], side],
        };
        let (index, flags) = match self.bolts.iter().position(|bolt| bolt.state == READY) {
            Some(index) => {
                self.last = index as i16;
                (index, DRAWN)
            }
            None => {
                self.last += 1;
                if self.last > 2 {
                    self.last = 0;
                }
                (self.last as usize, DRAWN | FREE)
            }
        };
        let bolt = &mut self.bolts[index];
        bolt.state = 0;
        bolt.flags = flags;
        bolt.tail_flags = 1;
        bolt.local = local;
        bolt.world = hand.comp(&local);
        bolt.dir = angle as i16;
        bolt.parent = Parent::Hand;
    }
}

/// What a bolt sees of an enemy: the slot's [`crate::weapon2::Target`], and
/// the `Workm` of its four parts in `BYTE_0053a110`'s row, as the last draw
/// left them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Body {
    pub target: crate::weapon2::Target,
    pub(crate) parts: [Option<(u8, Matrix)>; 4],
}

/// `BYTE_0053a110`'s row of an enemy id: the parts a bolt steers for and
/// sticks in, the first for the body.
pub fn aim_parts(id: u8) -> [u8; 4] {
    match id {
        0x20 => [1, 2, 6, 14],
        0x21 | 0x26 | 0x27 => [0, 0, 0, 0],
        0x22 | 0x24 => [0, 2, 1, 8],
        0x23 => [7, 1, 2, 8],
        0x25 => [0, 1, 2, 19],
        0x28 => [4, 9, 5, 16],
        0x29 => [0, 3, 1, 2],
        0x2E => [10, 15, 21, 2],
        0x30 => [2, 4, 3, 14],
        0x31 => [2, 4, 3, 16],
        0x33 => [2, 12, 17, 27],
        0x34 => [26, 9, 6, 17],
        0x36 => [1, 4, 5, 6],
        0x37 => [0, 1, 2, 3],
        0x39 => [11, 15, 21, 6],
        0x3A => [0, 1, 3, 6],
        _ => [0, 3, 6, 8],
    }
}

/// What a tick of [`pl_bow`] asks of the rest of the game, in its order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    Effect(crate::player2::EffectCall),
    Sound(u32, [i32; 3]),
    Blast(crate::weapon2::BlastEffect),
    /// The body's `+0x200` now holds the bolt, stuck in `part`.
    Stuck {
        slot: usize,
        part: u8,
        yaw: i16,
    },
}

/// The room and the game as `pl_bow` reads them.
pub struct World<'a> {
    pub bodies: &'a mut [Body],
    pub sca: Option<&'a crate::state::Sca>,
    pub water: &'a [crate::zombie2::WaterArea],
    pub rnd: &'a mut crate::zombie2::Rnd,
    /// `scd_var_stage` 4, room 9: a bolt's wobble there drops it 0xB4 off the
    /// `Hani_ck` area, which isn't ported.
    pub calls: Vec<Call>,
}

/// `pl_bow`: one tick of each bolt.
pub fn pl_bow(quiver: &mut Quiver, world: &mut World<'_>) {
    // Both last the whole call: the box widens with each hit, and once a bolt
    // is let go this tick none after it moves.
    let mut reach = REACH;
    let mut moving = true;
    for index in 0..BOLTS {
        if !tick_bolt(&mut quiver.bolts[index], world, &mut reach, &mut moving) {
            return;
        }
    }
}

/// `ApplyMatrixSV`.
fn apply(m: &[[i16; 3]; 3], v: [i32; 3]) -> [i16; 3] {
    std::array::from_fn(|row| {
        let sum: i32 = (0..3).map(|k| i32::from(m[row][k]) * v[k]).sum();
        (sum >> 12) as i16
    })
}

/// A point on a matrix, as `Esp_call` places it.
fn on(matrix: &Matrix, point: [i32; 3]) -> [i16; 3] {
    let turned = apply(&matrix.m, point);
    std::array::from_fn(|axis| matrix.t[axis].wrapping_add(i32::from(turned[axis])) as i16)
}

/// `FUN_004d7de0`: the angle of `(x, y)`.
fn angle_of(x: i32, y: i32) -> i16 {
    if x == 0 {
        return if y > 0 { 0xC00 } else { 0x400 };
    }
    let half = if x < 0 { 0x800 } else { 0 };
    ((-(catan(y.wrapping_shl(12) / x) + half)) & 0xFFF) as i16
}

/// `FUN_004d7d80`: the turn that points `y` along `v`: no roll, its yaw and
/// its pitch.
pub(crate) fn angles_of(v: [i16; 3]) -> [i16; 3] {
    let [x, y, z] = v.map(i32::from);
    let yaw = angle_of(x, z);
    let across = square_root0(x * x + z * z);
    let pitch = (angle_of(across, -y).wrapping_sub(0x400)) & 0xFFF;
    [0, yaw, pitch]
}

/// `FUN_004d7d20`: `current` a step of `rate` toward `target`, or `target`
/// when it's within the step; and whether it was further.
fn step_toward(target: i16, current: i16, rate: i16) -> (i16, bool) {
    let off =
        (i32::from(target as u16) - i32::from(current as u16) + i32::from(rate as u16)) & 0xFFF;
    if off < i32::from(rate) * 2 {
        return (target, false);
    }
    let mut stepped = current.wrapping_sub(rate);
    if off < 0x801 {
        stepped = stepped.wrapping_add(rate.wrapping_mul(2));
    }
    (stepped, true)
}

/// `FUN_004d70e0`: the nearest enemy in play within `range` whose way is
/// within 800 of `yaw`, or none (the player).
fn nearest_ahead(bodies: &[Body], at: [i32; 3], yaw: i16, range: u32) -> Option<usize> {
    let mut range = range;
    let mut found = None;
    for (index, body) in bodies.iter().enumerate() {
        let target = &body.target;
        if target.be_flg & 1 == 0 || target.kind & 0xE000 != 0 {
            continue;
        }
        let dx = at[0].wrapping_sub(target.pos[0]);
        let dz = at[2].wrapping_sub(target.pos[2]);
        let distance = square_root0(dz.wrapping_mul(dz).wrapping_add(dx.wrapping_mul(dx))) as u32;
        let way = crate::player2::direction_ck(at[0], at[2], target.pos[0], target.pos[2]) as i16;
        if target.life >= 0 && !step_toward(way, yaw, 800).1 && distance < range {
            range = distance;
            found = Some(index);
        }
    }
    found
}

/// `FUN_004d83a0`: which of the body's parts is nearest `at`, by the height
/// apart plus the distance across.
fn nearest_part(body: &Body, at: [i32; 3]) -> Option<(u8, Matrix)> {
    if body.target.id < 0x10 {
        return None;
    }
    let mut best = i32::MAX as u32;
    let mut found = None;
    for (part, matrix) in body.parts.iter().flatten() {
        let dy = matrix.t[1].wrapping_sub(at[1]);
        let dz = matrix.t[2].wrapping_sub(at[2]);
        let dx = matrix.t[0].wrapping_sub(at[0]);
        let across = square_root0(dx.wrapping_mul(dx).wrapping_add(dz.wrapping_mul(dz)));
        let distance = dy.unsigned_abs().wrapping_add(across as u32);
        if distance < best {
            best = distance;
            found = Some((*part, *matrix));
        }
    }
    found
}

fn ground(sca: Option<&crate::state::Sca>, at: [i32; 3], radius: i32) -> crate::sca::Ground {
    sca.map_or_else(
        || crate::sca::Ground {
            blocked: at[1] > 0,
            ..Default::default()
        },
        |sca| crate::sca::ground_at(sca, at, radius, 0x2000),
    )
}

fn effect(world: &mut World<'_>, word: u32, dir: i16, at: [i16; 3]) {
    world.calls.push(Call::Effect(crate::player2::EffectCall {
        kind: (word >> 24) as u8,
        row: (word >> 16) as u8,
        size: word as u16,
        dir,
        at,
        motion: None,
    }));
}

/// One bolt's `pl_bow`. False when a bolt let go inside a wall ends the
/// tick's bolts early.
fn tick_bolt(
    bolt: &mut Bolt,
    world: &mut World<'_>,
    reach: &mut [i16; 4],
    moving: &mut bool,
) -> bool {
    if bolt.state == 0 {
        bolt.flags |= FLYING;
        bolt.state = 1;
        bolt.tail_flags |= 0x8400;
        bolt.count = 10;
        bolt.swing = 0;
        bolt.splashed = false;
        let mut turn = [0i16, 0, i16::from(world.rnd.draw() & 1) * -0x28 + 0x14];
        if bolt.local.t[2] == -100 {
            turn[0] = 100;
            turn[2] = i16::from(world.rnd.draw() & 1) * -0x3C + 0x1E;
        }
        if bolt.local.t[2] == 100 {
            turn[0] = -100;
            turn[2] = i16::from(world.rnd.draw() & 1) * -0x3C + 0x1E;
        }
        bolt.world.m = mul_matrix(&bolt.world.m, &rot_matrix(turn));
        let found = ground(world.sca, bolt.world.t, 200);
        if found.blocked || found.under_record {
            bolt.state = 2;
            return false;
        }
        *moving = false;
    }
    match bolt.state {
        1 => fly(bolt, world, reach, *moving),
        2 | 3 => {
            if bolt.state == 2 {
                stop(bolt, world);
            }
            wobble(bolt);
        }
        4 => {
            let side = bolt.local.t[2];
            *bolt = Bolt {
                flags: FREE,
                local: Matrix {
                    m: bolt.local.m,
                    t: [NOCK[0], NOCK[1], 0],
                },
                ..Bolt::resting(side)
            };
        }
        5 | 6 => {
            if bolt.state == 5 {
                strike(bolt, world);
            }
            stay(bolt, world);
        }
        _ => {}
    }
    true
}

/// State 1: on 800 along the bolt (not on a tick a bolt was let go), steering
/// for the nearest body ahead while its count lasts.
fn fly(bolt: &mut Bolt, world: &mut World<'_>, reach: &mut [i16; 4], moving: bool) {
    let step = apply(&bolt.world.m, [0, 800, 0]);
    if moving {
        for (at, step) in bolt.world.t.iter_mut().zip(step) {
            *at = at.wrapping_add(i32::from(step));
        }
    }
    let mut turn = angles_of(step);
    bolt.yaw = turn[1];
    let range = if bolt.local.t[2] == 0 { 10000 } else { 5000 };
    let at = bolt.world.t;
    let ahead = nearest_ahead(world.bodies, at, turn[1], range);
    if let Some(index) = ahead
        && bolt.count != 0
        && let Some((_, part)) = nearest_part(&world.bodies[index], at)
    {
        let to = std::array::from_fn(|axis| (part.t[axis].wrapping_sub(at[axis]) >> 3) as i16);
        let want = angles_of(to);
        turn[1] = step_toward(want[1], turn[1], 0x20).0;
        turn[2] = step_toward(want[2], turn[2], 0x58).0;
        bolt.world.m = rot_matrix(turn);
        bolt.count -= 1;
    }
    let found = ground(world.sca, at, 100);
    if found.blocked || found.under_record || at[1] < found.below {
        bolt.state = 2;
    }
    if at.iter().any(|axis| !(-30000..=30000).contains(axis)) {
        bolt.state = 2;
        bolt.swing = 1;
    }
    let blast = crate::weapon2::Blast {
        at,
        dir: bolt.dir,
        word: u32::from(BOW),
    };
    let mut targets: Vec<_> = world.bodies.iter().map(|body| body.target).collect();
    let effects = crate::weapon2::blast_ck(&blast, reach, &mut targets);
    for (body, target) in world.bodies.iter_mut().zip(targets) {
        body.target = target;
    }
    let mut hit = None;
    for effect in effects {
        if let crate::weapon2::BlastEffect::Hit(found) = effect {
            hit = Some(found.slot);
        }
        world.calls.push(Call::Blast(effect));
    }
    bolt.target = hit;
    let level = crate::zombie2::water_ck(world.water, at[0], at[2]);
    if level != 0 && i32::from(level) < at[1] {
        let dir = turn[1].wrapping_add(0x800);
        for along in [0, 200, 400, 600] {
            let size = u32::from(world.rnd.draw()) * 2 + 0x418;
            effect(
                world,
                size | 0x1A00_0000,
                dir,
                on(&bolt.world, [0, along, 0]),
            );
        }
        if !bolt.splashed {
            bolt.splashed = true;
            let size = u32::from(world.rnd.draw()) * 2 + 0x418;
            effect(
                world,
                size | 0x1A05_0000,
                0,
                [at[0] as i16, level, at[2] as i16],
            );
        }
    }
    if let Some(slot) = hit {
        let part = world
            .bodies
            .iter()
            .find(|body| body.target.slot == slot)
            .and_then(|body| nearest_part(body, bolt.world.t))
            .map_or(0, |(part, _)| part);
        bolt.state = 5;
        bolt.part = part;
        world.calls.push(Call::Stuck {
            slot,
            part,
            yaw: bolt.yaw,
        });
    }
}

/// State 2: it stops where it hit, with its sound unless it left the room,
/// and goes on 400 into what it hit.
fn stop(bolt: &mut Bolt, world: &mut World<'_>) {
    bolt.flags &= !0x400;
    if bolt.swing == 0 {
        world.calls.push(Call::Sound(0x10A_0001, bolt.world.t));
    }
    bolt.count = 0x14;
    bolt.state = 3;
    bolt.tail_flags = 0;
    bolt.swing = -1;
    let step = apply(&bolt.world.m, [0, 400, 0]);
    for (at, step) in bolt.world.t.iter_mut().zip(step) {
        *at = at.wrapping_add(i32::from(step));
    }
}

/// State 3: it quivers, less each tick, for 20 ticks.
fn wobble(bolt: &mut Bolt) {
    let swing = i16::from(bolt.swing)
        .wrapping_mul(bolt.count)
        .wrapping_mul(8);
    bolt.world.m = mul_matrix(&bolt.world.m, &rot_matrix([0, swing, swing]));
    bolt.swing = bolt.swing.wrapping_neg();
    let count = bolt.count;
    bolt.count = count.wrapping_sub(1);
    if count == 0 {
        bolt.state = 4;
    }
}

/// The body a bolt is in and that part's matrix.
fn stuck_in<'a>(bolt: &Bolt, bodies: &'a [Body]) -> Option<(&'a Body, Matrix)> {
    let slot = bolt.target?;
    let body = bodies.iter().find(|body| body.target.slot == slot)?;
    let (_, matrix) = body
        .parts
        .iter()
        .flatten()
        .find(|(part, _)| *part == bolt.part)?;
    Some((body, *matrix))
}

/// The blood's row by the body's `Be_flg` (`0xC00000`), into `row`'s bits.
fn blood_row(be_flg: u32) -> u32 {
    be_flg >> 3 & 0x18_0000
}

/// State 5: into the part, three quarters of the way to its middle, with its
/// sound and blood; from now on it hangs from the part. A body stays struck
/// for 75 ticks, 10 for ids 0x36 and 0x23, none for 0x26.
fn strike(bolt: &mut Bolt, world: &mut World<'_>) {
    bolt.flags &= !0x400;
    bolt.tail_flags &= !0x400;
    let Some((body, part)) = stuck_in(bolt, world.bodies) else {
        bolt.state = 4;
        return;
    };
    bolt.count = match body.target.id {
        0x36 | 0x23 => 10,
        0x26 => 0,
        _ => 0x4B,
    };
    bolt.state = 6;
    bolt.swing = -1;
    let at = bolt.world.t;
    let to: [i16; 3] = std::array::from_fn(|axis| part.t[axis].wrapping_sub(at[axis]) as i16);
    bolt.world.m = rot_matrix(angles_of(to));
    let [x, y, z] = to.map(i32::from);
    let across = square_root0(x * x + z * z);
    let distance = square_root0(y * y + across * across) as i16;
    let step = apply(
        &bolt.world.m,
        [0, i32::from((distance >> 2).wrapping_mul(3)), 0],
    );
    for (at, step) in bolt.world.t.iter_mut().zip(step) {
        *at = at.wrapping_add(i32::from(step));
    }
    world.calls.push(Call::Sound(0x10F_0001, bolt.world.t));
    bolt.local = bolt.world.relative_to(&part);
    if let Parent::Hand = bolt.parent {
        bolt.parent = Parent::Body {
            slot: body.target.slot,
            part: bolt.part,
        };
    }
    bolt.flags &= !FREE;
    let be_flg = body.target.be_flg;
    if be_flg & 0x1_0000 == 0 {
        let word = blood_row(be_flg) | 0x1BB8;
        let dir = bolt.yaw.wrapping_sub(0x800);
        effect(world, word, dir, on(&part, bolt.local.t));
    }
}

/// State 6: it stays in the body, bleeding every fourth tick, until its count
/// runs out.
fn stay(bolt: &mut Bolt, world: &mut World<'_>) {
    let count = bolt.count;
    bolt.count = count.wrapping_sub(1);
    if count == 0 {
        bolt.state = 4;
    }
    let Some((body, part)) = stuck_in(bolt, world.bodies) else {
        return;
    };
    bolt.world = part.comp(&bolt.local);
    let be_flg = body.target.be_flg;
    if bolt.count & 3 == 0 && be_flg & 0x1_0000 == 0 {
        let dir = bolt.yaw.wrapping_sub(0x800);
        let word = (u32::from(world.rnd.draw()) * 2 + 0x400) | blood_row(be_flg);
        effect(world, word, dir, on(&part, bolt.local.t));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_turn_steps_toward_its_target_and_lands_on_it_when_close() {
        assert_eq!(step_toward(0x100, 0, 0x20), (0x20, true));
        assert_eq!(step_toward(0xF00, 0, 0x20), (-0x20, true));
        assert_eq!(step_toward(0x10, 0, 0x20), (0x10, false));
    }

    #[test]
    fn a_bolt_points_along_its_way() {
        assert_eq!(angles_of([0, 0, 100])[1], angle_of(0, 100));
        assert_eq!(angle_of(0, 1), 0xC00);
        assert_eq!(angle_of(0, -1), 0x400);
    }

    #[test]
    fn three_bolts_leave_the_bow_and_then_the_first_three_take_turns() {
        let mut quiver = Quiver::default();
        let hand = Matrix {
            m: IDENTITY,
            t: [1000, -1000, 0],
        };
        for _ in 0..2 {
            for side in 0..3 {
                quiver.launch(side, &hand, 0);
            }
        }
        assert!(quiver.bolts.iter().all(|bolt| bolt.state == 0));
        assert_eq!(quiver.bolts[1].local.t, [200, 0x390, 100]);
        assert_eq!(quiver.bolts[0].world.t, [1200, -1000 + 0x390, 0]);
        // `+0x7e8` was left at the sixth (5); stepped past 2, it wraps to the first.
        quiver.launch(0, &hand, 0);
        assert_eq!(quiver.last, 0);
        assert_eq!(quiver.bolts[0].flags, DRAWN | FREE);
        quiver.launch(0, &hand, 0);
        assert_eq!(quiver.last, 1);
    }
}
