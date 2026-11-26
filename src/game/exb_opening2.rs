//! Extreme Battle's opening (`Exbtl_Opening`, `0x446D50`): the lines that
//! brief the player over black, which `Game_loop`'s step 1 starts as task 1
//! once a game (`fg_common`'s `0xFE` still down), and which suspends the
//! room's task until it ends.
//!
//! The task has two routines, by its `+8`: `Load_open5_7` (`0x446D90`),
//! whose first pass suspends the room, raises `fg_stop`'s `0x2000000`, sets
//! the background black (`Bg_set_mode(2, 0)`), loads `OPEN05.TIM`,
//! `OPEN07a.tim` and `OPEN07b.tim` as textures 0x12, 0x13 and 0x14, plays
//! stage 7's voice line 2 (`Xa_play(0, 2)` with `scd_var_stage` 7: the
//! eighth stage's bank) and puts the four fades off (`Fade_adjust(n, -1)`),
//! then waits 0x5A ticks and sets fade 0 clear (`Fade_set(0x200, 0, 7, 0)`);
//! and `FUN_004474A0`, which types the lines (`src`, `0x525438`) at
//! (0x3C, 0x20 + 0xE a line) with `Mess_print`, a character at a time on
//! its own delay (`0x525488`), with cue 0x22 for a letter, 0x24 for a space
//! and 0x23 at a line's end. Each page holds ten lines, the second from
//! line 10; after a page's last line it waits 0x3C ticks and fades it out
//! by `Fade_adjust(0, level)`, 0x400 a tick for the first page and 0x80
//! for the second. Any of the raw pad's 0x9F0 skips: the page fades by
//! 0x400 and the opening ends with it. Its end (`0x447030`) waits for fade
//! 0, stops the voice, puts `fg_stop` and the game mode to 0, loads the
//! cut's background again, sets the music back on, frees the pictures and
//! wakes the room's task.
//!
//! Every tick the lines run, `FUN_00447140` steps a timeline over the
//! task's 20 sprites (`0x525668`, 8 bytes each: shown, kind, a byte, the
//! record of `0x5255DC`, x and y). Up to 0x9DD it shows each pair of the
//! first twelve for a while: the Japanese text's lines from `OPEN05.TIM`
//! (kind 0, and kind 1, which draws nothing), which Classic REbirth's build
//! never shows (no recording's frame does). After 0x9DD, its second phase
//! (no skip from then), it plays voice line 100 (`Xa_play(0, 100)`, which
//! the install lacks) and flashes the static (kind 0x10, the last eight: a
//! grid of four by two from `OPEN07a.tim` or `OPEN07b.tim` by the frame, a
//! new frame every tick) until the second page's fade ends it.
//!
//! [`Opening::tick`] is one pass of the task. It knows nothing of the game:
//! it returns what the pass asks of it as [`Effect`]s, so it is tested on
//! its own.

/// `Load_open5_7`'s wait before the lines (`WORD_006880E4`).
const LOAD_WAIT: u16 = 0x5A;
/// The raw pad's buttons that skip (`0x9885FE & 0x9F0`).
pub const SKIP_BUTTONS: u16 = 0x9F0;
/// The lines' place on the screen: x, the first line's y and the step.
pub const LINE_X: i32 = 0x3C;
pub const LINE_Y: i32 = 0x20;
pub const LINE_STEP: i32 = 0xE;
/// A page's lines, and the second page's first.
const PAGE_LINES: u32 = 10;
const SECOND_PAGE: u8 = 10;
/// The wait after a line (`> 0x14`) and after a page (`0x3C`).
const LINE_PAUSE: u32 = 0x14;
const PAGE_HOLD: u32 = 0x3C;
/// Fade 0's step out: a skip's and the first page's, and the second's.
const FADE_FAST: u16 = 0x400;
const FADE_SLOW: u16 = 0x80;
/// The cues (`Snd_se_on(0x2NN0000)`): a letter, a space, a line's end.
pub const CUE_LETTER: u8 = 0x22;
pub const CUE_SPACE: u8 = 0x24;
pub const CUE_LINE: u8 = 0x23;
/// The voice lines `Xa_play(0, n)` plays with `scd_var_stage` 7: the
/// opening's, and the static's, which the install lacks.
pub const VOICE_OPENING: u8 = 2;
pub const VOICE_STATIC: u8 = 100;
/// The stage whose voice bank `Load_open5_7` plays from (`scd_var_stage`).
pub const VOICE_STAGE: u8 = 7;
/// The task's sprites (`WORD_0099DA8C`).
pub const SPRITES: usize = 20;
/// The timeline's count at which its second phase starts.
const STATIC_AT: u16 = 0x9DD;

