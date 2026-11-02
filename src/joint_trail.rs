//! The gore strips of a burst joint: `joint_setup_attack_effect`.
//!
//! A joint in this state is hidden, and its triangles fly apart instead, each
//! group pushed out along its own first vertex and the whole strip falling
//! faster every frame. A zombie's head burst, its rocket gore and the Tyrant's
//! death all draw this way.
//!
//! The original (`FUN_0048a210` and its workers at `0x4399b0`) rewrites a
//! per-slot vertex buffer each frame. Here each frame builds the same
//! triangles into a throwaway [`Tmd`], one object per joint, drawn with the
//! joint's world matrix through the shared mesh path.

use crate::anim::Mat4x3;
use crate::game::{GameState, JointTrail, JointsBeforeHit};
use crate::model::{Tmd, TmdObject, TmdPrim};

/// The strip slots, shared by every entity (`g_trailSlotUsed`). A joint armed
/// with none free stays hidden and draws nothing.
const SLOTS: usize = 16;

/// A held joint's world height stops falling here.
const LOWEST_Y: i32 = 10000;

/// The unlit colour, scaled by 1/128 at `0x439e46`: dark red gore.
const GORE: [u8; 3] = [0x60, 0x28, 0x28];
/// The spider and the wasp bleed green (ids 3 and 4).
const GREEN_GORE: [u8; 3] = [0x18, 0x30, 0x18];

/// `joint_setup_attack_effect`: `kind` frames of strip, falling `timer` per
/// frame per frame, its triangles in groups of `frame_match`.
pub fn arm(
    game: &mut GameState,
    slot: usize,
    joint: usize,
    kind: u8,
    timer: i16,
    frame_match: u16,
) {
    if joint >= 32 || game.joint_trails[slot][joint].step & 0x80 != 0 {
        return;
    }
    let free = game
        .joint_trails
        .iter()
        .flatten()
        .filter(|trail| trail.has_slot)
        .count()
        < SLOTS;
    let after_compose = game.effect_pass_hit;
    let entity = &mut game.entities[slot];
    // `joint_enable_special_effect` only acts on a joint still drawn, but the
    // setup writes the parameters over a running strip regardless.
    let flag = entity.joint_flag(joint);
    let starts = flag & 1 != 0;
    if starts {
        entity.set_joint_flag(joint, (flag & !1) | 0x28);
    }
    let colour = if matches!(entity.id, 3 | 4) {
        GREEN_GORE
    } else {
        GORE
    };
    let trail = &mut game.joint_trails[slot][joint];
    if starts {
        trail.active = true;
        trail.has_slot = free;
        trail.colour = colour;
        trail.after_compose = after_compose;
    }
    trail.total = kind;
    trail.timer = timer;
    trail.frame_match = frame_match;
}

/// Weapon damage from the effect pass (`effect_rocket_explode`, the grenade
/// rounds). The original has already composed and drawn every entity by then,
/// so a strip armed here freezes on next tick's pose, and this tick's frame
/// still shows the joints whole, though in the colours of the hit: those are
/// written to the packets the frame has yet to flush.
pub fn damage_after_compose(game: &mut GameState, damage: impl FnOnce(&mut GameState) -> u8) -> u8 {
    let before: Vec<_> = game
        .entities
        .iter()
        .map(|entity| JointsBeforeHit {
            joint_flags: entity.joint_flags,
            fresh: true,
        })
        .collect();
    game.effect_pass_hit = true;
    let hit = damage(game);
    game.effect_pass_hit = false;
    for (entity, before) in game.entities.iter_mut().zip(before) {
        if entity.joint_flags != before.joint_flags {
            entity.joints_before_hit = Some(before);
        }
    }
    hit
}

