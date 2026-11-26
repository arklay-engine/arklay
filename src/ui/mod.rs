//! UI screens: the title, character select and the save-load picker.
//!
//! Every screen implements [`Screen`], the shared contract the engine's app
//! loop drives. A screen loads its art in [`Screen::open`], advances one fixed
//! 30 Hz tick in [`Screen::update`], draws itself in [`Screen::draw`] and
//! reports a completed interaction through [`ScreenResult`]. The app reacts to
//! [`ScreenAction`] values by switching modes.
//!
//! The same contract also serves gameplay modals: `crate::engine::GameSession`
//! owns an optional boxed screen and freezes the room tick while one is
//! installed, so screens that own all of their state (the item viewer) plug in
//! without changing the mode machine. The message window and pause menu are
//! not [`Screen`]s: they share [`crate::game::GameState`] and are driven
//! explicitly by the session.

pub mod death;
pub mod debug_menu;
pub mod item_icons;
pub mod lua_screen;
pub mod modern_save;
pub mod save_load;

use std::path::Path;

mod scripts {
    include!(concat!(env!("OUT_DIR"), "/ui_scripts.rs"));
}
/// The checked-in screen scripts (`games/re1/ui/**.lua`) as `(entry, source)`
/// pairs. `pack convert game` packs them; a pack that predates them (or a test
/// pack) falls back to these copies, so every pack has its screens.
pub use scripts::UI_SCRIPTS;

use anyhow::{Context, Result};

use crate::assets::{self, Artwork, Geometry};
use crate::font::Font;
use crate::pack::{AssetClass, Pack};
use crate::render::Framebuffer;
use crate::state::Image;
use crate::text::Text;
use crate::transition::FadeColor;

/// One frame of UI input.
///
/// `up`/`down`/`left`/`right` are edges: true only on the tick the key went
/// down, so a screen never has to debounce. The `held_*` flags are the keys'
/// held level, which the item viewer reads to spin its model continuously;
/// captures and synthetic tests leave them false. `any` covers every other key
/// the platform reported this tick and drives the title's "press any button"
/// gate.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UiInput {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    /// Held level of the direction keys, for continuous spin.
    pub held_up: bool,
    /// Held level of the direction keys, for continuous spin.
    pub held_down: bool,
    /// Held level of the direction keys, for continuous spin.
    pub held_left: bool,
    /// Held level of the direction keys, for continuous spin.
    pub held_right: bool,
    /// Confirm (Space or Return by default).
    pub confirm: bool,
    /// Held level of confirm: RE 2's option screen steps a volume by ten
    /// while it is held.
    pub held_confirm: bool,
    /// Cancel (X, Backspace or Escape by default).
    pub cancel: bool,
    /// L1 (`[` by default): pages the item box back.
    pub page_left: bool,
    /// R1 (`]` by default): pages the item box forward.
    pub page_right: bool,
    /// Held level of L1: the item box keeps paging back while it is held.
    pub held_page_left: bool,
    /// Held level of R1: the item box keeps paging forward while it is held.
    pub held_page_right: bool,
    /// START (Tab by default): opens the gameplay pause menu; the debug
    /// menu's INVENTORY tool uses it to clear a slot.
    pub start: bool,
    /// F1: the port-only debug room overlay's edge. The overlay is always
    /// available.
    pub debug_menu: bool,
    /// F9: the in-game return-to-title prompt's edge. A second F9 confirms
    /// the return and any other key cancels.
    pub return_title: bool,
    /// Escape: opens the native in-game menu during free play.
    pub system_menu: bool,
    /// Port-only: the map button's edge (M / the gamepad's back by default):
    /// opens the map straight from free play and closes it again.
    pub map: bool,
    /// RE 2's option key's edge (the raw pad's `0x100`; Ctrl or F5 by
    /// default): opens the option screen in play.
    pub option: bool,
    /// RE 2's ARRANGE MODE pause key's held level (the raw pad's `0x2`;
    /// Pause by default), whose edge `Psx_main` pauses and resumes on.
    pub held_pause: bool,
    /// Port-only: the weapon cycle's edge during free play (the mouse
    /// wheel by default): `-1` the previous weapon, `1` the next, `0` none.
    pub weapon_step: i8,
    /// Port-only: the zero-based inventory slot whose equip button went down
    /// this tick (1 to 0 by default).
    pub equip_slot: Option<u8>,
    /// Any key went down this tick.
    pub any: bool,
    /// The first bindable input pressed this tick, whatever it is bound to:
    /// the rebind screen's capture.
    pub captured: Option<crate::controls::Source>,
    /// The keys and gamepad sources pressed this tick, in order, whatever
    /// they are bound to (the first [`NEWLY_DOWN`]; mouse buttons and the
    /// wheel aren't kept): RE 2's KEYCONFIG page reads
    /// the keyboard's and the gamepad's apart, as the original does.
    pub newly_down: [Option<crate::controls::Source>; NEWLY_DOWN],
}

