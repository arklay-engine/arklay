//! Staging's `EnemyRandomizer` (`EnemyRandomizer.cs`): which rooms get enemies,
//! of which kind, and where. The output is a plan of changes to the rooms.
//! Writing them into the room scripts and the enemy table is the engine
//! crate's job. Staging's writer is `Re1CrModBuilder.cs:587-653`.
//!
//! Where arklay differs from staging, and why:
//!
//! - Spots are kept per room directory ([`Map::spots`]). The first logical room
//!   of a directory, in map order, that can take a kind and has spots claims
//!   them. Staging gives each RDT to the first room that lists it
//!   (`EnemyRandomizer.cs:178-208`), so a later room of that RDT keeps its rules
//!   but gets no placements. Here it keeps its retypes and gets no new enemies.
//!   No RE1 directory has two rule rooms, so this never applies to RE1.
//! - A directory is planned once, for both mansion visits. Staging plans each
//!   RDT on its own when `progression/mansion2` is `Always` (`:149`), and the
//!   map does not keep the RDTs apart.
//! - Only the `rooms` argument is planned, and only their rules and kill flags
//!   count. Staging plans every room of its map (`:181`).
//! - A spot is used once per room, where staging's endless bag repeats it
//!   (`:74-81`). A kill flag is used once, where staging's bag reuses them once
//!   it runs out (`:24-33`). What cannot be placed is counted in
//!   [`EnemyPlan::dropped`].
//! - One pose is drawn per enemy. Staging draws one per RDT copy (`:59`, `:99`).
//! - The `restricted` flag and a rule's condition are not read. Staging's enemy
//!   stage does not read `restricted` either, and the room script says where
//!   added enemies go.
//!
//! Staging runs this stage only when `enemies/random` is on
//! (`ClassicRandomizer.cs:736`), and so does [`randomize`].

use std::collections::{HashSet, VecDeque};

use indexmap::IndexMap;

use crate::config::Config;
use crate::map::{Enemies, EnemyEntry, EnemyPose, Map, Player, Room, Spot, tags};
use crate::rng::Rng;

/// The fixed enemy slots of a room (staging's `Enumerable.Range(0, 16)`,
/// `EnemyRandomizer.cs:41`).
const SLOTS: u8 = 16;

/// The enemy changes of one game. See the module docs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EnemyPlan {
    /// The rooms that change, by logical room key, in the order they were
    /// chosen.
    pub rooms: IndexMap<String, RoomEnemies>,
    /// Creates that could not be made: the directory had no spot left for
    /// them, the room had no free slot, or the game had no kill flag left.
    pub dropped: usize,
}

/// The enemy changes to one logical room.
#[derive(Clone, Debug, PartialEq)]
pub struct RoomEnemies {
    /// The room directory the changes are written to, in lowercase (`10c`).
    pub directory: String,
    /// The enemy kind chosen for the room: an entry key, such as `zombie`.
    pub kind: String,
    /// The effect sheets the kind needs.
    pub esp: Vec<u8>,
    pub placements: Vec<Placement>,
}

/// One change to a room's enemies.
#[derive(Clone, Debug, PartialEq)]
pub enum Placement {
    /// Makes the fixed enemy in slot `slot` the room's enemy type. The engine
    /// also resets its state as staging does (`Re1CrModBuilder.cs:602-610`).
    /// No pose is drawn, since staging's retype never writes one.
    Retype { slot: u8, enemy: u8 },
    /// Adds an enemy of type `enemy` in the free slot `slot`, at `spot`, in
    /// `pose`, with the kill flag `kill_flag` (`Re1CrModBuilder.cs:645-653`).
    Create {
        slot: u8,
        enemy: u8,
        pose: u8,
        kill_flag: u8,
        spot: Spot,
    },
}

