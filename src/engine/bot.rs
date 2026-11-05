//! A restricted, read-only Lua bot host.
//!
//! Unlike the pack's authoring hooks ([`crate::lua`]), this VM is driven by a
//! `bot/**.lua` mod and may only observe the live session and feed normal
//! player/UI input back to it. It cannot set flags, items or positions. The
//! session calls [`BotRuntime::tick`] once per app update, before the game
//! tick, and replaces the human input with the [`BotInput`] the script left
//! behind.
//!
//! The API is deliberately small and generic: the Lua mod owns route policy
//! (which door to take, when to skip a scene), while Rust owns the reusable
//! skills (collision-aware walking, turning, button pulses).
//!
//! Per-room puzzle data lives next to the room in `room/<id>/bot.xml`; the
//! parser is in [`meta`] and the Lua API exposes it read-only as
//! `bot:room_meta()`. The observed world model is in [`world`] and reaches
//! Lua as `bot:world()`.

mod meta;
mod stats;
mod steer;
mod world;

use std::cell::{Cell, RefCell};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};
use std::rc::Rc;

use anyhow::{Context, Result, anyhow, bail};
use mlua::{Function, Lua, Table, UserData, UserDataMethods, Value};

use super::GameSession;
use crate::budget;
use crate::game::{self, FlagBank, InventoryItem, RoomActionKind};
use crate::pack::Pack;
use crate::player;
use crate::state::{RoomId, RoomState};
use crate::ui::UiInput;
use crate::weapons::WeaponState;

/// The default headless tick budget (about 55 minutes at 30 Hz).
pub const DEFAULT_MAX_TICKS: u64 = 100_000;

/// The default `--bot-risk`: how much danger the planner may accept, 0..=1.
pub const DEFAULT_RISK: f64 = 0.25;

/// The bot run options the CLI adds on top of a normal boot.
#[derive(Clone, Debug, PartialEq)]
pub struct BotOptions {
    /// Drive the game from the pack's `bot/**.lua` entries.
    pub enabled: bool,
    /// Run without a window or audio, as fast as the simulation allows.
    pub headless: bool,
    /// Headless app-update budget; ignored by the interactive path.
    pub max_ticks: u64,
    /// Write the headless run's statistics as JSON to this path.
    pub stats_path: Option<std::path::PathBuf>,
    /// The planner's risk setting (`--bot-risk`), `0.0..=1.0`; read-only to
    /// Lua as `bot:risk()`.
    pub risk: f64,
}

impl Default for BotOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            headless: false,
            max_ticks: 0,
            stats_path: None,
            risk: DEFAULT_RISK,
        }
    }
}

/// One tick of input the Lua script asked for. Reset before every call.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BotInput {
    /// UI/menu edges.
    pub ui: UiInput,
    /// Player input.
    pub input: player::Input,
    /// The action/confirm held level the message window reads.
    pub action: bool,
}

impl BotInput {
    /// Drop everything from the previous tick.
    fn reset(&mut self) {
        *self = Self::default();
    }

    /// Apply one `set_input` table key. Returns `false` for an unknown key.
    ///
    /// The mapping is intentionally literal:
    /// - `up`/`down`/`left`/`right` hold the player direction and set the UI
    ///   edge and held flag;
    /// - `held_*` and `*_pressed` set only the UI level/edge;
    /// - `run`, `aim`, `fire` and `action` hold their player button;
    /// - `*_pressed` sets the player button's edge;
    /// - `confirm`, `cancel` and `start` are UI edges.
    fn set(&mut self, name: &str, value: bool) -> bool {
        match name {
            "up" => {
                self.input.up = value;
                self.ui.up = value;
                self.ui.held_up = value;
            }
            "down" => {
                self.input.down = value;
                self.ui.down = value;
                self.ui.held_down = value;
            }
            "left" => {
                self.input.left = value;
                self.ui.left = value;
                self.ui.held_left = value;
            }
            "right" => {
                self.input.right = value;
                self.ui.right = value;
                self.ui.held_right = value;
            }
            "held_up" => self.ui.held_up = value,
            "held_down" => self.ui.held_down = value,
            "held_left" => self.ui.held_left = value,
            "held_right" => self.ui.held_right = value,
            "up_pressed" => self.ui.up = value,
            "down_pressed" => self.ui.down = value,
            "left_pressed" => self.ui.left = value,
            "right_pressed" => self.ui.right = value,
            "run" => self.input.run = value,
            "run_pressed" => self.input.run_pressed = value,
            "aim" => self.input.aim = value,
            "aim_pressed" => self.input.aim_pressed = value,
            "fire" => self.input.fire = value,
            "fire_pressed" => self.input.fire_pressed = value,
            "action" | "action_held" => self.input.action_held = value,
            "action_pressed" => self.input.action_pressed = value,
            "confirm" => self.ui.confirm = value,
            "cancel" => self.ui.cancel = value,
            "start" => self.ui.start = value,
            _ => return false,
        }
        self.action = self.input.action_held || self.input.action_pressed;
        true
    }

    /// Set one action key held and pressed, for one tick.
    fn press_action(&mut self) {
        self.set("action", true);
        self.set("action_pressed", true);
    }

    /// Pulse the UI confirm edge, for one tick.
    fn pulse_confirm(&mut self) {
        self.set("confirm", true);
    }
}

/// The remembered state of one room action, kept per room in [`BotMemory`].
#[derive(Clone, Debug, PartialEq, Eq)]
struct ActionMemory {
    slot: u8,
    kind: &'static str,
    x: i32,
    z: i32,
    width: i32,
    depth: i32,
    item: Option<u8>,
    quantity: Option<u8>,
    next_room: Option<u8>,
    key: Option<u8>,
    lock: Option<u8>,
    handler: u8,
    flags: u8,
}

/// The remembered state of one door.
#[derive(Clone, Debug, PartialEq, Eq)]
struct DoorMemory {
    slot: u8,
    target: Option<String>,
    next_room: u8,
    key: u8,
    lock: u8,
    x: i32,
    z: i32,
    width: i32,
    depth: i32,
}

/// What the bot has seen and remembered this run. In-memory only.
#[derive(Debug, Default)]
struct BotMemory {
    /// The rooms entered, in order (repeats included when the room changes).
    room_path: Vec<String>,
    /// The current room key, so a re-entry is recorded as a new path step.
    current_room: Option<String>,
    visited: BTreeSet<String>,
    rooms: BTreeMap<String, (Vec<ActionMemory>, Vec<DoorMemory>)>,
    values: BTreeMap<String, String>,
    /// The observed world model (`bot:world()`).
    world: world::World,
}

impl BotMemory {
    /// Record the current room, its action table and its doors.
    fn observe(&mut self, snapshot: &SessionSnapshot) {
        let key = snapshot.room.room3();
        if self.current_room.as_deref() != Some(key.as_str()) {
            self.current_room = Some(key.clone());
            self.room_path.push(key.clone());
        }
        self.visited.insert(key.clone());
        let actions = snapshot
            .actions
            .iter()
            .map(ActionMemory::from)
            .collect::<Vec<_>>();
        let doors = snapshot
            .doors
            .iter()
            .map(DoorMemory::from)
            .collect::<Vec<_>>();
        self.rooms.insert(key, (actions, doors));
    }

    /// Whether the three-digit room id `key` was visited.
    fn is_visited_key(&self, key: &str) -> bool {
        self.visited.contains(&key.to_ascii_uppercase())
    }

    /// Whether the packed `(stage << 8) | room` code was visited.
    fn is_visited_code(&self, code: u16) -> bool {
        let id = RoomId {
            stage: (code >> 8) as u8,
            room: (code & 0xFF) as u8,
            player_flag: 0,
            mansion2: false,
        };
        self.visited.contains(&id.room3())
    }

    /// Mark a room visited from the Lua `visited` query.
    fn is_visited(&self, value: &Value) -> bool {
        match value {
            Value::String(text) => self.is_visited_key(&text.to_string_lossy()),
            Value::Integer(code) => self.is_visited_code((*code as u16) & 0x0FFF),
            _ => false,
        }
    }

    fn remember(&mut self, key: String, value: Option<String>) {
        match value {
            Some(value) => {
                self.values.insert(key, value);
            }
            None => {
                self.values.remove(&key);
            }
        }
    }

    fn log(&mut self, line: String) {
        eprintln!("[bot] {line}");
    }
}

impl From<&ActionSnapshot> for ActionMemory {
    fn from(action: &ActionSnapshot) -> Self {
        Self {
            slot: action.slot,
            kind: action.kind,
            x: action.x,
            z: action.z,
            width: action.width,
            depth: action.depth,
            item: action.item,
            quantity: action.quantity,
            next_room: action.next_room,
            key: action.key,
            lock: action.lock,
            handler: action.handler,
            flags: action.flags,
        }
    }
}

impl From<&DoorSnapshot> for DoorMemory {
    fn from(door: &DoorSnapshot) -> Self {
        Self {
            slot: door.slot,
            target: door.target.clone(),
            next_room: door.next_room,
            key: door.key,
            lock: door.lock,
            x: door.x,
            z: door.z,
            width: door.width,
            depth: door.depth,
        }
    }
}

/// A read-only snapshot of the session, built once per bot tick.
#[derive(Debug)]
struct SessionSnapshot {
    room: RoomId,
    camera: usize,
    player: PlayerSnapshot,
    scene: SceneSnapshot,
    actions: Vec<ActionSnapshot>,
    doors: Vec<DoorSnapshot>,
    enemies: Vec<EnemySnapshot>,
    weapon: WeaponSnapshot,
    /// The pause menu as the engine shows it, read-only.
    menu: MenuSnapshot,
    flags: [FlagBank; game::FLAG_BANK_COUNT],
    inventory: Vec<InventoryItem>,
}

#[derive(Clone, Copy, Debug)]
struct PlayerSnapshot {
    x: i32,
    y: i32,
    z: i32,
    angle: u16,
    health: i16,
    max_health: i16,
    state: u8,
    being_attacked: u8,
    equipped: Option<u8>,
    radius: i32,
    collision_flags: u8,
    /// The entity's grabbed zone flag (`0x80`). Combat stands down while a
    /// grab owns the player.
    grabbed: bool,
}

/// One active monster. Rows are sorted by `distance`, nearest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EnemySnapshot {
    slot: u8,
    id: u8,
    x: i32,
    y: i32,
    z: i32,
    angle: u16,
    /// Absolute 12-bit bearing from the player to the enemy.
    bearing: u16,
    health: i16,
    state: u8,
    status_flags: u8,
    action_behavior: u8,
    action_state: u8,
    hit_state: u8,
    /// Horizontal distance from the player.
    distance: i32,
    /// Whether the player's sight line to the enemy is clear.
    visible: bool,
}

/// The player's weapon sub-machine and inventory counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WeaponSnapshot {
    /// The equipped inventory item, or `None`.
    equipped: Option<u8>,
    /// The `g_playerEntity.equippedWeaponId` the aim/fire machine uses.
    weapon: u8,
    /// Rounds left in the equipped weapon's inventory stack.
    clip: u32,
    /// Ammunition item count (`weapon + 9`) in the inventory.
    reserve: u32,
    behavior: u8,
    state: u8,
    aim_state: u8,
    target: Option<u8>,
}

#[derive(Clone, Copy, Debug)]
struct SceneSnapshot {
    mode: &'static str,
    skippable: bool,
    film: bool,
    transition: bool,
    cutscene_skip: bool,
    message: Option<u16>,
    message_takes_input: bool,
    /// The window is in a page wait, a finished-text wait or a yes/no prompt:
    /// the next fresh action edge is consumed.
    message_waiting: bool,
    pickup: bool,
    frozen: bool,
    controls_locked: bool,
    controllable: bool,
    dead: bool,
}

#[derive(Clone, Copy, Debug)]
struct ActionSnapshot {
    slot: u8,
    kind: &'static str,
    x: i32,
    z: i32,
    width: i32,
    depth: i32,
    item: Option<u8>,
    quantity: Option<u8>,
    next_room: Option<u8>,
    key: Option<u8>,
    lock: Option<u8>,
    handler: u8,
    flags: u8,
}

#[derive(Clone, Debug)]
struct DoorSnapshot {
    slot: u8,
    target: Option<String>,
    next_room: u8,
    key: u8,
    lock: u8,
    x: i32,
    z: i32,
    width: i32,
    depth: i32,
}

/// One inventory slot as the pause menu lays it out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MenuItem {
    /// The menu's zero-based slot (cursor `8 + 2 * slot`).
    slot: usize,
    /// Item id; `0` is an empty slot.
    id: u8,
    quantity: u8,
    /// The catalog kind (`"weapon"`, `"heal"`, `"key"`, ...).
    kind: &'static str,
}

/// The pause/inventory menu, read from the engine's real UI state: the
/// session's menu slot, fade phase and sub-screens, the values the menu
/// script publishes (`cursor`, `slot`, `selected_item`, ...) and the
/// submenu fields of its `M.state` table. Nothing here is derived from what
/// the bot pressed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct MenuSnapshot {
    /// The menu screen exists (open, fading in, or fading out to close).
    open: bool,
    /// START was taken this frame; the menu opens on the next one.
    requested: bool,
    /// `"closed"`, `"requested"`, `"fade_out"`, `"fade_in"`, `"opening"`,
    /// `"closing"`, `"screen"` (a sub-screen owns the input), `"message"` or
    /// `"input"`.
    phase: &'static str,
    /// The menu takes pad input this frame (`phase == "input"`).
    accepts_input: bool,
    /// The sub-screen over the panel: `"item_box"`, `"map"`, `"file"`,
    /// `"examine"` or `"pickup"`.
    screen: Option<&'static str>,
    /// The menu's cursor byte: `0, 2, 4, 6` the top tabs, `8 + 2 * slot`
    /// the item slots.
    cursor: Option<i64>,
    /// The inventory slot under the cursor, if it is on one.
    slot: Option<i64>,
    /// `"navigation"`, `"actions"` (the item submenu) or `"move"`.
    mode: Option<&'static str>,
    /// The highlighted submenu option, zero-based.
    option: Option<i64>,
    /// The submenu's options for the selected item, top to bottom.
    options: Vec<&'static str>,
    /// The item under the cursor (`0` for none).
    selected_item: u8,
    /// The submenu's open animation has finished.
    ready: bool,
    /// The submenu's close animation is running.
    closing: bool,
    /// The CHECK slide or viewer is up.
    viewing: bool,
    /// The character's inventory slot count.
    capacity: usize,
    items: Vec<MenuItem>,
}

