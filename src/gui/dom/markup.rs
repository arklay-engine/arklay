//! Screen markup: the XML a screen is written in, its components and its
//! attribute expressions.
//!
//! A [`Library`] holds every parsed document by name. A document's root is
//! either a `<screen>` (a whole screen, opened by its file name) or a
//! `<components>` list of `<component name="...">` definitions any screen
//! can use as if they were elements. Attribute values are [`Expr`]s:
//!
//! * `text` — a literal;
//! * `{path}` — a binding: the store value, `item.*` inside a template,
//!   `focus.*` for the focused node's properties; `{a} / {b}` interpolates;
//! * `@key` — a GUI string in the current language, `@key(path)` with its
//!   `{}` filled from a binding; `@@` escapes a literal `@`.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result, bail};

use super::value::Value;

/// One parsed element.
#[derive(Debug, Clone)]
pub struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Element>,
    /// The source line, for errors.
    pub line: u32,
}

impl Element {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// Parse `source` into its root element.
    pub fn parse(source: &str) -> Result<Self> {
        let doc = roxmltree::Document::parse(source)?;
        Ok(Self::from_node(&doc, doc.root_element()))
    }

    fn from_node(doc: &roxmltree::Document<'_>, node: roxmltree::Node<'_, '_>) -> Self {
        Self {
            name: node.tag_name().name().to_string(),
            attrs: node
                .attributes()
                .map(|a| (a.name().to_string(), a.value().to_string()))
                .collect(),
            children: node
                .children()
                .filter(|child| child.is_element())
                .map(|child| Self::from_node(doc, child))
                .collect(),
            line: doc.text_pos_at(node.range().start).row,
        }
    }
}

/// An attribute's value as written.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Lit(Arc<str>),
    /// A binding path.
    Path(String),
    /// A GUI string, optionally formatted with a binding.
    Locale(String, Option<String>),
    /// Literal text and bindings, joined.
    Parts(Vec<Expr>),
}

impl Expr {
    pub fn parse(source: &str) -> Self {
        if let Some(rest) = source.strip_prefix("@@") {
            return Expr::Lit(Arc::from(format!("@{rest}")));
        }
        if let Some(rest) = source.strip_prefix('@') {
            return match rest.split_once('(') {
                Some((key, arg)) => Expr::Locale(
                    key.trim().to_string(),
                    Some(arg.trim_end_matches(')').trim().to_string()),
                ),
                None => Expr::Locale(rest.trim().to_string(), None),
            };
        }
        let mut parts = Vec::new();
        let mut text = String::new();
        let mut chars = source.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '{' if chars.peek() == Some(&'{') => {
                    chars.next();
                    text.push('{');
                }
                '{' => {
                    let path: String = chars.by_ref().take_while(|&c| c != '}').collect();
                    if !text.is_empty() {
                        parts.push(Expr::Lit(Arc::from(std::mem::take(&mut text))));
                    }
                    parts.push(Expr::Path(path.trim().to_string()));
                }
                '}' if chars.peek() == Some(&'}') => {
                    chars.next();
                    text.push('}');
                }
                c => text.push(c),
            }
        }
        if !text.is_empty() || parts.is_empty() {
            parts.push(Expr::Lit(Arc::from(text)));
        }
        if parts.len() == 1 {
            parts.pop().unwrap()
        } else {
            Expr::Parts(parts)
        }
    }

    /// The binding path, when the whole value is one binding.
    pub fn path(&self) -> Option<&str> {
        match self {
            Expr::Path(path) => Some(path),
            _ => None,
        }
    }

    /// Whether evaluating this reads the focused node.
    pub fn reads_focus(&self) -> bool {
        match self {
            Expr::Lit(_) => false,
            Expr::Path(path) => path.starts_with("focus."),
            Expr::Locale(_, arg) => arg.as_deref().is_some_and(|a| a.starts_with("focus.")),
            Expr::Parts(parts) => parts.iter().any(Expr::reads_focus),
        }
    }

    /// Evaluate with `lookup` resolving binding paths and `text` the GUI
    /// strings.
    pub fn eval(
        &self,
        lookup: &mut dyn FnMut(&str) -> Value,
        text: &dyn Fn(&str) -> String,
    ) -> Value {
        match self {
            Expr::Lit(text) => Value::Str(text.clone()),
            Expr::Path(path) => lookup(path),
            Expr::Locale(key, None) => Value::from(text(key)),
            Expr::Locale(key, Some(arg)) => {
                Value::from(text(key).replacen("{}", &lookup(arg).to_string(), 1))
            }
            Expr::Parts(parts) => {
                let mut out = String::new();
                for part in parts {
                    out += &part.eval(lookup, text).to_string();
                }
                Value::from(out)
            }
        }
    }
}

