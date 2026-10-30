//! The MPEG-1 Audio Layer II decoder (ISO/IEC 11172-3): the RE2 films'
//! 44.1 kHz stereo soundtrack.
//!
//! The bit allocation and scale factor steps follow pl_mpeg (after Martin
//! J. Fiedler's kjmp2), integer for integer; the requantisation uses the
//! standard's exact `(2s - (levels - 1)) / levels` fraction, where kjmp2
//! approximates it and plays small-allocation subbands quiet. The polyphase
//! synthesis computes the matrixing step straight from the standard's
//! `cos((16 + i)(2k + 1)π / 64)` matrix instead of kjmp2's unrolled fast
//! DCT, then windows with the same 512-tap table.

use std::sync::OnceLock;

use anyhow::{Result, bail};

use super::bits::Bits;
use super::tables::SYNTHESIS_WINDOW;

/// PCM frames one Layer II frame decodes to.
pub const SAMPLES_PER_FRAME: usize = 1152;

const SAMPLE_RATES: [u32; 3] = [44_100, 48_000, 32_000];
/// Kbit/s for bitrate indices 1..=14.
const BIT_RATES: [u32; 14] = [
    32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384,
];

const MODE_JOINT_STEREO: u32 = 1;
const MODE_MONO: u32 = 3;

/// The synthesis output for full scale, negated. kjmp2 divides by
/// `1.015625 * 2^30`, a gain that offsets its short requantisation step;
/// with the exact step it is `2^30`.
const OUTPUT_SCALE: f32 = -1_073_741_824.0;

const SCALEFACTOR_BASE: [i32; 3] = [0x0200_0000, 0x0196_5FEA, 0x0142_8A30];

/// Bitrate class per bitrate index: mono, then per channel of stereo.
const QUANT_LUT_STEP_1: [[u8; 14]; 2] = [
    [0, 0, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2, 2],
    [0, 0, 0, 0, 0, 0, 1, 1, 1, 2, 2, 2, 2, 2],
];

/// Table 3-B.2a (high rate, 27 subbands) to 3-B.2d, as `sblimit | table << 6`.
const TAB_A: u8 = 27 | 64;
const TAB_B: u8 = 30 | 64;
const TAB_C: u8 = 8;
const TAB_D: u8 = 12;

/// Bitrate class x sample rate: the allocation table and subband limit.
const QUANT_LUT_STEP_2: [[u8; 3]; 3] = [
    [TAB_C, TAB_C, TAB_D],
    [TAB_A, TAB_A, TAB_A],
    [TAB_B, TAB_A, TAB_B],
];

/// Allocation table x subband: `nbal << 4 | row`.
const QUANT_LUT_STEP_3: [[u8; 32]; 2] = [
    [
        0x44, 0x44, 0x34, 0x34, 0x34, 0x34, 0x34, 0x34, 0x34, 0x34, 0x34, 0x34, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ],
    [
        0x43, 0x43, 0x43, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x31, 0x31, 0x31, 0x31,
        0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
        0, 0,
    ],
];

/// Table row x allocation code: the quantiser (1-based into [`QUANT_TAB`],
/// 0 for none).
const QUANT_LUT_STEP_4: [[u8; 16]; 5] = [
    [0, 1, 2, 17, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 1, 2, 3, 4, 5, 6, 17, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 17],
    [0, 1, 3, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17],
    [0, 1, 2, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 17],
];

/// One quantiser: its level count, whether three samples share one
/// codeword, and the codeword width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Quantizer {
    levels: u16,
    grouped: bool,
    bits: u8,
}

const fn q(levels: u16, grouped: bool, bits: u8) -> Quantizer {
    Quantizer {
        levels,
        grouped,
        bits,
    }
}

const QUANT_TAB: [Quantizer; 17] = [
    q(3, true, 5),
    q(5, true, 7),
    q(7, false, 3),
    q(9, true, 10),
    q(15, false, 4),
    q(31, false, 5),
    q(63, false, 6),
    q(127, false, 7),
    q(255, false, 8),
    q(511, false, 9),
    q(1023, false, 10),
    q(2047, false, 11),
    q(4095, false, 12),
    q(8191, false, 13),
    q(16383, false, 14),
    q(32767, false, 15),
    q(65535, false, 16),
];

/// One frame header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub sample_rate: u32,
    sample_rate_index: usize,
    bitrate_index: usize,
    mode: u32,
    mode_extension: u32,
    has_crc: bool,
    /// The whole frame's length in bytes, header included.
    pub frame_len: usize,
}

