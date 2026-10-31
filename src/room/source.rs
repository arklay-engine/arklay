//! The room definition model: everything a `room/<id>/room.xml` holds, read
//! out of an RDT by [`RoomSource::from_rdt`] and built into the engine's
//! [`RoomState`] by [`RoomSource::build`] together with the files the room's
//! folder holds by convention.
//!
//! The model keeps the RDT's raw words wherever the engine reads them raw
//! (shape and collision words, mask sprite words, the footstep surface byte);
//! the XML spells them out as named attributes. It drops what the PC game
//! never reads: the VAB sound banks, the per-room item-icon tiles, the
//! embedded mask TIMs (the mask pages are the folder's `mask/` BMPs) and the
//! dead header fields. The embedded models, effect sprites and
//! player-animation pair become files in the room's folder, named by slot.

use anyhow::{Context, Result, bail};

use crate::charset::Script;
use crate::effects::pool::EffectBlock;
use crate::effects::room::{AnimFrame, FrameEntry, TimGeometry, UvRecord};
use crate::effects::{EffectSprite as StateSprite, RoomEffects, SpriteAnimation, SpriteInfo};
use crate::format::Dialect;
use crate::mask;
use crate::model::Texture8;
use crate::objects::ObjectAsset;
use crate::pack::{AssetClass, Pack};
use crate::state::{
    CameraLight, Collision, Cut, FootstepZone, Light, RoomId, RoomState, Sca, WalkZone, Zone,
    ZoneGate,
};
use crate::{assets, bmp, budget, emd, rdt, tim, tmd};

/// The room definition's file name inside the room's folder.
pub const XML_FILE: &str = "room.xml";

/// The highest model slot count a table declares (the RDT's count byte).
const MAX_MODEL_SLOTS: usize = u8::MAX as usize;

/// The index of the room's player-animation pair in `anim/`.
const PLAYER_ANIMATION: usize = 0;
/// RE2's enemy slots, whose room animations follow the player's.
const CHARACTER_ANIMATIONS: usize = 1;
const ENEMY_SLOTS: usize = 16;
/// `splayer_work`'s, after the enemy slots' ([`crate::scd2::host::SPLAYER_SLOT`]).
const SPLAYER_ANIMATION: usize = ENEMY_SLOTS;

/// Free entry of the effect sprite index table.
const FREE_SPRITE: u8 = 0xFF;

/// One room definition.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RoomSource {
    /// Ambient light colour, 12-bit per channel.
    pub ambient: [i16; 3],
    /// The three room lights.
    pub lights: [Light; 3],
    /// The camera cuts, in camera order.
    pub cameras: Vec<Camera>,
    /// Camera switch zones in flat `zoneIndex` order, group headers included:
    /// SCD `cut_zone_set` indexes this table.
    pub zones: Vec<Zone>,
    /// RE2's gates on `zones`, one per zone; empty for an RE1 room.
    pub zone_gates: Vec<ZoneGate>,
    /// Collision boundaries by quadrant.
    pub collision: Collision,
    /// RE2 collision records, from an RE2 room's SCA section.
    pub sca: Option<Sca>,
    /// The game the room comes from, which picks how the rest of its folder
    /// (scripts, masks) is read.
    pub dialect: Dialect,
    /// Walkable zones.
    pub walk_zones: Vec<WalkZone>,
    /// The footstep table's header word: its record count in every shipped
    /// room, though the lookup ignores it and scans every zone.
    pub footstep_count: u16,
    /// Footstep zones in table order; the last is the catch-all.
    pub footstep_zones: Vec<FootstepZone>,
    /// The object-model table's slot count when it is longer than its last
    /// slot with a model (`object/<slot>.tmd`): trailing null slots the SCD
    /// still sees. `None` when the files give the count.
    pub object_slots: Option<usize>,
    /// The item-model table's slot count, like [`RoomSource::object_slots`].
    pub item_slots: Option<usize>,
    /// The files the slots of each kind read, where `room.xml` names them.
    pub slots: Slots,
    /// The room's display strings, `(language tag, key, value)`: `name` (the
    /// debug menu and save list name) and `save_name` (the save screen's
    /// label). They reach the game as the `ROOM_<room3>_NAME` and
    /// `ROOM_<room3>_NAME_SAVE` language keys ([`super::language_layers`]).
    pub strings: Vec<crate::language::XmlString>,
}

/// The files one room variant's slots read from the room's `object`,
/// `item`, `effect` and `anim` folders, as `room.xml`'s `<slots>` names them
/// ([`SlotFiles`]). A kind with no list reads every file of its folder in
/// order, slot `n` from file `n`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Slots {
    pub object: Option<SlotFiles>,
    pub item: Option<SlotFiles>,
    pub effect: Option<SlotFiles>,
    /// The player animation's one slot.
    pub anim: Option<SlotFiles>,
}

