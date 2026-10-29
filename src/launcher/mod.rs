//! The startup launcher shown when no game pack is named on the command line.
//!
//! The launcher is not a game screen: it opens before any pack exists. Its
//! screens are the `launcher_*` documents in `assets/ui/`, run by the same
//! retained UI as the in-game menus ([`crate::gui::dom`]); this module
//! publishes the launcher's state into the UI's store, turns the actions the
//! screens report into installs, updates and settings, and presents the
//! painted canvas in the window.
//!
//! Play hands this window to the game in-process; closing the game window
//! ends the process. Every base pack is listed; Install / Rebuild lists the
//! games and opens the chosen one's install, and Update has a rebuild row
//! per game. Mods are discovered from every data root for every game and
//! toggled or reordered through `[mods]` in `arklay.toml`. Options is the
//! game's own options screen ([`SystemMenu::options`]), in the game's face,
//! text size and language. Installs and rebuilds run on a worker thread with
//! a live progress screen and cancellation.

use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use sdl3_sys::dialog::SDL_ShowOpenFolderDialog;
use sdl3_sys::events::{
    SDL_EVENT_GAMEPAD_ADDED, SDL_EVENT_GAMEPAD_AXIS_MOTION, SDL_EVENT_GAMEPAD_BUTTON_DOWN,
    SDL_EVENT_GAMEPAD_REMOVED, SDL_EVENT_KEY_DOWN, SDL_EVENT_MOUSE_BUTTON_DOWN,
    SDL_EVENT_MOUSE_BUTTON_UP, SDL_EVENT_MOUSE_MOTION, SDL_EVENT_MOUSE_WHEEL, SDL_EVENT_QUIT,
    SDL_EVENT_WINDOW_EXPOSED, SDL_EVENT_WINDOW_FOCUS_GAINED, SDL_EVENT_WINDOW_FOCUS_LOST,
    SDL_EVENT_WINDOW_PIXEL_SIZE_CHANGED, SDL_EVENT_WINDOW_RESIZED, SDL_Event, SDL_PollEvent,
    SDL_WaitEventTimeout,
};
use sdl3_sys::gamepad::{
    SDL_GAMEPAD_AXIS_LEFT_TRIGGER, SDL_GAMEPAD_AXIS_LEFTX, SDL_GAMEPAD_AXIS_LEFTY,
    SDL_GAMEPAD_AXIS_RIGHT_TRIGGER, SDL_GAMEPAD_BUTTON_BACK, SDL_GAMEPAD_BUTTON_DPAD_DOWN,
    SDL_GAMEPAD_BUTTON_DPAD_LEFT, SDL_GAMEPAD_BUTTON_DPAD_RIGHT, SDL_GAMEPAD_BUTTON_DPAD_UP,
    SDL_GAMEPAD_BUTTON_EAST, SDL_GAMEPAD_BUTTON_LEFT_SHOULDER, SDL_GAMEPAD_BUTTON_NORTH,
    SDL_GAMEPAD_BUTTON_RIGHT_SHOULDER, SDL_GAMEPAD_BUTTON_SOUTH, SDL_GAMEPAD_BUTTON_START,
    SDL_GAMEPAD_BUTTON_WEST,
};
use sdl3_sys::keycode::{SDL_KMOD_ALT, SDL_KMOD_CTRL, SDL_Keymod};
use sdl3_sys::mouse::{SDL_BUTTON_LEFT, SDL_MOUSEWHEEL_FLIPPED};
use sdl3_sys::scancode::{
    SDL_SCANCODE_1, SDL_SCANCODE_3, SDL_SCANCODE_A, SDL_SCANCODE_B, SDL_SCANCODE_D,
    SDL_SCANCODE_DOWN, SDL_SCANCODE_E, SDL_SCANCODE_EQUALS, SDL_SCANCODE_ESCAPE,
    SDL_SCANCODE_KP_ENTER, SDL_SCANCODE_LEFT, SDL_SCANCODE_MINUS, SDL_SCANCODE_O, SDL_SCANCODE_Q,
    SDL_SCANCODE_R, SDL_SCANCODE_RETURN, SDL_SCANCODE_RIGHT, SDL_SCANCODE_S, SDL_SCANCODE_SPACE,
    SDL_SCANCODE_TAB, SDL_SCANCODE_UP, SDL_SCANCODE_W, SDL_Scancode,
};

use crate::build_info;
use crate::config::{Config, Mods, UpdateChannel};
use crate::controls::{self, Source as Input};
use crate::convert::{self, CONVERT_VERSION, Game};
use crate::engine::{self, Display, Gamepads, ModOptions};
use crate::gui::canvas::{Canvas, Color, Rect};
use crate::gui::dom::{Custom, Library, Node, Ui, UiOutput, Value};
use crate::gui::locale::Locale;
use crate::gui::theme::palette;
use crate::gui::widgets::Painter;
use crate::gui::{Device, GuiEvent, GuiKey, MenuRequest, Settings, SystemMenu};
use crate::logging;
use crate::manifest;
use crate::modding;
use crate::pack;
use crate::paths;
use crate::player;
use crate::progress::ProgressSnapshot;
use crate::steam;
use crate::update::{self, UpdateInfo};
use crate::video::{self, RendererKind};

/// The launcher's window title. Play replaces it with the pack's name, and a
/// failed game puts it back.
const WINDOW_TITLE: &str = "Arklay";
/// The deflection that starts a gamepad menu direction.
const PAD_DEADZONE: i16 = 16_000;
/// The deflection a held direction releases at (hysteresis, so a stick
/// resting near the threshold does not chatter).
const PAD_RELEASE: i16 = 12_000;
/// The pause before a held vertical direction repeats.
const PAD_INITIAL_DELAY: Duration = Duration::from_millis(350);
/// The interval between repeats while a vertical direction is held.
const PAD_REPEAT_DELAY: Duration = Duration::from_millis(110);

/// One axis direction with press-repeat timing.
#[derive(Default)]
struct PadAxis {
    direction: i8,
    next: Option<Instant>,
    /// The held direction was taken for something else (a key being bound):
    /// it steps again only once released or reversed.
    silent: bool,
}

impl PadAxis {
    /// Fold an axis reading into a menu step. A new direction fires at once;
    /// with `repeat`, a held one repeats after [`PAD_INITIAL_DELAY`], then
    /// every [`PAD_REPEAT_DELAY`].
    fn poll(&mut self, value: i16, now: Instant, repeat: bool) -> Option<i8> {
        let direction = if value >= PAD_DEADZONE {
            1
        } else if value <= -PAD_DEADZONE {
            -1
        } else if self.direction > 0 && value >= PAD_RELEASE {
            1
        } else if self.direction < 0 && value <= -PAD_RELEASE {
            -1
        } else {
            0
        };
        if direction != self.direction {
            self.direction = direction;
            self.silent = false;
            self.next = (direction != 0 && repeat).then(|| now + PAD_INITIAL_DELAY);
            return (direction != 0).then_some(direction);
        }
        if self.silent {
            return None;
        }
        if repeat && direction != 0 && self.next.is_none_or(|next| now >= next) {
            self.next = Some(now + PAD_REPEAT_DELAY);
            return Some(direction);
        }
        None
    }

    /// Take `value`'s direction without a step; it stays silent until it is
    /// released or reversed.
    fn swallow(&mut self, value: i16) {
        self.poll(value, Instant::now(), false);
        self.next = None;
        self.silent = true;
    }
}

/// Thousands-separated integer, for the progress counters.
fn format_count(value: u64) -> String {
    let text = value.to_string();
    let mut out = String::with_capacity(text.len() + text.len() / 3);
    for (index, ch) in text.chars().enumerate() {
        if index > 0 && (text.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// `mm:ss` for the progress screen.
fn format_elapsed(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

/// The screens the launcher can show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Screen {
    Main,
    Play,
    /// The games Install / Rebuild offers.
    Games,
    /// One game's install.
    Install,
    Update,
    Mods,
    Progress,
    Message,
}

impl Screen {
    const ALL: [Self; 8] = [
        Self::Main,
        Self::Play,
        Self::Games,
        Self::Install,
        Self::Update,
        Self::Mods,
        Self::Progress,
        Self::Message,
    ];

    /// The screen's document in `assets/ui/`.
    fn name(self) -> &'static str {
        match self {
            Self::Main => "launcher_main",
            Self::Play => "launcher_play",
            Self::Games => "launcher_games",
            Self::Install => "launcher_install",
            Self::Update => "launcher_update",
            Self::Mods => "launcher_mods",
            Self::Progress => "launcher_progress",
            Self::Message => "launcher_message",
        }
    }

    fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|screen| screen.name() == name)
    }

    /// The screen this one is opened from, under the main menu.
    fn parent(self) -> Option<Self> {
        match self {
            Self::Install => Some(Self::Games),
            _ => None,
        }
    }
}

/// The launcher's screens and components.
const DOCUMENTS: [(&str, &str); 9] = [
    (
        "launcher_components",
        include_str!("../../assets/ui/launcher_components.xml"),
    ),
    (
        "launcher_main",
        include_str!("../../assets/ui/launcher_main.xml"),
    ),
    (
        "launcher_play",
        include_str!("../../assets/ui/launcher_play.xml"),
    ),
    (
        "launcher_games",
        include_str!("../../assets/ui/launcher_games.xml"),
    ),
    (
        "launcher_install",
        include_str!("../../assets/ui/launcher_install.xml"),
    ),
    (
        "launcher_update",
        include_str!("../../assets/ui/launcher_update.xml"),
    ),
    (
        "launcher_mods",
        include_str!("../../assets/ui/launcher_mods.xml"),
    ),
    (
        "launcher_progress",
        include_str!("../../assets/ui/launcher_progress.xml"),
    ),
    (
        "launcher_message",
        include_str!("../../assets/ui/launcher_message.xml"),
    ),
];

/// The engine's built-in screens with the launcher's added.
fn library() -> Result<Library> {
    let mut library = Library::builtin();
    for (name, source) in DOCUMENTS {
        library.add(name, source)?;
    }
    Ok(library)
}

/// Window sizes at the 4:3 aspect, from 3x to 8x the 320x240 frame.
const RESOLUTIONS_STANDARD: [(u32, u32); 5] = [
    (960, 720),
    (1280, 960),
    (1600, 1200),
    (1920, 1440),
    (2560, 1920),
];

/// Window sizes at the 16:9 aspect, from 3x to 8x the 320x180 band.
const RESOLUTIONS_WIDE: [(u32, u32); 5] = [
    (960, 540),
    (1280, 720),
    (1600, 900),
    (1920, 1080),
    (2560, 1440),
];

/// One installed pack found under a data root.
#[derive(Clone)]
struct PackRow {
    id: String,
    name: String,
    file: String,
    version: String,
    source: String,
    path: PathBuf,
    stale: bool,
    error: Option<String>,
}

/// The remembered game installation.
#[derive(Clone)]
struct Source {
    path: PathBuf,
    build: Option<String>,
}

/// One discovered mod.
#[derive(Clone)]
struct ModRow {
    name: String,
    /// The title of the game it is for, or its base id when no game has it.
    game: String,
    id: String,
    version: String,
    enabled: bool,
    load_order: i32,
    path: PathBuf,
}

/// The latest progress snapshot, copied out of the worker's lock.
#[derive(Clone, Default)]
struct ProgressState {
    label: String,
    item: String,
    done: u64,
    total: u64,
    phases_done: u64,
    phases_total: u64,
    overall: f64,
    elapsed: Duration,
}

impl From<ProgressSnapshot> for ProgressState {
    fn from(snapshot: ProgressSnapshot) -> Self {
        Self {
            label: snapshot.label,
            item: snapshot.item,
            done: snapshot.done,
            total: snapshot.total,
            phases_done: snapshot.phases_done,
            phases_total: snapshot.phases_total,
            overall: snapshot.overall,
            elapsed: snapshot.elapsed,
        }
    }
}

/// A background conversion and the UI-side handles to it.
struct Job {
    receiver: mpsc::Receiver<Result<(), String>>,
    cancel: Arc<AtomicBool>,
    state: Arc<Mutex<ProgressState>>,
    game: Game,
    source: PathBuf,
    build: Option<String>,
    target: PathBuf,
    rebuild: bool,
    /// The screen it was started from, which a failure returns to.
    back: Screen,
}

/// A background engine download.
struct UpdateJob {
    receiver: mpsc::Receiver<Result<(), String>>,
    cancel: Arc<AtomicBool>,
    state: Arc<Mutex<ProgressState>>,
}

/// The result of one folder-dialog invocation, filled in by the SDL callback.
struct DialogState {
    done: AtomicBool,
    path: Mutex<Option<PathBuf>>,
    /// Kept alive for the dialog's lifetime in case SDL uses the pointer
    /// after the call returns.
    _default: Option<CString>,
}

