//! RE2 `ADT` streams: the compressed camera backgrounds in
//! `Common/bin/roomcut.bin`.
//!
//! Ported from the openre decompilation's `AdtUnpacker` and `reorgAdt`
//! (`src/gfxadt.cpp`). A stream is a 4-byte header followed by blocks, each
//! introduced by a 16-bit symbol count; a zero count ends the stream, so a
//! stream is self-terminating and is decoded from its start offset without
//! knowing its length. Each block first reads three canonical Huffman tables
//! whose code lengths are delta-coded against the previous entry:
//!
//! - a 16-entry table used only to read the second table's lengths,
//! - a 512-entry table of symbols: below 256 a literal byte, otherwise a copy
//!   of `symbol - 253` bytes from the history,
//! - a 16-entry table of distance bit counts.
//!
//! Copies read a 16 KiB ring of the most recent output. The decoded buffer of
//! a background holds a 256 x 256 page followed by a 128 x 128 page whose two
//! 64-wide halves are the right-hand 64 columns of the 320 x 240 frame (rows
//! 0-127, then 128-239), all as 15-bit `0BBBBBGGGGGRRRRR` texels (the PSX order, red lowest); [`reorganize`] lays them out as
//! one 320 x 240 frame.

use anyhow::{Context, Result, ensure};

use crate::state::Image;

/// The background frame width.
pub const WIDTH: usize = 320;
/// The background frame height.
pub const HEIGHT: usize = 240;

/// The largest decoded stream the game accepts (`bg_load` refuses
/// `0x38300` bytes or more), and so the largest this decoder produces.
pub const MAX_OUTPUT: usize = 0x38300;

/// Bytes of the 256 x 256 page plus the 128 x 128 page a background needs.
pub const BACKGROUND_BYTES: usize = 256 * 256 * 2 + 128 * 128 * 2;

/// How far past the end of the input the bit reader may run (reading zero
/// bytes, as the original does) before the stream is declared truncated.
const MAX_OVERRUN: usize = 16;

/// The history ring size.
const HISTORY: usize = 0x4000;

/// No child: an unassigned tree slot.
const NONE: u32 = u32::MAX;

/// An MSB-first bit reader over a byte slice.
struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    byte: u8,
    bits: i32,
    overrun: usize,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            byte: 0,
            bits: 0,
            overrun: 0,
        }
    }

    /// The next input byte, or zero past the end.
    fn next_byte(&mut self) -> Result<u8> {
        match self.data.get(self.pos) {
            Some(&byte) => {
                self.pos += 1;
                Ok(byte)
            }
            None => {
                self.overrun += 1;
                ensure!(self.overrun <= MAX_OVERRUN, "ADT stream is truncated");
                Ok(0)
            }
        }
    }

    fn bits(&mut self, mut count: i32) -> Result<u32> {
        let mut high = 0u32;
        let mut value = u32::from(self.byte);
        while count > self.bits {
            count -= self.bits;
            let mut mask = (1u32 << self.bits) - 1;
            mask &= value;
            mask <<= count;
            self.byte = self.next_byte()?;
            value = u32::from(self.byte);
            self.bits = 8;
            high |= mask;
        }
        self.bits -= count;
        value >>= self.bits;
        Ok((value & ((1u32 << count) - 1)) | high)
    }

    fn bit(&mut self) -> Result<u32> {
        self.bits -= 1;
        if self.bits < 0 {
            self.bits = 7;
            self.byte = self.next_byte()?;
        }
        Ok(u32::from(self.byte >> self.bits) & 1)
    }

    /// An Elias-gamma style code: `n` zero bits, a one, then `n` more bits.
    fn code(&mut self) -> Result<usize> {
        let mut zeros = 0usize;
        while self.bit()? == 0 {
            zeros += 1;
            ensure!(zeros < 32, "ADT code is too long");
        }
        let mut result = 1usize;
        for _ in 0..zeros {
            result = (result << 1) + self.bit()? as usize;
        }
        Ok(result)
    }

    fn block_length(&mut self) -> Result<u16> {
        let low = self.bits(8)? as u16;
        let high = self.bits(8)? as u16;
        Ok((high << 8) | low)
    }
}

/// A canonical Huffman table of `len` symbols.
struct Huffman {
    lengths: Vec<u32>,
    starts: Vec<u32>,
    /// `[left, right]` children; the root is node `len`. A child below `len`
    /// is a symbol, at or above it another node.
    tree: Vec<[u32; 2]>,
}

