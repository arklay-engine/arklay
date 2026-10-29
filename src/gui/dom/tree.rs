//! The node tree a screen is built into, and the builder that expands
//! markup (components, slots, templates) into it.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Result, bail};

use super::markup::{Element, Expr, Library};
use super::value::Value;
use crate::gui::canvas::Rect;
use crate::gui::input::GuiKey;

/// A node's handle: its slot and the slot's generation, so a handle to a
/// removed node never reaches the node that reused its slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId {
    index: u32,
    generation: u32,
}

impl NodeId {
    /// The handle as one number (for scripts).
    pub fn to_bits(self) -> i64 {
        (i64::from(self.generation) << 32) | i64::from(self.index)
    }

    pub fn from_bits(bits: i64) -> Self {
        Self {
            index: bits as u32,
            generation: (bits >> 32) as u32,
        }
    }
}

/// What a node is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A whole screen: the backdrop and the standard padding.
    Frame,
    Stack,
    /// Two panes side by side: a list and what describes it.
    Split,
    Text,
    Rule,
    Separator,
    Image,
    Badge,
    Spacer,
    /// A scrolled, focus-moving list.
    List,
    /// A headed run of rows in a list.
    Group,
    Tabs,
    Tab,
    /// A focusable row that carries out an action.
    Item,
    /// An option row (see [`OptionKind`]).
    Option(OptionKind),
    HintBar,
    Prompt,
    Dialog,
    /// A label-over-value table.
    Facts,
    /// A widget painted by host code ([`super::Custom`]).
    Custom,
}

/// The option row kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionKind {
    /// Off/On.
    Toggle,
    /// One of `choices`.
    Choice,
    /// A number from `min` to `max` in `step`s.
    Slider,
    /// Opens something; it has no value.
    Link,
    /// Shows its description; it has no value.
    Info,
}

impl Kind {
    fn of(name: &str) -> Option<Self> {
        Some(match name {
            "frame" => Kind::Frame,
            "stack" => Kind::Stack,
            "split" => Kind::Split,
            "text" => Kind::Text,
            "rule" => Kind::Rule,
            "separator" => Kind::Separator,
            "image" => Kind::Image,
            "badge" => Kind::Badge,
            "spacer" => Kind::Spacer,
            "list" => Kind::List,
            "group" => Kind::Group,
            "tabs" => Kind::Tabs,
            "tab" => Kind::Tab,
            "item" => Kind::Item,
            "toggle" => Kind::Option(OptionKind::Toggle),
            "choice" => Kind::Option(OptionKind::Choice),
            "slider" => Kind::Option(OptionKind::Slider),
            "link" => Kind::Option(OptionKind::Link),
            "info" => Kind::Option(OptionKind::Info),
            "hint-bar" => Kind::HintBar,
            "prompt" => Kind::Prompt,
            "dialog" => Kind::Dialog,
            "facts" => Kind::Facts,
            _ => return None,
        })
    }

    /// Whether the node takes the focus.
    pub fn focusable(self) -> bool {
        matches!(self, Kind::Item | Kind::Option(_))
    }
}

/// A key a command answers, by its markup name.
pub fn key_named(name: &str) -> Option<GuiKey> {
    Some(match name {
        "up" => GuiKey::Up,
        "down" => GuiKey::Down,
        "left" => GuiKey::Left,
        "right" => GuiKey::Right,
        "confirm" => GuiKey::Confirm,
        "cancel" => GuiKey::Cancel,
        "tab_prev" => GuiKey::TabPrev,
        "tab_next" => GuiKey::TabNext,
        "sub_prev" => GuiKey::SubPrev,
        "sub_next" => GuiKey::SubNext,
        "reset_all" => GuiKey::ResetAll,
        "reset_group" => GuiKey::ResetGroup,
        "aux" => GuiKey::Aux,
        _ => return None,
    })
}

/// A `<command>`: a key, its hint and what it does.
#[derive(Debug, Clone)]
pub struct Command {
    pub key: GuiKey,
    pub label: Expr,
    pub action: Expr,
    /// Ask this first.
    pub confirm: Option<Expr>,
    pub note: Option<Expr>,
    /// Ask only while this is truthy.
    pub confirm_if: Option<Expr>,
    /// Shown in the hint bar.
    pub hint: bool,
    /// The keyboard cap its hint shows, when not the key's own.
    pub cap: Option<Expr>,
}