/// The submenu labels the RE1 menu script draws (`draw_submenu`): the first
/// option is EQUIP for a weapon and USE otherwise; MOVE is the combine.
fn menu_options(equippable: bool) -> Vec<&'static str> {
    vec![if equippable { "EQUIP" } else { "USE" }, "CHECK", "MOVE"]
}

impl MenuSnapshot {
    fn build(session: &GameSession) -> Self {
        let game = &session.game;
        let capacity = game.inventory_capacity();
        let items = (0..capacity)
            .map(|slot| {
                let stack = game.inventory.get(slot).copied().unwrap_or_default();
                MenuItem {
                    slot,
                    id: stack.id,
                    quantity: stack.quantity,
                    kind: game.item_defs.get(stack.id).kind.name(),
                }
            })
            .collect();
        let screen = if session.pickup_view.is_some() {
            Some("pickup")
        } else if session.examine.is_some() {
            Some("examine")
        } else if session.item_box.is_some() {
            Some("item_box")
        } else if session.map.is_some() {
            Some("map")
        } else if session.file.is_some() {
            Some("file")
        } else {
            None
        };
        let requested = session.menu_request.is_some();
        let Some(menu) = session.menu.as_deref() else {
            return Self {
                requested,
                phase: if requested { "requested" } else { "closed" },
                capacity,
                items,
                ..Self::default()
            };
        };
        let viewing = menu.value("viewing") != 0;
        let phase = match session.menu_fade_phase {
            super::MenuFadePhase::Out => "fade_out",
            super::MenuFadePhase::In => "fade_in",
            super::MenuFadePhase::Closing => "closing",
            super::MenuFadePhase::None if session.menu_open_delay => "opening",
            super::MenuFadePhase::None if screen.is_some() || viewing => "screen",
            super::MenuFadePhase::None if game.message.active => "message",
            super::MenuFadePhase::None => "input",
        };
        let mode = menu.state_int("mode").map(|mode| match mode {
            0 => "navigation",
            1 => "actions",
            _ => "move",
        });
        let selected_item = menu.value("selected_item") as u8;
        let options = match menu.state_int("equippable") {
            Some(equippable) if selected_item != 0 => menu_options(equippable != 0),
            _ => Vec::new(),
        };
        let slot = menu.value("slot");
        Self {
            open: true,
            requested,
            phase,
            accepts_input: phase == "input",
            screen,
            cursor: Some(menu.value("cursor")),
            slot: (slot >= 0).then_some(slot),
            mode,
            option: menu.state_int("action"),
            options,
            selected_item,
            ready: menu.value("ready") != 0,
            closing: menu.value("closing") != 0,
            viewing,
            capacity,
            items,
        }
    }
}

fn action_kind_name(kind: RoomActionKind) -> &'static str {
    match kind {
        RoomActionKind::Door => "door",
        RoomActionKind::Item => "item",
        RoomActionKind::ItemBox => "item_box",
        RoomActionKind::Event => "event",
        RoomActionKind::Message => "message",
        RoomActionKind::Typewriter => "typewriter",
        RoomActionKind::EffectZone => "effect_zone",
        RoomActionKind::Desk => "desk",
        RoomActionKind::StairsZone => "stairs_zone",
        RoomActionKind::StairsHeight => "stairs_height",
        RoomActionKind::DoorCheck => "door_check",
        RoomActionKind::FlagBankSet => "flag_bank_set",
        RoomActionKind::Other => "other",
    }
}

/// The active monsters a combat policy can see: entity ids below `0x20` (the
/// scripted characters `0x20..=0x2E` are excluded), active, and alive. Bosses
/// keep their normal ids and are included; the policy decides whether to fight
/// them. Sorted nearest first.
fn enemy_snapshots(
    game: &game::GameState,
    player_pos: [i32; 3],
    room: &RoomState,
) -> Vec<EnemySnapshot> {
    let mut enemies = Vec::new();
    for (slot, entity) in game.entities.iter().enumerate().skip(1) {
        if !entity.active() || entity.id >= 0x20 || entity.health <= 0 {
            continue;
        }
        let dx = entity.pos[0] - player_pos[0];
        let dz = entity.pos[2] - player_pos[2];
        let distance =
            ((i64::from(dx) * i64::from(dx) + i64::from(dz) * i64::from(dz)) as f64).sqrt() as i32;
        let visible = crate::combat::check_weapon_line_of_sight(game, room, entity.pos) == 0;
        enemies.push(EnemySnapshot {
            slot: slot as u8,
            id: entity.id,
            x: entity.pos[0],
            y: entity.pos[1],
            z: entity.pos[2],
            angle: entity.angle,
            bearing: crate::sfx::angle_between_xz(
                player_pos[0],
                player_pos[2],
                entity.pos[0],
                entity.pos[2],
            ),
            health: entity.health,
            state: entity.state(),
            status_flags: entity.status_flags,
            action_behavior: entity.action_behavior,
            action_state: entity.action_state,
            hit_state: entity.hit_state,
            distance,
            visible,
        });
    }
    enemies.sort_by_key(|enemy| (enemy.distance, enemy.slot));
    enemies
}

/// The weapon and inventory state the aim/fire machine reads.
fn weapon_snapshot(game: &game::GameState, state: &WeaponState) -> WeaponSnapshot {
    let weapon = game.equipped_weapon_id;
    let clip = game
        .inventory
        .iter()
        .find(|stack| stack.id == weapon)
        .map_or(0, |stack| {
            u32::from(crate::weapons::clip_rounds(weapon, stack.quantity))
        });
    let reserve = if weapon == 0 {
        0
    } else {
        game.item_count(crate::weapons::weapon_ammo_item_id(weapon))
    };
    WeaponSnapshot {
        equipped: game.equipped,
        weapon,
        clip,
        reserve,
        behavior: state.behavior,
        state: state.state,
        aim_state: state.aim_state,
        target: state.target,
    }
}

impl SessionSnapshot {
    fn build(session: &GameSession) -> Self {
        let game = &session.game;
        let id = session.loaded.id;
        let entity = &game.entities[0];
        let message = game.message.active.then(|| {
            u16::from(game.message.id.unwrap_or(0)) | (u16::from(game.message.id_high) << 8)
        });
        let frozen =
            (game.message_flags & game::MESSAGE_FLAG_PLAYER_STATE == 0) || game.itembox.menu_open;
        let controls_locked =
            entity.zone_flags & 0x20 != 0 || game.message_flags & game::MESSAGE_FLAG_CONTROLS == 0;
        let controllable = session.movie.is_none()
            && session.transition.is_none()
            && session.cutscene_skip.is_none()
            && !session.room_frozen()
            && session.death.is_idle()
            && session.boot_message.is_none()
            && !game.message.active
            && !frozen
            && !controls_locked;
        let scene = SceneSnapshot {
            mode: if session.movie.is_some() {
                "movie"
            } else if session.transition.is_some() {
                "transition"
            } else {
                "play"
            },
            skippable: session.cutscene_skippable(),
            film: session.movie.is_some(),
            transition: session.transition.is_some(),
            cutscene_skip: session.cutscene_skip.is_some(),
            message,
            message_takes_input: game.message.takes_input(),
            message_waiting: game.message.active
                && matches!(
                    game.message.phase(),
                    crate::message::MessagePhase::PageWait
                        | crate::message::MessagePhase::WaitInput
                        | crate::message::MessagePhase::YesNo
                ),
            pickup: session.pickup_view.is_some(),
            frozen,
            controls_locked,
            controllable,
            dead: !session.death.is_idle()
                || game.flags[5].bit(game::MSF_PLAYER_DEAD)
                || entity.health <= 0,
        };
        let player = PlayerSnapshot {
            x: session.player.pos[0],
            y: session.player.pos[1],
            z: session.player.pos[2],
            angle: session.player.angle,
            health: entity.health,
            max_health: game.max_health,
            state: entity.state(),
            being_attacked: entity.is_being_attacked,
            equipped: game.equipped,
            radius: session.player.radius,
            collision_flags: session.player.collision_flags,
            grabbed: entity.zone_flags & 0x80 != 0,
        };
        let actions = game
            .room_actions
            .iter()
            .flatten()
            .map(|action| {
                let door = game
                    .doors
                    .iter()
                    .flatten()
                    .find(|door| door.slot == action.slot);
                ActionSnapshot {
                    slot: action.slot,
                    kind: action_kind_name(action.kind),
                    x: i32::from(action.zone[0]),
                    z: i32::from(action.zone[1]),
                    width: i32::from(action.zone[2]),
                    depth: i32::from(action.zone[3]),
                    item: (action.kind == RoomActionKind::Item).then(|| action.item_id()),
                    quantity: (action.kind == RoomActionKind::Item).then(|| action.item_quantity()),
                    next_room: door.map(|door| door.next_room),
                    key: door.map(|door| door.key),
                    lock: door.map(|door| door.lock),
                    handler: action.handler,
                    flags: action.flags,
                }
            })
            .collect();
        let doors = game
            .doors
            .iter()
            .flatten()
            .map(|door| DoorSnapshot {
                slot: door.slot,
                target: game
                    .door_destination(id, door.next_room)
                    .map(|target| target.room3()),
                next_room: door.next_room,
                key: door.key,
                lock: door.lock,
                x: i32::from(door.zone[0]),
                z: i32::from(door.zone[1]),
                width: i32::from(door.zone[2]),
                depth: i32::from(door.zone[3]),
            })
            .collect();
        let enemies = enemy_snapshots(game, session.player.pos, &session.loaded.room);
        let weapon = weapon_snapshot(game, &session.player.weapon);
        let menu = MenuSnapshot::build(session);
        Self {
            room: id,
            camera: session.loaded.room.current_cut,
            player,
            scene,
            actions,
            doors,
            enemies,
            weapon,
            menu,
            flags: game.flags,
            inventory: game.inventory.clone(),
        }
    }

    fn flag(&self, bank: u8, bit: u8) -> bool {
        self.flags
            .get(usize::from(bank))
            .is_some_and(|flags| flags.bit(bit))
    }

    fn item_count(&self, item: u8) -> u32 {
        self.inventory
            .iter()
            .filter(|stack| stack.id == item)
            .map(|stack| u32::from(stack.quantity))
            .sum()
    }

    fn has_item(&self, item: u8) -> bool {
        self.inventory.iter().any(|stack| stack.id == item)
    }
}

/// The grid used by [`BotNav`]: 0..=32768 on both axes, in 512-unit steps.
const GRID_MIN: i32 = 0;
const GRID_MAX: i32 = 32768;
const GRID_STEP: i32 = 512;
const GRID_DIM: usize = ((GRID_MAX - GRID_MIN) / GRID_STEP + 1) as usize;
const GRID_NODES: usize = GRID_DIM * GRID_DIM;

/// Default `walk_to` arrival radius.
const DEFAULT_TOLERANCE: i32 = 300;
/// The heading error at or below which `face_to` reports `facing`, and the
/// in-place fallback walks on.
const FORWARD_DEADBAND: i32 = 0x40;
/// Remaining distance above which a run may start.
const RUN_START_DISTANCE: i32 = 800;
/// Remaining distance at or below which a run walks the final approach.
const RUN_STOP_DISTANCE: i32 = 400;
/// With a `face` option, the path is planned through a point this far behind
/// the goal on the line toward the face point, so the last stretch already
/// heads at the zone.
const FACE_LEAD: i32 = 800;
/// Within this distance of a `face` goal the walk steers onto the approach
/// line (see [`steer::face_carrot`]).
const FACE_APPROACH: i32 = 1400;
/// With a `next` waypoint, the walk starts turning toward it this far from
/// the goal.
const CORNER_RADIUS: i32 = 800;
/// With a `next` waypoint, a goal the bot is past (on the next waypoint's
/// side) counts as reached within this distance, so the corner is cut instead
/// of overshot and turned back on.
const CORNER_ARRIVE: i32 = 400;
/// An enemy-avoiding walk replans this often, so the path follows the
/// monsters as they move.
const AVOID_REPLAN_TICKS: u32 = 20;
/// Advance to the next waypoint within this distance of the current one.
const WAYPOINT_RADIUS: i32 = 256;
/// Pure-pursuit lookahead: aim at the farthest path point within this distance
/// that the player can already see, so the bot arcs through smoothed corners
/// instead of stopping on each one.
const LOOKAHEAD_DISTANCE: i32 = 1600;
/// A forward step shorter than this counts as no progress.
const STUCK_STEP: i32 = 8;
/// A position jump larger than this is a scripted move or a room warp, not
/// walking: the cached path is planned from the old position and must go.
const TELEPORT_STEP: i32 = 1500;
/// Forward ticks with no progress before a replan.
const STUCK_TICKS: u32 = 12;
/// Replans before `walk_to` reports `blocked`.
const MAX_REPLANS: u32 = 3;

