//! A reader for the 7z archive Classic REbirth keeps in its `ddraw.dll`
//! (resource `DATA/107`): the English copies of RE 2's status-screen
//! textures, among others.
//!
//! Only what that archive uses is read: an LZMA-packed header, and folders
//! of one coder each (LZMA, LZMA2 or a plain copy), each holding one or more
//! files. Anything else is an error rather than a guess.

use std::io::Cursor;

use anyhow::{Context, Result, bail, ensure};

/// A 7z archive's signature.
const SIGNATURE: [u8; 6] = [b'7', b'z', 0xBC, 0xAF, 0x27, 0x1C];
/// The signature header's size: the next header's offset is from its end.
const START_HEADER: usize = 32;

const K_END: u64 = 0x00;
const K_HEADER: u64 = 0x01;
const K_ARCHIVE_PROPERTIES: u64 = 0x02;
const K_MAIN_STREAMS: u64 = 0x04;
const K_FILES: u64 = 0x05;
const K_PACK_INFO: u64 = 0x06;
const K_UNPACK_INFO: u64 = 0x07;
const K_SUB_STREAMS: u64 = 0x08;
const K_SIZE: u64 = 0x09;
const K_CRC: u64 = 0x0A;
const K_FOLDER: u64 = 0x0B;
const K_UNPACK_SIZE: u64 = 0x0C;
const K_STREAM_COUNT: u64 = 0x0D;
const K_EMPTY_STREAM: u64 = 0x0E;
const K_NAME: u64 = 0x11;
const K_ENCODED_HEADER: u64 = 0x17;

const CODER_COPY: &[u8] = &[0x00];
const CODER_LZMA: &[u8] = &[0x03, 0x01, 0x01];
const CODER_LZMA2: &[u8] = &[0x21];

/// The first 7z archive inside `data` (a PE resource is stored as is), cut
/// to the length its signature header gives.
pub fn find(data: &[u8]) -> Option<&[u8]> {
    data.windows(SIGNATURE.len())
        .enumerate()
        .filter(|(_, window)| *window == SIGNATURE)
        .find_map(|(at, _)| {
            let archive = &data[at..];
            let header = archive.get(..START_HEADER)?;
            let offset = u64::from_le_bytes(header[12..20].try_into().ok()?);
            let size = u64::from_le_bytes(header[20..28].try_into().ok()?);
            let end = (START_HEADER as u64)
                .checked_add(offset)?
                .checked_add(size)?;
            archive.get(..usize::try_from(end).ok()?)
        })
}

/// One coder's folder: where its packed stream is and what it unpacks to.
#[derive(Debug, Clone)]
struct Folder {
    coder: Vec<u8>,
    props: Vec<u8>,
    unpack_size: u64,
    crc_defined: bool,
}

/// What a streams-info block describes.
#[derive(Debug, Default)]
struct Streams {
    pack_pos: u64,
    pack_sizes: Vec<u64>,
    folders: Vec<Folder>,
    /// Per folder, the sizes of the files it holds.
    files: Vec<Vec<u64>>,
}

/// A byte reader over a header, with 7z's variable-length numbers.
struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn byte(&mut self) -> Result<u8> {
        let byte = *self.data.get(self.at).context("7z header is truncated")?;
        self.at += 1;
        Ok(byte)
    }

    fn bytes(&mut self, count: usize) -> Result<&'a [u8]> {
        let end = self.at.checked_add(count).context("7z field is too long")?;
        let bytes = self
            .data
            .get(self.at..end)
            .context("7z header is truncated")?;
        self.at = end;
        Ok(bytes)
    }

    fn number(&mut self) -> Result<u64> {
        let first = self.byte()?;
        let mut mask = 0x80u8;
        let mut value = 0u64;
        for index in 0..8 {
            if first & mask == 0 {
                let high = u64::from(first & mask.wrapping_sub(1));
                return Ok(value | high << (8 * index));
            }
            value |= u64::from(self.byte()?) << (8 * index);
            mask >>= 1;
        }
        Ok(value)
    }

    fn count(&mut self) -> Result<usize> {
        let value = self.number()?;
        let count = usize::try_from(value).context("7z count is too large")?;
        ensure!(
            count <= self.data.len(),
            "7z count {count} is past the header"
        );
        Ok(count)
    }

    /// A bit vector of `count` bits, most significant bit first.
    fn bits(&mut self, count: usize) -> Result<Vec<bool>> {
        let bytes = self.bytes(count.div_ceil(8))?;
        Ok((0..count)
            .map(|index| bytes[index / 8] & (0x80 >> (index % 8)) != 0)
            .collect())
    }

    /// A digests block for `count` streams; returns which are defined.
    fn digests(&mut self, count: usize) -> Result<Vec<bool>> {
        let defined = if self.byte()? != 0 {
            vec![true; count]
        } else {
            self.bits(count)?
        };
        self.bytes(4 * defined.iter().filter(|&&bit| bit).count())?;
        Ok(defined)
    }
}

