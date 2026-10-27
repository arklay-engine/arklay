//! What the interactive loop shows over the game: the F6 debug HUD's
//! readouts and the corner notices (the game speed), drawn by
//! [`gui::Overlay`].

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use super::{App, Backend, Display, InputState, LoopClock, Mode};
use crate::controls::{ACTIONS, DEBUG_MENU_BIT, RETURN_TITLE_BIT, SYSTEM_MENU_BIT};
use crate::game::GameState;
use crate::gui::overlay::{Report, Section, Tone};
use crate::gui::{self, DebugMode};
use crate::process_stats::{self, CpuMeter};
use crate::state::RoomId;

/// The window the frame rate is counted over.
const FPS_WINDOW: Duration = Duration::from_secs(1);
/// The shortest span a CPU reading is taken over, in seconds.
const CPU_SPAN: f64 = 0.5;

/// The boss entity ids, in the order the HUD prefers them when more than
/// one is in the room: the fights the story needs won (or, for the first
/// Yawn, survived) first, then Neptune and the monster plant.
const BOSSES: [(u8, &str); 8] = [
    (0x10, "Tyrant (heliport)"),
    (0x0C, "Tyrant"),
    (0x08, "Plant 42"),
    (0x12, "Yawn (rematch)"),
    (0x0D, "Yawn"),
    (0x04, "Black Tiger"),
    (0x0B, "Neptune"),
    (0x0F, "Monster plant"),
];

/// A boss by room, entity slot and id.
type BossKey = (RoomId, usize, u8);

/// The overlay, with the samples its readouts need between refreshes.
pub(super) struct DebugHud {
    /// `None` when the GUI faces failed to load.
    overlay: Option<gui::Overlay>,
    /// When each recent frame was presented.
    frames: VecDeque<Instant>,
    cpu: CpuMeter,
    /// The most health each boss has been seen with, which the HUD reports
    /// as its maximum. Tracked every frame, shown or not, so turning the HUD
    /// on mid-fight still knows the boss's health on entry.
    boss_max: HashMap<BossKey, i16>,
}

impl DebugHud {
    pub(super) fn new(family: gui::FontFamily) -> Self {
        let overlay = gui::Overlay::new(family)
            .inspect_err(|err| eprintln!("warning: debug HUD unavailable: {err:#}"))
            .ok();
        Self {
            overlay,
            frames: VecDeque::new(),
            cpu: CpuMeter::default(),
            boss_max: HashMap::new(),
        }
    }

    /// F6: step the overlay's mode and say which it is now.
    pub(super) fn cycle(&mut self) {
        if let Some(overlay) = self.overlay.as_mut() {
            let label = match overlay.cycle() {
                DebugMode::Off => "off",
                DebugMode::Full => "full",
                DebugMode::Player => "player",
                DebugMode::Fps => "FPS",
            };
            overlay.notify("debug", format!("Debug HUD: {label}"));
        }
    }

    /// Show `text` in the bottom-left corner for a few seconds.
    pub(super) fn notify(&mut self, key: &'static str, text: impl Into<String>) {
        if let Some(overlay) = self.overlay.as_mut() {
            overlay.notify(key, text);
        }
    }

    /// Count a presented frame towards the frame rate.
    pub(super) fn frame_presented(&mut self) {
        let now = Instant::now();
        self.frames.push_back(now);
        while self
            .frames
            .front()
            .is_some_and(|&at| now.duration_since(at) > FPS_WINDOW)
        {
            self.frames.pop_front();
        }
    }

