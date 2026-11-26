//! A save/load screen implemented by a pack Lua script.
//!
//! The screen is a normal pack entry, and the pack's scripts register it with
//! `api:register_save_screen` from their `on_init` hook (the checked-in
//! `mods/crloadsave` is the reference). The engine loads it in the same
//! sandbox the hooks use and drives three optional globals:
//!
//! ```lua
//! function on_open(api) end        -- once, before the first tick
//! function update(api, input) end  -- once per fixed tick
//! function draw(api) end           -- once per frame
//! ```
//!
//! `api` is the general screen surface a mod can use for any full-screen
//! menu: the save list, save/load/remove actions, the pad edges, the screen
//! sounds, and immediate-mode drawing in the layout space: 320x240 at 4:3,
//! the presented band (320x180 at 16:9) at a wider aspect, or the full-height
//! canvas (427x240 at 16:9) a screen drawn on black asks for with
//! `api:set("canvas", 1)`. The full
//! method table is in `lua-types/screen.lua`.
//!
//! There is no Rust fallback: a script that fails to load is an error the
//! opener handles (a registered mod save screen falls back to the shipped
//! script; any other screen takes its fail action, or never opens), and a
//! script that errors while running is logged once and the screen leaves
//! (returning to gameplay or the title) instead of trapping the player.

use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::{Context, Result};
use mlua::{Function, Lua, Table, UserData, UserDataMethods, Value};

use crate::budget;
use crate::charset::{self, Script};
use crate::font::Tint;
use crate::game::GameState;
use crate::lua::{Budget, new_sandboxed_state};
use crate::model::Texture8;
use crate::pack::Pack;
use crate::render::{Framebuffer, SpriteBlend};
use crate::save::{self, SaveEntry, SaveFile};
use crate::state::Image;
use crate::text::Text;
use crate::transition::FadeColor;
use crate::ui::save_load::{SaveOutcome, consume_snapshot_ribbon};
use crate::ui::{Screen, ScreenAction, ScreenResult, UiContext, UiCue, UiInput};

pub use bindings::BindingsRequest;
use sources::{Loader, Sources};

mod bindings;
mod draw;
mod ex_models;
mod game_api;
mod option_model;
mod sources;

/// The coordinate range the drawing API accepts; wider values are clamped
/// so the sprite and font cursors cannot overflow.
const DRAW_LIMIT: i32 = 4096;

/// Which way the screen runs.
#[derive(Debug, Clone)]
enum Mode {
    /// The title's load screen.
    Load,
    /// A typewriter (or the next-cycle ending save): the block `file` is
    /// written where the script chooses, consuming an ink ribbon when
    /// asked. Boxed: a save block dwarfs the load variant.
    Save {
        file: Box<SaveFile>,
        ink_ribbon: bool,
    },
    /// A named engine screen (`ui/<id>.lua`): the script reports its own
    /// outcome with `api:exit(action)`, and a failing script reports `fail`.
    Screen { fail: ScreenAction },
}

/// One `args` field a screen's `open(api, args)` receives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScreenArg {
    Int(i64),
    Bool(bool),
    Str(String),
}

/// One decoded pack image, kept in the shape its format has.
enum Decoded {
    /// A BMP or 16bpp TIM: straight RGBA pixels.
    Rgba(Image),
    /// A 4bpp/8bpp TIM: palette indices plus CLUT rows.
    Indexed(Texture8),
}

impl Decoded {
    fn size(&self) -> (u32, u32) {
        match self {
            Self::Rgba(image) => (image.width, image.height),
            Self::Indexed(texture) => (texture.width, texture.height),
        }
    }
}

/// The persistent state behind the `api` userdata.
struct Host {
    /// The script's pack entry, the scope of what it keeps (`api:keep`).
    entry: String,
    mode: Mode,
    save_dir: PathBuf,
    /// The byte space the script's text is encoded in.
    script: Script,
    /// Whether text takes the session's tight metrics.
    tight: bool,
    /// The save directory's scan, refreshed on open and after a write.
    list: Vec<SaveEntry>,
    /// Decoded pixels the image cache holds, against
    /// [`budget::MAX_SCREEN_IMAGE_BYTES`].
    image_bytes: usize,
    /// Whether the missing-font warning has been logged.
    missing_font_logged: bool,
    ticks: u64,
    fade: u8,
    outcome: Option<SaveOutcome>,
    load: Option<PathBuf>,
    exited: bool,
    failed: bool,
    /// Whether the script has `on_film`, which `api:exit("film")` needs.
    plays_films: bool,
    /// Whether the script has `on_volumes`: it shows the settings' volumes.
    shows_volumes: bool,
    /// Whether the script has `on_boot`: it runs the game's boot itself.
    boots: bool,
    /// Whether the script has `on_bindings`: it shows and sets the
    /// player's bindings.
    shows_bindings: bool,
    /// What `api:set_bindings` asked for since the owner last took it.
    bindings_request: Option<BindingsRequest>,
    images: HashMap<String, Decoded>,
    /// The layout space the script lays itself out in (`api:width()`).
    layout: (u32, u32),
    /// The action a named `api:exit(action)` chose.
    action: Option<ScreenAction>,
    /// The overlay `api:overlay` set; `None` keeps the black-fade default.
    overlay: Option<Option<(FadeColor, u8)>>,
    /// Values the engine reads back, set with `api:set(key, value)`.
    values: HashMap<String, i64>,
    /// Art loaded by handle (`api:picture`, `api:page`).
    assets: draw::Assets,
    /// The screen's own billboard pool (`api:effect_scene`).
    effects: Option<Box<crate::engine::EffectScene>>,
    /// RE 2's KEYCONFIG model (`api:option_model`).
    option_model: option_model::OptionModel,
    /// Extreme Battle's four players on its character choice
    /// (`api:ex_models`).
    ex_models: ex_models::ExModels,
}

impl Host {
    /// Report a text call on a pack with no font, once per screen.
    fn note_missing_font(&mut self) {
        if !self.missing_font_logged {
            self.missing_font_logged = true;
            crate::warning!("[lua] the pack carries no font; screen text is not drawn");
        }
    }

    /// Rescan the save directory.
    fn rescan(&mut self) {
        self.list = save::scan_dir(&self.save_dir);
    }

