//! The MPEG-1 video decoder (ISO/IEC 11172-2): I, P and B pictures, 4:2:0,
//! half-pel motion compensation and the reference reorder that turns decode
//! order into display order.
//!
//! The structure follows pl_mpeg (itself after Zoltan Korandi's Java
//! decoder): VLC trees walked a bit at a time, coefficients dequantised and
//! premultiplied for its integer AAN-style IDCT, and three frame buffers
//! rotated so a B picture predicts from the two most recent references.
//! Departures: a damaged slice is an error the picture survives (its
//! macroblocks keep the previous contents) instead of a silent stop, a
//! zero coefficient stays zero through the oddification (`Sign(0) = 0`, as
//! the standard has it), and a motion vector that reaches past the reference
//! picture reads its edge pixels instead of whatever memory follows.

use anyhow::{Context, Result, bail};

use super::bits::Bits;
use super::tables::{
    ADDRESS_INCREMENT, CODED_BLOCK_PATTERN, DC_SIZE_CHROMINANCE, DC_SIZE_LUMINANCE, DCT_COEFF,
    ESCAPE, INTRA_QUANT_MATRIX, MOTION, PREMULTIPLIER, TYPE_B, TYPE_INTRA, TYPE_PREDICTIVE, Vlc,
    ZIG_ZAG,
};

/// The sequence header start code.
pub const SEQUENCE_HEADER: u8 = 0xB3;
/// The picture start code.
pub const PICTURE: u8 = 0x00;
const EXTENSION: u8 = 0xB5;
const USER_DATA: u8 = 0xB2;

const PICTURE_I: u8 = 1;
const PICTURE_P: u8 = 2;
const PICTURE_B: u8 = 3;

/// The macroblock type flags.
const MB_INTRA: i32 = 0x01;
const MB_PATTERN: i32 = 0x02;
const MB_BACKWARD: i32 = 0x04;
const MB_FORWARD: i32 = 0x08;
const MB_QUANT: i32 = 0x10;

/// The address increment tree's stuffing and escape leaves.
const STUFFING: i32 = 34;
const ESCAPE_33: i32 = 35;

/// Pictures larger than this in either dimension are refused (the RE2 films
/// are 640x480; 4095 is the field's limit).
const MAX_DIMENSION: usize = 4096;

/// The frame rates of `frame_rate_code` 1..=8, as exact fractions.
const FRAME_RATES: [(u32, u32); 8] = [
    (24_000, 1001),
    (24, 1),
    (25, 1),
    (30_000, 1001),
    (30, 1),
    (50, 1),
    (60_000, 1001),
    (60, 1),
];

/// A sequence header's picture size, rate and quantiser matrices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceHeader {
    pub width: usize,
    pub height: usize,
    /// Frames per second as `(num, den)`.
    pub frame_rate: (u32, u32),
    intra_matrix: [u8; 64],
    non_intra_matrix: [u8; 64],
}

impl SequenceHeader {
    /// Read a sequence header; `bits` sits just past its start code.
    pub fn read(bits: &mut Bits<'_>) -> Result<Self> {
        let width = bits.read(12) as usize;
        let height = bits.read(12) as usize;
        if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
            bail!("unsupported picture size {width}x{height}");
        }
        let _aspect = bits.read(4);
        let rate_code = bits.read(4) as usize;
        let frame_rate = *rate_code
            .checked_sub(1)
            .and_then(|index| FRAME_RATES.get(index))
            .with_context(|| format!("unknown frame rate code {rate_code}"))?;
        // bit_rate, marker, vbv_buffer_size, constrained_parameters_flag
        bits.skip(18 + 1 + 10 + 1);
        let mut intra_matrix = INTRA_QUANT_MATRIX;
        if bits.flag() {
            for &index in &ZIG_ZAG {
                intra_matrix[usize::from(index)] = bits.read(8) as u8;
            }
        }
        let mut non_intra_matrix = [16u8; 64];
        if bits.flag() {
            for &index in &ZIG_ZAG {
                non_intra_matrix[usize::from(index)] = bits.read(8) as u8;
            }
        }
        if bits.overrun() {
            bail!("truncated sequence header");
        }
        Ok(SequenceHeader {
            width,
            height,
            frame_rate,
            intra_matrix,
            non_intra_matrix,
        })
    }
}

