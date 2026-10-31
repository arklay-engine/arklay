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
//! or cell picks it, clicking activates it and the wheel moves the cursor.
//! F1 closes it from anywhere and reopening returns to
//! the tool and cursors it was left on; the room browser also follows a room
//! change made while the menu was closed. Edits that only touch [`GameState`]
//! (inventory, flags, cheats, the pending player selection) happen inside
//! [`DebugMenu::handle_events`]; everything that needs the room-load path, the
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
use crate::gui::dom::{ImageValue, Library, Ui, UiOutput, Value};
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
/// The same, in an RE2 room. Each has one outfit here.
const CHARACTER_NAMES_RE2: [&str; 2] = ["Leon", "Claire"];
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

/// The legacy view the ROOM SELECT and INVENTORY grids still draw in: its
/// canvas, faces and pointer targets.
struct View {
    fonts: Option<Fonts>,
    canvas: Canvas,
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
    flags: FlagEditor,
    ending: u8,
    settings: Option<gui::Settings>,
    locale: Locale,
    device: Device,
    /// The main list and the PLAYER, FLAGS, CHEATS and CLEAR GAME tools
    /// (`assets/ui/debug*.xml`).
    ui: Option<Ui>,
    /// The pack's strings, kept from the first draw for the labels the
    /// screens show.
    text: Option<Text>,
    /// Every flag's state when `debug.flags` was last published; the
    /// groups are rebuilt only when one changes (or the strings do).
    flag_values: Option<Vec<bool>>,
    /// Where the PLAYER and CHEATS screens were left, by screen name.
    cursors: std::collections::HashMap<String, usize>,
    view: View,
}

impl DebugMenu {
    /// Open the main list with every tool seeded from the live state.
    pub fn open(pack: &Pack, current: RoomId, game: &GameState) -> Self {
        let ui = Ui::new(Library::builtin())
            .map_err(|err| eprintln!("warning: debug menu: {err:#}"))
            .ok();
        let mut menu = Self {
            tool: None,
            main_cursor: 0,
            rooms: RoomSelect::new(pack, current),
            inventory: InventoryEditor::default(),
            flags: FlagEditor::default(),
            ending: default_ending(game).clamp(1, 7),
            settings: None,
            locale: Locale::default(),
            device: Device::Keyboard,
            ui,
            text: None,
            flag_values: None,
            cursors: Default::default(),
            view: View {
                fonts: Fonts::new()
                    .map_err(|err| eprintln!("warning: debug menu fonts: {err:#}"))
                    .ok(),
                canvas: Canvas::new(1, 1),
                hits: Vec::new(),
                art: Art::default(),
                dirty: true,
            },
        };
        menu.publish(game);
        if let Some(ui) = &mut menu.ui
            && let Err(err) = ui.open("debug")
        {
            eprintln!("warning: debug menu: {err:#}");
        }
        menu
    }

    /// Reattach the menu to the live session after it was closed: the open
    /// tool and every cursor are kept, while the room browser follows
    /// `current` if the room changed and the inventory cursor is clamped to
    /// the live capacity.
    pub fn reopen(&mut self, current: RoomId, game: &GameState) {
        // Settings may have changed while closed; take them again.
        self.settings = None;
        self.rooms.refresh(current);
        self.inventory.refresh(game);
        self.view.dirty = true;
        self.publish(game);
        if let Some(ui) = &mut self.ui {
            if ui.is_empty() {
                match ui.open("debug") {
                    Ok(()) => ui.set_focus_index(self.main_cursor),
                    Err(err) => eprintln!("warning: debug menu: {err:#}"),
                }
            }
            ui.invalidate();
        }
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
        self.ending
    }

    /// Whether [`Self::configure`] has yet to run.
    pub fn needs_settings(&self) -> bool {
        self.settings.is_none()
    }

    /// Take the menus' settings (face, text size, language, button style).
    pub fn configure(&mut self, settings: gui::Settings) {
        let tag = crate::language::resolve_tag(settings.language.as_deref());
        if self.locale.tag() != tag {
            self.locale = Locale::new(&tag);
            // The flag states are named in the language.
            self.flag_values = None;
        }
        if let Some(ui) = &mut self.ui {
            ui.set_locale(self.locale.clone());
            ui.set_theme(settings.game.ui_font, settings.game.menu_text_size.scale());
        }
        self.settings = Some(settings);
        self.view.dirty = true;
        let device = self.shown_device();
        if let Some(ui) = &mut self.ui {
            ui.set_device(device);
        }
    }

