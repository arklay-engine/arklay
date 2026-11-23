//! The original RE 2's save image: the 2048 bytes `Card_access` writes to a
//! `.BIOHAZARD2` file (`WriteSaveFile(name, 0x98E79C, 0x800)`, state 3 and
//! state 0x1F), and `Load_pop` reads back.
//!
//! The image is the game's save work in memory, as it stands on the write
//! tick. A new game fills its first 0x798 bytes from its scenario's record of
//! `INIT_TBL.DAT` (`FUN_004b7860`, `Memcpy(0x98E79C, record, 0x798)`), the
//! memory card header and icon included, and play keeps most of it live: the
//! flag banks, the script variables, the room music, `Item_work`, the item
//! box and the files are this memory. On the write tick `Card_access` takes
//! the ribbon and raises the count, and `Save_push` (`0x4C7830`) copies in
//! what lives elsewhere: the player, its position and angle, the equipped
//! weapon, the poison and `fg_status`. `WriteSaveFile` writes the bytes as
//! they are, with no checksum or scramble.
//!
//! [`build`] makes the image from arklay's state over the scenario's record,
//! [`REGIONS`] names every byte of it, and [`differences`] compares two images
//! under the regions' mask. A region is masked only when arklay doesn't hold
//! what the original writes there; its reason says what that is.

use crate::game::GameState;

/// Bytes in the image.
pub const IMAGE_LEN: usize = 0x800;
/// Bytes of the save work, which a new game copies from `INIT_TBL.DAT` and
/// `Load_pop`'s load copies back; the image's last 0x68 bytes are the memory
/// after it.
pub const WORK_LEN: usize = 0x798;
/// Where the image starts in the original's memory.
pub const ADDRESS: u32 = 0x98_E79C;

/// A run of the image's bytes and what the original keeps there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub name: &'static str,
    pub offset: usize,
    pub len: usize,
    /// Why the comparison leaves the region out, or `None` when it compares.
    pub masked: Option<&'static str>,
    /// With [`Self::masked`], the bits it leaves out, a byte of them for each
    /// of the region's bytes; `None` leaves out the whole region.
    pub bits: Option<&'static [u8]>,
}

const fn region(name: &'static str, offset: usize, len: usize) -> Region {
    Region {
        name,
        offset,
        len,
        masked: None,
        bits: None,
    }
}

const fn masked(name: &'static str, offset: usize, len: usize, why: &'static str) -> Region {
    Region {
        name,
        offset,
        len,
        masked: Some(why),
        bits: None,
    }
}

const fn masked_bits(
    name: &'static str,
    offset: usize,
    bits: &'static [u8],
    why: &'static str,
) -> Region {
    Region {
        name,
        offset,
        len: bits.len(),
        masked: Some(why),
        bits: Some(bits),
    }
}