/// The per-room navigation state.
#[derive(Debug, Default)]
struct BotNav {
    room_id: Option<RoomId>,
    /// Only the collision query is needed; the rest of `RoomState` stays out.
    collision: RoomState,
    goal: Option<(i32, i32)>,
    tolerance: i32,
    path: Vec<(i32, i32)>,
    index: usize,
    unreachable_goal: Option<(i32, i32)>,
    blocked_goal: Option<(i32, i32)>,
    last_pos: Option<(i32, i32)>,
    last_forward: bool,
    /// Whether the last walk tick ran. The run gait carries a cost to drop
    /// and to pick up again ([`steer::steer`]), so it holds through corners
    /// it can steer.
    running: bool,
    /// The `face` point of the current goal.
    face: Option<(i32, i32)>,
    /// The path index of the face approach point: the look-ahead does not
    /// skip past it until the bot is on the approach line.
    face_pin: Option<usize>,
    /// Whether the current goal routes around enemies.
    avoid: bool,
    /// The enemy zones the current path was planned with.
    zones: Vec<steer::EnemyZone>,
    /// Walk ticks since the enemy-avoiding path was planned.
    avoid_ticks: u32,
    /// The heading change the engine applies if this tick's turn press is
    /// released next tick ([`steer::release_delta`]).
    release_now: i32,
    /// The same for the previous tick: what releasing now still turns.
    release_prev: i32,
    /// Whether the previous tick's pad held aim: `face_to` then steers the
    /// aim, whose turn the release model does not describe.
    aiming: bool,
    /// Whether `walk_to` ran this tick. A tick without it (a scene, a door
    /// press, a `face_to`) drops the run latch.
    driving: bool,
    stuck: u32,
    replans: u32,
}