    /// The canvas the last [`Self::draw`] painted.
    pub fn canvas(&self) -> &Canvas {
        match (&self.ui, self.legacy()) {
            (Some(ui), false) => ui.canvas(),
            _ => &self.view.canvas,
        }
    }

    /// Whether the open tool still draws in the legacy view.
    fn legacy(&self) -> bool {
        matches!(self.tool, Some(Tool::Rooms | Tool::Inventory))
    }

    /// The device the hints show: the configured button style on a pad.
    fn shown_device(&self) -> Device {
        match (
            self.device,
            self.settings.as_ref().and_then(|s| s.game.button_style),
        ) {
            (Device::Gamepad(_), Some(style)) => Device::Gamepad(style),
            (device, _) => device,
        }
    }

    /// A pack UI string in title case (the key itself before the first
    /// draw handed the strings over).
    fn label(&self, key: &str) -> String {
        match &self.text {
            Some(text) => title_case(text.decode(text.ui(key)).trim()),
            None => key.to_string(),
        }
    }

    /// Publish the live state the screens show.
    fn publish(&mut self, game: &GameState) {
        let Some(mut ui) = self.ui.take() else {
            return;
        };
        let text = |key: &str| Value::from(self.label(key));
        ui.set(
            "debug.text",
            Value::map(
                [
                    ("title", "UI_DEBUG_TITLE"),
                    ("rooms", Tool::Rooms.label()),
                    ("inventory", Tool::Inventory.label()),
                    ("player", Tool::Player.label()),
                    ("flags", Tool::Flags.label()),
                    ("cheats", Tool::Cheats.label()),
                    ("save", Tool::Save.label()),
                    ("item_box", Tool::ItemBox.label()),
                    ("clear_game", Tool::ClearGame.label()),
                ]
                .map(|(name, key)| (name, text(key))),
            ),
        );
        ui.set("debug.state", self.state(game));

        // PLAYER.
        let current = game.player_choice();
        let choice = game.debug.pending_player.unwrap_or(current);
        let character = choice.character & 3;
        ui.set(
            "debug.character",
            Value::map([
                ("label", text("UI_DEBUG_PLAYER_CHARACTER")),
                (
                    "choices",
                    Value::list(
                        character_names(game)
                            .iter()
                            .map(|&n| Value::from(n))
                            .collect(),
                    ),
                ),
                ("value", Value::from(i64::from(character))),
            ]),
        );
        // Barry and Rebecca wear their one outfit: no OUTFIT row.
        ui.set(
            "debug.costume",
            if character < 2 && !game.is_re2_room {
                Value::map([
                    ("label", text("UI_DEBUG_PLAYER_COSTUME")),
                    (
                        "choices",
                        Value::list(COSTUME_NAMES.iter().map(|&k| text(k)).collect()),
                    ),
                    ("value", Value::from(i64::from(choice.costume.min(2)))),
                ])
            } else {
                Value::Nil
            },
        );
        ui.set(
            "debug.player_note",
            text(if game.debug.pending_player.is_some() {
                "UI_DEBUG_PLAYER_PENDING"
            } else {
                "UI_DEBUG_PLAYER_CURRENT"
            }),
        );
        ui.set(
            "debug.stand_in_note",
            if character >= 2 {
                Value::from(self.locale.get("debug.stand_in_note"))
            } else {
                Value::Nil
            },
        );

        // CHEATS.
        let on_off = Value::list(vec![text("UI_DEBUG_OFF"), text("UI_DEBUG_ON")]);
        let cheat = |key: &str, choices: &Value, value: usize| {
            Value::map([
                ("label", text(key)),
                ("choices", choices.clone()),
                ("value", Value::from(value)),
            ])
        };
        let debug = game.debug;
        ui.set(
            "debug.invincible",
            cheat(CHEAT_NAMES[0], &on_off, usize::from(debug.invincible)),
        );
        ui.set(
            "debug.ghost",
            cheat(CHEAT_NAMES[1], &on_off, usize::from(debug.ghost)),
        );
        let ammo = Value::list(AMMO_LABELS.iter().map(|&k| text(k)).collect());
        ui.set(
            "debug.ammo",
            cheat(CHEAT_NAMES[2], &ammo, ammo_index(debug.infinite_ammo)),
        );

        // CLEAR GAME.
        let default = default_ending(game);
        let expected = Value::from(self.locale.get("debug.expected"));
        ui.set(
            "debug.endings",
            Value::list(
                ENDING_HINTS
                    .iter()
                    .enumerate()
                    .map(|(index, key)| {
                        let id = index as u8 + 1;
                        Value::map([
                            ("id", Value::from(i64::from(id))),
                            ("label", text(key)),
                            (
                                "note",
                                if id == default {
                                    expected.clone()
                                } else {
                                    Value::Nil
                                },
                            ),
                            ("expected", Value::Bool(id == default)),
                        ])
                    })
                    .collect(),
            ),
        );

        // FLAGS, while they can be reached and when a flag changed.
        if matches!(ui.top(), Some("debug" | "debug_flags")) {
            let values: Vec<bool> = GROUPS
                .iter()
                .flat_map(|group| {
                    (0..usize::from(group.count))
                        .map(|row| game.flag_test(group.bank, group.selector(row), false))
                })
                .collect();
            if self.flag_values.as_ref() != Some(&values) {
                ui.set("debug.flags", self.flag_groups(game));
                self.flag_values = Some(values);
            }
        }
        self.ui = Some(ui);
    }

