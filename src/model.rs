//! Shared player-model data types.
//!
//! These are plain parsed values: the parsers in [`crate::tmd`], [`crate::emd`]
//! and [`crate::tim`] produce them and the renderer consumes them. No file
//! format details leak into this module.

/// Number of palette entries in one row of a decoded texture; every decoder
/// normalizes its CLUT rows to this stride.
pub const PALETTE_ROW_LEN: usize = 256;

/// One triangle packet from a TMD primitive list.
///
/// Vertex and normal fields are indices into the owning [`TmdObject`]'s
/// `vertices` and `normals`. `uv[i]` is `[u, v]` for vertex `i`. The embedded
/// RDT models use six packet commands; a textured gouraud quad is split into
/// two of these triangles at parse time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmdPrim {
    pub vertices: [u16; 3],
    pub normals: [u16; 3],
    pub uv: [[u8; 2]; 3],
    pub clut: u16,
    pub tsb: u16,
    /// Whether the primitive samples the texture page. An untextured packet
    /// (`0x30000406`) paints [`TmdPrim::flat_color`] instead.
    pub textured: bool,
    /// The packet's semi-transparency command bit (`0x36000609`). The
    /// renderer's fallback is a flat half blend.
    pub blend: bool,
    /// How a blending packet's texels meet the background.
    pub mix: PacketMix,
    /// The packet stores its vertices with an unnegated Y (`0x25010607`). The
    /// render path skips the standard Y conjugation for these vertices; every
    /// other form negates Y like the PSX vertex reader does.
    pub raw_y: bool,
    /// The packet colour of an untextured packet, `None` when textured.
    pub flat_color: Option<[u8; 3]>,
    /// For a primitive split out of a textured gouraud quad, the packet's four
    /// corner vertex indices in ring order; `None` for a real triangle packet.
    /// The renderer drops the whole quad when any of the four is inside the
    /// near plane, exactly like the original's per-packet clip test.
    pub quad: Option<[u16; 4]>,
    /// Per corner, the reference's degenerate-UV nudge (`PSXObject_Store`):
    /// when two of the packet's first three corners share a u (bit 0) or a
    /// v (bit 1), corner 1's normalised coordinate gains `0.003`, or
    /// `0.0001` when bit 2 is set (the gouraud triangle's store).
    pub uv_nudge: [u8; 3],
}

/// How a blending packet's texels meet the background: Marni's packet blend
/// modes (`TransSort`'s `0xF00000` field).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PacketMix {
    /// By the record's blend weight, a half blend without one (`0x100000`).
    #[default]
    Weighted,
    /// `source + destination`, saturating (`0x200000`): an RE2 door part
    /// with attribute `0x4000`.
    Add,
    /// `source * source + destination * source` (`0x700000`, `SRCCOLOR` on
    /// both sides): an RE2 blending object (`Obj_model_set` attribute 2) in
    /// rooms 302 and 30A.
    SourceColour,
}

/// A TMD object: one vertex pool, one normal pool and the primitives using them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TmdObject {
    pub vertices: Vec<[i16; 3]>,
    pub normals: Vec<[i16; 3]>,
    pub prims: Vec<TmdPrim>,
}

impl TmdObject {
    /// `Texture_Move`: the part's textured primitives go to page `page` of
    /// `texture`, counted from 0 at its left, 128 texels wide, each with
    /// that page's palette row (row `page`, as an RE 2 enemy's texture has
    /// one row a page: a zombie's other clothes). A page past the texture's
    /// width (or past 15, the last a primitive can name), or one whose row
    /// the texture hasn't, moves nothing and is the error.
    pub fn texture_move(&mut self, page: u8, texture: &Texture8) -> Result<(), String> {
        let rows = texture.palettes.len() / PALETTE_ROW_LEN;
        if page > 0xF || (u32::from(page) + 1) * 128 > texture.width {
            return Err(format!(
                "page {page} is past the {} x {} texture's",
                texture.width, texture.height
            ));
        }
        if usize::from(page) >= rows {
            return Err(format!(
                "page {page}'s palette row isn't among the texture's {rows}"
            ));
        }
        // A palette row is the CLUT word's y, counted from 480.
        let row = (480 + u16::from(page)) << 6;
        for prim in self.prims.iter_mut().filter(|prim| prim.textured) {
            prim.tsb = (prim.tsb & !0xF) | u16::from(page);
            prim.clut = (prim.clut & !(0x01FF << 6)) | row;
        }
        Ok(())
    }
}

