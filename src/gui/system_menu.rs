//! The in-game menu: an overlay over the frozen game with Restart, Load
//! Game, Options and Quit, their confirmation dialogs, the options screen
//! and the key configuration screen it opens.
//!
//! The menu and the options are the `menu` and `options` screens of a
//! [`Ui`] (`assets/ui/*.xml`); this owner publishes the settings as
//! `options.<id>` definitions and turns what the screens report back into
//! [`MenuRequest`]s. It never touches the game. The key configuration is
//! still its own screen, drawn over the options.

use anyhow::Result;

use super::canvas::{Canvas, Rect};
use super::dom::{Custom, Library, Node, Ui, UiOutput, Value};
use super::input::{Device, GuiEvent, PadStyle, Sound};
use super::key_config::{KeyConfigEvent, KeyConfigScreen};
use super::locale::Locale;
use super::options::OptionKind;
use super::pad_diagram;
use super::settings::{self, Settings};
use super::text::Fonts;
use super::theme::{Theme, palette};
use super::widgets::Painter;
use crate::controls::Bindings;

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

/// A version without its trailing zero parts beyond the second, as
/// drivers are usually named ("616.56.0.0" is 616.56).
fn short_version(version: &str) -> String {
    let mut parts: Vec<&str> = version.split('.').collect();
    while parts.len() > 2 && parts.last() == Some(&"0") {
        parts.pop();
    }
    parts.join(".")
}

/// The options whose description shows the controller diagram.
const PAD_PAGE: [&str; 3] = ["control_type", "button_style", "vibration"];

/// The controller diagram under a Controller row's description.
struct PadPaint {
    style: PadStyle,
    bindings: Bindings,
    text: Locale,
}

impl Custom for PadPaint {
    fn paint(&self, p: &mut Painter<'_>, rect: Rect, _node: &Node) {
        // A long description in large text can leave no room for it.
        if rect.h > p.u(120.0) {
            pad_diagram::draw(p, rect, self.style, &self.bindings, &self.text);
        }
    }
}

/// The key configuration and what it draws with.
struct KeyConfig {
    screen: KeyConfigScreen,
    fonts: Fonts,
    canvas: Canvas,
    dirty: bool,
}

pub struct SystemMenu {
    ui: Ui,
    settings: Settings,
    /// The strings in the settings' language.
    text: Locale,
    /// The key configuration, over the options.
    key_config: Option<KeyConfig>,
    /// The device of the last key, as detected; [`Self::shown_device`]
    /// applies the button-style setting.
    device: Device,
    recording: bool,
}

impl SystemMenu {
    pub fn new(settings: Settings) -> Result<Self> {
        let mut menu = Self {
            ui: Ui::new(Library::builtin())?,
            text: settings.locale(),
            settings,
            key_config: None,
            device: Device::Keyboard,
            recording: false,
        };
        menu.refresh_options();
        menu.ui.open("menu")?;
        Ok(menu)
    }

    /// The canvas the last [`Self::draw`] painted.
    pub fn canvas(&self) -> &Canvas {
        match &self.key_config {
            Some(keys) => &keys.canvas,
            None => self.ui.canvas(),
        }
    }

