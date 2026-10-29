//! Split out of the parent module so that edits there do not move these
//! definitions: types inside `GameState` are inlined into many codegen
//! units, and moving one recompiles all of them (#266).

use super::*;

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