unsafe extern "C" fn folder_callback(
    userdata: *mut c_void,
    filelist: *const *const c_char,
    _filter: c_int,
) {
    // SAFETY: `userdata` is the `Arc<DialogState>` kept alive in the launcher
    // until this callback marks it done.
    let state = unsafe { &*(userdata as *const DialogState) };
    let path = if filelist.is_null() {
        None
    } else {
        // SAFETY: SDL passes a NULL-terminated array of UTF-8 paths.
        let first = unsafe { *filelist };
        if first.is_null() {
            None
        } else {
            // SAFETY: the first entry is a NUL-terminated C string.
            unsafe { CStr::from_ptr(first) }
                .to_str()
                .ok()
                .map(PathBuf::from)
        }
    };
    if let Ok(mut slot) = state.path.lock() {
        *slot = path;
    }
    state.done.store(true, Ordering::Relaxed);
}

/// The progress screen's bar: the `value` in thousandths, filled from the
/// left in the attention colour once anything is done.
struct ProgressBar;

impl Custom for ProgressBar {
    fn paint(&self, p: &mut Painter<'_>, rect: Rect, node: &Node) {
        let value = node.prop("value").as_int().unwrap_or(0).clamp(0, 1000);
        let edge = p.u(2.0).max(1.0);
        p.canvas.fill_rect(rect, palette::RULE);
        let inner = rect.inset(edge, edge);
        p.canvas.fill_rect(inner, Color::rgb(0x10, 0x10, 0x10));
        if value > 0 {
            let filled = Rect::new(inner.x, inner.y, inner.w * value as f32 / 1000.0, inner.h);
            p.canvas.fill_rect(filled, palette::WARN);
        }
    }
}

/// A mod row's check box: `checked`, centred in its cell.
struct CheckBox;

impl Custom for CheckBox {
    fn paint(&self, p: &mut Painter<'_>, rect: Rect, node: &Node) {
        let checked = node.prop("checked").truthy();
        p.checkbox(rect.center_x(), rect.center_y(), 30.0, checked);
    }
}

/// An option row's definition for the UI: its label, description, choices
/// and the index of the current one.
fn option_def(label: &str, description: &str, choices: &[String], value: usize) -> Value {
    Value::map([
        ("label", Value::from(label)),
        ("description", Value::from(description)),
        (
            "choices",
            Value::list(choices.iter().map(|c| Value::from(c.as_str())).collect()),
        ),
        ("value", Value::from(value)),
    ])
}

/// A status's tone: the attention colour, or dim.
fn tone(warn: bool) -> &'static str {
    if warn { "warn" } else { "value-dim" }
}

/// The launcher state.
struct Launcher {
    ui: Ui,
    play_index: usize,
    mods_index: usize,
    config: Config,
    packs: Vec<PackRow>,
    mods: Vec<ModRow>,
    /// The game the install screen and a rebuild act on.
    game: Game,
    /// Each game's remembered or detected installation.
    sources: HashMap<Game, Source>,
    job: Option<Job>,
    update_job: Option<UpdateJob>,
    checking: bool,
    check_rx: Option<mpsc::Receiver<Result<Option<UpdateInfo>, String>>>,
    auto_update: bool,
    dialog: Option<Arc<DialogState>>,
    capture_progress: Option<ProgressState>,
    /// The game's options screen while it is open, over the launcher.
    options: Option<SystemMenu>,
    pads: Gamepads,
    pad_x: PadAxis,
    pad_y: PadAxis,
    /// The triggers, which page the options' sub-tabs.
    pad_lt: PadAxis,
    pad_rt: PadAxis,
    /// Each pad axis's latched direction, so the key configuration binds a
    /// stick or trigger on its press edge.
    axes: HashMap<(u32, i32), i8>,
    pad_focused: bool,
    /// The device the last pad input came from.
    pad_device: Device,
    /// The painted canvas made opaque for the window, and its size.
    rgba: Vec<u8>,
    size: (u32, u32),
    /// Whether a previous engine build can be restored, looked up when the
    /// Update screen opens rather than on every event.
    rollback: bool,
}

impl Launcher {
    fn new(config: &Config) -> Result<Self> {
        let packs = scan_packs();
        let sources = detect_sources(config);
        let mods = scan_mods(&packs, config, &Config::mod_dirs());
        Self::with(config, packs, sources, mods)
    }

    /// A launcher over the given packs, installations and mods.
    fn with(
        config: &Config,
        packs: Vec<PackRow>,
        sources: HashMap<Game, Source>,
        mods: Vec<ModRow>,
    ) -> Result<Self> {
        let game = initial_game(&packs, &sources);
        let mut ui = Ui::new(library()?)?;
        ui.register("progress-bar", Box::new(ProgressBar));
        ui.register("checkbox", Box::new(CheckBox));
        let mut launcher = Self {
            ui,
            play_index: 0,
            mods_index: 0,
            config: config.clone(),
            packs,
            mods,
            game,
            sources,
            job: None,
            update_job: None,
            checking: false,
            check_rx: None,
            auto_update: false,
            dialog: None,
            capture_progress: None,
            options: None,
            pads: Gamepads::default(),
            pad_x: PadAxis::default(),
            pad_y: PadAxis::default(),
            pad_lt: PadAxis::default(),
            pad_rt: PadAxis::default(),
            axes: HashMap::new(),
            pad_focused: true,
            pad_device: Device::Gamepad(Default::default()),
            rgba: Vec::new(),
            size: (0, 0),
            rollback: false,
        };
        launcher.apply_look();
        launcher.publish();
        launcher.open(Screen::Main);
        if launcher.packs.is_empty() {
            launcher.enter(Screen::Games);
        }
        Ok(launcher)
    }
}

/// Each game's remembered installation, else the one Steam has.
fn detect_sources(config: &Config) -> HashMap<Game, Source> {
    Game::ALL
        .into_iter()
        .filter_map(|game| {
            let source = config
                .games
                .source(game)
                .map(|path| Source {
                    path: path.to_path_buf(),
                    build: config.games.build(game).map(str::to_string),
                })
                .or_else(|| {
                    steam::find(game).map(|install| Source {
                        path: install.root,
                        build: install.build_id,
                    })
                })?;
            Some((game, source))
        })
        .collect()
}

impl Launcher {
    /// Draw in the game menus' face, text size and language.
    fn apply_look(&mut self) {
        let language = self.config.language.as_deref();
        engine::warm_gui_fonts(language);
        self.ui
            .set_locale(Locale::new(&crate::language::resolve_tag(language)));
        self.ui.set_theme(
            self.config.game.ui_font,
            self.config.game.menu_text_size.scale(),
        );
    }

    /// The selected game's pack, when it is installed.
    fn pack(&self) -> Option<&PackRow> {
        pack_for(&self.packs, self.game)
    }

    /// The selected game's installation.
    fn source(&self) -> Option<&Source> {
        self.sources.get(&self.game)
    }

    /// The pack file an install or rebuild of the selected game writes.
    fn target(&self) -> PathBuf {
        target_for(&self.packs, self.game)
    }

    /// The games with a pack or an installation, which Update offers to
    /// rebuild.
    fn games(&self) -> Vec<Game> {
        known_games(&self.packs, &self.sources)
    }

    /// Select `game` for the install screen and a rebuild.
    fn select_game(&mut self, game: Game) {
        self.game = game;
    }

    // ---- screens ------------------------------------------------------

    /// The screen on top.
    fn screen(&self) -> Screen {
        self.ui
            .top()
            .and_then(Screen::named)
            .unwrap_or(Screen::Main)
    }

    /// Open `screen` on top of the stack.
    fn open(&mut self, screen: Screen) {
        if let Err(err) = self.ui.open(screen.name()) {
            logging::write(&format!("launcher: {err:#}"));
        }
    }

    /// Open `screen` over the current one, starting its cursor where the
    /// launcher's state says.
    fn enter(&mut self, screen: Screen) {
        match screen {
            Screen::Mods => {
                self.refresh_mods();
                self.mods_index = 0;
            }
            Screen::Update => self.rollback = update::rollback_available(),
            _ => {}
        }
        self.publish_screen(screen);
        self.open(screen);
        match screen {
            Screen::Play => self.ui.set_focus_index(self.play_index),
            Screen::Games => {
                let index = Game::ALL.iter().position(|&game| game == self.game);
                self.ui.set_focus_index(index.unwrap_or(0));
            }
            Screen::Mods => self.ui.set_focus_index(self.mods_index),
            _ => {}
        }
    }

    /// Show `screen`: back out to it when it is open, else open it (after
    /// the screen it is opened from) over the main menu.
    fn goto(&mut self, screen: Screen) {
        self.publish();
        while self.ui.screens().len() > 1 && self.ui.top() != Some(screen.name()) {
            self.ui.close();
        }
        if self.ui.top() == Some(screen.name()) {
            return;
        }
        if self.ui.top() != Some(Screen::Main.name()) {
            self.ui.clear();
            self.open(Screen::Main);
        }
        if screen != Screen::Main {
            if let Some(parent) = screen.parent() {
                self.enter(parent);
            }
            self.enter(screen);
        }
    }

    /// Show a message over `back`; dismissing it returns there.
    fn show_message(&mut self, title: &str, lines: &[&str], back: Screen) {
        self.goto(back);
        self.ui.set("message.title", title);
        self.ui.set(
            "message.lines",
            Value::list(lines.iter().map(|&line| Value::from(line)).collect()),
        );
        self.open(Screen::Message);
    }

    /// Open the game's options screen over the launcher.
    fn open_options(&mut self, display: &mut Display) {
        display.note_window_size();
        self.sync_window(display);
        match SystemMenu::options(self.settings(display)) {
            Ok(menu) => self.options = Some(menu),
            Err(err) => {
                logging::write(&format!("launcher: options: {err:#}"));
                self.show_message("OPTIONS", &[&format!("{err:#}")], Screen::Main);
            }
        }
    }

    /// The settings the options screen edits: the config's, with the
    /// window as it is now.
    fn settings(&self, display: &Display) -> Settings {
        Settings {
            graphics: self.config.graphics,
            game: self.config.game,
            language: self.config.language.clone(),
            displays: engine::display_names(),
            bindings: self.config.bindings_or_default(),
            custom_pad: self.config.custom_pad_or_default(),
            system: engine::system_info(display),
        }
    }

    // ---- publishing ---------------------------------------------------

    /// Publish what the screen on top shows into the UI's store. Unchanged
    /// values cost nothing to set again; a screen is published as it opens.
    fn publish(&mut self) {
        self.publish_screen(self.screen());
    }

    /// Publish what `screen` shows, and the main menu's statuses.
    fn publish_screen(&mut self, screen: Screen) {
        self.ui.set("launcher.version", build_info::label());
        self.publish_main();
        match screen {
            Screen::Main | Screen::Message => {}
            Screen::Play => self.publish_play(),
            Screen::Games => self.publish_games(),
            Screen::Install => self.publish_install(),
            Screen::Update => self.publish_update(),
            Screen::Mods => self.publish_mods(),
            Screen::Progress => self.publish_progress(),
        }
    }

    fn publish_main(&mut self) {
        let stale: Vec<Game> = Game::ALL
            .into_iter()
            .filter(|&game| pack_for(&self.packs, game).is_some_and(|pack| pack.stale))
            .collect();
        let rebuild = match stale.as_slice() {
            [] => None,
            [game] => Some(game.title().to_string()),
            _ => Some(format!("{} games", stale.len())),
        };
        let update = self.config.updates.available_version.is_some();
        let plural = |count: usize, noun: &str| {
            format!("{count} {noun}{}", if count == 1 { "" } else { "s" })
        };
        let install = if self.packs.is_empty() {
            "Required".to_string()
        } else if let Some(games) = &rebuild {
            format!("Rebuild {games}")
        } else {
            String::new()
        };
        let update = match (update, &rebuild) {
            (true, Some(_)) => "Update + rebuild".to_string(),
            (true, None) => "Update available".to_string(),
            (false, Some(games)) => format!("Rebuild {games}"),
            (false, None) => String::new(),
        };
        self.ui.set("main.play", plural(self.packs.len(), "pack"));
        self.ui.set("main.install", install);
        self.ui.set("main.update", update);
        self.ui.set("main.mods", plural(self.mods.len(), "mod"));
    }

    fn publish_play(&mut self) {
        let rows = self
            .packs
            .iter()
            .map(|pack| {
                let version = if pack.error.is_some() {
                    "Error".to_string()
                } else if pack.stale {
                    format!(
                        "{} *",
                        if pack.version.is_empty() {
                            "-"
                        } else {
                            &pack.version
                        }
                    )
                } else if pack.version.is_empty() {
                    "-".to_string()
                } else {
                    pack.version.clone()
                };
                let warning = if let Some(error) = &pack.error {
                    format!("This pack could not be read: {error}")
                } else if pack.stale {
                    "This pack needs a rebuild before it matches this engine.".to_string()
                } else {
                    String::new()
                };
                Value::map([
                    ("name", Value::from(pack.name.as_str())),
                    ("file", Value::from(pack.file.as_str())),
                    ("version", Value::from(version)),
                    ("source", Value::from(pack.source.as_str())),
                    ("warning", Value::from(warning)),
                ])
            })
            .collect();
        self.ui.set("play.rows", Value::list(rows));
        self.ui.set("play.empty", self.packs.is_empty());
    }

