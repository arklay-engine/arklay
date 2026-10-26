//! The in-game menu: an overlay over the frozen game with Restart, Load
//! Game, Options and Quit down the left side, their confirmation dialogs,
//! the options screen and the key configuration screen it opens.
//!
//! The menu owns its canvas and faces and draws at the window's native
//! resolution. It never touches the game: every decision comes back as a
//! [`MenuRequest`] for the session or the app to carry out.

use std::time::Instant;

use anyhow::Result;

use super::canvas::{Axis, Canvas, Color, Rect};
use super::dialog::{Dialog, DialogResult};
use super::input::{Device, GuiEvent, GuiKey, Sound};
use super::key_config::{KeyConfigEvent, KeyConfigScreen};
use super::locale::Locale;
use super::options_screen::{OptionsEvent, OptionsScreen};
use super::settings::{self, Settings};
use super::text::{Align, Fonts};
use super::theme::{Role, Theme, palette};
use super::widgets::{Painter, SELECTION_GLOW};

/// What the menu asks its owner to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuRequest {
    /// Close the menu and resume play.
    Resume,
    /// Reload the last save (or restart the new game).
    Restart,
    /// Open the load screen.
    LoadGame,
    /// Return to the title screen.
    Quit,
    /// The options changed; apply and persist these settings.
    Settings(Box<Settings>),
    /// Play a feedback sound.
    Sound(Sound),
}

/// Where the menu's cursor is, compared across one event to pick its sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct View {
    /// How deep the menu is: 0 the item list, 1 its dialog, 2 the options, 3
    /// the options' dialog, 4 the key configuration, 5 its dialog or a key
    /// being captured.
    depth: u8,
    selected: usize,
    dialog: Option<usize>,
    options: Option<(usize, usize, usize, Option<usize>)>,
    keys: Option<(usize, usize, usize, bool, Option<usize>)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Item {
    Restart,
    LoadGame,
    Options,
    Quit,
}

/// The items and their label keys.
const ITEMS: [(Item, &str); 4] = [
    (Item::Restart, "menu.restart"),
    (Item::LoadGame, "menu.load_game"),
    (Item::Options, "menu.options"),
    (Item::Quit, "menu.quit"),
];

pub struct SystemMenu {
    fonts: Fonts,
    canvas: Canvas,
    settings: Settings,
    /// The strings in the settings' language.
    text: Locale,
    selected: usize,
    hits: Vec<Rect>,
    hint_hits: Vec<(Rect, GuiKey)>,
    dialog: Option<(Dialog, Item)>,
    options: Option<OptionsScreen>,
    /// The key configuration, over the options.
    key_config: Option<KeyConfigScreen>,
    /// The device of the last key, as detected; [`Self::shown_device`]
    /// applies the button-style setting.
    device: Device,
    opened: Instant,
    /// Whether the canvas is stale.
    dirty: bool,
    /// The opening animation's progress the canvas shows.
    drawn_appear: f32,
}

/// How long the menu takes to slide and fade in.
const APPEAR_SECONDS: f32 = 0.18;

impl SystemMenu {
    pub fn new(settings: Settings) -> Result<Self> {
        Ok(Self {
            fonts: Fonts::new()?,
            canvas: Canvas::new(1, 1),
            text: settings.locale(),
            settings,
            selected: 0,
            hits: Vec::new(),
            hint_hits: Vec::new(),
            dialog: None,
            options: None,
            key_config: None,
            device: Device::Keyboard,
            opened: Instant::now(),
            dirty: true,
            drawn_appear: 0.0,
        })
    }

    /// Finish the opening animation, for screenshots.
    #[cfg(test)]
    fn skip_intro(&mut self) {
        if let Some(earlier) = self.opened.checked_sub(std::time::Duration::from_secs(1)) {
            self.opened = earlier;
        }
    }

    /// The canvas the last [`Self::draw`] painted.
    pub fn canvas(&self) -> &Canvas {
        &self.canvas
    }

