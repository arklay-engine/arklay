//! The retained UI: screens written in markup, built into a node tree,
//! laid out, painted and driven by one runtime.
//!
//! A screen is an XML document ([`markup`]) of widgets: stacks, text, lists,
//! tab strips, option rows, items, a hint bar. [`Ui`] keeps a stack of open
//! screens (dialogs are modal screens on top), evaluates every node's
//! attribute bindings against the shared [`Store`], lays the tree out
//! ([`layout`]), routes input through it and paints it ([`paint`]).
//!
//! * **Bindings.** An attribute is a literal, a `{path}` into the store (or
//!   `item.*` inside a repeated template, `focus.*` for the focused node), or
//!   an `@key` GUI string. A host publishes data with [`Ui::set`]; a widget
//!   that changes a bound value writes it back and reports
//!   [`UiOutput::Changed`].
//! * **Input.** A key goes to the focused node, then up through its
//!   ancestors: each node's `<command>`s first, then the widget's own
//!   handling (a list moves the focus, a row steps its value, a tab strip
//!   switches on the key pair its nesting depth gives it).
//! * **Hints.** The hint bar shows the commands on the focus path, so a key
//!   and its hint are one declaration and change with the context.
//! * **Actions.** `back`, `open:<screen>` and `reset` are built in; a
//!   `confirm` asks first in a dialog; anything else goes to the screen's
//!   Lua script or comes back to the host as [`UiOutput::Action`].

pub mod layout;
pub mod lua;
pub mod markup;
pub mod paint;
pub mod style;
pub mod tree;
pub mod value;

use std::collections::HashMap;

use anyhow::{Result, anyhow, bail};

use crate::gui::canvas::{Canvas, Rect};
use crate::gui::input::{Device, GuiEvent, GuiKey, Sound};
use crate::gui::locale::Locale;
use crate::gui::text::Fonts;
use crate::gui::theme::{FontFamily, Theme, palette};
use crate::gui::widgets::Painter;

pub use markup::{Element, Expr, Library};
pub use tree::{Kind, Node, NodeId, OptionKind, Tree};
pub use value::{ImageValue, Store, Value};

use layout::LayoutCx;
use paint::PaintCx;
use style::Ink;
use tree::{Builder, Part};

/// A widget host code paints: the pad diagram, anything a screen needs that
/// the built-in widgets cannot draw.
pub trait Custom {
    fn paint(&self, p: &mut Painter<'_>, rect: Rect, node: &Node);
}

/// What the UI asks of its host.
#[derive(Debug, Clone, PartialEq)]
pub enum UiOutput {
    /// An action no built-in or script handled.
    Action {
        screen: String,
        name: String,
        arg: Value,
    },
    /// A widget wrote `value` to the store at `path`.
    Changed { path: String, value: Value },
    /// A screen opened from an action.
    Opened(String),
    /// A screen closed (backed out of, or its script failed).
    Closed(String),
    /// Play a feedback sound.
    Sound(Sound),
}

impl UiOutput {
    /// The action's name, when this is an action.
    pub fn action(&self) -> Option<(&str, &Value)> {
        match self {
            UiOutput::Action { name, arg, .. } => Some((name, arg)),
            _ => None,
        }
    }
}

/// One open screen.
struct Screen {
    uid: u64,
    name: String,
    tree: Tree,
    root: NodeId,
    focus: Option<NodeId>,
    modal: bool,
    /// A dialog's question: the screen and node that asked, and the action
    /// Yes carries out.
    pending: Option<(u64, NodeId, String)>,
    /// The slider the pointer drags.
    capture: Option<NodeId>,
    script: Option<lua::Script>,
}

/// The screen stack and everything it draws with.
pub struct Ui {
    library: Library,
    store: Store,
    locale: Locale,
    screens: Vec<Screen>,
    customs: HashMap<String, Box<dyn Custom>>,
    device: Device,
    fonts: Fonts,
    canvas: Canvas,
    family: FontFamily,
    text_scale: f32,
    /// The size the screens are laid out at.
    size: (u32, u32),
    outputs: Vec<UiOutput>,
    dirty: bool,
    next_uid: u64,
}

impl Ui {
    pub fn new(library: Library) -> Result<Self> {
        Ok(Self {
            library,
            store: Store::default(),
            locale: Locale::default(),
            screens: Vec::new(),
            customs: HashMap::new(),
            device: Device::Keyboard,
            fonts: Fonts::new()?,
            canvas: Canvas::new(1, 1),
            family: FontFamily::Modern,
            text_scale: 1.0,
            size: (1920, 1080),
            outputs: Vec::new(),
            dirty: true,
            next_uid: 1,
        })
    }

    pub fn library_mut(&mut self) -> &mut Library {
        &mut self.library
    }

    /// The store value at `path`.
    pub fn get(&self, path: &str) -> Value {
        self.store.get(path)
    }

    /// Publish `value` at `path`.
    pub fn set(&mut self, path: &str, value: impl Into<Value>) {
        if self.store.set(path, value.into()) {
            self.dirty = true;
        }
    }

    pub fn locale(&self) -> &Locale {
        &self.locale
    }

    pub fn set_locale(&mut self, locale: Locale) {
        if locale.tag() != self.locale.tag() {
            self.locale = locale;
            self.dirty = true;
        }
    }

