//! Extreme Battle's end: `Result` under `fg_system`'s `0x1000000`
//! (`Result_4B7B00`). `Result_init` loads no picture of the results and
//! plays main 0x38; then each tick runs `FUN_004B7910` and the lines.
//!
//! `FUN_004B7910`'s first pass sets fade 0 clear (`Fade_set(0x200, 0)`) and
//! sleeps 0x5A ticks (`WORD_006897A8`). From then on each pass counts
//! (`0x995C38`) and, on set counts, shows or hides its twelve sprites
//! (`BYTE_ARRAY_0052CD20`, drawn by `FUN_004B7E50` from texture 0x12),
//! which carry the Japanese build's lines and draw nothing in Classic
//! REbirth's English one.
//!
//! The lines (`ptr_ex_battle_end`, `0x52CD80`) are typed as Extreme
//! Battle's opening types its own ([`crate::game::exb_opening2`]), with
//! `Mess_print` at (0x3C, 0x20 + 0xD a line), a character at a time on its
//! own delay (`0x52CDD8`), with cue 0x22 for a letter, 0x24 for a space and
//! 0x23 at a line's end. Each page holds eleven lines, the second from line
//! 11; after a page's last line the task waits 0x3C ticks and fades it out
//! by `Fade_adjust(0, level)`, 0x400 a tick for the first page and 0x80 for
//! the second, which no key skips. The second page's end is
//! `Result_exit`'s. Level 3's `Fade_set(0x203, -0x100)` at the second page
//! and `Result_load_bg(7)` at count 0x4DB aren't ported.

use super::Fade;
pub use crate::game::exb_opening2::{CUE_LETTER, CUE_LINE, CUE_SPACE, LINE_X, LINE_Y};

/// The lines' step down the screen.
pub const LINE_STEP: i32 = 0xD;
/// A page's lines, and the second page's first.
const PAGE_LINES: u32 = 11;
const SECOND_PAGE: u8 = 11;
/// `FUN_004B7910`'s first wait (`WORD_006897A8`).
const WAIT: u16 = 0x5A;
/// The wait after a line (`> 0x14`) and after a page (`0x3C`).
const LINE_PAUSE: u32 = 0x14;
const PAGE_HOLD: u32 = 0x3C;
/// Fade 0's step out: the first page's and the second's.
const FADE_FAST: u16 = 0x400;
const FADE_SLOW: u16 = 0x80;
/// `Fade_set(0x200, 0)`'s blend: to black.
const FADE_BLACK: u8 = 2;
/// The sprites `FUN_004B7FB0` sets.
pub const SPRITES: usize = 12;
/// `FUN_004B7910`'s count at which the sprites of a mask are shown or
/// hidden.
const TIMELINE: [(u32, u16, bool); 10] = [
    (0x92, 0x3, true),
    (0x11E, 0x3, false),
    (0x134, 0xC, true),
    (0x2A9, 0xC, false),
    (0x2C1, 0x30, true),
    (0x4B0, 0x30, false),
    (0x4DF, 0xC0, true),
    (0x663, 0xC0, false),
    (0x679, 0x300, true),
    (0x7F4, 0x300, false),
];

