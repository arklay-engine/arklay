//! The modern save/load screen: a full-screen list of the saves in the save
//! directory, newest first, each with its character, the room's camera, the
//! location, the play time and when it was written.
//!
//! In save mode the first row always creates a new save; writing over an
//! existing one asks first. The screen owns no files: it reports what the
//! player chose as a [`SaveRequest`] and its owner reads and writes the
//! saves. The screens themselves are `assets/ui/load.xml` and
//! `save.xml`, built from the shared `save-row` component.

use std::sync::Arc;

use anyhow::Result;

use super::canvas::{Canvas, Picture};
use super::dom::{ImageValue, Library, Ui, UiOutput, Value};
use super::input::{Device, GuiEvent, Sound};
use super::settings::Settings;

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
    /// The slot tag ("01"), the autosave's rotation tag ("A1"), or the
    /// file's name for one outside the slots.
    pub tag: String,
    /// The room's camera in colour and in grey.
    pub thumbnail: Option<(Arc<Picture>, Arc<Picture>)>,
    /// The character's portrait in colour and in grey.
    pub portrait: Option<(Arc<Picture>, Arc<Picture>)>,
    /// The most recent save.
    pub latest: bool,
    /// One of the engine's door autosaves.
    pub autosave: bool,
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

pub struct SaveLoadScreen {
    ui: Ui,
    settings: Settings,
    mode: SaveMode,
    rows: Vec<SaveRow>,
    device: Device,
    /// Whether the canvas records for the GPU.
    recording: bool,
    /// Whether the owner gave it its settings.
    configured: bool,
}

impl SaveLoadScreen {
    pub fn new(mode: SaveMode, rows: Vec<SaveRow>) -> Result<Self> {
        let settings = Settings::default();
        let mut ui = Ui::new(Library::builtin())?;
        ui.set_locale(settings.locale());
        ui.set("saves.rows", publish(&rows));
        ui.set("saves.count", Value::from(rows.len()));
        ui.set("saves.empty", Value::Bool(rows.is_empty()));
        ui.set(
            "saves.confirm",
            Value::Bool(matches!(mode, SaveMode::Load { confirm: true })),
        );
        ui.open(match mode {
            SaveMode::Load { .. } => "load",
            SaveMode::Save => "save",
        })?;
        Ok(Self {
            ui,
            settings,
            mode,
            rows,
            device: Device::Keyboard,
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
        self.ui.set_locale(settings.locale());
        self.ui
            .set_theme(settings.game.ui_font, settings.game.menu_text_size.scale());
        self.settings = settings;
        self.ui.set_device(self.shown_device());
    }

    /// The canvas the last [`Self::draw`] painted.
    pub fn canvas(&self) -> &Canvas {
        self.ui.canvas()
    }

    pub fn rows(&self) -> &[SaveRow] {
        &self.rows
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
        for &event in events {
            let event = match event {
                GuiEvent::Key(key, device) => {
                    self.device = device;
                    GuiEvent::Key(key, self.shown_device())
                }
                other => other,
            };
            if self.ui.is_empty() {
                break;
            }
            for output in self.ui.handle(&[event]) {
                let request = match output {
                    UiOutput::Sound(sound) if self.settings.game.menu_volume > 0 => {
                        SaveRequest::Sound(sound)
                    }
                    UiOutput::Closed(_) if self.ui.is_empty() => SaveRequest::Back,
                    UiOutput::Action { name, arg, .. } => {
                        let index = arg.as_int().and_then(|i| usize::try_from(i).ok());
                        match (name.as_str(), index, self.mode) {
                            ("create", _, SaveMode::Save) => SaveRequest::Create,
                            ("overwrite", Some(i), SaveMode::Save) => SaveRequest::Overwrite(i),
                            ("load", Some(i), SaveMode::Load { .. }) => SaveRequest::Load(i),
                            _ => continue,
                        }
                    }
                    _ => continue,
                };
                requests.push(request);
            }
        }
        requests
    }

    /// Draw the screen at `width` x `height` pixels into [`Self::canvas`],
    /// recorded for the GPU when `gpu`; returns whether anything was
    /// repainted.
    pub fn draw(&mut self, width: u32, height: u32, gpu: bool) -> bool {
        if gpu != self.recording {
            self.recording = gpu;
            self.ui.set_recording(gpu);
        }
        self.ui.draw(width, height)
    }
}

/// The rows as the `save-row` component reads them.
fn publish(rows: &[SaveRow]) -> Value {
    let image = |picture: &Option<(Arc<Picture>, Arc<Picture>)>| match picture {
        Some((colour, grey)) => Value::Image(ImageValue {
            colour: colour.clone(),
            grey: Some(grey.clone()),
        }),
        None => Value::Nil,
    };
    Value::list(
        rows.iter()
            .map(|row| {
                let room = if row.room == row.location {
                    String::new()
                } else {
                    row.room.clone()
                };
                Value::map([
                    ("tag", Value::from(row.tag.as_str())),
                    ("location", Value::from(row.location.as_str())),
                    ("room", Value::from(room)),
                    ("character", Value::from(row.character.as_str())),
                    ("play_time", Value::from(row.play_time.as_str())),
                    ("date", Value::from(row.date.as_str())),
                    ("saves", Value::from(row.saves as i64)),
                    ("thumbnail", image(&row.thumbnail)),
                    ("portrait", image(&row.portrait)),
                    ("latest", Value::Bool(row.latest)),
                    ("autosave", Value::Bool(row.autosave)),
                ])
            })
            .collect(),
    )
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
                // The first two rows are the door autosaves.
                tag: if i < 2 {
                    format!("A{}", i + 1)
                } else {
                    format!("{:02}", i - 1)
                },
                thumbnail: picture(320, 240),
                portrait: picture(32, 32),
                latest: i == 0,
                autosave: i < 2,
            })
            .collect()
    }

    use crate::gui::input::GuiKey;

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
            screen.ui.invalidate();
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
        shot(&mut screen, "save", false);
        screen.handle(&[key(GuiKey::Down)]);
        shot(&mut screen, "save_row", true);
        screen.handle(&[key(GuiKey::Confirm)]);
        shot(&mut screen, "save_overwrite", false);
        let mut screen = SaveLoadScreen::new(SaveMode::Load { confirm: false }, rows()).unwrap();
        shot(&mut screen, "load", false);
        let mut screen =
            SaveLoadScreen::new(SaveMode::Load { confirm: false }, Vec::new()).unwrap();
        shot(&mut screen, "load_empty", false);
    }
}
