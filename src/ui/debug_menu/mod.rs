//! The port-only F1 debug menu.
//!
//! This is a development aid the retail game never shipped. F1 while playing
//! freezes the room and opens a tool list: ROOM SELECT, INVENTORY, PLAYER,
//! FLAGS, CHEATS, SAVE, ITEM BOX and CLEAR GAME. Up/down move the cursor,
//! confirm enters a tool, cancel backs out one level (tool, main list, closed)
//! and F1 closes the menu from anywhere. The menu is always available
//! (`--debug-menu` is accepted for compatibility only).
//!
//! The menu draws at the window's resolution with the in-game menu's faces,
//! theme and input hints, and takes its input per presented frame as
//! [`GuiEvent`]s (key repeat and the pointer included); each key is mapped
//! onto one [`UiInput`] edge for the tools' state machines. Hovering a row
//! or cell picks it, clicking activates it and the wheel scrolls the view.
//! F1 closes it from anywhere and reopening returns to
//! the tool and cursors it was left on; the room browser also follows a room
//! change made while the menu was closed. Edits that only touch [`GameState`]
//! (inventory, flags, cheats, the pending player selection) happen inside
//! [`DebugMenu::handle_input`]; everything that needs the room-load path, the
//! save directory or the app is returned as a [`DebugMenuEvent`].
//!
//! The reference has an F1 menu of its own; its flag editor's views and names
//! are mirrored, while the room grid, the icon picker and the cheats are
//! port-designed.

mod art;
mod flags;
mod inventory;
mod rooms;
mod widgets;

pub use flags::{FlagEditor, FlagGroup, FlagName, GROUPS, flag_name};
pub use inventory::{InventoryEditor, Picker};
pub use rooms::{RoomSelect, RoomVariants, room_entries, room_variants};

use std::sync::Arc;

use crate::ending;
use crate::game::{self, GameState, InfiniteAmmo};
use crate::gui::canvas::{Axis, Canvas, Color, Picture, Rect};
use crate::gui::input::{Device, GuiEvent, GuiKey, Sound};
use crate::gui::locale::Locale;
use crate::gui::text::{Align, Fonts};
use crate::gui::theme::{Role, Theme, palette};
use crate::gui::widgets::{Chevron, Painter};
use crate::gui::{self, FontFamily};
use crate::pack::Pack;
use crate::state::RoomId;
use crate::text::Text;
use crate::ui::UiInput;

use art::Art;
use widgets::{Label, ROW_H, ROW_W, Value, step_choice};

/// What one tick of the menu decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugMenuEvent {
    /// The input was consumed inside the menu.
    None,
    /// Close the menu and resume the frozen room.
    Close,
    /// Load `RoomId` and place the player.
    Jump(RoomId),
    /// Open the save screen in save mode (no ink ribbon needed).
    SaveGame,
    /// Open the item box.
    ItemBox,
    /// Start ending `id`'s film chain.
    ClearGame(u8),
}

/// The main list's tools, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    /// The stage/room browser.
    Rooms,
    /// The inventory editor.
    Inventory,
    /// Character and outfit selection.
    Player,
    /// The flag editor.
    Flags,
    /// The cheat toggles.
    Cheats,
    /// Open the save screen.
    Save,
    /// Open the item box.
    ItemBox,
    /// The ending chooser.
    ClearGame,
}

/// The main list, in order.
pub const TOOLS: [Tool; 8] = [
    Tool::Rooms,
    Tool::Inventory,
    Tool::Player,
    Tool::Flags,
    Tool::Cheats,
    Tool::Save,
    Tool::ItemBox,
    Tool::ClearGame,
];

impl Tool {
    /// The tool's label key (also its title).
    fn label(self) -> &'static str {
        match self {
            Tool::Rooms => "UI_DEBUG_TOOL_ROOMS",
            Tool::Inventory => "UI_DEBUG_TOOL_INVENTORY",
            Tool::Player => "UI_DEBUG_TOOL_PLAYER",
            Tool::Flags => "UI_DEBUG_TOOL_FLAGS",
            Tool::Cheats => "UI_DEBUG_TOOL_CHEATS",
            Tool::Save => "UI_DEBUG_SAVE_GAME",
            Tool::ItemBox => "UI_DEBUG_TOOL_ITEM_BOX",
            Tool::ClearGame => "UI_DEBUG_TOOL_CLEAR_GAME",
        }
    }
}

