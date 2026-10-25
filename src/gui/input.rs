//! GUI input: navigation keys and the pointer, delivered per frame.
//!
//! The GUI runs at the presented frame rate, not the 30 Hz game tick, so a
//! held direction repeats at the platform's key-repeat rate and the pointer
//! tracks every motion event. Navigation comes from the player's own action
//! bindings (so a rebind moves the menus too); the tab, sub-tab and reset
//! keys are fixed, like the hints drawn for them.

/// A navigation command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuiKey {
    Up,
    Down,
    Left,
    Right,
    Confirm,
    Cancel,
    /// Q / LB: the previous category tab.
    TabPrev,
    /// E / RB: the next category tab.
    TabNext,
    /// 1 / LT: the previous subcategory.
    SubPrev,
    /// 3 / RT: the next subcategory.
    SubNext,
    /// Tab / Back: reset every option.
    ResetAll,
    /// R / Y: reset the shown subcategory.
    ResetGroup,
}

/// Which kind of device produced the last input, so hints can show the
/// matching glyphs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Device {
    #[default]
    Keyboard,
    Gamepad(PadStyle),
}

/// Whose button names and symbols a controller's glyphs use. The buttons
/// are the same by position (SDL's south/east/west/north), only their
/// labels differ.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PadStyle {
    #[default]
    Xbox,
    PlayStation,
    Nintendo,
}

/// A feedback sound the GUI asks its owner to play.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sound {
    /// The cursor or a value moved.
    Cursor,
    /// Something opened or was carried out.
    Decide,
    /// Backed out.
    Cancel,
}

/// One input event, in canvas pixels for the pointer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GuiEvent {
    Key(GuiKey, Device),
    PointerMove {
        x: f32,
        y: f32,
    },
    /// The primary button went down.
    PointerDown {
        x: f32,
        y: f32,
    },
    /// The primary button came back up.
    PointerUp {
        x: f32,
        y: f32,
    },
    /// Wheel notches; positive scrolls up.
    Wheel(f32),
}

/// The key cap a keyboard hint shows for `key`.
pub fn key_label(key: GuiKey) -> &'static str {
    match key {
        GuiKey::Up => "Up",
        GuiKey::Down => "Down",
        GuiKey::Left => "Left",
        GuiKey::Right => "Right",
        GuiKey::TabPrev => "Q",
        GuiKey::TabNext => "E",
        GuiKey::SubPrev => "1",
        GuiKey::SubNext => "3",
        GuiKey::ResetAll => "Tab",
        GuiKey::ResetGroup => "R",
        GuiKey::Cancel => "Esc",
        GuiKey::Confirm => "Enter",
    }
}
