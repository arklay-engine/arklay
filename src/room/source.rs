//! The room definition model: everything a `room/<id>.xml` holds, read out of
//! an RDT by [`RoomSource::from_rdt`] and built into the engine's
//! [`RoomState`] by [`RoomSource::build`].
//!
//! The model keeps the RDT's raw words wherever the engine reads them raw
//! (flags, shape words, the footstep surface byte), and drops what the PC
//! game never reads: the VAB sound banks, the per-room item-icon tiles, the
//! embedded mask TIMs (the mask pages are `roommask/` BMPs) and the dead
//! header fields. The embedded models, effect sheets and player-animation
//! pair become shared sidecar files ([`Sidecars`]) the definition names.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

use anyhow::{Context, Result, bail};

use crate::charset::Script;
use crate::effects::pool::EffectBlock;
use crate::effects::room::{AnimFrame, FrameEntry, UvRecord};
use crate::effects::{EffectSprite as StateSprite, RoomEffects, SpriteAnimation, SpriteInfo};
use crate::mask;
use crate::model::Texture8;
use crate::objects::ObjectAsset;
use crate::pack::{AssetClass, Pack};
use crate::state::{Collision, Cut, FootstepZone, Light, RoomId, RoomState, WalkZone, Zone};
use crate::{assets, bmp, budget, emd, rdt, tim, tmd};

/// Pack directory of the shared room model pairs (`prop/<id>.tmd` + `.bmp`).
pub const PROP_DIR: &str = "prop/";
/// Pack entry of the prop manifest: which rooms and slots use each prop.
pub const PROP_MANIFEST: &str = "prop/props.toml";
/// Pack directory of the room effect sprite sheets (`effect/<id>.tim`).
pub const EFFECT_DIR: &str = "effect/";
/// Pack directory of the room player-animation pairs (`.emr` + `.edd`).
pub const ANIMATION_DIR: &str = "animation/";

/// Sidecar ids are eight hex digits: the converter names each file by the
/// CRC-32 of its content. A mod adding its own props, sheets or animations
/// may use any id no other file has.
const ID_DIGITS: usize = 8;

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
    /// Collision boundaries by quadrant.
    pub collision: Collision,
    /// Walkable zones.
    pub walk_zones: Vec<WalkZone>,
    /// The footstep table's header word: its record count in every shipped
    /// room, though the lookup ignores it and scans every zone.
    pub footstep_count: u16,
    /// Footstep zones in table order; the last is the catch-all.
    pub footstep_zones: Vec<FootstepZone>,
    /// The room's message block, absent when the RDT has none.
    pub messages: Option<Messages>,
    /// The effect sprite index table's declared slots, up to the first free
    /// one.
    pub effects: Vec<EffectSlot>,
    /// Object-model slots: the prop stem each holds, `None` for a null slot.
    /// The table's length is the SCD-visible slot count.
    pub object_models: Vec<Option<String>>,
    /// Item-model slots, like [`RoomSource::object_models`].
    pub item_models: Vec<Option<String>>,
    /// Stem of the room's player-animation pair (`animation/<id>`).
    pub player_animation: Option<String>,
    /// Pack entry of the room's own SCD container (init, per-frame and event
    /// scripts), e.g. `room/1000.scd`.
    pub scripts: Option<String>,
}

/// One camera cut.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Camera {
    pub pos: [i32; 3],
    pub look_at: [i32; 3],
    pub roll: i32,
    pub fov: i32,
    /// Background stem; `None` is the room's own `roomcut/<room>_<camera>`.
    pub background: Option<String>,
    /// Mask page stem; `None` is the room's own `roommask/<room>_<camera>`.
    pub mask: Option<String>,
    /// The mask placement table, group by group.
    pub mask_groups: Vec<MaskGroup>,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MaskSprite {
    /// Texel origin in the mask page.
    pub uv: (u8, u8),
    /// Screen delta from the group origin.
    pub offset: (u8, u8),
    /// The depth/brightness word.
    pub depth: u16,
    /// Raw flags; a non-zero high nibble selects the packed square size.
    pub flags: u16,
    /// The explicit size, present exactly when the flags' high nibble is 0.
    pub size: Option<(u16, u16)>,
}