    /// Draw text in `family` at `text_scale` times its size.
    pub fn set_theme(&mut self, family: FontFamily, text_scale: f32) {
        if (family, text_scale) != (self.family, self.text_scale) {
            self.family = family;
            self.text_scale = text_scale;
            self.fonts.clear_cache();
            self.dirty = true;
        }
    }

    /// Show hints for `device`.
    pub fn set_device(&mut self, device: Device) {
        if device != self.device {
            self.device = device;
            self.dirty = true;
        }
    }

    pub fn device(&self) -> Device {
        self.device
    }

    /// Paint `<name>` elements with `custom`.
    pub fn register(&mut self, name: &str, custom: Box<dyn Custom>) {
        self.customs.insert(name.to_string(), custom);
        self.dirty = true;
    }

    /// Repaint on the next draw.
    pub fn invalidate(&mut self) {
        self.dirty = true;
    }

    /// The open screens' names, bottom first.
    pub fn screens(&self) -> Vec<&str> {
        self.screens.iter().map(|s| s.name.as_str()).collect()
    }

    /// The top screen's name.
    pub fn top(&self) -> Option<&str> {
        self.screens.last().map(|s| s.name.as_str())
    }

    pub fn is_empty(&self) -> bool {
        self.screens.is_empty()
    }

    pub fn canvas(&self) -> &Canvas {
        &self.canvas
    }

    /// Record the canvas for the GPU (or paint it) from the next draw.
    pub fn set_recording(&mut self, on: bool) {
        self.dirty |= self.canvas.set_recording(on);
    }

    /// Open the screen `name` on top.
    pub fn open(&mut self, name: &str) -> Result<()> {
        let Some(el) = self.library.screen(name).cloned() else {
            bail!("no screen named {name:?}");
        };
        let mut tree = Tree::default();
        let customs: Vec<String> = self.customs.keys().cloned().collect();
        let is_custom = |n: &str| customs.iter().any(|c| c == n);
        let builder = Builder {
            library: &self.library,
            customs: &is_custom,
        };
        let root = builder
            .build(&mut tree, &el, None)
            .map_err(|err| anyhow!("{name}: {err:#}"))?;
        tree.root = Some(root);
        let uid = self.next_uid;
        self.next_uid += 1;
        let modal = tree[root].variant_attr() == Some("modal");
        let script = match tree[root].attr("script") {
            Some(Expr::Lit(path)) => {
                let Some(source) = self.library.scripts.get(&**path).cloned() else {
                    bail!("{name}: no script {path:?}");
                };
                Some(lua::Script::load(&format!("{path}"), &source)?)
            }
            _ => None,
        };
        let has_script = script.is_some();
        self.screens.push(Screen {
            uid,
            name: name.to_string(),
            tree,
            root,
            focus: None,
            modal,
            pending: None,
            capture: None,
            script,
        });
        self.dirty = true;
        self.refresh();
        if has_script {
            lua::call(self, uid, "open", Value::Nil);
            self.refresh();
        }
        Ok(())
    }

    /// Close the top screen.
    pub fn close(&mut self) {
        if let Some(screen) = self.screens.pop() {
            self.dirty = true;
            self.outputs.push(UiOutput::Closed(screen.name));
        }
    }

    /// Close every screen.
    pub fn clear(&mut self) {
        self.screens.clear();
        self.dirty = true;
    }

    fn index_of(&self, uid: u64) -> Option<usize> {
        self.screens.iter().position(|s| s.uid == uid)
    }

    /// The bottom-most screen drawn: the top non-modal one.
    fn base(&self) -> usize {
        self.screens.iter().rposition(|s| !s.modal).unwrap_or(0)
    }

    fn theme(&self) -> Theme {
        Theme::new(self.size.0, self.size.1, self.family).with_text_scale(self.text_scale)
    }

    // ---- bindings and layout ------------------------------------------

    /// Evaluate every binding of the drawn screens and lay them out.
    fn refresh(&mut self) {
        if self.screens.is_empty() {
            return;
        }
        for s in self.base()..self.screens.len() {
            self.evaluate(s, false);
            self.lay_out(s);
            if self.ensure_focus(s) {
                self.evaluate(s, false);
                self.lay_out(s);
            }
        }
    }

    /// Evaluate screen `s`'s bindings: the shown tabs only, or `all`.
    fn evaluate(&mut self, s: usize, all: bool) {
        let Ui {
            library,
            store,
            locale,
            screens,
            customs,
            ..
        } = self;
        let screen = &mut screens[s];
        let names: Vec<String> = customs.keys().cloned().collect();
        let is_custom = |n: &str| names.iter().any(|c| c == n);
        let builder = Builder {
            library,
            customs: &is_custom,
        };
        let text = |key: &str| locale.get(key).to_string();
        let mut stack = vec![screen.root];
        let mut focus_readers = Vec::new();
        while let Some(id) = stack.pop() {
            let tree = &mut screen.tree;
            if !tree.contains(id) {
                continue;
            }
            eval_node(tree, id, store, &text, screen.focus);
            if tree[id].reads_focus {
                focus_readers.push(id);
            }
            rebuild_template(tree, id, &builder);
            if tree[id].kind == Kind::Tabs && !all {
                // Only the shown tab's content is kept current; the others
                // need just their labels until they are switched to.
                let tabs: Vec<NodeId> = tree[id]
                    .children
                    .iter()
                    .copied()
                    .filter(|&c| tree[c].kind == Kind::Tab)
                    .collect();
                for &tab in &tabs {
                    eval_node(tree, tab, store, &text, screen.focus);
                }
                let shown = layout::tabs_of(tree, id);
                let selected = shown.get(tree[id].index.min(shown.len().saturating_sub(1)));
                for &c in tree[id].children.iter().rev() {
                    if !tabs.contains(&c) || Some(&c) == selected {
                        stack.push(c);
                    }
                }
                continue;
            }
            stack.extend(tree[id].children.iter().rev());
        }
        for id in focus_readers {
            eval_node(&mut screen.tree, id, store, &text, screen.focus);
        }
        // Tab strips know how deeply they nest.
        let tree = &mut screen.tree;
        for id in tree.descendants(screen.root) {
            if tree[id].kind == Kind::Tabs {
                let depth = tree
                    .ancestry(id)
                    .iter()
                    .skip(1)
                    .filter(|&&a| tree[a].kind == Kind::Tabs)
                    .count();
                tree[id].depth = depth as u8;
            }
        }
    }