    /// Refresh the readouts when due and paint the overlay over the in-game
    /// menu's layer, if any, at the window's pixel size.
    pub(super) fn draw(
        &mut self,
        app: &App,
        display: &Display,
        clock: &LoopClock,
        input: &InputState,
    ) {
        let Self {
            overlay,
            frames,
            cpu,
            boss_max,
        } = self;
        let Some(overlay) = overlay.as_mut() else {
            return;
        };
        if let Mode::Play(session) = &app.mode {
            track_bosses(&session.game, boss_max);
        }
        if let Some(menu) = app.system_menu.as_deref() {
            overlay.set_family(menu.settings().game.ui_font);
        }
        let now = Instant::now();
        if overlay.report_due(now) {
            let frames = FrameStats::new(frames);
            let report = match overlay.mode() {
                DebugMode::Off => Report::default(),
                DebugMode::Fps => fps_report(&frames, display),
                DebugMode::Player => Report {
                    left: game_sections(app, input, boss_max, false),
                    right: Vec::new(),
                },
                DebugMode::Full => Report {
                    left: game_sections(app, input, boss_max, true),
                    right: system_sections(app, display, clock, &frames, cpu),
                },
            };
            overlay.set_report(report, now);
        }
        let (width, height) = display.drawable_size();
        overlay.draw(width, height, app.native_layer(), display.gpu_active());
    }

    /// The layer to present over the frame: the overlay (already composed
    /// over the in-game menu) while it shows anything.
    pub(super) fn layer(&self) -> Option<&gui::Canvas> {
        self.overlay
            .as_ref()
            .filter(|overlay| overlay.visible())
            .map(gui::Overlay::canvas)
    }
}

/// The frame rate and pacing over the last [`FPS_WINDOW`].
struct FrameStats {
    fps: usize,
    /// The longest gap between two presented frames, in milliseconds.
    worst_ms: f64,
}

impl FrameStats {
    fn new(frames: &VecDeque<Instant>) -> Self {
        let worst_ms = frames
            .iter()
            .zip(frames.iter().skip(1))
            .map(|(a, b)| b.duration_since(*a).as_secs_f64() * 1000.0)
            .fold(0.0, f64::max);
        Self {
            fps: frames.len(),
            worst_ms,
        }
    }

    /// How close the rate is to `target`.
    fn tone(&self, target: u32) -> Tone {
        let fps = self.fps as f64;
        let target = f64::from(target.max(1));
        if fps >= target * 0.95 {
            Tone::Good
        } else if fps >= target * 0.75 {
            Tone::Warn
        } else {
            Tone::Bad
        }
    }
}

fn fps_report(frames: &FrameStats, display: &Display) -> Report {
    let mut section = Section::new("");
    section.toned(
        "",
        format!("{} FPS", frames.fps),
        frames.tone(display.frame_rate()),
    );
    Report {
        left: Vec::new(),
        right: vec![section],
    }
}

