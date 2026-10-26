//! The F4 debug render modes: which layers of the gameplay frame draw, the
//! collision views that replace the background with the room's collision
//! boundaries and AOTs, and the AOT overlay drawn over the normal frame.
//!
//! In the collision views the middle mouse button drives a free camera with
//! Blender's controls: drag to orbit, Shift+drag to pan, wheel to zoom. It
//! starts from the cut's camera each time a collision view is entered.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use crate::game::{RoomAction, RoomActionKind};
use crate::render::{Camera, DebugVertex, Framebuffer};
use crate::state::{Collision, CollisionRect, Image, RoomId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RenderMode {
    Normal,
    BackgroundOnly,
    MasksOnly,
    ModelsOnly,
    AotOverlay,
    CollisionWire,
    CollisionSolid,
}

impl RenderMode {
    const ALL: [RenderMode; 7] = [
        RenderMode::Normal,
        RenderMode::BackgroundOnly,
        RenderMode::MasksOnly,
        RenderMode::ModelsOnly,
        RenderMode::AotOverlay,
        RenderMode::CollisionWire,
        RenderMode::CollisionSolid,
    ];

    pub(super) fn label(self) -> &'static str {
        match self {
            RenderMode::Normal => "normal",
            RenderMode::BackgroundOnly => "background only",
            RenderMode::MasksOnly => "masks only",
            RenderMode::ModelsOnly => "no background or masks",
            RenderMode::AotOverlay => "AOTs over the scene",
            RenderMode::CollisionWire => "collision (wireframe)",
            RenderMode::CollisionSolid => "collision (solid)",
        }
    }

    pub(super) fn background(self) -> bool {
        matches!(
            self,
            RenderMode::Normal | RenderMode::BackgroundOnly | RenderMode::AotOverlay
        )
    }

    pub(super) fn masks(self) -> bool {
        matches!(
            self,
            RenderMode::Normal | RenderMode::MasksOnly | RenderMode::AotOverlay
        )
    }

    /// `Some(solid)` for the collision views.
    pub(super) fn collision(self) -> Option<bool> {
        match self {
            RenderMode::CollisionWire => Some(false),
            RenderMode::CollisionSolid => Some(true),
            _ => None,
        }
    }
}

static MODE: AtomicU8 = AtomicU8::new(0);

pub(super) fn mode() -> RenderMode {
    RenderMode::ALL[usize::from(MODE.load(Ordering::Relaxed)) % RenderMode::ALL.len()]
}

/// Step to the next mode and return it.
pub(super) fn cycle() -> RenderMode {
    let next = (MODE.load(Ordering::Relaxed) + 1) % RenderMode::ALL.len() as u8;
    MODE.store(next, Ordering::Relaxed);
    mode()
}

// ---------------------------------------------------------------------------
// Free camera

/// The collision views' free camera, and the mouse input queued for it.
struct FreeCam {
    /// `None` while the view follows the cuts.
    view: Option<FreeView>,
    middle_held: bool,
    /// Queued drags in logical (240-row) pixels, and wheel notches.
    orbit: [f64; 2],
    pan: [f64; 2],
    zoom: i32,
}

/// An engaged free camera's orbit, with the cut's focal length and roll at
/// the takeover and the room it was taken over in.
#[derive(Clone, Copy)]
struct FreeView {
    eye: [f64; 3],
    target: [f64; 3],
    focal: i32,
    roll: i32,
    room: RoomId,
}

static FREE_CAM: Mutex<FreeCam> = Mutex::new(FreeCam {
    view: None,
    middle_held: false,
    orbit: [0.0; 2],
    pan: [0.0; 2],
    zoom: 0,
});

fn free_cam() -> std::sync::MutexGuard<'static, FreeCam> {
    FREE_CAM
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// When a gameplay frame last drew in a collision view.
static LAST_FRAME: Mutex<Option<std::time::Instant>> = Mutex::new(None);

/// Whether the mouse drives the free camera rather than the game: only while
/// a collision view is actually being drawn, so menus and other screens keep
/// the mouse (and queue no camera input).
pub(super) fn mouse_captured() -> bool {
    mode().collision().is_some()
        && LAST_FRAME
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some_and(|at| at.elapsed() < std::time::Duration::from_millis(250))
}

/// The middle button went down or up; returns whether the free camera owns
/// the event (a press while captured, or the release of such a press).
pub(super) fn middle_button(down: bool) -> bool {
    let mut cam = free_cam();
    let owned = if down {
        mouse_captured()
    } else {
        cam.middle_held
    };
    cam.middle_held = down && owned;
    owned
}

/// A mouse move of `delta` window pixels in a window `window_height` tall.
pub(super) fn mouse_motion(delta: [f32; 2], window_height: f32, shift: bool) {
    let mut cam = free_cam();
    if !cam.middle_held {
        return;
    }
    let scale = 240.0 / f64::from(window_height.max(1.0));
    let delta = delta.map(|d| f64::from(d) * scale);
    let target = if shift { &mut cam.pan } else { &mut cam.orbit };
    target[0] += delta[0];
    target[1] += delta[1];
}

pub(super) fn mouse_wheel(notches: i32) {
    free_cam().zoom += notches;
}

/// Whether the frame is drawn from the free camera, which the cuts' switch
/// zones mean nothing to: the renderer then culls nothing by them.
static UNCULLED: AtomicBool = AtomicBool::new(false);

pub(super) fn unculled() -> bool {
    UNCULLED.load(Ordering::Relaxed)
}

/// The frame's camera: `cut_camera` outside the collision views, otherwise
/// the free camera with the queued mouse input applied.
pub(super) fn camera(cut_camera: Camera, room: RoomId) -> Camera {
    let collision = mode().collision().is_some();
    *LAST_FRAME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) =
        collision.then(std::time::Instant::now);
    let camera = view_camera(cut_camera, room, collision);
    UNCULLED.store(free_cam().view.is_some(), Ordering::Relaxed);
    camera
}