/// The lines (`src`, `0x525438`), in the message font's codes, each ending
/// where the executable's `0xFE` does. Lines 3 and 18 are empty.
pub const LINES: [&[u8]; 20] = [
    &[
        0x1D, 0x00, 0x44, 0x3D, 0x56, 0x3D, 0x4E, 0x40, 0x4B, 0x51, 0x4F, 0x00, 0x52, 0x45, 0x4E,
        0x51, 0x4F, 0x00, 0x44, 0x3D, 0x4F,
    ],
    &[
        0x3E, 0x41, 0x41, 0x4A, 0x00, 0x40, 0x41, 0x50, 0x41, 0x3F, 0x50, 0x41, 0x40, 0x00, 0x45,
        0x4A, 0x4F, 0x45, 0x40, 0x41,
    ],
    &[
        0x50, 0x44, 0x41, 0x00, 0x50, 0x4E, 0x3D, 0x4A, 0x4F, 0x4C, 0x4B, 0x4E, 0x50, 0x00, 0x50,
        0x4E, 0x3D, 0x45, 0x4A, 0x01,
    ],
    &[],
    &[
        0x2E, 0x41, 0x50, 0x51, 0x4E, 0x4A, 0x00, 0x50, 0x4B, 0x00, 0x50, 0x44, 0x41, 0x00, 0x4C,
        0x4B, 0x48, 0x45, 0x3F, 0x41,
    ],
    &[
        0x4C, 0x4E, 0x41, 0x3F, 0x45, 0x4A, 0x3F, 0x50, 0x18, 0x00, 0x42, 0x45, 0x4A, 0x40,
    ],
    &[
        0x50, 0x44, 0x41, 0x00, 0x09, 0x10, 0x00, 0x4F, 0x4C, 0x41, 0x3F, 0x45, 0x3D, 0x48, 0x00,
        0x3E, 0x4B, 0x49, 0x3E, 0x4F, 0x0A,
    ],
    &[
        0x48, 0x4B, 0x3F, 0x3D, 0x50, 0x41, 0x40, 0x00, 0x53, 0x45, 0x50, 0x44, 0x45, 0x4A, 0x00,
        0x3D, 0x4A, 0x40, 0x00, 0x51, 0x4F, 0x41,
    ],
    &[
        0x50, 0x44, 0x41, 0x49, 0x00, 0x50, 0x4B, 0x00, 0x45, 0x4A, 0x3F, 0x45, 0x4A, 0x41, 0x4E,
        0x3D, 0x50, 0x41, 0x00, 0x3D, 0x48, 0x48,
    ],
    &[
        0x50, 0x4E, 0x3D, 0x3F, 0x41, 0x4F, 0x00, 0x4B, 0x42, 0x00, 0x50, 0x44, 0x41, 0x00, 0x52,
        0x45, 0x4E, 0x51, 0x4F, 0x1A, 0x1A,
    ],
    &[
        0x1D, 0x48, 0x50, 0x44, 0x4B, 0x51, 0x43, 0x44, 0x00, 0x51, 0x4A, 0x47, 0x4A, 0x4B, 0x53,
        0x4A,
    ],
    &[
        0x50, 0x44, 0x4E, 0x41, 0x3D, 0x50, 0x4F, 0x00, 0x3D, 0x4A, 0x40, 0x00, 0x50, 0x41, 0x4E,
        0x4E, 0x4B, 0x4E, 0x4F,
    ],
    &[
        0x53, 0x45, 0x48, 0x48, 0x00, 0x3E, 0x41, 0x00, 0x41, 0x4A, 0x3F, 0x4B, 0x51, 0x4A, 0x50,
        0x41, 0x4E, 0x41, 0x40,
    ],
    &[
        0x50, 0x44, 0x4E, 0x4B, 0x51, 0x43, 0x44, 0x4B, 0x51, 0x50, 0x00, 0x55, 0x4B, 0x51, 0x4E,
    ],
    &[
        0x49, 0x45, 0x4F, 0x4F, 0x45, 0x4B, 0x4A, 0x18, 0x00, 0x50, 0x44, 0x41, 0x00, 0x42, 0x3D,
        0x50, 0x41, 0x00, 0x4B, 0x42, 0x00, 0x3D, 0x48, 0x48,
    ],
    &[
        0x50, 0x44, 0x41, 0x00, 0x44, 0x51, 0x49, 0x3D, 0x4A, 0x47, 0x45, 0x4A, 0x40, 0x00, 0x40,
        0x41, 0x4C, 0x41, 0x4A, 0x40, 0x4F,
    ],
    &[
        0x51, 0x4C, 0x4B, 0x4A, 0x00, 0x55, 0x4B, 0x51, 0x4E, 0x00, 0x45, 0x49, 0x49, 0x41, 0x40,
        0x45, 0x3D, 0x50, 0x41,
    ],
    &[0x3D, 0x3F, 0x50, 0x45, 0x4B, 0x4A, 0x01, 0x01],
    &[],
    &[0x23, 0x2B, 0x2B, 0x20, 0x00, 0x28, 0x31, 0x1F, 0x27, 0x1A],
];

