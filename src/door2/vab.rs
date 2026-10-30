//! The PlayStation VAB sound bank a DO2 carries (`pBAV`).
//!
//! A VAB is a header (VH) followed by the sample body (VB). The VH is a
//! 32-byte header, 128 program records of 16 bytes, 16 tone attributes of
//! 32 bytes for each of the `ps` programs, and a table of 256 `u16` VAG
//! sizes in 8-byte units (entry 0 is unused). The VB holds the VAGs back to
//! back in that order, as PlayStation 4-bit ADPCM.
//!
//! Only what the door sounds need is read: the tone attributes (volume, pan,
//! notes and the VAG they play) and the VAG bodies, decoded by
//! [`decode_adpcm`].

use anyhow::{Context, Result, bail, ensure};

/// The VAB magic, `VABp` stored little-endian.
pub const MAGIC: &[u8; 4] = b"pBAV";
/// Size of the VH header.
const HEADER_LEN: usize = 0x20;
/// The fixed program table: 128 records of 16 bytes.
const PROGRAMS_LEN: usize = 128 * 16;
/// Tone attributes per program.
pub const TONES_PER_PROGRAM: usize = 16;
/// Size of one tone attribute record (`VagAtr`).
const TONE_LEN: usize = 0x20;
/// The VAG size table: 256 `u16`s.
const VAG_TABLE_LEN: usize = 256 * 2;
/// One ADPCM block: a shift/filter byte, a flag byte and 28 samples.
const BLOCK_LEN: usize = 16;
/// Samples per ADPCM block.
const BLOCK_SAMPLES: usize = 28;
/// Block flag bit that ends a one-shot sample.
const FLAG_END: u8 = 0x01;
/// The rate the door sounds play at. A VAG carries no rate; the RE2 door
/// banks are recorded for 22.05 kHz at their centre note.
pub const SAMPLE_RATE: u32 = 22050;
/// The ADPCM prediction filters, in 1/64 units.
const FILTERS: [[i32; 2]; 5] = [[0, 0], [60, 0], [115, -52], [98, -55], [122, -60]];

/// One tone attribute record (`VagAtr`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Tone {
    pub priority: u8,
    pub mode: u8,
    /// Volume, `0..=127`.
    pub volume: u8,
    /// Pan, `0x40` centred.
    pub pan: u8,
    /// The note the sample plays at its recorded pitch.
    pub center: u8,
    /// Fine tuning of `center`, in 1/128 semitones.
    pub shift: u8,
    /// Lowest note the tone answers.
    pub min: u8,
    /// Highest note the tone answers.
    pub max: u8,
    /// The program the tone belongs to.
    pub program: i16,
    /// One-based VAG index; 0 is no sample.
    pub vag: i16,
}

/// A parsed VAB: its tone attributes and the raw ADPCM of each VAG.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Vab {
    /// The total VAB size the header declares (VH and VB).
    pub size: usize,
    /// The tone attributes, [`TONES_PER_PROGRAM`] per program in file order.
    pub tones: Vec<Tone>,
    /// Each VAG's ADPCM body; index 0 is VAG 1.
    pub vags: Vec<Vec<u8>>,
}

impl Vab {
    /// Parse the VAB at the start of `data`.
    pub fn parse(data: &[u8]) -> Result<Self> {
        ensure!(
            data.get(..4) == Some(MAGIC.as_slice()),
            "VAB does not start with pBAV"
        );
        let header = data.get(..HEADER_LEN).context("VAB header is truncated")?;
        let u16_at = |at: usize| u16::from_le_bytes([header[at], header[at + 1]]);
        let size = u32::from_le_bytes(header[0x0C..0x10].try_into().unwrap()) as usize;
        let programs = usize::from(u16_at(0x12));
        let vag_count = usize::from(u16_at(0x16));
        if programs > 128 || vag_count > 255 {
            bail!("VAB declares {programs} programs and {vag_count} VAGs");
        }
        let tones_at = HEADER_LEN + PROGRAMS_LEN;
        let table_at = tones_at + programs * TONES_PER_PROGRAM * TONE_LEN;
        let body_at = table_at + VAG_TABLE_LEN;
        ensure!(
            size >= body_at && data.len() >= size,
            "VAB declares {size} bytes; its header needs {body_at} and the data holds {}",
            data.len()
        );
        let tones = data[tones_at..table_at]
            .as_chunks::<TONE_LEN>()
            .0
            .iter()
            .map(|record| Tone {
                priority: record[0],
                mode: record[1],
                volume: record[2],
                pan: record[3],
                center: record[4],
                shift: record[5],
                min: record[6],
                max: record[7],
                program: i16::from_le_bytes([record[0x14], record[0x15]]),
                vag: i16::from_le_bytes([record[0x16], record[0x17]]),
            })
            .collect();
        let mut vags = Vec::with_capacity(vag_count);
        let mut at = body_at;
        for index in 1..=vag_count {
            let units = usize::from(u16::from_le_bytes([
                data[table_at + index * 2],
                data[table_at + index * 2 + 1],
            ]));
            let end = at + units * 8;
            let body = data
                .get(at..end)
                .filter(|_| end <= size)
                .with_context(|| format!("VAG {index} runs past the {size}-byte VAB"))?;
            vags.push(body.to_vec());
            at = end;
        }
        Ok(Self { size, tones, vags })
    }