impl MaskSprite {
    /// The sprite's pixel size: explicit, or packed in the flags.
    fn pixel_size(&self) -> (u16, u16) {
        match self.size {
            Some(size) => size,
            None => {
                let size = (self.flags & 0xF1FF) >> 9;
                (size, size)
            }
        }
    }
}

/// The room message block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Messages {
    /// One encoded stream per offset-table entry, each running to the next
    /// message's start (a message without its own `0x01` runs on into the
    /// next, exactly like the RDT), in the script the text was decoded with.
    Streams {
        script: Script,
        streams: Vec<Vec<u8>>,
    },
    /// A block whose offset table is not a plain ascending list, kept whole.
    Raw(Vec<u8>),
}

/// The stage digit whose Japanese text the game draws with `PBFONT3N.TIM`
/// (its stage byte 4).
const STAGE_FIVE: u8 = 5;

impl Messages {
    /// Split an RDT message block into its streams, or keep it raw when its
    /// offset table is not the shipped ascending form.
    ///
    /// Japanese stage-5 text is moved onto the stage-five kanji rows the game
    /// draws it with ([`crate::charset::stage_five_text`]).
    fn from_block(block: &[u8], script: Script, stage: u8) -> Self {
        let Some(first) = block
            .get(..2)
            .map(|raw| usize::from(u16::from_le_bytes([raw[0], raw[1]])))
        else {
            return Self::Raw(block.to_vec());
        };
        if first == 0 || first % 2 != 0 || first > block.len() {
            return Self::Raw(block.to_vec());
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
            return Self::Raw(block.to_vec());
        }
        let streams = offsets
            .iter()
            .enumerate()
            .map(|(index, &start)| {
                let end = offsets.get(index + 1).copied().unwrap_or(block.len());
                let stream = &block[start..end];
                if script == Script::Japanese && stage == STAGE_FIVE {
                    crate::charset::stage_five_text(stream)
                } else {
                    stream.to_vec()
                }
            })
            .collect();
        Self::Streams { script, streams }
    }

    /// The RDT-form block: the `u16` offset table, then the streams.
    pub fn to_block(&self) -> Result<Vec<u8>> {
        match self {
            Self::Raw(block) => Ok(block.clone()),
            Self::Streams { streams, .. } => {
                let mut block = Vec::new();
                let mut offset = streams.len() * 2;
                for stream in streams {
                    let value =
                        u16::try_from(offset).context("room message block exceeds 64 KiB")?;
                    block.extend_from_slice(&value.to_le_bytes());
                    offset += stream.len();
                }
                u16::try_from(offset).context("room message block exceeds 64 KiB")?;
                for stream in streams {
                    block.extend_from_slice(stream);
                }
                Ok(block)
            }
        }
    }
}

/// An RDT message block as the game draws it: a Japanese stage-5 block has
/// its streams moved onto the stage-five kanji rows, any other is unchanged.
pub(super) fn drawn_message_block(block: &[u8], script: Script, stage: u8) -> Vec<u8> {
    if script != Script::Japanese || stage != STAGE_FIVE {
        return block.to_vec();
    }
    // The remap rewrites operands in place, so the block keeps its length.
    Messages::from_block(block, script, stage)
        .to_block()
        .unwrap_or_else(|_| block.to_vec())
}

/// One declared effect sprite slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectSlot {
    /// The sprite index scripts spawn the sprite by.
    pub index: u8,
    /// The sprite's metadata; `None` when the RDT's record is malformed.
    pub sprite: Option<EffectSprite>,
}

/// One effect sprite's metadata; the art is the named sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectSprite {
    /// Stem of the 4bpp sheet TIM (`effect/<id>`); `None` when the RDT's TIM
    /// is malformed.
    pub sheet: Option<String>,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ModelTable {
    Object,
    Item,
}

impl ModelTable {
    fn name(self) -> &'static str {
        match self {
            Self::Object => "object",
            Self::Item => "item",
        }
    }
}

