//! The shared widget painters.
//!
//! Screens never draw chrome themselves: a key cap, a tab strip, a value
//! picker or a dialog band is always one of these, so every screen (and every
//! mod screen built on them) shares the same shapes, spacing and colours.
//! Painters are stateless; the caller owns selection and scroll state and
//! records the returned rectangles for pointer hit tests.

use super::canvas::{Axis, Canvas, Color, Rect};
use super::input::{Device, GuiKey, PadStyle, key_label};
use super::text::{Align, Face, Fonts, Style};
use super::theme::{Role, Theme, palette};

/// The selection sheen's brightness (0..=1). It is steady, so a menu at
/// rest repaints nothing.
pub const SELECTION_GLOW: f32 = 0.5;

/// A canvas, the faces and the theme for one frame's drawing.
pub struct Painter<'a> {
    pub canvas: &'a mut Canvas,
    pub fonts: &'a mut Fonts,
    pub theme: Theme,
}

/// A controller button's glyph.
#[derive(Debug, Clone, Copy, PartialEq)]
enum PadGlyph {
    /// A lettered face button: its letter, fill and ink.
    Face(&'static str, Color, Color),
    /// A PlayStation face button: its symbol and the symbol's colour.
    Symbol(Symbol, Color),
    Bumper(&'static str),
    Trigger(&'static str),
    /// Xbox's View button.
    View,
    /// PlayStation's Create (Share) button.
    Create,
    /// Nintendo's minus button.
    Minus,
    DPad,
}

/// A PlayStation face button's symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Symbol {
    Cross,
    Circle,
    Triangle,
}

/// The controller glyph for `key` in `style`. The keys sit on the same
/// buttons by position whatever the style: confirm south, cancel east,
/// reset-page north.
fn pad_glyph(style: PadStyle, key: GuiKey) -> PadGlyph {
    let white = Color::WHITE;
    let dark = Color::rgb(0x14, 0x14, 0x14);
    let nintendo = Color::rgb(0x3A, 0x3A, 0x3A);
    match (style, key) {
        (_, GuiKey::Up | GuiKey::Down | GuiKey::Left | GuiKey::Right) => PadGlyph::DPad,
        (PadStyle::Xbox, GuiKey::Confirm) => {
            PadGlyph::Face("A", Color::rgb(0x3C, 0xA0, 0x3C), white)
        }
        (PadStyle::Xbox, GuiKey::Cancel) => {
            PadGlyph::Face("B", Color::rgb(0xD0, 0x3A, 0x30), white)
        }
        (PadStyle::Xbox, GuiKey::ResetGroup) => {
            PadGlyph::Face("Y", Color::rgb(0xE8, 0xB8, 0x20), dark)
        }
        (PadStyle::Xbox, GuiKey::TabPrev) => PadGlyph::Bumper("LB"),
        (PadStyle::Xbox, GuiKey::TabNext) => PadGlyph::Bumper("RB"),
        (PadStyle::Xbox, GuiKey::SubPrev) => PadGlyph::Trigger("LT"),
        (PadStyle::Xbox, GuiKey::SubNext) => PadGlyph::Trigger("RT"),
        (PadStyle::Xbox, GuiKey::ResetAll) => PadGlyph::View,
        (PadStyle::PlayStation, GuiKey::Confirm) => {
            PadGlyph::Symbol(Symbol::Cross, Color::rgb(0x8C, 0xB4, 0xF0))
        }
        (PadStyle::PlayStation, GuiKey::Cancel) => {
            PadGlyph::Symbol(Symbol::Circle, Color::rgb(0xF0, 0x6E, 0x78))
        }
        (PadStyle::PlayStation, GuiKey::ResetGroup) => {
            PadGlyph::Symbol(Symbol::Triangle, Color::rgb(0x48, 0xCC, 0xAA))
        }
        (PadStyle::PlayStation, GuiKey::TabPrev) => PadGlyph::Bumper("L1"),
        (PadStyle::PlayStation, GuiKey::TabNext) => PadGlyph::Bumper("R1"),
        (PadStyle::PlayStation, GuiKey::SubPrev) => PadGlyph::Trigger("L2"),
        (PadStyle::PlayStation, GuiKey::SubNext) => PadGlyph::Trigger("R2"),
        (PadStyle::PlayStation, GuiKey::ResetAll) => PadGlyph::Create,
        // Nintendo prints A on the east button and B on the south one.
        (PadStyle::Nintendo, GuiKey::Confirm) => PadGlyph::Face("B", nintendo, white),
        (PadStyle::Nintendo, GuiKey::Cancel) => PadGlyph::Face("A", nintendo, white),
        (PadStyle::Nintendo, GuiKey::ResetGroup) => PadGlyph::Face("X", nintendo, white),
        (PadStyle::Nintendo, GuiKey::TabPrev) => PadGlyph::Bumper("L"),
        (PadStyle::Nintendo, GuiKey::TabNext) => PadGlyph::Bumper("R"),
        (PadStyle::Nintendo, GuiKey::SubPrev) => PadGlyph::Trigger("ZL"),
        (PadStyle::Nintendo, GuiKey::SubNext) => PadGlyph::Trigger("ZR"),
        (PadStyle::Nintendo, GuiKey::ResetAll) => PadGlyph::Minus,
    }
}

/// A chevron's direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chevron {
    Left,
    Right,
}

impl Painter<'_> {
    pub fn u(&self, units: f32) -> f32 {
        self.theme.u(units)
    }