/// The image's regions in order, every byte in one. The names are the
/// original's globals where it has them, by their address
/// ([`ADDRESS`] + offset).
pub const REGIONS: &[Region] = &[
    region("card header and icon", 0x000, 0x200),
    masked(
        "GameSeconds",
        0x200,
        4,
        "the play clock, which WinMain counts from the record's boot and arklay's run starts after",
    ),
    region("0x98E9A0", 0x204, 4),
    region("inventory_size", 0x208, 1),
    region("0x98E9A5", 0x209, 1),
    region("player_index", 0x20A, 1),
    region("save place", 0x20B, 1),
    masked(
        "Sfx_vol, Bgm_vol, key type",
        0x20C,
        3,
        "the options (config.ini's volumes and key type), which arklay keeps in its own settings",
    ),
    region("poison_timer", 0x20F, 1),
    region("poison_status", 0x210, 2),
    region("nExtremeLv", 0x212, 2),
    region("fg_status", 0x214, 4),
    region("equipped weapon", 0x218, 1),
    region("equipped slot", 0x219, 1),
    region("life", 0x21A, 2),
    masked(
        "kills, sprays",
        0x21C,
        4,
        "the results screen's counts (Em10_Die's kills, St_com_use_equip's sprays), which arklay doesn't keep",
    ),
    region("save count", 0x220, 2),
    region("position", 0x222, 6),
    // The new game's `DWORD_0098e9c4 = DWORD_0099c8a8 & 1`, and
    // `Flg_on(.., 0x12)` (0x2000) under `DWORD_00995b48 & 0x40`
    // (`FUN_004b73c0`). In play only the status screen's Konami code writes
    // it (`Status`, `0x501BD0`), which arklay doesn't port: its first entry
    // raises `0xFFFFFF00`, and its second puts the word back to bit 0 and
    // raises 0x2000, both masked, so a recording's image differs only while
    // the first entry's bits are up.
    masked_bits(
        "extras",
        0x228,
        &[0x01, 0x20, 0, 0],
        "0x98E9C4's bits from the clear records (infinite ammo, 0x2000 the unlocked submachine gun), which arklay doesn't keep",
    ),
    region("bgm_tbl", 0x22C, 0x11C),
    region("scd_variables", 0x348, 0x48),
    region("fg_scenario", 0x390, 0x20),
    region("fg_common", 0x3B0, 0x20),
    region("fg_room, fg_tick", 0x3D0, 8),
    region("fg_enemy_0", 0x3D8, 0x20),
    region("fg_enemy_1", 0x3F8, 0x20),
    region("fg_item", 0x418, 0x20),
    region("fg_item_2", 0x438, 0x20),
    masked(
        "fg_item_2's last words",
        0x458,
        0xC,
        "fg_item_2's words 8 to 10, past the eight words each of arklay's flag banks holds",
    ),
    region("floors (bank 35)", 0x464, 4),
    region("fg_map", 0x468, 0x14),
    region("fg_map_c", 0x47C, 4),
    region("fg_map_i", 0x480, 4),
    region("Pri_be_flg", 0x484, 0x100),
    region("File_flg", 0x584, 4),
    region("fg_zapping", 0x588, 8),
    region("fg_key", 0x590, 8),
    region("Item_work", 0x598, 0x2C),
    region("Item_boxwork", 0x5C4, 0x100),
    region("File_tag_bk", 0x6C4, 0x18),
    region("angle", 0x6DC, 2),
    masked(
        "0x98EE7A",
        0x6DE,
        1,
        "set by the new game from the clear records (BYTE_0099cd5e), which arklay doesn't keep",
    ),
    region("partner poison", 0x6DF, 1),
    region("saved_splayer_health", 0x6E0, 2),
    region("0x98EE7E", 0x6E2, 2),
    region("bank 36", 0x6E4, 0x20),
    region("bank 37", 0x704, 0x10),
    region("bank 38", 0x714, 0x20),
    region("bank 39", 0x734, 0x20),
    region("Extreme Battle", 0x754, 0x3C),
    region("hard_mode", 0x790, 1),
    region("censorship_off", 0x791, 1),
    region("Extreme Battle save", 0x792, 1),
    region("0x98EF2F", 0x793, 5),
    masked(
        "after the save work",
        WORK_LEN,
        IMAGE_LEN - WORK_LEN,
        "the memory past the work, the display environments (SetDefDispEnv at 0x98EF40), not saved state",
    ),
];

/// The game an arklay save holds, as a load applies it (`SaveFile::apply_to`),
/// for its image: an error unless the file has its RE 2 trailer and the
/// trailer's state applies (a supported older layout does). Variable 27 is
/// the save's own, before the load's `0x00FF`.
pub fn saved_state(file: &crate::save::SaveFile) -> Result<GameState, String> {
    let bytes = file.re2.as_deref().ok_or("no RE 2 trailer")?;
    let mut saved = crate::scd2::host::Re2State::default();
    saved
        .load_save_bytes(bytes)
        .ok_or("its RE 2 trailer's state has no supported length")?;
    let mut game = GameState::new(file.room_id(), &crate::state::RoomState::default());
    file.apply_to(&mut game);
    let last = crate::scd2::host::VAR_LAST_RDT;
    game.re2.vars[last] = saved.vars[last];
    Ok(game)
}

/// `BYTE_00989e7e`, the row of `INIT_TBL.DAT` a new game copies: the
/// character times two, plus one for scenario B.
pub fn init_row(game: &GameState) -> usize {
    usize::from(game.id.player_flag & 1) * 2 + usize::from(game.re2.scenario_b)
}

