//! The user configuration file and the default save location.
//!
//! `arklay.toml` lives in the per-user configuration directory: an existing
//! local `arklay.toml` (`%LOCALAPPDATA%\arklay` on Windows,
//! `$XDG_DATA_HOME/arklay` on Linux) takes over, otherwise the roaming
//! directory is used (`%APPDATA%\arklay` / `$XDG_CONFIG_HOME/arklay`), and
//! `$ARKLAY_CONFIG_DIR` overrides both. The file is optional: a missing file
//! is the default configuration. The grammar is the same small TOML subset the pack manifest
//! uses — comments, blank lines and `key = "value"`, plus TOML's literal
//! `key = 'value'` strings so a Windows path needs no escaping — with two
//! root keys and six tables (the `[graphics]` table also takes bare integers
//! and booleans, and so does `[game]`):
//!
//! ```toml
//! # Where saves are read and written (default: <config dir>/save).
//! save_dir = "~/.config/arklay/save"
//! # The language tag, when `--language` is not given.
//! language = "en-US"
//!
//! # Input bindings, one single-line array of sources per action; an action
//! # left out keeps its default (see `crate::controls`).
//! [bindings]
//! aim = ["key:C", "mouse:right", "axis:lefttrigger+"]
//!
//! # Gameplay options, both on by default (`--quick-turn` and
//! # `--tactical-reload` override them; `--no-...` turns one off for a run).
//! [game]
//! quick_turn = true      # BACKWARD + RUN turns the player 180 degrees
//! tactical_reload = true # AIM + RUN reloads a partially loaded gun
//! analog_movement = true # the left stick moves relative to the camera
//! auto_aim = true        # raising a weapon locks on to the nearest enemy
//! blood = true           # blood billboards, spurts and pools
//! chris_ink_ribbons = true # saving as Chris needs an ink ribbon
//! jill_ink_ribbons = false # saving as Jill needs an ink ribbon
//! autosave = true        # each door's room load writes autosave1.dat
//! room_script_fixes = true # run the room scripts with the port's fixes;
//!                        # false runs the original bytecode
//! ui_font = "modern"     # the menus' face: "modern" or "retro"
//! menu_text_size = "standard" # "small", "standard", "large"
//! modern_save_screen = true # the full-screen save/load list; false keeps
//!                        # the original's eight-slot screen
//! button_style = "auto"  # controller glyphs: "auto", "xbox", "playstation"
//!                        # or "nintendo"
//! control_type = "a"     # the gamepad layout preset last picked in the
//!                        # menu: "a", "b" or "c"; picking one rewrites the
//!                        # pad sources in [bindings], which is what's used
//! vibration = "normal"   # "off", "weak", "normal" or "strong"
//! master_volume = 100    # levels in percent (0..100): everything, then the
//! music_volume = 100     # music, the ambient loops (rain, machinery), the
//! ambient_volume = 100   # sound effects, the voices and the menus' cues
//! effects_volume = 100
//! voice_volume = 100
//! menu_volume = 100
//! subtitles = true       # the subtitle options (stored; not drawn yet)
//! subtitle_size = "standard"   # "small", "standard" or "large"
//! subtitle_background = 50     # the band's opacity in percent
//! subtitle_color = "white"     # "white", "red", "blue", "green", "yellow"
//! speaker_names = false
//! speaker_name_color = "yellow"
//!
//! # The interactive window (see `crate::video`); a key left out keeps its
//! # default. F8 and Alt+Enter rewrite `fullscreen` and the window size, and
//! # moving the window to another display rewrites `display`.
//! [graphics]
//! renderer = "gpu"       # or "software"; `--renderer` overrides it
//!                        # (GPU is the default and falls back to software)
//! fullscreen = false     # borderless fullscreen
//! vsync = true           # wait for the display refresh before presenting
//! window_width = 960     # the window's client size in pixels (F8 picks
//! window_height = 720    # multiples of 320x240; any size is letterboxed)
//! display = 1            # the display to open on, from 1
//! render_scale = 100     # GPU: the 3D scene's size, % of the viewport (25..400)
//! msaa = 1               # GPU: the 3D scene's samples per pixel (1, 2, 4, 8)
//! frame_rate = 30        # presented frames per second (30..1000); the game
//!                        # still runs at 30 Hz and higher rates tween
//! aspect = "4:3"         # or "16:9": crops the 4:3 frame to the wider shape
//! hdr = true             # HDR output when the display supports it
//!
//! # Discovered mods (the game pack's sibling `mods/` and `<config dir>/mods`).
//! # `enabled` narrows the discovered set when present; `disabled` always
//! # wins; `order` names ids applied first, in that order. Explicit `--mod`
//! # layers bypass the enable/disable selection.
//! [mods]
//! enabled = ["hd", "demo"]
//! disabled = ["balance"]
//! order = ["demo", "hd"]
//!
//! # The launcher's remembered installation (written after a successful
//! # install or rebuild, not usually edited by hand).
//! [games]
//! re1_source = 'C:\Games\Resident Evil'
//! re1_build = "21744136"
//!
//! # Update track and the last build found by a check.
//! [updates]
//! channel = "release"        # or "nightly"
//! check = true               # check on startup
//! last_check = 1775          # unix seconds
//! available_version = "0.1.1"
//! available_sha = "..."
//! ```
//!
//! Saved games default to `<config dir>/save` (the `savedat1..8.dat` slot
//! names and the 2690-byte block are unchanged), so a run from any pack
//! location reads the same saves.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

use crate::budget;
use crate::controls::{ACTIONS, Action, Bindings, Source};
use crate::video::{self, Graphics, RendererKind};

/// The configuration file's name inside the configuration directory.
pub const FILE_NAME: &str = crate::paths::CONFIG_FILE;
/// The per-user directory name under the platform's configuration root.
pub const DIR_NAME: &str = crate::paths::APP_DIR;
/// The environment variable that relocates the configuration directory (and
/// with it the default save directory).
pub const DIR_ENV: &str = crate::paths::CONFIG_ENV;
/// The default save directory's name inside the configuration directory.
pub const SAVE_DIR_NAME: &str = "save";
/// The per-user mods directory's name inside the configuration directory.
pub const MODS_DIR_NAME: &str = "mods";

/// The `[mods]` selection over auto-discovered mod layers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Mods {
    /// When `Some`, only these discovered mod ids apply; `None` means every
    /// discovered mod that is not disabled.
    pub enabled: Option<Vec<String>>,
    /// Discovered mod ids that never apply.
    pub disabled: Vec<String>,
    /// Mod ids applied first, in this order; the rest follow by their
    /// manifest `(load_order, id)`.
    pub order: Vec<String>,
}

/// The user configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    /// Where saves are read and written; `None` means the default location.
    pub save_dir: Option<PathBuf>,
    /// The language tag to use when `--language` is not given.
    pub language: Option<String>,
    /// The `[bindings]` table over the defaults; `None` when the file has
    /// none (the shipped defaults).
    pub bindings: Option<Bindings>,
    /// The `[graphics]` table over the defaults.
    pub graphics: Graphics,
    /// The `[mods]` enable/disable/order selection.
    pub mods: Mods,
    /// The `[games]` launcher bookkeeping.
    pub games: Games,
    /// The `[updates]` channel and last found build.
    pub updates: Updates,
    /// The `[game]` gameplay options.
    pub game: Game,
}

/// The update track the launcher follows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum UpdateChannel {
    #[default]
    Release,
    Nightly,
}

impl UpdateChannel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Release => "release",
            Self::Nightly => "nightly",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "release" => Some(Self::Release),
            "nightly" => Some(Self::Nightly),
            _ => None,
        }
    }
}

/// The `[updates]` table: channel, startup check and the last found build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Updates {
    pub channel: UpdateChannel,
    /// Check for a newer build on startup.
    pub check: bool,
    /// Unix seconds of the last completed check.
    pub last_check: u64,
    /// The channel of the stored available build.
    pub available_channel: Option<UpdateChannel>,
    pub available_version: Option<String>,
    pub available_sha: Option<String>,
}

impl Default for Updates {
    fn default() -> Self {
        Self {
            channel: UpdateChannel::Release,
            check: true,
            last_check: 0,
            available_channel: None,
            available_version: None,
            available_sha: None,
        }
    }
}

/// The `[games]` launcher bookkeeping: where each game's installation is.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Games {
    /// The last accepted Resident Evil installation, used to rebuild the
    /// pack without asking for the folder again.
    pub re1_source: Option<PathBuf>,
    /// The Steam build id recorded next to that source, when it came from
    /// Steam.
    pub re1_build: Option<String>,
}

/// A `[game]` option stored as one of a few quoted names.
pub trait Named: Copy + PartialEq + Sized + 'static {
    /// Every value, in menu order, with its name.
    const NAMES: &'static [(Self, &'static str)];

    fn name(self) -> &'static str {
        Self::NAMES
            .iter()
            .find(|(value, _)| *value == self)
            .map_or("", |(_, name)| name)
    }

    fn from_name(name: &str) -> Option<Self> {
        Self::NAMES
            .iter()
            .find(|(_, n)| *n == name)
            .map(|&(value, _)| value)
    }

    /// The value's position in [`Self::NAMES`].
    fn index(self) -> usize {
        Self::NAMES
            .iter()
            .position(|(value, _)| *value == self)
            .unwrap_or(0)
    }

    /// The value at `index` in [`Self::NAMES`].
    fn at(index: usize) -> Option<Self> {
        Self::NAMES.get(index).map(|&(value, _)| value)
    }
}

/// How strongly the controller rumbles.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Vibration {
    Off,
    Weak,
    #[default]
    Normal,
    Strong,
}

impl Named for Vibration {
    const NAMES: &'static [(Self, &'static str)] = &[
        (Vibration::Off, "off"),
        (Vibration::Weak, "weak"),
        (Vibration::Normal, "normal"),
        (Vibration::Strong, "strong"),
    ];
}