    pub fn style(&self, role: Role) -> Style {
        self.theme.style(role)
    }

    /// Text on one line, vertically centred on `center_y`.
    pub fn text(
        &mut self,
        role: Role,
        x: f32,
        center_y: f32,
        align: Align,
        color: Color,
        text: &str,
    ) -> f32 {
        let style = self.style(role);
        self.fonts
            .draw_centered(self.canvas, style, x, center_y, align, color, text)
    }

    pub fn measure(&mut self, role: Role, text: &str) -> f32 {
        let style = self.style(role);
        self.fonts.measure(style, text)
    }

    /// Wrapped text from `top`; returns the y below the last line.
    pub fn paragraph(
        &mut self,
        role: Role,
        x: f32,
        top: f32,
        max_width: f32,
        color: Color,
        text: &str,
    ) -> f32 {
        let style = self.style(role);
        let metrics = self.fonts.metrics(style);
        let mut baseline = top + metrics.ascent;
        for line in self.fonts.wrap(style, text, max_width) {
            self.fonts
                .draw(self.canvas, style, x, baseline, Align::Left, color, &line);
            baseline += metrics.line_height;
        }
        baseline - metrics.ascent
    }

    /// The height one line of `role` advances.
    pub fn line_height(&self, role: Role) -> f32 {
        self.fonts.metrics(self.style(role)).line_height
    }

    /// A horizontal rule `thickness` units thick.
    pub fn rule(&mut self, x: f32, y: f32, width: f32, thickness: f32, color: Color) {
        let t = self.u(thickness).max(1.0);
        self.canvas.fill_rect(Rect::new(x, y, width, t), color);
    }

    /// A vertical rule `thickness` units thick.
    pub fn vrule(&mut self, x: f32, y: f32, height: f32, thickness: f32, color: Color) {
        let t = self.u(thickness).max(1.0);
        self.canvas.fill_rect(Rect::new(x, y, t, height), color);
    }

    /// A keyboard key cap (`Q`, `Tab`, `Esc`) left-aligned at `x` and centred
    /// on `center_y`; returns its rectangle.
    pub fn key_cap(&mut self, x: f32, center_y: f32, label: &str) -> Rect {
        let style = self.style(Role::KeyCap);
        let height = self.u(32.0);
        let rect = Rect::new(
            x,
            center_y - height / 2.0,
            self.key_cap_width(label),
            height,
        );
        self.canvas.fill_rect(rect, palette::TEXT);
        let inner = rect.inset(self.u(2.0), self.u(2.0));
        self.canvas
            .stroke_rect(inner, self.u(1.5).max(1.0), Color::rgb(0x30, 0x30, 0x30));
        self.fonts.draw_centered(
            self.canvas,
            style,
            rect.center_x(),
            center_y,
            Align::Center,
            Color::rgb(0x10, 0x10, 0x10),
            label,
        );
        rect
    }

    fn key_cap_width(&mut self, label: &str) -> f32 {
        let style = self.style(Role::KeyCap);
        let text_width = self.fonts.measure(style, label);
        let height = self.u(32.0);
        (text_width + self.u(12.0)).max(height * 0.86)
    }

