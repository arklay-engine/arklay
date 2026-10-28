//! The SDL_GPU hardware renderer for the gameplay scene and the default.
//!
//! The software rasterizer in [`crate::render`] stays authoritative: the
//! oracle, the goldens, `--capture`, `--trace`, `--stats` and every test draw
//! through it. This renderer is an interactive-only alternative selected by
//! default (or explicitly with `--renderer gpu`). It draws the same
//! [`GameplayScene`] at the window's
//! resolution (see [`crate::video`]): the scene target is the viewport's size
//! times `render_scale` percent, optionally multisampled (`msaa`), and is
//! filtered into the viewport when the two differ.
//!
//!
//! * Every TMD is expanded once into a static vertex buffer
//!   ([`scene::mesh_vertices`]) and stays resident while it is drawn. An
//!   upload unused for [`EVICT_AFTER_FRAMES`] scene frames is released; a
//!   room change ([`GpuRenderer::retire`]) releases at once whatever the new
//!   room's first frame does not draw, and leaving gameplay releases
//!   everything ([`GpuRenderer::release_residency`]). Indexed texture pages,
//!   palettes and RGBA images (backgrounds, mask, shadow and effect pages) are
//!   cached the same way.
//! * Per draw, only each object's view transform (the software's
//!   `RefProjection`), the latched lights and the tint/blend travel as
//!   uniforms; `mesh.vert` drops and culls whole packets like the software,
//!   lights each corner like `reference_vertex_shade` and projects with the
//!   software's linear depth, so lavapipe rasterizes the same pixels.
//!   `mesh.frag` looks up the CLUT row in the shader, with "blends under a
//!   blending packet" in the palette's alpha.
//! * Visibility between meshes is a depth buffer; opaque mesh texels write
//!   it and blending texels blend in a second pass. Against masks it is the
//!   painter's sort: a depth-only pass first keeps the nearest mask's key per
//!   pixel, and a mesh triangle or shadow whose key is at or behind it (an
//!   effect: behind it) is discarded (see [`scene::Sprites`]). The keys are
//!   per pixel, so with MSAA a mask's edge is not antialiased against what
//!   it hides.
//!
//! * A pack's HD replacements ([`crate::htexture`]) are looked up as an image
//!   or page is uploaded, from the hash of the pixels it would otherwise
//!   upload. A replaced image is sampled at its own resolution
//!   (`sprite.frag`'s `scale`); a replaced page keeps its indices for
//!   transparency and STP and takes each CLUT row's colours from a layer of
//!   an array texture (`mesh.frag`).
//!
//! The 2D drawing (menus, messages, fades, HD art) is the 2D pass's
//! ([`crate::draw2d`]): while the GPU renderer draws a frame, its
//! [`Framebuffer`] records each 2D primitive as an item of a list instead of
//! writing pixels, and the scene is an item of its own where the frame shows
//! it. The pass draws the items in order into a target at the scene's
//! resolution (`draw2d.frag`), which the composite scales into the window:
//! the original art is sampled per frame pixel, so it keeps its pixels,
//! while the scene, the pack's HD art and the examine viewer's model (which
//! the CPU projects and the pass first renders into a target of its own,
//! `ivm.vert` with `mesh.frag`) are drawn at the target's resolution. The
//! software 3D rasterizer is never reached from this backend.
//!
//! The shaders are SPIR-V only for now, so the renderer needs a Vulkan
//! driver. Device creation failing is not an error for the engine: it warns
//! and falls back to the software renderer.

mod gui;
pub mod scene;

use std::collections::HashMap;
use std::ffi::{CStr, c_void};
use std::sync::Arc;

use anyhow::{Result, bail};
use sdl3_sys::error::SDL_GetError;
use sdl3_sys::gpu::*;
use sdl3_sys::pixels::SDL_FColor;
use sdl3_sys::rect::SDL_Rect;
use sdl3_sys::video::{SDL_GetWindowSizeInPixels, SDL_Window};

use crate::draw2d::{self, DrawList};
use crate::htexture;
use crate::model::{Texture8, Tmd};
use crate::pack::Pack;
use crate::render::{Framebuffer, GameplayScene, root_position};
use crate::state::Image;
use crate::video;

use scene::{DrawUniforms, MeshVertex, SpriteRun, SpriteVertex};

const MESH_VERT: &[u8] = include_bytes!("shaders/mesh.vert.spv");
const MESH_FRAG: &[u8] = include_bytes!("shaders/mesh.frag.spv");
const SPRITE_VERT: &[u8] = include_bytes!("shaders/sprite.vert.spv");
const SPRITE_FRAG: &[u8] = include_bytes!("shaders/sprite.frag.spv");
const COMPOSITE_VERT: &[u8] = include_bytes!("shaders/composite.vert.spv");
const COMPOSITE_FRAG: &[u8] = include_bytes!("shaders/composite.frag.spv");
const DRAW2D_VERT: &[u8] = include_bytes!("shaders/draw2d.vert.spv");
const DRAW2D_FRAG: &[u8] = include_bytes!("shaders/draw2d.frag.spv");
const UI_FRAG: &[u8] = include_bytes!("shaders/ui.frag.spv");
const IVM_VERT: &[u8] = include_bytes!("shaders/ivm.vert.spv");

/// Scene frames an unused upload stays resident before it is released.
pub const EVICT_AFTER_FRAMES: u64 = 300;

/// The rows of `draw2d.frag`'s `lut` per recoloured item: each index's
/// [`htexture::Recolour`] scale (alpha 1 when the index draws), luma weight
/// and flat colour.
const LUT_ROWS: u32 = 3;
/// The largest accepted offscreen resolution multiplier.
pub const MAX_SCALE: u32 = 8;
/// The largest scene target side, whatever the window and `render_scale`.
const MAX_SCENE_SIDE: u32 = 8192;

/// How long a present that had no swapchain image waits, so a loop held
/// back only by vsync does not spin while the window is hidden.
const HIDDEN_PRESENT_WAIT: std::time::Duration = std::time::Duration::from_millis(4);

/// What [`GpuRenderer::composite`] presents.
enum Layer<'a> {
    /// An image, uploaded into `layer` first.
    Upload(&'a [u8]),
    /// The image already in `layer`.
    Kept,
    /// The 2D pass's `overlay_target`.
    Target,
}

/// How a pipeline blends into its target.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Blend {
    /// The fragment replaces the target.
    None,
    /// By the fragment's alpha. The target's alpha keeps the coverage, so a
    /// target cleared to transparent ends up premultiplied.
    Alpha,
    /// A premultiplied fragment over the target.
    Premultiplied,
}

/// One corner of a 2D pass item (`draw2d.vert`'s input, 96 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct ItemVertex {
    /// In frame pixels.
    position: [f32; 2],
    uv: [f32; 2],
    colour: [f32; 4],
    /// [`draw2d::Map::dst`].
    dst: [i32; 4],
    /// [`draw2d::Map::src`] and [`draw2d::Map::atlas`].
    source: [i32; 4],
    /// `[x0, y0, x1, y1]` in frame pixels.
    clip: [i32; 4],
    /// The mode, the map's flags `<< 8`; the first `lut` row.
    mode: [u32; 2],
    /// A recoloured item's original texels per unit of `uv`.
    index_frame: [f32; 2],
}

// SAFETY: only 4-byte fields, so no padding.
unsafe impl scene::Plain for ItemVertex {}

/// [`ItemVertex::mode`]s, as `draw2d.frag` reads them.
const MODE_SOLID: u32 = 0;
const MODE_SPRITE: u32 = 1;
const MODE_ART: u32 = 2;
const MODE_RECOLOURED: u32 = 3;
const MODE_PREMULTIPLIED: u32 = 4;

/// The atlas texture's fewest rows.
const MIN_ATLAS_ROWS: u32 = 256;

/// [`GpuRenderer::atlases`]' slots.
const FRAME_ATLAS: usize = 0;
const UI_ATLAS: usize = 1;
const GUI_ATLAS: usize = 2;

/// An atlas texture of the 2D pass.
#[derive(Clone, Copy)]
struct AtlasTexture {
    /// [`draw2d::ATLAS_WIDTH`] wide, `rows` high; null until used.
    texture: *mut SDL_GPUTexture,
    rows: u32,
    /// The [`draw2d::Atlas::revision`] it holds and the rows uploaded.
    revision: (u64, u32),
}

impl AtlasTexture {
    const EMPTY: Self = Self {
        texture: std::ptr::null_mut(),
        rows: 0,
        revision: (0, 0),
    };
}

/// What an item of the 2D pass samples besides the atlas.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ItemImage {
    None,
    Scene,
    /// The `model_targets` slot.
    Model(usize),
    /// The `overlay_images` key, and a recoloured item's `overlay_indices`
    /// key.
    Art(usize, Option<usize>),
}

/// An item's six vertices from `first`, and what it samples.
struct ItemDraw {
    image: ItemImage,
    first: u32,
}

const LAYER_WIDTH: u32 = 320;
const LAYER_HEIGHT: u32 = 240;

fn sdl_error() -> String {
    unsafe { CStr::from_ptr(SDL_GetError()) }
        .to_string_lossy()
        .into_owned()
}

/// The number of CLUT rows a page's palette holds, at least one.
fn page_row_count(texture: &Texture8) -> usize {
    texture
        .palettes
        .len()
        .div_ceil(crate::model::PALETTE_ROW_LEN)
        .max(1)
}

/// A compiled shader, released when dropped; a pipeline keeps what it needs.
struct Shader {
    device: *mut SDL_GPUDevice,
    shader: *mut SDL_GPUShader,
}

impl Drop for Shader {
    fn drop(&mut self) {
        unsafe { SDL_ReleaseGPUShader(self.device, self.shader) };
    }
}

/// A cache entry made by the scene being recorded, forgotten again if the
/// recording fails before its uploads run.
#[derive(Clone, Copy)]
enum Upload {
    Mesh(usize),
    Page(PageKey),
    Image(usize),
}

/// The identity of an uploaded page: the texture, the named replacement it
/// was drawn with, and the pack its content hashes came from. A page may be
/// drawn in more than one of those states, and each needs its own upload.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct PageKey {
    texture: usize,
    named: usize,
    pack: usize,
}

/// A cached upload and the frame it was last drawn in.
struct Resident<T> {
    value: T,
    fingerprint: u64,
    last_used: u64,
}

/// A mesh's static vertex buffer; `buffer` is null for a mesh with no
/// drawable packet.
#[derive(Clone, Copy)]
struct MeshBuffer {
    buffer: *mut SDL_GPUBuffer,
    vertex_count: u32,
    blend_start: u32,
    /// The CLUT rows its packets read, one bit each.
    rows: u32,
}

/// An indexed page: the `R8_UINT` index texture and its palette, and the HD
/// replacements of its CLUT rows (both null without one).
#[derive(Clone, Copy)]
struct PageTextures {
    indices: *mut SDL_GPUTexture,
    palette: *mut SDL_GPUTexture,
    /// One RGBA layer per replaced row.
    hd: *mut SDL_GPUTexture,
    /// Per row, `mesh.frag`'s layer and extents.
    hd_rows: *mut SDL_GPUTexture,
}

/// An uploaded RGBA image: the original or its HD replacement, with the
/// replacement's texels per original texel.
#[derive(Clone, Copy)]
struct ImageTexture {
    texture: *mut SDL_GPUTexture,
    scale: [f32; 2],
}

/// The renderer's pipelines.
struct Pipelines {
    /// Both mesh pipelines are unculled: `mesh.vert` culls whole packets
    /// like the software path (and leaves the mirror copy unculled).
    mesh_opaque: *mut SDL_GPUGraphicsPipeline,
    mesh_blend: *mut SDL_GPUGraphicsPipeline,
    /// The masks' keys into `mask_keys` (depth only).
    mask_keys: *mut SDL_GPUGraphicsPipeline,
    sprite: *mut SDL_GPUGraphicsPipeline,
    /// [`Pipelines::sprite`] writing depth: the F4 debug geometry.
    sprite_depth: *mut SDL_GPUGraphicsPipeline,
    /// The 2D pass's items into `overlay_target` or `ui_texture`.
    draw2d: *mut SDL_GPUGraphicsPipeline,
    /// The 2D pass's projected models into their [`ModelTarget`]s.
    model_opaque: *mut SDL_GPUGraphicsPipeline,
    model_blend: *mut SDL_GPUGraphicsPipeline,
    /// Present-only: built for the swapchain's format.
    composite: *mut SDL_GPUGraphicsPipeline,
    /// Present-only: a UI layer blended over the composite.
    ui: *mut SDL_GPUGraphicsPipeline,
    /// Present-only: the native GUI's recorded frame ([`gui`]).
    gui: *mut SDL_GPUGraphicsPipeline,
}

/// The target a 2D pass's model ([`draw2d::Kind::Model`]) renders into
/// before the pass samples `colour`.
struct ModelTarget {
    colour: *mut SDL_GPUTexture,
    /// The multisampled colour target; null without MSAA.
    samples: *mut SDL_GPUTexture,
    depth: *mut SDL_GPUTexture,
    size: (u32, u32),
}

/// A 2D pass model to draw: its page, its run of vertices in
/// `model_buffer` and the `model_targets` slot it renders into.
#[derive(Clone, Copy)]
struct ModelRun {
    page: PageTextures,
    first: u32,
    count: u32,
    slot: usize,
}

/// A GPU's self-description, each part `None` when the driver keeps it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GpuInfo {
    pub name: Option<String>,
    pub driver_version: Option<String>,
    /// SDL's name for the graphics API ("vulkan", "direct3d12", "metal").
    pub api: Option<String>,
}

/// What `device` reports of itself; the strings are copied out.
fn device_info(device: *mut SDL_GPUDevice) -> GpuInfo {
    let string = |props: sdl3_sys::properties::SDL_PropertiesID, name: *const std::ffi::c_char| {
        let value =
            unsafe { sdl3_sys::properties::SDL_GetStringProperty(props, name, std::ptr::null()) };
        (!value.is_null())
            .then(|| {
                unsafe { std::ffi::CStr::from_ptr(value) }
                    .to_string_lossy()
                    .trim()
                    .to_string()
            })
            .filter(|value| !value.is_empty())
    };
    let props = unsafe { SDL_GetGPUDeviceProperties(device) };
    let api = unsafe { SDL_GetGPUDeviceDriver(device) };
    GpuInfo {
        name: string(props, SDL_PROP_GPU_DEVICE_NAME_STRING),
        driver_version: string(props, SDL_PROP_GPU_DEVICE_DRIVER_VERSION_STRING),
        api: (!api.is_null()).then(|| {
            unsafe { std::ffi::CStr::from_ptr(api) }
                .to_string_lossy()
                .into_owned()
        }),
    }
}

/// The SDL_GPU device, its pipelines and the resident uploads.
pub struct GpuRenderer {
    device: *mut SDL_GPUDevice,
    /// The claimed window; null for an offscreen renderer.
    window: *mut SDL_Window,
    /// The scene target's size as a percentage of the viewport.
    render_scale: u32,
    /// The scene target's size in pixels.
    scene_size: (u32, u32),
    /// Samples per scene pixel; above one the scene draws into
    /// `scene_samples` and resolves into `scene_colour`.
    samples: SDL_GPUSampleCount,
    depth_format: SDL_GPUTextureFormat,
    pipelines: Pipelines,
    sampler: *mut SDL_GPUSampler,
    /// The composite's sampler: the layers' sharp upscale and the scene's
    /// filtering both build on bilinear taps.
    linear: *mut SDL_GPUSampler,
    scene_colour: *mut SDL_GPUTexture,
    /// The multisampled colour target; null without MSAA.
    scene_samples: *mut SDL_GPUTexture,
    scene_depth: *mut SDL_GPUTexture,
    /// The nearest mask's painter's key per scene pixel (see
    /// [`scene::Sprites`]), single-sampled at the scene's size.
    mask_keys: *mut SDL_GPUTexture,
    /// A 1x1 cleared `mask_keys` (no mask) for the draws outside the scene.
    no_mask_keys: *mut SDL_GPUTexture,
    /// An image presented as the frame: a frame drawn in software, an HD
    /// film frame.
    layer: *mut SDL_GPUTexture,
    /// The persistent staging buffer of `layer`, cycled per present.
    layer_transfer: *mut SDL_GPUTransferBuffer,
    /// `layer`'s size ([`GpuRenderer::ensure_layer`]).
    layer_size: (u32, u32),
    /// The [`VideoFrame::serial`](crate::hdfilm::VideoFrame::serial) of the
    /// image `layer` holds, so a film frame shown across several presents is
    /// uploaded once.
    layer_image: Option<u64>,
    /// The UI layer presented over the frame (the F1 debug menu at a wider
    /// aspect), premultiplied: the 2D pass's target for its list; null until
    /// one is presented.
    ui_texture: *mut SDL_GPUTexture,
    /// The UI layer's frame size, which it is fitted to the window by.
    ui_size: (u32, u32),
    /// `ui_texture`'s size in pixels.
    ui_target_size: (u32, u32),
    /// Whether the present being recorded draws `ui_texture`.
    ui_shown: bool,
    /// The native-resolution GUI layer ([`crate::gui::Canvas`]), drawn
    /// one-to-one over the whole window above everything else.
    native_texture: *mut SDL_GPUTexture,
    native_size: (u32, u32),
    /// The [`crate::gui::Canvas::serial`] `native_texture` holds.
    native_serial: u64,
    /// Whether the next present draws `native_texture`.
    native_shown: bool,
    /// The native GUI layer recorded for the GPU, drawn in its place.
    gui: gui::GuiLayer,
    sprite_buffer: *mut SDL_GPUBuffer,
    sprite_capacity: u32,
    /// The 2D pass's target ([`crate::draw2d`]): the presented frame, with
    /// the scene drawn into it; null until a frame is drawn.
    overlay_target: *mut SDL_GPUTexture,
    overlay_size: (u32, u32),
    /// The 2D pass's atlases ([`draw2d::Atlas`]): the frame's
    /// ([`FRAME_ATLAS`]) and the UI layer's ([`UI_ATLAS`]), and the native
    /// GUI's masks and pictures ([`GUI_ATLAS`]).
    atlases: [AtlasTexture; 3],
    overlay_buffer: *mut SDL_GPUBuffer,
    overlay_capacity: u32,
    /// One per drawn model item of the 2D pass, in order, kept for the next
    /// frame.
    model_targets: Vec<ModelTarget>,
    /// The overlay frame that last drew a model.
    model_frame: u64,
    model_buffer: *mut SDL_GPUBuffer,
    model_capacity: u32,
    /// The HD art's textures by [`htexture::Replacement`], kept
    /// alive so the key is not reused, and the presented frame each was
    /// last drawn in. Unlike the scene's uploads they outlive gameplay, so
    /// menus and title screens do not upload them every frame.
    overlay_images: HashMap<usize, (Arc<htexture::Replacement>, *mut SDL_GPUTexture, u64)>,
    /// The recoloured items' original page indices as `R8_UINT` textures,
    /// by [`htexture::Indices`], kept like `overlay_images`.
    overlay_indices: HashMap<usize, (Arc<htexture::Indices>, *mut SDL_GPUTexture, u64)>,
    /// The frame's recolourings, [`LUT_ROWS`] rows of 256 per recoloured
    /// item (`draw2d.frag`'s `lut`), and the rows it holds.
    overlay_lut: *mut SDL_GPUTexture,
    overlay_lut_rows: u32,
    /// Bound in place of the indices for an item that is not recoloured.
    no_indices: *mut SDL_GPUTexture,
    /// Bound in place of the image for an item that samples none.
    no_image: *mut SDL_GPUTexture,
    /// The presented frame count the 2D pass's eviction runs on.
    overlay_frame: u64,
    meshes: HashMap<usize, Resident<MeshBuffer>>,
    pages: HashMap<PageKey, Resident<PageTextures>>,
    /// The named replacement each page key names, kept alive so its address
    /// cannot be reused for a different replacement while the entry is
    /// cached (the key stores that address).
    page_keeps: HashMap<PageKey, Arc<htexture::Replacement>>,
    /// The named HD replacement each mesh page stem resolved to in the pack
    /// at `named_pack` (its address), so the lookup runs once per stem.
    named_pages: HashMap<String, Option<Arc<htexture::Replacement>>>,
    named_pack: usize,
    images: HashMap<usize, Resident<ImageTexture>>,
    /// Bound in place of a page's replacements when it has none.
    no_hd: *mut SDL_GPUTexture,
    no_hd_rows: *mut SDL_GPUTexture,
    frame: u64,
    /// Set by [`GpuRenderer::retire`]: the next eviction keeps only what the
    /// frame that follows drew.
    retiring: bool,
    /// The entries the scene being recorded created.
    fresh: Vec<Upload>,
    /// Whether the swapchain uses mailbox, which never waits for vsync; the
    /// caller's frame pacing ([`crate::pacer`]) then sets the present rate.
    mailbox: bool,
    /// Make the next [`GpuRenderer::record_scene`] fail, for the tests of
    /// what a failed scene does.
    #[cfg(test)]
    pub(crate) fail_next_scene: bool,
}

