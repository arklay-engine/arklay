//! RE 2's FILE binders: the three 3D binders FILE's carousel draws
//! (`0x500470`), from `common\\data\\st_file.ts`.
//!
//! The file is a TIM (one 128 x 144 page with a CLUT row per binder) and
//! then two MD1 meshes, the binder and the file label a binder holding a
//! file shows. Sub 0 of FILE (`0x4F95B0`) copies the binder's 0x64C bytes
//! and the label's 0x8C out of it. Each binder is a coordinate with the
//! binder's own turn, `(0, 0xC00, 0)`, on the identity; `0x500560` places
//! it on the carousel each tick and `0x436A90` projects its polygons with
//! the GTE and sorts them into the ordering table. The polygons are unlit:
//! every corner's colour is `0x808080`, the texel as stored, through the
//! binder's own CLUT row (the work's draw flag, the binder's index).
//!
//! Each binder carries a label for each of its slots holding a file, the
//! second mesh placed in the binder's frame by its slot. A binder's opening
//! (FILE's subs 6 to 9) zooms the eye in and pulls the binders together,
//! and a file's opening and closing (0xB and 0xC) drop them with the panel.

use anyhow::{Context, Result, bail};

use crate::ivm::{Ivm, IvmObject, IvmPrim, IvmPrimKind};
use crate::player2::{Matrix, rot_matrix};
use crate::render::ScreenPolygon;
use crate::tim;

/// The pack entry the converter copies `st_file.ts` to.
pub const ENTRY: &str = "ui/status/st_file.ts";

/// The MD1 object table's start: the header's three words.
const MD1_OBJECTS: usize = 12;
/// An MD1 object descriptor: seven words.
const MD1_OBJECT_LEN: usize = 28;
/// A vertex: `x, y, z` and a pad.
const VECTOR_LEN: usize = 8;
/// A triangle's index or texture record, and a quad's.
const TRIANGLE_LEN: usize = 12;
const QUAD_LEN: usize = 16;

/// The eye's height at rest (`0x53F34C`, `0x226` less the zoom) and its
/// distance (`0x53F350`).
pub const EYE_Y: i32 = 0x226;
const EYE_Z: i32 = 1600;

/// `SquareRoot0`'s table (`0x5269F8`, from entry 0x40: the argument scaled
/// to its top bits is 0x40 to 0xFF).
const SQRT_TABLE: [u16; 192] = [
    4096, 4127, 4159, 4190, 4222, 4252, 4283, 4314, 4344, 4374, 4404, 4434, 4463, 4492, 4521, 4550,
    4579, 4608, 4636, 4664, 4692, 4720, 4748, 4775, 4802, 4830, 4857, 4884, 4910, 4937, 4964, 4990,
    5016, 5042, 5068, 5094, 5120, 5145, 5170, 5196, 5221, 5246, 5271, 5296, 5320, 5345, 5369, 5394,
    5418, 5442, 5466, 5490, 5514, 5538, 5561, 5585, 5608, 5632, 5655, 5678, 5701, 5724, 5747, 5769,
    5792, 5815, 5837, 5860, 5882, 5904, 5926, 5948, 5970, 5992, 6014, 6036, 6058, 6079, 6101, 6122,
    6144, 6165, 6186, 6207, 6228, 6249, 6270, 6291, 6312, 6333, 6353, 6374, 6394, 6415, 6435, 6456,
    6476, 6496, 6516, 6536, 6556, 6576, 6596, 6616, 6636, 6656, 6675, 6695, 6714, 6734, 6753, 6773,
    6792, 6811, 6830, 6850, 6869, 6888, 6907, 6926, 6945, 6963, 6982, 7001, 7020, 7038, 7057, 7075,
    7094, 7112, 7131, 7149, 7168, 7186, 7204, 7222, 7240, 7258, 7276, 7294, 7312, 7330, 7348, 7366,
    7384, 7401, 7419, 7437, 7454, 7472, 7489, 7507, 7524, 7542, 7559, 7576, 7594, 7611, 7628, 7645,
    7662, 7680, 7697, 7714, 7731, 7747, 7764, 7781, 7798, 7815, 7832, 7848, 7865, 7882, 7898, 7915,
    7931, 7948, 7964, 7981, 7997, 8014, 8030, 8046, 8062, 8079, 8095, 8111, 8127, 8143, 8159, 8175,
];

