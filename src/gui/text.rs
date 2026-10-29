//! TrueType text for the GUI, rasterised at the exact pixel size drawn.
//!
//! Faces are embedded so the GUI works before (and without) any pack. Glyphs
//! rasterise lazily the first time a `(face, size, char)` is drawn and stay
//! cached, so a resize only costs the glyphs the next frame actually uses and
//! a CJK string never rasterises a whole atlas. Sizes are whole pixels: the
//! theme rounds its scaled sizes, so the cache stays small and each face is
//! drawn at a size its hinting-free outlines still look even at.
//!
//! The embedded faces cover Latin (and the retro face Cyrillic). A character
//! they lack (Cyrillic in the modern face, Japanese in both) is drawn from
//! the first installed system face that has it; those load on first need,
//! once per process, and [`warm`] can load them ahead on a worker thread.

use std::collections::HashMap;
use std::sync::{LazyLock, OnceLock};

use anyhow::{Result, anyhow};
use fontdue::{Font, FontSettings};

use super::canvas::{Canvas, Color};

/// Barlow, the modern face; see `assets/Barlow-LICENSE`.
const BARLOW_REGULAR: &[u8] = include_bytes!("../../assets/Barlow-Regular.ttf");
const BARLOW_BOLD: &[u8] = include_bytes!("../../assets/Barlow-Bold.ttf");
/// Departure Mono, the retro face; see `assets/DepartureMono-LICENSE`.
const DEPARTURE_MONO: &[u8] = include_bytes!("../../assets/DepartureMono-Regular.otf");

/// System faces tried, in order, for a character the embedded regular and
/// mono faces lack.
#[cfg(windows)]
const FALLBACK_REGULAR: &[&str] = &[
    "Fonts/segoeui.ttf",
    "Fonts/YuGothM.ttc",
    "Fonts/meiryo.ttc",
    "Fonts/msgothic.ttc",
    "Fonts/arial.ttf",
];
/// As [`FALLBACK_REGULAR`], for the bold face (which then falls back to the
/// regular list).
#[cfg(windows)]
const FALLBACK_BOLD: &[&str] = &[
    "Fonts/segoeuib.ttf",
    "Fonts/YuGothB.ttc",
    "Fonts/meiryob.ttc",
];
#[cfg(target_os = "macos")]
const FALLBACK_REGULAR: &[&str] = &[
    "/System/Library/Fonts/Helvetica.ttc",
    "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
    "/System/Library/Fonts/Hiragino Sans GB.ttc",
    "/Library/Fonts/Arial Unicode.ttf",
];
#[cfg(target_os = "macos")]
const FALLBACK_BOLD: &[&str] = &["/System/Library/Fonts/ヒラギノ角ゴシック W6.ttc"];
#[cfg(not(any(windows, target_os = "macos")))]
const FALLBACK_REGULAR: &[&str] = &[
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/opentype/ipafont-gothic/ipag.ttf",
];
#[cfg(not(any(windows, target_os = "macos")))]
const FALLBACK_BOLD: &[&str] = &[
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc",
    "/usr/share/fonts/noto-cjk/NotoSansCJK-Bold.ttc",
    "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Bold.ttc",
];

/// A system face's path and the face, parsed on first use; a face that is
/// missing or does not parse stays `None`.
type Slot = (&'static str, OnceLock<Option<Font>>);

/// The fallback faces, bold then regular.
static FALLBACKS: LazyLock<Vec<Slot>> = LazyLock::new(|| {
    FALLBACK_BOLD
        .iter()
        .chain(FALLBACK_REGULAR)
        .map(|&path| (path, OnceLock::new()))
        .collect()
});

/// A fallback face by its [`FALLBACKS`] index, loading it on first use.
fn fallback(index: usize) -> Option<&'static Font> {
    let (path, slot) = &FALLBACKS[index];
    slot.get_or_init(|| {
        let path = if cfg!(windows) {
            let root = std::env::var_os("WINDIR").unwrap_or_else(|| r"C:\Windows".into());
            std::path::Path::new(&root).join(path)
        } else {
            std::path::PathBuf::from(path)
        };
        let bytes = std::fs::read(path).ok()?;
        Font::from_bytes(bytes, FontSettings::default()).ok()
    })
    .as_ref()
}

/// The first fallback face for `face` that has `ch`.
fn fallback_for(face: Face, ch: char) -> Option<&'static Font> {
    let first = match face {
        Face::Bold => 0,
        Face::Regular | Face::Mono => FALLBACK_BOLD.len(),
    };
    (first..FALLBACKS.len())
        .filter_map(fallback)
        .find(|font| font.lookup_glyph_index(ch) != 0)
}

