//! RE2 model readers: the enemy EMD, the player PLD and the weapon PLW.
//!
//! Each file starts with a `u32` directory offset and a `u32` chunk count;
//! the directory of `u32` chunk offsets ends the file and the chunks lie in
//! order between the header and it. The chunk kinds follow biohazard-utils'
//! `EmdFile`/`PldFile`/`PlwFile` for `Biohazard2`:
//!
//! | file | chunks |
//! |---|---|
//! | EMD | morph, EDD, EMR, EDD, EMR, EDD, EMR, MD1 (the texture is a separate `.TIM`) |
//! | PLD | EDD, EMR, MD1, TIM |
//! | PLW | EDD, EMR, MD1, TIM |
//!
//! Every EDD/EMR pair is one animation bank. The first EMR of a model also
//! holds the skeleton (relative joint positions and the child lists, laid
//! out as in RE1); the later ones may leave it out and only pose it. The
//! readers produce the engine's RE1 model types, so a parsed RE2 model draws
//! and animates on the same path as an RE1 one:
//!
//! - An EMR keyframe is RE1's 12-byte header (root offset and the
//!   root-motion vertex) followed by a 12-bit angle triple per joint, packed
//!   low nibble first (biohazard-utils `Emr.KeyFrame.GetAngle`).
//! - An EDD frame is a `u32` whose low 12 bits are the keyframe and whose
//!   upper bits are event flags (the `0x800` sound cues), not RE1's hold
//!   count, so every RE2 frame lasts one tick.
//! - The MD1 mesh is a pair of objects per part, triangles then quads, both
//!   using the triangle object's vertex and normal pools. Each primitive
//!   carries its own `u16` vertex and normal indices and a texture record
//!   whose CLUT and page words are PlayStation `clut`/`tpage` values (page
//!   `n` is the texture's `n`th 128-texel column, CLUT row `n` its palette).
//!   A quad splits into two triangles over the same `(0, 1, 3, 2)` corner
//!   ring as the TMD quad.

use anyhow::{Context, Result, bail};

use crate::budget;
use crate::emd::{chunk_slice, parse_clips, parse_skeleton, read_u16, read_u32, vec3};
use crate::model::{
    AnimBank, Clip, ClipFrame, Emd, Emw, Keyframe, PALETTE_ROW_LEN, Plw, Texture8, Tmd, TmdObject,
    TmdPrim,
};
use crate::{bmp, tim};

/// The EMR header: armature offset, keyframe offset, joint count, stride.
const EMR_HEADER_LEN: usize = 8;
/// An EMR keyframe's root offset and root-motion vertex.
const KEYFRAME_HEADER_LEN: usize = 12;
/// Nibbles per joint angle triple.
const ANGLE_NIBBLES: usize = 9;
/// An MD1 object descriptor: seven `i32`s.
const MD1_OBJECT_LEN: usize = 28;
/// Where the MD1 object descriptors start (and what their offsets are
/// relative to).
const MD1_OBJECTS: usize = 12;
/// One MD1 vertex or normal: `x, y, z` and a pad.
const MD1_VECTOR_LEN: usize = 8;
/// An MD1 triangle's index record and texture record.
const MD1_TRIANGLE_LEN: usize = 12;
/// An MD1 quad's index record and texture record.
const MD1_QUAD_LEN: usize = 16;
/// The EMD directory: the morph chunk, three EDD/EMR banks and the mesh.
const EMD_CHUNKS: usize = 8;
/// The PLD and PLW directory: EDD, EMR, MD1 and TIM.
const PLAYER_CHUNKS: usize = 4;
/// The player mesh part a weapon's first part replaces: the right hand
/// (biohazard-utils `ReplacePart(11, …)`).
pub const WEAPON_PART: usize = 11;
/// The texture page the weapon's texels are pasted into, and where on it:
/// biohazard-utils' `WithWeaponTexture` pastes the PLW's 56 x 32 TIM at
/// (72, 224) of the player texture's page 1.
const WEAPON_PAGE: usize = 1;
const WEAPON_AT: [usize; 2] = [72, 224];
/// Texels per texture page.
const PAGE_WIDTH: usize = 128;