/// `SquareRoot0` (`0x451780`): the table's root of `value` scaled to its
/// top six or seven bits, shifted back by half the scale.
fn square_root(value: i32) -> i32 {
    if value == 0 || value == -1 {
        return 0;
    }
    let lead = 31 - value.unsigned_abs().leading_zeros() as i32;
    let scale = (31 - lead) & !1;
    let index = if scale < 24 {
        value >> (24 - scale)
    } else {
        value << (scale - 24)
    };
    let root = u32::from(SQRT_TABLE[(index as usize).clamp(0x40, 0xFF) - 0x40]);
    ((root << ((31 - scale) >> 1)) >> 12) as i32
}

/// `Set_view` (`0x502DB0`): the view from the eye at `(0, eye_y, 1600)` to
/// the origin, a pitch and then a yaw onto the identity (`MulMatrix`,
/// `0x450DD0`, reads both operands before writing), each term divided by
/// the table's root and truncated, and the eye's place through it.
pub(crate) fn view(eye_y: i32) -> Matrix {
    let (dx, dy, dz) = (0, -eye_y, -EYE_Z);
    let identity = [[0x1000, 0, 0], [0, 0x1000, 0], [0, 0, 0x1000]];
    let length = square_root(dy * dy + dx * dx + dz * dz);
    let flat = square_root(dx * dx + dz * dz);
    let mut pitch = identity;
    pitch[1][1] = ((flat << 12) / length) as i16;
    pitch[2][1] = ((dy * 0x1000) / length) as i16;
    pitch[1][2] = pitch[2][1].wrapping_neg();
    pitch[2][2] = pitch[1][1];
    let mut m = crate::player2::mul_matrix(&identity, &pitch);
    if flat != 0 {
        let mut yaw = identity;
        yaw[2][0] = ((dx * 0x1000) / flat) as i16;
        yaw[0][0] = ((dz * 0x1000) / flat) as i16;
        yaw[0][2] = yaw[2][0].wrapping_neg();
        yaw[2][2] = yaw[0][0];
        m = crate::player2::mul_matrix(&m, &yaw);
    }
    let eye = [-dx, -dy, -dz];
    let t = std::array::from_fn(|row| {
        let sum: i32 = (0..3).map(|k| i32::from(m[row][k]) * -eye[k]).sum();
        sum >> 12
    });
    Matrix { m, t }
}
/// The binder's own turn (`+0x58` of its work, set by sub 0).
const BINDER_TURN: [i16; 3] = [0, 0xC00, 0];
/// The carousel's radius, and the turn between two binders (`0x555`).
const RADIUS: i16 = 0x1AE;
const SPACING: i32 = 0x555;
/// The turn of the binder in front at rest (`0x691F90`, set to `0x400`),
/// and its change per tick of a turn: `16 * n` on the `n`th.
pub const REST: i32 = 0x400;
const TURN_STEP: i32 = 16;
/// `0x500560` pulls each binder's x toward 0 by this plus twice the zoom,
/// through sub 8, and puts it at 0 from sub 9.
const PULL: i32 = 4;
/// `0x500560`'s drop in a file's opening (sub 0xB) and its rise (0xC).
const DROP: i32 = 0x26;
/// A label's place in its binder's frame (`0x500560`'s type 1): `x` less
/// the slot, `y` down the spine by the slot, `z` at the front.
const LABEL_X: i32 = -0x17;
const LABEL_Y: i32 = -0xDE;
const LABEL_STEP: i32 = 0x3E;
const LABEL_Z: i32 = 0xE9;
/// A label's CLUT rows: the selected slot of the binder in front, and the
/// others (`Texture_Move_Marni`).
const LABEL_SELECTED: usize = 0;
const LABEL_OTHER: usize = 1;
/// `SetGeomScreen`'s distance as the status screen leaves it (`0x526268`).
const SCREEN: i64 = 208;
/// The projection's centre: `0x436A70`'s x, and the panel's top plus 70.
const CENTRE_X: i32 = 0x72;
const CENTRE_DOWN: i32 = 0x46;
/// The GTE's own centre (`0x451190`, mode 2), which `0x436A90` takes back
/// off.
const GTE_X: f64 = 160.0;
const GTE_Y: f64 = 120.0;
/// The ordering table's last slot (`0x436A90` clamps to `0x1FFF`).
const OT_LAST: i32 = 0x1FFF;

