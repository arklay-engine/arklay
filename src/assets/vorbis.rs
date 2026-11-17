//! Refuses the OGG Vorbis streams Symphonia's demuxer cannot survive (#769).
//!
//! TODO: remove this guard once `symphonia-format-ogg` bounds these itself.
//! Its Vorbis mapper skips each setup header's codebooks without the limits
//! `symphonia-codec-vorbis` applies to the same fields, and its own
//! `lookup1_values` asserts on a codebook with no dimensions, 32 or more
//! dimensions, or (through float rounding) more than ~1.9M entries, so a few
//! bytes panic the demuxer. Every beginning-of-stream page also costs it about
//! 500 bytes that are held until it returns.
//!
//! The guard makes the stream's pages unambiguous, then holds every setup
//! header Symphonia would parse to the codec's own codebook limits:
//!
//! - The file must be a contiguous chain of valid pages from its first byte to
//!   its last. Symphonia resynchronises one byte past a page that fails its
//!   checksum, so on any other input it can reach a page hidden inside another
//!   page's body; on a clean chain it reads exactly these pages.
//! - Every logical stream must be Vorbis, so no other mapper runs, and the
//!   beginning-of-stream pages are capped by [`budget::MAX_OGG_STREAMS`].
//! - Packets are reassembled exactly as `LogicalStream::read_page_init` does,
//!   and each stream may carry one setup header: the mapper re-copies its
//!   accumulated codec data for every one, which is quadratic.
//! - Each setup header's codebooks are walked and refused past the codec's
//!   limits (1 to 32 dimensions, 131,072 entries), as a lookup-1 book of 32
//!   dimensions (which the demuxer's assertion overflows on), or when the
//!   codec would hold them in more than [`budget::MAX_VORBIS_SETUP_BYTES`]:
//!   its per-codebook lookup table alone can reach hundreds of MiB from a few
//!   bytes of header.
//! - The rest of each setup header is walked up to its mappings, refusing
//!   what the codec would later panic on while decoding audio: a codebook with
//!   no codewords, a floor 0 of 16 codebooks, a floor 1 x position of 0 or Y
//!   range that overflows its curve renderer, and a type 2 residue over a
//!   channel pair that does not start at channel 0.
//! - Before its first audio packet Symphonia feeds a repeated first page to
//!   the stream it belongs to, so the guard tracks that phase too.

use std::collections::HashMap;

use anyhow::{Result, bail};

use crate::budget;

/// The Ogg page capture pattern.
const CAPTURE: &[u8; 4] = b"OggS";

/// An Ogg page header's fixed part, before its lacing table.
const PAGE_HEADER_LEN: usize = 27;

/// Page header flags: a continued packet, and the first page of a stream.
const CONTINUED: u8 = 0x01;
const FIRST_PAGE: u8 = 0x02;

/// The prefixes of a Vorbis identification and setup header packet.
const IDENT_HEADER: &[u8; 7] = b"\x01vorbis";
const SETUP_HEADER: &[u8; 7] = b"\x05vorbis";

/// Symphonia's largest partial packet (`LogicalStream::MAX_PACKET_LEN`).
const MAX_PACKET_LEN: usize = 16 << 20;

/// The Vorbis codebook sync pattern, `BCV`.
const CODEBOOK_SYNC: u32 = 0x56_43_42;

/// `symphonia-codec-vorbis`'s codebook limits.
const MAX_CODEBOOK_DIMENSIONS: u32 = 32;
const MAX_CODEBOOK_ENTRIES: u32 = 128 * 1024;

/// The longest codeword the Vorbis spec allows.
const MAX_CODEWORD_LENGTH: u32 = 32;

/// Refuse a stream Symphonia's Ogg demuxer would panic on or grow without
/// bound; see the module docs for what is checked and why.
pub(super) fn check_stream(data: &[u8]) -> Result<()> {
    let mut streams: HashMap<u32, Stream> = HashMap::new();
    let mut phase = Phase::Streaming;
    let mut first_pages = 0usize;
    let mut offset = 0;
    while offset < data.len() {
        let page = Page::read(data, offset)?;
        offset = page.end;
        let first = page.flags & FIRST_PAGE != 0;
        if first {
            first_pages += 1;
            budget::check_len(
                first_pages,
                budget::MAX_OGG_STREAMS,
                "OGG logical stream count",
            )?;
        }
        match phase {
            // `start_new_physical_stream`: a run of first pages replaces the
            // group of logical streams, and their packets only identify the
            // codec.
            Phase::FirstPages | Phase::Streaming if first => {
                if phase == Phase::Streaming {
                    streams.clear();
                    phase = Phase::FirstPages;
                }
                streams.insert(page.serial, Stream::identify(&page)?);
                continue;
            }
            // It then feeds every page, first pages included, to its stream
            // until one buffers an audio packet.
            Phase::FirstPages | Phase::Headers => phase = Phase::Headers,
            Phase::Streaming => {}
        }
        // A page of no stream in the current group is discarded.
        if let Some(stream) = streams.get_mut(&page.serial) {
            stream.read_page(&page)?;
            if stream.has_audio {
                phase = Phase::Streaming;
            }
        }
    }
    Ok(())
}

/// Where Symphonia's `OggReader` is in a group of logical streams.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Reading the run of first pages that opens the group.
    FirstPages,
    /// Reading header pages until a stream buffers its first audio packet.
    Headers,
    /// Reading audio; a first page opens a new group.
    Streaming,
}

/// One logical stream's reassembly state, as Symphonia keeps it.
struct Stream {
    channels: u8,
    partial: Vec<u8>,
    last_sequence: Option<u32>,
    has_setup: bool,
    has_audio: bool,
}

impl Stream {
    /// A stream opened by its first page, which must hold exactly a whole
    /// Vorbis identification header packet.
    fn identify(page: &Page<'_>) -> Result<Self> {
        let Some(ident) = page
            .packets()
            .next()
            .filter(|packet| packet.starts_with(IDENT_HEADER))
        else {
            bail!("OGG stream {:#x} is not Vorbis", page.serial);
        };
        if page.partial().is_some() {
            bail!(
                "OGG Vorbis stream {:#x} continues a packet past its first page",
                page.serial
            );
        }
        // The channel count follows the header type, magic and version.
        let channels = ident.get(IDENT_HEADER.len() + 4).copied().unwrap_or(0);
        if channels == 0 {
            bail!("OGG Vorbis stream {:#x} has no channels", page.serial);
        }
        Ok(Self {
            channels,
            partial: Vec::new(),
            last_sequence: None,
            has_setup: false,
            has_audio: false,
        })
    }

