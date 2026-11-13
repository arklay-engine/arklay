//! RE 2's status screen opened straight onto a floor's map by a script
//! (`Sce_84`, opcode 0x84, `0x4E8FE0`): the status task's mode 4
//! (`St_tbl`'s fifth row, `0x5402F0`: `St_init04` at `0x4FBC70`,
//! `St_main04` at `0x4FBE90`, and the shared exit `0x501EA0`).
//!
//! `Sce_84` does what `Sce_Status` does (`fg_stop` saved, `fg_status`'s
//! `0x8000`, `Game` 1), with the item box byte (`0x691F70`) at 4 and its
//! operand at `0x692D62`. Room 301's talk with Ben and Ada ends with
//! `sce_84(2)`, which shows POLICE STATION B1 with the sewer's room lit.
//!
//! | tick | what |
//! |---|---|
//! | the open's 1 to 5 | `Game_loop`'s modes 1 and 2, as for START |
//! | 6 | `Status`'s start, then `St_init04`'s step 0: the floor from the operand, the map's pictures, the blink at 10 falling, the fade in set, one draw |
//! | 7 to 10 | step 1: the fade in runs, one draw a tick |
//! | 11 | the fade is done: page 2, cue 9 (`0x4090000`), no draw |
//! | 12 on | `St_main04`: the page's routine (`0x53DB48`), then one draw |
//!
//! Page 2 (`0x4FBE50`) waits for `Key_trg & 0x6000` (cancel or START) or
//! the PC's map key, then plays the cancel cue and goes to page 0, which
//! starts the close (`St_fade_out_set`, then `St_fade_out_wait`).
//!
//! The draw (`0x4FB800`) steps the shared blink (`0x691F88`) as MAP's
//! marker does, by 2 between 10 and 0x50, and plays the room bank's cue
//! `0x22B0000` on each turn from falling to rising. The target room's fill
//! is lit while the blink rises.

use super::super::GameState;
use super::Sound;
use super::map::{FILL, MapView, OUTLINE, OUTLINE_LIT, floors, map_view, patched, record_of};

/// `0x53DB28`: the room `Check_map_no` looks up on the floor, by operand.
const TARGET_ROOMS: [u8; 5] = [0x16, 0x0A, 0x1A, 0x07, 0x08];

/// The room bank's cue the draw plays as the blink turns to rise.
pub const SOUND_BLINK: u32 = 0x022B_0000;

/// The ticks `St_init04`'s step 1 waits for the fade in, counted from its
/// step 0: the fifth finds it done.
const FADE_IN_TICKS: u8 = 5;

/// The page's routines (`0x53DB48`): the close's two and the wait.
pub const ROUTINE_FADE_SET: u8 = 0;
pub const ROUTINE_FADE_WAIT: u8 = 1;
pub const ROUTINE_WAIT: u8 = 2;

/// The script map's state while its screen is up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptMap {
    /// `Sce_84`'s operand (`0x692D62`).
    pub target: u8,
    /// The floor (`0x691F7A`), which `St_init04` sets from the operand.
    pub floor: u8,
    /// The page's routine (`0x691F61`).
    pub routine: u8,
    /// `St_init04`'s ticks since its step 0, until the fade in is done;
    /// `None` before step 0.
    pub fade_in: Option<u8>,
    /// `St` is 1: `St_main04` runs.
    pub started: bool,
    /// The tick drew the map: every tick from `St_init04`'s step 0 but the
    /// one that finds the fade in done.
    pub drawn: bool,
}

/// What a tick of the map asks of the engine.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScriptMapOutcome {
    /// The core bank's cues.
    pub sounds: Vec<Sound>,
    /// The room bank's: the blink's.
    pub room_sounds: Vec<u32>,
    /// The page has gone to routine 0: the close starts.
    pub close: bool,
}

/// `St_init04`'s floor for an operand.
fn floor_for(target: u8) -> u8 {
    match target {
        1 | 4 => 3,
        3 => 0x10,
        _ => 5,
    }
}

impl ScriptMap {
    pub fn new(target: u8) -> Self {
        Self {
            target,
            // `0x691F7A` keeps its old value until `St_init04` runs; nothing
            // reads it before.
            floor: floor_for(target),
            routine: ROUTINE_WAIT,
            fade_in: None,
            started: false,
            drawn: false,
        }
    }

