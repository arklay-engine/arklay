//! Layout: sizes every shown node and places it, in canvas pixels.
//!
//! Stacks place their children along their `orientation` with the theme's
//! gap; a child's `width`/`height` is `auto` (its measured size), `fill`
//! (a share of what is left), a number of units, or a percentage of the
//! parent. Anything the attributes leave out comes from [`super::style`].

use crate::gui::canvas::Rect;
use crate::gui::text::Fonts;
use crate::gui::theme::{Role, Theme};

use super::style;
use super::tree::{Kind, NodeId, Tree};
use super::value::Value;

/// What layout measures text with.
pub struct LayoutCx<'a> {
    pub fonts: &'a mut Fonts,
    pub theme: Theme,
    /// The focused node, kept in view by its list.
    pub focus: Option<NodeId>,
}

impl LayoutCx<'_> {
    fn u(&self, units: f32) -> f32 {
        self.theme.u(units)
    }

    fn line_height(&self, role: Role) -> f32 {
        self.fonts.metrics(self.theme.style(role)).line_height
    }

    fn measure_text(&mut self, role: Role, text: &str) -> f32 {
        self.fonts.measure(self.theme.style(role), text)
    }
}

/// A requested size.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Size {
    Auto,
    Fill,
    Units(f32),
    Percent(f32),
}

fn size(tree: &Tree, id: NodeId, key: &str) -> Size {
    let text = tree[id].text(key);
    let text = text.trim();
    if text.is_empty() || text == "auto" {
        Size::Auto
    } else if text == "fill" {
        Size::Fill
    } else if let Some(percent) = text.strip_suffix('%') {
        percent.trim().parse().map_or(Size::Auto, Size::Percent)
    } else {
        text.parse().map_or(Size::Auto, Size::Units)
    }
}

/// Whether a node takes part in layout: visible, and not something empty
/// that would only leave a gap.
pub fn present(tree: &Tree, id: NodeId) -> bool {
    let node = &tree[id];
    if node.attr("visible").is_some() && !node.prop("visible").truthy() {
        return false;
    }
    match node.kind {
        Kind::Text | Kind::Badge | Kind::Prompt => !node.text("text").is_empty(),
        Kind::Image => matches!(node.prop("src"), Value::Image(_)),
        Kind::Option(_) => node.attr("def").is_none() || node.prop("def") != &Value::Nil,
        Kind::Facts => node.prop("items").truthy(),
        // A stack with nothing to show leaves no gap.
        Kind::Stack => node.children.iter().any(|&c| present(tree, c)),
        _ => true,
    }
}

fn horizontal(tree: &Tree, id: NodeId) -> bool {
    tree[id].text("orientation") == "horizontal"
}

fn stack_gap(tree: &Tree, id: NodeId, cx: &LayoutCx<'_>) -> f32 {
    let node = &tree[id];
    let named = style::gap(&node.text("gap"));
    let units = match node.kind {
        Kind::Frame => style::FRAME_GAP,
        Kind::List => named.unwrap_or_else(|| style::list_gap(&list_variant(tree, id))),
        _ => named.unwrap_or_else(|| style::default_gap(horizontal(tree, id))),
    };
    cx.u(units)
}

/// A list's row variant: the variant of its first item, or its own.
fn list_variant(tree: &Tree, id: NodeId) -> String {
    let own = tree[id].variant();
    if !own.is_empty() {
        return own;
    }
    tree.descendants(id)
        .into_iter()
        .find(|&d| tree[d].kind == Kind::Item)
        .map(|d| tree[d].variant())
        .unwrap_or_default()
}

fn padding(tree: &Tree, id: NodeId, cx: &LayoutCx<'_>) -> [f32; 4] {
    let node = &tree[id];
    let units = match node.kind {
        Kind::Frame if node.variant() == "modal" => [0.0; 4],
        Kind::Frame => style::FRAME_PADDING,
        Kind::Item => style::item_padding(&node.variant()),
        _ => [0.0; 4],
    };
    units.map(|v| cx.u(v))
}