    /// Reassemble one page's packets, mirroring `LogicalStream::read_page_init`,
    /// and check each setup header it completes.
    fn read_page(&mut self, page: &Page<'_>) -> Result<()> {
        // A sequence that steps back or skips a page drops the partial packet.
        if let Some(last) = self.last_sequence
            && (page.sequence < last || page.sequence - last > 1)
        {
            self.partial.clear();
        }
        self.last_sequence = Some(page.sequence);
        let continued = page.flags & CONTINUED != 0;
        if !continued {
            self.partial.clear();
        }
        // A continuation with nothing to continue drops its first packet, or
        // the whole page when no packet ends on it.
        let mut packets = page.packets();
        if continued && self.partial.is_empty() && packets.next().is_none() {
            return Ok(());
        }
        for packet in packets {
            let joined;
            let packet = if self.partial.is_empty() {
                packet
            } else {
                self.partial.extend_from_slice(packet);
                joined = std::mem::take(&mut self.partial);
                &joined[..]
            };
            // The mapper queues a nonempty packet of even type as audio.
            if packet.first().is_some_and(|kind| kind & 1 == 0) {
                self.has_audio = true;
            }
            if packet.starts_with(SETUP_HEADER) {
                if self.has_setup {
                    bail!(
                        "OGG Vorbis stream {:#x} has a second setup header",
                        page.serial
                    );
                }
                self.has_setup = true;
                check_setup(packet, self.channels)?;
            }
        }
        if let Some(tail) = page.partial() {
            if self.partial.len() + tail.len() > MAX_PACKET_LEN {
                bail!("OGG Vorbis packet runs past the {MAX_PACKET_LEN}-byte limit");
            }
            self.partial.extend_from_slice(tail);
        }
        Ok(())
    }
}

/// One Ogg page, validated in full.
struct Page<'a> {
    flags: u8,
    serial: u32,
    sequence: u32,
    lacing: &'a [u8],
    body: &'a [u8],
    /// The offset just past the page.
    end: usize,
}

impl<'a> Page<'a> {
    /// Read the page at `offset`, refusing anything Symphonia would skip or
    /// resynchronise past.
    fn read(data: &'a [u8], offset: usize) -> Result<Self> {
        let Some(header) = data
            .get(offset..offset + PAGE_HEADER_LEN)
            .filter(|header| header.starts_with(CAPTURE))
        else {
            bail!("OGG stream has no page at byte {offset}");
        };
        let flags = header[5];
        if header[4] != 0 || flags & 0xF8 != 0 {
            bail!("OGG page at byte {offset} has an unknown version or flags");
        }
        let word = |at: usize| u32::from_le_bytes(header[at..at + 4].try_into().unwrap());
        let lacing_start = offset + PAGE_HEADER_LEN;
        let lacing_end = lacing_start + usize::from(header[26]);
        let Some(lacing) = data.get(lacing_start..lacing_end) else {
            bail!("OGG page at byte {offset} is truncated");
        };
        let body_len: usize = lacing.iter().map(|&len| usize::from(len)).sum();
        let Some(body) = data.get(lacing_end..lacing_end + body_len) else {
            bail!("OGG page at byte {offset} is truncated");
        };
        let mut crc = crc32(0, &header[..22]);
        crc = crc32(crc, &[0; 4]);
        crc = crc32(crc, &header[26..]);
        crc = crc32(crc, lacing);
        crc = crc32(crc, body);
        if crc != word(22) {
            bail!("OGG page at byte {offset} fails its checksum");
        }
        Ok(Self {
            flags,
            serial: word(14),
            sequence: word(18),
            lacing,
            body,
            end: lacing_end + body_len,
        })
    }

    /// The packets that end on this page, the first possibly continuing one
    /// from an earlier page.
    fn packets(&self) -> impl Iterator<Item = &'a [u8]> + use<'a> {
        let body = self.body;
        let mut start = 0;
        let mut len = 0;
        self.lacing.iter().filter_map(move |&segment| {
            len += usize::from(segment);
            if segment == u8::MAX {
                return None;
            }
            let packet = &body[start..start + len];
            start += len;
            len = 0;
            Some(packet)
        })
    }

    /// The start of a packet that continues on a later page, if any.
    fn partial(&self) -> Option<&'a [u8]> {
        let complete = self
            .lacing
            .iter()
            .rposition(|&segment| segment < u8::MAX)
            .map_or(0, |last| {
                self.lacing[..=last]
                    .iter()
                    .map(|&len| usize::from(len))
                    .sum()
            });
        (complete < self.body.len()).then(|| &self.body[complete..])
    }
}

/// The Ogg page checksum: CRC-32 with polynomial `0x04C11DB7`, no reflection,
/// a zero initial value and no final XOR.
fn crc32(mut crc: u32, bytes: &[u8]) -> u32 {
    const TABLE: [u32; 256] = {
        let mut table = [0u32; 256];
        let mut index = 0;
        while index < 256 {
            let mut value = (index as u32) << 24;
            let mut bit = 0;
            while bit < 8 {
                value = if value & 0x8000_0000 != 0 {
                    (value << 1) ^ 0x04C1_1DB7
                } else {
                    value << 1
                };
                bit += 1;
            }
            table[index] = value;
            index += 1;
        }
        table
    };
    for &byte in bytes {
        crc = (crc << 8) ^ TABLE[usize::from((crc >> 24) as u8 ^ byte)];
    }
    crc
}

/// Walk a setup header as both Symphonia readers do, up to its mappings,
/// refusing what either would panic on or the codec would hold past
/// [`budget::MAX_VORBIS_SETUP_BYTES`]. `channels` is the stream's channel
/// count, from its identification header.
fn check_setup(packet: &[u8], channels: u8) -> Result<()> {
    let mut bits = Bits::new(&packet[SETUP_HEADER.len()..]);
    let entries = check_codebooks(&mut bits)?;
    for _ in 0..bits.read(6)? + 1 {
        if bits.read(16)? != 0 {
            bail!("OGG Vorbis setup header has a nonzero time-domain transform");
        }
    }
    check_floors(&mut bits, &entries)?;
    let residue_types = read_residue_types(&mut bits)?;
    check_mappings(&mut bits, channels, &residue_types)
}

