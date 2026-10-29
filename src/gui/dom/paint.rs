//! Painting: every widget drawn with the shared [`Painter`] in the rect
//! layout gave it, in the ink its row lends it.

use std::collections::HashMap;

use crate::gui::canvas::{Axis, Color, Rect};
use crate::gui::input::{Device, GuiKey};
use crate::gui::text::Align;
use crate::gui::theme::{Role, palette};
use crate::gui::widgets::{Chevron, Painter, SELECTION_GLOW};

use super::Custom;
use super::layout::{self, present};
use super::style::{self, Ink};
use super::tree::{Kind, NodeId, OptionKind, Part, Tree};
use super::value::Value;

/// What painting needs besides the tree.
pub struct PaintCx<'a> {
    pub device: Device,
    pub focus: Option<NodeId>,
    /// The hint bar's hints.
    pub hints: Vec<(GuiKey, String)>,
    /// The titles of the screens under this one.
    pub crumbs: Vec<String>,
    pub customs: &'a HashMap<String, Box<dyn Custom>>,
}

pub fn paint(tree: &mut Tree, id: NodeId, p: &mut Painter<'_>, cx: &PaintCx<'_>, ink: Ink) {
    if !tree[id].shown {
        return;
    }
    let rect = tree[id].rect;
    let kind = tree[id].kind;
    let variant = tree[id].variant();
    match kind {
        Kind::Frame => {
            if variant == "modal" {
                let bounds = p.canvas.bounds();
                p.canvas.fill_rect(bounds, style::MODAL_SHADE);
            } else {
                let bounds = p.canvas.bounds();
                // A darker band down the left, where the content starts.
                p.canvas.fill_gradient(
                    Rect::new(0.0, 0.0, bounds.w * 0.6, bounds.h),
                    Axis::Horizontal,
                    Color::rgba(0, 0, 0, 120),
                    Color::rgba(0, 0, 0, 0),
                );
            }
            children(tree, id, p, cx, ink);
        }
        Kind::Stack | Kind::Tab | Kind::Split => children(tree, id, p, cx, ink),
        Kind::Text => text(tree, id, p, cx, ink),
        Kind::Rule => {
            p.canvas.fill_gradient(
                Rect::new(rect.x, rect.y, rect.w, p.u(1.5).max(1.0)),
                Axis::Horizontal,
                palette::RULE_BRIGHT.fade(0.8),
                palette::RULE.fade(0.0),
            );
        }
        Kind::Separator => p.canvas.fill_rect(rect, ink.rule),
        Kind::Image => {
            if let Value::Image(image) = tree[id].prop("src").clone() {
                p.canvas.fill_rect(rect, Color::rgba(0, 0, 0, 200));
                let picture = match (&image.grey, ink.vivid) {
                    (Some(grey), false) => grey,
                    _ => &image.colour,
                };
                let opacity = if ink.vivid { 1.0 } else { 0.8 };
                p.canvas.push_clip(rect);
                // Fill the frame, trimming the picture's sides to it.
                let scale = (rect.w / picture.width.max(1) as f32)
                    .max(rect.h / picture.height.max(1) as f32);
                let (w, h) = (picture.width as f32 * scale, picture.height as f32 * scale);
                p.canvas.draw_image(
                    picture,
                    Rect::new(rect.center_x() - w / 2.0, rect.center_y() - h / 2.0, w, h),
                    opacity,
                );
                p.canvas.pop_clip();
                p.canvas.stroke_rect(rect, p.u(1.0).max(1.0), ink.rule);
            }
        }
        Kind::Badge => {
            p.canvas.fill_rect(rect, style::badge_color(&variant));
            let label = tree[id].text("text");
            p.text(
                Role::Badge,
                rect.center_x(),
                rect.center_y(),
                Align::Center,
                Color::WHITE,
                &label,
            );
        }
        Kind::List => {
            let node = &tree[id];
            let (content, scroll) = (node.content, node.scroll);
            p.canvas.push_clip(rect);
            for c in tree[id].children.clone() {
                let r = tree[c].rect;
                if r.bottom() > rect.y && r.y < rect.bottom() {
                    paint(tree, c, p, cx, ink);
                }
            }
            p.canvas.pop_clip();
            if content > rect.h + 0.5 {
                let bar = Rect::new(rect.right() - p.u(8.0), rect.y, p.u(8.0), rect.h);
                p.scrollbar(bar, scroll, rect.h, content);
            }
        }
        Kind::Group => {
            let label = tree[id].text("label");
            let first = rect.y
                + (tree[id].rect.h
                    - tree[id]
                        .children
                        .iter()
                        .filter(|&&c| tree[c].shown)
                        .map(|&c| tree[c].rect.h)
                        .sum::<f32>()
                    - p.u(style::GROUP_H));
            p.group_header(
                Rect::new(rect.x, first, rect.w, p.u(style::GROUP_H)),
                &label,
            );
            children(tree, id, p, cx, ink);
        }
        Kind::Tabs => tabs(tree, id, p, cx, ink),
        Kind::Item => item(tree, id, p, cx),
        Kind::Option(option) => option_row(tree, id, option, p, cx),
        Kind::HintBar => {
            let hints: Vec<(GuiKey, &str)> =
                cx.hints.iter().map(|(k, s)| (*k, s.as_str())).collect();
            let rects = p.hint_bar(rect.center_x(), rect.center_y(), cx.device, &hints);
            tree[id].parts = rects
                .into_iter()
                .zip(cx.hints.iter().map(|(k, _)| Part::Key(*k)))
                .collect();
        }
        Kind::Prompt => {
            let y = rect.y + p.u(6.0);
            let line = p.u(1.0).max(1.0);
            let half = rect.w * 0.3;
            p.canvas.fill_gradient(
                Rect::new(rect.center_x() - half, y, half, line),
                Axis::Horizontal,
                palette::RULE.fade(0.0),
                palette::RULE,
            );
            p.canvas.fill_gradient(
                Rect::new(rect.center_x(), y, half, line),
                Axis::Horizontal,
                palette::RULE,
                palette::RULE.fade(0.0),
            );
            let label = tree[id].text("text");
            p.text(
                Role::Body,
                rect.center_x(),
                y + (rect.bottom() - y) / 2.0,
                Align::Center,
                palette::TEXT,
                &label,
            );
        }
        Kind::Dialog => dialog(tree, id, p, cx),
        Kind::Facts => facts(tree, id, p),
        Kind::Custom => {
            if let Some(custom) = cx.customs.get(&tree[id].name) {
                custom.paint(p, rect, &tree[id]);
            }
        }
        Kind::Spacer => {}
    }
}