impl Header {
    /// Parse the four header bytes at the start of `data`; `None` unless
    /// they are an MPEG-1 Layer II header with a fixed bitrate.
    pub fn parse(data: &[u8]) -> Option<Self> {
        let bytes: [u8; 4] = data.get(..4)?.try_into().ok()?;
        // Sync (12 bits), MPEG-1 (1), Layer II (`10`).
        if bytes[0] != 0xFF || bytes[1] & 0xFE != 0xFC {
            return None;
        }
        let has_crc = bytes[1] & 1 == 0;
        let bitrate_code = usize::from(bytes[2] >> 4);
        let sample_rate_index = usize::from(bytes[2] >> 2 & 3);
        if !(1..=14).contains(&bitrate_code) || sample_rate_index == 3 {
            return None;
        }
        let bitrate_index = bitrate_code - 1;
        let padding = usize::from(bytes[2] >> 1 & 1);
        let sample_rate = SAMPLE_RATES[sample_rate_index];
        let frame_len = (144_000 * BIT_RATES[bitrate_index] / sample_rate) as usize + padding;
        Some(Header {
            sample_rate,
            sample_rate_index,
            bitrate_index,
            mode: u32::from(bytes[3] >> 6),
            mode_extension: u32::from(bytes[3] >> 4 & 3),
            has_crc,
            frame_len,
        })
    }
}

/// The frames of a Layer II elementary stream, as `(offset, header)`.
///
/// A frame is accepted where a header parses and, unless it is the last
/// frame, the next frame's header follows it exactly; anything else is
/// skipped a byte at a time until the stream resynchronises.
pub fn frames(data: &[u8]) -> impl Iterator<Item = (usize, Header)> + '_ {
    let mut at = 0;
    std::iter::from_fn(move || {
        while at + 4 <= data.len() {
            if let Some(header) = Header::parse(&data[at..]) {
                let end = at + header.frame_len;
                let next_ok = end + 4 > data.len()
                    || Header::parse(&data[end..])
                        .is_some_and(|next| next.sample_rate == header.sample_rate);
                if end <= data.len() && next_ok {
                    let frame = (at, header);
                    at = end;
                    return Some(frame);
                }
            }
            at += 1;
        }
        None
    })
}

/// The matrixing coefficients `N[i][k]`.
fn matrix() -> &'static [[f32; 32]; 64] {
    static MATRIX: OnceLock<Box<[[f32; 32]; 64]>> = OnceLock::new();
    MATRIX.get_or_init(|| {
        let mut n = Box::new([[0f32; 32]; 64]);
        for (i, row) in n.iter_mut().enumerate() {
            for (k, value) in row.iter_mut().enumerate() {
                let angle = ((16 + i) * (2 * k + 1)) as f64 * std::f64::consts::PI / 64.0;
                *value = angle.cos() as f32;
            }
        }
        n
    })
}

/// The decoder state that carries across frames: the synthesis filter
/// history.
pub struct Decoder {
    v: Box<[[f32; 1024]; 2]>,
    v_pos: usize,
    allocation: [[Option<Quantizer>; 32]; 2],
    scale_factor_info: [[u8; 32]; 2],
    scale_factor: [[[i32; 3]; 32]; 2],
    sample: [[[i32; 3]; 32]; 2],
}

impl Default for Decoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Decoder {
    pub fn new() -> Self {
        Decoder {
            v: Box::new([[0.0; 1024]; 2]),
            v_pos: 0,
            allocation: [[None; 32]; 2],
            scale_factor_info: [[0; 32]; 2],
            scale_factor: [[[0; 3]; 32]; 2],
            sample: [[[0; 3]; 32]; 2],
        }
    }