/// One decoded picture: three planes padded to whole macroblocks.
#[derive(Debug, Clone)]
pub struct Frame {
    /// Luma, `luma_stride` bytes per row.
    pub y: Vec<u8>,
    /// Blue-difference chroma, `chroma_stride` bytes per row.
    pub cb: Vec<u8>,
    /// Red-difference chroma, `chroma_stride` bytes per row.
    pub cr: Vec<u8>,
    pub luma_stride: usize,
    pub chroma_stride: usize,
}

impl Frame {
    fn new(mb_width: usize, mb_height: usize) -> Self {
        let luma = mb_width * 16 * mb_height * 16;
        Frame {
            y: vec![16; luma],
            cb: vec![128; luma / 4],
            cr: vec![128; luma / 4],
            luma_stride: mb_width * 16,
            chroma_stride: mb_width * 8,
        }
    }
}

/// One direction's motion vector state.
#[derive(Debug, Clone, Copy, Default)]
struct Motion {
    full_pel: bool,
    r_size: u32,
    h: i32,
    v: i32,
    is_set: bool,
}

/// The decoder: the sequence parameters, the three frame buffers and the
/// slice/macroblock state.
pub struct Decoder {
    width: usize,
    height: usize,
    mb_width: usize,
    mb_height: usize,
    intra_matrix: [u8; 64],
    non_intra_matrix: [u8; 64],
    /// The picture being decoded (after a B picture, the one shown).
    current: Frame,
    /// The older reference.
    forward: Frame,
    /// The newer reference.
    backward: Frame,
    picture_type: u8,
    forward_motion: Motion,
    backward_motion: Motion,
    quantizer_scale: i32,
    slice_begin: bool,
    address: i32,
    mb_row: usize,
    mb_col: usize,
    mb_type: i32,
    intra: bool,
    dc_predictor: [i32; 3],
    block: [i32; 64],
    /// A reference picture is waiting to be shown.
    has_reference: bool,
    /// Slices that failed to decode so far.
    damaged_slices: usize,
    /// The buffer due for display after the last decode.
    shown: Option<Shown>,
}

/// Which frame buffer is due for display.
#[derive(Clone, Copy)]
enum Shown {
    Current,
    Forward,
    Backward,
}

impl Decoder {
    /// A decoder for the stream `header` opens.
    pub fn new(header: &SequenceHeader) -> Self {
        let mb_width = header.width.div_ceil(16);
        let mb_height = header.height.div_ceil(16);
        Decoder {
            width: header.width,
            height: header.height,
            mb_width,
            mb_height,
            intra_matrix: header.intra_matrix,
            non_intra_matrix: header.non_intra_matrix,
            current: Frame::new(mb_width, mb_height),
            forward: Frame::new(mb_width, mb_height),
            backward: Frame::new(mb_width, mb_height),
            picture_type: 0,
            forward_motion: Motion::default(),
            backward_motion: Motion::default(),
            quantizer_scale: 0,
            slice_begin: false,
            address: 0,
            mb_row: 0,
            mb_col: 0,
            mb_type: 0,
            intra: false,
            dc_predictor: [128; 3],
            block: [0; 64],
            has_reference: false,
            damaged_slices: 0,
            shown: None,
        }
    }

    /// The picture width in pixels.
    pub fn width(&self) -> usize {
        self.width
    }

    /// The picture height in pixels.
    pub fn height(&self) -> usize {
        self.height
    }

    /// Slices that failed to decode so far.
    pub fn damaged_slices(&self) -> usize {
        self.damaged_slices
    }

    /// Decode one picture unit: the bytes from a picture start code up to
    /// the next one, with any sequence header that follows the picture's
    /// slices. Returns whether a picture is now due for display
    /// ([`Self::last_shown`]); a reference picture is shown one picture
    /// late, when the next reference arrives or at [`Self::flush`].
    pub fn decode(&mut self, unit: &[u8]) -> Result<bool> {
        self.shown = None;
        let mut bits = Bits::new(unit);
        if bits.next_start_code() != Some(PICTURE) {
            bail!("picture unit does not start with a picture start code");
        }
        let (picture_type, mut code) = self.decode_picture(&mut bits)?;
        self.shown = match picture_type {
            PICTURE_B => Some(Shown::Current),
            _ if self.has_reference => Some(Shown::Forward),
            _ => {
                self.has_reference = true;
                None
            }
        };
        // A sequence header after the slices applies to the next picture.
        while let Some(start) = code {
            if start == SEQUENCE_HEADER {
                let header = SequenceHeader::read(&mut bits)?;
                if (header.width, header.height) != (self.width, self.height) {
                    bail!(
                        "picture size changes mid-stream ({}x{} to {}x{})",
                        self.width,
                        self.height,
                        header.width,
                        header.height
                    );
                }
                self.intra_matrix = header.intra_matrix;
                self.non_intra_matrix = header.non_intra_matrix;
            }
            code = bits.next_start_code();
        }
        Ok(self.shown.is_some())
    }