fn children(tree: &mut Tree, id: NodeId, p: &mut Painter<'_>, cx: &PaintCx<'_>, ink: Ink) {
    for c in tree[id].children.clone() {
        paint(tree, c, p, cx, ink);
    }
}

fn text(tree: &mut Tree, id: NodeId, p: &mut Painter<'_>, cx: &PaintCx<'_>, ink: Ink) {
    let node = &tree[id];
    let rect = node.rect;
    let variant = node.variant();
    let (role, dim) = style::text_style(&variant);
    let color = if dim { ink.dim } else { ink.fg };
    let line = p.line_height(role);
    let lines = node.lines.clone();
    let align = match node.text("align").as_str() {
        "center" => Align::Center,
        "end" => Align::Right,
        _ => Align::Left,
    };
    let x = match align {
        Align::Center => rect.center_x(),
        Align::Right => rect.right(),
        _ => rect.x,
    };
    let clip = node.text("overflow") == "clip";
    let top = rect.y + (rect.h - line * lines.len() as f32).max(0.0) / 2.0;
    if clip {
        p.canvas.push_clip(rect);
    }
    let mut x0 = x;
    if variant == "title" && !cx.crumbs.is_empty() {
        // The screens this one was opened from, dim, before its own title.
        for crumb in &cx.crumbs {
            x0 += p.text(
                role,
                x0,
                top + line / 2.0,
                Align::Left,
                palette::TEXT_DIM,
                crumb,
            );
            x0 += p.u(18.0);
            p.chevron(
                x0 + p.u(6.0),
                top + line / 2.0,
                26.0,
                Chevron::Right,
                palette::TEXT_DIM,
            );
            x0 += p.u(30.0);
        }
    }
    for (i, text) in lines.iter().enumerate() {
        let shown = if clip || node_wraps(tree, id) {
            text.clone()
        } else {
            fit(p, role, text, rect.right() - x0)
        };
        p.text(
            role,
            x0,
            top + line * (i as f32 + 0.5),
            align,
            color,
            &shown,
        );
    }
    if clip {
        p.canvas.pop_clip();
    }
}