    /// A game's pack state for the game lists, and whether it needs
    /// attention.
    fn game_status(&self, game: Game) -> (String, bool) {
        match pack_for(&self.packs, game) {
            Some(pack) if pack.stale => ("Rebuild required".to_string(), true),
            Some(pack) if pack.version.is_empty() => ("Installed".to_string(), false),
            Some(pack) => (format!("Installed ({})", pack.version), false),
            None if self.sources.contains_key(&game) => ("Ready to install".to_string(), false),
            None => ("Not installed".to_string(), false),
        }
    }

    fn publish_games(&mut self) {
        let rows = Game::ALL
            .into_iter()
            .map(|game| {
                let (status, warn) = self.game_status(game);
                Value::map([
                    ("title", Value::from(game.title())),
                    ("status", Value::from(status)),
                    ("tone", Value::from(tone(warn))),
                ])
            })
            .collect();
        self.ui.set("games.rows", Value::list(rows));
    }

    fn publish_install(&mut self) {
        let pack = self.pack().cloned();
        let game_title = self.game.title();
        let source = self
            .source()
            .map(|source| source.path.display().to_string())
            .unwrap_or_else(|| "Not found".to_string());
        let pack_value = match &pack {
            Some(pack) => format!(
                "{}  ({})",
                pack.file,
                if pack.version.is_empty() {
                    "-"
                } else {
                    &pack.version
                }
            ),
            None => "Not installed".to_string(),
        };
        let (status, warn) = if self.dialog.is_some() {
            ("Waiting for the folder picker...".to_string(), None)
        } else if self.source().is_none() {
            (
                format!("{game_title} not found - choose its folder"),
                Some(true),
            )
        } else if !self.source().is_some_and(|source| source.path.is_dir()) {
            (
                "Source folder missing - choose it again".to_string(),
                Some(true),
            )
        } else if pack.is_none() {
            (format!("Ready - {}", install_size(self.game)), Some(false))
        } else if pack.as_ref().is_some_and(|pack| pack.stale) {
            ("Rebuild required".to_string(), Some(true))
        } else {
            ("Up to date - rebuild available".to_string(), Some(false))
        };
        let status_tone = match warn {
            Some(true) => "warn",
            Some(false) => "value",
            None => "value-dim",
        };
        self.ui.set("install.title", game_title.to_uppercase());
        self.ui.set(
            "install.caption",
            if self.packs.is_empty() {
                "First-time setup"
            } else {
                ""
            },
        );
        self.ui.set("install.source", source);
        self.ui
            .set("install.target", self.target().display().to_string());
        self.ui.set("install.pack", pack_value);
        self.ui.set("install.status", status);
        self.ui.set("install.status_tone", status_tone);
        self.ui.set(
            "install.action",
            if pack.is_some() { "Rebuild" } else { "Install" },
        );
    }

    fn publish_update(&mut self) {
        let published = build_info::is_published();
        let available = self.config.updates.available_version.clone();
        let (check, check_warn) = if !published {
            ("Dev build".to_string(), false)
        } else if self.checking {
            ("Checking...".to_string(), true)
        } else if available.is_some() {
            ("Update available".to_string(), true)
        } else if self.config.updates.last_check > 0 {
            ("Up to date".to_string(), false)
        } else {
            (String::new(), false)
        };
        let (engine, engine_warn) = if !published {
            ("Dev build".to_string(), false)
        } else if let Some(version) = &available {
            (version.clone(), true)
        } else {
            ("Up to date".to_string(), false)
        };
        let games = self
            .games()
            .into_iter()
            .map(|game| {
                let (status, warn) = match pack_for(&self.packs, game) {
                    None => ("Not installed", false),
                    Some(pack) if pack.stale => ("Required", true),
                    Some(_) => ("Up to date", false),
                };
                Value::map([
                    ("label", Value::from(format!("Rebuild {}", game.title()))),
                    ("status", Value::from(status)),
                    ("tone", Value::from(tone(warn))),
                ])
            })
            .collect();
        self.ui.set("update.check", check);
        self.ui.set("update.check_tone", tone(check_warn));
        self.ui.set("update.engine", engine);
        self.ui.set("update.engine_tone", tone(engine_warn));
        self.ui.set("update.games", Value::list(games));
        self.ui.set(
            "update.rollback",
            if self.rollback { "Available" } else { "None" },
        );
        let channel = match self.config.updates.channel {
            UpdateChannel::Release => 0,
            UpdateChannel::Nightly => 1,
        };
        self.ui.set(
            "update.channel",
            option_def(
                "Channel",
                "",
                &["Release".to_string(), "Nightly".to_string()],
                channel,
            ),
        );
        self.ui.set(
            "update.auto_check",
            option_def(
                "Auto-Check",
                "",
                &["Off".to_string(), "On".to_string()],
                usize::from(self.config.updates.check),
            ),
        );
    }

    fn publish_mods(&mut self) {
        let rows = self
            .mods
            .iter()
            .map(|row| {
                Value::map([
                    ("enabled", Value::Bool(row.enabled)),
                    ("name", Value::from(row.name.as_str())),
                    ("game", Value::from(row.game.as_str())),
                    ("id", Value::from(row.id.as_str())),
                    ("version", Value::from(row.version.as_str())),
                    ("path", Value::from(row.path.display().to_string())),
                ])
            })
            .collect();
        let (first, last) = match self.mods.len() {
            0 => ("", ""),
            1 => ("- Only mod -", ""),
            _ => ("- Low priority -", "- High priority -"),
        };
        let dir = paths::mods_dir(&paths::primary_data_dir());
        self.ui.set("mods.rows", Value::list(rows));
        self.ui.set("mods.empty", self.mods.is_empty());
        self.ui.set("mods.first", first);
        self.ui.set("mods.last", last);
        self.ui
            .set("mods.dir", format!("Put mods in {}", dir.display()));
        self.ui.set(
            "mods.move_hint",
            if self.mods.len() > 1 {
                "Move (Down = Higher)"
            } else {
                ""
            },
        );
    }

    fn publish_progress(&mut self) {
        let state = if let Some(job) = &self.job {
            job.state
                .lock()
                .map(|state| state.clone())
                .unwrap_or_default()
        } else if let Some(job) = &self.update_job {
            job.state
                .lock()
                .map(|state| state.clone())
                .unwrap_or_default()
        } else {
            self.capture_progress.clone().unwrap_or_default()
        };
        let rebuild = self.job.as_ref().is_some_and(|job| job.rebuild);
        let game = self.job.as_ref().map_or(self.game, |job| job.game);
        let game_title = game.title().to_uppercase();
        let title = if self.update_job.is_some() {
            "UPDATING ENGINE".to_string()
        } else if rebuild {
            format!("REBUILDING {game_title}")
        } else {
            format!("INSTALLING {game_title}")
        };
        let label = if state.label.is_empty() {
            "Starting".to_string()
        } else {
            capitalize(&state.label)
        };
        let count = if state.total > 0 {
            format!(
                "{} / {}",
                format_count(state.done),
                format_count(state.total)
            )
        } else {
            String::new()
        };
        let overall = state.overall.clamp(0.0, 1.0);
        let step = if state.phases_total > 0 {
            format!("Step {} / {}", state.phases_done, state.phases_total)
        } else {
            String::new()
        };
        let cancelling = self
            .job
            .as_ref()
            .is_some_and(|job| job.cancel.load(Ordering::Relaxed))
            || self
                .update_job
                .as_ref()
                .is_some_and(|job| job.cancel.load(Ordering::Relaxed));
        self.ui.set("progress.title", title);
        self.ui.set("progress.label", label);
        self.ui.set("progress.count", count);
        self.ui
            .set("progress.fraction", (overall * 1000.0).round() as i64);
        self.ui.set(
            "progress.percent",
            format!("{}%", (overall * 100.0).round() as u32),
        );
        self.ui.set("progress.item", state.item);
        self.ui.set(
            "progress.elapsed",
            format!("Elapsed {}", format_elapsed(state.elapsed)),
        );
        self.ui.set("progress.step", step);
        self.ui.set("progress.running", !cancelling);
        self.ui.set(
            "progress.cancel_hint",
            if cancelling {
                "Cancelling..."
            } else {
                "Cancel"
            },
        );
    }

    // ---- input --------------------------------------------------------

    /// Feed one event to the screens and act on what they report. Returns
    /// true when the launcher should close.
    fn handle(&mut self, event: GuiEvent, display: &mut Display) -> bool {
        if self.options.is_some() {
            self.options_events(&[event], display);
            return false;
        }
        self.publish();
        let outputs = self.ui.handle(&[event]);
        let mut close = false;
        for output in outputs {
            match output {
                UiOutput::Action { name, arg, .. } => {
                    close |= self.action(&name, &arg, display);
                }
                UiOutput::Changed { path, value } => self.changed(&path, &value),
                UiOutput::Opened(_) | UiOutput::Closed(_) | UiOutput::Sound(_) => {}
            }
        }
        // The cursor on PLAY is the pack Play starts; the one on MODS is the
        // mod the folder and reorder keys act on.
        match self.screen() {
            Screen::Play => {
                if let Some(index) = self.ui.focus_index() {
                    self.play_index = index;
                }
            }
            Screen::Mods => {
                if let Some(index) = self.ui.focus_index() {
                    self.mods_index = index;
                }
            }
            _ => {}
        }
        self.publish();
        close
    }

    /// Feed the options screen and carry out what it asks.
    fn options_events(&mut self, events: &[GuiEvent], display: &mut Display) {
        let Some(menu) = &mut self.options else {
            return;
        };
        let requests = menu.handle(events);
        self.options_requests(requests, display);
    }

    /// Hand the options screen's key configuration the raw press it waits
    /// for; `click` is where the pointer went down, if it did.
    fn options_capture(&mut self, source: Input, click: Option<(f32, f32)>, display: &mut Display) {
        let Some(menu) = &mut self.options else {
            return;
        };
        // The pad's start button is reserved, but it cancels a capture.
        if source.reserved_bit().is_some() && source != Input::PadButton(SDL_GAMEPAD_BUTTON_START.0)
        {
            return;
        }
        let requests = menu.capture(source, click);
        self.swallow_pads();
        self.options_requests(requests, display);
    }

    /// Whether the options screen's key configuration waits for a key.
    fn capturing(&self) -> bool {
        self.options.as_ref().is_some_and(SystemMenu::capturing)
    }

    fn options_requests(&mut self, requests: Vec<MenuRequest>, display: &mut Display) {
        for request in requests {
            match request {
                MenuRequest::Resume => {
                    self.options = None;
                    self.ui.invalidate();
                }
                MenuRequest::Settings(settings) => self.apply_settings(*settings, display),
                // The options screen offers none of these, and the launcher
                // has no sound.
                MenuRequest::Restart
                | MenuRequest::LoadGame
                | MenuRequest::Quit
                | MenuRequest::Sound(_) => {}
            }
        }
    }

    /// Apply an options change to the launcher's window and look, persist
    /// it as the game does and hand the screen back what took effect. The
    /// renderer settings only take effect when Play hands the window over.
    fn apply_settings(&mut self, settings: Settings, display: &mut Display) {
        let before = self.settings(display);
        let wanted = settings.graphics;
        let old = before.graphics;
        let mut graphics = wanted;
        if wanted.vsync != old.vsync
            && let Err(err) = display.set_vsync(wanted.vsync)
        {
            logging::write(&format!("warning: vsync not applied: {err:#}"));
        }
        if wanted.aspect != old.aspect && !wanted.fullscreen {
            // The window keeps its zoom at the new shape, as in game.
            let times = (old.window_width / 320).max(1);
            graphics.window_width = 320 * times;
            graphics.window_height = wanted.aspect.height() * times;
        }
        if (
            graphics.fullscreen,
            graphics.window_width,
            graphics.window_height,
            graphics.aspect,
        ) != (
            old.fullscreen,
            old.window_width,
            old.window_height,
            old.aspect,
        ) {
            graphics = display.apply_window_options(graphics);
        }
        if wanted.display != old.display {
            display.move_to_display(wanted.display);
            graphics.display = wanted.display;
        }
        if settings.bindings != before.bindings {
            if let Err(err) = Config::save_bindings(&Config::default_path(), &settings.bindings) {
                logging::write(&format!("warning: bindings not saved: {err:#}"));
            }
            self.config.bindings = Some(settings.bindings.clone());
        }
        if settings.custom_pad != before.custom_pad {
            let path = Config::default_path();
            if let Err(err) = Config::save_custom_pad(&path, &settings.custom_pad) {
                logging::write(&format!(
                    "warning: custom controller layout not saved: {err:#}"
                ));
            }
            self.config.custom_pad = Some(settings.custom_pad.clone());
        }
        let (file, _) = Config::load_default();
        let (saved_graphics, saved_language, saved_game) =
            engine::persisted_options(file, &before, &settings, graphics);
        if let Err(err) = Config::save_options(
            &Config::default_path(),
            &saved_graphics,
            saved_language.as_deref(),
            saved_game,
        ) {
            logging::write(&format!("warning: options not saved: {err:#}"));
        }
        self.config.graphics = graphics;
        self.config.game = settings.game;
        self.config.language = settings.language.clone();
        self.apply_look();
        self.ui.invalidate();
        if let Some(menu) = &mut self.options {
            menu.set_settings(Settings {
                graphics,
                ..settings
            });
        }
    }

