//! RE 2's results after a cleared game: the task `Result` (`0x4B57C0`), which
//! `Game_loop` chains in place of the game on `fg_system`'s `0x200000` (room
//! 11C's `set(0, 10, 1)` at the end of the game).
//!
//! Each tick runs what task 0 runs in one: `Result`'s step (`Result_init` on
//! the first), the routine `Result_mvtbl` names (`Result_00`'s fade in,
//! `Result_01`'s steps, `Result_02`'s fade out), `Result_draw_quads`, which
//! steps the pieces' colours as it draws them, and then `System_trans`'s fade.
//! The pieces are the executable's table `text_result_0` (`0x52D178`): which
//! show, fading in or out, and their colours; their words, places and the
//! glyphs they draw are the screen's (`ui/result.lua`), which draws what
//! [`Result::view`] says.
//!
//! An extra mode's game (`fg_status`'s `0x2000000`) takes `Result_01`'s step
//! 0x11 and is ported whole: the clear time (script variable 35), its rank,
//! the mode's pieces and picture, the two musics, the skip and `Result_exit`'s
//! hand-over to the title. The main game's steps (the ending films, the
//! records and unlocks, the save for the next scenario) aren't: the session
//! doesn't start the results for it (oracle/re2-deviations.md).

/// The pieces in `text_result_0`.
pub const PIECES: usize = 33;
/// A piece's flags: shown, fading in, fading out.
const SHOWN: u8 = 1;
const FADE_IN: u8 = 4;
const FADE_OUT: u8 = 8;
/// A piece's colour at full (`0x808080`), and the highest a fade leaves it.
const FULL: u32 = 0x80_8080;
const FULL_LIMIT: i32 = 0x80_807F;
/// The pieces' colours as the executable's data holds them: what the first
/// results after the boot start from.
const COLOURS: [u32; PIECES] = {
    let mut colours = [0; PIECES];
    let full = [2, 3, 21, 22, 23, 24, 25, 26, 29, 30];
    let mut index = 0;
    while index < full.len() {
        colours[full[index]] = FULL;
        index += 1;
    }
    colours
};
/// The pieces' fade step `Result_01` sets with each reveal (`0x995B84`).
const PIECE_STEP: u32 = 0x10_1010;

/// `Result_01`'s steps of an extra's results.
const STEP_EXTRA_WAIT: u32 = 0x11;
const STEP_EXTRA_PIECES: u32 = 0x12;
const STEP_EXTRA_RESULT: u32 = 0x13;

/// `Result_mvtbl`: the routine a tick runs (`0x995B80`).
const MV_FADE_IN: u8 = 0;
const MV_STEPS: u8 = 1;
const MV_FADE_OUT: u8 = 2;
const MV_EXIT: u8 = 3;

/// `0x995B48`'s bits.
/// `Result_00`'s fade in has started.
const FLAG_FADING_IN: u32 = 1;
/// `Result_02`'s fade out has started.
const FLAG_FADING_OUT: u32 = 2;
/// After the fade out, `Result_exit`.
const FLAG_THEN_EXIT: u32 = 4;
/// After the fade out, `Result_01` again.
const FLAG_THEN_STEPS: u32 = 8;
/// The fade in shows the picture (`Bg_set_mode(0, 0)`).
const FLAG_PICTURE: u32 = 0x800;
/// The fade in shows the pieces `0x995C34` names.
const FLAG_SHOW_PIECES: u32 = 0x1000;
/// The fade out hides every piece.
const FLAG_HIDE_PIECES: u32 = 0x2000;

/// `Fade_set(0x200, ...)`'s blend: the screen to black.
const FADE_BLACK: u8 = 2;
/// `Result_00`'s and `Result_02`'s steps.
const FADE_STEP: i16 = 0x100;

/// `Snd_bgm_fade_ON(0xB4, 0x18)`: the music fades out over 180 ticks.
pub const MUSIC_FADE: u8 = 0xB4;
/// The mains `Result_01` plays: the results' and the picture's.
const MAIN_RESULT: u8 = 0x30;
const MAIN_PICTURE: u8 = 0x2A;
/// `Snd_bgm_ctr(0x1000000)`: play the main.
const PLAY_MAIN: u32 = 0x0100_0000;

/// The raw pad's buttons that skip (`0x9885FE & 0x9FF`): any but the
/// directions.
const SKIP_KEYS: u16 = 0x09FF;

