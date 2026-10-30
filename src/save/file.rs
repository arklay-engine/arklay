//! Split out of the parent module so that edits there do not move these
//! definitions: types inside `GameState` are inlined into many codegen
//! units, and moving one recompiles all of them (#266).

use super::*;

/// One parsed save block.
///
/// The struct mirrors the original's BioCard layout field for field; see the
/// module docs for the byte offsets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveFile {
    /// Bio-card prefix copied verbatim from [`BIOCARD_PREFIX`] or a caller override.
    pub prefix: [u8; PREFIX_LEN],
    /// The 64-byte state image at BioCard 0x200..0x240, copied verbatim. The
    /// explicit fields below mirror the parts the port owns; the remaining
    /// bytes (0x208..0x20F, 0x214..0x223) survive only here.
    pub state_bytes: [u8; STATE_BYTES],
    /// Stage id (0x200), the original's 0-based `g_stageId`: the port's
    /// 1-based RDT stage digit minus one.
    pub stage: u8,
    /// Room id (0x201).
    pub room: u8,
    /// Room camera id (0x202).
    pub camera: u8,
    /// Cut saved by `cutnext` (0x204).
    pub cut: u8,
    /// Message-window menu choice (0x205).
    pub menu_choice: u8,
    /// Selected inventory item id (0x206).
    pub selected_item: u8,
    /// Number of held inventory slots (0x207).
    pub total_held: u8,
    /// Action hit by the forward position probe (0x210).
    pub fwd_action: u8,
    /// Action hit by the entity position probe (0x211).
    pub ent_action: u8,
    /// Last item used (0x212).
    pub used_item: u8,
    /// Last item picked up (0x213).
    pub picked_item: u8,
    /// Player health (0x21E).
    pub health: i16,
    /// Play timer snapshot (0x224).
    pub timer: u32,
    /// Save counter (0x228).
    pub saves: u8,
    /// Equipped item id (0x229).
    pub equipped: u8,
    /// Room-item backup byte (0x22A).
    pub room_item_backup: u8,
    /// Selected character id (0x22B).
    pub character: u8,
    /// Player position X (0x22C).
    pub pos_x: i16,
    /// Player position Z (0x22E).
    pub pos_z: i16,
    /// Port-only player height ([`HEIGHT_TRAILER`]); `0` for an original
    /// block.
    pub pos_y: i16,
    /// Port-only RE2 script state ([`super::RE2_TRAILER`]).
    pub re2: Option<Vec<u8>>,
    /// Player facing angle (0x230).
    pub angle: i16,
    /// Health status flags (0x232).
    pub health_status: u8,
    /// Scenario bank 2, 32 bytes (0x234).
    pub scenario2: [u8; 32],
    /// Door/desk lock flags, 8 bytes (0x254).
    pub locks: [u8; 8],
    /// Enemy flags, 32 bytes (0x25C).
    pub enemies: [u8; 32],
    /// Room-item flags, 32 bytes (0x27C).
    pub room_items: [u8; 32],
    /// Examined-item flags, 4 bytes (0x29C).
    pub examined: [u8; 4],
    /// Scenario bank 0, 16 bytes (0x2A0).
    pub scenario: [u8; 16],
    /// Room flags, 20 bytes (0x2B0).
    pub room_flags: [u8; 20],
    /// The 48 item-box slots (0x2C4).
    pub item_box: [InventoryItem; ITEM_BOX_SLOTS],
    /// The 12 player slots (0x324).
    pub player_slots: [InventoryItem; PLAYER_SLOT_COUNT],
    /// Room-BGM state table, 224 bytes (0x33C).
    pub room_bgm: [u8; ROOM_BGM_LEN],
}

impl Default for SaveFile {
    fn default() -> Self {
        Self {
            re2: None,
            prefix: [0; PREFIX_LEN],
            state_bytes: [0; STATE_BYTES],
            stage: 0,
            room: 0,
            camera: 0,
            cut: 0,
            menu_choice: 0,
            selected_item: 0,
            total_held: 0,
            fwd_action: 0,
            ent_action: 0,
            used_item: 0,
            picked_item: 0,
            health: 0,
            timer: 0,
            saves: 0,
            equipped: 0,
            room_item_backup: 0,
            character: 0,
            pos_x: 0,
            pos_z: 0,
            pos_y: 0,
            angle: 0,
            health_status: 0,
            scenario2: [0; 32],
            locks: [0; 8],
            enemies: [0; 32],
            room_items: [0; 32],
            examined: [0; 4],
            scenario: [0; 16],
            room_flags: [0; 20],
            item_box: [InventoryItem::default(); ITEM_BOX_SLOTS],
            player_slots: [InventoryItem::default(); PLAYER_SLOT_COUNT],
            room_bgm: [0; ROOM_BGM_LEN],
        }
    }
}