fn view_camera(cut_camera: Camera, room: RoomId, collision: bool) -> Camera {
    let mut cam = free_cam();
    if !collision {
        *cam = FreeCam {
            view: None,
            middle_held: cam.middle_held,
            orbit: [0.0; 2],
            pan: [0.0; 2],
            zoom: 0,
        };
        return cut_camera;
    }
    let orbit = std::mem::take(&mut cam.orbit);
    let pan = std::mem::take(&mut cam.pan);
    let zoom = std::mem::take(&mut cam.zoom);
    // The view follows the cuts until the mouse first moves it; a room change
    // hands it back to the cuts.
    // Once engaged, a cut change moves nothing: the focal length and roll
    // are the ones the view took over with.
    let (mut eye, mut target, [focal, roll]) = match cam.view {
        Some(view) if view.room == room => (view.eye, view.target, [view.focal, view.roll]),
        Some(_) => {
            cam.view = None;
            return cut_camera;
        }
        None if orbit == [0.0; 2] && pan == [0.0; 2] && zoom == 0 => return cut_camera,
        None => (
            cut_camera.from.map(f64::from),
            cut_camera.look_at.map(f64::from),
            [cut_camera.fov, cut_camera.roll],
        ),
    };

    let current =
        Camera::from_points_rolled(eye.map(|v| v as i32), target.map(|v| v as i32), focal, roll);
    // The view rows are the world directions of screen right and screen up.
    let right = current.view[0].map(|v| f64::from(v) / 4096.0);
    let up = current.view[1].map(|v| f64::from(v) / 4096.0);
    let mut offset = sub(eye, target);
    let mut distance = length(offset).max(1.0);
    let fov = f64::from(focal.max(1));

    // Grab-the-scene: the camera moves against the mouse.
    if orbit != [0.0; 2] {
        let step = 0.012 * distance;
        let moved = add(
            offset,
            add(scale(right, -orbit[0] * step), scale(up, orbit[1] * step)),
        );
        let moved = scale(moved, distance / length(moved).max(1e-6));
        // Stop short of the poles, where the look-at basis degenerates.
        let horizontal = (moved[0] * moved[0] + moved[2] * moved[2]).sqrt();
        offset = if horizontal > 0.05 * distance {
            moved
        } else {
            let sideways = add(offset, scale(right, -orbit[0] * step));
            scale(sideways, distance / length(sideways).max(1e-6))
        };
    }
    if pan != [0.0; 2] {
        let per_pixel = distance / fov;
        let shift = add(
            scale(right, -pan[0] * per_pixel),
            scale(up, pan[1] * per_pixel),
        );
        target = add(target, shift);
    }
    if zoom != 0 {
        // Never in through the near plane.
        let closest = near(&current) * 1.5;
        distance = (distance * 0.85f64.powi(zoom)).clamp(closest, closest.max(200_000.0));
        offset = scale(offset, distance / length(offset).max(1e-6));
    }
    eye = add(target, offset);
    cam.view = Some(FreeView {
        eye,
        target,
        focal,
        roll,
        room,
    });

    let mut camera =
        Camera::from_points_rolled(eye.map(|v| v as i32), target.map(|v| v as i32), focal, roll);
    camera.screen = cut_camera.screen;
    camera
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] + b[i])
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}

fn scale(a: [f64; 3], s: f64) -> [f64; 3] {
    a.map(|v| v * s)
}

fn length(a: [f64; 3]) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

// ---------------------------------------------------------------------------
// Drawing
//
// Everything is built as debug triangles ([`DebugVertex`]): a logical screen
// position plus the view depth each corner sits at. The GPU renderer draws
// them in its scene pass against the meshes' depth buffer; the software path
// rasterizes them here ([`draw_software`]) against [`crate::render::mesh_depth`].

/// How tall the blocking collision walls are drawn, in world units (Y is
/// down).
const WALL_HEIGHT: i32 = 1800;
/// How tall the floor-zone records (bit 8 of the flags clear) are drawn.
const LEDGE_HEIGHT: i32 = 400;
/// How tall a non-blocking record or an AOT's zone is drawn.
const LOW_HEIGHT: i32 = 200;
/// How far above its zone an AOT's marker floats.
const MARKER_HEIGHT: i32 = 900;
const GRID: i32 = 1000;
/// Half a line's width in logical pixels, for the GPU's resolution and for
/// the software's 320x240 (where a thinner quad would miss pixel centres).
pub(super) const GPU_LINE: f32 = 0.35;
pub(super) const SOFTWARE_LINE: f32 = 0.6;
/// A dashed line's period and drawn length, in logical pixels.
const DASH_PERIOD: f32 = 6.0;
const DASH_LENGTH: f32 = 3.0;
/// How far behind a model's surface a debug pixel still shows in software,
/// so zones on the floor are not lost under the feet in its coarse 320x240
/// depth. The GPU's per-pixel depth needs no such slack and has none.
const DEPTH_BIAS: f32 = 30.0;
/// The widest view-depth range one debug triangle or line piece spans, at
/// least. The GPU interpolates the scene's linear depth ramp linearly on
/// screen, which is off by about `span^2 / (4 z)` mid-piece, so faces and
/// lines are cut up until that stays well under [`LINE_PULL`]'s gap and
/// nothing z-fights; farther pieces may span more ([`max_span`]).
const MAX_DEPTH_SPAN: f32 = 120.0;

