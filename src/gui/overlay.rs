//! The F6 debug HUD and the corner notices.
//!
//! Both draw at the window's native resolution with the menu's faces and
//! theme, over the game and over the in-game menu when it is open. The
//! overlay knows nothing about the game: its owner hands it a [`Report`] of
//! labelled lines, refreshed at [`REPORT_INTERVAL`], and short-lived
//! [`Overlay::notify`] messages (the game speed) that fade out on their own.

use std::time::{Duration, Instant};

use anyhow::Result;

use super::canvas::{Canvas, Color, Rect};
use super::text::{Align, Fonts};
use super::theme::{FontFamily, Role, Theme, palette};
use super::widgets::Painter;

/// How often the owner refreshes the [`Report`] while the overlay is on: the
/// overlay only repaints (and re-uploads) when a refresh changed something,
/// so the readouts tick at this rate rather than every presented frame.
pub const REPORT_INTERVAL: Duration = Duration::from_millis(250);

/// How long a notice stays, fades included, in seconds.
pub const NOTICE_SECONDS: f32 = 2.5;
/// A notice's fade in, in seconds.
const NOTICE_FADE_IN: f32 = 0.12;
/// A notice's fade out at the end of its time, in seconds.
const NOTICE_FADE_OUT: f32 = 0.6;
/// The most notices shown at once; the oldest goes first.
const MAX_NOTICES: usize = 4;

/// What F6 shows; each press steps to the next.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DebugMode {
    #[default]
    Off,
    /// Everything: engine, system, game, player and boss.
    Full,
    /// The player and boss sections alone.
    Player,
    /// The frame rate in the corner.
    Fps,
}

impl DebugMode {
    /// The mode the next F6 press selects.
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::Full,
            Self::Full => Self::Player,
            Self::Player => Self::Fps,
            Self::Fps => Self::Off,
        }
    }
}

/// A value's colour.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Tone {
    #[default]
    Normal,
    Dim,
    Good,
    Warn,
    Bad,
}

impl Tone {
    fn color(self) -> Color {
        match self {
            Self::Normal => palette::TEXT,
            Self::Dim => palette::TEXT_DIM,
            Self::Good => Color::rgb(0x84, 0xD8, 0x7A),
            Self::Warn => Color::rgb(0xF0, 0xB4, 0x48),
            Self::Bad => Color::rgb(0xF0, 0x64, 0x64),
        }
    }
}

/// One labelled readout. An empty label lets the value take the row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub label: String,
    pub value: String,
    pub tone: Tone,
}

/// A titled panel of lines. An empty title draws none.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Section {
    pub title: String,
    pub lines: Vec<Line>,
}

impl Section {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            lines: Vec::new(),
        }
    }

    /// Add a line in the normal colour.
    pub fn line(&mut self, label: impl Into<String>, value: impl Into<String>) -> &mut Self {
        self.toned(label, value, Tone::Normal)
    }

    /// Add a line in `tone`.
    pub fn toned(
        &mut self,
        label: impl Into<String>,
        value: impl Into<String>,
        tone: Tone,
    ) -> &mut Self {
        self.lines.push(Line {
            label: label.into(),
            value: value.into(),
            tone,
        });
        self
    }
}

/// What the overlay shows: panels stacked down the left and right edges.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub left: Vec<Section>,
    pub right: Vec<Section>,
}

/// A short message in the bottom-left corner.
struct Notice {
    /// Replaces an earlier notice with the same key rather than stacking.
    key: &'static str,
    text: String,
    shown: Instant,
}

impl Notice {
    /// The opacity at `now` (0 once it has expired).
    fn alpha(&self, now: Instant) -> f32 {
        let age = now.saturating_duration_since(self.shown).as_secs_f32();
        let fade_in = (age / NOTICE_FADE_IN).min(1.0);
        let fade_out = ((NOTICE_SECONDS - age) / NOTICE_FADE_OUT).clamp(0.0, 1.0);
        fade_in.min(fade_out)
    }