    /// The picture the last [`Self::decode`] or [`Self::flush`] made due.
    pub fn last_shown(&self) -> Option<&Frame> {
        Some(match self.shown? {
            Shown::Current => &self.current,
            Shown::Forward => &self.forward,
            Shown::Backward => &self.backward,
        })
    }

    /// The last reference picture, once the stream has ended.
    pub fn flush(&mut self) -> Option<&Frame> {
        self.shown = std::mem::take(&mut self.has_reference).then_some(Shown::Backward);
        self.last_shown()
    }

    /// Decode the picture header and its slices. Returns the picture type
    /// and the first start code past the slices.
    fn decode_picture(&mut self, bits: &mut Bits<'_>) -> Result<(u8, Option<u8>)> {
        let _temporal_reference = bits.read(10);
        let picture_type = bits.read(3) as u8;
        let _vbv_delay = bits.read(16);
        if !(PICTURE_I..=PICTURE_B).contains(&picture_type) {
            bail!("unsupported picture coding type {picture_type}");
        }
        if picture_type != PICTURE_I {
            self.forward_motion.full_pel = bits.flag();
            let f_code = bits.read(3);
            if f_code == 0 {
                bail!("forward f_code of zero");
            }
            self.forward_motion.r_size = f_code - 1;
        }
        if picture_type == PICTURE_B {
            self.backward_motion.full_pel = bits.flag();
            let f_code = bits.read(3);
            if f_code == 0 {
                bail!("backward f_code of zero");
            }
            self.backward_motion.r_size = f_code - 1;
        }
        self.picture_type = picture_type;
        let reference = picture_type != PICTURE_B;
        if reference {
            // The newer reference becomes the one this picture predicts from.
            std::mem::swap(&mut self.forward, &mut self.backward);
        }

        let mut code = bits.next_start_code();
        while matches!(code, Some(EXTENSION | USER_DATA)) {
            code = bits.next_start_code();
        }
        let last = (self.mb_width * self.mb_height) as i32 - 1;
        while let Some(slice @ 0x01..=0xAF) = code {
            if self.decode_slice(bits, slice).is_err() {
                self.damaged_slices += 1;
            }
            code = bits.next_start_code();
            if self.address >= last {
                break;
            }
        }

        if reference {
            std::mem::swap(&mut self.backward, &mut self.current);
        }
        Ok((picture_type, code))
    }

    fn decode_slice(&mut self, bits: &mut Bits<'_>, slice: u8) -> Result<()> {
        let row = usize::from(slice) - 1;
        if row >= self.mb_height {
            bail!("slice {slice} is below the picture");
        }
        self.slice_begin = true;
        self.address = (row * self.mb_width) as i32 - 1;
        self.forward_motion.h = 0;
        self.forward_motion.v = 0;
        self.backward_motion.h = 0;
        self.backward_motion.v = 0;
        self.dc_predictor = [128; 3];
        self.quantizer_scale = bits.read(5) as i32;
        while bits.flag() {
            bits.skip(8);
        }
        let last = (self.mb_width * self.mb_height) as i32 - 1;
        loop {
            self.decode_macroblock(bits)?;
            if bits.overrun() {
                bail!("slice data runs past the picture");
            }
            if self.address >= last || bits.remaining() < 23 || bits.peek(23) == 0 {
                return Ok(());
            }
        }
    }

    fn read_vlc(bits: &mut Bits<'_>, table: &[Vlc], what: &str) -> Result<i32> {
        bits.vlc(table)
            .with_context(|| format!("invalid {what} code"))
    }

