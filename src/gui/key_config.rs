//! The key configuration screen: the keyboard and mouse bindings of every
//! action on two tabs, two columns of keys per action and a diagram of the
//! keyboard and mouse with the bound keys lit.
//!
//! The screen edits a copy of the bindings and reports each accepted change
//! as a whole [`Bindings`]; the gamepad sources ride along untouched. A key
//! is captured by the owner, which routes the next raw press through
//! [`KeyConfigScreen::capture`] while [`KeyConfigScreen::capturing`] says so.

use sdl3_sys::scancode::*;

use super::canvas::{Color, Rect};
use super::dialog::{Dialog, DialogResult};
use super::input::{Device, GuiEvent, GuiKey};
use super::locale::Locale;
use super::text::{Align, Style};
use super::theme::{Role, palette};
use super::widgets::{Painter, SELECTION_GLOW};
use crate::controls::{ACTIONS, Action, Bindings, Source, Wheel};

/// Row heights, in units.
const ROW_H: f32 = 66.0;
const GROUP_H: f32 = 40.0;
const GROUP_GAP: f32 = 24.0;

/// The key columns per action.
const COLUMNS: usize = 2;

/// One row: its string key under `keys.` and the action it binds.
type Row = (&'static str, Action);

/// One tab: its string key and its groups, each a string key and its rows.
type Tab = (&'static str, &'static [(&'static str, &'static [Row])]);

/// The tabs, their groups and rows, by string key. An action on both tabs
/// is one binding shown twice.
const TABS: [Tab; 2] = [
    (
        "action",
        &[
            (
                "move",
                &[
                    ("forward", Action::Up),
                    ("backward", Action::Down),
                    ("turn_left", Action::Left),
                    ("turn_right", Action::Right),
                    ("run", Action::Run),
                    ("quick_turn", Action::QuickTurn),
                ],
            ),
            (
                "weapon",
                &[
                    ("aim", Action::Aim),
                    ("fire", Action::Fire),
                    ("reload", Action::Reload),
                    ("weapon_prev", Action::WeaponPrev),
                    ("weapon_next", Action::WeaponNext),
                    ("target_prev", Action::TargetPrev),
                    ("target_next", Action::TargetNext),
                ],
            ),
            (
                "examine",
                &[
                    ("interact", Action::Confirm),
                    ("inventory", Action::Start),
                    ("map", Action::Map),
                ],
            ),
            (
                "equip",
                &[
                    ("equip_1", Action::EquipSlot1),
                    ("equip_2", Action::EquipSlot2),
                    ("equip_3", Action::EquipSlot3),
                    ("equip_4", Action::EquipSlot4),
                    ("equip_5", Action::EquipSlot5),
                    ("equip_6", Action::EquipSlot6),
                    ("equip_7", Action::EquipSlot7),
                    ("equip_8", Action::EquipSlot8),
                    ("equip_9", Action::EquipSlot9),
                    ("equip_10", Action::EquipSlot10),
                ],
            ),
        ],
    ),
    (
        "menu",
        &[
            (
                "cursor",
                &[
                    ("up", Action::Up),
                    ("down", Action::Down),
                    ("left", Action::Left),
                    ("right", Action::Right),
                ],
            ),
            (
                "select",
                &[("confirm", Action::Confirm), ("cancel", Action::Cancel)],
            ),
            (
                "pages",
                &[
                    ("page_left", Action::PageLeft),
                    ("page_right", Action::PageRight),
                ],
            ),
        ],
    ),
];

/// What the key configuration screen asks of its owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyConfigEvent {
    /// The bindings are now these.
    Changed(Box<Bindings>),
    /// Leave the screen.
    Back,
}

/// What a pointer press landed on.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Hit {
    Tab(usize),
    TabPrev,
    TabNext,
    /// A row's label (the row in the tab).
    Row(usize),
    /// A row's key column.
    Cell(usize, usize),
    Key(GuiKey),
}

/// One line of the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Line {
    Group(&'static str),
    Row(usize),
}

pub struct KeyConfigScreen {
    bindings: Bindings,
    tab: usize,
    row: usize,
    column: usize,
    /// The list's scroll offset in units.
    scroll: f32,
    /// Waiting for the key to bind to the selected cell.
    capturing: bool,
    /// The outcome of the last edit, shown under the description.
    note: Option<String>,
    hits: Vec<(Rect, Hit)>,
    dialog: Option<Dialog>,
    pub(super) device: Device,
    text: Locale,
}

impl KeyConfigScreen {
    pub fn new(bindings: Bindings, text: Locale) -> Self {
        Self {
            bindings,
            tab: 0,
            row: 0,
            column: 0,
            scroll: 0.0,
            capturing: false,
            note: None,
            hits: Vec::new(),
            dialog: None,
            device: Device::Keyboard,
            text,
        }
    }

    pub fn set_locale(&mut self, text: Locale) {
        self.text = text;
    }

    pub fn set_device(&mut self, device: Device) {
        self.device = device;
    }

    /// Take the bindings the owner applied.
    pub fn set_bindings(&mut self, bindings: Bindings) {
        self.bindings = bindings;
    }

    /// Whether the next raw press is the key to bind: the owner hands it to
    /// [`Self::capture`] instead of feeding events.
    pub fn capturing(&self) -> bool {
        self.capturing
    }

    /// The tab's rows in order.
    fn rows(&self) -> Vec<Row> {
        TABS[self.tab]
            .1
            .iter()
            .flat_map(|(_, rows)| rows.iter().copied())
            .collect()
    }

    /// The tab's list with its group headers.
    fn lines(&self) -> Vec<Line> {
        let mut lines = Vec::new();
        let mut row = 0;
        for &(group, rows) in TABS[self.tab].1 {
            lines.push(Line::Group(group));
            for _ in rows {
                lines.push(Line::Row(row));
                row += 1;
            }
        }
        lines
    }

    fn selected(&self) -> Row {
        self.rows()[self.row]
    }

    /// `action`'s keyboard and mouse sources, in order.
    fn keys(&self, action: Action) -> Vec<Source> {
        keys(&self.bindings, action)
    }

