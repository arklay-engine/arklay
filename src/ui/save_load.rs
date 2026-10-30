//! The save/load screen's engine side.
//!
//! The screen itself is `games/re1/ui/save_load.lua` (a port of the
//! original's per-tick state machine), or the pack entry a mod's scripts
//! registered with `api:register_save_screen`. Both run as a
//! [`LuaScreen`] in load or save mode: the title's load path opens it in
//! load mode, and a room typewriter (or the next-cycle ending save) opens it
//! in save mode over the frozen gameplay frame.
//!
//! Unless a mod registered its own, the modern screen
//! ([`super::modern_save`]) stands in for the shipped one while the Save
//! Screen option picks it.

use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;

use crate::game::InventoryItem;
use crate::items;
use crate::save::{PLAYER_SLOT_COUNT, SaveFile};

use super::Screen;
use super::UiContext;
use super::lua_screen::LuaScreen;
use super::modern_save::ModernSaveScreen;
use crate::gui;

/// Whether the modern screen stands in for the shipped one.
/// Scripted and headless runs keep the shipped screen; the interactive app
/// sets it from `arklay.toml`.
static MODERN: AtomicBool = AtomicBool::new(false);

/// Pick the modern screen (or the shipped one) for the screens opened from
/// now on.
pub fn set_modern(modern: bool) {
    MODERN.store(modern, Ordering::Relaxed);
}

/// Whether the modern screen opens: picked, and not an `--authentic` run.
fn modern(cx: &UiContext<'_>) -> bool {
    let authentic = cx
        .game
        .map_or(cx.authentic, |game| game.player_options.authentic);
    MODERN.load(Ordering::Relaxed) && !authentic
}

/// The shipped save/load screen script.
const SAVE_LOAD_ENTRY: &str = "ui/save_load.lua";

/// A completed save: the slot that was written and whether the flow consumed
/// an ink ribbon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SaveOutcome {
    /// The zero-based slot `api:save_slot` wrote; `None` when the screen
    /// named its own file with `api:save`.
    pub slot: Option<usize>,
    /// Whether one ink ribbon must be consumed from the live game state.
    pub ink_ribbon: bool,
}

/// A save/load screen the engine can open.
///
/// The screen writes (or loads) its files itself and reports a completed
/// write through [`SaveScreen::take_outcome`]; the engine mirrors the write
/// into the live state (ink ribbon, save counter).
pub trait SaveScreen: Screen {
    /// The completed save, once the screen left through a write.
    fn take_outcome(&mut self) -> Option<SaveOutcome>;

    /// The screen's native-resolution side, when it draws at the window's
    /// size and takes its input per frame.
    fn native(&mut self) -> Option<&mut dyn NativeScreen> {
        None
    }

    /// The canvas the native side last drew.
    fn native_canvas(&self) -> Option<&gui::Canvas> {
        None
    }
}

/// A screen drawn at the window's resolution: the app hands it the menus'
/// settings, each presented frame's GUI input, and draws it each frame.
pub trait NativeScreen {
    /// Whether [`NativeScreen::configure`] has yet to run.
    fn needs_settings(&self) -> bool;

    /// Take the menus' settings (face, text size, language, button style).
    fn configure(&mut self, settings: gui::Settings);

    /// Feed one frame's events; returns the sounds to play.
    fn handle(&mut self, events: &[gui::GuiEvent]) -> Vec<gui::Sound>;

    /// Draw at `width` x `height` pixels, recorded for the GPU when `gpu`;
    /// returns whether anything was repainted.
    fn draw(&mut self, width: u32, height: u32, gpu: bool) -> bool;
}

/// Open the load screen: the pack entry its scripts registered, the modern
/// screen, or the shipped screen. A registered screen that cannot load falls back to the
/// shipped one with a warning, so a broken mod never leaves the player
/// without a screen.
pub fn open_load_screen(
    entry: Option<&str>,
    cx: &mut UiContext<'_>,
) -> Result<Box<dyn SaveScreen>> {
    if let Some(entry) = entry {
        match LuaScreen::open_load(entry, cx) {
            Ok(screen) => return Ok(Box::new(screen)),
            Err(err) => eprintln!("warning: lua save screen {entry} unavailable: {err:#}"),
        }
    } else if modern(cx) {
        match ModernSaveScreen::open_load(cx) {
            Ok(screen) => return Ok(Box::new(screen)),
            Err(err) => eprintln!("warning: modern load screen unavailable: {err:#}"),
        }
    }
    Ok(Box::new(LuaScreen::open_load(SAVE_LOAD_ENTRY, cx)?))
}

/// Open the save screen for the pending `file`: the pack entry its scripts
/// registered, the modern screen, or the shipped screen. `ink_ribbon` makes the write consume
/// one.
pub fn open_save_screen(
    entry: Option<&str>,
    cx: &mut UiContext<'_>,
    file: SaveFile,
    ink_ribbon: bool,
) -> Result<Box<dyn SaveScreen>> {
    if let Some(entry) = entry {
        match LuaScreen::open_save(entry, cx, file.clone(), ink_ribbon) {
            Ok(screen) => return Ok(Box::new(screen)),
            Err(err) => eprintln!("warning: lua save screen {entry} unavailable: {err:#}"),
        }
    } else if modern(cx) {
        match ModernSaveScreen::open_save(cx, file.clone(), ink_ribbon) {
            Ok(screen) => return Ok(Box::new(screen)),
            Err(err) => eprintln!("warning: modern save screen unavailable: {err:#}"),
        }
    }
    Ok(Box::new(LuaScreen::open_save(
        SAVE_LOAD_ENTRY,
        cx,
        file,
        ink_ribbon,
    )?))
}

/// Remove one ink ribbon from the block the screen is about to write, the way
/// the original's `use_room_action_item` does before assembling the file.
pub(crate) fn consume_snapshot_ribbon(file: &mut SaveFile) {
    let ribbon = if file.re2.is_some() {
        crate::scd2::host::ITEM_INK_RIBBON
    } else {
        items::ITEM_INK_RIBBONS
    };
    let Some(index) = file
        .player_slots
        .iter()
        .position(|stack| stack.id == ribbon && stack.quantity > 0)
    else {
        return;
    };
    file.used_item = ribbon;
    file.player_slots[index].quantity -= 1;
    if file.player_slots[index].quantity == 0 {
        for read in index + 1..PLAYER_SLOT_COUNT {
            file.player_slots[read - 1] = file.player_slots[read];
        }
        file.player_slots[PLAYER_SLOT_COUNT - 1] = InventoryItem::default();
        file.total_held = file.total_held.saturating_sub(1);
    }
}