fn read_pack_info(reader: &mut Reader<'_>, streams: &mut Streams) -> Result<()> {
    streams.pack_pos = reader.number()?;
    let count = reader.count()?;
    loop {
        match reader.number()? {
            K_END => return Ok(()),
            K_SIZE => {
                streams.pack_sizes = (0..count).map(|_| reader.number()).collect::<Result<_>>()?;
            }
            K_CRC => {
                reader.digests(count)?;
            }
            other => bail!("7z pack info has property {other:#x}"),
        }
    }
}

fn read_folder(reader: &mut Reader<'_>) -> Result<Folder> {
    let coders = reader.count()?;
    ensure!(coders == 1, "7z folder has {coders} coders, only 1 is read");
    let flags = reader.byte()?;
    let coder = reader.bytes(usize::from(flags & 0x0F))?.to_vec();
    ensure!(flags & 0x10 == 0, "7z coder has several streams");
    let props = if flags & 0x20 != 0 {
        let size = reader.count()?;
        reader.bytes(size)?.to_vec()
    } else {
        Vec::new()
    };
    Ok(Folder {
        coder,
        props,
        unpack_size: 0,
        crc_defined: false,
    })
}

fn read_unpack_info(reader: &mut Reader<'_>, streams: &mut Streams) -> Result<()> {
    ensure!(
        reader.number()? == K_FOLDER,
        "7z unpack info has no folders"
    );
    let count = reader.count()?;
    ensure!(reader.byte()? == 0, "7z folders are stored elsewhere");
    streams.folders = (0..count)
        .map(|_| read_folder(reader))
        .collect::<Result<_>>()?;
    ensure!(
        reader.number()? == K_UNPACK_SIZE,
        "7z folders have no unpacked sizes"
    );
    for folder in &mut streams.folders {
        folder.unpack_size = reader.number()?;
    }
    loop {
        match reader.number()? {
            K_END => return Ok(()),
            K_CRC => {
                let defined = reader.digests(count)?;
                for (folder, defined) in streams.folders.iter_mut().zip(defined) {
                    folder.crc_defined = defined;
                }
            }
            other => bail!("7z unpack info has property {other:#x}"),
        }
    }
}

fn read_sub_streams(reader: &mut Reader<'_>, streams: &mut Streams) -> Result<()> {
    let mut counts = vec![1usize; streams.folders.len()];
    let mut id = reader.number()?;
    if id == K_STREAM_COUNT {
        for count in &mut counts {
            *count = reader.count()?;
        }
        id = reader.number()?;
    }
    let mut sizes = Vec::with_capacity(counts.len());
    for (folder, &count) in streams.folders.iter().zip(&counts) {
        let mut files = Vec::with_capacity(count);
        if count > 0 {
            if id == K_SIZE {
                for _ in 1..count {
                    files.push(reader.number()?);
                }
            }
            let used: u64 = files.iter().sum();
            let last = folder
                .unpack_size
                .checked_sub(used)
                .context("7z files are larger than their folder")?;
            files.push(last);
        }
        sizes.push(files);
    }
    if id == K_SIZE {
        id = reader.number()?;
    }
    loop {
        match id {
            K_END => break,
            K_CRC => {
                let unknown = streams
                    .folders
                    .iter()
                    .zip(&counts)
                    .map(|(folder, &count)| {
                        if count == 1 && folder.crc_defined {
                            0
                        } else {
                            count
                        }
                    })
                    .sum();
                reader.digests(unknown)?;
            }
            other => bail!("7z sub-streams have property {other:#x}"),
        }
        id = reader.number()?;
    }
    streams.files = sizes;
    Ok(())
}

fn read_streams(reader: &mut Reader<'_>) -> Result<Streams> {
    let mut streams = Streams::default();
    let mut sub_streams = false;
    loop {
        match reader.number()? {
            K_END => break,
            K_PACK_INFO => read_pack_info(reader, &mut streams)?,
            K_UNPACK_INFO => read_unpack_info(reader, &mut streams)?,
            K_SUB_STREAMS => {
                read_sub_streams(reader, &mut streams)?;
                sub_streams = true;
            }
            other => bail!("7z streams info has property {other:#x}"),
        }
    }
    if !sub_streams {
        streams.files = streams
            .folders
            .iter()
            .map(|folder| vec![folder.unpack_size])
            .collect();
    }
    ensure!(
        streams.pack_sizes.len() == streams.folders.len(),
        "7z has {} packed streams for {} folders",
        streams.pack_sizes.len(),
        streams.folders.len()
    );
    Ok(streams)
}

