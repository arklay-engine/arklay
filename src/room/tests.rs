use std::path::{Path, PathBuf};

use super::*;
use crate::charset::Script;
use crate::pack::PackWriter;
use crate::room::{ScriptSource, script_source};

/// The room state's every field, for comparing a room loaded two ways.
///
/// A model's `prop` stem only names its HD replacement, which an RDT has no
/// name for, so it is left out.
fn snapshot(state: &RoomState) -> String {
    let mut state = state.clone();
    for asset in state.object_models.iter_mut().chain(&mut state.item_models) {
        asset.hd_name = None;
    }
    format!("{state:?}")
}

/// A pack holding `entries`.
fn pack_of(entries: Vec<(String, Vec<u8>)>) -> Pack {
    let mut writer = PackWriter::new();
    for (path, data) in entries {
        writer.add(&path, data).unwrap();
    }
    Pack::from_bytes(writer.to_bytes().unwrap()).unwrap()
}

/// Convert `rdts` into a pack of room folders.
fn converted(rdts: &[(RoomId, Vec<u8>)]) -> Pack {
    let mut entries = Vec::new();
    for (id, data) in rdts {
        entries.extend(
            convert_rdt(data, *id, Vec::new())
                .unwrap_or_else(|error| panic!("{:04X}: {error:#}", id.rdt_number())),
        );
    }
    pack_of(entries)
}

/// Assert that `pack`'s XML definition of `id` loads the state and scripts
/// the RDT does, but for the RDT's message text, which the XML leaves to its
/// strings.
fn assert_same_room(pack: &Pack, id: RoomId, rdt: &[u8]) {
    let mut expected = crate::rdt::parse(rdt, id).unwrap();
    expected.messages = None;
    let loaded = load(pack, id).unwrap();
    assert_eq!(
        snapshot(&loaded.state),
        snapshot(&expected),
        "room {:04X} differs",
        id.rdt_number()
    );
    let scripts = loaded.own_scripts(pack).unwrap();
    match scd::reader::parse(rdt) {
        Ok(original) => assert_eq!(
            scripts.digest(),
            original.digest(),
            "room {:04X} scripts differ",
            id.rdt_number()
        ),
        Err(_) => assert_eq!(scripts, scd::ir::Scripts::default()),
    }
}

/// File `name` in RDT `id`'s own folder (`room/2051/room.xml`), where a
/// standalone RDT conversion puts it.
fn rdt_file(id: RoomId, name: &str) -> String {
    format!("{}/{name}", id.rdt_dir())
}

const ID: RoomId = RoomId {
    stage: 2,
    room: 5,
    player_flag: 1,
    mansion2: false,
};

