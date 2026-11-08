//! RE 2's files: what the FILE page's reader (`St_menu0_file`, `0x4F95B0`,
//! subs 6 to 0x12) knows of each file, and its work bytes.
//!
//! The files held are `File_tag_bk` (`0x98EE60`,
//! [`crate::scd2::host::Re2State::files`]): three binders of eight slots.

use crate::scd2::host::{FILE_SLOTS, NO_FILE};

/// The files the game has (`file_data`, `0x53E150`).
pub const FILES: usize = 25;

/// `file_data` (`0x53E150`), four bytes a file: the last page's index and
/// the height in rows of the file's picture. These are the English files'
/// counts, which Classic REbirth's `ddraw.dll` holds (file offset
/// `0x2C6828`) in place of the executable's own (the Japanese files'
/// shorter ones): the original's runs read file 1's six pages and file 3's
/// three (`status_file_reader`).
pub const FILE_DATA: [(u16, u16); FILES] = [
    (5, 0x70),
    (5, 0x50),
    (1, 0x50),
    (2, 0x50),
    (9, 0x50),
    (9, 0x50),
    (9, 0x50),
    (6, 0x70),
    (4, 0x70),
    (4, 0x50),
    (1, 0x80),
    (2, 0x90),
    (3, 0x90),
    (2, 0x90),
    (3, 0x70),
    (8, 0x50),
    (6, 0x50),
    (9, 0x50),
    (5, 0x50),
    (1, 0x90),
    (6, 0x50),
    (5, 0x50),
    (4, 0x60),
    (15, 0x50),
    (18, 0x50),
];

/// `st_mv` (`0x53DA1C`): the page slide's steps, indexed by the slide's
/// tick (1 to 11) on the way out and by 11 minus it on the way in.
pub const ST_MV: [u8; 12] = [1, 2, 4, 6, 8, 10, 16, 32, 48, 64, 96, 112];

/// File `file`'s last page (`file_data`'s `+0`). EASY's hint files (0x17
/// and 0x18) grow theirs from flags (`St_init00`), which isn't ported.
pub fn pages_last(file: u8) -> u16 {
    FILE_DATA.get(usize::from(file)).map_or(0, |data| data.0)
}

/// The height of file `file`'s picture in rows (`file_data`'s `+2`).
pub fn height(file: u8) -> u16 {
    FILE_DATA.get(usize::from(file)).map_or(0, |data| data.1)
}

/// The file in `binder`'s slot `slot`, [`NO_FILE`] for none.
pub fn file_at(files: &[u8; FILE_SLOTS], binder: u8, slot: i8) -> u8 {
    let index = usize::from(binder) * 8 + slot.clamp(0, 7) as usize;
    files.get(index).copied().unwrap_or(NO_FILE)
}

/// `move_file_tag` (`0x4FA1D0`): move the slot up (`up`) or down to the
/// binder's next file, wrapping between 0 and 7 over the empty slots. In a
/// binder holding no file the slot stays as it was.
pub fn move_file_tag(files: &[u8; FILE_SLOTS], binder: u8, slot: &mut i8, up: bool) {
    let base = usize::from(binder) * 8;
    let Some(binder_files) = files.get(base..base + 8) else {
        return;
    };
    if binder_files.iter().all(|&file| file == NO_FILE) {
        return;
    }
    loop {
        *slot = if up {
            if *slot <= 0 { 7 } else { *slot - 1 }
        } else if *slot >= 7 {
            0
        } else {
            *slot + 1
        };
        if binder_files[*slot as usize] != NO_FILE {
            return;
        }
    }
}

/// The status screen's blink (`0x691F88`, its way at `0x691F89`): one pair
/// of bytes the item cursor's pulse (`St_disp_cursol0`, `0x4FE5F0`), MAP's
/// marker (`St_menu0_map`'s sub 3) and the file reader's arrows
/// (`St_menu0_file`'s sub 0xD) share, each stepping it its own way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Blink {
    pub level: u8,
    /// The way (`0x691F89`): rising when false.
    pub falling: bool,
}

