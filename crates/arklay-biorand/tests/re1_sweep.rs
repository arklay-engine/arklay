//! The seed sweep: plans many games across the configuration space on the
//! real RE1 map and runs every static check of `arklay_biorand::audit` on
//! each, then summarizes the failures. No pack is written, so a game costs a
//! fraction of a second.
//!
//! The full sweep is ignored by default (`docs/biorand/plan.md`, "The seed
//! sweep"):
//!
//! ```sh
//! SWEEP_CASES=1000 SWEEP_OUT=target/scratch/sweep.txt \
//!   cargo test --profile test-release -p arklay-biorand --test re1_sweep \
//!   -- --ignored --nocapture
//! ```
//!
//! Environment: `SWEEP_CASES` (cases; each is planned for both players),
//! `SWEEP_FIRST` (the first seed, default 1), `SWEEP_THREADS`, `SWEEP_OUT`
//! (a file for the summary and a repro line per failure), `SWEEP_GREEDY=0`
//! to skip the greedy-spend measurement, `SWEEP_ONLY=<case>` to rerun one case,
//! `SWEEP_FORCE=<json>` to apply options over every case (to measure one
//! option's failure rate).
//!
//! A case is a seed and a configuration. The configurations come in three
//! kinds, in rotation: a hand-written matrix of named options, a random draw
//! from a factorial of option levels (both players, doors on/off, locks,
//! enemies, item options, inventory size, progression), and a configuration
//! with every option resolved at random.

mod common;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use arklay_biorand::audit;
use arklay_biorand::config::{self, Config, Kind, Value};
use arklay_biorand::map::{Map, Player};
use arklay_biorand::plan::{Players, generate};

use common::load_map;

/// A small deterministic generator for drawing configurations.
struct Draw(u64);

