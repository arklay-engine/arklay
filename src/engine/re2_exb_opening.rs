//! Extreme Battle's opening in play (`Exbtl_Opening`, `0x446D50`;
//! [`game::exb_opening2`]): `Game_loop`'s step 1 (`0x4BF810`) starts it as
//! task 1 on the tick after its first, under `fg_system`'s `0x1000000` while
//! `fg_common`'s `0xFE` is down, which it raises. The task's first pass runs
//! after the room's tick and suspends the room's task; from the next tick
//! only the task runs, until the tick its end wakes the room, which runs
//! again from the tick after.
//!
//! | tick | what |
//! |---|---|
//! | G | `Game_loop`'s first tick (`Title_game_init`'s room) |
//! | G+1 | step 1: `fg_common`'s `0xFE` and `Task_execute(1, Exbtl_Opening)`, then step 3's game mode 5 (arklay's entry from the title, [`GameSession::enter_from_re2_title`]), then the task's first pass |
//! | G+2 on | the task alone: its wait, its lines and their fades |
//! | E | the end (`0x447030`): `Task_signal(0)` |
//! | E+1 | the room again |
//!
//! What it draws replaces the room's frame: the background is black
//! (`Bg_set_mode(2, 0)`) and nothing of the room draws while its task is
//! suspended.

use super::*;

use game::exb_opening2::{Effect, LINE_X, Opening, STATIC_SIZE, STATIC_TEXELS, View};

/// The static's pictures (textures 0x13 and 0x14).
const STATIC_PAGES: [&str; 2] = ["ui/exbattle/open07a", "ui/exbattle/open07b"];

/// The opening's room bank cues, `Snd_se_on(0x2NN0000)`.
const CUE_TYPE: u32 = 0x0200_0000;

impl GameSession {
    /// `Game_loop`'s step 1, on the tick arklay enters the room from the
    /// title (game mode 5): under Extreme Battle, with the opening not yet
    /// shown, it raises `fg_common`'s `0xFE` and starts the task, whose
    /// first pass runs after the tick.
    pub(super) fn arm_re2_exb_opening(&mut self) {
        use crate::scd2::host::FG_SYSTEM_EXTREME;
        let re2 = &mut self.game.re2;
        if self.game.is_re2_room && re2.flag(0, FG_SYSTEM_EXTREME) && !re2.flag(COMMON_BANK, SHOWN)
        {
            re2.set_flag(COMMON_BANK, SHOWN, 1);
            self.re2_exb_opening_due = true;
        }
    }

    /// Whether the tick's step 1 started the task.
    pub(super) fn take_re2_exb_opening_due(&mut self) -> bool {
        std::mem::take(&mut self.re2_exb_opening_due)
    }

    /// `Task_execute(1, Exbtl_Opening)`: the task's first pass, after the
    /// room's tick has run its fades and its voice.
    pub(super) fn start_re2_exb_opening(&mut self, pack: &Pack) {
        let mut opening = Box::new(Opening::new());
        let effects = opening.tick(self.game.pad_repeat.trigger);
        self.re2_exb_static = Some(STATIC_PAGES.map(|stem| {
            let asset = pack.read_asset(stem, AssetClass::Image).ok().flatten()?;
            assets::decode_picture(&asset, assets::Geometry::Shipped, None).ok()
        }));
        self.re2_exb_view = Some(opening.view());
        self.re2_exb_opening = Some(opening);
        self.apply_re2_exb_opening(effects);
        // The room's tick has run `System_trans` and `Cd_system_control`:
        // the fades and the voice the pass set are seen as they stand.
        for write in std::mem::take(&mut self.game.re2.screen_fades) {
            write.apply(&mut self.room_fade);
        }
        self.room_fade_draw = self.room_fade.overlay();
        for (fade, drawn) in self
            .game
            .re2
            .fades
            .iter()
            .zip(&mut self.game.re2.fades_drawn)
        {
            *drawn = fade.overlay();
        }
        // `Xa_play` asks DirectSound for the line on its call, as a script's
        // `xa_on` does before the tick's voice step.
        if self.game.re2.voice.request.is_some() {
            tick_re2_voice(
                self.music.is_some(),
                &mut self.voice_cache,
                &mut self.game,
                pack,
            );
        }
    }

    /// A tick the opening holds the room: one pass of its task, its sounds,
    /// the voice and `System_trans`' fades. Returns whether it took the
    /// tick.
    pub(super) fn tick_re2_exb_opening(&mut self, pack: &Pack) -> bool {
        let Some(opening) = self.re2_exb_opening.as_mut() else {
            return false;
        };
        let effects = opening.tick(self.game.pad_repeat.trigger);
        self.re2_exb_view = Some(opening.view());
        let done = opening.done();
        // `Psx_main`'s `Swap_Cbuff` ends every frame, the room's task
        // suspended or not.
        self.game.re2.draw_buffer ^= 1;
        self.apply_re2_exb_opening(effects);
        let cut = self.sound_cut();
        play_re2_sounds(
            &mut self.music,
            &mut self.sfx_cache,
            &mut self.re2_sounds,
            pack,
            &self.loaded.room,
            &mut self.game,
            &mut self.player,
            cut,
        );
        bgm::apply_live(&mut self.music, &mut self.game, &mut self.bgm_cache, pack);
        tick_voice(&mut self.music, &mut self.voice_cache, &mut self.game, pack);
        self.tick_re2_fades();
        if done {
            self.re2_exb_opening = None;
            self.re2_exb_static = None;
        }
        true
    }