    /// The name an action goes by: its row on this tab, else the other's.
    fn action_label(&self, action: Action) -> String {
        let on = |tab: usize| {
            TABS[tab]
                .1
                .iter()
                .flat_map(|(_, rows)| rows.iter())
                .find(|(_, a)| *a == action)
                .map(|(key, _)| *key)
        };
        let key = on(self.tab).or_else(|| on(1 - self.tab)).unwrap_or("");
        self.text.get(&format!("keys.{key}")).to_string()
    }

    fn set_tab(&mut self, tab: usize) {
        self.hits.clear();
        if tab != self.tab {
            self.tab = tab;
            self.row = 0;
            self.scroll = 0.0;
        }
    }

    /// Accept `bindings` unless they leave an essential action without a
    /// key of its own.
    fn commit(&mut self, bindings: Bindings, events: &mut Vec<KeyConfigEvent>) -> bool {
        if let Some(action) = bindings.lockout() {
            let label = self.action_label(action);
            self.note = Some(self.text.format("keys.keep_one", label));
            return false;
        }
        if bindings != self.bindings {
            self.bindings = bindings.clone();
            events.push(KeyConfigEvent::Changed(Box::new(bindings)));
        }
        true
    }

    /// Feed the raw press that ends a capture, with where the pointer went
    /// down if it did. Escape, a gamepad press or a click on the cancel hint
    /// cancels; a key bound elsewhere moves here.
    pub fn capture(&mut self, source: Source, click: Option<(f32, f32)>) -> Vec<KeyConfigEvent> {
        let mut events = Vec::new();
        if !self.capturing {
            return events;
        }
        self.capturing = false;
        let on_cancel = source == Source::Mouse(1)
            && click.is_some_and(|(x, y)| matches!(self.hit(x, y), Some(Hit::Key(GuiKey::Cancel))));
        if on_cancel || source.is_pad() || source == Source::key(SDL_SCANCODE_ESCAPE) {
            return events;
        }
        if !source.round_trips() {
            self.note = Some(self.text.get("keys.unsaveable").to_string());
            return events;
        }
        let (_, action) = self.selected();
        let mut bindings = self.bindings.clone();
        let mut moved = Vec::new();
        for other in ACTIONS {
            if other != action && bindings.get(other).contains(&source) {
                let rest = bindings
                    .get(other)
                    .iter()
                    .copied()
                    .filter(|&s| s != source)
                    .collect();
                bindings.set(other, rest);
                moved.push(self.action_label(other));
            }
        }
        let mut keys = keys(&bindings, action);
        // A key already in the other column moves to this one.
        keys.retain(|&s| s != source);
        match keys.get_mut(self.column) {
            Some(slot) => *slot = source,
            None => keys.push(source),
        }
        set_keys(&mut bindings, action, keys);
        if self.commit(bindings, &mut events) && !moved.is_empty() {
            self.note = Some(self.text.format("keys.moved", moved.join(", ")));
        }
        events
    }

    /// Unbind the selected cell.
    fn clear(&mut self, events: &mut Vec<KeyConfigEvent>) {
        let (_, action) = self.selected();
        let mut keys = self.keys(action);
        if self.column < keys.len() {
            keys.remove(self.column);
            let mut bindings = self.bindings.clone();
            set_keys(&mut bindings, action, keys);
            self.commit(bindings, events);
        }
    }

    /// Every action's keyboard and mouse sources back to the defaults; the
    /// gamepad's stay.
    fn reset(&mut self, events: &mut Vec<KeyConfigEvent>) {
        let defaults = Bindings::default();
        let mut bindings = self.bindings.clone();
        for action in ACTIONS {
            set_keys(&mut bindings, action, keys(&defaults, action));
        }
        self.commit(bindings, events);
    }

    fn start_capture(&mut self) {
        self.capturing = true;
        self.note = None;
    }

    /// Feed one event; returns what the owner must do.
    pub fn handle(&mut self, event: GuiEvent) -> Vec<KeyConfigEvent> {
        let mut events = Vec::new();
        if let GuiEvent::Key(_, device) = event {
            self.device = device;
        }
        if let Some(dialog) = &mut self.dialog {
            match dialog.handle(event) {
                DialogResult::Open => {}
                DialogResult::Chosen(0) => {
                    self.dialog = None;
                    self.reset(&mut events);
                }
                DialogResult::Chosen(_) | DialogResult::Cancelled => self.dialog = None,
            }
            return events;
        }
        if self.capturing {
            return events;
        }
        if matches!(event, GuiEvent::Key(..) | GuiEvent::PointerDown { .. }) {
            self.note = None;
        }
        let rows = self.rows().len();
        let tabs = TABS.len();
        match event {
            GuiEvent::Key(key, _) => self.key(key, rows, tabs, &mut events),
            GuiEvent::PointerMove { x, y } => match self.hit(x, y) {
                Some(Hit::Row(row)) => self.row = row,
                Some(Hit::Cell(row, column)) => {
                    self.row = row;
                    self.column = column;
                }
                _ => {}
            },
            GuiEvent::PointerDown { x, y } => match self.hit(x, y) {
                Some(Hit::Tab(tab)) => self.set_tab(tab),
                Some(Hit::TabPrev) => self.key(GuiKey::TabPrev, rows, tabs, &mut events),
                Some(Hit::TabNext) => self.key(GuiKey::TabNext, rows, tabs, &mut events),
                Some(Hit::Row(row)) => self.row = row,
                Some(Hit::Cell(row, column)) => {
                    self.row = row;
                    self.column = column;
                    self.start_capture();
                }
                Some(Hit::Key(key)) => self.key(key, rows, tabs, &mut events),
                None => {}
            },
            GuiEvent::Wheel(notches) if rows > 0 => {
                let step = if notches > 0.0 { rows - 1 } else { 1 };
                self.row = (self.row + step) % rows;
            }
            GuiEvent::PointerUp { .. } | GuiEvent::Wheel(_) => {}
        }
        events
    }