/// Walk the codebooks, returning each one's entry count.
///
/// Refuses what the demuxer's codebook skipper would panic on, a codebook
/// with no codewords (the codec builds it an empty lookup table, then indexes
/// it), and codebooks the codec would hold in more than the budget.
fn check_codebooks(bits: &mut Bits<'_>) -> Result<Vec<u32>> {
    let codebooks = bits.read(8)? + 1;
    let mut all_entries = Vec::with_capacity(codebooks as usize);
    let mut bytes = 0usize;
    for index in 0..codebooks {
        if bits.read(24)? != CODEBOOK_SYNC {
            bail!("OGG Vorbis codebook {index} has a bad sync pattern");
        }
        let dimensions = bits.read(16)?;
        let entries = bits.read(24)?;
        if dimensions == 0 || dimensions > MAX_CODEBOOK_DIMENSIONS {
            bail!("OGG Vorbis codebook {index} has {dimensions} dimensions, outside 1 to 32");
        }
        if entries > MAX_CODEBOOK_ENTRIES {
            bail!(
                "OGG Vorbis codebook {index} has {entries} entries, over the \
                 {MAX_CODEBOOK_ENTRIES} limit"
            );
        }
        // How many codewords have each length, 1 to 32 bits.
        let mut lengths = [0usize; MAX_CODEWORD_LENGTH as usize + 1];
        if bits.read(1)? == 0 {
            let sparse = bits.read(1)? == 1;
            for _ in 0..entries {
                if !sparse || bits.read(1)? == 1 {
                    lengths[bits.read(5)? as usize + 1] += 1;
                }
            }
        } else {
            let mut length = bits.read(5)? + 1;
            let mut entry = 0;
            while entry < entries {
                if length > MAX_CODEWORD_LENGTH {
                    bail!("OGG Vorbis codebook {index} has a codeword longer than 32 bits");
                }
                let run = bits.read(ilog(entries - entry))?;
                lengths[length as usize] += run as usize;
                entry += run;
                length += 1;
            }
            if entry > entries {
                bail!("OGG Vorbis codebook {index} declares more codewords than entries");
            }
        }
        if lengths.iter().all(|&count| count == 0) {
            bail!("OGG Vorbis codebook {index} has no codewords");
        }
        bytes += table_bytes(&lengths) + entries as usize * ENTRY_BYTES;
        let lookup_bits = match bits.read(4)? {
            0 => 0,
            lookup @ (1 | 2) => {
                if lookup == 1 && dimensions == MAX_CODEBOOK_DIMENSIONS {
                    bail!("OGG Vorbis codebook {index} is a lookup-1 book of 32 dimensions");
                }
                bits.skip(64)?;
                let value_bits = bits.read(4)? + 1;
                bits.skip(1)?;
                bytes += entries as usize * dimensions as usize * size_of::<f32>();
                let lookups = if lookup == 1 {
                    lookup1_values(entries, dimensions)
                } else {
                    entries * dimensions
                };
                u64::from(lookups) * u64::from(value_bits)
            }
            lookup => bail!("OGG Vorbis codebook {index} has unknown lookup type {lookup}"),
        };
        budget::check_len(
            bytes,
            budget::MAX_VORBIS_SETUP_BYTES,
            "OGG Vorbis codebook storage in bytes",
        )?;
        bits.skip(lookup_bits)?;
        all_entries.push(entries);
    }
    Ok(all_entries)
}

/// Walk the floors, as `symphonia-codec-vorbis`'s `Floor0::read_setup` and
/// `Floor1::read_setup` read them.
fn check_floors(bits: &mut Bits<'_>, entries: &[u32]) -> Result<()> {
    for index in 0..bits.read(6)? + 1 {
        match bits.read(16)? {
            0 => {
                // Order, rate, Bark map size, amplitude bits and offset.
                bits.skip(8 + 16 + 16 + 6 + 8)?;
                let books = bits.read(4)? + 1;
                // Audio picks one with `ilog(books)` bits, which for 16 books
                // reaches past the codec's 16-entry list.
                if books == 16 {
                    bail!("OGG Vorbis floor {index} lists 16 codebooks");
                }
                bits.skip(u64::from(books) * 8)?;
            }
            1 => check_floor1(bits, index, entries)?,
            kind => bail!("OGG Vorbis floor {index} has unknown type {kind}"),
        }
    }
    Ok(())
}

/// Walk one floor 1, refusing an x position of 0 and Y values that would
/// overflow the codec's curve renderer.
///
/// The codec seeds the x list with 0 and `2^rangebits` and checks only the
/// positions it reads for uniqueness, so a read position of 0 (the only one
/// that can repeat a seed) leaves two neighbours at the same x, and
/// `render_point` divides by their zero distance.
///
/// `render_point` also multiplies a Y difference by an x distance in `u32`.
/// The two seeded Ys are read in at most 8 bits. Every later Y is a value
/// decoded from a subclass codebook (below its entry count `e`), `range - 1`
/// minus such a value, or lies between Ys already reconstructed or within
/// `0..=range`, so every Y lies in `min(0, range - e) ..= max(e - 1, 255)`,
/// and a difference is at most `2e + 255`.
fn check_floor1(bits: &mut Bits<'_>, index: u32, entries: &[u32]) -> Result<()> {
    let partitions = bits.read(5)? as usize;
    let mut classes = [0u32; 32];
    let mut dimensions = [0u32; 16];
    let mut largest_book = 0u64;
    if partitions > 0 {
        for class in &mut classes[..partitions] {
            *class = bits.read(4)?;
        }
        let used = classes[..partitions].iter().max().copied().unwrap_or(0) as usize + 1;
        for class_dimensions in &mut dimensions[..used] {
            *class_dimensions = bits.read(3)? + 1;
            let subclass_bits = bits.read(2)?;
            if subclass_bits != 0 {
                bits.skip(8)?;
            }
            for _ in 0..1 << subclass_bits {
                // A subclass book number is the codebook number plus one.
                if let Some(book) = bits.read(8)?.checked_sub(1) {
                    let book_entries = entries.get(book as usize).copied().unwrap_or(0);
                    largest_book = largest_book.max(u64::from(book_entries));
                }
            }
        }
    }
    // The multiplier, then the bit width of each x position.
    bits.skip(2)?;
    let range_bits = bits.read(4)?;
    for &class in &classes[..partitions] {
        for _ in 0..dimensions[class as usize] {
            if bits.read(range_bits)? == 0 {
                bail!("OGG Vorbis floor {index} repeats the x position 0");
            }
        }
    }
    let largest_step = (2 * largest_book + 255) * ((1 << range_bits) - 1);
    if largest_step > u64::from(u32::MAX) {
        bail!(
            "OGG Vorbis floor {index} pairs a {largest_book}-entry codebook with \
             {range_bits}-bit x positions, which overflows the floor curve"
        );
    }
    Ok(())
}

