//! RE 2's status screen opened straight onto the floor's map by the PC's
//! map key: the status task's mode 3 (`St_tbl`'s fourth row, `0x5402E4`:
//! `St_init03` at `0x4FB5E0`, `St_main03` at `0x4FB7D0`, and the shared exit
//! `0x501EA0`).
//!
//! `Game_loop` (`0x4C0379`) takes the key (`Key_trg & 0x4000`, the raw pad's
//! `0x10`) where it takes START, under the same gate: it puts 3 in the
//! status task's mode byte (`0x691F70`) and leaves for game mode 1 with the
//! decide cue, as START does. With both keys on one tick the map wins.
//!
//! | tick | what |
//! |---|---|
//! | the open's 1 to 5 | `Game_loop`'s modes 1 and 2, as for START |
//! | 6 | `Status`'s start, then `St_init03`'s step 0: the player's floor, the map's pictures, the fade in set, `St` 1 and the page's routine 2; no draw |
//! | 7 on | `St_main03`: the page's routine (`0x53DB1C`), then the map while the sub is 0, then the backdrop |
//!
//! `St_init03` sets `St` in its step 0, so its step 1 (which would wait
//! for the fade in and play cue 9) never runs: the map takes keys under
//! the fade in, and no cue plays.
//!
//! Routine 2 (`St_menu1_map`, `0x4FB380`) is the MAP page's shown map with
//! its own subs: sub 0 steps the blink, changes the floor on a repeat tick
//! with up or down held (the cursor cue, sub 1, and nothing else that
//! tick), and otherwise closes on START, the cancel key or the map key
//! (the cancel cue, routine 0). Sub 1 loads the floor's map, resets the
//! blink and goes back to sub 0, and that tick draws the map again, so a
//! floor change leaves the map out for one frame, where the MAP page
//! leaves it out for two. Routines 0 and 1 are the close's
//! (`St_fade_out_set`, `St_fade_out_wait`); the map is drawn under the
//! fade out with the blink held.

use super::super::GameState;
use super::Sound;
use super::files::Blink;
use super::map::{self, MapView};

/// The page's routines (`0x53DB1C`): the close's two and the map.
pub const ROUTINE_FADE_SET: u8 = 0;
pub const ROUTINE_FADE_WAIT: u8 = 1;
pub const ROUTINE_MAP: u8 = 2;

/// Routine 2's subs (`0x691F62`): the map shown, and a floor's load.
pub const SUB_SHOWN: u8 = 0;
pub const SUB_FLOOR: u8 = 1;

/// The map key's map while its screen is up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyMap {
    /// The page's routine (`0x691F61`).
    pub routine: u8,
    pub sub: u8,
    /// `St_init03`'s step 0 has run: `St` is 1 and `St_main03` runs.
    pub started: bool,
    /// What the last tick drew.
    pub drawn: Drawn,
    /// `St_exit` has run: the mode byte is 0 again.
    pub exited: bool,
}

/// What a tick of the screen draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drawn {
    /// Before `St_main03` first runs.
    Nothing,
    /// The backdrop alone: the tick of a floor change.
    Backdrop,
    /// The map over the backdrop.
    Map,
}

impl Default for KeyMap {
    fn default() -> Self {
        Self {
            routine: ROUTINE_MAP,
            sub: SUB_SHOWN,
            started: false,
            drawn: Drawn::Nothing,
            exited: false,
        }
    }
}

/// The keys routine 2 reads on a tick.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyMapInput {
    /// Up and down, gated on the key repeat's flag.
    pub up: bool,
    pub down: bool,
    /// A press of START, the cancel key or the map key.
    pub close: bool,
}

/// What a tick of the map asks of the engine.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeyMapOutcome {
    pub sounds: Vec<Sound>,
    /// The page has gone to routine 0: the close starts.
    pub close: bool,
}

impl GameState {
    /// The map key in play: the status screen opens in mode 3 where START
    /// would open it.
    pub fn request_re2_key_map(&mut self) {
        self.re2_key_map = Some(KeyMap::default());
    }

