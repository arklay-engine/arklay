//! The original 0x800-byte BioCard save block and the save directory.
//!
//! A save is the original block byte for byte: a 0x200-byte BioCard prefix
//! (magic, timestamp and unused icon data, hardcoded in [`BIOCARD_PREFIX`])
//! followed by the little-endian state fields at their original offsets. The
//! block is 0x800 bytes long; everything past the last field is zero padding.
//! A save away from floor level appends the port-only height trailer
//! ([`HEIGHT_TRAILER`]) after the block.
//!
//! [`SaveFile`] can be built from a [`GameState`], serialized, parsed back and
//! applied to a state, so a load rebuilds the room identity, position, health,
//! inventory, item box, flags, save counter and the room-BGM table.
//!
//! Slots live under a save directory as `savedat1.dat`..`savedat8.dat`
//! (zero-based slot indexes). [`scan_slots`] reports which slots hold data for
//! the load screen; missing or truncated files read as empty. The default save
//! directory is `<config dir>/save` ([`crate::config`]); [`scan_dir`] is the
//! folder-wide variant the Lua save screen uses: every regular file, sorted,
//! with the parsed header and modification time of each.
//!
//! The interactive game also autosaves into the same directory each time a
//! door loads a room ([`write_autosave`]): `autosave1.dat` is the newest and
//! `autosave2.dat` the one before it, so the pair is typically the start of
//! the current room and the start of the previous one.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::budget;
use crate::game::{
    BgmState, CameraState, FLAG_BANK_COUNT, FlagBank, GameState, ITEM_BOX_SLOTS, InventoryItem,
    STATE_BYTE_CHARACTER, STATE_BYTE_CUT, STATE_BYTE_ENT_ACTION, STATE_BYTE_EQUIPPED,
    STATE_BYTE_FWD_ACTION, STATE_BYTE_HEALTH_STATUS, STATE_BYTE_MENU_CHOICE,
    STATE_BYTE_PICKED_ITEM, STATE_BYTE_ROOM_CAMERA, STATE_BYTE_SAVES, STATE_BYTE_SELECTED_ITEM,
    STATE_BYTE_TOTAL_HELD, STATE_BYTE_USED_ITEM, STATE_BYTES, VoiceState, character_max_health,
};
use crate::state::RoomId;

/// Bytes in one save block.
pub const SAVE_BLOCK_SIZE: usize = 0x800;
/// Bytes of the block the field layout actually uses (0x000..=0x41B).
pub const SAVE_LAYOUT_SIZE: usize = 0x41C;
/// Port-only: the magic of the player-height trailer appended after the
/// 0x800 block (`AKY`, version 1), followed by the height as a little-endian
/// `i16` and two reserved zero bytes. The original stores only X and Z and
/// always loads the player at height 0, which is right at a typewriter but
/// not for an autosave taken on a stair landing or a raised floor. The
/// trailer is written only for a non-zero height, so a save at floor level
/// stays the plain block, and it is read only from a file exactly
/// [`SAVE_BLOCK_SIZE`] + [`HEIGHT_TRAILER_LEN`] long: the original's own
/// 0xA82-byte files keep their pad remap at 0x41C and controls past 0x800,
/// and are never mistaken for it.
pub const HEIGHT_TRAILER: [u8; 4] = *b"AKY\x01";
/// Bytes of the height trailer: the magic, the `i16` height and two
/// reserved bytes.
pub const HEIGHT_TRAILER_LEN: usize = 8;
/// Bytes of the bio-card prefix at the start of the block.
pub const PREFIX_LEN: usize = 0x200;
/// BioCard offset the [`GameState`] state image starts at.
pub const STATE_IMAGE_OFFSET: usize = 0x200;
/// The shipped BioCard save prefix (magic, timestamp and unused icon data).
/// Hardcoded so saves stay byte-identical without `data/bio_card.dat`.
pub const BIOCARD_PREFIX: [u8; PREFIX_LEN] = [
    0x53, 0x43, 0x13, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x73, 0x4e, 0x31, 0x4a, 0xf1, 0x3d, 0xcd, 0x3d, 0x6d, 0x2d, 0x6a, 0x31, 0xe9, 0x20,
    0x28, 0x29, 0xe6, 0x20, 0xa5, 0x14, 0x83, 0x14, 0x63, 0x0c, 0x2b, 0x25, 0x00, 0x80, 0x00, 0x00,
    0xee, 0xee, 0xee, 0xee, 0xee, 0xee, 0xee, 0xee, 0xee, 0xee, 0xee, 0xcc, 0xec, 0xec, 0xee, 0xee,
    0xee, 0xee, 0xac, 0xaa, 0xaa, 0xcc, 0xee, 0xee, 0xee, 0xac, 0x6a, 0x24, 0x42, 0xa8, 0xca, 0xee,
    0xce, 0x6a, 0x12, 0x11, 0x11, 0x41, 0xc8, 0xec, 0xac, 0x46, 0x21, 0x42, 0x24, 0x11, 0x83, 0xea,
    0x8a, 0x24, 0x42, 0x88, 0x88, 0x24, 0x42, 0xcb, 0x4a, 0x22, 0x84, 0x99, 0x14, 0x49, 0x32, 0xb5,
    0x47, 0x22, 0x98, 0xbb, 0xbb, 0x89, 0x32, 0x75, 0x5d, 0x42, 0xb9, 0xe4, 0xbe, 0x9b, 0x34, 0x75,
    0x57, 0x44, 0xb9, 0xee, 0xee, 0x9b, 0x54, 0xdd, 0x7b, 0x45, 0xb9, 0xee, 0xee, 0x9b, 0xd8, 0xc7,
    0xac, 0x89, 0xb8, 0xeb, 0xbe, 0x8b, 0xa8, 0xec, 0xce, 0xbc, 0xac, 0xbb, 0xbb, 0xac, 0xcc, 0xee,
    0xee, 0xce, 0xcc, 0xec, 0xee, 0xec, 0xee, 0xee, 0xee, 0xee, 0xee, 0xee, 0xee, 0xee, 0xee, 0xee,
    0xee, 0xee, 0xee, 0xee, 0xee, 0xee, 0xee, 0xee, 0xee, 0xee, 0xee, 0xcc, 0xec, 0xec, 0xee, 0xee,
    0xee, 0xee, 0xac, 0xaa, 0xaa, 0xcc, 0xee, 0xee, 0xee, 0xac, 0x6a, 0x24, 0x42, 0xa8, 0xca, 0xee,
    0xce, 0x6a, 0x12, 0x11, 0x11, 0x41, 0xc8, 0xec, 0xac, 0x46, 0x21, 0x42, 0x24, 0x11, 0x83, 0xea,
    0x8a, 0x24, 0x42, 0x88, 0x88, 0x24, 0x42, 0xcb, 0x4a, 0x22, 0x84, 0x99, 0x14, 0x49, 0x32, 0xb5,
    0x47, 0x22, 0x98, 0xbb, 0xbb, 0x89, 0x32, 0x75, 0x5d, 0x42, 0xb9, 0xe4, 0xbe, 0x9b, 0x34, 0x75,
    0x57, 0x44, 0xb9, 0xee, 0xee, 0x9b, 0x54, 0xdd, 0x7b, 0x45, 0xb9, 0xee, 0xee, 0x9b, 0xd8, 0xc7,
    0xac, 0x89, 0xb8, 0xeb, 0xbe, 0x8b, 0xa8, 0xec, 0xce, 0xbc, 0xac, 0xbb, 0xbb, 0xac, 0xcc, 0xee,
    0xee, 0xce, 0xcc, 0xec, 0xee, 0xec, 0xee, 0xee, 0xee, 0xee, 0xee, 0xee, 0xee, 0xee, 0xee, 0xee,
    0xee, 0xee, 0xee, 0xee, 0xee, 0xee, 0xee, 0xee, 0xee, 0xee, 0xee, 0xcc, 0xec, 0xec, 0xee, 0xee,
    0xee, 0xee, 0xac, 0xaa, 0xaa, 0xcc, 0xee, 0xee, 0xee, 0xac, 0x6a, 0x24, 0x42, 0xa8, 0xca, 0xee,
    0xce, 0x6a, 0x12, 0x11, 0x11, 0x41, 0xc8, 0xec, 0xac, 0x46, 0x21, 0x42, 0x24, 0x11, 0x83, 0xea,
    0x8a, 0x24, 0x42, 0x88, 0x88, 0x24, 0x42, 0xcb, 0x4a, 0x22, 0x84, 0x99, 0x14, 0x49, 0x32, 0xb5,
    0x47, 0x22, 0x98, 0xbb, 0xbb, 0x89, 0x32, 0x75, 0x5d, 0x42, 0xb9, 0xe4, 0xbe, 0x9b, 0x34, 0x75,
    0x57, 0x44, 0xb9, 0xee, 0xee, 0x9b, 0x54, 0xdd, 0x7b, 0x45, 0xb9, 0xee, 0xee, 0x9b, 0xd8, 0xc7,
    0xac, 0x89, 0xb8, 0xeb, 0xbe, 0x8b, 0xa8, 0xec, 0xce, 0xbc, 0xac, 0xbb, 0xbb, 0xac, 0xcc, 0xee,
    0xee, 0xce, 0xcc, 0xec, 0xee, 0xec, 0xee, 0xee, 0xee, 0xee, 0xee, 0xee, 0xee, 0xee, 0xee, 0xee,
];