/// One tick of every running strip, at the point the original's
/// `render_entity` steps it: after the whole frame's logic, so a joint armed
/// this tick draws its first frame this tick. The frame counter outlives the
/// last drawn frame by two ticks before the joint is let go.
pub fn advance(game: &mut GameState) {
    for slot in 0..game.entities.len() {
        match &mut game.entities[slot].joints_before_hit {
            Some(before) if before.fresh => before.fresh = false,
            before => *before = None,
        }
        if game.entities[slot].status_flags & 1 == 0 {
            continue;
        }
        for joint in 0..32 {
            let mut trail = game.joint_trails[slot][joint];
            trail.drawn = None;
            if !trail.active || trail.after_compose {
                trail.after_compose = false;
                game.joint_trails[slot][joint] = trail;
                continue;
            }
            // `rotate_entity`'s freeze on the first compose: the world matrix
            // the pose gives now is the last one the joint computes.
            if trail.world.is_none() {
                trail.world = game.joint_worlds[slot].get(joint).copied();
                trail.step = 0;
                let entity = &mut game.entities[slot];
                let flag = entity.joint_flag(joint);
                entity.set_joint_flag(joint, (flag & 0xF5) | 0x50);
            }
            trail.compose_world = trail.world;
            let frame = trail.step;
            if let Some(world) = &mut trail.world {
                let fallen = world.t[1].saturating_add(i32::from(trail.timer) * i32::from(frame));
                world.t[1] = if fallen > LOWEST_Y { LOWEST_Y } else { fallen };
            }
            if trail.has_slot && frame < trail.total {
                trail.drawn = Some(frame);
            }
            trail.step = frame.wrapping_add(1);
            if trail.total < frame {
                trail.step = 0x80;
                trail.active = false;
                trail.has_slot = false;
                let entity = &mut game.entities[slot];
                let flag = entity.joint_flag(joint);
                entity.set_joint_flag(joint, flag & 0xDE);
            }
            game.joint_trails[slot][joint] = trail;
        }
    }
}

/// The joints the armed strips hold at a fixed world matrix, for the compose.
pub fn held_joints(trails: &[JointTrail; 32]) -> Vec<(usize, Mat4x3)> {
    trails
        .iter()
        .enumerate()
        .filter_map(|(joint, trail)| trail.compose_world.map(|world| (joint, world)))
        .collect()
}

/// The first CLUT row of a model's page. `0x4399b0` looks the material up and
/// stops at the first record matching either key, and every model record has
/// x 0, so each strip takes the page's first material; the UVs are `u / 256`
/// with no texture page offset, which [`strip_object`] gets by clearing the
/// page bits.
const FIRST_MATERIAL_CLUT: u16 = 480 << 6;

/// `(v >> 4) * remaining + (v >> 1) * frame` per axis: the push of one group.
fn push(vertex: [i16; 3], frame: u8, total: u8) -> [i32; 3] {
    let frame = i32::from(frame);
    let remaining = i32::from(total) - frame;
    vertex.map(|axis| i32::from(axis >> 4) * remaining + i32::from(axis >> 1) * frame)
}

/// The packets a strip keeps: the textured gouraud triangles (`0x34000609`,
/// with or without the blend bit).
fn is_strip_packet(prim: &TmdPrim) -> bool {
    prim.textured && prim.quad.is_none() && !prim.raw_y && prim.flat_color.is_none()
}

/// One joint's triangles at `frame`, drawn from both sides (`triA` and its
/// reverse `triB`).
fn strip_object(source: &TmdObject, frame: u8, total: u8, frame_match: u16) -> TmdObject {
    let mut object = TmdObject::default();
    let Some(first) = source.prims.first().filter(|prim| is_strip_packet(prim)) else {
        return object;
    };
    let packets: Vec<&TmdPrim> = source.prims.iter().filter(|p| is_strip_packet(p)).collect();
    let vertex = |index: u16| {
        source
            .vertices
            .get(usize::from(index))
            .copied()
            .unwrap_or([0; 3])
    };
    let Some(last) = packets.last() else {
        return object;
    };
    object.vertices = vec![[0; 3]; packets.len() * 3];
    object.prims.reserve(packets.len() * 2);
    let mut moved = push(vertex(last.vertices[0]), frame, total);
    for (index, packet) in packets.iter().enumerate().rev() {
        if index % usize::from(frame_match.max(1)) == 0 {
            moved = push(vertex(packet.vertices[0]), frame, total);
        }
        let base = (index * 3) as u16;
        for corner in 0..3 {
            let [x, y, z] = vertex(packet.vertices[corner]);
            object.vertices[usize::from(base) + corner] = [
                (i32::from(x) + moved[0]).clamp(-32768, 32767) as i16,
                (-(i32::from(y) + moved[1])).clamp(-32768, 32767) as i16,
                (i32::from(z) + moved[2]).clamp(-32768, 32767) as i16,
            ];
        }
    }
    for (index, packet) in packets.iter().enumerate() {
        let base = (index * 3) as u16;
        let prim = |corners: [usize; 3]| TmdPrim {
            vertices: corners.map(|corner| base + corner as u16),
            normals: [0; 3],
            uv: corners.map(|corner| packet.uv[corner]),
            clut: FIRST_MATERIAL_CLUT,
            tsb: first.tsb & !0xF,
            textured: true,
            blend: false,
            additive: false,
            raw_y: true,
            flat_color: None,
            quad: None,
            uv_nudge: [0; 3],
        };
        object.prims.push(prim([0, 1, 2]));
        object.prims.push(prim([0, 2, 1]));
    }
    object
}