/// Plans the enemies of `player`'s game. `rooms` are the logical rooms the game
/// uses, and `rng` should be `Rng::named(seed, "enemy")`. Nothing is changed:
/// the result is the plan.
pub fn randomize(
    map: &Map,
    config: &Config,
    player: Player,
    rooms: &HashSet<String>,
    rng: &mut Rng,
) -> EnemyPlan {
    if !config.get_bool("enemies/random", false) {
        return EnemyPlan::default();
    }
    let game = Game {
        rooms,
        door_rando: config.get_bool("doors/random", false),
        player,
    };
    let mut kinds = enemy_kinds(map, config);
    let chosen = select(map, config, &game, &mut kinds, rng);
    let mut kill_flags = game.kill_flags(map, rng);

    let mut plan = EnemyPlan::default();
    for (candidate, index) in chosen {
        let kind = &kinds[index];
        // :39 the room's enemy type, one of the kind's ids.
        let enemy = *rng.pick(&kind.entry.ids);
        // :41-43 the slots that no rule of the room uses.
        let mut free: VecDeque<u8> = (0..SLOTS)
            .filter(|slot| !candidate.rules.iter().any(|rule| rule.id == Some(*slot)))
            .collect();
        let mut placements = Vec::new();
        for rule in &candidate.rules {
            match rule.id {
                // :46-64 retype. Staging skips `npc` rules here only (:48-49).
                Some(slot) => {
                    if rule.kind.as_deref() != Some("npc") {
                        placements.push(Placement::Retype { slot, enemy });
                    }
                }
                // :65-110 clear: the room gets new enemies at its spots.
                None => {
                    let count = clear_count(kind, rule, rng);
                    let spots = if candidate.owns_spots {
                        usable_spots(map, candidate.room, &kind.entry.key, rng)
                    } else {
                        Vec::new()
                    };
                    let mut made = 0;
                    for spot in spots.into_iter().take(count) {
                        let (Some(slot), Some(kill_flag)) = (free.pop_front(), kill_flags.pop())
                        else {
                            break;
                        };
                        let pose = choose_pose(&kind.entry.poses, rng);
                        placements.push(Placement::Create {
                            slot,
                            enemy,
                            pose,
                            kill_flag,
                            spot: spot.clone(),
                        });
                        made += 1;
                    }
                    plan.dropped += count - made;
                }
            }
        }
        if !placements.is_empty() {
            plan.rooms.insert(
                candidate.key.to_string(),
                RoomEnemies {
                    directory: candidate.room.room_id().to_ascii_lowercase(),
                    kind: kind.entry.key.clone(),
                    esp: kind.entry.esp.clone(),
                    placements,
                },
            );
        }
    }
    plan
}

/// What makes a room or a rule part of this game (staging's `Variation.Map`).
struct Game<'a> {
    rooms: &'a HashSet<String>,
    door_rando: bool,
    player: Player,
}

impl Game<'_> {
    fn includes(&self, room: &Room) -> bool {
        self.rooms.contains(room.key.as_str()) && room.filter.includes(self.door_rando, self.player)
    }

    /// The rules of `room` that are in the game.
    fn rules<'r>(&self, room: &'r Room) -> Vec<&'r Enemies> {
        room.enemies
            .iter()
            .filter(|rule| rule.filter.includes(self.door_rando, self.player))
            .collect()
    }

    /// The kill flags a created enemy may take: every flag from 0 to 254 that no
    /// rule in the game uses, shuffled (`EnemyRandomizer.cs:18-33`).
    fn kill_flags(&self, map: &Map, rng: &mut Rng) -> Vec<u8> {
        let used: HashSet<u16> = map
            .rooms
            .values()
            .filter(|room| self.includes(room))
            .flat_map(|room| self.rules(room))
            .filter_map(|rule| rule.flag)
            .collect();
        // Staging's `i < 255` (:26).
        let mut flags: Vec<u8> = (0..u8::MAX)
            .filter(|flag| !used.contains(&u16::from(*flag)))
            .collect();
        rng.shuffle(&mut flags);
        flags
    }
}

/// One enemy kind and its share of the rooms (staging's `EnemyInfo`,
/// `EnemyRandomizer.cs:302-315`).
struct Kind<'a> {
    entry: &'a EnemyEntry,
    /// `enemies/distribution/<key>`.
    weight: f64,
    /// `enemies/min/<key>`, at least 1 (`:172`).
    min: i32,
    /// `enemies/max/<key>`, at most 16 (`:173`).
    max: i32,
    /// The rooms still to get this kind (`:233`, `:250`).
    rooms: i64,
}

