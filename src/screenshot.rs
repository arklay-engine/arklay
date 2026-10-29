//! F12 screenshots: the presented game frame saved as a PNG.
//!
//! A screenshot is the frame the game drew, without the window's letterbox
//! bars or the overlays drawn over it (the debug HUD, its notices and the
//! in-game menu). The GPU renderer's frame is read back at its render scale;
//! the software renderer's 320x240 frame is enlarged by the whole factor the
//! window shows it at, nearest neighbour, so a screenshot is never smaller
//! than what was on screen. Only the band a wider aspect presents is kept.
//!
//! Screenshots go to `screenshots/` under the primary data root (see
//! [`crate::paths`]), named by the local time they were taken:
//! `arklay_2026-10-29_14-03-22_123.png`. The PNG is encoded and written on
//! a background thread so the game loop never waits for the disk.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::local_time::LocalTime;

/// The directory under a data root that screenshots are written to.
pub const DIR_NAME: &str = "screenshots";

/// The largest whole factor a software frame is enlarged by.
const MAX_SCALE: u32 = 8;

/// An RGBA image, row by row from the top.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Image {
    /// Wrap `rgba`, checking it holds exactly `width` x `height` pixels.
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Result<Self> {
        if width == 0 || height == 0 || rgba.len() != width as usize * height as usize * 4 {
            bail!("a {width}x{height} screenshot of {} bytes", rgba.len());
        }
        Ok(Self {
            width,
            height,
            rgba,
        })
    }

    /// Rows `top..top + height`, clamped to the image.
    pub fn rows(&self, top: u32, height: u32) -> Self {
        let top = top.min(self.height - 1);
        let height = height.clamp(1, self.height - top);
        let stride = self.width as usize * 4;
        let start = top as usize * stride;
        Self {
            width: self.width,
            height,
            rgba: self.rgba[start..start + height as usize * stride].to_vec(),
        }
    }

    /// The image enlarged `scale` times, nearest neighbour.
    pub fn enlarged(&self, scale: u32) -> Self {
        let scale = scale.clamp(1, MAX_SCALE);
        if scale == 1 {
            return self.clone();
        }
        let width = self.width * scale;
        let mut rgba = Vec::with_capacity(self.rgba.len() * (scale * scale) as usize);
        for row in self.rgba.chunks_exact(self.width as usize * 4) {
            let start = rgba.len();
            for pixel in row.as_chunks::<4>().0 {
                for _ in 0..scale {
                    rgba.extend_from_slice(pixel);
                }
            }
            for _ in 1..scale {
                rgba.extend_from_within(start..start + width as usize * 4);
            }
        }
        Self {
            width,
            height: self.height * scale,
            rgba,
        }
    }

    /// The image as an opaque RGB PNG (the frame's alpha is not coverage).
    pub fn encode_png(&self) -> Result<Vec<u8>> {
        let rgb: Vec<u8> = self
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
            .collect();
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, self.width, self.height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        let mut writer = encoder.write_header().context("PNG header")?;
        writer.write_image_data(&rgb).context("PNG image data")?;
        writer.finish().context("PNG end")?;
        Ok(bytes)
    }
}

/// The directory screenshots are written to.
pub fn dir() -> PathBuf {
    crate::paths::primary_data_dir().join(DIR_NAME)
}

/// The file name for a screenshot taken at `time`; without a reading, the
/// seconds since the Unix epoch.
pub fn file_name(time: Option<LocalTime>) -> String {
    match time {
        Some(t) => format!(
            "arklay_{:04}-{:02}-{:02}_{:02}-{:02}-{:02}_{:03}.png",
            t.year, t.month, t.day, t.hour, t.minute, t.second, t.millisecond
        ),
        None => {
            let seconds = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |since| since.as_secs());
            format!("arklay_{seconds}.png")
        }
    }
}

/// A path in `dir` named `name` that does not exist yet: `name` itself, or
/// with `-2`, `-3`, ... before the extension.
pub fn unused_path(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    if !path.exists() {
        return path;
    }
    let (stem, extension) = name.rsplit_once('.').unwrap_or((name, ""));
    (2u32..)
        .map(|n| match extension {
            "" => dir.join(format!("{stem}-{n}")),
            _ => dir.join(format!("{stem}-{n}.{extension}")),
        })
        .find(|path| !path.exists())
        .expect("an unused screenshot name")
}