    /// The glyph for `key` on `device`, left-aligned at `x` and centred on
    /// `center_y`: a key cap on the keyboard, the controller's own button
    /// shape on a gamepad. Returns its rectangle.
    pub fn glyph(&mut self, x: f32, center_y: f32, device: Device, key: GuiKey) -> Rect {
        match device {
            Device::Gamepad(style) => self.pad_glyph(x, center_y, pad_glyph(style, key)),
            Device::Keyboard => self.key_cap(x, center_y, key_label(key)),
        }
    }

    /// The width [`Painter::glyph`] would draw.
    pub fn glyph_width(&mut self, device: Device, key: GuiKey) -> f32 {
        match device {
            Device::Gamepad(style) => self.pad_glyph_size(pad_glyph(style, key)).0,
            Device::Keyboard => self.key_cap_width(key_label(key)),
        }
    }

    fn pad_glyph_size(&self, glyph: PadGlyph) -> (f32, f32) {
        match glyph {
            PadGlyph::Face(..)
            | PadGlyph::Symbol(..)
            | PadGlyph::View
            | PadGlyph::Create
            | PadGlyph::Minus
            | PadGlyph::DPad => (self.u(36.0), self.u(36.0)),
            PadGlyph::Bumper(_) => (self.u(54.0), self.u(28.0)),
            PadGlyph::Trigger(_) => (self.u(42.0), self.u(36.0)),
        }
    }

    /// Draw a controller button glyph.
    fn pad_glyph(&mut self, x: f32, center_y: f32, glyph: PadGlyph) -> Rect {
        let (width, height) = self.pad_glyph_size(glyph);
        let rect = Rect::new(x, center_y - height / 2.0, width, height);
        let style = self.style(Role::KeyCap);
        let bold = Style {
            face: Face::Bold,
            ..style
        };
        let dark = Color::rgb(0x14, 0x14, 0x14);
        match glyph {
            PadGlyph::Face(letter, color, ink) => {
                self.canvas.fill_round_rect(rect, width / 2.0, color);
                self.fonts.draw_centered(
                    self.canvas,
                    bold,
                    rect.center_x(),
                    center_y,
                    Align::Center,
                    ink,
                    letter,
                );
            }
            PadGlyph::Bumper(label) => {
                self.canvas
                    .fill_round_rect(rect, height * 0.45, palette::TEXT);
                self.fonts.draw_centered(
                    self.canvas,
                    bold,
                    rect.center_x(),
                    center_y,
                    Align::Center,
                    dark,
                    label,
                );
            }
            PadGlyph::Trigger(label) => {
                // Rounded on top, square at the bottom like the trigger's
                // face.
                self.canvas
                    .fill_round_rect(rect, width * 0.4, palette::TEXT);
                self.canvas.fill_rect(
                    Rect::new(rect.x, rect.center_y(), rect.w, rect.h / 2.0),
                    palette::TEXT,
                );
                self.fonts.draw_centered(
                    self.canvas,
                    bold,
                    rect.center_x(),
                    center_y + self.u(2.0),
                    Align::Center,
                    dark,
                    label,
                );
            }
            PadGlyph::View => {
                self.canvas
                    .fill_round_rect(rect, width / 2.0, palette::TEXT);
                // Two overlapping windows.
                let side = self.u(11.0);
                let line = self.u(2.0).max(1.0);
                let (cx, cy) = (rect.center_x(), rect.center_y());
                let back = Rect::new(cx - side * 0.75, cy - side * 0.75, side, side);
                let front = Rect::new(cx - side * 0.25, cy - side * 0.25, side, side);
                self.canvas.stroke_rect(back, line, dark);
                self.canvas.fill_rect(front, palette::TEXT);
                self.canvas.stroke_rect(front, line, dark);
            }
            PadGlyph::Symbol(symbol, color) => {
                self.canvas
                    .fill_round_rect(rect, width / 2.0, Color::rgb(0x5A, 0x5A, 0x5A));
                let rim = self.u(2.0).max(1.0);
                self.canvas.fill_round_rect(
                    rect.inset(rim, rim),
                    width / 2.0 - rim,
                    Color::rgb(0x1E, 0x1E, 0x1E),
                );
                self.ps_symbol(rect.center_x(), rect.center_y(), symbol, color);
            }
            PadGlyph::Create => {
                self.canvas
                    .fill_round_rect(rect, width / 2.0, palette::TEXT);
                // Three short strokes, the button's pencil-like mark.
                let (cx, cy) = (rect.center_x(), rect.center_y());
                let line = self.u(2.4).max(1.0);
                let reach = self.u(6.0);
                for offset in [-1.0f32, 0.0, 1.0] {
                    let x = cx + offset * self.u(5.0);
                    self.canvas.fill_polygon(
                        &[
                            (x - line / 2.0 + reach * 0.3, cy - reach),
                            (x + line / 2.0 + reach * 0.3, cy - reach),
                            (x + line / 2.0 - reach * 0.3, cy + reach),
                            (x - line / 2.0 - reach * 0.3, cy + reach),
                        ],
                        dark,
                    );
                }
            }
            PadGlyph::Minus => {
                self.canvas
                    .fill_round_rect(rect, width / 2.0, palette::TEXT);
                let bar = Rect::new(
                    rect.center_x() - self.u(8.0),
                    rect.center_y() - self.u(1.5),
                    self.u(16.0),
                    self.u(3.0).max(1.0),
                );
                self.canvas.fill_rect(bar, dark);
            }
            PadGlyph::DPad => {
                self.canvas
                    .fill_round_rect(rect, width / 2.0, Color::rgb(0x3A, 0x3A, 0x3A));
                let arm = self.u(9.0);
                let reach = self.u(13.0);
                let (cx, cy) = (rect.center_x(), rect.center_y());
                let radius = self.u(2.0);
                self.canvas.fill_round_rect(
                    Rect::new(cx - arm / 2.0, cy - reach, arm, reach * 2.0),
                    radius,
                    palette::TEXT,
                );
                self.canvas.fill_round_rect(
                    Rect::new(cx - reach, cy - arm / 2.0, reach * 2.0, arm),
                    radius,
                    palette::TEXT,
                );
            }
        }
        rect
    }