fn inset(rect: Rect, [top, right, bottom, left]: [f32; 4]) -> Rect {
    Rect::new(
        rect.x + left,
        rect.y + top,
        (rect.w - left - right).max(0.0),
        (rect.h - top - bottom).max(0.0),
    )
}

/// The children that take part in the flow (not anchored over it).
fn flow(tree: &Tree, id: NodeId) -> Vec<NodeId> {
    tree[id]
        .children
        .iter()
        .copied()
        .filter(|&c| present(tree, c) && tree[c].text("anchor").is_empty())
        .collect()
}

/// The tab strip's height in pixels (zero when hidden).
pub fn strip_height(tree: &Tree, id: NodeId, cx: &LayoutCx<'_>) -> (f32, f32) {
    let node = &tree[id];
    let (h, below, hide_single) = style::tab_strip(node.depth);
    let hide_single = match node.text("hide-single").as_str() {
        "true" => true,
        "false" => false,
        _ => hide_single,
    };
    let tabs = node
        .children
        .iter()
        .filter(|&&c| tree[c].kind == Kind::Tab && present(tree, c))
        .count();
    if hide_single && tabs <= 1 {
        (0.0, 0.0)
    } else {
        (cx.u(h), cx.u(below))
    }
}

/// The shown tabs of a strip.
pub fn tabs_of(tree: &Tree, id: NodeId) -> Vec<NodeId> {
    tree[id]
        .children
        .iter()
        .copied()
        .filter(|&c| tree[c].kind == Kind::Tab && present(tree, c))
        .collect()
}

/// The size `id` wants given `avail` pixels of width.
pub fn measure(tree: &mut Tree, id: NodeId, cx: &mut LayoutCx<'_>, avail: f32) -> (f32, f32) {
    let kind = tree[id].kind;
    let variant = tree[id].variant();
    let fixed = |cx: &LayoutCx<'_>, tree: &Tree, axis: &str, auto: f32| match size(tree, id, axis) {
        Size::Units(v) => cx.u(v),
        Size::Percent(p) => avail * p / 100.0,
        _ => auto,
    };
    let (w, h) = match kind {
        Kind::Text => {
            let (role, _) = style::text_style(&variant);
            let text = tree[id].text("text");
            let line = cx.line_height(role);
            if tree[id].text("wrap") == "none" {
                (cx.measure_text(role, &text).min(avail), line)
            } else {
                let style = cx.theme.style(role);
                let lines = cx.fonts.wrap(style, &text, avail.max(1.0));
                let width = lines
                    .iter()
                    .map(|l| cx.fonts.measure(style, l))
                    .fold(0.0, f32::max);
                (width, line * lines.len().max(1) as f32)
            }
        }
        Kind::Badge => {
            let text = tree[id].text("text");
            (cx.measure_text(Role::Badge, &text) + cx.u(20.0), cx.u(30.0))
        }
        Kind::Rule => (avail, cx.u(2.0).max(1.0)),
        Kind::Separator => (cx.u(1.0).max(1.0), cx.u(1.0).max(1.0)),
        Kind::Image => {
            let (w, h) = style::image_size(&variant);
            (cx.u(w), cx.u(h))
        }
        Kind::Item | Kind::Option(_) => {
            let h = style::row_height(kind, &variant).unwrap_or(66.0);
            (avail, cx.u(h))
        }
        Kind::HintBar => (avail, cx.u(style::HINT_BAR_H)),
        Kind::Prompt => (avail, cx.u(style::PROMPT_H)),
        Kind::Facts => {
            let rows = tree[id].prop("items").as_list().len();
            let line = cx.line_height(Role::Body);
            (avail, line * 2.2 * rows as f32 + cx.u(2.0))
        }
        Kind::Group => {
            let mut h = cx.u(style::GROUP_H);
            for c in flow(tree, id) {
                h += measure(tree, c, cx, avail).1;
            }
            (avail, h + group_gap(tree, id, cx))
        }
        Kind::Tabs => {
            let (strip, below) = strip_height(tree, id, cx);
            let index = tree[id].index;
            let content = tabs_of(tree, id)
                .get(index)
                .map_or(0.0, |&t| measure(tree, t, cx, avail).1);
            (avail, strip + below + content)
        }
        Kind::Split => (avail, 0.0),
        Kind::Spacer | Kind::Custom | Kind::Dialog => (0.0, 0.0),
        Kind::Frame | Kind::Stack | Kind::Tab | Kind::List => {
            let pad = padding(tree, id, cx);
            let inner = (avail - pad[1] - pad[3]).max(0.0);
            let gap = stack_gap(tree, id, cx);
            let children = flow(tree, id);
            let gaps = gap * children.len().saturating_sub(1) as f32;
            let (w, h) = if horizontal(tree, id) {
                let mut w = gaps;
                let mut h: f32 = 0.0;
                for c in children {
                    let (cw, ch) = measure(tree, c, cx, inner);
                    let cw = match size(tree, c, "width") {
                        Size::Units(v) => cx.u(v),
                        Size::Percent(p) => inner * p / 100.0,
                        _ => cw,
                    };
                    w += cw;
                    h = h.max(ch);
                }
                (w, h)
            } else {
                let mut w: f32 = 0.0;
                let mut h = gaps;
                for c in children {
                    let cw = match size(tree, c, "width") {
                        Size::Units(v) => cx.u(v).min(inner),
                        Size::Percent(p) => inner * p / 100.0,
                        _ => inner,
                    };
                    let (mw, mh) = measure(tree, c, cx, cw);
                    w = w.max(mw);
                    h += match size(tree, c, "height") {
                        Size::Units(v) => cx.u(v),
                        _ => mh,
                    };
                }
                (w, h)
            };
            (w + pad[1] + pad[3], h + pad[0] + pad[2])
        }
    };
    (fixed(cx, tree, "width", w), fixed(cx, tree, "height", h))
}

