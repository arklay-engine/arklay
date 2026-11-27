//! RE2's game over: the `Die` task (`Die_demo_move`'s phases).
//!
//! The white-out, the strobing stops, the sequence's length, the `Die_spr_*`
//! text and `Die_move_set_cut`'s spiralling camera (`Spiral_move`) are the
//! original's. With `censorship_off` clear, `Die_vcut_set` culls everything
//! the room would draw, so the camera has nothing to show; ARRANGE MODE
//! (`censorship_off`) shows the room's models from it, under
//! `Die_light_set`'s light ([`DieLight`]), and a held death's blood
//! (`Die_blood_move_main`, [`DieText::blood`]) under the text.

use crate::player2::{rcos, rsin};
use crate::scd2::host::Re2State;
use crate::state::Image;

/// `fg_stop`'s top byte when every routine is stopped.
const STOP_ALL: u32 = 0xFF00_0000;
/// The bits `Die_move_wait` flips to let one tick in three through.
const STOP_STROBE: u32 = 0xFC00_0000;
/// The stops the white-out leaves on once it's over: `splayer_work`'s, or
/// the player's when the death is the ally's.
const STOP_AFTER: u32 = 0x4000_0000;
const STOP_AFTER_ALLY: u32 = 0x8000_0000;
/// `BYTE_00525cdc`: who killed the player, which picks a row of the tables here.
const KILLERS: [u8; 15] = [
    0, 0x10, 0x22, 0x20, 0x30, 0x27, 0x2E, 0x33, 0x31, 0x23, 0x2A, 0x2B, 0x34, 0x36, 0x24,
];
/// `Die_move_fade_out`'s wait, `PTR_DAT_00525b18`, and the word past its
/// last row, which [`HELD_WAIT_PAST`] reads.
const HELD_WAITS: [i16; 16] = [
    180, 95, 10, 90, 145, 145, 0, 84, 81, 0, 0, 90, 95, 100, 10, 0,
];
/// The wait's row `Die_move_fade_out` gives a 0x34 with `Type`'s bit 0
/// clear: one past the table's fifteen.
const HELD_WAIT_PAST: usize = 15;
/// `Die_move_pause`'s length, in `DWORD_00525b38`.
const PAUSES: [i16; 15] = [
    91, 91, 96, 86, 150, 120, 120, 86, 82, 75, 83, 81, 81, 85, 96,
];
/// `Die_move_set_cut`'s rows (`DWORD_00525b38`, 0x1C bytes a killer), less
/// the pause ([`PAUSES`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SpiralRow {
    /// `+0`: the circle's radius (`DWORD_0099cf08`).
    radius: i32,
    /// `+4`: what it shrinks by each tick (`DWORD_0099cf10`).
    shrink: i32,
    /// `+8`: the eye's first fall (`WORD_0099cee0`).
    fall: i16,
    /// `+0xA`, `+0xC`: the turn each tick and the start past the player's
    /// facing, with the killer facing the same way as the player
    /// (`Cdir_ck`).
    turn_with: i16,
    start_with: i16,
    /// `+0xE`, `+0x10`: the same, facing the other way.
    turn_against: i16,
    start_against: i16,
    /// `+0x12`: the eye's height on the target.
    height: i16,
    /// `+0x16`: whose part the camera follows: 0 the player, 1 the killer.
    killer: bool,
    /// `+0x18`: the part.
    part: usize,
}

const fn row(
    radius: i32,
    shrink: i32,
    fall: i16,
    with: (i16, i16),
    against: (i16, i16),
    height: i16,
    (killer, part): (bool, usize),
) -> SpiralRow {
    SpiralRow {
        radius,
        shrink,
        fall,
        turn_with: with.0,
        start_with: with.1,
        turn_against: against.0,
        start_against: against.1,
        height,
        killer,
        part,
    }
}

#[rustfmt::skip]
const SPIRAL_ROWS: [SpiralRow; 15] = [
    row(3000, 8, -5, (2, 600), (2, 1500), -1000, (false, 0)),
    row(3000, 8, -5, (2, 600), (2, 1500), -1000, (false, 0)),
    row(3500, 8, -4, (7, 0), (-7, 2048), -1000, (false, 0)),
    row(3500, 10, 20, (4, 600), (-4, 1500), -600, (false, 2)),
    row(3000, 8, 2, (4, 600), (-4, 1500), -600, (false, 1)),
    row(3000, 8, 2, (4, 600), (-4, 1500), -600, (false, 8)),
    row(3000, 8, 2, (4, 600), (-4, 1500), -600, (false, 8)),
    row(3600, 8, 2, (4, 600), (-4, 1500), -600, (false, 0)),
    row(3000, -6, 8, (8, 600), (-8, 1500), -600, (false, 0)),
    row(4000, 8, -10, (12, 600), (12, 1500), -2000, (false, 0)),
    row(3000, 8, -10, (2, 600), (2, 1500), -600, (false, 0)),
    row(3000, 8, -10, (2, 600), (2, 1500), -600, (false, 0)),
    row(3500, 8, -10, (2, 600), (2, 1500), -600, (false, 0)),
    row(8600, 4, -10, (2, 2148), (-2, 1948), 1800, (true, 0)),
    row(3500, 8, -4, (7, 0), (-7, 2048), -1000, (false, 0)),
];

/// Where the spiral looks from and at: `pVp`, `DWORD_0099cef0` and
/// `DWORD_0099cef4`, then `pVr`, `DWORD_0099cefc` and `DWORD_0099cf00`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DieView {
    pub eye: [i32; 3],
    pub target: [i32; 3],
}

/// The spiral's state past `WORD_0099cee0` ([`Die::speed`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Spiral {
    row: usize,
    /// `DWORD_0099cf08`, `DWORD_0099cf10`.
    radius: i32,
    shrink: i32,
    /// `WORD_0099cf16`: what the fall grows by each tick.
    fall_step: i16,
    /// `WORD_0099cf0c`, `WORD_0099cf0e`.
    angle: u16,
    turn: i16,
    view: DieView,
}

/// The game over's light: what `Die_light_set` (`0x44E690`) writes into the
/// cut's LIT record (`rdt + 0x2C`, 0x28 bytes a cut), and what
/// `Die_move_main` and `Die_move_out` change of it after. Light 0 is a
/// point (`+1` 0) 300 above the player's part 0, light 1 black, light 2 at
/// the spiral's target, 500 across, 3000 down and 500 along. The bytes are
/// the record's own; [`DieLight::apply`] takes them to the parsed record's
/// scales.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DieLight {
    /// `+0x10`, `+0x1C`: lights 0's and 2's places.
    positions: [[i16; 3]; 2],
    /// `+4`, `+0xA`: their colours (light 1's, `+7`, is 0).
    colours: [[u8; 3]; 2],
    /// `+0x22`, `+0x26`: their reach.
    radii: [u16; 2],
}

impl DieLight {
    /// `Die_light_set`, from the player's part 0 and the spiral's target.
    fn set(part: [i32; 3], target: [i32; 3]) -> Self {
        Self {
            positions: [
                [
                    part[0] as i16,
                    (part[1] as i16).wrapping_sub(300),
                    part[2] as i16,
                ],
                [
                    (target[0] as i16).wrapping_add(500),
                    (target[1] as i16).wrapping_add(3000),
                    (target[2] as i16).wrapping_add(500),
                ],
            ],
            colours: [[0x78; 3], [0x84; 3]],
            radii: [4000, 8000],
        }
    }

    /// The cut's record `base` as the game over has changed it.
    pub fn apply(&self, mut base: crate::state::CameraLight) -> crate::state::CameraLight {
        let colour = |bytes: [u8; 3]| bytes.map(crate::rdt2::light_colour);
        for (light, index) in [0, 2].into_iter().enumerate() {
            let target = &mut base.lights[index];
            target.pos = self.positions[light].map(i32::from);
            target.color = colour(self.colours[light]);
            target.radius = self.radii[light];
        }
        base.lights[0].kind = 0;
        base.lights[1].color = [0; 3];
        base
    }
}

/// What [`Die::tick`] reads of the bodies: `Cdir_y`s and the parts' `Workm`
/// places, as the tick's draw left them.
#[derive(Debug, Clone, Default)]
pub struct DieBodies {
    pub player_angle: u16,
    pub player_parts: Vec<Option<[i32; 3]>>,
    pub killer_angle: Option<u16>,
    pub killer_parts: Vec<Option<[i32; 3]>>,
    pub ally: Option<DieAlly>,
}