/// The file each slot of a kind reads, by slot: `Some(3)` reads `03` in
/// the kind's folder (`item/03.tmd`), `None` leaves the slot empty. The list
/// is as long as the variant's table.
pub type SlotFiles = Vec<Option<u8>>;

impl Slots {
    /// The kinds a room folder numbers its files by slot.
    pub const KINDS: [&'static str; 4] = ["object", "item", "effect", "anim"];

    /// The list of `kind`, one of [`Slots::KINDS`].
    pub fn get(&self, kind: &str) -> Option<&SlotFiles> {
        match kind {
            "object" => self.object.as_ref(),
            "item" => self.item.as_ref(),
            "effect" => self.effect.as_ref(),
            "anim" => self.anim.as_ref(),
            _ => None,
        }
    }

    /// The list of `kind`, one of [`Slots::KINDS`], to set.
    pub fn get_mut(&mut self, kind: &str) -> Option<&mut Option<SlotFiles>> {
        match kind {
            "object" => Some(&mut self.object),
            "item" => Some(&mut self.item),
            "effect" => Some(&mut self.effect),
            "anim" => Some(&mut self.anim),
            _ => None,
        }
    }

    /// The file slot `slot` of `kind` reads: its listed file, else the
    /// slot's own number when the kind has no list.
    pub fn file(&self, kind: &str, slot: usize) -> Option<usize> {
        match self.get(kind) {
            Some(files) => files.get(slot).copied().flatten().map(usize::from),
            None => Some(slot),
        }
    }
}

/// One camera cut.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Camera {
    pub pos: [i32; 3],
    pub look_at: [i32; 3],
    pub roll: i32,
    pub fov: i32,
    /// The mask placement table, group by group.
    pub mask_groups: Vec<MaskGroup>,
    /// The camera's own lighting (an RE2 room's LIT record).
    pub light: Option<CameraLight>,
}

/// One mask group: its header words and its sprites.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MaskGroup {
    /// Texture page/CLUT bits.
    pub tex_bits: u16,
    /// Raw `originX`/`originY`, added to every sprite's screen delta.
    pub origin: (i16, i16),
    pub sprites: Vec<MaskSprite>,
}

/// One mask sprite record.
///
/// An RE1 sprite's `flags` is its sprite word, whose high nibble packs a
/// square size; an RE2 sprite's is its size word: the square's side, or 0
/// for an explicit size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MaskSprite {
    /// Texel origin in the mask page.
    pub uv: (u8, u8),
    /// Screen delta from the group origin.
    pub offset: (u8, u8),
    /// The depth/brightness word.
    pub depth: u16,
    /// Raw flags (RE1) or size word (RE2).
    pub flags: u16,
    /// The explicit size, present exactly when [`MaskSprite::packs_size`]
    /// is false.
    pub size: Option<(u16, u16)>,
}

impl MaskSprite {
    /// Whether `flags` carries the size, so the record has no explicit one.
    pub fn packs_size(flags: u16, dialect: Dialect) -> bool {
        match dialect {
            Dialect::Re1 => flags & 0xF000 != 0,
            Dialect::Re2 => flags != 0,
        }
    }

    /// The sprite's pixel size: explicit, or packed in the flags.
    fn pixel_size(&self, dialect: Dialect) -> (u16, u16) {
        match (self.size, dialect) {
            (Some(size), _) => size,
            (None, Dialect::Re1) => {
                let size = (self.flags & 0xF1FF) >> 9;
                (size, size)
            }
            (None, Dialect::Re2) => (self.flags, self.flags),
        }
    }
}

/// The stage digit whose Japanese text the game draws with `PBFONT3N.TIM`
/// (its stage byte 4).
const STAGE_FIVE: u8 = 5;

/// An RDT message block as the game draws it: a Japanese stage-5 block has
/// each stream moved onto the stage-five kanji rows
/// ([`crate::charset::stage_five_text`]), any other is unchanged. A block
/// whose offset table is not the shipped ascending form is kept as it is.
pub(super) fn drawn_message_block(block: &[u8], script: Script, stage: u8) -> Vec<u8> {
    if script != Script::Japanese || stage != STAGE_FIVE {
        return block.to_vec();
    }
    let Some(first) = block
        .get(..2)
        .map(|raw| usize::from(u16::from_le_bytes([raw[0], raw[1]])))
    else {
        return block.to_vec();
    };
    if first == 0 || first % 2 != 0 || first > block.len() {
        return block.to_vec();
    }
    let offsets: Vec<usize> = block[..first]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|raw| usize::from(u16::from_le_bytes(*raw)))
        .collect();
    if offsets.windows(2).any(|pair| pair[0] >= pair[1])
        || offsets.last().is_some_and(|&last| last > block.len())
    {
        return block.to_vec();
    }
    // The remap rewrites operands in place, so each stream keeps its length.
    let mut drawn = block.to_vec();
    for (index, &start) in offsets.iter().enumerate() {
        let end = offsets.get(index + 1).copied().unwrap_or(block.len());
        drawn[start..end].copy_from_slice(&crate::charset::stage_five_text(&block[start..end]));
    }
    drawn
}

