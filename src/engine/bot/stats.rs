//! Run statistics for the headless bot (`--bot-stats <path>`).
//!
//! Everything here is derived on the Rust side from the per-tick session
//! snapshot and the pad input the Lua route asked for, so it measures the
//! route without any cooperation from the script.
//!
//! Proxies (the engine has no counter for them):
//! - `shots_fired` counts decreases of the equipped weapon's loaded rounds.
//! - `enemy_hit_events` counts ticks where an enemy (same slot and id as the
//!   previous tick) lost health; it includes the knife and any other damage.
//! - `turn_in_place_ticks` counts ticks with left/right held, neither up nor
//!   down and no aim button.
//!
//! `saves` counts the saves the session's save screen reported writing,
//! `reloads` the sessions the bot started by loading its latest save from
//! the title's load screen, and `deaths_before_reload` the reloads that
//! followed a death.
//!
//! The `menu` counters come from the engine's menu state between two
//! consecutive snapshots, never from what the route pressed:
//! - `opens`: the inventory panel appeared (START); `viewer_opens`: the
//!   menu slot appeared under a pick-up viewer or item box instead;
//! - `cursor_moves` / `option_moves`: the menu's cursor byte or the
//!   submenu's highlighted option changed while the menu stayed open;
//! - `item_uses`: an item in the inventory lost quantity while the menu was
//!   open on the previous tick (`heal_uses` counts the heal-kind ones);
//! - `equips`: the equipped item changed while the menu was open;
//! - `box_deposits` / `box_withdrawals`: an item box slot went from empty to
//!   an item / from an item to empty while the box screen was up on both
//!   ticks (a swap of two items counts one of each);
//! - `combines`: the inventory changed while the menu was in its MOVE mode
//!   on the previous tick (a mix). Neither a box swap nor a mix counts as an
//!   item use.
//!
//! `activity` puts every tick in exactly one bucket, the first that applies:
//! `saving` (typewriter or save screen), `item_box`, `pickup_viewer`,
//! `examine`, `menu` (the inventory panel), `doors` (a room transition), `scenes`
//! (films, skippable scenes and messages), `pickup`, `combat` (aim or fire
//! held, or the player grabbed), `walking` (up or down held), `turning`
//! (left or right only), `uncontrolled` (no control for another reason) and
//! `waiting` (in control with no input).
//!
//! `hits` counts the ticks where the player's health fell by more than
//! [`HIT_THRESHOLD`] (poison and web ticks take 2 and are not hits), in all
//! and per room; a death's reload restores health and is no hit.
//!
//! `compute` is the bot's own cost per tick: the wall-clock time of the
//! snapshot, the Lua route and the path queries, in microseconds. A 30 Hz
//! tick has 33.3 ms for the whole frame, so a bot tick over a few
//! milliseconds can show as a stutter in the window; `over_ms` counts the
//! ticks over each bound and `worst` lists the slowest ones. `phases_secs`
//! splits the total by tick phase ([`PHASES`], with the script phase split
//! again into the route's own Lua and its API calls) and `api` lists the
//! API methods by the time their calls took. Unlike the rest of the report
//! it depends on the machine and its load.
//!
//! `score` gathers the numbers a playthrough is judged by: whether the game
//! was completed, the ticks it took, the game's own clear time, hits,
//! deaths, room loads and the compute load.
//!
//! `timeline` is the run-length encoding of the buckets, `[start tick,
//! length, bucket]` rows, for `scripts/bot-idle-report.py` to join with the
//! log. `stalled_walk_ticks` counts ticks after a walking tick that left
//! the player's position unchanged (a push into a wall or an object).

use super::{BotInput, EnemySnapshot, MenuSnapshot, SessionSnapshot};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// One locked door that became unlocked, and how long the route then took to
/// change room.
#[derive(Clone, Debug)]
struct UnlockRecord {
    room: String,
    slot: u8,
    unlock_tick: u64,
    ticks_to_room_change: Option<u64>,
}

/// A health drop above this is a hit (see the module docs).
const HIT_THRESHOLD: i16 = 2;
/// The bounds, in milliseconds, `compute.over_ms` counts bot ticks over.
const COMPUTE_BOUNDS_MS: [u32; 5] = [1, 4, 8, 16, 33];
/// How many of the slowest bot ticks `compute.worst` lists.
const COMPUTE_WORST: usize = 10;

