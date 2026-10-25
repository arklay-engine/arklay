//! The options screen: category tabs, subcategory tabs, grouped option rows
//! and a description pane, laid out across the whole window.
//!
//! The screen is generic over its [`OptionDef`] list; it never knows what an
//! option means. A changed value is reported back as `(index, value)` and
//! the owner applies it.

use super::canvas::{Color, Rect};
use super::dialog::{Dialog, DialogResult};
use super::input::{Device, GuiEvent, GuiKey};
use super::locale::Locale;
use super::options::{Category, OptionDef, OptionKind, tree};
use super::text::Align;
use super::theme::{Role, palette};
use super::widgets::{Chevron, Painter, SELECTION_GLOW};

/// Row heights, in units.
const ROW_H: f32 = 66.0;
const GROUP_H: f32 = 40.0;
const GROUP_GAP: f32 = 24.0;

/// What the options screen asks of its owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OptionsEvent {
    /// Option `index` now holds `value`.
    Changed { index: usize, value: i32 },
    /// Leave the screen.
    Back,
}

/// What a pointer press landed on.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Hit {
    Tab(usize),
    TabPrev,
    TabNext,
    Sub(usize),
    SubPrev,
    SubNext,
    /// A row (the visible list index).
    Row(usize),
    Decrease(usize),
    Increase(usize),
    /// A slider track: the row and the track's x span.
    Track(usize, f32, f32),
    ResetAll,
    ResetSub,
    Back,
}

impl Hit {
    /// The list row a hit belongs to, for any part of the row.
    fn row(self) -> Option<usize> {
        match self {
            Hit::Row(row) | Hit::Decrease(row) | Hit::Increase(row) | Hit::Track(row, ..) => {
                Some(row)
            }
            _ => None,
        }
    }
}

/// A pending confirmation.
enum Confirm {
    ResetAll,
    ResetSub,
}

/// One line of the list: a group header or an option row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Entry {
    Group(usize),
    Option(usize),
}

pub struct OptionsScreen {
    defs: Vec<OptionDef>,
    values: Vec<i32>,
    tree: Vec<Category>,
    category: usize,
    sub: usize,
    /// The selected row within the subcategory's option list.
    row: usize,
    /// The list's scroll offset in units.
    scroll: f32,
    hits: Vec<(Rect, Hit)>,
    /// The slider the pointer is dragging: its row and track span, until the
    /// button comes up.
    drag: Option<(usize, f32, f32)>,
    dialog: Option<(Dialog, Confirm)>,
    pub(super) device: Device,
    text: Locale,
}

impl OptionsScreen {
    pub fn new(defs: Vec<OptionDef>, values: Vec<i32>, text: Locale) -> Self {
        let tree = tree(&defs);
        Self {
            defs,
            values,
            tree,
            category: 0,
            sub: 0,
            row: 0,
            scroll: 0.0,
            hits: Vec::new(),
            drag: None,
            dialog: None,
            device: Device::Keyboard,
            text,
        }
    }

    /// Draw in another language from now on (the definitions arrive through
    /// [`Self::refresh`]).
    pub fn set_locale(&mut self, text: Locale) {
        self.text = text;
    }

    /// Show hints for `device` until the next input says otherwise.
    pub fn set_device(&mut self, device: Device) {
        self.device = device;
    }

    /// Replace the definitions (a change can enable or relabel others),
    /// keeping the selection.
    pub fn refresh(&mut self, defs: Vec<OptionDef>, values: Vec<i32>) {
        self.tree = tree(&defs);
        self.defs = defs;
        self.values = values;
        self.category = self.category.min(self.tree.len().saturating_sub(1));
        self.sub = self.sub.min(self.subs_len().saturating_sub(1));
        self.row = self.row.min(self.options().len().saturating_sub(1));
    }

    fn subs_len(&self) -> usize {
        self.tree
            .get(self.category)
            .map_or(0, |c| c.subcategories.len())
    }

    /// The option indices of the shown subcategory.
    fn options(&self) -> &[usize] {
        self.tree
            .get(self.category)
            .and_then(|c| c.subcategories.get(self.sub))
            .map_or(&[], |s| &s.options)
    }

    /// The shown list with its group headers.
    fn entries(&self) -> Vec<Entry> {
        let mut entries = Vec::new();
        let mut group: Option<&str> = None;
        for &index in self.options() {
            let name = self.defs[index].group.as_str();
            if !name.is_empty() && group != Some(name) {
                entries.push(Entry::Group(index));
            }
            group = Some(name);
            entries.push(Entry::Option(index));
        }
        entries
    }