/// The widest depth range a piece whose nearest point is `z` deep may span.
fn max_span(z: f32) -> f32 {
    MAX_DEPTH_SPAN.max(z * 0.08)
}
/// Lines sit this much nearer than the faces they edge (a fraction of their
/// depth plus a constant), so an edge always wins over its own face.
const LINE_PULL: f32 = 0.996;
const LINE_PULL_UNITS: f32 = 4.0;
/// The collision views' walls and zones draw a little see-through, and again
/// faintly over everything, so what they hide (each other, or behind a
/// model) still just shows.
const BLOCK_ALPHA: u8 = 215;
const GHOST_ALPHA: u8 = 40;

/// A vertical prism: a floor outline extruded upwards by `height` from the
/// per-corner base heights `base` (a stair ramp slopes).
struct Prism {
    outline: Vec<[i32; 2]>,
    base: Vec<i32>,
    height: i32,
    colour: [u8; 4],
    /// Draw dashed edges in the wireframe view (the AOTs).
    dashed: bool,
    /// Draw only every `uprights`th upright edge (the circles).
    uprights: usize,
}

/// The collision views' background: one fixed dark image, so the GPU keeps
/// a single upload (and looks up no HD replacement per frame).
pub(super) fn collision_backdrop() -> &'static Image {
    static BACKDROP: std::sync::LazyLock<Image> = std::sync::LazyLock::new(|| Image {
        width: crate::render::WIDTH,
        height: crate::render::HEIGHT,
        rgba: [8, 10, 18, 255]
            .repeat(crate::render::WIDTH as usize * crate::render::HEIGHT as usize),
    });
    &BACKDROP
}

/// A fixed floor grid at `floor_y` over the room's extent (what the
/// collision and AOTs cover, with a margin) and the collision quadrant split.
fn draw_grid(
    out: &mut Triangles,
    collision: &Collision,
    actions: &[Option<RoomAction>],
    camera: &Camera,
    floor_y: i32,
) {
    let mut bounds = [i32::MAX, i32::MAX, i32::MIN, i32::MIN];
    let mut extend = |[x0, z0, x1, z1]: [i32; 4]| {
        bounds = [
            bounds[0].min(x0),
            bounds[1].min(z0),
            bounds[2].max(x1),
            bounds[3].max(z1),
        ];
    };
    for rect in collision.quadrants.iter().flatten() {
        extend([
            i32::from(rect.x_min),
            i32::from(rect.z_min),
            i32::from(rect.x_max),
            i32::from(rect.z_max),
        ]);
    }
    for action in actions.iter().flatten() {
        extend(aot_bounds(action));
    }
    let [x0, z0, x1, z1] = if bounds[0] > bounds[2] {
        [-20 * GRID, -20 * GRID, 20 * GRID, 20 * GRID]
    } else {
        [
            (bounds[0] - 2 * GRID).div_euclid(GRID) * GRID,
            (bounds[1] - 2 * GRID).div_euclid(GRID) * GRID,
            (bounds[2] + 3 * GRID).div_euclid(GRID) * GRID,
            (bounds[3] + 3 * GRID).div_euclid(GRID) * GRID,
        ]
    };
    let grid = [28, 36, 60, 255];
    for x in (x0..=x1).step_by(GRID as usize) {
        out.line3(camera, [x, floor_y, z0], [x, floor_y, z1], grid, false);
    }
    for z in (z0..=z1).step_by(GRID as usize) {
        out.line3(camera, [x0, floor_y, z], [x1, floor_y, z], grid, false);
    }
    let cx = i32::from(collision.cell_x);
    let cz = i32::from(collision.cell_z);
    let split = [120, 50, 120, 255];
    out.line3(camera, [cx, floor_y, z0], [cx, floor_y, z1], split, false);
    out.line3(camera, [x0, floor_y, cz], [x1, floor_y, cz], split, false);
}

/// The collision views' debug triangles: the floor grid, each collision
/// record extruded into a wall coloured by shape and each AOT a low zone
/// coloured by type, then the AOT markers and the player's floor point over
/// all of it. `solid` fills the walls' and zones' shaded faces back to front
/// together; otherwise they draw as wireframes.
pub(super) fn collision_triangles(
    collision: &Collision,
    actions: &[Option<RoomAction>],
    camera: &Camera,
    floor_y: i32,
    player: [i32; 3],
    solid: bool,
    line: f32,
) -> Vec<DebugVertex> {
    let mut out = Triangles::new(line);
    draw_grid(&mut out, collision, actions, camera, floor_y);
    let mut prisms = Vec::new();
    let mut seen: Vec<CollisionRect> = Vec::new();
    for rect in collision.quadrants.iter().flatten() {
        if seen.contains(rect) {
            continue;
        }
        seen.push(*rect);
        prisms.push(record_prism(rect, floor_y));
    }
    let aots: Vec<&RoomAction> = actions.iter().flatten().collect();
    prisms.extend(aots.iter().map(|action| aot_prism(action, floor_y)));
    let first = out.vertices.len();
    if solid {
        draw_solid(&mut out, camera, &prisms);
    } else {
        for prism in &prisms {
            draw_wire(&mut out, camera, prism);
        }
    }
    // The ghost copy sits at the nearest depth and writes none, so it shows
    // through everything.
    let ghost: Vec<DebugVertex> = out.vertices[first..]
        .iter()
        .map(|vertex| DebugVertex {
            depth: 1.0,
            write: false,
            colour: with_alpha(vertex.colour, GHOST_ALPHA),
            ..*vertex
        })
        .collect();
    for vertex in &mut out.vertices[first..] {
        vertex.colour = with_alpha(vertex.colour, BLOCK_ALPHA);
    }
    out.vertices.extend(ghost);
    for action in &aots {
        draw_marker(&mut out, camera, action, floor_y, true);
    }

    let marker = [255, 255, 255, 255];
    let p = [player[0], player[1], player[2]];
    out.line3(
        camera,
        [p[0] - 150, p[1], p[2]],
        [p[0] + 150, p[1], p[2]],
        marker,
        false,
    );
    out.line3(
        camera,
        [p[0], p[1], p[2] - 150],
        [p[0], p[1], p[2] + 150],
        marker,
        false,
    );
    out.vertices
}

