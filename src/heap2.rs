//! RE 2's `Work_buffer` (`0x8FF8A0`), the memory a room's data and its enemies' works are put in, as
//! far as it can be known from the files the original loads into it.
//!
//! An enemy's work isn't cleared when it's put in play: `Sce_em_set` and the enemies' inits set most
//! of it, but not all, and what they leave keeps whatever last sat at that address. A zombie's
//! `Dest` (`+0x1C4`, `+0x1C6`) is the field that shows: its walk turns toward it until `Root_ck`
//! picks one. The works aren't in a fixed array: `Set_room` loads the room's music banks and then the
//! RDT at the start of `Work_buffer`, sets `Mem_top` to the RDT's section 2 (the PlayStation's
//! samples, which the PC doesn't use), and each `Sce_em_set` puts its work at `Mem_top` and the
//! model after it. So the memory under a new work is this room's RDT, a model this room or an earlier
//! one loaded, or what an earlier room's works and parts left. [`Heap`] keeps that image: the bytes
//! that came from a file, the parts' colour and each zombie's `Dest` as it left its room. A byte no
//! modelled writer put there (what the title and the prologue left, an enemy's live matrices) is
//! unknown, and so is everything from a boot that didn't load its rooms in order (a warp, a save).
//! Where a new zombie's `Dest` sits on an unknown byte, it starts at 0, as before.

use std::fmt;
use std::sync::Arc;

/// `Sce_em_set`'s work: the model goes after it.
pub const WORK_SIZE: usize = 0x248;
/// One `PARTS_W`.
pub const PART_SIZE: usize = 0xAC;
/// `Dest`'s offset in a work.
pub const DEST: usize = 0x1C4;
/// A part's `Poly_rgb`, which `PartsWork_link` sets to 0x808080.
const PART_POLY_RGB: usize = 0x70;
/// `Snd_bgm_main` and `Snd_bgm_sub` load their bank at `Mem_top + 0x10` (rounded down to 16 bytes),
/// with `Mem_top` at the start of `Work_buffer`.
const BGM_AT: usize = 0x10;
/// The RDT's header (8 bytes and 23 section offsets), which `Set_room` relocates.
const RDT_HEADER: usize = 100;
/// `Work_buffer`'s address, which the relocated pointers hold.
const BASE: u32 = 0x8F_F8A0;
/// How far the image goes: `Work_buffer` (0x28000 bytes) runs on into `Bg_buffer`, which ends at
/// `pCom_work` (0x9800A0). The largest RDT is 0x6FDF8 bytes. Nothing past this is kept, and a
/// `Mem_top` past it is unknown.
pub const CAP: usize = 0x9800A0 - BASE as usize;
/// The models and banks [`Files::of_pack`] keeps read at once, at most.
const CACHE_ENTRIES: usize = 64;
/// `Psp_init0` and `Psp_init1`'s works for each of the room's `rdt[7]` sprites.
const PSP_SIZE: usize = 4 + 0x40;
/// `Computer200`'s work, which it puts at `Mem_top` and takes back on its exit.
pub const COMPUTER_SIZE: usize = 0x53E0;

/// Where the heap reads the files the original loads: the pack's `heap/` entries
/// ([`rdt_entry`], [`tim_entry`], [`bgm_entry`]) and the enemies' EMDs.
pub trait Source: Send + Sync {
    fn read(&self, entry: &str) -> Option<Arc<[u8]>>;
}

/// The files, when the session has them. A session without them (a pack built before they were
/// kept, a test) knows no byte.
#[derive(Clone, Default)]
pub struct Files(pub Option<Arc<dyn Source>>);

impl Files {
    fn read(&self, entry: &str) -> Option<Arc<[u8]>> {
        self.0.as_ref()?.read(entry)
    }
}

impl fmt::Debug for Files {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(if self.0.is_some() {
            "Files(..)"
        } else {
            "Files(none)"
        })
    }
}

impl PartialEq for Files {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        }
    }
}

