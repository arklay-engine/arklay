//! `arklay script`: a Lua program that drives the engine step by step.
//!
//! The script is the program's `main`. Nothing runs until it asks:
//! `load_game_pack` opens a pack, a boot call (`load_room`, `new_game`,
//! `load_save`) starts a session, `hold`/`press` set the pad and `tick`
//! advances the game, while `game` reads and writes the live state between
//! ticks with the same methods the mod hooks get (`lua-types/hooks.lua`).
//! It is the automation seam for agents and tests: new automation goes into
//! this API rather than into more command-line flags.
//!
//! A tick is one app update, the unit `tick` counts: a fixed 30 Hz tick in
//! a room. The run is deterministic: the user's game options are ignored
//! (and, headless, their installed mods), the run options are the oracle
//! runs' unless the script passes them, no audio device opens and no film
//! plays. With a window (the default)
//! the ticks run at the game's own pace in a window opened as a normal launch
//! opens it: the renderer and window options of `arklay.toml` (the frame keeps
//! the 4:3 shape a trace compares against), the host keys (F2/F3 speed, F4,
//! F6, F8, F12, Alt+Enter) and the native in-game menu on Escape, its options
//! included, plus the F1 debug menu and the F9 prompt. While one of those
//! overlays is up the script's tick waits, a running bot is suspended and the
//! player drives the overlay. The window's game keys are otherwise ignored,
//! so a run nobody presses a host key in plays the ticks a `--headless` run
//! plays. Closing the window (or the menu's Quit) ends the run as `exit(0)`
//! does.
//!
//! Errors are strict. A Lua error, a failed boot or (unless the script calls
//! `strict(false)`) an engine warning raised while the script drives the game
//! fails the run with exit status 1.

use std::cell::{Cell, RefCell};
use std::io::Write as _;
use std::rc::Rc;
use std::time::{Duration, Instant};

use mlua::{
    AnyUserData, Function, HookTriggers, Lua, LuaOptions, MetaMethod, MultiValue, ObjectLike,
    StdLib, Table, UserData, UserDataMethods, Value, Variadic, VmState,
};

use super::*;
use crate::save::re2_image;

/// The regions two RE 2 save images differ in under the mask
/// ([`re2_image::differences`]), as the script functions return them.
fn differences_table(
    lua: &Lua,
    ours: &[u8; re2_image::IMAGE_LEN],
    theirs: &[u8; re2_image::IMAGE_LEN],
) -> mlua::Result<Table> {
    let hex = |bytes: &[u8]| {
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    };
    let list = lua.create_table()?;
    for difference in re2_image::differences(ours, theirs) {
        let entry = lua.create_table()?;
        entry.set("region", difference.region)?;
        entry.set("offset", difference.offset)?;
        entry.set("ours", hex(&difference.ours))?;
        entry.set("theirs", hex(&difference.theirs))?;
        list.push(entry)?;
    }
    Ok(list)
}

/// The allocation ceiling of a driver script's Lua state.
const MEMORY_LIMIT: usize = 256 * 1024 * 1024;
/// Lua instructions between two timeout checks.
const HOOK_INTERVAL: u32 = 10_000;
/// Base-library globals removed before the script runs: the filesystem
/// loaders, `load` and the collector control.
const REMOVED_GLOBALS: [&str; 4] = ["dofile", "loadfile", "load", "collectgarbage"];
/// The deepest table nesting `json` encodes.
const JSON_DEPTH: usize = 64;
/// The most values one `json` call encodes: shared subtables could otherwise
/// make an acyclic value exponentially large, in Rust where the timeout
/// hook never runs.
const JSON_VALUES: usize = 1_000_000;
/// Engine methods `game` may call before the first tick without counting as
/// an edit of the booted state: the read-only getters of
/// `impl UserData for GameState` in `src/lua.rs`.
const READ_ONLY: [&str; 14] = [
    "room",
    "authentic",
    "flag_get",
    "item_count",
    "player_pos",
    "player_angle",
    "health",
    "max_health",
    "character",
    "stage",
    "camera",
    "frame",
    "is_re2",
    "inventory",
];

/// One `arklay script` run.
#[derive(Debug, Clone)]
pub struct ScriptOptions {
    /// The chunk name errors report: the script's path, or `stdin`.
    pub name: String,
    /// The script's source.
    pub source: Vec<u8>,
    /// The arguments after the script, the script's `arg[1..]`.
    pub args: Vec<String>,
    /// Run without a window.
    pub headless: bool,
    /// Fail the run once it has taken this long.
    pub timeout: Option<Duration>,
}

/// Run a driver script and return its exit status: `0` when it finishes,
/// or the status it passed to `exit`. A Lua error, a failed engine call or a
/// strict warning is returned as an error, which the CLI reports with
/// status 1.
pub fn run_script(options: ScriptOptions) -> Result<i32> {
    // A trace compares against the 4:3 reference, as the oracle runs do.
    video::set_aspect(video::Aspect::STANDARD);
    let stop = Rc::new(Stop {
        exit: Cell::new(None),
        timed_out: Cell::new(false),
        // A timeout too long to reach is no timeout.
        deadline: options
            .timeout
            .and_then(|timeout| Instant::now().checked_add(timeout)),
        timeout: options.timeout.unwrap_or_default(),
    });
    let driver = Rc::new(RefCell::new(Driver::new(
        options.headless,
        Rc::clone(&stop),
    )));
    let lua = new_state(&stop)?;
    lua.set_app_data(Rc::clone(&driver));
    install(&lua, &driver, &options).map_err(|err| anyhow::anyhow!("{err}"))?;
    let result = lua
        .load(&options.source[..])
        .set_name(format!("@{}", options.name))
        .exec();
    let finished = driver.borrow_mut().finish();
    if let Some(code) = stop.exit.get() {
        finished?;
        return Ok(code);
    }
    // A `pcall` may have caught the timeout; the run still fails.
    if stop.timed_out.get() {
        bail!("{}", stop.timeout_message());
    }
    result.map_err(|err| anyhow::anyhow!("{}", describe(&err)))?;
    finished?;
    Ok(0)
}

/// Why the run must stop: `exit` was called or the deadline passed. Both
/// are final: once set, every Lua instruction fails, so no `pcall` keeps the
/// script running.
struct Stop {
    exit: Cell<Option<i32>>,
    timed_out: Cell<bool>,
    deadline: Option<Instant>,
    timeout: Duration,
}

impl Stop {
    fn timeout_message(&self) -> String {
        format!(
            "the script timed out after {} s (--timeout)",
            self.timeout.as_secs_f64()
        )
    }

    /// The error a stopped run raises, or `None` while it may go on. Passing
    /// the deadline stops the run.
    fn error(&self) -> Option<mlua::Error> {
        if self.exit.get().is_some() {
            return Some(mlua::Error::RuntimeError("exit".into()));
        }
        if !self.timed_out.get()
            && let Some(deadline) = self.deadline
            && Instant::now() >= deadline
        {
            self.timed_out.set(true);
        }
        self.timed_out
            .get()
            .then(|| mlua::Error::RuntimeError(self.timeout_message()))
    }

    /// The player closed the window or quit from the game's menu: the run
    /// ends as `exit(0)` ends it, with no error, unless it is already
    /// stopping with a status of its own.
    fn player_quit(&self) -> mlua::Error {
        if self.exit.get().is_none() {
            self.exit.set(Some(0));
        }
        mlua::Error::RuntimeError("exit".into())
    }
}

/// Make every instruction from now on fail with the stop's error.
fn halt(lua: &Lua, stop: &Rc<Stop>) {
    let stop = Rc::clone(stop);
    lua.set_hook(
        HookTriggers::new().every_nth_instruction(1),
        move |_lua, _debug| match stop.error() {
            Some(err) => Err(err),
            None => Ok(VmState::Continue),
        },
    );
}

/// The script's Lua state: base, `string`, `table`, `math` and `utf8`, the
/// loaders removed, the allocation ceiling and the stop hook installed.
fn new_state(stop: &Rc<Stop>) -> Result<Lua> {
    let lua = Lua::new_with(
        StdLib::STRING | StdLib::TABLE | StdLib::MATH | StdLib::UTF8,
        LuaOptions::default(),
    )
    .map_err(|err| anyhow::anyhow!("failed to create the Lua state: {err}"))?;
    let globals = lua.globals();
    for name in REMOVED_GLOBALS {
        globals
            .set(name, Value::Nil)
            .map_err(|err| anyhow::anyhow!("failed to remove the Lua global {name}: {err}"))?;
    }
    lua.set_memory_limit(MEMORY_LIMIT)
        .map_err(|err| anyhow::anyhow!("failed to set the Lua memory limit: {err}"))?;
    // The deadline, or the player closing the window, stops the run.
    let check = Rc::clone(stop);
    lua.set_hook(
        HookTriggers::new().every_nth_instruction(HOOK_INTERVAL),
        move |lua, _debug| match check.error() {
            Some(err) => {
                // From then on, fail every instruction: a `pcall` around
                // the loop cannot outlast it.
                halt(lua, &check);
                Err(err)
            }
            None => Ok(VmState::Continue),
        },
    );
    Ok(lua)
}

/// An error's message for the CLI, without mlua's nested callback framing.
fn describe(err: &mlua::Error) -> String {
    match err {
        mlua::Error::CallbackError { traceback, cause } => {
            format!("{}\n{traceback}", describe(cause))
        }
        mlua::Error::WithContext { context, cause } => format!("{context}: {}", describe(cause)),
        mlua::Error::RuntimeError(message) => message.clone(),
        other => other.to_string(),
    }
}

/// An engine error as a Lua error.
fn engine_error(err: anyhow::Error) -> mlua::Error {
    mlua::Error::RuntimeError(format!("{err:#}"))
}

/// A save directory the run owns, removed when it ends: a script that saves
/// in game never touches the player's own saves.
struct ScratchSaves(PathBuf);

impl ScratchSaves {
    fn create() -> Result<Self> {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "arklay-script-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path)
            .with_context(|| format!("failed to create {}", path.display()))?;
        Ok(Self(path))
    }
}