    fn key(&mut self, key: GuiKey, rows: usize, tabs: usize, events: &mut Vec<KeyConfigEvent>) {
        match key {
            GuiKey::Up => self.row = (self.row + rows - 1) % rows,
            GuiKey::Down => self.row = (self.row + 1) % rows,
            GuiKey::Left => self.column = self.column.saturating_sub(1),
            GuiKey::Right => self.column = (self.column + 1).min(COLUMNS - 1),
            GuiKey::Confirm => self.start_capture(),
            GuiKey::Cancel => events.push(KeyConfigEvent::Back),
            GuiKey::TabPrev => self.set_tab((self.tab + tabs - 1) % tabs),
            GuiKey::TabNext => self.set_tab((self.tab + 1) % tabs),
            GuiKey::ResetAll => self.clear(events),
            GuiKey::ResetGroup => {
                self.dialog = Some(Dialog::yes_no(
                    &self.text,
                    self.text.get("keys.reset_question"),
                    None,
                ));
            }
            GuiKey::SubPrev | GuiKey::SubNext => {}
        }
    }

    /// Where the cursor is: the tab, row and column, whether a key is being
    /// captured and the open dialog's choice, for the owner's sounds.
    pub(super) fn cursor(&self) -> (usize, usize, usize, bool, Option<usize>) {
        (
            self.tab,
            self.row,
            self.column,
            self.capturing,
            self.dialog.as_ref().map(|dialog| dialog.selected),
        )
    }

    fn hit(&self, x: f32, y: f32) -> Option<Hit> {
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
        let list_right = w * 0.517;
        let divider = w * 0.536;
        let desc_left = w * 0.556;

        p.text(
            Role::Title,
            w / 2.0,
            u(97.0),
            Align::Center,
            palette::TEXT,
            self.text.get("keys.title"),
        );

        // The tabs between the Q and E key caps, centred.
        let tab_y = u(153.0);
        let tab_w = u(200.0);
        let prev_w = p.glyph_width(self.device, GuiKey::TabPrev);
        let next_w = p.glyph_width(self.device, GuiKey::TabNext);
        let gap = u(35.0);
        let total = prev_w + gap + tab_w * TABS.len() as f32 + gap + next_w;
        let start = w / 2.0 - total / 2.0;
        let names: Vec<String> = TABS
            .iter()
            .map(|(key, _)| self.text.get(&format!("keys.tab.{key}")).to_string())
            .collect();
        let strip = p.tabs(start, tab_y, tab_w, self.device, &names, self.tab);
        self.hits.push((strip.prev, Hit::TabPrev));
        for (index, rect) in strip.tabs.into_iter().enumerate() {
            self.hits.push((rect, Hit::Tab(index)));
        }
        self.hits.push((strip.next, Hit::TabNext));
        p.rule(
            start - u(20.0),
            u(181.0),
            total + u(40.0),
            2.0,
            palette::RULE_BRIGHT,
        );

        // The list.
        let list = Rect::new(left, u(240.0), list_right - left, u(1000.0) - u(240.0));
        let lines = self.lines();
        let mut layout = Vec::with_capacity(lines.len());
        let mut y = 0.0;
        for (position, line) in lines.iter().enumerate() {
            match line {
                Line::Group(_) => {
                    if position > 0 {
                        y += GROUP_GAP;
                    }
                    layout.push((y, GROUP_H));
                    y += GROUP_H;
                }
                Line::Row(_) => {
                    layout.push((y, ROW_H));
                    y += ROW_H;
                }
            }
        }
        let total = y;
        let visible = list.h / p.theme.scale;
        if let Some(position) = lines.iter().position(|&l| l == Line::Row(self.row)) {
            let (top, height) = layout[position];
            let header = match position.checked_sub(1).map(|p| lines[p]) {
                Some(Line::Group(_)) => layout[position - 1].0,
                _ => top,
            };
            if header < self.scroll {
                self.scroll = header;
            } else if top + height > self.scroll + visible {
                self.scroll = top + height - visible;
            }
        }
        self.scroll = self.scroll.clamp(0.0, (total - visible).max(0.0));

        let rows = self.rows();
        let (_, selected_action) = rows[self.row];
        p.canvas.push_clip(list);
        for (position, line) in lines.iter().enumerate() {
            let (top, height) = layout[position];
            let rect = Rect::new(list.x, list.y + u(top - self.scroll), list.w, u(height));
            if rect.bottom() <= list.y || rect.y >= list.bottom() {
                continue;
            }
            match *line {
                Line::Group(key) => {
                    let name = self.text.get(&format!("keys.group.{key}")).to_string();
                    p.group_header(rect, &name);
                }
                Line::Row(row) => self.draw_row(p, rect, row, rows[row], pulse),
            }
        }
        p.canvas.pop_clip();
        let bar = Rect::new(divider - u(4.0), list.y, u(8.0), list.h);
        p.scrollbar(bar, self.scroll, visible, total);

        // The description pane.
        let width = right - desc_left;
        let (key, action) = rows[self.row];
        let mut y = p.paragraph(
            Role::Body,
            desc_left,
            list.y,
            width,
            palette::TEXT,
            self.text.get(&format!("keys.{key}_note")),
        );
        let other = TABS[1 - self.tab]
            .1
            .iter()
            .flat_map(|(_, rows)| rows.iter())
            .find(|(_, a)| *a == action);
        if let Some((other, _)) = other {
            let shared = self
                .text
                .format("keys.shared", self.text.get(&format!("keys.{other}")));
            y = p.paragraph(Role::Body, desc_left, y, width, palette::TEXT_DIM, &shared);
        }
        let status = if self.capturing {
            Some(self.text.get("keys.capture").to_string())
        } else {
            self.note.clone()
        };
        if let Some(status) = status {
            y += p.line_height(Role::Body) * 0.6;
            y = p.paragraph(Role::Body, desc_left, y, width, palette::TEXT, &status);
        }

        // The keyboard and mouse, as large as fits under the text.
        let room = list.bottom() - (y + u(30.0));
        let k = (width / DIAGRAM_W).min(room / DIAGRAM_H);
        if k > u(8.0) {
            let origin = (
                desc_left + (width - k * DIAGRAM_W) / 2.0,
                list.bottom() - k * DIAGRAM_H,
            );
            let tab_actions: Vec<Action> = rows.iter().map(|&(_, a)| a).collect();
            let state = |source: Source| {
                if source.reserved_bit().is_some() {
                    KeyState::Reserved
                } else if self.bindings.get(selected_action).contains(&source) {
                    KeyState::Selected
                } else if tab_actions
                    .iter()
                    .any(|&a| self.bindings.get(a).contains(&source))
                {
                    KeyState::Bound
                } else {
                    KeyState::Unbound
                }
            };
            draw_diagram(p, origin, k, &state);
        }

        // Hints.
        let hints: Vec<(GuiKey, &str)> = if self.capturing {
            vec![(GuiKey::Cancel, self.text.get("hint.cancel"))]
        } else {
            vec![
                (GuiKey::ResetAll, self.text.get("hint.clear")),
                (GuiKey::ResetGroup, self.text.get("hint.reset")),
                (GuiKey::Confirm, self.text.get("hint.select")),
                (GuiKey::Cancel, self.text.get("hint.back")),
            ]
        };
        let rects = p.hint_bar(w / 2.0, u(1040.0), self.device, &hints);
        for (rect, &(key, _)) in rects.into_iter().zip(&hints) {
            self.hits.push((rect, Hit::Key(key)));
        }

        if let Some(dialog) = &mut self.dialog {
            dialog.draw(p, pulse);
        }
    }

