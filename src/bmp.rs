//! BMP encode/decode.
//!
//! Encoding picks 8-bit indexed when the image has at most 256 unique RGB
//! colors and 24-bit BGR otherwise. Rows are stored bottom-up, padded to a
//! 4-byte boundary, and alpha is discarded.
//!
//! Magenta (`#FF00FF`, [`TRANSPARENT`]) is the transparent colour of every
//! BMP: decoding makes its texels transparent black. An 8-bit BMP whose palette
//! entry 0 is magenta is an indexed page ([`is_indexed_page`]): its texel
//! indices and palette order are meaningful, index 0 is the transparent
//! entry and each palette entry's reserved byte carries its STP bit.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};

use crate::budget;
use crate::model::Texture8;
use crate::state::Image;

/// The transparent colour of every BMP.
pub const TRANSPARENT: [u8; 3] = [0xFF, 0x00, 0xFF];

const FILE_HEADER_LEN: usize = 14;
const DIB_HEADER_LEN: usize = 40;

/// Encode an RGBA8 image as a BMP file at `path`.
pub fn encode(image: &Image, path: &Path) -> Result<()> {
    let data = encode_to_vec(image)?;
    std::fs::write(path, data)?;
    Ok(())
}

/// Decode a bottom-up 8-bit indexed or 24-bit BMP into an RGBA8 image.
///
/// Magenta texels decode as transparent black and every other texel is
/// opaque.
/// Top-down images, compressed data, and other bit depths are rejected.
pub fn decode(data: &[u8]) -> Result<Image> {
    decode_with_key(data, None)
}

/// Decode a room-mask page, cutting exact black `(0, 0, 0)` texels out.
///
/// A mask page's key colour is black: texels of that colour are holes and
/// every other texel is opaque. The converter preserves the page's colours
/// but rebuilds its palette in first-use order, so the key has to be matched
/// by colour rather than by palette index.
pub fn decode_mask(data: &[u8]) -> Result<Image> {
    decode_with_key(data, Some([0, 0, 0]))
}

