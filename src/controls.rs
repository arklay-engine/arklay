//! The logical action map: every action the game reads and the physical
//! inputs bound to it.
//!
//! The engine never asks about a key, a mouse button or a gamepad button
//! directly. Each input source is looked up in [`Bindings`], which maps it to
//! the [`Action`]s it drives; an action may carry any number of bindings from
//! any device (aim on the right mouse button and the left trigger at once).
//! The port-only inputs (F1 for the debug menu, F9 return to title, the
//! gamepad's start button for the in-game menu) are fixed and can never be
//! bound: see [`Source::reserved_bit`]. Pressing the gamepad's back and start
//! buttons together also opens the debug menu; back on its own stays
//! bindable (the map by default).
//!
//! Bindings persist in `arklay.toml`'s `[bindings]` table as one array of
//! source names per action:
//!
//! ```toml
//! [bindings]
//! aim = ["mouse:right", "axis:lefttrigger+"]
//! ```
//!
//! A source name is `key:<SDL scancode name>`, `mouse:left|middle|right|x1|x2`,
//! `wheel:up|down|left|right`, `pad:<SDL gamepad button>` (`a`, `b`, `dpup`,
//! `leftshoulder`, `start`, ...) or `axis:<SDL gamepad axis>+|-` (`leftx-`,
//! `lefttrigger+`, ...). An input SDL has no name for is written by number:
//! `key:#N`, `mouse:N`, `pad:#N`, `axis:#N+`. An action the table leaves out
//! keeps its default.

use std::ffi::{CStr, CString};
use std::fmt;

use anyhow::{Result, bail};
use sdl3_sys::gamepad::{
    SDL_GAMEPAD_AXIS_COUNT, SDL_GAMEPAD_AXIS_INVALID, SDL_GAMEPAD_BUTTON_BACK,
    SDL_GAMEPAD_BUTTON_COUNT, SDL_GAMEPAD_BUTTON_INVALID, SDL_GAMEPAD_BUTTON_START,
    SDL_GamepadAxis, SDL_GamepadButton, SDL_GetGamepadAxisFromString,
    SDL_GetGamepadButtonFromString, SDL_GetGamepadStringForAxis, SDL_GetGamepadStringForButton,
};
use sdl3_sys::keyboard::{SDL_GetScancodeFromName, SDL_GetScancodeName};
use sdl3_sys::scancode::{
    SDL_SCANCODE_0, SDL_SCANCODE_1, SDL_SCANCODE_A, SDL_SCANCODE_BACKSPACE, SDL_SCANCODE_COUNT,
    SDL_SCANCODE_D, SDL_SCANCODE_DOWN, SDL_SCANCODE_ESCAPE, SDL_SCANCODE_F, SDL_SCANCODE_F1,
    SDL_SCANCODE_F9, SDL_SCANCODE_I, SDL_SCANCODE_LEFT, SDL_SCANCODE_LEFTBRACKET,
    SDL_SCANCODE_LSHIFT, SDL_SCANCODE_M, SDL_SCANCODE_Q, SDL_SCANCODE_R, SDL_SCANCODE_RETURN,
    SDL_SCANCODE_RIGHT, SDL_SCANCODE_RIGHTBRACKET, SDL_SCANCODE_RSHIFT, SDL_SCANCODE_S,
    SDL_SCANCODE_TAB, SDL_SCANCODE_UNKNOWN, SDL_SCANCODE_UP, SDL_SCANCODE_W, SDL_Scancode,
};

/// One logical action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Action {
    Up,
    Down,
    Left,
    Right,
    /// Confirm / action / dismiss a message (the pad's cross).
    Confirm,
    /// Cancel in menus (the pad's circle).
    Cancel,
    Run,
    /// Aim the equipped weapon (held).
    Aim,
    /// Fire / reload while aiming.
    Fire,
    /// START: the inventory (pause menu).
    Start,
    /// L1: page the item box back.
    PageLeft,
    /// R1: page the item box forward.
    PageRight,
    /// Port-only: the quick turn on its own button (with `[game] quick_turn`).
    QuickTurn,
    /// Port-only: the tactical reload on its own button (with `[game]
    /// tactical_reload`).
    Reload,
    /// Port-only: open the map straight from gameplay.
    Map,
    /// Port-only: equip the previous weapon in inventory order (or none).
    WeaponPrev,
    /// Port-only: equip the next weapon in inventory order (or none).
    WeaponNext,
    /// Port-only: lock the auto aim on to the previous target.
    TargetPrev,
    /// Port-only: lock the auto aim on to the next target.
    TargetNext,
    /// Port-only: equip the weapon in inventory slot 1 (or unequip).
    EquipSlot1,
    EquipSlot2,
    EquipSlot3,
    EquipSlot4,
    EquipSlot5,
    EquipSlot6,
    EquipSlot7,
    EquipSlot8,
    EquipSlot9,
    EquipSlot10,
}

/// Every action, in the order the rebind screen and the config file list them.
pub const ACTIONS: [Action; 29] = [
    Action::Up,
    Action::Down,
    Action::Left,
    Action::Right,
    Action::Confirm,
    Action::Cancel,
    Action::Run,
    Action::Aim,
    Action::Fire,
    Action::Start,
    Action::PageLeft,
    Action::PageRight,
    Action::QuickTurn,
    Action::Reload,
    Action::Map,
    Action::WeaponPrev,
    Action::WeaponNext,
    Action::TargetPrev,
    Action::TargetNext,
    Action::EquipSlot1,
    Action::EquipSlot2,
    Action::EquipSlot3,
    Action::EquipSlot4,
    Action::EquipSlot5,
    Action::EquipSlot6,
    Action::EquipSlot7,
    Action::EquipSlot8,
    Action::EquipSlot9,
    Action::EquipSlot10,
];

