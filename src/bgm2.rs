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

/// The channels `Music` drives: the main's three, then the sub's two.
pub const CHANNELS: usize = 3 + SUB_CHANNELS;
/// The sequences `Seq_ctr` tracks: the main, and each sub channel.
const SEQUENCES: usize = 1 + SUB_CHANNELS;
/// A sequence's volume until a script changes it (`Snd_sys_init_sub2`).
const SEQUENCE_VOLUME: u8 = 0x6e;
/// `Seq_ctr[0]`, a sequence stopped by `sce_bgm_control`.
const STATE_STOPPED: u8 = 2;
/// `Seq_ctr[0]`, a sequence playing.
const STATE_PLAYING: u8 = 1;
/// `Seq_ctr[0]`, what a muted stop leaves.
const STATE_MUTED: u8 = 4;
/// `Seq_ctr[0]`, counting down a room table's fade to its stop at 5.
const STATE_ROOM_FADE: u8 = 0xf;
/// `Seq_ctr[0]`, counting down `sce_bgm_control`'s fade to its stop at 5.
const STATE_SCRIPT_FADE: u8 = 0x32;
/// The ticks `SsSeqSetDecrescendo` lowers a sequence for, a step of 1 a tick.
const FADE_TICKS: u8 = 0x5a;
/// `Seq_ctr[1]`, a sequence with nothing loaded to play.
const HOLD_NOTHING: u8 = 0xff;
/// `fg_system`'s bit that stops the game touching the music.
const SYSTEM_NO_MUSIC: u32 = 0x2000;
/// `fg_system`'s bit that leaves a main's channels at its volume on load.
const SYSTEM_ALL_AUDIBLE: u32 = 8;
/// `fg_system`'s bit that plays a main of stage 2's room 0 through once.
const SYSTEM_MAIN_ONCE: u32 = 0x100_0000;
/// `fg_system`'s bit that plays a sub through once.
const SYSTEM_SUB_ONCE: u32 = 0x20_0000;

/// One channel's wanted state, for the mixer to catch up with.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Channel {
    /// The pack track loaded.
    pub track: Option<u16>,
    /// `track` changed and wants loading.
    pub load: bool,
    /// `SsPlay`, with its loop flag: it starts a stopped channel and leaves a
    /// playing one where it is.
    pub play: Option<bool>,
    /// `SsStopGroup`.
    pub stop: bool,
    /// `SsSetVol`, 0-127.
    pub volume: u8,
    /// `SsSetPan`'s argument.
    pub pan: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Sequence {
    /// `Seq_ctr[0]`.
    state: u8,
    /// `Seq_ctr[1]`: 0 plays on the room's load.
    hold: u8,
    /// `Seq_ctr[2]`: a bank is loaded.
    loaded: bool,
    /// `ss_timer`.
    fade: u8,
    /// The volume a start gives it (`DWORD_00693804` and on).
    volume: u8,
}

/// Where `sce_bgm_control` runs, which some of its cases ask.
#[derive(Debug, Clone, Copy, Default)]
pub struct Place {
    /// `scd_var_stage`, from 0.
    pub stage: u8,
    pub room: u8,
    pub cut: usize,
    /// `fg_system`.
    pub system: u32,
    /// `player_index`.
    pub player: u8,
}

/// The state of RE2's music, the `Seq_ctr` table and what it asks of the
/// channels (`Snd_bgm_set`, `Snd_bgm_play_ck`, `Snd_bgm_ctr` and the fades of
/// `Snd_se_call`). A save doesn't hold it: the load enters the room again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Music {
    /// `Bgm`: the table's word for the room entered last.
    current: RoomMusic,
    sequences: [Sequence; SEQUENCES],
    pub channels: [Channel; CHANNELS],
}

impl Default for Music {
    fn default() -> Self {
        Self {
            current: RoomMusic {
                main: 0xff,
                sub: 0xff,
            },
            sequences: [Sequence {
                state: 0,
                hold: HOLD_NOTHING,
                loaded: false,
                fade: 0,
                volume: SEQUENCE_VOLUME,
            }; SEQUENCES],
            channels: Default::default(),
        }
    }
}

/// The channels of sequence `sequence`.
fn sequence_channels(sequence: usize) -> std::ops::Range<usize> {
    match sequence {
        0 => 0..3,
        n => 2 + n..3 + n,
    }
}

