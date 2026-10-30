//! CPU-side packing for the GPU renderer: the static per-mesh vertex
//! buffers, the per-draw uniform blocks and the frame's screen-space sprite
//! list. Nothing here touches SDL, so the maths is unit-tested without a GPU.
//!
//! The conventions mirror the software rasterizer in [`crate::render`]:
//!
//! * A mesh vertex is stored with the input Y conjugation already applied
//!   (`D v`, or `v` for a raw-Y packet), and each joint's uniform rows are
//!   the reference's object view transform (`CompMatrix` of the stored
//!   camera and the joint, with the shake tilt), the same terms
//!   `collect_triangles` projects through.
//! * The shader projects `sx = 160 + shake_x + vx * (fov / vz)` and
//!   `sy = 120 + shake_y - vy * (fov / vz)` like the software, then maps
//!   them through the reference's ortho matrix with `w = vz`.
//! * Every vertex carries its packet's corners, so a packet with any corner
//!   inside the near limit (`max(2 * fov, 1)`) is dropped whole and a packet
//!   is culled on its first three corners, as in software.
//! * Depth is the reference's linear ramp `TmdDepthNdc` written as `z / w`,
//!   so it interpolates linearly in screen space like llvmpipe's. Sprites
//!   without geometry (masks, effects) take the same mapping of their
//!   painter's key, which is a view-Z in the same units.

use crate::anim::Mat4x3;
use crate::model::{Texture8, Tmd};
use crate::render::{
    self, ABE_BIT, Camera, EntityLight, EntityMesh, GameplayScene, MirrorPass, apply_matrix_sv,
    mul_matrix0, stored_camera_matrix,
};
use crate::state::Image;

/// The logical frame the software rasterizer draws.
pub const LOGICAL_WIDTH: f32 = 320.0;
/// The logical frame the software rasterizer draws.
pub const LOGICAL_HEIGHT: f32 = 240.0;
/// The joints one draw can address: the software path's `hidden_joints` mask
/// is 32 bits, and no shipped model has more objects.
pub const MAX_JOINTS: usize = 32;

/// `in_packed` bits 16..22: which of `MeshVertex::corners` are the
/// triangle's three, two bits each (for its painter's key).
const PACKED_TRIANGLE_SHIFT: u32 = 16;
/// `in_packed` bit: the packet is semi-transparent (ABE or the TMD flag).
const PACKED_BLEND: u32 = 1 << 24;
/// `in_packed` bit: an untextured packet painting its colour word.
const PACKED_UNTEXTURED: u32 = 1 << 25;
/// `in_packed` bits 26..29: the corner's `uv_nudge` (`TmdPrim::uv_nudge`).
const PACKED_NUDGE_SHIFT: u32 = 26;
/// `in_packed` bits 29..31: which of `MeshVertex::corners` the vertex is.
const PACKED_CORNER_SHIFT: u32 = 29;
/// `in_packed` bit: a blending packet adds to the background
/// (`TmdPrim::additive`), drawn lit in the additive pass.
pub const PACKED_ADDITIVE: u32 = 1 << 31;

/// One mesh corner in the static vertex buffer (84 bytes).
/// A `repr(C)` type made only of 4-byte scalars, with no padding, so its
/// bytes can be uploaded as they are.
///
/// # Safety
///
/// Implementors must have no padding bytes.
pub unsafe trait Plain: Copy {}

// SAFETY: the vertex types are `repr(C)` runs of `f32`/`u32` fields.
unsafe impl Plain for MeshVertex {}
unsafe impl Plain for SpriteVertex {}
unsafe impl Plain for crate::htexture::ModelVertex {}
// SAFETY: a bare integer.
unsafe impl Plain for u16 {}
// SAFETY: plain scalars and arrays of them, with no padding.
unsafe impl Plain for u8 {}
unsafe impl Plain for [f32; 4] {}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshVertex {
    /// Every corner of the vertex's packet, stored with the input Y
    /// conjugation applied: the three the software culls the packet on (a
    /// quad's first fan triangle), then a quad's remaining corner (the first
    /// again for a triangle). The vertex is `corners[corner]`; the shader
    /// drops the whole packet when any corner is inside the near limit or the
    /// packet faces away, exactly like `collect_triangles`.
    pub corners: [[f32; 3]; 4],
    /// Stored normal with its Y negated (the reference's `PSXObjVtx` form).
    pub normal: [f32; 3],
    /// Texel coordinates including the packet's texture-page offset.
    pub uv: [f32; 2],
    /// An untextured packet's colour word / 1024; white otherwise.
    pub colour: [f32; 3],
    /// `joint | palette_row << 8 | triangle_corners << 16 | blend << 24 |
    /// untextured << 25 | uv_nudge << 26 | corner << 29`.
    pub packed: u32,
}

/// A mesh's static vertex buffer contents: every packet expanded to
/// triangles, the semi-transparent packets last so the blend pass can draw
/// just `blend_start..vertices.len()`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MeshVertices {
    pub vertices: Vec<MeshVertex>,
    pub blend_start: u32,
}

