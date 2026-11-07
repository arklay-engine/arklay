//! The modern save/load screen's engine side: [`gui::save_screen`] at the
//! window's resolution, fed the save directory's files and writing the one
//! the player chose.
//!
//! It runs where the shipped Lua screen does (the title's LOAD GAME, the
//! typewriter, the in-game menu, the next-cycle save), but takes its input
//! per presented frame through [`NativeScreen`] rather than per tick; the
//! tick only reports what the player chose.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;

use crate::assets::{self, Artwork, Geometry};
use crate::gui;
use crate::gui::canvas::Picture;
use crate::gui::save_screen::{self, SaveFocus, SaveLoadScreen, SaveMode, SaveRequest, SaveRow};
use crate::render::Framebuffer;
use crate::save::{self, SaveFile};
use crate::state::{Image, RoomId};

use super::save_load::{
    NativeScreen, SaveOutcome, SaveScreen, SaveScreenView, consume_snapshot_ribbon,
};
use super::{Screen, ScreenAction, ScreenResult, UiContext, UiInput};

/// The widest a room thumbnail is kept.
const THUMBNAIL_WIDTH: u32 = 320;

/// The full names the rows show, by character id.
const CHARACTERS: [&str; 2] = ["Chris Redfield", "Jill Valentine"];

/// 30 Hz ticks in one second of play.
const TICKS_PER_SECOND: u32 = 30;

/// What the screen was opened for.
enum Kind {
    Load,
    /// The pending block and whether writing it uses an ink ribbon.
    Save {
        file: Box<SaveFile>,
        ink_ribbon: bool,
    },
}

/// One listed file.
struct Listed {
    path: PathBuf,
    /// The zero-based numbered slot, for a `savedatN.dat` of the eight.
    slot: Option<usize>,
}

pub struct ModernSaveScreen {
    kind: Kind,
    gui: Option<SaveLoadScreen>,
    files: Vec<Listed>,
    save_dir: PathBuf,
    /// Whether no game is running under the screen (the title, the ending):
    /// it draws over black, and backing out of loading returns to the title.
    standalone: bool,
    result: Option<ScreenResult>,
    outcome: Option<SaveOutcome>,
    /// Where the cursor rested after the last input, for the bot's view.
    focus: SaveFocus,
}

impl ModernSaveScreen {
    pub fn open_load(cx: &mut UiContext<'_>) -> Result<Self> {
        Self::open_kind(Kind::Load, cx)
    }

    pub fn open_save(cx: &mut UiContext<'_>, file: SaveFile, ink_ribbon: bool) -> Result<Self> {
        Self::open_kind(
            Kind::Save {
                file: Box::new(file),
                ink_ribbon,
            },
            cx,
        )
    }

    fn open_kind(kind: Kind, cx: &mut UiContext<'_>) -> Result<Self> {
        let mut screen = Self {
            kind,
            gui: None,
            files: Vec::new(),
            save_dir: cx.save_dir.to_path_buf(),
            standalone: cx.game.is_none(),
            result: None,
            outcome: None,
            focus: SaveFocus::None,
        };
        screen.open(cx)?;
        Ok(screen)
    }

    /// Carry out one of the screen's requests; returns its sound.
    fn request(&mut self, request: SaveRequest) -> Option<gui::Sound> {
        match request {
            SaveRequest::Sound(sound) => return Some(sound),
            SaveRequest::Back => {
                self.result = Some(ScreenResult::Done(
                    if self.standalone && matches!(self.kind, Kind::Load) {
                        ScreenAction::Title
                    } else {
                        ScreenAction::Resume
                    },
                ));
            }
            SaveRequest::Load(index) => {
                if let Some(listed) = self.files.get(index) {
                    self.result = Some(ScreenResult::Done(match listed.slot {
                        Some(slot) => ScreenAction::LoadGame { slot },
                        None => ScreenAction::LoadFile(listed.path.clone()),
                    }));
                }
            }
            SaveRequest::Overwrite(index) => {
                if let Some(listed) = self.files.get(index) {
                    let (path, slot) = (listed.path.clone(), listed.slot);
                    self.write(path, slot);
                }
            }
            SaveRequest::Create => {
                let (path, slot) = free_file(&self.save_dir);
                self.write(path, slot);
            }
        }
        None
    }

