//! Room background-music tables and pack path naming.
//!
//! The shipped game selects music from per-room state bytes and three tables:
//! a room's candidate group ids, each group's up-to-three track ids, and each
//! track's whole-buffer loop flag. The tables and the initial room state live
//! in the pack's `game/music.lua` and `game/initial_state.lua`; the engine and
//! Lua only ever see numeric ids. The converter owns the original
//! `Bgm_*`/`Se_*` basename-to-id mapping, and every track is a pack entry
//! `bgm/<id:03x>`.

use anyhow::{Result, bail};

use crate::game::GameState;
use crate::pack::Pack;

/// Pack entry carrying the static music tables.
pub const MUSIC_ENTRY: &str = "game/music.lua";
/// Pack entry carrying the shipped new-game state.
pub const INITIAL_STATE_ENTRY: &str = "game/initial_state.lua";

/// One music track id. The pack entry is `bgm/<id:03x>`.
pub type TrackId = u16;
/// One group id. Group ids are logical presets, not pack paths.
pub type GroupId = u8;
/// A room row slot with no group.
pub const NO_GROUP: GroupId = 0xFF;

/// One group preset: up to three simultaneous channel tracks and their loop
/// flags. An empty group is silent, matching the shipped empty rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MusicGroup {
    /// Track loaded into each of the three BGM channels.
    pub tracks: [Option<TrackId>; 3],
    /// Whole-buffer loop flag for each channel.
    pub loops: [bool; 3],
}

/// The static music tables loaded from the pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MusicTables {
    /// Group presets indexed by group id.
    pub groups: [Option<MusicGroup>; 256],
    /// Up to four group variants per stage/room, selected by `state & 7`.
    pub rooms: [[[GroupId; 4]; 32]; 7],
}

impl Default for MusicTables {
    fn default() -> Self {
        Self {
            groups: [None; 256],
            rooms: [[[NO_GROUP; 4]; 32]; 7],
        }
    }
}

impl MusicTables {
    /// The group selected by `state` for the room at 0-based `stage` and
    /// `room`. `state == 0xFF` means no music.
    pub fn group_for(&self, stage: usize, room: usize, state: u8) -> Option<GroupId> {
        if state == 0xFF {
            return None;
        }
        let group = *self
            .rooms
            .get(stage)?
            .get(room)?
            .get(usize::from(state & 7))?;
        (group != NO_GROUP).then_some(group)
    }

    /// The tracks and loop flags of `group`, or `None` when the group is
    /// undefined.
    pub fn group(&self, group: GroupId) -> Option<&MusicGroup> {
        self.groups.get(usize::from(group))?.as_ref()
    }

    /// Define or replace `group`.
    pub fn set_group(&mut self, group: GroupId, value: MusicGroup) -> Result<()> {
        if group == NO_GROUP {
            bail!("music group 0xFF is reserved");
        }
        self.groups[usize::from(group)] = Some(value);
        Ok(())
    }

    /// Replace the whole room row with `groups` (at most four).
    pub fn set_room(&mut self, stage: usize, room: usize, groups: &[GroupId]) -> Result<()> {
        if stage >= self.rooms.len() {
            bail!("music stage {stage} is out of range");
        }
        if room >= self.rooms[stage].len() {
            bail!("music room {room:#04x} is out of range");
        }
        if groups.len() > 4 {
            bail!("a room has at most four music group variants");
        }
        let row = &mut self.rooms[stage][room];
        *row = [NO_GROUP; 4];
        row[..groups.len()].copy_from_slice(groups);
        Ok(())
    }

    /// Check that every referenced group exists and no room names 0xFF as a
    /// real group.
    pub fn validate(&self) -> Result<()> {
        for (stage, rooms) in self.rooms.iter().enumerate() {
            for (room, row) in rooms.iter().enumerate() {
                for &group in row {
                    if group == NO_GROUP {
                        continue;
                    }
                    if self.group(group).is_none() {
                        bail!(
                            "room {}{room:02X} names undefined music group {group:#04x}",
                            stage + 1
                        );
                    }
                }
            }
        }
        Ok(())
    }
}

/// The pack path of one track.
pub fn bgm_entry(track: TrackId) -> String {
    format!("bgm/{track:03x}.wav")
}