    /// The main list's state table: the room, the player, health, the next
    /// load and the cheats on.
    fn state(&self, game: &GameState) -> Value {
        let gui = |key: &str| self.locale.get(key).to_string();
        let room3 = game.id.room3();
        let room_name = self
            .text
            .as_ref()
            .and_then(|text| {
                text.room_name(&room3)
                    .map(|b| title_case(text.decode(b).trim()))
            })
            .unwrap_or_default();
        let choice = game.player_choice();
        let names = character_names(game);
        let who = |c: game::PlayerChoice| {
            let name = names[usize::from(c.character) % names.len()];
            if game.is_re2_room {
                return name.to_string();
            }
            format!(
                "{name}  ·  {}",
                self.label(COSTUME_NAMES[usize::from(c.costume.min(2))])
            )
        };
        let mut lines = vec![
            (gui("debug.room"), format!("{room3}  {room_name}")),
            (gui("debug.player"), who(choice)),
            (
                gui("debug.health"),
                format!("{} / {}", game.entities[0].health, game.max_health),
            ),
        ];
        if let Some(pending) = game.debug.pending_player {
            lines.push((gui("debug.next_load"), who(pending)));
        }
        let cheats: Vec<String> = [
            (game.debug.invincible, CHEAT_NAMES[0]),
            (game.debug.ghost, CHEAT_NAMES[1]),
            (
                game.debug.infinite_ammo != InfiniteAmmo::Off,
                CHEAT_NAMES[2],
            ),
        ]
        .into_iter()
        .filter(|(on, _)| *on)
        .map(|(_, key)| self.label(key))
        .collect();
        if !cheats.is_empty() {
            lines.push((gui("debug.cheats"), cheats.join(", ")));
        }
        Value::list(
            lines
                .into_iter()
                .map(|(label, value)| {
                    Value::map([("label", Value::from(label)), ("value", Value::from(value))])
                })
                .collect(),
        )
    }

    /// Every flag group with its rows: selector, name and state.
    fn flag_groups(&self, game: &GameState) -> Value {
        Value::list(
            GROUPS
                .iter()
                .map(|group| {
                    let states = match flags::state_names(group) {
                        Some((clear, set)) => Value::list(vec![
                            Value::from(self.locale.get(clear)),
                            Value::from(self.locale.get(set)),
                        ]),
                        None => Value::list(vec![
                            Value::from(self.label("UI_DEBUG_OFF")),
                            Value::from(self.label("UI_DEBUG_ON")),
                        ]),
                    };
                    let rows = (0..usize::from(group.count))
                        .map(|row| {
                            let sel = group.selector(row);
                            Value::map([
                                ("index", Value::from(row)),
                                ("lead", Value::from(format!("{sel:02X}"))),
                                ("label", Value::from(self.flag_label(group, sel))),
                                ("choices", states.clone()),
                                (
                                    "value",
                                    Value::from(usize::from(
                                        game.flag_test(group.bank, sel, false),
                                    )),
                                ),
                            ])
                        })
                        .collect();
                    Value::map([
                        ("label", Value::from(self.label(group.label))),
                        ("rows", Value::list(rows)),
                    ])
                })
                .collect(),
        )
    }