/// The PLAYER tool's character names, by character.
const CHARACTER_NAMES: [&str; 4] = ["Chris", "Jill", "Barry", "Rebecca"];
/// The PLAYER tool's outfit names, by [`PlayerChoice::costume`].
const COSTUME_NAMES: [&str; 3] = [
    "UI_DEBUG_COSTUME_NORMAL",
    "UI_DEBUG_COSTUME_ALT_1",
    "UI_DEBUG_COSTUME_ALT_2",
];
/// The CHEATS rows.
const CHEAT_NAMES: [&str; 3] = [
    "UI_DEBUG_CHEAT_INVINCIBLE",
    "UI_DEBUG_CHEAT_GHOST",
    "UI_DEBUG_CHEAT_AMMO",
];
/// The CLEAR GAME route hints, by ending id minus one.
const ENDING_HINTS: [&str; 7] = [
    "UI_DEBUG_ENDING_1",
    "UI_DEBUG_ENDING_2",
    "UI_DEBUG_ENDING_3",
    "UI_DEBUG_ENDING_4",
    "UI_DEBUG_ENDING_5",
    "UI_DEBUG_ENDING_6",
    "UI_DEBUG_ENDING_7",
];

/// What a tool's tick decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    /// Stay in the tool.
    Stay,
    /// Back out to the main list.
    Back,
    /// Hand an event to the engine.
    Event(DebugMenuEvent),
}

/// The ending the live state would play: [`ending::select_id`] over bank 1's
/// partner and second-survivor bits and the current character.
pub fn default_ending(game: &GameState) -> u8 {
    ending::select_id(
        game.flag_test(
            game::BANK_SCENARIO2,
            ending::SCENARIO2_FLAG_PARTNER_ALIVE,
            false,
        ),
        game.flag_test(
            game::BANK_SCENARIO2,
            ending::SCENARIO2_FLAG_SECOND_SURVIVOR,
            false,
        ),
        game::scenario_of(game.player_choice().character),
    )
}

/// The RDT variant the room select jumps in: the pending selection's
/// scenario, else the current one.
pub fn jump_character(game: &GameState) -> u8 {
    game::scenario_of(
        game.debug
            .pending_player
            .unwrap_or_else(|| game.player_choice())
            .character,
    )
}

/// The native view: its canvas, faces, settings and pointer targets.
struct View {
    fonts: Option<Fonts>,
    canvas: Canvas,
    settings: Option<gui::Settings>,
    locale: Locale,
    device: Device,
    /// What the pointer can click; later rectangles sit on top.
    hits: Vec<(Rect, Hit)>,
    art: Art,
    dirty: bool,
}

/// The menu's live state: the open tool and every tool's own state.
pub struct DebugMenu {
    tool: Option<Tool>,
    main_cursor: usize,
    rooms: RoomSelect,
    inventory: InventoryEditor,
    player_row: usize,
    flags: FlagEditor,
    cheat_row: usize,
    ending_row: usize,
    view: View,
}

impl DebugMenu {
    /// Open the main list with every tool seeded from the live state.
    pub fn open(pack: &Pack, current: RoomId, game: &GameState) -> Self {
        Self {
            tool: None,
            main_cursor: 0,
            rooms: RoomSelect::new(pack, current),
            inventory: InventoryEditor::default(),
            player_row: 0,
            flags: FlagEditor::default(),
            cheat_row: 0,
            ending_row: usize::from(default_ending(game).clamp(1, 7) - 1),
            view: View {
                fonts: Fonts::new()
                    .map_err(|err| eprintln!("warning: debug menu fonts: {err:#}"))
                    .ok(),
                canvas: Canvas::new(1, 1),
                settings: None,
                locale: Locale::default(),
                device: Device::Keyboard,
                hits: Vec::new(),
                art: Art::default(),
                dirty: true,
            },
        }
    }

    /// Reattach the menu to the live session after it was closed: the open
    /// tool and every cursor are kept, while the room browser follows
    /// `current` if the room changed and the inventory cursor is clamped to
    /// the live capacity.
    pub fn reopen(&mut self, current: RoomId, game: &GameState) {
        // Settings may have changed while closed; take them again.
        self.view.settings = None;
        self.rooms.refresh(current);
        self.inventory.refresh(game);
        self.view.dirty = true;
    }

    /// The open tool (`None` on the main list).
    pub fn tool(&self) -> Option<Tool> {
        self.tool
    }

    /// The main list's cursor.
    pub fn main_cursor(&self) -> usize {
        self.main_cursor
    }

    /// The room browser's state.
    pub fn rooms(&self) -> &RoomSelect {
        &self.rooms
    }

    /// The inventory editor's state.
    pub fn inventory(&self) -> &InventoryEditor {
        &self.inventory
    }

    /// The flag editor's state.
    pub fn flags(&self) -> &FlagEditor {
        &self.flags
    }

    /// The CLEAR GAME cursor's ending id.
    pub fn ending(&self) -> u8 {
        self.ending_row as u8 + 1
    }

    /// Whether [`Self::configure`] has yet to run.
    pub fn needs_settings(&self) -> bool {
        self.view.settings.is_none()
    }

    /// Take the menus' settings (face, text size, language, button style).
    pub fn configure(&mut self, settings: gui::Settings) {
        let tag = crate::language::resolve_tag(settings.language.as_deref());
        if self.view.locale.tag() != tag {
            self.view.locale = Locale::new(&tag);
        }
        self.view.settings = Some(settings);
        self.view.dirty = true;
    }

