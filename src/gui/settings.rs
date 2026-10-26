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
use crate::config::{self, ControlType, Named, SubtitleSize, TextColor, TextSize, Vibration};
use crate::video::{self, Aspect, Graphics, RendererKind};

/// The settings the options screen edits.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    pub graphics: Graphics,
    pub game: config::Game,
    /// The configured language tag; `None` follows the OS locale.
    pub language: Option<String>,
    /// The connected displays' names in SDL's order, for the monitor choice
    /// (`graphics.display` is 1-based into it).
    pub displays: Vec<String>,
    /// The action map the key configuration screen edits.
    pub bindings: crate::controls::Bindings,
    /// The machine, for the PC Specs row.
    pub system: SystemInfo,
}

/// What the PC Specs row shows of the machine.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SystemInfo {
    /// The processor's brand string, where the CPU reports one.
    pub cpu: Option<String>,
    pub cpu_cores: u32,
    pub ram_mb: u32,
    pub gpu: crate::gpu::GpuInfo,
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

/// The volume rows' option ids, in screen order.
const VOLUMES: [&str; 6] = [
    "master_volume",
    "effects_volume",
    "music_volume",
    "ambient_volume",
    "voice_volume",
    "menu_volume",
];

/// The 0..=100 percent setting a percent row edits.
fn percent_mut<'a>(game: &'a mut config::Game, id: &str) -> Option<&'a mut u8> {
    Some(match id {
        "master_volume" => &mut game.master_volume,
        "music_volume" => &mut game.music_volume,
        "ambient_volume" => &mut game.ambient_volume,
        "effects_volume" => &mut game.effects_volume,
        "voice_volume" => &mut game.voice_volume,
        "menu_volume" => &mut game.menu_volume,
        "subtitle_background" => &mut game.subtitle_background,
        _ => return None,
    })
}

/// The on/off settings, by option id.
fn switch_mut<'a>(game: &'a mut config::Game, id: &str) -> Option<&'a mut bool> {
    Some(match id {
        "quick_turn" => &mut game.quick_turn,
        "analog_movement" => &mut game.analog_movement,
        "auto_aim" => &mut game.auto_aim,
        "tactical_reload" => &mut game.tactical_reload,
        "blood" => &mut game.blood,
        "subtitles" => &mut game.subtitles,
        "speaker_names" => &mut game.speaker_names,
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

/// A PC Specs preset: the renderer settings it picks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Preset {
    renderer: RendererKind,
    render_scale: u32,
    msaa: u32,
}

/// The PC Specs presets, lowest first; any other combination is Custom, the
/// choice after them.
const PRESETS: [(&str, Preset); 4] = [
    (
        "low",
        Preset {
            renderer: RendererKind::Software,
            render_scale: 100,
            msaa: 1,
        },
    ),
    (
        "medium",
        Preset {
            renderer: RendererKind::Gpu,
            render_scale: 100,
            msaa: 1,
        },
    ),
    (
        "high",
        Preset {
            renderer: RendererKind::Gpu,
            render_scale: 150,
            msaa: 2,
        },
    ),
    (
        "ultra",
        Preset {
            renderer: RendererKind::Gpu,
            render_scale: 200,
            msaa: 4,
        },
    ),
];

/// The preset `graphics` matches, else Custom (`PRESETS.len()`). The
/// software renderer ignores the GPU settings, so any software setup is Low.
fn preset(graphics: &Graphics) -> usize {
    PRESETS
        .iter()
        .position(|(_, preset)| {
            preset.renderer == graphics.renderer
                && (graphics.renderer == RendererKind::Software
                    || (preset.render_scale, preset.msaa) == (graphics.render_scale, graphics.msaa))
        })
        .unwrap_or(PRESETS.len())
}

impl Settings {
    /// The strings the menus draw in: the configured language, else the
    /// system's.
    pub fn locale(&self) -> Locale {
        Locale::new(&crate::language::resolve_tag(self.language.as_deref()))
    }

    /// The monitor choices: the connected displays, with an unlisted
    /// configured one appended.
    fn monitors(&self) -> Vec<String> {
        let mut list = self.displays.clone();
        while list.len() < self.graphics.display as usize {
            list.push(String::new());
        }
        list
    }
}

fn on_off(text: &Locale, id: &str) -> OptionKind {
    OptionKind::Choice(vec![
        Choice::described(text.get("choice.off"), text.get(&format!("{id}.off"))),
        Choice::described(text.get("choice.on"), text.get(&format!("{id}.on"))),
    ])
}

/// The choices of a [`Named`] setting, labelled by `{id}.{name}`.
fn named<T: Named>(text: &Locale, id: &str) -> OptionKind {
    OptionKind::Choice(
        T::NAMES
            .iter()
            .map(|(_, name)| Choice::new(text.get(&format!("{id}.{name}"))))
            .collect(),
    )
}

/// The colour choices, labelled by the shared `color.` keys.
fn colors(text: &Locale) -> OptionKind {
    OptionKind::Choice(
        TextColor::NAMES
            .iter()
            .map(|(_, name)| Choice::new(text.get(&format!("color.{name}"))))
            .collect(),
    )
}

/// A 0..=100 % slider.
fn percent_slider() -> OptionKind {
    OptionKind::Slider {
        min: 0,
        max: 100,
        step: 5,
        suffix: "%",
    }
}

/// One option: its strings come from `{id}.label` and `{id}.description`, its
/// place from the `category.`, `page.` and `group.` keys. An empty group draws
/// no header.
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
        group: if group.is_empty() {
            String::new()
        } else {
            text.get(&format!("group.{group}")).to_string()
        },
        label: text.get(&format!("{id}.label")).to_string(),
        description: text.get(&format!("{id}.description")).to_string(),
        kind,
        default,
        disabled: false,
    }
}

