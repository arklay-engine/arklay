//! RE 2's option screen entered from play: `Config` (`0x449FD0`), task 1's
//! routine while it runs, which `Game_loop` (`0x4BF810`) starts on the
//! option key.
//!
//! `Game_loop` tests the raw pad's `0x100` edge (`0x9885FE`, which
//! `Pad_set` writes and its own `Vk_press & 2`, F5, raises outside a demo)
//! where it tests START and the map key, under the same gate, and first:
//! it raises `fg_system`'s `0x8000000` and `fg_status`'s `0x8000`, and the
//! three keys share the decide cue and game mode 1. With START or the map
//! key on the same tick the option screen wins, since step 5 tests
//! `0x8000000` before it starts `Status`.
//!
//! | tick | what |
//! |---|---|
//! | T | the press: `Game` 1, the decide cue, no one moves |
//! | T+1 to T+5 | modes 1 and 2 as START's open: the room fades out over three frames that still run under `fg_stop`, then black |
//! | T+6 | step 5: `Task_execute(1, Config)`, whose step 0 runs in the same pass (`FUN_0044af00`, `Config_set_data`) |
//! | then | `Config_main` as from the title, over its own picture |
//! | X | step 2: `FUN_0044b020` puts the player back and loads the room's picture, and `FUN_0044aed0` ends the task and wakes `Game_loop` |
//! | X+1 | step 8 clears `fg_status`'s `0x8040` |
//! | X+2 | step 0xB: game modes 3 and 5, the room fades in |
//!
//! `FUN_0044af00` keeps the player's work and gives them their handgun
//! (`Type` 2 for Leon's even `Id`s, 3 for Claire's odd ones, 0x13 for
//! 0xB, 1 for 0xD and 0 otherwise), and `Config_set_data` puts the work in
//! the page's place (-1530, 2020, 2700, facing 0xC00): the page's model is
//! the player. `FUN_0044b020` puts all of it back. The screen itself is the
//! title's (`ui/title.lua`'s option task, opened with `config`).

use super::*;

/// The open's tick on which step 5 starts `Config` (as [`RE2_SAVE_START`]).
pub(super) const RE2_CONFIG_START: u8 = 6;

/// `Config_set_data`'s place for the page's model (`0x989F28`), and its turn.
const PAGE_PLACE: [i32; 3] = [-1530, 2020, 2700];
const PAGE_TURN: u16 = 0xC00;

/// The option screen from play, by `Game_loop`'s steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Re2Config {
    /// The open's tick, 1 on T+1 (game mode 2), as START's.
    Open(u8),
    /// The screen is up (task 1 runs `Config`; `Game_loop` suspended).
    Screen,
    /// X: the screen's step 2 has run; still black.
    Closed,
    /// X+1: `Game_loop`'s step 8.
    Woken,
    /// X+2: step 0xB, game modes 3 and 5.
    Return,
}

impl Re2Config {
    /// The trace's `gm`: `Game`'s low byte at the end of the tick.
    pub(super) fn game_mode(self) -> u8 {
        match self {
            Self::Open(_) | Self::Screen | Self::Closed | Self::Woken => 2,
            Self::Return => 5,
        }
    }

    /// What the frame shows: START's open, then the screen, which draws
    /// itself over black.
    pub(super) fn view(self) -> Option<Re2StatusView> {
        match self {
            Self::Open(ticks) => Some(re2_status_open_view(ticks)),
            Self::Screen | Self::Closed | Self::Woken => Some(Re2StatusView::Black),
            Self::Return => None,
        }
    }

    /// Whether the open's tick still runs a `Game_loop` frame under
    /// `fg_stop` that shows the room.
    fn stopped_frame(self) -> bool {
        matches!(self, Self::Open(1..=3))
    }
}

/// The handgun `FUN_0044af00` gives the player with `Id` `id`: the page's
/// model holds it.
pub(super) fn page_weapon(id: u8) -> u16 {
    match id {
        0 | 2 | 4 | 6 | 8 | 10 | 12 => 2,
        1 | 3 | 5 | 7 | 9 | 14 => 3,
        11 => 0x13,
        13 => 1,
        _ => 0,
    }
}