impl Eq for Files {}

/// A pack the files are read from, opened again from its file so that the game state can hold it.
struct PackSource {
    pack: std::sync::Mutex<crate::pack::Pack>,
    /// The models and the music banks, which rooms load again and again.
    cache: std::sync::Mutex<std::collections::HashMap<String, Option<Arc<[u8]>>>>,
}

impl Source for PackSource {
    fn read(&self, entry: &str) -> Option<Arc<[u8]>> {
        let keep = !entry.ends_with(".rdt");
        if keep && let Some(bytes) = self.cache.lock().ok()?.get(entry) {
            return bytes.clone();
        }
        let bytes: Option<Arc<[u8]>> = self
            .pack
            .lock()
            .ok()?
            .read(entry)
            .ok()
            .map(|bytes| Arc::from(&*bytes));
        if keep {
            let mut cache = self.cache.lock().ok()?;
            if cache.len() >= CACHE_ENTRIES {
                cache.clear();
            }
            cache.insert(entry.to_owned(), bytes.clone());
        }
        bytes
    }
}

impl Files {
    /// The files of `pack`, when it keeps them (a pack converted before they were kept has no
    /// `heap/` entries) and was opened from a file.
    pub fn of_pack(pack: &crate::pack::Pack) -> Self {
        let Some(path) = pack.source() else {
            return Self(None);
        };
        // Every RE 2 install has room 100's RDT.
        if !pack.contains(&rdt_entry("1000")) {
            return Self(None);
        }
        match crate::pack::Pack::open(path) {
            Ok(pack) => Self(Some(Arc::new(PackSource {
                pack: std::sync::Mutex::new(pack),
                cache: Default::default(),
            }))),
            Err(_) => Self(None),
        }
    }
}

/// The pack entry of a room's RDT as the original loads it, by [`crate::state::RoomId::code`].
pub fn rdt_entry(code: &str) -> String {
    format!("heap/{code}.rdt")
}

/// The pack entry of an enemy's TIM as the original loads it (`pl<s>\emd<s>\em<s><id>.tim`).
pub fn tim_entry(id: u8, disc: u8) -> String {
    format!("heap/em{disc}{id:02x}.tim")
}

/// The pack entry of a music bank (`common\sound\bgm\main<nn>.bgm`, `sub_<nn>.bgm`).
pub fn bgm_entry(sub: bool, id: u8) -> String {
    let kind = if sub { "sub_" } else { "main" };
    format!("heap/{kind}{id:02x}.bgm")
}

/// The pack entry of an enemy's model, which the pack keeps as the original's file.
fn emd_entry(id: u8, disc: u8) -> String {
    format!("enemy/{id:02x}/enemy_{disc}.emd")
}

/// The bytes, and which of them are known.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Image {
    bytes: Vec<u8>,
    known: Vec<bool>,
}

impl Image {
    /// Grow to `end`, up to [`CAP`]; returns how far the image goes.
    fn reach(&mut self, end: usize) -> usize {
        let end = end.min(CAP);
        if self.bytes.len() < end {
            self.bytes.resize(end, 0);
            self.known.resize(end, false);
        }
        self.bytes.len()
    }

    /// Write `data` at `at`; what goes past [`CAP`] is dropped.
    fn write(&mut self, at: usize, data: &[u8]) {
        let end = at.saturating_add(data.len());
        let end = end.min(self.reach(end));
        if at >= end {
            return;
        }
        let len = end - at;
        self.bytes[at..end].copy_from_slice(&data[..len]);
        self.known[at..end].fill(true);
    }

    /// Add `by` to the word at `at` (a relocation): known when the word was.
    fn relocate(&mut self, at: usize, by: u32) {
        match self.word(at) {
            Some(word) => self.write(at, &word.wrapping_add(by).to_le_bytes()),
            None => self.forget(at, 4),
        }
    }