/// Load, on a worker thread, the fallback faces `text` needs that the
/// embedded faces lack, so the first frame drawing it does not stall.
pub fn warm(text: String) {
    let Ok(fonts) = Fonts::new() else {
        return;
    };
    let missing: Vec<(Face, char)> = text
        .chars()
        .filter(|&ch| !ch.is_whitespace() && !ch.is_control())
        .flat_map(|ch| [Face::Regular, Face::Bold, Face::Mono].map(|face| (face, ch)))
        .filter(|&(face, ch)| fonts.font(face).lookup_glyph_index(ch) == 0)
        .collect();
    if missing.is_empty() {
        return;
    }
    std::thread::spawn(move || {
        for (face, ch) in missing {
            fallback_for(face, ch);
        }
    });
}

/// One embedded face.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Face {
    Regular,
    Bold,
    Mono,
}

/// How a run of text sits on its anchor x.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// A face at a pixel size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Style {
    pub face: Face,
    pub px: u32,
}

impl Style {
    pub const fn new(face: Face, px: u32) -> Self {
        Self { face, px }
    }
}

/// One rasterised glyph.
struct Glyph {
    width: usize,
    height: usize,
    /// Offset of the bitmap's left edge from the pen.
    left: i32,
    /// Offset of the bitmap's bottom edge above the baseline.
    bottom: i32,
    advance: f32,
    bitmap: std::sync::Arc<[u8]>,
}

/// Vertical metrics of a style, in pixels.
#[derive(Debug, Clone, Copy)]
pub struct LineMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub line_height: f32,
}

/// The embedded faces and their glyph cache.
pub struct Fonts {
    faces: [Font; 3],
    glyphs: HashMap<(Style, char), Glyph>,
    /// Light-on-dark text reads thin with linear coverage; this lifts the
    /// mid-tones so small sizes keep their weight.
    gamma: [u8; 256],
}

impl Fonts {
    pub fn new() -> Result<Self> {
        let load = |bytes: &[u8], name: &str| {
            Font::from_bytes(bytes, FontSettings::default())
                .map_err(|err| anyhow!("embedded {name} font: {err}"))
        };
        let mut gamma = [0u8; 256];
        for (index, value) in gamma.iter_mut().enumerate() {
            *value = ((index as f32 / 255.0).powf(1.0 / 1.35) * 255.0).round() as u8;
        }
        Ok(Self {
            faces: [
                load(BARLOW_REGULAR, "Barlow Regular")?,
                load(BARLOW_BOLD, "Barlow Bold")?,
                load(DEPARTURE_MONO, "Departure Mono")?,
            ],
            glyphs: HashMap::new(),
            gamma,
        })
    }

    fn font(&self, face: Face) -> &Font {
        &self.faces[face as usize]
    }

    /// Whether an embedded face or a system fallback has every visible
    /// character of `text`.
    pub fn covers(&self, text: &str) -> bool {
        text.chars()
            .filter(|&ch| !ch.is_whitespace() && !ch.is_control())
            .all(|ch| {
                self.font(Face::Regular).lookup_glyph_index(ch) != 0
                    || fallback_for(Face::Regular, ch).is_some()
            })
    }

    /// Drop every cached glyph (a resize changed every size in use).
    pub fn clear_cache(&mut self) {
        self.glyphs.clear();
    }

    pub fn metrics(&self, style: Style) -> LineMetrics {
        let px = style.px as f32;
        match self.font(style.face).horizontal_line_metrics(px) {
            Some(metrics) => LineMetrics {
                ascent: metrics.ascent,
                descent: -metrics.descent,
                line_height: metrics.new_line_size,
            },
            None => LineMetrics {
                ascent: px * 0.8,
                descent: px * 0.2,
                line_height: px * 1.2,
            },
        }
    }

    fn glyph(&mut self, style: Style, ch: char) -> &Glyph {
        let font = &self.faces[style.face as usize];
        let gamma = &self.gamma;
        self.glyphs.entry((style, ch)).or_insert_with(|| {
            let font = match font.lookup_glyph_index(ch) {
                0 if !ch.is_whitespace() && !ch.is_control() => {
                    fallback_for(style.face, ch).unwrap_or(font)
                }
                _ => font,
            };
            let (metrics, bitmap) = font.rasterize(ch, style.px as f32);
            Glyph {
                width: metrics.width,
                height: metrics.height,
                left: metrics.xmin,
                bottom: metrics.ymin,
                advance: metrics.advance_width,
                bitmap: bitmap.into_iter().map(|c| gamma[c as usize]).collect(),
            }
        })
    }