impl Command {
    fn from_element(el: &Element) -> Result<Self> {
        let key = el.attr("key").unwrap_or("confirm");
        let Some(key) = key_named(key) else {
            bail!("line {}: unknown key {key:?}", el.line);
        };
        Ok(Self {
            key,
            label: Expr::parse(el.attr("label").unwrap_or("")),
            action: Expr::parse(el.attr("action").unwrap_or("")),
            confirm: el.attr("confirm").map(Expr::parse),
            note: el.attr("note").map(Expr::parse),
            confirm_if: el.attr("confirm-if").map(Expr::parse),
            hint: el.attr("hint") != Some("false"),
            cap: el.attr("cap").map(Expr::parse),
        })
    }
}

/// A clickable part of a node, found when it was painted.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Part {
    Decrease,
    Increase,
    /// A slider's track and its x span.
    Track(f32, f32),
    Tab(usize),
    TabPrev,
    TabNext,
    /// A hint: press its key.
    Key(GuiKey),
}

/// One node.
#[derive(Debug, Clone)]
pub struct Node {
    pub kind: Kind,
    /// The element name it was built from (a custom widget's name).
    pub name: String,
    pub attrs: Vec<(String, Expr)>,
    /// The attributes as last evaluated.
    pub props: HashMap<String, Value>,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub commands: Vec<Command>,
    /// A repeated list's template and the items it was last built for.
    pub template: Option<Arc<Vec<Element>>>,
    pub items: Value,
    /// A template instance's item and the store path it came from.
    pub scope: Option<(String, Value)>,
    /// Laid out this frame (in a shown tab, visible).
    pub shown: bool,
    pub rect: Rect,
    /// A list's content height and scroll offset, in pixels.
    pub content: f32,
    pub scroll: f32,
    /// A tab strip's selected tab.
    pub index: usize,
    /// A tab's focus when it was left.
    pub last_focus: Option<NodeId>,
    /// How many tab strips enclose this one.
    pub depth: u8,
    /// Wrapped text, as laid out.
    pub lines: Vec<String>,
    pub parts: Vec<(Rect, Part)>,
    pub reads_focus: bool,
}

impl Node {
    fn new(kind: Kind, name: &str) -> Self {
        Self {
            kind,
            name: name.to_string(),
            attrs: Vec::new(),
            props: HashMap::new(),
            parent: None,
            children: Vec::new(),
            commands: Vec::new(),
            template: None,
            items: Value::Nil,
            scope: None,
            shown: false,
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            content: 0.0,
            scroll: 0.0,
            index: 0,
            last_focus: None,
            depth: 0,
            lines: Vec::new(),
            parts: Vec::new(),
            reads_focus: false,
        }
    }

    pub fn prop(&self, name: &str) -> &Value {
        static NIL: Value = Value::Nil;
        self.props.get(name).unwrap_or(&NIL)
    }

    /// A property as text.
    pub fn text(&self, name: &str) -> String {
        self.prop(name).to_string()
    }

    pub fn attr(&self, name: &str) -> Option<&Expr> {
        self.attrs
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, expr)| expr)
    }

    pub fn set_attr(&mut self, name: &str, expr: Expr) {
        self.reads_focus |= expr.reads_focus();
        match self.attrs.iter_mut().find(|(key, _)| key == name) {
            Some(slot) => slot.1 = expr,
            None => self.attrs.push((name.to_string(), expr)),
        }
    }

    /// The variant the theme styles it by.
    pub fn variant(&self) -> String {
        self.text("variant")
    }
}

#[derive(Debug, Clone, Default)]
struct Slot {
    generation: u32,
    node: Option<Node>,
}

/// A screen's nodes.
#[derive(Debug, Clone, Default)]
pub struct Tree {
    slots: Vec<Slot>,
    free: Vec<u32>,
    pub root: Option<NodeId>,
}

impl std::ops::Index<NodeId> for Tree {
    type Output = Node;
    fn index(&self, id: NodeId) -> &Node {
        self.get(id).expect("a live node")
    }
}