    fn set_category(&mut self, category: usize) {
        self.drag = None;
        // The old page's rectangles are gone until the next draw.
        self.hits.clear();
        if category != self.category {
            self.category = category;
            self.sub = 0;
            self.row = 0;
            self.scroll = 0.0;
        }
    }

    fn set_sub(&mut self, sub: usize) {
        self.drag = None;
        // The old page's rectangles are gone until the next draw.
        self.hits.clear();
        if sub != self.sub {
            self.sub = sub;
            self.row = 0;
            self.scroll = 0.0;
        }
    }

    fn change(&mut self, index: usize, value: i32, events: &mut Vec<OptionsEvent>) {
        let def = &self.defs[index];
        if def.disabled {
            return;
        }
        let value = def.clamp(value);
        if self.values[index] != value {
            self.values[index] = value;
            events.push(OptionsEvent::Changed { index, value });
        }
    }

    fn step(&mut self, delta: i32, wrap: bool, events: &mut Vec<OptionsEvent>) {
        let Some(&index) = self.options().get(self.row) else {
            return;
        };
        let def = &self.defs[index];
        let current = self.values[index];
        let mut next = def.step(current, delta);
        if wrap
            && next == current
            && let OptionKind::Choice(choices) = &def.kind
        {
            next = if delta > 0 {
                0
            } else {
                choices.len() as i32 - 1
            };
        }
        self.change(index, next, events);
    }

    fn reset(&mut self, indices: Vec<usize>, events: &mut Vec<OptionsEvent>) {
        for index in indices {
            let default = self.defs[index].default;
            self.change(index, default, events);
        }
    }

    /// Feed one event; returns what the owner must do.
    pub fn handle(&mut self, event: GuiEvent) -> Vec<OptionsEvent> {
        let mut events = Vec::new();
        if let GuiEvent::Key(_, device) = event {
            self.device = device;
        }
        if let Some((dialog, confirm)) = &mut self.dialog {
            match dialog.handle(event) {
                DialogResult::Open => {}
                DialogResult::Cancelled | DialogResult::Chosen(1) => self.dialog = None,
                DialogResult::Chosen(_) => {
                    let indices = match confirm {
                        Confirm::ResetAll => (0..self.defs.len()).collect(),
                        Confirm::ResetSub => self.options().to_vec(),
                    };
                    self.dialog = None;
                    self.reset(indices, &mut events);
                }
            }
            return events;
        }
        let rows = self.options().len();
        let categories = self.tree.len().max(1);
        let subs = self.subs_len().max(1);
        match event {
            GuiEvent::Key(key, _) => match key {
                GuiKey::Up if rows > 0 => self.row = (self.row + rows - 1) % rows,
                GuiKey::Down if rows > 0 => self.row = (self.row + 1) % rows,
                GuiKey::Left => self.step(-1, false, &mut events),
                GuiKey::Right => self.step(1, false, &mut events),
                GuiKey::Confirm => self.step(1, true, &mut events),
                GuiKey::Cancel => events.push(OptionsEvent::Back),
                GuiKey::TabPrev => self.set_category((self.category + categories - 1) % categories),
                GuiKey::TabNext => self.set_category((self.category + 1) % categories),
                GuiKey::SubPrev => self.set_sub((self.sub + subs - 1) % subs),
                GuiKey::SubNext => self.set_sub((self.sub + 1) % subs),
                GuiKey::ResetAll => self.confirm(Confirm::ResetAll),
                GuiKey::ResetGroup => self.confirm(Confirm::ResetSub),
                _ => {}
            },
            GuiEvent::PointerMove { x, y } => {
                if let Some((row, left, right)) = self.drag {
                    self.slide(row, x, left, right, &mut events);
                } else if let Some(row) = self.hit(x, y).and_then(Hit::row) {
                    self.row = row;
                }
            }
            GuiEvent::PointerUp { .. } => self.drag = None,
            GuiEvent::PointerDown { x, y } => match self.hit(x, y) {
                Some(Hit::Tab(index)) => self.set_category(index),
                Some(Hit::TabPrev) => self.handle_key(GuiKey::TabPrev, &mut events),
                Some(Hit::TabNext) => self.handle_key(GuiKey::TabNext, &mut events),
                Some(Hit::Sub(index)) => self.set_sub(index),
                Some(Hit::SubPrev) => self.handle_key(GuiKey::SubPrev, &mut events),
                Some(Hit::SubNext) => self.handle_key(GuiKey::SubNext, &mut events),
                Some(Hit::Row(row)) => {
                    self.row = row;
                    self.step(1, true, &mut events);
                }
                Some(Hit::Decrease(row)) => {
                    self.row = row;
                    self.step(-1, false, &mut events);
                }
                Some(Hit::Increase(row)) => {
                    self.row = row;
                    self.step(1, false, &mut events);
                }
                Some(Hit::Track(row, left, right)) => {
                    self.drag = Some((row, left, right));
                    self.slide(row, x, left, right, &mut events);
                }
                Some(Hit::ResetAll) => self.confirm(Confirm::ResetAll),
                Some(Hit::ResetSub) => self.confirm(Confirm::ResetSub),
                Some(Hit::Back) => events.push(OptionsEvent::Back),
                None => {}
            },
            GuiEvent::Wheel(notches) if rows > 0 => {
                // The wheel walks the selection; the list follows it.
                let step = if notches > 0.0 { rows - 1 } else { 1 };
                self.row = (self.row + step) % rows;
            }
            GuiEvent::Wheel(_) => {}
        }
        events
    }

