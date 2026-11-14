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
/// `Esp_pos` copies the matrix `Esp_call` was given from where it points
/// (`+0x6c` to `+0x4c`), so the slot follows it.
const FLAG_FOLLOW: u16 = 0x0800;
/// The kinds `Esp_move` still moves under `fg_stop`'s `0x10000000`.
const MOVE_WHEN_STOPPED: [u8; 5] = [0x15, 0x16, 0x1A, 0x1C, 0x28];
const FLAG_LIVE: u16 = 0x8000;
/// What `Esp_call` leaves in a new slot.
const FLAGS_NEW: u16 = 0xa003;

/// `Esp_tbl`'s entries with a port here.
pub fn ported(id: u8) -> bool {
    matches!(
        id,
        0..=4
            | 6
            | 0x0b..=0x0e
            | 0x19
            | 0x20
            | 0x2B
            | 0x34
            | 0x36..=0x39
            | 0x3E
            | 0x3F
            | 0x44
            | 0x45
            | 0x46
            | 0x47
            | 0x4B
            | 0x4E
            | 0x52
            | 0x55
            | 0x57
    )
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
                crate::warning!("warning: invalid effect data: {error:#}");
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
            Err(error) => crate::warning!("warning: invalid effect texture: {error:#}"),
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

/// A `Sce_espr_on`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EsprCall {
    pub kind: u8,
    pub row: u8,
    pub size: u16,
    pub dir: i16,
    pub at: [i16; 3],
    pub rotation: [i16; 3],
    pub group: u8,
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
    /// `+0x1d`: the id `Esp_call2` gave it, for `Esp_kill2`.
    group: u8,
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
    /// `+0x3c`: where it was drawn last.
    last_pos: [i16; 3],
    /// `+0x42`: which way `Esp_22` drifts next. `Esp_call` leaves it at 1.
    sway: i8,
    /// `+0x44`: how `Esp_call3D` turned it.
    rotation: [i16; 3],
    /// `+0x4a`: how fast `Esp_22` falls before it swings back.
    sway_limit: i16,
    /// `+0x78`: this slot's group of blocks, in the motion table.
    blocks: usize,
    /// `+0x14` while it falls: `Sca_ck_hit`'s height, under it at the last look.
    ground: i32,
    /// `+0x4c`, the matrix `Esp_call` was given, kept for a slot whose
    /// `0x400` has `Esp_pos` turn the offset by it; and the point on it.
    hand: Option<([[i16; 3]; 3], [i32; 3])>,
    local: [i16; 3],
    /// `+0x42` as a word, as `Esp_1E` sets it: a count for the blocks after.
    countdown: u16,
    /// `+0x6c` when it points at a room object's matrix (`Sce_kirakira_set`'s
    /// twinkles): the object's slot, which `Esp_kill` matches and
    /// [`FLAG_FOLLOW`] follows.
    object: Option<u8>,
    /// `+0x6c` when it points at a body's part's `Workm`: whose, which
    /// [`FLAG_FOLLOW`] copies into `hand` on each tick from [`Effects::anchors`].
    anchor: Option<Anchor>,
}

/// A body's part whose `Workm` an effect was started on (`Esp_call`'s matrix).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Anchor {
    pub body: Body,
    pub part: u8,
}

/// Whose part an [`Anchor`] is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Body {
    Player,
    /// An enemy slot of `Em_work`.
    Enemy(u8),
}

/// What `FUN_004d6150` writes over a grenade's first block, and the hand's
/// matrix and point it was called with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Launch {
    pub accel: i8,
    pub velocity: [i16; 3],
    pub matrix: [[i16; 3]; 3],
    pub t: [i32; 3],
    pub point: [i16; 3],
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
    /// The player, for what `Esp_20` hits: the caller sets it before a tick
    /// and takes back what the hits left.
    pub target: Option<Target>,
    /// The room's water areas, for `Esp_05`'s `Water_ck`: the caller sets them
    /// before a tick.
    pub water: Vec<crate::zombie2::WaterArea>,
    /// The enemies, for what a grenade hits (`FUN_004d7e30`): the caller sets
    /// them before a tick, and gives [`Self::blasts`] to the works after.
    pub bodies: Vec<crate::weapon2::Target>,
    pub blasts: Vec<crate::weapon2::BlastEffect>,
    /// The player's `Type & 0xFFF`, which `Esp_11` reads.
    pub weapon: u8,
    /// The body the spark shot's last shot found (the player's `Sce_free2`
    /// and `Sce_free3`), by slot, which `Esp_0E` strikes: the caller sets it
    /// before a tick.
    pub spark_target: Option<usize>,
    /// What `Esp_0E` left fade 0 at this tick (`Fade_set(0x100, 0)` and
    /// `Fade_adjust`): its level, or `-1` for `Fade_off`. The caller takes it.
    pub fade: Option<i16>,
    /// Where each room object's twinkle sits on its matrix now, by slot: the
    /// caller sets them before a tick, for the slots that follow an object.
    pub objects: Vec<Option<[i16; 3]>>,
    /// The parts' `Workm` as this tick's `Joint_trans2` left them, for the
    /// slots on a part ([`Self::anchors_in_use`]): the caller sets them before
    /// a tick. `Game_loop` draws every body before `Esp_move`.
    pub(crate) anchors: Vec<(Anchor, crate::player2::Matrix)>,
    /// The bodies' boxes in `player_work`'s order, the player's first, for
    /// `Oba_ck_esp`: the caller sets them before a tick.
    pub oba: Vec<crate::zombie2::Oba>,
    /// `fg_common`'s 0x76, which turns room 407's steam off (`Esp_4B`): the
    /// caller sets it before a tick.
    pub steam_off: bool,
}

/// The player as `FUN_004d8260` tests them against an effect, and what the
/// hits of a tick left them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Target {
    pub pos: [i32; 3],
    /// `Atd[0]`'s `Ofs_y` and `At_h`: the height range is from `Ofs_y`
    /// above the feet, `At_h` either way.
    pub ofs_y: i16,
    pub at_h: u16,
    /// `+0x156`, the life, and `+0x154`'s `0x1000`.
    pub life: i16,
    pub near_death: bool,
    /// The routine the last hit set: `[2, 0, 0, 0]`, or `[3, 0, 0, 0]` when
    /// `Pl_life_down` said it killed.
    pub routine: Option<u8>,
    /// `Pl_life_down` said it killed: `fg_system`'s `0x4000000`.
    pub died: bool,
    /// `+0x1d3`, `Damage_cnt`: a hit waits until it's 0 again. A killing
    /// hit, and a poisoning one, set its `0x80`.
    pub damage_cnt: u8,
    /// A hit by an effect whose row has bit 8: `poison_status` 0x1518,
    /// `poison_timer` 0x1E.
    pub poisoned: bool,
    /// The new game's choice, which `Pl_life_down` reads.
    pub difficulty: crate::zombie2::Difficulty,
}

impl Target {
    /// `FUN_004d8260(pos, reach, down)` for the player alone: the player's
    /// in reach of `pos` across, and inside their height range, and still
    /// alive, and no hit reaction is running (`+0x1d3`).
    ///
    /// `+0x10f`'s `0xC0`, which also stops the hit, isn't modelled.
    fn hit(&mut self, pos: [i16; 3], reach: u32, down: i32) -> bool {
        let [x, y, z] = pos.map(i32::from);
        let h = u32::from(self.at_h);
        let above = (i32::from(self.ofs_y) - y + self.pos[1]) as u32;
        if self.damage_cnt != 0 || self.life < 0 || above.wrapping_add(h) >= h * 2 {
            return false;
        }
        let (dz, dx) = (z.wrapping_sub(self.pos[2]), x.wrapping_sub(self.pos[0]));
        let across = dz.wrapping_mul(dz).wrapping_add(dx.wrapping_mul(dx));
        if crate::sca::square_root0(across) as u32 >= reach {
            return false;
        }
        self.routine = Some(2);
        let (left, near_death, result) =
            crate::zombie2::life_down(self.life, down, false, self.near_death, self.difficulty);
        self.life = left;
        self.near_death = near_death;
        if result == 2 {
            self.damage_cnt |= 0x80;
            self.routine = Some(3);
            self.died = true;
        }
        true
    }
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
            sway: 1,
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

    /// `Esp_call2`, `Esp_call3D` and `Esp_call3D2`: `Esp_call` with an id for
    /// [`Self::kill_group`] and a turn for what's drawn in 3D.
    pub fn call_placed(&mut self, call: EsprCall) -> Option<usize> {
        let EsprCall {
            rotation, group, ..
        } = call;
        let first = self.call(call.kind, call.row, call.size, call.dir, call.at, false)?;
        for (index, slot) in self.slots.iter_mut().enumerate() {
            if index == first || slot.flags != 0 && first != 0 && usize::from(slot.parent) == first
            {
                slot.rotation = rotation;
                slot.group = group;
            }
        }
        Some(first)
    }

    /// `Esp_call3D`'s turn (`+0x44`), on a slot `Esp_call` just made and the
    /// slots that die with it.
    pub fn turn(&mut self, first: usize, rotation: [i16; 3]) {
        for (index, slot) in self.slots.iter_mut().enumerate() {
            if index == first || slot.flags != 0 && first != 0 && usize::from(slot.parent) == first
            {
                slot.rotation = rotation;
            }
        }
    }

    /// What `FUN_004d6150` writes over a grenade's first block once
    /// `Esp_call` made it: its `+8` and `+0xc` to `+0x10`.
    pub fn launch(&mut self, index: usize, launch: Launch) {
        if let Some(slot) = self.slots.get_mut(index) {
            slot.accel[0] = launch.accel;
            slot.velocity = launch.velocity;
            slot.hand = Some((launch.matrix, launch.t));
            slot.local = launch.point;
        }
    }

    /// `Esp_call` on a room object's matrix, with the point on it worked out
    /// already (`at`): the slot and those that die with it keep the object.
    pub fn call_on_object(
        &mut self,
        kind: u8,
        row: u8,
        size: u16,
        object: u8,
        at: [i16; 3],
    ) -> Option<usize> {
        let first = self.call(kind, row, size, 0, at, false)?;
        for (index, slot) in self.slots.iter_mut().enumerate() {
            if index == first || slot.flags != 0 && first != 0 && usize::from(slot.parent) == first
            {
                slot.object = Some(object);
            }
        }
        Some(first)
    }

