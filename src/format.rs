//! Per-file format detection.
//!
//! A pack holds one game, but its files are not tied to one game's formats: a
//! pack may carry RE1-format and RE2-format files side by side, so a loader
//! decides each file's format from the file itself, never from a pack-wide
//! setting. Detection checks, in order:
//!
//! 1. the extension, where the formats already differ (`.dor` / `.do2`,
//!    `.emd` / `.pld`), which the caller has already used to pick a family;
//! 2. header magic, where one exists ([`wav_codec`], [`is_mpeg_program`]);
//! 3. structural heuristics, where the header says nothing ([`rdt`], [`emd`]).
//!
//! A heuristic runs every dialect's plausibility checks (offset tables in
//! range and in order, directory sizes landing on the file length) and
//! reports how many each passed in a [`Scores`]. A dialect wins only when it
//! passes every check and the other fails at least one; otherwise the result
//! is [`Detection::Ambiguous`] or [`Detection::Unknown`], so a caller fails
//! loudly rather than guessing. Detection only reads headers and offset
//! tables; it never decodes.

use std::fmt;

/// A game's file-format dialect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Dialect {
    Re1,
    Re2,
}

impl Dialect {
    /// The dialect's stable name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Re1 => "re1",
            Self::Re2 => "re2",
        }
    }
}

impl fmt::Display for Dialect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// How one dialect's plausibility checks went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Score {
    /// Checks run.
    pub checks: u32,
    /// Checks passed.
    pub passed: u32,
}

impl Score {
    /// Whether every check passed.
    pub fn plausible(self) -> bool {
        self.checks > 0 && self.passed == self.checks
    }

    /// Checks failed.
    pub fn failed(self) -> u32 {
        self.checks - self.passed
    }

    fn check(&mut self, ok: bool) {
        self.checks += 1;
        self.passed += u32::from(ok);
    }
}

/// Both dialects' scores for one file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Scores {
    pub re1: Score,
    pub re2: Score,
}

impl Scores {
    /// The verdict the scores give.
    pub fn detection(self) -> Detection {
        match (self.re1.plausible(), self.re2.plausible()) {
            (true, false) => Detection::Dialect(Dialect::Re1),
            (false, true) => Detection::Dialect(Dialect::Re2),
            (true, true) => Detection::Ambiguous,
            (false, false) => Detection::Unknown,
        }
    }

    /// How many checks the losing dialect failed: the margin by which the
    /// winner won. Zero when there is no single winner.
    pub fn margin(self) -> u32 {
        match self.detection() {
            Detection::Dialect(Dialect::Re1) => self.re2.failed(),
            Detection::Dialect(Dialect::Re2) => self.re1.failed(),
            _ => 0,
        }
    }
}

/// A detection verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detection {
    /// Exactly one dialect is plausible.
    Dialect(Dialect),
    /// More than one dialect is plausible.
    Ambiguous,
    /// No dialect is plausible.
    Unknown,
}

impl Detection {
    /// The detected dialect, if exactly one was.
    pub fn dialect(self) -> Option<Dialect> {
        match self {
            Self::Dialect(dialect) => Some(dialect),
            _ => None,
        }
    }
}