/// The phases of a bot tick `compute.phases_secs` splits the cost into:
/// building the snapshot, observing it (statistics, navigation and the
/// world model), the Lua route (with the API calls it makes) and the
/// bookkeeping after it.
pub(super) const PHASES: [&str; 4] = ["snapshot", "observe", "script", "finish"];

/// What the Lua route's calls into the bot API cost, by method.
#[derive(Debug, Default)]
pub(super) struct ApiProfile {
    /// Per method: calls, total nanoseconds, the slowest call's nanoseconds.
    methods: BTreeMap<&'static str, (u64, u64, u64)>,
}

impl ApiProfile {
    pub(super) fn note(&mut self, name: &'static str, elapsed: std::time::Duration) {
        let nanos = u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX);
        let entry = self.methods.entry(name).or_default();
        entry.0 += 1;
        entry.1 += nanos;
        entry.2 = entry.2.max(nanos);
    }

    /// The methods by total time, costliest first, and their sum in seconds.
    fn to_json(&self) -> (Value, f64) {
        let mut rows: Vec<_> = self.methods.iter().collect();
        rows.sort_by(|a, b| b.1.1.cmp(&a.1.1).then(a.0.cmp(b.0)));
        let total: u64 = rows.iter().map(|(_, cost)| cost.1).sum();
        let rows: Vec<Value> = rows
            .into_iter()
            .map(|(name, (calls, nanos, max))| {
                json!({
                    "method": name,
                    "calls": calls,
                    "secs": *nanos as f64 / 1e9,
                    "max_us": max / 1000,
                })
            })
            .collect();
        (Value::Array(rows), total as f64 / 1e9)
    }
}

/// The bot's compute cost per tick (see the module docs).
#[derive(Debug, Default)]
struct ComputeLoad {
    /// The total nanoseconds per phase ([`PHASES`]).
    phases: [u64; 4],
    /// Every tick's cost in microseconds, in tick order.
    micros: Vec<u32>,
    /// The slowest ticks so far: `(microseconds, tick, room)`, slowest first.
    worst: Vec<(u32, u64, String)>,
}

impl ComputeLoad {
    fn note(&mut self, tick: u64, room: Option<&str>, micros: u32) {
        self.micros.push(micros);
        if self.worst.len() < COMPUTE_WORST
            || self.worst.last().is_some_and(|(least, ..)| micros > *least)
        {
            self.worst
                .push((micros, tick, room.unwrap_or_default().to_string()));
            self.worst.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
            self.worst.truncate(COMPUTE_WORST);
        }
    }

    fn to_json(&self, api: &ApiProfile) -> Value {
        if self.micros.is_empty() {
            return Value::Null;
        }
        let (api, api_secs) = api.to_json();
        let mut phases: BTreeMap<&str, f64> = PHASES
            .iter()
            .zip(self.phases)
            .map(|(name, nanos)| (*name, nanos as f64 / 1e9))
            .collect();
        // The route's own Lua: the script phase less its API calls.
        phases.insert("script_lua", phases["script"] - api_secs);
        phases.insert("script_api", api_secs);
        let mut sorted = self.micros.clone();
        sorted.sort_unstable();
        let percentile = |p: usize| sorted[(sorted.len() - 1) * p / 1000];
        let total: u64 = sorted.iter().map(|&us| u64::from(us)).sum();
        let over: BTreeMap<String, usize> = COMPUTE_BOUNDS_MS
            .iter()
            .map(|&ms| {
                let bound = ms * 1000;
                (
                    ms.to_string(),
                    sorted.len() - sorted.partition_point(|&us| us <= bound),
                )
            })
            .collect();
        let worst: Vec<Value> = self
            .worst
            .iter()
            .map(|(us, tick, room)| json!({ "us": us, "tick": tick, "room": room }))
            .collect();
        json!({
            "ticks": sorted.len(),
            "min_us": sorted[0],
            "avg_us": total / sorted.len() as u64,
            "p50_us": percentile(500),
            "p99_us": percentile(990),
            "p999_us": percentile(999),
            "max_us": sorted[sorted.len() - 1],
            "total_secs": total as f64 / 1e6,
            "over_ms": over,
            "worst": worst,
            "phases_secs": phases,
            "api": api,
        })
    }
}

