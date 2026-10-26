//! The options model: a flat list of option definitions the options screen
//! groups into categories, subcategories and groups by their names.
//!
//! The model is data, not code: the engine contributes its settings as
//! [`OptionDef`]s ([`crate::gui::settings`]) and anything else (a mod's own
//! options, later) appends more, and the screen lays out whatever it is
//! given. A value is an `i32`: a choice's index or a slider's position.

/// One choice of an enumerated option.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub label: String,
    /// What this choice does, listed under the option's description.
    pub description: Option<String>,
}

impl Choice {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            description: None,
        }
    }

    pub fn described(label: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            description: Some(description.into()),
        }
    }
}

/// What kind of control an option is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OptionKind {
    /// Pick one of the choices; the value is the choice index.
    Choice(Vec<Choice>),
    /// A number from `min` to `max` in `step`s; the value is the number.
    Slider {
        min: i32,
        max: i32,
        step: i32,
        /// Appended to the shown number (`%`).
        suffix: &'static str,
    },
    /// A row that opens a screen of its own; it has no value.
    Link,
    /// A row that only shows its description; it has no value.
    Info,
}

/// One option.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionDef {
    /// A stable key the owner maps back to its setting.
    pub id: String,
    pub category: String,
    pub subcategory: String,
    pub group: String,
    pub label: String,
    pub description: String,
    pub kind: OptionKind,
    pub default: i32,
    /// Shown but not changeable (greyed), e.g. a GPU-only option while
    /// the software renderer runs.
    pub disabled: bool,
}

impl OptionDef {
    /// Clamp `value` into the option's range.
    pub fn clamp(&self, value: i32) -> i32 {
        match &self.kind {
            OptionKind::Choice(choices) => value.clamp(0, choices.len().max(1) as i32 - 1),
            OptionKind::Slider { min, max, .. } => value.clamp(*min, *max),
            OptionKind::Link | OptionKind::Info => 0,
        }
    }

    /// The value one step `delta` away from `value`, clamped.
    pub fn step(&self, value: i32, delta: i32) -> i32 {
        match &self.kind {
            OptionKind::Choice(_) => self.clamp(value + delta),
            OptionKind::Slider { step, .. } => self.clamp(value + delta * step.max(&1)),
            OptionKind::Link | OptionKind::Info => 0,
        }
    }

    /// The shown text of `value`.
    pub fn value_label(&self, value: i32) -> String {
        match &self.kind {
            OptionKind::Choice(choices) => choices
                .get(value.max(0) as usize)
                .map(|choice| choice.label.clone())
                .unwrap_or_default(),
            OptionKind::Slider { suffix, .. } => format!("{value}{suffix}"),
            OptionKind::Link | OptionKind::Info => String::new(),
        }
    }

    /// The number of distinct positions.
    pub fn positions(&self) -> usize {
        match &self.kind {
            OptionKind::Choice(choices) => choices.len(),
            OptionKind::Slider { min, max, step, .. } => ((max - min) / step.max(&1)) as usize + 1,
            OptionKind::Link | OptionKind::Info => 1,
        }
    }
}

/// The tree the screen shows, built from the flat list in first-seen order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Category {
    pub name: String,
    pub subcategories: Vec<Subcategory>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subcategory {
    pub name: String,
    /// Indices into the option list, in order.
    pub options: Vec<usize>,
}

/// Group `defs` by category and subcategory, keeping first-seen order.
pub fn tree(defs: &[OptionDef]) -> Vec<Category> {
    let mut categories: Vec<Category> = Vec::new();
    for (index, def) in defs.iter().enumerate() {
        let category = match categories.iter_mut().position(|c| c.name == def.category) {
            Some(position) => &mut categories[position],
            None => {
                categories.push(Category {
                    name: def.category.clone(),
                    subcategories: Vec::new(),
                });
                categories.last_mut().unwrap()
            }
        };
        match category
            .subcategories
            .iter_mut()
            .find(|s| s.name == def.subcategory)
        {
            Some(sub) => sub.options.push(index),
            None => category.subcategories.push(Subcategory {
                name: def.subcategory.clone(),
                options: vec![index],
            }),
        }
    }
    categories
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(category: &str, sub: &str, id: &str) -> OptionDef {
        OptionDef {
            id: id.into(),
            category: category.into(),
            subcategory: sub.into(),
            group: String::new(),
            label: id.into(),
            description: String::new(),
            kind: OptionKind::Slider {
                min: 0,
                max: 10,
                step: 2,
                suffix: "",
            },
            default: 0,
            disabled: false,
        }
    }

    #[test]
    fn tree_keeps_first_seen_order() {
        let defs = [
            def("A", "x", "1"),
            def("B", "y", "2"),
            def("A", "z", "3"),
            def("A", "x", "4"),
        ];
        let tree = tree(&defs);
        assert_eq!(tree.len(), 2);
        assert_eq!(tree[0].subcategories[0].options, vec![0, 3]);
        assert_eq!(tree[0].subcategories[1].options, vec![2]);
        assert_eq!(tree[1].subcategories[0].options, vec![1]);
    }

    #[test]
    fn slider_steps_and_clamps() {
        let option = def("A", "x", "1");
        assert_eq!(option.step(4, 1), 6);
        assert_eq!(option.step(10, 1), 10);
        assert_eq!(option.step(0, -1), 0);
        assert_eq!(option.positions(), 6);
    }
}