/// Expand a TMD into its static vertex buffer.
///
/// Matches `collect_triangles` on the gameplay path: the page offsets come
/// from the texpage word, the palette row from the CLUT word, a zero or
/// missing corner normal borrows the packet's first real one and a packet with
/// no usable normal is dropped, as is one with an out-of-range vertex.
pub fn mesh_vertices(mesh: &Tmd) -> MeshVertices {
    let mut opaque = Vec::new();
    let mut blended = Vec::new();
    for (joint, object) in mesh.objects.iter().enumerate() {
        let joint = joint.min(0xFF) as u32;
        for prim in &object.prims {
            // The packet's corners in the order the shader tests them.
            let ring = match prim.quad {
                Some(quad) => [quad[0], quad[1], quad[3], quad[2]],
                None => [
                    prim.vertices[0],
                    prim.vertices[1],
                    prim.vertices[2],
                    prim.vertices[0],
                ],
            };
            let Some(ring_positions) = ring
                .iter()
                .map(|&index| object.vertices.get(usize::from(index)))
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            let Some(slots) = prim
                .vertices
                .iter()
                .map(|vertex| ring.iter().position(|index| index == vertex))
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            let mut normals = prim
                .normals
                .map(|index| object.normals.get(usize::from(index)).copied());
            let Some(substitute) = normals
                .iter()
                .flatten()
                .copied()
                .find(|normal| normal != &[0; 3])
            else {
                continue;
            };
            for normal in &mut normals {
                if normal.is_none() || normal == &Some([0; 3]) {
                    *normal = Some(substitute);
                }
            }
            let blend = prim.blend || prim.clut & ABE_BIT != 0;
            let page_x = f32::from(prim.tsb & 0xF) * 128.0;
            let page_y = f32::from((prim.tsb >> 4) & 1) * 256.0;
            let row = u32::from(((prim.clut >> 6) & 0x01FF).saturating_sub(480));
            let colour = prim
                .flat_color
                .map_or([1.0; 3], |colour| colour.map(|c| f32::from(c) / 1024.0));
            let triangle = slots.iter().enumerate().fold(0, |bits, (corner, &slot)| {
                bits | (slot as u32) << (2 * corner)
            });
            let mut packed = joint | (row & 0xFF) << 8 | triangle << PACKED_TRIANGLE_SHIFT;
            if blend {
                packed |= PACKED_BLEND;
            }
            if prim.flat_color.is_some() {
                packed |= PACKED_UNTEXTURED;
            }
            if blend && prim.additive {
                packed |= PACKED_ADDITIVE;
            }
            let corners: [[f32; 3]; 4] = std::array::from_fn(|corner| {
                let vertex = ring_positions[corner];
                let y = if prim.raw_y { vertex[1] } else { -vertex[1] };
                [f32::from(vertex[0]), f32::from(y), f32::from(vertex[2])]
            });
            let target = if blend { &mut blended } else { &mut opaque };
            for (corner, &slot) in slots.iter().enumerate() {
                let normal = normals[corner].unwrap_or(substitute);
                let nudge = u32::from(prim.uv_nudge[corner] & 7);
                target.push(MeshVertex {
                    corners,
                    normal: [
                        f32::from(normal[0]),
                        -f32::from(normal[1]),
                        f32::from(normal[2]),
                    ],
                    uv: [
                        f32::from(prim.uv[corner][0]) + page_x,
                        f32::from(prim.uv[corner][1]) + page_y,
                    ],
                    colour,
                    packed: packed
                        | nudge << PACKED_NUDGE_SHIFT
                        | (slot as u32) << PACKED_CORNER_SHIFT,
                });
            }
        }
    }
    let blend_start = opaque.len() as u32;
    opaque.extend(blended);
    MeshVertices {
        vertices: opaque,
        blend_start,
    }
}

/// The vertex-stage uniform block of one mesh draw (`mesh.vert`'s `Draw`).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawUniforms {
    /// `fov, shake_x, shake_y, near`.
    pub projection: [f32; 4],
    pub ambient: [f32; 4],
    pub light_colour: [[f32; 4]; 3],
    pub tint: [f32; 4],
    /// `hidden_joints, mode (`DRAW_*`), texture width, texture height`.
    pub flags: [u32; 4],
    /// Per joint: the three rows of the reference's object view transform
    /// (`render::object_view`), then the three light vectors.
    pub joints: [[f32; 4]; MAX_JOINTS * 6],
}

impl DrawUniforms {
    /// The block as bytes for `SDL_PushGPUVertexUniformData`.
    pub fn as_bytes(&self) -> &[u8] {
        // SAFETY: `DrawUniforms` is `repr(C)` and made only of 4-byte scalars,
        // so it has no padding and every byte is initialized.
        unsafe {
            std::slice::from_raw_parts(
                (self as *const Self).cast::<u8>(),
                std::mem::size_of::<Self>(),
            )
        }
    }
}

/// The projection terms shared by every draw through `camera`: `fov`, the
/// shake offset and the software's near limit.
fn camera_terms(camera: &Camera) -> [f32; 4] {
    let fov = camera.fov as f32;
    [
        fov,
        camera.screen[0] as f32,
        camera.screen[1] as f32,
        (2.0 * fov).max(1.0),
    ]
}

/// The view-space point `mesh.vert` computes for a vertex-buffer position
/// through one joint's rows.
#[cfg(test)]
fn view_point(rows: &[[f32; 4]], position: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|row| {
        let r = rows[row];
        r[0] * position[0] + r[1] * position[1] + r[2] * position[2] + r[3]
    })
}

/// The per-joint light vector the shader dots with the stored normal.
///
/// `reference_vertex_shade` computes `d = -(n_view . l) / 4096` with
/// `n_view = C R n' / 4096^3` and `l = ApplyMatrixSV(C, dir)`; folding the
/// matrices onto the light gives `d = -dot(n', (C R)^T l / 4096^4)`, so the
/// GPU evaluates one dot product per light and corner.
fn light_vector(
    stored: &[[i32; 3]; 3],
    joint: &Mat4x3,
    dir: [i32; 3],
    room_object: bool,
) -> [f32; 4] {
    let vec = if room_object {
        mul_matrix0(&[dir, [0; 3], [0; 3]], &joint.r)[0]
    } else {
        dir
    };
    let l = apply_matrix_sv(stored, vec);
    // The draw matrix is `MulMatrix0`'s rounded 4.12 short product.
    let draw = mul_matrix0(stored, &joint.r).map(|row| row.map(|entry| entry as i16));
    let mut out = [0.0f32; 4];
    for (k, slot) in out.iter_mut().take(3).enumerate() {
        let mut sum = 0.0f64;
        for (row, &l_row) in l.iter().enumerate() {
            sum += f64::from(draw[row][k]) * 4096.0 * f64::from(l_row);
        }
        *slot = (sum / 4096f64.powi(4)) as f32;
    }
    out
}