impl Drop for ScratchSaves {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The trace a script is recording (`trace_start`).
struct ScriptTrace {
    writer: trace::TraceWriter,
    /// Trace the frames of a door transition and the death screen too.
    all_frames: bool,
    /// A screen trace: started on a screen outside play (the title), it
    /// writes a line every tick in the screen format, whatever the app shows.
    screen: bool,
    /// `sound_wav`: the session's mix, captured as it plays.
    wav: Option<WavCapture>,
    /// Neither `sound` nor `sound_wav`: the session's voice lines pace
    /// nothing, as in the reference's silenced test runs.
    unpaced_voice: bool,
}

/// A trace's audio capture: each tick mixes the wall time it takes when
/// played (a door's frames run on the 16 ms limiter), and each traced tick
/// is logged with the capture's length after it, so the WAV lines up with
/// the original's capture tick by tick.
struct WavCapture {
    path: PathBuf,
    /// The sample frames owed to the capture, carried between ticks.
    due: f64,
    /// `FILE.ticks`: a traced tick and its time in seconds per line.
    ticks: String,
    /// The mix so far, taken from the session's capture mixer every tick,
    /// so it outlives the session: a clear's RESULT or a return to the title
    /// takes the mixer with the session. The capture is that session's; a
    /// later one, with no capture mixer, adds nothing.
    pcm: Vec<u8>,
}

impl ScriptTrace {
    /// Flush the trace and write its WAV and `.ticks` log.
    fn close(mut self) -> Result<()> {
        self.writer.flush()?;
        let Some(wav) = self.wav else {
            return Ok(());
        };
        write_stereo_wav(&wav.path, &wav.pcm)?;
        let ticks = wav.path.with_extension("ticks");
        std::fs::write(&ticks, wav.ticks)
            .with_context(|| format!("failed to write {}", ticks.display()))
    }
}

/// The live view of a windowed run.
struct Window {
    display: Display,
    event: SDL_Event,
    /// The player's keyboard and pads, with the `arklay.toml` bindings: only
    /// the host keys and the overlays they open reach the game.
    input: InputState,
    /// The F2/F3 speed (and the options' frame rate).
    clock: LoopClock,
    hud: debug_hud::DebugHud,
    /// F12 was pressed: save the next presented frame.
    screenshot: bool,
    /// When the next tick is due.
    next: Instant,
    /// When the next frame may be presented: a sped-up run presents no
    /// faster than the display's frame rate.
    present_at: Instant,
}

#[cfg(feature = "bot")]
mod bot_api;

#[cfg(not(feature = "bot"))]
mod bot_api {
    /// The bot functions a build without the bot answers with an error.
    pub(super) const FUNCTIONS: [&str; 6] = [
        "bot_start",
        "bot_finished",
        "bot_stats",
        "bot_memory",
        "bot_story",
        "bot_run",
    ];
}

/// The engine state a script drives.
struct Driver {
    headless: bool,
    stop: Rc<Stop>,
    /// Fail on engine warnings (`strict(false)` turns it off).
    strict: bool,
    /// The warning count already checked.
    warnings: u64,
    app: Option<App>,
    /// The scratch save directory the open pack's app uses, if it is ours.
    saves: Option<ScratchSaves>,
    /// The scripted pad latch.
    input: InputState,
    /// The keys held until released (`hold`/`release`/`pad`).
    held: u64,
    /// The keys held for the next tick only (`press`).
    pressed: u64,
    /// The physical key or pad button newly down on the next tick only
    /// (`press_key`), as a screen that binds keys reads it.
    pressed_source: Option<crate::controls::Source>,
    /// The booted session has not ticked yet: what its state was before
    /// the script's `game` edits, so the first tick (or a `save_game` or
    /// `capture`) reruns what they invalidate once, as an authored setup does.
    baseline: Option<AuthorBaseline>,
    /// A `game` call other than a getter ran since the boot.
    edited: bool,
    /// Arm the session-flow boot before the first tick, as the `--ui game`
    /// trace run does.
    arm: bool,
    /// The last tick drew the frame `capture` writes.
    drawn: bool,
    trace: Option<ScriptTrace>,
    window: Option<Window>,
    /// The window's player pressed a host key: an overlay up now is theirs
    /// to drive, and the script's tick waits for it to close. (A script's
    /// own `f9` opens a prompt the script drives.)
    player_overlay: bool,
    /// When `bot_start` attached the bot, for the report's elapsed time.
    #[cfg(feature = "bot")]
    bot_started: Option<Instant>,
}

impl Driver {
    fn new(headless: bool, stop: Rc<Stop>) -> Self {
        Self {
            headless,
            stop,
            strict: true,
            warnings: crate::logging::warnings_since(0).0,
            app: None,
            saves: None,
            input: InputState::default(),
            held: 0,
            pressed: 0,
            pressed_source: None,
            baseline: None,
            edited: false,
            arm: false,
            drawn: false,
            trace: None,
            window: None,
            player_overlay: false,
            #[cfg(feature = "bot")]
            bot_started: None,
        }
    }

    /// The open pack's app.
    fn app(&mut self) -> mlua::Result<&mut App> {
        self.app.as_mut().ok_or_else(|| {
            mlua::Error::RuntimeError("no game pack is open: call load_game_pack first".into())
        })
    }

    /// The running game session.
    fn session(&mut self) -> mlua::Result<&mut GameSession> {
        match &mut self.app()?.mode {
            Mode::Play(session) => Ok(session),
            _ => Err(mlua::Error::RuntimeError(
                "no game session is running: call load_room, new_game or load_save first".into(),
            )),
        }
    }

    /// Fail the call when the run must stop (`exit`, the timeout).
    fn check_running(&self) -> mlua::Result<()> {
        self.stop.error().map_or(Ok(()), Err)
    }

    /// Fail the call when the engine printed a warning since the last check
    /// and the run is strict.
    fn check_warnings(&mut self) -> mlua::Result<()> {
        let seen = self.warnings;
        let (count, first) = crate::logging::warnings_since(seen);
        self.warnings = count;
        if count > seen && self.strict {
            let more = match count - seen {
                1 => String::new(),
                raised => format!(" ({} more on stderr)", raised - 1),
            };
            return Err(mlua::Error::RuntimeError(format!(
                "engine warning (call strict(false) to allow warnings): {}{more}",
                first.as_deref().unwrap_or("").trim()
            )));
        }
        Ok(())
    }

    /// Count the warnings raised since the last check as seen without
    /// failing the run: the window's and the in-game menu's (a renderer
    /// falling back to software, an option not saved) and the game's while
    /// the player drives a menu are the host's, not the script's. They are
    /// still printed on stderr.
    fn absorb_host_warnings(&mut self) {
        self.warnings = crate::logging::warnings_since(self.warnings).0;
    }

    /// The game quit (its menu's Quit): with a window that is the player
    /// ending the run, as closing the window is; a headless game that quits
    /// fails the run.
    fn stop_on_quit(&self) -> mlua::Error {
        if self.window.is_some() {
            self.stop.player_quit()
        } else {
            mlua::Error::RuntimeError("the game quit".into())
        }
    }

    /// Drain the window's events between ticks, so it stays responsive, and
    /// carry out the host keys and the in-game menu's input as the
    /// interactive loop does; closing the window ends the run.
    fn pump(&mut self) -> mlua::Result<()> {
        if self.window.is_none() {
            return Ok(());
        }
        // The game's warnings so far count; the window's and the menu's
        // below do not.
        self.check_warnings()?;
        let result = self.pump_window();
        self.absorb_host_warnings();
        result
    }

    /// [`Self::pump`]'s window work.
    fn pump_window(&mut self) -> mlua::Result<()> {
        let Some(window) = self.window.as_mut() else {
            return Ok(());
        };
        let mut keys = PortKeys::default();
        if poll_events(
            &mut window.event,
            &mut window.input,
            &mut keys,
            Some(&mut window.display),
        ) {
            return Err(self.stop.player_quit());
        }
        window.screenshot |= keys.screenshot;
        let Some(app) = self.app.as_mut() else {
            // Nothing to show yet: the presses have nothing to act on.
            window.input.discard_presses();
            let _ = window.input.take_gui_events();
            return Ok(());
        };
        apply_port_keys(app, &keys, &mut window.clock, &mut window.hud);
        drive_system_menu(
            app,
            &mut window.display,
            &mut window.input,
            &mut window.clock,
        )
        .map_err(engine_error)
    }

    /// Wait out a host overlay the player opened (the in-game menu and its
    /// screens, the F1 debug menu, the F9 prompt): the frozen game updates
    /// with the player's input and is presented at its own pace until the
    /// overlay closes. The script's tick is not taken meanwhile, and a running
    /// bot is not ticked.
    fn hold_for_host(&mut self) -> mlua::Result<()> {
        loop {
            self.check_running()?;
            self.pump()?;
            let (Some(app), Some(window)) = (self.app.as_mut(), self.window.as_mut()) else {
                return Ok(());
            };
            if !(self.player_overlay && app.host_overlay()) {
                self.player_overlay = false;
                return Ok(());
            }
            let tick = window.input.tick();
            if let AppFlow::Quit = app
                .update(tick.ui, tick.player, tick.action)
                .map_err(engine_error)?
            {
                return Err(self.stop.player_quit());
            }
            self.absorb_host_warnings();
            self.present()?;
        }
    }

    /// Present the app in the window, paced to the game's tick and no faster
    /// than the display's frame rate; returns whether the software
    /// framebuffer was drawn for it.
    fn present(&mut self) -> mlua::Result<bool> {
        self.check_warnings()?;
        let (Some(app), Some(window)) = (self.app.as_mut(), self.window.as_mut()) else {
            return Ok(false);
        };
        let now = Instant::now();
        let mut drawn = false;
        if now >= window.present_at {
            drawn = present_app_frame(
                app,
                &mut window.display,
                &mut window.hud,
                &mut window.clock,
                &window.input,
                std::mem::take(&mut window.screenshot),
            )
            .map_err(engine_error)?;
            let frame =
                Duration::from_secs_f64(1.0 / f64::from(window.display.frame_rate().max(1)));
            window.present_at = now.max(window.present_at) + frame;
            self.warnings = crate::logging::warnings_since(self.warnings).0;
        }
        let tick_ms = window
            .clock
            .scaled_tick_interval(app.tick_interval(), app.speed_applies());
        window.next += Duration::from_secs_f64(tick_ms / 1000.0);
        let now = Instant::now();
        if window.next > now {
            std::thread::sleep(window.next - now);
        } else {
            window.next = now;
        }
        app.post_present();
        Ok(drawn)
    }