    fn forget(&mut self, at: usize, len: usize) {
        let end = at.saturating_add(len).min(self.known.len());
        if at < end {
            self.known[at..end].fill(false);
        }
    }

    fn copy_within(&mut self, from: usize, len: usize, to: usize) {
        let end = self.reach(from.max(to).saturating_add(len));
        let len = len.min(end.saturating_sub(from.max(to)));
        if len == 0 {
            return;
        }
        self.bytes.copy_within(from..from + len, to);
        self.known.copy_within(from..from + len, to);
    }

    fn word(&self, at: usize) -> Option<u32> {
        let known = self.known.get(at..at.checked_add(4)?)?;
        known
            .iter()
            .all(|&k| k)
            .then(|| u32::from_le_bytes(self.bytes[at..at + 4].try_into().unwrap()))
    }
}

/// A work put in play this room.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Work {
    slot: usize,
    at: usize,
    /// The `Dest` the work started with, when it was known.
    start: Option<[i16; 2]>,
}

/// The model `Sce_em_set` loaded last (`C_id`), which the next work of the same id shares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Model {
    id: u8,
    parts: usize,
}

/// See the module docs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Heap {
    /// Copied on write: a game state's clone shares it.
    image: Arc<Image>,
    /// `Mem_top`'s offset in `Work_buffer`, while it's known.
    top: Option<usize>,
    model: Option<Model>,
    /// `C_kind`, as `Em_kind_search` gives it.
    kind: u8,
    /// `Bgm`: the music word `Snd_bgm_set` last set, when it's known.
    bgm: Option<u16>,
    /// The room's works, in the order they were put in play.
    works: Vec<Work>,
    /// A work put in play by the load's scripts has an init that allocates after it, at its first
    /// move: `Mem_top` stops being known once the load ends.
    pending_init: bool,
    /// The RDT's sprites (`rdt[7]`), for [`Heap::after_load`].
    sprites: u8,
    /// The RDT's RBJ section (`rdt + 0x60`) and section 2, for [`Heap::rbj_reset`].
    rbj: Option<(usize, usize)>,
    /// The image as the load found it, so that a load whose init runs again (a setup script's flags)
    /// starts from it again.
    before: Option<(Arc<Image>, Option<u16>)>,
}

impl Heap {
    /// Nothing known: the new game's first room, a warp, a load.
    pub fn forget(&mut self) {
        *self = Self::default();
    }

    /// `Set_room`'s writes before the room's scripts: the music banks `Snd_bgm_set` loads for the
    /// room's music word (`bgm`), the RDT, and `Mem_top` at its section 2. `again` is a load whose
    /// init runs a second time, which starts from the image the first found.
    pub fn enter_room(&mut self, files: &Files, code: &str, bgm: u16, again: bool) {
        if again && let Some((image, word)) = self.before.clone() {
            self.image = image;
            self.bgm = word;
        }
        self.before = Some((Arc::clone(&self.image), self.bgm));
        self.works.clear();
        self.pending_init = false;
        self.model = None;
        self.kind = 0xFF;
        self.top = None;
        self.rbj = None;
        self.load_bgm(files, bgm);
        let Some(rdt) = files.read(&rdt_entry(code)) else {
            // Without the room's bytes nothing after them is known.
            self.image = Default::default();
            return;
        };
        self.sprites = rdt.get(7).copied().unwrap_or(0);
        let image = Arc::make_mut(&mut self.image);
        image.write(0, &rdt);
        relocate_rdt(image, &rdt);
        let section = |slot: usize| {
            let at = 8 + slot * 4;
            rdt.get(at..at + 4)
                .map(|word| u32::from_le_bytes(word.try_into().unwrap()) as usize)
        };
        let top = section(2).filter(|&vb| vb != 0).unwrap_or(rdt.len());
        self.rbj = section(22)
            .filter(|&rbj| rbj != 0 && rbj < top)
            .map(|rbj| (rbj, top));
        self.top = Some(top).filter(|&top| top < CAP);
    }

