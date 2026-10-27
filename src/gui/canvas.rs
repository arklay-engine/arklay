//! A premultiplied RGBA canvas at the window's native pixel size.
//!
//! Every GUI primitive draws through here: rectangles, gradients, glyph
//! coverage masks and scaled images. Coordinates are `f32` pixels; edges are
//! anti-aliased by their fractional coverage, so a layout computed in scaled
//! units never snaps unevenly at odd window sizes.
//!
//! A canvas can instead *record* ([`Canvas::set_recording`]): every primitive
//! is kept as an [`Op`] rather than drawn, so the GPU backend draws the frame
//! itself (`gpu::gui`) at the same coverage. [`Canvas::pixels`] rasterises a
//! recording in software for a frame the GPU could not present.

use std::borrow::Cow;
use std::sync::Arc;

/// A straight (non-premultiplied) RGBA colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const WHITE: Self = Self::rgb(0xFF, 0xFF, 0xFF);
    pub const BLACK: Self = Self::rgb(0x00, 0x00, 0x00);
    pub const TRANSPARENT: Self = Self::rgba(0, 0, 0, 0);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 0xFF }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// The same colour at `alpha` (0..=1) of its own opacity.
    pub fn fade(self, alpha: f32) -> Self {
        Self {
            a: (f32::from(self.a) * alpha.clamp(0.0, 1.0)).round() as u8,
            ..self
        }
    }

    /// Linear interpolation towards `other` by `t` (0..=1).
    pub fn mix(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let lerp = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
        Self {
            r: lerp(self.r, other.r),
            g: lerp(self.g, other.g),
            b: lerp(self.b, other.b),
            a: lerp(self.a, other.a),
        }
    }
}

/// An axis-aligned rectangle in pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    pub fn center_x(&self) -> f32 {
        self.x + self.w / 2.0
    }

    pub fn center_y(&self) -> f32 {
        self.y + self.h / 2.0
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }

    /// The rectangle shrunk by `dx` on the left and right and `dy` on the
    /// top and bottom.
    pub fn inset(&self, dx: f32, dy: f32) -> Self {
        Self::new(
            self.x + dx,
            self.y + dy,
            (self.w - 2.0 * dx).max(0.0),
            (self.h - 2.0 * dy).max(0.0),
        )
    }

    /// The overlap of two rectangles (empty when they do not meet).
    pub fn intersect(&self, other: &Self) -> Self {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        Self::new(x, y, (right - x).max(0.0), (bottom - y).max(0.0))
    }
}

/// The direction a gradient runs in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

/// A straight RGBA image the canvas can draw scaled, shared so a recorded
/// frame keeps it without a copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    /// Straight RGBA rows, top first.
    pub rgba: Vec<u8>,
}

/// One recorded primitive, with the clip it was drawn under.
#[derive(Debug, Clone)]
pub enum Op {
    /// A rectangle: solid (`from == to`), a gradient along `axis`, or a
    /// rounded rectangle when `radius` is above zero (solid `from`).
    Fill {
        rect: Rect,
        clip: Rect,
        from: Color,
        to: Color,
        axis: Axis,
        radius: f32,
    },
    /// A convex polygon.
    Polygon {
        points: Vec<(f32, f32)>,
        clip: Rect,
        color: Color,
    },
    /// An 8-bit coverage mask (a glyph) at integer `(x, y)`, tinted.
    Mask {
        x: i32,
        y: i32,
        width: usize,
        height: usize,
        mask: Arc<[u8]>,
        clip: Rect,
        color: Color,
    },
    /// A picture scaled into `dest`.
    Image {
        picture: Arc<Picture>,
        dest: Rect,
        clip: Rect,
        opacity: f32,
    },
}

/// A premultiplied RGBA pixel canvas.
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    /// Premultiplied RGBA rows, top first; empty while recording.
    pub rgba: Vec<u8>,
    /// Changes on every [`Self::reset`] and is unique across canvases, so a
    /// presenter can skip re-uploading a frame it already has.
    pub serial: u64,
    clip: Vec<Rect>,
    /// The frame's primitives while recording.
    ops: Option<Vec<Op>>,
}