fn node_wraps(tree: &Tree, id: NodeId) -> bool {
    tree[id].text("wrap") != "none"
}

/// `text` cut with an ellipsis to fit `max_width` pixels.
pub fn fit(p: &mut Painter<'_>, role: Role, text: &str, max_width: f32) -> String {
    if max_width <= 0.0 {
        return String::new();
    }
    if p.measure(role, text) <= max_width + 0.5 {
        return text.to_string();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let candidate: String = chars.iter().collect::<String>() + "…";
        if p.measure(role, &candidate) <= max_width {
            return candidate;
        }
    }
    String::new()
}

fn tabs(tree: &mut Tree, id: NodeId, p: &mut Painter<'_>, cx: &PaintCx<'_>, ink: Ink) {
    let rect = tree[id].rect;
    let tab_ids = layout::tabs_of(tree, id);
    let labels: Vec<String> = tab_ids.iter().map(|&t| tree[t].text("label")).collect();
    let dim: Vec<bool> = tab_ids
        .iter()
        .map(|&t| tree[t].prop("dim").truthy())
        .collect();
    let selected = tree[id].index;
    let depth = tree[id].depth;
    let (strip_units, _, _) = style::tab_strip(depth);
    let strip_h = p.u(strip_units);
    let hidden = depth > 0 && tab_ids.len() <= 1 && tree[id].text("hide-single") != "false";
    let mut parts = Vec::new();
    if !hidden && !tab_ids.is_empty() {
        let cy = rect.y + strip_h / 2.0;
        let x = rect.x + p.u(35.0);
        let left = x + p.glyph_width(cx.device, GuiKey::TabPrev) + p.u(35.0);
        let right = rect.right() - p.u(90.0);
        let tab_w = ((right - left) / labels.len() as f32).min(p.u(260.0));
        let widest = labels
            .iter()
            .map(|label| p.measure(Role::Tab, label))
            .fold(0.0, f32::max);
        // Outer tabs share the width evenly; too many to fit slide instead.
        let fits = widest + p.u(30.0) <= tab_w;
        let strip = if depth == 0 && fits {
            p.tabs(x, cy, tab_w, cx.device, &labels, selected)
        } else {
            let keys = if depth == 0 {
                (GuiKey::TabPrev, GuiKey::TabNext)
            } else {
                (GuiKey::SubPrev, GuiKey::SubNext)
            };
            p.sub_tabs(
                x,
                cy,
                rect.right(),
                cx.device,
                keys,
                &labels,
                selected,
                &dim,
            )
        };
        let rule_w = if depth == 0 {
            rect.w
        } else {
            (strip.next.right() - rect.x + p.u(35.0)).min(rect.w)
        };
        p.rule(rect.x, rect.y + strip_h, rule_w, 2.0, palette::RULE_BRIGHT);
        parts.push((strip.prev, Part::TabPrev));
        parts.extend(
            strip
                .tabs
                .into_iter()
                .enumerate()
                .map(|(i, r)| (r, Part::Tab(i))),
        );
        parts.push((strip.next, Part::TabNext));
    }
    tree[id].parts = parts;
    if let Some(&tab) = tab_ids.get(selected) {
        paint(tree, tab, p, cx, ink);
    }
}

