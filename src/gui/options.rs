//! The options model: one definition per setting, which the options screen
//! (`assets/ui/options.xml`) places by id.
//!
//! The engine contributes its settings as [`OptionDef`]s
//! ([`crate::gui::settings`]). A value is an `i32`: a choice's index or a
//! slider's position.

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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(id: &str) -> OptionDef {
        OptionDef {
            id: id.into(),
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
    fn slider_steps_and_clamps() {
        let option = def("1");
        assert_eq!(option.step(4, 1), 6);
        assert_eq!(option.step(10, 1), 10);
        assert_eq!(option.step(0, -1), 0);
    }
}
