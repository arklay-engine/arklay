//! BioRand: the engine's side of generated games.
//!
//! The randomizer is the `arklay-biorand` package, a pure library that reads a
//! map and plans a game. This module feeds it from a base pack and writes its
//! plan back as a game: [`generate`] reads the map (`biorand/map.xml` and each
//! room's `random.xml`) and the configuration, plans the game, and [`emit`]s a
//! new base pack per planned player: the base pack's entries with the planned
//! pickups written into the room scripts, and a manifest carrying the seed,
//! the system and scenario flags, the start inventory and the slot counts.
//! [`verify_pack`] reads a finished pack back and checks it against the plan,
//! which it makes again from what the pack records.
//!
//! Milestones 5 and 6 of `docs/biorand/plan.md` apply items and locks: the
//! doors and enemies stay the original's, whatever the configuration asks,
//! and the pack raises none of their flags (see [`emit::applied_flags`]).
//!
//! The same seed, configuration and [`GENERATOR_VERSION`] always make the same
//! bytes; bump the version whenever a change would alter a seed's output.

mod emit;
mod enemies;
mod locks;
mod verify;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use arklay_biorand::Players;
use arklay_biorand::config::{self, Config};
use arklay_biorand::map::{Map, Player};
use arklay_biorand::plan::{Game, Plan};
use serde_json::{Value as Json, json};

use crate::pack::Pack;

pub use verify::Report;

/// The version of the emitter's output for a given plan. A pack also records
/// the plan's own version ([`arklay_biorand::plan::GENERATOR_VERSION`]) as
/// `plan_version`; a change to either makes an older pack stale.
pub const GENERATOR_VERSION: u32 = 3;

/// The base pack a game is generated from.
pub const BASE_PACK_ID: &str = "re1";

/// The largest seed: seeds are six digits.
pub const MAX_SEED: u32 = 999_999;
/// How many digits a seed can be written with.
pub const SEED_DIGITS: usize = 6;

/// The pack entry recording how a pack was generated: the seed, the players
/// asked for and the configuration as given, so [`verify_pack`] can plan the
/// same game again.
pub const REQUEST_ENTRY: &str = "biorand/request.json";
/// The pack entries holding the spoiler log.
pub const SPOILER_JSON_ENTRY: &str = "biorand/spoiler.json";
pub const SPOILER_TEXT_ENTRY: &str = "biorand/spoiler.txt";

mod checked_in {
    include!(concat!(env!("OUT_DIR"), "/biorand_files.rs"));
}

/// The checked-in map-wide BioRand files (`games/re1/biorand/*.xml`) as
/// `(pack entry, text)`. `convert-game` packs exactly these, beside the rooms'
/// `random.xml` files.
pub use checked_in::BIORAND_FILES;

/// The pack id of a seed's game.
pub fn pack_id(seed: u32) -> String {
    format!("biorand-{seed:06}")
}

/// A seed's game file in the games directory.
pub fn pack_path(games_dir: &Path, seed: u32) -> PathBuf {
    games_dir.join(format!("{}.akpak", pack_id(seed)))
}

/// A seed from the system clock, for the launcher to offer.
pub fn random_seed() -> u32 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.subsec_nanos());
    nanos.wrapping_mul(2_654_435_761) % (MAX_SEED + 1)
}

/// What to generate besides the seed.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// The configuration, as staging's JSON gives it, over the definition's
    /// defaults: it need only name the options it changes.
    pub config: Option<Config>,
    /// Whose games to write: one pack per player planned.
    pub players: PlayerChoice,
    /// Write the packs even when a pack from the same seed, generator and base
    /// is already there.
    pub force: bool,
}

/// The players to plan, as the CLI and the request entry name them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlayerChoice {
    /// The configuration's `player` (drawn from the seed when `Random`).
    #[default]
    Configured,
    Chris,
    Jill,
    /// Both players' games, as two packs.
    Both,
}

impl PlayerChoice {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Configured => "configured",
            Self::Chris => "chris",
            Self::Jill => "jill",
            Self::Both => "both",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Some(match text.to_ascii_lowercase().as_str() {
            "configured" => Self::Configured,
            "chris" => Self::Chris,
            "jill" => Self::Jill,
            "both" => Self::Both,
            _ => return None,
        })
    }

    fn players(self) -> Players {
        match self {
            Self::Configured => Players::Configured,
            Self::Chris => Players::Only(Player::Chris),
            Self::Jill => Players::Only(Player::Jill),
            Self::Both => Players::Both,
        }
    }
}