    /// `Esp_call` on a body's part: `matrix` is the part's `Workm` as the
    /// call finds it and `point` the vector on it. The slot and those that die
    /// with it keep the part, and with [`FLAG_FOLLOW`] take its `Workm` again
    /// on every tick (`Esp_pos`).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn call_on_part(
        &mut self,
        kind: u8,
        row: u8,
        size: u16,
        dir: i16,
        anchor: Anchor,
        matrix: crate::player2::Matrix,
        point: [i16; 3],
    ) -> Option<usize> {
        self.call_anchored(kind, row, size, dir, anchor, matrix, point, false)
    }

    /// [`Self::call_on_part`], or `Esp_call3`'s with `deferred`.
    #[allow(clippy::too_many_arguments)]
    fn call_anchored(
        &mut self,
        kind: u8,
        row: u8,
        size: u16,
        dir: i16,
        anchor: Anchor,
        matrix: crate::player2::Matrix,
        point: [i16; 3],
        deferred: bool,
    ) -> Option<usize> {
        let at = apply_matrix(&matrix, point);
        let first = self.call(kind, row, size, dir, at, deferred)?;
        for (index, slot) in self.slots.iter_mut().enumerate() {
            if index == first || slot.flags != 0 && first != 0 && usize::from(slot.parent) == first
            {
                slot.anchor = Some(anchor);
                slot.hand = Some((matrix.m, matrix.t));
                slot.local = point;
            }
        }
        Some(first)
    }

    /// `Esp_call3` with a slot's own matrix (`+0x6c`) and point (`+0x2c`):
    /// the new slot is on whatever `parent` was started on, a part's `Workm`
    /// as it is now included.
    fn call_from(
        &mut self,
        parent: &Slot,
        kind: u8,
        row: u8,
        size: u16,
        dir: i16,
    ) -> Option<usize> {
        let Some(anchor) = parent.anchor else {
            return self.call(kind, row, size, dir, parent.base, true);
        };
        let matrix = self
            .anchors
            .iter()
            .find(|(other, _)| *other == anchor)
            .map(|(_, matrix)| *matrix)
            .or_else(|| parent.hand.map(|(m, t)| crate::player2::Matrix { m, t }))?;
        self.call_anchored(kind, row, size, dir, anchor, matrix, parent.local, true)
    }

    /// `Esp_call3` on the copy of the matrix the slot was started on
    /// (`+0x4c`), at `point` on it.
    fn call_on_slot_matrix(
        &mut self,
        parent: &Slot,
        kind: u8,
        row: u8,
        size: u16,
        dir: i16,
        point: [i16; 3],
    ) -> Option<usize> {
        match (parent.anchor, parent.hand) {
            (Some(anchor), Some((m, t))) => {
                let matrix = crate::player2::Matrix { m, t };
                self.call_anchored(kind, row, size, dir, anchor, matrix, point, true)
            }
            _ => self.call(kind, row, size, dir, parent.base, true),
        }
    }

    /// The parts the live slots were started on, each once: the `anchors` a
    /// tick needs.
    pub fn anchors_in_use(&self) -> Vec<Anchor> {
        let mut anchors = Vec::new();
        for slot in &self.slots {
            if let (true, Some(anchor)) = (slot.flags != 0, slot.anchor)
                && !anchors.contains(&anchor)
            {
                anchors.push(anchor);
            }
        }
        anchors
    }

    /// `Esp_kill` with an object's matrix: only the slots on that object.
    pub fn kill_on_object(&mut self, kind: u8, row: u8, object: u8) {
        for slot in &mut self.slots {
            if slot.kind == kind && slot.row == row && slot.object == Some(object) {
                slot.kill();
            }
        }
    }

    /// A room's bank holding one empty effect of `kind`, row 0, for other
    /// modules' tests of what calls and kills an effect.
    #[cfg(test)]
    pub(crate) fn test_enter_room_with(&mut self, kind: u8, row: u8) {
        let mut block = [0u8; BLOCK_LEN];
        block[18..20].copy_from_slice(&0x8001u16.to_le_bytes());
        self.enter_room(tests::bank_rows(kind, &[(row, &[block])]));
    }

    /// Whether a slot is live, for other modules' tests.
    #[cfg(test)]
    pub(crate) fn test_live(&self, slot: usize) -> bool {
        self.slots[slot].live()
    }

    /// A slot's place, for other modules' tests.
    #[cfg(test)]
    pub(crate) fn test_pos(&self, slot: usize) -> [i16; 3] {
        self.slots[slot].pos
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

    /// `Esp_die`: all go but kind 5's rows 2 and 3.
    pub fn die(&mut self) {
        for slot in &mut self.slots {
            if slot.kind != 0 && (slot.kind != 5 || (slot.row != 3 && slot.row != 2)) {
                slot.kill();
            }
        }
    }

    /// `Esp_kill2`.
    pub fn kill_group(&mut self, group: u8) {
        for slot in &mut self.slots {
            if slot.group == group {
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
            // `Esp_0B` scales the block's motion by the slot's size; its
            // `0x4E` does the same from a random first frame (`Esp_02`).
            0x0b | 0x4e => {
                self.next_block(index, id == 0x4e);
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
            // `Esp_05`: what's falling reaches the water, or the ground, or
            // something it was above.
            5 => {
                let level = crate::zombie2::water_ck(
                    &self.water,
                    i32::from(slot.pos[0]),
                    i32::from(slot.pos[2]),
                );
                if level != 0 && level < slot.pos[1] {
                    // Under the water's level it ends in a splash, `Esp_call3`'s.
                    self.call(0x1A, 1, slot.size, 0, slot.pos, true);
                    self.slots[index].kill();
                    if slot.kind == 0x1D {
                        self.behave(index, slot.start, sca);
                    }
                    return;
                }
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
            0x11 => {
                let bank = Arc::clone(&self.bank);
                let delay = bank.sprites.get(&slot.kind).map_or(0, |s| s.frame(0x12)[2]);
                let weapon = self.weapon;
                let timer = if weapon & 0x1F == 9 {
                    self.rnd() % 3 + 10
                } else {
                    0xF
                };
                let this = &mut self.slots[index];
                this.generation = weapon.wrapping_sub(9);
                this.anim = 0x16;
                this.update = 0x0F;
                this.flags = 0xB403;
                this.tpage |= 0x20;
                this.frame = 0x12;
                this.delay = delay;
                this.step = timer;
                self.grenade_flight(index, sca);
            }
            0x0f => self.grenade_flight(index, sca),
            // `Esp_17`: a rocket leaves smoke behind it.
            0x17 => {
                self.call(3, 0x0A, 0x1800, 0, slot.pos, true);
            }
            0x18 => self.rocket_flight(index, sca),
            0x46 => self.flame(index, sca),
            0x0e => self.spark_strike(index),
            0x3e => self.spark_miss(index),
            // `Esp_3F`: a spark that meets a wall is hidden.
            0x3f => {
                let ground = Self::ground_masked(sca, slot.pos, 0x80);
                if ground.blocked || ground.under_record {
                    self.slots[index].flags &= !FLAG_VISIBLE;
                }
            }
            // `Esp_16`: a puff of smoke left behind each tick, half again its size.
            0x16 => {
                let size = (u32::from(slot.size) * 0x96 / 100) as u16;
                self.call(3, 0x0B, size, 0, slot.pos, true);
            }
            0x2f => self.grenade_burst(index),
            0x13 => self.smoke_fade(index),
            // `Esp_1E`: on from a random first frame, with `+0x1b` and a count
            // by the row: 2 and 2 to 9 for row 4, 3 and 700 to 950 otherwise.
            0x1e => {
                self.next_block(index, true);
                let row4 = slot.row == 4;
                let draw = u16::from(self.rnd());
                let this = &mut self.slots[index];
                if row4 {
                    this.generation = 2;
                    this.countdown = (draw & 7) + 2;
                } else {
                    this.generation = 3;
                    this.countdown = (draw % 6 + 0xE) * 0x32;
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
            // `Esp_21`: shown for a random while, then back to a frame that
            // holds until frame 0x12 comes round again.
            0x21 => {
                if slot.count == 0 {
                    self.slots[index].flags = 0xB803;
                    self.slots[index].count = self.rnd() % 0x1E + 0x3C;
                } else {
                    self.slots[index].count -= 1;
                    if slot.frame == 0x12 {
                        let delay = self
                            .bank
                            .sprites
                            .get(&slot.kind)
                            .map_or(0, |s| s.frame(0x0B)[2]);
                        let this = &mut self.slots[index];
                        this.flags = 0x8000;
                        this.frame = 0x0B;
                        this.delay = delay;
                    }
                }
            }
            // `Esp_20`: what's thrown hits the player in reach, and takes the
            // behaviour `+2` names; or swells as it goes.
            0x20 => {
                if self.thrown_hit(&slot, 800) {
                    self.behave(index, slot.start, sca);
                    return;
                }
                let this = &mut self.slots[index];
                if (this.generation as i8) < 0x14 {
                    let grow = (u32::from(this.scale[0]) * 0x26 / 1000) as u16;
                    this.scale[0] = this.scale[0].wrapping_add(grow);
                    this.generation += 1;
                }
                let grow = (u32::from(this.scale[1]) * 5 / 1000) as u16;
                this.scale[1] = this.scale[1].wrapping_add(grow);
            }
            // `Esp_36`: `Esp_10`'s start of a fall, then `Esp_38` falls it.
            0x36 => {
                self.behave(index, 0x10, sca);
                let this = &mut self.slots[index];
                this.update = 0x38;
                this.rotation[1] = this.next_flags as i16;
            }
            // `Esp_38`: what's spat down hits the player in reach and takes
            // the behaviour `+2` names; in water, or near the ground 700
            // below it, it splashes (kind 3 row 0xF, `Esp_call3`) and does the
            // same; against a record `Esp_05`'s way picks `+2` or `+3`.
            0x38 => {
                if self.thrown_hit(&slot, 600) {
                    self.behave(index, slot.start, sca);
                    return;
                }
                let [x, y, z] = slot.pos.map(i32::from);
                let level = crate::zombie2::water_ck(&self.water, x, z);
                let splash = if level != 0 && i32::from(level) < y {
                    true
                } else {
                    let probe = [x, y + 700, z];
                    let ground = sca.map_or_else(
                        || crate::sca::Ground {
                            blocked: probe[1] > 0,
                            ..Default::default()
                        },
                        |sca| crate::sca::ground_at(sca, probe, 2, 0x2000),
                    );
                    if ground.height < probe[1] {
                        true
                    } else {
                        if ground.blocked || ground.under_record {
                            let id = if ground.height == slot.ground {
                                slot.count
                            } else {
                                slot.start
                            };
                            self.behave(index, id, sca);
                        }
                        self.slots[index].ground = ground.height;
                        false
                    }
                };
                if splash {
                    self.call(3, 0x0F, 0x2800, 0, slot.pos, true);
                    self.behave(index, slot.start, sca);
                }
            }
            // `Esp_37`: a spat blob narrows and lengthens as it falls, fast
            // for its first 20 ticks (counted in `+0x1b`) and slowly after.
            0x37 => {
                let this = &mut self.slots[index];
                let [wide, tall] = this.scale.map(u32::from);
                let (narrow, lengthen) = if this.generation < 0x14 {
                    this.generation += 1;
                    (50, 90)
                } else {
                    (5, 10)
                };
                this.scale[0] = (wide - wide * narrow / 1000) as u16;
                this.scale[1] = (tall + tall * lengthen / 1000) as u16;
            }
            // `Esp_39`: a blob that came down rests on the water or the
            // ground and squashes, then splashes as `Esp_34`, 30% bigger.
            0x39 => {
                let [x, _, z] = slot.pos.map(i32::from);
                let level = crate::zombie2::water_ck(&self.water, x, z);
                if level != 0 && i32::from(level) < i32::from(slot.pos[1]) {
                    let this = &mut self.slots[index];
                    this.accel[1] = 0;
                    this.velocity[1] = 0;
                    this.pos[1] = level;
                }
                let this = self.slots[index];
                let y = i32::from(this.pos[1]);
                let ground = Self::ground(sca, this.pos).height;
                let this = &mut self.slots[index];
                if ground < y {
                    this.accel[1] = 0;
                    this.velocity[1] = 0;
                }
                let [wide, tall] = this.scale.map(u32::from);
                if this.scale[1] > 0x1000 {
                    this.anim = 0;
                    this.update = 0x39;
                    this.scale[0] = (wide + wide * 100 / 1000) as u16;
                    this.scale[1] = (tall - tall * 300 / 1000) as u16;
                    return;
                }
                this.scale = [0x1100, 0xA00];
                let size = u32::from(this.size);
                this.size = (size + size * 30 / 100) as u16;
                self.behave(index, 0x34, sca);
            }
            0x4b => self.steam(index),
            // `Esp_57`: a puff of steam settles on the water or the ground.
            0x57 => {
                let [x, y, z] = slot.pos.map(i32::from);
                let level = i32::from(crate::zombie2::water_ck(&self.water, x, z));
                let rest = if level != 0 && level < y {
                    Some(level)
                } else {
                    let ground = Self::ground(sca, slot.pos).height;
                    (ground < y).then_some(ground)
                };
                if let Some(at) = rest {
                    self.land_drop(index, 8);
                    self.slots[index].pos[1] = at as i16;
                }
            }
            0x47 => self.stream_fill(index, sca),
            // `Esp_55`: the stream makes a drop (kind 0x1C row 2,
            // `Esp_call3`) every fourth tick, counting in `+0x12`.
            0x55 => {
                if slot.next_flags.is_multiple_of(4) {
                    self.call(0x1C, 2, slot.size, slot.dir, slot.base, true);
                }
                let this = &mut self.slots[index];
                this.next_flags = this.next_flags.wrapping_add(1);
            }
            0x44 => self.stream_drop(index, sca),
            // `Esp_34`: a splash where it hit, with its sound, and it stops
            // to play out from frame 0xD.
            0x34 => {
                self.sounds.push((0x304_0001, slot.pos));
                let this = &mut self.slots[index];
                this.frame = 0x0D;
                this.delay = 2;
                self.call(3, 0x0F, 0x2800, 0, slot.pos, true);
                let this = &mut self.slots[index];
                this.accel[0] = 0;
                this.accel[1] = 0;
                this.velocity[0] = 0;
                this.velocity[1] = 0;
                this.flags = 0xB001;
                this.anim = 0;
                this.update = 0;
                this.start = 0;
                this.count = 0;
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
            // `Esp_27`: what fell hits the ground, and breaks there.
            0x27 => {
                self.slots[index].anim = 0x54;
                self.slots[index].update = 0;
                self.call(0x14, 0, slot.size, 0, slot.pos, true);
            }
            // `Esp_56`: the Colt's reload, six spent cases (kind 2, row 6 at
            // its size) from where it was started, each turned a random step
            // of 400: one on its count's first tick, two on the second and the
            // fourth, and one on the seventh, when it goes.
            0x56 => {
                let calls = match slot.start {
                    0 | 6 => 1,
                    1 | 3 => 2,
                    _ => 0,
                };
                for _ in 0..calls {
                    let turn = (u32::from(self.rnd()) % 0x1E * 400) as i16;
                    self.call_from(&slot, 2, 6, slot.size, turn.wrapping_add(slot.dir));
                }
                if slot.start == 6 {
                    self.slots[index].kill();
                }
                let this = &mut self.slots[index];
                this.start = this.start.wrapping_add(1);
            }
            // `Esp_54`: and is gone, leaving a second burst.
            0x54 => {
                self.slots[index].kill();
                self.call(0x14, 0, slot.size, 0, slot.pos, true);
            }
            // `Esp_2A`: then two more.
            0x2A => {
                for (step, size) in [(3, 0xC00u16), (4, 0xA00)] {
                    let dir = u16::from(self.rnd()) << 4;
                    let row = slot.start.wrapping_add(step);
                    self.call_from(&slot, 9, row, size, dir as i16);
                }
                self.slots[index].anim = 0;
            }
            // `Esp_29`: bursts into three puffs of the next rows.
            0x29 => {
                for (step, size) in [0x800u16, 0x1000, 0xC00].into_iter().enumerate() {
                    let dir = u16::from(self.rnd()) << 4;
                    let row = slot.start.wrapping_add(step as u8);
                    self.call_from(&slot, 9, row, size, dir as i16);
                }
                self.slots[index].anim = 0x2A;
            }
            // `Esp_2B`: a drip's splash, flat on the ground, to the end of its frames.
            0x2B => {
                let delay = self
                    .bank
                    .sprites
                    .get(&slot.kind)
                    .map_or(0, |s| s.frame(0x1E)[2]);
                let this = &mut self.slots[index];
                this.anim = 0;
                this.update = 0;
                this.pos[1] = this.ground as i16;
                this.frame = 0x1E;
                this.delay = delay;
                this.flags &= !FLAG_INTEGRATE;
            }
            // `Esp_14`: gone on touching the ground.
            0x14 => {
                if Self::ground(sca, slot.pos).blocked {
                    self.slots[index].kill();
                }
            }
            // `Esp_19` puts out a fire in water, which isn't ported.
            0x19 => {}
            0x52 => self.ripple_ring(index),
            // `Esp_45`: a fire burning for 90 ticks, three flames (kind 0x26,
            // a random one of the three rows from `row & 0xF8`) a tick, each
            // turned a random step of 400; it dies down over its last 65.
            0x45 => {
                if slot.start == 0 {
                    let this = &mut self.slots[index];
                    this.flags = 0x8002;
                    this.start = 1;
                    this.count = 0x5A;
                } else if slot.start != 1 {
                    return;
                }
                if self.slots[index].count == 0 {
                    self.slots[index].kill();
                    return;
                }
                for _ in 0..3 {
                    let turn = (u32::from(self.rnd()) % 0x1E * 400) as i16;
                    let row = (self.rnd() % 3).wrapping_add(slot.row & 0xF8);
                    let this = self.slots[index];
                    self.call(
                        0x26,
                        row,
                        this.size,
                        turn.wrapping_add(this.dir),
                        this.base,
                        true,
                    );
                }
                let this = &mut self.slots[index];
                if this.count < 0x41 {
                    this.size = (u32::from(this.size) * 0x3BB / 1000) as u16;
                }
                this.count -= 1;
            }
            // `Esp_4A`: ten ripples of row `row & 0xF8` at its size, each
            // turned a random step of 400, and it goes.
            0x4A => {
                for _ in 0..10 {
                    let turn = (u32::from(self.rnd()) % 0x1E * 400) as i16;
                    let call = u32::from(slot.size) | ((u32::from(slot.row & 0xF8) | 0x1A00) << 16);
                    let at = self.slots[index].base;
                    self.call(
                        (call >> 24) as u8,
                        (call >> 16) as u8,
                        call as u16,
                        turn.wrapping_add(slot.dir),
                        at,
                        true,
                    );
                }
                self.slots[index].kill();
            }
            // `Esp_15`: on every other of its `+2` ticks, a drop (kind 3,
            // row 9, `0xB00` larger) where it's drawn, unless that's the origin.
            0x15 => {
                if slot.start != 0 {
                    let [x, y, z] = slot.pos.map(i32::from);
                    if slot.start & 1 == 0 && x + y + z != 0 {
                        let call = (u32::from(slot.size) + 0xB00) | 0x309_0000;
                        self.call(
                            (call >> 24) as u8,
                            (call >> 16) as u8,
                            call as u16,
                            0,
                            slot.pos,
                            true,
                        );
                    }
                    self.slots[index].start -= 1;
                }
            }
            // `Esp_41`: back at its first frame, it rests a while from the
            // frame `step` names, then comes back at a slightly smaller size.
            0x41 => {
                if slot.count == 0 {
                    if slot.frame == slot.start {
                        let delay = self
                            .bank
                            .sprites
                            .get(&slot.kind)
                            .map_or(0, |s| s.frame(slot.step)[2]);
                        let count = self.rnd() % 0x32 + 0x32;
                        let this = &mut self.slots[index];
                        this.frame = slot.step;
                        this.delay = delay;
                        this.flags &= 0xDFFC;
                        this.count = count;
                    }
                } else {
                    self.slots[index].count -= 1;
                    if self.slots[index].count == 0 {
                        let shrink = u16::from(self.rnd() % 10) * 10;
                        let this = &mut self.slots[index];
                        this.flags |= 0x2003;
                        this.scale = [slot.size.wrapping_sub(shrink); 2];
                    }
                }
            }
            // `Esp_22`: a feather rocking from side to side as it falls.
            0x22 => {
                if slot.next_flags == 0 {
                    if slot.velocity[1] < 1 {
                        self.slots[index].next_flags = 1;
                    }
                } else if slot.velocity[1] < slot.sway_limit {
                    self.feather_swing(index);
                    self.slots[index].next_flags = 0;
                }
            }
            // `Esp_24`: stopped by a wall, it swings back.
            0x24 => {
                self.feather_swing(index);
                let this = &mut self.slots[index];
                this.pos = this.last_pos;
            }
            _ => {
                if id < 128 && self.unported & 1 << id == 0 {
                    self.unported |= 1 << id;
                    eprintln!("note: effect behaviour 0x{id:02x} isn't ported");
                }
            }
        }
    }

    /// `Esp_tbl`'s `0x52`: a splash's rings, over five ticks. Each tick it
    /// moves its base up or down by a share of its size and starts ripples
    /// (`Esp_call3`, kind 0x1A, row `(row & 0xF8) + 1`) there, each at a
    /// share of its size and turned a random step of 400 from its own turn;
    /// on the fifth it starts one more of row `+ 4` at its size, and goes.
    fn ripple_ring(&mut self, index: usize) {
        /// The base's moves (in 1/4096ths of the size) and the ripples'
        /// sizes (in hundredths), in the order each tick makes them.
        enum Step {
            Move(i32),
            Ripple(u32),
        }
        use Step::{Move, Ripple};
        const TICKS: [&[Step]; 5] = [
            &[
                Ripple(180),
                Move(-300),
                Ripple(150),
                Move(-290),
                Ripple(120),
            ],
            &[Move(-260), Ripple(115), Move(-230), Ripple(100)],
            &[Move(-190), Ripple(90)],
            &[Move(190), Ripple(90), Move(230), Ripple(105)],
            &[
                Move(260),
                Ripple(110),
                Move(290),
                Ripple(140),
                Move(300),
                Ripple(170),
            ],
        ];
        let slot = self.slots[index];
        let Some(steps) = TICKS.get(usize::from(slot.start)) else {
            return;
        };
        if slot.start == 0 {
            let this = &mut self.slots[index];
            this.flags = 0x8002;
            this.next_flags = this.base[1] as u16;
        }
        let size = u32::from(slot.size);
        let row = u32::from((slot.row & 0xF8).wrapping_add(1)) << 16;
        for step in *steps {
            match *step {
                Move(by) => {
                    // The original divides toward zero.
                    let by = (size as i32 * by) / 4096;
                    let base = &mut self.slots[index].base[1];
                    *base = base.wrapping_add(by as i16);
                }
                Ripple(share) => {
                    let turn = (u32::from(self.rnd()) % 0x1E * 400) as i16;
                    let call = (size * share / 100) | 0x1A00_0000 | row;
                    let at = self.slots[index].base;
                    self.call(
                        (call >> 24) as u8,
                        (call >> 16) as u8,
                        call as u16,
                        turn.wrapping_add(slot.dir),
                        at,
                        true,
                    );
                }
            }
        }
        if slot.start < 4 {
            self.slots[index].start = slot.start + 1;
            return;
        }
        let call = (u32::from((slot.row & 0xF8).wrapping_add(4)) << 16) | size | 0x1A00_0000;
        let at = self.slots[index].base;
        self.call(
            (call >> 24) as u8,
            (call >> 16) as u8,
            call as u16,
            slot.dir,
            at,
            true,
        );
        self.slots[index].kill();
    }

    /// What `Esp_22` and `Esp_24` share: a new way to drift, the other way.
    fn feather_swing(&mut self, index: usize) {
        let across = i16::from(self.rnd()) % 0x46 + 0x32;
        let along = i16::from(self.rnd()) % 0x32;
        let down = i16::from(self.rnd()) % 0x14 + 0x28;
        let this = &mut self.slots[index];
        let sway = i16::from(this.sway);
        this.velocity = [across * sway, down, along * sway];
        this.sway_limit = down * -3 / 10;
        this.sway = -this.sway;
    }

    /// A slot's frame, with that frame's delay.
    fn show_frame(&mut self, index: usize, frame: u8) {
        let delay = self
            .bank
            .sprites
            .get(&self.slots[index].kind)
            .map_or(0, |s| s.frame(frame)[2]);
        let this = &mut self.slots[index];
        this.frame = frame;
        this.delay = delay;
    }

    /// A drop of the stream lands: it shows `frame`, stops falling and runs
    /// no more behaviours.
    fn land_drop(&mut self, index: usize, frame: u8) {
        self.show_frame(index, frame);
        let this = &mut self.slots[index];
        this.update = 0;
        this.anim = 0;
        this.accel[1] = 0;
        this.velocity[1] = 0;
    }

    /// `FUN_004b9590(1)`: one whole tick of one slot.
    fn step(&mut self, index: usize, sca: Option<&Sca>) {
        let anim = self.slots[index].anim;
        self.behave(index, anim, sca);
        self.advance(index, sca);
    }

    /// `Oba_ck_esp(pos, reach, height)`: a box of a body in play (`Be_flg`'s
    /// 1 without its 2) holds `pos`, `reach` wider and `height` taller.
    fn oba_ck_esp(&self, pos: [i16; 3], reach: i32, height: i32) -> bool {
        let [x, y, z] = pos.map(i32::from);
        self.oba
            .iter()
            .filter(|body| body.be_flg & 3 == 1)
            .any(|body| {
                let atd = &body.atd;
                let wide = i32::from(atd.at_w) + reach;
                let dx = atd.at[0].wrapping_sub(x);
                let dz = atd.at[2].wrapping_sub(z);
                if dx.wrapping_add(wide) as u32 > (wide * 2) as u32
                    || dz.wrapping_add(wide) as u32 > (wide * 2) as u32
                {
                    return false;
                }
                let distance =
                    crate::sca::square_root0(dz.wrapping_mul(dz).wrapping_add(dx.wrapping_mul(dx)));
                if wide.wrapping_sub(distance) <= 0 {
                    return false;
                }
                let tall = i32::from(atd.at_h) + height;
                let dy = atd.at[1].wrapping_sub(y);
                -tall < dy && dy < tall
            })
    }

    /// `Esp_4B`: a steam vent (room 407's) puffs a row 1 slot on its own
    /// matrix every fifth tick, counting in `+0x12`. Once `fg_common`'s 0x76
    /// is up it narrows each puff by a fifth and goes out after 0x5A ticks
    /// (counted in `+0x14`).
    fn steam(&mut self, index: usize) {
        if self.steam_off {
            let this = &mut self.slots[index];
            this.next_tpage = this.next_tpage.wrapping_add(1);
            if this.next_tpage > 0x5A {
                this.update = 0;
                this.anim = 0;
                this.flags = 0;
                this.next_flags = this.next_flags.wrapping_add(1);
                return;
            }
            if this.next_flags.is_multiple_of(5) {
                this.scale[0] = (u32::from(this.scale[0]) * 80 / 100) as u16;
                self.puff(index);
            }
        } else {
            self.slots[index].flags = 0x8000;
            if self.slots[index].next_flags.is_multiple_of(5) {
                self.puff(index);
            }
        }
        let this = &mut self.slots[index];
        this.next_flags = this.next_flags.wrapping_add(1);
    }

    /// A puff of [`Self::steam`], started at once (`0xB803`) at the vent's
    /// width.
    fn puff(&mut self, index: usize) {
        let vent = self.slots[index];
        if let Some(puff) = self.call_from(&vent, vent.kind, 1, vent.size, vent.dir) {
            let this = &mut self.slots[puff];
            this.flags = 0xB803;
            if self.steam_off {
                this.scale[0] = vent.scale[0];
            }
        }
    }

    /// `Esp_47`, once: a stream (room 405's) fills in at once. A drop is made
    /// every fourth count and run on for as many ticks as the count, until
    /// one has reached the water or the ground (the pipe's full length) or
    /// the pool is full; then the stream goes on to `Esp_55`.
    fn stream_fill(&mut self, index: usize, sca: Option<&Sca>) {
        self.next_block(index, false);
        let mut count: u32 = 1;
        loop {
            count = count.wrapping_add(1);
            if count == 0 {
                break;
            }
            if !count.is_multiple_of(4) {
                continue;
            }
            let stream = self.slots[index];
            let Some(drop) = self.call(0x1C, 2, stream.size, stream.dir, stream.base, true) else {
                break;
            };
            for _ in 0..count {
                self.step(drop, sca);
                self.slots[drop].next_flags = 0;
            }
            let this = &mut self.slots[drop];
            this.flags = 0xB003;
            this.next_flags = 6;
            this.anim = 0;
            this.update = 0x44;
            let [x, y, z] = this.pos.map(i32::from);
            let level = crate::zombie2::water_ck(&self.water, x, z);
            let ground = Self::ground(sca, this.pos).height;
            let landed = if level != 0 {
                i32::from(level) < y
            } else {
                ground < y
            };
            if landed {
                let this = &mut self.slots[drop];
                this.frame = 0x0A;
                this.scale[1] = 0x1000;
                this.update = 0;
                this.anim = 0;
                this.accel[1] = 0;
                this.velocity[1] = 0;
                break;
            }
        }
        let this = &mut self.slots[index];
        this.flags = 0x8000;
        this.anim = 0x55;
        this.update = 0;
    }

    /// `Esp_44`: a drop of the stream falls, lengthening, until it meets a
    /// body (two splashes, kind 0x1A, `Esp_call3`), the water or the ground.
    fn stream_drop(&mut self, index: usize, sca: Option<&Sca>) {
        if self.slots[index].frame == 0x0A {
            self.show_frame(index, 6);
        }
        let slot = self.slots[index];
        let y = i32::from(slot.pos[1]);
        let height = 1i32.wrapping_shl((y / -1800) as u32);
        if self.oba_ck_esp(slot.pos, 0x96, height) {
            self.land_drop(index, 0x0B);
            self.slots[index].scale[1] = 0x1000;
            for turn in [0, 0xD90] {
                let spread = i32::from(self.rnd() % 10);
                let spread = if turn == 0 { 0x34 - spread } else { spread };
                let dir = ((spread * 12) as i16)
                    .wrapping_add(slot.dir)
                    .wrapping_add(turn);
                let size = (u32::from(self.rnd() & 7) << 7) + u32::from(slot.size);
                let row = (size >> 16) as u8;
                self.call(0x1A, row, size as u16, dir, slot.pos, true);
            }
            return;
        }
        let this = &mut self.slots[index];
        this.scale[1] = this.scale[1].wrapping_add(0x78);
        let [x, _, z] = slot.pos.map(i32::from);
        let level = crate::zombie2::water_ck(&self.water, x, z);
        if level != 0 && i32::from(level) < y {
            self.land_drop(index, 0x0B);
            self.slots[index].scale[1] = 0x1000;
        }
        let ground = Self::ground(sca, slot.pos).height;
        if ground < y {
            self.land_drop(index, 8);
            self.slots[index].pos[1] = ground as i16;
        }
    }

    /// `FUN_004d8260(pos, reach, 20)` for what's thrown or spat (`Esp_20`,
    /// `Esp_38`): the hit on the player, and a row with bit 8 poisons
    /// (`poison_status` 0x1518, `poison_timer` 0x1E, `Damage_cnt`'s `0x80`).
    ///
    /// The search's enemies (its `0x98ee7b` case) aren't targets here.
    fn thrown_hit(&mut self, slot: &Slot, reach: u32) -> bool {
        let Some(target) = self.target.as_mut() else {
            return false;
        };
        if !target.hit(slot.pos, reach, 20) {
            return false;
        }
        if slot.row & 8 != 0 {
            target.poisoned = true;
            target.damage_cnt |= 0x80;
        }
        true
    }

    /// `Sca_ck_hit`, at what's being tested for a landing.
    fn ground(sca: Option<&Sca>, pos: [i16; 3]) -> crate::sca::Ground {
        Self::ground_masked(sca, pos, 0x2000)
    }

    /// [`Self::ground`] with `Sca_ck_hit`'s mask.
    fn ground_masked(sca: Option<&Sca>, pos: [i16; 3], mask: u16) -> crate::sca::Ground {
        let pos = pos.map(i32::from);
        sca.map_or_else(
            || crate::sca::Ground {
                blocked: pos[1] > 0,
                ..Default::default()
            },
            |sca| crate::sca::ground_at(sca, pos, 2, mask),
        )
    }

    /// `Esp_0F`: a grenade flies on until it reaches the water, hits a body,
    /// or its time runs out, when it takes the behaviour `+2` names from
    /// `+0x1b`'s row; against a wall it steps back and takes the one `+3`
    /// names.
    fn grenade_flight(&mut self, index: usize, sca: Option<&Sca>) {
        let slot = self.slots[index];
        let [x, y, z] = slot.pos.map(i32::from);
        let level = crate::zombie2::water_ck(&self.water, x, z);
        let then = |id: u8| (slot.generation as i8 as u8).wrapping_add(id);
        if level != 0 && level < slot.pos[1] {
            let pos = &mut self.slots[index].pos;
            pos[1] = pos[1].wrapping_add(500);
            let at = *pos;
            self.call(0x1A, 5, 0x1C00, slot.dir, at, true);
            self.behave(index, then(slot.start), sca);
            return;
        }
        let timer = slot.step.wrapping_sub(1);
        let ground = Self::ground(sca, slot.pos);
        let this = &mut self.slots[index];
        this.step = timer;
        let under = ground.height.wrapping_sub(10) as u32;
        this.ground = under as i32;
        this.next_tpage = under as u16;
        this.range = (under >> 16) as u16;
        let word = 0x3_0009u32.wrapping_add(slot.generation as i8 as u32);
        let mut rect = [-1400, 0, 350, 250];
        let mut hit = false;
        for y in [y + 1000, y - 1000] {
            let blast = crate::weapon2::Blast {
                at: [x, y, z],
                dir: slot.dir,
                word,
            };
            let effects = crate::weapon2::blast_ck(&blast, &mut rect, &mut self.bodies);
            hit |= effects
                .iter()
                .any(|effect| matches!(effect, crate::weapon2::BlastEffect::Hit(_)));
            self.blasts.extend(effects);
        }
        if hit || timer == 0 {
            self.slots[index].flags |= 0x80;
            self.behave(index, then(slot.start), sca);
            return;
        }
        if !ground.blocked && !ground.under_record {
            return;
        }
        let this = &mut self.slots[index];
        for axis in 0..3 {
            this.velocity[axis] = this.velocity[axis].wrapping_sub(i16::from(this.accel[axis]));
            this.offset[axis] = this.offset[axis].wrapping_sub(this.velocity[axis]);
            this.velocity[axis] = this.velocity[axis].wrapping_sub(i16::from(this.accel[axis]));
            this.offset[axis] = this.offset[axis].wrapping_add(this.velocity[axis] / -3);
        }
        Self::place(this);
        self.behave(index, then(slot.count), sca);
    }

    /// `Esp_18`: a rocket flies on until it hurts what it meets, at it or on
    /// the floor below it, when it takes the behaviour `+2` names (its
    /// burst); against a wall it steps back and takes the one `+3` names.
    /// Past 0x7D00 either way it goes, though it still looks this tick.
    /// Where it burst (`DWORD_00989e80`) isn't kept.
    fn rocket_flight(&mut self, index: usize, sca: Option<&Sca>) {
        const EDGE: i16 = 0x7D00;
        let slot = self.slots[index];
        let [x, y, z] = slot.pos.map(i32::from);
        if slot.pos[0] > EDGE || slot.pos[2] > EDGE || slot.pos[0] < -EDGE || slot.pos[2] < -EDGE {
            let this = &mut self.slots[index];
            this.anim = 0;
            this.flags = 0;
        }
        let mut rect = [-800, 0, 400, 200];
        let mut blast = |this: &mut Self, y: i32, word: u32| {
            let blast = crate::weapon2::Blast {
                at: [x, y, z],
                dir: slot.dir,
                word,
            };
            let effects = crate::weapon2::blast_ck(&blast, &mut rect, &mut this.bodies);
            let hit = effects
                .iter()
                .any(|effect| matches!(effect, crate::weapon2::BlastEffect::Hit(_)));
            this.blasts.extend(effects);
            hit
        };
        if blast(self, y, 0x3_0011) {
            self.behave(index, slot.start, sca);
            return;
        }
        let ground = Self::ground(sca, slot.pos);
        let under = ground.height.wrapping_sub(10) as u32;
        let this = &mut self.slots[index];
        this.ground = under as i32;
        this.next_tpage = under as u16;
        this.range = (under >> 16) as u16;
        if blast(self, ground.height, 0x2_0011) {
            self.behave(index, slot.start, sca);
            return;
        }
        if !ground.blocked && !ground.under_record {
            return;
        }
        let this = &mut self.slots[index];
        for axis in 0..3 {
            this.velocity[axis] = this.velocity[axis].wrapping_sub(i16::from(this.accel[axis]));
            this.offset[axis] = this.offset[axis].wrapping_sub(this.velocity[axis]);
            this.velocity[axis] = this.velocity[axis].wrapping_sub(i16::from(this.accel[axis]));
            this.offset[axis] = this.offset[axis].wrapping_add(this.velocity[axis] / -3);
        }
        Self::place(this);
        self.behave(index, slot.count, sca);
    }

    /// `Esp_13`: smoke that hangs (`+0x1b` 3) for its count, shrinks for 30
    /// to 37 ticks (2), then for 90 to 100 more a little faster (1), and goes
    /// (0). With `+0x4a` set it also burns a body in reach (`Esp_28`) once
    /// it's past 15 ticks and wider than the size.
    fn smoke_fade(&mut self, index: usize) {
        let slot = self.slots[index];
        if slot.sway_limit != 0 {
            if slot.range > 0xF && slot.scale[0] > 0x1000 {
                self.fire_reach(index);
            }
            let this = &mut self.slots[index];
            this.range = this.range.wrapping_add(1);
        }
        let shrink = |this: &mut Slot, across: u32, up: u32| {
            this.scale[0] = (u32::from(this.scale[0]) * across / 1000) as u16;
            this.scale[1] = (u32::from(this.scale[1]) * up / 1000) as u16;
        };
        match self.slots[index].generation {
            0 => self.slots[index].kill(),
            1 => {
                let this = &mut self.slots[index];
                if this.countdown == 0 {
                    this.generation = 0;
                } else {
                    this.countdown -= 1;
                    shrink(this, 0x3DE, 0x3D4);
                }
            }
            2 => {
                if self.slots[index].countdown == 0 {
                    let draw = u16::from(self.rnd());
                    let this = &mut self.slots[index];
                    this.generation = 1;
                    this.countdown = draw % 0xB + 0x5A;
                } else {
                    let this = &mut self.slots[index];
                    this.countdown -= 1;
                    shrink(this, 0x3F1, 0x3EA);
                }
            }
            3 => {
                if self.slots[index].countdown != 0 {
                    self.slots[index].countdown -= 1;
                } else {
                    let draw = u16::from(self.rnd());
                    let this = &mut self.slots[index];
                    this.generation = 2;
                    this.countdown = (draw & 7) + 0x1E;
                }
            }
            _ => {}
        }
    }

    /// `Esp_46`: a flamethrower's flame. Each tick until it stops it looks for
    /// the water (where it shrinks, with a puff of steam), a body, which it
    /// burns, and the floor; on any of them, or inside a wall, it stops and
    /// rises as it burns out, three ticks on leaving smoke. Where it last
    /// burnt a body (`DWORD_00989e80`) isn't kept, and `Water_ck` is asked
    /// for `x` twice, as the original does.
    fn flame(&mut self, index: usize, sca: Option<&Sca>) {
        let this = &mut self.slots[index];
        this.start = this.start.wrapping_sub(1);
        if this.start == 0 {
            this.accel[1] = -10;
        }
        let slot = *this;
        if slot.count != 0 {
            if slot.start == 3 {
                self.call(4, 0x0C, 0x2000, 0, slot.pos, true);
            }
            return;
        }
        let at = slot.pos.map(i32::from);
        let mut stopped = false;
        let level = crate::zombie2::water_ck(&self.water, at[0], at[0]);
        if level != 0 && level < slot.pos[1] {
            let size = &mut self.slots[index].size;
            *size = (u32::from(*size) * 0x41 / 100) as u16;
            self.call(3, 0x0F, 0x2000, 0, slot.pos, true);
            stopped = true;
        }
        let blast = crate::weapon2::Blast {
            at,
            dir: slot.dir,
            word: 0x2_0010,
        };
        let effects = crate::weapon2::blast_ck(&blast, &mut [-1400, 0, 400, 200], &mut self.bodies);
        stopped |= effects
            .iter()
            .any(|effect| matches!(effect, crate::weapon2::BlastEffect::Hit(_)));
        self.blasts.extend(effects);
        let ground = Self::ground(sca, slot.pos);
        stopped |= ground.height < at[1];
        if ground.blocked || ground.under_record {
            let size = &mut self.slots[index].size;
            *size = (u32::from(*size) * 0x46 / 100) as u16;
        } else if !stopped {
            return;
        }
        let delay = self
            .bank
            .sprites
            .get(&slot.kind)
            .map_or(0, |sprite| sprite.frame(0x0C)[2]);
        let this = &mut self.slots[index];
        this.accel[0] = 0;
        this.accel[1] = 0;
        this.velocity[0] = 0;
        this.velocity[1] = 0x14;
        this.frame = 0x0C;
        this.delay = delay;
        this.start = 6;
        this.count = 1;
    }

    /// `Esp_0E`: the spark shot's strike on the body its shot found. On its
    /// first tick it sounds, hurts every body in a little box at that body's
    /// `Atd[0]` (`FUN_004d7e30`, row 0xD, the band by the height), and
    /// strings sparks from the gun to it; then it flashes the screen twice,
    /// over six ticks.
    fn spark_strike(&mut self, index: usize) {
        /// `Fade_adjust`'s levels for fade 0, by tick, and `Fade_off`'s -1.
        const FLASH: [i16; 7] = [0x2000, 0x1800, 0x1600, -1, 0x1400, 0x1200, -1];
        let slot = self.slots[index];
        if slot.start == 0 {
            self.slots[index].flags = slot.next_flags;
            self.sounds.push((0x110_0001, slot.pos));
            // `0x98a0c8`, the player's `Sce_free2` and `Sce_free3`: the body,
            // at its `Atd[0]` (`+0x84`).
            let body = self
                .spark_target
                .and_then(|target| self.bodies.iter().find(|body| body.slot == target))
                .map(|body| [body.at[0], body.at_y, body.at[1]]);
            if let Some(to) = body {
                let blast = crate::weapon2::Blast {
                    at: to,
                    dir: slot.dir,
                    word: 0x3_000E,
                };
                let effects =
                    crate::weapon2::blast_ck(&blast, &mut [-200, 0, 100, 0x32], &mut self.bodies);
                self.blasts.extend(effects);
                self.spark_trail(&slot, to);
            }
        }
        if let Some(&level) = FLASH.get(usize::from(slot.start)) {
            self.fade = Some(level);
        }
        if slot.start == 6 {
            self.slots[index].kill();
        }
        let this = &mut self.slots[index];
        this.start = this.start.wrapping_add(1);
    }

    /// `Esp_0E`'s sparks: one each `0x1b0` of the way to the body, up the
    /// gun's matrix from the slot's point and leaning over by the height, each
    /// turned at random.
    fn spark_trail(&mut self, slot: &Slot, to: [i32; 3]) {
        let from = slot.pos.map(i32::from);
        let lift = (from[1].abs_diff(to[1]) * 0x14 / 100) as i16;
        let [dx, dy, dz] = [0, 1, 2].map(|axis| from[axis].abs_diff(to[axis]) as i32);
        let root = crate::sca::square_root0;
        let across = root(dz.wrapping_mul(dz).wrapping_add(dx.wrapping_mul(dx)));
        let distance = root(
            across
                .wrapping_mul(across)
                .wrapping_add(dy.wrapping_mul(dy)),
        );
        let count = ((distance as u32) / 0x1B0).max(1);
        let lean = (to[1].wrapping_sub(from[1]) as u32 / count) as i16;
        let dir =
            (crate::se2::se_dir_ck(from[0], from[2], to[0], to[2]) as i16).wrapping_sub(slot.dir);
        let mut point = slot.local;
        for _ in 0..count {
            let Some(spark) = self.call_on_slot_matrix(slot, 6, 1, 0x2600, dir, point) else {
                break;
            };
            let x = (i16::from(self.rnd()) % 0x1E - 0xF) * 10;
            let y = (i16::from(self.rnd()) % 0x32 - 0x19) * 0x28;
            let along = i16::from(self.rnd());
            let side = i16::from(self.rnd());
            let z = lift
                .wrapping_add((side & 2) * 0x400)
                .wrapping_add(0x338)
                .wrapping_add(along % 0x14 * 0x1E);
            self.slots[spark].rotation = [x, y, z];
            point[1] = point[1].wrapping_add(0x1B0);
            point[0] = point[0].wrapping_sub(lean);
        }
    }

    /// `Esp_3E`: the spark shot's miss, nineteen sparks up the gun's matrix
    /// `0x1b0` apart, each turned at random, and it's gone.
    fn spark_miss(&mut self, index: usize) {
        let slot = self.slots[index];
        let mut point = slot.local;
        for _ in 0..0x13 {
            let Some(spark) = self.call_on_slot_matrix(&slot, 6, 4, 0x2000, slot.dir, point) else {
                break;
            };
            let y = (i16::from(self.rnd()) % 0x32 - 0x19) * 0x28;
            let z = ((!self.rnd() & 2 | 1) as i16) << 10;
            let rotation = &mut self.slots[spark].rotation;
            rotation[1] = y;
            rotation[2] = z;
            point[1] = point[1].wrapping_add(0x1B0);
        }
        self.slots[index].kill();
    }

    /// `Esp_28`: a flame round's fire burns the first body in reach (the
    /// weapon 0xA's third field), and stops spreading.
    fn fire_reach(&mut self, index: usize) {
        let slot = self.slots[index];
        let at = slot.pos.map(i32::from);
        let blast = crate::weapon2::Blast {
            at: [at[0], at[1] - 100, at[2]],
            dir: slot.dir,
            word: 0x2002_000A,
        };
        let effects = crate::weapon2::blast_ck(&blast, &mut [-600, 0, 300, 150], &mut self.bodies);
        let hit = effects
            .iter()
            .any(|effect| matches!(effect, crate::weapon2::BlastEffect::Hit(_)));
        self.blasts.extend(effects);
        if hit {
            let this = &mut self.slots[index];
            this.start = 0x40;
            this.count = 0;
            this.velocity[0] = 0;
            this.accel[0] = 0;
            this.next_flags &= !2;
        }
    }

    /// `Esp_2F`: a grenade's burst. The first tick it hurts the nearest body
    /// below and above, and sets off the fire and smoke; four ticks on, the
    /// last cloud, and it's gone.
    fn grenade_burst(&mut self, index: usize) {
        let slot = self.slots[index];
        match slot.next_flags {
            0 => {
                let this = &mut self.slots[index];
                this.flags = 0x8400;
                this.anim = 0;
                this.update = 0x2F;
                let at = slot.pos.map(i32::from);
                if slot.row == 0x0C {
                    self.sounds.push((0x111_0001, slot.pos));
                    let mut rect = [-2000, 0, 1000, 500];
                    for y in [at[1], at[1] + 900] {
                        let blast = crate::weapon2::Blast {
                            at: [at[0], y, at[2]],
                            dir: slot.dir,
                            word: 0x1002_0009,
                        };
                        let effects = crate::weapon2::blast_ck(&blast, &mut rect, &mut self.bodies);
                        self.blasts.extend(effects);
                    }
                } else {
                    self.sounds.push((0x114_0001, slot.pos));
                }
                if slot.step == 0xFF {
                    let pos = &mut self.slots[index].pos;
                    pos[1] = pos[1].wrapping_sub(0x1D6);
                }
                let pos = self.slots[index].pos;
                let size = (u32::from(slot.row) * 0xA00).wrapping_sub(0x7800) & 0xFFFF;
                for word in [
                    (size + 0x1700) | 0x31F_0000,
                    (size + 0x1F00) | 0x40C_0000,
                    (size + 0x1700) | 0x41D_0000,
                ] {
                    self.call(
                        (word >> 24) as u8,
                        (word >> 16) as u8,
                        word as u16,
                        0,
                        pos,
                        true,
                    );
                }
                self.slots[index].next_flags = 1;
            }
            1..=3 => self.slots[index].next_flags += 1,
            4 => {
                if slot.step == 0xFF {
                    let pos = &mut self.slots[index].pos;
                    pos[1] = pos[1].wrapping_sub(600);
                }
                let pos = self.slots[index].pos;
                self.call(4, 0x15, 0x2700, 0, pos, true);
                self.slots[index].kill();
            }
            _ => {}
        }
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
            if let Some(child) = this.call_from(&slot, slot.kind, slot.row, size, slot.dir) {
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
        slot.last_pos = slot.pos;
        let [x, y, z] = slot.offset.map(i32::from);
        let [bx, by, bz] = slot.base;
        if let (true, Some((m, t))) = (slot.flags & FLAG_NO_TURN != 0, slot.hand) {
            let at: [i32; 3] = std::array::from_fn(|axis| {
                i32::from(slot.local[axis].wrapping_add(slot.offset[axis]))
            });
            slot.pos = std::array::from_fn(|axis| {
                let turned: i32 = (0..3).map(|i| i32::from(m[axis][i]) * at[i]).sum();
                t[axis].wrapping_add(turned >> 12) as i16
            });
            return;
        }
        if let (Some(_), Some((m, t))) = (slot.anchor, slot.hand)
            && slot.flags & FLAG_NO_TURN == 0
        {
            let at = apply_matrix(&crate::player2::Matrix { m, t }, slot.local);
            let (sin, cos) = (
                crate::player2::rsin(i32::from(slot.dir)),
                crate::player2::rcos(i32::from(slot.dir)),
            );
            let tx = (cos * x + sin * z) >> 12;
            let tz = (cos * z - sin * x) >> 12;
            slot.pos = [
                at[0].wrapping_add(tx as i16),
                at[1].wrapping_add(y as i16),
                at[2].wrapping_add(tz as i16),
            ];
            return;
        }
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
        let slot = &mut self.slots[index];
        if slot.flags & FLAG_FOLLOW != 0
            && let Some(Some(at)) = slot
                .object
                .and_then(|object| self.objects.get(usize::from(object)))
        {
            slot.base = *at;
        }
        if slot.flags & FLAG_FOLLOW != 0
            && let Some(anchor) = slot.anchor
            && let Some((_, matrix)) = self.anchors.iter().find(|(other, _)| *other == anchor)
        {
            slot.hand = Some((matrix.m, matrix.t));
        }
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
        self.move_slots(sca, false);
        *rnd = self.random;
    }

    /// `Esp_move` under `fg_stop`'s `0x10000000`: only some kinds still move.
    pub fn tick_stopped(&mut self, rnd: &mut crate::zombie2::Rnd, sca: Option<&Sca>) {
        self.random = *rnd;
        self.move_slots(sca, true);
        *rnd = self.random;
    }

    fn move_slots(&mut self, sca: Option<&Sca>, stopped: bool) {
        let moves = |slot: &Slot| !stopped || MOVE_WHEN_STOPPED.contains(&slot.kind);
        for index in 0..self.slots.len() {
            let slot = self.slots[index];
            if !slot.live() || !moves(&slot) {
                continue;
            }
            if slot.parent != 0 && self.slots[usize::from(slot.parent)].flags == 0 {
                self.slots[index].kill();
            }
            let anim = self.slots[index].anim;
            self.behave(index, anim, sca);
        }
        for index in 0..self.slots.len() {
            if !moves(&self.slots[index]) {
                continue;
            }
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
    /// `Hit_ck_point4` on the cut, and `depth_bias` what `Esp_trans_3d` adds to
    /// the depth of the 3D ones.
    pub fn quads(
        &self,
        camera: &Camera,
        depth_bias: i32,
        in_view: impl Fn(i32, i32) -> bool,
    ) -> (Vec<EffectQuad>, Vec<Page>) {
        let mut quads = Vec::new();
        let mut pages = Vec::new();
        for slot in self.slots.iter().rev() {
            if slot.flags & (FLAG_LIVE | FLAG_VISIBLE) != FLAG_LIVE | FLAG_VISIBLE {
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
            let (blend, op) = if slot.flags & FLAG_SEMI == 0 {
                (0, EffectBlend::Normal)
            } else {
                // The PC's table has only the two (`DWORD_00524e24`).
                match (slot.tpage >> 5) & 3 {
                    1 => (0, EffectBlend::Add),
                    _ => (0x80, EffectBlend::Normal),
                }
            };
            if slot.flags & FLAG_3D != 0 {
                for tile in sprite.tiles.iter().skip(first).take(count) {
                    let Some((corners, key)) = corners_3d(slot, camera, tile, size, depth_bias)
                    else {
                        break;
                    };
                    let (row, v) = core_tile(&mut sheet, slot.clut, tile[1]);
                    quads.push(EffectQuad {
                        key,
                        page: page_index(&mut pages, Page { sheet, row }),
                        uv: [tile[0], v, 0, 0],
                        source: [(size - 1) as u8; 2],
                        pos: corners[0],
                        size: [1, 1],
                        corners: Some(corners),
                        mirror: [false; 2],
                        tint: [0xff; 3],
                        blend,
                        op,
                        sheet: [0, 0xffff],
                    });
                }
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
            for tile in sprite.tiles.iter().skip(first).take(count) {
                let (row, v) = core_tile(&mut sheet, slot.clut, tile[1]);
                let page = page_index(&mut pages, Page { sheet, row });
                let left = (i64::from(screen_x) << 16) + i64::from(tile[2] as i8) * texel[0];
                let top = (i64::from(screen_y) << 16) + i64::from(tile[3] as i8) * texel[1];
                let (x0, y0) = ((left >> 16) as i32, (top >> 16) as i32);
                let (x1, y1) = (((left + width) >> 16) as i32, ((top + height) >> 16) as i32);
                quads.push(EffectQuad {
                    key: ((depth.min(0x7fff) as u32) >> 5) << 5,
                    page,
                    uv: [tile[0], v, 0, 0],
                    source: [source[0] as u8, source[1] as u8],
                    pos: [x0, y0],
                    size: [x1 - x0, y1 - y0],
                    corners: None,
                    tint: [0xff; 3],
                    blend,
                    op,
                    sheet: [0, 0xffff],
                    mirror: [false; 2],
                });
            }
        }
        // The order table is last in, first out: what's submitted last draws
        // first among equal depths, so a fire covers the smoke spawned after it.
        quads.reverse();
        (quads, pages)
    }

    /// The oracle's `esp` field: how many slots are live (`+0x18`'s `0x8000`),
    /// then every slot whose `+0x18` isn't 0, the slots `Esp_call`'s free search
    /// sees as taken, as `slot:anim,update,kind,row,flags,frame,x,y,z` (`+0`,
    /// `+1`, `+0x1c`, `+0x1e`, `+0x18` and `+0x21` in hex, `+0x34` in decimal),
    /// joined by `;`, or `-` for none.
    pub fn trace_field(&self) -> String {
        let live = self.slots.iter().filter(|slot| slot.live()).count();
        let rows: Vec<String> = self
            .slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.flags != 0)
            .map(|(index, slot)| {
                let [x, y, z] = slot.pos;
                format!(
                    "{index}:{:x},{:x},{:x},{:x},{:x},{:x},{x},{y},{z}",
                    slot.anim, slot.update, slot.kind, slot.row, slot.flags, slot.frame
                )
            })
            .collect();
        if rows.is_empty() {
            format!("{live}/-")
        } else {
            format!("{live}/{}", rows.join(";"))
        }
    }
}

/// `ApplyMatrix` on a short vector, then the matrix's place: where `point`
/// on `matrix` is, in shorts as `Esp_pos` keeps it.
pub(crate) fn apply_matrix(matrix: &crate::player2::Matrix, point: [i16; 3]) -> [i16; 3] {
    std::array::from_fn(|axis| {
        let turned: i32 = (0..3)
            .map(|i| i32::from(matrix.m[axis][i]) * i32::from(point[i]))
            .sum();
        matrix.t[axis].wrapping_add(turned >> 12) as i16
    })
}

/// A page's place in the frame's list.
fn page_index(pages: &mut Vec<Page>, page: Page) -> usize {
    pages
        .iter()
        .position(|known| *known == page)
        .unwrap_or_else(|| {
            pages.push(page);
            pages.len() - 1
        })
}

/// `Esp_trans_3d` on one tile: its corners on the screen, and its depth key.
/// `None` where the original stops drawing the slot, too near the camera.
fn corners_3d(
    slot: &Slot,
    camera: &Camera,
    tile: &[u8; 4],
    size: u32,
    depth_bias: i32,
) -> Option<([[i32; 2]; 4], u32)> {
    let per_size = u32::from(slot.size).wrapping_mul(camera.fov as u32 >> 4) as i32;
    let width = (i64::from(slot.scale[0]) * i64::from(per_size)) as i32 >> 12;
    let height = (i64::from(slot.scale[1]) * i64::from(per_size)) as i32 >> 12;
    let (left, top) = (
        ((tile[2] as i8 as i32).wrapping_mul(width) >> 12) as i16,
        ((tile[3] as i8 as i32).wrapping_mul(height) >> 12) as i16,
    );
    let right = ((width as u32).wrapping_mul(size) >> 12) as i16;
    let bottom = ((height as u32).wrapping_mul(size) >> 12) as i16;
    let (right, bottom) = (left.wrapping_add(right), top.wrapping_add(bottom));
    let turn = crate::player2::rot_matrix(slot.rotation);
    let at: [i32; 3] =
        std::array::from_fn(|axis| i32::from(slot.offset[axis]) + i32::from(slot.base[axis]));
    let mut corners = [[0; 2]; 4];
    let mut depths = 0;
    for (corner, (x, y)) in
        corners
            .iter_mut()
            .zip([(left, top), (right, top), (left, bottom), (right, bottom)])
    {
        let world: [i32; 3] = std::array::from_fn(|axis| {
            let [ax, ay, _] = turn[axis].map(i32::from);
            at[axis] + ((ax * i32::from(x) + ay * i32::from(y)) >> 12)
        });
        *corner = camera.project(world)?;
        depths += camera.view_position(world)[2] >> 2;
    }
    let depth = (depths / 4).min(0x1fff);
    if depth < 0x100 {
        return None;
    }
    Some((corners, (depth_bias + depth * 4).max(0) as u32))
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

/// A one-type bank of kind 7, which doesn't move under a stop, whose block
/// moves 10 along x a tick, for other modules' tests.
#[cfg(test)]
pub(crate) fn test_moving_bank() -> Arc<Bank> {
    let mut block = tests::fire_block();
    block[12..14].copy_from_slice(&10u16.to_le_bytes());
    tests::bank_of(7, &[block])
}

/// A one-type bank of kind 7 whose block follows its matrix (`0x0800`), for
/// other modules' tests.
#[cfg(test)]
pub(crate) fn test_following_bank() -> Arc<Bank> {
    let mut first = [0u8; BLOCK_LEN];
    first[1] = 1;
    first[18..20].copy_from_slice(&0xb803u16.to_le_bytes());
    tests::bank_of(7, &[first, [0; BLOCK_LEN]])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-type bank: two frames, one tile, and a row with one group of
    /// `blocks`.
    fn bank(blocks: &[[u8; BLOCK_LEN]]) -> Arc<Bank> {
        bank_of(7, blocks)
    }

    pub(super) fn bank_of(kind: u8, blocks: &[[u8; BLOCK_LEN]]) -> Arc<Bank> {
        bank_rows(kind, &[(0, blocks)])
    }

    /// [`bank_of`] with blocks for several rows, each a group of its own.
    pub(super) fn bank_rows(kind: u8, rows: &[(u8, &[[u8; BLOCK_LEN]])]) -> Arc<Bank> {
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
        // The motion table, in words of four bytes from its start: each
        // row's one group after the table, in turn.
        let mut at = 4u16;
        let mut table = [0u16; 8];
        for (row, blocks) in rows {
            table[usize::from(*row)] = at;
            at += 2 + blocks.len() as u16 * (BLOCK_LEN / 4) as u16;
        }
        for word in table {
            record.extend_from_slice(&word.to_le_bytes());
        }
        for (_, blocks) in rows {
            record.extend_from_slice(&1u16.to_le_bytes());
            record.extend_from_slice(&0u16.to_le_bytes());
            record.extend_from_slice(&(blocks.len() as u16).to_le_bytes());
            record.extend_from_slice(&0u16.to_le_bytes());
            for block in *blocks {
                record.extend_from_slice(block);
            }
        }
        let data = container(&[Part {
            id: kind,
            record: &record,
            tim: &[],
        }]);
        Arc::new(Bank::load(None, Some(&data)))
    }

    pub(super) fn fire_block() -> [u8; BLOCK_LEN] {
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

    /// The trace lists the slots `Esp_call`'s search sees as taken, the
    /// deferred one included, after the count of live ones.
    #[test]
    fn the_trace_field_lists_the_taken_slots_after_the_live_count() {
        let mut held = [0u8; BLOCK_LEN];
        held[4..6].copy_from_slice(&0x1000u16.to_le_bytes());
        held[6..8].copy_from_slice(&0x1000u16.to_le_bytes());
        let mut effects = Effects::default();
        assert_eq!(effects.trace_field(), "0/-");
        effects.enter_room(bank(&[fire_block(), held]));
        assert_eq!(effects.trace_field(), "0/-");
        effects.call(7, 0, 0x1000, 0, [100, 0, 0], false).unwrap();
        effects.call(7, 0, 0x1000, 0, [100, 0, 0], true).unwrap();
        assert_eq!(
            effects.trace_field(),
            "1/94:2,0,7,0,4000,0,0,0,0;95:2,0,7,0,a003,0,0,0,0"
        );
        effects.tick(&mut Default::default(), None);
        assert_eq!(
            effects.trace_field(),
            "2/94:0,0,7,0,b003,1,100,0,0;95:0,0,7,0,b003,1,100,0,0"
        );
    }

    /// `Esp_kill` clears `+0x18` but leaves the kind and row, so the trace
    /// goes by the flags.
    #[test]
    fn a_killed_slot_leaves_the_trace_but_keeps_its_kind() {
        let mut effects = Effects::default();
        effects.enter_room(bank(&[fire_block()]));
        let slot = effects.call(7, 0, 0x1000, 0, [0; 3], false).unwrap();
        effects.kill(7, 0);
        assert_eq!(effects.slots[slot].kind, 7);
        assert_eq!(effects.trace_field(), "0/-");
    }

    /// `Esp_die` leaves only kind 5's rows 2 and 3.
    #[test]
    fn dying_kills_all_but_two_of_the_rows_of_kind_five() {
        let mut effects = Effects::default();
        effects.enter_room(bank(&[fire_block()]));
        let call = |effects: &mut Effects, kind, row| {
            let slot = effects.call(7, 0, 0x1000, 0, [0; 3], false).unwrap();
            effects.tick(&mut Default::default(), None);
            effects.slots[slot].kind = kind;
            effects.slots[slot].row = row;
            slot
        };
        let kept = [call(&mut effects, 5, 2), call(&mut effects, 5, 3)];
        let gone = [call(&mut effects, 5, 0), call(&mut effects, 7, 2)];
        effects.die();
        assert!(kept.iter().all(|&slot| effects.slots[slot].live()));
        assert!(!gone.iter().any(|&slot| effects.slots[slot].live()));
    }

    /// `Esp_move` under `fg_stop`: only five kinds carry on.
    #[test]
    fn a_stopped_tick_moves_only_the_kinds_that_carry_on() {
        for (kind, carries_on) in [(7, false), (0x1A, true)] {
            let mut effects = Effects::default();
            effects.enter_room(bank_of(kind, &[fire_block()]));
            let slot = effects.call(kind, 0, 0x1000, 0, [0; 3], false).unwrap();
            effects.tick(&mut Default::default(), None);
            let before = format!("{:?}", effects.slots[slot]);
            effects.tick_stopped(&mut Default::default(), None);
            let after = format!("{:?}", effects.slots[slot]);
            assert_eq!(before != after, carries_on, "kind {kind}");
        }
    }

    /// `Sce_kirakira_set`'s twinkle is called on its object's matrix:
    /// `Esp_pos` takes the object's place while the slot's `0x0800` is up
    /// (room 10C's jewel twinkle follows the statue's part down), and
    /// `Esp_kill` with the matrix takes only the slots on that object.
    #[test]
    fn a_twinkle_follows_its_object_and_dies_with_it() {
        let mut block = [0u8; BLOCK_LEN];
        block[18..20].copy_from_slice(&0x8001u16.to_le_bytes());
        let mut effects = Effects::default();
        effects.enter_room(bank(&[block]));
        let on_object = effects
            .call_on_object(7, 0, 0x2800, 1, [10, -90, 20])
            .unwrap();
        let other = effects
            .call_on_object(7, 0, 0x2800, 2, [30, -90, 40])
            .unwrap();
        effects.objects = vec![None, Some([-21550, -5690, -12500]), None];
        effects.tick(&mut Default::default(), None);
        assert_eq!(
            effects.slots[on_object].pos,
            [10, -90, 20],
            "no 0x0800, no follow"
        );
        effects.slots[on_object].flags |= FLAG_FOLLOW;
        effects.tick(&mut Default::default(), None);
        assert_eq!(effects.slots[on_object].pos, [-21550, -5690, -12500]);
        effects.kill_on_object(7, 0, 1);
        assert!(!effects.slots[on_object].live());
        assert!(effects.slots[other].live());
    }

    /// `pl_w02_t2_move`'s flash (kind 1, row 0) in `spider_shots_302`: on
    /// the hand's `Workm` (part 11) at `(0x8a, 0x267, 8)`. Its first block's
    /// `Esp_07` takes it to `0xb803`, and from then `Esp_pos` copies the hand's
    /// matrix again on each tick (`0x0800`): the original's places on ticks
    /// 361 to 363.
    #[test]
    fn the_flash_follows_the_hand_on_each_tick() {
        let mut first = [0u8; BLOCK_LEN];
        first[1] = 1;
        first[18..20].copy_from_slice(&0xb803u16.to_le_bytes());
        let mut effects = Effects::default();
        effects.enter_room(bank(&[first, [0; BLOCK_LEN]]));
        let anchor = Anchor {
            body: Body::Player,
            part: 11,
        };
        let hand = |m: [[i16; 3]; 3], t: [i32; 3]| crate::player2::Matrix { m, t };
        let at_call = hand(
            [[-6, -2262, -3414], [-4098, -2, 5], [-5, 3412, -2263]],
            [-15235, -2506, -11260],
        );
        let flash = effects
            .call_on_part(7, 0, 0xe00, 0, anchor, at_call, [0x8a, 0x267, 8])
            .unwrap();
        assert_eq!(effects.anchors_in_use(), vec![anchor]);
        effects.anchors = vec![(anchor, at_call)];
        effects.tick(&mut Default::default(), None);
        assert_eq!(effects.slots[flash].pos, [-15582, -2645, -10753]);
        assert_eq!(effects.slots[flash].flags, 0xb803);
        effects.anchors = vec![(
            anchor,
            hand(
                [[285, -2206, -3440], [-4060, -546, 15], [-470, 3406, -2227]],
                [-15203, -2490, -11302],
            ),
        )];
        effects.tick(&mut Default::default(), None);
        assert_eq!(effects.slots[flash].pos, [-15532, -2709, -10811]);
        let last = hand(
            [
                [1068, -1883, -3475],
                [-3443, -2214, 139],
                [-1944, 2884, -2164],
            ],
            [-15128, -2526, -11419],
        );
        effects.anchors = vec![(anchor, last)];
        effects.tick(&mut Default::default(), None);
        assert_eq!(effects.slots[flash].pos, [-15382, -2975, -11056]);
        // A part the tick doesn't give keeps the matrix the slot last took.
        effects.anchors.clear();
        effects.tick(&mut Default::default(), None);
        assert_eq!(effects.slots[flash].pos, [-15382, -2975, -11056]);
    }

    /// `Esp_29`'s splats are `Esp_call3`s on the blood's own matrix pointer
    /// (`+0x6c`) and point (`+0x2c`): on the part it was started on, where
    /// that part is now.
    #[test]
    fn a_splat_starts_on_its_parents_part() {
        let mut effects = Effects::default();
        effects.enter_room(bank(&[[0; BLOCK_LEN]]));
        let anchor = Anchor {
            body: Body::Enemy(3),
            part: 8,
        };
        let at_call = crate::player2::Matrix {
            m: crate::player2::rot_matrix([0; 3]),
            t: [2000, -1000, 500],
        };
        let blood = effects
            .call_on_part(7, 0, 0x1000, 0, anchor, at_call, [0, 100, 0])
            .unwrap();
        let now = crate::player2::Matrix {
            m: crate::player2::rot_matrix([0, 0x400, 0]),
            t: [2100, -900, 400],
        };
        effects.anchors = vec![(anchor, now)];
        let parent = effects.slots[blood];
        let splat = effects.call_from(&parent, 7, 0, 0x800, 0).unwrap();
        let splat = effects.slots[splat];
        assert_eq!(splat.anchor, Some(anchor));
        assert_eq!(splat.local, [0, 100, 0]);
        assert_eq!(splat.base, [2100, -800, 400]);
        assert_eq!(splat.flags, FLAG_DEFERRED);
        // One started on the world's matrix starts at its parent's place.
        let plain = effects.call(7, 0, 0x1000, 0, [5, 6, 7], false).unwrap();
        let parent = effects.slots[plain];
        let child = effects.call_from(&parent, 7, 0, 0x800, 0).unwrap();
        assert_eq!(effects.slots[child].base, [5, 6, 7]);
        assert_eq!(effects.slots[child].anchor, None);
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
    fn falling_glass_breaks_on_the_ground_and_goes() {
        let mut block = [0u8; BLOCK_LEN];
        block[1] = 5;
        block[2] = 0x27;
        block[3] = 0x27;
        block[9] = 12;
        block[18..20].copy_from_slice(&0xb003u16.to_le_bytes());
        let mut effects = Effects::default();
        effects.enter_room(bank(&[block]));
        let slot = effects.call(7, 0, 0x800, 0, [0, -100, 0], false).unwrap();
        let mut ticks = 0;
        while effects.slots[slot].live() {
            effects.tick(&mut Default::default(), None);
            ticks += 1;
            assert!(
                ticks < 100,
                "still falling at {:?}",
                effects.slots[slot].pos
            );
        }
        assert_eq!(effects.slots[slot].update, 0);
    }

    /// `Esp_tbl`'s `0x4E`, a water ripple's: the next block from `Esp_02`'s
    /// random frame, one draw of `Rnd`, with its motion scaled by the size.
    #[test]
    fn a_ripple_draws_once_and_scales_its_motion_by_its_size() {
        let mut first = [0u8; BLOCK_LEN];
        first[0] = 0x4E;
        first[11] = 1;
        let mut next = [0u8; BLOCK_LEN];
        next[8] = 30;
        next[12..14].copy_from_slice(&300i16.to_le_bytes());
        next[18..20].copy_from_slice(&0xa003u16.to_le_bytes());
        next[22..24].copy_from_slice(&1u16.to_le_bytes());
        let mut effects = Effects::default();
        effects.enter_room(bank_of(0x1A, &[first, next]));
        let slot = effects.call(0x1A, 0, 1500, 0, [0; 3], false).unwrap();
        let mut rnd = crate::zombie2::Rnd::default();
        let mut expected = rnd;
        expected.draw();
        effects.tick(&mut rnd, None);
        assert_eq!(rnd, expected);
        let slot = effects.slots[slot];
        assert_eq!(slot.accel[0], 15);
        assert_eq!(slot.velocity[0], 150);
    }

    /// `Esp_tbl`'s `0x52` draws a turn for each ripple it starts, three, two,
    /// one, two and three over five ticks, moves its base down and back up
    /// by shares of its size, and goes.
    #[test]
    fn a_splashs_rings_draw_for_each_ripple_and_go_after_five_ticks() {
        let mut block = [0u8; BLOCK_LEN];
        block[0] = 0x52;
        let mut effects = Effects::default();
        // Its ripples' row 1 isn't in this bank, so they draw and start nothing.
        effects.enter_room(bank_of(0x18, &[block]));
        let slot = effects
            .call(0x18, 0, 4096, 0, [0, -1000, 0], false)
            .unwrap();
        let mut rnd = crate::zombie2::Rnd::default();
        let mut lowest = 0;
        for draws in [3, 2, 1, 2, 3] {
            let mut expected = rnd;
            for _ in 0..draws {
                expected.draw();
            }
            effects.tick(&mut rnd, None);
            assert_eq!(rnd, expected);
            lowest = lowest.min(effects.slots[slot].base[1]);
        }
        assert_eq!(lowest, -1000 - 300 - 290 - 260 - 230 - 190);
        assert_eq!(effects.slots[slot].base[1], -1000);
        assert!(!effects.slots[slot].live());
    }

    /// `Esp_tbl`'s `0x45` draws two for each of its three flames a tick for
    /// 90 ticks, shrinking over the last 65, and goes.
    #[test]
    fn a_fire_throws_three_flames_a_tick_for_ninety_ticks() {
        let mut block = [0u8; BLOCK_LEN];
        block[0] = 0x45;
        let mut effects = Effects::default();
        // Its flames' kind 0x26 isn't in this bank, so they draw and start nothing.
        effects.enter_room(bank_of(0x18, &[block]));
        let slot = effects.call(0x18, 0, 4000, 0, [0; 3], false).unwrap();
        let mut rnd = crate::zombie2::Rnd::default();
        for tick in 0..0x5A {
            let mut expected = rnd;
            for _ in 0..6 {
                expected.draw();
            }
            effects.tick(&mut rnd, None);
            assert_eq!(rnd, expected, "tick {tick}");
        }
        assert!(effects.slots[slot].size < 4000);
        let before = rnd;
        effects.tick(&mut rnd, None);
        assert_eq!(rnd, before);
        assert!(!effects.slots[slot].live());
    }

    /// `Esp_56`, the Colt's reload: six cases, one on its first tick, two
    /// on the second and the fourth, one on the seventh, and then it's
    /// gone. Each is `Esp_call3` of kind 2, row 6 at the slot's size, turned
    /// from the slot's direction by a `Rnd` step of 400.
    #[test]
    fn the_colts_reload_throws_six_cases_over_seven_ticks_and_goes() {
        let mut reload = [0u8; BLOCK_LEN];
        reload[0] = 0x56;
        let mut case = fire_block();
        case[0] = 0;
        let mut effects = Effects::default();
        effects.enter_room(bank_rows(2, &[(0, &[reload]), (6, &[case])]));
        let slot = effects
            .call(2, 0, 0x900, 0x123, [10, 20, 30], false)
            .unwrap();
        let cases = |effects: &Effects| -> Vec<usize> {
            (0..POOL_SIZE)
                .filter(|&index| {
                    let other = &effects.slots[index];
                    other.flags != 0 && other.kind == 2 && other.row == 6
                })
                .collect()
        };
        let mut rnd = crate::zombie2::Rnd::default();
        let mut made = Vec::new();
        for _ in 0..7 {
            assert!(effects.slots[slot].live());
            let before = cases(&effects);
            let mut draws = rnd;
            effects.tick(&mut rnd, None);
            // The pool is searched from the top: the first call takes the
            // highest free slot.
            let mut new: Vec<usize> = cases(&effects)
                .into_iter()
                .filter(|index| !before.contains(index))
                .collect();
            new.sort_unstable_by(|a, b| b.cmp(a));
            for &index in &new {
                let turn = (u32::from(draws.draw()) % 0x1E * 400) as i16;
                let case = &effects.slots[index];
                assert_eq!((case.size, case.dir), (0x900, turn.wrapping_add(0x123)));
                assert_eq!(case.base, [10, 20, 30], "from where the reload was");
            }
            assert_eq!(draws, rnd, "a draw for each case and no other");
            made.push(new.len());
        }
        assert_eq!(made, [1, 2, 0, 2, 0, 0, 1]);
        assert!(!effects.slots[slot].live());
    }

    /// `Esp_call3D`'s turn (`+0x44` to `+0x48`): the slot it made and the
    /// slots that die with it take it, and no other slot does.
    #[test]
    fn a_turn_reaches_the_slot_and_its_children_only() {
        let mut effects = Effects::default();
        effects.enter_room(bank(&[fire_block()]));
        let first = effects.call(7, 0, 0x1000, 0, [0; 3], false).unwrap();
        let other = effects.call(7, 0, 0x1000, 0, [0; 3], false).unwrap();
        // A slot of `first`'s row's further group dies with it.
        let child = other - 1;
        effects.slots[child] = Slot {
            parent: first as u8,
            ..effects.slots[first]
        };
        let turn = [0, 0, 0x400];
        effects.turn(first, turn);
        assert_eq!(effects.slots[first].rotation, turn);
        assert_eq!(effects.slots[child].rotation, turn);
        assert_eq!(effects.slots[other].rotation, [0; 3]);
    }

    /// A flame of `Esp_46` in slot 0 at `pos`, ten ticks to go.
    fn flame_at(pos: [i16; 3]) -> Effects {
        let mut effects = Effects::default();
        effects.enter_room(bank_rows(4, &[(4, &[fire_block()])]));
        effects.slots[0] = Slot {
            flags: FLAG_LIVE | FLAG_ANIMATE | FLAG_INTEGRATE,
            kind: 3,
            row: 0x1D,
            update: 0x46,
            start: 10,
            accel: [2, 0, 0],
            velocity: [0x10, 0, 0],
            size: 1000,
            pos,
            ..Slot::default()
        };
        effects
    }

    /// A spark shot's effect of `row` in slot 0, `update` its behaviour, on
    /// an unturned matrix at `(0, -1000, 0)` (part 11's) at the point
    /// `pl_mv05_w3_t2` gives it.
    fn spark_at(row: u8, update: u8) -> Effects {
        let mut effects = Effects::default();
        effects.enter_room(bank_rows(6, &[(1, &[fire_block()]), (4, &[fire_block()])]));
        let unturned = [[0x1000, 0, 0], [0, 0x1000, 0], [0, 0, 0x1000]];
        effects.slots[0] = Slot {
            flags: FLAGS_NEW,
            kind: 6,
            row,
            update,
            next_flags: 0xB803,
            dir: 0x400,
            pos: [0, -1000, 0],
            base: [0, -1000, 0],
            anchor: Some(Anchor {
                body: Body::Player,
                part: 11,
            }),
            hand: Some((unturned, [0, -1000, 0])),
            local: [0x82, 0x6B8, 0],
            ..Slot::default()
        };
        effects
    }

    /// A zombie in slot 2 standing at `pos`.
    fn spark_body(pos: [i32; 3]) -> crate::weapon2::Target {
        crate::weapon2::Target {
            slot: 2,
            id: 0x1F,
            be_flg: 1,
            life: 100,
            pos,
            at: [pos[0], pos[2]],
            at_y: pos[1],
            at_w1: 400,
            ofs_y: -1000,
            at_h: 1000,
            ..Default::default()
        }
    }

    /// `Esp_0E`'s first tick: its sound, every body in the little box at the
    /// body the shot found hurt by the spark shot's row (60 in its first
    /// band), and a spark each `0x1b0` of the way there, up the gun and
    /// leaning over by the height, with four `Rnd`s each.
    #[test]
    fn the_sparks_strike_strikes_the_body_the_shot_found() {
        let mut effects = spark_at(2, 0x0E);
        effects.bodies = vec![spark_body([0, 0, 2000])];
        effects.spark_target = Some(2);
        effects.random = crate::zombie2::Rnd::default();
        effects.behave(0, 0x0E, None);
        assert_eq!(effects.sounds, [(0x110_0001, [0, -1000, 0])]);
        assert_eq!(effects.slots[0].flags, 0xB803, "its block's flags");
        let hit = effects.blasts.iter().find_map(|effect| match effect {
            crate::weapon2::BlastEffect::Hit(hit) => Some(*hit),
            crate::weapon2::BlastEffect::Tested(_) => None,
        });
        let hit = hit.expect("the body is struck");
        assert_eq!((hit.slot, hit.weapon, hit.damage), (2, 0x0E, 60));
        // 2236 away: five sparks, the 1000 up shared out as 200 a spark, and
        // a lift of a fifth of it. The first takes the highest free slot.
        let sparks: Vec<Slot> = effects.slots[1..]
            .iter()
            .rev()
            .filter(|slot| slot.flags & FLAG_DEFERRED != 0)
            .copied()
            .collect();
        assert_eq!(sparks.len(), 5);
        let to = crate::se2::se_dir_ck(0, 0, 0, 2000) as i16;
        let mut replay = crate::zombie2::Rnd::default();
        for (n, spark) in sparks.iter().enumerate() {
            let n = n as i16;
            assert_eq!((spark.kind, spark.row, spark.size), (6, 1, 0x2600));
            assert_eq!(spark.dir, to.wrapping_sub(0x400));
            assert_eq!(spark.local, [0x82 - 200 * n, 0x6B8 + 0x1B0 * n, 0]);
            let x = (i16::from(replay.draw()) % 0x1E - 0xF) * 10;
            let y = (i16::from(replay.draw()) % 0x32 - 0x19) * 0x28;
            let along = i16::from(replay.draw());
            let side = i16::from(replay.draw());
            let z = 200 + (side & 2) * 0x400 + 0x338 + along % 0x14 * 0x1E;
            assert_eq!(spark.rotation, [x, y, z], "spark {n}");
        }
        assert_eq!(effects.random, replay, "four draws a spark");
    }

    /// `Esp_0E` then flashes the screen on fade 0 and is gone on its seventh
    /// tick: `Fade_adjust` to 0x2000, 0x1800 and 0x1600, `Fade_off`, then
    /// 0x1400 and 0x1200, and `Fade_off`.
    #[test]
    fn the_sparks_strike_flashes_twice_and_goes() {
        let mut effects = spark_at(2, 0x0E);
        let mut levels = Vec::new();
        for _ in 0..7 {
            assert!(effects.slots[0].live());
            effects.behave(0, 0x0E, None);
            levels.push(effects.fade.take());
        }
        assert_eq!(
            levels,
            [0x2000, 0x1800, 0x1600, -1, 0x1400, 0x1200, -1].map(Some)
        );
        assert!(!effects.slots[0].live());
        // With no body found, nothing is struck and no sparks go out.
        assert!(effects.blasts.is_empty());
        assert!(effects.slots[1..].iter().all(|slot| slot.flags == 0));
    }

    /// `Esp_3F`: a spark that meets a wall (`Sca_ck_hit`, here the floor) is
    /// hidden, and one in the open is left alone.
    #[test]
    fn a_spark_that_meets_a_wall_is_hidden() {
        for (y, shown) in [(-1000, true), (100, false)] {
            let mut effects = spark_at(1, 0x3F);
            effects.slots[0].flags = FLAGS_NEW | FLAG_VISIBLE;
            effects.slots[0].pos = [0, y, 0];
            effects.behave(0, 0x3F, None);
            assert_eq!(effects.slots[0].flags & FLAG_VISIBLE != 0, shown, "at {y}");
        }
    }

    /// `Esp_3E`, the miss: nineteen sparks `0x1b0` apart up the gun, each
    /// turned with two `Rnd`s, and it's gone.
    #[test]
    fn the_sparks_miss_throws_nineteen_sparks() {
        let mut effects = spark_at(3, 0x3E);
        effects.random = crate::zombie2::Rnd::default();
        effects.behave(0, 0x3E, None);
        assert!(!effects.slots[0].live());
        let sparks: Vec<Slot> = effects.slots[1..]
            .iter()
            .rev()
            .filter(|slot| slot.flags & FLAG_DEFERRED != 0)
            .copied()
            .collect();
        assert_eq!(sparks.len(), 19);
        let mut replay = crate::zombie2::Rnd::default();
        for (n, spark) in sparks.iter().enumerate() {
            assert_eq!((spark.kind, spark.row, spark.size), (6, 4, 0x2000));
            assert_eq!(spark.dir, 0x400);
            assert_eq!(spark.local, [0x82, 0x6B8 + 0x1B0 * n as i16, 0]);
            let y = (i16::from(replay.draw()) % 0x32 - 0x19) * 0x28;
            let z = ((!replay.draw() & 2 | 1) as i16) << 10;
            assert_eq!(spark.rotation[1..], [y, z], "spark {n}");
        }
        assert_eq!(effects.random, replay);
    }

    /// A rocket (kind 2 row 0xD's second block) at `pos`, flying on
    /// `Esp_18`, bursting on `Esp_2F` and steps back from a wall to `Esp_0D`.
    fn rocket_at(pos: [i16; 3]) -> Effects {
        let mut effects = Effects::default();
        effects.enter_room(bank_rows(3, &[(2, &[fire_block()])]));
        effects.slots[0] = Slot {
            flags: 0xB403,
            kind: 2,
            row: 0x0D,
            anim: 0x17,
            update: 0x18,
            start: 0x2F,
            count: 0x0D,
            pos,
            ..Slot::default()
        };
        effects
    }

    fn hits(effects: &Effects) -> Vec<crate::weapon2::BlastHit> {
        effects
            .blasts
            .iter()
            .filter_map(|effect| match effect {
                crate::weapon2::BlastEffect::Hit(hit) => Some(*hit),
                crate::weapon2::BlastEffect::Tested(_) => None,
            })
            .collect()
    }

    /// `Esp_18` hurts every body in its box at the rocket (row 0x11 of
    /// `Wp_dmg_tbl`, a zombie's 900) and bursts: `Esp_2F`, row 0xD's sound.
    #[test]
    fn a_rocket_hurts_every_body_it_meets_and_bursts() {
        let mut effects = rocket_at([0, -1000, 0]);
        let mut other = spark_body([300, 0, -300]);
        other.slot = 5;
        effects.bodies = vec![spark_body([0, 0, 0]), other];
        effects.behave(0, 0x18, None);
        let hits = hits(&effects);
        assert_eq!(
            hits.iter()
                .map(|hit| (hit.slot, hit.weapon, hit.damage))
                .collect::<Vec<_>>(),
            [(2, 0x11, 900), (5, 0x11, 900)]
        );
        assert_eq!(effects.slots[0].update, 0x2F, "it bursts");
        assert_eq!(effects.sounds, [(0x114_0001, [0, -1000, 0])]);
    }

    /// Out of reach of the bodies at its height, `Esp_18` looks again on the
    /// floor below (`Sca_ck_hit`), for the first body there only; with
    /// nothing there it flies on, the floor kept at `+0x14`.
    #[test]
    fn a_rocket_hurts_the_first_body_on_the_floor_below_it() {
        let mut effects = rocket_at([0, -3000, 0]);
        let mut other = spark_body([300, 0, -300]);
        other.slot = 5;
        effects.bodies = vec![spark_body([0, 0, 0]), other];
        effects.behave(0, 0x18, None);
        let hits = hits(&effects);
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert_eq!((hits[0].slot, hits[0].weapon), (2, 0x11));
        assert_eq!(effects.slots[0].update, 0x2F);

        let mut effects = rocket_at([0, -3000, 0]);
        effects.behave(0, 0x18, None);
        assert!(effects.blasts.is_empty());
        let slot = effects.slots[0];
        assert_eq!((slot.update, slot.ground), (0x18, -10));
        assert!(effects.sounds.is_empty());
    }

    /// In a wall (`Sca_ck_hit` blocked) `Esp_18` steps the rocket back by
    /// its speed and a third more and takes `+3`'s behaviour; past 0x7D00 it
    /// goes.
    #[test]
    fn a_rocket_steps_back_from_a_wall_and_goes_past_the_edge() {
        let mut effects = rocket_at([0, 100, 0]);
        effects.slots[0].velocity = [30, 0, 0];
        effects.behave(0, 0x18, None);
        let slot = effects.slots[0];
        assert_eq!(slot.offset[0], -40);
        assert_eq!(slot.pos[0], -40);
        assert_eq!((slot.flags, slot.anim), (FLAGS_NEW, 0), "`Esp_0D`");

        for pos in [[0x7D01, -1000, 0], [0, -1000, -0x7D01]] {
            let mut effects = rocket_at(pos);
            effects.behave(0, 0x18, None);
            let slot = effects.slots[0];
            assert_eq!((slot.flags, slot.anim, slot.update), (0, 0, 0x18));
        }
    }

    /// `Esp_17`: a rocket leaves smoke where it is, on the next tick.
    #[test]
    fn a_rocket_leaves_smoke_behind_it() {
        let mut effects = rocket_at([100, -1000, 200]);
        effects.behave(0, 0x17, None);
        let smoke = effects.slots[1..]
            .iter()
            .find(|slot| slot.flags != 0)
            .expect("a smoke");
        assert_eq!((smoke.kind, smoke.row, smoke.size), (3, 0x0A, 0x1800));
        assert_eq!(smoke.base, [100, -1000, 200]);
        assert!(smoke.flags & FLAG_DEFERRED != 0);
    }

    /// `Esp_46` in the air with nothing in reach goes on, its count down.
    #[test]
    fn a_flame_with_nothing_in_reach_goes_on() {
        let mut effects = flame_at([0, -1000, 0]);
        effects.behave(0, 0x46, None);
        let slot = effects.slots[0];
        assert_eq!((slot.start, slot.count, slot.size), (9, 0, 1000));
        assert_eq!(slot.velocity[0], 0x10);
        assert!(effects.blasts.is_empty());
    }

    /// `Esp_46` burns the first body in its box (`FUN_004d7e30` with
    /// `0x20010`: the flamethrower's row, by height) and stops: it rises on
    /// frame 0xc, and three ticks on leaves smoke (kind 4, row 0xc).
    #[test]
    fn a_flame_burns_the_body_in_reach_then_rises_and_smokes() {
        let mut effects = flame_at([0, -1000, 0]);
        effects.bodies = vec![crate::weapon2::Target {
            slot: 1,
            id: 0x1F,
            be_flg: 1,
            life: 50,
            pos: [-700, 0, 0],
            at: [-700, 0],
            at_w1: 400,
            ofs_y: -1000,
            at_h: 1000,
            ..Default::default()
        }];
        effects.behave(0, 0x46, None);
        let hit = effects.blasts.iter().find_map(|effect| match effect {
            crate::weapon2::BlastEffect::Hit(hit) => Some(*hit),
            crate::weapon2::BlastEffect::Tested(_) => None,
        });
        let hit = hit.expect("the body burns");
        assert_eq!(
            (hit.slot, hit.weapon, hit.damage, hit.damage_no),
            (1, 0x10, 15, 1)
        );
        let slot = effects.slots[0];
        assert_eq!((slot.start, slot.count, slot.frame), (6, 1, 0x0C));
        assert_eq!(slot.velocity[..2], [0, 0x14]);
        assert_eq!(slot.accel[..2], [0, 0]);
        assert_eq!(slot.size, 1000, "out of the walls it keeps its size");
        let smoke = |effects: &Effects| {
            effects.slots[1..]
                .iter()
                .any(|slot| slot.flags & FLAG_DEFERRED != 0 && slot.kind == 4 && slot.row == 0x0C)
        };
        for start in [5, 4] {
            effects.behave(0, 0x46, None);
            assert_eq!(effects.slots[0].start, start);
            assert!(!smoke(&effects));
        }
        effects.behave(0, 0x46, None);
        assert!(smoke(&effects));
    }

    /// Under the floor (`Sca_ck_hit`'s height above it, and `DWORD_00695e84`)
    /// a flame stops, shrunk to 70 in 100.
    #[test]
    fn a_flame_stops_shrunk_under_the_floor() {
        let mut effects = flame_at([0, 100, 0]);
        effects.behave(0, 0x46, None);
        let slot = effects.slots[0];
        assert_eq!((slot.count, slot.size), (1, 700));
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

    /// `Esp_05` in water: a case that falls below the water's level goes
    /// there, in a splash (kind 0x1a, row 1, which this bank doesn't have),
    /// without reaching the floor or bouncing.
    #[test]
    fn a_case_falling_into_water_goes_in_a_splash() {
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
        effects.water = vec![crate::zombie2::WaterArea {
            zone: [-1000, -1000, 2000, 2000],
            quad: None,
            level: -50,
        }];
        let slot = effects.call(7, 0, 0x700, 0, [0, -100, 0], false).unwrap();
        let mut ticks = 0;
        while effects.slots[slot].live() {
            effects.tick(&mut Default::default(), None);
            ticks += 1;
            assert!(ticks < 40, "still falling at {:?}", effects.slots[slot].pos);
        }
        assert!(effects.sounds.is_empty(), "it never bounced");
        let y = effects.slots[slot].pos[1];
        assert!(
            -50 < y && y <= 0,
            "gone at {y}, under the water but over the floor"
        );
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
    fn a_drips_splash_stops_falling_and_moving() {
        let mut effects = Effects::default();
        effects.enter_room(bank(&[fire_block(), [0; BLOCK_LEN]]));
        let slot = effects.call(7, 0, 0x1000, 0, [0, -100, 0], false).unwrap();
        effects.slots[slot].anim = 0x2B;
        effects.slots[slot].update = 5;
        effects.slots[slot].ground = -20;
        effects.tick(&mut Default::default(), None);
        let slot = effects.slots[slot];
        assert_eq!((slot.anim, slot.update), (0, 0));
        assert_eq!(slot.flags & FLAG_INTEGRATE, 0);
    }

    fn spit(effects: &mut Effects, target: Target) -> usize {
        effects.enter_room(bank(&[fire_block(), [0; BLOCK_LEN]]));
        effects.target = Some(target);
        let slot = effects.call(7, 8, 0x1000, 0, [0, -500, 0], false).unwrap();
        let this = &mut effects.slots[slot];
        this.anim = 0x20;
        this.update = 0;
        this.start = 0x34;
        this.scale = [0x1000, 0x1000];
        this.pos = [0, -500, 0];
        slot
    }

    fn player_at(x: i32) -> Target {
        Target {
            pos: [x, 0, 0],
            ofs_y: -0x5FA,
            at_h: 0x5FA,
            life: 200,
            ..Target::default()
        }
    }

    #[test]
    fn a_spit_in_reach_hurts_and_poisons_the_player_and_splashes() {
        let mut effects = Effects::default();
        let slot = spit(&mut effects, player_at(700));
        effects.tick(&mut Default::default(), None);
        let target = effects.target.unwrap();
        assert_eq!(target.life, 170);
        assert_eq!(target.routine, Some(2));
        assert!(target.poisoned);
        assert_eq!(target.damage_cnt, 0x80);
        let this = effects.slots[slot];
        assert_eq!((this.anim, this.update, this.flags), (0, 0, 0xB001));
        assert!(effects.sounds.iter().any(|&(id, _)| id == 0x304_0001));
    }

    #[test]
    fn a_hit_in_hard_mode_takes_a_quarter_more() {
        let mut target = Target {
            life: 100,
            difficulty: crate::zombie2::Difficulty {
                hard: true,
                uncensored: false,
            },
            ..player_at(0)
        };
        assert!(target.hit([0, 0, 0], 800, 20));
        assert_eq!(target.life, 75);
    }

    #[test]
    fn a_spit_out_of_reach_swells_as_it_flies() {
        let mut effects = Effects::default();
        let slot = spit(&mut effects, player_at(900));
        effects.tick(&mut Default::default(), None);
        assert_eq!(effects.target.unwrap().life, 200);
        let this = effects.slots[slot];
        assert_eq!(this.scale, [0x1000 + 0x9B, 0x1000 + 0x14]);
        assert_eq!((this.anim, this.generation), (0x20, 1));
    }

    /// A ceiling spider's spit (kind 0x1D, row 0x0A) on `Esp_38`, `y` up,
    /// with `+2`'s behaviour 0x23, which ends it; the bank holds kind 3,
    /// the splash.
    fn ceiling_spit(effects: &mut Effects, target: Target, y: i16) -> usize {
        effects.enter_room(bank_rows(3, &[(7, &[fire_block(), [0; BLOCK_LEN]])]));
        effects.target = Some(target);
        let slot = 0x40;
        effects.slots[slot] = Slot {
            kind: 0x1D,
            row: 0x0A,
            update: 0x38,
            start: 0x23,
            pos: [0, y, 0],
            base: [0, y, 0],
            flags: FLAG_LIVE,
            ..Slot::default()
        };
        slot
    }

    #[test]
    fn a_ceiling_spit_hurts_and_poisons_the_player_within_600() {
        let mut effects = Effects::default();
        let slot = ceiling_spit(&mut effects, player_at(550), -1000);
        effects.tick(&mut Default::default(), None);
        let target = effects.target.unwrap();
        // 20, as `Pl_life_down` takes it on normal.
        assert_eq!(target.life, 170);
        assert!(target.poisoned);
        assert_eq!(target.damage_cnt, 0x80);
        assert_eq!(effects.slots[slot].flags, 0);
    }

    #[test]
    fn a_ceiling_spit_out_of_reach_falls_on() {
        let mut effects = Effects::default();
        let slot = ceiling_spit(&mut effects, player_at(650), -1000);
        effects.tick(&mut Default::default(), None);
        assert_eq!(effects.target.unwrap().life, 200);
        assert_eq!(effects.slots[slot].update, 0x38);
        assert!(effects.slots.iter().all(|s| s.kind != 3));
    }

    #[test]
    fn a_ceiling_spit_splashes_within_700_of_the_ground() {
        let mut effects = Effects::default();
        let slot = ceiling_spit(&mut effects, player_at(5000), -699);
        effects.tick(&mut Default::default(), None);
        assert_eq!(effects.slots[slot].flags, 0);
        let splash = effects.slots.iter().find(|s| s.kind == 3).unwrap();
        assert_eq!(
            (splash.row, splash.size, splash.base),
            (0x0F, 0x2800, [0, -699, 0])
        );
    }

    #[test]
    fn a_ceiling_spit_starts_its_fall_on_the_ground_below() {
        let mut effects = Effects::default();
        let slot = ceiling_spit(&mut effects, player_at(5000), -1000);
        let this = &mut effects.slots[slot];
        this.update = 0;
        this.step = 0x2C;
        this.next_flags = 0x1234;
        effects.behave(slot, 0x36, None);
        let this = effects.slots[slot];
        assert_eq!((this.anim, this.update, this.ground), (0x2C, 0x38, 0));
        assert_eq!(this.rotation[1], 0x1234);
    }

    /// Room 405's stream (kind 0x1C): its drops (row 2) and their splashes
    /// (kind 0x1A).
    fn stream_effects() -> Effects {
        let mut drop = [0u8; BLOCK_LEN];
        drop[4..6].copy_from_slice(&0x1000u16.to_le_bytes());
        drop[6..8].copy_from_slice(&0x1000u16.to_le_bytes());
        drop[18..20].copy_from_slice(&0xb003u16.to_le_bytes());
        let mut bank = Arc::try_unwrap(bank_rows(0x1C, &[(2, &[drop])])).unwrap();
        bank.sprites.extend(
            Arc::try_unwrap(bank_of(0x1A, &[fire_block()]))
                .unwrap()
                .sprites,
        );
        let mut effects = Effects::default();
        effects.enter_room(Arc::new(bank));
        effects
    }

    fn stream_at(effects: &mut Effects, y: i16) -> usize {
        let slot = 0x10;
        effects.slots[slot] = Slot {
            kind: 0x1C,
            row: 1,
            size: 0x1000,
            base: [0, y, 0],
            pos: [0, y, 0],
            flags: 0xB003,
            next_flags: 0xB003,
            anim: 0x47,
            ..Slot::default()
        };
        slot
    }

    fn drops(effects: &Effects) -> usize {
        effects
            .slots
            .iter()
            .filter(|s| s.flags != 0 && s.kind == 0x1C && s.row == 2)
            .count()
    }

    #[test]
    fn a_stream_fills_in_until_a_drop_reaches_the_ground() {
        let mut effects = stream_effects();
        let stream = stream_at(&mut effects, 100);
        effects.behave(stream, 0x47, None);
        assert_eq!(drops(&effects), 1);
        let drop = effects.slots[POOL_SIZE - 1];
        assert_eq!((drop.frame, drop.update, drop.flags), (0x0A, 0, 0xB003));
        let this = effects.slots[stream];
        assert_eq!((this.anim, this.update, this.flags), (0x55, 0, 0x8000));
    }

    #[test]
    fn a_stream_that_never_lands_fills_the_pool() {
        let mut effects = stream_effects();
        let stream = stream_at(&mut effects, -100);
        effects.behave(stream, 0x47, None);
        assert_eq!(drops(&effects), POOL_SIZE - 1);
        assert!(effects.slots.iter().all(|s| s.flags != 0));
        assert!(
            effects
                .slots
                .iter()
                .filter(|s| s.row == 2)
                .all(|s| s.update == 0x44 && s.next_flags == 6)
        );
        assert_eq!(effects.slots[stream].anim, 0x55);
    }

    #[test]
    fn a_running_stream_makes_a_drop_every_fourth_tick() {
        let mut effects = stream_effects();
        let stream = stream_at(&mut effects, -100);
        for _ in 0..8 {
            effects.behave(stream, 0x55, None);
        }
        assert_eq!(drops(&effects), 2);
        assert_eq!(effects.slots[stream].next_flags, 0xB003 + 8);
        assert!(
            effects
                .slots
                .iter()
                .filter(|s| s.row == 2)
                .all(|s| s.flags == FLAG_DEFERRED)
        );
    }

    fn drop_at(effects: &mut Effects, y: i16) -> usize {
        let slot = 0x20;
        effects.slots[slot] = Slot {
            kind: 0x1C,
            row: 2,
            size: 0x800,
            pos: [0, y, 0],
            flags: 0xB003,
            update: 0x44,
            frame: 0x0A,
            velocity: [0, 40, 0],
            accel: [0, 4, 0],
            scale: [0x1000, 0x1000],
            ..Slot::default()
        };
        slot
    }

    #[test]
    fn a_drop_lengthens_as_it_falls() {
        let mut effects = stream_effects();
        let drop = drop_at(&mut effects, -1000);
        effects.behave(drop, 0x44, None);
        let this = effects.slots[drop];
        assert_eq!((this.frame, this.scale[1], this.update), (6, 0x1078, 0x44));
    }

    #[test]
    fn a_drop_lands_on_the_ground() {
        let mut effects = stream_effects();
        let drop = drop_at(&mut effects, 50);
        effects.behave(drop, 0x44, None);
        let this = effects.slots[drop];
        assert_eq!((this.frame, this.update, this.pos[1]), (8, 0, 0));
        assert_eq!((this.velocity[1], this.accel[1]), (0, 0));
    }

    #[test]
    fn a_drop_on_a_body_splashes_twice() {
        let mut effects = stream_effects();
        let drop = drop_at(&mut effects, -500);
        let body = |be_flg| crate::zombie2::Oba {
            be_flg,
            atd: crate::zombie2::Atd {
                at_w: 100,
                at_h: 100,
                at: [200, -400, 0],
                ..Default::default()
            },
            ..Default::default()
        };
        effects.oba = vec![body(3)];
        effects.behave(drop, 0x44, None);
        assert_eq!(
            effects.slots[drop].frame, 6,
            "a body out of play is no target"
        );
        effects.oba = vec![body(1)];
        effects.behave(drop, 0x44, None);
        let this = effects.slots[drop];
        assert_eq!((this.frame, this.update, this.scale[1]), (0x0B, 0, 0x1000));
        assert_eq!(effects.slots.iter().filter(|s| s.kind == 0x1A).count(), 2);
    }

    #[test]
    fn a_falling_blob_narrows_and_lengthens_fast_then_slowly() {
        let mut effects = Effects::default();
        let slot = ceiling_spit(&mut effects, player_at(5000), -1000);
        effects.slots[slot].scale = [0x1000, 0x1000];
        effects.behave(slot, 0x37, None);
        let this = effects.slots[slot];
        assert_eq!(
            (this.scale, this.generation),
            ([0x1000 - 0xCC, 0x1000 + 0x170], 1)
        );
        effects.slots[slot].scale = [0x1000, 0x1000];
        effects.slots[slot].generation = 0x14;
        effects.behave(slot, 0x37, None);
        let this = effects.slots[slot];
        assert_eq!(
            (this.scale, this.generation),
            ([0x1000 - 0x14, 0x1000 + 0x28], 0x14)
        );
    }

    #[test]
    fn a_landed_blob_squashes_then_splashes_bigger() {
        let mut effects = Effects::default();
        let slot = ceiling_spit(&mut effects, player_at(5000), 50);
        let this = &mut effects.slots[slot];
        this.scale = [0x1000, 0x1400];
        this.size = 0x1000;
        this.velocity[1] = 30;
        this.accel[1] = 2;
        effects.behave(slot, 0x39, None);
        let this = effects.slots[slot];
        assert_eq!((this.anim, this.update), (0, 0x39));
        assert_eq!((this.velocity[1], this.accel[1]), (0, 0));
        assert_eq!(this.scale, [0x1000 + 0x199, 0x1400 - 0x600]);
        effects.behave(slot, 0x39, None);
        let this = effects.slots[slot];
        // `Esp_34`'s splash from frame 0xD, with its sound.
        assert_eq!((this.size, this.frame, this.flags), (0x14CC, 0x0D, 0xB001));
        assert_eq!(this.scale, [0x1100, 0x0A00]);
        assert!(effects.sounds.iter().any(|&(id, _)| id == 0x304_0001));
    }

    /// Room 407's steam vent (kind 0x28, `Esp_4B`) and its puffs (row 1).
    fn steam_vent(effects: &mut Effects, off: bool) -> usize {
        effects.enter_room(bank_rows(0x28, &[(1, &[fire_block()])]));
        effects.steam_off = off;
        let slot = 0x10;
        effects.slots[slot] = Slot {
            kind: 0x28,
            size: 0x1000,
            scale: [0x1000, 0x1000],
            base: [0, -7000, 0],
            flags: 0xA003,
            anim: 0x4B,
            ..Slot::default()
        };
        slot
    }

    fn puffs(effects: &Effects) -> Vec<Slot> {
        effects
            .slots
            .iter()
            .filter(|s| s.flags != 0 && s.kind == 0x28 && s.row == 1)
            .copied()
            .collect()
    }

    #[test]
    fn a_steam_vent_puffs_every_fifth_tick() {
        let mut effects = Effects::default();
        let vent = steam_vent(&mut effects, false);
        for _ in 0..6 {
            effects.behave(vent, 0x4B, None);
        }
        let puffs = puffs(&effects);
        assert_eq!(puffs.len(), 2);
        assert!(
            puffs
                .iter()
                .all(|p| p.flags == 0xB803 && p.base == [0, -7000, 0])
        );
        let this = effects.slots[vent];
        assert_eq!((this.flags, this.next_flags), (0x8000, 6));
    }

    #[test]
    fn a_steam_vent_turned_off_narrows_and_goes_out() {
        let mut effects = Effects::default();
        let vent = steam_vent(&mut effects, true);
        effects.behave(vent, 0x4B, None);
        assert_eq!(effects.slots[vent].scale[0], 0xCCC);
        assert_eq!(puffs(&effects)[0].scale[0], 0xCCC);
        for _ in 1..0x5A {
            effects.behave(vent, 0x4B, None);
        }
        assert_eq!(effects.slots[vent].flags, 0xA003);
        effects.behave(vent, 0x4B, None);
        assert_eq!(effects.slots[vent].flags, 0);
        assert_eq!(effects.slots[vent].next_tpage, 0x5B);
    }

    #[test]
    fn a_puff_settles_on_the_ground() {
        let mut effects = Effects::default();
        let vent = steam_vent(&mut effects, false);
        effects.slots[vent] = Slot {
            kind: 0x28,
            row: 1,
            update: 0x57,
            pos: [0, -100, 0],
            velocity: [0, 30, 0],
            flags: 0xB803,
            ..Slot::default()
        };
        effects.behave(vent, 0x57, None);
        assert_eq!(effects.slots[vent].update, 0x57);
        effects.slots[vent].pos[1] = 40;
        effects.behave(vent, 0x57, None);
        let this = effects.slots[vent];
        assert_eq!(
            (this.update, this.frame, this.pos[1], this.velocity[1]),
            (0, 8, 0, 0)
        );
    }

    #[test]
    fn a_spit_misses_a_player_still_reeling_from_a_hit() {
        let mut effects = Effects::default();
        spit(
            &mut effects,
            Target {
                damage_cnt: 1,
                ..player_at(0)
            },
        );
        effects.tick(&mut Default::default(), None);
        assert_eq!(effects.target.unwrap().life, 200);
    }

    #[test]
    fn a_resting_effect_comes_back_a_little_smaller() {
        let mut effects = Effects::default();
        effects.enter_room(bank(&[fire_block(), [0; BLOCK_LEN]]));
        let slot = effects.call(7, 0, 0x1000, 0, [0, 0, 0], false).unwrap();
        effects.slots[slot].frame = effects.slots[slot].start;
        effects.slots[slot].flags |= 0x2003;
        effects.behave(slot, 0x41, None);
        let rest = effects.slots[slot];
        assert!((0x32..0x64).contains(&rest.count));
        assert_eq!(rest.flags & 0x2003, 0);
        effects.slots[slot].count = 1;
        effects.behave(slot, 0x41, None);
        let back = effects.slots[slot];
        assert_eq!(back.flags & 0x2003, 0x2003);
        assert_eq!(back.scale[0], back.scale[1]);
        assert!(back.scale[0] <= back.size && back.size - back.scale[0] <= 90);
    }

    #[test]
    fn a_kill_by_group_takes_only_that_group() {
        let mut effects = Effects::default();
        effects.enter_room(bank(&[fire_block(), [0; BLOCK_LEN]]));
        let one = effects
            .call_placed(EsprCall {
                kind: 7,
                row: 0,
                size: 0x1000,
                dir: 0,
                at: [0; 3],
                rotation: [0; 3],
                group: 2,
            })
            .unwrap();
        let other = effects
            .call_placed(EsprCall {
                kind: 7,
                row: 0,
                size: 0x1000,
                dir: 0,
                at: [0; 3],
                rotation: [0; 3],
                group: 3,
            })
            .unwrap();
        effects.kill_group(2);
        assert!(!effects.slots[one].live());
        assert!(effects.slots[other].live());
    }

    #[test]
    fn a_3d_effect_turns_on_its_rotation_and_sorts_by_its_depth() {
        let camera = Camera::from_points([0, 0, -8000], [0, 0, 0], 208);
        let slot = Slot {
            scale: [4096, 4096],
            size: 0x1000,
            ..Slot::default()
        };
        let tile = [0, 0, 0xf0, 0xf0];
        let (flat, key) = corners_3d(&slot, &camera, &tile, 32, 0).unwrap();
        assert_ne!(flat[0][0], flat[1][0]);
        assert_eq!(flat[0][1], flat[1][1]);
        assert!((7990..=8000).contains(&key), "{key}");
        let (_, biased) = corners_3d(&slot, &camera, &tile, 32, 2000).unwrap();
        assert_eq!(biased, key + 2000);
        let edge_on = Slot {
            rotation: [0, 0x400, 0],
            ..slot
        };
        let (turned, _) = corners_3d(&edge_on, &camera, &tile, 32, 0).unwrap();
        assert!((turned[0][0] - turned[1][0]).abs() <= 8);
        assert!((flat[0][0] - flat[1][0]).abs() > 8);
    }

    #[test]
    fn a_3d_effect_too_near_the_camera_is_not_drawn() {
        let camera = Camera::from_points([0, 0, -500], [0, 0, 0], 100);
        let slot = Slot {
            scale: [4096, 4096],
            size: 0x100,
            ..Slot::default()
        };
        assert!(corners_3d(&slot, &camera, &[0, 0, 0xf0, 0xf0], 32, 0).is_none());
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
