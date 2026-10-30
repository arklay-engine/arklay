//! RE2 room music: `bio2.exe`'s room music table and its BGM tracks.
//!
//! Every room has one `u16` in the save's `bgm_tbl` (142 rooms: stage `s`'s
//! rooms start at [`STAGE_START`]`[s]`). The low byte picks the main music
//! and the high byte the sub music, each a 6-bit id plus two flag bits; an id
//! of `0x3f` (the `0xff` the scripts write) is none. Nothing in the
//! executable fills the table: the room scripts' `sce_bgmtbl_set` (opcode
//! 0x57) writes it ([`BgmTable::set`]), and `Snd_bgm_set` reads the current
//! room's entry when the room loads.
//!
//! A main id loads up to three tracks at once (`bgm_lut`), played together
//! as BGM channels 0-2; a channel starts silent unless [`MAIN_AUDIBLE`] marks
//! it (or system flag 3 is set) and a script raises it later. A sub id loads
//! up to two (`sbgm_lut`, whose third column the loader never reads). Each
//! track is one streamed `.sap` WAV, `bgm/<track:03x>` in the pack
//! ([`crate::convert_re2::bgm_track`]).
//!
//! The pQES `.BGM` files beside the `.sap`s are the PlayStation sequences:
//! a `pQES` score, a `pBAV` VAB and its sample bodies, with a footer naming
//! the score-plus-header length and the VAB's offset. The PC executable has
//! no sequencer. `Snd_bgm_main` copies the score and VAB header into memory
//! only to size-check the VAB (a main track whose samples would not fit the
//! PlayStation's sound RAM is refused), and `SsSeqSetDecrescendo` is a
//! volume fade timer, so the streamed WAVs are all the music the PC plays and
//! the `.BGM` files are not packed.

/// Rooms in the music table.
pub const ROOMS: usize = 142;

/// The first table entry of each stage, 0-based (`0x53c790` in
/// `bio2 1.10.exe`); stage 7 runs to [`ROOMS`].
pub const STAGE_START: [u8; 7] = [0, 31, 61, 77, 96, 108, 134];

/// The id that means no music.
pub const NONE: u8 = 0x3f;

/// `mainbmg_name_tbl` (`0x522470`): the main track names, indexed by
/// [`MAIN_TRACKS`]; entries 50 and 58 point at a sentinel.
#[rustfmt::skip]
pub const MAIN_NAMES: [Option<&str>; 64] = [
    Some("main00_1"), Some("main01"), Some("main02"), Some("main03"), Some("main04"),
    Some("main05"), Some("main06"), Some("main07"), Some("main08"), Some("main09"),
    Some("main0a"), Some("main00"), Some("main0c"), Some("main0d"), Some("main0e"),
    Some("main0f"), Some("main10"), Some("main11"), Some("main12"), Some("main13"),
    Some("main14"), Some("main15"), Some("main16"), Some("main17"), Some("main18"),
    Some("main19"), Some("main1a"), Some("main1b"), Some("main1c"), Some("main1d"),
    Some("main1e"), Some("main1f"), Some("main20"), Some("main21"), Some("main22"),
    Some("main23"), Some("main24"), Some("main25"), Some("main26"), Some("main27"),
    Some("main28"), Some("main29"), Some("main2a"), Some("main2b"), Some("main2c"),
    Some("main2d"), Some("main2e"), Some("main2f"), Some("main30"), Some("main31"),
    None, Some("main33"), Some("main34"), Some("main35"), Some("main36"),
    Some("main37"), Some("main38"), Some("main39"), None, Some("main00_2"),
    Some("main04_1"), Some("main0f_1"), Some("main14_1"), Some("main1b_1"),
];