    fn lay_out(&mut self, s: usize) {
        let theme = self.theme();
        let screen = &mut self.screens[s];
        let mut cx = LayoutCx {
            fonts: &mut self.fonts,
            theme,
            focus: screen.focus,
        };
        let bounds = Rect::new(0.0, 0.0, theme.width, theme.height);
        layout::layout(&mut screen.tree, screen.root, &mut cx, bounds);
    }

    /// Put the focus somewhere shown; returns whether it moved.
    fn ensure_focus(&mut self, s: usize) -> bool {
        let screen = &mut self.screens[s];
        let tree = &screen.tree;
        if let Some(focus) = screen.focus
            && focusable(tree, focus)
        {
            return false;
        }
        let shown = tree.descendants(screen.root);
        // A tab remembers where it was left.
        let remembered = shown
            .iter()
            .filter(|&&id| tree[id].kind == Kind::Tab && tree[id].shown)
            .filter_map(|&id| tree[id].last_focus)
            .find(|&f| focusable(tree, f));
        let default = shown
            .iter()
            .copied()
            .find(|&id| focusable(tree, id) && tree[id].prop("autofocus").truthy());
        let first = shown.iter().copied().find(|&id| focusable(tree, id));
        let next = remembered.or(default).or(first);
        let moved = next != screen.focus;
        screen.focus = next;
        moved
    }

    // ---- input --------------------------------------------------------

    /// Feed one presented frame's events; returns what the host must do.
    pub fn handle(&mut self, events: &[GuiEvent]) -> Vec<UiOutput> {
        self.refresh();
        for &event in events {
            if self.screens.is_empty() {
                break;
            }
            if !matches!(event, GuiEvent::PointerMove { .. }) {
                self.dirty = true;
            }
            match event {
                GuiEvent::Key(key, device) => {
                    self.device = device;
                    self.key(key);
                }
                GuiEvent::PointerMove { x, y } => self.pointer_move(x, y),
                GuiEvent::PointerDown { x, y } => self.pointer_down(x, y),
                GuiEvent::PointerUp { .. } => {
                    if let Some(screen) = self.screens.last_mut() {
                        screen.capture = None;
                    }
                }
                GuiEvent::Wheel(notches) if notches != 0.0 => {
                    self.key(if notches > 0.0 {
                        GuiKey::Up
                    } else {
                        GuiKey::Down
                    });
                }
                GuiEvent::Wheel(_) => {}
            }
            self.refresh();
        }
        std::mem::take(&mut self.outputs)
    }

    /// Press `key` on the top screen.
    pub fn key(&mut self, key: GuiKey) {
        let Some(s) = self.screens.len().checked_sub(1) else {
            return;
        };
        let path = self.focus_path(s);
        for id in path {
            if !self.screens[s].tree.contains(id) {
                return;
            }
            let command = self.screens[s].tree[id]
                .commands
                .iter()
                .rev()
                .find(|c| c.key == key)
                .cloned();
            if let Some(command) = command {
                let uid = self.screens[s].uid;
                let screen = &self.screens[s];
                let eval = |expr: &Expr| eval_in(&screen.tree, id, expr, &self.store, &self.locale);
                let action = eval(&command.action).to_string();
                let ask = command.confirm_if.as_ref().is_none_or(|e| eval(e).truthy());
                let confirm = command.confirm.as_ref().filter(|_| ask).map(|e| {
                    (
                        eval(e).to_string(),
                        command.note.as_ref().map(|n| eval(n).to_string()),
                    )
                });
                self.run(uid, id, &action, confirm);
                return;
            }
            if self.widget_key(s, id, key) {
                return;
            }
        }
    }

    /// Where a key starts: the focused node, or the deepest shown tab when
    /// nothing is focused; then up to the root.
    fn focus_path(&self, s: usize) -> Vec<NodeId> {
        let screen = &self.screens[s];
        let tree = &screen.tree;
        let start = screen.focus.filter(|&f| tree.contains(f)).or_else(|| {
            tree.descendants(screen.root)
                .into_iter()
                .rev()
                .find(|&id| tree[id].kind == Kind::Tab && tree[id].shown)
        });
        tree.ancestry(start.unwrap_or(screen.root))
    }