    /// `St_init03`'s step 0, on the open's sixth tick after `Status`'s
    /// start: the page's routine and the player's floor. It draws nothing.
    pub fn start_re2_key_map(&mut self) {
        let floor = self.re2_player_floor();
        let Some(map) = self.re2_key_map.as_mut() else {
            return;
        };
        map.started = true;
        map.routine = ROUTINE_MAP;
        map.sub = SUB_SHOWN;
        self.re2_file.map_floor = floor;
        self.trace_re2_key_map();
    }

    /// One tick of the screen after its start (`St_main03`): the page's
    /// routine, then the draw. `None` while no map key's map is up.
    pub fn tick_re2_key_map(&mut self, input: KeyMapInput) -> Option<KeyMapOutcome> {
        let mut map = self.re2_key_map?;
        let mut out = KeyMapOutcome::default();
        if !map.started {
            return Some(out);
        }
        match map.routine {
            ROUTINE_FADE_SET => map.routine = ROUTINE_FADE_WAIT,
            ROUTINE_FADE_WAIT => {}
            _ => self.step_key_map(&mut map, input, &mut out),
        }
        map.drawn = if map.sub == SUB_SHOWN {
            Drawn::Map
        } else {
            Drawn::Backdrop
        };
        self.re2_key_map = Some(map);
        self.trace_re2_key_map();
        Some(out)
    }

    /// `St_menu1_map` (`0x4FB380`).
    fn step_key_map(&mut self, map: &mut KeyMap, input: KeyMapInput, out: &mut KeyMapOutcome) {
        let work = &mut self.re2_file;
        if map.sub != SUB_SHOWN {
            work.blink = Blink::RESET;
            map.sub = SUB_SHOWN;
            return;
        }
        work.blink.step_map();
        let re2 = &self.re2;
        let floor = map::floor_after(work.map_floor, input.up, input.down, |bank, index| {
            re2.flag(bank, index)
        });
        if floor != work.map_floor {
            // The change returns before the close's test.
            work.map_floor = floor;
            map.sub = SUB_FLOOR;
            out.sounds.push(Sound::Cursor);
            return;
        }
        if input.close {
            out.sounds.push(Sound::Cancel);
            map.routine = ROUTINE_FADE_SET;
            out.close = true;
        }
    }

    /// `St_exit` (`0x501EA0`) on the close's eighth tick: the mode byte is
    /// cleared, and the screen draws no more.
    pub fn exit_re2_key_map(&mut self) {
        if let Some(map) = self.re2_key_map.as_mut() {
            map.exited = true;
            map.drawn = Drawn::Nothing;
        }
    }

    /// The trace's `ip` while the map key's map is up: `Status`'s start
    /// clears the work block, and only the sub moves.
    fn trace_re2_key_map(&mut self) {
        if let Some(map) = self.re2_key_map {
            self.re2_items_trace = format!("{},0,0,0,0,0,0,0", map.sub);
        }
    }