/// Read `st_file.ts` into an item-view model: the page, and the binder as
/// its one object, each primitive with its MD1 corners in order.
pub fn parse(data: &[u8]) -> Result<Ivm> {
    let (page, meshes) = crate::ivm::split(data).context("st_file.ts")?;
    let texture = tim::decode_8bpp(page).context("st_file.ts's page")?;
    let binder = parse_mesh(meshes).context("st_file.ts's binder")?;
    let label = meshes
        .get(BINDER_LEN..)
        .context("st_file.ts has no label")
        .and_then(|label| parse_mesh(label).context("st_file.ts's label"))?;
    Ok(Ivm {
        texture,
        objects: vec![binder, label],
    })
}

/// The binder mesh's bytes, which sub 0 copies out; the label follows.
const BINDER_LEN: usize = 0x64C;

fn word(data: &[u8], at: usize) -> Result<u32> {
    let bytes = data
        .get(at..at + 4)
        .with_context(|| format!("MD1 is truncated at {at:#x}"))?;
    Ok(u32::from_le_bytes(bytes.try_into().expect("four bytes")))
}

fn half(data: &[u8], at: usize) -> Result<u16> {
    let bytes = data
        .get(at..at + 2)
        .with_context(|| format!("MD1 is truncated at {at:#x}"))?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

/// An MD1 mesh of one part: a triangle object and a quad object sharing the
/// first one's vertices.
fn parse_mesh(data: &[u8]) -> Result<IvmObject> {
    if word(data, 8)? != 2 {
        bail!("the mesh is not one MD1 part");
    }
    let field = |object: usize, slot: usize| -> Result<usize> {
        Ok(word(data, MD1_OBJECTS + object * MD1_OBJECT_LEN + slot * 4)? as usize)
    };
    let (vertices_at, vertex_count) = (field(0, 0)?, field(0, 1)?);
    crate::budget::check_len(vertex_count, crate::budget::MAX_RECORDS, "MD1 vertices")?;
    let vertices = (0..vertex_count)
        .map(|index| {
            let at = MD1_OBJECTS + vertices_at + index * VECTOR_LEN;
            Ok([
                half(data, at)? as i16,
                half(data, at + 2)? as i16,
                half(data, at + 4)? as i16,
            ])
        })
        .collect::<Result<Vec<_>>>()?;
    let mut prims = Vec::new();
    for (object, corners, len) in [(0, 3, TRIANGLE_LEN), (1, 4, QUAD_LEN)] {
        let (at, count, textures) = (field(object, 4)?, field(object, 5)?, field(object, 6)?);
        crate::budget::check_len(count, crate::budget::MAX_RECORDS, "MD1 primitives")?;
        for index in 0..count {
            let record = MD1_OBJECTS + at + index * len;
            let texture = MD1_OBJECTS + textures + index * len;
            let mut prim = IvmPrim {
                kind: if corners == 4 {
                    IvmPrimKind::TexturedGouraudQuad
                } else {
                    IvmPrimKind::TexturedGouraud
                },
                vertex_count: corners as u8,
                vertices: [0; 4],
                normals: [0; 4],
                uv: [[0; 2]; 4],
                colors: [[0x80; 3]; 3],
                clut: half(data, texture + 2)?,
                tsb: half(data, texture + 6)?,
                blend: false,
            };
            for corner in 0..corners {
                // Each corner is a normal index, then a vertex index.
                let vertex = half(data, record + corner * 4 + 2)?;
                if usize::from(vertex) >= vertices.len() {
                    bail!(
                        "primitive {index} uses vertex {vertex} of {}",
                        vertices.len()
                    );
                }
                prim.vertices[corner] = vertex;
                let uv = data
                    .get(texture + corner * 4..texture + corner * 4 + 2)
                    .context("MD1 texture record is truncated")?;
                prim.uv[corner] = [uv[0], uv[1]];
            }
            prims.push(prim);
        }
    }
    Ok(IvmObject {
        vertices,
        normals: Vec::new(),
        prims,
    })
}

/// The carousel's turn (`0x691F90`) on FILE's sub `sub`, `command` ticks
/// into it: it falls by `16 * n` on the `n`th tick of a turn left (sub 4)
/// and rises as much turning right (sub 5); the turn's end puts the next
/// binder in front and the turn back to [`REST`].
pub fn turn(sub: u8, command: u8) -> i32 {
    let n = i32::from(command);
    let swept = TURN_STEP * n * (n + 1) / 2;
    match sub {
        super::FILE_LEFT => REST - swept,
        super::FILE_RIGHT => REST + swept,
        _ => REST,
    }
}

/// What the binders' draw (`0x500470`) reads: the binder in front
/// (`0x691F71`) and its slot, FILE's sub and its count, the zoom
/// (`0x691F8E`), the zoom the eye was last moved by, the binders' spin, the
/// files held and the FILE panel's top (`0x691FAE`), which the projection
/// follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BinderView {
    pub front: u8,
    pub slot: i8,
    pub sub: u8,
    pub command: u8,
    pub zoom: i16,
    pub view_zoom: i16,
    pub spin: i16,
    pub files: [u8; crate::scd2::host::FILE_SLOTS],
    pub top: i32,
}