/// One collision query snapshot for a room, refreshed on a room change.
fn collision_room(room: &RoomState) -> RoomState {
    RoomState {
        collision: room.collision.clone(),
        sca: room.sca.clone(),
        ..RoomState::default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Turn {
    None,
    Left,
    Right,
}

/// A planned path and the index of its face approach point, if it has one.
type PlannedPath = (Vec<(i32, i32)>, Option<usize>);

/// The result of the read-only `bot:path` query.
#[derive(Clone, Debug, PartialEq, Eq)]
struct PathQuery {
    points: Vec<(i32, i32)>,
    length: i32,
    /// The enemy zones' summed cost along the path (always measured, whether
    /// or not the plan avoided them).
    enemy_cost: u32,
}

#[derive(Clone, Copy, Debug)]
struct NavOutcome {
    forward: bool,
    run: bool,
    turn: Turn,
    status: &'static str,
    distance: i32,
    angle_error: i32,
}

/// The options `bot:walk_to(x, z, opts)` accepts.
#[derive(Clone, Copy, Debug)]
struct WalkOptions {
    run: bool,
    tolerance: i32,
    /// The interaction zone point the walk should arrive facing.
    face: Option<(i32, i32)>,
    /// Plan around the enemies' avoidance zones.
    avoid_enemies: bool,
    /// The waypoint after this one: the walk turns toward it on the last
    /// stretch, and a goal it has passed on the way there counts as reached
    /// within [`CORNER_ARRIVE`].
    next: Option<(i32, i32)>,
}

impl Default for WalkOptions {
    fn default() -> Self {
        Self {
            run: false,
            tolerance: DEFAULT_TOLERANCE,
            face: None,
            avoid_enemies: false,
            next: None,
        }
    }
}

/// Whether `pos` is past `corner` on the way to `next`.
fn passed(pos: (i32, i32), corner: (i32, i32), next: (i32, i32)) -> bool {
    let along = i64::from(pos.0 - corner.0) * i64::from(next.0 - corner.0)
        + i64::from(pos.1 - corner.1) * i64::from(next.1 - corner.1);
    along >= 0
}

fn distance_xz(from: (i32, i32), to: (i32, i32)) -> i32 {
    let dx = i64::from(to.0) - i64::from(from.0);
    let dz = i64::from(to.1) - i64::from(from.1);
    ((dx * dx + dz * dz) as f64).sqrt() as i32
}

/// Signed shortest heading error from `current` to `target`, in 12-bit units.
/// Positive means turn right (the player's `angle` grows clockwise).
fn angle_error(current: u16, target: u16) -> i32 {
    let diff = i32::from(target.wrapping_sub(current) & 0x0FFF);
    if diff > 0x800 { diff - 0x1000 } else { diff }
}

fn grid_xy(index: usize) -> (i32, i32) {
    (
        GRID_MIN + (index % GRID_DIM) as i32 * GRID_STEP,
        GRID_MIN + (index / GRID_DIM) as i32 * GRID_STEP,
    )
}

fn grid_index(x: i32, z: i32) -> Option<usize> {
    if !(GRID_MIN..=GRID_MAX).contains(&x) || !(GRID_MIN..=GRID_MAX).contains(&z) {
        return None;
    }
    let ix = ((x - GRID_MIN) / GRID_STEP) as usize;
    let iz = ((z - GRID_MIN) / GRID_STEP) as usize;
    Some(iz * GRID_DIM + ix)
}

fn nearest_grid(x: i32, z: i32) -> (i32, i32) {
    let snap = |value: i32| {
        let snapped =
            (value - GRID_MIN + GRID_STEP / 2).div_euclid(GRID_STEP) * GRID_STEP + GRID_MIN;
        snapped.clamp(GRID_MIN, GRID_MAX)
    };
    (snap(x), snap(z))
}

/// Whether `(x, z)` is blocked for the player body.
///
/// The engine's collision resolver grows every box by an 18-unit skin on top
/// of the player radius; `position_blocked` alone does not. Planning with the
/// bare radius lets a smoothed path graze a wall that the resolver then pushes
/// against, so the bot wedges and replans. Add the skin here so the plan has
/// the same margin the movement has.
fn blocked(room: &RoomState, x: i32, z: i32, y: i32, radius: i32, flags: u8) -> bool {
    player::position_blocked(
        room,
        [x, y, z],
        radius + i32::from(player::COLLISION_SKIN),
        flags,
    )
}

/// Whether a straight segment is clear enough to walk, sampled every 96 units.
fn line_clear(
    room: &RoomState,
    from: (i32, i32),
    to: (i32, i32),
    y: i32,
    radius: i32,
    flags: u8,
) -> bool {
    let length = distance_xz(from, to);
    let samples = ((length + 95) / 96).max(1);
    for step in 1..=samples {
        let x = from.0 + (to.0 - from.0) * step / samples;
        let z = from.1 + (to.1 - from.1) * step / samples;
        if blocked(room, x, z, y, radius, flags) {
            return false;
        }
    }
    true
}

/// Find the nearest free grid node to `(x, z)`, searching a few rings.
fn nearest_free(
    room: &RoomState,
    x: i32,
    z: i32,
    y: i32,
    radius: i32,
    flags: u8,
) -> Option<(i32, i32)> {
    let (cx, cz) = nearest_grid(x, z);
    let index = grid_index(cx, cz)?;
    let base = (index % GRID_DIM, index / GRID_DIM);
    for ring in 0..=4i32 {
        let mut best: Option<((i32, i32), i32)> = None;
        for dz in -ring..=ring {
            for dx in -ring..=ring {
                if ring > 0 && dx.abs() != ring && dz.abs() != ring {
                    continue;
                }
                let ix = base.0 as i32 + dx;
                let iz = base.1 as i32 + dz;
                if !(0..GRID_DIM as i32).contains(&ix) || !(0..GRID_DIM as i32).contains(&iz) {
                    continue;
                }
                let (nx, nz) = grid_xy(iz as usize * GRID_DIM + ix as usize);
                if blocked(room, nx, nz, y, radius, flags) {
                    continue;
                }
                let dist = (nx - x).pow(2) + (nz - z).pow(2);
                if best.is_none_or(|(_, best_dist)| dist < best_dist) {
                    best = Some(((nx, nz), dist));
                }
            }
        }
        if let Some((point, _)) = best {
            return Some(point);
        }
    }
    None
}

/// A* over the 65x65 collision grid. Returns grid-cell centres from start to
/// goal, or `None` when the goal is enclosed.
#[cfg(test)]
fn find_grid_path(
    room: &RoomState,
    start: (i32, i32),
    goal: (i32, i32),
    y: i32,
    radius: i32,
    flags: u8,
) -> Option<Vec<(i32, i32)>> {
    find_grid_path_with(room, start, goal, y, radius, flags, &[])
}

/// [`find_grid_path`] with each node also paying the enemy zones' cost.
fn find_grid_path_with(
    room: &RoomState,
    start: (i32, i32),
    goal: (i32, i32),
    y: i32,
    radius: i32,
    flags: u8,
    zones: &[steer::EnemyZone],
) -> Option<Vec<(i32, i32)>> {
    const DIRS: [(i32, i32, u32); 8] = [
        (1, 0, 10),
        (-1, 0, 10),
        (0, 1, 10),
        (0, -1, 10),
        (1, 1, 14),
        (1, -1, 14),
        (-1, 1, 14),
        (-1, -1, 14),
    ];
    let exact_goal = goal;
    let start = nearest_free(room, start.0, start.1, y, radius, flags)?;
    // The player may start wedged in a soft spot, but a goal the route names
    // must be walkable; silently snapping it to the far side of a wall would
    // make `arrived` lie about where the player is.
    let goal = nearest_grid(goal.0, goal.1);
    if blocked(room, goal.0, goal.1, y, radius, flags) {
        return None;
    }
    let start_index = grid_index(start.0, start.1)?;
    let goal_index = grid_index(goal.0, goal.1)?;
    let mut blocked_nodes = vec![false; GRID_NODES];
    for (index, node) in blocked_nodes.iter_mut().enumerate() {
        let (x, z) = grid_xy(index);
        *node = blocked(room, x, z, y, radius, flags);
    }
    let mut open: BinaryHeap<(Reverse<u32>, usize)> = BinaryHeap::new();
    let mut cost = vec![u32::MAX; GRID_NODES];
    let mut parent = vec![usize::MAX; GRID_NODES];
    let mut closed = vec![false; GRID_NODES];
    let heuristic = |index: usize| -> u32 {
        let (x, z) = grid_xy(index);
        let dx = (x - goal.0).unsigned_abs() / GRID_STEP as u32;
        let dz = (z - goal.1).unsigned_abs() / GRID_STEP as u32;
        let diagonal = dx.min(dz);
        (dx + dz - diagonal) * 10 + diagonal * 14
    };
    cost[start_index] = 0;
    open.push((Reverse(heuristic(start_index)), start_index));
    while let Some((Reverse(_), index)) = open.pop() {
        if index == goal_index {
            let mut path = vec![grid_xy(index)];
            let mut current = index;
            while parent[current] != usize::MAX {
                current = parent[current];
                path.push(grid_xy(current));
            }
            path.reverse();
            // The search snaps the goal to a 512-unit node. End the path at
            // the point the route actually named when the last leg is clear,
            // so the bot does not run past it and spin back.
            if exact_goal != goal && !blocked(room, exact_goal.0, exact_goal.1, y, radius, flags) {
                let last = path.len() - 1;
                if last > 0 && line_clear(room, path[last - 1], exact_goal, y, radius, flags) {
                    path[last] = exact_goal;
                } else if line_clear(room, path[last], exact_goal, y, radius, flags) {
                    path.push(exact_goal);
                }
            }
            return Some(path);
        }
        if closed[index] {
            continue;
        }
        closed[index] = true;
        let (x, z) = grid_xy(index);
        for (dx, dz, step_cost) in DIRS {
            let nx = x + dx * GRID_STEP;
            let nz = z + dz * GRID_STEP;
            let Some(next) = grid_index(nx, nz) else {
                continue;
            };
            if closed[next] || blocked_nodes[next] {
                continue;
            }
            // A diagonal may not cut a blocked corner: both orthogonal
            // neighbours must be walkable too.
            if dx != 0 && dz != 0 {
                let side_x = grid_index(nx, z).is_some_and(|side| !blocked_nodes[side]);
                let side_z = grid_index(x, nz).is_some_and(|side| !blocked_nodes[side]);
                if !side_x || !side_z {
                    continue;
                }
            }
            // The wall can be thinner than the 512-unit grid step, so an
            // endpoint-only test would jump it. Sample the edge itself.
            if !line_clear(room, (x, z), (nx, nz), y, radius, flags) {
                continue;
            }
            let next_cost = cost[index]
                .saturating_add(step_cost)
                .saturating_add(steer::zone_penalty(zones, nx, nz));
            if next_cost < cost[next] {
                cost[next] = next_cost;
                parent[next] = index;
                open.push((Reverse(next_cost + heuristic(next)), next));
            }
        }
    }
    None
}

/// Greedily drop waypoints whose straight segment is clear (and, with enemy
/// zones, does not cut back into a zone the grid path went around).
fn smooth_path(
    room: &RoomState,
    start: (i32, i32),
    path: Vec<(i32, i32)>,
    y: i32,
    radius: i32,
    flags: u8,
    zones: &[steer::EnemyZone],
) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    let mut anchor = start;
    let mut index = 0;
    while index < path.len() {
        let mut best = None;
        for candidate in (index..path.len()).rev() {
            if line_clear(room, anchor, path[candidate], y, radius, flags)
                && steer::segment_avoids(zones, anchor, path[candidate])
            {
                best = Some(candidate);
                break;
            }
        }
        let Some(best) = best else {
            break;
        };
        out.push(path[best]);
        anchor = path[best];
        index = best + 1;
    }
    out
}

impl BotNav {
    fn sync_room(&mut self, id: RoomId, room: &RoomState) {
        if self.room_id == Some(id) {
            return;
        }
        self.room_id = Some(id);
        self.collision = collision_room(room);
        self.goal = None;
        self.path.clear();
        self.index = 0;
        self.unreachable_goal = None;
        self.blocked_goal = None;
        self.last_pos = None;
        self.last_forward = false;
        self.running = false;
        self.driving = false;
        self.stuck = 0;
        self.replans = 0;
        self.face = None;
        self.face_pin = None;
        self.avoid = false;
        self.zones.clear();
        self.avoid_ticks = 0;
    }

    fn clear_goal(&mut self) {
        self.goal = None;
        self.path.clear();
        self.index = 0;
        self.unreachable_goal = None;
        self.blocked_goal = None;
        self.running = false;
        self.face_pin = None;
    }

    /// Drop the cached path so the next walk tick replans.
    fn drop_path(&mut self) {
        self.path.clear();
        self.index = 0;
        self.face_pin = None;
    }

    /// Plan a smoothed path from `pos` to `goal`, through the face approach
    /// point when one is walkable. Returns the path and the approach point's
    /// index in it.
    fn plan(
        &self,
        pos: (i32, i32),
        goal: (i32, i32),
        face: Option<(i32, i32)>,
        body: steer::Body,
        zones: &[steer::EnemyZone],
    ) -> Option<PlannedPath> {
        let room = &self.collision;
        let (y, radius, flags) = (body.y, body.radius, body.flags);
        let approach = face
            .and_then(|face| steer::approach_point(goal, face, FACE_LEAD))
            .filter(|&point| {
                !blocked(room, point.0, point.1, y, radius, flags)
                    && line_clear(room, point, goal, y, radius, flags)
                    && distance_xz(pos, goal) > FACE_LEAD / 2
            });
        if let Some(point) = approach
            && let Some(path) = find_grid_path_with(room, pos, point, y, radius, flags, zones)
        {
            let mut path = smooth_path(room, pos, path, y, radius, flags, zones);
            if path.last() == Some(&point) {
                path.push(goal);
                let pin = path.len() - 2;
                return Some((path, Some(pin)));
            }
        }
        let path = find_grid_path_with(room, pos, goal, y, radius, flags, zones)?;
        Some((smooth_path(room, pos, path, y, radius, flags, zones), None))
    }

    /// Whether the bot can steer straight from `from` to `to`: clear of the
    /// room, and with enemy zones, not deeper into one than its endpoints.
    fn visible(&self, from: (i32, i32), to: (i32, i32), body: steer::Body) -> bool {
        line_clear(&self.collision, from, to, body.y, body.radius, body.flags)
            && steer::segment_avoids(&self.zones, from, to)
    }

    /// Start a bot tick: last tick's press becomes the pending release.
    fn begin_tick(&mut self) {
        self.release_prev = self.release_now;
        self.release_now = 0;
        self.driving = false;
    }

    /// End a bot tick with the pad the script left. A turn combined with
    /// aim, a backward walk or a forward walk the nav did not choose is not
    /// an in-place turn, so it leaves no in-place release to plan around.
    fn end_tick(&mut self, input: &BotInput) {
        let pad = input.input;
        if pad.aim || pad.down || (pad.up && !self.driving) {
            self.release_now = 0;
        }
        self.aiming = pad.aim;
        if !self.driving {
            // The script did not drive movement this tick (a scene, a door
            // press, a `face_to`): do not resume a stale run next tick.
            self.clear_run();
        }
    }

    /// Drop the run latch without touching the path. The runtime calls this
    /// when the script stops feeding movement for a tick.
    fn clear_run(&mut self) {
        self.running = false;
    }

    /// The `walk_to` skill: collision-aware movement toward `(x, z)`.
    #[cfg(test)]
    fn walk_to(
        &mut self,
        player: &PlayerSnapshot,
        x: i32,
        z: i32,
        opts: WalkOptions,
    ) -> NavOutcome {
        self.walk_to_with(player, x, z, opts, &[])
    }

    /// [`Self::walk_to`] with the live enemies, which an `avoid_enemies` walk
    /// plans around.
    fn walk_to_with(
        &mut self,
        player: &PlayerSnapshot,
        x: i32,
        z: i32,
        opts: WalkOptions,
        enemies: &[EnemySnapshot],
    ) -> NavOutcome {
        self.driving = true;
        let pos = (player.x, player.z);
        let goal = (x, z);
        let body = steer::Body {
            y: player.y,
            radius: player.radius,
            flags: player.collision_flags,
        };
        let distance = distance_xz(pos, goal);
        let stop = |status: &'static str| NavOutcome {
            forward: false,
            run: false,
            turn: Turn::None,
            status,
            distance,
            angle_error: 0,
        };
        // A short next leg is walked exactly: cutting into it would skip it.
        let corner_arrive = |next: (i32, i32)| CORNER_ARRIVE.min(distance_xz(goal, next) / 2);
        let cut_corner = opts.next.is_some_and(|next| {
            distance <= opts.tolerance.max(corner_arrive(next))
                && passed(pos, goal, next)
                && line_clear(&self.collision, pos, next, body.y, body.radius, body.flags)
        });
        if distance <= opts.tolerance || cut_corner {
            self.clear_goal();
            return stop("arrived");
        }
        if self.goal != Some(goal)
            || self.tolerance != opts.tolerance
            || self.face != opts.face
            || self.avoid != opts.avoid_enemies
        {
            self.goal = Some(goal);
            self.tolerance = opts.tolerance;
            self.face = opts.face;
            self.avoid = opts.avoid_enemies;
            self.drop_path();
            self.unreachable_goal = None;
            self.blocked_goal = None;
            self.running = false;
            self.replans = 0;
            self.stuck = 0;
        }
        // A big jump is a scripted move (the 105 Barry scene, a cutscene
        // teleport), so throw away the path planned from the old position,
        // along with any terminal status that only applied there, and replan
        // from where the player actually is.
        let jumped = self
            .last_pos
            .is_some_and(|last| (pos.0 - last.0).abs() + (pos.1 - last.1).abs() > TELEPORT_STEP);
        if jumped {
            self.drop_path();
            self.stuck = 0;
            self.replans = 0;
            self.unreachable_goal = None;
            self.blocked_goal = None;
        }
        if self.blocked_goal == Some(goal) {
            self.running = false;
            return stop("blocked");
        }
        if self.unreachable_goal == Some(goal) {
            self.running = false;
            return stop("unreachable");
        }
        // Track forward progress: turning in place is not a stall.
        if let Some(last) = self.last_pos {
            let moved = (pos.0 - last.0).abs() + (pos.1 - last.1).abs();
            if self.last_forward && moved < STUCK_STEP {
                self.stuck += 1;
            } else if moved >= STUCK_STEP {
                self.stuck = 0;
            }
        }
        self.last_pos = Some(pos);
        if self.stuck >= STUCK_TICKS {
            self.stuck = 0;
            self.replans += 1;
            self.drop_path();
            self.running = false;
            if self.replans > MAX_REPLANS {
                self.blocked_goal = Some(goal);
                return stop("blocked");
            }
        }
        if self.avoid && !enemies.is_empty() {
            self.avoid_ticks += 1;
            if self.avoid_ticks >= AVOID_REPLAN_TICKS {
                self.drop_path();
            }
        }
        if self.path.is_empty() || self.index >= self.path.len() {
            self.zones = if self.avoid {
                enemies
                    .iter()
                    .map(steer::EnemyZone::from_snapshot)
                    .collect()
            } else {
                Vec::new()
            };
            self.avoid_ticks = 0;
            let Some((path, pin)) = self.plan(pos, goal, opts.face, body, &self.zones) else {
                self.unreachable_goal = Some(goal);
                self.drop_path();
                self.running = false;
                return stop("unreachable");
            };
            self.path = path;
            self.face_pin = pin;
            self.index = 0;
        }
        // On the face approach line the pinned point has done its job.
        let carrot = opts
            .face
            .and_then(|face| steer::face_carrot(pos, goal, face, FACE_APPROACH))
            .filter(|&carrot| self.visible(pos, carrot, body) && self.visible(pos, goal, body));
        if carrot.is_some() {
            self.face_pin = None;
        }
        // The goal itself is never skipped by the waypoint radius: it ends
        // within `tolerance`, and stepping past it would re-append it every
        // tick. A path that ends short of the goal may step past its end.
        let skip_limit = if self.path.last() == Some(&goal) {
            self.path.len() - 1
        } else {
            self.path.len()
        };
        while self.index < skip_limit && distance_xz(pos, self.path[self.index]) <= WAYPOINT_RADIUS
        {
            self.index += 1;
        }
        // The arc toward the look-ahead point cuts the corner, so it may
        // never come within the waypoint radius: a corner the bot is already
        // past, with the next point in sight, is done.
        while self.index + 1 < self.path.len()
            && self.face_pin.is_none_or(|pin| self.index < pin)
            && passed(pos, self.path[self.index], self.path[self.index + 1])
            && self.visible(pos, self.path[self.index + 1], body)
        {
            self.index += 1;
        }
        if self.face_pin.is_some_and(|pin| self.index > pin) {
            self.face_pin = None;
        }
        // Pure pursuit: of the path points already inside the lookahead,
        // steer at the farthest one the player can see. The smoothed path's
        // points are corners, and line of sight proves a later one is safe to
        // skip, so the bot arcs through the corner instead of stopping on it.
        if self.index < self.path.len() {
            let mut best = self.index;
            let mut best_distance = distance_xz(pos, self.path[self.index]);
            let last = self.face_pin.unwrap_or(self.path.len() - 1);
            for candidate in self.index + 1..=last {
                let distance = distance_xz(pos, self.path[candidate]);
                if distance > LOOKAHEAD_DISTANCE {
                    break;
                }
                if distance >= best_distance && self.visible(pos, self.path[candidate], body) {
                    best = candidate;
                    best_distance = distance;
                }
            }
            self.index = best;
        }
        if self.index >= self.path.len() {
            // The smoothed path ended short of the exact goal: try the goal
            // itself as a direct final waypoint, else replan from here.
            if line_clear(
                &self.collision,
                pos,
                goal,
                player.y,
                player.radius,
                player.collision_flags,
            ) {
                self.path.push(goal);
            } else {
                // The grid goal is walkable but the exact point the route
                // named is inside geometry; report it rather than spinning on
                // a path that can never end there.
                self.unreachable_goal = Some(goal);
                self.drop_path();
                self.running = false;
                return stop("unreachable");
            }
        }
        let final_leg = self.index + 1 == self.path.len();
        // Turning into the next leg: steer at a point that slides from the
        // goal toward the next waypoint as the goal comes near.
        let corner = opts
            .next
            .filter(|&next| {
                final_leg && distance <= CORNER_RADIUS && corner_arrive(next) > opts.tolerance
            })
            .filter(|&next| self.visible(goal, next, body))
            .and_then(|next| {
                let length = distance_xz(goal, next).max(1);
                let slide = (CORNER_RADIUS - distance).min(length);
                let point = (
                    goal.0 + (next.0 - goal.0) * slide / length,
                    goal.1 + (next.1 - goal.1) * slide / length,
                );
                self.visible(pos, point, body).then_some(point)
            });
        let (target, arrive) = if let Some(carrot) = carrot {
            (carrot, opts.tolerance.min(WAYPOINT_RADIUS))
        } else if let Some(point) = corner {
            (point, WAYPOINT_RADIUS)
        } else if final_leg {
            (self.path[self.index], opts.tolerance)
        } else {
            (self.lookahead_point(pos, body), WAYPOINT_RADIUS)
        };
        let allow_run = opts.run
            && if self.running {
                distance > RUN_STOP_DISTANCE
            } else {
                distance > RUN_START_DISTANCE
            };
        let query = steer::SteerQuery {
            pos,
            angle: player.angle,
            target,
            arrive,
            allow_run,
            running: self.running,
        };
        let choice = steer::steer(&self.collision, body, &query);
        let target_angle = crate::sfx::angle_between_xz(pos.0, pos.1, target.0, target.1);
        self.running = choice.run;
        self.last_forward = choice.forward;
        self.release_now = steer::release_delta(&choice);
        NavOutcome {
            forward: choice.forward,
            run: choice.run,
            turn: choice.turn,
            status: "moving",
            distance,
            angle_error: angle_error(player.angle, target_angle),
        }
    }

    /// The steering point past the current waypoint: the farthest point on
    /// the next segment, within the lookahead, that is still in sight. The
    /// bot then starts its arc before the corner instead of on it.
    fn lookahead_point(&self, pos: (i32, i32), body: steer::Body) -> (i32, i32) {
        let corner = self.path[self.index];
        let next_index = self.index + 1;
        if next_index >= self.path.len()
            || self.face_pin.is_some_and(|pin| self.index >= pin)
            || !self.visible(pos, corner, body)
        {
            return corner;
        }
        let next = self.path[next_index];
        let length = distance_xz(corner, next);
        let steps = (length / 256).max(1);
        let mut best = corner;
        for step in 1..=steps {
            let point = (
                corner.0 + (next.0 - corner.0) * step / steps,
                corner.1 + (next.1 - corner.1) * step / steps,
            );
            if distance_xz(pos, point) > LOOKAHEAD_DISTANCE || !self.visible(pos, point, body) {
                break;
            }
            best = point;
        }
        best
    }

    /// The read-only path query behind `bot:path`: the smoothed path from
    /// the player to `(x, z)`, optionally around the enemies, with its
    /// length and enemy-zone exposure. It plans afresh and leaves the walk's
    /// own path alone.
    fn query_path(
        &self,
        player: &PlayerSnapshot,
        goal: (i32, i32),
        avoid_enemies: bool,
        enemies: &[EnemySnapshot],
    ) -> Option<PathQuery> {
        let pos = (player.x, player.z);
        let body = steer::Body {
            y: player.y,
            radius: player.radius,
            flags: player.collision_flags,
        };
        let all: Vec<steer::EnemyZone> = enemies
            .iter()
            .map(steer::EnemyZone::from_snapshot)
            .collect();
        let zones: &[steer::EnemyZone] = if avoid_enemies { &all } else { &[] };
        let (path, _) = self.plan(pos, goal, None, body, zones)?;
        let mut length = 0;
        let mut from = pos;
        for &point in &path {
            length += distance_xz(from, point);
            from = point;
        }
        Some(PathQuery {
            enemy_cost: steer::path_exposure(&all, pos, &path),
            length,
            points: path,
        })
    }

    fn face_to(&mut self, player: &PlayerSnapshot, x: i32, z: i32) -> NavOutcome {
        let pos = (player.x, player.z);
        let target = crate::sfx::angle_between_xz(pos.0, pos.1, x, z);
        let error = angle_error(player.angle, target);
        // A released turn still rotates once more, so plan the presses
        // around that instead of letting go inside the band and overshooting.
        let (turn, settled) = if self.aiming {
            let turn = match error {
                e if e > FORWARD_DEADBAND => Turn::Right,
                e if e < -FORWARD_DEADBAND => Turn::Left,
                _ => Turn::None,
            };
            (turn, turn == Turn::None)
        } else {
            steer::plan_turn(error, self.release_prev, FORWARD_DEADBAND)
        };
        self.release_now = match turn {
            Turn::Right => i32::from(player::TURN_STEP),
            Turn::Left => -i32::from(player::TURN_STEP),
            Turn::None => 0,
        };
        self.goal = None;
        self.drop_path();
        self.last_forward = false;
        self.running = false;
        NavOutcome {
            forward: false,
            run: false,
            turn,
            status: if settled { "facing" } else { "turning" },
            distance: distance_xz(pos, (x, z)),
            angle_error: error,
        }
    }
}