impl Music {
    fn set_volume(&mut self, sequence: usize, volume: u8) {
        for index in sequence_channels(sequence) {
            self.channels[index].volume = volume;
        }
    }

    fn play(&mut self, sequence: usize, looping: bool) {
        for index in sequence_channels(sequence) {
            self.channels[index].play = Some(looping);
        }
    }

    fn stop(&mut self, sequence: usize) {
        for index in sequence_channels(sequence) {
            self.channels[index].stop = true;
            self.channels[index].play = None;
        }
    }

    fn load(&mut self, index: usize, track: Option<&str>, volume: u8) {
        self.channels[index] = Channel {
            track: track.and_then(crate::convert_re2::bgm_track),
            load: true,
            play: None,
            stop: true,
            volume,
            pan: 0,
        };
    }

    /// `Snd_bgm_main`: the room's main replaces what's loaded.
    fn load_main(&mut self, word: u8, system: u32) -> bool {
        let id = word & 0x3f;
        let tracks = if word == 0xff {
            [None; 3]
        } else {
            main_tracks(id)
        };
        let base = self.sequences[0].volume;
        for (index, &track) in tracks.iter().enumerate() {
            let audible = main_audible(id, index) || system & SYSTEM_ALL_AUDIBLE != 0;
            self.load(index, track, if audible { base } else { 0 });
        }
        let sequence = &mut self.sequences[0];
        sequence.state = 0;
        sequence.loaded = tracks.iter().any(Option::is_some);
        sequence.loaded
    }

    /// `Snd_bgm_sub`: the room's sub replaces what's loaded. Its channels start
    /// silent, but in two rooms of the first stage.
    fn load_sub(&mut self, word: u8, place: Place) -> bool {
        let tracks = if word == 0xff {
            [None; SUB_CHANNELS]
        } else {
            sub_tracks(word & 0x3f)
        };
        let audible = place.stage == 0 && matches!(place.room, 0 | 4);
        let mut any = false;
        for (channel, &track) in tracks.iter().enumerate() {
            let base = self.sequences[1 + channel].volume;
            self.load(3 + channel, track, if audible { base } else { 0 });
            let sequence = &mut self.sequences[1 + channel];
            sequence.state = 0;
            sequence.loaded = track.is_some();
            any |= track.is_some();
        }
        any
    }

    fn fade(&mut self, sequence: usize) {
        self.sequences[sequence].fade = FADE_TICKS;
    }

    /// `Snd_bgm_set` and `Snd_bgm_play_ck`, as a room loads: `word` is the
    /// table's word for it.
    pub fn enter_room(&mut self, word: RoomMusic, place: Place) {
        if place.system & SYSTEM_NO_MUSIC != 0 {
            return;
        }
        let main_changed = (word.main ^ self.current.main) & 0x3f != 0;
        if main_changed {
            let loaded = self.load_main(word.main, place.system);
            self.sequences[0].hold = if loaded { word.main >> 6 } else { HOLD_NOTHING };
        } else if word.main & 0xc0 == 0 {
            if self.sequences[0].state == STATE_STOPPED {
                if self.sequences[0].loaded {
                    let base = self.sequences[0].volume;
                    self.set_volume(0, base);
                }
                self.sequences[0].hold = 0;
            }
        } else if !matches!(self.sequences[0].state, 0 | 0xff) {
            self.fade(0);
            self.sequences[0].state = STATE_ROOM_FADE;
        }
        if (word.sub ^ self.current.sub) & 0x3f == 0 && !main_changed {
            if word.sub & 0x40 != 0 && self.sequences[1].state != 0 {
                self.fade(1);
            }
            if word.sub & 0x80 != 0 && self.sequences[2].state != 0 {
                self.fade(2);
            }
        } else if self.load_sub(word.sub, place) {
            self.sequences[1].hold = (word.sub >> 6) & 1;
            self.sequences[2].hold = word.sub >> 7;
        } else {
            self.sequences[1].hold = HOLD_NOTHING;
            self.sequences[2].hold = HOLD_NOTHING;
        }
        self.current = word;
        for sequence in 0..SEQUENCES {
            if self.sequences[sequence].hold == 0 && self.sequences[sequence].loaded {
                self.play(sequence, true);
                self.sequences[sequence].state = STATE_PLAYING;
            }
        }
    }

