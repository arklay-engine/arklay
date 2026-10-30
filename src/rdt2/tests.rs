use super::*;

/// A minimal RE2 RDT: the header, one camera, a two-record zone table, a
/// one-record collision table and the camera's lights.
pub(crate) fn sample() -> Vec<u8> {
    let rid = 0x64;
    let rvd = rid + CAMERA_SIZE;
    let sca = rvd + 2 * ZONE_SIZE + 4;
    let lit = sca + 2 * SCA_RECORD_SIZE;
    let mut data = vec![0u8; lit + LIGHT_SIZE];
    data[1] = 1;
    let mut put = |at: usize, bytes: &[u8]| data[at..at + bytes.len()].copy_from_slice(bytes);
    put(OFFSETS + SCA_SLOT * 4, &(sca as u32).to_le_bytes());
    put(OFFSETS + RID_SLOT * 4, &(rid as u32).to_le_bytes());
    put(OFFSETS + RVD_SLOT * 4, &(rvd as u32).to_le_bytes());
    put(OFFSETS + LIT_SLOT * 4, &(lit as u32).to_le_bytes());

    put(rid + 2, &0x683Cu16.to_le_bytes());
    for (n, value) in [100i32, -2000, -3000, 400, 500, -600].iter().enumerate() {
        put(rid + 4 + n * 4, &value.to_le_bytes());
    }

    put(rvd, &[1, 0xFF, 0, 0]);
    put(rvd + ZONE_SIZE, &[1, 0xFF, 0, 1]);
    put(rvd + ZONE_SIZE + 4, &(-5i16).to_le_bytes());
    put(rvd + 2 * ZONE_SIZE, &[0xFF; 4]);

    put(sca, &(-900i16).to_le_bytes());
    put(sca + 2, &(-1304i16).to_le_bytes());
    put(sca + 4, &2u32.to_le_bytes());
    put(sca + 8, &(-55800i32).to_le_bytes());
    put(sca + 12, &0xC5C5_C5C5u32.to_le_bytes());
    let record = sca + SCA_RECORD_SIZE;
    put(record, &(-27905i16).to_le_bytes());
    put(record + 2, &(-17852i16).to_le_bytes());
    put(record + 4, &54620u16.to_le_bytes());
    put(record + 6, &6290u16.to_le_bytes());
    put(record + 8, &0xFE86u16.to_le_bytes());
    put(record + 10, &0x914Au16.to_le_bytes());
    put(record + 12, &63u32.to_le_bytes());

    put(lit, &[0, 0, 1, 0]);
    put(lit + 4, &[255, 128, 0, 10, 20, 30, 0, 0, 0]);
    put(lit + 13, &[255, 64, 1]);
    for (n, value) in [-100i16, 200, -300, 1, 2, 3, 0, 0, 0].iter().enumerate() {
        put(lit + 16 + n * 2, &value.to_le_bytes());
    }
    put(lit + 34, &[0x00, 0x30, 0x00, 0x10, 0, 0]);
    data
}

#[test]
fn reads_cameras_zones_and_collision() {
    let room = read(&sample()).unwrap();
    assert_eq!(room.cameras.len(), 1);
    let camera = &room.cameras[0];
    assert_eq!(camera.pos, [100, -2000, -3000]);
    assert_eq!(camera.look_at, [400, 500, -600]);
    assert_eq!(camera.fov, 0x683C >> 7);

    assert_eq!(room.zones.len(), 2);
    assert_eq!((room.zones[1].cam_from, room.zones[1].cam_to), (0, 1));
    assert_eq!(room.zones[1].corners[0], [-5, 0]);

    let sca = room.sca.unwrap();
    assert_eq!((sca.cell_x, sca.cell_z, sca.ceiling), (-900, -1304, -55800));
    assert_eq!(sca.records.len(), 1);
    let rect = sca.records[0];
    assert_eq!(
        (rect.x, rect.z, rect.width, rect.depth),
        (-27905, -17852, 54620, 6290)
    );
    assert_eq!(rect.shape(), 6);
    assert_eq!(rect.floor, 63);
}