    /// `rbj_reset` (`Replace_rdt`) compacts the RDT's RBJ section, moves sections 0 and 1 down after
    /// it, and sets `Mem_top` short of section 2. That isn't followed: the bytes it moves are
    /// unknown, and so is `Mem_top` for the rest of the room.
    pub fn rbj_reset(&mut self) {
        if let Some((rbj, top)) = self.rbj {
            Arc::make_mut(&mut self.image).forget(rbj, top - rbj);
        }
        self.top = None;
    }

    /// `Sce_em_set2` allocates at `Mem_top` as `Sce_em_set` does, which isn't followed: `Mem_top`
    /// is unknown for the rest of the room.
    pub fn em_set2(&mut self) {
        self.top = None;
    }

    /// `Snd_bgm_set`: `Snd_bgm_main` loads the main bank when its id changed, and `Snd_bgm_sub` the
    /// sub bank when its id changed or the main was loaded, both at [`BGM_AT`]; an id of 0xFF loads
    /// nothing. (`Snd_bgm_set` returns early under `fg_system`'s 0x2000, the attract demos, which
    /// don't play rooms here. A new game's `Snd_sys_init_sub2` sets `Bgm` to 0xFFFF, so its first room
    /// loads both banks; arklay leaves that room's banks unknown, which changes no run's `Dest`.)
    fn load_bgm(&mut self, files: &Files, word: u16) {
        let [main, sub] = word.to_le_bytes();
        let last = self.bgm.map(u16::to_le_bytes);
        self.bgm = Some(word);
        let image = Arc::make_mut(&mut self.image);
        let load = |sub_bank: bool, id: u8, image: &mut Image| {
            if id == 0xFF {
                return;
            }
            match files.read(&bgm_entry(sub_bank, id & 0x3F)) {
                Some(bytes) => image.write(BGM_AT, &bytes),
                None => image.forget(BGM_AT, usize::MAX / 2),
            }
        };
        let Some([last_main, last_sub]) = last else {
            // Whether the banks load depends on the music before: what they'd write isn't known.
            image.forget(BGM_AT, usize::MAX / 2);
            return;
        };
        let main_loaded = main & 0x3F != last_main & 0x3F;
        if main_loaded {
            load(false, main, image);
        }
        if main_loaded || sub & 0x3F != last_sub & 0x3F {
            load(true, sub, image);
        }
    }

    /// `Sce_em_set` puts enemy `id` (model type `model_type`, as the script gives it) in `slot`: its
    /// work at `Mem_top`, its model after it unless the last `Sce_em_set` loaded the same id, and its
    /// parts. Returns the `Dest` the work holds, when it's known. `disc` is the scenario's (0 Leon,
    /// 1 Claire), whose folder the files come from; `mirror` is `fg_status`'s 0x10000, whose
    /// `Mirror_model_cp` isn't modelled; `loading` is a set by the load's scripts.
    #[allow(clippy::too_many_arguments)]
    pub fn em_set(
        &mut self,
        files: &Files,
        slot: usize,
        id: u8,
        model_type: u8,
        disc: u8,
        mirror: bool,
        loading: bool,
    ) -> Option<[i16; 2]> {
        let at = self.top?;
        let image = Arc::make_mut(&mut self.image);
        let start = image.word(at + DEST).map(|word| {
            let [a, b, c, d] = word.to_le_bytes();
            [i16::from_le_bytes([a, b]), i16::from_le_bytes([c, d])]
        });
        self.works.push(Work { slot, at, start });
        image.forget(at, WORK_SIZE);
        if (0x50..=0x5A).contains(&id) {
            // `Emd_load` picks these models' files by the zapping flags.
            self.top = None;
            return start;
        }
        let buffer = at + WORK_SIZE;
        let parts = match self.model.filter(|model| model.id == id) {
            Some(model) => {
                self.top = Some(buffer);
                model.parts
            }
            None => {
                // `Em_kind_search` only ever finds id 0x10: its loop stops at the first entry whose
                // next id isn't 0xFF. A kind that differs from the last clears the type's 0x80.
                let kind = if id == 0x10 { 0x10 } else { 0x80 };
                let model_type = if kind != self.kind {
                    model_type & 0x7F
                } else {
                    model_type
                };
                self.kind = kind;
                match load_model(image, files, buffer, id, model_type, disc) {
                    Some((end, parts)) => {
                        self.model = Some(Model { id, parts });
                        self.top = Some(end);
                        parts
                    }
                    None => {
                        self.model = None;
                        self.top = None;
                        return start;
                    }
                }
            }
        };
        let Some(top) = self.top.filter(|&top| top < CAP) else {
            self.top = None;
            return start;
        };
        image.forget(top, parts * PART_SIZE);
        for part in 0..parts {
            image.write(
                top + part * PART_SIZE + PART_POLY_RGB,
                &0x80_8080u32.to_le_bytes(),
            );
        }
        self.top = Some(top + parts * PART_SIZE).filter(|&top| top < CAP);
        // A mirror's copy of the model, and an init that allocates more after its work at its first
        // move (Mr. X's and the G's joints, Ada's, Leon's as a partner), leave `Mem_top` where this
        // doesn't follow it.
        if mirror {
            self.top = None;
        } else if !plain_init(id) {
            if loading {
                self.pending_init = true;
            } else {
                self.top = None;
            }
        }
        start
    }