impl std::ops::IndexMut<NodeId> for Tree {
    fn index_mut(&mut self, id: NodeId) -> &mut Node {
        self.get_mut(id).expect("a live node")
    }
}

impl Tree {
    pub fn get(&self, id: NodeId) -> Option<&Node> {
        let slot = self.slots.get(id.index as usize)?;
        (slot.generation == id.generation)
            .then_some(slot.node.as_ref())
            .flatten()
    }

    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        let slot = self.slots.get_mut(id.index as usize)?;
        (slot.generation == id.generation)
            .then_some(slot.node.as_mut())
            .flatten()
    }

    pub fn contains(&self, id: NodeId) -> bool {
        self.get(id).is_some()
    }

    fn insert(&mut self, node: Node) -> NodeId {
        match self.free.pop() {
            Some(index) => {
                let slot = &mut self.slots[index as usize];
                slot.node = Some(node);
                NodeId {
                    index,
                    generation: slot.generation,
                }
            }
            None => {
                self.slots.push(Slot {
                    generation: 0,
                    node: Some(node),
                });
                NodeId {
                    index: self.slots.len() as u32 - 1,
                    generation: 0,
                }
            }
        }
    }

    /// Create a detached node of element `name`.
    pub fn create(&mut self, name: &str, custom: bool) -> Option<NodeId> {
        let kind = Kind::of(name).or(custom.then_some(Kind::Custom))?;
        Some(self.insert(Node::new(kind, name)))
    }

    /// Attach `child` as the last child of `parent` (detaching it first).
    pub fn append(&mut self, parent: NodeId, child: NodeId) {
        self.detach(child);
        self[child].parent = Some(parent);
        self[parent].children.push(child);
    }

    fn detach(&mut self, id: NodeId) {
        if let Some(parent) = self[id].parent.take()
            && let Some(node) = self.get_mut(parent)
        {
            node.children.retain(|&c| c != id);
        }
    }

    /// Remove `id` and everything under it.
    pub fn remove(&mut self, id: NodeId) {
        if !self.contains(id) {
            return;
        }
        self.detach(id);
        let mut stack = vec![id];
        while let Some(id) = stack.pop() {
            let slot = &mut self.slots[id.index as usize];
            if let Some(node) = slot.node.take() {
                stack.extend(node.children);
                slot.generation += 1;
                self.free.push(id.index);
            }
        }
    }

    /// `id` and every node under it, in document order.
    pub fn descendants(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack = vec![id];
        while let Some(id) = stack.pop() {
            out.push(id);
            stack.extend(self[id].children.iter().rev());
        }
        out
    }

    /// `id`, its parent, and so on to the root.
    pub fn ancestry(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = vec![id];
        let mut at = id;
        while let Some(parent) = self.get(at).and_then(|n| n.parent) {
            out.push(parent);
            at = parent;
        }
        out
    }

    /// The first node with `id="name"` under `from`.
    pub fn find_id(&self, from: NodeId, name: &str) -> Option<NodeId> {
        self.descendants(from)
            .into_iter()
            .find(|&id| matches!(self[id].attr("id"), Some(Expr::Lit(s)) if &**s == name))
    }

    /// Nodes matching a selector: `#id` or an element name.
    pub fn select(&self, from: NodeId, selector: &str) -> Vec<NodeId> {
        match selector.strip_prefix('#') {
            Some(id) => self.find_id(from, id).into_iter().collect(),
            None => self
                .descendants(from)
                .into_iter()
                .filter(|&id| self[id].name == selector)
                .collect(),
        }
    }
}

/// Expands markup into a tree.
pub struct Builder<'a> {
    pub library: &'a Library,
    /// Element names host code paints.
    pub customs: &'a dyn Fn(&str) -> bool,
}

/// What a component's `<slot>`s are filled with: the using element's
/// children by slot name, and the parameters of the place they came from.
#[derive(Clone, Default)]
struct Fill<'e> {
    slots: HashMap<String, Vec<&'e Element>>,
    outer: Option<Box<Fill<'e>>>,
    params: HashMap<String, String>,
}