    /// The settings as the menu last left them.
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Replace the settings after the owner applied them (an option the
    /// platform refused, a renderer that fell back to software).
    pub fn set_settings(&mut self, settings: Settings) {
        if settings != self.settings {
            self.settings = settings;
            self.dirty = true;
            self.refresh_options();
        }
    }

    /// Rebuild the options from the settings, in their language.
    fn refresh_options(&mut self) {
        // Comparing tags is cheap; building the strings is not.
        let tag = crate::language::resolve_tag(self.settings.language.as_deref());
        if self.text.tag() != tag {
            self.text = Locale::new(&tag);
            if let Some(screen) = &mut self.options {
                screen.set_locale(self.text.clone());
            }
        }
        let device = self.shown_device();
        if let Some(screen) = &mut self.key_config {
            screen.set_locale(self.text.clone());
            screen.set_device(device);
            screen.set_bindings(self.settings.bindings.clone());
        }
        if let Some(screen) = &mut self.options {
            screen.set_device(device);
            let defs = settings::defs(&self.settings, &self.text);
            let values = defs
                .iter()
                .map(|d| settings::value(&d.id, &self.settings))
                .collect();
            screen.refresh(defs, values);
        }
    }

    fn open_options(&mut self) {
        let defs = settings::defs(&self.settings, &self.text);
        let values = defs
            .iter()
            .map(|d| settings::value(&d.id, &self.settings))
            .collect();
        let mut screen = OptionsScreen::new(defs, values, self.text.clone());
        screen.set_device(self.shown_device());
        self.options = Some(screen);
    }

    fn open_key_config(&mut self) {
        let mut screen = KeyConfigScreen::new(self.settings.bindings.clone(), self.text.clone());
        screen.set_device(self.shown_device());
        self.key_config = Some(screen);
    }

    /// Whether the key configuration waits for a key: the owner hands the
    /// next raw press to [`Self::capture`] instead of feeding events.
    pub fn capturing(&self) -> bool {
        self.key_config
            .as_ref()
            .is_some_and(KeyConfigScreen::capturing)
    }

    /// Bind the raw press the key configuration waited for.
    pub fn capture(&mut self, source: crate::controls::Source) -> Vec<MenuRequest> {
        let mut requests = Vec::new();
        let before = self.view();
        if let Some(screen) = &mut self.key_config {
            let events = screen.capture(source);
            self.key_config_events(events, &mut requests);
            self.dirty = true;
        }
        if let Some(sound) = self.sound(before, &requests)
            && self.settings.game.menu_volume > 0
        {
            requests.push(MenuRequest::Sound(sound));
        }
        requests
    }

    fn key_config_events(&mut self, events: Vec<KeyConfigEvent>, requests: &mut Vec<MenuRequest>) {
        for event in events {
            match event {
                KeyConfigEvent::Back => self.key_config = None,
                KeyConfigEvent::Changed(bindings) => {
                    self.settings.bindings = *bindings;
                    requests.push(MenuRequest::Settings(Box::new(self.settings.clone())));
                }
            }
        }
    }

    /// The device the hints draw for: a controller shows the configured
    /// button style, or its own when that is automatic.
    fn shown_device(&self) -> Device {
        match (self.device, self.settings.game.button_style) {
            (Device::Gamepad(_), Some(style)) => Device::Gamepad(style),
            (device, _) => device,
        }
    }

    /// Feed the frame's events; returns the requests in order.
    pub fn handle(&mut self, events: &[GuiEvent]) -> Vec<MenuRequest> {
        let mut requests = Vec::new();
        // Any input may change what is drawn (a hover, a step); a pointer
        // that moves over nothing costs one repaint.
        self.dirty |= !events.is_empty();
        for &event in events {
            let event = match event {
                GuiEvent::Key(key, device) => {
                    self.device = device;
                    GuiEvent::Key(key, self.shown_device())
                }
                other => other,
            };
            let before = self.view();
            let start = requests.len();
            self.handle_one(event, &mut requests);
            if let Some(sound) = self.sound(before, &requests[start..])
                && self.settings.game.menu_volume > 0
            {
                requests.push(MenuRequest::Sound(sound));
            }
        }
        requests
    }

    fn view(&self) -> View {
        let options = self.options.as_ref().map(OptionsScreen::cursor);
        let keys = self.key_config.as_ref().map(KeyConfigScreen::cursor);
        View {
            depth: match (&keys, &options, &self.dialog) {
                (Some((.., true, _) | (.., Some(_))), ..) => 5,
                (Some(_), ..) => 4,
                (None, Some((.., Some(_))), _) => 3,
                (None, Some(_), _) => 2,
                (None, None, Some(_)) => 1,
                (None, None, None) => 0,
            },
            selected: self.selected,
            dialog: self.dialog.as_ref().map(|(dialog, _)| dialog.selected),
            options,
            keys,
        }
    }

    /// The sound one event earns, given the view before it and the requests
    /// it raised: going deeper or carrying something out decides, backing
    /// out cancels, and any other move of the cursor or a value ticks.
    fn sound(&self, before: View, requests: &[MenuRequest]) -> Option<Sound> {
        let after = self.view();
        let acted = requests.iter().any(|request| {
            matches!(
                request,
                MenuRequest::Restart | MenuRequest::LoadGame | MenuRequest::Quit
            )
        });
        let changed = requests
            .iter()
            .any(|request| matches!(request, MenuRequest::Settings(_)));
        if requests.contains(&MenuRequest::Resume) {
            Some(Sound::Cancel)
        } else if acted || after.depth > before.depth {
            Some(Sound::Decide)
        } else if after.depth < before.depth {
            // A dialog that reset options was answered Yes.
            Some(if changed {
                Sound::Decide
            } else {
                Sound::Cancel
            })
        } else if changed || after != before {
            Some(Sound::Cursor)
        } else {
            None
        }
    }

    fn handle_one(&mut self, event: GuiEvent, requests: &mut Vec<MenuRequest>) {
        if let Some(screen) = &mut self.key_config {
            let events = screen.handle(event);
            self.key_config_events(events, requests);
            return;
        }
        if let Some(screen) = &mut self.options {
            for result in screen.handle(event) {
                match result {
                    OptionsEvent::Back => self.options = None,
                    OptionsEvent::Open { index } => {
                        let defs = settings::defs(&self.settings, &self.text);
                        if defs.get(index).is_some_and(|def| def.id == "key_config") {
                            self.open_key_config();
                        }
                    }
                    OptionsEvent::Changed { index, value } => {
                        let defs = settings::defs(&self.settings, &self.text);
                        if let Some(def) = defs.get(index) {
                            settings::apply(&def.id, value, &mut self.settings);
                            requests.push(MenuRequest::Settings(Box::new(self.settings.clone())));
                        }
                    }
                }
            }
            if self.options.is_some() {
                self.refresh_options();
            }
            return;
        }
        if let Some((dialog, item)) = &mut self.dialog {
            match dialog.handle(event) {
                DialogResult::Open => {}
                DialogResult::Chosen(0) => {
                    let item = *item;
                    self.dialog = None;
                    requests.push(match item {
                        Item::Restart => MenuRequest::Restart,
                        _ => MenuRequest::Quit,
                    });
                }
                DialogResult::Chosen(_) | DialogResult::Cancelled => self.dialog = None,
            }
            return;
        }
        let count = ITEMS.len();
        match event {
            GuiEvent::Key(GuiKey::Up, _) => self.selected = (self.selected + count - 1) % count,
            GuiEvent::Key(GuiKey::Down, _) => self.selected = (self.selected + 1) % count,
            GuiEvent::Key(GuiKey::Confirm, _) => self.activate(requests),
            GuiEvent::Key(GuiKey::Cancel, _) => requests.push(MenuRequest::Resume),
            GuiEvent::PointerMove { x, y } => {
                if let Some(index) = self.hits.iter().position(|r| r.contains(x, y)) {
                    self.selected = index;
                }
            }
            GuiEvent::PointerDown { x, y } => {
                if let Some(index) = self.hits.iter().position(|r| r.contains(x, y)) {
                    self.selected = index;
                    self.activate(requests);
                } else if let Some(&(_, key)) =
                    self.hint_hits.iter().find(|(r, _)| r.contains(x, y))
                {
                    self.handle_one(GuiEvent::Key(key, self.shown_device()), requests);
                }
            }
            _ => {}
        }
    }

    fn activate(&mut self, requests: &mut Vec<MenuRequest>) {
        match ITEMS[self.selected].0 {
            Item::Restart => {
                self.dialog = Some((
                    Dialog::yes_no(
                        &self.text,
                        self.text.get("menu.restart_question"),
                        Some(self.text.get("menu.unsaved_note")),
                    ),
                    Item::Restart,
                ))
            }
            Item::LoadGame => requests.push(MenuRequest::LoadGame),
            Item::Options => self.open_options(),
            Item::Quit => {
                self.dialog = Some((
                    Dialog::yes_no(
                        &self.text,
                        self.text.get("menu.quit_question"),
                        Some(self.text.get("menu.unsaved_note")),
                    ),
                    Item::Quit,
                ))
            }
        }
    }

    /// Draw the menu at `width` x `height` pixels into [`Self::canvas`],
    /// returning whether anything was repainted: a menu at rest (no input,
    /// no animation, the same size) keeps its last frame.
    pub fn draw(&mut self, width: u32, height: u32) -> bool {
        let (width, height) = (width.max(1), height.max(1));
        let resized = (self.canvas.width, self.canvas.height) != (width, height);
        if resized {
            self.fonts.clear_cache();
        }
        let t = self.opened.elapsed().as_secs_f32();
        let appear = (t / APPEAR_SECONDS).min(1.0);
        if !self.dirty && !resized && self.drawn_appear >= 1.0 {
            return false;
        }
        self.dirty = false;
        self.drawn_appear = appear;
        let shade = if self.options.is_some() || self.key_config.is_some() {
            palette::SCREEN_SHADE
        } else {
            palette::MENU_SHADE.fade(appear)
        };
        self.canvas.reset_to(width, height, shade);
        let theme = Theme::new(width, height, self.settings.game.ui_font)
            .with_text_scale(self.settings.game.menu_text_size.scale());
        let device = self.shown_device();
        let mut p = Painter {
            canvas: &mut self.canvas,
            fonts: &mut self.fonts,
            theme,
        };
        if let Some(screen) = &mut self.key_config {
            screen.draw(&mut p);
            return true;
        }
        if let Some(screen) = &mut self.options {
            screen.draw(&mut p);
            return true;
        }
        draw_menu(
            &mut p,
            &self.text,
            self.selected,
            &mut self.hits,
            &mut self.hint_hits,
            device,
            appear,
        );
        if let Some((dialog, _)) = &mut self.dialog {
            dialog.draw(&mut p, SELECTION_GLOW);
        }
        true
    }
}

fn draw_menu(
    p: &mut Painter<'_>,
    text: &Locale,
    selected: usize,
    hits: &mut Vec<Rect>,
    hint_hits: &mut Vec<(Rect, GuiKey)>,
    device: Device,
    appear: f32,
) {
    let pulse = SELECTION_GLOW;
    let bounds = p.canvas.bounds();
    let scale = p.theme.scale;
    let u = |v: f32| v * scale;
    // The canvas starts darkened; heavier towards the menu side.
    p.canvas.fill_gradient(
        Rect::new(0.0, 0.0, bounds.w * 0.55, bounds.h),
        Axis::Horizontal,
        Color::rgba(0, 0, 0, (200.0 * appear) as u8),
        Color::rgba(0, 0, 0, 0),
    );

    let left = bounds.w * 0.07;
    let row_h = u(78.0);
    let top = bounds.h * 0.5 - row_h * ITEMS.len() as f32 / 2.0;
    // Slide in from the left as the menu appears.
    let slide = (1.0 - appear) * u(-60.0);
    hits.clear();
    let width = u(560.0);
    for (index, &(_, key)) in ITEMS.iter().enumerate() {
        let rect = Rect::new(
            left - u(40.0) + slide,
            top + row_h * index as f32,
            width,
            row_h - u(10.0),
        );
        let active = index == selected;
        if active {
            // A sheen fading out to the right.
            p.canvas.fill_gradient(
                rect,
                Axis::Horizontal,
                Color::rgba(0xC8, 0xC8, 0xC8, (90.0 + 40.0 * pulse) as u8),
                Color::rgba(0xC8, 0xC8, 0xC8, 0),
            );
            p.canvas
                .fill_rect(Rect::new(rect.x, rect.y, u(5.0), rect.h), palette::ACCENT);
        }
        let color = if active {
            palette::TEXT
        } else {
            palette::TEXT_DIM
        };
        p.text(
            Role::Menu,
            left + slide,
            rect.center_y(),
            Align::Left,
            color.fade(appear),
            text.get(key),
        );
        hits.push(rect);
    }

    let hints = [
        (GuiKey::Confirm, text.get("hint.select")),
        (GuiKey::Cancel, text.get("hint.back")),
    ];
    let rects = p.hint_bar(
        bounds.w / 2.0,
        u(1040.0).min(bounds.h - u(40.0)),
        device,
        &hints,
    );
    hint_hits.clear();
    hint_hits.extend(rects.into_iter().zip([GuiKey::Confirm, GuiKey::Cancel]));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;
    use crate::gui::PadStyle;

    fn menu() -> SystemMenu {
        SystemMenu::new(Settings::default()).unwrap()
    }

    fn key(key: GuiKey) -> GuiEvent {
        GuiEvent::Key(key, Device::Keyboard)
    }

    /// The requests without the feedback sounds.
    fn actions(menu: &mut SystemMenu, events: &[GuiEvent]) -> Vec<MenuRequest> {
        let mut requests = menu.handle(events);
        requests.retain(|request| !matches!(request, MenuRequest::Sound(_)));
        requests
    }

    /// The feedback sounds alone.
    fn sounds(menu: &mut SystemMenu, events: &[GuiEvent]) -> Vec<Sound> {
        menu.handle(events)
            .into_iter()
            .filter_map(|request| match request {
                MenuRequest::Sound(sound) => Some(sound),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn every_step_sounds() {
        use Sound::{Cancel, Cursor, Decide};
        let mut menu = menu();
        assert_eq!(sounds(&mut menu, &[key(GuiKey::Down)]), [Cursor]);
        assert_eq!(sounds(&mut menu, &[key(GuiKey::Left)]), [], "nothing moved");
        // Options opens, a value steps, its page changes, it closes.
        assert_eq!(
            sounds(&mut menu, &[key(GuiKey::Down), key(GuiKey::Confirm)]),
            [Cursor, Decide]
        );
        assert_eq!(sounds(&mut menu, &[key(GuiKey::Left)]), [Cursor]);
        assert_eq!(sounds(&mut menu, &[key(GuiKey::TabNext)]), [Cursor]);
        assert_eq!(sounds(&mut menu, &[key(GuiKey::Cancel)]), [Cancel]);
        // Quit's dialog: open, move, answer No, open again, answer Yes.
        assert_eq!(
            sounds(
                &mut menu,
                &[
                    key(GuiKey::Down),
                    key(GuiKey::Confirm),
                    key(GuiKey::Confirm)
                ]
            ),
            [Cursor, Decide, Cancel]
        );
        assert_eq!(
            sounds(
                &mut menu,
                &[
                    key(GuiKey::Confirm),
                    key(GuiKey::Left),
                    key(GuiKey::Confirm)
                ]
            ),
            [Decide, Cursor, Decide]
        );
        assert_eq!(
            sounds(&mut menu, &[key(GuiKey::Cancel)]),
            [Cancel],
            "resume"
        );
    }

    #[test]
    fn a_silent_menu_volume_silences_every_step() {
        let mut menu = SystemMenu::new(Settings {
            game: config::Game {
                menu_volume: 0,
                ..config::Game::default()
            },
            ..Settings::default()
        })
        .unwrap();
        assert_eq!(sounds(&mut menu, &[key(GuiKey::Down)]), []);
        assert_eq!(sounds(&mut menu, &[key(GuiKey::Confirm)]), []);
        assert_eq!(sounds(&mut menu, &[key(GuiKey::Cancel)]), []);
    }

    #[test]
    fn cancel_resumes() {
        let mut menu = menu();
        assert_eq!(
            actions(&mut menu, &[key(GuiKey::Cancel)]),
            vec![MenuRequest::Resume]
        );
    }

    #[test]
    fn restart_asks_first_and_defaults_to_no() {
        let mut menu = menu();
        assert!(actions(&mut menu, &[key(GuiKey::Confirm)]).is_empty());
        // Confirming the default (No) closes the dialog only.
        assert!(actions(&mut menu, &[key(GuiKey::Confirm)]).is_empty());
        assert!(menu.dialog.is_none());
        let requests = actions(
            &mut menu,
            &[
                key(GuiKey::Confirm),
                key(GuiKey::Left),
                key(GuiKey::Confirm),
            ],
        );
        assert_eq!(requests, vec![MenuRequest::Restart]);
    }

    #[test]
    fn quit_confirms() {
        let mut menu = menu();
        let requests = actions(
            &mut menu,
            &[
                key(GuiKey::Up),
                key(GuiKey::Confirm),
                key(GuiKey::Up),
                key(GuiKey::Confirm),
            ],
        );
        assert_eq!(requests, vec![MenuRequest::Quit]);
    }

    #[test]
    fn options_change_reports_settings() {
        let mut menu = menu();
        // Options, then the first row (Quick Turn) one step left.
        let requests = actions(
            &mut menu,
            &[
                key(GuiKey::Down),
                key(GuiKey::Down),
                key(GuiKey::Confirm),
                key(GuiKey::Left),
            ],
        );
        let [MenuRequest::Settings(settings)] = requests.as_slice() else {
            panic!("{requests:?}");
        };
        assert!(!settings.game.quick_turn);
        // Back out of the options, then out of the menu.
        let requests = actions(&mut menu, &[key(GuiKey::Cancel), key(GuiKey::Cancel)]);
        assert_eq!(requests, vec![MenuRequest::Resume]);
    }

    /// `ARKLAY_GUI_DUMP=<dir> cargo test --lib gui::system_menu::tests::dump`
    /// writes each screen over a grey test pattern as BMPs for eyeballing.
    #[test]
    fn dump() {
        let Some(dir) = std::env::var_os("ARKLAY_GUI_DUMP") else {
            return;
        };
        let dir = std::path::PathBuf::from(dir);
        let mut menu = menu();
        menu.skip_intro();
        let shot = |menu: &mut SystemMenu, name: &str, w: u32, h: u32| {
            menu.draw(w, h);
            let canvas = menu.canvas();
            let mut rgba = Vec::with_capacity(canvas.rgba.len());
            for (i, px) in canvas.rgba.chunks(4).enumerate() {
                let (x, y) = (i as u32 % w, i as u32 / w);
                let bg = if (x / 64 + y / 64) % 2 == 0 {
                    90.0
                } else {
                    60.0
                };
                let keep = 1.0 - f32::from(px[3]) / 255.0;
                for &channel in &px[..3] {
                    rgba.push((f32::from(channel) + bg * keep) as u8);
                }
                rgba.push(255);
            }
            let image = crate::state::Image {
                width: w,
                height: h,
                rgba,
            };
            crate::bmp::encode(&image, &dir.join(format!("{name}.bmp"))).unwrap();
        };
        shot(&mut menu, "menu", 1920, 1080);
        menu.handle(&[key(GuiKey::Confirm)]);
        shot(&mut menu, "restart", 1920, 1080);
        menu.handle(&[
            key(GuiKey::Cancel),
            key(GuiKey::Down),
            key(GuiKey::Down),
            key(GuiKey::Confirm),
        ]);
        shot(&mut menu, "options", 1920, 1080);
        menu.handle(&[key(GuiKey::TabNext), key(GuiKey::Down), key(GuiKey::Down)]);
        shot(&mut menu, "options_graphics", 1920, 1080);
        menu.handle(&[key(GuiKey::SubNext), key(GuiKey::Down)]);
        shot(&mut menu, "options_rendering", 2560, 1080);
        shot(&mut menu, "options_4x3", 1024, 768);
        let pad = |key| GuiEvent::Key(key, Device::Gamepad(PadStyle::Xbox));
        menu.handle(&[pad(GuiKey::Up)]);
        shot(&mut menu, "options_gamepad", 1920, 1080);
        for (style, name) in [
            (PadStyle::PlayStation, "playstation"),
            (PadStyle::Nintendo, "nintendo"),
        ] {
            menu.handle(&[GuiEvent::Key(GuiKey::Down, Device::Gamepad(style))]);
            shot(&mut menu, &format!("options_{name}"), 1920, 1080);
        }
        menu.handle(&[key(GuiKey::TabNext)]);
        shot(&mut menu, "options_audio", 1920, 1080);
        menu.handle(&[pad(GuiKey::Cancel)]);
        shot(&mut menu, "menu_gamepad", 1920, 1080);
        menu.handle(&[GuiEvent::Key(
            GuiKey::Down,
            Device::Gamepad(PadStyle::PlayStation),
        )]);
        shot(&mut menu, "menu_playstation", 1920, 1080);
        for tag in ["ja-JP", "ru-RU", "de-DE"] {
            let mut settings = menu.settings().clone();
            settings.language = Some(tag.to_string());
            menu.set_settings(settings);
            menu.selected = 0;
            menu.handle(&[key(GuiKey::Confirm)]);
            shot(&mut menu, &format!("restart_{tag}"), 1920, 1080);
            menu.handle(&[
                key(GuiKey::Cancel),
                key(GuiKey::Down),
                key(GuiKey::Down),
                key(GuiKey::Confirm),
            ]);
            menu.handle(&[key(GuiKey::TabNext)]);
            shot(&mut menu, &format!("options_{tag}"), 1920, 1080);
            menu.handle(&[key(GuiKey::Cancel), key(GuiKey::Up), key(GuiKey::Up)]);
        }
        // The key configuration, from its link row.
        let mut menu = self::menu();
        menu.skip_intro();
        menu.handle(&[
            key(GuiKey::Down),
            key(GuiKey::Down),
            key(GuiKey::Confirm),
            key(GuiKey::SubNext),
            key(GuiKey::SubNext),
        ]);
        shot(&mut menu, "options_keyboard", 1920, 1080);
        menu.handle(&[key(GuiKey::Confirm)]);
        shot(&mut menu, "keys", 1920, 1080);
        menu.handle(&[key(GuiKey::Down), key(GuiKey::Right), key(GuiKey::Confirm)]);
        shot(&mut menu, "keys_capture", 1920, 1080);
        menu.capture(crate::controls::Source::Mouse(3));
        shot(&mut menu, "keys_mouse", 1920, 1080);
        menu.handle(&[key(GuiKey::TabNext), key(GuiKey::Up)]);
        shot(&mut menu, "keys_menu", 1920, 1080);
        shot(&mut menu, "keys_4x3", 1024, 768);
    }

    #[test]
    fn changing_the_language_relabels_the_open_options() {
        let mut menu = menu();
        menu.handle(&[key(GuiKey::Up), key(GuiKey::Up), key(GuiKey::Confirm)]);
        let language = menu.options.as_ref().map(|o| o.cursor());
        assert!(language.is_some());
        let mut settings = menu.settings().clone();
        settings.language = Some("de-DE".to_string());
        menu.set_settings(settings);
        assert_eq!(menu.text.get("menu.quit"), "Beenden");
        assert_eq!(
            menu.options.as_ref().map(|o| o.cursor()),
            language,
            "the cursor stays"
        );
    }

    #[test]
    fn opens_options_with_the_menus_device() {
        let mut menu = menu();
        menu.handle(&[
            GuiEvent::Key(GuiKey::Down, Device::Gamepad(PadStyle::Xbox)),
            GuiEvent::Key(GuiKey::Down, Device::Gamepad(PadStyle::Xbox)),
            GuiEvent::Key(GuiKey::Confirm, Device::Gamepad(PadStyle::Xbox)),
        ]);
        assert_eq!(
            menu.options.as_ref().map(|o| o.device),
            Some(Device::Gamepad(PadStyle::Xbox))
        );
    }

    #[test]
    fn button_style_overrides_the_detected_controller() {
        let mut menu = menu();
        let ps = |key| GuiEvent::Key(key, Device::Gamepad(PadStyle::PlayStation));
        menu.handle(&[ps(GuiKey::Up), ps(GuiKey::Up), ps(GuiKey::Confirm)]);
        let shown = |menu: &SystemMenu| menu.options.as_ref().map(|o| o.device);
        assert_eq!(shown(&menu), Some(Device::Gamepad(PadStyle::PlayStation)));
        let mut settings = menu.settings().clone();
        settings.game.button_style = Some(PadStyle::Nintendo);
        menu.set_settings(settings.clone());
        assert_eq!(shown(&menu), Some(Device::Gamepad(PadStyle::Nintendo)));
        menu.handle(&[ps(GuiKey::Down)]);
        assert_eq!(shown(&menu), Some(Device::Gamepad(PadStyle::Nintendo)));
        // Automatic again: back to what the controller is.
        settings.game.button_style = None;
        menu.set_settings(settings);
        assert_eq!(shown(&menu), Some(Device::Gamepad(PadStyle::PlayStation)));
        // The keyboard keeps its caps whatever the style.
        menu.handle(&[key(GuiKey::Down)]);
        assert_eq!(shown(&menu), Some(Device::Keyboard));
    }

    #[test]
    fn draws_at_any_size() {
        let mut menu = menu();
        for (w, h) in [(640, 480), (1920, 1080), (3440, 1440), (300, 200)] {
            menu.draw(w, h);
            let canvas = menu.canvas();
            assert_eq!((canvas.width, canvas.height), (w, h));
        }
        menu.handle(&[key(GuiKey::Down), key(GuiKey::Down), key(GuiKey::Confirm)]);
        menu.draw(1920, 1080);
        assert!(menu.canvas().rgba.chunks(4).any(|px| px[3] == 255));
    }

    #[test]
    fn repaints_only_on_change() {
        let mut menu = menu();
        menu.skip_intro();
        assert!(menu.draw(640, 480), "the first frame paints");
        assert!(!menu.draw(640, 480), "a menu at rest keeps its frame");
        assert!(menu.draw(800, 600), "a resize repaints");
        menu.handle(&[key(GuiKey::Down)]);
        assert!(menu.draw(800, 600), "input repaints");
        assert!(!menu.draw(800, 600));
    }
}