fn item(tree: &mut Tree, id: NodeId, p: &mut Painter<'_>, cx: &PaintCx<'_>) {
    let node = &tree[id];
    let rect = node.rect;
    let variant = node.variant();
    let selected = cx.focus == Some(id);
    let label = node.text("text");
    match variant.as_str() {
        "menu" => {
            let pad = style::item_padding("menu")[3];
            p.menu_item(rect, rect.x + p.u(pad), &label, selected, 1.0);
        }
        "dialog" => {
            if selected {
                p.selection(rect, SELECTION_GLOW);
            }
            let color = if selected {
                palette::TEXT
            } else {
                palette::TEXT_DIM
            };
            p.text(
                Role::Row,
                rect.center_x(),
                rect.center_y(),
                Align::Center,
                color,
                &label,
            );
        }
        "save" => save_frame(p, rect, selected, node.prop("highlight").truthy()),
        _ => {
            p.list_row(rect, selected);
            let cy = rect.center_y();
            let mut x = rect.x + p.u(30.0);
            let lead = node.text("lead");
            if !lead.is_empty() {
                p.text(Role::Row, x, cy, Align::Left, palette::TEXT_DIM, &lead);
                x += p.u(64.0);
            }
            let (left, right) = style::VALUE_COLUMN;
            let left = rect.x + rect.w * left;
            let right = rect.x + rect.w * right;
            let shown = fit(p, Role::Row, &label, left - x - p.u(30.0));
            p.text(Role::Row, x, cy, Align::Left, palette::TEXT, &shown);
            let note = node.text("note");
            if !note.is_empty() {
                p.text(
                    Role::Row,
                    (left + right) / 2.0,
                    cy,
                    Align::Center,
                    palette::TEXT_DIM,
                    &note,
                );
            }
            if node.prop("action").truthy() {
                p.link_icon(right, cy, 30.0, palette::TEXT);
            }
        }
    }
    let ink = style::item_ink(&variant, selected);
    children(tree, id, p, cx, ink);
}

/// A save-style row's frame: the pale band when selected, else a faint
/// panel; a red band and marker when highlighted.
fn save_frame(p: &mut Painter<'_>, rect: Rect, selected: bool, highlight: bool) {
    if selected {
        p.canvas.fill_rect(rect, style::SELECTED_BAND);
    } else {
        p.canvas.fill_gradient(
            rect,
            Axis::Horizontal,
            Color::rgba(0x24, 0x24, 0x24, 200),
            Color::rgba(0x18, 0x18, 0x18, 150),
        );
    }
    if highlight {
        let band = Rect::new(rect.x, rect.y, rect.w * 0.45, rect.h);
        p.canvas.fill_gradient(
            band,
            Axis::Horizontal,
            style::HIGHLIGHT.fade(if selected { 0.35 } else { 0.55 }),
            style::HIGHLIGHT.fade(0.0),
        );
    }
    let edge = p.u(4.0).max(1.0);
    let marker = if highlight {
        style::HIGHLIGHT
    } else if selected {
        style::SELECTED_TEXT
    } else {
        palette::RULE
    };
    p.canvas
        .fill_rect(Rect::new(rect.x, rect.y, edge, rect.h), marker);
}

/// The shown text of an option row's `value`.
pub fn value_label(node: &super::tree::Node, kind: OptionKind, value: i64) -> String {
    match kind {
        OptionKind::Toggle | OptionKind::Choice => node
            .prop("choices")
            .as_list()
            .get(value.max(0) as usize)
            .map(Value::to_string)
            .unwrap_or_default(),
        OptionKind::Slider => format!("{value}{}", node.text("suffix")),
        OptionKind::Link | OptionKind::Info => String::new(),
    }
}

