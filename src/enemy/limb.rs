//! Severed limbs: a joint flagged 4 leaves the skeleton and falls.
//!
//! The hit that cuts a limb flags its joint 12. The render pass composes a
//! joint flagged 8 one last time and lets it go (`rotate_entity`), and from
//! then on moves every joint flagged 4 itself (`render_entity`'s call of
//! `FUN_004896c0`): thrown back and up from the body, falling, bouncing once
//! off the floor and tumbling until it lies flat.
//!
//! That pass composes the whole skeleton before it moves anything, so
//! whatever hangs off the joint (a forearm's hand) follows a frame behind:
//! the clock's held joints carry the world the skeleton composes from, and
//! its loose joints the world the limb itself is drawn at. The step's own
//! bytes are the joint's blood scratch, the same words of the joint struct.

use crate::anim::Mat4x3;
use crate::game::GameState;

/// The throw `render_entity` reloads into the joint each frame, in the body's
/// own axes: back along X, and up.
const THROW: [i32; 2] = [-20, 200];
/// What each frame in the air takes off the upward speed.
const GRAVITY: i16 = -35;
/// The height a limb rests at; the pool under it is drawn at exactly this.
pub const FLOOR_Y: i32 = -100;
/// The height the first touch throws the limb back up to.
const BOUNCE_Y: i32 = -350;
/// The frame counter a touch of the floor restarts from.
const TOUCH_FRAME: u8 = 3;
/// The state byte once the limb has touched the floor.
const TOUCHED: u8 = 0x80;
/// The state bit of a limb lying still.
const AT_REST: u8 = 0x01;

/// Before `slot`'s skeleton is composed: each loose joint is where the last
/// frame's move left it, and the joints under it compose from there.
pub fn settle(game: &mut GameState, slot: usize) {
    let clock = &mut game.entity_anims[slot];
    for &(joint, world) in &clock.loose {
        if let Some(held) = clock.held.iter_mut().find(|(other, _)| *other == joint) {
            held.1 = world;
        }
    }
}

/// The render pass's work on `slot`'s severed limbs, given the worlds its
/// compose just built: let go of each newly cut joint where it is, and move
/// every loose one a frame. A moved joint's world is written back; the joints
/// under it keep the worlds composed before the move.
pub fn advance(game: &mut GameState, slot: usize, worlds: &mut [Mat4x3]) {
    // `render_entity` walks the joints from the last to the first.
    for joint in (0..worlds.len().min(32)).rev() {
        let flag = game.entities[slot].joint_flag(joint);
        if flag & 4 == 0 {
            continue;
        }
        if flag & 8 != 0 && flag & 0x40 == 0 {
            let entity = &mut game.entities[slot];
            entity.set_joint_flag(joint, (flag & 0xF5) | 0x50);
            entity.joint_blood[joint].counter = 0;
            let clock = &mut game.entity_anims[slot];
            clock.held.retain(|(other, _)| *other != joint);
            clock.held.push((joint, worlds[joint]));
            clock.loose.retain(|(other, _)| *other != joint);
            clock.loose.push((joint, worlds[joint]));
        }
        let Some(index) = game.entity_anims[slot]
            .loose
            .iter()
            .position(|(other, _)| *other == joint)
        else {
            continue;
        };
        let next_y = worlds.get(joint + 1).map_or(0, |world| world.t[1]);
        let angle = game.entities[slot].angle;
        let mut world = game.entity_anims[slot].loose[index].1;
        let scratch = &mut game.entities[slot].joint_blood[joint];
        if step(&mut world, scratch, next_y, angle) {
            game.entity_anims[slot].loose[index].1 = world;
            worlds[joint] = world;
        }
    }
}

