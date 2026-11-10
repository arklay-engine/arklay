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

mod knowledge;
#[cfg(test)]
mod lua_lint;
mod meta;
mod params;

pub use params::parse_assignment as parse_bot_param;
mod stats;
pub(crate) use stats::RunFacts;
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
use crate::ui::save_load::SaveScreenView;
use crate::weapons::WeaponState;

/// The default headless tick budget (about 55 minutes at 30 Hz).
pub const DEFAULT_MAX_TICKS: u64 = 100_000;

/// The flag banks whose changes the run log records (`story: ...`): bank 0
/// (scenario) and bank 1 (common), where the story's scenes and branches
/// leave their marks.
const STORY_BANKS: usize = 2;
const STORY_BANK_NAMES: [&str; STORY_BANKS] = ["scenario", "common"];

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
    /// The room whose entry ends the run (`--bot-goal-room`), such as
    /// `"20D"`; read-only to Lua as `bot:goal_room()`.
    pub goal_room: Option<String>,
    /// The game-RNG seed (`--bot-seed`); `None` leaves the game's own random
    /// stream untouched.
    pub seed: Option<u32>,
    /// The story parameters picked for this run (`--bot-param NAME=VALUE`),
    /// checked against the pack's `params/<game>.xml` when the bot attaches.
    pub params: Vec<(String, String)>,
    /// The debug INVINCIBLE cheat for every session of the run
    /// (`--bot-invincible`): health stays full and the game-over machine
    /// never runs. The bot is not told; it plays as it would.
    pub invincible: bool,
}

/// The game RNG state for the `session`th session of a `--bot-seed` run
/// (0 for the first, then one more per bot reload). A pure function of
/// both, so a seeded run replays exactly; different seeds, and a reload,
/// land on different streams. Never 1, the unseeded start state.
pub fn seeded_rand_state(seed: u32, session: u32) -> u32 {
    let mut x = (u64::from(seed) << 32 | u64::from(session)).wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    let state = (x ^ (x >> 32)) as u32;
    if state == 1 { 2 } else { state }
}

impl Default for BotOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            headless: false,
            max_ticks: 0,
            stats_path: None,
            risk: DEFAULT_RISK,
            goal_room: None,
            seed: None,
            params: Vec::new(),
            invincible: false,
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
#[derive(Clone, Debug, Default)]
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
        if LOG.load(std::sync::atomic::Ordering::Relaxed) {
            eprintln!("[bot] {line}");
        }
    }

    /// Carry the values a reload keeps over from `current` (the memory at
    /// the death): the bosses met (`fight_met:` keys). A player who died to
    /// a boss knows it is there, though the save was written before it.
    fn keep_lessons(&mut self, current: &BotMemory) {
        for (key, value) in &current.values {
            if key.starts_with(LESSON_PREFIX) {
                self.values.insert(key.clone(), value.clone());
            }
        }
    }
}

/// Whether the bot's log lines (`[bot] tick ...`) are written to stderr.
static LOG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// Write the bot's log lines to stderr (the default), or drop them: a
/// driver script's `log = false`.
pub(crate) fn set_log(on: bool) {
    LOG.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// `BOT_BOOT_IDLE`: ticks the bot idles at the start of the run (0 unset).
fn boot_idle_ticks() -> u64 {
    static TICKS: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *TICKS.get_or_init(|| {
        std::env::var("BOT_BOOT_IDLE")
            .ok()
            .and_then(|text| text.parse().ok())
            .unwrap_or(0)
    })
}

/// The memory keys a reload keeps ([`BotMemory::keep_lessons`]).
const LESSON_PREFIX: &str = "fight_met:";

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
    /// The room's state is the room's own. Between a door load's room-id
    /// switch and the destination's `room_set` (a stage change yields a
    /// frame there) the doors and actions are still the source room's, so
    /// the snapshot leaves them empty and the world model skips the tick.
    booted: bool,
    camera: usize,
    player: PlayerSnapshot,
    scene: SceneSnapshot,
    actions: Vec<ActionSnapshot>,
    doors: Vec<DoorSnapshot>,
    /// The room's active objects.
    objects: Vec<ObjectSnapshot>,
    enemies: Vec<EnemySnapshot>,
    /// The acid globs in flight (effect behaviours 22 and 55, the drop and
    /// splash tests that hurt the player): their world positions.
    projectiles: Vec<[i32; 3]>,
    weapon: WeaponSnapshot,
    /// The pause menu as the engine shows it, read-only.
    menu: MenuSnapshot,
    flags: [FlagBank; game::FLAG_BANK_COUNT],
    inventory: Vec<InventoryItem>,
    /// The room's camera switch zones (group headers left out).
    cameras: Vec<CameraZoneSnapshot>,
    /// The typewriter save screen as a player sees it, while it is up.
    save_screen: Option<SaveScreenView>,
    /// This session's completed saves and the ink ribbon rule.
    saves: SaveSnapshot,
    /// The item catalog, for each item's kind and a weapon's ammunition.
    item_defs: std::sync::Arc<crate::item_catalog::ItemCatalog>,
    /// The player stands in the guardhouse drug store, where chemicals mix.
    drug_store: bool,
    /// The lab computer terminal, while it is up.
    computer: Option<ComputerSnapshot>,
    /// Room 408's numeric panel, while it runs.
    panel: Option<PanelSnapshot>,
}

/// Room 408's numeric panel (`super::passcode_panel`) as the screen shows
/// it, read-only.
#[derive(Clone, Copy, Debug, Default)]
struct PanelSnapshot {
    /// Whether the pad takes input (the prompt was answered, not yet solved).
    active: bool,
    /// The cursor's column and row, 0..=2.
    cursor: (u8, u8),
    /// The nine lamps, row-major.
    lamps: [bool; 9],
}

/// The lab computer terminal's state machine as its screen keeps it
/// (`games/re1/interactive/computer_lab.lua`'s state table), read-only.
#[derive(Clone, Debug, Default)]
struct ComputerSnapshot {
    /// What the terminal waits for: `"login"`, `"menu"`, `"exit"` (the
    /// menu's leave question), `"command"`, `"unlocked"`, or `"busy"` while
    /// it takes no input.
    stage: &'static str,
    /// The field being typed: `"user"` or `"secret"` (empty otherwise).
    sub: &'static str,
    /// The keyboard cursor cell (row * 8 + column).
    cell: i64,
    /// The door menu's highlighted row: 0 and 1 the cell doors, 2 exit.
    menu_row: i64,
    /// How many chars the field being typed holds.
    typed: usize,
}

/// The lab terminal's state while it is up (scenario flag 0x20 with the
/// system bit that selects it), read from the screen script's state table.
///
/// The stages follow `computer_lab.lua`'s `func`/`sub`/`sub2`/`sub3` block:
/// `sub` 1 is the login (its steps 1 and 3 type the user and the password),
/// 2 the door menu (step 1 picks a row, step 3 asks whether to leave), 3 the
/// command prompt and 4 the unlocked screen, which a press dismisses.
fn computer_snapshot(game: &game::GameState) -> Option<ComputerSnapshot> {
    use crate::interactive::ScriptValue;
    let (flag, entry) = crate::interactive::SCREENS[1];
    if !game.flag_test(game::BANK_SCENARIO, 0x20, false)
        || !game.flag_test(game::BANK_SYSTEM, flag, false)
    {
        return None;
    }
    let state = game.interactive.state(entry)?;
    let int = |key: &str| state.int(key).unwrap_or(0);
    let window_busy = |index: i64| {
        state
            .field("win")
            .and_then(|windows| windows.index(index))
            .and_then(|window| window.int("anim_cmd"))
            .unwrap_or(0)
            != 0
    };
    let typed = |record: i64| {
        state
            .field("text")
            .and_then(|records| records.index(record))
            .and_then(|record| record.field("text"))
            .map_or(0, |text| {
                (0..9)
                    .take_while(
                        |&at| matches!(text.index(at), Some(ScriptValue::Int(byte)) if *byte != 0),
                    )
                    .count()
            })
    };
    let (func, sub, sub2, sub3) = (int("func"), int("sub"), int("sub2"), int("sub3"));
    let input = int("input_enabled") != 0;
    let (stage, field, typed) = match (func, sub, sub2, sub3) {
        (1, 1, 1, 1) => ("login", "user", typed(0)),
        (1, 1, 1, 3) => ("login", "secret", typed(1)),
        (1, 2, 1.., 1) if input => ("menu", "", 0),
        (1, 2, 1.., 3) => ("exit", "", 0),
        (1, 3, 1, 0) if !window_busy(2) && !window_busy(3) => ("command", "user", typed(0)),
        (1, 4.., 2.., _) => ("unlocked", "", 0),
        _ => ("busy", "", 0),
    };
    Some(ComputerSnapshot {
        stage,
        sub: field,
        cell: int("row") * 8 + int("col"),
        menu_row: int("caret_pos"),
        typed,
    })
}

/// The session's save facts: what the save screen reported writing, and
/// whether a save needs an ink ribbon. Read-only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SaveSnapshot {
    /// Saves this session's typewriter screens wrote.
    written: u32,
    /// The slot of the last one (0-based).
    slot: Option<usize>,
    /// A typewriter save needs, and spends, an ink ribbon (the character's
    /// ink ribbon option).
    needs_ribbon: bool,
    /// Ink ribbons held.
    ribbons: u32,
}

/// One camera switch zone: standing in it while the camera is `from` cuts
/// to `to`. `x`/`z` is the mean of its four corners.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CameraZoneSnapshot {
    from: usize,
    to: usize,
    x: i32,
    z: i32,
}

/// The switch zones of `room`: every record after its group's header whose
/// cuts are both in range, as `camera_for_position` reads them.
fn camera_zone_snapshots(room: &RoomState) -> Vec<CameraZoneSnapshot> {
    let mut zones = Vec::new();
    let mut group = None;
    for zone in &room.zones {
        if zone.cam_from < 0 {
            group = None;
            continue;
        }
        if group != Some(zone.cam_from) {
            group = Some(zone.cam_from);
            continue;
        }
        if zone.cam_to < 0 {
            continue;
        }
        let (from, to) = (zone.cam_from as usize, zone.cam_to as usize);
        if from >= room.cuts.len() || to >= room.cuts.len() {
            continue;
        }
        let corner = |axis: usize| -> i32 {
            zone.corners
                .iter()
                .map(|c| i32::from(c[axis] as u16))
                .sum::<i32>()
                / 4
        };
        zones.push(CameraZoneSnapshot {
            from,
            to,
            x: corner(0),
            z: corner(1),
        });
    }
    zones
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
    /// Whether a locked player action (a vault, stairs, a ladder, a reach)
    /// is running. A finished ladder leaves `being_attacked`'s sequence bit
    /// up, so the bit alone does not mean a sequence is in progress.
    locked: bool,
    equipped: Option<u8>,
    radius: i32,
    collision_flags: u8,
    /// The entity's grabbed zone flag (`0x80`). Combat stands down while a
    /// grab owns the player.
    grabbed: bool,
    /// `GameState::health_status`: [`POISON`] and [`SERIOUS_POISON`].
    health_status: u8,
    /// The player entity's animation clip. The Black Tiger's acid hits only
    /// clip 1, the standing idle (`effects::behaviour`'s splash timer).
    animation: u8,
}

/// The health-status bit a blue herb clears (`cure_poison`): the drain that
/// takes 2 health every 120 ticks.
const POISON: u8 = 0x02;
/// The health-status bit the serum clears (`cure_serious_poison`).
const SERIOUS_POISON: u8 = 0x20;

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
    /// The window shows a yes/no prompt.
    prompt: bool,
    /// The prompt's highlighted answer: `true` for Yes.
    prompt_yes: bool,
    pickup: bool,
    /// The pick-up opened the document reader, which pages on the D-pad and
    /// closes on cancel.
    reader: bool,
    frozen: bool,
    controls_locked: bool,
    controllable: bool,
    dead: bool,
    /// The scripted death variant is up: the escape scene's way of ending
    /// the game, which chains the ending rather than the DIED screen.
    ending: bool,
    /// The typewriter's save hand-off (the fade into or out of the save
    /// screen) is running.
    typewriter: bool,
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
    /// A drawer (`Desk`, handler 0x0E): what `check_desk` reads.
    desk: Option<DeskSnapshot>,
}

/// A drawer's parameter words as `GameState::check_desk` reads them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DeskSnapshot {
    /// Word 0: the bank-2 (LocksFlags) bit; the drawer opens without a key
    /// once it is set.
    lock_bit: u8,
    /// Word 1: the room action slot of the item the drawer holds.
    item_slot: u8,
    /// That item action's id and quantity, when the slot holds an item
    /// action (a drawer item may lack the player probe bit, so the item row
    /// itself can be dormant and hide them).
    item: Option<u8>,
    quantity: Option<u8>,
    /// The item is still in the drawer: its bank-7 room-items bit is set.
    full: bool,
}

/// The drawer fields of the room action `action`, or `None` for any other
/// kind.
fn desk_snapshot(game: &game::GameState, action: &game::RoomAction) -> Option<DeskSnapshot> {
    if action.kind != RoomActionKind::Desk {
        return None;
    }
    let item_slot = action.param_word(1) as u8;
    let item = game
        .room_actions
        .get(usize::from(item_slot))
        .copied()
        .flatten()
        .filter(|item| item.kind == RoomActionKind::Item);
    Some(DeskSnapshot {
        lock_bit: action.param_word(0) as u8,
        item_slot,
        item: item.map(|item| item.item_id()),
        quantity: item.map(|item| item.item_quantity()),
        full: item.is_some_and(|item| game.room_item_present(item.room_items_flag)),
    })
}

/// One active room object (a pushable block, a statue, a shelf), read-only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ObjectSnapshot {
    slot: usize,
    /// The live position (the record's `pos`).
    x: i32,
    z: i32,
    /// The collision half-extents along X and Z.
    half_x: i32,
    half_z: i32,
    /// Solid and without `OF_NOT_PUSHABLE`: walking into it can push it.
    pushable: bool,
    /// Neither intangible nor without collision: the player cannot walk
    /// through it, so the navigation plans around it.
    solid: bool,
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
    /// The catalog says a USE of it restores health (a red herb alone is a
    /// `"heal"` that restores none and the menu refuses it).
    heals: bool,
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
    /// The MOVE (combine) cursor byte while `mode` is `"move"`.
    move_cursor: Option<i64>,
    /// The item box screen, while it is up over the panel.
    item_box: Option<ItemBoxView>,
    /// The CHECK viewer (`ui/item_view.lua`), while it is up.
    examine: Option<ExamineView>,
}

/// The CHECK viewer as the shipped script publishes it with `api:set`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ExamineView {
    /// The item on the turntable (a doom book becomes its medal here).
    item: u8,
    /// The turntable's yaw, `0..0x1000`; left raises it, right lowers it.
    yaw: i64,
    /// The turntable takes the d-pad and confirm (`display`/`turn`).
    ready: bool,
}

impl ExamineView {
    fn build(session: &GameSession) -> Option<Self> {
        let viewer = session.examine.as_deref()?;
        Some(Self {
            item: viewer.value("item") as u8,
            yaw: viewer.value("yaw"),
            ready: viewer.value("ready") != 0,
        })
    }
}

/// The item box screen as a player sees it, read from the shipped script's
/// `M.state` (`ui/item_box.lua`) and the box's slots, without running any of
/// its code.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ItemBoxView {
    /// `"browse"` (the inventory cursor moves) or `"armed"` (a confirmed
    /// inventory slot waits for the box slot to swap with).
    mode: String,
    /// The box slot shown in the preview (0-based).
    box_cursor: i64,
    /// The inventory cursor byte (`8 + 2 * slot`; `4`/`6` the EXIT tab).
    cursor: i64,
    /// A page slide is running; the screen reads no buttons until it ends.
    sliding: bool,
    /// The box's slots in order: item id (`0` empty) and quantity.
    slots: Vec<(u8, u8)>,
}

impl ItemBoxView {
    fn build(session: &GameSession) -> Option<Self> {
        let screen = session.item_box.as_deref()?;
        Some(Self {
            mode: screen.state_str("mode")?,
            box_cursor: screen.state_int("box_cursor")?,
            cursor: screen.state_int("player_cursor")?,
            sliding: screen.state_int("slide_dir")? != 0,
            slots: session
                .game
                .item_box
                .iter()
                .map(|stack| (stack.id, stack.quantity))
                .collect(),
        })
    }
}

/// A `<use>` row's done sense and its `after`/`until` gates, as the
/// interact rows spell them (`done_clear`, `after_bank`/`after_bit`) plus
/// `until_bank`/`until_bit`.
fn set_use_gates(row: &Table, fact: &meta::Use) -> mlua::Result<()> {
    row.set("done_clear", fact.done_clear)?;
    row.set("after_bank", fact.after.map(|(bank, _)| bank))?;
    row.set("after_bit", fact.after.map(|(_, bit)| bit))?;
    row.set("until_bank", fact.until.map(|(bank, _)| bank))?;
    row.set("until_bit", fact.until.map(|(_, bit)| bit))?;
    Ok(())
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
                    heals: game.item_defs.get(stack.id).heal.amount
                        != crate::item_catalog::HealAmount::None,
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
        let item_box = ItemBoxView::build(session);
        let examine = ExamineView::build(session);
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
            move_cursor: (mode == Some("move"))
                .then(|| menu.state_int("move_cursor"))
                .flatten(),
            item_box,
            examine,
        }
    }
}

/// The `bot:combine_result(slot, target)` table: `result` is `"applied"`
/// (an ordinary recipe), `"herb"` (a herb mix, applied after the menu's
/// yes/no prompt), `"no_recipe"`, `"herb_no_recipe"` or `"drug_store"`; for
/// the first two, the two slots afterwards (`cursor_item`/`cursor_quantity`,
/// `target_item`/`target_quantity`, id `0` when emptied) and `frees`, one of
/// them is emptied.
fn combine_result_table(
    lua: &Lua,
    snapshot: &SessionSnapshot,
    slot: usize,
    target: usize,
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    match game::combine_outcome(
        &snapshot.item_defs,
        &snapshot.inventory,
        slot,
        target,
        snapshot.drug_store,
    ) {
        Ok(outcome) => {
            table.set("result", if outcome.herb { "herb" } else { "applied" })?;
            table.set("cursor_item", outcome.cursor.id)?;
            table.set("cursor_quantity", outcome.cursor.quantity)?;
            table.set("target_item", outcome.target.id)?;
            table.set("target_quantity", outcome.target.quantity)?;
            table.set("frees", outcome.cursor.id == 0 || outcome.target.id == 0)?;
        }
        Err(refusal) => {
            table.set(
                "result",
                match refusal {
                    game::CombineResult::HerbNoRecipe => "herb_no_recipe",
                    game::CombineResult::NeedsDrugStore => "drug_store",
                    _ => "no_recipe",
                },
            )?;
            table.set("frees", false)?;
        }
    }
    Ok(table)
}