impl Draw {
    fn next(&mut self) -> u64 {
        // splitmix64
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

type Settings = Vec<(String, Value)>;

fn s(key: &str, value: impl Into<Value>) -> (String, Value) {
    (key.to_string(), value.into())
}

/// The item kinds the map has, by prefix (`ammo/`, `health/`, `weapon/`).
fn kinds(map: &Map, prefix: &str) -> Vec<String> {
    let mut kinds: Vec<String> = map
        .items
        .values()
        .filter(|def| def.kind.starts_with(prefix))
        .map(|def| def.kind.clone())
        .collect();
    kinds.sort();
    kinds.dedup();
    kinds
}

/// The levels of each factor: one is drawn per factor for a case.
fn factors(map: &Map) -> Vec<Vec<Settings>> {
    let ammo = kinds(map, "ammo/");
    let health = kinds(map, "health/");
    let weapons = kinds(map, "weapon/");
    let ratio = |kinds: &[String], value: f64| -> Settings {
        kinds
            .iter()
            .map(|kind| s(&format!("items/distribution/{kind}"), value))
            .collect()
    };
    let mut ammo_max = ratio(&ammo, 1.0);
    let mut ammo_min = ratio(&ammo, 0.0);
    for kind in &ammo {
        ammo_max.push(s(&format!("items/stack/min/{kind}"), 100));
        ammo_max.push(s(&format!("items/stack/max/{kind}"), 100));
    }
    for def in map
        .items
        .values()
        .filter(|def| def.kind.starts_with("weapon/"))
    {
        ammo_max.push(s(
            &format!("items/weapon/initial/min/{}", def.kind),
            i64::from(def.max),
        ));
        ammo_min.push(s(&format!("items/weapon/initial/max/{}", def.kind), 0));
    }
    let _ = weapons;
    let mut no_weapons: Settings = Vec::new();
    for def in map
        .items
        .values()
        .filter(|def| def.kind.starts_with("weapon/") && def.kind != "weapon/knife")
    {
        no_weapons.push(s(&format!("items/weapon/enabled/{}", def.kind), false));
    }
    vec![
        // The doors need random items; the factor keeps the pair together.
        vec![
            vec![s("doors/random", false), s("items/random", true)],
            vec![s("doors/random", true), s("items/random", true)],
            vec![
                s("doors/random", true),
                s("items/random", true),
                s("doors/segments", 1),
                s("doors/rooms", 0.0),
            ],
            vec![
                s("doors/random", true),
                s("items/random", true),
                s("doors/segments", 1),
                s("doors/rooms", 0.7),
            ],
            vec![
                s("doors/random", true),
                s("items/random", true),
                s("doors/segments", 6),
                s("doors/rooms", 1.0),
            ],
            vec![s("doors/random", false), s("items/random", false)],
        ],
        vec![
            vec![s("locks/random", false)],
            vec![s("locks/random", true), s("locks/ratio", 0.25)],
            vec![s("locks/random", true), s("locks/ratio", 1.0)],
            vec![
                s("locks/random", true),
                s("locks/ratio", 0.5),
                s("locks/preserve", true),
            ],
        ],
        vec![
            vec![s("enemies/random", false)],
            vec![s("enemies/random", true)],
        ],
        vec![
            vec![],
            vec![s("items/documents", false), s("items/hidden/keys", false)],
            vec![s("items/documents", true), s("items/documents/keys", true)],
            vec![s("items/hidden/keys", true), s("items/documents", true)],
        ],
        vec![vec![], ammo_max, ammo_min, no_weapons],
        vec![
            vec![],
            ratio(&health, 1.0),
            ratio(&health, 0.0),
            [ratio(&health, 1.0), ratio(&ammo, 0.0)].concat(),
        ],
        vec![
            vec![s("inventory/size", "6")],
            vec![s("inventory/size", "8")],
            vec![],
        ],
        vec![
            vec![],
            vec![
                s("progression/guardhouse", "Never"),
                s("progression/mansion2", "Never"),
            ],
            vec![
                s("progression/mansion2", "Never"),
                s("progression/lab", "Never"),
            ],
            vec![
                s("progression/guardhouse", "Random"),
                s("progression/mansion2", "Random"),
                s("progression/lab", "Random"),
            ],
            vec![
                s("progression/lab", "Always"),
                s("progression/caves", "Always"),
            ],
            vec![s("progression/lab", "Random (Segmented)")],
        ],
        vec![
            vec![],
            vec![
                s("progression/tyrant1", "Always"),
                s("progression/tyrant2", "Always"),
            ],
            vec![
                s("progression/tyrant1", "Never"),
                s("progression/tyrant2", "Never"),
            ],
        ],
        vec![
            vec![],
            vec![s("inventory/special/lockpick", "Never")],
            vec![s("inventory/special/lockpick", "Always"), s("hard", true)],
            vec![s("ink/enable", "Never")],
            vec![
                s("ink/enable", "Always"),
                s("inventory/min", 0),
                s("inventory/max", 8),
            ],
        ],
        vec![vec![], vec![s("cutscenes/disable", true)]],
    ]
}

/// The named matrix: the one-option-at-a-time cases of the task.
fn named(map: &Map) -> Vec<(String, Settings)> {
    let f = factors(map);
    let mut cases = Vec::new();
    // Each factor level alone on the defaults (doors off first).
    for (i, levels) in f.iter().enumerate() {
        for (j, level) in levels.iter().enumerate() {
            let mut settings = vec![s("doors/random", false)];
            settings.extend(level.clone());
            cases.push((format!("factor{i}.{j}"), settings));
        }
    }
    cases
}

/// Every option drawn at random: a dropdown takes any of its options
/// (`Random` among them), a switch a coin, a range a value in range.
fn fully_random(definition: &config::Definition, draw: &mut Draw) -> Settings {
    let mut settings: Settings = Vec::new();
    for item in definition.items() {
        let value: Value = match &item.kind {
            Kind::Switch => Value::Bool(draw.below(2) == 0),
            Kind::Dropdown { options } => {
                // Half the time say `Random` where it is allowed.
                if options.iter().any(|o| o == "Random") && draw.below(2) == 0 {
                    Value::Str("Random".into())
                } else {
                    Value::Str(draw.pick(options).clone())
                }
            }
            Kind::Range { min, max, step } | Kind::Percent { min, max, step } => {
                let mut value = min + (max - min) * draw.unit();
                if let Some(step) = step.filter(|step| *step > 0.0) {
                    value = min + ((value - min) / step).round() * step;
                }
                // Integer ranges (counts) stay integers.
                match &item.default {
                    Value::Int(_) => Value::Int(value.round() as i64),
                    _ => Value::Float(value),
                }
            }
        };
        settings.push((item.id.clone(), value));
    }
    // Door randomization needs random items.
    if settings
        .iter()
        .any(|(k, v)| k == "doors/random" && v.as_bool() == Some(true))
    {
        settings.retain(|(k, _)| k != "items/random");
        settings.push(s("items/random", true));
    }
    settings
}

struct Case {
    index: usize,
    seed: u64,
    kind: &'static str,
    label: String,
    config: Config,
}

fn build_cases(map: &Map, cases: usize, first: u64) -> Vec<Case> {
    let definition = config::definition(map);
    let defaults = Config::defaults(&definition);
    let factors = factors(map);
    let named = named(map);
    let mut out = Vec::new();
    for index in 0..cases {
        let seed = first + index as u64;
        let mut draw = Draw(seed.wrapping_mul(0x2545_F491_4F6C_DD1D) ^ 0xA5A5);
        let mut config = defaults.clone();
        let (kind, label) = match index % 3 {
            0 => {
                let (name, settings) = &named[(index / 3) % named.len()];
                for (key, value) in settings {
                    config.set(key.as_str(), value.clone());
                }
                ("named", name.clone())
            }
            1 => {
                let mut label = Vec::new();
                for (i, levels) in factors.iter().enumerate() {
                    let j = draw.below(levels.len());
                    label.push(format!("{i}.{j}"));
                    for (key, value) in &levels[j] {
                        config.set(key.as_str(), value.clone());
                    }
                }
                ("factorial", label.join(" "))
            }
            _ => {
                for (key, value) in fully_random(&definition, &mut draw) {
                    config.set(key.as_str(), value);
                }
                ("random", "every option".to_string())
            }
        };
        if let Ok(force) = std::env::var("SWEEP_FORCE") {
            config = config.merge(&Config::from_json(&force).expect("SWEEP_FORCE is JSON"));
        }
        out.push(Case {
            index,
            seed,
            kind,
            label,
            config,
        });
    }
    out
}

/// The options that differ from the defaults, as the JSON a configuration
/// file takes.
fn changed(config: &Config, defaults: &Config) -> String {
    let mut map = serde_json::Map::new();
    for (key, value) in config.iter() {
        if defaults.get(key) == Some(value) {
            continue;
        }
        let json = match value {
            Value::Bool(b) => serde_json::Value::from(*b),
            Value::Int(i) => serde_json::Value::from(*i),
            Value::Float(f) => serde_json::Value::from(*f),
            Value::Str(t) => serde_json::Value::from(t.clone()),
        };
        map.insert(key.to_string(), json);
    }
    serde_json::Value::Object(map).to_string()
}

/// A failure message reduced to its class: digits and quoted names removed.
fn class_of(message: &str) -> String {
    let first = message.lines().next().unwrap_or("");
    let mut out = String::new();
    let mut prev_digit = false;
    for c in first.chars().take(90) {
        if c.is_ascii_digit() {
            if !prev_digit {
                out.push('#');
            }
            prev_digit = true;
        } else {
            out.push(c);
            prev_digit = false;
        }
    }
    out
}

#[derive(Default)]
struct Totals {
    games: usize,
    ok: usize,
    greedy_fail: usize,
    greedy_run: usize,
    /// Failure class to (count, first example).
    classes: BTreeMap<String, (usize, String)>,
    /// Failures per case kind and player.
    by_kind: BTreeMap<String, (usize, usize)>,
    repro: Vec<String>,
    greedy_repro: Vec<String>,
    millis: u128,
}

fn run(map: &Map, case: &Case, player: Player, greedy: bool, totals: &Mutex<Totals>) {
    let defaults = Config::defaults(&config::definition(map));
    let started = Instant::now();
    let name = format!("{player:?}").to_lowercase();
    let repro = format!(
        "case {} ({} {}) seed {} player {name} config {}",
        case.index,
        case.kind,
        case.label,
        case.seed,
        changed(&case.config, &defaults)
    );
    let outcome = std::panic::catch_unwind(|| {
        let plan = match generate(map, &case.config, case.seed, Players::Only(player)) {
            Ok(plan) => plan,
            Err(err) => return (vec![format!("generate: {err:#}")], None),
        };
        let game = &plan.games[0];
        let problems = audit::check_all(map, game);
        let greedy = if greedy && problems.is_empty() {
            Some(audit::greedy_spend(map, game).err())
        } else {
            None
        };
        (problems, greedy)
    });
    let elapsed = started.elapsed().as_millis();
    let mut totals = totals.lock().unwrap();
    totals.games += 1;
    totals.millis += elapsed;
    let entry = totals.by_kind.entry(case.kind.to_string()).or_default();
    entry.0 += 1;
    let (problems, greedy_result) = match outcome {
        Ok(result) => result,
        Err(panic) => {
            let message = panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default();
            (vec![format!("panic: {message}")], None)
        }
    };
    if let Some(result) = greedy_result {
        totals.greedy_run += 1;
        if let Some(err) = result {
            totals.greedy_fail += 1;
            totals
                .greedy_repro
                .push(format!("{repro}\n    {}", class_of(&err)));
        }
    }
    if problems.is_empty() {
        totals.ok += 1;
        return;
    }
    totals.by_kind.get_mut(case.kind).unwrap().1 += 1;
    for problem in &problems {
        let class = class_of(problem);
        let slot = totals
            .classes
            .entry(class)
            .or_insert_with(|| (0, problem.clone()));
        slot.0 += 1;
    }
    totals.repro.push(format!(
        "{repro}\n    {}",
        problems.join("\n    ").replace('\n', "\n    ")
    ));
}

fn env(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn sweep(cases: usize, first: u64, threads: usize, greedy: bool) -> (String, usize) {
    let map = load_map();
    if let Err(err) = audit::check_map_data(&map) {
        panic!("the map's own data: {err}");
    }
    let mut list = build_cases(&map, cases, first);
    if let Some(only) = std::env::var("SWEEP_ONLY")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
    {
        list.retain(|case| case.index == only);
    }
    let next = AtomicUsize::new(0);
    let totals = Mutex::new(Totals::default());
    let started = Instant::now();
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                // Plans recurse deeply in places; give the workers room.
                loop {
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    let Some(case) = list.get(i) else {
                        break;
                    };
                    for player in [Player::Chris, Player::Jill] {
                        run(&map, case, player, greedy, &totals);
                    }
                }
            });
        }
    });
    let totals = totals.into_inner().unwrap();
    let mut out = String::new();
    let _ = writeln!(
        out,
        "sweep: {} games planned ({} cases x 2 players, seeds {first}..{}), {} passed every check, {:.1}s wall, {:.0} ms/game",
        totals.games,
        list.len(),
        first + cases as u64 - 1,
        totals.ok,
        started.elapsed().as_secs_f64(),
        totals.millis as f64 / totals.games.max(1) as f64
    );
    for (kind, (games, failed)) in &totals.by_kind {
        let _ = writeln!(out, "  {kind}: {games} games, {failed} failed");
    }
    let _ = writeln!(out, "failure classes:");
    for (class, (count, example)) in &totals.classes {
        let _ = writeln!(
            out,
            "  {count:5}  {class}\n         e.g. {}",
            example.lines().next().unwrap_or("")
        );
    }
    if greedy {
        let _ = writeln!(
            out,
            "greedy spend (consumable keys spent on the first lock found): {} of {} passing games strand the player ({:.2}%)",
            totals.greedy_fail,
            totals.greedy_run,
            100.0 * totals.greedy_fail as f64 / totals.greedy_run.max(1) as f64
        );
    }
    let _ = writeln!(out, "\nfailures to reproduce:");
    for line in &totals.repro {
        let _ = writeln!(out, "{line}");
    }
    let _ = writeln!(out, "\ngreedy-spend strandings:");
    for line in &totals.greedy_repro {
        let _ = writeln!(out, "{line}");
    }
    (out, totals.repro.len())
}