    /// `Psp_init0` and `Psp_init1`, after the load's scripts: the RDT's `rdt[7]` works of
    /// 0x44 bytes at `Mem_top`.
    pub fn after_load(&mut self) {
        let sprites = usize::from(self.sprites);
        if let Some(top) = self.top {
            Arc::make_mut(&mut self.image).forget(top, sprites * PSP_SIZE);
            self.top = Some(top + sprites * PSP_SIZE).filter(|&top| top < CAP);
        }
        if std::mem::take(&mut self.pending_init) {
            self.top = None;
        }
    }

    /// Something writes `len` bytes the model doesn't follow at `Mem_top` and gives them back
    /// (`Computer200`'s work).
    pub fn scratch(&mut self, len: usize) {
        if let Some(top) = self.top {
            Arc::make_mut(&mut self.image).forget(top, len);
        }
    }

    /// The room ends: each zombie's work keeps the `Dest` it has then. `dest` gives a slot's zombie's
    /// `Dest`, or `None` for a slot that holds no zombie now. A work whose `Dest` was unknown at the
    /// start and is still 0 hasn't been given one, so it stays unknown.
    pub fn leave_room(&mut self, dest: impl Fn(usize) -> Option<[i16; 2]>) {
        self.before = None;
        let works = std::mem::take(&mut self.works);
        let image = Arc::make_mut(&mut self.image);
        for work in works {
            let Some(now) = dest(work.slot) else {
                continue;
            };
            if work.start.is_none() && now == [0, 0] {
                continue;
            }
            let [x, z] = now.map(i16::to_le_bytes);
            image.write(work.at + DEST, &[x[0], x[1], z[0], z[1]]);
        }
    }
}