impl NavOutcome {
    fn apply(self, input: &mut BotInput) {
        match self.turn {
            Turn::Left => {
                input.set("left", true);
            }
            Turn::Right => {
                input.set("right", true);
            }
            Turn::None => {}
        }
        if self.forward {
            input.set("up", true);
            if self.run {
                input.set("run", true);
            }
        }
    }
}

/// The values shared between [`BotRuntime`] and the Lua userdata.
struct BotShares {
    memory: Rc<RefCell<BotMemory>>,
    nav: Rc<RefCell<BotNav>>,
    input: Rc<RefCell<BotInput>>,
    finished: Rc<Cell<bool>>,
    finish_reason: Rc<RefCell<Option<String>>>,
}

impl Default for BotShares {
    fn default() -> Self {
        Self {
            memory: Rc::new(RefCell::new(BotMemory::default())),
            nav: Rc::new(RefCell::new(BotNav::default())),
            input: Rc::new(RefCell::new(BotInput::default())),
            finished: Rc::new(Cell::new(false)),
            finish_reason: Rc::new(RefCell::new(None)),
        }
    }
}

/// The Lua-facing bot handle. Every method is a read of the snapshot or a
/// normal input write; there is deliberately no state-authoring method.
struct BotCtx {
    snapshot: SessionSnapshot,
    /// The current room's authored metadata, cached by [`BotRuntime`].
    meta: Option<Rc<meta::RoomMeta>>,
    tick: u64,
    /// The `--bot-risk` setting.
    risk: f64,
    shares: BotShares,
}

impl BotCtx {
    fn input(&self) -> std::cell::RefMut<'_, BotInput> {
        self.shares.input.borrow_mut()
    }

    fn memory(&self) -> std::cell::RefMut<'_, BotMemory> {
        self.shares.memory.borrow_mut()
    }

    fn nav(&self) -> std::cell::RefMut<'_, BotNav> {
        self.shares.nav.borrow_mut()
    }

    fn log(&self, text: &str) {
        let room = self.snapshot.room.room3();
        self.memory()
            .log(format!("tick {} room {room}: {text}", self.tick));
    }

    fn finish(&self, reason: &str) {
        if !self.shares.finished.get() {
            self.shares.finished.set(true);
            *self.shares.finish_reason.borrow_mut() = Some(reason.to_string());
            self.log(&format!("finished: {reason}"));
        }
    }
}

/// A point argument: `{ x = .., z = .. }` or `{ x, z }`.
fn point_arg(table: &Table) -> mlua::Result<(i32, i32)> {
    let x = match table.get::<Option<f64>>("x")? {
        Some(x) => x,
        None => table.get::<f64>(1)?,
    };
    let z = match table.get::<Option<f64>>("z")? {
        Some(z) => z,
        None => table.get::<f64>(2)?,
    };
    Ok((x as i32, z as i32))
}

fn outcome_table(lua: &Lua, outcome: NavOutcome) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("status", outcome.status)?;
    table.set("distance", outcome.distance)?;
    table.set("angle_error", outcome.angle_error)?;
    Ok(table)
}

impl UserData for BotCtx {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("tick", |_lua, bot, ()| Ok(bot.tick));

        methods.add_method("room", |lua, bot, ()| {
            let id = bot.snapshot.room;
            let table = lua.create_table()?;
            table.set("id", id.room3())?;
            table.set("stage", id.stage)?;
            table.set("room", id.room)?;
            table.set("player_flag", id.player_flag)?;
            table.set("mansion2", id.mansion2)?;
            table.set("camera", bot.snapshot.camera)?;
            table.set("controllable", bot.snapshot.scene.controllable)?;
            Ok(table)
        });

        methods.add_method("room_meta", |lua, bot, ()| {
            let Some(meta) = &bot.meta else {
                return Ok(Value::Nil);
            };
            let table = lua.create_table()?;
            table.set("format", meta.format)?;
            let puzzles = lua.create_table()?;
            for (puzzle_index, puzzle) in meta.puzzles.iter().enumerate() {
                let puzzle_table = lua.create_table()?;
                puzzle_table.set("id", puzzle.id.as_str())?;
                if let Some(title) = &puzzle.title {
                    puzzle_table.set("title", title.as_str())?;
                }
                let steps = lua.create_table()?;
                for (step_index, step) in puzzle.steps.iter().enumerate() {
                    let step_table = lua.create_table()?;
                    step_table.set("slot", step.slot)?;
                    step_table.set("bank", step.bank)?;
                    step_table.set("bit", step.bit)?;
                    step_table.set("label", step.label.as_str())?;
                    let walks = lua.create_table()?;
                    for (walk_index, walk) in step.walks.iter().enumerate() {
                        let point = lua.create_table()?;
                        point.set("x", walk[0])?;
                        point.set("z", walk[1])?;
                        walks.set(walk_index + 1, point)?;
                    }
                    step_table.set("walks", walks)?;
                    if let Some(face) = step.face {
                        let point = lua.create_table()?;
                        point.set("x", face[0])?;
                        point.set("z", face[1])?;
                        step_table.set("face", point)?;
                    }
                    steps.set(step_index + 1, step_table)?;
                }
                puzzle_table.set("steps", steps)?;
                puzzles.set(puzzle_index + 1, puzzle_table)?;
            }
            table.set("puzzles", puzzles)?;
            let interacts = lua.create_table()?;
            for fact in &meta.interacts {
                let row = lua.create_table()?;
                row.set("slot", fact.slot)?;
                row.set("bank", fact.bank)?;
                row.set("bit", fact.bit)?;
                row.set("label", fact.label.as_deref())?;
                interacts.push(row)?;
            }
            table.set("interacts", interacts)?;
            let answers = lua.create_table()?;
            for fact in &meta.answers {
                let row = lua.create_table()?;
                row.set("choice", if fact.yes { "yes" } else { "no" })?;
                row.set("slot", fact.slot)?;
                row.set("bank", fact.flag.map(|(bank, _)| bank))?;
                row.set("bit", fact.flag.map(|(_, bit)| bit))?;
                row.set("label", fact.label.as_deref())?;
                answers.push(row)?;
            }
            table.set("answers", answers)?;
            let avoids = lua.create_table()?;
            for fact in &meta.avoids {
                let row = lua.create_table()?;
                row.set("slot", fact.slot)?;
                row.set("reason", fact.reason.as_str())?;
                avoids.push(row)?;
            }
            table.set("avoids", avoids)?;
            Ok(Value::Table(table))
        });

        methods.add_method("world", |lua, bot, ()| {
            bot.shares.memory.borrow().world.to_lua(lua, &bot.snapshot)
        });

        methods.add_method("risk", |_lua, bot, ()| Ok(bot.risk));

        methods.add_method("player", |lua, bot, ()| {
            let p = &bot.snapshot.player;
            let table = lua.create_table()?;
            table.set("x", p.x)?;
            table.set("y", p.y)?;
            table.set("z", p.z)?;
            table.set("angle", p.angle)?;
            table.set("health", p.health)?;
            table.set("max_health", p.max_health)?;
            table.set("state", p.state)?;
            table.set("being_attacked", p.being_attacked)?;
            table.set("equipped", p.equipped)?;
            table.set("collision_flags", p.collision_flags)?;
            table.set("grabbed", p.grabbed)?;
            Ok(table)
        });

        methods.add_method("enemies", |lua, bot, ()| {
            let table = lua.create_table()?;
            for (index, enemy) in bot.snapshot.enemies.iter().enumerate() {
                let row = lua.create_table()?;
                row.set("slot", enemy.slot)?;
                row.set("id", enemy.id)?;
                row.set("x", enemy.x)?;
                row.set("y", enemy.y)?;
                row.set("z", enemy.z)?;
                row.set("angle", enemy.angle)?;
                row.set("bearing", enemy.bearing)?;
                row.set("health", enemy.health)?;
                row.set("state", enemy.state)?;
                row.set("status_flags", enemy.status_flags)?;
                row.set("action_behavior", enemy.action_behavior)?;
                row.set("action_state", enemy.action_state)?;
                row.set("hit_state", enemy.hit_state)?;
                row.set("distance", enemy.distance)?;
                row.set("visible", enemy.visible)?;
                table.set(index + 1, row)?;
            }
            Ok(table)
        });

        methods.add_method("weapon", |lua, bot, ()| {
            let weapon = bot.snapshot.weapon;
            let table = lua.create_table()?;
            table.set("equipped", weapon.equipped)?;
            table.set("weapon", weapon.weapon)?;
            table.set("clip", weapon.clip)?;
            table.set("reserve", weapon.reserve)?;
            table.set("behavior", weapon.behavior)?;
            table.set("state", weapon.state)?;
            table.set("aim_state", weapon.aim_state)?;
            table.set("target", weapon.target)?;
            Ok(table)
        });

        methods.add_method("scene", |lua, bot, ()| {
            let scene = &bot.snapshot.scene;
            let table = lua.create_table()?;
            table.set("mode", scene.mode)?;
            table.set("skippable", scene.skippable)?;
            table.set("film", scene.film)?;
            table.set("transition", scene.transition)?;
            table.set("cutscene_skip", scene.cutscene_skip)?;
            table.set("message", scene.message)?;
            table.set("message_takes_input", scene.message_takes_input)?;
            table.set("message_waiting", scene.message_waiting)?;
            table.set("pickup", scene.pickup)?;
            table.set("frozen", scene.frozen)?;
            table.set("controls_locked", scene.controls_locked)?;
            table.set("controllable", scene.controllable)?;
            table.set("dead", scene.dead)?;
            Ok(table)
        });

        methods.add_method("actions", |lua, bot, ()| {
            let table = lua.create_table()?;
            for (index, action) in bot.snapshot.actions.iter().enumerate() {
                let row = lua.create_table()?;
                row.set("slot", action.slot)?;
                row.set("kind", action.kind)?;
                row.set("x", action.x)?;
                row.set("z", action.z)?;
                row.set("width", action.width)?;
                row.set("depth", action.depth)?;
                row.set("item", action.item)?;
                row.set("quantity", action.quantity)?;
                row.set("next_room", action.next_room)?;
                row.set("key", action.key)?;
                row.set("lock", action.lock)?;
                row.set("handler", action.handler)?;
                row.set("flags", action.flags)?;
                table.set(index + 1, row)?;
            }
            Ok(table)
        });

        methods.add_method("doors", |lua, bot, ()| {
            let table = lua.create_table()?;
            for (index, door) in bot.snapshot.doors.iter().enumerate() {
                let row = lua.create_table()?;
                row.set("slot", door.slot)?;
                row.set("target", door.target.clone())?;
                row.set("next_room", door.next_room)?;
                row.set("key", door.key)?;
                row.set("lock", door.lock)?;
                row.set("x", door.x)?;
                row.set("z", door.z)?;
                row.set("width", door.width)?;
                row.set("depth", door.depth)?;
                table.set(index + 1, row)?;
            }
            Ok(table)
        });

        methods.add_method("flag", |_lua, bot, (bank, bit): (u8, u8)| {
            Ok(bot.snapshot.flag(bank, bit))
        });

        methods.add_method("has_item", |_lua, bot, item: u8| {
            Ok(bot.snapshot.has_item(item))
        });

        methods.add_method("item_count", |_lua, bot, item: u8| {
            Ok(bot.snapshot.item_count(item))
        });

        methods.add_method("inventory", |lua, bot, ()| {
            let table = lua.create_table()?;
            for (index, stack) in bot.snapshot.inventory.iter().enumerate() {
                let row = lua.create_table()?;
                row.set("slot", index + 1)?;
                row.set("item", stack.id)?;
                row.set("quantity", stack.quantity)?;
                table.set(index + 1, row)?;
            }
            Ok(table)
        });