fn enemy_kinds<'a>(map: &'a Map, config: &Config) -> Vec<Kind<'a>> {
    map.enemies
        .iter()
        .flat_map(|group| &group.entries)
        // An entry with no type cannot be placed.
        .filter(|entry| !entry.ids.is_empty())
        .map(|entry| Kind {
            entry,
            weight: config.get_f64(&format!("enemies/distribution/{}", entry.key), 0.0),
            min: clamp_i32(config.get_int(&format!("enemies/min/{}", entry.key), 1)).max(1),
            max: clamp_i32(config.get_int(&format!("enemies/max/{}", entry.key), 6)).min(16),
            rooms: 0,
        })
        .collect()
}

/// A room that has enemy rules (staging's `RoomRdt`).
struct Candidate<'a> {
    key: &'a str,
    room: &'a Room,
    /// Its rules in the game.
    rules: Vec<&'a Enemies>,
    /// The kinds it can take, as indexes into the kinds.
    supported: Vec<usize>,
    /// Whether it places its new enemies at its directory's spots.
    owns_spots: bool,
}

/// Every room of the game that has enemy rules, in map order, with the kinds it
/// can take (staging's room list, `EnemyRandomizer.cs:178-208`).
fn room_list<'a>(
    map: &'a Map,
    config: &Config,
    game: &Game<'_>,
    kinds: &[Kind<'_>],
) -> Vec<Candidate<'a>> {
    // :152-159 the rooms that get no enemies unless their setting allows them.
    let banned = |room: &Room| {
        (!config.get_bool("enemies/box", false) && room.filter.has_tag(tags::BOX))
            || (!config.get_bool("enemies/safe", false) && room.filter.has_tag(tags::SAFE))
            || (!config.get_bool("enemies/save", false) && room.filter.has_tag(tags::SAVE))
    };
    let mut claimed = HashSet::new();
    let mut list = Vec::new();
    for (key, room) in &map.rooms {
        if !game.includes(room) {
            continue;
        }
        let rules = game.rules(room);
        if rules.is_empty() {
            continue;
        }
        let supported: Vec<usize> = if banned(room) {
            Vec::new()
        } else {
            (0..kinds.len())
                .filter(|&index| supports(&rules, kinds[index].entry))
                .collect()
        };
        let owns_spots = !supported.is_empty()
            && !map.spots_for(room).is_empty()
            && claimed.insert(room.room_id().to_ascii_lowercase());
        list.push(Candidate {
            key: key.as_str(),
            room,
            rules,
            supported,
            owns_spots,
        });
    }
    list
}

/// Whether a room with `rules` can take the kind `entry` (staging's
/// `SupportsEnemy`, `EnemyRandomizer.cs:257-281`). Only the room's first rule
/// that is not `npc` counts.
fn supports(rules: &[&Enemies], entry: &EnemyEntry) -> bool {
    let Some(rule) = rules
        .iter()
        .find(|rule| rule.kind.as_deref() != Some("npc"))
    else {
        return false;
    };
    match &rule.include {
        Some(include) => entry.ids.iter().any(|id| include.contains(id)),
        None => !entry.ids.iter().any(|id| rule.exclude.contains(id)),
    }
}

/// Chooses the rooms that get enemies, and the kind of each (staging's
/// `GetRoomSelection`, `EnemyRandomizer.cs:145-255`), in the order they are
/// planned.
fn select<'a>(
    map: &'a Map,
    config: &Config,
    game: &Game<'_>,
    kinds: &mut [Kind<'_>],
    rng: &mut Rng,
) -> Vec<(Candidate<'a>, usize)> {
    let mut candidates = room_list(map, config, game, kinds);
    // :211 shuffle. :214-223 keep the first rooms of the shuffled list that can
    // take a kind, up to the fraction `enemies/rooms` of all candidates (:215).
    rng.shuffle(&mut candidates);
    let target =
        (config.get_f64("enemies/rooms", 0.0) * candidates.len() as f64).round_ties_even() as usize;
    candidates.retain(|candidate| !candidate.supported.is_empty());
    candidates.truncate(target);
    // :226-228 the rooms with the fewest kinds first (a stable sort).
    candidates.sort_by_key(|candidate| candidate.supported.len());
    // :230-234 each kind's share of the enemy rooms, by its weight. A zero total
    // gives NaN, which casts to 0 and so no rooms.
    let enemy_rooms = candidates.len() as f64;
    let total_weight: f64 = kinds.iter().map(|kind| kind.weight).sum();
    for kind in kinds.iter_mut() {
        kind.rooms = (enemy_rooms * (kind.weight / total_weight)).ceil() as i64;
    }
    // :237-252
    let mut chosen = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let open: Vec<usize> = candidate
            .supported
            .iter()
            .copied()
            .filter(|&index| kinds[index].rooms > 0)
            .collect();
        let index = if open.is_empty() {
            // :246-248 the kind with the lowest weight, the first of equal ones.
            candidate
                .supported
                .iter()
                .copied()
                .min_by(|&a, &b| kinds[a].weight.total_cmp(&kinds[b].weight))
        } else {
            Some(*rng.pick(&open))
        };
        let Some(index) = index else {
            continue;
        };
        kinds[index].rooms -= 1;
        chosen.push((candidate, index));
    }
    chosen
}

