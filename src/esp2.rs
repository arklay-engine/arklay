//! RE2's effect sprites (`Esp_*`): the street fires and their smoke.
//!
//! A room's RDT holds up to eight effect types, each with a sprite table, a
//! motion table and a TIM (`Esp_data_set0`, `Esp_data_set1`), and
//! `Common/Data/CORE00.ESP` holds the eight every room shares, drawn from
//! `Common/Data/espcore.bin`. `convert-game` writes each set as one
//! [`container`] entry.
//!
//! A slot of the pool runs two behaviours a tick, picked from `Esp_tbl` by the
//! motion block it has reached (`Esp_move`). [`ported`] names the ones here; a
//! slot on any other id still animates and draws, and moves no further.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result, bail};

use crate::model::Texture8;
use crate::render::{Camera, EffectBlend, EffectQuad};
use crate::state::{Image, Sca};

/// `Esp_`'s slots.
pub const POOL_SIZE: usize = 0x60;
/// The room's effect entry, beside its other files.
pub const ROOM_ENTRY: &str = "esp.bin";
/// The shared effect types' entry.
pub const CORE_ENTRY: &str = "esp/core.bin";

/// The size of a motion block, the part of a slot `Esp_01` overwrites.
const BLOCK_LEN: usize = 24;
/// The size of a frame record.
const FRAME_LEN: usize = 8;
/// The size of a tile record.
const TILE_LEN: usize = 4;

const FLAG_ANIMATE: u16 = 0x0001;
const FLAG_INTEGRATE: u16 = 0x0002;
/// `Esp_pos` adds the accumulator before the matrix instead of turning it.
const FLAG_NO_TURN: u16 = 0x0400;
const FLAG_3D: u16 = 0x0200;
const FLAG_SEMI: u16 = 0x1000;
const FLAG_VISIBLE: u16 = 0x2000;
/// `Esp_call3`'s: start on the next tick's second pass.
const FLAG_DEFERRED: u16 = 0x4000;
const FLAG_LIVE: u16 = 0x8000;
/// What `Esp_call` leaves in a new slot.
const FLAGS_NEW: u16 = 0xa003;

/// `Esp_tbl`'s entries with a port here.
pub fn ported(id: u8) -> bool {
    matches!(id, 0..=4 | 6 | 0x0b..=0x0d | 0x19)
}

/// The `kind`s of texture a type's sprites sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sheet {
    /// A TIM of the room's, by its position in the room's table.
    Room(u8),
    /// The n-th TIM of `espcore.bin`, the original's texture page `10 + n`.
    Core(u8),
}

/// One effect type: `pEspdt` and `pEspmv` of an id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sprite {
    frames: Vec<[u8; FRAME_LEN]>,
    tiles: Vec<[u8; TILE_LEN]>,
    /// The clut word of row 0.
    clut: u16,
    /// The tpage word (`puVar10[3]`), whose low byte chose the sheet.
    tpage: u16,
    sheet: Option<Sheet>,
    /// The motion table, from its eight row offsets on.
    motion: Vec<u8>,
}

impl Sprite {
    /// Parse a sprite's record, `data` starting at its first word.
    fn parse(data: &[u8], sheet: Option<Sheet>) -> Result<Self> {
        let word = |at: usize| -> Result<u16> {
            let bytes = data.get(at..at + 2).context("effect record is truncated")?;
            Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
        };
        let (frame_count, tile_count) = (usize::from(word(0)?), usize::from(word(2)?));
        let frames_at = 8;
        let tiles_at = frames_at + frame_count * FRAME_LEN;
        let motion_at = tiles_at + tile_count * TILE_LEN;
        let frames = data
            .get(frames_at..tiles_at)
            .context("effect frames are truncated")?
            .as_chunks::<FRAME_LEN>()
            .0
            .to_vec();
        let tiles = data
            .get(tiles_at..motion_at)
            .context("effect tiles are truncated")?
            .as_chunks::<TILE_LEN>()
            .0
            .to_vec();
        Ok(Self {
            frames,
            tiles,
            clut: word(4)?,
            tpage: word(6)?,
            sheet,
            motion: data
                .get(motion_at..)
                .context("effect motion is truncated")?
                .to_vec(),
        })
    }

    fn frame(&self, frame: u8) -> [u8; FRAME_LEN] {
        self.frames
            .get(usize::from(frame))
            .copied()
            .unwrap_or_default()
    }

    /// A row's blocks, by group: `Esp_call`'s walk of the row's table.
    fn groups(&self, row: u8) -> Vec<usize> {
        let word = |at: usize| -> Option<usize> {
            let bytes = self.motion.get(at..at + 2)?;
            Some(usize::from(u16::from_le_bytes([bytes[0], bytes[1]])))
        };
        let Some(start) = word(usize::from(row & 7) * 2).filter(|&start| start != 0) else {
            return Vec::new();
        };
        let start = start * 4;
        let count = word(start).unwrap_or(0);
        let mut groups = Vec::with_capacity(count);
        let mut group = start + 4;
        for _ in 0..count {
            let Some(blocks) = word(group) else {
                break;
            };
            groups.push(group + 4);
            group += blocks * BLOCK_LEN + 4;
        }
        groups
    }

    fn block(&self, at: usize, index: u8) -> Option<&[u8]> {
        let at = at + usize::from(index) * BLOCK_LEN;
        self.motion.get(at..at + BLOCK_LEN)
    }
}

/// The effect data a room can use, with the sheets decoded.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Bank {
    sprites: HashMap<u8, Sprite>,
    sheets: Vec<(Sheet, Texture8)>,
}

impl Bank {
    /// Read [`container`]s: the shared set, then the room's over it.
    pub fn load(core: Option<&[u8]>, room: Option<&[u8]>) -> Self {
        let mut bank = Self::default();
        for (data, is_core) in [(core, true), (room, false)] {
            let Some(data) = data else {
                continue;
            };
            if let Err(error) = bank.add(data, is_core) {
                eprintln!("warning: invalid effect data: {error:#}");
            }
        }
        bank
    }