    /// Rerun what the script's `game` edits since the boot invalidate, once,
    /// before the session first ticks or is saved or captured.
    fn settle_authoring(&mut self) -> mlua::Result<()> {
        let Some(baseline) = self.baseline.take() else {
            return Ok(());
        };
        let edited = std::mem::take(&mut self.edited);
        let session = self.session()?;
        if edited {
            session.finish_authoring(baseline, true);
        } else {
            session.game.re2.authored_flags = None;
        }
        Ok(())
    }

    /// Open the live view if this run has a window and it is not open yet.
    fn open_window(&mut self, title: &str) -> Result<()> {
        if self.headless || self.window.is_some() {
            return Ok(());
        }
        // The user's window, as a normal launch opens it: the renderer,
        // scale, frame rate and window options of `arklay.toml`. The frame
        // keeps the 4:3 shape the run's traces and captures compare against
        // (the options can still change it). The window's geometry is not
        // written back, but the options the player changes are.
        let (config, _) = crate::config::Config::load_default();
        let graphics = Graphics {
            aspect: video::Aspect::STANDARD,
            ..config.graphics
        };
        let mut display = Display::new(title, false, graphics).map_err(|err| {
            anyhow::anyhow!("cannot open a window ({err:#}); pass --headless to run without one")
        })?;
        display.persist = false;
        let input = InputState::interactive();
        let now = Instant::now();
        self.window = Some(Window {
            clock: LoopClock::new(display.frame_rate(), false),
            hud: debug_hud::DebugHud::new(config.game.ui_font),
            display,
            event: SDL_Event::default(),
            input,
            screenshot: false,
            next: now,
            present_at: now,
        });
        Ok(())
    }

    /// Start a booted session: drop what the last one left behind.
    /// Forget the voice lengths a screen's trace held for the game that
    /// screen starts: the trace or the screen is gone.
    fn drop_pending_voice(&mut self) {
        if let Some(app) = self.app.as_mut() {
            app.pending_voice_ticks = None;
            app.pending_sound = false;
        }
    }

    fn start(&mut self, mut session: Box<GameSession>, arm: bool) -> mlua::Result<()> {
        let baseline = session.author_baseline();
        // The script's game, not one a screen started.
        self.drop_pending_voice();
        let app = self.app()?;
        app.start_session(session);
        app.pending_movie = None;
        self.baseline = Some(baseline);
        self.edited = false;
        self.arm = arm;
        self.drawn = false;
        self.input = InputState::default();
        Ok(())
    }

    /// Advance one app update with the scripted pad.
    fn tick(&mut self) -> mlua::Result<()> {
        self.check_running()?;
        if matches!(self.app()?.mode, Mode::Boot) {
            return Err(mlua::Error::RuntimeError(
                "nothing is booted: call load_room, new_game, load_save or open_title first".into(),
            ));
        }
        self.settle_authoring()?;
        self.hold_for_host()?;
        let word = self.held | self.pressed;
        self.pressed = 0;
        let arm = std::mem::take(&mut self.arm);
        let Some(app) = self.app.as_mut() else {
            unreachable!("checked above");
        };
        if let Mode::Play(session) = &mut app.mode {
            if arm {
                session.arm_new_game_boot();
            }
            session.voice_cache.unpaced =
                self.trace.as_ref().is_some_and(|trace| trace.unpaced_voice);
        }
        let was_play = matches!(app.mode, Mode::Play(_));
        let in_transition =
            matches!(&app.mode, Mode::Play(session) if session.transition.is_some());
        let screen_before =
            matches!(&app.mode, Mode::Play(session) if session.death_screen.is_some());
        // The wall time this tick takes when played, which the audio capture
        // mixes for it.
        let frame_ms = match &app.mode {
            Mode::Play(session) => {
                session_tick_interval(session.transition.is_some(), session.ui_tick_rate())
            }
            _ => TICK_MS,
        };
        let mut traced = None;
        let mut tick = self.input.tick_scripted(word);
        if let Some(source) = self.pressed_source.take() {
            tick.ui.captured = Some(source);
        }
        if let Some(window) = self.window.as_mut() {
            // The player's host keys reach the game; their other game keys
            // do not.
            self.player_overlay |= add_host_keys(&mut tick.ui, &window.input.tick().ui);
        }
        if let AppFlow::Quit = app
            .update(tick.ui, tick.player, tick.action)
            .map_err(engine_error)?
        {
            return Err(self.stop_on_quit());
        }
        // A script's film (`film_pads`) ends with nothing held, as its
        // `Pad_set`s saw: a key held through it is pressed anew on the next
        // tick, the port's own (START, the map) as well as the game's.
        if let Mode::Play(session) = &mut app.mode
            && std::mem::take(&mut session.film_ended)
        {
            self.input.held = 0;
        }
        // Films never play: the frames stay deterministic and audio-free, as
        // in a trace, and a film chain (the ending's) runs on to what follows
        // it. An attached bot sees them and skips them as a player would, as
        // it does in the game window.
        if !app.bot_attached() {
            if let Mode::Play(session) = &mut app.mode {
                session.movie = None;
            }
            if let AppFlow::Quit = app.drop_pending_movie().map_err(engine_error)? {
                return Err(self.stop_on_quit());
            }
        }
        // A headless bot run draws nothing: no frame is shown, traced or
        // (until `capture` draws its own) written, and a game is long. A
        // window draws the frames it presents itself, on its renderer.
        self.drawn = self.trace.is_some()
            || !(self.window.is_some() || (self.headless && app.bot_attached()));
        if self.drawn {
            app.draw();
        }
        if let Some(trace) = self.trace.as_mut() {
            if trace.screen {
                // A screen's trace that goes on into play (a new game's
                // choice, a load) writes the game's line from there, a
                // line a tick as before it.
                if let Mode::Play(session) = &app.mode {
                    app.ui_sounds.clear();
                    trace
                        .writer
                        .write_tick(&session.trace_tick(&app.framebuffer))
                        .map_err(engine_error)?;
                } else {
                    let sounds = std::mem::take(&mut app.ui_sounds);
                    let line = screen_trace_line(app, trace.writer.tick_index(), word, &sounds);
                    trace.writer.write_line(&line).map_err(engine_error)?;
                }
            } else if let Mode::Play(session) = &app.mode
                && was_play
                && ((!in_transition && !screen_before) || trace.all_frames)
            {
                traced = Some(trace.writer.tick_index());
                trace
                    .writer
                    .write_tick(&session.trace_tick(&app.framebuffer))
                    .map_err(engine_error)?;
            }
        }
        if self.window.is_some() {
            self.drawn |= self.present()?;
        } else {
            app.post_present();
        }
        if let Some(wav) = self.trace.as_mut().and_then(|trace| trace.wav.as_mut())
            && let Some(app) = self.app.as_mut()
            && let Mode::Play(session) = &mut app.mode
            && let Some(mixer) = &mut session.music
        {
            wav.due += frame_ms * f64::from(audio::SAMPLE_RATE) / 1000.0;
            let frames = wav.due.floor();
            wav.due -= frames;
            mixer.capture_frames(frames as usize);
            if let Some(pcm) = mixer.take_captured() {
                wav.pcm.extend_from_slice(&pcm);
            }
            if let Some(index) = traced {
                let mixed = wav.pcm.len() / 4;
                let seconds = mixed as f64 / f64::from(audio::SAMPLE_RATE);
                wav.ticks.push_str(&format!("{index} {seconds:.4}\n"));
            }
        }
        self.pump()?;
        self.check_warnings()
    }

    /// Close the running trace, if any.
    fn close_trace(&mut self) -> Result<()> {
        // The voice lengths a trace gave, held for a screen's game or not yet
        // used by the session's lines, go with the trace.
        self.drop_pending_voice();
        if let Some(App {
            mode: Mode::Play(session),
            ..
        }) = self.app.as_mut()
        {
            session.game.re2.voice_ticks.clear();
        }
        match self.trace.take() {
            Some(trace) => trace.close(),
            None => Ok(()),
        }
    }

    /// Flush the trace and close the window at the end of the run.
    fn finish(&mut self) -> Result<()> {
        self.close_trace()?;
        self.window = None;
        self.app = None;
        self.saves = None;
        Ok(())
    }
}

/// Add the player's host keys (Escape's in-game menu, F1, F9) to the
/// scripted tick's `ui`, returning whether they pressed one; the player's
/// other keys stay out of the game.
fn add_host_keys(ui: &mut UiInput, player: &UiInput) -> bool {
    ui.system_menu |= player.system_menu;
    ui.debug_menu |= player.debug_menu;
    ui.return_title |= player.return_title;
    if player.system_menu || player.debug_menu || player.return_title {
        // The press is the player's, a keyboard key's.
        ui.any = true;
        return true;
    }
    false
}

/// The driver behind a callback.
type Shared = Rc<RefCell<Driver>>;

/// Borrow the driver for one engine call.
fn with<R>(driver: &Shared, call: impl FnOnce(&mut Driver) -> mlua::Result<R>) -> mlua::Result<R> {
    let mut driver = driver.try_borrow_mut().map_err(|_| {
        mlua::Error::RuntimeError("the engine is busy: a script call re-entered it".into())
    })?;
    driver.check_running()?;
    driver.pump()?;
    let result = call(&mut driver);
    // Check the warnings whatever the result, so one the failed call raised
    // is not blamed on the next.
    let warned = driver.check_warnings();
    let value = result?;
    warned?;
    Ok(value)
}

/// An options table whose unknown keys are an error, so a misspelt option
/// fails instead of being ignored.
struct Options<'a> {
    function: &'a str,
    table: Option<Table>,
    known: Vec<&'static str>,
}

impl<'a> Options<'a> {
    fn new(function: &'a str, table: Option<Table>) -> Self {
        Self {
            function,
            table,
            known: Vec::new(),
        }
    }

    fn get<T: mlua::FromLua>(&mut self, key: &'static str) -> mlua::Result<Option<T>> {
        self.known.push(key);
        match &self.table {
            Some(table) => table.get::<Option<T>>(key).map_err(|err| {
                mlua::Error::RuntimeError(format!("{}: option `{key}`: {err}", self.function))
            }),
            None => Ok(None),
        }
    }