    /// A PlayStation face symbol, outlined, centred on `(cx, cy)`.
    fn ps_symbol(&mut self, cx: f32, cy: f32, symbol: Symbol, color: Color) {
        let size = self.u(9.0);
        let line = self.u(2.6).max(1.0);
        let hole = Color::rgb(0x1E, 0x1E, 0x1E);
        match symbol {
            Symbol::Circle => {
                let outer = Rect::new(cx - size, cy - size, size * 2.0, size * 2.0);
                self.canvas.fill_round_rect(outer, size, color);
                self.canvas
                    .fill_round_rect(outer.inset(line, line), size - line, hole);
            }
            Symbol::Triangle => {
                // Equilateral, its centroid on the centre.
                let r = size * 1.15;
                let corners = |radius: f32| {
                    [-90.0f32, 30.0, 150.0].map(|degrees| {
                        let angle = degrees.to_radians();
                        (cx + radius * angle.cos(), cy + radius * angle.sin())
                    })
                };
                // The inner triangle's corners sit twice the stroke closer
                // (the inradius is half the circumradius).
                self.canvas.fill_polygon(&corners(r), color);
                self.canvas.fill_polygon(&corners(r - line * 2.0), hole);
            }
            Symbol::Cross => {
                let reach = size * 0.75;
                let half = line / 2.0 * std::f32::consts::SQRT_2;
                for sign in [1.0f32, -1.0] {
                    // A bar from top-left to bottom-right, or mirrored.
                    self.canvas.fill_polygon(
                        &[
                            (cx - sign * reach - sign * half, cy - reach),
                            (cx - sign * reach + sign * half, cy - reach),
                            (cx + sign * reach + sign * half, cy + reach),
                            (cx + sign * reach - sign * half, cy + reach),
                        ],
                        color,
                    );
                }
            }
        }
    }

    /// One input hint: the key's glyph and its action; returns the whole
    /// hint's rectangle, which a pointer can click.
    pub fn hint(
        &mut self,
        x: f32,
        center_y: f32,
        device: Device,
        key: GuiKey,
        label: &str,
    ) -> Rect {
        let glyph = self.glyph(x, center_y, device, key);
        let text_x = glyph.right() + self.u(14.0);
        let width = self.text(
            Role::Hint,
            text_x,
            center_y,
            Align::Left,
            palette::TEXT,
            label,
        );
        Rect::new(x, glyph.y, text_x + width - x, glyph.h)
    }

    /// The width [`Painter::hint`] would draw.
    pub fn hint_width(&mut self, device: Device, key: GuiKey, label: &str) -> f32 {
        self.glyph_width(device, key) + self.u(14.0) + self.measure(Role::Hint, label)
    }