/// How many of a tick's presses [`UiInput::newly_down`] keeps.
pub const NEWLY_DOWN: usize = 8;

/// Add `source` to a tick's presses, once, while there is room: a mouse
/// button or the wheel isn't one.
pub fn push_newly_down(
    list: &mut [Option<crate::controls::Source>; NEWLY_DOWN],
    source: crate::controls::Source,
) {
    if matches!(
        source,
        crate::controls::Source::Mouse(_) | crate::controls::Source::Wheel(_)
    ) {
        return;
    }
    if list.contains(&Some(source)) {
        return;
    }
    if let Some(slot) = list.iter_mut().find(|slot| slot.is_none()) {
        *slot = Some(source);
    }
}

/// What a screen reports after one tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScreenResult {
    /// Keep this screen and tick again.
    Continue,
    /// The screen finished; the app should apply [`ScreenAction`].
    Done(ScreenAction),
}

/// What a title chose for a new game besides the character: RE 2's
/// difficulty and ARRANGE MODE (`ui/title.lua`). The default is the game as
/// it starts with no choice: NORMAL, ORIGINAL MODE.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NewGameChoice {
    /// EASY: `fg_system`'s `0x20`.
    pub easy: bool,
    /// HARD: `hard_mode`.
    pub hard_mode: bool,
    /// ARRANGE MODE's `censorship_off`.
    pub censorship_off: bool,
    /// SPECIAL's extra mode, by its player: 0xC the 4th Survivor's HUNK,
    /// 0xD To-fu's TOFU, 0 for a game of the scenario's character.
    pub extra_player: u8,
    /// Extreme Battle's game, as its character's choice left it.
    pub exbattle: Option<ExBattleChoice>,
}

/// What Extreme Battle's title and its character's choice (`ExBattle`)
/// hand `Title_game_init`'s new game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExBattleChoice {
    /// The player (`0x989EF8`): 0 Leon, 1 Claire, 0xE Ada or 0xB Chris.
    pub player: u8,
    /// The level, 1 to 3 (`WORD_0098EB22`, the level page's `0x691EF3` + 1).
    pub level: u8,
    /// The levels open (`0x989E91`, which `WORD_0098EE7E` takes).
    pub levels: u8,
    /// `FUN_004E3A20`'s placement: which four of eight places (`DWORD_0098EBD0`'s
    /// low byte, `BYTE_0098EE9C` its complement), drawn from `Rnd` as the
    /// title left it.
    pub bits: u8,
}