/// Each character's wait (`0x525488`), by its place in the page: a
/// character shows for one tick more than its wait.
const DELAYS: [u8; 0x150] = [
    5, 10, 5, 3, 5, 3, 5, 5, 3, 3, 5, 10, 5, 3, 5, 3, 5, 10, 5, 3, //
    5, 5, 3, 3, 5, 10, 5, 3, 5, 3, 5, 5, 3, 5, 10, 3, 5, 5, 3, 5, //
    3, 5, 5, 3, 10, 5, 5, 3, 5, 5, 5, 3, 5, 5, 10, 5, 5, 3, 3, 5, //
    6, 5, 3, 5, 3, 5, 5, 10, 5, 3, 10, 5, 5, 3, 10, 5, 3, 5, 3, 5, //
    3, 5, 5, 3, 5, 3, 5, 5, 5, 10, 5, 3, 5, 5, 5, 5, 3, 10, 6, 6, //
    10, 5, 5, 3, 5, 3, 3, 5, 10, 5, 3, 5, 5, 5, 6, 5, 3, 5, 3, 5, //
    3, 5, 10, 5, 3, 5, 5, 3, 5, 10, 3, 5, 5, 10, 3, 5, 3, 5, 5, 3, //
    5, 10, 5, 3, 10, 3, 5, 5, 3, 5, 3, 5, 3, 5, 3, 10, 3, 5, 3, 5, //
    5, 3, 5, 3, 5, 10, 3, 5, 10, 5, 5, 3, 10, 5, 3, 5, 3, 5, 6, 6, //
    3, 5, 5, 5, 3, 3, 5, 5, 10, 3, 5, 5, 5, 3, 5, 5, 5, 5, 5, 3, //
    3, 5, 5, 10, 3, 5, 5, 10, 5, 3, 5, 5, 3, 5, 5, 5, 3, 5, 5, 10, //
    5, 3, 10, 3, 5, 5, 3, 3, 5, 5, 3, 5, 3, 5, 5, 5, 5, 3, 3, 5, //
    5, 3, 3, 5, 10, 5, 3, 3, 5, 5, 3, 5, 5, 3, 3, 5, 6, 10, 5, 5, //
    3, 10, 5, 3, 5, 3, 10, 3, 5, 10, 3, 5, 4, 5, 5, 3, 10, 5, 3, 5, //
    3, 5, 5, 3, 5, 5, 10, 5, 3, 5, 3, 5, 5, 5, 3, 5, 3, 5, 10, 5, //
    3, 3, 5, 10, 3, 5, 5, 3, 5, 3, 3, 5, 3, 3, 5, 5, 3, 3, 5, 6, //
    6, 5, 3, 3, 5, 10, 5, 3, 5, 5, 5, 0, 0, 0, 0, 0,
];