/// `splayer_work` in its death routine (`Routine_0` 3) with `fg_status`'s
/// `0x10000000` up: what `Die_sprt_load` checks for, and what the game over
/// then follows in place of the player.
#[derive(Debug, Clone, Default)]
pub struct DieAlly {
    pub id: u8,
    pub pos: [i32; 3],
    pub parts: Vec<Option<[i32; 3]>>,
}

impl DieBodies {
    fn part(&self, killer: bool, part: usize) -> [i32; 3] {
        let parts = if killer {
            &self.killer_parts
        } else {
            &self.player_parts
        };
        parts.get(part).copied().flatten().unwrap_or_default()
    }
}

/// What a killer below `0x20` waits, whichever zombie it is.
const ZOMBIE_ROW: usize = 1;
const MAIN: i16 = 0xAA;
const OUT_FADE: i16 = 0x6E;
const OUT: i16 = 0xA2;
const LEVEL_FULL: u16 = 0x7FFF;

/// `Be_flg`'s bit that keeps a character from being drawn.
pub const BE_HIDDEN: u32 = 0x8_0000;
/// The ally's `Routine_0` in its death, which `Die_sprt_load` checks for.
pub const ALLY_DYING: u8 = 3;

/// `Die_enemy_set`: those near `player` lose their collisions (`Be_flg`'s 8) and
/// the rest aren't drawn. The one holding the body is always near.
pub fn enemy_set<'a>(
    characters: impl Iterator<Item = &'a mut crate::scd2::host::Character>,
    player: [i32; 3],
) {
    /// The id `Die_enemy_set` leaves alone when far.
    const ID_KEPT: u8 = 0x29;
    for character in characters.filter(|character| character.be_flg & 1 != 0) {
        let kind = character.work.kind;
        let holding = kind & crate::zombie2::TYPE_HELD != 0;
        let (dx, dz) = (
            player[0] - character.body.pos[0],
            player[2] - character.body.pos[2],
        );
        let near = crate::sca::square_root0(dx * dx + dz * dz) <= 0x1C20;
        if holding || (kind >> 8) & 0xC4 == 0 && near {
            character.be_flg |= 8;
        } else if (kind >> 8) & 0xC4 != 0 || character.id != ID_KEPT {
            character.be_flg |= BE_HIDDEN;
        }
    }
}

/// `Die_blood_move_main`'s sound.
pub const CRY: u32 = 0x0400_0000;

/// The pack's entry `Die_sprt_load` reads for a player's id.
pub fn text_entry(player: u8) -> &'static str {
    match player {
        1 | 3 | 5 | 7 | 9 => "die/died01.tim",
        0xB => "die/died0b.tim",
        0xC => "die/died0c.tim",
        0xD => "die/died0d.tim",
        0xE => "die/died0e.tim",
        0xF => "die/died0f.tim",
        _ => "die/died00.tim",
    }
}

/// The texture of a `died*.die`, whose first TIM is what `Die_spr_init` uploads.
pub fn text_sheet(data: &[u8]) -> anyhow::Result<Image> {
    let texture = crate::tim::decode_8bpp(data)?;
    Ok(Image {
        width: texture.width,
        height: texture.height,
        rgba: texture
            .indices
            .iter()
            .flat_map(|&index| texture.palette(0, index))
            .collect(),
    })
}

/// The core sounds `Die_snd_trans` loads for a player's id.
pub fn sound_bank(player: u8) -> u8 {
    match player {
        0xB..=0xF => player + 0xF,
        _ => player & 1 | 0x18,
    }
}

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
    /// `Die_enemy_set` is due.
    pub set_enemies: bool,
    /// `Die_snd_trans`: load the core sounds of the game over.
    pub load_sounds: bool,
    /// `Die_blood_move_main`'s `Snd_se_on`.
    pub cry: bool,
    /// `Die_move_end`: the task has exited.
    pub finished: bool,
    /// `Die_move_fade_out`'s `fg_status` `0x100`: `Cut_check` stops, and the
    /// camera keeps its cut from then on.
    pub lock_cut: bool,
}

/// The `Die` task's variables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Die {
    /// `WORD_0099cee2`.
    phase: u8,
    /// `fg_system`'s `0x100`: killed in an enemy's hands.
    held: bool,
    /// `Die_move_pause`'s length.
    pause: i16,
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
    /// `DWORD_0099cec8`, from `Die_spr_init` on.
    text: Option<Box<DieText>>,
    /// ARRANGE MODE (`censorship_off`).
    arrange: bool,
    /// The killer's row of the tables.
    row: usize,
    /// From `Die_move_set_cut` on.
    spiral: Option<Spiral>,
    /// `Die_light_set`'s light, from `Die_move_set_cut` on.
    light: Option<DieLight>,
    /// `BYTE_0099cf14`'s 1 from `Die_sprt_load`: the ally's id when the
    /// death is the ally's.
    ally: Option<u8>,
}

/// Whatever had hold of the player: its `Id` and `Type` (`+0x10E`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DieKiller {
    pub id: u8,
    pub kind: u16,
}

/// `Die_move_set_cut`'s row: a search from the end, which stops at the
/// first when there's no match. A 0x34 with `Type`'s bit 0 clear takes
/// 0x33's row, and 0x39 takes 0x2E's.
fn killer_row(killer: DieKiller) -> usize {
    let id = match killer.id {
        0x34 if killer.kind & 1 == 0 => 0x33,
        0x39 => 0x2E,
        id => id,
    };
    KILLERS.iter().rposition(|&row| row == id).unwrap_or(0)
}

/// `Die_move_fade_out`'s row of the wait: a killer below 0x20 waits as a
/// zombie does, and a 0x34 with `Type`'s bit 0 clear reads past the table.
fn wait_row(killer: DieKiller) -> usize {
    match killer.id {
        0x34 if killer.kind & 1 == 0 => HELD_WAIT_PAST,
        0x20.. => killer_row(killer),
        _ => ZOMBIE_ROW,
    }
}

impl Die {
    /// `killer` is the id of whatever had hold of the player, when something
    /// did: `fg_system`'s `0x100`.
    pub fn new(killer: Option<DieKiller>, arrange: bool) -> Self {
        let held = killer.is_some();
        let (wait, pause) = killer.map_or((0, PAUSES[ZOMBIE_ROW]), |killer| {
            (HELD_WAITS[wait_row(killer)], PAUSES[killer_row(killer)])
        });
        Self {
            phase: 0,
            held,
            pause,
            count: 0,
            speed: 0,
            level: 0x400,
            strobe: 0,
            wait,
            sleep: 0,
            white_done: false,
            text: None,
            arrange,
            row: killer.map_or(0, killer_row),
            spiral: None,
            light: None,
            ally: None,
        }
    }

    /// The ally whose death this is, from `Die_sprt_load` on.
    pub fn ally(&self) -> Option<u8> {
        self.ally
    }

    /// The id `Die_sprt_load` picks the text by: the player's, or for an
    /// ally's death `(Id & 4 | 0x38) >> 2`.
    pub fn text_player(&self, player: u8) -> u8 {
        self.ally.map_or(player, |id| (id & 4 | 0x38) >> 2)
    }

    /// The part `Spiral_move` circles: the killer's for a row that names it,
    /// else the ally's under `BYTE_0099cf14`'s 1, else the player's.
    fn followed(&self, bodies: &DieBodies, row: SpiralRow) -> [i32; 3] {
        match (&bodies.ally, self.ally) {
            (Some(ally), Some(_)) if !row.killer => ally
                .parts
                .get(row.part)
                .copied()
                .flatten()
                .unwrap_or_default(),
            _ => bodies.part(row.killer, row.part),
        }
    }

    /// `Spiral_move`'s camera, from `Die_move_set_cut` on.
    pub fn view(&self) -> Option<DieView> {
        self.spiral.map(|spiral| spiral.view)
    }

    /// The cut's light as the game over has set it, from `Die_move_set_cut`
    /// on.
    pub fn light(&self) -> Option<DieLight> {
        self.light
    }

    /// The trace's `dieview`: the eye, then the target.
    pub fn view_trace(&self) -> Option<String> {
        self.trace_field()?;
        let view = self.view()?;
        let values: Vec<String> = view
            .eye
            .iter()
            .chain(&view.target)
            .map(i32::to_string)
            .collect();
        Some(values.join(","))
    }