    fn draw_row(&mut self, p: &mut Painter<'_>, rect: Rect, row: usize, entry: Row, pulse: f32) {
        let scale = p.theme.scale;
        let u = |v: f32| v * scale;
        let (key, action) = entry;
        let selected = row == self.row;
        let label_w = rect.w * 0.57;
        let cell_w = (rect.w - label_w) / COLUMNS as f32;
        let cy = rect.center_y();
        let label_rect = Rect::new(rect.x, rect.y, label_w, rect.h);
        if selected {
            p.selection(label_rect.inset(0.0, u(1.0)), pulse);
        }
        p.rule(rect.x, rect.bottom() - u(1.0), rect.w, 1.0, palette::RULE);
        let label = self.text.get(&format!("keys.{key}")).to_string();
        p.text(
            Role::Row,
            rect.x + u(30.0),
            cy,
            Align::Left,
            palette::TEXT,
            &label,
        );
        self.hits.push((label_rect, Hit::Row(row)));
        let keys = self.keys(action);
        for column in 0..COLUMNS {
            let cell = Rect::new(
                rect.x + label_w + cell_w * column as f32,
                rect.y,
                cell_w,
                rect.h,
            );
            p.vrule(cell.x, cell.y, cell.h, 1.0, palette::RULE);
            let here = selected && column == self.column;
            if here {
                p.selection(cell.inset(0.0, u(1.0)), pulse);
            }
            if here && self.capturing {
                p.text(
                    Role::KeyCap,
                    cell.center_x(),
                    cy,
                    Align::Center,
                    palette::TEXT,
                    self.text.get("keys.waiting"),
                );
            } else if let Some(&source) = keys.get(column) {
                draw_source(p, cell.center_x(), cy, source);
            }
            if column == COLUMNS - 1 && keys.len() > COLUMNS {
                p.text(
                    Role::KeyCap,
                    cell.right() - u(10.0),
                    cy,
                    Align::Right,
                    palette::TEXT_DIM,
                    &format!("+{}", keys.len() - COLUMNS),
                );
            }
            self.hits.push((cell, Hit::Cell(row, column)));
        }
    }
}

/// `action`'s keyboard and mouse sources in `bindings`, in order.
fn keys(bindings: &Bindings, action: Action) -> Vec<Source> {
    bindings
        .get(action)
        .iter()
        .copied()
        .filter(|s| !s.is_pad())
        .collect()
}

/// Replace `action`'s keyboard and mouse sources, keeping its gamepad ones.
fn set_keys(bindings: &mut Bindings, action: Action, keys: Vec<Source>) {
    let pads = bindings.get(action).iter().copied().filter(|s| s.is_pad());
    let sources = keys.into_iter().chain(pads).collect();
    bindings.set(action, sources);
}

/// A source in a list cell, centred on `(cx, cy)`: a key cap, or the mouse
/// with the button lit.
fn draw_source(p: &mut Painter<'_>, cx: f32, cy: f32, source: Source) {
    match source {
        Source::Key(_) => {
            let label = source.short_name();
            let width = p.key_cap_width(&label);
            p.key_cap(cx - width / 2.0, cy, &label);
        }
        _ => {
            let s = p.u(40.0) / MOUSE_H;
            let origin = (cx - s * MOUSE_W / 2.0, cy - s * MOUSE_H / 2.0);
            draw_mouse(p, origin, s, &|other| {
                if other == source {
                    KeyState::Selected
                } else {
                    KeyState::Unbound
                }
            });
        }
    }
}

/// How a key in the diagram is lit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyState {
    Unbound,
    /// Bound to an action on this tab.
    Bound,
    /// Bound to the selected action.
    Selected,
    /// A fixed port key (F1, F9) that cannot be bound.
    Reserved,
}

impl KeyState {
    /// The face and the label's ink.
    fn colors(self) -> (Color, Color) {
        match self {
            KeyState::Unbound => (Color::rgb(0x22, 0x22, 0x22), palette::TEXT_DIM),
            KeyState::Bound => (Color::rgb(0x84, 0x84, 0x84), Color::rgb(0x10, 0x10, 0x10)),
            KeyState::Selected => (palette::TEXT, Color::rgb(0x10, 0x10, 0x10)),
            KeyState::Reserved => (Color::rgb(0x22, 0x22, 0x22), palette::TEXT_DISABLED),
        }
    }
}