/// One declared effect sprite slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectSlot {
    /// The sprite index scripts spawn the sprite by.
    pub index: u8,
    /// The sprite's metadata; `None` when the RDT's record is malformed.
    pub sprite: Option<EffectSprite>,
}

/// One effect sprite's metadata; the art is the slot's `effect/<slot>` sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectSprite {
    /// The page/CLUT descriptor word.
    pub page_clut_word: u16,
    pub frames: Vec<FrameEntry>,
    pub uvs: Vec<UvRecord>,
    /// The raw eight-entry depth table.
    pub depth_table: [u8; 8],
    /// The frames each depth row resolves to; rows the table marks unused
    /// stay empty.
    pub depth_frames: [Vec<AnimFrame>; 8],
}

/// Which model table a slot belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelTable {
    Object,
    Item,
}

impl ModelTable {
    /// The table's folder in a room folder.
    fn kind(self) -> &'static str {
        match self {
            Self::Object => "object",
            Self::Item => "item",
        }
    }
}

/// Pack stem of slot `slot` in a room's `kind` folder `dir`
/// (`room/100/item` -> `room/100/item/00`).
fn slot_stem(dir: &str, slot: usize) -> String {
    format!("{dir}/{slot:02x}")
}

/// A room converted out of an RDT: its definition and the other files of its
/// folder (`room/<id>/...`) as `(pack entry, bytes)`, besides the camera
/// images.
#[derive(Debug, Clone)]
pub struct Converted {
    pub source: RoomSource,
    pub files: Vec<(String, Vec<u8>)>,
}

/// The file a model texture TIM is stored as: an indexed BMP when it holds the
/// decoded texture exactly, else the TIM itself.
fn model_texture_file(tim: &[u8]) -> (&'static str, Vec<u8>) {
    let exact = tim::decode_8bpp(tim).ok().and_then(|texture| {
        let page = bmp::encode_indexed_page(&texture, 0).ok()?;
        let back = bmp::decode_indexed_page(&page).ok()?;
        (back == texture).then_some(page)
    });
    match exact {
        Some(page) => ("bmp", page),
        None => ("tim", tim.to_vec()),
    }
}

/// The file an effect sprite's 4bpp TIM is stored as: a CLUT sheet BMP when
/// it reads back as exactly the TIM's texture and geometry, else the TIM.
fn effect_sheet_file(tim: &[u8]) -> (&'static str, Vec<u8>) {
    let expected = crate::effects::room::parse_tim(tim);
    let exact = expected.1.as_ref().and_then(|texture| {
        let sheet = bmp::encode_clut_sheet(texture, usize::from(expected.0.clut_rows)).ok()?;
        (decode_effect_sheet(&sheet).ok()? == expected).then_some(sheet)
    });
    match exact {
        Some(sheet) => ("bmp", sheet),
        None => ("tim", tim.to_vec()),
    }
}

/// An effect sheet BMP's texture and the geometry a TIM of it would have.
fn decode_effect_sheet(data: &[u8]) -> Result<(TimGeometry, Option<Texture8>)> {
    let (texture, rows) = bmp::decode_clut_sheet(data)?;
    let geometry = TimGeometry {
        width: u16::try_from(texture.width).context("effect sheet is too wide")?,
        height: u16::try_from(texture.height).context("effect sheet is too tall")?,
        clut_rows: rows as u8,
    };
    Ok((geometry, Some(texture)))
}

/// Read a little-endian `u32`.
fn u32_at(data: &[u8], offset: usize) -> Option<u32> {
    let raw = data.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes(raw.try_into().unwrap()))
}

/// Read a little-endian `i32`.
fn i32_at(data: &[u8], offset: usize) -> Option<i32> {
    u32_at(data, offset).map(|value| value as i32)
}

/// What an RE1 RDT embeds besides the room definition: the files of the
/// room's folder (model pairs, effect sheets, the player animation) and the
/// effect definitions and model table lengths that go with them.
struct Embedded {
    /// The models, effect sheets and player animation, as `(pack entry,
    /// bytes)`.
    files: Vec<(String, Vec<u8>)>,
    /// Each effect slot's definition, `effect/<slot>.xml`.
    effects: Vec<(String, Vec<u8>)>,
    object_slots: Option<usize>,
    item_slots: Option<usize>,
}

