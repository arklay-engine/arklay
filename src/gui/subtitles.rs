//! Subtitles: a voiced line's text drawn at the window's native resolution,
//! centred along the bottom edge in the style the Language › Subtitles
//! options pick.
//!
//! The painter knows nothing about where a line comes from. Its owner hands
//! it a [`Subtitle`]: the lines on screen and, when the data names one, who
//! speaks them. RE1 builds these from the message window's voiced lines
//! ([`crate::message::MessageWindow::is_subtitle`]); another game's subtitle
//! data only has to produce the same two fields.

use super::canvas::{Color, Rect};
use super::text::Align;
use super::theme::Role;
use super::widgets::Painter;
use crate::config::{self, SubtitleSize, TextColor};

/// One subtitle on screen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Subtitle {
    /// Who speaks the line, when the data says.
    pub speaker: Option<String>,
    /// The text, one entry per authored line; the painter wraps a line that
    /// is too wide for the screen.
    pub lines: Vec<String>,
}

/// How subtitles look: the subtitle options of `[game]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubtitleStyle {
    pub size: SubtitleSize,
    /// The opacity of the band behind the text, 0..=100 percent.
    pub background: u8,
    pub color: TextColor,
    pub speaker_names: bool,
    pub speaker_color: TextColor,
}

impl Default for SubtitleStyle {
    fn default() -> Self {
        Self::from(&config::Game::default())
    }
}

impl From<&config::Game> for SubtitleStyle {
    fn from(game: &config::Game) -> Self {
        Self {
            size: game.subtitle_size,
            background: game.subtitle_background.min(100),
            color: game.subtitle_color,
            speaker_names: game.speaker_names,
            speaker_color: game.speaker_name_color,
        }
    }
}

impl SubtitleSize {
    /// The factor on the subtitle text's size.
    pub fn scale(self) -> f32 {
        match self {
            SubtitleSize::Small => 0.8,
            SubtitleSize::Standard => 1.0,
            SubtitleSize::Large => 1.25,
        }
    }
}

/// The colour a subtitle colour option draws.
pub fn text_color(color: TextColor) -> Color {
    match color {
        TextColor::White => Color::rgb(0xF2, 0xF2, 0xF2),
        TextColor::Red => Color::rgb(0xFF, 0x6A, 0x6A),
        TextColor::Blue => Color::rgb(0x78, 0xB4, 0xFF),
        TextColor::Green => Color::rgb(0x84, 0xE0, 0x84),
        TextColor::Yellow => Color::rgb(0xFF, 0xE0, 0x5C),
    }
}

/// The gap between the screen's bottom edge and the last line, in units.
const BOTTOM_MARGIN: f32 = 72.0;
/// The share of the screen's width a line may take before it wraps.
const MAX_WIDTH: f32 = 0.8;
/// The band's padding around a line, in units.
const PAD_X: f32 = 22.0;
const PAD_Y: f32 = 6.0;
/// Below this background opacity the text gets a drop shadow.
const SHADOW_BELOW: u8 = 60;