/// The lines (`ptr_ex_battle_end`), in the message font's codes, each
/// ending where the executable's `0xFE` does. The empty ones point at a
/// lone `0xFE`.
pub const LINES: [&[u8]; 22] = [
    &[
        0x29, 0x45, 0x4F, 0x4F, 0x45, 0x4B, 0x4A, 0x00, 0x3F, 0x4B, 0x49, 0x4C, 0x48, 0x41, 0x50,
        0x41, 0x1A, 0x1A,
    ],
    &[],
    &[
        0x30, 0x44, 0x3D, 0x4A, 0x47, 0x4F, 0x00, 0x50, 0x4B, 0x00, 0x55, 0x4B, 0x51, 0x4E, 0x00,
        0x41, 0x42, 0x42, 0x4B, 0x4E, 0x50, 0x4F, 0x18,
    ],
    &[
        0x50, 0x44, 0x41, 0x00, 0x44, 0x3D, 0x56, 0x3D, 0x4E, 0x40, 0x4B, 0x51, 0x4F, 0x00, 0x52,
        0x45, 0x4E, 0x51, 0x4F,
    ],
    &[
        0x44, 0x3D, 0x4F, 0x00, 0x3E, 0x41, 0x41, 0x4A, 0x00, 0x41, 0x4E, 0x3D, 0x40, 0x45, 0x3F,
        0x3D, 0x50, 0x41, 0x40,
    ],
    &[
        0x42, 0x4E, 0x4B, 0x49, 0x00, 0x4B, 0x51, 0x4E, 0x00, 0x53, 0x4B, 0x4E, 0x48, 0x40, 0x01,
    ],
    &[],
    &[
        0x1D, 0x48, 0x48, 0x00, 0x4B, 0x42, 0x00, 0x44, 0x51, 0x49, 0x3D, 0x4A, 0x45, 0x50, 0x55,
        0x00, 0x44, 0x3D, 0x4F,
    ],
    &[
        0x3E, 0x41, 0x41, 0x4A, 0x00, 0x4F, 0x3D, 0x52, 0x41, 0x40, 0x00, 0x42, 0x4E, 0x4B, 0x49,
    ],
    &[
        0x50, 0x44, 0x41, 0x00, 0x50, 0x41, 0x4E, 0x4E, 0x4B, 0x4E, 0x00, 0x4B, 0x42, 0x00, 0x50,
        0x44, 0x45, 0x4F,
    ],
    &[
        0x1E, 0x45, 0x4B, 0x00, 0x24, 0x3D, 0x56, 0x3D, 0x4E, 0x40, 0x01,
    ],
    &[
        0x2F, 0x50, 0x4B, 0x4E, 0x45, 0x41, 0x4F, 0x00, 0x53, 0x45, 0x48, 0x48, 0x00, 0x3E, 0x41,
    ],
    &[
        0x50, 0x4B, 0x48, 0x40, 0x00, 0x42, 0x4B, 0x4E, 0x00, 0x43, 0x41, 0x4A, 0x41, 0x4E, 0x3D,
        0x50, 0x45, 0x4B, 0x4A, 0x4F, 0x00, 0x50, 0x4B,
    ],
    &[
        0x3F, 0x4B, 0x49, 0x41, 0x00, 0x3D, 0x3E, 0x4B, 0x51, 0x50, 0x00, 0x55, 0x4B, 0x51, 0x4E,
        0x00, 0x43, 0x4E, 0x41, 0x3D, 0x50,
    ],
    &[
        0x3D, 0x3F, 0x44, 0x45, 0x41, 0x52, 0x41, 0x49, 0x41, 0x4A, 0x50,
    ],
    &[
        0x4B, 0x42, 0x00, 0x4F, 0x45, 0x4A, 0x43, 0x48, 0x41, 0x44, 0x3D, 0x4A, 0x40, 0x41, 0x40,
        0x48, 0x55,
    ],
    &[
        0x4B, 0x52, 0x41, 0x4E, 0x3F, 0x4B, 0x49, 0x45, 0x4A, 0x43, 0x00, 0x3D, 0x48, 0x48, 0x00,
        0x3D, 0x40, 0x52, 0x41, 0x4E, 0x4F, 0x45, 0x50, 0x55, 0x01,
    ],
    &[],
    &[],
    &[
        0x00, 0x1F, 0x2B, 0x2A, 0x23, 0x2E, 0x1D, 0x30, 0x31, 0x28, 0x1D, 0x30, 0x25, 0x2B, 0x2A,
        0x2F,
    ],
    &[],
    &[
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x24, 0x21, 0x2E, 0x2B, 0x1A, 0x1A,
    ],
];