#[test]
#[ignore = "the sweep takes minutes; see the module docs"]
fn re1_sweep() {
    let cases = env("SWEEP_CASES", 1000);
    let first = env("SWEEP_FIRST", 1) as u64;
    let threads = env("SWEEP_THREADS", 6);
    let greedy = env("SWEEP_GREEDY", 1) != 0;
    let (summary, failures) = sweep(cases, first, threads, greedy);
    println!("{summary}");
    if let Ok(path) = std::env::var("SWEEP_OUT") {
        std::fs::write(&path, &summary).unwrap_or_else(|err| panic!("{path}: {err}"));
    }
    assert_eq!(failures, 0, "{failures} games failed a check");
}

/// A few cases of every kind, fast enough to run with the other tests.
#[test]
fn re1_sweep_smoke() {
    let (summary, failures) = sweep(9, 1, 4, false);
    assert_eq!(failures, 0, "{summary}");
}

/// Reruns one failure from a sweep's summary: `SWEEP_CONFIG` is the JSON of
/// its `config` (options over the defaults), `SWEEP_SEED` its seed and
/// `SWEEP_PLAYER` `chris` or `jill`. Prints every problem the checks find.
#[test]
#[ignore = "reruns one case; see the module docs"]
fn re1_repro() {
    let map = load_map();
    let mut config = Config::defaults(&config::definition(&map));
    let json = std::env::var("SWEEP_CONFIG").unwrap_or_else(|_| "{}".into());
    config = config.merge(&Config::from_json(&json).expect("SWEEP_CONFIG is JSON"));
    let seed = env("SWEEP_SEED", 1) as u64;
    let player = match std::env::var("SWEEP_PLAYER").as_deref() {
        Ok("jill") => Player::Jill,
        _ => Player::Chris,
    };
    let plan = generate(&map, &config, seed, Players::Only(player))
        .unwrap_or_else(|err| panic!("generate: {err:#}"));
    let game = &plan.games[0];
    let problems = audit::check_all(&map, game);
    if std::env::var("SWEEP_LOG").is_ok() {
        if let Some(keys) = &game.keys {
            println!("KEY LOG\n{}", keys.log.join("\n"));
        }
    } else {
        println!("{}", game.spoiler.to_text());
    }
    for problem in &problems {
        println!("PROBLEM: {problem}");
    }
    if let Ok(names) = std::env::var("SWEEP_DOORS") {
        for lock in game.locks.iter().filter(|l| {
            names
                .split(',')
                .any(|n| n == format!("{}:{}", l.room, l.id))
        }) {
            println!("LOCK {lock:?}");
        }
    }
    println!("greedy spend: {:?}", audit::greedy_spend(&map, game));
    assert!(problems.is_empty());
}