/// `subbgm_name_tbl` (`0x522628`): the sub track names, indexed by
/// [`SUB_TRACKS`]; entries 25 and 37 point at a sentinel and 69 is null.
#[rustfmt::skip]
pub const SUB_NAMES: [Option<&str>; 70] = [
    Some("sub00"), Some("sub01"), Some("sub02"), Some("sub03"), Some("sub04"),
    Some("sub05"), Some("sub2f_1"), Some("main16"), Some("main04_1"), Some("sub09"),
    Some("sub0a"), Some("sub0b"), Some("sub0c"), Some("sub0d"), Some("sub0e"),
    Some("sub0f"), Some("sub10"), Some("sub11"), Some("sub12"), Some("sub13"),
    Some("sub14"), Some("sub15"), Some("sub16"), Some("sub17"), Some("sub18"),
    None, Some("sub1a"), Some("sub1b"), Some("sub1c"), Some("main0f"),
    Some("sub1e"), Some("sub1f"), Some("sub20"), Some("sub21"), Some("sub22"),
    Some("sub23"), Some("sub24"), None, Some("sub26"), Some("sub27"),
    Some("sub28"), Some("sub29"), Some("sub2a"), Some("main14"), Some("main1b"),
    Some("sub2d"), Some("sub2e"), Some("sub2f"), Some("sub30"), Some("main0a"),
    Some("sub32"), Some("sub33"), Some("sub34"), Some("sub35"), Some("sub36"),
    Some("sub37"), Some("sub36"), Some("sub39"), Some("sub3a"), Some("sub3b"),
    Some("main12"), Some("sub20"), Some("sub3e"), Some("sub3f"), Some("sub40"),
    Some("sub41"), Some("sub42"), Some("sub43"), Some("sub44"), None,
];

/// `bgm_lut` (`0x522570`): each main id's three channels, as
/// [`MAIN_NAMES`] indices or -1.
#[rustfmt::skip]
pub const MAIN_TRACKS: [[i8; 3]; 61] = [
    [0, 11, 59], [1, -1, -1], [2, -1, -1], [3, -1, -1], [4, 60, -1], [5, -1, -1],
    [6, -1, -1], [7, -1, -1], [8, -1, -1], [-1, -1, -1], [10, -1, -1], [0, 59, -1],
    [12, -1, -1], [13, -1, -1], [14, -1, -1], [15, 61, -1], [16, -1, -1], [17, -1, -1],
    [18, -1, -1], [19, -1, -1], [20, 62, -1], [21, -1, -1], [22, -1, -1], [23, -1, -1],
    [24, -1, -1], [25, -1, -1], [26, -1, -1], [27, -1, -1], [28, -1, -1], [29, -1, -1],
    [30, -1, -1], [31, -1, -1], [32, -1, -1], [33, -1, -1], [34, -1, -1], [35, -1, -1],
    [36, -1, -1], [37, -1, -1], [38, -1, -1], [39, -1, -1], [40, -1, -1], [41, -1, -1],
    [42, -1, -1], [43, -1, -1], [44, -1, -1], [45, -1, -1], [46, -1, -1], [47, -1, -1],
    [48, -1, -1], [49, -1, -1], [50, -1, -1], [51, -1, -1], [52, -1, -1], [53, -1, -1],
    [54, -1, -1], [55, -1, -1], [56, -1, -1], [57, -1, -1], [58, -1, -1], [0, 0, 0],
    [0, 0, 0],
];