/// The slot-equip actions, slot 1 first.
pub const EQUIP_SLOTS: [Action; 10] = [
    Action::EquipSlot1,
    Action::EquipSlot2,
    Action::EquipSlot3,
    Action::EquipSlot4,
    Action::EquipSlot5,
    Action::EquipSlot6,
    Action::EquipSlot7,
    Action::EquipSlot8,
    Action::EquipSlot9,
    Action::EquipSlot10,
];

/// The fixed debug-menu bit (F1, or the gamepad's back and start pressed
/// together), above every action bit.
pub const DEBUG_MENU_BIT: u64 = 1 << 32;
/// The fixed F9 return-to-title bit.
pub const RETURN_TITLE_BIT: u64 = 1 << 33;
/// The fixed in-game menu bit: the gamepad's Start button (Escape raises it
/// too, on top of its bindings).
pub const SYSTEM_MENU_BIT: u64 = 1 << 34;

impl Action {
    /// The action's bit in an input word (the `--input` script word and the
    /// engine's latched held/pressed words).
    pub const fn bit(self) -> u64 {
        1 << self as u64
    }

    /// The action's `[bindings]` key.
    pub const fn name(self) -> &'static str {
        match self {
            Action::Up => "up",
            Action::Down => "down",
            Action::Left => "left",
            Action::Right => "right",
            Action::Confirm => "confirm",
            Action::Cancel => "cancel",
            Action::Run => "run",
            Action::Aim => "aim",
            Action::Fire => "fire",
            Action::Start => "start",
            Action::PageLeft => "page_left",
            Action::PageRight => "page_right",
            Action::QuickTurn => "quick_turn",
            Action::Reload => "reload",
            Action::Map => "map",
            Action::WeaponPrev => "weapon_prev",
            Action::WeaponNext => "weapon_next",
            Action::TargetPrev => "target_prev",
            Action::TargetNext => "target_next",
            Action::EquipSlot1 => "equip_1",
            Action::EquipSlot2 => "equip_2",
            Action::EquipSlot3 => "equip_3",
            Action::EquipSlot4 => "equip_4",
            Action::EquipSlot5 => "equip_5",
            Action::EquipSlot6 => "equip_6",
            Action::EquipSlot7 => "equip_7",
            Action::EquipSlot8 => "equip_8",
            Action::EquipSlot9 => "equip_9",
            Action::EquipSlot10 => "equip_10",
        }
    }

    /// The action for a `[bindings]` key.
    pub fn from_name(name: &str) -> Option<Self> {
        ACTIONS.into_iter().find(|action| action.name() == name)
    }

    /// The rebind screen's label key.
    pub const fn label(self) -> &'static str {
        match self {
            Action::Up => "UI_CONTROLS_UP",
            Action::Down => "UI_CONTROLS_DOWN",
            Action::Left => "UI_CONTROLS_LEFT",
            Action::Right => "UI_CONTROLS_RIGHT",
            Action::Confirm => "UI_CONTROLS_CONFIRM",
            Action::Cancel => "UI_CONTROLS_CANCEL",
            Action::Run => "UI_CONTROLS_RUN",
            Action::Aim => "UI_CONTROLS_AIM",
            Action::Fire => "UI_CONTROLS_FIRE",
            Action::Start => "UI_CONTROLS_START",
            Action::PageLeft => "UI_CONTROLS_PAGE_LEFT",
            Action::PageRight => "UI_CONTROLS_PAGE_RIGHT",
            Action::QuickTurn => "UI_CONTROLS_QUICK_TURN",
            Action::Reload => "UI_CONTROLS_RELOAD",
            Action::Map => "UI_CONTROLS_MAP",
            Action::WeaponPrev => "UI_CONTROLS_WEAPON_PREV",
            Action::WeaponNext => "UI_CONTROLS_WEAPON_NEXT",
            Action::TargetPrev => "UI_CONTROLS_TARGET_PREV",
            Action::TargetNext => "UI_CONTROLS_TARGET_NEXT",
            Action::EquipSlot1 => "UI_CONTROLS_EQUIP_1",
            Action::EquipSlot2 => "UI_CONTROLS_EQUIP_2",
            Action::EquipSlot3 => "UI_CONTROLS_EQUIP_3",
            Action::EquipSlot4 => "UI_CONTROLS_EQUIP_4",
            Action::EquipSlot5 => "UI_CONTROLS_EQUIP_5",
            Action::EquipSlot6 => "UI_CONTROLS_EQUIP_6",
            Action::EquipSlot7 => "UI_CONTROLS_EQUIP_7",
            Action::EquipSlot8 => "UI_CONTROLS_EQUIP_8",
            Action::EquipSlot9 => "UI_CONTROLS_EQUIP_9",
            Action::EquipSlot10 => "UI_CONTROLS_EQUIP_10",
        }
    }

    /// Whether the rebind screen itself is driven by the action, so it must
    /// always keep a keyboard key of its own (see [`Bindings::lockout`]).
    pub const fn is_essential(self) -> bool {
        matches!(
            self,
            Action::Up
                | Action::Down
                | Action::Left
                | Action::Right
                | Action::Confirm
                | Action::Cancel
                | Action::Start
        )
    }
}

/// A mouse wheel direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Wheel {
    Up,
    Down,
    Left,
    Right,
}

const WHEEL_NAMES: [(Wheel, &str); 4] = [
    (Wheel::Up, "up"),
    (Wheel::Down, "down"),
    (Wheel::Left, "left"),
    (Wheel::Right, "right"),
];

/// SDL's left mouse button.
const MOUSE_LEFT: u8 = 1;
/// SDL's right mouse button.
const MOUSE_RIGHT: u8 = 3;
/// SDL's X1 mouse button: back.
const MOUSE_BACK: u8 = 4;
/// SDL's X2 mouse button: forward.
const MOUSE_FORWARD: u8 = 5;

/// SDL's mouse button numbers and their names.
const MOUSE_NAMES: [(u8, &str); 5] = [
    (MOUSE_LEFT, "left"),
    (2, "middle"),
    (MOUSE_RIGHT, "right"),
    (MOUSE_BACK, "x1"),
    (MOUSE_FORWARD, "x2"),
];