    /// `Snd_bgm_ctr`, `sce_bgm_control`'s operands. The volume and pan count
    /// from 1, 0 leaving them be.
    pub fn control(&mut self, [target, op, channel, volume, pan]: [u8; 5], place: Place) {
        let sequence = usize::from(target);
        if place.system & SYSTEM_NO_MUSIC != 0 || sequence >= SEQUENCES {
            return;
        }
        let mut pan = pan;
        if place.stage == 0 && place.room == 0x0b && place.cut == 0x0d {
            self.sequences[0].volume = 0x3f;
            pan = 0x40;
        }
        let state = self.sequences[sequence];
        if state.hold == HOLD_NOTHING || !state.loaded {
            return;
        }
        let looping = !plays_once(target, place);
        match op {
            1 => {
                if sequence == 0 {
                    self.channels[0].volume = state.volume;
                } else {
                    self.set_volume(sequence, state.volume);
                }
                self.play(sequence, looping);
                self.sequences[sequence].state = STATE_PLAYING;
            }
            2 if state.state != 0 => {
                self.set_volume(sequence, state.volume);
                self.stop(sequence);
                self.sequences[sequence].state = STATE_STOPPED;
            }
            3 => {
                self.set_volume(sequence, state.volume);
                self.play(sequence, sequence == 0);
                self.sequences[sequence].state = STATE_PLAYING;
            }
            4 => {
                self.set_volume(sequence, 0);
                self.stop(sequence);
                self.sequences[sequence].state = STATE_MUTED;
            }
            5 => {
                self.fade(sequence);
                self.sequences[sequence].state = STATE_SCRIPT_FADE;
            }
            _ => {}
        }
        // Its numbers are of the main's channels or of the sub's: a sub's
        // sequence doesn't change which channel 1 or 2 means.
        let family = if sequence == 0 { 0..3 } else { 3..CHANNELS };
        let targets = match channel {
            0 => family,
            n => {
                let index = family.start + usize::from(n) - 1;
                index..(index + 1).min(family.end)
            }
        };
        if volume != 0 {
            for (offset, index) in targets.clone().enumerate() {
                let level = main_room8_volume(sequence, channel, offset, place);
                self.channels[index].volume = level.unwrap_or(volume - 1);
            }
        }
        if pan != 0 {
            for index in targets {
                self.channels[index].pan = pan - 1;
            }
        }
    }

    /// One tick of `Snd_se_call`'s fades.
    pub fn tick(&mut self) {
        for sequence in 0..SEQUENCES {
            if self.sequences[sequence].fade > 0 {
                self.sequences[sequence].fade -= 1;
                for index in sequence_channels(sequence) {
                    self.channels[index].volume = self.channels[index].volume.saturating_sub(1);
                }
            }
            if self.sequences[sequence].state > 5 {
                self.sequences[sequence].state -= 1;
                if self.sequences[sequence].state == 5 {
                    self.sequences[sequence].state = 0;
                    self.stop(sequence);
                }
            }
        }
    }
}

/// The main's volumes `Snd_bgm_ctr` fixes for two cuts of stage 1's room 8.
fn main_room8_volume(sequence: usize, channel: u8, offset: usize, place: Place) -> Option<u8> {
    const CUT_0: [u8; 3] = [0x48, 0x39, 0x1d];
    const CUT_D: [u8; 3] = [0, 0x45, 0x1d];
    if sequence != 0 || channel != 0 || place.stage != 0 || place.room != 8 {
        return None;
    }
    match place.cut {
        0 => CUT_0.get(offset).copied(),
        0x0d => CUT_D.get(offset).copied(),
        _ => None,
    }
}