    fn add(&mut self, data: &[u8], is_core: bool) -> Result<()> {
        let mut at = 0;
        let mut take = |len: usize| -> Result<&[u8]> {
            let bytes = data
                .get(at..at + len)
                .context("effect container is truncated")?;
            at += len;
            Ok(bytes)
        };
        let count = take(1)?[0];
        let mut tims = Vec::new();
        for slot in 0..count {
            let id = take(1)?[0];
            let len = take(4)?;
            let len = u32::from_le_bytes(len.try_into().unwrap()) as usize;
            let record = take(len)?;
            let len = take(4)?;
            let len = u32::from_le_bytes(len.try_into().unwrap()) as usize;
            let tim = take(len)?;
            if !tim.is_empty() {
                tims.push((slot, tim));
            }
            // In the room's data the n-th TIM is texture `n`. The shared set
            // names one of the pages `espcore.bin` fills, and each tile says
            // which TIM of them (`core_tile`).
            let sheet = if is_core {
                let page = record.get(6).copied().unwrap_or(0) & 0x1f;
                CORE_PAGES
                    .contains(&page)
                    .then_some(Sheet::Core(page - CORE_PAGES.start()))
            } else {
                Some(Sheet::Room(slot))
            };
            let mut sprite = Sprite::parse(record, sheet)?;
            // `Esp_data_set1`: a room's own colours start at row 0 of its TIM.
            if !is_core {
                sprite.clut = 0;
            }
            self.sprites.insert(id, sprite);
        }
        for (slot, mut tim) in tims {
            if !is_core {
                self.add_sheet(Sheet::Room(slot), tim);
                continue;
            }
            // `Init_main` uploads `espcore.bin`'s TIMs one after another.
            let mut index = 0;
            while let Some(len) = tim_len(tim).filter(|&len| len <= tim.len()) {
                let (this, rest) = tim.split_at(len);
                self.add_sheet(Sheet::Core(index), this);
                tim = rest;
                index += 1;
            }
        }
        Ok(())
    }

    fn add_sheet(&mut self, sheet: Sheet, tim: &[u8]) {
        match crate::tim::decode_4bpp(tim) {
            Ok(texture) => self.sheets.push((sheet, texture)),
            Err(error) => eprintln!("warning: invalid effect texture: {error:#}"),
        }
    }

    fn sheet(&self, sheet: Sheet) -> Option<&Texture8> {
        self.sheets
            .iter()
            .find(|(id, _)| *id == sheet)
            .map(|(_, texture)| texture)
    }

    /// A sheet with a clut row's colours.
    pub fn page(&self, page: Page) -> Option<Image> {
        let texture = self.sheet(page.sheet)?;
        let (width, height) = (texture.width, texture.height);
        let row = usize::from(page.row);
        let mut rgba = Vec::with_capacity(texture.indices.len() * 4);
        for &index in &texture.indices {
            let mut colour = texture.palette(row, index);
            // Colour 0 draws nothing.
            if colour[..3] == [0, 0, 0] && !texture.palette_stp(row, index) {
                colour[3] = 0;
            }
            rgba.extend_from_slice(&colour);
        }
        Some(Image {
            width,
            height,
            rgba,
        })
    }
}

/// The tpages `espcore.bin` is drawn from, which on the PlayStation hold all its
/// TIMs.
const CORE_PAGES: std::ops::RangeInclusive<u8> = 0x1e..=0x1f;

/// A texture a billboard samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Page {
    pub sheet: Sheet,
    pub row: u8,
}

/// `Esp_trans_main`'s choice of texture for a tile (its `Graphicsprdata == 0`
/// case): the page, the clut row and the `v` in that TIM. `sheet` carries over
/// from one of a frame's tiles to the next, as the original's page does: what's
/// past the first TIM makes the frame's later tiles look in the second page.
fn core_tile(sheet: &mut Sheet, clut: u16, v: u8) -> (u8, u8) {
    let Sheet::Core(page) = *sheet else {
        return ((clut >> 6) as u8, v);
    };
    let clut = clut >> 6;
    let row = |first: u16| clut.wrapping_sub(first) as u8;
    let (next, row, v) = match (page, v) {
        (0, 0xa8..) => (1, row(0x1e4), v.wrapping_add(0x58)),
        (0, _) => (0, row(0x1e0), v),
        (_, ..0x58) => (2, row(0x1e8), v),
        (_, ..0xa8) => (3, row(0x1ec), v.wrapping_add(0xa8)),
        (_, ..0xd0) => (4, 0, v.wrapping_add(0x58)),
        _ => (5, 0, v.wrapping_add(0x30)),
    };
    *sheet = Sheet::Core(next);
    let row = match (page, row) {
        (0, 10) => 0,
        (1.., 5) if next == 3 => 4,
        _ => row,
    };
    (row, v)
}

/// A live slot of the pool.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Slot {
    /// The behaviours (`+0`, `+1`).
    anim: u8,
    update: u8,
    /// `+2`: the first frame, or a behaviour's own count.
    start: u8,
    /// `+3`: a behaviour's countdown.
    count: u8,
    /// `+4`, `+6`: the width and height, in 1/4096ths of the size.
    scale: [u16; 2],
    /// `+8`: added to the velocity.
    accel: [i8; 3],
    /// `+0xb`: how far `Esp_01` moves along the group.
    step: u8,
    /// `+0xc`: added to the accumulator.
    velocity: [i16; 3],
    /// `+0x12`, `+0x14`, `+0x16`: `Esp_01`'s flags, tpage bits, and `Esp_02`'s
    /// range of first frames.
    next_flags: u16,
    next_tpage: u16,
    range: u16,
    /// `+0x18`.
    flags: u16,
    /// `+0x1a`: the slot this one dies with, 0 for none.
    parent: u8,
    /// `+0x1b`: how many generations `Esp_0C` has spawned.
    generation: u8,
    /// `+0x1c`, `+0x1e`: what `Esp_call` was asked for.
    kind: u8,
    row: u8,
    /// `+0x1f`: the block reached within `blocks`.
    block: u8,
    /// `+0x20`: ticks left on this frame.
    delay: u8,
    /// `+0x21`.
    frame: u8,
    /// `+0x22`.
    dir: i16,
    /// `+0x24`: the offset from `base`.
    offset: [i16; 3],
    /// `+0x2a`.
    tpage: u16,
    /// `+0x2c`: where it was spawned.
    base: [i16; 3],
    /// `+0x32`.
    clut: u16,
    /// `+0x34`: where it's drawn.
    pos: [i16; 3],
    /// `+0x3a`: the size from the script.
    size: u16,
    /// `+0x78`: this slot's group of blocks, in the motion table.
    blocks: usize,
    /// `+0x14` while it falls: `Sca_ck_hit`'s height, under it at the last look.
    ground: i32,
}