/// The next [`Canvas::serial`]; zero is never handed out.
static NEXT_SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn next_serial() -> u64 {
    NEXT_SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

impl Canvas {
    pub fn new(width: u32, height: u32) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        Self {
            width,
            height,
            rgba: vec![0; width as usize * height as usize * 4],
            serial: next_serial(),
            clip: Vec::new(),
            ops: None,
        }
    }

    /// Record the primitives from the next [`Self::reset`] on instead of
    /// drawing them (the GPU backend draws them), or draw them again.
    /// Returns whether the mode changed, which makes the canvas stale.
    pub fn set_recording(&mut self, on: bool) -> bool {
        if on == self.ops.is_some() {
            return false;
        }
        if on {
            self.ops = Some(Vec::new());
            self.rgba = Vec::new();
        } else {
            self.ops = None;
            self.rgba = vec![0; self.width as usize * self.height as usize * 4];
        }
        self.serial = next_serial();
        true
    }

    /// The recorded primitives, in order, while recording.
    pub fn ops(&self) -> Option<&[Op]> {
        self.ops.as_deref()
    }

    /// Start this frame with another canvas's (its recording, or its
    /// pixels): the debug HUD composes over the menu. A canvas of another
    /// size is ignored.
    pub fn copy_from(&mut self, base: &Canvas) {
        if (base.width, base.height) != (self.width, self.height) {
            return;
        }
        match (&mut self.ops, base.ops()) {
            (Some(ops), Some(base)) => ops.extend_from_slice(base),
            (None, None) => self.rgba.copy_from_slice(&base.rgba),
            (Some(ops), None) => ops.push(Op::Image {
                picture: Arc::new(unpremultiplied(base)),
                dest: base.bounds(),
                clip: base.bounds(),
                opacity: 1.0,
            }),
            (None, Some(_)) => {
                let pixels = base.pixels();
                self.rgba.copy_from_slice(&pixels);
            }
        }
    }

    /// The premultiplied pixels: the canvas's own, or a recording drawn in
    /// software.
    pub fn pixels(&self) -> Cow<'_, [u8]> {
        let Some(ops) = &self.ops else {
            return Cow::Borrowed(&self.rgba);
        };
        let mut canvas = Canvas::new(self.width, self.height);
        for op in ops {
            canvas.replay(op);
        }
        Cow::Owned(canvas.rgba)
    }

    /// Draw one recorded primitive under its own clip.
    fn replay(&mut self, op: &Op) {
        let clip = match op {
            Op::Fill { clip, .. }
            | Op::Polygon { clip, .. }
            | Op::Mask { clip, .. }
            | Op::Image { clip, .. } => *clip,
        };
        self.clip.clear();
        self.clip.push(clip);
        match op {
            Op::Fill {
                rect,
                from,
                to,
                axis,
                radius,
                ..
            } => {
                if *radius > 0.0 {
                    self.fill_round_rect(*rect, *radius, *from);
                } else if from == to {
                    self.fill_rect(*rect, *from);
                } else {
                    self.fill_gradient(*rect, *axis, *from, *to);
                }
            }
            Op::Polygon { points, color, .. } => self.fill_polygon(points, *color),
            Op::Mask {
                x,
                y,
                width,
                height,
                mask,
                color,
                ..
            } => self.draw_mask(*x, *y, *width, *height, mask, *color),
            Op::Image {
                picture,
                dest,
                opacity,
                ..
            } => self.draw_image(picture, *dest, *opacity),
        }
        self.clip.clear();
    }

    /// Record the primitive `op` builds from the clip when recording;
    /// returns whether it was taken.
    fn record(&mut self, op: impl FnOnce(Rect) -> Op) -> bool {
        let clip = self.current_clip();
        match &mut self.ops {
            Some(ops) => {
                if clip.w > 0.0 && clip.h > 0.0 {
                    ops.push(op(clip));
                }
                true
            }
            None => false,
        }
    }

    /// Resize (keeping the allocation when the size is unchanged) and clear
    /// to transparent.
    pub fn reset(&mut self, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        if let Some(ops) = &mut self.ops {
            ops.clear();
            self.width = width;
            self.height = height;
        } else if (self.width, self.height) != (width, height) {
            self.width = width;
            self.height = height;
            self.rgba = vec![0; width as usize * height as usize * 4];
        } else {
            self.rgba.fill(0);
        }
        self.clip.clear();
        self.serial = next_serial();
    }

    /// [`Self::reset`] to a solid `color` instead of transparent: a
    /// full-window shade costs a fill rather than a blend.
    pub fn reset_to(&mut self, width: u32, height: u32, color: Color) {
        self.reset(width, height);
        if self.ops.is_some() {
            self.fill_rect(self.bounds(), color);
            return;
        }
        let [r, g, b, a] = premultiply(color, u32::from(color.a)).map(|c| c as u8);
        if a != 0 {
            for pixel in self.rgba.as_chunks_mut::<4>().0 {
                pixel.copy_from_slice(&[r, g, b, a]);
            }
        }
    }

    pub fn bounds(&self) -> Rect {
        Rect::new(0.0, 0.0, self.width as f32, self.height as f32)
    }

    /// Restrict later draws to `rect` (intersected with any active clip).
    pub fn push_clip(&mut self, rect: Rect) {
        let current = self.current_clip();
        self.clip.push(current.intersect(&rect));
    }

    pub fn pop_clip(&mut self) {
        self.clip.pop();
    }

    fn current_clip(&self) -> Rect {
        self.clip.last().copied().unwrap_or_else(|| self.bounds())
    }

    /// The pixel span `rect` touches inside the clip.
    fn span(&self, rect: Rect) -> Option<Span> {
        let clipped = rect.intersect(&self.current_clip());
        if clipped.w <= 0.0 || clipped.h <= 0.0 {
            return None;
        }
        let x0 = clipped.x.floor().max(0.0) as usize;
        let y0 = clipped.y.floor().max(0.0) as usize;
        let x1 = (clipped.right().ceil() as usize).min(self.width as usize);
        let y1 = (clipped.bottom().ceil() as usize).min(self.height as usize);
        (x0 < x1 && y0 < y1).then_some(Span {
            rect: clipped,
            x0,
            y0,
            x1,
            y1,
        })
    }

    /// The pixels of row `y` from `x0` to `x1`.
    #[inline]
    fn row(&mut self, y: usize, x0: usize, x1: usize) -> &mut [u8] {
        let start = (y * self.width as usize + x0) * 4;
        &mut self.rgba[start..start + (x1 - x0) * 4]
    }

    /// Blend one pixel: `color` (straight) at `coverage` (0..=1).
    #[inline]
    fn blend(&mut self, x: usize, y: usize, color: Color, coverage: f32) {
        let alpha = coverage_alpha(color.a, coverage);
        if alpha == 0 {
            return;
        }
        let index = (y * self.width as usize + x) * 4;
        over(&mut self.rgba[index..index + 4], premultiply(color, alpha));
    }

    /// A filled rectangle.
    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        if color.a == 0 {
            return;
        }
        if self.record(|clip| Op::Fill {
            rect,
            clip,
            from: color,
            to: color,
            axis: Axis::Horizontal,
            radius: 0.0,
        }) {
            return;
        }
        let Some(span) = self.span(rect) else {
            return;
        };
        for y in span.y0..span.y1 {
            let cy = span.row_coverage(y);
            let inner = premultiply(color, coverage_alpha(color.a, cy));
            let (left, right) = (
                span.column_coverage(span.x0),
                span.column_coverage(span.x1 - 1),
            );
            let row = self.row(y, span.x0, span.x1);
            let last = row.len() / 4 - 1;
            for (index, pixel) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                let edge = match index {
                    0 => left,
                    _ if index == last => right,
                    _ => 1.0,
                };
                if edge >= 1.0 {
                    over(pixel, inner);
                } else {
                    over(
                        pixel,
                        premultiply(color, coverage_alpha(color.a, edge * cy)),
                    );
                }
            }
        }
    }

    /// A rectangle filled with a two-stop linear gradient along `axis`.
    pub fn fill_gradient(&mut self, rect: Rect, axis: Axis, from: Color, to: Color) {
        if from.a == 0 && to.a == 0 {
            return;
        }
        if self.record(|clip| Op::Fill {
            rect,
            clip,
            from,
            to,
            axis,
            radius: 0.0,
        }) {
            return;
        }
        let Some(span) = self.span(rect) else {
            return;
        };
        let (start, length) = match axis {
            Axis::Horizontal => (rect.x, rect.w),
            Axis::Vertical => (rect.y, rect.h),
        };
        let length = length.max(1.0);
        let at = |pixel: usize| from.mix(to, (pixel as f32 + 0.5 - start) / length);
        // Each column's colour and coverage, premultiplied once for the
        // fully covered rows.
        let columns: Vec<(Color, f32, [u32; 4])> = (span.x0..span.x1)
            .map(|x| {
                let color = at(x);
                let cx = span.column_coverage(x);
                (color, cx, premultiply(color, coverage_alpha(color.a, cx)))
            })
            .collect();
        for y in span.y0..span.y1 {
            let cy = span.row_coverage(y);
            let row_color = at(y);
            let row = self.row(y, span.x0, span.x1);
            match axis {
                Axis::Horizontal if cy >= 1.0 => {
                    for (pixel, &(_, _, source)) in
                        row.as_chunks_mut::<4>().0.iter_mut().zip(&columns)
                    {
                        over(pixel, source);
                    }
                }
                Axis::Horizontal => {
                    for (pixel, &(color, cx, _)) in
                        row.as_chunks_mut::<4>().0.iter_mut().zip(&columns)
                    {
                        over(pixel, premultiply(color, coverage_alpha(color.a, cx * cy)));
                    }
                }
                Axis::Vertical => {
                    let inner = premultiply(row_color, coverage_alpha(row_color.a, cy));
                    for (pixel, &(_, cx, _)) in row.as_chunks_mut::<4>().0.iter_mut().zip(&columns)
                    {
                        if cx >= 1.0 {
                            over(pixel, inner);
                        } else {
                            let alpha = coverage_alpha(row_color.a, cx * cy);
                            over(pixel, premultiply(row_color, alpha));
                        }
                    }
                }
            }
        }
    }

    /// A rectangle with corners rounded to `radius` (a circle when `rect`
    /// is square and `radius` is half its side), anti-aliased by each
    /// pixel's distance from the edge. Meant for small shapes (button
    /// glyphs).
    pub fn fill_round_rect(&mut self, rect: Rect, radius: f32, color: Color) {
        if color.a == 0 {
            return;
        }
        if self.record(|clip| Op::Fill {
            rect,
            clip,
            from: color,
            to: color,
            axis: Axis::Horizontal,
            radius: radius.min(rect.w / 2.0).min(rect.h / 2.0).max(0.0),
        }) {
            return;
        }
        let Some(span) = self.span(rect) else {
            return;
        };
        let radius = radius.min(rect.w / 2.0).min(rect.h / 2.0).max(0.0);
        let (cx, cy) = (rect.center_x(), rect.center_y());
        let (half_w, half_h) = (rect.w / 2.0 - radius, rect.h / 2.0 - radius);
        for y in span.y0..span.y1 {
            let dy = ((y as f32 + 0.5 - cy).abs() - half_h).max(0.0);
            let row_cover = span.row_coverage(y);
            let x0 = span.x0;
            let row = self.row(y, span.x0, span.x1);
            for (index, pixel) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                let x = x0 + index;
                let dx = ((x as f32 + 0.5 - cx).abs() - half_w).max(0.0);
                // Signed distance to the rounded outline, + inside.
                let inside = radius - (dx * dx + dy * dy).sqrt();
                let cover = (inside + 0.5).clamp(0.0, 1.0) * row_cover;
                let alpha = coverage_alpha(color.a, cover);
                if alpha != 0 {
                    over(pixel, premultiply(color, alpha));
                }
            }
        }
    }

    /// A one-colour outline `thickness` pixels wide inside `rect`.
    pub fn stroke_rect(&mut self, rect: Rect, thickness: f32, color: Color) {
        let t = thickness.min(rect.w / 2.0).min(rect.h / 2.0);
        self.fill_rect(Rect::new(rect.x, rect.y, rect.w, t), color);
        self.fill_rect(Rect::new(rect.x, rect.bottom() - t, rect.w, t), color);
        self.fill_rect(Rect::new(rect.x, rect.y + t, t, rect.h - 2.0 * t), color);
        self.fill_rect(
            Rect::new(rect.right() - t, rect.y + t, t, rect.h - 2.0 * t),
            color,
        );
    }

    /// A convex polygon, anti-aliased exactly across each row and on four
    /// sub-rows down it, for the shapes (arrows, diamonds, tab slants) the
    /// widgets need.
    pub fn fill_polygon(&mut self, points: &[(f32, f32)], color: Color) {
        if points.len() < 3 || color.a == 0 {
            return;
        }
        if self.record(|clip| Op::Polygon {
            points: points.to_vec(),
            clip,
            color,
        }) {
            return;
        }
        let (mut min_x, mut min_y) = (f32::MAX, f32::MAX);
        let (mut max_x, mut max_y) = (f32::MIN, f32::MIN);
        for &(x, y) in points {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
        let Some(span) = self.span(Rect::new(min_x, min_y, max_x - min_x, max_y - min_y)) else {
            return;
        };
        const SUB_ROWS: usize = 4;
        let mut coverage = vec![0.0f32; span.x1 - span.x0];
        for y in span.y0..span.y1 {
            coverage.fill(0.0);
            for sub in 0..SUB_ROWS {
                let sy = y as f32 + (sub as f32 + 0.5) / SUB_ROWS as f32;
                // A convex outline crosses a horizontal line in one interval.
                let (mut left, mut right) = (f32::MAX, f32::MIN);
                for (index, &(ax, ay)) in points.iter().enumerate() {
                    let (bx, by) = points[(index + 1) % points.len()];
                    if (ay <= sy && sy < by) || (by <= sy && sy < ay) {
                        let x = ax + (sy - ay) / (by - ay) * (bx - ax);
                        left = left.min(x);
                        right = right.max(x);
                    }
                }
                let left = left.max(span.rect.x);
                let right = right.min(span.rect.right());
                if left >= right {
                    continue;
                }
                let first = (left.floor() as usize).max(span.x0);
                let end = (right.ceil() as usize).min(span.x1);
                for x in first..end {
                    let fx = x as f32;
                    let overlap = (right.min(fx + 1.0) - left.max(fx)).clamp(0.0, 1.0);
                    coverage[x - span.x0] += overlap / SUB_ROWS as f32;
                }
            }
            let row = self.row(y, span.x0, span.x1);
            for (pixel, &cover) in row.as_chunks_mut::<4>().0.iter_mut().zip(&coverage) {
                let alpha = coverage_alpha(color.a, cover);
                if alpha != 0 {
                    over(pixel, premultiply(color, alpha));
                }
            }
        }
    }

    /// Blend an 8-bit coverage mask (a rasterised glyph) at integer
    /// `(x, y)`, tinted `color`.
    pub fn draw_mask(
        &mut self,
        x: i32,
        y: i32,
        width: usize,
        height: usize,
        mask: &Arc<[u8]>,
        color: Color,
    ) {
        if width == 0 || color.a == 0 {
            return;
        }
        if self.record(|clip| Op::Mask {
            x,
            y,
            width,
            height,
            mask: mask.clone(),
            clip,
            color,
        }) {
            return;
        }
        let clip = self.current_clip();
        let cx0 = (clip.x.ceil() as i32).max(x).max(0);
        let cy0 = (clip.y.ceil() as i32).max(y).max(0);
        let cx1 = (clip.right().floor() as i32)
            .min(x + width as i32)
            .min(self.width as i32);
        let cy1 = (clip.bottom().floor() as i32)
            .min(y + height as i32)
            .min(self.height as i32);
        if cx0 >= cx1 || cy0 >= cy1 {
            return;
        }
        let opacity = u32::from(color.a);
        for py in cy0..cy1 {
            let source = (py - y) as usize * width + (cx0 - x) as usize;
            let coverage = &mask[source..source + (cx1 - cx0) as usize];
            let row = self.row(py as usize, cx0 as usize, cx1 as usize);
            for (pixel, &cover) in row.as_chunks_mut::<4>().0.iter_mut().zip(coverage) {
                if cover != 0 {
                    let alpha = div255(opacity * u32::from(cover));
                    over(pixel, premultiply(color, alpha));
                }
            }
        }
    }

    /// Draw a picture scaled into `dest` with bilinear filtering at
    /// `opacity`.
    pub fn draw_image(&mut self, picture: &Arc<Picture>, dest: Rect, opacity: f32) {
        let (width, height) = (picture.width, picture.height);
        if width == 0
            || height == 0
            || picture.rgba.len() < width as usize * height as usize * 4
            || opacity <= 0.0
        {
            return;
        }
        if self.record(|clip| Op::Image {
            picture: picture.clone(),
            dest,
            clip,
            opacity,
        }) {
            return;
        }
        let image = &picture.rgba;
        let Some(span) = self.span(dest) else {
            return;
        };
        let sx = width as f32 / dest.w.max(1.0);
        let sy = height as f32 / dest.h.max(1.0);
        for y in span.y0..span.y1 {
            let cy = span.row_coverage(y);
            let v = ((y as f32 + 0.5 - dest.y) * sy - 0.5).clamp(0.0, (height - 1) as f32);
            let v0 = v.floor() as usize;
            let v1 = (v0 + 1).min(height as usize - 1);
            let fv = v - v0 as f32;
            for x in span.x0..span.x1 {
                let u = ((x as f32 + 0.5 - dest.x) * sx - 0.5).clamp(0.0, (width - 1) as f32);
                let u0 = u.floor() as usize;
                let u1 = (u0 + 1).min(width as usize - 1);
                let fu = u - u0 as f32;
                let texel = |tu: usize, tv: usize, channel: usize| {
                    f32::from(image[(tv * width as usize + tu) * 4 + channel])
                };
                let sample = |channel: usize| {
                    let top = texel(u0, v0, channel) * (1.0 - fu) + texel(u1, v0, channel) * fu;
                    let bottom = texel(u0, v1, channel) * (1.0 - fu) + texel(u1, v1, channel) * fu;
                    (top * (1.0 - fv) + bottom * fv + 0.5) as u8
                };
                let color = Color::rgba(sample(0), sample(1), sample(2), sample(3));
                let coverage = span.column_coverage(x) * cy * opacity;
                self.blend(x, y, color, coverage);
            }
        }
    }
}

