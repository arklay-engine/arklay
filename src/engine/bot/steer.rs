//! The bot's steering controller and its enemy-aware path costs.
//!
//! The controller drives the player's tank controls (`src/player.rs`), whose
//! facts it is built on:
//!
//! - `up` walks at [`player::WALK_SPEED`] (0x5D a tick, less on the footfall
//!   frames); `up` + `run` runs at [`player::RUN_SPEED`] (0xD2), always
//!   entered through one walk tick (the walk handler's hand-over).
//! - `left`/`right` *with* `up` steer the walk by [`player::DIAGONAL_TURN`]
//!   (0x28 a tick) and the run by [`player::RUN_TURN`] (0x30) while moving at
//!   full speed (`turned_angle`). Without `up` they turn in place by
//!   [`player::TURN_STEP`] (0x60) and do not move.
//! - Releasing `up` from a run plays a [`player::RUN_STOP_TICKS`]-tick
//!   decelerating stop; releasing `run` with `up` held hands back to the walk.
//!
//! So a moving turn has a fixed radius (about 1500 units walking and 2800
//! running) and costs no time, while an in-place turn is twice as fast in
//! angle but makes no progress. Each tick the controller simulates the walk
//! arc and the run arc toward the look-ahead point with the room collision,
//! estimates the ticks each option needs to reach it (arc plus straight
//! remainder, or in-place turn plus straight), and picks the cheapest one
//! that does not hit a wall. An in-place turn is left for reversals and for
//! arcs the room has no space for.
//!
//! The enemy zones are a path cost, not collision: a margin around each
//! monster, longer in front of it than behind, so a planned path passes
//! behind it and well clear of where it is looking.

use super::{EnemySnapshot, Turn, angle_error, blocked, distance_xz};
use crate::player;
use crate::state::RoomState;

/// The heading error a moving bot leaves alone: half the smaller steer step,
/// so a correction never overshoots into the other side's band.
pub(super) const STEER_DEADBAND: i32 = 0x18;
/// The heading error up to which the fallback walks straight on (the old
/// controller's forward band) rather than turning in place.
const STRAIGHT_DEADBAND: i32 = 0x40;
/// Ticks the arc simulation looks ahead: a walk turns 0x800 in 52 ticks.
const SIM_TICKS: u32 = 64;
/// Ticks charged for picking the run back up (its entry walk tick).
const RUN_ENTRY_COST: f64 = 1.0;
/// Ticks charged for dropping a run to a walk and picking it up again.
const RUN_DROP_COST: f64 = 2.0;
/// A moving option is preferred over an in-place turn by this many ticks:
/// the estimate ignores the stride the in-place turn breaks. Measured on the
/// re1 bench: 0 to 8 left more in-place turns, and 10 to 30 were flat.
const MOVE_BIAS: f64 = 20.0;

/// The player body the collision queries test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Body {
    pub(super) y: i32,
    pub(super) radius: i32,
    pub(super) flags: u8,
}

/// Which pad the controller chose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Gait {
    /// `up`, steering with `left`/`right`.
    Walk,
    /// `up` + `run`, steering with `left`/`right`.
    Run,
    /// `left`/`right` alone.
    TurnInPlace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Steer {
    pub(super) forward: bool,
    pub(super) run: bool,
    pub(super) turn: Turn,
    pub(super) gait: Gait,
}

/// One steering decision's inputs.
#[derive(Clone, Copy, Debug)]
pub(super) struct SteerQuery {
    pub(super) pos: (i32, i32),
    pub(super) angle: u16,
    /// The look-ahead point the bot steers at.
    pub(super) target: (i32, i32),
    /// Coming this close to `target` counts as reaching it.
    pub(super) arrive: i32,
    /// Whether a run may be held this tick (the route asked for it and the
    /// goal is far enough).
    pub(super) allow_run: bool,
    /// Whether the last tick ran.
    pub(super) running: bool,
}

/// The unit movement vector of a 12-bit heading: angle 0 walks along +X and
/// a growing angle turns toward -Z (`player::move_player_xz`).
pub(super) fn heading_dir(angle: u16) -> (f64, f64) {
    let theta = f64::from(angle & 0x0FFF) * std::f64::consts::TAU / 4096.0;
    (theta.cos(), -theta.sin())
}

