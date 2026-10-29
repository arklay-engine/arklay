//! The theme's measurements and inks for every widget, by kind and
//! variant.
//!
//! Screens name a `variant`, never a size or a colour: everything that
//! decides how a widget looks is here, so two screens built from the same
//! widgets look the same. Sizes are in units of the 1080-row reference
//! screen ([`crate::gui::theme::Theme::u`]).

use crate::gui::canvas::Color;
use crate::gui::theme::{Role, palette};

use super::tree::Kind;

/// A screen's padding: top, right, bottom, left.
pub const FRAME_PADDING: [f32; 4] = [52.0, 106.0, 14.0, 106.0];
/// The space between a screen's header, content, prompt and hints.
pub const FRAME_GAP: f32 = 18.0;
/// The modal shade over the screen under a dialog.
pub const MODAL_SHADE: Color = Color::rgba(0, 0, 0, 0x90);

/// The selected save-style row: a pale band with dark text.
pub const SELECTED_BAND: Color = Color::rgb(0xE8, 0xE6, 0xE2);
pub const SELECTED_TEXT: Color = Color::rgb(0x14, 0x14, 0x14);
pub const SELECTED_DIM: Color = Color::rgb(0x4A, 0x4A, 0x4A);
/// The highlight (a save's LATEST) red.
pub const HIGHLIGHT: Color = Color::rgb(0xB0, 0x14, 0x14);

/// A named gap.
pub fn gap(name: &str) -> Option<f32> {
    Some(match name {
        "none" => 0.0,
        "small" => 12.0,
        "medium" => 24.0,
        "large" => 40.0,
        _ => return None,
    })
}

/// A stack's gap when it names none.
pub fn default_gap(horizontal: bool) -> f32 {
    if horizontal { 24.0 } else { 0.0 }
}

/// The fixed height of a row-like node, when it has one.
pub fn row_height(kind: Kind, variant: &str) -> Option<f32> {
    match (kind, variant) {
        (Kind::Item, "menu") => Some(68.0),
        (Kind::Item, "save") => Some(132.0),
        (Kind::Item, "dialog") => Some(52.0),
        (Kind::Item, _) | (Kind::Option(_), _) => Some(66.0),
        _ => None,
    }
}

/// The space between a list's rows.
pub fn list_gap(variant: &str) -> f32 {
    match variant {
        "menu" => 10.0,
        "save" => 12.0,
        "dialog" => 8.0,
        _ => 0.0,
    }
}

/// An item's padding: top, right, bottom, left.
pub fn item_padding(variant: &str) -> [f32; 4] {
    match variant {
        "save" => [14.0, 12.0, 14.0, 8.0],
        "menu" => [0.0, 0.0, 0.0, 40.0],
        _ => [0.0, 24.0, 0.0, 30.0],
    }
}

/// A group header's height and the space above every header but the
/// first.
pub const GROUP_H: f32 = 40.0;
pub const GROUP_GAP: f32 = 24.0;

/// A tab strip's height by depth, the space under its rule, and whether
/// a strip of one tab hides.
pub fn tab_strip(depth: u8) -> (f32, f32, bool) {
    match depth {
        0 => (64.0, 26.0, false),
        _ => (66.0, 30.0, true),
    }
}

/// The fraction of a split the first pane takes, and the gap between.
pub const SPLIT: (f32, f32) = (0.53, 100.0);

/// The width a list keeps for its scrollbar.
pub const SCROLLBAR_ROOM: f32 = 28.0;

/// An image's size by variant.
pub fn image_size(variant: &str) -> (f32, f32) {
    match variant {
        "portrait" => (85.0, 104.0),
        "thumbnail" => (139.0, 104.0),
        "face" => (220.0, 220.0),
        _ => (160.0, 120.0),
    }
}

/// The hint bar's height and the prompt line's.
pub const HINT_BAR_H: f32 = 56.0;
pub const PROMPT_H: f32 = 64.0;

/// What a text variant draws in: its role and whether it is dim.
pub fn text_style(variant: &str) -> (Role, bool) {
    match variant {
        "title" => (Role::Title, false),
        "caption" => (Role::Caption, true),
        "note" => (Role::Body, true),
        "label" => (Role::Body, true),
        "value" => (Role::Row, false),
        "heading" => (Role::Heading, false),
        "heading-dim" => (Role::Heading, true),
        "detail" => (Role::Detail, false),
        "detail-dim" => (Role::Detail, true),
        "tag" => (Role::Tag, true),
        "menu" => (Role::Menu, false),
        _ => (Role::Body, false),
    }
}

/// A badge's fill.
pub fn badge_color(variant: &str) -> Color {
    match variant {
        "highlight" => HIGHLIGHT,
        _ => palette::RULE,
    }
}

/// The ink text draws in: bright and dim.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ink {
    pub fg: Color,
    pub dim: Color,
    /// Dividers.
    pub rule: Color,
    /// Pictures draw in colour (else their grey twin).
    pub vivid: bool,
}

impl Ink {
    pub const NORMAL: Ink = Ink {
        fg: palette::TEXT,
        dim: palette::TEXT_DIM,
        rule: palette::RULE,
        vivid: false,
    };
    pub const DISABLED: Ink = Ink {
        fg: palette::TEXT_DISABLED,
        dim: palette::TEXT_DISABLED,
        rule: palette::RULE,
        vivid: false,
    };
}

/// The ink inside an item of `variant`, selected or not.
pub fn item_ink(variant: &str, selected: bool) -> Ink {
    match (variant, selected) {
        ("save", true) => Ink {
            fg: SELECTED_TEXT,
            dim: SELECTED_DIM,
            rule: SELECTED_DIM,
            vivid: true,
        },
        (_, true) => Ink {
            vivid: true,
            ..Ink::NORMAL
        },
        _ => Ink::NORMAL,
    }
}

/// Where an option row's value arrows sit, as fractions of its width.
pub const VALUE_COLUMN: (f32, f32) = (0.565, 0.965);
