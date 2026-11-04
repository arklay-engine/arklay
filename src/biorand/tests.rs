use std::path::{Path, PathBuf};

use super::*;
use crate::manifest::Manifest;
use crate::pack::PackWriter;

/// A scratch directory removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("arklay-biorand-{}-{label}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const QUIET: &(dyn Fn(u64, u64) + Sync) = &|_, _| {};

/// A base pack of the checked-in room definitions, room scripts and map: all
/// a game is planned and emitted from.
fn base_pack(dir: &Path) -> PathBuf {
    let mut manifest = Manifest::base(BASE_PACK_ID);
    manifest.name = Some("Resident Evil".to_string());
    manifest.convert_version = Some(1);
    manifest.source_hash = Some("0123456789abcdef".to_string());
    base_pack_with(dir, manifest)
}

/// A base pack with `manifest`.
fn base_pack_with(dir: &Path, manifest: Manifest) -> PathBuf {
    let mut writer = PackWriter::new();
    writer
        .add(crate::manifest::ENTRY, manifest.render().into_bytes())
        .unwrap();
    for (path, text) in crate::room::RE1_FILES {
        writer
            .add(&format!("room/{path}"), text.as_bytes().to_vec())
            .unwrap();
    }
    for script in crate::scd::ROOM_SCRIPTS {
        writer
            .add(
                &format!("room/{:03x}/room.scd", script.room),
                script.container.to_vec(),
            )
            .unwrap();
    }
    for (path, text) in BIORAND_FILES {
        writer.add(path, text.as_bytes().to_vec()).unwrap();
    }
    writer.add("bgm/000.wav", b"music".to_vec()).unwrap();
    writer.compress_entries(0, |_| false).unwrap();
    let path = dir.join(format!("{BASE_PACK_ID}.akpak"));
    writer.write(&path).unwrap();
    path
}

/// The pack folder room directory `dir` is in: its new id when the game's
/// doors are randomized, `None` for a room the game leaves out.
fn folder(game: &arklay_biorand::plan::Game, dir: &str) -> Option<String> {
    match &game.doors {
        Some(plan) => plan.new_id(dir).map(str::to_string),
        None => Some(dir.to_string()),
    }
}

fn both(seed: u32, dir: &TempDir) -> (PathBuf, Vec<Generated>) {
    let base = base_pack(&dir.0);
    let options = Options {
        players: PlayerChoice::Both,
        ..Options::default()
    };
    let packs = generate_with(&base, seed, &dir.0.join("games"), &options, 0, QUIET).unwrap();
    (base, packs)
}

#[test]
fn a_seed_names_its_pack() {
    assert_eq!(pack_id(42), "biorand-000042");
    assert_eq!(
        pack_path(Path::new("games"), 42),
        Path::new("games").join("biorand-000042.akpak")
    );
}

#[test]
fn the_map_files_are_checked_in_under_biorand() {
    assert!(
        BIORAND_FILES
            .iter()
            .any(|(entry, _)| *entry == "biorand/map.xml")
    );
}

#[test]
fn a_request_round_trips_through_json() {
    let mut config = Config::new();
    config.set("items/random", true);
    config.set("player", "Jill");
    let request = Request {
        seed: 4321,
        players: PlayerChoice::Both,
        config,
    };
    assert_eq!(Request::from_json(&request.to_json()).unwrap(), request);
    assert!(Request::from_json("{}").is_err());
    for choice in ["configured", "chris", "jill", "both"] {
        assert_eq!(PlayerChoice::parse(choice).unwrap().as_str(), choice);
    }
    assert_eq!(PlayerChoice::parse("rebecca"), None);
}