/// Every screen and component the UI can build, by name.
#[derive(Debug, Clone, Default)]
pub struct Library {
    pub(super) screens: HashMap<String, Arc<Element>>,
    pub(super) components: HashMap<String, Arc<Element>>,
    pub(super) scripts: HashMap<String, Arc<str>>,
}

/// The engine's own screens and components.
const BUILTIN: [(&str, &str); 11] = [
    (
        "components",
        include_str!("../../../assets/ui/components.xml"),
    ),
    ("options", include_str!("../../../assets/ui/options.xml")),
    ("menu", include_str!("../../../assets/ui/menu.xml")),
    ("load", include_str!("../../../assets/ui/load.xml")),
    ("save", include_str!("../../../assets/ui/save.xml")),
    ("dialog", include_str!("../../../assets/ui/dialog.xml")),
    ("debug", include_str!("../../../assets/ui/debug.xml")),
    (
        "debug_player",
        include_str!("../../../assets/ui/debug_player.xml"),
    ),
    (
        "debug_cheats",
        include_str!("../../../assets/ui/debug_cheats.xml"),
    ),
    (
        "debug_clear",
        include_str!("../../../assets/ui/debug_clear.xml"),
    ),
    (
        "debug_flags",
        include_str!("../../../assets/ui/debug_flags.xml"),
    ),
];

impl Library {
    /// The engine's built-in screens.
    pub fn builtin() -> Self {
        let mut library = Self::default();
        for (name, source) in BUILTIN {
            library
                .add(name, source)
                .unwrap_or_else(|err| panic!("built-in screen {name}: {err:#}"));
        }
        library
    }

    /// Add (or replace) the document `name`: a `<screen>` opened by that
    /// name, or a `<components>` list.
    pub fn add(&mut self, name: &str, source: &str) -> Result<()> {
        let root = Element::parse(source).with_context(|| format!("parsing {name}"))?;
        match root.name.as_str() {
            "screen" => {
                self.screens.insert(name.to_string(), Arc::new(root));
            }
            "components" => {
                for component in root.children {
                    if component.name != "component" {
                        bail!(
                            "{name}:{}: <components> holds only <component>",
                            component.line
                        );
                    }
                    let Some(id) = component.attr("name") else {
                        bail!("{name}:{}: a component needs a name", component.line);
                    };
                    self.components.insert(id.to_string(), Arc::new(component));
                }
            }
            other => bail!("{name}: unknown root <{other}>"),
        }
        Ok(())
    }

    /// Add a Lua script a screen's `script` attribute can name.
    pub fn add_script(&mut self, name: &str, source: &str) {
        self.scripts.insert(name.to_string(), Arc::from(source));
    }

    pub fn screen(&self, name: &str) -> Option<&Arc<Element>> {
        self.screens.get(name)
    }

    /// The names of every screen.
    pub fn screen_names(&self) -> impl Iterator<Item = &str> {
        self.screens.keys().map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expressions_parse() {
        assert_eq!(Expr::parse("plain"), Expr::Lit(Arc::from("plain")));
        assert_eq!(Expr::parse("{a.b}"), Expr::Path("a.b".into()));
        assert_eq!(
            Expr::parse("@save.count(saves.count)"),
            Expr::Locale("save.count".into(), Some("saves.count".into()))
        );
        assert_eq!(Expr::parse("@@x"), Expr::Lit(Arc::from("@x")));
        let parts = Expr::parse("{a}  {b}");
        let mut lookup = |path: &str| Value::from(path.to_uppercase());
        assert_eq!(parts.eval(&mut lookup, &|k| k.into()), Value::from("A  B"));
        assert_eq!(Expr::parse("{{x}}"), Expr::Lit(Arc::from("{x}")));
    }
}