    /// Remember where the cursor rests, for [`SaveScreen::bot_view`].
    fn note_focus(&mut self) {
        if let Some(screen) = &mut self.gui {
            self.focus = screen.focus();
        }
    }

    /// Write the pending block to `path`, then leave.
    fn write(&mut self, path: PathBuf, slot: Option<usize>) {
        let Kind::Save { file, ink_ribbon } = &self.kind else {
            return;
        };
        let mut file = (**file).clone();
        if *ink_ribbon {
            consume_snapshot_ribbon(&mut file);
        }
        match save::save_to_path(&path, &file) {
            Ok(()) => {
                self.outcome = Some(SaveOutcome {
                    slot,
                    ink_ribbon: *ink_ribbon,
                });
                self.result = Some(ScreenResult::Done(ScreenAction::Resume));
            }
            // The list stays up, so the player can pick another file.
            Err(err) => eprintln!("warning: could not save: {err:#}"),
        }
    }
}

impl Screen for ModernSaveScreen {
    fn open(&mut self, cx: &mut UiContext<'_>) -> Result<()> {
        let mut saves: Vec<(save::SaveEntry, SaveFile)> = save::scan_dir(cx.save_dir)
            .into_iter()
            .filter(save::SaveEntry::loadable)
            .filter_map(|entry| {
                let file = save::load_file(&entry.path).ok()?;
                Some((entry, file))
            })
            .collect();
        // Newest first; the numbered slots in order among equals.
        saves.sort_by(|(a, _), (b, _)| b.timestamp.total_cmp(&a.timestamp));
        let mut art = Art::default();
        let newest = saves.first().map(|(entry, _)| entry.timestamp);
        let mut rows = Vec::with_capacity(saves.len());
        self.files.clear();
        for (index, (entry, file)) in saves.iter().enumerate() {
            let slot = slot_of(&entry.name);
            rows.push(SaveRow {
                location: location(cx, file),
                room: room_name(cx, file),
                character: character_name(cx, file.character),
                play_time: play_time(file.timer),
                date: local_date(entry.timestamp),
                saves: u32::from(file.saves),
                tag: match (slot, entry.autosave) {
                    (Some(slot), _) => format!("{:02}", slot + 1),
                    (None, Some(index)) => format!("A{index}"),
                    (None, None) => entry.base.clone(),
                },
                thumbnail: art.thumbnail(cx, file),
                portrait: art.portrait(cx, file.character),
                latest: index == 0 && newest.is_some_and(|t| t > 0.0),
                autosave: entry.autosave.is_some(),
            });
            self.files.push(Listed {
                path: entry.path.clone(),
                slot,
            });
        }
        let mode = match self.kind {
            Kind::Load => SaveMode::Load {
                confirm: !self.standalone,
            },
            Kind::Save { .. } => SaveMode::Save,
        };
        self.gui = Some(SaveLoadScreen::new(mode, rows)?);
        self.note_focus();
        Ok(())
    }

    fn update(&mut self, _cx: &UiContext<'_>, _input: UiInput) -> ScreenResult {
        self.result.take().unwrap_or(ScreenResult::Continue)
    }

    fn draw(&mut self, _cx: &UiContext<'_>, framebuffer: &mut Framebuffer) {
        // Over a running game the native layer covers the frozen frame.
        if self.standalone {
            framebuffer.clear();
        }
    }
}

impl SaveScreen for ModernSaveScreen {
    fn take_outcome(&mut self) -> Option<SaveOutcome> {
        self.outcome.take()
    }

    fn native(&mut self) -> Option<&mut dyn NativeScreen> {
        Some(self)
    }

