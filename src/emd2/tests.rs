use super::*;

fn put16(data: &mut Vec<u8>, value: u16) {
    data.extend_from_slice(&value.to_le_bytes());
}

fn put32(data: &mut Vec<u8>, value: u32) {
    data.extend_from_slice(&value.to_le_bytes());
}

/// An RE2 model file: the header, `chunks` in order and the directory.
fn container(chunks: &[Vec<u8>]) -> Vec<u8> {
    let mut data = vec![0; 8];
    let mut offsets = Vec::new();
    for chunk in chunks {
        offsets.push(data.len() as u32);
        data.extend_from_slice(chunk);
    }
    let directory = data.len() as u32;
    for offset in offsets {
        put32(&mut data, offset);
    }
    data[0..4].copy_from_slice(&directory.to_le_bytes());
    data[4..8].copy_from_slice(&(chunks.len() as u32).to_le_bytes());
    data
}

/// One clip of two frames, keyframes 1 and 0, the first with the `0x800`
/// event flag in its upper half.
fn edd() -> Vec<u8> {
    let mut data = Vec::new();
    put16(&mut data, 2);
    put16(&mut data, 4);
    put32(&mut data, 0x0800_0001);
    put32(&mut data, 0x0000_2000);
    data
}

/// Two joints (joint 0 the parent of joint 1) and two keyframes.
fn emr(skeleton: bool) -> Vec<u8> {
    let joints = 2u16;
    // 12 header bytes and 9 bytes of packed angles, padded to 24.
    let stride = 24u16;
    let mut data = Vec::new();
    let (armature, keyframes) = if skeleton { (20u16, 30u16) } else { (0, 8) };
    put16(&mut data, armature);
    put16(&mut data, keyframes);
    put16(&mut data, joints);
    put16(&mut data, stride);
    if skeleton {
        for value in [0i16, -100, 0, 10, 20, 30] {
            put16(&mut data, value as u16);
        }
        // Armature entries: joint 0 has one child listed at +8, joint 1 none.
        put16(&mut data, 1);
        put16(&mut data, 8);
        put16(&mut data, 0);
        put16(&mut data, 8);
        data.push(1);
        data.push(0);
    }
    assert_eq!(data.len(), usize::from(keyframes));
    for frame in 0..2u16 {
        for value in [frame, 2, 3, 4, 5, 6] {
            put16(&mut data, value);
        }
        // Joint 0: 0x123, 0x456, 0x789; joint 1: 0xABC, 0xDEF, 0x010 (low
        // nibble first).
        data.extend_from_slice(&[0x23, 0x61, 0x45, 0x89, 0xC7, 0xAB, 0xEF, 0x0D, 0x01]);
        data.extend_from_slice(&[0; 3]);
    }
    data
}

/// One part: three vertices and normals, a triangle on page 1 and a quad on
/// page 0.
fn md1() -> Vec<u8> {
    let mut body = Vec::new();
    let vertices = body.len();
    for vertex in [[1i16, 2, 3], [4, 5, 6], [7, 8, 9], [10, 11, 12]] {
        for value in vertex {
            put16(&mut body, value as u16);
        }
        put16(&mut body, 0);
    }
    let normals = body.len();
    for _ in 0..4 {
        for value in [0i16, 4096, 0, 0] {
            put16(&mut body, value as u16);
        }
    }
    let triangles = body.len();
    for value in [0u16, 0, 1, 1, 2, 2] {
        put16(&mut body, value);
    }
    let triangle_textures = body.len();
    body.extend_from_slice(&[1, 2]);
    put16(&mut body, 0x7840);
    body.extend_from_slice(&[3, 4]);
    put16(&mut body, 0x81);
    body.extend_from_slice(&[5, 6]);
    put16(&mut body, 0);
    let quads = body.len();
    for value in [0u16, 0, 1, 1, 2, 2, 3, 3] {
        put16(&mut body, value);
    }
    let quad_textures = body.len();
    for corner in 0..4u8 {
        body.extend_from_slice(&[corner * 10, corner * 10 + 1]);
        put16(&mut body, if corner == 0 { 0x7800 } else { 0 });
        if corner == 1 {
            body.pop();
            body.pop();
            put16(&mut body, 0x80);
        }
    }
    let table = 2 * MD1_OBJECT_LEN;
    let mut data = Vec::new();
    put32(&mut data, 0);
    put32(&mut data, 0);
    put32(&mut data, 2);
    for (prims, textures) in [(triangles, triangle_textures), (quads, quad_textures)] {
        for value in [
            vertices + table,
            4,
            normals + table,
            4,
            prims + table,
            1,
            textures + table,
        ] {
            put32(&mut data, value as u32);
        }
    }
    data.extend_from_slice(&body);
    data
}