    /// Where the cursor is: the tab, the subcategory, the row and the open
    /// dialog's choice, for the owner's feedback sounds.
    pub(super) fn cursor(&self) -> (usize, usize, usize, Option<usize>) {
        (
            self.category,
            self.sub,
            self.row,
            self.dialog.as_ref().map(|(dialog, _)| dialog.selected),
        )
    }

    /// Set row `row`'s slider to the step nearest `x` on its track.
    fn slide(&mut self, row: usize, x: f32, left: f32, right: f32, events: &mut Vec<OptionsEvent>) {
        let Some(&index) = self.options().get(row) else {
            return;
        };
        self.row = row;
        if let OptionKind::Slider { min, max, step, .. } = self.defs[index].kind {
            let fraction = ((x - left) / (right - left).max(1.0)).clamp(0.0, 1.0);
            let steps = ((max - min) as f32 / step.max(1) as f32 * fraction).round();
            self.change(index, min + steps as i32 * step, events);
        }
    }

    fn handle_key(&mut self, key: GuiKey, events: &mut Vec<OptionsEvent>) {
        events.extend(self.handle(GuiEvent::Key(key, self.device)));
    }

    fn confirm(&mut self, confirm: Confirm) {
        self.drag = None;
        let title = self.text.get(match confirm {
            Confirm::ResetAll => "options.reset_all_question",
            Confirm::ResetSub => "options.reset_page_question",
        });
        self.dialog = Some((Dialog::yes_no(&self.text, title, None), confirm));
    }

    fn hit(&self, x: f32, y: f32) -> Option<Hit> {
        // Later rectangles sit on top (an arrow inside its row).
        self.hits
            .iter()
            .rev()
            .find(|(rect, _)| rect.contains(x, y))
            .map(|&(_, hit)| hit)
    }