/// Writes the sweep's cases as files, for the full-pack sweep
/// (`scripts/biorand-sweep.sh`): `SWEEP_DUMP=<dir>` gets `case-<n>.json` (the
/// options that differ from the defaults, as `arklay biorand generate
/// --config` reads them) and `cases.txt` (`<n> <seed> <kind> <label>`).
#[test]
#[ignore = "writes the case files; see the module docs"]
fn re1_sweep_dump() {
    let Ok(dir) = std::env::var("SWEEP_DUMP") else {
        panic!("SWEEP_DUMP names the directory to write the cases to");
    };
    let map = load_map();
    let defaults = Config::defaults(&config::definition(&map));
    std::fs::create_dir_all(&dir).expect("the directory is created");
    let mut index = String::new();
    let cases = build_cases(&map, env("SWEEP_CASES", 100), env("SWEEP_FIRST", 1) as u64);
    for case in &cases {
        let path = format!("{dir}/case-{}.json", case.index);
        std::fs::write(&path, changed(&case.config, &defaults)).expect("a case is written");
        let _ = writeln!(
            index,
            "{} {} {} {}",
            case.index,
            case.seed,
            case.kind,
            case.label.replace(' ', "_")
        );
    }
    std::fs::write(format!("{dir}/cases.txt"), index).expect("the index is written");
}