impl Huffman {
    fn new(len: usize) -> Self {
        Self {
            lengths: vec![NONE; len],
            starts: vec![NONE; len],
            tree: vec![[NONE; 2]; len * 2],
        }
    }

    fn len(&self) -> usize {
        self.lengths.len()
    }

    /// Assign canonical codes from the lengths and build the decoding tree.
    fn build(&mut self) -> Result<()> {
        let len = self.len();
        let mut counts = [0u16; 17];
        for &length in &self.lengths {
            if length <= 16 {
                counts[length as usize] += 1;
            }
        }
        let mut next = [0u16; 18];
        for i in 0..16 {
            next[i + 2] = next[i + 1].wrapping_add(counts[i + 1]) << 1;
        }
        self.starts.fill(NONE);
        for (length, code) in next.iter_mut().enumerate() {
            for symbol in 0..len {
                if self.lengths[symbol] == length as u32 {
                    self.starts[symbol] = u32::from(*code);
                    *code = code.wrapping_add(1);
                }
            }
        }

        self.tree.fill([NONE; 2]);
        let mut free = len + 1;
        for symbol in 0..len {
            let length = self.lengths[symbol];
            // A zero-length symbol never occurs in the stream.
            if length == 0 {
                continue;
            }
            // A length above 17 has no canonical start code (the original
            // would walk garbage), so refuse it.
            ensure!(length <= 17, "ADT Huffman code length {length} is too long");
            let start = self.starts[symbol];
            let mut node = len;
            for step in 0..length {
                let side = ((start >> (length - step - 1)) & 1) as usize;
                if step + 1 == length {
                    self.tree[node][side] = symbol as u32;
                    break;
                }
                match self.tree[node][side] {
                    NONE => {
                        ensure!(free < self.tree.len(), "ADT Huffman tree overflows");
                        self.tree[node][side] = free as u32;
                        node = free;
                        free += 1;
                    }
                    child => node = child as usize,
                }
            }
        }
        Ok(())
    }

    fn read(&self, reader: &mut BitReader) -> Result<usize> {
        let len = self.len();
        let mut node = len;
        loop {
            let side = reader.bit()? as usize;
            let child = self.tree[node][side];
            ensure!(child != NONE, "ADT stream reads an unassigned Huffman code");
            node = child as usize;
            if node < len {
                return Ok(node);
            }
        }
    }
}

/// Read a 16-entry table whose lengths are delta-coded against the previous.
fn read_small_table(reader: &mut BitReader, table: &mut Huffman) -> Result<()> {
    let mut previous = 0u32;
    for index in 0..table.len() {
        if reader.bit()? != 0 {
            previous ^= reader.code()? as u32;
        }
        table.lengths[index] = previous;
    }
    table.build()
}

/// Read the 512-entry symbol table: alternating zero runs and runs of
/// lengths read through `small`, then XOR-accumulated.
fn read_symbol_table(reader: &mut BitReader, small: &Huffman, table: &mut Huffman) -> Result<()> {
    let len = table.len();
    let mut deltas = vec![0u32; len];
    let mut literal = reader.bit()? != 0;
    let mut at = 0usize;
    while at < len {
        let run = reader.code()?;
        ensure!(at + run <= len, "ADT symbol table run overflows");
        if literal {
            for delta in &mut deltas[at..at + run] {
                *delta = small.read(reader)? as u32;
            }
        }
        at += run;
        literal = !literal;
    }
    let mut length = 0u32;
    for (index, delta) in deltas.into_iter().enumerate() {
        length ^= delta;
        table.lengths[index] = length;
    }
    table.build()
}

