//! RE 2's key repeat: the one repeat counter `Pad_set` (`0x4D0F30`) keeps for
//! every key of a mask, which `Pad_rep_set` (`0x4D1150`) sets with two
//! delays. Its flag (`fg_system`'s bit 31) is up on a press and then on
//! each repeat, and the screens that gate a direction on it read
//! "flag and the key held" (`0x9885FC`).

/// The raw pad's bits (`0x9885FC`).
pub const UP: u16 = 0x1000;
pub const RIGHT: u16 = 0x2000;
pub const DOWN: u16 = 0x4000;
pub const LEFT: u16 = 0x8000;
/// The aim key.
pub const AIM: u16 = 0x0008;

/// The setting the title leaves for play (`Title`, `0x5035B0`): the up,
/// right, down and left keys, a first delay of 10 and a period of 2.
pub const TITLE_SETTING: (u16, u16) = (0x5000, 0x020A);
/// The setting each `Status` start gives (`0x501BD0`): all four directions
/// and the aim key's neighbours, a first delay of 10 and a period of 6. The
/// flag is up on the press, 11 ticks after it, then every 7.
pub const STATUS_SETTING: (u16, u16) = (0xF01C, 0x060A);

/// `Pad_set`'s repeat state: the mask and delays (`0x98F074`, `0x98F078`),
/// the counter (`0x533938`), the flag and the pad it saw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PadRepeat {
    pub mask: u16,
    pub delays: u16,
    pub count: u8,
    pub flag: bool,
    /// The raw pad as of the last step.
    pub raw: u16,
    last_raw: u16,
}

impl Default for PadRepeat {
    /// The title's setting with the counter at 1, as the executable's data
    /// has it.
    fn default() -> Self {
        Self {
            mask: TITLE_SETTING.0,
            delays: TITLE_SETTING.1,
            count: 1,
            flag: false,
            raw: 0,
            last_raw: 0,
        }
    }
}

impl PadRepeat {
    /// `Pad_rep_set`: the mask and the delays. The counter is kept.
    pub fn set(&mut self, (mask, delays): (u16, u16)) {
        self.mask = mask;
        self.delays = delays;
    }

    /// One `Pad_set` (lines 71 to 92) with this tick's raw pad.
    pub fn step(&mut self, raw: u16) {
        let newly_down = raw & !self.last_raw;
        self.last_raw = raw;
        self.raw = raw;
        if newly_down & self.mask != 0 {
            self.count = self.delays as u8;
            self.flag = true;
        } else if self.count == 0 {
            // No test of the pad: the tick after the last held one still
            // repeats.
            self.count = (self.delays >> 8) as u8;
            self.flag = true;
        } else {
            if raw & self.mask != 0 {
                self.count -= 1;
            }
            self.flag = false;
        }
    }

    /// The flag with `bit` held: what a gated test reads.
    pub fn gated(&self, bit: u16) -> bool {
        self.flag && self.raw & bit != 0
    }

    /// The oracle's `prep` field: the flag, the counter, the mask and the
    /// delays.
    pub fn trace(&self) -> String {
        format!(
            "{},{},{},{}",
            u8::from(self.flag),
            self.count,
            self.mask,
            self.delays
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ticks of `0..ticks` the flag is up on, with `raw(tick)` the pad.
    fn flags(setting: (u16, u16), ticks: u32, raw: impl Fn(u32) -> u16) -> Vec<u32> {
        let mut pad = PadRepeat::default();
        pad.set(setting);
        (0..ticks)
            .filter(|&tick| {
                pad.step(raw(tick));
                pad.flag
            })
            .collect()
    }

    #[test]
    fn the_status_screens_setting_repeats_at_11_then_every_7() {
        assert_eq!(flags(STATUS_SETTING, 40, |_| UP), [0, 11, 18, 25, 32, 39]);
    }

    #[test]
    fn the_titles_setting_repeats_at_11_then_every_3() {
        assert_eq!(flags(TITLE_SETTING, 21, |_| UP), [0, 11, 14, 17, 20]);
    }

    #[test]
    fn a_release_keeps_the_count_and_a_new_press_reloads_it() {
        let held = |tick| {
            if (..5).contains(&tick) || tick >= 20 {
                UP
            } else {
                0
            }
        };
        assert_eq!(flags(STATUS_SETTING, 32, held), [0, 20, 31]);
    }

    #[test]
    fn the_count_0_branch_repeats_with_nothing_held() {
        // Held from tick 0 to 10: the count reaches 0 on tick 10.
        let mut pad = PadRepeat::default();
        pad.set(STATUS_SETTING);
        let mut up = Vec::new();
        for tick in 0..40 {
            pad.step(if tick <= 10 { UP } else { 0 });
            if pad.flag {
                up.push((tick, pad.count));
            }
        }
        assert_eq!(up, [(0, 10), (11, 6)]);
    }

    #[test]
    fn the_counter_is_shared_by_every_key() {
        let raw = |tick| if tick >= 7 { UP | RIGHT } else { UP };
        assert_eq!(flags(STATUS_SETTING, 20, raw), [0, 7, 18]);
    }

    #[test]
    fn the_aim_key_cycles_the_counter_under_the_status_setting() {
        assert_eq!(flags(STATUS_SETTING, 20, |_| AIM), [0, 11, 18]);
        // The title's mask has no aim bit.
        assert_eq!(flags(TITLE_SETTING, 20, |_| AIM), Vec::<u32>::new());
    }

    #[test]
    fn nothing_held_never_raises_the_flag() {
        let mut pad = PadRepeat::default();
        for _ in 0..10 {
            pad.step(0);
            assert!(!pad.flag);
        }
        assert_eq!(pad.count, 1);
    }

    #[test]
    fn set_keeps_the_count() {
        let mut pad = PadRepeat::default();
        pad.step(UP);
        pad.step(UP);
        let count = pad.count;
        pad.set(STATUS_SETTING);
        assert_eq!((pad.mask, pad.delays, pad.count), (0xF01C, 0x060A, count));
        assert_eq!(pad.trace(), format!("0,{count},61468,1546"));
    }
}
