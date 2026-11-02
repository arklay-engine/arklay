//! The player death sequence and the game-over ("YOU DIED") screen.
//!
//! A player whose health goes below zero raises the dead flag; the original's
//! main loop then runs a small game-over machine that waits 90 frames (the
//! fall), fades the screen to white, and opens the DIED screen, the pack's
//! `ui/death.lua`, which shows the fallen body over the `died.tim` static
//! backdrop with the sine-wavy "YOU DIED" strip, then fades back out and
//! returns to the title.
//!
//! # The game-over machine
//!
//! [`DeathSequence`] mirrors the original's death machine: state 0 triggers
//! (the Yawn rooms and the water tank skip the delay), state 1 counts the
//! 90-frame delay down, state 2 arms the white fade, state 3 waits for the
//! fade to wrap negative and then opens the screen. A room that is running
//! the attract demo or the scripted death variant skips the screen entirely.
//!
use crate::state::RoomId;
use crate::transition::{Fade, FadeColor, Overlay};

/// Frames the game-over machine waits before the fade in an ordinary room.
pub const DEATH_DELAY: u8 = 0x5A;
/// The attract reel-end fade's counter (type 2 black, `TimeoutDeathFadeOut`'s
/// attract branch).
const ATTRACT_FADE_OUT_COUNTER: i32 = 0x800;

/// The pre-screen fade counter (a 2-alpha-per-frame white-out).
const DEATH_FADE_OUT_COUNTER: i32 = 0x100;
/// The ticks between the clock running out and the accumulator's -1.
const EXPLOSION_DELAY: u8 = 2;
/// Stage digit of the second mansion floor (the Yawn rooms).
const STAGE_MANSION_2F: u8 = 2;
/// The attic, where the first Yawn's death skips the delay.
const ROOM_ATTIC: u8 = 0x10;
/// The lesson room, where the second Yawn's death (on the second visit)
/// skips the delay.
const ROOM_LESSON: u8 = 0x0C;
/// Stage digit of the guardhouse.
const STAGE_GUARDHOUSE: u8 = 4;
/// The water tank, where the Neptune death skips the delay.
const ROOM_WATER_TANK: u8 = 0x0E;
/// The armor room, whose scripted death skips the blood pool.
const ROOM_ARMOR: u8 = 0x05;
/// The drug storehouse, whose scripted death skips the blood pool.
const ROOM_DRUG_STOREHOUSE: u8 = 0x09;
/// Stage digit of the laboratory.
const STAGE_LABORATORY: u8 = 5;
/// The morgue, whose scripted death skips the blood pool.
const ROOM_MORGUE: u8 = 0x07;

/// Whether this room's death fades immediately instead of running the
/// 90-frame delay (the Yawn rooms and the Neptune water tank).
pub fn immediate_fade(id: RoomId) -> bool {
    (id.stage == STAGE_MANSION_2F && !id.mansion2 && id.room == ROOM_ATTIC)
        || (id.stage == STAGE_MANSION_2F && id.mansion2 && id.room == ROOM_LESSON)
        || (id.stage == STAGE_GUARDHOUSE && id.room == ROOM_WATER_TANK)
}

/// Whether this room's scripted death skips the blood pool and hands the
/// player straight to the blocked state (the armor room, the drug storehouse
/// and the morgue).
pub fn skips_blood_pool(id: RoomId) -> bool {
    (id.stage == STAGE_MANSION_2F && !id.mansion2 && id.room == ROOM_ARMOR)
        || (id.stage == STAGE_GUARDHOUSE && id.room == ROOM_DRUG_STOREHOUSE)
        || (id.stage == STAGE_LABORATORY && id.room == ROOM_MORGUE)
}

/// What one game-over tick produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeathTick {
    /// Keep running the delay or the fade.
    None,
    /// The fade completed; the DIED screen should open.
    OpenScreen,
    /// The variant path skipped the screen; the title should return.
    Finished,
}