    /// A widget's own handling of `key`; returns whether it took it.
    fn widget_key(&mut self, s: usize, id: NodeId, key: GuiKey) -> bool {
        let kind = self.screens[s].tree[id].kind;
        match kind {
            Kind::Option(option) => self.option_key(s, id, option, key),
            Kind::Item => {
                if key != GuiKey::Confirm {
                    return false;
                }
                let screen = &self.screens[s];
                let node = &screen.tree[id];
                let action = node.text("action");
                if action.is_empty() {
                    return false;
                }
                let confirm = confirm_of(node);
                let uid = screen.uid;
                self.run(uid, id, &action, confirm);
                true
            }
            Kind::List => match key {
                GuiKey::Up => self.move_focus(s, id, -1),
                GuiKey::Down => self.move_focus(s, id, 1),
                GuiKey::Left | GuiKey::Right => {
                    let node = &self.screens[s].tree[id];
                    if node.content <= node.rect.h + 0.5 {
                        return false;
                    }
                    // A list taller than its view pages through.
                    let rows = self.focusables(s, id).len().max(1);
                    let row_h = node.content / rows as f32;
                    let page = ((node.rect.h / row_h.max(1.0)).floor() as isize).max(1);
                    let step = if key == GuiKey::Left { -page } else { page };
                    self.move_focus_clamped(s, id, step)
                }
                _ => false,
            },
            Kind::Tabs => {
                let depth = self.screens[s].tree[id].depth;
                let (prev, next) = if depth == 0 {
                    (GuiKey::TabPrev, GuiKey::TabNext)
                } else {
                    (GuiKey::SubPrev, GuiKey::SubNext)
                };
                if key == prev {
                    self.step_tab(s, id, -1)
                } else if key == next {
                    self.step_tab(s, id, 1)
                } else {
                    false
                }
            }
            Kind::Dialog => match key {
                GuiKey::Up | GuiKey::Left => self.move_focus(s, id, -1),
                GuiKey::Down | GuiKey::Right => self.move_focus(s, id, 1),
                GuiKey::Cancel => {
                    let uid = self.screens[s].uid;
                    self.run(uid, id, "dialog:no", None);
                    true
                }
                _ => false,
            },
            _ => false,
        }
    }

    fn option_key(&mut self, s: usize, id: NodeId, option: OptionKind, key: GuiKey) -> bool {
        let screen = &self.screens[s];
        let node = &screen.tree[id];
        if !matches!(key, GuiKey::Left | GuiKey::Right | GuiKey::Confirm) {
            return false;
        }
        if node.prop("disabled").truthy() {
            return true;
        }
        let value = node.prop("value").as_int().unwrap_or(0);
        let next = match option {
            OptionKind::Info => return false,
            OptionKind::Link => {
                if key != GuiKey::Confirm {
                    return false;
                }
                let action = node.text("action");
                let confirm = confirm_of(node);
                let uid = screen.uid;
                self.run(uid, id, &action, confirm);
                return true;
            }
            OptionKind::Toggle | OptionKind::Choice => {
                let count = node.prop("choices").as_list().len().max(1) as i64;
                match key {
                    GuiKey::Left => (value - 1).max(0),
                    GuiKey::Right => (value + 1).min(count - 1),
                    _ => (value + 1) % count,
                }
            }
            OptionKind::Slider => {
                let min = node.prop("min").as_int().unwrap_or(0);
                let max = node.prop("max").as_int().unwrap_or(100);
                let step = node.prop("step").as_int().unwrap_or(1).max(1);
                let delta = if key == GuiKey::Left { -step } else { step };
                (value + delta).clamp(min, max)
            }
        };
        if next != value {
            self.write_value(s, id, next);
            self.outputs.push(UiOutput::Sound(Sound::Cursor));
        }
        true
    }

    /// Write an option row's value to the store path it is bound to.
    fn write_value(&mut self, s: usize, id: NodeId, value: i64) {
        let tree = &self.screens[s].tree;
        let Some(path) = value_path(tree, id) else {
            return;
        };
        let value = match self.store.get(&path) {
            Value::Bool(_) => Value::Bool(value != 0),
            _ => Value::Int(value),
        };
        if self.store.set(&path, value.clone()) {
            self.screens[s].tree[id]
                .props
                .insert("value".into(), value.clone());
            self.dirty = true;
            self.outputs.push(UiOutput::Changed { path, value });
        }
    }

    /// The focusable, shown nodes under `id`, in order.
    fn focusables(&self, s: usize, id: NodeId) -> Vec<NodeId> {
        let tree = &self.screens[s].tree;
        tree.descendants(id)
            .into_iter()
            .filter(|&n| focusable(tree, n))
            .collect()
    }

    /// Move the focus `step` places among `container`'s focusables,
    /// wrapping.
    fn move_focus(&mut self, s: usize, container: NodeId, step: isize) -> bool {
        let items = self.focusables(s, container);
        if items.is_empty() {
            return false;
        }
        let count = items.len() as isize;
        let at = self.screens[s]
            .focus
            .and_then(|f| items.iter().position(|&i| i == f));
        let next = match at {
            Some(at) => (at as isize + step).rem_euclid(count) as usize,
            None => 0,
        };
        self.focus(s, items[next], true)
    }

    /// Move the focus `step` places, stopping at the ends.
    fn move_focus_clamped(&mut self, s: usize, container: NodeId, step: isize) -> bool {
        let items = self.focusables(s, container);
        let at = self.screens[s]
            .focus
            .and_then(|f| items.iter().position(|&i| i == f))
            .unwrap_or(0);
        let next = (at as isize + step).clamp(0, items.len() as isize - 1) as usize;
        match items.get(next) {
            Some(&id) => self.focus(s, id, true),
            None => false,
        }
    }