fn bearing(from: (i32, i32), to: (i32, i32)) -> u16 {
    crate::sfx::angle_between_xz(from.0, from.1, to.0, to.1)
}

/// How strictly a simulated arc tests the room. A bot already inside the
/// resolver's skin (it slid along a wall) would fail every arc on its first
/// step, so the start position decides how much margin the arc gets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Margin {
    Skin,
    Bare,
    Ignore,
}

fn margin_at(room: &RoomState, body: Body, pos: (i32, i32)) -> Margin {
    if !blocked(room, pos.0, pos.1, body.y, body.radius, body.flags) {
        Margin::Skin
    } else if !player::position_blocked(room, [pos.0, body.y, pos.1], body.radius, body.flags) {
        Margin::Bare
    } else {
        Margin::Ignore
    }
}

fn hits(room: &RoomState, body: Body, margin: Margin, x: i32, z: i32) -> bool {
    match margin {
        Margin::Skin => blocked(room, x, z, body.y, body.radius, body.flags),
        Margin::Bare => player::position_blocked(room, [x, body.y, z], body.radius, body.flags),
        Margin::Ignore => false,
    }
}

fn segment_hits(
    room: &RoomState,
    body: Body,
    margin: Margin,
    from: (i32, i32),
    to: (i32, i32),
) -> bool {
    let length = distance_xz(from, to);
    let samples = ((length + 95) / 96).max(1);
    (1..=samples).any(|step| {
        let x = from.0 + (to.0 - from.0) * step / samples;
        let z = from.1 + (to.1 - from.1) * step / samples;
        hits(room, body, margin, x, z)
    })
}

