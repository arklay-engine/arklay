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
//! The label and the binder opening (FILE's subs 6 to 18) aren't drawn: no
//! route binder holds a file (`docs/internals/deviations/re2.md`).

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

/// The view `0x502DB0` (`GsSetRefView2`) builds from `(0, 550, 1600)` to
/// the origin, as the original leaves it at `0x98F220`. Its square root
/// (`0x451780`) is table based and its `MulMatrix` (`0x450DD0`) writes into
/// its own operand, which gives the -4101 and the uneven pitch terms; the
/// port takes the recorded rows rather than rebuild those. The eye's height
/// is `550 - 0x691F8E`, which only a binder's opening moves.
const VIEW: Matrix = Matrix {
    m: [[-4101, 0, 0], [0, 3877, -1336], [0, -1334, -3882]],
    t: [0, 1, 1695],
};
/// The binder's own turn (`+0x58` of its work, set by sub 0).
const BINDER_TURN: [i16; 3] = [0, 0xC00, 0];
/// The carousel's radius, and the turn between two binders (`0x555`).
const RADIUS: i16 = 0x1AE;
const SPACING: i32 = 0x555;
/// The turn of the binder in front at rest (`0x691F90`, set to `0x400`),
/// and its change per tick of a turn: `16 * n` on the `n`th.
pub const REST: i32 = 0x400;
const TURN_STEP: i32 = 16;
/// `0x500560` pulls each binder's x this much toward 0, through sub 8.
const PULL: i32 = 4;
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
    Ok(Ivm {
        texture,
        objects: vec![binder],
    })
}

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
        bail!("the binder is not one MD1 part");
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

/// Where binder `binder` sits on the carousel (`0x500560`): `binder - front`
/// turns of [`SPACING`] past `turn`, at [`RADIUS`]. The original applies
/// the turn to `(RADIUS, 0, 0)` with `ApplyMatrix`, whose three long
/// results it reads back as shorts: x is the first's low half, y its high
/// half (the sign) and z the second's low half. So the binders go round
/// the vertical axis, though the turn is about z. Then x moves [`PULL`]
/// toward 0, not past it.
fn place(binder: u8, front: u8, turn: i32) -> [i32; 3] {
    let angle = (turn + SPACING * (i32::from(front) - i32::from(binder))) as i16;
    let m = rot_matrix([0, 0, angle]);
    let reach = i32::from(RADIUS);
    let across = (i32::from(m[0][0]) * reach) >> 12;
    let along = (i32::from(m[1][0]) * reach) >> 12;
    let mut x = i32::from(across as i16);
    if x >= 0 {
        x = (x - PULL).max(0);
    }
    if x <= 0 {
        x = (x + PULL).min(0);
    }
    [x, across >> 16, i32::from(along as i16)]
}

/// The binders' polygons on the screen, in the order the original draws
/// them: `front` is the binder in front (`0x691F71`), `turn` the
/// carousel's ([`turn`]) and `top` the FILE panel's top (`0x691FAE`).
///
/// `0x500470` draws binder 2, then 1, then 0. Each polygon's corners go
/// through `RotTransPers` (`0x451190`): the view times the binder's world
/// matrix, each row's sum floored, a zero depth taken as 1, then
/// `x * 208 / z + 160` truncated, moved to the projection's centre. Its
/// ordering-table slot is the mean of its four depths (a triangle's last
/// counts twice) over 4; slots draw far to near, and within a slot the
/// last polygon added draws first (`0x416500` puts it at the list's head).
/// A polygon whose mean depth is within the screen distance isn't added.
pub fn polygons(model: &IvmObject, front: u8, turn: i32, top: i32) -> Vec<ScreenPolygon> {
    let mut sorted: Vec<(i32, usize, ScreenPolygon)> = Vec::new();
    let own = Matrix {
        m: rot_matrix(BINDER_TURN),
        t: [0; 3],
    };
    for binder in (0..super::BINDERS).rev() {
        let world = Matrix {
            t: place(binder, front, turn),
            ..own
        };
        let screen = VIEW.comp(&world);
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
                let project = |value: i64, centre: f64| {
                    (value as f64 * SCREEN as f64 / z as f64 + centre) as i32
                };
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
                palette_row: usize::from(binder),
            };
            sorted.push((mean.min(OT_LAST) >> 2, sorted.len(), polygon));
        }
    }
    sorted.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    sorted.into_iter().map(|(_, _, polygon)| polygon).collect()
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
                    place(binder as u8, front, turn),
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
        let screen = |t| VIEW.comp(&Matrix { t, ..own });
        let at_rest = screen(place(0, 0, REST));
        assert_eq!(
            at_rest.m,
            [[0, 0, 4101], [-1336, 3877, 0], [-3882, -1334, 0]]
        );
        assert_eq!(at_rest.t, [0, -140, 1287]);
        assert_eq!(
            screen(place(0, 0, turn(super::super::FILE_LEFT, 6))).t,
            [-208, -121, 1340]
        );
        assert_eq!(screen(place(0, 2, REST)).t, [-369, 71, 1899]);
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
