//! The modern save/load screen: a full-screen list of the saves in the save
//! directory, newest first, each with its character, the room's camera, the
//! location, the play time and when it was written.
//!
//! In save mode the first row always creates a new save; writing over an
//! existing one asks first. The screen owns no files: it reports what the
//! player chose as a [`SaveRequest`] and its owner reads and writes the
//! saves.

use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;

use super::canvas::{Axis, Canvas, Color, Picture, Rect};
use super::dialog::{Dialog, DialogResult};
use super::input::{Device, GuiEvent, GuiKey, Sound};
use super::locale::Locale;
use super::settings::Settings;
use super::text::{Align, Face, Fonts, Style};
use super::theme::{FontFamily, Role, Theme, palette};
use super::widgets::{Painter, SELECTION_GLOW};

/// Whether the screen loads or saves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveMode {
    /// Pick a save to load. `confirm` asks first (a game is running).
    Load { confirm: bool },
    /// Pick a save to write over, or the new save row.
    Save,
}

/// One save as the list shows it.
#[derive(Debug, Clone, Default)]
pub struct SaveRow {
    /// The shorthand location ("Hall 1F").
    pub location: String,
    /// The room's name ("Main Hall").
    pub room: String,
    /// The character's full name.
    pub character: String,
    /// The play time, `h:mm:ss`.
    pub play_time: String,
    /// When the file was written, in local time.
    pub date: String,
    /// The save counter.
    pub saves: u32,
    /// The slot tag ("01"), or the file's name for one outside the slots.
    pub tag: String,
    /// The room's camera in colour and in grey.
    pub thumbnail: Option<(Arc<Picture>, Arc<Picture>)>,
    /// The character's portrait in colour and in grey.
    pub portrait: Option<(Arc<Picture>, Arc<Picture>)>,
    /// The most recent save.
    pub latest: bool,
}

/// What the screen asks its owner to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveRequest {
    /// Load row `index` of [`SaveLoadScreen::rows`].
    Load(usize),
    /// Write over row `index`.
    Overwrite(usize),
    /// Write a new save.
    Create,
    /// Leave without loading or saving.
    Back,
    /// Play a feedback sound.
    Sound(Sound),
}

/// One entry of the list: the new save row or a save.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Entry {
    Create,
    Row(usize),
}

/// How long the screen takes to fade in.
const APPEAR_SECONDS: f32 = 0.2;

/// The row height and the gap between rows, in units.
const ROW_H: f32 = 132.0;
const ROW_GAP: f32 = 12.0;

/// The screen's backdrop: near-black over the frozen game.
const BACKDROP: Color = Color::rgba(0x0B, 0x0B, 0x0C, 0xF2);
/// The selected row: a pale band with dark text.
const SELECTED: Color = Color::rgb(0xE8, 0xE6, 0xE2);
const SELECTED_TEXT: Color = Color::rgb(0x14, 0x14, 0x14);
const SELECTED_DIM: Color = Color::rgb(0x4A, 0x4A, 0x4A);
/// The latest save's red.
const LATEST: Color = Color::rgb(0xB0, 0x14, 0x14);

pub struct SaveLoadScreen {
    fonts: Fonts,
    canvas: Canvas,
    settings: Settings,
    text: Locale,
    mode: SaveMode,
    rows: Vec<SaveRow>,
    selected: usize,
    /// The first entry shown.
    scroll: usize,
    /// The entries' rectangles as last drawn, with their entry index.
    hits: Vec<(Rect, usize)>,
    hint_hits: Vec<(Rect, GuiKey)>,
    dialog: Option<(Dialog, Entry)>,
    device: Device,
    opened: Instant,
    dirty: bool,
    drawn_appear: f32,
    /// Whether the canvas records for the GPU.
    recording: bool,
    /// Whether the owner gave it its settings.
    configured: bool,
}

impl SaveLoadScreen {
    pub fn new(mode: SaveMode, rows: Vec<SaveRow>) -> Result<Self> {
        let settings = Settings::default();
        Ok(Self {
            fonts: Fonts::new()?,
            canvas: Canvas::new(1, 1),
            text: settings.locale(),
            settings,
            mode,
            rows,
            selected: 0,
            scroll: 0,
            hits: Vec::new(),
            hint_hits: Vec::new(),
            dialog: None,
            device: Device::Keyboard,
            opened: Instant::now(),
            dirty: true,
            drawn_appear: 0.0,
            recording: false,
            configured: false,
        })
    }