/// The squared distance from `point` to the segment `a..b`.
fn segment_distance_sq(a: (f64, f64), b: (f64, f64), point: (f64, f64)) -> f64 {
    let (dx, dz) = (b.0 - a.0, b.1 - a.1);
    let len_sq = dx * dx + dz * dz;
    let t = if len_sq > 0.0 {
        (((point.0 - a.0) * dx + (point.1 - a.1) * dz) / len_sq).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (cx, cz) = (a.0 + dx * t, a.1 + dz * t);
    (point.0 - cx).powi(2) + (point.1 - cz).powi(2)
}

/// Simulate steering at `query.target` while moving at `speed` and turning
/// `step` a tick, the way the player's movement applies it (turn first, then
/// move along the new heading). Returns the ticks until the heading is
/// aligned (or the target is reached) and the straight distance left then,
/// or `None` when the arc hits the room, never aligns inside the horizon
/// (the target lies inside the turning circle), or ends without a clear line.
pub(super) fn simulate_arc(
    room: &RoomState,
    body: Body,
    query: &SteerQuery,
    speed: f64,
    step: u16,
) -> Option<(u32, f64)> {
    let margin = margin_at(room, body, query.pos);
    let target = (f64::from(query.target.0), f64::from(query.target.1));
    let arrive = f64::from(query.arrive.max(1));
    let (mut x, mut z) = (f64::from(query.pos.0), f64::from(query.pos.1));
    let mut angle = query.angle & 0x0FFF;
    for tick in 0..=SIM_TICKS {
        let here = (x.round() as i32, z.round() as i32);
        let remaining = ((target.0 - x).powi(2) + (target.1 - z).powi(2)).sqrt();
        if remaining <= arrive {
            return Some((tick, 0.0));
        }
        let error = angle_error(angle, bearing(here, query.target));
        if error.abs() <= STEER_DEADBAND {
            if segment_hits(room, body, margin, here, query.target) {
                return None;
            }
            return Some((tick, remaining));
        }
        if tick == SIM_TICKS {
            break;
        }
        angle = if error > 0 {
            angle.wrapping_add(step)
        } else {
            angle.wrapping_sub(step)
        } & 0x0FFF;
        let (dx, dz) = heading_dir(angle);
        let next = (x + dx * speed, z + dz * speed);
        if segment_distance_sq((x, z), next, target) <= arrive * arrive {
            return Some((tick + 1, 0.0));
        }
        if hits(
            room,
            body,
            margin,
            next.0.round() as i32,
            next.1.round() as i32,
        ) {
            return None;
        }
        (x, z) = next;
    }
    None
}

fn turn_toward(error: i32) -> Turn {
    match error {
        e if e > 0 => Turn::Right,
        e if e < 0 => Turn::Left,
        _ => Turn::None,
    }
}

/// Choose this tick's pad for steering at `query.target`.
pub(super) fn steer(room: &RoomState, body: Body, query: &SteerQuery) -> Steer {
    let error = angle_error(query.angle, bearing(query.pos, query.target));
    let distance = f64::from(distance_xz(query.pos, query.target));
    let walk_speed = f64::from(player::WALK_SPEED);
    let run_speed = f64::from(player::RUN_SPEED);
    // The speed a leg continues at once the heading is aligned.
    let cruise = if query.allow_run {
        run_speed
    } else {
        walk_speed
    };
    let entry = if query.allow_run { RUN_ENTRY_COST } else { 0.0 };
    let steer_turn = if error.abs() > STEER_DEADBAND {
        turn_toward(error)
    } else {
        Turn::None
    };

    let mut best: Option<(f64, Gait)> = None;
    let mut consider = |cost: f64, gait: Gait| {
        if best.is_none_or(|(best_cost, _)| cost < best_cost) {
            best = Some((cost, gait));
        }
    };
    // The run goes first: on a tie (an aligned heading) it wins, since the
    // walk's estimate already assumes the run is picked up afterwards.
    if query.allow_run
        && let Some((ticks, remaining)) =
            simulate_arc(room, body, query, run_speed, player::RUN_TURN)
    {
        let pick_up = if query.running { 0.0 } else { RUN_ENTRY_COST };
        consider(
            f64::from(ticks) + remaining / run_speed + pick_up - MOVE_BIAS,
            Gait::Run,
        );
    }
    if let Some((ticks, remaining)) =
        simulate_arc(room, body, query, walk_speed, player::DIAGONAL_TURN)
    {
        let drop = if query.running { RUN_DROP_COST } else { 0.0 };
        consider(
            f64::from(ticks) + remaining / cruise + entry + drop - MOVE_BIAS,
            Gait::Walk,
        );
    }
    if error.abs() > STRAIGHT_DEADBAND {
        let step = f64::from(player::TURN_STEP);
        let turn_ticks = (f64::from(error.abs() - STEER_DEADBAND) / step).ceil();
        let stop = if query.running {
            f64::from(player::RUN_STOP_TICKS)
        } else {
            0.0
        };
        consider(
            turn_ticks + distance / cruise + entry + stop,
            Gait::TurnInPlace,
        );
    }
    match best {
        Some((_, Gait::Run)) => Steer {
            forward: true,
            run: true,
            turn: steer_turn,
            gait: Gait::Run,
        },
        Some((_, Gait::TurnInPlace)) => Steer {
            forward: false,
            run: false,
            turn: turn_toward(error),
            gait: Gait::TurnInPlace,
        },
        // A feasible walk, or no feasible arc and a heading inside the
        // straight band: walk on and let the resolver slide, as before.
        Some((_, Gait::Walk)) | None => Steer {
            forward: true,
            run: false,
            turn: steer_turn,
            gait: Gait::Walk,
        },
    }
}

/// The heading change the engine applies on the tick a pad is released after
/// `steer` (`player.rs`: the release tick dispatches through the previous
/// behaviour, so a released in-place turn rotates once more by 0x60 and a
/// released walking steer by 0x28). A run release decelerates without
/// turning.
pub(super) fn release_delta(steer: &Steer) -> i32 {
    let sign = match steer.turn {
        Turn::Right => 1,
        Turn::Left => -1,
        Turn::None => 0,
    };
    match steer.gait {
        Gait::TurnInPlace => sign * i32::from(player::TURN_STEP),
        Gait::Walk => sign * i32::from(player::DIAGONAL_TURN),
        Gait::Run => 0,
    }
}

fn wrap_error(error: i32) -> i32 {
    let error = error.rem_euclid(0x1000);
    if error > 0x800 { error - 0x1000 } else { error }
}

/// Plan an in-place turn onto a heading `error` away, given the `pending`
/// heading change the engine still applies if the pad is released now.
///
/// An in-place press turns 0x60, and its release turns 0x60 more, so the
/// smallest tap is 0xC0: wider than the `band` either side of the target. A
/// naive "turn until inside the band, then let go" overshoots on the release
/// and then oscillates. This searches the press/release sequences (pressing
/// the other way first, then back, nets a single 0x60) for the shortest one
/// that comes to rest inside the band, and returns its first press, or
/// `Turn::None` to release. Returns whether the heading is settled now.
pub(super) fn plan_turn(error: i32, pending: i32, band: i32) -> (Turn, bool) {
    if pending == 0 && error.abs() <= band {
        return (Turn::None, true);
    }
    let step = i32::from(player::TURN_STEP);
    // Far from the target the first press is plainly toward it.
    if error.abs() > 4 * step {
        return (if error > 0 { Turn::Right } else { Turn::Left }, false);
    }
    let mut seen = std::collections::HashSet::new();
    let mut queue = std::collections::VecDeque::new();
    for first in [Turn::None, Turn::Right, Turn::Left] {
        queue.push_back((error, pending, first, first));
    }
    let mut depth_left = 256;
    while let Some((error, pending, first, action)) = queue.pop_front() {
        let (error, pending) = match action {
            Turn::None if pending == 0 => continue,
            Turn::None => (wrap_error(error - pending), 0),
            Turn::Right => (wrap_error(error - step), step),
            Turn::Left => (wrap_error(error + step), -step),
        };
        if pending == 0 && error.abs() <= band {
            return (first, false);
        }
        if !seen.insert((error, pending)) {
            continue;
        }
        depth_left -= 1;
        if depth_left == 0 {
            break;
        }
        for next in [Turn::None, Turn::Right, Turn::Left] {
            queue.push_back((error, pending, first, next));
        }
    }
    (turn_toward(error - pending), false)
}

/// The point the walk steers at on the final approach to a zone: on the line
/// that ends at `goal` heading toward `face`, halfway between the bot's
/// projection onto that line and the goal. Steering at it converges the
/// heading onto the zone's face by the time the bot arrives. `None` when the
/// bot is not behind the goal inside a 45-degree cone around that line, or
/// farther than `reach`.
pub(super) fn face_carrot(
    pos: (i32, i32),
    goal: (i32, i32),
    face: (i32, i32),
    reach: i32,
) -> Option<(i32, i32)> {
    let (ux, uz) = unit(goal, face)?;
    let (gx, gz) = (f64::from(goal.0 - pos.0), f64::from(goal.1 - pos.1));
    let along = gx * ux + gz * uz;
    let lateral = (gx * uz - gz * ux).abs();
    if along <= 0.0 || lateral > along || distance_xz(pos, goal) > reach {
        return None;
    }
    let back = along * 0.5;
    Some((
        (f64::from(goal.0) - ux * back).round() as i32,
        (f64::from(goal.1) - uz * back).round() as i32,
    ))
}

/// The point `lead` units behind `goal` on the line toward `face`.
pub(super) fn approach_point(goal: (i32, i32), face: (i32, i32), lead: i32) -> Option<(i32, i32)> {
    let (ux, uz) = unit(goal, face)?;
    Some((
        (f64::from(goal.0) - ux * f64::from(lead)).round() as i32,
        (f64::from(goal.1) - uz * f64::from(lead)).round() as i32,
    ))
}

fn unit(from: (i32, i32), to: (i32, i32)) -> Option<(f64, f64)> {
    let (dx, dz) = (f64::from(to.0 - from.0), f64::from(to.1 - from.1));
    let length = (dx * dx + dz * dz).sqrt();
    (length >= 1.0).then(|| (dx / length, dz / length))
}

/// The avoidance margin straight ahead of a monster.
pub(super) const AVOID_FRONT: f64 = 2400.0;
/// The avoidance margin behind a monster.
pub(super) const AVOID_BACK: f64 = 900.0;
/// Inside this radius a node also pays [`CORE_PENALTY`]: the attack reach.
pub(super) const AVOID_CORE: f64 = 700.0;
/// The per-node cost at the monster itself, falling linearly to zero at the
/// margin. A grid step costs 10.
const AVOID_WEIGHT: f64 = 60.0;
const CORE_PENALTY: u32 = 200;

/// One monster's avoidance field.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct EnemyZone {
    x: f64,
    z: f64,
    dir: (f64, f64),
}