/// The accumulated run statistics.
#[derive(Debug, Default)]
pub(super) struct BotStats {
    last_tick: u64,
    room_entries: Vec<(u64, String)>,
    current_room: Option<String>,
    first_health: Option<i16>,
    last_health: Option<i16>,
    min_health: Option<i16>,
    damage_taken: u64,
    /// Health drops above [`HIT_THRESHOLD`], in all and per room.
    hits: u32,
    room_hits: BTreeMap<String, u32>,
    /// The hits by the kind of the nearest living enemy (`"06"`; `"none"`
    /// with no enemy in the room: poison, a trap).
    enemy_hits: BTreeMap<String, u32>,
    /// The most living enemies of each kind seen at once.
    enemy_kinds: BTreeMap<String, u32>,
    /// The bot's compute cost per tick.
    compute: ComputeLoad,
    deaths: u32,
    was_dead: bool,
    /// Saves the typewriter screen wrote.
    saves: u32,
    /// Saves the bot loaded through the title's load screen.
    reloads: u32,
    /// Deaths the bot answered with a reload.
    deaths_before_reload: u32,
    fire_presses: u64,
    fire_held_ticks: u64,
    shots_fired: u64,
    enemy_hit_events: u64,
    enemy_damage_dealt: u64,
    turn_in_place_ticks: u64,
    move_ticks: u64,
    message_ticks: u64,
    scene_ticks: u64,
    transition_ticks: u64,
    pickup_ticks: u64,
    uncontrollable_ticks: u64,
    prev_weapon: Option<(u8, Option<u8>, u32)>,
    prev_enemies: BTreeMap<(u8, u8), i16>,
    /// Doors of the current room that are locked, by slot, with the bank-2 bit.
    locked_doors: BTreeMap<u8, u8>,
    unlocks: Vec<UnlockRecord>,
    menu: MenuCounters,
    /// The previous tick's menu state and equipped item.
    prev_menu: Option<(MenuSnapshot, Option<u8>)>,
    /// Ticks per activity bucket (see the module docs).
    activity: BTreeMap<&'static str, u64>,
    /// The same buckets per room.
    room_activity: BTreeMap<String, BTreeMap<&'static str, u64>>,
    /// This tick's bucket when the snapshot alone decides it, else `None`
    /// with the snapshot facts the input still needs.
    pending_activity: Option<PendingActivity>,
    /// Run-length timeline of the buckets: `(start tick, length, bucket)`.
    timeline: Vec<(u64, u64, &'static str)>,
    /// The player's position this tick, and whether the previous tick held
    /// up or down while in control (`stalled_walk_ticks` counts the ticks
    /// that moved the pad but not the player).
    pos: (i32, i32),
    prev_pos: Option<(i32, i32)>,
    prev_walk: bool,
    stalled_walk_ticks: u64,
    /// Run-length rows `(start tick, length)` of the stalled walking ticks.
    stalled_runs: Vec<(u64, u64)>,
}

/// What the snapshot said about the tick, before the input is known.
#[derive(Clone, Copy, Debug)]
enum PendingActivity {
    Fixed(&'static str),
    Controlled { grabbed: bool, controllable: bool },
}

/// The bucket the snapshot alone decides, if any.
fn snapshot_activity(snapshot: &SessionSnapshot) -> PendingActivity {
    let scene = &snapshot.scene;
    let menu = &snapshot.menu;
    let fixed = if snapshot.save_screen.is_some() || scene.typewriter {
        Some("saving")
    } else if menu.item_box.is_some() || menu.screen == Some("item_box") {
        Some("item_box")
    } else if menu.open {
        Some(match menu.screen {
            Some("pickup") => "pickup_viewer",
            Some("examine") => "examine",
            _ => "menu",
        })
    } else if scene.transition {
        Some("doors")
    } else if scene.film || scene.skippable || scene.cutscene_skip || scene.message.is_some() {
        Some("scenes")
    } else if scene.pickup || scene.reader {
        Some("pickup")
    } else {
        None
    };
    match fixed {
        Some(bucket) => PendingActivity::Fixed(bucket),
        None => PendingActivity::Controlled {
            grabbed: snapshot.player.grabbed,
            controllable: scene.controllable,
        },
    }
}

/// Counted menu transitions (see the module docs).
#[derive(Debug, Default)]
struct MenuCounters {
    opens: u64,
    viewer_opens: u64,
    open_ticks: u64,
    cursor_moves: u64,
    option_moves: u64,
    item_uses: u64,
    heal_uses: u64,
    equips: u64,
    box_deposits: u64,
    box_withdrawals: u64,
    combines: u64,
}

impl MenuCounters {
    fn observe(
        &mut self,
        prev: Option<&(MenuSnapshot, Option<u8>)>,
        now: &MenuSnapshot,
        equipped: Option<u8>,
    ) {
        self.open_ticks += u64::from(now.open);
        let Some((before, was_equipped)) = prev else {
            return;
        };
        if now.open && !before.open {
            // A pick-up viewer or item box opens the same menu slot with its
            // own screen already over it; START opens the bare panel.
            if now.screen.is_none() {
                self.opens += 1;
            } else {
                self.viewer_opens += 1;
            }
        }
        if !before.open {
            return;
        }
        if now.open {
            if now.cursor != before.cursor {
                self.cursor_moves += 1;
            }
            if now.option != before.option && now.mode == Some("actions") {
                self.option_moves += 1;
            }
        }
        if let (Some(was), Some(box_now)) = (&before.item_box, &now.item_box) {
            for (old, new) in was.slots.iter().zip(&box_now.slots) {
                if old.0 == 0 && new.0 != 0 {
                    self.box_deposits += 1;
                } else if old.0 != 0 && new.0 == 0 {
                    self.box_withdrawals += 1;
                } else if old.0 != new.0 {
                    self.box_deposits += 1;
                    self.box_withdrawals += 1;
                }
            }
        }
        if equipped != *was_equipped {
            self.equips += 1;
        }
        if before.item_box.is_some() || now.item_box.is_some() {
            return;
        }
        let stacks = |menu: &MenuSnapshot| {
            menu.items
                .iter()
                .map(|item| (item.id, item.quantity))
                .collect::<Vec<_>>()
        };
        if before.mode == Some("move") && stacks(before) != stacks(now) {
            self.combines += 1;
            return;
        }
        let held = |menu: &MenuSnapshot| {
            let mut counts: BTreeMap<u8, (u32, &'static str)> = BTreeMap::new();
            for item in menu.items.iter().filter(|item| item.id != 0) {
                let entry = counts.entry(item.id).or_insert((0, item.kind));
                entry.0 += u32::from(item.quantity);
            }
            counts
        };
        let after = held(now);
        for (id, (quantity, kind)) in held(before) {
            if after.get(&id).map_or(0, |entry| entry.0) < quantity {
                self.item_uses += 1;
                if kind == "heal" {
                    self.heal_uses += 1;
                }
            }
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "opens": self.opens,
            "viewer_opens": self.viewer_opens,
            "open_ticks": self.open_ticks,
            "cursor_moves": self.cursor_moves,
            "option_moves": self.option_moves,
            "item_uses": self.item_uses,
            "heal_uses": self.heal_uses,
            "equips": self.equips,
            "box_deposits": self.box_deposits,
            "box_withdrawals": self.box_withdrawals,
            "combines": self.combines,
        })
    }
}

impl BotStats {
    /// What bot tick `tick` cost to compute, in microseconds.
    pub(super) fn note_compute(&mut self, tick: u64, micros: u32) {
        self.compute
            .note(tick, self.current_room.as_deref(), micros);
    }

