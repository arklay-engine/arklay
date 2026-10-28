//! The debug menu's lists, grids and tab strips, built on the shared GUI
//! painters: every tool scrolls, highlights, steps and hit-tests the same
//! way, and the rows look like the options screen's.

use std::sync::Arc;

use super::{Cx, Hit, fit};
use crate::gui::canvas::{Axis, Color, Picture, Rect};
use crate::gui::input::GuiKey;
use crate::gui::text::Align;
use crate::gui::theme::{Role, palette};
use crate::gui::widgets::Painter;
use crate::ui::UiInput;

/// A list row's height and the widest a list runs, in units.
pub(super) const ROW_H: f32 = 66.0;
pub(super) const ROW_W: f32 = 900.0;
/// Where a row's value arrows sit, as fractions of its width.
const VALUE_LEFT: f32 = 0.565;
const VALUE_RIGHT: f32 = 0.965;
/// The leading column (a slot number, a flag selector) and its gap.
const LEAD_W: f32 = 64.0;

/// A scrolled view's first shown row. It keeps the cursor in view until the
/// wheel scrolls it, and stops at either end.
#[derive(Debug, Clone, Copy)]
pub(super) struct Scroll {
    top: usize,
    follow: bool,
}

impl Default for Scroll {
    fn default() -> Self {
        Self {
            top: 0,
            follow: true,
        }
    }
}

impl Scroll {
    /// The first shown row as last laid out.
    pub fn top(&self) -> usize {
        self.top
    }

    /// Bring the cursor back into view on the next layout.
    pub fn follow(&mut self) {
        self.follow = true;
    }

    /// Scroll by `rows` and leave the cursor where it is.
    pub fn by(&mut self, rows: isize) {
        self.top = self.top.saturating_add_signed(rows);
        self.follow = false;
    }

    /// Lay out `total` rows of which `visible` fit, the cursor on row
    /// `cursor`; returns the first shown row.
    pub fn layout(&mut self, cursor: usize, visible: usize, total: usize) -> usize {
        let visible = visible.max(1);
        if self.follow {
            if cursor < self.top {
                self.top = cursor;
            } else if cursor >= self.top + visible {
                self.top = cursor + 1 - visible;
            }
        }
        self.top = self.top.min(total.saturating_sub(visible));
        self.top
    }
}

/// Step a `count`-way choice at `current` the options screen's way:
/// left/right stop at the ends, confirm wraps. `None` when nothing changes.
pub(super) fn step_choice(current: usize, count: usize, input: UiInput) -> Option<usize> {
    let next = if input.left {
        current.checked_sub(1)?
    } else if input.right {
        (current + 1 < count).then_some(current + 1)?
    } else if input.confirm {
        (current + 1) % count.max(1)
    } else {
        return None;
    };
    (next != current).then_some(next)
}

/// Move a grid cursor at `cell` of `count` cells laid `columns` to a row:
/// left/right step through every cell and wrap, up/down move a row and stop
/// at the first and last rows. `None` when nothing changes.
pub(super) fn grid_step(
    cell: usize,
    count: usize,
    columns: usize,
    input: UiInput,
) -> Option<usize> {
    if count == 0 {
        return None;
    }
    let columns = columns.max(1);
    let next = if input.left {
        (cell + count - 1) % count
    } else if input.right {
        (cell + 1) % count
    } else if input.up {
        cell.checked_sub(columns)?
    } else if input.down {
        if cell + columns < count {
            cell + columns
        } else if cell / columns < (count - 1) / columns {
            count - 1
        } else {
            return None;
        }
    } else {
        return None;
    };
    (next != cell).then_some(next)
}