/// A parsed TMD mesh, usually one animated body part per object.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Tmd {
    pub objects: Vec<TmdObject>,
}

/// An 8bpp indexed texture with its palettes expanded to RGBA8.
///
/// `indices` is row-major, `width * height` bytes. `palettes` is row-major too:
/// `height`-independent CLUT rows of 256 entries each (`h` rows in file order).
/// `stp` mirrors `palettes` entry for entry: it carries the PSX CLUT word's
/// semi-transparency bit, which a textured packet blends on a per-texel basis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Texture8 {
    pub width: u32,
    pub height: u32,
    pub indices: Vec<u8>,
    pub palettes: Vec<[u8; 4]>,
    /// One STP flag per palette entry; empty when the decoder had no CLUT.
    pub stp: Vec<bool>,
}

impl Texture8 {
    /// Look up one palette entry; out-of-range lookups are black.
    pub fn palette(&self, row: usize, index: u8) -> [u8; 4] {
        self.palettes
            .get(row * PALETTE_ROW_LEN + index as usize)
            .copied()
            .unwrap_or([0, 0, 0, 0])
    }

    /// Whether one palette entry carries the STP bit; out-of-range is false.
    pub fn palette_stp(&self, row: usize, index: u8) -> bool {
        self.stp
            .get(row * PALETTE_ROW_LEN + index as usize)
            .copied()
            .unwrap_or(false)
    }

    /// Whether the reference builds an STP knock-out twin for this page under
    /// palette `row`: some texels are STP and the opaque (non-STP, non-zero)
    /// texels outnumber them.
    pub fn has_knockout_twin(&self, row: usize) -> bool {
        let (mut stp, mut opaque) = (0usize, 0usize);
        for &index in &self.indices {
            if self.palette_stp(row, index) {
                stp += 1;
            } else if index != 0 {
                opaque += 1;
            }
        }
        stp > 0 && opaque > stp
    }

    /// Whether any entry of palette `row` carries the STP bit.
    pub fn row_has_stp(&self, row: usize) -> bool {
        let start = row * PALETTE_ROW_LEN;
        let end = (start + PALETTE_ROW_LEN).min(self.stp.len());
        start < end && self.stp[start..end].iter().any(|&flag| flag)
    }
}

/// The joint hierarchy: relative joint positions and the child lists.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Skeleton {
    pub relative: Vec<[i16; 3]>,
    pub children: Vec<Vec<u8>>,
}

/// One animation keyframe: a root translation, the root-motion vertex and a
/// rotation per joint.
///
/// The on-disk frame header is six `i16`s before the rotations: the first three
/// are the root joint's translation (the original's `Joint_move` writes them
/// into `joint->transform.t[0..2]`), the next three are the root-motion vertex
/// (`entity_extract_anim_vertex` reads bytes +6/+8/+10). They are separate
/// values: an attack whose joint translation is `(0, -1031, 0)` can carry a
/// vertex of `(-1031, 0, 0)`, and using one for the other puts the entity a
/// thousand units off.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Keyframe {
    pub offset: [i16; 3],
    pub vertex: [i16; 3],
    pub rotations: Vec<[i16; 3]>,
}

/// One entry in an animation clip: which keyframe to show and for how long.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ClipFrame {
    pub keyframe: u16,
    pub timing: u16,
    /// An RE2 frame's word without its index (`pNow_seq`): its event marks.
    pub flags: u32,
}

/// An animation clip: an ordered list of keyframe/timing pairs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Clip {
    pub frames: Vec<ClipFrame>,
}