/// The right column: the engine, the window, the process and the pack.
fn system_sections(
    app: &App,
    display: &Display,
    clock: &LoopClock,
    frames: &FrameStats,
    cpu: &mut CpuMeter,
) -> Vec<Section> {
    let mut engine = Section::new("ARKLAY");
    engine.line("Version", crate::build_info::label());
    let target = display.frame_rate();
    let vsync = display.graphics().is_none_or(|graphics| graphics.vsync);
    engine.toned(
        "FPS",
        format!(
            "{} / {target} \u{00B7} vsync {}",
            frames.fps,
            if vsync { "on" } else { "off" }
        ),
        frames.tone(target),
    );
    engine.line("Worst frame", format!("{:.0} ms", frames.worst_ms));
    let speed = clock.speed.multiplier();
    engine.toned(
        "Game speed",
        format!("{speed:.1}x"),
        if speed == 1.0 {
            Tone::Normal
        } else {
            Tone::Warn
        },
    );
    let (width, height) = display.drawable_size();
    let fullscreen = display
        .graphics()
        .is_some_and(|graphics| graphics.fullscreen);
    engine.line(
        "Resolution",
        format!(
            "{width} x {height} {}",
            if fullscreen { "fullscreen" } else { "windowed" }
        ),
    );
    let renderer = match &display.backend {
        Backend::Gpu(_) => match display.graphics() {
            Some(graphics) => format!(
                "GPU \u{00B7} scale {}% \u{00B7} MSAA {}x",
                graphics.render_scale, graphics.msaa
            ),
            None => "GPU".to_string(),
        },
        Backend::Software { .. } | Backend::None => "Software".to_string(),
    };
    engine.line("Renderer", renderer);
    if let Some(graphics) = display.graphics() {
        engine.line(
            "Aspect",
            format!(
                "{} \u{00B7} frame 320 x {}",
                graphics.aspect.name(),
                graphics.aspect.height()
            ),
        );
    }

    let mut system = Section::new("SYSTEM");
    if let Some(memory) = process_stats::memory() {
        let mut value = format!(
            "{} \u{00B7} peak {}",
            megabytes(memory.resident),
            megabytes(memory.peak)
        );
        if let Some(total) = memory.total {
            value += &format!(" \u{00B7} of {:.1} GB", total as f64 / (1u64 << 30) as f64);
        }
        system.line("Memory", value);
    }
    let threads = process_stats::threads();
    let share = cpu
        .sample(CPU_SPAN)
        .map_or_else(|| "...".to_string(), |share| format!("{share:.0}%"));
    system.line("CPU", format!("{share} \u{00B7} {threads} threads"));

    let mut pack = Section::new("GAME PACK");
    let base = app.pack.manifest();
    pack.line(
        "Pack",
        base.map_or_else(|| "unnamed".to_string(), manifest_label),
    );
    pack.line(
        "Location",
        app.pack
            .source()
            .map_or_else(|| "memory".to_string(), |path| path.display().to_string()),
    );
    pack.line("Language", app.pack.language().unwrap_or("default"));
    let mut mods = app.pack.overrides().peekable();
    if mods.peek().is_none() {
        pack.toned("Mods", "none", Tone::Dim);
    }
    for (index, layer) in mods.enumerate() {
        let name = match layer.manifest() {
            Some(manifest) => manifest_label(manifest),
            None => layer.source().map_or_else(
                || "unnamed".to_string(),
                |path| {
                    path.file_name().map_or_else(
                        || path.display().to_string(),
                        |name| name.to_string_lossy().into_owned(),
                    )
                },
            ),
        };
        pack.line(if index == 0 { "Mods" } else { "" }, name);
    }
    // Lines without a label share the label column of the first mod.
    for line in &mut pack.lines {
        if line.label.is_empty() {
            line.label = " ".to_string();
        }
    }
    vec![engine, system, pack]
}

fn manifest_label(manifest: &crate::manifest::Manifest) -> String {
    let name = manifest.name.as_deref().unwrap_or(&manifest.id);
    match &manifest.version {
        Some(version) => format!("{name} {version}"),
        None => name.to_string(),
    }
}

fn megabytes(bytes: u64) -> String {
    format!("{} MB", bytes >> 20)
}

/// The left column: the screen and room (with `full`), the player and the
/// boss.
fn game_sections(
    app: &App,
    input: &InputState,
    boss_max: &HashMap<BossKey, i16>,
    full: bool,
) -> Vec<Section> {
    let mut sections = Vec::new();
    let session = match &app.mode {
        Mode::Play(session) => Some(session),
        _ => None,
    };
    if full {
        let mut game = Section::new("GAME");
        game.line("Screen", mode_name(&app.mode));
        if let Some(session) = session {
            let state = &session.game;
            let id = state.id;
            game.line(
                "Room",
                format!(
                    "{}{:02X}{} \u{00B7} stage {} room {:02X} \u{00B7} cut {}",
                    id.stage, id.room, id.player_flag, id.stage, id.room, state.camera.current_cut
                ),
            );
            game.line("Enemies", enemy_count(state).to_string());
        }
        sections.push(game);
    }
    let Some(session) = session else {
        if !full {
            let mut player = Section::new("PLAYER");
            player.toned("", "not in game", Tone::Dim);
            sections.push(player);
        }
        return sections;
    };
    let state = &session.game;
    sections.push(if full {
        player_section(state, &session.text, input)
    } else {
        mini_player_section(state)
    });
    if let Some(section) = boss_section(state, boss_max, full) {
        sections.push(section);
    }
    sections
}