impl Embedded {
    /// Read what RDT `data`, parsed as `state`, embeds, its files named in
    /// room folder `dir` (`""` for paths relative to the folder).
    fn read(data: &[u8], dir: &str, state: &RoomState) -> Self {
        let kind = |kind: &str| {
            if dir.is_empty() {
                kind.to_string()
            } else {
                format!("{dir}/{kind}")
            }
        };
        let pointers = rdt::pointers(data);
        let mut files = Vec::new();
        let mut effects = Vec::new();

        let mut models = |table: ModelTable, pointer: u32, count: u8| {
            let mut used = 0;
            for slot in 0..usize::from(count) {
                let Some((tmd, tim)) = model_pair(data, pointer, slot) else {
                    continue;
                };
                let stem = slot_stem(&kind(table.kind()), slot);
                files.push((format!("{stem}.tmd"), tmd.to_vec()));
                if let Some(tim) = tim {
                    let (extension, data) = model_texture_file(tim);
                    files.push((format!("{stem}.{extension}"), data));
                }
                used = slot + 1;
            }
            (usize::from(count) > used).then_some(usize::from(count))
        };
        let object_slots = models(
            ModelTable::Object,
            pointers[OBJECT_MODELS_SLOT],
            state.omodel_slot_count,
        );
        let item_slots = models(
            ModelTable::Item,
            pointers[ITEM_MODELS_SLOT],
            state.item_count,
        );

        for (slot, (effect, tim)) in effect_slots(data, &pointers, &state.effects)
            .into_iter()
            .enumerate()
        {
            let stem = slot_stem(&kind("effect"), slot);
            effects.push((
                format!("{stem}.xml"),
                super::xml::write_effect(&effect).into_bytes(),
            ));
            if let Some(tim) = tim {
                let (extension, data) = effect_sheet_file(tim);
                files.push((format!("{stem}.{extension}"), data));
            }
        }
        // A pair that does not parse (room 5020 ships one) leaves the RDT on
        // the no-op stance, so the room has none.
        if let Some((emr, edd)) = rdt::room_anim_bounds(data)
            .and_then(|(header, base, bound)| {
                Some((data.get(header..base)?, data.get(base..bound)?))
            })
            .filter(|(emr, edd)| emd::parse_room_anim_files(emr, edd).is_ok())
        {
            let stem = slot_stem(&kind("anim"), PLAYER_ANIMATION);
            files.push((format!("{stem}.emr"), emr.to_vec()));
            files.push((format!("{stem}.edd"), edd.to_vec()));
        }
        Self {
            files,
            effects,
            object_slots,
            item_slots,
        }
    }
}

/// The game art RE1 RDT `data` of room `id` holds for the room's folder, as
/// `(path in the folder, bytes)` (`item/00.tmd`): the object and item model
/// pairs, the effect sprite sheets and the player-animation pair. The room's
/// definition (`room.xml`, the effect definitions, the scripts) is not read.
pub fn rdt_assets(data: &[u8], id: RoomId) -> Result<Vec<(String, Vec<u8>)>> {
    let state = rdt::parse(data, id)?;
    Ok(Embedded::read(data, "", &state).files)
}

impl RoomSource {
    /// Read an RDT into a room definition and the files of its folder (the
    /// RDT's own, `room/1001/`, since an RDT is one variant's): the model
    /// pairs, effect sprites, player animation and scripts. The RDT's
    /// message block is not kept: a room's text is its `<strings>`.
    pub fn from_rdt(data: &[u8], id: RoomId) -> Result<Converted> {
        let state = rdt::parse(data, id)?;
        let tables = rdt::mask_tables(data)?;
        let cameras = state
            .cuts
            .iter()
            .zip(&tables)
            .map(|(cut, table)| camera_source(cut, table))
            .collect();
        let pointers = rdt::pointers(data);
        let Embedded {
            mut files,
            effects,
            object_slots,
            item_slots,
        } = Embedded::read(data, &id.rdt_dir(), &state);
        files.extend(effects);
        match crate::scd::reader::extract_container(data) {
            Ok(Some(container)) => {
                files.push((format!("{}/{}", id.rdt_dir(), super::SCD_FILE), container))
            }
            Ok(None) => {}
            // A stub RDT carries no script pointers at all; a real room whose
            // scripts do not read is not one the XML can carry.
            Err(_) if state.cuts.is_empty() => {}
            Err(error) => return Err(error.context("the room's scripts do not read")),
        }

        let source = Self {
            ambient: state.ambient,
            lights: state.lights,
            cameras,
            zones: state.zones,
            zone_gates: Vec::new(),
            collision: state.collision,
            sca: None,
            dialect: Dialect::Re1,
            walk_zones: state.walk_zones,
            footstep_count: footstep_count(data, pointers[FOOTSTEP_SLOT]),
            footstep_zones: state.footstep_zones,
            object_slots,
            item_slots,
            slots: Slots::default(),
            strings: Vec::new(),
        };
        Ok(Converted { source, files })
    }