/// Every engine option, in screen order, in `text`'s language.
pub fn defs(settings: &Settings, text: &Locale) -> Vec<OptionDef> {
    let defaults = Settings {
        displays: settings.displays.clone(),
        ..Settings::default()
    };
    let game = &settings.game;
    let gpu = settings.graphics.renderer == RendererKind::Gpu;
    let aspect_list = aspects(settings.graphics.aspect);
    let rate_list = frame_rates(settings.graphics.frame_rate);
    let resolution_list = resolutions(&settings.graphics);
    let language_list = languages();
    const CHARACTER: (&str, &str, &str) = ("controls", "basic", "character_controls");
    const AIMING: (&str, &str, &str) = ("controls", "basic", "when_aiming");
    const BUTTONS: (&str, &str, &str) = ("controls", "controller", "button_settings");
    const FUNCTIONS: (&str, &str, &str) = ("controls", "controller", "function_settings");
    const KEYBOARD: (&str, &str, &str) = ("controls", "keyboard_mouse", "keyboard_mouse_settings");
    const HUD: (&str, &str, &str) = ("hud", "hud", "");
    const DISPLAY: (&str, &str, &str) = ("graphics", "display", "");
    const IMAGE: (&str, &str, &str) = ("graphics", "image", "");
    const QUALITY: (&str, &str, &str) = ("graphics", "image", "quality_settings");
    const VOLUME: (&str, &str, &str) = ("audio", "general", "volume");
    const LANGUAGE: (&str, &str, &str) = ("language", "general", "language");
    const SUBTITLES: (&str, &str, &str) = ("language", "general", "subtitles");

    // A default is read back through `value`, so each row's mapping lives in
    // one place; a list that grows with the current setting is searched
    // instead.
    let row = |id: &str, place, kind| {
        let graphics = &defaults.graphics;
        let default = match id {
            "resolution" => resolution_list
                .iter()
                .position(|&size| size == (graphics.window_width, graphics.window_height))
                .unwrap_or(0) as i32,
            "aspect" => aspect_list
                .iter()
                .position(|&a| a == graphics.aspect)
                .unwrap_or(0) as i32,
            "frame_rate" => rate_list
                .iter()
                .position(|&r| r == graphics.frame_rate)
                .unwrap_or(0) as i32,
            _ => value(id, &defaults),
        };
        def(text, id, place, kind, default)
    };
    let mut list = vec![
        row("quick_turn", CHARACTER, on_off(text, "quick_turn")),
        row(
            "analog_movement",
            CHARACTER,
            on_off(text, "analog_movement"),
        ),
        row("auto_aim", AIMING, on_off(text, "auto_aim")),
        row("tactical_reload", AIMING, on_off(text, "tactical_reload")),
        row(
            "control_type",
            BUTTONS,
            named::<ControlType>(text, "control_type"),
        ),
        row(
            "button_style",
            BUTTONS,
            OptionKind::Choice(vec![
                Choice::described(
                    text.get("button_style.auto"),
                    text.get("button_style.auto_note"),
                ),
                Choice::new("Xbox"),
                Choice::new("PlayStation"),
                Choice::new("Nintendo"),
            ]),
        ),
        row(
            "vibration",
            FUNCTIONS,
            named::<Vibration>(text, "vibration"),
        ),
        // Opens the key configuration screen; it has no value.
        row("key_config", KEYBOARD, OptionKind::Link),
        row(
            "ui_font",
            HUD,
            OptionKind::Choice(vec![
                Choice::described(text.get("ui_font.modern"), text.get("ui_font.modern_note")),
                Choice::described(text.get("ui_font.retro"), text.get("ui_font.retro_note")),
            ]),
        ),
        row(
            "menu_text_size",
            HUD,
            named::<TextSize>(text, "menu_text_size"),
        ),
        row(
            "fullscreen",
            DISPLAY,
            OptionKind::Choice(vec![
                Choice::new(text.get("fullscreen.window")),
                Choice::new(text.get("fullscreen.fullscreen")),
            ]),
        ),
        row(
            "monitor",
            DISPLAY,
            OptionKind::Choice(
                settings
                    .monitors()
                    .iter()
                    .enumerate()
                    .map(|(index, name)| {
                        Choice::new(match name.as_str() {
                            "" => format!("{}", index + 1),
                            name => format!("{}: {name}", index + 1),
                        })
                    })
                    .collect(),
            ),
        ),
        row(
            "resolution",
            DISPLAY,
            OptionKind::Choice(
                resolution_list
                    .iter()
                    .map(|&(width, height)| Choice::new(format!("{width}x{height}")))
                    .collect(),
            ),
        ),
        row(
            "aspect",
            DISPLAY,
            OptionKind::Choice(
                aspect_list
                    .iter()
                    .map(|aspect| Choice::new(aspect.name()))
                    .collect(),
            ),
        ),
        row(
            "frame_rate",
            DISPLAY,
            OptionKind::Choice(
                rate_list
                    .iter()
                    .map(|rate| Choice::new(text.format("frame_rate.value", rate)))
                    .collect(),
            ),
        ),
        row("vsync", DISPLAY, on_off(text, "vsync")),
        row(
            "hdr",
            DISPLAY,
            OptionKind::Choice(vec![
                Choice::described(text.get("hdr.auto"), text.get("hdr.auto_note")),
                Choice::described(text.get("choice.off"), text.get("hdr.off")),
            ]),
        ),
        // Shows the machine's specifications; it has no value.
        row("pc_specs", IMAGE, OptionKind::Info),
        row("blood", IMAGE, on_off(text, "blood")),
        row(
            "graphics_quality",
            QUALITY,
            OptionKind::Choice(
                PRESETS
                    .iter()
                    .map(|(name, _)| *name)
                    .chain(["custom"])
                    .map(|name| {
                        Choice::described(
                            text.get(&format!("graphics_quality.{name}")),
                            text.get(&format!("graphics_quality.{name}_note")),
                        )
                    })
                    .collect(),
            ),
        ),
        row(
            "renderer",
            QUALITY,
            OptionKind::Choice(vec![
                Choice::described(
                    text.get("renderer.software"),
                    text.get("renderer.software_note"),
                ),
                Choice::described(text.get("renderer.gpu"), text.get("renderer.gpu_note")),
            ]),
        ),
        row(
            "render_scale",
            QUALITY,
            OptionKind::Slider {
                min: 25,
                max: 400,
                step: 25,
                suffix: "%",
            },
        ),
        row(
            "msaa",
            QUALITY,
            OptionKind::Choice(
                video::MSAA_COUNTS
                    .iter()
                    .map(|&count| match count {
                        1 => Choice::new(text.get("choice.off")),
                        _ => Choice::new(text.format("msaa.value", count)),
                    })
                    .collect(),
            ),
        ),
    ];
    for id in VOLUMES {
        list.push(row(id, VOLUME, percent_slider()));
    }
    list.extend([
        row(
            "voice_language",
            LANGUAGE,
            OptionKind::Choice(vec![Choice::new(endonym("en-US"))]),
        ),
        row(
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
        ),
        row("subtitles", SUBTITLES, on_off(text, "subtitles")),
        row(
            "subtitle_size",
            SUBTITLES,
            named::<SubtitleSize>(text, "subtitle_size"),
        ),
        row("subtitle_background", SUBTITLES, percent_slider()),
        row("subtitle_color", SUBTITLES, colors(text)),
        row("speaker_names", SUBTITLES, on_off(text, "speaker_names")),
        row("speaker_name_color", SUBTITLES, colors(text)),
    ]);
    for option in &mut list {
        option.disabled = match option.id.as_str() {
            "render_scale" | "msaa" => !gpu,
            // A fullscreen window takes the display's size.
            "resolution" => settings.graphics.fullscreen,
            "subtitle_size" | "subtitle_background" | "subtitle_color" | "speaker_names" => {
                !game.subtitles
            }
            "speaker_name_color" => !game.subtitles || !game.speaker_names,
            _ => false,
        };
    }
    list
}