/// Row `row` of `INIT_TBL.DAT` (its records are [`WORK_LEN`] bytes).
pub fn init_record(table: &[u8], row: usize) -> Option<&[u8]> {
    table.get(row * WORK_LEN..(row + 1) * WORK_LEN)
}

fn put(image: &mut [u8], offset: usize, bytes: &[u8]) {
    image[offset..offset + bytes.len()].copy_from_slice(bytes);
}

fn put_words(image: &mut [u8], offset: usize, words: &[u32]) {
    for (index, word) in words.iter().enumerate() {
        put(image, offset + index * 4, &word.to_le_bytes());
    }
}

/// `fg_system`'s bits that `Save_push` copies into the saved `fg_status`:
/// 0x20 as 0x4000000 and 0x80000 as 0x20000000. `Load_pop` gives them back.
fn saved_status(fg_system: u32, fg_status: u32) -> u32 {
    let mut status = fg_status;
    if fg_system & 0x20 != 0 {
        status |= 0x0400_0000;
    }
    if fg_system & 0x8_0000 != 0 {
        status |= 0x2000_0000;
    }
    status
}

/// The image `Card_access` writes for `game` on a save's write tick, over
/// the scenario's `INIT_TBL.DAT` record (`record`, [`WORK_LEN`] bytes, or
/// zeros without one). The bytes after the work are zeros.
pub fn build(game: &GameState, record: Option<&[u8]>) -> [u8; IMAGE_LEN] {
    let mut image = [0; IMAGE_LEN];
    if let Some(record) = record {
        let len = record.len().min(WORK_LEN);
        image[..len].copy_from_slice(&record[..len]);
    }
    let re2 = &game.re2;
    let player = &game.entities[0];
    // A new game's set-up (`FUN_004b73c0`).
    put(&mut image, 0x204, &0u32.to_le_bytes());
    image[0x209] = 0;
    // `Save_push`, with the play clock arklay keeps: `WinMain` adds 2 a
    // frame at 30 fps and a second past 60, so 31 frames a second.
    put(&mut image, 0x200, &(game.play_timer() / 31).to_le_bytes());
    image[0x208] = re2.inventory_size;
    image[0x20A] = re2.pl_id;
    image[0x20B] = re2.save_place;
    image[0x20F] = re2.player.poison_timer;
    let poison = if re2.player.poisoned {
        crate::game::pl_change2::POISON_STATUS
    } else {
        0
    };
    put(&mut image, 0x210, &poison.to_le_bytes());
    // Extreme Battle's level, which a scenario's game leaves at 0.
    put(&mut image, 0x212, &0u16.to_le_bytes());
    let status = saved_status(
        re2.banks[0][0],
        re2.banks[usize::from(crate::scd2::host::STATUS_BANK)][0],
    );
    put(&mut image, 0x214, &status.to_le_bytes());
    // `St+0xA` and `St+8`'s low bytes.
    image[0x218] = game.equipped_weapon_id;
    image[0x219] = game.re2_equip_slot();
    put(&mut image, 0x21A, &player.health.to_le_bytes());
    put(&mut image, 0x220, &game.re2_save_count().to_le_bytes());
    // The extras word: arklay holds none of its bits (the new game's, from
    // the clear records, and the status screen's Konami code, not ported).
    put(&mut image, 0x228, &0u32.to_le_bytes());
    for (axis, &value) in player.pos.iter().enumerate() {
        put(&mut image, 0x222 + axis * 2, &(value as i16).to_le_bytes());
    }
    for (room, word) in re2.bgm.words().iter().enumerate() {
        put(&mut image, 0x22C + room * 2, &word.to_le_bytes());
    }
    for index in 0..0x24 {
        put(
            &mut image,
            0x348 + index * 2,
            &script_var(game, index).to_le_bytes(),
        );
    }
    // The flag banks where `Flg_addr` has them, each as long as the
    // original's memory gives it before the next.
    // `fg_item_2` is eleven words, of which arklay holds eight.
    let banks: [(usize, usize, usize); 13] = [
        (3, 0x390, 8),
        (4, 0x3B0, 8),
        (5, 0x3D0, 2),
        (6, 0x3D8, 8),
        (7, 0x3F8, 8),
        (8, 0x418, 8),
        (34, 0x438, 8),
        (35, 0x464, 1),
        (9, 0x468, 5),
        (32, 0x47C, 1),
        (33, 0x480, 1),
        (29, 0x588, 2),
        (36, 0x6E4, 8),
    ];
    for (bank, offset, words) in banks {
        put_words(&mut image, offset, &re2.banks[bank][..words]);
    }
    put_words(&mut image, 0x704, &re2.banks[37][..4]);
    put_words(&mut image, 0x714, &re2.banks[38]);
    put_words(&mut image, 0x734, &re2.banks[39]);
    // `Pri_be_flg`: banks 13 to 28, four words each.
    for camera in 0..16 {
        put_words(
            &mut image,
            0x484 + camera * 0x10,
            &re2.banks[13 + camera][..4],
        );
    }
    put_words(&mut image, 0x590, &re2.door_locks);
    // `Item_work`: ten slots and the personal item, `[id, count, mark, _]`.
    let marks = game.re2_marks();
    for slot in 0..10 {
        let stack = game.inventory.get(slot).copied().unwrap_or_default();
        let mark = marks.get(slot).copied().unwrap_or(0);
        put(
            &mut image,
            0x598 + slot * 4,
            &[stack.id, stack.quantity, mark],
        );
    }
    put(&mut image, 0x598 + 40, &re2.personal_item);
    image[0x598 + 42] = 0;
    for (slot, entry) in re2.item_box.iter().enumerate() {
        put(&mut image, 0x5C4 + slot * 4, entry);
    }
    put(&mut image, 0x6C4, &re2.files);
    // `Save_push`'s `pPl.Cdir.y`.
    put(&mut image, 0x6DC, &player.angle.to_le_bytes());
    image[0x6DF] = (image[0x6DF] & !1) | u8::from(re2.partner_poisoned);
    put(&mut image, 0x6E0, &re2.partner_life.to_le_bytes());
    image[0x790] = u8::from(re2.hard_mode);
    image[0x791] = u8::from(re2.censorship_off);
    // `Card_access`'s Extreme Battle byte: 0 outside it.
    image[0x792] = 0;
    image
}