    /// The decoded pack image, cached. The cache is bounded by count and
    /// by the pixels it holds, so a script cannot pin memory with many
    /// large images.
    fn image<'a>(&'a mut self, pack: &Pack, entry: &str) -> Result<&'a Decoded> {
        if !self.images.contains_key(entry) {
            budget::check_len(
                self.images.len() + 1,
                budget::MAX_SCREEN_IMAGES,
                "screen image cache size",
            )?;
            let decoded = decode_image(pack, entry)?;
            let (width, height) = decoded.size();
            let bytes = (width as usize)
                .saturating_mul(height as usize)
                .saturating_mul(4);
            budget::check_len(
                self.image_bytes.saturating_add(bytes),
                budget::MAX_SCREEN_IMAGE_BYTES,
                "screen image cache bytes",
            )?;
            self.image_bytes = self.image_bytes.saturating_add(bytes);
            self.images.insert(entry.to_string(), decoded);
        }
        Ok(self.images.get(entry).expect("just inserted"))
    }

    /// The decoded pack image `entry` and, when `framebuffer` captures
    /// replacements, the HD replacement `pack` carries for it drawn with
    /// `opts`.
    fn entry_art(
        &mut self,
        pack: &Pack,
        entry: &str,
        framebuffer: &Framebuffer,
        opts: &Options,
    ) -> Result<(&Decoded, draw::HdArt)> {
        self.image(pack, entry)?;
        let image = self.images.get(entry).expect("just loaded");
        let hd = if framebuffer.records() {
            self.assets.entry_replacement(pack, entry, image, opts.clut)
        } else {
            None
        };
        Ok((image, hd))
    }
}

/// A save/load screen driven by a Lua script.
pub struct LuaScreen {
    entry: String,
    host: Host,
    vm: ScreenVm,
}

impl LuaScreen {
    /// Load the `entry` script as the title's load screen.
    pub fn open_load(entry: &str, cx: &mut UiContext<'_>) -> Result<Self> {
        Self::open(entry, Mode::Load, &[], cx, None)
    }

    /// Open the engine screen `id`: the pack's `ui/<id>.lua`, or the
    /// checked-in copy when the pack predates it. `args` reach the script's
    /// `open(api, args)`; a script that fails while running reports `fail`.
    pub fn open_screen(
        id: &str,
        args: &[(&str, ScreenArg)],
        fail: ScreenAction,
        cx: &mut UiContext<'_>,
    ) -> Result<Self> {
        Self::open(&screen_entry(id), Mode::Screen { fail }, args, cx, None)
    }

    /// [`LuaScreen::open_screen`] lending the script the session's game
    /// state, which its `open` may change (the pause menu). `cx.game` is
    /// unused while `game` is lent.
    pub fn open_screen_with_game(
        id: &str,
        args: &[(&str, ScreenArg)],
        fail: ScreenAction,
        cx: &mut UiContext<'_>,
        game: &mut GameState,
    ) -> Result<Self> {
        Self::open(
            &screen_entry(id),
            Mode::Screen { fail },
            args,
            cx,
            Some(game),
        )
    }

    /// Load the `entry` script as a save screen writing `file`;
    /// `ink_ribbon` makes the write consume one.
    pub fn open_save(
        entry: &str,
        cx: &mut UiContext<'_>,
        file: SaveFile,
        ink_ribbon: bool,
    ) -> Result<Self> {
        Self::open_save_with(entry, cx, file, ink_ribbon, &[])
    }

    /// [`LuaScreen::open_save`] with `args` for the script's `open`.
    pub fn open_save_with(
        entry: &str,
        cx: &mut UiContext<'_>,
        file: SaveFile,
        ink_ribbon: bool,
        args: &[(&str, ScreenArg)],
    ) -> Result<Self> {
        Self::open(
            entry,
            Mode::Save {
                file: Box::new(file),
                ink_ribbon,
            },
            args,
            cx,
            None,
        )
    }

    fn open(
        entry: &str,
        mode: Mode,
        args: &[(&str, ScreenArg)],
        cx: &mut UiContext<'_>,
        game: Option<&mut GameState>,
    ) -> Result<Self> {
        let sources = Sources::collect(cx.pack, entry)?;
        let text = cx.text;
        let mut host = Host {
            entry: entry.to_string(),
            mode,
            save_dir: cx.save_dir.to_path_buf(),
            script: text.map(Text::script).unwrap_or_default(),
            tight: text.map(Text::uses_tight_metrics).unwrap_or(true),
            list: Vec::new(),
            image_bytes: 0,
            missing_font_logged: false,
            ticks: 0,
            fade: 0,
            outcome: None,
            load: None,
            exited: false,
            failed: false,
            plays_films: false,
            shows_volumes: false,
            boots: false,
            shows_bindings: false,
            bindings_request: None,
            images: HashMap::new(),
            layout: (crate::render::WIDTH, crate::video::aspect().height()),
            action: None,
            overlay: None,
            values: HashMap::new(),
            assets: draw::Assets::default(),
            effects: None,
            option_model: Default::default(),
            ex_models: Default::default(),
        };
        let vm = ScreenVm::load(entry, Loader::new(sources))?;
        host.plays_films = vm.has_function("on_film");
        host.shows_volumes = vm.has_function("on_volumes");
        host.boots = vm.has_function("on_boot");
        host.shows_bindings = vm.has_function("on_bindings");
        host.rescan();
        vm.call_open(&mut host, cx, game, args)
            .with_context(|| format!("screen script {entry} open failed"))?;
        Ok(Self {
            entry: entry.to_string(),
            host,
            vm,
        })
    }

    /// The completed save, once the script wrote one.
    pub fn take_outcome(&mut self) -> Option<SaveOutcome> {
        self.host.outcome.take()
    }

    /// A value the script published with `api:set(key, value)`; `0` when
    /// it never set one.
    pub fn value(&self, key: &str) -> i64 {
        self.host.values.get(key).copied().unwrap_or(0)
    }

    /// A value the script published with `api:set(key, value)`, or `None`
    /// when it never set one.
    pub fn published(&self, key: &str) -> Option<i64> {
        self.host.values.get(key).copied()
    }

    /// RE 2's option screen opened from play: KEYCONFIG's model is the
    /// player, `work` as play left it.
    pub fn set_option_player(&mut self, work: &crate::player2::Work) {
        self.host.option_model.set_player(work);
    }

    /// KEYCONFIG's model, once the page made it.
    pub fn option_model(&self) -> Option<&crate::player2::keyconfig::PageModel> {
        self.host.option_model.model()
    }

    /// Drop a published value, as if the script never set it.
    pub fn forget(&mut self, key: &str) {
        self.host.values.remove(key);
    }

    /// Whether the script has `on_volumes`: its own option screen shows and
    /// sets the settings' music and effects volumes, which its owner tells
    /// it and takes back from what it publishes.
    pub fn shows_volumes(&self) -> bool {
        !self.host.failed && self.host.shows_volumes
    }