/// The AOT overlay's debug triangles: the room's AOTs as wireframe zones,
/// then their markers.
pub(super) fn aot_triangles(
    actions: &[Option<RoomAction>],
    camera: &Camera,
    floor_y: i32,
    line: f32,
) -> Vec<DebugVertex> {
    let mut out = Triangles::new(line);
    let aots: Vec<&RoomAction> = actions.iter().flatten().collect();
    for action in &aots {
        let mut prism = aot_prism(action, floor_y);
        prism.dashed = false;
        draw_wire(&mut out, camera, &prism);
    }
    for action in &aots {
        draw_marker(&mut out, camera, action, floor_y, false);
    }
    out.vertices
}

/// Draw `triangles` over the finished 320x240 software scene in
/// `framebuffer`, hidden where `meshes` are in front.
pub(super) fn draw_software(
    framebuffer: &mut Framebuffer,
    triangles: &[DebugVertex],
    meshes: &[crate::render::EntityMesh<'_>],
    camera: &Camera,
) {
    let (width, height) = (crate::render::WIDTH, crate::render::HEIGHT);
    let mut depth = crate::render::mesh_depth(meshes, camera, width, height);
    let x_offset = framebuffer.width.saturating_sub(width) / 2;
    let mut image = Image {
        width: framebuffer.width,
        height: framebuffer.height,
        rgba: std::mem::take(&mut framebuffer.rgba),
    };
    rasterize(
        &mut image,
        x_offset,
        [1.0, 1.0],
        triangles,
        Some(&mut depth),
    );
    framebuffer.rgba = image.rgba;
}

/// Blend `triangles` into `image` in order. A logical point lands at
/// `x * scale + x_offset`; `depth`, when given, is a logical 320x240 depth
/// buffer that hides farther pixels and that the writing triangles update.
fn rasterize(
    image: &mut Image,
    x_offset: u32,
    scale: [f32; 2],
    triangles: &[DebugVertex],
    mut depth: Option<&mut Vec<f32>>,
) {
    let (w, h) = (image.width as i32, image.height as i32);
    for triangle in triangles.chunks_exact(3) {
        let p: [[f32; 2]; 3] = std::array::from_fn(|i| {
            [
                triangle[i].position[0] * scale[0] + x_offset as f32,
                triangle[i].position[1] * scale[1],
            ]
        });
        let area =
            (p[1][0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[2][0] - p[0][0]) * (p[1][1] - p[0][1]);
        if area.abs() < 1e-6 || !area.is_finite() {
            continue;
        }
        let inverse: [f32; 3] = std::array::from_fn(|i| 1.0 / triangle[i].depth.max(1.0));
        let colour = triangle[0].colour;
        let alpha = u32::from(colour[3]);
        let x0 = (p.iter().map(|v| v[0]).fold(f32::MAX, f32::min).floor() as i32).max(0);
        let x1 = (p.iter().map(|v| v[0]).fold(f32::MIN, f32::max).ceil() as i32).min(w - 1);
        let y0 = (p.iter().map(|v| v[1]).fold(f32::MAX, f32::min).floor() as i32).max(0);
        let y1 = (p.iter().map(|v| v[1]).fold(f32::MIN, f32::max).ceil() as i32).min(h - 1);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let c = [x as f32 + 0.5, y as f32 + 0.5];
                let edge = |a: [f32; 2], b: [f32; 2]| {
                    (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])
                };
                let l0 = edge(p[1], p[2]) / area;
                let l1 = edge(p[2], p[0]) / area;
                let l2 = edge(p[0], p[1]) / area;
                if l0 < 0.0 || l1 < 0.0 || l2 < 0.0 {
                    continue;
                }
                if let Some(depth) = depth.as_deref_mut() {
                    let lx = ((x - x_offset as i32) as f32 / scale[0]) as i32;
                    let ly = (y as f32 / scale[1]) as i32;
                    let z = 1.0 / (l0 * inverse[0] + l1 * inverse[1] + l2 * inverse[2]);
                    if (0..crate::render::WIDTH as i32).contains(&lx)
                        && (0..crate::render::HEIGHT as i32).contains(&ly)
                        && let Some(near) =
                            depth.get_mut(ly as usize * crate::render::WIDTH as usize + lx as usize)
                    {
                        if z > *near + DEPTH_BIAS {
                            continue;
                        }
                        if triangle[0].write && z < *near {
                            *near = z;
                        }
                    }
                }
                let at = (y as usize * w as usize + x as usize) * 4;
                let pixel = &mut image.rgba[at..at + 4];
                for channel in 0..3 {
                    let old = u32::from(pixel[channel]);
                    pixel[channel] =
                        ((old * (255 - alpha) + u32::from(colour[channel]) * alpha) / 255) as u8;
                }
                pixel[3] = 255;
            }
        }
    }
}

fn shape_colour(rect: &CollisionRect) -> [u8; 4] {
    let blocks = rect.flags & 0x300 != 0;
    let colour = match rect.kind & 0xFF {
        // A floor-zone record (bit 8 clear): an entity in floor mode
        // (collision flag 0x04) reads it as a height step instead of a wall.
        1 | 5 if blocks && rect.flags & 0x100 == 0 => [190, 110, 255],
        1 if blocks => [255, 70, 60],
        1 => [255, 170, 60],
        3 => [60, 200, 255],
        4 => [80, 255, 120],
        5 => [255, 230, 60],
        _ => [190, 190, 190],
    };
    [colour[0], colour[1], colour[2], 255]
}

