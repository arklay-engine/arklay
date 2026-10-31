//! RE2's game over: the `Die` task (`Die_demo_move`'s phases).
//!
//! The white-out, the strobing stops and the sequence's length are the
//! original's. `Die_move_set_cut`'s spiralling camera, the room's lights and the
//! `Die_spr_*` sprites aren't ported: see `docs/re2-deviations.md`.

use crate::scd2::host::Re2State;

/// `fg_stop`'s top byte when every routine is stopped.
const STOP_ALL: u32 = 0xFF00_0000;
/// The bits `Die_move_wait` flips to let one tick in three through.
const STOP_STROBE: u32 = 0xFC00_0000;
/// The stops the white-out leaves on once it's over.
const STOP_AFTER: u32 = 0x4000_0000;
/// `Die_move_fade_out`'s wait for a player killed by a zombie, `PTR_DAT_00525b18`.
const HELD_WAIT: i16 = 95;
/// `Die_move_pause`'s length, the zombie row of `DWORD_00525b38`.
const PAUSE: i16 = 91;
const MAIN: i16 = 0xAA;
const OUT_FADE: i16 = 0x6E;
const OUT: i16 = 0xA2;
const LEVEL_FULL: u16 = 0x7FFF;

/// A change to `Fade_set`'s first slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DieFade {
    /// `Fade_adjust`: the white's level.
    White(i16),
    /// `Fade_set(0x100, -0xBBC)`: the white clears.
    WhiteClear,
    /// `Fade_set(0x200, 0x200)`: to black.
    Black,
}

/// What [`Die::tick`] asks of the rest of the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DieTick {
    pub fade: Option<DieFade>,
    /// `Snd_bgm_ctr`: the music is cut.
    pub stop_music: bool,
    /// `Die_move_end`: the task has exited.
    pub finished: bool,
}

/// The `Die` task's variables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Die {
    /// `WORD_0099cee2`.
    phase: u8,
    /// `fg_system`'s `0x100`: killed in a zombie's hands.
    held: bool,
    /// `WORD_0099cee4`.
    count: i16,
    /// `WORD_0099cee0`.
    speed: i16,
    /// `WORD_0099cee6`.
    level: u16,
    /// `BYTE_0099cf14`.
    strobe: u8,
    /// `WORD_0099cf18`.
    wait: i16,
    /// The ticks `Die_move_wait` sleeps after its last white, the load's.
    sleep: u8,
    /// `pCtcb + 9`: the white-out is over.
    white_done: bool,
}

impl Die {
    pub fn new(held: bool) -> Self {
        Self {
            phase: 0,
            held,
            count: 0,
            speed: 0,
            level: 0x400,
            strobe: 0,
            wait: if held { HELD_WAIT } else { 0 },
            sleep: 0,
            white_done: false,
        }
    }

    /// One tick of `Die`, run after the tick's routines.
    pub fn tick(&mut self, re2: &mut Re2State) -> DieTick {
        let mut out = DieTick::default();
        if self.sleep > 0 {
            self.sleep -= 1;
            return out;
        }
        match self.phase {
            // `Die_move_fade_out`.
            0 => {
                *re2.fg_stop_mut() |= 0x0200_0000;
                self.phase = if self.held { 1 } else { 2 };
            }
            // `Die_move_timer`.
            1 => {
                self.wait -= 1;
                if self.wait < 0 {
                    self.phase = 2;
                }
            }
            2 => self.wait_white(re2, &mut out),
            // `Die_move_pause`.
            3 => {
                self.count += 1;
                if self.count > PAUSE {
                    self.phase = 4;
                    self.count = 0;
                }
            }
            // `Die_move_main`.
            4 => {
                self.count += 1;
                if self.count == MAIN {
                    self.phase = 5;
                    self.count = 0;
                }
            }
            // `Die_move_out`.
            5 => {
                self.count += 1;
                if self.count == OUT_FADE {
                    out.fade = Some(DieFade::Black);
                }
                if self.count > OUT {
                    self.phase = 6;
                }
            }
            // `Die_move_end`.
            _ => out.finished = true,
        }
        out
    }

    /// `Die_move_wait`.
    fn wait_white(&mut self, re2: &mut Re2State, out: &mut DieTick) {
        if self.white_done {
            self.count = 0;
            self.phase = if self.held { 3 } else { 4 };
            let stop = re2.fg_stop_mut();
            *stop = (*stop & 0x03FF_FFFF) | STOP_AFTER;
            out.fade = Some(DieFade::WhiteClear);
            return;
        }
        if self.level < LEVEL_FULL {
            self.count = self.count.wrapping_add(self.speed);
            self.speed += if self.held { 0x18 } else { 2 };
            self.strobe += 1;
            let stop = re2.fg_stop_mut();
            if self.strobe < 3 {
                *stop |= STOP_ALL;
            } else {
                *stop ^= STOP_STROBE;
                self.strobe = 0;
                out.stop_music = true;
            }
            self.level = self.level.wrapping_add(self.count as u16);
            if self.level >= LEVEL_FULL {
                self.level = LEVEL_FULL;
            }
            out.fade = Some(DieFade::White(self.level as i16));
            return;
        }
        // `Die_sprt_load`, `Bg_set_mode`, the cut and the lights go here.
        self.strobe = 0;
        self.white_done = true;
        self.sleep = 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ticks the original's `fg_stop` changes on after a zombie's kill,
    /// counted from the kill.
    #[test]
    fn a_held_death_stops_the_routines_when_the_original_does() {
        let mut re2 = Re2State::default();
        let mut die = Die::new(true);
        let mut changes = Vec::new();
        let mut last = 0;
        let mut end = None;
        for tick in 0..1000 {
            let out = die.tick(&mut re2);
            let stop = *re2.fg_stop_mut();
            if stop != last {
                changes.push((tick, stop));
                last = stop;
            }
            if out.finished {
                end = Some(tick);
                break;
            }
        }
        assert_eq!(changes[0], (0, 0x0200_0000));
        assert_eq!(changes[1], (97, 0xFF00_0000));
        assert_eq!(changes[2], (99, 0x0300_0000));
        assert_eq!(changes[3], (100, 0xFF00_0000));
        assert_eq!(*changes.last().unwrap(), (119, 0x4300_0000));
        assert_eq!(end, Some(545));
    }

    #[test]
    fn the_white_climbs_to_full_then_clears_and_ends_in_black() {
        let mut re2 = Re2State::default();
        let mut die = Die::new(true);
        let mut whites = Vec::new();
        let mut others = Vec::new();
        for tick in 0..546 {
            match die.tick(&mut re2).fade {
                Some(DieFade::White(level)) => whites.push(level),
                Some(other) => others.push((tick, other)),
                None => {}
            }
        }
        assert_eq!(whites.len(), 20);
        assert!(whites.windows(2).all(|pair| pair[0] <= pair[1]));
        assert_eq!(*whites.last().unwrap(), 0x7FFF);
        assert_eq!(others, [(119, DieFade::WhiteClear), (491, DieFade::Black)]);
    }

    #[test]
    fn a_death_outside_a_zombies_hands_has_no_wait_or_pause() {
        let mut re2 = Re2State::default();
        let mut die = Die::new(false);
        die.tick(&mut re2);
        die.tick(&mut re2);
        assert_eq!(*re2.fg_stop_mut(), 0xFF00_0000);
    }
}