    /// Whether the script has `on_bindings`: it shows the player's
    /// bindings and may ask for them changed ([`Self::take_bindings_request`]).
    pub fn shows_bindings(&self) -> bool {
        !self.host.failed && self.host.shows_bindings
    }

    /// Tell the script the settings' bindings and the shipped ones
    /// (`on_bindings(api, bindings, defaults)`).
    pub fn tell_bindings(
        &mut self,
        bindings: &crate::controls::Bindings,
        defaults: &crate::controls::Bindings,
        cx: &UiContext<'_>,
    ) {
        if self.shows_bindings() {
            self.signal_with(
                "on_bindings",
                bindings::BindingsArg { bindings, defaults },
                cx,
            );
        }
    }

    /// What the script asked of the bindings (`api:set_bindings`) since
    /// the last call.
    pub fn take_bindings_request(&mut self) -> Option<BindingsRequest> {
        self.host.bindings_request.take()
    }

    /// What the screen shows a player, as its script's `view()` publishes
    /// it, for observers such as the player bot. The view is the screen's
    /// contract with them: they read nothing else of its state. `None` when
    /// the script defines no `view`, or it failed (logged; the screen runs
    /// on).
    pub fn view(&self) -> Option<crate::interactive::ScriptValue> {
        match self.vm.call_view() {
            Ok(view) => view,
            Err(err) => {
                crate::warning!("[lua] screen {} {err}", self.entry);
                None
            }
        }
    }

    /// A number or boolean field of the script's own `M.state` table (the
    /// pause menu exposes its cursor state there), read without running any
    /// script code; `None` when the script keeps no such field. A boolean
    /// reads as `0`/`1`.
    pub fn state_int(&self, key: &str) -> Option<i64> {
        let Ok(Value::Table(state)) = self.vm.module.raw_get::<Value>("state") else {
            return None;
        };
        match state.raw_get::<Value>(key).ok()? {
            Value::Integer(value) => Some(value),
            Value::Number(value) => Some(value as i64),
            Value::Boolean(value) => Some(i64::from(value)),
            _ => None,
        }
    }

    /// A string field of the script's `M.state` table, read like
    /// [`Self::state_int`]; `None` when the field is absent or not a string.
    pub fn state_str(&self, key: &str) -> Option<String> {
        let Ok(Value::Table(state)) = self.vm.module.raw_get::<Value>("state") else {
            return None;
        };
        match state.raw_get::<Value>(key).ok()? {
            Value::String(text) => Some(text.to_string_lossy()),
            _ => None,
        }
    }

    /// The truthiness of each entry of an array field of `M.state` (a slot
    /// list holding an info table or `false` per row), read like
    /// [`Self::state_int`]; `None` when the field is not a table.
    pub fn state_truthy_list(&self, key: &str) -> Option<Vec<bool>> {
        let Ok(Value::Table(state)) = self.vm.module.raw_get::<Value>("state") else {
            return None;
        };
        let Value::Table(list) = state.raw_get::<Value>(key).ok()? else {
            return None;
        };
        let len = list.raw_len();
        Some(
            (1..=len)
                .map(|index| {
                    !matches!(
                        list.raw_get::<Value>(index),
                        Ok(Value::Nil | Value::Boolean(false)) | Err(_)
                    )
                })
                .collect(),
        )
    }

    /// Overwrite a published value, as the script's `api:set` would.
    #[cfg(test)]
    pub fn set_value(&mut self, key: &str, value: i64) {
        self.host.values.insert(key.to_owned(), value);
    }

    /// Run the script's `M.<name>(api)` hook, if it defines one: the engine's
    /// way to tell a running screen something happened (the pick-up viewer's
    /// `begin_exit`). A failing hook is logged and fails the screen.
    pub fn signal(&mut self, name: &str, cx: &UiContext<'_>) {
        self.signal_with(name, (), cx);
    }

    /// [`LuaScreen::signal`] with one argument after `api`: `M.<name>(api,
    /// value)` (the item box's `cursor_shown`).
    pub fn signal_with(&mut self, name: &str, value: impl mlua::IntoLuaMulti, cx: &UiContext<'_>) {
        self.hook(name, value, cx, None);
    }

    /// [`LuaScreen::signal_with`] lending the hook the session's game
    /// state, which it may change (the pause menu's per-frame `tick`).
    pub fn signal_with_game(
        &mut self,
        name: &str,
        value: impl mlua::IntoLuaMulti,
        cx: &UiContext<'_>,
        game: &mut GameState,
    ) {
        self.hook(name, value, cx, Some(game));
    }

    fn hook(
        &mut self,
        name: &str,
        value: impl mlua::IntoLuaMulti,
        cx: &UiContext<'_>,
        game: Option<&mut GameState>,
    ) {
        let Self { host, vm, entry } = self;
        if let Err(err) = vm.call_hook(name, host, cx, game, value) {
            crate::warning!("[lua] screen {entry} {name} failed: {err}");
            host.failed = true;
        }
    }

    /// [`Screen::update`] lending the script the session's game state,
    /// which it may change (the pause menu's use, equip and combine).
    pub fn update_with_game(
        &mut self,
        cx: &UiContext<'_>,
        input: UiInput,
        game: &mut GameState,
    ) -> ScreenResult {
        self.step(cx, input, Some(game))
    }

    fn step(
        &mut self,
        cx: &UiContext<'_>,
        input: UiInput,
        game: Option<&mut GameState>,
    ) -> ScreenResult {
        let Self { host, vm, entry } = self;
        if let Err(err) = vm.call_update(host, cx, game, input) {
            crate::warning!("[lua] screen {entry} update failed: {err}");
            host.failed = true;
        }
        self.host.ticks = self.host.ticks.saturating_add(1);
        if let Some(path) = self.host.load.take() {
            return ScreenResult::Done(ScreenAction::LoadFile(path));
        }
        if self.host.exited || self.host.failed {
            return ScreenResult::Done(self.exit_action());
        }
        ScreenResult::Continue
    }

    /// [`Screen::draw`] with the engine's actors available to
    /// `api:draw_actor`.
    pub fn draw_with_actors(
        &mut self,
        cx: &UiContext<'_>,
        framebuffer: &mut Framebuffer,
        actors: Option<&mut ActorDraw<'_>>,
    ) {
        let Self { host, vm, entry } = self;
        // The script lays itself out in the presented band: the whole frame
        // at 4:3 or on a canvas, the shorter rows a wider aspect shows
        // otherwise.
        framebuffer.with_view(|framebuffer| {
            host.layout = (framebuffer.width, framebuffer.height);
            // `api:clip` lasts for this draw only.
            let clip = framebuffer.clip_rect();
            if let Err(err) = vm.call_draw(host, cx, framebuffer, actors) {
                crate::warning!("[lua] screen {entry} draw failed: {err}");
                host.failed = true;
            }
            framebuffer.set_clip_rect(clip);
        });
    }