    fn decode_macroblock(&mut self, bits: &mut Bits<'_>) -> Result<()> {
        let mut increment = 0;
        let mut code = Self::read_vlc(bits, &ADDRESS_INCREMENT, "macroblock address")?;
        while code == STUFFING {
            code = Self::read_vlc(bits, &ADDRESS_INCREMENT, "macroblock address")?;
        }
        while code == ESCAPE_33 {
            increment += 33;
            code = Self::read_vlc(bits, &ADDRESS_INCREMENT, "macroblock address")?;
        }
        increment += code;

        let size = (self.mb_width * self.mb_height) as i32;
        if self.slice_begin {
            // A slice's first increment counts from the row start.
            self.slice_begin = false;
            self.address += increment;
        } else {
            if self.address + increment >= size {
                bail!("macroblock address past the picture");
            }
            if increment > 1 {
                // Skipped macroblocks reset the DC predictors, and in a P
                // picture the forward vector.
                self.dc_predictor = [128; 3];
                if self.picture_type == PICTURE_P {
                    self.forward_motion.h = 0;
                    self.forward_motion.v = 0;
                }
            }
            while increment > 1 {
                self.address += 1;
                self.set_position();
                self.predict_macroblock();
                increment -= 1;
            }
            self.address += 1;
        }
        if self.address < 0 || self.address >= size {
            bail!("macroblock address out of range");
        }
        self.set_position();

        let table: &[Vlc] = match self.picture_type {
            PICTURE_I => &TYPE_INTRA,
            PICTURE_P => &TYPE_PREDICTIVE,
            _ => &TYPE_B,
        };
        self.mb_type = Self::read_vlc(bits, table, "macroblock type")?;
        self.intra = self.mb_type & MB_INTRA != 0;
        self.forward_motion.is_set = self.mb_type & MB_FORWARD != 0;
        self.backward_motion.is_set = self.mb_type & MB_BACKWARD != 0;
        if self.mb_type & MB_QUANT != 0 {
            self.quantizer_scale = bits.read(5) as i32;
        }
        if self.intra {
            self.forward_motion.h = 0;
            self.forward_motion.v = 0;
            self.backward_motion.h = 0;
            self.backward_motion.v = 0;
        } else {
            self.dc_predictor = [128; 3];
            self.decode_motion_vectors(bits)?;
            self.predict_macroblock();
        }

        let pattern = if self.mb_type & MB_PATTERN != 0 {
            Self::read_vlc(bits, &CODED_BLOCK_PATTERN, "coded block pattern")?
        } else if self.intra {
            0x3F
        } else {
            0
        };
        for block in 0..6 {
            if pattern & (0x20 >> block) != 0 {
                self.decode_block(bits, block)?;
            }
        }
        Ok(())
    }

    fn set_position(&mut self) {
        let address = self.address as usize;
        self.mb_row = address / self.mb_width;
        self.mb_col = address % self.mb_width;
    }

    fn decode_motion_vectors(&mut self, bits: &mut Bits<'_>) -> Result<()> {
        if self.forward_motion.is_set {
            let r_size = self.forward_motion.r_size;
            self.forward_motion.h = decode_motion_vector(bits, r_size, self.forward_motion.h)?;
            self.forward_motion.v = decode_motion_vector(bits, r_size, self.forward_motion.v)?;
        } else if self.picture_type == PICTURE_P {
            // A P macroblock without a vector predicts with a zero one.
            self.forward_motion.h = 0;
            self.forward_motion.v = 0;
        }
        if self.backward_motion.is_set {
            let r_size = self.backward_motion.r_size;
            self.backward_motion.h = decode_motion_vector(bits, r_size, self.backward_motion.h)?;
            self.backward_motion.v = decode_motion_vector(bits, r_size, self.backward_motion.v)?;
        }
        Ok(())
    }

    /// Fill the current macroblock from its reference(s).
    fn predict_macroblock(&mut self) {
        let scale = |motion: &Motion| {
            if motion.full_pel {
                (motion.h << 1, motion.v << 1)
            } else {
                (motion.h, motion.v)
            }
        };
        let forward = scale(&self.forward_motion);
        if self.picture_type == PICTURE_B {
            let backward = scale(&self.backward_motion);
            if self.forward_motion.is_set {
                self.copy_from(Reference::Forward, forward, false);
                if self.backward_motion.is_set {
                    self.copy_from(Reference::Backward, backward, true);
                }
            } else {
                self.copy_from(Reference::Backward, backward, false);
            }
        } else {
            self.copy_from(Reference::Forward, forward, false);
        }
    }