    /// `Die_move_set_cut_usual` (`0x44E4B0`), or `Die_move_set_cut`
    /// (`0x44E300`) for a death in a killer's hands, then the first
    /// `Spiral_move`. A held death is never the ally's, so its first target is
    /// the row's part of the player or the killer.
    fn set_cut(&mut self, bodies: &DieBodies) {
        self.count = 0;
        let spiral = if self.held {
            let row = SPIRAL_ROWS[self.row];
            // `Cdir_ck(pPl, killer)`.
            let with = bodies.killer_angle.is_some_and(|killer| {
                killer.wrapping_sub(bodies.player_angle).wrapping_add(0x400) & 0xFFF < 0x800
            });
            let (turn, start) = if with {
                (row.turn_with, row.start_with)
            } else {
                (row.turn_against, row.start_against)
            };
            let part = bodies.part(row.killer, row.part);
            self.speed = row.fall;
            Spiral {
                row: self.row,
                radius: row.radius,
                shrink: row.shrink,
                fall_step: 0,
                angle: (bodies.player_angle & 0xFFF).wrapping_add(start as u16),
                turn,
                view: DieView {
                    eye: [0, i32::from(row.height) + part[1], 0],
                    target: [part[0], part[1] - 300, part[2]],
                },
            }
        } else {
            let part = bodies.part(false, 0);
            self.speed = 8;
            Spiral {
                row: 0,
                radius: 3000,
                shrink: 0,
                fall_step: 1,
                angle: bodies.player_angle,
                turn: 0x10,
                view: DieView {
                    eye: [0, part[1] - 1000, 0],
                    target: [part[0], part[1] - 300, part[2]],
                },
            }
        };
        self.light = Some(DieLight::set(bodies.part(false, 0), spiral.view.target));
        self.spiral = Some(spiral);
        self.spiral_move(bodies);
    }

    /// `Die_move_main`'s changes to the light, on its count before the
    /// tick's: a held death's light 2 flashes red on counts 6 to 9 and
    /// fades back over 10 to 20, and light 0 is 0x3C from the first tick.
    fn main_light(&mut self) {
        let (held, count) = (self.held, self.count);
        let Some(light) = self.light.as_mut() else {
            return;
        };
        if held {
            if (6..=9).contains(&count) {
                light.colours[1] = [0xFF, 0, 0];
                light.radii[1] = 20000;
            } else if (10..0x15).contains(&count) {
                let [r, g, b] = &mut light.colours[1];
                *r = r.wrapping_sub(0xC);
                *g = g.wrapping_add(0xD);
                *b = b.wrapping_add(0xD);
                light.radii[1] = light.radii[1].wrapping_sub(0x4B0);
            }
        }
        light.colours[0] = [0x3C; 3];
    }

    /// `Die_move_out`'s dimming: light 0's colour down by one a tick to 0,
    /// and its reach by 100 while it's over 100.
    fn out_light(&mut self) {
        let Some(light) = self.light.as_mut() else {
            return;
        };
        light.colours[0] = light.colours[0].map(|channel| channel.saturating_sub(1));
        if light.radii[0] > 100 {
            light.radii[0] -= 100;
        }
    }

    /// `Spiral_move` (`0x44E9D0`): the circle shrinks and turns about the
    /// row's part, the eye falls, and the target closes on the part, a
    /// thirtieth of the way across and a fifth of the way down (to 300
    /// above it) each tick.
    fn spiral_move(&mut self, bodies: &DieBodies) {
        let Some(row) = self.spiral.map(|spiral| SPIRAL_ROWS[spiral.row]) else {
            return;
        };
        let part = self.followed(bodies, row);
        let Some(spiral) = self.spiral.as_mut() else {
            return;
        };
        spiral.radius = spiral.radius.wrapping_sub(spiral.shrink);
        let fall = self.speed;
        self.speed = self.speed.wrapping_add(spiral.fall_step);
        let view = &mut spiral.view;
        view.eye[1] = view.eye[1].wrapping_sub(i32::from(fall));
        let angle = i32::from(spiral.angle as i16);
        view.eye[0] = (rcos(angle).wrapping_mul(spiral.radius) >> 12).wrapping_add(part[0]);
        view.eye[2] = part[2].wrapping_sub(rsin(angle).wrapping_mul(spiral.radius) >> 12);
        spiral.angle = spiral.angle.wrapping_sub(spiral.turn as u16) & 0xFFF;
        view.target[0] += (part[0] - view.target[0]) / 30;
        view.target[2] += (part[2] - view.target[2]) / 30;
        view.target[1] += (part[1] - view.target[1] - 300) / 5;
    }

    /// `Bg_set_mode(2, 0)` and `Die_vcut_set`: nothing of the room is drawn.
    pub fn blackout(&self) -> bool {
        self.white_done
    }

    pub fn text(&self) -> Option<&DieText> {
        self.text.as_deref()
    }

    /// What the trace compares: the phase, its count, and the text's state and
    /// timer.
    pub fn trace_field(&self) -> Option<String> {
        (self.white_done && self.phase < 7).then(|| {
            let (state, timer) = self
                .text
                .as_ref()
                .map_or((0, 0), |text| (text.state, text.timer));
            format!("{},{},{state},{timer}", self.phase, self.count)
        })
    }

    /// `Die_spr_move`.
    fn move_text(&mut self, re2: &mut Re2State, out: &mut DieTick) {
        let Some(text) = self.text.as_mut() else {
            return;
        };
        text.tick(&mut re2.rnd);
        if self.held {
            out.cry |= text.bleed(self.arrange);
        }
    }

    /// One tick of `Die`, run after the tick's routines.
    pub fn tick(&mut self, re2: &mut Re2State, bodies: &DieBodies) -> DieTick {
        let mut out = DieTick::default();
        if self.sleep > 0 {
            self.sleep -= 1;
            return out;
        }
        match self.phase {
            // `Die_move_fade_out`.
            0 => {
                *re2.fg_stop_mut() |= 0x0200_0000;
                // `fg_status`'s `0x8000000` comes down, unless `fg_system`'s
                // `0x4` is up (whose branch plays XA 1, which isn't ported);
                // its `0x140` is the camera's lock and the hold, which
                // arklay keeps outside the bank.
                if re2.banks[0][0] & 0x4 == 0 {
                    use crate::scd2::host::{FG_STATUS_EXTRA_MAP, STATUS_BANK};
                    re2.set_flag(STATUS_BANK, FG_STATUS_EXTRA_MAP, 0);
                }
                out.lock_cut = true;
                // ARRANGE MODE's held death has no `Die_move_timer`.
                self.phase = if self.held && !self.arrange { 1 } else { 2 };
            }
            // `Die_move_timer`.
            1 => {
                self.wait -= 1;
                if self.wait < 0 {
                    self.phase = 2;
                }
            }
            2 => self.wait_white(re2, bodies, &mut out),
            // `Die_move_pause`.
            3 => {
                self.spiral_move(bodies);
                self.count += 1;
                if self.count > self.pause {
                    self.phase = 4;
                    self.count = 0;
                    self.speed = 0;
                    if let Some(spiral) = self.spiral.as_mut() {
                        spiral.shrink = 0;
                        spiral.turn = 0;
                    }
                }
            }
            // `Die_move_main`.
            4 => {
                self.main_light();
                if self.count == 0 {
                    out.load_sounds = true;
                    self.text = Some(Box::new(DieText::new(self.held)));
                }
                self.move_text(re2, &mut out);
                self.spiral_move(bodies);
                self.count += 1;
                if self.count == MAIN {
                    self.phase = 5;
                    self.count = 0;
                    self.speed = 10;
                    if let Some(spiral) = self.spiral.as_mut() {
                        spiral.fall_step = 2;
                        spiral.turn = 0x10;
                    }
                }
            }
            // `Die_move_out`.
            5 => {
                self.move_text(re2, &mut out);
                self.spiral_move(bodies);
                self.count += 1;
                self.out_light();
                if self.count == OUT_FADE {
                    out.fade = Some(DieFade::Black);
                }
                if self.count > OUT {
                    self.phase = 6;
                }
            }
            // `Die_move_end`.
            _ => {
                self.phase = 7;
                out.finished = true;
            }
        }
        out
    }