/// What a pack records about its generation ([`REQUEST_ENTRY`]).
#[derive(Clone, Debug, PartialEq)]
struct Request {
    seed: u32,
    players: PlayerChoice,
    /// The configuration as given, before it is resolved.
    config: Config,
}

impl Request {
    fn to_json(&self) -> String {
        let config: Json =
            serde_json::from_str(&self.config.to_json()).expect("a configuration is JSON");
        let value = json!({
            "seed": self.seed,
            "players": self.players.as_str(),
            "config": config,
        });
        let mut text = serde_json::to_string_pretty(&value).expect("JSON values serialize");
        text.push('\n');
        text
    }

    fn from_json(text: &str) -> Result<Self> {
        let value: Json = serde_json::from_str(text).context("not JSON")?;
        let seed = value["seed"]
            .as_u64()
            .and_then(|seed| u32::try_from(seed).ok())
            .ok_or_else(|| anyhow!("no `seed`"))?;
        let players = value["players"]
            .as_str()
            .and_then(PlayerChoice::parse)
            .ok_or_else(|| anyhow!("no `players`"))?;
        let config = Config::from_json(&value["config"].to_string()).context("bad `config`")?;
        Ok(Self {
            seed,
            players,
            config,
        })
    }

    /// Plans the request's games on `map`, with the options this milestone
    /// doesn't apply turned off.
    fn plan(&self, map: &Map) -> Result<Plan> {
        arklay_biorand::generate(
            map,
            &self.config,
            u64::from(self.seed),
            self.players.players(),
        )
    }
}

/// The map a pack carries: `biorand/map.xml` and every room's `random.xml`,
/// read in room directory order.
fn read_map(pack: &Pack) -> Result<Map> {
    let map_xml = read_text(pack, "biorand/map.xml")?;
    let mut rooms: Vec<(String, String)> = Vec::new();
    for path in pack.own_paths() {
        let Some(dir) = path
            .strip_prefix("room/")
            .and_then(|rest| rest.strip_suffix("/random.xml"))
        else {
            continue;
        };
        rooms.push((dir.to_ascii_lowercase(), read_text(pack, path)?));
    }
    if rooms.is_empty() {
        bail!("the pack has no room random.xml files");
    }
    rooms.sort();
    Map::parse(
        &map_xml,
        rooms
            .iter()
            .map(|(dir, text)| (dir.as_str(), text.as_str())),
    )
}

fn read_text(pack: &Pack, path: &str) -> Result<String> {
    let bytes = pack.read(path).with_context(|| format!("no {path}"))?;
    String::from_utf8(bytes.into_owned()).with_context(|| format!("{path} is not UTF-8"))
}

/// The default configuration of `map`'s definition.
pub fn default_config(map: &Map) -> Config {
    Config::defaults(&config::definition(map))
}

/// The character byte (`0` Chris, `1` Jill) of a player.
fn character_of(player: Player) -> u8 {
    match player {
        Player::Chris => 0,
        Player::Jill => 1,
    }
}

fn player_name(player: Player) -> &'static str {
    match player {
        Player::Chris => "chris",
        Player::Jill => "jill",
    }
}

/// One generated pack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Generated {
    pub path: PathBuf,
    pub player: Player,
}

/// Generate `seed`'s game from the pack at `base` into `games_dir` with the
/// default configuration and the configured player, as the launcher does:
/// `biorand-<seed>.akpak`. A pack already there from the same seed, generator
/// and base is kept. `progress` gets `(done, total)` while the pack is
/// written.
pub fn generate(
    base: &Path,
    seed: u32,
    games_dir: &Path,
    jobs: usize,
    progress: &(dyn Fn(u64, u64) + Sync),
) -> Result<PathBuf> {
    let packs = generate_with(base, seed, games_dir, &Options::default(), jobs, progress)?;
    packs
        .into_iter()
        .next()
        .map(|generated| generated.path)
        .ok_or_else(|| anyhow!("seed {seed} planned no game"))
}