    /// The action an exit reports: the one the script named, else back to
    /// gameplay from a save screen, back to the title from the load screen
    /// and the opener's fallback from a named screen.
    fn exit_action(&mut self) -> ScreenAction {
        if !self.host.failed
            && let Some(action) = self.host.action.take()
        {
            return action;
        }
        match &self.host.mode {
            Mode::Save { .. } => ScreenAction::Resume,
            Mode::Load => ScreenAction::Title,
            Mode::Screen { fail } => fail.clone(),
        }
    }

    /// A Lua global, for the unit tests.
    #[cfg(test)]
    pub(super) fn test_global<T: mlua::FromLua>(&self, name: &str) -> T {
        self.vm
            .global(name)
            .unwrap_or_else(|err| panic!("global {name}: {err}"))
    }
}

impl LuaScreen {
    /// Whether the script raised an error: a failed screen ignores every
    /// later call, so its owner must close it.
    pub fn failed(&self) -> bool {
        self.host.failed
    }

    /// Whether the script has `on_resume`: its owner then keeps it while the
    /// screen it left for runs, and resumes it when that one comes back.
    pub fn resumes(&self) -> bool {
        !self.host.failed && self.vm.has_function("on_resume")
    }

    /// Whether the script has `on_loaded` as well: its owner then gives it
    /// a save the screen it left for chose, to start the game from itself.
    pub fn continues(&self) -> bool {
        self.resumes() && self.vm.has_function("on_loaded")
    }

    /// Put a screen that left back to work, calling its `on_resume(api)`:
    /// the screen it left for has come back to it.
    pub fn resume(&mut self, cx: &UiContext<'_>) {
        self.host.exited = false;
        self.host.action = None;
        self.signal("on_resume", cx);
    }

    /// Whether the script has `on_film`: it plays its own films
    /// ([`ScreenAction::Film`]) and its owner adds none of its own to a new
    /// game it starts.
    pub fn plays_films(&self) -> bool {
        !self.host.failed && self.host.plays_films
    }

    /// Put a screen that left for a film back to work, calling its
    /// `on_film(api, skipped, played)`: the film is over, by a press when
    /// `skipped`, or never started when not `played`.
    pub fn resume_film(&mut self, cx: &UiContext<'_>, skipped: bool, played: bool) {
        self.host.exited = false;
        self.host.action = None;
        self.signal_with("on_film", (skipped, played), cx);
    }

    /// Whether the script has `on_boot`: as the boot's first title it shows
    /// what the game shows before its title and plays its own opening film,
    /// so its owner adds none.
    pub fn boots(&self) -> bool {
        !self.host.failed && self.host.boots
    }

    /// [`LuaScreen::resume`] after the screen it left for chose a save,
    /// calling its `on_loaded(api, place, status)` instead, with the save's
    /// place and `fg_status` when the save has them (`start`).
    pub fn resume_loaded(&mut self, cx: &UiContext<'_>, start: Option<(u8, u32)>) {
        self.host.exited = false;
        self.host.action = None;
        self.signal_with(
            "on_loaded",
            start.map_or((None, None), |(place, status)| (Some(place), Some(status))),
            cx,
        );
    }
}

#[cfg(test)]
impl LuaScreen {
    /// Evaluate `body` with the screen's module table bound to `M`.
    pub(crate) fn test_eval<T: mlua::FromLuaMulti>(&self, body: &str) -> T {
        self.vm
            .eval(body)
            .unwrap_or_else(|err| panic!("eval {body}: {err}"))
    }

    /// Evaluate `body` with the module table bound to `M` and the API bound
    /// to `api` over `game` when one is lent, for the unit tests that reach the API.
    pub(crate) fn test_eval_with_game<T: mlua::FromLuaMulti>(
        &mut self,
        cx: &UiContext<'_>,
        game: Option<&mut GameState>,
        body: &str,
    ) -> T {
        self.vm
            .eval_with_game(&mut self.host, cx, game, body)
            .unwrap_or_else(|err| panic!("eval {body}: {err}"))
    }

    /// How many pieces of art the screen holds against its budget.
    pub(crate) fn test_live_art(&self) -> usize {
        self.host.assets.live()
    }

    /// Mark the script failed, as a call that raised an error does.
    pub(crate) fn test_fail(&mut self) {
        self.host.failed = true;
    }

    /// Publish `key` as if the script had called `api:set`.
    pub(crate) fn test_set_value(&mut self, key: &str, value: i64) {
        self.host.values.insert(key.to_owned(), value);
    }
}

impl Screen for LuaScreen {
    /// Nothing to load: [`LuaScreen::open_load`]/[`LuaScreen::open_save`]
    /// read the script and ran `on_open` already.
    fn open(&mut self, _cx: &mut UiContext<'_>) -> Result<()> {
        Ok(())
    }

    fn update(&mut self, cx: &UiContext<'_>, input: UiInput) -> ScreenResult {
        self.step(cx, input, None)
    }

    fn draw(&mut self, cx: &UiContext<'_>, framebuffer: &mut Framebuffer) {
        self.draw_with_actors(cx, framebuffer, None);
    }

    fn fade(&self) -> u8 {
        self.host.fade
    }

    fn overlay(&self) -> Option<(FadeColor, u8)> {
        match self.host.overlay {
            Some(overlay) => overlay,
            None => match self.host.fade {
                0 => None,
                alpha => Some((FadeColor::Black, alpha)),
            },
        }
    }
}

impl crate::ui::save_load::SaveScreen for LuaScreen {
    fn take_outcome(&mut self) -> Option<SaveOutcome> {
        LuaScreen::take_outcome(self)
    }

    fn value(&self, key: &str) -> Option<i64> {
        self.host.values.get(key).copied()
    }

    fn bot_view(&self) -> Option<crate::ui::save_load::SaveScreenView> {
        let stage = self.state_str("stage")?;
        let slots = self.state_truthy_list("slots")?;
        let autosaves = self
            .state_truthy_list("autosaves")
            .map_or(0, |rows| rows.len());
        Some(crate::ui::save_load::SaveScreenView {
            save: self.state_int("save").unwrap_or(0) != 0,
            stage,
            cursor: self.state_int("cursor").unwrap_or(0),
            exit_row: (slots.len() + autosaves) as i64,
            slots,
            rows: None,
            answer: None,
        })
    }
}