/// Paint `subtitle` along the bottom of `p`'s canvas. `p.theme` should carry
/// the subtitle size as its text scale (see [`SubtitleSize::scale`]).
pub fn draw(p: &mut Painter<'_>, subtitle: &Subtitle, style: &SubtitleStyle) {
    let speaker = subtitle
        .speaker
        .as_deref()
        .filter(|name| style.speaker_names && !name.is_empty())
        .map(|name| format!("{name}: "));
    let text_style = p.style(Role::Subtitle);
    let speaker_style = p.style(Role::Speaker);
    let max_width = p.theme.width * MAX_WIDTH;
    // Wrap with the speaker's prefix in place, so the first line keeps room
    // for it; the prefix is split back off when the line is drawn.
    let mut rows: Vec<(Option<String>, String)> = Vec::new();
    for (index, line) in subtitle.lines.iter().enumerate() {
        let prefix = speaker.as_ref().filter(|_| index == 0);
        let full = match prefix {
            Some(prefix) => format!("{prefix}{line}"),
            None => line.clone(),
        };
        for (row, wrapped) in p
            .fonts
            .wrap(text_style, &full, max_width)
            .into_iter()
            .enumerate()
        {
            match prefix {
                Some(prefix) if row == 0 && wrapped.starts_with(prefix.as_str()) => {
                    let rest = wrapped[prefix.len()..].to_string();
                    rows.push((Some(prefix.trim_end().to_string()), rest));
                }
                _ => rows.push((None, wrapped)),
            }
        }
    }
    if rows.is_empty() {
        return;
    }
    let metrics = p.fonts.metrics(text_style);
    let line_h = metrics.line_height + 2.0 * p.u(PAD_Y);
    let space = p.fonts.measure(text_style, " ");
    let shadow = (style.background < SHADOW_BELOW).then(|| p.u(2.0).max(1.0));
    let band = Color::rgba(0, 0, 0, (u32::from(style.background) * 255 / 100) as u8);
    let center_x = p.theme.width / 2.0;
    let mut top = p.theme.height - p.u(BOTTOM_MARGIN) - line_h * rows.len() as f32;
    for (name, text) in &rows {
        let name_w = name
            .as_deref()
            .map_or(0.0, |name| p.fonts.measure(speaker_style, name) + space);
        let width = name_w + p.fonts.measure(text_style, text);
        let left = center_x - width / 2.0;
        if style.background > 0 {
            let pad_x = p.u(PAD_X);
            p.canvas.fill_rect(
                Rect::new(left - pad_x, top, width + 2.0 * pad_x, line_h),
                band,
            );
        }
        let baseline = top + p.u(PAD_Y) + metrics.ascent;
        let mut runs = Vec::new();
        if let Some(name) = name {
            runs.push((
                speaker_style,
                left,
                text_color(style.speaker_color),
                name.as_str(),
            ));
        }
        runs.push((
            text_style,
            left + name_w,
            text_color(style.color),
            text.as_str(),
        ));
        for (run_style, x, color, run) in runs {
            if let Some(offset) = shadow {
                p.fonts.draw(
                    p.canvas,
                    run_style,
                    x + offset,
                    baseline + offset,
                    Align::Left,
                    Color::rgba(0, 0, 0, 0xC0),
                    run,
                );
            }
            p.fonts
                .draw(p.canvas, run_style, x, baseline, Align::Left, color, run);
        }
        top += line_h;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::canvas::Canvas;
    use crate::gui::text::Fonts;
    use crate::gui::theme::{FontFamily, Theme};

    fn paint(subtitle: &Subtitle, style: &SubtitleStyle) -> Canvas {
        let mut canvas = Canvas::new(640, 480);
        let mut fonts = Fonts::new().unwrap();
        let theme = Theme::new(640, 480, FontFamily::Modern).with_text_scale(style.size.scale());
        let mut p = Painter {
            canvas: &mut canvas,
            fonts: &mut fonts,
            theme,
        };
        draw(&mut p, subtitle, style);
        canvas
    }

    /// The rows (top, bottom) any pixel was painted on.
    fn painted_rows(canvas: &Canvas) -> Option<(usize, usize)> {
        let width = canvas.width as usize * 4;
        let rows: Vec<usize> = canvas
            .rgba
            .chunks(width)
            .enumerate()
            .filter(|(_, row)| row.chunks(4).any(|px| px[3] != 0))
            .map(|(index, _)| index)
            .collect();
        Some((*rows.first()?, *rows.last()?))
    }

    fn line(text: &str) -> Subtitle {
        Subtitle {
            speaker: None,
            lines: vec![text.to_string()],
        }
    }

    #[test]
    fn a_subtitle_sits_along_the_bottom() {
        let canvas = paint(&line("Wait!"), &SubtitleStyle::default());
        let (top, bottom) = painted_rows(&canvas).expect("something is painted");
        assert!(top > 380, "the line starts in the bottom fifth: {top}");
        assert!(bottom < 470, "and keeps clear of the edge: {bottom}");
    }

    #[test]
    fn nothing_is_painted_for_no_lines() {
        let canvas = paint(&Subtitle::default(), &SubtitleStyle::default());
        assert_eq!(painted_rows(&canvas), None);
    }

    #[test]
    fn the_size_option_scales_the_text() {
        let height = |size| {
            let style = SubtitleStyle {
                size,
                background: 0,
                ..SubtitleStyle::default()
            };
            let (top, bottom) = painted_rows(&paint(&line("Hello"), &style)).unwrap();
            bottom - top
        };
        assert!(height(SubtitleSize::Small) < height(SubtitleSize::Standard));
        assert!(height(SubtitleSize::Standard) < height(SubtitleSize::Large));
    }

    #[test]
    fn the_background_option_sets_the_band_opacity() {
        // The band covers more pixels than the glyphs, so its alpha is the
        // most common one.
        let band_alpha = |background| {
            let style = SubtitleStyle {
                background,
                ..SubtitleStyle::default()
            };
            let canvas = paint(&line("Hello there"), &style);
            let mut counts = [0usize; 256];
            for px in canvas.rgba.chunks(4).filter(|px| px[3] != 0) {
                counts[usize::from(px[3])] += 1;
            }
            (0..256).max_by_key(|&alpha| counts[alpha]).unwrap()
        };
        assert_eq!(band_alpha(100), 255);
        assert_eq!(band_alpha(50), 127);
        let glyphs_only = paint(
            &line("Hello there"),
            &SubtitleStyle {
                background: 0,
                ..SubtitleStyle::default()
            },
        );
        let banded = paint(&line("Hello there"), &SubtitleStyle::default());
        let covered = |canvas: &Canvas| canvas.rgba.chunks(4).filter(|px| px[3] != 0).count();
        assert!(covered(&banded) > covered(&glyphs_only));
    }

    #[test]
    fn the_colour_option_tints_the_text() {
        let style = SubtitleStyle {
            background: 100,
            color: TextColor::Red,
            ..SubtitleStyle::default()
        };
        let canvas = paint(&line("HHHH"), &style);
        let reddest = canvas
            .rgba
            .chunks(4)
            .filter(|px| px[3] == 255)
            .map(|px| i32::from(px[0]) - i32::from(px[1]))
            .max()
            .unwrap();
        assert!(reddest > 0x60, "red text: {reddest}");
    }

    #[test]
    fn the_speaker_shows_only_when_asked() {
        let subtitle = Subtitle {
            speaker: Some("Rebecca".to_string()),
            lines: vec!["Hi".to_string()],
        };
        let width = |speaker_names| {
            let style = SubtitleStyle {
                background: 0,
                speaker_names,
                ..SubtitleStyle::default()
            };
            let canvas = paint(&subtitle, &style);
            canvas
                .rgba
                .chunks(4)
                .enumerate()
                .filter(|(_, px)| px[3] != 0)
                .map(|(index, _)| index % 640)
                .fold((usize::MAX, 0), |(lo, hi), x| (lo.min(x), hi.max(x)))
        };
        let (off_lo, off_hi) = width(false);
        let (on_lo, on_hi) = width(true);
        assert!(
            on_hi - on_lo > (off_hi - off_lo) * 3,
            "the name widens the line"
        );
    }

    #[test]
    fn a_long_line_wraps_inside_the_screen() {
        let long = "word ".repeat(60);
        let style = SubtitleStyle {
            background: 0,
            ..SubtitleStyle::default()
        };
        let one = painted_rows(&paint(&line("word"), &style)).unwrap();
        let many = painted_rows(&paint(&line(long.trim()), &style)).unwrap();
        assert!(
            many.1 - many.0 > (one.1 - one.0) * 2,
            "it takes several rows"
        );
        let canvas = paint(&line(long.trim()), &style);
        let edge = |x: usize| {
            canvas
                .rgba
                .chunks(4)
                .enumerate()
                .any(|(index, px)| index % 640 == x && px[3] != 0)
        };
        assert!(!edge(0) && !edge(639));
    }
}