/// What `FUN_0044af00` keeps of the player for `FUN_0044b020` to put back:
/// `pPl` (the place, the turn and the routine and motion work, which
/// KEYCONFIG's model moves), and the weapon whose `Pl_weapon_ch` the return
/// repeats when the page's handgun was another.
#[derive(Debug, Clone)]
pub(super) struct KeptPlayer {
    pos: [i32; 3],
    angle: u16,
    work: Box<crate::player2::Work>,
    weapon: u8,
}

impl GameSession {
    /// The option key in play, under START's gate: the request is START's,
    /// for the option screen.
    pub(super) fn wants_re2_config(&self, ui: UiInput) -> bool {
        self.game.is_re2_room && ui.option && !self.in_demo()
    }

    /// The tick after the press: `Game_loop`'s modes 1 and 2 start.
    pub(super) fn begin_re2_config_open(&mut self) {
        self.game.re2_config_request = false;
        self.game.re2_key_map = None;
        self.re2_config = Some(Re2Config::Open(1));
        self.game.re2.stopped_door = false;
    }

    /// The open's count, at the top of the tick; the return's tick is the
    /// last of it.
    pub(super) fn step_re2_config(&mut self) {
        self.re2_config = match self.re2_config {
            // A door the stopped frames' `Sce_at` opened put `Game` back
            // to 1: on the tick the fade is done the open starts again, as
            // START's does.
            Some(Re2Config::Open(ticks)) if ticks < RE2_CONFIG_START => {
                let ticks = ticks + 1;
                if ticks == RE2_STATUS_BLACK && std::mem::take(&mut self.game.re2.stopped_door) {
                    Some(Re2Config::Open(1))
                } else {
                    Some(Re2Config::Open(ticks))
                }
            }
            Some(Re2Config::Closed) => Some(Re2Config::Woken),
            Some(Re2Config::Woken) => Some(Re2Config::Return),
            Some(Re2Config::Return) => None,
            other => other,
        };
    }

    /// One tick of the option screen from play. Returns whether it took
    /// the tick.
    pub(super) fn tick_re2_config(&mut self, pack: &Pack, ui: UiInput) -> bool {
        let Some(config) = self.re2_config else {
            return false;
        };
        match config {
            Re2Config::Open(ticks) if ticks < RE2_CONFIG_START => {
                if config.stopped_frame() && self.game.entities[0].health >= 0 {
                    self.re2_stopped_enemy_pass(pack);
                    self.re2_stopped_frame_draw(true);
                    self.re2_stopped_areas();
                }
            }
            Re2Config::Open(_) => self.start_re2_config(pack, ui),
            Re2Config::Screen => self.tick_re2_config_screen(pack, ui),
            Re2Config::Closed | Re2Config::Woken => {}
            Re2Config::Return => self.return_re2_room(pack),
        }
        self.play_re2_tick_sounds(pack);
        self.tick_re2_fades();
        true
    }