/// The sandboxed VM one screen script runs in.
struct ScreenVm {
    lua: Lua,
    budget: Rc<Budget>,
    /// The entry chunk's environment, where a legacy script's globals live.
    #[cfg_attr(not(test), allow(dead_code))]
    env: Table,
    /// The table the entry chunk returned (the env for a legacy script).
    module: Table,
    on_open: Option<Function>,
    update: Option<Function>,
    draw: Option<Function>,
    /// The published view, `view()` (see [`LuaScreen::view`]).
    view: Option<Function>,
}

impl ScreenVm {
    /// Compile and run the entry chunk, then look up its entry points: the
    /// `open`/`update`/`draw` fields of the table it returns, or (the legacy
    /// form) the `on_open`/`update`/`draw` globals it defines.
    fn load(name: &str, loader: Loader) -> Result<Self> {
        let (lua, budget) = new_sandboxed_state()?;
        budget.begin();
        let source = loader
            .source(name)
            .with_context(|| format!("cannot read screen script {name}"))?;
        let env = loader
            .env(&lua, name)
            .map_err(|err| anyhow::anyhow!("{name}: {err}"))?;
        let returned: Value = lua
            .load(source)
            .set_name(name.to_string())
            .set_environment(env.clone())
            .eval()
            .map_err(|err| anyhow::anyhow!("{name}: {err}"))?;
        let table = match returned {
            Value::Table(table) => table,
            _ => env.clone(),
        };
        let lookup = |name: &str| match table.get::<Value>(name) {
            Ok(Value::Function(function)) => Some(function),
            _ => None,
        };
        let update = lookup("update");
        if update.is_none() {
            anyhow::bail!("{name} defines no update(api, input), so it could never leave");
        }
        Ok(Self {
            on_open: lookup("open").or_else(|| lookup("on_open")),
            update,
            draw: lookup("draw"),
            view: lookup("view"),
            lua,
            budget,
            env,
            module: table,
        })
    }

    /// Run `open(api, args)`.
    fn call_open(
        &self,
        host: &mut Host,
        cx: &UiContext<'_>,
        game: Option<&mut GameState>,
        args: &[(&str, ScreenArg)],
    ) -> Result<()> {
        let Some(function) = &self.on_open else {
            return Ok(());
        };
        let table = args_table(&self.lua, args).map_err(|err| anyhow::anyhow!("{err}"))?;
        self.budget.begin();
        self.lua
            .scope(|scope| {
                let api = scope.create_userdata(Api {
                    host: &mut *host,
                    cx,
                    game_mut: game,
                    framebuffer: None,
                    actors: None,
                })?;
                function.call::<()>((api, table))
            })
            .map_err(|err| anyhow::anyhow!("{err}"))?;
        Ok(())
    }

    /// Run `update(api, input)`.
    fn call_update(
        &self,
        host: &mut Host,
        cx: &UiContext<'_>,
        game: Option<&mut GameState>,
        input: UiInput,
    ) -> Result<()> {
        let Some(function) = &self.update else {
            return Ok(());
        };
        let table = input_table(&self.lua, input).map_err(|err| anyhow::anyhow!("{err}"))?;
        self.budget.begin();
        self.lua
            .scope(|scope| {
                let api = scope.create_userdata(Api {
                    host: &mut *host,
                    cx,
                    game_mut: game,
                    framebuffer: None,
                    actors: None,
                })?;
                function.call::<()>((api, table))
            })
            .map_err(|err| anyhow::anyhow!("{err}"))?;
        Ok(())
    }

    /// Whether the module has a function `name`.
    fn has_function(&self, name: &str) -> bool {
        matches!(self.module.get::<Value>(name), Ok(Value::Function(_)))
    }

    /// Run the module's `name(api)`, when it is a function.
    fn call_hook(
        &self,
        name: &str,
        host: &mut Host,
        cx: &UiContext<'_>,
        game: Option<&mut GameState>,
        value: impl mlua::IntoLuaMulti,
    ) -> Result<()> {
        let Ok(Value::Function(function)) = self.module.get::<Value>(name) else {
            return Ok(());
        };
        self.budget.begin();
        self.lua
            .scope(|scope| {
                let api = scope.create_userdata(Api {
                    host: &mut *host,
                    cx,
                    game_mut: game,
                    framebuffer: None,
                    actors: None,
                })?;
                let mut args = mlua::IntoLuaMulti::into_lua_multi(value, &self.lua)?;
                args.push_front(Value::UserData(api));
                function.call::<()>(args)
            })
            .map_err(|err| anyhow::anyhow!("{err}"))?;
        Ok(())
    }

    /// Run `view()`: the table it returns as a state tree, `None` when the
    /// script defines no `view`.
    fn call_view(&self) -> Result<Option<crate::interactive::ScriptValue>> {
        let Some(function) = &self.view else {
            return Ok(None);
        };
        self.budget.begin();
        let table: Table = function
            .call(())
            .map_err(|err| anyhow::anyhow!("view(): {err}"))?;
        let view =
            crate::interactive::from_lua(&table).map_err(|err| anyhow::anyhow!("view(): {err}"))?;
        Ok(Some(view))
    }

    /// Run `draw(api)`.
    fn call_draw(
        &self,
        host: &mut Host,
        cx: &UiContext<'_>,
        framebuffer: &mut Framebuffer,
        actors: Option<&mut ActorDraw<'_>>,
    ) -> Result<()> {
        let Some(function) = &self.draw else {
            return Ok(());
        };
        self.budget.begin();
        self.lua
            .scope(|scope| {
                let api = scope.create_userdata(Api {
                    host: &mut *host,
                    cx,
                    game_mut: None,
                    framebuffer: Some(&mut *framebuffer),
                    // Shorten the closure's bound to the scope's lifetime.
                    actors: actors.map(|draw| draw as &mut ActorDraw<'_>),
                })?;
                function.call::<()>(api)
            })
            .map_err(|err| anyhow::anyhow!("{err}"))?;
        Ok(())
    }

    /// Evaluate `body` with the module table bound to `M` in the scripts'
    /// environment, for the unit tests.
    #[cfg(test)]
    fn eval<T: mlua::FromLuaMulti>(&self, body: &str) -> mlua::Result<T> {
        let function: Function = self
            .lua
            .load(format!("return function(M) {body} end"))
            .set_environment(self.env.clone())
            .eval()?;
        function.call(self.module.clone())
    }

    /// Evaluate `body` with the module table bound to `M` and the API bound
    /// to `api`, built over the lent game, if any, the way the callbacks build it.
    #[cfg(test)]
    fn eval_with_game<T: mlua::FromLuaMulti>(
        &self,
        host: &mut Host,
        cx: &UiContext<'_>,
        game: Option<&mut GameState>,
        body: &str,
    ) -> mlua::Result<T> {
        let function: Function = self
            .lua
            .load(format!("return function(M, api) {body} end"))
            .set_environment(self.env.clone())
            .eval()?;
        self.budget.begin();
        self.lua.scope(|scope| {
            let api = scope.create_userdata(Api {
                host: &mut *host,
                cx,
                game_mut: game,
                framebuffer: None,
                actors: None,
            })?;
            function.call((self.module.clone(), api))
        })
    }

    /// A Lua global, for the unit tests.
    #[cfg(test)]
    fn global<T: mlua::FromLua>(&self, name: &str) -> mlua::Result<T> {
        self.env.get(name)
    }
}

/// The pack entry of engine screen `id`.
pub fn screen_entry(id: &str) -> String {
    format!("ui/{id}.lua")
}

/// The `args` table one `open` call receives.
fn args_table(lua: &Lua, args: &[(&str, ScreenArg)]) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (key, value) in args {
        match value {
            ScreenArg::Int(value) => table.set(*key, *value)?,
            ScreenArg::Bool(value) => table.set(*key, *value)?,
            ScreenArg::Str(value) => table.set(*key, value.as_str())?,
        }
    }
    Ok(table)
}