    fn native_canvas(&self) -> Option<&gui::Canvas> {
        self.gui.as_ref().map(SaveLoadScreen::canvas)
    }

    /// The list in the classic screen's terms: `cursor` is the highlighted
    /// row, and `rows` names the numbered slot each row writes or loads
    /// (save mode's first row is the new save, which takes the first free
    /// file).
    fn bot_view(&self) -> Option<SaveScreenView> {
        let save = matches!(self.kind, Kind::Save { .. });
        let mut rows: Vec<i64> = Vec::with_capacity(self.files.len() + 1);
        if save {
            let (_, slot) = free_file(&self.save_dir);
            rows.push(slot.map_or(-1, |slot| slot as i64));
        }
        rows.extend(
            self.files
                .iter()
                .map(|listed| listed.slot.map_or(-1, |slot| slot as i64)),
        );
        let mut slots = vec![false; save::SAVE_SLOT_COUNT];
        for slot in self.files.iter().filter_map(|listed| listed.slot) {
            slots[slot] = true;
        }
        let offset = usize::from(save);
        let (stage, cursor, answer) = match self.focus {
            _ if self.result.is_some() => ("fade_out", -1, None),
            SaveFocus::Dialog { yes } => (
                if save {
                    "confirm_overwrite"
                } else {
                    "confirm_load"
                },
                -1,
                Some(yes),
            ),
            SaveFocus::Create => ("idle", 0, None),
            SaveFocus::Row(index) => ("idle", (index + offset) as i64, None),
            SaveFocus::None => ("idle", -1, None),
        };
        Some(SaveScreenView {
            save,
            stage: stage.to_string(),
            cursor,
            exit_row: rows.len() as i64,
            slots,
            rows: Some(rows),
            answer,
        })
    }
}

impl NativeScreen for ModernSaveScreen {
    fn needs_settings(&self) -> bool {
        self.gui
            .as_ref()
            .is_some_and(SaveLoadScreen::needs_settings)
    }

    fn configure(&mut self, settings: gui::Settings) {
        if let Some(screen) = &mut self.gui {
            screen.configure(settings);
        }
    }

    fn handle(&mut self, events: &[gui::GuiEvent]) -> Vec<gui::Sound> {
        // A choice already made waits for the tick to report it.
        if self.result.is_some() {
            return Vec::new();
        }
        let Some(screen) = &mut self.gui else {
            return Vec::new();
        };
        let requests = screen.handle(events);
        let sounds = requests
            .into_iter()
            .filter_map(|request| self.request(request))
            .collect();
        self.note_focus();
        sounds
    }

    fn draw(&mut self, width: u32, height: u32, gpu: bool) -> bool {
        self.gui
            .as_mut()
            .is_some_and(|screen| screen.draw(width, height, gpu))
    }
}

/// The zero-based slot of a `savedatN.dat` name among the eight.
fn slot_of(name: &str) -> Option<usize> {
    (0..save::SAVE_SLOT_COUNT).find(|&index| save::slot_name(index).as_deref() == Some(name))
}

/// The first free `savedatN.dat` in `dir`, with its slot when it is one of
/// the eight.
fn free_file(dir: &std::path::Path) -> (PathBuf, Option<usize>) {
    (1..)
        .map(|n| (dir.join(format!("savedat{n}.dat")), n - 1))
        .find(|(path, _)| !path.exists())
        .map(|(path, index)| (path, (index < save::SAVE_SLOT_COUNT).then_some(index)))
        .expect("an unused file name")
}

/// The room's three-digit id, as the cut names spell it.
fn room3(file: &SaveFile) -> String {
    file.room_id().room3()
}

/// The save screen's shorthand location ("Hall 1F").
fn location(cx: &UiContext<'_>, file: &SaveFile) -> String {
    cx.text
        .and_then(|text| {
            text.save_location(file.room_id().stage, file.room)
                .map(|bytes| text.decode(bytes))
        })
        .map(|label| label.trim().to_string())
        .filter(|label| !label.is_empty())
        .unwrap_or_else(|| format!("Room {}", room3(file)))
}