/// The space above a group header that follows other rows.
fn group_gap(tree: &Tree, id: NodeId, cx: &LayoutCx<'_>) -> f32 {
    let Some(parent) = tree[id].parent else {
        return 0.0;
    };
    let first = tree[parent]
        .children
        .iter()
        .copied()
        .find(|&c| present(tree, c));
    if first == Some(id) {
        0.0
    } else {
        cx.u(style::GROUP_GAP)
    }
}

/// Lay out `root` (and everything under it) inside `rect`.
pub fn layout(tree: &mut Tree, root: NodeId, cx: &mut LayoutCx<'_>, rect: Rect) {
    for id in tree.descendants(root) {
        tree[id].shown = false;
    }
    arrange(tree, root, cx, rect);
}

fn arrange(tree: &mut Tree, id: NodeId, cx: &mut LayoutCx<'_>, rect: Rect) {
    {
        let node = &mut tree[id];
        node.rect = rect;
        node.shown = true;
    }
    match tree[id].kind {
        Kind::Text => {
            let (role, _) = style::text_style(&tree[id].variant());
            let text = tree[id].text("text");
            let lines = if tree[id].text("wrap") == "none" {
                vec![text]
            } else {
                let style = cx.theme.style(role);
                cx.fonts.wrap(style, &text, rect.w.max(1.0))
            };
            tree[id].lines = lines;
        }
        Kind::Frame | Kind::Stack | Kind::Tab => arrange_stack(tree, id, cx, rect),
        Kind::Item => {
            let inner = inset(rect, padding(tree, id, cx));
            arrange_stack(tree, id, cx, rect);
            anchor(tree, id, cx, inner);
        }
        Kind::Group => {
            let top = rect.y + group_gap(tree, id, cx) + cx.u(style::GROUP_H);
            let mut y = top;
            for c in flow(tree, id) {
                let h = measure(tree, c, cx, rect.w).1;
                arrange(tree, c, cx, Rect::new(rect.x, y, rect.w, h));
                y += h;
            }
        }
        Kind::Split => {
            let children = flow(tree, id);
            let (ratio, gap) = style::SPLIT;
            let gap = cx.u(gap);
            let first_w = ((rect.w - gap) * ratio).max(0.0);
            if let Some(&first) = children.first() {
                arrange(tree, first, cx, Rect::new(rect.x, rect.y, first_w, rect.h));
            }
            if let Some(&second) = children.get(1) {
                let x = rect.x + first_w + gap;
                arrange(
                    tree,
                    second,
                    cx,
                    Rect::new(x, rect.y, rect.right() - x, rect.h),
                );
            }
        }
        Kind::Tabs => {
            let (strip, below) = strip_height(tree, id, cx);
            let tabs = tabs_of(tree, id);
            let index = tree[id].index.min(tabs.len().saturating_sub(1));
            tree[id].index = index;
            if let Some(&tab) = tabs.get(index) {
                let top = rect.y + strip + below;
                arrange(
                    tree,
                    tab,
                    cx,
                    Rect::new(rect.x, top, rect.w, (rect.bottom() - top).max(0.0)),
                );
            }
        }
        Kind::List => arrange_list(tree, id, cx, rect),
        Kind::Dialog => arrange_dialog(tree, id, cx, rect),
        _ => {}
    }
}