fn option_row(
    tree: &mut Tree,
    id: NodeId,
    kind: OptionKind,
    p: &mut Painter<'_>,
    cx: &PaintCx<'_>,
) {
    let node = &tree[id];
    let rect = node.rect;
    let selected = cx.focus == Some(id);
    let disabled = node.prop("disabled").truthy();
    let ink = if disabled { Ink::DISABLED } else { Ink::NORMAL };
    let value = node.prop("value").as_int().unwrap_or(0);
    let cy = rect.center_y();
    p.list_row(rect, selected);
    let mut x = rect.x + p.u(30.0);
    let lead = node.text("lead");
    if !lead.is_empty() {
        p.text(Role::Row, x, cy, Align::Left, palette::TEXT_DIM, &lead);
        x += p.u(64.0);
    }
    let (left, right) = style::VALUE_COLUMN;
    let left_arrow = rect.x + rect.w * left;
    let right_arrow = rect.x + rect.w * right;
    let label = fit(
        p,
        Role::Row,
        &node.text("label"),
        left_arrow - x - p.u(30.0),
    );
    p.text(Role::Row, x, cy, Align::Left, ink.fg, &label);
    let mut parts = Vec::new();
    match kind {
        OptionKind::Toggle | OptionKind::Choice => {
            let count = node.prop("choices").as_list().len();
            let shown = value_label(node, kind, value);
            let shown = fit(p, Role::Row, &shown, right_arrow - left_arrow - p.u(60.0));
            let (decrease, increase) = p.stepper(
                rect,
                left_arrow,
                right_arrow,
                &shown,
                Some((count, value.max(0) as usize)),
                ink.fg,
                ink.fg,
            );
            parts.push((decrease, Part::Decrease));
            parts.push((increase, Part::Increase));
        }
        OptionKind::Slider => {
            let min = node.prop("min").as_int().unwrap_or(0);
            let max = node.prop("max").as_int().unwrap_or(100);
            let step = node.prop("step").as_int().unwrap_or(1).max(1);
            let label_w = p
                .measure(Role::Row, &value_label(node, kind, min))
                .max(p.measure(Role::Row, &value_label(node, kind, max)));
            let value_right = rect.right() - p.u(24.0);
            let right_chevron = value_right - label_w - p.u(24.0);
            let track_left = left_arrow + p.u(18.0);
            let track_right = right_chevron - p.u(18.0);
            p.chevron(left_arrow, cy, 22.0, Chevron::Left, ink.fg);
            p.chevron(right_chevron, cy, 22.0, Chevron::Right, ink.fg);
            let fraction = (value - min) as f32 / (max - min).max(1) as f32;
            let positions = (((max - min) / step) as usize + 1).min(21);
            p.slider(
                Rect::new(
                    track_left,
                    cy - p.u(20.0),
                    track_right - track_left,
                    p.u(40.0),
                ),
                fraction,
                positions,
                ink.fg,
            );
            let shown = value_label(node, kind, value);
            p.text(Role::Row, value_right, cy, Align::Right, ink.fg, &shown);
            parts.push((
                Rect::new(left_arrow - p.u(20.0), rect.y, p.u(40.0), rect.h),
                Part::Decrease,
            ));
            parts.push((
                Rect::new(right_chevron - p.u(20.0), rect.y, p.u(40.0), rect.h),
                Part::Increase,
            ));
            parts.push((
                Rect::new(
                    track_left - p.u(8.0),
                    rect.y,
                    track_right - track_left + p.u(16.0),
                    rect.h,
                ),
                Part::Track(track_left, track_right),
            ));
        }
        OptionKind::Link => {
            let note = node.text("note");
            if !note.is_empty() {
                p.text(
                    Role::Row,
                    (left_arrow + right_arrow) / 2.0,
                    cy,
                    Align::Center,
                    palette::TEXT_DIM,
                    &note,
                );
            }
            p.link_icon(right_arrow, cy, 30.0, ink.fg);
        }
        OptionKind::Info => {}
    }
    if let Some(default) = node.prop("default").as_int()
        && matches!(
            kind,
            OptionKind::Toggle | OptionKind::Choice | OptionKind::Slider
        )
        && value != default
    {
        p.diamond(rect.right() + p.u(16.0), cy, 7.0, Color::WHITE);
    }
    tree[id].parts = parts;
}