/// The timeline's first phase (`FUN_00447140` while `0x99D03C` is 0): at
/// each count, the sprites it shows or hides. Its last entry starts the
/// second phase.
const FIRST_PHASE: [(u16, u32, bool); 12] = [
    (6, 0x3, true),
    (0x1AA, 0x3, false),
    (0x1C3, 0xC, true),
    (0x341, 0xC, false),
    (0x357, 0x30, true),
    (0x532, 0x30, false),
    (0x567, 0xC0, true),
    (0x741, 0xC0, false),
    (0x757, 0x300, true),
    (0x953, 0x300, false),
    (0x97C, 0xC00, true),
    (STATIC_AT, 0xC00, false),
];

/// The second phase: the static's rows (sprites 12 to 15 the top one, 16 to
/// 19 the bottom one) on and off by the count.
const SECOND_PHASE: [(u16, u32, bool); 13] = [
    (1, 0xF000, true),
    (2, 0xF000, false),
    (0x14, 0xF_0000, true),
    (0x15, 0xF_0000, false),
    (0x23, 0xF_F000, true),
    (0x24, 0xF_F000, false),
    (0x2D, 0xF_0000, true),
    (0x2E, 0xF_0000, false),
    (0x32, 0xF000, true),
    (0x33, 0xF000, false),
    (0x37, 0xF_0000, true),
    (0x39, 0xF_0000, false),
    (0x3C, 0xF_F000, true),
];

/// One of the static's eight pictures (`0x525668`'s records 12 to 19, and
/// their records of `0x5255DC`, 6 to 13): its place on the screen, and the
/// texels it draws scaled to 80 by 120 from a 120 by 180 rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Static {
    pub x: i32,
    pub y: i32,
    pub u: u8,
    pub v: u8,
}

/// The static's size on the screen, and in the picture.
pub const STATIC_SIZE: [i32; 2] = [80, 120];
pub const STATIC_TEXELS: [i32; 2] = [120, 180];

/// Sprites 12 to 19.
pub const STATICS: [Static; 8] = [
    Static {
        x: 5,
        y: 15,
        u: 0,
        v: 0,
    },
    Static {
        x: 0x50,
        y: 15,
        u: 3,
        v: 20,
    },
    Static {
        x: 0xA0,
        y: 15,
        u: 5,
        v: 30,
    },
    Static {
        x: 0xEB,
        y: 15,
        u: 7,
        v: 0,
    },
    Static {
        x: 5,
        y: 0x69,
        u: 0,
        v: 75,
    },
    Static {
        x: 0x50,
        y: 0x69,
        u: 5,
        v: 50,
    },
    Static {
        x: 0xA0,
        y: 0x69,
        u: 3,
        v: 40,
    },
    Static {
        x: 0xEB,
        y: 0x69,
        u: 7,
        v: 75,
    },
];

/// What a pass asks of the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// `Load_open5_7`'s first pass: `Task_suspend(0)`, `fg_stop`'s
    /// `0x2000000`, the background black, and the four fades off
    /// (`Fade_adjust(n, -1)`).
    Begin,
    /// `Xa_play(0, n)` with `scd_var_stage` [`VOICE_STAGE`].
    Voice(u8),
    /// `Snd_se_on(0x2NN0000)`: a sound of the room's bank, not placed.
    Cue(u8),
    /// `Fade_set(0x200, 0, 7, 0)`: fade 0 clear, its step 0.
    FadeClear,
    /// `Fade_adjust(0, level, 0)`.
    Fade(i16),
    /// The end (`0x447030`): `Xa_stop` while the voice bit is up, `fg_stop`
    /// and the game mode 0, the cut's background loaded again,
    /// `Snd_bgm_ctr(0x1000000)` and `Snd_bgm_tbl_set(0x612FF37)`, and
    /// `Task_signal(0)`: the room runs from the next tick.
    End,
}