/// Whether the player's action probe can fire an AOT with these flags: the
/// probe (`GameState::probe_actions`) passes the player mask `0x01`, and an
/// action-key zone (`0x80`) also needs that bit.
fn player_can_fire(flags: u8) -> bool {
    flags & 0x01 != 0
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
        // Dead is below zero, as the engine's damage rule has it: a zombie
        // shot down to exactly 0 still walks and bites.
        if !entity.active() || entity.id >= 0x20 || entity.health < 0 {
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

/// The effect behaviours whose splash tests the player
/// (`effect_projectile_hit_check`): the faded fall (21, which runs the drop
/// splat every tick before handing over to it: the Black Tiger's spit flies
/// this way), the drop splat and the splash timer.
const PROJECTILE_BEHAVIOURS: [u8; 3] = [21, 22, 55];

/// The acid globs in flight: the live effects running a behaviour that
/// tests the player (the Black Tiger's spit, Plant 42's).
fn projectile_snapshots(game: &game::GameState) -> Vec<[i32; 3]> {
    (0..crate::effects::pool::EFFECT_POOL_SIZE)
        .filter_map(|index| game.effects.slot(index))
        .filter(|effect| {
            effect.anim_id != 0
                && (PROJECTILE_BEHAVIOURS.contains(&effect.anim_id)
                    || PROJECTILE_BEHAVIOURS.contains(&effect.update_id))
        })
        .map(|effect| {
            [
                i32::from(effect.pos[0]),
                i32::from(effect.pos[1]),
                i32::from(effect.pos[2]),
            ]
        })
        .collect()
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
            prompt: game.message.active
                && game.message.phase() == crate::message::MessagePhase::YesNo,
            prompt_yes: game.message.menu_choice_id() & 1 == 0,
            pickup: session.pickup_view.is_some(),
            reader: session
                .pickup_view
                .as_ref()
                .is_some_and(|pickup| pickup.reader.is_some()),
            frozen,
            controls_locked,
            controllable,
            dead: !session.death.is_idle()
                || game.flags[5].bit(game::MSF_PLAYER_DEAD)
                || entity.health < 0,
            ending: game.flags[5].bit(game::MSF2_DEATH_VARIANT)
                && !game.flags[5].bit(game::MSF2_ATTRACT_DEMO),
            typewriter: session.typewriter_arm.is_some()
                || session.typewriter_fade.is_some()
                || session.typewriter_open.is_some()
                || session.typewriter_exit_fade,
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
            locked: session.player.locked != player::LockedAction::None || session.player.vault_bit,
            equipped: game.equipped,
            radius: session.player.radius,
            collision_flags: session.player.collision_flags,
            grabbed: entity.zone_flags & 0x80 != 0,
            health_status: game.health_status,
            animation: entity.animation_id,
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
                // A pickup the player's probe cannot fire (no player bit, as
                // a script arms one only once a statue turns) is dormant.
                let item = action.kind == RoomActionKind::Item && player_can_fire(action.flags);
                ActionSnapshot {
                    slot: action.slot,
                    kind: if action.kind == RoomActionKind::Item && !item {
                        "item_dormant"
                    } else {
                        action_kind_name(action.kind)
                    },
                    x: i32::from(action.zone[0]),
                    z: i32::from(action.zone[1]),
                    width: i32::from(action.zone[2]),
                    depth: i32::from(action.zone[3]),
                    item: item.then(|| action.item_id()),
                    quantity: item.then(|| action.item_quantity()),
                    next_room: door.map(|door| door.next_room),
                    key: door.map(|door| door.key),
                    lock: door.map(|door| door.lock),
                    handler: action.handler,
                    flags: action.flags,
                    desk: desk_snapshot(game, action),
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
        let booted = !session
            .transition
            .as_ref()
            .is_some_and(|transition| transition.placed && !transition.entered);
        let objects = game
            .objects
            .records
            .iter()
            .enumerate()
            .filter(|(_, record)| record.active())
            .map(|(slot, record)| {
                use crate::objects::{
                    OBJECT_FLAG_INTANGIBLE, OBJECT_FLAG_NO_COLLISION, OBJECT_FLAG_NOT_PUSHABLE,
                };
                ObjectSnapshot {
                    slot,
                    x: record.pos[0],
                    z: record.pos[2],
                    half_x: i32::from(record.half_extents[0]),
                    half_z: i32::from(record.half_extents[2]),
                    pushable: record.flag
                        & (OBJECT_FLAG_NOT_PUSHABLE
                            | OBJECT_FLAG_INTANGIBLE
                            | OBJECT_FLAG_NO_COLLISION)
                        == 0,
                    solid: record.flag & (OBJECT_FLAG_INTANGIBLE | OBJECT_FLAG_NO_COLLISION) == 0,
                }
            })
            .collect();
        let (actions, doors, objects) = if booted {
            (actions, doors, objects)
        } else {
            (Vec::new(), Vec::new(), Vec::new())
        };
        let enemies = enemy_snapshots(game, session.player.pos, &session.loaded.room);
        let projectiles = projectile_snapshots(game);
        let weapon = weapon_snapshot(game, &session.player.weapon);
        let menu = MenuSnapshot::build(session);
        Self {
            room: id,
            booted,
            camera: session.loaded.room.current_cut,
            player,
            scene,
            actions,
            doors,
            objects,
            enemies,
            projectiles,
            weapon,
            menu,
            flags: game.flags,
            inventory: game.inventory.clone(),
            cameras: camera_zone_snapshots(&session.loaded.room),
            save_screen: session
                .save_screen
                .as_ref()
                .and_then(|screen| screen.bot_view()),
            saves: SaveSnapshot {
                written: session.saves_written,
                slot: session.last_saved_slot,
                needs_ribbon: game.needs_ink_ribbons(),
                ribbons: game.item_count(game.ink_ribbon_item()),
            },
            item_defs: game.item_defs.clone(),
            drug_store: game.is_drug_store(),
            computer: computer_snapshot(game),
            panel: session.passcode_panel.as_ref().map(|panel| PanelSnapshot {
                active: panel.active(),
                cursor: panel.cursor(),
                lamps: panel.lamps(),
            }),
        }
    }

    /// The catalog kind name of `item` (`"weapon"`, `"ammo"`, `"key"`, ...).
    fn item_kind(&self, item: u8) -> &'static str {
        self.item_defs.get(item).kind.name()
    }

    /// The ammunition item `weapon` fires (the engine's `weapon + 9`), when
    /// the catalog lists `weapon` as a weapon and that item as ammunition.
    fn ammo_for(&self, weapon: u8) -> Option<u8> {
        let ammo = crate::weapons::weapon_ammo_item_id(weapon);
        (weapon != 0 && self.item_kind(weapon) == "weapon" && self.item_kind(ammo) == "ammo")
            .then_some(ammo)
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
/// How much smaller than a room object its navigation box is, per side
/// ([`BotNav::sync_objects`]).
const OBJECT_NAV_INSET: i32 = 128;

/// The per-room navigation state.
#[derive(Debug, Default)]
struct BotNav {
    room_id: Option<RoomId>,
    /// Only the collision query is needed; the rest of `RoomState` stays out.
    /// It holds the room's boxes plus one box per solid room object.
    collision: RoomState,
    /// The room's own collision, before the object boxes.
    base_collision: crate::state::Collision,
    /// The object boxes `collision` was built with, as `(x, z, half_x,
    /// half_z)`.
    object_boxes: Vec<(i32, i32, i32, i32)>,
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
    /// The narrow-passage fallback lattice of this room ([`find_path`]),
    /// built on the first search the coarse lattice fails.
    fine: RefCell<Option<FineGrid>>,
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

/// The zones of the room's `<keep_out>` facts whose gate holds, as centre
/// and half extents: AOT slots the walks plan around. A slot that is not
/// set this tick has no zone.
fn keep_out_boxes(
    meta: Option<&meta::RoomMeta>,
    snapshot: &SessionSnapshot,
) -> Vec<(i32, i32, i32, i32)> {
    let Some(meta) = meta else {
        return Vec::new();
    };
    meta.keep_outs
        .iter()
        .filter(|fact| {
            fact.gate
                .is_none_or(|gate| snapshot.flag(gate.flag.0, gate.flag.1) == gate.set)
        })
        .filter_map(|fact| {
            snapshot
                .actions
                .iter()
                .find(|action| action.slot == fact.slot)
        })
        .map(|action| {
            (
                action.x + action.width / 2,
                action.z + action.depth / 2,
                action.width / 2,
                action.depth / 2,
            )
        })
        .collect()
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

/// The longest segment a sampled walk check looks at: the span of a 16-bit
/// room. A longer one comes from a garbage position, is never walkable, and
/// sampling it every 96 units would take billions of steps.
const MAX_SEGMENT: i32 = 0x10000;

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
    if length > MAX_SEGMENT {
        return false;
    }
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

/// A square planning lattice over `0..=GRID_MAX` with its blocked nodes.
///
/// The coarse lattice ([`GRID_STEP`]) is built for each search; the fine one
/// ([`FINE_STEP`]) is the narrow-passage fallback, built once per room and
/// body ([`FineGrid`]). Both test a node with the same [`blocked`] query (the
/// player radius plus the resolver skin) and an edge with [`line_clear`], so
/// a path on either never enters a solid record.
struct Lattice<'a> {
    step: i32,
    dim: usize,
    blocked: &'a [bool],
}

impl Lattice<'_> {
    fn xy(&self, index: usize) -> (i32, i32) {
        (
            GRID_MIN + (index % self.dim) as i32 * self.step,
            GRID_MIN + (index / self.dim) as i32 * self.step,
        )
    }

    fn index(&self, x: i32, z: i32) -> Option<usize> {
        if !(GRID_MIN..=GRID_MAX).contains(&x) || !(GRID_MIN..=GRID_MAX).contains(&z) {
            return None;
        }
        let ix = ((x - GRID_MIN) / self.step) as usize;
        let iz = ((z - GRID_MIN) / self.step) as usize;
        Some(iz * self.dim + ix)
    }

    fn snap(&self, x: i32, z: i32) -> (i32, i32) {
        let snap = |value: i32| {
            let snapped =
                (value - GRID_MIN + self.step / 2).div_euclid(self.step) * self.step + GRID_MIN;
            snapped.clamp(GRID_MIN, GRID_MAX)
        };
        (snap(x), snap(z))
    }

    /// The nearest free node to `(x, z)` within `rings` rings of its snap,
    /// whose straight line from `(x, z)` passes `accept`.
    fn nearest_free(
        &self,
        x: i32,
        z: i32,
        rings: i32,
        accept: impl Fn(usize) -> bool,
    ) -> Option<usize> {
        let (cx, cz) = self.snap(x, z);
        let base = self.index(cx, cz)?;
        let base = ((base % self.dim) as i32, (base / self.dim) as i32);
        for ring in 0..=rings {
            let mut best: Option<(usize, i64)> = None;
            for dz in -ring..=ring {
                for dx in -ring..=ring {
                    if ring > 0 && dx.abs() != ring && dz.abs() != ring {
                        continue;
                    }
                    let (ix, iz) = (base.0 + dx, base.1 + dz);
                    if !(0..self.dim as i32).contains(&ix) || !(0..self.dim as i32).contains(&iz) {
                        continue;
                    }
                    let index = iz as usize * self.dim + ix as usize;
                    if self.blocked[index] || !accept(index) {
                        continue;
                    }
                    let (nx, nz) = self.xy(index);
                    let dist = i64::from(nx - x).pow(2) + i64::from(nz - z).pow(2);
                    if best.is_none_or(|(_, best_dist)| dist < best_dist) {
                        best = Some((index, dist));
                    }
                }
            }
            if let Some((index, _)) = best {
                return Some(index);
            }
        }
        None
    }

    /// A* from `start` to `goal` (node indices), eight neighbours, no
    /// diagonal across a blocked corner, every edge sampled against the
    /// room. Enemy zones add their cost per [`GRID_STEP`] of travel.
    #[allow(clippy::too_many_arguments)]
    fn search(
        &self,
        room: &RoomState,
        start: usize,
        goal: usize,
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
        let nodes = self.dim * self.dim;
        let goal_xy = self.xy(goal);
        let mut open: BinaryHeap<(Reverse<u32>, usize)> = BinaryHeap::new();
        let mut cost = vec![u32::MAX; nodes];
        let mut parent = vec![usize::MAX; nodes];
        let mut closed = vec![false; nodes];
        let heuristic = |index: usize| -> u32 {
            let (x, z) = self.xy(index);
            let dx = (x - goal_xy.0).unsigned_abs() / self.step as u32;
            let dz = (z - goal_xy.1).unsigned_abs() / self.step as u32;
            let diagonal = dx.min(dz);
            (dx + dz - diagonal) * 10 + diagonal * 14
        };
        cost[start] = 0;
        open.push((Reverse(heuristic(start)), start));
        while let Some((Reverse(_), index)) = open.pop() {
            if index == goal {
                let mut path = vec![self.xy(index)];
                let mut current = index;
                while parent[current] != usize::MAX {
                    current = parent[current];
                    path.push(self.xy(current));
                }
                path.reverse();
                return Some(path);
            }
            if closed[index] {
                continue;
            }
            closed[index] = true;
            let (x, z) = self.xy(index);
            for (dx, dz, step_cost) in DIRS {
                let nx = x + dx * self.step;
                let nz = z + dz * self.step;
                let Some(next) = self.index(nx, nz) else {
                    continue;
                };
                if closed[next] || self.blocked[next] {
                    continue;
                }
                // A diagonal may not cut a blocked corner: both orthogonal
                // neighbours must be walkable too.
                if dx != 0 && dz != 0 {
                    let side_x = self.index(nx, z).is_some_and(|side| !self.blocked[side]);
                    let side_z = self.index(x, nz).is_some_and(|side| !self.blocked[side]);
                    if !side_x || !side_z {
                        continue;
                    }
                }
                // The wall can be thinner than the grid step, so an
                // endpoint-only test would jump it. Sample the edge itself.
                if !line_clear(room, (x, z), (nx, nz), y, radius, flags) {
                    continue;
                }
                let penalty = u64::from(steer::zone_penalty(zones, nx, nz)) * self.step as u64
                    / GRID_STEP as u64;
                let next_cost = cost[index]
                    .saturating_add(step_cost)
                    .saturating_add(penalty.min(u64::from(u32::MAX)) as u32);
                if next_cost < cost[next] {
                    cost[next] = next_cost;
                    parent[next] = index;
                    open.push((Reverse(next_cost + heuristic(next)), next));
                }
            }
        }
        None
    }
}

/// The coarse lattice's snap of `(x, z)`.
#[cfg(test)]
fn nearest_grid(x: i32, z: i32) -> (i32, i32) {
    Lattice {
        step: GRID_STEP,
        dim: GRID_DIM,
        blocked: &[],
    }
    .snap(x, z)
}

/// End a lattice path at the point the route actually named when the last
/// leg is clear, so the bot does not run past the node snap and spin back.
fn end_at_exact_goal(
    room: &RoomState,
    mut path: Vec<(i32, i32)>,
    exact_goal: (i32, i32),
    y: i32,
    radius: i32,
    flags: u8,
) -> Vec<(i32, i32)> {
    let last = path.len() - 1;
    if path[last] == exact_goal || blocked(room, exact_goal.0, exact_goal.1, y, radius, flags) {
        return path;
    }
    if last > 0 && line_clear(room, path[last - 1], exact_goal, y, radius, flags) {
        path[last] = exact_goal;
    } else if line_clear(room, path[last], exact_goal, y, radius, flags) {
        path.push(exact_goal);
    }
    path
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
    let mut nodes = vec![false; GRID_NODES];
    let probe = Lattice {
        step: GRID_STEP,
        dim: GRID_DIM,
        blocked: &[],
    };
    for (index, node) in nodes.iter_mut().enumerate() {
        let (x, z) = probe.xy(index);
        *node = blocked(room, x, z, y, radius, flags);
    }
    let lattice = Lattice {
        blocked: &nodes,
        ..probe
    };
    let start_index = lattice.nearest_free(start.0, start.1, 4, |_| true)?;
    // The player may start wedged in a soft spot, but a goal the route names
    // must be walkable; silently snapping it to the far side of a wall would
    // make `arrived` lie about where the player is.
    let snapped = lattice.snap(goal.0, goal.1);
    let goal_index = lattice.index(snapped.0, snapped.1)?;
    if nodes[goal_index] {
        return None;
    }
    let path = lattice.search(room, start_index, goal_index, y, radius, flags, zones)?;
    Some(end_at_exact_goal(room, path, goal, y, radius, flags))
}

/// The narrow-passage fallback's lattice step. A corridor whose band of
/// free player centres is narrower than [`GRID_STEP`] can fall between two
/// coarse rows (the 10C bed passage's band is under 500 units); a 128-unit
/// lattice puts several rows in any band a body can stand in.
const FINE_STEP: i32 = 128;
const FINE_DIM: usize = ((GRID_MAX - GRID_MIN) / FINE_STEP + 1) as usize;
/// Rings the fine lattice searches for a free node next to the start or goal.
const FINE_RINGS: i32 = 4;
/// Half steps a goal no node sees walks out along an axis, looking for a
/// point that one does: 4096 units.
const FINE_BEND_STEPS: i32 = 64;

/// The fine lattice of one room and body: its blocked nodes and the
/// four-connected component of each free node, so a goal in another part of
/// the room fails without a search.
#[derive(Debug)]
struct FineGrid {
    key: (i32, i32, u8),
    blocked: Vec<bool>,
    component: Vec<u32>,
}

impl FineGrid {
    /// The cache key: the body, and its height only where the collision
    /// reads it (an RE2 room's SCA records).
    fn key(room: &RoomState, y: i32, radius: i32, flags: u8) -> (i32, i32, u8) {
        (if room.sca.is_some() { y } else { 0 }, radius, flags)
    }

    fn build(room: &RoomState, y: i32, radius: i32, flags: u8) -> Self {
        let nodes = FINE_DIM * FINE_DIM;
        let probe = Lattice {
            step: FINE_STEP,
            dim: FINE_DIM,
            blocked: &[],
        };
        let blocked_nodes: Vec<bool> = (0..nodes)
            .map(|index| {
                let (x, z) = probe.xy(index);
                blocked(room, x, z, y, radius, flags)
            })
            .collect();
        let mut component = vec![u32::MAX; nodes];
        let mut label = 0;
        let mut queue = Vec::new();
        for seed in 0..nodes {
            if blocked_nodes[seed] || component[seed] != u32::MAX {
                continue;
            }
            component[seed] = label;
            queue.push(seed);
            while let Some(index) = queue.pop() {
                let (x, z) = probe.xy(index);
                for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let (nx, nz) = (x + dx * FINE_STEP, z + dz * FINE_STEP);
                    let Some(next) = probe.index(nx, nz) else {
                        continue;
                    };
                    if blocked_nodes[next]
                        || component[next] != u32::MAX
                        || !line_clear(room, (x, z), (nx, nz), y, radius, flags)
                    {
                        continue;
                    }
                    component[next] = label;
                    queue.push(next);
                }
            }
            label += 1;
        }
        Self {
            key: Self::key(room, y, radius, flags),
            blocked: blocked_nodes,
            component,
        }
    }

    fn lattice(&self) -> Lattice<'_> {
        Lattice {
            step: FINE_STEP,
            dim: FINE_DIM,
            blocked: &self.blocked,
        }
    }

    /// A fine-lattice path from `start` to `goal`, or `None` when the goal
    /// is blocked, or no free node next to it shares the start's component.
    #[allow(clippy::too_many_arguments)]
    fn path(
        &self,
        room: &RoomState,
        start: (i32, i32),
        goal: (i32, i32),
        y: i32,
        radius: i32,
        flags: u8,
        zones: &[steer::EnemyZone],
    ) -> Option<Vec<(i32, i32)>> {
        if blocked(room, goal.0, goal.1, y, radius, flags) {
            return None;
        }
        let lattice = self.lattice();
        // The goal node must see the exact goal, so the tail cannot hop a
        // wall thinner than the step.
        let sighted = |point: (i32, i32)| {
            lattice.nearest_free(point.0, point.1, FINE_RINGS, |index| {
                line_clear(room, lattice.xy(index), point, y, radius, flags)
            })
        };
        // A goal at the end of a corridor narrower than the step (406's
        // strip beside the crate) has no node in sight: walk out of it along
        // an axis, in sight of the goal, to the first point a node sees, and
        // bend there.
        let (goal_index, bend) = match sighted(goal) {
            Some(index) => (index, None),
            None => (1..=FINE_BEND_STEPS)
                .flat_map(|step| {
                    let reach = step * FINE_STEP / 2;
                    [(reach, 0), (-reach, 0), (0, reach), (0, -reach)]
                })
                .map(|(dx, dz)| (goal.0 + dx, goal.1 + dz))
                .filter(|&point| line_clear(room, goal, point, y, radius, flags))
                .find_map(|point| sighted(point).map(|index| (index, Some(point))))?,
        };
        let part = self.component[goal_index];
        let start_index = lattice.nearest_free(start.0, start.1, FINE_RINGS, |index| {
            self.component[index] == part
        })?;
        let mut path = lattice.search(room, start_index, goal_index, y, radius, flags, zones)?;
        path.extend(bend);
        Some(end_at_exact_goal(room, path, goal, y, radius, flags))
    }
}

/// The path search behind `walk_to` and `bot:path`: the coarse lattice
/// first, then, only when it finds nothing, the fine lattice of `fine`
/// (built on first use for this room and body). Both lattices test nodes and
/// edges with the same body and skin, so the fallback finds narrow passages
/// without ever routing through collision.
#[allow(clippy::too_many_arguments)]
fn find_path(
    room: &RoomState,
    fine: &RefCell<Option<FineGrid>>,
    start: (i32, i32),
    goal: (i32, i32),
    y: i32,
    radius: i32,
    flags: u8,
    zones: &[steer::EnemyZone],
) -> Option<Vec<(i32, i32)>> {
    if let Some(path) = find_grid_path_with(room, start, goal, y, radius, flags, zones) {
        return Some(path);
    }
    let key = FineGrid::key(room, y, radius, flags);
    let mut cache = fine.borrow_mut();
    if cache.as_ref().is_none_or(|grid| grid.key != key) {
        *cache = Some(FineGrid::build(room, y, radius, flags));
    }
    cache
        .as_ref()
        .and_then(|grid| grid.path(room, start, goal, y, radius, flags, zones))
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
        // A start inside the body margin (a scripted move such as a stair
        // can leave the player overlapping the conservative skin) sees
        // nothing: step to the search's first node, the nearest free one.
        let best = match best {
            Some(best) => best,
            None if out.is_empty() && blocked(room, start.0, start.1, y, radius, flags) => 0,
            None => break,
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
            // A scripted collision rewrite (`inst_cfg`) can land after the
            // first tick in the room: on a door entry the init's edits are
            // applied once its scripts run. 300's elevator shaft is walled
            // until its init opens it, so a stale copy kept the platform
            // out of reach. Take the live boundaries again.
            if room.collision != self.base_collision {
                self.collision = collision_room(room);
                self.base_collision = self.collision.collision.clone();
                self.object_boxes.clear();
                self.fine = RefCell::new(None);
                self.drop_path();
                self.unreachable_goal = None;
                self.blocked_goal = None;
            }
            return;
        }
        self.room_id = Some(id);
        self.collision = collision_room(room);
        self.base_collision = self.collision.collision.clone();
        self.object_boxes.clear();
        self.fine = RefCell::new(None);
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

    /// Plan around the room's solid objects: walking into a pushable one
    /// would push it (a solved block pushed off its mark), and a solid one
    /// stops the walk. A moved object rebuilds the boxes and drops the cached
    /// path and the narrow-passage lattice. RE2's SCA rooms keep their own
    /// collision.
    ///
    /// The boxes are [`OBJECT_NAV_INSET`] smaller than the object on each
    /// side, so the narrow strip between an object and a wall (where a push
    /// stand point often lies) stays on the lattice; a walk that grazes the
    /// box edge slides off without the nine forward frames a push needs.
    ///
    /// `keep_outs` (centre and half extents, from the room's `<keep_out>`
    /// facts) are walled off whole: a trap probe the walk must not cross.
    fn sync_objects(&mut self, objects: &[ObjectSnapshot], keep_outs: &[(i32, i32, i32, i32)]) {
        if self.collision.sca.is_some() {
            return;
        }
        let boxes: Vec<(i32, i32, i32, i32)> = objects
            .iter()
            .filter(|object| {
                object.solid && object.half_x > OBJECT_NAV_INSET && object.half_z > OBJECT_NAV_INSET
            })
            .map(|object| {
                (
                    object.x,
                    object.z,
                    object.half_x - OBJECT_NAV_INSET,
                    object.half_z - OBJECT_NAV_INSET,
                )
            })
            .chain(keep_outs.iter().copied())
            .collect();
        if boxes == self.object_boxes {
            return;
        }
        let mut collision = self.base_collision.clone();
        let clamp = |value: i32| value.clamp(0, i32::from(u16::MAX)) as u16;
        for &(x, z, half_x, half_z) in &boxes {
            let rect = crate::state::CollisionRect {
                x_max: clamp(x + half_x),
                z_max: clamp(z + half_z),
                x_min: clamp(x - half_x),
                z_min: clamp(z - half_z),
                kind: 1,
                flags: 0,
            };
            for quadrant in &mut collision.quadrants {
                quadrant.push(rect);
            }
        }
        self.collision.collision = collision;
        self.object_boxes = boxes;
        self.fine = RefCell::new(None);
        self.drop_path();
        self.unreachable_goal = None;
        self.blocked_goal = None;
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
            && let Some(path) = find_path(room, &self.fine, pos, point, y, radius, flags, zones)
        {
            let mut path = smooth_path(room, pos, path, y, radius, flags, zones);
            if path.last() == Some(&point) {
                path.push(goal);
                let pin = path.len() - 2;
                return Some((path, Some(pin)));
            }
        }
        let path = find_path(room, &self.fine, pos, goal, y, radius, flags, zones)?;
        let path = smooth_path(room, pos, path, y, radius, flags, zones);
        (!path.is_empty()).then_some((path, None))
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
    /// What the route's API calls cost, by method.
    profile: Rc<RefCell<stats::ApiProfile>>,
    /// What the last `bot:world()` table was built from.
    world_key: Rc<RefCell<Option<world::LuaKey>>>,
    memory: Rc<RefCell<BotMemory>>,
    nav: Rc<RefCell<BotNav>>,
    input: Rc<RefCell<BotInput>>,
    finished: Rc<Cell<bool>>,
    finish_reason: Rc<RefCell<Option<String>>>,
}

impl BotShares {
    /// Another handle on the same shared values.
    fn share(&self) -> Self {
        Self {
            profile: Rc::clone(&self.profile),
            world_key: Rc::clone(&self.world_key),
            memory: Rc::clone(&self.memory),
            nav: Rc::clone(&self.nav),
            input: Rc::clone(&self.input),
            finished: Rc::clone(&self.finished),
            finish_reason: Rc::clone(&self.finish_reason),
        }
    }
}

impl Default for BotShares {
    fn default() -> Self {
        Self {
            profile: Rc::default(),
            world_key: Rc::default(),
            memory: Rc::new(RefCell::new(BotMemory::default())),
            nav: Rc::new(RefCell::new(BotNav::default())),
            input: Rc::new(RefCell::new(BotInput::default())),
            finished: Rc::new(Cell::new(false)),
            finish_reason: Rc::new(RefCell::new(None)),
        }
    }
}

/// One completed save the bot saw: the slot the save screen wrote, the room
/// and the bot tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SaveRecord {
    slot: Option<usize>,
    room: RoomId,
    tick: u64,
}

/// The run's saves, reloads and deaths, observed from the engine and kept
/// across a reload (`bot:saves()`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SaveLog {
    /// Saves written this run.
    count: u32,
    /// The latest one.
    last: Option<SaveRecord>,
    /// Saves the bot loaded after a death.
    reloads: u32,
    /// Deaths this run.
    deaths: u32,
    /// Deaths since the latest save was written.
    deaths_since_save: u32,
}

/// The `bot:saves()` table.
fn saves_table(lua: &Lua, log: &SaveLog, facts: Option<&SaveSnapshot>) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("count", log.count)?;
    table.set("reloads", log.reloads)?;
    table.set("deaths", log.deaths)?;
    table.set("deaths_since_save", log.deaths_since_save)?;
    if let Some(last) = &log.last {
        table.set("slot", last.slot)?;
        table.set("room", last.room.room3())?;
        table.set("tick", last.tick)?;
    }
    if let Some(facts) = facts {
        table.set("needs_ribbon", facts.needs_ribbon)?;
        table.set("ribbons", facts.ribbons)?;
    }
    Ok(table)
}