/// Number of save slots.
pub const SAVE_SLOT_COUNT: usize = 8;
/// Bytes of room-BGM state at the end of the layout.
pub const ROOM_BGM_LEN: usize = crate::game::ROOM_BGM_LEN;
/// Player inventory slots in the block (Chris uses 6, Jill 8).
pub const PLAYER_SLOT_COUNT: usize = 12;
/// Longest file name a save-screen action accepts, in bytes.
pub const MAX_FILE_NAME: usize = 255;
/// Number of autosave files kept in rotation.
pub const AUTOSAVE_COUNT: usize = 2;

mod file;
pub use file::SaveFile;

impl SaveFile {
    /// Mirror every field the port owns into [`SaveFile::state_bytes`], so the
    /// captured image describes exactly what [`SaveFile::to_bytes`] writes and
    /// a reload followed by a re-capture is identical. The bytes with no
    /// explicit field (0x208..0x20F, 0x214..0x223) are left untouched.
    fn sync_state_image(&mut self) {
        // The state image keeps the port's 1-based stage digit; the block's
        // own 0x200 byte (`self.stage`) is the original's 0-based g_stageId.
        self.state_bytes[0] = self.room_id().stage;
        self.state_bytes[1] = self.room;
        self.state_bytes[usize::from(STATE_BYTE_ROOM_CAMERA)] = self.camera;
        self.state_bytes[usize::from(STATE_BYTE_CUT)] = self.cut;
        self.state_bytes[usize::from(STATE_BYTE_MENU_CHOICE)] = self.menu_choice;
        self.state_bytes[usize::from(STATE_BYTE_SELECTED_ITEM)] = self.selected_item;
        self.state_bytes[usize::from(STATE_BYTE_TOTAL_HELD)] = self.total_held;
        self.state_bytes[usize::from(STATE_BYTE_FWD_ACTION)] = self.fwd_action;
        self.state_bytes[usize::from(STATE_BYTE_ENT_ACTION)] = self.ent_action;
        self.state_bytes[usize::from(STATE_BYTE_USED_ITEM)] = self.used_item;
        self.state_bytes[usize::from(STATE_BYTE_PICKED_ITEM)] = self.picked_item;
        put_i16(&mut self.state_bytes, 0x1E, self.health);
        put_u32(&mut self.state_bytes, 0x24, self.timer);
        self.state_bytes[usize::from(STATE_BYTE_SAVES)] = self.saves;
        self.state_bytes[usize::from(STATE_BYTE_EQUIPPED)] = self.equipped;
        self.state_bytes[0x2A] = self.room_item_backup;
        self.state_bytes[usize::from(STATE_BYTE_CHARACTER)] = self.character;
        put_i16(&mut self.state_bytes, 0x2C, self.pos_x);
        put_i16(&mut self.state_bytes, 0x2E, self.pos_z);
        put_i16(&mut self.state_bytes, 0x30, self.angle);
        self.state_bytes[usize::from(STATE_BYTE_HEALTH_STATUS)] = self.health_status;
        // BioCard 0x234 opens the 32-byte scenario bank 2; its first 12 bytes
        // fall inside the 0x200..0x240 image.
        self.state_bytes[0x34..].copy_from_slice(&self.scenario2[..STATE_BYTES - 0x34]);
    }

    /// Capture a game state. The prefix is the shipped BioCard header
    /// [`BIOCARD_PREFIX`], so a fresh save stays byte-identical without any
    /// original card asset.
    ///
    /// The whole BioCard state image is captured: the explicit fields mirror
    /// the parts the port owns and [`SaveFile::state_bytes`] preserves the
    /// rest, including BioCard 0x208..0x20F (special-room light, character
    /// model id, last enemy flags, bullet effect id, pickup quantities A/B/C)
    /// and the 0x214..0x223 words (fade state, light, rand seed, countdown,
    /// health copy, pad), so the quantities opcode 0x4C moves survive a reload.
    pub fn from_state(state: &GameState) -> Self {
        // BioCard 0x224 is the play-timer snapshot the original mirrors every
        // frame; it lives in the state byte image.
        let timer = u32::from_le_bytes(
            state.state_bytes[0x24..0x28]
                .try_into()
                .expect("timer bytes"),
        );
        let mut file = Self {
            prefix: BIOCARD_PREFIX,
            state_bytes: state.state_bytes,
            // The block stores the original's 0-based `g_stageId`, whose
            // stages 6 and 7 are the second mansion visit; the port's
            // `RoomId.stage` is the 1-based RDT digit.
            stage: state.id.table_row() as u8,
            room: state.id.room,
            camera: state.camera.current_cut.min(0xFF) as u8,
            cut: state.state_bytes[usize::from(STATE_BYTE_CUT)],
            menu_choice: state.state_bytes[usize::from(STATE_BYTE_MENU_CHOICE)],
            selected_item: state.selected_item.unwrap_or(0),
            total_held: state.inventory.len().min(PLAYER_SLOT_COUNT) as u8,
            fwd_action: state.state_bytes[usize::from(STATE_BYTE_FWD_ACTION)],
            ent_action: state.state_bytes[usize::from(STATE_BYTE_ENT_ACTION)],
            used_item: state.last_used_item.unwrap_or(0),
            picked_item: state.last_picked_item.unwrap_or(0),
            health: state.entities[0].health,
            timer,
            saves: state.state_bytes[usize::from(STATE_BYTE_SAVES)],
            equipped: state.equipped.unwrap_or(0),
            room_item_backup: state.state_bytes[0x2A],
            character: state.state_bytes[usize::from(STATE_BYTE_CHARACTER)],
            pos_x: state.entities[0].pos[0] as i16,
            pos_z: state.entities[0].pos[2] as i16,
            pos_y: state.entities[0].pos[1] as i16,
            angle: state.entities[0].angle as i16,
            health_status: state.health_status,
            examined: state.examined_flags(),
            item_box: state.item_box,
            room_bgm: state.room_bgm,
            ..Self::default()
        };
        file.scenario.copy_from_slice(&state.flags[0].bytes()[..16]);
        file.scenario2.copy_from_slice(state.flags[1].bytes());
        file.locks.copy_from_slice(&state.flags[2].bytes()[..8]);
        file.enemies.copy_from_slice(state.flags[3].bytes());
        file.room_items.copy_from_slice(state.flags[7].bytes());
        file.room_flags
            .copy_from_slice(&state.flags[8].bytes()[..20]);
        // The 12 slot bytes are one contiguous run: the player's slots first,
        // then Rebecca's. Rebecca's block goes down first so a Jill inventory
        // (up to eight slots) overwrites the two slots it shares with her.
        for (slot, stack) in file.player_slots[6..]
            .iter_mut()
            .zip(state.rebecca_inventory.iter())
        {
            *slot = *stack;
        }
        for (slot, stack) in file.player_slots.iter_mut().zip(state.inventory.iter()) {
            *slot = *stack;
        }
        file.sync_state_image();
        file
    }

    /// Capture a state with a caller-supplied bio-card prefix override.
    pub fn from_state_with_prefix(state: &GameState, prefix: &[u8]) -> Result<Self> {
        let mut file = Self::from_state(state);
        file.set_prefix(prefix)?;
        Ok(file)
    }

    /// Copy the 0x200-byte bio-card prefix.
    pub fn set_prefix(&mut self, data: &[u8]) -> Result<()> {
        if data.len() < PREFIX_LEN {
            bail!(
                "bio-card prefix is {} bytes; {PREFIX_LEN} are required",
                data.len()
            );
        }
        self.prefix.copy_from_slice(&data[..PREFIX_LEN]);
        Ok(())
    }