/// Where binder `binder` sits on the carousel (`0x500560`): `binder - front`
/// turns of [`SPACING`] past `turn`, at [`RADIUS`]. The original applies
/// the turn to `(RADIUS, 0, 0)` with `ApplyMatrix`, whose three long
/// results it reads back as shorts: x is the first's low half, y its high
/// half (the sign) and z the second's low half. So the binders go round
/// the vertical axis, though the turn is about z. A file's opening (sub
/// 0xB) drops each by [`DROP`] and its closing (0xC) raises it as much.
/// Through sub 8 x moves `2 * zoom + 4` toward 0, not past it; from sub 9
/// it is 0, the binders stacked.
fn place(binder: u8, front: u8, turn: i32, sub: u8, zoom: i16) -> [i32; 3] {
    let angle = (turn + SPACING * (i32::from(front) - i32::from(binder))) as i16;
    let m = rot_matrix([0, 0, angle]);
    let reach = i32::from(RADIUS);
    let across = (i32::from(m[0][0]) * reach) >> 12;
    let along = (i32::from(m[1][0]) * reach) >> 12;
    let mut y = across >> 16;
    match sub {
        super::FILE_READ_IN => y += DROP,
        super::FILE_READ_OUT => y -= DROP,
        _ => {}
    }
    let mut x = i32::from(across as i16);
    if sub < super::FILE_SPIN_BACK {
        let pull = i32::from(zoom) * 2 + PULL;
        if x >= 0 {
            x = (x - pull).max(0);
        }
        if x <= 0 {
            x = (x + pull).min(0);
        }
    } else {
        x = 0;
    }
    [x, y, i32::from(along as i16)]
}

