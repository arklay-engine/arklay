//! RE 2's ARRANGE MODE pause: `Psx_main` (`0x4C3C70`) after `Pad_set`.
//!
//! Only ARRANGE MODE has it (`censorship_off`). Under its gate a new press
//! of the raw pad's `0x2` (`0x9885F8`, `Pad_set`'s trigger word) pauses
//! the game, and the next resumes it:
//!
//! | tick | what |
//! |---|---|
//! | P | the press: `bPause` (`0x98F1BA`), `GetLastTime`, `SetGameSeconds(1)` reads the seconds, and `Curtain2` and the word `Pause` go into the frame; the game runs the tick as any other |
//! | P+1 to R | `Pad_set`, `Movie_set(1)` and `VSync`, then `Psx_main` returns: no task, script, sound call, fade or draw runs, the presented frame holds, and `WinMain` counts each frame on the clock |
//! | R | the next press: `bPause` 0 and `SetGameSeconds` puts the seconds back |
//! | R+1 | the game runs again |
//!
//! The gate is tested on the paused ticks too, and nothing that runs then
//! changes what it reads. `Vk_press`'s `0x20`, which `Psx_main` tests with
//! the pad's bit, is `Pad_set`'s copy of its `0x10`, which nothing in the
//! executable or in Classic REbirth's `WndProc` raises. The pad's bit maps
//! to no `Key` bit, and the executable's keys leave it unbound (`KeyDef`'s
//! entry 6; the pad's table has it on button 16).

use super::*;

/// ARRANGE MODE's pause key, the raw pad's `0x2` (`Psx_main`, `0x4C3D63`).
pub(super) const RE2_PAUSE_KEY: u16 = 0x0002;
/// The word the pause shows (`0x52D908`).
const RE2_PAUSE_WORD: &str = "Pause";

impl GameSession {
    /// `Psx_main`'s gate for ARRANGE MODE's pause (`0x4C3CF6`), on the state
    /// the last tick left: `censorship_off`, a game in play (`fg_system`'s
    /// `0x40000000`, which the title's steps before play don't have), `Game`
    /// 0 (no START, map or option key taken, no status screen, get-item
    /// window, computer, save screen or option screen), no cut load pending
    /// (`0x98F07B`), no `fg_stop & 0x1000000` (a script's or a
    /// message's hold, a door's first tick, `Die`), no door, film, demo or
    /// results (`fg_system & 0x2202200`), and neither the scene bars nor a
    /// cut the last tick's `Swap_Cbuff` loaded (`fg_status & 0x100010`).
    pub(super) fn re2_pause_gate(&self) -> bool {
        use crate::scd2::host::{FG_SYSTEM_MOVIE, FG_SYSTEM_RESULT};
        let game = &self.game;
        game.is_re2_room
            && game.re2.censorship_off
            && self.re2_title_start.is_none_or(|left| left == 0)
            && self.menu_request.is_none()
            && self.menu.is_none()
            && self.re2_status_open.is_none()
            && self.re2_save.is_none()
            && self.re2_config.is_none()
            && !game.re2_config_request
            && game.re2_item_get.is_none()
            && game.re2_computer.is_none()
            && game.re2_computer_request.is_none()
            && !game.re2_status_request
            && !game.re2.bg_load
            && !game.re2.input_stopped()
            && self.transition.is_none()
            && self.movie.is_none()
            && !game.re2.flag(0, FG_SYSTEM_MOVIE)
            && !self.in_demo()
            && !game.re2.flag(0, FG_SYSTEM_RESULT)
            && !game.scene_bars()
            && !self.re2_cut_loaded
            // The bits themselves, where a script writes them: `fg_system`'s
            // door, results, demo and film, `fg_status`'s bars and load.
            && game.re2.banks[0][0] & 0x0220_2200 == 0
            && game.re2.banks[1][0] & 0x0010_0010 == 0
    }

    /// The pause's test after `Pad_set` (see the module): a press under
    /// the gate pauses, and the paused ticks hold the frame, keep `Key` and
    /// count the clock until the next press. Returns whether the tick is a
    /// paused one, which runs nothing more.
    pub(super) fn step_re2_pause(&mut self, input: &player::Input) -> bool {
        // A host overlay (the in-game menu, the F1 overlay, F9's prompt) is
        // arklay's, over the game: the pause neither starts nor ends under
        // it, and a paused game stays paused.
        if self.system_menu_open || self.debug_menu.is_some() || self.return_title.is_some() {
            return self.re2_pause.is_some();
        }
        if self.re2_pause.is_none() {
            self.re2_pause_shown = false;
        }
        if !self.re2_pause_gate() {
            return false;
        }
        let pressed = self.game.pad_repeat.trigger & RE2_PAUSE_KEY != 0;
        let Some(seconds) = self.re2_pause else {
            if pressed {
                self.re2_pause = Some(self.game.re2_game_seconds());
                self.re2_pause_shown = true;
                self.re2_pause_drawn = false;
            }
            return false;
        };
        self.present_skip.movie_set(1);
        if pressed {
            self.re2_pause = None;
            self.game.set_re2_game_seconds(seconds);
        }
        self.game.re2.set_key(crate::player2::key_word(input));
        // No sound call runs.
        self.re2_sounds.played.clear();
        // The frame held is the press's, overlay and all: until it has been
        // drawn (two ticks in one pass of the loop) the paused tick draws it.
        let presented = self.present_skip.present();
        self.present_held = self.re2_pause_drawn && !presented;
        self.game.advance_play_timer();
        true
    }

    /// The pause's press tick's overlay (`Psx_main`, `0x4C3D75`): `Curtain2`,
    /// a black 320x240 tile in semi-transparency mode 0 (half the screen
    /// and half black, otag 5), and over it the word `Pause` at (0x87,
    /// 0x6B) in the message font's 14px cells at their proportional advances
    /// (`Prim14`'s `0x4000`, colour row 0), as the recordings show it.
    /// `System_trans`'s fades (otag 1) and curtains (otag 0) come after.
    pub(super) fn draw_re2_pause(&mut self) {
        if !self.re2_pause_shown {
            return;
        }
        self.re2_pause_drawn = true;
        self.framebuffer.with_screen(|screen| {
            screen.blend_rect([0, 0, 320, 240], [0, 0, 0], 0x80);
        });
        if let Some(font) = &self.font {
            let mut word = Vec::new();
            crate::charset::encode(RE2_PAUSE_WORD, font.script(), &mut word);
            self.framebuffer.with_screen(|screen| {
                font.draw_text(screen, 0x87, 0x6B, font::Tint::White, 2, &word, true);
            });
        }
    }
}