/// What a row shows on its right.
pub(super) enum Value<'a> {
    None,
    /// A value stepped between arrows, with (count, current) pips.
    Step(&'a str, Option<(usize, usize)>),
    /// The options screen's link mark, with an optional note beside it.
    Link(Option<&'a str>),
}

/// What a row shows on its left.
pub(super) struct Label<'a> {
    /// A dim leading column: a slot number or a selector.
    pub lead: Option<&'a str>,
    /// An icon column; `Some(None)` keeps the column with nothing in it.
    pub icon: Option<Option<Arc<Picture>>>,
    pub text: &'a str,
    pub color: Color,
}

impl<'a> Label<'a> {
    pub fn plain(text: &'a str) -> Self {
        Self {
            lead: None,
            icon: None,
            text,
            color: palette::TEXT,
        }
    }

    pub fn lead(lead: &'a str, text: &'a str) -> Self {
        Self {
            lead: Some(lead),
            ..Self::plain(text)
        }
    }
}

/// One list row, options-style: the selection sheen and foot rule, the
/// label and the value. Clicking the row picks it; clicking either half of
/// a stepped value steps it.
pub(super) fn row(
    p: &mut Painter<'_>,
    cx: &mut Cx<'_>,
    rect: Rect,
    index: usize,
    selected: bool,
    label: Label<'_>,
    value: Value<'_>,
) {
    let theme = p.theme;
    let u = |v: f32| theme.u(v);
    p.list_row(rect, selected);
    let cy = rect.center_y();
    let left_arrow = rect.x + rect.w * VALUE_LEFT;
    let right_arrow = rect.x + rect.w * VALUE_RIGHT;
    let mut x = rect.x + u(30.0);
    if let Some(lead) = label.lead {
        p.text(Role::Row, x, cy, Align::Left, palette::TEXT_DIM, lead);
        x += u(LEAD_W);
    }
    if let Some(icon) = label.icon {
        let h = rect.h - u(12.0);
        let frame = Rect::new(x, cy - h / 2.0, h * 4.0 / 3.0, h);
        if let Some(icon) = icon {
            p.canvas.draw_image(&icon, frame, 1.0);
        }
        x = frame.right() + u(20.0);
    }
    let text = fit(p, Role::Row, label.text, left_arrow - x - u(30.0));
    p.text(Role::Row, x, cy, Align::Left, label.color, &text);
    cx.hits.push((rect, Hit::Item(index)));
    match value {
        Value::None => {}
        Value::Step(value, pips) => {
            let value = fit(p, Role::Row, value, right_arrow - left_arrow - u(60.0));
            let (decrease, increase) = p.stepper(
                rect,
                left_arrow,
                right_arrow,
                &value,
                pips,
                palette::TEXT,
                palette::TEXT,
            );
            cx.hits.push((decrease, Hit::Decrease(index)));
            cx.hits.push((increase, Hit::Increase(index)));
        }
        Value::Link(note) => {
            if let Some(note) = note {
                let center = (left_arrow + right_arrow) / 2.0;
                p.text(
                    Role::Row,
                    center,
                    cy,
                    Align::Center,
                    palette::TEXT_DIM,
                    note,
                );
            }
            p.link_icon(right_arrow, cy, 30.0, palette::TEXT);
        }
    }
}

/// The scrollbar beside a list or grid when it does not all fit.
pub(super) fn scrollbar(
    p: &mut Painter<'_>,
    x: f32,
    y: f32,
    h: f32,
    top: usize,
    visible: usize,
    total: usize,
) {
    if total > visible {
        let bar = Rect::new(x, y, p.u(8.0), h);
        p.scrollbar(bar, top as f32, visible as f32, total as f32);
    }
}