    /// The canvas the last [`Self::draw`] painted.
    pub fn canvas(&self) -> &Canvas {
        &self.view.canvas
    }

    /// Everything a key can change: the cursors and the state the tools
    /// edit, to tell a key that did something from one that did not.
    fn snapshot(&self, game: &GameState) -> Snapshot {
        let (group, sel) = self.flags.selected();
        Snapshot {
            cursors: [
                self.main_cursor,
                self.rooms.column(),
                self.rooms.row(),
                self.inventory.slot(),
                usize::from(self.inventory.on_size()),
                self.inventory
                    .picker()
                    .map_or(usize::MAX, |p| usize::from(p.item())),
                self.player_row,
                self.flags.group(),
                self.flags.row(),
                self.cheat_row,
                self.ending_row,
            ],
            tool: self.tool,
            debug: game.debug,
            inventory: game.inventory.clone(),
            flag: game.flag_test(group.bank, sel, false),
        }
    }

    /// How deep the menu is: 0 the main list, 1 a tool, 2 the item picker.
    fn depth(&self) -> u8 {
        match self.tool {
            None => 0,
            Some(Tool::Inventory) if self.inventory.picker().is_some() => 2,
            Some(_) => 1,
        }
    }

    /// Feed one presented frame's GUI events against the live state; returns
    /// the events that leave the menu (the first one ends the frame's input)
    /// and the feedback sounds.
    pub fn handle_events(
        &mut self,
        events: &[GuiEvent],
        game: &mut GameState,
    ) -> (Vec<DebugMenuEvent>, Vec<Sound>) {
        let mut out = Vec::new();
        let mut sounds = Vec::new();
        for &event in events {
            if !matches!(event, GuiEvent::PointerMove { .. }) {
                self.view.dirty = true;
            }
            let depth = self.depth();
            let before = self.snapshot(game);
            let result = match event {
                GuiEvent::Key(key, device) => {
                    self.view.device = device;
                    Some(self.handle_input(key_input(key), game))
                }
                GuiEvent::PointerDown { x, y } => match self.hit(x, y) {
                    Some(Hit::Item(index)) => {
                        self.point(index, game);
                        Some(self.handle_input(key_input(GuiKey::Confirm), game))
                    }
                    Some(Hit::Decrease(index)) => {
                        self.point(index, game);
                        Some(self.handle_input(key_input(GuiKey::Left), game))
                    }
                    Some(Hit::Increase(index)) => {
                        self.point(index, game);
                        Some(self.handle_input(key_input(GuiKey::Right), game))
                    }
                    Some(Hit::Tab(index)) => {
                        self.set_tab(index);
                        Some(DebugMenuEvent::None)
                    }
                    Some(Hit::Key(key)) => Some(self.handle_input(key_input(key), game)),
                    None => None,
                },
                GuiEvent::Wheel(notches) => {
                    // The wheel scrolls the view; the cursor stays put.
                    if notches != 0.0 {
                        self.scroll_by(if notches > 0.0 { -1 } else { 1 });
                    }
                    None
                }
                GuiEvent::PointerMove { x, y } => {
                    // Hovering a row or cell picks it, as in the options.
                    if let Some(Hit::Item(index) | Hit::Decrease(index) | Hit::Increase(index)) =
                        self.hit(x, y)
                    {
                        self.point(index, game);
                        self.view.dirty = true;
                    }
                    None
                }
                GuiEvent::PointerUp { .. } => None,
            };
            let Some(result) = result else {
                continue;
            };
            let after = self.depth();
            let sound = match result {
                DebugMenuEvent::Close => Sound::Cancel,
                DebugMenuEvent::None if after < depth => Sound::Cancel,
                DebugMenuEvent::None if after == depth => {
                    // A key that changed nothing (an end stop) stays quiet.
                    if self.snapshot(game) == before {
                        continue;
                    }
                    Sound::Cursor
                }
                _ => Sound::Decide,
            };
            sounds.push(sound);
            if result != DebugMenuEvent::None {
                out.push(result);
                break;
            }
        }
        let volume = self
            .view
            .settings
            .as_ref()
            .map_or(1, |settings| settings.game.menu_volume);
        if volume == 0 {
            sounds.clear();
        }
        (out, sounds)
    }

    /// The pointer target under `(x, y)`.
    fn hit(&self, x: f32, y: f32) -> Option<Hit> {
        self.view
            .hits
            .iter()
            .rev()
            .find(|(rect, _)| rect.contains(x, y))
            .map(|&(_, hit)| hit)
    }

    /// Switch the open tool's tab strip to tab `index`.
    fn set_tab(&mut self, index: usize) {
        match self.tool {
            Some(Tool::Rooms) => self.rooms.set_stage(index),
            Some(Tool::Flags) => self.flags.set_group(index),
            _ => {}
        }
    }