    /// Whether the owner has yet to hand over its settings.
    pub fn needs_settings(&self) -> bool {
        !self.configured
    }

    /// Take the menus' settings: the face, the text size, the language,
    /// the button style and the menu volume.
    pub fn configure(&mut self, settings: Settings) {
        self.configured = true;
        let tag = crate::language::resolve_tag(settings.language.as_deref());
        if self.text.tag() != tag {
            self.text = Locale::new(&tag);
        }
        self.settings = settings;
        self.dirty = true;
    }

    /// The canvas the last [`Self::draw`] painted.
    pub fn canvas(&self) -> &Canvas {
        &self.canvas
    }

    pub fn rows(&self) -> &[SaveRow] {
        &self.rows
    }

    /// The list's entries: the new save row first when saving.
    fn entries(&self) -> Vec<Entry> {
        let create = matches!(self.mode, SaveMode::Save).then_some(Entry::Create);
        create
            .into_iter()
            .chain((0..self.rows.len()).map(Entry::Row))
            .collect()
    }

    fn shown_device(&self) -> Device {
        match (self.device, self.settings.game.button_style) {
            (Device::Gamepad(_), Some(style)) => Device::Gamepad(style),
            (device, _) => device,
        }
    }

    /// Feed the frame's events; returns the requests in order.
    pub fn handle(&mut self, events: &[GuiEvent]) -> Vec<SaveRequest> {
        let mut requests = Vec::new();
        self.dirty |= !events.is_empty();
        for &event in events {
            let event = match event {
                GuiEvent::Key(key, device) => {
                    self.device = device;
                    GuiEvent::Key(key, self.shown_device())
                }
                other => other,
            };
            let before = (self.selected, self.dialog.as_ref().map(|(d, _)| d.selected));
            let depth = self.dialog.is_some();
            let start = requests.len();
            self.handle_one(event, &mut requests);
            let after = (self.selected, self.dialog.as_ref().map(|(d, _)| d.selected));
            let acted = requests[start..].iter().any(|request| {
                matches!(
                    request,
                    SaveRequest::Load(_) | SaveRequest::Overwrite(_) | SaveRequest::Create
                )
            });
            let sound = if requests[start..].contains(&SaveRequest::Back) {
                Some(Sound::Cancel)
            } else if acted || (!depth && self.dialog.is_some()) {
                Some(Sound::Decide)
            } else if depth && self.dialog.is_none() {
                Some(Sound::Cancel)
            } else if after != before {
                Some(Sound::Cursor)
            } else {
                None
            };
            if let Some(sound) = sound
                && self.settings.game.menu_volume > 0
            {
                requests.push(SaveRequest::Sound(sound));
            }
        }
        requests
    }

    fn handle_one(&mut self, event: GuiEvent, requests: &mut Vec<SaveRequest>) {
        if let Some((dialog, entry)) = &mut self.dialog {
            match dialog.handle(event) {
                DialogResult::Open => {}
                DialogResult::Chosen(0) => {
                    let entry = *entry;
                    self.dialog = None;
                    if let Entry::Row(index) = entry {
                        requests.push(match self.mode {
                            SaveMode::Load { .. } => SaveRequest::Load(index),
                            SaveMode::Save => SaveRequest::Overwrite(index),
                        });
                    }
                }
                DialogResult::Chosen(_) | DialogResult::Cancelled => self.dialog = None,
            }
            return;
        }
        let count = self.entries().len();
        match event {
            GuiEvent::Key(GuiKey::Up, _) if count > 0 => {
                self.selected = (self.selected + count - 1) % count;
            }
            GuiEvent::Key(GuiKey::Down, _) if count > 0 => {
                self.selected = (self.selected + 1) % count;
            }
            GuiEvent::Key(GuiKey::Left | GuiKey::TabPrev, _) if count > 0 => {
                self.selected = self.selected.saturating_sub(self.page());
            }
            GuiEvent::Key(GuiKey::Right | GuiKey::TabNext, _) if count > 0 => {
                self.selected = (self.selected + self.page()).min(count - 1);
            }
            GuiEvent::Key(GuiKey::Confirm, _) => self.activate(requests),
            GuiEvent::Key(GuiKey::Cancel, _) => requests.push(SaveRequest::Back),
            GuiEvent::PointerMove { x, y } => {
                if let Some(&(_, index)) = self.hits.iter().find(|(r, _)| r.contains(x, y)) {
                    self.selected = index;
                }
            }
            GuiEvent::PointerDown { x, y } => {
                if let Some(&(_, index)) = self.hits.iter().find(|(r, _)| r.contains(x, y)) {
                    self.selected = index;
                    self.activate(requests);
                } else if let Some(&(_, key)) =
                    self.hint_hits.iter().find(|(r, _)| r.contains(x, y))
                {
                    self.handle_one(GuiEvent::Key(key, self.shown_device()), requests);
                }
            }
            GuiEvent::Wheel(notches) if count > 0 => {
                self.selected = if notches > 0.0 {
                    self.selected.saturating_sub(1)
                } else {
                    (self.selected + 1).min(count - 1)
                };
            }
            _ => {}
        }
    }