/// The `scene.save_screen` table.
fn save_screen_table(lua: &Lua, view: &SaveScreenView) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("mode", if view.save { "save" } else { "load" })?;
    table.set("stage", view.stage.as_str())?;
    table.set("cursor", view.cursor)?;
    table.set("exit_row", view.exit_row)?;
    table.set(
        "slots",
        lua.create_sequence_from(view.slots.iter().copied())?,
    )?;
    if let Some(rows) = &view.rows {
        table.set("rows", lua.create_sequence_from(rows.iter().copied())?)?;
    }
    if let Some(yes) = view.answer {
        table.set("answer", if yes { "yes" } else { "no" })?;
    }
    Ok(table)
}

/// The Lua-facing bot handle. Every method is a read of the snapshot or a
/// normal input write; there is deliberately no state-authoring method.
struct BotCtx {
    snapshot: SessionSnapshot,
    /// The run's save history.
    saves: SaveLog,
    /// The current room's authored metadata, cached by [`BotRuntime`].
    meta: Option<Rc<meta::RoomMeta>>,
    /// Every room's `<use>` facts, cached by [`BotRuntime`].
    uses: Rc<Vec<meta::RoomUse>>,
    /// Every room's `<holds>` facts, cached by [`BotRuntime`].
    holds: Rc<Vec<meta::RoomHolds>>,
    /// Every room's `<fight>` facts, cached by [`BotRuntime`].
    fights: Rc<Vec<meta::RoomFight>>,
    /// Every room's `<ride>` facts, cached by [`BotRuntime`].
    rides: Rc<Vec<meta::RoomRide>>,
    /// Every room's `<revisit>` facts, cached by [`BotRuntime`].
    revisits: Rc<Vec<meta::RoomRevisit>>,
    /// Every room's `<reach>` facts, cached by [`BotRuntime`].
    reaches: Rc<Vec<meta::RoomReach>>,
    /// Every room's `<deliver>`/`<fetch>` facts, cached by [`BotRuntime`].
    errands: Rc<Vec<meta::RoomErrand>>,
    /// Every room's `<brew>` facts with its taps, cached by [`BotRuntime`].
    brews: Rc<Vec<meta::RoomBrew>>,
    /// Every room's `<avoid>` facts, cached by [`BotRuntime`].
    avoid_facts: Rc<Vec<meta::RoomAvoid>>,
    /// Every room's `<door>` facts, cached by [`BotRuntime`].
    door_vias: Rc<Vec<meta::RoomDoorVia>>,
    /// The game's combat knowledge file, if the pack carries one.
    knowledge: Option<Rc<knowledge::Knowledge>>,
    tick: u64,
    /// The `--bot-risk` setting.
    risk: f64,
    /// The story parameters (`params/<game>.xml`, `--bot-param`).
    params: Rc<params::Params>,
    /// The `--bot-goal-room` setting.
    goal_room: Option<Rc<str>>,
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

/// The registry name of the Lua table holding the results of the tick's
/// calls to the methods [`Profiled::add_tick_method`] registered. Slot 0 is
/// the tick they belong to.
const TICK_MEMO: &str = "bot.tick_memo";
/// The registry name of the table holding the results
/// [`Profiled::add_run_method`]'s methods built for this Lua state.
const RUN_MEMO: &str = "bot.run_memo";

/// The memo table registered as `name`, fresh when it is missing or, with
/// a `tick`, was filled on another tick.
fn memo_table(lua: &Lua, name: &str, tick: Option<u64>) -> mlua::Result<Table> {
    if let Some(memo) = lua.named_registry_value::<Option<Table>>(name)?
        && tick.is_none_or(|tick| memo.raw_get::<Option<u64>>(0).ok().flatten() == Some(tick))
    {
        return Ok(memo);
    }
    let memo = lua.create_table()?;
    if let Some(tick) = tick {
        memo.raw_set(0, tick)?;
    }
    lua.set_named_registry_value(name, &memo)?;
    Ok(memo)
}

/// The result `memo` holds for `method`, or the one `build` makes, kept
/// there. A nil result is kept as the memo table itself.
fn memoised(
    memo: &Table,
    method: &'static str,
    build: impl FnOnce() -> mlua::Result<Value>,
) -> mlua::Result<Value> {
    match memo.raw_get::<Value>(method)? {
        Value::Nil => {}
        Value::Table(held) if held == *memo => return Ok(Value::Nil),
        held => return Ok(held),
    }
    let value = build()?;
    match &value {
        Value::Nil => memo.raw_set(method, memo)?,
        value => memo.raw_set(method, value)?,
    }
    Ok(value)
}

/// Registers [`BotCtx`]'s methods so that every call's time is added to the
/// run's API profile (`compute.api` of the report).
struct Profiled<'a, M>(&'a mut M);

impl<M: UserDataMethods<BotCtx>> Profiled<'_, M> {
    /// A method without arguments whose result is the same for the whole
    /// tick (it reads the tick's snapshot or the room's facts): the first
    /// call of a tick builds it and the later ones get the same value, so
    /// the route must not write to it.
    fn add_tick_method<R: mlua::IntoLua>(
        &mut self,
        name: &'static str,
        method: impl Fn(&Lua, &BotCtx, ()) -> mlua::Result<R> + 'static,
    ) {
        self.add_method(name, move |lua, bot, ()| {
            let memo = memo_table(lua, TICK_MEMO, Some(bot.tick))?;
            memoised(&memo, name, || method(lua, bot, ())?.into_lua(lua))
        });
    }

    /// As [`Self::add_tick_method`], for a result that is the same for the
    /// whole run: built once for the Lua state.
    fn add_run_method<R: mlua::IntoLua>(
        &mut self,
        name: &'static str,
        method: impl Fn(&Lua, &BotCtx, ()) -> mlua::Result<R> + 'static,
    ) {
        self.add_method(name, move |lua, bot, ()| {
            let memo = memo_table(lua, RUN_MEMO, None)?;
            memoised(&memo, name, || method(lua, bot, ())?.into_lua(lua))
        });
    }

    fn add_method<A, R>(
        &mut self,
        name: &'static str,
        method: impl Fn(&Lua, &BotCtx, A) -> mlua::Result<R> + 'static,
    ) where
        A: mlua::FromLuaMulti,
        R: mlua::IntoLuaMulti,
    {
        self.0.add_method(name, move |lua, bot, args| {
            let started = std::time::Instant::now();
            let result = method(lua, bot, args);
            bot.shares
                .profile
                .borrow_mut()
                .note(name, started.elapsed());
            result
        });
    }
}

impl UserData for BotCtx {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        let methods = &mut Profiled(methods);
        methods.add_method("tick", |_lua, bot, ()| Ok(bot.tick));