    /// What the opening's pass asks of the game.
    fn apply_re2_exb_opening(&mut self, effects: Vec<Effect>) {
        use crate::scd2::host::{FadeWrite, STATUS_BANK, STATUS_PLAYER, STATUS_VOICE};
        for effect in effects {
            match effect {
                Effect::Begin => {
                    *self.game.re2.fg_stop_mut() |= 0x0200_0000;
                    self.game.re2.screen_fades.push(FadeWrite::Adjust(-1));
                    for fade in &mut self.game.re2.fades {
                        fade.set_state(-1);
                    }
                }
                Effect::Voice { line, stage } => {
                    // `Xa_play(0, line)`: the bank of `scd_var_stage` + 1, by
                    // `fg_status`'s bit 31.
                    let stage = stage.map_or(self.game.origin().re2_stage(), |stage| stage + 1);
                    let re2 = &mut self.game.re2;
                    let player = u8::from(re2.flag(STATUS_BANK, STATUS_PLAYER));
                    re2.voice.request = Some(format!("voice/{player}/{stage:02}_{line:02x}.wav"));
                    re2.set_flag(STATUS_BANK, STATUS_VOICE, 1);
                }
                Effect::Cue(slot) => {
                    if let Some(id) = crate::se2::SoundId::decode(CUE_TYPE | u32::from(slot) << 16)
                    {
                        self.game
                            .re2
                            .sounds
                            .push(crate::scd2::host::SoundRequest::new(id, [0; 3]));
                    }
                }
                Effect::FadeClear => self
                    .game
                    .re2
                    .screen_fades
                    .push(FadeWrite::Set { kind: 2, add: 0 }),
                Effect::Fade(level) => self.game.re2.screen_fades.push(FadeWrite::Adjust(level)),
                Effect::End => {
                    let re2 = &mut self.game.re2;
                    // `Xa_stop`, while the line's bit is up.
                    if re2.flag(STATUS_BANK, STATUS_VOICE) {
                        re2.set_flag(STATUS_BANK, STATUS_VOICE, 0);
                        re2.voice = Default::default();
                        self.game.voice.stop_requested = true;
                    }
                    let place = self.game.re2_music_place();
                    let re2 = &mut self.game.re2;
                    *re2.fg_stop_mut() = 0;
                    // The entry's stop, which the room's tick would have
                    // taken down: the next tick's `Pad_set` reads the pad.
                    re2.door_key_stop = false;
                    re2.objects_key_stop = false;
                    // `BYTE_0098F07B`: the cut's background again.
                    re2.bg_load = true;
                    // `Snd_bgm_ctr(0x1000000)`, the main played, and
                    // `Snd_bgm_tbl_set(0x612FF37)`: room F12's music word.
                    re2.music.control(PLAY_MAIN, place);
                    re2.bgm.set(ROOM_F12, STAGE_F, MUSIC_F12, 0);
                    self.game.mask_reload = true;
                }
            }
        }
    }

    /// Whether the opening's frame replaces the room's.
    pub(super) fn re2_exb_opening_shows(&self) -> bool {
        self.re2_exb_opening.is_some()
    }

    /// The opening's frame: black, the lines in the message font, the
    /// static over them, and fade 0 over all.
    pub(super) fn draw_re2_exb_opening(&mut self) {
        self.framebuffer.clear();
        self.framebuffer.fill_rect(
            [
                0,
                0,
                self.framebuffer.width as i32,
                self.framebuffer.height as i32,
            ],
            [0, 0, 0, 255],
        );
        let Some(view) = self.re2_exb_view.clone() else {
            return;
        };
        if let Some(font) = &self.font {
            for &(y, text) in &view.lines {
                self.framebuffer.with_screen(|screen| {
                    font.draw_text(screen, LINE_X, y, font::Tint::White, 2, text, true);
                });
            }
        }
        self.draw_re2_exb_static(&view);
        self.draw_room_fade();
    }

    /// `FUN_00447830`'s static: each shown picture's 120 by 180 texels
    /// drawn at 80 by 120.
    fn draw_re2_exb_static(&mut self, view: &View) {
        let (page, shift) = view.static_page();
        let Some(Some(image)) = self.re2_exb_static.as_ref().map(|pages| &pages[page]) else {
            return;
        };
        for sprite in &view.statics {
            let src = [
                i32::from(sprite.u) + shift,
                i32::from(sprite.v),
                STATIC_TEXELS[0],
                STATIC_TEXELS[1],
            ];
            let dst = [sprite.x, sprite.y, STATIC_SIZE[0], STATIC_SIZE[1]];
            self.framebuffer
                .with_screen(|screen| screen.draw_rgba_sprite_scaled(image, src, dst, 255));
        }
    }

    /// The trace's `exo`: the opening's task while it runs.
    pub(super) fn re2_exb_opening_trace(&self) -> String {
        match &self.re2_exb_opening {
            Some(opening) => opening.trace(),
            None => "-".to_string(),
        }
    }
}

/// `Snd_bgm_ctr(0x1000000)`'s operands: the main's sequence played.
const PLAY_MAIN: [u8; 5] = [0, 1, 0, 0, 0];
/// `Snd_bgm_tbl_set(0x612FF37)`: stage 6 (1-based, the letter `F`), room
/// 0x12, and its word.
const STAGE_F: u8 = 6;
const ROOM_F12: u8 = 0x12;
const MUSIC_F12: u16 = 0xFF37;

/// `fg_common`, and its bit for the opening shown.
const COMMON_BANK: u8 = 4;
const SHOWN: u8 = 0xFE;