/// The opening's task and its work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opening {
    /// The task's `+8` (0 `Load_open5_7`, 1 the lines) and `+9`.
    routine: u8,
    step: u8,
    /// `WORD_006880E4`.
    wait: u16,
    /// The timeline's count and phase (`0x99D024`, `0x99D03C`).
    count: u16,
    phase: u16,
    /// The lines' state, line, column, count and delay index (`0x99DA84`,
    /// `0x99DA90`, `0x99D034`, `0x99D038`, `0x99DA80`).
    state: u32,
    line: u32,
    column: u32,
    ticks: u32,
    delay: u32,
    /// Fade 0's level and step (`0x99D01A`, `0x99D030`).
    level: u16,
    add: u16,
    /// The last page's flag (`0x99D026`) and the page's first line
    /// (`0x99D028`).
    last: bool,
    page: u8,
    /// The static's frame (`0x99D01D`) and the frames drawn (`0x99D02C`).
    frame: u8,
    frames: u32,
    /// Each sprite's shown flag.
    shown: [u8; SPRITES],
    /// Fade 0's level as `Fade_status(0)` reads it.
    fade: i16,
    /// The line this pass typed into, and how much of it it shows.
    typed: Option<(u8, u8)>,
    /// Whether this pass drew the page and the sprites.
    drawn: bool,
    /// The task has exited.
    done: bool,
}

impl Default for Opening {
    fn default() -> Self {
        Self::new()
    }
}

impl Opening {
    /// The task as `Game_loop`'s `Task_execute(1, Exbtl_Opening)` leaves
    /// it, before its first pass.
    pub fn new() -> Self {
        Self {
            routine: 0,
            step: 0,
            wait: 0,
            count: 0,
            phase: 0,
            state: 0,
            line: 0,
            column: 0,
            ticks: 0,
            delay: 0,
            level: 0,
            add: 0,
            last: false,
            page: 0,
            frame: 0,
            frames: 0,
            shown: [0; SPRITES],
            fade: -1,
            typed: None,
            drawn: false,
            done: false,
        }
    }

    /// Whether the task has exited: the room's task runs again.
    pub fn done(&self) -> bool {
        self.done
    }

    /// Whether the room's task is suspended: from the first pass's
    /// `Task_suspend(0)` until the end's `Task_signal(0)`.
    pub fn holds_room(&self) -> bool {
        !self.done
    }

    /// One pass of the task (`Exbtl_Opening`), on the tick's raw pad
    /// trigger (`0x9885FE`).
    pub fn tick(&mut self, trigger: u16) -> Vec<Effect> {
        let mut effects = Vec::new();
        self.typed = None;
        self.drawn = false;
        if self.done {
            return effects;
        }
        if self.routine != 1 {
            if self.load(&mut effects) {
                return effects;
            }
            self.routine = 1;
        }
        if !self.lines(trigger, &mut effects) {
            self.routine = 0;
            self.done = true;
        }
        effects
    }

    /// `Load_open5_7`. Returns whether it slept.
    fn load(&mut self, effects: &mut Vec<Effect>) -> bool {
        if self.step == 1 {
            if self.wait == 0 {
                effects.push(Effect::FadeClear);
                self.fade = 0;
                self.step = 0;
                return false;
            }
        } else {
            effects.push(Effect::Begin);
            effects.push(Effect::Voice(VOICE_OPENING));
            *self = Self {
                step: 1,
                wait: LOAD_WAIT,
                ..Self::new()
            };
        }
        self.wait -= 1;
        true
    }