fn u32_at(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        data.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

/// The RE1 RDT header: 0x48 bytes of fields, then 19 `u32` section offsets;
/// the camera table follows at 0x94.
const RE1_RDT_TABLE: usize = 0x48;
const RE1_RDT_POINTERS: usize = 19;
const RE1_RDT_HEADER: usize = RE1_RDT_TABLE + RE1_RDT_POINTERS * 4;

/// The RE2 RDT header: eight `u8` counts, then 23 `u32` section offsets.
const RE2_RDT_TABLE: usize = 8;
const RE2_RDT_POINTERS: usize = 23;
const RE2_RDT_HEADER: usize = RE2_RDT_TABLE + RE2_RDT_POINTERS * 4;

/// Score one offset table: every entry zero or inside the file past the
/// header (an empty last section may start at the very end), and at least
/// one entry set.
fn score_offsets(data: &[u8], table: usize, count: usize, header: usize) -> Score {
    let mut score = Score::default();
    score.check(data.len() > header);
    let mut set = 0;
    for index in 0..count {
        let offset = u32_at(data, table + index * 4);
        score.check(offset.is_some_and(|offset| {
            offset == 0 || (offset as usize >= header && (offset as usize) <= data.len())
        }));
        set += usize::from(offset.is_some_and(|offset| offset != 0));
    }
    score.check(set > 0);
    score
}

/// Score an RDT against both dialects' headers.
pub fn rdt_scores(data: &[u8]) -> Scores {
    let mut re1 = score_offsets(data, RE1_RDT_TABLE, RE1_RDT_POINTERS, RE1_RDT_HEADER);
    let mut re2 = score_offsets(data, RE2_RDT_TABLE, RE2_RDT_POINTERS, RE2_RDT_HEADER);
    // Byte 1 is the camera count in both, and a room has at least one.
    let cameras = data.get(1).copied().unwrap_or(0);
    re1.check(cameras > 0);
    re2.check(cameras > 0);
    Scores { re1, re2 }
}

/// Detect an RDT's dialect.
pub fn rdt(data: &[u8]) -> Detection {
    rdt_scores(data).detection()
}

/// Where the TIM at `at` ends, from its header and block lengths.
fn tim_end(data: &[u8], at: usize) -> Option<usize> {
    let flags = u32_at(data, at.checked_add(4)?)?;
    let mut end = at.checked_add(8)?;
    if flags & 8 != 0 {
        end = end.checked_add(u32_at(data, end)? as usize)?;
    }
    end.checked_add(u32_at(data, end)? as usize)
}

/// The RE1 EMD directory: five `u32` chunk offsets in the last 20 bytes, the
/// last chunk an embedded TIM.
const RE1_EMD_DIRECTORY: usize = 5;

/// Score an EMD against both dialects' directories.
pub fn emd_scores(data: &[u8]) -> Scores {
    let mut re1 = Score::default();
    let len = data.len();
    let directory = len.checked_sub(RE1_EMD_DIRECTORY * 4);
    re1.check(directory.is_some());
    let offsets: Vec<Option<u32>> = (0..RE1_EMD_DIRECTORY)
        .map(|slot| directory.and_then(|start| u32_at(data, start + slot * 4)))
        .collect();
    // Slot 0 may be junk in shipped models; slots 1-4 are the EMR, EDD, TMD
    // and TIM chunks, in file order and before the directory.
    for pair in offsets[1..].windows(2) {
        re1.check(matches!(pair, [Some(a), Some(b)] if a < b));
    }
    let tim = offsets[4].map(|tim| tim as usize);
    re1.check(tim.is_some_and(|tim| u32_at(data, tim) == Some(0x10)));
    // The TIM's own block lengths end it exactly where the directory starts.
    re1.check(matches!((tim, directory), (Some(tim), Some(end))
        if tim_end(data, tim) == Some(end)));

    // RE2: a `u32` directory offset and chunk count up front, the directory
    // ending the file, and the chunks in order between the header and it.
    let mut re2 = Score::default();
    let start = u32_at(data, 0).map(|offset| offset as usize);
    let count = u32_at(data, 4).map(|count| count as usize);
    re2.check(count.is_some_and(|count| (1..=32).contains(&count)));
    let fits = matches!((start, count), (Some(start), Some(count))
        if (1..=32).contains(&count) && start >= 8 && start.checked_add(count * 4) == Some(len));
    re2.check(fits);
    if fits {
        let (start, count) = (start.unwrap(), count.unwrap());
        let chunks: Vec<u32> = (0..count)
            .filter_map(|slot| u32_at(data, start + slot * 4))
            .collect();
        re2.check(chunks.first().is_some_and(|&first| first >= 8));
        re2.check(chunks.windows(2).all(|pair| pair[0] <= pair[1]));
        re2.check(chunks.last().is_some_and(|&last| (last as usize) <= start));
    }
    Scores { re1, re2 }
}

/// Detect an EMD's dialect.
pub fn emd(data: &[u8]) -> Detection {
    emd_scores(data).detection()
}

/// The RIFF/WAVE format tag (1 PCM, 2 MS-ADPCM), if `data` is a WAV with a
/// `fmt ` chunk first.
pub fn wav_codec(data: &[u8]) -> Option<u16> {
    if data.get(0..4)? != b"RIFF" || data.get(8..12)? != b"WAVE" || data.get(12..16)? != b"fmt " {
        return None;
    }
    Some(u16::from_le_bytes(data.get(20..22)?.try_into().ok()?))
}

/// The PCM WAVE format tag.
pub const WAV_PCM: u16 = 1;
/// The Microsoft ADPCM WAVE format tag (RE2's voice and BGM).
pub const WAV_MS_ADPCM: u16 = 2;

/// Whether `data` starts with an MPEG program stream pack header.
pub fn is_mpeg_program(data: &[u8]) -> bool {
    data.starts_with(&[0x00, 0x00, 0x01, 0xBA])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(data: &mut [u8], offset: usize, value: u32) {
        data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn re1_rdt() -> Vec<u8> {
        let mut data = vec![0u8; 0x200];
        data[1] = 2;
        // Fields between the counts and the table look like RE1's lights:
        // colours and positions, nothing like offsets.
        for offset in (8..RE1_RDT_TABLE).step_by(4) {
            put(&mut data, offset, 0x8080_7F40);
        }
        put(&mut data, RE1_RDT_TABLE, 0x100);
        put(&mut data, RE1_RDT_TABLE + 4, 0x180);
        data
    }

    fn re2_rdt() -> Vec<u8> {
        let mut data = vec![0u8; 0x200];
        data[1] = 3;
        put(&mut data, RE2_RDT_TABLE, 0x64);
        put(&mut data, RE2_RDT_TABLE + 8, 0x120);
        // Past RE2's own table, what RE1 reads as its table is RE2 section
        // data.
        for offset in (RE2_RDT_HEADER..RE1_RDT_HEADER).step_by(4) {
            put(&mut data, offset, 0xFFFF_0123);
        }
        data
    }

    #[test]
    fn rdt_headers_are_told_apart() {
        assert_eq!(rdt(&re1_rdt()), Detection::Dialect(Dialect::Re1));
        assert_eq!(rdt(&re2_rdt()), Detection::Dialect(Dialect::Re2));
        assert!(rdt_scores(&re1_rdt()).margin() > 0);
    }

    #[test]
    fn an_rdt_both_headers_fit_is_ambiguous() {
        let mut data = vec![0u8; 0x200];
        data[1] = 1;
        put(&mut data, RE1_RDT_TABLE, 0x100);
        assert_eq!(rdt(&data), Detection::Ambiguous);
        assert_eq!(rdt_scores(&data).margin(), 0);
    }

    #[test]
    fn junk_is_unknown() {
        assert_eq!(rdt(&[0xFF; 0x200]), Detection::Unknown);
        assert_eq!(rdt(&[]), Detection::Unknown);
        assert_eq!(emd(&[0xFF; 64]), Detection::Unknown);
        assert_eq!(emd(&[]), Detection::Unknown);
    }

    #[test]
    fn emd_directories_are_told_apart() {
        // RE1: chunks at 0x10, 0x20, 0x30, TIM at 0x40, directory last.
        let mut re1 = vec![0u8; 0x70];
        put(&mut re1, 0, 0xDEAD_BEEF);
        let directory = re1.len() - 20;
        // A 16-bit TIM with no CLUT whose image block runs to the directory.
        put(&mut re1, 0x40, 0x10);
        put(&mut re1, 0x44, 2);
        put(&mut re1, 0x48, (directory - 0x48) as u32);
        for (slot, offset) in [0, 0x10, 0x20, 0x30, 0x40].into_iter().enumerate() {
            put(&mut re1, directory + slot * 4, offset);
        }
        assert_eq!(emd(&re1), Detection::Dialect(Dialect::Re1));

        // RE2: header, eight chunks, directory last.
        let mut re2 = vec![0u8; 0x48];
        put(&mut re2, 0, 0x28);
        put(&mut re2, 4, 8);
        for slot in 0..8 {
            put(&mut re2, 0x28 + slot * 4, 8 + slot as u32 * 4);
        }
        assert_eq!(emd(&re2), Detection::Dialect(Dialect::Re2));
    }

    #[test]
    fn wav_tags_and_mpeg_magic() {
        let mut wav = b"RIFF\0\0\0\0WAVEfmt \x10\0\0\0".to_vec();
        wav.extend([2, 0, 1, 0]);
        assert_eq!(wav_codec(&wav), Some(WAV_MS_ADPCM));
        assert_eq!(wav_codec(b"RIFX"), None);
        assert!(is_mpeg_program(&[0, 0, 1, 0xBA, 0x21]));
        assert!(!is_mpeg_program(b"RIFF"));
    }
}