/// Load the static music tables for `pack`.
///
/// The pack's `game/music.lua` wins; a missing or invalid entry falls back to
/// the copy embedded from `games/re1/game/music.lua`.
pub fn load(pack: &Pack) -> MusicTables {
    if let Ok(bytes) = pack.read(MUSIC_ENTRY) {
        let mut scratch = GameState::default();
        match crate::lua::run_chunk(&mut scratch, &bytes, MUSIC_ENTRY, 0) {
            Ok(()) => match scratch.music.validate() {
                Ok(()) => return scratch.music,
                Err(err) => {
                    eprintln!("[lua] {MUSIC_ENTRY}: {err}; using the built-in music tables")
                }
            },
            Err(err) => eprintln!("[lua] {err}; using the built-in music tables"),
        }
    }
    load_builtin()
}

/// Load the copy embedded at build time from `games/re1/game/music.lua`.
pub fn load_builtin() -> MusicTables {
    let Some((_, source)) = crate::lua::GAME_SCRIPTS
        .iter()
        .find(|(entry, _)| *entry == MUSIC_ENTRY)
    else {
        return MusicTables::default();
    };
    let mut scratch = GameState::default();
    if let Err(err) = crate::lua::run_chunk(&mut scratch, source.as_bytes(), MUSIC_ENTRY, 0) {
        eprintln!("[lua] built-in {MUSIC_ENTRY}: {err}");
        return MusicTables::default();
    }
    match scratch.music.validate() {
        Ok(()) => scratch.music,
        Err(err) => {
            eprintln!("[lua] built-in {MUSIC_ENTRY}: {err}");
            MusicTables::default()
        }
    }
}