        methods.add_method("menu", |lua, bot, ()| {
            let menu = &bot.snapshot.menu;
            let table = lua.create_table()?;
            table.set("open", menu.open)?;
            table.set("requested", menu.requested)?;
            table.set("phase", menu.phase)?;
            table.set("accepts_input", menu.accepts_input)?;
            table.set("screen", menu.screen)?;
            table.set("cursor", menu.cursor)?;
            table.set("slot", menu.slot)?;
            table.set("mode", menu.mode)?;
            table.set("option", menu.option)?;
            table.set(
                "options",
                lua.create_sequence_from(menu.options.iter().copied())?,
            )?;
            table.set("selected_item", menu.selected_item)?;
            table.set("ready", menu.ready)?;
            table.set("closing", menu.closing)?;
            table.set("viewing", menu.viewing)?;
            table.set("capacity", menu.capacity)?;
            table.set("equipped", bot.snapshot.player.equipped)?;
            let items = lua.create_table()?;
            for (index, item) in menu.items.iter().enumerate() {
                let row = lua.create_table()?;
                row.set("slot", item.slot)?;
                row.set("item", item.id)?;
                row.set("quantity", item.quantity)?;
                row.set("kind", item.kind)?;
                items.set(index + 1, row)?;
            }
            table.set("items", items)?;
            Ok(table)
        });

        methods.add_method("can_see", |_lua, bot, slot: u8| {
            Ok(bot
                .snapshot
                .enemies
                .iter()
                .find(|enemy| enemy.slot == slot)
                .is_some_and(|enemy| enemy.visible))
        });

        methods.add_method("memory", |lua, bot, ()| {
            let table = lua.create_table()?;
            for (key, value) in &bot.memory().values {
                table.set(key.clone(), value.clone())?;
            }
            Ok(table)
        });

        methods.add_method("visited", |_lua, bot, room: Value| {
            Ok(bot.memory().is_visited(&room))
        });

        methods.add_method("remember", |_lua, bot, (key, value): (String, Value)| {
            let value = match value {
                Value::Nil => None,
                Value::String(text) => Some(text.to_string_lossy()),
                Value::Integer(value) => Some(value.to_string()),
                Value::Number(value) => Some(value.to_string()),
                Value::Boolean(value) => Some(value.to_string()),
                _ => {
                    return Err(mlua::Error::RuntimeError(
                        "remember value must be a string, number or boolean".to_string(),
                    ));
                }
            };
            bot.memory().remember(key, value);
            Ok(())
        });

        methods.add_method("log", |_lua, bot, text: String| {
            bot.log(&text);
            Ok(())
        });

        methods.add_method("finish", |_lua, bot, reason: String| {
            bot.finish(&reason);
            Ok(())
        });

        methods.add_method("set_input", |_lua, bot, table: Table| {
            for pair in table.pairs::<String, bool>() {
                let (name, value) = pair?;
                if !bot.input().set(&name, value) {
                    return Err(mlua::Error::RuntimeError(format!(
                        "unknown bot input key `{name}`"
                    )));
                }
            }
            Ok(())
        });

        methods.add_method("press_action", |_lua, bot, ()| {
            bot.input().press_action();
            Ok(())
        });

        methods.add_method("pulse_confirm", |_lua, bot, ()| {
            bot.input().pulse_confirm();
            Ok(())
        });

        methods.add_method(
            "walk_to",
            |lua, bot, (x, z, options): (i32, i32, Option<Table>)| {
                let mut walk = WalkOptions::default();
                if let Some(options) = options {
                    if options.get::<Option<bool>>("run")?.unwrap_or(false) {
                        walk.run = true;
                    }
                    if options.get::<Option<bool>>("walk")?.unwrap_or(false) {
                        walk.run = false;
                    }
                    if let Some(tolerance) = options.get::<Option<f64>>("tolerance")? {
                        walk.tolerance = tolerance.max(0.0) as i32;
                    }
                    if let Some(face) = options.get::<Option<Table>>("face")? {
                        walk.face = Some(point_arg(&face)?);
                    }
                    if let Some(next) = options.get::<Option<Table>>("next")? {
                        walk.next = Some(point_arg(&next)?);
                    }
                    if options
                        .get::<Option<bool>>("avoid_enemies")?
                        .unwrap_or(false)
                    {
                        walk.avoid_enemies = true;
                    }
                }
                let outcome =
                    bot.nav()
                        .walk_to_with(&bot.snapshot.player, x, z, walk, &bot.snapshot.enemies);
                outcome.apply(&mut bot.input());
                outcome_table(lua, outcome)
            },
        );

        methods.add_method("face_to", |lua, bot, (x, z): (i32, i32)| {
            let outcome = bot.nav().face_to(&bot.snapshot.player, x, z);
            outcome.apply(&mut bot.input());
            outcome_table(lua, outcome)
        });

        methods.add_method(
            "path",
            |lua, bot, (x, z, options): (i32, i32, Option<Table>)| {
                let avoid = match options {
                    Some(options) => options
                        .get::<Option<bool>>("avoid_enemies")?
                        .unwrap_or(false),
                    None => false,
                };
                let table = lua.create_table()?;
                let found = bot.shares.nav.borrow().query_path(
                    &bot.snapshot.player,
                    (x, z),
                    avoid,
                    &bot.snapshot.enemies,
                );
                let Some(found) = found else {
                    table.set("status", "unreachable")?;
                    return Ok(table);
                };
                table.set("status", "ok")?;
                table.set("length", found.length)?;
                table.set("enemy_cost", found.enemy_cost)?;
                let points = lua.create_table()?;
                for (index, (px, pz)) in found.points.iter().enumerate() {
                    let point = lua.create_table()?;
                    point.set("x", *px)?;
                    point.set("z", *pz)?;
                    points.set(index + 1, point)?;
                }
                table.set("points", points)?;
                Ok(table)
            },
        );

        methods.add_method("distance", |_lua, bot, (x, z): (i32, i32)| {
            let p = &bot.snapshot.player;
            Ok(distance_xz((p.x, p.z), (x, z)))
        });
    }
}

/// The sandboxed Lua VM and the `on_bot_tick` function it defines.
struct BotVm {
    lua: Lua,
    budget: Rc<crate::lua::Budget>,
    on_tick: Function,
}

impl BotVm {
    /// Load every `bot/**.lua` entry in lowercased-path order and look up the
    /// required `on_bot_tick` global.
    fn load(pack: &Pack) -> Result<Self> {
        let paths = chunk_paths(pack)?;
        let (lua, budget) = crate::lua::new_sandboxed_state()?;
        run_chunks(&lua, &budget, &paths, pack)?;
        let on_tick = find_on_tick(&lua)?;
        Ok(Self {
            lua,
            budget,
            on_tick,
        })
    }

    /// Re-execute the pack's bot chunks in this VM.
    ///
    /// The Lua state is kept, so globals a route stores outside `on_bot_tick`
    /// survive (`BOT_STATE` in the checked-in route). The chunks re-run in the
    /// same order and the new `on_bot_tick` replaces the old one only after
    /// every chunk succeeds; a failing chunk returns the error and leaves the
    /// previous function callable.
    fn reload(&mut self, pack: &Pack) -> Result<()> {
        let paths = chunk_paths(pack)?;
        run_chunks(&self.lua, &self.budget, &paths, pack)?;
        self.on_tick = find_on_tick(&self.lua)?;
        Ok(())
    }

    fn call(&self, ctx: BotCtx) -> Result<()> {
        self.budget.begin();
        self.lua
            .scope(|scope| {
                let bot = scope.create_userdata(ctx)?;
                self.on_tick.call::<()>(bot)
            })
            .map_err(|err| anyhow!("{err}"))
    }
}

/// The `bot/**.lua` paths to run: lowercased-path order, with the exact-case
/// path kept for reads and error messages.
fn chunk_paths(pack: &Pack) -> Result<Vec<String>> {
    let mut paths = pack
        .entries()
        .map(|entry| entry.path().to_string())
        .filter(|path| {
            path.get(..4)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("bot/"))
                && path.to_ascii_lowercase().ends_with(".lua")
        })
        .collect::<Vec<_>>();
    paths.sort_by(|a, b| {
        a.to_ascii_lowercase()
            .cmp(&b.to_ascii_lowercase())
            .then_with(|| a.cmp(b))
    });
    if paths.is_empty() {
        bail!("no bot/**.lua entries in the pack (pass `--mod mods/bot`)");
    }
    budget::check_len(paths.len(), budget::MAX_LUA_CHUNKS, "bot chunk count")?;
    Ok(paths)
}

/// Run every bot chunk in order, each under its own instruction budget.
fn run_chunks(lua: &Lua, budget: &crate::lua::Budget, paths: &[String], pack: &Pack) -> Result<()> {
    for path in paths {
        let bytes = pack
            .read(path)
            .with_context(|| format!("cannot read bot chunk {path}"))?;
        budget::check_len(
            bytes.len(),
            budget::MAX_LUA_CHUNK,
            &format!("bot chunk {path} size"),
        )?;
        budget.begin();
        lua.load(&*bytes)
            .set_name(path.clone())
            .exec()
            .map_err(|err| anyhow!("failed to run bot chunk {path}: {err}"))?;
    }
    Ok(())
}

/// The required `on_bot_tick` global.
fn find_on_tick(lua: &Lua) -> Result<Function> {
    lua.globals()
        .get::<Function>("on_bot_tick")
        .map_err(|err| anyhow!("bot chunk(s) must define `on_bot_tick(bot)`: {err}"))
}

/// The live bot: its VM, remembered state and navigation cache.
pub struct BotRuntime {
    vm: BotVm,
    shares: BotShares,
    tick: u64,
    /// The current room's parsed metadata; shared with the Lua handle so a
    /// tick clones only the pointer.
    meta: Option<Rc<meta::RoomMeta>>,
    /// The room `meta` was loaded for, so each room loads it once.
    meta_room: Option<RoomId>,
    /// The `--bot-risk` setting; kept across [`Self::reset`].
    risk: f64,
    /// Run statistics for `--bot-stats`.
    stats: stats::BotStats,
}

impl BotRuntime {
    /// Load the pack's bot chunks.
    pub fn load(pack: &Pack) -> Result<Self> {
        Ok(Self {
            vm: BotVm::load(pack)?,
            shares: BotShares::default(),
            tick: 0,
            meta: None,
            meta_room: None,
            risk: DEFAULT_RISK,
            stats: stats::BotStats::default(),
        })
    }

    /// Set the planner's risk setting (`--bot-risk`), clamped to `0..=1`.
    pub fn with_risk(mut self, risk: f64) -> Self {
        self.risk = if risk.is_finite() {
            risk.clamp(0.0, 1.0)
        } else {
            DEFAULT_RISK
        };
        self
    }

    /// Reload the pack's bot chunks in place, preserving the Lua globals, the
    /// remembered memory/navigation and the tick counter.
    ///
    /// The room metadata cache is invalidated too, so a changed `bot.xml` is
    /// reparsed on the next tick.
    pub fn reload(&mut self, pack: &Pack) -> Result<()> {
        self.reload_meta();
        self.vm.reload(pack)
    }

    /// Drop the cached room metadata so the next tick reparses it.
    pub fn reload_meta(&mut self) {
        self.meta = None;
        self.meta_room = None;
    }

    /// Run one `on_bot_tick`, returning the input it asked for.
    ///
    /// The tick counter advances even when the script fails, so the error can
    /// name the exact tick and room.
    pub fn tick(&mut self, session: &GameSession, pack: &Pack) -> Result<BotInput> {
        self.tick = self.tick.saturating_add(1);
        let room_id = session.loaded.id;
        if self.meta_room != Some(room_id) {
            self.meta = meta::load(pack, room_id)?.map(Rc::new);
            self.meta_room = Some(room_id);
        }
        let snapshot = SessionSnapshot::build(session);
        self.stats.observe_snapshot(self.tick, &snapshot);
        {
            let mut input = self.shares.input.borrow_mut();
            input.reset();
            let mut nav = self.shares.nav.borrow_mut();
            nav.sync_room(session.loaded.id, &session.loaded.room);
            nav.begin_tick();
            let mut memory = self.shares.memory.borrow_mut();
            memory.observe(&snapshot);
            memory.world.observe(self.tick, &snapshot);
        }
        let ctx = BotCtx {
            snapshot,
            meta: self.meta.clone(),
            tick: self.tick,
            risk: self.risk,
            shares: BotShares {
                memory: Rc::clone(&self.shares.memory),
                nav: Rc::clone(&self.shares.nav),
                input: Rc::clone(&self.shares.input),
                finished: Rc::clone(&self.shares.finished),
                finish_reason: Rc::clone(&self.shares.finish_reason),
            },
        };
        if let Err(err) = self.vm.call(ctx) {
            self.shares.finished.set(true);
            if self.shares.finish_reason.borrow().is_none() {
                *self.shares.finish_reason.borrow_mut() = Some(format!("Lua error: {err}"));
            }
            return Err(err).with_context(|| {
                format!(
                    "bot tick {} in room {}",
                    self.tick,
                    session.loaded.id.room3()
                )
            });
        }
        let input = *self.shares.input.borrow();
        self.stats.observe_input(&input);
        self.shares
            .memory
            .borrow_mut()
            .world
            .observe_input(self.tick, &input);
        self.shares.nav.borrow_mut().end_tick(&input);
        Ok(input)
    }

    /// Clear the remembered state and tick count for a new game, and rebuild
    /// the Lua VM from the pack so route globals (`BOT_STATE`) cannot leak
    /// across a restart.
    ///
    /// A fresh load failure returns the error. The caller drops the runtime
    /// rather than reusing the old VM's route state.
    pub fn reset(&mut self, pack: &Pack) -> Result<()> {
        self.shares = BotShares::default();
        self.tick = 0;
        self.stats = stats::BotStats::default();
        self.reload_meta();
        self.vm = BotVm::load(pack)?;
        Ok(())
    }

    pub fn finished(&self) -> bool {
        self.shares.finished.get()
    }

    pub fn finish_reason(&self) -> Option<String> {
        self.shares.finish_reason.borrow().clone()
    }

    pub fn ticks(&self) -> u64 {
        self.tick
    }

    /// The run statistics as JSON (see [`stats`]).
    pub fn stats_json(&self, outcome: &str, elapsed_secs: f64) -> serde_json::Value {
        self.stats.to_json(
            outcome,
            self.finish_reason().as_deref(),
            self.tick,
            elapsed_secs,
        )
    }