/// The chunk offsets of an RE2 model file, with the directory start as the
/// final bound. `expected` is the chunk count the file kind has.
fn directory(data: &[u8], expected: usize, kind: &str) -> Result<Vec<usize>> {
    let start = read_u32(data, 0).with_context(|| format!("{kind} header"))? as usize;
    let count = read_u32(data, 4).with_context(|| format!("{kind} header"))? as usize;
    if count != expected {
        bail!("{kind} has {count} chunks, expected {expected}");
    }
    let end = start
        .checked_add(count * 4)
        .context("directory offset overflows")?;
    if start < 8 || end > data.len() {
        bail!(
            "{kind} directory at 0x{start:X} does not fit the {}-byte file",
            data.len()
        );
    }
    let mut offsets = (0..count)
        .map(|slot| Ok(read_u32(data, start + slot * 4)? as usize))
        .collect::<Result<Vec<_>>>()?;
    offsets.push(start);
    let mut previous = 8;
    for &offset in &offsets {
        if offset < previous {
            bail!("{kind} chunk offsets are out of order: {offsets:X?}");
        }
        previous = offset;
    }
    Ok(offsets)
}

/// Parse an RE2 enemy EMD with the texture of its `.TIM` beside it.
pub fn parse_emd(data: &[u8], tim: &[u8]) -> Result<Emd> {
    let texture = tim::decode_8bpp(tim).context("EMD texture")?;
    parse_emd_with(data, texture)
}

/// Texels per side of a palette swatch: 16x16 is one 256-entry CLUT row.
const SWATCH_SIDE: u32 = 16;

/// An RE2 enemy texture as the pack stores it, from its `.TIM`: the page as
/// an indexed BMP in CLUT row 0's colours ([`bmp::encode_indexed_page`]),
/// then one palette swatch per further CLUT row, a 16x16 indexed BMP whose
/// texel `n` is entry `n` ([`texture_from_bmps`] reverses it exactly).
///
/// A 256-entry palette is all an 8-bit BMP holds, and the retail textures
/// carry one to four CLUT rows, one per 128-texel page, though not always
/// in page order.
pub fn texture_bmps(tim: &[u8]) -> Result<Vec<Vec<u8>>> {
    let texture = tim::decode_8bpp(tim)?;
    if texture.palettes.len() % PALETTE_ROW_LEN != 0 || texture.palettes.is_empty() {
        bail!(
            "the TIM's {} CLUT entries are not whole {PALETTE_ROW_LEN}-entry rows",
            texture.palettes.len()
        );
    }
    let rows = texture.palettes.len() / PALETTE_ROW_LEN;
    let mut out = vec![bmp::encode_indexed_page(&texture, 0)?];
    for row in 1..rows {
        let range = row * PALETTE_ROW_LEN..(row + 1) * PALETTE_ROW_LEN;
        let swatch = Texture8 {
            width: SWATCH_SIDE,
            height: SWATCH_SIDE,
            indices: (0..=255).collect(),
            palettes: texture.palettes[range.clone()].to_vec(),
            stp: texture.stp[range].to_vec(),
        };
        out.push(bmp::encode_indexed_page(&swatch, 0)?);
    }
    Ok(out)
}

/// An RE2 enemy texture from its page BMP and the swatches of its CLUT rows
/// 1 and up ([`texture_bmps`]). The page's texels and row 0 come from
/// `page`; each swatch supplies only its palette.
pub fn texture_from_bmps<T: AsRef<[u8]>>(page: &[u8], swatches: &[T]) -> Result<Texture8> {
    let mut texture = bmp::decode_indexed_page(page).context("texture page")?;
    for (index, swatch) in swatches.iter().enumerate() {
        let row = bmp::decode_indexed_page(swatch.as_ref())
            .with_context(|| format!("CLUT row {} swatch", index + 1))?;
        texture
            .palettes
            .extend_from_slice(&row.palettes[..PALETTE_ROW_LEN]);
        texture.stp.extend_from_slice(&row.stp[..PALETTE_ROW_LEN]);
    }
    budget::check_len(
        texture.palettes.len(),
        budget::MAX_CLUT_ENTRIES,
        "texture CLUT entry count",
    )?;
    Ok(texture)
}