    /// Apply the parsed block to a game state.
    ///
    /// The saved flags replace the ten banks (transient banks 4/5/6/9 are
    /// cleared), the inventory and item box are rebuilt, and the BioCard state
    /// bytes are reseeded from the block.
    pub fn apply_to(&self, state: &mut GameState) {
        // The block's stage byte is the original's 0-based `g_stageId`; the
        // port's `RoomId.stage` is the 1-based RDT digit.
        state.id = self.room_id();
        state.flags = [FlagBank::new(); FLAG_BANK_COUNT];
        state.flags[0].bytes_mut()[..16].copy_from_slice(&self.scenario);
        state.flags[1].bytes_mut().copy_from_slice(&self.scenario2);
        state.flags[2].bytes_mut()[..8].copy_from_slice(&self.locks);
        state.flags[3].bytes_mut().copy_from_slice(&self.enemies);
        state.flags[7].bytes_mut().copy_from_slice(&self.room_items);
        state.flags[8].bytes_mut()[..20].copy_from_slice(&self.room_flags);

        state.camera = CameraState {
            current_cut: usize::from(self.camera),
            saved_cut: None,
            locked: false,
        };
        // `InitializeGame` writes the block's selected character into the
        // player entity's id on the continue path too (`g_playerEntity.id =
        // g_SelectedCharactedId`, GameStart.cpp), exactly as the new-game and
        // arbitrary-room boots seed it in `seed_start_health`. Room scripts
        // read that byte (`g_playerEntity.id & 3` is Jill), so a load that
        // left it zero ran the other character's item and spawn branches.
        state.entities[0].id = self.character & 1;
        state.entities[0].health = self.health;
        state.entities[0].pos = [
            i32::from(self.pos_x),
            i32::from(self.pos_y),
            i32::from(self.pos_z),
        ];
        state.entities[0].angle = self.angle as u16 & 0x0FFF;
        // The maximum is not stored in the block; InitializeGame re-derives it
        // from the selected character on every path, new game or continue.
        state.max_health = character_max_health(self.character);
        state.health_status = self.health_status;
        state.message.examined = self.examined;
        state.selected_item = (self.selected_item != 0).then_some(self.selected_item);
        state.last_used_item = (self.used_item != 0).then_some(self.used_item);
        state.last_picked_item = (self.picked_item != 0).then_some(self.picked_item);
        state.equipped = (self.equipped != 0).then_some(self.equipped);
        state.equipped_weapon_id = self.equipped;
        state.inventory = self
            .player_slots
            .iter()
            .take(usize::from(self.total_held).min(PLAYER_SLOT_COUNT))
            .copied()
            .filter(|stack| stack.id != 0)
            .collect();
        state.item_box = self.item_box;
        state
            .rebecca_inventory
            .copy_from_slice(&self.player_slots[6..12]);
        // The per-room BGM table round-trips through the block; the live state
        // byte is not saved, so it resets to the nothing-playing value and the
        // destination room's entry rebuilds the channels on load.
        state.room_bgm = self.room_bgm;
        state.bgm = BgmState::default();
        state.voice = VoiceState::default();

        // The saved image comes first; the explicit fields below restate the
        // bytes the port owns, exactly like the original's prefix-then-fields
        // memcpy. The state image's stage byte is the port's own 1-based digit.
        state.state_bytes = self.state_bytes;
        state.state_bytes[0] = state.id.stage;
        state.state_bytes[1] = self.room;
        state.state_bytes[usize::from(STATE_BYTE_ROOM_CAMERA)] = self.camera;
        state.state_bytes[usize::from(STATE_BYTE_CUT)] = self.cut;
        state.state_bytes[usize::from(STATE_BYTE_MENU_CHOICE)] = self.menu_choice;
        state.state_bytes[usize::from(STATE_BYTE_SELECTED_ITEM)] = self.selected_item;
        state.state_bytes[usize::from(STATE_BYTE_TOTAL_HELD)] = self.total_held;
        state.state_bytes[usize::from(STATE_BYTE_FWD_ACTION)] = self.fwd_action;
        state.state_bytes[usize::from(STATE_BYTE_ENT_ACTION)] = self.ent_action;
        state.state_bytes[usize::from(STATE_BYTE_USED_ITEM)] = self.used_item;
        state.state_bytes[usize::from(STATE_BYTE_PICKED_ITEM)] = self.picked_item;
        put_i16(&mut state.state_bytes, 0x1E, self.health);
        put_u32(&mut state.state_bytes, 0x24, self.timer);
        state.state_bytes[usize::from(STATE_BYTE_SAVES)] = self.saves;
        state.state_bytes[usize::from(STATE_BYTE_EQUIPPED)] = self.equipped;
        state.state_bytes[0x2A] = self.room_item_backup;
        state.state_bytes[usize::from(STATE_BYTE_CHARACTER)] = self.character;
        put_i16(&mut state.state_bytes, 0x2C, self.pos_x);
        put_i16(&mut state.state_bytes, 0x2E, self.pos_z);
        put_i16(&mut state.state_bytes, 0x30, self.angle);
        state.state_bytes[usize::from(STATE_BYTE_HEALTH_STATUS)] = self.health_status;
    }

    /// The bytes a save file holds: the block, then the height trailer when
    /// the player is off floor level ([`HEIGHT_TRAILER`]).
    pub fn to_file_bytes(&self) -> Vec<u8> {
        let mut bytes = self.to_bytes().to_vec();
        if self.pos_y != 0 {
            bytes.extend_from_slice(&HEIGHT_TRAILER);
            bytes.extend_from_slice(&self.pos_y.to_le_bytes());
            bytes.extend_from_slice(&[0, 0]);
        }
        bytes
    }

    /// Serialize the block: 0x800 bytes, little-endian, zero-padded. The
    /// port-only height is not part of it ([`Self::to_file_bytes`]).
    pub fn to_bytes(&self) -> [u8; SAVE_BLOCK_SIZE] {
        let mut bytes = [0u8; SAVE_BLOCK_SIZE];
        bytes[..PREFIX_LEN].copy_from_slice(&self.prefix);
        // The whole state image goes down first; the explicit field writes
        // below restate the bytes the port owns and the rest survives verbatim.
        bytes[STATE_IMAGE_OFFSET..STATE_IMAGE_OFFSET + STATE_BYTES]
            .copy_from_slice(&self.state_bytes);
        bytes[0x200] = self.stage;
        bytes[0x201] = self.room;
        bytes[0x202] = self.camera;
        bytes[0x204] = self.cut;
        bytes[0x205] = self.menu_choice;
        bytes[0x206] = self.selected_item;
        bytes[0x207] = self.total_held;
        bytes[0x210] = self.fwd_action;
        bytes[0x211] = self.ent_action;
        bytes[0x212] = self.used_item;
        bytes[0x213] = self.picked_item;
        put_i16(&mut bytes, 0x21E, self.health);
        put_u32(&mut bytes, 0x224, self.timer);
        bytes[0x228] = self.saves;
        bytes[0x229] = self.equipped;
        bytes[0x22A] = self.room_item_backup;
        bytes[0x22B] = self.character;
        put_i16(&mut bytes, 0x22C, self.pos_x);
        put_i16(&mut bytes, 0x22E, self.pos_z);
        put_i16(&mut bytes, 0x230, self.angle);
        bytes[0x232] = self.health_status;
        bytes[0x234..0x254].copy_from_slice(&self.scenario2);
        bytes[0x254..0x25C].copy_from_slice(&self.locks);
        bytes[0x25C..0x27C].copy_from_slice(&self.enemies);
        bytes[0x27C..0x29C].copy_from_slice(&self.room_items);
        bytes[0x29C..0x2A0].copy_from_slice(&self.examined);
        bytes[0x2A0..0x2B0].copy_from_slice(&self.scenario);
        bytes[0x2B0..0x2C4].copy_from_slice(&self.room_flags);
        for (index, slot) in self.item_box.iter().enumerate() {
            bytes[0x2C4 + index * 2] = slot.id;
            bytes[0x2C4 + index * 2 + 1] = slot.quantity;
        }
        for (index, slot) in self.player_slots.iter().enumerate() {
            bytes[0x324 + index * 2] = slot.id;
            bytes[0x324 + index * 2 + 1] = slot.quantity;
        }
        bytes[0x33C..SAVE_LAYOUT_SIZE].copy_from_slice(&self.room_bgm);
        bytes
    }