    fn kern(&self, style: Style, left: char, right: char) -> f32 {
        self.font(style.face)
            .horizontal_kern(left, right, style.px as f32)
            .unwrap_or(0.0)
    }

    /// The advance width of `text` on one line.
    pub fn measure(&mut self, style: Style, text: &str) -> f32 {
        let mut width = 0.0;
        let mut previous = None;
        for ch in text.chars() {
            if let Some(left) = previous {
                width += self.kern(style, left, ch);
            }
            width += self.glyph(style, ch).advance;
            previous = Some(ch);
        }
        width
    }

    /// Draw one line of `text` with its baseline at `baseline`, aligned on
    /// `x`. Returns the drawn width.
    #[allow(clippy::too_many_arguments)] // a text run's full placement
    pub fn draw(
        &mut self,
        canvas: &mut Canvas,
        style: Style,
        x: f32,
        baseline: f32,
        align: Align,
        color: Color,
        text: &str,
    ) -> f32 {
        let width = self.measure(style, text);
        let mut pen = match align {
            Align::Left => x,
            Align::Center => x - width / 2.0,
            Align::Right => x - width,
        };
        let baseline = baseline.round() as i32;
        let mut previous = None;
        for ch in text.chars() {
            if let Some(left) = previous {
                pen += self.kern(style, left, ch);
            }
            let glyph = self.glyph(style, ch);
            if glyph.width > 0 {
                let gx = pen.round() as i32 + glyph.left;
                let gy = baseline - glyph.bottom - glyph.height as i32;
                canvas.draw_mask(gx, gy, glyph.width, glyph.height, &glyph.bitmap, color);
            }
            pen += glyph.advance;
            previous = Some(ch);
        }
        width
    }

    /// Draw `text` vertically centred on `center_y` (by the style's cap
    /// band rather than its full line box, so mixed sizes line up).
    #[allow(clippy::too_many_arguments)]
    pub fn draw_centered(
        &mut self,
        canvas: &mut Canvas,
        style: Style,
        x: f32,
        center_y: f32,
        align: Align,
        color: Color,
        text: &str,
    ) -> f32 {
        let baseline = center_y + self.cap_height(style) / 2.0;
        self.draw(canvas, style, x, baseline, align, color, text)
    }

    /// The height of a capital letter, for optical vertical centring.
    pub fn cap_height(&mut self, style: Style) -> f32 {
        let glyph = self.glyph(style, 'H');
        (glyph.height as i32 + glyph.bottom) as f32
    }

    /// Break `text` into lines no wider than `max_width`, at spaces where it
    /// can (a single word wider than the line is split by character).
    /// Explicit `\n`s always break.
    pub fn wrap(&mut self, style: Style, text: &str, max_width: f32) -> Vec<String> {
        let mut lines = Vec::new();
        for paragraph in text.split('\n') {
            let mut line = String::new();
            for word in paragraph.split(' ') {
                let candidate = if line.is_empty() {
                    word.to_string()
                } else {
                    format!("{line} {word}")
                };
                if self.measure(style, &candidate) <= max_width {
                    line = candidate;
                    continue;
                }
                if !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                }
                // A word that does not fit on its own breaks by character.
                let mut piece = String::new();
                for ch in word.chars() {
                    piece.push(ch);
                    if self.measure(style, &piece) > max_width && piece.chars().count() > 1 {
                        piece.pop();
                        lines.push(std::mem::take(&mut piece));
                        piece.push(ch);
                    }
                }
                line = piece;
            }
            lines.push(line);
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measure_grows_with_size() {
        let mut fonts = Fonts::new().unwrap();
        let small = fonts.measure(Style::new(Face::Regular, 16), "Options");
        let large = fonts.measure(Style::new(Face::Regular, 32), "Options");
        assert!(small > 0.0);
        assert!((large / small - 2.0).abs() < 0.1);
    }

    #[test]
    fn wrap_breaks_at_spaces() {
        let mut fonts = Fonts::new().unwrap();
        let style = Style::new(Face::Regular, 20);
        let width = fonts.measure(style, "Change the default");
        let lines = fonts.wrap(style, "Change the default movement.", width + 1.0);
        assert_eq!(lines, vec!["Change the default", "movement."]);
    }

    #[test]
    fn draw_marks_pixels() {
        let mut fonts = Fonts::new().unwrap();
        let mut canvas = Canvas::new(64, 32);
        fonts.draw(
            &mut canvas,
            Style::new(Face::Bold, 24),
            2.0,
            24.0,
            Align::Left,
            Color::WHITE,
            "Hi",
        );
        assert!(canvas.rgba.chunks(4).any(|pixel| pixel[3] > 0));
    }
}