    /// `Die_move_wait`.
    fn wait_white(&mut self, re2: &mut Re2State, bodies: &DieBodies, out: &mut DieTick) {
        if self.white_done {
            self.count = 0;
            self.phase = if self.held { 3 } else { 4 };
            let stop = re2.fg_stop_mut();
            let after = if self.ally.is_some() {
                STOP_AFTER_ALLY
            } else {
                STOP_AFTER
            };
            *stop = (*stop & 0x03FF_FFFF) | after;
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
        // `Die_sprt_load`, whose file the session reads when the text needs it,
        // and `Bg_set_mode`. Its check for the ally sets `BYTE_0099cf14`.
        self.strobe = 0;
        self.ally = bodies
            .ally
            .as_ref()
            .filter(|_| !self.held)
            .map(|ally| ally.id);
        self.count = 0;
        self.white_done = true;
        re2.effects.die();
        re2.player.be_flg |= crate::player2::BE_NO_SCA;
        out.set_enemies = true;
        self.set_cut(bodies);
        self.sleep = 1;
    }
}

/// The vertices along each side of the text's mesh.
const GRID: usize = 17;
const CELLS: usize = GRID - 1;
/// `Die_spr_move`'s view of the mesh: `Set_view` from `DWORD_ARRAY_00525e5c` to
/// `DWORD_ARRAY_00525e68`, straight down, and `SetGeomScreen`.
const EYE: [i32; 3] = [240, 1008, 88];
const FOCAL: i32 = 0xD0;
/// The vertex colours `Die_spr_move_init` starts with, of which only the first
/// channel ever changes.
const COLOUR_START: u8 = 0x20;
/// `Die_spr_move_main`'s limit and `AddPolyGT4`'s, which it writes back.
const COLOUR_MAX: u8 = 0xFA;
const COLOUR_DRAWN: u8 = 0x80;

/// The grid's vertices of a cell, in `POLY_GT4`'s order.
const CORNERS: [(usize, usize); 4] = [(0, 0), (1, 0), (0, 1), (1, 1)];

/// "YOU DIED": `Die_spr_move_*`'s mesh, falling towards the screen, dripping and
/// then washing away. A vertex is `[column][row]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DieText {
    /// `+0`, `+2`.
    state: u8,
    timer: i16,
    /// `+4`, `+5`.
    speed: u8,
    speed_count: u8,
    /// `+0xeb54`: the ticks before it shows.
    wait: i16,
    /// `+0x680a`, `+0x680c`, `+0x7110`.
    y: [[i16; GRID]; GRID],
    z: [[i16; GRID]; GRID],
    distance: [[i16; GRID]; GRID],
    /// The red of a cell's corners in the two `POLY_GT4` buffers, `[row][column]`.
    colours: [[[[u8; 4]; CELLS]; CELLS]; 2],
    /// `DWORD_009888d8`'s low byte, which buffer this tick draws.
    front: usize,
    /// Whether this tick's state submitted the mesh.
    drawn: bool,
    /// `Die_blood_move_main`'s count (`+6`), and the count this tick drew
    /// its blood at.
    bleeding: i16,
    blood_drawn: Option<i16>,
}

impl DieText {
    /// `Die_spr_move_init`.
    fn new(held: bool) -> Self {
        let mut y = [[0; GRID]; GRID];
        let mut z = [[0; GRID]; GRID];
        let mut distance = [[0; GRID]; GRID];
        for column in 0..GRID {
            for row in 0..GRID {
                let x = column as i32 * 30;
                let at_z = row as i32 * 11 + (rcos(column as i32 * 0x1000) >> 9);
                y[column][row] = 0x212;
                z[column][row] = at_z as i16;
                let (dx, dz) = (0xF0 - x, 0x58 - at_z);
                distance[column][row] = crate::sca::square_root0(dx * dx + dz * dz) as i16;
            }
        }
        Self {
            state: 0,
            timer: 0,
            speed: 0xF,
            speed_count: 1,
            wait: if held { 0x34 } else { 0x16 },
            y,
            z,
            distance,
            colours: [[[[COLOUR_START; 4]; CELLS]; CELLS]; 2],
            front: 0,
            drawn: false,
            bleeding: 0,
            blood_drawn: None,
        }
    }

    fn tick(&mut self, rnd: &mut crate::zombie2::Rnd) {
        self.front ^= 1;
        self.drawn = false;
        match self.state {
            0 => self.grow(),
            1 => self.hold(rnd),
            2 => self.erase(),
            _ => self.end(),
        }
        if self.drawn {
            for cell in self.colours[self.front].as_flattened_mut() {
                *cell = cell.map(|red| red.min(COLOUR_DRAWN));
            }
        }
    }

    /// `Die_spr_move_main`.
    fn grow(&mut self) {
        self.timer += 1;
        if self.timer < self.wait {
            return;
        }
        for column in &mut self.y {
            for y in column {
                *y += i16::from(self.speed);
            }
        }
        self.speed_count -= 1;
        if self.speed_count == 0 {
            self.speed_count = match self.speed {
                2 => 8,
                3 => 5,
                4 => 3,
                _ => 1,
            };
            self.speed = self.speed.saturating_sub(1);
        }
        for row in 0..CELLS {
            for column in 0..CELLS {
                for (corner, (right, down)) in CORNERS.into_iter().enumerate() {
                    let distance = i32::from(self.distance[column + right][row + down]);
                    let red = (0xFF - distance) / 0x1C
                        + i32::from(self.colours[self.front][row][column][corner]);
                    let red = red.min(i32::from(COLOUR_MAX)) as u8;
                    self.colours[0][row][column][corner] = red;
                    self.colours[1][row][column][corner] = red;
                }
            }
        }
        self.drawn = true;
        if i32::from(self.timer) > i32::from(self.wait) + 0x30 {
            self.timer = 0;
            self.state = 1;
        }
    }

    /// `Die_spr_move_hold`: the bottom of the letters drips.
    fn hold(&mut self, rnd: &mut crate::zombie2::Rnd) {
        for row in 0..GRID {
            for column in 0..GRID {
                if row == 0xF && column & 1 != 0 {
                    self.z[column][row] += i16::from(rnd.draw() & 1);
                } else if row == 0xC && column & 1 == 0 && self.timer & 1 != 0 {
                    for lower in 0..5 {
                        self.z[column][row + lower] += i16::from(rnd.draw() & 1);
                    }
                }
            }
        }
        self.drawn = true;
        self.timer += 1;
        if self.timer > 0x3C {
            self.timer = 0;
            self.state = 2;
        }
    }

    /// `Die_spr_move_erase`.
    fn erase(&mut self) {
        let timer = i32::from(self.timer);
        for column in 0..GRID {
            for row in 0..GRID {
                let distance = i32::from(self.distance[column][row]);
                let wave = rsin(distance * 0x40 - timer * 0x80) >> 6;
                self.y[column][row] =
                    (timer * wave / (distance * 0x14 / 0xFF + 0x50)) as i16 + 0x29F;
            }
        }
        for row in 0..CELLS {
            for column in 0..CELLS {
                for (corner, (right, down)) in CORNERS.into_iter().enumerate() {
                    let distance = i32::from(self.distance[column + right][row + down]);
                    let red = &mut self.colours[self.front][row][column][corner];
                    *red = (i32::from(*red) - ((0xFF - distance) >> 5)).max(0) as u8;
                }
            }
        }
        self.drawn = true;
        self.timer += 1;
        if self.timer > 200 {
            self.timer = 0;
            self.state = 3;
        }
    }

    /// `Die_spr_move_end`.
    fn end(&mut self) {
        self.timer += 1;
        if self.timer > 0xB4 {
            self.timer = 0;
            self.state = 0;
        }
    }

    fn screen(&self, column: usize, row: usize) -> [i32; 2] {
        project([
            column as i32 * 30,
            i32::from(self.y[column][row]),
            i32::from(self.z[column][row]),
        ])
    }

    /// `Die_blood_move_main` (`0x44F860`), after the text's state, in a held
    /// death: its sound on its count's first tick, and under ARRANGE MODE its
    /// blood while the count is 100 or less (the count stops there).
    /// Censored, the count runs on and nothing is drawn. Returns whether the
    /// sound plays.
    fn bleed(&mut self, arrange: bool) -> bool {
        let cry = self.bleeding == 0;
        self.blood_drawn = None;
        if arrange {
            if self.bleeding > BLOOD_END {
                return cry;
            }
            self.blood_drawn = Some(self.bleeding);
        }
        self.bleeding += 1;
        cry
    }