impl GpuRenderer {
    /// Create a device, claim `window` for it and build the pipelines.
    ///
    /// The scene target follows the window: [`crate::video::fit`]'s viewport
    /// of its client area, times `render_scale` percent, with `msaa` samples
    /// per pixel (or the most the device supports below that).
    ///
    /// # Safety
    ///
    /// `window` must be a live SDL window that outlives the renderer and is
    /// not claimed by another device or an SDL renderer.
    pub unsafe fn new(window: *mut SDL_Window, render_scale: u32, msaa: u32) -> Result<Self> {
        let mut renderer = Self::create(
            (LAYER_WIDTH, LAYER_HEIGHT),
            render_scale.clamp(
                *video::RENDER_SCALE_RANGE.start(),
                *video::RENDER_SCALE_RANGE.end(),
            ),
            msaa,
        )?;
        if !unsafe { SDL_ClaimWindowForGPUDevice(renderer.device, window) } {
            bail!("SDL_ClaimWindowForGPUDevice failed: {}", sdl_error());
        }
        renderer.window = window;
        renderer.follow_window()?;
        // SDL starts a claimed window in FIFO (vsync) presentation. The
        // caller applies `set_vsync` for an explicit off preference.
        let format = unsafe { SDL_GetGPUSwapchainTextureFormat(renderer.device, window) };
        renderer.pipelines.composite = renderer.composite_pipeline(format)?;
        renderer.pipelines.ui = renderer.ui_pipeline(format)?;
        renderer.pipelines.gui = renderer.gui_pipeline(format)?;
        Ok(renderer)
    }

    /// Select vsync (FIFO) or the lowest-latency present mode the window
    /// supports. Returns false when SDL rejects every candidate, in which
    /// case the current mode stays in force.
    pub fn set_vsync(&mut self, vsync: bool) -> bool {
        if self.window.is_null() {
            return false;
        }
        // Without vsync, prefer immediate present (tearing) and fall back to
        // mailbox; both never wait for the compositor, and the frame pacer
        // still limits presents to `frame_rate`.
        let modes = if vsync {
            [SDL_GPU_PRESENTMODE_VSYNC; 3]
        } else {
            [
                SDL_GPU_PRESENTMODE_IMMEDIATE,
                SDL_GPU_PRESENTMODE_MAILBOX,
                SDL_GPU_PRESENTMODE_VSYNC,
            ]
        };
        let count = if vsync { 1 } else { modes.len() };
        for &mode in &modes[..count] {
            if unsafe {
                SDL_WindowSupportsGPUPresentMode(self.device, self.window, mode)
                    && SDL_SetGPUSwapchainParameters(
                        self.device,
                        self.window,
                        SDL_GPU_SWAPCHAINCOMPOSITION_SDR,
                        mode,
                    )
            } {
                self.mailbox = mode == SDL_GPU_PRESENTMODE_MAILBOX;
                return true;
            }
        }
        false
    }

    /// Give the window back once in-flight work is done, so an SDL renderer
    /// can claim it; presenting fails from then on.
    pub fn release_window(&mut self) {
        if !self.window.is_null() {
            unsafe {
                SDL_WaitForGPUIdle(self.device);
                SDL_ReleaseWindowFromGPUDevice(self.device, self.window);
            }
            self.window = std::ptr::null_mut();
        }
    }

    /// A renderer with no window, for tests and tools: it records scenes and
    /// reads them back with [`GpuRenderer::read_scene`].
    /// Its scene target is `scale` times 320x240 (clamped to
    /// `1..=MAX_SCALE`), single-sampled.
    pub fn offscreen(scale: u32) -> Result<Self> {
        let scale = scale.clamp(1, MAX_SCALE);
        Self::create((LAYER_WIDTH * scale, LAYER_HEIGHT * scale), 100, 1)
    }

    /// What a device on this machine reports of itself, from one created
    /// for the purpose; `None` when no device can be created.
    pub fn probe() -> Option<GpuInfo> {
        let device =
            unsafe { SDL_CreateGPUDevice(SDL_GPU_SHADERFORMAT_SPIRV, false, std::ptr::null()) };
        if device.is_null() {
            return None;
        }
        let info = device_info(device);
        unsafe { SDL_DestroyGPUDevice(device) };
        Some(info)
    }

    /// What this renderer's device reports of itself.
    pub fn info(&self) -> GpuInfo {
        device_info(self.device)
    }

    fn create(scene_size: (u32, u32), render_scale: u32, msaa: u32) -> Result<Self> {
        let device =
            unsafe { SDL_CreateGPUDevice(SDL_GPU_SHADERFORMAT_SPIRV, false, std::ptr::null()) };
        if device.is_null() {
            bail!("SDL_CreateGPUDevice failed: {}", sdl_error());
        }
        let mut renderer = Self {
            device,
            window: std::ptr::null_mut(),
            render_scale,
            scene_size,
            samples: SDL_GPU_SAMPLECOUNT_1,
            depth_format: SDL_GPU_TEXTUREFORMAT_D24_UNORM,
            pipelines: Pipelines {
                mesh_opaque: std::ptr::null_mut(),
                mesh_blend: std::ptr::null_mut(),
                mask_keys: std::ptr::null_mut(),
                sprite: std::ptr::null_mut(),
                sprite_depth: std::ptr::null_mut(),
                draw2d: std::ptr::null_mut(),
                model_opaque: std::ptr::null_mut(),
                model_blend: std::ptr::null_mut(),
                composite: std::ptr::null_mut(),
                ui: std::ptr::null_mut(),
                gui: std::ptr::null_mut(),
            },
            sampler: std::ptr::null_mut(),
            linear: std::ptr::null_mut(),
            scene_colour: std::ptr::null_mut(),
            scene_samples: std::ptr::null_mut(),
            scene_depth: std::ptr::null_mut(),
            mask_keys: std::ptr::null_mut(),
            no_mask_keys: std::ptr::null_mut(),
            layer: std::ptr::null_mut(),
            layer_transfer: std::ptr::null_mut(),
            layer_size: (0, 0),
            layer_image: None,
            ui_texture: std::ptr::null_mut(),
            ui_size: (0, 0),
            ui_target_size: (0, 0),
            ui_shown: false,
            native_texture: std::ptr::null_mut(),
            native_size: (0, 0),
            native_serial: 0,
            native_shown: false,
            gui: gui::GuiLayer::default(),
            sprite_buffer: std::ptr::null_mut(),
            sprite_capacity: 0,
            overlay_target: std::ptr::null_mut(),
            overlay_size: (0, 0),
            atlases: [AtlasTexture::EMPTY; 3],
            overlay_buffer: std::ptr::null_mut(),
            overlay_capacity: 0,
            model_targets: Vec::new(),
            model_frame: 0,
            model_buffer: std::ptr::null_mut(),
            model_capacity: 0,
            overlay_images: HashMap::new(),
            overlay_indices: HashMap::new(),
            overlay_lut: std::ptr::null_mut(),
            overlay_lut_rows: 0,
            no_indices: std::ptr::null_mut(),
            no_image: std::ptr::null_mut(),
            overlay_frame: 0,
            meshes: HashMap::new(),
            pages: HashMap::new(),
            page_keeps: HashMap::new(),
            named_pages: HashMap::new(),
            named_pack: 0,
            images: HashMap::new(),
            no_hd: std::ptr::null_mut(),
            no_hd_rows: std::ptr::null_mut(),
            frame: 0,
            retiring: false,
            fresh: Vec::new(),
            mailbox: false,
            #[cfg(test)]
            fail_next_scene: false,
        };
        // From here a failure drops `renderer`, which releases whatever was
        // created so far.
        renderer.depth_format = if unsafe {
            SDL_GPUTextureSupportsFormat(
                device,
                SDL_GPU_TEXTUREFORMAT_D32_FLOAT,
                SDL_GPU_TEXTURETYPE_2D,
                SDL_GPU_TEXTUREUSAGE_DEPTH_STENCIL_TARGET,
            )
        } {
            SDL_GPU_TEXTUREFORMAT_D32_FLOAT
        } else {
            SDL_GPU_TEXTUREFORMAT_D24_UNORM
        };
        renderer.samples = renderer.sample_count(msaa);
        renderer.create_scene_targets()?;
        renderer.ensure_layer(LAYER_WIDTH, LAYER_HEIGHT)?;
        renderer.sampler = renderer.create_sampler(SDL_GPU_FILTER_NEAREST)?;
        renderer.linear = renderer.create_sampler(SDL_GPU_FILTER_LINEAR)?;
        renderer.no_hd = renderer.array_texture(SDL_GPU_TEXTUREFORMAT_R8G8B8A8_UNORM, 1, 1, 1)?;
        renderer.no_hd_rows = renderer.texture(
            SDL_GPU_TEXTUREFORMAT_R16G16B16A16_UINT,
            2,
            1,
            SDL_GPU_TEXTUREUSAGE_SAMPLER,
        )?;
        renderer.no_indices = renderer.texture(
            SDL_GPU_TEXTUREFORMAT_R8_UINT,
            1,
            1,
            SDL_GPU_TEXTUREUSAGE_SAMPLER,
        )?;
        renderer.no_image = renderer.texture(
            SDL_GPU_TEXTUREFORMAT_R8G8B8A8_UNORM,
            1,
            1,
            SDL_GPU_TEXTUREUSAGE_SAMPLER,
        )?;
        renderer.ensure_overlay_lut(LUT_ROWS)?;
        renderer.no_mask_keys = renderer.mask_key_texture(1, 1)?;
        renderer.build_scene_pipelines()?;
        renderer.clear_scene()?;
        Ok(renderer)
    }

    /// (Re)create `layer` and its staging buffer at `width` x `height`
    /// unless they already have that size. The released ones stay alive
    /// until the presents still reading them finish.
    fn ensure_layer(&mut self, width: u32, height: u32) -> Result<()> {
        if self.layer_size == (width, height) && !self.layer_transfer.is_null() {
            return Ok(());
        }
        unsafe {
            if !self.layer.is_null() {
                SDL_ReleaseGPUTexture(self.device, self.layer);
            }
            if !self.layer_transfer.is_null() {
                SDL_ReleaseGPUTransferBuffer(self.device, self.layer_transfer);
            }
        }
        self.layer = std::ptr::null_mut();
        self.layer_transfer = std::ptr::null_mut();
        self.layer_size = (0, 0);
        self.layer_image = None;
        self.layer = self.texture(
            SDL_GPU_TEXTUREFORMAT_R8G8B8A8_UNORM,
            width,
            height,
            SDL_GPU_TEXTUREUSAGE_SAMPLER,
        )?;
        let transfer_info = SDL_GPUTransferBufferCreateInfo {
            usage: SDL_GPU_TRANSFERBUFFERUSAGE_UPLOAD,
            size: width * height * 4,
            ..Default::default()
        };
        self.layer_transfer = unsafe { SDL_CreateGPUTransferBuffer(self.device, &transfer_info) };
        if self.layer_transfer.is_null() {
            bail!("SDL_CreateGPUTransferBuffer failed: {}", sdl_error());
        }
        self.layer_size = (width, height);
        Ok(())
    }

    /// The most samples up to `msaa` both scene formats support.
    fn sample_count(&self, msaa: u32) -> SDL_GPUSampleCount {
        let counts = [
            (8, SDL_GPU_SAMPLECOUNT_8),
            (4, SDL_GPU_SAMPLECOUNT_4),
            (2, SDL_GPU_SAMPLECOUNT_2),
        ];
        for (samples, count) in counts {
            if samples <= msaa
                && unsafe {
                    SDL_GPUTextureSupportsSampleCount(
                        self.device,
                        SDL_GPU_TEXTUREFORMAT_R8G8B8A8_UNORM,
                        count,
                    ) && SDL_GPUTextureSupportsSampleCount(self.device, self.depth_format, count)
                }
            {
                return count;
            }
        }
        SDL_GPU_SAMPLECOUNT_1
    }

    fn create_sampler(&self, filter: SDL_GPUFilter) -> Result<*mut SDL_GPUSampler> {
        let info = SDL_GPUSamplerCreateInfo {
            min_filter: filter,
            mag_filter: filter,
            mipmap_mode: SDL_GPU_SAMPLERMIPMAPMODE_NEAREST,
            address_mode_u: SDL_GPU_SAMPLERADDRESSMODE_CLAMP_TO_EDGE,
            address_mode_v: SDL_GPU_SAMPLERADDRESSMODE_CLAMP_TO_EDGE,
            address_mode_w: SDL_GPU_SAMPLERADDRESSMODE_CLAMP_TO_EDGE,
            ..Default::default()
        };
        let sampler = unsafe { SDL_CreateGPUSampler(self.device, &info) };
        if sampler.is_null() {
            bail!("SDL_CreateGPUSampler failed: {}", sdl_error());
        }
        Ok(sampler)
    }

    /// Create the scene's colour, multisample and depth targets at
    /// `scene_size`.
    fn create_scene_targets(&mut self) -> Result<()> {
        let (width, height) = self.scene_size;
        self.scene_colour = self.texture(
            SDL_GPU_TEXTUREFORMAT_R8G8B8A8_UNORM,
            width,
            height,
            SDL_GPUTextureUsageFlags(
                SDL_GPU_TEXTUREUSAGE_SAMPLER.0 | SDL_GPU_TEXTUREUSAGE_COLOR_TARGET.0,
            ),
        )?;
        if self.samples != SDL_GPU_SAMPLECOUNT_1 {
            self.scene_samples = self.target(
                SDL_GPU_TEXTUREFORMAT_R8G8B8A8_UNORM,
                width,
                height,
                SDL_GPU_TEXTUREUSAGE_COLOR_TARGET,
                self.samples,
            )?;
        }
        self.scene_depth = self.target(
            self.depth_format,
            width,
            height,
            SDL_GPU_TEXTUREUSAGE_DEPTH_STENCIL_TARGET,
            self.samples,
        )?;
        self.mask_keys = self.mask_key_texture(width, height)?;
        Ok(())
    }

    /// Release the scene targets. SDL keeps a texture alive until the work
    /// already submitted against it is done.
    fn release_scene_targets(&mut self) {
        for texture in [
            &mut self.scene_colour,
            &mut self.scene_samples,
            &mut self.scene_depth,
            &mut self.mask_keys,
        ] {
            if !texture.is_null() {
                unsafe { SDL_ReleaseGPUTexture(self.device, *texture) };
                *texture = std::ptr::null_mut();
            }
        }
    }

    /// Resize the scene targets to the window's current viewport times
    /// `render_scale`; a renderer with no window keeps its size.
    fn follow_window(&mut self) -> Result<()> {
        if self.window.is_null() {
            return Ok(());
        }
        let (mut width, mut height) = (0, 0);
        if !unsafe { SDL_GetWindowSizeInPixels(self.window, &mut width, &mut height) } {
            bail!("SDL_GetWindowSizeInPixels failed: {}", sdl_error());
        }
        // The scene covers the whole 4:3 frame a wider aspect's band is
        // cropped from.
        let band = video::shown(LAYER_WIDTH, LAYER_HEIGHT, video::aspect().centred());
        let fit = video::fit_rows(width.max(1) as u32, height.max(1) as u32, band.height);
        let (_, _, frame_w, frame_h) = video::frame_viewport(fit, band, LAYER_HEIGHT);
        let viewport = video::Viewport {
            x: 0,
            y: 0,
            width: frame_w.round().max(1.0) as u32,
            height: frame_h.round().max(1.0) as u32,
        };
        let (width, height) = video::scene_size(viewport, self.render_scale);
        let size = (width.min(MAX_SCENE_SIDE), height.min(MAX_SCENE_SIDE));
        if size == self.scene_size && !self.scene_colour.is_null() {
            return Ok(());
        }
        self.release_scene_targets();
        self.scene_size = size;
        self.create_scene_targets()?;
        self.clear_scene()
    }

    /// The scene target's size in pixels.
    pub fn scene_size(&self) -> (u32, u32) {
        self.scene_size
    }

    /// A sampled D16 target for the masks' keys ([`scene::MASK_KEY_RANGE`]).
    fn mask_key_texture(&self, width: u32, height: u32) -> Result<*mut SDL_GPUTexture> {
        self.texture(
            SDL_GPU_TEXTUREFORMAT_D16_UNORM,
            width,
            height,
            SDL_GPUTextureUsageFlags(
                SDL_GPU_TEXTUREUSAGE_SAMPLER.0 | SDL_GPU_TEXTUREUSAGE_DEPTH_STENCIL_TARGET.0,
            ),
        )
    }

    fn texture(
        &self,
        format: SDL_GPUTextureFormat,
        width: u32,
        height: u32,
        usage: SDL_GPUTextureUsageFlags,
    ) -> Result<*mut SDL_GPUTexture> {
        self.target(format, width, height, usage, SDL_GPU_SAMPLECOUNT_1)
    }

    /// A sampled 2D array texture of `layers` layers.
    fn array_texture(
        &self,
        format: SDL_GPUTextureFormat,
        width: u32,
        height: u32,
        layers: u32,
    ) -> Result<*mut SDL_GPUTexture> {
        let info = SDL_GPUTextureCreateInfo {
            r#type: SDL_GPU_TEXTURETYPE_2D_ARRAY,
            format,
            usage: SDL_GPU_TEXTUREUSAGE_SAMPLER,
            width,
            height,
            layer_count_or_depth: layers,
            num_levels: 1,
            sample_count: SDL_GPU_SAMPLECOUNT_1,
            ..Default::default()
        };
        let texture = unsafe { SDL_CreateGPUTexture(self.device, &info) };
        if texture.is_null() {
            bail!("SDL_CreateGPUTexture failed: {}", sdl_error());
        }
        Ok(texture)
    }

    /// [`GpuRenderer::texture`] with `samples` per pixel.
    fn target(
        &self,
        format: SDL_GPUTextureFormat,
        width: u32,
        height: u32,
        usage: SDL_GPUTextureUsageFlags,
        samples: SDL_GPUSampleCount,
    ) -> Result<*mut SDL_GPUTexture> {
        let info = SDL_GPUTextureCreateInfo {
            r#type: SDL_GPU_TEXTURETYPE_2D,
            format,
            usage,
            width,
            height,
            layer_count_or_depth: 1,
            num_levels: 1,
            sample_count: samples,
            ..Default::default()
        };
        let texture = unsafe { SDL_CreateGPUTexture(self.device, &info) };
        if texture.is_null() {
            bail!("SDL_CreateGPUTexture failed: {}", sdl_error());
        }
        Ok(texture)
    }