/// Script variable `index` as the original's `scd_variables` holds it: the
/// stage, the room and the cut are the room's.
fn script_var(game: &GameState, index: usize) -> u16 {
    const STAGE: usize = 24;
    const ROOM: usize = 25;
    const CUT: usize = 26;
    match index {
        STAGE => u16::from(game.origin().stage.saturating_sub(1)),
        ROOM => u16::from(game.origin().room),
        CUT => game.camera.current_cut as u16,
        _ => game.re2.vars[index],
    }
}

/// An unmasked region whose bytes differ: the span from its first
/// differing byte to its last.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Difference {
    pub region: &'static str,
    pub offset: usize,
    pub ours: Vec<u8>,
    pub theirs: Vec<u8>,
}

/// The unmasked regions where `ours` and `theirs` differ.
pub fn differences(ours: &[u8; IMAGE_LEN], theirs: &[u8; IMAGE_LEN]) -> Vec<Difference> {
    REGIONS
        .iter()
        .filter(|region| region.masked.is_none() || region.bits.is_some())
        .filter_map(|region| {
            let range = region.offset..region.offset + region.len;
            let left_out = |at: usize| region.bits.map_or(0, |bits| bits[at - region.offset]);
            let differ = |at: &usize| (ours[*at] ^ theirs[*at]) & !left_out(*at) != 0;
            let first = range.clone().find(differ)?;
            let last = range.rev().find(differ)?;
            Some(Difference {
                region: region.name,
                offset: first,
                ours: ours[first..=last].to_vec(),
                theirs: theirs[first..=last].to_vec(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::InventoryItem;
    use crate::state::{RoomId, RoomState};

    fn game() -> GameState {
        let mut game = GameState::new(RoomId::parse("2000").unwrap(), &RoomState::default());
        game.is_re2_room = true;
        game
    }

    fn u16_at(image: &[u8], offset: usize) -> u16 {
        u16::from_le_bytes([image[offset], image[offset + 1]])
    }

    fn u32_at(image: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(image[offset..offset + 4].try_into().unwrap())
    }

    #[test]
    fn the_regions_cover_the_image_in_order() {
        let mut at = 0;
        for region in REGIONS {
            assert_eq!(
                region.offset, at,
                "{} starts where the last ended",
                region.name
            );
            assert!(region.len > 0);
            at += region.len;
        }
        assert_eq!(at, IMAGE_LEN);
    }

    #[test]
    fn the_record_is_the_base_and_the_tail_is_zero() {
        let record: Vec<u8> = (0..WORK_LEN).map(|at| (at % 251) as u8 + 1).collect();
        let image = build(&game(), Some(&record));
        assert_eq!(image[..0x200], record[..0x200]);
        // A byte no field writes, the save work's last.
        assert_eq!(image[0x797], record[0x797]);
        assert!(image[WORK_LEN..].iter().all(|&byte| byte == 0));
        assert_eq!(
            init_record(&[0; WORK_LEN * 6], 5).map(<[u8]>::len),
            Some(WORK_LEN)
        );
        assert_eq!(init_record(&[0; WORK_LEN * 6], 6), None);
    }

    #[test]
    fn save_push_fields_go_to_their_offsets() {
        let mut game = game();
        game.re2.inventory_size = 8;
        game.re2.pl_id = 1;
        game.re2.save_place = 0x0F;
        game.re2.player.poisoned = true;
        game.re2.player.poison_timer = 7;
        game.re2.banks[1][0] = 0x8000_0001;
        game.re2.banks[0][0] = 0x20 | 0x8_0000;
        game.entities[0].health = 150;
        game.entities[0].pos = [-12200, -1800, -1900];
        game.entities[0].angle = 0x0400;
        game.state_bytes[usize::from(crate::game::STATE_BYTE_SAVES)] = 3;
        game.inventory = vec![
            InventoryItem {
                id: 2,
                quantity: 18,
            },
            InventoryItem {
                id: 0x1E,
                quantity: 2,
            },
        ];
        game.set_equipped(Some(2));
        game.re2.personal_item = [0x2F, 1];
        game.re2.item_box[1] = [0x14, 30, 0];
        game.re2.files[0] = 4;
        game.re2.partner_life = 120;
        game.re2.partner_poisoned = true;
        game.re2.hard_mode = true;
        game.re2.vars[29] = 0xCE7F;
        game.re2.vars[32] = 1;
        game.play_timer_frozen = false;
        game.set_play_timer(31 * 15 + 30);
        let image = build(&game, None);
        assert_eq!(u32_at(&image, 0x200), 15);
        assert_eq!(image[0x208], 8);
        assert_eq!(image[0x20A], 1);
        assert_eq!(image[0x20B], 0x0F);
        assert_eq!(image[0x20F], 7);
        assert_eq!(u16_at(&image, 0x210), 0x1518);
        assert_eq!(u32_at(&image, 0x214), 0xA400_0001);
        assert_eq!(image[0x218], 2);
        assert_eq!(image[0x219], 0);
        assert_eq!(u16_at(&image, 0x21A), 150);
        assert_eq!(u16_at(&image, 0x220), 3);
        assert_eq!(image[0x222..0x228], [0x58, 0xD0, 0xF8, 0xF8, 0x94, 0xF8]);
        assert_eq!(u16_at(&image, 0x348 + 29 * 2), 0xCE7F);
        assert_eq!(u16_at(&image, 0x348 + 32 * 2), 1);
        // The room 200's stage and room.
        assert_eq!(u16_at(&image, 0x348 + 24 * 2), 1);
        assert_eq!(u16_at(&image, 0x348 + 25 * 2), 0);
        assert_eq!(image[0x598..0x5A0], [2, 18, 0, 0, 0x1E, 2, 0, 0]);
        assert_eq!(image[0x5C0..0x5C2], [0x2F, 1]);
        assert_eq!(image[0x5C8..0x5CB], [0x14, 30, 0]);
        assert_eq!(image[0x6C4], 4);
        assert_eq!(u16_at(&image, 0x6DC), 0x0400);
        assert_eq!(image[0x6DF], 1);
        assert_eq!(u16_at(&image, 0x6E0), 120);
        assert_eq!(image[0x790], 1);
        assert_eq!(image[0x791], 0);
    }

    #[test]
    fn the_flag_banks_keep_their_addresses_and_lengths() {
        let mut game = game();
        game.re2.banks[4][0] = 0x0200_0000;
        game.re2.banks[9] = [0x8000_0002, 1, 2, 3, 4, 5, 6, 7];
        game.re2.banks[35][0] = 0xA000_0000;
        game.re2.banks[5] = [0x11, 0x22, 0x33, 0, 0, 0, 0, 0];
        game.re2.door_locks = [0x4000, 0x8000_0000];
        let image = build(&game, None);
        // `fg_common`'s first word, low byte first: 3:128 is byte 19's 0x80.
        assert_eq!(image[0x3B0..0x3B4], [0, 0, 0, 2]);
        assert_eq!(u32_at(&image, 0x464), 0xA000_0000);
        assert_eq!(u32_at(&image, 0x468), 0x8000_0002);
        // `fg_map` is five words: `fg_map_c` follows it.
        assert_eq!(u32_at(&image, 0x478), 4);
        assert_eq!(u32_at(&image, 0x47C), game.re2.banks[32][0]);
        // Bank 5 is two words: `fg_enemy_0` follows.
        assert_eq!(u32_at(&image, 0x3D4), 0x22);
        assert_eq!(u32_at(&image, 0x3D8), game.re2.banks[6][0]);
        // A new game's `Pri_be_flg` is all set.
        assert!(image[0x484..0x584].iter().all(|&byte| byte == 0xFF));
        assert_eq!(u32_at(&image, 0x590), 0x4000);
        assert_eq!(u32_at(&image, 0x594), 0x8000_0000);
    }

    #[test]
    fn a_masked_region_never_differs() {
        let game = game();
        let ours = build(&game, None);
        let mut theirs = ours;
        for region in REGIONS.iter().filter(|region| region.masked.is_some()) {
            theirs[region.offset] ^= region.bits.map_or(0xFF, |bits| bits[0]);
        }
        // The extras' 0x2000 too, which the record's saves have.
        theirs[0x229] ^= 0x20;
        assert!(differences(&ours, &theirs).is_empty());
        theirs[0x5A1] ^= 3;
        theirs[0x5A3] ^= 1;
        assert_eq!(
            differences(&ours, &theirs),
            [Difference {
                region: "Item_work",
                offset: 0x5A1,
                ours: ours[0x5A1..0x5A4].to_vec(),
                theirs: theirs[0x5A1..0x5A4].to_vec(),
            }]
        );
    }

    #[test]
    fn the_extras_compare_but_for_the_clear_records_bits() {
        let ours = build(&game(), None);
        let mut theirs = ours;
        theirs[0x228] |= 0x02;
        let found = differences(&ours, &theirs);
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].region, found[0].offset), ("extras", 0x228));
    }

    #[test]
    fn the_save_count_is_the_whole_word() {
        let mut game = game();
        game.state_bytes[usize::from(crate::game::STATE_BYTE_SAVES)] = 1;
        game.re2.saves_high = 1;
        assert_eq!(u16_at(&build(&game, None), 0x220), 0x101);
    }

    #[test]
    fn a_saved_state_needs_its_re2_trailer() {
        let mut game = game();
        game.state_bytes[usize::from(crate::game::STATE_BYTE_SAVES)] = 4;
        game.re2.leave_room(RoomId::parse("1000").unwrap());
        let file = crate::save::SaveFile::from_state(&game);
        let saved = saved_state(&file).unwrap();
        let image = build(&saved, None);
        assert_eq!(u16_at(&image, 0x220), 4);
        assert_eq!(
            u16_at(&image, 0x348 + 27 * 2),
            0x100,
            "the save's own room left"
        );
        // An RE 1 block, and a trailer of no supported length.
        let mut old = file.clone();
        old.re2 = None;
        assert!(saved_state(&old).is_err());
        let mut cut = file.clone();
        let bytes = cut.re2.as_mut().unwrap();
        bytes.truncate(bytes.len() - 3);
        assert!(saved_state(&cut).is_err());
        // The layout from before the count's high byte still applies.
        let mut older = file;
        let bytes = older.re2.as_mut().unwrap();
        bytes.truncate(bytes.len() - 1);
        assert!(saved_state(&older).is_ok());
    }
}
