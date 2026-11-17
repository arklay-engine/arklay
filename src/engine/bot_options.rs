//! The player bot's run options for an app boot ([`super::run_ui_with_mods`]),
//! known to every build: one without the `bot` feature refuses a boot that
//! asks for the bot.

/// The default headless tick budget (about 55 minutes at 30 Hz).
pub const DEFAULT_MAX_TICKS: u64 = 100_000;

/// The default risk: how much danger the planner may accept, 0..=1.
pub const DEFAULT_RISK: f64 = 0.25;

/// The bot run options an app boot adds on top of a normal one.
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
    /// The planner's risk setting, `0.0..=1.0`; read-only to
    /// Lua as `bot:risk()`.
    pub risk: f64,
    /// The room whose entry ends the run, such as
    /// `"20D"`; read-only to Lua as `bot:goal_room()`.
    pub goal_room: Option<String>,
    /// The game-RNG seed; `None` leaves the game's own random
    /// stream untouched.
    pub seed: Option<u32>,
    /// The story parameters picked for this run,
    /// checked against the pack's `params/<game>.xml` when the bot attaches.
    pub params: Vec<(String, String)>,
    /// The debug INVINCIBLE cheat for every session of the run
    /// (the bot's `invincible` option): health stays full and the game-over machine
    /// never runs. The bot is not told; it plays as it would.
    pub invincible: bool,
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