/// The game-over machine (the original's death-state counter, delay and fade).
#[derive(Debug, Clone)]
pub struct DeathSequence {
    /// 0 idle, 1 delay, 2 arm the fade, 3 fading, 4 screen, 5 done, 6-7 the
    /// explosion's wait.
    state: u8,
    /// The delay countdown.
    delay: u8,
    /// The attract/demo or scripted death variant: no DIED screen.
    variant: bool,
    /// The death fade (white to the screen, black out of it).
    fade: Fade,
    /// The sequence ended; the caller should return to the title.
    pub finished: bool,
    /// The explosion's standalone screen: nothing of the room shows.
    blank: bool,
}

impl Default for DeathSequence {
    fn default() -> Self {
        Self::idle()
    }
}

impl DeathSequence {
    /// A sequence that has not been triggered.
    pub const fn idle() -> Self {
        Self {
            state: 0,
            delay: 0,
            variant: false,
            fade: Fade::inactive(),
            finished: false,
            blank: false,
        }
    }

    /// Whether the sequence has not been triggered yet.
    pub fn is_idle(&self) -> bool {
        self.state == 0
    }

    /// Trigger the sequence: an immediate-fade room arms the white fade at
    /// once, every other room starts the 90-frame delay. `variant` is the
    /// scripted-death flag (the escape's game completion), which arms a black
    /// fade at once and skips the screen; `attract` is the
    /// attract-reel end, whose `TimeoutDeathFadeOut` branch arms a black fade
    /// with the `0x800` counter.
    ///
    /// The original's case 0 falls straight through into case 1 in the same
    /// tick, so the freshly-armed delay is decremented once here; the arm tick
    /// also applies the fade's first accumulator step (`main_loop`'s
    /// draw-then-add runs in the same frame the machine arms it).
    pub fn start(&mut self, immediate: bool, variant: bool, attract: bool) {
        self.variant = variant || attract;
        if attract {
            self.arm_fade(2, ATTRACT_FADE_OUT_COUNTER);
            self.fade.tick();
            self.state = 3;
        } else if variant {
            // `TimeoutDeathFadeOut`'s scripted-variant branch: the black fade.
            self.arm_fade(2, DEATH_FADE_OUT_COUNTER);
            self.fade.tick();
            self.state = 3;
        } else if immediate {
            self.arm_fade(1, DEATH_FADE_OUT_COUNTER);
            self.fade.tick();
            self.state = 3;
        } else {
            self.delay = DEATH_DELAY - 1;
            self.state = 1;
        }
    }

    /// `TimeoutDeathFadeOut`'s explosion branch: no fade. The task sleeps a
    /// frame, parks the accumulator at -1 and the screen goes standalone, so
    /// the room is gone until the DIED screen opens. `counter` and `state` are
    /// the room fade's, which the original's single accumulator still holds.
    pub fn start_explosion(&mut self, counter: i32, state: i16) {
        self.fade.set(2, counter, state);
        self.delay = EXPLOSION_DELAY;
        self.state = 6;
    }

    /// `die_state`'s frame after the screen: no sprites queued, so black.
    /// `counter` and `state` are the screen's last, which the original's single
    /// accumulator still holds.
    pub fn end_screen(&mut self, counter: i32, state: i16) {
        self.fade.set(2, counter, state);
        self.blank = true;
    }

    /// Whether the death machine has armed the single accumulator; until then
    /// it still holds the room fade's values.
    pub fn owns_fade(&self) -> bool {
        self.state >= 3
    }

    /// Whether the explosion's wait owns the tick.
    pub fn is_explosion(&self) -> bool {
        matches!(self.state, 6 | 7)
    }

    /// Whether the explosion's wait ends this tick.
    pub fn explosion_ends(&self) -> bool {
        self.state == 7
    }