    fn finish(self) -> mlua::Result<()> {
        let Some(table) = self.table else {
            return Ok(());
        };
        for pair in table.pairs::<Value, Value>() {
            let (key, _) = pair?;
            let name = match &key {
                Value::String(name) => name.to_string_lossy(),
                other => format!("{other:?}"),
            };
            if !self.known.contains(&name.as_str()) {
                return Err(mlua::Error::RuntimeError(format!(
                    "{}: unknown option `{name}` (expected one of: {})",
                    self.function,
                    self.known.join(", ")
                )));
            }
        }
        Ok(())
    }
}

/// Open `path` with the `mods` layers the script names. With `user_mods` the
/// user's installed mods are layered under them too, discovered and selected
/// by `arklay.toml` as a normal launch layers them; without it there is no
/// discovery and no config selection.
fn open_pack(
    path: &Path,
    mods: &[PathBuf],
    user_mods: bool,
    language: Option<&str>,
) -> Result<Pack> {
    // `re1` names the installed RE1 pack, found as the launcher finds it.
    let path = &crate::pack::resolve_game_pack(path)?;
    if user_mods {
        let (config, _) = crate::config::Config::load_default();
        let options = ModOptions {
            explicit: mods.to_vec(),
            no_mods: false,
            layers: crate::pack::LayerOptions {
                enabled: config.mods.enabled,
                disabled: config.mods.disabled,
                order: config.mods.order,
            },
            global_dirs: crate::config::Config::mod_dirs(),
            watch: false,
        };
        return open_game_pack(path, &options, language);
    }
    let mut pack = if mods.is_empty() {
        Pack::open(path)?
    } else {
        let layers: Vec<ModLayer<'_>> = mods
            .iter()
            .map(|path| ModLayer {
                path,
                explicit: true,
            })
            .collect();
        let pack = Pack::open_layered_with(path, &layers, &LayerOptions::default())?;
        for warning in pack.warnings() {
            crate::warning!("warning: {warning}");
        }
        pack
    };
    pack.set_language(language);
    Ok(pack)
}

/// Whether a converted pack holds what another build would pack, as the
/// launcher judges it stale: its conversion revision or the fingerprint of
/// the engine's own sources it packs (`games/re2`'s scripts, say) is not this
/// build's, or is missing, so a run of it plays those, not the ones under
/// test. A pack no conversion wrote (neither field) is left alone.
fn converted_by_another_build(manifest: &crate::manifest::Manifest) -> bool {
    if manifest.convert_version.is_none() && manifest.source_hash.is_none() {
        return false;
    }
    let game = crate::convert::Game::from_id(&manifest.id).unwrap_or(crate::convert::Game::Re1);
    crate::launcher::is_stale(manifest, &[(game, game.packed())].into())
}

/// `load_game_pack`'s `title_keys`: config.ini's `KeyDef` and
/// `JoyDef_Emulated` as `{ keys = "...", pad = "..." }`, each the line's
/// value. Anything else is an error, since a table that isn't what was meant
/// lists other keys.
fn title_keys_option(table: &Table) -> mlua::Result<player::TitleKeys> {
    let bad =
        |what: String| mlua::Error::RuntimeError(format!("load_game_pack: title_keys {what}"));
    let mut fields = [None, None];
    for pair in table.pairs::<Value, Value>() {
        let (key, value) = pair?;
        let slot = match key.as_string().and_then(|key| key.to_str().ok()).as_deref() {
            Some("keys") => 0,
            Some("pad") => 1,
            _ => return Err(bad("is `keys` and `pad`".to_owned())),
        };
        let Value::String(text) = value else {
            return Err(bad("holds config.ini's values as strings".to_owned()));
        };
        fields[slot] = Some(text.to_str()?.to_owned());
    }
    let [Some(keys), Some(pad)] = fields else {
        return Err(bad("needs both `keys` and `pad`".to_owned()));
    };
    player::TitleKeys::parse(&keys, &pad).map_err(|what| bad(format!("is wrong: {what}")))
}

/// `load_game_pack`'s `title_gallery`: config.ini's `GALLERY0` to
/// `GALLERY4`, a table of exactly five integers of 32 bits. Anything else is
/// an error, since a word that isn't what was meant opens other pictures.
fn gallery_words(table: &Table) -> mlua::Result<[u32; 5]> {
    let bad =
        |what: String| mlua::Error::RuntimeError(format!("load_game_pack: title_gallery {what}"));
    let mut words = [None; 5];
    for pair in table.pairs::<Value, Value>() {
        let (key, value) = pair?;
        let slot = match key {
            Value::Integer(n @ 1..=5) => n as usize - 1,
            _ => return Err(bad("is five words, numbered 1 to 5".to_owned())),
        };
        words[slot] = Some(match value {
            Value::Integer(word) => u32::try_from(word)
                .map_err(|_| bad(format!("word {} isn't a 32-bit value", slot + 1)))?,
            _ => return Err(bad(format!("word {} isn't an integer", slot + 1))),
        });
    }
    match words {
        [Some(a), Some(b), Some(c), Some(d), Some(e)] => Ok([a, b, c, d, e]),
        _ => Err(bad("is five words, numbered 1 to 5".to_owned())),
    }
}

/// The key bit a button name maps to: the input-script names plus `forward`
/// and `backward` for the tank controls' up and down.
fn button_bit(name: &str) -> mlua::Result<u64> {
    let bit = match name {
        "forward" => Some(KEY_UP),
        "backward" | "back" => Some(KEY_DOWN),
        _ => script_button_bit(name),
    };
    bit.ok_or_else(|| mlua::Error::RuntimeError(format!("unknown button `{name}`")))
}

/// The bits of every named button.
fn buttons(names: &[String]) -> mlua::Result<u64> {
    names
        .iter()
        .try_fold(0, |word, name| Ok(word | button_bit(name)?))
}

/// The parsed room id of `room` for `player`, optionally on its second visit.
fn room_id(room: &str, player: u8, mansion2: bool) -> mlua::Result<RoomId> {
    let id = RoomId::from_room_and_player(room, player).map_err(engine_error)?;
    if mansion2 && !id.is_mansion() {
        return Err(mlua::Error::RuntimeError(format!(
            "mansion2 needs a mansion room (stage 1 or 2), not {room}"
        )));
    }
    Ok(id.with_visit(mansion2))
}

/// The name `mode()` reports for the app's screen.
fn mode_name(mode: &Mode) -> &'static str {
    match mode {
        Mode::Boot => "none",
        Mode::Title(_) => "title",
        Mode::Select(_) => "select",
        Mode::Load(_) => "load",
        Mode::Play(_) => "play",
        Mode::Movie(_) => "movie",
        Mode::Ending(_) => "ending",
        Mode::Epilogue(_) => "epilogue",
        Mode::Save(_) => "save",
    }
}

/// Encode a Lua value as JSON. A table with only the keys `1..n` is an
/// array, any other table an object with sorted keys; an empty table is
/// `[]`, the empty list a getter such as `inventory()` returns.
fn to_json(value: &Value) -> mlua::Result<serde_json::Value> {
    JsonEncoder {
        path: Vec::new(),
        values: 0,
    }
    .encode(value)
}

/// [`to_json`]'s state: the tables being encoded (a cycle repeats one) and
/// the number of values encoded so far.
struct JsonEncoder {
    path: Vec<*const std::ffi::c_void>,
    values: usize,
}

impl JsonEncoder {
    fn encode(&mut self, value: &Value) -> mlua::Result<serde_json::Value> {
        use serde_json::Value as Json;
        let fail = |message: String| Err(mlua::Error::RuntimeError(format!("json: {message}")));
        self.values += 1;
        if self.values > JSON_VALUES {
            return fail(format!("the value holds more than {JSON_VALUES} values"));
        }
        let table = match value {
            Value::Nil => return Ok(Json::Null),
            Value::Boolean(value) => return Ok(Json::Bool(*value)),
            Value::Integer(value) => return Ok(Json::from(*value)),
            Value::Number(value) => {
                return Ok(serde_json::Number::from_f64(*value).map_or(Json::Null, Json::Number));
            }
            Value::String(value) => return Ok(Json::String(value.to_string_lossy())),
            Value::Table(table) => table,
            other => return fail(format!("a {} has no JSON form", other.type_name())),
        };
        let pointer = table.to_pointer();
        if self.path.contains(&pointer) {
            return fail("the table contains itself".into());
        }
        if self.path.len() >= JSON_DEPTH {
            return fail(format!("tables nest more than {JSON_DEPTH} deep"));
        }
        self.path.push(pointer);
        let length = table.raw_len();
        let mut count = 0;
        for pair in table.pairs::<Value, Value>() {
            pair?;
            count += 1;
        }
        let json = if count == length {
            let mut items = Vec::with_capacity(length);
            for index in 1..=length {
                items.push(self.encode(&table.raw_get::<Value>(index)?)?);
            }
            Json::Array(items)
        } else {
            let mut object = serde_json::Map::new();
            for pair in table.pairs::<Value, Value>() {
                let (key, item) = pair?;
                let key = match key {
                    Value::String(key) => key.to_string_lossy(),
                    Value::Integer(key) => key.to_string(),
                    Value::Number(key) => key.to_string(),
                    Value::Boolean(key) => key.to_string(),
                    other => {
                        return fail(format!("a {} cannot be an object key", other.type_name()));
                    }
                };
                object.insert(key, self.encode(&item)?);
            }
            Json::Object(object)
        };
        self.path.pop();
        Ok(json)
    }
}

/// Write the presented frame to `path`: PNG, or BMP for a `.bmp` path.
fn write_capture(app: &App, path: &Path) -> Result<()> {
    let image = presented_image(app.frame());
    let bmp = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("bmp"));
    if bmp {
        return capture_bmp(&image, path);
    }
    let png = crate::screenshot::Image::new(image.width, image.height, image.rgba)?.encode_png()?;
    crate::atomic::write(path, &png)
}

/// The `game` global: forwards each method call to the running session's
/// game state, the userdata the mod hooks get as `api`. Indexing it with any
/// name gives a forwarding function, so `game.anything` is never nil; an
/// unknown method fails when called.
struct GameProxy;