/// Decode the ADT stream starting at `data[0]`.
///
/// Reading stops at the stream's zero block length, so `data` may run on
/// past it (to the end of `roomcut.bin`).
pub fn decode(data: &[u8]) -> Result<Vec<u8>> {
    ensure!(data.len() >= 4, "ADT stream is shorter than its header");
    let mut reader = BitReader::new(&data[4..]);
    let mut history = vec![0u8; HISTORY];
    let mut at = 0usize;
    let mut out = Vec::new();
    let mut lengths = Huffman::new(16);
    let mut symbols = Huffman::new(512);
    let mut distances = Huffman::new(16);

    loop {
        let count = reader.block_length()?;
        if count == 0 {
            break;
        }
        read_small_table(&mut reader, &mut lengths)?;
        read_symbol_table(&mut reader, &lengths, &mut symbols)?;
        read_small_table(&mut reader, &mut distances)?;

        for _ in 0..count {
            let symbol = symbols.read(&mut reader)?;
            if symbol < 256 {
                let byte = symbol as u8;
                history[at] = byte;
                at = (at + 1) & (HISTORY - 1);
                out.push(byte);
            } else {
                let run = symbol - 253;
                let mut distance = distances.read(&mut reader)?;
                if distance != 0 {
                    let bits = distance as i32 - 1;
                    distance = reader.bits(bits)? as usize + (1usize << bits);
                }
                let mut from = at.wrapping_sub(distance + 1) & (HISTORY - 1);
                for _ in 0..run {
                    let byte = history[from];
                    from = (from + 1) & (HISTORY - 1);
                    history[at] = byte;
                    at = (at + 1) & (HISTORY - 1);
                    out.push(byte);
                }
            }
            ensure!(
                out.len() < MAX_OUTPUT,
                "ADT stream decodes to {MAX_OUTPUT} bytes or more"
            );
        }
    }
    Ok(out)
}

/// Lay a decoded background's 256 x 256 and 128 x 128 pages out as one
/// 320 x 240 frame of 16-bit texels, row-major.
pub fn reorganize(decoded: &[u8]) -> Result<Vec<u16>> {
    ensure!(
        decoded.len() >= BACKGROUND_BYTES,
        "decoded background is {} bytes, needs {BACKGROUND_BYTES}",
        decoded.len()
    );
    let texel = |offset: usize| u16::from_le_bytes([decoded[offset], decoded[offset + 1]]);
    let side = 256 * 256 * 2;
    let mut frame = vec![0u16; WIDTH * HEIGHT];
    for y in 0..HEIGHT {
        for x in 0..256 {
            frame[y * WIDTH + x] = texel((y * 256 + x) * 2);
        }
        // Rows 0-127 come from the side page's left half, rows 128-239 from
        // its right half, starting again at its top.
        let (row, column) = if y < 128 { (y, 0) } else { (y - 128, 64) };
        for x in 0..64 {
            frame[y * WIDTH + 256 + x] = texel(side + (row * 128 + column + x) * 2);
        }
    }
    Ok(frame)
}

/// Widen a 15-bit `0BBBBBGGGGGRRRRR` texel to 8-bit channels by bit
/// replication.
fn widen(texel: u16) -> [u8; 3] {
    let channel = |value: u16| {
        let value = (value & 0x1F) as u8;
        (value << 3) | (value >> 2)
    };
    [channel(texel), channel(texel >> 5), channel(texel >> 10)]
}

/// Decode a background stream into a 320 x 240 opaque image.
pub fn background(data: &[u8]) -> Result<Image> {
    let decoded = decode(data)?;
    let frame = reorganize(&decoded).context("ADT stream is not a background")?;
    let mut rgba = Vec::with_capacity(WIDTH * HEIGHT * 4);
    for texel in frame {
        let [red, green, blue] = widen(texel);
        rgba.extend_from_slice(&[red, green, blue, 0xFF]);
    }
    Ok(Image {
        width: WIDTH as u32,
        height: HEIGHT as u32,
        rgba,
    })
}

/// Stream builders for tests outside this module.
#[cfg(test)]
pub(crate) mod fixture {
    use super::BACKGROUND_BYTES;

    /// MSB-first bit writer for building fixtures.
    #[derive(Default)]
    pub(crate) struct Bits {
        pub(crate) out: Vec<u8>,
        used: u32,
    }

    impl Bits {
        pub(crate) fn push(&mut self, value: u32, count: u32) {
            for index in (0..count).rev() {
                if self.used == 0 {
                    self.out.push(0);
                }
                let bit = ((value >> index) & 1) as u8;
                let last = self.out.len() - 1;
                self.out[last] |= bit << (7 - self.used);
                self.used = (self.used + 1) % 8;
            }
        }

        /// Write `value` (>= 1) as the reader's gamma code.
        pub(crate) fn code(&mut self, value: u32) {
            let width = 32 - value.leading_zeros();
            self.push(0, width - 1);
            self.push(value, width);
        }
    }