fn mode_name(mode: &Mode) -> &'static str {
    match mode {
        Mode::Boot => "Boot",
        Mode::Title(_) => "Title",
        Mode::Select(_) => "Character select",
        Mode::Load(_) => "Load",
        Mode::Play(_) => "Play",
        Mode::Movie(_) => "Film",
        Mode::Ending(_) => "Ending",
        Mode::Epilogue(_) => "Epilogue",
        Mode::Save(_) => "Save",
    }
}

/// Whether the entity in `slot` is a live enemy.
fn is_live_enemy(state: &GameState, slot: usize) -> bool {
    let entity = &state.entities[slot];
    slot != 0 && entity.active() && entity.id < crate::enemy::data::FIRST_ID && entity.health > 0
}

fn enemy_count(state: &GameState) -> usize {
    (0..state.entities.len())
        .filter(|&slot| is_live_enemy(state, slot))
        .count()
}

/// The health state the status screen's EKG shows: `(health - 1) /
/// (max >> 2)` clamped to `0..=3`; 0 is Danger, 1 Caution, 2 and 3 Fine.
fn health_state(health: i16, max: i16) -> i16 {
    if health <= 0 || max <= 0 {
        return 0;
    }
    let step = max >> 2;
    if step <= 0 {
        return 3;
    }
    ((health - 1) / step).clamp(0, 3)
}

/// The player's condition as the status screen names it, and its colour;
/// poison overrides the colour.
fn condition(state: &GameState) -> (String, Tone) {
    let (name, tone) = match health_state(state.entities[0].health, state.max_health) {
        0 => ("Danger", Tone::Bad),
        1 => ("Caution", Tone::Warn),
        _ => ("Fine", Tone::Good),
    };
    if state.health_status & 0x22 != 0 {
        (format!("{name} \u{00B7} poisoned"), Tone::Bad)
    } else {
        (name.to_string(), tone)
    }
}

/// The weapon the player has equipped and carries, if any. This reads the
/// equipped item rather than `equipped_weapon_id`, which keeps a weapon
/// even when nothing is equipped.
fn equipped_weapon(state: &GameState) -> Option<u8> {
    state
        .equipped
        .filter(|&item| crate::weapons::is_weapon(item))
        .filter(|&item| state.inventory.iter().any(|stack| stack.id == item))
}

/// The equipped weapon's ammo (`clip / capacity · spare`) and its colour,
/// for a weapon that takes ammunition.
fn ammo(state: &GameState) -> Option<(String, Tone)> {
    let weapon = equipped_weapon(state)?;
    let capacity = crate::weapons::clip_capacity(weapon);
    if capacity <= 1 {
        return None;
    }
    let quantity = state
        .inventory
        .iter()
        .find(|stack| stack.id == weapon)
        .map_or(0, |stack| stack.quantity);
    let clip = crate::weapons::clip_rounds(weapon, quantity);
    if crate::weapons::refills_itself(state, weapon) {
        return Some((
            format!("{clip} / {capacity} \u{00B7} infinite"),
            Tone::Normal,
        ));
    }
    let spare = state.item_count(crate::weapons::weapon_ammo_item_id(weapon));
    let tone = if clip == 0 && spare == 0 {
        Tone::Bad
    } else if clip == 0 {
        Tone::Warn
    } else {
        Tone::Normal
    };
    Some((format!("{clip} / {capacity} \u{00B7} {spare} spare"), tone))
}