    /// Build the engine's room state, reading the room's models, effect
    /// sprites and player animation from its folder in `pack`.
    ///
    /// Like the RDT parser, a damaged model, effect sheet or animation never
    /// fails the room: a model pair that does not load is skipped with a
    /// warning in [`RoomState::model_warnings`], an effect sheet that does
    /// not load leaves its sprite without art, and an animation pair that
    /// does not load leaves the room on the no-op stance with a model
    /// warning. An effect definition that does not parse fails the room, like
    /// a broken `room.xml`.
    pub fn build(&self, id: RoomId, pack: &Pack) -> Result<RoomState> {
        let cuts = self
            .cameras
            .iter()
            .enumerate()
            .map(|(index, camera)| build_cut(camera, index, self.dialect))
            .collect::<Result<Vec<_>>>()?;
        budget::check_len(
            self.zones.len(),
            budget::MAX_RECORDS,
            "camera switch zone count",
        )?;
        budget::check_len(
            self.footstep_zones.len(),
            budget::MAX_RECORDS,
            "footstep zone count",
        )?;
        let records: usize = self.collision.quadrants.iter().map(Vec::len).sum();
        budget::check_len(records, budget::MAX_RECORDS, "collision record count")?;

        let dir = super::Folder::of(pack, id).dir().to_string();
        let kind = |kind: &str| format!("{dir}/{kind}");
        let mut model_warnings = Vec::new();
        let (omodel_slot_count, object_models) = build_models(
            ModelTable::Object,
            self.object_slots,
            self.slots.object.as_deref(),
            &kind("object"),
            pack,
            &mut model_warnings,
        )?;
        let (item_count, item_models) = build_models(
            ModelTable::Item,
            self.item_slots,
            self.slots.item.as_deref(),
            &kind("item"),
            pack,
            &mut model_warnings,
        )?;
        let anim = self
            .slots
            .file("anim", PLAYER_ANIMATION)
            .map(|file| slot_stem(&kind("anim"), file));
        let room_anim = anim
            .filter(|anim| pack.contains(&format!("{anim}.emr")))
            .and_then(|anim| {
                build_anim(&anim, pack, self.dialect)
                    .map_err(|error| model_warnings.push(format!("animation {anim}: {error:#}")))
                    .ok()
            });

        let character_anims = if self.dialect == Dialect::Re2 {
            (0..=SPLAYER_ANIMATION)
                .filter_map(|slot| {
                    let stem = slot_stem(&kind("anim"), CHARACTER_ANIMATIONS + slot);
                    pack.contains(&format!("{stem}.emr"))
                        .then(|| build_anim(&stem, pack, self.dialect).ok())
                        .flatten()
                        .map(|anim| (slot as u8, anim))
                })
                .collect()
        } else {
            Vec::new()
        };

        Ok(RoomState {
            stage: id.stage,
            mansion2: id.mansion2,
            room: id.room,
            player_flag: id.player_flag,
            omodel_slot_count,
            item_count,
            cuts,
            current_cut: 0,
            room_sfx: 0,
            ambient: self.ambient,
            lights: self.lights,
            collision: self.collision.clone(),
            sca: self.sca.clone(),
            zones: self.zones.clone(),
            zone_gates: self.zone_gates.clone(),
            walk_zones: self.walk_zones.clone(),
            footstep_zones: self.footstep_zones.clone(),
            messages: None,
            effects: build_effects(&kind("effect"), self.slots.effect.as_deref(), pack)?,
            object_models,
            item_models,
            room_anim,
            character_anims,
            model_warnings,
        })
    }
}

/// RDT pointer slot of the object-model pair table.
const OBJECT_MODELS_SLOT: usize = 2;
/// RDT pointer slot of the item-model pair table.
const ITEM_MODELS_SLOT: usize = 3;
/// RDT pointer slot of the footstep zone table.
const FOOTSTEP_SLOT: usize = 5;
/// RDT pointer slots of the effect index, sprite-info and TIM tables.
const EFFECT_SLOTS: [usize; 3] = [13, 14, 15];

/// The footstep table's header word, 0 when the room has no table.
fn footstep_count(data: &[u8], pointer: u32) -> u16 {
    if pointer == 0 {
        return 0;
    }
    data.get(pointer as usize..pointer as usize + 2)
        .map_or(0, |raw| u16::from_le_bytes([raw[0], raw[1]]))
}