/// Each character's delay (`BYTE_ARRAY_0052CDD8`), counted from a page's first.
const DELAYS: [u8; 160] = [
    5, 3, 5, 5, 3, 3, 5, 10, 5, 3, 5, 5, 5, 3, 5, 3, 6, 4, 5, 5, 3, 5, 5, 5, 10, 5, 3, 10, 5, 3, 3,
    5, 10, 3, 5, 5, 3, 5, 5, 5, 4, 5, 5, 3, 10, 5, 3, 5, 3, 5, 5, 3, 3, 5, 10, 5, 3, 5, 3, 5, 5, 3,
    5, 10, 5, 3, 3, 5, 10, 3, 5, 3, 5, 3, 5, 3, 5, 3, 5, 5, 5, 3, 5, 10, 3, 3, 5, 10, 5, 3, 5, 3,
    5, 6, 3, 5, 3, 10, 3, 5, 10, 5, 3, 5, 3, 5, 3, 5, 5, 10, 5, 3, 5, 5, 3, 3, 5, 10, 5, 3, 5, 3,
    5, 10, 5, 5, 3, 5, 5, 5, 3, 10, 5, 3, 5, 5, 3, 5, 10, 3, 5, 10, 5, 5, 3, 5, 5, 3, 3, 10, 5, 3,
    5, 3, 5, 5, 6, 5, 5, 3,
];

/// What a pass asks of the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// `Snd_se_on(0x2NN0000)`.
    Cue(u8),
    /// `Result_exit`.
    Exit,
}

/// The task's work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExEnd {
    /// The task's `+10`: `FUN_004B7910` is in its first wait.
    waiting: bool,
    /// `0x995C38`, `WORD_006897A8`.
    count: u32,
    wait: u16,
    /// The lines' state, line, column, count and delay index (`0x995B40`,
    /// `0x99CE1C`, `0x995C28`, `0x995C2C`, `0x99C874`).
    state: u32,
    line: u32,
    column: u32,
    ticks: u32,
    delay: u32,
    /// Fade 0's level and step (`0x995B46`, `0x995B88`).
    level: u16,
    add: u16,
    /// The last page's flag (`0x995C30`) and the page's first line
    /// (`0x99C871`).
    last: bool,
    page: u8,
    /// Each sprite's shown flag.
    shown: [u8; SPRITES],
    /// The line this pass typed into, and how much of it it shows.
    typed: Option<(u8, u8)>,
    /// Whether this pass drew the page.
    drawn: bool,
}

impl Default for ExEnd {
    fn default() -> Self {
        Self::new()
    }
}

impl ExEnd {
    /// The work as `Result_init` leaves it: the count at 0 and the sprites
    /// hidden.
    pub fn new() -> Self {
        Self {
            waiting: false,
            count: 0,
            wait: 0,
            state: 0,
            line: 0,
            column: 0,
            ticks: 0,
            delay: 0,
            level: 0,
            add: 0,
            last: false,
            page: 0,
            shown: [0; SPRITES],
            typed: None,
            drawn: false,
        }
    }

    /// One pass of `Result_4B7B00` on fade slot 0.
    pub fn tick(&mut self, fade: &mut Fade) -> Vec<Effect> {
        let mut effects = Vec::new();
        self.typed = None;
        self.drawn = false;
        if self.timeline(fade) {
            return effects;
        }
        loop {
            self.level = self.level.wrapping_add(self.add);
            // `Fade_adjust(0, level)`, then `Fade_status(0)`.
            fade.level = self.level as i16;
            if !fade.done() {
                break;
            }
            self.add = 0;
            self.level = 0xFFFF;
            if self.last {
                effects.push(Effect::Exit);
                return effects;
            }
            // The first page is out: the second from its start, and
            // `FUN_004B7910` again.
            fade.set(FADE_BLACK, 0);
            self.state = 0;
            self.line = 0;
            self.column = 0;
            self.ticks = 0;
            self.delay = 0;
            self.level = 0;
            self.add = 0;
            self.last = true;
            self.page = SECOND_PAGE;
            if self.timeline(fade) {
                return effects;
            }
        }
        self.run_page(&mut effects);
        self.drawn = true;
        effects
    }