/// Original `Sound/` track basename -> pack track id (`bgm/<id:03x>`).
/// Conversion-time only; the engine and Lua see ids.
pub const MUSIC_TRACKS: &[(u16, &str)] = &[
    (0x000, "Bgm_00"),
    (0x002, "Bgm_02"),
    (0x004, "Bgm_04"),
    (0x005, "Bgm_05"),
    (0x007, "Bgm_07"),
    (0x008, "Bgm_08"),
    (0x009, "Bgm_09"),
    (0x00a, "Bgm_0a"),
    (0x00b, "Bgm_0b"),
    (0x00c, "Bgm_0c"),
    (0x00e, "Bgm_0e"),
    (0x00f, "Bgm_0f"),
    (0x010, "Bgm_10"),
    (0x011, "Bgm_11"),
    (0x012, "Bgm_12"),
    (0x013, "Bgm_13"),
    (0x014, "Bgm_14"),
    (0x015, "Bgm_15"),
    (0x016, "Bgm_16"),
    (0x017, "Bgm_17"),
    (0x018, "Bgm_18"),
    (0x019, "Bgm_19"),
    (0x01a, "Bgm_1a"),
    (0x01b, "Bgm_1b"),
    (0x01c, "Bgm_1c"),
    (0x01d, "Bgm_1d"),
    (0x01e, "Bgm_1e"),
    (0x01f, "Bgm_1f"),
    (0x020, "Bgm_20"),
    (0x023, "Bgm_23"),
    (0x024, "Bgm_24"),
    (0x025, "Bgm_25"),
    (0x026, "Bgm_26"),
    (0x028, "Bgm_28"),
    (0x029, "Bgm_29"),
    (0x02b, "Bgm_2b"),
    (0x02c, "Bgm_2c"),
    (0x02d, "Bgm_2d"),
    (0x02e, "Bgm_2e"),
    (0x02f, "Bgm_2f"),
    (0x030, "Bgm_30"),
    (0x031, "Bgm_31"),
    (0x032, "Bgm_32"),
    (0x033, "Bgm_33"),
    (0x034, "Bgm_34"),
    (0x036, "Bgm_36"),
    (0x037, "Bgm_37"),
    (0x038, "Bgm_38"),
    (0x03a, "Bgm_3a"),
    (0x03b, "Bgm_3b"),
    (0x03d, "Bgm_3d"),
    (0x03e, "Bgm_3e"),
    (0x03f, "Bgm_3f"),
    (0x040, "Bgm_40"),
    (0x048, "Bgm_48"),
    (0x049, "Bgm_49"),
    (0x04a, "Bgm_4a"),
    (0x056, "Bgm_56"),
    (0x057, "Bgm_57"),
    (0x100, "chain1"),
    (0x101, "Se_01"),
    (0x102, "Se_03"),
    (0x103, "Se_06"),
    (0x104, "Se_39lp"),
    (0x105, "Se_3c"),
    (0x106, "Se_41"),
    (0x107, "Se_42"),
    (0x108, "Se_43"),
    (0x109, "Se_44"),
    (0x10a, "Se_45"),
    (0x10b, "Se_46"),
    (0x10c, "Se_4b"),
    (0x10d, "Se_4c"),
    (0x10e, "Se_4d"),
    (0x10f, "Se_4e"),
    (0x110, "Se_4f"),
    (0x111, "Se_50"),
    (0x112, "Se_51"),
    (0x113, "Se_53"),
    (0x114, "Se_54"),
    (0x115, "Se_55"),
    (0x116, "Se_59"),
    (0x117, "Se_5a"),
    (0x118, "V110_00"),
    (0x200, "Bgm_24a"),
    (0x201, "Bgm_24b"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_ids_are_unique_and_paths_are_three_hex_digits() {
        let mut ids: Vec<u16> = MUSIC_TRACKS.iter().map(|&(id, _)| id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), MUSIC_TRACKS.len());
        assert_eq!(bgm_entry(0x013), "bgm/013.wav");
        assert_eq!(bgm_entry(0x100), "bgm/100.wav");
        assert_eq!(bgm_entry(0x200), "bgm/200.wav");
    }

    fn pack_with_music(source: &str) -> Pack {
        let mut writer = crate::pack::PackWriter::new();
        writer.add(MUSIC_ENTRY, source.as_bytes().to_vec()).unwrap();
        Pack::from_bytes(writer.to_bytes().unwrap()).unwrap()
    }

    #[test]
    fn pack_music_entry_overrides_builtin_and_invalid_falls_back() {
        let custom = r#"
            local api = ...
            api:set_music_group(0x01, { { 0x013, true } })
            api:set_room_music(1, 0x00, { 0x01 })
        "#;
        let tables = load(&pack_with_music(custom));
        assert_eq!(tables.group(0x01).unwrap().tracks[0], Some(0x013));
        assert_eq!(tables.group_for(0, 0x00, 0x08), Some(0x01));
        assert_eq!(tables.group_for(0, 0x00, 0x40), Some(0x01));

        // An undefined group reference makes the pack table invalid; the
        // built-in copy is used instead.
        let invalid = r#"
            local api = ...
            api:set_room_music(1, 0x00, { 0x02 })
        "#;
        let tables = load(&pack_with_music(invalid));
        assert_eq!(tables.group_for(0, 0x00, 0x40), Some(0x0B));
    }

    #[test]
    fn parsed_builtin_tables_match_known_rooms() {
        let tables = load_builtin();
        tables.validate().expect("valid tables");
        assert_eq!(tables.group_for(0, 0x00, 0x40), Some(0x0B));
        assert_eq!(tables.group_for(0, 0x00, 0xFF), None);
        assert_eq!(tables.group_for(7, 0, 0x40), None);
        assert_eq!(
            tables.group(0x0B).unwrap().tracks,
            [Some(0x013), None, None]
        );
        assert_eq!(
            tables.group(0x00).unwrap().tracks,
            [Some(0x000), Some(0x101), Some(0x005)]
        );
        // Group 0x18 has a gap in the source slots: Bgm_05 is slot 2 and
        // loops even though slot 1 is empty.
        assert_eq!(
            tables.group(0x18).unwrap().tracks,
            [Some(0x018), Some(0x005), None]
        );
        assert_eq!(tables.group(0x18).unwrap().loops, [true, true, false]);
        assert_eq!(tables.group(0x2a).unwrap().tracks, [None; 3]);
        assert_eq!(tables.group(0x38).unwrap().tracks, [None; 3]);
        assert_eq!(tables.group_for(5, 0x03, 0x08), Some(0x18));
        assert_eq!(tables.group_for(0, 0x00, 0x41), Some(0x0F));
    }
}