impl EnemyZone {
    pub(super) fn new(x: i32, z: i32, angle: u16) -> Self {
        Self {
            x: f64::from(x),
            z: f64::from(z),
            dir: heading_dir(angle),
        }
    }

    pub(super) fn from_snapshot(enemy: &EnemySnapshot) -> Self {
        Self::new(enemy.x, enemy.z, enemy.angle)
    }

    /// The margin toward `(x, z)`: [`AVOID_FRONT`] straight ahead of the
    /// monster, [`AVOID_BACK`] straight behind, blended by the cosine between.
    pub(super) fn margin_toward(&self, x: i32, z: i32) -> f64 {
        let (dx, dz) = (f64::from(x) - self.x, f64::from(z) - self.z);
        let length = (dx * dx + dz * dz).sqrt();
        let cos = if length > 0.0 {
            (dx * self.dir.0 + dz * self.dir.1) / length
        } else {
            1.0
        };
        AVOID_BACK + (AVOID_FRONT - AVOID_BACK) * (1.0 + cos) * 0.5
    }

    /// The A* cost of standing on `(x, z)`.
    pub(super) fn penalty(&self, x: i32, z: i32) -> u32 {
        let (dx, dz) = (f64::from(x) - self.x, f64::from(z) - self.z);
        if dx.abs() > AVOID_FRONT || dz.abs() > AVOID_FRONT {
            return 0;
        }
        let distance = (dx * dx + dz * dz).sqrt();
        let margin = self.margin_toward(x, z);
        if distance >= margin {
            return 0;
        }
        let core = if distance < AVOID_CORE {
            CORE_PENALTY
        } else {
            0
        };
        (AVOID_WEIGHT * (margin - distance) / margin).ceil() as u32 + core
    }
}