    /// A centred row of hints; returns each hint's rectangle in order.
    pub fn hint_bar(
        &mut self,
        center_x: f32,
        center_y: f32,
        device: Device,
        hints: &[(GuiKey, &str)],
    ) -> Vec<Rect> {
        let gap = self.u(36.0);
        let total: f32 = hints
            .iter()
            .map(|&(key, label)| self.hint_width(device, key, label))
            .sum::<f32>()
            + gap * hints.len().saturating_sub(1) as f32;
        let mut x = center_x - total / 2.0;
        let mut rects = Vec::with_capacity(hints.len());
        for &(key, label) in hints {
            let rect = self.hint(x, center_y, device, key, label);
            x = rect.right() + gap;
            rects.push(rect);
        }
        rects
    }

    /// A thin chevron `size` units tall centred on `(cx, cy)`.
    pub fn chevron(&mut self, cx: f32, cy: f32, size: f32, direction: Chevron, color: Color) {
        let h = self.u(size) / 2.0;
        let w = h * 0.55;
        let t = self.u(2.4).max(1.2);
        let (tip, back) = match direction {
            Chevron::Left => (cx - w / 2.0, cx + w / 2.0),
            Chevron::Right => (cx + w / 2.0, cx - w / 2.0),
        };
        let inward = if direction == Chevron::Left { t } else { -t };
        // Two strokes meeting at the tip.
        self.canvas.fill_polygon(
            &[
                (back, cy - h),
                (back + inward, cy - h),
                (tip + inward, cy),
                (tip, cy),
            ],
            color,
        );
        self.canvas.fill_polygon(
            &[
                (tip, cy),
                (tip + inward, cy),
                (back + inward, cy + h),
                (back, cy + h),
            ],
            color,
        );
    }

    /// A small filled diamond (the changed-from-default marker).
    pub fn diamond(&mut self, cx: f32, cy: f32, radius: f32, color: Color) {
        let r = self.u(radius);
        self.canvas.fill_polygon(
            &[(cx, cy - r), (cx + r, cy), (cx, cy + r), (cx - r, cy)],
            color,
        );
    }

    /// The selected row's silver sheen.
    pub fn selection(&mut self, rect: Rect, pulse: f32) {
        let glow = 0.85 + 0.15 * pulse;
        self.canvas.fill_gradient(
            rect,
            Axis::Vertical,
            Color::rgba(0x70, 0x70, 0x70, (230.0 * glow) as u8),
            Color::rgba(0x38, 0x38, 0x38, (230.0 * glow) as u8),
        );
        // A soft bright band through the lower middle, fading out sideways.
        let band = Rect::new(rect.x, rect.bottom() - rect.h * 0.32, rect.w, rect.h * 0.18);
        let half = Rect::new(band.x, band.y, band.w / 2.0, band.h);
        self.canvas.fill_gradient(
            half,
            Axis::Horizontal,
            Color::rgba(0xFF, 0xFF, 0xFF, 0),
            Color::rgba(0xFF, 0xFF, 0xFF, (70.0 * glow) as u8),
        );
        self.canvas.fill_gradient(
            Rect::new(half.right(), band.y, band.w / 2.0, band.h),
            Axis::Horizontal,
            Color::rgba(0xFF, 0xFF, 0xFF, (70.0 * glow) as u8),
            Color::rgba(0xFF, 0xFF, 0xFF, 0),
        );
        let edge = self.u(2.0).max(1.0);
        self.canvas.fill_rect(
            Rect::new(rect.x, rect.y, rect.w, edge),
            Color::rgba(0xE0, 0xE0, 0xE0, 0xC0),
        );
        self.canvas.fill_rect(
            Rect::new(rect.x, rect.bottom() - edge, rect.w, edge),
            Color::rgba(0xE0, 0xE0, 0xE0, 0xC0),
        );
    }

    /// A tab's underline glow: bright in the middle, fading to the sides.
    pub fn underline_glow(&mut self, center_x: f32, y: f32, width: f32) {
        let t = self.u(3.0).max(1.0);
        let half = width / 2.0;
        self.canvas.fill_gradient(
            Rect::new(center_x - half, y - t, half, t),
            Axis::Horizontal,
            Color::rgba(0xFF, 0xFF, 0xFF, 0),
            Color::WHITE,
        );
        self.canvas.fill_gradient(
            Rect::new(center_x, y - t, half, t),
            Axis::Horizontal,
            Color::WHITE,
            Color::rgba(0xFF, 0xFF, 0xFF, 0),
        );
    }