/// One physical input a binding can name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Source {
    /// A keyboard key, by SDL scancode.
    Key(i32),
    /// A mouse button, by SDL button number (1 left .. 5 X2).
    Mouse(u8),
    /// One notch of the mouse wheel: a press with no hold.
    Wheel(Wheel),
    /// A gamepad button, by `SDL_GamepadButton`.
    PadButton(i32),
    /// A gamepad axis pushed past its threshold in one direction, by
    /// `SDL_GamepadAxis` (the triggers only ever read positive).
    PadAxis(i32, bool),
}

impl Source {
    /// A gamepad button or axis.
    pub fn is_pad(self) -> bool {
        matches!(self, Source::PadButton(_) | Source::PadAxis(..))
    }

    /// A keyboard key.
    pub fn key(scancode: SDL_Scancode) -> Self {
        Source::Key(scancode.0)
    }

    /// The fixed port-only bit a reserved input raises (F1 for the debug
    /// menu, F9, the gamepad's start button for the in-game menu), or `None`
    /// for a source that can be bound.
    pub fn reserved_bit(self) -> Option<u64> {
        match self {
            Source::Key(code) if code == SDL_SCANCODE_F1.0 => Some(DEBUG_MENU_BIT),
            Source::Key(code) if code == SDL_SCANCODE_F9.0 => Some(RETURN_TITLE_BIT),
            Source::PadButton(button) if button == SDL_GAMEPAD_BUTTON_START.0 => {
                Some(SYSTEM_MENU_BIT)
            }
            _ => None,
        }
    }

    /// Whether the source's `device:name` form reads back as the same source,
    /// so it can be saved (SDL names two scancodes `Return`, for one).
    pub fn round_trips(self) -> bool {
        Source::parse(&self.to_string()).is_ok_and(|parsed| parsed == self)
    }

    /// Parse a `[bindings]` source name; a reserved input is an error.
    pub fn parse(text: &str) -> Result<Self> {
        let source = Self::parse_any(text)?;
        if source.reserved_bit().is_some() {
            bail!("`{text}` is reserved for the port's fixed inputs");
        }
        Ok(source)
    }

    /// Parse a source name, reserved inputs included: a config written
    /// before an input became reserved (`pad:start`) still loads, and
    /// [`Bindings::set`] drops the reserved source.
    pub fn parse_any(text: &str) -> Result<Self> {
        let Some((device, name)) = text.split_once(':') else {
            bail!("`{text}` is not `device:name`");
        };
        let source = match device {
            "key" => {
                if let Some(code) = numbered(name, SDL_SCANCODE_COUNT.0) {
                    if code == SDL_SCANCODE_UNKNOWN.0 {
                        bail!("unknown key `{name}`");
                    }
                    Source::Key(code)
                } else {
                    let scancode = cstr_lookup(name, |c| unsafe { SDL_GetScancodeFromName(c) });
                    match scancode {
                        Some(code) if code != SDL_SCANCODE_UNKNOWN => Source::key(code),
                        _ => bail!("unknown key `{name}`"),
                    }
                }
            }
            "mouse" => match MOUSE_NAMES.iter().find(|(_, n)| *n == name) {
                Some(&(button, _)) => Source::Mouse(button),
                None => match name.parse::<u8>() {
                    Ok(button) if button > 0 => Source::Mouse(button),
                    _ => bail!("unknown mouse button `{name}`"),
                },
            },
            "wheel" => match WHEEL_NAMES.iter().find(|(_, n)| *n == name) {
                Some(&(wheel, _)) => Source::Wheel(wheel),
                None => bail!("unknown wheel direction `{name}`"),
            },
            "pad" => {
                let button = numbered(name, SDL_GAMEPAD_BUTTON_COUNT.0)
                    .map(SDL_GamepadButton)
                    .or_else(|| {
                        cstr_lookup(name, |c| unsafe { SDL_GetGamepadButtonFromString(c) })
                    });
                match button {
                    Some(button) if button != SDL_GAMEPAD_BUTTON_INVALID => {
                        Source::PadButton(button.0)
                    }
                    _ => bail!("unknown gamepad button `{name}`"),
                }
            }
            "axis" => {
                let (axis, positive) = if let Some(axis) = name.strip_suffix('+') {
                    (axis, true)
                } else if let Some(axis) = name.strip_suffix('-') {
                    (axis, false)
                } else {
                    bail!("gamepad axis `{name}` needs a `+` or `-` direction");
                };
                let axis = numbered(axis, SDL_GAMEPAD_AXIS_COUNT.0)
                    .map(SDL_GamepadAxis)
                    .or_else(|| cstr_lookup(axis, |c| unsafe { SDL_GetGamepadAxisFromString(c) }));
                match axis {
                    Some(axis) if axis != SDL_GAMEPAD_AXIS_INVALID => {
                        Source::PadAxis(axis.0, positive)
                    }
                    _ => bail!("unknown gamepad axis `{name}`"),
                }
            }
            other => bail!("unknown input device `{other}`"),
        };
        Ok(source)
    }

    /// The short upper-case label the rebind screen draws.
    pub fn label(self) -> String {
        match self {
            Source::Key(_) => self.name().1.to_ascii_uppercase(),
            Source::Mouse(_) | Source::Wheel(_) | Source::PadButton(_) | Source::PadAxis(..) => {
                let (device, name) = self.name();
                format!("{device} {name}").to_ascii_uppercase()
            }
        }
    }

    /// The source's name without its device (`Left Shift`, `left`), as the
    /// key configuration screen draws a key.
    pub fn short_name(self) -> String {
        self.name().1
    }