    /// An ADT stream that decodes to a full background of `byte`: one
    /// literal, then 258-byte copies of the previous byte, then one shorter
    /// copy for the remainder. The three symbols (the literal, the remainder
    /// copy and the 258-byte copy) all have 2-bit codes.
    pub(crate) fn uniform_background(byte: u8) -> Vec<u8> {
        let rest = BACKGROUND_BYTES - 1;
        let (copies, remainder) = (rest / 258, rest % 258);
        assert!((3..257).contains(&remainder) && byte > 0);
        let short = 253 + remainder as u32;
        let mut bits = Bits::default();
        let count = 1 + copies + 1;
        bits.push(count as u32 & 0xFF, 8);
        bits.push((count >> 8) as u32, 8);
        // Small table: only symbol 2 (the delta 2) has a code, `0`.
        bits.push(0, 1);
        bits.push(0, 1);
        bits.push(1, 1);
        bits.code(1);
        bits.push(1, 1);
        bits.code(1);
        for _ in 4..16 {
            bits.push(0, 1);
        }
        // Symbol table: length 2 at `byte` and `short` (set then cleared on
        // the next entry) and at 511, as zero runs between delta runs.
        bits.push(0, 1);
        let mut at = 0u32;
        for symbol in [u32::from(byte), short] {
            if symbol > at {
                bits.code(symbol - at);
            }
            bits.code(2);
            bits.push(0, 1);
            bits.push(0, 1);
            at = symbol + 2;
        }
        bits.code(511 - at);
        bits.code(1);
        bits.push(0, 1);
        // Distance table: only symbol 0.
        bits.push(1, 1);
        bits.code(1);
        bits.push(1, 1);
        bits.code(1);
        for _ in 2..16 {
            bits.push(0, 1);
        }
        // Codes in symbol order: `byte` 00, `short` 01, 511 10.
        bits.push(0b00, 2);
        for _ in 0..copies {
            bits.push(0b10, 2);
            bits.push(0, 1);
        }
        bits.push(0b01, 2);
        bits.push(0, 1);
        bits.push(0, 16);
        let mut stream = vec![0u8; 4];
        stream.extend(bits.out);
        stream
    }
}

#[cfg(test)]
mod tests {
    use super::fixture::Bits;
    use super::*;

    #[test]
    fn widen_replicates_the_top_bits() {
        assert_eq!(widen(0x7FFF), [0xFF, 0xFF, 0xFF]);
        assert_eq!(widen(0x0000), [0, 0, 0]);
        assert_eq!(widen(0x001F), [0xFF, 0, 0]);
        assert_eq!(widen(0x7C00), [0, 0, 0xFF]);
        assert_eq!(widen(0x0010), [0x84, 0, 0]);
    }