/// `Result_load_bg`'s pictures of the extras: the 4th Survivor's, To-fu's.
pub const PICTURE_4TH: u8 = 5;
pub const PICTURE_TOFU: u8 = 6;

/// The glyph of a digit `n` (`0x17 + n`), and the others `Result_retrieve`
/// writes: the time's colons and the saves of an extra (blank).
const GLYPH_ZERO: u8 = 0x17;
const GLYPH_NINE: u8 = 0x20;
const GLYPH_FIVE: u8 = 0x1C;
const GLYPH_COLON: u8 = 0x24;
const GLYPH_BLANK: u8 = 0x26;

/// What the results ask of the music on a tick, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Music {
    /// `Snd_bgm_fade_ON(time, 0x18)`.
    FadeOut(u8),
    /// `Snd_bgm_main` of this main (`pBgm_table`'s word `0xFF00 | id`, with
    /// `Seq_ctr[1]` 0).
    Main(u8),
    /// `Snd_bgm_ctr(word)`.
    Control(u32),
}

/// The music the results drive: their calls as they make them, and
/// `Fade_rtn` as a step reads it (after the calls before it).
pub trait MusicHost {
    fn call(&mut self, music: Music);
    /// `Fade_rtn` is set: `Snd_bgm_fade_ON`'s fade hasn't freed the music.
    fn fading(&self) -> bool;
}

/// What a tick of the results did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Output {
    /// `Result_exit`: `Init_global` and the title's task, from the next tick.
    pub exit: bool,
}

/// A fade slot (`Fade_set`): its level, step and blend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fade {
    pub level: i16,
    pub add: i16,
    pub mode: u8,
}

impl Fade {
    /// `Fade_set(mode << 8, add)`.
    fn set(&mut self, mode: u8, add: i16) {
        self.mode = mode;
        self.add = add;
        self.level = if add < 0 {
            add.wrapping_add(i16::MIN)
        } else {
            0
        };
    }

    /// `Fade_status`: done once the level is negative.
    fn done(&self) -> bool {
        self.level < 0
    }
}

/// What `Result_retrieve` (`0x4B6E10`) makes of the clear time and the saves:
/// the ten glyphs of `HH:MM:SS` and the saves (`Reslt`, `0x995B30`), the
/// hours, minutes, seconds and saves themselves (`0x99CE22`), the rank
/// (`0x99CE20`, 0 for S to 4 for D) and the main game's scores (`0x995B45`
/// the time's, `0x995B44` the saves', `0x99CE18` the third).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Retrieve {
    pub digits: [u8; 10],
    pub time: [u8; 4],
    pub rank: u16,
    pub scores: [u8; 3],
}

fn digit(n: u32) -> u8 {
    GLYPH_ZERO + (n % 10) as u8
}

/// The digits and the time `Result_retrieve` writes for `seconds` played and
/// `saves` made, before the extras' blanks.
fn clear_time(seconds: u32, saves: u32) -> ([u8; 10], [u8; 4]) {
    let mut digits = [0; 10];
    let hours = (seconds / 3600) as u8;
    let mut time = [hours, 0, 0, saves as u8];
    if seconds < 360_000 {
        let pair = match hours {
            0 => [GLYPH_ZERO; 2],
            1..99 => [digit(u32::from(hours) / 10), digit(u32::from(hours))],
            _ => [GLYPH_NINE; 2],
        };
        digits[..2].copy_from_slice(&pair);
        let minutes = seconds % 3600 / 60;
        time[1] = minutes as u8;
        digits[3] = digit(minutes / 10);
        digits[4] = digit(minutes);
        let rest = seconds % 60;
        time[2] = rest as u8;
        digits[6] = digit(rest / 10);
        digits[7] = digit(rest);
    } else {
        // The original leaves `Reslt`'s minutes and seconds as the last
        // results did; they start at 0 here, and only the main game's
        // records read them.
        digits[..2].copy_from_slice(&[GLYPH_NINE; 2]);
        digits[3..5].copy_from_slice(&[GLYPH_FIVE, GLYPH_NINE]);
        digits[6..8].copy_from_slice(&[GLYPH_FIVE, GLYPH_NINE]);
    }
    digits[2] = GLYPH_COLON;
    digits[5] = GLYPH_COLON;
    if saves as u8 >= 99 {
        digits[8..].copy_from_slice(&[GLYPH_NINE; 2]);
    } else {
        digits[8] = digit((saves & 0xFF) / 10);
        digits[9] = digit(saves & 0xFF);
    }
    (digits, time)
}