    /// Decode one frame (`frame` starts at its header) and append its
    /// [`SAMPLES_PER_FRAME`] interleaved stereo samples (-1.0..=1.0) to
    /// `out`; a mono frame is duplicated to both channels.
    pub fn decode(&mut self, frame: &[u8], out: &mut Vec<f32>) -> Result<Header> {
        let Some(header) = Header::parse(frame) else {
            bail!("not an MPEG-1 Layer II frame header");
        };
        if frame.len() < header.frame_len {
            bail!("truncated Layer II frame");
        }
        let mut bits = Bits::new(&frame[..header.frame_len]);
        bits.skip(32 + if header.has_crc { 16 } else { 0 });

        let mono = header.mode == MODE_MONO;
        let channels = if mono { 1 } else { 2 };
        let class = QUANT_LUT_STEP_1[usize::from(!mono)][header.bitrate_index];
        let table = QUANT_LUT_STEP_2[usize::from(class)][header.sample_rate_index];
        let sblimit = usize::from(table & 63);
        let table = usize::from(table >> 6);
        let bound = match header.mode {
            MODE_JOINT_STEREO => ((header.mode_extension as usize + 1) << 2).min(sblimit),
            MODE_MONO => 0,
            _ => sblimit,
        };

        let read_allocation = |bits: &mut Bits<'_>, subband: usize| {
            let entry = QUANT_LUT_STEP_3[table][subband];
            let code = bits.read(u32::from(entry >> 4)) as usize;
            match QUANT_LUT_STEP_4[usize::from(entry & 15)][code] {
                0 => None,
                index => Some(QUANT_TAB[usize::from(index) - 1]),
            }
        };
        for subband in 0..bound {
            self.allocation[0][subband] = read_allocation(&mut bits, subband);
            self.allocation[1][subband] = read_allocation(&mut bits, subband);
        }
        for subband in bound..sblimit {
            let allocation = read_allocation(&mut bits, subband);
            self.allocation[0][subband] = allocation;
            self.allocation[1][subband] = allocation;
        }

        for subband in 0..sblimit {
            for channel in 0..channels {
                if self.allocation[channel][subband].is_some() {
                    self.scale_factor_info[channel][subband] = bits.read(2) as u8;
                }
            }
            if mono {
                self.scale_factor_info[1][subband] = self.scale_factor_info[0][subband];
            }
        }
        for subband in 0..sblimit {
            for channel in 0..channels {
                if self.allocation[channel][subband].is_none() {
                    continue;
                }
                let factors = &mut self.scale_factor[channel][subband];
                match self.scale_factor_info[channel][subband] {
                    0 => {
                        factors[0] = bits.read(6) as i32;
                        factors[1] = bits.read(6) as i32;
                        factors[2] = bits.read(6) as i32;
                    }
                    1 => {
                        factors[0] = bits.read(6) as i32;
                        factors[1] = factors[0];
                        factors[2] = bits.read(6) as i32;
                    }
                    2 => {
                        factors[0] = bits.read(6) as i32;
                        factors[1] = factors[0];
                        factors[2] = factors[0];
                    }
                    _ => {
                        factors[0] = bits.read(6) as i32;
                        factors[1] = bits.read(6) as i32;
                        factors[2] = factors[1];
                    }
                }
            }
            if mono {
                self.scale_factor[1][subband] = self.scale_factor[0][subband];
            }
        }