impl Slot {
    fn live(&self) -> bool {
        self.flags & FLAG_LIVE != 0
    }

    /// The first half of a slot's death: `Esp_kill`.
    fn kill(&mut self) {
        self.anim = 0;
        self.update = 0;
        self.flags = 0;
    }

    fn load_block(&mut self, block: &[u8]) {
        let word = |at: usize| u16::from_le_bytes([block[at], block[at + 1]]);
        self.anim = block[0];
        self.update = block[1];
        self.start = block[2];
        self.count = block[3];
        self.scale = [word(4), word(6)];
        self.accel = [block[8] as i8, block[9] as i8, block[10] as i8];
        self.step = block[11];
        self.velocity = [word(12) as i16, word(14) as i16, word(16) as i16];
        self.next_flags = word(18);
        self.next_tpage = word(20);
        self.range = word(22);
    }
}

/// `Esp_`, and the data it draws on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Effects {
    bank: Arc<Bank>,
    slots: Vec<Slot>,
    /// `Rnd`'s stand-in. The original shares its stream with the rest of the
    /// game; effects here don't disturb any stream that's compared.
    /// `Rnd`, for the length of a tick.
    random: crate::zombie2::Rnd,
    unported: u128,
    /// `Snd_se_on`s since the last tick was taken.
    pub sounds: Vec<(u32, [i16; 3])>,
}

impl Effects {
    /// `Esp_init_R`: an empty pool on a room's data.
    pub fn enter_room(&mut self, bank: Arc<Bank>) {
        self.bank = bank;
        self.slots = vec![Slot::default(); POOL_SIZE];
    }

    pub fn bank(&self) -> &Arc<Bank> {
        &self.bank
    }

    fn rnd(&mut self) -> u8 {
        self.random.draw()
    }

    /// `Esp_call`, or `Esp_call3` if `deferred`. The pool is searched from the
    /// top, the way the original's is, and each further group of the row gets
    /// a slot of its own that dies with the first.
    pub fn call(
        &mut self,
        kind: u8,
        row: u8,
        size: u16,
        dir: i16,
        at: [i16; 3],
        deferred: bool,
    ) -> Option<usize> {
        let bank = Arc::clone(&self.bank);
        let sprite = bank.sprites.get(&kind)?;
        let groups = sprite.groups(row);
        let (&first_group, rest) = groups.split_first()?;
        let first = self.free_below(self.slots.len())?;
        let mut slot = Slot {
            flags: if deferred { FLAG_DEFERRED } else { FLAGS_NEW },
            kind,
            row,
            delay: sprite.frame(0)[2],
            dir,
            tpage: sprite.tpage,
            base: at,
            clut: sprite.clut.wrapping_add(u16::from(row & 0xf8) << 3),
            size,
            blocks: first_group,
            ..Slot::default()
        };
        if let Some(block) = sprite.block(first_group, 0) {
            slot.load_block(block);
        }
        self.slots[first] = slot;
        let mut below = first;
        for &group in rest.iter().rev() {
            let Some(index) = self.free_below(below) else {
                break;
            };
            below = index;
            let mut other = slot;
            other.parent = first as u8;
            other.blocks = group;
            if let Some(block) = sprite.block(group, 0) {
                other.load_block(block);
            }
            self.slots[index] = other;
        }
        Some(first)
    }

    /// A slot and those that die with it follow a matrix `Esp_call` was given.
    pub fn move_to(&mut self, index: usize, at: [i16; 3]) {
        for (other, slot) in self.slots.iter_mut().enumerate() {
            if other == index || usize::from(slot.parent) == index && slot.parent != 0 {
                slot.base = at;
            }
        }
    }

    fn free_below(&self, top: usize) -> Option<usize> {
        (0..top).rev().find(|&index| self.slots[index].flags == 0)
    }

    /// `Esp_kill`, of what's spawned at the world's origin.
    pub fn kill(&mut self, kind: u8, row: u8) {
        for slot in &mut self.slots {
            if slot.kind == kind && slot.row == row {
                slot.kill();
            }
        }
    }

    /// `Esp_01`, or `Esp_02` with a random first frame: on to the next block.
    fn next_block(&mut self, index: usize, random_frame: bool) {
        let slot = self.slots[index];
        let bank = Arc::clone(&self.bank);
        let Some(sprite) = bank.sprites.get(&slot.kind) else {
            return;
        };
        let frame = if random_frame {
            let range = u32::from(slot.range) + 1;
            (u32::from(self.rnd()) % range) as u8
        } else {
            0
        };
        let slot = &mut self.slots[index];
        slot.flags = slot.next_flags;
        slot.tpage |= slot.next_tpage;
        slot.frame = frame.wrapping_add(slot.start);
        slot.delay = sprite.frame(slot.frame)[2];
        slot.block = slot.block.wrapping_add(slot.step);
        if let Some(block) = sprite.block(slot.blocks, slot.block) {
            slot.load_block(block);
        }
    }