/// One camera's definition from its parsed cut and raw mask table.
fn camera_source(cut: &Cut, table: &mask::MaskTable) -> Camera {
    Camera {
        pos: cut.pos,
        look_at: cut.look_at,
        roll: cut.roll,
        fov: cut.fov,
        mask_groups: mask_groups(table, Dialect::Re1),
        light: cut.light,
    }
}

/// A parsed mask table as the camera definition's groups.
pub(crate) fn mask_groups(table: &mask::MaskTable, dialect: Dialect) -> Vec<MaskGroup> {
    let mut sprites = table.sprites.iter();
    table
        .groups
        .iter()
        .map(|group| MaskGroup {
            tex_bits: group.tex_bits,
            origin: group.origin,
            sprites: sprites
                .by_ref()
                .take(usize::from(group.sprite_count))
                .map(|sprite| MaskSprite {
                    uv: sprite.uv,
                    offset: (
                        (sprite.pos.0 - i32::from(group.origin.0)) as u8,
                        (sprite.pos.1 - i32::from(group.origin.1)) as u8,
                    ),
                    depth: sprite.pos_data,
                    flags: sprite.flags,
                    size: (!MaskSprite::packs_size(sprite.flags, dialect)).then_some(sprite.size),
                })
                .collect(),
        })
        .collect()
}

/// Slot `slot`'s `{TMD, TIM}` pair in the model table at `pointer`, each cut
/// to its own length; `None` when the slot names no loadable mesh.
fn model_pair(data: &[u8], pointer: u32, slot: usize) -> Option<(&[u8], Option<&[u8]>)> {
    if pointer == 0 {
        return None;
    }
    let at = pointer as usize + slot * 8;
    let tmd_pointer = u32_at(data, at)? as usize;
    let tim_pointer = u32_at(data, at + 4)? as usize;
    if tmd_pointer == 0 {
        return None;
    }
    let tmd_bytes = data.get(tmd_pointer..)?;
    let tmd_bytes = &tmd_bytes[..tmd::byte_len(tmd_bytes).ok()?];
    let tim_bytes = (tim_pointer != 0)
        .then(|| data.get(tim_pointer..))
        .flatten()
        .and_then(|tim| Some(&tim[..tim::byte_len(tim).ok()?]));
    Some((tmd_bytes, tim_bytes))
}

/// The effect slots of an RDT, each with its sprite's 4bpp sheet TIM (cut to
/// its own length; `None` when the RDT's TIM is malformed).
fn effect_slots<'a>(
    data: &'a [u8],
    pointers: &[u32; 19],
    parsed: &RoomEffects,
) -> Vec<(EffectSlot, Option<&'a [u8]>)> {
    let [index_ptr, _, tim_ptr] = EFFECT_SLOTS.map(|slot| pointers[slot]);
    if EFFECT_SLOTS.iter().any(|&slot| pointers[slot] == 0) {
        return Vec::new();
    }
    let mut slots = Vec::new();
    for slot in 0..crate::effects::room::EFFECT_SPRITE_SLOTS {
        let Some(&index) = data.get(index_ptr as usize + slot) else {
            break;
        };
        if index == FREE_SPRITE {
            break;
        }
        let Some(sprite) = parsed.sprite(index) else {
            slots.push((
                EffectSlot {
                    index,
                    sprite: None,
                },
                None,
            ));
            continue;
        };
        let sheet = (tim_ptr as usize)
            .checked_sub(4 * slot)
            .and_then(|at| i32_at(data, at))
            .and_then(|offset| usize::try_from(offset).ok())
            .and_then(|offset| data.get(offset..))
            .and_then(|tim| Some(&tim[..tim::byte_len(tim).ok()?]));
        let effect = EffectSprite {
            page_clut_word: sprite.info.page_clut_word,
            frames: sprite.info.frames.clone(),
            uvs: sprite.info.uvs.clone(),
            depth_table: sprite.anim.depth_table,
            depth_frames: sprite.anim.depth_frames.clone(),
        };
        slots.push((
            EffectSlot {
                index,
                sprite: Some(effect),
            },
            sheet,
        ));
    }
    slots
}