/// The number of enemies a clear rule adds (`EnemyRandomizer.cs:68-73`): a
/// number in the kind's range, capped by the rule's max difficulty.
fn clear_count(kind: &Kind<'_>, rule: &Enemies, rng: &mut Rng) -> usize {
    let mut max = kind.max;
    if let Some(difficulty) = rule.max_difficulty {
        // Staging divides by `4 - difficulty` (:71). From 4 up the divisor is
        // zero or less, and staging's cast of the result is undefined, so no cap
        // applies there.
        let divisor = 4 - i32::from(difficulty);
        if divisor > 0 {
            let scaled = (f64::from(max) / f64::from(divisor)).round_ties_even() as i32;
            max = max.min(scaled.max(1));
        }
    }
    usize::try_from(rng.next_in(kind.min, max.saturating_add(1))).unwrap_or(0)
}

/// The spots of the room's directory that take the kind `key`, shuffled.
/// Staging's endless bag of positions (`:74-81`) can repeat a spot, and this
/// does not. A spot's `placeholder` is not used: every spot of a directory is
/// a candidate for each clear rule.
fn usable_spots<'a>(map: &'a Map, room: &Room, key: &str, rng: &mut Rng) -> Vec<&'a Spot> {
    let mut spots: Vec<&Spot> = map
        .spots_for(room)
        .iter()
        .filter(|spot| meets(spot, key))
        .collect();
    rng.shuffle(&mut spots);
    spots
}

/// Whether a spot takes the kind `key` (`EnemyRandomizer.cs:114-125`): its
/// `include` list when it has one, otherwise its `exclude` list.
fn meets(spot: &Spot, key: &str) -> bool {
    if spot.include.is_empty() {
        !spot.exclude.iter().any(|excluded| excluded == key)
    } else {
        spot.include.iter().any(|included| included == key)
    }
}

/// A pose by probability (`EnemyRandomizer.cs:127-143`). Pose 0 when no pose
/// has a chance. Staging's `NextDouble(0, total)` is not guarded, and neither
/// is this draw.
fn choose_pose(poses: &[EnemyPose], rng: &mut Rng) -> u8 {
    let total: f64 = poses.iter().map(|pose| pose.probability).sum();
    if total == 0.0 {
        return 0;
    }
    let n = rng.next_f64() * total;
    let mut c = 0.0;
    for pose in poses {
        if n <= c + pose.probability {
            return pose.pose;
        }
        c += pose.probability;
    }
    0
}

