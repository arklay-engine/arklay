//! The native GUI layer on the GPU: a recorded [`Canvas`] frame
//! ([`Canvas::set_recording`]) drawn straight into the swapchain over the
//! composited frame, at the window's pixels.
//!
//! Each [`Op`] becomes a quad (a polygon, a fan of triangles) carrying the
//! primitive whole; `gui.frag` gives every pixel the coverage the software
//! canvas gives it. Glyph masks and pictures are packed into an atlas of
//! their own ([`GUI_ATLAS`]), which a frame redrawing the same text and
//! pictures leaves as it was, so only the vertices upload again.

use std::ffi::c_void;

use anyhow::{Result, bail};
use sdl3_sys::gpu::*;

use super::scene::Plain;
use super::{Blend, GUI_ATLAS, GpuRenderer, as_bytes, sdl_error};
use crate::draw2d::Atlas;
use crate::gui::canvas::{Axis, Canvas, Color, Op, Rect};

const GUI_VERT: &[u8] = include_bytes!("shaders/gui.vert.spv");
const GUI_FRAG: &[u8] = include_bytes!("shaders/gui.frag.spv");

/// `gui.frag`'s modes.
const MODE_FILL: f32 = 0.0;
const MODE_POLYGON: f32 = 1.0;
const MODE_MASK: f32 = 2.0;
const MODE_IMAGE: f32 = 3.0;

/// A distance standing in for "no edge here".
const FAR: f32 = 1.0e4;

/// One corner of a GUI primitive (`gui.vert`'s input, 112 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct GuiVertex {
    position: [f32; 2],
    /// The mode, then a fill's radius or a picture's opacity.
    mode: [f32; 2],
    from: [f32; 4],
    to: [f32; 4],
    /// `[x0, y0, x1, y1]`: a fill's or a picture's rectangle, a mask's
    /// origin.
    rect: [f32; 4],
    /// `[x0, y0, x1, y1]`.
    clip: [f32; 4],
    /// The atlas origin and the size of a mask or picture.
    source: [f32; 4],
    /// A polygon triangle's three edge distances; `w` the gradient axis.
    edges: [f32; 4],
}

// SAFETY: `repr(C)` and made only of `f32`s, so it has no padding.
unsafe impl Plain for GuiVertex {}

/// The uploaded frame of the native layer.
#[derive(Debug)]
pub(super) struct GuiLayer {
    pub(super) buffer: *mut SDL_GPUBuffer,
    pub(super) capacity: u32,
    pub(super) vertices: u32,
    /// The [`Canvas::serial`] uploaded.
    pub(super) serial: u64,
    /// The canvas's size in pixels.
    pub(super) size: (u32, u32),
    /// Whether the next present draws it.
    pub(super) shown: bool,
    /// The masks and pictures, kept across frames.
    pub(super) atlas: Atlas,
}

impl Default for GuiLayer {
    fn default() -> Self {
        Self {
            buffer: std::ptr::null_mut(),
            capacity: 0,
            vertices: 0,
            serial: 0,
            size: (0, 0),
            shown: false,
            atlas: Atlas::default(),
        }
    }
}

fn colour(color: Color) -> [f32; 4] {
    [color.r, color.g, color.b, color.a].map(|c| f32::from(c) / 255.0)
}

fn corners(rect: Rect) -> [f32; 4] {
    [rect.x, rect.y, rect.right(), rect.bottom()]
}

/// `rect` cut to `clip` and grown to whole pixels; `None` when nothing is
/// left.
fn pixel_bounds(rect: Rect, clip: Rect) -> Option<[f32; 4]> {
    let cut = rect.intersect(&clip);
    (cut.w > 0.0 && cut.h > 0.0).then(|| {
        [
            cut.x.floor(),
            cut.y.floor(),
            cut.right().ceil(),
            cut.bottom().ceil(),
        ]
    })
}