/// `Snd_bgm_ctr`'s rooms whose start of `target` doesn't loop.
fn plays_once(target: u8, place: Place) -> bool {
    let room = place.room;
    let by_room = match place.stage {
        0 => matches!(room, 0x13..=0x15) && target == 1,
        1 => room == 0x19 || (matches!(room, 0x1b | 6) && target == 2),
        2 => match room {
            0 => target == 2 || (target == 0 && place.system & SYSTEM_MAIN_ONCE == 0),
            1 => true,
            4 | 8 | 10 | 0x0b => target == 1,
            _ => false,
        },
        3 => match room {
            0 => target == 2 && place.player < 10 && place.player & 1 != 0,
            7 => target == 2,
            _ => false,
        },
        5 => match room {
            0x0c => matches!(target, 1 | 2),
            0x14 => target == 2,
            _ => false,
        },
        _ => false,
    };
    by_room || (place.system & SYSTEM_SUB_ONCE != 0 && target == 1)
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

    fn street() -> Place {
        Place {
            stage: 0,
            room: 0x10,
            ..Place::default()
        }
    }

    fn word(main: u8, sub: u8) -> RoomMusic {
        RoomMusic { main, sub }
    }

    #[test]
    fn a_room_starts_its_main_silent_and_a_script_raises_it() {
        let mut music = Music::default();
        music.enter_room(word(0, 0xff), street());
        for index in 0..3 {
            assert_eq!(music.channels[index].play, Some(true));
        }
        assert!(
            music.channels[..3]
                .iter()
                .any(|channel| channel.track.is_some())
        );
        assert!(!main_audible(0, 0));
        assert_eq!(music.channels[0].volume, 0);
        music.control([0, 0, 1, 120, 51], street());
        assert_eq!(music.channels[0].volume, 119);
        assert_eq!(music.channels[0].pan, 50);
        assert_eq!(music.channels[1].volume, 0);
    }

    #[test]
    fn the_same_main_keeps_playing_through_a_door() {
        let mut music = Music::default();
        music.enter_room(word(0, 0xff), street());
        music.control([0, 0, 1, 120, 51], street());
        for channel in &mut music.channels {
            channel.load = false;
        }
        music.enter_room(word(0, 0xff), street());
        assert!(!music.channels[0].load);
        assert_eq!(music.channels[0].volume, 119);
    }

    /// `Snd_bgm_play_ck` still plays what the first room started, so the fade
    /// lowers the main by its 90 steps and doesn't stop it.
    #[test]
    fn a_room_flagged_to_end_the_main_fades_it() {
        let mut music = Music::default();
        music.enter_room(word(0, 0xff), street());
        music.control([0, 0, 0, 101, 0], street());
        music.channels[0].stop = false;
        music.enter_room(word(0x40, 0xff), street());
        for _ in 0..200 {
            music.tick();
        }
        assert_eq!(music.channels[0].volume, 10);
        assert!(!music.channels[0].stop);
    }

    #[test]
    fn a_main_first_loaded_by_a_flagged_room_waits_for_a_script() {
        let mut music = Music::default();
        music.enter_room(word(0x40, 0xff), street());
        assert_eq!(music.channels[0].play, None);
        music.control([0, 1, 0, 0, 0], street());
        assert_eq!(music.channels[0].play, Some(true));
    }

    #[test]
    fn a_script_fade_stops_the_main_after_its_count() {
        let mut music = Music::default();
        music.enter_room(word(0, 0xff), street());
        music.control([0, 0, 0, 101, 0], street());
        music.channels[0].stop = false;
        music.control([0, 5, 0, 0, 0], street());
        for _ in 0..44 {
            music.tick();
        }
        assert_eq!(music.channels[0].volume, 56);
        assert!(!music.channels[0].stop);
        music.tick();
        assert!(music.channels[0].stop);
    }

    #[test]
    fn a_stopped_main_plays_again_on_entering_a_room() {
        let mut music = Music::default();
        music.enter_room(word(0, 0xff), street());
        music.channels[0].play = None;
        music.control([0, 2, 0, 0, 0], street());
        assert!(music.channels[0].stop);
        music.enter_room(word(0, 0xff), street());
        assert_eq!(music.channels[0].play, Some(true));
    }

    #[test]
    fn a_sub_loads_on_its_own_channels_and_holds_by_its_flags() {
        let mut music = Music::default();
        let id = (0..0x38)
            .find(|&id| sub_tracks(id).iter().all(Option::is_some))
            .unwrap();
        music.enter_room(word(0xff, id | 0x80), street());
        assert_eq!(music.channels[3].play, Some(true));
        assert_eq!(music.channels[4].play, None);
        assert_eq!(music.channels[3].volume, 0);
        music.control([1, 0, 1, 81, 0], street());
        assert_eq!(music.channels[3].volume, 80);
    }

    #[test]
    fn control_on_nothing_loaded_does_nothing() {
        let mut music = Music::default();
        music.enter_room(word(0xff, 0xff), street());
        music.control([0, 1, 0, 101, 0], street());
        assert_eq!(music.channels[0].play, None);
        assert_eq!(music.channels[0].volume, 0);
    }

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