    /// `FUN_004474A0`. Returns whether it slept; when it didn't, the task
    /// has ended.
    fn lines(&mut self, trigger: u16, effects: &mut Vec<Effect>) -> bool {
        if self.step == 1 {
            return self.end(effects);
        }
        loop {
            if trigger & SKIP_BUTTONS != 0 && self.phase == 0 {
                self.last = true;
                self.add = FADE_FAST;
            }
            self.level = self.level.wrapping_add(self.add);
            self.fade = self.level as i16;
            effects.push(Effect::Fade(self.fade));
            if self.fade >= 0 {
                self.run_page(effects);
                self.timeline(effects);
                self.drawn = true;
                self.frames = self.frames.wrapping_add(1);
                if self.frames == u32::MAX {
                    self.frames = u32::MAX - 1;
                }
                return true;
            }
            self.add = 0;
            self.level = 0xFFFF;
            if self.last {
                self.step = 1;
                return self.end(effects);
            }
            // The first page is out: the second from its start.
            effects.push(Effect::FadeClear);
            self.fade = 0;
            self.level = 0;
            self.state = 0;
            self.line = 0;
            self.column = 0;
            self.ticks = 0;
            self.delay = 0;
            self.add = 0;
            self.last = true;
            self.page = SECOND_PAGE;
        }
    }

    /// The page's state (`0x99DA84`'s switch) and its typing.
    fn run_page(&mut self, effects: &mut Vec<Effect>) {
        match self.state {
            0 => {
                self.state = 1;
                self.column = 0;
                self.type_character(effects);
            }
            1 => self.type_character(effects),
            2 => {
                self.ticks += 1;
                if self.ticks > LINE_PAUSE {
                    self.ticks = 0;
                    self.state = 0;
                }
            }
            3 | 4 => {
                if self.state == 3 {
                    self.ticks = PAGE_HOLD;
                    self.state = 4;
                }
                self.ticks = self.ticks.wrapping_sub(1);
                if self.ticks == 0 {
                    if self.add == 0 {
                        self.add = if self.last { FADE_SLOW } else { FADE_FAST };
                    }
                    self.state = 5;
                }
            }
            _ => {}
        }
    }

    /// The line's next character, or its end.
    fn type_character(&mut self, effects: &mut Vec<Effect>) {
        let index = usize::from(self.page) + self.line as usize;
        let text = LINES[index];
        let Some(&code) = text.get(self.column as usize) else {
            self.line += 1;
            if self.line < PAGE_LINES {
                effects.push(Effect::Cue(CUE_LINE));
                self.state = 2;
            } else {
                self.state = 3;
            }
            return;
        };
        self.typed = Some((index as u8, self.column as u8 + 1));
        if self.ticks == 0 {
            effects.push(Effect::Cue(if code == 0 { CUE_SPACE } else { CUE_LETTER }));
        }
        self.ticks += 1;
        let wait = DELAYS.get(self.delay as usize).copied().unwrap_or(0);
        if u32::from(wait) < self.ticks {
            self.delay += 1;
            self.column += 1;
            self.ticks = 0;
        }
    }

    /// `FUN_00447140`: the timeline's count and the sprites it shows.
    fn timeline(&mut self, effects: &mut Vec<Effect>) {
        if self.phase == 0 {
            if let Some(&(_, mask, on)) = FIRST_PHASE.iter().find(|(at, ..)| *at == self.count) {
                self.show(mask, on);
                if self.count == STATIC_AT {
                    self.phase = 1;
                    self.count = 1;
                    return;
                }
            }
        } else {
            self.frame = (self.frame + 1) & 3;
            if self.count == 1 {
                effects.push(Effect::Voice(VOICE_STATIC));
            }
            if let Some(&(_, mask, on)) = SECOND_PHASE.iter().find(|(at, ..)| *at == self.count) {
                self.show(mask, on);
            }
        }
        self.count = self.count.wrapping_add(1);
    }

    /// `FUN_004479E0(on, 0)`: the sprites in `mask` shown or hidden.
    fn show(&mut self, mask: u32, on: bool) {
        for (n, shown) in self.shown.iter_mut().enumerate() {
            if mask >> n & 1 != 0 {
                *shown = u8::from(on);
            }
        }
    }

    /// The end (`0x447030`): it waits for fade 0, then hands the room back.
    fn end(&mut self, effects: &mut Vec<Effect>) -> bool {
        if self.fade >= 0 {
            return true;
        }
        effects.push(Effect::End);
        self.shown = [0; SPRITES];
        false
    }