/// The `input` table one `update` call receives.
fn input_table(lua: &Lua, input: UiInput) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("up", input.up)?;
    table.set("down", input.down)?;
    table.set("left", input.left)?;
    table.set("right", input.right)?;
    table.set("confirm", input.confirm)?;
    table.set("cancel", input.cancel)?;
    table.set("page_left", input.page_left)?;
    table.set("page_right", input.page_right)?;
    table.set("start", input.start)?;
    table.set("map", input.map)?;
    table.set("option", input.option)?;
    table.set("any", input.any)?;
    table.set("return_title", input.return_title)?;
    table.set("pad", pad_word(input))?;
    table.set("held", held_word(input))?;
    let (keys, buttons) = bindings::pressed_names(&input.newly_down);
    table.set("keys", keys)?;
    table.set("buttons", buttons)?;
    Ok(table)
}

/// The `api` userdata handed to one script call.
///
/// It borrows the persistent host, the screen context and — during
/// `draw` — the framebuffer. Non-`'static` userdata is exactly what
/// `Lua::scope` supports, so the surface needs no raw pointers.
struct Api<'a> {
    host: &'a mut Host,
    cx: &'a UiContext<'a>,
    /// The session's game state when the engine lends it mutably (the
    /// pause menu); `cx.game` is the read-only view otherwise.
    game_mut: Option<&'a mut GameState>,
    framebuffer: Option<&'a mut Framebuffer>,
    actors: Option<&'a mut ActorDraw<'a>>,
}

impl Api<'_> {
    /// The paused session's game state, mutable loan or shared view.
    fn game(&self) -> Option<&GameState> {
        self.game_mut.as_deref().or(self.cx.game)
    }

    /// The mutable game state, or an error outside a lending call.
    fn game_mut(&mut self, method: &str) -> mlua::Result<&mut GameState> {
        self.game_mut.as_deref_mut().ok_or_else(|| {
            mlua::Error::RuntimeError(format!(
                "api:{method} needs the game state, which this screen cannot change"
            ))
        })
    }
}

/// Draws an engine-owned actor (the DIED screen's corpse) named by
/// `api:draw_actor` through the screen's camera; returns whether it drew.
pub type ActorDraw<'a> = dyn FnMut(&str, &crate::render::Camera, &mut Framebuffer) -> bool + 'a;

/// The draw options table shared by `text`, `sprite` and friends.
struct Options {
    tint: Tint,
    brightness: u8,
    tight: bool,
    clut: usize,
    blend: SpriteBlend,
}

/// The tint a text draws in over `font`: that of the options table's `row`,
/// where the font has a colour of its own for the row, else the options'
/// `colour`. Only a text reads `row`, so a table shared with a sprite may
/// hold anything under the name.
fn text_tint(
    opts: &Options,
    table: Option<&Table>,
    font: &crate::font::Font,
) -> mlua::Result<Tint> {
    let row = match table {
        Some(table) => table.get::<Option<i64>>("row")?,
        None => None,
    };
    Ok(match row.map(|row| row.clamp(0, 255) as i32) {
        Some(row) if font.tints_row(row) => Tint::from_clut_row(row),
        _ => opts.tint,
    })
}

/// Parse an options table: `{colour = "green"|3, brightness = 2,
/// tight = true, clut = 0, blend = "opaque"}`. A text's `row` is
/// [`text_tint`]'s to read.
fn options(host: &Host, table: Option<&Table>) -> mlua::Result<Options> {
    let mut parsed = Options {
        tint: Tint::White,
        brightness: 2,
        tight: host.tight,
        clut: 0,
        blend: SpriteBlend::Opaque,
    };
    let Some(table) = table else {
        return Ok(parsed);
    };
    if let Some(value) = table.get::<Option<Value>>("colour")? {
        parsed.tint = colour_tint(&value)?;
    }
    if let Some(brightness) = table.get::<Option<i64>>("brightness")? {
        parsed.brightness = brightness.clamp(0, 30) as u8;
    }
    if let Some(tight) = table.get::<Option<bool>>("tight")? {
        parsed.tight = tight;
    }
    if let Some(clut) = table.get::<Option<i64>>("clut")? {
        parsed.clut = clut.clamp(0, 255) as usize;
    }
    if let Some(blend) = table.get::<Option<String>>("blend")? {
        parsed.blend = match blend.to_ascii_lowercase().as_str() {
            "opaque" => SpriteBlend::Opaque,
            "half" => SpriteBlend::Half,
            "half_variant" | "variant" => SpriteBlend::HalfVariant,
            other => {
                return Err(mlua::Error::RuntimeError(format!(
                    "unknown blend `{other}` (opaque, half, half_variant)"
                )));
            }
        };
    }
    Ok(parsed)
}

/// A text colour: a tint name, a CLUT row number or `"#RRGGBB"`. A row is
/// the text's colour row, which a font may have a colour of its own for; a
/// name or `"#RRGGBB"` is that colour on any font.
pub(super) fn colour_tint(value: &Value) -> mlua::Result<Tint> {
    match value {
        Value::Integer(row) => Ok(Tint::from_clut_row(*row as i32)),
        Value::String(text) => {
            let text = text.to_string_lossy();
            if let Some(hex) = text.strip_prefix('#') {
                return parse_hex_colour(hex)
                    .map(|[red, green, blue]| Tint::Rgb(red, green, blue))
                    .ok_or_else(|| mlua::Error::RuntimeError(format!("invalid colour `{text}`")));
            }
            match tint_named(&text) {
                Some(Tint::White) => Ok(Tint::White),
                Some(tint) => {
                    let [red, green, blue] = tint.rgb();
                    Ok(Tint::Rgb(red, green, blue))
                }
                None => Err(mlua::Error::RuntimeError(format!(
                    "unknown colour `{text}` (white, green, red, grey, yellow, a CLUT row, or \"#RRGGBB\")"
                ))),
            }
        }
        other => Err(mlua::Error::RuntimeError(format!(
            "expected a colour name or CLUT row, got {}",
            other.type_name()
        ))),
    }
}