    /// Advance the machine one tick (before the screen opens).
    pub fn tick(&mut self) -> DeathTick {
        match self.state {
            1 => {
                if self.delay == 0 {
                    self.arm_fade(1, DEATH_FADE_OUT_COUNTER);
                    self.state = 3;
                    // The original's die_state fade step runs in the same
                    // frame the countdown expires.
                    self.fade.tick();
                } else {
                    self.delay -= 1;
                    return DeathTick::None;
                }
            }
            2 => {
                self.arm_fade(1, DEATH_FADE_OUT_COUNTER);
                self.state = 3;
            }
            // The machine reads the accumulator before `main_loop` steps it,
            // so the frame after the fade wraps still runs the room.
            3 => {
                if self.fade.state() < 0 {
                    self.state = 4;
                    if self.variant {
                        self.finished = true;
                        return DeathTick::Finished;
                    }
                    return DeathTick::OpenScreen;
                }
                self.fade.tick();
            }
            6 => {
                self.blank = true;
                self.delay -= 1;
                if self.delay == 0 {
                    self.fade.set_state(-1);
                    self.state = 7;
                }
            }
            7 => {
                self.blank = false;
                self.fade = Fade::inactive();
                self.state = 4;
                return DeathTick::OpenScreen;
            }
            _ => {}
        }
        DeathTick::None
    }

    /// Arm a fade the way the original's `fade_update` does: a positive
    /// counter ramps up from zero, a negative one decays from `0x7FFF`.
    fn arm_fade(&mut self, fade_type: u8, counter: i32) {
        let state = if counter <= 0 { 0x7FFF } else { 0 };
        self.fade.set(fade_type, counter, state);
    }

    /// Arm the white death fade now, skipping the rest of the 90-frame delay.
    /// A scripted death (the hunter pounce's `player_anim_dispatch_4c2ac8`)
    /// calls the original's `fade_update` itself at its own countdown; a fade
    /// already running makes this a no-op, exactly like the original's
    /// `g_fading_state <= 0` guard.
    pub fn arm_fade_now(&mut self) {
        if self.state == 1 || self.state == 2 {
            self.arm_fade(1, DEATH_FADE_OUT_COUNTER);
            self.state = 3;
        }
    }

    /// The full-screen overlay the death fade draws, if any.
    pub fn overlay(&self) -> Option<Overlay> {
        self.fade.overlay()
    }

    /// The full-screen overlay the death fade *draws* this frame: the original
    /// draws with the accumulator's pre-step value (see
    /// [`Fade::drawn_overlay`]).
    pub fn drawn_overlay(&self) -> Option<Overlay> {
        if self.blank {
            return Some(Overlay {
                alpha: 255,
                color: FadeColor::from_id(2),
            });
        }
        // The explosion's parked accumulator is not a fade.
        if self.state == 6 {
            return None;
        }
        self.fade.drawn_overlay()
    }

    /// The death fade's accumulator step (the original's `g_fading_counter`
    /// while the death machine owns the screen).
    pub fn fade_counter(&self) -> i32 {
        self.fade.counter()
    }