    /// `Esp_tbl[id]`.
    fn behave(&mut self, index: usize, id: u8, sca: Option<&Sca>) {
        let slot = self.slots[index];
        match id {
            0 => {}
            1 => self.next_block(index, false),
            2 => self.next_block(index, true),
            3 => {
                if slot.count != 0 {
                    self.slots[index].count -= 1;
                    return;
                }
                let bank = Arc::clone(&self.bank);
                let Some(sprite) = bank.sprites.get(&slot.kind) else {
                    return;
                };
                let slot = &mut self.slots[index];
                slot.block = slot.block.wrapping_add(slot.step);
                if let Some(block) = sprite.block(slot.blocks, slot.block) {
                    slot.load_block(block);
                }
            }
            4 => {
                if slot.count == 0 {
                    self.next_block(index, false);
                } else {
                    self.slots[index].count -= 1;
                }
            }
            6 => {
                if slot.start == 0 {
                    self.slots[index].kill();
                } else {
                    self.slots[index].start -= 1;
                }
            }
            0x0b => {
                self.next_block(index, false);
                let slot = &mut self.slots[index];
                let size = i32::from(slot.size);
                for accel in &mut slot.accel {
                    *accel = (size * i32::from(*accel) / 3000) as i8;
                }
                for velocity in &mut slot.velocity {
                    *velocity = (size * i32::from(*velocity) / 3000) as i16;
                }
            }
            0x0c => self.smoke_trail(index),
            0x0d => {
                let slot = &mut self.slots[index];
                slot.flags = FLAGS_NEW;
                slot.anim = 0;
            }
            0x0a => {
                let frame = slot.next_flags as u8;
                let delay = self
                    .bank
                    .sprites
                    .get(&slot.kind)
                    .map_or(0, |s| s.frame(frame)[2]);
                let this = &mut self.slots[index];
                this.flags = FLAG_LIVE | FLAG_ANIMATE | FLAG_INTEGRATE;
                this.frame = frame;
                this.delay = delay;
                if slot.row != 6 {
                    self.slots[index].velocity[2] = i16::from(self.rnd() & 3) * 5;
                    let down = i16::from(self.rnd() % 3) * -10;
                    let velocity = &mut self.slots[index].velocity[1];
                    *velocity = velocity.wrapping_add(down);
                }
            }
            // `Esp_07`: on to a further row of colours, with a sound.
            7 => {
                let this = &mut self.slots[index];
                this.row = this.row.wrapping_add(this.count);
                let id = if slot.generation == 0 {
                    u32::from(slot.range) << 16 | 1
                } else {
                    0x31F_0001
                };
                self.sounds.push((id, slot.pos));
                self.next_block(index, false);
            }
            // `Esp_05`, without `Water_ck`'s splash: what's falling reaches the
            // ground, or something it was above.
            5 => {
                let ground = Self::ground(sca, slot.pos);
                let y = i32::from(slot.pos[1]);
                if ground.height < y {
                    self.behave(index, slot.start, sca);
                } else if ground.blocked {
                    let id = if ground.height == slot.ground {
                        slot.count
                    } else {
                        slot.start
                    };
                    self.behave(index, id, sca);
                } else {
                    self.slots[index].ground = ground.height;
                }
            }
            // `Esp_10`: start falling, on the ground below.
            0x10 => {
                let ground = Self::ground(sca, slot.pos).height;
                let this = &mut self.slots[index];
                this.ground = ground;
                this.anim = this.step;
                this.update = 5;
            }
            0x23 => self.slots[index].kill(),
            // A falling case's first and second bounce.
            0x2c => {
                let this = &mut self.slots[index];
                this.start = 0x23;
                this.velocity[0] = (i32::from(this.velocity[0]) * 0x3C / 100) as i16;
                this.velocity[1] = (i32::from(this.velocity[1]) * -0x28 / 100) as i16;
                self.bounce(index);
            }
            0x2d => {
                let this = &mut self.slots[index];
                this.count = 0x23;
                this.velocity[0] = this.velocity[0].wrapping_neg();
                self.bounce(index);
            }
            // `Esp_08`: the next block, scattered. `+0x44` and `+0x46` aren't
            // used by anything drawn, but drawing them moves the stream.
            8 => {
                self.next_block(index, false);
                let wider = u16::from(self.rnd() % 5) << 9;
                let on = self.rnd() & 1;
                self.rnd();
                self.rnd();
                let this = self.slots[index];
                let frame = this.frame.wrapping_add(on);
                let delay = self
                    .bank
                    .sprites
                    .get(&this.kind)
                    .map_or(0, |s| s.frame(frame)[2]);
                let this = &mut self.slots[index];
                this.scale[0] = this.scale[0].wrapping_add(wider);
                this.frame = frame;
                this.delay = delay;
                for (base, by) in this
                    .base
                    .iter_mut()
                    .zip([this.start, this.count, this.step])
                {
                    *base = base.wrapping_add(i16::from(by as i8));
                }
            }
            // `Esp_09`: wider as it falls, gone on touching the ground.
            9 => {
                self.slots[index].scale[0] = slot.scale[0].wrapping_add(0x100);
                if Self::ground(sca, slot.pos).blocked {
                    self.slots[index].kill();
                }
            }
            // `Esp_25`: stopped on its last frame.
            0x25 => {
                let delay = self
                    .bank
                    .sprites
                    .get(&slot.kind)
                    .map_or(0, |s| s.frame(6)[2]);
                let this = &mut self.slots[index];
                this.anim = 0;
                this.update = 0;
                this.start = 0;
                this.count = 0;
                this.accel[0] = 0;
                this.accel[1] = 0;
                this.velocity[0] = 0;
                this.velocity[1] = 0;
                this.frame = 6;
                this.delay = delay;
                this.flags &= 0xA001;
            }
            // `Esp_26`.
            0x26 => {
                let this = &mut self.slots[index];
                this.count = 0;
                this.velocity[0] = 0;
                this.accel[0] = 0;
            }
            // `Esp_2A`: then two more.
            0x2A => {
                for (step, size) in [(3, 0xC00u16), (4, 0xA00)] {
                    let dir = u16::from(self.rnd()) << 4;
                    let row = slot.start.wrapping_add(step);
                    self.call(9, row, size, dir as i16, slot.base, true);
                }
                self.slots[index].anim = 0;
            }
            // `Esp_29`: bursts into three puffs of the next rows.
            0x29 => {
                for (step, size) in [0x800u16, 0x1000, 0xC00].into_iter().enumerate() {
                    let dir = u16::from(self.rnd()) << 4;
                    let row = slot.start.wrapping_add(step as u8);
                    self.call(9, row, size, dir as i16, slot.base, true);
                }
                self.slots[index].anim = 0x2A;
            }
            // `Esp_14`: gone on touching the ground.
            0x14 => {
                if Self::ground(sca, slot.pos).blocked {
                    self.slots[index].kill();
                }
            }
            // `Esp_19` puts out a fire in water, which isn't ported.
            0x19 => {}
            _ => {
                if id < 128 && self.unported & 1 << id == 0 {
                    self.unported |= 1 << id;
                    eprintln!("note: effect behaviour 0x{id:02x} isn't ported");
                }
            }
        }
    }