/// Unpack folder `index` of `streams`.
fn unpack(archive: &[u8], streams: &Streams, index: usize) -> Result<Vec<u8>> {
    let folder = &streams.folders[index];
    let before: u64 = streams.pack_sizes[..index].iter().sum();
    let start = (START_HEADER as u64)
        .checked_add(streams.pack_pos)
        .and_then(|start| start.checked_add(before))
        .context("7z packed stream is out of range")?;
    let end = start
        .checked_add(streams.pack_sizes[index])
        .context("7z packed stream is out of range")?;
    let packed = usize::try_from(start)
        .ok()
        .zip(usize::try_from(end).ok())
        .and_then(|(start, end)| archive.get(start..end))
        .context("7z packed stream is past the archive")?;
    let size = usize::try_from(folder.unpack_size).context("7z folder is too large")?;
    let mut out = Vec::with_capacity(size);
    match folder.coder.as_slice() {
        CODER_COPY => out.extend_from_slice(packed),
        CODER_LZMA => {
            ensure!(folder.props.len() == 5, "7z LZMA coder has no properties");
            let mut input = folder.props.clone();
            input.extend_from_slice(packed);
            let options = lzma_rs::decompress::Options {
                unpacked_size: lzma_rs::decompress::UnpackedSize::UseProvided(Some(
                    folder.unpack_size,
                )),
                ..Default::default()
            };
            lzma_rs::lzma_decompress_with_options(&mut Cursor::new(input), &mut out, &options)
                .map_err(|error| anyhow::anyhow!("7z LZMA stream: {error}"))?;
        }
        CODER_LZMA2 => {
            lzma_rs::lzma2_decompress(&mut Cursor::new(packed), &mut out)
                .map_err(|error| anyhow::anyhow!("7z LZMA2 stream: {error}"))?;
        }
        coder => bail!("7z coder {coder:02x?} is not read"),
    }
    ensure!(
        out.len() == size,
        "7z folder unpacked to {} bytes, not {size}",
        out.len()
    );
    Ok(out)
}

/// The file names in a header's files info, and which have no stream.
fn read_files(reader: &mut Reader<'_>) -> Result<Vec<(String, bool)>> {
    let count = reader.count()?;
    let mut names = vec![String::new(); count];
    let mut empty = vec![false; count];
    loop {
        let property = reader.number()?;
        if property == K_END {
            break;
        }
        let size = reader.count()?;
        let data = reader.bytes(size)?;
        match property {
            K_EMPTY_STREAM => {
                empty = Reader { data, at: 0 }.bits(count)?;
            }
            K_NAME => {
                ensure!(data.first() == Some(&0), "7z names are stored elsewhere");
                let units: Vec<u16> = data[1..]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| u16::from_le_bytes(*pair))
                    .collect();
                let mut parts = units.split(|&unit| unit == 0);
                for name in &mut names {
                    let part = parts.next().context("7z has fewer names than files")?;
                    *name = String::from_utf16_lossy(part);
                }
            }
            _ => {}
        }
    }
    Ok(names.into_iter().zip(empty).collect())
}