fn record_prism(rect: &CollisionRect, floor_y: i32) -> Prism {
    let x0 = i32::from(rect.x_min);
    let x1 = i32::from(rect.x_max);
    let z0 = i32::from(rect.z_min);
    let z1 = i32::from(rect.z_max);
    let shape = rect.kind & 0xFF;
    let height = if rect.flags & 0x300 == 0 || shape == 4 {
        LOW_HEIGHT
    } else if rect.flags & 0x100 == 0 {
        LEDGE_HEIGHT
    } else {
        WALL_HEIGHT
    };
    let circle = shape == 3;
    let outline: Vec<[i32; 2]> = if circle {
        // A circle inscribed in the record's box.
        let (mx, mz) = ((x0 + x1) / 2, (z0 + z1) / 2);
        let (rx, rz) = ((x1 - x0) as f32 / 2.0, (z1 - z0) as f32 / 2.0);
        (0..24)
            .map(|i| {
                let a = i as f32 / 24.0 * std::f32::consts::TAU;
                [mx + (a.cos() * rx) as i32, mz + (a.sin() * rz) as i32]
            })
            .collect()
    } else {
        vec![[x0, z0], [x1, z0], [x1, z1], [x0, z1]]
    };
    Prism {
        base: vec![floor_y; outline.len()],
        outline,
        height,
        colour: shape_colour(rect),
        dashed: false,
        uprights: if circle { 6 } else { 1 },
    }
}

fn aot_colour(kind: RoomActionKind) -> [u8; 4] {
    let colour = match kind {
        RoomActionKind::Door => [80, 140, 255],
        RoomActionKind::DoorCheck => [60, 90, 170],
        RoomActionKind::Item => [255, 220, 40],
        RoomActionKind::ItemBox => [200, 120, 255],
        RoomActionKind::Event => [255, 80, 200],
        RoomActionKind::Message => [240, 240, 240],
        RoomActionKind::Typewriter => [120, 255, 255],
        RoomActionKind::EffectZone => [100, 200, 100],
        RoomActionKind::Desk => [210, 150, 90],
        RoomActionKind::StairsZone | RoomActionKind::StairsHeight => [255, 140, 40],
        RoomActionKind::FlagBankSet => [255, 120, 120],
        RoomActionKind::Other => [150, 150, 150],
    };
    [colour[0], colour[1], colour[2], 255]
}

/// An AOT's zone extends from its origin towards +X/+Z (see
/// [`RoomAction::contains`]).
fn aot_bounds(action: &RoomAction) -> [i32; 4] {
    let [x, z, width, depth] = action.zone;
    let x0 = i32::from(x);
    let z0 = i32::from(z);
    [
        x0,
        z0,
        x0 + i32::from(width as u16),
        z0 + i32::from(depth as u16),
    ]
}

/// The floor height under an AOT at `(x, z)`: a stair ramp's own slope (the
/// height the original holds the player at), `floor_y` for everything else.
fn aot_floor(action: &RoomAction, floor_y: i32, x: i32, z: i32) -> i32 {
    match crate::stairs::StairZone::from_action(action) {
        Some(zone @ crate::stairs::StairZone::Height { .. }) => {
            crate::stairs::ramp_height(&zone, x, z)
        }
        _ => floor_y,
    }
}

fn aot_prism(action: &RoomAction, floor_y: i32) -> Prism {
    let [x0, z0, x1, z1] = aot_bounds(action);
    let outline = vec![[x0, z0], [x1, z0], [x1, z1], [x0, z1]];
    Prism {
        base: outline
            .iter()
            .map(|&[x, z]| aot_floor(action, floor_y, x, z))
            .collect(),
        outline,
        height: LOW_HEIGHT,
        colour: aot_colour(action.kind),
        dashed: true,
        uprights: 1,
    }
}

fn draw_wire(out: &mut Triangles, camera: &Camera, prism: &Prism) {
    let colour = prism.colour;
    let top = colour.map(|c| c / 2 + 40);
    let upright = colour.map(|c| c / 4 * 3);
    let n = prism.outline.len();
    for i in 0..n {
        let j = (i + 1) % n;
        let [ax, az] = prism.outline[i];
        let [bx, bz] = prism.outline[j];
        let (ay, by) = (prism.base[i], prism.base[j]);
        let h = prism.height;
        out.line3(camera, [ax, ay, az], [bx, by, bz], colour, prism.dashed);
        out.line3(
            camera,
            [ax, ay - h, az],
            [bx, by - h, bz],
            top,
            prism.dashed,
        );
        if i % prism.uprights == 0 {
            out.line3(
                camera,
                [ax, ay, az],
                [ax, ay - h, az],
                upright,
                prism.dashed,
            );
        }
    }
}

/// One face of the solid view: its world corners, shaded colour and depth.
struct Face {
    corners: Vec<[i32; 3]>,
    colour: [u8; 4],
    depth: i64,
    /// The face's view position scales by this: the same screen position a
    /// little nearer, so an AOT zone flush with a wall shows over it.
    pull: f64,
}

/// `colour` with its alpha scaled by `alpha / 255`.
fn with_alpha([r, g, b, a]: [u8; 4], alpha: u8) -> [u8; 4] {
    [r, g, b, (u16::from(a) * u16::from(alpha) / 255) as u8]
}

fn shade(colour: [u8; 4], light: f32) -> [u8; 4] {
    let [r, g, b, a] = colour;
    let lit = |c: u8| (f32::from(c) * light).min(255.0) as u8;
    [lit(r), lit(g), lit(b), a]
}