/// A key cap's legend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cap {
    Text(&'static str),
    Arrow(Arrow),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Arrow {
    Up,
    Down,
    Left,
    Right,
}

/// One key of the diagram, in key units from the diagram's top left.
#[derive(Clone, Copy)]
struct KeyShape {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    code: SDL_Scancode,
    cap: Cap,
}

/// The diagram's size in key units: the main block, then the navigation
/// keys, the numeric keypad and the mouse below it.
const DIAGRAM_W: f32 = 15.0;
const DIAGRAM_H: f32 = 11.75;
/// Where the lower block starts.
const LOWER_Y: f32 = 6.75;
/// The mouse's box in mouse units, and where it sits in key units.
const MOUSE_W: f32 = 4.25;
const MOUSE_H: f32 = 5.0;
const MOUSE_X: f32 = 9.5;

/// A full-size keyboard by SDL scancode.
fn keyboard() -> Vec<KeyShape> {
    use Cap::Text as T;
    type Item = (Option<SDL_Scancode>, Cap, f32);
    const fn k(code: SDL_Scancode, cap: &'static str, w: f32) -> Item {
        (Some(code), Cap::Text(cap), w)
    }
    const fn gap(w: f32) -> Item {
        (None, Cap::Text(""), w)
    }
    fn row(keys: &mut Vec<KeyShape>, x: f32, y: f32, items: &[Item]) {
        let mut x = x;
        for &(code, cap, w) in items {
            if let Some(code) = code {
                keys.push(KeyShape {
                    x,
                    y,
                    w,
                    h: 1.0,
                    code,
                    cap,
                });
            }
            x += w;
        }
    }
    let mut keys = Vec::new();
    let one = |code, cap| k(code, cap, 1.0);
    row(
        &mut keys,
        0.0,
        0.0,
        &[
            one(SDL_SCANCODE_ESCAPE, "Esc"),
            gap(1.0),
            one(SDL_SCANCODE_F1, "F1"),
            one(SDL_SCANCODE_F2, "F2"),
            one(SDL_SCANCODE_F3, "F3"),
            one(SDL_SCANCODE_F4, "F4"),
            gap(0.5),
            one(SDL_SCANCODE_F5, "F5"),
            one(SDL_SCANCODE_F6, "F6"),
            one(SDL_SCANCODE_F7, "F7"),
            one(SDL_SCANCODE_F8, "F8"),
            gap(0.5),
            one(SDL_SCANCODE_F9, "F9"),
            one(SDL_SCANCODE_F10, "F10"),
            one(SDL_SCANCODE_F11, "F11"),
            one(SDL_SCANCODE_F12, "F12"),
        ],
    );
    row(
        &mut keys,
        0.0,
        1.25,
        &[
            one(SDL_SCANCODE_GRAVE, "`"),
            one(SDL_SCANCODE_1, "1"),
            one(SDL_SCANCODE_2, "2"),
            one(SDL_SCANCODE_3, "3"),
            one(SDL_SCANCODE_4, "4"),
            one(SDL_SCANCODE_5, "5"),
            one(SDL_SCANCODE_6, "6"),
            one(SDL_SCANCODE_7, "7"),
            one(SDL_SCANCODE_8, "8"),
            one(SDL_SCANCODE_9, "9"),
            one(SDL_SCANCODE_0, "0"),
            one(SDL_SCANCODE_MINUS, "-"),
            one(SDL_SCANCODE_EQUALS, "="),
            k(SDL_SCANCODE_BACKSPACE, "Bksp", 2.0),
        ],
    );
    row(
        &mut keys,
        0.0,
        2.25,
        &[
            k(SDL_SCANCODE_TAB, "Tab", 1.5),
            one(SDL_SCANCODE_Q, "Q"),
            one(SDL_SCANCODE_W, "W"),
            one(SDL_SCANCODE_E, "E"),
            one(SDL_SCANCODE_R, "R"),
            one(SDL_SCANCODE_T, "T"),
            one(SDL_SCANCODE_Y, "Y"),
            one(SDL_SCANCODE_U, "U"),
            one(SDL_SCANCODE_I, "I"),
            one(SDL_SCANCODE_O, "O"),
            one(SDL_SCANCODE_P, "P"),
            one(SDL_SCANCODE_LEFTBRACKET, "["),
            one(SDL_SCANCODE_RIGHTBRACKET, "]"),
            k(SDL_SCANCODE_BACKSLASH, "\\", 1.5),
        ],
    );
    row(
        &mut keys,
        0.0,
        3.25,
        &[
            k(SDL_SCANCODE_CAPSLOCK, "Caps", 1.75),
            one(SDL_SCANCODE_A, "A"),
            one(SDL_SCANCODE_S, "S"),
            one(SDL_SCANCODE_D, "D"),
            one(SDL_SCANCODE_F, "F"),
            one(SDL_SCANCODE_G, "G"),
            one(SDL_SCANCODE_H, "H"),
            one(SDL_SCANCODE_J, "J"),
            one(SDL_SCANCODE_K, "K"),
            one(SDL_SCANCODE_L, "L"),
            one(SDL_SCANCODE_SEMICOLON, ";"),
            one(SDL_SCANCODE_APOSTROPHE, "'"),
            k(SDL_SCANCODE_RETURN, "Enter", 2.25),
        ],
    );
    row(
        &mut keys,
        0.0,
        4.25,
        &[
            k(SDL_SCANCODE_LSHIFT, "Shift", 2.25),
            one(SDL_SCANCODE_Z, "Z"),
            one(SDL_SCANCODE_X, "X"),
            one(SDL_SCANCODE_C, "C"),
            one(SDL_SCANCODE_V, "V"),
            one(SDL_SCANCODE_B, "B"),
            one(SDL_SCANCODE_N, "N"),
            one(SDL_SCANCODE_M, "M"),
            one(SDL_SCANCODE_COMMA, ","),
            one(SDL_SCANCODE_PERIOD, "."),
            one(SDL_SCANCODE_SLASH, "/"),
            k(SDL_SCANCODE_RSHIFT, "Shift", 2.75),
        ],
    );
    row(
        &mut keys,
        0.0,
        5.25,
        &[
            k(SDL_SCANCODE_LCTRL, "Ctrl", 1.25),
            k(SDL_SCANCODE_LGUI, "Win", 1.25),
            k(SDL_SCANCODE_LALT, "Alt", 1.25),
            k(SDL_SCANCODE_SPACE, "Space", 6.25),
            k(SDL_SCANCODE_RALT, "Alt", 1.25),
            k(SDL_SCANCODE_RGUI, "Win", 1.25),
            k(SDL_SCANCODE_APPLICATION, "Menu", 1.25),
            k(SDL_SCANCODE_RCTRL, "Ctrl", 1.25),
        ],
    );
    // The navigation keys and arrows.
    let y = LOWER_Y;
    row(
        &mut keys,
        0.0,
        y,
        &[
            one(SDL_SCANCODE_PRINTSCREEN, "PrtSc"),
            one(SDL_SCANCODE_SCROLLLOCK, "ScrLk"),
            one(SDL_SCANCODE_PAUSE, "Pause"),
        ],
    );
    row(
        &mut keys,
        0.0,
        y + 1.0,
        &[
            one(SDL_SCANCODE_INSERT, "Ins"),
            one(SDL_SCANCODE_HOME, "Home"),
            one(SDL_SCANCODE_PAGEUP, "PgUp"),
        ],
    );
    row(
        &mut keys,
        0.0,
        y + 2.0,
        &[
            one(SDL_SCANCODE_DELETE, "Del"),
            one(SDL_SCANCODE_END, "End"),
            one(SDL_SCANCODE_PAGEDOWN, "PgDn"),
        ],
    );
    let arrow = |code, arrow| (Some(code), Cap::Arrow(arrow), 1.0);
    row(
        &mut keys,
        0.0,
        y + 3.0,
        &[gap(1.0), arrow(SDL_SCANCODE_UP, Arrow::Up)],
    );
    row(
        &mut keys,
        0.0,
        y + 4.0,
        &[
            arrow(SDL_SCANCODE_LEFT, Arrow::Left),
            arrow(SDL_SCANCODE_DOWN, Arrow::Down),
            arrow(SDL_SCANCODE_RIGHT, Arrow::Right),
        ],
    );
    // The keypad, its plus and enter two rows tall and its zero two wide.
    let pad = 3.5;
    row(
        &mut keys,
        pad,
        y,
        &[
            one(SDL_SCANCODE_NUMLOCKCLEAR, "Num"),
            one(SDL_SCANCODE_KP_DIVIDE, "/"),
            one(SDL_SCANCODE_KP_MULTIPLY, "*"),
            one(SDL_SCANCODE_KP_MINUS, "-"),
        ],
    );
    for (r, codes) in [
        (
            1.0,
            [
                (SDL_SCANCODE_KP_7, "7"),
                (SDL_SCANCODE_KP_8, "8"),
                (SDL_SCANCODE_KP_9, "9"),
            ],
        ),
        (
            2.0,
            [
                (SDL_SCANCODE_KP_4, "4"),
                (SDL_SCANCODE_KP_5, "5"),
                (SDL_SCANCODE_KP_6, "6"),
            ],
        ),
        (
            3.0,
            [
                (SDL_SCANCODE_KP_1, "1"),
                (SDL_SCANCODE_KP_2, "2"),
                (SDL_SCANCODE_KP_3, "3"),
            ],
        ),
    ] {
        let items = codes.map(|(code, cap)| one(code, cap));
        row(&mut keys, pad, y + r, &items);
    }
    row(
        &mut keys,
        pad,
        y + 4.0,
        &[
            k(SDL_SCANCODE_KP_0, "0", 2.0),
            one(SDL_SCANCODE_KP_PERIOD, "."),
        ],
    );
    for (code, cap, r) in [
        (SDL_SCANCODE_KP_PLUS, "+", 1.0),
        (SDL_SCANCODE_KP_ENTER, "Ent", 3.0),
    ] {
        keys.push(KeyShape {
            x: pad + 3.0,
            y: y + r,
            w: 1.0,
            h: 2.0,
            code,
            cap: T(cap),
        });
    }
    keys
}

/// The keyboard and mouse with each key lit by `state`, `k` pixels to a key
/// unit from `origin`.
fn draw_diagram(
    p: &mut Painter<'_>,
    origin: (f32, f32),
    k: f32,
    state: &dyn Fn(Source) -> KeyState,
) {
    let (ox, oy) = origin;
    // The frames around the main block and the lower block.
    let line = p.u(2.0).max(1.0);
    let pad = k * 0.15;
    for frame in [
        Rect::new(ox, oy, DIAGRAM_W * k, 6.25 * k),
        Rect::new(ox, oy + LOWER_Y * k, 7.5 * k, 5.0 * k),
    ] {
        let outer = Rect::new(
            frame.x - pad,
            frame.y - pad,
            frame.w + pad * 2.0,
            frame.h + pad * 2.0,
        );
        p.canvas.fill_round_rect(outer, k * 0.2, palette::RULE);
        p.canvas.fill_round_rect(
            outer.inset(line, line),
            k * 0.2 - line,
            Color::rgb(0x0A, 0x0A, 0x0A),
        );
    }
    for key in keyboard() {
        let rect = Rect::new(ox + key.x * k, oy + key.y * k, key.w * k, key.h * k);
        draw_key(
            p,
            rect.inset(k * 0.05, k * 0.05),
            k * 0.9,
            key.cap,
            state(Source::key(key.code)),
        );
    }
    draw_mouse(p, (ox + MOUSE_X * k, oy + LOWER_Y * k), k, state);
}

/// One key cap of the diagram.
fn draw_key(p: &mut Painter<'_>, rect: Rect, unit: f32, cap: Cap, state: KeyState) {
    let (face, ink) = state.colors();
    let radius = rect.h.min(rect.w) * 0.12;
    let edge = p.u(1.5).max(1.0);
    let rim = match state {
        KeyState::Selected => Color::WHITE,
        _ => palette::RULE,
    };
    p.canvas.fill_round_rect(rect, radius, rim);
    p.canvas
        .fill_round_rect(rect.inset(edge, edge), (radius - edge).max(0.0), face);
    let (cx, cy) = (rect.center_x(), rect.center_y());
    match cap {
        Cap::Text(label) => {
            let base = p.style(Role::KeyCap);
            let mut px = (rect.h.min(unit).min(rect.w * 1.4) * 0.4).round().max(6.0);
            let room = rect.w - edge * 4.0;
            let measured = p.fonts.measure(Style::new(base.face, px as u32), label);
            if measured > room {
                px = (px * room / measured).floor().max(6.0);
            }
            let style = Style::new(base.face, px as u32);
            p.fonts
                .draw_centered(p.canvas, style, cx, cy, Align::Center, ink, label);
        }
        Cap::Arrow(arrow) => {
            let r = rect.h.min(rect.w) * 0.2;
            let points = match arrow {
                Arrow::Up => [(cx, cy - r), (cx + r, cy + r * 0.6), (cx - r, cy + r * 0.6)],
                Arrow::Down => [(cx, cy + r), (cx - r, cy - r * 0.6), (cx + r, cy - r * 0.6)],
                Arrow::Left => [(cx - r, cy), (cx + r * 0.6, cy - r), (cx + r * 0.6, cy + r)],
                Arrow::Right => [(cx + r, cy), (cx - r * 0.6, cy + r), (cx - r * 0.6, cy - r)],
            };
            p.canvas.fill_polygon(&points, ink);
        }
    }
    if state == KeyState::Reserved {
        // Struck through: the port keeps the key.
        let t = edge;
        p.canvas.fill_polygon(
            &[
                (rect.x + t, rect.bottom() - t * 2.0),
                (rect.right() - t * 2.0, rect.y + t),
                (rect.right() - t, rect.y + t * 2.0),
                (rect.x + t * 2.0, rect.bottom() - t),
            ],
            palette::RULE,
        );
    }
}

/// The mouse, `s` pixels to a mouse unit from `origin` (its box is
/// [`MOUSE_W`] by [`MOUSE_H`]): the body with its two buttons and wheel, and
/// the side buttons 4 and 5 off its left flank, each lit by `state`.
fn draw_mouse(p: &mut Painter<'_>, origin: (f32, f32), s: f32, state: &dyn Fn(Source) -> KeyState) {
    let (ox, oy) = origin;
    let at = |x: f32, y: f32, w: f32, h: f32| Rect::new(ox + x * s, oy + y * s, w * s, h * s);
    let edge = p.u(1.5).max(1.0);
    let body = at(0.75, 0.0, 3.5, 5.0);
    let radius = body.w / 2.0;
    p.canvas.fill_round_rect(body, radius, palette::RULE);
    let inner = body.inset(edge, edge);
    p.canvas
        .fill_round_rect(inner, radius - edge, KeyState::Unbound.colors().0);
    // Each button lights its part of the body.
    for (button, part) in [(1, at(0.75, 0.0, 1.75, 2.0)), (3, at(2.5, 0.0, 1.75, 2.0))] {
        let (face, _) = state(Source::Mouse(button)).colors();
        p.canvas.push_clip(part);
        p.canvas.fill_round_rect(inner, radius - edge, face);
        p.canvas.pop_clip();
    }
    p.canvas.fill_rect(
        Rect::new(body.x, oy + 2.0 * s - edge / 2.0, body.w, edge),
        palette::RULE,
    );
    p.canvas.fill_rect(
        Rect::new(ox + 2.5 * s - edge / 2.0, body.y, edge, 2.0 * s),
        palette::RULE,
    );
    // The wheel: its middle click, its notches up and down at either end and
    // its tilts as arrows beside it.
    let wheel = at(2.15, 0.35, 0.7, 1.3);
    p.canvas.fill_round_rect(
        wheel.inset(-edge, -edge),
        wheel.w / 2.0 + edge,
        palette::RULE,
    );
    let (face, _) = state(Source::Mouse(2)).colors();
    p.canvas.fill_round_rect(wheel, wheel.w / 2.0, face);
    for (direction, half) in [
        (
            Wheel::Up,
            Rect::new(wheel.x, wheel.y, wheel.w, wheel.h * 0.3),
        ),
        (
            Wheel::Down,
            Rect::new(
                wheel.x,
                wheel.bottom() - wheel.h * 0.3,
                wheel.w,
                wheel.h * 0.3,
            ),
        ),
    ] {
        let lit = state(Source::Wheel(direction));
        if lit != KeyState::Unbound {
            p.canvas.push_clip(half);
            p.canvas
                .fill_round_rect(wheel, wheel.w / 2.0, lit.colors().0);
            p.canvas.pop_clip();
        }
    }
    let cy = wheel.center_y();
    for (direction, x, sign) in [
        (Wheel::Left, wheel.x - 0.3 * s, -1.0f32),
        (Wheel::Right, wheel.right() + 0.3 * s, 1.0),
    ] {
        let lit = state(Source::Wheel(direction));
        let color = match lit {
            KeyState::Unbound => palette::RULE,
            other => other.colors().0,
        };
        let r = 0.18 * s;
        p.canvas.fill_polygon(
            &[
                (x + sign * r, cy),
                (x - sign * r, cy - r),
                (x - sign * r, cy + r),
            ],
            color,
        );
    }
    // The side buttons: forward (5) above back (4).
    for (button, y) in [(5u8, 2.3), (4, 3.2)] {
        let rect = at(0.0, y, 0.6, 0.8);
        let lit = state(Source::Mouse(button));
        let (face, ink) = lit.colors();
        p.canvas.fill_round_rect(rect, rect.w * 0.3, palette::RULE);
        p.canvas
            .fill_round_rect(rect.inset(edge, edge), (rect.w * 0.3 - edge).max(0.0), face);
        if rect.w > p.u(14.0) {
            let base = p.style(Role::KeyCap);
            let style = Style::new(base.face, (rect.w * 0.6).round().max(6.0) as u32);
            p.fonts.draw_centered(
                p.canvas,
                style,
                rect.center_x(),
                rect.center_y(),
                Align::Center,
                ink,
                &button.to_string(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::canvas::Canvas;
    use crate::gui::text::Fonts;
    use crate::gui::theme::{FontFamily, Theme};

    fn screen() -> KeyConfigScreen {
        KeyConfigScreen::new(Bindings::default(), Locale::default())
    }

    fn key(key: GuiKey) -> GuiEvent {
        GuiEvent::Key(key, Device::Keyboard)
    }

    fn changed(events: Vec<KeyConfigEvent>) -> Bindings {
        match events.as_slice() {
            [KeyConfigEvent::Changed(bindings)] => (**bindings).clone(),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn every_action_has_a_row() {
        for action in ACTIONS {
            assert!(
                TABS.iter()
                    .flat_map(|(_, groups)| groups.iter())
                    .flat_map(|(_, rows)| rows.iter())
                    .any(|&(_, a)| a == action),
                "{action:?}"
            );
        }
    }

    #[test]
    fn every_string_is_translated_to_english() {
        let text = Locale::default();
        let mut keys = vec!["keys.title".to_string()];
        for (tab, groups) in TABS {
            keys.push(format!("keys.tab.{tab}"));
            for &(group, rows) in groups {
                keys.push(format!("keys.group.{group}"));
                for &(row, _) in rows {
                    keys.push(format!("keys.{row}"));
                    keys.push(format!("keys.{row}_note"));
                }
            }
        }
        for key in keys {
            assert_ne!(text.get(&key), key, "missing");
        }
    }

    #[test]
    fn the_layout_has_each_key_once_inside_the_diagram() {
        let keys = keyboard();
        for (index, key) in keys.iter().enumerate() {
            assert!(
                keys[..index].iter().all(|other| other.code != key.code),
                "{:?} twice",
                key.cap
            );
            assert!(key.x + key.w <= DIAGRAM_W && key.y + key.h <= DIAGRAM_H);
            // A reserved key never parses back, being unbindable.
            let source = Source::key(key.code);
            assert!(
                source.reserved_bit().is_some() || source.round_trips(),
                "{:?}",
                key.cap
            );
        }
        const { assert!(MOUSE_X + MOUSE_W <= DIAGRAM_W) };
        const { assert!(LOWER_Y + MOUSE_H <= DIAGRAM_H) };
    }

    #[test]
    fn a_captured_key_replaces_the_cell_and_moves_from_its_old_action() {
        let mut screen = screen();
        // Move Forward's first key: Q, which was Quick Turn's.
        screen.handle(key(GuiKey::Confirm));
        assert!(screen.capturing());
        let bindings = changed(screen.capture(Source::key(SDL_SCANCODE_Q), None));
        assert!(!screen.capturing());
        assert_eq!(
            keys(&bindings, Action::Up),
            [Source::key(SDL_SCANCODE_Q), Source::key(SDL_SCANCODE_UP)]
        );
        assert!(bindings.get(Action::QuickTurn).is_empty());
        // The gamepad sources stay.
        let pads = |b: &Bindings| b.get(Action::Up).iter().filter(|s| s.is_pad()).count();
        assert_eq!(pads(&bindings), pads(&Bindings::default()));
        assert!(screen.note.is_some(), "the move is reported");
    }

    #[test]
    fn the_second_column_takes_a_mouse_button() {
        let mut screen = screen();
        screen.handle(key(GuiKey::Right));
        screen.handle(key(GuiKey::Confirm));
        let bindings = changed(screen.capture(Source::Mouse(3), None));
        assert_eq!(
            keys(&bindings, Action::Up),
            [Source::key(SDL_SCANCODE_W), Source::Mouse(3)]
        );
    }

    #[test]
    fn escape_and_the_gamepad_cancel_a_capture() {
        let mut screen = screen();
        for source in [Source::key(SDL_SCANCODE_ESCAPE), Source::PadButton(0)] {
            screen.handle(key(GuiKey::Confirm));
            assert!(screen.capture(source, None).is_empty());
            assert!(!screen.capturing());
        }
        assert_eq!(screen.bindings, Bindings::default());
    }

    #[test]
    fn events_wait_while_capturing() {
        let mut screen = screen();
        screen.handle(key(GuiKey::Confirm));
        assert!(screen.handle(key(GuiKey::Cancel)).is_empty());
        assert!(screen.capturing());
    }

    #[test]
    fn an_essential_action_keeps_a_key() {
        let mut screen = screen();
        // Menu tab, Cancel: clear Escape, then Backspace is refused.
        screen.handle(key(GuiKey::TabNext));
        for _ in 0..5 {
            screen.handle(key(GuiKey::Down));
        }
        assert_eq!(screen.selected().1, Action::Cancel);
        changed(screen.handle(key(GuiKey::ResetAll)));
        assert!(screen.handle(key(GuiKey::ResetAll)).is_empty());
        assert_eq!(
            keys(&screen.bindings, Action::Cancel),
            [Source::key(SDL_SCANCODE_BACKSPACE)]
        );
        assert!(screen.note.is_some());
    }

    #[test]
    fn reset_restores_the_keys_and_keeps_the_gamepad() {
        let mut bindings = Bindings::default();
        bindings.set(Action::Fire, vec![Source::Mouse(2), Source::PadButton(0)]);
        let mut screen = KeyConfigScreen::new(bindings, Locale::default());
        screen.handle(key(GuiKey::ResetGroup));
        // The dialog defaults to No.
        screen.handle(key(GuiKey::Left));
        let reset = changed(screen.handle(key(GuiKey::Confirm)));
        assert_eq!(
            keys(&reset, Action::Fire),
            [
                Source::key(SDL_SCANCODE_F),
                Source::key(SDL_SCANCODE_RETURN),
                Source::Mouse(1)
            ]
        );
        assert!(reset.get(Action::Fire).contains(&Source::PadButton(0)));
    }

    #[test]
    fn clicking_a_cell_starts_a_capture() {
        let mut screen = screen();
        let mut canvas = Canvas::new(1920, 1080);
        let mut fonts = Fonts::new().unwrap();
        screen.draw(&mut Painter {
            canvas: &mut canvas,
            fonts: &mut fonts,
            theme: Theme::new(1920, 1080, FontFamily::Modern),
        });
        let (rect, _) = *screen
            .hits
            .iter()
            .find(|(_, hit)| *hit == Hit::Cell(2, 1))
            .unwrap();
        screen.handle(GuiEvent::PointerDown {
            x: rect.center_x(),
            y: rect.center_y(),
        });
        assert!(screen.capturing());
        assert_eq!((screen.row, screen.column), (2, 1));
    }

    #[test]
    fn clicking_the_cancel_hint_cancels_a_capture() {
        let mut screen = screen();
        let mut canvas = Canvas::new(1920, 1080);
        let mut fonts = Fonts::new().unwrap();
        screen.handle(key(GuiKey::Confirm));
        screen.draw(&mut Painter {
            canvas: &mut canvas,
            fonts: &mut fonts,
            theme: Theme::new(1920, 1080, FontFamily::Modern),
        });
        let (rect, _) = *screen
            .hits
            .iter()
            .find(|(_, hit)| *hit == Hit::Key(GuiKey::Cancel))
            .unwrap();
        let click = Some((rect.center_x(), rect.center_y()));
        assert!(screen.capture(Source::Mouse(1), click).is_empty());
        assert!(!screen.capturing());
        // Another button pressed over it binds.
        screen.handle(key(GuiKey::Right));
        screen.handle(key(GuiKey::Confirm));
        changed(screen.capture(Source::Mouse(3), click));
    }
}