/// Encode `image` and write it into `dir` under a new name, returning the
/// path written.
pub fn save_to(dir: &Path, image: &Image, time: Option<LocalTime>) -> Result<PathBuf> {
    std::fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;
    let bytes = image.encode_png()?;
    let path = unused_path(dir, &file_name(time));
    crate::atomic::write(&path, &bytes)?;
    Ok(path)
}

/// Save `image` to [`dir`] on a background thread, named by the time now,
/// returning the file name it will have. The outcome is logged.
pub fn save_in_background(image: Image) -> String {
    let time = crate::local_time::now();
    let name = file_name(time);
    std::thread::spawn(move || match save_to(&dir(), &image, time) {
        Ok(path) => eprintln!("screenshot saved to {}", path.display()),
        Err(err) => eprintln!("warning: screenshot not saved: {err:#}"),
    });
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `width` x `height` image whose pixel `(x, y)` is `[x, y, 7, 0]`.
    fn gradient(width: u32, height: u32) -> Image {
        let rgba = (0..height)
            .flat_map(|y| (0..width).flat_map(move |x| [x as u8, y as u8, 7, 0]))
            .collect();
        Image::new(width, height, rgba).unwrap()
    }

    fn pixel(image: &Image, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * image.width + x) * 4) as usize;
        image.rgba[i..i + 4].try_into().unwrap()
    }

    #[test]
    fn a_mis_sized_image_is_rejected() {
        assert!(Image::new(2, 2, vec![0; 15]).is_err());
        assert!(Image::new(0, 2, Vec::new()).is_err());
    }

    #[test]
    fn rows_keep_the_band_and_clamp_to_the_image() {
        let image = gradient(4, 10);
        let band = image.rows(2, 3);
        assert_eq!((band.width, band.height), (4, 3));
        assert_eq!(pixel(&band, 3, 0), [3, 2, 7, 0]);
        assert_eq!(pixel(&band, 1, 2), [1, 4, 7, 0]);
        let clamped = image.rows(8, 5);
        assert_eq!(clamped.height, 2);
        assert_eq!(pixel(&clamped, 0, 1), [0, 9, 7, 0]);
    }

    #[test]
    fn enlarging_repeats_each_pixel_in_a_block() {
        let image = gradient(3, 2);
        let big = image.enlarged(3);
        assert_eq!((big.width, big.height), (9, 6));
        assert_eq!(big.rgba.len(), 9 * 6 * 4);
        for y in 0..6 {
            for x in 0..9 {
                assert_eq!(pixel(&big, x, y), pixel(&image, x / 3, y / 3));
            }
        }
        assert_eq!(image.enlarged(0), image);
        assert_eq!(image.enlarged(100).width, 3 * MAX_SCALE);
    }

    #[test]
    fn the_png_decodes_back_to_the_opaque_pixels() {
        let image = gradient(5, 4);
        let bytes = image.encode_png().unwrap();
        let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
            .read_info()
            .unwrap();
        let mut out = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut out).unwrap();
        assert_eq!((info.width, info.height), (5, 4));
        assert_eq!(info.color_type, png::ColorType::Rgb);
        assert_eq!(&out[..3], &[0, 0, 7]);
        let last = (4 * 5 - 1) * 3;
        assert_eq!(&out[last..last + 3], &[4, 3, 7]);
    }

    #[test]
    fn names_carry_the_local_time() {
        let time = LocalTime {
            year: 2026,
            month: 3,
            day: 4,
            hour: 5,
            minute: 6,
            second: 7,
            millisecond: 8,
        };
        assert_eq!(file_name(Some(time)), "arklay_2026-03-04_05-06-07_008.png");
        let fallback = file_name(None);
        assert!(fallback.starts_with("arklay_") && fallback.ends_with(".png"));
    }

    #[test]
    fn saving_never_overwrites_an_earlier_screenshot() {
        let dir =
            std::env::temp_dir().join(format!("arklay-screenshot-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let image = gradient(2, 2);
        let first = save_to(&dir, &image, None).unwrap();
        let second = save_to(&dir, &image, None).unwrap();
        assert_ne!(first, second);
        assert!(first.is_file() && second.is_file());
        assert_eq!(
            std::fs::read(&first).unwrap()[..8],
            [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