/// Walk the residues, as `Residue::read_setup` reads them, returning each
/// one's type.
fn read_residue_types(bits: &mut Bits<'_>) -> Result<Vec<u32>> {
    let count = bits.read(6)? + 1;
    let mut types = Vec::with_capacity(count as usize);
    for index in 0..count {
        let kind = bits.read(16)?;
        if kind > 2 {
            bail!("OGG Vorbis residue {index} has unknown type {kind}");
        }
        // Begin, end and partition size, then the classifications.
        bits.skip(3 * 24)?;
        let classifications = bits.read(6)? + 1;
        bits.skip(8)?;
        let mut books = 0;
        for _ in 0..classifications {
            let low = bits.read(3)?;
            let high = if bits.read(1)? == 1 { bits.read(5)? } else { 0 };
            books += ((high << 3) | low).count_ones();
        }
        bits.skip(u64::from(books) * 8)?;
        types.push(kind);
    }
    Ok(types)
}

/// Walk the mappings, as `read_mapping_type0` reads them, refusing a type 2
/// residue that decodes exactly two channels other than a pair starting at
/// channel 0: the codec's `deinterleave_2` splits the channels at the second
/// index as if it counted from the first, which reads the wrong channel or
/// past the last.
fn check_mappings(bits: &mut Bits<'_>, channels: u8, residue_types: &[u32]) -> Result<()> {
    for index in 0..bits.read(6)? + 1 {
        let kind = bits.read(16)?;
        if kind != 0 {
            bail!("OGG Vorbis mapping {index} has unknown type {kind}");
        }
        let submaps = if bits.read(1)? == 1 {
            bits.read(4)? + 1
        } else {
            1
        };
        if bits.read(1)? == 1 {
            let steps = bits.read(8)? + 1;
            let channel_bits = ilog(u32::from(channels) - 1);
            bits.skip(u64::from(steps) * 2 * u64::from(channel_bits))?;
        }
        if bits.read(2)? != 0 {
            bail!("OGG Vorbis mapping {index} sets its reserved bits");
        }
        let mut multiplex = vec![0; usize::from(channels)];
        if submaps > 1 {
            for submap in &mut multiplex {
                *submap = bits.read(4)?;
            }
        }
        for submap in 0..submaps {
            // An unused field and the floor, then the residue.
            bits.skip(8 + 8)?;
            let residue = bits.read(8)?;
            if residue_types.get(residue as usize) != Some(&2) {
                continue;
            }
            let mut decoded = (0..).zip(&multiplex).filter(|&(_, &mux)| mux == submap);
            if let (Some((first, _)), Some((second, _)), None) =
                (decoded.next(), decoded.next(), decoded.next())
                && first != 0
            {
                bail!(
                    "OGG Vorbis mapping {index} gives a type 2 residue channels \
                     {first} and {second}, which Symphonia cannot deinterleave"
                );
            }
        }
    }
    Ok(())
}

/// Bytes a codebook entry costs the codec besides its lookup table: its
/// codeword length and value, and the table builder's record of it.
const ENTRY_BYTES: usize = 32;

/// Bytes one lookup table slot costs: a 16-byte `Entry<Entry32x32>`, doubled
/// for the table's amortised growth.
const TABLE_SLOT_BYTES: usize = 32;

/// An upper bound on the bytes of the lookup table
/// `symphonia_core::io::vlc::CodebookBuilder` builds for these codeword
/// lengths.
///
/// The builder reads `width` bits at a time, the longest codeword clamped to 4
/// to 8, and gives each distinct codeword prefix at every multiple of `width`
/// bits its own block of `2^width` slots. The prefixes at depth `d` number no
/// more than the codewords longer than `d`, nor than `2^d`.
fn table_bytes(lengths: &[usize; MAX_CODEWORD_LENGTH as usize + 1]) -> usize {
    let Some(longest) = lengths.iter().rposition(|&count| count > 0) else {
        return 0;
    };
    let width = longest.clamp(4, 8);
    let mut blocks = 1;
    let mut depth = width;
    while depth < longest {
        let longer: usize = lengths[depth + 1..].iter().sum();
        blocks += longer.min(1 << depth);
        depth += width;
    }
    (blocks << width) * TABLE_SLOT_BYTES
}

/// The bit count of `value`: the Vorbis spec's `ilog`.
fn ilog(value: u32) -> u32 {
    u32::BITS - value.leading_zeros()
}