/// The current value of option `id`.
pub fn value(id: &str, settings: &Settings) -> i32 {
    let graphics = &settings.graphics;
    let game = &settings.game;
    let index = |position: usize| position as i32;
    match id {
        "control_type" => index(game.control_type.index()),
        "vibration" => index(game.vibration.index()),
        "menu_text_size" => index(game.menu_text_size.index()),
        "subtitle_size" => index(game.subtitle_size.index()),
        "subtitle_color" => index(game.subtitle_color.index()),
        "speaker_name_color" => index(game.speaker_name_color.index()),
        "fullscreen" => i32::from(graphics.fullscreen),
        "monitor" => graphics.display.saturating_sub(1) as i32,
        "resolution" => index(
            resolutions(graphics)
                .iter()
                .position(|&size| size == (graphics.window_width, graphics.window_height))
                .unwrap_or(0),
        ),
        "button_style" => index(
            BUTTON_STYLES
                .iter()
                .position(|&style| style == game.button_style)
                .unwrap_or(0),
        ),
        "aspect" => index(
            aspects(graphics.aspect)
                .iter()
                .position(|&a| a == graphics.aspect)
                .unwrap_or(0),
        ),
        "vsync" => i32::from(graphics.vsync),
        "hdr" => i32::from(!graphics.hdr),
        "frame_rate" => index(
            frame_rates(graphics.frame_rate)
                .iter()
                .position(|&r| r == graphics.frame_rate)
                .unwrap_or(0),
        ),
        "graphics_quality" => index(preset(graphics)),
        "renderer" => i32::from(graphics.renderer == RendererKind::Gpu),
        "render_scale" => graphics.render_scale as i32,
        "msaa" => index(
            video::MSAA_COUNTS
                .iter()
                .position(|&m| m == graphics.msaa)
                .unwrap_or(0),
        ),
        "ui_font" => i32::from(game.ui_font == FontFamily::Retro),
        "voice_language" => 0,
        "language" => index(
            languages()
                .iter()
                .position(|choice| choice.as_deref() == settings.language.as_deref())
                .unwrap_or(0),
        ),
        id => {
            let mut game = *game;
            if let Some(switch) = switch_mut(&mut game, id) {
                i32::from(*switch)
            } else {
                percent_mut(&mut game, id).map_or(0, |percent| i32::from(*percent))
            }
        }
    }
}