    /// Focus `id`; returns whether the focus moved.
    fn focus(&mut self, s: usize, id: NodeId, sound: bool) -> bool {
        let screen = &mut self.screens[s];
        if screen.focus == Some(id) {
            return false;
        }
        screen.focus = Some(id);
        self.dirty = true;
        if sound {
            self.outputs.push(UiOutput::Sound(Sound::Cursor));
        }
        true
    }

    fn step_tab(&mut self, s: usize, tabs: NodeId, step: isize) -> bool {
        let count = layout::tabs_of(&self.screens[s].tree, tabs).len();
        if count < 2 {
            return false;
        }
        let index = self.screens[s].tree[tabs].index as isize;
        let next = (index + step).rem_euclid(count as isize) as usize;
        self.select_tab(s, tabs, next)
    }

    fn select_tab(&mut self, s: usize, tabs: NodeId, index: usize) -> bool {
        let screen = &mut self.screens[s];
        let tree = &mut screen.tree;
        let list = layout::tabs_of(tree, tabs);
        if index >= list.len() || index == tree[tabs].index {
            return false;
        }
        // Leave the focus with the tab it was in.
        if let Some(focus) = screen.focus
            && let Some(&old) = list.get(tree[tabs].index)
            && tree.ancestry(focus).contains(&old)
        {
            tree[old].last_focus = Some(focus);
        }
        tree[tabs].index = index;
        screen.focus = None;
        screen.capture = None;
        self.dirty = true;
        self.outputs.push(UiOutput::Sound(Sound::Cursor));
        true
    }

    // ---- pointer ------------------------------------------------------

    /// The nodes under `(x, y)` on the top screen, innermost first.
    fn hit(&self, x: f32, y: f32) -> Vec<NodeId> {
        let Some(screen) = self.screens.last() else {
            return Vec::new();
        };
        let tree = &screen.tree;
        let mut path = Vec::new();
        let mut at = screen.root;
        loop {
            path.push(at);
            let node = &tree[at];
            let clip = node.kind == Kind::List;
            let next = node.children.iter().rev().copied().find(|&c| {
                let child = &tree[c];
                child.shown && child.rect.contains(x, y) && (!clip || node.rect.contains(x, y))
            });
            match next {
                Some(c) => at = c,
                None => break,
            }
        }
        path.reverse();
        path
    }

    fn pointer_move(&mut self, x: f32, y: f32) {
        let Some(s) = self.screens.len().checked_sub(1) else {
            return;
        };
        if let Some(row) = self.screens[s].capture {
            self.slide(s, row, x);
            return;
        }
        let tree = &self.screens[s].tree;
        if let Some(&id) = self.hit(x, y).iter().find(|&&id| focusable(tree, id))
            && self.focus(s, id, false)
        {
            self.dirty = true;
        }
    }

    fn pointer_down(&mut self, x: f32, y: f32) {
        let Some(s) = self.screens.len().checked_sub(1) else {
            return;
        };
        let path = self.hit(x, y);
        for &id in &path {
            let part = self.screens[s].tree[id]
                .parts
                .iter()
                .rev()
                .find(|(r, _)| r.contains(x, y))
                .map(|&(_, part)| part);
            let Some(part) = part else {
                continue;
            };
            match part {
                Part::Key(key) => self.key(key),
                Part::Tab(index) => {
                    self.select_tab(s, id, index);
                }
                Part::TabPrev => {
                    self.step_tab(s, id, -1);
                }
                Part::TabNext => {
                    self.step_tab(s, id, 1);
                }
                Part::Decrease | Part::Increase => {
                    self.focus(s, id, false);
                    let key = if part == Part::Decrease {
                        GuiKey::Left
                    } else {
                        GuiKey::Right
                    };
                    self.widget_key(s, id, key);
                }
                Part::Track(..) => {
                    self.focus(s, id, false);
                    self.screens[s].capture = Some(id);
                    self.slide(s, id, x);
                }
            }
            return;
        }
        let tree = &self.screens[s].tree;
        if let Some(&id) = path.iter().find(|&&id| focusable(tree, id)) {
            self.focus(s, id, false);
            self.key(GuiKey::Confirm);
        }
    }

    /// Set a dragged slider to the step nearest `x`.
    fn slide(&mut self, s: usize, id: NodeId, x: f32) {
        let tree = &self.screens[s].tree;
        let Some(node) = tree.get(id) else {
            return;
        };
        let Some(&(_, Part::Track(left, right))) = node
            .parts
            .iter()
            .find(|(_, p)| matches!(p, Part::Track(..)))
        else {
            return;
        };
        if node.prop("disabled").truthy() {
            return;
        }
        let min = node.prop("min").as_int().unwrap_or(0);
        let max = node.prop("max").as_int().unwrap_or(100);
        let step = node.prop("step").as_int().unwrap_or(1).max(1);
        let fraction = ((x - left) / (right - left).max(1.0)).clamp(0.0, 1.0);
        let steps = ((max - min) as f32 / step as f32 * fraction).round() as i64;
        let value = min + steps * step;
        if node.prop("value").as_int() != Some(value) {
            self.write_value(s, id, value);
        }
    }

    // ---- actions ------------------------------------------------------