/// `flags.y` of a mesh draw: [`DRAW_CULL`] unless it is the mirror copy, and
/// a transparent record's [`DRAW_RECORD`] bits.
fn draw_mode(blend_weight: Option<u8>, cull: bool) -> u32 {
    let mut mode = if cull { DRAW_CULL } else { 0 };
    if let Some(weight) = blend_weight {
        mode |= DRAW_RECORD;
        if weight != 0 {
            mode |= DRAW_RECORD_BLENDS;
        }
    }
    mode
}

/// Which vertices of a mesh's buffer the pass draws, as `(first, count)` of
/// `vertex_count` with the blended packets from `blend_start`: the opaque
/// pass draws them all (its shader drops the blending texels), the blend pass
/// the blended packets, or every packet of a blending record and none of a
/// record at weight zero.
pub fn pass_range(
    blend_weight: Option<u8>,
    pass: MeshPass,
    blend_start: u32,
    vertex_count: u32,
) -> (u32, u32) {
    match (pass != MeshPass::Opaque, blend_weight) {
        (false, _) | (true, Some(1..)) => (0, vertex_count),
        (true, Some(0)) => (0, 0),
        (true, None) => (blend_start, vertex_count - blend_start),
    }
}

/// The uniform block of one mesh draw through `camera`.
///
/// `room_object` selects the room pass's light rotation (the software
/// `room_object` flag; never set for the mirror copy). `mirror` hides the
/// joints the mirror probe rejects, on top of the mesh's own hidden joints,
/// and draws unculled like the software's reflection.
pub fn draw_uniforms(
    camera: &Camera,
    mesh: &EntityMesh<'_>,
    light: &EntityLight,
    room_object: bool,
    mirror: Option<&MirrorPass>,
) -> DrawUniforms {
    let projection = camera_terms(camera);
    let stored = stored_camera_matrix(camera.from, camera.look_at);
    let mut joints = [[0.0f32; 4]; MAX_JOINTS * 6];
    let mut hidden = 0u32;
    for index in 0..MAX_JOINTS {
        let joint = mesh.joints.get(index);
        let visible = index < mesh.mesh.objects.len()
            && mesh.joint_visible(index)
            && joint.is_some_and(|joint| mirror.is_none_or(|mirror| mirror.joint_visible(joint.t)));
        let Some(joint) = joint.filter(|_| visible) else {
            hidden |= 1 << index;
            continue;
        };
        let base = index * 6;
        let (rotation, translation, _) = render::object_view(camera, joint);
        for row in 0..3 {
            let r = rotation[row];
            joints[base + row] = [
                r[0] as f32,
                r[1] as f32,
                r[2] as f32,
                translation[row] as f32,
            ];
        }
        for (slot, source) in light.sources.iter().enumerate() {
            joints[base + 3 + slot] = light_vector(&stored, joint, source.dir, room_object);
        }
    }
    let ambient = light.ambient.map(|channel| channel as f32);
    DrawUniforms {
        projection,
        ambient: [ambient[0], ambient[1], ambient[2], 0.0],
        light_colour: light.sources.map(|source| {
            let colour = source
                .color
                .map(|channel| f32::from(channel) * 255.0 / 128.0);
            [colour[0], colour[1], colour[2], 0.0]
        }),
        tint: [
            f32::from(mesh.tint[0]),
            f32::from(mesh.tint[1]),
            f32::from(mesh.tint[2]),
            0.0,
        ],
        flags: [
            hidden,
            draw_mode(mesh.blend_weight, mirror.is_none()),
            mesh.texture.width,
            mesh.texture.height,
        ],
        joints,
    }
}

/// The clip-space position `mesh.vert` computes for a view-space point, for
/// tests: the software's float projection (`RefProjection::project`) mapped
/// through the reference's ortho matrix and premultiplied by `w = vz`, with
/// the linear depth ramp of `TmdDepthNdc`.
pub fn clip_position(view: [f32; 3], fov: f32, shake: [f32; 2]) -> [f32; 4] {
    let iz = fov / view[2];
    let sx = 160.0 + shake[0] + view[0] * iz;
    let sy = 120.0 + shake[1] - view[1] * iz;
    let w = view[2];
    [
        (sx * (2.0 / LOGICAL_WIDTH) - 1.0) * w,
        (1.0 - sy * (2.0 / LOGICAL_HEIGHT)) * w,
        render::depth_ndc(w) * w,
        w,
    ]
}

/// The normalized depth of a painter's key (a view-Z): the depth a mesh
/// fragment at that view-Z would have (`TmdDepthNdc`).
pub fn key_depth(key: u32) -> f32 {
    render::depth_ndc(key as f32)
}

/// The fragment-stage uniform block of a mesh draw (`mesh.frag`'s
/// `Material`): `x` the alpha of a blended texel, `render::blend_alpha`
/// rounded to unorm8 as the software blend rounds it, `y` the pass (0
/// opaque, 1 blend, 2 additive).
pub fn material(blend_weight: Option<u8>, pass: MeshPass) -> [f32; 4] {
    let alpha = (render::blend_alpha(blend_weight) * 255.0).round_ties_even() / 255.0;
    let pass = match pass {
        MeshPass::Opaque => 0.0,
        MeshPass::Blend => 1.0,
        MeshPass::Additive => 2.0,
    };
    [alpha, pass, 0.0, 0.0]
}

/// The passes of a mesh draw, in order: its opaque texels, then the STP
/// texels of its blending packets, then those of its additive packets
/// (`PACKED_ADDITIVE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeshPass {
    Opaque,
    Blend,
    Additive,
}

/// `flags.y` bit: cull packets facing away.
pub const DRAW_CULL: u32 = 1;
/// `flags.y` bit: the mesh is a transparent record (`EntityMesh::blend_weight`):
/// every packet is full-bright and blends by [`DRAW_RECORD_BLENDS`] alone.
pub const DRAW_RECORD: u32 = 2;
/// `flags.y` bit: a record's packets all blend (a non-zero weight).
pub const DRAW_RECORD_BLENDS: u32 = 4;

