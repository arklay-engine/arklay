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
    /// The page whose chunk texture 0x1D holds: `page` as it was on the
    /// file's open or on a turn's last slide tick, where the original
    /// reloads it. A turn moves `page` on its press tick, so the page that
    /// slides out is still the old one.
    pub shown_page: u8,
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
            shown_page: 0,
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

/// The page's y (`0x691FBE`).
pub const PAGE_Y: i16 = 0x1E;
/// Where the file's picture is drawn.
pub const PICTURE_X: i16 = 100;
pub const PICTURE_Y: i16 = 0x3C;
/// The arrows' and the end marker's y.
const ARROW_Y: i16 = 0x6E;
/// The status sheets the arrows (texture 0x14) and the end marker (0x19)
/// are on, counted from the first (0x12).
pub const SHEET_ARROWS: u8 = 2;
pub const SHEET_MARKER: u8 = 7;
/// `AddSprt`'s unmodulated colour.
pub const BRIGHT: u8 = 0x80;

/// What a page sprite is drawn from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageTexture {
    /// A chunk of the file (`ui/status/file<nn>_<k>.tim`): 1 the picture, `2
    /// + page` a page.
    Chunk(u8),
    /// A status sheet (`st<n>_jp_<k>`), counted from texture 0x12.
    Sheet(u8),
}

/// One sprite of a page's draw, at 320 × 240.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageSprite {
    pub texture: PageTexture,
    pub u: u16,
    pub v: u16,
    pub w: u16,
    pub h: u16,
    pub x: i16,
    pub y: i16,
    /// `AddSprt`'s colour: 0x80 draws the texels as they are, less dims
    /// them (`rgb / 0x80`).
    pub colour: u8,
}

/// Which of the page's sprites over the page show: the right arrow or the
/// end marker (on the last page), whether the marker is lit, and the left
/// arrow (from page 1). `dim_way` is the reader's rule that a right arrow
/// outside sub 0xD dims on the blink's rising way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PageLit {
    pub right: bool,
    pub lit: bool,
    pub left: bool,
    pub dim_way: bool,
}

impl PageLit {
    /// The reader's `St_disp_file` (`0x500270`) by the sub its tick left:
    /// the right arrow or the marker in 0xD and 0xE, lit in 0xE, the left
    /// arrow in 0xD. `St_init02`'s fade draws with this rule and the get-item
    /// window's sub 0, so with neither.
    pub fn reader(sub: u8) -> Self {
        Self {
            right: matches!(sub, super::FILE_PAGE | super::FILE_END),
            lit: sub == super::FILE_END,
            left: sub == super::FILE_PAGE,
            dim_way: sub != super::FILE_PAGE,
        }
    }

    /// The pick-up's `St_disp_file2` (`0x4FDAC0`) by `St_get_file`'s sub:
    /// the right arrow or the marker in subs 0 and 1, lit in 1, the left
    /// arrow in 0, both arrows always bright.
    pub fn pickup(sub: u8) -> Self {
        Self {
            right: sub < 2,
            lit: sub == 1,
            left: sub == 0,
            dim_way: false,
        }
    }
}

