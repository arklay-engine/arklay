//! The names of a game's flag bits, for the debug menu's FLAGS screen.
//!
//! The game banks (scenario, common, lock, enemy and item bits) are the
//! game's own data, so the pack names them in [`ENTRY`], a Lua chunk of
//! `api:set_flag_name(bank, bit, name, description)` calls. `build.rs`
//! generates the shipped chunk from `games/re1/inc/flags.akinc`, the defines
//! the room scripts are written with ([`generate`]), and `convert-game` packs
//! it with the other `game/*.lua` scripts. The engine's own banks keep the
//! names in [`crate::ui::debug_menu`].

pub mod generate;

use std::collections::BTreeMap;

use crate::game::GameState;
use crate::pack::Pack;

/// The pack entry that names the game banks' bits.
pub const ENTRY: &str = "game/flags.lua";

/// One name of a bit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlagLabel {
    pub name: String,
    pub description: String,
}

/// Every named bit, by `(bank, bit)`; a bit can carry one name per player.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FlagNames {
    names: BTreeMap<(u8, u8), Vec<FlagLabel>>,
}

impl FlagNames {
    /// Add a name for `bit` of `bank`; a name already given is kept once.
    pub fn add(&mut self, bank: u8, bit: u8, name: &str, description: &str) {
        let labels = self.names.entry((bank, bit)).or_default();
        if labels.iter().all(|label| label.name != name) {
            labels.push(FlagLabel {
                name: name.to_string(),
                description: description.to_string(),
            });
        }
    }

    /// The names of `bit` of `bank`, in the order they were given.
    pub fn get(&self, bank: u8, bit: u8) -> &[FlagLabel] {
        self.names.get(&(bank, bit)).map_or(&[], Vec::as_slice)
    }

    /// The names of `bit` of `bank` for one row: `J_X / C_Y` for a bit each
    /// player names, or `None`.
    pub fn label(&self, bank: u8, bit: u8) -> Option<String> {
        let labels = self.get(bank, bit);
        (!labels.is_empty()).then(|| {
            labels
                .iter()
                .map(|label| label.name.as_str())
                .collect::<Vec<_>>()
                .join(" / ")
        })
    }

    pub fn len(&self) -> usize {
        self.names.values().map(Vec::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

/// The names `pack` gives: its [`ENTRY`], or the shipped copy when the pack
/// has none or it fails to run.
pub fn load(pack: &Pack) -> FlagNames {
    if let Ok(bytes) = pack.read(ENTRY) {
        match run(&bytes) {
            Ok(names) => return names,
            Err(err) => crate::warning!("[lua] {err}; using the built-in flag names"),
        }
    }
    load_builtin()
}

/// The shipped names, generated from `games/re1/inc/flags.akinc`.
pub fn load_builtin() -> FlagNames {
    let Some((_, source)) = crate::lua::GAME_SCRIPTS
        .iter()
        .find(|(entry, _)| *entry == ENTRY)
    else {
        return FlagNames::default();
    };
    run(source.as_bytes()).unwrap_or_else(|err| {
        crate::warning!("[lua] built-in {err}");
        FlagNames::default()
    })
}

fn run(bytes: &[u8]) -> Result<FlagNames, String> {
    let mut scratch = GameState::default();
    crate::lua::run_chunk(&mut scratch, bytes, ENTRY, 0).map_err(|err| err.to_string())?;
    Ok(scratch.flag_names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_chunk_names_the_game_banks_from_flags_akinc() {
        let names = load_builtin();
        assert_eq!(names.label(1, 0x02).as_deref(), Some("KENNETH_ZOMBIE_SEEN"));
        // A bit each player uses for something else carries both names.
        assert_eq!(
            names.label(1, 0xA0).as_deref(),
            Some("J_BARRY_DINING_ENTRY / C_MUSIC_SCORE_GIVEN")
        );
        assert_eq!(names.label(3, 0x1C).as_deref(), Some("104_ZOMBIE_GREEN_0"));
        assert_eq!(names.label(7, 0x34).as_deref(), Some("106_INK_3"));
        assert_eq!(names.label(2, 0x12).as_deref(), Some("103_104"));
        assert!(names.get(0, 0x7B)[0].description.contains("cleared"));
        // A bit no script uses, and the engine's banks, are not named here.
        assert_eq!(names.label(1, 0x06), None);
        assert_eq!(names.label(5, 0x0F), None);
    }

    #[test]
    fn the_generator_skips_placeholders_and_system_banks() {
        let akinc = "// header\n\
            #define F_COMMON_A4 0xA4\n\
            #define F_COMMON_INTRO_SEEN 0x00      // The \"intro\" played.\n\
            #define F_STATUS_LETTERBOX 0x0F       // Engine bit.\n\
            #define F_ITEM_104_AMMO_HANDGUN_0 0x29\n";
        let defines = generate::flag_defines(akinc);
        assert_eq!(defines.len(), 2);
        assert_eq!(
            (defines[0].bank, defines[0].bit, defines[0].name.as_str()),
            (1, 0x00, "INTRO_SEEN")
        );
        assert_eq!(defines[1].description, "");
        let lua = generate::flags_lua(akinc, "inc/flags.akinc");
        assert!(
            lua.contains(
                "api:set_flag_name(1, 0x00, \"INTRO_SEEN\", \"The \\\"intro\\\" played.\")"
            ),
            "{lua}"
        );
        let mut scratch = GameState::default();
        crate::lua::run_chunk(&mut scratch, lua.as_bytes(), ENTRY, 0).unwrap();
        assert_eq!(
            scratch.flag_names.label(7, 0x29).as_deref(),
            Some("104_AMMO_HANDGUN_0")
        );
    }
}