/// `sprite.frag` modes.
pub const SPRITE_TEXTURED: u32 = 0;
pub const SPRITE_SHADOW: u32 = 1;
pub const SPRITE_BACKGROUND: u32 = 2;

/// One screen-space sprite corner (48 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpriteVertex {
    pub clip: [f32; 4],
    pub uv: [f32; 2],
    pub colour: [f32; 4],
    pub mode: u32,
    /// The painter's key at or behind which a mask hides the sprite
    /// ([`NEVER_MASKED`] for none); see [`MASK_KEY_RANGE`].
    pub key: f32,
}

/// A sprite key no mask hides.
pub const NEVER_MASKED: f32 = -1.0;

/// The mask-key target's scale: a D16 depth of `key / MASK_KEY_RANGE`
/// stores a mask's integer painter's key exactly, and the cleared 1.0 is "no
/// mask". A mask key past the range (none ships one; `fade` buckets top out
/// at 65520) clamps just under it.
pub const MASK_KEY_RANGE: f32 = 65535.0;

/// The mask-key target depth of a mask's painter's key.
pub fn mask_key_depth(key: u32) -> f32 {
    key.min(MASK_KEY_RANGE as u32 - 1) as f32 / MASK_KEY_RANGE
}

/// A contiguous run of sprite triangles sampling one image.
#[derive(Debug, Clone, Copy)]
pub struct SpriteRun<'a> {
    pub image: &'a Image,
    pub first: u32,
    pub count: u32,
    /// The camera background a mask run's page belongs to, which names the
    /// mask's HD replacement when its own hash doesn't
    /// ([`crate::htexture::HdTable::paired_masks`]).
    pub background: Option<&'a Image>,
}

/// The frame's sprites: `before` draws ahead of the meshes (background,
/// masks, shadows), `after` behind them (the effect billboards).
/// `mask_keys` draws first into the mask-key target: the masks again, each
/// at [`mask_key_depth`] of its key, so the target keeps the nearest mask's
/// key per pixel.
///
/// That target stands in for the painter's sort where a mask is involved:
/// the software sorts shadows, triangles, masks and effects (submitted in
/// that order) stably far to near, so a mask paints over a shadow or
/// triangle whose key is at or behind its own and over an effect strictly
/// behind it. Meshes, shadows and effects drop a fragment whose key the
/// target says a mask covers, and the masks write no scene depth.
#[derive(Debug, Default)]
pub struct Sprites<'a> {
    pub vertices: Vec<SpriteVertex>,
    pub mask_keys: Option<SpriteRun<'a>>,
    pub before: Vec<SpriteRun<'a>>,
    pub after: Vec<SpriteRun<'a>>,
    /// The F4 debug triangles after everything, each run with whether it
    /// writes depth.
    pub debug: Vec<(SpriteRun<'a>, bool)>,
}

/// A logical pixel position to normalized device coordinates.
fn ndc(x: f32, y: f32) -> [f32; 2] {
    [
        x / (LOGICAL_WIDTH / 2.0) - 1.0,
        1.0 - y / (LOGICAL_HEIGHT / 2.0),
    ]
}

impl<'a> Sprites<'a> {
    /// Append one screen rectangle as two triangles.
    fn push_rect(
        &mut self,
        rect: [f32; 4],
        uv: [f32; 4],
        depth: f32,
        colour: [f32; 4],
        mode: u32,
        key: f32,
    ) {
        let [x0, y0, x1, y1] = rect;
        let corner = |x: f32, y: f32, u: f32, v: f32| {
            let [nx, ny] = ndc(x, y);
            SpriteVertex {
                clip: [nx, ny, depth, 1.0],
                uv: [u, v],
                colour,
                mode,
                key,
            }
        };
        let a = corner(x0, y0, uv[0], uv[1]);
        let b = corner(x1, y0, uv[2], uv[1]);
        let c = corner(x0, y1, uv[0], uv[3]);
        let d = corner(x1, y1, uv[2], uv[3]);
        self.vertices.extend([a, b, c, c, b, d]);
    }