/// A label's frame in its binder's (`0x500560`'s type 1, composed with the
/// binder's matrix by `CompM`): unturned, at its slot's place.
fn label_place(slot: u8) -> Matrix {
    let slot = i32::from(slot);
    Matrix {
        m: rot_matrix([0, 0, 0]),
        t: [LABEL_X - slot, LABEL_Y + slot * LABEL_STEP, LABEL_Z],
    }
}

/// The binders' polygons on the screen, and their labels', in the order the
/// original draws them (`model` is [`parse`]'s: the binder, then the label).
///
/// `0x500470` builds the view from the eye the last draw left
/// ([`BinderView::view_zoom`]) and draws binder 2, then its labels from slot
/// 7 to 0, then binder 1 and binder 0 the same way. Each polygon's corners go
/// through `RotTransPers` (`0x451190`): the view times the binder's world
/// matrix, each row's sum floored, a zero depth taken as 1, then
/// `x * 208 / z + 160` truncated, moved to the projection's centre. Its
/// ordering-table slot is the mean of its four depths (a triangle's last
/// counts twice) over 4; slots draw far to near, and within a slot the
/// last polygon added draws first (`0x416500` puts it at the list's head).
/// A polygon whose mean depth is within the screen distance isn't added.
pub fn polygons(model: &Ivm, at: &BinderView) -> Vec<ScreenPolygon> {
    let mut sorted: Vec<(i32, usize, ScreenPolygon)> = Vec::new();
    let Some(binder_mesh) = model.objects.first() else {
        return Vec::new();
    };
    let label_mesh = model.objects.get(1);
    let turn = turn(at.sub, at.command);
    let view = view(EYE_Y - i32::from(at.view_zoom));
    let own = Matrix {
        m: rot_matrix([
            BINDER_TURN[0],
            BINDER_TURN[1].wrapping_add(at.spin),
            BINDER_TURN[2],
        ]),
        t: [0; 3],
    };
    for binder in (0..super::BINDERS).rev() {
        let world = Matrix {
            t: place(binder, at.front, turn, at.sub, at.zoom),
            ..own
        };
        project(
            binder_mesh,
            &view.comp(&world),
            usize::from(binder),
            at.top,
            &mut sorted,
        );
        let Some(label_mesh) = label_mesh else {
            continue;
        };
        for slot in (0..8u8).rev() {
            let held = at.files[usize::from(binder) * 8 + usize::from(slot)];
            if held == crate::scd2::host::NO_FILE {
                continue;
            }
            let row = if binder == at.front && i32::from(slot) == i32::from(at.slot) {
                LABEL_SELECTED
            } else {
                LABEL_OTHER
            };
            let label = world.comp(&label_place(slot));
            project(label_mesh, &view.comp(&label), row, at.top, &mut sorted);
        }
    }
    sorted.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    sorted.into_iter().map(|(_, _, polygon)| polygon).collect()
}