impl UserData for GameProxy {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_function(
            MetaMethod::Index,
            |lua, (_proxy, key): (AnyUserData, String)| {
                lua.create_function(move |lua, mut args: MultiValue| {
                    // `game:method(...)` passes the proxy itself first; a
                    // `game.method(...)` call would shift every argument.
                    match args.pop_front() {
                        Some(Value::UserData(this)) if this.is::<GameProxy>() => {}
                        _ => {
                            return Err(mlua::Error::RuntimeError(format!(
                                "call game methods with a colon: game:{key}(...)"
                            )));
                        }
                    }
                    let driver = lua
                        .app_data_ref::<Shared>()
                        .map(|driver| Rc::clone(&driver))
                        .ok_or_else(|| mlua::Error::RuntimeError("no script driver".into()))?;
                    with(&driver, |driver| {
                        if driver.baseline.is_some() && !READ_ONLY.contains(&key.as_str()) {
                            driver.edited = true;
                        }
                        let session = driver.session()?;
                        let result = call_game(lua, &mut session.game, &key, args)?;
                        // The live player follows a position or angle the
                        // call wrote.
                        session.game.sync_player(&mut session.player);
                        Ok(result)
                    })
                })
            },
        );
        methods.add_meta_function(MetaMethod::ToString, |_lua, _proxy: AnyUserData| Ok("game"));
    }
}

/// Call the game-state method `key` on `game` with `args`.
fn call_game(
    lua: &Lua,
    game: &mut game::GameState,
    key: &str,
    mut args: MultiValue,
) -> mlua::Result<MultiValue> {
    lua.scope(|scope| {
        let api = scope.create_userdata_ref_mut(game)?;
        let Value::Function(method) = api.get::<Value>(key)? else {
            return Err(mlua::Error::RuntimeError(format!(
                "game has no method `{key}`"
            )));
        };
        args.push_front(Value::UserData(api));
        method.call::<MultiValue>(args)
    })
}

