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
//! 0x11: the clear time (script variable 35), its rank, the mode's pieces and
//! picture, the two musics, the skip and `Result_exit`'s hand-over to the
//! title. The main game's take steps 0 to 7: the ending film
//! (`Init_movie_work(0x11)` after 300 ticks in a scenario A, `(0xE)` at once
//! in a B), the character's picture with the rank, the time and the saves,
//! the special weapons a clear has earned, and the save for the next
//! scenario (`Mem_card`, which [`Output::save`] asks the session for). A
//! scenario A's results then end; a B's go on with steps 8 to 0x17: the
//! credits' music, the picture of a clear with the records' bit 0x12, HARD's
//! `sp_e` and the end. `Result_retrieve` makes the scores and the rank, the
//! weapons and records [`Result::next_game`] hands to the next scenario, and
//! the gallery's bits ([`Result::gallery`]).

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

/// `Result_01`'s steps of the main game's results: 0 waits for the music's
/// fade, 1 counts to a scenario A's film, 3 waits for the film, 4 for the
/// music again before the picture, 5 shows the rank and the time, 6 the
/// special weapons, 7 waits for the music before the save.
const STEP_WAIT: u32 = 0;
const STEP_FILM_A: u32 = 1;
const STEP_FILM: u32 = 3;
const STEP_PICTURE: u32 = 4;
const STEP_RANK: u32 = 5;
const STEP_WEAPONS: u32 = 6;
const STEP_SAVE: u32 = 7;
/// A scenario B's steps after the save: 8 (or 0x14 without `Special`'s 0x40)
/// goes to 0xE, which shows the picture of a clear with the records' 0x12
/// (0xF), else goes to 0x16, HARD's picture (0x17), and 0x10 ends.
const STEP_CREDITS: u32 = 8;
const STEP_CREDITS_NO_SPECIAL: u32 = 0x14;
const STEP_BONUS: u32 = 0xE;
const STEP_BONUS_SHOW: u32 = 0xF;
const STEP_END: u32 = 0x10;
const STEP_HARD: u32 = 0x16;
const STEP_HARD_SHOW: u32 = 0x17;
/// `Result_01`'s steps of an extra's results.
const STEP_EXTRA_WAIT: u32 = 0x11;
const STEP_EXTRA_PIECES: u32 = 0x12;
const STEP_EXTRA_RESULT: u32 = 0x13;

/// The ending films: a scenario A's and a B's (`Init_movie_work`'s ids).
pub const FILM_A: u8 = 0x11;
pub const FILM_B: u8 = 0xE;
/// `gmovie_tbl0` (`0x523248`): the gallery's films in its order, whose
/// index `Init_movie_work` opens in the gallery's fourth word.
const GALLERY_FILMS: [u8; 16] = [
    0, 3, 0xF, 0x10, 0xA, 5, 4, 6, 7, 0xB, 8, 9, 0xC, 0x11, 0xD, 0xE,
];

/// The special weapons a clear can earn (`FUN_004b7350`'s ids): the
/// submachine gun, the rocket launcher and the gatling gun.
pub const SUBMACHINE: u8 = 0x0F;
pub const ROCKET: u8 = 0x11;
pub const GATLING: u8 = 0x12;
/// The pieces of the weapons' picture (`text_result_0`'s 18 to 20).
const PIECE_SUBMACHINE: usize = 18;
const PIECE_ROCKET: usize = 19;
const PIECE_GATLING: usize = 20;

/// `fg_zapping`'s bits (`0x98ED24`, the records the next scenario carries):
/// a scenario A cleared without `fg_item`'s 0x9C, a B cleared at rank S,
/// a B cleared at S again past the second clear.
const RECORD_A: u32 = 0x12;
const RECORD_S: u32 = 0x13;
const RECORD_S_AGAIN: u32 = 0x14;

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
/// `Result_retrieve`'s weapons: the next scenario's box holds the
/// submachine gun, the rocket launcher, the gatling gun; 0x80 a new one shows
/// (step 6).
pub const FLAG_SUBMACHINE: u32 = 0x10;
pub const FLAG_ROCKET: u32 = 0x20;
pub const FLAG_GATLING: u32 = 0x40;
const FLAG_NEW_WEAPON: u32 = 0x80;
/// `Result_retrieve`'s records: a B's first S, a later one, and the bonus
/// picture of a B after an A with the records' 0x12.
const FLAG_FIRST_S: u32 = 0x100;
const FLAG_S_AGAIN: u32 = 0x200;
const FLAG_BONUS: u32 = 0x400;
/// `Special`'s 0x40 is down: a B's credits start at step 0x14.
const FLAG_NO_SPECIAL: u32 = 0x1_0000;
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