/// An 8bpp TIM `width` x `height` with `rows` CLUT rows, entry `i` of row
/// `r` being BGR555 `r * 256 + i` and every texel `index(x, y)`.
pub(crate) fn tim(width: u16, height: u16, rows: u16, index: impl Fn(u16, u16) -> u8) -> Vec<u8> {
    let mut data = Vec::new();
    put32(&mut data, 0x10);
    put32(&mut data, 0x09);
    put32(&mut data, 12 + u32::from(rows) * 512);
    put16(&mut data, 0);
    put16(&mut data, 480);
    put16(&mut data, 256);
    put16(&mut data, rows);
    for row in 0..rows {
        for entry in 0..256u16 {
            put16(&mut data, (row * 256 + entry) & 0x7FFF);
        }
    }
    put32(&mut data, 12 + u32::from(width) * u32::from(height));
    put16(&mut data, 0);
    put16(&mut data, 0);
    put16(&mut data, width / 2);
    put16(&mut data, height);
    for y in 0..height {
        for x in 0..width {
            data.push(index(x, y));
        }
    }
    data
}

/// A whole PLD: one clip, two joints, one part and a two-page texture.
pub(crate) fn pld() -> Vec<u8> {
    container(&[edd(), emr(true), md1(), tim(256, 256, 2, |_, _| 0)])
}

#[test]
fn a_pld_parses_its_skeleton_bank_mesh_and_texture() {
    let model = parse_pld(&pld()).unwrap();
    assert_eq!(model.skeleton.relative, vec![[0, -100, 0], [10, 20, 30]]);
    assert_eq!(model.skeleton.children, vec![vec![1], vec![]]);
    assert_eq!(model.keyframes.len(), 2);
    assert_eq!(model.keyframes[1].offset, [1, 2, 3]);
    assert_eq!(model.keyframes[1].vertex, [4, 5, 6]);
    assert_eq!(
        model.keyframes[0].rotations,
        vec![[0x123, 0x456, 0x789], [0xABC, 0xDEF, 0x010]]
    );
    assert_eq!(model.texture.width, 256);
    assert!(model.banks.is_empty());
}

#[test]
fn an_edd_frame_s_upper_bits_are_flags_not_a_hold() {
    let model = parse_pld(&pld()).unwrap();
    let frames = &model.clips[0].frames;
    assert_eq!(
        frames
            .iter()
            .map(|frame| (frame.keyframe, frame.timing))
            .collect::<Vec<_>>(),
        vec![(1, 0), (0, 0)]
    );
}