    /// `FUN_004B7910`. Returns whether it slept.
    fn timeline(&mut self, fade: &mut Fade) -> bool {
        if !self.waiting && self.count == 0 {
            *self = Self {
                waiting: true,
                wait: WAIT,
                shown: self.shown,
                ..Self::new()
            };
            fade.set(FADE_BLACK, 0);
        }
        if self.waiting && self.wait != 0 {
            self.wait -= 1;
            return true;
        }
        if let Some(&(_, mask, on)) = TIMELINE.iter().find(|(at, ..)| *at == self.count) {
            for (n, shown) in self.shown.iter_mut().enumerate() {
                if mask >> n & 1 != 0 {
                    *shown = u8::from(on);
                }
            }
        }
        self.count = self.count.wrapping_add(1);
        self.waiting = false;
        false
    }

    /// The page's state (`0x995B40`'s switch) and its typing.
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
        let Some(&code) = LINES[index].get(self.column as usize) else {
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

    /// What the pass drew: the line being typed as far as it goes, then the
    /// page's lines before it, each at its y (at x [`LINE_X`]).
    pub fn lines(&self) -> Vec<(i32, &'static [u8])> {
        let mut lines = Vec::new();
        if !self.drawn {
            return lines;
        }
        if let Some((index, shown)) = self.typed {
            let text = LINES[usize::from(index)];
            lines.push((
                line_y(u32::from(index - self.page)),
                &text[..usize::from(shown)],
            ));
        }
        for n in 0..self.line {
            lines.push((line_y(n), LINES[usize::from(self.page) + n as usize]));
        }
        lines
    }

    /// The trace's `exe` (`scripts/oracle-re2.py`'s `ex_end_trace`), after
    /// the task's `+8` the caller gives.
    pub fn trace(&self, step: u8) -> String {
        let mask = self
            .shown
            .iter()
            .enumerate()
            .fold(0u32, |mask, (n, &shown)| mask | u32::from(shown & 1) << n);
        format!(
            "{step},{},{},{},{},{},{},{},{},{},{},{},{},{mask:x}",
            u8::from(self.waiting),
            self.count,
            self.wait,
            self.state,
            self.line,
            self.column,
            self.ticks,
            self.delay,
            self.level,
            self.add,
            u8::from(self.last),
            self.page,
        )
    }
}

fn line_y(line: u32) -> i32 {
    LINE_Y + line as i32 * LINE_STEP
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clear_fade() -> Fade {
        Fade {
            level: -1,
            add: 0,
            mode: 0,
        }
    }

    /// Passes of the task until it exits, with each pass's effects.
    fn run(ex_end: &mut ExEnd, fade: &mut Fade) -> Vec<Vec<Effect>> {
        let mut passes = Vec::new();
        for _ in 0..4000 {
            let effects = ex_end.tick(fade);
            let exit = effects.contains(&Effect::Exit);
            passes.push(effects);
            if exit {
                break;
            }
            // `System_trans`.
            if fade.level >= 0 {
                fade.level = fade.level.wrapping_add(fade.add);
            }
        }
        passes
    }

    #[test]
    fn the_first_pass_clears_fade_0_and_waits_0x5a() {
        let mut ex_end = ExEnd::new();
        let mut fade = clear_fade();
        assert!(ex_end.tick(&mut fade).is_empty());
        assert_eq!((fade.level, fade.mode), (0, FADE_BLACK));
        assert_eq!(ex_end.trace(1), "1,1,0,89,0,0,0,0,0,0,0,0,0,0");
        for _ in 0..89 {
            assert!(ex_end.tick(&mut fade).is_empty());
            assert!(ex_end.lines().is_empty());
        }
        // The pass the wait has run out counts and types the first letter.
        assert_eq!(ex_end.tick(&mut fade), [Effect::Cue(CUE_LETTER)]);
        assert_eq!(ex_end.trace(1), "1,0,1,0,1,0,0,1,0,0,0,0,0,0");
        assert_eq!(ex_end.lines(), [(LINE_Y, &LINES[0][..1])]);
    }

    /// The original's `exe` (`special_ex_leon_end`): from the first pass
    /// (4302) the first page fades from 5604, the second starts on 5636 with
    /// `FUN_004B7910` run twice that tick, and `Result_exit` is on 7024.
    #[test]
    fn both_pages_run_to_result_exit_as_the_original_does() {
        let mut ex_end = ExEnd::new();
        let mut fade = clear_fade();
        let passes = run(&mut ex_end, &mut fade);
        assert_eq!(passes.len(), 7024 - 4302 + 1);
        let mut ex_end = ExEnd::new();
        let mut fade = clear_fade();
        for _ in 4302..=5604 {
            ex_end.tick(&mut fade);
        }
        assert_eq!(ex_end.trace(1), "1,0,1213,0,5,11,11,0,157,0,1024,0,0,0");
        for _ in 5605..=5636 {
            ex_end.tick(&mut fade);
        }
        assert_eq!(ex_end.trace(1), "1,0,1246,0,1,0,0,1,0,0,0,1,11,0");
    }

    #[test]
    fn the_cues_are_a_letters_a_spaces_and_a_lines_end() {
        let mut ex_end = ExEnd::new();
        let mut fade = clear_fade();
        let cues: Vec<u8> = run(&mut ex_end, &mut fade)
            .into_iter()
            .flatten()
            .filter_map(|effect| match effect {
                Effect::Cue(slot) => Some(slot),
                Effect::Exit => None,
            })
            .collect();
        let letters = LINES
            .iter()
            .flat_map(|line| line.iter())
            .filter(|&&code| code != 0)
            .count();
        let spaces = LINES
            .iter()
            .flat_map(|line| line.iter())
            .filter(|&&code| code == 0)
            .count();
        assert_eq!(
            cues.iter().filter(|&&cue| cue == CUE_LETTER).count(),
            letters
        );
        assert_eq!(cues.iter().filter(|&&cue| cue == CUE_SPACE).count(), spaces);
        // Every line's end but each page's last.
        assert_eq!(cues.iter().filter(|&&cue| cue == CUE_LINE).count(), 20);
    }

    #[test]
    fn the_timeline_shows_its_sprites_in_pairs() {
        let mut ex_end = ExEnd::new();
        let mut fade = clear_fade();
        // The wait (90 passes), then counts 0 to 0x92.
        for _ in 0..90 + 0x93 {
            ex_end.tick(&mut fade);
        }
        assert_eq!(ex_end.shown[..4], [1, 1, 0, 0]);
        for _ in 0x93..=0x11E {
            ex_end.tick(&mut fade);
        }
        assert_eq!(ex_end.shown[..2], [0, 0]);
    }

    #[test]
    fn the_finished_lines_stay_under_the_one_being_typed() {
        let mut ex_end = ExEnd::new();
        let mut fade = clear_fade();
        // To 4700, the fourth line's first letter.
        for _ in 4302..=4700 {
            ex_end.tick(&mut fade);
        }
        let lines = ex_end.lines();
        assert_eq!(lines[0], (LINE_Y + 3 * LINE_STEP, &LINES[3][..1]));
        assert_eq!(
            lines[1..],
            [
                (LINE_Y, LINES[0]),
                (LINE_Y + LINE_STEP, LINES[1]),
                (LINE_Y + 2 * LINE_STEP, LINES[2])
            ]
        );
    }
}