fn arrange_stack(tree: &mut Tree, id: NodeId, cx: &mut LayoutCx<'_>, rect: Rect) {
    let inner = inset(rect, padding(tree, id, cx));
    let gap = stack_gap(tree, id, cx);
    let children = flow(tree, id);
    if children.is_empty() {
        return;
    }
    let gaps = gap * (children.len() - 1) as f32;
    let horizontal = horizontal(tree, id);
    let (main, axis, cross_axis) = if horizontal {
        (inner.w, "width", "height")
    } else {
        (inner.h, "height", "width")
    };
    // Sizes along the main axis; `None` fills.
    let mut sizes: Vec<Option<f32>> = Vec::with_capacity(children.len());
    for &c in &children {
        let s = match size(tree, c, axis) {
            Size::Fill => None,
            Size::Units(v) => Some(cx.u(v)),
            Size::Percent(p) => Some(main * p / 100.0),
            Size::Auto if tree[c].kind == Kind::Spacer => None,
            // Panes that scroll or split take what the column leaves.
            Size::Auto
                if !horizontal
                    && tree[c].attr("height").is_none()
                    && matches!(
                        tree[c].kind,
                        Kind::Custom | Kind::List | Kind::Split | Kind::Tabs | Kind::Dialog
                    ) =>
            {
                None
            }
            Size::Auto => {
                let (w, h) = measure(tree, c, cx, inner.w);
                Some(if horizontal { w } else { h })
            }
        };
        sizes.push(s);
    }
    let used: f32 = sizes.iter().flatten().sum::<f32>() + gaps;
    let fills = sizes.iter().filter(|s| s.is_none()).count();
    let share = if fills > 0 {
        ((main - used) / fills as f32).max(0.0)
    } else {
        0.0
    };
    let mut at = if horizontal { inner.x } else { inner.y };
    for (&c, s) in children.iter().zip(sizes) {
        let along = s.unwrap_or(share);
        let cross_size = match size(tree, c, cross_axis) {
            Size::Units(v) => Some(cx.u(v)),
            Size::Percent(p) => Some(if horizontal { inner.h } else { inner.w } * p / 100.0),
            _ => None,
        };
        let child = if horizontal {
            // Centred across a row unless it fills it.
            let h = match (cross_size, tree[c].kind) {
                (Some(h), _) => h,
                (
                    None,
                    Kind::Separator | Kind::Stack | Kind::Custom | Kind::List | Kind::Spacer,
                ) => inner.h,
                (None, _) => measure(tree, c, cx, along).1.min(inner.h),
            };
            let h = if tree[c].kind == Kind::Separator && cross_size.is_none() {
                // A divider stops short of a row's edges.
                (inner.h - cx.u(16.0)).max(0.0)
            } else {
                h
            };
            Rect::new(at, inner.y + (inner.h - h) / 2.0, along, h)
        } else {
            let w = cross_size.unwrap_or(inner.w).min(inner.w);
            let x = match tree[c].text("align").as_str() {
                "center" => inner.x + (inner.w - w) / 2.0,
                "end" => inner.right() - w,
                _ => inner.x,
            };
            Rect::new(x, at, w, along)
        };
        arrange(tree, c, cx, child);
        at += along + gap;
    }
}