    fn shader(
        &self,
        code: &[u8],
        stage: SDL_GPUShaderStage,
        samplers: u32,
        uniforms: u32,
    ) -> Result<Shader> {
        let info = SDL_GPUShaderCreateInfo {
            code_size: code.len(),
            code: code.as_ptr(),
            entrypoint: c"main".as_ptr(),
            format: SDL_GPU_SHADERFORMAT_SPIRV,
            stage,
            num_samplers: samplers,
            num_uniform_buffers: uniforms,
            ..Default::default()
        };
        let shader = unsafe { SDL_CreateGPUShader(self.device, &info) };
        if shader.is_null() {
            bail!("SDL_CreateGPUShader failed: {}", sdl_error());
        }
        Ok(Shader {
            device: self.device,
            shader,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn pipeline(
        &self,
        vertex: *mut SDL_GPUShader,
        fragment: *mut SDL_GPUShader,
        stride: u32,
        attributes: &[SDL_GPUVertexAttribute],
        cull: SDL_GPUCullMode,
        blend: Blend,
        depth: Option<(SDL_GPUTextureFormat, bool)>,
        colour_format: Option<SDL_GPUTextureFormat>,
        samples: SDL_GPUSampleCount,
    ) -> Result<*mut SDL_GPUGraphicsPipeline> {
        let buffer = SDL_GPUVertexBufferDescription {
            slot: 0,
            pitch: stride,
            input_rate: SDL_GPU_VERTEXINPUTRATE_VERTEX,
            instance_step_rate: 0,
        };
        let blend_state = match blend {
            Blend::None => SDL_GPUColorTargetBlendState::default(),
            Blend::Alpha | Blend::Premultiplied => SDL_GPUColorTargetBlendState {
                src_color_blendfactor: if blend == Blend::Alpha {
                    SDL_GPU_BLENDFACTOR_SRC_ALPHA
                } else {
                    SDL_GPU_BLENDFACTOR_ONE
                },
                dst_color_blendfactor: SDL_GPU_BLENDFACTOR_ONE_MINUS_SRC_ALPHA,
                color_blend_op: SDL_GPU_BLENDOP_ADD,
                src_alpha_blendfactor: SDL_GPU_BLENDFACTOR_ONE,
                dst_alpha_blendfactor: SDL_GPU_BLENDFACTOR_ONE_MINUS_SRC_ALPHA,
                alpha_blend_op: SDL_GPU_BLENDOP_ADD,
                enable_blend: true,
                ..Default::default()
            },
        };
        let colour_target = SDL_GPUColorTargetDescription {
            format: colour_format.unwrap_or(SDL_GPU_TEXTUREFORMAT_INVALID),
            blend_state,
        };
        let (depth_format, depth_stencil_state) = match depth {
            Some((format, write)) => (
                format,
                SDL_GPUDepthStencilState {
                    compare_op: SDL_GPU_COMPAREOP_LESS_OR_EQUAL,
                    enable_depth_test: true,
                    enable_depth_write: write,
                    ..Default::default()
                },
            ),
            None => (
                SDL_GPU_TEXTUREFORMAT_INVALID,
                SDL_GPUDepthStencilState::default(),
            ),
        };
        let info = SDL_GPUGraphicsPipelineCreateInfo {
            vertex_shader: vertex,
            fragment_shader: fragment,
            vertex_input_state: SDL_GPUVertexInputState {
                vertex_buffer_descriptions: if attributes.is_empty() {
                    std::ptr::null()
                } else {
                    &buffer
                },
                num_vertex_buffers: u32::from(!attributes.is_empty()),
                vertex_attributes: attributes.as_ptr(),
                num_vertex_attributes: attributes.len() as u32,
            },
            primitive_type: SDL_GPU_PRIMITIVETYPE_TRIANGLELIST,
            rasterizer_state: SDL_GPURasterizerState {
                fill_mode: SDL_GPU_FILLMODE_FILL,
                cull_mode: cull,
                // The software path keeps a packet whose screen-space area is
                // positive in its Y-down frame: clockwise on screen.
                front_face: SDL_GPU_FRONTFACE_CLOCKWISE,
                enable_depth_clip: true,
                ..Default::default()
            },
            multisample_state: SDL_GPUMultisampleState {
                sample_count: samples,
                ..Default::default()
            },
            depth_stencil_state,
            target_info: SDL_GPUGraphicsPipelineTargetInfo {
                color_target_descriptions: if colour_format.is_some() {
                    &colour_target
                } else {
                    std::ptr::null()
                },
                num_color_targets: u32::from(colour_format.is_some()),
                depth_stencil_format: depth_format,
                has_depth_stencil_target: depth.is_some(),
                ..Default::default()
            },
            ..Default::default()
        };
        let pipeline = unsafe { SDL_CreateGPUGraphicsPipeline(self.device, &info) };
        if pipeline.is_null() {
            bail!("SDL_CreateGPUGraphicsPipeline failed: {}", sdl_error());
        }
        Ok(pipeline)
    }

    fn build_scene_pipelines(&mut self) -> Result<()> {
        let depth_format = self.depth_format;
        let attribute = |location: u32, format: SDL_GPUVertexElementFormat, offset: u32| {
            SDL_GPUVertexAttribute {
                location,
                buffer_slot: 0,
                format,
                offset,
            }
        };
        let mesh_attributes = [
            attribute(0, SDL_GPU_VERTEXELEMENTFORMAT_FLOAT3, 0),
            attribute(1, SDL_GPU_VERTEXELEMENTFORMAT_FLOAT3, 12),
            attribute(2, SDL_GPU_VERTEXELEMENTFORMAT_FLOAT3, 24),
            attribute(3, SDL_GPU_VERTEXELEMENTFORMAT_FLOAT3, 36),
            attribute(4, SDL_GPU_VERTEXELEMENTFORMAT_FLOAT3, 48),
            attribute(5, SDL_GPU_VERTEXELEMENTFORMAT_FLOAT2, 60),
            attribute(6, SDL_GPU_VERTEXELEMENTFORMAT_FLOAT3, 68),
            attribute(7, SDL_GPU_VERTEXELEMENTFORMAT_UINT, 80),
        ];
        let sprite_attributes = [
            attribute(0, SDL_GPU_VERTEXELEMENTFORMAT_FLOAT4, 0),
            attribute(1, SDL_GPU_VERTEXELEMENTFORMAT_FLOAT2, 16),
            attribute(2, SDL_GPU_VERTEXELEMENTFORMAT_FLOAT4, 24),
            attribute(3, SDL_GPU_VERTEXELEMENTFORMAT_UINT, 40),
            attribute(4, SDL_GPU_VERTEXELEMENTFORMAT_FLOAT, 44),
        ];
        let mesh_stride = std::mem::size_of::<MeshVertex>() as u32;
        let sprite_stride = std::mem::size_of::<SpriteVertex>() as u32;
        let colour = SDL_GPU_TEXTUREFORMAT_R8G8B8A8_UNORM;

        let mesh_vert = self.shader(MESH_VERT, SDL_GPU_SHADERSTAGE_VERTEX, 0, 1)?;
        let mesh_frag = self.shader(MESH_FRAG, SDL_GPU_SHADERSTAGE_FRAGMENT, 5, 1)?;
        let sprite_vert = self.shader(SPRITE_VERT, SDL_GPU_SHADERSTAGE_VERTEX, 0, 0)?;
        let sprite_frag = self.shader(SPRITE_FRAG, SDL_GPU_SHADERSTAGE_FRAGMENT, 2, 1)?;
        let mesh = |this: &Self, blend, write| {
            this.pipeline(
                mesh_vert.shader,
                mesh_frag.shader,
                mesh_stride,
                &mesh_attributes,
                SDL_GPU_CULLMODE_NONE,
                blend,
                Some((depth_format, write)),
                Some(colour),
                this.samples,
            )
        };
        let sprite = |this: &Self, depth, colour: Option<SDL_GPUTextureFormat>, samples| {
            this.pipeline(
                sprite_vert.shader,
                sprite_frag.shader,
                sprite_stride,
                &sprite_attributes,
                SDL_GPU_CULLMODE_NONE,
                if colour.is_some() {
                    Blend::Alpha
                } else {
                    Blend::None
                },
                Some(depth),
                colour,
                samples,
            )
        };
        // Each pipeline is stored as soon as it exists, so a later failure
        // still releases it (via `Drop`).
        self.pipelines.mesh_opaque = mesh(self, Blend::None, true)?;
        self.pipelines.mesh_blend = mesh(self, Blend::Alpha, false)?;
        self.pipelines.mask_keys = sprite(
            self,
            (SDL_GPU_TEXTUREFORMAT_D16_UNORM, true),
            None,
            SDL_GPU_SAMPLECOUNT_1,
        )?;
        self.pipelines.sprite = sprite(self, (depth_format, false), Some(colour), self.samples)?;
        self.pipelines.sprite_depth =
            sprite(self, (depth_format, true), Some(colour), self.samples)?;
        let model_attributes = [
            attribute(0, SDL_GPU_VERTEXELEMENTFORMAT_FLOAT4, 0),
            attribute(1, SDL_GPU_VERTEXELEMENTFORMAT_FLOAT2, 16),
            attribute(2, SDL_GPU_VERTEXELEMENTFORMAT_FLOAT3, 24),
            attribute(3, SDL_GPU_VERTEXELEMENTFORMAT_UINT, 36),
        ];
        let model_stride = std::mem::size_of::<htexture::ModelVertex>() as u32;
        let ivm_vert = self.shader(IVM_VERT, SDL_GPU_SHADERSTAGE_VERTEX, 0, 0)?;
        // The CPU already dropped the back faces.
        let model = |this: &Self, blend, write| {
            this.pipeline(
                ivm_vert.shader,
                mesh_frag.shader,
                model_stride,
                &model_attributes,
                SDL_GPU_CULLMODE_NONE,
                blend,
                Some((depth_format, write)),
                Some(colour),
                this.samples,
            )
        };
        self.pipelines.model_opaque = model(self, Blend::None, true)?;
        self.pipelines.model_blend = model(self, Blend::Alpha, false)?;
        let item_attributes = [
            attribute(0, SDL_GPU_VERTEXELEMENTFORMAT_FLOAT2, 0),
            attribute(1, SDL_GPU_VERTEXELEMENTFORMAT_FLOAT2, 8),
            attribute(2, SDL_GPU_VERTEXELEMENTFORMAT_FLOAT4, 16),
            attribute(3, SDL_GPU_VERTEXELEMENTFORMAT_INT4, 32),
            attribute(4, SDL_GPU_VERTEXELEMENTFORMAT_INT4, 48),
            attribute(5, SDL_GPU_VERTEXELEMENTFORMAT_INT4, 64),
            attribute(6, SDL_GPU_VERTEXELEMENTFORMAT_UINT2, 80),
            attribute(7, SDL_GPU_VERTEXELEMENTFORMAT_FLOAT2, 88),
        ];
        let draw2d_vert = self.shader(DRAW2D_VERT, SDL_GPU_SHADERSTAGE_VERTEX, 0, 1)?;
        let draw2d_frag = self.shader(DRAW2D_FRAG, SDL_GPU_SHADERSTAGE_FRAGMENT, 4, 1)?;
        self.pipelines.draw2d = self.pipeline(
            draw2d_vert.shader,
            draw2d_frag.shader,
            std::mem::size_of::<ItemVertex>() as u32,
            &item_attributes,
            SDL_GPU_CULLMODE_NONE,
            Blend::Premultiplied,
            None,
            Some(colour),
            SDL_GPU_SAMPLECOUNT_1,
        )?;
        Ok(())
    }

    fn composite_pipeline(
        &self,
        format: SDL_GPUTextureFormat,
    ) -> Result<*mut SDL_GPUGraphicsPipeline> {
        let vertex = self.shader(COMPOSITE_VERT, SDL_GPU_SHADERSTAGE_VERTEX, 0, 0)?;
        let fragment = self.shader(COMPOSITE_FRAG, SDL_GPU_SHADERSTAGE_FRAGMENT, 1, 0)?;
        self.pipeline(
            vertex.shader,
            fragment.shader,
            0,
            &[],
            SDL_GPU_CULLMODE_NONE,
            Blend::None,
            None,
            Some(format),
            SDL_GPU_SAMPLECOUNT_1,
        )
    }

    fn ui_pipeline(&self, format: SDL_GPUTextureFormat) -> Result<*mut SDL_GPUGraphicsPipeline> {
        let vertex = self.shader(COMPOSITE_VERT, SDL_GPU_SHADERSTAGE_VERTEX, 0, 0)?;
        let fragment = self.shader(UI_FRAG, SDL_GPU_SHADERSTAGE_FRAGMENT, 1, 0)?;
        self.pipeline(
            vertex.shader,
            fragment.shader,
            0,
            &[],
            SDL_GPU_CULLMODE_NONE,
            Blend::Alpha,
            None,
            Some(format),
            SDL_GPU_SAMPLECOUNT_1,
        )
    }

    fn command_buffer(&self) -> Result<*mut SDL_GPUCommandBuffer> {
        let command = unsafe { SDL_AcquireGPUCommandBuffer(self.device) };
        if command.is_null() {
            bail!("SDL_AcquireGPUCommandBuffer failed: {}", sdl_error());
        }
        Ok(command)
    }

    fn submit(command: *mut SDL_GPUCommandBuffer) -> Result<()> {
        if !unsafe { SDL_SubmitGPUCommandBuffer(command) } {
            bail!("SDL_SubmitGPUCommandBuffer failed: {}", sdl_error());
        }
        Ok(())
    }

    /// Clear the scene target to black so a composite before the first scene
    /// shows nothing under the overlays.
    fn clear_scene(&self) -> Result<()> {
        let command = self.command_buffer()?;
        let target = SDL_GPUColorTargetInfo {
            texture: self.scene_colour,
            clear_color: SDL_FColor {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
            load_op: SDL_GPU_LOADOP_CLEAR,
            store_op: SDL_GPU_STOREOP_STORE,
            ..Default::default()
        };
        let pass = unsafe { SDL_BeginGPURenderPass(command, &target, 1, std::ptr::null()) };
        if !pass.is_null() {
            unsafe { SDL_EndGPURenderPass(pass) };
        }
        // No mask anywhere.
        let keys = SDL_GPUDepthStencilTargetInfo {
            texture: self.no_mask_keys,
            clear_depth: 1.0,
            load_op: SDL_GPU_LOADOP_CLEAR,
            store_op: SDL_GPU_STOREOP_STORE,
            stencil_load_op: SDL_GPU_LOADOP_DONT_CARE,
            stencil_store_op: SDL_GPU_STOREOP_DONT_CARE,
            ..Default::default()
        };
        let pass = unsafe { SDL_BeginGPURenderPass(command, std::ptr::null(), 0, &keys) };
        if pass.is_null() {
            let error = sdl_error();
            // Submit what was recorded; the failure is the error reported.
            let _ = Self::submit(command);
            bail!("SDL_BeginGPURenderPass failed: {error}");
        }
        unsafe { SDL_EndGPURenderPass(pass) };
        Self::submit(command)
    }

    /// Stage `bytes` in a transient transfer buffer.
    fn stage(&self, bytes: &[u8]) -> Result<*mut SDL_GPUTransferBuffer> {
        let info = SDL_GPUTransferBufferCreateInfo {
            usage: SDL_GPU_TRANSFERBUFFERUSAGE_UPLOAD,
            size: bytes.len() as u32,
            ..Default::default()
        };
        let transfer = unsafe { SDL_CreateGPUTransferBuffer(self.device, &info) };
        if transfer.is_null() {
            bail!("SDL_CreateGPUTransferBuffer failed: {}", sdl_error());
        }
        let mapped = unsafe { SDL_MapGPUTransferBuffer(self.device, transfer, false) };
        if mapped.is_null() {
            unsafe { SDL_ReleaseGPUTransferBuffer(self.device, transfer) };
            bail!("SDL_MapGPUTransferBuffer failed: {}", sdl_error());
        }
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), mapped.cast::<u8>(), bytes.len());
            SDL_UnmapGPUTransferBuffer(self.device, transfer);
        }
        Ok(transfer)
    }

    /// Record an upload of a whole texture; the staging buffer is released
    /// once the copy has executed.
    fn upload_texture(
        &self,
        copy: *mut SDL_GPUCopyPass,
        texture: *mut SDL_GPUTexture,
        width: u32,
        height: u32,
        bytes: &[u8],
    ) -> Result<()> {
        self.upload_layer(copy, texture, 0, width, height, bytes)
    }

    /// [`GpuRenderer::upload_texture`] into the top-left `width` x `height`
    /// of an array texture's `layer`.
    fn upload_layer(
        &self,
        copy: *mut SDL_GPUCopyPass,
        texture: *mut SDL_GPUTexture,
        layer: u32,
        width: u32,
        height: u32,
        bytes: &[u8],
    ) -> Result<()> {
        let transfer = self.stage(bytes)?;
        let source = SDL_GPUTextureTransferInfo {
            transfer_buffer: transfer,
            offset: 0,
            pixels_per_row: width,
            rows_per_layer: height,
        };
        let destination = SDL_GPUTextureRegion {
            texture,
            layer,
            w: width,
            h: height,
            d: 1,
            ..Default::default()
        };
        unsafe {
            SDL_UploadToGPUTexture(copy, &source, &destination, false);
            SDL_ReleaseGPUTransferBuffer(self.device, transfer);
        }
        Ok(())
    }

    fn upload_buffer(
        &self,
        copy: *mut SDL_GPUCopyPass,
        buffer: *mut SDL_GPUBuffer,
        bytes: &[u8],
        cycle: bool,
    ) -> Result<()> {
        let transfer = self.stage(bytes)?;
        let source = SDL_GPUTransferBufferLocation {
            transfer_buffer: transfer,
            offset: 0,
        };
        let destination = SDL_GPUBufferRegion {
            buffer,
            offset: 0,
            size: bytes.len() as u32,
        };
        unsafe {
            SDL_UploadToGPUBuffer(copy, &source, &destination, cycle);
            SDL_ReleaseGPUTransferBuffer(self.device, transfer);
        }
        Ok(())
    }

    fn vertex_buffer(&self, size: u32) -> Result<*mut SDL_GPUBuffer> {
        let info = SDL_GPUBufferCreateInfo {
            usage: SDL_GPU_BUFFERUSAGE_VERTEX,
            size,
            ..Default::default()
        };
        let buffer = unsafe { SDL_CreateGPUBuffer(self.device, &info) };
        if buffer.is_null() {
            bail!("SDL_CreateGPUBuffer failed: {}", sdl_error());
        }
        Ok(buffer)
    }

    /// Make `mesh`'s vertex buffer resident.
    fn ensure_mesh(&mut self, copy: *mut SDL_GPUCopyPass, mesh: &Tmd) -> Result<MeshBuffer> {
        let key = mesh as *const Tmd as usize;
        let fingerprint = scene::mesh_fingerprint(mesh);
        if let Some(entry) = self.meshes.get_mut(&key)
            && entry.fingerprint == fingerprint
        {
            entry.last_used = self.frame;
            return Ok(entry.value);
        }
        let built = scene::mesh_vertices(mesh);
        let bytes = as_bytes(&built.vertices);
        let buffer = if bytes.is_empty() {
            std::ptr::null_mut()
        } else {
            let buffer = self.vertex_buffer(bytes.len() as u32)?;
            if let Err(err) = self.upload_buffer(copy, buffer, bytes, false) {
                unsafe { SDL_ReleaseGPUBuffer(self.device, buffer) };
                return Err(err);
            }
            buffer
        };
        let rows = built.vertices.iter().fold(0u32, |rows, vertex| {
            rows | 1u32.checked_shl((vertex.packed >> 8) & 0xFFFF).unwrap_or(0)
        });
        let value = MeshBuffer {
            buffer,
            vertex_count: built.vertices.len() as u32,
            blend_start: built.blend_start,
            rows,
        };
        self.fresh.push(Upload::Mesh(key));
        if let Some(old) = self.meshes.insert(
            key,
            Resident {
                value,
                fingerprint,
                last_used: self.frame,
            },
        ) {
            self.release_mesh(old.value);
        }
        Ok(value)
    }

    /// The HD replacement `pack` names for the mesh page `stem`
    /// (`htexture/<stem>`, covering the whole page), looked up once per stem.
    /// `None` leaves the page to its content hashes.
    fn named_page(
        &mut self,
        pack: &Pack,
        stem: &str,
        texture: &Texture8,
    ) -> Option<Arc<htexture::Replacement>> {
        let address = pack as *const Pack as usize;
        if self.named_pack != address {
            self.named_pack = address;
            self.named_pages.clear();
        }
        if let Some(found) = self.named_pages.get(stem) {
            return found.clone();
        }
        let found = htexture::named_replacement(pack, stem, [texture.width, texture.height]);
        self.named_pages.insert(stem.to_string(), found.clone());
        found
    }

    /// Make an indexed page and its palette resident, with the HD
    /// replacements `pack` carries for its CLUT rows. A missing replacement
    /// is reported only for the rows in `used`.
    fn ensure_page(
        &mut self,
        copy: *mut SDL_GPUCopyPass,
        texture: &Texture8,
        used: u32,
        pack: Option<&Pack>,
        named: Option<&Arc<htexture::Replacement>>,
    ) -> Result<Option<PageTextures>> {
        // Like the software rasterizer, an empty page draws nothing; a
        // truncated one draws its missing texels as index 0 (transparent).
        if texture.width == 0 || texture.height == 0 {
            return Ok(None);
        }
        let key = PageKey {
            texture: texture as *const Texture8 as usize,
            named: named.map_or(0, |replacement| Arc::as_ptr(replacement) as usize),
            pack: pack.map_or(0, |pack| pack as *const Pack as usize),
        };
        let pack = pack.filter(|pack| !pack.htexture().is_empty());
        let fingerprint = scene::texture_fingerprint(texture);
        if let Some(entry) = self.pages.get_mut(&key)
            && entry.fingerprint == fingerprint
        {
            entry.last_used = self.frame;
            return Ok(Some(entry.value));
        }
        let indices = self.texture(
            SDL_GPU_TEXTUREFORMAT_R8_UINT,
            texture.width,
            texture.height,
            SDL_GPU_TEXTUREUSAGE_SAMPLER,
        )?;
        let (rows, palette_rgba) = scene::palette_rgba(texture);
        let palette = match self.texture(
            SDL_GPU_TEXTUREFORMAT_R8G8B8A8_UNORM,
            crate::model::PALETTE_ROW_LEN as u32,
            rows,
            SDL_GPU_TEXTUREUSAGE_SAMPLER,
        ) {
            Ok(palette) => palette,
            Err(err) => {
                unsafe { SDL_ReleaseGPUTexture(self.device, indices) };
                return Err(err);
            }
        };
        let mut value = PageTextures {
            indices,
            palette,
            hd: std::ptr::null_mut(),
            hd_rows: std::ptr::null_mut(),
        };
        let pixels = (texture.width * texture.height) as usize;
        let padded;
        let page_indices = if texture.indices.len() >= pixels {
            &texture.indices[..pixels]
        } else {
            padded = {
                let mut padded = texture.indices.clone();
                padded.resize(pixels, 0);
                padded
            };
            &padded[..]
        };
        let uploaded = self
            .upload_texture(copy, indices, texture.width, texture.height, page_indices)
            .and_then(|()| {
                self.upload_texture(
                    copy,
                    palette,
                    crate::model::PALETTE_ROW_LEN as u32,
                    rows,
                    &palette_rgba,
                )
            })
            .and_then(|()| match (named, pack) {
                (Some(replacement), _) => {
                    self.upload_page_named(copy, &mut value, texture, replacement)
                }
                (None, Some(pack)) => self.upload_page_hd(copy, &mut value, texture, used, pack),
                (None, None) => Ok(()),
            });
        if let Err(err) = uploaded {
            self.release_page(value);
            return Err(err);
        }
        self.fresh.push(Upload::Page(key));
        if let Some(old) = self.pages.insert(
            key,
            Resident {
                value,
                fingerprint,
                last_used: self.frame,
            },
        ) {
            self.release_page(old.value);
        }
        match named {
            Some(replacement) => {
                self.page_keeps.insert(key, Arc::clone(replacement));
            }
            None => {
                self.page_keeps.remove(&key);
            }
        }
        Ok(Some(value))
    }

    /// Look up the replacement of each of `texture`'s CLUT rows and upload
    /// the ones found into `page`; a miss is reported for the rows in `used`.
    fn upload_page_hd(
        &self,
        copy: *mut SDL_GPUCopyPass,
        page: &mut PageTextures,
        texture: &Texture8,
        used: u32,
        pack: &Pack,
    ) -> Result<()> {
        let table = pack.htexture();
        // Rows sharing a replacement share its layer.
        let mut layers: Vec<Image> = Vec::new();
        let mut layer_by_path: Vec<(String, usize)> = Vec::new();
        let mut rows: Vec<(usize, usize, [u32; 2])> = Vec::new();
        for row in 0..page_row_count(texture) {
            let candidates = htexture::page_candidates(texture, row);
            let found = if used & 1u32.checked_shl(row as u32).unwrap_or(0) != 0 {
                table.find(&candidates, || {
                    format!(
                        "{}x{} model page, CLUT row {row}",
                        texture.width, texture.height
                    )
                })
            } else {
                table.find_quietly(&candidates)
            };
            let Some(found) = found else {
                continue;
            };
            let layer = match layer_by_path.iter().find(|(path, _)| *path == found.path) {
                Some((_, layer)) => *layer,
                None => match htexture::load(pack, &found.path) {
                    Ok(image) if image.width > 0 && image.height > 0 => {
                        layers.push(image);
                        let layer = layers.len() - 1;
                        layer_by_path.push((found.path, layer));
                        layer
                    }
                    Ok(_) => continue,
                    Err(err) => {
                        eprintln!("htexture: {err:#}");
                        continue;
                    }
                },
            };
            rows.push((row, layer, [found.frame[0], found.frame[1]]));
        }
        self.upload_page_layers(copy, page, texture, &layers, &rows)
    }

    /// Upload a named per-item page replacement, which covers the whole page
    /// and replaces its first CLUT row.
    fn upload_page_named(
        &self,
        copy: *mut SDL_GPUCopyPass,
        page: &mut PageTextures,
        texture: &Texture8,
        replacement: &htexture::Replacement,
    ) -> Result<()> {
        let layers = [replacement.image.clone()];
        let rows = [(
            0,
            0,
            [replacement.frame[0].max(1), replacement.frame[1].max(1)],
        )];
        self.upload_page_layers(copy, page, texture, &layers, &rows)
    }

    /// Upload one array layer per replacement image and the per-row placement
    /// table `mesh.frag` reads.
    fn upload_page_layers(
        &self,
        copy: *mut SDL_GPUCopyPass,
        page: &mut PageTextures,
        texture: &Texture8,
        layers: &[Image],
        rows: &[(usize, usize, [u32; 2])],
    ) -> Result<()> {
        if layers.is_empty() || rows.is_empty() {
            return Ok(());
        }
        let row_count = page_row_count(texture);
        let mut placed = vec![[0u16; 8]; row_count];
        for &(row, layer, frame) in rows {
            let image = &layers[layer];
            if row >= row_count || image.width == 0 || image.height == 0 {
                continue;
            }
            placed[row] = [
                layer as u16 + 1,
                frame[0].min(0xFFFF) as u16,
                frame[1].min(0xFFFF) as u16,
                0,
                image.width.min(0xFFFF) as u16,
                image.height.min(0xFFFF) as u16,
                0,
                0,
            ];
        }
        let width = layers.iter().map(|image| image.width).max().unwrap_or(1);
        let height = layers.iter().map(|image| image.height).max().unwrap_or(1);
        page.hd = self.array_texture(
            SDL_GPU_TEXTUREFORMAT_R8G8B8A8_UNORM,
            width,
            height,
            layers.len() as u32,
        )?;
        page.hd_rows = self.texture(
            SDL_GPU_TEXTUREFORMAT_R16G16B16A16_UINT,
            2,
            row_count as u32,
            SDL_GPU_TEXTUREUSAGE_SAMPLER,
        )?;
        for (layer, image) in layers.iter().enumerate() {
            let bytes = &image.rgba[..(image.width * image.height * 4) as usize];
            self.upload_layer(
                copy,
                page.hd,
                layer as u32,
                image.width,
                image.height,
                bytes,
            )?;
        }
        let bytes: Vec<u8> = placed
            .iter()
            .flatten()
            .flat_map(|value| value.to_le_bytes())
            .collect();
        self.upload_texture(copy, page.hd_rows, 2, row_count as u32, &bytes)
    }

    /// The HD replacement `pack` carries for `image`, and its texels per
    /// original texel. A room mask's `background` names its replacement
    /// when the mask's own hashes don't.
    fn image_replacement(
        image: &Image,
        background: Option<&Image>,
        pack: &Pack,
    ) -> Option<(Image, [f32; 2])> {
        let table = pack.htexture();
        let mut candidates = htexture::image_candidates(image);
        if let Some(background) = background {
            candidates.extend(table.paired_masks(&htexture::image_candidates(background)));
        }
        let found = table.find(&candidates, || {
            format!("{}x{} image", image.width, image.height)
        })?;
        match htexture::load(pack, &found.path) {
            Ok(hd)
                if hd.width > 0
                    && hd.height > 0
                    && hd.rgba.len() >= (hd.width * hd.height * 4) as usize =>
            {
                let scale = [
                    hd.width as f32 / found.frame[0].max(1) as f32,
                    hd.height as f32 / found.frame[1].max(1) as f32,
                ];
                Some((hd, scale))
            }
            Ok(_) => None,
            Err(err) => {
                eprintln!("htexture: {err:#}");
                None
            }
        }
    }

    /// Make an RGBA image, or the HD replacement `pack` carries for it,
    /// resident.
    fn ensure_image(
        &mut self,
        copy: *mut SDL_GPUCopyPass,
        image: &Image,
        background: Option<&Image>,
        pack: Option<&Pack>,
    ) -> Result<Option<ImageTexture>> {
        if image.width == 0
            || image.height == 0
            || image.rgba.len() < (image.width * image.height * 4) as usize
        {
            return Ok(None);
        }
        let key = image as *const Image as usize;
        let fingerprint = scene::image_fingerprint(image);
        if let Some(entry) = self.images.get_mut(&key)
            && entry.fingerprint == fingerprint
        {
            entry.last_used = self.frame;
            return Ok(Some(entry.value));
        }
        let replacement = pack
            .filter(|pack| !pack.htexture().is_empty())
            .and_then(|pack| Self::image_replacement(image, background, pack));
        let (source, scale) = match &replacement {
            Some((hd, scale)) => (hd, *scale),
            None => (image, [1.0, 1.0]),
        };
        let texture = self.texture(
            SDL_GPU_TEXTUREFORMAT_R8G8B8A8_UNORM,
            source.width,
            source.height,
            SDL_GPU_TEXTUREUSAGE_SAMPLER,
        )?;
        let bytes = &source.rgba[..(source.width * source.height * 4) as usize];
        if let Err(err) = self.upload_texture(copy, texture, source.width, source.height, bytes) {
            unsafe { SDL_ReleaseGPUTexture(self.device, texture) };
            return Err(err);
        }
        let value = ImageTexture { texture, scale };
        self.fresh.push(Upload::Image(key));
        if let Some(old) = self.images.insert(
            key,
            Resident {
                value,
                fingerprint,
                last_used: self.frame,
            },
        ) {
            unsafe { SDL_ReleaseGPUTexture(self.device, old.value.texture) };
        }
        Ok(Some(value))
    }

    fn release_mesh(&self, mesh: MeshBuffer) {
        if !mesh.buffer.is_null() {
            unsafe { SDL_ReleaseGPUBuffer(self.device, mesh.buffer) };
        }
    }

    fn release_page(&self, page: PageTextures) {
        unsafe {
            SDL_ReleaseGPUTexture(self.device, page.indices);
            SDL_ReleaseGPUTexture(self.device, page.palette);
            for texture in [page.hd, page.hd_rows] {
                if !texture.is_null() {
                    SDL_ReleaseGPUTexture(self.device, texture);
                }
            }
        }
    }

    /// Release every upload not drawn in the last [`EVICT_AFTER_FRAMES`]
    /// scene frames. SDL defers the release until in-flight work is done.
    /// After [`GpuRenderer::retire`], release every upload the frame did not
    /// draw.
    fn evict(&mut self) {
        let frame = self.frame;
        let horizon = if std::mem::take(&mut self.retiring) {
            0
        } else {
            EVICT_AFTER_FRAMES
        };
        let stale = |last_used: u64| frame.saturating_sub(last_used) > horizon;
        let meshes: Vec<usize> = self
            .meshes
            .iter()
            .filter(|(_, entry)| stale(entry.last_used))
            .map(|(&key, _)| key)
            .collect();
        for key in meshes {
            if let Some(entry) = self.meshes.remove(&key) {
                self.release_mesh(entry.value);
            }
        }
        let pages: Vec<PageKey> = self
            .pages
            .iter()
            .filter(|(_, entry)| stale(entry.last_used))
            .map(|(&key, _)| key)
            .collect();
        for key in pages {
            self.page_keeps.remove(&key);
            if let Some(entry) = self.pages.remove(&key) {
                self.release_page(entry.value);
            }
        }
        let images: Vec<usize> = self
            .images
            .iter()
            .filter(|(_, entry)| stale(entry.last_used))
            .map(|(&key, _)| key)
            .collect();
        for key in images {
            if let Some(entry) = self.images.remove(&key) {
                unsafe { SDL_ReleaseGPUTexture(self.device, entry.value.texture) };
            }
        }
    }

    /// Tear down the previous room's residency: the next recorded scene
    /// keeps what it draws and releases everything else.
    pub fn retire(&mut self) {
        self.retiring = true;
    }

    /// Release every resident upload: gameplay has ended, so nothing cached
    /// will be drawn soon.
    pub fn release_residency(&mut self) {
        for (_, entry) in std::mem::take(&mut self.meshes) {
            self.release_mesh(entry.value);
        }
        self.page_keeps.clear();
        self.named_pages.clear();
        for (_, entry) in std::mem::take(&mut self.pages) {
            self.release_page(entry.value);
        }
        for (_, entry) in std::mem::take(&mut self.images) {
            unsafe { SDL_ReleaseGPUTexture(self.device, entry.value.texture) };
        }
        self.release_model_targets();
    }

    /// Forget the entries a failed recording created: their uploads were
    /// cancelled with its command buffer, so the data never arrived.
    fn forget_fresh(&mut self) {
        for upload in std::mem::take(&mut self.fresh) {
            match upload {
                Upload::Mesh(key) => {
                    if let Some(entry) = self.meshes.remove(&key) {
                        self.release_mesh(entry.value);
                    }
                }
                Upload::Page(key) => {
                    self.page_keeps.remove(&key);
                    if let Some(entry) = self.pages.remove(&key) {
                        self.release_page(entry.value);
                    }
                }
                Upload::Image(key) => {
                    if let Some(entry) = self.images.remove(&key) {
                        unsafe { SDL_ReleaseGPUTexture(self.device, entry.value.texture) };
                    }
                }
            }
        }
    }

    /// The number of resident meshes, pages and images.
    pub fn resident_counts(&self) -> (usize, usize, usize) {
        (self.meshes.len(), self.pages.len(), self.images.len())
    }

    /// Draw `scene` into the scene target, with the HD replacements `pack`
    /// carries. The result persists until the next call, so a frozen frame
    /// (the pause menu) keeps compositing it.
    pub fn record_scene(&mut self, scene: &GameplayScene<'_>, pack: Option<&Pack>) -> Result<()> {
        #[cfg(test)]
        if std::mem::take(&mut self.fail_next_scene) {
            bail!("simulated scene failure");
        }
        self.follow_window()?;
        self.frame += 1;
        self.fresh.clear();
        let command = self.command_buffer()?;
        let recorded = self.record_scene_into(command, scene, pack);
        let submitted = match recorded {
            Ok(()) => Self::submit(command),
            Err(err) => {
                unsafe { SDL_CancelGPUCommandBuffer(command) };
                Err(err)
            }
        };
        if submitted.is_err() {
            self.forget_fresh();
        } else {
            self.fresh.clear();
            self.evict();
        }
        submitted
    }

    fn record_scene_into(
        &mut self,
        command: *mut SDL_GPUCommandBuffer,
        scene: &GameplayScene<'_>,
        pack: Option<&Pack>,
    ) -> Result<()> {
        // Uploads first: the residency checks and this frame's sprites.
        let copy = unsafe { SDL_BeginGPUCopyPass(command) };
        if copy.is_null() {
            bail!("SDL_BeginGPUCopyPass failed: {}", sdl_error());
        }
        let uploads = (|| -> Result<_> {
            let mut meshes = Vec::with_capacity(scene.meshes.len());
            for mesh in scene.meshes {
                let buffer = self.ensure_mesh(copy, mesh.mesh)?;
                // A named image covers one palette row, so a page with more
                // rows keeps its per-row content hashes.
                let named = match (mesh.hd_name, pack) {
                    (Some(stem), Some(pack)) if page_row_count(mesh.texture) == 1 => {
                        self.named_page(pack, stem, mesh.texture)
                    }
                    _ => None,
                };
                let page =
                    self.ensure_page(copy, mesh.texture, buffer.rows, pack, named.as_ref())?;
                meshes.push(page.map(|page| (buffer, page)));
            }
            let sprites = scene::scene_sprites(scene);
            let mut sprite_textures = HashMap::new();
            for run in sprites
                .before
                .iter()
                .chain(&sprites.after)
                .chain(sprites.debug.iter().map(|(run, _)| run))
            {
                let key = run.image as *const Image as usize;
                if let std::collections::hash_map::Entry::Vacant(slot) = sprite_textures.entry(key)
                    && let Some(texture) =
                        self.ensure_image(copy, run.image, run.background, pack)?
                {
                    slot.insert(texture);
                }
            }
            let bytes = as_bytes(&sprites.vertices);
            if !bytes.is_empty() {
                if self.sprite_capacity < bytes.len() as u32 {
                    let capacity = (bytes.len() as u32).next_power_of_two().max(4096);
                    let buffer = self.vertex_buffer(capacity)?;
                    if !self.sprite_buffer.is_null() {
                        unsafe { SDL_ReleaseGPUBuffer(self.device, self.sprite_buffer) };
                    }
                    self.sprite_buffer = buffer;
                    self.sprite_capacity = capacity;
                }
                self.upload_buffer(copy, self.sprite_buffer, bytes, true)?;
            }
            Ok((meshes, sprites, sprite_textures))
        })();
        unsafe { SDL_EndGPUCopyPass(copy) };
        let (meshes, sprites, sprite_textures) = uploads?;

        // Without a background the scene is transparent where nothing is
        // drawn, so the 2D pass shows what it drew under it (the DIED
        // screen's backdrop).
        let clear_color = SDL_FColor {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: if scene.background.is_some() { 1.0 } else { 0.0 },
        };
        // With MSAA the samples resolve into the target the composite reads.
        let colour = if self.scene_samples.is_null() {
            SDL_GPUColorTargetInfo {
                texture: self.scene_colour,
                clear_color,
                load_op: SDL_GPU_LOADOP_CLEAR,
                store_op: SDL_GPU_STOREOP_STORE,
                cycle: true,
                ..Default::default()
            }
        } else {
            SDL_GPUColorTargetInfo {
                texture: self.scene_samples,
                clear_color,
                load_op: SDL_GPU_LOADOP_CLEAR,
                store_op: SDL_GPU_STOREOP_RESOLVE,
                resolve_texture: self.scene_colour,
                cycle: true,
                cycle_resolve_texture: true,
                ..Default::default()
            }
        };
        let depth = SDL_GPUDepthStencilTargetInfo {
            texture: self.scene_depth,
            clear_depth: 1.0,
            load_op: SDL_GPU_LOADOP_CLEAR,
            store_op: SDL_GPU_STOREOP_DONT_CARE,
            stencil_load_op: SDL_GPU_LOADOP_DONT_CARE,
            stencil_store_op: SDL_GPU_STOREOP_DONT_CARE,
            cycle: true,
            ..Default::default()
        };
        let draw_sprites = |pass: *mut SDL_GPURenderPass,
                            pipeline: *mut SDL_GPUGraphicsPipeline,
                            mask_keys: *mut SDL_GPUTexture,
                            runs: &[SpriteRun<'_>]| {
            if self.sprite_buffer.is_null() {
                return;
            }
            let binding = SDL_GPUBufferBinding {
                buffer: self.sprite_buffer,
                offset: 0,
            };
            for run in runs {
                let Some(&texture) = sprite_textures.get(&(run.image as *const Image as usize))
                else {
                    continue;
                };
                let samplers = [
                    SDL_GPUTextureSamplerBinding {
                        texture: texture.texture,
                        sampler: self.sampler,
                    },
                    SDL_GPUTextureSamplerBinding {
                        texture: mask_keys,
                        sampler: self.sampler,
                    },
                ];
                let scale = [texture.scale[0], texture.scale[1], 0.0, 0.0];
                unsafe {
                    SDL_BindGPUGraphicsPipeline(pass, pipeline);
                    SDL_PushGPUFragmentUniformData(
                        command,
                        0,
                        scale.as_ptr().cast::<c_void>(),
                        std::mem::size_of_val(&scale) as u32,
                    );
                    SDL_BindGPUVertexBuffers(pass, 0, &binding, 1);
                    SDL_BindGPUFragmentSamplers(pass, 0, samplers.as_ptr(), samplers.len() as u32);
                    SDL_DrawGPUPrimitives(pass, run.count, 1, run.first, 0);
                }
            }
        };

        // The masks' keys first, into their own depth-only target.
        let keys = SDL_GPUDepthStencilTargetInfo {
            texture: self.mask_keys,
            clear_depth: 1.0,
            load_op: SDL_GPU_LOADOP_CLEAR,
            store_op: SDL_GPU_STOREOP_STORE,
            stencil_load_op: SDL_GPU_LOADOP_DONT_CARE,
            stencil_store_op: SDL_GPU_STOREOP_DONT_CARE,
            cycle: true,
            ..Default::default()
        };
        let pass = unsafe { SDL_BeginGPURenderPass(command, std::ptr::null(), 0, &keys) };
        if pass.is_null() {
            bail!("SDL_BeginGPURenderPass failed: {}", sdl_error());
        }
        draw_sprites(
            pass,
            self.pipelines.mask_keys,
            self.no_mask_keys,
            sprites.mask_keys.as_slice(),
        );
        unsafe { SDL_EndGPURenderPass(pass) };

        let pass = unsafe { SDL_BeginGPURenderPass(command, &colour, 1, &depth) };
        if pass.is_null() {
            bail!("SDL_BeginGPURenderPass failed: {}", sdl_error());
        }
        draw_sprites(pass, self.pipelines.sprite, self.mask_keys, &sprites.before);

        // The opaque texels of every mesh, then the mirror copy, then the
        // STP texels of the blended packets in the same order.
        let lights: Vec<_> = scene
            .meshes
            .iter()
            .map(|mesh| {
                scene
                    .lighting
                    .latched(mesh.light_pos.unwrap_or(root_position(mesh.joints)))
            })
            .collect();
        let lights = &lights;
        let primary = scene.meshes.iter().enumerate().map(|(index, mesh)| {
            (
                index,
                mesh,
                scene::draw_uniforms(
                    scene.camera,
                    mesh,
                    &lights[index],
                    index < scene.mirror_from,
                    None,
                ),
            )
        });
        let mirrored = scene.mirror.into_iter().flat_map(|mirror| {
            scene
                .meshes
                .iter()
                .enumerate()
                .skip(scene.mirror_from)
                .map(move |(index, mesh)| {
                    (
                        index,
                        mesh,
                        scene::draw_uniforms(
                            &mirror.camera,
                            mesh,
                            &lights[index],
                            false,
                            Some(mirror),
                        ),
                    )
                })
        });
        let draws: Vec<_> = primary.chain(mirrored).collect();
        for blend_pass in [false, true] {
            for &(index, mesh, ref uniforms) in &draws {
                let Some((buffer, page)) = meshes[index] else {
                    continue;
                };
                let (first, count) = scene::pass_range(
                    mesh.blend_weight,
                    blend_pass,
                    buffer.blend_start,
                    buffer.vertex_count,
                );
                if buffer.buffer.is_null() || count == 0 || uniforms.flags[0] == u32::MAX {
                    continue;
                }
                let pipeline = if blend_pass {
                    self.pipelines.mesh_blend
                } else {
                    self.pipelines.mesh_opaque
                };
                let mut material = scene::material(mesh.blend_weight, blend_pass);
                let hd = if page.hd.is_null() {
                    [self.no_hd, self.no_hd_rows]
                } else {
                    material[2] = 1.0;
                    [page.hd, page.hd_rows]
                };
                draw_mesh(
                    command,
                    pass,
                    pipeline,
                    uniforms,
                    material,
                    buffer.buffer,
                    [page.indices, page.palette, hd[0], hd[1], self.mask_keys],
                    self.sampler,
                    first,
                    count,
                );
            }
        }

        draw_sprites(pass, self.pipelines.sprite, self.mask_keys, &sprites.after);
        for (run, write) in &sprites.debug {
            let pipeline = if *write {
                self.pipelines.sprite_depth
            } else {
                self.pipelines.sprite
            };
            draw_sprites(pass, pipeline, self.mask_keys, std::slice::from_ref(run));
        }
        unsafe { SDL_EndGPURenderPass(pass) };
        Ok(())
    }

    /// Present `frame`, and `ui` over it.
    ///
    /// A frame drawn for the GPU ([`Framebuffer::record`]) is drawn by the
    /// 2D pass ([`crate::draw2d`]), with the gameplay scene wherever it
    /// shows it; a frame drawn in software (a capture driver's) is presented
    /// as its pixels. `ui`, a canvas of the aspect's full size (the F1 debug
    /// menu at a wider aspect), is drawn the same way into a clear layer
    /// and blended over the frame across the rectangle that canvas fits.
    pub fn present(&mut self, frame: &Framebuffer, ui: Option<&Framebuffer>) -> Result<()> {
        if self.window.is_null() || self.pipelines.composite.is_null() {
            bail!("the GPU renderer has no window to present to");
        }
        // A 320x240 frame, or a wider aspect's full-height canvas.
        let size = (frame.width, frame.height);
        if size.1 != LAYER_HEIGHT || size.0 < LAYER_WIDTH || size.0 > video::MAX_CANVAS_WIDTH {
            bail!(
                "expected a {LAYER_WIDTH}x{LAYER_HEIGHT} framebuffer, got {}x{}",
                size.0,
                size.1
            );
        }
        self.overlay_frame += 1;
        self.ui_shown = false;
        if frame.list().is_some_and(|list| !list.shows_scene()) {
            // No scene was recorded to size the targets this frame: follow
            // the window before the UI layer is sized from them.
            self.follow_window()?;
        }
        if let Some(ui) = ui {
            self.draw_ui(ui)?;
            self.ui_shown = true;
        }
        let layer = match frame.list() {
            Some(list) => {
                self.draw_frame(list)?;
                Layer::Target
            }
            None => {
                if frame.rgba.len() != size.0 as usize * size.1 as usize * 4 {
                    bail!(
                        "a {}x{} frame of {} bytes",
                        size.0,
                        size.1,
                        frame.rgba.len()
                    );
                }
                Layer::Upload(&frame.rgba)
            }
        };
        self.evict_overlay_images();
        // The whole 4:3 frame is composited into the viewport that puts its
        // visible band on the fitted rectangle, and the scissor crops the
        // rest (the band is the whole frame at 4:3).
        let shown = video::shown(frame.width, frame.height, frame.view);
        self.composite(layer, size, |width, height| {
            let fit = video::fit_size(width, height, shown.width, shown.height);
            (video::frame_viewport(fit, shown, frame.height), fit)
        })
    }

    /// Present a `width` x `height` RGBA image (an HD film frame) at its own
    /// resolution where `frame` would be: letterboxed to its aspect inside
    /// the 4:3 frame's viewport and cropped to the band `frame` shows. An
    /// image whose `serial` is already uploaded is not uploaded again.
    pub fn present_image(
        &mut self,
        frame: &Framebuffer,
        rgba: &[u8],
        (width, height): (u32, u32),
        serial: u64,
    ) -> Result<()> {
        if self.window.is_null() || self.pipelines.composite.is_null() {
            bail!("the GPU renderer has no window to present to");
        }
        if width == 0
            || height == 0
            || width > crate::media::MAX_DIMENSION
            || height > crate::media::MAX_DIMENSION
            || rgba.len() != width as usize * height as usize * 4
        {
            bail!("unexpected {width}x{height} image of {} bytes", rgba.len());
        }
        self.ui_shown = false;
        let shown = video::shown(frame.width, frame.height, frame.view);
        let layer = if self.layer_image == Some(serial) && self.layer_size == (width, height) {
            Layer::Kept
        } else {
            Layer::Upload(rgba)
        };
        self.composite(layer, (width, height), |target_w, target_h| {
            let fit = video::fit_size(target_w, target_h, shown.width, shown.height);
            let frame_viewport = video::frame_viewport(fit, shown, frame.height);
            (video::image_viewport(frame_viewport, width, height), fit)
        })?;
        self.layer_image = Some(serial);
        Ok(())
    }

    /// Set the native GUI layer the next present draws over the whole
    /// window, uploading it; `None` draws none.
    pub fn set_native(&mut self, canvas: Option<&crate::gui::Canvas>) -> Result<()> {
        self.native_shown = false;
        self.gui.shown = false;
        let Some(canvas) = canvas else {
            return Ok(());
        };
        if canvas.ops().is_some() {
            return self.set_gui(canvas);
        }
        let size = (canvas.width, canvas.height);
        if size.0 == 0
            || size.1 == 0
            || size.0 > MAX_SCENE_SIDE
            || size.1 > MAX_SCENE_SIDE
            || canvas.rgba.len() != size.0 as usize * size.1 as usize * 4
        {
            bail!(
                "unexpected {}x{} GUI layer of {} bytes",
                size.0,
                size.1,
                canvas.rgba.len()
            );
        }
        if !self.native_texture.is_null()
            && self.native_size == size
            && self.native_serial == canvas.serial
        {
            // The texture already holds this frame.
            self.native_shown = true;
            return Ok(());
        }
        if self.native_texture.is_null() || self.native_size != size {
            if !self.native_texture.is_null() {
                unsafe { SDL_ReleaseGPUTexture(self.device, self.native_texture) };
                self.native_texture = std::ptr::null_mut();
            }
            self.native_texture = self.texture(
                SDL_GPU_TEXTUREFORMAT_R8G8B8A8_UNORM,
                size.0,
                size.1,
                SDL_GPU_TEXTUREUSAGE_SAMPLER,
            )?;
            self.native_size = size;
        }
        let command = self.command_buffer()?;
        let copy = unsafe { SDL_BeginGPUCopyPass(command) };
        if copy.is_null() {
            unsafe { SDL_CancelGPUCommandBuffer(command) };
            bail!("SDL_BeginGPUCopyPass failed: {}", sdl_error());
        }
        let uploaded = self.upload_texture(copy, self.native_texture, size.0, size.1, &canvas.rgba);
        unsafe { SDL_EndGPUCopyPass(copy) };
        if let Err(err) = uploaded {
            unsafe { SDL_CancelGPUCommandBuffer(command) };
            return Err(err);
        }
        Self::submit(command)?;
        self.native_serial = canvas.serial;
        self.native_shown = true;
        Ok(())
    }

    /// `texture`, `current` in size, if it is `size`; otherwise a new `size`
    /// target of the 2D pass, `texture` released once it exists.
    fn refit_target(
        &self,
        texture: *mut SDL_GPUTexture,
        current: (u32, u32),
        size: (u32, u32),
    ) -> Result<*mut SDL_GPUTexture> {
        if !texture.is_null() && current == size {
            return Ok(texture);
        }
        let fresh = self.texture(
            SDL_GPU_TEXTUREFORMAT_R8G8B8A8_UNORM,
            size.0,
            size.1,
            SDL_GPU_TEXTUREUSAGE_SAMPLER | SDL_GPU_TEXTUREUSAGE_COLOR_TARGET,
        )?;
        if !texture.is_null() {
            unsafe { SDL_ReleaseGPUTexture(self.device, texture) };
        }
        Ok(fresh)
    }

    /// The size the 2D pass draws a `width` x `height` frame at: the scene
    /// target's resolution.
    fn pass_size(&self, (width, height): (u32, u32)) -> (u32, u32) {
        let (scene_w, scene_h) = self.scene_size;
        let side = |scene: u32, frame: u32, layer: u32| {
            (scene as u64 * frame as u64 / layer as u64).clamp(1, MAX_SCENE_SIDE as u64) as u32
        };
        (
            side(scene_w, width, LAYER_WIDTH),
            side(scene_h, height, LAYER_HEIGHT),
        )
    }

    /// Draw `list` into `overlay_target` (tests and tools; presenting a
    /// recorded frame draws it).
    pub fn draw_frame(&mut self, list: &DrawList) -> Result<()> {
        if !list.shows_scene() {
            // No scene was recorded to size the targets this frame.
            self.follow_window()?;
        }
        let size = self.pass_size((list.size[0], list.size[1]));
        self.overlay_target = self.refit_target(self.overlay_target, self.overlay_size, size)?;
        self.overlay_size = size;
        self.draw_list(list, FRAME_ATLAS, self.overlay_target, size, true)
    }

    /// Draw `ui` into `ui_texture`: its list, or its pixels.
    fn draw_ui(&mut self, ui: &Framebuffer) -> Result<()> {
        let frame = (ui.width, ui.height);
        if frame.0 == 0 || frame.1 == 0 || frame.0 > video::MAX_CANVAS_WIDTH {
            bail!("unexpected {}x{} UI layer", frame.0, frame.1);
        }
        let size = match ui.list() {
            Some(_) => self.pass_size(frame),
            None => frame,
        };
        self.ui_texture = self.refit_target(self.ui_texture, self.ui_target_size, size)?;
        self.ui_target_size = size;
        self.ui_size = frame;
        match ui.list() {
            Some(list) => self.draw_list(list, UI_ATLAS, self.ui_texture, size, false),
            None => self.upload_ui(ui),
        }
    }

    /// Upload `ui`'s premultiplied pixels into `ui_texture`, which is their
    /// size: the software backend's UI layer (its coverage), presented here
    /// after a session falls back to software.
    fn upload_ui(&mut self, ui: &Framebuffer) -> Result<()> {
        let (width, height) = (ui.width, ui.height);
        if ui.rgba.len() != width as usize * height as usize * 4 {
            bail!(
                "unexpected {width}x{height} UI layer of {} bytes",
                ui.rgba.len()
            );
        }
        let command = self.command_buffer()?;
        let copy = unsafe { SDL_BeginGPUCopyPass(command) };
        if copy.is_null() {
            unsafe { SDL_CancelGPUCommandBuffer(command) };
            bail!("SDL_BeginGPUCopyPass failed: {}", sdl_error());
        }
        let uploaded = self.upload_texture(copy, self.ui_texture, width, height, &ui.rgba);
        unsafe { SDL_EndGPUCopyPass(copy) };
        if let Err(err) = uploaded {
            unsafe { SDL_CancelGPUCommandBuffer(command) };
            return Err(err);
        }
        Self::submit(command)
    }

    /// Draw `list`'s items in order into `target`, a `size` 2D pass target,
    /// over black (`opaque`) or clear, its sprites from atlas `slot`.
    fn draw_list(
        &mut self,
        list: &DrawList,
        slot: usize,
        target: *mut SDL_GPUTexture,
        size: (u32, u32),
        opaque: bool,
    ) -> Result<()> {
        if let Some(error) = list.error {
            bail!("the 2D pass cannot draw the frame: {error}");
        }
        let command = self.command_buffer()?;
        let mut fresh = Vec::new();
        // The models' pages join the scene's residency.
        self.fresh.clear();
        let recorded = self.record_list(command, list, slot, (target, size, opaque), &mut fresh);
        let submitted = match recorded {
            Ok(()) => Self::submit(command),
            Err(err) => {
                unsafe { SDL_CancelGPUCommandBuffer(command) };
                Err(err)
            }
        };
        if submitted.is_ok() {
            self.fresh.clear();
        } else {
            // Their uploads were cancelled with the command buffer.
            self.forget_fresh();
            self.atlases[slot].revision = (0, 0);
            for key in fresh {
                let texture = match self.overlay_images.remove(&key) {
                    Some((_, texture, _)) => Some(texture),
                    None => self
                        .overlay_indices
                        .remove(&key)
                        .map(|(_, texture, _)| texture),
                };
                if let Some(texture) = texture {
                    unsafe { SDL_ReleaseGPUTexture(self.device, texture) };
                }
            }
        }
        submitted
    }

    /// Make atlas `slot` hold at least `rows` rows (a power of two),
    /// forgetting what it holds when it grows.
    fn ensure_atlas(&mut self, slot: usize, rows: u32) -> Result<()> {
        let atlas = self.atlases[slot];
        if !atlas.texture.is_null() && atlas.rows >= rows {
            return Ok(());
        }
        let rows = rows
            .next_power_of_two()
            .clamp(MIN_ATLAS_ROWS, draw2d::ATLAS_MAX_HEIGHT);
        let texture = self.texture(
            SDL_GPU_TEXTUREFORMAT_R8G8B8A8_UNORM,
            draw2d::ATLAS_WIDTH,
            rows,
            SDL_GPU_TEXTUREUSAGE_SAMPLER,
        )?;
        if !atlas.texture.is_null() {
            unsafe { SDL_ReleaseGPUTexture(self.device, atlas.texture) };
        }
        self.atlases[slot] = AtlasTexture {
            texture,
            rows,
            revision: (0, 0),
        };
        Ok(())
    }

    /// Upload `atlas` into atlas `slot` unless it already holds it.
    fn upload_atlas(
        &mut self,
        copy: *mut SDL_GPUCopyPass,
        slot: usize,
        atlas: &draw2d::Atlas,
    ) -> Result<()> {
        let stride = draw2d::ATLAS_WIDTH as usize * 4;
        let rows = atlas.height.min((atlas.texels.len() / stride) as u32);
        if rows == 0 {
            return Ok(());
        }
        self.ensure_atlas(slot, rows)?;
        // The texels are the ones uploaded while the revision holds; a
        // rewound frame using fewer rows needs nothing new.
        let (revision, held) = self.atlases[slot].revision;
        if revision == atlas.revision && held >= rows {
            return Ok(());
        }
        let bytes = &atlas.texels[..rows as usize * stride];
        self.upload_texture(
            copy,
            self.atlases[slot].texture,
            draw2d::ATLAS_WIDTH,
            rows,
            bytes,
        )?;
        self.atlases[slot].revision = (atlas.revision, rows);
        Ok(())
    }

    /// Upload `replacement`'s image unless it is resident, noting a new
    /// texture in `fresh`.
    fn ensure_overlay_image(
        &mut self,
        copy: *mut SDL_GPUCopyPass,
        replacement: &Arc<htexture::Replacement>,
        fresh: &mut Vec<usize>,
    ) -> Result<()> {
        let key = Arc::as_ptr(replacement) as usize;
        if let Some(entry) = self.overlay_images.get_mut(&key) {
            entry.2 = self.overlay_frame;
            return Ok(());
        }
        let image = &replacement.image;
        let texture = self.texture(
            SDL_GPU_TEXTUREFORMAT_R8G8B8A8_UNORM,
            image.width,
            image.height,
            SDL_GPU_TEXTUREUSAGE_SAMPLER,
        )?;
        let bytes = &image.rgba[..(image.width * image.height * 4) as usize];
        if let Err(err) = self.upload_texture(copy, texture, image.width, image.height, bytes) {
            unsafe { SDL_ReleaseGPUTexture(self.device, texture) };
            return Err(err);
        }
        self.overlay_images
            .insert(key, (replacement.clone(), texture, self.overlay_frame));
        fresh.push(key);
        Ok(())
    }

    /// Make `model_targets[index]` exist at `size`.
    fn ensure_model_target(&mut self, index: usize, size: (u32, u32)) -> Result<()> {
        if self
            .model_targets
            .get(index)
            .is_some_and(|target| target.size == size)
        {
            return Ok(());
        }
        let mut target = ModelTarget {
            colour: std::ptr::null_mut(),
            samples: std::ptr::null_mut(),
            depth: std::ptr::null_mut(),
            size,
        };
        let created = (|| -> Result<()> {
            target.colour = self.texture(
                SDL_GPU_TEXTUREFORMAT_R8G8B8A8_UNORM,
                size.0,
                size.1,
                SDL_GPUTextureUsageFlags(
                    SDL_GPU_TEXTUREUSAGE_SAMPLER.0 | SDL_GPU_TEXTUREUSAGE_COLOR_TARGET.0,
                ),
            )?;
            if self.samples != SDL_GPU_SAMPLECOUNT_1 {
                target.samples = self.target(
                    SDL_GPU_TEXTUREFORMAT_R8G8B8A8_UNORM,
                    size.0,
                    size.1,
                    SDL_GPU_TEXTUREUSAGE_COLOR_TARGET,
                    self.samples,
                )?;
            }
            target.depth = self.target(
                self.depth_format,
                size.0,
                size.1,
                SDL_GPU_TEXTUREUSAGE_DEPTH_STENCIL_TARGET,
                self.samples,
            )?;
            Ok(())
        })();
        if let Err(err) = created {
            self.release_model_target(target);
            return Err(err);
        }
        if index < self.model_targets.len() {
            let old = std::mem::replace(&mut self.model_targets[index], target);
            self.release_model_target(old);
        } else {
            self.model_targets.push(target);
        }
        Ok(())
    }

    fn release_model_targets(&mut self) {
        for target in std::mem::take(&mut self.model_targets) {
            self.release_model_target(target);
        }
    }

    fn release_model_target(&self, target: ModelTarget) {
        for texture in [target.colour, target.samples, target.depth] {
            if !texture.is_null() {
                unsafe { SDL_ReleaseGPUTexture(self.device, texture) };
            }
        }
    }

    /// Record a pass drawing a model, whose vertices are `run`'s in
    /// `model_buffer`, into `target` cleared to transparent, premultiplied:
    /// the opaque texels, then the blended ones.
    fn render_model(
        &self,
        command: *mut SDL_GPUCommandBuffer,
        target: &ModelTarget,
        run: ModelRun,
    ) -> Result<()> {
        let ModelRun {
            page, first, count, ..
        } = run;
        let clear_color = SDL_FColor {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        };
        // With MSAA the samples resolve into the target the overlay samples.
        let colour = if target.samples.is_null() {
            SDL_GPUColorTargetInfo {
                texture: target.colour,
                clear_color,
                load_op: SDL_GPU_LOADOP_CLEAR,
                store_op: SDL_GPU_STOREOP_STORE,
                cycle: true,
                ..Default::default()
            }
        } else {
            SDL_GPUColorTargetInfo {
                texture: target.samples,
                clear_color,
                load_op: SDL_GPU_LOADOP_CLEAR,
                store_op: SDL_GPU_STOREOP_RESOLVE,
                resolve_texture: target.colour,
                cycle: true,
                cycle_resolve_texture: true,
                ..Default::default()
            }
        };
        let depth = SDL_GPUDepthStencilTargetInfo {
            texture: target.depth,
            clear_depth: 1.0,
            load_op: SDL_GPU_LOADOP_CLEAR,
            store_op: SDL_GPU_STOREOP_DONT_CARE,
            stencil_load_op: SDL_GPU_LOADOP_DONT_CARE,
            stencil_store_op: SDL_GPU_STOREOP_DONT_CARE,
            cycle: true,
            ..Default::default()
        };
        let pass = unsafe { SDL_BeginGPURenderPass(command, &colour, 1, &depth) };
        if pass.is_null() {
            bail!("SDL_BeginGPURenderPass failed: {}", sdl_error());
        }
        let binding = SDL_GPUBufferBinding {
            buffer: self.model_buffer,
            offset: 0,
        };
        let mut material = scene::material(None, false);
        let hd = if page.hd.is_null() {
            [self.no_hd, self.no_hd_rows]
        } else {
            [page.hd, page.hd_rows]
        };
        let samplers =
            [page.indices, page.palette, hd[0], hd[1], self.no_mask_keys].map(|texture| {
                SDL_GPUTextureSamplerBinding {
                    texture,
                    sampler: self.sampler,
                }
            });
        for blend_pass in [false, true] {
            let pipeline = if blend_pass {
                self.pipelines.model_blend
            } else {
                self.pipelines.model_opaque
            };
            material[1] = if blend_pass { 1.0 } else { 0.0 };
            material[2] = if page.hd.is_null() { 0.0 } else { 1.0 };
            unsafe {
                SDL_BindGPUGraphicsPipeline(pass, pipeline);
                SDL_PushGPUFragmentUniformData(
                    command,
                    0,
                    material.as_ptr().cast::<c_void>(),
                    std::mem::size_of_val(&material) as u32,
                );
                SDL_BindGPUVertexBuffers(pass, 0, &binding, 1);
                SDL_BindGPUFragmentSamplers(pass, 0, samplers.as_ptr(), samplers.len() as u32);
                SDL_DrawGPUPrimitives(pass, count, 1, first, 0);
            }
        }
        unsafe { SDL_EndGPURenderPass(pass) };
        Ok(())
    }

    /// Release the 2D replacements not drawn in the last
    /// [`EVICT_AFTER_FRAMES`] presented frames, and the model targets when
    /// no model was.
    fn evict_overlay_images(&mut self) {
        let frame = self.overlay_frame;
        if frame.saturating_sub(self.model_frame) > EVICT_AFTER_FRAMES {
            self.release_model_targets();
        }
        fn release_stale<T>(
            device: *mut SDL_GPUDevice,
            frame: u64,
            textures: &mut HashMap<usize, (T, *mut SDL_GPUTexture, u64)>,
        ) {
            textures.retain(|_, (_, texture, last_used)| {
                let keep = frame.saturating_sub(*last_used) <= EVICT_AFTER_FRAMES;
                if !keep {
                    unsafe { SDL_ReleaseGPUTexture(device, *texture) };
                }
                keep
            });
        }
        release_stale(self.device, frame, &mut self.overlay_images);
        release_stale(self.device, frame, &mut self.overlay_indices);
    }

    /// Upload `indices` for this frame's recoloured quads unless they are
    /// resident, noting a new texture in `fresh`.
    fn ensure_overlay_indices(
        &mut self,
        copy: *mut SDL_GPUCopyPass,
        indices: &Arc<htexture::Indices>,
        fresh: &mut Vec<usize>,
    ) -> Result<()> {
        let key = Arc::as_ptr(indices) as usize;
        if let Some(entry) = self.overlay_indices.get_mut(&key) {
            entry.2 = self.overlay_frame;
            return Ok(());
        }
        let (width, height) = (indices.width.max(1), indices.height.max(1));
        let mut texels = indices.texels.clone();
        texels.resize(width as usize * height as usize, 0);
        let texture = self.texture(
            SDL_GPU_TEXTUREFORMAT_R8_UINT,
            width,
            height,
            SDL_GPU_TEXTUREUSAGE_SAMPLER,
        )?;
        if let Err(err) = self.upload_texture(copy, texture, width, height, as_bytes(&texels)) {
            unsafe { SDL_ReleaseGPUTexture(self.device, texture) };
            return Err(err);
        }
        self.overlay_indices
            .insert(key, (indices.clone(), texture, self.overlay_frame));
        fresh.push(key);
        Ok(())
    }

    /// Make `overlay_lut` hold at least `rows` rows.
    fn ensure_overlay_lut(&mut self, rows: u32) -> Result<()> {
        if !self.overlay_lut.is_null() && self.overlay_lut_rows >= rows {
            return Ok(());
        }
        let rows = rows.next_power_of_two().max(LUT_ROWS);
        let texture = self.texture(
            SDL_GPU_TEXTUREFORMAT_R32G32B32A32_FLOAT,
            crate::model::PALETTE_ROW_LEN as u32,
            rows,
            SDL_GPU_TEXTUREUSAGE_SAMPLER,
        )?;
        if !self.overlay_lut.is_null() {
            unsafe { SDL_ReleaseGPUTexture(self.device, self.overlay_lut) };
        }
        self.overlay_lut = texture;
        self.overlay_lut_rows = rows;
        Ok(())
    }

    /// Record [`GpuRenderer::draw_list`]'s uploads and passes into
    /// `command`, noting the textures it creates in `fresh`.
    fn record_list(
        &mut self,
        command: *mut SDL_GPUCommandBuffer,
        list: &DrawList,
        slot: usize,
        (target, (target_w, target_h), opaque): (*mut SDL_GPUTexture, (u32, u32), bool),
        fresh: &mut Vec<usize>,
    ) -> Result<()> {
        let [frame_w, frame_h] = list.size.map(|side| side.max(1));
        let copy = unsafe { SDL_BeginGPUCopyPass(command) };
        if copy.is_null() {
            bail!("SDL_BeginGPUCopyPass failed: {}", sdl_error());
        }
        let uploads = (|| -> Result<(Vec<ItemVertex>, Vec<ItemDraw>, Vec<ModelRun>)> {
            self.upload_atlas(copy, slot, &list.atlas)?;
            let mut vertices = Vec::with_capacity(list.items.len() * 6);
            let mut draws = Vec::with_capacity(list.items.len());
            let mut models = Vec::new();
            let mut model_vertices = Vec::new();
            let mut lut = Vec::new();
            let mut lut_rows: Vec<(usize, u32)> = Vec::new();
            for item in &list.items {
                if item.map.flags & draw2d::PUNCH != 0 {
                    continue;
                }
                let mut lut_row = 0;
                let mut index_frame = [0.0; 2];
                let (mode, image) = match &item.kind {
                    draw2d::Kind::Solid => (MODE_SOLID, ItemImage::None),
                    draw2d::Kind::Sprite => (MODE_SPRITE, ItemImage::None),
                    draw2d::Kind::Scene => (MODE_PREMULTIPLIED, ItemImage::Scene),
                    draw2d::Kind::Art(art) => {
                        self.ensure_overlay_image(copy, art, fresh)?;
                        (MODE_ART, ItemImage::Art(Arc::as_ptr(art) as usize, None))
                    }
                    draw2d::Kind::Recoloured(recoloured) => {
                        self.ensure_overlay_image(copy, &recoloured.art, fresh)?;
                        self.ensure_overlay_indices(copy, &recoloured.indices, fresh)?;
                        // Items sharing a recolouring share its rows.
                        let key = Arc::as_ptr(recoloured) as usize;
                        lut_row = match lut_rows.iter().find(|&&(at, _)| at == key) {
                            Some(&(_, row)) => row,
                            None => {
                                let row = (lut.len() / crate::model::PALETTE_ROW_LEN) as u32;
                                lut.extend(lut_texels(&recoloured.lut));
                                lut_rows.push((key, row));
                                row
                            }
                        };
                        index_frame = recoloured.art.frame.map(|side| side as f32);
                        let indices = Arc::as_ptr(&recoloured.indices) as usize;
                        let art = Arc::as_ptr(&recoloured.art) as usize;
                        (MODE_RECOLOURED, ItemImage::Art(art, Some(indices)))
                    }
                    draw2d::Kind::Model(model) => {
                        let page = self.ensure_page(
                            copy,
                            &model.ivm.texture,
                            0,
                            None,
                            model.page.as_ref(),
                        )?;
                        let count = model.vertices.len() as u32;
                        let Some(page) = page.filter(|_| count > 0) else {
                            continue;
                        };
                        let first = model_vertices.len() as u32;
                        model_vertices.extend_from_slice(&model.vertices);
                        // The item's own rect at the target's resolution.
                        let [x0, y0, x1, y1] = item.rect;
                        let side = |extent: f32, target: u32, frame: u32| {
                            ((extent * target as f32 / frame as f32).ceil() as u32)
                                .clamp(1, MAX_SCENE_SIDE)
                        };
                        let size = (
                            side(x1 - x0, target_w, frame_w),
                            side(y1 - y0, target_h, frame_h),
                        );
                        let run = ModelRun {
                            page,
                            first,
                            count,
                            slot: models.len(),
                        };
                        models.push((run, size));
                        (MODE_PREMULTIPLIED, ItemImage::Model(run.slot))
                    }
                };
                let first = vertices.len() as u32;
                vertices.extend(item_vertices(item, mode, lut_row, index_frame));
                draws.push(ItemDraw { image, first });
            }
            if !lut.is_empty() {
                let rows = (lut.len() / crate::model::PALETTE_ROW_LEN) as u32;
                self.ensure_overlay_lut(rows)?;
                let width = crate::model::PALETTE_ROW_LEN as u32;
                self.upload_texture(copy, self.overlay_lut, width, rows, as_bytes(&lut))?;
            }
            let bytes = as_bytes(&vertices);
            if !bytes.is_empty() {
                if self.overlay_capacity < bytes.len() as u32 {
                    let capacity = (bytes.len() as u32).next_power_of_two();
                    let buffer = self.vertex_buffer(capacity)?;
                    if !self.overlay_buffer.is_null() {
                        unsafe { SDL_ReleaseGPUBuffer(self.device, self.overlay_buffer) };
                    }
                    self.overlay_buffer = buffer;
                    self.overlay_capacity = capacity;
                }
                self.upload_buffer(copy, self.overlay_buffer, bytes, true)?;
            }
            let bytes = as_bytes(&model_vertices);
            if !bytes.is_empty() {
                if self.model_capacity < bytes.len() as u32 {
                    let capacity = (bytes.len() as u32).next_power_of_two();
                    let buffer = self.vertex_buffer(capacity)?;
                    if !self.model_buffer.is_null() {
                        unsafe { SDL_ReleaseGPUBuffer(self.device, self.model_buffer) };
                    }
                    self.model_buffer = buffer;
                    self.model_capacity = capacity;
                }
                self.upload_buffer(copy, self.model_buffer, bytes, true)?;
            }
            for &(run, size) in &models {
                self.ensure_model_target(run.slot, size)?;
            }
            Ok((
                vertices,
                draws,
                models.into_iter().map(|(run, _)| run).collect(),
            ))
        })();
        unsafe { SDL_EndGPUCopyPass(copy) };
        let (vertices, draws, models) = uploads?;

        for run in &models {
            self.render_model(command, &self.model_targets[run.slot], *run)?;
            self.model_frame = self.overlay_frame;
        }

        let target = SDL_GPUColorTargetInfo {
            texture: target,
            clear_color: SDL_FColor {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: if opaque { 1.0 } else { 0.0 },
            },
            load_op: SDL_GPU_LOADOP_CLEAR,
            store_op: SDL_GPU_STOREOP_STORE,
            cycle: true,
            ..Default::default()
        };
        let pass = unsafe { SDL_BeginGPURenderPass(command, &target, 1, std::ptr::null()) };
        if pass.is_null() {
            bail!("SDL_BeginGPURenderPass failed: {}", sdl_error());
        }
        if vertices.is_empty() {
            unsafe { SDL_EndGPURenderPass(pass) };
            return Ok(());
        }
        let viewport = SDL_GPUViewport {
            x: 0.0,
            y: 0.0,
            w: target_w as f32,
            h: target_h as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };
        let binding = SDL_GPUBufferBinding {
            buffer: self.overlay_buffer,
            offset: 0,
        };
        let frame_size: [f32; 4] = [frame_w as f32, frame_h as f32, 0.0, 0.0];
        let frame_per_target: [f32; 4] = [
            frame_w as f32 / target_w as f32,
            frame_h as f32 / target_h as f32,
            0.0,
            0.0,
        ];
        unsafe {
            SDL_BindGPUGraphicsPipeline(pass, self.pipelines.draw2d);
            SDL_SetGPUViewport(pass, &viewport);
            SDL_BindGPUVertexBuffers(pass, 0, &binding, 1);
            SDL_PushGPUVertexUniformData(
                command,
                0,
                frame_size.as_ptr().cast::<c_void>(),
                std::mem::size_of_val(&frame_size) as u32,
            );
            SDL_PushGPUFragmentUniformData(
                command,
                0,
                frame_per_target.as_ptr().cast::<c_void>(),
                std::mem::size_of_val(&frame_per_target) as u32,
            );
        }
        let or_none = |texture: *mut SDL_GPUTexture| {
            if texture.is_null() {
                self.no_image
            } else {
                texture
            }
        };
        let atlas = or_none(self.atlases[slot].texture);
        let lut = or_none(self.overlay_lut);
        let textures = |image: ItemImage| -> [*mut SDL_GPUTexture; 2] {
            let resident = |key: usize| {
                self.overlay_images
                    .get(&key)
                    .map_or(self.no_image, |(_, texture, _)| *texture)
            };
            match image {
                ItemImage::None => [self.no_image, self.no_indices],
                ItemImage::Scene => [self.scene_colour, self.no_indices],
                ItemImage::Model(slot) => [self.model_targets[slot].colour, self.no_indices],
                ItemImage::Art(art, indices) => [
                    resident(art),
                    indices
                        .and_then(|key| self.overlay_indices.get(&key))
                        .map_or(self.no_indices, |(_, texture, _)| *texture),
                ],
            }
        };
        // Runs of items drawing from the same textures are one draw.
        let mut start = 0;
        while start < draws.len() {
            let bound = textures(draws[start].image);
            let mut end = start + 1;
            while end < draws.len() && textures(draws[end].image) == bound {
                end += 1;
            }
            let first = draws[start].first;
            let count = draws
                .get(end)
                .map_or(vertices.len() as u32, |draw| draw.first)
                - first;
            let samplers = [
                (atlas, self.sampler),
                (bound[0], self.linear),
                (bound[1], self.sampler),
                (lut, self.sampler),
            ]
            .map(|(texture, sampler)| SDL_GPUTextureSamplerBinding { texture, sampler });
            unsafe {
                SDL_BindGPUFragmentSamplers(pass, 0, samplers.as_ptr(), samplers.len() as u32);
                SDL_DrawGPUPrimitives(pass, count, 1, first, 0);
            }
            start = end;
        }
        unsafe { SDL_EndGPURenderPass(pass) };
        Ok(())
    }

    /// Present `layer`, `size` in pixels, into the viewport `place` returns
    /// for the swapchain size, clipped to its rectangle.
    fn composite(
        &mut self,
        layer: Layer<'_>,
        (layer_w, layer_h): (u32, u32),
        place: impl FnOnce(u32, u32) -> ((f32, f32, f32, f32), video::Viewport),
    ) -> Result<()> {
        if let Layer::Upload(image) = layer {
            self.ensure_layer(layer_w, layer_h)?;
            // Whatever it held is replaced.
            self.layer_image = None;
            // Cycling the staging buffer and the texture lets this upload
            // proceed while the previous present may still read them.
            let mapped =
                unsafe { SDL_MapGPUTransferBuffer(self.device, self.layer_transfer, true) };
            if mapped.is_null() {
                bail!("SDL_MapGPUTransferBuffer failed: {}", sdl_error());
            }
            let bytes = (layer_w * layer_h * 4) as usize;
            unsafe {
                std::ptr::copy_nonoverlapping(image.as_ptr(), mapped.cast::<u8>(), bytes);
                SDL_UnmapGPUTransferBuffer(self.device, self.layer_transfer);
            }
        }
        let command = self.command_buffer()?;
        let texture = match layer {
            Layer::Upload(_) => {
                self.upload_layer_image(command, (layer_w, layer_h))?;
                self.layer
            }
            Layer::Kept => self.layer,
            Layer::Target => self.overlay_target,
        };
        self.composite_pass(command, texture, place)?;
        Self::submit(command)
    }

    /// Upload the staged image into `layer`.
    fn upload_layer_image(
        &self,
        command: *mut SDL_GPUCommandBuffer,
        (layer_w, layer_h): (u32, u32),
    ) -> Result<()> {
        let copy = unsafe { SDL_BeginGPUCopyPass(command) };
        if copy.is_null() {
            unsafe { SDL_CancelGPUCommandBuffer(command) };
            bail!("SDL_BeginGPUCopyPass failed: {}", sdl_error());
        }
        let source = SDL_GPUTextureTransferInfo {
            transfer_buffer: self.layer_transfer,
            offset: 0,
            pixels_per_row: layer_w,
            rows_per_layer: layer_h,
        };
        let destination = SDL_GPUTextureRegion {
            texture: self.layer,
            w: layer_w,
            h: layer_h,
            d: 1,
            ..Default::default()
        };
        unsafe {
            SDL_UploadToGPUTexture(copy, &source, &destination, true);
            SDL_EndGPUCopyPass(copy);
        }
        Ok(())
    }

    /// Record the composite of `frame` into the swapchain, placed by
    /// `place`. The command buffer is cancelled on failure and left to the
    /// caller to submit otherwise.
    fn composite_pass(
        &self,
        command: *mut SDL_GPUCommandBuffer,
        frame: *mut SDL_GPUTexture,
        place: impl FnOnce(u32, u32) -> ((f32, f32, f32, f32), video::Viewport),
    ) -> Result<()> {
        // Never wait for an image: with every image in flight (or a hidden
        // window) this frame is skipped and the uploads still run.
        let mut swapchain = std::ptr::null_mut();
        let (mut width, mut height) = (0, 0);
        if !unsafe {
            SDL_AcquireGPUSwapchainTexture(
                command,
                self.window,
                &mut swapchain,
                &mut width,
                &mut height,
            )
        } {
            unsafe { SDL_CancelGPUCommandBuffer(command) };
            bail!("SDL_AcquireGPUSwapchainTexture failed: {}", sdl_error());
        }
        if swapchain.is_null() {
            // Nothing to draw to: keep a vsync-paced loop from spinning.
            if !self.mailbox {
                std::thread::sleep(HIDDEN_PRESENT_WAIT);
            }
        } else {
            let target = SDL_GPUColorTargetInfo {
                texture: swapchain,
                clear_color: SDL_FColor {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                },
                load_op: SDL_GPU_LOADOP_CLEAR,
                store_op: SDL_GPU_STOREOP_STORE,
                ..Default::default()
            };
            let pass = unsafe { SDL_BeginGPURenderPass(command, &target, 1, std::ptr::null()) };
            if !pass.is_null() {
                let sampler = SDL_GPUTextureSamplerBinding {
                    texture: frame,
                    sampler: self.linear,
                };
                let ((x, y, w, h), fit) = place(width, height);
                let viewport = SDL_GPUViewport {
                    x,
                    y,
                    w,
                    h,
                    min_depth: 0.0,
                    max_depth: 1.0,
                };
                let scissor = SDL_Rect {
                    x: fit.x as i32,
                    y: fit.y as i32,
                    w: fit.width as i32,
                    h: fit.height as i32,
                };
                unsafe {
                    SDL_BindGPUGraphicsPipeline(pass, self.pipelines.composite);
                    SDL_SetGPUViewport(pass, &viewport);
                    SDL_SetGPUScissor(pass, &scissor);
                    SDL_BindGPUFragmentSamplers(pass, 0, &sampler, 1);
                    SDL_DrawGPUPrimitives(pass, 3, 1, 0, 0);
                }
                if self.ui_shown && !self.pipelines.ui.is_null() {
                    // The canvas fills the rectangle a frame of its shape
                    // fits, which a wider aspect's band shares.
                    let fit = video::fit_size(width, height, self.ui_size.0, self.ui_size.1);
                    let viewport = SDL_GPUViewport {
                        x: fit.x as f32,
                        y: fit.y as f32,
                        w: fit.width as f32,
                        h: fit.height as f32,
                        min_depth: 0.0,
                        max_depth: 1.0,
                    };
                    let scissor = SDL_Rect {
                        x: fit.x as i32,
                        y: fit.y as i32,
                        w: fit.width as i32,
                        h: fit.height as i32,
                    };
                    let sampler = SDL_GPUTextureSamplerBinding {
                        texture: self.ui_texture,
                        sampler: self.linear,
                    };
                    unsafe {
                        SDL_BindGPUGraphicsPipeline(pass, self.pipelines.ui);
                        SDL_SetGPUViewport(pass, &viewport);
                        SDL_SetGPUScissor(pass, &scissor);
                        SDL_BindGPUFragmentSamplers(pass, 0, &sampler, 1);
                        SDL_DrawGPUPrimitives(pass, 3, 1, 0, 0);
                    }
                }
                if self.native_shown && !self.pipelines.ui.is_null() {
                    // Drawn at the canvas's own size from the top left, so
                    // a canvas matching the swapchain maps texel for pixel.
                    let viewport = SDL_GPUViewport {
                        x: 0.0,
                        y: 0.0,
                        w: self.native_size.0 as f32,
                        h: self.native_size.1 as f32,
                        min_depth: 0.0,
                        max_depth: 1.0,
                    };
                    let scissor = SDL_Rect {
                        x: 0,
                        y: 0,
                        w: width.min(self.native_size.0) as i32,
                        h: height.min(self.native_size.1) as i32,
                    };
                    let sampler = SDL_GPUTextureSamplerBinding {
                        texture: self.native_texture,
                        sampler: self.linear,
                    };
                    unsafe {
                        SDL_BindGPUGraphicsPipeline(pass, self.pipelines.ui);
                        SDL_SetGPUViewport(pass, &viewport);
                        SDL_SetGPUScissor(pass, &scissor);
                        SDL_BindGPUFragmentSamplers(pass, 0, &sampler, 1);
                        SDL_DrawGPUPrimitives(pass, 3, 1, 0, 0);
                    }
                }
                self.draw_gui(command, pass, (width, height));
                unsafe { SDL_EndGPURenderPass(pass) };
            }
        }
        Ok(())
    }

    /// Read the scene target back as RGBA rows (tests and tools).
    pub fn read_scene(&self) -> Result<Vec<u8>> {
        self.read_texture(self.scene_colour, self.scene_size())
    }

    /// Read the frame the 2D pass last drew back as RGBA rows, and its size
    /// (tests and tools).
    pub fn read_frame(&self) -> Result<(Vec<u8>, (u32, u32))> {
        if self.overlay_target.is_null() {
            bail!("no frame has been drawn");
        }
        let size = self.overlay_size;
        Ok((self.read_texture(self.overlay_target, size)?, size))
    }

    /// Read a `width` x `height` RGBA texture back as rows.
    fn read_texture(
        &self,
        texture: *mut SDL_GPUTexture,
        (width, height): (u32, u32),
    ) -> Result<Vec<u8>> {
        let size = width * height * 4;
        let info = SDL_GPUTransferBufferCreateInfo {
            usage: SDL_GPU_TRANSFERBUFFERUSAGE_DOWNLOAD,
            size,
            ..Default::default()
        };
        let transfer = unsafe { SDL_CreateGPUTransferBuffer(self.device, &info) };
        if transfer.is_null() {
            bail!("SDL_CreateGPUTransferBuffer failed: {}", sdl_error());
        }
        let result = (|| -> Result<Vec<u8>> {
            let command = self.command_buffer()?;
            let copy = unsafe { SDL_BeginGPUCopyPass(command) };
            if copy.is_null() {
                unsafe { SDL_CancelGPUCommandBuffer(command) };
                bail!("SDL_BeginGPUCopyPass failed: {}", sdl_error());
            }
            let source = SDL_GPUTextureRegion {
                texture,
                w: width,
                h: height,
                d: 1,
                ..Default::default()
            };
            let destination = SDL_GPUTextureTransferInfo {
                transfer_buffer: transfer,
                offset: 0,
                pixels_per_row: width,
                rows_per_layer: height,
            };
            unsafe {
                SDL_DownloadFromGPUTexture(copy, &source, &destination);
                SDL_EndGPUCopyPass(copy);
            }
            let fence = unsafe { SDL_SubmitGPUCommandBufferAndAcquireFence(command) };
            if fence.is_null() {
                bail!(
                    "SDL_SubmitGPUCommandBufferAndAcquireFence failed: {}",
                    sdl_error()
                );
            }
            let waited = unsafe { SDL_WaitForGPUFences(self.device, true, &fence, 1) };
            unsafe { SDL_ReleaseGPUFence(self.device, fence) };
            if !waited {
                bail!("SDL_WaitForGPUFences failed: {}", sdl_error());
            }
            let mapped = unsafe { SDL_MapGPUTransferBuffer(self.device, transfer, false) };
            if mapped.is_null() {
                bail!("SDL_MapGPUTransferBuffer failed: {}", sdl_error());
            }
            let mut out = vec![0u8; size as usize];
            unsafe {
                std::ptr::copy_nonoverlapping(mapped.cast::<u8>(), out.as_mut_ptr(), out.len());
                SDL_UnmapGPUTransferBuffer(self.device, transfer);
            }
            Ok(out)
        })();
        unsafe { SDL_ReleaseGPUTransferBuffer(self.device, transfer) };
        result
    }
}

/// Bind and draw one mesh range.
#[allow(clippy::too_many_arguments)]
fn draw_mesh(
    command: *mut SDL_GPUCommandBuffer,
    pass: *mut SDL_GPURenderPass,
    pipeline: *mut SDL_GPUGraphicsPipeline,
    uniforms: &DrawUniforms,
    material: [f32; 4],
    buffer: *mut SDL_GPUBuffer,
    textures: [*mut SDL_GPUTexture; 5],
    sampler: *mut SDL_GPUSampler,
    first: u32,
    count: u32,
) {
    let bytes = uniforms.as_bytes();
    let binding = SDL_GPUBufferBinding { buffer, offset: 0 };
    let samplers = textures.map(|texture| SDL_GPUTextureSamplerBinding { texture, sampler });
    unsafe {
        SDL_BindGPUGraphicsPipeline(pass, pipeline);
        SDL_PushGPUVertexUniformData(
            command,
            0,
            bytes.as_ptr().cast::<c_void>(),
            bytes.len() as u32,
        );
        SDL_PushGPUFragmentUniformData(
            command,
            0,
            material.as_ptr().cast::<c_void>(),
            std::mem::size_of_val(&material) as u32,
        );
        SDL_BindGPUVertexBuffers(pass, 0, &binding, 1);
        SDL_BindGPUFragmentSamplers(pass, 0, samplers.as_ptr(), samplers.len() as u32);
        SDL_DrawGPUPrimitives(pass, count, 1, first, 0);
    }
}

/// The two triangles of `item`, drawn in `mode` (the map's flags joined to
/// it) from `lut_row` of the recolourings.
fn item_vertices(
    item: &draw2d::Item,
    mode: u32,
    lut_row: u32,
    index_frame: [f32; 2],
) -> [ItemVertex; 6] {
    let map = item.map;
    let corner = |x: f32, y: f32, u: f32, v: f32| ItemVertex {
        position: [x, y],
        uv: [u, v],
        colour: item.colour,
        dst: map.dst,
        source: [map.src[0], map.src[1], map.atlas[0], map.atlas[1]],
        clip: item.clip,
        mode: [mode | map.flags << 8, lut_row],
        index_frame,
    };
    let [x0, y0, x1, y1] = item.rect;
    let [u0, v0, u1, v1] = item.uv;
    [
        corner(x0, y0, u0, v0),
        corner(x1, y0, u1, v0),
        corner(x1, y1, u1, v1),
        corner(x0, y0, u0, v0),
        corner(x1, y1, u1, v1),
        corner(x0, y1, u0, v1),
    ]
}

/// A recoloured item's [`LUT_ROWS`] rows of `draw2d.frag`'s `lut`.
fn lut_texels(lut: &[Option<htexture::Recolour>; 256]) -> Vec<[f32; 4]> {
    let row = |texel: fn(&htexture::Recolour) -> [f32; 4]| {
        lut.iter()
            .map(move |entry| entry.as_ref().map_or([0.0; 4], texel))
    };
    row(|recolour| {
        let [r, g, b] = recolour.scale;
        [r, g, b, 1.0]
    })
    .chain(row(|recolour| {
        let [r, g, b] = recolour.luma;
        [r, g, b, 0.0]
    }))
    .chain(row(|recolour| {
        let [r, g, b] = recolour.flat;
        [r, g, b, 0.0]
    }))
    .collect()
}

/// A slice of plain `repr(C)` vertices as bytes.
fn as_bytes<T: scene::Plain>(values: &[T]) -> &[u8] {
    // SAFETY: `Plain` types have no padding, so every byte is initialised.
    unsafe {
        std::slice::from_raw_parts(values.as_ptr().cast::<u8>(), std::mem::size_of_val(values))
    }
}

impl Drop for GpuRenderer {
    fn drop(&mut self) {
        unsafe {
            SDL_WaitForGPUIdle(self.device);
        }
        self.release_residency();
        self.release_scene_targets();
        unsafe {
            let pipelines = [
                self.pipelines.mesh_opaque,
                self.pipelines.mesh_blend,
                self.pipelines.mask_keys,
                self.pipelines.sprite,
                self.pipelines.sprite_depth,
                self.pipelines.draw2d,
                self.pipelines.model_opaque,
                self.pipelines.model_blend,
                self.pipelines.composite,
                self.pipelines.ui,
                self.pipelines.gui,
            ];
            for pipeline in pipelines.into_iter().filter(|p| !p.is_null()) {
                SDL_ReleaseGPUGraphicsPipeline(self.device, pipeline);
            }
            let textures = [
                self.layer,
                self.ui_texture,
                self.native_texture,
                self.no_hd,
                self.no_hd_rows,
                self.no_mask_keys,
                self.overlay_target,
                self.atlases[FRAME_ATLAS].texture,
                self.atlases[UI_ATLAS].texture,
                self.atlases[GUI_ATLAS].texture,
                self.overlay_lut,
                self.no_indices,
                self.no_image,
            ];
            for texture in textures.into_iter().filter(|t| !t.is_null()) {
                SDL_ReleaseGPUTexture(self.device, texture);
            }
            for (_, (_, texture, _)) in std::mem::take(&mut self.overlay_images) {
                SDL_ReleaseGPUTexture(self.device, texture);
            }
            for (_, (_, texture, _)) in std::mem::take(&mut self.overlay_indices) {
                SDL_ReleaseGPUTexture(self.device, texture);
            }
            self.release_model_targets();
            for buffer in [
                self.sprite_buffer,
                self.overlay_buffer,
                self.model_buffer,
                self.gui.buffer,
            ] {
                if !buffer.is_null() {
                    SDL_ReleaseGPUBuffer(self.device, buffer);
                }
            }
            if !self.layer_transfer.is_null() {
                SDL_ReleaseGPUTransferBuffer(self.device, self.layer_transfer);
            }
            for sampler in [self.sampler, self.linear] {
                if !sampler.is_null() {
                    SDL_ReleaseGPUSampler(self.device, sampler);
                }
            }
            if !self.window.is_null() {
                SDL_ReleaseWindowFromGPUDevice(self.device, self.window);
            }
            SDL_DestroyGPUDevice(self.device);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim::Mat4x3;
    use crate::model::{TmdObject, TmdPrim};
    use crate::render::{self, Camera, EntityMesh, Lighting, SpriteBlend};

    /// A cube with a different palette colour on each face.
    fn cube() -> (Tmd, Texture8) {
        let s = 600i16;
        let vertices = vec![
            [-s, -s, -s],
            [s, -s, -s],
            [s, s, -s],
            [-s, s, -s],
            [-s, -s, s],
            [s, -s, s],
            [s, s, s],
            [-s, s, s],
        ];
        // Each face lists its corners anticlockwise seen from outside.
        let faces: [([u16; 4], [i16; 3]); 6] = [
            ([0, 3, 2, 1], [0, 0, -4096]),
            ([4, 5, 6, 7], [0, 0, 4096]),
            ([0, 4, 7, 3], [-4096, 0, 0]),
            ([1, 2, 6, 5], [4096, 0, 0]),
            ([0, 1, 5, 4], [0, -4096, 0]),
            ([3, 7, 6, 2], [0, 4096, 0]),
        ];
        let mut prims = Vec::new();
        for (face, (corners, _)) in faces.iter().enumerate() {
            // Face `face` samples the 8x8 cell at u = face * 8, index face + 1.
            let u = (face * 8 + 4) as u8;
            for triangle in [[0, 1, 2], [0, 2, 3]] {
                prims.push(TmdPrim {
                    vertices: triangle.map(|corner| corners[corner]),
                    normals: [face as u16; 3],
                    uv: [[u, 4]; 3],
                    clut: 480 << 6,
                    tsb: 0,
                    textured: true,
                    blend: false,
                    raw_y: false,
                    uv_nudge: [0; 3],
                    flat_color: None,
                    quad: None,
                });
            }
        }
        let mesh = Tmd {
            objects: vec![TmdObject {
                vertices,
                normals: faces.iter().map(|(_, normal)| *normal).collect(),
                prims,
            }],
        };
        let (width, height) = (64u32, 8u32);
        let indices = (0..width * height)
            .map(|i| ((i % width) / 8 + 1) as u8)
            .collect();
        let mut palettes = vec![[0u8; 4]; crate::model::PALETTE_ROW_LEN];
        let colours = [
            [200, 40, 40],
            [40, 200, 40],
            [40, 40, 200],
            [200, 200, 40],
            [200, 40, 200],
            [40, 200, 200],
        ];
        for (index, colour) in colours.iter().enumerate() {
            palettes[index + 1] = [colour[0], colour[1], colour[2], 255];
        }
        let texture = Texture8 {
            width,
            height,
            indices,
            palettes,
            stp: vec![false; crate::model::PALETTE_ROW_LEN],
        };
        (mesh, texture)
    }

    /// The GPU scene agrees with the software rasterizer on a lit, rotated
    /// cube: same faces culled, same palette lookups, close shading. Skips
    /// when the machine has no SDL video or no SPIR-V capable GPU (CI).
    #[test]
    fn gpu_scene_matches_the_software_rasterizer() {
        let _lock = crate::audio::test_lock::sdl();
        let Ok(_sdl) = crate::engine::SdlHandle::acquire() else {
            eprintln!("skipping: SDL video unavailable");
            return;
        };
        let mut gpu = match GpuRenderer::offscreen(1) {
            Ok(gpu) => gpu,
            Err(err) => {
                eprintln!("skipping: {err:#}");
                return;
            }
        };
        let (mesh, texture) = cube();
        let joints = [Mat4x3 {
            r: crate::anim::rotation_matrix(200, 500, 0),
            t: [100, -700, 0],
        }];
        let meshes = [EntityMesh {
            mesh: &mesh,
            texture: &texture,
            joints: &joints,
            tint: [255; 3],
            blend_weight: None,
            hidden_joints: 0,
            light_pos: None,
            per_joint_light: false,
            light_rotation: None,
            joint_colours: crate::render::NO_JOINT_COLOURS,
            hd_name: None,
            complex_object: None,
            aliased_light: false,
        }];
        let camera = Camera::from_points([2600, -2400, 5200], [0, -700, 0], 600);
        let lighting = Lighting {
            ambient: [3000, 3000, 3000],
            lights: Default::default(),
        };
        let scene = GameplayScene {
            background: None,
            origin: [0, 0],
            colour: [0x80; 3],
            meshes: &meshes,
            shadows: &[],
            camera: &camera,
            lighting: &lighting,
            mask_layer: None,
            effect_layer: None,
            mirror_from: meshes.len(),
            mirror: None,
            debug: &[],
        };
        let mut software = Framebuffer::new();
        scene.draw(&mut software);
        gpu.record_scene(&scene, None).unwrap();
        let hardware = gpu.read_scene().unwrap();
        assert_eq!(hardware.len(), software.rgba.len());

        let mut covered = 0;
        let mut different = 0;
        for (sw, hw) in software.rgba.chunks(4).zip(hardware.chunks(4)) {
            let lit = |pixel: &[u8]| pixel[..3].iter().any(|&c| c != 0);
            if lit(sw) || lit(hw) {
                covered += 1;
                let distance = (0..3)
                    .map(|c| i32::from(sw[c]).abs_diff(i32::from(hw[c])))
                    .max()
                    .unwrap_or(0);
                if distance > 24 {
                    different += 1;
                }
            }
        }
        eprintln!("{different} of {covered} pixels differ");
        if let Ok(dir) = std::env::var("ARKLAY_GPU_TEST_DUMP") {
            std::fs::write(format!("{dir}/sw.rgba"), &software.rgba).unwrap();
            std::fs::write(format!("{dir}/hw.rgba"), &hardware).unwrap();
        }
        assert!(covered > 5000, "the cube covers only {covered} pixels");
        // Only the edges may disagree (different fill rules).
        assert!(
            different * 20 < covered,
            "{different} of {covered} pixels differ"
        );

        // The cube stays resident across a frame that does not draw it, and
        // leaves once a room change retires it.
        let resident = gpu.resident_counts();
        assert_eq!((resident.0, resident.1), (1, 1));
        let empty = GameplayScene {
            meshes: &[],
            mirror_from: 0,
            ..scene
        };
        gpu.record_scene(&empty, None).unwrap();
        assert_eq!(gpu.resident_counts(), resident);
        gpu.retire();
        gpu.record_scene(&empty, None).unwrap();
        assert_eq!(gpu.resident_counts(), (0, 0, 0));
    }

    /// A background is fingerprinted each frame, so an in-place edit is
    /// seen and uploaded again.
    #[test]
    fn gpu_image_fingerprint_sees_an_in_place_edit() {
        let _lock = crate::audio::test_lock::sdl();
        let Ok(_sdl) = crate::engine::SdlHandle::acquire() else {
            eprintln!("skipping: SDL video unavailable");
            return;
        };
        let mut gpu = match GpuRenderer::offscreen(1) {
            Ok(gpu) => gpu,
            Err(err) => {
                eprintln!("skipping: {err:#}");
                return;
            }
        };
        let camera = Camera::from_points([0, 0, 1000], [0, 0, 0], 600);
        let lighting = Lighting {
            ambient: [4096; 3],
            lights: Default::default(),
        };
        let mut background = Image {
            width: 320,
            height: 240,
            rgba: [200, 0, 0, 255].repeat(320 * 240),
        };
        let mut draw = |background: &Image| {
            let scene = GameplayScene {
                background: Some(background),
                origin: [0, 0],
                colour: [255; 3],
                meshes: &[],
                shadows: &[],
                camera: &camera,
                lighting: &lighting,
                mask_layer: None,
                effect_layer: None,
                mirror_from: 0,
                mirror: None,
                debug: &[],
            };
            gpu.record_scene(&scene, None).unwrap();
            assert_eq!(gpu.resident_counts().2, 1);
            let pixels = gpu.read_scene().unwrap();
            let (width, height) = gpu.scene_size();
            let centre = ((height / 2 * width + width / 2) * 4) as usize;
            [pixels[centre], pixels[centre + 1], pixels[centre + 2]]
        };
        let paint = |background: &mut Image, colour: [u8; 4]| {
            for pixel in background.rgba.chunks_mut(4) {
                pixel.copy_from_slice(&colour);
            }
        };
        assert_eq!(draw(&background), [200, 0, 0]);
        paint(&mut background, [0, 200, 0, 255]);
        assert_eq!(draw(&background), [0, 200, 0]);
    }

    /// A pack's `htexture/` replacements are drawn in place of the background
    /// and the model page whose pixels hash to their names, at their own
    /// resolution.
    #[test]
    fn gpu_scene_draws_hd_replacements() {
        let _lock = crate::audio::test_lock::sdl();
        let Ok(_sdl) = crate::engine::SdlHandle::acquire() else {
            eprintln!("skipping: SDL video unavailable");
            return;
        };
        let mut gpu = match GpuRenderer::offscreen(1) {
            Ok(gpu) => gpu,
            Err(err) => {
                eprintln!("skipping: {err:#}");
                return;
            }
        };
        let solid = |width: u32, height: u32, colour: fn(u32) -> [u8; 3]| Image {
            width,
            height,
            rgba: (0..width * height)
                .flat_map(|i| {
                    let [r, g, b] = colour(i % width);
                    [r, g, b, 255]
                })
                .collect(),
        };
        let background = solid(320, 240, |x| [(x / 2) as u8, 64, 64]);
        // Twice the background's resolution: blue in the left half of the
        // original's texels, green in the right.
        let hd_background = solid(
            640,
            480,
            |x| if x < 320 { [0, 0, 240] } else { [0, 240, 0] },
        );
        let (mesh, texture) = cube();
        // The cube's 64-wide page covers 64 x 256 texels of its replacement.
        let hd_page = solid(128, 512, |_| [240, 240, 240]);
        let mut writer = crate::pack::PackWriter::new();
        let background_hash = htexture::image_candidates(&background)[0].hash;
        let page_hash = htexture::page_candidates(&texture, 0)[0].hash;
        for (path, image) in [
            (
                format!("htexture/bgd/{background_hash:08X}.bmp"),
                &hd_background,
            ),
            (format!("htexture/skin/{page_hash:08X}.bmp"), &hd_page),
        ] {
            writer
                .add(&path, crate::bmp::encode_to_vec(image).unwrap())
                .unwrap();
        }
        let pack = Pack::from_bytes(writer.to_bytes().unwrap()).unwrap();

        let joints = [Mat4x3 {
            r: crate::anim::rotation_matrix(200, 500, 0),
            t: [100, -700, 0],
        }];
        let meshes = [EntityMesh {
            mesh: &mesh,
            texture: &texture,
            joints: &joints,
            tint: [255; 3],
            blend_weight: None,
            hidden_joints: 0,
            light_pos: None,
            per_joint_light: false,
            light_rotation: None,
            joint_colours: crate::render::NO_JOINT_COLOURS,
            hd_name: None,
            complex_object: None,
            aliased_light: false,
        }];
        let camera = Camera::from_points([2600, -2400, 5200], [0, -700, 0], 600);
        let lighting = Lighting {
            ambient: [4096, 4096, 4096],
            lights: Default::default(),
        };
        let scene = GameplayScene {
            background: Some(&background),
            origin: [0, 0],
            colour: [255; 3],
            meshes: &meshes,
            shadows: &[],
            camera: &camera,
            lighting: &lighting,
            mask_layer: None,
            effect_layer: None,
            mirror_from: meshes.len(),
            mirror: None,
            debug: &[],
        };
        // Where the cube is: what its original draw changes.
        let mut draw = |scene: &GameplayScene<'_>, pack: Option<&Pack>| {
            gpu.release_residency();
            gpu.record_scene(scene, pack).unwrap();
            gpu.read_scene().unwrap()
        };
        let original = draw(&scene, None);
        let bare = draw(
            &GameplayScene {
                meshes: &[],
                mirror_from: 0,
                ..scene
            },
            None,
        );
        let hardware = draw(&scene, Some(&pack));
        let (mut cube_pixels, mut grey) = (0, 0);
        for (index, ((hd, original), bare)) in hardware
            .chunks(4)
            .zip(original.chunks(4))
            .zip(bare.chunks(4))
            .enumerate()
        {
            let x = index % LAYER_WIDTH as usize;
            let [r, g, b] = [hd[0], hd[1], hd[2]];
            if original != bare {
                // The cube: its faces' colours are replaced by grey.
                cube_pixels += 1;
                grey += usize::from(r.abs_diff(g) < 16 && g.abs_diff(b) < 16);
            } else if x < 150 {
                assert!(b > 200 && g < 40, "{index} is {:?}", [r, g, b]);
            } else if x > 170 {
                assert!(g > 200 && b < 40, "{index} is {:?}", [r, g, b]);
            }
        }
        assert!(
            cube_pixels > 5000,
            "the cube covers only {cube_pixels} pixels"
        );
        assert!(
            grey * 10 > cube_pixels * 9,
            "{grey} of {cube_pixels} cube pixels are grey"
        );
    }

    /// An item of the 2D pass over `rect`, cut to `clip`, sampling its
    /// image whole.
    fn item(kind: draw2d::Kind, rect: [f32; 4], clip: [i32; 4], colour: [f32; 4]) -> draw2d::Item {
        draw2d::Item {
            kind,
            rect,
            uv: [0.0, 0.0, 1.0, 1.0],
            clip,
            colour,
            map: draw2d::Map::default(),
        }
    }

    /// A 320x240 list of `items`.
    fn list_of(items: Vec<draw2d::Item>) -> DrawList {
        let mut list = DrawList::new([LAYER_WIDTH, LAYER_HEIGHT]);
        list.items = items;
        list
    }

    /// `overlay_target` read back, and a reader of its RGB at a frame
    /// pixel (`scale` target pixels per frame pixel).
    fn read_target(gpu: &GpuRenderer) -> (Vec<u8>, (u32, u32)) {
        let size = gpu.overlay_size;
        (gpu.read_texture(gpu.overlay_target, size).unwrap(), size)
    }

    fn rgb_at(pixels: &[u8], size: (u32, u32), scale: usize, x: usize, y: usize) -> [u8; 3] {
        let i = (scale * y * size.0 as usize + scale * x) * 4;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    }

    #[test]
    fn item_vertices_carry_the_item_to_each_corner() {
        let image = Arc::new(htexture::Replacement {
            image: Image {
                width: 1,
                height: 1,
                rgba: vec![255; 4],
            },
            frame: [1, 1],
        });
        let mut item = item(
            draw2d::Kind::Art(image),
            [0.0, 0.0, 160.0, 120.0],
            [1, 2, 3, 4],
            [0.5, 0.5, 0.5, 1.0],
        );
        // Mirrored horizontally: its u runs backwards.
        item.uv = [1.0, 0.0, 0.0, 1.0];
        item.map = draw2d::Map {
            dst: [5, 6, 7, 8],
            src: [9, 10],
            atlas: [11, 12],
            flags: draw2d::MASKED,
        };
        let vertices = item_vertices(&item, MODE_ART, 6, [3.0, 1.0]);
        let top_left = &vertices[0];
        assert_eq!(top_left.position, [0.0, 0.0]);
        assert_eq!(top_left.uv, [1.0, 0.0]);
        assert_eq!(vertices[1].position, [160.0, 0.0]);
        assert_eq!(vertices[1].uv, [0.0, 0.0]);
        assert_eq!(vertices[2].position, [160.0, 120.0]);
        assert!(vertices.iter().all(|vertex| {
            vertex.colour == [0.5, 0.5, 0.5, 1.0]
                && vertex.dst == [5, 6, 7, 8]
                && vertex.source == [9, 10, 11, 12]
                && vertex.clip == [1, 2, 3, 4]
                && vertex.mode == [MODE_ART | draw2d::MASKED << 8, 6]
                && vertex.index_frame == [3.0, 1.0]
        }));
        assert_eq!(std::mem::size_of::<ItemVertex>(), 96);
    }

    /// At one target pixel per frame pixel the 2D pass draws the frame the
    /// software rasterizer draws, but for the GPU's blend rounding.
    #[test]
    fn gpu_2d_pass_matches_the_software_frame() {
        let _lock = crate::audio::test_lock::sdl();
        let Ok(_sdl) = crate::engine::SdlHandle::acquire() else {
            eprintln!("skipping: SDL video unavailable");
            return;
        };
        let mut gpu = match GpuRenderer::offscreen(1) {
            Ok(gpu) => gpu,
            Err(err) => {
                eprintln!("skipping: {err:#}");
                return;
            }
        };
        // A 7x5 image of distinct colours with a clear column.
        let mut rgba = Vec::new();
        for y in 0..5u8 {
            for x in 0..7u8 {
                let alpha = if x == 3 { 0 } else { 255 };
                rgba.extend_from_slice(&[x * 36, y * 60, 255 - x * 30, alpha]);
            }
        }
        let image = Image {
            width: 7,
            height: 5,
            rgba,
        };
        let draw = |framebuffer: &mut Framebuffer| {
            framebuffer.draw_background(&image, [3, -2], [200, 180, 255]);
            framebuffer.fill_rect([4, 4, 300, 100], [30, 60, 90, 255]);
            framebuffer.draw_rgba_sprite(&image, [0, 0, 7, 5], [10, 10, 21, 15], 0);
            framebuffer.draw_rgba_sprite(&image, [1, 1, 5, 4], [40, 12, 11, 9], 14);
            let blends = [
                SpriteBlend::Half,
                SpriteBlend::HalfVariant,
                SpriteBlend::BlackAlpha(128),
                SpriteBlend::Add,
            ];
            for (index, blend) in blends.into_iter().enumerate() {
                let x = 60 + 30 * index as i32;
                framebuffer.draw_sprites(&mut [render::SpriteDraw::rgba(
                    &image,
                    [0, 0, 7, 5],
                    [x, 8, 21, 15],
                    0,
                )
                .with_blend(blend)
                .with_mirror([index == 1, index == 2])]);
            }
            framebuffer.set_clip_rect(Some([12, 11, 5, 3]));
            framebuffer.draw_rgba_sprite(&image, [0, 0, 7, 5], [10, 40, 14, 10], 0);
            framebuffer.set_clip_rect(None);
            framebuffer.blend_rect([100, 100, 50, 40], [255, 0, 0], 100);
            framebuffer.blend_black_rect([120, 120, 40, 40], 80);
            framebuffer.blend_black_rect_weighted([10, 150, 40, 40], 200);
            framebuffer.draw_line([0, 200], [319, 180], [255, 255, 0]);
            framebuffer.draw_line([200, 10], [190, 230], [0, 255, 0]);
            framebuffer.fade_to_black_weighted(64);
        };
        let mut software = Framebuffer::new();
        draw(&mut software);
        let mut recorded = Framebuffer::new();
        recorded.record();
        draw(&mut recorded);
        assert!(
            recorded.rgba.iter().all(|&byte| byte == 0),
            "a recording draws no pixels"
        );
        gpu.draw_frame(recorded.list().unwrap()).unwrap();
        let (pixels, size) = read_target(&gpu);
        assert_eq!(size, (LAYER_WIDTH, LAYER_HEIGHT));
        let mut worst = (0, 0, 0);
        for (index, (got, want)) in pixels
            .as_chunks::<4>()
            .0
            .iter()
            .zip(software.rgba.as_chunks::<4>().0)
            .enumerate()
        {
            for channel in 0..3 {
                let diff = got[channel].abs_diff(want[channel]);
                if diff > worst.0 {
                    worst = (diff, index % 320, index / 320);
                }
            }
        }
        assert!(
            worst.0 <= 1,
            "({}, {}) is {} off the software frame",
            worst.1,
            worst.2,
            worst.0
        );
    }

    /// The 2D pass draws its items in order, each cut to its clip and times
    /// its colour, the scene where an item shows it.
    #[test]
    fn gpu_2d_pass_draws_its_items_over_the_scene_in_order() {
        let _lock = crate::audio::test_lock::sdl();
        let Ok(_sdl) = crate::engine::SdlHandle::acquire() else {
            eprintln!("skipping: SDL video unavailable");
            return;
        };
        let mut gpu = match GpuRenderer::offscreen(1) {
            Ok(gpu) => gpu,
            Err(err) => {
                eprintln!("skipping: {err:#}");
                return;
            }
        };
        let solid = |width: u32, height: u32, rgba: [u8; 4]| Image {
            width,
            height,
            rgba: rgba.repeat((width * height) as usize),
        };
        let background = solid(320, 240, [200, 0, 0, 255]);
        let camera = Camera::from_points([0, 0, 5000], [0, 0, 0], 600);
        let lighting = Lighting {
            ambient: [4096; 3],
            lights: Default::default(),
        };
        let scene = GameplayScene {
            background: Some(&background),
            origin: [0, 0],
            colour: [255; 3],
            meshes: &[],
            shadows: &[],
            camera: &camera,
            lighting: &lighting,
            mask_layer: None,
            effect_layer: None,
            mirror_from: 0,
            mirror: None,
            debug: &[],
        };
        gpu.record_scene(&scene, None).unwrap();
        let replacement = |rgba: [u8; 4]| {
            Arc::new(htexture::Replacement {
                image: solid(64, 64, rgba),
                frame: [32, 32],
            })
        };
        let whole = [0, 0, 320, 240];
        let items = vec![
            item(
                draw2d::Kind::Scene,
                [0.0, 0.0, 320.0, 240.0],
                whole,
                [1.0; 4],
            ),
            // Green over x 40..120, y 40..120, cut at x 100.
            item(
                draw2d::Kind::Art(replacement([0, 240, 0, 255])),
                [40.0, 40.0, 120.0, 120.0],
                [0, 0, 100, 240],
                [1.0; 4],
            ),
            // Half-bright blue over the lower right of the green.
            item(
                draw2d::Kind::Art(replacement([0, 0, 240, 255])),
                [80.0, 80.0, 160.0, 160.0],
                whole,
                [0.5, 0.5, 0.5, 1.0],
            ),
            // A half black over the lower right of that.
            item(
                draw2d::Kind::Solid,
                [150.0, 150.0, 170.0, 170.0],
                whole,
                [0.0, 0.0, 0.0, 0.5],
            ),
        ];
        let list = list_of(items);
        gpu.draw_frame(&list).unwrap();
        let (pixels, size) = read_target(&gpu);
        assert_eq!(size, (LAYER_WIDTH, LAYER_HEIGHT));
        let at = |x, y| rgb_at(&pixels, size, 1, x, y);
        let near =
            |got: [u8; 3], want: [u8; 3]| got.iter().zip(want).all(|(&g, w)| g.abs_diff(w) <= 2);
        for (x, y, want) in [
            (10, 10, [200, 0, 0]),
            (60, 60, [0, 240, 0]),
            (110, 60, [200, 0, 0]),
            (90, 90, [0, 0, 120]),
            (150, 150, [0, 0, 60]),
            (165, 165, [100, 0, 0]),
            (200, 200, [200, 0, 0]),
        ] {
            assert!(near(at(x, y), want), "({x}, {y}) is {:?}", at(x, y));
        }

        // Without the scene the items are drawn over black.
        let mut list = list;
        list.items.remove(0);
        gpu.draw_frame(&list).unwrap();
        let (pixels, _) = read_target(&gpu);
        assert_eq!(&pixels[..4], &[0, 0, 0, 255]);
        assert_eq!(gpu.overlay_images.len(), 2);

        // An atlas that filled up is refused.
        list.error = Some("the 2D atlas is full");
        assert!(gpu.draw_frame(&list).is_err());

        // Replacements not drawn for long enough are released.
        gpu.overlay_frame += EVICT_AFTER_FRAMES;
        gpu.evict_overlay_images();
        assert_eq!(gpu.overlay_images.len(), 2);
        gpu.overlay_frame += 1;
        gpu.evict_overlay_images();
        assert!(gpu.overlay_images.is_empty());
    }

    /// A clear scene (the DIED screen's, with no background) shows the
    /// items drawn under it where it draws nothing.
    #[test]
    fn gpu_2d_pass_shows_what_lies_under_a_clear_scene() {
        let _lock = crate::audio::test_lock::sdl();
        let Ok(_sdl) = crate::engine::SdlHandle::acquire() else {
            eprintln!("skipping: SDL video unavailable");
            return;
        };
        let mut gpu = match GpuRenderer::offscreen(1) {
            Ok(gpu) => gpu,
            Err(err) => {
                eprintln!("skipping: {err:#}");
                return;
            }
        };
        let camera = Camera::from_points([0, 0, 5000], [0, 0, 0], 600);
        let lighting = Lighting {
            ambient: [4096; 3],
            lights: Default::default(),
        };
        let scene = GameplayScene {
            background: None,
            origin: [0, 0],
            colour: [255; 3],
            meshes: &[],
            shadows: &[],
            camera: &camera,
            lighting: &lighting,
            mask_layer: None,
            effect_layer: None,
            mirror_from: 0,
            mirror: None,
            debug: &[],
        };
        gpu.record_scene(&scene, None).unwrap();
        let whole = [0, 0, 320, 240];
        let list = list_of(vec![
            item(
                draw2d::Kind::Solid,
                [0.0, 0.0, 320.0, 240.0],
                whole,
                [0.0, 0.5, 0.0, 1.0],
            ),
            item(
                draw2d::Kind::Scene,
                [0.0, 0.0, 320.0, 240.0],
                whole,
                [1.0; 4],
            ),
        ]);
        gpu.draw_frame(&list).unwrap();
        let (pixels, size) = read_target(&gpu);
        let green = rgb_at(&pixels, size, 1, 160, 120);
        assert!(green[0] == 0 && green[1].abs_diff(128) <= 1 && green[2] == 0);
    }

    /// A recoloured item takes each original texel's recolouring, and an
    /// index without one stays clear.
    #[test]
    fn gpu_2d_pass_recolours_an_item_by_its_originals_indices() {
        let _lock = crate::audio::test_lock::sdl();
        let Ok(_sdl) = crate::engine::SdlHandle::acquire() else {
            eprintln!("skipping: SDL video unavailable");
            return;
        };
        let mut gpu = match GpuRenderer::offscreen(1) {
            Ok(gpu) => gpu,
            Err(err) => {
                eprintln!("skipping: {err:#}");
                return;
            }
        };
        // A 3x1 page of indices 1, 2 and 0 under a 6x2 HD image, each
        // original texel 100 pixels wide: index 1 tinted to half, index 2
        // swapped for red, index 0 clear.
        let art = Arc::new(htexture::Replacement {
            image: Image {
                width: 6,
                height: 2,
                rgba: [200, 100, 50, 255].repeat(12),
            },
            frame: [3, 1],
        });
        let mut lut = [None; 256];
        lut[1] = Some(htexture::Recolour::of([200, 100, 50], [100, 50, 25]));
        lut[2] = Some(htexture::Recolour::of([200, 100, 50], [255, 0, 0]));
        let recoloured = htexture::Recoloured {
            art,
            indices: Arc::new(htexture::Indices {
                width: 3,
                height: 1,
                texels: vec![1, 2, 0],
            }),
            lut,
        };
        let list = list_of(vec![item(
            draw2d::Kind::Recoloured(Arc::new(recoloured)),
            [0.0, 0.0, 300.0, 240.0],
            [0, 0, 320, 240],
            [1.0; 4],
        )]);
        gpu.draw_frame(&list).unwrap();
        let (pixels, size) = read_target(&gpu);
        let at = |x| rgb_at(&pixels, size, 1, x, 100);
        let near =
            |got: [u8; 3], want: [u8; 3]| got.iter().zip(want).all(|(&g, w)| g.abs_diff(w) <= 2);
        for (x, want) in [(50, [100, 50, 25]), (150, [255, 0, 0]), (250, [0, 0, 0])] {
            assert!(near(at(x), want), "({x}, 100) is {:?}", at(x));
        }
        assert_eq!(gpu.overlay_indices.len(), 1);
        gpu.overlay_frame += EVICT_AFTER_FRAMES + 1;
        gpu.evict_overlay_images();
        assert!(gpu.overlay_indices.is_empty());
    }

    /// A model item renders into its own clear target at the pass's
    /// resolution, depth tested, and lands in its rect over what the frame
    /// drew under it.
    #[test]
    fn gpu_2d_pass_renders_a_model_over_what_lies_under_it() {
        let _lock = crate::audio::test_lock::sdl();
        let Ok(_sdl) = crate::engine::SdlHandle::acquire() else {
            eprintln!("skipping: SDL video unavailable");
            return;
        };
        let mut gpu = match GpuRenderer::offscreen(2) {
            Ok(gpu) => gpu,
            Err(err) => {
                eprintln!("skipping: {err:#}");
                return;
            }
        };
        let mut palettes = vec![[0, 0, 0, 0]; crate::model::PALETTE_ROW_LEN];
        palettes[1] = [255, 255, 255, 255];
        let ivm = Arc::new(crate::ivm::Ivm {
            texture: Texture8 {
                width: 1,
                height: 1,
                indices: vec![1],
                palettes,
                stp: vec![false; crate::model::PALETTE_ROW_LEN],
            },
            objects: Vec::new(),
        });
        // The item's top-left half, untextured: red in front, then green
        // behind it.
        let corner = |x: f32, y: f32, depth: f32, shade: [f32; 3]| htexture::ModelVertex {
            clip: [x, y, depth, 1.0],
            uv: [0.0, 0.0],
            shade,
            packed: 1 << 25,
        };
        let half = |depth: f32, shade: [f32; 3]| {
            [
                corner(-1.0, 1.0, depth, shade),
                corner(1.0, 1.0, depth, shade),
                corner(-1.0, -1.0, depth, shade),
            ]
        };
        let mut vertices = half(0.5, [1.0, 0.0, 0.0]).to_vec();
        vertices.extend(half(0.9, [0.0, 1.0, 0.0]));
        let rect = [100.0, 80.0, 200.0, 160.0];
        let clip = [100, 80, 200, 160];
        let model = |vertices: Vec<htexture::ModelVertex>| {
            item(
                draw2d::Kind::Model(Arc::new(htexture::Model {
                    ivm: ivm.clone(),
                    vertices,
                    page: None,
                })),
                rect,
                clip,
                [1.0; 4],
            )
        };
        // The navy the viewer draws under the model.
        let navy = item(
            draw2d::Kind::Solid,
            rect,
            clip,
            [0.0, 0.0, 32.0 / 255.0, 1.0],
        );
        let mut list = list_of(vec![navy, model(vertices.clone())]);
        let check = |gpu: &mut GpuRenderer, list: &DrawList| {
            gpu.draw_frame(list).unwrap();
            let (pixels, size) = read_target(gpu);
            assert_eq!(size, (2 * LAYER_WIDTH, 2 * LAYER_HEIGHT));
            assert_eq!(gpu.model_targets.len(), 1);
            assert_eq!(gpu.model_targets[0].size, (200, 160));
            // In frame pixels, read at the target's double resolution.
            for (x, y, want) in [
                (120, 90, [255, 0, 0]),
                (190, 150, [0, 0, 32]),
                (190, 82, [255, 0, 0]),
                (50, 50, [0, 0, 0]),
                (210, 100, [0, 0, 0]),
            ] {
                assert_eq!(rgb_at(&pixels, size, 2, x, y), want, "({x}, {y})");
            }
        };
        check(&mut gpu, &list);
        // The page joined the scene's residency.
        assert_eq!(gpu.resident_counts().1, 1);

        // A model with nothing to draw takes no target; the next one still
        // lands in its own.
        list.items.insert(1, model(Vec::new()));
        check(&mut gpu, &list);

        // The targets are released once no model is drawn for long enough.
        gpu.overlay_frame += EVICT_AFTER_FRAMES;
        gpu.evict_overlay_images();
        assert_eq!(gpu.model_targets.len(), 1);
        gpu.overlay_frame += 1;
        gpu.evict_overlay_images();
        assert!(gpu.model_targets.is_empty());
    }

    /// A model's HD page replacement is bound and sampled; without one the
    /// base palette draws.
    #[test]
    fn gpu_model_draws_its_page_replacement() {
        let _lock = crate::audio::test_lock::sdl();
        let Ok(_sdl) = crate::engine::SdlHandle::acquire() else {
            eprintln!("skipping: SDL video unavailable");
            return;
        };
        let mut gpu = match GpuRenderer::offscreen(1) {
            Ok(gpu) => gpu,
            Err(err) => {
                eprintln!("skipping: {err:#}");
                return;
            }
        };
        let mut palettes = vec![[0, 0, 0, 0]; crate::model::PALETTE_ROW_LEN];
        palettes[1] = [255, 255, 255, 255];
        let ivm = Arc::new(crate::ivm::Ivm {
            texture: Texture8 {
                width: 1,
                height: 1,
                indices: vec![1],
                palettes,
                stp: vec![false; crate::model::PALETTE_ROW_LEN],
            },
            objects: Vec::new(),
        });
        let corner = |x: f32, y: f32| htexture::ModelVertex {
            clip: [x, y, 0.5, 1.0],
            uv: [0.0, 0.0],
            shade: [1.0; 3],
            packed: 0,
        };
        let vertices = vec![corner(-1.0, 1.0), corner(1.0, 1.0), corner(-1.0, -1.0)];
        let solid = |rgba: [u8; 4]| Image {
            width: 1,
            height: 1,
            rgba: rgba.to_vec(),
        };
        let model = |page: Option<Arc<htexture::Replacement>>| {
            list_of(vec![item(
                draw2d::Kind::Model(Arc::new(htexture::Model {
                    ivm: ivm.clone(),
                    vertices: vertices.clone(),
                    page,
                })),
                [100.0, 80.0, 200.0, 160.0],
                [100, 80, 200, 160],
                [1.0; 4],
            )])
        };
        let pixel = |gpu: &mut GpuRenderer, list: &DrawList| {
            gpu.draw_frame(list).unwrap();
            let (pixels, size) = read_target(gpu);
            rgb_at(&pixels, size, 1, 120, 90)
        };

        // The base page is white.
        assert_eq!(pixel(&mut gpu, &model(None)), [255, 255, 255]);
        // The named replacement is red, at its own resolution.
        let red = Arc::new(htexture::Replacement {
            image: solid([255, 0, 0, 255]),
            frame: [1, 1],
        });
        assert_eq!(pixel(&mut gpu, &model(Some(red))), [255, 0, 0]);
    }
}