/// A premultiplied canvas's pixels as a straight picture.
fn unpremultiplied(canvas: &Canvas) -> Picture {
    let mut rgba = canvas.rgba.clone();
    for pixel in rgba.as_chunks_mut::<4>().0 {
        let alpha = u32::from(pixel[3]);
        if alpha != 0 && alpha != 255 {
            for channel in &mut pixel[..3] {
                *channel = (u32::from(*channel) * 255 / alpha).min(255) as u8;
            }
        }
    }
    Picture {
        width: canvas.width,
        height: canvas.height,
        rgba,
    }
}

/// The pixels a rectangle touches, with its clipped float edges for the
/// partial coverage of the border pixels.
struct Span {
    rect: Rect,
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
}

impl Span {
    #[inline]
    fn column_coverage(&self, x: usize) -> f32 {
        let fx = x as f32;
        (self.rect.right().min(fx + 1.0) - self.rect.x.max(fx)).clamp(0.0, 1.0)
    }

    #[inline]
    fn row_coverage(&self, y: usize) -> f32 {
        let fy = y as f32;
        (self.rect.bottom().min(fy + 1.0) - self.rect.y.max(fy)).clamp(0.0, 1.0)
    }
}

/// `value / 255`, rounded, for `value` up to `255 * 255`.
#[inline]
fn div255(value: u32) -> u32 {
    let value = value + 128;
    (value + (value >> 8)) >> 8
}