    /// Predict the macroblock's three planes from `reference` with the
    /// half-pel luma vector `(h, v)`, averaging into the current contents
    /// when `interpolate`.
    fn copy_from(&mut self, reference: Reference, (h, v): (i32, i32), interpolate: bool) {
        let source = match reference {
            Reference::Forward => &self.forward,
            Reference::Backward => &self.backward,
        };
        let block = Placement {
            row: self.mb_row,
            col: self.mb_col,
            height: self.mb_height,
        };
        let luma = self.current.luma_stride;
        let chroma = self.current.chroma_stride;
        // Chroma vectors are the luma vector halved toward zero.
        predict(
            &source.y,
            &mut self.current.y,
            luma,
            block,
            (h, v),
            16,
            interpolate,
        );
        let half = (h / 2, v / 2);
        predict(
            &source.cr,
            &mut self.current.cr,
            chroma,
            block,
            half,
            8,
            interpolate,
        );
        predict(
            &source.cb,
            &mut self.current.cb,
            chroma,
            block,
            half,
            8,
            interpolate,
        );
    }

    fn decode_block(&mut self, bits: &mut Bits<'_>, block: usize) -> Result<()> {
        let mut n = 0usize;
        if self.intra {
            let plane = block.saturating_sub(3);
            let table: &[Vlc] = if plane == 0 {
                &DC_SIZE_LUMINANCE
            } else {
                &DC_SIZE_CHROMINANCE
            };
            let size = Self::read_vlc(bits, table, "DC size")? as u32;
            let predictor = self.dc_predictor[plane];
            let dc = if size > 0 {
                let differential = bits.read(size) as i32;
                if differential & (1 << (size - 1)) != 0 {
                    predictor + differential
                } else {
                    predictor + ((-1i32 << size) | (differential + 1))
                }
            } else {
                predictor
            };
            self.dc_predictor[plane] = dc;
            // Dequantised (x8) and premultiplied (x32).
            self.block[0] = dc << 8;
            n = 1;
        }

        let matrix = if self.intra {
            &self.intra_matrix
        } else {
            &self.non_intra_matrix
        };
        loop {
            let coefficient = Self::read_vlc(bits, &DCT_COEFF, "DCT coefficient")?;
            // `10` after the first coefficient is end_of_block; `1s` as the
            // first one of a non-intra block is run 0, level 1.
            if coefficient == 0x0001 && n > 0 && !bits.flag() {
                break;
            }
            let (run, level) = if coefficient == ESCAPE {
                let run = bits.read(6) as usize;
                let level = match bits.read(8) as i32 {
                    0 => bits.read(8) as i32,
                    128 => bits.read(8) as i32 - 256,
                    level if level > 128 => level - 256,
                    level => level,
                };
                (run, level)
            } else {
                let level = coefficient & 0xFF;
                let level = if bits.flag() { -level } else { level };
                ((coefficient >> 8) as usize, level)
            };
            n += run;
            if n >= 64 {
                self.block = [0; 64];
                bail!("DCT coefficient past the end of the block");
            }
            let index = usize::from(ZIG_ZAG[n]);
            n += 1;
            let mut level = level << 1;
            if !self.intra {
                level += level.signum();
            }
            level = (level * self.quantizer_scale * i32::from(matrix[index])) >> 4;
            if level & 1 == 0 {
                level -= level.signum();
            }
            let level = level.clamp(-2048, 2047);
            self.block[index] = level * i32::from(PREMULTIPLIER[index]);
            if bits.overrun() {
                self.block = [0; 64];
                bail!("block data runs past the picture");
            }
        }

        let (plane, stride, at) = if block < 4 {
            let stride = self.current.luma_stride;
            let mut at = (self.mb_row * stride + self.mb_col) * 16;
            if block & 1 != 0 {
                at += 8;
            }
            if block & 2 != 0 {
                at += stride * 8;
            }
            (&mut self.current.y, stride, at)
        } else {
            let stride = self.current.chroma_stride;
            let at = (self.mb_row * stride + self.mb_col) * 8;
            let plane = if block == 4 {
                &mut self.current.cb
            } else {
                &mut self.current.cr
            };
            (plane, stride, at)
        };
        let clamp = |value: i32| value.clamp(0, 255) as u8;
        if n == 1 {
            // Only the DC term: a flat block.
            let value = (self.block[0] + 128) >> 8;
            self.block[0] = 0;
            for row in plane[at..].chunks_mut(stride).take(8) {
                for pixel in &mut row[..8] {
                    *pixel = if self.intra {
                        clamp(value)
                    } else {
                        clamp(i32::from(*pixel) + value)
                    };
                }
            }
        } else {
            idct(&mut self.block);
            for (row, values) in plane[at..]
                .chunks_mut(stride)
                .zip(self.block.as_chunks::<8>().0)
            {
                for (pixel, &value) in row[..8].iter_mut().zip(values) {
                    *pixel = if self.intra {
                        clamp(value)
                    } else {
                        clamp(i32::from(*pixel) + value)
                    };
                }
            }
            self.block = [0; 64];
        }
        Ok(())
    }
}