/// Build one cut, laying the mask sprites out exactly like the RDT parser
/// of its dialect.
fn build_cut(camera: &Camera, index: usize, dialect: Dialect) -> Result<Cut> {
    if camera.mask_groups.len() > budget::MAX_MASK_GROUPS {
        bail!(
            "camera {index} has {} mask groups, more than the supported {}",
            camera.mask_groups.len(),
            budget::MAX_MASK_GROUPS
        );
    }
    let mut table = mask::MaskTable::default();
    for (group_index, group) in camera.mask_groups.iter().enumerate() {
        let sprite_count = u16::try_from(group.sprites.len())
            .with_context(|| format!("camera {index} mask group {group_index} is too large"))?;
        table.groups.push(mask::MaskGroup {
            sprite_count,
            tex_bits: group.tex_bits,
            origin: group.origin,
        });
        for sprite in &group.sprites {
            if MaskSprite::packs_size(sprite.flags, dialect) == sprite.size.is_some() {
                bail!(
                    "camera {index} mask group {group_index}: a sprite has an explicit size \
                     exactly when its flags do not pack one"
                );
            }
            budget::check_len(
                table.sprites.len() + 1,
                budget::MAX_MASK_SPRITES,
                "mask sprite count",
            )?;
            table.sprites.push(mask::MaskSprite {
                uv: sprite.uv,
                pos: (
                    i32::from(group.origin.0) + i32::from(sprite.offset.0),
                    i32::from(group.origin.1) + i32::from(sprite.offset.1),
                ),
                size: sprite.pixel_size(dialect),
                pos_data: sprite.depth,
                flags: sprite.flags,
                // RE2's `Psp_set` numbers the groups from the last.
                group: match dialect {
                    Dialect::Re1 => group_index + 1,
                    Dialect::Re2 => camera.mask_groups.len() - group_index,
                } as u8,
            });
        }
    }
    Ok(Cut {
        index,
        pos: camera.pos,
        look_at: camera.look_at,
        roll: camera.roll,
        fov: camera.fov,
        background: None,
        mask_group_count: table.groups.len() as u8,
        mask_active: table.active_bits(),
        masks: table.sprites,
        mask_dialect: dialect,
        light: camera.light,
    })
}

/// A decoded model pair by stem.
/// Load model table `table`'s slots from the room's folder: its slot count
/// (the declared `slots`, or one past the last slot with a `.tmd`) and every
/// slot that builds. A slot whose pair has no texture is declared but
/// unbuilt, like an RDT pair with a null TIM half.
fn build_models(
    table: ModelTable,
    slots: Option<usize>,
    files: Option<&[Option<u8>]>,
    dir: &str,
    pack: &Pack,
    warnings: &mut Vec<String>,
) -> Result<(u8, Vec<ObjectAsset>)> {
    let has_model = |file: usize| pack.contains(&format!("{}.tmd", slot_stem(dir, file)));
    // `(slot, file)` of each slot with a model.
    let present: Vec<(usize, usize)> = match files {
        Some(files) => files
            .iter()
            .enumerate()
            .filter_map(|(slot, file)| Some((slot, usize::from((*file)?))))
            .filter(|&(_, file)| has_model(file))
            .collect(),
        None => (0..MAX_MODEL_SLOTS)
            .filter(|&file| has_model(file))
            .map(|file| (file, file))
            .collect(),
    };
    let count = present
        .last()
        .map_or(0, |(last, _)| last + 1)
        .max(files.map_or(0, <[_]>::len))
        .max(slots.unwrap_or(0));
    let count = u8::try_from(count).with_context(|| {
        format!("more than {MAX_MODEL_SLOTS} {table:?} model slots").to_lowercase()
    })?;
    let mut assets = Vec::new();
    for (slot, file) in present {
        let stem = slot_stem(dir, file);
        match load_model(&stem, pack) {
            Ok(Some((model, texture))) => assets.push(ObjectAsset {
                pair_index: slot,
                hd_name: Some(stem),
                model,
                texture,
            }),
            Ok(None) => {}
            Err(error) => warnings.push(format!("model {stem}: {error:#}")),
        }
    }
    Ok((count, assets))
}

/// Load model `stem`'s mesh and texture; `Ok(None)` when it has no texture.
fn load_model(stem: &str, pack: &Pack) -> Result<Option<(crate::model::Tmd, Texture8)>> {
    let tmd_entry = format!("{stem}.tmd");
    let data = pack
        .read(&tmd_entry)
        .with_context(|| format!("missing {tmd_entry}"))?;
    let magic = u32_at(&data, 0).context("TMD is truncated")?;
    if magic != 0x41 {
        bail!("bad TMD magic 0x{magic:08X}");
    }
    let model = tmd::parse(&data)?;
    let Some(asset) = pack.read_asset(stem, AssetClass::Image)? else {
        return Ok(None);
    };
    // The converter's indexed page keeps the TIM's indices and STP bits;
    // read it as such whatever its entry 0 is. Any other image is a mod's
    // replacement, re-indexed like the other indexed textures.
    let texture = if asset.extension() == "bmp" {
        bmp::decode_indexed_page(asset.data())
    } else {
        assets::decode_texture(&asset, assets::Geometry::Free)
    }
    .with_context(|| format!("invalid {}", asset.path()))?;
    Ok(Some((model, texture)))
}