/// `Emd_load` into `buffer`: the TIM, then the EMD over it, the model type's moves (0x80 moves the
/// TMD down to the first sub-animation's offset, 0x40 moves what follows the sub-animations down
/// over the main ones), and `Mem_top` past the TMD. Returns that end and the parts' count.
fn load_model(
    image: &mut Image,
    files: &Files,
    buffer: usize,
    id: u8,
    model_type: u8,
    disc: u8,
) -> Option<(usize, usize)> {
    match files.read(&tim_entry(id, disc)) {
        Some(tim) => image.write(buffer, &tim),
        None => image.forget(buffer, 0x40000),
    }
    let emd = files.read(&emd_entry(id, disc))?;
    image.write(buffer, &emd);
    let word = |at: usize| {
        emd.get(at..at + 4)
            .map(|word| u32::from_le_bytes(word.try_into().unwrap()) as usize)
    };
    let footer = word(0)?;
    let sections: Vec<usize> = (0..8)
        .map(|k| word(footer + k * 4))
        .collect::<Option<_>>()?;
    let (main_seq, sub0_seq, tmd) = (sections[1], sections[3], sections[7]);
    if tmd > emd.len() || sub0_seq > tmd || main_seq > sub0_seq {
        return None;
    }
    let mut tmd_at = tmd;
    if model_type & 0x80 != 0 {
        image.copy_within(buffer + tmd, emd.len() - tmd, buffer + sub0_seq);
        tmd_at = sub0_seq;
    }
    if model_type & 0x40 != 0 {
        image.copy_within(buffer + sub0_seq, emd.len() - sub0_seq, buffer + main_seq);
        tmd_at -= sub0_seq - main_seq;
    }
    let tmd = buffer + tmd_at;
    let size = image.word(tmd)? as usize;
    let parts = image.word(tmd + 8)? as usize >> 1;
    map_tmd(image, tmd);
    Some((tmd + (size & !3), parts))
}

/// `Mapping_tmd`'s `mapModelingData(tmd + 4)`: the word at `tmd + 4` becomes 1, and each of the
/// `word(tmd + 8)` objects (0x1C bytes from `tmd + 0xC`) has `tmd + 0xC`'s address added to its
/// dwords 0, 2, 4 and 6.
fn map_tmd(image: &mut Image, tmd: usize) {
    let objects = image.word(tmd + 8);
    image.write(tmd + 4, &1u32.to_le_bytes());
    let table = tmd + 0xC;
    let Some(objects) = objects.map(|n| n as usize).filter(|&n| n != 0) else {
        // An unknown count, or 0 (where the original's loop runs on): the table is unknown.
        image.forget(table, CAP);
        return;
    };
    let by = BASE.wrapping_add(table as u32);
    for object in 0..objects.min(CAP / 0x1C) {
        for dword in [0, 2, 4, 6] {
            image.relocate(table + object * 0x1C + dword * 4, by);
        }
    }
}

/// `Set_room`'s relocations: the header's nonzero section offsets, the `+0x1C` word of each of
/// section 7's `rdt[1]` 0x20-byte entries, and both words of each of section 10's `rdt[2]` 8-byte
/// pairs, each made an address in `Work_buffer`.
fn relocate_rdt(image: &mut Image, rdt: &[u8]) {
    let word = |at: usize| {
        rdt.get(at..at + 4)
            .map(|word| u32::from_le_bytes(word.try_into().unwrap()) as usize)
    };
    for at in (8..RDT_HEADER).step_by(4) {
        if word(at).is_some_and(|offset| offset != 0) {
            image.relocate(at, BASE);
        }
    }
    let count = |at: usize| usize::from(rdt.get(at).copied().unwrap_or(0));
    if count(1) != 0
        && let Some(cameras) = word(8 + 7 * 4)
    {
        for entry in 0..count(1) {
            image.relocate(cameras + 0x1C + entry * 0x20, BASE);
        }
    }
    if count(2) != 0
        && let Some(models) = word(8 + 10 * 4)
    {
        for pair in 0..count(2) {
            image.relocate(models + pair * 8, BASE);
            image.relocate(models + pair * 8 + 4, BASE);
        }
    }
}

/// Whether the id's init leaves `Mem_top` as `Sce_em_set` left it: the zombies (`Em10_Init`), the
/// dog (`Em20_Init`), the crow, the licker (`Em22_Init`), `Em_2D` (room 205's ten, which replays
/// show allocate nothing between them), and the people other than Ada (`Spl_in41`) and Leon
/// (`Spl_in5a`). The spider (0x25) calls `Set_enemy_work`, so it isn't one.
fn plain_init(id: u8) -> bool {
    matches!(id, 0x10..=0x22 | 0x2D | 0x40 | 0x42..=0x4F)
}

#[cfg(test)]
mod tests;