    /// The blood this tick drew, as `AddPolyFT4_2` adds it: each layer's
    /// eight strips (the last layer is drawn first), on the
    /// [`SpriteBlend`](crate::render::SpriteBlend) its count gives.
    pub fn blood(&self, sheet: &Image) -> Option<(Vec<Image>, BloodBlend)> {
        let count = self.blood_drawn?;
        let left = BLOOD_END - count;
        let blend = if left >= 0x29 {
            BloodBlend::Opaque
        } else {
            BloodBlend::Half
        };
        let layers = BLOOD_LAYERS
            .iter()
            .enumerate()
            .rev()
            .filter_map(|(layer, row)| {
                let mut image = Image {
                    width: 320,
                    height: 240,
                    rgba: vec![0; 320 * 240 * 4],
                };
                let step = count - row.start;
                if step < 0 {
                    return None;
                }
                let (mut red, depth) = if step < row.ramp {
                    (step * 40, (row.depth)(step))
                } else {
                    (200, row.held)
                };
                if red == 0 {
                    return None;
                }
                if left < 0x29 {
                    red = left * 12 / 2;
                    if left < 0x14 {
                        red = left * 12 / 4;
                    }
                }
                for strip in 0..8 {
                    let column = strip * 2;
                    let corner = |column: usize, row_: usize, lower: bool| {
                        let mut z = i32::from(self.z[column][row_]) + row.offset[1];
                        z = (z as i16).wrapping_mul(2) as i32;
                        if count > 0x14 {
                            let wave = (rcos(((layer * 0x400 + column) * 0x40) as i32)
                                + 0x1000
                                + rcos(((layer * 7 + column) * 0x100) as i32))
                                / 0x140;
                            let drip = i32::from(count - 0x14) * wave;
                            z += if lower { drip * 2 / 0x28 } else { drip / 0x28 };
                        }
                        if lower {
                            z += 0xF0;
                        }
                        let x = column as i32 * 30 + row.offset[0];
                        project([x, i32::from(depth), z])
                    };
                    let u0 = (strip * 32) as f32;
                    let u1 = u0 + if strip == 7 { 31.0 } else { 32.0 };
                    let corners = [
                        (corner(column, 0, false), [u0, 64.0]),
                        (corner(column + 2, 0, false), [u1, 64.0]),
                        (corner(column, 1, true), [u0, 192.0]),
                        (corner(column + 2, 1, true), [u1, 192.0]),
                    ]
                    .map(|(at, uv)| Corner {
                        at,
                        uv,
                        red: red as f32,
                    });
                    blood_triangle(&mut image, sheet, [corners[0], corners[1], corners[2]]);
                    blood_triangle(&mut image, sheet, [corners[1], corners[3], corners[2]]);
                }
                Some(image)
            })
            .collect();
        Some((layers, blend))
    }

    /// `AddPolyGT4`'s sum of the mesh on the `sheet` (`died*.die`'s texture),
    /// into a transparent `image`.
    pub fn draw(&self, sheet: &Image, image: &mut Image) {
        if !self.drawn {
            return;
        }
        for row in 0..CELLS {
            for column in 0..CELLS {
                let reds = self.colours[self.front][row][column];
                let corners: [Corner; 4] = std::array::from_fn(|corner| {
                    let (right, down) = CORNERS[corner];
                    Corner {
                        at: self.screen(column + right, row + down),
                        uv: [
                            (column * 16 + right * 16) as f32,
                            (row * 4 + down * 4) as f32,
                        ],
                        red: f32::from(reds[corner]),
                    }
                });
                add_triangle(image, sheet, [corners[0], corners[1], corners[2]]);
                add_triangle(image, sheet, [corners[1], corners[3], corners[2]]);
            }
        }
    }
}

/// `Die_spr_move`'s projection of a mesh point `(x, y, z)`, `y` its depth.
fn project([x, y, z]: [i32; 3]) -> [i32; 2] {
    let depth = EYE[1] - y;
    let scale = |at: i32| at * FOCAL / depth.max(1);
    [160 + scale(x - EYE[0]), 120 + scale(z - EYE[2])]
}

/// `Die_blood_move_main`'s count past which it draws nothing more.
const BLOOD_END: i16 = 100;

/// One of `Die_blood_move_main`'s three splashes: the count it starts on,
/// the ticks it takes to come close (its red 40 a tick till then, then
/// 200), its depth meanwhile and after, and its offset on the text's mesh
/// (`x`, and `z` before it is doubled).
struct BloodLayer {
    start: i16,
    ramp: i16,
    depth: fn(i16) -> i16,
    held: i16,
    offset: [i32; 2],
}

const BLOOD_LAYERS: [BloodLayer; 3] = [
    BloodLayer {
        start: 0,
        ramp: 5,
        depth: |step| step * step * 0xE + 0x168,
        held: 600,
        offset: [0x5A, 0],
    },
    BloodLayer {
        start: 4,
        ramp: 5,
        depth: |step| step * step * 0xE + 200,
        held: 0x1B8,
        offset: [0x104, -0x70],
    },
    BloodLayer {
        start: 9,
        ramp: 3,
        depth: |step| (step * step + 0x10) * 10,
        held: 0x140,
        offset: [-0x10E, -0x54],
    },
];

/// How the blood's strips meet the frame: `code` 0, then `code` 2 as it
/// fades. `AddPolyFT4_2` (`0x440FF0`) takes Marni's blend from `tag & 3`,
/// which is 0 (`0x100000`, half), so the late `tpage` 0xB5 changes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BloodBlend {
    Opaque,
    Half,
}