/// Write `value` into option `id`'s setting.
pub fn apply(id: &str, value: i32, settings: &mut Settings) {
    let on = value != 0;
    let index = value.max(0) as usize;
    let monitors = settings.monitors().len();
    let graphics = &mut settings.graphics;
    let game = &mut settings.game;
    fn set<T: Named>(slot: &mut T, index: usize) {
        if let Some(value) = T::at(index) {
            *slot = value;
        }
    }
    match id {
        "control_type" => set(&mut game.control_type, index),
        "vibration" => set(&mut game.vibration, index),
        "menu_text_size" => set(&mut game.menu_text_size, index),
        "subtitle_size" => set(&mut game.subtitle_size, index),
        "subtitle_color" => set(&mut game.subtitle_color, index),
        "speaker_name_color" => set(&mut game.speaker_name_color, index),
        "fullscreen" => graphics.fullscreen = on,
        "monitor" => {
            if index < monitors {
                graphics.display = index as u32 + 1;
            }
        }
        "resolution" => {
            if let Some(&(width, height)) = resolutions(graphics).get(index) {
                graphics.window_width = width;
                graphics.window_height = height;
            }
        }
        "button_style" => {
            if let Some(&style) = BUTTON_STYLES.get(index) {
                game.button_style = style;
            }
        }
        "aspect" => {
            if let Some(&aspect) = aspects(graphics.aspect).get(index) {
                graphics.aspect = aspect;
            }
        }
        "vsync" => graphics.vsync = on,
        "hdr" => graphics.hdr = !on,
        "frame_rate" => {
            if let Some(&rate) = frame_rates(graphics.frame_rate).get(index) {
                graphics.frame_rate = rate;
            }
        }
        // Custom keeps whatever the quality rows say.
        "graphics_quality" => {
            if let Some((_, preset)) = PRESETS.get(index) {
                graphics.renderer = preset.renderer;
                if preset.renderer == RendererKind::Gpu {
                    graphics.render_scale = preset.render_scale;
                    graphics.msaa = preset.msaa;
                }
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
            game.ui_font = if on {
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
        id => {
            if let Some(switch) = switch_mut(game, id) {
                *switch = on;
            } else if let Some(percent) = percent_mut(game, id) {
                *percent = value.clamp(0, 100) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings {
            displays: vec!["Left".into(), "Right".into()],
            ..Settings::default()
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
                    OptionKind::Link | OptionKind::Info => 0,
                };
                // Custom is what the quality rows add up to, not a choice
                // of its own.
                if option.id == "graphics_quality" && wanted == PRESETS.len() as i32 {
                    continue;
                }
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

    #[test]
    fn every_string_is_translated_to_english() {
        // A missing string draws as its key.
        let is_key = |shown: &str| {
            shown.contains('.')
                && shown
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '_' || c == '.')
        };
        for option in defs(&settings(), &Locale::default()) {
            let choices = match &option.kind {
                OptionKind::Choice(choices) => choices.clone(),
                OptionKind::Slider { .. } | OptionKind::Link | OptionKind::Info => Vec::new(),
            };
            for shown in [
                &option.category,
                &option.subcategory,
                &option.group,
                &option.label,
                &option.description,
            ]
            .into_iter()
            .chain(choices.iter().map(|c| &c.label))
            .chain(choices.iter().filter_map(|c| c.description.as_ref()))
            {
                assert!(!is_key(shown), "{}: {shown}", option.id);
            }
        }
    }

    #[test]
    fn the_graphics_quality_follows_the_quality_rows() {
        let mut base = settings();
        assert_eq!(
            value("graphics_quality", &base),
            1,
            "the defaults are Medium"
        );
        apply("msaa", 3, &mut base);
        assert_eq!(
            value("graphics_quality", &base),
            PRESETS.len() as i32,
            "Custom"
        );
        apply("graphics_quality", 3, &mut base);
        assert_eq!(
            (base.graphics.render_scale, base.graphics.msaa),
            (200, 4),
            "Ultra"
        );
        // Low only swaps the renderer; the GPU rows keep their values.
        apply("graphics_quality", 0, &mut base);
        assert_eq!(base.graphics.renderer, RendererKind::Software);
        assert_eq!(base.graphics.render_scale, 200);
        // Custom changes nothing.
        let before = base.clone();
        apply("graphics_quality", PRESETS.len() as i32, &mut base);
        assert_eq!(base, before);
    }
}