impl Builder<'_> {
    /// Build `el` under `parent`.
    pub fn build(&self, tree: &mut Tree, el: &Element, parent: Option<NodeId>) -> Result<NodeId> {
        let fill = Fill::default();
        let built = self.element(tree, el, parent, &fill, 0)?;
        match built.as_slice() {
            [id] => Ok(*id),
            _ => bail!("line {}: <{}> must build one node", el.line, el.name),
        }
    }

    fn element(
        &self,
        tree: &mut Tree,
        el: &Element,
        parent: Option<NodeId>,
        fill: &Fill<'_>,
        depth: usize,
    ) -> Result<Vec<NodeId>> {
        if depth > 64 {
            bail!("line {}: components nest too deep", el.line);
        }
        match el.name.as_str() {
            "command" => {
                let Some(parent) = parent else {
                    bail!("line {}: a command needs a parent", el.line);
                };
                let mut command = Command::from_element(el)?;
                command.label = substitute_expr(&command.label, el, "label", fill);
                command.action = substitute_expr(&command.action, el, "action", fill);
                tree[parent].commands.push(command);
                return Ok(Vec::new());
            }
            "template" => {
                let Some(parent) = parent else {
                    bail!("line {}: a template needs a parent", el.line);
                };
                tree[parent].template = Some(Arc::new(el.children.clone()));
                return Ok(Vec::new());
            }
            "slot" => {
                let name = el.attr("name").unwrap_or("");
                let content = fill.slots.get(name).cloned().unwrap_or_default();
                if content.is_empty() {
                    return Ok(Vec::new());
                }
                let outer = fill.outer.as_deref().cloned().unwrap_or_default();
                // A bare slot puts its content straight into its parent.
                if el.attrs.iter().all(|(key, _)| key == "name") {
                    let mut built = Vec::new();
                    for child in content {
                        built.extend(self.element(tree, child, parent, &outer, depth + 1)?);
                    }
                    return Ok(built);
                }
                // The slot is a stack holding what fills it.
                let id = tree.create("stack", false).unwrap();
                apply_attrs(tree, id, el, fill, &["name"]);
                if let Some(parent) = parent {
                    tree.append(parent, id);
                }
                for child in content {
                    self.element(tree, child, Some(id), &outer, depth + 1)?;
                }
                return Ok(vec![id]);
            }
            _ => {}
        }
        let screen_use = (el.name == "screen").then(|| el.attr("use")).flatten();
        let component = self
            .library
            .components
            .get(screen_use.unwrap_or(&el.name))
            .cloned();
        if let Some(component) = component {
            return self.component(tree, el, &component, parent, fill, depth);
        }
        let name = if el.name == "screen" {
            "frame"
        } else {
            &el.name
        };
        // A hyphenated name nothing else claims is a host-painted widget.
        let custom = (self.customs)(name) || name.contains('-');
        let Some(id) = tree.create(name, custom) else {
            bail!("line {}: unknown element <{}>", el.line, el.name);
        };
        apply_attrs(tree, id, el, fill, &["use"]);
        if let Some(parent) = parent {
            tree.append(parent, id);
        }
        for child in &el.children {
            self.element(tree, child, Some(id), fill, depth + 1)?;
        }
        Ok(vec![id])
    }

    /// Expand a use of `component`.
    fn component(
        &self,
        tree: &mut Tree,
        usage: &Element,
        component: &Element,
        parent: Option<NodeId>,
        fill: &Fill<'_>,
        depth: usize,
    ) -> Result<Vec<NodeId>> {
        let [body] = component.children.as_slice() else {
            bail!(
                "line {}: component {:?} must hold one element",
                component.line,
                component.attr("name")
            );
        };
        let mut inner = Fill {
            outer: Some(Box::new(fill.clone())),
            ..Fill::default()
        };
        for (key, value) in &usage.attrs {
            inner
                .params
                .insert(key.clone(), substitute(value, fill).to_string());
        }
        let mut commands = Vec::new();
        for child in &usage.children {
            match child.name.as_str() {
                "command" | "template" => commands.push(child),
                _ => inner
                    .slots
                    .entry(child.attr("slot").unwrap_or("").to_string())
                    .or_default()
                    .push(child),
            }
        }
        let built = self.element(tree, body, parent, &inner, depth + 1)?;
        let Some(&root) = built.first() else {
            return Ok(built);
        };
        // The using element's own layout and identity apply to the
        // component's root, and its commands attach there (after the
        // component's, so they win).
        for key in [
            "id", "title", "width", "height", "visible", "variant", "script", "backdrop",
        ] {
            if let Some(value) = usage.attr(key) {
                tree[root].set_attr(key, Expr::parse(&substitute(value, fill)));
            }
        }
        for child in commands {
            self.element(tree, child, Some(root), fill, depth + 1)?;
        }
        Ok(built)
    }
}

