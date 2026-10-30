//! Microsoft ADPCM (WAVE format tag 2), RE2's voice, music and sound-bank
//! codec.
//!
//! Ported from biohazard-utils' `MSADPCMDecoder`. The `data` chunk is a run
//! of `block_align`-byte blocks, the last one possibly short. Each block
//! opens with a 7-byte header per channel: the predictor index, the initial
//! step (`delta`), then the two newest history samples, newest first, which
//! are also the block's first two output frames (oldest first). The rest of
//! the block is 4-bit nibbles, high nibble first, interleaved by channel.
//! Each nibble predicts from the two history samples through the predictor's
//! coefficient pair (8.8 fixed point), adds the signed nibble times `delta`,
//! and adapts `delta` through the standard adaptation table.
//!
//! The coefficient pairs come from the `fmt ` chunk's extension; every
//! retail RE2 file carries the seven standard pairs.

use anyhow::{Context, Result, bail, ensure};

use crate::budget;

/// The WAVE format tag of Microsoft ADPCM.
pub const FORMAT_TAG: u16 = 2;

/// The step adaptation table, by nibble.
const ADAPTATION: [i32; 16] = [
    230, 230, 230, 230, 307, 409, 512, 614, 768, 614, 512, 409, 307, 230, 230, 230,
];

/// The smallest step the adaptation keeps.
const MIN_DELTA: i32 = 16;

/// The largest step: ffmpeg's bound, so the adaptation product cannot
/// overflow on hostile input.
const MAX_DELTA: i32 = i32::MAX / 768;

/// The header bytes each channel's block opens with.
const HEADER_PER_CHANNEL: usize = 7;

/// The decoding parameters a `fmt ` chunk declares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Format {
    /// Interleaved channels (1 or 2 in the retail data).
    pub channels: u16,
    /// Frames per second.
    pub sample_rate: u32,
    /// Bytes per block.
    pub block_align: u16,
    /// Frames a full block decodes to.
    pub samples_per_block: u16,
    /// The predictor coefficient pairs, 8.8 fixed point.
    pub coefficients: Vec<(i16, i16)>,
}

fn u16_at(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(data.get(at..at + 2)?.try_into().ok()?))
}

fn i16_at(data: &[u8], at: usize) -> Option<i16> {
    Some(i16::from_le_bytes(data.get(at..at + 2)?.try_into().ok()?))
}

impl Format {
    /// Read the body of an MS-ADPCM `fmt ` chunk: the 16-byte common part,
    /// then `cbSize`, the frames per block, the coefficient count and the
    /// pairs.
    pub fn parse(fmt: &[u8]) -> Result<Self> {
        let field = |at| u16_at(fmt, at).context("MS-ADPCM `fmt ` chunk is truncated");
        ensure!(
            field(0)? == FORMAT_TAG,
            "not an MS-ADPCM `fmt ` chunk (tag {})",
            field(0)?
        );
        let channels = field(2)?;
        let sample_rate = u32::from(field(4)?) | u32::from(field(6)?) << 16;
        let block_align = field(12)?;
        let bits = field(14)?;
        let samples_per_block = field(18)?;
        let count = usize::from(field(20)?);
        ensure!(bits == 4, "MS-ADPCM declares {bits} bits per sample, not 4");
        ensure!(channels > 0, "MS-ADPCM declares zero channels");
        ensure!(sample_rate > 0, "MS-ADPCM declares a zero sample rate");
        ensure!(count > 0, "MS-ADPCM declares no predictor coefficients");
        ensure!(
            samples_per_block != 1,
            "MS-ADPCM declares 1 frame per block, fewer than its two header frames"
        );
        let header = HEADER_PER_CHANNEL * usize::from(channels);
        ensure!(
            usize::from(block_align) > header,
            "MS-ADPCM block of {block_align} bytes cannot hold its {header}-byte header"
        );
        let coefficients = (0..count)
            .map(|index| {
                let at = 22 + index * 4;
                Some((i16_at(fmt, at)?, i16_at(fmt, at + 2)?))
            })
            .collect::<Option<Vec<_>>>()
            .context("MS-ADPCM coefficient table is truncated")?;
        Ok(Self {
            channels,
            sample_rate,
            block_align,
            samples_per_block,
            coefficients,
        })
    }