impl Vibration {
    /// The rumble motors' strength as a fraction of full.
    pub fn strength(self) -> f32 {
        match self {
            Vibration::Off => 0.0,
            Vibration::Weak => 0.4,
            Vibration::Normal => 0.7,
            Vibration::Strong => 1.0,
        }
    }
}

/// The gamepad layout preset (see
/// [`crate::controls::Bindings::with_control_type`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ControlType {
    #[default]
    A,
    B,
    C,
}

impl Named for ControlType {
    const NAMES: &'static [(Self, &'static str)] = &[
        (ControlType::A, "a"),
        (ControlType::B, "b"),
        (ControlType::C, "c"),
    ];
}

/// The size of the native-resolution menus' text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TextSize {
    Small,
    #[default]
    Standard,
    Large,
}

impl Named for TextSize {
    const NAMES: &'static [(Self, &'static str)] = &[
        (TextSize::Small, "small"),
        (TextSize::Standard, "standard"),
        (TextSize::Large, "large"),
    ];
}

impl TextSize {
    /// The factor on the theme's text sizes.
    pub fn scale(self) -> f32 {
        match self {
            TextSize::Small => 0.75,
            TextSize::Standard => 0.85,
            TextSize::Large => 1.0,
        }
    }
}

/// The size of the subtitles.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SubtitleSize {
    Small,
    #[default]
    Standard,
    Large,
}

impl Named for SubtitleSize {
    const NAMES: &'static [(Self, &'static str)] = &[
        (SubtitleSize::Small, "small"),
        (SubtitleSize::Standard, "standard"),
        (SubtitleSize::Large, "large"),
    ];
}

/// A subtitle or speaker name colour.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TextColor {
    #[default]
    White,
    Red,
    Blue,
    Green,
    Yellow,
}

impl Named for TextColor {
    const NAMES: &'static [(Self, &'static str)] = &[
        (TextColor::White, "white"),
        (TextColor::Red, "red"),
        (TextColor::Blue, "blue"),
        (TextColor::Green, "green"),
        (TextColor::Yellow, "yellow"),
    ];
}

/// The `[game]` gameplay options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Game {
    /// Classic REbirth's quick turn: a RUN press while walking backward
    /// turns the player 180 degrees in place over eight ticks. On by default.
    pub quick_turn: bool,
    /// Classic REbirth's tactical reload (`[DLL] TactReload`): with the aim
    /// held, holding RUN reloads the equipped gun instead of firing when the
    /// clip is not full and the matching ammunition item is held. On by
    /// default.
    pub tactical_reload: bool,
    /// Camera-relative movement on the left analog stick (the RE1 remake's
    /// alternate controls); the keyboard and the d-pad keep the tank
    /// controls. On by default.
    pub analog_movement: bool,
    /// Raising a weapon locks on to the nearest enemy, as the original does;
    /// off aims where the player faces. On by default.
    pub auto_aim: bool,
    /// The blood billboards, spurts and pools. On by default.
    pub blood: bool,
    /// Saving as Chris needs an ink ribbon, as in every release. On by
    /// default.
    pub chris_ink_ribbons: bool,
    /// Saving as Jill needs an ink ribbon, as in the USA release; the
    /// Japanese release lets her save without one. Off by default.
    pub jill_ink_ribbons: bool,
    /// A door's room load writes an autosave (`autosave1.dat`, rotating the
    /// previous one to `autosave2.dat`) once the player has control. On by
    /// default.
    pub autosave: bool,
    /// The room scripts run with the port's fixes (the `#else` of each
    /// `#if AUTHENTIC` block in `games/re1/room/*.akscr`, which
    /// `convert-game` packs as `room/<id>/room.scd`). Off runs each room's original
    /// bytecode instead; a mod's own scripts still apply. On by default.
    pub room_script_fixes: bool,
    /// The face the native-resolution menus draw with: `"modern"` (the
    /// default) or `"retro"`.
    pub ui_font: crate::gui::FontFamily,
    /// The size of the native-resolution menus' text.
    pub menu_text_size: TextSize,
    /// The save/load screen: the modern full-screen list of every save (the
    /// default), or the original's eight slots. `--authentic` runs and a
    /// mod's own screen keep theirs.
    pub modern_save_screen: bool,
    /// The controller glyphs the menus draw; `None` (`"auto"`, the default)
    /// follows the controller in use.
    pub button_style: Option<crate::gui::PadStyle>,
    /// The gamepad layout preset last picked in the menu. Picking one
    /// rewrites the pad sources in `[bindings]`; the bindings, not this,
    /// drive the input, so pad rebinds made since are kept.
    pub control_type: ControlType,
    /// How strongly the controller rumbles.
    pub vibration: Vibration,
    /// The level of everything, 0..=100 percent, on top of the others.
    pub master_volume: u8,
    /// The music's level, 0..=100 percent.
    pub music_volume: u8,
    /// The ambient loops' level (rain, machinery: the room tracks that are
    /// sounds rather than music), 0..=100 percent.
    pub ambient_volume: u8,
    /// The sound effects' level, 0..=100 percent.
    pub effects_volume: u8,
    /// The voices' level, 0..=100 percent.
    pub voice_volume: u8,
    /// The menus' cursor, confirm and cancel sounds, 0..=100 percent.
    pub menu_volume: u8,
    /// Show subtitles. Stored only: subtitles are not drawn yet.
    pub subtitles: bool,
    pub subtitle_size: SubtitleSize,
    /// The opacity of the band behind the subtitles, 0..=100 percent.
    pub subtitle_background: u8,
    pub subtitle_color: TextColor,
    /// Show the speaker's name with a subtitle.
    pub speaker_names: bool,
    pub speaker_name_color: TextColor,
}

impl Default for Game {
    fn default() -> Self {
        Self {
            quick_turn: true,
            tactical_reload: true,
            analog_movement: true,
            auto_aim: true,
            blood: true,
            chris_ink_ribbons: true,
            jill_ink_ribbons: false,
            autosave: true,
            room_script_fixes: true,
            ui_font: crate::gui::FontFamily::Modern,
            menu_text_size: TextSize::Standard,
            modern_save_screen: true,
            button_style: None,
            control_type: ControlType::A,
            vibration: Vibration::Normal,
            master_volume: 100,
            music_volume: 100,
            ambient_volume: 100,
            effects_volume: 100,
            voice_volume: 100,
            menu_volume: 100,
            subtitles: true,
            subtitle_size: SubtitleSize::Standard,
            subtitle_background: 50,
            subtitle_color: TextColor::White,
            speaker_names: false,
            speaker_name_color: TextColor::Yellow,
        }
    }
}

impl Game {
    /// The volume settings as mixer gains.
    pub fn volumes(&self) -> crate::audio::Volumes {
        crate::audio::Volumes {
            master: crate::audio::percent_gain(self.master_volume),
            music: crate::audio::percent_gain(self.music_volume),
            ambient: crate::audio::percent_gain(self.ambient_volume),
            effects: crate::audio::percent_gain(self.effects_volume),
            voice: crate::audio::percent_gain(self.voice_volume),
            menu: crate::audio::percent_gain(self.menu_volume),
        }
    }
}

/// The table the parser is in.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Table {
    Root,
    Bindings,
    Graphics,
    Mods,
    Games,
    Updates,
    Game,
    /// A table that could not be used; its keys are skipped.
    Ignored,
}

impl Config {
    /// The configuration directory: [`crate::paths::config_dir`], which is
    /// `$ARKLAY_CONFIG_DIR` when set, else an existing local `arklay.toml`,
    /// else the platform's roaming configuration directory.
    pub fn dir() -> PathBuf {
        crate::paths::config_dir()
    }

    /// The configuration file's path inside [`Config::dir`].
    pub fn default_path() -> PathBuf {
        Self::dir().join(FILE_NAME)
    }

    /// The default save directory: `<config dir>/save`.
    pub fn default_save_dir() -> PathBuf {
        Self::dir().join(SAVE_DIR_NAME)
    }

    /// The per-user mods directory: `<config dir>/mods`.
    pub fn default_mods_dir() -> PathBuf {
        Self::dir().join(MODS_DIR_NAME)
    }