/// `sbgm_lut` (`0x522740`): each sub id's channels, as [`SUB_NAMES`]
/// indices or -1. The loader reads only the first two columns.
#[rustfmt::skip]
pub const SUB_TRACKS: [[i8; 3]; 56] = [
    [0, -1, -1], [1, -1, -1], [2, -1, -1], [3, -1, -1], [4, -1, -1], [5, -1, -1],
    [6, -1, -1], [7, -1, -1], [8, 9, -1], [10, 11, -1], [10, 12, -1], [13, -1, -1],
    [14, -1, -1], [15, -1, -1], [16, -1, -1], [17, 18, -1], [19, -1, -1], [20, 21, -1],
    [22, 23, -1], [20, 24, -1], [26, -1, -1], [27, -1, -1], [32, 28, 29], [35, 43, -1],
    [30, 31, -1], [32, 33, -1], [30, 34, -1], [29, -1, -1], [36, -1, -1], [37, -1, -1],
    [38, -1, -1], [40, -1, -1], [39, 46, -1], [41, -1, -1], [42, -1, -1], [44, -1, -1],
    [45, -1, -1], [47, 48, 6], [49, -1, -1], [50, -1, -1], [50, 51, -1], [50, 52, -1],
    [56, 53, -1], [54, -1, -1], [62, -1, -1], [57, -1, -1], [63, -1, -1], [59, 55, -1],
    [60, -1, -1], [58, -1, -1], [20, 64, -1], [65, 66, -1], [67, -1, -1], [68, -1, -1],
    [37, 11, -1], [0, 0, 0],
];

/// The sub channels the loader fills.
pub const SUB_CHANNELS: usize = 2;

/// `0x53c5d8`: whether each main id's channel starts at the BGM volume;
/// a clear channel starts silent unless system flag 3 is set.
#[rustfmt::skip]
pub const MAIN_AUDIBLE: [[bool; 3]; 61] = {
    const F: [bool; 3] = [false; 3];
    const T: [bool; 3] = [true, false, false];
    [
        F, F, T, T, F, T, F, T, T, F, F, F, T, F, T, F, F, T, F, T,
        F, F, F, T, F, F, F, F, T, F, F, F, F, F, T, T, T, T, F, F,
        F, T, F, F, F, F, F, T, T, T, T, T, T, T, T, T, T, T, T, F,
        F,
    ]
};

/// One room's music: the main and sub ids with their flag bits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoomMusic {
    /// The main byte: id in bits 0-5, flags in 6-7.
    pub main: u8,
    /// The sub byte: id in bits 0-5, flags in 6-7.
    pub sub: u8,
}

impl RoomMusic {
    /// The table word: main in the low byte.
    pub fn from_word(word: u16) -> Self {
        Self {
            main: word as u8,
            sub: (word >> 8) as u8,
        }
    }

    /// The main id, unless it is [`NONE`].
    pub fn main_id(self) -> Option<u8> {
        Some(self.main & 0x3f).filter(|&id| id != NONE)
    }

    /// The sub id, unless it is [`NONE`].
    pub fn sub_id(self) -> Option<u8> {
        Some(self.sub & 0x3f).filter(|&id| id != NONE)
    }
}

/// The room music table (`bgm_tbl`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BgmTable {
    rooms: [u16; ROOMS],
}

impl Default for BgmTable {
    fn default() -> Self {
        Self { rooms: [0; ROOMS] }
    }
}

/// The table index of `stage` (0-based) and `room`, if the table has one.
pub fn index(stage: u8, room: u8) -> Option<usize> {
    let start = usize::from(*STAGE_START.get(usize::from(stage))?);
    let end = STAGE_START
        .get(usize::from(stage) + 1)
        .map_or(ROOMS, |&next| usize::from(next));
    let index = start + usize::from(room);
    (index < end).then_some(index)
}

impl BgmTable {
    /// The table's raw words, for a save.
    pub fn words(&self) -> &[u16; ROOMS] {
        &self.rooms
    }

    pub fn words_mut(&mut self) -> &mut [u16; ROOMS] {
        &mut self.rooms
    }

    /// The music of `stage` (0-based) and `room`.
    pub fn get(&self, stage: u8, room: u8) -> Option<RoomMusic> {
        Some(RoomMusic::from_word(self.rooms[index(stage, room)?]))
    }

    /// Apply one `sce_bgmtbl_set`: `stage` is 1-based as the opcode
    /// carries it, and the word is the opcode's two `u16` operands or'd
    /// together (`Sce_bgmtbl_set`). A room outside the table is ignored.
    pub fn set(&mut self, room: u8, stage: u8, low: u16, high: u16) {
        if let Some(index) = stage.checked_sub(1).and_then(|stage| index(stage, room)) {
            self.rooms[index] = low | high;
        }
    }
}