    /// `Sca_ck_hit`, at what's being tested for a landing.
    fn ground(sca: Option<&Sca>, pos: [i16; 3]) -> crate::sca::Ground {
        let pos = pos.map(i32::from);
        sca.map_or_else(
            || crate::sca::Ground {
                blocked: pos[1] > 0,
                ..Default::default()
            },
            |sca| crate::sca::ground_at(sca, pos, 2, 0x2000),
        )
    }

    /// The end of `Esp_2C` and `Esp_2D`: a little sideways, and the sound of it.
    fn bounce(&mut self, index: usize) {
        self.slots[index].velocity[2] = i16::from(self.rnd() & 3) * 3;
        let slot = &self.slots[index];
        self.sounds
            .push((u32::from(slot.next_flags) << 16 | 1, slot.pos));
    }

    /// `Esp_0C`: leave more of the same behind at one frame, up to ten times,
    /// the later ones smaller, and shrink and stop rising near the end.
    fn smoke_trail(&mut self, index: usize) {
        let slot = self.slots[index];
        let at_frame = slot.frame == slot.start;
        let child = |this: &mut Self, size: u16| {
            if let Some(child) = this.call(slot.kind, slot.row, size, slot.dir, slot.base, true) {
                this.slots[child].generation = this.slots[index].generation;
            }
        };
        if slot.step == 0 {
            if at_frame {
                child(self, slot.size);
            }
            return;
        }
        if (slot.generation as i8) < 10 && at_frame {
            self.slots[index].generation += 1;
            let size = if self.slots[index].generation < 6 {
                slot.size
            } else {
                (u32::from(slot.size) * 800 / 1000) as u16
            };
            child(self, size);
        }
        let slot = &mut self.slots[index];
        if slot.frame > 0x12 {
            slot.size = (u32::from(slot.size) * 0x3d4 / 1000) as u16;
            slot.accel[1] = 0;
        }
        if slot.frame > 0x14 {
            slot.velocity[1] = 0;
        }
    }

    /// `Esp_pos`, on the world's matrix.
    fn place(slot: &mut Slot) {
        let [x, y, z] = slot.offset.map(i32::from);
        let [bx, by, bz] = slot.base;
        if slot.flags & FLAG_NO_TURN != 0 {
            slot.pos = [
                bx.wrapping_add(x as i16),
                by.wrapping_add(y as i16),
                bz.wrapping_add(z as i16),
            ];
            return;
        }
        let (sin, cos) = (
            crate::player2::rsin(i32::from(slot.dir)),
            crate::player2::rcos(i32::from(slot.dir)),
        );
        let tx = (cos * x + sin * z) >> 12;
        let tz = (cos * z - sin * x) >> 12;
        slot.pos = [
            bx.wrapping_add(tx as i16),
            by.wrapping_add(y as i16),
            bz.wrapping_add(tz as i16),
        ];
    }

    /// The second half of a slot's tick (`FUN_004b9590`).
    fn advance(&mut self, index: usize, sca: Option<&Sca>) {
        Self::place(&mut self.slots[index]);
        let update = self.slots[index].update;
        self.behave(index, update, sca);
        let bank = Arc::clone(&self.bank);
        let slot = &mut self.slots[index];
        if slot.flags & FLAG_INTEGRATE != 0 {
            for axis in 0..3 {
                slot.offset[axis] = slot.offset[axis].wrapping_add(slot.velocity[axis]);
                slot.velocity[axis] = slot.velocity[axis].wrapping_add(i16::from(slot.accel[axis]));
            }
        }
        if slot.flags & FLAG_ANIMATE == 0 {
            return;
        }
        let Some(sprite) = bank.sprites.get(&slot.kind) else {
            return;
        };
        if slot.delay == 0 {
            slot.frame = slot.frame.wrapping_add(1);
            let mut frame = sprite.frame(slot.frame);
            if frame[2] == 0 {
                if frame[0] == 0 {
                    slot.kill();
                    return;
                }
            } else if frame[2] == 0xff {
                slot.frame = frame[0];
                frame = sprite.frame(slot.frame);
            }
            slot.delay = frame[2];
        }
        slot.delay = slot.delay.wrapping_sub(1);
    }

    /// `Esp_move`: one tick, drawing on the game's `Rnd`.
    pub fn tick(&mut self, rnd: &mut crate::zombie2::Rnd, sca: Option<&Sca>) {
        self.random = *rnd;
        self.move_slots(sca);
        *rnd = self.random;
    }

    fn move_slots(&mut self, sca: Option<&Sca>) {
        for index in 0..self.slots.len() {
            let slot = self.slots[index];
            if !slot.live() {
                continue;
            }
            if slot.parent != 0 && self.slots[usize::from(slot.parent)].flags == 0 {
                self.slots[index].kill();
            }
            let anim = self.slots[index].anim;
            self.behave(index, anim, sca);
        }
        for index in 0..self.slots.len() {
            if self.slots[index].flags & FLAG_DEFERRED != 0 {
                self.slots[index].flags = FLAGS_NEW;
                let anim = self.slots[index].anim;
                self.behave(index, anim, sca);
            }
            if self.slots[index].live() {
                self.advance(index, sca);
            }
        }
    }

    /// What the slots that can draw sample.
    pub fn pages(&self) -> Vec<Page> {
        let mut pages = Vec::new();
        for slot in &self.slots {
            if slot.flags & FLAG_VISIBLE == 0 {
                continue;
            }
            let Some(sprite) = self.bank.sprites.get(&slot.kind) else {
                continue;
            };
            let Some(mut sheet) = sprite.sheet else {
                continue;
            };
            let frame = sprite.frame(slot.frame);
            let (first, count) = (usize::from(frame[0]), usize::from(frame[1]));
            for tile in sprite.tiles.iter().skip(first).take(count) {
                let (row, _) = core_tile(&mut sheet, slot.clut, tile[1]);
                let page = Page { sheet, row };
                if !pages.contains(&page) {
                    pages.push(page);
                }
            }
        }
        pages
    }