    /// Close the vertices pushed since `first` into a run.
    fn run(&self, image: &'a Image, first: usize) -> Option<SpriteRun<'a>> {
        let count = self.vertices.len() - first;
        (count > 0).then_some(SpriteRun {
            image,
            first: first as u32,
            count: count as u32,
            background: None,
        })
    }
}

/// Build the frame's background, mask, shadow and effect sprites.
pub fn scene_sprites<'a>(scene: &GameplayScene<'a>) -> Sprites<'a> {
    let mut sprites = Sprites::default();
    let camera = scene.camera;

    // The background is stretched over the logical frame with its display
    // origin shifted in (`Framebuffer::draw_background`), at the far plane.
    if let Some(image) = scene
        .background
        .filter(|image| image.width > 0 && image.height > 0)
    {
        let first = sprites.vertices.len();
        let (w, h) = (image.width as f32, image.height as f32);
        let (ox, oy) = (scene.origin[0] as f32, scene.origin[1] as f32);
        let colour = scene.colour.map(|channel| f32::from(channel) / 255.0);
        sprites.push_rect(
            [0.0, 0.0, LOGICAL_WIDTH, LOGICAL_HEIGHT],
            [
                -ox * w / LOGICAL_WIDTH,
                -oy * h / LOGICAL_HEIGHT,
                (LOGICAL_WIDTH - ox) * w / LOGICAL_WIDTH,
                (LOGICAL_HEIGHT - oy) * h / LOGICAL_HEIGHT,
            ],
            1.0,
            [colour[0], colour[1], colour[2], 1.0],
            SPRITE_BACKGROUND,
            NEVER_MASKED,
        );
        let run = sprites.run(image, first).map(|run| SpriteRun { ..run });
        sprites.before.extend(run);
    }

    // The masks are opaque cut-outs, far to near; their keys go to the
    // mask-key target first.
    if let Some(layer) = scene.mask_layer {
        let mut quads = render::mask_quads(layer, camera.screen);
        quads.sort_by_key(|quad| std::cmp::Reverse(quad.key));
        for keys in [true, false] {
            let first = sprites.vertices.len();
            for quad in &quads {
                let (x, y) = (quad.pos.0 as f32, quad.pos.1 as f32);
                let (w, h) = (quad.size.0 as f32, quad.size.1 as f32);
                let (u, v) = (quad.uv.0 as f32, quad.uv.1 as f32);
                let depth = if keys {
                    mask_key_depth(quad.key)
                } else {
                    key_depth(quad.key)
                };
                sprites.push_rect(
                    [x, y, x + w, y + h],
                    [u, v, u + w, v + h],
                    depth,
                    [1.0; 4],
                    SPRITE_TEXTURED,
                    NEVER_MASKED,
                );
            }
            let run = sprites.run(layer.page, first).map(|run| SpriteRun {
                background: scene.background,
                ..run
            });
            if keys {
                sprites.mask_keys = run;
            } else {
                sprites.before.extend(run);
            }
        }
    }

    // The shadows keep their perspective-correct UVs: each projected corner
    // goes back to clip space with `w` = its view Z. They blend towards the
    // tint by the page's coverage alpha, far to near.
    let mut shadows = render::shadow_polys(scene.shadows, camera);
    shadows.sort_by_key(|poly| std::cmp::Reverse(poly.key));
    for poly in &shadows {
        let first = sprites.vertices.len();
        let depth = key_depth(poly.key);
        let corner = |vertex: &crate::raster::GlVertex| {
            let w = vertex.w;
            let [nx, ny] = ndc(vertex.x, vertex.y);
            SpriteVertex {
                clip: [nx * w, ny * w, depth * w, w],
                uv: [vertex.u, vertex.v],
                colour: vertex.color,
                mode: SPRITE_SHADOW,
                key: poly.key as f32,
            }
        };
        for triangle in &poly.triangles {
            sprites.vertices.extend(triangle.iter().map(corner));
        }
        sprites.before.extend(sprites.run(poly.texture, first));
    }

    // The effect billboards draw after the meshes, far to near, with the
    // tint record and an optional half blend.
    if let Some(layer) = scene.effect_layer {
        let mut quads: Vec<_> = layer.quads.to_vec();
        quads.sort_by_key(|quad| std::cmp::Reverse(quad.key));
        for quad in &quads {
            let Some(page) = layer.pages.get(quad.page).copied().flatten() else {
                continue;
            };
            if quad.size[0] <= 0 || quad.size[1] <= 0 || page.width == 0 || page.height == 0 {
                continue;
            }
            let first = sprites.vertices.len();
            let (x, y) = (quad.pos[0] as f32, quad.pos[1] as f32);
            let (u, v) = (f32::from(quad.uv[0]), f32::from(quad.uv[1]));
            let source = quad.source.map(|size| f32::from(size.max(1)));
            let (tint, alpha) = quad.colour();
            sprites.push_rect(
                [x, y, x + quad.size[0] as f32, y + quad.size[1] as f32],
                [u, v, u + source[0], v + source[1]],
                key_depth(quad.key),
                [tint[0], tint[1], tint[2], alpha],
                SPRITE_TEXTURED,
                // An effect follows the masks in the submission order, so a
                // mask only covers one strictly behind it.
                quad.key as f32 - 1.0,
            );
            sprites.after.extend(sprites.run(page, first));
        }
    }

    // The F4 debug triangles last, over everything but behind nearer meshes.
    if !scene.debug.is_empty() {
        static WHITE: std::sync::LazyLock<Image> = std::sync::LazyLock::new(|| Image {
            width: 1,
            height: 1,
            rgba: vec![255; 4],
        });
        for group in scene.debug.chunk_by(|a, b| a.write == b.write) {
            let first = sprites.vertices.len();
            for vertex in group {
                let [x, y] = vertex.position;
                let [nx, ny] = ndc(x, y);
                let w = vertex.depth.max(1.0);
                sprites.vertices.push(SpriteVertex {
                    clip: [nx * w, ny * w, render::depth_ndc(w) * w, w],
                    uv: [0.0, 0.0],
                    colour: vertex.colour.map(|channel| f32::from(channel) / 255.0),
                    mode: SPRITE_TEXTURED,
                    key: NEVER_MASKED,
                });
            }
            if let Some(run) = sprites.run(&WHITE, first) {
                sprites.debug.push((run, group[0].write));
            }
        }
    }
    sprites
}

/// The palette texture of an indexed page: 256 entries per row, alpha set
/// where a blending packet blends the entry. That is every entry of a row
/// without a knock-out twin ([`Texture8::has_knockout_twin`]) and only the
/// STP entries of one with it, whose twin paints the rest back opaque. Rows
/// past the end are padded black (the software lookup's out-of-range
/// colour).
pub fn palette_rgba(texture: &Texture8) -> (u32, Vec<u8>) {
    let rows = texture
        .palettes
        .len()
        .div_ceil(crate::model::PALETTE_ROW_LEN)
        .max(1);
    // `has_knockout_twin` per row from one histogram of the page.
    let mut histogram = [0usize; 256];
    for &index in &texture.indices {
        histogram[usize::from(index)] += 1;
    }
    let twins: Vec<bool> = (0..rows)
        .map(|row| {
            let (mut stp, mut opaque) = (0usize, 0usize);
            for (index, &count) in histogram.iter().enumerate() {
                if texture.palette_stp(row, index as u8) {
                    stp += count;
                } else if index != 0 {
                    opaque += count;
                }
            }
            stp > 0 && opaque > stp
        })
        .collect();
    let mut rgba = vec![0u8; rows * crate::model::PALETTE_ROW_LEN * 4];
    for (index, entry) in texture.palettes.iter().enumerate() {
        let stp = texture.stp.get(index).copied().unwrap_or(false);
        let blends = stp || !twins[index / crate::model::PALETTE_ROW_LEN];
        rgba[index * 4..index * 4 + 4].copy_from_slice(&[
            entry[0],
            entry[1],
            entry[2],
            if blends { 255 } else { 0 },
        ]);
    }
    (rows as u32, rgba)
}