/// The pack stem of track name `name` (`bgm/<track:03x>`).
pub fn track_stem(name: &str) -> Option<String> {
    crate::convert_re2::bgm_track(name).map(|track| format!("bgm/{track:03x}"))
}

/// The track names main id `id` plays on channels 0-2.
pub fn main_tracks(id: u8) -> [Option<&'static str>; 3] {
    let row = MAIN_TRACKS.get(usize::from(id)).copied().unwrap_or([-1; 3]);
    row.map(|name| {
        MAIN_NAMES
            .get(usize::try_from(name).ok()?)
            .copied()
            .flatten()
    })
}

/// The track names sub id `id` plays on its two channels.
pub fn sub_tracks(id: u8) -> [Option<&'static str>; SUB_CHANNELS] {
    let row = SUB_TRACKS.get(usize::from(id)).copied().unwrap_or([-1; 3]);
    [row[0], row[1]].map(|name| {
        SUB_NAMES
            .get(usize::try_from(name).ok()?)
            .copied()
            .flatten()
    })
}

/// Whether main id `id`'s `channel` starts audible ([`MAIN_AUDIBLE`]).
pub fn main_audible(id: u8, channel: usize) -> bool {
    MAIN_AUDIBLE
        .get(usize::from(id))
        .is_some_and(|row| row.get(channel).copied().unwrap_or(false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_index_their_slice_of_the_table() {
        assert_eq!(index(0, 0), Some(0));
        assert_eq!(index(0, 30), Some(30));
        assert_eq!(index(0, 31), None);
        assert_eq!(index(1, 0), Some(31));
        assert_eq!(index(6, 7), Some(141));
        assert_eq!(index(6, 8), None);
        assert_eq!(index(7, 0), None);
    }

    #[test]
    fn scripts_write_the_table() {
        let mut table = BgmTable::default();
        // Room 108: `sce_bgmtbl_set 0, 8, 1, 65280, 0`.
        table.set(8, 1, 0xff00, 0);
        let music = table.get(0, 8).unwrap();
        assert_eq!(music.main_id(), Some(0));
        assert_eq!(music.sub_id(), None);
        // A flag operand ors into the word.
        table.set(25, 2, 4863, 0xc000);
        let music = table.get(1, 25).unwrap();
        assert_eq!(
            music,
            RoomMusic {
                main: 0xff,
                sub: 0xd2
            }
        );
        assert_eq!(music.main_id(), None);
        assert_eq!(music.sub_id(), Some(0x12));
        // Stage 0 and rooms past a stage are not in the table.
        let before = table.clone();
        table.set(0, 0, 1, 0);
        table.set(8, 7, 1, 0);
        assert_eq!(table, before);
    }

    #[test]
    fn ids_resolve_to_tracks() {
        assert_eq!(
            main_tracks(0),
            [Some("main00_1"), Some("main00"), Some("main00_2")]
        );
        assert_eq!(main_tracks(9), [None; 3]);
        assert_eq!(main_tracks(0x26), [Some("main26"), None, None]);
        assert_eq!(main_tracks(0x32), [None; 3]);
        assert_eq!(main_tracks(NONE), [None; 3]);
        assert_eq!(sub_tracks(0x16), [Some("sub20"), Some("sub1c")]);
        assert_eq!(sub_tracks(0x25), [Some("sub2f"), Some("sub30")]);
        assert_eq!(track_stem("main00_2").as_deref(), Some("bgm/400"));
        assert_eq!(track_stem("sub2f_1").as_deref(), Some("bgm/32f"));
        assert!(main_audible(2, 0));
        assert!(!main_audible(2, 1));
        assert!(!main_audible(0, 0));
        assert!(!main_audible(NONE, 0));
    }
}