/// Push `template` at the six corners of `bounds`.
fn quad(out: &mut Vec<GuiVertex>, [x0, y0, x1, y1]: [f32; 4], template: GuiVertex) {
    for position in [[x0, y0], [x1, y0], [x0, y1], [x1, y0], [x1, y1], [x0, y1]] {
        out.push(GuiVertex {
            position,
            ..template
        });
    }
}

/// Store a glyph mask in the atlas as white texels of its coverage.
fn store_mask(atlas: &mut Atlas, width: usize, height: usize, mask: &[u8]) -> Option<[u32; 2]> {
    let texels: Vec<u8> = mask[..width * height]
        .iter()
        .flat_map(|&cover| [0xFF, 0xFF, 0xFF, cover])
        .collect();
    atlas.insert(width as u32, height as u32, &texels)
}

/// The vertices of `ops`, their masks and pictures packed into `atlas`. A
/// mask or picture the atlas has no room for is left out.
pub(super) fn tessellate(ops: &[Op], atlas: &mut Atlas) -> Vec<GuiVertex> {
    atlas.repack();
    let mut out = Vec::with_capacity(ops.len() * 6);
    for op in ops {
        match op {
            Op::Fill {
                rect,
                clip,
                from,
                to,
                axis,
                radius,
            } => {
                let Some(bounds) = pixel_bounds(*rect, *clip) else {
                    continue;
                };
                quad(
                    &mut out,
                    bounds,
                    GuiVertex {
                        mode: [MODE_FILL, *radius],
                        from: colour(*from),
                        to: colour(*to),
                        rect: corners(*rect),
                        clip: corners(*clip),
                        edges: [
                            0.0,
                            0.0,
                            0.0,
                            if *axis == Axis::Vertical { 1.0 } else { 0.0 },
                        ],
                        ..GuiVertex::default()
                    },
                );
            }
            Op::Polygon {
                points,
                clip,
                color,
            } => polygon(&mut out, points, *clip, *color),
            Op::Mask {
                x,
                y,
                width,
                height,
                mask,
                clip,
                color,
            } => {
                if mask.len() < width * height {
                    continue;
                }
                let area = Rect::new(*x as f32, *y as f32, *width as f32, *height as f32);
                let Some(bounds) = pixel_bounds(area, *clip) else {
                    continue;
                };
                let Some([u, v]) = store_mask(atlas, *width, *height, mask) else {
                    continue;
                };
                quad(
                    &mut out,
                    bounds,
                    GuiVertex {
                        mode: [MODE_MASK, 0.0],
                        from: colour(*color),
                        rect: corners(area),
                        clip: corners(*clip),
                        source: [u as f32, v as f32, *width as f32, *height as f32],
                        ..GuiVertex::default()
                    },
                );
            }
            Op::Image {
                picture,
                dest,
                clip,
                opacity,
            } => {
                let Some(bounds) = pixel_bounds(*dest, *clip) else {
                    continue;
                };
                let Some([u, v]) = atlas.insert(picture.width, picture.height, &picture.rgba)
                else {
                    continue;
                };
                quad(
                    &mut out,
                    bounds,
                    GuiVertex {
                        mode: [MODE_IMAGE, opacity.clamp(0.0, 1.0)],
                        rect: corners(*dest),
                        clip: corners(*clip),
                        source: [
                            u as f32,
                            v as f32,
                            picture.width as f32,
                            picture.height as f32,
                        ],
                        ..GuiVertex::default()
                    },
                );
            }
        }
    }
    out
}