/// The identity of a cached upload: `parts` (sizes and allocation
/// addresses) and every byte of `chunks`, hashed a word at a time (a 320x240 background costs
/// tens of microseconds). Residency is keyed on the asset's address, so this
/// catches an address reused by a different asset, or contents changed in
/// place.
pub fn fingerprint(parts: &[u64], chunks: &[&[u8]]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut mix = |value: u64| {
        hash = (hash ^ value)
            .wrapping_mul(0x0000_0100_0000_01b3)
            .rotate_left(29);
    };
    for &part in parts {
        mix(part);
    }
    for bytes in chunks {
        mix(bytes.len() as u64);
        let (words, rest) = bytes.as_chunks::<8>();
        for &word in words {
            mix(u64::from_le_bytes(word));
        }
        for &byte in rest {
            mix(u64::from(byte));
        }
    }
    hash
}

/// [`fingerprint`] of a TMD's geometry: its vertices and packet layout.
pub fn mesh_fingerprint(mesh: &Tmd) -> u64 {
    let mut parts = vec![mesh.objects.len() as u64, mesh.objects.as_ptr() as u64];
    for object in &mesh.objects {
        parts.push(object.prims.len() as u64);
        parts.push(object.prims.as_ptr() as u64);
        parts.push(object.vertices.len() as u64);
        parts.push(object.vertices.as_ptr() as u64);
        for vertex in &object.vertices {
            parts.push(
                u64::from(vertex[0] as u16)
                    | u64::from(vertex[1] as u16) << 16
                    | u64::from(vertex[2] as u16) << 32,
            );
        }
    }
    fingerprint(&parts, &[])
}

/// [`fingerprint`] of an indexed texture page, its palettes and STP flags.
pub fn texture_fingerprint(texture: &Texture8) -> u64 {
    let stp: Vec<u8> = texture.stp.iter().map(|&flag| u8::from(flag)).collect();
    fingerprint(
        &[
            u64::from(texture.width),
            u64::from(texture.height),
            texture.indices.as_ptr() as u64,
            texture.palettes.as_ptr() as u64,
        ],
        &[&texture.indices, texture.palettes.as_flattened(), &stp],
    )
}

