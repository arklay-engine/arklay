//! The engine's own settings as options-screen rows.
//!
//! [`Settings`] is the slice of `arklay.toml` the in-game options edit;
//! [`defs`] describes each setting as an [`OptionDef`], [`value`] reads one
//! back as the screen's `i32` and [`apply`] writes a changed value into the
//! settings. The owner diffs the old and new settings to decide what to
//! re-apply (the window, the renderer, the player options) and persists them.

use super::input::PadStyle;
use super::locale::Locale;
use super::options::{Choice, OptionDef, OptionKind};
use super::theme::FontFamily;
use crate::config;
use crate::video::{self, Aspect, Graphics, RendererKind};

/// The settings the options screen edits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub graphics: Graphics,
    pub game: config::Game,
    /// The configured language tag; `None` follows the OS locale.
    pub language: Option<String>,
}

/// The two common shapes first, then the rarer ones.
const ASPECTS: [Aspect; 4] = [
    Aspect::STANDARD,
    Aspect::WIDE,
    Aspect {
        width: 16,
        height: 10,
    },
    Aspect {
        width: 21,
        height: 9,
    },
];

/// The aspect choices, with an unlisted configured aspect appended.
fn aspects(current: Aspect) -> Vec<Aspect> {
    let mut list = ASPECTS.to_vec();
    if !list.contains(&current) {
        list.push(current);
    }
    list
}

/// The window sizes for `graphics`' aspect, with an unlisted configured size
/// appended.
fn resolutions(graphics: &Graphics) -> Vec<(u32, u32)> {
    let mut list = crate::launcher::resolution_presets(graphics.aspect).to_vec();
    let current = (graphics.window_width, graphics.window_height);
    if !list.contains(&current) {
        list.push(current);
        list.sort_unstable();
    }
    list
}

/// The button-style choices; `None` follows the controller.
const BUTTON_STYLES: [Option<PadStyle>; 4] = [
    None,
    Some(PadStyle::Xbox),
    Some(PadStyle::PlayStation),
    Some(PadStyle::Nintendo),
];

/// The volume rows: option id, then the setting it edits.
const VOLUMES: [&str; 4] = [
    "music_volume",
    "ambient_volume",
    "effects_volume",
    "voice_volume",
];

fn volume_mut<'a>(game: &'a mut config::Game, id: &str) -> Option<&'a mut u8> {
    Some(match id {
        "music_volume" => &mut game.music_volume,
        "ambient_volume" => &mut game.ambient_volume,
        "effects_volume" => &mut game.effects_volume,
        "voice_volume" => &mut game.voice_volume,
        _ => return None,
    })
}

/// The frame-rate choices, with an unlisted configured rate appended.
fn frame_rates(current: u32) -> Vec<u32> {
    let mut list = video::FRAME_RATE_PRESETS.to_vec();
    if !list.contains(&current) {
        list.push(current);
        list.sort_unstable();
    }
    list
}

fn languages() -> Vec<Option<String>> {
    crate::launcher::language_choices()
}

/// A language's own name for itself, so a player can find theirs whatever
/// language the menu is in.
fn endonym(tag: &str) -> &str {
    match tag {
        "en-US" => "English",
        "ja-JP" => "日本語",
        "fr-FR" => "Français",
        "de-DE" => "Deutsch",
        "it-IT" => "Italiano",
        "es-ES" => "Español",
        "ru-RU" => "Русский",
        "fi-FI" => "Suomi",
        _ => tag,
    }
}

impl Settings {
    /// The strings the menus draw in: the configured language, else the
    /// system's.
    pub fn locale(&self) -> Locale {
        Locale::new(&crate::language::resolve_tag(self.language.as_deref()))
    }
}

fn on_off(text: &Locale, id: &str) -> OptionKind {
    OptionKind::Choice(vec![
        Choice::described(text.get("choice.off"), text.get(&format!("{id}.off"))),
        Choice::described(text.get("choice.on"), text.get(&format!("{id}.on"))),
    ])
}