    /// Carry out an action a screen reported. Returns true to close.
    fn action(&mut self, name: &str, arg: &Value, display: &mut Display) -> bool {
        let index = arg.as_int().unwrap_or(0).max(0) as usize;
        match name {
            "exit" => return true,
            "play_screen" => self.enter(Screen::Play),
            "install_screen" => self.enter(Screen::Games),
            "update_screen" => self.enter(Screen::Update),
            "mods_screen" => self.enter(Screen::Mods),
            "options_screen" => self.open_options(display),
            "play" => {
                self.play_index = index;
                return self.play(display);
            }
            "install_game" => {
                if let Some(&game) = Game::ALL.get(index) {
                    self.select_game(game);
                    self.enter(Screen::Install);
                }
            }
            "install" => self.install(),
            "rebuild" => {
                if let Some(&game) = self.games().get(index) {
                    self.select_game(game);
                    self.install();
                }
            }
            "browse" => self.browse(display),
            "check" => self.start_check(true),
            "update" => self.start_update(),
            "rollback" => self.rollback(),
            "toggle" => {
                self.mods_index = index;
                self.toggle_mod();
            }
            "move" => {
                self.move_mod(arg.as_int().unwrap_or(0) as isize);
                self.publish();
                self.ui.set_focus_index(self.mods_index);
            }
            "folder" => {
                let dir = match self.mods.get(self.mods_index) {
                    Some(row) if row.path.is_dir() => row.path.clone(),
                    Some(row) => row
                        .path
                        .parent()
                        .map(Path::to_path_buf)
                        .unwrap_or_else(|| row.path.clone()),
                    None => paths::mods_dir(&paths::primary_data_dir()),
                };
                open_folder(&dir);
            }
            "cancel_job" => {
                if let Some(job) = &self.job {
                    job.cancel.store(true, Ordering::Relaxed);
                }
                if let Some(job) = &self.update_job {
                    job.cancel.store(true, Ordering::Relaxed);
                }
            }
            other => logging::write(&format!("launcher: unknown action {other:?}")),
        }
        false
    }

    /// Apply a value an update row changed.
    fn changed(&mut self, path: &str, value: &Value) {
        let Some(index) = value.as_int() else {
            return;
        };
        match path {
            "update.channel.value" => {
                let channel = if index == 0 {
                    UpdateChannel::Release
                } else {
                    UpdateChannel::Nightly
                };
                if channel != self.config.updates.channel {
                    self.toggle_channel();
                }
            }
            "update.auto_check.value" if (index != 0) != self.config.updates.check => {
                self.toggle_auto_check();
            }
            _ => {}
        }
    }

    /// Wait at most 100 ms, or less when a held stick is close to repeating.
    fn pad_wait(&self) -> i32 {
        if !self.pad_focused {
            return 100;
        }
        let now = Instant::now();
        let mut wait = 100;
        for next in [self.pad_x.next, self.pad_y.next].into_iter().flatten() {
            let remaining = next.saturating_duration_since(now).as_millis();
            wait = wait.min(remaining.clamp(1, 100) as i32);
        }
        wait
    }

    /// Fold the left stick into navigation steps, and the triggers into the
    /// options' sub-tab paging. The D-pad is handled as button edges, so a
    /// quick tap is never missed by polling. Returns whether a step was
    /// taken, and whether the launcher should close.
    fn poll_pad_navigation(&mut self, display: &mut Display) -> (bool, bool) {
        if !self.pad_focused {
            return (false, false);
        }
        if self.capturing() {
            self.swallow_pads();
            return (false, false);
        }
        let (x, y, lt, rt) = self.pad_readings();
        let now = Instant::now();
        let device = self.pad_device;
        let mut keys = Vec::new();
        // Horizontal changes are one-shot: holding left on a choice must not
        // keep flipping it.
        if let Some(direction) = self.pad_x.poll(x, now, false) {
            keys.push(if direction < 0 {
                GuiKey::Left
            } else {
                GuiKey::Right
            });
        }
        if let Some(direction) = self.pad_y.poll(y, now, true) {
            keys.push(if direction < 0 {
                GuiKey::Up
            } else {
                GuiKey::Down
            });
        }
        if self.pad_lt.poll(lt, now, false) == Some(1) {
            keys.push(GuiKey::SubPrev);
        }
        if self.pad_rt.poll(rt, now, false) == Some(1) {
            keys.push(GuiKey::SubNext);
        }
        let mut close = false;
        for &key in &keys {
            close |= self.handle(GuiEvent::Key(key, device), display);
        }
        (!keys.is_empty(), close)
    }

    /// Let every held pad direction go unanswered until it is released: a
    /// direction bound in the key configuration must not then move its
    /// cursor.
    fn swallow_pads(&mut self) {
        let (x, y, lt, rt) = self.pad_readings();
        self.pad_x.swallow(x);
        self.pad_y.swallow(y);
        self.pad_lt.swallow(lt);
        self.pad_rt.swallow(rt);
    }

    /// The D-pad or left stick across and down, and the two triggers.
    fn pad_readings(&self) -> (i16, i16, i16, i16) {
        let x = if self.pads.button(SDL_GAMEPAD_BUTTON_DPAD_RIGHT) {
            i16::MAX
        } else if self.pads.button(SDL_GAMEPAD_BUTTON_DPAD_LEFT) {
            i16::MIN
        } else {
            self.pads.axis(SDL_GAMEPAD_AXIS_LEFTX)
        };
        let y = if self.pads.button(SDL_GAMEPAD_BUTTON_DPAD_DOWN) {
            i16::MAX
        } else if self.pads.button(SDL_GAMEPAD_BUTTON_DPAD_UP) {
            i16::MIN
        } else {
            self.pads.axis(SDL_GAMEPAD_AXIS_LEFTY)
        };
        let lt = self.pads.axis(SDL_GAMEPAD_AXIS_LEFT_TRIGGER);
        let rt = self.pads.axis(SDL_GAMEPAD_AXIS_RIGHT_TRIGGER);
        (x, y, lt, rt)
    }

    /// Handle one gamepad button from pad `which`. Returns true when the
    /// launcher should close.
    fn gamepad_button(&mut self, button: u8, which: u32, display: &mut Display) -> bool {
        if !self.pad_focused {
            return false;
        }
        self.pad_device = Device::Gamepad(engine::pad_style(which));
        let button = i32::from(button);
        if self.capturing() {
            self.options_capture(Input::PadButton(button), None, display);
            return false;
        }
        let options = self.options.is_some();
        let mut dpad = None;
        let key = match button {
            b if b == SDL_GAMEPAD_BUTTON_DPAD_UP.0 => {
                dpad = Some((false, -1));
                GuiKey::Up
            }
            b if b == SDL_GAMEPAD_BUTTON_DPAD_DOWN.0 => {
                dpad = Some((false, 1));
                GuiKey::Down
            }
            b if b == SDL_GAMEPAD_BUTTON_DPAD_LEFT.0 => {
                dpad = Some((true, -1));
                GuiKey::Left
            }
            b if b == SDL_GAMEPAD_BUTTON_DPAD_RIGHT.0 => {
                dpad = Some((true, 1));
                GuiKey::Right
            }
            b if b == SDL_GAMEPAD_BUTTON_SOUTH.0 => GuiKey::Confirm,
            b if b == SDL_GAMEPAD_BUTTON_EAST.0 => GuiKey::Cancel,
            // In the options the face and system buttons are the game's.
            b if b == SDL_GAMEPAD_BUTTON_START.0 && options => GuiKey::Cancel,
            b if b == SDL_GAMEPAD_BUTTON_BACK.0 && options => GuiKey::ResetAll,
            b if b == SDL_GAMEPAD_BUTTON_NORTH.0 && options => GuiKey::ResetGroup,
            b if b == SDL_GAMEPAD_BUTTON_START.0 => GuiKey::Confirm,
            b if b == SDL_GAMEPAD_BUTTON_BACK.0 => GuiKey::Cancel,
            b if b == SDL_GAMEPAD_BUTTON_WEST.0 => GuiKey::Aux,
            b if b == SDL_GAMEPAD_BUTTON_LEFT_SHOULDER.0 => GuiKey::TabPrev,
            b if b == SDL_GAMEPAD_BUTTON_RIGHT_SHOULDER.0 => GuiKey::TabNext,
            _ => return false,
        };
        let closed = self.handle(GuiEvent::Key(key, self.pad_device), display);
        if let Some((horizontal, direction)) = dpad {
            let state = if horizontal {
                &mut self.pad_x
            } else {
                &mut self.pad_y
            };
            state.direction = direction;
            // Horizontal directions stay one-shot; vertical ones repeat.
            state.next = (!horizontal).then(|| Instant::now() + PAD_INITIAL_DELAY);
        }
        closed
    }

    /// The navigation key a keyboard key is on the current screen. The
    /// options screen takes the game's menu keys.
    fn keyboard_key(&self, scancode: SDL_Scancode, modifiers: SDL_Keymod) -> Option<GuiKey> {
        let ctrl = modifiers & SDL_KMOD_CTRL != 0;
        let options = self.options.is_some();
        Some(match scancode {
            SDL_SCANCODE_ESCAPE => GuiKey::Cancel,
            SDL_SCANCODE_RETURN | SDL_SCANCODE_KP_ENTER | SDL_SCANCODE_SPACE => GuiKey::Confirm,
            SDL_SCANCODE_UP | SDL_SCANCODE_W if ctrl => GuiKey::TabPrev,
            SDL_SCANCODE_DOWN | SDL_SCANCODE_S if ctrl => GuiKey::TabNext,
            SDL_SCANCODE_UP | SDL_SCANCODE_W => GuiKey::Up,
            SDL_SCANCODE_DOWN | SDL_SCANCODE_S => GuiKey::Down,
            SDL_SCANCODE_LEFT | SDL_SCANCODE_A => GuiKey::Left,
            SDL_SCANCODE_RIGHT | SDL_SCANCODE_D => GuiKey::Right,
            SDL_SCANCODE_Q if options => GuiKey::TabPrev,
            SDL_SCANCODE_E if options => GuiKey::TabNext,
            SDL_SCANCODE_1 if options => GuiKey::SubPrev,
            SDL_SCANCODE_3 if options => GuiKey::SubNext,
            SDL_SCANCODE_TAB if options => GuiKey::ResetAll,
            SDL_SCANCODE_R if options => GuiKey::ResetGroup,
            SDL_SCANCODE_EQUALS => GuiKey::TabPrev,
            SDL_SCANCODE_MINUS => GuiKey::TabNext,
            SDL_SCANCODE_B if !options && self.screen() == Screen::Install => GuiKey::Aux,
            SDL_SCANCODE_O if !options && self.screen() == Screen::Mods => GuiKey::Aux,
            _ => return None,
        })
    }

    // ---- window and updates ------------------------------------------

    /// Copy the window values the display currently has into the config, so
    /// the options and Play start from the real size.
    fn sync_window(&mut self, display: &Display) {
        if let Some(current) = display.graphics() {
            self.config.graphics.fullscreen = current.fullscreen;
            self.config.graphics.display = current.display;
            self.config.graphics.window_width = current.window_width;
            self.config.graphics.window_height = current.window_height;
        }
    }

    /// Check on startup when enabled and the six-hour cache has expired.
    fn maybe_start_check(&mut self) {
        if !build_info::is_published()
            || !self.config.updates.check
            || self.checking
            || self.config.updates.last_check.saturating_add(6 * 60 * 60) > unix_now()
        {
            return;
        }
        self.start_check(false);
    }