/// `Result_load_bg`'s pictures (`end1<n>`): Leon's, Claire's, the bonus
/// picture of a B, the 4th Survivor's, To-fu's.
pub const PICTURE_LEON: u8 = 0;
pub const PICTURE_CLAIRE: u8 = 1;
pub const PICTURE_BONUS: u8 = 3;
pub const PICTURE_4TH: u8 = 5;
pub const PICTURE_TOFU: u8 = 6;
/// HARD's picture, `sp_e.adt`, which step 0x16 loads itself.
pub const PICTURE_HARD: u8 = 9;

/// The name of picture `index` under `ui/result/`: `end1<n>`, or HARD's
/// `sp_e`.
pub fn picture_name(index: u8) -> String {
    if index == PICTURE_HARD {
        "sp_e".to_owned()
    } else {
        format!("end1{index}")
    }
}

/// The main mains and the sub `Result_01` plays: none (`0xFF`, which stops
/// the main), and a B's credits' sub.
const MAIN_NONE: u8 = 0xFF;
const SUB_CREDITS: u8 = 0x33;
/// `Snd_bgm_ctr(0x21000000)`: play the sub.
const PLAY_SUB: u32 = 0x2100_0000;

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
    /// `Snd_bgm_sub` of this sub (the word's second byte, with `Seq_ctr[1]`
    /// and `[2]` 0 and their volumes 100).
    Sub(u8),
    /// `Snd_bgm_ctr(word)`.
    Control(u32),
}

/// What the results ask of the session: the music's calls as they make
/// them, `Fade_rtn` as a step reads it (after the calls before it), and the
/// ending films.
pub trait MusicHost {
    fn call(&mut self, music: Music);
    /// `Fade_rtn` is set: `Snd_bgm_fade_ON`'s fade hasn't freed the music.
    fn fading(&self) -> bool;
    /// `Init_movie_work(id)`: the film starts, or with the films off is
    /// over within the call (`fg_system`'s `0x200` is down again).
    fn film(&mut self, id: u8);
    /// `fg_system`'s `0x200`: a film plays.
    fn film_playing(&self) -> bool;
}

/// What a tick of the results did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Output {
    /// `Result_exit`: `Init_global` and the title's task, from the next tick.
    pub exit: bool,
    /// Step 7: `FUN_004b73c0` has made the next scenario's game
    /// ([`Result::next_game`]) and `Task_execute(1, Mem_card)` the save
    /// screen. The results sleep under it ([`Result::sleep`]) until it ends.
    pub save: bool,
}

/// What `Result_retrieve` and the main game's steps read of the game they
/// end. The counts are the save work's as `Result_init` copies it
/// (`0x99C880`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Clear {
    /// `fg_status`'s `0x2000000`, an extra mode's game, and `fg_system`'s
    /// 1, To-fu's.
    pub extra: bool,
    pub tofu: bool,
    /// `fg_status`'s `0x80000000` (Claire) and `0x40000000` (scenario B).
    pub claire: bool,
    pub scenario_b: bool,
    /// `fg_status`'s `0x800000`, 50 off the score, and `0x4000000`, which
    /// leaves out the weapons and records.
    pub penalty: bool,
    pub no_records: bool,
    pub hard: bool,
    /// The clear time in seconds: `SetGameSeconds(1)` in the main game, an
    /// extra's script variable 35 read signed (`(dword)(short)`).
    pub seconds: u32,
    /// The saves (`0x99C8A0`, the work's `0x98E9BC`), and the third count
    /// (`0x99C89E`, the work's `0x98E9BA`).
    pub saves: u32,
    pub third: u16,
    /// `fg_item`'s bit 0x9C.
    pub item_9c: bool,
    /// `fg_zapping` (`0x98ED24`), the records.
    pub zapping: [u32; 2],
    /// The clears before (`0x99CD5E`, the work's `0x98EE7A`).
    pub clears: u8,
    /// config.ini's `SPECIAL` (`Special`).
    pub special: u8,
    /// `FUN_004b7350`: the inventory or the box holds an infinite
    /// submachine gun, an infinite rocket launcher, a gatling gun.
    pub held: [bool; 3],
}