    /// Scroll the open list by `rows` without moving its cursor.
    fn scroll_by(&mut self, rows: isize) {
        match self.tool {
            Some(Tool::Rooms) => self.rooms.scroll_by(rows),
            Some(Tool::Inventory) => self.inventory.scroll_by(rows),
            Some(Tool::Flags) => self.flags.scroll_by(rows * 3),
            _ => {}
        }
    }

    /// Put the open list's cursor on `index`.
    fn point(&mut self, index: usize, game: &GameState) {
        match self.tool {
            None => self.main_cursor = index.min(TOOLS.len() - 1),
            Some(Tool::Rooms) => self.rooms.point(index),
            Some(Tool::Inventory) => self.inventory.point(index, game),
            Some(Tool::Player) => self.player_row = index.min(1),
            Some(Tool::Flags) => self.flags.point(index),
            Some(Tool::Cheats) => self.cheat_row = index.min(CHEAT_NAMES.len() - 1),
            Some(Tool::ClearGame) => self.ending_row = index.min(ENDING_HINTS.len() - 1),
            Some(Tool::Save | Tool::ItemBox) => {}
        }
    }

    /// One tick of edge-triggered menu input against the live state.
    pub fn handle_input(&mut self, input: UiInput, game: &mut GameState) -> DebugMenuEvent {
        if input.debug_menu {
            return DebugMenuEvent::Close;
        }
        let Some(tool) = self.tool else {
            return self.handle_main(input);
        };
        let step = match tool {
            Tool::Rooms => self.rooms.handle_input(input, jump_character(game)),
            Tool::Inventory => self.inventory.handle_input(input, game),
            Tool::Player => self.handle_player(input, game),
            Tool::Flags => self.flags.handle_input(input, game),
            Tool::Cheats => self.handle_cheats(input, game),
            Tool::ClearGame => self.handle_clear(input),
            Tool::Save | Tool::ItemBox => Step::Back,
        };
        match step {
            Step::Stay => DebugMenuEvent::None,
            Step::Back => {
                self.tool = None;
                DebugMenuEvent::None
            }
            Step::Event(event) => event,
        }
    }

    /// The main list: move, enter a tool, or close.
    fn handle_main(&mut self, input: UiInput) -> DebugMenuEvent {
        let count = TOOLS.len();
        if input.cancel {
            return DebugMenuEvent::Close;
        }
        if input.up {
            self.main_cursor = (self.main_cursor + count - 1) % count;
        } else if input.down {
            self.main_cursor = (self.main_cursor + 1) % count;
        } else if input.confirm {
            match TOOLS[self.main_cursor] {
                Tool::ItemBox => return DebugMenuEvent::ItemBox,
                Tool::Save => return DebugMenuEvent::SaveGame,
                tool => self.tool = Some(tool),
            }
        }
        DebugMenuEvent::None
    }

    /// PLAYER: up/down pick the field, left/right/confirm change it. The
    /// change stays pending until the next room load; picking the current
    /// character and outfit again drops it.
    fn handle_player(&mut self, input: UiInput, game: &mut GameState) -> Step {
        if input.cancel {
            return Step::Back;
        }
        let current = game.player_choice();
        let mut choice = game.debug.pending_player.unwrap_or(current);
        // Barry and Rebecca wear their one outfit: no OUTFIT row.
        if input.up || input.down {
            if choice.character & 3 < 2 {
                self.player_row ^= 1;
            }
            return Step::Stay;
        }
        if self.player_row == 0 {
            if let Some(next) = step_choice(usize::from(choice.character & 3), 4, input) {
                choice.character = next as u8;
            }
        } else if let Some(next) = step_choice(usize::from(choice.costume.min(2)), 3, input) {
            choice.costume = next as u8;
        }
        game.debug.pending_player = (choice != current).then_some(choice);
        if choice.character & 3 >= 2 {
            self.player_row = 0;
        }
        Step::Stay
    }

    /// CHEATS: up/down pick the row, left/right/confirm step it (INFINITE
    /// AMMO through off/reload/infinite). Turning INFINITE on fills the
    /// equipped clip at once.
    fn handle_cheats(&mut self, input: UiInput, game: &mut GameState) -> Step {
        if input.cancel {
            return Step::Back;
        }
        let count = CHEAT_NAMES.len();
        if input.up {
            self.cheat_row = (self.cheat_row + count - 1) % count;
        } else if input.down {
            self.cheat_row = (self.cheat_row + 1) % count;
        } else {
            let debug = &mut game.debug;
            match self.cheat_row {
                0 => {
                    if let Some(on) = step_choice(usize::from(debug.invincible), 2, input) {
                        debug.invincible = on == 1;
                    }
                }
                1 => {
                    if let Some(on) = step_choice(usize::from(debug.ghost), 2, input) {
                        debug.ghost = on == 1;
                    }
                }
                2 => {
                    if let Some(mode) = step_choice(ammo_index(debug.infinite_ammo), 3, input) {
                        debug.infinite_ammo = AMMO_MODES[mode];
                        if debug.infinite_ammo == InfiniteAmmo::Infinite {
                            crate::weapons::refill_clip(game);
                        }
                    }
                }
                _ => {}
            }
        }
        Step::Stay
    }