    /// The `(device, name)` pair of the source's `device:name` form.
    fn name(self) -> (&'static str, String) {
        match self {
            Source::Key(code) => {
                let name = (0..SDL_SCANCODE_COUNT.0)
                    .contains(&code)
                    .then(|| sdl_str(unsafe { SDL_GetScancodeName(SDL_Scancode(code)) }))
                    .flatten()
                    .unwrap_or_else(|| format!("#{code}"));
                ("key", name)
            }
            Source::Mouse(button) => {
                let name = MOUSE_NAMES
                    .iter()
                    .find(|(b, _)| *b == button)
                    .map_or_else(|| format!("{button}"), |(_, n)| n.to_string());
                ("mouse", name)
            }
            Source::Wheel(wheel) => {
                let name = WHEEL_NAMES.iter().find(|(w, _)| *w == wheel).unwrap().1;
                ("wheel", name.to_string())
            }
            Source::PadButton(button) => {
                let name = (0..SDL_GAMEPAD_BUTTON_COUNT.0)
                    .contains(&button)
                    .then(|| {
                        sdl_str(unsafe { SDL_GetGamepadStringForButton(SDL_GamepadButton(button)) })
                    })
                    .flatten()
                    .unwrap_or_else(|| format!("#{button}"));
                ("pad", name)
            }
            Source::PadAxis(axis, positive) => {
                let name = (0..SDL_GAMEPAD_AXIS_COUNT.0)
                    .contains(&axis)
                    .then(|| sdl_str(unsafe { SDL_GetGamepadStringForAxis(SDL_GamepadAxis(axis)) }))
                    .flatten()
                    .unwrap_or_else(|| format!("#{axis}"));
                (
                    "axis",
                    format!("{name}{}", if positive { '+' } else { '-' }),
                )
            }
        }
    }
}

impl fmt::Display for Source {
    /// The `device:name` form [`Source::parse`] reads back.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (device, name) = self.name();
        write!(f, "{device}:{name}")
    }
}

/// The `N` of a `#N` name, when below `count`.
fn numbered(name: &str, count: i32) -> Option<i32> {
    let index = name.strip_prefix('#')?.parse::<i32>().ok()?;
    (0..count).contains(&index).then_some(index)
}

/// Run an SDL name lookup on `name` as a C string (`None` for an interior NUL).
fn cstr_lookup<T>(name: &str, lookup: impl FnOnce(*const std::ffi::c_char) -> T) -> Option<T> {
    let name = CString::new(name).ok()?;
    Some(lookup(name.as_ptr()))
}

/// An SDL-owned name, `None` when null or empty.
fn sdl_str(ptr: *const std::ffi::c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let name = unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned();
    (!name.is_empty()).then_some(name)
}

/// Every action's bindings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bindings {
    sources: [Vec<Source>; ACTIONS.len()],
}

impl Default for Bindings {
    /// The shipped layout.
    ///
    /// The keyboard and mouse: WASD or the arrows move, Shift runs, F, Return
    /// or the left mouse button act (and fire while aiming, like the
    /// original's action button), the right mouse button aims, R reloads, Q
    /// turns around, Tab or I opens the inventory, M the map, 1 to 0 equip an
    /// inventory slot's weapon, the wheel cycles the weapons and the mouse's
    /// back and forward buttons cycle the auto-aim target.
    ///
    /// The gamepad: the d-pad and left stick move, south confirms and fires
    /// (the original's action button), east cancels, west runs, the triggers
    /// aim and fire, the shoulders page the item box and cycle the target,
    /// north opens the inventory and back the map; start is the fixed
    /// in-game menu button.
    fn default() -> Self {
        use sdl3_sys::gamepad::{
            SDL_GAMEPAD_AXIS_LEFT_TRIGGER, SDL_GAMEPAD_AXIS_LEFTX, SDL_GAMEPAD_AXIS_LEFTY,
            SDL_GAMEPAD_AXIS_RIGHT_TRIGGER, SDL_GAMEPAD_BUTTON_DPAD_DOWN,
            SDL_GAMEPAD_BUTTON_DPAD_LEFT, SDL_GAMEPAD_BUTTON_DPAD_RIGHT,
            SDL_GAMEPAD_BUTTON_DPAD_UP, SDL_GAMEPAD_BUTTON_EAST, SDL_GAMEPAD_BUTTON_LEFT_SHOULDER,
            SDL_GAMEPAD_BUTTON_NORTH, SDL_GAMEPAD_BUTTON_RIGHT_SHOULDER, SDL_GAMEPAD_BUTTON_SOUTH,
            SDL_GAMEPAD_BUTTON_WEST,
        };
        let key = Source::key;
        let pad = |button: SDL_GamepadButton| Source::PadButton(button.0);
        let axis = |axis: SDL_GamepadAxis, positive| Source::PadAxis(axis.0, positive);
        let mut bindings = Self::empty();
        for (action, sources) in [
            (
                Action::Up,
                vec![
                    key(SDL_SCANCODE_W),
                    key(SDL_SCANCODE_UP),
                    pad(SDL_GAMEPAD_BUTTON_DPAD_UP),
                    axis(SDL_GAMEPAD_AXIS_LEFTY, false),
                ],
            ),
            (
                Action::Down,
                vec![
                    key(SDL_SCANCODE_S),
                    key(SDL_SCANCODE_DOWN),
                    pad(SDL_GAMEPAD_BUTTON_DPAD_DOWN),
                    axis(SDL_GAMEPAD_AXIS_LEFTY, true),
                ],
            ),
            (
                Action::Left,
                vec![
                    key(SDL_SCANCODE_A),
                    key(SDL_SCANCODE_LEFT),
                    pad(SDL_GAMEPAD_BUTTON_DPAD_LEFT),
                    axis(SDL_GAMEPAD_AXIS_LEFTX, false),
                ],
            ),
            (
                Action::Right,
                vec![
                    key(SDL_SCANCODE_D),
                    key(SDL_SCANCODE_RIGHT),
                    pad(SDL_GAMEPAD_BUTTON_DPAD_RIGHT),
                    axis(SDL_GAMEPAD_AXIS_LEFTX, true),
                ],
            ),
            (
                Action::Confirm,
                vec![
                    key(SDL_SCANCODE_F),
                    key(SDL_SCANCODE_RETURN),
                    Source::Mouse(MOUSE_LEFT),
                    pad(SDL_GAMEPAD_BUTTON_SOUTH),
                ],
            ),
            (
                Action::Cancel,
                vec![
                    key(SDL_SCANCODE_ESCAPE),
                    key(SDL_SCANCODE_BACKSPACE),
                    pad(SDL_GAMEPAD_BUTTON_EAST),
                ],
            ),
            (
                Action::Run,
                vec![
                    key(SDL_SCANCODE_LSHIFT),
                    key(SDL_SCANCODE_RSHIFT),
                    pad(SDL_GAMEPAD_BUTTON_WEST),
                ],
            ),
            (
                Action::Aim,
                vec![
                    Source::Mouse(MOUSE_RIGHT),
                    axis(SDL_GAMEPAD_AXIS_LEFT_TRIGGER, true),
                ],
            ),
            (
                Action::Fire,
                vec![
                    key(SDL_SCANCODE_F),
                    key(SDL_SCANCODE_RETURN),
                    Source::Mouse(MOUSE_LEFT),
                    pad(SDL_GAMEPAD_BUTTON_SOUTH),
                    axis(SDL_GAMEPAD_AXIS_RIGHT_TRIGGER, true),
                ],
            ),
            (
                Action::Start,
                vec![
                    key(SDL_SCANCODE_TAB),
                    key(SDL_SCANCODE_I),
                    pad(SDL_GAMEPAD_BUTTON_NORTH),
                ],
            ),
            (
                Action::PageLeft,
                vec![
                    key(SDL_SCANCODE_LEFTBRACKET),
                    pad(SDL_GAMEPAD_BUTTON_LEFT_SHOULDER),
                ],
            ),
            (
                Action::PageRight,
                vec![
                    key(SDL_SCANCODE_RIGHTBRACKET),
                    pad(SDL_GAMEPAD_BUTTON_RIGHT_SHOULDER),
                ],
            ),
            (Action::QuickTurn, vec![key(SDL_SCANCODE_Q)]),
            (Action::Reload, vec![key(SDL_SCANCODE_R)]),
            (
                Action::Map,
                vec![key(SDL_SCANCODE_M), pad(SDL_GAMEPAD_BUTTON_BACK)],
            ),
            (Action::WeaponPrev, vec![Source::Wheel(Wheel::Up)]),
            (Action::WeaponNext, vec![Source::Wheel(Wheel::Down)]),
            (
                Action::TargetPrev,
                vec![
                    Source::Mouse(MOUSE_BACK),
                    pad(SDL_GAMEPAD_BUTTON_LEFT_SHOULDER),
                ],
            ),
            (
                Action::TargetNext,
                vec![
                    Source::Mouse(MOUSE_FORWARD),
                    pad(SDL_GAMEPAD_BUTTON_RIGHT_SHOULDER),
                ],
            ),
        ] {
            bindings.sources[action as usize] = sources;
        }
        // The number row: 1 to 9 equip slots 1 to 9, 0 slot 10.
        for (index, action) in EQUIP_SLOTS.into_iter().enumerate() {
            let scancode = if index < 9 {
                SDL_SCANCODE_1.0 + index as i32
            } else {
                SDL_SCANCODE_0.0
            };
            bindings.sources[action as usize] = vec![Source::Key(scancode)];
        }
        bindings
    }
}