/// A 0..=100 % volume slider.
fn volume_slider() -> OptionKind {
    OptionKind::Slider {
        min: 0,
        max: 100,
        step: 5,
        suffix: "%",
    }
}

/// One option: its strings come from `{id}.label` and `{id}.description`, its
/// place from the `category.`, `page.` and `group.` keys.
fn def(
    text: &Locale,
    id: &str,
    (category, page, group): (&str, &str, &str),
    kind: OptionKind,
    default: i32,
) -> OptionDef {
    OptionDef {
        id: id.to_string(),
        category: text.get(&format!("category.{category}")).to_string(),
        subcategory: text.get(&format!("page.{page}")).to_string(),
        group: text.get(&format!("group.{group}")).to_string(),
        label: text.get(&format!("{id}.label")).to_string(),
        description: text.get(&format!("{id}.description")).to_string(),
        kind,
        default,
        disabled: false,
    }
}

/// Every engine option, in screen order, in `text`'s language.
pub fn defs(settings: &Settings, text: &Locale) -> Vec<OptionDef> {
    let graphics_default = Graphics::default();
    let game_default = config::Game::default();
    let gpu = settings.graphics.renderer == RendererKind::Gpu;
    let aspect_list = aspects(settings.graphics.aspect);
    let rate_list = frame_rates(settings.graphics.frame_rate);
    let resolution_list = resolutions(&settings.graphics);
    let language_list = languages();
    const CHARACTER: (&str, &str, &str) = ("controls", "basic", "character_controls");
    const AIMING: (&str, &str, &str) = ("controls", "basic", "when_aiming");
    const CONTROLLER: (&str, &str, &str) = ("controls", "basic", "controller");
    const DISPLAY: (&str, &str, &str) = ("graphics", "display", "display");
    const RENDERER: (&str, &str, &str) = ("graphics", "rendering", "renderer");
    const TEXT: (&str, &str, &str) = ("interface", "general", "text");
    const VOLUME: (&str, &str, &str) = ("audio", "general", "volume");
    const SOUND: (&str, &str, &str) = ("audio", "general", "menus");
    const LANGUAGE: (&str, &str, &str) = ("language", "language", "text");

    let mut list = vec![
        def(
            text,
            "quick_turn",
            CHARACTER,
            on_off(text, "quick_turn"),
            i32::from(game_default.quick_turn),
        ),
        def(
            text,
            "tactical_reload",
            AIMING,
            on_off(text, "tactical_reload"),
            i32::from(game_default.tactical_reload),
        ),
        def(
            text,
            "button_style",
            CONTROLLER,
            OptionKind::Choice(vec![
                Choice::described(
                    text.get("button_style.auto"),
                    text.get("button_style.auto_note"),
                ),
                Choice::new("Xbox"),
                Choice::new("PlayStation"),
                Choice::new("Nintendo"),
            ]),
            BUTTON_STYLES
                .iter()
                .position(|&style| style == game_default.button_style)
                .unwrap_or(0) as i32,
        ),
        def(
            text,
            "fullscreen",
            DISPLAY,
            OptionKind::Choice(vec![
                Choice::new(text.get("fullscreen.window")),
                Choice::new(text.get("fullscreen.fullscreen")),
            ]),
            i32::from(graphics_default.fullscreen),
        ),
        def(
            text,
            "resolution",
            DISPLAY,
            OptionKind::Choice(
                resolution_list
                    .iter()
                    .map(|&(width, height)| Choice::new(format!("{width}x{height}")))
                    .collect(),
            ),
            resolution_list
                .iter()
                .position(|&size| {
                    size == (
                        graphics_default.window_width,
                        graphics_default.window_height,
                    )
                })
                .unwrap_or(0) as i32,
        ),
        def(
            text,
            "aspect",
            DISPLAY,
            OptionKind::Choice(
                aspect_list
                    .iter()
                    .map(|aspect| Choice::new(aspect.name()))
                    .collect(),
            ),
            aspect_list
                .iter()
                .position(|&a| a == graphics_default.aspect)
                .unwrap_or(0) as i32,
        ),
        def(
            text,
            "vsync",
            DISPLAY,
            on_off(text, "vsync"),
            i32::from(graphics_default.vsync),
        ),
        def(
            text,
            "frame_rate",
            DISPLAY,
            OptionKind::Choice(
                rate_list
                    .iter()
                    .map(|rate| Choice::new(text.format("frame_rate.value", rate)))
                    .collect(),
            ),
            rate_list
                .iter()
                .position(|&r| r == graphics_default.frame_rate)
                .unwrap_or(0) as i32,
        ),
        def(
            text,
            "renderer",
            RENDERER,
            OptionKind::Choice(vec![
                Choice::described(
                    text.get("renderer.software"),
                    text.get("renderer.software_note"),
                ),
                Choice::described(text.get("renderer.gpu"), text.get("renderer.gpu_note")),
            ]),
            i32::from(graphics_default.renderer == RendererKind::Gpu),
        ),
        def(
            text,
            "render_scale",
            RENDERER,
            OptionKind::Slider {
                min: 25,
                max: 400,
                step: 25,
                suffix: "%",
            },
            graphics_default.render_scale as i32,
        ),
        def(
            text,
            "msaa",
            RENDERER,
            OptionKind::Choice(
                video::MSAA_COUNTS
                    .iter()
                    .map(|&count| match count {
                        1 => Choice::new(text.get("choice.off")),
                        _ => Choice::new(text.format("msaa.value", count)),
                    })
                    .collect(),
            ),
            video::MSAA_COUNTS
                .iter()
                .position(|&m| m == graphics_default.msaa)
                .unwrap_or(0) as i32,
        ),
        def(
            text,
            "music_volume",
            VOLUME,
            volume_slider(),
            i32::from(game_default.music_volume),
        ),
        def(
            text,
            "ambient_volume",
            VOLUME,
            volume_slider(),
            i32::from(game_default.ambient_volume),
        ),
        def(
            text,
            "effects_volume",
            VOLUME,
            volume_slider(),
            i32::from(game_default.effects_volume),
        ),
        def(
            text,
            "voice_volume",
            VOLUME,
            volume_slider(),
            i32::from(game_default.voice_volume),
        ),
        def(
            text,
            "menu_sounds",
            SOUND,
            on_off(text, "menu_sounds"),
            i32::from(game_default.menu_sounds),
        ),
        def(
            text,
            "ui_font",
            TEXT,
            OptionKind::Choice(vec![
                Choice::described(text.get("ui_font.modern"), text.get("ui_font.modern_note")),
                Choice::described(text.get("ui_font.retro"), text.get("ui_font.retro_note")),
            ]),
            (game_default.ui_font == FontFamily::Retro) as i32,
        ),
        def(
            text,
            "language",
            LANGUAGE,
            OptionKind::Choice(
                language_list
                    .iter()
                    .map(|choice| match choice {
                        None => Choice::described(
                            text.get("language.system"),
                            text.format(
                                "language.system_note",
                                endonym(&crate::language::resolve_tag(None)),
                            ),
                        ),
                        Some(tag) => Choice::new(endonym(tag)),
                    })
                    .collect(),
            ),
            0,
        ),
    ];
    for option in &mut list {
        if matches!(option.id.as_str(), "render_scale" | "msaa") && !gpu {
            option.disabled = true;
        }
        // A fullscreen window takes the display's size.
        if option.id == "resolution" && settings.graphics.fullscreen {
            option.disabled = true;
        }
    }
    list
}