/// A completed screen interaction, interpreted by the app's mode machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScreenAction {
    /// Start a new game as the character (`0` Chris, `1` Jill), with what
    /// the title chose besides.
    NewGame {
        character: u8,
        choice: NewGameChoice,
    },
    /// Continue from save slot `index` (zero-based, `savedat{index+1}.dat`).
    LoadGame { slot: usize },
    /// Continue from an explicit save file, the file a Lua screen chose.
    LoadFile(std::path::PathBuf),
    /// Continue from the save the load screen chose while the title that
    /// opened it slept (its `on_loaded`): the title's hand-over, after the
    /// prologue the save's start runs (RE 2's `Prologue_load`) when
    /// `prologue`.
    Continue { prologue: bool },
    /// Play an attract-demo reel (`demo/<hex>.dat`), then return to the
    /// title: reel `reel` where the title picks its own, as RE 2's does,
    /// else the next of the pack's.
    Attract { reel: Option<u8> },
    /// Return to the title screen.
    Title,
    /// Open the character-selection screen.
    CharSelect,
    /// Open the load screen.
    SaveLoad,
    /// Play RE 2's film `id` ([`crate::movie::re2_film`]), Claire's with
    /// `claire`, while the title that asked sleeps, then wake it with how
    /// the film ended (its `on_film`). A film that can't play is over at
    /// once. `gallery` is the gallery's playing of it: any skip button
    /// ends it whatever its own mask, and the frame the title drew as it
    /// asked shows until the film starts.
    Film { id: u8, claire: bool, gallery: bool },
    /// The ending films finished: open the RESULT screen.
    EndingResult,
    /// The RESULT screen finished: run the rocket-launcher epilogue (endings
    /// 6/7 without the special key) or go straight to the next-cycle save.
    EndingEpilogue,
    /// The epilogue finished (or was skipped): build the next-cycle card and
    /// open the save screen.
    EndingSave,
    /// Close a gameplay modal and resume the frozen room.
    Resume,
    /// Quit the app.
    Quit,
}

/// One UI cue a screen asks the app to play after its tick.
///
/// Every port screen is silent on its own: it queues a cue through
/// [`UiContext::play_cue`], and the app resolves and mixes it when an audio
/// device is open. Captures leave the queue drained and unplayed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiCue {
    /// The title's confirm voice line (`Evil01`, global bank 12 slot 0).
    Title,
    /// A cursor move (character-table slot 4).
    Cursor,
    /// A cancel (character-table slot 5).
    Cancel,
    /// A confirm/decide (character-table slot 6).
    Decide,
    /// The character-select cursor move (`Select06`, select bank slot 0).
    SelectCursor,
    /// The character-select confirm (`Select05`, select bank slot 1).
    SelectConfirm,
    /// The save/load cursor move (`type01`, title bank slot 14).
    SaveCursor,
    /// The save/load confirm (`type02`, title bank slot 15). The typed
    /// reveal's "typewriter" tick plays the same slot per revealed character.
    SaveConfirm,
    /// A FILE reader page turn (`Chris10`, character-table slot 8).
    PageTurn,
    /// The MAP tab's floor-plan zoom (`Chris09`, character-table slot 9).
    MapZoom,
    /// The MAP tab's objective-label blink (`Mapled`, character-table slot 10).
    MapLabel,
    /// A slot of a global sound bank (`LoadSoundBank(bank)` then
    /// `play_sfx(1, slot)`): the ending's bank 14 for its own cues.
    Bank(u8, u8),
    /// A current-room bank sound (`play_sfx(2, slot)`): the slot of the
    /// loaded room's sound-table row, which only the session can resolve.
    RoomSound(u8),
    /// An RE 2 core-bank slot (`Snd_se_on(0x4nn0000)`), from the bank the
    /// screen's game loaded (the title's `Snd_load_core(0x10)`).
    Core(u8),
    /// `Snd_load_core(id)`: the core bank the later [`UiCue::Core`] cues
    /// play from (the 4th Survivor's title loads 0x12, To-fu's 0x13).
    CoreBank(u8),
    /// RE 2's `Xa_play(1, line)`: line `line` of the voice bank no stage
    /// owns, for `player` (`voice/<player>/00_<line>.wav`), at the
    /// prologue's level.
    Voice { player: u8, line: u8 },
    /// `Xa_stop`: the voice line off.
    VoiceStop,
    /// `Snd_bgm_main` and `Snd_bgm_ctr(0x1000000)`: RE 2's main music `id`
    /// from its start.
    Music(u8),
    /// `Snd_bgm_fade_ON`: the music fades out.
    MusicFade,
    /// The music off.
    MusicStop,
}