    /// What the pass drew: each line's codes as `Mess_print` printed them,
    /// at its place, then the static's pictures over them.
    pub fn view(&self) -> View {
        let mut lines = Vec::new();
        if self.drawn {
            if let Some((index, shown)) = self.typed {
                let text = LINES[usize::from(index)];
                lines.push((
                    self.line_y(u32::from(index - self.page)),
                    &text[..usize::from(shown)],
                ));
            }
            for n in 0..self.line {
                let text = LINES[usize::from(self.page) + n as usize];
                lines.push((self.line_y(n), text));
            }
        }
        let statics = if self.drawn {
            STATICS
                .iter()
                .enumerate()
                .filter(|(n, _)| self.shown[12 + n] & 1 != 0)
                .map(|(_, sprite)| *sprite)
                .collect()
        } else {
            Vec::new()
        };
        View {
            lines,
            statics,
            frame: self.frame,
        }
    }

    fn line_y(&self, line: u32) -> i32 {
        LINE_Y + line as i32 * LINE_STEP
    }

    /// The trace's `exo` (`scripts/oracle-re2.py`'s `exbattle_opening_trace`).
    pub fn trace(&self) -> String {
        if self.done {
            return "-".to_string();
        }
        let mask = self
            .shown
            .iter()
            .enumerate()
            .fold(0u32, |mask, (n, &shown)| mask | u32::from(shown & 1) << n);
        format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{mask:x}",
            self.routine,
            self.step,
            self.wait,
            self.count,
            self.phase,
            self.state,
            self.line,
            self.column,
            self.ticks,
            self.delay,
            self.level,
            self.add,
            u8::from(self.last),
            self.page,
            self.frame,
            self.frames,
        )
    }
}

/// What one pass drew (`FUN_004474A0`'s `Mess_print`s and `FUN_00447830`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    /// Each line's y (at x [`LINE_X`]) and its codes.
    pub lines: Vec<(i32, &'static [u8])>,
    /// The static's pictures shown.
    pub statics: Vec<Static>,
    /// The static's frame: 0 and 1 from `OPEN07a.tim`, 2 and 3 from
    /// `OPEN07b.tim`, 1 and 2 from the picture's right half (u + 0x80).
    pub frame: u8,
}