    /// The extra mod directories scanned for every game, after the pack's
    /// own sibling `mods/` directory: each data root's `mods/`, then the
    /// legacy [`Config::default_mods_dir`]. The launcher and a command-line
    /// run both discover through this list.
    pub fn mod_dirs() -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = crate::paths::data_roots()
            .iter()
            .map(|root| crate::paths::mods_dir(root))
            .collect();
        let legacy = Self::default_mods_dir();
        if !dirs.contains(&legacy) {
            dirs.push(legacy);
        }
        dirs
    }

    /// Read the configuration file, never failing: a missing file is the
    /// default configuration, and anything wrong with it (an unreadable file,
    /// a bad line or value) is a warning while that part keeps its default.
    pub fn load(path: &Path) -> (Self, Vec<String>) {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return (Self::default(), Vec::new());
            }
            Err(err) => {
                let warning = format!("{}: {err}; using the defaults", path.display());
                return (Self::default(), vec![warning]);
            }
        };
        let (mut config, warnings) = Self::parse_lenient(&text);
        config.save_dir = config.save_dir.map(|dir| expand_tilde(&dir));
        config.games.re1_source = config.games.re1_source.map(|dir| expand_tilde(&dir));
        let warnings = warnings
            .into_iter()
            .map(|warning| format!("{}: {warning}", path.display()))
            .collect();
        (config, warnings)
    }

    /// Read the default configuration file ([`Config::load`]).
    pub fn load_default() -> (Self, Vec<String>) {
        Self::load(&Self::default_path())
    }

    /// Parse the file text strictly: any problem [`Config::parse_lenient`]
    /// would warn about is an error. Saving uses this, so a file with a
    /// mistake in it is never rewritten (and the mistake lost).
    pub fn parse(text: &str) -> Result<Self> {
        let (config, warnings) = Self::parse_lenient(text);
        if !warnings.is_empty() {
            bail!("{}", warnings.join("; "));
        }
        Ok(config)
    }

    /// Parse the file text, skipping what it cannot use: an unknown key or
    /// table, a malformed line or an out-of-range value is reported in the
    /// returned warnings and leaves that setting at its default, so a typo is
    /// reported without stopping the game.
    pub fn parse_lenient(text: &str) -> (Self, Vec<String>) {
        let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
        if let Err(err) = budget::check_len(text.len(), budget::MAX_TEXT_BYTES, "config file size")
        {
            return (
                Self::default(),
                vec![format!("{err:#}; using the defaults")],
            );
        }
        let mut config = Self::default();
        let mut warnings = Vec::new();
        let mut table = Table::Root;
        let mut seen_graphics = false;
        let mut seen_mods = false;
        let mut seen_games = false;
        let mut seen_updates = false;
        let mut seen_game = false;
        let mut bound = Vec::new();
        let mut graphics_keys: Vec<String> = Vec::new();
        let mut mod_keys: Vec<String> = Vec::new();
        let mut games_keys: Vec<String> = Vec::new();
        let mut update_keys: Vec<String> = Vec::new();
        let mut game_keys: Vec<String> = Vec::new();
        for (index, raw) in text.lines().enumerate() {
            let line = index + 1;
            let content = strip_comment(raw).trim();
            if content.is_empty() {
                continue;
            }
            if let Some(header) = content.strip_prefix('[') {
                table = match header.strip_suffix(']').map(str::trim) {
                    Some("bindings") if config.bindings.is_none() => {
                        config.bindings = Some(Bindings::default());
                        Table::Bindings
                    }
                    Some("graphics") if !seen_graphics => {
                        seen_graphics = true;
                        Table::Graphics
                    }
                    Some("mods") if !seen_mods => {
                        seen_mods = true;
                        Table::Mods
                    }
                    Some("games") if !seen_games => {
                        seen_games = true;
                        Table::Games
                    }
                    Some("updates") if !seen_updates => {
                        seen_updates = true;
                        Table::Updates
                    }
                    Some("game") if !seen_game => {
                        seen_game = true;
                        Table::Game
                    }
                    Some(
                        name @ ("bindings" | "graphics" | "mods" | "games" | "updates" | "game"),
                    ) => {
                        warnings.push(format!("line {line}: duplicate [{name}] table ignored"));
                        Table::Ignored
                    }
                    Some(other) => {
                        warnings.push(format!("line {line}: unknown table `[{other}]` ignored"));
                        Table::Ignored
                    }
                    None => {
                        warnings.push(format!("line {line}: unterminated table header"));
                        Table::Ignored
                    }
                };
                continue;
            }
            let parsed = match table {
                Table::Ignored => Ok(()),
                Table::Root => parse_root_key(&mut config, content, line),
                Table::Graphics => {
                    parse_graphics_line(&mut config.graphics, &mut graphics_keys, content, line)
                }
                Table::Mods => parse_mods_line(&mut config.mods, &mut mod_keys, content, line),
                Table::Games => parse_games_line(&mut config.games, &mut games_keys, content, line),
                Table::Updates => {
                    parse_updates_line(&mut config.updates, &mut update_keys, content, line)
                }
                Table::Game => parse_game_line(&mut config.game, &mut game_keys, content, line),
                Table::Bindings => parse_binding_line(&mut config, &mut bound, content, line),
            };
            if let Err(err) = parsed {
                warnings.push(format!("{err:#}"));
            }
        }
        if let Some(action) = config.bindings.as_ref().and_then(Bindings::lockout) {
            warnings.push(format!(
                "[bindings]: `{}` needs a keyboard key bound to none of up, down, left, right, \
                 confirm, cancel and start; using the default bindings",
                action.name()
            ));
            config.bindings = None;
        }
        (config, warnings)
    }

    /// Render the canonical file text (the documented keys, defaults omitted:
    /// the `[bindings]` table lists only the actions that differ from the
    /// shipped layout, so the rest keep following it).
    pub fn render(&self) -> String {
        let mut out = String::new();
        if let Some(dir) = &self.save_dir {
            out.push_str(&format!("save_dir = {}\n", quote(&dir.to_string_lossy())));
        }
        if let Some(language) = &self.language {
            out.push_str(&format!("language = {}\n", quote(language)));
        }
        let mods = render_mods(&self.mods);
        if !mods.is_empty() {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str("[mods]\n");
            out.push_str(&mods);
        }
        let games = render_games(&self.games);
        if !games.is_empty() {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str("[games]\n");
            out.push_str(&games);
        }
        let updates = render_updates(&self.updates);
        if !updates.is_empty() {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str("[updates]\n");
            out.push_str(&updates);
        }
        if let Some(bindings) = &self.bindings {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str("[bindings]\n");
            let defaults = Bindings::default();
            for action in ACTIONS {
                if bindings.get(action) == defaults.get(action) {
                    continue;
                }
                let names: Vec<String> = bindings
                    .get(action)
                    .iter()
                    .map(|source| quote(&source.to_string()))
                    .collect();
                out.push_str(&format!("{} = [{}]\n", action.name(), names.join(", ")));
            }
        }
        let game = render_game(&self.game);
        if !game.is_empty() {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str("[game]\n");
            out.push_str(&game);
        }
        let graphics = render_graphics(&self.graphics);
        if !graphics.is_empty() {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str("[graphics]\n");
            out.push_str(&graphics);
        }
        out
    }

    /// The configured bindings, or the shipped defaults.
    pub fn bindings_or_default(&self) -> Bindings {
        self.bindings.clone().unwrap_or_default()
    }

    /// Persist `bindings` into the file at `path`, keeping its other keys.
    ///
    /// Bindings equal to the defaults drop the table, so a reset leaves a
    /// fresh file. The file is rewritten in its canonical form (comments are
    /// not kept; `save_dir` stays as written, `~` included). A file that does
    /// not parse is left alone and reported.
    pub fn save_bindings(path: &Path, bindings: &Bindings) -> Result<()> {
        Self::update(path, |config| {
            config.bindings = (*bindings != Bindings::default()).then(|| bindings.clone());
        })
    }

    /// Persist the `[mods]` enable/disable/order selection.
    pub fn save_mods(path: &Path, mods: &Mods) -> Result<()> {
        Self::update(path, |config| config.mods = mods.clone())
    }

    /// Persist the update channel, check settings and last found build.
    pub fn save_updates(path: &Path, updates: &Updates) -> Result<()> {
        Self::update(path, |config| config.updates = updates.clone())
    }

    /// Persist the launcher's remembered Resident Evil source and Steam build
    /// id, keeping the file's other keys. A `None` source removes the entry.
    pub fn save_games(path: &Path, source: Option<&Path>, build: Option<&str>) -> Result<()> {
        Self::update(path, |config| {
            config.games.re1_source = source.map(Path::to_path_buf);
            config.games.re1_build = build.map(str::to_string);
        })
    }

    /// Persist the window keys of `graphics` (`fullscreen`, `window_width`,
    /// `window_height` and `display`) into the file at `path`, keeping its other keys. The
    /// renderer keys stay as the file has them, since the command line may
    /// have overridden them for this run. Rewritten like
    /// [`Config::save_bindings`].
    pub fn save_window(path: &Path, graphics: &Graphics) -> Result<()> {
        Self::update(path, |config| {
            config.graphics.fullscreen = graphics.fullscreen;
            config.graphics.window_width = graphics.window_width;
            config.graphics.window_height = graphics.window_height;
            config.graphics.display = graphics.display;
        })
    }

    /// Persist `[graphics] frame_rate` into the file at `path`, keeping its
    /// other keys. Rewritten like [`Config::save_bindings`].
    pub fn save_frame_rate(path: &Path, frame_rate: u32) -> Result<()> {
        Self::update(path, |config| config.graphics.frame_rate = frame_rate)
    }

    /// Persist `[graphics] aspect` into the file at `path`, keeping its other
    /// keys. Rewritten like [`Config::save_bindings`].
    pub fn save_aspect(path: &Path, aspect: video::Aspect) -> Result<()> {
        Self::update(path, |config| config.graphics.aspect = aspect)
    }

    /// Persist the launcher's options: the whole `[graphics]` table, the
    /// root `language` key and the `[game]` toggles. Rewritten like
    /// [`Config::save_bindings`].
    pub fn save_options(
        path: &Path,
        graphics: &Graphics,
        language: Option<&str>,
        game: Game,
    ) -> Result<()> {
        Self::update(path, |config| {
            config.graphics = *graphics;
            config.language = language.map(str::to_string);
            config.game = game;
        })
    }

    /// Read the file at `path` (a missing one is empty), apply `change` and
    /// write it back in its canonical form. A file that does not parse is
    /// left alone and reported.
    fn update(path: &Path, change: impl FnOnce(&mut Self)) -> Result<()> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(err) => {
                return Err(err).with_context(|| format!("failed to read {}", path.display()));
            }
        };
        let mut config =
            Self::parse(&text).with_context(|| format!("failed to parse {}", path.display()))?;
        change(&mut config);
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)
                .with_context(|| format!("failed to create {}", dir.display()))?;
        }
        crate::atomic::write(path, config.render().as_bytes())
    }
}

/// Split a `key = value` line.
fn key_value(content: &str, line: usize) -> Result<(&str, &str)> {
    let Some((key, value)) = content.split_once('=') else {
        bail!("line {line}: expected `key = \"value\"`");
    };
    Ok((key.trim(), value.trim()))
}