/// Fill `prisms`' faces, painter-sorted far to near among themselves.
fn draw_solid(out: &mut Triangles, camera: &Camera, prisms: &[Prism]) {
    let mut faces = Vec::new();
    for prism in prisms {
        let n = prism.outline.len();
        let h = prism.height;
        // Only the AOT zones dash.
        let pull = if prism.dashed { 0.998 } else { 1.0 };
        faces.push(Face {
            corners: (0..n)
                .map(|i| {
                    let [x, z] = prism.outline[i];
                    [x, prism.base[i] - h, z]
                })
                .collect(),
            colour: prism.colour,
            depth: 0,
            pull,
        });
        for i in 0..n {
            let j = (i + 1) % n;
            let [ax, az] = prism.outline[i];
            let [bx, bz] = prism.outline[j];
            let (ay, by) = (prism.base[i], prism.base[j]);
            // Light the walls by their facing so the corners read.
            let (dx, dz) = ((bx - ax) as f32, (bz - az) as f32);
            let length = (dx * dx + dz * dz).sqrt().max(1.0);
            let light = 0.4 + 0.45 * ((dz / length) * 0.6 - (dx / length) * 0.8).abs();
            faces.push(Face {
                corners: vec![
                    [ax, ay, az],
                    [bx, by, bz],
                    [bx, by - h, bz],
                    [ax, ay - h, az],
                ],
                colour: shade(prism.colour, light),
                depth: 0,
                pull,
            });
        }
    }
    for face in &mut faces {
        let sum: i64 = face
            .corners
            .iter()
            .map(|&corner| i64::from(camera.view_position(corner)[2]))
            .sum();
        face.depth = sum / face.corners.len() as i64;
    }
    faces.sort_by_key(|face| std::cmp::Reverse(face.depth));
    for face in &faces {
        let view: Vec<[f64; 3]> = face
            .corners
            .iter()
            .map(|&corner| camera.view_position(corner).map(f64::from))
            .collect();
        let clipped = clip_near(&view, near(camera));
        if clipped.len() < 3 {
            continue;
        }
        let pulled: Vec<[f64; 3]> = clipped.iter().map(|v| v.map(|c| c * face.pull)).collect();
        for i in 1..pulled.len() - 1 {
            out.view_triangle(
                camera,
                [pulled[0], pulled[i], pulled[i + 1]],
                face.colour,
                0,
            );
        }
        let points: Vec<([f32; 2], f32)> = clipped
            .iter()
            .map(|&v| (project_view(camera, v), v[2] as f32))
            .collect();
        let edge = face.colour.map(|c| c / 3);
        for i in 0..points.len() {
            let (a, b) = (points[i], points[(i + 1) % points.len()]);
            out.line2(a, b, [edge[0], edge[1], edge[2], 255], false);
        }
    }
}

/// The near plane's view depth ([`Camera::project`] rejects anything nearer).
fn near(camera: &Camera) -> f64 {
    f64::from(2 * camera.fov.max(1)) + 1.0
}

fn project_view(camera: &Camera, [vx, vy, vz]: [f64; 3]) -> [f32; 2] {
    let scale = f64::from(camera.fov) / vz;
    [
        (vx * scale + f64::from(crate::render::WIDTH as i32 / 2 + camera.screen[0])) as f32,
        (f64::from(crate::render::HEIGHT as i32 / 2 + camera.screen[1]) - vy * scale) as f32,
    ]
}

/// The part `[t_in, t_out]` of the screen segment `a + d * t` (`t` in 0..1)
/// inside the logical screen and a small margin, or `None` when off it.
fn clip_to_screen(a: [f32; 2], d: [f32; 2]) -> Option<(f32, f32)> {
    const MARGIN: f32 = 8.0;
    let (mut t_in, mut t_out) = (0.0f32, 1.0f32);
    let bounds = [
        (-MARGIN, crate::render::WIDTH as f32 + MARGIN),
        (-MARGIN, crate::render::HEIGHT as f32 + MARGIN),
    ];
    for axis in 0..2 {
        let (low, high) = bounds[axis];
        if d[axis].abs() < 1e-6 {
            if a[axis] < low || a[axis] > high {
                return None;
            }
            continue;
        }
        let (t0, t1) = ((low - a[axis]) / d[axis], (high - a[axis]) / d[axis]);
        t_in = t_in.max(t0.min(t1));
        t_out = t_out.min(t0.max(t1));
    }
    (t_in < t_out).then_some((t_in, t_out))
}

/// Clip a view-space polygon to the part at or beyond `near`.
fn clip_near(points: &[[f64; 3]], near: f64) -> Vec<[f64; 3]> {
    let mut out = Vec::with_capacity(points.len() + 2);
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let (a_in, b_in) = (a[2] >= near, b[2] >= near);
        if a_in {
            out.push(a);
        }
        if a_in != b_in {
            let t = (near - a[2]) / (b[2] - a[2]);
            out.push(std::array::from_fn(|k| a[k] + (b[k] - a[k]) * t));
        }
    }
    out
}

/// A screen-space glyph over the AOT's centre, its shape naming the type, at
/// the depth of the point it floats at, or over everything when `on_top`.
fn draw_marker(
    out: &mut Triangles,
    camera: &Camera,
    action: &RoomAction,
    floor_y: i32,
    on_top: bool,
) {
    let [x0, z0, x1, z1] = aot_bounds(action);
    let (cx, cz) = ((x0 + x1) / 2, (z0 + z1) / 2);
    let base = aot_floor(action, floor_y, cx, cz);
    let centre = [cx, base, cz];
    let raised = [cx, base - MARKER_HEIGHT, cz];
    let colour = aot_colour(action.kind);
    // A pole from the zone up to the marker.
    out.line3(camera, centre, raised, colour.map(|c| c / 2), false);
    let view = camera.view_position(raised);
    if f64::from(view[2]) < near(camera) {
        return;
    }
    let [mx, my] = project_view(camera, view.map(f64::from));
    // Pulled a little towards the camera so the glyph sits on its pole. The
    // glyph only tests depth, so one never hides another's pole.
    out.pen = if on_top {
        1.0
    } else {
        (view[2] - 20).max(1) as f32
    };
    out.write = false;
    draw_glyph(out, action.kind, colour, [mx, my]);
    out.write = true;
}