/// `Result_retrieve` for an extra's results: the clear time is script
/// variable 35 (`0x98EB2A`, read signed), the saves show blank, and the rank
/// is S at 240 seconds or less, then one lower each 60 seconds more, D past
/// 420. It returns before the scores, the records and the unlocks.
pub fn retrieve_extra(seconds: i16, saves: u32) -> Retrieve {
    let seconds = i32::from(seconds) as u32;
    let (mut digits, time) = clear_time(seconds, saves);
    digits[8] = GLYPH_BLANK;
    digits[9] = GLYPH_BLANK;
    let mut rank = 0;
    if seconds > 240 {
        let mut limit = 240;
        rank = 4;
        for next in 1..4 {
            limit += 60;
            if seconds <= limit {
                rank = next;
                break;
            }
        }
    }
    Retrieve {
        digits,
        time,
        rank,
        scores: [0; 3],
    }
}

/// `Result_retrieve`'s scores and rank for the main game: 100 for the time
/// (`seconds`) at 2:30 or less, 10 less for each hour begun past it; 100 for
/// the saves (`saves`, `0x99C8A0`'s low word) less 5 a save, none from 20;
/// 100 for the third (`count`, `0x99C89E`) less 15 each, none from 6; 50 off
/// the sum with `fg_status`'s `0x800000`. The rank is S for 269 or more, A
/// for 199, B for 144, C for 99 and D below.
pub fn main_scores(seconds: u32, saves: u16, count: u16, penalty: bool) -> ([u8; 3], u16) {
    let mut steps = 0u8;
    if seconds > 9000 {
        let mut limit = 9000;
        loop {
            limit += 3600;
            steps += 1;
            if steps > 9 {
                steps = 10;
                break;
            }
            if limit >= seconds {
                break;
            }
        }
    }
    let time = 100 - 10 * steps;
    let save = if saves < 20 {
        100u8.wrapping_sub((saves as u8).wrapping_mul(5))
    } else {
        0
    };
    let third = if count < 6 { 100 - 15 * count as u8 } else { 0 };
    let mut sum = u16::from(time) + u16::from(save) + u16::from(third);
    if penalty {
        sum = sum.saturating_sub(50);
    }
    const LIMITS: [u16; 5] = [0x10D, 199, 0x90, 99, 0];
    let mut rank = 0;
    if sum < LIMITS[0] {
        loop {
            rank += 1;
            if sum >= LIMITS[rank] {
                break;
            }
        }
    }
    ([time, save, third], rank as u16)
}

/// The results' screen as a tick leaves it, for the screen to draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct View {
    /// The pieces' flags (bit 0 shown) and colours (`0x808080` full).
    pub flags: [u8; PIECES],
    pub colours: [u32; PIECES],
    /// Whether the tick drew the pieces (`Result_draw_quads`).
    pub drawn: bool,
    /// The picture `Result_load_bg` last loaded, and whether the background
    /// shows it (`Bg_set_mode(0, 0)`) or is black.
    pub picture: Option<u8>,
    pub background: bool,
    pub digits: [u8; 10],
    /// Fade slot 0 as `System_trans` drew it, when it did.
    pub fade: Option<(i16, u8)>,
}

/// The results' task, from the tick `Game_loop` chains it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Result {
    /// `Result_init` has run.
    started: bool,
    /// The task's step (`pCtcb + 8`: 0 to init, 2 running, 3 a tick of only
    /// the draw) and sub-step (`pCtcb + 9`).
    task_step: u8,
    task_sub: u8,
    mv: u8,
    step: u32,
    count: u32,
    flags: u32,
    piece_step: u32,
    skip: u8,
    pieces_shown: u32,
    retrieve: Retrieve,
    piece_flags: [u8; PIECES],
    colours: [u32; PIECES],
    fade: Fade,
    drawn_fade: Option<(i16, u8)>,
    drawn: bool,
    background: bool,
    picture: Option<u8>,
    /// The raw pad `Pad_set` saw last, for the tick's new presses.
    last_raw: u16,
    /// `fg_status`'s `0x2000000` and `fg_system`'s 1, as the game left them.
    extra: bool,
    tofu: bool,
    seconds: i16,
    saves: u32,
}