    /// The target room's record on the floor (`Check_map_no`).
    pub fn target_record(&self) -> u8 {
        let room = TARGET_ROOMS
            .get(usize::from(self.target))
            .copied()
            .unwrap_or(0);
        record_of(self.floor, room)
    }
}

impl GameState {
    /// `Sce_84`: the status screen opens on the next tick in mode 4, on
    /// the floor operand `target` picks.
    pub fn request_re2_script_map(&mut self, target: u8) {
        self.re2_script_map = Some(ScriptMap::new(target));
        self.re2_status_request = true;
    }

    /// The draw's blink step (`0x4FB800`'s top).
    fn step_script_map_blink(&mut self, out: &mut ScriptMapOutcome) {
        let blink = &mut self.re2_file.blink;
        if blink.falling && blink.level < 10 {
            out.room_sounds.push(SOUND_BLINK);
        }
        blink.step_map();
    }

    /// `St_init04`'s step 0, on the open's sixth tick after `Status`'s
    /// start: the floor, the blink at 10 falling, and one draw.
    pub fn start_re2_script_map(&mut self) -> Option<ScriptMapOutcome> {
        let map = self.re2_script_map.as_mut()?;
        map.floor = floor_for(map.target);
        map.fade_in = Some(0);
        map.drawn = true;
        self.re2_file.blink = super::files::Blink {
            level: 10,
            falling: true,
        };
        let mut out = ScriptMapOutcome::default();
        self.step_script_map_blink(&mut out);
        Some(out)
    }

    /// One tick of the map's screen after its start: `St_init04`'s fade in,
    /// then `St_main04`. `None` while no script map is up.
    pub fn tick_re2_script_map(&mut self, cancel: bool) -> Option<ScriptMapOutcome> {
        let mut map = self.re2_script_map?;
        let mut out = ScriptMapOutcome::default();
        if !map.started {
            let Some(ticks) = map.fade_in.as_mut() else {
                return Some(out);
            };
            *ticks += 1;
            if *ticks >= FADE_IN_TICKS {
                map.fade_in = None;
                map.started = true;
                map.routine = ROUTINE_WAIT;
                out.sounds.push(Sound::Map);
                map.drawn = false;
            } else {
                self.step_script_map_blink(&mut out);
            }
            self.re2_script_map = Some(map);
            return Some(out);
        }
        self.step_script_map_routine(&mut map, cancel, &mut out);
        self.step_script_map_blink(&mut out);
        map.drawn = true;
        self.re2_script_map = Some(map);
        Some(out)
    }

    /// The close's ticks, while the screen fades out: the page's routines
    /// and the draw run until the screen's task reaches its exit.
    pub fn tick_re2_script_map_close(&mut self) -> Option<ScriptMapOutcome> {
        let mut map = self.re2_script_map?;
        let mut out = ScriptMapOutcome::default();
        if map.started {
            self.step_script_map_routine(&mut map, false, &mut out);
            self.step_script_map_blink(&mut out);
        }
        self.re2_script_map = Some(map);
        Some(out)
    }

    fn step_script_map_routine(
        &mut self,
        map: &mut ScriptMap,
        cancel: bool,
        out: &mut ScriptMapOutcome,
    ) {
        match map.routine {
            ROUTINE_FADE_SET => map.routine = ROUTINE_FADE_WAIT,
            ROUTINE_FADE_WAIT => {}
            _ => {
                if cancel {
                    out.sounds.push(Sound::Cancel);
                    map.routine = ROUTINE_FADE_SET;
                    out.close = true;
                }
            }
        }
    }