/// The action pairs the shipped layout binds a source to on purpose: the
/// original's action button confirms and fires, and the shoulders page the
/// item box in its menu and cycle the auto-aim target while aiming.
#[cfg(test)]
const SHARED_PAIRS: [(Action, Action); 3] = [
    (Action::Confirm, Action::Fire),
    (Action::PageLeft, Action::TargetPrev),
    (Action::PageRight, Action::TargetNext),
];

impl Bindings {
    /// These bindings with every gamepad source replaced by the `control_type`
    /// preset's; the keyboard and mouse sources are kept.
    /// - A: the default layout (west runs, the triggers aim and fire, the
    ///   shoulders page and cycle the target).
    /// - B: the original PlayStation layout (the right shoulder aims, south
    ///   fires, the triggers page and cycle the target).
    /// - C: A with confirm and cancel swapped (east confirms and fires, south
    ///   cancels).
    /// - Custom: A, the layout a custom one starts from (the player's own
    ///   is kept apart and laid in with [`Bindings::with_pads_of`]).
    pub fn with_control_type(self, control_type: crate::config::ControlType) -> Self {
        use crate::config::ControlType;
        use sdl3_sys::gamepad::{
            SDL_GAMEPAD_AXIS_LEFT_TRIGGER, SDL_GAMEPAD_AXIS_RIGHT_TRIGGER, SDL_GAMEPAD_BUTTON_EAST,
            SDL_GAMEPAD_BUTTON_RIGHT_SHOULDER, SDL_GAMEPAD_BUTTON_SOUTH,
        };
        let pad = |button: SDL_GamepadButton| Source::PadButton(button.0);
        let axis = |axis: SDL_GamepadAxis| Source::PadAxis(axis.0, true);
        let mut layout = Self::default();
        match control_type {
            ControlType::A | ControlType::Custom => {}
            ControlType::B => {
                for (action, source) in [
                    (Action::Aim, pad(SDL_GAMEPAD_BUTTON_RIGHT_SHOULDER)),
                    (Action::Fire, pad(SDL_GAMEPAD_BUTTON_SOUTH)),
                    (Action::PageLeft, axis(SDL_GAMEPAD_AXIS_LEFT_TRIGGER)),
                    (Action::PageRight, axis(SDL_GAMEPAD_AXIS_RIGHT_TRIGGER)),
                    (Action::TargetPrev, axis(SDL_GAMEPAD_AXIS_LEFT_TRIGGER)),
                    (Action::TargetNext, axis(SDL_GAMEPAD_AXIS_RIGHT_TRIGGER)),
                ] {
                    layout.replace_pad(action, vec![source]);
                }
            }
            ControlType::C => {
                layout.replace_pad(Action::Confirm, vec![pad(SDL_GAMEPAD_BUTTON_EAST)]);
                layout.replace_pad(Action::Cancel, vec![pad(SDL_GAMEPAD_BUTTON_SOUTH)]);
                layout.replace_pad(
                    Action::Fire,
                    vec![
                        pad(SDL_GAMEPAD_BUTTON_EAST),
                        axis(SDL_GAMEPAD_AXIS_RIGHT_TRIGGER),
                    ],
                );
            }
        }
        self.with_pads_of(&layout)
    }