    /// Whether the notice is still changing (fading) at `now`.
    fn animating(&self, now: Instant) -> bool {
        let age = now.saturating_duration_since(self.shown).as_secs_f32();
        !(NOTICE_FADE_IN..=NOTICE_SECONDS - NOTICE_FADE_OUT).contains(&age)
    }

    fn expired(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.shown).as_secs_f32() >= NOTICE_SECONDS
    }
}

/// The debug HUD and notices, with the canvas and faces they draw with.
pub struct Overlay {
    fonts: Fonts,
    canvas: Canvas,
    family: FontFamily,
    mode: DebugMode,
    report: Report,
    /// When the owner last refreshed the report.
    reported: Option<Instant>,
    notices: Vec<Notice>,
    /// Whether the canvas is stale.
    dirty: bool,
    /// The [`Canvas::serial`] of the layer the canvas was composed over
    /// (0 for none).
    base: u64,
}

impl Overlay {
    pub fn new(family: FontFamily) -> Result<Self> {
        Ok(Self {
            fonts: Fonts::new()?,
            canvas: Canvas::new(1, 1),
            family,
            mode: DebugMode::Off,
            report: Report::default(),
            reported: None,
            notices: Vec::new(),
            dirty: true,
            base: 0,
        })
    }

    pub fn mode(&self) -> DebugMode {
        self.mode
    }

    /// F6: step to the next mode and return it.
    pub fn cycle(&mut self) -> DebugMode {
        self.mode = self.mode.next();
        self.report = Report::default();
        self.reported = None;
        self.dirty = true;
        self.mode
    }

    /// Follow the options' UI font.
    pub fn set_family(&mut self, family: FontFamily) {
        if family != self.family {
            self.family = family;
            self.fonts.clear_cache();
            self.dirty = true;
        }
    }

    /// Whether the owner should build a new [`Report`] now.
    pub fn report_due(&self, now: Instant) -> bool {
        self.mode != DebugMode::Off
            && self
                .reported
                .is_none_or(|at| now.saturating_duration_since(at) >= REPORT_INTERVAL)
    }

    /// Replace the report; the canvas repaints only when it changed.
    pub fn set_report(&mut self, report: Report, now: Instant) {
        self.reported = Some(now);
        if report != self.report {
            self.report = report;
            self.dirty = true;
        }
    }

    /// Show `text` in the bottom-left corner for [`NOTICE_SECONDS`],
    /// replacing a notice still showing under the same `key`.
    pub fn notify(&mut self, key: &'static str, text: impl Into<String>) {
        self.notices.retain(|notice| notice.key != key);
        if self.notices.len() >= MAX_NOTICES {
            self.notices.remove(0);
        }
        self.notices.push(Notice {
            key,
            text: text.into(),
            shown: Instant::now(),
        });
        self.dirty = true;
    }

    /// Whether there is anything to show.
    pub fn visible(&self) -> bool {
        self.mode != DebugMode::Off || !self.notices.is_empty()
    }

    /// The canvas the last [`Self::draw`] painted.
    pub fn canvas(&self) -> &Canvas {
        &self.canvas
    }

    /// Paint at `width` x `height` pixels over `base` (the in-game menu's
    /// layer, when open), returning whether anything was repainted. Nothing
    /// is painted while [`Self::visible`] is false. With `gpu` the canvas
    /// records its frame for the GPU backend to draw.
    pub fn draw(&mut self, width: u32, height: u32, base: Option<&Canvas>, gpu: bool) -> bool {
        let now = Instant::now();
        self.dirty |= self.canvas.set_recording(gpu);
        let before = self.notices.len();
        self.notices.retain(|notice| !notice.expired(now));
        if self.notices.len() != before || self.notices.iter().any(|notice| notice.animating(now)) {
            self.dirty = true;
        }
        if !self.visible() {
            return false;
        }
        let (width, height) = (width.max(1), height.max(1));
        let resized = (self.canvas.width, self.canvas.height) != (width, height);
        if resized {
            self.fonts.clear_cache();
        }
        let base = base.filter(|base| (base.width, base.height) == (width, height));
        let base_serial = base.map_or(0, |base| base.serial);
        if !self.dirty && !resized && base_serial == self.base {
            return false;
        }
        self.dirty = false;
        self.base = base_serial;
        self.canvas.reset(width, height);
        if let Some(base) = base {
            self.canvas.copy_from(base);
        }
        let theme = Theme::new(width, height, self.family);
        let mut p = Painter {
            canvas: &mut self.canvas,
            fonts: &mut self.fonts,
            theme,
        };
        if self.mode != DebugMode::Off {
            draw_column(&mut p, &self.report.left, false);
            draw_column(&mut p, &self.report.right, true);
        }
        draw_notices(&mut p, &self.notices, now);
        true
    }
}