/// [`fingerprint`] of an RGBA image.
pub fn image_fingerprint(image: &Image) -> u64 {
    fingerprint(
        &[
            u64::from(image.width),
            u64::from(image.height),
            image.rgba.as_ptr() as u64,
        ],
        &[&image.rgba],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprints_see_every_byte_and_the_stp_flags() {
        let mut image = Image {
            width: 320,
            height: 240,
            rgba: vec![0; 320 * 240 * 4],
        };
        let before = image_fingerprint(&image);
        image.rgba[1234 * 4 + 1] = 1;
        assert_ne!(image_fingerprint(&image), before);

        let mut texture = Texture8 {
            width: 4,
            height: 4,
            indices: vec![1; 16],
            palettes: vec![[0; 4]; crate::model::PALETTE_ROW_LEN],
            stp: vec![false; crate::model::PALETTE_ROW_LEN],
        };
        let before = texture_fingerprint(&texture);
        assert_eq!(texture_fingerprint(&texture), before, "must be stable");
        texture.stp[7] = true;
        let stp = texture_fingerprint(&texture);
        assert_ne!(stp, before);
        texture.palettes[200][2] = 9;
        assert_ne!(texture_fingerprint(&texture), stp);
    }
    use crate::model::{TmdObject, TmdPrim};
    use crate::render::{EntityLightSource, Lighting};

    fn identity() -> Mat4x3 {
        Mat4x3 {
            r: [[4096, 0, 0], [0, 4096, 0], [0, 0, 4096]],
            t: [0, 0, 0],
        }
    }

    fn prim(vertices: [u16; 3]) -> TmdPrim {
        TmdPrim {
            vertices,
            normals: [0, 0, 0],
            uv: [[1, 2], [3, 4], [5, 6]],
            clut: (481 << 6) as u16,
            tsb: 0x11,
            textured: true,
            blend: false,
            additive: false,
            raw_y: false,
            uv_nudge: [0; 3],
            flat_color: None,
            quad: None,
        }
    }

    fn mesh() -> Tmd {
        let mut blended = prim([0, 1, 2]);
        blended.blend = true;
        let mut raw = prim([0, 1, 2]);
        raw.raw_y = true;
        Tmd {
            objects: vec![TmdObject {
                vertices: vec![[10, 20, 30], [40, 50, 60], [70, 80, 90]],
                normals: vec![[0, -4096, 0]],
                prims: vec![blended, prim([0, 1, 2]), raw, prim([0, 1, 9])],
            }],
        }
    }

    /// The vertex's own position among its packet's corners.
    fn position(vertex: &MeshVertex) -> [f32; 3] {
        vertex.corners[(vertex.packed >> PACKED_CORNER_SHIFT & 3) as usize]
    }

    #[test]
    fn mesh_vertices_put_blend_packets_last_and_drop_bad_indices() {
        let built = mesh_vertices(&mesh());
        // Two opaque packets and one blended; the out-of-range one is dropped.
        assert_eq!(built.vertices.len(), 9);
        assert_eq!(built.blend_start, 6);
        let first = built.vertices[0];
        assert_eq!(position(&first), [10.0, -20.0, 30.0]);
        assert_eq!(first.normal, [0.0, 4096.0, 0.0]);
        // Page 1 of the low row: +128 on u, +256 on v; palette row 1.
        assert_eq!(first.uv, [129.0, 258.0]);
        assert_eq!((first.packed >> 8) & 0xFF, 1);
        assert_eq!(first.packed & PACKED_BLEND, 0);
        // A triangle packet's fourth corner repeats its first.
        assert_eq!(first.corners[3], first.corners[0]);
        assert_eq!(position(&built.vertices[2]), [70.0, -80.0, 90.0]);
        // The raw-Y packet keeps the stored Y.
        assert_eq!(position(&built.vertices[3]), [10.0, 20.0, 30.0]);
        assert_ne!(built.vertices[6].packed & PACKED_BLEND, 0);
    }

    #[test]
    fn quad_halves_carry_the_whole_packet_and_their_nudges() {
        // A quad's ring (0, 1, 3, 2) fans into (0, 1, 3) and (0, 3, 2).
        let quad = [0u16, 1, 2, 3];
        let mut halves = [prim([0, 1, 3]), prim([0, 3, 2])];
        for half in &mut halves {
            half.quad = Some(quad);
        }
        halves[0].uv_nudge = [0, 5, 0];
        let tmd = Tmd {
            objects: vec![TmdObject {
                vertices: vec![[0, 0, 0], [100, 0, 0], [0, 100, 0], [100, 100, 0]],
                normals: vec![[0, -4096, 0]],
                prims: halves.to_vec(),
            }],
        };
        let built = mesh_vertices(&tmd);
        assert_eq!(built.vertices.len(), 6);
        // Every vertex lists the quad's culled corners (0, 1, 3), then 2.
        let ring = [
            [0.0, 0.0, 0.0],
            [100.0, 0.0, 0.0],
            [100.0, -100.0, 0.0],
            [0.0, -100.0, 0.0],
        ];
        for vertex in &built.vertices {
            assert_eq!(vertex.corners, ring);
        }
        let slots: Vec<u32> = built
            .vertices
            .iter()
            .map(|vertex| vertex.packed >> PACKED_CORNER_SHIFT & 3)
            .collect();
        assert_eq!(slots, [0, 1, 2, 0, 2, 3]);
        assert_eq!(built.vertices[1].packed >> PACKED_NUDGE_SHIFT & 7, 5);
        assert_eq!(built.vertices[0].packed >> PACKED_NUDGE_SHIFT & 7, 0);
        // A quad with a corner out of range is dropped whole.
        let mut broken = tmd.clone();
        broken.objects[0].vertices.pop();
        assert!(mesh_vertices(&broken).vertices.is_empty());
    }

    #[test]
    fn clip_position_reproduces_the_software_projection() {
        let mut camera = Camera::from_points([1200, -1800, 6000], [0, -900, 0], 600);
        camera.screen = [2, -1];
        let projection = camera_terms(&camera);
        let mut joint = identity();
        joint.r = crate::anim::rotation_matrix(300, 1200, 50);
        joint.t = [100, -400, 250];
        let texture = Texture8 {
            width: 256,
            height: 256,
            indices: vec![],
            palettes: vec![],
            stp: vec![],
        };
        let tmd = mesh();
        let joints = [joint];
        let entity = EntityMesh {
            mesh: &tmd,
            texture: &texture,
            joints: &joints,
            tint: [255; 3],
            blend_weight: None,
            hidden_joints: 0,
            light_pos: None,
            per_joint_light: false,
            light_rotation: None,
            fixed_light: None,
            joint_colours: crate::render::NO_JOINT_COLOURS,
            hd_name: None,
            complex_object: None,
            aliased_light: false,
        };
        let light = Lighting {
            ambient: [0; 3],
            lights: Default::default(),
        }
        .latched([0; 3]);
        let uniforms = draw_uniforms(&camera, &entity, &light, false, None);
        assert_eq!(uniforms.flags, [!1, DRAW_CULL, 256, 256]);
        assert_eq!(projection[3], 1200.0);
        let (rotation, translation, near) = render::object_view(&camera, &joint);
        assert_eq!(near, 1200.0);
        for stored in [[0i16, 900, 0], [300, 1200, -500], [-700, 0, 900]] {
            // The vertex buffer's conjugated position.
            let position = [
                f32::from(stored[0]),
                -f32::from(stored[1]),
                f32::from(stored[2]),
            ];
            // `RefProjection::project` in doubles.
            let p = position.map(f64::from);
            let [vx, vy, vz] = std::array::from_fn(|row| {
                let m = rotation[row];
                m[0] * p[0] + m[1] * p[1] + m[2] * p[2] + translation[row]
            });
            let expected = [162.0 + vx * 600.0 / vz, 119.0 - vy * 600.0 / vz];
            let v = view_point(&uniforms.joints[..3], position);
            let clip = clip_position(v, projection[0], [projection[1], projection[2]]);
            let sx = (clip[0] / clip[3] + 1.0) * 160.0;
            let sy = (1.0 - clip[1] / clip[3]) * 120.0;
            assert!(
                (f64::from(sx) - expected[0]).abs() < 1e-2,
                "{sx} vs {expected:?}"
            );
            assert!(
                (f64::from(sy) - expected[1]).abs() < 1e-2,
                "{sy} vs {expected:?}"
            );
            // The depth is the software's linear ramp, as is a painter's key
            // at the same view-Z.
            let depth = clip[2] / clip[3];
            assert!((depth - render::depth_ndc(vz as f32)).abs() < 1e-6);
            assert!((key_depth(vz as u32) - depth).abs() < 1e-4);
        }
    }

    #[test]
    fn light_vectors_match_the_reference_shade() {
        let camera = Camera::from_points([1200, -1800, 6000], [0, -900, 0], 600);
        let lighting = Lighting {
            ambient: [512, 1024, 2048],
            lights: Default::default(),
        };
        let mut light = lighting.latched([0, 0, 0]);
        light.sources[0] = EntityLightSource {
            dir: [1200, -3000, 2400],
            color: [0x80, 0x40, 0x20],
        };
        light.sources[1] = EntityLightSource {
            dir: [-2000, 1000, -3000],
            color: [0x10, 0x80, 0x40],
        };
        let texture = Texture8 {
            width: 1,
            height: 1,
            indices: vec![0],
            palettes: vec![],
            stp: vec![],
        };
        let tmd = mesh();
        let mut joint = identity();
        joint.r = crate::anim::rotation_matrix(300, 1200, 50);
        let joints = [joint];
        let entity = EntityMesh {
            mesh: &tmd,
            texture: &texture,
            joints: &joints,
            tint: [255; 3],
            blend_weight: None,
            hidden_joints: 0,
            light_pos: None,
            per_joint_light: false,
            light_rotation: None,
            fixed_light: None,
            joint_colours: crate::render::NO_JOINT_COLOURS,
            hd_name: None,
            complex_object: None,
            aliased_light: false,
        };
        let stored = stored_camera_matrix(camera.from, camera.look_at);
        let draw_matrix = mul_matrix0(&stored, &joint.r)
            .map(|row| row.map(|entry| f32::from(entry as i16) / 4096.0));
        for room_object in [false, true] {
            let uniforms = draw_uniforms(&camera, &entity, &light, room_object, None);
            for normal in [[0i16, -4096, 0], [2896, 0, 2896], [-1000, 3000, -2000]] {
                let reference = render::reference_vertex_shade(
                    &stored,
                    &draw_matrix,
                    normal,
                    &light,
                    room_object.then_some(&joint.r),
                );
                let n = [
                    f32::from(normal[0]),
                    -f32::from(normal[1]),
                    f32::from(normal[2]),
                ];
                let mut shade = uniforms.ambient;
                for slot in 0..3 {
                    let l = uniforms.joints[3 + slot];
                    let d = -(n[0] * l[0] + n[1] * l[1] + n[2] * l[2]);
                    if d > 0.0 {
                        for (value, colour) in shade.iter_mut().zip(uniforms.light_colour[slot]) {
                            *value += d * colour;
                        }
                    }
                }
                for channel in 0..3 {
                    assert!(
                        (f64::from(shade[channel]) - reference[channel]).abs() < 0.05,
                        "{shade:?} vs {reference:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn hidden_and_missing_joints_are_masked() {
        let tmd = Tmd {
            objects: vec![TmdObject::default(), TmdObject::default()],
        };
        let texture = Texture8 {
            width: 1,
            height: 1,
            indices: vec![0],
            palettes: vec![],
            stp: vec![],
        };
        let joints = [identity()];
        let entity = EntityMesh {
            mesh: &tmd,
            texture: &texture,
            joints: &joints,
            tint: [255; 3],
            blend_weight: None,
            hidden_joints: 0,
            light_pos: None,
            per_joint_light: false,
            light_rotation: None,
            fixed_light: None,
            joint_colours: crate::render::NO_JOINT_COLOURS,
            hd_name: None,
            complex_object: None,
            aliased_light: false,
        };
        let camera = Camera::from_points([0, 0, -5000], [0, 0, 0], 500);
        let light = Lighting {
            ambient: [0; 3],
            lights: Default::default(),
        }
        .latched([0; 3]);
        let uniforms = draw_uniforms(&camera, &entity, &light, false, None);
        // Joint 0 draws; joint 1 has no matrix; joints past the mesh never draw.
        assert_eq!(uniforms.flags[0], !1);
        let entity = EntityMesh {
            hidden_joints: 1,
            ..entity
        };
        assert_eq!(
            draw_uniforms(&camera, &entity, &light, false, None).flags[0],
            !0
        );
    }

    #[test]
    fn palette_alpha_marks_the_entries_a_blending_packet_blends() {
        // Row 0: entry 2 is STP; the page's opaque texels (index 1)
        // outnumber its STP ones, so the row has a knock-out twin and only
        // entry 2 blends. Row 1: entry 1 is STP and outnumbers entry 2, so
        // there is no twin and every entry blends.
        let mut palettes = vec![[10, 20, 30, 255]; 2 * crate::model::PALETTE_ROW_LEN];
        palettes[1] = [40, 50, 60, 255];
        let mut stp = vec![false; 2 * crate::model::PALETTE_ROW_LEN];
        stp[2] = true;
        stp[crate::model::PALETTE_ROW_LEN + 1] = true;
        let texture = Texture8 {
            width: 4,
            height: 1,
            indices: vec![1, 1, 2, 0],
            palettes,
            stp,
        };
        assert!(texture.has_knockout_twin(0));
        assert!(!texture.has_knockout_twin(1));
        let (rows, rgba) = palette_rgba(&texture);
        assert_eq!(rows, 2);
        assert_eq!(rgba.len(), 2 * 256 * 4);
        assert_eq!(
            &rgba[..12],
            &[10, 20, 30, 0, 40, 50, 60, 0, 10, 20, 30, 255]
        );
        let row1 = 256 * 4;
        assert!(rgba[row1..].chunks(4).all(|entry| entry[3] == 255));
    }

    #[test]
    fn transparent_records_override_the_packet_blend() {
        assert_eq!(draw_mode(None, true), DRAW_CULL);
        assert_eq!(draw_mode(Some(0), false), DRAW_RECORD);
        assert_eq!(
            draw_mode(Some(0x33), true),
            DRAW_CULL | DRAW_RECORD | DRAW_RECORD_BLENDS
        );
        assert_eq!(pass_range(None, MeshPass::Opaque, 6, 9), (0, 9));
        assert_eq!(pass_range(None, MeshPass::Blend, 6, 9), (6, 3));
        assert_eq!(pass_range(None, MeshPass::Additive, 6, 9), (6, 3));
        assert_eq!(pass_range(Some(0x80), MeshPass::Blend, 6, 9), (0, 9));
        assert_eq!(pass_range(Some(0), MeshPass::Blend, 6, 9), (0, 0));
    }
}