/// Which reference picture a prediction reads.
#[derive(Clone, Copy)]
enum Reference {
    Forward,
    Backward,
}

/// The macroblock a prediction writes, and the picture height in
/// macroblocks.
#[derive(Clone, Copy)]
struct Placement {
    row: usize,
    col: usize,
    height: usize,
}

/// Decode one motion vector component against its predictor `motion`.
fn decode_motion_vector(bits: &mut Bits<'_>, r_size: u32, motion: i32) -> Result<i32> {
    let fscale = 1i32 << r_size;
    let code = Decoder::read_vlc(bits, &MOTION, "motion vector")?;
    let delta = if code != 0 && fscale != 1 {
        let residual = bits.read(r_size) as i32;
        let delta = ((code.abs() - 1) << r_size) + residual + 1;
        if code < 0 { -delta } else { delta }
    } else {
        code
    };
    let mut motion = motion + delta;
    if motion > (fscale << 4) - 1 {
        motion -= fscale << 5;
    } else if motion < -(fscale << 4) {
        motion += fscale << 5;
    }
    Ok(motion)
}

/// Predict one `size`-square block of a plane `stride` wide from `source`
/// with the half-pel vector `(h, v)`.
fn predict(
    source: &[u8],
    dest: &mut [u8],
    stride: usize,
    block: Placement,
    (h, v): (i32, i32),
    size: usize,
    interpolate: bool,
) {
    let rows = block.height * size;
    let x = (block.col * size) as i32 + (h >> 1);
    let y = (block.row * size) as i32 + (v >> 1);
    let odd_h = h & 1 != 0;
    let odd_v = v & 1 != 0;
    let at = block.row * size * stride + block.col * size;
    let inside = x >= 0
        && y >= 0
        && x as usize + size + usize::from(odd_h) <= stride
        && y as usize + size + usize::from(odd_v) <= rows;
    if !inside {
        predict_clamped(
            source,
            dest,
            stride,
            rows,
            at,
            (x, y),
            (odd_h, odd_v),
            size,
            interpolate,
        );
        return;
    }
    let from = y as usize * stride + x as usize;
    match (odd_h, odd_v, interpolate) {
        (false, false, false) => copy::<false, false, false>(source, dest, stride, from, at, size),
        (false, true, false) => copy::<false, true, false>(source, dest, stride, from, at, size),
        (true, false, false) => copy::<true, false, false>(source, dest, stride, from, at, size),
        (true, true, false) => copy::<true, true, false>(source, dest, stride, from, at, size),
        (false, false, true) => copy::<false, false, true>(source, dest, stride, from, at, size),
        (false, true, true) => copy::<false, true, true>(source, dest, stride, from, at, size),
        (true, false, true) => copy::<true, false, true>(source, dest, stride, from, at, size),
        (true, true, true) => copy::<true, true, true>(source, dest, stride, from, at, size),
    }
}

/// The in-bounds prediction loop, specialised per half-pel case.
#[inline(always)]
fn copy<const ODD_H: bool, const ODD_V: bool, const INTERPOLATE: bool>(
    source: &[u8],
    dest: &mut [u8],
    stride: usize,
    from: usize,
    at: usize,
    size: usize,
) {
    for row in 0..size {
        let s = &source[from + row * stride..];
        let d = &mut dest[at + row * stride..at + row * stride + size];
        for (column, out) in d.iter_mut().enumerate() {
            let a = u32::from(s[column]);
            let value = match (ODD_H, ODD_V) {
                (false, false) => a,
                (false, true) => (a + u32::from(s[column + stride]) + 1) >> 1,
                (true, false) => (a + u32::from(s[column + 1]) + 1) >> 1,
                (true, true) => {
                    (a + u32::from(s[column + 1])
                        + u32::from(s[column + stride])
                        + u32::from(s[column + stride + 1])
                        + 2)
                        >> 2
                }
            };
            *out = if INTERPOLATE {
                ((u32::from(*out) + value + 1) >> 1) as u8
            } else {
                value as u8
            };
        }
    }
}