impl Result {
    /// The task `Game_loop` chains: an extra's when `extra`, To-fu's with
    /// `tofu`; `seconds` is script variable 35, the extra's clear time, and
    /// `saves` the save count `Result_retrieve` reads. `colours` are the
    /// pieces' as the last results left them (the executable's after the
    /// boot, [`Result::boot_colours`]), and `fade` fade slot 0 as the game
    /// left it. Until its first tick the work is as the boot left it (0) and
    /// the task's step is `Game_loop`'s (2).
    pub fn new(
        extra: bool,
        tofu: bool,
        seconds: i16,
        saves: u32,
        colours: [u32; PIECES],
        fade: Fade,
    ) -> Self {
        Self {
            started: false,
            task_step: 2,
            task_sub: 0,
            mv: MV_FADE_IN,
            step: 0,
            count: 0,
            flags: 0,
            piece_step: 0,
            skip: 0,
            pieces_shown: 0,
            retrieve: Retrieve {
                digits: [0; 10],
                time: [0; 4],
                rank: 0,
                scores: [0; 3],
            },
            piece_flags: [0; PIECES],
            colours,
            fade,
            drawn_fade: None,
            drawn: false,
            background: false,
            picture: None,
            last_raw: 0,
            extra,
            tofu,
            seconds,
            saves,
        }
    }

    /// The task's step and sub-step (`pCtcb + 8`, `+ 9`), which the title's
    /// task finds on the tick `Result_exit` chains it.
    pub fn task(&self) -> (u8, u8) {
        (self.task_step, self.task_sub)
    }

    /// Fade slot 0 as the results leave it: what the title after them takes.
    pub fn fade(&self) -> Fade {
        self.fade
    }

    /// The pieces' colours the executable's data holds.
    pub fn boot_colours() -> [u32; PIECES] {
        COLOURS
    }

    pub fn colours(&self) -> [u32; PIECES] {
        self.colours
    }

    /// `Result_init` (`0x4B5840`): every piece hidden, the background black,
    /// the pad's repeat set (the session's), the first step (0x11 for an
    /// extra), the counts and flags cleared, `Result_retrieve` and the four
    /// fades off. The music's fade comes first.
    fn init(&mut self, music: &mut dyn MusicHost) {
        music.call(Music::FadeOut(MUSIC_FADE));
        self.piece_flags = [0; PIECES];
        self.background = false;
        self.mv = MV_STEPS;
        self.step = if self.extra { STEP_EXTRA_WAIT } else { 0 };
        self.count = 0;
        self.flags = FLAG_HIDE_PIECES;
        self.pieces_shown = 0;
        // The main game's scores aren't ported (see the module's notes).
        self.retrieve = retrieve_extra(self.seconds, self.saves);
        // `Fade_adjust(n, -1, ...)`: each slot off, its step and blend kept.
        self.fade.level = -1;
    }

    /// `FUN_004B6DD0(mask, flags, clear)`: the pieces of `mask`'s bits (0 to
    /// 30) take `flags`; with `clear` the others are hidden.
    fn set_pieces(&mut self, mask: u32, flags: u8, clear: bool) {
        for (index, piece) in self.piece_flags.iter_mut().enumerate().take(31) {
            if mask >> index & 1 != 0 {
                *piece = flags;
            } else if clear {
                *piece = 0;
            }
        }
    }

    /// `Result_00` (`0x4B5C80`): the fade in from black, which shows the
    /// picture under `FLAG_PICTURE` and the pieces `FLAG_SHOW_PIECES`
    /// names, then `Result_01` again.
    fn fade_in(&mut self) {
        if self.flags & FLAG_FADING_IN == 0 {
            self.fade.set(FADE_BLACK, -FADE_STEP);
            self.mv = MV_FADE_IN;
            self.flags |= FLAG_FADING_IN;
            if self.flags & FLAG_PICTURE == 0 {
                self.background = false;
            } else {
                self.background = true;
                self.flags ^= FLAG_PICTURE;
            }
            if self.flags & FLAG_SHOW_PIECES != 0 {
                self.set_pieces(self.pieces_shown, SHOWN, false);
                self.flags ^= FLAG_SHOW_PIECES;
                self.pieces_shown = 0;
            }
        } else if self.fade.done() {
            self.mv = MV_STEPS;
            self.flags ^= FLAG_FADING_IN;
        }
    }