/// `FUN_004896c0`, one frame of a loose joint. `next_y` is the world height of
/// the joint after it and `angle` the body's facing. Returns whether the joint
/// moved.
fn step(
    world: &mut Mat4x3,
    scratch: &mut crate::game::JointBlood,
    next_y: i32,
    angle: u16,
) -> bool {
    // `render_entity` writes the throw into the joint's rotation words before
    // every call, so a limb at rest keeps showing it.
    scratch.vel_x = THROW[0] as i16;
    scratch.vel_y = THROW[1] as i16;
    scratch.rot_z = 0;

    let this_y = world.t[1];
    if this_y >= FLOOR_Y && scratch.flags & TOUCHED != 0 {
        scratch.flags |= AT_REST;
        return false;
    }

    // The limb tumbles until it has touched the floor, and after that while
    // it stands more than 200 off the next joint's height either way.
    let gap = (next_y as i16)
        .wrapping_sub(this_y as i16)
        .wrapping_add(200) as u16;
    if gap > 400 || scratch.flags & TOUCHED == 0 {
        // The spin turns over once the limb has bounced, which lays it flat.
        let touched = i32::from(scratch.flags & TOUCHED);
        let spin_x = (0x20 - touched) >> 1;
        let spin_y = i32::from(((0x40 - touched) as u16).wrapping_mul(2) as i16);
        let spin = crate::anim::rotation_matrix(spin_x, spin_y, 0);
        world.r = crate::anim::mul_matrix0(&world.r, &spin);
    }

    // The throw, turned to the body's facing.
    let (throw_x, throw_z) = crate::player::rotate_xz(angle, THROW[0], 0);
    scratch.vel_x = throw_x as i16;
    scratch.rot_z = throw_z as i16;
    world.t[0] = world.t[0].wrapping_add(i32::from(scratch.vel_x));
    let rise = i16::from(scratch.counter)
        .wrapping_mul(GRAVITY)
        .wrapping_add(THROW[1] as i16);
    scratch.vel_y = rise;
    world.t[2] = world.t[2].wrapping_add(i32::from(scratch.rot_z));
    world.t[1] = this_y.wrapping_sub(i32::from(rise));

    if world.t[1] >= FLOOR_Y {
        // The first touch bounces; the second lands on the floor exactly.
        world.t[1] = if scratch.flags & TOUCHED != 0 {
            FLOOR_Y
        } else {
            BOUNCE_Y
        };
        scratch.counter = TOUCH_FRAME;
        scratch.flags = TOUCHED;
    }
    scratch.counter = scratch.counter.wrapping_add(1);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::JointBlood;

    fn upright(t: [i32; 3]) -> Mat4x3 {
        Mat4x3 {
            r: crate::anim::IDENTITY_MATRIX,
            t,
        }
    }

    /// Step a limb until it rests, returning each frame's height.
    fn fall(world: &mut Mat4x3, scratch: &mut JointBlood, next_y: i32) -> Vec<i32> {
        let mut heights = Vec::new();
        for _ in 0..200 {
            if !step(world, scratch, next_y, 0) {
                break;
            }
            heights.push(world.t[1]);
        }
        heights
    }

    #[test]
    fn a_cut_limb_rises_falls_bounces_once_and_rests_on_the_floor() {
        let mut world = upright([0, -1000, 0]);
        let mut scratch = JointBlood::default();
        let heights = fall(&mut world, &mut scratch, -1000);

        // Thrown up by 200, less 35 for each frame in the air.
        assert_eq!(&heights[..4], &[-1200, -1365, -1495, -1590]);
        let bounce = heights
            .iter()
            .position(|&y| y == BOUNCE_Y)
            .expect("the first touch bounces");
        assert!(heights[..bounce].iter().all(|&y| y < FLOOR_Y));
        assert_eq!(heights.last(), Some(&FLOOR_Y), "the second lands");
        assert_eq!(
            heights.iter().filter(|&&y| y == BOUNCE_Y).count(),
            1,
            "one bounce"
        );
        assert_eq!(scratch.flags, TOUCHED | AT_REST);
        // At rest the joint shows the throw `render_entity` writes each frame.
        assert_eq!((scratch.vel_x, scratch.vel_y, scratch.rot_z), (-20, 200, 0));
    }

    #[test]
    fn the_throw_follows_the_bodys_facing() {
        // Facing 0 the body's X is the world's, and the limb goes back along
        // it (a unit short of 20: the yaw matrix's 1.0 is 0x0FFF).
        let mut world = upright([0, -1000, 0]);
        step(&mut world, &mut JointBlood::default(), -1000, 0);
        assert_eq!((world.t[0], world.t[2]), (-19, 0));

        // A quarter turn puts the body's X along the world's -Z.
        let mut world = upright([0, -1000, 0]);
        step(&mut world, &mut JointBlood::default(), -1000, 0x400);
        assert_eq!((world.t[0], world.t[2]), (0, 19));
    }

    #[test]
    fn a_bounced_limb_level_with_the_next_joint_stops_tumbling() {
        let mut world = upright([0, BOUNCE_Y, 0]);
        let mut scratch = JointBlood {
            counter: TOUCH_FRAME + 1,
            flags: TOUCHED,
            ..JointBlood::default()
        };
        // Within 200 of the next joint's height: it falls without turning.
        step(&mut world, &mut scratch, BOUNCE_Y + 200, 0);
        assert_eq!(world.r, crate::anim::IDENTITY_MATRIX);

        // Further off, it turns the other way to how it left the body.
        let mut world = upright([0, BOUNCE_Y, 0]);
        let mut scratch = JointBlood {
            counter: TOUCH_FRAME + 1,
            flags: TOUCHED,
            ..JointBlood::default()
        };
        step(&mut world, &mut scratch, BOUNCE_Y + 201, 0);
        assert_eq!(
            world.r,
            crate::anim::mul_matrix0(
                &crate::anim::IDENTITY_MATRIX,
                &crate::anim::rotation_matrix(-48, -128, 0)
            )
        );
    }

    #[test]
    fn a_newly_cut_joint_is_let_go_where_the_compose_left_it() {
        let mut game = GameState::default();
        game.entities[1].status_flags = 1;
        let flag = game.entities[1].joint_flag(4);
        game.entities[1].set_joint_flag(4, flag | 12);
        game.entities[1].joint_blood[4].counter = 9;
        let composed: Vec<Mat4x3> = (0..6).map(|i| upright([i * 10, -1000, 0])).collect();
        let mut worlds = composed.clone();

        settle(&mut game, 1);
        advance(&mut game, 1, &mut worlds);

        let entity = &game.entities[1];
        assert_eq!(entity.joint_flag(4) & 0x5E, 0x54, "loose, and still cut");
        assert_eq!(entity.joint_flag(4) & 1, 1, "and still drawn");
        assert_eq!(entity.joint_blood[4].counter, 1);
        // Moved a frame from the composed world, which the joints under it
        // still compose from.
        assert_eq!(worlds[4].t, [21, -1200, 0]);
        assert_eq!(game.entity_anims[1].loose, vec![(4, worlds[4])]);
        assert_eq!(game.entity_anims[1].held, vec![(4, composed[4])]);
        // Uncut joints are left alone.
        assert_eq!(worlds[3], composed[3]);
        assert_eq!(worlds[5], composed[5]);
    }

    #[test]
    fn the_joints_under_a_loose_one_follow_a_frame_behind() {
        let mut game = GameState::default();
        game.entities[1].status_flags = 1;
        let flag = game.entities[1].joint_flag(4);
        game.entities[1].set_joint_flag(4, flag | 12);
        let mut worlds: Vec<Mat4x3> = (0..6).map(|_| upright([0, -1000, 0])).collect();
        settle(&mut game, 1);
        advance(&mut game, 1, &mut worlds);
        let first = worlds[4];

        // The next frame composes from where the first move left the joint,
        // and only then moves it again.
        settle(&mut game, 1);
        assert_eq!(game.entity_anims[1].held, vec![(4, first)]);
        let mut worlds: Vec<Mat4x3> = (0..6).map(|_| first).collect();
        advance(&mut game, 1, &mut worlds);
        assert_eq!(game.entity_anims[1].held, vec![(4, first)]);
        assert_ne!(game.entity_anims[1].loose[0].1, first);
        assert_eq!(worlds[5], first);
    }
}