    /// CLEAR GAME: pick an ending id and start it.
    fn handle_clear(&mut self, input: UiInput) -> Step {
        let count = ENDING_HINTS.len();
        if input.cancel {
            return Step::Back;
        }
        if input.up {
            self.ending_row = (self.ending_row + count - 1) % count;
        } else if input.down {
            self.ending_row = (self.ending_row + 1) % count;
        } else if input.confirm {
            return Step::Event(DebugMenuEvent::ClearGame(self.ending()));
        }
        Step::Stay
    }

    /// Paint the menu at `width` x `height` pixels into [`Self::canvas`],
    /// returning whether anything was repainted. With `gpu` the canvas
    /// records its frame for the GPU backend.
    pub fn draw(
        &mut self,
        width: u32,
        height: u32,
        gpu: bool,
        pack: &Pack,
        text: &Text,
        game: &GameState,
    ) -> bool {
        let (width, height) = (width.max(1), height.max(1));
        let view = &mut self.view;
        view.dirty |= view.canvas.set_recording(gpu);
        let resized = (view.canvas.width, view.canvas.height) != (width, height);
        let Some(fonts) = view.fonts.as_mut() else {
            return false;
        };
        if resized {
            fonts.clear_cache();
        }
        if !view.dirty && !resized {
            return false;
        }
        view.dirty = false;
        view.canvas
            .reset_to(width, height, Color::rgba(0x00, 0x00, 0x00, 0xB8));
        let (family, text_scale) = view
            .settings
            .as_ref()
            .map_or((FontFamily::Modern, 1.0), |s| {
                (s.game.ui_font, s.game.menu_text_size.scale())
            });
        let theme = Theme::new(width, height, family).with_text_scale(text_scale);
        let device = match (
            view.device,
            view.settings.as_ref().and_then(|s| s.game.button_style),
        ) {
            (Device::Gamepad(_), Some(style)) => Device::Gamepad(style),
            (device, _) => device,
        };
        let mut p = Painter {
            canvas: &mut view.canvas,
            fonts,
            theme,
        };
        let mut cx = Cx {
            pack,
            text,
            game,
            locale: &view.locale,
            art: &mut view.art,
            hits: &mut view.hits,
            device,
            more: false,
        };
        cx.hits.clear();
        let bounds = p.canvas.bounds();
        let u = |v: f32| theme.u(v);
        // A darker band down the left, where the lists sit.
        p.canvas.fill_gradient(
            Rect::new(0.0, 0.0, bounds.w * 0.6, bounds.h),
            Axis::Horizontal,
            Color::rgba(0, 0, 0, 120),
            Color::rgba(0, 0, 0, 0),
        );
        let left = (bounds.w * 0.06).max(u(48.0));
        let right = bounds.w - left;
        // The heading: the menu, then the open tool.
        let title = cx.label("UI_DEBUG_TITLE");
        let title_y = u(96.0);
        let mut x = left;
        match self.tool {
            None => {
                p.text(Role::Title, x, title_y, Align::Left, palette::TEXT, &title);
            }
            Some(tool) => {
                x += p.text(
                    Role::Title,
                    x,
                    title_y,
                    Align::Left,
                    palette::TEXT_DIM,
                    &title,
                );
                x += u(18.0);
                p.chevron(x + u(6.0), title_y, 26.0, Chevron::Right, palette::TEXT_DIM);
                x += u(30.0);
                let name = if tool == Tool::Inventory && self.inventory.picker().is_some() {
                    cx.label("UI_DEBUG_PICKER_TITLE")
                } else {
                    cx.label(tool.label())
                };
                p.text(Role::Title, x, title_y, Align::Left, palette::TEXT, &name);
            }
        }
        let rule_y = u(140.0);
        p.canvas.fill_gradient(
            Rect::new(left, rule_y, right - left, u(1.5).max(1.0)),
            Axis::Horizontal,
            palette::RULE_BRIGHT.fade(0.8),
            palette::RULE.fade(0.0),
        );
        let hint_y = u(1040.0).min(bounds.h - u(40.0));
        let area = Rect::new(
            left,
            rule_y + u(30.0),
            right - left,
            hint_y - u(60.0) - (rule_y + u(30.0)),
        );
        let hints: Vec<(GuiKey, String)> = match self.tool {
            None => {
                Self::draw_main(self.main_cursor, &mut p, &mut cx, area);
                vec![
                    (GuiKey::Confirm, cx.gui("hint.select")),
                    (GuiKey::Cancel, cx.gui("debug.close")),
                ]
            }
            Some(Tool::Rooms) => {
                self.rooms.draw(&mut p, &mut cx, area);
                vec![
                    (GuiKey::Confirm, cx.gui("debug.jump")),
                    (GuiKey::Cancel, cx.gui("hint.back")),
                ]
            }
            Some(Tool::Inventory) => {
                self.inventory.draw(&mut p, &mut cx, area);
                if self.inventory.picker().is_some() {
                    vec![
                        (GuiKey::Confirm, cx.gui("debug.pick")),
                        (GuiKey::Cancel, cx.gui("hint.back")),
                    ]
                } else if self.inventory.on_size() {
                    vec![
                        (GuiKey::Left, cx.gui("debug.change")),
                        (GuiKey::Cancel, cx.gui("hint.back")),
                    ]
                } else {
                    vec![
                        (GuiKey::Confirm, cx.gui("debug.choose_item")),
                        (GuiKey::Left, cx.gui("debug.quantity")),
                        (GuiKey::ResetAll, cx.gui("hint.clear")),
                        (GuiKey::Cancel, cx.gui("hint.back")),
                    ]
                }
            }
            Some(Tool::Player) => {
                Self::draw_player(self.player_row, &mut p, &mut cx, area);
                vec![
                    (GuiKey::Left, cx.gui("debug.change")),
                    (GuiKey::Cancel, cx.gui("hint.back")),
                ]
            }
            Some(Tool::Flags) => {
                self.flags.draw(&mut p, &mut cx, area);
                vec![
                    (GuiKey::Confirm, cx.gui("debug.toggle")),
                    (GuiKey::Left, cx.gui("debug.change")),
                    (GuiKey::Cancel, cx.gui("hint.back")),
                ]
            }
            Some(Tool::Cheats) => {
                Self::draw_cheats(self.cheat_row, &mut p, &mut cx, area);
                vec![
                    (GuiKey::Left, cx.gui("debug.change")),
                    (GuiKey::Cancel, cx.gui("hint.back")),
                ]
            }
            Some(Tool::ClearGame) => {
                Self::draw_clear(self.ending_row, &mut p, &mut cx, area);
                vec![
                    (GuiKey::Confirm, cx.gui("debug.play_ending")),
                    (GuiKey::Cancel, cx.gui("hint.back")),
                ]
            }
            Some(Tool::Save | Tool::ItemBox) => Vec::new(),
        };
        let pairs: Vec<(GuiKey, &str)> = hints.iter().map(|(k, s)| (*k, s.as_str())).collect();
        let rects = p.hint_bar(bounds.w / 2.0, hint_y, device, &pairs);
        cx.hits.extend(
            rects
                .into_iter()
                .zip(hints.iter().map(|(k, _)| Hit::Key(*k))),
        );
        // Art still loading: paint again next frame.
        view.dirty |= cx.more;
        true
    }