    /// `Eff_sprt_trans` and `Esp_trans_main`: this frame's billboards, each
    /// frame's tiles one quad, and the pages they sample. `in_view` is
    /// `Hit_ck_point4` on the cut. The 3D ones (`Esp_trans_3d`) aren't drawn.
    pub fn quads(
        &self,
        camera: &Camera,
        in_view: impl Fn(i32, i32) -> bool,
    ) -> (Vec<EffectQuad>, Vec<Page>) {
        let mut quads = Vec::new();
        let mut pages = Vec::new();
        for slot in self.slots.iter().rev() {
            if slot.flags & (FLAG_LIVE | FLAG_VISIBLE) != FLAG_LIVE | FLAG_VISIBLE
                || slot.flags & FLAG_3D != 0
            {
                continue;
            }
            let [x, y, z] = slot.pos.map(i32::from);
            if !in_view(x, z) {
                continue;
            }
            let Some(sprite) = self.bank.sprites.get(&slot.kind) else {
                continue;
            };
            let Some(mut sheet) = sprite.sheet else {
                continue;
            };
            let frame = sprite.frame(slot.frame);
            let (first, count, size) = (
                usize::from(frame[0]),
                usize::from(frame[1]),
                u32::from(frame[3]),
            );
            if count == 0 || size == 0 {
                continue;
            }
            let depth = camera.view_position([x, y, z])[2];
            if depth < 0x200 {
                continue;
            }
            let Some([screen_x, screen_y]) = camera.project([x, y, z]) else {
                continue;
            };
            let per_size = i64::from(slot.size) * i64::from(size) * i64::from(camera.fov)
                / (i64::from(depth) << 4);
            let width = i64::from(slot.scale[0]) * per_size;
            let height = i64::from(slot.scale[1]) * per_size;
            let texel = [width / i64::from(size), height / i64::from(size)];
            let source = [
                size - u32::from(texel[0] > 0x1ffff),
                size - u32::from(texel[1] > 0x1ffff),
            ];
            let (blend, op) = if slot.flags & FLAG_SEMI == 0 {
                (0, EffectBlend::Normal)
            } else {
                // The PC's table has only the two (`DWORD_00524e24`).
                match (slot.tpage >> 5) & 3 {
                    1 => (0, EffectBlend::Add),
                    _ => (0x80, EffectBlend::Normal),
                }
            };
            for tile in sprite.tiles.iter().skip(first).take(count) {
                let (row, v) = core_tile(&mut sheet, slot.clut, tile[1]);
                let page = Page { sheet, row };
                let page_index = pages
                    .iter()
                    .position(|known| *known == page)
                    .unwrap_or_else(|| {
                        pages.push(page);
                        pages.len() - 1
                    });
                let left = (i64::from(screen_x) << 16) + i64::from(tile[2] as i8) * texel[0];
                let top = (i64::from(screen_y) << 16) + i64::from(tile[3] as i8) * texel[1];
                let (x0, y0) = ((left >> 16) as i32, (top >> 16) as i32);
                let (x1, y1) = (((left + width) >> 16) as i32, ((top + height) >> 16) as i32);
                quads.push(EffectQuad {
                    key: ((depth.min(0x7fff) as u32) >> 5) << 5,
                    page: page_index,
                    uv: [tile[0], v, 0, 0],
                    source: [source[0] as u8, source[1] as u8],
                    pos: [x0, y0],
                    size: [x1 - x0, y1 - y0],
                    tint: [0xff; 3],
                    blend,
                    op,
                    sheet: [0, 0xffff],
                });
            }
        }
        // The order table is last in, first out: what's submitted last draws
        // first among equal depths, so a fire covers the smoke spawned after it.
        quads.reverse();
        (quads, pages)
    }
}

/// The record lengths of a set: each runs up to the next, in file order, and
/// the last up to `end`.
fn record_ranges(starts: &[usize], end: usize) -> Vec<(usize, usize)> {
    starts
        .iter()
        .map(|&start| {
            let next = starts.iter().copied().filter(|&other| other > start).min();
            (start, next.unwrap_or(end))
        })
        .collect()
}

/// A TIM's whole length.
fn tim_len(data: &[u8]) -> Option<usize> {
    let word = |at: usize| -> Option<usize> {
        let bytes = data.get(at..at + 4)?;
        Some(u32::from_le_bytes(bytes.try_into().ok()?) as usize)
    };
    if word(0)? != 0x10 {
        return None;
    }
    let flags = word(4)?;
    let clut = if flags & 8 != 0 { word(8)? } else { 0 };
    Some(8 + clut + word(8 + clut)?)
}

/// One entry of [`container`].
pub struct Part<'a> {
    pub id: u8,
    pub record: &'a [u8],
    pub tim: &'a [u8],
}