    fn start_check(&mut self, manual: bool) {
        if self.checking {
            return;
        }
        if !build_info::is_published() {
            if manual {
                self.show_message(
                    "CHECK FOR UPDATES",
                    &["Development builds do not update themselves."],
                    Screen::Update,
                );
            }
            return;
        }
        let channel = self.config.updates.channel;
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(update::check(channel).map_err(|err| format!("{err:#}")));
        });
        self.check_rx = Some(receiver);
        self.checking = true;
    }

    fn poll_check(&mut self) -> bool {
        let Some(receiver) = &self.check_rx else {
            return false;
        };
        let received = match receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return false,
            Err(TryRecvError::Disconnected) => Err("the update check stopped".to_string()),
        };
        self.check_rx = None;
        self.checking = false;
        self.config.updates.last_check = unix_now();
        match received {
            Ok(Some(info)) => {
                self.config.updates.available_channel = UpdateChannel::parse(&info.channel);
                self.config.updates.available_version = Some(info.version);
                self.config.updates.available_sha = Some(info.sha);
            }
            Ok(None) => {
                self.config.updates.available_channel = None;
                self.config.updates.available_version = None;
                self.config.updates.available_sha = None;
            }
            Err(err) => {
                logging::write(&format!("update check failed: {err}"));
            }
        }
        self.save_updates();
        true
    }

    fn clear_available_if_current(&mut self) {
        let updates = &self.config.updates;
        let Some(sha) = updates.available_sha.as_deref() else {
            return;
        };
        let current = match updates.available_channel {
            Some(UpdateChannel::Nightly) => !sha.is_empty() && sha == build_info::COMMIT,
            _ => updates.available_version.as_deref() == Some(build_info::VERSION),
        };
        if current {
            self.config.updates.available_channel = None;
            self.config.updates.available_version = None;
            self.config.updates.available_sha = None;
            self.save_updates();
        }
    }

    fn save_updates(&self) {
        if let Err(err) = Config::save_updates(&Config::default_path(), &self.config.updates) {
            logging::write(&format!("warning: update settings not saved: {err:#}"));
        }
    }

    fn toggle_channel(&mut self) {
        self.config.updates.channel = match self.config.updates.channel {
            UpdateChannel::Release => UpdateChannel::Nightly,
            UpdateChannel::Nightly => UpdateChannel::Release,
        };
        self.config.updates.available_channel = None;
        self.config.updates.available_version = None;
        self.config.updates.available_sha = None;
        self.save_updates();
        self.start_check(true);
    }

    fn toggle_auto_check(&mut self) {
        self.config.updates.check = !self.config.updates.check;
        self.save_updates();
    }

    fn start_update(&mut self) {
        if !build_info::is_published() {
            self.show_message(
                "UPDATE ENGINE",
                &["Development builds do not update themselves."],
                Screen::Update,
            );
            return;
        }
        let (Some(version), Some(sha)) = (
            self.config.updates.available_version.clone(),
            self.config.updates.available_sha.clone(),
        ) else {
            self.show_message(
                "UPDATE ENGINE",
                &["There is no newer published build."],
                Screen::Update,
            );
            return;
        };
        let channel = self.config.updates.channel;
        // The feed is re-fetched so the download URL and checksum come from
        // the same JSON as the offered version.
        let (sender, receiver) = mpsc::channel();
        let state = Arc::new(Mutex::new(ProgressState {
            label: "download".to_string(),
            item: "arklay".to_string(),
            ..ProgressState::default()
        }));
        let cancel = Arc::new(AtomicBool::new(false));
        let thread_state = Arc::clone(&state);
        let thread_cancel = Arc::clone(&cancel);
        std::thread::spawn(move || {
            let result = (|| -> Result<()> {
                let Some(info) = update::check(channel)? else {
                    bail!("there is no newer published build");
                };
                if info.version != version || info.sha != sha {
                    bail!("the update feed changed; check for updates again");
                }
                let exe = update::current_exe()?;
                let dest = update::staging_path(&exe);
                let start = std::time::Instant::now();
                update::download(&info, &dest, &thread_cancel, |done, total| {
                    if let Ok(mut state) = thread_state.lock() {
                        state.done = done;
                        state.total = total;
                        state.overall = if total > 0 {
                            done.min(total) as f64 / total as f64
                        } else {
                            0.0
                        };
                        state.elapsed = start.elapsed();
                    }
                })
            })();
            let _ = sender.send(result.map_err(|err| format!("{err:#}")));
        });
        self.update_job = Some(UpdateJob {
            receiver,
            cancel,
            state,
        });
        self.job = None;
        self.publish_screen(Screen::Progress);
        self.open(Screen::Progress);
    }

    fn poll_update_job(&mut self) -> bool {
        let Some(job) = &self.update_job else {
            return false;
        };
        let result = match job.receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return false,
            Err(TryRecvError::Disconnected) => Err("the update download stopped".to_string()),
        };
        let job = self.update_job.take().expect("an update job");
        if job.cancel.load(Ordering::Relaxed) {
            logging::write("update cancelled");
            self.show_message(
                "UPDATE CANCELLED",
                &["The engine was not changed."],
                Screen::Update,
            );
            return true;
        }
        match result {
            Ok(()) => {
                logging::write("update: installing the downloaded engine");
                if let Err(err) = update::install_staged() {
                    self.show_message("UPDATE FAILED", &[&format!("{err:#}")], Screen::Update);
                    return true;
                }
                if let Err(err) = update::restart() {
                    self.show_message(
                        "RESTART FAILED",
                        &[&format!("{err:#}"), "Restart Arklay manually."],
                        Screen::Update,
                    );
                    return true;
                }
                std::process::exit(0);
            }
            Err(err) => {
                logging::write(&format!("update failed: {err}"));
                self.show_message(
                    "UPDATE FAILED",
                    &[&err, "See the log for details."],
                    Screen::Update,
                );
            }
        }
        true
    }

    fn rollback(&mut self) {
        if !update::rollback_available() {
            self.show_message(
                "ROLL BACK LAST UPDATE",
                &["There is no previous build to restore."],
                Screen::Update,
            );
            return;
        }
        match update::rollback() {
            Ok(()) => {
                logging::write("update: rolling back");
                if let Err(err) = update::restart() {
                    self.show_message(
                        "RESTART FAILED",
                        &[&format!("{err:#}"), "Restart Arklay manually."],
                        Screen::Update,
                    );
                    return;
                }
                std::process::exit(0);
            }
            Err(err) => {
                self.show_message("ROLL BACK FAILED", &[&format!("{err:#}")], Screen::Update)
            }
        }
    }

    fn refresh_mods(&mut self) {
        self.mods = scan_mods(&self.packs, &self.config, &Config::mod_dirs());
        if self.mods_index >= self.mods.len() {
            self.mods_index = self.mods.len().saturating_sub(1);
        }
    }

    fn toggle_mod(&mut self) {
        let Some((id, enabled)) = self
            .mods
            .get(self.mods_index)
            .map(|row| (row.id.clone(), !row.enabled))
        else {
            return;
        };
        if let Some(row) = self.mods.get_mut(self.mods_index) {
            row.enabled = enabled;
        }
        set_mod_enabled(&mut self.config.mods, &id, enabled);
        self.save_mods();
    }

    fn move_mod(&mut self, delta: isize) {
        let len = self.mods.len();
        if len < 2 {
            return;
        }
        let target = (self.mods_index as isize + delta).rem_euclid(len as isize) as usize;
        self.mods.swap(self.mods_index, target);
        self.mods_index = target;
        let shown: Vec<String> = self.mods.iter().map(|row| row.id.clone()).collect();
        self.config.mods.order = merge_order(&self.config.mods.order, &shown);
        self.save_mods();
    }

    fn save_mods(&self) {
        if let Err(err) = Config::save_mods(&Config::default_path(), &self.config.mods) {
            logging::write(&format!("warning: mod selection not saved: {err:#}"));
        }
    }

    /// Start (or restart) the conversion for the current source and target.
    fn install(&mut self) {
        if self.job.is_some() {
            return;
        }
        let game = self.game;
        let title = game.title().to_uppercase();
        let Some(source) = self.source().cloned() else {
            self.show_message(
                &format!("{title} NOT FOUND"),
                &["Choose the game folder with Change Source."],
                Screen::Install,
            );
            return;
        };
        let path = match resolve_source(&source.path, game) {
            Ok(path) => path,
            Err(err) => {
                self.show_message(
                    &format!("NOT {title}"),
                    &[&format!("{err:#}")],
                    if self.screen() == Screen::Update {
                        Screen::Update
                    } else {
                        Screen::Install
                    },
                );
                return;
            }
        };
        let pack = self.pack().cloned();
        let target = self.target();
        if let Some(parent) = target.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let state = Arc::new(Mutex::new(ProgressState::default()));
        let cancel = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = mpsc::channel();
        let thread_state = Arc::clone(&state);
        let thread_cancel = Arc::clone(&cancel);
        let thread_source = path.clone();
        let thread_target = target.clone();
        std::thread::spawn(move || {
            let observer = Box::new(move |snapshot: ProgressSnapshot| {
                if let Ok(mut state) = thread_state.lock() {
                    *state = ProgressState::from(snapshot);
                }
            });
            let result = convert::convert_game_observed(
                &thread_source,
                &thread_target,
                game,
                0,
                false,
                Some(observer),
                Some(thread_cancel),
            );
            let _ = sender.send(result.map_err(|err| format!("{err:#}")));
        });
        logging::write(&format!(
            "install {}: {} -> {}",
            game.id(),
            path.display(),
            target.display()
        ));
        self.sources.insert(
            game,
            Source {
                path: path.clone(),
                build: source.build.clone(),
            },
        );
        self.job = Some(Job {
            receiver,
            cancel,
            state,
            game,
            source: path,
            build: source.build,
            target,
            rebuild: pack.is_some(),
            back: if self.screen() == Screen::Update {
                Screen::Update
            } else {
                Screen::Install
            },
        });
        self.publish_screen(Screen::Progress);
        self.open(Screen::Progress);
    }

    /// Hand the launcher's window to the game. Returning true means the
    /// process should now exit, which is what closing the game window does.
    fn play(&mut self, display: &mut Display) -> bool {
        let Some(pack) = self.packs.get(self.play_index).cloned() else {
            return false;
        };
        if let Some(error) = &pack.error {
            self.show_message("PACK ERROR", &[error], Screen::Play);
            return false;
        }
        // Hand the pads to the game's input latch; it opens its own handles.
        self.pads = Gamepads::default();
        display.note_window_size();
        let mut graphics = self.config.graphics;
        if let Some(current) = display.graphics().cloned() {
            graphics.fullscreen = current.fullscreen;
            graphics.display = current.display;
            graphics.window_width = current.window_width;
            graphics.window_height = current.window_height;
        }
        video::set_aspect(graphics.aspect);
        if let Err(err) = display.set_renderer(graphics) {
            self.pads.init();
            self.show_message("GAME FAILED", &[&format!("{err:#}")], Screen::Play);
            return false;
        }
        let save_dir = self
            .config
            .save_dir
            .clone()
            .unwrap_or_else(Config::default_save_dir);
        let mods = ModOptions {
            explicit: Vec::new(),
            no_mods: false,
            layers: crate::pack::LayerOptions {
                enabled: self.config.mods.enabled.clone(),
                disabled: self.config.mods.disabled.clone(),
                order: self.config.mods.order.clone(),
            },
            global_dirs: Config::mod_dirs(),
        };
        let player_options = player::Options {
            quick_turn: self.config.game.quick_turn,
            tactical_reload: self.config.game.tactical_reload,
            analog_movement: self.config.game.analog_movement,
            no_auto_aim: !self.config.game.auto_aim,
            no_blood: !self.config.game.blood,
            no_chris_ink_ribbons: !self.config.game.chris_ink_ribbons,
            jill_ink_ribbons: self.config.game.jill_ink_ribbons,
            no_autosave: !self.config.game.autosave,
            no_room_script_fixes: !self.config.game.room_script_fixes,
            // Only the command line runs in authentic mode.
            authentic: false,
        };
        logging::write(&format!("play: {}", pack.path.display()));
        let result = engine::run_root_with_display(
            &pack.path,
            None,
            &save_dir,
            &mods,
            false,
            self.config.language.as_deref(),
            graphics,
            player_options,
            display,
        );
        match result {
            Ok(()) => true,
            Err(err) => {
                logging::write(&format!("game failed: {err:#}"));
                self.pads.init();
                let mut software = graphics;
                software.renderer = RendererKind::Software;
                let _ = display.set_renderer(software);
                let _ = display.set_title(WINDOW_TITLE);
                self.show_message("GAME FAILED", &[&format!("{err:#}")], Screen::Play);
                false
            }
        }
    }

    /// Open the native folder picker for a manually installed copy.
    fn browse(&mut self, display: &Display) {
        if self.dialog.is_some() {
            return;
        }
        let default = self
            .source()
            .and_then(|source| CString::new(source.path.to_string_lossy().as_bytes()).ok());
        let state = Arc::new(DialogState {
            done: AtomicBool::new(false),
            path: Mutex::new(None),
            _default: default,
        });
        let userdata = Arc::as_ptr(&state).cast_mut().cast::<c_void>();
        unsafe {
            SDL_ShowOpenFolderDialog(
                Some(folder_callback),
                userdata,
                display.window_ptr(),
                state
                    ._default
                    .as_ref()
                    .map_or(std::ptr::null(), |path| path.as_ptr()),
                false,
            );
        }
        self.dialog = Some(state);
        logging::write("folder picker opened");
    }

    /// Finish a folder-dialog invocation and adopt a validated choice.
    fn poll_dialog(&mut self) -> bool {
        let Some(dialog) = &self.dialog else {
            return false;
        };
        if !dialog.done.load(Ordering::Relaxed) {
            return false;
        }
        let path = dialog.path.lock().ok().and_then(|mut path| path.take());
        self.dialog = None;
        let Some(path) = path else {
            return true;
        };
        // A folder holding the other game is adopted for that game and
        // selects it, so RE2 can be set up by picking its folder.
        let others = Game::ALL.into_iter().filter(|&game| game != self.game);
        let found = std::iter::once(self.game)
            .chain(others)
            .find_map(|game| resolve_source(&path, game).ok().map(|path| (game, path)));
        match found {
            Some((game, path)) => {
                logging::write(&format!("source {}: {}", game.id(), path.display()));
                self.sources.insert(game, Source { path, build: None });
                self.select_game(game);
                self.ui.invalidate();
            }
            None => {
                let err = resolve_source(&path, self.game)
                    .err()
                    .unwrap_or_else(|| anyhow!("not a game installation"));
                logging::write(&format!("source rejected: {err:#}"));
                let title = format!("NOT {}", self.game.title().to_uppercase());
                self.show_message(&title, &[&format!("{err:#}")], Screen::Install);
            }
        }
        true
    }

    /// Collect a finished conversion.
    fn poll_job(&mut self) -> bool {
        let Some(job) = &self.job else {
            return false;
        };
        let received = match job.receiver.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => return false,
            Err(TryRecvError::Disconnected) => {
                Some(Err("the conversion thread stopped unexpectedly".to_string()))
            }
        };
        let Some(result) = received else {
            return false;
        };
        let job = self.job.take().expect("a running job");
        self.game = job.game;
        let back = job.back;
        let cancelled = job.cancel.load(Ordering::Relaxed);
        match result {
            Ok(()) => {
                if let Err(err) = Config::save_games(
                    &Config::default_path(),
                    job.game,
                    Some(&job.source),
                    job.build.as_deref(),
                ) {
                    logging::write(&format!("warning: game source not saved: {err:#}"));
                }
                self.sources.insert(
                    job.game,
                    Source {
                        path: job.source.clone(),
                        build: job.build.clone(),
                    },
                );
                self.packs = scan_packs();
                self.game = job.game;
                self.refresh_mods();
                self.goto(Screen::Main);
                self.ui.set_focus_index(0);
                let title = if job.rebuild {
                    "PACK REBUILT"
                } else {
                    "PACK READY"
                };
                self.show_message(
                    title,
                    &[
                        &job.target.display().to_string(),
                        "The Play screen lists it.",
                    ],
                    Screen::Main,
                );
            }
            Err(_) if cancelled => {
                logging::write("install cancelled");
                self.show_message("INSTALL CANCELLED", &["No pack was written."], back);
            }
            Err(err) => {
                logging::write(&format!("install failed: {err}"));
                self.show_message("INSTALL FAILED", &[&err, "See the log for details."], back);
            }
        }
        true
    }

    /// Pick a screen for a headless preview capture.
    fn set_capture_screen(&mut self, name: &str, display: &mut Display) {
        let screen = match name {
            "play" => Screen::Play,
            "games" => Screen::Games,
            "install" => Screen::Install,
            "update" => Screen::Update,
            "mods" => Screen::Mods,
            "options" => {
                self.open_options(display);
                return;
            }
            "progress" => {
                self.capture_progress = Some(ProgressState {
                    label: "roomcut".to_string(),
                    item: "roomcut/600_123.bmp".to_string(),
                    done: 1204,
                    total: 2058,
                    phases_done: 6,
                    phases_total: 15,
                    overall: 0.58,
                    elapsed: Duration::from_secs(42),
                });
                Screen::Progress
            }
            "message" => {
                self.show_message("PREVIEW", &["A sample message screen."], Screen::Main);
                return;
            }
            _ => Screen::Main,
        };
        self.goto(screen);
    }

    /// Paint the screens at `width` x `height` and make the canvas opaque
    /// for the window; returns whether anything was repainted.
    fn draw(&mut self, width: u32, height: u32) -> bool {
        self.publish();
        let painted = match &mut self.options {
            Some(menu) => menu.draw(width, height),
            None => self.ui.draw(width, height),
        };
        if !painted && !self.rgba.is_empty() {
            return false;
        }
        // The canvas is premultiplied; over black that is its colour as is.
        let canvas: &Canvas = match &self.options {
            Some(menu) => menu.canvas(),
            None => self.ui.canvas(),
        };
        self.size = (canvas.width, canvas.height);
        let pixels = canvas.pixels();
        self.rgba.clear();
        self.rgba.extend_from_slice(&pixels);
        for pixel in self.rgba.as_chunks_mut::<4>().0 {
            pixel[3] = 255;
        }
        true
    }
}