    fn draw_main(cursor: usize, p: &mut Painter<'_>, cx: &mut Cx<'_>, area: Rect) {
        let theme = p.theme;
        let u = |v: f32| theme.u(v);
        let row_h = u(78.0);
        let width = (area.w * 0.45).min(u(560.0));
        for (index, tool) in TOOLS.iter().enumerate() {
            let rect = Rect::new(
                area.x - u(40.0),
                area.y + row_h * index as f32,
                width,
                row_h - u(10.0),
            );
            let label = cx.label(tool.label());
            p.menu_item(rect, area.x, &label, index == cursor, 1.0);
            cx.hits.push((rect, Hit::Item(index)));
        }
        // The live state, down the right.
        let panel = Rect::new(
            area.x + width + u(80.0),
            area.y,
            area.right() - (area.x + width + u(80.0)),
            area.h,
        );
        if panel.w < u(300.0) {
            return;
        }
        let game = cx.game;
        let room = game.id;
        let room3 = room.room3();
        let room_name = cx.room_name(&room3);
        let choice = game.player_choice();
        let pending = game.debug.pending_player;
        let mut lines: Vec<(String, String)> = vec![
            (cx.gui("debug.room"), format!("{room3}  {room_name}")),
            (
                cx.gui("debug.player"),
                format!(
                    "{}  ·  {}",
                    CHARACTER_NAMES[usize::from(choice.character & 3)],
                    cx.label(COSTUME_NAMES[usize::from(choice.costume.min(2))])
                ),
            ),
            (
                cx.gui("debug.health"),
                format!("{} / {}", game.entities[0].health, game.max_health),
            ),
        ];
        if let Some(pending) = pending {
            lines.push((
                cx.gui("debug.next_load"),
                format!(
                    "{}  ·  {}",
                    CHARACTER_NAMES[usize::from(pending.character & 3)],
                    cx.label(COSTUME_NAMES[usize::from(pending.costume.min(2))])
                ),
            ));
        }
        let cheats: Vec<String> = [
            (game.debug.invincible, "UI_DEBUG_CHEAT_INVINCIBLE"),
            (game.debug.ghost, "UI_DEBUG_CHEAT_GHOST"),
            (
                game.debug.infinite_ammo != InfiniteAmmo::Off,
                "UI_DEBUG_CHEAT_AMMO",
            ),
        ]
        .into_iter()
        .filter(|(on, _)| *on)
        .map(|(_, key)| cx.label(key))
        .collect();
        if !cheats.is_empty() {
            lines.push((cx.gui("debug.cheats"), cheats.join(", ")));
        }
        let line_h = p.line_height(Role::Row) * 1.35;
        let label_w = lines
            .iter()
            .map(|(label, _)| p.measure(Role::Body, label))
            .fold(0.0, f32::max);
        let mut y = panel.y + line_h / 2.0;
        for (label, value) in &lines {
            p.text(
                Role::Body,
                panel.x,
                y,
                Align::Left,
                palette::TEXT_DIM,
                label,
            );
            let x = panel.x + label_w + u(28.0);
            let value = fit(p, Role::Row, value, panel.right() - x);
            p.text(Role::Row, x, y, Align::Left, palette::TEXT, &value);
            y += line_h;
        }
    }