/// The pack entry of an RE2 enemy texture's CLUT row `row` swatch, beside
/// the page `<stem>.bmp`: `<stem>_clut_<row:02x>.bmp`.
pub fn clut_entry(stem: &str, row: usize) -> String {
    format!("{stem}_clut_{row:02x}.bmp")
}

/// Parse an RE2 enemy EMD with `texture` as its skin.
pub fn parse_emd_with(data: &[u8], texture: Texture8) -> Result<Emd> {
    let chunks = directory(data, EMD_CHUNKS, "EMD")?;
    let skeleton = parse_skeleton(data, chunks[2], chunks[3]).context("EMD skeleton")?;
    let mut banks = (0..3)
        .map(|bank| {
            let edd = chunks[1 + bank * 2];
            let emr = chunks[2 + bank * 2];
            parse_bank(data, edd, emr, chunks[3 + bank * 2])
                .with_context(|| format!("EMD animation bank {bank}"))
        })
        .collect::<Result<Vec<_>>>()?;
    let mesh = parse_md1(&data[chunks[7]..chunks[8]]).context("EMD mesh")?;
    let first = banks.remove(0);
    Ok(Emd {
        skeleton,
        keyframes: first.keyframes,
        clips: first.clips,
        damage_clips: Vec::new(),
        damage_keyframes: Vec::new(),
        banks,
        mesh,
        texture,
    })
}

/// Parse an RE2 player PLD: the character's skeleton, its one animation
/// bank, the mesh and the texture.
pub fn parse_pld(data: &[u8]) -> Result<Emd> {
    let chunks = directory(data, PLAYER_CHUNKS, "PLD")?;
    let skeleton = parse_skeleton(data, chunks[1], chunks[2]).context("PLD skeleton")?;
    let bank = parse_bank(data, chunks[0], chunks[1], chunks[2]).context("PLD animation")?;
    let mesh = parse_md1(&data[chunks[2]..chunks[3]]).context("PLD mesh")?;
    let texture = tim::decode_8bpp(&data[chunks[3]..chunks[4]]).context("PLD texture")?;
    Ok(Emd {
        skeleton,
        keyframes: bank.keyframes,
        clips: bank.clips,
        damage_clips: Vec::new(),
        damage_keyframes: Vec::new(),
        banks: Vec::new(),
        mesh,
        texture,
    })
}

/// Parse an RE2 player weapon PLW: its animation bank (posing the
/// character's skeleton), the hand-and-weapon mesh and the weapon texels.
pub fn parse_plw(data: &[u8]) -> Result<Plw> {
    let chunks = directory(data, PLAYER_CHUNKS, "PLW")?;
    let bank = parse_bank(data, chunks[0], chunks[1], chunks[2]).context("PLW animation")?;
    let mesh = parse_md1(&data[chunks[2]..chunks[3]]).context("PLW mesh")?;
    let texture = tim::decode_8bpp(&data[chunks[3]..chunks[4]]).context("PLW texture")?;
    Ok(Plw {
        keyframes: bank.keyframes,
        clips: bank.clips,
        mesh,
        texture,
    })
}

/// The character `pld` holding `plw` as the two banks [`crate::player2`]
/// reads: its own clips (`pKan_t_ptr`), and the weapon's, which hold the walk,
/// run and idle (`pSub0_kan_t_ptr`). The keyframes are one table, so both
/// banks' frames index it. `None` when the weapon's bank has no locomotion, as
/// a stub's doesn't.
pub fn player_banks(pld: &Emd, plw: &Plw) -> Option<(Emd, Emw)> {
    if plw.clips.len() < LOCOMOTION_CLIPS {
        return None;
    }
    let base = pld.keyframes.len() as u16;
    let weapon_clips = plw
        .clips
        .iter()
        .map(|clip| Clip {
            frames: clip
                .frames
                .iter()
                .map(|frame| ClipFrame {
                    keyframe: frame.keyframe + base,
                    ..*frame
                })
                .collect(),
        })
        .collect();
    let keyframes: Vec<_> = pld
        .keyframes
        .iter()
        .chain(&plw.keyframes)
        .cloned()
        .collect();
    let mut body = with_weapon(pld, plw);
    body.keyframes = keyframes.clone();
    body.clips = pld.clips.clone();
    body.banks = Vec::new();
    let weapon = Emw {
        skeleton: pld.skeleton.clone(),
        keyframes,
        clips: weapon_clips,
        mesh: Tmd::default(),
    };
    Some((body, weapon))
}

