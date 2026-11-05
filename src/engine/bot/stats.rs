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

use super::{BotInput, EnemySnapshot, SessionSnapshot};
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
    deaths: u32,
    was_dead: bool,
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
}

impl BotStats {
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
        }
        self.first_health.get_or_insert(health);
        self.last_health = Some(health);
        self.min_health = Some(self.min_health.map_or(health, |m| m.min(health)));
        let dead = snapshot.scene.dead || health <= 0;
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
    }

    /// The report as JSON.
    pub(super) fn to_json(
        &self,
        outcome: &str,
        finish_reason: Option<&str>,
        ticks: u64,
        elapsed_secs: f64,
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
        json!({
            "outcome": outcome,
            "finish_reason": finish_reason,
            "ticks": ticks,
            "elapsed_secs": elapsed_secs,
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
            },
            "states": {
                "message_ticks": self.message_ticks,
                "scene_ticks": self.scene_ticks,
                "transition_ticks": self.transition_ticks,
                "pickup_ticks": self.pickup_ticks,
                "uncontrollable_ticks": self.uncontrollable_ticks,
            },
            "door_unlocks": unlocks,
            "unlock_to_room_change_ticks_total": known.iter().sum::<u64>(),
        })
    }
}