    /// Add `elapsed` to phase `phase` of [`PHASES`].
    pub(super) fn note_phase(&mut self, phase: usize, elapsed: std::time::Duration) {
        self.compute.phases[phase] += u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX);
    }

    /// A save the save screen completed.
    pub(super) fn note_save(&mut self) {
        self.saves += 1;
    }

    /// A reload of the latest save; `after_death` when the player had died.
    pub(super) fn note_reload(&mut self, after_death: bool) {
        self.reloads += 1;
        self.deaths_before_reload += u32::from(after_death);
    }

    /// Observe the snapshot taken at the start of bot tick `tick`.
    pub(super) fn observe_snapshot(&mut self, tick: u64, snapshot: &SessionSnapshot) {
        self.last_tick = tick;
        let room = snapshot.room.room3();
        if self.current_room.as_deref() != Some(room.as_str()) {
            for record in &mut self.unlocks {
                if record.ticks_to_room_change.is_none() {
                    record.ticks_to_room_change = Some(tick - record.unlock_tick);
                }
            }
            self.current_room = Some(room.clone());
            self.room_entries.push((tick, room.clone()));
            self.prev_enemies.clear();
            self.locked_doors = snapshot
                .doors
                .iter()
                .filter(|door| door.lock >= 0x80 && door.key != 0)
                .filter(|door| !snapshot.flag(2, door.lock % 64))
                .map(|door| (door.slot, door.lock % 64))
                .collect();
        }
        let unlocked: Vec<u8> = self
            .locked_doors
            .iter()
            .filter(|(_, bit)| snapshot.flag(2, **bit))
            .map(|(slot, _)| *slot)
            .collect();
        for slot in unlocked {
            self.locked_doors.remove(&slot);
            self.unlocks.push(UnlockRecord {
                room: room.clone(),
                slot,
                unlock_tick: tick,
                ticks_to_room_change: None,
            });
        }

        let health = snapshot.player.health;
        if let Some(last) = self.last_health
            && health < last
        {
            self.damage_taken += (last - health) as u64;
            if last - health > HIT_THRESHOLD {
                self.hits += 1;
                *self.room_hits.entry(room.clone()).or_default() += 1;
                let nearest = snapshot
                    .enemies
                    .iter()
                    .filter(|enemy| enemy.health > 0)
                    .min_by_key(|enemy| enemy.distance)
                    .map_or_else(|| "none".to_string(), |enemy| format!("{:02X}", enemy.id));
                *self.enemy_hits.entry(nearest).or_default() += 1;
            }
        }
        self.first_health.get_or_insert(health);
        self.last_health = Some(health);
        self.min_health = Some(self.min_health.map_or(health, |m| m.min(health)));
        // The escape scene ends the game through the death machine (its
        // scripted variant): that is the ending, not a death.
        let dead = (snapshot.scene.dead || health <= 0) && !snapshot.scene.ending;
        if dead && !self.was_dead {
            self.deaths += 1;
        }
        self.was_dead = dead;

        let scene = &snapshot.scene;
        self.message_ticks += u64::from(scene.message.is_some());
        self.scene_ticks += u64::from(scene.skippable || scene.cutscene_skip || scene.film);
        self.transition_ticks += u64::from(scene.transition);
        self.pickup_ticks += u64::from(scene.pickup);
        self.uncontrollable_ticks += u64::from(!scene.controllable);

        let weapon = (
            snapshot.weapon.weapon,
            snapshot.weapon.equipped,
            snapshot.weapon.clip,
        );
        if let Some(prev) = self.prev_weapon
            && prev.0 == weapon.0
            && prev.1 == weapon.1
            && weapon.2 < prev.2
        {
            self.shots_fired += u64::from(prev.2 - weapon.2);
        }
        self.prev_weapon = Some(weapon);

        let mut now = BTreeMap::new();
        for EnemySnapshot {
            slot, id, health, ..
        } in &snapshot.enemies
        {
            if let Some(before) = self.prev_enemies.get(&(*slot, *id))
                && health < before
            {
                self.enemy_hit_events += 1;
                self.enemy_damage_dealt += (*before - *health) as u64;
            }
            now.insert((*slot, *id), *health);
        }
        self.prev_enemies = now;
        let mut living: BTreeMap<u8, u32> = BTreeMap::new();
        for enemy in snapshot.enemies.iter().filter(|enemy| enemy.health > 0) {
            *living.entry(enemy.id).or_default() += 1;
        }
        for (id, count) in living {
            let most = self.enemy_kinds.entry(format!("{id:02X}")).or_default();
            *most = (*most).max(count);
        }

        self.pending_activity = Some(snapshot_activity(snapshot));
        self.pos = (snapshot.player.x, snapshot.player.z);
        if self.prev_walk && self.prev_pos == Some(self.pos) {
            self.stalled_walk_ticks += 1;
            match self.stalled_runs.last_mut() {
                Some((start, len)) if *start + *len == tick => *len += 1,
                _ => self.stalled_runs.push((tick, 1)),
            }
        }

        let equipped = snapshot.player.equipped;
        self.menu
            .observe(self.prev_menu.as_ref(), &snapshot.menu, equipped);
        self.prev_menu = Some((snapshot.menu.clone(), equipped));
    }