    /// The frames a block of `len` bytes decodes to: the two header frames
    /// plus two per byte per channel of nibbles, capped at
    /// [`Self::samples_per_block`]. A block too short for its header is
    /// none.
    fn frames_in(&self, len: usize) -> usize {
        let channels = usize::from(self.channels);
        let header = HEADER_PER_CHANNEL * channels;
        if len < header {
            return 0;
        }
        let frames = 2 + (len - header) * 2 / channels;
        match usize::from(self.samples_per_block) {
            0 => frames,
            declared => frames.min(declared),
        }
    }
}

/// How the prediction's 8.8 fixed-point product is scaled back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rounding {
    /// An arithmetic shift, rounding down: biohazard-utils' port, and what
    /// [`decode`] uses.
    Shift,
    /// A division, rounding toward zero: ffmpeg's `adpcm_ms`. A negative
    /// prediction can come out one higher, and the filter carries the
    /// difference on through the rest of the block.
    Divide,
}

/// One channel's decoder state within a block.
#[derive(Clone, Copy)]
struct Channel {
    coefficient1: i32,
    coefficient2: i32,
    delta: i32,
    sample1: i32,
    sample2: i32,
}

impl Channel {
    fn expand(&mut self, nibble: u8, rounding: Rounding) -> i16 {
        let signed = i32::from(nibble) - if nibble >= 8 { 16 } else { 0 };
        // A file's own coefficients can be any `i16`, so the sum can pass
        // `i32`; the prediction is clamped below either way.
        let product = i64::from(self.sample1) * i64::from(self.coefficient1)
            + i64::from(self.sample2) * i64::from(self.coefficient2);
        let prediction = match rounding {
            Rounding::Shift => product >> 8,
            Rounding::Divide => product / 256,
        }
        .clamp(i32::MIN.into(), i32::MAX.into()) as i32;
        let sample = (prediction + signed * self.delta).clamp(i16::MIN.into(), i16::MAX.into());
        self.sample2 = self.sample1;
        self.sample1 = sample;
        self.delta =
            ((ADAPTATION[usize::from(nibble)] * self.delta) >> 8).clamp(MIN_DELTA, MAX_DELTA);
        sample as i16
    }
}

/// Decode an MS-ADPCM `data` chunk to interleaved signed 16-bit samples.
///
/// A trailing partial block decodes the nibbles it holds; one too short for
/// its header is dropped. A predictor index past the coefficient table is an
/// error.
pub fn decode(format: &Format, data: &[u8]) -> Result<Vec<i16>> {
    decode_with(format, data, Rounding::Shift)
}