/// A convex polygon as a fan of triangles from its first point. Each
/// triangle carries its distances to the polygon's outline (inside
/// positive) on the edges that are the outline's, which interpolate exactly
/// across it.
fn polygon(out: &mut Vec<GuiVertex>, points: &[(f32, f32)], clip: Rect, color: Color) {
    let count = points.len();
    if count < 3 {
        return;
    }
    let area: f32 = (0..count)
        .map(|i| {
            let (ax, ay) = points[i];
            let (bx, by) = points[(i + 1) % count];
            ax * by - bx * ay
        })
        .sum();
    if area.abs() < 1.0e-6 {
        return;
    }
    let sign = area.signum();
    // The signed distance of `p` from the edge `a`→`b`, positive inside.
    let distance = |a: (f32, f32), b: (f32, f32), p: (f32, f32)| {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let length = (dx * dx + dy * dy).sqrt().max(1.0e-6);
        sign * (dx * (p.1 - a.1) - dy * (p.0 - a.0)) / length
    };
    let template = GuiVertex {
        mode: [MODE_POLYGON, 0.0],
        from: colour(color),
        clip: corners(clip),
        ..GuiVertex::default()
    };
    for i in 1..count - 1 {
        let triangle = [points[0], points[i], points[i + 1]];
        let mut outline = vec![(points[i], points[i + 1])];
        if i == 1 {
            outline.push((points[0], points[1]));
        }
        if i == count - 2 {
            outline.push((points[count - 1], points[0]));
        }
        for &p in &triangle {
            let mut edges = [FAR; 4];
            for (slot, &(a, b)) in outline.iter().enumerate() {
                edges[slot] = distance(a, b, p);
            }
            edges[3] = 0.0;
            out.push(GuiVertex {
                position: [p.0, p.1],
                edges,
                ..template
            });
        }
    }
}

impl GpuRenderer {
    /// The native layer's pipeline, built for the swapchain's `format`.
    pub(super) fn gui_pipeline(
        &self,
        format: SDL_GPUTextureFormat,
    ) -> Result<*mut SDL_GPUGraphicsPipeline> {
        let attribute = |location: u32, format: SDL_GPUVertexElementFormat, offset: u32| {
            SDL_GPUVertexAttribute {
                location,
                buffer_slot: 0,
                format,
                offset,
            }
        };
        let attributes = [
            attribute(0, SDL_GPU_VERTEXELEMENTFORMAT_FLOAT2, 0),
            attribute(1, SDL_GPU_VERTEXELEMENTFORMAT_FLOAT2, 8),
            attribute(2, SDL_GPU_VERTEXELEMENTFORMAT_FLOAT4, 16),
            attribute(3, SDL_GPU_VERTEXELEMENTFORMAT_FLOAT4, 32),
            attribute(4, SDL_GPU_VERTEXELEMENTFORMAT_FLOAT4, 48),
            attribute(5, SDL_GPU_VERTEXELEMENTFORMAT_FLOAT4, 64),
            attribute(6, SDL_GPU_VERTEXELEMENTFORMAT_FLOAT4, 80),
            attribute(7, SDL_GPU_VERTEXELEMENTFORMAT_FLOAT4, 96),
        ];
        let vertex = self.shader(GUI_VERT, SDL_GPU_SHADERSTAGE_VERTEX, 0, 1)?;
        let fragment = self.shader(GUI_FRAG, SDL_GPU_SHADERSTAGE_FRAGMENT, 1, 0)?;
        self.pipeline(
            vertex.shader,
            fragment.shader,
            std::mem::size_of::<GuiVertex>() as u32,
            &attributes,
            SDL_GPU_CULLMODE_NONE,
            Blend::Premultiplied,
            None,
            Some(format),
            SDL_GPU_SAMPLECOUNT_1,
        )
    }

    /// Upload `canvas`'s recorded frame for the next present to draw, unless
    /// it is the frame already uploaded.
    pub(super) fn set_gui(&mut self, canvas: &Canvas) -> Result<()> {
        let Some(ops) = canvas.ops() else {
            return Ok(());
        };
        if self.pipelines.gui.is_null() {
            return Ok(());
        }
        let size = (canvas.width, canvas.height);
        if self.gui.serial == canvas.serial && self.gui.size == size {
            self.gui.shown = true;
            return Ok(());
        }
        let mut atlas = std::mem::take(&mut self.gui.atlas);
        let vertices = tessellate(ops, &mut atlas);
        let uploaded = self.upload_gui(&vertices, &atlas);
        self.gui.atlas = atlas;
        if let Err(err) = uploaded {
            // What the textures and buffer hold is unknown now.
            self.atlases[GUI_ATLAS].revision = (0, 0);
            self.gui.serial = 0;
            return Err(err);
        }
        self.gui.vertices = vertices.len() as u32;
        self.gui.serial = canvas.serial;
        self.gui.size = size;
        self.gui.shown = true;
        Ok(())
    }