    /// A group header strip: a slanted grey tab with `label`.
    pub fn group_header(&mut self, rect: Rect, label: &str) {
        let slant = rect.h * 0.9;
        let text_width = self.measure(Role::Group, label);
        let width = (text_width + self.u(60.0) + slant)
            .max(self.u(420.0))
            .min(rect.w);
        // The strip's body, then its slanted right end in the same gradient.
        let steps = 12;
        for step in 0..steps {
            let t0 = step as f32 / steps as f32;
            let t1 = (step + 1) as f32 / steps as f32;
            let color = palette::GROUP_TOP.mix(palette::GROUP_BOTTOM, (t0 + t1) / 2.0);
            let y0 = rect.y + rect.h * t0;
            let y1 = rect.y + rect.h * t1;
            let x0 = rect.x + width - slant + slant * t0;
            let x1 = rect.x + width - slant + slant * t1;
            self.canvas
                .fill_polygon(&[(rect.x, y0), (x0, y0), (x1, y1), (rect.x, y1)], color);
        }
        self.rule(
            rect.x,
            rect.bottom() - self.u(1.0),
            rect.w,
            1.0,
            palette::RULE,
        );
        self.text(
            Role::Group,
            rect.x + self.u(12.0),
            rect.center_y(),
            Align::Left,
            palette::TEXT,
            label,
        );
    }

    /// The value pips under an enumerated value: one segment per choice,
    /// the current one bright.
    pub fn pips(&mut self, center_x: f32, y: f32, count: usize, current: usize) {
        if count < 2 {
            return;
        }
        let width = self.u(if count > 6 { 18.0 } else { 36.0 });
        let gap = self.u(5.0);
        let height = self.u(3.0).max(1.0);
        let total = width * count as f32 + gap * (count - 1) as f32;
        let mut x = center_x - total / 2.0;
        for index in 0..count {
            let color = if index == current {
                palette::TEXT
            } else {
                palette::PIP
            };
            self.canvas.fill_rect(Rect::new(x, y, width, height), color);
            x += width + gap;
        }
    }

    /// A tick-marked slider track across `rect` with its thumb at `fraction`.
    pub fn slider(&mut self, rect: Rect, fraction: f32, ticks: usize, color: Color) {
        let cy = rect.center_y();
        let line = self.u(1.5).max(1.0);
        self.canvas.fill_rect(
            Rect::new(rect.x, cy - line / 2.0, rect.w, line),
            color.fade(0.8),
        );
        let ticks = ticks.max(2);
        for index in 0..ticks {
            let x = rect.x + rect.w * index as f32 / (ticks - 1) as f32;
            let major = index == 0 || index == ticks - 1 || index * 2 == ticks - 1;
            let h = self.u(if major { 22.0 } else { 12.0 });
            self.canvas.fill_rect(
                Rect::new(x - line / 2.0, cy - h / 2.0, line, h),
                color.fade(0.8),
            );
        }
        let x = rect.x + rect.w * fraction.clamp(0.0, 1.0);
        let thumb = self.u(3.0).max(1.5);
        let h = self.u(36.0);
        self.canvas.fill_rect(
            Rect::new(x - thumb / 2.0, cy - h / 2.0, thumb, h),
            palette::TEXT,
        );
    }

    /// A vertical scrollbar: the track and a thumb covering `visible` of
    /// `total` from `offset`.
    pub fn scrollbar(&mut self, rect: Rect, offset: f32, visible: f32, total: f32) {
        let track = self.u(2.0).max(1.0);
        self.canvas.fill_rect(
            Rect::new(rect.center_x() - track / 2.0, rect.y, track, rect.h),
            palette::RULE,
        );
        if total <= visible || total <= 0.0 {
            self.canvas.fill_rect(
                Rect::new(rect.center_x() - track, rect.y, track * 2.0, rect.h),
                palette::TEXT,
            );
            return;
        }
        let thumb_h = (rect.h * visible / total).max(self.u(40.0));
        let travel = rect.h - thumb_h;
        let y = rect.y + travel * (offset / (total - visible)).clamp(0.0, 1.0);
        self.canvas.fill_rect(
            Rect::new(rect.center_x() - track, y, track * 2.0, thumb_h),
            palette::TEXT,
        );
    }
}