/// Register the script API's globals.
fn install(lua: &Lua, driver: &Shared, options: &ScriptOptions) -> mlua::Result<()> {
    let globals = lua.globals();

    let arg = lua.create_table()?;
    arg.set(0, options.name.as_str())?;
    for (index, value) in options.args.iter().enumerate() {
        arg.set(index + 1, value.as_str())?;
    }
    globals.set("arg", arg)?;

    globals.set(
        "print",
        lua.create_function(|lua, values: Variadic<Value>| {
            let tostring: Function = lua.globals().get("tostring")?;
            let mut line = String::new();
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    line.push('\t');
                }
                line.push_str(
                    &tostring
                        .call::<mlua::String>(value.clone())?
                        .to_string_lossy(),
                );
            }
            let mut out = std::io::stdout().lock();
            writeln!(out, "{line}").map_err(mlua::Error::external)?;
            Ok(())
        })?,
    )?;

    globals.set(
        "json",
        lua.create_function(|_lua, value: Value| {
            serde_json::to_string(&to_json(&value)?).map_err(mlua::Error::external)
        })?,
    )?;

    let stop = Rc::clone(&driver.borrow().stop);
    globals.set(
        "exit",
        lua.create_function(move |lua, code: Option<i64>| -> mlua::Result<()> {
            let code = code.unwrap_or(0);
            let Ok(code) = u8::try_from(code) else {
                return Err(mlua::Error::RuntimeError(format!(
                    "exit: the status must be 0..=255, not {code}"
                )));
            };
            if stop.exit.get().is_none() {
                stop.exit.set(Some(i32::from(code)));
            }
            halt(lua, &stop);
            Err(mlua::Error::RuntimeError("exit".into()))
        })?,
    )?;

    let shared = Rc::clone(driver);
    globals.set(
        "strict",
        lua.create_function(move |_lua, on: Option<bool>| {
            with(&shared, |driver| {
                driver.strict = on.unwrap_or(true);
                Ok(())
            })
        })?,
    )?;

    let shared = Rc::clone(driver);
    globals.set(
        "load_game_pack",
        lua.create_function(move |_lua, (path, table): (String, Option<Table>)| {
            with(&shared, |driver| {
                let mut options = Options::new("load_game_pack", table);
                let mods: Vec<String> = options.get("mods")?.unwrap_or_default();
                let user_mods: Option<bool> = options.get("user_mods")?;
                let language: Option<String> = options.get("language")?;
                let save_dir: Option<String> = options.get("save_dir")?;
                let quick_turn: Option<bool> = options.get("quick_turn")?;
                let tactical_reload: Option<bool> = options.get("tactical_reload")?;
                let authentic: Option<bool> = options.get("authentic")?;
                let auto_aim: Option<bool> = options.get("auto_aim")?;
                let blood: Option<bool> = options.get("blood")?;
                let difficulty: Option<String> = options.get("difficulty")?;
                let arrange: Option<bool> = options.get("arrange")?;
                let title_special: Option<i8> = options.get("title_special")?;
                let title_gallery: Option<Table> = options.get("title_gallery")?;
                let title_seed: Option<u32> = options.get("title_seed")?;
                let title_keys: Option<Table> = options.get("title_keys")?;
                options.finish()?;
                let title_gallery = title_gallery.as_ref().map(gallery_words).transpose()?;
                let title_keys = title_keys.as_ref().map(title_keys_option).transpose()?;
                let (easy, hard_mode) = match difficulty.as_deref().unwrap_or("normal") {
                    "normal" => (false, false),
                    "easy" => (true, false),
                    "hard" => (false, true),
                    other => {
                        return Err(mlua::Error::RuntimeError(format!(
                            "load_game_pack: difficulty `{other}` is not normal, easy or hard"
                        )));
                    }
                };
                let mods: Vec<PathBuf> = mods.into_iter().map(PathBuf::from).collect();
                // A window shows the user's game: their HD and audio packs
                // too. A headless run stays isolated from what is installed.
                let user_mods = user_mods.unwrap_or(!driver.headless);
                let pack = open_pack(Path::new(&path), &mods, user_mods, language.as_deref())
                    .map_err(engine_error)?;
                // Printed, not counted: a strict script still runs the pack.
                if pack.manifest().is_some_and(converted_by_another_build) {
                    eprintln!(
                        "warning: {path} was converted from other sources than this build's \
                         (its scripts, language or converter): convert it again"
                    );
                }
                let (saves, save_dir) = match save_dir {
                    Some(dir) => (None, PathBuf::from(dir)),
                    None => {
                        let saves = ScratchSaves::create().map_err(engine_error)?;
                        let dir = saves.0.clone();
                        (Some(saves), dir)
                    }
                };
                driver.check_warnings()?;
                driver
                    .open_window(&window_title(pack.manifest()))
                    .map_err(engine_error)?;
                driver.absorb_host_warnings();
                let mut app = App::new(pack, save_dir, false);
                app.player_options = player::Options {
                    quick_turn: quick_turn.unwrap_or(false),
                    tactical_reload: tactical_reload.unwrap_or(false),
                    no_auto_aim: !auto_aim.unwrap_or(true),
                    no_blood: !blood.unwrap_or(true),
                    // A script never writes an autosave.
                    no_autosave: true,
                    authentic: authentic.unwrap_or(false),
                    new_game: crate::ui::NewGameChoice {
                        easy,
                        hard_mode,
                        censorship_off: arrange.unwrap_or(false),
                        extra_player: 0,
                    },
                    title_special,
                    title_gallery,
                    title_seed,
                    title_keys,
                    ..player::Options::default()
                };
                if let Some(window) = driver.window.as_mut() {
                    // The in-game menu's controls page shows the player's
                    // bindings, as a normal launch's does.
                    window.input.no_combo = !app.debug_menu;
                    app.set_bindings(window.input.bindings().clone());
                    warm_gui_fonts(app.pack.language());
                }
                driver.app = Some(app);
                driver.saves = saves;
                driver.baseline = None;
                driver.edited = false;
                driver.arm = false;
                driver.drawn = false;
                Ok(())
            })
        })?,
    )?;

    let shared = Rc::clone(driver);
    globals.set(
        "load_room",
        lua.create_function(move |_lua, (room, table): (String, Option<Table>)| {
            with(&shared, |driver| {
                let mut options = Options::new("load_room", table);
                let player: Option<u8> = options.get("player")?;
                let mansion2: Option<bool> = options.get("mansion2")?;
                options.finish()?;
                let id = room_id(&room, player.unwrap_or(0), mansion2.unwrap_or(false))?;
                let app = driver.app()?;
                let session =
                    GameSession::from_room_with(&app.pack, id, &app.save_dir, app.player_options)
                        .map_err(engine_error)?;
                driver.start(session, false)?;
                Ok(())
            })
        })?,
    )?;

    let shared = Rc::clone(driver);
    globals.set(
        "new_game",
        lua.create_function(move |_lua, table: Option<Table>| {
            with(&shared, |driver| {
                let mut options = Options::new("new_game", table);
                let character: Option<u8> = options.get("character")?;
                let room: Option<String> = options.get("room")?;
                let player: Option<u8> = options.get("player")?;
                let mansion2: Option<bool> = options.get("mansion2")?;
                let interactive: Option<bool> = options.get("interactive")?;
                options.finish()?;
                let character = character.unwrap_or(0);
                let app = driver.app()?;
                let session = match room {
                    Some(room) => {
                        let id = room_id(
                            &room,
                            player.unwrap_or(character & 1),
                            mansion2.unwrap_or(false),
                        )?;
                        GameSession::new_in_room_with(
                            &app.pack,
                            id,
                            character,
                            &app.save_dir,
                            app.player_options,
                        )
                    }
                    None => GameSession::new_with(
                        &app.pack,
                        character,
                        &app.save_dir,
                        app.player_options,
                    ),
                }
                .map_err(engine_error)?;
                if interactive.unwrap_or(false) {
                    let mut session = session;
                    session.place_room_boot_spawn();
                    driver.start(session, false)?;
                } else {
                    driver.start(session, true)?;
                }
                Ok(())
            })
        })?,
    )?;

    let shared = Rc::clone(driver);
    globals.set(
        "load_save",
        lua.create_function(move |_lua, path: String| {
            with(&shared, |driver| {
                let app = driver.app()?;
                let file = save::load_file(Path::new(&path)).map_err(engine_error)?;
                let session = app
                    .continue_session(&file, false, false)
                    .map_err(engine_error)?;
                driver.start(session, true)?;
                Ok(())
            })
        })?,
    )?;

    let shared = Rc::clone(driver);
    globals.set(
        "mark_authored",
        lua.create_function(move |_lua, ()| {
            with(&shared, |driver| {
                if driver.baseline.is_none() {
                    return Err(mlua::Error::RuntimeError(
                        "mark_authored: only between a boot and its first tick".into(),
                    ));
                }
                driver.edited = true;
                Ok(())
            })
        })?,
    )?;

    let shared = Rc::clone(driver);
    globals.set(
        "open_title",
        lua.create_function(move |_lua, table: Option<Table>| {
            let mut options = Options::new("open_title", table);
            let boot = match options.get::<Value>("boot")? {
                None | Some(Value::Nil) => false,
                Some(Value::Boolean(boot)) => boot,
                Some(other) => {
                    return Err(mlua::Error::RuntimeError(format!(
                        "open_title: option `boot`: expected a boolean, got {}",
                        other.type_name()
                    )));
                }
            };
            options.finish()?;
            with(&shared, |driver| {
                driver.drop_pending_voice();
                let app = driver.app()?;
                app.open_title_from(boot, None, None)
                    .map_err(engine_error)?;
                app.pending_movie = None;
                driver.baseline = None;
                driver.edited = false;
                driver.arm = false;
                driver.drawn = false;
                driver.input = InputState::default();
                Ok(())
            })
        })?,
    )?;

    let shared = Rc::clone(driver);
    globals.set(
        "open_ending",
        lua.create_function(move |_lua, (id, table): (u8, Option<Table>)| {
            with(&shared, |driver| {
                if crate::ending::row(id).is_none() {
                    return Err(mlua::Error::RuntimeError(format!(
                        "open_ending: ending {id} is outside 1-7"
                    )));
                }
                let mut options = Options::new("open_ending", table);
                let character: u8 = options.get("character")?.unwrap_or(0);
                options.finish()?;
                if character > 1 {
                    return Err(mlua::Error::RuntimeError(format!(
                        "open_ending: character {character} is not 0 (Chris) or 1 (Jill)"
                    )));
                }
                driver.drop_pending_voice();
                let app = driver.app()?;
                app.open_debug_ending(id, character).map_err(engine_error)?;
                app.pending_movie = None;
                driver.baseline = None;
                driver.edited = false;
                driver.arm = false;
                driver.drawn = false;
                driver.input = InputState::default();
                Ok(())
            })
        })?,
    )?;

    let shared = Rc::clone(driver);
    globals.set(
        "hold",
        lua.create_function(move |_lua, names: Variadic<String>| {
            with(&shared, |driver| {
                driver.held |= buttons(&names)?;
                Ok(())
            })
        })?,
    )?;

    let shared = Rc::clone(driver);
    globals.set(
        "release",
        lua.create_function(move |_lua, names: Variadic<String>| {
            with(&shared, |driver| {
                if names.is_empty() {
                    driver.held = 0;
                } else {
                    driver.held &= !buttons(&names)?;
                }
                Ok(())
            })
        })?,
    )?;

    let shared = Rc::clone(driver);
    globals.set(
        "press",
        lua.create_function(move |_lua, names: Variadic<String>| {
            with(&shared, |driver| {
                driver.pressed |= buttons(&names)?;
                Ok(())
            })
        })?,
    )?;

    let shared = Rc::clone(driver);
    globals.set(
        "press_key",
        lua.create_function(move |_lua, name: String| {
            with(&shared, |driver| {
                let source = crate::controls::Source::parse(&name)
                    .map_err(|err| mlua::Error::RuntimeError(format!("press_key: {err:#}")))?;
                if !matches!(source, crate::controls::Source::Key(_)) && !source.is_pad() {
                    return Err(mlua::Error::RuntimeError(format!(
                        "press_key: `{name}` is not a keyboard key or a gamepad source"
                    )));
                }
                driver.pressed_source = Some(source);
                Ok(())
            })
        })?,
    )?;

    let shared = Rc::clone(driver);
    globals.set(
        "pad",
        lua.create_function(move |_lua, word: Option<u64>| {
            with(&shared, |driver| {
                let previous = driver.held;
                if let Some(word) = word {
                    driver.held = word;
                }
                Ok(previous)
            })
        })?,
    )?;

    let shared = Rc::clone(driver);
    globals.set(
        "tick",
        lua.create_function(move |_lua, count: Option<u32>| {
            with(&shared, |driver| {
                for _ in 0..count.unwrap_or(1) {
                    driver.tick()?;
                }
                Ok(())
            })
        })?,
    )?;

    let shared = Rc::clone(driver);
    globals.set(
        "tick_until",
        lua.create_function(move |_lua, (until, max): (Function, u32)| {
            for ticks in 0..=max {
                if until.call::<bool>(())? {
                    return Ok(ticks);
                }
                if ticks < max {
                    with(&shared, Driver::tick)?;
                }
            }
            Err(mlua::Error::RuntimeError(format!(
                "tick_until: the condition did not hold within {max} ticks"
            )))
        })?,
    )?;

    let shared = Rc::clone(driver);
    globals.set(
        "mode",
        lua.create_function(move |_lua, ()| {
            with(&shared, |driver| {
                Ok(driver
                    .app
                    .as_ref()
                    .map_or("none", |app| mode_name(&app.mode)))
            })
        })?,
    )?;

    let shared = Rc::clone(driver);
    globals.set(
        "capture",
        lua.create_function(move |_lua, path: String| {
            with(&shared, |driver| {
                if matches!(driver.app()?.mode, Mode::Boot) {
                    return Err(mlua::Error::RuntimeError(
                        "capture: nothing is booted".into(),
                    ));
                }
                driver.settle_authoring()?;
                let drawn = driver.drawn;
                let app = driver.app()?;
                if !drawn {
                    app.draw();
                }
                write_capture(app, Path::new(&path)).map_err(engine_error)?;
                driver.drawn = true;
                Ok(())
            })
        })?,
    )?;

    let shared = Rc::clone(driver);
    globals.set(
        "save_game",
        lua.create_function(move |_lua, path: String| {
            with(&shared, |driver| {
                driver.settle_authoring()?;
                driver
                    .session()?
                    .write_save(Path::new(&path))
                    .map_err(engine_error)?;
                Ok(())
            })
        })?,
    )?;

    let shared = Rc::clone(driver);
    globals.set(
        "re2_save_image",
        lua.create_function(move |lua, reference: Option<String>| {
            with(&shared, |driver| {
                driver.settle_authoring()?;
                let app = driver.app()?;
                let Mode::Play(session) = &app.mode else {
                    return Err(mlua::Error::RuntimeError(
                        "re2_save_image: no game session is running".into(),
                    ));
                };
                if !session.game.is_re2_room {
                    return Err(mlua::Error::RuntimeError(
                        "re2_save_image: the session isn't an RE 2 game".into(),
                    ));
                }
                let entry = crate::bgm2::INIT_TABLE_ENTRY;
                let table = app.pack.read(entry).map_err(|err| {
                    mlua::Error::RuntimeError(format!(
                        "re2_save_image: the pack has no {entry}, the record the image starts from: {err}"
                    ))
                })?;
                let row = re2_image::init_row(&session.game);
                let record = re2_image::init_record(&table, row).ok_or_else(|| {
                    mlua::Error::RuntimeError(format!(
                        "re2_save_image: {entry} has no row {row} ({} bytes)",
                        table.len()
                    ))
                })?;
                let image = re2_image::build(&session.game, Some(record));
                let ours = lua.create_string(image)?;
                let Some(reference) = reference else {
                    return Ok((ours, None));
                };
                let theirs = std::fs::read(&reference)
                    .with_context(|| format!("re2_save_image: reading {reference}"))
                    .map_err(engine_error)?;
                let theirs: [u8; re2_image::IMAGE_LEN] =
                    theirs.try_into().map_err(|bytes: Vec<u8>| {
                        mlua::Error::RuntimeError(format!(
                            "re2_save_image: {reference} is {} bytes, not {}",
                            bytes.len(),
                            re2_image::IMAGE_LEN
                        ))
                    })?;
                let list = differences_table(lua, &image, &theirs)?;
                Ok((ours, Some(list)))
            })
        })?,
    )?;

    let shared = Rc::clone(driver);
    globals.set(
        "re2_saves",
        lua.create_function(move |lua, references: Option<Table>| {
            with(&shared, |driver| {
                let app = driver.app()?;
                let Mode::Play(session) = &app.mode else {
                    return Err(mlua::Error::RuntimeError(
                        "re2_saves: no game session is running".into(),
                    ));
                };
                let table = app
                    .pack
                    .read(crate::bgm2::INIT_TABLE_ENTRY)
                    .map_err(|err| {
                        mlua::Error::RuntimeError(format!(
                            "re2_saves: the pack has no {}: {err}",
                            crate::bgm2::INIT_TABLE_ENTRY
                        ))
                    })?;
                let mut paths: Vec<PathBuf> = std::fs::read_dir(&session.save_dir)
                    .map(|entries| {
                        entries
                            .filter_map(|entry| Some(entry.ok()?.path()))
                            .collect()
                    })
                    .unwrap_or_default();
                paths.sort();
                let list = lua.create_table()?;
                for path in paths {
                    let row = lua.create_table()?;
                    row.set(
                        "name",
                        path.file_name()
                            .map(|name| name.to_string_lossy().into_owned()),
                    )?;
                    let read = crate::save::load_file(&path)
                        .map_err(|err| format!("{err:#}"))
                        .and_then(|file| re2_image::saved_state(&file));
                    match read {
                        Err(why) => row.set("error", why)?,
                        Ok(game) => {
                            let record = re2_image::init_record(&table, re2_image::init_row(&game));
                            let image = re2_image::build(&game, record);
                            let count = u16::from_le_bytes([image[0x220], image[0x221]]);
                            row.set("count", count)?;
                            let reference = match &references {
                                Some(refs) => refs.get::<Option<String>>(count)?,
                                None => None,
                            };
                            if let Some(reference) = reference {
                                let theirs = std::fs::read(&reference)
                                    .with_context(|| format!("re2_saves: reading {reference}"))
                                    .map_err(engine_error)?;
                                let theirs: [u8; re2_image::IMAGE_LEN] =
                                    theirs.try_into().map_err(|_| {
                                        mlua::Error::RuntimeError(format!(
                                            "re2_saves: {reference} isn't a save image"
                                        ))
                                    })?;
                                row.set("differ", differences_table(lua, &image, &theirs)?)?;
                            }
                        }
                    }
                    list.push(row)?;
                }
                Ok(list)
            })
        })?,
    )?;

    let shared = Rc::clone(driver);
    globals.set(
        "trace_start",
        lua.create_function(move |_lua, (path, table): (String, Option<Table>)| {
            with(&shared, |driver| {
                let mut options = Options::new("trace_start", table);
                let all_frames: Option<bool> = options.get("all_frames")?;
                let sound: Option<bool> = options.get("sound")?;
                let voice_ticks: Option<Vec<u32>> = options.get("voice_ticks")?;
                let sound_wav: Option<String> = options.get("sound_wav")?;
                let film_pads: Option<Vec<u32>> = options.get("film_pads")?;
                options.finish()?;
                driver.close_trace().map_err(engine_error)?;
                let screen = driver
                    .app
                    .as_ref()
                    .is_some_and(|app| !matches!(app.mode, Mode::Boot | Mode::Play(_)));
                // On a screen: for the game it starts, once the trace is made.
                let (screen_voice, voice_ticks) = if screen {
                    (voice_ticks, None)
                } else {
                    (None, voice_ticks)
                };
                let (screen_sound, sound) = if screen {
                    (sound.unwrap_or(false), None)
                } else {
                    (false, sound)
                };
                if sound.unwrap_or(false)
                    || voice_ticks.is_some()
                    || sound_wav.is_some()
                    || film_pads.is_some()
                {
                    let app = driver.app()?;
                    let Mode::Play(session) = &mut app.mode else {
                        return Err(mlua::Error::RuntimeError(
                            "trace_start: sound, voice_ticks, sound_wav and film_pads need a game \
                             session (sound and voice_ticks a screen too)"
                                .into(),
                        ));
                    };
                    if let Some(pads) = film_pads {
                        // The session's flag reaches the scripts as each tick
                        // starts; an authored rerun before then sees it too.
                        session.films = true;
                        session.game.re2.films = true;
                        session.film_pads = Some(pads.into());
                    }
                    session.sound_device |= sound.unwrap_or(false);
                    if sound_wav.is_some() {
                        session.music = Some(Mixer::capture());
                        session.start_audio(&app.pack);
                    }
                }
                let writer = if screen {
                    driver.app()?.ui_sounds.clear();
                    trace::TraceWriter::create_screen(Path::new(&path))
                } else {
                    trace::TraceWriter::create(Path::new(&path))
                }
                .map_err(engine_error)?;
                // The voice lengths go with the trace, so only once it is made.
                if let Some(ticks) = screen_voice {
                    driver.app()?.pending_voice_ticks = Some(ticks.into());
                }
                if screen_sound {
                    driver.app()?.pending_sound = true;
                }
                if let Some(ticks) = voice_ticks
                    && let Mode::Play(session) = &mut driver.app()?.mode
                {
                    session.game.re2.voice_ticks = ticks.into();
                }
                let unpaced_voice = !sound.unwrap_or(screen_sound) && sound_wav.is_none();
                driver.trace = Some(ScriptTrace {
                    writer,
                    all_frames: all_frames.unwrap_or(false),
                    screen,
                    unpaced_voice,
                    wav: sound_wav.map(|path| WavCapture {
                        path: PathBuf::from(path),
                        due: 0.0,
                        ticks: String::new(),
                        pcm: Vec::new(),
                    }),
                });
                Ok(())
            })
        })?,
    )?;

    let shared = Rc::clone(driver);
    globals.set(
        "trace_ticks",
        lua.create_function(move |_lua, ()| {
            with(&shared, |driver| {
                Ok(driver.trace.as_ref().map(|trace| trace.writer.tick_index()))
            })
        })?,
    )?;

    let shared = Rc::clone(driver);
    globals.set(
        "trace_stop",
        lua.create_function(move |_lua, ()| {
            with(&shared, |driver| driver.close_trace().map_err(engine_error))
        })?,
    )?;

    #[cfg(feature = "bot")]
    bot_api::install(lua, driver)?;
    #[cfg(not(feature = "bot"))]
    for name in bot_api::FUNCTIONS {
        globals.set(
            name,
            lua.create_function(move |_lua, _: MultiValue| -> mlua::Result<()> {
                Err(mlua::Error::RuntimeError(format!(
                    "{name}: this build has no player bot: it was built without the `bot` feature"
                )))
            })?,
        )?;
    }
    globals.set("game", lua.create_userdata(GameProxy)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval_json(source: &str) -> String {
        let lua = Lua::new();
        let value: Value = lua.load(source).eval().unwrap();
        serde_json::to_string(&to_json(&value).unwrap()).unwrap()
    }

    /// A pack this build would convert as it is passes; one with another
    /// fingerprint or revision, or with only one of them, is another
    /// build's; one with neither (no conversion's) is left alone.
    #[test]
    fn a_pack_from_other_sources_is_another_builds() {
        let (revision, hash) = crate::convert::Game::Re2.packed();
        let mut manifest = crate::manifest::Manifest::base("re2");
        assert!(!converted_by_another_build(&manifest), "no conversion's");
        manifest.convert_version = Some(revision);
        assert!(converted_by_another_build(&manifest), "no fingerprint");
        manifest.source_hash = Some(hash.clone());
        assert!(!converted_by_another_build(&manifest));
        manifest.source_hash = Some("0123456789abcdef".into());
        assert!(converted_by_another_build(&manifest), "other scripts");
        manifest.source_hash = Some(hash);
        manifest.convert_version = Some(revision + 1);
        assert!(converted_by_another_build(&manifest), "another revision");
    }

    #[test]
    fn json_encodes_arrays_objects_and_scalars() {
        assert_eq!(eval_json("{1, 2, 3}"), "[1,2,3]");
        assert_eq!(eval_json("{b = 1, a = 'x'}"), r#"{"a":"x","b":1}"#);
        assert_eq!(eval_json("{}"), "[]");
        assert_eq!(
            eval_json("{[1] = true, [3] = false}"),
            r#"{"1":true,"3":false}"#
        );
        assert_eq!(eval_json("1.5"), "1.5");
        assert_eq!(eval_json("0/0"), "null");
        assert_eq!(eval_json("'a\"b'"), r#""a\"b""#);
    }

    #[test]
    fn json_stops_on_a_value_that_shares_its_subtables() {
        let lua = Lua::new();
        let value: Value = lua
            .load("local t = {} for _ = 1, 60 do t = {t, t} end return t")
            .eval()
            .unwrap();
        assert!(to_json(&value).is_err());
    }

    #[test]
    fn json_rejects_a_cycle() {
        let lua = Lua::new();
        let value: Value = lua.load("local t = {} t.t = t return t").eval().unwrap();
        assert!(to_json(&value).is_err());
    }

    #[test]
    fn only_the_players_host_keys_reach_a_scripted_tick() {
        let mut ui = UiInput {
            confirm: true,
            ..UiInput::default()
        };
        let player = UiInput {
            system_menu: true,
            debug_menu: true,
            return_title: true,
            cancel: true,
            up: true,
            start: true,
            ..UiInput::default()
        };
        assert!(add_host_keys(&mut ui, &player));
        assert_eq!(
            ui,
            UiInput {
                confirm: true,
                system_menu: true,
                debug_menu: true,
                return_title: true,
                any: true,
                ..UiInput::default()
            }
        );
        assert!(!add_host_keys(&mut ui, &UiInput::default()));
    }

    use crate::engine::tests::{TempDir, film_pack};

    /// A headless driver over a game session, as a script's
    /// `load_game_pack` and `new_game` leave it, in a pack with every ending
    /// film or none.
    fn playing_driver(dir: &TempDir, films: bool) -> Driver {
        let films: Vec<(u8, &str)> = if films {
            (14..=27).map(|id| (id, "film")).collect()
        } else {
            Vec::new()
        };
        let pack = film_pack(dir, &films);
        let mut app = App::new(pack, dir.0.join("saves"), false);
        let session = GameSession::new(&app.pack, 0, &app.save_dir).unwrap();
        app.start_session(session);
        let stop = Rc::new(Stop {
            exit: Cell::new(None),
            timed_out: Cell::new(false),
            deadline: None,
            timeout: Duration::default(),
        });
        let mut driver = Driver::new(true, stop);
        // Other tests' warnings share the counter.
        driver.strict = false;
        driver.app = Some(app);
        driver
    }

    fn trace(dir: &TempDir, unpaced_voice: bool, wav: Option<&str>) -> ScriptTrace {
        ScriptTrace {
            writer: trace::TraceWriter::create(&dir.0.join("run.trace")).unwrap(),
            all_frames: false,
            screen: false,
            wav: wav.map(|name| WavCapture {
                path: dir.0.join(name),
                due: 0.0,
                ticks: String::new(),
                pcm: Vec::new(),
            }),
            unpaced_voice,
        }
    }

    fn play_session(driver: &mut Driver) -> &mut GameSession {
        match &mut driver.app.as_mut().unwrap().mode {
            Mode::Play(session) => session,
            _ => panic!("no session"),
        }
    }

    /// A trace without voice or sound options starts before any pack is
    /// open, as `load_game_pack` comes after it.
    #[test]
    fn a_trace_starts_before_a_pack_is_open() {
        let dir = TempDir::new();
        let stop = Rc::new(Stop {
            exit: Cell::new(None),
            timed_out: Cell::new(false),
            deadline: None,
            timeout: Duration::default(),
        });
        let lua = new_state(&stop).unwrap();
        let options = ScriptOptions {
            name: "test".into(),
            source: Vec::new(),
            args: Vec::new(),
            headless: true,
            timeout: None,
        };
        let shared: Shared = Rc::new(RefCell::new(Driver::new(true, stop)));
        lua.set_app_data(Rc::clone(&shared));
        install(&lua, &shared, &options).unwrap();
        let path = dir
            .0
            .join("run.trace")
            .display()
            .to_string()
            .replace('\\', "/");
        lua.load(format!("trace_start({path:?})")).exec().unwrap();
        assert!(shared.borrow().trace.is_some());
    }

    /// Voice lengths and `sound` a trace asks for on a screen wait for the
    /// game that screen starts, and only while their trace runs:
    /// `trace_stop`, a trace started over it, a failed `trace_start` and a
    /// script's own title or game drop them.
    #[test]
    fn a_screens_voice_lengths_go_with_their_trace() {
        let dir = TempDir::new();
        let mut driver = playing_driver(&dir, true);
        let stop = Rc::new(Stop {
            exit: Cell::new(None),
            timed_out: Cell::new(false),
            deadline: None,
            timeout: Duration::default(),
        });
        let lua = new_state(&stop).unwrap();
        let options = ScriptOptions {
            name: "test".into(),
            source: Vec::new(),
            args: Vec::new(),
            headless: true,
            timeout: None,
        };
        driver
            .app
            .as_mut()
            .unwrap()
            .open_debug_ending(1, 0)
            .unwrap();
        let shared: Shared = Rc::new(RefCell::new(driver));
        lua.set_app_data(Rc::clone(&shared));
        install(&lua, &shared, &options).unwrap();
        let trace = dir
            .0
            .join("a.trace")
            .display()
            .to_string()
            .replace('\\', "/");
        let missing = dir
            .0
            .join("no/a.trace")
            .display()
            .to_string()
            .replace('\\', "/");
        let run = |code: String| lua.load(code).exec();
        let pending = || {
            shared
                .borrow()
                .app
                .as_ref()
                .unwrap()
                .pending_voice_ticks
                .clone()
                .map(Vec::from)
        };
        let start = format!("trace_start({trace:?}, {{ voice_ticks = {{ 18, 22 }} }})");
        run(start.clone()).unwrap();
        assert_eq!(pending(), Some(vec![18, 22]), "held for the screen's game");
        run("trace_stop()".into()).unwrap();
        assert_eq!(pending(), None, "trace_stop");
        run(start.clone()).unwrap();
        run(format!("trace_start({trace:?})")).unwrap();
        assert_eq!(pending(), None, "a trace started over it");
        run(start.clone()).unwrap();
        let failed = format!("trace_start({missing:?}, {{ voice_ticks = {{ 18 }} }})");
        assert!(run(failed).is_err());
        assert_eq!(pending(), None, "a trace that couldn't be made");
        run(start.clone()).unwrap();
        run("open_ending(1)".into()).unwrap();
        assert_eq!(pending(), None, "the script's own screen");
        run(start.clone()).unwrap();
        run("new_game()".into()).unwrap();
        assert_eq!(pending(), None, "the script's own game");
        // The screen hands its game the lengths; the trace's end takes the
        // ones its lines haven't used.
        let session_voice = || match &shared.borrow().app.as_ref().unwrap().mode {
            Mode::Play(session) => Vec::from(session.game.re2.voice_ticks.clone()),
            _ => panic!("no session"),
        };
        run("open_ending(1)".into()).unwrap();
        run(start).unwrap();
        {
            let mut driver = shared.borrow_mut();
            let app = driver.app.as_mut().unwrap();
            let session = GameSession::new(&app.pack, 0, &app.save_dir).unwrap();
            app.start_session(session);
        }
        assert_eq!(session_voice(), [18, 22]);
        let sound_device = || match &shared.borrow().app.as_ref().unwrap().mode {
            Mode::Play(session) => session.sound_device,
            _ => panic!("no session"),
        };
        assert!(!sound_device(), "no `sound` asked for");
        run("trace_stop()".into()).unwrap();
        assert!(session_voice().is_empty(), "trace_stop in play");
        // `sound` on a screen is the game's that screen starts, and goes
        // with its trace as the lengths do.
        let pending_sound = || shared.borrow().app.as_ref().unwrap().pending_sound;
        run("open_ending(1)".into()).unwrap();
        let sounded = format!("trace_start({trace:?}, {{ sound = true }})");
        run(sounded.clone()).unwrap();
        assert!(pending_sound(), "held for the screen's game");
        run("trace_stop()".into()).unwrap();
        assert!(!pending_sound(), "trace_stop");
        run(sounded).unwrap();
        {
            let mut driver = shared.borrow_mut();
            let app = driver.app.as_mut().unwrap();
            let session = GameSession::new(&app.pack, 0, &app.save_dir).unwrap();
            app.start_session(session);
        }
        assert!(sound_device(), "the screen's game has a sound device");
        assert!(!pending_sound());
        run("trace_stop()".into()).unwrap();
        // In play, a trace that can't be made sets none.
        let failed = format!("trace_start({missing:?}, {{ voice_ticks = {{ 18 }} }})");
        assert!(run(failed).is_err());
        assert!(session_voice().is_empty(), "a failed trace_start in play");
        run(format!(
            "trace_start({trace:?}, {{ voice_ticks = {{ 30 }} }})"
        ))
        .unwrap();
        assert_eq!(session_voice(), [30]);
        run("trace_stop()".into()).unwrap();
        assert!(session_voice().is_empty());
    }

    #[test]
    fn each_tick_sets_the_voice_pacing_from_the_running_trace() {
        let dir = TempDir::new();
        let mut driver = playing_driver(&dir, false);
        driver.tick().unwrap();
        assert!(
            !play_session(&mut driver).voice_cache.unpaced,
            "no trace: paced"
        );

        // The oracle's trace: the reference's sound is silenced.
        driver.trace = Some(trace(&dir, true, None));
        driver.tick().unwrap();
        assert!(play_session(&mut driver).voice_cache.unpaced);

        driver.close_trace().unwrap();
        driver.tick().unwrap();
        assert!(
            !play_session(&mut driver).voice_cache.unpaced,
            "paced again"
        );

        // A trace with `sound` keeps the lines paced.
        driver.trace = Some(trace(&dir, false, None));
        driver.tick().unwrap();
        assert!(!play_session(&mut driver).voice_cache.unpaced);
    }

    /// A script's film ends with nothing held: the next tick takes a key held
    /// through it as a press, the port's own as well.
    #[test]
    fn a_key_held_through_a_script_s_film_is_pressed_anew() {
        let dir = TempDir::new();
        let mut driver = playing_driver(&dir, false);
        driver.held = 1;
        driver.tick().unwrap();
        assert_eq!(driver.input.held, 1, "the next tick's edge leaves it out");
        play_session(&mut driver).film_ended = true;
        driver.tick().unwrap();
        assert!(!play_session(&mut driver).film_ended);
        assert_eq!(driver.input.held, 0, "the next tick's edge has it");
    }

    /// Three traced ticks with a sound capture, then the escape's clear:
    /// RESULT opens and the WAV still holds the three ticks' mix.
    fn clear_to_result(films: bool) {
        let dir = TempDir::new();
        let mut driver = playing_driver(&dir, films);
        let app = driver.app.as_mut().unwrap();
        let Mode::Play(session) = &mut app.mode else {
            unreachable!();
        };
        session.music = Some(Mixer::capture());
        session.start_audio(&app.pack);
        driver.trace = Some(trace(&dir, false, Some("run.wav")));
        for _ in 0..3 {
            driver.tick().unwrap();
        }

        // The death variant ends in the ending's film chain. The driver drops
        // the films (with none in the pack the chain opens RESULT inside the
        // app's update), and the chain runs on to RESULT.
        play_session(&mut driver).clear_game_requested = Some(4);
        driver.tick().unwrap();
        let app = driver.app.as_ref().unwrap();
        assert!(matches!(app.mode, Mode::Ending(_)), "RESULT is open");
        assert!(app.pending_movie.is_none() && !app.chain_active);

        // RESULT took the session's mixer, but not the mix: three ticks of
        // 735 stereo frames, four bytes each, after the header.
        driver.close_trace().unwrap();
        let wav = std::fs::read(dir.0.join("run.wav")).unwrap();
        assert_eq!(wav.len(), 44 + 3 * 735 * 4, "the mix was kept");
        let ticks = std::fs::read_to_string(dir.0.join("run.ticks")).unwrap();
        assert_eq!(ticks, "0 0.0333\n1 0.0667\n2 0.1000\n");
    }

    #[test]
    fn a_scripted_clear_reaches_the_result_screen_and_keeps_its_mix() {
        clear_to_result(true);
    }

    #[test]
    fn a_clear_with_no_films_keeps_its_mix_too() {
        clear_to_result(false);
    }

    #[test]
    fn the_titles_gallery_words_are_five_whole_32_bit_values_or_an_error() {
        let lua = Lua::new();
        let words = |source: &str| {
            let table: Table = lua.load(source).eval().unwrap();
            gallery_words(&table).map_err(|error| error.to_string())
        };
        assert_eq!(
            words("return { 0x90000000, 0x10, 0, 0, 0xFFFFFFFF }"),
            Ok([0x9000_0000, 0x10, 0, 0, u32::MAX])
        );
        for (source, problem) in [
            ("return {}", "is five words"),
            ("return { 1, 2, 3, 4 }", "is five words"),
            ("return { 1, 2, 3, 4, 5, 6 }", "is five words"),
            ("return { 1, 2, 3, 4, 5, [7] = 6 }", "is five words"),
            ("return { 1, 2, 3, 4, 5, more = 6 }", "is five words"),
            ("return { 1, 2, nil, 4, 5 }", "is five words"),
            ("return { 1, 2, 3, 4, 1.5 }", "word 5 isn't an integer"),
            ("return { 1, '2', 3, 4, 5 }", "word 2 isn't an integer"),
            ("return { -1, 2, 3, 4, 5 }", "word 1 isn't a 32-bit value"),
            (
                "return { 1, 2, 3, 0x100000000, 5 }",
                "word 4 isn't a 32-bit value",
            ),
        ] {
            let error = words(source).unwrap_err();
            assert!(error.contains(problem), "{source}: {error}");
        }
    }

    #[test]
    fn button_names_cover_the_input_names_and_the_tank_aliases() {
        assert_eq!(button_bit("forward").unwrap(), KEY_UP);
        assert_eq!(button_bit("action").unwrap(), KEY_CONFIRM);
        assert!(button_bit("jump").is_err());
    }
}
