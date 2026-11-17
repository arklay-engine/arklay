//! The app's side of the player bot: the bot attached to an [`App`] and the
//! update hooks that hand it the pad.

use anyhow::{Result, bail};

use super::super::{
    App, AppFlow, GameSession, Mode, gui_cues, player, seeded_rand_state, ui, watch,
};
use super::{BotInput, BotRuntime, BotScreen, RunFacts, RunState};
use crate::ui::UiInput;

/// The bot attached to an app ([`App::attach_bot`]): its runtime and what it
/// sets on every session it plays.
pub(in crate::engine) struct AttachedBot {
    pub(in crate::engine) runtime: BotRuntime,
    /// The game-RNG seed; `None` leaves the game's own stream untouched.
    seed: Option<u32>,
    /// The sessions the app's bots have seeded so far: a bot attached again
    /// in the same app (a second `bot_start`) carries the count on, so it
    /// plays a fresh stream rather than the first bot's again.
    seeded_sessions: u32,
    /// Every session the bot runs has the INVINCIBLE cheat on.
    invincible: bool,
}

impl AttachedBot {
    /// Seed `session`'s game RNG from the bot seed (a no-op without it),
    /// count the session, and turn on the INVINCIBLE cheat for an invincible
    /// run.
    fn prepare(&mut self, session: &mut GameSession) {
        if let Some(seed) = self.seed {
            session.game.rand_state = seeded_rand_state(seed, self.seeded_sessions);
            self.seeded_sessions += 1;
        }
        session.game.debug.invincible |= self.invincible;
    }
}

impl App {
    /// The attached bot's runtime.
    pub(in crate::engine) fn bot(&self) -> Option<&BotRuntime> {
        self.bot.as_ref().map(|bot| &bot.runtime)
    }

    /// Whether a bot is attached.
    pub(in crate::engine) fn bot_attached(&self) -> bool {
        self.bot.is_some()
    }

    /// Whether the running bot, rather than the human, is feeding the pad.
    ///
    /// The interactive loop must not poll film skips with the human word while
    /// this is true: the poll makes the film session the external sampler, so
    /// the fixed tick would then ignore the bot's `ui.confirm` edge.
    pub(in crate::engine) fn bot_owns_input(&self) -> bool {
        self.bot().is_some_and(|bot| !bot.finished())
    }

    /// Attach the pack's Lua bot to the running game session.
    pub(in crate::engine) fn attach_bot(
        &mut self,
        risk: f64,
        goal_room: Option<&str>,
        seed: Option<u32>,
        params: &[(String, String)],
        invincible: bool,
    ) -> Result<()> {
        if !matches!(self.mode, Mode::Play(_)) {
            bail!("the bot requires a game session");
        }
        let runtime = BotRuntime::load(&self.pack)?
            .with_risk(risk)
            .with_goal_room(goal_room)
            .with_params(&self.pack, params)?;
        let summary = runtime.params_summary();
        if !summary.is_empty() {
            eprintln!("[bot] params: {summary}");
        }
        if invincible {
            eprintln!("[bot] invincible: the INVINCIBLE cheat is on for the run");
        }
        let seeded_sessions = self.bot.as_ref().map_or(0, |bot| bot.seeded_sessions);
        let bot = self.bot.insert(AttachedBot {
            runtime,
            seed,
            seeded_sessions,
            invincible,
        });
        // The boot already started the first session; it has not ticked yet.
        if let Mode::Play(session) = &mut self.mode {
            bot.prepare(session);
        }
        Ok(())
    }

    /// Reload what of the attached bot the changed files belong to (`--watch`).
    pub(in crate::engine) fn watch_bot(&mut self, kinds: &[watch::Changed]) {
        let Some(bot) = self.bot.as_mut() else {
            return;
        };
        if kinds.contains(&watch::Changed::BotRoute) {
            match bot.runtime.reload(&self.pack) {
                Ok(()) => eprintln!("[watch] reloaded the bot route"),
                Err(err) => eprintln!("[watch] bot route reload failed: {err:#}"),
            }
        } else if kinds.contains(&watch::Changed::BotMeta) {
            bot.runtime.reload_meta();
            eprintln!("[watch] invalidated the bot room metadata");
        }
    }

    /// Whether the running bot's player has died.
    fn bot_player_dead(&self) -> bool {
        matches!(&self.mode, Mode::Play(session) if session.player_dead())
    }

    /// How the attached bot's run stands ([`super::play`]): a dead player
    /// with a save to load plays on, through the title's load screen.
    pub(in crate::engine) fn bot_run_state(&self) -> RunState {
        let Some(runtime) = self.bot() else {
            return RunState::Running;
        };
        if runtime.finished() {
            RunState::Finished
        } else if self.bot_player_dead() && !runtime.has_save() {
            RunState::Stranded
        } else {
            RunState::Running
        }
    }

    /// What the bot's report reads from the session when it is written: the
    /// ending the game's own rule picks, once the escape scene has completed
    /// the game (the scripted death variant), and the play timer.
    pub(in crate::engine) fn bot_run_facts(&self) -> RunFacts {
        let Mode::Play(session) = &self.mode else {
            return RunFacts::default();
        };
        let game = &session.game;
        RunFacts {
            ending: session
                .game_completed()
                .then(|| ui::debug_menu::default_ending(game)),
            game_time_ticks: Some(game.play_timer()),
        }
    }