/// `FUN_004b7350(id)`: whether `slots` (`Item_work`'s eleven, then the
/// box's 64, each `[id, count, ..]`) hold `id`, the submachine gun and the
/// rocket launcher only with a count of `0xFF` (infinite).
pub fn holds(slots: impl IntoIterator<Item = [u8; 2]>, id: u8) -> bool {
    slots
        .into_iter()
        .any(|[item, count]| item == id && (!matches!(id, SUBMACHINE | ROCKET) || count == 0xFF))
}

/// `Flg_ck` on a word array: bit `0x80000000 >> (n & 31)` of word `n >> 5`.
fn flag(words: &[u32], n: u32) -> bool {
    words[(n >> 5) as usize] & (0x8000_0000 >> (n & 31)) != 0
}

fn flag_on(words: &mut [u32], n: u32) {
    words[(n >> 5) as usize] |= 0x8000_0000 >> (n & 31);
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
/// variable 35 (`0x98EB2A`, read signed: [`Clear::seconds`]), the saves show blank, and the rank
/// is S at 240 seconds or less, then one lower each 60 seconds more, D past
/// 420. It returns before the scores, the records and the unlocks.
pub fn retrieve_extra(seconds: u32, saves: u32) -> Retrieve {
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

/// What `Result_retrieve` makes of a main game's clear beyond its digits
/// and rank: `0x995B48`'s bits ([`FLAG_SUBMACHINE`] and the rest), the weapons'
/// pieces' colours it sets, the records as the next scenario takes them and
/// the gallery's bits (`Set_registry_flg`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Earned {
    pub flags: u32,
    pub colours: [Option<u32>; 3],
    pub zapping: [u32; 2],
    pub gallery: [u32; 5],
}

/// `Set_registry_flg(index, sub)` (`0x442EA0`): the gallery's word
/// `index + sub / 32`, bit `0x80000000 >> sub % 32`.
fn registry(gallery: &mut [u32; 5], index: u32, sub: u32) {
    if let Some(word) = gallery.get_mut(((sub >> 5) + index) as usize) {
        *word |= 0x8000_0000 >> (sub & 31);
    }
}

/// `Result_retrieve` (`0x4B6E10`) for the main game: the digits, the scores
/// and the rank ([`main_scores`]), then, unless `fg_status`'s `0x4000000`
/// is up, the special weapons outside HARD (the rocket launcher for a time
/// score of 100 at rank A or better; a scenario B's submachine gun and
/// gatling gun for the third and the saves' scores), the gallery's bits
/// and the records.
pub fn retrieve_main(clear: &Clear) -> (Retrieve, Earned) {
    let (digits, time) = clear_time(clear.seconds, clear.saves);
    let (scores, rank) = main_scores(
        clear.seconds,
        clear.saves as u16,
        clear.third,
        clear.penalty,
    );
    let retrieve = Retrieve {
        digits,
        time,
        rank,
        scores,
    };
    let mut earned = Earned {
        flags: 0,
        colours: [None; 3],
        zapping: clear.zapping,
        gallery: [0; 5],
    };
    if clear.no_records {
        return (retrieve, earned);
    }
    let [submachine, rocket, gatling] = clear.held;
    // A weapon earned by `score` at rank A or better, which the box gets
    // and, the first time, step 6 shows; one already held only the box.
    let weapon = |earned: &mut Earned, piece: usize, flag: u32, won: bool, held: bool| {
        if held {
            earned.flags |= flag;
            earned.colours[piece - PIECE_SUBMACHINE] = Some(0);
        } else if won {
            earned.flags |= flag | FLAG_NEW_WEAPON;
            earned.colours[piece - PIECE_SUBMACHINE] = Some(FULL);
        }
    };
    let top = rank < 2;
    if !clear.hard {
        weapon(
            &mut earned,
            PIECE_ROCKET,
            FLAG_ROCKET,
            scores[0] == 100 && top,
            rocket,
        );
    }
    if !clear.scenario_b {
        for (index, sub) in [
            (2, 0x1B),
            (2, 0x1C),
            (0, 0x1C),
            (0, 0x1D),
            (0, 0x1E),
            (0, 0x1F),
        ] {
            registry(&mut earned.gallery, index, sub);
        }
        if !clear.hard {
            weapon(
                &mut earned,
                PIECE_SUBMACHINE,
                FLAG_SUBMACHINE,
                false,
                submachine,
            );
            weapon(&mut earned, PIECE_GATLING, FLAG_GATLING, false, gatling);
        }
        if !clear.item_9c {
            flag_on(&mut earned.zapping, RECORD_A);
        }
        return (retrieve, earned);
    }
    registry(&mut earned.gallery, 2, 0x1D);
    registry(&mut earned.gallery, 2, 0x1E);
    if !clear.hard {
        weapon(
            &mut earned,
            PIECE_SUBMACHINE,
            FLAG_SUBMACHINE,
            scores[2] == 100 && top,
            submachine,
        );
        weapon(
            &mut earned,
            PIECE_GATLING,
            FLAG_GATLING,
            scores[1] == 100 && top,
            gatling,
        );
    }
    if !flag(&earned.zapping, RECORD_S) {
        if rank == 0 {
            flag_on(&mut earned.zapping, RECORD_S);
            earned.flags |= FLAG_FIRST_S;
        }
    } else if !flag(&earned.zapping, RECORD_S_AGAIN) && clear.clears > 1 {
        flag_on(&mut earned.zapping, RECORD_S_AGAIN);
        earned.flags |= FLAG_S_AGAIN;
    }
    if !clear.item_9c && flag(&earned.zapping, RECORD_A) {
        earned.flags |= FLAG_BONUS;
    }
    if clear.special & 0x40 == 0 {
        earned.flags |= FLAG_NO_SPECIAL;
    }
    (retrieve, earned)
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
    /// the draw) and sub-step (`pCtcb + 9`: 1 after the save screen, 2 and
    /// 4 the exit).
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
    earned: Option<Earned>,
    piece_flags: [u8; PIECES],
    colours: [u32; PIECES],
    fade: Fade,
    drawn_fade: Option<(i16, u8)>,
    drawn: bool,
    background: bool,
    picture: Option<u8>,
    /// The raw pad `Pad_set` saw last, for the tick's new presses.
    last_raw: u16,
    /// The gallery's bits the results have set (`Set_registry_flg`).
    gallery: [u32; 5],
    clear: Clear,
}

impl Result {
    /// The task `Game_loop` chains for the game `clear` describes.
    /// `colours` are the pieces' as the last results left them (the
    /// executable's after the boot, [`Result::boot_colours`]), and `fade`
    /// fade slot 0 as the game left it. Until its first tick the work is as
    /// the boot left it (0) and the task's step is `Game_loop`'s (2).
    pub fn new(clear: Clear, colours: [u32; PIECES], fade: Fade) -> Self {
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
            earned: None,
            piece_flags: [0; PIECES],
            colours,
            fade,
            drawn_fade: None,
            drawn: false,
            background: false,
            picture: None,
            last_raw: 0,
            gallery: [0; 5],
            clear,
        }
    }

    /// The gallery's bits the results have set, to OR into config.ini's
    /// `GALLERY0` to `GALLERY4`.
    pub fn gallery(&self) -> [u32; 5] {
        self.gallery
    }

    /// Results that have set the gallery's `bits`.
    #[cfg(test)]
    pub fn set_gallery(&mut self, bits: [u32; 5]) {
        self.gallery = bits;
    }

    /// What `FUN_004b73c0` gives the next scenario's game from this clear:
    /// `0x995B48`'s weapons' bits and the records, once `Result_retrieve`
    /// has run in the main game.
    pub fn next_game(&self) -> Option<Earned> {
        self.earned
    }

    /// A tick the task sleeps under the save screen: `Pad_set` still reads
    /// the pad, and fade slot 0 is the screen's (`Cardaccess_init`'s and
    /// `Cardaccess_exit`'s), which the results find when they run again.
    pub fn sleep(&mut self, raw: u16, fade: Fade) {
        self.last_raw = raw;
        self.fade = fade;
        self.drawn = false;
        self.drawn_fade = None;
    }

    /// The rank (0 for S to 4 for D) `Result_retrieve` gave.
    pub fn rank(&self) -> u16 {
        self.retrieve.rank
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
    /// extra, else 0), the counts and flags cleared, `Result_retrieve` and
    /// the four fades off. The music's fade comes first.
    fn init(&mut self, music: &mut dyn MusicHost) {
        music.call(Music::FadeOut(MUSIC_FADE));
        self.piece_flags = [0; PIECES];
        self.background = false;
        self.mv = MV_STEPS;
        self.step = if self.clear.extra {
            STEP_EXTRA_WAIT
        } else {
            STEP_WAIT
        };
        self.count = 0;
        self.flags = FLAG_HIDE_PIECES;
        self.pieces_shown = 0;
        if self.clear.extra {
            self.retrieve = retrieve_extra(self.clear.seconds, self.clear.saves);
        } else {
            let (retrieve, earned) = retrieve_main(&self.clear);
            self.retrieve = retrieve;
            self.flags |= earned.flags;
            for (offset, colour) in earned.colours.iter().enumerate() {
                if let Some(colour) = colour {
                    self.colours[PIECE_SUBMACHINE + offset] = *colour;
                }
            }
            for (word, bits) in self.gallery.iter_mut().zip(earned.gallery) {
                *word |= bits;
            }
            self.earned = Some(earned);
        }
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

    /// `Init_movie_work(id)` (`0x4CAEC0`): the film, whose place in
    /// `gmovie_tbl0` it opens in the gallery's fourth word (Claire's from
    /// bit 16).
    fn film(&mut self, id: u8, music: &mut dyn MusicHost) {
        let base = if self.clear.claire { 0x10 } else { 0 };
        for (index, &film) in GALLERY_FILMS.iter().enumerate() {
            if film == id {
                registry(&mut self.gallery, 3, base + index as u32);
            }
        }
        music.film(id);
    }

    /// `Result_01`'s sub-step 1, the tick the save screen hands back
    /// (`Task_signal(0)`): `SuperHardSwitch` is set (this install's config
    /// has it), so the scenario's bits go back, and a scenario A's results
    /// end; a B's stop the main and start the credits' sub, at step 8, or
    /// 0x14 without `Special`'s 0x40.
    fn after_save(&mut self, music: &mut dyn MusicHost, out: &mut Output) -> bool {
        self.task_sub = 0;
        if !self.clear.scenario_b {
            self.task_sub = 2;
            return self.exit(out);
        }
        music.call(Music::Main(MAIN_NONE));
        music.call(Music::Sub(SUB_CREDITS));
        self.count = 0;
        self.step = STEP_CREDITS;
        if self.flags & FLAG_NO_SPECIAL != 0 {
            self.skip = 0;
            self.step = STEP_CREDITS_NO_SPECIAL;
            self.count = 1;
            return false;
        }
        self.count += 1;
        false
    }

    /// Step 5's reveal: the rank's letter, the scenario's name, and the
    /// time, the saves and their words.
    fn reveal_main(&mut self) {
        let rank = 4 + usize::from(self.retrieve.rank);
        if let Some(piece) = self.piece_flags.get_mut(rank) {
            *piece = SHOWN | FADE_IN;
        }
        let name = 9 + usize::from(self.clear.claire) + 2 * usize::from(self.clear.scenario_b);
        self.piece_flags[name] = SHOWN | FADE_IN;
        self.set_pieces(0x3_E000, SHOWN | FADE_IN, false);
        self.piece_step = PIECE_STEP;
        self.count = 1;
        self.skip = 1;
    }

    /// Step 5's end: the weapons' picture when a new one is earned (step 6,
    /// through a fade out and in that shows it), else the fade out to the
    /// save (step 7).
    fn rank_end(&mut self, music: &mut dyn MusicHost) -> bool {
        if self.flags & FLAG_NEW_WEAPON != 0 {
            self.pieces_shown = 0x1DC_0000;
            self.flags |= FLAG_SHOW_PIECES | FLAG_HIDE_PIECES;
            self.step = STEP_WEAPONS;
            self.fade_out();
        } else {
            music.call(Music::FadeOut(MUSIC_FADE));
            self.flags |= FLAG_HIDE_PIECES | FLAG_THEN_STEPS;
            self.step = STEP_SAVE;
            self.fade_out();
        }
        self.count = 1;
        false
    }

    /// The main game's steps (0 to 0x10, 0x14 to 0x17). Returns whether the
    /// task chained the title.
    fn main_steps(&mut self, trig: u16, music: &mut dyn MusicHost, out: &mut Output) -> bool {
        let key = trig & SKIP_KEYS != 0;
        match self.step {
            STEP_WAIT => {
                if !music.fading() {
                    if self.clear.scenario_b {
                        self.film(FILM_B, music);
                        self.step = STEP_FILM;
                    } else {
                        self.step = STEP_FILM_A;
                    }
                    self.count = 1;
                    return false;
                }
            }
            STEP_FILM_A => {
                if self.count == 300 {
                    self.film(FILM_A, music);
                    self.step = STEP_FILM;
                    self.count = 1;
                    return false;
                }
            }
            STEP_FILM => {
                if !music.film_playing() {
                    if !self.clear.scenario_b {
                        self.flags |= FLAG_PICTURE;
                        music.call(Music::FadeOut(MUSIC_FADE));
                    }
                    self.step = STEP_PICTURE;
                    self.count = 1;
                    return false;
                }
            }
            STEP_PICTURE => {
                if !music.fading() {
                    if !self.clear.scenario_b {
                        music.call(Music::Main(MAIN_NONE));
                    }
                    self.show_picture(if self.clear.claire {
                        PICTURE_CLAIRE
                    } else {
                        PICTURE_LEON
                    });
                    music.call(Music::Main(MAIN_PICTURE));
                    music.call(Music::Control(PLAY_MAIN));
                    self.skip = 0;
                    self.step = STEP_RANK;
                    self.count = 1;
                    return false;
                }
            }
            STEP_RANK => {
                if key {
                    if self.skip == 0 {
                        self.count = 0x1E0;
                        self.reveal_main();
                        return false;
                    }
                    if self.count >= 0x3C && self.skip == 1 {
                        self.count = 0x4B1;
                        return self.rank_end(music);
                    }
                }
                if self.count == 0x1E0 {
                    if self.skip == 0 {
                        self.reveal_main();
                        return false;
                    }
                } else if self.count > 0x4B0 {
                    return self.rank_end(music);
                }
            }
            STEP_WEAPONS => {
                if self.count >= 0x3D && key {
                    self.count = 0x385;
                } else if self.count < 0x385 {
                    self.count += 1;
                    return false;
                }
                music.call(Music::FadeOut(MUSIC_FADE));
                self.flags |= FLAG_HIDE_PIECES | FLAG_THEN_STEPS;
                self.fade_out();
                self.step = STEP_SAVE;
                self.count = 1;
                return false;
            }
            STEP_SAVE => {
                if !music.fading() {
                    // `Task_execute(1, Mem_card)` and `Task_sleep(1)`, which
                    // yields: the task's step 3, and no draw this tick.
                    out.save = true;
                    self.task_sub = 1;
                    return true;
                }
            }
            STEP_CREDITS..=0xD | STEP_CREDITS_NO_SPECIAL | 0x15 | STEP_BONUS => {
                self.step = STEP_BONUS;
                if self.flags & FLAG_BONUS == 0 {
                    self.step = STEP_HARD;
                    self.count = 1;
                    return false;
                }
                self.show_picture(PICTURE_BONUS);
                music.call(Music::Control(PLAY_SUB));
                self.skip = 0;
                self.step = STEP_BONUS_SHOW;
                self.count = 1;
                return false;
            }
            STEP_BONUS_SHOW => {
                if self.count == 0x28 {
                    if self.skip != 0 {
                        self.count += 1;
                        return false;
                    }
                    self.set_pieces(1 << 27, SHOWN | FADE_IN, false);
                }
                if self.count == 0x3C {
                    self.set_pieces(1 << 29, SHOWN, false);
                }
                if self.count == 0xF0 {
                    self.set_pieces(0x2800_0000, 0, false);
                    self.set_pieces(1 << 28, SHOWN | FADE_IN, false);
                }
                if self.count == 0x104 {
                    self.set_pieces(1 << 30, SHOWN, false);
                }
                if self.count > 0x1B8 {
                    self.set_pieces(1 << 30, 0, false);
                    self.flags |= FLAG_HIDE_PIECES | FLAG_THEN_STEPS;
                    self.fade_out();
                    self.step = STEP_HARD;
                    self.count = 1;
                    return false;
                }
            }
            STEP_END => {
                if self.count == 0x1E {
                    self.set_pieces(0x5000_0000, 0, false);
                    self.task_sub = 4;
                    return self.exit(out);
                }
            }
            STEP_HARD => {
                if !self.clear.hard {
                    self.step = STEP_END;
                    self.count = 1;
                    return false;
                }
                self.show_picture(PICTURE_HARD);
                self.step = STEP_HARD_SHOW;
                self.count = 1;
                return false;
            }
            STEP_HARD_SHOW => {
                if key && self.count >= 0xB5 {
                    self.count = 0x155;
                } else if self.count < 0x155 {
                    self.count += 1;
                    return false;
                }
                self.flags |= FLAG_HIDE_PIECES | FLAG_THEN_STEPS;
                self.fade_out();
                self.step = STEP_END;
                self.count = 1;
                return false;
            }
            _ => {}
        }
        self.count += 1;
        false
    }

    /// `Result_01` (`0x4B5E00`): the sub-step first (1 after the save
    /// screen, 2, 4 and 5 the exit, 3 counts), then the step: a main game's
    /// ([`Result::main_steps`]), or an extra's: 0x11 waits for the music's
    /// fade, 0x12 shows the mode's pieces on their counts and its picture on
    /// 0x4B0, 0x13 the rank, the time and the mode's name on 0x1E0 (or a
    /// key's), and goes on 0x514 to `Result_exit`. Returns whether the task
    /// chained the title.
    fn steps(&mut self, trig: u16, music: &mut dyn MusicHost, out: &mut Output) -> bool {
        match self.task_sub {
            1 => return self.after_save(music, out),
            2 | 4 | 5 => return self.exit(out),
            3 => {
                self.count += 1;
                return false;
            }
            _ => {}
        }
        if self.step < STEP_EXTRA_WAIT || self.step > STEP_EXTRA_RESULT {
            return self.main_steps(trig, music, out);
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
                    self.show_picture(if self.clear.tofu {
                        PICTURE_TOFU
                    } else {
                        PICTURE_4TH
                    });
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
        if self.clear.tofu {
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

    /// A drawn tick showing `pieces` at full colour and nothing else, for
    /// the screen's tests.
    #[cfg(test)]
    pub fn showing(pieces: &[usize]) -> Self {
        let mut result = Self::new(
            Clear::default(),
            [0; PIECES],
            Fade {
                level: 0,
                add: 0,
                mode: 0,
            },
        );
        for &piece in pieces {
            result.piece_flags[piece] = 1;
            result.colours[piece] = 0x80_8080;
        }
        result.drawn = true;
        result
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
        let long = retrieve_extra(i16::MAX as u32, 3);
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
        let retrieve = retrieve_extra(u32::MAX, 0);
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
        films: Vec<u8>,
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
        fn film(&mut self, id: u8) {
            self.films.push(id);
        }
        fn film_playing(&self) -> bool {
            false
        }
    }

    fn extra(tofu: bool) -> Clear {
        Clear {
            extra: true,
            tofu,
            seconds: 330,
            ..Clear::default()
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
        let mut result = Result::new(extra(tofu), Result::boot_colours(), fade);
        let mut music = FakeMusic {
            left: 0,
            fade_ticks,
            calls: Vec::new(),
            films: Vec::new(),
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
        let mut result = Result::new(extra(false), [0; PIECES], fade);
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

    fn main_clear() -> Clear {
        Clear {
            special: 15,
            ..Clear::default()
        }
    }

    #[test]
    fn a_fast_scenario_a_earns_the_rocket_launcher_and_its_records() {
        let (retrieve, earned) = retrieve_main(&main_clear());
        assert_eq!(retrieve.rank, 0);
        assert_eq!(retrieve.scores, [100, 100, 100]);
        assert_eq!(earned.flags, FLAG_ROCKET | FLAG_NEW_WEAPON);
        assert_eq!(earned.colours, [None, Some(FULL), None]);
        // The records' 0x12 (`fg_item`'s 0x9C is down).
        assert_eq!(earned.zapping[0], 0x8000_0000 >> 0x12);
        // `Set_registry_flg(2, 0x1B)`, `(2, 0x1C)` and `(0, 0x1C)` to `(0, 0x1F)`.
        assert_eq!(earned.gallery, [0xF, 0, 0x18, 0, 0]);
    }

    #[test]
    fn a_held_weapon_only_goes_to_the_box_and_hard_earns_none() {
        let held = Clear {
            held: [true, true, false],
            item_9c: true,
            ..main_clear()
        };
        let (_, earned) = retrieve_main(&held);
        assert_eq!(earned.flags, FLAG_SUBMACHINE | FLAG_ROCKET);
        assert_eq!(earned.colours, [Some(0), Some(0), None]);
        assert_eq!(
            earned.zapping, [0; 2],
            "fg_item's 0x9C keeps the record off"
        );
        let hard = Clear {
            hard: true,
            ..main_clear()
        };
        assert_eq!(retrieve_main(&hard).1.flags, 0);
        let none = Clear {
            no_records: true,
            ..main_clear()
        };
        assert_eq!(retrieve_main(&none).1.gallery, [0; 5]);
    }

    #[test]
    fn a_scenario_b_at_s_earns_all_three_and_its_records() {
        let clear = Clear {
            scenario_b: true,
            zapping: [0x8000_0000 >> RECORD_A, 0],
            ..main_clear()
        };
        let (_, earned) = retrieve_main(&clear);
        assert_eq!(
            earned.flags,
            FLAG_SUBMACHINE
                | FLAG_ROCKET
                | FLAG_GATLING
                | FLAG_NEW_WEAPON
                | FLAG_FIRST_S
                | FLAG_BONUS
                | FLAG_NO_SPECIAL
        );
        assert!(flag(&earned.zapping, RECORD_S));
        // A later S past the second clear.
        let again = Clear {
            zapping: earned.zapping,
            clears: 2,
            special: 0x40,
            ..clear
        };
        let (_, later) = retrieve_main(&again);
        assert_eq!(
            later.flags & (FLAG_FIRST_S | FLAG_S_AGAIN | FLAG_NO_SPECIAL),
            FLAG_S_AGAIN
        );
    }

    #[test]
    fn only_an_infinite_submachine_gun_or_rocket_launcher_counts_as_held() {
        assert!(!holds([[SUBMACHINE, 4]], SUBMACHINE));
        assert!(holds([[SUBMACHINE, 0xFF]], SUBMACHINE));
        assert!(holds([[GATLING, 1]], GATLING));
        assert!(!holds([[2, 0xFF]], ROCKET));
    }

    /// Run a main game's results with films that end at once, keys at
    /// `keys` (tick, raw), and the save screen handing back the tick after
    /// it opens; give each tick's (step, count, output).
    fn run_main(clear: Clear, keys: &[(u32, u16)]) -> (Vec<(u32, u32, Output)>, Vec<u8>) {
        let fade = Fade {
            level: -1,
            add: 0,
            mode: 2,
        };
        let mut result = Result::new(clear, Result::boot_colours(), fade);
        let mut music = FakeMusic {
            left: 0,
            fade_ticks: 180,
            calls: Vec::new(),
            films: Vec::new(),
        };
        let mut ticks = Vec::new();
        for tick in 0..6000 {
            let raw = keys
                .iter()
                .find(|(at, _)| *at == tick)
                .map_or(0, |&(_, raw)| raw);
            let out = result.tick(raw, &mut music);
            music.left = music.left.saturating_sub(1);
            ticks.push((result.step, result.count, out.clone()));
            if out.exit {
                break;
            }
        }
        (ticks, music.films)
    }

    #[test]
    fn a_scenario_as_results_play_their_film_then_save_and_end() {
        let (ticks, films) = run_main(main_clear(), &[]);
        assert_eq!(films, [FILM_A]);
        // The music's fade, then 300 counts to the film.
        assert_eq!(ticks[180].0, STEP_FILM_A);
        assert_eq!(ticks[480].0, STEP_FILM);
        let save = ticks.iter().position(|t| t.2.save).unwrap();
        assert_eq!(ticks[save].0, STEP_SAVE);
        // The weapons' picture came first: step 6 ran.
        assert!(ticks.iter().any(|t| t.0 == STEP_WEAPONS));
        // The tick after the save screen hands back, `Result_exit`.
        assert_eq!(ticks.len(), save + 3);
        assert!(ticks.last().unwrap().2.exit);
    }

    #[test]
    fn a_scenario_bs_results_go_on_after_the_save_to_hards_picture() {
        let clear = Clear {
            scenario_b: true,
            hard: true,
            ..main_clear()
        };
        let (ticks, films) = run_main(clear, &[]);
        assert_eq!(films, [FILM_B]);
        let save = ticks.iter().position(|t| t.2.save).unwrap();
        let after: Vec<u32> = ticks[save..].iter().map(|t| t.0).collect();
        for step in [STEP_HARD, STEP_HARD_SHOW, STEP_END] {
            assert!(after.contains(&step), "step {step:x} after the save");
        }
        assert!(ticks.last().unwrap().2.exit);
        // HARD's picture shows for 0x155 counts with no key.
        let shown = after.iter().filter(|&&step| step == STEP_HARD_SHOW).count();
        assert!(shown > 0x155);
    }

    #[test]
    fn the_ending_films_open_their_gallery_bits() {
        let (_, _) = run_main(main_clear(), &[]);
        let fade = Fade {
            level: -1,
            add: 0,
            mode: 2,
        };
        let claire = Clear {
            claire: true,
            scenario_b: true,
            ..main_clear()
        };
        let mut result = Result::new(claire, Result::boot_colours(), fade);
        let mut music = FakeMusic {
            left: 0,
            fade_ticks: 0,
            calls: Vec::new(),
            films: Vec::new(),
        };
        result.tick(0, &mut music);
        // Film 0xE is `gmovie_tbl0`'s 15th, Claire's from bit 16: bit 31.
        assert_eq!(result.gallery()[3], 1);
    }
}