/// Parse one top-level `key = "value"` line into `config`.
fn parse_root_key(config: &mut Config, content: &str, line: usize) -> Result<()> {
    let (key, value) = key_value(content, line)?;
    if !value.starts_with(['"', '\'']) {
        bail!("line {line}: `{key}` expects a quoted string");
    }
    let value = parse_quoted(value, line)?;
    match key {
        "save_dir" => {
            if value.is_empty() {
                bail!("line {line}: save_dir is empty");
            }
            config.save_dir = Some(PathBuf::from(value));
        }
        "language" => {
            if value.is_empty() {
                bail!("line {line}: language is empty");
            }
            config.language = Some(value);
        }
        other => bail!("line {line}: unknown key `{other}`"),
    }
    Ok(())
}

/// Parse one `[bindings]` line into `config`; a bad line leaves the action
/// at its default.
fn parse_binding_line(
    config: &mut Config,
    bound: &mut Vec<Action>,
    content: &str,
    line: usize,
) -> Result<()> {
    let (key, value) = key_value(content, line)?;
    let Some(action) = Action::from_name(key) else {
        bail!("line {line}: unknown action `{key}`");
    };
    if bound.contains(&action) {
        bail!("line {line}: duplicate action `{key}`");
    }
    bound.push(action);
    let mut sources = Vec::new();
    for name in parse_string_array(value, line)? {
        // An input reserved since the file was written is dropped, not an
        // error, so the rest of the file still loads and saves.
        let source = Source::parse_any(&name)
            .with_context(|| format!("line {line}: bad binding for `{key}`"))?;
        if source.reserved_bit().is_none() {
            sources.push(source);
        }
    }
    if let Some(bindings) = config.bindings.as_mut() {
        bindings.set(action, sources);
    }
    Ok(())
}

/// Parse one `[mods]` line into `mods`.
fn parse_mods_line(
    mods: &mut Mods,
    seen: &mut Vec<String>,
    content: &str,
    line: usize,
) -> Result<()> {
    let (key, value) = key_value(content, line)?;
    if seen.iter().any(|seen| seen == key) {
        bail!("line {line}: duplicate key `{key}`");
    }
    seen.push(key.to_string());
    let ids = parse_string_array(value, line)?;
    if ids.iter().any(String::is_empty) {
        bail!("line {line}: `{key}` contains an empty mod id");
    }
    match key {
        "enabled" => mods.enabled = Some(ids),
        "disabled" => mods.disabled = ids,
        "order" => mods.order = ids,
        other => bail!("line {line}: unknown key `{other}`"),
    }
    Ok(())
}

/// Parse one `[graphics]` line into `graphics`.
fn parse_graphics_line(
    graphics: &mut Graphics,
    seen: &mut Vec<String>,
    content: &str,
    line: usize,
) -> Result<()> {
    let (key, value) = key_value(content, line)?;
    if seen.iter().any(|seen| seen == key) {
        bail!("line {line}: duplicate key `{key}`");
    }
    seen.push(key.to_string());
    parse_graphics_key(graphics, key, value, line)
}

/// Parse one `[graphics]` key into `graphics`.
fn parse_graphics_key(graphics: &mut Graphics, key: &str, value: &str, line: usize) -> Result<()> {
    let integer = |range: std::ops::RangeInclusive<u32>| -> Result<u32> {
        let Ok(number) = value.parse::<u32>() else {
            bail!("line {line}: `{key}` expects an integer");
        };
        if !range.contains(&number) {
            bail!(
                "line {line}: `{key}` must be {}..={}",
                range.start(),
                range.end()
            );
        }
        Ok(number)
    };
    match key {
        "renderer" => {
            let name = parse_quoted(value, line)?;
            let Some(renderer) = RendererKind::from_name(&name) else {
                bail!("line {line}: renderer must be \"software\" or \"gpu\"");
            };
            graphics.renderer = renderer;
        }
        "fullscreen" => {
            graphics.fullscreen = match value {
                "true" => true,
                "false" => false,
                _ => bail!("line {line}: `fullscreen` expects true or false"),
            };
        }
        "vsync" => {
            graphics.vsync = match value {
                "true" => true,
                "false" => false,
                _ => bail!("line {line}: `vsync` expects true or false"),
            };
        }
        "window_width" => graphics.window_width = integer(1..=video::MAX_WINDOW_SIDE)?,
        "window_height" => graphics.window_height = integer(1..=video::MAX_WINDOW_SIDE)?,
        "display" => graphics.display = integer(1..=video::MAX_DISPLAY)?,
        "render_scale" => graphics.render_scale = integer(video::RENDER_SCALE_RANGE)?,
        "msaa" => {
            let samples = integer(1..=8)?;
            if !video::MSAA_COUNTS.contains(&samples) {
                bail!("line {line}: `msaa` must be 1, 2, 4 or 8");
            }
            graphics.msaa = samples;
        }
        "frame_rate" => {
            graphics.frame_rate = integer(video::MIN_FRAME_RATE..=video::MAX_FRAME_RATE)?
        }
        "aspect" => {
            let name = parse_quoted(value, line)?;
            graphics.aspect = video::Aspect::parse(&name)
                .with_context(|| format!("line {line}: invalid `aspect`"))?;
        }
        "hdr" => {
            graphics.hdr = match value {
                "true" => true,
                "false" => false,
                _ => bail!("line {line}: `hdr` expects true or false"),
            };
        }
        other => bail!("line {line}: unknown key `{other}`"),
    }
    Ok(())
}

/// The `[mods]` keys that are set; an absent `enabled` is not written.
fn render_mods(mods: &Mods) -> String {
    let mut out = String::new();
    if let Some(enabled) = &mods.enabled {
        let ids: Vec<String> = enabled.iter().map(|id| quote(id)).collect();
        out.push_str(&format!("enabled = [{}]\n", ids.join(", ")));
    }
    for (key, ids) in [("disabled", &mods.disabled), ("order", &mods.order)] {
        if !ids.is_empty() {
            let ids: Vec<String> = ids.iter().map(|id| quote(id)).collect();
            out.push_str(&format!("{key} = [{}]\n", ids.join(", ")));
        }
    }
    out
}

/// Parse one `[games]` line into `games`.
fn parse_games_line(
    games: &mut Games,
    seen: &mut Vec<String>,
    content: &str,
    line: usize,
) -> Result<()> {
    let (key, value) = key_value(content, line)?;
    if seen.iter().any(|seen| seen == key) {
        bail!("line {line}: duplicate key `{key}`");
    }
    seen.push(key.to_string());
    if !value.starts_with(['"', '\'']) {
        bail!("line {line}: `{key}` expects a quoted string");
    }
    let value = parse_quoted(value, line)?;
    match key {
        "re1_source" => {
            if value.is_empty() {
                bail!("line {line}: re1_source is empty");
            }
            games.re1_source = Some(PathBuf::from(value));
        }
        "re1_build" => {
            if value.is_empty() {
                bail!("line {line}: re1_build is empty");
            }
            games.re1_build = Some(value);
        }
        other => bail!("line {line}: unknown key `{other}`"),
    }
    Ok(())
}

/// The `[games]` keys that are set.
fn render_games(games: &Games) -> String {
    let mut out = String::new();
    if let Some(source) = &games.re1_source {
        out.push_str(&format!(
            "re1_source = {}\n",
            quote(&source.to_string_lossy())
        ));
    }
    if let Some(build) = &games.re1_build {
        out.push_str(&format!("re1_build = {}\n", quote(build)));
    }
    out
}

/// Parse one `[updates]` line into `updates`.
fn parse_updates_line(
    updates: &mut Updates,
    seen: &mut Vec<String>,
    content: &str,
    line: usize,
) -> Result<()> {
    let (key, value) = key_value(content, line)?;
    if seen.iter().any(|seen| seen == key) {
        bail!("line {line}: duplicate key `{key}`");
    }
    seen.push(key.to_string());
    let quoted = || -> Result<String> {
        if !value.starts_with(['"', '\'']) {
            bail!("line {line}: `{key}` expects a quoted string");
        }
        parse_quoted(value, line)
    };
    match key {
        "channel" => {
            let text = quoted()?;
            updates.channel = UpdateChannel::parse(&text)
                .with_context(|| format!("line {line}: channel must be release or nightly"))?;
        }
        "check" => {
            updates.check = match value {
                "true" => true,
                "false" => false,
                _ => bail!("line {line}: `check` expects true or false"),
            };
        }
        "last_check" => {
            updates.last_check = value
                .parse::<u64>()
                .map_err(|_| anyhow!("line {line}: bad last_check {value}"))?;
        }
        "available_channel" => {
            let text = quoted()?;
            updates.available_channel = Some(
                UpdateChannel::parse(&text)
                    .with_context(|| format!("line {line}: bad available_channel"))?,
            );
        }
        "available_version" => updates.available_version = Some(quoted()?),
        "available_sha" => updates.available_sha = Some(quoted()?),
        other => bail!("line {line}: unknown key `{other}`"),
    }
    Ok(())
}

/// The `[updates]` keys that differ from the defaults.
fn render_updates(updates: &Updates) -> String {
    let defaults = Updates::default();
    let mut out = String::new();
    if updates.channel != defaults.channel {
        out.push_str(&format!("channel = {}\n", quote(updates.channel.as_str())));
    }
    if updates.check != defaults.check {
        out.push_str(&format!("check = {}\n", updates.check));
    }
    if updates.last_check != defaults.last_check {
        out.push_str(&format!("last_check = {}\n", updates.last_check));
    }
    if let Some(channel) = updates.available_channel {
        out.push_str(&format!(
            "available_channel = {}\n",
            quote(channel.as_str())
        ));
    }
    if let Some(version) = &updates.available_version {
        out.push_str(&format!("available_version = {}\n", quote(version)));
    }
    if let Some(sha) = &updates.available_sha {
        out.push_str(&format!("available_sha = {}\n", quote(sha)));
    }
    out
}