    /// Tone `tone` of program `program`, if the bank has it.
    pub fn tone(&self, program: usize, tone: usize) -> Option<&Tone> {
        if tone >= TONES_PER_PROGRAM {
            return None;
        }
        self.tones.get(program * TONES_PER_PROGRAM + tone)
    }

    /// The ADPCM body of one-based VAG `index`.
    pub fn vag(&self, index: i16) -> Option<&[u8]> {
        let index = usize::try_from(index).ok()?.checked_sub(1)?;
        self.vags.get(index).map(Vec::as_slice)
    }
}

/// Decode PlayStation 4-bit ADPCM to 16-bit PCM, up to the block that sets
/// the end flag.
pub fn decode_adpcm(data: &[u8]) -> Vec<i16> {
    let mut out = Vec::with_capacity(data.len() / BLOCK_LEN * BLOCK_SAMPLES);
    let (mut old, mut older) = (0i32, 0i32);
    for block in data.as_chunks::<BLOCK_LEN>().0 {
        let shift = u32::from(block[0] & 0x0F).min(12);
        let [a, b] = FILTERS[usize::from(block[0] >> 4).min(FILTERS.len() - 1)];
        for byte in &block[2..] {
            for nibble in [byte & 0x0F, byte >> 4] {
                // Sign-extend the nibble into the top of a 16-bit word.
                let sample = i32::from(((u16::from(nibble) << 12) as i16) >> shift);
                let value = (sample + (old * a + older * b + 32) / 64).clamp(-32768, 32767);
                out.push(value as i16);
                older = old;
                old = value;
            }
        }
        if block[1] & FLAG_END != 0 {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-program VAB with tone 0 playing VAG 1 (one block).
    pub(crate) fn vab(block: [u8; BLOCK_LEN]) -> Vec<u8> {
        let mut data = vec![0u8; HEADER_LEN + PROGRAMS_LEN + TONES_PER_PROGRAM * TONE_LEN];
        data[..4].copy_from_slice(MAGIC);
        data[0x12] = 1;
        data[0x16] = 1;
        let tone = HEADER_LEN + PROGRAMS_LEN;
        data[tone + 2] = 0x7F;
        data[tone + 3] = 0x40;
        data[tone + 0x16] = 1;
        let mut table = [0u8; VAG_TABLE_LEN];
        table[2] = (BLOCK_LEN / 8) as u8;
        data.extend_from_slice(&table);
        data.extend_from_slice(&block);
        let size = data.len() as u32;
        data[0x0C..0x10].copy_from_slice(&size.to_le_bytes());
        data
    }

    #[test]
    fn reads_tones_and_vags() {
        let mut block = [0u8; BLOCK_LEN];
        block[1] = FLAG_END;
        let data = vab(block);
        let bank = Vab::parse(&data).unwrap();
        assert_eq!(bank.size, data.len());
        assert_eq!(bank.tones.len(), TONES_PER_PROGRAM);
        let tone = bank.tone(0, 0).unwrap();
        assert_eq!((tone.volume, tone.pan, tone.vag), (0x7F, 0x40, 1));
        assert_eq!(bank.vag(1), Some(block.as_slice()));
        assert_eq!(bank.vag(0), None);
        assert_eq!(bank.vag(2), None);
        assert!(bank.tone(0, TONES_PER_PROGRAM).is_none());
    }

    #[test]
    fn rejects_a_bad_magic_and_a_short_body() {
        assert!(Vab::parse(b"VABp").is_err());
        let mut data = vab([0; BLOCK_LEN]);
        data.truncate(data.len() - 1);
        assert!(Vab::parse(&data).is_err());
    }

    #[test]
    fn adpcm_predicts_and_stops_at_the_end_flag() {
        // Shift 12 keeps each nibble as is; filter 0 adds no prediction.
        let mut first = [0u8; BLOCK_LEN];
        first[0] = 0x0C;
        first[2] = 0x71; // samples 1 then 7
        let mut second = [0u8; BLOCK_LEN];
        second[0] = 0x1C; // filter 1, shift 12
        second[1] = FLAG_END;
        let mut data = first.to_vec();
        data.extend_from_slice(&second);
        data.extend_from_slice(&first);
        let pcm = decode_adpcm(&data);
        assert_eq!(pcm.len(), 2 * BLOCK_SAMPLES);
        assert_eq!(&pcm[..2], &[1, 7]);
        // Filter 1 decays the last sample (0) by 60/64: still 0.
        assert!(pcm[BLOCK_SAMPLES..].iter().all(|&sample| sample == 0));

        // A full-scale negative nibble at shift 0 is -32768.
        let mut loud = [0u8; BLOCK_LEN];
        loud[1] = FLAG_END;
        loud[2] = 0x08;
        assert_eq!(decode_adpcm(&loud)[0], -32768);
    }
}