fn draw_glyph(out: &mut Triangles, kind: RoomActionKind, colour: [u8; 4], [mx, my]: [f32; 2]) {
    let r = 5.0;
    let black = [0, 0, 0, 255];
    let at = |dx: f32, dy: f32| [mx + dx, my + dy];
    let poly = |out: &mut Triangles, points: &[[f32; 2]], fill: bool| {
        if fill {
            out.fill2(points, colour);
        }
        let edge = if fill { black } else { colour };
        for i in 0..points.len() {
            out.flat_line(points[i], points[(i + 1) % points.len()], edge);
        }
    };
    let ring: Vec<[f32; 2]> = (0..12)
        .map(|i| {
            let a = i as f32 / 12.0 * std::f32::consts::TAU;
            at(a.cos() * r, a.sin() * r)
        })
        .collect();
    let square = [at(-r, -r), at(r, -r), at(r, r), at(-r, r)];
    match kind {
        // A door: a tall panel with a knob.
        RoomActionKind::Door => {
            let (w, h) = (r * 0.7, r * 1.4);
            poly(out, &[at(-w, -h), at(w, -h), at(w, h), at(-w, h)], true);
            out.fill2(
                &[at(1.5, -0.5), at(2.5, -0.5), at(2.5, 0.5), at(1.5, 0.5)],
                black,
            );
        }
        // A door side latch: an outlined narrow panel.
        RoomActionKind::DoorCheck => {
            let w = r * 0.5;
            poly(out, &[at(-w, -r), at(w, -r), at(w, r), at(-w, r)], false);
        }
        RoomActionKind::Item => poly(
            out,
            &[at(0.0, -r), at(r, 0.0), at(0.0, r), at(-r, 0.0)],
            true,
        ),
        // An item box: a chest with a lid line.
        RoomActionKind::ItemBox => {
            poly(out, &square, true);
            out.flat_line(at(-r, -r * 0.3), at(r, -r * 0.3), black);
        }
        RoomActionKind::Event => poly(out, &[at(0.0, -r), at(r, r), at(-r, r)], true),
        RoomActionKind::Message => poly(out, &ring, true),
        // A typewriter: a boxed T.
        RoomActionKind::Typewriter => {
            poly(out, &square, false);
            out.flat_line(at(-r * 0.6, -r * 0.5), at(r * 0.6, -r * 0.5), colour);
            out.flat_line(at(0.0, -r * 0.5), at(0.0, r * 0.6), colour);
        }
        RoomActionKind::EffectZone => poly(out, &ring, false),
        RoomActionKind::Desk => {
            out.flat_line(at(-r, -r), at(r, r), colour);
            out.flat_line(at(r, -r), at(-r, r), colour);
        }
        // Stairs: an up arrow, filled for the entry, hollow for the ramp.
        RoomActionKind::StairsZone | RoomActionKind::StairsHeight => {
            let s = r * 0.4;
            let head = [at(0.0, -r), at(r, s), at(-r, s)];
            let shaft = [at(-s, s), at(s, s), at(s, r), at(-s, r)];
            if kind == RoomActionKind::StairsZone {
                out.fill2(&head, colour);
                out.fill2(&shaft, colour);
            }
            let outline = [
                at(0.0, -r),
                at(r, s),
                at(s, s),
                at(s, r),
                at(-s, r),
                at(-s, s),
                at(-r, s),
            ];
            let edge = if kind == RoomActionKind::StairsZone {
                black
            } else {
                colour
            };
            for i in 0..outline.len() {
                out.flat_line(outline[i], outline[(i + 1) % outline.len()], edge);
            }
        }
        RoomActionKind::FlagBankSet => {
            out.flat_line(at(-r, 0.0), at(r, 0.0), colour);
            out.flat_line(at(0.0, -r), at(0.0, r), colour);
        }
        RoomActionKind::Other => poly(
            out,
            &[at(-2.0, -2.0), at(2.0, -2.0), at(2.0, 2.0), at(-2.0, 2.0)],
            true,
        ),
    }
}

/// The debug triangle list being built.
struct Triangles {
    vertices: Vec<DebugVertex>,
    /// The view depth screen-space glyphs draw at.
    pen: f32,
    /// Half a line's width, in logical pixels.
    half_width: f32,
    /// Whether what is drawn now writes depth.
    write: bool,
}

impl Triangles {
    fn new(half_width: f32) -> Self {
        Self {
            vertices: Vec::new(),
            pen: 1.0,
            half_width,
            write: true,
        }
    }

    fn triangle(&mut self, corners: [([f32; 2], f32); 3], colour: [u8; 4]) {
        for (position, depth) in corners {
            self.vertices.push(DebugVertex {
                position,
                depth,
                colour,
                write: self.write,
            });
        }
    }