    /// Step 5's `Task_execute(1, Config)` and the task's step 0 in the same
    /// pass: `FUN_0044af00` keeps the player and gives them their handgun,
    /// `Config_set_data` puts them in the page's place, and the screen
    /// runs its first tick. A screen that can't open hands the room back.
    fn start_re2_config(&mut self, pack: &Pack, ui: UiInput) {
        let id = self.game.re2.pl_id;
        let weapon = page_weapon(id);
        let player = &mut self.game.entities[0];
        self.re2_config_player = Some(KeptPlayer {
            pos: player.pos,
            angle: player.angle,
            work: Box::new(self.game.re2.player.clone()),
            weapon: self.game.equipped_weapon_id,
        });
        player.pos = PAGE_PLACE;
        player.angle = PAGE_TURN;
        self.game.pad_repeat.set(game::pad_repeat::CONFIG_SETTING);
        let args = [
            ("config", ScreenArg::Bool(true)),
            ("id", ScreenArg::Int(i64::from(id))),
            ("weapon", ScreenArg::Int(i64::from(weapon))),
            (
                "disc",
                ScreenArg::Int(i64::from(re2_player_disc(&self.game))),
            ),
        ];
        let volumes = self.game.re2.volumes.unwrap_or([100, 100]);
        let work = self.game.re2.player.clone();
        let opened = {
            let mut cx = self.ui_context(pack);
            LuaScreen::open_screen("title", &args, ScreenAction::Resume, &mut cx).map(
                |mut screen| {
                    // KEYCONFIG's model is `pPl`: the player as play left
                    // them.
                    screen.set_option_player(&work);
                    screen.signal_with(
                        "on_volumes",
                        (i64::from(volumes[0]), i64::from(volumes[1])),
                        &cx,
                    );
                    (screen, cx.cues.into_inner())
                },
            )
        };
        // A title that didn't take `config` (an older pack's) would run as
        // the title itself, with no way back to play.
        let opened = opened.and_then(|(screen, cues)| {
            if screen.published("opt_config") == Some(1) {
                Ok((screen, cues))
            } else {
                Err(anyhow::anyhow!(
                    "the pack's ui/title.lua has no option screen from play"
                ))
            }
        });
        match opened {
            Ok((screen, cues)) => {
                self.re2_config_shown =
                    SCREEN_VOLUME_KEYS.map(|key| screen_volume(screen.published(key)));
                self.re2_config_screen = Some(Box::new(screen));
                self.play_re2_screen_cues(pack, cues);
                self.re2_config = Some(Re2Config::Screen);
                self.tick_re2_config_screen(pack, ui);
            }
            Err(err) => {
                crate::warning!("warning: the option screen unavailable: {err:#}");
                self.end_re2_config();
            }
        }
    }

    /// One tick of the screen (`Config_main` and its draw), its cues, and
    /// a change it made to the volumes, which are the run's.
    fn tick_re2_config_screen(&mut self, pack: &Pack, ui: UiInput) {
        let (result, cues) = {
            let Self {
                re2_config_screen,
                save_dir,
                font,
                text,
                game,
                screen_statics,
                ..
            } = self;
            let Some(screen) = re2_config_screen.as_mut() else {
                return;
            };
            let cx = UiContext {
                statics: Some(screen_statics),
                authentic: game.player_options.authentic,
                pack,
                save_dir,
                font: font.as_ref(),
                text: Some(text),
                ticks: game.frame,
                cues: std::cell::RefCell::new(Vec::new()),
                game: Some(game),
            };
            let result = screen.update(&cx, ui);
            (result, cx.cues.into_inner())
        };
        self.play_re2_screen_cues(pack, cues);
        self.poll_re2_config_volumes();
        self.mirror_re2_config_model();
        if result != ScreenResult::Continue {
            self.end_re2_config();
        }
    }

    /// KEYCONFIG's model is `pPl` in the original, so the player is where
    /// the page moved it, with its routines and motion, until
    /// `FUN_0044b020` puts the player back.
    fn mirror_re2_config_model(&mut self) {
        let Some(model) = self
            .re2_config_screen
            .as_ref()
            .and_then(|screen| screen.option_model())
        else {
            return;
        };
        let player = &mut self.game.entities[0];
        player.pos = model.body.pos;
        player.angle = model.body.angle;
        let work = &mut self.game.re2.player;
        work.routine = model.work.routine;
        work.move_no = model.work.move_no;
        work.move_cnt = model.work.move_cnt;
        work.hokan = model.work.hokan;
        work.d_life_u = model.work.d_life_u;
        work.spd = model.work.spd;
    }