/// A complete EMD player/enemy model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Emd {
    pub skeleton: Skeleton,
    pub keyframes: Vec<Keyframe>,
    pub clips: Vec<Clip>,
    /// The EMD directory's first chunk: the second EDD clip bank the original
    /// loads into the player's damage scratch pair (`emdScratchPtr1/2`). On
    /// enemy models it carries the player's hit/bite/grab reaction clips; the
    /// player models ship no second bank and parse an empty list.
    pub damage_clips: Vec<Clip>,
    /// The keyframes the [`Emd::damage_clips`] frames index: the original's
    /// `emdScratchPtr1` points at the EMD file base, whose own EMR-style header
    /// describes a keyframe table that ends where the damage clip table
    /// begins. This is NOT the model's main [`Emd::keyframes`] table: the
    /// damage clips' frame indices only resolve against this one.
    pub damage_keyframes: Vec<Keyframe>,
    /// An RE2 EMD's further animation banks (directory chunks 3/4 and 5/6),
    /// in file order; RE1 models have none. Their keyframes pose
    /// [`Emd::skeleton`].
    pub banks: Vec<AnimBank>,
    pub mesh: Tmd,
    pub texture: Texture8,
    /// An RE2 EMD's vertex animations (`pSa_dat`), from its first chunk.
    pub morphs: Vec<PartMorph>,
}

impl Emd {
    /// The bytes the decoded model holds: its skeleton, animations, mesh,
    /// morphs and its texture's indices and palettes. A file's parts can
    /// share their vertices and primitives, each part decoding its own, so
    /// this can be far more than the file.
    pub fn decoded_bytes(&self) -> usize {
        use std::mem::size_of;
        const VECTOR: usize = size_of::<[i16; 3]>();
        let keyframes = |list: &[Keyframe]| -> usize {
            list.iter()
                .map(|keyframe| size_of::<Keyframe>() + keyframe.rotations.len() * VECTOR)
                .sum()
        };
        let clips = |list: &[Clip]| -> usize {
            list.iter()
                .map(|clip| size_of::<Clip>() + clip.frames.len() * size_of::<ClipFrame>())
                .sum()
        };
        let shapes = |list: &[Vec<[i16; 3]>]| -> usize {
            list.iter()
                .map(|shape| size_of::<Vec<[i16; 3]>>() + shape.len() * VECTOR)
                .sum()
        };
        let skeleton = self.skeleton.relative.len() * VECTOR
            + self
                .skeleton
                .children
                .iter()
                .map(|children| size_of::<Vec<u8>>() + children.len())
                .sum::<usize>();
        let mesh: usize = self
            .mesh
            .objects
            .iter()
            .map(|object| {
                size_of::<TmdObject>()
                    + (object.vertices.len() + object.normals.len()) * VECTOR
                    + object.prims.len() * size_of::<TmdPrim>()
            })
            .sum();
        let banks: usize = self
            .banks
            .iter()
            .map(|bank| keyframes(&bank.keyframes) + clips(&bank.clips))
            .sum();
        let morphs: usize = self
            .morphs
            .iter()
            .map(|morph| {
                size_of::<PartMorph>()
                    + shapes(&morph.targets)
                    + shapes(&morph.joint_targets)
                    + shapes(&morph.all_targets)
                    + (morph.joints.len() + morph.all_joints.len()) * size_of::<(u8, [i16; 3])>()
                    + morph.weights.len() * size_of::<i16>()
            })
            .sum();
        let texture = self.texture.indices.len()
            + self.texture.palettes.len() * size_of::<[u8; 4]>()
            + self.texture.stp.len();
        skeleton
            + mesh
            + banks
            + morphs
            + texture
            + keyframes(&self.keyframes)
            + keyframes(&self.damage_keyframes)
            + clips(&self.clips)
            + clips(&self.damage_clips)
    }
}

/// The shapes a part of the mesh can blend between (`Sa_parts_allmv`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PartMorph {
    pub part: u8,
    /// Each target's vertices, in the part's order.
    pub targets: Vec<Vec<[i16; 3]>>,
    /// The part's children and where they sit under it at rest: the first
    /// of the chunk's tables of joint positions.
    pub joints: Vec<(u8, [i16; 3])>,
    /// Where each target puts those children, from the table the high half
    /// of its weight's word names, in the order of [`PartMorph::joints`].
    pub joint_targets: Vec<Vec<[i16; 3]>>,
    /// `FUN_004e0070`'s version of [`PartMorph::joints`] and
    /// [`PartMorph::joint_targets`]: every part but the root, not the
    /// children of this part alone.
    pub all_joints: Vec<(u8, [i16; 3])>,
    pub all_targets: Vec<Vec<[i16; 3]>>,
    /// Each target's weight as the model's data holds it (the low half of
    /// the word `Sa_parts_allmv` reads), which a body that doesn't move it
    /// is drawn with.
    pub weights: Vec<i16>,
}