    /// The rows one page step moves.
    fn page(&self) -> usize {
        visible_rows(&Theme::new(
            self.canvas.width,
            self.canvas.height,
            FontFamily::Modern,
        ))
        .max(1)
    }

    fn activate(&mut self, requests: &mut Vec<SaveRequest>) {
        let Some(&entry) = self.entries().get(self.selected) else {
            return;
        };
        match (entry, self.mode) {
            (Entry::Create, _) => requests.push(SaveRequest::Create),
            (Entry::Row(index), SaveMode::Load { confirm: false }) => {
                requests.push(SaveRequest::Load(index));
            }
            (Entry::Row(_), SaveMode::Load { confirm: true }) => {
                self.dialog = Some((
                    Dialog::yes_no(
                        &self.text,
                        self.text.get("save.load_question"),
                        Some(self.text.get("menu.unsaved_note")),
                    ),
                    entry,
                ));
            }
            (Entry::Row(_), SaveMode::Save) => {
                self.dialog = Some((
                    Dialog::yes_no(&self.text, self.text.get("save.overwrite_question"), None),
                    entry,
                ));
            }
        }
    }

    /// Draw the screen at `width` x `height` pixels into [`Self::canvas`],
    /// recorded for the GPU when `gpu`; returns whether anything was
    /// repainted.
    pub fn draw(&mut self, width: u32, height: u32, gpu: bool) -> bool {
        let (width, height) = (width.max(1), height.max(1));
        if gpu != self.recording {
            self.recording = gpu;
            self.dirty |= self.canvas.set_recording(gpu);
        }
        let resized = (self.canvas.width, self.canvas.height) != (width, height);
        if resized {
            self.fonts.clear_cache();
        }
        let appear = (self.opened.elapsed().as_secs_f32() / APPEAR_SECONDS).min(1.0);
        if !self.dirty && !resized && self.drawn_appear >= 1.0 {
            return false;
        }
        self.dirty = false;
        self.drawn_appear = appear;
        self.canvas
            .reset_to(width, height, BACKDROP.fade(0.6 + 0.4 * appear));
        let theme = Theme::new(width, height, self.settings.game.ui_font)
            .with_text_scale(self.settings.game.menu_text_size.scale());
        let device = self.shown_device();
        let entries = self.entries();
        // Keep the selection on screen.
        let visible = visible_rows(&theme).max(1);
        self.selected = self.selected.min(entries.len().saturating_sub(1));
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + visible {
            self.scroll = self.selected + 1 - visible;
        }
        self.scroll = self.scroll.min(entries.len().saturating_sub(visible));
        let mut p = Painter {
            canvas: &mut self.canvas,
            fonts: &mut self.fonts,
            theme,
        };
        let layout = Layout {
            text: &self.text,
            mode: self.mode,
            rows: &self.rows,
            entries: &entries,
            selected: self.selected,
            scroll: self.scroll,
            device,
            appear,
        };
        layout.draw(&mut p, &mut self.hits, &mut self.hint_hits);
        if let Some((dialog, _)) = &mut self.dialog {
            dialog.draw(&mut p, SELECTION_GLOW);
        }
        true
    }
}

/// The list's top and bottom, in pixels.
fn list_span(theme: &Theme) -> (f32, f32) {
    (theme.u(250.0), theme.height - theme.u(200.0))
}

/// How many rows fit the list.
fn visible_rows(theme: &Theme) -> usize {
    let (top, bottom) = list_span(theme);
    let pitch = theme.u(ROW_H + ROW_GAP);
    (((bottom - top) + theme.u(ROW_GAP)) / pitch)
        .floor()
        .max(1.0) as usize
}