#[test]
fn a_seed_makes_a_verified_pack_per_player() {
    let dir = TempDir::new("both");
    let (base, packs) = both(42, &dir);
    let players: Vec<Player> = packs.iter().map(|generated| generated.player).collect();
    assert_eq!(players, [Player::Chris, Player::Jill]);
    for (generated, name) in packs.iter().zip(["chris", "jill"]) {
        assert_eq!(
            generated.path.file_name().unwrap().to_str().unwrap(),
            format!("biorand-000042-{name}.akpak")
        );
        let report = verify_with(&generated.path, Some(&base)).unwrap();
        assert!(report.against_base);
        // The default configuration randomizes the doors, which leaves some
        // rooms out.
        assert!(report.rooms > 50, "{report}");
        assert!(report.pickups > 50, "{report}");
        assert!(
            report.routed_doors > 50 && report.walked_rooms > 50,
            "{report}"
        );
        // The base is found beside the games directory's packs only when it
        // is there.
        assert!(!verify_pack(&generated.path).unwrap().against_base);

        let pack = Pack::open(&generated.path).unwrap();
        let manifest = pack.manifest().unwrap();
        assert_eq!(manifest.seed, Some(42));
        assert_eq!(manifest.generator_version, Some(GENERATOR_VERSION));
        assert_eq!(
            manifest.plan_version,
            Some(arklay_biorand::plan::GENERATOR_VERSION)
        );
        assert_eq!(manifest.character, Some(character_of(generated.player)));
        assert_eq!(manifest.source_hash.as_deref(), Some("0123456789abcdef"));
        let flags = manifest.biorand_flags.as_ref().unwrap();
        assert!(flags.iter().any(|flag| flag == "ITEM_RANDOMIZER"));
        // Random doors write every door's lock byte; the locks themselves
        // are not randomized.
        for on in ["DOOR_RANDOMIZER", "BR_LOCK_FULL_BYTE"] {
            assert!(flags.iter().any(|flag| flag == on), "{on}");
        }
        assert!(!flags.iter().any(|flag| flag == "LOCK_RANDOMIZER"));
        // The game starts in the main hall, wherever it is now.
        let plan =
            Request::from_json(std::str::from_utf8(&pack.read(REQUEST_ENTRY).unwrap()).unwrap())
                .unwrap()
                .plan(&read_map(&pack).unwrap())
                .unwrap();
        let game = game_for(&plan, character_of(generated.player)).unwrap();
        let hall = game.doors.as_ref().unwrap().new_id("106").unwrap();
        assert_eq!(
            manifest
                .start_room
                .as_deref()
                .map(str::to_ascii_lowercase)
                .as_deref(),
            Some(hall)
        );
        let start = manifest.setup.start_items.as_ref().unwrap();
        assert!(!start.is_empty());
        assert!(
            manifest
                .setup
                .scenario_flags
                .iter()
                .any(|&(flag, _)| flag == 122)
        );
        for entry in [REQUEST_ENTRY, SPOILER_JSON_ENTRY, SPOILER_TEXT_ENTRY] {
            assert!(pack.contains(entry), "{entry}");
        }
        let spoiler =
            String::from_utf8(pack.read(SPOILER_TEXT_ENTRY).unwrap().into_owned()).unwrap();
        assert!(
            spoiler.contains("000042") || spoiler.contains("42"),
            "{spoiler}"
        );
        // The segments are the stages, so the debug room select says so
        // rather than naming the original's stages.
        let segments = game
            .doors
            .as_ref()
            .unwrap()
            .rooms
            .values()
            .map(|room| room.stage)
            .max()
            .unwrap();
        let names = crate::lua::stage_names(&pack);
        assert_eq!(names.len(), usize::from(segments));
        assert_eq!(names.first().map(String::as_str), Some("Seg. 1"));
        assert_eq!(names.last().cloned(), Some(format!("Seg. {segments}")),);
        // The other entries are the base's.
        assert_eq!(pack.read("bgm/000.wav").unwrap().as_ref(), b"music");
    }
}

#[test]
fn a_seed_always_makes_the_same_bytes() {
    let dir = TempDir::new("same");
    let base = base_pack(&dir.0);
    let options = Options {
        players: PlayerChoice::Jill,
        force: true,
        ..Options::default()
    };
    let first = generate_with(&base, 7, &dir.0.join("a"), &options, 0, QUIET).unwrap();
    let second = generate_with(&base, 7, &dir.0.join("b"), &options, 4, QUIET).unwrap();
    assert_eq!(
        std::fs::read(&first[0].path).unwrap(),
        std::fs::read(&second[0].path).unwrap()
    );
    assert_eq!(
        first[0].path.file_name().unwrap().to_str().unwrap(),
        "biorand-000007.akpak"
    );
    let other = generate_with(&base, 8, &dir.0.join("c"), &options, 0, QUIET).unwrap();
    assert_ne!(
        Pack::open(&first[0].path)
            .unwrap()
            .read(SPOILER_TEXT_ENTRY)
            .unwrap(),
        Pack::open(&other[0].path)
            .unwrap()
            .read(SPOILER_TEXT_ENTRY)
            .unwrap()
    );
}