    /// A flag's name: its identifier, room or document.
    fn flag_label(&self, group: &FlagGroup, sel: u8) -> String {
        match flag_name(group, sel) {
            FlagName::None => String::new(),
            FlagName::Static(name) => name.to_string(),
            FlagName::Room(label, room3) => {
                let name = self
                    .text
                    .as_ref()
                    .and_then(|t| t.room_name(&room3).map(|b| title_case(t.decode(b).trim())))
                    .unwrap_or_default();
                if name.is_empty() {
                    format!("{label} {room3}")
                } else {
                    format!("{room3}  {name}")
                }
            }
            FlagName::Item(item) => match self
                .text
                .as_ref()
                .and_then(|t| t.item_name(u16::from(item)).map(|b| (t, b)))
            {
                Some((t, bytes)) => title_case(t.decode(bytes).trim()),
                None => format!("Item {item:02X}"),
            },
        }
    }

    /// Everything a legacy key can change, to tell a key that did something
    /// from one that did not.
    fn snapshot(&self, game: &GameState) -> Snapshot {
        Snapshot {
            cursors: [
                self.rooms.column(),
                self.rooms.row(),
                self.inventory.slot(),
                usize::from(self.inventory.on_size()),
                self.inventory
                    .picker()
                    .map_or(usize::MAX, |p| usize::from(p.item())),
            ],
            tool: self.tool,
            inventory: game.inventory.clone(),
        }
    }