/// Every file in `archive` whose name `wanted` accepts, by name (`/`
/// separated, as stored), unpacking only the folders that hold one.
pub fn extract(archive: &[u8], wanted: impl Fn(&str) -> bool) -> Result<Vec<(String, Vec<u8>)>> {
    ensure!(
        archive.starts_with(&SIGNATURE) && archive.len() >= START_HEADER,
        "not a 7z archive"
    );
    let offset = u64::from_le_bytes(archive[12..20].try_into()?);
    let size = u64::from_le_bytes(archive[20..28].try_into()?);
    let start = usize::try_from(offset)?
        .checked_add(START_HEADER)
        .context("7z header is out of range")?;
    let end = start
        .checked_add(usize::try_from(size)?)
        .context("7z header is out of range")?;
    let mut header = archive
        .get(start..end)
        .context("7z header is past the archive")?
        .to_vec();
    let mut reader = Reader {
        data: &header,
        at: 0,
    };
    if reader.number()? == K_ENCODED_HEADER {
        let streams = read_streams(&mut reader)?;
        ensure!(streams.folders.len() == 1, "7z encoded header is in pieces");
        header = unpack(archive, &streams, 0).context("7z encoded header")?;
        reader = Reader {
            data: &header,
            at: 0,
        };
        ensure!(
            reader.number()? == K_HEADER,
            "7z encoded header is no header"
        );
    }
    let mut streams = None;
    let mut files = Vec::new();
    loop {
        match reader.number()? {
            K_END => break,
            K_ARCHIVE_PROPERTIES => loop {
                if reader.number()? == K_END {
                    break;
                }
                let size = reader.count()?;
                reader.bytes(size)?;
            },
            K_MAIN_STREAMS => streams = Some(read_streams(&mut reader)?),
            K_FILES => files = read_files(&mut reader)?,
            other => bail!("7z header has property {other:#x}"),
        }
    }
    let streams = streams.unwrap_or_default();
    // Files with a stream take the folders' streams in order.
    let mut slots = streams
        .files
        .iter()
        .enumerate()
        .flat_map(|(folder, sizes)| sizes.iter().map(move |&size| (folder, size)));
    let mut cache: Option<(usize, Vec<u8>)> = None;
    let mut out = Vec::new();
    let mut at_folder = usize::MAX;
    let mut offset = 0u64;
    for (name, empty) in files {
        if empty {
            continue;
        }
        let (folder, size) = slots.next().context("7z has more files than streams")?;
        if folder != at_folder {
            at_folder = folder;
            offset = 0;
        }
        let start = offset;
        offset += size;
        let name = name.replace('\\', "/");
        if !wanted(&name) {
            continue;
        }
        if cache.as_ref().is_none_or(|(index, _)| *index != folder) {
            let data = unpack(archive, &streams, folder).with_context(|| format!("7z {name}"))?;
            cache = Some((folder, data));
        }
        let data = &cache.as_ref().expect("cached above").1;
        let range = usize::try_from(start)?..usize::try_from(offset)?;
        let bytes = data.get(range).context("7z file is past its folder")?;
        out.push((name, bytes.to_vec()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Write `value` as a 7z number.
    fn number(out: &mut Vec<u8>, value: u64) {
        assert!(value < 0x80, "the fixtures only need one-byte numbers");
        out.push(value as u8);
    }

    /// A stored (copy-coder) archive of `files`, one folder each, with a
    /// plain header.
    fn stored(files: &[(&str, &[u8])]) -> Vec<u8> {
        let packed: Vec<u8> = files.iter().flat_map(|(_, data)| data.to_vec()).collect();
        let mut header = vec![K_HEADER as u8, K_MAIN_STREAMS as u8, K_PACK_INFO as u8];
        number(&mut header, 0);
        number(&mut header, files.len() as u64);
        header.push(K_SIZE as u8);
        for (_, data) in files {
            number(&mut header, data.len() as u64);
        }
        header.push(K_END as u8);
        header.extend([K_UNPACK_INFO as u8, K_FOLDER as u8]);
        number(&mut header, files.len() as u64);
        header.push(0);
        for _ in files {
            header.extend([1, 0x01, 0x00]);
        }
        header.push(K_UNPACK_SIZE as u8);
        for (_, data) in files {
            number(&mut header, data.len() as u64);
        }
        header.extend([K_END as u8, K_END as u8, K_FILES as u8]);
        number(&mut header, files.len() as u64);
        let mut names = vec![0u8];
        for (name, _) in files {
            for unit in name.encode_utf16().chain([0]) {
                names.extend(unit.to_le_bytes());
            }
        }
        header.push(K_NAME as u8);
        number(&mut header, names.len() as u64);
        header.extend(names);
        header.extend([K_END as u8, K_END as u8]);

        let mut archive = SIGNATURE.to_vec();
        archive.extend([0, 4, 0, 0, 0, 0]);
        archive.extend((packed.len() as u64).to_le_bytes());
        archive.extend((header.len() as u64).to_le_bytes());
        archive.extend([0; 4]);
        archive.extend(packed);
        archive.extend(header);
        archive
    }

    #[test]
    fn extracts_the_wanted_files_of_a_stored_archive() {
        let archive = stored(&[("common\\data\\a.tim", b"alpha"), ("b.bin", b"be")]);
        let mut dll = b"MZ padding".to_vec();
        dll.extend(&archive);
        dll.extend(b"trailing resource bytes");
        let found = find(&dll).expect("the archive is found");
        assert_eq!(found, archive.as_slice());
        let files = extract(found, |name| name.ends_with(".tim")).unwrap();
        assert_eq!(
            files,
            vec![("common/data/a.tim".to_owned(), b"alpha".to_vec())]
        );
        let all = extract(found, |_| true).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[1], ("b.bin".to_owned(), b"be".to_vec()));
    }

    #[test]
    fn reads_seven_zip_numbers() {
        let mut reader = Reader {
            data: &[0x7F, 0x81, 0x02, 0xC0, 0x34, 0x12],
            at: 0,
        };
        assert_eq!(reader.number().unwrap(), 0x7F);
        assert_eq!(reader.number().unwrap(), 0x102);
        assert_eq!(reader.number().unwrap(), 0x1234);
    }

    #[test]
    fn rejects_other_data() {
        assert!(find(b"no archive here").is_none());
        assert!(extract(b"7z but not quite", |_| true).is_err());
    }
}