/// Decode a BMP into RGBA8, giving magenta texels and texels equal to `key`
/// (when a key is supplied) a zero alpha. Every other texel is opaque.
fn decode_with_key(data: &[u8], key: Option<[u8; 3]>) -> Result<Image> {
    if data.len() < 2 || &data[0..2] != b"BM" {
        bail!("not a BMP file: bad magic");
    }
    if data.len() < FILE_HEADER_LEN {
        bail!(
            "truncated BMP: file header is {} bytes (expected {FILE_HEADER_LEN})",
            data.len()
        );
    }
    let pixel_offset = u32_at(data, 10) as usize;

    if data.len() < FILE_HEADER_LEN + 4 {
        bail!("truncated BMP: missing DIB header");
    }
    let dib_size = u32_at(data, 14) as usize;
    if dib_size < DIB_HEADER_LEN {
        bail!("unsupported BMP DIB header size {dib_size} (expected at least {DIB_HEADER_LEN})");
    }
    if data.len() < FILE_HEADER_LEN + dib_size {
        bail!(
            "truncated BMP: DIB header needs {dib_size} bytes, have {}",
            data.len() - FILE_HEADER_LEN
        );
    }

    let width = i32_at(data, 18);
    let height = i32_at(data, 22);
    let planes = u16_at(data, 26);
    let bpp = u16_at(data, 28);
    let compression = u32_at(data, 30);
    let colors_used = u32_at(data, 46);

    if width <= 0 || height <= 0 {
        bail!("unsupported BMP dimensions {width}x{height} (must be positive, bottom-up)");
    }
    if planes != 1 {
        bail!("unsupported BMP planes {planes} (expected 1)");
    }
    if compression != 0 {
        bail!("unsupported BMP compression {compression} (expected 0)");
    }
    if bpp != 8 && bpp != 24 {
        bail!("unsupported BMP bit depth {bpp} (expected 8 or 24)");
    }

    let dib_end = FILE_HEADER_LEN + dib_size;
    let palette_len = if colors_used > 0 {
        colors_used as usize
    } else if pixel_offset > dib_end {
        (pixel_offset - dib_end) / 4
    } else {
        0
    };

    let mut palette = Vec::new();
    if bpp == 8 {
        for entry in data
            .get(dib_end..)
            .unwrap_or(&[])
            .as_chunks::<4>()
            .0
            .iter()
            .take(palette_len.min(256))
        {
            palette.push([entry[2], entry[1], entry[0]]);
        }
    }

    let width = width as usize;
    let height = height as usize;
    let pixel_count = width
        .checked_mul(height)
        .context("BMP dimensions overflow")?;
    budget::check_len(pixel_count, budget::MAX_PIXELS, "BMP pixel count")?;
    let row_bytes = width * bpp as usize / 8;
    let row_stride = (row_bytes + 3) & !3;
    let image_size = row_stride
        .checked_mul(height)
        .ok_or_else(|| anyhow!("BMP dimensions {width}x{height} are too large"))?;
    let end = pixel_offset
        .checked_add(image_size)
        .ok_or_else(|| anyhow!("BMP pixel data offset overflows"))?;
    if end > data.len() {
        bail!(
            "truncated BMP: pixel data ends at {end}, file has {} bytes",
            data.len()
        );
    }

    let pixels = &data[pixel_offset..end];
    let rgba_len = pixel_count
        .checked_mul(4)
        .context("BMP RGBA buffer overflows")?;
    let mut rgba = budget::alloc::<u8>(rgba_len, "BMP RGBA buffer")?;
    rgba.resize(rgba_len, 0);
    for y in 0..height {
        let src = &pixels[(height - 1 - y) * row_stride..];
        let dst = &mut rgba[y * width * 4..(y + 1) * width * 4];
        for x in 0..width {
            let (r, g, b) = if bpp == 8 {
                let index = src[x] as usize;
                let color = palette.get(index).ok_or_else(|| {
                    anyhow!(
                        "BMP palette index {index} out of range (palette has {} entries)",
                        palette.len()
                    )
                })?;
                (color[0], color[1], color[2])
            } else {
                (src[x * 3 + 2], src[x * 3 + 1], src[x * 3])
            };
            let pixel = if [r, g, b] == TRANSPARENT {
                [0, 0, 0, 0]
            } else if key.is_some_and(|key| [r, g, b] == key) {
                [r, g, b, 0]
            } else {
                [r, g, b, 255]
            };
            dst[x * 4..x * 4 + 4].copy_from_slice(&pixel);
        }
    }

    Ok(Image {
        width: width as u32,
        height: height as u32,
        rgba,
    })
}