/// The gap between the screen edge and the panels, in units.
const MARGIN: f32 = 16.0;
/// A panel's inner padding, in units.
const PADDING: f32 = 10.0;
/// The gap between stacked panels, in units.
const GAP: f32 = 10.0;
/// The gap between a label and its value, in units.
const COLUMN_GAP: f32 = 18.0;

/// Stack `sections` down the left edge, or the right with `right`.
fn draw_column(p: &mut Painter<'_>, sections: &[Section], right: bool) {
    let mut y = p.u(MARGIN);
    let x = if right {
        p.theme.width - p.u(MARGIN)
    } else {
        p.u(MARGIN)
    };
    for section in sections {
        y += draw_section(p, x, y, right, section) + p.u(GAP);
    }
}

/// One panel with its top edge at `y`, its left edge at `x` (its right edge
/// with `right`); returns its height.
fn draw_section(p: &mut Painter<'_>, x: f32, y: f32, right: bool, section: &Section) -> f32 {
    let line_h = p.line_height(Role::Debug);
    let heading_h = p.line_height(Role::DebugHeading);
    let label_w = section
        .lines
        .iter()
        .map(|line| p.measure(Role::Debug, &line.label))
        .fold(0.0, f32::max);
    let gap = if label_w > 0.0 { p.u(COLUMN_GAP) } else { 0.0 };
    let mut inner_w = section
        .lines
        .iter()
        .map(|line| {
            let value = p.measure(Role::Debug, &line.value);
            if line.label.is_empty() {
                value
            } else {
                label_w + gap + value
            }
        })
        .fold(0.0, f32::max);
    if !section.title.is_empty() {
        inner_w = inner_w.max(p.measure(Role::DebugHeading, &section.title));
    }
    let pad = p.u(PADDING);
    let title_h = if section.title.is_empty() {
        0.0
    } else {
        heading_h + p.u(4.0)
    };
    let width = inner_w + 2.0 * pad;
    let height = title_h + line_h * section.lines.len() as f32 + 2.0 * pad;
    let left = if right { x - width } else { x };
    p.canvas.fill_rect(
        Rect::new(left, y, width, height),
        Color::rgba(0x00, 0x00, 0x00, 0xB0),
    );
    let accent = p.u(3.0).max(1.0);
    let edge = if right { left + width - accent } else { left };
    p.canvas
        .fill_rect(Rect::new(edge, y, accent, height), palette::RULE_BRIGHT);

    let text_x = left + pad;
    let mut row_y = y + pad;
    if !section.title.is_empty() {
        p.text(
            Role::DebugHeading,
            text_x,
            row_y + heading_h / 2.0,
            Align::Left,
            palette::ACCENT,
            &section.title,
        );
        row_y += title_h;
    }
    for line in &section.lines {
        let center = row_y + line_h / 2.0;
        let value_x = if line.label.is_empty() {
            text_x
        } else {
            p.text(
                Role::Debug,
                text_x,
                center,
                Align::Left,
                palette::TEXT_DIM,
                &line.label,
            );
            text_x + label_w + gap
        };
        p.text(
            Role::Debug,
            value_x,
            center,
            Align::Left,
            line.tone.color(),
            &line.value,
        );
        row_y += line_h;
    }
    height
}

