use std::path::{Path, PathBuf};

use super::*;
use crate::charset::Script;
use crate::pack::PackWriter;

/// The room state's every field, for comparing a room loaded two ways.
///
/// A model's `prop` stem only names its HD replacement, which an RDT has no
/// name for, so it is left out.
fn snapshot(state: &RoomState) -> String {
    let mut state = state.clone();
    for asset in state.object_models.iter_mut().chain(&mut state.item_models) {
        asset.prop = None;
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

/// Convert `rdts` into a pack of XML definitions and sidecars.
fn converted(rdts: &[(RoomId, Vec<u8>)], script: Script) -> Pack {
    let mut sidecars = Sidecars::default();
    let mut entries = Vec::new();
    for (id, data) in rdts {
        let text = convert_rdt(data, *id, script, &mut sidecars)
            .unwrap_or_else(|error| panic!("{:04X}: {error:#}", id.rdt_number()));
        entries.push((id.xml_entry(), text.into_bytes()));
    }
    entries.extend(sidecars.into_entries());
    pack_of(entries)
}

/// Assert that `pack`'s XML definition of `id` loads the state and scripts
/// the RDT does.
fn assert_same_room(pack: &Pack, id: RoomId, rdt: &[u8]) {
    let expected = crate::rdt::parse(rdt, id).unwrap();
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

const ID: RoomId = RoomId {
    stage: 2,
    room: 5,
    player_flag: 1,
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
    let pack = converted(&[(ID, rdt.clone())], Script::Latin);
    assert_eq!(format(&pack, ID), Some(Format::Xml));
    assert!(!pack.contains(&ID.rdt_entry()));
    assert_same_room(&pack, ID, &rdt);

    let text = std::str::from_utf8(&pack.read(&ID.xml_entry()).unwrap())
        .unwrap()
        .to_string();
    assert!(
        text.contains("<message id=\"0\">A key door<br/><color n=\"1\"/><end action=\"0\"/>"),
        "{text}"
    );
    assert!(
        text.contains("<view zoneIndex=\"0\" to=\"0\" x0=\"65535\""),
        "{text}"
    );
    assert!(text.contains("<switch zoneIndex=\"1\" to=\"1\""), "{text}");
    assert!(text.contains("radius=\"65535\""), "{text}");
    assert!(text.contains("<scripts src=\"room/2051.scd\"/>"), "{text}");
}

#[test]
fn a_stub_rdt_is_an_empty_room() {
    let stub = vec![0x00, 0x00, 0x02, 0x01];
    let pack = converted(&[(ID, stub.clone())], Script::Latin);
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
    let dir = std::env::temp_dir().join(format!("arklay-room-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let base_path = dir.join("base.akpak");
    let mod_path = dir.join("mod.akpak");

    let mut writer = PackWriter::new();
    writer
        .add(
            crate::manifest::ENTRY,
            crate::manifest::Manifest::base("re1").render().into_bytes(),
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

/// The synthetic room converted: its XML and every sidecar entry.
fn converted_entries() -> (String, Vec<(String, Vec<u8>)>) {
    let mut sidecars = Sidecars::default();
    let xml = convert_rdt(&synthetic_rdt(), ID, Script::Latin, &mut sidecars).unwrap();
    let mut entries = sidecars.into_entries();
    entries.push((ID.xml_entry(), xml.clone().into_bytes()));
    (xml, entries)
}

#[test]
fn the_higher_layer_definition_wins() {
    let (_, base) = converted_entries();
    let (pack, dir) = layered(
        "layers",
        base.clone(),
        vec![(ID.rdt_entry(), vec![0, 0, 0, 0])],
    );
    assert_eq!(format(&pack, ID), Some(Format::Rdt));
    assert!(load_state(&pack, ID).unwrap().cuts.is_empty());
    assert_eq!(ids(&pack), [ID]);
    assert_eq!(format(&pack_of(base), ID), Some(Format::Xml));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_xml_only_mod_keeps_the_scripts_override() {
    use crate::engine::{ScriptSource, script_source};
    let (xml, mut base) = converted_entries();
    let container = base
        .iter()
        .find(|(path, _)| *path == scripts_entry(ID))
        .unwrap()
        .1
        .clone();
    base.push((ID.scd_entry(), container.clone()));
    let edited = xml.replacen("<ambient", "<!-- edited --><ambient", 1);

    // An edit of the XML alone still names the base container, so the base
    // layer's override keeps winning.
    let only_xml = vec![(ID.xml_entry(), edited.clone().into_bytes())];
    let (pack, dir) = layered("xml-mod", base.clone(), only_xml);
    assert_eq!(script_source(&pack, ID), ScriptSource::Override);
    let _ = std::fs::remove_dir_all(&dir);

    // A mod that ships the scripts too runs its own.
    let with_scripts = vec![
        (ID.xml_entry(), edited.into_bytes()),
        (scripts_entry(ID), container),
    ];
    let (pack, dir) = layered("scripts-mod", base, with_scripts);
    assert_eq!(script_source(&pack, ID), ScriptSource::Rdt);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn malformed_hand_edits_fail_without_panicking() {
    let (xml, _) = converted_entries();
    // Non-ASCII hex data.
    let message = "<message id=\"0\">A key door<br/><color n=\"1\"/><end action=\"0\"/></message>";
    assert!(xml.contains(message), "{xml}");
    let raw = xml.replacen(message, "<message id=\"0\" raw=\"a\u{e9}b\"/>", 1);
    let error = format!("{:#}", xml::parse(&raw, ID).unwrap_err());
    assert!(error.contains("non-hex digit"), "{error}");
    // A definition filed under another room.
    assert!(xml::parse(&xml, RoomId::parse("1000").unwrap()).is_err());
    // Text the font cannot spell.
    let unspellable = xml.replacen("A key door", "A key \u{263A}", 1);
    let error = format!("{:#}", xml::parse(&unspellable, ID).unwrap_err());
    assert!(error.contains("cannot spell"), "{error}");
}

#[test]
fn sidecar_ids_follow_content_not_conversion_order() {
    // The same room converted after another keeps its file names.
    let (alone, _) = converted_entries();
    let mut sidecars = Sidecars::default();
    convert_rdt(
        &[0x00, 0x00, 0x02, 0x01],
        RoomId::parse("1000").unwrap(),
        Script::Latin,
        &mut sidecars,
    )
    .unwrap();
    let after = convert_rdt(&synthetic_rdt(), ID, Script::Latin, &mut sidecars).unwrap();
    assert_eq!(alone, after);
}

#[test]
fn hand_edits_with_unknown_names_are_refused() {
    let rdt = synthetic_rdt();
    let mut sidecars = Sidecars::default();
    let text = convert_rdt(&rdt, ID, Script::Latin, &mut sidecars).unwrap();
    let typo = text.replacen("cellX", "celX", 1);
    let error = format!("{:#}", xml::parse(&typo, ID).unwrap_err());
    assert!(error.contains("unknown attribute `celX`"), "{error}");
    let element = text.replacen("<walkZones>", "<walkZones><wlak/>", 1);
    assert!(xml::parse(&element, ID).is_err());
}

/// The shipped RDTs under `ARKLAY_RE1_ROOT` (the JPN tree) and, when
/// present beside it, every Steam language tree.
fn shipped_trees() -> Option<Vec<(PathBuf, Script)>> {
    let root = PathBuf::from(std::env::var("ARKLAY_RE1_ROOT").ok()?);
    let mut trees = vec![(root.join("JPN"), Script::Japanese)];
    let steam = std::env::var("ARKLAY_RE1_STEAM_ROOT")
        .ok()
        .map(PathBuf::from);
    if let Some(steam) = steam {
        for (language, tree, script) in [
            ("english", "USA", Script::Latin),
            ("french", "FRA", Script::Latin),
            ("german", "GER", Script::Latin),
            ("japanese", "JPN", Script::Japanese),
        ] {
            trees.push((steam.join(language).join(tree), script));
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
    for (tree, script) in trees {
        let rdts = tree_rdts(&tree);
        assert!(
            rdts.len() > 300,
            "{}: only {} RDTs",
            tree.display(),
            rdts.len()
        );
        let pack = converted(&rdts, script);
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
        // by the prop its HD replacement is keyed on.
        let models = state("1070").object_models;
        assert!(!models.is_empty());
        assert!(models.iter().all(|asset| {
            asset
                .prop
                .as_deref()
                .is_some_and(|stem| pack.contains(&format!("{stem}.tmd")))
        }));
        // Room 10C1: item models.
        assert!(!state("10C1").item_models.is_empty());
        // A stub room is empty.
        let stub = rdts.iter().find(|(_, data)| data.len() < 0x94).unwrap();
        assert!(load_state(&pack, stub.0).unwrap().cuts.is_empty());
        // Some room declares effects and has its sheet art.
        assert!(rdts.iter().any(|(id, _)| {
            let effects = load_state(&pack, *id).unwrap().effects;
            !effects.is_empty() && effects.sprites.iter().all(|sprite| sprite.tim.is_some())
        }));
        // A prop shared by an item slot and an object slot is one file.
        let manifest = std::str::from_utf8(&pack.read(source::PROP_MANIFEST).unwrap())
            .unwrap()
            .to_string();
        assert!(
            manifest
                .split("\n[prop.")
                .any(|prop| prop.contains("objects = [") && prop.contains("items = [")),
            "no prop is shared by an item and an object slot"
        );
    }
}

#[test]
fn verify_fails_a_room_naming_a_missing_sidecar() {
    let (xml, entries) = converted_entries();
    let edited = xml.replacen(&scripts_entry(ID), "room/dead.scd", 1);
    let mut entries: Vec<_> = entries
        .into_iter()
        .filter(|(path, _)| *path != ID.xml_entry())
        .collect();
    entries.push((ID.xml_entry(), edited.into_bytes()));
    let report = crate::verify::verify_pack(&pack_of(entries), false);
    let failure = report
        .failures
        .iter()
        .find(|failure| failure.path == ID.xml_entry())
        .expect("the room fails");
    assert!(
        failure.error.contains("missing room/dead.scd"),
        "{}",
        failure.error
    );
}