/// A face at `units`, as the theme sets its roles: Retro swaps in the
/// pixel face on its grid.
fn sized(theme: &Theme, face: Face, units: f32, scaled: bool) -> Style {
    let px = theme.u(units) * if scaled { theme.text_scale } else { 1.0 };
    match theme.family {
        FontFamily::Modern => Style::new(face, px.round().max(6.0) as u32),
        FontFamily::Retro => {
            let px = (px * 0.8 / 11.0).round().max(1.0) * 11.0;
            Style::new(Face::Mono, px as u32)
        }
    }
}

/// One frame's drawing inputs.
struct Layout<'a> {
    text: &'a Locale,
    mode: SaveMode,
    rows: &'a [SaveRow],
    entries: &'a [Entry],
    selected: usize,
    scroll: usize,
    device: Device,
    appear: f32,
}

impl Layout<'_> {
    fn draw(
        &self,
        p: &mut Painter<'_>,
        hits: &mut Vec<(Rect, usize)>,
        hint_hits: &mut Vec<(Rect, GuiKey)>,
    ) {
        let bounds = p.canvas.bounds();
        let theme = p.theme;
        let u = |v: f32| theme.u(v);
        let fade = self.appear;
        self.backdrop(p);

        let left = (bounds.w * 0.07).max(u(60.0));
        let width = (bounds.w - left * 2.0).min(u(1560.0));

        // The heading: huge and thin, over a rule with the count beside it.
        let (title, caption) = match self.mode {
            SaveMode::Load { .. } => ("save.load_title", "save.load_caption"),
            SaveMode::Save => ("save.save_title", "save.save_caption"),
        };
        let title_style = sized(&p.theme, Face::Regular, 92.0, false);
        p.fonts.draw_centered(
            p.canvas,
            title_style,
            left - u(4.0),
            u(118.0),
            Align::Left,
            palette::TEXT.fade(fade),
            self.text.get(title),
        );
        let rule_y = u(190.0);
        p.canvas.fill_gradient(
            Rect::new(left, rule_y, width, u(1.5).max(1.0)),
            Axis::Horizontal,
            palette::RULE_BRIGHT.fade(0.8 * fade),
            palette::RULE.fade(0.0),
        );
        let small = sized(&p.theme, Face::Regular, 24.0, true);
        p.fonts.draw_centered(
            p.canvas,
            small,
            left,
            rule_y + u(26.0),
            Align::Left,
            palette::TEXT_DIM.fade(fade),
            self.text.get(caption),
        );
        let count = self.text.format("save.count", self.rows.len());
        p.fonts.draw_centered(
            p.canvas,
            small,
            left + width,
            rule_y + u(26.0),
            Align::Right,
            palette::TEXT_DIM.fade(fade),
            &count,
        );

        // The list.
        let (top, bottom) = list_span(&p.theme);
        let visible = visible_rows(&p.theme);
        hits.clear();
        if self.entries.is_empty() {
            p.text(
                Role::Row,
                bounds.w / 2.0,
                (top + bottom) / 2.0,
                Align::Center,
                palette::TEXT_DIM.fade(fade),
                self.text.get("save.empty"),
            );
        }
        let slide = (1.0 - fade) * u(24.0);
        for (shown, index) in (self.scroll..self.entries.len()).take(visible).enumerate() {
            let y = top + u(ROW_H + ROW_GAP) * shown as f32 + slide;
            let rect = Rect::new(left, y, width, u(ROW_H));
            match self.entries[index] {
                Entry::Create => self.create_row(p, rect, index == self.selected),
                Entry::Row(row) => self.save_row(p, rect, &self.rows[row], index == self.selected),
            }
            hits.push((rect, index));
        }
        if self.entries.len() > visible {
            let bar = Rect::new(
                left + width + u(28.0),
                top,
                u(8.0),
                bottom - top - u(ROW_GAP),
            );
            p.scrollbar(
                bar,
                self.scroll as f32,
                visible as f32,
                self.entries.len() as f32,
            );
        }

        // The prompt and the hints along the bottom.
        let prompt = match self.mode {
            SaveMode::Load { .. } => "save.load_prompt",
            SaveMode::Save => "save.save_prompt",
        };
        let prompt_y = bounds.h - u(130.0);
        p.canvas.fill_gradient(
            Rect::new(
                bounds.w * 0.2,
                prompt_y - u(1.0) - u(30.0),
                bounds.w * 0.3,
                u(1.0).max(1.0),
            ),
            Axis::Horizontal,
            palette::RULE.fade(0.0),
            palette::RULE.fade(fade),
        );
        p.canvas.fill_gradient(
            Rect::new(
                bounds.w * 0.5,
                prompt_y - u(1.0) - u(30.0),
                bounds.w * 0.3,
                u(1.0).max(1.0),
            ),
            Axis::Horizontal,
            palette::RULE.fade(fade),
            palette::RULE.fade(0.0),
        );
        p.text(
            Role::Body,
            bounds.w / 2.0,
            prompt_y,
            Align::Center,
            palette::TEXT.fade(fade),
            self.text.get(prompt),
        );
        let confirm = match self.mode {
            SaveMode::Load { .. } => self.text.get("save.hint_load"),
            SaveMode::Save => self.text.get("save.hint_save"),
        };
        let hints = [
            (GuiKey::Confirm, confirm),
            (GuiKey::Cancel, self.text.get("hint.back")),
        ];
        let rects = p.hint_bar(
            bounds.w / 2.0,
            u(1040.0).min(bounds.h - u(40.0)),
            self.device,
            &hints,
        );
        hint_hits.clear();
        hint_hits.extend(rects.into_iter().zip([GuiKey::Confirm, GuiKey::Cancel]));
    }

    /// The near-black backdrop: a vignette, a faint light from the top left
    /// and a worn band behind the list.
    fn backdrop(&self, p: &mut Painter<'_>) {
        let bounds = p.canvas.bounds();
        let fade = self.appear;
        p.canvas.fill_gradient(
            Rect::new(0.0, 0.0, bounds.w * 0.6, bounds.h),
            Axis::Horizontal,
            Color::rgba(0x2A, 0x28, 0x26, (90.0 * fade) as u8),
            Color::rgba(0x2A, 0x28, 0x26, 0),
        );
        p.canvas.fill_gradient(
            Rect::new(0.0, 0.0, bounds.w, bounds.h * 0.35),
            Axis::Vertical,
            Color::rgba(0x1E, 0x1D, 0x1C, (110.0 * fade) as u8),
            Color::rgba(0x1E, 0x1D, 0x1C, 0),
        );
        let edge = bounds.h * 0.22;
        p.canvas.fill_gradient(
            Rect::new(0.0, bounds.h - edge, bounds.w, edge),
            Axis::Vertical,
            Color::rgba(0, 0, 0, 0),
            Color::rgba(0, 0, 0, (160.0 * fade) as u8),
        );
        let side = bounds.w * 0.12;
        p.canvas.fill_gradient(
            Rect::new(bounds.w - side, 0.0, side, bounds.h),
            Axis::Horizontal,
            Color::rgba(0, 0, 0, 0),
            Color::rgba(0, 0, 0, (120.0 * fade) as u8),
        );
        // Scuffed streaks, like worn paper under the list.
        let theme = p.theme;
        let u = |v: f32| theme.u(v);
        for (i, &(x, y, w, a)) in [
            (0.05, 0.31, 0.42, 7u8),
            (0.38, 0.47, 0.5, 5),
            (0.12, 0.66, 0.36, 6),
            (0.55, 0.78, 0.35, 4),
        ]
        .iter()
        .enumerate()
        {
            let x = bounds.w * x;
            let y = bounds.h * y;
            let w = bounds.w * w;
            let h = u(60.0 + 30.0 * i as f32);
            let skew = u(40.0);
            p.canvas.fill_polygon(
                &[(x + skew, y), (x + w + skew, y), (x + w, y + h), (x, y + h)],
                Color::rgba(0xFF, 0xFF, 0xF0, (f32::from(a) * fade) as u8),
            );
        }
    }

    /// The row's frame: the selection band, or a faint panel.
    fn frame(&self, p: &mut Painter<'_>, rect: Rect, selected: bool, latest: bool) {
        let fade = self.appear;
        if selected {
            p.canvas.fill_rect(rect, SELECTED.fade(fade));
        } else {
            p.canvas.fill_gradient(
                rect,
                Axis::Horizontal,
                Color::rgba(0x24, 0x24, 0x24, (200.0 * fade) as u8),
                Color::rgba(0x18, 0x18, 0x18, (150.0 * fade) as u8),
            );
        }
        if latest {
            // A red band from the left edge, fading out.
            let band = Rect::new(rect.x, rect.y, rect.w * 0.45, rect.h);
            p.canvas.fill_gradient(
                band,
                Axis::Horizontal,
                LATEST.fade(if selected { 0.35 } else { 0.55 } * fade),
                LATEST.fade(0.0),
            );
        }
        let edge = p.u(4.0).max(1.0);
        let marker = if latest {
            LATEST
        } else if selected {
            SELECTED_TEXT
        } else {
            palette::RULE
        };
        p.canvas
            .fill_rect(Rect::new(rect.x, rect.y, edge, rect.h), marker.fade(fade));
    }

    fn create_row(&self, p: &mut Painter<'_>, rect: Rect, selected: bool) {
        let fade = self.appear;
        let theme = p.theme;
        let u = |v: f32| theme.u(v);
        self.frame(p, rect, selected, false);
        let (fg, dim) = colours(selected);
        // A plus in a box where the pictures go.
        let box_h = rect.h - u(28.0);
        let boxed = Rect::new(
            rect.x + u(104.0),
            rect.y + u(14.0),
            box_h * 4.0 / 3.0,
            box_h,
        );
        p.canvas.stroke_rect(boxed, u(2.0).max(1.0), dim.fade(fade));
        let arm = box_h * 0.22;
        let t = u(3.0).max(1.0);
        p.canvas.fill_rect(
            Rect::new(
                boxed.center_x() - arm,
                boxed.center_y() - t / 2.0,
                arm * 2.0,
                t,
            ),
            fg.fade(fade),
        );
        p.canvas.fill_rect(
            Rect::new(
                boxed.center_x() - t / 2.0,
                boxed.center_y() - arm,
                t,
                arm * 2.0,
            ),
            fg.fade(fade),
        );
        let x = boxed.right() + u(36.0);
        let style = sized(&p.theme, Face::Regular, 38.0, true);
        p.fonts.draw_centered(
            p.canvas,
            style,
            x,
            rect.center_y(),
            Align::Left,
            fg.fade(fade),
            self.text.get("save.new_save"),
        );
    }

    fn save_row(&self, p: &mut Painter<'_>, rect: Rect, row: &SaveRow, selected: bool) {
        let fade = self.appear;
        let theme = p.theme;
        let u = |v: f32| theme.u(v);
        self.frame(p, rect, selected, row.latest);
        let (fg, dim) = colours(selected);

        // The slot tag, large and faint.
        let tag_style = sized(&p.theme, Face::Regular, 40.0, false);
        p.fonts.draw_centered(
            p.canvas,
            tag_style,
            rect.x + u(52.0),
            rect.center_y(),
            Align::Center,
            dim.fade(fade),
            &row.tag,
        );

        // The portrait and the room's camera.
        let pic_h = rect.h - u(28.0);
        let pic_y = rect.y + u(14.0);
        let mut x = rect.x + u(104.0);
        let opacity = if selected { 1.0 } else { 0.8 } * fade;
        let face = Rect::new(x, pic_y, pic_h * 0.82, pic_h);
        if let Some((colour, grey)) = &row.portrait {
            p.canvas
                .fill_rect(face, Color::rgba(0, 0, 0, (160.0 * fade) as u8));
            let picture = if selected { colour } else { grey };
            p.canvas.push_clip(face);
            // The square face, its sides trimmed to the frame.
            let side = face.h;
            p.canvas.draw_image(
                picture,
                Rect::new(face.center_x() - side / 2.0, face.y, side, side),
                opacity,
            );
            p.canvas.pop_clip();
            x = face.right() + u(10.0);
        }
        let shot = Rect::new(x, pic_y, pic_h * 4.0 / 3.0, pic_h);
        p.canvas
            .fill_rect(shot, Color::rgba(0, 0, 0, (200.0 * fade) as u8));
        if let Some((colour, grey)) = &row.thumbnail {
            p.canvas
                .draw_image(if selected { colour } else { grey }, shot, opacity);
        }
        p.canvas.stroke_rect(
            shot,
            u(1.0).max(1.0),
            if selected {
                SELECTED_DIM
            } else {
                palette::RULE
            }
            .fade(fade),
        );

        // The location and the room; the character and the play time.
        let text_x = shot.right() + u(36.0);
        let right_col = rect.right() - u(340.0);
        let name = sized(&p.theme, Face::Regular, 36.0, true);
        let detail = sized(&p.theme, Face::Regular, 26.0, true);
        let line1 = rect.y + rect.h * 0.36;
        let line2 = rect.y + rect.h * 0.68;
        let room_x = text_x
            + p.fonts.draw_centered(
                p.canvas,
                name,
                text_x,
                line1,
                Align::Left,
                fg.fade(fade),
                &row.location,
            );
        if !row.room.is_empty() && row.room != row.location {
            let gap = u(22.0);
            p.canvas.fill_rect(
                Rect::new(room_x + gap, line1 - u(12.0), u(2.0).max(1.0), u(24.0)),
                dim.fade(fade),
            );
            p.canvas.push_clip(Rect::new(
                text_x,
                rect.y,
                right_col - u(24.0) - text_x,
                rect.h,
            ));
            p.fonts.draw_centered(
                p.canvas,
                name,
                room_x + gap * 2.0,
                line1,
                Align::Left,
                dim.fade(fade),
                &row.room,
            );
            p.canvas.pop_clip();
        }
        let line = format!(
            "{}     {} {}",
            row.character,
            self.text.get("save.play_time"),
            row.play_time
        );
        p.fonts.draw_centered(
            p.canvas,
            detail,
            text_x,
            line2,
            Align::Left,
            dim.fade(fade),
            &line,
        );

        // The date and the save count, past a divider.
        p.canvas.fill_rect(
            Rect::new(
                right_col,
                rect.y + u(22.0),
                u(1.0).max(1.0),
                rect.h - u(44.0),
            ),
            if selected {
                SELECTED_DIM
            } else {
                palette::RULE
            }
            .fade(fade),
        );
        let col_x = right_col + u(32.0);
        p.fonts.draw_centered(
            p.canvas,
            detail,
            col_x,
            line1,
            Align::Left,
            fg.fade(fade),
            &row.date,
        );
        p.fonts.draw_centered(
            p.canvas,
            detail,
            col_x,
            line2,
            Align::Left,
            dim.fade(fade),
            &self.text.format("save.saves", row.saves),
        );
        if row.latest {
            let label = self.text.get("save.latest");
            let small = sized(&p.theme, Face::Bold, 20.0, true);
            let w = p.fonts.measure(small, label) + u(20.0);
            let tag = Rect::new(rect.right() - w - u(12.0), rect.y + u(12.0), w, u(30.0));
            p.canvas.fill_rect(tag, LATEST.fade(fade));
            p.fonts.draw_centered(
                p.canvas,
                small,
                tag.center_x(),
                tag.center_y(),
                Align::Center,
                Color::WHITE.fade(fade),
                label,
            );
        }
    }
}