/// What `convert-game` writes for a set: the count, then each type's id, its
/// record and its TIM, each with its length.
pub fn container(parts: &[Part<'_>]) -> Vec<u8> {
    let mut out = vec![parts.len() as u8];
    for part in parts {
        out.push(part.id);
        out.extend_from_slice(&(part.record.len() as u32).to_le_bytes());
        out.extend_from_slice(part.record);
        out.extend_from_slice(&(part.tim.len() as u32).to_le_bytes());
        out.extend_from_slice(part.tim);
    }
    out
}

/// The tables of `Esp_data_set0`: `data[index..]` the ids up to 0xff and at most
/// eight, and a table of offsets from `base`, running backward from `table`.
fn sprite_records(
    data: &[u8],
    index: usize,
    table: usize,
    base: usize,
) -> Result<Vec<(u8, usize)>> {
    let mut records = Vec::new();
    for slot in 0..8 {
        let id = *data.get(index + slot).context("effect ids are truncated")?;
        if id == 0xff {
            break;
        }
        let at = table
            .checked_sub(slot * 4)
            .context("effect table underflows")?;
        let bytes = data.get(at..at + 4).context("effect table is truncated")?;
        let offset = u32::from_le_bytes(bytes.try_into().unwrap()) as usize;
        records.push((id, base + offset));
    }
    Ok(records)
}

/// A room's [`container`] from its RDT's slots (`rdt + 0x50` on): the id table,
/// the records' offsets, the TIMs' base and the TIMs' offsets. `None` for a room
/// with no effects.
pub fn room_container(data: &[u8], slots: [u32; 4]) -> Result<Option<Vec<u8>>> {
    let [ids, records, tims, tim_table] = slots.map(|slot| slot as usize);
    if ids == 0 || records == 0 {
        return Ok(None);
    }
    let found = sprite_records(data, ids, records, ids)?;
    if found.is_empty() {
        return Ok(None);
    }
    let table_start = records + 4 - found.len() * 4;
    let starts: Vec<usize> = found.iter().map(|&(_, at)| at).collect();
    let ranges = record_ranges(&starts, table_start);
    let mut parts = Vec::new();
    for (slot, (&(id, _), &(start, end))) in found.iter().zip(&ranges).enumerate() {
        let record = data
            .get(start..end)
            .with_context(|| format!("effect record {slot} is truncated"))?;
        let tim = if tims == 0 || tim_table == 0 {
            &[][..]
        } else {
            let at = tim_table - (slot + 1) * 4;
            let bytes = data
                .get(at..at + 4)
                .context("effect TIM table is truncated")?;
            let start = tims + u32::from_le_bytes(bytes.try_into().unwrap()) as usize;
            let Some(len) = data.get(start..).and_then(tim_len) else {
                bail!("effect TIM {slot} is invalid");
            };
            data.get(start..start + len)
                .with_context(|| format!("effect TIM {slot} is truncated"))?
        };
        parts.push(Part { id, record, tim });
    }
    Ok(Some(container(&parts)))
}

/// The shared [`container`] from `CORE00.ESP`, whose offsets run backward from
/// the file's end (`Esp_init_C`), and `espcore.bin`.
pub fn core_container(esp: &[u8], sheet: &[u8]) -> Result<Vec<u8>> {
    let end = esp.len() & !3;
    let last = end.checked_sub(4).context("CORE00.ESP is truncated")?;
    let found = sprite_records(esp, 0, last, 0)?;
    let table_start = end - found.len() * 4;
    let starts: Vec<usize> = found.iter().map(|&(_, at)| at).collect();
    let ranges = record_ranges(&starts, table_start);
    let mut parts = Vec::new();
    for (slot, (&(id, _), &(start, end))) in found.iter().zip(&ranges).enumerate() {
        let record = esp
            .get(start..end)
            .with_context(|| format!("shared effect record {slot} is truncated"))?;
        parts.push(Part {
            id,
            record,
            tim: if slot == 0 { sheet } else { &[] },
        });
    }
    Ok(container(&parts))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-type bank: two frames, one tile, and a row with one group of
    /// `blocks`.
    fn bank(blocks: &[[u8; BLOCK_LEN]]) -> Arc<Bank> {
        let mut record = Vec::new();
        record.extend_from_slice(&3u16.to_le_bytes());
        record.extend_from_slice(&1u16.to_le_bytes());
        record.extend_from_slice(&0u16.to_le_bytes());
        record.extend_from_slice(&0u16.to_le_bytes());
        // Frames: tile 0, one tile, two ticks; then the end of the loop.
        record.extend_from_slice(&[0, 1, 2, 16, 0, 0, 0, 0]);
        record.extend_from_slice(&[0, 1, 2, 16, 0, 0, 0, 0]);
        record.extend_from_slice(&[0, 0, 0xff, 0, 0, 0, 0, 0]);
        record.extend_from_slice(&[0, 0, 0xf8, 0xf8]);
        // Row 0 at word 4 of the motion table.
        for row in 0..8 {
            record.extend_from_slice(&(if row == 0 { 4u16 } else { 0 }).to_le_bytes());
        }
        record.extend_from_slice(&1u16.to_le_bytes());
        record.extend_from_slice(&0u16.to_le_bytes());
        record.extend_from_slice(&(blocks.len() as u16).to_le_bytes());
        record.extend_from_slice(&0u16.to_le_bytes());
        for block in blocks {
            record.extend_from_slice(block);
        }
        let data = container(&[Part {
            id: 7,
            record: &record,
            tim: &[],
        }]);
        Arc::new(Bank::load(None, Some(&data)))
    }

    fn fire_block() -> [u8; BLOCK_LEN] {
        let mut block = [0u8; BLOCK_LEN];
        block[0] = 2;
        block[4..6].copy_from_slice(&0x1000u16.to_le_bytes());
        block[6..8].copy_from_slice(&0x1000u16.to_le_bytes());
        block[11] = 1;
        block[18..20].copy_from_slice(&0xb003u16.to_le_bytes());
        block[20..22].copy_from_slice(&0x20u16.to_le_bytes());
        block[22..24].copy_from_slice(&1u16.to_le_bytes());
        block
    }

    #[test]
    fn a_slot_takes_its_flags_and_blend_from_the_first_block() {
        let mut held = [0u8; BLOCK_LEN];
        held[4..6].copy_from_slice(&0x1000u16.to_le_bytes());
        held[6..8].copy_from_slice(&0x1000u16.to_le_bytes());
        let mut effects = Effects::default();
        effects.enter_room(bank(&[fire_block(), held]));
        let slot = effects.call(7, 0, 0x1000, 0, [100, 0, 0], false).unwrap();
        assert_eq!(slot, POOL_SIZE - 1);
        effects.tick(&mut Default::default(), None);
        let slot = effects.slots[slot];
        assert_eq!(slot.flags, 0xb003);
        assert_eq!(slot.tpage & 0x60, 0x20);
        assert_eq!(slot.anim, 0);
        assert_eq!(slot.pos, [100, 0, 0]);
    }

    #[test]
    fn the_animation_loops_at_a_jump_frame() {
        let mut effects = Effects::default();
        effects.enter_room(bank(&[fire_block(), [0; BLOCK_LEN]]));
        let slot = effects.call(7, 0, 0x1000, 0, [0, 0, 0], false).unwrap();
        for _ in 0..20 {
            effects.tick(&mut Default::default(), None);
            assert!(effects.slots[slot].frame < 2);
            assert!(effects.slots[slot].live());
        }
    }

    #[test]
    fn a_falling_case_bounces_off_the_ground_with_a_sound_then_goes() {
        let mut block = [0u8; BLOCK_LEN];
        block[0] = 0x0A;
        block[1] = 0x10;
        block[2] = 0x2C;
        block[3] = 0x2D;
        block[11] = 0x0D;
        block[14..16].copy_from_slice(&40i16.to_le_bytes());
        block[18..20].copy_from_slice(&0x0102u16.to_le_bytes());
        let mut effects = Effects::default();
        effects.enter_room(bank(&[block]));
        let slot = effects.call(7, 0, 0x700, 0, [0, -100, 0], false).unwrap();
        effects.tick(&mut Default::default(), None);
        assert_eq!(effects.slots[slot].anim, 0x0D);
        assert_eq!(effects.slots[slot].update, 5);
        assert!(effects.sounds.is_empty());
        let mut rising = false;
        for _ in 0..20 {
            effects.tick(&mut Default::default(), None);
            rising |= effects.slots[slot].velocity[1] < 0;
            if rising {
                break;
            }
        }
        assert!(rising);
        assert_eq!(effects.slots[slot].start, 0x23);
        assert_eq!(effects.sounds.len(), 1);
        assert_eq!(effects.sounds[0].0, 0x0102_0001);
        for _ in 0..40 {
            effects.tick(&mut Default::default(), None);
        }
        assert!(!effects.slots[slot].live());
    }

    #[test]
    fn a_puff_widens_as_it_falls_and_goes_on_the_ground() {
        let mut block = [0u8; BLOCK_LEN];
        block[0] = 9;
        block[14..16].copy_from_slice(&40i16.to_le_bytes());
        let mut effects = Effects::default();
        effects.enter_room(bank(&[block]));
        let slot = effects.call(7, 0, 0x1000, 0, [0, -1000, 0], false).unwrap();
        effects.tick(&mut Default::default(), None);
        let scale = effects.slots[slot].scale[0];
        effects.tick(&mut Default::default(), None);
        assert_eq!(effects.slots[slot].scale[0], scale.wrapping_add(0x100));
        for _ in 0..100 {
            effects.tick(&mut Default::default(), None);
        }
        assert!(!effects.slots[slot].live());
    }

    #[test]
    fn a_kill_takes_the_row_it_names() {
        let mut effects = Effects::default();
        effects.enter_room(bank(&[fire_block(), [0; BLOCK_LEN]]));
        let slot = effects.call(7, 0, 0x1000, 0, [0, 0, 0], false).unwrap();
        effects.kill(7, 1);
        assert!(effects.slots[slot].live());
        effects.kill(7, 0);
        assert!(!effects.slots[slot].live());
    }

    #[test]
    fn a_page_takes_the_colours_of_its_clut_row() {
        let mut tim = Vec::new();
        tim.extend_from_slice(&0x10u32.to_le_bytes());
        tim.extend_from_slice(&8u32.to_le_bytes());
        // Two rows of sixteen colours: index 1 is pure red in row 0, blue in 1.
        tim.extend_from_slice(&(12u32 + 64).to_le_bytes());
        for word in [0u16, 0, 16, 2] {
            tim.extend_from_slice(&word.to_le_bytes());
        }
        for colour in [0x001fu16, 0x7c00] {
            for index in 0..16u16 {
                let word = if index == 1 { colour } else { 0 };
                tim.extend_from_slice(&word.to_le_bytes());
            }
        }
        tim.extend_from_slice(&(12u32 + 2).to_le_bytes());
        for word in [0u16, 0, 1, 1] {
            tim.extend_from_slice(&word.to_le_bytes());
        }
        tim.extend_from_slice(&[0x11, 0x11]);
        let mut record = vec![0u8; 8];
        record.extend_from_slice(&[0u8; 16]);
        let data = container(&[Part {
            id: 7,
            record: &record,
            tim: &tim,
        }]);
        let bank = Bank::load(None, Some(&data));
        let colour = |row| {
            let page = Page {
                sheet: Sheet::Room(0),
                row,
            };
            bank.page(page).unwrap().rgba[..3].to_vec()
        };
        assert_ne!(colour(0)[0], 0);
        assert_eq!(colour(0)[2], 0);
        assert_eq!(colour(1)[0], 0);
        assert_ne!(colour(1)[2], 0);
    }

    #[test]
    fn a_shared_tile_finds_its_tim_in_espcore() {
        let tile = |page, clut: u16, v| {
            let mut sheet = Sheet::Core(page);
            let (row, v) = core_tile(&mut sheet, clut << 6, v);
            (sheet, row, v)
        };
        // Page 0x1e: its first TIM, and past that its second.
        assert_eq!(tile(0, 0x1e1, 0xa7), (Sheet::Core(0), 1, 0xa7));
        assert_eq!(tile(0, 0x1e4, 0xa8), (Sheet::Core(1), 0, 0));
        assert_eq!(tile(0, 0x1ea, 0), (Sheet::Core(0), 0, 0));
        // Page 0x1f: the rest.
        assert_eq!(tile(1, 0x1e9, 0x57), (Sheet::Core(2), 1, 0x57));
        assert_eq!(tile(1, 0x1f1, 0x58), (Sheet::Core(3), 4, 0));
        assert_eq!(tile(1, 0x1ec, 0xa8), (Sheet::Core(4), 0, 0));
        assert_eq!(tile(1, 0x1ec, 0xd0), (Sheet::Core(5), 0, 0));
    }

    #[test]
    fn a_rooms_tile_keeps_its_row_and_position() {
        let mut sheet = Sheet::Room(2);
        assert_eq!(core_tile(&mut sheet, 3 << 6, 0xb0), (3, 0xb0));
        assert_eq!(sheet, Sheet::Room(2));
    }

    #[test]
    fn a_type_this_room_lacks_spawns_nothing() {
        let mut effects = Effects::default();
        effects.enter_room(bank(&[fire_block()]));
        assert_eq!(effects.call(9, 0, 0x1000, 0, [0, 0, 0], false), None);
    }

    #[test]
    fn a_container_survives_the_trip_through_the_rooms_tables() {
        // Ids, then the sprite record, then its offset table.
        let mut data = vec![7, 0xff, 0, 0, 0, 0, 0, 0];
        let record_at = data.len();
        data.extend_from_slice(&[0u8; 8]);
        let table = data.len();
        data.extend_from_slice(&(record_at as u32).to_le_bytes());
        let out = room_container(&data, [0, table as u32, 0, 0]);
        // Offset 0 means the room has no id table.
        assert!(out.unwrap().is_none());
        let mut data = vec![0u8; 4];
        data.extend_from_slice(&[7, 0xff, 0, 0, 0, 0, 0, 0]);
        let record_at = data.len();
        data.extend_from_slice(&[0u8; 8]);
        let table = data.len();
        data.extend_from_slice(&((record_at - 4) as u32).to_le_bytes());
        let out = room_container(&data, [4, table as u32, 0, 0])
            .unwrap()
            .unwrap();
        assert_eq!(out[0], 1);
        assert_eq!(out[1], 7);
    }
}