/// The clips every real PLW starts with; a stub for a weapon the character
/// never holds has one.
const LOCOMOTION_CLIPS: usize = 9;

/// The character `pld` holding `plw`: the weapon's first part replaces the
/// right hand ([`WEAPON_PART`]), its texels are pasted into the hand's
/// texture page ([`weapon_texture`]) and its clips drive the skeleton, with
/// the character's own clips as the one further bank. A weapon with no
/// keyframes of its own (the knife's bank is an empty chunk) keeps the
/// character's clips.
pub fn with_weapon(pld: &Emd, plw: &Plw) -> Emd {
    let mut mesh = pld.mesh.clone();
    if let (Some(hand), Some(weapon)) =
        (mesh.objects.get_mut(WEAPON_PART), plw.mesh.objects.first())
    {
        *hand = weapon.clone();
    }
    let own = AnimBank {
        keyframes: pld.keyframes.clone(),
        clips: pld.clips.clone(),
    };
    let (bank, banks) = if plw.keyframes.is_empty() {
        (own, Vec::new())
    } else {
        let weapon = AnimBank {
            keyframes: plw.keyframes.clone(),
            clips: plw.clips.clone(),
        };
        (weapon, vec![own])
    };
    Emd {
        skeleton: pld.skeleton.clone(),
        keyframes: bank.keyframes,
        clips: bank.clips,
        damage_clips: Vec::new(),
        damage_keyframes: Vec::new(),
        banks,
        mesh,
        texture: weapon_texture(&pld.texture, &plw.texture),
    }
}

/// The player texture with the weapon's texels pasted at `WEAPON_AT` of
/// page `WEAPON_PAGE`, read through that page's palette row.
///
/// The PC game blits both textures to 16-bit surfaces, so a pasted texel
/// keeps its exact colour. An indexed texture has to find each weapon colour
/// a slot in the page's palette row: an entry already holding it, else one
/// no texel of the page outside the pasted box uses, else (a full row) the
/// nearest colour.
pub fn weapon_texture(player: &Texture8, weapon: &Texture8) -> Texture8 {
    let mut texture = player.clone();
    let (width, height) = (texture.width as usize, texture.height as usize);
    let page_x = WEAPON_PAGE * PAGE_WIDTH;
    let row_start = WEAPON_PAGE * PALETTE_ROW_LEN;
    if page_x + WEAPON_AT[0] >= width
        || WEAPON_AT[1] >= height
        || texture.palettes.len() < row_start + PALETTE_ROW_LEN
        || texture.stp.len() < row_start + PALETTE_ROW_LEN
    {
        return texture;
    }
    let paste_w = (weapon.width as usize)
        .min(PAGE_WIDTH - WEAPON_AT[0])
        .min(width - page_x - WEAPON_AT[0]);
    let paste_h = (weapon.height as usize).min(height - WEAPON_AT[1]);
    let inside = |x: usize, y: usize| {
        (page_x + WEAPON_AT[0]..page_x + WEAPON_AT[0] + paste_w).contains(&x)
            && (WEAPON_AT[1]..WEAPON_AT[1] + paste_h).contains(&y)
    };
    let mut used = [false; PALETTE_ROW_LEN];
    for y in 0..height {
        for x in page_x..(page_x + PAGE_WIDTH).min(width) {
            if !inside(x, y) {
                used[usize::from(texture.indices[y * width + x])] = true;
            }
        }
    }
    let mut slots: Vec<Option<u8>> = vec![None; PALETTE_ROW_LEN];
    for y in 0..paste_h {
        for x in 0..paste_w {
            let source = weapon.indices[y * weapon.width as usize + x];
            let slot = match slots[usize::from(source)] {
                Some(slot) => slot,
                None => {
                    let colour = weapon.palette(0, source);
                    let stp = weapon.palette_stp(0, source);
                    let slot = palette_slot(&mut texture, row_start, &mut used, colour, stp);
                    slots[usize::from(source)] = Some(slot);
                    slot
                }
            };
            texture.indices[(WEAPON_AT[1] + y) * width + page_x + WEAPON_AT[0] + x] = slot;
        }
    }
    texture
}