    fn draw_player(cursor: usize, p: &mut Painter<'_>, cx: &mut Cx<'_>, area: Rect) {
        let theme = p.theme;
        let u = |v: f32| theme.u(v);
        let game = cx.game;
        let current = game.player_choice();
        let choice = game.debug.pending_player.unwrap_or(current);
        let character = usize::from(choice.character & 3);
        let costume = usize::from(choice.costume.min(2));
        // The chosen character's face.
        let face_side = u(220.0);
        if let Some(face) = cx.art.portrait(cx.pack, choice.character & 3) {
            let rect = Rect::new(area.x, area.y, face_side, face_side);
            p.canvas
                .fill_rect(rect.inset(-u(4.0), -u(4.0)), palette::RULE);
            p.canvas.draw_image(&face, rect, 1.0);
        }
        let x = area.x + face_side + u(60.0);
        let width = (area.right() - x).min(u(ROW_W));
        let row_h = u(ROW_H);
        let rows = [
            (
                cx.label("UI_DEBUG_PLAYER_CHARACTER"),
                CHARACTER_NAMES[character].to_string(),
                (4, character),
            ),
            (
                cx.label("UI_DEBUG_PLAYER_COSTUME"),
                cx.label(COSTUME_NAMES[costume]),
                (3, costume),
            ),
        ];
        let shown = if character >= 2 { 1 } else { rows.len() };
        for (index, (label, value, pips)) in rows.iter().take(shown).enumerate() {
            let rect = Rect::new(x, area.y + row_h * index as f32, width, row_h);
            widgets::row(
                p,
                cx,
                rect,
                index,
                index == cursor,
                Label::plain(label),
                Value::Step(value, Some(*pips)),
            );
        }
        let note = if game.debug.pending_player.is_some() {
            cx.label("UI_DEBUG_PLAYER_PENDING")
        } else {
            cx.label("UI_DEBUG_PLAYER_CURRENT")
        };
        let y = area.y + row_h * shown as f32 + u(30.0);
        p.paragraph(Role::Body, x, y, width, palette::TEXT_DIM, &note);
        if choice.character & 3 >= 2 {
            let note = cx.gui("debug.stand_in_note");
            p.paragraph(
                Role::Body,
                x,
                y + p.line_height(Role::Body) * 1.5,
                width,
                palette::TEXT_DIM,
                &note,
            );
        }
    }

    fn draw_cheats(cursor: usize, p: &mut Painter<'_>, cx: &mut Cx<'_>, area: Rect) {
        let theme = p.theme;
        let u = |v: f32| theme.u(v);
        let debug = cx.game.debug;
        let on_off = |on: bool| {
            (
                if on { "UI_DEBUG_ON" } else { "UI_DEBUG_OFF" },
                (2, usize::from(on)),
            )
        };
        let ammo = ammo_index(debug.infinite_ammo);
        let states = [
            on_off(debug.invincible),
            on_off(debug.ghost),
            (AMMO_LABELS[ammo], (3, ammo)),
        ];
        let width = area.w.min(u(ROW_W));
        let row_h = u(ROW_H);
        for (index, (key, (value, pips))) in CHEAT_NAMES.iter().zip(states).enumerate() {
            let rect = Rect::new(area.x, area.y + row_h * index as f32, width, row_h);
            let label = cx.label(key);
            let value = cx.label(value);
            widgets::row(
                p,
                cx,
                rect,
                index,
                index == cursor,
                Label::plain(&label),
                Value::Step(&value, Some(pips)),
            );
        }
    }

    fn draw_clear(cursor: usize, p: &mut Painter<'_>, cx: &mut Cx<'_>, area: Rect) {
        let theme = p.theme;
        let u = |v: f32| theme.u(v);
        let default = default_ending(cx.game);
        let width = area.w.min(u(ROW_W));
        let row_h = u(ROW_H).min(area.h / ENDING_HINTS.len() as f32);
        let expected = cx.gui("debug.expected");
        for (index, key) in ENDING_HINTS.iter().enumerate() {
            let rect = Rect::new(area.x, area.y + row_h * index as f32, width, row_h);
            let id = index as u8 + 1;
            let lead = format!("{id}");
            let label = cx.label(key);
            let note = (id == default).then_some(expected.as_str());
            widgets::row(
                p,
                cx,
                rect,
                index,
                index == cursor,
                Label::lead(&lead, &label),
                Value::Link(note),
            );
        }
    }
}