/// Place the anchored children of `id` over `inner`.
fn anchor(tree: &mut Tree, id: NodeId, cx: &mut LayoutCx<'_>, inner: Rect) {
    let anchored: Vec<NodeId> = tree[id]
        .children
        .iter()
        .copied()
        .filter(|&c| present(tree, c) && !tree[c].text("anchor").is_empty())
        .collect();
    for c in anchored {
        let (w, h) = measure(tree, c, cx, inner.w);
        let anchor = tree[c].text("anchor");
        let x = if anchor.ends_with("end") {
            inner.right() - w
        } else {
            inner.x
        };
        let y = if anchor.starts_with("bottom") {
            inner.bottom() - h
        } else {
            inner.y
        };
        arrange(tree, c, cx, Rect::new(x, y, w, h));
    }
}

fn arrange_list(tree: &mut Tree, id: NodeId, cx: &mut LayoutCx<'_>, rect: Rect) {
    let children = flow(tree, id);
    let gap = stack_gap(tree, id, cx);
    let mut heights = Vec::with_capacity(children.len());
    let mut total = 0.0;
    for (i, &c) in children.iter().enumerate() {
        let h = measure(tree, c, cx, rect.w).1;
        heights.push(h);
        total += h + if i > 0 { gap } else { 0.0 };
    }
    // Keep room for the scrollbar when it shows.
    let width = if total > rect.h + 0.5 {
        rect.w - cx.u(style::SCROLLBAR_ROOM)
    } else {
        rect.w
    };
    if (width - rect.w).abs() > f32::EPSILON {
        heights.clear();
        total = 0.0;
        for (i, &c) in children.iter().enumerate() {
            let h = measure(tree, c, cx, width).1;
            heights.push(h);
            total += h + if i > 0 { gap } else { 0.0 };
        }
    }
    tree[id].content = total;
    // Lay out at offset zero first to find the focused row.
    let lay = |tree: &mut Tree, scroll: f32, cx: &mut LayoutCx<'_>| {
        let mut y = rect.y - scroll;
        for (&c, &h) in children.iter().zip(&heights) {
            arrange(tree, c, cx, Rect::new(rect.x, y, width, h));
            y += h + gap;
        }
    };
    let scroll = tree[id].scroll.clamp(0.0, (total - rect.h).max(0.0));
    lay(tree, scroll, cx);
    let mut want = scroll;
    if let Some(focus) = cx.focus
        && tree.contains(focus)
        && tree.ancestry(focus).contains(&id)
    {
        let row = tree[focus].rect;
        // A group's first row brings its header along.
        let top = tree[focus]
            .parent
            .filter(|&p| tree[p].kind == Kind::Group)
            .filter(|&p| flow(tree, p).first() == Some(&focus))
            .map_or(row.y, |p| tree[p].rect.y + group_gap(tree, p, cx));
        if top < rect.y {
            want -= rect.y - top;
        } else if row.bottom() > rect.bottom() {
            want += row.bottom() - rect.bottom();
        }
    }
    let want = want.clamp(0.0, (total - rect.h).max(0.0));
    if (want - scroll).abs() > 0.01 {
        lay(tree, want, cx);
    }
    tree[id].scroll = want;
}

fn arrange_dialog(tree: &mut Tree, id: NodeId, cx: &mut LayoutCx<'_>, rect: Rect) {
    let title_h = cx.line_height(Role::DialogTitle);
    let note_h = if tree[id].text("note").is_empty() {
        0.0
    } else {
        cx.line_height(Role::DialogBody)
    };
    let choices = flow(tree, id);
    let choice_h = cx.u(60.0);
    let gap = cx.u(36.0);
    let band_h = cx.u(70.0) + title_h + note_h + gap + choice_h * choices.len() as f32;
    let band = Rect::new(rect.x, rect.y + (rect.h - band_h) / 2.0, rect.w, band_h);
    tree[id].rect = band;
    let width = cx.u(420.0);
    let mut y = band.y + cx.u(35.0) + title_h + note_h + gap;
    for c in choices {
        arrange(
            tree,
            c,
            cx,
            Rect::new(
                rect.center_x() - width / 2.0,
                y,
                width,
                choice_h - cx.u(8.0),
            ),
        );
        y += choice_h;
    }
}