/// The room's display name in title case ("Main Hall").
fn room_name(cx: &UiContext<'_>, file: &SaveFile) -> String {
    cx.text
        .and_then(|text| text.room_name(&room3(file)).map(|bytes| text.decode(bytes)))
        .map(|name| title_case(name.trim()))
        .unwrap_or_default()
}

/// `text` with each word's first letter capitalised and the rest lowered;
/// text without ASCII capitals (a Japanese name) is left alone.
fn title_case(text: &str) -> String {
    if !text.chars().any(|c| c.is_ascii_uppercase()) {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut start = true;
    for c in text.chars() {
        if c.is_alphanumeric() || c == '\'' {
            if start {
                out.extend(c.to_uppercase());
            } else {
                out.extend(c.to_lowercase());
            }
            start = false;
        } else {
            out.push(c);
            start = true;
        }
    }
    out
}

/// The character's full name.
fn character_name(cx: &UiContext<'_>, character: u8) -> String {
    let text = |key: String| cx.text.and_then(|text| text.text(&key));
    text(format!("UI_SAVE_CHARACTER_{character}_FULL_NAME"))
        .or_else(|| {
            CHARACTERS
                .get(usize::from(character))
                .map(|name| name.to_string())
        })
        .or_else(|| text(format!("UI_SAVE_CHARACTER_{character}_NAME")))
        .unwrap_or_else(|| format!("#{character}"))
}

/// The play timer as `h:mm:ss`.
pub(crate) fn play_time(timer: u32) -> String {
    let seconds = timer / TICKS_PER_SECOND;
    format!(
        "{}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
}

/// A file time in Unix seconds as a local `YYYY/MM/DD HH:MM`.
fn local_date(timestamp: f64) -> String {
    if timestamp <= 0.0 {
        return String::new();
    }
    let mut dt: sdl3_sys::time::SDL_DateTime = unsafe { std::mem::zeroed() };
    let ticks = (timestamp as i64).saturating_mul(1_000_000_000);
    if !unsafe { sdl3_sys::time::SDL_TimeToDateTime(ticks, &mut dt, true) } {
        return String::new();
    }
    format!(
        "{:04}/{:02}/{:02}  {:02}:{:02}",
        dt.year, dt.month, dt.day, dt.hour, dt.minute
    )
}

/// A picture in colour and in grey.
type PicturePair = (Arc<Picture>, Arc<Picture>);

/// The pictures the rows share, loaded once per screen.
#[derive(Default)]
struct Art {
    thumbnails: HashMap<(u8, u8, u8), Option<PicturePair>>,
    faces: Option<Option<Image>>,
}

impl Art {
    /// The camera the save was made in, in colour and in grey.
    fn thumbnail(&mut self, cx: &UiContext<'_>, file: &SaveFile) -> Option<PicturePair> {
        self.thumbnails
            .entry((file.stage, file.room, file.camera))
            .or_insert_with(|| {
                let id = RoomId::parse(&format!("{}0", room3(file))).ok()?;
                let stem = crate::room::Folder::of(cx.pack, id).cut_stem(usize::from(file.camera));
                let image = super::load_picture(cx.pack, &stem, Geometry::Free, None).ok()?;
                let picture = save_screen::downscale(opaque(image), THUMBNAIL_WIDTH);
                Some(pair(picture))
            })
            .clone()
    }

    /// The character's status-screen face, in colour and in grey.
    fn portrait(&mut self, cx: &UiContext<'_>, character: u8) -> Option<PicturePair> {
        let sheet = self
            .faces
            .get_or_insert_with(|| {
                super::load_page(cx.pack, "ui/statface")
                    .ok()
                    .map(|page| match page {
                        Artwork::Indexed(texture) => assets::texture_to_rgba(&texture),
                        Artwork::Rgba(image) => image,
                    })
            })
            .as_ref()?;
        let (x0, y0) = (u32::from(character & 1) * 32, u32::from(character & 2) * 16);
        let side = 32;
        if x0 + side > sheet.width || y0 + side > sheet.height {
            return None;
        }
        let mut rgba = Vec::with_capacity((side * side * 4) as usize);
        for y in y0..y0 + side {
            let at = ((y * sheet.width + x0) * 4) as usize;
            rgba.extend_from_slice(&sheet.rgba[at..at + (side * 4) as usize]);
        }
        Some(pair(Picture {
            width: side,
            height: side,
            rgba,
        }))
    }
}

/// A decoded picture as an opaque GUI picture.
fn opaque(image: Image) -> Picture {
    let mut rgba = image.rgba;
    for px in rgba.as_chunks_mut::<4>().0 {
        px[3] = 255;
    }
    Picture {
        width: image.width,
        height: image.height,
        rgba,
    }
}

fn pair(picture: Picture) -> PicturePair {
    let grey = save_screen::greyscale(&picture);
    (Arc::new(picture), Arc::new(grey))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::Pack;
    use crate::ui::save_load::pad_events;

    /// A fresh save directory, removed when dropped.
    struct Dir(PathBuf);

    impl Dir {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("arklay-modern-save-{}-{label}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn pack() -> &'static Pack {
        use std::sync::OnceLock;
        static PACK: OnceLock<Pack> = OnceLock::new();
        PACK.get_or_init(|| {
            let mut writer = crate::pack::PackWriter::new();
            writer.add("unused", Vec::new()).unwrap();
            Pack::from_bytes(writer.to_bytes().unwrap()).unwrap()
        })
    }

    fn context(dir: &Dir) -> UiContext<'_> {
        UiContext {
            authentic: false,
            pack: pack(),
            save_dir: &dir.0,
            font: None,
            text: None,
            ticks: 0,
            cues: Default::default(),
            game: None,
        }
    }

    /// A directory holding saves in numbered slots 0 and 2.
    fn two_saves(label: &str) -> Dir {
        let dir = Dir::new(label);
        save::save(&dir.0, 0, &SaveFile::default()).unwrap();
        save::save(&dir.0, 2, &SaveFile::default()).unwrap();
        dir
    }

    /// One bot press, as the engine hands it to the native screen.
    fn press(screen: &mut ModernSaveScreen, set: impl FnOnce(&mut UiInput)) {
        let mut ui = UiInput::default();
        set(&mut ui);
        let events = pad_events(&ui);
        assert_eq!(events.len(), 1);
        screen.handle(&events);
    }

    fn view(screen: &ModernSaveScreen) -> SaveScreenView {
        screen.bot_view().expect("the modern screen is readable")
    }

    #[test]
    fn the_modern_screens_publish_the_classic_view() {
        let dir = two_saves("view");
        let mut cx = context(&dir);
        let screen = ModernSaveScreen::open_save(&mut cx, SaveFile::default(), false).unwrap();
        let saving = view(&screen);
        assert!(saving.save);
        assert_eq!(saving.stage, "idle");
        // The cursor opens on the new save row, which takes slot 1.
        assert_eq!(saving.cursor, 0);
        let rows = saving.rows.clone().unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0], 1);
        let mut listed = rows[1..].to_vec();
        listed.sort();
        assert_eq!(listed, [0, 2]);
        assert_eq!(saving.exit_row, 3);
        assert_eq!(
            saving.slots,
            [true, false, true, false, false, false, false, false]
        );
        assert_eq!(saving.answer, None);

        let screen = ModernSaveScreen::open_load(&mut cx).unwrap();
        let loading = view(&screen);
        assert!(!loading.save);
        assert_eq!(loading.stage, "idle");
        assert_eq!(loading.cursor, 0);
        let mut listed = loading.rows.clone().unwrap();
        listed.sort();
        assert_eq!(listed, [0, 2]);
        assert_eq!(loading.slots, saving.slots);
    }

    #[test]
    fn pad_presses_move_the_cursor_and_overwrite_a_save() {
        let dir = two_saves("overwrite");
        let mut cx = context(&dir);
        let mut screen = ModernSaveScreen::open_save(&mut cx, SaveFile::default(), false).unwrap();
        let rows = view(&screen).rows.unwrap();
        let target = rows.iter().rposition(|&slot| slot == 2).unwrap() as i64;
        while view(&screen).cursor < target {
            let before = view(&screen).cursor;
            press(&mut screen, |ui| ui.down = true);
            assert_eq!(view(&screen).cursor, before + 1);
        }
        press(&mut screen, |ui| ui.up = true);
        assert_eq!(view(&screen).cursor, target - 1);
        press(&mut screen, |ui| ui.down = true);
        assert_eq!(view(&screen).cursor, target);
        // Writing over a save asks first, on NO.
        press(&mut screen, |ui| ui.confirm = true);
        let asking = view(&screen);
        assert_eq!(asking.stage, "confirm_overwrite");
        assert_eq!(asking.answer, Some(false));
        press(&mut screen, |ui| ui.left = true);
        assert_eq!(view(&screen).answer, Some(true));
        press(&mut screen, |ui| ui.confirm = true);
        assert_eq!(view(&screen).stage, "fade_out");
        let cx = context(&dir);
        assert_eq!(
            screen.update(&cx, UiInput::default()),
            ScreenResult::Done(ScreenAction::Resume)
        );
        assert_eq!(
            screen.take_outcome(),
            Some(SaveOutcome {
                slot: Some(2),
                ink_ribbon: false,
            })
        );
    }

    #[test]
    fn a_pad_confirm_on_the_new_save_row_writes_the_first_free_slot() {
        let dir = two_saves("create");
        let mut cx = context(&dir);
        let mut screen = ModernSaveScreen::open_save(&mut cx, SaveFile::default(), false).unwrap();
        press(&mut screen, |ui| ui.confirm = true);
        let cx = context(&dir);
        assert_eq!(
            screen.update(&cx, UiInput::default()),
            ScreenResult::Done(ScreenAction::Resume)
        );
        assert_eq!(
            screen.take_outcome().and_then(|outcome| outcome.slot),
            Some(1)
        );
        assert!(dir.0.join("savedat2.dat").exists());
    }

    #[test]
    fn a_pad_cancel_leaves_and_a_pad_confirm_loads_the_highlighted_row() {
        let dir = two_saves("load");
        let mut cx = context(&dir);
        let mut screen = ModernSaveScreen::open_load(&mut cx).unwrap();
        let rows = view(&screen).rows.unwrap();
        press(&mut screen, |ui| ui.down = true);
        press(&mut screen, |ui| ui.confirm = true);
        let cx = context(&dir);
        assert_eq!(
            screen.update(&cx, UiInput::default()),
            ScreenResult::Done(ScreenAction::LoadGame {
                slot: rows[1] as usize
            })
        );
        let mut cx = context(&dir);
        let mut screen = ModernSaveScreen::open_load(&mut cx).unwrap();
        press(&mut screen, |ui| ui.cancel = true);
        let cx = context(&dir);
        assert_eq!(
            screen.update(&cx, UiInput::default()),
            ScreenResult::Done(ScreenAction::Title)
        );
    }

    #[test]
    fn pad_events_map_each_press_to_one_key() {
        let ui = UiInput {
            up: true,
            confirm: true,
            held_down: true,
            ..UiInput::default()
        };
        let keys: Vec<gui::GuiKey> = pad_events(&ui)
            .into_iter()
            .map(|event| match event {
                gui::GuiEvent::Key(key, _) => key,
                other => panic!("not a key: {other:?}"),
            })
            .collect();
        assert_eq!(keys, [gui::GuiKey::Up, gui::GuiKey::Confirm]);
        assert!(pad_events(&UiInput::default()).is_empty());
    }
}