#[test]
fn an_md1_part_becomes_one_tmd_object_with_split_quads() {
    let mesh = parse_md1(&md1()).unwrap();
    assert_eq!(mesh.objects.len(), 1);
    let object = &mesh.objects[0];
    assert_eq!(object.vertices[3], [10, 11, 12]);
    assert_eq!(object.normals[0], [0, 4096, 0]);
    assert_eq!(object.prims.len(), 3);
    let triangle = &object.prims[0];
    assert_eq!(triangle.vertices, [0, 1, 2]);
    assert_eq!(triangle.uv, [[1, 2], [3, 4], [5, 6]]);
    assert_eq!((triangle.clut, triangle.tsb), (0x7840, 0x81));
    assert_eq!(triangle.quad, None);
    // The quad's ring is (0, 1, 3, 2), fanned from corner 0.
    let (first, second) = (&object.prims[1], &object.prims[2]);
    assert_eq!(first.vertices, [0, 1, 3]);
    assert_eq!(second.vertices, [0, 3, 2]);
    assert_eq!(first.uv, [[0, 1], [10, 11], [30, 31]]);
    assert_eq!((first.clut, first.tsb), (0x7800, 0x80));
    assert_eq!(first.quad, Some([0, 1, 2, 3]));
}

/// A whole enemy EMD: three banks (the last empty) and one part.
pub(crate) fn emd() -> Vec<u8> {
    let empty = vec![0u8; 4];
    container(&[
        empty.clone(),
        edd(),
        emr(true),
        edd(),
        emr(false),
        empty.clone(),
        empty,
        md1(),
    ])
}

/// A three-row enemy texture with STP bits set, including on the black
/// transparent entry.
pub(crate) fn enemy_tim() -> Vec<u8> {
    let mut data = tim(384, 4, 3, |x, y| (x ^ (y * 7)) as u8);
    // Entry `i` of row `r` is the `u16` at 20 + 2 * (r * 256 + i).
    for (row, entry) in [(0usize, 0usize), (2, 5), (1, 255)] {
        data[20 + 2 * (row * 256 + entry) + 1] |= 0x80;
    }
    data
}

#[test]
fn an_enemy_texture_round_trips_through_its_page_and_clut_swatches() {
    let tim = enemy_tim();
    let bmps = texture_bmps(&tim).unwrap();
    assert_eq!(bmps.len(), 3);
    let expected = crate::tim::decode_8bpp(&tim).unwrap();
    assert_eq!(texture_from_bmps(&bmps[0], &bmps[1..]).unwrap(), expected);
    // Every file is an ordinary indexed BMP: the page shows in row 0's
    // colours and a swatch is its row's 256 entries.
    let swatch = crate::bmp::decode_indexed_page(&bmps[2]).unwrap();
    assert_eq!((swatch.width, swatch.height), (16, 16));
    assert_eq!(swatch.palettes[..256], expected.palettes[512..]);
    assert_eq!(
        clut_entry("enemy/3a/enemy_1", 2),
        "enemy/3a/enemy_1_clut_02.bmp"
    );
}

#[test]
fn an_emd_has_three_banks_and_takes_its_texture_from_the_tim() {
    let data = emd();
    let model = parse_emd(&data, &tim(128, 256, 1, |_, _| 7)).unwrap();
    assert_eq!(model.clips.len(), 1);
    assert_eq!(model.banks.len(), 2);
    assert_eq!(model.banks[0].keyframes.len(), 2);
    assert_eq!(
        model.banks[0].keyframes[0].rotations[1],
        [0xABC, 0xDEF, 0x010]
    );
    // EM020's second bank: four-byte chunks with nothing in them.
    assert_eq!(model.banks[1], AnimBank::default());
    assert_eq!(model.texture.indices[0], 7);
}

#[test]
fn a_bad_directory_is_an_error() {
    let mut data = pld();
    data[4] = 5;
    assert!(parse_pld(&data).is_err());
    let mut data = pld();
    let directory = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
    data[directory + 4..directory + 8].copy_from_slice(&1u32.to_le_bytes());
    assert!(parse_pld(&data).is_err());
}