/// The current value of option `id`.
pub fn value(id: &str, settings: &Settings) -> i32 {
    let graphics = &settings.graphics;
    match id {
        "quick_turn" => i32::from(settings.game.quick_turn),
        "tactical_reload" => i32::from(settings.game.tactical_reload),
        "fullscreen" => i32::from(graphics.fullscreen),
        "resolution" => resolutions(graphics)
            .iter()
            .position(|&size| size == (graphics.window_width, graphics.window_height))
            .unwrap_or(0) as i32,
        "menu_sounds" => i32::from(settings.game.menu_sounds),
        "button_style" => BUTTON_STYLES
            .iter()
            .position(|&style| style == settings.game.button_style)
            .unwrap_or(0) as i32,
        "aspect" => aspects(graphics.aspect)
            .iter()
            .position(|&a| a == graphics.aspect)
            .unwrap_or(0) as i32,
        "vsync" => i32::from(graphics.vsync),
        "frame_rate" => frame_rates(graphics.frame_rate)
            .iter()
            .position(|&r| r == graphics.frame_rate)
            .unwrap_or(0) as i32,
        "renderer" => i32::from(graphics.renderer == RendererKind::Gpu),
        "render_scale" => graphics.render_scale as i32,
        "msaa" => video::MSAA_COUNTS
            .iter()
            .position(|&m| m == graphics.msaa)
            .unwrap_or(0) as i32,
        "ui_font" => i32::from(settings.game.ui_font == FontFamily::Retro),
        id if VOLUMES.contains(&id) => {
            let mut game = settings.game;
            volume_mut(&mut game, id).map_or(0, |volume| i32::from(*volume))
        }
        "language" => languages()
            .iter()
            .position(|choice| choice.as_deref() == settings.language.as_deref())
            .unwrap_or(0) as i32,
        _ => 0,
    }
}