    /// The death fade's accumulator (the original's `g_fading_state`).
    pub fn fade_state(&self) -> i16 {
        self.fade.state()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Room `text` in the original's numbering: 6xx and 7xx are the second
    /// mansion visit.
    fn room(text: &str) -> RoomId {
        RoomId::from_original(u16::from_str_radix(&format!("{text}0"), 16).unwrap()).unwrap()
    }

    #[test]
    fn the_explosion_blanks_the_room_and_opens_the_screen_without_a_fade() {
        let mut death = DeathSequence::idle();
        death.start_explosion(-6144, -4097);
        assert_eq!(death.fade_counter(), -6144, "the room fade's accumulator");
        assert_eq!(
            death.drawn_overlay(),
            None,
            "the room shows on the tick it ends"
        );
        assert_eq!(death.tick(), DeathTick::None);
        let black = death.drawn_overlay().expect("the screen went standalone");
        assert_eq!((black.alpha, black.color), (255, FadeColor::from_id(2)));
        assert_eq!(death.fade_state(), -4097);
        assert_eq!(death.tick(), DeathTick::None);
        assert_eq!(death.fade_state(), -1);
        assert_eq!(death.tick(), DeathTick::OpenScreen);
        assert_eq!(death.drawn_overlay(), None);
    }

    #[test]
    fn the_frame_after_the_screen_is_black_and_keeps_the_screen_accumulator() {
        let mut death = DeathSequence::idle();
        death.start(true, false, false);
        death.end_screen(0x180, -32512);
        let black = death.drawn_overlay().expect("no sprites are queued");
        assert_eq!((black.alpha, black.color), (255, FadeColor::from_id(2)));
        assert_eq!((death.fade_counter(), death.fade_state()), (0x180, -32512));
    }

    #[test]
    fn the_machine_holds_the_room_fade_until_it_arms_its_own() {
        let mut death = DeathSequence::idle();
        assert!(!death.owns_fade());
        death.start(false, false, false);
        assert!(!death.owns_fade(), "the fall still shows the room fade");
        for _ in 0..DEATH_DELAY {
            death.tick();
        }
        assert!(death.owns_fade());
    }

    #[test]
    fn the_room_variants_dispatch_by_room() {
        // The Yawn rooms and the water tank fade immediately.
        assert!(immediate_fade(room("210")));
        assert!(immediate_fade(room("70C")));
        assert!(immediate_fade(room("40E")));
        assert!(!immediate_fade(room("100")));
        assert!(!immediate_fade(room("205")));
        // The scripted deaths skip the blood pool.
        assert!(skips_blood_pool(room("205")));
        assert!(skips_blood_pool(room("409")));
        assert!(skips_blood_pool(room("507")));
        assert!(!skips_blood_pool(room("210")));
        assert!(!skips_blood_pool(room("100")));
    }

    #[test]
    fn an_ordinary_death_waits_ninety_frames_then_fades() {
        let mut sequence = DeathSequence::idle();
        sequence.start(false, false, false);
        // The original's case 0 falls through into case 1 and decrements the
        // freshly-armed delay once in the same tick.
        assert_eq!(sequence.delay, DEATH_DELAY - 1);
        // The 89 remaining delay ticks produce nothing and leave the fade
        // inactive.
        for _ in 0..DEATH_DELAY - 1 {
            assert_eq!(sequence.tick(), DeathTick::None);
            assert!(sequence.overlay().is_none());
        }
        // The 90th tick arms the white fade and applies its first step.
        assert_eq!(sequence.tick(), DeathTick::None);
        let overlay = sequence.overlay().unwrap();
        assert_eq!(overlay.color, FadeColor::White);
        assert_eq!(overlay.alpha, 2);
        // 128 adds of 0x100 reach the alpha ceiling, then the accumulator
        // wraps negative and the screen opens on the frame after. The arm tick
        // already applied the first step, so 127 more steps follow.
        let mut opened = None;
        for tick in 0..200 {
            if sequence.tick() == DeathTick::OpenScreen {
                opened = Some(tick);
                break;
            }
        }
        let opened = opened.expect("the fade completes");
        assert_eq!(opened, 127);
    }

    #[test]
    fn an_immediate_room_arms_the_fade_without_the_delay() {
        let mut sequence = DeathSequence::idle();
        sequence.start(true, false, false);
        let overlay = sequence.overlay().unwrap();
        assert_eq!(overlay.color, FadeColor::White);
        assert_eq!(overlay.alpha, 2, "the arm tick applies the first step");
    }

    #[test]
    fn a_scripted_death_arms_the_fade_ahead_of_the_delay() {
        // A normal room starts the 90-frame delay; the request skips it.
        let mut sequence = DeathSequence::idle();
        sequence.start(false, false, false);
        assert_eq!(sequence.delay, DEATH_DELAY - 1);
        assert!(sequence.overlay().is_none());
        sequence.arm_fade_now();
        let overlay = sequence.overlay().unwrap();
        assert_eq!(overlay.color, FadeColor::White);
        assert_eq!(overlay.alpha, 0);
        assert_eq!(sequence.state, 3);
        // The fade advances on the next tick (the delay is gone).
        assert_eq!(sequence.tick(), DeathTick::None);
        assert_eq!(sequence.overlay().unwrap().alpha, 2);
        // A running fade is not re-armed.
        sequence.arm_fade_now();
        assert_eq!(sequence.overlay().unwrap().alpha, 2);
    }

    #[test]
    fn the_variant_path_skips_the_screen() {
        let mut sequence = DeathSequence::idle();
        sequence.start(false, true, false);
        assert_eq!(
            sequence.overlay().unwrap().color,
            crate::transition::FadeColor::Black,
            "the variant fades to black"
        );
        for _ in 0..300 {
            if sequence.tick() == DeathTick::Finished {
                break;
            }
        }
        assert!(sequence.finished);
    }
}