/// A tint by name.
fn tint_named(name: &str) -> Option<Tint> {
    Some(match name.to_ascii_lowercase().as_str() {
        "white" => Tint::White,
        "green" => Tint::Green,
        "red" => Tint::Red,
        "grey" | "gray" => Tint::Grey,
        "yellow" => Tint::Yellow,
        _ => return None,
    })
}

/// An RGB colour: `0xRRGGBB`, `"#RRGGBB"`, or a tint name.
pub(super) fn colour_rgb(value: &Value) -> mlua::Result<[u8; 3]> {
    match value {
        Value::Integer(rgb) => {
            let rgb = *rgb as u32;
            Ok([(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8])
        }
        Value::String(text) => {
            let text = text.to_string_lossy();
            if let Some(hex) = text.strip_prefix('#') {
                return parse_hex_colour(hex)
                    .ok_or_else(|| mlua::Error::RuntimeError(format!("invalid colour `{text}`")));
            }
            match tint_named(&text) {
                Some(tint) => Ok(tint.rgb()),
                None => Err(mlua::Error::RuntimeError(format!(
                    "invalid colour `{text}` (0xRRGGBB, \"#RRGGBB\" or a tint name)"
                ))),
            }
        }
        other => Err(mlua::Error::RuntimeError(format!(
            "expected 0xRRGGBB or \"#RRGGBB\", got {}",
            other.type_name()
        ))),
    }
}

/// Parse `RRGGBB` or `RRGGBBAA` (alpha ignored).
///
/// The input is checked as ASCII hex digits byte for byte: indexing a
/// `str` by byte would panic on a multi-byte character.
pub(super) fn parse_hex_colour(hex: &str) -> Option<[u8; 3]> {
    let bytes = hex.as_bytes();
    if bytes.len() != 6 && bytes.len() != 8 {
        return None;
    }
    let digit = |byte: u8| (byte as char).to_digit(16).map(|value| value as u8);
    let byte = |index: usize| Some(digit(bytes[index])? << 4 | digit(bytes[index + 1])?);
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// Decode a pack image, resolving the entry the way every other screen
/// does: by its stem, so a mod's `webp`/`png`/`bmp`/`tim` replacement
/// wins like it does for the shipped art, and a `tim` keeps its palette
/// when it is indexed.
fn decode_image(pack: &Pack, entry: &str) -> Result<Decoded> {
    let asset = pack
        .read_asset(crate::assets::stem(entry), crate::pack::AssetClass::Image)?
        .with_context(|| format!("cannot read image {entry}"))?;
    match crate::assets::decode_page(&asset, crate::assets::Geometry::Free)? {
        crate::assets::Artwork::Indexed(texture) => Ok(Decoded::Indexed(texture)),
        crate::assets::Artwork::Rgba(image) => Ok(Decoded::Rgba(image)),
    }
}

/// A drawing coordinate or size a script supplied, clamped to a range no
/// screen needs and no cursor arithmetic can overflow.
fn coord(value: i32) -> i32 {
    value.clamp(-DRAW_LIMIT, DRAW_LIMIT)
}

/// Draw one decoded image at `dst`, scaling `src` into it, or its HD
/// replacement `hd` in its place. A blended draw keeps the original art: the
/// GPU quad cannot blend like the software sprite does.
fn draw_image(
    framebuffer: &mut Framebuffer,
    image: &Decoded,
    hd: draw::HdArt,
    src: Option<[i32; 4]>,
    dst: [i32; 4],
    opts: &Options,
) {
    let (width, height) = image.size();
    let src = src.unwrap_or([0, 0, width as i32, height as i32]);
    let draw = |framebuffer: &mut Framebuffer| match image {
        Decoded::Rgba(image) => {
            if opts.blend == SpriteBlend::Opaque {
                framebuffer.draw_rgba_sprite(image, src, dst, opts.brightness);
            } else {
                framebuffer.draw_rgba_sprite_blended(image, src, dst, opts.brightness, opts.blend);
            }
        }
        Decoded::Indexed(texture) => {
            if opts.blend == SpriteBlend::Opaque {
                framebuffer.draw_indexed_sprite(
                    texture,
                    src,
                    dst,
                    opts.clut,
                    opts.brightness,
                    opts.tint,
                );
            } else {
                framebuffer.draw_indexed_sprite_blended(
                    texture,
                    src,
                    dst,
                    opts.clut,
                    opts.brightness,
                    opts.tint,
                    opts.blend,
                );
            }
        }
    };
    match hd.filter(|_| opts.blend == SpriteBlend::Opaque) {
        Some(hd) => {
            let (corners, uv) = draw::sprite_quad(&hd, src, dst, [false; 2]);
            // An RGBA sprite ignores the tint; an indexed one mixes it in.
            let tint = match image {
                Decoded::Rgba(_) => Tint::White,
                Decoded::Indexed(_) => opts.tint,
            };
            let scale = crate::render::brightness_scale(opts.brightness);
            let colour = draw::sprite_colour(tint, scale);
            framebuffer.draw_replaced(hd, corners, uv, colour, draw);
        }
        None => draw(framebuffer),
    }
}

/// The original pad word of the input's edge flags, the same bit layout
/// the film and attract paths consume.
pub(super) fn pad_word(input: UiInput) -> u16 {
    let mut word = 0u16;
    for (flag, bit) in [
        (input.confirm, 0x0001),
        (input.cancel, 0x0002),
        (input.start, 0x0008),
        (input.up, 0x0010),
        (input.down, 0x0020),
        (input.left, 0x0040),
        (input.right, 0x0080),
        (input.page_left, 0x0400),
        (input.page_right, 0x0800),
    ] {
        if flag {
            word |= bit;
        }
    }
    word
}

/// The pad word of the held confirm, direction keys and L1/R1.
pub(super) fn held_word(input: UiInput) -> u16 {
    let mut word = 0u16;
    for (flag, bit) in [
        (input.held_confirm, 0x0001),
        (input.held_up, 0x0010),
        (input.held_down, 0x0020),
        (input.held_left, 0x0040),
        (input.held_right, 0x0080),
        (input.held_page_left, 0x0400),
        (input.held_page_right, 0x0800),
    ] {
        if flag {
            word |= bit;
        }
    }
    word
}

/// The cue a script name selects.
fn cue_of(name: &str) -> Option<UiCue> {
    Some(match name {
        "cursor" => UiCue::Cursor,
        "cancel" | "error" => UiCue::Cancel,
        "decide" => UiCue::Decide,
        "title" => UiCue::Title,
        "select_cursor" => UiCue::SelectCursor,
        "select_confirm" => UiCue::SelectConfirm,
        "save_cursor" => UiCue::SaveCursor,
        "save_confirm" => UiCue::SaveConfirm,
        "page_turn" => UiCue::PageTurn,
        "map_zoom" => UiCue::MapZoom,
        "map_label" => UiCue::MapLabel,
        _ => return None,
    })
}

mod methods;

/// A save slot's header fields as a Lua table.
fn slot_table(lua: &Lua, info: &save::SaveSlotInfo) -> mlua::Result<Table> {
    let row = lua.create_table()?;
    row.set("character", info.character)?;
    row.set("saves", info.saves)?;
    row.set("stage", info.stage)?;
    row.set("room", info.room)?;
    row.set("mansion2", info.mansion2)?;
    row.set("re2", info.re2)?;
    Ok(row)
}

/// The action an `api:exit(name, arg, choice)` names. `choice` is a new
/// game's table of `easy`, `hard_mode` and `censorship_off`, each true or 1
/// when set ([`super::NewGameChoice`]), or a film's, with `claire`.
fn action_of(name: &str, arg: Option<i64>, choice: Option<&Table>) -> mlua::Result<ScreenAction> {
    let given = arg;
    let arg = |what: &str| {
        arg.ok_or_else(|| mlua::Error::RuntimeError(format!("api:exit(\"{name}\") needs {what}")))
    };
    Ok(match name {
        "title" => ScreenAction::Title,
        "char_select" => ScreenAction::CharSelect,
        "save_load" => ScreenAction::SaveLoad,
        "continue" => ScreenAction::Continue {
            prologue: given.is_some_and(|ran| ran != 0),
        },
        "attract" => ScreenAction::Attract {
            reel: given
                .map(|reel| {
                    u8::try_from(reel).map_err(|_| {
                        mlua::Error::RuntimeError(
                            "api:exit(\"attract\"): the reel is not a demo's number".into(),
                        )
                    })
                })
                .transpose()?,
        },
        "resume" => ScreenAction::Resume,
        "quit" => ScreenAction::Quit,
        "ending_result" => ScreenAction::EndingResult,
        "ending_epilogue" => ScreenAction::EndingEpilogue,
        "ending_save" => ScreenAction::EndingSave,
        "new_game" => ScreenAction::NewGame {
            character: arg("a character")?.clamp(0, 255) as u8,
            choice: new_game_choice(choice)?,
        },
        "load_game" => ScreenAction::LoadGame {
            slot: arg("a slot")?.clamp(0, 7) as usize,
        },
        "film" => ScreenAction::Film {
            id: u8::try_from(arg("a film id")?).map_err(|_| {
                mlua::Error::RuntimeError("api:exit(\"film\"): the id is not a film's".into())
            })?,
            claire: match choice {
                Some(choice) => exit_flag(choice, name, "claire")?,
                None => false,
            },
            gallery: match choice {
                Some(choice) => exit_flag(choice, name, "gallery")?,
                None => false,
            },
        },
        other => {
            return Err(mlua::Error::RuntimeError(format!(
                "unknown exit action `{other}`"
            )));
        }
    })
}

/// A flag of the table an `api:exit(name, ...)` passes: true, or a number
/// other than 0.
fn exit_flag(table: &Table, name: &str, key: &str) -> mlua::Result<bool> {
    Ok(match table.get::<Value>(key)? {
        Value::Nil => false,
        Value::Boolean(on) => on,
        Value::Integer(n) => n != 0,
        Value::Number(n) => n != 0.0,
        other => {
            return Err(mlua::Error::RuntimeError(format!(
                "api:exit(\"{name}\"): `{key}` is a {}, not a flag",
                other.type_name()
            )));
        }
    })
}

/// A new game's choice from `api:exit("new_game", character, choice)`.
fn new_game_choice(choice: Option<&Table>) -> mlua::Result<super::NewGameChoice> {
    let Some(choice) = choice else {
        return Ok(super::NewGameChoice::default());
    };
    let set = |key: &str| exit_flag(choice, "new_game", key);
    let extra_player = match choice.get::<Value>("extra_player")? {
        Value::Nil => 0,
        Value::Integer(0xC) => 0xC,
        Value::Integer(0xD) => 0xD,
        other => {
            return Err(mlua::Error::RuntimeError(format!(
                "api:exit(\"new_game\"): `extra_player` is {other:?}, not 0xC (HUNK) or 0xD (TOFU)"
            )));
        }
    };
    let exbattle = match choice.get::<Value>("exbattle")? {
        Value::Nil => None,
        Value::Integer(player @ (0 | 1 | 0xB | 0xE)) => {
            let byte = |key: &str, range: std::ops::RangeInclusive<i64>| -> mlua::Result<u8> {
                match choice.get::<Value>(key)? {
                    Value::Integer(n) if range.contains(&n) => Ok(n as u8),
                    other => Err(mlua::Error::RuntimeError(format!(
                        "api:exit(\"new_game\"): `{key}` is {other:?}, not in {range:?}"
                    ))),
                }
            };
            Some(super::ExBattleChoice {
                player: player as u8,
                level: byte("exbattle_level", 1..=3)?,
                levels: byte("exbattle_levels", 0..=255)?,
                bits: byte("exbattle_bits", 0..=255)?,
            })
        }
        other => {
            return Err(mlua::Error::RuntimeError(format!(
                "api:exit(\"new_game\"): `exbattle` is {other:?}, not 0 (Leon), 1 (Claire), 0xB (Chris) or 0xE (Ada)"
            )));
        }
    };
    Ok(super::NewGameChoice {
        easy: set("easy")?,
        hard_mode: set("hard_mode")?,
        censorship_off: set("censorship_off")?,
        extra_player,
        exbattle,
    })
}

/// The error a draw method reports outside `draw`.
fn no_draw() -> mlua::Error {
    mlua::Error::RuntimeError("draw methods need `draw(api)`".into())
}

#[cfg(test)]
mod re2_computer_tests;
#[cfg(test)]
mod re2_gallery_tests;
#[cfg(test)]
mod re2_result_tests;
#[cfg(test)]
mod re2_save_tests;
#[cfg(test)]
mod re2_status_tests;
#[cfg(test)]
mod re2_title_tests;
#[cfg(test)]
mod screen_tests;
#[cfg(test)]
mod tests;