impl Launcher {
    fn run_loop(&mut self, display: &mut Display) -> Result<()> {
        let mut present = true;
        let mut event = SDL_Event::default();
        loop {
            let dialog_changed = self.poll_dialog();
            let job_changed = self.poll_job();
            let update_changed = self.poll_update_job();
            let check_changed = self.poll_check();
            if self.auto_update
                && !self.checking
                && self.update_job.is_none()
                && self.job.is_none()
                && self.config.updates.available_version.is_some()
            {
                self.auto_update = false;
                self.start_update();
            }
            if dialog_changed || job_changed || update_changed || check_changed {
                self.ui.invalidate();
            }
            let (width, height) = display.drawable_size();
            if width > 0 && height > 0 && (self.draw(width, height) || present) {
                display.present_ui(self.size.0, self.size.1, &self.rgba)?;
                display.show()?;
                present = false;
            }
            if !unsafe { SDL_WaitEventTimeout(&mut event, self.pad_wait()) } {
                let (_, close) = self.poll_pad_navigation(display);
                if close {
                    self.save_window(display);
                    return Ok(());
                }
                continue;
            }
            loop {
                let kind = unsafe { event.r#type };
                let mut close = false;
                if kind == SDL_EVENT_QUIT {
                    // A conversion's temporary pack is only removed by the
                    // worker; give it a moment to see the cancel flag before
                    // the process exits.
                    if let Some(job) = &self.job {
                        job.cancel.store(true, Ordering::Relaxed);
                    }
                    if let Some(job) = &self.update_job {
                        job.cancel.store(true, Ordering::Relaxed);
                    }
                    let deadline = std::time::Instant::now() + Duration::from_secs(3);
                    while self.job.is_some() && std::time::Instant::now() < deadline {
                        self.poll_job();
                        std::thread::sleep(Duration::from_millis(20));
                    }
                    self.save_window(display);
                    return Ok(());
                } else if kind == SDL_EVENT_KEY_DOWN {
                    let scancode = unsafe { event.key.scancode };
                    let modifiers = unsafe { event.key.r#mod };
                    let repeat = unsafe { event.key.repeat };
                    if self.capturing() {
                        if !repeat {
                            self.options_capture(Input::key(scancode), None, display);
                        }
                    } else if repeat {
                        // The options repeat a held direction, as in game.
                        let key = self.keyboard_key(scancode, modifiers);
                        if self.options.is_some()
                            && let Some(
                                key @ (GuiKey::Up | GuiKey::Down | GuiKey::Left | GuiKey::Right),
                            ) = key
                        {
                            self.handle(GuiEvent::Key(key, Device::Keyboard), display);
                        }
                    } else {
                        if modifiers & SDL_KMOD_ALT != 0
                            && matches!(scancode, SDL_SCANCODE_RETURN | SDL_SCANCODE_KP_ENTER)
                        {
                            display.toggle_fullscreen();
                        } else if let Some(key) = self.keyboard_key(scancode, modifiers) {
                            close = self.handle(GuiEvent::Key(key, Device::Keyboard), display);
                        }
                    }
                } else if kind == SDL_EVENT_GAMEPAD_ADDED {
                    self.pads.open(unsafe { event.gdevice.which });
                } else if kind == SDL_EVENT_GAMEPAD_REMOVED {
                    self.pads.close(unsafe { event.gdevice.which });
                } else if kind == SDL_EVENT_GAMEPAD_AXIS_MOTION {
                    // Only the key configuration takes a stick or trigger;
                    // navigation polls them.
                    let axis = i32::from(unsafe { event.gaxis.axis });
                    let value = unsafe { event.gaxis.value };
                    let which = unsafe { event.gaxis.which }.0;
                    let state = self.axes.get(&(which, axis)).copied().unwrap_or(0);
                    let next = if value >= PAD_DEADZONE || (state > 0 && value >= PAD_RELEASE) {
                        1
                    } else if value <= -PAD_DEADZONE || (state < 0 && value <= -PAD_RELEASE) {
                        -1
                    } else {
                        0
                    };
                    self.axes.insert((which, axis), next);
                    if next != state && next != 0 && self.capturing() {
                        self.options_capture(Input::PadAxis(axis, next > 0), None, display);
                    }
                } else if kind == SDL_EVENT_GAMEPAD_BUTTON_DOWN {
                    let button = unsafe { event.gbutton.button };
                    let which = unsafe { event.gbutton.which };
                    close = self.gamepad_button(button, which.0, display);
                } else if kind == SDL_EVENT_MOUSE_BUTTON_DOWN || kind == SDL_EVENT_MOUSE_BUTTON_UP {
                    let button = unsafe { event.button.button };
                    let (x, y) = pointer(display, unsafe { event.button.x }, unsafe {
                        event.button.y
                    });
                    if self.capturing() {
                        if kind == SDL_EVENT_MOUSE_BUTTON_DOWN {
                            self.options_capture(Input::Mouse(button), Some((x, y)), display);
                        }
                    } else if button == SDL_BUTTON_LEFT as u8 {
                        let event = if kind == SDL_EVENT_MOUSE_BUTTON_DOWN {
                            GuiEvent::PointerDown { x, y }
                        } else {
                            GuiEvent::PointerUp { x, y }
                        };
                        close = self.handle(event, display);
                    }
                } else if kind == SDL_EVENT_MOUSE_MOTION {
                    let (x, y) = pointer(display, unsafe { event.motion.x }, unsafe {
                        event.motion.y
                    });
                    close = self.handle(GuiEvent::PointerMove { x, y }, display);
                } else if kind == SDL_EVENT_MOUSE_WHEEL {
                    let wheel = unsafe { event.wheel };
                    let notches = wheel.y;
                    if self.capturing() {
                        // Whole notches, the right way up, as the game reads
                        // the wheel.
                        let sign = if wheel.direction == SDL_MOUSEWHEEL_FLIPPED {
                            -1
                        } else {
                            1
                        };
                        let (x, y) = (wheel.integer_x * sign, wheel.integer_y * sign);
                        let notch =
                            [
                                (y, controls::Wheel::Up, controls::Wheel::Down),
                                (x, controls::Wheel::Right, controls::Wheel::Left),
                            ]
                            .into_iter()
                            .find_map(|(delta, positive, negative)| match delta.signum() {
                                1 => Some(positive),
                                -1 => Some(negative),
                                _ => None,
                            });
                        if let Some(notch) = notch {
                            self.options_capture(Input::Wheel(notch), None, display);
                        }
                    } else {
                        close = self.handle(GuiEvent::Wheel(notches), display);
                    }
                } else if kind == SDL_EVENT_WINDOW_FOCUS_LOST {
                    // Do not keep polling or repeating a direction while
                    // another window owns the input.
                    self.pad_focused = false;
                    self.pad_x = PadAxis::default();
                    self.pad_y = PadAxis::default();
                    self.pad_lt = PadAxis::default();
                    self.pad_rt = PadAxis::default();
                } else if kind == SDL_EVENT_WINDOW_FOCUS_GAINED {
                    self.pad_focused = true;
                    self.pad_x = PadAxis::default();
                    self.pad_y = PadAxis::default();
                    self.pad_lt = PadAxis::default();
                    self.pad_rt = PadAxis::default();
                } else if kind == SDL_EVENT_WINDOW_RESIZED
                    || kind == SDL_EVENT_WINDOW_PIXEL_SIZE_CHANGED
                    || kind == SDL_EVENT_WINDOW_EXPOSED
                {
                    if kind == SDL_EVENT_WINDOW_RESIZED {
                        display.note_window_size();
                        self.sync_window(display);
                    }
                    self.ui.invalidate();
                    if let Some(menu) = &mut self.options {
                        menu.set_settings(Settings {
                            graphics: self.config.graphics,
                            ..menu.settings().clone()
                        });
                    }
                    present = true;
                }
                if close {
                    self.save_window(display);
                    return Ok(());
                }
                if !unsafe { SDL_PollEvent(&mut event) } {
                    break;
                }
            }
            let (_, close) = self.poll_pad_navigation(display);
            if close {
                self.save_window(display);
                return Ok(());
            }
        }
    }

    fn save_window(&self, display: &mut Display) {
        display.note_window_size();
        display.save_window();
    }
}

/// A pointer position in canvas pixels. Mouse events are in window
/// coordinates; the canvas is in drawable pixels (they differ under a
/// display scale above 100%).
fn pointer(display: &Display, x: f32, y: f32) -> (f32, f32) {
    let (window_w, window_h) = display.window_size();
    let (pixel_w, pixel_h) = display.drawable_size();
    let scale_x = pixel_w as f32 / window_w.max(1) as f32;
    let scale_y = pixel_h as f32 / window_h.max(1) as f32;
    (x * scale_x, y * scale_y)
}

/// The window-size presets matching `aspect`'s frame shape.
pub(crate) fn resolution_presets(aspect: video::Aspect) -> &'static [(u32, u32)] {
    if aspect.is_standard() {
        &RESOLUTIONS_STANDARD
    } else {
        &RESOLUTIONS_WIDE
    }
}

/// `SYSTEM`, then every shipped language tag in a stable, familiar order.
pub(crate) fn language_choices() -> Vec<Option<String>> {
    let mut tags: Vec<String> = crate::language::LANGUAGE_FILES
        .iter()
        .filter_map(|(entry, _)| {
            entry
                .strip_prefix("language/")?
                .strip_suffix(".txt")
                .map(str::to_string)
        })
        .collect();
    tags.sort_by(|a, b| {
        crate::language::menu_rank(a)
            .cmp(&crate::language::menu_rank(b))
            .then_with(|| a.cmp(b))
    });
    tags.dedup();
    let mut choices = Vec::with_capacity(tags.len() + 1);
    choices.push(None);
    choices.extend(tags.into_iter().map(Some));
    choices
}

/// `text` with its first letter in capitals, for a progress label.
fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn scan_packs() -> Vec<PackRow> {
    let roots = paths::data_roots();
    let custom = env::var_os(paths::DATA_ENV).is_some();
    let source_hash = convert::packed_source_hash();
    let mut seen: HashSet<String> = HashSet::new();
    let mut packs = Vec::new();
    for (index, root) in roots.iter().enumerate() {
        let dir = paths::games_dir(root);
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        let mut files: Vec<PathBuf> = entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_file()
                    && path
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("akpak"))
            })
            .collect();
        files.sort();
        for path in files {
            let stem = path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default();
            let read = pack::read_manifest(&path);
            let (manifest, error) = match read {
                Ok(manifest) => (manifest, None),
                Err(err) => (None, Some(format!("{err:#}"))),
            };
            let id = manifest
                .as_ref()
                .map(|manifest| manifest.id.clone())
                .unwrap_or_else(|| stem.clone());
            if !seen.insert(id.to_ascii_lowercase()) {
                continue;
            }
            let name = manifest
                .as_ref()
                .and_then(|manifest| manifest.name.clone())
                .unwrap_or_else(|| stem.clone());
            let version = manifest
                .as_ref()
                .and_then(|manifest| manifest.version.clone())
                .unwrap_or_default();
            let stale = manifest
                .as_ref()
                .is_none_or(|manifest| is_stale(manifest, &source_hash));
            packs.push(PackRow {
                id,
                name,
                file: path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                version,
                source: root_label(index, custom),
                path,
                stale,
                error,
            });
        }
    }
    packs.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
            .then_with(|| a.file.cmp(&b.file))
    });
    packs
}