impl UiCue {
    /// The pack `se/` name the cue plays, or `None` when the bank slot is
    /// empty.
    pub fn name(self) -> Option<&'static str> {
        match self {
            UiCue::Title => crate::sfx::global_sfx(12, 0),
            UiCue::Cursor => Some(crate::sfx::UI_CURSOR),
            UiCue::Cancel => Some(crate::sfx::UI_CANCEL),
            UiCue::Decide => Some(crate::sfx::UI_DECIDE),
            UiCue::SelectCursor => crate::sfx::global_sfx(13, 0),
            UiCue::SelectConfirm => crate::sfx::global_sfx(13, 1),
            UiCue::SaveCursor => crate::sfx::global_sfx(12, 14),
            UiCue::SaveConfirm => crate::sfx::global_sfx(12, 15),
            UiCue::PageTurn => Some(crate::sfx::UI_PAGE_TURN),
            UiCue::MapZoom => Some(crate::sfx::UI_MAP_ZOOM),
            UiCue::MapLabel => Some(crate::sfx::UI_MAP_LABEL),
            UiCue::Bank(bank, slot) => crate::sfx::global_sfx(bank, slot),
            UiCue::RoomSound(_)
            | UiCue::Core(_)
            | UiCue::CoreBank(_)
            | UiCue::Voice { .. }
            | UiCue::VoiceStop
            | UiCue::Music(_)
            | UiCue::MusicFade
            | UiCue::MusicStop => None,
        }
    }
}

/// Load one UI page by stem: the shipped TIM or a mod's replacement.
///
/// A page is addressed by fixed UVs or sprite rectangles, so a replacement
/// must keep the shipped page's dimensions; see [`Geometry::Shipped`].
pub fn load_page(pack: &Pack, stem: &str) -> Result<Artwork> {
    let asset = pack
        .read_asset(stem, AssetClass::Image)
        .with_context(|| format!("failed to read {stem}"))?
        .with_context(|| format!("missing {stem}"))?;
    assets::decode_page(&asset, Geometry::Shipped)
        .with_context(|| format!("invalid {}", asset.path()))
}

/// Load one full-frame picture by stem, with an optional transparency key.
///
/// [`Geometry::Free`] accepts any size (the caller stretches the picture over
/// its frame); [`Geometry::Shipped`] keeps a sheet that is sampled by fixed
/// rectangles at the shipped dimensions.
pub fn load_picture(
    pack: &Pack,
    stem: &str,
    geometry: Geometry,
    key: Option<[u8; 3]>,
) -> Result<Image> {
    let asset = pack
        .read_asset(stem, AssetClass::Image)
        .with_context(|| format!("failed to read {stem}"))?
        .with_context(|| format!("missing {stem}"))?;
    assets::decode_picture(&asset, geometry, key)
        .with_context(|| format!("invalid {}", asset.path()))
}

/// What the screens keep from one opening to the next: the original holds
/// such work in the executable's static data (the save list's name scroll,
/// the option screen's rows), where it lasts as long as the process. The
/// app owns one and shares it with its sessions; it is never saved.
///
/// Each screen script has its own keys (its pack entry is the scope, in any
/// case, as the entry is looked up), so one script can neither read nor
/// crowd out another's.
#[derive(Debug, Clone, Default)]
pub struct ScreenStatics(
    std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<(String, String), i64>>>,
    /// RE 2's handgun byte, and what this copy's own loads have done to it
    /// since it was made ([`ScreenStatics::re2_adopt`]).
    std::rc::Rc<std::cell::Cell<(crate::player2::HandgunFrame, crate::player2::HandgunUpdate)>>,
);

impl ScreenStatics {
    /// The most keys one script may keep.
    pub const MAX_KEYS: usize = 64;

    /// The value the script `scope` keeps under `key`.
    pub fn get(&self, scope: &str, key: &str) -> Option<i64> {
        self.0
            .borrow()
            .get(&(scope.to_ascii_lowercase(), key.to_owned()))
            .copied()
    }

    /// Keep `value` under the script `scope`'s `key`, or forget the key with
    /// `None`. A script's new key past [`Self::MAX_KEYS`] is refused.
    pub fn set(&self, scope: &str, key: &str, value: Option<i64>) -> bool {
        let mut kept = self.0.borrow_mut();
        let at = (scope.to_ascii_lowercase(), key.to_owned());
        match value {
            None => {
                kept.remove(&at);
            }
            Some(value) => {
                let held = kept.keys().filter(|(owner, _)| *owner == at.0).count();
                if !kept.contains_key(&at) && held >= Self::MAX_KEYS {
                    return false;
                }
                kept.insert(at, value);
            }
        }
        true
    }