#[test]
fn a_weapon_replaces_the_hand_and_pastes_its_texels_with_their_colours() {
    let mut player = parse_pld(&container(&[
        edd(),
        emr(true),
        md1(),
        tim(256, 256, 2, |x, _| if x >= 128 { 5 } else { 0 }),
    ]))
    .unwrap();
    player.mesh.objects = vec![TmdObject::default(); 15];
    let weapon = Plw {
        keyframes: vec![Keyframe::default(); 3],
        clips: vec![Clip::default()],
        mesh: parse_md1(&md1()).unwrap(),
        texture: tim::decode_8bpp(&tim(56, 32, 1, |x, _| if x == 0 { 5 } else { 9 })).unwrap(),
    };
    let model = with_weapon(&player, &weapon);
    assert_eq!(model.mesh.objects[WEAPON_PART], weapon.mesh.objects[0]);
    assert_eq!(model.mesh.objects[0], TmdObject::default());
    assert_eq!(model.keyframes.len(), 3);
    // The character's own clips stay reachable as the further bank.
    assert_eq!(model.banks[0].keyframes, player.keyframes);
    // A weapon with no keyframes (the knife) keeps the character's clips.
    let knife = Plw {
        keyframes: Vec::new(),
        clips: vec![Clip::default()],
        ..weapon.clone()
    };
    let model = with_weapon(&player, &knife);
    assert_eq!(
        (model.keyframes, model.clips),
        (player.keyframes.clone(), player.clips.clone())
    );
    assert!(model.banks.is_empty());
    let model = with_weapon(&player, &weapon);
    let texture = &model.texture;
    let at = |x: usize, y: usize| texture.indices[y * 256 + x];
    // Outside the box the page is untouched.
    assert_eq!(at(128 + 71, 224), 5);
    assert_eq!(at(128 + 72, 223), 5);
    // Each pasted texel shows the weapon's colour through palette row 1.
    for (x, source) in [(0usize, 5u8), (1, 9)] {
        let index = at(128 + 72 + x, 224);
        assert_eq!(
            texture.palette(1, index),
            weapon.texture.palette(0, source),
            "texel {x}"
        );
    }
    // Row 1's own entry 5 is still what the rest of the page uses.
    assert_eq!(texture.palette(1, 5), player.texture.palette(1, 5));
}

fn flat(width: u32, height: u32, index: u8, palettes: Vec<[u8; 4]>) -> Texture8 {
    Texture8 {
        width,
        height,
        indices: vec![index; (width * height) as usize],
        stp: vec![false; palettes.len()],
        palettes,
    }
}

#[test]
fn a_weapon_pastes_only_inside_a_narrow_player_texture() {
    let palettes = (0..512u32).map(|i| [i as u8, 0, 0, 255]).collect();
    let player = flat(220, 256, 3, palettes);
    let weapon = flat(56, 32, 1, vec![[7, 7, 7, 255]; 256]);
    let texture = weapon_texture(&player, &weapon);
    assert_eq!(texture.indices.len(), player.indices.len());
    for y in 0..256 {
        // Column 0 of each row is outside the box, so no paste wrapped into it.
        assert_eq!(texture.indices[y * 220], 3, "row {y}");
    }
    assert_ne!(texture.indices[224 * 220 + 128 + 72], 3);
}

#[test]
fn a_full_palette_row_never_maps_a_weapon_colour_to_the_transparent_slot() {
    // Every slot of row 1 is used and none holds the weapon's colour; slot 0
    // is the nearest one.
    let mut palettes: Vec<[u8; 4]> = (0..512u32)
        .map(|i| [255, (i % 256) as u8, 255, 255])
        .collect();
    palettes[256] = [0, 0, 0, 0];
    let mut player = flat(256, 256, 0, palettes);
    for (i, index) in player.indices.iter_mut().enumerate() {
        *index = ((i % 256 + i / 256) % 256) as u8;
    }
    let weapon = flat(56, 32, 1, vec![[1, 1, 1, 255]; 256]);
    let texture = weapon_texture(&player, &weapon);
    assert_ne!(texture.indices[224 * 256 + 128 + 72], 0);
}