/// Win32 `MulDiv(value, factor, 0x1000)`: rounded to the nearest, halves away
/// from zero.
fn mul_div_4096(value: i32, factor: i32) -> i32 {
    let product = i64::from(value) * i64::from(factor);
    let half = if product < 0 { -2048 } else { 2048 };
    ((product + half) / 4096) as i32
}

impl PartMorph {
    /// The vertices of `base` blended through each target in turn by its
    /// weight, where 0x1000 is all of it (`LoadAverageShort12`).
    pub fn shape(&self, base: &[[i16; 3]], weights: &[i16]) -> Vec<[i16; 3]> {
        let mut shape = base.to_vec();
        for (target, &weight) in self.targets.iter().zip(weights) {
            if weight == 0 {
                continue;
            }
            for (vertex, to) in shape.iter_mut().zip(target) {
                *vertex = std::array::from_fn(|axis| {
                    let keep = i32::from(vertex[axis]) * (0xFFF - i32::from(weight));
                    ((keep + i32::from(to[axis]) * i32::from(weight)) >> 12) as i16
                });
            }
        }
        shape
    }

    /// `Sa_parts_allmv`'s children: each at its place at rest, blended
    /// through each target's place by its weight (`LoadAverage12`, where the
    /// rest keeps 0xfff).
    pub fn joint_offsets(&self, weights: &[i16]) -> Vec<(u8, [i16; 3])> {
        Self::blend(&self.joints, &self.joint_targets, weights)
    }

    /// `FUN_004e0070`: every part's place blended the same way.
    pub fn all_joint_offsets(&self, weights: &[i16]) -> Vec<(u8, [i16; 3])> {
        Self::blend(&self.all_joints, &self.all_targets, weights)
    }

    fn blend(
        rest: &[(u8, [i16; 3])],
        targets: &[Vec<[i16; 3]>],
        weights: &[i16],
    ) -> Vec<(u8, [i16; 3])> {
        let mut joints: Vec<(u8, [i32; 3])> = rest
            .iter()
            .map(|&(child, at)| (child, at.map(i32::from)))
            .collect();
        for (target, &weight) in targets.iter().zip(weights) {
            if weight == 0 {
                continue;
            }
            let weight = i32::from(weight);
            for ((_, at), to) in joints.iter_mut().zip(target) {
                // `LoadAverage12`: `MulDiv` of each term by 0x1000, which rounds
                // to the nearest.
                *at = std::array::from_fn(|axis| {
                    mul_div_4096(at[axis], 0xFFF - weight)
                        + mul_div_4096(i32::from(to[axis]), weight)
                });
            }
        }
        joints
            .into_iter()
            .map(|(child, at)| (child, at.map(|axis| axis as i16)))
            .collect()
    }
}

/// One animation bank: a keyframe table and the clips that index it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AnimBank {
    pub keyframes: Vec<Keyframe>,
    pub clips: Vec<Clip>,
}

/// The RDT-embedded player animation pair (pointer slots 9 and 10): an EMR
/// armature/keyframe header plus an EDD clip table. The clips are the
/// room-specific push, vault and ladder motions the original drives with
/// `Joint_move` through `jointMoveData2`/`jointMoveData3`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RoomAnim {
    pub skeleton: Skeleton,
    pub keyframes: Vec<Keyframe>,
    pub clips: Vec<Clip>,
}

/// An RE2 weapon PLW: the clips the character plays holding the weapon
/// (posing the PLD's skeleton), the hand-and-weapon mesh whose first part
/// stands in for the character's right hand, and the weapon's texels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plw {
    pub keyframes: Vec<Keyframe>,
    pub clips: Vec<Clip>,
    pub mesh: Tmd,
    pub texture: Texture8,
}