/// One entity's strips this frame, ready for an `EntityMesh`.
pub struct Strips {
    pub mesh: Tmd,
    pub joints: Vec<Mat4x3>,
    pub colours: [[u8; 3]; 32],
    /// Joints without a strip.
    pub hidden: u32,
}

impl Strips {
    /// `in_zone` tests a joint's world position against the camera's switch
    /// zone; a strip outside it does not draw that frame but keeps counting.
    pub fn build(
        trails: &[JointTrail; 32],
        mesh: &Tmd,
        joints: &[Mat4x3],
        in_zone: impl Fn([i32; 3]) -> bool,
    ) -> Option<Self> {
        if !trails.iter().any(|trail| trail.drawn.is_some()) {
            return None;
        }
        let mut strips = Self {
            mesh: Tmd {
                objects: vec![TmdObject::default(); joints.len().min(mesh.objects.len())],
            },
            joints: joints.to_vec(),
            colours: crate::render::NO_JOINT_COLOURS,
            hidden: u32::MAX,
        };
        let mut any = false;
        for (index, trail) in trails.iter().enumerate() {
            let Some(frame) = trail.drawn else {
                continue;
            };
            let (Some(source), Some(drawn)) =
                (mesh.objects.get(index), strips.joints.get_mut(index))
            else {
                continue;
            };
            if let Some(world) = trail.world {
                *drawn = world;
            }
            let world = *drawn;
            if !in_zone(world.t) {
                continue;
            }
            strips.mesh.objects[index] =
                strip_object(source, frame, trail.total, trail.frame_match);
            strips.colours[index] = trail.colour;
            strips.hidden &= !(1 << index);
            any = true;
        }
        any.then_some(strips)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(vertices: [u16; 3]) -> TmdPrim {
        TmdPrim {
            vertices,
            normals: [0; 3],
            uv: [[1, 2], [3, 4], [5, 6]],
            clut: 0x7800,
            tsb: 3,
            textured: true,
            blend: false,
            additive: false,
            raw_y: false,
            flat_color: None,
            quad: None,
            uv_nudge: [0; 3],
        }
    }

    fn source() -> TmdObject {
        TmdObject {
            vertices: vec![
                [160, 320, -160],
                [16, 0, 0],
                [0, 16, 0],
                [320, -32, 64],
                [32, 0, 0],
                [0, 32, 0],
            ],
            normals: Vec::new(),
            prims: vec![packet([0, 1, 2]), packet([3, 4, 5])],
        }
    }

    #[test]
    fn a_strip_pushes_each_group_along_its_first_vertex() {
        let object = strip_object(&source(), 2, 30, 2);
        // Index 1 has no group of its own and flies with the last vertex's
        // push; index 0 starts a group.
        let last = push([320, -32, 64], 2, 30);
        assert_eq!(
            last,
            [
                (20 * 28) + (160 * 2),
                (-2 * 28) + (-16 * 2),
                (4 * 28) + (32 * 2)
            ]
        );
        assert_eq!(
            object.vertices[3],
            [
                320 + last[0] as i16,
                32 - last[1] as i16,
                64 + last[2] as i16
            ]
        );
        let first = push([160, 320, -160], 2, 30);
        assert_eq!(object.prims[0].clut, FIRST_MATERIAL_CLUT);
        assert_eq!(object.prims[0].tsb, 0);
        assert_eq!(
            object.vertices[0],
            [
                160 + first[0] as i16,
                -320 - first[1] as i16,
                -160 + first[2] as i16
            ]
        );
    }
}