/// A colour's alpha (0..=255) scaled by a 0..=1 coverage.
#[inline]
fn coverage_alpha(alpha: u8, coverage: f32) -> u32 {
    if coverage <= 0.0 {
        return 0;
    }
    div255(u32::from(alpha) * (coverage.min(1.0) * 255.0 + 0.5) as u32)
}

/// A straight colour as premultiplied channels at `alpha`.
#[inline]
fn premultiply(color: Color, alpha: u32) -> [u32; 4] {
    [
        div255(u32::from(color.r) * alpha),
        div255(u32::from(color.g) * alpha),
        div255(u32::from(color.b) * alpha),
        alpha,
    ]
}

/// Composite premultiplied `source` over one premultiplied pixel.
#[inline]
fn over(pixel: &mut [u8], source: [u32; 4]) {
    match source[3] {
        0 => return,
        255 => {
            for (channel, &value) in pixel.iter_mut().zip(&source) {
                *channel = value as u8;
            }
            return;
        }
        _ => {}
    }
    let keep = 255 - source[3];
    for (channel, &value) in pixel.iter_mut().zip(&source) {
        *channel = (value + div255(u32::from(*channel) * keep)).min(255) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_rect_blends_partial_edges() {
        let mut canvas = Canvas::new(4, 1);
        canvas.fill_rect(Rect::new(0.5, 0.0, 2.0, 1.0), Color::WHITE);
        let alpha: Vec<u8> = canvas.rgba.chunks(4).map(|pixel| pixel[3]).collect();
        assert_eq!(alpha, vec![128, 255, 128, 0]);
    }

    #[test]
    fn clip_limits_drawing() {
        let mut canvas = Canvas::new(4, 1);
        canvas.push_clip(Rect::new(1.0, 0.0, 1.0, 1.0));
        canvas.fill_rect(canvas.bounds(), Color::WHITE);
        canvas.pop_clip();
        let alpha: Vec<u8> = canvas.rgba.chunks(4).map(|pixel| pixel[3]).collect();
        assert_eq!(alpha, vec![0, 255, 0, 0]);
    }

    #[test]
    fn premultiplied_blend_over_transparent() {
        let mut canvas = Canvas::new(1, 1);
        canvas.fill_rect(canvas.bounds(), Color::rgba(255, 0, 0, 128));
        assert_eq!(&canvas.rgba[..], &[128, 0, 0, 128]);
    }
}