impl Blink {
    /// `St_init_disp_cursol0` (`0x4FE540`), as each opening of the screen
    /// (`Status`) leaves it.
    pub const OPEN: Self = Self {
        level: 0x32,
        falling: false,
    };
    /// What the reader's subs 0xE and 0xF to 0x12 and MAP's sub 4 set.
    pub const RESET: Self = Self {
        level: 2,
        falling: false,
    };
    /// The level the item cursor holds off the grid (`St_disp_cursol0`).
    pub const HELD: u8 = 0x3C;

    /// One step: rising by `step`, turning once past `high`; falling by
    /// `step`, turning once under `low`. The turn's tick still moves the
    /// old way.
    fn step(&mut self, step: u8, high: u8, low: u8) {
        if self.falling {
            if self.level < low {
                self.falling = false;
            }
            self.level = self.level.wrapping_sub(step);
        } else {
            if self.level > high {
                self.falling = true;
            }
            self.level = self.level.wrapping_add(step);
        }
    }

    /// The item cursor's pulse (`St_disp_cursol0`): by 3, past 0x78, under
    /// 0x32.
    pub fn step_cursor(&mut self) {
        self.step(3, 0x78, 0x32);
    }

    /// MAP's marker (`0x4FA240`'s sub 3): by 2, past 0x50, under 10.
    pub fn step_map(&mut self) {
        self.step(2, 0x50, 10);
    }

    /// The reader's arrows (`0x4F95B0`'s sub 0xD): by 2, past 0x32, under
    /// 10.
    pub fn step_reader(&mut self) {
        self.step(2, 0x32, 10);
    }
}

/// The FILE page's work bytes (`0x691F60`'s block) that persist across the
/// screen's openings, as `Status` leaves them alone, and the draw state the
/// binders' opening keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileWork {
    /// The binder in front (`0x691F71`), 0 to 2.
    pub binder: u8,
    /// The slot in the binder (`0x691F72`), 0 to 7.
    pub slot: i8,
    /// The page of the file being read (`0x691F73`).
    pub page: u8,
    pub blink: Blink,
    /// The binders' zoom (`0x691F8E`).
    pub zoom: i16,
    /// The zoom `ST_Om_trans` (`0x500470`) last moved the eye by (it writes
    /// the eye's height after building the view, so a view lags the zoom
    /// by a tick), and the zoom the last view was built with.
    pub eye_zoom: i16,
    pub view_zoom: i16,
    /// The binders' own turn about y on top of their rest (`Cdir_y` from
    /// 0xC00): `0x500560` adds 0x26 a draw in sub 8 and takes it off in
    /// sub 9.
    pub spin: i16,
    /// The carousel's turn (`0x691F90`).
    pub turn: i16,
    /// The page's x (`0x691FBC`).
    pub x: i16,
}

impl Default for FileWork {
    fn default() -> Self {
        Self {
            binder: 0,
            slot: 0,
            page: 0,
            // The work block is clear until the screen first opens.
            blink: Blink {
                level: 0,
                falling: false,
            },
            zoom: 0,
            eye_zoom: 0,
            view_zoom: 0,
            spin: 0,
            turn: 0,
            x: 0,
        }
    }
}

impl FileWork {
    /// The oracle's `fl` field: the binder, slot, page, the blink's level
    /// and way, the zoom, the carousel's turn and the page's x, then the 24
    /// bytes of `File_tag_bk` in hex.
    pub fn trace(&self, files: &[u8; FILE_SLOTS]) -> String {
        let mut text = format!(
            "{},{},{},{},{},{},{},{}/",
            self.binder,
            self.slot,
            self.page,
            self.blink.level,
            u8::from(self.blink.falling),
            self.zoom,
            self.turn,
            self.x
        );
        for file in files {
            text.push_str(&format!("{file:02x}"));
        }
        text
    }
}