/// Encode an image to BMP bytes.
pub fn encode_to_vec(image: &Image) -> Result<Vec<u8>> {
    let width = image.width as usize;
    let height = image.height as usize;
    let expected = width * height * 4;
    if image.rgba.len() != expected {
        bail!(
            "rgba buffer has {} bytes but a {}x{} image needs {expected}",
            image.rgba.len(),
            image.width,
            image.height
        );
    }

    let mut palette: Vec<[u8; 3]> = Vec::new();
    let mut indices: HashMap<[u8; 3], u8> = HashMap::new();
    let mut indexed = true;
    for pixel in image.rgba.as_chunks::<4>().0 {
        let rgb = [pixel[0], pixel[1], pixel[2]];
        if let Entry::Vacant(slot) = indices.entry(rgb) {
            if palette.len() == 256 {
                indexed = false;
                break;
            }
            slot.insert(palette.len() as u8);
            palette.push(rgb);
        }
    }

    let bpp: u16 = if indexed { 8 } else { 24 };
    let palette_len = if indexed { palette.len() } else { 0 };
    let row_bytes = width * bpp as usize / 8;
    let row_stride = (row_bytes + 3) & !3;
    let image_size = row_stride * height;
    let pixel_offset = FILE_HEADER_LEN + DIB_HEADER_LEN + palette_len * 4;
    let file_size = pixel_offset + image_size;

    let mut out = Vec::with_capacity(file_size);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(file_size as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(pixel_offset as u32).to_le_bytes());
    out.extend_from_slice(&(DIB_HEADER_LEN as u32).to_le_bytes());
    out.extend_from_slice(&(image.width as i32).to_le_bytes());
    out.extend_from_slice(&(image.height as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&bpp.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(image_size as u32).to_le_bytes());
    out.extend_from_slice(&0i32.to_le_bytes());
    out.extend_from_slice(&0i32.to_le_bytes());
    out.extend_from_slice(&(palette_len as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());

    if indexed {
        for rgb in &palette {
            out.extend_from_slice(&[rgb[2], rgb[1], rgb[0], 0]);
        }
    }

    for y in (0..height).rev() {
        let row = &image.rgba[y * width * 4..(y + 1) * width * 4];
        if indexed {
            for pixel in row.as_chunks::<4>().0 {
                out.push(indices[&[pixel[0], pixel[1], pixel[2]]]);
            }
        } else {
            for pixel in row.as_chunks::<4>().0 {
                out.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
            }
        }
        out.resize(out.len() + (row_stride - row_bytes), 0);
    }

    Ok(out)
}

/// Encode an 8bpp indexed texture through its first CLUT row as a BMP.
///
/// Mask pages carry one 256-colour CLUT row, so row 0 is the whole palette.
/// The palette is rebuilt in first-use order exactly like the generic encoder,
/// but a per-index lookup keeps the pixel pass free of colour hashing.
pub fn encode_texture8_to_vec(texture: &Texture8) -> Result<Vec<u8>> {
    let width = texture.width as usize;
    let height = texture.height as usize;
    let expected = width * height;
    if texture.indices.len() != expected {
        bail!(
            "texture has {} indices but a {}x{} image needs {expected}",
            texture.indices.len(),
            texture.width,
            texture.height
        );
    }

    let mut lut = [u16::MAX; 256];
    let mut palette: Vec<[u8; 3]> = Vec::new();
    let mut colors: HashMap<[u8; 3], u8> = HashMap::new();
    for &index in &texture.indices {
        let slot = &mut lut[index as usize];
        if *slot != u16::MAX {
            continue;
        }
        let rgba = texture.palette(0, index);
        let rgb = [rgba[0], rgba[1], rgba[2]];
        let assigned = match colors.entry(rgb) {
            Entry::Occupied(color) => *color.get(),
            Entry::Vacant(color) => {
                let Ok(assigned) = u8::try_from(palette.len()) else {
                    bail!("texture uses more than 256 colours");
                };
                color.insert(assigned);
                palette.push(rgb);
                assigned
            }
        };
        *slot = u16::from(assigned);
    }

    let palette_len = palette.len();
    let row_bytes = width;
    let row_stride = (row_bytes + 3) & !3;
    let image_size = row_stride * height;
    let pixel_offset = FILE_HEADER_LEN + DIB_HEADER_LEN + palette_len * 4;
    let file_size = pixel_offset + image_size;

    let mut out = Vec::with_capacity(file_size);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(file_size as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(pixel_offset as u32).to_le_bytes());
    out.extend_from_slice(&(DIB_HEADER_LEN as u32).to_le_bytes());
    out.extend_from_slice(&(texture.width as i32).to_le_bytes());
    out.extend_from_slice(&(texture.height as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&8u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(image_size as u32).to_le_bytes());
    out.extend_from_slice(&0i32.to_le_bytes());
    out.extend_from_slice(&0i32.to_le_bytes());
    out.extend_from_slice(&(palette_len as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());

    for rgb in &palette {
        out.extend_from_slice(&[rgb[2], rgb[1], rgb[0], 0]);
    }

    for y in (0..height).rev() {
        let row = &texture.indices[y * width..(y + 1) * width];
        for &index in row {
            out.push(lut[index as usize] as u8);
        }
        out.resize(out.len() + (row_stride - row_bytes), 0);
    }

    Ok(out)
}

/// Encode an 8bpp indexed texture page as a BMP that keeps its texel indices.
///
/// The palette holds `row` of the page's CLUT as 256 entries; the reserved
/// palette byte carries each entry's STP bit, so [`decode_indexed_page`]
/// restores the exact page, including transparency. A black entry 0, the
/// page's transparent entry, is written as magenta so any decoder sees it as
/// transparent and [`is_indexed_page`] recognises the page. Unlike
/// [`encode_texture8_to_vec`], indices are not merged or renumbered: a model
/// page's UVs and its index-0 holes depend on them.
pub fn encode_indexed_page(texture: &Texture8, row: usize) -> Result<Vec<u8>> {
    let width = texture.width as usize;
    let height = texture.height as usize;
    let expected = width * height;
    if texture.indices.len() != expected {
        bail!(
            "texture has {} indices but a {}x{} image needs {expected}",
            texture.indices.len(),
            texture.width,
            texture.height
        );
    }

    let palette_len = 256usize;
    let row_bytes = width;
    let row_stride = (row_bytes + 3) & !3;
    let image_size = row_stride
        .checked_mul(height)
        .ok_or_else(|| anyhow!("BMP dimensions {width}x{height} are too large"))?;
    let pixel_offset = FILE_HEADER_LEN + DIB_HEADER_LEN + palette_len * 4;
    let file_size = pixel_offset + image_size;

    let mut out = Vec::with_capacity(file_size);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(file_size as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(pixel_offset as u32).to_le_bytes());
    out.extend_from_slice(&(DIB_HEADER_LEN as u32).to_le_bytes());
    out.extend_from_slice(&(texture.width as i32).to_le_bytes());
    out.extend_from_slice(&(texture.height as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&8u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(image_size as u32).to_le_bytes());
    out.extend_from_slice(&0i32.to_le_bytes());
    out.extend_from_slice(&0i32.to_le_bytes());
    out.extend_from_slice(&(palette_len as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());

    for index in 0..=255u8 {
        let [mut r, mut g, mut b, _] = texture.palette(row, index);
        if index == 0 && [r, g, b] == [0, 0, 0] {
            [r, g, b] = TRANSPARENT;
        }
        let stp = u8::from(texture.palette_stp(row, index));
        out.extend_from_slice(&[b, g, r, stp]);
    }

    for y in (0..height).rev() {
        let row = &texture.indices[y * width..(y + 1) * width];
        out.extend_from_slice(row);
        out.resize(out.len() + (row_stride - row_bytes), 0);
    }

    Ok(out)
}

/// Decode an 8bpp indexed BMP produced by [`encode_indexed_page`].
///
/// The palette becomes one 256-entry CLUT row and each palette entry's
/// reserved byte is read back as its STP flag. A magenta entry 0 is read back
/// as the black it was written for. A 24-bit or shorter-palette
/// file is refused: only an indexed page can preserve a model's texel
/// indices.
pub fn decode_indexed_page(data: &[u8]) -> Result<Texture8> {
    if data.len() < 2 || &data[0..2] != b"BM" {
        bail!("not a BMP file: bad magic");
    }
    if data.len() < FILE_HEADER_LEN + DIB_HEADER_LEN {
        bail!("truncated BMP: missing file or DIB header");
    }
    let pixel_offset = u32_at(data, 10) as usize;
    let dib_size = u32_at(data, 14) as usize;
    if dib_size < DIB_HEADER_LEN {
        bail!("unsupported BMP DIB header size {dib_size} (expected at least {DIB_HEADER_LEN})");
    }
    let dib_end = FILE_HEADER_LEN + dib_size;
    if data.len() < dib_end {
        bail!("truncated BMP: DIB header ends past the file");
    }

    let width = i32_at(data, 18);
    let height = i32_at(data, 22);
    let planes = u16_at(data, 26);
    let bpp = u16_at(data, 28);
    let compression = u32_at(data, 30);
    if width <= 0 || height <= 0 {
        bail!("unsupported BMP dimensions {width}x{height} (must be positive, bottom-up)");
    }
    if planes != 1 {
        bail!("unsupported BMP planes {planes} (expected 1)");
    }
    if compression != 0 {
        bail!("unsupported BMP compression {compression} (expected 0)");
    }
    if bpp != 8 {
        bail!("model page BMP is {bpp}-bit (expected an 8-bit indexed page)");
    }

    let width = width as usize;
    let height = height as usize;
    let pixel_count = width
        .checked_mul(height)
        .context("BMP dimensions overflow")?;
    budget::check_len(pixel_count, budget::MAX_PIXELS, "BMP pixel count")?;
    let row_stride = (width + 3) & !3;
    let image_size = row_stride
        .checked_mul(height)
        .ok_or_else(|| anyhow!("BMP dimensions {width}x{height} are too large"))?;
    let end = pixel_offset
        .checked_add(image_size)
        .ok_or_else(|| anyhow!("BMP pixel data offset overflows"))?;
    if end > data.len() {
        bail!(
            "truncated BMP: pixel data ends at {end}, file has {} bytes",
            data.len()
        );
    }
    if pixel_offset < dib_end {
        bail!("BMP pixel data starts inside its header");
    }
    let entries = (pixel_offset - dib_end) / 4;
    if entries < 256 {
        bail!("model page BMP palette has {entries} entries (expected 256)");
    }
    let palette = data.get(dib_end..dib_end + 256 * 4).unwrap();
    let mut palettes = Vec::with_capacity(256);
    let mut stp = Vec::with_capacity(256);
    for (index, entry) in palette.as_chunks::<4>().0.iter().enumerate() {
        let rgb = [entry[2], entry[1], entry[0]];
        let [r, g, b] = if index == 0 && rgb == TRANSPARENT {
            [0, 0, 0]
        } else {
            rgb
        };
        palettes.push([r, g, b, 255]);
        stp.push(entry[3] != 0);
    }

    let mut indices = budget::alloc::<u8>(pixel_count, "BMP indexed page")?;
    indices.resize(pixel_count, 0);
    for y in 0..height {
        let src = &data[pixel_offset + (height - 1 - y) * row_stride..][..width];
        indices[y * width..(y + 1) * width].copy_from_slice(src);
    }

    Ok(Texture8 {
        width: width as u32,
        height: height as u32,
        indices,
        palettes,
        stp,
    })
}

/// Whether `data` is an indexed page: an uncompressed 8-bit BMP with a full
/// 256-entry palette whose entry 0 is magenta, as [`encode_indexed_page`]
/// writes it. Such a page decodes through [`decode_indexed_page`] so its
/// indices, palette order and STP bits survive.
pub fn is_indexed_page(data: &[u8]) -> bool {
    if data.len() < FILE_HEADER_LEN + DIB_HEADER_LEN || &data[0..2] != b"BM" {
        return false;
    }
    let pixel_offset = u32_at(data, 10) as usize;
    let dib_end = FILE_HEADER_LEN + u32_at(data, 14) as usize;
    if u16_at(data, 28) != 8 || u32_at(data, 30) != 0 || pixel_offset < dib_end + 256 * 4 {
        return false;
    }
    data.get(dib_end..dib_end + 3)
        .is_some_and(|entry| [entry[2], entry[1], entry[0]] == TRANSPARENT)
}

fn u16_at(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

fn u32_at(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

fn i32_at(data: &[u8], offset: usize) -> i32 {
    i32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn build(width: u32, height: u32, mut pixel: impl FnMut(u32, u32) -> [u8; 4]) -> Image {
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                rgba.extend_from_slice(&pixel(x, y));
            }
        }
        Image {
            width,
            height,
            rgba,
        }
    }

    fn few_colors(width: u32, height: u32) -> Image {
        const PALETTE: [[u8; 3]; 4] = [[10, 20, 30], [200, 40, 60], [7, 180, 90], [250, 250, 250]];
        build(width, height, |x, y| {
            let c = PALETTE[((x + y * 3) % 4) as usize];
            [c[0], c[1], c[2], 255]
        })
    }

    fn many_colors(width: u32, height: u32) -> Image {
        build(width, height, |x, y| {
            let i = y * width + x;
            [i as u8, (i >> 8) as u8, 0, 255]
        })
    }

    fn temp_path(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("arklay-bmp-{}-{name}.bmp", std::process::id()));
        let _ = std::fs::create_dir_all(path.parent().unwrap());
        path
    }

    fn pixel_offset(data: &[u8]) -> usize {
        u32_at(data, 10) as usize
    }

    #[test]
    fn indexed_roundtrip_with_padded_rows() {
        let image = few_colors(5, 3);
        let path = temp_path("indexed-padded");
        encode(&image, &path).unwrap();
        let data = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(u16_at(&data, 28), 8);
        let decoded = decode(&data).unwrap();
        assert_eq!(decoded.width, image.width);
        assert_eq!(decoded.height, image.height);
        assert_eq!(decoded.rgba, image.rgba);
    }

    #[test]
    fn truecolor_roundtrip_with_over_256_colors() {
        let image = many_colors(17, 16);
        let data = encode_to_vec(&image).unwrap();
        assert_eq!(u16_at(&data, 28), 24);
        let decoded = decode(&data).unwrap();
        assert_eq!(decoded.rgba, image.rgba);
    }

    #[test]
    fn rows_are_stored_bottom_up() {
        let image = many_colors(16, 17);
        let data = encode_to_vec(&image).unwrap();
        assert_eq!(u16_at(&data, 28), 24);

        let stride = 16 * 3;
        let first_file_row = pixel_offset(&data);
        let last_file_row = first_file_row + 16 * stride;
        let mut expected_first = Vec::new();
        let mut expected_last = Vec::new();
        for x in 0..16usize {
            let top = &image.rgba[x * 4..x * 4 + 3];
            let bottom = &image.rgba[16 * 16 * 4 + x * 4..16 * 16 * 4 + x * 4 + 3];
            expected_first.extend_from_slice(&[bottom[2], bottom[1], bottom[0]]);
            expected_last.extend_from_slice(&[top[2], top[1], top[0]]);
        }
        assert_eq!(
            &data[first_file_row..first_file_row + stride],
            expected_first.as_slice()
        );
        assert_eq!(
            &data[last_file_row..last_file_row + stride],
            expected_last.as_slice()
        );
    }

    #[test]
    fn palette_uses_first_occurrence_order() {
        let image = build(4, 1, |x, _| match x {
            0 => [0, 0, 255, 255],
            1 => [255, 0, 0, 255],
            2 => [0, 255, 0, 255],
            _ => [255, 0, 0, 255],
        });
        let data = encode_to_vec(&image).unwrap();
        assert_eq!(u16_at(&data, 28), 8);
        assert_eq!(u32_at(&data, 46), 3);

        let palette = FILE_HEADER_LEN + DIB_HEADER_LEN;
        assert_eq!(&data[palette..palette + 4], &[255, 0, 0, 0]);
        assert_eq!(&data[palette + 4..palette + 8], &[0, 0, 255, 0]);
        assert_eq!(&data[palette + 8..palette + 12], &[0, 255, 0, 0]);
    }

    #[test]
    fn decodes_foreign_24bpp_fixture() {
        let data: [u8; 70] = [
            0x42, 0x4D, 0x46, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x36, 0x00, 0x00, 0x00,
            0x28, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x01, 0x00,
            0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0x00,
            0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0xFF, 0x00, 0xFF, 0x00, 0x00, 0x00,
        ];

        let image = decode(&data).unwrap();

        let expected: Vec<u8> = [
            [255, 0, 0, 255],
            [0, 255, 0, 255],
            [0, 0, 255, 255],
            [255, 255, 255, 255],
        ]
        .concat();
        assert_eq!((image.width, image.height), (2, 2));
        assert_eq!(image.rgba, expected);
    }

    #[test]
    fn decodes_foreign_8bpp_fixture_with_inferred_palette() {
        let data: [u8; 70] = [
            0x42, 0x4D, 0x46, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x3E, 0x00, 0x00, 0x00,
            0x28, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x01, 0x00,
            0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0xFF, 0x00, 0x00, 0xFF, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
        ];

        let image = decode(&data).unwrap();

        let expected: Vec<u8> = [
            [255, 0, 0, 255],
            [0, 255, 0, 255],
            [0, 255, 0, 255],
            [255, 0, 0, 255],
        ]
        .concat();
        assert_eq!((image.width, image.height), (2, 2));
        assert_eq!(image.rgba, expected);
    }

    #[test]
    fn encodes_an_indexed_texture_through_its_first_palette_row() {
        let mut palettes = vec![[0u8; 4]; 256];
        palettes[0] = [10, 20, 30, 255];
        palettes[1] = [200, 100, 50, 255];
        let texture = Texture8 {
            width: 3,
            height: 1,
            indices: vec![0, 1, 0],
            palettes,
            stp: Vec::new(),
        };

        let data = encode_texture8_to_vec(&texture).unwrap();
        let image = decode(&data).unwrap();

        assert_eq!((image.width, image.height), (3, 1));
        let expected: Vec<u8> =
            [[10, 20, 30, 255], [200, 100, 50, 255], [10, 20, 30, 255]].concat();
        assert_eq!(image.rgba, expected);
    }

    #[test]
    fn texture8_encoding_matches_the_generic_rgba_path() {
        let mut palettes = vec![[0u8; 4]; 256];
        palettes[0] = [10, 20, 30, 255];
        palettes[1] = [200, 100, 50, 255];
        palettes[2] = [10, 20, 30, 255];
        let texture = Texture8 {
            width: 4,
            height: 2,
            indices: vec![2, 1, 0, 2, 1, 1, 0, 0],
            palettes,
            stp: Vec::new(),
        };

        let mut rgba = Vec::with_capacity(texture.indices.len() * 4);
        for &index in &texture.indices {
            rgba.extend_from_slice(&texture.palette(0, index));
        }
        let legacy = encode_to_vec(&Image {
            width: texture.width,
            height: texture.height,
            rgba,
        })
        .unwrap();

        assert_eq!(encode_texture8_to_vec(&texture).unwrap(), legacy);
    }

    #[test]
    fn mask_decode_makes_exact_black_transparent_regardless_of_palette_order() {
        // The first pixel is red, so the encoder's first-use palette starts
        // with red and the black entry lands at index 1 - the converter really
        // does produce such pages.
        let image = build(4, 1, |x, _| match x {
            0 => [200, 30, 40, 255],
            1 | 2 => [0, 0, 0, 255],
            _ => [10, 220, 60, 255],
        });
        let data = encode_to_vec(&image).unwrap();

        let mask = decode_mask(&data).unwrap();
        let expected: Vec<u8> = [
            [200, 30, 40, 255],
            [0, 0, 0, 0],
            [0, 0, 0, 0],
            [10, 220, 60, 255],
        ]
        .concat();
        assert_eq!(mask.rgba, expected);

        // The plain decoder still reports every texel opaque.
        let plain = decode(&data).unwrap();
        assert!(
            plain
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| pixel[3] == 255)
        );
    }

    #[test]
    fn mask_decode_leaves_near_black_opaque() {
        let image = build(2, 1, |x, _| match x {
            0 => [0, 0, 0, 255],
            _ => [0, 0, 8, 255],
        });
        let data = encode_to_vec(&image).unwrap();

        let mask = decode_mask(&data).unwrap();
        assert_eq!(&mask.rgba[0..4], &[0, 0, 0, 0]);
        assert_eq!(&mask.rgba[4..8], &[0, 0, 8, 255]);
    }

    #[test]
    fn indexed_page_round_trips_indices_palette_and_stp() {
        let mut palettes = vec![[0u8; 4]; 256];
        palettes[1] = [10, 20, 30, 255];
        palettes[7] = [200, 100, 50, 255];
        palettes[255] = [1, 2, 3, 255];
        let mut stp = vec![false; 256];
        stp[7] = true;
        stp[255] = true;
        let texture = Texture8 {
            width: 3,
            height: 2,
            indices: vec![0, 7, 255, 1, 0, 7],
            palettes,
            stp,
        };

        let data = encode_indexed_page(&texture, 0).unwrap();
        assert_eq!(&data[0..2], b"BM");
        assert_eq!(u16_at(&data, 28), 8, "the page is an 8-bit BMP");
        let decoded = decode_indexed_page(&data).unwrap();
        assert_eq!(decoded.width, 3);
        assert_eq!(decoded.height, 2);
        assert_eq!(decoded.indices, texture.indices, "indices are preserved");
        assert_eq!(decoded.palette(0, 0), [0, 0, 0, 255]);
        assert_eq!(decoded.palette(0, 7), [200, 100, 50, 255]);
        assert_eq!(decoded.palette(0, 255), [1, 2, 3, 255]);
        assert!(!decoded.palette_stp(0, 0));
        assert!(decoded.palette_stp(0, 7));
        assert!(decoded.palette_stp(0, 255));
        assert_eq!(decoded.palettes.len(), 256);
        assert_eq!(decoded.stp.len(), 256);
    }

    #[test]
    fn magenta_is_transparent_and_marks_an_indexed_page() {
        let mut palettes = vec![[0, 0, 0, 255]; 256];
        palettes[1] = [255, 0, 0, 255];
        let page = Texture8 {
            width: 2,
            height: 1,
            indices: vec![0, 1],
            palettes,
            stp: (0..256).map(|index| index == 0).collect(),
        };
        let data = encode_indexed_page(&page, 0).unwrap();
        assert!(is_indexed_page(&data));
        // Entry 0 is stored as magenta and read back as the black it was.
        let decoded = decode_indexed_page(&data).unwrap();
        assert_eq!(decoded.palette(0, 0), [0, 0, 0, 255]);
        assert!(decoded.palette_stp(0, 0));
        assert_eq!(decoded.indices, page.indices);
        // A true-colour decode cuts the magenta texel out.
        let image = decode(&data).unwrap();
        assert_eq!(image.rgba, [0, 0, 0, 0, 255, 0, 0, 255]);

        // A generic BMP is not an indexed page, but its magenta is still
        // transparent.
        let opaque = encode_to_vec(&build(2, 1, |x, _| {
            if x == 0 {
                [0, 0, 0, 255]
            } else {
                [255, 0, 255, 255]
            }
        }))
        .unwrap();
        assert!(!is_indexed_page(&opaque));
        assert_eq!(decode(&opaque).unwrap().rgba, [0, 0, 0, 255, 0, 0, 0, 0]);
    }

    #[test]
    fn indexed_page_rejects_a_direct_colour_bmp() {
        let mut many = Vec::new();
        for index in 0..300u32 {
            many.extend_from_slice(&[(index & 0xFF) as u8, (index >> 8) as u8, 0, 255]);
        }
        let data24 = encode_to_vec(&Image {
            width: 300,
            height: 1,
            rgba: many,
        })
        .unwrap();
        assert_eq!(u16_at(&data24, 28), 24);
        assert!(decode_indexed_page(&data24).is_err());
    }

    #[test]
    fn indexed_page_rejects_a_short_palette() {
        let mut data = encode_indexed_page(
            &Texture8 {
                width: 1,
                height: 1,
                indices: vec![0],
                palettes: vec![[0u8; 4]; 256],
                stp: vec![false; 256],
            },
            0,
        )
        .unwrap();
        // Shrink the palette by moving the pixel data over its last entry.
        let colors_used = u32_at(&data, 46);
        let pixel_offset = u32_at(&data, 10);
        assert_eq!(colors_used, 256);
        let shorter = (pixel_offset - 4) as usize;
        data.drain(shorter..shorter + 4);
        data[10..14].copy_from_slice(&((pixel_offset - 4) as u32).to_le_bytes());
        let file_size = u32_at(&data, 2);
        data[2..6].copy_from_slice(&(file_size - 4).to_le_bytes());
        assert!(decode_indexed_page(&data).is_err());
    }

    #[test]
    fn encode_texture_rejects_wrong_index_len() {
        let texture = Texture8 {
            width: 2,
            height: 2,
            indices: vec![0; 3],
            palettes: vec![[0u8; 4]; 256],
            stp: Vec::new(),
        };
        assert!(encode_texture8_to_vec(&texture).is_err());
    }

    #[test]
    fn encode_rejects_wrong_rgba_len() {
        let image = Image {
            width: 2,
            height: 2,
            rgba: vec![0; 15],
        };
        assert!(encode(&image, &temp_path("bad-len")).is_err());
    }

    #[test]
    fn decode_rejects_bad_magic() {
        let mut data = encode_to_vec(&few_colors(2, 2)).unwrap();
        data[0] = b'X';
        let err = decode(&data).unwrap_err();
        assert!(err.to_string().contains("magic"), "{err}");
    }

    #[test]
    fn decode_rejects_unsupported_bpp() {
        let mut data = encode_to_vec(&few_colors(2, 2)).unwrap();
        data[28..30].copy_from_slice(&4u16.to_le_bytes());
        let err = decode(&data).unwrap_err();
        assert!(err.to_string().contains("bit depth"), "{err}");
    }

    #[test]
    fn decode_rejects_unsupported_compression() {
        let mut data = encode_to_vec(&few_colors(2, 2)).unwrap();
        data[30..34].copy_from_slice(&1u32.to_le_bytes());
        let err = decode(&data).unwrap_err();
        assert!(err.to_string().contains("compression"), "{err}");
    }

    #[test]
    fn decode_rejects_dimensions_over_the_pixel_cap() {
        let mut data = encode_to_vec(&few_colors(2, 2)).unwrap();
        data[18..22].copy_from_slice(&i32::MAX.to_le_bytes());
        data[22..26].copy_from_slice(&2i32.to_le_bytes());
        let message = budget::assert_cap_error(decode(&data));
        assert!(message.contains("pixel count"), "{message}");
    }

    #[test]
    fn decode_rejects_truncated_data() {
        let data = encode_to_vec(&few_colors(5, 3)).unwrap();
        assert!(decode(&[]).is_err());
        assert!(decode(&data[..10]).is_err());
        assert!(decode(&data[..30]).is_err());
        let err = decode(&data[..data.len() - 1]).unwrap_err();
        assert!(err.to_string().contains("truncated"), "{err}");
    }
}