    /// Draw the screen over a canvas cleared to [`palette::SCREEN_SHADE`].
    pub fn draw(&mut self, p: &mut Painter<'_>) {
        let pulse = SELECTION_GLOW;
        self.hits.clear();
        let w = p.theme.width;
        let scale = p.theme.scale;
        let u = |v: f32| v * scale;

        let left = w * 0.056;
        let right = w * 0.944;
        let pane_right = w * 0.524;
        let desc_left = w * 0.58;

        // Title.
        p.text(
            Role::Title,
            left,
            u(97.0),
            Align::Left,
            palette::TEXT,
            self.text.get("options.title"),
        );

        // Category tabs between the Q and E key caps.
        let tab_y = u(153.0);
        let cap = p.glyph(left + u(35.0), tab_y, self.device, GuiKey::TabPrev);
        self.hits.push((cap, Hit::TabPrev));
        let tabs_left = cap.right() + u(40.0);
        let tabs_right = right - u(90.0);
        let count = self.tree.len().max(1);
        let tab_w = ((tabs_right - tabs_left) / count as f32).min(u(260.0));
        for (index, category) in self.tree.iter().enumerate() {
            let x = tabs_left + tab_w * index as f32;
            let rect = Rect::new(x, tab_y - u(26.0), tab_w, u(52.0));
            let selected = index == self.category;
            let color = if selected {
                palette::TEXT
            } else {
                palette::TEXT_DIM
            };
            p.vrule(x, tab_y - u(20.0), u(40.0), 2.0, palette::RULE_BRIGHT);
            p.text(
                Role::Tab,
                rect.center_x(),
                tab_y,
                Align::Center,
                color,
                &category.name,
            );
            if selected {
                let text_w = p.measure(Role::Tab, &category.name);
                p.underline_glow(rect.center_x(), u(181.0), text_w + u(40.0));
            }
            self.hits.push((rect, Hit::Tab(index)));
        }
        let last = tabs_left + tab_w * count as f32;
        p.vrule(last, tab_y - u(20.0), u(40.0), 2.0, palette::RULE_BRIGHT);
        let cap = p.glyph(last + u(35.0), tab_y, self.device, GuiKey::TabNext);
        self.hits.push((cap, Hit::TabNext));
        p.rule(left, u(181.0), right - left, 2.0, palette::RULE_BRIGHT);

        // Subcategory tabs.
        let sub_y = u(260.0);
        let cap = p.glyph(left + u(35.0), sub_y, self.device, GuiKey::SubPrev);
        self.hits.push((cap, Hit::SubPrev));
        let mut x = cap.right() + u(30.0);
        let subs: Vec<String> = self
            .tree
            .get(self.category)
            .map(|c| c.subcategories.iter().map(|s| s.name.clone()).collect())
            .unwrap_or_default();
        for (index, name) in subs.iter().enumerate() {
            let text_w = p.measure(Role::SubTab, name);
            let rect = Rect::new(x, sub_y - u(24.0), text_w + u(60.0), u(48.0));
            let selected = index == self.sub;
            let color = if selected {
                palette::TEXT
            } else {
                palette::TEXT_DIM
            };
            p.text(
                Role::SubTab,
                rect.center_x(),
                sub_y,
                Align::Center,
                color,
                name,
            );
            if selected {
                p.underline_glow(rect.center_x(), u(285.0), text_w + u(20.0));
            }
            self.hits.push((rect, Hit::Sub(index)));
            x = rect.right() + u(20.0);
        }
        let cap = p.glyph(x, sub_y, self.device, GuiKey::SubNext);
        self.hits.push((cap, Hit::SubNext));
        p.rule(left, u(285.0), pane_right - left, 2.0, palette::RULE_BRIGHT);

        // The list.
        let list = Rect::new(left, u(329.0), pane_right - left, u(960.0) - u(329.0));
        let entries = self.entries();
        let mut layout = Vec::with_capacity(entries.len());
        let mut y = 0.0;
        for (position, entry) in entries.iter().enumerate() {
            match entry {
                Entry::Group(_) => {
                    if position > 0 {
                        y += GROUP_GAP;
                    }
                    layout.push((y, GROUP_H));
                    y += GROUP_H;
                }
                Entry::Option(_) => {
                    layout.push((y, ROW_H));
                    y += ROW_H;
                }
            }
        }
        let total = y;
        let visible = list.h / p.theme.scale;
        // Keep the selected row (and its group header) in view.
        if let Some(position) = entries
            .iter()
            .position(|e| matches!(e, Entry::Option(i) if self.options().get(self.row) == Some(i)))
        {
            let (top, height) = layout[position];
            let top = match position.checked_sub(1).map(|p| entries[p]) {
                Some(Entry::Group(_)) => layout[position - 1].0,
                _ => top,
            };
            if top < self.scroll {
                self.scroll = top;
            } else if layout[position].0 + height > self.scroll + visible {
                self.scroll = layout[position].0 + height - visible;
            }
        }
        self.scroll = self.scroll.clamp(0.0, (total - visible).max(0.0));

        p.canvas.push_clip(list);
        let mut row_number = 0;
        for (position, entry) in entries.iter().enumerate() {
            let (top, height) = layout[position];
            let rect = Rect::new(list.x, list.y + u(top - self.scroll), list.w, u(height));
            match *entry {
                Entry::Group(index) => {
                    let name = self.defs[index].group.clone();
                    p.group_header(rect, &name);
                }
                Entry::Option(index) => {
                    let selected = row_number == self.row;
                    if rect.bottom() > list.y && rect.y < list.bottom() {
                        self.draw_row(p, rect, index, row_number, selected, pulse);
                    }
                    row_number += 1;
                }
            }
        }
        p.canvas.pop_clip();

        // The scrollbar between the list and the description.
        let bar = Rect::new(w * 0.543 - u(4.0), list.y, u(8.0), list.h);
        p.scrollbar(bar, self.scroll, visible, total);

        // The description pane.
        if let Some(&index) = self.options().get(self.row) {
            let def = self.defs[index].clone();
            let width = right - desc_left;
            let mut y = p.paragraph(
                Role::Body,
                desc_left,
                list.y,
                width,
                palette::TEXT,
                &def.description,
            );
            if let OptionKind::Choice(choices) = &def.kind {
                let described: Vec<_> = choices
                    .iter()
                    .filter_map(|c| c.description.as_ref().map(|d| format!("{}: {d}", c.label)))
                    .collect();
                if !described.is_empty() {
                    y += p.line_height(Role::Body) * 0.6;
                    for line in described {
                        y = p.paragraph(Role::Body, desc_left, y, width, palette::TEXT, &line);
                    }
                }
            }
            y += p.line_height(Role::Body) * 0.6;
            let default = self
                .text
                .format("options.default", def.value_label(def.default));
            p.paragraph(Role::Body, desc_left, y, width, palette::TEXT, &default);
        }

        // Hints.
        let hints = [
            (GuiKey::ResetAll, self.text.get("hint.reset_all")),
            (GuiKey::ResetGroup, self.text.get("hint.reset_page")),
            (GuiKey::Cancel, self.text.get("hint.back")),
        ];
        let rects = p.hint_bar(w / 2.0, u(1040.0), self.device, &hints);
        for (rect, hit) in rects
            .into_iter()
            .zip([Hit::ResetAll, Hit::ResetSub, Hit::Back])
        {
            self.hits.push((rect, hit));
        }

        if let Some((dialog, _)) = &mut self.dialog {
            dialog.draw(p, pulse);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_row(
        &mut self,
        p: &mut Painter<'_>,
        rect: Rect,
        index: usize,
        row: usize,
        selected: bool,
        pulse: f32,
    ) {
        let w = p.theme.width;
        let scale = p.theme.scale;
        let u = |v: f32| v * scale;
        let def = &self.defs[index];
        let value = self.values[index];
        let cy = rect.center_y();
        if selected {
            p.selection(rect.inset(0.0, u(1.0)), pulse);
        }
        p.rule(rect.x, rect.bottom() - u(1.0), rect.w, 1.0, palette::RULE);
        let (text, value_color) = if def.disabled {
            (palette::TEXT_DISABLED, palette::TEXT_DISABLED)
        } else {
            (palette::TEXT, palette::TEXT)
        };
        p.text(
            Role::Row,
            rect.x + u(30.0),
            cy,
            Align::Left,
            text,
            &def.label,
        );
        self.hits.push((rect, Hit::Row(row)));

        let left_arrow = w * 0.3205;
        let right_arrow = w * 0.5075;
        let arrows = if def.disabled {
            palette::TEXT_DISABLED
        } else {
            palette::TEXT
        };
        match &def.kind {
            OptionKind::Choice(choices) => {
                let center = (left_arrow + right_arrow) / 2.0;
                p.chevron(left_arrow, cy, 22.0, Chevron::Left, arrows);
                p.chevron(right_arrow, cy, 22.0, Chevron::Right, arrows);
                let label = def.value_label(value);
                p.text(
                    Role::Row,
                    center,
                    cy - u(4.0),
                    Align::Center,
                    value_color,
                    &label,
                );
                p.pips(center, cy + u(18.0), choices.len(), value.max(0) as usize);
                let half = (right_arrow - left_arrow) / 2.0;
                self.hits.push((
                    Rect::new(left_arrow - u(30.0), rect.y, half + u(30.0), rect.h),
                    Hit::Decrease(row),
                ));
                self.hits.push((
                    Rect::new(left_arrow + half, rect.y, half + u(30.0), rect.h),
                    Hit::Increase(row),
                ));
            }
            OptionKind::Slider { min, max, .. } => {
                // The value sits right-aligned inside the row, sized for
                // the widest label so the track never moves under it.
                let label_w = p
                    .measure(Role::Row, &def.value_label(*min))
                    .max(p.measure(Role::Row, &def.value_label(*max)));
                let value_right = rect.right() - u(24.0);
                let right_chevron = value_right - label_w - u(24.0);
                let left_chevron = left_arrow;
                let track_left = left_chevron + u(18.0);
                let track_right = right_chevron - u(18.0);
                p.chevron(left_chevron, cy, 22.0, Chevron::Left, arrows);
                p.chevron(right_chevron, cy, 22.0, Chevron::Right, arrows);
                let fraction = (value - min) as f32 / (max - min).max(1) as f32;
                let positions = def.positions().min(21);
                p.slider(
                    Rect::new(track_left, cy - u(20.0), track_right - track_left, u(40.0)),
                    fraction,
                    positions,
                    arrows,
                );
                let label = def.value_label(value);
                p.text(
                    Role::Row,
                    value_right,
                    cy,
                    Align::Right,
                    value_color,
                    &label,
                );
                self.hits.push((
                    Rect::new(left_chevron - u(20.0), rect.y, u(40.0), rect.h),
                    Hit::Decrease(row),
                ));
                self.hits.push((
                    Rect::new(right_chevron - u(20.0), rect.y, u(40.0), rect.h),
                    Hit::Increase(row),
                ));
                self.hits.push((
                    Rect::new(
                        track_left - u(8.0),
                        rect.y,
                        track_right - track_left + u(16.0),
                        rect.h,
                    ),
                    Hit::Track(row, track_left, track_right),
                ));
            }
        }
        if value != def.default {
            p.diamond(rect.right() + u(16.0), cy, 7.0, Color::WHITE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::canvas::Canvas;
    use crate::gui::options::Choice;
    use crate::gui::text::Fonts;
    use crate::gui::theme::{FontFamily, Theme};

    /// One page: a choice and a 0..=100 slider.
    fn screen() -> OptionsScreen {
        let def = |id: &str, kind| OptionDef {
            id: id.to_string(),
            category: "C".to_string(),
            subcategory: "S".to_string(),
            group: "G".to_string(),
            label: id.to_string(),
            description: String::new(),
            kind,
            default: 0,
            disabled: false,
        };
        let defs = vec![
            def(
                "choice",
                OptionKind::Choice(vec![Choice::new("A"), Choice::new("B")]),
            ),
            def(
                "slider",
                OptionKind::Slider {
                    min: 0,
                    max: 100,
                    step: 5,
                    suffix: "%",
                },
            ),
        ];
        let mut screen = OptionsScreen::new(defs, vec![0, 0], Locale::default());
        let mut canvas = Canvas::new(1920, 1080);
        let mut fonts = Fonts::new().unwrap();
        screen.draw(&mut Painter {
            canvas: &mut canvas,
            fonts: &mut fonts,
            theme: Theme::new(1920, 1080, FontFamily::Modern),
        });
        screen
    }

    /// The centre of the first rectangle `want` accepts.
    fn find(screen: &OptionsScreen, want: impl Fn(Hit) -> bool) -> (Rect, Hit) {
        *screen
            .hits
            .iter()
            .find(|&&(_, hit)| want(hit))
            .expect("drawn")
    }

    #[test]
    fn hovering_any_part_of_a_row_selects_it() {
        let mut screen = screen();
        for want in [
            |hit| matches!(hit, Hit::Increase(1)),
            |hit| matches!(hit, Hit::Track(1, ..)),
            |hit| matches!(hit, Hit::Decrease(0)),
        ] {
            let (rect, hit) = find(&screen, want);
            screen.handle(GuiEvent::PointerMove {
                x: rect.center_x(),
                y: rect.center_y(),
            });
            assert_eq!(Some(screen.row), hit.row(), "{hit:?}");
        }
    }

    #[test]
    fn dragging_a_slider_follows_the_pointer_until_release() {
        let mut screen = screen();
        let (rect, Hit::Track(_, left, right)) =
            find(&screen, |hit| matches!(hit, Hit::Track(1, ..)))
        else {
            unreachable!()
        };
        let y = rect.center_y();
        let at = |fraction: f32| left + (right - left) * fraction;
        let changed = |events: Vec<OptionsEvent>| match events.as_slice() {
            [OptionsEvent::Changed { index: 1, value }] => Some(*value),
            _ => None,
        };
        assert_eq!(
            changed(screen.handle(GuiEvent::PointerDown { x: at(0.5), y })),
            Some(50)
        );
        // Off the track and off the row, the drag still steers it.
        assert_eq!(
            changed(screen.handle(GuiEvent::PointerMove {
                x: at(0.8),
                y: y + 400.0
            })),
            Some(80)
        );
        assert_eq!(
            changed(screen.handle(GuiEvent::PointerMove {
                x: right + 500.0,
                y
            })),
            Some(100)
        );
        screen.handle(GuiEvent::PointerUp { x: at(1.0), y });
        assert_eq!(
            changed(screen.handle(GuiEvent::PointerMove { x: at(0.1), y })),
            None
        );
        assert_eq!(screen.values[1], 100);
    }
}
