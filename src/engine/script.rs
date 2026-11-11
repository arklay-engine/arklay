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
//! A tick is one app update, the unit `--ticks` counts: a fixed 30 Hz tick in
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
    /// Trace the frames of a door transition and the death screen too, as a
    /// `--trace` run with `--input` does.
    all_frames: bool,
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
    /// The booted session has not ticked yet: what its state was before
    /// the script's `game` edits, so the first tick (or a `save_game` or
    /// `capture`) reruns what they invalidate once, as `--setup-script` does.
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
            baseline: None,
            edited: false,
            arm: false,
            drawn: false,
            trace: None,
            window: None,
            player_overlay: false,
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
        if std::mem::take(&mut self.edited) {
            self.session()?.finish_authoring(baseline, true);
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
    fn start(&mut self, session: Box<GameSession>, arm: bool) -> mlua::Result<()> {
        let baseline = session.author_baseline();
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
                "nothing is booted: call load_room, new_game or load_save first".into(),
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
        if arm && let Mode::Play(session) = &mut app.mode {
            session.arm_new_game_boot();
        }
        let was_play = matches!(app.mode, Mode::Play(_));
        let in_transition =
            matches!(&app.mode, Mode::Play(session) if session.transition.is_some());
        let screen_before =
            matches!(&app.mode, Mode::Play(session) if session.death_screen.is_some());
        let mut tick = self.input.tick_scripted(word);
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
        // Films never play: the frames stay deterministic and audio-free, as
        // on the `--trace` path.
        if let Mode::Play(session) = &mut app.mode {
            session.movie = None;
        }
        app.pending_movie = None;
        // A headless bot run draws nothing: no frame is shown, traced or
        // (until `capture` draws its own) written, and a game is long. A
        // window draws the frames it presents itself, on its renderer.
        self.drawn = self.trace.is_some()
            || !(self.window.is_some() || (self.headless && app.bot.is_some()));
        if self.drawn {
            app.draw();
        }
        if let Some(trace) = self.trace.as_mut()
            && let Mode::Play(session) = &app.mode
            && was_play
            && ((!in_transition && !screen_before) || trace.all_frames)
        {
            trace
                .writer
                .write_tick(&session.trace_tick(&app.framebuffer))
                .map_err(engine_error)?;
        }
        if self.window.is_some() {
            self.drawn |= self.present()?;
        } else {
            app.post_present();
        }
        self.pump()?;
        self.check_warnings()
    }

    /// Attach the open pack's bot to the running session (`bot_start`).
    fn bot_start(&mut self, start: &BotStart) -> mlua::Result<()> {
        let app = self.app()?;
        if !matches!(app.mode, Mode::Play(_)) {
            return Err(mlua::Error::RuntimeError(
                "bot_start: no game session is running: call new_game, load_room or load_save \
                 first"
                    .into(),
            ));
        }
        bot::set_log(start.log);
        // The bot skips scenes with the buttons a player would, and a scene
        // it skips runs to its end in one update, so where control returns
        // never depends on the machine's load.
        app.enable_skips();
        app.attach_bot(
            start.risk,
            start.goal_room.as_deref(),
            start.seed,
            &start.params,
            start.invincible,
        )
        .map_err(engine_error)?;
        if let Some(path) = &start.memory {
            let json = std::fs::read_to_string(path)
                .with_context(|| format!("reading the bot memory {path}"))
                .and_then(|text| {
                    serde_json::from_str(&text)
                        .with_context(|| format!("parsing the bot memory {path}"))
                })
                .map_err(engine_error)?;
            if let Some(runtime) = app.bot.take() {
                app.bot = Some(runtime.with_memory(json).map_err(engine_error)?);
            }
        }
        if let Some(runtime) = app.bot.as_ref() {
            for (key, value) in &start.remember {
                runtime.remember(key, value);
            }
        }
        app.unbounded_cutscene_skip = true;
        if let Mode::Play(session) = &mut app.mode {
            session.cutscene_skip_burst = None;
        }
        self.bot_started = Some(Instant::now());
        Ok(())
    }

    /// The attached bot.
    fn bot(&mut self) -> mlua::Result<&bot::BotRuntime> {
        self.app()?.bot.as_ref().ok_or_else(|| {
            mlua::Error::RuntimeError("no bot is attached: call bot_start or bot_run first".into())
        })
    }

    /// The attached bot's report (`bot_stats`), as `--bot-stats` writes it.
    fn bot_report(&mut self, outcome: &str) -> mlua::Result<serde_json::Value> {
        let elapsed = self
            .bot_started
            .map_or(0.0, |started| started.elapsed().as_secs_f64());
        let facts = self.app()?.bot_run_facts();
        Ok(self.bot()?.stats_json(outcome, elapsed, facts))
    }

    /// How the attached bot's run stands: `running`, `finished` (the bot
    /// ended it, with its reason) or `death` (the player died with no save
    /// to load).
    fn bot_outcome(&mut self) -> mlua::Result<&'static str> {
        if self.bot()?.finished() {
            return Ok("finished");
        }
        let app = self.app()?;
        let stranded = app.bot_player_dead() && !app.bot.as_ref().is_some_and(|bot| bot.has_save());
        Ok(if stranded { "death" } else { "running" })
    }

    /// Tick until the attached bot's run ends or `max_ticks` more ticks have
    /// run (`bot_run`): the outcome, `max_ticks` when the budget ran out.
    fn bot_play(&mut self, max_ticks: u64) -> mlua::Result<&'static str> {
        // A bot without a save gets a moment to see its death and finish
        // with its own reason.
        let mut dead_ticks = 0u32;
        for _ in 0..max_ticks {
            match self.bot_outcome()? {
                "running" => dead_ticks = 0,
                "death" if dead_ticks < 2 => dead_ticks += 1,
                outcome => return Ok(outcome),
            }
            self.tick()?;
        }
        Ok(match self.bot_outcome()? {
            "running" => "max_ticks",
            outcome => outcome,
        })
    }

    /// Flush the trace and close the window at the end of the run.
    fn finish(&mut self) -> Result<()> {
        if let Some(mut trace) = self.trace.take() {
            trace.writer.flush()?;
        }
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

/// What `bot_start` and `bot_run` take to attach the bot.
struct BotStart {
    risk: f64,
    goal_room: Option<String>,
    seed: Option<u32>,
    params: Vec<(String, String)>,
    invincible: bool,
    log: bool,
    /// A memory file an earlier run wrote with `bot_memory`, to start from.
    memory: Option<String>,
    /// Notes to put in the bot's memory before its first tick.
    remember: Vec<(String, String)>,
}

impl BotStart {
    /// Read the attach options of `function`'s table; the caller reads its
    /// own options and then calls [`Options::finish`].
    fn read(options: &mut Options<'_>) -> mlua::Result<Self> {
        let function = options.function;
        let risk: Option<f64> = options.get("risk")?;
        let goal_room: Option<String> = options.get("goal_room")?;
        let seed: Option<u32> = options.get("seed")?;
        let params: Option<Table> = options.get("params")?;
        let invincible: Option<bool> = options.get("invincible")?;
        let log: Option<bool> = options.get("log")?;
        let memory: Option<String> = options.get("memory")?;
        let remember: Option<Table> = options.get("remember")?;
        let risk = risk.unwrap_or(bot::DEFAULT_RISK);
        if !(0.0..=1.0).contains(&risk) {
            return Err(mlua::Error::RuntimeError(format!(
                "{function}: risk {risk} is not within 0..=1"
            )));
        }
        // Sorted by name, so the run does not depend on the table's order;
        // the pack's parameter file checks the names and values.
        let strings = |option: &str, table: Option<Table>| -> mlua::Result<Vec<(String, String)>> {
            let mut pairs = Vec::new();
            let Some(table) = table else {
                return Ok(pairs);
            };
            for pair in table.pairs::<String, Value>() {
                let (name, value) = pair.map_err(|err| {
                    mlua::Error::RuntimeError(format!("{function}: option `{option}`: {err}"))
                })?;
                let value = match value {
                    Value::String(text) => text.to_string_lossy(),
                    Value::Boolean(on) => if on { "yes" } else { "no" }.to_string(),
                    Value::Integer(number) => number.to_string(),
                    other => {
                        return Err(mlua::Error::RuntimeError(format!(
                            "{function}: `{name}` of `{option}` is a {}, not a string",
                            other.type_name()
                        )));
                    }
                };
                pairs.push((name, value));
            }
            pairs.sort();
            Ok(pairs)
        };
        Ok(Self {
            risk,
            goal_room,
            seed,
            params: strings("params", params)?,
            invincible: invincible.unwrap_or(false),
            log: log.unwrap_or(true),
            memory,
            remember: strings("remember", remember)?,
        })
    }
}

/// A JSON value as the Lua value [`to_json`] would encode back to it: an
/// array as a sequence, an object as a table, `null` as nil.
fn from_json(lua: &Lua, value: &serde_json::Value) -> mlua::Result<Value> {
    Ok(match value {
        serde_json::Value::Null => Value::Nil,
        serde_json::Value::Bool(on) => Value::Boolean(*on),
        serde_json::Value::Number(number) => match number.as_i64() {
            Some(integer) => Value::Integer(integer),
            None => Value::Number(number.as_f64().unwrap_or(f64::NAN)),
        },
        serde_json::Value::String(text) => Value::String(lua.create_string(text)?),
        serde_json::Value::Array(items) => {
            let table = lua.create_table_with_capacity(items.len(), 0)?;
            for (index, item) in items.iter().enumerate() {
                table.raw_set(index + 1, from_json(lua, item)?)?;
            }
            Value::Table(table)
        }
        serde_json::Value::Object(fields) => {
            let table = lua.create_table_with_capacity(0, fields.len())?;
            for (key, item) in fields {
                table.raw_set(key.as_str(), from_json(lua, item)?)?;
            }
            Value::Table(table)
        }
    })
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

/// The key bit a button name maps to: the `--input` names plus `forward`
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
                options.finish()?;
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
                    },
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
                driver.start(session, true)?;
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
                let session = GameSession::from_save_with(
                    &app.pack,
                    &file,
                    &app.save_dir,
                    app.player_options,
                )
                .map_err(engine_error)?;
                driver.start(session, true)?;
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
        "trace_start",
        lua.create_function(move |_lua, (path, table): (String, Option<Table>)| {
            with(&shared, |driver| {
                let mut options = Options::new("trace_start", table);
                let all_frames: Option<bool> = options.get("all_frames")?;
                options.finish()?;
                if let Some(mut trace) = driver.trace.take() {
                    trace.writer.flush().map_err(engine_error)?;
                }
                let writer = trace::TraceWriter::create(Path::new(&path)).map_err(engine_error)?;
                driver.trace = Some(ScriptTrace {
                    writer,
                    all_frames: all_frames.unwrap_or(false),
                });
                Ok(())
            })
        })?,
    )?;

    let shared = Rc::clone(driver);
    globals.set(
        "trace_stop",
        lua.create_function(move |_lua, ()| {
            with(&shared, |driver| {
                if let Some(mut trace) = driver.trace.take() {
                    trace.writer.flush().map_err(engine_error)?;
                }
                Ok(())
            })
        })?,
    )?;

    let shared = Rc::clone(driver);
    globals.set(
        "bot_start",
        lua.create_function(move |_lua, table: Option<Table>| {
            with(&shared, |driver| {
                let mut options = Options::new("bot_start", table);
                let start = BotStart::read(&mut options)?;
                options.finish()?;
                driver.bot_start(&start)
            })
        })?,
    )?;

    let shared = Rc::clone(driver);
    globals.set(
        "bot_finished",
        lua.create_function(move |_lua, ()| {
            with(&shared, |driver| {
                let outcome = driver.bot_outcome()?;
                let reason = match outcome {
                    "running" => None,
                    "death" => Some("dead, no save".to_string()),
                    _ => driver.bot()?.finish_reason(),
                };
                Ok((outcome != "running", reason))
            })
        })?,
    )?;

    let shared = Rc::clone(driver);
    globals.set(
        "bot_stats",
        lua.create_function(move |lua, ()| {
            let report = with(&shared, |driver| {
                let outcome = driver.bot_outcome()?;
                driver.bot_report(outcome)
            })?;
            from_json(lua, &report)
        })?,
    )?;

    let shared = Rc::clone(driver);
    globals.set(
        "bot_memory",
        lua.create_function(move |_lua, path: String| {
            with(&shared, |driver| {
                let json = driver.bot()?.memory_json().map_err(engine_error)?;
                let text = serde_json::to_string(&json).map_err(mlua::Error::external)?;
                std::fs::write(&path, text).map_err(|err| {
                    mlua::Error::RuntimeError(format!("bot_memory: writing {path}: {err}"))
                })
            })
        })?,
    )?;

    let shared = Rc::clone(driver);
    globals.set(
        "bot_story",
        lua.create_function(move |_lua, (bank, bit): (usize, u8)| {
            with(&shared, |driver| Ok(driver.bot()?.story_seen(bank, bit)))
        })?,
    )?;

    let shared = Rc::clone(driver);
    globals.set(
        "bot_run",
        lua.create_function(move |lua, table: Option<Table>| {
            let report = with(&shared, |driver| {
                let mut options = Options::new("bot_run", table);
                let start = BotStart::read(&mut options)?;
                let max_ticks: Option<u64> = options.get("max_ticks")?;
                let stats: Option<String> = options.get("stats")?;
                options.finish()?;
                driver.bot_start(&start)?;
                let outcome = driver.bot_play(max_ticks.unwrap_or(bot::DEFAULT_MAX_TICKS))?;
                let report = driver.bot_report(outcome)?;
                if let Some(path) = stats {
                    let text =
                        serde_json::to_string_pretty(&report).map_err(mlua::Error::external)?;
                    std::fs::write(&path, text).map_err(|err| {
                        mlua::Error::RuntimeError(format!("bot_run: writing {path}: {err}"))
                    })?;
                }
                Ok(report)
            })?;
            from_json(lua, &report)
        })?,
    )?;

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

    #[test]
    fn button_names_cover_the_input_names_and_the_tank_aliases() {
        assert_eq!(button_bit("forward").unwrap(), KEY_UP);
        assert_eq!(button_bit("action").unwrap(), KEY_CONFIRM);
        assert!(button_bit("jump").is_err());
    }
}