    /// How deep the legacy tools are: 1 a tool, 2 the item picker.
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
        if !events.is_empty() {
            self.publish(game);
        }
        for &event in events {
            if let GuiEvent::Key(_, device) = event {
                self.device = device;
            }
            let result = if self.legacy() || self.ui.is_none() {
                self.legacy_event(event, game, &mut sounds)
            } else {
                self.ui_event(event, game, &mut sounds)
            };
            if let Some(result) = result
                && result != DebugMenuEvent::None
            {
                out.push(result);
                break;
            }
        }
        let volume = self
            .settings
            .as_ref()
            .map_or(1, |settings| settings.game.menu_volume);
        if volume == 0 {
            sounds.clear();
        }
        (out, sounds)
    }

    /// One event for the screens of the [`Ui`].
    fn ui_event(
        &mut self,
        event: GuiEvent,
        game: &mut GameState,
        sounds: &mut Vec<Sound>,
    ) -> Option<DebugMenuEvent> {
        let event = match event {
            GuiEvent::Key(key, _) => GuiEvent::Key(key, self.shown_device()),
            other => other,
        };
        let ui = self.ui.as_mut()?;
        let outputs = ui.handle(&[event]);
        let mut result = None;
        let mut opened = Vec::new();
        for output in outputs {
            match output {
                UiOutput::Sound(sound) => sounds.push(sound),
                UiOutput::Closed(_) if ui.is_empty() => result = Some(DebugMenuEvent::Close),
                UiOutput::Action { name, arg, .. } => match name.as_str() {
                    "rooms" => self.tool = Some(Tool::Rooms),
                    "inventory" => self.tool = Some(Tool::Inventory),
                    "save" => result = Some(DebugMenuEvent::SaveGame),
                    "item_box" => result = Some(DebugMenuEvent::ItemBox),
                    "ending" => {
                        let id = arg.as_int().unwrap_or(1).clamp(1, 7) as u8;
                        self.ending = id;
                        result = Some(DebugMenuEvent::ClearGame(id));
                    }
                    other => eprintln!("warning: debug menu: unknown action {other:?}"),
                },
                UiOutput::Changed { path, value } => {
                    apply_change(&path, value.as_int().unwrap_or(0), game);
                }
                UiOutput::Opened(name) => opened.push(name),
                UiOutput::Closed(_) => {}
            }
        }
        for name in opened {
            self.restore(&name);
        }
        self.sync();
        self.publish(game);
        result
    }

    /// Put a tool's screen, just opened, back on the cursor it was left on.
    fn restore(&mut self, name: &str) {
        let Some(ui) = &mut self.ui else {
            return;
        };
        match name {
            "debug" => ui.set_focus_index(self.main_cursor),
            "debug_flags" => {
                ui.set_tab_index(self.flags.group());
                ui.set_focus_index(self.flags.row());
            }
            "debug_clear" => ui.set_focus_index(usize::from(self.ending.max(1) - 1)),
            _ => {
                if let Some(&index) = self.cursors.get(name) {
                    ui.set_focus_index(index);
                }
            }
        }
    }

    /// Follow the screens: the open tool and the cursors the accessors
    /// report.
    fn sync(&mut self) {
        if self.legacy() {
            return;
        }
        let Some(ui) = &mut self.ui else {
            return;
        };
        let top = ui.top().map(str::to_string);
        self.tool = match top.as_deref() {
            Some("debug_player") => Some(Tool::Player),
            Some("debug_flags") => Some(Tool::Flags),
            Some("debug_cheats") => Some(Tool::Cheats),
            Some("debug_clear") => Some(Tool::ClearGame),
            _ => None,
        };
        let index = |name: &str| {
            ui.focus_prop(name)
                .as_int()
                .and_then(|i| usize::try_from(i).ok())
        };
        match top.as_deref() {
            Some("debug") => {
                if let Some(i) = index("index") {
                    self.main_cursor = i.min(TOOLS.len() - 1);
                }
            }
            Some("debug_flags") => {
                let group = ui.tab_indices().first().copied().unwrap_or(0);
                if group == self.flags.group() {
                    self.flags.sync(group, index("index").unwrap_or(0));
                } else {
                    // Another group: back to the row it was left on.
                    self.flags.sync(group, self.flags.row_in(group));
                    ui.set_focus_index(self.flags.row());
                }
            }
            Some("debug_clear") => {
                if let Some(id) = index("lead") {
                    self.ending = id.clamp(1, 7) as u8;
                }
            }
            Some(name @ ("debug_player" | "debug_cheats")) => {
                if let Some(i) = ui.focus_index() {
                    self.cursors.insert(name.to_string(), i);
                }
            }
            _ => {}
        }
    }

    /// One event for the legacy ROOM SELECT and INVENTORY grids.
    fn legacy_event(
        &mut self,
        event: GuiEvent,
        game: &mut GameState,
        sounds: &mut Vec<Sound>,
    ) -> Option<DebugMenuEvent> {
        if !matches!(event, GuiEvent::PointerMove { .. }) {
            self.view.dirty = true;
        }
        let depth = self.depth();
        let before = self.snapshot(game);
        let result = match event {
            GuiEvent::Key(key, _) => Some(self.handle_input(key_input(key), game)),
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
                    if self.tool == Some(Tool::Rooms) {
                        self.rooms.set_stage(index);
                    }
                    Some(DebugMenuEvent::None)
                }
                Some(Hit::Key(key)) => Some(self.handle_input(key_input(key), game)),
                None => None,
            },
            // The wheel moves the cursor, as on every other screen.
            GuiEvent::Wheel(notches) if notches != 0.0 => {
                let key = if notches > 0.0 {
                    GuiKey::Up
                } else {
                    GuiKey::Down
                };
                Some(self.handle_input(key_input(key), game))
            }
            GuiEvent::Wheel(_) => None,
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
        let result = result?;
        let after = self.depth();
        let sound = match result {
            DebugMenuEvent::Close => Some(Sound::Cancel),
            DebugMenuEvent::None if after < depth => Some(Sound::Cancel),
            // A key that changed nothing (an end stop) stays quiet.
            DebugMenuEvent::None if self.snapshot(game) == before => None,
            DebugMenuEvent::None => Some(Sound::Cursor),
            _ => Some(Sound::Decide),
        };
        sounds.extend(sound);
        if !self.legacy()
            && let Some(ui) = &mut self.ui
        {
            // Back on the main list.
            ui.invalidate();
        }
        Some(result)
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

    /// Put the open grid's cursor on `index`.
    fn point(&mut self, index: usize, game: &GameState) {
        match self.tool {
            Some(Tool::Rooms) => self.rooms.point(index),
            Some(Tool::Inventory) => self.inventory.point(index, game),
            _ => {}
        }
    }

    /// One tick of edge-triggered input for the legacy grids.
    fn handle_input(&mut self, input: UiInput, game: &mut GameState) -> DebugMenuEvent {
        if input.debug_menu {
            return DebugMenuEvent::Close;
        }
        let step = match self.tool {
            Some(Tool::Rooms) => self.rooms.handle_input(input, jump_character(game)),
            Some(Tool::Inventory) => self.inventory.handle_input(input, game),
            _ if input.cancel => return DebugMenuEvent::Close,
            _ => Step::Stay,
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
        if self.text.is_none() {
            self.text = Some(text.clone());
            // The flags take their names from the strings too.
            self.flag_values = None;
        }
        if !self.legacy() && self.ui.is_some() {
            self.publish(game);
            let portrait = {
                let current = game.player_choice();
                let choice = game.debug.pending_player.unwrap_or(current);
                self.view.art.portrait(pack, choice.character & 3)
            };
            let ui = self.ui.as_mut().unwrap();
            ui.set(
                "debug.portrait",
                portrait.map_or(Value::Nil, |colour| {
                    Value::Image(ImageValue { colour, grey: None })
                }),
            );
            ui.set_recording(gpu);
            return ui.draw(width, height);
        }
        self.draw_legacy(width, height, gpu, pack, text, game)
    }

    fn draw_legacy(
        &mut self,
        width: u32,
        height: u32,
        gpu: bool,
        pack: &Pack,
        text: &Text,
        game: &GameState,
    ) -> bool {
        let (width, height) = (width.max(1), height.max(1));
        let device = self.shown_device();
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
        let (family, text_scale) = self
            .settings
            .as_ref()
            .map_or((FontFamily::Modern, 1.0), |s| {
                (s.game.ui_font, s.game.menu_text_size.scale())
            });
        let theme = Theme::new(width, height, family).with_text_scale(text_scale);
        let mut p = Painter {
            canvas: &mut view.canvas,
            fonts,
            theme,
        };
        let mut cx = Cx {
            pack,
            text,
            game,
            locale: &self.locale,
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
        let name = match self.tool {
            Some(Tool::Inventory) if self.inventory.picker().is_some() => {
                cx.label("UI_DEBUG_PICKER_TITLE")
            }
            Some(tool) => cx.label(tool.label()),
            None => String::new(),
        };
        p.text(Role::Title, x, title_y, Align::Left, palette::TEXT, &name);
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
            _ => Vec::new(),
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
}

/// Apply a change a debug screen made to the live state.
/// The PLAYER tool's choices for the game being played.
fn character_names(game: &GameState) -> &'static [&'static str] {
    if game.is_re2_room {
        &CHARACTER_NAMES_RE2
    } else {
        &CHARACTER_NAMES
    }
}

fn apply_change(path: &str, value: i64, game: &mut GameState) {
    let Some(path) = path.strip_prefix("debug.") else {
        return;
    };
    let debug = &mut game.debug;
    match path {
        "character.value" | "costume.value" => {
            let current = game.player_choice();
            let mut choice = game.debug.pending_player.unwrap_or(current);
            if path == "character.value" {
                choice.character = value.clamp(0, character_names(game).len() as i64 - 1) as u8;
            } else {
                choice.costume = value.clamp(0, 2) as u8;
            }
            game.debug.pending_player = (choice != current).then_some(choice);
        }
        "invincible.value" => debug.invincible = value != 0,
        "ghost.value" => debug.ghost = value != 0,
        "ammo.value" => {
            debug.infinite_ammo = AMMO_MODES[value.clamp(0, 2) as usize];
            if debug.infinite_ammo == InfiniteAmmo::Infinite {
                crate::weapons::refill_clip(game);
            }
        }
        _ => {
            // flags.<group>.rows.<row>.value
            let parts: Vec<&str> = path.split('.').collect();
            if let ["flags", group, "rows", row, "value"] = parts.as_slice()
                && let (Ok(group), Ok(row)) = (group.parse::<usize>(), row.parse::<usize>())
                && let Some(group) = GROUPS.get(group)
            {
                // Mode 0 sets the bit, 1 clears it.
                let sel = group.selector(row);
                game.apply_flag(group.bank, sel, if value == 1 { 0 } else { 1 });
            }
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
    cursors: [usize; 5],
    tool: Option<Tool>,
    inventory: Vec<game::InventoryItem>,
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
        GuiKey::Aux => {}
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_re2_room_names_its_own_characters() {
        let mut game = GameState::new(RoomId::default(), &crate::state::RoomState::default());
        assert_eq!(character_names(&game)[0], "Chris");
        game.is_re2_room = true;
        assert_eq!(character_names(&game), ["Leon", "Claire"]);
        apply_change("debug.character.value", 3, &mut game);
        assert_eq!(
            game.debug.pending_player.map(|choice| choice.character),
            Some(1)
        );
    }

    fn key(key: GuiKey) -> GuiEvent {
        GuiEvent::Key(key, Device::Keyboard)
    }

    /// `ARKLAY_GUI_PACK=<re1.akpak> ARKLAY_GUI_DUMP=<dir> cargo test --lib
    /// ui::debug_menu::tests::dump` writes the main list and its tools as
    /// BMPs for eyeballing.
    #[test]
    fn dump() {
        let (Some(dir), Some(pack)) = (
            std::env::var_os("ARKLAY_GUI_DUMP"),
            std::env::var_os("ARKLAY_GUI_PACK"),
        ) else {
            return;
        };
        let dir = std::path::PathBuf::from(dir);
        let pack = Pack::open(std::path::Path::new(&pack)).unwrap();
        let text = Text::load(&pack, pack.language());
        let room = RoomId::parse("1060").unwrap();
        let mut game = GameState::default();
        game.id = room;
        // A few held items for the inventory editor.
        game.inventory = [(0x02, 15), (0x0A, 0), (0x2F, 3), (0x40, 1)]
            .into_iter()
            .map(|(id, quantity)| game::InventoryItem { id, quantity })
            .collect();
        let mut menu = DebugMenu::open(&pack, room, &game);
        menu.configure(gui::Settings::default());
        let shot = |menu: &mut DebugMenu, game: &mut GameState, name: &str, keys: &[GuiKey]| {
            let events: Vec<GuiEvent> = keys.iter().map(|&k| key(k)).collect();
            menu.handle_events(&events, game);
            let (w, h) = (1920, 1080);
            // Twice: the first draw hands the pack's strings over.
            menu.draw(w, h, false, &pack, &text, game);
            menu.handle_events(&[], game);
            if let Some(ui) = &mut menu.ui {
                ui.invalidate();
            }
            menu.draw(w, h, false, &pack, &text, game);
            let canvas = menu.canvas();
            let mut rgba = Vec::with_capacity(canvas.rgba.len());
            for (i, px) in canvas.rgba.chunks(4).enumerate() {
                let (x, y) = (i as u32 % w, i as u32 / w);
                let bg = if (x / 64 + y / 64) % 2 == 0 {
                    90.0
                } else {
                    60.0
                };
                let keep = 1.0 - f32::from(px[3]) / 255.0;
                for &channel in &px[..3] {
                    rgba.push((f32::from(channel) + bg * keep) as u8);
                }
                rgba.push(255);
            }
            let image = crate::state::Image {
                width: w,
                height: h,
                rgba,
            };
            crate::bmp::encode(&image, &dir.join(format!("{name}.bmp"))).unwrap();
        };
        use GuiKey::*;
        shot(&mut menu, &mut game, "debug", &[]);
        shot(&mut menu, &mut game, "debug_player", &[Down, Down, Confirm]);
        shot(&mut menu, &mut game, "debug_player_barry", &[Right, Right]);
        shot(
            &mut menu,
            &mut game,
            "debug_flags",
            &[Cancel, Down, Confirm],
        );
        shot(
            &mut menu,
            &mut game,
            "debug_flags_visited",
            &[TabPrev, TabPrev, TabPrev, TabPrev, Down],
        );
        shot(
            &mut menu,
            &mut game,
            "debug_cheats",
            &[Cancel, Down, Confirm, Right],
        );
        shot(
            &mut menu,
            &mut game,
            "debug_clear",
            &[Cancel, Down, Down, Down, Confirm],
        );
        shot(&mut menu, &mut game, "debug_state", &[Cancel]);
        shot(&mut menu, &mut game, "debug_rooms", &[Down, Confirm]);
        shot(
            &mut menu,
            &mut game,
            "debug_inventory",
            &[Cancel, Down, Confirm],
        );
        shot(&mut menu, &mut game, "debug_picker", &[Confirm]);
    }
}