/// The shared files the room definitions name, collected while exporting the
/// RDTs: model pairs (`prop/`), effect sheets (`effect/`), player-animation
/// pairs (`animation/`) and each room's own scripts.
///
/// A file's id is the CRC-32 of its content, so identical content is one
/// file (a pair used by both an item slot and an object slot, or by many
/// rooms) and a file keeps its id whichever rooms or region a pack was
/// converted from.
#[derive(Debug, Default, Clone)]
pub struct Sidecars {
    props: BTreeMap<u32, (Vec<u8>, Option<Vec<u8>>)>,
    /// Every room slot that names each prop, in conversion order.
    prop_uses: BTreeMap<u32, Vec<(RoomId, ModelTable, usize)>>,
    effects: BTreeMap<u32, Vec<u8>>,
    animations: BTreeMap<u32, (Vec<u8>, Vec<u8>)>,
    rooms: Vec<(String, Vec<u8>)>,
}

/// A sidecar id as its stem digits.
fn sidecar_id(id: u32) -> String {
    format!("{id:0ID_DIGITS$x}")
}

/// The CRC-32 of `parts`, each length-prefixed so no two splits of the same
/// bytes share an id.
fn content_id(parts: &[&[u8]]) -> u32 {
    let mut bytes = Vec::new();
    for part in parts {
        bytes.extend_from_slice(&(part.len() as u32).to_le_bytes());
        bytes.extend_from_slice(part);
    }
    crate::hash::crc32(&bytes)
}

/// File `value` under `id` in `files`. Two different contents with one id
/// is an error rather than a silent overwrite.
fn intern<T: PartialEq>(files: &mut BTreeMap<u32, T>, id: u32, value: T, what: &str) -> Result<()> {
    match files.get(&id) {
        Some(existing) if *existing != value => {
            bail!("two different {what} files share the id {}", sidecar_id(id))
        }
        Some(_) => {}
        None => {
            files.insert(id, value);
        }
    }
    Ok(())
}

/// Whether `value` can be filed under `id` in `files` without a clash.
fn fits<T: PartialEq>(files: &BTreeMap<u32, T>, id: u32, value: &T) -> bool {
    files.get(&id).is_none_or(|existing| existing == value)
}

impl Sidecars {
    fn prop(
        &mut self,
        tmd: &[u8],
        tim: Option<&[u8]>,
        user: (RoomId, ModelTable, usize),
    ) -> Result<String> {
        let id = match tim {
            Some(tim) => content_id(&[tmd, tim]),
            None => content_id(&[tmd]),
        };
        intern(
            &mut self.props,
            id,
            (tmd.to_vec(), tim.map(<[u8]>::to_vec)),
            "prop",
        )?;
        self.prop_uses.entry(id).or_default().push(user);
        Ok(format!("{PROP_DIR}{}", sidecar_id(id)))
    }

    fn effect(&mut self, tim: &[u8]) -> Result<String> {
        let id = content_id(&[tim]);
        intern(&mut self.effects, id, tim.to_vec(), "effect sheet")?;
        Ok(format!("{EFFECT_DIR}{}", sidecar_id(id)))
    }

    fn animation(&mut self, emr: &[u8], edd: &[u8]) -> Result<String> {
        let id = content_id(&[emr, edd]);
        intern(
            &mut self.animations,
            id,
            (emr.to_vec(), edd.to_vec()),
            "animation",
        )?;
        Ok(format!("{ANIMATION_DIR}{}", sidecar_id(id)))
    }

    /// Add every file `other` collected. Nothing is added when one of them
    /// clashes with a file already here.
    pub fn absorb(&mut self, other: Sidecars) -> Result<()> {
        let clash = other
            .props
            .iter()
            .any(|(id, prop)| !fits(&self.props, *id, prop))
            || other
                .effects
                .iter()
                .any(|(id, tim)| !fits(&self.effects, *id, tim))
            || other
                .animations
                .iter()
                .any(|(id, pair)| !fits(&self.animations, *id, pair));
        if clash {
            bail!("a sidecar file shares its id with a different one");
        }
        self.props.extend(other.props);
        for (id, uses) in other.prop_uses {
            self.prop_uses.entry(id).or_default().extend(uses);
        }
        self.effects.extend(other.effects);
        self.animations.extend(other.animations);
        self.rooms.extend(other.rooms);
        Ok(())
    }