    /// Ready a session the app is entering for the attached bot.
    pub(in crate::engine) fn start_bot_session(&mut self, session: &mut GameSession) {
        let Some(bot) = self.bot.as_mut() else {
            return;
        };
        // A session the bot reached through the load screen is its reload
        // and carries the run on; any other is a fresh run.
        if let Err(err) = bot.runtime.start_session(&self.pack) {
            // Ending the run (rather than dropping the bot) keeps the old
            // route state from running on and lets a headless run report it.
            eprintln!("[bot] could not reset the bot route: {err:#}");
            bot.runtime
                .fail(format!("could not reset the route: {err:#}"));
        }
        // The bot seed: every session the bot runs gets its own stream, the
        // reload after a death included (a save does not carry the RNG state).
        bot.prepare(session);
    }

    /// A running bot owns the pad on the title and its load screen too,
    /// where it loads its latest save after a death: its UI input there in
    /// place of `ui`.
    pub(in crate::engine) fn bot_screen_ui(&mut self, ui: UiInput) -> Result<UiInput> {
        Ok(match self.bot_screen_input()? {
            Some(bot) => {
                self.feed_bot_native_screen(&bot.ui);
                bot.ui
            }
            None => ui,
        })
    }

    /// A running bot owns the pad: its tick runs once, before the game tick,
    /// and its input replaces the human one. Once it finishes the normal
    /// input returns.
    pub(in crate::engine) fn bot_pad(
        &mut self,
        ui: UiInput,
        input: player::Input,
        action: bool,
    ) -> Result<(UiInput, player::Input, bool)> {
        let Mode::Play(session) = &mut self.mode else {
            return Ok((ui, input, action));
        };
        let Some(runtime) = self
            .bot
            .as_mut()
            .map(|bot| &mut bot.runtime)
            .filter(|runtime| !runtime.finished())
        else {
            return Ok((ui, input, action));
        };
        // The bot's only output is pad/UI input: equip, heal, item use and
        // room actions go through the menu and the action button exactly as
        // a player's input does.
        let bot = runtime.tick(session, &self.pack)?;
        self.feed_bot_native_screen(&bot.ui);
        Ok((bot.ui, bot.input, bot.action))
    }

    /// The player's host keys while a bot plays: the bot owns the pad, but
    /// not Escape, F1 or F9. Escape asks for the native in-game menu and
    /// freezes the update; F1 and F9 reach the session alone. While an
    /// overlay they opened is up ([`GameSession::host_overlay`]) the bot is
    /// not ticked and the player's input drives the overlay; it resumes when
    /// the overlay closes. `None` when the bot plays this update as usual,
    /// which is every update of a run nobody presses those keys in, so the
    /// bot's ticks for a seed are unchanged.
    pub(in crate::engine) fn bot_host_update(
        &mut self,
        ui: UiInput,
        input: player::Input,
        action: bool,
    ) -> Result<Option<AppFlow>> {
        if !self.bot_owns_input() {
            return Ok(None);
        }
        let Mode::Play(session) = &mut self.mode else {
            return Ok(None);
        };
        // The in-game menu freezes the room wherever it is: nothing ticks
        // under it, not even a door's animation.
        if session.system_menu_open || session.system_menu_requested {
            return Ok(Some(AppFlow::Continue));
        }
        if ui.system_menu && session.attract.is_none() {
            session.system_menu_requested = true;
            return Ok(Some(AppFlow::Continue));
        }
        if session.host_overlay() {
            return self.play_update(ui, input, action).map(Some);
        }
        if ui.debug_menu || ui.return_title {
            let host = UiInput {
                debug_menu: ui.debug_menu,
                return_title: ui.return_title,
                any: ui.any,
                ..UiInput::default()
            };
            return self
                .play_update(host, player::Input::default(), false)
                .map(Some);
        }
        Ok(None)
    }

    /// Hand the running bot's pad presses to the open native save/load
    /// screen, which reads key events rather than the tick's input: the
    /// same keys a player's presses give it.
    fn feed_bot_native_screen(&mut self, ui: &UiInput) {
        let events = ui::save_load::pad_events(ui);
        if events.is_empty() {
            return;
        }
        let Some(screen) = self.native_screen() else {
            return;
        };
        let sounds = screen.handle(&events);
        self.play_ui_cues(gui_cues(sounds));
    }

    /// The running bot's input on the title or the title's load screen, read
    /// from what the screen shows; `None` on any other screen or without a
    /// running bot.
    fn bot_screen_input(&mut self) -> Result<Option<BotInput>> {
        let Some(runtime) = self
            .bot
            .as_mut()
            .map(|bot| &mut bot.runtime)
            .filter(|runtime| !runtime.finished())
        else {
            return Ok(None);
        };
        let screen = match &self.mode {
            Mode::Title(screen) => BotScreen::title(screen.view().as_ref()),
            Mode::Load(screen) => BotScreen::Load(screen.bot_view()),
            _ => return Ok(None),
        };
        runtime.tick_screen(screen).map(Some)
    }
}