    /// A view-space triangle, halved across its widest depth edge until it
    /// spans at most [`max_span`] of its nearest depth.
    fn view_triangle(
        &mut self,
        camera: &Camera,
        corners: [[f64; 3]; 3],
        colour: [u8; 4],
        level: u32,
    ) {
        let z = corners.map(|corner| corner[2]);
        // Nothing off screen is cut up (or drawn).
        let screen = corners.map(|corner| project_view(camera, corner));
        let (w, h) = (crate::render::WIDTH as f32, crate::render::HEIGHT as f32);
        if screen.iter().all(|p| p[0] < 0.0)
            || screen.iter().all(|p| p[0] > w)
            || screen.iter().all(|p| p[1] < 0.0)
            || screen.iter().all(|p| p[1] > h)
        {
            return;
        }
        let nearest = z.iter().copied().fold(f64::MAX, f64::min);
        let span = z.iter().copied().fold(f64::MIN, f64::max) - nearest;
        if span <= f64::from(max_span(nearest as f32)) || level >= 14 {
            self.triangle(
                corners.map(|corner| (project_view(camera, corner), corner[2] as f32)),
                colour,
            );
            return;
        }
        // Split the edge with the widest depth range at its view-space
        // midpoint.
        let edge = (0..3)
            .max_by(|&a, &b| {
                let range = |i: usize| (z[i] - z[(i + 1) % 3]).abs();
                range(a).total_cmp(&range(b))
            })
            .unwrap_or(0);
        let (a, b, c) = (
            corners[edge],
            corners[(edge + 1) % 3],
            corners[(edge + 2) % 3],
        );
        let mid: [f64; 3] = std::array::from_fn(|k| (a[k] + b[k]) / 2.0);
        self.view_triangle(camera, [a, mid, c], colour, level + 1);
        self.view_triangle(camera, [mid, b, c], colour, level + 1);
    }

    /// A convex polygon of `(position, view depth)` points, as a fan.
    fn fan(&mut self, points: &[([f32; 2], f32)], colour: [u8; 4]) {
        for i in 1..points.len().saturating_sub(1) {
            self.triangle([points[0], points[i], points[i + 1]], colour);
        }
    }

    /// A convex screen-space polygon at the pen depth.
    fn fill2(&mut self, points: &[[f32; 2]], colour: [u8; 4]) {
        let points: Vec<_> = points.iter().map(|&p| (p, self.pen)).collect();
        self.fan(&points, colour);
    }

    /// A screen-space line at the pen depth.
    fn flat_line(&mut self, a: [f32; 2], b: [f32; 2], colour: [u8; 4]) {
        let pen = self.pen;
        self.line2((a, pen), (b, pen), colour, false);
    }

    /// A world-space line, clipped to the near plane.
    fn line3(&mut self, camera: &Camera, a: [i32; 3], b: [i32; 3], colour: [u8; 4], dashed: bool) {
        let mut a = camera.view_position(a).map(f64::from);
        let mut b = camera.view_position(b).map(f64::from);
        let near = near(camera);
        if a[2] < near && b[2] < near {
            return;
        }
        if a[2] < near || b[2] < near {
            let t = (near - a[2]) / (b[2] - a[2]);
            let cut: [f64; 3] = std::array::from_fn(|k| a[k] + (b[k] - a[k]) * t);
            if a[2] < near {
                a = cut;
            } else {
                b = cut;
            }
        }
        self.line2(
            (project_view(camera, a), a[2] as f32),
            (project_view(camera, b), b[2] as f32),
            colour,
            dashed,
        );
    }

    /// A screen-space line between two `(position, view depth)` points, as a
    /// thin quad (or a run of dashes); the depth interpolates as `1/z`.
    fn line2(
        &mut self,
        (a, za): ([f32; 2], f32),
        (b, zb): ([f32; 2], f32),
        colour: [u8; 4],
        dashed: bool,
    ) {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let length = (dx * dx + dy * dy).sqrt();
        if !length.is_finite() || length < 1e-4 {
            return;
        }
        let normal = [
            -dy / length * self.half_width,
            dx / length * self.half_width,
        ];
        let (za, zb) = (
            (za * LINE_PULL - LINE_PULL_UNITS).max(1.0),
            (zb * LINE_PULL - LINE_PULL_UNITS).max(1.0),
        );
        let (ia, ib) = (1.0 / za, 1.0 / zb);
        let at = |t: f32| {
            let p = [a[0] + dx * t, a[1] + dy * t];
            (p, 1.0 / (ia + (ib - ia) * t))
        };
        let piece = |out: &mut Self, t0: f32, t1: f32| {
            let ((p0, z0), (p1, z1)) = (at(t0), at(t1));
            let corner =
                |p: [f32; 2], sign: f32| [p[0] + normal[0] * sign, p[1] + normal[1] * sign];
            let quad = [
                (corner(p0, 1.0), z0),
                (corner(p1, 1.0), z1),
                (corner(p1, -1.0), z1),
                (corner(p0, -1.0), z0),
            ];
            out.fan(&quad, colour);
        };
        // Each piece is halved until it spans at most `max_span`.
        let piece = |out: &mut Self, t0: f32, t1: f32| {
            let mut pending = vec![(t0, t1, 0)];
            while let Some((t0, t1, level)) = pending.pop() {
                let (z0, z1) = (at(t0).1, at(t1).1);
                if (z0 - z1).abs() <= max_span(z0.min(z1)) || level >= 12 {
                    piece(out, t0, t1);
                } else {
                    let mid = (t0 + t1) / 2.0;
                    pending.push((mid, t1, level + 1));
                    pending.push((t0, mid, level + 1));
                }
            }
        };
        // Only the part on screen is cut up: a line passing near the eye
        // projects thousands of pixels long.
        let Some((t_in, t_out)) = clip_to_screen(a, [dx, dy]) else {
            return;
        };
        if dashed {
            // Dashes keep their phase from `a`, so they do not crawl as the
            // clipped part changes.
            let mut start = (t_in * length / DASH_PERIOD).floor() * DASH_PERIOD;
            while start < t_out * length {
                let end = (start + DASH_LENGTH).min(length);
                piece(
                    self,
                    start.max(t_in * length) / length,
                    end.min(t_out * length) / length,
                );
                start += DASH_PERIOD;
            }
        } else {
            piece(self, t_in, t_out);
        }
    }
}