#[test]
fn a_camera_s_lights_are_read_on_the_re1_scales() {
    let room = read(&sample()).unwrap();
    let light = room.cameras[0].light.unwrap();
    // A 12-bit ambient: 255 is full, and anything lit stays lit.
    assert_eq!(light.ambient, [0x1000, 0x405, 0x11]);
    let [first, second, third] = light.lights;
    assert_eq!(first.color, [0x80, 0x41, 0]);
    assert_eq!(first.pos, [-100, 200, -300]);
    assert_eq!((first.kind, first.radius), (0, 0x3000));
    assert_eq!(second.color, [6, 11, 16]);
    assert_eq!((second.pos, second.radius), ([1, 2, 3], 0x1000));
    assert_eq!((third.color, third.radius), ([0; 3], 0));
    // No LIT section: the camera falls back to the room's lights.
    let mut data = sample();
    data[OFFSETS + LIT_SLOT * 4..][..4].fill(0);
    assert_eq!(read(&data).unwrap().cameras[0].light, None);
}

#[test]
fn the_sample_detects_as_re2() {
    assert_eq!(
        crate::format::rdt(&sample()).dialect(),
        Some(crate::format::Dialect::Re2)
    );
}

#[test]
fn sound_sections_run_to_the_next_section() {
    let mut data = sample();
    assert_eq!(sound_sections(&data).unwrap(), None);
    let end = data.len();
    data.extend([0xAA; 8]);
    data.extend([0xBB; 4]);
    data[OFFSETS..OFFSETS + 4].copy_from_slice(&(end as u32).to_le_bytes());
    data[OFFSETS + 4..OFFSETS + 8].copy_from_slice(&(end as u32 + 8).to_le_bytes());
    // A bad offset in a section not read neither fails nor ends one.
    data[OFFSETS + 20..OFFSETS + 24].copy_from_slice(&u32::MAX.to_le_bytes());
    let (edt, vh) = sound_sections(&data).unwrap().unwrap();
    assert_eq!(edt, [0xAA; 8]);
    assert_eq!(vh, [0xBB; 4]);
}