#[test]
fn the_launchers_generate_keeps_a_current_pack() {
    let dir = TempDir::new("launcher");
    let base = base_pack(&dir.0);
    let games = dir.0.join("games");
    let path = generate(&base, 99, &games, 0, QUIET).unwrap();
    assert_eq!(path, pack_path(&games, 99));
    let written = std::fs::metadata(&path).unwrap().modified().unwrap();
    assert_eq!(generate(&base, 99, &games, 0, QUIET).unwrap(), path);
    assert_eq!(
        std::fs::metadata(&path).unwrap().modified().unwrap(),
        written,
        "the pack is reused"
    );
}

/// Whether `options` would reuse the pack of `seed` already in `games`.
fn reused(base: &Path, seed: u32, games: &Path, options: &Options) -> bool {
    let path = pack_path(games, seed);
    let written = std::fs::metadata(&path).unwrap().modified().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(30));
    generate_with(base, seed, games, options, 0, QUIET).unwrap();
    std::fs::metadata(&path).unwrap().modified().unwrap() == written
}

#[test]
fn a_pack_is_rebuilt_when_the_options_or_the_base_change() {
    let dir = TempDir::new("stale");
    let base = base_pack(&dir.0);
    let games = dir.0.join("games");
    let defaults = default_config(&read_map(&Pack::open(&base).unwrap()).unwrap());
    let with = |id: &str, value: bool| {
        let mut config = defaults.clone();
        config.set(id, value);
        Options {
            config: Some(config),
            ..Options::default()
        }
    };

    // The first pack is made with the options as the launcher holds them.
    let wanted = Options {
        config: Some(defaults.clone()),
        ..Options::default()
    };
    assert!(!is_generated(&base, 7, &games, &wanted), "nothing yet");
    generate_with(&base, 7, &games, &wanted, 0, QUIET).unwrap();
    assert!(is_generated(&base, 7, &games, &wanted));
    assert!(!is_generated(&base, 8, &games, &wanted), "another seed");

    // The same options, spelled as staging's JSON spells them or by the
    // defaults alone, keep it.
    assert!(reused(&base, 7, &games, &wanted));
    assert!(reused(&base, 7, &games, &Options::default()));
    let reread = Config::from_json(&defaults.to_json()).unwrap();
    assert!(reused(
        &base,
        7,
        &games,
        &Options {
            config: Some(reread),
            ..Options::default()
        }
    ));

    // A changed option or player rebuilds, and then the new pack is current.
    let hard = with("hard", true);
    assert!(!is_generated(&base, 7, &games, &hard));
    assert!(!reused(&base, 7, &games, &hard));
    assert!(is_generated(&base, 7, &games, &hard));
    assert!(!is_generated(&base, 7, &games, &wanted));
    let chris = Options {
        players: PlayerChoice::Chris,
        ..hard.clone()
    };
    assert!(!is_generated(&base, 7, &games, &chris));
    assert!(!reused(&base, 7, &games, &chris));
    assert!(reused(&base, 7, &games, &chris));

    // A pack made by another generator is rebuilt, and a pack tells how it
    // was made.
    let path = pack_path(&games, 7);
    let recorded = pack_options(&path).unwrap();
    assert_eq!(recorded.players, PlayerChoice::Chris);
    assert!(recorded.config.unwrap().get_bool("hard", false));

    // A rebuilt base game makes every pack of it stale.
    let mut manifest = Manifest::base(BASE_PACK_ID);
    manifest.name = Some("Resident Evil".to_string());
    manifest.convert_version = Some(1);
    manifest.source_hash = Some("fedcba9876543210".to_string());
    let rebuilt = dir.0.join("rebuilt");
    std::fs::create_dir_all(&rebuilt).unwrap();
    let other = base_pack_with(&rebuilt, manifest);
    assert!(!is_generated(&other, 7, &games, &chris));
}