/// The INFINITE AMMO modes in step order, and their labels.
const AMMO_MODES: [InfiniteAmmo; 3] = [
    InfiniteAmmo::Off,
    InfiniteAmmo::Reload,
    InfiniteAmmo::Infinite,
];
const AMMO_LABELS: [&str; 3] = [
    "UI_DEBUG_OFF",
    "UI_DEBUG_AMMO_RELOAD",
    "UI_DEBUG_AMMO_INFINITE",
];

fn ammo_index(mode: InfiniteAmmo) -> usize {
    AMMO_MODES.iter().position(|&m| m == mode).unwrap_or(0)
}

/// See [`DebugMenu::snapshot`].
#[derive(PartialEq)]
struct Snapshot {
    cursors: [usize; 11],
    tool: Option<Tool>,
    debug: game::DebugTools,
    inventory: Vec<game::InventoryItem>,
    flag: bool,
}

/// What a pointer target does when clicked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hit {
    /// Pick list entry `index` and confirm it.
    Item(usize),
    /// Step entry `index`'s value down.
    Decrease(usize),
    /// Step entry `index`'s value up.
    Increase(usize),
    /// Switch to tab `index`.
    Tab(usize),
    /// Press a key, from its glyph.
    Key(GuiKey),
}

/// One frame's drawing context: the game data and the art cache.
struct Cx<'a> {
    pack: &'a Pack,
    text: &'a Text,
    game: &'a GameState,
    locale: &'a Locale,
    art: &'a mut Art,
    hits: &'a mut Vec<(Rect, Hit)>,
    /// The device whose glyphs the hints show.
    device: Device,
    /// Some art is still to load; paint again next frame.
    more: bool,
}

impl Cx<'_> {
    /// A pack UI string in title case.
    fn label(&self, key: &str) -> String {
        title_case(self.text.decode(self.text.ui(key)).trim())
    }

    /// A GUI string.
    fn gui(&self, key: &str) -> String {
        self.locale.get(key).to_string()
    }

    /// A room's display name in title case, or empty.
    fn room_name(&self, room3: &str) -> String {
        self.text
            .room_name(room3)
            .map(|bytes| title_case(self.text.decode(bytes).trim()))
            .unwrap_or_default()
    }

    /// An item's name in title case, or its hex id.
    fn item_name(&self, item: u8) -> String {
        match self.text.item_name(u16::from(item)) {
            Some(bytes) => title_case(self.text.decode(bytes).trim()),
            None => format!("Item {item:02X}"),
        }
    }

    /// `item`'s icon, HD when the pack has one.
    fn icon(&mut self, item: u8) -> Option<Arc<Picture>> {
        self.art.icon(self.pack, item)
    }
}

/// The GUI key's [`UiInput`] edge.
fn key_input(key: GuiKey) -> UiInput {
    let mut input = UiInput::default();
    match key {
        GuiKey::Up => input.up = true,
        GuiKey::Down => input.down = true,
        GuiKey::Left => input.left = true,
        GuiKey::Right => input.right = true,
        GuiKey::Confirm => input.confirm = true,
        GuiKey::Cancel => input.cancel = true,
        GuiKey::TabPrev | GuiKey::SubPrev => input.page_left = true,
        GuiKey::TabNext | GuiKey::SubNext => input.page_right = true,
        GuiKey::ResetAll | GuiKey::ResetGroup => input.start = true,
    }
    input
}

/// `text` with each word's first letter capitalised and the rest lowered;
/// text without ASCII capitals (a Japanese name) is left alone.
pub(crate) fn title_case(text: &str) -> String {
    if !text.chars().any(|c| c.is_ascii_uppercase()) || text.chars().any(|c| c.is_ascii_lowercase())
    {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut start = true;
    for c in text.chars() {
        if c.is_alphanumeric() || c == '\'' {
            if start {
                out.extend(c.to_uppercase());
            } else {
                out.extend(c.to_lowercase());
            }
            start = false;
        } else {
            out.push(c);
            start = true;
        }
    }
    out
}

/// `text` cut with an ellipsis to fit `max_width` pixels in `role`.
fn fit(p: &mut Painter<'_>, role: Role, text: &str, max_width: f32) -> String {
    if max_width <= 0.0 {
        return String::new();
    }
    if p.measure(role, text) <= max_width {
        return text.to_string();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let candidate: String = chars.iter().collect::<String>() + "…";
        if p.measure(role, &candidate) <= max_width {
            return candidate;
        }
    }
    String::new()
}