#[test]
fn truncated_tables_fail_cleanly() {
    let data = sample();
    for len in [0, 1, 0x20, 0x70, data.len() - 1] {
        assert!(read(&data[..len]).is_err(), "{len} bytes");
    }
    // A zone table that runs off the end without its terminator.
    let rvd = 0x64 + CAMERA_SIZE;
    let mut unterminated = data.clone();
    unterminated.truncate(rvd + 2 * ZONE_SIZE);
    unterminated[OFFSETS + SCA_SLOT * 4..][..4].fill(0);
    assert!(read(&unterminated).is_err());
    // A bad offset in a section that is not read leaves the room loadable.
    let mut stray = data.clone();
    stray[OFFSETS..][..4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(read(&stray).is_ok());
    let mut overlong = sample();
    let sca = u32::from_le_bytes(overlong[OFFSETS + SCA_SLOT * 4..][..4].try_into().unwrap());
    overlong[sca as usize + 4..][..4].copy_from_slice(&1000u32.to_le_bytes());
    assert!(read(&overlong).is_err());
}

#[test]
fn the_definition_round_trips_through_xml() {
    let room = read(&sample()).unwrap();
    let text = crate::room::xml::write(&room).unwrap();
    assert_eq!(crate::room::xml::parse(&text).unwrap(), room);
}

/// [`sample`] with a two-group mask table on its camera: a square sprite
/// and a rectangle in the first group, a square in the second.
fn sample_with_masks() -> Vec<u8> {
    let mut data = sample();
    let table = data.len();
    let rid = u32_at(&data, OFFSETS + RID_SLOT * 4).unwrap() as usize;
    data[rid + 0x1C..rid + 0x20].copy_from_slice(&(table as u32).to_le_bytes());
    for word in [2u16, 3, 2, 0x7800, 10, 20, 1, 0x7800, 0, 100] {
        data.extend_from_slice(&word.to_le_bytes());
    }
    data.extend_from_slice(&[0, 0, 1, 2, 30, 0, 16, 0]);
    data.extend_from_slice(&[16, 0, 3, 4, 31, 0, 0, 0, 24, 0, 8, 0]);
    data.extend_from_slice(&[0, 64, 5, 6, 2, 0, 32, 0]);
    data
}

#[test]
fn reads_a_camera_s_mask_table() {
    let room = read(&sample_with_masks()).unwrap();
    let groups = &room.cameras[0].mask_groups;
    assert_eq!(groups.len(), 2);
    assert_eq!((groups[0].origin, groups[0].tex_bits), ((10, 20), 0x7800));
    assert_eq!(groups[0].sprites[0].size, None);
    assert_eq!(groups[0].sprites[0].flags, 16);
    assert_eq!(groups[0].sprites[1].size, Some((24, 8)));
    assert_eq!(groups[1].sprites[0].depth, 2);

    let pack = crate::pack::PackWriter::new().to_bytes().unwrap();
    let pack = crate::pack::Pack::from_bytes(pack).unwrap();
    let state = room
        .build(crate::state::RoomId::parse("100").unwrap(), &pack)
        .unwrap();
    let cut = &state.cuts[0];
    assert_eq!(cut.mask_dialect, Dialect::Re2);
    let sprites: Vec<_> = cut
        .masks
        .iter()
        .map(|sprite| (sprite.group, sprite.pos, sprite.size))
        .collect();
    assert_eq!(
        sprites,
        [
            (2, (11, 22), (16, 16)),
            (2, (13, 24), (24, 8)),
            (1, (5, 106), (32, 32)),
        ]
    );
    assert_eq!(cut.mask_active, 0b11);
}

#[test]
fn a_mask_table_round_trips_through_xml() {
    let room = read(&sample_with_masks()).unwrap();
    let text = crate::room::xml::write(&room).unwrap();
    assert!(text.contains(r#"size="16""#), "{text}");
    assert_eq!(crate::room::xml::parse(&text).unwrap(), room);
}

#[test]
fn reads_the_footstep_zones_by_their_count() {
    let mut data = sample();
    let table = data.len();
    data[OFFSETS + FLR_SLOT * 4..OFFSETS + FLR_SLOT * 4 + 4]
        .copy_from_slice(&(table as u32).to_le_bytes());
    for word in [
        2u16, 0xFC18, 500, 2000, 3000, 0x17, 0, 100, 200, 300, 400, 0x1A, 0x83,
    ] {
        data.extend_from_slice(&word.to_le_bytes());
    }
    let room = read(&data).unwrap();
    assert_eq!(room.footstep_count, 2);
    let zone = room.footstep_zones[0];
    assert_eq!(
        (zone.base_x, zone.base_z, zone.width, zone.height),
        (0xFC18, 500, 2000, 3000)
    );
    assert_eq!((zone.sound_data, zone.flags), (0x17, 0));
    assert_eq!(room.footstep_zones[1].flags, 0x83);
    let text = crate::room::xml::write(&room).unwrap();
    assert!(text.contains(r#"flags="0x0083""#), "{text}");
    assert_eq!(crate::room::xml::parse(&text).unwrap(), room);

    // A count past the file's end fails rather than reading short.
    data.truncate(data.len() - 2);
    assert!(read(&data).is_err());
}

#[test]
fn reads_the_last_room_animations_for_the_player() {
    let mut data = sample();
    assert!(room_anim(&data).is_none());
    let start = data.len();
    data[OFFSETS + RBJ_SLOT * 4..][..4].copy_from_slice(&(start as u32).to_le_bytes());
    // A header, then two banks for the player and one for another character.
    for word in [44u32, 3] {
        data.extend(word.to_le_bytes());
    }
    for (mask, emr, edd) in [(1u32, 1u8, 2u8), (5, 4, 5), (2, 6, 7)] {
        data.extend(mask.to_le_bytes());
        data.extend([emr; 4]);
        data.extend([edd; 4]);
    }
    for bank in [[8u32, 16], [20, 28], [32, 40]] {
        data.extend(bank.iter().flat_map(|word| word.to_le_bytes()));
    }
    let (emr, edd) = room_anim(&data).unwrap();
    assert_eq!(emr, [4u8; 4]);
    assert_eq!(edd, [5u8; 4]);
}