    /// `Result_02` (`0x4B5D40`): the fade out to black, then `Result_exit`,
    /// `Result_01` or the fade in, with every piece hidden under
    /// `FLAG_HIDE_PIECES`.
    fn fade_out(&mut self) {
        if self.flags & FLAG_FADING_OUT == 0 {
            self.fade.set(FADE_BLACK, FADE_STEP);
            self.flags |= FLAG_FADING_OUT;
            self.mv = MV_FADE_OUT;
            return;
        }
        if !self.fade.done() {
            return;
        }
        let flags = self.flags ^ FLAG_FADING_OUT;
        if flags & FLAG_THEN_EXIT != 0 {
            self.mv = MV_EXIT;
            self.flags ^= FLAG_FADING_OUT | FLAG_THEN_EXIT;
        } else if flags & FLAG_THEN_STEPS != 0 {
            self.mv = MV_STEPS;
            self.flags ^= FLAG_FADING_OUT | FLAG_THEN_STEPS;
        } else {
            self.mv = MV_FADE_IN;
            self.flags = flags;
        }
        if self.flags & FLAG_HIDE_PIECES != 0 {
            self.flags ^= FLAG_HIDE_PIECES;
            self.set_pieces(u32::MAX, 0, true);
            self.piece_flags[31] = 0;
            self.piece_flags[32] = 0;
        }
        self.background = false;
    }

    /// `Result_02` from `Result_01`: hide every piece as the fade out ends,
    /// then `Result_01` again.
    fn fade_out_then_steps(&mut self) {
        self.flags |= FLAG_HIDE_PIECES | FLAG_THEN_STEPS;
        self.fade_out();
    }

    /// `Result_load_bg(index)`, `Fade_adjust2(0, 0, 0)` and the fade in
    /// that shows it.
    fn show_picture(&mut self, index: u8) {
        self.picture = Some(index);
        self.fade.level = 0;
        self.flags |= FLAG_PICTURE;
        self.fade_in();
    }

    /// `Result_01` (`0x4B5E00`), an extra's steps: 0x11 waits for the
    /// music's fade, 0x12 shows the mode's pieces on their counts and its
    /// picture on 0x4B0, 0x13 the rank, the time and the mode's name on
    /// 0x1E0 (or a key's), and goes on 0x514 to `Result_exit`. Returns
    /// whether the task chained the title.
    fn steps(&mut self, trig: u16, music: &mut dyn MusicHost, out: &mut Output) -> bool {
        match self.task_sub {
            4 | 5 => return self.exit(out),
            _ => {}
        }
        match self.step {
            STEP_EXTRA_WAIT => {
                if music.fading() {
                    self.count += 1;
                    return false;
                }
                music.call(Music::Main(MAIN_RESULT));
                music.call(Music::Control(PLAY_MAIN));
                self.fade_in();
                self.piece_step = PIECE_STEP;
                self.step = STEP_EXTRA_PIECES;
                self.count = 1;
                return false;
            }
            STEP_EXTRA_PIECES => match self.count {
                0xB4 => self.set_pieces(1, SHOWN | FADE_IN, false),
                200 => self.set_pieces(4, SHOWN, false),
                500 => {
                    self.set_pieces(4, 0, false);
                    self.fade_out_then_steps();
                }
                0x280 => self.set_pieces(2, SHOWN | FADE_IN, false),
                0x294 => self.set_pieces(8, SHOWN, false),
                0x424 => {
                    self.set_pieces(8, 0, false);
                    self.fade_out_then_steps();
                    music.call(Music::FadeOut(MUSIC_FADE));
                }
                0x4B0 => {
                    self.show_picture(if self.tofu { PICTURE_TOFU } else { PICTURE_4TH });
                    music.call(Music::Main(MAIN_PICTURE));
                    music.call(Music::Control(PLAY_MAIN));
                    self.skip = 0;
                    self.step = STEP_EXTRA_RESULT;
                    self.count = 1;
                    return false;
                }
                _ => {}
            },
            STEP_EXTRA_RESULT => {
                if trig & SKIP_KEYS != 0 {
                    if self.count < 300 {
                        if self.skip == 0 {
                            self.count = 0x1E0;
                            self.reveal();
                            return false;
                        }
                    } else if self.skip == 0 {
                        self.count = 0x1E0;
                        self.reveal();
                        return false;
                    } else if self.skip == 1 {
                        self.count = 0x4B2;
                        return false;
                    }
                }
                if self.count == 0x1E0 {
                    if self.skip == 0 {
                        self.reveal();
                        return false;
                    }
                } else {
                    if self.count == 0x4B0 {
                        music.call(Music::FadeOut(MUSIC_FADE));
                        self.fade_out_then_steps();
                    }
                    if self.count == 0x514 {
                        self.task_sub = 5;
                        return self.exit(out);
                    }
                }
            }
            _ => {}
        }
        self.count += 1;
        false
    }