/// One triangle of a blood strip: the sheet's texel under the strip's red
/// alone (`r0`; its green and blue are 0), over what the layer has.
fn blood_triangle(image: &mut Image, sheet: &Image, corners: [Corner; 3]) {
    let [a, b, c] = corners.map(|corner| corner.at);
    let area = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
    if area == 0 {
        return;
    }
    let (width, height) = (image.width as i32, image.height as i32);
    let (left, right) = (
        a[0].min(b[0]).min(c[0]).max(0),
        a[0].max(b[0]).max(c[0]).min(width - 1),
    );
    let (top, bottom) = (
        a[1].min(b[1]).min(c[1]).max(0),
        a[1].max(b[1]).max(c[1]).min(height - 1),
    );
    let whole = area as f32;
    for y in top..=bottom {
        for x in left..=right {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let edge = |from: [i32; 2], to: [i32; 2]| {
                (to[0] - from[0]) as f32 * (py - from[1] as f32)
                    - (to[1] - from[1]) as f32 * (px - from[0] as f32)
            };
            let weights = [edge(b, c), edge(c, a), edge(a, b)].map(|w| w / whole);
            if weights.iter().any(|&w| w < 0.0) {
                continue;
            }
            let mix = |value: fn(&Corner) -> f32| {
                weights
                    .iter()
                    .zip(&corners)
                    .map(|(&weight, corner)| weight * value(corner))
                    .sum::<f32>()
            };
            let (u, v) = (mix(|corner| corner.uv[0]), mix(|corner| corner.uv[1]));
            let texel = (v as usize).min(sheet.height as usize - 1) * sheet.width as usize
                + (u as usize).min(sheet.width as usize - 1);
            let Some(colour) = sheet.rgba.get(texel * 4..texel * 4 + 4) else {
                continue;
            };
            if colour[3] == 0 {
                continue;
            }
            let at = (y as usize * image.width as usize + x as usize) * 4;
            // `AddPolyFT4_2` caps the red at 0x80; Marni draws the texel's
            // level as `c * 8 + 7`, as the recordings show.
            let level = colour[0] >> 3;
            let texel = if level == 0 {
                0.0
            } else {
                f32::from(level * 8 + 7)
            };
            let red = (texel * corners[0].red.min(128.0) / 128.0) as u8;
            image.rgba[at..at + 4].copy_from_slice(&[red, 0, 0, 0xFF]);
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Corner {
    at: [i32; 2],
    uv: [f32; 2],
    red: f32,
}

/// One triangle of the mesh. A pixel on an edge two triangles share goes to one
/// of them, so that the sum shows no seams.
fn add_triangle(image: &mut Image, sheet: &Image, corners: [Corner; 3]) {
    let [a, b, c] = corners.map(|corner| corner.at);
    let edge = |from: [i32; 2], to: [i32; 2], x: i32, y: i32| {
        (to[0] - from[0]) * (2 * y + 1 - 2 * from[1])
            - (to[1] - from[1]) * (2 * x + 1 - 2 * from[0])
    };
    let area = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
    if area == 0 {
        return;
    }
    let sign = area.signum();
    let (width, height) = (image.width as i32, image.height as i32);
    let (left, right) = (
        a[0].min(b[0]).min(c[0]).max(0),
        a[0].max(b[0]).max(c[0]).min(width - 1),
    );
    let (top, bottom) = (
        a[1].min(b[1]).min(c[1]).max(0),
        a[1].max(b[1]).max(c[1]).min(height - 1),
    );
    let owns = |from: [i32; 2], to: [i32; 2]| {
        let (dx, dy) = ((to[0] - from[0]) * sign, (to[1] - from[1]) * sign);
        dy > 0 || (dy == 0 && dx < 0)
    };
    let edges = [(b, c), (c, a), (a, b)];
    let whole = 2.0 * area as f32;
    for y in top..=bottom {
        for x in left..=right {
            let weights = edges.map(|(from, to)| sign * edge(from, to, x, y));
            if !weights
                .iter()
                .zip(edges)
                .all(|(&weight, (from, to))| weight > 0 || (weight == 0 && owns(from, to)))
            {
                continue;
            }
            let mix = |value: fn(&Corner) -> f32| {
                weights
                    .iter()
                    .zip(&corners)
                    .map(|(&weight, corner)| (sign * weight) as f32 * value(corner))
                    .sum::<f32>()
                    / whole
            };
            let (u, v) = (mix(|corner| corner.uv[0]), mix(|corner| corner.uv[1]));
            let red = mix(|corner| corner.red);
            let texel = (v as usize).min(sheet.height as usize - 1) * sheet.width as usize
                + (u as usize).min(sheet.width as usize - 1);
            let Some(colour) = sheet.rgba.get(texel * 4..texel * 4 + 4) else {
                continue;
            };
            if colour[3] == 0 {
                continue;
            }
            let at = (y as usize * image.width as usize + x as usize) * 4;
            let lit = [red, f32::from(COLOUR_START), f32::from(COLOUR_START)];
            for channel in 0..3 {
                // A vertex colour reaches the screen at half, as with the pool
                // of blood, measured against the original.
                let sum = f32::from(image.rgba[at + channel])
                    + f32::from(colour[channel]) * lit[channel] / 256.0;
                image.rgba[at + channel] = sum.min(255.0) as u8;
            }
            image.rgba[at + 3] = 0xFF;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A death in the hands of `id`, whose `Type` is 0.
    fn held(id: u8) -> Option<DieKiller> {
        Some(DieKiller { id, kind: 0 })
    }

    /// A held death's text, past `Die_spr_init`.
    fn text_at(ticks: usize) -> (DieText, crate::zombie2::Rnd) {
        let mut text = DieText::new(true);
        let mut rnd = crate::zombie2::Rnd::default();
        for _ in 0..ticks {
            text.tick(&mut rnd);
        }
        (text, rnd)
    }

    #[test]
    fn the_text_keeps_the_originals_timetable() {
        // The original's trace: state 1 from `Die_move_main`'s count 101.
        let (text, _) = text_at(101);
        assert_eq!((text.state, text.timer), (1, 0));
        let (text, _) = text_at(101 + 61);
        assert_eq!((text.state, text.timer), (2, 0));
    }

    #[test]
    fn the_text_only_draws_after_its_wait_and_rises_to_its_limit() {
        let (text, _) = text_at(0x33);
        assert!(!text.drawn);
        assert_eq!(text.y[0][0], 0x212);
        let (text, _) = text_at(0x34);
        assert!(text.drawn);
        assert_eq!(text.y[0][0], 0x212 + 0xF);
        let (text, _) = text_at(101);
        assert_eq!(text.colours[text.front][8][8][0], COLOUR_DRAWN);
    }

    #[test]
    fn only_the_hold_draws_from_rnd() {
        let (_, rnd) = text_at(101);
        assert_eq!(rnd, crate::zombie2::Rnd::default());
        let (_, held) = text_at(102);
        assert_ne!(held, rnd);
    }

    #[test]
    fn the_view_is_straight_down_on_the_meshs_middle() {
        let (text, _) = text_at(0);
        // 0xD0 over the 478 from the eye to the mesh.
        assert_eq!(text.screen(8, 8), [160, 120 + 8 * 208 / 478]);
        assert_eq!(text.screen(0, 8)[0], 160 - 240 * 208 / 478);
        assert_eq!(text.screen(8, 0)[1], 120 - 80 * 208 / 478);
    }

    #[test]
    fn each_player_has_their_own_file_and_sounds() {
        assert_eq!(text_entry(0), "die/died00.tim");
        assert_eq!(text_entry(1), "die/died01.tim");
        assert_eq!(text_entry(0xE), "die/died0e.tim");
        assert_eq!(sound_bank(0), 0x18);
        assert_eq!(sound_bank(1), 0x19);
        assert_eq!(sound_bank(0xB), 0x1A);
        assert_eq!(sound_bank(0xF), 0x1E);
    }

    #[test]
    fn a_triangle_pair_adds_each_pixel_once() {
        let sheet = Image {
            width: 1,
            height: 1,
            rgba: vec![0x80, 0, 0, 0xFF],
        };
        let mut image = Image {
            width: 8,
            height: 8,
            rgba: vec![0; 8 * 8 * 4],
        };
        let corner = |x, y| Corner {
            at: [x, y],
            uv: [0.0, 0.0],
            red: 0x80 as f32,
        };
        add_triangle(
            &mut image,
            &sheet,
            [corner(1, 1), corner(5, 1), corner(1, 5)],
        );
        add_triangle(
            &mut image,
            &sheet,
            [corner(5, 1), corner(5, 5), corner(1, 5)],
        );
        for y in 1..5 {
            for x in 1..5 {
                assert_eq!(image.rgba[(y * 8 + x) * 4], 0x40, "{x},{y}");
            }
        }
        assert_eq!(image.rgba[0], 0);
    }

    #[test]
    fn the_killer_picks_the_wait_and_the_pause() {
        let zombie = Die::new(held(0x10), false);
        assert_eq!((zombie.wait, zombie.pause), (95, 91));
        // A zombie this table has no row for waits as the others do.
        let other = Die::new(held(0x1E), false);
        assert_eq!((other.wait, other.pause), (95, 91));
        let licker = Die::new(held(0x22), false);
        assert_eq!((licker.wait, licker.pause), (10, 96));
        let none = Die::new(None, false);
        assert_eq!((none.wait, none.held), (0, false));
    }

    #[test]
    fn a_0x34_without_types_bit_0_takes_0x33s_row_and_the_word_past_the_waits() {
        // `Die_move_set_cut` swaps it for 0x33 (row 7), and
        // `Die_move_fade_out` reads its wait from row 15, the 0 past the
        // table's end.
        let swapped = Die::new(held(0x34), false);
        assert_eq!((swapped.row, swapped.wait, swapped.pause), (7, 0, 86));
        // With the bit up it keeps its own row, 12, for both.
        let own = Die::new(Some(DieKiller { id: 0x34, kind: 1 }), false);
        assert_eq!((own.row, own.wait, own.pause), (12, 95, 81));
        let g3 = Die::new(held(0x33), false);
        assert_eq!((g3.row, g3.wait, g3.pause), (7, 84, 86));
    }

    fn ally_bodies() -> DieBodies {
        DieBodies {
            player_parts: vec![Some([0, 0, 0])],
            ally: Some(DieAlly {
                id: 0x44,
                pos: [5000, 0, 6000],
                parts: vec![Some([5000, -900, 6000])],
            }),
            ..DieBodies::default()
        }
    }

    /// From `Die_move_wait`'s last white to the tick after its cut.
    fn past_the_white(die: &mut Die, bodies: &DieBodies) -> Re2State {
        let mut re2 = Re2State::default();
        die.phase = 2;
        die.level = LEVEL_FULL;
        die.tick(&mut re2, bodies);
        die.tick(&mut re2, bodies);
        die.tick(&mut re2, bodies);
        re2
    }

    #[test]
    fn an_allys_death_stops_the_player_and_circles_the_ally() {
        let bodies = ally_bodies();
        let mut die = Die::new(None, true);
        let mut re2 = past_the_white(&mut die, &bodies);
        assert_eq!(die.ally(), Some(0x44));
        // `Die_move_wait` leaves the player's stop up, not `splayer_work`'s.
        assert_eq!(*re2.fg_stop_mut() & 0xFC00_0000, STOP_AFTER_ALLY);
        // The usual cut starts on the player's part, and `Spiral_move`
        // closes on the ally's.
        let target = die.view().unwrap().target;
        die.tick(&mut re2, &bodies);
        let moved = die.view().unwrap().target;
        assert!(moved[0] > target[0] && moved[2] > target[2]);
        // The text is the one for the ally's id: `(0x44 & 4 | 0x38) >> 2`.
        assert_eq!(die.text_player(0), 0xF);
        assert_eq!(text_entry(die.text_player(0)), "die/died0f.tim");
    }

    #[test]
    fn a_held_death_ignores_the_ally() {
        let bodies = ally_bodies();
        let mut die = Die::new(held(0x10), true);
        let mut re2 = past_the_white(&mut die, &bodies);
        assert_eq!(die.ally(), None);
        assert_eq!(*re2.fg_stop_mut() & 0xFC00_0000, STOP_AFTER);
        assert_eq!(die.text_player(1), 1);
    }

    /// The ticks the original's `fg_stop` changes on after a zombie's kill,
    /// counted from the kill.
    #[test]
    fn a_held_death_stops_the_routines_when_the_original_does() {
        let mut re2 = Re2State::default();
        let mut die = Die::new(held(0x10), false);
        let mut changes = Vec::new();
        let mut last = 0;
        let mut end = None;
        for tick in 0..1000 {
            let out = die.tick(&mut re2, &DieBodies::default());
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

    /// `Die_move_fade_out` takes down `fg_status`'s `0x8000000` (an extra
    /// mode's map closed) on its tick, but under `fg_system`'s `0x4`.
    /// The tick `Die` finishes on.
    fn end_of(mut die: Die) -> usize {
        let mut re2 = Re2State::default();
        (0..2000)
            .find(|_| die.tick(&mut re2, &DieBodies::default()).finished)
            .expect("Die finishes")
    }

    #[test]
    fn arrange_modes_held_death_has_no_timer() {
        // `Die_move_fade_out` goes straight to `Die_move_wait` under
        // `censorship_off`, so the zombie's timer (its wait of 95, and the
        // tick that passes it) is skipped.
        assert_eq!(end_of(Die::new(held(0x10), false)), 545);
        assert_eq!(end_of(Die::new(held(0x10), true)), 545 - 96);
        // A death outside a killer's hands has no timer either way.
        assert_eq!(end_of(Die::new(None, true)), end_of(Die::new(None, false)));
    }

    fn bodies(player: [i32; 3], angle: u16) -> DieBodies {
        DieBodies {
            player_angle: angle,
            player_parts: vec![Some(player)],
            ..DieBodies::default()
        }
    }

    #[test]
    fn the_usual_spiral_circles_the_player_and_falls_faster() {
        let body = bodies([1000, -500, 2000], 0);
        let mut die = Die::new(None, true);
        die.white_done = true;
        die.phase = 2;
        die.spiral = None;
        // `Die_move_set_cut_usual` then the first `Spiral_move`: 3000 out
        // along the player's facing, 1000 up and the first fall of 8.
        die.set_cut(&body);
        let view = die.view().unwrap();
        assert_eq!(view.eye, [4000, -1508, 2000]);
        assert_eq!(view.target, [1000, -800, 2000]);
        // Each `Spiral_move` turns 0x10 the other way and falls one more.
        die.spiral_move(&body);
        let view = die.view().unwrap();
        let angle = i32::from(0u16.wrapping_sub(0x10) & 0xFFF);
        assert_eq!(view.eye[0], ((rcos(angle) * 3000) >> 12) + 1000);
        assert_eq!(view.eye[1], -1508 - 9);
        assert_eq!(view.eye[2], 2000 - ((rsin(angle) * 3000) >> 12));
        assert_eq!(view.target, [1000, -800, 2000]);
    }

    #[test]
    fn the_target_closes_on_the_part_by_a_thirtieth_and_a_fifth() {
        let mut die = Die::new(None, true);
        die.set_cut(&bodies([0, 0, 0], 0));
        let moved = bodies([3000, -1000, -600], 0);
        die.spiral_move(&moved);
        let view = die.view().unwrap();
        // 300 above the part, a fifth of the way at a time.
        assert_eq!(view.target, [100, -300 + (-1000 + 300 - 300) / 5, -20]);
    }

    #[test]
    fn a_held_spiral_takes_its_killers_row_and_facing() {
        // Room 11B's zombie (0x10), row 1: 3000 out, shrinking 8, from 600
        // past the player's facing when the zombie faces the same way, and
        // 1000 above the player's part.
        let mut with = bodies([0, 0, 0], 0x100);
        with.killer_angle = Some(0x100);
        let mut die = Die::new(held(0x10), true);
        die.set_cut(&with);
        let spiral = die.spiral.unwrap();
        assert_eq!((spiral.radius, spiral.shrink), (3000 - 8, 8));
        assert_eq!(spiral.angle, (0x100 + 600 - 2) & 0xFFF);
        assert_eq!(die.view().unwrap().eye[1], -1000 + 5);
        // Facing the player, it starts 1500 past.
        let mut against = with.clone();
        against.killer_angle = Some(0x900);
        let mut die = Die::new(held(0x10), true);
        die.set_cut(&against);
        assert_eq!(die.spiral.unwrap().angle, (0x100 + 1500 - 2) & 0xFFF);
    }

    #[test]
    fn the_pause_stills_the_spiral_and_main_sets_it_falling() {
        let body = bodies([0, 0, 0], 0);
        let mut re2 = Re2State::default();
        let mut die = Die::new(held(0x10), true);
        die.white_done = true;
        die.phase = 2;
        die.set_cut(&body);
        die.phase = 3;
        // `Die_move_pause`'s last tick zeroes the shrink, the turn and the
        // fall.
        for _ in 0..=die.pause {
            die.tick(&mut re2, &body);
        }
        assert_eq!(die.phase, 4);
        let spiral = die.spiral.unwrap();
        assert_eq!((spiral.shrink, spiral.turn, die.speed), (0, 0, 0));
        // `Die_move_main`'s 0xAA'th tick sets 10, growing 2, turning 0x10.
        for _ in 0..MAIN {
            die.tick(&mut re2, &body);
        }
        assert_eq!(die.phase, 5);
        let spiral = die.spiral.unwrap();
        assert_eq!((spiral.fall_step, spiral.turn, die.speed), (2, 0x10, 10));
    }

    #[test]
    fn the_light_is_set_on_the_player_and_the_target() {
        let mut die = Die::new(None, true);
        die.set_cut(&bodies([1000, -500, 2000], 0));
        let light = die.light().unwrap();
        // `Die_light_set` reads the target before the first `Spiral_move`.
        assert_eq!(light.positions, [[1000, -800, 2000], [1500, 2200, 2500]]);
        assert_eq!(light.colours, [[0x78; 3], [0x84; 3]]);
        assert_eq!(light.radii, [4000, 8000]);
        let base = crate::state::CameraLight {
            ambient: [7; 3],
            lights: [crate::state::Light {
                kind: 1,
                color: [9; 3],
                re2_gain: Some(3),
                ..Default::default()
            }; 3],
        };
        let lit = light.apply(base);
        assert_eq!(lit.ambient, [7; 3]);
        assert_eq!(lit.lights[0].kind, 0);
        assert_eq!(lit.lights[0].pos, [1000, -800, 2000]);
        assert_eq!(lit.lights[0].color, [crate::rdt2::light_colour(0x78); 3]);
        assert_eq!(lit.lights[1].color, [0; 3]);
        assert_eq!(lit.lights[1].kind, 1);
        assert_eq!(lit.lights[2].kind, 1);
        assert_eq!(lit.lights[2].radius, 8000);
        assert_eq!(lit.lights[2].re2_gain, Some(3));
    }

    #[test]
    fn a_held_deaths_light_flashes_red_and_the_out_dims_it() {
        let body = bodies([0, 0, 0], 0);
        let mut re2 = Re2State::default();
        let mut die = Die::new(held(0x10), true);
        die.white_done = true;
        die.set_cut(&body);
        die.phase = 4;
        die.count = 0;
        let mut seen = Vec::new();
        for _ in 0..0x16 {
            die.tick(&mut re2, &body);
            let light = die.light().unwrap();
            seen.push((light.colours, light.radii));
        }
        // Counts 0 to 5 keep light 2; light 0 is 0x3C from the first.
        assert_eq!(seen[5], ([[0x3C; 3], [0x84; 3]], [4000, 8000]));
        // 6 to 9: red, reaching 20000.
        assert_eq!(seen[9], ([[0x3C; 3], [0xFF, 0, 0]], [4000, 20000]));
        // 10 to 20: 12 less red, 13 more green and blue, 1200 less reach.
        assert_eq!(seen[10].0[1], [0xF3, 0x0D, 0x0D]);
        assert_eq!(seen[20].0[1], [0xFF - 11 * 0xC, 11 * 0xD, 11 * 0xD]);
        assert_eq!(seen[20].1[1], 20000 - 11 * 1200);
        assert_eq!(seen[21], seen[20]);
        // `Die_move_out` takes light 0 down a step and its reach by 100.
        die.phase = 5;
        die.count = 0;
        die.tick(&mut re2, &body);
        let light = die.light().unwrap();
        assert_eq!((light.colours[0], light.radii[0]), ([0x3B; 3], 3900));
        // Unheld, light 2 stays as set.
        let mut die = Die::new(None, true);
        die.white_done = true;
        die.set_cut(&body);
        die.phase = 4;
        for _ in 0..0x16 {
            die.tick(&mut re2, &body);
        }
        assert_eq!(die.light().unwrap().colours[1], [0x84; 3]);
    }

    /// A sheet whose blood rows (v 64 to 191) are red level 8.
    fn blood_sheet() -> Image {
        let mut rgba = vec![0; 256 * 256 * 4];
        for texel in rgba
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .skip(256 * 64)
            .take(256 * 128)
        {
            texel.copy_from_slice(&[(8 << 3) | 2, 0, 0, 0xFF]);
        }
        Image {
            width: 256,
            height: 256,
            rgba,
        }
    }

    #[test]
    fn the_blood_cries_once_and_only_draws_under_arrange_mode() {
        let sheet = blood_sheet();
        let mut censored = DieText::new(true);
        assert!(censored.bleed(false));
        for _ in 0..200 {
            assert!(!censored.bleed(false));
            assert!(censored.blood(&sheet).is_none());
        }
        let mut text = DieText::new(true);
        assert!(text.bleed(true));
        // Count 0: the first layer's red is still 0, so nothing shows.
        let (layers, _) = text.blood(&sheet).unwrap();
        assert!(layers.is_empty());
        let mut counts = Vec::new();
        for _ in 0..110 {
            assert!(!text.bleed(true));
            counts.push(
                text.blood(&sheet)
                    .map(|(layers, blend)| (layers.len(), blend)),
            );
        }
        // Counts 1 to 4 one layer, 5 to 9 two, then three.
        assert_eq!(counts[0], Some((1, BloodBlend::Opaque)));
        assert_eq!(counts[4], Some((2, BloodBlend::Opaque)));
        assert_eq!(counts[9], Some((3, BloodBlend::Opaque)));
        // From count 60 it blends by half (`tag` 0, whatever its `tpage`),
        // and past 100 it's gone, its count held there.
        assert_eq!(counts[58], Some((3, BloodBlend::Opaque)));
        assert_eq!(counts[59], Some((3, BloodBlend::Half)));
        assert_eq!(counts[80], Some((3, BloodBlend::Half)));
        assert_eq!(counts[99], Some((3, BloodBlend::Half)));
        assert_eq!(counts[100], None);
        assert_eq!(text.bleeding, BLOOD_END + 1);
    }

    /// A sheet whose blood rows' red level is `u / 8` (at least 1), so a
    /// drawn pixel tells which texel column it came from.
    fn columned_sheet() -> Image {
        let mut sheet = blood_sheet();
        for (texel, colour) in sheet.rgba.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            if colour[3] != 0 {
                colour[0] = ((texel % 256 / 8).max(1) as u8) << 3;
            }
        }
        sheet
    }

    /// The first splash's image (the last drawn) at `count`.
    fn first_splash(count: i16, sheet: &Image) -> Image {
        let mut text = DieText::new(true);
        for _ in 0..=count {
            text.bleed(true);
        }
        text.blood(sheet).unwrap().0.pop().unwrap()
    }

    fn level(image: &Image, [x, y]: [i32; 2]) -> u8 {
        let at = (y as usize * image.width as usize + x as usize) * 4;
        image.rgba[at] >> 3
    }

    #[test]
    fn the_blood_strips_sit_on_the_texts_mesh_with_their_texels() {
        let sheet = columned_sheet();
        // Count 10: the first splash held at depth 600, no drip yet. The
        // mesh's rows 0 and 1 sit at `z` 8 and 19 (`rcos` of whole turns
        // `>> 9`), doubled, the lower 0xF0 further: 16 to 278. Strip `j`
        // spans `x` 60j to 60j + 60, 0x5A along, and `u` 32j to 32j + 32.
        let image = first_splash(10, &sheet);
        for strip in 0..8 {
            let left = strip * 60 + 0x5A;
            for (across, u) in [(15, 8), (45, 24)] {
                let at = project([left + across, 600, (16 + 278) / 2]);
                if at[0] >= 320 {
                    continue;
                }
                let u = strip * 32 + u;
                assert_eq!(
                    level(&image, at),
                    (u / 8) as u8,
                    "strip {strip} at {across}"
                );
            }
        }
        // Past count 20 the strips drip: the lower corners twice as far as
        // the upper. Strip 0's bottom moves down by its wave.
        let wave = (rcos(0) + 0x1000 + rcos(0)) / 0x140;
        let bottom = |image: &Image| {
            let x = project([0x5A + 2, 600, 0])[0];
            (0..240).rev().find(|&y| level(image, [x, y]) != 0).unwrap()
        };
        let before = bottom(&first_splash(20, &sheet));
        let after = bottom(&first_splash(40, &sheet));
        let drop = project([0, 600, 278 + 20 * wave * 2 / 0x28])[1] - project([0, 600, 278])[1];
        assert!(
            (after - before - drop).abs() <= 1,
            "{before} {after} {drop}"
        );
    }

    #[test]
    fn the_blood_shows_the_texels_level_up_to_a_red_of_0x80() {
        let sheet = blood_sheet();
        let mut text = DieText::new(true);
        let reds = |text: &DieText| -> Vec<u8> {
            let (layers, _) = text.blood(&sheet).unwrap();
            let mut reds: Vec<u8> = layers
                .iter()
                .flat_map(|layer| layer.rgba.as_chunks::<4>().0.iter().map(|texel| texel[0]))
                .filter(|&red| red != 0)
                .collect();
            reds.dedup();
            reds
        };
        // Count 1: red 40 of 0x80 over level 8's 71, 22.
        text.bleed(true);
        text.bleed(true);
        assert_eq!(reds(&text), [22]);
        // Count 5 on: 200, which shows as 0x80 would.
        for _ in 0..4 {
            text.bleed(true);
        }
        assert!(reds(&text).iter().all(|&red| red == 71 || red == 22));
        for _ in 0..10 {
            text.bleed(true);
        }
        assert_eq!(reds(&text), [71]);
    }

    #[test]
    fn the_fade_out_takes_down_the_extras_map_bit() {
        use crate::scd2::host::{FG_STATUS_EXTRA_MAP, STATUS_BANK};
        for (system, kept) in [(0, false), (0x4, true)] {
            let mut re2 = Re2State::default();
            re2.banks[0][0] = system;
            re2.set_flag(STATUS_BANK, FG_STATUS_EXTRA_MAP, 1);
            let mut die = Die::new(held(0x10), false);
            die.tick(&mut re2, &DieBodies::default());
            assert_eq!(
                re2.flag(STATUS_BANK, FG_STATUS_EXTRA_MAP),
                kept,
                "{system:#x}"
            );
        }
    }

    #[test]
    fn the_white_climbs_to_full_then_clears_and_ends_in_black() {
        let mut re2 = Re2State::default();
        let mut die = Die::new(held(0x10), false);
        let mut whites = Vec::new();
        let mut others = Vec::new();
        for tick in 0..546 {
            match die.tick(&mut re2, &DieBodies::default()).fade {
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
        let mut die = Die::new(None, false);
        die.tick(&mut re2, &DieBodies::default());
        die.tick(&mut re2, &DieBodies::default());
        assert_eq!(*re2.fg_stop_mut(), 0xFF00_0000);
    }
}