    #[test]
    fn an_empty_stream_decodes_to_nothing() {
        // Header, then a zero block length.
        assert_eq!(decode(&[0, 0, 0, 0, 0, 0]).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn a_stream_shorter_than_its_header_fails() {
        assert!(decode(&[0, 0]).is_err());
    }

    /// One block with two symbols of length 1 in the 512 table: `0x41` ('A')
    /// and the 3-byte copy symbol 256, with a single distance symbol 0.
    fn literal_and_copy_stream() -> Vec<u8> {
        let mut bits = Bits::default();
        // Block of 3 symbols: A, A, copy(3, distance 0).
        bits.push(3, 8);
        bits.push(0, 8);
        // Small table: symbol 1 has length 1, symbol 2 length 1, others 0.
        // Deltas: [0, 1, 0, 1, 0...]: entry 0 keeps 0; entry 1 flips to 1;
        // entry 2 keeps 1; entry 3 flips back to 0.
        bits.push(0, 1);
        bits.push(1, 1);
        bits.code(1);
        bits.push(0, 1);
        bits.push(1, 1);
        bits.code(1);
        for _ in 4..16 {
            bits.push(0, 1);
        }
        // Symbol table: lengths 1 at 0x41 and 256, read as deltas via the
        // small table where symbol 1 is code `0` and symbol 2 is code `1`.
        // Delta 1 at 0x41 (sets 1), delta 1 at 0x42 (clears), delta 1 at
        // 256 (sets), delta 1 at 257 (clears).
        bits.push(0, 1); // start with a zero run
        bits.code(0x41);
        bits.code(2); // literal run of 2
        bits.push(0, 1); // delta 1
        bits.push(0, 1); // delta 1
        bits.code(256 - 0x43);
        bits.code(2);
        bits.push(0, 1);
        bits.push(0, 1);
        bits.code(512 - 258);
        // Distance table: only symbol 0, length 1.
        bits.push(1, 1);
        bits.code(1);
        bits.push(1, 1);
        bits.code(1);
        for _ in 2..16 {
            bits.push(0, 1);
        }
        // Codes: 0x41 is `0`, 256 is `1`; distance symbol 0 is `0`.
        bits.push(0, 1);
        bits.push(0, 1);
        bits.push(1, 1);
        bits.push(0, 1);
        // End of stream.
        bits.push(0, 16);
        let mut stream = vec![0u8; 4];
        stream.extend(bits.out);
        stream
    }

    #[test]
    fn literals_and_history_copies_decode() {
        assert_eq!(decode(&literal_and_copy_stream()).unwrap(), b"AAAAA");
    }

    #[test]
    fn a_truncated_stream_fails() {
        let stream = literal_and_copy_stream();
        assert!(decode(&stream[..6]).is_err());
    }

    /// A stream of `bits`, preceded by the 4-byte header and a block count
    /// of one.
    fn one_block(bits: Bits) -> Vec<u8> {
        let mut stream = vec![0u8; 4];
        stream.extend([1, 0]);
        stream.extend(bits.out);
        stream
    }

    #[test]
    fn a_code_length_over_17_fails() {
        let mut bits = Bits::default();
        bits.push(1, 1);
        bits.code(20);
        for _ in 1..16 {
            bits.push(0, 1);
        }
        let error = decode(&one_block(bits)).unwrap_err();
        assert!(error.to_string().contains("too long"), "{error}");
    }

    #[test]
    fn an_unassigned_code_fails() {
        // Only symbol 0 has a code (`0`) in the small table; the symbol
        // table then reads `1` through it.
        let mut bits = Bits::default();
        bits.push(1, 1);
        bits.code(1);
        bits.push(1, 1);
        bits.code(1);
        for _ in 2..16 {
            bits.push(0, 1);
        }
        bits.push(1, 1);
        bits.code(1);
        bits.push(1, 1);
        let error = decode(&one_block(bits)).unwrap_err();
        assert!(error.to_string().contains("unassigned"), "{error}");
    }

    #[test]
    fn reading_far_past_the_end_fails() {
        // A gamma code of zeros runs off the end: the reader stops after
        // `MAX_OVERRUN` phantom bytes instead of spinning.
        let error = decode(&[0, 0, 0, 0, 1, 0, 0x80]).unwrap_err();
        assert!(format!("{error:#}").contains("ADT"), "{error:#}");
    }

    #[test]
    fn arbitrary_bytes_never_panic() {
        // A fixed xorshift sequence of streams, some built on a valid prefix,
        // stands in for a fuzz run.
        let valid = super::fixture::uniform_background(0x21);
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for round in 0..500 {
            let len = (next() % 600) as usize;
            let mut data: Vec<u8> = (0..len).map(|_| next() as u8).collect();
            if round % 2 == 0 {
                let keep = (next() as usize) % valid.len();
                data.splice(0..0, valid[..keep].iter().copied());
            }
            let _ = decode(&data);
            let _ = background(&data);
        }
    }

    #[test]
    fn a_uniform_background_fixture_decodes() {
        let image = background(&super::fixture::uniform_background(0x21)).unwrap();
        assert_eq!((image.width, image.height), (320, 240));
        // 0x2121 is red 1, green 9, blue 8.
        let [red, green, blue] = widen(0x2121);
        assert!(
            image
                .rgba
                .chunks(4)
                .all(|pixel| pixel == [red, green, blue, 0xFF])
        );
    }

    #[test]
    fn reorganize_places_the_side_page_halves() {
        let mut decoded = vec![0u8; BACKGROUND_BYTES];
        let side = 256 * 256 * 2;
        let put = |decoded: &mut Vec<u8>, offset: usize, value: u16| {
            decoded[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        };
        put(&mut decoded, (5 * 256 + 7) * 2, 0x1111);
        put(&mut decoded, side + (3 * 128 + 2) * 2, 0x2222);
        put(&mut decoded, side + (4 * 128 + 64 + 9) * 2, 0x3333);
        let frame = reorganize(&decoded).unwrap();
        assert_eq!(frame[5 * WIDTH + 7], 0x1111);
        assert_eq!(frame[3 * WIDTH + 256 + 2], 0x2222);
        assert_eq!(frame[(128 + 4) * WIDTH + 256 + 9], 0x3333);
        assert!(reorganize(&decoded[..BACKGROUND_BYTES - 1]).is_err());
    }
}