fn put_u16(data: &mut [u8], at: usize, value: u16) {
    data[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(data: &mut [u8], at: usize, value: u32) {
    data[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

/// A synthetic RDT with a camera and its mask table, switch zones, collision,
/// walk and footstep zones, a message block and an init script.
fn synthetic_rdt() -> Vec<u8> {
    let mut data = vec![0u8; 0x94 + 44];
    data[0x01] = 1;
    put_u16(&mut data, 0x06, 0x0123);
    // Light 0: a point light with the widest radius.
    put_u32(&mut data, 0x0C, (-500i32) as u32);
    data[0x18..0x1B].copy_from_slice(&[10, 20, 30]);
    put_u16(&mut data, 0x1E, 0xFFFF);
    // Camera 0.
    for (field, value) in [(2, 1000), (3, -2000), (4, 3000), (5, 4), (10, 750)] {
        put_u32(&mut data, 0x94 + field * 4, value as u32);
    }
    let pointer = |data: &mut Vec<u8>, slot: usize| {
        let at = data.len();
        put_u32(data, 0x48 + slot * 4, at as u32);
    };

    // Mask table: one group, a packed and an explicit-size sprite.
    let mask = data.len();
    put_u32(&mut data, 0x94, mask as u32);
    data.extend_from_slice(&1i32.to_le_bytes());
    for word in [2u16, 0x0011, 10, 20] {
        data.extend_from_slice(&word.to_le_bytes());
    }
    for word in [
        0x0201u16, 0x0403, 380, 0x1800, 0x0605, 0x0807, 90, 0x0003, 8, 40,
    ] {
        data.extend_from_slice(&word.to_le_bytes());
    }

    pointer(&mut data, 0);
    for (to, from, corner) in [(0i16, 0i16, -1i16), (1, 0, 100), (0, 1, 200)] {
        data.extend_from_slice(&to.to_le_bytes());
        data.extend_from_slice(&from.to_le_bytes());
        for _ in 0..8 {
            data.extend_from_slice(&corner.to_le_bytes());
        }
    }
    data.extend_from_slice(&u32::MAX.to_le_bytes());

    pointer(&mut data, 1);
    for word in [100i16, 200] {
        data.extend_from_slice(&word.to_le_bytes());
    }
    for count in [1i32, 0, 2, 0, 0x55] {
        data.extend_from_slice(&count.to_le_bytes());
    }
    for _ in 0..3 {
        for word in [60000u16, 2, 3, 4, 0x8001, 0x0102] {
            data.extend_from_slice(&word.to_le_bytes());
        }
    }

    pointer(&mut data, 4);
    data.extend_from_slice(&[1, 0]);
    for word in [-30000i16, 5, 30000, 6, 0x0708, 0x090A] {
        data.extend_from_slice(&word.to_le_bytes());
    }

    pointer(&mut data, 5);
    data.extend_from_slice(&2u16.to_le_bytes());
    for zone in [[0u16, 0, 500, 500, 0x0301], [0, 0, 0xFFFF, 0xFFFF, 0x0002]] {
        for word in zone {
            data.extend_from_slice(&word.to_le_bytes());
        }
    }

    // Init script: one block holding `nop`-sized filler, then the chain end.
    pointer(&mut data, 6);
    data.extend_from_slice(&4u16.to_le_bytes());
    data.extend_from_slice(&[0x00, 0x00]);
    data.extend_from_slice(&0u16.to_le_bytes());

    pointer(&mut data, 11);
    let mut text = Vec::new();
    crate::charset::encode("A key door", Script::Latin, &mut text);
    let first: Vec<u8> = [text.as_slice(), &[0x02, 0x05, 0x01], &[0x01, 0x00]].concat();
    let second = vec![0x06, 0x00, 0x07, 0x08, 0x01, 0x0A, 0x05, 0x01, 0x00];
    let table = [4u16, 4 + first.len() as u16];
    for offset in table {
        data.extend_from_slice(&offset.to_le_bytes());
    }
    data.extend_from_slice(&first);
    data.extend_from_slice(&second);
    // A section after the message block bounds it.
    pointer(&mut data, 12);
    data.extend_from_slice(&[0; 4]);
    data
}

#[test]
fn a_synthetic_room_loads_the_same_from_xml() {
    let rdt = synthetic_rdt();
    let pack = converted(&[(ID, rdt.clone())]);
    assert_eq!(format(&pack, ID), Some(Format::Xml));
    assert!(!pack.contains(&rdt_file(ID, "room.rdt")));
    assert_same_room(&pack, ID, &rdt);

    let text = std::str::from_utf8(&pack.read(&rdt_file(ID, "room.xml")).unwrap())
        .unwrap()
        .to_string();
    // The RDT's message text is not carried over: a room's text is its
    // strings.
    assert!(!text.contains("<message"), "{text}");
    assert!(
        text.contains("<view zoneIndex=\"0\" to=\"0\" x0=\"65535\""),
        "{text}"
    );
    assert!(text.contains("<switch zoneIndex=\"1\" to=\"1\""), "{text}");
    assert!(text.contains("radius=\"65535\""), "{text}");
    // Nothing in the definition names a file or its own room.
    assert!(!text.contains("src=") && !text.contains("room/"), "{text}");
    assert!(!text.contains(" id=\"2051\""), "{text}");
    assert!(pack.contains(&rdt_file(ID, "room.scd")));
}

#[test]
fn a_room_folder_renamed_is_the_room_of_its_new_id() {
    let rdt = synthetic_rdt();
    let other = RoomId::parse("1000").unwrap();
    let from = format!("{}/", ID.rdt_dir());
    let renamed: Vec<(String, Vec<u8>)> = convert_rdt(&rdt, ID, Vec::new())
        .unwrap()
        .into_iter()
        .map(|(path, data)| {
            let file = path.strip_prefix(&from).unwrap();
            (format!("{}/{file}", other.rdt_dir()), data)
        })
        .collect();
    let pack = pack_of(renamed);
    // Every variant of the folder's room.
    assert_eq!(rooms(&pack), [other]);
    assert!(ids(&pack).contains(&other));
    let state = load_state(&pack, other).unwrap();
    assert_eq!(state.cuts.len(), 1);
    assert!(!own_scripts(&pack, other).unwrap().init.is_empty());
}

#[test]
fn a_stub_rdt_is_an_empty_room() {
    let stub = vec![0x00, 0x00, 0x02, 0x01];
    let pack = converted(&[(ID, stub.clone())]);
    let state = load_state(&pack, ID).unwrap();
    assert!(state.cuts.is_empty());
    assert_eq!((state.omodel_slot_count, state.item_count), (2, 1));
    assert_same_room(&pack, ID, &stub);
}

/// A base pack of `base` entries with a mod layer of `layer` entries over
/// it, written under a fresh temporary directory named after `name`.
fn layered(
    name: &str,
    base: Vec<(String, Vec<u8>)>,
    layer: Vec<(String, Vec<u8>)>,
) -> (Pack, PathBuf) {
    layered_in(name, None, base, layer)
}

/// [`layered`] over a base pack whose manifest records `region`.
fn layered_in(
    name: &str,
    region: Option<crate::scd::akscr::Region>,
    base: Vec<(String, Vec<u8>)>,
    layer: Vec<(String, Vec<u8>)>,
) -> (Pack, PathBuf) {
    let dir = std::env::temp_dir().join(format!("arklay-room-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let base_path = dir.join("base.akpak");
    let mod_path = dir.join("mod.akpak");

    let mut writer = PackWriter::new();
    writer
        .add(
            crate::manifest::ENTRY,
            crate::manifest::Manifest {
                region,
                ..crate::manifest::Manifest::base("re1")
            }
            .render()
            .into_bytes(),
        )
        .unwrap();
    for (path, data) in base {
        writer.add(&path, data).unwrap();
    }
    writer.write(&base_path).unwrap();

    let mut writer = PackWriter::new();
    let manifest = crate::manifest::Manifest {
        kind: crate::manifest::PackKind::Mod,
        base: Some("re1".to_string()),
        load_order: 10,
        ..crate::manifest::Manifest::base("room-mod")
    };
    writer
        .add(crate::manifest::ENTRY, manifest.render().into_bytes())
        .unwrap();
    for (path, data) in layer {
        writer.add(&path, data).unwrap();
    }
    writer.write(&mod_path).unwrap();
    let pack = Pack::open_layered(&base_path, std::slice::from_ref(&mod_path)).unwrap();
    (pack, dir)
}

/// The synthetic room converted: its XML and every entry of its folder.
fn converted_entries() -> (String, Vec<(String, Vec<u8>)>) {
    let entries = convert_rdt(&synthetic_rdt(), ID, Vec::new()).unwrap();
    let xml = entries
        .iter()
        .find(|(path, _)| *path == rdt_file(ID, "room.xml"))
        .map(|(_, data)| String::from_utf8(data.clone()).unwrap())
        .unwrap();
    (xml, entries)
}

/// The bytes of entry `path` among `entries`.
fn entry_of(entries: &[(String, Vec<u8>)], path: &str) -> Vec<u8> {
    entries
        .iter()
        .find(|(entry, _)| entry == path)
        .unwrap_or_else(|| panic!("no {path}"))
        .1
        .clone()
}

#[test]
fn the_higher_layer_definition_wins() {
    let (_, base) = converted_entries();
    let (pack, dir) = layered(
        "layers",
        base.clone(),
        vec![(rdt_file(ID, "room.rdt"), vec![0, 0, 0, 0])],
    );
    assert_eq!(format(&pack, ID), Some(Format::Rdt));
    assert!(load_state(&pack, ID).unwrap().cuts.is_empty());
    assert!(ids(&pack).contains(&ID));
    assert_eq!(format(&pack_of(base), ID), Some(Format::Xml));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_xml_only_mod_keeps_the_base_scripts() {
    let (xml, base) = converted_entries();
    let container = entry_of(&base, &rdt_file(ID, "room.scd"));
    let edited = xml.replacen("<ambient", "<!-- edited --><ambient", 1);

    // An edit of the XML alone keeps the base layer's `room.scd`.
    let only_xml = vec![(rdt_file(ID, "room.xml"), edited.clone().into_bytes())];
    let (pack, dir) = layered("xml-mod", base.clone(), only_xml);
    assert_eq!(script_source(&pack, ID), ScriptSource::Room);
    assert!(!own_scripts(&pack, ID).unwrap().init.is_empty());
    let _ = std::fs::remove_dir_all(&dir);

    // An RDT mod over the XML room runs the scripts embedded in it.
    let rdt = vec![(rdt_file(ID, "room.rdt"), synthetic_rdt())];
    let (pack, dir) = layered("rdt-mod", base, rdt);
    assert_eq!(script_source(&pack, ID), ScriptSource::Rdt);
    assert_eq!(
        own_scripts(&pack, ID).unwrap().digest(),
        crate::scd::reader::parse(&container).unwrap().digest()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A standalone container whose init sets flag `bit` of `FG_SCENARIO`, so
/// each script set is told apart by what it parses to.
fn flag_container(bit: u8) -> Vec<u8> {
    crate::scd::asm::assemble(&format!(
        ".version 1\n\n.init\n.block\n    set FG_SCENARIO, {bit}, 0\n\n.main\n"
    ))
    .unwrap()
    .to_container()
    .unwrap()
}

#[test]
fn a_mods_room_scd_replaces_the_base_scripts() {
    use crate::engine::room_scripts;
    let parse = |container: &[u8]| crate::scd::reader::parse(container).unwrap().digest();
    let (_, base) = converted_entries();
    let modded = flag_container(2);
    assert_ne!(
        parse(&entry_of(&base, &rdt_file(ID, "room.scd"))),
        parse(&modded)
    );
    let with_scripts = vec![(rdt_file(ID, "room.scd"), modded.clone())];
    let (pack, dir) = layered("scd-mod", base, with_scripts);
    let loaded = load(&pack, ID).unwrap();
    assert_eq!(script_source(&pack, ID), ScriptSource::Room);
    let (scripts, _) = room_scripts(&pack, ID, &loaded).unwrap();
    assert_eq!(scripts.digest(), parse(&modded));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_format_2_room_reads_without_its_messages() {
    let (xml, _) = converted_entries();
    let current = xml::parse(&xml).unwrap();
    // Format 2 kept the RDT's message text, which a room no longer has: its
    // text is its `<strings>`.
    let messages =
        "<messages script=\"latin\">\n<message id=\"0\">A</message>\n</messages>\n</room>";
    let old = xml
        .replacen("format=\"3\"", "format=\"2\"", 1)
        .replacen("</room>", messages, 1);
    assert_eq!(xml::parse(&old).unwrap(), current);
    // Format 3 has no such block, and no other format reads.
    let error = format!(
        "{:#}",
        xml::parse(&xml.replacen("</room>", messages, 1)).unwrap_err()
    );
    assert!(error.contains("unknown room element <messages>"), "{error}");
    let error = format!(
        "{:#}",
        xml::parse(&xml.replacen("format=\"3\"", "format=\"1\"", 1)).unwrap_err()
    );
    assert!(error.contains("format 1 is not supported"), "{error}");
}

#[test]
fn hand_edits_with_unknown_names_are_refused() {
    let (text, _) = converted_entries();
    let typo = text.replacen("cellX", "celX", 1);
    let error = format!("{:#}", xml::parse(&typo).unwrap_err());
    assert!(error.contains("unknown attribute `celX`"), "{error}");
    let element = text.replacen("<walkZones>", "<walkZones><wlak/>", 1);
    assert!(xml::parse(&element).is_err());
    // The room id is the folder's, not the definition's.
    let id = text.replacen("<room ", "<room id=\"2051\" ", 1);
    assert!(xml::parse(&id).is_err());
}

/// The shipped RDTs under `ARKLAY_RE1_ROOT` (the JPN tree) and, when
/// present beside it, every Steam language tree.
fn shipped_trees() -> Option<Vec<PathBuf>> {
    let root = PathBuf::from(std::env::var("ARKLAY_RE1_ROOT").ok()?);
    let mut trees = vec![root.join("JPN")];
    let steam = std::env::var("ARKLAY_RE1_STEAM_ROOT")
        .ok()
        .map(PathBuf::from);
    if let Some(steam) = steam {
        for (language, tree) in [
            ("english", "USA"),
            ("french", "FRA"),
            ("german", "GER"),
            ("japanese", "JPN"),
        ] {
            trees.push(steam.join(language).join(tree));
        }
    }
    Some(trees)
}

/// Every `ROOM####.RDT` in a tree's stage directories, by id.
fn tree_rdts(tree: &Path) -> Vec<(RoomId, Vec<u8>)> {
    let mut rdts = Vec::new();
    for stage in std::fs::read_dir(tree).unwrap().flatten() {
        let name = stage.file_name().to_string_lossy().to_ascii_lowercase();
        if !name.starts_with("stage") {
            continue;
        }
        for file in std::fs::read_dir(stage.path()).unwrap().flatten() {
            let name = file.file_name().to_string_lossy().to_string();
            if let Some(Ok(id)) = crate::convert::rdt_id_from_file_name(&name) {
                rdts.push((id, std::fs::read(file.path()).unwrap()));
            }
        }
    }
    rdts.sort_by_key(|(id, _)| id.rdt_number());
    rdts
}

#[test]
#[ignore = "requires a real RE1 installation via ARKLAY_RE1_ROOT"]
fn every_shipped_room_loads_the_same_from_xml() {
    let Some(trees) = shipped_trees() else {
        return;
    };
    for tree in trees {
        let rdts = tree_rdts(&tree);
        assert!(
            rdts.len() > 300,
            "{}: only {} RDTs",
            tree.display(),
            rdts.len()
        );
        let pack = converted(&rdts);
        for (id, rdt) in &rdts {
            assert_same_room(&pack, *id, rdt);
        }
        assert!(!pack.paths().any(|path| path.ends_with(".rdt")));

        let room = |text: &str| RoomId::parse(text).unwrap();
        let state = |text: &str| load_state(&pack, room(text)).unwrap();
        // Room 1000: seven mask groups on camera 0.
        assert_eq!(state("1000").cuts[0].mask_group_count, 7);
        // Room 3010: nine object slots.
        assert_eq!(state("3010").omodel_slot_count, 9);
        // Room 1070: the ladder and the shelf are object models, each named
        // by the file its HD replacement is keyed on.
        let models = state("1070").object_models;
        assert!(!models.is_empty());
        assert!(models.iter().all(|asset| {
            asset.hd_name.as_deref().is_some_and(|stem| {
                stem.starts_with("room/1070/object/") && pack.contains(&format!("{stem}.tmd"))
            })
        }));
        // Room 10C1: item models.
        assert!(!state("10C1").item_models.is_empty());
        // A stub room is empty.
        let stub = rdts.iter().find(|(_, data)| data.len() < 0x94).unwrap();
        assert!(load_state(&pack, stub.0).unwrap().cuts.is_empty());
        // Some room declares effects and has its sheet art as a BMP.
        assert!(rdts.iter().any(|(id, _)| {
            let effects = load_state(&pack, *id).unwrap().effects;
            !effects.is_empty()
                && effects.sprites.iter().all(|sprite| sprite.tim.is_some())
                && pack.contains(&format!("{}/effect/00.bmp", id.rdt_dir()))
        }));
        // Every room is a folder: nothing is shared by content.
        assert!(pack.paths().all(|path| path.starts_with("room/")));
    }
}

#[test]
fn slots_name_the_files_each_variant_reads() {
    let (xml, _) = converted_entries();
    let xml = xml.replacen(
        "<footsteps",
        "<slots>\n    <item when=\"JILL MANSION2\">-</item>\n    \
         <item when=\"JILL\">02 01</item>\n    <item>00 01</item>\n    \
         <anim when=\"JILL\">01</anim>\n    <effect when=\"JILL\">03</effect>\n  \
         </slots>\n  <footsteps",
        1,
    );
    let room = RoomId::parse("205").unwrap();
    // Every file is broken, so each one a variant reads warns by name.
    let mut entries = vec![(room.xml_entry(), xml.clone().into_bytes())];
    for file in ["item/00", "item/01", "item/02", "anim/00", "anim/01"] {
        for extension in ["tmd", "emr", "edd"] {
            entries.push((format!("room/205/{file}.{extension}"), b"broken".to_vec()));
        }
    }
    let pack = pack_of(entries);
    let read = |id: RoomId| {
        let state = load_state(&pack, id).unwrap();
        let mut files: Vec<String> = state
            .model_warnings
            .iter()
            .chain(&state.effects.warnings)
            .filter_map(|warning| warning.split("room/205/").nth(1))
            .map(|file| file.split([':', '.']).next().unwrap().to_string())
            .collect();
        files.sort();
        (state.item_count, files)
    };
    let chris = RoomId {
        player_flag: 0,
        ..room
    };
    assert_eq!(
        read(chris),
        (
            2,
            vec!["anim/00".into(), "item/00".into(), "item/01".into()]
        )
    );
    assert_eq!(
        read(ID),
        (
            2,
            vec![
                "anim/01".into(),
                "effect/03".into(),
                "item/01".into(),
                "item/02".into()
            ]
        )
    );

    // The written room keeps the variant's lists.
    let source = xml::parse_for(&xml, ID).unwrap();
    assert_eq!(source.slots.item, Some(vec![Some(2), Some(1)]));
    let written = xml::write(&source).unwrap();
    assert!(
        written.contains("<slots>\n    <item>02 01</item>\n"),
        "{written}"
    );
    assert_eq!(xml::parse(&written).unwrap().slots, source.slots);
    // An empty list empties the kind.
    let source = xml::parse_for(&xml, ID.with_visit(true)).unwrap();
    assert_eq!(source.slots.item, Some(vec![None]));
    // A list is two hex digits or `-` per slot.
    for bad in [
        "<item>0</item>",
        "<item>xx</item>",
        "<anim>00 01</anim>",
        "<sound>00</sound>",
    ] {
        let text = xml.replacen("<item>00 01</item>", bad, 1);
        assert!(xml::parse_for(&text, chris).is_err(), "{bad}");
    }
}

#[test]
fn verify_fails_a_room_with_a_broken_model() {
    let (_, mut entries) = converted_entries();
    let model = format!("{}/object/00", ID.rdt_dir());
    entries.push((format!("{model}.tmd"), b"not a tmd".to_vec()));
    let report = crate::verify::verify_pack(&pack_of(entries), false);
    let failure = report
        .failures
        .iter()
        .find(|failure| failure.path == rdt_file(ID, "room.xml"))
        .expect("the room fails");
    assert!(failure.error.contains(&model), "{}", failure.error);
}

#[test]
fn a_japanese_stage_five_rdt_draws_its_text_from_the_stage_five_rows() {
    // `0xFA 0x26` is the cell `PBFONT3N.TIM` draws as 第.
    let state = |stage| RoomState {
        stage,
        messages: Some(vec![0x02, 0x00, 0xFA, 0x26, 0x01]),
        ..RoomState::default()
    };
    let drawn = drawn_messages(state(5), Script::Japanese);
    assert_eq!(
        drawn.messages.as_deref(),
        Some([0x02, 0x00, 0xFA, 0xA6, 0x01].as_slice())
    );
    // Already-moved text stays put, so a room built from its XML matches.
    assert_eq!(
        drawn_messages(drawn.clone(), Script::Japanese).messages,
        drawn.messages
    );
    // Other stages and the Western script keep the kanji page's cell.
    for (stage, script) in [(4, Script::Japanese), (5, Script::Latin)] {
        assert_eq!(
            drawn_messages(state(stage), script).messages,
            state(stage).messages
        );
    }
}

#[test]
fn a_camera_s_own_lighting_round_trips_beside_its_mask_groups() {
    use crate::state::{CameraLight, Light};

    let (xml, _) = converted_entries();
    let mut room = xml::parse(&xml).unwrap();
    let light = CameraLight {
        ambient: [0x1000, 0x405, 0x11],
        lights: [
            Light {
                pos: [-100, 200, 300],
                color: [0x80, 0x41, 0],
                kind: 0,
                radius: 0x2000,
            },
            Light::default(),
            Light {
                pos: [1, 2, 3],
                color: [6, 11, 16],
                kind: 0,
                radius: 7,
            },
        ],
    };
    room.cameras.push(room.cameras[0].clone());
    room.cameras[0].mask_groups = Vec::new();
    room.cameras[0].light = Some(light);
    // The pushed copy keeps the synthetic camera's mask groups, if any.
    let last = room.cameras.len() - 1;
    room.cameras[last].light = Some(light);
    let text = xml::write(&room).unwrap();
    assert!(text.contains("<lighting "), "{text}");
    let back = xml::parse(&text).unwrap();
    assert_eq!(back, room);
    // A camera without lighting reads back without it.
    assert!(
        back.cameras[1..last]
            .iter()
            .all(|camera| camera.light.is_none())
    );
}

#[test]
fn a_camera_s_lighting_must_come_once_before_its_masks_with_three_lights() {
    let (xml, _) = converted_entries();
    let camera = xml.find("<camera ").unwrap();
    let end = camera + xml[camera..].find('>').unwrap();
    let open = xml[camera..end].trim_end_matches('/').to_string() + ">";
    let light = "<light x=\"0\" y=\"0\" z=\"0\" r=\"0\" g=\"0\" b=\"0\" kind=\"0\" radius=\"0\"/>";
    let lighting = |count: usize| {
        format!(
            "<lighting r=\"0\" g=\"0\" b=\"0\">{}</lighting>",
            light.repeat(count)
        )
    };
    let mask = "<maskGroup originX=\"0\" originY=\"0\"/>";
    let with = |body: String| {
        let close = if xml[..=end].ends_with("/>") {
            end + 1
        } else {
            camera + xml[camera..].find("</camera>").unwrap() + "</camera>".len()
        };
        format!("{}{open}{body}</camera>{}", &xml[..camera], &xml[close..])
    };
    let good = xml::parse(&with(lighting(3) + mask)).unwrap();
    assert!(good.cameras[0].light.is_some());
    for (bad, why) in [
        (
            mask.to_string() + &lighting(3),
            "lighting after a mask group",
        ),
        (lighting(3) + &lighting(3), "two lightings"),
        (lighting(2), "two lights"),
        (lighting(4), "four lights"),
    ] {
        assert!(xml::parse(&with(bad)).is_err(), "{why}");
    }
}

/// `(tag, key, value)` strings, owned.
fn strings(values: &[(&str, &str, &str)]) -> Vec<crate::language::XmlString> {
    values
        .iter()
        .map(|(tag, key, value)| (tag.to_string(), key.to_string(), value.to_string()))
        .collect()
}

#[test]
fn a_room_s_strings_round_trip_and_become_room_keys() {
    let named = strings(&[
        ("en-US", "name", "SAVE ROOM"),
        ("en-US", "save_name", "M. Room 1F"),
        ("ja-JP", "save_name", "館 {s}小部屋"),
    ]);
    let entries = convert_rdt(&synthetic_rdt(), ID, named.clone()).unwrap();
    let text = String::from_utf8(entry_of(&entries, &rdt_file(ID, "room.xml"))).unwrap();
    assert_eq!(xml::parse(&text).unwrap().strings, named);
    let read: Vec<crate::language::XmlString> = room_strings(text.as_bytes())
        .unwrap()
        .into_iter()
        .map(|(tag, key, value, _)| (tag, key, value))
        .collect();
    assert_eq!(read, named);

    // The room's folder names it; a raw RDT names nothing.
    let mut files = entries;
    files.push(("room/1000/room.rdt".to_string(), synthetic_rdt()));
    let pack = pack_of(files);
    let layers = language_layers(&pack);
    let layer = |tag: &str| &layers.iter().find(|(t, _)| t == tag).unwrap().1;
    assert_eq!(layer("en-us").len(), 2);
    assert!(layer("ja-jp").get("ROOM_205_NAME_SAVE").is_some());
    let text = crate::text::Text::load(&pack, Some("en-US"));
    assert_eq!(text.decode(text.room_name("205").unwrap()), "SAVE ROOM");
    assert_eq!(text.decode(text.room_label(2, 5).unwrap()), "M. Room 1F");
    assert_eq!(text.room_name("100"), None);
}

#[test]
fn room_message_texts_become_text_keys_per_variant() {
    let chris = RoomId {
        player_flag: 0,
        ..ID
    };
    let jill = RoomId {
        player_flag: 1,
        ..ID
    };
    // One room.xml for every variant: a string only some have carries
    // `when`.
    let written = r#"<?xml version="1.0" encoding="UTF-8"?>
<room format="3">
  <strings>
    <language lang="en-US">
      <string key="0">Same</string>
      <string key="0_speaker">Rebecca</string>
      <string key="10">Chris</string>
      <string key="10" when="JILL">Jill</string>
      <string key="10_speaker_1">Barry</string>
      <string key="10_speaker_1" when="JILL">Wesker</string>
      <string key="11" when="JILL MANSION2">Again</string>
    </language>
  </strings>
</room>
"#;
    let pack = pack_of(vec![(chris.xml_entry(), written.as_bytes().to_vec())]);
    let layers = language_layers(&pack);
    let english = &layers.iter().find(|(tag, _)| tag == "en-us").unwrap().1;
    let mut keys: Vec<&str> = english.iter().map(|(key, _)| key).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "ROOM_205_JILL_MANSION2_TEXT_0B",
            "ROOM_205_JILL_TEXT_0A",
            "ROOM_205_JILL_TEXT_0A_SPEAKER_1",
            "ROOM_205_TEXT_00",
            "ROOM_205_TEXT_00_SPEAKER",
            "ROOM_205_TEXT_0A",
            "ROOM_205_TEXT_0A_SPEAKER_1",
        ]
    );
    // Read for one variant, the room's strings are that variant's.
    let jill_strings = xml::parse_for(written, jill).unwrap().strings;
    assert!(jill_strings.contains(&("en-US".into(), "10".into(), "Jill".into())));
    assert!(!jill_strings.iter().any(|(_, key, _)| key == "11"));
    assert!(xml::parse(written).is_err(), "a `when` needs a variant");
    // Each variant reads its own text and speakers.
    let text = crate::text::Text::load(&pack, Some("en-US"));
    let state = |id: RoomId| RoomState {
        stage: id.stage,
        room: id.room,
        player_flag: id.player_flag,
        mansion2: id.mansion2,
        ..RoomState::default()
    };
    assert_eq!(
        text.message(&state(jill.with_visit(true)), 11)
            .map(|bytes| text.decode(bytes)),
        Some("Again".to_string())
    );
    assert_eq!(text.speaker(&state(jill), 0, 0).as_deref(), Some("Rebecca"));
    assert_eq!(text.speaker(&state(chris), 10, 1).as_deref(), Some("Barry"));
    assert_eq!(text.speaker(&state(jill), 10, 1).as_deref(), Some("Wesker"));
    assert_eq!(text.speaker(&state(jill), 10, 0), None);
}

#[test]
fn bad_room_strings_are_refused() {
    for bad in [
        r#"<room><strings><language lang="en-US"><string key="title">A</string></language></strings></room>"#,
        r#"<room><strings><language lang="en-US"><string key="01">A</string></language></strings></room>"#,
        r#"<room><strings><language lang="en-US"><string key="16384">A</string></language></strings></room>"#,
        r#"<room><strings><language lang="en-US"><string key="-1">A</string></language></strings></room>"#,
        r#"<room><strings><language lang="en-US"><string key="1_speaker_01">A</string></language></strings></room>"#,
        r#"<room><strings><language lang="en-US"><string key="1_speakers">A</string></language></strings></room>"#,
        r#"<room><strings><language lang="en-US"><string key="name_speaker">A</string></language></strings></room>"#,
        r#"<room><strings><language lang="en-US"><string key="name">A\q</string></language></strings></room>"#,
        r#"<room><strings /><strings /></room>"#,
    ] {
        assert!(xml::parse(bad).is_err(), "{bad}");
    }
}

#[test]
fn every_checked_in_re1_room_builds() {
    let rooms: Vec<RoomId> = RE1_FILES
        .iter()
        .filter_map(|(path, _)| RoomId::parse(path.strip_suffix("/room.xml")?).ok())
        .collect();
    assert!(rooms.len() > 100, "{} rooms", rooms.len());
    // A pack of the definitions alone: the models and animations are the
    // install's, and a room builds without them.
    let files = rooms
        .iter()
        .flat_map(|id| re1_definition(*id))
        .map(|(entry, text)| (entry, text.as_bytes().to_vec()))
        .collect();
    let pack = pack_of(files);
    // Every variant of every room.
    let ids = ids(&pack);
    assert!(ids.len() > 300, "{} variants", ids.len());
    for id in &ids {
        load(&pack, *id).unwrap_or_else(|err| panic!("{}: {err:#}", id.xml_entry()));
    }
    // The display names and message texts are in the rooms themselves.
    let layers = language_layers(&pack);
    let english = &layers.iter().find(|(tag, _)| tag == "en-us").unwrap().1;
    assert!(english.len() > 1_000, "{} strings", english.len());

    // Each file is in the form the writer gives it, `<strings>` last.
    for (path, text) in RE1_FILES {
        // A room whose variants differ is no one variant's writer output.
        if path.ends_with("/room.xml") && !text.contains(" when=") {
            let written = xml::write(&xml::parse(text).unwrap()).unwrap();
            assert!(written == *text, "{path} is not in canonical form");
        }
    }
}

#[test]
fn an_re2_rdt_converts_to_a_room_that_builds_the_same() {
    let data = crate::rdt2::tests::sample();
    let id = RoomId::parse("1000").unwrap();
    let entries = convert_rdt2(&data, id, Vec::new()).unwrap();
    assert_eq!(entries.len(), 2);
    let pack = pack_of(entries);
    let loaded = load(&pack, id).unwrap();
    assert_eq!(loaded.dialect, Dialect::Re2);
    let expected = Loaded::from_rdt(&pack, id, data).unwrap();
    assert_eq!(snapshot(&loaded.state), snapshot(&expected.state));
}