/// A file page's draw (`St_disp_file`, `0x500270`, and `St_disp_file2`,
/// `0x4FDAC0`), in paint order: the picture behind (ordering-table entry
/// 0xF), the page (0xA), then the right arrow or the end marker and the
/// left arrow (6). The page is the chunk of [`FileWork::shown_page`] at the
/// page's x; the picture is the bottom `height` rows of chunk 1. The arrows
/// sit by the blink's way (`0x691F89`); the marker is dim (0x30) unless lit.
///
/// A tall picture (a height of 0x80 or more) is drawn as both functions
/// draw it from their second frame on, chunk 1's 0x80 rows at the
/// picture's place; their first frame on each draw buffer, which adds chunk
/// 0, isn't modelled.
pub fn page_draw(work: &FileWork, file: u8, lit: PageLit) -> Vec<PageSprite> {
    let h = height(file);
    let mut sprites = Vec::with_capacity(4);
    let (v, rows) = if h < 0x80 { (0x80 - h, h) } else { (0, 0x80) };
    sprites.push(PageSprite {
        texture: PageTexture::Chunk(1),
        u: 0,
        v,
        w: 0x80,
        h: rows,
        x: PICTURE_X,
        y: PICTURE_Y,
        colour: BRIGHT,
    });
    sprites.push(PageSprite {
        texture: PageTexture::Chunk(2 + work.shown_page),
        u: 0,
        v: 0,
        w: 0x100,
        h: 0x100u16.saturating_sub(h),
        x: work.x,
        y: PAGE_Y,
        colour: BRIGHT,
    });
    let way = i16::from(work.blink.falling);
    if lit.right {
        let sprite = if u16::from(work.page) == pages_last(file) {
            PageSprite {
                texture: PageTexture::Sheet(SHEET_MARKER),
                u: 0,
                v: 0x20,
                w: 0x2A,
                h: 0xE,
                x: 0x118,
                y: ARROW_Y,
                colour: if lit.lit { BRIGHT } else { 0x30 },
            }
        } else {
            PageSprite {
                texture: PageTexture::Sheet(SHEET_ARROWS),
                u: 0x2A,
                v: 0x18,
                w: 0xC,
                h: 0xD,
                x: (way + 0x5E) * 3,
                y: ARROW_Y,
                colour: if lit.dim_way && way == 0 {
                    0x40
                } else {
                    BRIGHT
                },
            }
        };
        sprites.push(sprite);
    }
    if lit.left && work.page > 0 {
        sprites.push(PageSprite {
            texture: PageTexture::Sheet(SHEET_ARROWS),
            u: 0x1C,
            v: 0x18,
            w: 0xC,
            h: 0xD,
            x: (4 - way) * 3,
            y: ARROW_Y,
            colour: BRIGHT,
        });
    }
    sprites
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::status2::{FILE_END, FILE_NEXT_OUT, FILE_PAGE};

    fn work(page: u8, falling: bool) -> FileWork {
        FileWork {
            page,
            shown_page: page,
            x: 0x19,
            blink: Blink { level: 20, falling },
            ..FileWork::default()
        }
    }

    fn sprite(
        texture: PageTexture,
        uv: (u16, u16),
        wh: (u16, u16),
        xy: (i16, i16),
        colour: u8,
    ) -> PageSprite {
        PageSprite {
            texture,
            u: uv.0,
            v: uv.1,
            w: wh.0,
            h: wh.1,
            x: xy.0,
            y: xy.1,
            colour,
        }
    }

    #[test]
    fn the_readers_first_page_has_its_right_arrow() {
        let sprites = page_draw(&work(0, false), 1, PageLit::reader(FILE_PAGE));
        assert_eq!(
            sprites,
            [
                sprite(PageTexture::Chunk(1), (0, 48), (128, 80), (100, 60), 0x80),
                sprite(PageTexture::Chunk(2), (0, 0), (256, 176), (25, 30), 0x80),
                sprite(
                    PageTexture::Sheet(SHEET_ARROWS),
                    (0x2A, 0x18),
                    (12, 13),
                    (282, 110),
                    0x80
                ),
            ]
        );
    }

    #[test]
    fn the_readers_last_page_has_the_end_marker() {
        // File 1's last page is 5 (Classic REbirth's table).
        let sprites = page_draw(&work(5, false), 1, PageLit::reader(FILE_PAGE));
        let marker = sprite(
            PageTexture::Sheet(SHEET_MARKER),
            (0, 0x20),
            (0x2A, 0xE),
            (280, 110),
            0x30,
        );
        assert_eq!(sprites[2], marker);
        assert_eq!(sprites[3].texture, PageTexture::Sheet(SHEET_ARROWS));
        let sprites = page_draw(&work(5, false), 1, PageLit::reader(FILE_END));
        assert_eq!(
            sprites[2],
            PageSprite {
                colour: 0x80,
                ..marker
            }
        );
        assert_eq!(sprites.len(), 3, "no left arrow in 0xE");
    }

    #[test]
    fn the_arrows_move_with_the_blinks_way() {
        let sprites = page_draw(&work(2, true), 1, PageLit::reader(FILE_PAGE));
        assert_eq!((sprites[2].x, sprites[3].x), (285, 9));
        assert_eq!(sprites[3].u, 0x1C);
    }

    #[test]
    fn a_turn_draws_the_shown_page_only() {
        let mut work = work(1, false);
        work.shown_page = 0;
        let sprites = page_draw(&work, 1, PageLit::reader(FILE_NEXT_OUT));
        assert_eq!(sprites.len(), 2);
        assert_eq!(sprites[1].texture, PageTexture::Chunk(2));
    }
}