/// An `i64` setting as an `i32`, saturating.
fn clamp_i32(value: i64) -> i32 {
    value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{EnemyGroup, Filter};

    fn entry(key: &str, ids: &[u8], esp: &[u8]) -> EnemyEntry {
        EnemyEntry {
            key: key.into(),
            name: key.into(),
            ids: ids.to_vec(),
            esp: esp.to_vec(),
            poses: vec![EnemyPose {
                name: "Standing".into(),
                pose: 0,
                probability: 1.0,
            }],
        }
    }

    /// A map with the given rooms and the spots of each directory. Zombies are
    /// type 0 with effect sheet 3, and crows are type 5.
    fn map(rooms: Vec<Room>, spots: &[(&str, Vec<Spot>)]) -> Map {
        let mut map = Map {
            enemies: vec![EnemyGroup {
                name: "Enemies".into(),
                background: String::new(),
                foreground: String::new(),
                entries: vec![entry("zombie", &[0], &[3]), entry("crow", &[5], &[])],
            }],
            ..Map::default()
        };
        for room in rooms {
            map.rooms.insert(room.key.clone(), room);
        }
        for (directory, list) in spots {
            map.spots.insert((*directory).to_string(), list.clone());
        }
        map
    }

    fn room(key: &str, enemies: Vec<Enemies>) -> Room {
        Room {
            key: key.into(),
            enemies,
            ..Room::default()
        }
    }

    fn tagged(room: Room, tag: &str) -> Room {
        Room {
            filter: Filter {
                tags: vec![tag.into()],
                ..Filter::default()
            },
            ..room
        }
    }

    /// A rule with no id, which replaces the room's enemies with new ones.
    fn clear() -> Enemies {
        Enemies::default()
    }

    /// A rule that retypes the fixed enemy in `slot`.
    fn retype(slot: u8) -> Enemies {
        Enemies {
            id: Some(slot),
            ..Enemies::default()
        }
    }

    fn spot(x: i32) -> Spot {
        Spot {
            x,
            ..Spot::default()
        }
    }

    /// Enemies on, every room allowed, and zombies only, one in each room.
    fn config() -> Config {
        let mut config = Config::new();
        config.set("enemies/random", true);
        config.set("enemies/rooms", 1.0);
        config.set("enemies/distribution/zombie", 1.0);
        config
    }

    fn rooms(keys: &[&str]) -> HashSet<String> {
        keys.iter().map(|key| (*key).to_string()).collect()
    }

    fn run(map: &Map, config: &Config, keys: &[&str], seed: u64) -> EnemyPlan {
        randomize(
            map,
            config,
            Player::Chris,
            &rooms(keys),
            &mut Rng::new(seed),
        )
    }

    /// The slot, enemy, pose, kill flag and spot x of each created enemy.
    fn creates(room: &RoomEnemies) -> Vec<(u8, u8, u8, u8, i32)> {
        room.placements
            .iter()
            .filter_map(|placement| match placement {
                Placement::Create {
                    slot,
                    enemy,
                    pose,
                    kill_flag,
                    spot,
                } => Some((*slot, *enemy, *pose, *kill_flag, spot.x)),
                Placement::Retype { .. } => None,
            })
            .collect()
    }

    #[test]
    fn nothing_is_planned_while_enemies_are_off() {
        let map = map(vec![room("10C", vec![clear()])], &[("10c", vec![spot(1)])]);
        let mut config = config();
        config.set("enemies/random", false);
        assert_eq!(run(&map, &config, &["10C"], 1), EnemyPlan::default());
    }

    #[test]
    fn a_clear_rule_places_enemies_at_the_directorys_spots() {
        let spots: Vec<Spot> = (0..4).map(spot).collect();
        let map = map(vec![room("10C", vec![clear()])], &[("10c", spots)]);
        let mut config = config();
        config.set("enemies/min/zombie", 2);
        config.set("enemies/max/zombie", 2);
        let plan = run(&map, &config, &["10C"], 1);

        let planned = &plan.rooms["10C"];
        assert_eq!(planned.directory, "10c");
        assert_eq!(planned.kind, "zombie");
        assert_eq!(planned.esp, [3]);
        let created = creates(planned);
        assert_eq!(created.len(), 2);
        assert_ne!(created[0].0, created[1].0, "free slots differ");
        assert_ne!(created[0].3, created[1].3, "kill flags differ");
        assert_ne!(created[0].4, created[1].4, "spots differ");
        assert!(created.iter().all(|c| c.1 == 0 && c.2 == 0));
        assert!(created.iter().all(|c| (0..4).contains(&c.4)));
        assert_eq!(plan.dropped, 0);
    }

    #[test]
    fn a_retype_changes_the_fixed_enemys_type() {
        let map = map(vec![room("10C", vec![retype(2)])], &[]);
        let plan = run(&map, &config(), &["10C"], 1);
        assert_eq!(
            plan.rooms["10C"].placements,
            [Placement::Retype { slot: 2, enemy: 0 }]
        );
    }

    #[test]
    fn npc_rules_are_left_alone() {
        let npc = Enemies {
            kind: Some("npc".into()),
            ..retype(2)
        };
        let map = map(vec![room("10C", vec![npc])], &[]);
        assert!(run(&map, &config(), &["10C"], 1).rooms.is_empty());
    }

    #[test]
    fn a_directory_without_spots_gets_no_new_enemies() {
        let map = map(vec![room("10C", vec![clear()])], &[]);
        let mut config = config();
        config.set("enemies/min/zombie", 2);
        config.set("enemies/max/zombie", 2);
        let plan = run(&map, &config, &["10C"], 1);
        assert!(plan.rooms.is_empty());
        assert_eq!(plan.dropped, 2);
    }

    #[test]
    fn only_the_first_room_of_a_directory_gets_its_spots() {
        let spots: Vec<Spot> = (0..4).map(spot).collect();
        let map = map(
            vec![room("10C", vec![clear()]), room("10C/BACK", vec![clear()])],
            &[("10c", spots)],
        );
        let mut config = config();
        config.set("enemies/min/zombie", 2);
        config.set("enemies/max/zombie", 2);
        let plan = run(&map, &config, &["10C", "10C/BACK"], 1);
        assert_eq!(creates(&plan.rooms["10C"]).len(), 2);
        assert!(!plan.rooms.contains_key("10C/BACK"));
        assert_eq!(plan.dropped, 2);
    }

    #[test]
    fn kill_flags_skip_the_flags_of_rules_in_the_game() {
        // Rules in the game with flags 0 to 253 leave flag 254 for the one enemy
        // that the room asks for second.
        let taken: Vec<Enemies> = (0..254u16)
            .map(|flag| Enemies {
                kind: Some("npc".into()),
                flag: Some(flag),
                ..retype(0)
            })
            .collect();
        let map = map(
            vec![room("99X", taken), room("10C", vec![clear()])],
            &[("10c", (0..4).map(spot).collect())],
        );
        let mut config = config();
        config.set("enemies/min/zombie", 2);
        config.set("enemies/max/zombie", 2);
        let plan = run(&map, &config, &["99X", "10C"], 1);
        let created = creates(&plan.rooms["10C"]);
        assert_eq!(created.iter().map(|c| c.3).collect::<Vec<_>>(), [254]);
        assert_eq!(plan.dropped, 1);

        // Left out of the game, those rules take no flags.
        let plan = run(&map, &config, &["10C"], 1);
        assert_eq!(creates(&plan.rooms["10C"]).len(), 2);
        assert_eq!(plan.dropped, 0);
    }

    #[test]
    fn the_difficulty_caps_the_count_with_banker_rounding() {
        // Staging rounds the cap with Math.Round: 10 / 4 = 2.5 is 2 (:71).
        let spots: Vec<Spot> = (0..10).map(spot).collect();
        let map = map(
            vec![room(
                "10C",
                vec![Enemies {
                    max_difficulty: Some(0),
                    ..clear()
                }],
            )],
            &[("10c", spots)],
        );
        let mut config = config();
        config.set("enemies/max/zombie", 10);
        for seed in 0..20 {
            let plan = run(&map, &config, &["10C"], seed);
            let count = creates(&plan.rooms["10C"]).len();
            assert!((1..=2).contains(&count), "seed {seed}: {count}");
        }
    }

    #[test]
    fn a_spot_is_used_only_by_the_kinds_it_takes() {
        let spots = vec![
            Spot {
                include: vec!["crow".into()],
                ..spot(1)
            },
            Spot {
                exclude: vec!["zombie".into()],
                ..spot(2)
            },
            spot(3),
        ];
        let map = map(vec![room("10C", vec![clear()])], &[("10c", spots)]);
        let mut config = config();
        config.set("enemies/min/zombie", 1);
        config.set("enemies/max/zombie", 1);
        let plan = run(&map, &config, &["10C"], 1);
        let xs: Vec<i32> = creates(&plan.rooms["10C"]).iter().map(|c| c.4).collect();
        assert_eq!(xs, [3]);
    }

    #[test]
    fn a_kind_is_chosen_by_its_distribution_weight() {
        let map = map(
            vec![room("10C", vec![retype(1)]), room("20A", vec![retype(1)])],
            &[],
        );
        let mut config = config();
        config.set("enemies/distribution/zombie", 0.0);
        config.set("enemies/distribution/crow", 1.0);
        let plan = run(&map, &config, &["10C", "20A"], 3);
        for key in ["10C", "20A"] {
            assert_eq!(plan.rooms[key].kind, "crow");
            assert_eq!(
                plan.rooms[key].placements,
                [Placement::Retype { slot: 1, enemy: 5 }]
            );
        }
    }

    #[test]
    fn the_room_fraction_limits_the_enemy_rooms() {
        let map = map(
            ["10C", "11A", "12A", "13A"]
                .into_iter()
                .map(|key| room(key, vec![retype(1)]))
                .collect(),
            &[],
        );
        let mut config = config();
        config.set("enemies/rooms", 0.5);
        assert_eq!(
            run(&map, &config, &["10C", "11A", "12A", "13A"], 1)
                .rooms
                .len(),
            2
        );
        config.set("enemies/rooms", 0.0);
        assert!(
            run(&map, &config, &["10C", "11A", "12A", "13A"], 1)
                .rooms
                .is_empty()
        );
    }

    #[test]
    fn banned_rooms_stay_empty_unless_their_setting_allows_them() {
        let map = map(vec![tagged(room("10C", vec![retype(1)]), tags::BOX)], &[]);
        let mut config = config();
        config.set("enemies/box", false);
        assert!(run(&map, &config, &["10C"], 1).rooms.is_empty());
        config.set("enemies/box", true);
        assert!(run(&map, &config, &["10C"], 1).rooms.contains_key("10C"));
    }

    #[test]
    fn filters_leave_out_what_is_not_in_the_game() {
        let jill = Room {
            filter: Filter {
                player: Some(Player::Jill),
                ..Filter::default()
            },
            ..room("10C", vec![retype(1)])
        };
        let door_only = room(
            "20A",
            vec![Enemies {
                filter: Filter {
                    door_rando: Some(true),
                    ..Filter::default()
                },
                ..retype(1)
            }],
        );
        let map = map(vec![jill, door_only], &[]);
        let keys = ["10C", "20A"];
        let config = config();

        let plan = randomize(
            &map,
            &config,
            Player::Chris,
            &rooms(&keys),
            &mut Rng::new(1),
        );
        assert!(plan.rooms.is_empty(), "neither is in Chris's game");
        let plan = randomize(&map, &config, Player::Jill, &rooms(&keys), &mut Rng::new(1));
        let planned: Vec<&str> = plan.rooms.keys().map(String::as_str).collect();
        assert_eq!(planned, ["10C"]);
    }

    #[test]
    fn with_no_weight_the_first_kind_takes_every_room() {
        let map = map(vec![room("10C", vec![retype(1)])], &[]);
        let mut config = config();
        config.set("enemies/distribution/zombie", 0.0);
        let plan = run(&map, &config, &["10C"], 1);
        assert_eq!(plan.rooms["10C"].kind, "zombie");
    }

    #[test]
    fn the_same_seed_gives_the_same_plan() {
        let spots: Vec<Spot> = (0..6).map(spot).collect();
        let map = map(
            vec![
                room("10C", vec![retype(1), clear()]),
                room("20A", vec![clear()]),
            ],
            &[("10c", spots.clone()), ("20a", spots)],
        );
        let config = config();
        assert_eq!(
            run(&map, &config, &["10C", "20A"], 9),
            run(&map, &config, &["10C", "20A"], 9)
        );
    }

    #[test]
    fn a_pose_follows_its_probabilities() {
        let poses = [
            EnemyPose {
                name: "Never".into(),
                pose: 3,
                probability: 0.0,
            },
            EnemyPose {
                name: "Always".into(),
                pose: 7,
                probability: 1.0,
            },
        ];
        for seed in 0..20 {
            assert_eq!(choose_pose(&poses, &mut Rng::new(seed)), 7);
        }
        assert_eq!(choose_pose(&[], &mut Rng::new(1)), 0);
    }
}