/// Write `value` into option `id`'s setting.
pub fn apply(id: &str, value: i32, settings: &mut Settings) {
    let on = value != 0;
    let index = value.max(0) as usize;
    let graphics = &mut settings.graphics;
    match id {
        "quick_turn" => settings.game.quick_turn = on,
        "tactical_reload" => settings.game.tactical_reload = on,
        "fullscreen" => graphics.fullscreen = on,
        "resolution" => {
            if let Some(&(width, height)) = resolutions(graphics).get(index) {
                graphics.window_width = width;
                graphics.window_height = height;
            }
        }
        "menu_sounds" => settings.game.menu_sounds = on,
        "button_style" => {
            if let Some(&style) = BUTTON_STYLES.get(index) {
                settings.game.button_style = style;
            }
        }
        id if VOLUMES.contains(&id) => {
            if let Some(volume) = volume_mut(&mut settings.game, id) {
                *volume = value.clamp(0, 100) as u8;
            }
        }
        "aspect" => {
            if let Some(&aspect) = aspects(graphics.aspect).get(index) {
                graphics.aspect = aspect;
            }
        }
        "vsync" => graphics.vsync = on,
        "frame_rate" => {
            if let Some(&rate) = frame_rates(graphics.frame_rate).get(index) {
                graphics.frame_rate = rate;
            }
        }
        "renderer" => {
            graphics.renderer = if on {
                RendererKind::Gpu
            } else {
                RendererKind::Software
            }
        }
        "render_scale" => {
            graphics.render_scale = (value.max(0) as u32).clamp(
                *video::RENDER_SCALE_RANGE.start(),
                *video::RENDER_SCALE_RANGE.end(),
            )
        }
        "msaa" => {
            if let Some(&msaa) = video::MSAA_COUNTS.get(index) {
                graphics.msaa = msaa;
            }
        }
        "ui_font" => {
            settings.game.ui_font = if on {
                FontFamily::Retro
            } else {
                FontFamily::Modern
            }
        }
        "language" => {
            if let Some(choice) = languages().get(index) {
                settings.language = choice.clone();
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings {
            graphics: Graphics::default(),
            game: config::Game::default(),
            language: None,
        }
    }

    #[test]
    fn every_option_round_trips() {
        let base = settings();
        for option in defs(&base, &Locale::default()) {
            for position in 0..option.positions() as i32 {
                let wanted = match option.kind {
                    OptionKind::Choice(_) => position,
                    OptionKind::Slider { min, step, .. } => min + position * step,
                };
                let mut changed = base.clone();
                apply(&option.id, wanted, &mut changed);
                assert_eq!(value(&option.id, &changed), wanted, "{}", option.id);
            }
        }
    }

    #[test]
    fn defaults_match_the_default_settings() {
        let base = settings();
        for option in defs(&base, &Locale::default()) {
            assert_eq!(option.default, value(&option.id, &base), "{}", option.id);
        }
    }
}