    /// Carry out `action` for node `owner` on screen `uid`, asking
    /// `confirm` first.
    fn run(
        &mut self,
        uid: u64,
        owner: NodeId,
        action: &str,
        confirm: Option<(String, Option<String>)>,
    ) {
        if let Some((title, note)) = confirm {
            self.store.set("dialog.title", Value::from(title));
            self.store
                .set("dialog.note", Value::from(note.unwrap_or_default()));
            match self.open("dialog") {
                Ok(()) => {
                    let top = self.screens.last_mut().unwrap();
                    top.pending = Some((uid, owner, action.to_string()));
                    self.outputs.push(UiOutput::Sound(Sound::Decide));
                }
                Err(err) => eprintln!("warning: ui: {err:#}"),
            }
            return;
        }
        self.execute(uid, owner, action);
    }

    fn execute(&mut self, uid: u64, owner: NodeId, action: &str) {
        let Some(s) = self.index_of(uid) else {
            return;
        };
        let (name, arg) = match action.split_once(':') {
            Some((name, arg)) => (name.trim(), Some(arg.trim())),
            None => (action.trim(), None),
        };
        match name {
            "" => {}
            "back" => {
                self.screens.truncate(s + 1);
                self.close();
                self.outputs.push(UiOutput::Sound(Sound::Cancel));
            }
            "open" => {
                let target = arg.unwrap_or_default();
                match self.open(target) {
                    Ok(()) => {
                        self.outputs.push(UiOutput::Opened(target.to_string()));
                        self.outputs.push(UiOutput::Sound(Sound::Decide));
                    }
                    Err(err) => eprintln!("warning: ui: {err:#}"),
                }
            }
            "reset" => {
                self.reset(s, owner);
                self.outputs.push(UiOutput::Sound(Sound::Decide));
            }
            "dialog" if self.screens[s].modal => {
                let pending = self.screens[s].pending.take();
                self.screens.truncate(s);
                self.dirty = true;
                match (arg, pending) {
                    (Some("yes"), Some((uid, owner, action))) => self.execute(uid, owner, &action),
                    _ => self.outputs.push(UiOutput::Sound(Sound::Cancel)),
                }
            }
            _ => {
                let arg = match arg {
                    None => Value::Nil,
                    Some(text) => text
                        .parse::<i64>()
                        .map_or_else(|_| Value::from(text), Value::Int),
                };
                if self.screens[s].script.is_some()
                    && lua::call(self, uid, &format!("action:{name}"), arg.clone())
                {
                    return;
                }
                self.outputs.push(UiOutput::Sound(Sound::Decide));
                self.outputs.push(UiOutput::Action {
                    screen: self.screens[s].name.clone(),
                    name: name.to_string(),
                    arg,
                });
            }
        }
        self.dirty = true;
    }

    /// Reset the option rows under `owner` (only its shown tab, when it is
    /// a tab strip) to their defaults.
    fn reset(&mut self, s: usize, owner: NodeId) {
        self.evaluate(s, true);
        let tree = &self.screens[s].tree;
        let scope = if tree[owner].kind == Kind::Tabs {
            let index = tree[owner].index;
            match layout::tabs_of(tree, owner).get(index) {
                Some(&tab) => tab,
                None => return,
            }
        } else {
            owner
        };
        let rows: Vec<(NodeId, i64)> = tree
            .descendants(scope)
            .into_iter()
            .filter(|&id| {
                matches!(
                    tree[id].kind,
                    Kind::Option(OptionKind::Toggle | OptionKind::Choice | OptionKind::Slider)
                ) && !tree[id].prop("disabled").truthy()
                    && layout::present(tree, id)
            })
            .filter_map(|id| Some((id, tree[id].prop("default").as_int()?)))
            .collect();
        for (id, default) in rows {
            self.write_value(s, id, default);
        }
    }

    // ---- hints and drawing --------------------------------------------

    /// The hints the focus path offers, in the canonical key order.
    fn hints(&self, s: usize) -> Vec<(GuiKey, String)> {
        let screen = &self.screens[s];
        let tree = &screen.tree;
        let mut out: Vec<(GuiKey, String)> = Vec::new();
        let mut add = |key: GuiKey, label: String| {
            if !label.is_empty() && !out.iter().any(|(k, _)| *k == key) {
                out.push((key, label));
            }
        };
        let path = self.focus_path(s);
        if let Some(&focus) = path.first()
            && screen.focus == Some(focus)
        {
            let node = &tree[focus];
            match node.kind {
                Kind::Item if !node.text("action").is_empty() => {
                    let hint = node.text("hint");
                    add(
                        GuiKey::Confirm,
                        if hint.is_empty() {
                            self.locale.get("hint.select").to_string()
                        } else {
                            hint
                        },
                    );
                }
                Kind::Option(OptionKind::Link) if !node.prop("disabled").truthy() => {
                    add(GuiKey::Confirm, self.locale.get("hint.select").to_string());
                }
                Kind::Option(OptionKind::Toggle | OptionKind::Choice | OptionKind::Slider)
                    if !node.prop("disabled").truthy() =>
                {
                    add(GuiKey::Left, self.locale.get("hint.change").to_string());
                }
                _ => {}
            }
        }
        for &id in &path {
            for command in tree[id].commands.iter().rev().filter(|c| c.hint) {
                let label = eval_in(tree, id, &command.label, &self.store, &self.locale);
                add(command.key, label.to_string());
            }
        }
        let order = |key: GuiKey| match key {
            GuiKey::Confirm => 0,
            GuiKey::Left | GuiKey::Right => 1,
            GuiKey::Up | GuiKey::Down => 2,
            GuiKey::TabPrev | GuiKey::TabNext | GuiKey::SubPrev | GuiKey::SubNext => 3,
            GuiKey::ResetAll => 4,
            GuiKey::ResetGroup => 5,
            GuiKey::Cancel => 6,
        };
        out.sort_by_key(|(key, _)| order(*key));
        out
    }