/// A no-weapon EMW animation + mesh file. The EMW carries its own armature and
/// keyframes; its locomotion clips drive the same 15-joint skeleton as the EMD.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Emw {
    pub skeleton: Skeleton,
    pub keyframes: Vec<Keyframe>,
    pub clips: Vec<Clip>,
    pub mesh: Tmd,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prim(tsb: u16, clut: u16, textured: bool) -> TmdPrim {
        TmdPrim {
            vertices: [0; 3],
            normals: [0; 3],
            uv: [[0; 2]; 3],
            clut,
            tsb,
            textured,
            blend: false,
            mix: PacketMix::Weighted,
            raw_y: false,
            flat_color: None,
            quad: None,
            uv_nudge: [0; 3],
        }
    }

    /// A zombie's part on page 1 (palette row 0x1E1) moved to page 3 takes
    /// row 0x1E3, and one on page 0 moved to page 2 takes row 0x1E2, as
    /// `Em10_Init` passes `Texture_Move` the page and its row; an
    /// untextured primitive keeps its words.
    /// A texture `pages` pages wide with a palette row a page.
    fn texture(pages: u32) -> Texture8 {
        Texture8 {
            width: pages * 128,
            height: 256,
            indices: Vec::new(),
            palettes: vec![[0; 4]; pages as usize * PALETTE_ROW_LEN],
            stp: Vec::new(),
        }
    }

    /// A page the texture hasn't, or whose palette row it hasn't, moves
    /// nothing: a two-page replacement for a four-page zombie, a page past
    /// 15 (which a primitive's four bits would wrap to 0), and a wide
    /// texture short of rows.
    #[test]
    fn texture_move_refuses_a_page_the_texture_hasnt() {
        let before = vec![prim(0x81, 0x7840, true)];
        let mut object = TmdObject {
            prims: before.clone(),
            ..TmdObject::default()
        };
        assert!(
            object
                .texture_move(3, &texture(2))
                .unwrap_err()
                .contains("page 3")
        );
        assert!(object.texture_move(16, &texture(32)).is_err());
        let short = Texture8 {
            palettes: vec![[0; 4]; 2 * PALETTE_ROW_LEN],
            ..texture(4)
        };
        assert!(
            object
                .texture_move(3, &short)
                .unwrap_err()
                .contains("palette row")
        );
        assert_eq!(object.prims, before);
        assert!(object.texture_move(1, &texture(2)).is_ok());
    }

    /// The row is the destination page's, whatever row the primitive had: a
    /// mod's primitive on page 1 with row 0 goes to page 2 with row 2, and
    /// page 3 of a texture with three rows is refused.
    #[test]
    fn texture_move_writes_the_destination_pages_row() {
        let mut object = TmdObject {
            prims: vec![prim(0x81, 0x7800 | 0x5, true)],
            ..TmdObject::default()
        };
        let three_rows = Texture8 {
            palettes: vec![[0; 4]; 3 * PALETTE_ROW_LEN],
            ..texture(4)
        };
        assert!(object.texture_move(3, &three_rows).is_err());
        object.texture_move(2, &three_rows).unwrap();
        assert_eq!(
            (object.prims[0].tsb, object.prims[0].clut),
            (0x82, 0x7880 | 0x5)
        );
    }

    #[test]
    fn texture_move_takes_the_page_and_its_palette_row() {
        let mut object = TmdObject {
            prims: vec![
                prim(0x81, 0x7840, true),
                prim(0x80, 0x7800, true),
                prim(0x81, 0x7840, false),
            ],
            ..TmdObject::default()
        };
        object.texture_move(3, &texture(4)).unwrap();
        assert_eq!((object.prims[0].tsb, object.prims[0].clut), (0x83, 0x78C0));
        assert_eq!((object.prims[1].tsb, object.prims[1].clut), (0x83, 0x78C0));
        assert_eq!((object.prims[2].tsb, object.prims[2].clut), (0x81, 0x7840));
        object.texture_move(2, &texture(4)).unwrap();
        assert_eq!((object.prims[0].tsb, object.prims[0].clut), (0x82, 0x7880));
    }
}