/// Whether a pack needs a rebuild to match this engine. RE1 packs carry
/// the engine's own scripts and language sheets, fingerprinted by
/// `source_hash`; an RE2 pack packs none of them, so only its conversion
/// revision is compared.
fn is_stale(manifest: &manifest::Manifest, source_hash: &str) -> bool {
    if manifest.convert_version != Some(CONVERT_VERSION) {
        return true;
    }
    Game::from_id(&manifest.id) != Some(Game::Re2)
        && manifest.source_hash.as_deref() != Some(source_hash)
}

/// Discover every mod beside any pack or in `dirs`, whatever its game,
/// ordered by the config's explicit order then each manifest's load order.
fn scan_mods(packs: &[PackRow], config: &Config, dirs: &[PathBuf]) -> Vec<ModRow> {
    // A pack's own `mods` folder is searched; with no pack, the folder a
    // first install writes to.
    let mut roots: Vec<PathBuf> = packs.iter().map(|pack| pack.path.clone()).collect();
    if roots.is_empty() {
        roots.push(default_target(Game::Re1));
    }
    let mut seen = HashSet::new();
    let mut paths = Vec::new();
    for root in &roots {
        for path in modding::discover_mods(root, dirs) {
            let canonical = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            if seen.insert(canonical) {
                paths.push(path);
            }
        }
    }
    let mut ids = HashSet::new();
    let mut rows = Vec::new();
    for path in paths {
        let manifest = match read_mod_manifest(&path) {
            Ok(Some(manifest)) => manifest,
            Ok(None) => continue,
            Err(err) => {
                logging::write(&format!("warning: mod {}: {err:#}", path.display()));
                continue;
            }
        };
        if manifest.kind != manifest::PackKind::Mod {
            continue;
        }
        if !ids.insert(manifest.id.to_ascii_lowercase()) {
            continue;
        }
        let base = manifest.base.unwrap_or_default();
        let game = Game::from_id(&base).map_or(base, |game| game.title().to_string());
        rows.push(ModRow {
            enabled: mod_enabled(&config.mods, &manifest.id),
            name: manifest.name.unwrap_or_else(|| manifest.id.clone()),
            game,
            version: manifest.version.unwrap_or_default(),
            load_order: manifest.load_order,
            id: manifest.id,
            path,
        });
    }
    rows.sort_by(|a, b| {
        let rank = |id: &str| {
            config
                .mods
                .order
                .iter()
                .position(|ordered| ordered == id)
                .unwrap_or(usize::MAX)
        };
        rank(&a.id)
            .cmp(&rank(&b.id))
            .then_with(|| a.load_order.cmp(&b.load_order))
            .then_with(|| a.id.cmp(&b.id))
    });
    rows
}

/// The `[mods]` order after reordering the `shown` mods, with entries for
/// mods not found this run kept where they were. The shown mods fill the
/// slots they already held, in their new order; any not listed before are
/// appended.
fn merge_order(order: &[String], shown: &[String]) -> Vec<String> {
    let mut shown_iter = shown.iter();
    let mut merged: Vec<String> = order
        .iter()
        .filter_map(|id| {
            if shown.contains(id) {
                shown_iter.next().cloned()
            } else {
                Some(id.clone())
            }
        })
        .collect();
    merged.extend(shown_iter.cloned());
    merged
}

/// Read a mod image's manifest, or a source directory's `manifest.toml`.
fn read_mod_manifest(path: &Path) -> Result<Option<manifest::Manifest>> {
    if path.is_dir() {
        let file = path.join(manifest::ENTRY);
        let text = fs::read_to_string(&file)
            .with_context(|| format!("failed to read {}", file.display()))?;
        return manifest::Manifest::parse(&text)
            .map(Some)
            .with_context(|| format!("invalid {}", file.display()));
    }
    pack::read_manifest(path)
}

fn mod_enabled(mods: &Mods, id: &str) -> bool {
    if mods.disabled.iter().any(|disabled| disabled == id) {
        return false;
    }
    mods.enabled
        .as_ref()
        .is_none_or(|enabled| enabled.iter().any(|listed| listed == id))
}

fn set_mod_enabled(mods: &mut Mods, id: &str, enabled: bool) {
    mods.disabled.retain(|listed| listed != id);
    if let Some(list) = &mut mods.enabled {
        list.retain(|listed| listed != id);
    }
    if enabled {
        if let Some(list) = &mut mods.enabled {
            list.push(id.to_string());
        }
    } else {
        mods.disabled.push(id.to_string());
    }
}

/// Ask the desktop to open `path`.
fn open_folder(path: &Path) {
    let _ = fs::create_dir_all(path);
    let command = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    if let Err(err) = std::process::Command::new(command).arg(path).spawn() {
        logging::write(&format!(
            "warning: could not open {}: {err}",
            path.display()
        ));
    }
}

/// A short label for a data root in the pack table.
fn root_label(index: usize, custom: bool) -> String {
    if custom {
        return format!("Data {}", index + 1);
    }
    match index {
        0 => "Local".to_string(),
        1 => "Roaming".to_string(),
        2 => "Install".to_string(),
        other => format!("Path {}", other + 1),
    }
}

/// The pack file a first install of `game` writes.
fn default_target(game: Game) -> PathBuf {
    paths::games_dir(&paths::primary_data_dir()).join(game.default_pack_name())
}

/// The installed pack of `game`: the first pack with its id, which
/// [`scan_packs`] keeps in precedence order.
fn pack_for(packs: &[PackRow], game: Game) -> Option<&PackRow> {
    packs
        .iter()
        .find(|pack| pack.id.eq_ignore_ascii_case(game.id()))
}

/// The pack an install or rebuild of `game` writes: its resolved pack, else
/// the default target.
fn target_for(packs: &[PackRow], game: Game) -> PathBuf {
    pack_for(packs, game)
        .map(|pack| pack.path.clone())
        .unwrap_or_else(|| default_target(game))
}

/// The games the launcher offers: RE1 always, then every other game with a
/// pack or an installation.
fn known_games(packs: &[PackRow], sources: &HashMap<Game, Source>) -> Vec<Game> {
    Game::ALL
        .into_iter()
        .filter(|&game| {
            game == Game::Re1 || pack_for(packs, game).is_some() || sources.contains_key(&game)
        })
        .collect()
}

/// The game selected at startup: the first with a pack, else the first with
/// an installation, else RE1.
fn initial_game(packs: &[PackRow], sources: &HashMap<Game, Source>) -> Game {
    Game::ALL
        .into_iter()
        .find(|&game| pack_for(packs, game).is_some())
        .or_else(|| {
            Game::ALL
                .into_iter()
                .find(|game| sources.contains_key(game))
        })
        .unwrap_or(Game::Re1)
}

/// The rough size of `game`'s converted pack, for the install screen. RE2's
/// depends on the release: the Steam copy's films pack to about 215 MB in
/// all, the original PC release's to about 1.5 GB.
fn install_size(game: Game) -> &'static str {
    match game {
        Game::Re1 => "ABOUT 486 MB",
        Game::Re2 => "UP TO 1.5 GB",
    }
}

/// Validate a chosen folder as an installation of `game`, accepting a Steam
/// library or `common` folder as well as the game root itself.
fn resolve_source(path: &Path, game: Game) -> Result<PathBuf> {
    let dir = steam::install_dir(game);
    let mut candidates = vec![
        path.to_path_buf(),
        path.join(dir),
        path.join("common").join(dir),
        path.join("steamapps").join("common").join(dir),
    ];
    // RE2's data folder is found a few levels down, so the bare folder
    // would also accept a library holding the game; the Steam install root
    // is the better source when there is one.
    if game == Game::Re2 {
        candidates.rotate_left(1);
    }
    let mut first = None;
    for candidate in candidates {
        match convert::validate_install_as(&candidate, game) {
            Ok(()) => return Ok(candidate),
            // The bare folder's error is the one that names what was picked.
            Err(err) if candidate == path => first = Some(err),
            Err(_) => {}
        }
    }
    Err(first.unwrap_or_else(|| anyhow!("not a {} installation", game.title())))
}

/// A preview size from `ARKLAY_LAUNCHER_CAPTURE_SIZE=WxH`.
fn capture_size() -> Option<(u32, u32)> {
    let value = env::var("ARKLAY_LAUNCHER_CAPTURE_SIZE").ok()?;
    let (width, height) = value.split_once(['x', 'X', ','])?;
    let width = width.trim().parse().ok()?;
    let height = height.trim().parse().ok()?;
    (width > 0 && height > 0).then_some((width, height))
}

/// Unix seconds, for the update-check cache.
fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

