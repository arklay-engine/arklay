//! The native-resolution GUI: the in-game menu, its dialogs and the options
//! screen, and the toolkit they are built from.
//!
//! The game's own screens draw in the original's 320x240 space; this layer
//! draws at the window's pixel size over whatever the game shows, with
//! TrueType text rasterised at the size it appears. It is built in layers:
//!
//! * [`canvas`]: a premultiplied RGBA canvas and its anti-aliased primitives;
//! * [`text`]: the embedded faces, a lazy glyph cache, measuring and wrapping;
//! * [`theme`]: the one look: unit scale, font roles and palette;
//! * [`widgets`]: the shared painters (key caps, hints, tabs, rows, sliders)
//!   every screen is assembled from, so screens look alike by construction;
//! * [`options`]: the data model of the options screen, a flat list any
//!   owner can contribute to;
//! * the screens: [`dialog`], [`options_screen`], [`save_screen`] and
//!   [`system_menu`];
//! * [`overlay`]: the F6 debug HUD and the corner notices, drawn over
//!   everything else.
//!
//! Input arrives per presented frame as [`GuiEvent`]s, not per game tick, so
//! navigation repeats at the keyboard's rate and the pointer is live.

pub mod canvas;
pub mod dialog;
pub mod input;
pub mod key_config;
pub mod locale;
pub mod options;
pub mod options_screen;
pub mod overlay;
pub mod pad_diagram;
pub mod save_screen;
pub mod settings;
pub mod system_menu;
pub mod text;
pub mod theme;
pub mod widgets;

pub use canvas::Canvas;
pub use input::{Device, GuiEvent, GuiKey, PadStyle, Sound};
pub use overlay::{DebugMode, Overlay};
pub use settings::Settings;
pub use system_menu::{MenuRequest, SystemMenu};
pub use theme::FontFamily;