/// The summed zone cost of `(x, z)`.
pub(super) fn zone_penalty(zones: &[EnemyZone], x: i32, z: i32) -> u32 {
    zones.iter().map(|zone| zone.penalty(x, z)).sum()
}

/// The highest zone cost along a segment, sampled every 128 units.
pub(super) fn segment_penalty(zones: &[EnemyZone], from: (i32, i32), to: (i32, i32)) -> u32 {
    if zones.is_empty() {
        return 0;
    }
    let samples = ((distance_xz(from, to) + 127) / 128).max(1);
    (0..=samples)
        .map(|step| {
            let x = from.0 + (to.0 - from.0) * step / samples;
            let z = from.1 + (to.1 - from.1) * step / samples;
            zone_penalty(zones, x, z)
        })
        .max()
        .unwrap_or(0)
}

/// Whether a shortcut stays out of the zones: no sample costs more than the
/// dearer endpoint, so smoothing cannot pull a detour back through a monster.
pub(super) fn segment_avoids(zones: &[EnemyZone], from: (i32, i32), to: (i32, i32)) -> bool {
    if zones.is_empty() {
        return true;
    }
    let ends = zone_penalty(zones, from.0, from.1).max(zone_penalty(zones, to.0, to.1));
    segment_penalty(zones, from, to) <= ends
}