/// A component attribute written `{name}` takes the using element's
/// `name` attribute as written (or nothing).
fn substitute<'v>(value: &'v str, fill: &'v Fill<'_>) -> std::borrow::Cow<'v, str> {
    let param = value
        .strip_prefix('{')
        .and_then(|v| v.strip_suffix('}'))
        .filter(|name| !name.contains(['.', '{', '}', ' ']));
    match param {
        Some(name) if fill.outer.is_some() => fill
            .params
            .get(name)
            .map_or(std::borrow::Cow::Borrowed(""), |v| {
                std::borrow::Cow::Owned(v.clone())
            }),
        _ => std::borrow::Cow::Borrowed(value),
    }
}

fn substitute_expr(expr: &Expr, el: &Element, key: &str, fill: &Fill<'_>) -> Expr {
    match el.attr(key) {
        Some(raw) => Expr::parse(&substitute(raw, fill)),
        None => expr.clone(),
    }
}

fn apply_attrs(tree: &mut Tree, id: NodeId, el: &Element, fill: &Fill<'_>, skip: &[&str]) {
    for (key, value) in &el.attrs {
        if key == "slot" || skip.contains(&key.as_str()) {
            continue;
        }
        tree[id].set_attr(key, Expr::parse(&substitute(value, fill)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library() -> Library {
        let mut library = Library::default();
        library
            .add(
                "components",
                r#"<components>
                  <component name="panel">
                    <stack variant="panel">
                      <text text="{title}"/>
                      <slot gap="small"/>
                      <slot name="end"/>
                    </stack>
                  </component>
                </components>"#,
            )
            .unwrap();
        library
    }

    #[test]
    fn components_fill_their_slots_and_params() {
        let library = library();
        let builder = Builder {
            library: &library,
            customs: &|_| false,
        };
        let el = Element::parse(
            r#"<panel title="@hello" id="p">
                 <command key="cancel" label="@hint.back" action="back"/>
                 <text text="body"/>
                 <text slot="end" text="end"/>
               </panel>"#,
        )
        .unwrap();
        let mut tree = Tree::default();
        let root = builder.build(&mut tree, &el, None).unwrap();
        assert_eq!(tree.find_id(root, "p"), Some(root));
        assert_eq!(tree[root].commands.len(), 1);
        let kids = &tree[root].children;
        assert_eq!(kids.len(), 3);
        assert_eq!(
            tree[kids[0]].attr("text"),
            Some(&Expr::Locale("hello".into(), None))
        );
        // A slot with attributes became a stack holding its content; a
        // bare one put it straight in.
        assert_eq!(tree[kids[1]].kind, Kind::Stack);
        let body = tree[kids[1]].children[0];
        assert_eq!(tree[body].attr("text"), Some(&Expr::Lit("body".into())));
        assert_eq!(tree[kids[2]].attr("text"), Some(&Expr::Lit("end".into())));
    }

    #[test]
    fn removed_handles_go_stale() {
        let mut tree = Tree::default();
        let a = tree.create("stack", false).unwrap();
        let b = tree.create("text", false).unwrap();
        tree.append(a, b);
        tree.remove(a);
        assert!(!tree.contains(b));
        let c = tree.create("text", false).unwrap();
        assert!(!tree.contains(b));
        assert!(tree.contains(c));
    }

    #[test]
    fn unknown_elements_are_errors() {
        let library = Library::default();
        let builder = Builder {
            library: &library,
            customs: &|_| false,
        };
        let el = Element::parse("<stack><blink/></stack>").unwrap();
        let err = builder.build(&mut Tree::default(), &el, None).unwrap_err();
        assert!(err.to_string().contains("blink"), "{err}");
    }
}