    /// Step 0x13's reveal: the rank's letter, CONGRATULATIONS, RANKING,
    /// CLEAR GAME and TOTAL TIME with the time, and the mode's name.
    fn reveal(&mut self) {
        let rank = 4 + usize::from(self.retrieve.rank);
        if let Some(piece) = self.piece_flags.get_mut(rank) {
            *piece = SHOWN | FADE_IN;
        }
        for index in [9, 10, 11, 12] {
            self.piece_flags[index] = 0;
        }
        self.set_pieces(0x1E000, SHOWN | FADE_IN, false);
        if self.tofu {
            self.piece_flags[31] = 0;
            self.piece_flags[32] = SHOWN | FADE_IN;
        } else {
            self.piece_flags[31] = SHOWN | FADE_IN;
            self.piece_flags[32] = 0;
        }
        self.skip = 1;
        self.piece_step = PIECE_STEP;
        self.count = 1;
    }

    /// `Result_exit` (`0x4B5C30`): `Bg_set_mode(2, 0)` and `Task_sleep(1)`
    /// sixty times over, which on the PC doesn't yield, then `Init_global`
    /// and the title's task, which runs from the next tick.
    fn exit(&mut self, out: &mut Output) -> bool {
        self.background = false;
        out.exit = true;
        true
    }

    /// One tick of task 0: `raw` is the pad as `Pad_set` read it.
    pub fn tick(&mut self, raw: u16, music: &mut dyn MusicHost) -> Output {
        let mut out = Output::default();
        let trig = raw & !self.last_raw;
        self.last_raw = raw;
        let mut chained = false;
        match self.task_step {
            3 => {}
            _ => {
                if !std::mem::replace(&mut self.started, true) {
                    self.init(music);
                }
                chained = match self.mv {
                    MV_FADE_IN => {
                        self.fade_in();
                        false
                    }
                    MV_STEPS => self.steps(trig, music, &mut out),
                    MV_FADE_OUT => {
                        self.fade_out();
                        false
                    }
                    _ => self.exit(&mut out),
                };
            }
        }
        self.drawn = !chained;
        if chained {
            self.task_step = 3;
        } else {
            self.task_step = 2;
            self.draw_quads();
        }
        // `System_trans`.
        self.drawn_fade = None;
        if self.fade.level >= 0 {
            self.drawn_fade = Some((self.fade.level, self.fade.mode));
            self.fade.level = self.fade.level.wrapping_add(self.fade.add);
        }
        out
    }

    /// `Result_draw_quads`'s steps of the pieces' colours: a piece fading
    /// out darkens by the step, one fading in brightens to full. Both test
    /// the colour signed (`jl`), so a piece fading out is hidden only on
    /// reaching full, never on wrapping below 0.
    fn draw_quads(&mut self) {
        for (flags, colour) in self.piece_flags.iter_mut().zip(self.colours.iter_mut()) {
            if *flags & SHOWN == 0 {
                continue;
            }
            if *flags & FADE_OUT != 0 {
                *colour = colour.wrapping_sub(self.piece_step);
                if *colour as i32 > FULL_LIMIT {
                    *flags = 0;
                    *colour = 0;
                }
            }
            if *flags & FADE_IN != 0 {
                *colour = colour.wrapping_add(self.piece_step);
                if *colour as i32 > FULL_LIMIT {
                    *colour = FULL;
                    *flags &= !FADE_IN;
                }
            }
        }
    }

