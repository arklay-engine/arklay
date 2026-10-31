//! RE2's game over: the `Die` task (`Die_demo_move`'s phases).
//!
//! The white-out, the strobing stops, the sequence's length and the `Die_spr_*`
//! text are the original's. With `censorship_off` clear, `Die_vcut_set` culls
//! everything the room would draw, so `Die_move_set_cut`'s spiralling camera and
//! the room's lights have nothing to show and aren't ported: see
//! `docs/re2-deviations.md`.

use crate::player2::{rcos, rsin};
use crate::scd2::host::Re2State;
use crate::state::Image;

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

/// `Die_enemy_set`: those near `player` lose their collisions (`Be_flg`'s 8) and
/// the rest aren't drawn. The one holding the body is always near.
pub fn enemy_set<'a>(
    characters: impl Iterator<Item = &'a mut crate::scd2::host::Character>,
    player: [i32; 3],
) {
    const BE_HIDDEN: u32 = 0x8_0000;
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
    /// `DWORD_0099cec8`, from `Die_spr_init` on.
    text: Option<Box<DieText>>,
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
            text: None,
        }
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
            out.cry |= !text.cried;
            text.cried = true;
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
                if self.count == 0 {
                    out.load_sounds = true;
                    self.text = Some(Box::new(DieText::new(self.held)));
                }
                self.move_text(re2, &mut out);
                self.count += 1;
                if self.count == MAIN {
                    self.phase = 5;
                    self.count = 0;
                }
            }
            // `Die_move_out`.
            5 => {
                self.move_text(re2, &mut out);
                self.count += 1;
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
        // `Die_sprt_load`, whose file the session reads when the text needs it,
        // and `Bg_set_mode`.
        self.strobe = 0;
        self.count = 0;
        self.white_done = true;
        out.set_enemies = true;
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
    /// `Die_blood_move_main` has played its sound (`+6`).
    cried: bool,
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
            cried: false,
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
        let depth = EYE[1] - i32::from(self.y[column][row]);
        let x = column as i32 * 30 - EYE[0];
        let z = i32::from(self.z[column][row]) - EYE[2];
        let scale = |at: i32| at * FOCAL / depth.max(1);
        [160 + scale(x), 120 + scale(z)]
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