        methods.add_tick_method("room", |lua, bot, ()| {
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

        methods.add_tick_method("room_meta", |lua, bot, ()| {
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
                match fact.target {
                    meta::Target::Slot(slot) => {
                        row.set("kind", "press")?;
                        row.set("slot", slot)?;
                    }
                    meta::Target::Camera(cut) => {
                        row.set("kind", "camera")?;
                        row.set("cut", cut)?;
                    }
                    meta::Target::Push {
                        object,
                        slot,
                        direction,
                        clear_full,
                        until_stop,
                        axis,
                    } => {
                        row.set("kind", "push")?;
                        row.set("object", object)?;
                        row.set("slot", slot)?;
                        if let Some(direction) = direction {
                            let (dx, dz) = direction.step();
                            row.set("direction", direction.name())?;
                            row.set("dx", dx)?;
                            row.set("dz", dz)?;
                            row.set("clear_full", clear_full)?;
                            row.set("until_stop", until_stop)?;
                        }
                        if let Some(axis) = axis {
                            row.set("axis", axis.name())?;
                        }
                    }
                    meta::Target::Climb { object, slot } => {
                        row.set("kind", "climb")?;
                        row.set("object", object)?;
                        row.set("slot", slot)?;
                    }
                    meta::Target::Fight {
                        enemy,
                        stage,
                        optional,
                    } => {
                        row.set("kind", "fight")?;
                        row.set("enemy", enemy)?;
                        row.set("optional", optional)?;
                        row.set("stage", stage.map(|byte| char::from(byte).to_string()))?;
                    }
                    meta::Target::Wait => {
                        row.set("kind", "wait")?;
                    }
                }
                row.set("bank", fact.flag.map(|(bank, _)| bank))?;
                row.set("bit", fact.flag.map(|(_, bit)| bit))?;
                row.set("done_clear", fact.done_clear)?;
                row.set("after_bank", fact.after.map(|(bank, _)| bank))?;
                row.set("after_bit", fact.after.map(|(_, bit)| bit))?;
                row.set("after_clear", fact.after_clear)?;
                row.set("reveals", fact.reveals)?;
                row.set("opens", fact.opens)?;
                row.set("label", fact.label.as_deref())?;
                interacts.push(row)?;
            }
            table.set("interacts", interacts)?;
            let uses = lua.create_table()?;
            for fact in &meta.uses {
                let row = lua.create_table()?;
                row.set("slot", fact.slot)?;
                row.set("item", fact.item)?;
                row.set("bank", fact.flag.0)?;
                row.set("bit", fact.flag.1)?;
                set_use_gates(&row, fact)?;
                row.set("label", fact.label.as_deref())?;
                uses.push(row)?;
            }
            table.set("uses", uses)?;
            let answers = lua.create_table()?;
            for fact in &meta.answers {
                let row = lua.create_table()?;
                row.set("choice", if fact.yes { "yes" } else { "no" })?;
                row.set("slot", fact.slot)?;
                row.set("bank", fact.flag.map(|(bank, _)| bank))?;
                row.set("bit", fact.flag.map(|(_, bit)| bit))?;
                row.set("message", fact.message)?;
                row.set("label", fact.label.as_deref())?;
                answers.push(row)?;
            }
            table.set("answers", answers)?;
            let avoids = lua.create_table()?;
            for fact in &meta.avoids {
                let row = lua.create_table()?;
                row.set("slot", fact.slot)?;
                row.set("reason", fact.reason.as_str())?;
                if let Some(gate) = fact.gate {
                    row.set("bank", gate.flag.0)?;
                    row.set("bit", gate.flag.1)?;
                    row.set("while", if gate.set { "set" } else { "clear" })?;
                }
                row.set("unless_held", fact.unless_held)?;
                avoids.push(row)?;
            }
            table.set("avoids", avoids)?;
            let watches = lua.create_table()?;
            for fact in &meta.watches {
                let row = lua.create_table()?;
                row.set("bank", fact.flag.0)?;
                row.set("bit", fact.flag.1)?;
                row.set("after_bank", fact.after.map(|(bank, _)| bank))?;
                row.set("after_bit", fact.after.map(|(_, bit)| bit))?;
                row.set("label", fact.label.as_deref())?;
                watches.push(row)?;
            }
            table.set("watches", watches)?;
            let leaves = lua.create_table()?;
            for fact in &meta.leaves {
                let row = lua.create_table()?;
                row.set("slot", fact.slot)?;
                row.set("bank", fact.flag.0)?;
                row.set("bit", fact.flag.1)?;
                row.set("escape", fact.escape)?;
                row.set("label", fact.label.as_deref())?;
                leaves.push(row)?;
            }
            table.set("leaves", leaves)?;
            let taps = lua.create_table()?;
            for fact in &meta.taps {
                let row = lua.create_table()?;
                row.set("slot", fact.slot)?;
                row.set("item", fact.item)?;
                row.set("label", fact.label.as_deref())?;
                taps.push(row)?;
            }
            table.set("taps", taps)?;
            let brews = lua.create_table()?;
            for fact in &meta.brews {
                let row = lua.create_table()?;
                row.set("item", fact.item)?;
                row.set("bottle", fact.bottle)?;
                row.set("bank", fact.flag.0)?;
                row.set("bit", fact.flag.1)?;
                row.set("after_bank", fact.after.map(|(bank, _)| bank))?;
                row.set("after_bit", fact.after.map(|(_, bit)| bit))?;
                row.set("after_clear", fact.after_clear)?;
                row.set("label", fact.label.as_deref())?;
                brews.push(row)?;
            }
            table.set("brews", brews)?;
            let kites = lua.create_table()?;
            for fact in &meta.kites {
                let row = lua.create_table()?;
                row.set("enemy", fact.enemy)?;
                row.set("x", fact.centre.0)?;
                row.set("z", fact.centre.1)?;
                row.set("radius", fact.radius)?;
                kites.push(row)?;
            }
            table.set("kites", kites)?;
            Ok(Value::Table(table))
        });

        methods.add_tick_method("use_facts", |lua, bot, ()| {
            let table = lua.create_table()?;
            for entry in bot.uses.iter() {
                let row = lua.create_table()?;
                row.set("room", entry.room.as_str())?;
                row.set("slot", entry.fact.slot)?;
                row.set("item", entry.fact.item)?;
                row.set("bank", entry.fact.flag.0)?;
                row.set("bit", entry.fact.flag.1)?;
                set_use_gates(&row, &entry.fact)?;
                row.set("label", entry.fact.label.as_deref())?;
                table.push(row)?;
            }
            Ok(table)
        });

        methods.add_tick_method("item_facts", |lua, bot, ()| {
            let table = lua.create_table()?;
            for entry in bot.holds.iter() {
                let row = lua.create_table()?;
                row.set("room", entry.room.as_str())?;
                row.set("item", entry.fact.item)?;
                row.set("label", entry.fact.label.as_deref())?;
                table.push(row)?;
            }
            Ok(table)
        });

        methods.add_tick_method("fight_facts", |lua, bot, ()| {
            let table = lua.create_table()?;
            for entry in bot.fights.iter() {
                let row = lua.create_table()?;
                row.set("room", entry.room.as_str())?;
                row.set("enemy", entry.enemy)?;
                row.set("bank", entry.flag.map(|(bank, _)| bank))?;
                row.set("bit", entry.flag.map(|(_, bit)| bit))?;
                row.set(
                    "stage",
                    entry.stage.map(|byte| char::from(byte).to_string()),
                )?;
                row.set("optional", entry.optional)?;
                table.push(row)?;
            }
            Ok(table)
        });

        methods.add_tick_method("ride_facts", |lua, bot, ()| {
            let table = lua.create_table()?;
            for entry in bot.rides.iter() {
                let row = lua.create_table()?;
                row.set("room", entry.room.as_str())?;
                row.set("slot", entry.fact.slot)?;
                row.set("from", entry.fact.from.as_str())?;
                row.set("label", entry.fact.label.as_deref())?;
                table.push(row)?;
            }
            Ok(table)
        });

        methods.add_tick_method("revisit_facts", |lua, bot, ()| {
            let table = lua.create_table()?;
            for entry in bot.revisits.iter() {
                let row = lua.create_table()?;
                row.set("room", entry.room.as_str())?;
                row.set("bank", entry.fact.gate.flag.0)?;
                row.set("bit", entry.fact.gate.flag.1)?;
                row.set("set", entry.fact.gate.set)?;
                row.set("label", entry.fact.label.as_deref())?;
                table.push(row)?;
            }
            Ok(table)
        });

        methods.add_tick_method("reach_facts", |lua, bot, ()| {
            let table = lua.create_table()?;
            for entry in bot.reaches.iter() {
                let row = lua.create_table()?;
                row.set("room", entry.room.as_str())?;
                row.set("slot", entry.fact.slot)?;
                row.set("bank", entry.fact.gate.flag.0)?;
                row.set("bit", entry.fact.gate.flag.1)?;
                row.set("set", entry.fact.gate.set)?;
                row.set("same_entry", entry.fact.same_entry)?;
                if let Some((bank, bit)) = entry.fact.here {
                    row.set("here_bank", bank)?;
                    row.set("here_bit", bit)?;
                }
                row.set("label", entry.fact.label.as_deref())?;
                table.push(row)?;
            }
            Ok(table)
        });

        methods.add_tick_method("door_facts", |lua, bot, ()| {
            let table = lua.create_table()?;
            for entry in bot.door_vias.iter() {
                let row = lua.create_table()?;
                row.set("room", entry.room.as_str())?;
                row.set("slot", entry.fact.slot)?;
                row.set("via", entry.fact.via)?;
                row.set("gate_bank", entry.fact.gate.map(|gate| gate.flag.0))?;
                row.set("gate_bit", entry.fact.gate.map(|gate| gate.flag.1))?;
                row.set("gate_set", entry.fact.gate.map(|gate| gate.set))?;
                row.set("label", entry.fact.label.as_deref())?;
                table.push(row)?;
            }
            Ok(table)
        });

        methods.add_tick_method("avoid_facts", |lua, bot, ()| {
            let table = lua.create_table()?;
            for entry in bot.avoid_facts.iter() {
                let row = lua.create_table()?;
                row.set("room", entry.room.as_str())?;
                row.set("slot", entry.fact.slot)?;
                row.set("reason", entry.fact.reason.as_str())?;
                row.set("bank", entry.fact.gate.map(|gate| gate.flag.0))?;
                row.set("bit", entry.fact.gate.map(|gate| gate.flag.1))?;
                row.set(
                    "while",
                    entry
                        .fact
                        .gate
                        .map(|gate| if gate.set { "set" } else { "clear" }),
                )?;
                row.set("unless_held", entry.fact.unless_held)?;
                table.push(row)?;
            }
            Ok(table)
        });

        // Every room's open-or-not <brew> facts with that room's taps:
        // `{room, item, bottle, bank, bit, after_bank, after_bit,
        // after_clear, label, taps = {{slot, item, label}, ...}}`.
        methods.add_tick_method("brew_facts", |lua, bot, ()| {
            let table = lua.create_table()?;
            for entry in bot.brews.iter() {
                let fact = &entry.fact;
                let row = lua.create_table()?;
                row.set("room", entry.room.as_str())?;
                row.set("item", fact.item)?;
                row.set("bottle", fact.bottle)?;
                row.set("bank", fact.flag.0)?;
                row.set("bit", fact.flag.1)?;
                row.set("after_bank", fact.after.map(|(bank, _)| bank))?;
                row.set("after_bit", fact.after.map(|(_, bit)| bit))?;
                row.set("after_clear", fact.after_clear)?;
                row.set("label", fact.label.as_deref())?;
                let taps = lua.create_table()?;
                for tap in &entry.taps {
                    let t = lua.create_table()?;
                    t.set("slot", tap.slot)?;
                    t.set("item", tap.item)?;
                    t.set("label", tap.label.as_deref())?;
                    taps.push(t)?;
                }
                row.set("taps", taps)?;
                table.push(row)?;
            }
            Ok(table)
        });

        methods.add_tick_method("errand_facts", |lua, bot, ()| {
            let table = lua.create_table()?;
            for entry in bot.errands.iter() {
                let fact = &entry.fact;
                let row = lua.create_table()?;
                row.set("room", entry.room.as_str())?;
                match fact.kind {
                    meta::ErrandKind::Deliver { cut } => {
                        row.set("kind", "deliver")?;
                        row.set("cut", cut)?;
                    }
                    meta::ErrandKind::Fetch { slot } => {
                        row.set("kind", "fetch")?;
                        row.set("slot", slot)?;
                    }
                }
                row.set("item", fact.item)?;
                row.set("bank", fact.flag.0)?;
                row.set("bit", fact.flag.1)?;
                row.set("after_bank", fact.after.map(|(bank, _)| bank))?;
                row.set("after_bit", fact.after.map(|(_, bit)| bit))?;
                row.set("label", fact.label.as_deref())?;
                table.push(row)?;
            }
            Ok(table)
        });

        // The whole world model is a large table and changes on few ticks:
        // the last one built is handed out again while what it was built
        // from still holds.
        methods.add_tick_method("world", |lua, bot, ()| {
            let memo = memo_table(lua, RUN_MEMO, None)?;
            let memory = bot.shares.memory.borrow();
            let mut key = bot.shares.world_key.borrow_mut();
            if let Some(key) = key.as_ref()
                && memory.world.lua_key_holds(key, &bot.snapshot)
                && let Some(table) = memo.raw_get::<Option<Table>>("world")?
            {
                return Ok(table);
            }
            let table = memory.world.to_lua(lua, &bot.snapshot)?;
            memo.raw_set("world", &table)?;
            *key = Some(memory.world.lua_key(&bot.snapshot));
            Ok(table)
        });

        methods.add_method("risk", |_lua, bot, ()| Ok(bot.risk));

        // `bot:param(name)`: the story parameter's value for this run. An
        // undeclared name is an error, so a typo in a route fails loudly.
        methods.add_method("param", |_lua, bot, name: String| {
            bot.params
                .get(&name)
                .map(str::to_string)
                .ok_or_else(|| mlua::Error::RuntimeError(format!("unknown bot parameter `{name}`")))
        });

        methods.add_run_method("knowledge", |lua, bot, ()| match &bot.knowledge {
            Some(knowledge) => knowledge.to_lua(lua),
            None => {
                let table = lua.create_table()?;
                table.set("enemies", lua.create_table()?)?;
                table.set("weapons", lua.create_table()?)?;
                Ok(table)
            }
        });

        methods.add_method("spacing_point", |lua, bot, (slot, want): (u8, i32)| {
            let enemies = &bot.snapshot.enemies;
            let Some(target) = enemies.iter().find(|enemy| enemy.slot == slot) else {
                return Ok(Value::Nil);
            };
            let others: Vec<(i32, i32)> = enemies
                .iter()
                .filter(|enemy| enemy.slot != slot)
                .map(|enemy| (enemy.x, enemy.z))
                .collect();
            let p = &bot.snapshot.player;
            let body = steer::Body {
                y: p.y,
                radius: p.radius,
                flags: p.collision_flags,
            };
            let nav = bot.shares.nav.borrow();
            let Some((x, z)) = steer::spacing_goal(
                &nav.collision,
                body,
                (p.x, p.z),
                (target.x, target.z),
                &others,
                want,
            ) else {
                return Ok(Value::Nil);
            };
            let table = lua.create_table()?;
            table.set("x", x)?;
            table.set("z", z)?;
            Ok(Value::Table(table))
        });

        methods.add_method("goal_room", |_lua, bot, ()| {
            Ok(bot.goal_room.as_deref().map(str::to_owned))
        });

        methods.add_tick_method("player", |lua, bot, ()| {
            let p = &bot.snapshot.player;
            let table = lua.create_table()?;
            table.set("x", p.x)?;
            table.set("y", p.y)?;
            table.set("z", p.z)?;
            table.set("angle", p.angle)?;
            // The forward action probe (`probe_actions`): a zone press fires
            // only when this point is inside the zone.
            let (reach_x, reach_z) = crate::player::reach_offset(p.angle);
            table.set("reach_x", p.x + reach_x)?;
            table.set("reach_z", p.z + reach_z)?;
            table.set("health", p.health)?;
            table.set("max_health", p.max_health)?;
            table.set("state", p.state)?;
            table.set("being_attacked", p.being_attacked)?;
            table.set("locked", p.locked)?;
            table.set("equipped", p.equipped)?;
            table.set("collision_flags", p.collision_flags)?;
            table.set("grabbed", p.grabbed)?;
            table.set("poisoned", p.health_status & POISON != 0)?;
            table.set("seriously_poisoned", p.health_status & SERIOUS_POISON != 0)?;
            table.set("animation", p.animation)?;
            Ok(table)
        });

        methods.add_tick_method("enemies", |lua, bot, ()| {
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

        methods.add_tick_method("projectiles", |lua, bot, ()| {
            let table = lua.create_table()?;
            for (index, pos) in bot.snapshot.projectiles.iter().enumerate() {
                let row = lua.create_table()?;
                row.set("x", pos[0])?;
                row.set("y", pos[1])?;
                row.set("z", pos[2])?;
                table.set(index + 1, row)?;
            }
            Ok(table)
        });

        methods.add_tick_method("weapon", |lua, bot, ()| {
            let weapon = bot.snapshot.weapon;
            let table = lua.create_table()?;
            table.set("equipped", weapon.equipped)?;
            table.set("weapon", weapon.weapon)?;
            table.set("clip", weapon.clip)?;
            table.set("reserve", weapon.reserve)?;
            table.set("ammo", bot.snapshot.ammo_for(weapon.weapon))?;
            table.set("behavior", weapon.behavior)?;
            table.set("state", weapon.state)?;
            table.set("aim_state", weapon.aim_state)?;
            table.set("target", weapon.target)?;
            Ok(table)
        });

        methods.add_tick_method("scene", |lua, bot, ()| {
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
            table.set("prompt", scene.prompt)?;
            table.set("prompt_choice", if scene.prompt_yes { "yes" } else { "no" })?;
            table.set("pickup", scene.pickup)?;
            table.set("reader", scene.reader)?;
            table.set("frozen", scene.frozen)?;
            table.set("controls_locked", scene.controls_locked)?;
            table.set("controllable", scene.controllable)?;
            table.set("dead", scene.dead)?;
            table.set("ending", scene.ending)?;
            table.set("typewriter", scene.typewriter)?;
            if let Some(view) = &bot.snapshot.save_screen {
                table.set("save_screen", save_screen_table(lua, view)?)?;
            }
            if let Some(computer) = &bot.snapshot.computer {
                let view = lua.create_table()?;
                view.set("stage", computer.stage)?;
                view.set("sub", computer.sub)?;
                view.set("cell", computer.cell)?;
                view.set("menu_row", computer.menu_row)?;
                view.set("typed", computer.typed)?;
                table.set("computer", view)?;
            }
            if let Some(panel) = &bot.snapshot.panel {
                let view = lua.create_table()?;
                view.set("active", panel.active)?;
                view.set("x", panel.cursor.0)?;
                view.set("y", panel.cursor.1)?;
                view.set("lamps", lua.create_sequence_from(panel.lamps)?)?;
                table.set("panel", view)?;
            }
            Ok(table)
        });

        methods.add_tick_method("saves", |lua, bot, ()| {
            saves_table(lua, &bot.saves, Some(&bot.snapshot.saves))
        });

        methods.add_tick_method("actions", |lua, bot, ()| {
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
                if let Some(desk) = action.desk {
                    // The lock flag as a (bank, bit) pair for `bot:flag`.
                    row.set("lock_bank", game::BANK_LOCKS)?;
                    row.set("lock_bit", desk.lock_bit)?;
                    row.set("item_slot", desk.item_slot)?;
                    row.set("desk_item", desk.item)?;
                    row.set("desk_quantity", desk.quantity)?;
                    row.set("full", desk.full)?;
                }
                table.set(index + 1, row)?;
            }
            Ok(table)
        });

        methods.add_tick_method("cameras", |lua, bot, ()| {
            let table = lua.create_table()?;
            for (index, zone) in bot.snapshot.cameras.iter().enumerate() {
                let row = lua.create_table()?;
                row.set("from", zone.from)?;
                row.set("to", zone.to)?;
                row.set("x", zone.x)?;
                row.set("z", zone.z)?;
                table.set(index + 1, row)?;
            }
            Ok(table)
        });

        methods.add_tick_method("objects", |lua, bot, ()| {
            let table = lua.create_table()?;
            for (index, object) in bot.snapshot.objects.iter().enumerate() {
                let row = lua.create_table()?;
                row.set("slot", object.slot)?;
                row.set("x", object.x)?;
                row.set("z", object.z)?;
                row.set("half_x", object.half_x)?;
                row.set("half_z", object.half_z)?;
                row.set("pushable", object.pushable)?;
                table.set(index + 1, row)?;
            }
            Ok(table)
        });

        methods.add_tick_method("doors", |lua, bot, ()| {
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

        methods.add_tick_method("inventory", |lua, bot, ()| {
            let table = lua.create_table()?;
            for (index, stack) in bot.snapshot.inventory.iter().enumerate() {
                let row = lua.create_table()?;
                row.set("slot", index + 1)?;
                row.set("item", stack.id)?;
                row.set("quantity", stack.quantity)?;
                row.set("kind", bot.snapshot.item_kind(stack.id))?;
                table.set(index + 1, row)?;
            }
            Ok(table)
        });

        // The catalog kind, whether a pickup merges into a held stack, and
        // whether a USE restores health (false for a blue herb, the serum
        // or a red herb alone).
        methods.add_method("item_kind", |_lua, bot, item: u8| {
            let def = bot.snapshot.item_defs.get(item);
            let heals = def.heal.amount != crate::item_catalog::HealAmount::None;
            Ok((bot.snapshot.item_kind(item), def.merge, heals))
        });

        methods.add_method("ammo_for", |_lua, bot, weapon: u8| {
            Ok(bot.snapshot.ammo_for(weapon))
        });

        methods.add_tick_method("menu", |lua, bot, ()| {
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
                row.set("heals", item.heals)?;
                items.set(index + 1, row)?;
            }
            table.set("items", items)?;
            table.set("move_cursor", menu.move_cursor)?;
            if let Some(view) = &menu.item_box {
                let item_box = lua.create_table()?;
                item_box.set("mode", view.mode.as_str())?;
                item_box.set("box_cursor", view.box_cursor)?;
                item_box.set("cursor", view.cursor)?;
                item_box.set("sliding", view.sliding)?;
                let slots = lua.create_table()?;
                for (index, (id, quantity)) in view.slots.iter().enumerate() {
                    let row = lua.create_table()?;
                    row.set("slot", index)?;
                    row.set("item", *id)?;
                    row.set("quantity", *quantity)?;
                    row.set("kind", bot.snapshot.item_kind(*id))?;
                    row.set(
                        "heals",
                        bot.snapshot.item_defs.get(*id).heal.amount
                            != crate::item_catalog::HealAmount::None,
                    )?;
                    slots.set(index + 1, row)?;
                }
                item_box.set("items", slots)?;
                table.set("box", item_box)?;
            }
            if let Some(view) = &menu.examine {
                let examine = lua.create_table()?;
                examine.set("item", view.item)?;
                examine.set("yaw", view.yaw)?;
                examine.set("ready", view.ready)?;
                table.set("examine", examine)?;
            }
            Ok(table)
        });

        // What the menu's MOVE of inventory slot `slot` onto `target` (both
        // 0-based menu slots) would do, from the engine's recipe and effect
        // code on the snapshot's inventory; nothing is written.
        methods.add_method(
            "combine_result",
            |lua, bot, (slot, target): (usize, usize)| {
                combine_result_table(lua, &bot.snapshot, slot, target)
            },
        );

        // The engine's recipe for MOVE-ing item `cursor` onto item `target`
        // (`items::combine`), whatever is held: `{cursor, target, effect}`,
        // the ids the two slots become and the effect byte, or nil when the
        // pair does not mix.
        methods.add_method("recipe", |lua, _bot, (cursor, target): (u8, u8)| {
            let Some(record) = crate::items::combine(cursor, target) else {
                return Ok(Value::Nil);
            };
            let table = lua.create_table()?;
            table.set("cursor", record.new_cursor)?;
            table.set("target", record.new_target)?;
            table.set("effect", record.effect)?;
            Ok(Value::Table(table))
        });

        methods.add_method("can_see", |_lua, bot, slot: u8| {
            Ok(bot
                .snapshot
                .enemies
                .iter()
                .find(|enemy| enemy.slot == slot)
                .is_some_and(|enemy| enemy.visible))
        });

        methods.add_tick_method("memory", |lua, bot, ()| {
            let table = lua.create_table()?;
            for (key, value) in &bot.memory().values {
                table.set(key.clone(), value.clone())?;
            }
            Ok(table)
        });

        methods.add_method("visited", |_lua, bot, room: Value| {
            Ok(bot.memory().is_visited(&room))
        });

        methods.add_method("remember", |lua, bot, (key, value): (String, Value)| {
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
            // The tick's `memory` table is stale now.
            memo_table(lua, TICK_MEMO, Some(bot.tick))?.raw_set("memory", Value::Nil)?;
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

/// An app screen the bot drives outside a game session, as a player sees
/// it: the title (after a death returned there) or the title's load screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BotScreen {
    /// The title: its stage (`press`, `menu`, `fade_out`, `attract_fade`),
    /// the highlighted entry (1 NEW GAME, 2 LOAD GAME) and whether LOAD
    /// GAME is offered.
    Title {
        stage: String,
        selection: i64,
        any_saves: bool,
    },
    /// The load screen; `None` for a screen with no readable state.
    Load(Option<SaveScreenView>),
}

/// The Lua handle `on_bot_screen(bot)` gets on an app screen. Like
/// [`BotCtx`] it only reads (the screen, the save history, memory) and writes
/// pad/UI input.
struct BotScreenCtx {
    screen: BotScreen,
    saves: SaveLog,
    tick: u64,
    risk: f64,
    shares: BotShares,
}

impl BotScreenCtx {
    fn finish(&self, reason: &str) {
        if !self.shares.finished.get() {
            self.shares.finished.set(true);
            *self.shares.finish_reason.borrow_mut() = Some(reason.to_string());
            self.shares
                .memory
                .borrow_mut()
                .log(format!("tick {} screen: finished: {reason}", self.tick));
        }
    }
}

impl UserData for BotScreenCtx {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("tick", |_lua, bot, ()| Ok(bot.tick));
        methods.add_method("risk", |_lua, bot, ()| Ok(bot.risk));
        methods.add_method("screen", |lua, bot, ()| match &bot.screen {
            BotScreen::Title {
                stage,
                selection,
                any_saves,
            } => {
                let table = lua.create_table()?;
                table.set("kind", "title")?;
                table.set("stage", stage.as_str())?;
                table.set("selection", *selection)?;
                table.set("any_saves", *any_saves)?;
                Ok(table)
            }
            BotScreen::Load(view) => {
                let table = match view {
                    Some(view) => save_screen_table(lua, view)?,
                    None => lua.create_table()?,
                };
                table.set("kind", "load")?;
                table.set("readable", view.is_some())?;
                Ok(table)
            }
        });
        methods.add_method("saves", |lua, bot, ()| saves_table(lua, &bot.saves, None));
        methods.add_method("memory", |lua, bot, ()| {
            let table = lua.create_table()?;
            for (key, value) in &bot.shares.memory.borrow().values {
                table.set(key.clone(), value.clone())?;
            }
            Ok(table)
        });
        methods.add_method("log", |_lua, bot, text: String| {
            bot.shares
                .memory
                .borrow_mut()
                .log(format!("tick {} screen: {text}", bot.tick));
            Ok(())
        });
        methods.add_method("finish", |_lua, bot, reason: String| {
            bot.finish(&reason);
            Ok(())
        });
        methods.add_method("set_input", |_lua, bot, table: Table| {
            for pair in table.pairs::<String, bool>() {
                let (name, value) = pair?;
                if !bot.shares.input.borrow_mut().set(&name, value) {
                    return Err(mlua::Error::RuntimeError(format!(
                        "unknown bot input key `{name}`"
                    )));
                }
            }
            Ok(())
        });
        methods.add_method("pulse_confirm", |_lua, bot, ()| {
            bot.shares.input.borrow_mut().pulse_confirm();
            Ok(())
        });
    }
}

/// The sandboxed Lua VM and the `on_bot_tick` function it defines, plus the
/// optional `on_bot_screen` for app screens.
struct BotVm {
    lua: Lua,
    budget: Rc<crate::lua::Budget>,
    on_tick: Function,
    on_screen: Option<Function>,
}

impl BotVm {
    /// Load every `bot/**.lua` entry in lowercased-path order and look up the
    /// required `on_bot_tick` global.
    fn load(pack: &Pack) -> Result<Self> {
        let paths = chunk_paths(pack)?;
        let (lua, budget) = crate::lua::new_sandboxed_state()?;
        run_chunks(&lua, &budget, &paths, pack)?;
        let on_tick = find_on_tick(&lua)?;
        let on_screen = lua
            .globals()
            .get::<Option<Function>>("on_bot_screen")
            .map_err(|err| anyhow!("{err}"))?;
        Ok(Self {
            lua,
            budget,
            on_tick,
            on_screen,
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
        self.on_screen = self
            .lua
            .globals()
            .get::<Option<Function>>("on_bot_screen")
            .map_err(|err| anyhow!("{err}"))?;
        Ok(())
    }

    /// Run `on_bot_screen(bot)`; `false` when the route defines none.
    fn call_screen(&self, ctx: BotScreenCtx) -> Result<bool> {
        let Some(on_screen) = &self.on_screen else {
            return Ok(false);
        };
        self.budget.begin();
        self.lua
            .scope(|scope| {
                let bot = scope.create_userdata(ctx)?;
                on_screen.call::<()>(bot)
            })
            .map_err(|err| anyhow!("{err}"))?;
        Ok(true)
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
    /// Every room's `<use>` facts for one character (`true`: Jill), read
    /// from the pack on the first tick and again if the character changes.
    uses: Option<(bool, Rc<Vec<meta::RoomUse>>)>,
    /// Every room's `<holds>` facts, loaded with `uses`.
    holds: Rc<Vec<meta::RoomHolds>>,
    /// Every room's `<fight>` facts, loaded with `uses`.
    fights: Rc<Vec<meta::RoomFight>>,
    /// Every room's `<ride>` facts, loaded with `uses`.
    rides: Rc<Vec<meta::RoomRide>>,
    /// Every room's `<revisit>` facts, loaded with `uses`.
    revisits: Rc<Vec<meta::RoomRevisit>>,
    /// Every room's `<reach>` facts, loaded with `uses`.
    reaches: Rc<Vec<meta::RoomReach>>,
    /// Every room's errand facts, loaded with `uses`.
    errands: Rc<Vec<meta::RoomErrand>>,
    /// Every room's `<brew>` facts, loaded with `uses`.
    brews: Rc<Vec<meta::RoomBrew>>,
    /// Every room's `<avoid>` facts, loaded with `uses`.
    avoid_facts: Rc<Vec<meta::RoomAvoid>>,
    /// Every room's `<door>` facts, loaded with `uses`.
    door_vias: Rc<Vec<meta::RoomDoorVia>>,
    /// `knowledge/<game>.xml`, read with the chunks.
    knowledge: Option<Rc<knowledge::Knowledge>>,
    /// The `--bot-risk` setting; kept across [`Self::reset`].
    risk: f64,
    /// The story parameters: declared in `params/<game>.xml`, set by
    /// `--bot-param`; kept across [`Self::reset`].
    params: Rc<params::Params>,
    /// The story flag banks (scenario, common) as the last tick saw them:
    /// each bit that changes is logged (`story: ...`), the coverage record
    /// of which scenes and branches a run played.
    story: Option<[game::FlagBank; STORY_BANKS]>,
    /// The story flags a tick of the run saw become set, per bank: the
    /// scenes the run reached, whatever cleared them later.
    story_seen: [BTreeSet<u8>; STORY_BANKS],
    /// The `--bot-goal-room` setting; kept across [`Self::reset`].
    goal_room: Option<Rc<str>>,
    /// Run statistics for `--bot-stats`.
    stats: stats::BotStats,
    /// Saves, reloads and deaths; kept across a reload.
    save_log: SaveLog,
    /// The session's `saves_written` already counted.
    seen_saves_written: u32,
    /// Whether the player was dead on the previous tick.
    was_dead: bool,
    /// The bot's memory as it stood when the latest save was written, put
    /// back when that save is loaded: what the bot saw after the save did
    /// not happen in the loaded game.
    checkpoint: Option<BotMemory>,
    /// The bot drove an app screen (the title or the load screen) since the
    /// last session started, so the next session is its reload.
    screen_driven: bool,
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
            uses: None,
            holds: Rc::default(),
            fights: Rc::default(),
            rides: Rc::default(),
            revisits: Rc::default(),
            reaches: Rc::default(),
            errands: Rc::default(),
            brews: Rc::default(),
            door_vias: Rc::default(),
            avoid_facts: Rc::default(),
            knowledge: knowledge::load(pack)?.map(Rc::new),
            risk: DEFAULT_RISK,
            params: Rc::new(params::load(pack, &[])?),
            story: None,
            story_seen: Default::default(),
            goal_room: None,
            stats: stats::BotStats::default(),
            save_log: SaveLog::default(),
            seen_saves_written: 0,
            was_dead: false,
            checkpoint: None,
            screen_driven: false,
        })
    }

    /// Set the room whose entry ends the run (`--bot-goal-room`). The id is
    /// stored uppercased, the way `bot:room().id` spells it.
    pub fn with_goal_room(mut self, room: Option<&str>) -> Self {
        self.goal_room = room.map(|room| Rc::from(room.to_ascii_uppercase()));
        self
    }

    /// Log each story flag (scenario and common banks) that changed since the
    /// last tick: `story: scenario 0x2B set`. The first tick of a session
    /// only records the banks. A reload logs the bits the loaded save
    /// differs in, so a run's union of `set` lines is the scenes it reached.
    fn observe_story(&mut self, snapshot: &SessionSnapshot) {
        let now: [game::FlagBank; STORY_BANKS] = std::array::from_fn(|bank| snapshot.flags[bank]);
        if let Some(before) = &self.story {
            let room = snapshot.room.room3();
            for (bank, name) in STORY_BANK_NAMES.iter().enumerate() {
                if before[bank] == now[bank] {
                    continue;
                }
                for sel in 0..=u8::MAX {
                    let (was, is) = (before[bank].bit(sel), now[bank].bit(sel));
                    if was != is {
                        if is {
                            self.story_seen[bank].insert(sel);
                        }
                        let what = if is { "set" } else { "cleared" };
                        self.shares.memory.borrow_mut().log(format!(
                            "tick {} room {room}: story: {name} {sel:#04X} {what}",
                            self.tick
                        ));
                    }
                }
            }
        }
        self.story = Some(now);
    }

    /// Set the story parameters (`--bot-param NAME=VALUE`, repeatable): each
    /// name and value must be declared in the pack's `params/<game>.xml`.
    pub fn with_params(mut self, pack: &Pack, overrides: &[(String, String)]) -> Result<Self> {
        self.params = Rc::new(params::load(pack, overrides)?);
        Ok(self)
    }

    /// The run's story parameters, `name=value` in declaration order.
    pub fn params_summary(&self) -> String {
        self.params.summary()
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
        self.knowledge = knowledge::load(pack)?.map(Rc::new);
        self.vm.reload(pack)
    }

    /// Drop the cached room metadata so the next tick reparses it.
    pub fn reload_meta(&mut self) {
        self.meta = None;
        self.meta_room = None;
        self.uses = None;
    }

    /// Run one `on_bot_tick`, returning the input it asked for.
    ///
    /// The tick counter advances even when the script fails, so the error can
    /// name the exact tick and room.
    pub fn tick(&mut self, session: &GameSession, pack: &Pack) -> Result<BotInput> {
        let started = std::time::Instant::now();
        let input = self.tick_inner(session, pack);
        let micros = u32::try_from(started.elapsed().as_micros()).unwrap_or(u32::MAX);
        self.stats.note_compute(self.tick, micros);
        input
    }

    fn tick_inner(&mut self, session: &GameSession, pack: &Pack) -> Result<BotInput> {
        self.tick = self.tick.saturating_add(1);
        let room_id = session.loaded.id;
        if self.meta_room != Some(room_id) {
            self.meta = meta::load(pack, room_id, &self.params)?.map(Rc::new);
            self.meta_room = Some(room_id);
        }
        let jill = meta::jill(room_id.player_flag);
        if self.uses.as_ref().is_none_or(|(held, _)| *held != jill) {
            self.uses = Some((
                jill,
                Rc::new(meta::load_uses(pack, Some(jill), Some(&self.params))?),
            ));
            self.holds = Rc::new(meta::load_holds(pack, Some(jill), Some(&self.params))?);
            self.fights = Rc::new(meta::load_fights(pack, Some(jill), Some(&self.params))?);
            self.rides = Rc::new(meta::load_rides(pack, Some(jill), Some(&self.params))?);
            self.revisits = Rc::new(meta::load_revisits(pack, Some(jill), Some(&self.params))?);
            self.reaches = Rc::new(meta::load_reaches(pack, Some(jill), Some(&self.params))?);
            self.errands = Rc::new(meta::load_errands(pack, Some(jill), Some(&self.params))?);
            self.brews = Rc::new(meta::load_brews(pack, Some(jill), Some(&self.params))?);
            self.door_vias = Rc::new(meta::load_door_vias(pack, Some(jill), Some(&self.params))?);
            self.avoid_facts = Rc::new(meta::load_avoids(pack, Some(jill), Some(&self.params))?);
        }
        let started = std::time::Instant::now();
        let snapshot = SessionSnapshot::build(session);
        self.stats.note_phase(0, started.elapsed());
        let started = std::time::Instant::now();
        self.observe_story(&snapshot);
        self.stats.observe_snapshot(self.tick, &snapshot);
        let saved = self.observe_saves(&snapshot);
        {
            let mut input = self.shares.input.borrow_mut();
            input.reset();
            let mut nav = self.shares.nav.borrow_mut();
            nav.sync_room(session.loaded.id, &session.loaded.room);
            nav.sync_objects(
                &snapshot.objects,
                &keep_out_boxes(self.meta.as_deref(), &snapshot),
            );
            nav.begin_tick();
            let mut memory = self.shares.memory.borrow_mut();
            memory.observe(&snapshot);
            memory.world.observe(self.tick, &snapshot);
        }
        let ctx = BotCtx {
            snapshot,
            saves: self.save_log,
            meta: self.meta.clone(),
            uses: self
                .uses
                .as_ref()
                .map(|(_, uses)| uses.clone())
                .unwrap_or_default(),
            holds: self.holds.clone(),
            fights: self.fights.clone(),
            rides: self.rides.clone(),
            revisits: self.revisits.clone(),
            reaches: self.reaches.clone(),
            errands: self.errands.clone(),
            brews: self.brews.clone(),
            door_vias: self.door_vias.clone(),
            avoid_facts: self.avoid_facts.clone(),
            knowledge: self.knowledge.clone(),
            tick: self.tick,
            risk: self.risk,
            params: self.params.clone(),
            goal_room: self.goal_room.clone(),
            shares: self.shares.share(),
        };
        self.stats.note_phase(1, started.elapsed());
        let started = std::time::Instant::now();
        let called = self.vm.call(ctx);
        self.stats.note_phase(2, started.elapsed());
        let started = std::time::Instant::now();
        if let Err(err) = called {
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
        let mut input = *self.shares.input.borrow();
        // `BOT_BOOT_IDLE=N` (benchmark noise measurement only): no input for
        // the run's first N ticks, a behaviour-neutral shift of every later
        // tick (docs/bot-scoreboard.md, "noise floor").
        if self.tick < boot_idle_ticks() {
            input = BotInput::default();
        }
        self.stats.observe_input(&input);
        self.shares
            .memory
            .borrow_mut()
            .world
            .observe_input(self.tick, &input);
        self.shares.nav.borrow_mut().end_tick(&input);
        if saved {
            // After the route's tick, so what it remembered about the save
            // (its slot, room and tick) is part of the checkpoint.
            self.checkpoint = Some(self.shares.memory.borrow().clone());
        }
        self.stats.note_phase(3, started.elapsed());
        Ok(input)
    }

    /// Count a save the session's save screen completed and a new death.
    /// Returns whether a save was written since the last tick.
    fn observe_saves(&mut self, snapshot: &SessionSnapshot) -> bool {
        let dead = snapshot.scene.dead;
        if dead && !self.was_dead {
            self.save_log.deaths += 1;
            self.save_log.deaths_since_save += 1;
        }
        self.was_dead = dead;
        if snapshot.saves.written <= self.seen_saves_written {
            return false;
        }
        self.seen_saves_written = snapshot.saves.written;
        self.save_log.count += 1;
        self.save_log.deaths_since_save = 0;
        self.save_log.last = Some(SaveRecord {
            slot: snapshot.saves.slot,
            room: snapshot.room,
            tick: self.tick,
        });
        self.stats.note_save();
        true
    }

    /// Run one `on_bot_screen` on an app screen (the title or its load
    /// screen), returning the input it asked for. A route without the hook
    /// finishes the run, since nothing would drive the screen.
    pub fn tick_screen(&mut self, screen: BotScreen) -> Result<BotInput> {
        self.tick = self.tick.saturating_add(1);
        self.screen_driven = true;
        self.shares.input.borrow_mut().reset();
        let ctx = BotScreenCtx {
            screen,
            saves: self.save_log,
            tick: self.tick,
            risk: self.risk,
            shares: self.shares.share(),
        };
        match self.vm.call_screen(ctx) {
            Ok(true) => {}
            Ok(false) => {
                self.shares.finished.set(true);
                *self.shares.finish_reason.borrow_mut() =
                    Some("left the game session: the route has no on_bot_screen".to_string());
            }
            Err(err) => {
                self.shares.finished.set(true);
                if self.shares.finish_reason.borrow().is_none() {
                    *self.shares.finish_reason.borrow_mut() = Some(format!("Lua error: {err}"));
                }
                return Err(err).with_context(|| format!("bot screen tick {}", self.tick));
            }
        }
        let input = *self.shares.input.borrow();
        Ok(input)
    }

    /// Whether the run has a save the bot could load after a death.
    pub fn has_save(&self) -> bool {
        self.save_log.last.is_some()
    }

    /// A new session is starting: the bot's reload when it drove the load
    /// screen to it ([`Self::resume_after_load`]), a fresh run otherwise
    /// ([`Self::reset`]).
    pub fn start_session(&mut self, pack: &Pack) -> Result<()> {
        if std::mem::take(&mut self.screen_driven) {
            self.resume_after_load(pack)
        } else {
            self.reset(pack)
        }
    }

    /// Carry the run on in a session the bot loaded from its latest save:
    /// keep the tick count, the statistics and the save history, put the
    /// memory back as it was at that save (keeping the full room path), drop
    /// the navigation cache and rebuild the Lua VM, as a player who reloads
    /// keeps only what they knew when they saved.
    pub fn resume_after_load(&mut self, pack: &Pack) -> Result<()> {
        self.save_log.reloads += 1;
        self.stats.note_reload(self.was_dead);
        self.seen_saves_written = 0;
        self.was_dead = false;
        let room_path = std::mem::take(&mut self.shares.memory.borrow_mut().room_path);
        let mut memory = self.checkpoint.clone().unwrap_or_default();
        memory.keep_lessons(&self.shares.memory.borrow());
        memory.room_path = room_path;
        memory.current_room = None;
        let last = self.save_log.last;
        memory.log(format!(
            "tick {} reloaded the save {} (reload {}, {} death(s) since it)",
            self.tick,
            last.map_or("?".to_string(), |save| format!(
                "in slot {} written in room {} at tick {}",
                save.slot.map_or(-1, |slot| slot as i64),
                save.room.room3(),
                save.tick
            )),
            self.save_log.reloads,
            self.save_log.deaths_since_save
        ));
        *self.shares.memory.borrow_mut() = memory;
        *self.shares.nav.borrow_mut() = BotNav::default();
        self.shares.input.borrow_mut().reset();
        self.reload_meta();
        self.vm = BotVm::load(pack)?;
        Ok(())
    }

    /// Clear the remembered state and tick count for a new game, and rebuild
    /// the Lua VM from the pack so route globals (`BOT_STATE`) cannot leak
    /// across a restart.
    ///
    /// A fresh load failure returns the error. The caller drops the runtime
    /// rather than reusing the old VM's route state.
    pub fn reset(&mut self, pack: &Pack) -> Result<()> {
        self.shares = BotShares::default();
        self.story = None;
        self.tick = 0;
        self.stats = stats::BotStats::default();
        self.save_log = SaveLog::default();
        self.seen_saves_written = 0;
        self.was_dead = false;
        self.checkpoint = None;
        self.screen_driven = false;
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
    /// Whether a tick of the run saw story flag `sel` of `bank` (0 scenario,
    /// 1 common) become set.
    pub fn story_seen(&self, bank: usize, sel: u8) -> bool {
        self.story_seen
            .get(bank)
            .is_some_and(|seen| seen.contains(&sel))
    }

    pub fn stats_json(&self, outcome: &str, elapsed_secs: f64, run: RunFacts) -> serde_json::Value {
        let mut report = self.stats_report(outcome, elapsed_secs, run);
        let story: serde_json::Map<String, serde_json::Value> = STORY_BANK_NAMES
            .iter()
            .zip(&self.story_seen)
            .map(|(name, seen)| ((*name).to_string(), serde_json::json!(seen)))
            .collect();
        report["story"] = serde_json::Value::Object(story);
        report
    }

    fn stats_report(&self, outcome: &str, elapsed_secs: f64, run: RunFacts) -> serde_json::Value {
        self.stats.to_json(
            outcome,
            self.finish_reason().as_deref(),
            self.tick,
            elapsed_secs,
            run,
            &self.shares.profile.borrow(),
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
            "projectiles",
            "weapon",
            "scene",
            "actions",
            "doors",
            "objects",
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
            "param",
            "goal_room",
            "cameras",
            "saves",
            "knowledge",
            "spacing_point",
            "item_kind",
            "ammo_for",
            "use_facts",
            "item_facts",
            "fight_facts",
            "ride_facts",
            "revisit_facts",
            "reach_facts",
            "errand_facts",
            "brew_facts",
            "door_facts",
            "avoid_facts",
            "combine_result",
            "recipe",
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
        // The removed shortcuts into game handlers must never come back, and
        // a save is loaded through the load screen, never by a call.
        const FORBIDDEN: &[&str] = &[
            "use_item",
            "use_heal",
            "equip",
            "use_action",
            "use_stair",
            "load",
            "load_slot",
            "save",
        ];

        let lua = Lua::new();
        // The app-screen handle may only read and press.
        let proxy = lua
            .create_proxy::<BotScreenCtx>()
            .expect("BotScreenCtx proxy");
        let index: Table = proxy
            .metatable()
            .expect("BotScreenCtx metatable")
            .get("__index")
            .expect("BotScreenCtx methods are an __index table");
        let mut screen_names = Vec::new();
        for pair in index.pairs::<String, Value>() {
            screen_names.push(pair.expect("method entry").0);
        }
        screen_names.sort();
        assert_eq!(
            screen_names,
            [
                "finish",
                "log",
                "memory",
                "pulse_confirm",
                "risk",
                "saves",
                "screen",
                "set_input",
                "tick"
            ],
            "on_bot_screen's handle reads, keeps the bot's bookkeeping and presses"
        );
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
    fn combine_result_reports_the_engine_recipe_and_whether_it_frees_a_slot() {
        let lua = Lua::new();
        let mut snapshot = world::tests::snapshot(0x100, 0, 0);
        let herb = |id| InventoryItem { id, quantity: 1 };
        snapshot.inventory = vec![
            herb(game::HERB_MIN + 1),
            herb(game::HERB_MIN),
            InventoryItem {
                id: 0x02,
                quantity: 15,
            },
            herb(game::HERB_MIN + 1),
        ];
        let row = |slot, target| combine_result_table(&lua, &snapshot, slot, target).unwrap();
        // Green onto red is a herb mix: the menu asks first, and one slot
        // empties.
        let mix = row(0, 1);
        assert_eq!(mix.get::<String>("result").unwrap(), "herb");
        assert!(mix.get::<bool>("frees").unwrap());
        // Green onto green mixes too.
        let greens = row(0, 3);
        assert_eq!(greens.get::<String>("result").unwrap(), "herb");
        assert!(greens.get::<bool>("frees").unwrap());
        // A herb onto the handgun has no recipe.
        let none = row(0, 2);
        assert_eq!(none.get::<String>("result").unwrap(), "no_recipe");
        assert!(!none.get::<bool>("frees").unwrap());
        // A slot onto itself, or past the inventory, is no recipe.
        assert_eq!(row(1, 1).get::<String>("result").unwrap(), "no_recipe");
        assert_eq!(row(0, 7).get::<String>("result").unwrap(), "no_recipe");
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
        spawn(3, 0x00, -1, 500, true);
        spawn(4, 0x20, 100, 200, true);
        spawn(5, 0x00, 100, 2000, false);
        // Health 0 is still alive: a monster dies below zero.
        spawn(6, 0x00, 0, 4000, true);

        let enemies = enemy_snapshots(&game, [0, 0, 0], &room);
        assert_eq!(
            enemies.iter().map(|enemy| enemy.slot).collect::<Vec<_>>(),
            [2, 1, 6],
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
    fn desk_snapshot_reads_the_lock_bit_item_slot_and_contents() {
        let mut game = game::GameState::default();
        // A drawer on slot 0 (lock bit 5, item slot 1, camera 3) holding
        // seven of item 0x0C behind room-items bit 9.
        let desk = game::RoomAction {
            slot: 0,
            kind: RoomActionKind::Desk,
            zone: [100, 100, 10, 10],
            sce: 0x0E,
            handler: 0x0E,
            flags: 0x81,
            params: [0, 0, 5, 0, 1, 0, 3, 0],
            item_data: None,
            room_items_flag: 0xFF,
            reach_animation: false,
        };
        game.room_actions[0] = Some(desk);
        game.room_actions[1] = Some(game::RoomAction {
            slot: 1,
            kind: RoomActionKind::Item,
            zone: [0, 0, 10, 10],
            sce: 2,
            handler: 2,
            flags: 0x00,
            params: [0x0C, 7, 0, 0, 0, 0, 0, 0],
            item_data: None,
            room_items_flag: 9,
            reach_animation: false,
        });
        game.apply_flag(7, 9, 0);
        let snap = desk_snapshot(&game, &desk).unwrap();
        assert_eq!(
            snap,
            DeskSnapshot {
                lock_bit: 5,
                item_slot: 1,
                item: Some(0x0C),
                quantity: Some(7),
                full: true,
            }
        );
        // Taken: the room-items bit clears and the drawer reads empty.
        game.mark_item_taken(9);
        assert!(!desk_snapshot(&game, &desk).unwrap().full);
        // No item action on the slot: no contents.
        game.room_actions[1] = None;
        let snap = desk_snapshot(&game, &desk).unwrap();
        assert_eq!((snap.item, snap.full), (None, false));
        // Any other kind has no drawer fields.
        let item = game::RoomAction {
            kind: RoomActionKind::Item,
            ..desk
        };
        assert_eq!(desk_snapshot(&game, &item), None);
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
    fn reload_keeps_the_bosses_met_only() {
        let mut checkpoint = BotMemory::default();
        checkpoint.remember("steps_done".to_string(), Some("1".to_string()));
        let mut current = BotMemory::default();
        current.remember("steps_done".to_string(), Some("4".to_string()));
        current.remember("fight_met:30C:04".to_string(), Some("1".to_string()));
        checkpoint.keep_lessons(&current);
        assert_eq!(
            checkpoint.values.get("steps_done").map(String::as_str),
            Some("1")
        );
        assert_eq!(
            checkpoint
                .values
                .get("fight_met:30C:04")
                .map(String::as_str),
            Some("1")
        );
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

    /// Two walls leaving an east-west corridor whose free band of body
    /// centres (100 radius plus the 18-unit skin) is `z_lo + 118 ..
    /// z_hi - 118`: the only way from the west half to the east half.
    fn corridor_room(z_lo: u16, z_hi: u16) -> RoomState {
        let mut room = RoomState::default();
        room.collision.quadrants[0].push(wall(8000, 12000, 0, z_lo));
        room.collision.quadrants[0].push(wall(8000, 12000, z_hi, 32768));
        room
    }

    fn assert_path_clear(room: &RoomState, start: (i32, i32), path: &[(i32, i32)]) {
        let mut from = start;
        for &point in path {
            assert!(
                !blocked(room, point.0, point.1, 0, 100, 0),
                "path point {point:?} is inside the collision: {path:?}"
            );
            assert!(
                line_clear(room, from, point, 0, 100, 0),
                "path leg {from:?} -> {point:?} crosses the collision: {path:?}"
            );
            from = point;
        }
    }

    #[test]
    fn fine_fallback_finds_a_corridor_between_coarse_rows() {
        // Free centres z 3218..3432: no 512-unit row (3072, 3584) lies in
        // the band, so the coarse lattice sees the halves as separate.
        let room = corridor_room(3100, 3550);
        let (start, goal) = ((4000, 16000), (16000, 16000));
        assert!(find_grid_path(&room, start, goal, 0, 100, 0).is_none());
        let fine = RefCell::new(None);
        let path = find_path(&room, &fine, start, goal, 0, 100, 0, &[])
            .expect("the fine lattice finds the corridor");
        assert_eq!(path.last().copied(), Some(goal));
        assert!(
            path.iter()
                .any(|&(x, z)| (8000..=12000).contains(&x) && (3218..=3432).contains(&z)),
            "the path must run through the corridor: {path:?}"
        );
        assert_path_clear(&room, start, &path);
        // The smoothed walk path stays clear too.
        let smooth = smooth_path(&room, start, path, 0, 100, 0, &[]);
        assert_path_clear(&room, start, &smooth);
        assert!(fine.borrow().is_some(), "the fine lattice is cached");
    }

    #[test]
    fn fine_fallback_never_squeezes_through_a_gap_narrower_than_the_body() {
        // A 200-unit gap: narrower than the 236-unit body plus skin.
        let room = corridor_room(3100, 3300);
        let fine = RefCell::new(None);
        assert!(find_path(&room, &fine, (4000, 16000), (16000, 16000), 0, 100, 0, &[]).is_none());
        // The same gap is open to a body that fits it (radius 60).
        assert!(find_path(&room, &fine, (4000, 16000), (16000, 16000), 0, 60, 0, &[]).is_some());
    }

    #[test]
    fn smoothing_steps_out_of_a_start_inside_the_body_margin() {
        // A scripted move (a stair) can leave the player overlapping the
        // conservative body margin: nothing is in sight from there, so the
        // smoothed path starts at the search's nearest free node.
        let room = corridor_room(3100, 3550);
        let start = (7950, 16000);
        let goal = (4000, 16000);
        assert!(blocked(&room, start.0, start.1, 0, 100, 0));
        let raw = find_path(&room, &RefCell::new(None), start, goal, 0, 100, 0, &[])
            .expect("the search starts from the nearest free node");
        let path = smooth_path(&room, start, raw, 0, 100, 0, &[]);
        assert_eq!(path.last().copied(), Some(goal));
        assert_path_clear(&room, path[0], &path[1..]);
        assert!(!blocked(&room, path[0].0, path[0].1, 0, 100, 0));
    }

    #[test]
    fn fine_fallback_reaches_a_dead_end_narrower_than_the_step() {
        // A north-south dead end whose free centres x 10118..10182 hold no
        // 128-unit lattice column (10112, 10240): 406's strip beside the
        // crate. The goal is reached through one bend on the strip's line.
        let mut room = RoomState::default();
        room.collision.quadrants[0].push(wall(0, 10000, 9000, 32768));
        room.collision.quadrants[0].push(wall(10300, 32768, 9000, 32768));
        let (start, goal) = ((4000, 4000), (10150, 11000));
        let fine = RefCell::new(None);
        let path = find_path(&room, &fine, start, goal, 0, 100, 0, &[])
            .expect("the bend reaches the dead end");
        assert_eq!(path.last().copied(), Some(goal));
        assert_path_clear(&room, start, &path);
        let smooth = smooth_path(&room, start, path, 0, 100, 0, &[]);
        assert_eq!(smooth.last().copied(), Some(goal));
        assert_path_clear(&room, start, &smooth);
    }

    #[test]
    fn fine_fallback_rejects_a_blocked_goal_and_another_part() {
        let room = corridor_room(3100, 3300);
        let fine = RefCell::new(None);
        // Inside the wall itself.
        assert!(find_path(&room, &fine, (4000, 16000), (10000, 16000), 0, 100, 0, &[]).is_none());
        // A free goal in the sealed east half.
        assert!(find_path(&room, &fine, (4000, 16000), (16000, 9000), 0, 100, 0, &[]).is_none());
        // A free goal in the start's own half still plans.
        assert!(find_path(&room, &fine, (4000, 16000), (4000, 9000), 0, 100, 0, &[]).is_some());
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
            locked: false,
            equipped: None,
            radius: 100,
            collision_flags: 0,
            grabbed: false,
            health_status: 0,
            animation: 0,
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
            locked: false,
            equipped: None,
            radius: 100,
            collision_flags: 0,
            grabbed: false,
            health_status: 0,
            animation: 0,
        }
    }

    #[test]
    fn a_collision_edit_after_the_first_tick_reaches_the_plan() {
        let id = RoomId {
            stage: 3,
            room: 0x00,
            player_flag: 1,
            mansion2: false,
        };
        let wall = crate::state::CollisionRect {
            x_max: 9000,
            z_max: 9000,
            x_min: 7000,
            z_min: 7000,
            kind: 1,
            flags: 0,
        };
        let mut room = RoomState::default();
        for quadrant in &mut room.collision.quadrants {
            quadrant.push(wall);
        }
        let mut nav = BotNav::default();
        nav.sync_room(id, &room);
        assert!(blocked(&nav.collision, 8000, 8000, 0, 100, 0));
        // The init's `inst_cfg` opens the wall on a later tick in the same
        // room (300's elevator shaft): the plan follows the live room.
        for quadrant in &mut room.collision.quadrants {
            quadrant.clear();
        }
        nav.sync_room(id, &room);
        assert!(!blocked(&nav.collision, 8000, 8000, 0, 100, 0));
    }

    #[test]
    fn solid_room_objects_block_the_plan_and_follow_a_push() {
        let mut nav = BotNav::default();
        nav.sync_room(
            RoomId {
                stage: 2,
                room: 0x05,
                player_flag: 1,
                mansion2: false,
            },
            &RoomState::default(),
        );
        let object = |x: i32, solid: bool| ObjectSnapshot {
            slot: 1,
            x,
            z: 8000,
            half_x: 450,
            half_z: 450,
            pushable: solid,
            solid,
        };
        let centre = (8000, 8000);
        assert!(!blocked(&nav.collision, centre.0, centre.1, 0, 100, 0));
        nav.sync_objects(&[object(8000, true)], &[]);
        assert!(blocked(&nav.collision, centre.0, centre.1, 0, 100, 0));
        // The box is inset, so the floor just past the object's edge (inside
        // the inset plus the body) stays free.
        assert!(!blocked(
            &nav.collision,
            8000 + 450 - OBJECT_NAV_INSET + 120,
            8000,
            0,
            100,
            0
        ));
        // A pushed object moves its box; an intangible one has none.
        nav.sync_objects(&[object(5000, true)], &[]);
        assert!(!blocked(&nav.collision, centre.0, centre.1, 0, 100, 0));
        assert!(blocked(&nav.collision, 5000, 8000, 0, 100, 0));
        nav.sync_objects(&[object(5000, false)], &[]);
        assert!(!blocked(&nav.collision, 5000, 8000, 0, 100, 0));
        // A keep-out zone blocks the floor like a solid box.
        nav.sync_objects(&[], &[(12000, 8000, 400, 400)]);
        assert!(blocked(&nav.collision, 12000, 8000, 0, 100, 0));
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
            locked: false,
            equipped: None,
            radius: 100,
            collision_flags: 0,
            grabbed: false,
            health_status: 0,
            animation: 0,
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
                local g = m.avoids[2]
                local u = m.uses[1]
                local wa = m.watches[1]
                local w = bot:world()
                RESULT = string.format("%s|%d|%d|%d|%d|%s|%s|%s|%s|%d|%s|%s|%d|%g|%s|%d:%d:%d:%d:%s|%s:%d:%d:%d:%s|%d:%d:%d:%d:%s:%d",
                    m.format, #m.puzzles, i.slot, i.bank, i.bit, i.label,
                    a.choice, tostring(a.slot), tostring(a.bank), v.slot, v.reason,
                    w.room, #w.frontier, bot:risk(), tostring(bot:goal_room()),
                    u.slot, u.item, u.bank, u.bit, u.label,
                    tostring(v.bank), g.slot, g.bank, g.bit, g["while"],
                    wa.bank, wa.bit, wa.after_bank, wa.after_bit, wa.label, #m.watches)
            end
"#,
        )
        .unwrap();
        let pack = Pack::open(&temp.path).unwrap();
        let vm = BotVm::load(&pack).unwrap();
        let meta = meta::parse(
            r#"<bot format="2">
                <interact slot="4" bank="1" bit="0x22" label="statue"/>
                <use slot="2" item="0x26" bank="1" bit="0xA9" label="pump"/>
                <answer choice="no" slot="6"/>
                <avoid slot="7" reason="plant"/>
                <avoid slot="9" bank="1" bit="0xA1" while="clear" reason="barry"/>
                <watch bank="1" bit="0xAD" after_bank="1" after_bit="0xAC" label="zombie"/>
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
            saves: SaveLog::default(),
            meta: Some(Rc::new(meta)),
            uses: Rc::default(),
            holds: Rc::default(),
            fights: Rc::default(),
            rides: Rc::default(),
            revisits: Rc::default(),
            reaches: Rc::default(),
            errands: Rc::default(),
            brews: Rc::default(),
            door_vias: Rc::default(),
            avoid_facts: Rc::default(),
            knowledge: None,
            tick: 1,
            risk: 0.6,
            params: Rc::default(),
            goal_room: Some(Rc::from("20D")),
            shares,
        })
        .unwrap();
        assert_eq!(
            vm.lua.globals().get::<String>("RESULT").unwrap(),
            "2|0|4|1|34|statue|no|6|nil|7|plant|117|1|0.6|20D|2:38:1:169:pump|nil:9:1:161:clear|1:173:1:172:zombie:1"
        );
    }

    /// The tables the API hands out are built once per tick (the knowledge
    /// once per run, the world once per change) and shared by every call.
    #[test]
    fn snapshot_tables_are_built_once_a_tick_and_the_world_once_a_change() {
        let temp = TempDir::new("memo");
        let bot = temp.path.join("bot");
        fs::create_dir_all(&bot).unwrap();
        fs::write(
            bot.join("init.lua"),
            br#"function on_bot_tick(bot)
                local same = bot:player() == bot:player() and bot:menu() == bot:menu()
                    and bot:knowledge() == bot:knowledge() and bot:world() == bot:world()
                    and bot:room_meta() == nil and bot:room_meta() == nil
                local before = bot:memory()
                bot:remember("key", tostring(bot:tick()))
                local after = bot:memory()
                RESULT = string.format("%s|%s|%s|%s|%s|%s|%d", tostring(same),
                    tostring(bot:player() == PLAYER), tostring(bot:knowledge() == KNOWLEDGE),
                    tostring(bot:world() == WORLD), tostring(before.key), after.key,
                    #bot:world().frontier)
                PLAYER, KNOWLEDGE, WORLD = bot:player(), bot:knowledge(), bot:world()
            end
"#,
        )
        .unwrap();
        let pack = Pack::open(&temp.path).unwrap();
        let vm = BotVm::load(&pack).unwrap();
        let shares = BotShares::default();
        let mut results = Vec::new();
        for tick in 1..=3 {
            let mut snapshot = world::tests::snapshot(0x117, 0, 0);
            if tick == 3 {
                // A door comes into view: the world changed.
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
            }
            shares.memory.borrow_mut().world.observe(tick, &snapshot);
            vm.call(BotCtx {
                snapshot,
                saves: SaveLog::default(),
                meta: None,
                uses: Rc::default(),
                holds: Rc::default(),
                fights: Rc::default(),
                rides: Rc::default(),
                revisits: Rc::default(),
                reaches: Rc::default(),
                errands: Rc::default(),
                brews: Rc::default(),
                door_vias: Rc::default(),
                avoid_facts: Rc::default(),
                knowledge: None,
                tick,
                risk: 0.25,
                params: Rc::default(),
                goal_room: None,
                shares: shares.share(),
            })
            .unwrap();
            results.push(vm.lua.globals().get::<String>("RESULT").unwrap());
        }
        assert_eq!(
            results,
            [
                // same in the tick | player | knowledge | world as last tick |
                // memory before and after remember | frontier
                "true|false|false|false|nil|1|0",
                "true|false|true|true|1|2|0",
                "true|false|true|false|2|3|1",
            ]
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

    /// The checked-in route, loaded from `mods/bot`.
    fn route_vm() -> BotVm {
        let pack = Pack::open(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mods/bot")).unwrap();
        BotVm::load(&pack).unwrap()
    }

    fn eval<T: mlua::FromLuaMulti>(vm: &BotVm, body: &str) -> T {
        vm.lua.load(body).eval::<T>().unwrap()
    }

    /// The route VM with the checked-in knowledge file as the global `K`.
    fn tactics_vm() -> BotVm {
        let pack = Pack::open(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mods/bot")).unwrap();
        let knowledge = knowledge::load(&pack)
            .unwrap()
            .expect("knowledge/re1.xml by the mod's base");
        assert_eq!(knowledge.source, "knowledge/re1.xml");
        let vm = BotVm::load(&pack).unwrap();
        let table = knowledge.to_lua(&vm.lua).unwrap();
        vm.lua.globals().set("K", table).unwrap();
        vm
    }

    #[test]
    fn aim_pose_follows_the_status_bit_the_engine_matches() {
        let vm = tactics_vm();
        let pose = |args: &str| -> String {
            eval(&vm, &format!("return tostring((BotTactics.pose({args})))"))
        };
        // A standing zombie (0x40, plus 0x20 inside 4500 and 0x80 inside
        // 3000): the handgun aims at the torso, never the legs.
        assert_eq!(
            pose("K.enemies[0], K.weapons[2], 0x40 | 0x20, 4000"),
            "torso"
        );
        assert_eq!(pose("K.enemies[0], K.weapons[2], 0xE0, 2000"), "torso");
        // A crawler shows only the low bit: shoot down.
        assert_eq!(pose("K.enemies[0], K.weapons[2], 0x20, 2500"), "down");
        // The shotgun inside its headshot distance aims up at a zombie
        // showing the high bit; past it, the torso.
        assert_eq!(pose("K.enemies[0], K.weapons[3], 0xE0, 2500"), "up");
        assert_eq!(pose("K.enemies[0], K.weapons[3], 0x60, 2500"), "torso");
        assert_eq!(pose("K.enemies[0], K.weapons[3], 0xE0, 3500"), "torso");
        // A hunter has no head to pop: the shotgun aims at its standing pose.
        assert_eq!(pose("K.enemies[6], K.weapons[3], 0xC0, 2500"), "torso");
        // The handgun never aims up at a zombie (detect_gun refuses it).
        assert_eq!(pose("K.enemies[0], K.weapons[2], 0x80, 2000"), "nil");
        // A crow high up shows only 0x80: the handgun may aim up at it.
        assert_eq!(pose("K.enemies[5], K.weapons[2], 0x80, 3000"), "up");
        // No aim bit at all (a ceiling spider, a crow in flight): no shot.
        assert_eq!(pose("K.enemies[3], K.weapons[2], 0x00, 3000"), "nil");
        // A zombie the room has not woken (still feeding) shows only 0x01:
        // no pose, so the target choice skips it.
        assert_eq!(pose("K.enemies[0x11], K.weapons[2], 0x01, 2900"), "nil");
        // A monster without a knowledge entry still gets the matching bit.
        assert_eq!(pose("nil, K.weapons[2], 0x40, 3000"), "torso");
        assert_eq!(pose("nil, K.weapons[2], 0x80, 3000"), "nil");
        assert_eq!(
            eval::<String>(
                &vm,
                "local i = BotTactics.aim_input('down') return tostring(i.aim and i.down)"
            ),
            "true"
        );
    }

    #[test]
    fn fight_or_run_follows_policy_health_rounds_and_the_room() {
        let vm = tactics_vm();
        vm.lua
            .load(
                r#"
                function verdict(over)
                    local o = { policy = "fight", gun = true, rounds = 30, needed = 5,
                        health = 96, max_health = 96, risk = 0.25, crossing = false,
                        distance = 5000, reach = 1500, outrun = true }
                    for k, v in pairs(over or {}) do o[k] = v end
                    local v, why = BotTactics.decide(o)
                    return v .. ":" .. why
                end
            "#,
            )
            .exec()
            .unwrap();
        let verdict = |over: &str| -> String { eval(&vm, &format!("return verdict({over})")) };
        // Only in the way: a slow zombie far off in a room not crossed again.
        assert_eq!(verdict("{}"), "run:only in the way");
        assert_eq!(
            verdict("{ crossing = true }"),
            "fight:the room is crossed again"
        );
        assert_eq!(verdict("{ distance = 2900 }"), "fight:closing in");
        // Closing in, it is shot with what is left; farther off, the rounds
        // must cover it.
        assert_eq!(
            verdict("{ distance = 2900, rounds = 2 }"),
            "fight:closing in"
        );
        assert_eq!(
            verdict("{ distance = 2900, rounds = 0 }"),
            "run:too few rounds"
        );
        assert_eq!(
            verdict("{ rounds = 2, crossing = true }"),
            "run:too few rounds"
        );
        // A dog cannot be outrun: it is fought wherever it is.
        assert_eq!(verdict("{ outrun = false }"), "fight:cannot outrun it");
        // The policy, knowledge and rounds come first.
        assert_eq!(
            verdict("{ policy = 'pass', outrun = false }"),
            "run:policy pass"
        );
        assert_eq!(
            verdict("{ policy = false }"),
            "run:not in the knowledge file"
        );
        assert_eq!(
            verdict("{ gun = false, outrun = false }"),
            "run:no firearm with rounds"
        );
        // Rounds: 1.5x the need at risk 0, 1x at risk 1.
        assert_eq!(
            verdict("{ rounds = 7, risk = 0, outrun = false }"),
            "run:too few rounds"
        );
        assert_eq!(
            verdict("{ rounds = 7, risk = 1, outrun = false }"),
            "fight:cannot outrun it"
        );
        assert_eq!(
            verdict("{ crowd_needed = 40, outrun = false }"),
            "run:too few rounds for the crowd"
        );
        // Health floor: 0.45 of max at risk 0, 0.15 at risk 1.
        assert_eq!(
            verdict("{ health = 40, risk = 0, outrun = false }"),
            "run:low health"
        );
        assert_eq!(
            verdict("{ health = 40, risk = 1, outrun = false }"),
            "fight:cannot outrun it"
        );
        // At low health a slow zombie closing in is run from; a hunter
        // (speed unknown) or a dog closing in is shot.
        assert_eq!(
            verdict("{ health = 30, distance = 2900 }"),
            "run:low health"
        );
        assert_eq!(
            verdict("{ health = 30, distance = 2900, outrun = 'unknown' }"),
            "fight:closing in, cannot outrun it"
        );
        // One that hurt the bot on an earlier pass is fought with the bare
        // kill's rounds (5), no margin; fewer still run.
        assert_eq!(
            verdict("{ rounds = 5, crossing = true, hurt_before = true }"),
            "fight:it hurt the bot on an earlier pass"
        );
        assert_eq!(
            verdict("{ rounds = 4, crossing = true, hurt_before = true }"),
            "run:too few rounds"
        );
    }

    #[test]
    fn spacing_backs_off_reloads_far_and_steps_into_the_headshot() {
        let vm = tactics_vm();
        let space = |args: &str| -> String {
            eval(
                &vm,
                &format!("local m, d = BotTactics.spacing({args}) return m .. ':' .. d"),
            )
        };
        // The zombie's handgun band is 2000..4000. Inside its near edge the
        // bot holds and shoots: leaving the aim drifts it into the grab.
        assert_eq!(
            space("{ distance = 1600, min = 2000, max = 4000 }"),
            "hold:1600"
        );
        assert_eq!(
            space("{ distance = 2500, min = 2000, max = 4000 }"),
            "hold:2500"
        );
        assert_eq!(
            space("{ distance = 6000, min = 2000, max = 4000 }"),
            "hold:6000"
        );
        // Reloading backs off to the far edge of the band first.
        assert_eq!(
            space("{ distance = 2500, min = 2000, max = 4000, reloading = true }"),
            "back:4000"
        );
        assert_eq!(
            space("{ distance = 4200, min = 2000, max = 4000, reloading = true }"),
            "hold:4200"
        );
        // A zombie (reach 1500, 45 a tick) closes 945 while the bot turns
        // around (21 ticks): too close to back off, so it holds and shoots.
        assert_eq!(
            space("{ distance = 1900, min = 2000, max = 4000, reach = 1500, speed = 45 }"),
            "hold:1900"
        );
        assert_eq!(
            space(
                "{ distance = 3000, min = 2000, max = 4000, reach = 1500, speed = 45, reloading = true }"
            ),
            "back:4000"
        );
        assert_eq!(
            space(
                "{ distance = 2300, min = 2000, max = 4000, reach = 1500, speed = 45, reloading = true }"
            ),
            "hold:2300"
        );
        // A hand weapon (the knife, band 0..1300) walks in from past the
        // band to its middle and holds inside it.
        assert_eq!(
            space("{ distance = 2000, min = 0, max = 1300, melee = true }"),
            "close:650"
        );
        assert_eq!(
            space("{ distance = 1100, min = 0, max = 1300, melee = true }"),
            "hold:1100"
        );
        // Inside the reach (a hunter's swipe starts at 3000), back-offs stop.
        assert_eq!(
            space("{ distance = 1300, min = 3200, max = 6000, reach = 3000 }"),
            "hold:1300"
        );
        // A crawler reaches 2200: the near edge moves out to 2500, and the
        // bot holds there too.
        assert_eq!(
            space("{ distance = 2300, min = 2000, max = 4000, crawl_reach = 2200 }"),
            "hold:2300"
        );
        // The shotgun (band 1800..2800, headshot 3000) steps in from just
        // past the band, but not from across the room.
        assert_eq!(
            space("{ distance = 3200, min = 1800, max = 2800, headshot = 3000 }"),
            "close:2300"
        );
        assert_eq!(
            space("{ distance = 5000, min = 1800, max = 2800, headshot = 3000 }"),
            "hold:5000"
        );
        // Past the band but already inside the headshot: shoot, do not walk.
        assert_eq!(
            space("{ distance = 2900, min = 1800, max = 2800, headshot = 3000 }"),
            "hold:2900"
        );
        // The backward walk (0x40 a tick) outpaces a zombie (45): inside the
        // near edge the bot still holds and shoots; a reload backpedals,
        // still facing it, to the far edge. Inside its reach it shoots.
        assert_eq!(
            space(
                "{ distance = 1900, min = 2000, max = 4000, reach = 1500, speed = 45, back_speed = 0x40 }"
            ),
            "hold:1900"
        );
        assert_eq!(
            space(
                "{ distance = 2500, min = 2000, max = 4000, reach = 1500, speed = 45, back_speed = 0x40, reloading = true }"
            ),
            "retreat:4000"
        );
        assert_eq!(
            space(
                "{ distance = 1400, min = 2000, max = 4000, reach = 1500, speed = 45, back_speed = 0x40 }"
            ),
            "hold:1400"
        );
        assert_eq!(
            space(
                "{ distance = 2500, min = 2000, max = 4000, reach = 1500, speed = 45, back_speed = 0x40 }"
            ),
            "hold:2500"
        );
        // A dog (240) or a hunter (speed unknown) is not backpedalled from.
        assert_eq!(
            space(
                "{ distance = 1200, min = 1500, max = 5000, reach = 1000, speed = 240, back_speed = 0x40 }"
            ),
            "hold:1200"
        );
        assert_eq!(
            space("{ distance = 3100, min = 3200, max = 6000, reach = 3000, back_speed = 0x40 }"),
            "hold:3100"
        );
    }

    #[test]
    fn dormancy_counts_only_until_the_monster_first_shows_an_aim_bit() {
        let vm = tactics_vm();
        let ticks: Vec<i64> = eval(
            &vm,
            r#"
            local s, e, out = { now = 0 }, { slot = 1 }, {}
            local function at(now, shootable)
                s.now = now
                out[#out + 1] = BotTactics.no_aim_ticks(s, e, shootable)
            end
            at(100, false) at(101, false) at(115, false)
            -- Unseen for more than a second: the count restarts.
            at(200, false) at(210, false)
            -- Woken: a later gap (a dog in mid-leap) is not dormancy.
            at(211, true) at(212, false) at(240, false)
            return out
            "#,
        );
        assert_eq!(ticks, vec![0, 1, 15, 0, 10, 0, 0, 0]);
    }

    #[test]
    fn retreat_spot_must_lie_behind_the_player() {
        let vm = tactics_vm();
        let behind =
            |args: &str| -> bool { eval(&vm, &format!("return BotTactics.behind({args})")) };
        // The player at the origin, the target 2000 along +X.
        assert!(behind("0, 0, 2000, 0, -1000, 0"));
        assert!(behind("0, 0, 2000, 0, -1000, 600"));
        // Sideways (a wall straight behind) or towards the target: no.
        assert!(!behind("0, 0, 2000, 0, -300, 1000"));
        assert!(!behind("0, 0, 2000, 0, 0, 1000"));
        assert!(!behind("0, 0, 2000, 0, 1000, 0"));
        assert!(!behind("0, 0, 2000, 0, 0, 0"));
    }

    #[test]
    fn a_brew_mixes_what_is_held_before_it_fills_and_picks_the_cheaper_recipe() {
        let vm = route_vm();
        // A stub bot over the drug store's chemistry (the ids of
        // `items::combine`): water 0x14, UMB No.2 0x15, UMB No.4 0x16,
        // UMB No.7 0x17, UMB No.13 0x18, Yellow-6 0x19, NP-003 0x1A,
        // V-JOLT 0x1B; a mix leaves the empty bottle 0x13 in the cursor slot.
        let step = |held: &str| -> String {
            eval(
                &vm,
                &format!(
                    r#"local mixes = {{ ["20,21"] = 26, ["21,22"] = 25, ["20,25"] = 23,
                        ["22,26"] = 23, ["23,25"] = 24, ["24,26"] = 27 }}
                    local held = {{ {held} }}
                    local bot = {{}}
                    function bot:room_meta()
                        return {{ taps = {{ {{ slot = 8, item = 20 }}, {{ slot = 11, item = 21 }},
                            {{ slot = 12, item = 22 }} }} }}
                    end
                    function bot:recipe(a, b)
                        local lo, hi = a, b
                        if lo > hi then lo, hi = hi, lo end
                        local made = mixes[lo .. "," .. hi]
                        return made and {{ cursor = 19, target = made, effect = 0 }} or nil
                    end
                    function bot:inventory()
                        local out = {{}}
                        for i, item in ipairs(held) do out[i] = {{ item = item }} end
                        return out
                    end
                    function bot:menu()
                        local out = {{}}
                        for i, item in ipairs(held) do out[i] = {{ item = item, slot = i - 1 }} end
                        return {{ items = out }}
                    end
                    function bot:has_item(item)
                        for _, h in ipairs(held) do if h == item then return true end end
                        return false
                    end
                    local op, why = BotMix.next_step(bot, {{ item = 27, bottle = 19 }})
                    if not op then return "wait: " .. why end
                    if op.kind == "fill" then return "fill " .. op.item end
                    local lo, hi = op.a, op.b
                    if lo > hi then lo, hi = hi, lo end
                    return string.format("mix %d+%d", lo, hi)"#
                ),
            )
        };
        // Nothing held but a bottle: fill first.
        assert!(step("19").starts_with("fill "), "{}", step("19"));
        // NP-003, Yellow-6, UMB No.4 and water held, no bottle: the plan
        // still needs one fill, and a mix of two held chemicals into UMB
        // No.7 goes first, which also empties a bottle for it.
        let ready = step("26, 25, 22, 20");
        assert!(
            ["mix 20+25", "mix 22+26"].contains(&ready.as_str()),
            "{ready}"
        );
        // Neither a bottle nor a ready mix: wait.
        assert_eq!(step("26"), "wait: no empty bottle held");
        // The last step.
        assert_eq!(step("24, 26"), "mix 24+26");
    }

    #[test]
    fn loop_rule_counts_stalls_and_room_repeats() {
        let vm = route_vm();
        // A pursued target: each room entry whose route is no shorter than
        // the shortest seen is a stall; the third stall is a loop.
        assert_eq!(
            eval::<String>(
                &vm,
                r#"local rec, out = {}, {}
                for _, steps in ipairs({ 4, 3, 2, 1 }) do
                    out[#out + 1] = tostring(BotPlanner.pursue(rec, steps))
                end
                return table.concat(out, ",") .. ":" .. rec.best .. ":" .. (rec.stalls or 0)"#
            ),
            "false,false,false,false:1:0"
        );
        assert_eq!(
            eval::<String>(
                &vm,
                r#"local rec, out = {}, {}
                for _, steps in ipairs({ 3, 2, 3, 2, 2 }) do
                    out[#out + 1] = tostring(BotPlanner.pursue(rec, steps))
                end
                return table.concat(out, ",")"#
            ),
            "false,false,false,false,true"
        );
        // Room repeats since the last real progress: entries 1..6 are fine,
        // 7..12 mark the pursued goal, the 13th finishes the run.
        assert_eq!(
            eval::<String>(
                &vm,
                r#"local counts, out = {}, {}
                for _ = 1, 13 do
                    local verdict, n = BotPlanner.repeat_check(counts, "R")
                    out[#out + 1] = verdict:sub(1, 1) .. n
                end
                BotPlanner.repeat_check(counts, "S")
                return table.concat(out, ",") .. ":" .. counts.S"#
            ),
            "o1,o2,o3,o4,o5,o6,l7,l8,l9,l10,l11,l12,f13:1"
        );
    }

    #[test]
    fn a_failed_item_approach_is_not_headed_back_for_remotely() {
        let vm = route_vm();
        // Arguments: mark kind, mark reason, inventory signature now, entry
        // count now, remote. The mark was made with signature "a" on entry 1.
        let holds = |kind: &str, reason: &str, sig: &str, entry: u32, remote: bool| -> bool {
            eval(
                &vm,
                &format!(
                    r#"local m = {{ kind = "{kind}", reason = "{reason}", sig = "a", entry = 1 }}
                    return BotPlanner.mark_holds(m, "{sig}", {entry}, {remote})"#
                ),
            )
        };
        let failed = "every approach failed";
        // An item out of reach stays off remote planning after the
        // inventory changes...
        assert!(holds("item", failed, "b", 1, true));
        assert!(holds("item", failed, "a", 2, true));
        // ...but is tried again in its room on a new entry or inventory.
        assert!(holds("item", failed, "a", 1, false));
        assert!(!holds("item", failed, "a", 2, false));
        assert!(!holds("item", failed, "b", 1, false));
        // A door keeps the inventory rule for routing.
        assert!(!holds("door", failed, "b", 1, true));
        assert!(holds("door", failed, "a", 2, true));
        // Any other mark lifts on an inventory change only.
        let full = "inventory full: 02";
        assert!(holds("item", full, "a", 2, true));
        assert!(!holds("item", full, "b", 1, true));
        assert!(holds("item", full, "a", 2, false));
    }

    #[test]
    fn the_nearest_item_box_in_this_room_is_chosen() {
        let vm = route_vm();
        // Two boxes in one room: the near one (by planned approach length)
        // wins whatever the action order; an unplanned one comes last.
        let pick = |rows: &str| -> String {
            eval(
                &vm,
                &format!(
                    r#"local rows = {{ {rows} }}
                    local box, len = BotPlanner.nearest(rows, function(r) return r.len end)
                    return box.slot .. ":" .. tostring(len)"#
                ),
            )
        };
        assert_eq!(
            pick("{ slot = 2, len = 9000 }, { slot = 5, len = 1200 }"),
            "5:1200"
        );
        assert_eq!(
            pick("{ slot = 5, len = 1200 }, { slot = 2, len = 9000 }"),
            "5:1200"
        );
        assert_eq!(pick("{ slot = 2 }, { slot = 5, len = 9000 }"), "5:9000");
        assert_eq!(
            pick("{ slot = 2, len = 700 }, { slot = 5, len = 700 }"),
            "2:700"
        );
        // A box in another room: see
        // the_remote_box_is_chosen_by_walking_distance_through_the_map.
    }

    /// A synthetic remembered world for the remote box choice. The bot
    /// stands in S. A is one door away, its box 20000 deep inside; C is two
    /// doors away (S -> B -> C), its box beside the door the bot arrives at.
    /// By walking distance C's box is nearer; by door count A's is.
    const BOX_WORLD: &str = r#"{ rooms = {
        { id = "S", doors = {
            { slot = 0, target = "A", x = 1000, z = 0, present = true },
            { slot = 1, target = "B", x = 1000, z = 2000, present = true } }, actions = {} },
        { id = "A", doors = { { slot = 0, target = "S", x = 0, z = 0, present = true } },
          actions = { { slot = 4, kind = "item_box", x = 20000, z = 0 } } },
        { id = "B", doors = {
            { slot = 0, target = "S", x = 0, z = 0, present = true },
            { slot = 1, target = "C", x = 2000, z = 0, present = true } }, actions = {} },
        { id = "C", doors = { { slot = 0, target = "B", x = 0, z = 0, present = true } },
          actions = { { slot = 2, kind = "item_box", x = 500, z = 0 } } },
    } }"#;

    #[test]
    fn the_remote_box_is_chosen_by_walking_distance_through_the_map() {
        let vm = route_vm();
        let pick = |arrivals: &str| -> String {
            eval(
                &vm,
                &format!(
                    r#"local world = {BOX_WORLD}
                    local best = BotPlanner.pick_remote_box(world, "S", {{
                        exit_len = function(slot) return 1500 end,
                        passable = function(room, door) return true end,
                        usable_box = function(room, action) return true end,
                        arrivals = {arrivals},
                    }})
                    return best.room .. ":" .. best.slot .. ":" .. #best.steps"#
                ),
            )
        };
        // Nothing learned from earlier choices: the map alone decides.
        assert_eq!(pick("{}"), "C:2:2");
        // Arrival points, when remembered, are where the leg starts: arriving
        // in A next to its box makes A's the nearer one.
        assert_eq!(pick(r#"{ A = { S = { x = 19500, z = 0 } } }"#), "A:4:1");
    }

    #[test]
    fn a_room_holding_a_needed_item_is_worth_a_visit() {
        let vm = route_vm();
        // 102 holds the broken shotgun, which 116's rack <use> needs.
        let bonus = |held: &str, done: &str| -> String {
            eval(
                &vm,
                &format!(
                    r#"local holds = {{ {{ room = "102", item = 0x1C, label = "broken shotgun" }} }}
                    local uses = {{ {{ room = "116", slot = 5, item = 0x1C, bank = 7, bit = 1,
                        label = "rack" }} }}
                    local b, tag = BotPlanner.holds_bonus("102", holds, uses,
                        function(item) return {held} end,
                        function(fact) return {done} end,
                        function(item) return "key" end, nil, {{}})
                    return b .. ":" .. tostring(tag)"#
                ),
            )
        };
        let needed = bonus("false", "false");
        assert!(needed.starts_with(&format!("{}:", 70 * 600)), "{needed}");
        assert!(needed.ends_with("holds 0x1C for rack"), "{needed}");
        // Held already, or its use done: nothing to fetch.
        assert_eq!(bonus("true", "false"), "0:nil");
        assert_eq!(bonus("false", "true"), "0:nil");
        // Another room's facts do not count.
        let other: String = eval(
            &vm,
            r#"return (BotPlanner.holds_bonus("103", { { room = "102", item = 0x1C } }, {},
                function() return false end, function() return false end,
                function() return "key" end, nil, {}))"#,
        );
        assert_eq!(other, "0");
    }

    #[test]
    fn a_needed_key_ranks_before_herbs_and_ammunition() {
        let vm = route_vm();
        // The way-out bonus at equal distance: the bigger bonus goes first.
        let bonus = |item: &str, kind: &str, ammo: &str| -> i64 {
            eval(
                &vm,
                &format!(
                    r#"local needed = {{ [0x34] = "204 -> 20D" }}
                    return (BotPlanner.item_bonus({item}, "{kind}", {ammo}, needed))"#
                ),
            )
        };
        let key = bonus("0x34", "key", "0x0B");
        let herb = bonus("0x44", "heal", "0x0B");
        let ammo = bonus("0x0B", "ammo", "0x0B");
        assert!(key > herb, "a needed key before a herb");
        assert!(key > ammo, "a needed key before the needed ammunition");
        assert!(ammo > herb);
        // A key no remembered door needs is a plain item.
        assert_eq!(bonus("0x36", "key", "nil"), herb);
    }

    #[test]
    fn desk_gating_follows_the_lock_bit_key_and_lockpick() {
        let vm = route_vm();
        let gate = |flags: &str, items: &str, full: &str| -> String {
            eval(
                &vm,
                &format!(
                    r#"local flags, items = {{ {flags} }}, {{ {items} }}
                    local bot = {{
                        flag = function(_, bank, bit) return flags[bank .. ":" .. bit] == true end,
                        has_item = function(_, id) return items[id] == true end,
                    }}
                    local desk = {{ kind = "desk", slot = 0, item_slot = 3, lock_bank = 2,
                        lock_bit = 5, desk_item = 0x0C, full = {full} }}
                    return tostring(BotPlanner.desk_blocker(bot, desk))"#
                ),
            )
        };
        // Lock bit set: opens on a press.
        assert_eq!(gate(r#"["2:5"] = true"#, "", "true"), "nil");
        // Lock bit clear: the desk key, the lockpick item or the lockpick
        // scenario flag opens it; nothing held blocks it.
        assert_eq!(gate("", "[0x3D] = true", "true"), "nil");
        assert_eq!(gate("", "[0x31] = true", "true"), "nil");
        assert_eq!(gate(r#"["0:124"] = true"#, "", "true"), "nil");
        assert_eq!(
            gate("", "[0x33] = true", "true"),
            "drawer locked: no desk key 0x3D or lockpick"
        );
        // An emptied drawer is no goal, whatever its lock.
        assert_eq!(
            gate(r#"["2:5"] = true"#, "", "false"),
            "the drawer is empty"
        );
        // The drawers map their item slots; other kinds are ignored.
        let slots: String = eval(
            &vm,
            r#"local by = BotPlanner.drawers({
                { kind = "desk", slot = 0, item_slot = 3 },
                { kind = "item", slot = 3, item_slot = 9 },
                { kind = "desk", slot = 5, item_slot = 6 },
            })
            local out = {}
            for item, desk in pairs(by) do out[#out + 1] = item .. "=" .. desk.slot end
            table.sort(out)
            return table.concat(out, ",")"#,
        );
        assert_eq!(slots, "3=0,6=5");
    }

    #[test]
    fn box_policy_keeps_needs_and_takes_back_what_is_wanted() {
        let vm = route_vm();
        // An 8-slot inventory: the handgun (equipped), its clips, a key, a
        // crest a <use> fact needs, a herb, the grenade gun the knowledge
        // file does not list, a spare clip of another gun and a book.
        let deposits: String = eval(
            &vm,
            r#"local ctx = { equipped = 0x02, guns = { [0x02] = true }, ammo = { [0x0B] = true },
                uses = { [0x2C] = true } }
            local items = {
                { slot = 0, item = 0x02, kind = "weapon", quantity = 15 },
                { slot = 1, item = 0x0B, kind = "ammo", quantity = 15 },
                { slot = 2, item = 0x34, kind = "key", quantity = 1 },
                { slot = 3, item = 0x2C, kind = "special", quantity = 1 },
                { slot = 4, item = 0x44, kind = "heal", quantity = 1 },
                { slot = 5, item = 0x07, kind = "weapon", quantity = 0 },
                { slot = 6, item = 0x0D, kind = "ammo", quantity = 6 },
                { slot = 7, item = 0x50, kind = "book", quantity = 1 },
            }
            local out = {}
            for _, row in ipairs(BotInventory.deposits(items, ctx)) do
                out[#out + 1] = string.format("%02X", row.item)
            end
            return table.concat(out, ",")"#,
        );
        assert_eq!(deposits, "07,0D,50");
        // Withdrawals: a use item, a key a locked door needs, the low ammo,
        // and one heal stack when none is held, capped at the free slots.
        let withdrawals = |free: u8| -> String {
            eval(
                &vm,
                &format!(
                    r#"local ctx = {{ uses = {{ [0x2C] = true }}, keys = {{ [0x35] = true }},
                        ammo_low = 0x0B, heals = 0 }}
                    local rows = {{
                        {{ slot = 0, item = 0x07, kind = "weapon" }},
                        {{ slot = 1, item = 0x44, kind = "heal" }},
                        {{ slot = 2, item = 0x2C, kind = "special" }},
                        {{ slot = 3, item = 0x43, kind = "heal" }},
                        {{ slot = 4, item = 0x35, kind = "key" }},
                        {{ slot = 5, item = 0x0B, kind = "ammo" }},
                    }}
                    local out = {{}}
                    for _, row in ipairs(BotInventory.withdrawals(rows, ctx, {free})) do
                        out[#out + 1] = string.format("%02X", row.item)
                    end
                    return table.concat(out, ",")"#
                ),
            )
        };
        assert_eq!(withdrawals(8), "44,2C,35,0B");
        assert_eq!(withdrawals(2), "44,2C");
        assert_eq!(withdrawals(0), "");
        // The top-up at a box here: every stack of the gun's ammunition,
        // while RESERVE (1) slots stay free; never from a remote reason.
        let top_up = |free: u8, remote: bool| -> String {
            eval(
                &vm,
                &format!(
                    r#"local ctx = {{ top_up = 0x0B, reserve_only = {remote} }}
                    local rows = {{
                        {{ slot = 0, item = 0x0B, kind = "ammo" }},
                        {{ slot = 1, item = 0x07, kind = "weapon" }},
                        {{ slot = 2, item = 0x0B, kind = "ammo" }},
                    }}
                    local out = {{}}
                    for _, row in ipairs(BotInventory.withdrawals(rows, ctx, {free})) do
                        out[#out + 1] = string.format("%02X", row.item)
                    end
                    return table.concat(out, ",")"#
                ),
            )
        };
        assert_eq!(top_up(4, false), "0B,0B");
        assert_eq!(top_up(2, false), "0B");
        assert_eq!(top_up(1, false), "");
        assert_eq!(top_up(4, true), "");
        // The heal top-up at a box here: every heal stack that restores
        // health while RESERVE slots stay free (a lone red herb stays).
        let heal_top_up = |free: u8, remote: bool| -> String {
            eval(
                &vm,
                &format!(
                    r#"local ctx = {{ heals = 1, heal_top_up = true, reserve_only = {remote} }}
                    local rows = {{
                        {{ slot = 0, item = 0x44, kind = "heal" }},
                        {{ slot = 1, item = 0x43, kind = "heal", heals = false }},
                        {{ slot = 2, item = 0x4A, kind = "heal" }},
                    }}
                    local out = {{}}
                    for _, row in ipairs(BotInventory.withdrawals(rows, ctx, {free})) do
                        out[#out + 1] = string.format("%02X", row.item)
                    end
                    return table.concat(out, ",")"#
                ),
            )
        };
        // The rearm: the shotgun and its shells come out ahead of a heal
        // top-up, into any free slot, even from a remote reason.
        let rearm = eval::<String>(
            &vm,
            r#"local ctx = { heals = 1, heal_top_up = true, reserve_only = true,
                rearm = { [0x03] = true, [0x0C] = true } }
            local rows = {
                { slot = 0, item = 0x44, kind = "heal" },
                { slot = 1, item = 0x0C, kind = "ammo" },
                { slot = 2, item = 0x03, kind = "weapon" },
            }
            local out = {}
            for _, row in ipairs(BotInventory.withdrawals(rows, ctx, 2)) do
                out[#out + 1] = string.format("%02X", row.item)
            end
            return table.concat(out, ",")"#,
        );
        assert_eq!(rearm, "0C,03");
        // Two boxed knives the loadout wants: one comes out (the deposit
        // rule would box the second again).
        let knives = eval::<String>(
            &vm,
            r#"local ctx = { heals = 1, loadout = { [0x01] = true } }
            local rows = {
                { slot = 0, item = 0x01, kind = "weapon" },
                { slot = 1, item = 0x44, kind = "heal" },
                { slot = 2, item = 0x01, kind = "weapon" },
            }
            local out = {}
            for _, row in ipairs(BotInventory.withdrawals(rows, ctx, 4)) do
                out[#out + 1] = string.format("%02X", row.item)
            end
            return table.concat(out, ",")"#,
        );
        assert_eq!(knives, "01");
        assert_eq!(heal_top_up(4, false), "44,4A");
        assert_eq!(heal_top_up(2, false), "44");
        assert_eq!(heal_top_up(1, false), "");
        assert_eq!(heal_top_up(4, true), "");
        // The slot reserve: deposit at a box here once no more than one slot
        // is free and more pickups than that need one.
        let reserve = |used: u8, deposits: u8, pickups: u8| -> bool {
            eval(
                &vm,
                &format!(
                    "return BotInventory.reserve_due({{ capacity = 8, used = {used} }}, \
                     {deposits}, {pickups})"
                ),
            )
        };
        assert!(reserve(7, 1, 2));
        assert!(reserve(8, 1, 1));
        assert!(!reserve(7, 1, 1), "the one free slot covers the one pickup");
        assert!(!reserve(6, 1, 5), "two free slots");
        assert!(!reserve(7, 0, 3), "nothing may go in the box");
        // Paging the box cursor: the nearest wanted slot, wrapping.
        let target = |cursor: u8, want: u8| -> String {
            eval(
                &vm,
                &format!(
                    r#"local rows = {{}}
                    for i = 0, 47 do rows[#rows + 1] = {{ slot = i, item = 0 }} end
                    rows[3].item, rows[47].item = 0x2C, 0x0B
                    local row, dir = BotInventory.box_target(rows, {cursor},
                        function(r) return r.item == {want} end)
                    return row and (row.slot .. ":" .. dir) or "none""#
                ),
            )
        };
        assert_eq!(target(0, 0x2C), "2:1");
        assert_eq!(target(5, 0x2C), "2:-1");
        assert_eq!(target(2, 0x2C), "2:0");
        assert_eq!(target(1, 0x0B), "46:-1");
        assert_eq!(target(40, 0x0B), "46:1");
        assert_eq!(target(0, 0x99), "none");
    }

    #[test]
    fn combine_rule_frees_a_slot_heals_first_and_spares_use_items() {
        let vm = route_vm();
        // A fake engine reply: herb pairs mix, the clip goes into the gun,
        // and nothing else combines.
        let pick = |uses: &str| -> String {
            eval(
                &vm,
                &format!(
                    r#"local items = {{
                        {{ slot = 0, item = 0x02, kind = "weapon", quantity = 0 }},
                        {{ slot = 1, item = 0x0B, kind = "ammo", quantity = 15 }},
                        {{ slot = 2, item = 0x44, kind = "heal", quantity = 1 }},
                        {{ slot = 3, item = 0x43, kind = "heal", quantity = 1 }},
                        {{ slot = 4, item = 0x2C, kind = "special", quantity = 1 }},
                    }}
                    local function result(a, b)
                        local x, y = items[a + 1].item, items[b + 1].item
                        if (x == 0x44 and y == 0x43) or (x == 0x43 and y == 0x44) then
                            return {{ result = "herb", frees = true }}
                        end
                        if x == 0x02 and y == 0x0B then
                            return {{ result = "applied", frees = true }}
                        end
                        if x == 0x2C then
                            return {{ result = "applied", frees = true }}
                        end
                        return {{ result = "no_recipe", frees = false }}
                    end
                    local p = BotInventory.combine_pick(items, {{ uses = {uses} }}, result)
                    return p and string.format("%d>%d", p.slot, p.target) or "none""#
                ),
            )
        };
        // Herbs first (rank 1+1), and the crest is never a candidate kind.
        assert_eq!(pick("{}"), "2>3");
        // A <use> fact on a herb leaves only the gun and its clip.
        assert_eq!(pick("{ [0x44] = true }"), "0>1");
        assert_eq!(pick("{ [0x44] = true, [0x0B] = true }"), "none");
    }

    #[test]
    fn push_error_with_a_direction_clears_the_zone_that_way() {
        let vm = route_vm();
        // The 10F shelf (centre 17394,11190, half 1300x550) over the score
        // zone (17000,11500, 800x1000).
        let error = |x: i32, dir: &str| -> String {
            eval(
                &vm,
                &format!(
                    r#"local ex, ez = BotNav.push_error({{ x = {x}, z = 11190, half_x = 1300,
                        half_z = 550 }}, {{ x = 17000, z = 11500, width = 800, depth = 1000 }}, {dir})
                    return ex .. "," .. ez"#
                ),
            )
        };
        // East: the west face must pass 17600 (the zone's far quarter).
        assert_eq!(error(17394, "{ x = 1, z = 0 }"), "1506,0");
        assert_eq!(error(19000, "{ x = 1, z = 0 }"), "0,0");
        // West: the east face must pass 17200.
        assert_eq!(error(17394, "{ x = -1, z = 0 }"), "-1494,0");
        assert_eq!(error(15800, "{ x = -1, z = 0 }"), "0,0");
        // No direction: the centre into the middle half on both axes.
        assert_eq!(error(17394, "nil"), "0,560");
        assert!(eval::<bool>(
            &vm,
            r#"local d = BotPuzzle.push_dir({ direction = "east", dx = 1, dz = 0 })
            return d.x == 1 and d.z == 0 and BotPuzzle.push_dir({}) == nil"#
        ));
    }

    #[test]
    fn lean_rule_tests_the_probe_against_the_zone() {
        let vm = route_vm();
        // The 102 drawer: x 2000..3750, z 2237..2937, both edges inclusive.
        let zone = "{ x = 2000, z = 2237, width = 1750, depth = 700 }";
        let call = |f: &str, args: &str| -> bool {
            eval(&vm, &format!("return BotNav.{f}({zone}, {args})"))
        };
        assert!(call("reach_in", "2911, 2937"));
        assert!(call("reach_in", "3750, 2237"));
        assert!(!call("reach_in", "2911, 2938"));
        // The walk stops a skin off the desk: facing north from z 3538 the
        // probe lands one unit short, and a step forward carries it in.
        assert!(call("reach_short", "2911, 3542, 2911, 2943"));
        // Already inside, or facing away (the zone behind the probe): no.
        assert!(!call("reach_short", "2911, 3518, 2911, 2918"));
        assert!(!call("reach_short", "2911, 2343, 2911, 2943"));
        // Short by more than an approach on an axis: not a lean.
        assert!(!call("reach_short", "2911, 3838, 2911, 3238"));
        // An overshoot past the far edge lies behind the probe.
        assert!(!call("reach_short", "2911, 2600, 2911, 2000"));
        // Within a reach and an approach of the zone (Manhattan gap), not
        // beyond it.
        assert!(call("within_reach", "2885, 3538"));
        assert!(call("within_reach", "1700, 3437"));
        assert!(!call("within_reach", "2885, 3838"));
        assert!(!call("within_reach", "1000, 3500"));
    }

    #[test]
    fn side_offsets_reach_a_narrow_zone_from_its_open_end() {
        let vm = route_vm();
        let offsets = |span: i32| -> String {
            eval(
                &vm,
                &format!("return table.concat(BotNav.side_offsets({span}), ',')"),
            )
        };
        // A long side: the middle and a quarter in from each end.
        assert_eq!(offsets(1800), "0,-450,450");
        assert_eq!(offsets(600), "0,-150,150");
        // 102's broken shotgun (x 6500..7000) beside the east wall: the
        // middle (6750) is inside the wall's grown box, so a short side
        // also tries a quarter approach in from each end (6575 and 6925).
        assert_eq!(offsets(500), "0,-175,175");
        // Too short for an end point inside the span: the middle only.
        assert_eq!(offsets(150), "0");
        assert_eq!(offsets(0), "0");
    }

    fn pick_margin_with(vm: &BotVm, guns: &str, entry: &str) -> String {
        eval(
            vm,
            &format!(
                r#"local row = BotTactics.pick_gun({{ {guns} }}, {entry}, nil, 1.5)
                return string.format("%#04x", row.item)"#
            ),
        )
    }

    #[test]
    fn pick_gun_switches_only_when_the_plain_pick_cannot_finish_the_kill() {
        let vm = route_vm();
        let pick = |guns: &str, entry: &str, health: &str| -> String {
            eval(
                &vm,
                &format!(
                    r#"local row = BotTactics.pick_gun({{ {guns} }}, {entry}, {health})
                    return row and string.format("%#04x", row.item) or "none""#
                ),
            )
        };
        // The zombie's keep bands: handgun 12 a hit, shotgun 53.
        let zombie =
            "{ health = 99, keep = { [0x02] = { damage = 12 }, [0x03] = { damage = 53 } } }";
        let handgun = "{ item = 0x02, rounds = 45, equipped = true }";
        let shotgun = "{ item = 0x03, rounds = 7, equipped = false }";
        // The equipped handgun covers the kill (9 of 45 rounds): it stands,
        // and the shells are kept for a fight it cannot finish.
        assert_eq!(
            pick(&format!("{handgun}, {shotgun}"), zombie, "nil"),
            "0x02"
        );
        // Five handgun rounds do not cover a full-health zombie; two shells do.
        let short = "{ item = 0x02, rounds = 5, equipped = true }";
        assert_eq!(pick(&format!("{short}, {shotgun}"), zombie, "nil"), "0x03");
        // A wounded zombie (live health 40): four handgun rounds cover it.
        assert_eq!(pick(&format!("{short}, {shotgun}"), zombie, "40"), "0x02");
        // Nothing covers it: the plain pick, the equipped gun.
        let last_shell = "{ item = 0x03, rounds = 1, equipped = false }";
        assert_eq!(
            pick(&format!("{short}, {last_shell}"), zombie, "nil"),
            "0x02"
        );
        // Not equipped: the plain pick is the most rounds.
        let unequipped = "{ item = 0x02, rounds = 45, equipped = false }";
        assert_eq!(
            pick(&format!("{shotgun}, {unequipped}"), zombie, "nil"),
            "0x02"
        );
        // No knowledge of the enemy: the plain pick.
        assert_eq!(pick(&format!("{short}, {shotgun}"), "nil", "nil"), "0x02");
        // With a margin for misses: nine handgun rounds cover the bare kill
        // (9) but not 1.5 times it, while two shells (3 of 7) do.
        let nine = "{ item = 0x02, rounds = 9, equipped = true }";
        let pick_margin = |guns: &str| -> String {
            eval(
                &vm,
                &format!(
                    r#"local row = BotTactics.pick_gun({{ {guns} }}, {zombie}, nil, 1.5)
                    return string.format("%#04x", row.item)"#
                ),
            )
        };
        assert_eq!(pick_margin(&format!("{nine}, {shotgun}")), "0x03");
        // No gun covers the margin: the plain pick covers the bare kill.
        assert_eq!(pick_margin(&format!("{nine}, {last_shell}")), "0x02");
        // An equipped gun with no kill data (no keep band for this enemy)
        // gives way to one whose band covers the kill.
        let launcher = "{ item = 0x07, rounds = 3, equipped = true }";
        assert_eq!(
            pick(&format!("{launcher}, {unequipped}"), zombie, "nil"),
            "0x02"
        );
        assert_eq!(pick(launcher, zombie, "nil"), "0x07");
        // A reserve gun (the launcher's scarce rounds) is a switch only when
        // no other gun covers the bare kill: nine handgun rounds cover a
        // zombie (9) though not with the margin, so the launcher stays put.
        let hunter =
            "{ health = 99, keep = { [0x02] = { damage = 12 }, [0x07] = { damage = 100 } } }";
        let held_launcher = "{ item = 0x07, rounds = 6, equipped = false, reserve = true }";
        let nine_rounds = "{ item = 0x02, rounds = 9, equipped = true }";
        assert_eq!(
            pick_margin_with(&vm, &format!("{nine_rounds}, {held_launcher}"), hunter),
            "0x02"
        );
        // Five handgun rounds do not: the launcher is spent.
        assert_eq!(
            pick_margin_with(&vm, &format!("{short}, {held_launcher}"), hunter),
            "0x07"
        );
        // Equipped, it gives way to a gun that covers the kill.
        let equipped_launcher = "{ item = 0x07, rounds = 6, equipped = true, reserve = true }";
        let full_handgun = "{ item = 0x02, rounds = 45, equipped = false }";
        assert_eq!(
            pick_margin_with(&vm, &format!("{equipped_launcher}, {full_handgun}"), hunter),
            "0x02"
        );
        assert_eq!(pick("", zombie, "nil"), "none");
    }

    #[test]
    fn use_pending_waits_only_for_a_probe_where_the_last_step_acted() {
        let vm = route_vm();
        let pending = |kind: &str, zone: &str, items: &str, flags: &str| -> String {
            eval(
                &vm,
                &format!(
                    r#"local items, flags = {{ {items} }}, {{ {flags} }}
                    local bot = {{
                        flag = function(_, bank, bit) return flags[bank .. ":" .. bit] == true end,
                        has_item = function(_, id) return items[id] == true end,
                        actions = function() return {{
                            {{ slot = 5, kind = "{kind}", x = 9820, z = 5340, width = 2000, depth = 2000 }},
                        }} end,
                        room_meta = function() return {{ uses = {{
                            {{ slot = 5, item = 0x1C, bank = 7, bit = 1, label = "rack" }},
                        }} }} end,
                    }}
                    local fact = BotPuzzle.use_pending(bot, {zone})
                    return fact and fact.label or "none""#
                ),
            )
        };
        // 116: the rack pickup's zone overlaps AOT 5, still SCE_NONE.
        let rack = "{ x = 10200, z = 5700, width = 1400, depth = 1400 }";
        assert_eq!(pending("other", rack, "[0x1C] = true", ""), "rack");
        // Live already, done, or the item not held: nothing to wait for.
        assert_eq!(pending("flag_bank_set", rack, "[0x1C] = true", ""), "none");
        assert_eq!(
            pending("other", rack, "[0x1C] = true", r#"["7:1"] = true"#),
            "none"
        );
        assert_eq!(pending("other", rack, "", ""), "none");
        // A step elsewhere in the room (10F's piano, far from the hollow).
        let far = "{ x = 17000, z = 11500, width = 800, depth = 1000 }";
        assert_eq!(pending("other", far, "[0x1C] = true", ""), "none");
        assert_eq!(pending("other", "nil", "[0x1C] = true", ""), "none");
        let touching = "{ x = 11820, z = 7340, width = 10, depth = 10 }";
        assert_eq!(pending("other", touching, "[0x1C] = true", ""), "rack");
        // Just entered (40A): a probe anywhere in the room counts.
        let anywhere = |kind: &str| -> String {
            eval(
                &vm,
                &format!(
                    r#"local bot = {{
                        flag = function() return false end,
                        has_item = function(_, id) return id == 0x3E end,
                        actions = function() return {{
                            {{ slot = 5, kind = "{kind}", x = 11300, z = 9600, width = 1100, depth = 1700 }},
                        }} end,
                        room_meta = function() return {{ uses = {{
                            {{ slot = 5, item = 0x3E, bank = 0, bit = 0x24, label = "red book" }},
                        }} }} end,
                    }}
                    local fact = BotPuzzle.use_pending(bot, nil, true)
                    return fact and fact.label or "none""#
                ),
            )
        };
        assert_eq!(anywhere("event"), "red book");
        assert_eq!(anywhere("flag_bank_set"), "none");
    }

    #[test]
    fn avoid_unless_held_lifts_while_the_item_is_held() {
        let vm = route_vm();
        let avoided = |avoid: &str, flags: &str, items: &str| -> String {
            eval(
                &vm,
                &format!(
                    r#"local flags, items = {{ {flags} }}, {{ {items} }}
                    local bot = {{
                        flag = function(_, bank, bit) return flags[bank .. ":" .. bit] == true end,
                        has_item = function(_, id) return items[id] == true end,
                        room_meta = function() return {{ avoids = {{ {avoid} }} }} end,
                    }}
                    local reason, gate = BotPuzzle.avoided(bot, 1)
                    return tostring(reason) .. "|" .. tostring(gate ~= nil)"#
                ),
            )
        };
        let rack = r#"{ slot = 1, unless_held = 0x1C, reason = "trap" }"#;
        // Without the broken shotgun the rack is off limits, and the avoid
        // is gated (not permanent), so the mark lifts once it is held.
        assert_eq!(avoided(rack, "", ""), "trap|true");
        assert_eq!(avoided(rack, "", "[0x1C] = true"), "nil|false");
        // With a flag gate too, both must allow it.
        let both = r#"{ slot = 1, unless_held = 0x1C, bank = 7, bit = 1, ["while"] = "clear",
            reason = "trap" }"#;
        assert_eq!(avoided(both, "", ""), "trap|true");
        assert_eq!(avoided(both, r#"["7:1"] = true"#, ""), "nil|false");
        assert_eq!(avoided(both, "", "[0x1C] = true"), "nil|false");
        // A plain avoid always holds and has no gate.
        assert_eq!(
            avoided(r#"{ slot = 1, reason = "x" }"#, "", "[0x1C] = true"),
            "x|false"
        );
        // gate_holds reads the item from any room.
        assert!(eval::<bool>(
            &vm,
            r#"local bot = { has_item = function() return true end }
            return BotPuzzle.gate_holds(bot, { unless_held = 0x1C }) == false"#
        ));
    }

    #[test]
    fn player_row_reports_the_forward_action_probe() {
        let temp = TempDir::new("reach");
        let bot = temp.path.join("bot");
        fs::create_dir_all(&bot).unwrap();
        fs::write(
            bot.join("init.lua"),
            br#"function on_bot_tick(bot)
                local p = bot:player()
                RESULT = string.format("%d,%d", p.reach_x, p.reach_z)
            end
"#,
        )
        .unwrap();
        let pack = Pack::open(&temp.path).unwrap();
        let vm = BotVm::load(&pack).unwrap();
        let mut snapshot = world::tests::snapshot(0x102, 2911, 3518);
        // Facing north (-Z): the probe is a reach ahead, where probe_actions
        // tests the zones.
        snapshot.player.angle = 0x400;
        let (dx, dz) = crate::player::reach_offset(0x400);
        assert!(dx.abs() <= 1 && (590..=600).contains(&-dz));
        vm.call(BotCtx {
            snapshot,
            saves: SaveLog::default(),
            meta: None,
            uses: Rc::default(),
            holds: Rc::default(),
            fights: Rc::default(),
            rides: Rc::default(),
            revisits: Rc::default(),
            reaches: Rc::default(),
            errands: Rc::default(),
            brews: Rc::default(),
            door_vias: Rc::default(),
            avoid_facts: Rc::default(),
            knowledge: None,
            tick: 1,
            risk: 0.25,
            params: Rc::default(),
            goal_room: None,
            shares: BotShares::default(),
        })
        .unwrap();
        assert_eq!(
            vm.lua.globals().get::<String>("RESULT").unwrap(),
            format!("{},{}", 2911 + dx, 3518 + dz)
        );
    }

    #[test]
    fn cure_rule_uses_a_held_cure_for_the_poison_the_engine_reports() {
        let vm = tactics_vm();
        let pick = |player: &str, held: &str, low: bool| -> String {
            eval(
                &vm,
                &format!(
                    r#"local held = {held}
                    local c = BotSurvival.pick_cure(K.cures, {player},
                        function(item) return held[item] == true end, {low})
                    return c and string.format("0x%02X", c.item) or "none""#
                ),
            )
        };
        let poisoned = "{ poisoned = true, seriously_poisoned = false }";
        let serious = "{ poisoned = false, seriously_poisoned = true }";
        let healthy = "{ poisoned = false, seriously_poisoned = false }";
        // Not poisoned: nothing, whatever is held.
        assert_eq!(pick(healthy, "{ [0x45] = true }", true), "none");
        // The 0x02 poison: the blue herb, at once.
        assert_eq!(
            pick(poisoned, "{ [0x45] = true, [0x48] = true }", false),
            "0x45"
        );
        // A herb mix also heals, so it waits until health is low.
        assert_eq!(pick(poisoned, "{ [0x48] = true }", false), "none");
        assert_eq!(pick(poisoned, "{ [0x48] = true }", true), "0x48");
        // The serum clears only the 0x20 poison, and a herb never does.
        assert_eq!(pick(poisoned, "{ [0x42] = true }", true), "none");
        assert_eq!(
            pick(serious, "{ [0x45] = true, [0x42] = true }", false),
            "0x42"
        );
        // No cure held.
        assert_eq!(pick(poisoned, "{ [0x44] = true }", true), "none");
    }

    #[test]
    fn only_a_pickup_with_the_player_bit_is_an_item() {
        assert!(player_can_fire(0x81));
        assert!(player_can_fire(0x01));
        // 10D's wind crest before the statue turns: SAT_MANUAL | SAT_UNDER.
        assert!(!player_can_fire(0xC0));
        assert!(!player_can_fire(0x84));
    }

    #[test]
    fn ammo_floor_scales_with_risk_and_the_hardest_kill() {
        let vm = tactics_vm();
        // The handgun's hardest `fight` kill in re1.xml: Plant 42, its body
        // and five vines (340 health) at 15 a hit, is 23 shots (Yawn's
        // rematch 14, a zombie 9, a cerberus 5, a hunter 8).
        assert_eq!(
            eval::<i64>(&vm, "return BotSurvival.shots_per_kill(K, 2)"),
            23
        );
        // The shotgun: Plant 42 (340 at 20) is 18 shots; Yawn (400 at 40)
        // 11, the cerberus (122 at 40) and the hunter (111 at 32) 4.
        assert_eq!(
            eval::<i64>(&vm, "return BotSurvival.shots_per_kill(K, 3)"),
            18
        );
        // A weapon nobody fights with has no floor.
        assert!(eval::<bool>(
            &vm,
            "return BotSurvival.shots_per_kill(K, 0x40) == nil"
        ));
        assert_eq!(eval::<i64>(&vm, "return BotSurvival.ammo_floor(nil, 0)"), 0);
        // 1 + 2 * (1 - risk) kills, rounded up: 3 kills at risk 0, 2.5 at
        // the default 0.25, one at risk 1.
        assert_eq!(eval::<i64>(&vm, "return BotSurvival.ammo_floor(9, 0)"), 27);
        assert_eq!(
            eval::<i64>(&vm, "return BotSurvival.ammo_floor(9, 0.25)"),
            23
        );
        assert_eq!(eval::<i64>(&vm, "return BotSurvival.ammo_floor(9, 1)"), 9);
        assert_eq!(eval::<i64>(&vm, "return BotSurvival.ammo_floor(4, 0.5)"), 8);
    }

    #[test]
    fn save_decision_follows_risk_ribbons_and_cooldown() {
        let vm = route_vm();
        // A base case: new progress, healthy, calm, never saved.
        vm.lua
            .load(
                r#"
                function decide(over)
                    local o = { risk = 0.25, can_save = true, health = 90, max_health = 100,
                        critical = false, risky = false, progress = 5, saved_progress = 4 }
                    for k, v in pairs(over or {}) do o[k] = v end
                    local wants, why = BotSave.wants(o)
                    return tostring(wants) .. ":" .. why
                end
            "#,
            )
            .exec()
            .unwrap();
        let decide = |over: &str| -> String { eval(&vm, &format!("return decide({over})")) };
        // The ribbon rule: a character that needs ribbons saves only with one.
        assert!(eval::<bool>(
            &vm,
            "return BotSave.can_save({ needs_ribbon = false, ribbons = 0 })"
        ));
        assert!(!eval::<bool>(
            &vm,
            "return BotSave.can_save({ needs_ribbon = true, ribbons = 0 })"
        ));
        assert!(eval::<bool>(
            &vm,
            "return BotSave.can_save({ needs_ribbon = true, ribbons = 2 })"
        ));
        assert_eq!(decide("{ can_save = false }"), "false:no ink ribbon");
        // A cautious bot saves only for a reason: no milestone, no save.
        assert_eq!(
            decide("{ risk = 0, since_save = 9000 }"),
            "false:no milestone since the last save"
        );
        assert_eq!(
            decide("{ progress = 4, milestone = true }"),
            "false:nothing new since the last save"
        );
        // A milestone saves once the last save is milestone_gap old (2 min
        // at risk 0, 3 at 0.25), and at once when no save exists yet.
        assert_eq!(
            eval::<i64>(&vm, "return BotSave.milestone_gap(0)"),
            2 * 60 * 30
        );
        assert_eq!(
            eval::<i64>(&vm, "return BotSave.milestone_gap(0.25)"),
            3 * 60 * 30
        );
        assert_eq!(
            decide("{ milestone = true }"),
            "true:the first milestone of the run (no save yet)"
        );
        assert_eq!(
            decide("{ milestone = true, since_save = 5399 }"),
            "false:a milestone, but the last save is recent"
        );
        assert_eq!(
            decide("{ milestone = true, since_save = 5400 }"),
            "true:a milestone since the last save"
        );
        // Low health, or a monster in the next room below 3/4 health.
        assert_eq!(
            decide("{ since_save = 9000, health = 49 }"),
            "true:low health"
        );
        assert_eq!(
            decide("{ since_save = 9000, risky_next = '101', health = 74 }"),
            "true:a risky room is next: 101"
        );
        assert_eq!(
            decide("{ since_save = 9000, risky_next = '101', health = 75 }"),
            "false:no milestone since the last save"
        );
        // Inventory work due here comes first, even before critical health.
        assert_eq!(
            decide("{ milestone = true, critical = true, inventory_due = 'a combine is due' }"),
            "false:inventory work first: a combine is due"
        );
        // Milestones: key items taken, doors unlocked, <use> facts done and
        // the planner's finished puzzle steps.
        let milestones: i64 = eval(
            &vm,
            r#"local world = { rooms = {
                   { items = { { item = 0x35, taken = true }, { item = 0x44, taken = true },
                               { item = 0x34, taken = false } },
                     doors = { { unlocked = true }, { unlocked = false } } } } }
               local kinds = { [0x35] = "key", [0x34] = "key", [0x44] = "heal" }
               local facts = { { done = true }, { done = false } }
               return BotSave.milestones(world, function(i) return kinds[i] end, facts,
                   function(f) return f.done end, 2)"#,
        );
        assert_eq!(milestones, 5);
        // The cooldown: 20 s at risk 0, 120 s at risk 1.
        assert_eq!(eval::<i64>(&vm, "return BotSave.cooldown(0)"), 20 * 30);
        assert_eq!(eval::<i64>(&vm, "return BotSave.cooldown(1)"), 120 * 30);
        assert_eq!(
            decide("{ risk = 0, since_save = 599 }"),
            "false:saved recently"
        );
        assert!(decide("{ risk = 0, since_save = 600, health = 40 }").starts_with("true:"));
        // Critical health skips the cooldown, not the progress test.
        assert_eq!(
            decide("{ since_save = 10, critical = true }"),
            "true:critical health"
        );
        assert!(decide("{ progress = 4, critical = true }").starts_with("false:"));
        // A bold bot saves only on low health or in a risky room.
        assert!(decide("{ risk = 1 }").starts_with("false:a bold bot"));
        assert_eq!(decide("{ risk = 1, health = 40 }"), "true:low health");
        assert_eq!(decide("{ risk = 1, risky = true }"), "true:risky room");
        assert!(decide("{ risk = 0.5, milestone = true }").starts_with("true:"));
        assert!(decide("{ risk = 0.6, milestone = true }").starts_with("false:a bold bot"));
    }

    #[test]
    fn save_safety_refuses_a_save_the_reload_dies_in_again() {
        let vm = route_vm();
        let dead_end =
            |o: &str| -> String { eval(&vm, &format!("return tostring(BotSave.dead_end({o}))")) };
        // The critical floor at risk 0.25 is 0.1625 of max health: 14 of 96
        // is under it, 16 is not.
        let low = "health = 14, max_health = 96, risk = 0.25";
        assert_eq!(
            dead_end(&format!("{{ {low}, heals = 0, rounds = 30, shots = 9 }}")),
            "critical health with no heal item"
        );
        assert_eq!(
            dead_end(&format!("{{ {low}, heals = 1, rounds = 0, shots = 9 }}")),
            "critical health without the rounds for one kill"
        );
        assert_eq!(
            dead_end(&format!("{{ {low}, heals = 1, rounds = 8, shots = 9 }}")),
            "critical health without the rounds for one kill"
        );
        // Critical health with a heal and a kill's worth of rounds is fine,
        // and so is anything above the floor.
        assert_eq!(
            dead_end(&format!("{{ {low}, heals = 1, rounds = 9, shots = 9 }}")),
            "nil"
        );
        assert_eq!(
            dead_end("{ health = 16, max_health = 96, risk = 0.25, heals = 0, rounds = 0 }"),
            "nil"
        );
        // No other choice: no save yet, or the latest one is no better.
        let no_choice = |latest: &str| -> bool {
            eval(
                &vm,
                &format!(
                    "return BotSave.no_choice({latest}, {{ health = 14, rounds = 0, heals = 0 }})"
                ),
            )
        };
        assert!(no_choice("nil"));
        assert!(no_choice("{ health = 10, rounds = 0, heals = 0 }"));
        assert!(!no_choice("{ health = 60, rounds = 0, heals = 0 }"));
        assert!(!no_choice("{ health = 10, rounds = 15, heals = 0 }"));
        // A save recorded before the condition was kept counts as better.
        assert!(!no_choice("{ count = 1 }"));
        // The decision: a dead end skips every save rule, critical health
        // and the cautious bot's save-everywhere alike, unless it is the
        // only choice.
        vm.lua
            .load(
                r#"
                function decide_safe(over)
                    local o = { risk = 0.25, can_save = true, health = 14, max_health = 96,
                        critical = true, risky = true, progress = 5, saved_progress = 4,
                        dead_end = "critical health with no heal item", no_choice = false }
                    for k, v in pairs(over or {}) do o[k] = v end
                    local wants, why = BotSave.wants(o)
                    return tostring(wants) .. ":" .. why
                end
            "#,
            )
            .exec()
            .unwrap();
        let decide = |over: &str| -> String { eval(&vm, &format!("return decide_safe({over})")) };
        assert_eq!(
            decide("{}"),
            "false:a reload would start in the same death: critical health with no heal item"
        );
        assert_eq!(
            decide("{ critical = false, risk = 0 }"),
            "false:a reload would start in the same death: critical health with no heal item"
        );
        // The critical-health save stays where it helps.
        assert_eq!(decide("{ no_choice = true }"), "true:critical health");
        assert_eq!(decide("{ dead_end = false }"), "true:critical health");
        // The ribbon and progress rules still come first.
        assert_eq!(decide("{ can_save = false }"), "false:no ink ribbon");
        assert_eq!(
            decide("{ progress = 4 }"),
            "false:nothing new since the last save"
        );
        // The condition is recorded with the save and read back.
        let read: String = eval(
            &vm,
            r#"local e = BotSave.track({}, { count = 1, slot = 0, room = "R", tick = 50 }, 7,
                   { health = 40, rounds = 12, heals = 1 })
               local m = {}
               for k, v in pairs(e) do m[k] = tostring(v) end
               local l = BotSave.latest(m)
               return string.format("%d|%d|%d", l.health, l.rounds, l.heals)"#,
        );
        assert_eq!(read, "40|12|1");
    }

    #[test]
    fn ammo_sourcing_explores_and_fights_only_what_it_must() {
        let vm = tactics_vm();
        // Under the floor (an ammunition item is needed) with nothing known
        // to reach: sourcing. Known ammunition, or rounds at the floor: not.
        assert!(eval::<bool>(&vm, "return BotSurvival.sourcing(0x0B, 0)"));
        assert!(eval::<bool>(&vm, "return BotSurvival.sourcing(0x0B, nil)"));
        assert!(!eval::<bool>(&vm, "return BotSurvival.sourcing(0x0B, 1)"));
        assert!(!eval::<bool>(&vm, "return BotSurvival.sourcing(nil, 0)"));
        // While sourcing, a monster only in a room crossed again is run
        // past; what closes in, or cannot be outrun, is still fought.
        vm.lua
            .load(
                r#"
                function verdict_src(over)
                    local o = { policy = "fight", gun = true, rounds = 30, needed = 5,
                        health = 96, max_health = 96, risk = 0.25, crossing = true,
                        sourcing = true, distance = 5000, reach = 1500, outrun = true }
                    for k, v in pairs(over or {}) do o[k] = v end
                    local v, why = BotTactics.decide(o)
                    return v .. ":" .. why
                end
            "#,
            )
            .exec()
            .unwrap();
        let verdict = |over: &str| -> String { eval(&vm, &format!("return verdict_src({over})")) };
        assert_eq!(
            verdict("{}"),
            "run:sourcing ammunition: only what must be fought"
        );
        assert_eq!(
            verdict("{ sourcing = false }"),
            "fight:the room is crossed again"
        );
        assert_eq!(verdict("{ distance = 2900 }"), "fight:closing in");
        assert_eq!(verdict("{ outrun = false }"), "fight:cannot outrun it");
        // The item box: one look while sourcing, before any box was opened,
        // and only with a slot free for what it may hold.
        let look =
            |args: &str| -> bool { eval(&vm, &format!("return BotInventory.look_in_box({args})")) };
        assert!(look("{ used = 6, capacity = 8 }, nil, true"));
        assert!(!look("{ used = 6, capacity = 8 }, '', true"));
        assert!(!look("{ used = 6, capacity = 8 }, '0B', true"));
        assert!(!look("{ used = 6, capacity = 8 }, nil, false"));
        assert!(!look("{ used = 8, capacity = 8 }, nil, true"));
        // At a box in this room the first look needs no sourcing.
        assert!(look("{ used = 6, capacity = 8 }, nil, false, true"));
        assert!(!look("{ used = 6, capacity = 8 }, '0B', false, true"));
        assert!(!look("{ used = 8, capacity = 8 }, nil, false, true"));
    }

    #[test]
    fn latest_save_tracking_and_slot_choice() {
        let vm = route_vm();
        // A new save is reported once; memory strings read back as numbers.
        let first: String = eval(
            &vm,
            r#"local e = BotSave.track({}, { count = 1, slot = 0, room = "R", tick = 50 }, 7)
               return string.format("%d|%d|%s|%d|%d", e.save_count, e.save_slot, e.save_room,
                   e.save_tick, e.save_progress)"#,
        );
        assert_eq!(first, "1|0|R|50|7");
        assert!(eval::<bool>(
            &vm,
            r#"return BotSave.track({ save_count = "1" }, { count = 1, slot = 0 }, 7) == nil"#
        ));
        assert!(eval::<bool>(
            &vm,
            r#"return BotSave.track({ save_count = "1" }, { count = 2, slot = 1 }, 9).save_slot == 1"#
        ));
        let latest: String = eval(
            &vm,
            r#"local l = BotSave.latest({ save_count = "2", save_slot = "1", save_room = "R",
                   save_tick = "900", save_progress = "9" })
               return string.format("%d|%d|%s|%d|%d", l.count, l.slot, l.room, l.tick, l.progress)"#,
        );
        assert_eq!(latest, "2|1|R|900|9");
        assert!(eval::<bool>(&vm, "return BotSave.latest({}) == nil"));
        // The first empty slot, then the slots in turn.
        assert_eq!(
            eval::<i64>(&vm, "return BotSave.pick_slot({ true, true, false }, 2)"),
            2
        );
        assert_eq!(
            eval::<i64>(
                &vm,
                "return BotSave.pick_slot({ true, true, true, true, true, true, true, true }, 10)"
            ),
            2
        );
    }

    #[test]
    fn the_runtime_counts_saves_and_deaths_and_a_reload_restores_the_save_memory() {
        let temp = TempDir::new("saves");
        let bot = temp.path.join("bot");
        fs::create_dir_all(&bot).unwrap();
        fs::write(bot.join("init.lua"), b"function on_bot_tick(bot) end\n").unwrap();
        let pack = Pack::open(&temp.path).unwrap();
        let mut runtime = BotRuntime::load(&pack).unwrap();
        let mut snapshot = world::tests::snapshot(0x100, 0, 0);
        runtime.tick = 10;
        assert!(!runtime.observe_saves(&snapshot));
        assert!(!runtime.has_save());
        // The save screen wrote slot 3.
        snapshot.saves.written = 1;
        snapshot.saves.slot = Some(3);
        assert!(runtime.observe_saves(&snapshot));
        assert!(!runtime.observe_saves(&snapshot), "one save counts once");
        assert_eq!(
            runtime.save_log.last,
            Some(SaveRecord {
                slot: Some(3),
                room: snapshot.room,
                tick: 10
            })
        );
        runtime
            .shares
            .memory
            .borrow_mut()
            .remember("save_slot".into(), Some("3".into()));
        runtime.checkpoint = Some(runtime.shares.memory.borrow().clone());
        // Later knowledge and a death.
        runtime
            .shares
            .memory
            .borrow_mut()
            .remember("later".into(), Some("x".into()));
        snapshot.scene.dead = true;
        runtime.observe_saves(&snapshot);
        runtime.observe_saves(&snapshot);
        assert_eq!(
            runtime.save_log.deaths, 1,
            "a death counts on its first tick"
        );
        assert_eq!(runtime.save_log.deaths_since_save, 1);
        // The bot drove the load screen: the next session is its reload.
        runtime.tick_screen(BotScreen::Load(None)).unwrap();
        runtime.start_session(&pack).unwrap();
        let memory = runtime.shares.memory.borrow();
        assert_eq!(
            memory.values.get("save_slot").map(String::as_str),
            Some("3")
        );
        assert!(
            !memory.values.contains_key("later"),
            "post-save memory is dropped"
        );
        drop(memory);
        assert_eq!(runtime.save_log.reloads, 1);
        assert_eq!(runtime.save_log.count, 1);
        assert!(runtime.has_save());
        let json = runtime.stats_json("finished", 0.0, RunFacts::default());
        assert_eq!(json["saves"], 1);
        assert_eq!(json["reloads"], 1);
        assert_eq!(json["deaths_before_reload"], 1);
        // A session the bot did not load is a fresh run.
        runtime.start_session(&pack).unwrap();
        assert!(!runtime.has_save());
        assert_eq!(runtime.save_log, SaveLog::default());
    }
}