    /// Paint the screens at `width` x `height` into [`Self::canvas`];
    /// returns whether anything was repainted.
    pub fn draw(&mut self, width: u32, height: u32) -> bool {
        let (width, height) = (width.max(1), height.max(1));
        let resized = (self.canvas.width, self.canvas.height) != (width, height);
        if resized {
            self.fonts.clear_cache();
            self.size = (width, height);
            self.dirty = true;
        }
        if !self.dirty {
            return false;
        }
        self.dirty = false;
        self.refresh();
        if self.screens.is_empty() {
            self.canvas.reset(width, height);
            return true;
        }
        let base = self.base();
        // A screen over the game can let more of it through.
        let shade = match self.screens[base].tree[self.screens[base].root].attr("backdrop") {
            Some(Expr::Lit(b)) if &**b == "light" => palette::MENU_SHADE,
            _ => palette::SCREEN_SHADE,
        };
        self.canvas.reset_to(width, height, shade);
        let theme = self.theme();
        let crumbs: Vec<String> = self.screens[..base]
            .iter()
            .filter(|s| !s.modal)
            .map(|s| s.tree[s.root].text("title"))
            .filter(|t| !t.is_empty())
            .collect();
        let hints = self.hints(base);
        for s in base..self.screens.len() {
            let cx = PaintCx {
                device: self.device,
                focus: self.screens[s].focus,
                hints: if s == base { hints.clone() } else { Vec::new() },
                crumbs: if s == base {
                    crumbs.clone()
                } else {
                    Vec::new()
                },
                customs: &self.customs,
            };
            let mut p = Painter {
                canvas: &mut self.canvas,
                fonts: &mut self.fonts,
                theme,
            };
            let screen = &mut self.screens[s];
            paint::paint(&mut screen.tree, screen.root, &mut p, &cx, Ink::NORMAL);
        }
        true
    }

    // ---- inspection (tests and hosts) ---------------------------------

    /// The focused node's property `name` on the top screen.
    pub fn focus_prop(&self, name: &str) -> Value {
        let Some(screen) = self.screens.last() else {
            return Value::Nil;
        };
        screen
            .focus
            .and_then(|f| screen.tree.get(f))
            .map(|n| n.prop(name).clone())
            .unwrap_or_default()
    }

    /// The selected tab of every tab strip on the top screen, outermost
    /// first.
    pub fn tab_indices(&self) -> Vec<usize> {
        let Some(screen) = self.screens.last() else {
            return Vec::new();
        };
        let tree = &screen.tree;
        tree.descendants(screen.root)
            .into_iter()
            .filter(|&id| tree[id].kind == Kind::Tabs && tree[id].shown)
            .map(|id| tree[id].index)
            .collect()
    }

    /// The hints the top screen shows.
    pub fn shown_hints(&self) -> Vec<(GuiKey, String)> {
        if self.screens.is_empty() {
            return Vec::new();
        }
        self.hints(self.base())
    }