    /// RE 2's handgun row's first byte as the process's loads of it have
    /// left it ([`crate::player2::HandgunFrame`]), which play and the
    /// KEYCONFIG page share.
    pub fn re2_handgun_frame(&self) -> crate::player2::HandgunFrame {
        self.1.get().0
    }

    /// `Pl_weapon_ch`'s end for character `id` holding `weapon`
    /// ([`crate::player2::HandgunFrame::equip`]), returning the new byte.
    pub fn re2_equip(&self, id: u8, weapon: u8) -> crate::player2::HandgunFrame {
        let update = crate::player2::HandgunUpdate::equip(id, weapon);
        let (frame, since) = self.1.get();
        let frame = update.apply(frame);
        self.1.set((frame, since.then(update)));
        frame
    }

    /// Take on the loads `other` saw since it was made: a session's own
    /// statics, which its room boot loaded the player into before the app
    /// gave it these, the process's.
    pub fn re2_adopt(&self, other: &ScreenStatics) {
        if std::rc::Rc::ptr_eq(&self.1, &other.1) {
            return;
        }
        let (frame, since) = self.1.get();
        let update = other.1.get().1;
        self.1.set((update.apply(frame), since.then(update)));
    }
}

/// Everything a screen may read while it runs.
///
/// The context is built fresh each tick by the app, so screens never own (or
/// borrow beyond a call) the pack.
pub struct UiContext<'a> {
    /// The open game pack.
    pub pack: &'a Pack,
    /// Save directory (`--save-dir`, `arklay.toml`'s `save_dir`, default
    /// `<config dir>/save`).
    pub save_dir: &'a Path,
    /// The decoded font, when the pack carries one.
    pub font: Option<&'a Font>,
    /// The decoded text tables, when the pack carries them.
    pub text: Option<&'a Text>,
    /// Fixed ticks the app has run, for deterministic animations.
    pub ticks: u64,
    /// Cues the screen requested this tick; the app drains them after
    /// [`Screen::update`]. Interior mutability keeps `update`'s `&UiContext`
    /// borrow (screens only ever push).
    pub cues: std::cell::RefCell<Vec<UiCue>>,
    /// The paused session's game state, for the in-game screens' rule
    /// queries; `None` outside a session.
    pub game: Option<&'a crate::game::GameState>,
    /// Where the screen's scripts keep values past the screen
    /// (`api:keep`); `None` for a context with nowhere to keep them (a unit
    /// test's), where a kept value reads as absent.
    pub statics: Option<&'a ScreenStatics>,
    /// Whether the run was started with `--authentic`. Screens with no paused
    /// session read it through `api:authentic()`; a paused session's game
    /// state answers when there is one.
    pub authentic: bool,
}

impl UiContext<'_> {
    /// Queue one cue for the app to play after this tick.
    pub fn play_cue(&self, cue: UiCue) {
        self.cues.borrow_mut().push(cue);
    }
}