    /// Observe the pad input the route produced this tick.
    pub(super) fn observe_input(&mut self, input: &BotInput) {
        let pad = &input.input;
        self.fire_presses += u64::from(pad.fire_pressed);
        self.fire_held_ticks += u64::from(pad.fire);
        let turning = pad.left || pad.right;
        let moving = pad.up || pad.down;
        self.move_ticks += u64::from(moving);
        self.turn_in_place_ticks += u64::from(turning && !moving && !pad.aim);
        let bucket = match self.pending_activity.take() {
            Some(PendingActivity::Fixed(bucket)) => bucket,
            Some(PendingActivity::Controlled {
                grabbed,
                controllable,
            }) => {
                if pad.aim || pad.fire || grabbed {
                    "combat"
                } else if moving {
                    "walking"
                } else if turning {
                    "turning"
                } else if !controllable {
                    "uncontrolled"
                } else {
                    "waiting"
                }
            }
            None => "uncontrolled",
        };
        *self.activity.entry(bucket).or_default() += 1;
        self.prev_walk = bucket == "walking";
        self.prev_pos = Some(self.pos);
        match self.timeline.last_mut() {
            Some((start, len, last)) if *last == bucket && *start + *len == self.last_tick => {
                *len += 1;
            }
            _ => self.timeline.push((self.last_tick, 1, bucket)),
        }
        if let Some(room) = &self.current_room {
            *self
                .room_activity
                .entry(room.clone())
                .or_default()
                .entry(bucket)
                .or_default() += 1;
        }
    }