    /// Parse a save block. The layout needs [`SAVE_LAYOUT_SIZE`] bytes; the
    /// full [`SAVE_BLOCK_SIZE`] block is the usual input and
    /// [`budget::MAX_SAVE_BYTES`] is the largest accepted.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < SAVE_LAYOUT_SIZE {
            bail!(
                "save block is {} bytes; at least {SAVE_LAYOUT_SIZE} are required",
                bytes.len()
            );
        }
        budget::check_len(bytes.len(), budget::MAX_SAVE_BYTES, "save block size")?;
        let mut file = Self {
            prefix: bytes[..PREFIX_LEN].try_into().expect("prefix bytes"),
            state_bytes: bytes[STATE_IMAGE_OFFSET..STATE_IMAGE_OFFSET + STATE_BYTES]
                .try_into()
                .expect("state bytes"),
            stage: bytes[0x200],
            room: bytes[0x201],
            camera: bytes[0x202],
            cut: bytes[0x204],
            menu_choice: bytes[0x205],
            selected_item: bytes[0x206],
            total_held: bytes[0x207],
            fwd_action: bytes[0x210],
            ent_action: bytes[0x211],
            used_item: bytes[0x212],
            picked_item: bytes[0x213],
            health: get_i16(bytes, 0x21E),
            timer: get_u32(bytes, 0x224),
            saves: bytes[0x228],
            equipped: bytes[0x229],
            room_item_backup: bytes[0x22A],
            character: bytes[0x22B],
            pos_x: get_i16(bytes, 0x22C),
            pos_z: get_i16(bytes, 0x22E),
            pos_y: height_trailer(bytes).unwrap_or(0),
            angle: get_i16(bytes, 0x230),
            health_status: bytes[0x232],
            ..Self::default()
        };
        file.scenario2.copy_from_slice(&bytes[0x234..0x254]);
        file.locks.copy_from_slice(&bytes[0x254..0x25C]);
        file.enemies.copy_from_slice(&bytes[0x25C..0x27C]);
        file.room_items.copy_from_slice(&bytes[0x27C..0x29C]);
        file.examined.copy_from_slice(&bytes[0x29C..0x2A0]);
        file.scenario.copy_from_slice(&bytes[0x2A0..0x2B0]);
        file.room_flags.copy_from_slice(&bytes[0x2B0..0x2C4]);
        for (index, slot) in file.item_box.iter_mut().enumerate() {
            slot.id = bytes[0x2C4 + index * 2];
            slot.quantity = bytes[0x2C4 + index * 2 + 1];
        }
        for (index, slot) in file.player_slots.iter_mut().enumerate() {
            slot.id = bytes[0x324 + index * 2];
            slot.quantity = bytes[0x324 + index * 2 + 1];
        }
        file.room_bgm
            .copy_from_slice(&bytes[0x33C..SAVE_LAYOUT_SIZE]);
        // The state image keeps the port's 1-based stage digit; the block's
        // own 0x200 byte read above is the original's 0-based `g_stageId`.
        file.state_bytes[0] = file.room_id().stage;
        Ok(file)
    }
}

/// The height a file's trailer carries ([`HEIGHT_TRAILER`]), if it has one.
fn height_trailer(bytes: &[u8]) -> Option<i16> {
    let trailer = bytes.get(SAVE_BLOCK_SIZE..)?;
    (trailer.len() == HEIGHT_TRAILER_LEN && trailer[..4] == HEIGHT_TRAILER)
        .then(|| i16::from_le_bytes([trailer[4], trailer[5]]))
}

/// The block's 1-based file name for a zero-based slot index.
pub fn slot_name(index: usize) -> Option<String> {
    (index < SAVE_SLOT_COUNT).then(|| format!("savedat{}.dat", index + 1))
}

/// The path of slot `index` in `dir`.
pub fn slot_path(dir: &Path, index: usize) -> Option<PathBuf> {
    slot_name(index).map(|name| dir.join(name))
}

/// The file name of autosave `index`: `1` is the newest, up to
/// [`AUTOSAVE_COUNT`].
pub fn autosave_name(index: usize) -> Option<String> {
    (1..=AUTOSAVE_COUNT)
        .contains(&index)
        .then(|| format!("autosave{index}.dat"))
}

/// The rotation index (`1` newest) of an autosave file name, compared
/// case-insensitively like the folder scan, or `None` for any other file.
pub fn autosave_index(name: &str) -> Option<usize> {
    (1..=AUTOSAVE_COUNT).find(|&index| {
        autosave_name(index).is_some_and(|autosave| autosave.eq_ignore_ascii_case(name))
    })
}

/// Write `file` as the newest autosave under `dir`, creating the directory.
///
/// The rotation keeps [`AUTOSAVE_COUNT`] files: the new block is first
/// written and synced to a sibling temporary file, then each older autosave
/// moves one place down (`autosave1.dat` becomes `autosave2.dat`, replacing
/// it) and the temporary file is renamed to `autosave1.dat`. A failed write
/// leaves the existing autosaves untouched, and no step leaves a partial save
/// under an autosave name.
pub fn write_autosave(dir: &Path, file: &SaveFile) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;
    let newest = dir.join(autosave_name(1).expect("in range"));
    let temp = crate::atomic::temp_path(&newest)?;
    let rotate = || -> Result<()> {
        crate::atomic::write(&temp, &file.to_file_bytes())?;
        for index in (1..AUTOSAVE_COUNT).rev() {
            let from = dir.join(autosave_name(index).expect("in range"));
            let to = dir.join(autosave_name(index + 1).expect("in range"));
            match fs::rename(&from, &to) {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => {
                    return Err(err).with_context(|| {
                        format!("failed to move {} to {}", from.display(), to.display())
                    });
                }
            }
        }
        fs::rename(&temp, &newest).with_context(|| {
            format!(
                "failed to move {} into place at {}",
                temp.display(),
                newest.display()
            )
        })
    };
    let result = rotate();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

/// Writes autosaves on a background thread, in the order they are queued, so
/// the disk write (and its sync) never stalls a game tick.
///
/// The thread starts with the first autosave, so a run that never autosaves
/// never spawns it. Dropping the writer waits for the queued writes to finish.
#[derive(Default)]
pub struct AutosaveWriter {
    worker: Option<(
        std::sync::mpsc::Sender<AutosaveJob>,
        std::thread::JoinHandle<()>,
    )>,
}

/// One queued autosave: the save directory and the block.
type AutosaveJob = (PathBuf, Box<SaveFile>);

impl AutosaveWriter {
    /// Queue `file` as the newest autosave under `dir`. A failed write is
    /// reported on stderr.
    pub fn write(&mut self, dir: &Path, file: SaveFile) {
        let job = (dir.to_path_buf(), Box::new(file));
        let job = match &self.worker {
            Some((sender, _)) => match sender.send(job) {
                Ok(()) => return,
                Err(std::sync::mpsc::SendError(job)) => job,
            },
            None => job,
        };
        // No worker yet, or it is gone: start one.
        let (sender, receiver) = std::sync::mpsc::channel::<AutosaveJob>();
        let spawned = std::thread::Builder::new()
            .name("autosave".to_string())
            .spawn(move || {
                for (dir, file) in receiver {
                    if let Err(err) = write_autosave(&dir, &file) {
                        eprintln!("warning: autosave failed: {err:#}");
                    }
                }
            });
        match spawned {
            Ok(handle) => {
                let _ = sender.send(job);
                self.worker = Some((sender, handle));
            }
            Err(err) => {
                eprintln!("warning: autosave thread unavailable ({err}); writing inline");
                if let Err(err) = write_autosave(&job.0, &job.1) {
                    eprintln!("warning: autosave failed: {err:#}");
                }
            }
        }
    }

    /// Wait for every queued autosave to be written.
    pub fn flush(&mut self) {
        if let Some((sender, handle)) = self.worker.take() {
            drop(sender);
            let _ = handle.join();
        }
    }
}

impl Drop for AutosaveWriter {
    fn drop(&mut self) {
        self.flush();
    }
}

/// The default save directory: `<config dir>/save` (`~/.config/arklay/save`
/// on Linux, `%APPDATA%\arklay\save` on Windows).
pub fn default_save_dir() -> PathBuf {
    crate::config::Config::default_save_dir()
}

/// Header fields the save/load screen shows on a slot row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SaveSlotInfo {
    /// Character id (`0` Chris, `1` Jill).
    pub character: u8,
    /// Save counter.
    pub saves: u8,
    /// The 1-based stage of the room the block was saved in.
    pub stage: u8,
    /// Room id.
    pub room: u8,
    /// Whether the room was on its second mansion visit.
    pub mansion2: bool,
}

impl SaveSlotInfo {
    /// Read the header fields out of a parsed block.
    pub fn from_file(file: &SaveFile) -> Self {
        let id = file.room_id();
        Self {
            character: file.character,
            saves: file.saves,
            stage: id.stage,
            room: file.room,
            mansion2: id.mansion2,
        }
    }
}

impl SaveFile {
    /// The room the block was saved in: its 0-based `g_stageId` is the
    /// original's, whose stages 6 and 7 are the second mansion visit.
    pub fn room_id(&self) -> RoomId {
        RoomId {
            player_flag: self.character & 1,
            ..RoomId::from_table_row(usize::from(self.stage), self.room)
        }
    }
}

/// One file in the save directory, as the folder-based screen sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct SaveEntry {
    /// The file's path.
    pub path: PathBuf,
    /// The file's name with its extension.
    pub name: String,
    /// The file's name without its extension, the screen's row label.
    pub base: String,
    /// Modification time in Unix seconds, with the filesystem's sub-second
    /// part, or `0` when unavailable.
    pub timestamp: f64,
    /// The parsed header, when the file is a loadable save block.
    pub info: Option<SaveSlotInfo>,
    /// The autosave rotation index (`1` newest) when the file is one of the
    /// autosaves ([`autosave_index`]).
    pub autosave: Option<usize>,
}

impl SaveEntry {
    /// Whether the file parses as a save block.
    pub fn loadable(&self) -> bool {
        self.info.is_some()
    }
}