    /// Run `f` on the top screen's tree.
    pub fn with_tree<R>(&self, f: impl FnOnce(&Tree, Option<NodeId>) -> R) -> Option<R> {
        let screen = self.screens.last()?;
        Some(f(&screen.tree, screen.focus))
    }
}

impl Node {
    fn variant_attr(&self) -> Option<&str> {
        match self.attr("variant") {
            Some(Expr::Lit(v)) => Some(v),
            _ => None,
        }
    }
}

/// Whether `id` can take the focus now.
fn focusable(tree: &Tree, id: NodeId) -> bool {
    tree.get(id).is_some_and(|n| {
        n.kind.focusable()
            && n.shown
            && layout::present(tree, id)
            && match n.kind {
                Kind::Option(OptionKind::Info) => true,
                _ => true,
            }
    })
}

/// A node's `confirm`/`note`/`confirm-if` attributes, when it should ask.
fn confirm_of(node: &Node) -> Option<(String, Option<String>)> {
    let title = node.text("confirm");
    let gated = match node.attr("confirm-if") {
        Some(Expr::Lit(text)) => !text.is_empty(),
        Some(_) => true,
        None => false,
    };
    if title.is_empty() || (gated && !node.prop("confirm-if").truthy()) {
        return None;
    }
    let note = node.text("note");
    Some((title, (!note.is_empty()).then_some(note)))
}

/// The store path a binding path names from node `id`: `item.*` resolves
/// through the nearest template instance.
fn store_path(tree: &Tree, id: NodeId, path: &str) -> Option<String> {
    if path == "item" || path.starts_with("item.") {
        let rest = path.strip_prefix("item").unwrap_or("");
        let (base, _) = tree
            .ancestry(id)
            .into_iter()
            .find_map(|a| tree[a].scope.clone())?;
        if base.is_empty() {
            return None;
        }
        return Some(format!("{base}{rest}"));
    }
    if path.starts_with("focus.") {
        return None;
    }
    Some(path.to_string())
}

/// Where an option row's value is written: its `value` binding, else its
/// `def`'s `.value`.
fn value_path(tree: &Tree, id: NodeId) -> Option<String> {
    let node = &tree[id];
    if let Some(path) = node.attr("value").and_then(Expr::path) {
        return store_path(tree, id, path);
    }
    let def = node.attr("def").and_then(Expr::path)?;
    Some(format!("{}.value", store_path(tree, id, def)?))
}

/// Resolve a binding path for node `id`.
fn lookup(tree: &Tree, id: NodeId, path: &str, store: &Store, focus: Option<NodeId>) -> Value {
    if path == "item" || path.starts_with("item.") {
        let rest = path
            .strip_prefix("item")
            .unwrap_or("")
            .trim_start_matches('.');
        return tree
            .ancestry(id)
            .into_iter()
            .find_map(|a| tree[a].scope.as_ref().map(|(_, v)| v.clone()))
            .and_then(|v| v.at(rest).cloned())
            .unwrap_or_default();
    }
    if let Some(rest) = path.strip_prefix("focus.") {
        return focus
            .and_then(|f| tree.get(f))
            .map(|n| n.prop(rest).clone())
            .unwrap_or_default();
    }
    store.get(path)
}

fn eval_in(tree: &Tree, id: NodeId, expr: &Expr, store: &Store, locale: &Locale) -> Value {
    let mut look = |path: &str| lookup(tree, id, path, store, None);
    expr.eval(&mut look, &|key| locale.get(key).to_string())
}

fn eval_node(
    tree: &mut Tree,
    id: NodeId,
    store: &Store,
    text: &dyn Fn(&str) -> String,
    focus: Option<NodeId>,
) {
    let attrs = tree[id].attrs.clone();
    let mut props = HashMap::with_capacity(attrs.len() + 8);
    for (key, expr) in &attrs {
        let mut look = |path: &str| lookup(tree, id, path, store, focus);
        props.insert(key.clone(), expr.eval(&mut look, text));
    }
    if let Kind::Option(kind) = tree[id].kind {
        derive_option(&mut props, kind, text);
    }
    tree[id].props = props;
}

/// Fill an option row's properties from its `def` table and work out what
/// the description pane shows for it.
fn derive_option(
    props: &mut HashMap<String, Value>,
    kind: OptionKind,
    text: &dyn Fn(&str) -> String,
) {
    if let Some(Value::Map(def)) = props.get("def").cloned() {
        for (key, value) in def.iter() {
            props.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }
    if kind == OptionKind::Toggle && !props.get("choices").is_some_and(Value::truthy) {
        props.insert(
            "choices".into(),
            Value::list(vec![
                Value::from(text("choice.off")),
                Value::from(text("choice.on")),
            ]),
        );
    }
    let has_value = matches!(
        kind,
        OptionKind::Toggle | OptionKind::Choice | OptionKind::Slider
    );
    props.insert("has_value".into(), Value::Bool(has_value));
    let label_of = |value: i64| match kind {
        OptionKind::Toggle | OptionKind::Choice => props
            .get("choices")
            .and_then(|c| c.as_list().get(value.max(0) as usize).cloned())
            .unwrap_or_default()
            .to_string(),
        OptionKind::Slider => format!(
            "{value}{}",
            props.get("suffix").cloned().unwrap_or_default()
        ),
        _ => String::new(),
    };
    if let Some(default) = props.get("default").and_then(Value::as_int) {
        props.insert("default_label".into(), Value::from(label_of(default)));
    }
    // Each described choice, under the option's description.
    let choices = props.get("choices").cloned().unwrap_or_default();
    let described: Vec<String> = props
        .get("choice_descriptions")
        .map(|d| d.as_list().to_vec())
        .unwrap_or_default()
        .iter()
        .zip(choices.as_list())
        .filter(|(d, _)| d.truthy())
        .map(|(d, c)| format!("{c}: {d}"))
        .collect();
    props.insert("choice_text".into(), Value::from(described.join("\n")));
}

/// Build (or rebuild) a repeated node's children from its template when
/// its items changed.
fn rebuild_template(tree: &mut Tree, id: NodeId, builder: &Builder<'_>) {
    let Some(template) = tree[id].template.clone() else {
        return;
    };
    let items = tree[id].prop("items").clone();
    if items == tree[id].items {
        return;
    }
    let base = tree[id]
        .attr("items")
        .and_then(Expr::path)
        .and_then(|p| store_path(tree, id, p))
        .unwrap_or_default();
    let scope = |index: usize, item: &Value| {
        let path = if base.is_empty() {
            String::new()
        } else {
            format!("{base}.{index}")
        };
        let item = match item {
            Value::Map(fields) if !fields.contains_key("index") => {
                let mut fields = (**fields).clone();
                fields.insert("index".into(), Value::from(index));
                Value::Map(std::sync::Arc::new(fields))
            }
            other => other.clone(),
        };
        (path, item)
    };
    let list = items.as_list();
    let children = tree[id].children.clone();
    if children.len() == list.len() * template.len() && !children.is_empty() {
        // The same number of items: keep the nodes (and the focus), only
        // their items change.
        for (n, &child) in children.iter().enumerate() {
            let index = n / template.len();
            tree[child].scope = Some(scope(index, &list[index]));
        }
    } else {
        for child in children {
            tree.remove(child);
        }
        for (index, item) in list.iter().enumerate() {
            for el in template.iter() {
                match builder.build(tree, el, Some(id)) {
                    Ok(child) => tree[child].scope = Some(scope(index, item)),
                    Err(err) => eprintln!("warning: ui template: {err:#}"),
                }
            }
        }
    }
    tree[id].items = items;
}

#[cfg(test)]
mod tests;