    /// These bindings with every gamepad source replaced by `layout`'s; the
    /// keyboard and mouse sources are kept.
    pub fn with_pads_of(mut self, layout: &Bindings) -> Self {
        // The layout's pad sources fill the old ones' slots in order, so the
        // keyboard and mouse slots stay where the rebind screen shows them.
        for action in ACTIONS {
            let mut pads = layout.get(action).iter().copied().filter(|s| s.is_pad());
            let mut sources = Vec::new();
            for &source in &self.sources[action as usize] {
                if !source.is_pad() {
                    sources.push(source);
                } else if let Some(pad) = pads.next() {
                    sources.push(pad);
                }
            }
            sources.extend(pads);
            self.sources[action as usize] = sources;
        }
        self
    }

    /// Replace `action`'s gamepad sources with `pads`.
    fn replace_pad(&mut self, action: Action, pads: Vec<Source>) {
        let sources = &mut self.sources[action as usize];
        sources.retain(|source| !source.is_pad());
        sources.extend(pads);
    }

    /// No action bound at all.
    pub fn empty() -> Self {
        Self {
            sources: Default::default(),
        }
    }

    /// The sources bound to `action`, in order.
    pub fn get(&self, action: Action) -> &[Source] {
        &self.sources[action as usize]
    }

    /// Replace `action`'s bindings (duplicates and reserved keys dropped).
    pub fn set(&mut self, action: Action, sources: Vec<Source>) {
        let list = &mut self.sources[action as usize];
        list.clear();
        for source in sources {
            if source.reserved_bit().is_none() && !list.contains(&source) {
                list.push(source);
            }
        }
    }

    /// Bind `source` to `action` at `slot`: replace that binding, or append
    /// when `slot` is past the end. Returns `false` (and changes nothing) for
    /// a reserved input, one that cannot be saved ([`Source::round_trips`])
    /// or a source the action already has.
    pub fn bind(&mut self, action: Action, slot: usize, source: Source) -> bool {
        let list = &mut self.sources[action as usize];
        if source.reserved_bit().is_some() || !source.round_trips() || list.contains(&source) {
            return false;
        }
        match list.get_mut(slot) {
            Some(existing) => *existing = source,
            None => list.push(source),
        }
        true
    }

    /// Remove `action`'s binding at `slot`; `false` when there is none.
    pub fn unbind(&mut self, action: Action, slot: usize) -> bool {
        let list = &mut self.sources[action as usize];
        if slot < list.len() {
            list.remove(slot);
            true
        } else {
            false
        }
    }

    /// The action bits `source` drives (plus the fixed bit of a reserved
    /// key).
    pub fn mask(&self, source: Source) -> u64 {
        let mut mask = source.reserved_bit().unwrap_or(0);
        for action in ACTIONS {
            if self.get(action).contains(&source) {
                mask |= action.bit();
            }
        }
        mask
    }

    /// The first essential action ([`Action::is_essential`]) left without a
    /// keyboard key of its own (bound to no other essential action), or
    /// `None` when every one has one. Such bindings could leave the menus,
    /// the rebind screen included, undrivable: a key shared with cancel
    /// cancels before it confirms, and a gamepad can be unplugged.
    pub fn lockout(&self) -> Option<Action> {
        let essentials = || ACTIONS.into_iter().filter(|action| action.is_essential());
        essentials().find(|&action| {
            !self.get(action).iter().any(|&source| {
                matches!(source, Source::Key(_))
                    && essentials()
                        .all(|other| other == action || !self.get(other).contains(&source))
            })
        })
    }

    /// The gamepad's counterpart of [`Bindings::lockout`]: the first of the
    /// menu actions (the directions, confirm and cancel) left without a
    /// controller button of its own, or the inventory left with none at
    /// all; `None` when the controller can drive every menu. The pad's
    /// start button always backs out, but a player with only a controller
    /// in hand needs these to get anywhere. The inventory may share cancel's
    /// button: a menu takes that press as cancel.
    pub fn pad_lockout(&self) -> Option<Action> {
        let menu = [
            Action::Up,
            Action::Down,
            Action::Left,
            Action::Right,
            Action::Confirm,
            Action::Cancel,
        ];
        menu.into_iter()
            .find(|&action| {
                !self.get(action).iter().any(|&source| {
                    source.is_pad()
                        && menu
                            .iter()
                            .all(|&other| other == action || !self.get(other).contains(&source))
                })
            })
            .or_else(|| {
                let start = self.get(Action::Start);
                (!start.iter().any(|source| source.is_pad())).then_some(Action::Start)
            })
    }