    /// Every collected file as `(pack entry, bytes)`, the prop manifest
    /// included.
    ///
    /// A model texture becomes an 8-bit indexed BMP keeping the TIM's texel
    /// indices, palette and STP bits ([`bmp::encode_indexed_page`]). A TIM a
    /// BMP cannot hold exactly (more than one CLUT row, or one that does not
    /// decode) stays a `.tim`; the loader reads either.
    pub fn into_entries(self) -> Vec<(String, Vec<u8>)> {
        let mut entries = Vec::new();
        let mut manifest = String::from(
            "# Room model props written by convert-game: each id's first use and every\n\
             # room/slot that names it. The ids are opaque; this file is for people.\n",
        );
        for (index, (tmd, tim)) in &self.props {
            let id = sidecar_id(*index);
            entries.push((format!("{PROP_DIR}{id}.tmd"), tmd.clone()));
            if let Some(tim) = tim {
                let (extension, data) = prop_texture_file(tim);
                entries.push((format!("{PROP_DIR}{id}.{extension}"), data));
            }
            let uses = &self.prop_uses[index];
            let (room, table, slot) = uses[0];
            let _ = writeln!(manifest, "\n[prop.{id}]");
            let _ = writeln!(
                manifest,
                "label = \"room {:04x} {} {slot}\"",
                room.rdt_number(),
                table.name()
            );
            let _ = writeln!(manifest, "source = \"ROOM{:04X}.RDT\"", room.rdt_number());
            let mut rooms: Vec<String> = uses
                .iter()
                .map(|(room, _, _)| format!("\"{:04x}\"", room.rdt_number()))
                .collect();
            rooms.dedup();
            let _ = writeln!(manifest, "rooms = [{}]", rooms.join(", "));
            for kind in [ModelTable::Object, ModelTable::Item] {
                let slots: Vec<String> = uses
                    .iter()
                    .filter(|(_, table, _)| *table == kind)
                    .map(|(room, _, slot)| format!("\"{:04x}:{slot}\"", room.rdt_number()))
                    .collect();
                if !slots.is_empty() {
                    let _ = writeln!(manifest, "{}s = [{}]", kind.name(), slots.join(", "));
                }
            }
        }
        if !self.props.is_empty() {
            entries.push((PROP_MANIFEST.to_string(), manifest.into_bytes()));
        }
        for (id, tim) in self.effects {
            entries.push((format!("{EFFECT_DIR}{}.tim", sidecar_id(id)), tim));
        }
        for (id, (emr, edd)) in self.animations {
            let id = sidecar_id(id);
            entries.push((format!("{ANIMATION_DIR}{id}.emr"), emr));
            entries.push((format!("{ANIMATION_DIR}{id}.edd"), edd));
        }
        entries.extend(self.rooms);
        entries
    }
}

/// The file a prop texture TIM is stored as: an indexed BMP when it holds the
/// decoded texture exactly, else the TIM itself.
fn prop_texture_file(tim: &[u8]) -> (&'static str, Vec<u8>) {
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

/// Read a little-endian `u32`.
fn u32_at(data: &[u8], offset: usize) -> Option<u32> {
    let raw = data.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes(raw.try_into().unwrap()))
}

/// Read a little-endian `i32`.
fn i32_at(data: &[u8], offset: usize) -> Option<i32> {
    u32_at(data, offset).map(|value| value as i32)
}

