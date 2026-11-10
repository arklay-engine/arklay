//! RE 2's file pick-up: a file taken in a room goes into the file list
//! through the get-item window's file path (`St_init02`'s file case,
//! `0x4FD2C0`, and `St_get_file`, `0x4FDC70`).
//!
//! The window is the item path's (`Sce_Item`, game mode 2), but a file asks
//! no question and takes no inventory slot: on the window's sixth tick the
//! file goes into `File_tag_bk`, the page slides in, the player reads it
//! with the reader's rules, and cancel (or decide on the end marker) files
//! it, which raises message 10. The area, its flag and its object go once
//! that message is down.

use super::files::{self, Blink, FileWork, ST_MV};
use super::{Input, PAGE_X_NEXT, PAGE_X_PREV, Sound};
use crate::scd2::host::{FILE_SLOTS, NO_FILE};

/// `0x691F62` in `St_get_file`: a page shows.
const PAGE: u8 = 0;
/// The end marker after the last page.
const END: u8 = 1;
/// The page slides out to the left, and the next one in.
const NEXT_OUT: u8 = 2;
const NEXT_IN: u8 = 3;
/// The page slides out to the right, and the one before in.
const PREV_OUT: u8 = 4;
const PREV_IN: u8 = 5;
/// Message 10 is up, and the take waits for it.
const CLOSE: u8 = 6;

/// The slide's length in ticks (the 12th changes sub).
const SLIDE_TICKS: u8 = 11;

/// What a tick of `St_get_file` did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// The file is being read.
    Reading,
    /// Sub 6 has just been entered: message 10 is to be raised.
    Filed,
    /// Sub 6 saw the message down: the take happens now.
    Done,
}

/// `St_get_file`'s state: the sub (`0x691F62`) and its count (`0x691F63`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileGet {
    sub: u8,
    count: u8,
    /// The sub the last tick started in: `St_disp_file2` draws the page on
    /// every tick but one that starts in sub 6.
    ran: u8,
}

/// `St_init02`'s file case (steps 1, 2 and 4): the first free byte of
/// `File_tag_bk` takes `file`, the binder, slot and page say where, and the
/// page waits off the right edge. With all 24 slots held `St_file_set`
/// returns 24 and writes nothing, so the binder is 3 and the slot 0, past
/// the list (the original then reads beyond `File_tag_bk`, which isn't
/// modelled).
pub fn open(work: &mut FileWork, files: &mut [u8; FILE_SLOTS], file: u8) -> FileGet {
    let slot = match files.iter().position(|&held| held == NO_FILE) {
        Some(slot) => {
            files[slot] = file;
            slot
        }
        None => {
            crate::warning!(
                "warning: file {file} has no free slot in File_tag_bk; the original reads past it"
            );
            FILE_SLOTS
        }
    };
    work.binder = (slot / 8) as u8;
    work.slot = (slot % 8) as i8;
    work.page = 0;
    work.shown_page = 0;
    work.page_draws = 0;
    work.x = PAGE_X_NEXT;
    FileGet {
        sub: NEXT_IN,
        count: 0,
        ran: NEXT_IN,
    }
}

impl FileGet {
    /// The sub (`0x691F62`).
    pub fn sub(&self) -> u8 {
        self.sub
    }

    /// Whether the page is on the screen's way out: sub 6 draws only the
    /// message.
    pub fn closing(&self) -> bool {
        self.sub == CLOSE
    }

    /// The page's draw this tick (`St_disp_file2`, `0x4FDAC0`, which every
    /// path of `St_get_file` ends in), by the sub the tick left: none on a
    /// tick that started in sub 6, which returns first, so the tick that
    /// enters sub 6 still draws the page.
    pub fn page_draw(
        &self,
        work: &FileWork,
        held: &[u8; FILE_SLOTS],
    ) -> Option<Vec<files::PageSprite>> {
        (self.ran != CLOSE).then(|| {
            let file = files::file_at(held, work.binder, work.slot);
            files::page_draw(work, file, files::PageLit::pickup(self.sub))
        })
    }