/// A slot of the palette row at `row_start` for `colour`: one holding it,
/// else a free one claimed for it, else the nearest colour's.
fn palette_slot(
    texture: &mut Texture8,
    row_start: usize,
    used: &mut [bool; PALETTE_ROW_LEN],
    colour: [u8; 4],
    stp: bool,
) -> u8 {
    let entry = |texture: &Texture8, index: usize| {
        (
            texture.palettes[row_start + index],
            texture.stp[row_start + index],
        )
    };
    if let Some(index) = (0..PALETTE_ROW_LEN).find(|&index| entry(texture, index) == (colour, stp))
    {
        used[index] = true;
        return index as u8;
    }
    // Index 0 stays the transparent texel's.
    if let Some(index) = (1..PALETTE_ROW_LEN).find(|&index| !used[index]) {
        used[index] = true;
        texture.palettes[row_start + index] = colour;
        texture.stp[row_start + index] = stp;
        return index as u8;
    }
    let distance = |other: [u8; 4]| {
        (0..3)
            .map(|channel| (i32::from(other[channel]) - i32::from(colour[channel])).pow(2))
            .sum::<i32>()
    };
    // An equal colour may share slot 0, but a nearest one would turn the
    // texel transparent.
    (1..PALETTE_ROW_LEN)
        .min_by_key(|&index| distance(texture.palettes[row_start + index]))
        .unwrap_or(0) as u8
}

/// A room's own player animations ([`crate::rdt2::room_anim`]). They pose the
/// PLD's skeleton, so the bank has none of its own.
pub fn parse_room_anim(emr: &[u8], edd: &[u8]) -> Result<crate::model::RoomAnim> {
    let data = [edd, emr].concat();
    let bank = parse_bank(&data, 0, edd.len(), data.len())?;
    Ok(crate::model::RoomAnim {
        skeleton: Default::default(),
        keyframes: bank.keyframes,
        clips: bank.clips,
    })
}

/// One EDD/EMR pair over `edd..emr..end`. A chunk too short for its header
/// (EM020's empty second bank is four bytes each) is an empty bank.
fn parse_bank(data: &[u8], edd: usize, emr: usize, end: usize) -> Result<AnimBank> {
    let clips = if emr - edd < 4 {
        Vec::new()
    } else {
        parse_clips(data, edd, emr)?
            .into_iter()
            .map(|clip| Clip {
                frames: clip
                    .frames
                    .into_iter()
                    .map(|mut frame| {
                        // `pNow_seq`'s whole word: RE1's hold count is its top half.
                        frame.flags |= u32::from(frame.timing) << 16;
                        frame.timing = 0;
                        frame
                    })
                    .collect(),
            })
            .collect()
    };
    let keyframes = if end - emr < EMR_HEADER_LEN {
        Vec::new()
    } else {
        parse_keyframes(data, emr, end)?
    };
    Ok(AnimBank { keyframes, clips })
}

