//! A big-endian bit reader over one elementary-stream buffer.
//!
//! Reads past the end return zero bits rather than failing, the way a
//! hardware decoder sees a truncated stream; [`Bits::overrun`] reports it so
//! a decoder can stop at the end of a damaged unit.

use super::tables::Vlc;

/// A bit cursor over a byte slice.
#[derive(Debug, Clone)]
pub struct Bits<'a> {
    data: &'a [u8],
    /// The next bit, counted from the slice start.
    pos: usize,
}

impl<'a> Bits<'a> {
    /// A reader at the first bit of `data`.
    pub fn new(data: &'a [u8]) -> Self {
        Bits { data, pos: 0 }
    }

    /// The next 64 bits from the byte holding the cursor, zero-padded.
    fn window(&self) -> u64 {
        let byte = self.pos >> 3;
        match self.data.get(byte..byte + 8) {
            Some(bytes) => u64::from_be_bytes(bytes.try_into().expect("eight bytes")),
            None => {
                let mut word = 0u64;
                for index in 0..8 {
                    word =
                        (word << 8) | u64::from(self.data.get(byte + index).copied().unwrap_or(0));
                }
                word
            }
        }
    }

    /// The next `count` bits (at most 32) without consuming them.
    pub fn peek(&self, count: u32) -> u32 {
        debug_assert!(count <= 32);
        if count == 0 {
            return 0;
        }
        ((self.window() << (self.pos & 7)) >> (64 - count)) as u32
    }

    /// Read `count` bits (at most 32).
    pub fn read(&mut self, count: u32) -> u32 {
        let value = self.peek(count);
        self.pos += count as usize;
        value
    }

    /// Read one bit as a flag.
    pub fn flag(&mut self) -> bool {
        self.read(1) != 0
    }

    /// Skip `count` bits.
    pub fn skip(&mut self, count: usize) {
        self.pos += count;
    }

    /// Move to the next byte boundary.
    pub fn align(&mut self) {
        self.pos = (self.pos + 7) & !7;
    }

    /// The cursor in bits from the slice start.
    pub fn position(&self) -> usize {
        self.pos
    }

    /// The cursor in whole bytes, rounded up.
    pub fn byte_position(&self) -> usize {
        self.pos.div_ceil(8)
    }

    /// Whether the cursor has passed the end of the data.
    pub fn overrun(&self) -> bool {
        self.pos > self.data.len() * 8
    }

    /// The bits left before the end of the data.
    pub fn remaining(&self) -> usize {
        (self.data.len() * 8).saturating_sub(self.pos)
    }

    /// Align, then find the next `00 00 01 xx` start code, leave the cursor
    /// after it and return `xx`; `None` (cursor at the end) when there is
    /// none.
    pub fn next_start_code(&mut self) -> Option<u8> {
        self.align();
        let from = (self.pos >> 3).min(self.data.len());
        match find_start_code(self.data, from) {
            Some(at) => {
                self.pos = (at + 4) * 8;
                Some(self.data[at + 3])
            }
            None => {
                self.pos = self.data.len() * 8;
                None
            }
        }
    }

    /// Decode one code through a VLC tree; `None` for an invalid code.
    pub fn vlc(&mut self, table: &[Vlc]) -> Option<i32> {
        let mut node = 0usize;
        loop {
            let (next, value) = *table.get(node + self.read(1) as usize)?;
            match next {
                0 => return Some(value),
                next if next > 0 => node = next as usize,
                _ => return None,
            }
        }
    }
}

/// The byte offset of the first `00 00 01 xx` start code at or after `from`
/// with its code byte inside `data`.
pub fn find_start_code(data: &[u8], from: usize) -> Option<usize> {
    let mut at = from;
    while at + 4 <= data.len() {
        // Skip ahead two bytes at a time while the middle byte cannot start
        // a prefix, the usual start-code scan.
        let byte = data[at + 2];
        if byte > 1 {
            at += 3;
        } else if byte == 1 && data[at] == 0 && data[at + 1] == 0 {
            return Some(at);
        } else {
            at += 1;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_across_bytes_and_pads_with_zeros() {
        let mut bits = Bits::new(&[0b1010_1100, 0b0101_0011, 0xFF]);
        assert_eq!(bits.read(3), 0b101);
        assert_eq!(bits.read(7), 0b011_0001);
        assert_eq!(bits.peek(6), 0b01_0011);
        assert_eq!(bits.read(14), 0b01_0011_1111_1111);
        assert!(!bits.overrun());
        assert_eq!(bits.read(8), 0);
        assert!(bits.overrun());
    }

    #[test]
    fn finds_start_codes_at_any_alignment() {
        let data = [0x12, 0, 0, 1, 0xB3, 0, 0, 0, 1, 0x00, 0xFF];
        assert_eq!(find_start_code(&data, 0), Some(1));
        assert_eq!(find_start_code(&data, 2), Some(6));
        assert_eq!(find_start_code(&data, 7), None);
        let mut bits = Bits::new(&data);
        bits.skip(3);
        assert_eq!(bits.next_start_code(), Some(0xB3));
        assert_eq!(bits.next_start_code(), Some(0x00));
        assert_eq!(bits.read(8), 0xFF);
        assert_eq!(bits.next_start_code(), None);
    }
}