impl RoomSource {
    /// Read an RDT into a room definition, adding its models, effect sheets,
    /// player animation and scripts to `sidecars`.
    ///
    /// `script` is the text script the room's messages are written in.
    pub fn from_rdt(
        data: &[u8],
        id: RoomId,
        script: Script,
        sidecars: &mut Sidecars,
    ) -> Result<Self> {
        let state = rdt::parse(data, id)?;
        let tables = rdt::mask_tables(data)?;
        let cameras = state
            .cuts
            .iter()
            .zip(&tables)
            .map(|(cut, table)| camera_source(cut, table, id))
            .collect();
        let pointers = rdt::pointers(data);

        let mut models = |table: ModelTable, pointer: u32, count: u8| {
            (0..usize::from(count))
                .map(|slot| match model_pair(data, pointer, slot) {
                    Some((tmd, tim)) => sidecars.prop(tmd, tim, (id, table, slot)).map(Some),
                    None => Ok(None),
                })
                .collect::<Result<Vec<_>>>()
        };
        let object_models = models(
            ModelTable::Object,
            pointers[OBJECT_MODELS_SLOT],
            state.omodel_slot_count,
        )?;
        let item_models = models(
            ModelTable::Item,
            pointers[ITEM_MODELS_SLOT],
            state.item_count,
        )?;

        let effects = effect_slots(data, &pointers, &state.effects, sidecars)?;
        // A pair that does not parse (room 5020 ships one) leaves the RDT on
        // the no-op stance, so the room names none.
        let player_animation = rdt::room_anim_bounds(data)
            .and_then(|(header, base, bound)| {
                Some((data.get(header..base)?, data.get(base..bound)?))
            })
            .filter(|(emr, edd)| emd::parse_room_anim_files(emr, edd).is_ok())
            .map(|(emr, edd)| sidecars.animation(emr, edd))
            .transpose()?;
        let scripts = match crate::scd::reader::extract_container(data) {
            Ok(Some(container)) => {
                let entry = scripts_entry(id);
                sidecars.rooms.push((entry.clone(), container));
                Some(entry)
            }
            Ok(None) => None,
            // A stub RDT carries no script pointers at all; a real room whose
            // scripts do not read is not one the XML can carry.
            Err(_) if state.cuts.is_empty() => None,
            Err(error) => return Err(error.context("the room's scripts do not read")),
        };

        Ok(Self {
            ambient: state.ambient,
            lights: state.lights,
            cameras,
            zones: state.zones,
            collision: state.collision,
            walk_zones: state.walk_zones,
            footstep_count: footstep_count(data, pointers[FOOTSTEP_SLOT]),
            footstep_zones: state.footstep_zones,
            messages: state
                .messages
                .as_deref()
                .map(|block| Messages::from_block(block, script, id.stage)),
            effects,
            object_models,
            item_models,
            player_animation,
            scripts,
        })
    }

    /// The pack entries the definition needs besides its camera images: each
    /// model mesh (a texture is optional), effect sheet, animation half and
    /// the script container.
    pub fn references(&self) -> Vec<String> {
        let mut entries: Vec<String> = self
            .object_models
            .iter()
            .chain(&self.item_models)
            .flatten()
            .map(|stem| format!("{stem}.tmd"))
            .collect();
        entries.extend(
            self.effects
                .iter()
                .filter_map(|slot| slot.sprite.as_ref()?.sheet.as_deref())
                .map(|stem| format!("{stem}.tim")),
        );
        if let Some(stem) = &self.player_animation {
            entries.push(format!("{stem}.emr"));
            entries.push(format!("{stem}.edd"));
        }
        entries.extend(self.scripts.clone());
        entries.sort();
        entries.dedup();
        entries
    }

    /// Build the engine's room state, reading the named sidecars from `pack`.
    ///
    /// Like the RDT parser, a damaged model, effect sheet or animation never
    /// fails the room: a model pair that does not load is skipped with a
    /// warning in [`RoomState::model_warnings`], an effect sheet that does
    /// not load leaves its sprite without art, and an animation pair that
    /// does not load leaves the room on the no-op stance with a model
    /// warning.
    pub fn build(&self, id: RoomId, pack: &Pack) -> Result<RoomState> {
        let cuts = self
            .cameras
            .iter()
            .enumerate()
            .map(|(index, camera)| build_cut(camera, index, id))
            .collect::<Result<Vec<_>>>()?;
        let slot_count = |slots: &[Option<String>], what: &str| {
            u8::try_from(slots.len()).with_context(|| format!("more than 255 {what} slots"))
        };
        let omodel_slot_count = slot_count(&self.object_models, "object model")?;
        let item_count = slot_count(&self.item_models, "item model")?;
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
        if self.effects.len() > crate::effects::room::EFFECT_SPRITE_SLOTS {
            bail!(
                "{} effect slots declared, the table holds {}",
                self.effects.len(),
                crate::effects::room::EFFECT_SPRITE_SLOTS
            );
        }

        let mut model_warnings = Vec::new();
        let mut cache = HashMap::new();
        let object_models =
            build_models(&self.object_models, pack, &mut cache, &mut model_warnings);
        let item_models = build_models(&self.item_models, pack, &mut cache, &mut model_warnings);

        Ok(RoomState {
            stage: id.stage,
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
            zones: self.zones.clone(),
            walk_zones: self.walk_zones.clone(),
            footstep_zones: self.footstep_zones.clone(),
            messages: self.messages.as_ref().map(Messages::to_block).transpose()?,
            effects: build_effects(&self.effects, pack),
            object_models,
            item_models,
            room_anim: self.player_animation.as_deref().and_then(|stem| {
                build_anim(stem, pack)
                    .map_err(|error| model_warnings.push(format!("animation {stem}: {error:#}")))
                    .ok()
            }),
            model_warnings,
        })
    }
}