    fn upload_gui(&mut self, vertices: &[GuiVertex], atlas: &Atlas) -> Result<()> {
        let bytes = as_bytes(vertices);
        if !bytes.is_empty() && self.gui.capacity < bytes.len() as u32 {
            let capacity = (bytes.len() as u32).next_power_of_two();
            let buffer = self.vertex_buffer(capacity)?;
            if !self.gui.buffer.is_null() {
                unsafe { SDL_ReleaseGPUBuffer(self.device, self.gui.buffer) };
            }
            self.gui.buffer = buffer;
            self.gui.capacity = capacity;
        }
        let command = self.command_buffer()?;
        let copy = unsafe { SDL_BeginGPUCopyPass(command) };
        if copy.is_null() {
            unsafe { SDL_CancelGPUCommandBuffer(command) };
            bail!("SDL_BeginGPUCopyPass failed: {}", sdl_error());
        }
        let uploaded = (|| {
            // A frame of fills alone still binds an atlas.
            self.ensure_atlas(GUI_ATLAS, 1)?;
            self.upload_atlas(copy, GUI_ATLAS, atlas)?;
            if !bytes.is_empty() {
                self.upload_buffer(copy, self.gui.buffer, bytes, true)?;
            }
            Ok(())
        })();
        unsafe { SDL_EndGPUCopyPass(copy) };
        if let Err(err) = uploaded {
            unsafe { SDL_CancelGPUCommandBuffer(command) };
            return Err(err);
        }
        Self::submit(command)
    }

    /// Draw the uploaded native layer into `pass`, the swapchain's, from
    /// its top left at its own size.
    pub(super) fn draw_gui(
        &self,
        command: *mut SDL_GPUCommandBuffer,
        pass: *mut SDL_GPURenderPass,
        (width, height): (u32, u32),
    ) {
        if !self.gui.shown || self.gui.vertices == 0 || self.gui.buffer.is_null() {
            return;
        }
        let (canvas_w, canvas_h) = self.gui.size;
        let viewport = SDL_GPUViewport {
            x: 0.0,
            y: 0.0,
            w: canvas_w as f32,
            h: canvas_h as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };
        let scissor = sdl3_sys::rect::SDL_Rect {
            x: 0,
            y: 0,
            w: width.min(canvas_w) as i32,
            h: height.min(canvas_h) as i32,
        };
        let binding = SDL_GPUBufferBinding {
            buffer: self.gui.buffer,
            offset: 0,
        };
        let atlas = self.atlases[GUI_ATLAS].texture;
        if atlas.is_null() {
            return;
        }
        let sampler = SDL_GPUTextureSamplerBinding {
            texture: atlas,
            sampler: self.linear,
        };
        let size: [f32; 4] = [canvas_w as f32, canvas_h as f32, 0.0, 0.0];
        unsafe {
            SDL_BindGPUGraphicsPipeline(pass, self.pipelines.gui);
            SDL_SetGPUViewport(pass, &viewport);
            SDL_SetGPUScissor(pass, &scissor);
            SDL_BindGPUVertexBuffers(pass, 0, &binding, 1);
            SDL_PushGPUVertexUniformData(
                command,
                0,
                size.as_ptr().cast::<c_void>(),
                std::mem::size_of_val(&size) as u32,
            );
            SDL_BindGPUFragmentSamplers(pass, 0, &sampler, 1);
            SDL_DrawGPUPrimitives(pass, self.gui.vertices, 1, 0, 0);
        }
    }
}