/// Run the launcher. With `ARKLAY_LAUNCHER_CAPTURE` set it renders one frame
/// to that BMP and exits, which is how the layout is previewed headlessly.
pub fn run() -> Result<()> {
    update::cleanup();
    let (config, warnings) = Config::load_default();
    for warning in warnings {
        logging::write(&format!("warning: {warning}"));
        eprintln!("warning: {warning}");
    }
    let capture = env::var_os("ARKLAY_LAUNCHER_CAPTURE").map(PathBuf::from);
    let mut graphics = config.graphics;
    // The launcher is 2D; the game backend is chosen when Play hands the
    // window over.
    graphics.renderer = RendererKind::Software;
    let mut display = Display::new(WINDOW_TITLE, capture.is_some(), graphics)?;
    if capture.is_none() {
        display.set_resizable(true);
    }
    let (width, height) = if capture.is_some() {
        capture_size().unwrap_or_else(|| display.drawable_size())
    } else {
        display.drawable_size()
    };
    let mut launcher = Launcher::new(&config)?;
    if capture.is_none() {
        launcher.pads.init();
    }
    launcher.clear_available_if_current();
    launcher.maybe_start_check();
    // Headless checkpoints use this to exercise the install path without a
    // keyboard; it never runs for an ordinary launch.
    if capture.is_none() && env::var_os("ARKLAY_LAUNCHER_AUTOINSTALL").is_some() {
        launcher.goto(Screen::Install);
        launcher.install();
    }
    if capture.is_none() && env::var_os("ARKLAY_LAUNCHER_AUTOUPDATE").is_some() {
        launcher.auto_update = true;
        launcher.goto(Screen::Update);
        launcher.ui.set_focus_index(1);
        launcher.start_check(true);
    }
    if capture.is_none() && env::var_os("ARKLAY_LAUNCHER_AUTOROLLBACK").is_some() {
        launcher.rollback();
    }
    if let Some(path) = capture {
        if let Some(game) = env::var("ARKLAY_LAUNCHER_GAME")
            .ok()
            .and_then(|id| Game::from_id(&id))
        {
            launcher.select_game(game);
        }
        if let Ok(name) = env::var("ARKLAY_LAUNCHER_SCREEN") {
            launcher.set_capture_screen(&name, &mut display);
        }
        launcher.draw(width, height);
        let image = crate::state::Image {
            width: launcher.size.0,
            height: launcher.size.1,
            rgba: launcher.rgba.clone(),
        };
        let bytes = crate::bmp::encode_to_vec(&image)?;
        std::fs::write(&path, bytes)
            .with_context(|| format!("failed to write {}", path.display()))?;
        return Ok(());
    }
    logging::write("launcher: ready");
    let result = launcher.run_loop(&mut display);
    logging::write("launcher: exit");
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pad_axes_latch_and_repeat() {
        let start = Instant::now();
        let mut repeat = PadAxis::default();
        assert_eq!(repeat.poll(PAD_DEADZONE, start, true), Some(1));
        assert_eq!(
            repeat.poll(PAD_DEADZONE, start + Duration::from_millis(100), true),
            None
        );
        assert_eq!(
            repeat.poll(PAD_DEADZONE, start + PAD_INITIAL_DELAY, true),
            Some(1)
        );
        assert_eq!(
            repeat.poll(
                PAD_DEADZONE,
                start + PAD_INITIAL_DELAY + PAD_REPEAT_DELAY,
                true
            ),
            Some(1)
        );

        // Inside the release band the direction stays latched; crossing it
        // releases without a step, and a reverse press steps immediately.
        let mut hysteresis = PadAxis::default();
        assert_eq!(hysteresis.poll(PAD_DEADZONE, start, true), Some(1));
        assert_eq!(
            hysteresis.poll(PAD_RELEASE, start + Duration::from_millis(50), true),
            None
        );
        assert_eq!(
            hysteresis.poll(PAD_RELEASE - 1, start + Duration::from_millis(50), true),
            None
        );
        assert_eq!(
            hysteresis.poll(PAD_DEADZONE - 1, start + Duration::from_millis(50), true),
            None
        );
        assert_eq!(
            hysteresis.poll(-PAD_DEADZONE, start + Duration::from_millis(50), true),
            Some(-1)
        );

        // One-shot directions fire on the edge and never repeat.
        let mut once = PadAxis::default();
        assert_eq!(once.poll(PAD_DEADZONE, start, false), Some(1));
        assert_eq!(
            once.poll(PAD_DEADZONE, start + PAD_INITIAL_DELAY, false),
            None
        );
        assert_eq!(once.poll(0, start + PAD_INITIAL_DELAY, false), None);
    }

    #[test]
    fn counts_are_grouped() {
        assert_eq!(format_count(0), "0");
        assert_eq!(format_count(999), "999");
        assert_eq!(format_count(1204), "1,204");
        assert_eq!(format_count(486_480_423), "486,480,423");
    }

    #[test]
    fn every_games_mods_are_listed_and_ordered_by_config_then_load_order() {
        use crate::manifest::{Manifest, PackKind};
        use crate::pack::PackWriter;

        let dir = std::env::temp_dir().join(format!("arklay-launcher-mods-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let games = dir.join("games");
        let mods_dir = dir.join("mods");
        fs::create_dir_all(&games).unwrap();
        fs::create_dir_all(&mods_dir).unwrap();

        let write_pack = |path: &Path, manifest: Manifest| {
            let mut writer = PackWriter::new();
            writer
                .add(manifest::ENTRY, manifest.render().into_bytes())
                .unwrap();
            writer.write(path).unwrap();
        };
        let base = games.join("re1.akpak");
        write_pack(&base, Manifest::base("re1"));
        write_pack(
            &mods_dir.join("hd.akpak"),
            Manifest {
                kind: PackKind::Mod,
                base: Some("re1".to_string()),
                name: Some("HD".to_string()),
                load_order: 20,
                ..Manifest::base("hd")
            },
        );
        write_pack(
            &mods_dir.join("early.akpak"),
            Manifest {
                kind: PackKind::Mod,
                base: Some("re1".to_string()),
                load_order: 5,
                ..Manifest::base("early")
            },
        );
        write_pack(
            &mods_dir.join("other.akpak"),
            Manifest {
                kind: PackKind::Mod,
                base: Some("re2".to_string()),
                load_order: 1,
                ..Manifest::base("other")
            },
        );
        let demo = mods_dir.join("demo");
        fs::create_dir_all(&demo).unwrap();
        fs::write(
            demo.join(manifest::ENTRY),
            Manifest {
                kind: PackKind::Mod,
                base: Some("re1".to_string()),
                name: Some("Demo reels".to_string()),
                load_order: 10,
                ..Manifest::base("demo")
            }
            .render(),
        )
        .unwrap();

        let pack = PackRow {
            id: "re1".to_string(),
            name: "Resident Evil".to_string(),
            file: "re1.akpak".to_string(),
            version: "0.1.0".to_string(),
            source: "LOCAL".to_string(),
            path: base,
            stale: false,
            error: None,
        };
        let config = Config::default();
        let packs = std::slice::from_ref(&pack);
        let rows = scan_mods(packs, &config, std::slice::from_ref(&mods_dir));
        assert_eq!(
            rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
            ["other", "early", "demo", "hd"]
        );
        assert_eq!(
            rows.iter().map(|row| row.game.as_str()).collect::<Vec<_>>(),
            [
                "Resident Evil 2",
                "Resident Evil",
                "Resident Evil",
                "Resident Evil"
            ]
        );

        let mut config = Config::default();
        config.mods.order = vec!["hd".to_string()];
        let rows = scan_mods(packs, &config, std::slice::from_ref(&mods_dir));
        assert_eq!(
            rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
            ["hd", "other", "early", "demo"]
        );

        let re2 = games.join("re2.akpak");
        write_pack(&re2, Manifest::base("re2"));
        let re2_pack = PackRow {
            id: "re2".to_string(),
            name: "Resident Evil 2".to_string(),
            file: "re2.akpak".to_string(),
            path: re2,
            ..pack.clone()
        };
        // A second pack beside the first finds each mod once.
        let rows = scan_mods(&[pack, re2_pack], &config, std::slice::from_ref(&mods_dir));
        assert_eq!(rows.len(), 4);

        let mut mods = Mods::default();
        assert!(mod_enabled(&mods, "hd"));
        set_mod_enabled(&mut mods, "hd", false);
        assert!(!mod_enabled(&mods, "hd"));
        set_mod_enabled(&mut mods, "hd", true);
        assert!(mod_enabled(&mods, "hd"));
        assert!(mods.disabled.is_empty());
        let mut mods = Mods {
            enabled: Some(vec!["early".to_string()]),
            ..Mods::default()
        };
        assert!(!mod_enabled(&mods, "hd"));
        set_mod_enabled(&mut mods, "hd", true);
        assert_eq!(
            mods.enabled.as_deref(),
            Some(["early".to_string(), "hd".to_string()].as_slice())
        );
        set_mod_enabled(&mut mods, "early", false);
        assert_eq!(mods.disabled, vec!["early".to_string()]);
        assert_eq!(mods.enabled.as_deref(), Some(["hd".to_string()].as_slice()));

        let _ = fs::remove_dir_all(&dir);
    }

    fn pack_row(id: &str) -> PackRow {
        PackRow {
            id: id.to_string(),
            name: id.to_string(),
            file: format!("{id}.akpak"),
            version: String::new(),
            source: "LOCAL".to_string(),
            path: PathBuf::from(format!("/packs/{id}.akpak")),
            stale: false,
            error: None,
        }
    }

    #[test]
    fn games_are_offered_by_pack_and_installation() {
        let source = Source {
            path: PathBuf::from("/games/re2"),
            build: None,
        };
        let no_sources = HashMap::new();
        let re2_source = HashMap::from([(Game::Re2, source)]);

        // An RE1-only setup offers one game, so nothing switches.
        let packs = vec![pack_row("re1"), pack_row("custom")];
        assert_eq!(known_games(&packs, &no_sources), [Game::Re1]);
        assert_eq!(initial_game(&packs, &no_sources), Game::Re1);
        assert_eq!(known_games(&[], &no_sources), [Game::Re1]);
        assert_eq!(initial_game(&[], &no_sources), Game::Re1);
        assert_eq!(
            target_for(&packs, Game::Re1),
            PathBuf::from("/packs/re1.akpak")
        );
        assert_eq!(
            target_for(&packs, Game::Re2).file_name().unwrap(),
            "re2.akpak"
        );

        // An RE2 install or pack adds RE2; RE1 stays first.
        assert_eq!(known_games(&packs, &re2_source), [Game::Re1, Game::Re2]);
        assert_eq!(initial_game(&packs, &re2_source), Game::Re1);
        let both = vec![pack_row("re2"), pack_row("re1")];
        assert_eq!(known_games(&both, &no_sources), [Game::Re1, Game::Re2]);
        assert_eq!(initial_game(&both, &no_sources), Game::Re1);
        assert_eq!(
            pack_for(&both, Game::Re2).map(|pack| pack.file.as_str()),
            Some("re2.akpak")
        );

        // With only RE2 present it is the one selected.
        let re2_only = vec![pack_row("re2")];
        assert_eq!(initial_game(&re2_only, &no_sources), Game::Re2);
        assert_eq!(initial_game(&[], &re2_source), Game::Re2);
    }

    #[test]
    fn reordering_keeps_undiscovered_mods_in_place() {
        let ids = |list: &[&str]| list.iter().map(|id| id.to_string()).collect::<Vec<_>>();
        let order = ids(&["re1a", "re2a", "re1b", "re2b"]);
        assert_eq!(
            merge_order(&order, &ids(&["re2b", "re2a"])),
            ids(&["re1a", "re2b", "re1b", "re2a"])
        );
        assert_eq!(
            merge_order(&order, &ids(&["re2c", "re2a", "re2b"])),
            ids(&["re1a", "re2c", "re1b", "re2a", "re2b"])
        );
        assert_eq!(merge_order(&[], &ids(&["b", "a"])), ids(&["b", "a"]));
    }

    #[test]
    fn an_re2_pack_is_current_without_a_source_hash() {
        use crate::manifest::Manifest;
        let current = |id: &str| Manifest {
            convert_version: Some(CONVERT_VERSION),
            ..Manifest::base(id)
        };
        assert!(!is_stale(&current("re2"), "hash"));
        assert!(is_stale(&current("re1"), "hash"));
        assert!(is_stale(&Manifest::base("re2"), "hash"));
        assert!(!is_stale(
            &Manifest {
                source_hash: Some("hash".to_string()),
                ..current("re1")
            },
            "hash"
        ));
    }

    #[test]
    fn an_re2_folder_resolves_from_a_steam_library() {
        let dir = std::env::temp_dir().join(format!("arklay-launcher-re2-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let game = dir
            .join("steamapps")
            .join("common")
            .join(steam::RE2_INSTALL_DIR);
        fs::create_dir_all(game.join("english").join("Pl0").join("Rdt")).unwrap();
        assert_eq!(resolve_source(&dir, Game::Re2).unwrap(), game);
        assert_eq!(
            resolve_source(&dir.join("steamapps"), Game::Re2).unwrap(),
            game
        );
        assert_eq!(resolve_source(&game, Game::Re2).unwrap(), game);
        let error = resolve_source(&game, Game::Re1).unwrap_err();
        assert!(
            format!("{error:#}").contains("Resident Evil 2"),
            "{error:#}"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_screen_builds() {
        let mut ui = Ui::new(library().unwrap()).unwrap();
        ui.register("progress-bar", Box::new(ProgressBar));
        ui.register("checkbox", Box::new(CheckBox));
        for screen in Screen::ALL {
            ui.open(screen.name()).unwrap();
        }
    }

    #[test]
    fn a_game_install_opens_over_the_game_list() {
        let packs = vec![pack_row("re1")];
        let mut launcher =
            Launcher::with(&Config::default(), packs, HashMap::new(), Vec::new()).unwrap();
        assert_eq!(launcher.ui.screens(), [Screen::Main.name()]);
        launcher.goto(Screen::Install);
        assert_eq!(
            launcher.ui.screens(),
            [Screen::Main, Screen::Games, Screen::Install].map(Screen::name)
        );
        launcher.show_message("PREVIEW", &["A sample message."], Screen::Install);
        assert_eq!(launcher.screen(), Screen::Message);
        launcher.goto(Screen::Progress);
        assert_eq!(
            launcher.ui.screens(),
            [Screen::Main, Screen::Progress].map(Screen::name)
        );
    }
}