/// Pack entry of a room's own SCD container, e.g. `room/1000.scd`.
pub fn scripts_entry(id: RoomId) -> String {
    format!("room/{:04x}.scd", id.rdt_number())
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
fn camera_source(cut: &Cut, table: &mask::MaskTable, id: RoomId) -> Camera {
    let mut sprites = table.sprites.iter();
    let mask_groups: Vec<MaskGroup> = table
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
                    size: (sprite.flags & 0xF000 == 0).then_some(sprite.size),
                })
                .collect(),
        })
        .collect();
    Camera {
        pos: cut.pos,
        look_at: cut.look_at,
        roll: cut.roll,
        fov: cut.fov,
        background: Some(id.cut_stem(cut.index)),
        mask: (!mask_groups.is_empty()).then(|| id.roommask_stem(cut.index)),
        mask_groups,
    }
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

/// The effect slots of an RDT, naming each sprite's sheet.
fn effect_slots(
    data: &[u8],
    pointers: &[u32; 19],
    parsed: &RoomEffects,
    sidecars: &mut Sidecars,
) -> Result<Vec<EffectSlot>> {
    let [index_ptr, _, tim_ptr] = EFFECT_SLOTS.map(|slot| pointers[slot]);
    if EFFECT_SLOTS.iter().any(|&slot| pointers[slot] == 0) {
        return Ok(Vec::new());
    }
    let mut slots = Vec::new();
    for slot in 0..crate::effects::room::EFFECT_SPRITE_SLOTS {
        let Some(&index) = data.get(index_ptr as usize + slot) else {
            break;
        };
        if index == FREE_SPRITE {
            break;
        }
        let sprite = match parsed.sprite(index) {
            Some(sprite) => {
                let sheet = (tim_ptr as usize)
                    .checked_sub(4 * slot)
                    .and_then(|at| i32_at(data, at))
                    .and_then(|offset| usize::try_from(offset).ok())
                    .and_then(|offset| data.get(offset..))
                    .and_then(|tim| Some(&tim[..tim::byte_len(tim).ok()?]))
                    .map(|tim| sidecars.effect(tim))
                    .transpose()?;
                Some(EffectSprite {
                    sheet,
                    page_clut_word: sprite.info.page_clut_word,
                    frames: sprite.info.frames.clone(),
                    uvs: sprite.info.uvs.clone(),
                    depth_table: sprite.anim.depth_table,
                    depth_frames: sprite.anim.depth_frames.clone(),
                })
            }
            None => None,
        };
        slots.push(EffectSlot { index, sprite });
    }
    Ok(slots)
}

/// Build one cut, laying the mask sprites out exactly like the RDT parser.
fn build_cut(camera: &Camera, index: usize, id: RoomId) -> Result<Cut> {
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
            if (sprite.flags & 0xF000 == 0) != sprite.size.is_some() {
                bail!(
                    "camera {index} mask group {group_index}: a sprite has an explicit size \
                     exactly when its flags' high nibble is 0"
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
                size: sprite.pixel_size(),
                pos_data: sprite.depth,
                flags: sprite.flags,
                group: (group_index + 1) as u8,
            });
        }
    }
    let own = |stem: &Option<String>, default: String| stem.clone().filter(|stem| *stem != default);
    Ok(Cut {
        index,
        pos: camera.pos,
        look_at: camera.look_at,
        roll: camera.roll,
        fov: camera.fov,
        background: None,
        background_stem: own(&camera.background, id.cut_stem(index)),
        mask_stem: own(&camera.mask, id.roommask_stem(index)),
        mask_group_count: table.groups.len() as u8,
        mask_active: table.active_bits(),
        masks: table.sprites,
    })
}