    /// What the map's screen draws (`0x4FB800`): the floor's rooms as the
    /// MAP page draws them, with no marker, and the target room's fill lit
    /// while the blink rises and plain while it falls. `None` on a tick
    /// that draws no map.
    pub fn re2_script_map_view(&self) -> Option<MapView> {
        let map = self.re2_script_map.filter(|map| map.drawn)?;
        let context = self.re2_map_context();
        let flag = |bank: u8, index: u8| self.re2.flag(bank, index);
        let target = map.target_record();
        // No floor is the player's, so no room is lit as the player's and
        // no marker is drawn.
        let mut view = map_view(map.floor, u8::MAX, 0, [0, 0], 0, &context, flag);
        let entry = floors(&context).get(usize::from(map.floor))?;
        let record = &patched(
            map.floor,
            usize::from(target),
            entry.rooms.get(usize::from(target))?,
            &context,
        );
        let lit = flag(super::map::FILL_BANK, record.fill);
        let rising = !self.re2_file.blink.falling;
        let fill = if rising {
            super::map::MapSprite {
                texture: FILL,
                u: record.u,
                v: record.v,
                clut: 2 * u8::from(lit) + 1,
            }
        } else {
            super::map::MapSprite {
                texture: if lit { OUTLINE_LIT } else { OUTLINE },
                u: record.u,
                v: record.v,
                clut: 0,
            }
        };
        let outline = super::map::MapSprite {
            texture: if lit { OUTLINE_LIT } else { OUTLINE },
            u: record.u,
            v: record.v,
            clut: 0,
        };
        let drawn = super::map::DrawnRoom {
            x: record.x,
            y: record.y,
            w: record.w,
            h: record.h,
            fill: Some(fill),
            outline,
        };
        // The target is drawn in its place in the record order whether or
        // not it is visited.
        match view
            .rooms
            .iter()
            .position(|room| room.x == record.x && room.y == record.y)
        {
            Some(index) => view.rooms[index] = drawn,
            None => view.rooms.push(drawn),
        }
        Some(view)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game() -> GameState {
        GameState::new(
            crate::state::RoomId::parse("1020").unwrap(),
            &crate::state::RoomState::default(),
        )
    }

    #[test]
    fn the_opcode_requests_the_map() {
        let mut game = game();
        crate::scd2::host::run_op_for_test(&mut game, "sce_84", &[2]);
        assert!(game.re2_status_request);
        assert_eq!(game.re2_script_map.map(|map| map.target), Some(2));
    }

    #[test]
    fn operand_2_shows_b1_with_the_sewer_room() {
        let map = ScriptMap::new(2);
        assert_eq!(map.floor, 5);
        assert_eq!(ScriptMap::new(1).floor, 3);
        assert_eq!(ScriptMap::new(4).floor, 3);
        assert_eq!(ScriptMap::new(3).floor, 0x10);
        assert_eq!(ScriptMap::new(0).floor, 5);
    }

    /// `leon_a_219_301`'s recording: the start on 4558 leaves the blink
    /// at 8 falling, 4559 turns it with the room bank's cue, 4563 finds
    /// the fade done with cue 9 and no step, and the steps go on from 4564.
    #[test]
    fn the_open_steps_the_blink_as_the_recording_does() {
        let mut game = game();
        game.request_re2_script_map(2);
        assert!(game.re2_status_request);
        let out = game.start_re2_script_map().unwrap();
        assert!(out.room_sounds.is_empty());
        assert_eq!(game.re2_file.blink.level, 8);
        assert!(game.re2_file.blink.falling);
        let out = game.tick_re2_script_map(false).unwrap();
        assert_eq!(out.room_sounds, vec![SOUND_BLINK]);
        assert_eq!(game.re2_file.blink.level, 6);
        assert!(!game.re2_file.blink.falling);
        for level in [8, 10, 12] {
            game.tick_re2_script_map(false);
            assert_eq!(game.re2_file.blink.level, level);
        }
        let out = game.tick_re2_script_map(false).unwrap();
        assert_eq!(out.sounds, vec![Sound::Map]);
        assert_eq!(game.re2_file.blink.level, 12);
        assert!(
            game.re2_script_map_view().is_none(),
            "the fade's end draws no map"
        );
        assert!(game.re2_script_map.unwrap().started);
        game.tick_re2_script_map(false);
        assert_eq!(game.re2_file.blink.level, 14);
        assert!(game.re2_script_map_view().is_some());
    }

    /// The press plays the cancel cue and starts the close; the draw still
    /// steps on the press's tick and on the close's.
    #[test]
    fn cancel_closes_the_map() {
        let mut game = game();
        game.request_re2_script_map(2);
        game.start_re2_script_map();
        for _ in 0..5 {
            game.tick_re2_script_map(false);
        }
        let level = game.re2_file.blink.level;
        let out = game.tick_re2_script_map(true).unwrap();
        assert!(out.close);
        assert_eq!(out.sounds, vec![Sound::Cancel]);
        assert_eq!(game.re2_file.blink.level, level + 2);
        game.tick_re2_script_map_close();
        let map = game.re2_script_map.unwrap();
        assert_eq!(map.routine, ROUTINE_FADE_WAIT);
        assert_eq!(game.re2_file.blink.level, level + 4);
    }
}