#[test]
fn a_generated_game_is_not_a_base() {
    let dir = TempDir::new("reseed");
    let (_, packs) = both(5, &dir);
    let err = generate_with(&packs[0].path, 6, &dir.0, &Options::default(), 0, QUIET)
        .unwrap_err()
        .to_string();
    assert!(err.contains("generate from the base game"), "{err}");
    assert!(
        generate_with(
            &packs[0].path,
            1_000_000,
            &dir.0,
            &Options::default(),
            0,
            QUIET
        )
        .is_err()
    );
}

#[test]
fn the_enemy_option_raises_its_flag_and_verifies() {
    let dir = TempDir::new("enemies");
    let base = base_pack(&dir.0);
    let mut config = Config::new();
    config.set("enemies/random", true);
    config.set("player", "Chris");
    let options = Options {
        config: Some(config),
        ..Options::default()
    };
    let packs = generate_with(&base, 3, &dir.0.join("games"), &options, 0, QUIET).unwrap();
    let pack = Pack::open(&packs[0].path).unwrap();
    let flags = pack.manifest().unwrap().biorand_flags.clone().unwrap();
    assert!(flags.iter().any(|flag| flag == "ENEMY_RANDOMIZER"));
    let spoiler: Json = serde_json::from_slice(&pack.read(SPOILER_JSON_ENTRY).unwrap()).unwrap();
    let deferred = spoiler["deferred"].as_array().unwrap();
    assert!(
        !deferred.iter().any(|option| option == "enemies/random"),
        "{deferred:?}"
    );
    let request =
        Request::from_json(std::str::from_utf8(&pack.read(REQUEST_ENTRY).unwrap()).unwrap())
            .unwrap();
    assert!(request.config.get_bool("enemies/random", false));
    verify_with(&packs[0].path, Some(&base)).unwrap();
}

/// `pack` with entry `path` replaced by `edit` of its bytes.
fn tamper(pack: &Path, out: &Path, path: &str, edit: impl Fn(&mut Vec<u8>)) {
    let source = Pack::open(pack).unwrap();
    let mut writer = PackWriter::new();
    for entry in source.own_paths() {
        let mut data = source.read(entry).unwrap().into_owned();
        if entry == path {
            edit(&mut data);
        }
        writer.add(entry, data).unwrap();
    }
    writer.write(out).unwrap();
}

#[test]
fn verification_names_what_is_wrong() {
    let dir = TempDir::new("tamper");
    let (base, packs) = both(11, &dir);
    let generated = &packs[0].path;
    let source = Pack::open(generated).unwrap();
    let plan_room = {
        // A room whose script the plan changes, and one of its pickups.
        let request =
            Request::from_json(std::str::from_utf8(&source.read(REQUEST_ENTRY).unwrap()).unwrap())
                .unwrap();
        let map = read_map(&source).unwrap();
        let plan = request.plan(&map).unwrap();
        let game = game_for(&plan, 0).unwrap();
        game.items
            .keys()
            .find(|dir| !emit::planned_flags(game, dir).is_empty() && folder(game, dir).is_some())
            .map(|dir| (dir.clone(), folder(game, dir).unwrap()))
            .unwrap()
    };
    let (plan_room, plan_folder) = plan_room;
    let path = format!("room/{plan_folder}/room.scd");
    let scd = source.read(&path).unwrap().into_owned();
    let scripts = crate::scd::reader::parse(&scd).unwrap();
    let request =
        Request::from_json(std::str::from_utf8(&source.read(REQUEST_ENTRY).unwrap()).unwrap())
            .unwrap();
    let plan = request.plan(&read_map(&source).unwrap()).unwrap();
    let flags = emit::planned_flags(game_for(&plan, 0).unwrap(), &plan_room);
    let offset = emit::commands(&scripts)
        .find(|insn| insn.op == emit::ITEM_AOT_SET && flags.contains(&insn.bytes[emit::ITEM_FLAG]))
        .unwrap()
        .offset;

    // A pickup holding another amount.
    let out = dir.0.join("amount.akpak");
    tamper(generated, &out, &path, |data| {
        data[offset + emit::ITEM_AMOUNT] = data[offset + emit::ITEM_AMOUNT].wrapping_add(1);
    });
    let err = format!("{:#}", verify_with(&out, None).unwrap_err());
    assert!(err.contains(&path) && err.contains("the plan"), "{err}");

    // A byte the plan does not touch, which only the base comparison sees.
    let out = dir.0.join("other.akpak");
    tamper(generated, &out, &path, |data| {
        data[offset + 2] ^= 1;
    });
    verify_with(&out, None).unwrap();
    let err = format!("{:#}", verify_with(&out, Some(&base)).unwrap_err());
    assert!(err.contains("differs from the base's"), "{err}");

    // An entry the base doesn't have, and another flag.
    let out = dir.0.join("manifest.akpak");
    tamper(generated, &out, crate::manifest::ENTRY, |data| {
        let text = String::from_utf8(data.clone()).unwrap();
        *data = text
            .replace(
                "\"ITEM_RANDOMIZER\"",
                "\"ITEM_RANDOMIZER\", \"ENEMY_RANDOMIZER\"",
            )
            .into_bytes();
    });
    let err = format!("{:#}", verify_with(&out, Some(&base)).unwrap_err());
    assert!(err.contains("biorand_flags"), "{err}");

    // A spoiler edited.
    let out = dir.0.join("spoiler.akpak");
    tamper(generated, &out, SPOILER_TEXT_ENTRY, |data| data.push(b'!'));
    let err = format!("{:#}", verify_with(&out, Some(&base)).unwrap_err());
    assert!(err.contains("spoiler"), "{err}");
}