/// The Player mode's panel: health with the condition, and the ammo.
fn mini_player_section(state: &GameState) -> Section {
    let mut section = Section::new("PLAYER");
    let (condition, tone) = condition(state);
    section.toned(
        "Health",
        format!(
            "{} / {} \u{00B7} {condition}",
            state.entities[0].health, state.max_health
        ),
        tone,
    );
    if let Some((value, tone)) = ammo(state) {
        section.toned("Ammo", value, tone);
    }
    section
}

fn player_section(state: &GameState, text: &crate::text::Text, input: &InputState) -> Section {
    let mut section = Section::new("PLAYER");
    let player = &state.entities[0];
    section.line(
        "Character",
        match state.id.player_flag {
            0 => "Chris",
            1 => "Jill",
            _ => "?",
        },
    );
    let [x, y, z] = player.pos;
    section.line("Position", format!("{x}, {y}, {z}"));
    let angle = player.angle & 0x0FFF;
    section.line(
        "Rotation",
        format!(
            "{angle:#05X} \u{00B7} {:.0}\u{00B0}",
            f64::from(angle) * 360.0 / 4096.0
        ),
    );
    let (condition, tone) = condition(state);
    section.toned(
        "Health",
        format!("{} / {}", player.health, state.max_health),
        tone,
    );
    section.toned("Status", condition, tone);
    section.line(
        "State",
        format!(
            "{:#06X} \u{00B7} anim {:#04X}",
            player.state_field, player.animation_id
        ),
    );

    if let Some(weapon) = equipped_weapon(state) {
        let name = text
            .item_name(u16::from(weapon))
            .map(|bytes| text.decode(bytes))
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| format!("item {weapon:#04X}"));
        section.line("Weapon", name);
        if let Some((value, tone)) = ammo(state) {
            section.toned("Ammo", value, tone);
        }
    } else {
        section.toned("Weapon", "none", Tone::Dim);
    }

    let held = input.held_word();
    let mut names: Vec<&str> = ACTIONS
        .iter()
        .filter(|action| held & action.bit() != 0)
        .map(|action| action.name())
        .collect();
    for (bit, name) in [
        (DEBUG_MENU_BIT, "debug"),
        (RETURN_TITLE_BIT, "title"),
        (SYSTEM_MENU_BIT, "menu"),
    ] {
        if held & bit != 0 {
            names.push(name);
        }
    }
    let input_value = if names.is_empty() {
        format!("{held:#07X}")
    } else {
        format!("{held:#07X} \u{00B7} {}", names.join(" "))
    };
    section.line("Input", input_value);
    section
}

/// The first boss in the room, by [`BOSSES`] order then slot.
/// Whether the entity in `slot` is a boss with `id`. Yawn's body segments
/// are copies of its head, id and all, so they are skipped.
fn is_boss(state: &GameState, slot: usize, id: u8) -> bool {
    let entity = &state.entities[slot];
    slot != 0
        && entity.id == id
        && entity.active()
        && crate::enemy::yawn::segment_head(entity).is_none()
}

fn find_boss(state: &GameState) -> Option<(usize, u8, &'static str)> {
    BOSSES.iter().find_map(|&(id, name)| {
        (1..state.entities.len())
            .find(|&slot| is_boss(state, slot, id))
            .map(|slot| (slot, id, name))
    })
}

/// Raise each boss in the room's recorded maximum to its current health.
fn track_bosses(state: &GameState, boss_max: &mut HashMap<BossKey, i16>) {
    for slot in 1..state.entities.len() {
        let id = state.entities[slot].id;
        if BOSSES.iter().any(|&(boss, _)| boss == id) && is_boss(state, slot, id) {
            let health = state.entities[slot].health;
            let max = boss_max.entry((state.id, slot, id)).or_insert(health);
            *max = (*max).max(health);
        }
    }
}