fn dialog(tree: &mut Tree, id: NodeId, p: &mut Painter<'_>, cx: &PaintCx<'_>) {
    let band = tree[id].rect;
    let fade = p.u(24.0);
    p.canvas.fill_gradient(
        Rect::new(band.x, band.y - fade, band.w, fade),
        Axis::Vertical,
        Color::rgba(8, 8, 8, 0),
        palette::DIALOG_BAND,
    );
    p.canvas.fill_rect(band, palette::DIALOG_BAND);
    p.canvas.fill_gradient(
        Rect::new(band.x, band.bottom(), band.w, fade),
        Axis::Vertical,
        palette::DIALOG_BAND,
        Color::rgba(8, 8, 8, 0),
    );
    for y in [band.y, band.bottom() - p.u(1.0)] {
        let line = Rect::new(band.w * 0.2, y, band.w * 0.6, p.u(1.0).max(1.0));
        let half = Rect::new(line.x, line.y, line.w / 2.0, line.h);
        p.canvas.fill_gradient(
            half,
            Axis::Horizontal,
            Color::rgba(0x80, 0x80, 0x80, 0),
            palette::RULE,
        );
        p.canvas.fill_gradient(
            Rect::new(half.right(), line.y, half.w, line.h),
            Axis::Horizontal,
            palette::RULE,
            Color::rgba(0x80, 0x80, 0x80, 0),
        );
    }
    let title = tree[id].text("title");
    let note = tree[id].text("note");
    let title_h = p.line_height(Role::DialogTitle);
    let mut y = band.y + p.u(35.0) + title_h / 2.0;
    p.text(
        Role::DialogTitle,
        band.center_x(),
        y,
        Align::Center,
        palette::TEXT,
        &title,
    );
    if !note.is_empty() {
        let note_h = p.line_height(Role::DialogBody);
        y += title_h / 2.0 + note_h / 2.0;
        p.text(
            Role::DialogBody,
            band.center_x(),
            y,
            Align::Center,
            palette::TEXT_DIM,
            &note,
        );
    }
    children(tree, id, p, cx, Ink::NORMAL);
}

/// A table of labelled values, each label over its value with a rule
/// beneath.
fn facts(tree: &mut Tree, id: NodeId, p: &mut Painter<'_>) {
    let rect = tree[id].rect;
    let items = tree[id].prop("items").clone();
    let line = p.line_height(Role::Body);
    let mut y = rect.y;
    p.rule(rect.x, y, rect.w, 2.0, palette::RULE);
    for item in items.as_list() {
        let label = item
            .field("label")
            .map(Value::to_string)
            .unwrap_or_default();
        let value = item
            .field("value")
            .map(Value::to_string)
            .unwrap_or_default();
        p.text(
            Role::Body,
            rect.x,
            y + line * 0.6,
            Align::Left,
            palette::TEXT_DIM,
            &label,
        );
        let value = fit(p, Role::Body, &value, rect.w);
        p.text(
            Role::Body,
            rect.right(),
            y + line * 1.6,
            Align::Right,
            palette::TEXT,
            &value,
        );
        y += line * 2.2;
        p.rule(rect.x, y, rect.w, 2.0, palette::RULE);
    }
}

/// Whether `id` and its ancestors are all laid out.
pub fn visible(tree: &Tree, id: NodeId) -> bool {
    tree.get(id).is_some_and(|n| n.shown) && present(tree, id)
}
