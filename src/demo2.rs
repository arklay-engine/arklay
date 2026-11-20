//! RE 2's attract-demo reels (`demo/<hex>.dat`, the original's
//! `PDEMO<disc><n>.DAT`).
//!
//! A reel is the block the title reads into `Pdemo` (`0x991FC8`) when its
//! idle count runs out: the start `Title_game_init`'s demo branch
//! (`0x506750`) gives the game in place of a new game's, and the raw pad
//! words `Pad_set` (`0x4D0F30`) plays, one a tick of play, in place of the
//! pad. The title picks the reel (`games/re2/ui/title.lua`): the converter
//! packs `PDEMO<disc><n>.DAT` as `demo/<n * 2 + disc>.dat`.
//!
//! | offset | |
//! |---|---|
//! | 0, 1, 2 | the stage (from 0), the room and the cut |
//! | 3, 4 | the equipped item and its slot |
//! | 5 | the key config the words were recorded under |
//! | 6 | the player |
//! | 8 | `Item_work`: eleven slots of id, count, mark and a spare byte |
//! | 0x34 | the count of pad words |
//! | 0x36 | the player's x, y and z, and the angle |
//! | 0x3E | the pad words |

use anyhow::{Result, bail};

/// The bytes before the pad words.
pub const HEADER_LEN: usize = 0x3E;
/// `Item_work`'s slots: ten and the personal item's.
pub const ITEM_SLOTS: usize = 11;

/// One attract-demo reel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reel {
    /// The pack entry the reel was read from.
    pub entry: String,
    /// The stage, from 0 (`scd_var_stage`).
    pub stage: u8,
    pub room: u8,
    pub cut: u8,
    /// The equipped item (`0x98E9B4`).
    pub equipped: u8,
    /// The equipped item's slot (`0x98E9B5`).
    pub equipped_slot: u8,
    /// The key config the pad words go through (`0x98E9AA`).
    pub key_type: u8,
    /// The player (`pPl.Id`).
    pub player: u8,
    /// `Item_work`: id, count, mark and a spare byte a slot.
    pub items: [[u8; 4]; ITEM_SLOTS],
    pub pos: [i16; 3],
    pub angle: i16,
    /// The raw pad words, one a tick of play.
    pub pads: Vec<u16>,
}

impl Reel {
    /// Parse a reel's file.
    pub fn parse(entry: impl Into<String>, data: &[u8]) -> Result<Self> {
        let entry = entry.into();
        if data.len() < HEADER_LEN {
            bail!(
                "demo reel {entry} is {} bytes; its header is {HEADER_LEN}",
                data.len()
            );
        }
        let word = |at: usize| u16::from_le_bytes([data[at], data[at + 1]]);
        let count = usize::from(word(0x34));
        let Some(words) = data.get(HEADER_LEN..HEADER_LEN + count * 2) else {
            bail!(
                "demo reel {entry} counts {count} pad words; it holds {}",
                (data.len() - HEADER_LEN) / 2
            );
        };
        let mut items = [[0; 4]; ITEM_SLOTS];
        for (slot, bytes) in items.iter_mut().zip(data[8..0x34].as_chunks::<4>().0) {
            *slot = *bytes;
        }
        Ok(Self {
            entry,
            stage: data[0],
            room: data[1],
            cut: data[2],
            equipped: data[3],
            equipped_slot: data[4],
            key_type: data[5],
            player: data[6],
            items,
            pos: [word(0x36) as i16, word(0x38) as i16, word(0x3A) as i16],
            angle: word(0x3C) as i16,
            pads: words
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_le_bytes(*pair))
                .collect(),
        })
    }
}

/// A reel's play: `Pad_set`'s demo branch and what `Game_loop` reads of it.
#[derive(Debug, Clone)]
pub struct Playback {
    pub reel: Reel,
    /// The next word (`pG + 0x804A`): past the count once the play is over.
    next: usize,
    /// A key ended the play before its last word (`0x98F1BB`).
    aborted: bool,
}

impl Playback {
    pub fn new(reel: Reel) -> Self {
        Self {
            reel,
            next: 0,
            aborted: false,
        }
    }