/// The notices stacked up from the bottom-left corner, newest lowest.
fn draw_notices(p: &mut Painter<'_>, notices: &[Notice], now: Instant) {
    let margin = p.u(32.0);
    let (pad_x, pad_y) = (p.u(18.0), p.u(10.0));
    let line_h = p.line_height(Role::Notice);
    let mut bottom = p.theme.height - margin;
    for notice in notices.iter().rev() {
        let alpha = notice.alpha(now);
        let width = p.measure(Role::Notice, &notice.text) + 2.0 * pad_x;
        let height = line_h + 2.0 * pad_y;
        let rect = Rect::new(margin, bottom - height, width, height);
        p.canvas
            .fill_rect(rect, Color::rgba(0x00, 0x00, 0x00, 0xC0).fade(alpha));
        let accent = p.u(3.0).max(1.0);
        p.canvas.fill_rect(
            Rect::new(rect.x, rect.y, accent, rect.h),
            palette::RULE_BRIGHT.fade(alpha),
        );
        p.text(
            Role::Notice,
            rect.x + pad_x,
            rect.center_y(),
            Align::Left,
            palette::TEXT.fade(alpha),
            &notice.text,
        );
        bottom -= height + p.u(GAP);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f6_cycles_full_player_fps_then_off() {
        let mut mode = DebugMode::Off;
        let mut seen = Vec::new();
        for _ in 0..4 {
            mode = mode.next();
            seen.push(mode);
        }
        assert_eq!(
            seen,
            [
                DebugMode::Full,
                DebugMode::Player,
                DebugMode::Fps,
                DebugMode::Off
            ]
        );
    }

    #[test]
    fn a_notice_fades_in_holds_and_fades_out() {
        let shown = Instant::now();
        let notice = Notice {
            key: "speed",
            text: String::new(),
            shown,
        };
        let at = |seconds: f32| shown + Duration::from_secs_f32(seconds);
        assert_eq!(notice.alpha(shown), 0.0);
        assert_eq!(notice.alpha(at(1.0)), 1.0);
        assert!(!notice.animating(at(1.0)));
        assert!(notice.alpha(at(NOTICE_SECONDS - 0.3)) < 1.0);
        assert!(notice.expired(at(NOTICE_SECONDS)));
    }

    #[test]
    fn a_notice_replaces_one_with_the_same_key() {
        let mut overlay = Overlay::new(FontFamily::Modern).unwrap();
        overlay.notify("speed", "1.5x");
        overlay.notify("speed", "2.0x");
        overlay.notify("other", "hello");
        let texts: Vec<_> = overlay.notices.iter().map(|n| n.text.as_str()).collect();
        assert_eq!(texts, ["2.0x", "hello"]);
    }

    #[test]
    fn the_overlay_paints_only_what_changed() {
        let mut overlay = Overlay::new(FontFamily::Modern).unwrap();
        assert!(!overlay.draw(640, 480, None, false), "nothing to show");
        overlay.cycle();
        let mut section = Section::new("PLAYER");
        section.line("Health", "140 / 140");
        let report = Report {
            left: vec![section],
            right: Vec::new(),
        };
        let now = Instant::now();
        overlay.set_report(report.clone(), now);
        assert!(overlay.draw(640, 480, None, false));
        assert!(overlay.canvas().rgba.chunks(4).any(|px| px[3] != 0));
        overlay.set_report(report, now);
        assert!(!overlay.draw(640, 480, None, false), "an unchanged report");
        assert!(overlay.draw(800, 600, None, false), "a resize");
    }

    #[test]
    fn the_overlay_composes_over_the_menu_layer() {
        let mut overlay = Overlay::new(FontFamily::Modern).unwrap();
        overlay.notify("speed", "2.0x");
        let mut menu = Canvas::new(64, 48);
        menu.rgba.fill(0x40);
        assert!(overlay.draw(64, 48, Some(&menu), false));
        assert_eq!(&overlay.canvas().rgba[..4], &[0x40; 4]);
        // The menu repainting forces the overlay to compose again.
        overlay.notices[0].shown -= Duration::from_secs(1);
        overlay.draw(64, 48, Some(&menu), false);
        assert!(!overlay.draw(64, 48, Some(&menu), false), "nothing changed");
        menu.reset(64, 48);
        assert!(overlay.draw(64, 48, Some(&menu), false), "a new menu frame");
    }
}