/// The shared screen contract.
pub trait Screen {
    /// Load the screen's art and initial state. Called once before the first
    /// update; missing optional art is logged and leaves that layer empty.
    fn open(&mut self, cx: &mut UiContext<'_>) -> Result<()>;

    /// Advance one fixed tick from edge-triggered input.
    fn update(&mut self, cx: &UiContext<'_>, input: UiInput) -> ScreenResult;

    /// Draw one frame into `framebuffer`. The framebuffer already holds the
    /// frozen gameplay frame when the screen runs as a modal.
    fn draw(&mut self, cx: &UiContext<'_>, framebuffer: &mut Framebuffer);

    /// Black-overlay opacity in `0..=255` drawn over the screen after it
    /// draws; `0` means fully visible and `255` fully black.
    fn fade(&self) -> u8 {
        0
    }

    /// The full-screen overlay to blend after the screen draws, or `None`
    /// while the frame is clear.
    ///
    /// The default maps [`Screen::fade`] to a black overlay, so a screen that
    /// only ever fades to black keeps implementing `fade`; screens with the
    /// original's white flash override this to select the colour.
    fn overlay(&self) -> Option<(FadeColor, u8)> {
        match self.fade() {
            0 => None,
            alpha => Some((FadeColor::Black, alpha)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_handguns_first_frame_is_the_processs_and_shared() {
        use crate::player2::HandgunFrame;
        let app = ScreenStatics::default();
        // The session's copy is the app's: play's loads reach the title.
        let session = app.clone();
        assert_eq!(app.re2_handgun_frame(), HandgunFrame(0x85));
        assert_eq!(session.re2_equip(8, 2), HandgunFrame(0x03));
        assert_eq!(app.re2_handgun_frame(), HandgunFrame(0x03));
        assert_eq!(app.re2_equip(0, 2), HandgunFrame(0x83));
        assert_eq!(app.re2_equip(1, 3), HandgunFrame(0x83));
        assert_eq!(session.re2_handgun_frame(), HandgunFrame(0x83));
    }

    /// A session's room boot loads its player before the app gives it the
    /// process's statics; the app takes on those loads, in order.
    #[test]
    fn the_app_takes_on_the_loads_a_sessions_boot_made() {
        use crate::player2::HandgunFrame;
        for (loads, after) in [
            (&[][..], 0x85),
            (&[(8, 2)][..], 0x03),
            (&[(0xA, 2), (0, 2)][..], 0x83),
            (&[(0, 2)][..], 0x85),
            (&[(8, 3)][..], 0x85),
        ] {
            let app = ScreenStatics::default();
            let boot = ScreenStatics::default();
            for &(id, weapon) in loads {
                boot.re2_equip(id, weapon);
            }
            app.re2_adopt(&boot);
            assert_eq!(app.re2_handgun_frame(), HandgunFrame(after), "{loads:?}");
            app.re2_adopt(&app.clone());
            assert_eq!(app.re2_handgun_frame(), HandgunFrame(after));
        }
        // After 8's, the app's own 0x83 stays 0x83 on a later boot by Leon.
        let app = ScreenStatics::default();
        app.re2_equip(8, 2);
        let boot = ScreenStatics::default();
        boot.re2_equip(0, 2);
        app.re2_adopt(&boot);
        assert_eq!(app.re2_handgun_frame(), HandgunFrame(0x83));
    }

    #[test]
    fn cue_names_resolve_to_pack_sounds() {
        assert_eq!(UiCue::Cursor.name(), Some("cursor"));
        assert_eq!(UiCue::Cancel.name(), Some("cancel"));
        assert_eq!(UiCue::Decide.name(), Some("decide"));
        assert_eq!(UiCue::Title.name(), Some("Evil01"));
        assert_eq!(UiCue::SelectCursor.name(), Some("Select06"));
        assert_eq!(UiCue::SelectConfirm.name(), Some("Select05"));
        assert_eq!(UiCue::SaveCursor.name(), Some("type01"));
        assert_eq!(UiCue::SaveConfirm.name(), Some("type02"));
        assert_eq!(UiCue::MapZoom.name(), Some("Chris09"));
        assert_eq!(UiCue::MapLabel.name(), Some("Mapled"));
        assert_eq!(UiCue::PageTurn.name(), Some("Chris10"));
        assert_eq!(
            UiCue::RoomSound(0x21).name(),
            None,
            "the session resolves it"
        );
    }

    #[test]
    fn the_default_overlay_is_the_black_fade() {
        struct Black {
            alpha: u8,
        }
        impl Screen for Black {
            fn open(&mut self, _cx: &mut UiContext<'_>) -> Result<()> {
                Ok(())
            }
            fn update(&mut self, _cx: &UiContext<'_>, _input: UiInput) -> ScreenResult {
                ScreenResult::Continue
            }
            fn draw(&mut self, _cx: &UiContext<'_>, _framebuffer: &mut Framebuffer) {}
            fn fade(&self) -> u8 {
                self.alpha
            }
        }

        assert_eq!(Black { alpha: 0 }.overlay(), None);
        assert_eq!(
            Black { alpha: 128 }.overlay(),
            Some((FadeColor::Black, 128))
        );
    }
}