impl View {
    /// The picture the static draws from (0 `OPEN07a`, 1 `OPEN07b`) and
    /// its u offset, by the frame.
    pub fn static_page(&self) -> (usize, i32) {
        match self.frame & 3 {
            0 => (0, 0),
            1 => (0, 0x80),
            2 => (1, 0x80),
            _ => (1, 0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run passes until `until` says stop, with `skip` pressed on pass
    /// `skip_at` (counting from the first pass, 0); returns the passes
    /// run and every pass's effects.
    fn run(
        opening: &mut Opening,
        skip_at: Option<u32>,
        mut until: impl FnMut(&Opening) -> bool,
    ) -> (u32, Vec<Vec<Effect>>) {
        let mut all = Vec::new();
        let mut pass = 0;
        while !until(opening) {
            let trigger = if Some(pass) == skip_at { 0x80 } else { 0 };
            all.push(opening.tick(trigger));
            pass += 1;
            assert!(pass < 10_000, "the opening never ended");
        }
        (pass, all)
    }

    #[test]
    fn the_first_pass_suspends_plays_the_line_and_waits() {
        let mut opening = Opening::new();
        let effects = opening.tick(0);
        assert_eq!(effects, [Effect::Begin, Effect::Voice(VOICE_OPENING)]);
        assert_eq!(opening.trace(), "0,1,89,0,0,0,0,0,0,0,0,0,0,0,0,0,0");
        assert!(opening.holds_room());
        assert!(opening.view().lines.is_empty());
        // `special_ex_claire`: the first pass on 1398, fade 0 clear on 1488
        // with the first letter's cue.
        for _ in 1..90 {
            assert!(opening.tick(0).is_empty());
        }
        let effects = opening.tick(0);
        assert_eq!(
            effects,
            [Effect::FadeClear, Effect::Fade(0), Effect::Cue(CUE_LETTER)]
        );
        assert_eq!(opening.view().lines, [(LINE_Y, &LINES[0][..1])]);
    }

    #[test]
    fn the_letters_cue_on_their_first_tick_and_spaces_cue_their_own() {
        let mut opening = Opening::new();
        for _ in 0..90 {
            opening.tick(0);
        }
        // 'A' (wait 5): six ticks, then the space (wait 10), its cue 0x24,
        // as `special_ex_claire` has them on 1488 and 1494, and 'h' on 1505.
        let mut cues = Vec::new();
        for pass in 0..18 {
            for effect in opening.tick(0) {
                if let Effect::Cue(cue) = effect {
                    cues.push((pass, cue));
                }
            }
        }
        assert_eq!(cues, [(0, CUE_LETTER), (6, CUE_SPACE), (17, CUE_LETTER)]);
    }

    #[test]
    fn a_skip_fades_the_page_and_ends_the_opening() {
        // `special_ex_claire`: the first pass on 1398, a skip on 1600 (pass
        // 202), fade 0 at 1024 on it, and the end on 1631.
        let mut opening = Opening::new();
        let (passes, effects) = run(&mut opening, Some(202), Opening::done);
        assert_eq!(passes, 1631 - 1398 + 1);
        assert!(effects[202].contains(&Effect::Fade(0x400)));
        let last = effects.last().unwrap();
        assert_eq!(last.first(), Some(&Effect::Fade(-0x8000)));
        assert_eq!(last.last(), Some(&Effect::End));
        assert_eq!(opening.trace(), "-");
    }

    #[test]
    fn without_a_key_both_pages_and_the_static_run_to_the_second_fade() {
        // `special_ex_leon`: the first pass on 1408, the first page's fade
        // from 2846, the second page from 2877, and the static from 4023.
        let mut opening = Opening::new();
        let first = 1408;
        let (passes, effects) = run(&mut opening, None, Opening::done);
        let at = |tick: u32| &effects[(tick - first) as usize];
        assert!(at(2846).contains(&Effect::Fade(0x400)));
        assert!(at(2845).contains(&Effect::Fade(0)));
        assert!(at(2877).contains(&Effect::FadeClear));
        assert!(at(4024).contains(&Effect::Voice(VOICE_STATIC)));
        assert_eq!(first + passes - 1, 4300);
        assert_eq!(effects.last().unwrap().last(), Some(&Effect::End));
    }

    #[test]
    fn a_skip_in_the_static_does_nothing() {
        let mut opening = Opening::new();
        run(&mut opening, None, |opening| opening.phase == 1);
        // The second page is still up (its fade starts after the static's):
        // the press starts no fade.
        assert_eq!((opening.add, opening.level), (0, 0));
        opening.tick(0x80);
        assert_eq!((opening.add, opening.level), (0, 0));
    }

    #[test]
    fn the_static_flashes_its_rows_and_cycles_its_frames() {
        let mut opening = Opening::new();
        run(&mut opening, None, |opening| opening.phase == 1);
        // The pass that turned the phase: count 1 next, which shows the top
        // row and plays line 100.
        let effects = opening.tick(0);
        assert!(effects.contains(&Effect::Voice(VOICE_STATIC)));
        let view = opening.view();
        assert_eq!(view.statics, STATICS[..4]);
        assert_eq!(view.frame, 1);
        assert_eq!(view.static_page(), (0, 0x80));
        opening.tick(0);
        assert!(opening.view().statics.is_empty());
        assert_eq!(opening.view().static_page(), (1, 0x80));
    }

    #[test]
    fn the_finished_lines_stay_under_the_one_being_typed() {
        let mut opening = Opening::new();
        run(&mut opening, None, |opening| opening.line == 2);
        // The line end's pass draws the two finished lines alone.
        let view = opening.view();
        assert_eq!(
            view.lines,
            [(LINE_Y, LINES[0]), (LINE_Y + LINE_STEP, LINES[1])]
        );
        // The empty line 3 ends at once: its pass has no typed line.
        run(&mut opening, None, |opening| opening.line == 4);
        assert_eq!(opening.view().lines.len(), 4);
    }
}