    /// The other actions `source` is also bound to, besides `action`.
    pub fn conflicts(&self, action: Action, source: Source) -> Vec<Action> {
        ACTIONS
            .into_iter()
            .filter(|&other| other != action && self.get(other).contains(&source))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_control_type_swaps_only_the_gamepad_sources() {
        use crate::config::ControlType;
        use sdl3_sys::gamepad::{
            SDL_GAMEPAD_AXIS_LEFT_TRIGGER, SDL_GAMEPAD_AXIS_RIGHT_TRIGGER, SDL_GAMEPAD_BUTTON_EAST,
            SDL_GAMEPAD_BUTTON_RIGHT_SHOULDER, SDL_GAMEPAD_BUTTON_SOUTH,
        };
        let mut bindings = Bindings::default();
        assert!(bindings.bind(Action::Aim, usize::MAX, Source::key(SDL_SCANCODE_W)));
        assert_eq!(bindings.clone().with_control_type(ControlType::A), bindings);
        let b = bindings.clone().with_control_type(ControlType::B);
        assert_eq!(
            b.get(Action::Aim),
            [
                Source::Mouse(MOUSE_RIGHT),
                Source::PadButton(SDL_GAMEPAD_BUTTON_RIGHT_SHOULDER.0),
                Source::key(SDL_SCANCODE_W),
            ]
        );
        assert_eq!(
            b.get(Action::Fire),
            [
                Source::key(SDL_SCANCODE_F),
                Source::key(SDL_SCANCODE_RETURN),
                Source::Mouse(MOUSE_LEFT),
                Source::PadButton(SDL_GAMEPAD_BUTTON_SOUTH.0),
            ]
        );
        assert_eq!(
            b.get(Action::TargetNext).last(),
            Some(&Source::PadAxis(SDL_GAMEPAD_AXIS_RIGHT_TRIGGER.0, true))
        );
        assert_eq!(
            b.get(Action::TargetPrev).last(),
            Some(&Source::PadAxis(SDL_GAMEPAD_AXIS_LEFT_TRIGGER.0, true))
        );
        assert_eq!(b.with_control_type(ControlType::A), bindings);
        let c = bindings.clone().with_control_type(ControlType::C);
        assert_eq!(
            c.get(Action::Confirm).last(),
            Some(&Source::PadButton(SDL_GAMEPAD_BUTTON_EAST.0))
        );
        assert_eq!(
            c.get(Action::Fire)[3..],
            [
                Source::PadButton(SDL_GAMEPAD_BUTTON_EAST.0),
                Source::PadAxis(SDL_GAMEPAD_AXIS_RIGHT_TRIGGER.0, true),
            ]
        );
        assert_eq!(c.with_control_type(ControlType::A), bindings);
    }

    #[test]
    fn default_keyboard_and_mouse_match_the_readme_controls() {
        let bindings = Bindings::default();
        let names = |action| -> Vec<String> {
            bindings
                .get(action)
                .iter()
                .filter(|source| !source.is_pad())
                .map(|source| source.to_string())
                .collect()
        };
        assert_eq!(names(Action::Up), ["key:W", "key:Up"]);
        assert_eq!(names(Action::Down), ["key:S", "key:Down"]);
        assert_eq!(names(Action::Left), ["key:A", "key:Left"]);
        assert_eq!(names(Action::Right), ["key:D", "key:Right"]);
        assert_eq!(
            names(Action::Confirm),
            ["key:F", "key:Return", "mouse:left"]
        );
        assert_eq!(names(Action::Fire), names(Action::Confirm));
        assert_eq!(names(Action::Cancel), ["key:Escape", "key:Backspace"]);
        assert_eq!(names(Action::Run), ["key:Left Shift", "key:Right Shift"]);
        assert_eq!(names(Action::Aim), ["mouse:right"]);
        assert_eq!(names(Action::Start), ["key:Tab", "key:I"]);
        assert_eq!(names(Action::PageLeft), ["key:["]);
        assert_eq!(names(Action::PageRight), ["key:]"]);
        assert_eq!(names(Action::QuickTurn), ["key:Q"]);
        assert_eq!(names(Action::Reload), ["key:R"]);
        assert_eq!(names(Action::Map), ["key:M"]);
        assert_eq!(names(Action::WeaponPrev), ["wheel:up"]);
        assert_eq!(names(Action::WeaponNext), ["wheel:down"]);
        assert_eq!(names(Action::TargetPrev), ["mouse:x1"]);
        assert_eq!(names(Action::TargetNext), ["mouse:x2"]);
        let slots: Vec<String> = EQUIP_SLOTS.into_iter().flat_map(names).collect();
        assert_eq!(
            slots,
            [
                "key:1", "key:2", "key:3", "key:4", "key:5", "key:6", "key:7", "key:8", "key:9",
                "key:0"
            ]
        );
        assert_eq!(
            bindings.get(Action::Map).last(),
            Some(&Source::PadButton(SDL_GAMEPAD_BUTTON_BACK.0))
        );
        let shared = |a: Action, b: Action| {
            SHARED_PAIRS
                .iter()
                .any(|&pair| pair == (a, b) || pair == (b, a))
        };
        for action in ACTIONS {
            for &source in bindings.get(action) {
                for other in bindings.conflicts(action, source) {
                    assert!(
                        shared(action, other),
                        "{source} is bound to both {action:?} and {other:?}"
                    );
                }
            }
        }
        assert_eq!(bindings.lockout(), None);
    }

    #[test]
    fn every_default_source_name_round_trips() {
        let bindings = Bindings::default();
        for action in ACTIONS {
            for &source in bindings.get(action) {
                let name = source.to_string();
                assert_eq!(Source::parse(&name).unwrap(), source, "{name}");
            }
        }
        for name in [
            "mouse:left",
            "mouse:x2",
            "wheel:up",
            "wheel:right",
            "pad:a",
            "pad:guide",
            "axis:righty-",
            "mouse:6",
        ] {
            assert_eq!(Source::parse(name).unwrap().to_string(), name);
        }
        assert_eq!(
            Source::parse("axis:lefttrigger+").unwrap().label(),
            "AXIS LEFTTRIGGER+"
        );
        assert_eq!(Source::parse("key:Space").unwrap().label(), "SPACE");
    }

    #[test]
    fn bad_and_reserved_names_are_rejected() {
        for name in [
            "Space",
            "key:NoSuchKey",
            "mouse:thumb",
            "wheel:sideways",
            "pad:nope",
            "axis:leftx",
            "axis:nope+",
            "joy:a",
            "key:F1",
            "key:F9",
            "pad:start",
            "key:#0",
            "key:#99999",
            "mouse:0",
            "pad:#99",
        ] {
            assert!(Source::parse(name).is_err(), "{name} must be rejected");
        }
    }

    #[test]
    fn nameless_inputs_are_saved_by_number_or_refused() {
        // Scancode 1 has no SDL name; it is written and read back by number.
        let nameless = Source::Key(1);
        assert_eq!(nameless.to_string(), "key:#1");
        assert!(nameless.round_trips());
        assert_eq!(Source::parse("pad:#0").unwrap(), Source::PadButton(0));
        assert_eq!(
            Source::parse("axis:#1-").unwrap(),
            Source::PadAxis(1, false)
        );
        let mut bindings = Bindings::default();
        assert!(bindings.bind(Action::Aim, usize::MAX, nameless));
        assert!(bindings.bind(Action::Aim, usize::MAX, Source::Mouse(9)));
        // SDL names RETURN2 `Return` too, which would reload as RETURN: such
        // an input is refused rather than saved as a different key.
        let return2 = Source::Key(158);
        assert_eq!(return2.to_string(), "key:Return");
        assert!(!return2.round_trips());
        assert!(!bindings.bind(Action::Aim, usize::MAX, return2));
    }

    #[test]
    fn an_essential_action_needs_a_key_of_its_own() {
        let mut bindings = Bindings::default();
        assert_eq!(bindings.lockout(), None);
        // Gamepad-only confirm: unplugging the pad would strand the menus.
        bindings.set(Action::Confirm, vec![Source::PadButton(0)]);
        assert_eq!(bindings.lockout(), Some(Action::Confirm));
        // A key shared with cancel cancels before it confirms.
        bindings.set(Action::Confirm, vec![Source::key(SDL_SCANCODE_ESCAPE)]);
        assert_eq!(bindings.lockout(), Some(Action::Confirm));
        // Sharing with a non-essential action is fine.
        bindings.set(Action::Confirm, vec![Source::key(SDL_SCANCODE_Q)]);
        assert_eq!(bindings.lockout(), None);
        bindings.set(Action::Start, Vec::new());
        assert_eq!(bindings.lockout(), Some(Action::Start));
    }

    #[test]
    fn one_action_takes_bindings_from_every_device() {
        let mut bindings = Bindings::default();
        let mouse = Source::Mouse(2);
        assert!(bindings.bind(Action::Aim, usize::MAX, mouse));
        assert!(
            !bindings.bind(Action::Aim, usize::MAX, mouse),
            "no duplicates"
        );
        assert_eq!(bindings.get(Action::Aim).len(), 3);
        assert_eq!(bindings.mask(mouse), Action::Aim.bit());
        // The same source on two actions drives both and is a conflict.
        assert!(bindings.bind(Action::Fire, 0, mouse));
        assert_eq!(bindings.mask(mouse), Action::Aim.bit() | Action::Fire.bit());
        assert_eq!(bindings.conflicts(Action::Aim, mouse), [Action::Fire]);
        assert!(
            !bindings
                .get(Action::Fire)
                .contains(&Source::key(SDL_SCANCODE_F)),
            "slot 0 replaced F"
        );
        assert!(bindings.unbind(Action::Fire, 0));
        assert!(!bindings.unbind(Action::Fire, 9));
        assert!(!bindings.bind(Action::Fire, 0, Source::key(SDL_SCANCODE_F1)));
        assert_eq!(
            bindings.mask(Source::key(SDL_SCANCODE_F1)),
            DEBUG_MENU_BIT,
            "F1 stays fixed"
        );
        let start = Source::PadButton(SDL_GAMEPAD_BUTTON_START.0);
        assert!(!bindings.bind(Action::Fire, 0, start));
        assert_eq!(
            bindings.mask(start),
            SYSTEM_MENU_BIT,
            "pad start stays fixed"
        );
        // Back on its own is an ordinary binding (the map by default).
        let back = Source::PadButton(SDL_GAMEPAD_BUTTON_BACK.0);
        assert_eq!(bindings.mask(back), Action::Map.bit());
        assert!(bindings.bind(Action::Fire, 0, back));
    }

    #[test]
    fn custom_starts_as_type_a_and_takes_any_layouts_pads() {
        use crate::config::ControlType;
        let mut bindings = Bindings::default();
        assert!(bindings.bind(Action::Aim, usize::MAX, Source::key(SDL_SCANCODE_W)));
        let a = bindings.clone().with_control_type(ControlType::A);
        assert_eq!(bindings.clone().with_control_type(ControlType::Custom), a);
        // A layout's pads, the keyboard kept.
        let c = Bindings::default().with_control_type(ControlType::C);
        let laid = bindings.clone().with_pads_of(&c);
        assert_eq!(laid, bindings.with_control_type(ControlType::C));
    }

    #[test]
    fn the_menus_need_a_controller_button_of_their_own() {
        use crate::config::ControlType;
        for control_type in [ControlType::A, ControlType::B, ControlType::C] {
            let bindings = Bindings::default().with_control_type(control_type);
            assert_eq!(bindings.pad_lockout(), None, "{control_type:?}");
        }
        let east = Source::PadButton(1);
        let mut bindings = Bindings::default();
        // The inventory may share cancel's button.
        let keys = |b: &Bindings, a| -> Vec<Source> {
            b.get(a).iter().copied().filter(|s| !s.is_pad()).collect()
        };
        let mut start = keys(&bindings, Action::Start);
        start.push(east);
        bindings.set(Action::Start, start);
        assert_eq!(bindings.pad_lockout(), None);
        // Confirm on cancel's button only: the menus could not confirm.
        let mut confirm = keys(&bindings, Action::Confirm);
        confirm.push(east);
        bindings.set(Action::Confirm, confirm);
        assert_eq!(bindings.pad_lockout(), Some(Action::Confirm));
        // No inventory button at all.
        let mut bindings = Bindings::default();
        bindings.set(Action::Start, keys(&bindings, Action::Start));
        assert_eq!(bindings.pad_lockout(), Some(Action::Start));
    }
}