/// Parse one `[game]` line into `game`; a bad line leaves its option at the
/// default.
fn parse_game_line(
    game: &mut Game,
    seen: &mut Vec<String>,
    content: &str,
    line: usize,
) -> Result<()> {
    let (key, value) = key_value(content, line)?;
    if seen.iter().any(|seen| seen == key) {
        bail!("line {line}: duplicate key `{key}`");
    }
    seen.push(key.to_string());
    let boolean = || -> Result<bool> {
        match value {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => bail!("line {line}: `{key}` expects true or false"),
        }
    };
    let percent = || -> Result<u8> {
        value
            .parse::<u8>()
            .ok()
            .filter(|volume| *volume <= 100)
            .ok_or_else(|| anyhow::anyhow!("line {line}: `{key}` expects 0 to 100"))
    };
    fn named<T: Named>(key: &str, value: &str, line: usize) -> Result<T> {
        let name = parse_quoted(value, line)?;
        T::from_name(&name).ok_or_else(|| {
            let names: Vec<String> = T::NAMES.iter().map(|(_, n)| format!("\"{n}\"")).collect();
            anyhow::anyhow!("line {line}: `{key}` expects {}", names.join(", "))
        })
    }
    match key {
        "quick_turn" => game.quick_turn = boolean()?,
        "tactical_reload" => game.tactical_reload = boolean()?,
        "analog_movement" => game.analog_movement = boolean()?,
        "auto_aim" => game.auto_aim = boolean()?,
        "blood" => game.blood = boolean()?,
        "chris_ink_ribbons" => game.chris_ink_ribbons = boolean()?,
        "jill_ink_ribbons" => game.jill_ink_ribbons = boolean()?,
        "autosave" => game.autosave = boolean()?,
        "room_script_fixes" => game.room_script_fixes = boolean()?,
        "modern_save_screen" => game.modern_save_screen = boolean()?,
        "ui_font" => {
            game.ui_font = match parse_quoted(value, line)?.as_str() {
                "modern" => crate::gui::FontFamily::Modern,
                "retro" => crate::gui::FontFamily::Retro,
                _ => bail!("line {line}: `ui_font` expects \"modern\" or \"retro\""),
            };
        }
        "menu_text_size" => {
            // The largest size was once "extra_large".
            game.menu_text_size = match parse_quoted(value, line)?.as_str() {
                "extra_large" => TextSize::Large,
                _ => named(key, value, line)?,
            };
        }
        // The on/off switch the menu volume replaced: off is silence.
        "menu_sounds" => {
            if !boolean()? {
                game.menu_volume = 0;
            }
        }
        "button_style" => {
            game.button_style = match parse_quoted(value, line)?.as_str() {
                "auto" => None,
                "xbox" => Some(crate::gui::PadStyle::Xbox),
                "playstation" => Some(crate::gui::PadStyle::PlayStation),
                "nintendo" => Some(crate::gui::PadStyle::Nintendo),
                _ => bail!(
                    "line {line}: `button_style` expects \"auto\", \"xbox\", \"playstation\" or \"nintendo\""
                ),
            };
        }
        "control_type" => game.control_type = named(key, value, line)?,
        "vibration" => game.vibration = named(key, value, line)?,
        "master_volume" => game.master_volume = percent()?,
        "music_volume" => game.music_volume = percent()?,
        "ambient_volume" => game.ambient_volume = percent()?,
        "effects_volume" => game.effects_volume = percent()?,
        "voice_volume" => game.voice_volume = percent()?,
        "menu_volume" => game.menu_volume = percent()?,
        "subtitles" => game.subtitles = boolean()?,
        "subtitle_size" => game.subtitle_size = named(key, value, line)?,
        "subtitle_background" => game.subtitle_background = percent()?,
        "subtitle_color" => game.subtitle_color = named(key, value, line)?,
        "speaker_names" => game.speaker_names = boolean()?,
        "speaker_name_color" => game.speaker_name_color = named(key, value, line)?,
        other => bail!("line {line}: unknown key `{other}`"),
    }
    Ok(())
}

/// The `[game]` keys that differ from the defaults.
fn render_game(game: &Game) -> String {
    let defaults = Game::default();
    let mut out = String::new();
    for (key, value, default) in [
        ("quick_turn", game.quick_turn, defaults.quick_turn),
        (
            "tactical_reload",
            game.tactical_reload,
            defaults.tactical_reload,
        ),
        (
            "analog_movement",
            game.analog_movement,
            defaults.analog_movement,
        ),
        ("auto_aim", game.auto_aim, defaults.auto_aim),
        ("blood", game.blood, defaults.blood),
        (
            "chris_ink_ribbons",
            game.chris_ink_ribbons,
            defaults.chris_ink_ribbons,
        ),
        (
            "jill_ink_ribbons",
            game.jill_ink_ribbons,
            defaults.jill_ink_ribbons,
        ),
        ("autosave", game.autosave, defaults.autosave),
        (
            "room_script_fixes",
            game.room_script_fixes,
            defaults.room_script_fixes,
        ),
        (
            "modern_save_screen",
            game.modern_save_screen,
            defaults.modern_save_screen,
        ),
    ] {
        if value != default {
            out.push_str(&format!("{key} = {value}\n"));
        }
    }
    if game.ui_font != defaults.ui_font {
        let name = match game.ui_font {
            crate::gui::FontFamily::Modern => "modern",
            crate::gui::FontFamily::Retro => "retro",
        };
        out.push_str(&format!("ui_font = \"{name}\"\n"));
    }
    if game.menu_text_size != defaults.menu_text_size {
        out.push_str(&format!(
            "menu_text_size = \"{}\"\n",
            game.menu_text_size.name()
        ));
    }
    if game.button_style != defaults.button_style {
        let name = match game.button_style {
            None => "auto",
            Some(crate::gui::PadStyle::Xbox) => "xbox",
            Some(crate::gui::PadStyle::PlayStation) => "playstation",
            Some(crate::gui::PadStyle::Nintendo) => "nintendo",
        };
        out.push_str(&format!("button_style = \"{name}\"\n"));
    }
    if game.control_type != defaults.control_type {
        out.push_str(&format!(
            "control_type = \"{}\"\n",
            game.control_type.name()
        ));
    }
    if game.vibration != defaults.vibration {
        out.push_str(&format!("vibration = \"{}\"\n", game.vibration.name()));
    }
    for (key, volume, default) in [
        ("master_volume", game.master_volume, defaults.master_volume),
        ("music_volume", game.music_volume, defaults.music_volume),
        (
            "ambient_volume",
            game.ambient_volume,
            defaults.ambient_volume,
        ),
        (
            "effects_volume",
            game.effects_volume,
            defaults.effects_volume,
        ),
        ("voice_volume", game.voice_volume, defaults.voice_volume),
        ("menu_volume", game.menu_volume, defaults.menu_volume),
    ] {
        if volume != default {
            out.push_str(&format!("{key} = {volume}\n"));
        }
    }
    if game.subtitles != defaults.subtitles {
        out.push_str(&format!("subtitles = {}\n", game.subtitles));
    }
    if game.subtitle_size != defaults.subtitle_size {
        out.push_str(&format!(
            "subtitle_size = \"{}\"\n",
            game.subtitle_size.name()
        ));
    }
    if game.subtitle_background != defaults.subtitle_background {
        out.push_str(&format!(
            "subtitle_background = {}\n",
            game.subtitle_background
        ));
    }
    if game.subtitle_color != defaults.subtitle_color {
        out.push_str(&format!(
            "subtitle_color = \"{}\"\n",
            game.subtitle_color.name()
        ));
    }
    if game.speaker_names != defaults.speaker_names {
        out.push_str(&format!("speaker_names = {}\n", game.speaker_names));
    }
    if game.speaker_name_color != defaults.speaker_name_color {
        out.push_str(&format!(
            "speaker_name_color = \"{}\"\n",
            game.speaker_name_color.name()
        ));
    }
    out
}

/// The `[graphics]` keys that differ from the defaults.
fn render_graphics(graphics: &Graphics) -> String {
    let defaults = Graphics::default();
    let mut out = String::new();
    if graphics.renderer != defaults.renderer {
        out.push_str(&format!("renderer = {}\n", quote(graphics.renderer.name())));
    }
    if graphics.fullscreen != defaults.fullscreen {
        out.push_str(&format!("fullscreen = {}\n", graphics.fullscreen));
    }
    if graphics.vsync != defaults.vsync {
        out.push_str(&format!("vsync = {}\n", graphics.vsync));
    }
    if graphics.aspect != defaults.aspect {
        out.push_str(&format!("aspect = {}\n", quote(&graphics.aspect.name())));
    }
    if graphics.hdr != defaults.hdr {
        out.push_str(&format!("hdr = {}\n", graphics.hdr));
    }
    for (key, value, default) in [
        ("window_width", graphics.window_width, defaults.window_width),
        (
            "window_height",
            graphics.window_height,
            defaults.window_height,
        ),
        ("display", graphics.display, defaults.display),
        ("render_scale", graphics.render_scale, defaults.render_scale),
        ("msaa", graphics.msaa, defaults.msaa),
        ("frame_rate", graphics.frame_rate, defaults.frame_rate),
    ] {
        if value != default {
            out.push_str(&format!("{key} = {value}\n"));
        }
    }
    out
}