/// A tab strip across the top of `area`, like the options screen's
/// subcategories, with the page keys' glyphs at its ends; returns its
/// height.
pub(super) fn tabs(
    p: &mut Painter<'_>,
    cx: &mut Cx<'_>,
    area: Rect,
    labels: &[String],
    selected: usize,
    dim: &[bool],
) -> f32 {
    let theme = p.theme;
    let u = |v: f32| theme.u(v);
    let cy = area.y + u(24.0);
    let strip = p.sub_tabs(
        area.x + u(35.0),
        cy,
        area.right(),
        cx.device,
        (GuiKey::TabPrev, GuiKey::TabNext),
        labels,
        selected,
        dim,
    );
    cx.hits.push((strip.prev, Hit::Key(GuiKey::TabPrev)));
    for (index, rect) in strip.tabs.into_iter().enumerate() {
        cx.hits.push((rect, Hit::Tab(index)));
    }
    cx.hits.push((strip.next, Hit::Key(GuiKey::TabNext)));
    p.rule(area.x, cy + u(25.0), area.w, 2.0, palette::RULE_BRIGHT);
    u(70.0)
}

/// A grid of picture cells laid out across an area.
pub(super) struct Grid {
    area: Rect,
    pub columns: usize,
    cell_w: f32,
    picture_h: f32,
    cell_h: f32,
    gap: f32,
    /// Whole rows that fit.
    pub rows: usize,
}

impl Grid {
    /// Lay out cells at least `min_w` units wide with 4:3 pictures and,
    /// with `labelled`, a caption line under each, leaving room for the
    /// scrollbar on the right.
    pub fn new(p: &Painter<'_>, area: Rect, min_w: f32, labelled: bool) -> Self {
        let u = |v: f32| p.theme.u(v);
        let area = Rect::new(area.x, area.y, area.w - u(28.0), area.h);
        let gap = u(18.0);
        let columns = (((area.w + gap) / (u(min_w) + gap)).floor() as usize).max(2);
        let cell_w = (area.w - gap * (columns - 1) as f32) / columns as f32;
        let picture_h = cell_w * 0.75;
        let label_h = if labelled {
            p.line_height(Role::Body) * 1.25
        } else {
            0.0
        };
        let cell_h = picture_h + label_h;
        let rows = (((area.h + gap) / (cell_h + gap)).floor() as usize).max(1);
        Self {
            area,
            columns,
            cell_w,
            picture_h,
            cell_h,
            gap,
            rows,
        }
    }

    /// The `shown`th cell on screen: its whole rectangle and its picture.
    pub fn cell(&self, shown: usize) -> (Rect, Rect) {
        let x = self.area.x + (self.cell_w + self.gap) * (shown % self.columns) as f32;
        let y = self.area.y + (self.cell_h + self.gap) * (shown / self.columns) as f32;
        (
            Rect::new(x, y, self.cell_w, self.cell_h),
            Rect::new(x, y, self.cell_w, self.picture_h),
        )
    }

    /// The scrollbar for `total` rows from row `top`.
    pub fn scrollbar(&self, p: &mut Painter<'_>, top: usize, total: usize) {
        let x = self.area.right() + p.u(12.0);
        scrollbar(p, x, self.area.y, self.area.h, top, self.rows, total);
    }
}

/// A grid cell's picture: a dark backdrop, the picture dimmed unless
/// `selected`, and the accent outline on the selected cell.
pub(super) fn grid_picture(
    p: &mut Painter<'_>,
    frame: Rect,
    picture: Option<&Arc<Picture>>,
    selected: bool,
) {
    let u = |v: f32| p.theme.u(v);
    let (outline, rule) = (u(3.0).max(1.0), u(1.0).max(1.0));
    p.canvas.fill_gradient(
        frame,
        Axis::Vertical,
        Color::rgb(0x26, 0x26, 0x26),
        Color::rgb(0x14, 0x14, 0x14),
    );
    if let Some(picture) = picture {
        p.canvas
            .draw_image(picture, frame, if selected { 1.0 } else { 0.65 });
    }
    if selected {
        p.canvas
            .stroke_rect(frame.inset(-outline, -outline), outline, palette::ACCENT);
    } else {
        p.canvas.stroke_rect(frame, rule, palette::RULE);
    }
}