/// The zone cost of a whole path, sampled every 128 units: what the path
/// query reports as `enemy_cost`.
pub(super) fn path_exposure(zones: &[EnemyZone], start: (i32, i32), path: &[(i32, i32)]) -> u32 {
    if zones.is_empty() {
        return 0;
    }
    let mut total = 0u32;
    let mut from = start;
    for &to in path {
        let samples = ((distance_xz(from, to) + 127) / 128).max(1);
        for step in 1..=samples {
            let x = from.0 + (to.0 - from.0) * step / samples;
            let z = from.1 + (to.1 - from.1) * step / samples;
            total = total.saturating_add(zone_penalty(zones, x, z));
        }
        from = to;
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::CollisionRect;

    const BODY: Body = Body {
        y: 0,
        radius: 100,
        flags: 0,
    };

    fn query(pos: (i32, i32), angle: u16, target: (i32, i32)) -> SteerQuery {
        SteerQuery {
            pos,
            angle,
            target,
            arrive: 256,
            allow_run: false,
            running: false,
        }
    }

    fn wall(x_min: u16, x_max: u16, z_min: u16, z_max: u16) -> CollisionRect {
        CollisionRect {
            x_max,
            z_max,
            x_min,
            z_min,
            kind: 1,
            flags: 0,
        }
    }

    #[test]
    fn heading_dir_matches_the_engine_bearing() {
        // Angle 0 walks +X, 0x400 walks -Z, 0xC00 walks +Z.
        for (angle, target) in [(0u16, (1000, 0)), (0x400, (0, -1000)), (0xC00, (0, 1000))] {
            let (dx, dz) = heading_dir(angle);
            assert!((dx - f64::from(target.0) / 1000.0).abs() < 1e-9);
            assert!((dz - f64::from(target.1) / 1000.0).abs() < 1e-9);
            assert_eq!(bearing((0, 0), target), angle);
        }
    }

    #[test]
    fn steer_arcs_through_a_moderate_error_in_open_space() {
        let room = RoomState::default();
        // Facing +X; the target is 45 degrees to the +Z side, 4000 away.
        let q = query((10000, 8000), 0, (12828, 10828));
        let choice = steer(&room, BODY, &q);
        assert_eq!(choice.gait, Gait::Walk);
        assert!(choice.forward);
        assert_eq!(choice.turn, Turn::Left, "+Z lies at a falling angle");
    }

    #[test]
    fn steer_turns_in_place_for_a_reversal() {
        let room = RoomState::default();
        let q = query((10000, 8000), 0, (6000, 8000));
        let choice = steer(&room, BODY, &q);
        assert_eq!(choice.gait, Gait::TurnInPlace);
        assert!(!choice.forward);
        assert_ne!(choice.turn, Turn::None);
    }

    #[test]
    fn steer_turns_in_place_when_a_wall_leaves_no_room_for_the_arc() {
        let mut room = RoomState::default();
        // A wall 400 units ahead: a walking left arc swings +X into it.
        room.collision.quadrants[0].push(wall(10400, 12000, 0, 20000));
        let q = query((10000, 8000), 0, (10000, 14000));
        assert!(simulate_arc(&room, BODY, &q, 93.0, player::DIAGONAL_TURN).is_none());
        let choice = steer(&room, BODY, &q);
        assert_eq!(choice.gait, Gait::TurnInPlace);
        assert_eq!(choice.turn, Turn::Left);
        // Without the wall the same 90-degree error is walked as an arc.
        let open = steer(&RoomState::default(), BODY, &q);
        assert_eq!(open.gait, Gait::Walk);
    }

    #[test]
    fn steer_runs_when_aligned_and_steers_a_run_through_a_wide_corner() {
        let room = RoomState::default();
        let mut q = query((10000, 8000), 0, (14000, 8000));
        q.allow_run = true;
        let start = steer(&room, BODY, &q);
        assert_eq!(start.gait, Gait::Run);
        assert!(start.forward && start.run);
        assert_eq!(start.turn, Turn::None);
        // A running bot with a 0x80 error and room ahead keeps running and
        // steers instead of stopping.
        q.running = true;
        q.angle = 0xF80;
        let corner = steer(&room, BODY, &q);
        assert_eq!(corner.gait, Gait::Run);
        assert_eq!(corner.turn, Turn::Right);
        // Without the run the same error is a walking steer.
        q.allow_run = false;
        q.running = false;
        let walk = steer(&room, BODY, &q);
        assert_eq!(walk.gait, Gait::Walk);
        assert_eq!(walk.turn, Turn::Right);
    }

    #[test]
    fn simulated_arc_reports_a_target_inside_the_turning_circle() {
        let room = RoomState::default();
        // 600 units to the side: inside the walk's ~1500-unit circle, so a
        // moving turn orbits it and only an in-place turn gets there.
        let q = query((10000, 8000), 0, (10000, 8600));
        assert!(simulate_arc(&room, BODY, &q, 93.0, player::DIAGONAL_TURN).is_none());
        assert_eq!(steer(&room, BODY, &q).gait, Gait::TurnInPlace);
    }

    /// Run `plan_turn` against the engine's press/release model and return
    /// the final error and the ticks it took.
    fn settle(mut error: i32, mut pending: i32) -> (i32, u32) {
        for tick in 0..40 {
            let (turn, settled) = plan_turn(error, pending, 0x40);
            if settled {
                return (error, tick);
            }
            match turn {
                Turn::None => {
                    error = wrap_error(error - pending);
                    pending = 0;
                }
                Turn::Right => {
                    error = wrap_error(error - 0x60);
                    pending = 0x60;
                }
                Turn::Left => {
                    error = wrap_error(error + 0x60);
                    pending = -0x60;
                }
            }
        }
        panic!("the turn never settled: error={error:#x} pending={pending:#x}");
    }

    #[test]
    fn plan_turn_settles_inside_the_band_despite_the_release_step() {
        // Every start error and pending release settles in the band, and a
        // turn needing between one and two taps does not loop.
        for start in (-0x800..0x800).step_by(0x10) {
            for pending in [0, 0x60, -0x60, 0x28, -0x28] {
                let (error, ticks) = settle(start, pending);
                assert!(
                    error.abs() <= 0x40,
                    "start {start:#x}/{pending:#x} -> {error:#x}"
                );
                assert!(ticks <= 26, "start {start:#x}/{pending:#x} took {ticks}");
            }
        }
        // The naive release-in-band rule loops on 0x50; the plan does not.
        assert!(settle(0x50, 0).1 <= 4);
        // A pending release that lands in the band is let go, not pressed.
        assert_eq!(plan_turn(0x70, 0x60, 0x40), (Turn::None, false));
    }

    #[test]
    fn face_carrot_lies_on_the_approach_line_inside_its_cone() {
        let goal = (10000, 10000);
        let face = (10000, 12000);
        // Straight behind the goal: the carrot is halfway along the line.
        assert_eq!(
            face_carrot((10000, 9000), goal, face, 1400),
            Some((10000, 9500))
        );
        // Off to the side but inside 45 degrees: still on the line.
        assert_eq!(
            face_carrot((10400, 9000), goal, face, 1400),
            Some((10000, 9500))
        );
        // Outside the cone, past the goal, or too far: none.
        assert_eq!(face_carrot((11200, 9000), goal, face, 1400), None);
        assert_eq!(face_carrot((10000, 10500), goal, face, 1400), None);
        assert_eq!(face_carrot((10000, 7000), goal, face, 1400), None);
        assert_eq!(approach_point(goal, face, 800), Some((10000, 9200)));
    }

    #[test]
    fn enemy_zone_reaches_farther_in_front_than_behind() {
        // Facing +X.
        let zone = EnemyZone::new(10000, 10000, 0);
        assert!((zone.margin_toward(12000, 10000) - AVOID_FRONT).abs() < 1e-6);
        assert!((zone.margin_toward(8000, 10000) - AVOID_BACK).abs() < 1e-6);
        let side = zone.margin_toward(10000, 12000);
        assert!(side > AVOID_BACK && side < AVOID_FRONT);
        // 1500 ahead costs; 1500 behind is free.
        assert!(zone.penalty(11500, 10000) > 0);
        assert_eq!(zone.penalty(8500, 10000), 0);
        // The cost grows toward the monster, with the core on top.
        assert!(zone.penalty(10500, 10000) > zone.penalty(11500, 10000) + CORE_PENALTY / 2);
        assert_eq!(zone.penalty(13000, 10000), 0);
    }

    #[test]
    fn segment_avoids_rejects_a_shortcut_through_a_zone() {
        let zones = [EnemyZone::new(10000, 10000, 0)];
        // Both ends free: a line through the monster is refused, one that
        // passes behind it, outside the back margin, is accepted.
        assert!(!segment_avoids(&zones, (10000, 7000), (10000, 13000)));
        assert!(segment_avoids(&zones, (8800, 7000), (8800, 13000)));
        assert!(segment_avoids(&[], (10000, 7000), (10000, 13000)));
        assert!(path_exposure(&zones, (10000, 7000), &[(10000, 13000)]) > 0);
    }
}