    /// What the screen draws (`St_main03`): `None` on a tick that draws
    /// nothing, `Some(None)` for the backdrop alone, else the floor's map
    /// as the MAP page draws it (`St_disp_map`).
    pub fn re2_key_map_view(&self) -> Option<Option<MapView>> {
        match self.re2_key_map?.drawn {
            Drawn::Nothing => None,
            Drawn::Backdrop => Some(None),
            Drawn::Map => Some(Some(self.re2_map_view())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{RoomId, RoomState};

    const NONE: KeyMapInput = KeyMapInput {
        up: false,
        down: false,
        close: false,
    };
    const UP: KeyMapInput = KeyMapInput { up: true, ..NONE };
    const DOWN: KeyMapInput = KeyMapInput { down: true, ..NONE };
    const CLOSE: KeyMapInput = KeyMapInput {
        close: true,
        ..NONE
    };

    /// Room 201 (floor 2) with floors 3 and 4 stood on, the map key's map
    /// requested and the status screen started, as `map_key_201` has it on
    /// tick 406.
    fn started() -> GameState {
        let mut game = GameState::new(RoomId::parse("2010").unwrap(), &RoomState::default());
        game.is_re2_room = true;
        game.re2_room_set();
        for floor in [3, 4] {
            game.re2.set_flag(map::FLOOR_BANK, floor, 1);
        }
        game.request_re2_key_map();
        game.init_re2_status();
        game.start_re2_key_map();
        game
    }

    fn tick(game: &mut GameState, input: KeyMapInput) -> KeyMapOutcome {
        game.tick_re2_key_map(input).unwrap()
    }

    /// `map_key_201`: the map key's tick shows the floor and the blink as
    /// they were left, and the start sets the player's floor and the
    /// screen's blink.
    #[test]
    fn the_start_takes_the_players_floor() {
        let mut game = GameState::new(RoomId::parse("2010").unwrap(), &RoomState::default());
        game.is_re2_room = true;
        assert_eq!(game.re2_map_trace(), "-");
        game.request_re2_key_map();
        assert_eq!(game.re2_map_trace(), "0,0,0");
        // Before the start a tick does nothing and draws nothing.
        assert_eq!(tick(&mut game, CLOSE), KeyMapOutcome::default());
        assert_eq!(game.re2_key_map_view(), None);
        game.init_re2_status();
        game.start_re2_key_map();
        assert_eq!(game.re2_map_trace(), "2,50,0");
        assert_eq!(game.re2_items_trace, "0,0,0,0,0,0,0,0");
        // The start's tick draws nothing; the first tick of `St_main03`
        // steps the blink and draws the map.
        assert_eq!(game.re2_key_map_view(), None);
        tick(&mut game, NONE);
        assert_eq!(game.re2_map_trace(), "2,52,0");
        assert!(matches!(game.re2_key_map_view(), Some(Some(_))));
    }

    /// Ticks 460 and 461: up changes to floor 3 with the cursor cue and
    /// sub 1, drawing only the backdrop, and the next tick loads it and
    /// draws it.
    #[test]
    fn a_floor_change_leaves_the_map_out_for_one_tick() {
        let mut game = started();
        tick(&mut game, NONE);
        let out = tick(&mut game, UP);
        assert_eq!(out.sounds, vec![Sound::Cursor]);
        assert_eq!(game.re2_map_trace(), "3,54,0");
        assert_eq!(game.re2_items_trace, "1,0,0,0,0,0,0,0");
        assert_eq!(game.re2_key_map_view(), Some(None));
        // The key still held does nothing on the load's tick.
        let out = tick(&mut game, UP);
        assert!(out.sounds.is_empty());
        assert_eq!(game.re2_map_trace(), "3,2,0");
        assert_eq!(game.re2_items_trace, "0,0,0,0,0,0,0,0");
        let view = game.re2_key_map_view().unwrap().unwrap();
        assert_eq!((view.floor, view.map), (3, 2));
        assert!(view.marker.is_none());
        assert_eq!(view.arrows.len(), 2);
        tick(&mut game, DOWN);
        tick(&mut game, NONE);
        assert_eq!(game.re2_file.map_floor, 2);
    }

    /// The floor change returns before the close's test: a close on its
    /// tick is lost.
    #[test]
    fn a_floor_change_wins_over_a_close() {
        let mut game = started();
        let out = tick(&mut game, KeyMapInput { close: true, ..UP });
        assert_eq!(out.sounds, vec![Sound::Cursor]);
        assert!(!out.close);
        assert_eq!(game.re2_key_map.unwrap().routine, ROUTINE_MAP);
    }

    /// Ticks 620 to 628: the close's press steps the blink and plays the
    /// cancel cue; the close's ticks hold the blink and still draw the
    /// map, until `St_exit` clears the mode.
    #[test]
    fn a_close_holds_the_blink_and_draws_the_map_to_the_exit() {
        let mut game = started();
        tick(&mut game, NONE);
        let out = tick(&mut game, CLOSE);
        assert!(out.close);
        assert_eq!(out.sounds, vec![Sound::Cancel]);
        assert_eq!(game.re2_map_trace(), "2,54,0");
        assert_eq!(game.re2_key_map.unwrap().routine, ROUTINE_FADE_SET);
        for routine in [ROUTINE_FADE_WAIT, ROUTINE_FADE_WAIT] {
            // The keys do nothing any more.
            let out = tick(&mut game, UP);
            assert_eq!(out, KeyMapOutcome::default());
            assert_eq!(game.re2_key_map.unwrap().routine, routine);
            assert_eq!(game.re2_map_trace(), "2,54,0");
            assert!(matches!(game.re2_key_map_view(), Some(Some(_))));
        }
        game.exit_re2_key_map();
        assert_eq!(game.re2_map_trace(), "-");
        assert_eq!(game.re2_key_map_view(), None);
    }
}