/// The boss panel; without `full`, just its name and health.
fn boss_section(
    state: &GameState,
    boss_max: &HashMap<BossKey, i16>,
    full: bool,
) -> Option<Section> {
    let (slot, id, name) = find_boss(state)?;
    let health = state.entities[slot].health;
    let max = boss_max
        .get(&(state.id, slot, id))
        .map_or(health, |&max| max.max(health));
    let mut section = Section::new("BOSS");
    let tone = if health <= 0 {
        Tone::Dim
    } else if i32::from(health) * 4 <= i32::from(max) {
        Tone::Bad
    } else if i32::from(health) * 2 <= i32::from(max) {
        Tone::Warn
    } else {
        Tone::Good
    };
    let value = if health <= 0 {
        format!("dead ({health})")
    } else {
        format!("{health} / {max}")
    };
    if !full {
        section.toned(name, value, tone);
        return Some(section);
    }
    section.line("Name", name);
    section.toned("Health", value, tone);
    let entity = &state.entities[slot];
    let [x, y, z] = entity.pos;
    section.line("Position", format!("{x}, {y}, {z}"));
    section.line("Slot", slot.to_string());
    Some(section)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ammo_ignores_the_volley_latch() {
        let mut state = GameState::default();
        assert_eq!(ammo(&state), None, "nothing equipped");
        state.equipped_weapon_id = 2;
        state.inventory = vec![crate::game::InventoryItem {
            id: 2,
            quantity: 15,
        }];
        assert_eq!(ammo(&state), None, "a weapon held but not equipped");
        state.equipped = Some(2);
        state.inventory = vec![crate::game::InventoryItem {
            id: 2,
            quantity: 0x80 | 14,
        }];
        let (value, tone) = ammo(&state).unwrap();
        assert!(value.starts_with("14 / "), "{value}");
        assert_eq!(tone, Tone::Normal);
        state.inventory[0].quantity = 0x80;
        assert_eq!(
            ammo(&state).unwrap().1,
            Tone::Bad,
            "an empty clip, no spare"
        );
    }

    #[test]
    fn the_health_state_matches_the_ekg() {
        assert_eq!(health_state(140, 140), 3);
        assert_eq!(health_state(70, 140), 1);
        assert_eq!(health_state(35, 140), 0);
        assert_eq!(health_state(0, 140), 0);
        assert_eq!(health_state(1, 3), 3, "a tiny maximum is always fine");
    }

    #[test]
    fn the_boss_is_the_first_live_one_in_priority_order() {
        let mut state = GameState::default();
        assert_eq!(find_boss(&state), None);
        state.entities[3].id = 0x04;
        state.entities[3].status_flags = 1;
        state.entities[5].id = 0x0C;
        state.entities[5].status_flags = 1;
        assert_eq!(
            find_boss(&state).map(|(slot, id, _)| (slot, id)),
            Some((5, 0x0C))
        );
        state.entities[5].status_flags = 0;
        assert_eq!(
            find_boss(&state).map(|(slot, id, _)| (slot, id)),
            Some((3, 0x04))
        );
    }

    #[test]
    fn a_yawn_body_segment_is_not_the_boss() {
        let mut state = GameState::default();
        state.entities[1].id = 0x0D;
        state.entities[1].status_flags = 1;
        state.entities[1].health = -1;
        state.entities[1].yawn_head = Some(4);
        state.entities[4].id = 0x0D;
        state.entities[4].status_flags = 1;
        state.entities[4].health = 200;
        assert_eq!(
            find_boss(&state).map(|(slot, id, _)| (slot, id)),
            Some((4, 0x0D))
        );
    }

    #[test]
    fn the_boss_maximum_is_the_most_health_seen() {
        let mut state = GameState::default();
        state.entities[2].id = 0x0D;
        state.entities[2].status_flags = 1;
        state.entities[2].health = 300;
        let mut boss_max = HashMap::new();
        track_bosses(&state, &mut boss_max);
        state.entities[2].health = 120;
        track_bosses(&state, &mut boss_max);
        let section = boss_section(&state, &boss_max, true).unwrap();
        assert_eq!(section.lines[1].value, "120 / 300");
        assert_eq!(section.lines[1].tone, Tone::Warn);
    }
}