/// The prediction loop for a vector reaching past the reference: every
/// sample is read through clamped coordinates.
#[allow(clippy::too_many_arguments)]
fn predict_clamped(
    source: &[u8],
    dest: &mut [u8],
    stride: usize,
    rows: usize,
    at: usize,
    (x, y): (i32, i32),
    (odd_h, odd_v): (bool, bool),
    size: usize,
    interpolate: bool,
) {
    let sample = |sx: i32, sy: i32| {
        let sx = sx.clamp(0, stride as i32 - 1) as usize;
        let sy = sy.clamp(0, rows as i32 - 1) as usize;
        u32::from(source[sy * stride + sx])
    };
    for row in 0..size {
        for column in 0..size {
            let (sx, sy) = (x + column as i32, y + row as i32);
            let a = sample(sx, sy);
            let value = match (odd_h, odd_v) {
                (false, false) => a,
                (false, true) => (a + sample(sx, sy + 1) + 1) >> 1,
                (true, false) => (a + sample(sx + 1, sy) + 1) >> 1,
                (true, true) => {
                    (a + sample(sx + 1, sy) + sample(sx, sy + 1) + sample(sx + 1, sy + 1) + 2) >> 2
                }
            };
            let out = &mut dest[at + row * stride + column];
            *out = if interpolate {
                ((u32::from(*out) + value + 1) >> 1) as u8
            } else {
                value as u8
            };
        }
    }
}

/// The inverse DCT of a dequantised, premultiplied block, in place, to
/// pixel values (pl_mpeg's AAN-style integer transform).
fn idct(block: &mut [i32; 64]) {
    for column in 0..8 {
        let out = idct8(|k| block[k * 8 + column]);
        for (k, value) in out.into_iter().enumerate() {
            block[k * 8 + column] = value;
        }
    }
    for row in (0..64).step_by(8) {
        let out = idct8(|k| block[row + k]);
        for (k, value) in out.into_iter().enumerate() {
            block[row + k] = (value + 128) >> 8;
        }
    }
}

