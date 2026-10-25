//! The GUI's single look: scale, font roles and palette.
//!
//! Layout is authored in *units* of a 1080-row reference screen. The scale
//! follows the window's height (and its width when the window is narrower
//! than 4:3), so a layout fills any resolution the same way and text is
//! rasterised at the size it is shown, never stretched. Every screen and
//! widget asks the theme for sizes and colours rather than choosing its own,
//! which is what keeps engine and mod screens looking like one game.

use super::canvas::Color;
use super::text::{Face, Style};

/// The reference screen height layouts are authored against.
pub const REFERENCE_HEIGHT: f32 = 1080.0;
/// The narrowest reference width: a 4:3 screen.
pub const MIN_REFERENCE_WIDTH: f32 = 1440.0;

/// Which embedded faces the theme draws text with.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FontFamily {
    /// Barlow, a clean sans for the HD look.
    #[default]
    Modern,
    /// Departure Mono, a pixel face for the original look.
    Retro,
}

/// A text role; the theme maps each to a face and size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// A screen heading (OPTIONS).
    Title,
    /// The category tabs.
    Tab,
    /// The subcategory tabs.
    SubTab,
    /// A group header strip.
    Group,
    /// An option row's label and value.
    Row,
    /// The description pane.
    Body,
    /// Input hints and key caps.
    Hint,
    /// The text inside a key cap.
    KeyCap,
    /// The in-game menu entries.
    Menu,
    /// A dialog's question.
    DialogTitle,
    /// A dialog's secondary line.
    DialogBody,
    /// The F6 debug HUD's lines.
    Debug,
    /// The F6 debug HUD's section headings.
    DebugHeading,
    /// A corner notice (the game speed).
    Notice,
}

/// The palette.
pub mod palette {
    use super::Color;

    pub const TEXT: Color = Color::rgb(0xF2, 0xF2, 0xF2);
    pub const TEXT_DIM: Color = Color::rgb(0x9A, 0x9A, 0x9A);
    pub const TEXT_DISABLED: Color = Color::rgb(0x5E, 0x5E, 0x5E);
    pub const RULE: Color = Color::rgb(0x5A, 0x5A, 0x5A);
    pub const RULE_BRIGHT: Color = Color::rgb(0xB8, 0xB8, 0xB8);
    pub const PIP: Color = Color::rgb(0x6A, 0x6A, 0x6A);
    pub const GROUP_TOP: Color = Color::rgb(0x52, 0x52, 0x52);
    pub const GROUP_BOTTOM: Color = Color::rgb(0x2C, 0x2C, 0x2C);
    pub const SCREEN_SHADE: Color = Color::rgba(0x00, 0x00, 0x00, 0xE6);
    pub const MENU_SHADE: Color = Color::rgba(0x00, 0x00, 0x00, 0xA0);
    pub const DIALOG_BAND: Color = Color::rgba(0x08, 0x08, 0x08, 0xF0);
    pub const ACCENT: Color = Color::rgb(0xFF, 0xFF, 0xFF);
}

/// The theme resolved for one window size.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    /// Pixels per unit.
    pub scale: f32,
    /// The canvas size in pixels.
    pub width: f32,
    pub height: f32,
    pub family: FontFamily,
}

impl Theme {
    pub fn new(width: u32, height: u32, family: FontFamily) -> Self {
        let (width, height) = (width.max(1) as f32, height.max(1) as f32);
        let scale = (height / REFERENCE_HEIGHT).min(width / MIN_REFERENCE_WIDTH);
        Self {
            scale: scale.max(0.25),
            width,
            height,
            family,
        }
    }

    /// `units` in pixels.
    pub fn u(&self, units: f32) -> f32 {
        units * self.scale
    }

    /// The canvas width in units.
    pub fn width_units(&self) -> f32 {
        self.width / self.scale
    }

    /// The canvas height in units.
    pub fn height_units(&self) -> f32 {
        self.height / self.scale
    }

    /// The face and pixel size `role` draws at.
    pub fn style(&self, role: Role) -> Style {
        let (face, units) = match role {
            Role::Title => (Face::Bold, 46.0),
            Role::Tab => (Face::Bold, 34.0),
            Role::SubTab => (Face::Regular, 30.0),
            Role::Group => (Face::Regular, 30.0),
            Role::Row => (Face::Regular, 31.0),
            Role::Body => (Face::Regular, 31.0),
            Role::Hint => (Face::Regular, 31.0),
            Role::KeyCap => (Face::Regular, 22.0),
            Role::Menu => (Face::Regular, 44.0),
            Role::DialogTitle => (Face::Regular, 38.0),
            Role::DialogBody => (Face::Regular, 30.0),
            Role::Debug => (Face::Regular, 22.0),
            Role::DebugHeading => (Face::Bold, 22.0),
            Role::Notice => (Face::Regular, 30.0),
        };
        let px = self.u(units);
        match self.family {
            FontFamily::Modern => Style::new(face, px.round().max(6.0) as u32),
            // Departure Mono is drawn on an 11-pixel grid; whole multiples
            // keep its pixels square. It runs wide, so it is set a step
            // smaller than the sans.
            FontFamily::Retro => {
                let px = (px * 0.8 / 11.0).round().max(1.0) * 11.0;
                Style::new(Face::Mono, px as u32)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_follows_height_then_width() {
        assert_eq!(Theme::new(1920, 1080, FontFamily::Modern).scale, 1.0);
        assert_eq!(Theme::new(3840, 2160, FontFamily::Modern).scale, 2.0);
        // Narrower than 4:3: the width limits the scale.
        assert_eq!(Theme::new(720, 1080, FontFamily::Modern).scale, 0.5);
    }

    #[test]
    fn retro_sizes_are_multiples_of_eleven() {
        let theme = Theme::new(1920, 1080, FontFamily::Retro);
        assert_eq!(theme.style(Role::Row).px % 11, 0);
    }
}