    /// The SOUND page's volumes as the screen publishes them: a change is
    /// the run's, which the save keeps and the app takes for its settings.
    fn poll_re2_config_volumes(&mut self) {
        let Some(screen) = self.re2_config_screen.as_ref() else {
            return;
        };
        let mut volumes = self.game.re2.volumes.unwrap_or([100, 100]);
        let mut changed = false;
        for (index, key) in SCREEN_VOLUME_KEYS.into_iter().enumerate() {
            let shown = &mut self.re2_config_shown[index];
            if let (Some(before), Some(percent)) = (*shown, screen_volume(screen.published(key)))
                && before != percent
            {
                *shown = Some(percent);
                volumes[index] = percent;
                changed = true;
            }
        }
        if changed {
            self.game.re2.volumes = Some(volumes);
            self.re2_config_volumes = Some(volumes);
        }
    }

    /// The screen's step 2: `FUN_0044b020` puts the player back, with
    /// their weapon's `Pl_weapon_ch` when the page's handgun was another,
    /// and loads the room's picture (`BYTE_0098f07b`), then `FUN_0044aed0`
    /// ends the task and wakes `Game_loop` for the next tick.
    fn end_re2_config(&mut self) {
        self.re2_config_screen = None;
        if let Some(kept) = self.re2_config_player.take() {
            let player = &mut self.game.entities[0];
            player.pos = kept.pos;
            player.angle = kept.angle;
            self.game.re2.player = *kept.work;
            let id = self.game.re2.pl_id;
            if u16::from(kept.weapon) != page_weapon(id) {
                self.screen_statics.re2_equip(id, kept.weapon);
            }
        }
        self.game.re2.bg_load = true;
        self.game.mask_reload = true;
        self.re2_config = Some(Re2Config::Closed);
    }

    /// The volumes the screen changed, once, for the app's settings.
    pub(super) fn take_re2_config_volumes(&mut self) -> Option<[u8; 2]> {
        self.re2_config_volumes.take()
    }

    /// The trace's `opt`, `kc` and `kcm` while the screen is up.
    pub(super) fn re2_config_trace(&self) -> String {
        match self.re2_config_screen.as_deref() {
            Some(screen) => option_trace_fields(screen),
            None => "opt=- kc=- kcm=-".to_string(),
        }
    }

    /// Draw the screen, over the black the session cleared.
    pub(super) fn draw_re2_config(&mut self, pack: &Pack) {
        let Self {
            re2_config_screen,
            save_dir,
            screen_statics,
            font,
            text,
            game,
            framebuffer,
            ..
        } = self;
        let Some(screen) = re2_config_screen.as_mut() else {
            return;
        };
        let cx = UiContext {
            statics: Some(screen_statics),
            authentic: game.player_options.authentic,
            pack,
            save_dir,
            font: font.as_ref(),
            text: Some(text),
            ticks: game.frame,
            cues: std::cell::RefCell::new(Vec::new()),
            game: Some(game),
        };
        screen.draw(&cx, framebuffer);
    }
}

/// The title's `opt`, `kc` and `kcm` fields from what its script publishes
/// (`scripts/oracle-re2.py`'s `option_trace` and `key_config_trace`).
pub(super) fn option_trace_fields(screen: &LuaScreen) -> String {
    let v = |key: &str| screen.value(key);
    let mut line = if v("opt_on") != 0 {
        format!(
            "opt={},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            v("opt_step"),
            v("opt_state"),
            v("opt_cursor"),
            v("opt_type"),
            v("opt_wait"),
            v("opt_speaker"),
            v("opt_quality"),
            v("opt_bgm"),
            v("opt_se"),
            v("opt_blink_mode"),
            v("opt_blink_quality"),
            v("opt_blink_bgm"),
            v("opt_blink_se"),
            v("opt_blink_exit"),
        )
    } else {
        "opt=-".to_string()
    };
    // KEYCONFIG's page and its model: the script writes the fields' values.
    match screen.state_str("kc") {
        Some(page) => line.push_str(&format!(" kc={page}")),
        None => line.push_str(" kc=-"),
    }
    match screen.state_str("kcm") {
        Some(model) => line.push_str(&format!(" kcm={model}")),
        None => line.push_str(" kcm=-"),
    }
    line
}