    /// One tick of `Pad_set`: the raw pad the game reads, or `None` where it
    /// stays the pad's own. `key` is a new press of any button but a
    /// direction (the raw pad's low twelve bits), and `playing` is
    /// `fg_system`'s `0x40000000`, up from the game's first tick.
    ///
    /// A word is read on every tick of play; a key, or the tick after the
    /// last word, takes the count past its end, and the pad is 0 from then
    /// on. Before play the pad isn't replaced and a key does nothing.
    pub fn pad(&mut self, key: bool, playing: bool) -> Option<u16> {
        let count = self.reel.pads.len();
        if !playing {
            return None;
        }
        if key || self.next >= count {
            self.aborted |= self.next < count;
            self.next = count + 1;
            return Some(0);
        }
        let word = self.reel.pads[self.next];
        self.next += 1;
        Some(word)
    }

    /// The play is over (`Game_loop`'s test: the count is under the next
    /// word's index).
    pub fn over(&self) -> bool {
        self.next > self.reel.pads.len()
    }

    /// A key ended it.
    pub fn aborted(&self) -> bool {
        self.aborted
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A reel in room 203 as Claire, with `pads` as its words.
    pub(crate) fn reel_bytes(pads: &[u16]) -> Vec<u8> {
        let mut data = vec![0u8; HEADER_LEN];
        data[..7].copy_from_slice(&[1, 3, 2, 3, 0, 0, 1]);
        data[8..12].copy_from_slice(&[0x03, 13, 0, 0]);
        data[12..16].copy_from_slice(&[0x01, 1, 0, 0]);
        data[8 + 40..8 + 44].copy_from_slice(&[0x30, 1, 0, 0]);
        data[0x34..0x36].copy_from_slice(&(pads.len() as u16).to_le_bytes());
        data[0x36..0x38].copy_from_slice(&(-25967i16).to_le_bytes());
        data[0x38..0x3A].copy_from_slice(&(-1800i16).to_le_bytes());
        data[0x3A..0x3C].copy_from_slice(&(-21525i16).to_le_bytes());
        data[0x3C..0x3E].copy_from_slice(&3784i16.to_le_bytes());
        for word in pads {
            data.extend_from_slice(&word.to_le_bytes());
        }
        // The files are a fixed size: what follows the count isn't played.
        data.extend_from_slice(&0x1000u16.to_le_bytes());
        data
    }

    #[test]
    fn a_reel_parses_its_start_and_its_counted_words() {
        let reel = Reel::parse("demo/01.dat", &reel_bytes(&[0x1000, 0x1040, 0x0088])).unwrap();
        assert_eq!((reel.stage, reel.room, reel.cut), (1, 3, 2));
        assert_eq!((reel.equipped, reel.equipped_slot), (3, 0));
        assert_eq!((reel.key_type, reel.player), (0, 1));
        assert_eq!(reel.items[0], [0x03, 13, 0, 0]);
        assert_eq!(reel.items[1], [0x01, 1, 0, 0]);
        assert_eq!(reel.items[10], [0x30, 1, 0, 0]);
        assert_eq!(reel.pos, [-25967, -1800, -21525]);
        assert_eq!(reel.angle, 3784);
        assert_eq!(reel.pads, [0x1000, 0x1040, 0x0088]);
    }

    #[test]
    fn a_short_reel_is_refused() {
        let mut data = reel_bytes(&[1, 2, 3]);
        assert!(Reel::parse("demo/00.dat", &data[..HEADER_LEN - 1]).is_err());
        data.truncate(HEADER_LEN + 5);
        assert!(Reel::parse("demo/00.dat", &data).is_err());
    }

    fn playback(pads: &[u16]) -> Playback {
        Playback::new(Reel::parse("demo/00.dat", &reel_bytes(pads)).unwrap())
    }

    #[test]
    fn the_words_play_a_tick_each_and_end_the_tick_after_the_last() {
        let mut play = playback(&[0x1000, 0x0088]);
        // Before play nothing is read, whatever is pressed.
        assert_eq!(play.pad(false, false), None);
        assert_eq!(play.pad(true, false), None);
        assert_eq!(play.pad(false, true), Some(0x1000));
        assert_eq!(play.pad(false, true), Some(0x0088));
        assert!(!play.over());
        assert_eq!(play.pad(false, true), Some(0));
        assert!(play.over() && !play.aborted());
        assert_eq!(play.pad(true, true), Some(0));
        assert!(!play.aborted(), "a key after the end changes nothing");
    }

    #[test]
    fn a_key_ends_the_play_at_once() {
        let mut play = playback(&[0x1000, 0x0088, 0x1000]);
        assert_eq!(play.pad(false, true), Some(0x1000));
        assert_eq!(play.pad(true, true), Some(0));
        assert!(play.over() && play.aborted());
        assert_eq!(play.pad(false, true), Some(0));
    }
}