/// Scan every regular file in `dir`, sorted by lowercased file name.
///
/// A missing directory is an empty list. Each file is read and parsed; a file
/// that is not a save block (or is over [`budget::MAX_SAVE_BYTES`]) is still
/// listed, with `info` `None`, so the screen can show it as unloadable.
pub fn scan_dir(dir: &Path) -> Vec<SaveEntry> {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut entries: Vec<SaveEntry> = read_dir
        .flatten()
        // `fs::metadata` (unlike `DirEntry::file_type`) follows symlinks, so a
        // linked save file lists and loads like the vanilla slot reader's.
        .filter(|entry| fs::metadata(entry.path()).is_ok_and(|meta| meta.is_file()))
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_str()?.to_string();
            let base = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or_default()
                .to_string();
            Some(SaveEntry {
                autosave: autosave_index(&name),
                path,
                name,
                base,
                timestamp: timestamp(&entry),
                info: None,
            })
        })
        .collect();
    entries.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
            .then_with(|| a.name.cmp(&b.name))
    });
    for entry in &mut entries {
        entry.info = read_slot_info_path(&entry.path);
    }
    entries
}

/// Read a save block's header from an explicit path, or `None` when the file
/// does not parse.
fn read_slot_info_path(path: &Path) -> Option<SaveSlotInfo> {
    let bytes = read_slot_bytes(path).ok()?;
    let file = SaveFile::from_bytes(&bytes).ok()?;
    Some(SaveSlotInfo::from_file(&file))
}

/// A directory entry's modification time in Unix seconds, with the
/// filesystem's sub-second part, or `0`.
fn timestamp(entry: &fs::DirEntry) -> f64 {
    entry
        .metadata()
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0.0, |since| since.as_secs_f64())
}

/// Resolve a screen-supplied file name under `dir`.
///
/// The name must be one plain file name — no directory part, no `.`/`..`, not
/// empty, at most [`MAX_FILE_NAME`] bytes — so a script cannot escape the save
/// directory. The extension is the caller's choice.
pub fn file_path(dir: &Path, name: &str) -> Option<PathBuf> {
    if name.is_empty() || name.len() > MAX_FILE_NAME || name.contains('\0') {
        return None;
    }
    let path = Path::new(name);
    if path.components().count() != 1
        || !matches!(
            path.components().next(),
            Some(std::path::Component::Normal(_))
        )
    {
        return None;
    }
    Some(dir.join(path))
}

/// Write `file` to an explicit path, creating the parent directory.
pub fn save_to_path(path: &Path, file: &SaveFile) -> Result<()> {
    if let Some(dir) = path.parent()
        && !dir.as_os_str().is_empty()
    {
        fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;
    }
    fs::write(path, file.to_file_bytes())
        .with_context(|| format!("failed to write {}", path.display()))
}

/// Remove a save file. A missing file is not an error.
pub fn remove_file(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).with_context(|| format!("failed to remove {}", path.display())),
    }
}

/// Read a slot file after checking its size against the save cap.
fn read_slot_bytes(path: &Path) -> Result<Vec<u8>> {
    let metadata =
        fs::metadata(path).with_context(|| format!("failed to stat {}", path.display()))?;
    budget::check_len_u64(
        metadata.len(),
        budget::MAX_SAVE_BYTES as u64,
        "save slot file size",
    )?;
    fs::read(path).with_context(|| format!("failed to read {}", path.display()))
}

/// Read one slot's header, or `None` when the file is missing or too short to
/// be a save.
pub fn read_slot_info(dir: &Path, index: usize) -> Option<SaveSlotInfo> {
    let path = slot_path(dir, index)?;
    let bytes = read_slot_bytes(&path).ok()?;
    let file = SaveFile::from_bytes(&bytes).ok()?;
    Some(SaveSlotInfo::from_file(&file))
}

/// Scan all eight slots for the load screen.
pub fn scan_slots(dir: &Path) -> [Option<SaveSlotInfo>; SAVE_SLOT_COUNT] {
    std::array::from_fn(|index| read_slot_info(dir, index))
}

/// Write `file` to slot `index` under `dir`, creating the directory.
pub fn save(dir: &Path, index: usize, file: &SaveFile) -> Result<()> {
    let path =
        slot_path(dir, index).with_context(|| format!("save slot {index} is out of range"))?;
    fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;
    fs::write(&path, file.to_file_bytes())
        .with_context(|| format!("failed to write {}", path.display()))
}

/// Read slot `index` from `dir`.
pub fn load(dir: &Path, index: usize) -> Result<SaveFile> {
    let path =
        slot_path(dir, index).with_context(|| format!("save slot {index} is out of range"))?;
    let bytes = read_slot_bytes(&path)?;
    SaveFile::from_bytes(&bytes).with_context(|| format!("failed to parse {}", path.display()))
}

/// Read a save block from an explicit file path.
///
/// Unlike [`load`], the file need not be a `savedatN.dat` under a save
/// directory: the CLI `--load`/`--test-load` oracle seam takes any path, so a
/// scenario save checked into the repository loads without copying it into a
/// save slot.
pub fn load_file(path: &Path) -> Result<SaveFile> {
    let bytes = read_slot_bytes(path)?;
    SaveFile::from_bytes(&bytes).with_context(|| format!("failed to parse {}", path.display()))
}