/// One 8-point pass of [`idct`] over the inputs `at(0..8)`.
#[inline(always)]
fn idct8(at: impl Fn(usize) -> i32) -> [i32; 8] {
    let b1 = at(4);
    let b3 = at(2) + at(6);
    let b4 = at(5) - at(3);
    let tmp1 = at(1) + at(7);
    let tmp2 = at(3) + at(5);
    let b6 = at(1) - at(7);
    let b7 = tmp1 + tmp2;
    let m0 = at(0);
    let x4 = ((b6 * 473 - b4 * 196 + 128) >> 8) - b7;
    let x0 = x4 - (((tmp1 - tmp2) * 362 + 128) >> 8);
    let x1 = m0 - b1;
    let x2 = (((at(2) - at(6)) * 362 + 128) >> 8) - b3;
    let x3 = m0 + b1;
    let y3 = x1 + x2;
    let y4 = x3 + b3;
    let y5 = x1 - x2;
    let y6 = x3 - b3;
    let y7 = -x0 - ((b4 * 473 + b6 * 196 + 128) >> 8);
    [
        b7 + y4,
        x4 + y3,
        y5 - x0,
        y6 - y7,
        y6 + y7,
        x0 + y5,
        y3 - x4,
        y4 - b7,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every leaf of a VLC tree, as `(code length, value)`, walking every
    /// path; panics on a cycle.
    fn leaves(table: &[Vlc]) -> Vec<(usize, i32)> {
        fn walk(table: &[Vlc], node: usize, depth: usize, out: &mut Vec<(usize, i32)>) {
            assert!(depth <= 16, "VLC tree deeper than 16 bits");
            for bit in 0..2 {
                match table[node + bit] {
                    (0, value) => out.push((depth + 1, value)),
                    (next, _) if next > 0 => walk(table, next as usize, depth + 1, out),
                    _ => {}
                }
            }
        }
        let mut out = Vec::new();
        walk(table, 0, 0, &mut out);
        out
    }

    #[test]
    fn vlc_trees_reach_every_value_once() {
        let mut increments: Vec<_> = leaves(&ADDRESS_INCREMENT).iter().map(|l| l.1).collect();
        increments.sort();
        assert_eq!(increments, (1..=35).collect::<Vec<_>>());
        let mut patterns: Vec<_> = leaves(&CODED_BLOCK_PATTERN).iter().map(|l| l.1).collect();
        patterns.sort();
        assert_eq!(patterns, (1..=63).collect::<Vec<_>>());
        let mut motion: Vec<_> = leaves(&MOTION).iter().map(|l| l.1).collect();
        motion.sort();
        assert_eq!(motion, (-16..=16).collect::<Vec<_>>());
        for table in [&DC_SIZE_LUMINANCE[..], &DC_SIZE_CHROMINANCE[..]] {
            let mut sizes: Vec<_> = leaves(table).iter().map(|l| l.1).collect();
            sizes.sort();
            assert_eq!(sizes, (0..=8).collect::<Vec<_>>());
        }
        // 111 run/level pairs (B.5c-f, the first-coefficient `1s` shared
        // with end_of_block's prefix) plus the escape.
        let coefficients = leaves(&DCT_COEFF);
        assert_eq!(coefficients.len(), 112);
        assert!(coefficients.contains(&(6, ESCAPE)));
        // The code lengths of a complete prefix code sum to one, less the
        // invalid all-zero branches.
        let kraft: f64 = coefficients
            .iter()
            .map(|(len, _)| 0.5f64.powi(*len as i32))
            .sum();
        assert!(kraft < 1.0 && kraft > 0.99, "{kraft}");
    }

    #[test]
    fn idct_matches_the_floating_point_transform() {
        // A pseudo-random block of dequantised coefficients.
        let mut state = 12345u32;
        for _ in 0..50 {
            let mut coefficients = [0i32; 64];
            for value in coefficients.iter_mut().take(20) {
                state = state.wrapping_mul(1_103_515_245).wrapping_add(12345);
                *value = ((state >> 16) % 201) as i32 - 100;
            }
            let mut block = [0i32; 64];
            for (index, value) in coefficients.iter().enumerate() {
                block[index] = value * i32::from(PREMULTIPLIER[index]);
            }
            idct(&mut block);
            for y in 0..8 {
                for x in 0..8 {
                    let mut sum = 0.0f64;
                    for v in 0..8 {
                        for u in 0..8 {
                            let cu = if u == 0 { 0.5f64.sqrt() } else { 1.0 };
                            let cv = if v == 0 { 0.5f64.sqrt() } else { 1.0 };
                            sum += cu
                                * cv
                                * f64::from(coefficients[v * 8 + u])
                                * ((2 * x + 1) as f64 * u as f64 * std::f64::consts::PI / 16.0)
                                    .cos()
                                * ((2 * y + 1) as f64 * v as f64 * std::f64::consts::PI / 16.0)
                                    .cos();
                        }
                    }
                    let expected = sum / 4.0;
                    let got = f64::from(block[y * 8 + x]);
                    assert!((got - expected).abs() <= 1.5, "{got} vs {expected}");
                }
            }
        }
    }

    #[test]
    fn motion_vectors_wrap_inside_their_range() {
        // f_code 1 (r_size 0): range -16..=15; code +1 from 15 wraps to -16.
        let mut bits = Bits::new(&[0b0100_0000]);
        assert_eq!(decode_motion_vector(&mut bits, 0, 15).unwrap(), -16);
        // r_size 1: code -2 (`0011`), residual 1 → delta -(2 + 1 + 1) = -4.
        let mut bits = Bits::new(&[0b0011_1000]);
        assert_eq!(decode_motion_vector(&mut bits, 1, 3).unwrap(), -1);
    }

    #[test]
    fn prediction_clamps_vectors_past_the_edge() {
        // A 16x16 plane with a horizontal ramp; a vector one pixel left of
        // the picture repeats the edge column.
        let source: Vec<u8> = (0..256).map(|i| (i % 16) as u8 * 10).collect();
        let mut dest = vec![0u8; 256];
        let block = Placement {
            row: 0,
            col: 0,
            height: 1,
        };
        predict(&source, &mut dest, 16, block, (-2, 0), 16, false);
        assert_eq!(&dest[..4], &[0, 0, 10, 20]);
        predict(&source, &mut dest, 16, block, (1, 0), 16, false);
        assert_eq!(&dest[..2], &[5, 15]);
        assert_eq!(dest[15], 150);
    }
}