/// The text colours of a row: bright and dim, dark on the selection.
fn colours(selected: bool) -> (Color, Color) {
    if selected {
        (SELECTED_TEXT, SELECTED_DIM)
    } else {
        (palette::TEXT, palette::TEXT_DIM)
    }
}

/// `picture` in grey, slightly darkened, for the rows not selected.
pub fn greyscale(picture: &Picture) -> Picture {
    let rgba = picture
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|px| {
            let luma =
                (u32::from(px[0]) * 54 + u32::from(px[1]) * 183 + u32::from(px[2]) * 19) >> 8;
            let v = (luma * 220 / 255) as u8;
            [v, v, v, px[3]]
        })
        .collect();
    Picture {
        width: picture.width,
        height: picture.height,
        rgba,
    }
}

/// `picture` box-filtered down to at most `max_width` wide.
pub fn downscale(picture: Picture, max_width: u32) -> Picture {
    if picture.width <= max_width || picture.width == 0 || picture.height == 0 {
        return picture;
    }
    let factor = picture.width.div_ceil(max_width).max(1);
    let (width, height) = (
        (picture.width / factor).max(1),
        (picture.height / factor).max(1),
    );
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let mut sum = [0u32; 4];
            for dy in 0..factor {
                for dx in 0..factor {
                    let sx = (x * factor + dx).min(picture.width - 1);
                    let sy = (y * factor + dy).min(picture.height - 1);
                    let at = ((sy * picture.width + sx) * 4) as usize;
                    for (s, &v) in sum.iter_mut().zip(&picture.rgba[at..at + 4]) {
                        *s += u32::from(v);
                    }
                }
            }
            let n = factor * factor;
            rgba.extend(sum.iter().map(|&s| (s / n) as u8));
        }
    }
    Picture {
        width,
        height,
        rgba,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Vec<SaveRow> {
        let picture = |w: u32, h: u32| {
            let rgba = (0..w * h)
                .flat_map(|i| [(i % w * 255 / w) as u8, (i / w * 255 / h) as u8, 0x60, 0xFF])
                .collect();
            let picture = Picture {
                width: w,
                height: h,
                rgba,
            };
            let grey = greyscale(&picture);
            Some((Arc::new(picture), Arc::new(grey)))
        };
        (0..9)
            .map(|i| SaveRow {
                location: "Hall 1F".to_string(),
                room: "Main Hall".to_string(),
                character: "Chris Redfield".to_string(),
                play_time: format!("{i}:12:34"),
                date: "2026/10/27  02:18".to_string(),
                saves: i + 1,
                tag: format!("{:02}", i + 1),
                thumbnail: picture(320, 240),
                portrait: picture(32, 32),
                latest: i == 0,
            })
            .collect()
    }

    fn key(key: GuiKey) -> GuiEvent {
        GuiEvent::Key(key, Device::Keyboard)
    }

    fn actions(screen: &mut SaveLoadScreen, events: &[GuiEvent]) -> Vec<SaveRequest> {
        let mut requests = screen.handle(events);
        requests.retain(|request| !matches!(request, SaveRequest::Sound(_)));
        requests
    }

    #[test]
    fn saving_starts_on_a_new_save_and_asks_before_overwriting() {
        let mut screen = SaveLoadScreen::new(SaveMode::Save, rows()).unwrap();
        assert_eq!(
            actions(&mut screen, &[key(GuiKey::Confirm)]),
            [SaveRequest::Create]
        );
        assert_eq!(
            actions(&mut screen, &[key(GuiKey::Down), key(GuiKey::Confirm)]),
            []
        );
        assert_eq!(
            actions(&mut screen, &[key(GuiKey::Left), key(GuiKey::Confirm)]),
            [SaveRequest::Overwrite(0)]
        );
        assert_eq!(
            actions(&mut screen, &[key(GuiKey::Cancel)]),
            [SaveRequest::Back]
        );
    }

    #[test]
    fn loading_asks_only_over_a_running_game() {
        let mut screen = SaveLoadScreen::new(SaveMode::Load { confirm: false }, rows()).unwrap();
        assert_eq!(
            actions(&mut screen, &[key(GuiKey::Down), key(GuiKey::Confirm)]),
            [SaveRequest::Load(1)]
        );
        let mut screen = SaveLoadScreen::new(SaveMode::Load { confirm: true }, rows()).unwrap();
        assert_eq!(actions(&mut screen, &[key(GuiKey::Confirm)]), []);
        assert_eq!(
            actions(&mut screen, &[key(GuiKey::Left), key(GuiKey::Confirm)]),
            [SaveRequest::Load(0)]
        );
    }

    /// `ARKLAY_GUI_DUMP=<dir> cargo test --lib gui::save_screen::tests::dump`
    /// writes the screen in both modes as BMPs for eyeballing, painted and
    /// replayed from the GPU recording.
    #[test]
    fn dump() {
        let Some(dir) = std::env::var_os("ARKLAY_GUI_DUMP") else {
            return;
        };
        let dir = std::path::PathBuf::from(dir);
        let shot = |screen: &mut SaveLoadScreen, name: &str, gpu: bool| {
            let (w, h) = (1920, 1080);
            screen.dirty = true;
            screen.draw(w, h, gpu);
            let pixels = screen.canvas().pixels();
            let rgba = pixels
                .chunks(4)
                .flat_map(|px| [px[0], px[1], px[2], 255])
                .collect();
            let image = crate::state::Image {
                width: w,
                height: h,
                rgba,
            };
            crate::bmp::encode(&image, &dir.join(format!("{name}.bmp"))).unwrap();
        };
        let mut screen = SaveLoadScreen::new(SaveMode::Save, rows()).unwrap();
        screen.drawn_appear = 1.0;
        screen.opened -= std::time::Duration::from_secs(1);
        shot(&mut screen, "save", false);
        screen.handle(&[key(GuiKey::Down)]);
        shot(&mut screen, "save_row", true);
        screen.handle(&[key(GuiKey::Confirm)]);
        shot(&mut screen, "save_overwrite", false);
        let mut screen = SaveLoadScreen::new(SaveMode::Load { confirm: false }, rows()).unwrap();
        screen.opened -= std::time::Duration::from_secs(1);
        shot(&mut screen, "load", false);
        let mut screen =
            SaveLoadScreen::new(SaveMode::Load { confirm: false }, Vec::new()).unwrap();
        screen.opened -= std::time::Duration::from_secs(1);
        shot(&mut screen, "load_empty", false);
    }
}