    pub fn room_path(&self) -> Vec<String> {
        self.shares.memory.borrow().room_path.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::CollisionRect;
    use std::fs;
    use std::path::PathBuf;

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("arklay-bot-{}-{label}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn wall(x_min: u16, x_max: u16, z_min: u16, z_max: u16) -> CollisionRect {
        CollisionRect {
            x_max,
            z_max,
            x_min,
            z_min,
            kind: 1,
            flags: 0,
        }
    }

    #[test]
    fn input_mapping_sets_player_and_ui_levels() {
        let mut input = BotInput::default();
        assert!(input.set("up", true));
        assert!(input.set("run", true));
        assert!(input.set("action", true));
        assert!(input.set("confirm", true));
        assert!(input.input.up);
        assert!(input.ui.up);
        assert!(input.ui.held_up);
        assert!(input.input.run);
        assert!(input.input.action_held);
        assert!(input.action);
        assert!(input.ui.confirm);
        // The UI edge and held level are distinct.
        assert!(input.set("held_up", false));
        assert!(!input.ui.held_up);
        assert!(input.ui.up);
        // An unknown key is a loud script error point, not a silent no-op.
        assert!(!input.set("teleport", true));
    }

    #[test]
    fn press_action_and_pulse_confirm_are_edges() {
        let mut input = BotInput::default();
        input.press_action();
        assert!(input.input.action_held);
        assert!(input.input.action_pressed);
        assert!(input.action);
        input.reset();
        assert!(!input.action);
        input.pulse_confirm();
        assert!(input.ui.confirm);
        assert!(!input.input.action_pressed);
    }

    /// The bot may only press buttons. Every `BotCtx` method is listed here
    /// by what it may touch; a new method fails this test until it is
    /// classified, and nothing may be added to `INPUT` that is not pad/UI
    /// input. The methods are read from the userdata's registered method
    /// table, not from this source file.
    #[test]
    fn bot_ctx_has_no_state_changing_method_but_input() {
        // Reads of the per-tick snapshot or the room metadata.
        const READ: &[&str] = &[
            "tick",
            "room",
            "room_meta",
            "player",
            "enemies",
            "weapon",
            "scene",
            "actions",
            "doors",
            "flag",
            "has_item",
            "item_count",
            "inventory",
            "menu",
            "can_see",
            "memory",
            "visited",
            "distance",
            "path",
            "world",
            "risk",
        ];
        // The bot's own memory and run bookkeeping, never the game.
        const BOT_ONLY: &[&str] = &["remember", "log", "finish"];
        // Pad/UI input for this tick. `walk_to`/`face_to` only compute and
        // set the direction/run buttons.
        const INPUT: &[&str] = &[
            "set_input",
            "press_action",
            "pulse_confirm",
            "walk_to",
            "face_to",
        ];
        // The removed shortcuts into game handlers must never come back.
        const FORBIDDEN: &[&str] = &["use_item", "use_heal", "equip", "use_action", "use_stair"];

        let lua = Lua::new();
        let proxy = lua.create_proxy::<BotCtx>().expect("BotCtx proxy");
        let index: Table = proxy
            .metatable()
            .expect("BotCtx metatable")
            .get("__index")
            .expect("BotCtx methods are an __index table");
        let mut names = Vec::new();
        for pair in index.pairs::<String, Value>() {
            let (name, value) = pair.expect("method entry");
            assert!(
                matches!(value, Value::Function(_)),
                "{name} is not a method"
            );
            names.push(name);
        }
        names.sort();
        assert!(!names.is_empty(), "no BotCtx methods were found");
        for name in &names {
            assert!(
                !FORBIDDEN.contains(&name.as_str()),
                "BotCtx exposes the removed state-changing method `{name}`"
            );
            assert!(
                READ.contains(&name.as_str())
                    || BOT_ONLY.contains(&name.as_str())
                    || INPUT.contains(&name.as_str()),
                "BotCtx method `{name}` is not classified as a read, bot bookkeeping \
                 or pad/UI input; the bot may not change game state any other way"
            );
        }
        for name in READ.iter().chain(BOT_ONLY).chain(INPUT) {
            assert!(
                names.iter().any(|n| n == name),
                "listed method `{name}` is gone"
            );
        }
    }

    #[test]
    fn bot_input_carries_only_pad_and_ui_state() {
        // The struct is the whole host-facing output of a bot tick; any field
        // beyond these three would be a side channel into the game.
        let BotInput { ui, input, action } = BotInput::default();
        assert_eq!(ui, UiInput::default());
        assert_eq!(input, player::Input::default());
        assert!(!action);
    }

    #[test]
    fn menu_options_follow_the_menu_scripts_labels() {
        assert_eq!(menu_options(true), vec!["EQUIP", "CHECK", "MOVE"]);
        assert_eq!(menu_options(false), vec!["USE", "CHECK", "MOVE"]);
    }

    #[test]
    fn enemy_snapshot_filters_dead_and_scripted_entities() {
        let mut game = game::GameState::default();
        let room = RoomState::default();
        let mut spawn = |slot: usize, id: u8, health: i16, x: i32, active: bool| {
            let entity = &mut game.entities[slot];
            entity.id = id;
            entity.health = health;
            entity.pos = [x, 0, 0];
            entity.angle = 0x200;
            entity.set_active(active);
            entity.status_flags |= 0x40;
            entity.state_field = 4;
            entity.action_behavior = 0x12;
            entity.action_state = 3;
            entity.hit_state = 1;
        };
        // Far zombie, near boss, a dead monster, and a scripted character.
        spawn(1, 0x00, 100, 3000, true);
        spawn(2, 0x0C, 400, 1000, true);
        spawn(3, 0x00, 0, 500, true);
        spawn(4, 0x20, 100, 200, true);
        spawn(5, 0x00, 100, 2000, false);

        let enemies = enemy_snapshots(&game, [0, 0, 0], &room);
        assert_eq!(
            enemies.iter().map(|enemy| enemy.slot).collect::<Vec<_>>(),
            [2, 1],
            "nearest living monster first; scripted characters, corpses and \
             inactive slots are excluded"
        );
        assert_eq!(enemies[0].id, 0x0C);
        assert_eq!(enemies[0].distance, 1000);
        assert_eq!(enemies[0].state, 4);
        assert_eq!(enemies[0].status_flags, 0x41);
        assert_eq!(enemies[0].action_behavior, 0x12);
        assert_eq!(enemies[0].action_state, 3);
        assert_eq!(enemies[0].hit_state, 1);
        assert!(enemies[0].visible);
        assert_eq!(enemies[1].health, 100);
    }

    #[test]
    fn weapon_snapshot_reads_the_clip_reserve_and_lock() {
        let mut game = game::GameState::default();
        game.equipped = Some(0x02);
        game.equipped_weapon_id = 0x02;
        game.inventory.push(InventoryItem {
            id: 0x02,
            quantity: 12,
        });
        game.inventory.push(InventoryItem {
            id: 0x0B,
            quantity: 30,
        });
        let state = WeaponState {
            behavior: 0x13,
            state: 2,
            aim_state: 3,
            target: Some(7),
            ..WeaponState::default()
        };
        let weapon = weapon_snapshot(&game, &state);
        assert_eq!(weapon.equipped, Some(0x02));
        assert_eq!(weapon.weapon, 0x02);
        assert_eq!(weapon.clip, 12);
        assert_eq!(weapon.reserve, 30);
        assert_eq!(weapon.behavior, 0x13);
        assert_eq!(weapon.state, 2);
        assert_eq!(weapon.aim_state, 3);
        assert_eq!(weapon.target, Some(7));
    }

    #[test]
    fn memory_remembers_values_and_visited_rooms() {
        let mut memory = BotMemory::default();
        memory.remember("door".to_string(), Some("open".to_string()));
        assert_eq!(memory.values.get("door").map(String::as_str), Some("open"));
        memory.remember("door".to_string(), None);
        assert!(!memory.values.contains_key("door"));

        let id = RoomId {
            stage: 1,
            room: 0x06,
            player_flag: 1,
            mansion2: false,
        };
        memory.visited.insert(id.room3());
        assert!(memory.is_visited_key("106"));
        assert!(memory.is_visited_key("106".to_ascii_lowercase().as_str()));
        assert!(!memory.is_visited_key("105"));
        assert!(memory.is_visited_code(0x106));
        assert!(!memory.is_visited_code(0x105));
    }

    #[test]
    fn grid_path_routes_around_a_wall() {
        let mut room = RoomState::default();
        // A wall spanning most of the room leaves both ends open, so the
        // straight crossing is blocked but the goal is still reachable.
        room.collision.quadrants[0].push(wall(8000, 12000, 4000, 28000));
        let path = find_grid_path(&room, (4000, 16000), (16000, 16000), 0, 100, 0)
            .expect("a path around the wall");
        assert!(path.len() > 2, "the path must detour: {path:?}");
        assert_eq!(path.first().copied(), Some(nearest_grid(4000, 16000)));
        // The tail is the route's exact point, not the 512-unit grid snap.
        assert_eq!(path.last().copied(), Some((16000, 16000)));
        for &(x, z) in &path {
            assert!(
                !blocked(&room, x, z, 0, 100, 0),
                "path hits a wall at {x},{z}"
            );
        }
    }

    #[test]
    fn grid_path_reports_an_enclosed_goal() {
        let mut room = RoomState::default();
        // A sealed box around (16000, 16000).
        room.collision.quadrants[0].push(wall(14000, 18000, 14000, 14200));
        room.collision.quadrants[0].push(wall(14000, 18000, 17800, 18000));
        room.collision.quadrants[0].push(wall(14000, 14200, 14000, 18000));
        room.collision.quadrants[0].push(wall(17800, 18000, 14000, 18000));
        assert!(find_grid_path(&room, (4000, 4000), (16000, 16000), 0, 100, 0).is_none());
    }

    #[test]
    fn walk_turns_before_stepping_forward() {
        let room = RoomState::default();
        let player = PlayerSnapshot {
            x: 16000,
            y: 0,
            z: 16000,
            // Facing -Z (0x400); the target is +X (angle 0), so turn left.
            angle: 0x400,
            health: 96,
            max_health: 96,
            state: 1,
            being_attacked: 0,
            equipped: None,
            radius: 100,
            collision_flags: 0,
            grabbed: false,
        };
        let mut nav = BotNav::default();
        nav.sync_room(
            RoomId {
                stage: 1,
                room: 0x06,
                player_flag: 1,
                mansion2: false,
            },
            &room,
        );
        let outcome = nav.walk_to(&player, 18000, 16000, WalkOptions::default());
        assert_eq!(outcome.status, "moving");
        assert!(!outcome.forward);
        assert_eq!(outcome.turn, Turn::Left);
        assert!(outcome.angle_error < 0);
    }

    fn player_at(x: i32, z: i32, angle: u16) -> PlayerSnapshot {
        PlayerSnapshot {
            x,
            y: 0,
            z,
            angle,
            health: 96,
            max_health: 96,
            state: 1,
            being_attacked: 0,
            equipped: None,
            radius: 100,
            collision_flags: 0,
            grabbed: false,
        }
    }

    #[test]
    fn planning_keeps_the_resolver_skin_from_walls() {
        let mut room = RoomState::default();
        // A wall with its south face at z = 4000.
        room.collision.quadrants[0].push(wall(0, 20000, 4000, 5000));
        // 110 units south of the face: clear for the bare 100-unit radius,
        // inside the resolver's 18-unit skin, so the plan must reject it.
        assert!(!player::position_blocked(&room, [10000, 0, 3890], 100, 0));
        assert!(blocked(&room, 10000, 3890, 0, 100, 0));
        // 130 units clear is fine for both.
        assert!(!blocked(&room, 10000, 3870, 0, 100, 0));
    }

    #[test]
    fn grid_path_ends_at_the_exact_route_point() {
        let room = RoomState::default();
        let goal = (4300, 4800);
        let path = find_grid_path(&room, (4000, 4000), goal, 0, 100, 0)
            .expect("a path across an empty room");
        assert_eq!(
            path.last().copied(),
            Some(goal),
            "the path must not stop at the 512-unit grid snap: {path:?}"
        );
    }

    #[test]
    fn walk_replans_after_a_scripted_move() {
        let room = RoomState::default();
        let goal = (12000, 4000);
        let mut nav = BotNav::default();
        nav.sync_room(
            RoomId {
                stage: 1,
                room: 0x06,
                player_flag: 1,
                mansion2: false,
            },
            &room,
        );
        // A goal the old position reported as blocked: a scripted move can
        // make it reachable, so the stale status must not stick.
        nav.goal = Some(goal);
        nav.tolerance = DEFAULT_TOLERANCE;
        nav.blocked_goal = Some(goal);
        nav.last_pos = Some((4000, 4000));
        let player = player_at(8000, 8000, 0);
        let outcome = nav.walk_to(
            &player,
            goal.0,
            goal.1,
            WalkOptions {
                run: true,
                tolerance: DEFAULT_TOLERANCE,
                ..WalkOptions::default()
            },
        );
        assert_eq!(outcome.status, "moving");
        assert_eq!(nav.blocked_goal, None);
        assert_eq!(nav.path.last().copied(), Some(goal));
    }

    #[test]
    fn walk_looks_past_near_waypoints_it_can_see() {
        let room = RoomState::default();
        let goal = (9000, 4000);
        let mut nav = BotNav::default();
        nav.sync_room(
            RoomId {
                stage: 1,
                room: 0x06,
                player_flag: 1,
                mansion2: false,
            },
            &room,
        );
        let player = player_at(4000, 4000, 0);
        // Establish the goal, then hand the nav a path with three points
        // inside the lookahead. The farthest visible one must win.
        nav.walk_to(
            &player,
            goal.0,
            goal.1,
            WalkOptions {
                run: true,
                tolerance: DEFAULT_TOLERANCE,
                ..WalkOptions::default()
            },
        );
        nav.path = vec![(4400, 4000), (5000, 4000), (5500, 4000), goal];
        nav.index = 0;
        nav.walk_to(
            &player,
            goal.0,
            goal.1,
            WalkOptions {
                run: true,
                tolerance: DEFAULT_TOLERANCE,
                ..WalkOptions::default()
            },
        );
        assert_eq!(nav.index, 2, "target the farthest point inside 1600 units");
    }

    #[test]
    fn walk_reports_arrived_within_tolerance() {
        let room = RoomState::default();
        let player = PlayerSnapshot {
            x: 16000,
            y: 0,
            z: 16000,
            angle: 0,
            health: 96,
            max_health: 96,
            state: 1,
            being_attacked: 0,
            equipped: None,
            radius: 100,
            collision_flags: 0,
            grabbed: false,
        };
        let mut nav = BotNav::default();
        nav.sync_room(
            RoomId {
                stage: 1,
                room: 0x06,
                player_flag: 1,
                mansion2: false,
            },
            &room,
        );
        let outcome = nav.walk_to(&player, 16200, 16100, WalkOptions::default());
        assert_eq!(outcome.status, "arrived");
        assert!(!outcome.forward);
    }

    fn test_nav(room: &RoomState) -> BotNav {
        let mut nav = BotNav::default();
        nav.sync_room(
            RoomId {
                stage: 1,
                room: 0x06,
                player_flag: 1,
                mansion2: false,
            },
            room,
        );
        nav
    }

    /// What one drive of [`drive`] did.
    #[derive(Debug)]
    struct Drive {
        status: &'static str,
        turn_in_place: u32,
    }

    /// Feed `walk_to` outcomes into a kinematic model of the player's tank
    /// controls (`player.rs`: the walk steers 0x28 a tick at 0x5D, the run
    /// 0x30 at 0xD2, an in-place turn is 0x60 and does not move) until the
    /// walk stops reporting `moving`. The room must be open: the model does
    /// not resolve collision.
    fn drive(
        nav: &mut BotNav,
        player: &mut PlayerSnapshot,
        goal: (i32, i32),
        opts: WalkOptions,
        enemies: &[EnemySnapshot],
    ) -> Drive {
        let mut turn_in_place = 0;
        for _ in 0..2000 {
            let outcome = nav.walk_to_with(player, goal.0, goal.1, opts, enemies);
            if outcome.status != "moving" {
                return Drive {
                    status: outcome.status,
                    turn_in_place,
                };
            }
            let sign = match outcome.turn {
                Turn::Right => 1i32,
                Turn::Left => -1,
                Turn::None => 0,
            };
            let (speed, step) = if !outcome.forward {
                turn_in_place += u32::from(sign != 0);
                (0, player::TURN_STEP)
            } else if outcome.run {
                (player::RUN_SPEED, player::RUN_TURN)
            } else {
                (player::WALK_SPEED, player::DIAGONAL_TURN)
            };
            player.angle =
                (i32::from(player.angle) + sign * i32::from(step)).rem_euclid(0x1000) as u16;
            let (dx, dz) = steer::heading_dir(player.angle);
            player.x += (dx * f64::from(speed)).round() as i32;
            player.z += (dz * f64::from(speed)).round() as i32;
        }
        panic!("the walk to {goal:?} never finished: {player:?}");
    }

    #[test]
    fn walk_steers_through_a_turn_instead_of_stopping() {
        let room = RoomState::default();
        let mut nav = test_nav(&room);
        // Facing +X; the goal is 60 degrees off to the +Z side, 6000 away.
        let mut player = player_at(8000, 8000, 0);
        let result = drive(
            &mut nav,
            &mut player,
            (11000, 13196),
            WalkOptions::default(),
            &[],
        );
        assert_eq!(result.status, "arrived");
        assert_eq!(result.turn_in_place, 0, "{result:?}");
        // A reversal still turns in place first.
        let mut nav = test_nav(&room);
        let mut player = player_at(8000, 8000, 0);
        let result = drive(
            &mut nav,
            &mut player,
            (3000, 8000),
            WalkOptions::default(),
            &[],
        );
        assert_eq!(result.status, "arrived");
        assert!(result.turn_in_place > 0, "{result:?}");
    }

    #[test]
    fn walk_with_a_face_point_arrives_facing_the_zone() {
        let room = RoomState::default();
        // Approach a zone north (+Z) of the goal from the south-west.
        let goal = (12000, 12000);
        let face = (12000, 14000);
        let face_error = |player: &PlayerSnapshot| {
            let angle = crate::sfx::angle_between_xz(player.x, player.z, face.0, face.1);
            angle_error(player.angle, angle).abs()
        };
        let opts = WalkOptions {
            run: true,
            tolerance: 120,
            ..WalkOptions::default()
        };

        let mut nav = test_nav(&room);
        let mut plain = player_at(6000, 7000, 0);
        let result = drive(&mut nav, &mut plain, goal, opts, &[]);
        assert_eq!(result.status, "arrived");
        assert!(
            face_error(&plain) > FORWARD_DEADBAND,
            "without the option the walk arrives off the face: {plain:?}"
        );

        let mut nav = test_nav(&room);
        let mut faced = player_at(6000, 7000, 0);
        let opts = WalkOptions {
            face: Some(face),
            ..opts
        };
        let result = drive(&mut nav, &mut faced, goal, opts, &[]);
        assert_eq!(result.status, "arrived");
        assert!(
            face_error(&faced) <= FORWARD_DEADBAND,
            "the face option arrives facing the zone, so face_to needs no turn: {faced:?}"
        );
        assert!(distance_xz((faced.x, faced.z), goal) <= 120);
        assert_eq!(nav.face_to(&faced, face.0, face.1).status, "facing");
    }

    fn enemy_at(x: i32, z: i32, angle: u16) -> EnemySnapshot {
        EnemySnapshot {
            slot: 1,
            id: 1,
            x,
            y: 0,
            z,
            angle,
            bearing: 0,
            health: 100,
            state: 1,
            status_flags: 0,
            action_behavior: 0,
            action_state: 0,
            hit_state: 0,
            distance: 0,
            visible: true,
        }
    }

    #[test]
    fn avoid_enemies_path_passes_behind_a_monster() {
        let room = RoomState::default();
        let nav = test_nav(&room);
        let player = player_at(6000, 16000, 0);
        let goal = (26000, 16000);
        // A monster on the straight line, facing +Z.
        let enemies = [enemy_at(16000, 16000, 0xC00)];
        let straight = nav
            .query_path(&player, goal, false, &enemies)
            .expect("a straight path");
        assert_eq!(straight.points, vec![goal]);
        let around = nav
            .query_path(&player, goal, true, &enemies)
            .expect("a path around the monster");
        assert!(
            around.enemy_cost < straight.enemy_cost,
            "{around:?} vs {straight:?}"
        );
        assert!(around.length > straight.length);
        // It passes behind the monster (-Z), and every sample is outside
        // the core reach.
        let mut from = (player.x, player.z);
        let mut min_z = i32::MAX;
        let mut max_z = i32::MIN;
        for &to in &around.points {
            for step in 0..=32 {
                let x = from.0 + (to.0 - from.0) * step / 32;
                let z = from.1 + (to.1 - from.1) * step / 32;
                assert!(
                    distance_xz((x, z), (16000, 16000)) as f64 > steer::AVOID_CORE,
                    "the path enters the core at {x},{z}: {around:?}"
                );
                min_z = min_z.min(z);
                max_z = max_z.max(z);
            }
            from = to;
        }
        assert!(
            16000 - min_z > max_z - 16000,
            "the detour goes behind: {around:?}"
        );
        // Facing -Z instead, the same query goes around the +Z side.
        let flipped = nav
            .query_path(&player, goal, true, &[enemy_at(16000, 16000, 0x400)])
            .expect("a path around the flipped monster");
        let max_z = flipped.points.iter().map(|p| p.1).max().unwrap();
        let min_z = flipped.points.iter().map(|p| p.1).min().unwrap();
        assert!(max_z - 16000 > 16000 - min_z, "{flipped:?}");
    }

    #[test]
    fn walk_avoids_enemies_only_when_asked() {
        let room = RoomState::default();
        let enemies = [enemy_at(16000, 16000, 0xC00)];
        let goal = (26000, 16000);
        let mut nav = test_nav(&room);
        let mut player = player_at(6000, 16000, 0);
        nav.walk_to_with(&player, goal.0, goal.1, WalkOptions::default(), &enemies);
        assert_eq!(nav.path, vec![goal], "the default walk ignores the monster");
        let opts = WalkOptions {
            avoid_enemies: true,
            ..WalkOptions::default()
        };
        let result = drive(&mut nav, &mut player, goal, opts, &enemies);
        assert_eq!(result.status, "arrived");
    }

    #[test]
    fn bot_runtime_reset_reloads_globals_and_clears_progress() {
        let temp = TempDir::new("reset");
        let bot = temp.path.join("bot");
        fs::create_dir_all(&bot).unwrap();
        let file = bot.join("init.lua");
        fs::write(&file, b"VERSION = 1\nfunction on_bot_tick(bot) end\n").unwrap();

        let pack = Pack::open(&temp.path).unwrap();
        let mut runtime = BotRuntime::load(&pack).unwrap();
        // Simulate a finished route with progress in both the VM and shares.
        runtime.tick = 42;
        runtime.shares.finished.set(true);
        runtime.vm.lua.globals().set("VERSION", 99).unwrap();

        runtime.reset(&pack).unwrap();
        assert_eq!(runtime.ticks(), 0);
        assert!(!runtime.finished());
        assert_eq!(runtime.vm.lua.globals().get::<i64>("VERSION").unwrap(), 1);
    }

    #[test]
    fn room_meta_facts_world_and_risk_reach_lua() {
        let temp = TempDir::new("facts");
        let bot = temp.path.join("bot");
        fs::create_dir_all(&bot).unwrap();
        fs::write(
            bot.join("init.lua"),
            br#"function on_bot_tick(bot)
                local m = bot:room_meta()
                local i, a, v = m.interacts[1], m.answers[1], m.avoids[1]
                local w = bot:world()
                RESULT = string.format("%s|%d|%d|%d|%d|%s|%s|%s|%s|%d|%s|%s|%d|%g",
                    m.format, #m.puzzles, i.slot, i.bank, i.bit, i.label,
                    a.choice, tostring(a.slot), tostring(a.bank), v.slot, v.reason,
                    w.room, #w.frontier, bot:risk())
            end
"#,
        )
        .unwrap();
        let pack = Pack::open(&temp.path).unwrap();
        let vm = BotVm::load(&pack).unwrap();
        let meta = meta::parse(
            r#"<bot format="2">
                <interact slot="4" bank="1" bit="0x22" label="statue"/>
                <answer choice="no" slot="6"/>
                <avoid slot="7" reason="plant"/>
            </bot>"#,
        )
        .unwrap();
        let mut snapshot = world::tests::snapshot(0x117, 0, 0);
        snapshot.doors.push(DoorSnapshot {
            slot: 1,
            target: Some("118".to_string()),
            next_room: 0x18,
            key: 0,
            lock: 0,
            x: 0,
            z: 0,
            width: 100,
            depth: 100,
        });
        let shares = BotShares::default();
        shares.memory.borrow_mut().world.observe(1, &snapshot);
        vm.call(BotCtx {
            snapshot,
            meta: Some(Rc::new(meta)),
            tick: 1,
            risk: 0.6,
            shares,
        })
        .unwrap();
        assert_eq!(
            vm.lua.globals().get::<String>("RESULT").unwrap(),
            "2|0|4|1|34|statue|no|6|nil|7|plant|117|1|0.6"
        );
    }

    #[test]
    fn bot_runtime_keeps_a_clamped_risk_across_reset() {
        let temp = TempDir::new("risk");
        let bot = temp.path.join("bot");
        fs::create_dir_all(&bot).unwrap();
        fs::write(bot.join("init.lua"), b"function on_bot_tick(bot) end\n").unwrap();
        let pack = Pack::open(&temp.path).unwrap();
        let mut runtime = BotRuntime::load(&pack).unwrap();
        assert_eq!(runtime.risk, DEFAULT_RISK);
        runtime = runtime.with_risk(0.75);
        runtime.reset(&pack).unwrap();
        assert_eq!(runtime.risk, 0.75);
        assert_eq!(BotRuntime::load(&pack).unwrap().with_risk(3.0).risk, 1.0);
        assert_eq!(
            BotRuntime::load(&pack).unwrap().with_risk(f64::NAN).risk,
            DEFAULT_RISK
        );
    }

    #[test]
    fn bot_vm_reload_keeps_globals_and_refetches_on_tick() {
        let temp = TempDir::new("reload");
        let bot = temp.path.join("bot");
        fs::create_dir_all(&bot).unwrap();
        let file = bot.join("init.lua");
        fs::write(&file, b"VERSION = 1\nfunction on_bot_tick(bot) end\n").unwrap();

        let pack = Pack::open(&temp.path).unwrap();
        let mut vm = BotVm::load(&pack).unwrap();
        assert_eq!(vm.lua.globals().get::<i64>("VERSION").unwrap(), 1);

        // Re-executing the chunk in the same state replaces the function and
        // keeps a route's global state (`BOT_STATE` in the checked-in route).
        fs::write(&file, b"VERSION = 2\nfunction on_bot_tick(bot) end\n").unwrap();
        vm.reload(&pack).unwrap();
        assert_eq!(vm.lua.globals().get::<i64>("VERSION").unwrap(), 2);
        assert!(vm.lua.globals().get::<Function>("on_bot_tick").is_ok());
    }
}