fn put_i16(buf: &mut [u8], offset: usize, value: i16) {
    buf[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(buf: &mut [u8], offset: usize, value: u32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn get_i16(buf: &[u8], offset: usize) -> i16 {
    i16::from_le_bytes([buf[offset], buf[offset + 1]])
}

fn get_u32(buf: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        buf[offset],
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
    ])
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::state::RoomState;

    /// Self-deleting temporary directory unique to this process and label.
    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("arklay-save-{}-{label}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn filled() -> SaveFile {
        let mut file = SaveFile::default();
        for (index, byte) in file.prefix.iter_mut().enumerate() {
            *byte = index as u8;
        }
        file.stage = 0x12;
        file.room = 0x34;
        file.camera = 0x56;
        file.cut = 0x78;
        file.menu_choice = 0x9A;
        file.selected_item = 0xBC;
        file.total_held = 0xDE;
        file.fwd_action = 0x11;
        file.ent_action = 0x22;
        file.used_item = 0x33;
        file.picked_item = 0x44;
        file.health = 0x1234;
        file.timer = 0x89AB_CDEF;
        file.saves = 0x5A;
        file.equipped = 0x6B;
        file.room_item_backup = 0x7C;
        file.character = 0x01;
        file.pos_x = 0x0102;
        file.pos_z = 0x0304;
        file.angle = 0x0506;
        file.health_status = 0x08;
        file.scenario2 = [0xA1; 32];
        file.locks = [0xB2; 8];
        file.enemies = [0xC3; 32];
        file.room_items = [0xD4; 32];
        file.examined = [0xE5; 4];
        file.scenario = [0xF6; 16];
        file.room_flags = [0x17; 20];
        file.item_box[0] = InventoryItem {
            id: 0x0B,
            quantity: 0x0F,
        };
        file.item_box[47] = InventoryItem {
            id: 0x42,
            quantity: 1,
        };
        file.player_slots[0] = InventoryItem {
            id: 0x2F,
            quantity: 3,
        };
        file.player_slots[11] = InventoryItem {
            id: 0x41,
            quantity: 1,
        };
        for (index, byte) in file.room_bgm.iter_mut().enumerate() {
            *byte = (index as u8).wrapping_mul(3);
        }
        file.sync_state_image();
        file
    }

    #[test]
    fn every_field_lands_at_its_table_offset() {
        let file = filled();
        let bytes = file.to_bytes();

        assert_eq!(&bytes[..PREFIX_LEN], &file.prefix);
        assert_eq!(bytes[0x200], 0x12);
        assert_eq!(bytes[0x201], 0x34);
        assert_eq!(bytes[0x202], 0x56);
        assert_eq!(bytes[0x203], 0x00, "the attract camera stays zeroed");
        assert_eq!(bytes[0x204], 0x78);
        assert_eq!(bytes[0x205], 0x9A);
        assert_eq!(bytes[0x206], 0xBC);
        assert_eq!(bytes[0x207], 0xDE);
        assert_eq!(bytes[0x210], 0x11);
        assert_eq!(bytes[0x211], 0x22);
        assert_eq!(bytes[0x212], 0x33);
        assert_eq!(bytes[0x213], 0x44);
        assert_eq!(&bytes[0x21E..0x220], &0x1234i16.to_le_bytes());
        assert_eq!(&bytes[0x224..0x228], &0x89AB_CDEFu32.to_le_bytes());
        assert_eq!(bytes[0x228], 0x5A);
        assert_eq!(bytes[0x229], 0x6B);
        assert_eq!(bytes[0x22A], 0x7C);
        assert_eq!(bytes[0x22B], 0x01);
        assert_eq!(&bytes[0x22C..0x22E], &0x0102i16.to_le_bytes());
        assert_eq!(&bytes[0x22E..0x230], &0x0304i16.to_le_bytes());
        assert_eq!(&bytes[0x230..0x232], &0x0506i16.to_le_bytes());
        assert_eq!(bytes[0x232], 0x08);
        assert_eq!(&bytes[0x234..0x254], &[0xA1; 32]);
        assert_eq!(&bytes[0x254..0x25C], &[0xB2; 8]);
        assert_eq!(&bytes[0x25C..0x27C], &[0xC3; 32]);
        assert_eq!(&bytes[0x27C..0x29C], &[0xD4; 32]);
        assert_eq!(&bytes[0x29C..0x2A0], &[0xE5; 4]);
        assert_eq!(&bytes[0x2A0..0x2B0], &[0xF6; 16]);
        assert_eq!(&bytes[0x2B0..0x2C4], &[0x17; 20]);
        assert_eq!(bytes[0x2C4], 0x0B);
        assert_eq!(bytes[0x2C5], 0x0F);
        assert_eq!(bytes[0x2C4 + 47 * 2], 0x42);
        assert_eq!(bytes[0x2C4 + 47 * 2 + 1], 0x01);
        assert_eq!(bytes[0x324], 0x2F);
        assert_eq!(bytes[0x325], 3);
        assert_eq!(bytes[0x324 + 11 * 2], 0x41);
        assert_eq!(&bytes[0x33C..SAVE_LAYOUT_SIZE], &file.room_bgm);
        assert!(
            bytes[SAVE_LAYOUT_SIZE..].iter().all(|&byte| byte == 0),
            "the tail must stay zero-padded"
        );
    }

    #[test]
    fn bytes_round_trip() {
        let file = filled();
        assert_eq!(SaveFile::from_bytes(&file.to_bytes()).unwrap(), file);
    }

    #[test]
    fn a_height_rides_in_the_trailer() {
        let file = SaveFile {
            pos_y: -2885,
            ..filled()
        };
        let bytes = file.to_file_bytes();
        assert_eq!(bytes.len(), SAVE_BLOCK_SIZE + HEIGHT_TRAILER_LEN);
        assert_eq!(SaveFile::from_bytes(&bytes).unwrap(), file);
        // The bare block drops only the port-only height.
        let block = SaveFile::from_bytes(&file.to_bytes()).unwrap();
        assert_eq!(block.pos_y, 0);
        assert_eq!(SaveFile { pos_y: 0, ..file }, block);
    }

    #[test]
    fn a_floor_level_save_is_the_plain_block() {
        let file = filled();
        assert_eq!(file.to_file_bytes(), file.to_bytes().to_vec());
    }

    #[test]
    fn an_original_file_never_reads_as_a_height() {
        // The original's 0xA82-byte file keeps the pad remap at 0x41C and
        // the key bindings past 0x800; neither is a height.
        let mut bytes = vec![0u8; 0xA82];
        bytes[..SAVE_BLOCK_SIZE].copy_from_slice(&filled().to_bytes());
        bytes[0x41C..0x420].copy_from_slice(&[0x00, 0x10, 0x00, 0x20]);
        bytes[SAVE_BLOCK_SIZE..SAVE_BLOCK_SIZE + 4].copy_from_slice(&HEIGHT_TRAILER);
        bytes[SAVE_BLOCK_SIZE + 4] = 0x45;
        assert_eq!(SaveFile::from_bytes(&bytes).unwrap().pos_y, 0);
    }

    #[test]
    fn rebecca_slots_round_trip_through_the_state() {
        let mut state = GameState::new(RoomId::parse("1060").unwrap(), &RoomState::default());
        state.rebecca_inventory[0] = InventoryItem {
            id: 0x02,
            quantity: 15,
        };
        let file = SaveFile::from_state(&state);
        assert_eq!(file.player_slots[6].id, 0x02);
        assert_eq!(file.player_slots[6].quantity, 15);

        let mut restored = GameState::default();
        file.apply_to(&mut restored);
        assert_eq!(restored.rebecca_inventory[0].id, 0x02);
        assert_eq!(restored.rebecca_inventory[0].quantity, 15);
    }

    #[test]
    fn state_image_offsets_match_the_bio_card_layout() {
        assert_eq!(STATE_IMAGE_OFFSET, 0x200);
        assert_eq!(STATE_IMAGE_OFFSET + 0x0C, 0x20C, "pickup quantity A");
        assert_eq!(STATE_IMAGE_OFFSET + 0x0E, 0x20E, "pickup quantity C");
        assert_eq!(STATE_IMAGE_OFFSET + 0x14, 0x214, "first dropped word");
        assert_eq!(STATE_IMAGE_OFFSET + 0x23, 0x223, "last dropped word");
        assert_eq!(STATE_IMAGE_OFFSET + STATE_BYTES, 0x240);
    }

    #[test]
    fn state_image_round_trips_the_dropped_bio_card_bytes() {
        let mut state = GameState::new(RoomId::parse("11C1").unwrap(), &RoomState::default());
        // 0x208..0x20F: special-room light, character model id, last enemy
        // flags, bullet effect id and the three pickup quantities.
        state.state_bytes[0x08..0x10].copy_from_slice(&[0x21, 0x02, 0x43, 0x04, 0x05, 7, 240, 240]);
        // 0x214..0x21D and 0x220..0x223: fade state, light, rand seed,
        // countdown and the held/pressed pad words (0x21E is the health copy,
        // owned by the explicit field).
        for (index, byte) in state.state_bytes[0x14..0x1E].iter_mut().enumerate() {
            *byte = 0x30 + index as u8;
        }
        for (index, byte) in state.state_bytes[0x20..0x24].iter_mut().enumerate() {
            *byte = 0x50 + index as u8;
        }
        state.entities[0].health = 0x1234;
        state.state_bytes[0x2A] = 0x5C;

        let file = SaveFile::from_state(&state);
        assert_eq!(
            &file.state_bytes[0x08..0x10],
            &state.state_bytes[0x08..0x10]
        );
        let bytes = file.to_bytes();
        assert_eq!(
            &bytes[0x208..0x210],
            &state.state_bytes[0x08..0x10],
            "the prefix rides the block verbatim"
        );
        assert_eq!(&bytes[0x214..0x21E], &state.state_bytes[0x14..0x1E]);
        assert_eq!(&bytes[0x220..0x224], &state.state_bytes[0x20..0x24]);
        assert_eq!(
            &bytes[0x21E..0x220],
            &0x1234i16.to_le_bytes(),
            "health copy"
        );
        assert_eq!(bytes[0x22A], 0x5C, "room-item backup");

        let parsed = SaveFile::from_bytes(&bytes).unwrap();
        let mut restored = GameState::default();
        parsed.apply_to(&mut restored);
        assert_eq!(
            &restored.state_bytes[0x08..0x10],
            &state.state_bytes[0x08..0x10]
        );
        assert_eq!(
            &restored.state_bytes[0x14..0x1E],
            &state.state_bytes[0x14..0x1E]
        );
        assert_eq!(
            &restored.state_bytes[0x20..0x24],
            &state.state_bytes[0x20..0x24]
        );
        assert_eq!(restored.state_bytes[0x2A], 0x5C);
        assert_eq!(SaveFile::from_state(&restored), file);
    }

    #[test]
    fn old_slots_load_every_state_image_byte_as_written() {
        // A pre-M15 writer never touched 0x208..0x20F and 0x214..0x223, so an
        // old slot loads them zero instead of failing.
        let file = filled();
        let mut bytes = file.to_bytes();
        bytes[0x208..0x210].fill(0);
        bytes[0x214..0x21E].fill(0);
        bytes[0x220..0x224].fill(0);
        let parsed = SaveFile::from_bytes(&bytes).unwrap();
        assert!(parsed.state_bytes[0x08..0x10].iter().all(|&byte| byte == 0));
        assert!(parsed.state_bytes[0x14..0x1E].iter().all(|&byte| byte == 0));
        assert!(parsed.state_bytes[0x20..0x24].iter().all(|&byte| byte == 0));
        assert_eq!(parsed.prefix, file.prefix);
    }

    #[test]
    fn from_bytes_rejects_a_short_block() {
        let bytes = filled().to_bytes();
        assert!(SaveFile::from_bytes(&bytes[..SAVE_LAYOUT_SIZE - 1]).is_err());
        assert!(SaveFile::from_bytes(&[]).is_err());
        assert!(SaveFile::from_bytes(&bytes).is_ok());
    }

    /// The save layout did not grow an enemy-snapshot block: an existing slot
    /// loads, and the destination's own combat tables survive the restore.
    #[test]
    fn existing_saves_load_without_touching_the_combat_tables() {
        let state = GameState::new(RoomId::parse("11C1").unwrap(), &RoomState::default());
        let file = SaveFile::from_state(&state);
        let bytes = file.to_bytes();
        assert!(bytes.len() >= SAVE_LAYOUT_SIZE);

        let mut restored = GameState::default();
        restored.combat.weapon_ranges[0][0] = 12_345;
        restored.saved_enemies[3].valid = 2;
        SaveFile::from_bytes(&bytes)
            .unwrap()
            .apply_to(&mut restored);
        assert_eq!(
            restored.combat.weapon_ranges[0][0], 12_345,
            "apply_to must not clobber the loaded combat tables"
        );
        assert_eq!(restored.saved_enemies[3].valid, 2);
    }

    #[test]
    fn from_bytes_rejects_a_block_over_the_cap() {
        let bytes = vec![0u8; budget::MAX_SAVE_BYTES + 1];
        let message = budget::assert_cap_error(SaveFile::from_bytes(&bytes));
        assert!(message.contains("save block size"), "{message}");
    }

    #[test]
    fn state_round_trips_through_the_block() {
        let mut state = GameState::new(RoomId::parse("11C1").unwrap(), &RoomState::default());
        state.entities[0].health = 96;
        state.entities[0].pos = [1234, 7, -2345];
        state.entities[0].angle = 0x456;
        // Jill's derived maximum; the block does not carry it.
        state.max_health = 96;
        state.health_status = 0x20;
        state.camera.current_cut = 3;
        state.state_bytes[0x24..0x28].copy_from_slice(&123456u32.to_le_bytes());
        state.state_bytes[usize::from(STATE_BYTE_SAVES)] = 4;
        state.state_bytes[usize::from(STATE_BYTE_CUT)] = 1;
        state.state_bytes[usize::from(STATE_BYTE_MENU_CHOICE)] = 1;
        state.state_bytes[usize::from(STATE_BYTE_FWD_ACTION)] = 5;
        state.state_bytes[usize::from(STATE_BYTE_ENT_ACTION)] = 6;
        state.add_item(0x0B, 15);
        state.add_item(0x2F, 1);
        state.select_item(Some(0x0B));
        state.record_used_item(0x41);
        state.set_equipped(Some(0x02));
        state.mark_examined(0x33);
        state.item_box[3] = InventoryItem {
            id: 0x41,
            quantity: 1,
        };
        for bank in [0u8, 1, 2, 3, 7, 8] {
            state.apply_flag(bank, 0, 0);
        }

        let file = SaveFile::from_state(&state);
        // The block stores the 0-based `g_stageId` of the 1-based room digit.
        assert_eq!(file.stage, 0);
        assert_eq!(file.to_bytes()[0x200], 0);
        assert_eq!(file.room, 0x1C);
        assert_eq!(file.character, 1);
        assert_eq!(file.health, 96);
        assert_eq!(file.timer, 123456);
        assert_eq!(file.saves, 4);
        assert_eq!(file.pos_x, 1234);
        assert_eq!(file.pos_z, -2345);
        assert_eq!(file.angle, 0x456);
        assert_eq!(file.health_status, 0x20);
        assert_eq!(file.total_held, 2);
        assert_eq!(file.selected_item, 0x0B);
        assert_eq!(file.used_item, 0x41);
        assert_eq!(file.equipped, 0x02);
        assert_eq!(file.item_box[3].id, 0x41);
        // The examined bank comes from the item-name lookup's own store (item
        // 0x33's class-3 bit) and round-trips at BioCard 0x29C.
        assert_eq!(file.examined[3] & 0x10, 0x10);
        assert_eq!(file.to_bytes()[0x29F] & 0x10, 0x10);

        let parsed = SaveFile::from_bytes(&file.to_file_bytes()).unwrap();
        let mut restored = GameState::default();
        parsed.apply_to(&mut restored);

        assert_eq!(restored.id, state.id);
        // The maximum is not in the block; apply_to re-derives it from the
        // saved character exactly as InitializeGame does (Jill here).
        assert_eq!(restored.max_health, 96);
        // InitializeGame restores the selected character into the player
        // entity's id on the continue path too; room scripts branch on it.
        assert_eq!(restored.entities[0].id, 1);
        assert_eq!(restored.entities[0].health, 96);
        assert_eq!(restored.entities[0].pos[0], 1234);
        assert_eq!(restored.entities[0].pos[2], -2345);
        assert_eq!(
            restored.entities[0].pos[1], 7,
            "the height rides in the trailer"
        );
        assert_eq!(restored.entities[0].angle, 0x456);
        assert_eq!(restored.health_status, 0x20);
        assert_eq!(restored.examined_flags(), state.examined_flags());
        assert_eq!(restored.inventory, state.inventory);
        assert_eq!(restored.item_box, state.item_box);
        assert_eq!(restored.selected_item, Some(0x0B));
        assert_eq!(restored.last_used_item, Some(0x41));
        assert_eq!(restored.equipped, Some(0x02));
        assert_eq!(restored.state_bytes[usize::from(STATE_BYTE_SAVES)], 4);
        for bank in 0..FLAG_BANK_COUNT {
            assert_eq!(
                restored.flags[bank].bytes(),
                state.flags[bank].bytes(),
                "bank {bank}"
            );
        }
        assert_eq!(SaveFile::from_state(&restored), file);
    }

    #[test]
    fn apply_to_restores_the_player_entity_character_from_the_block() {
        // A continue builds a fresh state (its entity id defaults to zero) and
        // applies the block. InitializeGame writes the block's selected
        // character into the player entity's id on that path too, so the room
        // scripts' `g_playerEntity.id & 3` tests see the saved character.
        let mut jill = GameState::new(RoomId::parse("1000").unwrap(), &RoomState::default());
        jill.state_bytes[usize::from(STATE_BYTE_CHARACTER)] = 1;
        let file = SaveFile::from_state(&jill);
        assert_eq!(file.character, 1);

        let mut restored = GameState::default();
        file.apply_to(&mut restored);
        assert_eq!(restored.entities[0].id, 1);

        // And a Chris block restores zero even when the fresh state carried a
        // one: the block wins over the boot's own seeding.
        let mut chris = GameState::new(RoomId::parse("1000").unwrap(), &RoomState::default());
        chris.entities[0].id = 1;
        let file = SaveFile::from_state(&chris);
        assert_eq!(file.character, 0);

        let mut restored = GameState::default();
        restored.entities[0].id = 1;
        file.apply_to(&mut restored);
        assert_eq!(restored.entities[0].id, 0);
    }

    #[test]
    fn room_bgm_table_round_trips_the_live_table() {
        let mut state = GameState::new(RoomId::parse("1000").unwrap(), &RoomState::default());
        // A script's `room_bgm_state_set` write must survive the block, not the shipped
        // constant.
        state.room_bgm[7] = 0x40;
        let file = SaveFile::from_state(&state);
        assert_eq!(file.room_bgm[7], 0x40);

        let parsed = SaveFile::from_bytes(&file.to_bytes()).unwrap();
        let mut restored = GameState::default();
        parsed.apply_to(&mut restored);
        assert_eq!(restored.room_bgm, state.room_bgm);
        // The live state byte is not in the block; it resets so the room entry
        // rebuilds the channels from the table.
        assert_eq!(restored.bgm.state, 0xFF);
    }

    #[test]
    fn slot_names_and_paths_are_one_based() {
        assert_eq!(slot_name(0).unwrap(), "savedat1.dat");
        assert_eq!(slot_name(7).unwrap(), "savedat8.dat");
        assert!(slot_name(8).is_none());
        assert_eq!(
            slot_path(Path::new("saves"), 2).unwrap(),
            Path::new("saves").join("savedat3.dat")
        );
        assert_eq!(
            default_save_dir(),
            crate::config::Config::default_save_dir()
        );
    }

    #[test]
    fn the_folder_scan_lists_sorted_files_with_their_headers() {
        let dir = TempDir::new("scan-dir");
        let file = filled();
        fs::create_dir_all(&dir.path).unwrap();
        fs::write(dir.path.join("beta.dat"), file.to_bytes()).unwrap();
        fs::write(dir.path.join("alpha.dat"), file.to_bytes()).unwrap();
        fs::write(dir.path.join("notes.txt"), b"not a save").unwrap();
        fs::create_dir_all(dir.path.join("subdir")).unwrap();

        let entries = scan_dir(&dir.path);
        let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, ["alpha.dat", "beta.dat", "notes.txt"]);
        assert_eq!(entries[0].base, "alpha");
        assert!(entries[0].loadable());
        assert_eq!(entries[0].info, Some(SaveSlotInfo::from_file(&file)));
        assert_eq!(entries[0].path, dir.path.join("alpha.dat"));
        assert!(entries[0].timestamp > 0.0, "the mtime is recorded");
        assert!(!entries[2].loadable());
        assert_eq!(entries[2].info, None);

        // A missing directory is an empty list, not an error.
        assert!(scan_dir(&dir.path.join("missing")).is_empty());
    }

    #[test]
    fn autosaves_rotate_two_files_and_list_as_autosaves() {
        let dir = TempDir::new("autosave");
        let save_dir = dir.path.join("nested");
        let mut first = filled();
        first.room = 1;
        let mut second = filled();
        second.room = 2;
        let mut third = filled();
        third.room = 3;

        write_autosave(&save_dir, &first).unwrap();
        assert_eq!(load_file(&save_dir.join("autosave1.dat")).unwrap().room, 1);
        assert!(!save_dir.join("autosave2.dat").exists());

        write_autosave(&save_dir, &second).unwrap();
        write_autosave(&save_dir, &third).unwrap();
        assert_eq!(load_file(&save_dir.join("autosave1.dat")).unwrap().room, 3);
        assert_eq!(load_file(&save_dir.join("autosave2.dat")).unwrap().room, 2);
        let names: Vec<String> = fs::read_dir(&save_dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(names.len(), 2, "only the two autosaves remain: {names:?}");

        fs::write(save_dir.join("savedat1.dat"), first.to_bytes()).unwrap();
        let entries = scan_dir(&save_dir);
        let marks: Vec<(&str, Option<usize>)> = entries
            .iter()
            .map(|entry| (entry.name.as_str(), entry.autosave))
            .collect();
        assert_eq!(
            marks,
            [
                ("autosave1.dat", Some(1)),
                ("autosave2.dat", Some(2)),
                ("savedat1.dat", None),
            ]
        );
        assert_eq!(autosave_index("AUTOSAVE2.DAT"), Some(2));
        assert_eq!(autosave_index("autosave3.dat"), None);
    }

    #[test]
    fn the_autosave_writer_writes_in_queue_order() {
        let dir = TempDir::new("autosave-writer");
        let mut writer = AutosaveWriter::default();
        for room in 1..=3 {
            let mut file = filled();
            file.room = room;
            writer.write(&dir.path, file);
        }
        writer.flush();
        assert_eq!(load_file(&dir.path.join("autosave1.dat")).unwrap().room, 3);
        assert_eq!(load_file(&dir.path.join("autosave2.dat")).unwrap().room, 2);
        // A writer is reusable after a flush.
        let mut file = filled();
        file.room = 4;
        writer.write(&dir.path, file);
        drop(writer);
        assert_eq!(load_file(&dir.path.join("autosave1.dat")).unwrap().room, 4);
    }

    #[test]
    fn a_failed_autosave_keeps_the_previous_pair() {
        let dir = TempDir::new("autosave-failed");
        let mut first = filled();
        first.room = 1;
        let mut second = filled();
        second.room = 2;
        write_autosave(&dir.path, &first).unwrap();
        write_autosave(&dir.path, &second).unwrap();
        // A directory squatting on the temporary name makes the write fail.
        let temp = crate::atomic::temp_path(&dir.path.join("autosave1.dat")).unwrap();
        fs::create_dir_all(&temp).unwrap();
        assert!(write_autosave(&dir.path, &filled()).is_err());
        assert_eq!(load_file(&dir.path.join("autosave1.dat")).unwrap().room, 2);
        assert_eq!(load_file(&dir.path.join("autosave2.dat")).unwrap().room, 1);
    }

    #[test]
    fn a_truncated_file_still_appears_as_unloadable() {
        let dir = TempDir::new("scan-truncated");
        fs::write(dir.path.join("broken.dat"), [0u8; 8]).unwrap();
        let entries = scan_dir(&dir.path);
        assert_eq!(entries.len(), 1);
        assert!(!entries[0].loadable());
    }

    #[test]
    fn explicit_paths_write_remove_and_validate_names() {
        let dir = TempDir::new("explicit");
        let file = filled();
        let path = dir.path.join("Jill_01.dat");
        save_to_path(&path, &file).unwrap();
        assert_eq!(load_file(&path).unwrap(), file);

        assert_eq!(file_path(&dir.path, "Jill_01.dat"), Some(path.clone()));
        assert_eq!(file_path(&dir.path, "a/b.dat"), None);
        assert_eq!(file_path(&dir.path, ".."), None);
        assert_eq!(file_path(&dir.path, "."), None);
        assert_eq!(file_path(&dir.path, ""), None);
        assert_eq!(file_path(&dir.path, "/abs.dat"), None);
        assert_eq!(file_path(&dir.path, &"x".repeat(MAX_FILE_NAME + 1)), None);

        remove_file(&path).unwrap();
        assert!(!path.exists());
        remove_file(&path).unwrap();
        remove_file(&dir.path.join("missing.dat")).unwrap();
    }

    #[test]
    fn an_unwritable_explicit_path_is_reported() {
        let dir = TempDir::new("explicit-blocked");
        let blocked = dir.path.join("blocked");
        fs::write(&blocked, b"not a directory").unwrap();
        assert!(save_to_path(&blocked.join("x.dat"), &filled()).is_err());
    }

    #[test]
    fn directory_io_scans_and_reloads_slots() {
        let dir = TempDir::new("io");
        let file = filled();
        save(&dir.path, 0, &file).unwrap();
        save(&dir.path, 4, &file).unwrap();

        let scan = scan_slots(&dir.path);
        let info = SaveSlotInfo::from_file(&file);
        assert_eq!(scan[0], Some(info));
        assert_eq!(scan[4], Some(info));
        assert_eq!(scan[1], None);
        assert_eq!(scan[7], None);

        let loaded = load(&dir.path, 0).unwrap();
        assert_eq!(loaded, file);
        assert_eq!(
            fs::read(dir.path.join("savedat1.dat")).unwrap(),
            file.to_bytes()
        );

        // A slot past the range is an error, not a panic.
        assert!(load(&dir.path, 8).is_err());
        assert!(save(&dir.path, 8, &file).is_err());
    }

    #[test]
    fn an_unwritable_save_directory_is_reported() {
        let dir = TempDir::new("unwritable");
        // A regular file where the save directory should be: creating the
        // directory fails before any slot is written.
        let blocked = dir.path.join("blocked");
        fs::write(&blocked, b"not a directory").unwrap();

        let err = save(&blocked, 0, &filled()).unwrap_err();

        let text = format!("{err:#}");
        assert!(text.contains("failed to create"), "{text}");
        assert!(text.contains("blocked"), "{text}");
        assert_eq!(fs::read(&blocked).unwrap(), b"not a directory");
        assert!(load(&blocked, 0).is_err());
    }

    #[test]
    fn an_explicit_path_loads_a_written_block() {
        let dir = TempDir::new("file");
        let file = filled();
        let path = dir.path.join("scenario.dat");
        fs::write(&path, file.to_bytes()).unwrap();

        assert_eq!(load_file(&path).unwrap(), file);
        assert!(load_file(&dir.path.join("missing.dat")).is_err());
    }

    #[test]
    fn a_truncated_slot_reads_as_empty() {
        let dir = TempDir::new("truncated");
        fs::write(dir.path.join("savedat2.dat"), [0u8; 8]).unwrap();
        assert_eq!(read_slot_info(&dir.path, 1), None);
        assert!(load(&dir.path, 1).is_err());
    }

    #[test]
    fn from_state_uses_the_hardcoded_prefix() {
        let file = SaveFile::from_state(&GameState::default());
        assert_eq!(file.prefix, BIOCARD_PREFIX);
    }

    #[test]
    fn prefix_must_be_at_least_the_prefix_length() {
        let mut file = SaveFile::default();
        assert!(file.set_prefix(&[0u8; PREFIX_LEN]).is_ok());
        assert!(file.set_prefix(&[0u8; 8]).is_err());
        assert_eq!(
            SaveFile::from_state_with_prefix(&GameState::default(), &[0xAA; SAVE_LAYOUT_SIZE])
                .unwrap()
                .prefix,
            [0xAA; PREFIX_LEN]
        );
    }

    #[test]
    #[ignore = "requires a real RE1 installation via ARKLAY_RE1_ROOT"]
    fn real_prefix_writes_and_reloads_through_a_directory() {
        let Ok(root) = std::env::var("ARKLAY_RE1_ROOT") else {
            return;
        };
        let card =
            fs::read(Path::new(&root).join("JPN/DATA/bio_card.dat")).expect("read bio_card.dat");
        assert_eq!(&card[..PREFIX_LEN], &BIOCARD_PREFIX);
        let mut state = GameState::new(RoomId::parse("1001").unwrap(), &RoomState::default());
        state.entities[0].health = 96;
        state.add_item(0x0B, 15);
        let file = SaveFile::from_state_with_prefix(&state, &card).unwrap();
        assert_eq!(&file.prefix[..], &card[..PREFIX_LEN]);
        assert_eq!(&file.to_bytes()[..PREFIX_LEN], &card[..PREFIX_LEN]);

        let dir = TempDir::new("real-io");
        save(&dir.path, 0, &file).unwrap();
        let scan = scan_slots(&dir.path);
        assert_eq!(scan[0], Some(SaveSlotInfo::from_file(&file)));
        let loaded = load(&dir.path, 0).unwrap();
        assert_eq!(loaded, file);
        let mut restored = GameState::default();
        loaded.apply_to(&mut restored);
        assert_eq!(restored.entities[0].health, 96);
        assert_eq!(restored.item_count(0x0B), 15);
    }
}
