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

/// The default headless tick budget (about 55 minutes at 30 Hz).
pub const DEFAULT_MAX_TICKS: u64 = 100_000;

/// The bot run options the CLI adds on top of a normal boot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BotOptions {
    /// Drive the game from the pack's `bot/**.lua` entries.
    pub enabled: bool,
    /// Run without a window or audio, as fast as the simulation allows.
    pub headless: bool,
    /// Headless app-update budget; ignored by the interactive path.
    pub max_ticks: u64,
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
    /// A room item action the script asked the host to run this tick. This is
    /// the same handler a normal action press on that item would run; it is
    /// used for scripted handovers whose zone is not reachable (the 106
    /// lockpick item sits at the room origin).
    pub room_action: Option<(u8, u8)>,
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

    /// Ask the host to run the item action in `slot` this tick.
    fn use_action(&mut self, slot: u8, handler: u8) {
        self.room_action = Some((slot, handler));
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

/// The display name of a room action kind.
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
        Self {
            room: id,
            camera: session.loaded.room.current_cut,
            player,
            scene,
            actions,
            doors,
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
/// The heading error at or below which the walk starts moving.
const FORWARD_DEADBAND: i32 = 0x40;
/// The heading error at or below which the run may start.
const RUN_START_DEADBAND: i32 = 0x60;
/// The heading error at or below which a running bot steers while moving
/// (the engine's run turn is 0x30 a tick) instead of stopping to turn.
const RUN_TURN_DEADBAND: i32 = 0x100;
/// The heading error above which the run latch drops at a sharp corner.
const RUN_STOP_DEADBAND: i32 = 0x1C0;
/// Remaining distance above which a run may start.
const RUN_START_DISTANCE: i32 = 800;
/// Remaining distance at or below which a run walks the final approach.
const RUN_STOP_DISTANCE: i32 = 400;
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
    /// Whether the run is latched: once picked up it survives moderate
    /// heading errors as a run-turn, and drops at sharp corners and on the
    /// final approach.
    running: bool,
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

#[derive(Clone, Copy, Debug)]
struct NavOutcome {
    forward: bool,
    run: bool,
    turn: Turn,
    status: &'static str,
    distance: i32,
    angle_error: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Movement {
    forward: bool,
    run: bool,
    turn: Turn,
    /// The run latch after this tick.
    running: bool,
}

/// Decide the pad for one navigation tick.
///
/// The run is latched: it starts only on a small heading error at a worthwhile
/// distance, then survives moderate errors as a run-turn. A sharp corner drops
/// the latch and stops for an in-place turn; the final approach always walks.
fn movement_for(error: i32, distance: i32, allow_run: bool, running: bool) -> Movement {
    let abs = error.abs();
    let running = running && allow_run && distance > RUN_STOP_DISTANCE && abs <= RUN_STOP_DEADBAND;
    let running =
        running || (allow_run && distance > RUN_START_DISTANCE && abs <= RUN_START_DEADBAND);
    if running {
        let turn = turn_for(error, FORWARD_DEADBAND);
        if abs <= RUN_TURN_DEADBAND {
            return Movement {
                forward: true,
                run: true,
                turn,
                running: true,
            };
        }
        // Stop for the sharp part of the turn but keep the latch: the run
        // resumes as soon as the heading is back inside the run arc.
        return Movement {
            forward: false,
            run: false,
            turn,
            running: true,
        };
    }
    let turn = turn_for(error, FORWARD_DEADBAND);
    Movement {
        forward: turn == Turn::None,
        run: false,
        turn,
        running: false,
    }
}

/// The options `bot:walk_to(x, z, opts)` accepts.
#[derive(Clone, Copy, Debug)]
struct WalkOptions {
    run: bool,
    tolerance: i32,
}

impl Default for WalkOptions {
    fn default() -> Self {
        Self {
            run: false,
            tolerance: DEFAULT_TOLERANCE,
        }
    }
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

fn turn_for(error: i32, deadband: i32) -> Turn {
    if error > deadband {
        Turn::Right
    } else if error < -deadband {
        Turn::Left
    } else {
        Turn::None
    }
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
fn find_grid_path(
    room: &RoomState,
    start: (i32, i32),
    goal: (i32, i32),
    y: i32,
    radius: i32,
    flags: u8,
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
            let next_cost = cost[index].saturating_add(step_cost);
            if next_cost < cost[next] {
                cost[next] = next_cost;
                parent[next] = index;
                open.push((Reverse(next_cost + heuristic(next)), next));
            }
        }
    }
    None
}

/// Greedily drop waypoints whose straight segment is clear.
fn smooth_path(
    room: &RoomState,
    start: (i32, i32),
    path: Vec<(i32, i32)>,
    y: i32,
    radius: i32,
    flags: u8,
) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    let mut anchor = start;
    let mut index = 0;
    while index < path.len() {
        let mut best = None;
        for candidate in (index..path.len()).rev() {
            if line_clear(room, anchor, path[candidate], y, radius, flags) {
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
    }

    fn clear_goal(&mut self) {
        self.goal = None;
        self.path.clear();
        self.index = 0;
        self.unreachable_goal = None;
        self.blocked_goal = None;
        self.running = false;
    }

    /// Drop the run latch without touching the path. The runtime calls this
    /// when the script stops feeding movement for a tick.
    fn clear_run(&mut self) {
        self.running = false;
    }

    /// The `walk_to` skill: collision-aware movement toward `(x, z)`.
    fn walk_to(
        &mut self,
        player: &PlayerSnapshot,
        x: i32,
        z: i32,
        opts: WalkOptions,
    ) -> NavOutcome {
        self.driving = true;
        let pos = (player.x, player.z);
        let goal = (x, z);
        let distance = distance_xz(pos, goal);
        if distance <= opts.tolerance {
            self.clear_goal();
            return NavOutcome {
                forward: false,
                run: false,
                turn: Turn::None,
                status: "arrived",
                distance,
                angle_error: 0,
            };
        }
        if self.goal != Some(goal) || self.tolerance != opts.tolerance {
            self.goal = Some(goal);
            self.tolerance = opts.tolerance;
            self.path.clear();
            self.index = 0;
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
            self.path.clear();
            self.index = 0;
            self.stuck = 0;
            self.replans = 0;
            self.unreachable_goal = None;
            self.blocked_goal = None;
        }
        if self.blocked_goal == Some(goal) {
            self.running = false;
            return NavOutcome {
                forward: false,
                run: false,
                turn: Turn::None,
                status: "blocked",
                distance,
                angle_error: 0,
            };
        }
        if self.unreachable_goal == Some(goal) {
            self.running = false;
            return NavOutcome {
                forward: false,
                run: false,
                turn: Turn::None,
                status: "unreachable",
                distance,
                angle_error: 0,
            };
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
            self.path.clear();
            self.index = 0;
            self.running = false;
            if self.replans > MAX_REPLANS {
                self.blocked_goal = Some(goal);
                return NavOutcome {
                    forward: false,
                    run: false,
                    turn: Turn::None,
                    status: "blocked",
                    distance,
                    angle_error: 0,
                };
            }
        }
        if self.path.is_empty() || self.index >= self.path.len() {
            let found = find_grid_path(
                &self.collision,
                pos,
                goal,
                player.y,
                player.radius,
                player.collision_flags,
            );
            let Some(path) = found else {
                self.unreachable_goal = Some(goal);
                self.path.clear();
                self.index = 0;
                self.running = false;
                return NavOutcome {
                    forward: false,
                    run: false,
                    turn: Turn::None,
                    status: "unreachable",
                    distance,
                    angle_error: 0,
                };
            };
            self.path = smooth_path(
                &self.collision,
                pos,
                path,
                player.y,
                player.radius,
                player.collision_flags,
            );
            self.index = 0;
        }
        while self.index < self.path.len()
            && distance_xz(pos, self.path[self.index]) <= WAYPOINT_RADIUS
        {
            self.index += 1;
        }
        // Pure pursuit: of the path points already inside the lookahead,
        // steer at the farthest one the player can see. The smoothed path's
        // points are corners, and line of sight proves a later one is safe to
        // skip, so the bot arcs through the corner instead of stopping on it.
        if self.index < self.path.len() {
            let mut best = self.index;
            let mut best_distance = distance_xz(pos, self.path[self.index]);
            for candidate in self.index + 1..self.path.len() {
                let distance = distance_xz(pos, self.path[candidate]);
                if distance > LOOKAHEAD_DISTANCE {
                    break;
                }
                if distance >= best_distance
                    && line_clear(
                        &self.collision,
                        pos,
                        self.path[candidate],
                        player.y,
                        player.radius,
                        player.collision_flags,
                    )
                {
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
                self.path.clear();
                self.index = 0;
                self.running = false;
                return NavOutcome {
                    forward: false,
                    run: false,
                    turn: Turn::None,
                    status: "unreachable",
                    distance,
                    angle_error: 0,
                };
            }
        }
        let target = self.path[self.index];
        let target_angle = crate::sfx::angle_between_xz(pos.0, pos.1, target.0, target.1);
        let error = angle_error(player.angle, target_angle);
        let movement = movement_for(error, distance, opts.run, self.running);
        self.running = movement.running;
        self.last_forward = movement.forward;
        NavOutcome {
            forward: movement.forward,
            run: movement.run,
            turn: movement.turn,
            status: "moving",
            distance,
            angle_error: error,
        }
    }

    fn face_to(&mut self, player: &PlayerSnapshot, x: i32, z: i32) -> NavOutcome {
        let pos = (player.x, player.z);
        let target = crate::sfx::angle_between_xz(pos.0, pos.1, x, z);
        let error = angle_error(player.angle, target);
        // The in-place turn step is 0x60, so a deadband below 0x30 would
        // oscillate on a heading that lands half a step away.
        let turn = turn_for(error, FORWARD_DEADBAND);
        self.goal = None;
        self.path.clear();
        self.index = 0;
        self.last_forward = false;
        self.running = false;
        NavOutcome {
            forward: false,
            run: false,
            turn,
            status: if turn == Turn::None {
                "facing"
            } else {
                "turning"
            },
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
    tick: u64,
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

        methods.add_method("use_action", |_lua, bot, slot: u8| {
            let Some(action) = bot.snapshot.actions.iter().find(|a| a.slot == slot) else {
                return Err(mlua::Error::RuntimeError(format!(
                    "no room action in slot {slot}"
                )));
            };
            if action.kind != "item" {
                return Err(mlua::Error::RuntimeError(format!(
                    "room action slot {slot} is `{}`, not an item",
                    action.kind
                )));
            }
            bot.input().use_action(slot, action.handler);
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
                }
                let outcome = bot.nav().walk_to(&bot.snapshot.player, x, z, walk);
                outcome.apply(&mut bot.input());
                outcome_table(lua, outcome)
            },
        );

        methods.add_method("face_to", |lua, bot, (x, z): (i32, i32)| {
            let outcome = bot.nav().face_to(&bot.snapshot.player, x, z);
            outcome.apply(&mut bot.input());
            outcome_table(lua, outcome)
        });

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
        let (lua, budget) = crate::lua::new_sandboxed_state()?;
        for path in &paths {
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
        let globals = lua.globals();
        let on_tick = globals
            .get::<Function>("on_bot_tick")
            .map_err(|err| anyhow!("bot chunk(s) must define `on_bot_tick(bot)`: {err}"))?;
        Ok(Self {
            lua,
            budget,
            on_tick,
        })
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

/// The live bot: its VM, remembered state and navigation cache.
pub struct BotRuntime {
    vm: BotVm,
    shares: BotShares,
    tick: u64,
}

impl BotRuntime {
    /// Load the pack's bot chunks.
    pub fn load(pack: &Pack) -> Result<Self> {
        Ok(Self {
            vm: BotVm::load(pack)?,
            shares: BotShares::default(),
            tick: 0,
        })
    }

    /// Run one `on_bot_tick`, returning the input it asked for.
    ///
    /// The tick counter advances even when the script fails, so the error can
    /// name the exact tick and room.
    pub fn tick(&mut self, session: &GameSession) -> Result<BotInput> {
        self.tick = self.tick.saturating_add(1);
        let snapshot = SessionSnapshot::build(session);
        {
            let mut input = self.shares.input.borrow_mut();
            input.reset();
            let mut nav = self.shares.nav.borrow_mut();
            nav.sync_room(session.loaded.id, &session.loaded.room);
            nav.driving = false;
            let mut memory = self.shares.memory.borrow_mut();
            memory.observe(&snapshot);
        }
        let ctx = BotCtx {
            snapshot,
            tick: self.tick,
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
        let mut nav = self.shares.nav.borrow_mut();
        if !nav.driving {
            // The script did not drive movement this tick (a scene, a door
            // press, a `face_to`): do not resume a stale run next tick.
            nav.clear_run();
        }
        drop(nav);
        Ok(input)
    }

    /// Clear the remembered state and tick count for a new game.
    pub fn reset(&mut self) {
        self.shares = BotShares::default();
        self.tick = 0;
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

    pub fn room_path(&self) -> Vec<String> {
        self.shares.memory.borrow().room_path.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::CollisionRect;

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

    #[test]
    fn use_action_is_a_one_tick_room_action_request() {
        let mut input = BotInput::default();
        input.use_action(6, 4);
        assert_eq!(input.room_action, Some((6, 4)));
        input.reset();
        assert_eq!(input.room_action, None);
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

    #[test]
    fn run_latch_starts_only_on_a_small_error_at_distance() {
        assert_eq!(
            movement_for(0, 2000, true, false),
            Movement {
                forward: true,
                run: true,
                turn: Turn::None,
                running: true,
            }
        );
        // A moderate error is a run-turn, not a stop.
        assert_eq!(
            movement_for(0x60, 2000, true, false),
            Movement {
                forward: true,
                run: true,
                turn: Turn::Right,
                running: true,
            }
        );
        // Too sharp to start, and too near to be worthwhile: walk.
        assert_eq!(
            movement_for(0x80, 2000, true, false),
            Movement {
                forward: false,
                run: false,
                turn: Turn::Right,
                running: false,
            }
        );
        assert_eq!(
            movement_for(0x100, 2000, true, false),
            Movement {
                forward: false,
                run: false,
                turn: Turn::Right,
                running: false,
            }
        );
        assert_eq!(
            movement_for(0, 800, true, false),
            Movement {
                forward: true,
                run: false,
                turn: Turn::None,
                running: false,
            }
        );
        assert_eq!(
            movement_for(0, 2000, false, true),
            Movement {
                forward: true,
                run: false,
                turn: Turn::None,
                running: false,
            }
        );
    }

    #[test]
    fn run_latch_survives_moderate_errors_and_drops_at_sharp_ones() {
        assert_eq!(
            movement_for(0x100, 2000, true, true),
            Movement {
                forward: true,
                run: true,
                turn: Turn::Right,
                running: true,
            }
        );
        // Too sharp to steer while running, but not sharp enough to drop the
        // latch: stop and turn in place, then resume the run.
        assert_eq!(
            movement_for(0x140, 2000, true, true),
            Movement {
                forward: false,
                run: false,
                turn: Turn::Right,
                running: true,
            }
        );
        assert_eq!(
            movement_for(0x1D0, 2000, true, true),
            Movement {
                forward: false,
                run: false,
                turn: Turn::Right,
                running: false,
            }
        );
        // Inside the final-approach distance the run walks even with a clear
        // heading, and drops the latch.
        assert_eq!(
            movement_for(0, 300, true, true),
            Movement {
                forward: true,
                run: false,
                turn: Turn::None,
                running: false,
            }
        );
    }

    #[test]
    fn walk_keeps_the_run_latch_through_an_in_place_turn() {
        let room = RoomState::default();
        let mut player = PlayerSnapshot {
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
        let first = nav.walk_to(
            &player,
            20000,
            16000,
            WalkOptions {
                run: true,
                tolerance: DEFAULT_TOLERANCE,
            },
        );
        assert!(
            first.forward && first.run,
            "the first tick picks up the run"
        );
        assert!(nav.running);
        // A sharp heading error stops for an in-place turn without dropping
        // the latch; the run resumes when the heading comes back inside the
        // run arc.
        player.angle = 0xEC0;
        let turn = nav.walk_to(
            &player,
            20000,
            16000,
            WalkOptions {
                run: true,
                tolerance: DEFAULT_TOLERANCE,
            },
        );
        assert!(!turn.forward);
        assert!(!turn.run);
        assert_eq!(turn.turn, Turn::Right);
        assert!(nav.running, "the latch survives the in-place turn");
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
}