/// [`decode`] with the prediction scaled by `rounding`.
pub fn decode_with(format: &Format, data: &[u8], rounding: Rounding) -> Result<Vec<i16>> {
    let channels = usize::from(format.channels);
    let block_align = usize::from(format.block_align);
    let frames: usize = data
        .chunks(block_align)
        .map(|block| format.frames_in(block.len()))
        .sum();
    let samples = frames
        .checked_mul(channels)
        .context("MS-ADPCM sample count overflows")?;
    budget::check_len(samples, budget::MAX_WAV_SAMPLES, "MS-ADPCM sample count")?;
    let mut out = budget::alloc::<i16>(samples, "MS-ADPCM samples")?;
    let mut state = vec![
        Channel {
            coefficient1: 0,
            coefficient2: 0,
            delta: 0,
            sample1: 0,
            sample2: 0,
        };
        channels
    ];
    for (index, block) in data.chunks(block_align).enumerate() {
        let frames = format.frames_in(block.len());
        if frames == 0 {
            continue;
        }
        for (channel, state) in state.iter_mut().enumerate() {
            let predictor = usize::from(block[channel]);
            let Some(&(coefficient1, coefficient2)) = format.coefficients.get(predictor) else {
                bail!(
                    "MS-ADPCM block {index} channel {channel} uses predictor {predictor} of {}",
                    format.coefficients.len()
                );
            };
            let word = |field: usize| {
                i32::from(i16_at(block, channels * (1 + 2 * field) + 2 * channel).unwrap())
            };
            *state = Channel {
                coefficient1: coefficient1.into(),
                coefficient2: coefficient2.into(),
                delta: word(0),
                sample1: word(1),
                sample2: word(2),
            };
        }
        out.extend(state.iter().map(|channel| channel.sample2 as i16));
        out.extend(state.iter().map(|channel| channel.sample1 as i16));
        let nibbles = (frames - 2) * channels;
        let body = &block[HEADER_PER_CHANNEL * channels..];
        for at in 0..nibbles {
            let byte = body[at / 2];
            let nibble = if at % 2 == 0 { byte >> 4 } else { byte & 0x0f };
            out.push(state[at % channels].expand(nibble, rounding));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The seven standard coefficient pairs every retail file carries.
    const STANDARD: [(i16, i16); 7] = [
        (256, 0),
        (512, -256),
        (0, 0),
        (192, 64),
        (240, 0),
        (460, -208),
        (392, -232),
    ];

    fn fmt(channels: u16, block_align: u16, samples_per_block: u16) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend(FORMAT_TAG.to_le_bytes());
        out.extend(channels.to_le_bytes());
        out.extend(22050u32.to_le_bytes());
        out.extend(11100u32.to_le_bytes());
        out.extend(block_align.to_le_bytes());
        out.extend(4u16.to_le_bytes());
        out.extend(32u16.to_le_bytes());
        out.extend(samples_per_block.to_le_bytes());
        out.extend((STANDARD.len() as u16).to_le_bytes());
        for (a, b) in STANDARD {
            out.extend(a.to_le_bytes());
            out.extend(b.to_le_bytes());
        }
        out
    }

    #[test]
    fn reads_the_format_extension() {
        let format = Format::parse(&fmt(1, 512, 1012)).unwrap();
        assert_eq!(format.channels, 1);
        assert_eq!(format.sample_rate, 22050);
        assert_eq!(format.block_align, 512);
        assert_eq!(format.samples_per_block, 1012);
        assert_eq!(format.coefficients, STANDARD);
        assert!(Format::parse(&fmt(1, 512, 1012)[..30]).is_err());
        assert!(Format::parse(&fmt(0, 512, 1012)).is_err());
        assert!(Format::parse(&fmt(2, 14, 2)).is_err());
        // A block always decodes its two header frames.
        assert!(Format::parse(&fmt(1, 512, 1)).is_err());
    }

    #[test]
    fn header_frames_come_oldest_first() {
        let format = Format::parse(&fmt(1, 9, 4)).unwrap();
        // Predictor 0, delta 16, sample1 100, sample2 -50, then nibbles 0, 0.
        let block = [0, 16, 0, 100, 0, 0xce, 0xff, 0x00];
        // Predictor 0 is (256, 0): each zero nibble repeats sample1.
        assert_eq!(decode(&format, &block).unwrap(), [-50, 100, 100, 100]);
    }

    #[test]
    fn nibbles_predict_and_adapt() {
        let format = Format::parse(&fmt(1, 8, 4)).unwrap();
        // Predictor 1 (512, -256) extrapolates linearly: 2*s1 - s2.
        // Nibble 1: (2*20 - 10) + 1*16 = 46, delta 16*230>>8 = 14 -> 16.
        // Nibble 0xf (-1): (2*46 - 20) - 16 = 56.
        let block = [1, 16, 0, 20, 0, 10, 0, 0x1f];
        assert_eq!(decode(&format, &block).unwrap(), [10, 20, 46, 56]);
    }

    #[test]
    fn stereo_interleaves_high_nibble_left() {
        let format = Format::parse(&fmt(2, 15, 4)).unwrap();
        let mut block = vec![0, 0];
        block.extend([16i16, 32].iter().flat_map(|v| v.to_le_bytes()));
        block.extend([1i16, 2].iter().flat_map(|v| v.to_le_bytes()));
        block.extend([3i16, 4].iter().flat_map(|v| v.to_le_bytes()));
        block.push(0x10);
        // Left: s1 1 + 1*16; right: s1 2 + 0.
        assert_eq!(decode(&format, &block).unwrap(), [3, 4, 1, 2, 17, 2]);
    }

    #[test]
    fn samples_clamp_and_partial_blocks_decode() {
        let format = Format::parse(&fmt(1, 8, 4)).unwrap();
        let full = [0, 0x00, 0x40, 0xff, 0x7f, 0, 0, 0x77];
        let short = [0, 16, 0, 5, 0, 5, 0];
        let mut data = full.to_vec();
        data.extend(short);
        assert_eq!(
            decode(&format, &data).unwrap(),
            [0, i16::MAX, i16::MAX, i16::MAX, 5, 5]
        );
        // A final block too short for its header decodes to nothing.
        let mut data = full.to_vec();
        data.extend([0, 16, 0]);
        assert_eq!(decode(&format, &data).unwrap().len(), 4);
    }

    #[test]
    fn rounding_differs_only_for_negative_predictions() {
        let format = Format::parse(&fmt(1, 8, 4)).unwrap();
        // Predictor 3 (192, 64): (192 * -3 + 64 * -2) / 256 = -704 / 256,
        // -3 shifted and -2 divided.
        let block = [3, 16, 0, 0xfd, 0xff, 0xfe, 0xff, 0x00];
        assert_eq!(decode(&format, &block).unwrap()[2], -3);
        assert_eq!(
            decode_with(&format, &block, Rounding::Divide).unwrap()[2],
            -2
        );
        let positive = [3, 16, 0, 3, 0, 2, 0, 0x00];
        assert_eq!(
            decode(&format, &positive).unwrap(),
            decode_with(&format, &positive, Rounding::Divide).unwrap()
        );
    }

    #[test]
    fn extreme_coefficients_clamp_instead_of_overflowing() {
        let mut fmt = fmt(1, 8, 4);
        // Predictor 0 becomes (-32768, -32768).
        fmt[22..26].copy_from_slice(&[0x00, 0x80, 0x00, 0x80]);
        let format = Format::parse(&fmt).unwrap();
        let block = [0, 16, 0, 0x00, 0x80, 0x00, 0x80, 0x00];
        assert_eq!(
            decode(&format, &block).unwrap(),
            [i16::MIN, i16::MIN, i16::MAX, 128]
        );
    }

    #[test]
    fn unknown_predictors_fail() {
        let format = Format::parse(&fmt(1, 8, 4)).unwrap();
        assert!(decode(&format, &[7, 16, 0, 0, 0, 0, 0, 0]).is_err());
    }

    #[test]
    fn the_step_never_drops_below_sixteen() {
        let mut channel = Channel {
            coefficient1: 256,
            coefficient2: 0,
            delta: 16,
            sample1: 0,
            sample2: 0,
        };
        channel.expand(0, Rounding::Shift);
        assert_eq!(channel.delta, MIN_DELTA);
        for _ in 0..64 {
            channel.expand(7, Rounding::Shift);
        }
        assert_eq!(channel.delta, MAX_DELTA);
        assert_eq!(channel.sample1, i32::from(i16::MAX));
    }
}