/// One mesh's polygons through `screen` into the ordering table `sorted`,
/// in CLUT row `row`.
fn project(
    model: &IvmObject,
    screen: &Matrix,
    row: usize,
    top: i32,
    sorted: &mut Vec<(i32, usize, ScreenPolygon)>,
) {
    for prim in &model.prims {
        let count = usize::from(prim.vertex_count);
        let mut corners = [[0i32; 2]; 4];
        let mut depths = [0i64; 4];
        for corner in 0..count {
            let vertex = model.vertices[usize::from(prim.vertices[corner])];
            let [x, y, z] = std::array::from_fn(|row| {
                let sum: i32 = (0..3)
                    .map(|k| i32::from(screen.m[row][k]) * i32::from(vertex[k]))
                    .sum();
                i64::from((sum >> 12) + screen.t[row])
            });
            let z = if z == 0 { 1 } else { z };
            let project =
                |value: i64, centre: f64| (value as f64 * SCREEN as f64 / z as f64 + centre) as i32;
            corners[corner] = [
                project(x, GTE_X) + CENTRE_X - GTE_X as i32,
                project(y, GTE_Y) + top + CENTRE_DOWN - GTE_Y as i32,
            ];
            depths[corner] = z;
        }
        if count == 3 {
            depths[3] = depths[2];
        }
        let mean = (depths.iter().sum::<i64>() / 4) as i32;
        if i64::from(mean) <= SCREEN {
            continue;
        }
        let polygon = ScreenPolygon {
            corners,
            uv: prim.uv,
            count,
            palette_row: row,
        };
        sorted.push((mean.min(OT_LAST) >> 2, sorted.len(), polygon));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The binders' world translations (`+0x4C` of each work) the original
    /// had on recorded ticks of `status_file`: at rest, then the first turn
    /// left 1, 6 and 12 ticks in, then at rest with binder 2 in front.
    #[test]
    fn the_binders_sit_where_the_original_put_them() {
        let cases: [(u8, i32, [[i32; 3]; 3]); 5] = [
            (0, REST, [[0, 0, 430], [368, 0, -215], [-369, -1, -216]]),
            (
                0,
                turn(super::super::FILE_LEFT, 1),
                [[6, 0, 429], [363, 0, -224], [-374, -1, -207]],
            ),
            (
                0,
                turn(super::super::FILE_LEFT, 6),
                [[207, 0, 374], [214, 0, -371], [-426, -1, -4]],
            ),
            (
                0,
                turn(super::super::FILE_LEFT, 12),
                [[400, 0, -145], [-324, -1, -279], [-74, -1, 422]],
            ),
            (2, REST, [[368, 0, -216], [-369, -1, -215], [0, 0, 430]]),
        ];
        for (front, turn, places) in cases {
            for (binder, expected) in places.iter().enumerate() {
                assert_eq!(
                    place(binder as u8, front, turn, super::super::FILE_SHOWN, 0),
                    *expected,
                    "binder {binder}, turn {turn}"
                );
            }
        }
    }

    /// The GTE matrix the last binder drawn (binder 0) left at `0x99CE80`.
    #[test]
    fn the_screen_matrix_is_the_originals() {
        let own = Matrix {
            m: rot_matrix(BINDER_TURN),
            t: [0; 3],
        };
        let screen = |t| view(EYE_Y).comp(&Matrix { t, ..own });
        let rest = |front, turn| place(0, front, turn, super::super::FILE_SHOWN, 0);
        let at_rest = screen(rest(0, REST));
        assert_eq!(
            at_rest.m,
            [[0, 0, 4101], [-1336, 3877, 0], [-3882, -1334, 0]]
        );
        assert_eq!(at_rest.t, [0, -140, 1287]);
        assert_eq!(
            screen(rest(0, turn(super::super::FILE_LEFT, 6))).t,
            [-208, -121, 1340]
        );
        assert_eq!(screen(rest(2, REST)).t, [-369, 71, 1899]);
    }

    /// `Set_view` from the eye at rest gives the view the original leaves
    /// at `0x98F220`, its table root's -4101 and uneven pitch included.
    #[test]
    fn the_view_is_set_views() {
        let at_rest = view(EYE_Y);
        assert_eq!(
            at_rest.m,
            [[-4101, 0, 0], [0, 3877, -1336], [0, -1334, -3882]]
        );
        assert_eq!(at_rest.t, [0, 1, 1695]);
        assert_eq!(square_root(1600 * 1600), 1598);
        assert_eq!(square_root(0), 0);
    }

    #[test]
    fn an_opening_pulls_the_binders_in_and_stacks_them() {
        use super::super::{FILE_LIST, FILE_OPEN, FILE_READ_IN, FILE_READ_OUT};
        // At rest binder 1 is at x 368 after the pull of 4; at zoom 24 in sub
        // 6 the pull is 52.
        assert_eq!(place(1, 0, REST, FILE_OPEN, 24)[0], 368 + 4 - 52);
        for binder in 0..3 {
            assert_eq!(place(binder, 0, REST, FILE_LIST, 484)[0], 0);
            assert_eq!(
                place(binder, 0, REST, FILE_READ_IN, 484)[1],
                DROP + place(binder, 0, REST, FILE_LIST, 484)[1]
            );
            assert_eq!(
                place(binder, 0, REST, FILE_READ_OUT, 0)[1],
                place(binder, 0, REST, FILE_LIST, 0)[1] - DROP
            );
        }
    }

    #[test]
    fn a_label_sits_by_its_slot() {
        let slot_1 = label_place(1);
        assert_eq!(slot_1.t, [-0x18, -0xDE + 0x3E, 0xE9]);
        assert_eq!(slot_1.m, rot_matrix([0, 0, 0]));
    }

    #[test]
    fn a_turn_sweeps_from_rest() {
        assert_eq!(turn(super::super::FILE_SHOWN, 0), REST);
        assert_eq!(turn(super::super::FILE_LEFT, 1), 1008);
        assert_eq!(turn(super::super::FILE_LEFT, 6), 688);
        assert_eq!(turn(super::super::FILE_LEFT, 12), -224);
        assert_eq!(turn(super::super::FILE_RIGHT, 12), 2272);
    }

    #[test]
    fn a_mesh_reads_its_corners_and_texture_coordinates() {
        // One part: a triangle object and a quad object over four vertices.
        let mut data = vec![0u8; 12 + 2 * MD1_OBJECT_LEN];
        data[8] = 2;
        let vertices = data.len() - MD1_OBJECTS;
        for vertex in [[1i16, 2, 3], [4, 5, 6], [7, 8, 9], [10, 11, 12]] {
            for axis in vertex {
                data.extend(axis.to_le_bytes());
            }
            data.extend([0, 0]);
        }
        let triangles = data.len() - MD1_OBJECTS;
        data.extend([0, 0, 2, 0, 0, 0, 1, 0, 0, 0, 0, 0]);
        let triangle_uv = data.len() - MD1_OBJECTS;
        data.extend([10, 11, 0, 0x78, 12, 13, 0x80, 0, 14, 15, 0, 0]);
        let quads = data.len() - MD1_OBJECTS;
        data.extend([0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 3, 0]);
        let quad_uv = data.len() - MD1_OBJECTS;
        data.extend([1, 2, 0, 0, 3, 4, 0, 0, 5, 6, 0, 0, 7, 8, 0, 0]);
        let set = |data: &mut Vec<u8>, object: usize, slot: usize, value: usize| {
            let at = MD1_OBJECTS + object * MD1_OBJECT_LEN + slot * 4;
            data[at..at + 4].copy_from_slice(&(value as u32).to_le_bytes());
        };
        for object in 0..2 {
            set(&mut data, object, 0, vertices);
            set(&mut data, object, 1, 4);
        }
        for (object, (at, uv)) in [(triangles, triangle_uv), (quads, quad_uv)]
            .into_iter()
            .enumerate()
        {
            set(&mut data, object, 4, at);
            set(&mut data, object, 5, 1);
            set(&mut data, object, 6, uv);
        }
        let mesh = parse_mesh(&data).unwrap();
        assert_eq!(
            mesh.vertices,
            vec![[1, 2, 3], [4, 5, 6], [7, 8, 9], [10, 11, 12]]
        );
        assert_eq!(mesh.prims.len(), 2);
        assert_eq!(mesh.prims[0].vertex_count, 3);
        assert_eq!(&mesh.prims[0].vertices[..3], &[2, 1, 0]);
        assert_eq!(&mesh.prims[0].uv[..3], &[[10, 11], [12, 13], [14, 15]]);
        assert_eq!(mesh.prims[1].vertices, [0, 1, 2, 3]);
        assert_eq!(mesh.prims[1].uv, [[1, 2], [3, 4], [5, 6], [7, 8]]);
    }
}