/// The EMR keyframes with their packed 12-bit angles.
fn parse_keyframes(data: &[u8], emr: usize, end: usize) -> Result<Vec<Keyframe>> {
    let keyframe_offset = usize::from(read_u16(data, emr + 2)?);
    let joints = usize::from(read_u16(data, emr + 4)?);
    let stride = usize::from(read_u16(data, emr + 6)?);
    budget::check_len(joints, budget::MAX_EMD_JOINTS, "EMR joint count")?;
    let start = emr
        .checked_add(keyframe_offset)
        .context("EMR keyframe offset overflows")?;
    if keyframe_offset < EMR_HEADER_LEN || start > end {
        bail!("EMR keyframe offset 0x{keyframe_offset:X} is outside the armature chunk");
    }
    let angle_bytes = (joints * ANGLE_NIBBLES).div_ceil(2);
    if stride < KEYFRAME_HEADER_LEN + angle_bytes {
        bail!("EMR frame stride {stride} is too small for {joints} joints");
    }
    let count = (end - start) / stride;
    budget::check_len(count, budget::MAX_EMD_KEYFRAMES, "EMR keyframe count")?;
    let mut keyframes = budget::alloc(count, "EMR keyframe list")?;
    for index in 0..count {
        let base = start + index * stride;
        let frame = chunk_slice(data, base, stride, end, "EMR keyframe")?;
        let angles = &frame[KEYFRAME_HEADER_LEN..KEYFRAME_HEADER_LEN + angle_bytes];
        let nibble = |at: usize| {
            let byte = angles[at / 2];
            u16::from(if at.is_multiple_of(2) {
                byte & 0x0F
            } else {
                byte >> 4
            })
        };
        let angle = |at: usize| (nibble(at) | nibble(at + 1) << 4 | nibble(at + 2) << 8) as i16;
        keyframes.push(Keyframe {
            offset: vec3(&frame[0..6]),
            vertex: vec3(&frame[6..12]),
            rotations: (0..joints)
                .map(|joint| {
                    let at = joint * ANGLE_NIBBLES;
                    [angle(at), angle(at + 3), angle(at + 6)]
                })
                .collect(),
        });
    }
    Ok(keyframes)
}

/// Read one MD1 object descriptor: vertex, normal and primitive spans
/// relative to [`MD1_OBJECTS`].
#[derive(Debug, Clone, Copy)]
struct Md1Object {
    vertices: usize,
    vertex_count: usize,
    normals: usize,
    normal_count: usize,
    prims: usize,
    prim_count: usize,
    textures: usize,
}

fn md1_object(data: &[u8], index: usize) -> Result<Md1Object> {
    let at = MD1_OBJECTS + index * MD1_OBJECT_LEN;
    let field = |slot: usize| -> Result<usize> {
        let value = read_u32(data, at + slot * 4)? as i32;
        usize::try_from(value).with_context(|| format!("MD1 object {index} has a negative field"))
    };
    Ok(Md1Object {
        vertices: field(0)?,
        vertex_count: field(1)?,
        normals: field(2)?,
        normal_count: field(3)?,
        prims: field(4)?,
        prim_count: field(5)?,
        textures: field(6)?,
    })
}

/// A span of `count` records of `len` bytes at `offset` past the object
/// table base.
fn md1_span<'a>(
    data: &'a [u8],
    offset: usize,
    count: usize,
    len: usize,
    what: &str,
) -> Result<&'a [u8]> {
    budget::check_len(count, budget::MAX_RECORDS, what)?;
    let start = MD1_OBJECTS
        .checked_add(offset)
        .with_context(|| format!("{what} offset overflows"))?;
    chunk_slice(data, start, count * len, data.len(), what)
}

fn md1_vectors(data: &[u8], offset: usize, count: usize, what: &str) -> Result<Vec<[i16; 3]>> {
    Ok(md1_span(data, offset, count, MD1_VECTOR_LEN, what)?
        .as_chunks::<MD1_VECTOR_LEN>()
        .0
        .iter()
        .map(|vector| vec3(vector))
        .collect())
}

/// Parse an MD1 mesh into one TMD object per part.
pub fn parse_md1(data: &[u8]) -> Result<Tmd> {
    let count = read_u32(data, 8).context("MD1 header")? as usize;
    if !count.is_multiple_of(2) {
        bail!("MD1 has {count} objects; parts come in triangle/quad pairs");
    }
    budget::check_len(count, budget::MAX_RECORDS, "MD1 object count")?;
    chunk_slice(
        data,
        MD1_OBJECTS,
        count * MD1_OBJECT_LEN,
        data.len(),
        "MD1 object table",
    )?;
    let objects = (0..count / 2)
        .map(|part| parse_part(data, part).with_context(|| format!("MD1 part {part}")))
        .collect::<Result<_>>()?;
    Ok(Tmd { objects })
}