    /// One tick of `St_get_file`. `held` is `File_tag_bk`, `message_open`
    /// whether message 10 is still up (`fg_message` bit 7).
    pub fn tick(
        &mut self,
        work: &mut FileWork,
        held: &[u8; FILE_SLOTS],
        input: Input,
        message_open: bool,
        sounds: &mut Vec<Sound>,
    ) -> Step {
        let file = files::file_at(held, work.binder, work.slot);
        self.ran = self.sub;
        match self.sub {
            PAGE => {
                // MAP's marker bounds, not the reader's.
                work.blink.step_map();
                if input.cancel {
                    self.sub = CLOSE;
                    sounds.push(Sound::Cancel);
                    return Step::Filed;
                }
                let back = |get: &mut Self, work: &mut FileWork| {
                    if input.left && work.page > 0 {
                        work.page -= 1;
                        get.sub = PREV_OUT;
                        true
                    } else {
                        false
                    }
                };
                if input.right {
                    if work.pages_last(file) <= u16::from(work.page) {
                        self.sub = END;
                        sounds.push(Sound::Cursor);
                        if back(self, work) {
                            sounds.push(Sound::Page);
                        }
                    } else {
                        work.page += 1;
                        self.sub = NEXT_OUT;
                        sounds.push(Sound::Page);
                    }
                } else if back(self, work) {
                    sounds.push(Sound::Page);
                }
                Step::Reading
            }
            END => {
                if input.left {
                    self.sub = PAGE;
                    work.blink = Blink::RESET;
                }
                if input.confirm || input.cancel {
                    self.sub = CLOSE;
                    sounds.push(Sound::Decide);
                    return Step::Filed;
                }
                Step::Reading
            }
            NEXT_OUT | NEXT_IN => {
                if self.count < SLIDE_TICKS {
                    self.count += 1;
                    let n = usize::from(self.count);
                    let step = if self.sub == NEXT_OUT {
                        ST_MV[n]
                    } else {
                        ST_MV[11 - n]
                    };
                    work.x -= i16::from(step);
                } else {
                    // The next page's chunk loads as the old one leaves.
                    if self.sub == NEXT_OUT {
                        work.x = PAGE_X_NEXT;
                        work.shown_page = work.page;
                        self.sub = NEXT_IN;
                    } else {
                        self.sub = PAGE;
                    }
                    self.count = 0;
                    work.blink = Blink::RESET;
                }
                Step::Reading
            }
            PREV_OUT | PREV_IN => {
                if self.count < SLIDE_TICKS {
                    self.count += 1;
                    let n = usize::from(self.count);
                    let step = if self.sub == PREV_OUT {
                        ST_MV[n]
                    } else {
                        ST_MV[11 - n]
                    };
                    work.x += i16::from(step);
                } else {
                    if self.sub == PREV_OUT {
                        work.x = PAGE_X_PREV;
                        work.shown_page = work.page;
                        self.sub = PREV_IN;
                    } else {
                        self.sub = PAGE;
                    }
                    self.count = 0;
                    work.blink = Blink::RESET;
                }
                Step::Reading
            }
            _ => {
                if message_open {
                    Step::Reading
                } else {
                    Step::Done
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty() -> [u8; FILE_SLOTS] {
        [NO_FILE; FILE_SLOTS]
    }

    fn tick(
        get: &mut FileGet,
        work: &mut FileWork,
        files: &[u8; FILE_SLOTS],
        input: Input,
    ) -> (Step, Vec<Sound>) {
        let mut sounds = Vec::new();
        let step = get.tick(work, files, input, false, &mut sounds);
        (step, sounds)
    }

    /// File 3 (three pages) opened and slid in.
    fn reading() -> (FileGet, FileWork, [u8; FILE_SLOTS]) {
        let mut files = empty();
        let mut work = FileWork::default();
        let mut get = open(&mut work, &mut files, 3);
        for _ in 0..12 {
            tick(&mut get, &mut work, &files, Input::default());
        }
        (get, work, files)
    }

    const RIGHT: Input = Input {
        up: false,
        down: false,
        left: false,
        right: true,
        confirm: false,
        cancel: false,
        start: false,
        held: 0,
    };
    const LEFT: Input = Input {
        left: true,
        right: false,
        ..RIGHT
    };
    const CANCEL: Input = Input {
        right: false,
        cancel: true,
        ..RIGHT
    };
    const DECIDE: Input = Input {
        right: false,
        confirm: true,
        ..RIGHT
    };

    /// Slide to the end of the page's slide, to sub 0 again.
    fn settle(get: &mut FileGet, work: &mut FileWork, files: &[u8; FILE_SLOTS]) {
        for _ in 0..30 {
            if get.sub == PAGE || get.sub == END {
                return;
            }
            tick(get, work, files, Input::default());
        }
        panic!("the slide never ended");
    }

    #[test]
    fn open_files_the_document_in_the_first_free_slot() {
        let mut files = empty();
        let mut work = FileWork::default();
        open(&mut work, &mut files, 3);
        assert_eq!(files[0], 3);
        assert_eq!(
            (work.binder, work.slot, work.page, work.x),
            (0, 0, 0, 0x138)
        );

        open(&mut work, &mut files, 4);
        assert_eq!((files[1], work.binder, work.slot), (4, 0, 1));

        let mut files = empty();
        files[..9].fill(0);
        open(&mut work, &mut files, 9);
        assert_eq!((files[9], work.binder, work.slot), (9, 1, 1));
    }

    #[test]
    fn open_with_every_slot_held_writes_nothing() {
        let mut files = [0; FILE_SLOTS];
        let mut work = FileWork::default();
        open(&mut work, &mut files, 3);
        assert_eq!(files, [0; FILE_SLOTS]);
        assert_eq!((work.binder, work.slot), (3, 0));
    }

    #[test]
    fn the_page_slides_in_then_shows() {
        let mut files = empty();
        let mut work = FileWork::default();
        let mut get = open(&mut work, &mut files, 3);
        let mut x = 0x138;
        for n in 1..=11 {
            tick(&mut get, &mut work, &files, Input::default());
            x -= i16::from(ST_MV[11 - n]);
            assert_eq!(work.x, x);
        }
        assert_eq!(x, 0x19);
        assert_eq!(get.sub(), NEXT_IN);
        tick(&mut get, &mut work, &files, Input::default());
        assert_eq!(get.sub(), PAGE);
        assert_eq!(work.blink, Blink::RESET);
    }

    #[test]
    fn sub_0_pages_and_files() {
        let (mut get, mut work, files) = reading();
        let (_, sounds) = tick(&mut get, &mut work, &files, RIGHT);
        assert_eq!(
            (get.sub(), work.page, sounds),
            (NEXT_OUT, 1, vec![Sound::Page])
        );
        settle(&mut get, &mut work, &files);
        tick(&mut get, &mut work, &files, RIGHT);
        settle(&mut get, &mut work, &files);
        assert_eq!(work.page, 2);
        let (_, sounds) = tick(&mut get, &mut work, &files, RIGHT);
        assert_eq!((get.sub(), sounds), (END, vec![Sound::Cursor]));

        // Right and left together on the last page.
        let mut both = RIGHT;
        both.left = true;
        let (mut get, mut work, files) = {
            let (mut get, mut work, files) = reading();
            work.page = 2;
            get.sub = PAGE;
            (get, work, files)
        };
        let (_, sounds) = tick(&mut get, &mut work, &files, both);
        assert_eq!(
            (get.sub(), work.page, sounds),
            (PREV_OUT, 1, vec![Sound::Cursor, Sound::Page])
        );

        // Left on page 0 does nothing.
        let (mut get, mut work, files) = reading();
        let (step, sounds) = tick(&mut get, &mut work, &files, LEFT);
        assert_eq!((step, get.sub(), sounds), (Step::Reading, PAGE, vec![]));

        let (step, sounds) = tick(&mut get, &mut work, &files, CANCEL);
        assert_eq!(
            (step, get.sub(), sounds),
            (Step::Filed, CLOSE, vec![Sound::Cancel])
        );
    }

    #[test]
    fn the_end_marker_files_on_decide_or_cancel() {
        let (mut get, mut work, files) = reading();
        get.sub = END;
        let (step, sounds) = tick(&mut get, &mut work, &files, DECIDE);
        assert_eq!(
            (step, get.sub(), sounds),
            (Step::Filed, CLOSE, vec![Sound::Decide])
        );

        get.sub = END;
        let (step, _) = tick(&mut get, &mut work, &files, CANCEL);
        assert_eq!((step, get.sub()), (Step::Filed, CLOSE));

        get.sub = END;
        work.blink.level = 0x40;
        tick(&mut get, &mut work, &files, LEFT);
        assert_eq!((get.sub(), work.blink), (PAGE, Blink::RESET));

        get.sub = END;
        let mut both = DECIDE;
        both.left = true;
        let (step, _) = tick(&mut get, &mut work, &files, both);
        assert_eq!((step, get.sub()), (Step::Filed, CLOSE));
    }

    #[test]
    fn the_pick_up_blink_uses_maps_bounds() {
        let (mut get, mut work, files) = reading();
        work.blink = Blink {
            level: 0x4E,
            falling: false,
        };
        let mut seen = Vec::new();
        for _ in 0..4 {
            tick(&mut get, &mut work, &files, Input::default());
            seen.push((work.blink.level, work.blink.falling));
        }
        // The reader's bounds would have turned at 0x32; MAP's turn past 0x50,
        // and the turning tick still moves the old way.
        assert_eq!(
            seen,
            [(0x50, false), (0x52, false), (0x54, true), (0x52, true)]
        );
    }

    #[test]
    fn sub_6_waits_for_the_message() {
        let (mut get, mut work, files) = reading();
        tick(&mut get, &mut work, &files, CANCEL);
        let mut sounds = Vec::new();
        assert_eq!(
            get.tick(&mut work, &files, Input::default(), true, &mut sounds),
            Step::Reading
        );
        assert_eq!(
            get.tick(&mut work, &files, Input::default(), false, &mut sounds),
            Step::Done
        );
    }

    #[test]
    fn a_turn_shows_the_old_page_until_its_slide_out_ends() {
        let (mut get, mut work, files) = reading();
        assert_eq!(work.shown_page, 0);
        tick(&mut get, &mut work, &files, RIGHT);
        assert_eq!((work.page, work.shown_page), (1, 0));
        for _ in 0..11 {
            tick(&mut get, &mut work, &files, Input::default());
            assert_eq!(work.shown_page, 0);
        }
        tick(&mut get, &mut work, &files, Input::default());
        assert_eq!((get.sub(), work.shown_page), (NEXT_IN, 1));
        settle(&mut get, &mut work, &files);
        tick(&mut get, &mut work, &files, LEFT);
        assert_eq!((work.page, work.shown_page), (0, 1));
        for _ in 0..12 {
            tick(&mut get, &mut work, &files, Input::default());
        }
        assert_eq!((get.sub(), work.shown_page), (PREV_IN, 0));
    }

    fn textures(sprites: &[files::PageSprite]) -> Vec<files::PageTexture> {
        sprites.iter().map(|sprite| sprite.texture).collect()
    }

    #[test]
    fn the_pick_up_draws_its_page_until_a_tick_starts_in_sub_6() {
        use files::PageTexture::{Chunk, Sheet};
        let (mut get, mut work, files) = reading();
        // Sub 0 on page 0: the right arrow, no left arrow.
        let sprites = get.page_draw(&work, &files).unwrap();
        assert_eq!(
            textures(&sprites),
            [Chunk(1), Chunk(2), Sheet(files::SHEET_ARROWS)]
        );
        // The last page in sub 0: the marker, dim; in sub 1, lit.
        work.page = 2;
        work.shown_page = 2;
        let sprites = get.page_draw(&work, &files).unwrap();
        assert_eq!(sprites[2].texture, Sheet(files::SHEET_MARKER));
        assert_eq!(sprites[2].colour, 0x30);
        assert_eq!(sprites[3].x, 12);
        tick(&mut get, &mut work, &files, RIGHT);
        assert_eq!(get.sub(), END);
        let sprites = get.page_draw(&work, &files).unwrap();
        assert_eq!(
            textures(&sprites),
            [Chunk(1), Chunk(4), Sheet(files::SHEET_MARKER)]
        );
        assert_eq!(sprites[2].colour, 0x80);
        // The decide that enters sub 6 still draws the page, with no marker.
        tick(&mut get, &mut work, &files, DECIDE);
        assert!(get.closing());
        let sprites = get.page_draw(&work, &files).unwrap();
        assert_eq!(textures(&sprites), [Chunk(1), Chunk(4)]);
        // The next tick starts in sub 6 and draws nothing.
        let mut sounds = Vec::new();
        get.tick(&mut work, &files, Input::default(), true, &mut sounds);
        assert_eq!(get.page_draw(&work, &files), None);
    }

    #[test]
    fn the_pick_ups_page_waits_off_screen_while_it_fades_in() {
        let mut files = empty();
        let mut work = FileWork::default();
        open(&mut work, &mut files, 3);
        let file = files::file_at(&files, work.binder, work.slot);
        let sprites = files::page_draw(&work, file, files::PageLit::reader(0));
        assert_eq!(sprites.len(), 2);
        assert_eq!((sprites[1].x, sprites[1].y), (312, 30));
    }
}