    /// The report as JSON.
    pub(super) fn to_json(
        &self,
        outcome: &str,
        finish_reason: Option<&str>,
        ticks: u64,
        elapsed_secs: f64,
        run: RunFacts,
        api: &ApiProfile,
    ) -> Value {
        let entries: Vec<Value> = self
            .room_entries
            .iter()
            .map(|(tick, room)| json!({ "tick": tick, "room": room }))
            .collect();
        let path: Vec<&str> = self.room_entries.iter().map(|(_, r)| r.as_str()).collect();
        let unlocks: Vec<Value> = self
            .unlocks
            .iter()
            .map(|u| {
                json!({
                    "room": u.room,
                    "door_slot": u.slot,
                    "unlock_tick": u.unlock_tick,
                    "ticks_to_room_change": u.ticks_to_room_change,
                })
            })
            .collect();
        let known: Vec<u64> = self
            .unlocks
            .iter()
            .filter_map(|u| u.ticks_to_room_change)
            .collect();
        let compute = self.compute.to_json(api);
        let room_loads = self.room_entries.len().saturating_sub(1);
        json!({
            "outcome": outcome,
            "finish_reason": finish_reason,
            "ticks": ticks,
            "elapsed_secs": elapsed_secs,
            "score": {
                "completed": run.ending.is_some(),
                "ending": run.ending,
                "ticks": ticks,
                "game_time_ticks": run.game_time_ticks,
                "game_time": run.game_time_ticks.map(clock),
                "hits": self.hits,
                "deaths": self.deaths,
                "room_loads": room_loads,
                "compute": {
                    "min_us": compute.get("min_us"),
                    "avg_us": compute.get("avg_us"),
                    "p99_us": compute.get("p99_us"),
                    "max_us": compute.get("max_us"),
                    "over_ms": compute.get("over_ms"),
                },
            },
            "hits": self.hits,
            "room_hits": self.room_hits,
            "enemy_hits": self.enemy_hits,
            "enemy_kinds": self.enemy_kinds,
            "room_loads": room_loads,
            "compute": compute,
            "rooms_reached": self.room_entries.len(),
            "unique_rooms": path.iter().collect::<std::collections::BTreeSet<_>>().len(),
            "last_room": path.last(),
            "room_path": path,
            "room_entries": entries,
            "health": {
                "start": self.first_health,
                "final": self.last_health,
                "min": self.min_health,
                "damage_taken": self.damage_taken,
            },
            "deaths": self.deaths,
            "saves": self.saves,
            "reloads": self.reloads,
            "deaths_before_reload": self.deaths_before_reload,
            "combat": {
                "fire_presses": self.fire_presses,
                "fire_held_ticks": self.fire_held_ticks,
                "shots_fired_proxy": self.shots_fired,
                "enemy_hit_events_proxy": self.enemy_hit_events,
                "enemy_damage_dealt_proxy": self.enemy_damage_dealt,
            },
            "movement": {
                "turn_in_place_ticks": self.turn_in_place_ticks,
                "move_ticks": self.move_ticks,
                "stalled_walk_ticks": self.stalled_walk_ticks,
            },
            "states": {
                "message_ticks": self.message_ticks,
                "scene_ticks": self.scene_ticks,
                "transition_ticks": self.transition_ticks,
                "pickup_ticks": self.pickup_ticks,
                "uncontrollable_ticks": self.uncontrollable_ticks,
            },
            "menu": self.menu.to_json(),
            "activity": self.activity,
            "timeline": self.timeline,
            "stalled_runs": self.stalled_runs,
            "room_activity": self.room_activity,
            "door_unlocks": unlocks,
            "unlock_to_room_change_ticks_total": known.iter().sum::<u64>(),
        })
    }
}

/// What the report says about the run that the per-tick snapshots do not
/// carry: read from the session when the report is written.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct RunFacts {
    /// The ending the game's own rule picked (rows 1 to 7 of
    /// `src/ending.rs`), once the escape scene completed the game.
    pub ending: Option<u8>,
    /// The game's play timer in 30 Hz ticks, the clear time the RESULT
    /// screen shows. A reload winds it back to the save's.
    pub game_time_ticks: Option<u32>,
}