/// A decoded model pair, shared by every slot that names it.
type ModelCache = HashMap<String, Option<(crate::model::Tmd, Texture8)>>;

/// Load each slot's prop. A slot whose pair has no texture is declared but
/// unbuilt, like an RDT pair with a null TIM half.
fn build_models(
    slots: &[Option<String>],
    pack: &Pack,
    cache: &mut ModelCache,
    warnings: &mut Vec<String>,
) -> Vec<ObjectAsset> {
    let mut assets = Vec::new();
    for (slot, stem) in slots.iter().enumerate() {
        let Some(stem) = stem else {
            continue;
        };
        let loaded = cache
            .entry(stem.clone())
            .or_insert_with(|| match load_prop(stem, pack) {
                Ok(loaded) => loaded,
                Err(error) => {
                    warnings.push(format!("model {stem}: {error:#}"));
                    None
                }
            });
        if let Some((model, texture)) = loaded {
            assets.push(ObjectAsset {
                pair_index: slot,
                prop: Some(stem.clone()),
                model: model.clone(),
                texture: texture.clone(),
            });
        }
    }
    assets
}

/// Load prop `stem`'s mesh and texture; `Ok(None)` when it has no texture.
fn load_prop(stem: &str, pack: &Pack) -> Result<Option<(crate::model::Tmd, Texture8)>> {
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

/// Build the room's effect sprites from their metadata and named sheets.
fn build_effects(slots: &[EffectSlot], pack: &Pack) -> RoomEffects {
    let mut effects = RoomEffects::default();
    let mut sheets: BTreeMap<&str, Option<Vec<u8>>> = BTreeMap::new();
    for (slot, declared) in slots.iter().enumerate() {
        effects.index[slot] = declared.index;
        let Some(sprite) = &declared.sprite else {
            effects.warnings.push(format!(
                "effect sprite {}: no sprite record",
                declared.index
            ));
            continue;
        };
        let tim = sprite.sheet.as_deref().and_then(|stem| {
            sheets
                .entry(stem)
                .or_insert_with(|| {
                    let entry = format!("{stem}.tim");
                    match pack.read(&entry) {
                        Ok(data) => Some(data.into_owned()),
                        Err(error) => {
                            effects
                                .warnings
                                .push(format!("effect sheet {entry}: {error:#}"));
                            None
                        }
                    }
                })
                .clone()
        });
        let (geometry, texture) = crate::effects::room::parse_tim(tim.as_deref().unwrap_or(&[]));
        effects.sprites.push(StateSprite {
            index: declared.index,
            info: SpriteInfo {
                uv_count: sprite.uvs.len() as u8,
                frames: sprite.frames.clone(),
                uvs: sprite.uvs.clone(),
                page_clut_word: sprite.page_clut_word,
                page_id: 0,
                page_v: 0,
            },
            anim: SpriteAnimation {
                depth_table: sprite.depth_table,
                depth_frames: sprite.depth_frames.clone(),
            },
            tim: texture,
            geometry,
        });
    }
    effects
}

/// Load the room's player-animation pair `stem` (`.emr` + `.edd`).
fn build_anim(stem: &str, pack: &Pack) -> Result<crate::model::RoomAnim> {
    let read = |extension: &str| pack.read(&format!("{stem}.{extension}"));
    emd::parse_room_anim_files(&read("emr")?, &read("edd")?)
}

/// An animation frame from its raw 24-byte blocks.
pub(crate) fn anim_frame(blocks: Vec<[u8; 24]>) -> AnimFrame {
    AnimFrame {
        blocks: blocks.into_iter().map(EffectBlock).collect(),
    }
}