fn parse_part(data: &[u8], part: usize) -> Result<TmdObject> {
    let triangles = md1_object(data, part * 2)?;
    let quads = md1_object(data, part * 2 + 1)?;
    if (
        quads.vertices,
        quads.vertex_count,
        quads.normals,
        quads.normal_count,
    ) != (
        triangles.vertices,
        triangles.vertex_count,
        triangles.normals,
        triangles.normal_count,
    ) {
        bail!("the quad object does not share the triangle object's vertex and normal pools");
    }
    let vertices = md1_vectors(
        data,
        triangles.vertices,
        triangles.vertex_count,
        "MD1 vertices",
    )?;
    let normals = md1_vectors(
        data,
        triangles.normals,
        triangles.normal_count,
        "MD1 normals",
    )?;
    let mut prims = Vec::with_capacity(triangles.prim_count + quads.prim_count * 2);
    let indices = md1_span(
        data,
        triangles.prims,
        triangles.prim_count,
        MD1_TRIANGLE_LEN,
        "MD1 triangles",
    )?;
    let textures = md1_span(
        data,
        triangles.textures,
        triangles.prim_count,
        MD1_TRIANGLE_LEN,
        "MD1 triangle textures",
    )?;
    for (index, texture) in indices
        .as_chunks::<MD1_TRIANGLE_LEN>()
        .0
        .iter()
        .zip(textures.as_chunks::<MD1_TRIANGLE_LEN>().0)
    {
        let word = |slot: usize| u16::from_le_bytes([index[slot * 2], index[slot * 2 + 1]]);
        prims.push(prim(
            [word(1), word(3), word(5)],
            [word(0), word(2), word(4)],
            [0, 4, 8].map(|at| [texture[at], texture[at + 1]]),
            texture,
            None,
        ));
    }
    let indices = md1_span(
        data,
        quads.prims,
        quads.prim_count,
        MD1_QUAD_LEN,
        "MD1 quads",
    )?;
    let textures = md1_span(
        data,
        quads.textures,
        quads.prim_count,
        MD1_QUAD_LEN,
        "MD1 quad textures",
    )?;
    for (index, texture) in indices
        .as_chunks::<MD1_QUAD_LEN>()
        .0
        .iter()
        .zip(textures.as_chunks::<MD1_QUAD_LEN>().0)
    {
        let word = |slot: usize| u16::from_le_bytes([index[slot * 2], index[slot * 2 + 1]]);
        let corners = [word(1), word(3), word(5), word(7)];
        let corner_normals = [word(0), word(2), word(4), word(6)];
        let uvs = [0, 4, 8, 12].map(|at| [texture[at], texture[at + 1]]);
        // The quad's corner ring is (0, 1, 3, 2), fanned like the TMD quad.
        for fan in [[0usize, 1, 3], [0, 3, 2]] {
            prims.push(prim(
                fan.map(|corner| corners[corner]),
                fan.map(|corner| corner_normals[corner]),
                fan.map(|corner| uvs[corner]),
                texture,
                Some(corners),
            ));
        }
    }
    for (index, prim) in prims.iter().enumerate() {
        if let Some(&vertex) = prim
            .vertices
            .iter()
            .find(|&&vertex| usize::from(vertex) >= vertices.len())
        {
            bail!(
                "primitive {index} uses vertex {vertex} of {}",
                vertices.len()
            );
        }
        if let Some(&normal) = prim
            .normals
            .iter()
            .find(|&&normal| usize::from(normal) >= normals.len())
        {
            bail!(
                "primitive {index} uses normal {normal} of {}",
                normals.len()
            );
        }
    }
    Ok(TmdObject {
        vertices,
        normals,
        prims,
    })
}

/// A textured MD1 primitive; `texture` is its texture record, whose words 1
/// and 3 are the CLUT and the page.
fn prim(
    vertices: [u16; 3],
    normals: [u16; 3],
    uv: [[u8; 2]; 3],
    texture: &[u8],
    quad: Option<[u16; 4]>,
) -> TmdPrim {
    TmdPrim {
        vertices,
        normals,
        uv,
        clut: u16::from_le_bytes([texture[2], texture[3]]),
        tsb: u16::from_le_bytes([texture[6], texture[7]]),
        textured: true,
        blend: false,
        additive: false,
        raw_y: false,
        flat_color: None,
        quad,
        uv_nudge: [0; 3],
    }
}

#[cfg(test)]
pub(crate) mod tests;