/// 30 Hz ticks as `h:mm:ss`.
fn clock(ticks: u32) -> String {
    let secs = ticks / 30;
    format!("{}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

#[cfg(test)]
mod tests {
    use super::super::{ItemBoxView, MenuItem};
    use super::*;

    fn menu(open: bool, screen: Option<&'static str>, cursor: i64, herbs: u8) -> MenuSnapshot {
        MenuSnapshot {
            open,
            phase: if open { "input" } else { "closed" },
            accepts_input: open,
            screen,
            cursor: open.then_some(cursor),
            capacity: 2,
            items: vec![
                MenuItem {
                    slot: 0,
                    id: 0x02,
                    quantity: 15,
                    kind: "weapon",
                    heals: false,
                },
                MenuItem {
                    slot: 1,
                    id: 0x44,
                    quantity: herbs,
                    kind: "heal",
                    heals: true,
                },
            ],
            ..MenuSnapshot::default()
        }
    }

    #[test]
    fn menu_counters_follow_the_engine_menu_state() {
        let mut counters = MenuCounters::default();
        let mut prev: Option<(MenuSnapshot, Option<u8>)> = None;
        let steps = [
            (menu(false, None, 0, 1), None),
            (menu(true, None, 8, 1), None),
            (menu(true, None, 10, 1), None),
            (menu(true, None, 10, 0), None),
            (menu(true, None, 8, 0), Some(0x02)),
            (menu(false, None, 0, 0), Some(0x02)),
            (menu(true, Some("pickup"), 8, 0), Some(0x02)),
        ];
        for (now, equipped) in steps {
            counters.observe(prev.as_ref(), &now, equipped);
            prev = Some((now, equipped));
        }
        assert_eq!(counters.opens, 1, "START opens");
        assert_eq!(
            counters.viewer_opens, 1,
            "the pick-up viewer is not an inventory open"
        );
        assert_eq!(counters.cursor_moves, 2);
        assert_eq!(counters.item_uses, 1);
        assert_eq!(counters.heal_uses, 1);
        assert_eq!(counters.equips, 1);
    }

    #[test]
    fn box_swaps_and_mixes_are_counted_apart_from_item_uses() {
        let boxed = |slots: Vec<(u8, u8)>, herbs: u8| MenuSnapshot {
            item_box: Some(ItemBoxView {
                mode: "browse".to_string(),
                slots,
                ..ItemBoxView::default()
            }),
            ..menu(true, Some("item_box"), 8, herbs)
        };
        let mut counters = MenuCounters::default();
        let mut prev: Option<(MenuSnapshot, Option<u8>)> = None;
        let mut moving = menu(true, None, 10, 2);
        moving.mode = Some("move");
        let steps = [
            boxed(vec![(0, 0), (0, 0)], 1),
            // Deposit the herb into box slot 0.
            boxed(vec![(0x44, 1), (0, 0)], 0),
            // Withdraw it again.
            boxed(vec![(0, 0), (0, 0)], 1),
            menu(false, None, 0, 1),
            menu(true, None, 10, 2),
            moving,
            // The mix leaves one herb stack.
            menu(true, None, 10, 1),
        ];
        for now in steps {
            counters.observe(prev.as_ref(), &now, None);
            prev = Some((now, None));
        }
        assert_eq!(counters.box_deposits, 1);
        assert_eq!(counters.box_withdrawals, 1);
        assert_eq!(counters.combines, 1);
        assert_eq!(
            counters.item_uses, 0,
            "neither a box swap nor a mix is a use"
        );
    }
}