// Locks.

#[test]
fn a_locked_seed_writes_and_verifies_every_door_lock() {
    let dir = TempDir::new("locks");
    let base = base_pack(&dir.0);
    let mut config = Config::new();
    config.set("locks/random", true);
    let options = Options {
        config: Some(config),
        players: PlayerChoice::Both,
        ..Options::default()
    };
    let packs = generate_with(&base, 21, &dir.0.join("games"), &options, 0, QUIET).unwrap();
    assert_eq!(packs.len(), 2);
    for generated in &packs {
        let report = verify_with(&generated.path, Some(&base)).unwrap();
        assert!(report.doors > 100, "{report}");

        let source = Pack::open(&generated.path).unwrap();
        let flags = source.manifest().unwrap().biorand_flags.clone().unwrap();
        for flag in ["LOCK_RANDOMIZER", "BR_LOCK_FULL_BYTE"] {
            assert!(flags.iter().any(|f| f == flag), "{flag}");
        }
        let spoiler =
            String::from_utf8(source.read(SPOILER_TEXT_ENTRY).unwrap().into_owned()).unwrap();
        assert!(spoiler.contains("Locked doors:"), "{spoiler}");

        // A locked door's key, changed: the plain check sees it.
        let request =
            Request::from_json(std::str::from_utf8(&source.read(REQUEST_ENTRY).unwrap()).unwrap())
                .unwrap();
        let plan = request.plan(&read_map(&source).unwrap()).unwrap();
        let character = character_of(generated.player);
        let game = game_for(&plan, character).unwrap();
        let door = game
            .locks
            .iter()
            .find(|lock| lock.drawn && lock.key_item().is_some())
            .unwrap();
        let path = format!("room/{}/room.scd", folder(game, &door.dir).unwrap());
        let scd = source.read(&path).unwrap().into_owned();
        let scripts = crate::scd::reader::parse(&scd).unwrap();
        let planned = locks::planned(game, &door.dir);
        let offset = locks::doors(&scripts)
            .find(|insn| {
                locks::expected(&door.dir, &insn.bytes, &planned)
                    .unwrap()
                    .planned
                    .is_some_and(|planned| planned == door)
            })
            .unwrap()
            .offset;
        assert_eq!(scd[offset + locks::DOOR_LOCK], door.lock);
        assert_eq!(scd[offset + locks::DOOR_KEY], door.key);
        let out = dir.0.join("key.akpak");
        tamper(&generated.path, &out, &path, |data| {
            data[offset + locks::DOOR_KEY] ^= 1;
        });
        let err = format!("{:#}", verify_with(&out, None).unwrap_err());
        assert!(
            err.contains(&path) && err.contains("the plan lock"),
            "{err}"
        );
    }
}