/// A basic `"..."` string with the escapes [`parse_quoted`] reads back.
fn quote(text: &str) -> String {
    let mut out = String::from('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

/// Parse a one-line array of quoted strings (`["a", 'b']`), or a lone quoted
/// string as a one-element array.
fn parse_string_array(value: &str, line: usize) -> Result<Vec<String>> {
    if value.starts_with(['"', '\'']) {
        return Ok(vec![parse_quoted(value, line)?]);
    }
    let Some(inner) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) else {
        bail!("line {line}: expected an array of quoted strings");
    };
    let mut items = Vec::new();
    let mut rest = inner.trim();
    while !rest.is_empty() {
        let end = quoted_end(rest)
            .ok_or_else(|| anyhow::anyhow!("line {line}: expected a quoted string in the array"))?;
        items.push(parse_quoted(&rest[..end], line)?);
        rest = rest[end..].trim_start();
        match rest.strip_prefix(',') {
            Some(after) => rest = after.trim_start(),
            None if rest.is_empty() => {}
            None => bail!("line {line}: expected `,` between array items"),
        }
    }
    Ok(items)
}

/// The byte length of the quoted string `text` starts with, quotes included.
fn quoted_end(text: &str) -> Option<usize> {
    let quote = text.chars().next().filter(|c| matches!(c, '"' | '\''))?;
    let mut escaped = false;
    for (index, ch) in text.char_indices().skip(1) {
        if escaped {
            escaped = false;
        } else if quote == '"' && ch == '\\' {
            escaped = true;
        } else if ch == quote {
            return Some(index + 1);
        }
    }
    None
}

/// The home directory: `$HOME`, else `%USERPROFILE%`.
pub fn home_dir() -> Option<PathBuf> {
    env_path("HOME").or_else(|| env_path("USERPROFILE"))
}

/// Expand a leading `~/` against the home directory.
pub fn expand_tilde(path: &Path) -> PathBuf {
    match home_dir() {
        Some(home) => expand_tilde_with(path, &home),
        None => path.to_path_buf(),
    }
}

/// [`expand_tilde`] against an explicit home directory.
pub fn expand_tilde_with(path: &Path, home: &Path) -> PathBuf {
    let Some(rest) = path.to_str().and_then(|text| text.strip_prefix("~/")) else {
        return path.to_path_buf();
    };
    home.join(rest)
}

/// A non-empty environment variable as a path.
fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// Drop a trailing `#` comment, honouring quoted strings: a `#` inside a value
/// is not a comment, and an escape only applies inside a basic `"..."` string.
fn strip_comment(line: &str) -> &str {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (index, ch) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match quote {
            Some('"') => match ch {
                '\\' => escaped = true,
                '"' => quote = None,
                _ => {}
            },
            Some(_) => {
                if ch == '\'' {
                    quote = None;
                }
            }
            None => match ch {
                '"' | '\'' => quote = Some(ch),
                '#' => return &line[..index],
                _ => {}
            },
        }
    }
    line
}