/// Generate `seed`'s games from the pack at `base` into `games_dir`: one
/// pack per player the options plan, `biorand-<seed>.akpak` for one player
/// and `biorand-<seed>-<player>.akpak` for each of both. Every pack is
/// verified once written, and deleted if it fails.
pub fn generate_with(
    base: &Path,
    seed: u32,
    games_dir: &Path,
    options: &Options,
    jobs: usize,
    progress: &(dyn Fn(u64, u64) + Sync),
) -> Result<Vec<Generated>> {
    if seed > MAX_SEED {
        bail!("seed {seed} has more than six digits");
    }
    if !options.force
        && options.config.is_none()
        && options.players == PlayerChoice::Configured
        && let Some(existing) = reusable(base, seed, games_dir)?
    {
        return Ok(vec![existing]);
    }
    let source = Pack::open(base).with_context(|| format!("failed to open {}", base.display()))?;
    let base_manifest = source
        .manifest()
        .ok_or_else(|| anyhow!("{} has no manifest", base.display()))?;
    if base_manifest.seed.is_some() {
        bail!(
            "{} is a generated game; generate from the base game",
            base.display()
        );
    }
    let map = read_map(&source)
        .with_context(|| format!("failed to read the map of {}", base.display()))?;
    // A configuration names only the options it changes: the rest keep the
    // definition's defaults rather than staging's fallbacks for a missing key.
    let defaults = default_config(&map);
    let request = Request {
        seed,
        players: options.players,
        config: options
            .config
            .as_ref()
            .map_or_else(|| defaults.clone(), |config| defaults.merge(config)),
    };
    let plan = request
        .plan(&map)
        .with_context(|| format!("seed {seed} has no game"))?;

    std::fs::create_dir_all(games_dir)
        .with_context(|| format!("failed to create {}", games_dir.display()))?;
    let mut written = Vec::new();
    for game in &plan.games {
        let id = match options.players {
            PlayerChoice::Both => format!("{}-{}", pack_id(seed), player_name(game.player)),
            _ => pack_id(seed),
        };
        let out = games_dir.join(format!("{id}.akpak"));
        let writer = emit::pack(&source, &id, &request, &plan, game)
            .with_context(|| format!("failed to write the {} game", player_name(game.player)))?;
        emit::write(writer, &out, jobs, progress)?;
        if let Err(err) = verify::verify(&out, Some(&source)) {
            // Leave nothing the Play list would offer.
            let _ = std::fs::remove_file(&out);
            return Err(err.context(format!("seed {seed} made a broken game")));
        }
        written.push(Generated {
            path: out,
            player: game.player,
        });
    }
    Ok(written)
}

/// The launcher's pack for `seed` when it is already there from this
/// generator and base, generated with the defaults.
fn reusable(base: &Path, seed: u32, games_dir: &Path) -> Result<Option<Generated>> {
    let out = pack_path(games_dir, seed);
    let Ok(Some(existing)) = crate::pack::read_manifest(&out) else {
        return Ok(None);
    };
    let Some(base_manifest) = crate::pack::read_manifest(base)? else {
        return Ok(None);
    };
    let current = existing.seed == Some(seed)
        && existing.generator_version == Some(GENERATOR_VERSION)
        && existing.plan_version == Some(arklay_biorand::plan::GENERATOR_VERSION)
        && existing.convert_version == base_manifest.convert_version
        && existing.source_hash == base_manifest.source_hash;
    let player = match existing.character {
        Some(1) => Player::Jill,
        _ => Player::Chris,
    };
    Ok(current.then_some(Generated { path: out, player }))
}

/// Check a generated pack, as [`verify_with`] with the base pack found beside
/// it (`re1.akpak`), when there is one.
pub fn verify_pack(path: &Path) -> Result<Report> {
    let base = path
        .parent()
        .map(|dir| dir.join(format!("{BASE_PACK_ID}.akpak")))
        .filter(|base| base.is_file());
    verify_with(path, base.as_deref())
}

/// Check a generated pack against the plan made again from what it records,
/// and, given its `base` pack, that nothing else differs from the base.
pub fn verify_with(path: &Path, base: Option<&Path>) -> Result<Report> {
    let base = base
        .map(|base| Pack::open(base).with_context(|| format!("failed to open {}", base.display())))
        .transpose()?;
    verify::verify(path, base.as_ref())
}

/// The game of `plan` that `manifest`'s character plays.
fn game_for(plan: &Plan, character: u8) -> Option<&Game> {
    plan.games
        .iter()
        .find(|game| character_of(game.player) == character)
}

#[cfg(test)]
mod tests;