/// Build the room's effect sprites from its `effect` folder `dir`: for each
/// slot of `files` (without a list, from 0 up to the first one missing), its
/// file's `<file>.xml` definition, drawn from the `<file>` sheet (a CLUT
/// sheet BMP or a 4bpp TIM).
fn build_effects(dir: &str, files: Option<&[Option<u8>]>, pack: &Pack) -> Result<RoomEffects> {
    let mut effects = RoomEffects::default();
    for slot in 0..crate::effects::room::EFFECT_SPRITE_SLOTS {
        let file = match files {
            Some(files) => match files.get(slot) {
                Some(Some(file)) => usize::from(*file),
                Some(None) => continue,
                None => break,
            },
            None => slot,
        };
        let stem = slot_stem(dir, file);
        let entry = format!("{stem}.xml");
        if !pack.contains(&entry) {
            if files.is_some() {
                effects
                    .warnings
                    .push(format!("effect slot {slot}: missing {entry}"));
                continue;
            }
            break;
        }
        let data = pack.read(&entry)?;
        let text = std::str::from_utf8(&data).with_context(|| format!("{entry} is not UTF-8"))?;
        let declared =
            super::xml::parse_effect(text).with_context(|| format!("invalid {entry}"))?;
        effects.index[slot] = declared.index;
        let Some(sprite) = declared.sprite else {
            effects.warnings.push(format!(
                "effect sprite {}: no sprite record",
                declared.index
            ));
            continue;
        };
        let (geometry, texture) = match pack.read_asset(&stem, AssetClass::Image) {
            Ok(Some(asset)) => {
                let sheet = match asset.extension().as_str() {
                    "tim" => Ok(crate::effects::room::parse_tim(asset.data())),
                    "bmp" => decode_effect_sheet(asset.data()),
                    other => Err(anyhow::anyhow!(
                        "a .{other} sheet is not supported (bmp or tim)"
                    )),
                };
                sheet.unwrap_or_else(|error| {
                    effects
                        .warnings
                        .push(format!("effect sheet {}: {error:#}", asset.path()));
                    Default::default()
                })
            }
            Ok(None) => Default::default(),
            Err(error) => {
                effects
                    .warnings
                    .push(format!("effect sheet {stem}: {error:#}"));
                Default::default()
            }
        };
        effects.sprites.push(StateSprite {
            index: declared.index,
            info: SpriteInfo {
                uv_count: sprite.uvs.len() as u8,
                frames: sprite.frames,
                uvs: sprite.uvs,
                page_clut_word: sprite.page_clut_word,
                page_id: 0,
                page_v: 0,
            },
            anim: SpriteAnimation {
                depth_table: sprite.depth_table,
                depth_frames: sprite.depth_frames,
            },
            tim: texture,
            geometry,
        });
    }
    Ok(effects)
}

/// Load the room's player-animation pair `stem` (`.emr` + `.edd`).
fn build_anim(stem: &str, pack: &Pack, dialect: Dialect) -> Result<crate::model::RoomAnim> {
    let read = |extension: &str| pack.read(&format!("{stem}.{extension}"));
    let (emr, edd) = (read("emr")?, read("edd")?);
    match dialect {
        Dialect::Re1 => emd::parse_room_anim_files(&emr, &edd),
        Dialect::Re2 => crate::emd2::parse_room_anim(&emr, &edd),
    }
}

/// The pack entries of an RE2 room's own player animations, as `(entry, bytes)`.
pub(super) fn re2_anim_files(data: &[u8], id: RoomId) -> Vec<(String, Vec<u8>)> {
    let dir = format!("{}/anim", id.rdt_dir());
    let mut files = Vec::new();
    let mut add = |pair: Option<(&[u8], &[u8])>, slot: usize| {
        if let Some((emr, edd)) =
            pair.filter(|(emr, edd)| crate::emd2::parse_room_anim(emr, edd).is_ok())
        {
            let stem = slot_stem(&dir, slot);
            files.push((format!("{stem}.emr"), emr.to_vec()));
            files.push((format!("{stem}.edd"), edd.to_vec()));
        }
    };
    add(crate::rdt2::room_anim(data), PLAYER_ANIMATION);
    for slot in 0..ENEMY_SLOTS {
        add(
            crate::rdt2::character_anim(data, slot as u8),
            CHARACTER_ANIMATIONS + slot,
        );
    }
    add(
        crate::rdt2::splayer_anim(data),
        CHARACTER_ANIMATIONS + SPLAYER_ANIMATION,
    );
    files
}

/// An animation frame from its raw 24-byte blocks.
pub(crate) fn anim_frame(blocks: Vec<[u8; 24]>) -> AnimFrame {
    AnimFrame {
        blocks: blocks.into_iter().map(EffectBlock).collect(),
    }
}