/// The largest `r` with `r ^ dimensions <= entries`: the Vorbis spec's
/// `lookup1_values`, computed exactly for `dimensions` of at least 1.
fn lookup1_values(entries: u32, dimensions: u32) -> u32 {
    let fits = |base: u32| {
        base.checked_pow(dimensions)
            .is_some_and(|power| power <= entries)
    };
    let (mut low, mut high) = (0, entries);
    while low < high {
        let mid = low + (high - low).div_ceil(2);
        if fits(mid) {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    low
}

/// A Vorbis bitpacking reader: least significant bit first within each byte.
struct Bits<'a> {
    data: &'a [u8],
    bit: u64,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, bit: 0 }
    }

    fn remaining(&self) -> u64 {
        self.data.len() as u64 * 8 - self.bit
    }

    /// Read `count` bits (at most 32) as an unsigned value.
    fn read(&mut self, count: u32) -> Result<u32> {
        debug_assert!(count <= 32);
        if u64::from(count) > self.remaining() {
            bail!("OGG Vorbis setup header is truncated");
        }
        let mut value = 0u32;
        for shift in 0..count {
            let byte = self.data[(self.bit / 8) as usize];
            value |= u32::from((byte >> (self.bit % 8)) & 1) << shift;
            self.bit += 1;
        }
        Ok(value)
    }

    fn skip(&mut self, count: u64) -> Result<()> {
        if count > self.remaining() {
            bail!("OGG Vorbis setup header is truncated");
        }
        self.bit += count;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::decode_audio_bytes;

    const SERIAL: u32 = 0x5EED;

    /// One page with a raw lacing table, its checksum filled in.
    fn raw_page(serial: u32, sequence: u32, flags: u8, lacing: &[u8], body: &[u8]) -> Vec<u8> {
        let mut page = CAPTURE.to_vec();
        page.extend_from_slice(&[0, flags]);
        page.extend_from_slice(&[0; 8]);
        page.extend_from_slice(&serial.to_le_bytes());
        page.extend_from_slice(&sequence.to_le_bytes());
        page.extend_from_slice(&[0; 4]);
        page.push(lacing.len() as u8);
        page.extend_from_slice(lacing);
        page.extend_from_slice(body);
        let crc = crc32(0, &page);
        page[22..26].copy_from_slice(&crc.to_le_bytes());
        page
    }

    /// The lacing values of one whole packet.
    fn lace(len: usize) -> Vec<u8> {
        let mut lacing = vec![u8::MAX; len / 255];
        lacing.push((len % 255) as u8);
        lacing
    }

    /// One page holding whole `packets`.
    fn page(serial: u32, sequence: u32, flags: u8, packets: &[&[u8]]) -> Vec<u8> {
        let lacing: Vec<u8> = packets
            .iter()
            .flat_map(|packet| lace(packet.len()))
            .collect();
        raw_page(serial, sequence, flags, &lacing, &packets.concat())
    }

    /// A logical stream of `serial` with each header packet on its own page.
    fn stream_of(serial: u32, packets: &[Vec<u8>]) -> Vec<u8> {
        let mut out = Vec::new();
        for (sequence, packet) in packets.iter().enumerate() {
            let flags = if sequence == 0 { FIRST_PAGE } else { 0 };
            out.extend(page(serial, sequence as u32, flags, &[packet]));
        }
        out
    }

    fn stream(packets: &[Vec<u8>]) -> Vec<u8> {
        stream_of(SERIAL, packets)
    }

    /// A mono 22,050 Hz identification header with 256/2048 blocksizes.
    fn ident() -> Vec<u8> {
        let mut packet = IDENT_HEADER.to_vec();
        packet.extend_from_slice(&0u32.to_le_bytes());
        packet.push(1);
        packet.extend_from_slice(&22_050u32.to_le_bytes());
        packet.extend_from_slice(&[0; 12]);
        packet.push(0xB8);
        packet.push(1);
        packet
    }

    /// An empty comment header, or one declaring `count` comments it lacks.
    fn comment(count: u32) -> Vec<u8> {
        let mut packet = b"\x03vorbis".to_vec();
        packet.extend_from_slice(&0u32.to_le_bytes());
        packet.extend_from_slice(&count.to_le_bytes());
        packet.push(1);
        packet
    }

    /// A Vorbis bitpacking writer, least significant bit first. A field
    /// wider than 32 bits is zero above `value`.
    #[derive(Default)]
    struct BitWriter {
        bytes: Vec<u8>,
        bit: usize,
    }

    impl BitWriter {
        fn put(&mut self, value: u32, count: u32) -> &mut Self {
            for shift in 0..count {
                if self.bit.is_multiple_of(8) {
                    self.bytes.push(0);
                }
                let last = self.bytes.last_mut().unwrap();
                let bit = value.checked_shr(shift).unwrap_or(0) & 1;
                *last |= (bit as u8) << (self.bit % 8);
                self.bit += 1;
            }
            self
        }
    }

    /// How a test codebook stores its codeword lengths.
    #[derive(Clone, Copy)]
    enum Lengths {
        /// Ordered: every entry gets one length in a single run.
        Ordered,
        /// Unordered, a five-bit length per entry.
        Dense,
        /// Unordered, a used flag per entry and a length for every other one.
        Sparse,
    }

    /// One test codebook: its shape, its lookup `kind`, and the
    /// `multiplicands` of `value_bits` each that follow a VQ lookup.
    #[derive(Clone, Copy)]
    struct Book {
        entries: u32,
        dimensions: u32,
        lengths: Lengths,
        kind: u32,
        value_bits: u32,
        multiplicands: u32,
    }

    impl Book {
        /// An ordered book whose entries all share one codeword length, a
        /// complete tree when `entries` is a power of two.
        fn ordered(entries: u32, dimensions: u32, kind: u32, multiplicands: u32) -> Self {
            Self {
                entries,
                dimensions,
                lengths: Lengths::Ordered,
                kind,
                value_bits: 1,
                multiplicands,
            }
        }

        fn write(&self, bits: &mut BitWriter) {
            bits.put(CODEBOOK_SYNC, 24)
                .put(self.dimensions, 16)
                .put(self.entries, 24);
            match self.lengths {
                Lengths::Ordered => {
                    let length = ilog(self.entries).saturating_sub(1).max(1);
                    bits.put(1, 1)
                        .put(length - 1, 5)
                        .put(self.entries, ilog(self.entries));
                }
                Lengths::Dense => {
                    bits.put(0, 1).put(0, 1);
                    for entry in 0..self.entries {
                        bits.put(entry % 32, 5);
                    }
                }
                Lengths::Sparse => {
                    bits.put(0, 1).put(1, 1);
                    for entry in 0..self.entries {
                        bits.put(entry % 2, 1);
                        if entry % 2 == 1 {
                            bits.put(entry % 32, 5);
                        }
                    }
                }
            }
            bits.put(self.kind, 4);
            if self.kind != 0 {
                bits.put(0x6000_0000, 32).put(0x6010_0000, 32);
                bits.put(self.value_bits - 1, 4).put(1, 1);
                for value in 0..self.multiplicands {
                    bits.put(value, self.value_bits);
                }
            }
        }
    }

    /// The setup fields after the codebooks: one time-domain transform, one
    /// floor, one residue and one mapping. The guard stops after the
    /// mappings, so the modes are left out.
    #[derive(Clone)]
    struct Tail {
        /// Floor 1 x positions (at most eight), each `range_bits` wide.
        xs: Vec<u32>,
        range_bits: u32,
        /// Floor 1's single subclass book number: a codebook number plus one,
        /// or 0 for none.
        subclass_book: u32,
        /// A floor 0 with this many codebooks instead of a floor 1.
        floor0_books: Option<u32>,
        residue_type: u32,
        /// The submap each channel decodes through; its length is the
        /// channel count.
        multiplex: Vec<u32>,
    }

    impl Default for Tail {
        fn default() -> Self {
            Self {
                xs: Vec::new(),
                range_bits: 8,
                subclass_book: 0,
                floor0_books: None,
                residue_type: 0,
                multiplex: vec![0],
            }
        }
    }

    impl Tail {
        fn write(&self, bits: &mut BitWriter) {
            bits.put(0, 6).put(0, 16);
            bits.put(0, 6);
            if let Some(books) = self.floor0_books {
                bits.put(0, 16)
                    .put(0, 8 + 16 + 16 + 6 + 8)
                    .put(books - 1, 4);
                for _ in 0..books {
                    bits.put(0, 8);
                }
            } else {
                bits.put(1, 16);
                if self.xs.is_empty() {
                    bits.put(0, 5);
                } else {
                    // One partition of class 0, as wide as `xs`, with one
                    // subclass.
                    bits.put(1, 5).put(0, 4);
                    bits.put(self.xs.len() as u32 - 1, 3)
                        .put(0, 2)
                        .put(self.subclass_book, 8);
                }
                bits.put(0, 2).put(self.range_bits, 4);
                for &x in &self.xs {
                    bits.put(x, self.range_bits);
                }
            }
            // One residue of one classification with no books.
            bits.put(0, 6).put(self.residue_type, 16);
            bits.put(0, 24).put(0, 24).put(0, 24).put(0, 6).put(0, 8);
            bits.put(0, 3).put(0, 1);
            // One mapping, its submaps all on floor 0 and residue 0.
            let submaps = self.multiplex.iter().max().copied().unwrap_or(0) + 1;
            bits.put(0, 6).put(0, 16);
            if submaps > 1 {
                bits.put(1, 1).put(submaps - 1, 4);
            } else {
                bits.put(0, 1);
            }
            bits.put(0, 1).put(0, 2);
            if submaps > 1 {
                for &submap in &self.multiplex {
                    bits.put(submap, 4);
                }
            }
            for _ in 0..submaps {
                bits.put(0, 8).put(0, 8).put(0, 8);
            }
        }

        fn channels(&self) -> u8 {
            self.multiplex.len() as u8
        }
    }

    /// A setup header holding `books`, then `tail`.
    fn setup_with(books: &[Book], tail: &Tail) -> Vec<u8> {
        let mut bits = BitWriter::default();
        bits.put(books.len() as u32 - 1, 8);
        for book in books {
            book.write(&mut bits);
        }
        tail.write(&mut bits);
        let mut packet = SETUP_HEADER.to_vec();
        packet.extend_from_slice(&bits.bytes);
        packet
    }

    fn setup(books: &[Book]) -> Vec<u8> {
        setup_with(books, &Tail::default())
    }

    /// Check a setup header for a mono stream.
    fn check(packet: &[u8]) -> Result<()> {
        check_setup(packet, 1)
    }

    /// Check `books` then `tail`, returning the refusal.
    fn refusal(books: &[Book], tail: &Tail) -> String {
        check_setup(&setup_with(books, tail), tail.channels())
            .unwrap_err()
            .to_string()
    }

    fn headers(books: &[Book]) -> Vec<u8> {
        stream(&[ident(), comment(0), setup(books)])
    }

    fn error(data: &[u8]) -> String {
        format!("{:#}", decode_audio_bytes("ogg", data).unwrap_err())
    }

    fn real_track() -> Vec<u8> {
        include_bytes!("../../mods/hd/bgm/013.ogg").to_vec()
    }

    /// The minimised #769 fuzz input: a comment header declaring 0x80000001
    /// comments, which lewton reserved as 48-byte entries (0x1800000030
    /// bytes) before reading the first one. Symphonia reads it safely.
    #[test]
    fn the_769_comment_count_is_an_error() {
        decode_audio_bytes("ogg", &stream(&[ident(), comment(0x8000_0001)])).unwrap_err();
    }

    /// The demuxer's `lookup1_values` divides by the dimension count and
    /// asserts on the overflowed result.
    #[test]
    fn a_codebook_with_no_dimensions_is_refused() {
        let error = error(&headers(&[Book::ordered(1, 0, 1, 0)]));
        assert!(error.contains("0 dimensions"), "{error}");
    }

    /// `(value + 1).pow(32)` overflows in the demuxer's assertion.
    #[test]
    fn a_lookup_1_book_of_32_dimensions_is_refused() {
        let error = error(&headers(&[Book::ordered(2, 32, 1, 1)]));
        assert!(error.contains("lookup-1 book of 32"), "{error}");
        check(&setup(&[Book::ordered(2, 32, 2, 64)])).unwrap();
    }

    /// Past the codec's own limit; at 2^24 - 1 entries the demuxer's float
    /// `lookup1_values` also trips its assertion.
    #[test]
    fn a_codebook_over_the_codec_s_limits_is_refused() {
        for (entries, dimensions) in [(MAX_CODEBOOK_ENTRIES + 1, 1), ((1 << 24) - 1, 3)] {
            let error = error(&headers(&[Book::ordered(entries, dimensions, 1, 1)]));
            assert!(error.contains("entries, over the"), "{error}");
        }
        let error = error(&headers(&[Book::ordered(4, 33, 0, 0)]));
        assert!(error.contains("33 dimensions"), "{error}");
    }

    /// One codebook of 2^17 17-bit codewords is within the codec's limits
    /// but makes it build a 256 MiB lookup table, and peak over 400 MiB.
    #[test]
    fn a_codebook_with_a_huge_lookup_table_is_refused() {
        let error = error(&headers(&[Book::ordered(MAX_CODEBOOK_ENTRIES, 1, 0, 0)]));
        assert!(error.contains("codebook storage"), "{error}");
    }

    /// Books each well within the budget still cannot add up past it.
    #[test]
    fn codebook_storage_is_capped_across_books() {
        let book = Book::ordered(1 << 12, 2, 1, 64);
        check(&setup(&[book])).unwrap();
        let error = error(&headers(&[book; 256]));
        assert!(error.contains("codebook storage"), "{error}");
    }

    #[test]
    fn table_bytes_bounds_the_symphonia_lookup_table() {
        let mut lengths = [0; MAX_CODEWORD_LENGTH as usize + 1];
        lengths[3] = 8;
        // One 4-bit-wide block for codewords of at most 4 bits.
        assert_eq!(table_bytes(&lengths), 16 * TABLE_SLOT_BYTES);
        lengths[3] = 0;
        lengths[17] = 1 << 17;
        // 8-bit blocks: the root, 256 at 8 bits and 65,536 at 16 bits.
        assert_eq!(
            table_bytes(&lengths),
            (1 + 256 + 65_536) * 256 * TABLE_SLOT_BYTES
        );
    }

    /// The codec's floor 1 uniqueness check misses a repeat of the seeded
    /// x position 0, then divides by the zero distance between them.
    #[test]
    fn a_floor_x_position_of_0_is_refused() {
        let book = Book::ordered(4, 1, 0, 0);
        let tail = |xs: &[u32]| Tail {
            xs: xs.to_vec(),
            ..Tail::default()
        };
        check(&setup_with(&[book], &tail(&[128, 64, 255]))).unwrap();
        let error = refusal(&[book], &tail(&[128, 0, 255]));
        assert!(error.contains("repeats the x position 0"), "{error}");
    }

    /// The codec builds a codebook with no codewords an empty lookup table,
    /// then indexes it.
    #[test]
    fn a_codebook_with_no_codewords_is_refused() {
        let empty = Book {
            lengths: Lengths::Sparse,
            ..Book::ordered(1, 1, 0, 0)
        };
        let error = refusal(&[empty], &Tail::default());
        assert!(error.contains("has no codewords"), "{error}");
    }

    /// Floor 0 audio picks its codebook with `ilog(books)` bits, which for
    /// 16 books indexes past the codec's 16-entry list.
    #[test]
    fn a_floor_0_of_16_codebooks_is_refused() {
        let book = Book::ordered(4, 1, 0, 0);
        let tail = |books| Tail {
            floor0_books: Some(books),
            ..Tail::default()
        };
        check(&setup_with(&[book], &tail(15))).unwrap();
        let error = refusal(&[book], &tail(16));
        assert!(error.contains("lists 16 codebooks"), "{error}");
    }

    /// The bound at its boundary: a 65,536-entry subclass book's Y range
    /// times the largest 15-bit x distance passes `u32::MAX`, and times the
    /// largest 14-bit one stays within it. The positions themselves are only
    /// there to make the floor valid.
    #[test]
    fn a_floor_1_y_range_that_overflows_is_refused() {
        let books = [Book::ordered(4, 1, 0, 0), Book::ordered(1 << 16, 1, 0, 0)];
        let tail = |range_bits| Tail {
            xs: vec![1, 2],
            range_bits,
            subclass_book: 2,
            ..Tail::default()
        };
        check(&setup_with(&books, &tail(14))).unwrap();
        let error = refusal(&books, &tail(15));
        assert!(error.contains("overflows the floor curve"), "{error}");
    }

    /// `deinterleave_2` treats the second channel index as relative to the
    /// first, so only a pair starting at channel 0 is safe.
    #[test]
    fn a_type_2_residue_pair_not_at_channel_0_is_refused() {
        let book = Book::ordered(4, 1, 0, 0);
        let tail = |residue_type, multiplex: &[u32]| Tail {
            residue_type,
            multiplex: multiplex.to_vec(),
            ..Tail::default()
        };
        for safe in [
            tail(2, &[0, 0]),
            tail(2, &[0, 0, 1]),
            tail(2, &[0, 0, 0]),
            tail(1, &[0, 1, 1]),
        ] {
            check_setup(&setup_with(&[book], &safe), safe.channels()).unwrap();
        }
        for multiplex in [[0, 1, 1], [1, 0, 0]] {
            let error = refusal(&[book], &tail(2, &multiplex));
            assert!(error.contains("cannot deinterleave"), "{error}");
        }
    }

    /// Before the first audio packet Symphonia feeds a repeated first page
    /// to the stream it belongs to, setup header and all.
    #[test]
    fn a_setup_on_a_repeated_first_page_is_checked() {
        let bad = setup(&[Book::ordered(1, 0, 1, 0)]);
        let mut data = page(SERIAL, 0, FIRST_PAGE, &[&ident()]);
        data.extend(page(SERIAL, 1, 0, &[&comment(0)]));
        data.extend(page(SERIAL, 2, FIRST_PAGE, &[&ident(), &bad]));
        let error = error(&data);
        assert!(error.contains("0 dimensions"), "{error}");
    }

    /// Every codeword-length layout and lookup type, with wide multiplicands,
    /// walks to the exact bit where the next codebook starts: a fifth book
    /// with a broken sync is reported as codebook 4.
    #[test]
    fn mixed_codebooks_stay_aligned() {
        let books = [
            Book {
                lengths: Lengths::Dense,
                ..Book::ordered(37, 1, 0, 0)
            },
            Book {
                lengths: Lengths::Sparse,
                value_bits: 7,
                ..Book::ordered(9, 2, 1, 3)
            },
            Book {
                value_bits: 13,
                ..Book::ordered(3, 2, 2, 6)
            },
            Book {
                lengths: Lengths::Dense,
                value_bits: 16,
                ..Book::ordered(625, 4, 1, 5)
            },
            Book::ordered(5, 1, 0, 0),
        ];
        check(&setup(&books)).unwrap();

        let mut broken = BitWriter::default();
        broken.put(books.len() as u32 - 1, 8);
        for book in &books[..4] {
            book.write(&mut broken);
        }
        broken.put(CODEBOOK_SYNC ^ 1, 24);
        let mut packet = SETUP_HEADER.to_vec();
        packet.extend_from_slice(&broken.bytes);
        let error = check(&packet).unwrap_err().to_string();
        assert!(error.contains("codebook 4 has a bad sync"), "{error}");
    }

    /// An ordered book that never reaches its entry count keeps lengthening
    /// its codewords; the spec stops them at 32 bits.
    #[test]
    fn an_ordered_book_with_overlong_codewords_is_refused() {
        let mut bits = BitWriter::default();
        bits.put(0, 8)
            .put(CODEBOOK_SYNC, 24)
            .put(1, 16)
            .put(2, 24)
            .put(1, 1)
            .put(0, 5);
        for _ in 0..40 {
            bits.put(0, 2);
        }
        let mut packet = SETUP_HEADER.to_vec();
        packet.extend_from_slice(&bits.bytes);
        let error = check(&packet).unwrap_err().to_string();
        assert!(error.contains("longer than 32 bits"), "{error}");
    }

    /// Symphonia parses a chained link's headers mid-decode, so the guard
    /// checks every link, not just the first.
    #[test]
    fn a_chained_link_s_setup_is_checked() {
        let mut data = real_track();
        data.extend(stream_of(
            0xC4A1,
            &[ident(), comment(0), setup(&[Book::ordered(1, 0, 1, 0)])],
        ));
        let error = error(&data);
        assert!(error.contains("0 dimensions"), "{error}");
    }

    #[test]
    fn too_many_logical_streams_are_refused() {
        let mut data = Vec::new();
        for serial in 0..=budget::MAX_OGG_STREAMS as u32 {
            data.extend(page(serial, 0, FIRST_PAGE, &[&ident()]));
        }
        let error = error(&data);
        assert!(error.contains("logical stream count"), "{error}");
    }

    /// Only the Vorbis mapper is vetted, so no other may run.
    #[test]
    fn a_stream_that_is_not_vorbis_is_refused() {
        let mut data = real_track();
        data.extend(page(0x0905, 0, FIRST_PAGE, &[b"OpusHead\x01\x01"]));
        let error = error(&data);
        assert!(error.contains("is not Vorbis"), "{error}");
    }

    /// Anything off the page chain is a place Symphonia would resynchronise.
    #[test]
    fn the_page_chain_must_be_clean() {
        let track = real_track();
        let mut corrupt = track.clone();
        let last = corrupt.len() - 1;
        corrupt[last] ^= 1;
        let mut trailing = track.clone();
        trailing.extend_from_slice(b"junk");
        let mut leading = b"junk".to_vec();
        leading.extend_from_slice(&track);
        let truncated = &track[..track.len() - 1];
        for (data, expected) in [
            (&corrupt[..], "fails its checksum"),
            (&trailing[..], "no page at byte"),
            (&leading[..], "no page at byte 0"),
            (truncated, "is truncated"),
        ] {
            let error = error(data);
            assert!(error.contains(expected), "{error}");
        }
    }

    #[test]
    fn a_first_page_that_continues_a_packet_is_refused() {
        let ident = ident();
        let data = raw_page(SERIAL, 0, FIRST_PAGE, &[ident.len() as u8, u8::MAX], &{
            let mut body = ident.clone();
            body.extend_from_slice(&[0; 255]);
            body
        });
        let error = check_stream(&data).unwrap_err().to_string();
        assert!(error.contains("past its first page"), "{error}");
    }

    /// The mapper re-copies its codec data for every setup header.
    #[test]
    fn a_second_setup_header_is_refused() {
        let good = setup(&[Book::ordered(4, 1, 0, 0)]);
        let data = stream(&[ident(), comment(0), good.clone(), good]);
        let error = check_stream(&data).unwrap_err().to_string();
        assert!(error.contains("second setup header"), "{error}");
    }

    /// A setup header split across pages is checked once reassembled; the
    /// packets Symphonia drops (a continuation with nothing to continue, or a
    /// partial packet before a sequence gap) are dropped here too.
    #[test]
    fn packets_are_reassembled_as_symphonia_does() {
        // A long first book pushes the bad one onto the second page.
        let long = Book {
            lengths: Lengths::Dense,
            ..Book::ordered(600, 1, 0, 0)
        };
        let bad = setup(&[long, Book::ordered(1, 0, 1, 0)]);
        let (head, tail) = bad.split_at(255);
        let start = |sequence| raw_page(SERIAL, sequence, 0, &[u8::MAX], head);
        let finish = |sequence| raw_page(SERIAL, sequence, CONTINUED, &lace(tail.len()), tail);
        let mut base = page(SERIAL, 0, FIRST_PAGE, &[&ident()]);
        base.extend(page(SERIAL, 1, 0, &[&comment(0)]));
        let assemble = |pages: &[Vec<u8>]| {
            let mut data = base.clone();
            data.extend(pages.concat());
            check_stream(&data)
        };

        let error = assemble(&[start(2), finish(3)]).unwrap_err().to_string();
        assert!(error.contains("0 dimensions"), "{error}");
        // A sequence gap drops the partial packet, and the continuation that
        // follows then has nothing to continue.
        assemble(&[start(2), finish(4)]).unwrap();
        // A page that is not a continuation drops the partial packet.
        let fresh = raw_page(SERIAL, 3, 0, &lace(tail.len()), tail);
        assemble(&[start(2), fresh]).unwrap();
        // A continuation with nothing to continue drops its first packet.
        let orphan = raw_page(SERIAL, 2, CONTINUED, &lace(bad.len()), &bad);
        assemble(&[orphan]).unwrap();
    }

    #[test]
    fn lookup1_values_matches_the_spec() {
        assert_eq!(lookup1_values(1025, 10), 2);
        assert_eq!(lookup1_values(1024, 10), 2);
        assert_eq!(lookup1_values(1023, 10), 1);
        assert_eq!(lookup1_values(3125, 5), 5);
        assert_eq!(lookup1_values(3124, 5), 4);
        assert_eq!(lookup1_values(81, 4), 3);
        assert_eq!(lookup1_values(1, 1), 1);
        assert_eq!(lookup1_values(0, 15), 0);
        assert_eq!(lookup1_values(MAX_CODEBOOK_ENTRIES, 31), 1);
    }

    #[test]
    fn the_page_checksum_is_ogg_s_crc32() {
        assert_eq!(crc32(0, b"123456789"), 0x89A1_897F);
    }

    /// The example mod's real libvorbis track passes the guard and decodes
    /// to its exact length: 0.5 s of mono 22,050 Hz.
    #[test]
    fn the_example_mod_track_still_decodes() {
        let data = real_track();
        check_stream(&data).unwrap();
        let wav = decode_audio_bytes("ogg", &data).unwrap();
        assert_eq!((wav.channels, wav.sample_rate), (1, 22_050));
        assert_eq!(wav.data.len(), 11_025 * 2);
    }
}