/// Parse one quoted string: `"..."` with the manifest's escapes, or `'...'`
/// verbatim (TOML's literal string, so a Windows path needs no escaping).
fn parse_quoted(value: &str, line: usize) -> Result<String> {
    let Some(quote @ ('"' | '\'')) = value.chars().next() else {
        bail!("line {line}: expected a quoted string");
    };
    if !value.ends_with(quote) || value.len() < 2 {
        bail!("line {line}: unterminated string");
    }
    let inner = &value[1..value.len() - 1];
    if quote == '\'' {
        return Ok(inner.to_string());
    }
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some(other) => bail!("line {line}: unknown escape `\\{other}`"),
            None => bail!("line {line}: unterminated escape"),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_is_the_default_config() {
        let dir =
            std::env::temp_dir().join(format!("arklay-config-{}-missing", std::process::id()));
        let path = dir.join("arklay.toml");
        assert_eq!(Config::load(&path), (Config::default(), Vec::new()));
    }

    #[test]
    fn parses_every_key() {
        let config = Config::parse(
            "# a comment\n\
             save_dir = \"/tmp/saves\"   # trailing\n\
             language = \"ja-JP\"\n",
        )
        .unwrap();
        assert_eq!(config.save_dir.as_deref(), Some(Path::new("/tmp/saves")));
        assert_eq!(config.language.as_deref(), Some("ja-JP"));
    }

    #[test]
    fn rejects_malformed_lines_and_unknown_keys() {
        for text in [
            "save_dir = /tmp/saves\n",
            "save_dir = \"unterminated\n",
            "save_dir = \"a\" trailing\n",
            "unknown_key = \"x\"\n",
            "language = \"\"\n",
            "save_dir\n",
        ] {
            assert!(Config::parse(text).is_err(), "{text:?} must be rejected");
        }
    }

    #[test]
    fn literal_strings_need_no_escaping() {
        // TOML's single-quoted literal string: a Windows path is written as
        // it is, backslashes and all.
        let config = Config::parse(
            "save_dir = 'C:\\Users\\me\\saves'   # a comment\n\
             language = 'en-US'\n",
        )
        .unwrap();
        assert_eq!(
            config.save_dir.as_deref(),
            Some(Path::new("C:\\Users\\me\\saves"))
        );
        assert_eq!(config.language.as_deref(), Some("en-US"));
        // A `#` inside either string form is not a comment, and a backslash
        // inside a literal one is not an escape.
        let config = Config::parse("save_dir = 'C:\\temp\\a#b'\n").unwrap();
        assert_eq!(config.save_dir.as_deref(), Some(Path::new("C:\\temp\\a#b")));
        assert!(Config::parse("save_dir = 'unterminated\n").is_err());
    }

    #[test]
    fn the_old_extra_large_text_size_is_the_largest() {
        let config = Config::parse(
            "[game]
menu_text_size = \"extra_large\"
",
        )
        .unwrap();
        assert_eq!(config.game.menu_text_size, TextSize::Large);
        assert!(
            Config::parse(
                "[game]
menu_text_size = \"huge\"
"
            )
            .is_err()
        );
    }

    #[test]
    fn quotes_and_escapes_round_trip() {
        let config = Config::parse("save_dir = \"a\\\\b\\\"c\"\n").unwrap();
        assert_eq!(config.save_dir.as_deref(), Some(Path::new("a\\b\"c")));
        let text = Config {
            save_dir: Some(PathBuf::from("/tmp/saves")),
            language: Some("en-US".into()),
            bindings: None,
            graphics: Graphics::default(),
            mods: Mods::default(),
            games: Games::default(),
            updates: Updates::default(),
            game: Game::default(),
        }
        .render();
        assert_eq!(text, "save_dir = \"/tmp/saves\"\nlanguage = \"en-US\"\n");
        assert_eq!(
            Config::parse(&text).unwrap().language.as_deref(),
            Some("en-US")
        );
    }

    #[test]
    fn a_bindings_table_overrides_only_the_actions_it_names() {
        let config = Config::parse(
            "language = \"en-US\"\n\
             \n\
             [bindings]  # the input map\n\
             aim = [\"key:C\", 'mouse:right', \"axis:lefttrigger+\"]  # three devices\n\
             fire = \"wheel:up\"\n\
             page_left = []\n",
        )
        .unwrap();
        assert_eq!(config.language.as_deref(), Some("en-US"));
        let bindings = config.bindings_or_default();
        let names = |action| -> Vec<String> {
            bindings.get(action).iter().map(|s| s.to_string()).collect()
        };
        assert_eq!(
            names(Action::Aim),
            ["key:C", "mouse:right", "axis:lefttrigger+"]
        );
        assert_eq!(names(Action::Fire), ["wheel:up"]);
        assert!(names(Action::PageLeft).is_empty(), "an empty array unbinds");
        assert_eq!(
            bindings.get(Action::Confirm),
            Bindings::default().get(Action::Confirm),
            "unnamed actions keep their defaults"
        );
        // The rendered table parses back to the same map.
        let text = config.render();
        assert!(text.contains("\n[bindings]\n"), "{text}");
        assert!(
            !text.contains("confirm"),
            "defaults are not written: {text}"
        );
        assert_eq!(Config::parse(&text).unwrap(), config);
    }

    #[test]
    fn rejects_bad_bindings() {
        for text in [
            "[bindings]\nnope = [\"key:C\"]\n",
            "[bindings]\naim = [\"key:NoSuchKey\"]\n",
            "[bindings]\naim = [\"key:C\" \"key:Z\"]\n",
            "[bindings]\naim = [\"key:C\"\n",
            "[bindings]\nsave_dir = \"/tmp\"\n",
            "[bindings]\n[bindings]\n",
            "[other]\n",
            "[bindings]\naim = []\naim = []\n",
            "[bindings]\nconfirm = []\n",
            "[bindings]\ncancel = [\"key:X\", \"key:Space\", \"key:Return\"]\n",
        ] {
            assert!(Config::parse(text).is_err(), "{text:?} must be rejected");
        }
    }

    #[test]
    fn reserved_inputs_in_old_bindings_are_dropped() {
        // `pad:start` was bindable before it opened the in-game menu.
        let config =
            Config::parse("[bindings]\nstart = [\"key:Tab\", \"pad:start\"]\naim = [\"key:F1\"]\n")
                .unwrap();
        let bindings = config.bindings.unwrap();
        assert_eq!(
            bindings.get(Action::Start),
            [Source::parse("key:Tab").unwrap()]
        );
        assert!(bindings.get(Action::Aim).is_empty());
    }

    #[test]
    fn saved_bindings_survive_a_reload_and_a_reset_drops_them() {
        let dir =
            std::env::temp_dir().join(format!("arklay-config-{}-bindings", std::process::id()));
        let path = dir.join("arklay.toml");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(&path, "language = \"ja-JP\"\n").unwrap();
        let mut bindings = Bindings::default();
        assert!(bindings.bind(Action::Aim, usize::MAX, Source::Mouse(3)));
        Config::save_bindings(&path, &bindings).unwrap();
        let (loaded, _) = Config::load(&path);
        assert_eq!(loaded.language.as_deref(), Some("ja-JP"), "other keys kept");
        assert_eq!(loaded.bindings_or_default(), bindings);
        Config::save_bindings(&path, &Bindings::default()).unwrap();
        assert_eq!(Config::load(&path).0.bindings, None);
        assert_eq!(fs::read_to_string(&path).unwrap(), "language = \"ja-JP\"\n");
        // A file that does not parse is never overwritten.
        fs::write(&path, "garbage\n").unwrap();
        assert!(Config::save_bindings(&path, &bindings).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "garbage\n");
        // Paths that need escaping, and `~`, survive a save untouched.
        let text = "save_dir = 'C:\\Users\\me \"x\"\\saves'\nlanguage = \"en-US\"\n";
        fs::write(&path, text).unwrap();
        Config::save_bindings(&path, &bindings).unwrap();
        let saved = Config::parse(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            saved.save_dir.as_deref(),
            Some(Path::new("C:\\Users\\me \"x\"\\saves"))
        );
        fs::write(&path, "save_dir = \"~/saves\"\n").unwrap();
        Config::save_bindings(&path, &bindings).unwrap();
        let saved = Config::parse(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved.save_dir.as_deref(), Some(Path::new("~/saves")));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_graphics_table_sets_the_window_and_renderer_options() {
        let config = Config::parse(
            "[graphics]\n\
             renderer = \"gpu\"\n\
             fullscreen = true  # borderless\n\
             vsync = false\n\
             window_width = 1280\n\
             window_height = 1024\n\
             display = 2\n\
             render_scale = 150\n\
             msaa = 4\n\
             frame_rate = 75\n\
             aspect = \"16:9\"\n",
        )
        .unwrap();
        assert_eq!(
            config.graphics,
            Graphics {
                renderer: RendererKind::Gpu,
                fullscreen: true,
                window_width: 1280,
                window_height: 1024,
                display: 2,
                render_scale: 150,
                msaa: 4,
                vsync: false,
                frame_rate: 75,
                aspect: video::Aspect::WIDE,
                hdr: true,
            }
        );
        let text = config.render();
        assert!(text.starts_with("[graphics]\n"), "{text}");
        assert_eq!(Config::parse(&text).unwrap(), config);
        // Keys left out keep their defaults, and defaults are not written.
        let config = Config::parse("[graphics]\nwindow_width = 640\n").unwrap();
        assert_eq!(config.graphics.msaa, 1);
        assert!(config.graphics.vsync);
        assert_eq!(config.graphics.window_height, 720);
        assert_eq!(config.render(), "[graphics]\nwindow_width = 640\n");
    }

    #[test]
    fn bad_graphics_values_warn_and_keep_their_defaults() {
        for text in [
            "[graphics]\nrenderer = \"opengl\"\n",
            "[graphics]\nrenderer = gpu\n",
            "[graphics]\nfullscreen = \"yes\"\n",
            "[graphics]\nwindow_width = 0\n",
            "[graphics]\nwindow_height = \"720\"\n",
            "[graphics]\nwindow_width = 99999\n",
            "[graphics]\ndisplay = 0\n",
            "[graphics]\nrender_scale = 10\n",
            "[graphics]\nmsaa = 3\n",
            "[graphics]\nframe_rate = 29\n",
            "[graphics]\nframe_rate = 1001\n",
            "[graphics]\nvsync = 1\n",
            "[graphics]\nmsaa = 1\nmsaa = 2\n",
            "[graphics]\n[graphics]\n",
        ] {
            assert!(Config::parse(text).is_err(), "{text:?} must be rejected");
            let (config, warnings) = Config::parse_lenient(text);
            assert_eq!(warnings.len(), 1, "{text:?}: {warnings:?}");
            assert_eq!(config.graphics, Graphics::default(), "{text:?}");
        }
        // Only the bad line falls back; the rest of the file still applies.
        let (config, warnings) = Config::parse_lenient(
            "language = \"ja-JP\"\n\
             [graphics]\n\
             renderer = \"gpu\"\n\
             msaa = 3\n\
             window_width = 640\n\
             [bindings]\n\
             aim = [\"key:NoSuchKey\"]\n\
             fire = \"wheel:up\"\n",
        );
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(warnings[0].contains("line 4"), "{warnings:?}");
        assert_eq!(config.language.as_deref(), Some("ja-JP"));
        assert_eq!(config.graphics.renderer, RendererKind::Gpu);
        assert_eq!(config.graphics.msaa, 1);
        assert_eq!(config.graphics.window_width, 640);
        let bindings = config.bindings_or_default();
        assert_eq!(
            bindings.get(Action::Aim),
            Bindings::default().get(Action::Aim)
        );
        assert_eq!(
            bindings.get(Action::Fire),
            [Source::Wheel(crate::controls::Wheel::Up)]
        );
        // A bindings table that locks out the menus falls back as a whole.
        let (config, warnings) = Config::parse_lenient("[bindings]\nconfirm = []\n");
        assert_eq!((config.bindings, warnings.len()), (None, 1));
    }

    #[test]
    fn a_game_table_sets_the_gameplay_options() {
        let config = Config::parse("[game]\nquick_turn = true  # BACKWARD + RUN\n").unwrap();
        assert!(config.game.quick_turn);
        // Both behaviours are on by default, so an explicit `true` is not
        // written back.
        assert_eq!(config.render(), "");
        assert_eq!(Config::parse(&config.render()).unwrap(), config);
        let config = Config::parse("[game]\nquick_turn = false\n").unwrap();
        assert!(!config.game.quick_turn);
        assert_eq!(config.render(), "[game]\nquick_turn = false\n");
        assert_eq!(Config::parse(&config.render()).unwrap(), config);
        // The tactical reload is the second `[game]` behaviour and renders in
        // the table's key order.
        let config = Config::parse("[game]\ntactical_reload = true\n").unwrap();
        assert!(config.game.tactical_reload);
        assert_eq!(config.render(), "");
        assert_eq!(Config::parse(&config.render()).unwrap(), config);
        let config = Config::parse("[game]\nquick_turn = true\ntactical_reload = true\n").unwrap();
        assert_eq!(config.render(), "");
        let config =
            Config::parse("[game]\nquick_turn = false\ntactical_reload = false\n").unwrap();
        assert_eq!(
            config.render(),
            "[game]\nquick_turn = false\ntactical_reload = false\n"
        );
        assert_eq!(Config::parse(&config.render()).unwrap(), config);
        assert!(Config::parse("").unwrap().game.quick_turn);
        assert!(Config::parse("").unwrap().game.tactical_reload);
    }

    #[test]
    fn bad_game_values_warn_and_keep_their_defaults() {
        for text in [
            "[game]\nquick_turn = \"yes\"\n",
            "[game]\nquick_turn = 1\n",
            "[game]\ntactical_reload = \"yes\"\n",
            "[game]\ntactical_reload = 1\n",
            "[game]\nunknown = true\n",
            "[game]\n[game]\n",
        ] {
            assert!(Config::parse(text).is_err(), "{text:?} must be rejected");
            let (config, warnings) = Config::parse_lenient(text);
            assert!(!warnings.is_empty(), "{text:?} must warn");
            assert!(config.game.quick_turn, "{text:?}");
            assert!(config.game.tactical_reload, "{text:?}");
        }
        // A duplicate key warns and keeps the first value.
        for (key, value) in [
            ("quick_turn", "quick_turn"),
            ("tactical_reload", "tactical_reload"),
        ] {
            let text = format!("[game]\n{key} = true\n{value} = false\n");
            assert!(Config::parse(&text).is_err());
            let (config, warnings) = Config::parse_lenient(&text);
            assert_eq!(warnings.len(), 1, "{warnings:?}");
            assert!(
                if key == "quick_turn" {
                    config.game.quick_turn
                } else {
                    config.game.tactical_reload
                },
                "the first value is kept"
            );
        }
        // Only the bad line falls back; the rest of the file still applies.
        let (config, warnings) = Config::parse_lenient("[game]\nquick_turn = true\n");
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(config.game.quick_turn);
        let (config, warnings) = Config::parse_lenient(
            "[game]\nquick_turn = true\n[bindings]\naim = [\"key:NoSuchKey\"]\n",
        );
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(config.game.quick_turn);
    }

    #[test]
    fn a_mods_table_selects_and_orders_discovered_mods() {
        let config = Config::parse(
            "[mods]\n\
             enabled = [\"hd\", 'demo']  # only these discovered mods\n\
             disabled = [\"balance\"]\n\
             order = [\"demo\", \"hd\"]\n",
        )
        .unwrap();
        assert_eq!(
            config.mods,
            Mods {
                enabled: Some(vec!["hd".into(), "demo".into()]),
                disabled: vec!["balance".into()],
                order: vec!["demo".into(), "hd".into()],
            }
        );
        let text = config.render();
        assert!(text.starts_with("[mods]\n"), "{text}");
        assert_eq!(Config::parse(&text).unwrap(), config);
        // An empty `enabled` list is meaningful (apply no discovered mods) and
        // survives the round trip.
        let config = Config::parse("[mods]\nenabled = []\n").unwrap();
        assert_eq!(config.mods.enabled, Some(Vec::new()));
        assert_eq!(Config::parse(&config.render()).unwrap(), config);
        // A config without the table keeps the all-discovered default.
        assert_eq!(Config::parse("").unwrap().mods, Mods::default());
    }

    #[test]
    fn bad_mods_values_warn_and_keep_their_defaults() {
        for text in [
            "[mods]\nenabled = [\"\"]\n",
            "[mods]\nenabled = [\"hd\" \"demo\"]\n",
            "[mods]\nenabled = [\"hd\"\n",
            "[mods]\nunknown = [\"hd\"]\n",
            "[mods]\nenabled = [\"hd\"]\norder = [\"hd\"]\norder = []\n",
            "[mods]\n[mods]\n",
            "[mods]\nenabled = 3\n",
        ] {
            assert!(Config::parse(text).is_err(), "{text:?} must be rejected");
            let (_, warnings) = Config::parse_lenient(text);
            assert!(!warnings.is_empty(), "{text:?} must warn");
        }
        // A bad mods line does not lose the rest of the file.
        let (config, warnings) =
            Config::parse_lenient("language = \"ja-JP\"\n[mods]\nunknown = []\n");
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(config.language.as_deref(), Some("ja-JP"));
    }

    #[test]
    fn saving_the_window_keeps_the_renderer_keys() {
        let dir = std::env::temp_dir().join(format!("arklay-config-{}-window", std::process::id()));
        let path = dir.join("arklay.toml");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            &path,
            "language = \"ja-JP\"\n[graphics]\nrender_scale = 50\n[game]\ntactical_reload = true\n",
        )
        .unwrap();
        let run = Graphics {
            renderer: RendererKind::Gpu,
            fullscreen: true,
            window_width: 1600,
            window_height: 1200,
            display: 2,
            render_scale: 200,
            msaa: 8,
            vsync: false,
            frame_rate: 120,
            aspect: video::Aspect::WIDE,
            hdr: false,
        };
        Config::save_window(&path, &run).unwrap();
        let (saved, warnings) = Config::load(&path);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(saved.language.as_deref(), Some("ja-JP"));
        assert!(saved.game.tactical_reload, "the [game] table is kept");
        assert_eq!(
            saved.graphics,
            Graphics {
                fullscreen: true,
                window_width: 1600,
                window_height: 1200,
                display: 2,
                render_scale: 50,
                ..Graphics::default()
            },
            "only the window keys are written"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn saving_options_writes_the_graphics_language_and_game_tables() {
        let dir =
            std::env::temp_dir().join(format!("arklay-config-{}-options", std::process::id()));
        let path = dir.join("arklay.toml");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            &path,
            "save_dir = \"/tmp/saves\"\n[mods]\ndisabled = [\"x\"]\n",
        )
        .unwrap();
        let graphics = Graphics {
            renderer: RendererKind::Software,
            fullscreen: true,
            window_width: 1920,
            window_height: 1080,
            display: 2,
            render_scale: 150,
            msaa: 4,
            vsync: false,
            frame_rate: 144,
            aspect: video::Aspect::WIDE,
            hdr: false,
        };
        let game = Game {
            quick_turn: true,
            tactical_reload: true,
            analog_movement: false,
            auto_aim: false,
            blood: false,
            chris_ink_ribbons: false,
            jill_ink_ribbons: true,
            autosave: false,
            room_script_fixes: false,
            ui_font: crate::gui::FontFamily::Retro,
            menu_text_size: TextSize::Large,
            modern_save_screen: false,
            button_style: Some(crate::gui::PadStyle::Nintendo),
            control_type: ControlType::C,
            vibration: Vibration::Off,
            master_volume: 70,
            music_volume: 40,
            ambient_volume: 0,
            effects_volume: 100,
            voice_volume: 85,
            menu_volume: 0,
            subtitles: false,
            subtitle_size: SubtitleSize::Large,
            subtitle_background: 0,
            subtitle_color: TextColor::Green,
            speaker_names: true,
            speaker_name_color: TextColor::Red,
        };
        Config::save_options(&path, &graphics, Some("ja-JP"), game).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("renderer = \"software\""), "{text}");
        let (saved, warnings) = Config::load(&path);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(saved.graphics, graphics);
        assert_eq!(saved.language.as_deref(), Some("ja-JP"));
        assert_eq!(saved.game, game);
        assert_eq!(saved.save_dir.as_deref(), Some(Path::new("/tmp/saves")));
        assert_eq!(saved.mods.disabled, ["x"]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn saving_the_frame_rate_keeps_the_other_keys() {
        let dir = std::env::temp_dir().join(format!("arklay-config-{}-rate", std::process::id()));
        let path = dir.join("arklay.toml");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(&path, "[graphics]\nmsaa = 4\n").unwrap();
        Config::save_frame_rate(&path, 144).unwrap();
        let (saved, warnings) = Config::load(&path);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!((saved.graphics.msaa, saved.graphics.frame_rate), (4, 144));
        // The default rate is not written.
        Config::save_frame_rate(&path, 30).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "[graphics]\nmsaa = 4\n");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn saving_the_aspect_keeps_the_other_keys() {
        let dir = std::env::temp_dir().join(format!("arklay-config-{}-aspect", std::process::id()));
        let path = dir.join("arklay.toml");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(&path, "[graphics]\nmsaa = 4\n").unwrap();
        Config::save_aspect(&path, video::Aspect::WIDE).unwrap();
        let (saved, warnings) = Config::load(&path);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(
            (saved.graphics.msaa, saved.graphics.aspect),
            (4, video::Aspect::WIDE)
        );
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains("aspect = \"16:9\"\n")
        );
        // The default 4:3 is not written.
        Config::save_aspect(&path, video::Aspect::STANDARD).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "[graphics]\nmsaa = 4\n");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parses_any_aspect_and_rejects_a_bad_one() {
        let config = Config::parse("[graphics]\naspect = \"16:10\"\n").unwrap();
        assert_eq!(config.graphics.aspect.name(), "16:10");
        for text in [
            "[graphics]\naspect = \"wide\"\n",
            "[graphics]\naspect = \"0:9\"\n",
        ] {
            assert!(Config::parse(text).is_err(), "{text:?} must be rejected");
        }
    }

    #[test]
    fn the_default_render_is_empty_and_parses_back() {
        assert_eq!(Config::default().render(), "");
        assert_eq!(Config::parse("").unwrap(), Config::default());
    }

    #[test]
    fn tilde_expands_against_the_home_directory() {
        assert_eq!(
            expand_tilde_with(Path::new("~/.config/arklay/save"), Path::new("/home/ted")),
            Path::new("/home/ted/.config/arklay/save")
        );
        assert_eq!(
            expand_tilde_with(Path::new("~/saves"), Path::new("/home/ted")),
            Path::new("/home/ted/saves")
        );
        // Only a leading `~/` expands; `~user` and a bare `~` do not.
        assert_eq!(
            expand_tilde_with(Path::new("~ted/saves"), Path::new("/home/ted")),
            Path::new("~ted/saves")
        );
        assert_eq!(
            expand_tilde_with(Path::new("saves/~x"), Path::new("/home/ted")),
            Path::new("saves/~x")
        );
    }

    #[test]
    fn the_config_directory_ends_with_the_directory_name() {
        assert!(Config::dir().ends_with(DIR_NAME));
        assert!(Config::default_save_dir().ends_with(SAVE_DIR_NAME));
        assert_eq!(
            Config::default_path()
                .file_name()
                .and_then(|name| name.to_str()),
            Some(FILE_NAME)
        );
    }

    #[test]
    fn load_reads_a_file_and_reports_other_errors() {
        let dir = std::env::temp_dir().join(format!("arklay-config-{}-load", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("arklay.toml");
        fs::write(&path, "language = \"ja-JP\"\n").unwrap();
        assert_eq!(Config::load(&path).0.language.as_deref(), Some("ja-JP"));

        // A directory where the file should be is reported, and the defaults
        // apply.
        let blocked = dir.join("blocked");
        fs::create_dir_all(&blocked).unwrap();
        let (config, warnings) = Config::load(&blocked);
        assert_eq!(config, Config::default());
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        // A bad value is reported with the file's path.
        fs::write(&path, "[graphics]\nmsaa = 3\n").unwrap();
        let (config, warnings) = Config::load(&path);
        assert_eq!(config, Config::default());
        assert!(warnings[0].contains("arklay.toml"), "{warnings:?}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_updates_table_roundtrips_and_saves() {
        let text = "[updates]\nchannel = \"nightly\"\ncheck = false\nlast_check = 1775\n\
                    available_channel = \"nightly\"\navailable_version = \"0.1.0-nightly\"\n\
                    available_sha = \"abc123\"\n";
        let config = Config::parse(text).unwrap();
        assert_eq!(config.updates.channel, UpdateChannel::Nightly);
        assert!(!config.updates.check);
        assert_eq!(config.updates.last_check, 1775);
        assert_eq!(config.updates.available_sha.as_deref(), Some("abc123"));
        assert_eq!(Config::parse(&config.render()).unwrap(), config);

        let dir =
            std::env::temp_dir().join(format!("arklay-config-{}-updates", std::process::id()));
        let path = dir.join("arklay.toml");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let updates = Updates {
            channel: UpdateChannel::Nightly,
            available_version: Some("0.1.0-nightly".to_string()),
            ..Updates::default()
        };
        Config::save_updates(&path, &updates).unwrap();
        assert_eq!(Config::load(&path).0.updates, updates);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_games_source_roundtrips_and_saves() {
        let text = "[games]\nre1_source = 'C:\\Games\\RE1'\nre1_build = \"21744136\"\n";
        let config = Config::parse(text).unwrap();
        assert_eq!(
            config.games.re1_source.as_deref(),
            Some(Path::new(r"C:\Games\RE1"))
        );
        assert_eq!(config.games.re1_build.as_deref(), Some("21744136"));
        assert_eq!(Config::parse(&config.render()).unwrap(), config);

        let dir = std::env::temp_dir().join(format!("arklay-config-{}-games", std::process::id()));
        let path = dir.join("arklay.toml");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(&path, "language = \"en-US\"\n").unwrap();
        Config::save_games(&path, Some(Path::new(r"D:\Steam\RE1")), Some("42")).unwrap();
        let (loaded, warnings) = Config::load(&path);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(
            loaded.games.re1_source.as_deref(),
            Some(Path::new(r"D:\Steam\RE1"))
        );
        assert_eq!(loaded.games.re1_build.as_deref(), Some("42"));
        assert_eq!(loaded.language.as_deref(), Some("en-US"));

        Config::save_games(&path, None, None).unwrap();
        assert_eq!(Config::load(&path).0.games, Games::default());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_over_cap_file_is_rejected() {
        let text = "x".repeat(budget::MAX_TEXT_BYTES + 1);
        let message = budget::assert_cap_error(Config::parse(&text));
        assert!(message.contains("config file size"), "{message}");
    }
}
