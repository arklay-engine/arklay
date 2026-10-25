//! A modal question: a dark band across the screen with a question, an
//! optional note and a row of choices.

use super::canvas::{Axis, Color, Rect};
use super::input::{GuiEvent, GuiKey};
use super::locale::Locale;
use super::text::Align;
use super::theme::{Role, palette};
use super::widgets::Painter;

/// What one round of input did to a dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogResult {
    Open,
    /// The choice at this index was confirmed.
    Chosen(usize),
    /// Backed out without choosing.
    Cancelled,
}

pub struct Dialog {
    pub title: String,
    pub note: Option<String>,
    pub choices: Vec<String>,
    pub selected: usize,
    hits: Vec<Rect>,
}

impl Dialog {
    pub fn new(
        title: impl Into<String>,
        note: Option<&str>,
        choices: &[&str],
        selected: usize,
    ) -> Self {
        Self {
            title: title.into(),
            note: note.map(str::to_string),
            choices: choices.iter().map(|c| c.to_string()).collect(),
            selected: selected.min(choices.len().saturating_sub(1)),
            hits: Vec::new(),
        }
    }

    /// A Yes/No question in `text`'s language that starts on No.
    pub fn yes_no(text: &Locale, title: &str, note: Option<&str>) -> Self {
        Self::new(
            title,
            note,
            &[text.get("dialog.yes"), text.get("dialog.no")],
            1,
        )
    }

    pub fn handle(&mut self, event: GuiEvent) -> DialogResult {
        let count = self.choices.len();
        match event {
            GuiEvent::Key(GuiKey::Cancel, _) => return DialogResult::Cancelled,
            // A prompt without choices can only be cancelled.
            _ if count == 0 => {}
            GuiEvent::Key(GuiKey::Up | GuiKey::Left, _) => {
                self.selected = (self.selected + count - 1) % count;
            }
            GuiEvent::Key(GuiKey::Down | GuiKey::Right, _) => {
                self.selected = (self.selected + 1) % count;
            }
            GuiEvent::Key(GuiKey::Confirm, _) => return DialogResult::Chosen(self.selected),
            GuiEvent::PointerMove { x, y } => {
                if let Some(index) = self.hits.iter().position(|r| r.contains(x, y)) {
                    self.selected = index;
                }
            }
            GuiEvent::PointerDown { x, y } => {
                if let Some(index) = self.hits.iter().position(|r| r.contains(x, y)) {
                    self.selected = index;
                    return DialogResult::Chosen(index);
                }
            }
            _ => {}
        }
        DialogResult::Open
    }

    pub fn draw(&mut self, p: &mut Painter<'_>, pulse: f32) {
        let bounds = p.canvas.bounds();
        p.canvas.fill_rect(bounds, Color::rgba(0, 0, 0, 0x90));

        let title_h = p.line_height(Role::DialogTitle);
        let note_h = if self.note.is_some() {
            p.line_height(Role::DialogBody)
        } else {
            0.0
        };
        let choice_h = p.u(60.0);
        let gap = p.u(36.0);
        let band_h = p.u(70.0) + title_h + note_h + gap + choice_h * self.choices.len() as f32;
        let band = Rect::new(0.0, (bounds.h - band_h) / 2.0, bounds.w, band_h);
        // The band, with soft edges top and bottom.
        let fade = p.u(24.0);
        p.canvas.fill_gradient(
            Rect::new(0.0, band.y - fade, band.w, fade),
            Axis::Vertical,
            Color::rgba(8, 8, 8, 0),
            palette::DIALOG_BAND,
        );
        p.canvas.fill_rect(band, palette::DIALOG_BAND);
        p.canvas.fill_gradient(
            Rect::new(0.0, band.bottom(), band.w, fade),
            Axis::Vertical,
            palette::DIALOG_BAND,
            Color::rgba(8, 8, 8, 0),
        );
        for y in [band.y, band.bottom() - p.u(1.0)] {
            let line = Rect::new(band.w * 0.2, y, band.w * 0.6, p.u(1.0).max(1.0));
            let half = Rect::new(line.x, line.y, line.w / 2.0, line.h);
            p.canvas.fill_gradient(
                half,
                Axis::Horizontal,
                Color::rgba(0x80, 0x80, 0x80, 0),
                palette::RULE,
            );
            p.canvas.fill_gradient(
                Rect::new(half.right(), line.y, half.w, line.h),
                Axis::Horizontal,
                palette::RULE,
                Color::rgba(0x80, 0x80, 0x80, 0),
            );
        }

        let cx = bounds.w / 2.0;
        let mut y = band.y + p.u(35.0) + title_h / 2.0;
        p.text(
            Role::DialogTitle,
            cx,
            y,
            Align::Center,
            palette::TEXT,
            &self.title,
        );
        y += title_h / 2.0;
        if let Some(note) = &self.note {
            y += note_h / 2.0;
            p.text(
                Role::DialogBody,
                cx,
                y,
                Align::Center,
                palette::TEXT_DIM,
                note,
            );
            y += note_h / 2.0;
        }
        y += gap;
        self.hits.clear();
        let width = p.u(420.0);
        for (index, choice) in self.choices.iter().enumerate() {
            let rect = Rect::new(cx - width / 2.0, y, width, choice_h - p.u(8.0));
            let selected = index == self.selected;
            if selected {
                p.selection(rect, pulse);
            }
            let color = if selected {
                palette::TEXT
            } else {
                palette::TEXT_DIM
            };
            p.text(Role::Row, cx, rect.center_y(), Align::Center, color, choice);
            self.hits.push(rect);
            y += choice_h;
        }
    }
}