    pub fn view(&self) -> View {
        View {
            flags: self.piece_flags,
            colours: self.colours,
            drawn: self.drawn,
            picture: self.picture,
            background: self.background,
            digits: self.retrieve.digits,
            fade: self.drawn_fade,
        }
    }

    /// The trace's `res`, `rest`, `rfade` and `rmus` fields, as
    /// `scripts/oracle-re2.py` writes them; `music` is `Fade_rtn` and
    /// `Fade_time`.
    pub fn trace_fields(&self, music: (u8, u8)) -> String {
        let shown: Vec<String> = (0..PIECES)
            .filter(|&index| self.piece_flags[index] != 0)
            .map(|index| {
                format!(
                    "{index}:{:x}:{:x}",
                    self.piece_flags[index], self.colours[index]
                )
            })
            .collect();
        let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
        format!(
            "res={},{},{},{:x},{},{:x},{:x},{},{},{},{},{},{},{} rest={} rfade={},{},{} rmus={},{}",
            self.task_step,
            self.task_sub,
            self.mv,
            self.step,
            self.count,
            self.flags,
            self.piece_step,
            self.skip,
            self.retrieve.rank,
            self.retrieve.scores[0],
            self.retrieve.scores[1],
            self.retrieve.scores[2],
            hex(&self.retrieve.digits),
            hex(&self.retrieve.time),
            if shown.is_empty() {
                "-".to_string()
            } else {
                shown.join("/")
            },
            self.fade.level,
            self.fade.add,
            self.fade.mode,
            music.0,
            music.1,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_extras_time_shows_as_its_digits_with_blank_saves() {
        let retrieve = retrieve_extra(330, 0);
        // 00:05:30, then two blanks.
        assert_eq!(
            retrieve.digits,
            [0x17, 0x17, 0x24, 0x17, 0x1C, 0x24, 0x1A, 0x17, 0x26, 0x26]
        );
        assert_eq!(retrieve.time, [0, 5, 30, 0]);
        let long = retrieve_extra(i16::MAX, 3);
        // 9:06:07.
        assert_eq!(
            &long.digits[..8],
            &[0x17, 0x20, 0x24, 0x17, 0x1D, 0x24, 0x17, 0x1E]
        );
        assert_eq!(long.time, [9, 6, 7, 3]);
    }

    #[test]
    fn an_extras_rank_steps_down_each_minute_past_four() {
        let rank = |seconds| retrieve_extra(seconds, 0).rank;
        assert_eq!(rank(0), 0);
        assert_eq!(rank(240), 0);
        assert_eq!(rank(241), 1);
        assert_eq!(rank(300), 1);
        assert_eq!(rank(301), 2);
        assert_eq!(rank(330), 2);
        assert_eq!(rank(360), 2);
        assert_eq!(rank(420), 3);
        assert_eq!(rank(421), 4);
        assert_eq!(rank(9999), 4);
    }

    #[test]
    fn a_negative_clear_time_reads_as_past_a_hundred_hours() {
        // `(dword)(short)` wraps a negative variable to a huge count.
        let retrieve = retrieve_extra(-1, 0);
        assert_eq!(
            &retrieve.digits[..8],
            &[0x20, 0x20, 0x24, 0x1C, 0x20, 0x24, 0x1C, 0x20]
        );
        assert_eq!(retrieve.rank, 4);
    }

    #[test]
    fn the_main_games_scores_give_its_rank() {
        // 2:30 or less, no saves, nothing counted: S.
        assert_eq!(main_scores(9000, 0, 0, false), ([100, 100, 100], 0));
        // An hour begun past 2:30 costs 10.
        assert_eq!(main_scores(9001, 0, 0, false).0[0], 90);
        assert_eq!(main_scores(12600, 0, 0, false).0[0], 90);
        assert_eq!(main_scores(12601, 0, 0, false).0[0], 80);
        assert_eq!(main_scores(99 * 3600, 0, 0, false).0[0], 0);
        // 5 a save, none from 20; 15 a count, none from 6.
        assert_eq!(main_scores(0, 4, 2, false).0, [100, 80, 70]);
        assert_eq!(main_scores(0, 20, 6, false).0, [100, 0, 0]);
        // 250: A; with the penalty, 200: A; 150: B.
        assert_eq!(main_scores(9001, 12, 0, false), ([90, 40, 100], 1));
        assert_eq!(main_scores(0, 10, 0, true).1, 1);
        assert_eq!(main_scores(0, 20, 0, false).1, 1);
        assert_eq!(main_scores(0, 20, 0, true).1, 2);
        assert_eq!(main_scores(0, 20, 6, false).1, 3);
        assert_eq!(main_scores(99 * 3600, 20, 6, false).1, 4);
    }

    /// A music whose fade frees it `fade_ticks` ticks after it starts.
    struct FakeMusic {
        left: u32,
        fade_ticks: u32,
        calls: Vec<Music>,
    }

    impl MusicHost for FakeMusic {
        fn call(&mut self, music: Music) {
            if let Music::FadeOut(_) = music {
                self.left = self.fade_ticks;
            }
            self.calls.push(music);
        }
        fn fading(&self) -> bool {
            self.left > 0
        }
    }

    /// Run a 4th Survivor's results with no key and no music fade from
    /// `fade_ticks` ticks of it, and give each tick's steps.
    fn run(tofu: bool, fade_ticks: u32, keys: &[(u32, u16)]) -> Vec<(u8, u32, u32, bool)> {
        let fade = Fade {
            level: 14336,
            add: -6144,
            mode: 2,
        };
        let mut result = Result::new(true, tofu, 330, 0, Result::boot_colours(), fade);
        let mut music = FakeMusic {
            left: 0,
            fade_ticks,
            calls: Vec::new(),
        };
        let mut ticks = Vec::new();
        for tick in 0..4000 {
            let raw = keys
                .iter()
                .find(|(at, _)| *at == tick)
                .map_or(0, |&(_, raw)| raw);
            let out = result.tick(raw, &mut music);
            music.left = music.left.saturating_sub(1);
            ticks.push((result.mv, result.step, result.count, out.exit));
            if out.exit {
                break;
            }
        }
        ticks
    }

    #[test]
    fn an_extras_results_wait_for_the_music_then_reveal_on_their_counts() {
        let ticks = run(false, 180, &[]);
        // The music's fade: step 0x11 counts.
        assert_eq!(ticks[0], (MV_STEPS, STEP_EXTRA_WAIT, 1, false));
        assert_eq!(ticks[179].1, STEP_EXTRA_WAIT);
        // Its end starts the fade in.
        assert_eq!(ticks[180], (MV_FADE_IN, STEP_EXTRA_PIECES, 1, false));
        // 128 ticks of fade, then the counts.
        let steps = ticks
            .iter()
            .position(|t| t.0 == MV_STEPS && t.1 == STEP_EXTRA_PIECES);
        assert_eq!(steps, Some(180 + 128));
        let last = ticks.last().unwrap();
        assert!(last.3, "the results end in `Result_exit`");
        // Step 0x13 reaches 0x514 with its own fade out on the way.
        let result_start = ticks.iter().position(|t| t.1 == STEP_EXTRA_RESULT).unwrap();
        assert!(ticks.len() - result_start > 0x514);
    }

    #[test]
    fn a_key_in_the_last_step_reveals_then_ends_it_sooner() {
        let plain = run(false, 0, &[]);
        let start = plain.iter().position(|t| t.1 == STEP_EXTRA_RESULT).unwrap() as u32;
        // The picture fades in for 128 ticks, when no key is read; then one
        // reveals, and one 300 counts after the reveal ends the step.
        let keyed = run(false, 0, &[(start + 200, 0x80), (start + 520, 0x80)]);
        assert!(keyed.len() < plain.len());
        let reveal = &keyed[start as usize + 200];
        assert_eq!(reveal.2, 1, "the reveal starts the count again");
        let jump = &keyed[start as usize + 520];
        assert_eq!(jump.2, 0x4B2, "a key after the reveal goes on to the end");
    }

    #[test]
    fn a_piece_fades_in_in_eight_steps() {
        let fade = Fade {
            level: -1,
            add: 0,
            mode: 0,
        };
        let mut result = Result::new(true, false, 330, 0, [0; PIECES], fade);
        result.task_step = 2;
        result.piece_step = PIECE_STEP;
        result.set_pieces(1, SHOWN | FADE_IN, false);
        for step in 1..8 {
            result.draw_quads();
            assert_eq!(result.colours[0], PIECE_STEP * step);
        }
        result.draw_quads();
        assert_eq!((result.piece_flags[0], result.colours[0]), (SHOWN, FULL));
    }
}