    /// Record the canvas for the GPU (or paint it) from the next draw.
    pub fn set_recording(&mut self, on: bool) {
        self.recording = on;
        self.ui.set_recording(on);
        if let Some(keys) = &mut self.key_config {
            keys.dirty |= keys.canvas.set_recording(on);
        }
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
            self.refresh_options();
        }
    }

    /// Publish the settings: the language, the look and every option.
    fn refresh_options(&mut self) {
        // Comparing tags is cheap; building the strings is not.
        let tag = crate::language::resolve_tag(self.settings.language.as_deref());
        if self.text.tag() != tag {
            self.text = Locale::new(&tag);
        }
        self.ui.set_locale(self.text.clone());
        self.ui.set_theme(
            self.settings.game.ui_font,
            self.settings.game.menu_text_size.scale(),
        );
        let device = self.shown_device();
        self.ui.set_device(device);
        if let Some(keys) = &mut self.key_config {
            keys.screen.set_locale(self.text.clone());
            keys.screen.set_device(device);
            keys.screen.set_bindings(self.settings.bindings.clone());
            keys.dirty = true;
        }
        let style = match (device, self.settings.game.button_style) {
            (Device::Gamepad(style), _) => style,
            (Device::Keyboard, style) => style.unwrap_or_default(),
        };
        self.ui.register(
            "pad-diagram",
            Box::new(PadPaint {
                style,
                bindings: self.settings.bindings.clone(),
                text: self.text.clone(),
            }),
        );
        let specs = self.specs();
        for def in settings::defs(&self.settings, &self.text) {
            let value = settings::value(&def.id, &self.settings);
            let mut fields = vec![
                ("label", Value::from(def.label)),
                ("description", Value::from(def.description)),
                ("default", Value::from(def.default)),
                ("value", Value::from(value)),
                ("disabled", Value::Bool(def.disabled)),
                ("pad", Value::Bool(PAD_PAGE.contains(&def.id.as_str()))),
            ];
            match def.kind {
                OptionKind::Choice(choices) => {
                    fields.push((
                        "choices",
                        Value::list(
                            choices
                                .iter()
                                .map(|c| Value::from(c.label.as_str()))
                                .collect(),
                        ),
                    ));
                    fields.push((
                        "choice_descriptions",
                        Value::list(
                            choices
                                .iter()
                                .map(|c| Value::from(c.description.clone().unwrap_or_default()))
                                .collect(),
                        ),
                    ));
                }
                OptionKind::Slider {
                    min,
                    max,
                    step,
                    suffix,
                } => {
                    fields.push(("min", Value::from(min)));
                    fields.push(("max", Value::from(max)));
                    fields.push(("step", Value::from(step)));
                    fields.push(("suffix", Value::from(suffix)));
                }
                OptionKind::Link | OptionKind::Info => {}
            }
            if def.id == "pc_specs" {
                fields.push((
                    "facts",
                    Value::list(
                        specs
                            .iter()
                            .map(|(label, value)| {
                                Value::map([
                                    ("label", Value::from(label.as_str())),
                                    ("value", Value::from(value.as_str())),
                                ])
                            })
                            .collect(),
                    ),
                ));
            }
            self.ui
                .set(&format!("options.{}", def.id), Value::map(fields));
        }
    }

    /// The PC Specs row's table: what is known of the machine.
    fn specs(&self) -> Vec<(String, String)> {
        let system = &self.settings.system;
        let text = &self.text;
        let cpu = system
            .cpu
            .clone()
            .unwrap_or_else(|| text.format("specs.cores", system.cpu_cores));
        let ram = text.format("specs.gb", format!("{:.2}", system.ram_mb as f32 / 1024.0));
        let api = system.gpu.api.as_deref().map(|api| match api {
            "vulkan" => "Vulkan".to_string(),
            "direct3d12" => "Direct3D 12".to_string(),
            "metal" => "Metal".to_string(),
            other => other.to_string(),
        });
        [
            ("specs.cpu", Some(cpu)),
            ("specs.ram", Some(ram)),
            ("specs.gpu", system.gpu.name.clone()),
            (
                "specs.driver",
                system.gpu.driver_version.as_deref().map(short_version),
            ),
            ("specs.api", api),
        ]
        .into_iter()
        .filter_map(|(key, value)| Some((text.get(key).to_string(), value?)))
        .collect()
    }

    fn open_key_config(&mut self) -> Result<()> {
        let mut screen = KeyConfigScreen::new(self.settings.bindings.clone(), self.text.clone());
        screen.set_device(self.shown_device());
        let mut canvas = Canvas::new(1, 1);
        canvas.set_recording(self.recording);
        self.key_config = Some(KeyConfig {
            screen,
            fonts: Fonts::new()?,
            canvas,
            dirty: true,
        });
        Ok(())
    }

    /// Whether the key configuration waits for a key: the owner hands the
    /// next raw press to [`Self::capture`] instead of feeding events.
    pub fn capturing(&self) -> bool {
        self.key_config
            .as_ref()
            .is_some_and(|keys| keys.screen.capturing())
    }

    /// Bind the raw press the key configuration waited for; `click` is
    /// where the pointer went down this frame, if it did.
    pub fn capture(
        &mut self,
        source: crate::controls::Source,
        click: Option<(f32, f32)>,
    ) -> Vec<MenuRequest> {
        let mut requests = Vec::new();
        if let Some(keys) = &mut self.key_config {
            let before = keys.screen.cursor();
            let events = keys.screen.capture(source, click);
            keys.dirty = true;
            self.key_config_events(before, events, &mut requests);
        }
        requests
    }

    /// Carry out the key configuration's events and sound the step.
    fn key_config_events(
        &mut self,
        before: (usize, usize, usize, bool, Option<usize>),
        events: Vec<KeyConfigEvent>,
        requests: &mut Vec<MenuRequest>,
    ) {
        let mut changed = false;
        let mut back = false;
        for event in events {
            match event {
                KeyConfigEvent::Back => back = true,
                KeyConfigEvent::Changed(bindings) => {
                    self.settings.bindings = *bindings;
                    changed = true;
                    requests.push(MenuRequest::Settings(Box::new(self.settings.clone())));
                }
            }
        }
        let after = self.key_config.as_ref().map(|keys| keys.screen.cursor());
        let deep = |c: (usize, usize, usize, bool, Option<usize>)| c.3 || c.4.is_some();
        let sound = match after {
            _ if back => Some(Sound::Cancel),
            Some(after) if deep(after) && !deep(before) => Some(Sound::Decide),
            Some(after) if deep(before) && !deep(after) => Some(if changed {
                Sound::Decide
            } else {
                Sound::Cancel
            }),
            Some(after) if changed || after != before => Some(Sound::Cursor),
            _ => None,
        };
        if back {
            self.key_config = None;
            self.ui.invalidate();
        }
        if changed {
            self.refresh_options();
        }
        if let Some(sound) = sound
            && self.settings.game.menu_volume > 0
        {
            requests.push(MenuRequest::Sound(sound));
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
        for &event in events {
            let event = match event {
                GuiEvent::Key(key, device) => {
                    self.device = device;
                    GuiEvent::Key(key, self.shown_device())
                }
                other => other,
            };
            if let Some(keys) = &mut self.key_config {
                let before = keys.screen.cursor();
                let events = keys.screen.handle(event);
                keys.dirty = true;
                self.key_config_events(before, events, &mut requests);
                continue;
            }
            if self.ui.is_empty() {
                break;
            }
            for output in self.ui.handle(&[event]) {
                self.output(output, &mut requests);
            }
        }
        requests
    }

    fn output(&mut self, output: UiOutput, requests: &mut Vec<MenuRequest>) {
        match output {
            UiOutput::Sound(sound) => {
                if self.settings.game.menu_volume > 0 {
                    requests.push(MenuRequest::Sound(sound));
                }
            }
            UiOutput::Closed(_) if self.ui.is_empty() => requests.push(MenuRequest::Resume),
            UiOutput::Action { name, .. } => match name.as_str() {
                "restart" => requests.push(MenuRequest::Restart),
                "load_game" => requests.push(MenuRequest::LoadGame),
                "quit" => requests.push(MenuRequest::Quit),
                "key_config" => {
                    if let Err(err) = self.open_key_config() {
                        eprintln!("warning: key configuration: {err:#}");
                    }
                }
                other => eprintln!("warning: menu: unknown action {other:?}"),
            },
            UiOutput::Changed { path, value } => {
                let id = path
                    .strip_prefix("options.")
                    .and_then(|p| p.strip_suffix(".value"));
                if let (Some(id), Some(value)) = (id, value.as_int()) {
                    settings::apply(id, value as i32, &mut self.settings);
                    requests.push(MenuRequest::Settings(Box::new(self.settings.clone())));
                    self.refresh_options();
                }
            }
            UiOutput::Opened(_) | UiOutput::Closed(_) => {}
        }
    }

    /// Draw the menu at `width` x `height` pixels into [`Self::canvas`],
    /// returning whether anything was repainted: a menu at rest (no input,
    /// the same size) keeps its last frame.
    pub fn draw(&mut self, width: u32, height: u32) -> bool {
        let Some(keys) = &mut self.key_config else {
            return self.ui.draw(width, height);
        };
        let (width, height) = (width.max(1), height.max(1));
        let resized = (keys.canvas.width, keys.canvas.height) != (width, height);
        if resized {
            keys.fonts.clear_cache();
        }
        if !keys.dirty && !resized {
            return false;
        }
        keys.dirty = false;
        keys.canvas.reset_to(width, height, palette::SCREEN_SHADE);
        let theme = Theme::new(width, height, self.settings.game.ui_font)
            .with_text_scale(self.settings.game.menu_text_size.scale());
        let mut p = Painter {
            canvas: &mut keys.canvas,
            fonts: &mut keys.fonts,
            theme,
        };
        keys.screen.draw(&mut p);
        // The menu shows again when the key configuration closes.
        self.ui.invalidate();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;
    use crate::gui::PadStyle;
    use crate::gui::input::GuiKey;

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
        assert_eq!(menu.ui.top(), Some("menu"));
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
        // Options, then the first row (Chris's ink ribbons) one step left.
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
        assert!(!settings.game.chris_ink_ribbons);
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
        menu.handle(&[
            key(GuiKey::TabNext),
            key(GuiKey::TabNext),
            key(GuiKey::Down),
            key(GuiKey::Down),
        ]);
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
        menu.capture(crate::controls::Source::Mouse(3), None);
        shot(&mut menu, "keys_mouse", 1920, 1080);
        menu.handle(&[key(GuiKey::TabNext), key(GuiKey::Up)]);
        shot(&mut menu, "keys_menu", 1920, 1080);
        shot(&mut menu, "keys_4x3", 1024, 768);
        // The controller diagram, per icon type and with type B.
        let mut menu = self::menu();
        menu.handle(&[
            key(GuiKey::Down),
            key(GuiKey::Down),
            key(GuiKey::Confirm),
            key(GuiKey::SubNext),
        ]);
        for (style, name) in [
            (PadStyle::Xbox, "xbox"),
            (PadStyle::PlayStation, "playstation"),
            (PadStyle::Nintendo, "nintendo"),
        ] {
            menu.handle(&[GuiEvent::Key(GuiKey::Down, Device::Gamepad(style))]);
            shot(&mut menu, &format!("pad_{name}"), 1920, 1080);
        }
        let mut settings = menu.settings().clone();
        settings.game.control_type = crate::config::ControlType::B;
        settings.bindings = settings
            .bindings
            .clone()
            .with_control_type(crate::config::ControlType::B);
        menu.set_settings(settings);
        shot(&mut menu, "pad_type_b", 1920, 1080);
        shot(&mut menu, "pad_4x3", 1024, 768);
        // The PC Specs row's table.
        let mut settings = menu.settings().clone();
        settings.system = settings::SystemInfo {
            cpu: Some("AMD Ryzen 9 7950X 16-Core Processor".to_string()),
            cpu_cores: 32,
            ram_mb: 64_666,
            gpu: crate::gpu::GpuInfo {
                name: Some("NVIDIA GeForce RTX 5080".to_string()),
                driver_version: Some("616.56".to_string()),
                api: Some("vulkan".to_string()),
            },
        };
        menu.set_settings(settings);
        menu.handle(&[
            key(GuiKey::TabNext),
            key(GuiKey::TabNext),
            key(GuiKey::SubNext),
        ]);
        shot(&mut menu, "options_specs", 1920, 1080);
        menu.handle(&[key(GuiKey::TabNext)]);
        shot(&mut menu, "options_audio_single", 1920, 1080);
    }

    #[test]
    fn driver_versions_drop_their_trailing_zeros() {
        assert_eq!(short_version("616.56.0.0"), "616.56");
        assert_eq!(short_version("32.0.15.6094"), "32.0.15.6094");
        assert_eq!(short_version("1.0"), "1.0");
    }

    #[test]
    fn changing_the_language_relabels_the_open_options() {
        let mut menu = menu();
        menu.handle(&[key(GuiKey::Up), key(GuiKey::Up), key(GuiKey::Confirm)]);
        menu.handle(&[key(GuiKey::TabNext), key(GuiKey::Down)]);
        assert_eq!(menu.ui.top(), Some("options"));
        let tabs = menu.ui.tab_indices();
        let mut settings = menu.settings().clone();
        settings.language = Some("de-DE".to_string());
        menu.set_settings(settings);
        menu.ui.handle(&[]);
        assert_eq!(menu.text.get("menu.quit"), "Beenden");
        assert_eq!(menu.ui.tab_indices(), tabs, "the tabs stay");
        assert_eq!(
            menu.ui.focus_prop("label"),
            Value::from(menu.text.get("analog_movement.label")),
            "the cursor stays, relabelled"
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
        assert_eq!(menu.ui.top(), Some("options"));
        assert_eq!(menu.ui.device(), Device::Gamepad(PadStyle::Xbox));
    }

    #[test]
    fn button_style_overrides_the_detected_controller() {
        let mut menu = menu();
        let ps = |key| GuiEvent::Key(key, Device::Gamepad(PadStyle::PlayStation));
        menu.handle(&[ps(GuiKey::Up), ps(GuiKey::Up), ps(GuiKey::Confirm)]);
        let shown = |menu: &SystemMenu| menu.ui.device();
        assert_eq!(shown(&menu), Device::Gamepad(PadStyle::PlayStation));
        let mut settings = menu.settings().clone();
        settings.game.button_style = Some(PadStyle::Nintendo);
        menu.set_settings(settings.clone());
        assert_eq!(shown(&menu), Device::Gamepad(PadStyle::Nintendo));
        menu.handle(&[ps(GuiKey::Down)]);
        assert_eq!(shown(&menu), Device::Gamepad(PadStyle::Nintendo));
        // Automatic again: back to what the controller is.
        settings.game.button_style = None;
        menu.set_settings(settings);
        assert_eq!(shown(&menu), Device::Gamepad(PadStyle::PlayStation));
        // The keyboard keeps its caps whatever the style.
        menu.handle(&[key(GuiKey::Down)]);
        assert_eq!(shown(&menu), Device::Keyboard);
    }

    #[test]
    fn every_setting_has_one_row() {
        let source = include_str!("../../assets/ui/options.xml");
        for def in settings::defs(&Settings::default(), &Locale::default()) {
            let binding = format!("{{options.{}}}", def.id);
            assert_eq!(
                source.matches(&binding).count(),
                1,
                "{} needs exactly one row in options.xml",
                def.id
            );
        }
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
        assert!(menu.draw(640, 480), "the first frame paints");
        assert!(!menu.draw(640, 480), "a menu at rest keeps its frame");
        assert!(menu.draw(800, 600), "a resize repaints");
        menu.handle(&[key(GuiKey::Down)]);
        assert!(menu.draw(800, 600), "input repaints");
        assert!(!menu.draw(800, 600));
    }
}