        let start = out.len();
        out.resize(start + SAMPLES_PER_FRAME * 2, 0.0);
        let mut out_pos = 0;
        for part in 0..3 {
            for _granule in 0..4 {
                for subband in 0..bound {
                    self.read_samples(&mut bits, 0, subband, part);
                    self.read_samples(&mut bits, 1, subband, part);
                }
                for subband in bound..sblimit {
                    self.read_samples(&mut bits, 0, subband, part);
                    self.sample[1][subband] = self.sample[0][subband];
                }
                for subband in sblimit..32 {
                    self.sample[0][subband] = [0; 3];
                    self.sample[1][subband] = [0; 3];
                }
                for step in 0..3 {
                    self.v_pos = (self.v_pos + 1024 - 64) & 1023;
                    for channel in 0..2 {
                        let samples = self.synthesize(channel, step);
                        for (j, value) in samples.iter().enumerate() {
                            out[start + ((out_pos + j) << 1) + channel] = value / OUTPUT_SCALE;
                        }
                    }
                    out_pos += 32;
                }
            }
        }
        if bits.overrun() {
            bail!("Layer II frame data runs past the frame");
        }
        Ok(header)
    }

    /// Read and requantise subband `subband`'s three samples of `part`.
    fn read_samples(&mut self, bits: &mut Bits<'_>, channel: usize, subband: usize, part: usize) {
        let Some(quantizer) = self.allocation[channel][subband] else {
            self.sample[channel][subband] = [0; 3];
            return;
        };
        let factor = self.scale_factor[channel][subband][part];
        let factor = if factor == 63 {
            0
        } else {
            let shift = factor / 3;
            (SCALEFACTOR_BASE[(factor % 3) as usize] + ((1 << shift) >> 1)) >> shift
        };
        let levels = i32::from(quantizer.levels);
        let mut samples = [0i32; 3];
        if quantizer.grouped {
            let mut value = bits.read(u32::from(quantizer.bits)) as i32;
            samples[0] = value % levels;
            value /= levels;
            samples[1] = value % levels;
            samples[2] = value / levels;
        } else {
            for sample in &mut samples {
                *sample = bits.read(u32::from(quantizer.bits)) as i32;
            }
        }
        for sample in &mut samples {
            // The standard's fraction `(2s - (levels - 1)) / levels` in units
            // of 2^-15, negated as kjmp2's synthesis expects. kjmp2's
            // `65536 / (levels + 1)` step is 25% short at three levels.
            let value = (i64::from(levels - 1 - 2 * *sample) * 32768 / i64::from(levels)) as i32;
            *sample = (value * (factor >> 12) + ((value * (factor & 4095) + 2048) >> 12)) >> 12;
        }
        self.sample[channel][subband] = samples;
    }

    /// Matrix the 32 subband samples of `step` into the filter history and
    /// window it into 32 output samples.
    fn synthesize(&mut self, channel: usize, step: usize) -> [f32; 32] {
        let n = matrix();
        let samples: [f32; 32] = std::array::from_fn(|k| self.sample[channel][k][step] as f32);
        let v = &mut self.v[channel];
        for (i, row) in n.iter().enumerate() {
            let mut sum = 0.0f32;
            for (coefficient, sample) in row.iter().zip(&samples) {
                sum += coefficient * sample;
            }
            v[self.v_pos + i] = sum;
        }

        // The window is read as two copies of the 512-tap table back to back.
        let window = |index: usize| SYNTHESIS_WINDOW[index & 511];
        let mut u = [0f32; 32];
        let mut d_index = 512 - (self.v_pos >> 1);
        let mut v_index = (self.v_pos % 128) >> 1;
        while v_index < 1024 {
            for value in &mut u {
                *value += window(d_index) * v[v_index];
                d_index += 1;
                v_index += 1;
            }
            v_index += 128 - 32;
            d_index += 64 - 32;
        }
        d_index -= 512 - 32;
        v_index = (128 - 32 + 1024) - v_index;
        while v_index < 1024 {
            for value in &mut u {
                *value += window(d_index) * v[v_index];
                d_index += 1;
                v_index += 1;
            }
            v_index += 128 - 32;
            d_index += 64 - 32;
        }
        u
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 192 kbit/s 44.1 kHz stereo frame with no bits allocated: silence.
    fn silent_frame() -> Vec<u8> {
        let header = Header::parse(&[0xFF, 0xFD, 0xA0, 0x00]).unwrap();
        let mut frame = vec![0u8; header.frame_len];
        frame[..4].copy_from_slice(&[0xFF, 0xFD, 0xA0, 0x00]);
        frame
    }

    #[test]
    fn parses_the_film_header() {
        let header = Header::parse(&[0xFF, 0xFD, 0xA0, 0x04]).unwrap();
        assert_eq!(header.sample_rate, 44_100);
        assert_eq!(header.frame_len, 626);
        assert!(!header.has_crc);
        // Padded, with CRC.
        let header = Header::parse(&[0xFF, 0xFC, 0xA2, 0x04]).unwrap();
        assert_eq!(header.frame_len, 627);
        assert!(header.has_crc);
        // Layer III, free format and the reserved rate are refused.
        assert!(Header::parse(&[0xFF, 0xFB, 0x90, 0x04]).is_none());
        assert!(Header::parse(&[0xFF, 0xFD, 0x00, 0x04]).is_none());
        assert!(Header::parse(&[0xFF, 0xFD, 0x9C, 0x04]).is_none());
    }

    #[test]
    fn frames_resynchronise_past_garbage() {
        let frame = silent_frame();
        let mut stream = vec![0xFF, 0x00, 0x12];
        stream.extend_from_slice(&frame);
        stream.extend_from_slice(&frame);
        let found: Vec<usize> = frames(&stream).map(|(at, _)| at).collect();
        assert_eq!(found, [3, 3 + frame.len()]);
    }

    #[test]
    fn an_empty_allocation_decodes_to_silence() {
        let mut decoder = Decoder::new();
        let mut out = Vec::new();
        decoder.decode(&silent_frame(), &mut out).unwrap();
        assert_eq!(out.len(), SAMPLES_PER_FRAME * 2);
        assert!(out.iter().all(|&sample| sample == 0.0));
    }

    #[test]
    fn matrixing_matches_the_standard_symmetries() {
        let n = matrix();
        // Row 16 is cos((2k + 1)π/2) = 0 and row 48 is cos((2k + 1)π) = -1.
        assert!(n[16].iter().all(|value| value.abs() < 1e-6));
        assert!(n[48].iter().all(|value| (value + 1.0).abs() < 1e-6));
    }
}
