use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::convert::{Game, convert_game, convert_game_as, detect_game};
use crate::pack::Pack;

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!("arklay-re2-{}-{label}", std::process::id()));
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

fn write(path: &Path, data: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, data).unwrap();
}

/// A minimal RE2 RDT: `cuts` cameras and one section offset past the header.
fn rdt(cuts: u8) -> Vec<u8> {
    let mut data = vec![0u8; 0x100];
    data[1] = cuts;
    data[8..12].copy_from_slice(&0x64u32.to_le_bytes());
    data
}

fn riff(body: &[u8]) -> Vec<u8> {
    let mut out = b"RIFF".to_vec();
    out.extend((body.len() as u32).to_le_bytes());
    out.extend(body);
    out
}

/// A one-WAV `.sap` around a WAV whose body is `tag`.
fn sap(tag: &[u8]) -> Vec<u8> {
    let mut out = vec![1, 0, 0, 0, 0, 0, 0, 0];
    out.extend(riff(tag));
    out
}

/// A `roomcut.bin` whose groups for `rooms` (stage group, room, cuts) point
/// at one shared uniform background stream, plus pointers left for the
/// caller to break.
fn roomcut(groups: &[(usize, u8, usize)]) -> (Vec<u8>, usize) {
    let stream = crate::adt::fixture::uniform_background(0x21);
    let table = 8 * 32 * 64;
    let mut bin = vec![0u8; table];
    let at = bin.len();
    bin.extend(&stream);
    for &(group, room, cuts) in groups {
        let base = (room as usize + 32 * group) << 6;
        for cut in 0..cuts {
            bin[base + cut * 4..base + cut * 4 + 4].copy_from_slice(&(at as u32).to_le_bytes());
        }
    }
    (bin, at)
}

/// A small RE2 install with every category, mixing name cases the way the
/// retail tree does.
fn make_install(root: &Path) {
    let data = root.join("data");
    write(&data.join("Pl0/Rdt/ROOM1000.RDT"), &rdt(2));
    write(&data.join("Pl1/Rdt/room1001.rdt"), &rdt(2));
    write(&data.join("Pl0/Rdt/ROOMA010.RDT"), &rdt(1));
    write(&data.join("Pl0/Rdt/ROOM2000.RDT"), &rdt(1));
    write(&data.join("Pl0/Rdt/ROOM7050.RDT"), &[0, 0, 0, 0]);
    let (mut bin, _) = roomcut(&[(0, 0, 2), (0, 1, 1)]);
    // Room 200's only cut points past the end of the file.
    let base = 32 << 6;
    bin[base..base + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    write(&data.join("Common/bin/roomcut.bin"), &bin);

    write(&data.join("Pl0/emd0/em010.emd"), b"emd-010");
    write(&data.join("Pl0/emd0/EM010.TIM"), b"tim-010");
    write(&data.join("Pl1/emd1/EM110.EMD"), b"emd-110");
    write(&data.join("Pl1/emd1/EM13A.TIM"), b"tim-13a");
    write(&data.join("Pl0/emd0/EM13A.TIM"), b"stray-13a");

    write(&data.join("Pl0/PLD/PL00.PLD"), b"leon-0");
    write(&data.join("Pl1/PLD/PL00.PLD"), b"leon-1");
    write(&data.join("Pl0/PLD/PL00CH.PLD"), b"leon-ch");
    write(&data.join("Pl0/PLD/PL0BW0A.PLW"), b"weapon");
    for (disc, take) in [(0, 0), (1, 0), (0, 1), (1, 1), (0, 2), (1, 2)] {
        let name = format!("Pl{disc}/PLD/PDEMO{disc}{take}.DAT");
        write(&data.join(name), format!("demo-{disc}{take}").as_bytes());
    }
    write(&data.join("Pl0/PLD/Pdemo00p.dat"), b"alternate");

    write(&data.join("Pl0/VOICE/stage1/v010.sap"), &sap(b"WAVEv0"));
    write(&data.join("Pl1/Voice/stage0/v000.sap"), &sap(b"WAVEv1"));
    write(&data.join("Common/Sound/BGM/main00.sap"), &sap(b"WAVEbgm"));
    write(&data.join("Common/Sound/BGM/MAIN00.BGM"), b"sequence");
    write(&data.join("Common/DOOR/DOOR1F.DO2"), b"door");
    write(&data.join("Common/DOOR/vielogo.tga"), b"logo");
    write(&data.join("Zmovie/r109.bin"), &[0, 0, 1, 0xBA, 1]);
    write(&data.join("Pl0/Zmovie/opn1stl.bin"), &[0, 0, 1, 0xBA, 2]);
    write(&data.join("Pl1/Zmovie/titlec.bin"), b"not a film");
}

fn read(pack: &Pack, path: &str) -> Vec<u8> {
    pack.read(path)
        .unwrap_or_else(|error| panic!("{path}: {error:#}"))
        .to_vec()
}

#[test]
fn room_ids_parse_and_map_to_roomcut_groups() {
    let room = Re2Room::parse("10a1").unwrap();
    assert_eq!((room.stage, room.room, room.disc), ('1', 0x0A, 1));
    assert_eq!(room.folder(), "10a1");
    assert_eq!(room.pointer_offset(), 0x0A << 6);
    let letter = Re2Room::parse("G010").unwrap();
    assert_eq!(letter.folder(), "g010");
    assert_eq!(letter.stage_group(), 6);
    assert_eq!(letter.pointer_offset(), (1 + 32 * 6) << 6);
    assert_eq!(
        Re2Room::parse("A000").unwrap().pointer_offset(),
        Re2Room::parse("1000").unwrap().pointer_offset()
    );
    for bad in ["8000", "H000", "10X0", "100", "1000a", "100A"] {
        assert!(Re2Room::parse(bad).is_none(), "{bad}");
    }
}

#[test]
fn player_and_enemy_names_map_to_entries() {
    assert_eq!(
        player_entry("PL00", "PLD", 0).unwrap(),
        "player/00/model_0.pld"
    );
    assert_eq!(
        player_entry("PL0E", "PLD", 1).unwrap(),
        "player/0e/model_1.pld"
    );
    assert_eq!(
        player_entry("PL01CH", "PLD", 1).unwrap(),
        "player/01/model_ch_1.pld"
    );
    assert_eq!(
        player_entry("PL0BW13", "PLW", 0).unwrap(),
        "player/0b/weapon/13_0.plw"
    );
    assert_eq!(player_entry("PDEMO00", "DAT", 0), None);
    assert_eq!(player_entry("PL00W1", "PLW", 0), None);
    assert_eq!(
        enemy_entry("EM02A", "EMD").unwrap(),
        ("enemy/2a/enemy_0.emd".to_string(), 0)
    );
    assert_eq!(
        enemy_entry("EM13A", "TIM").unwrap(),
        ("enemy/3a/enemy_1.tim".to_string(), 1)
    );
    assert_eq!(enemy_entry("EM210", "EMD"), None);
    assert_eq!(enemy_entry("EM010", "PLD"), None);
}

#[test]
fn game_detection_finds_the_data_directory_at_any_level() {
    let root = TempDir::new("detect");
    assert_eq!(detect_game(&root.path).unwrap(), Game::Re1);
    let install = root.path.join("a/b");
    write(&install.join("DATA/pl0/rdt/ROOM1000.RDT"), &rdt(1));
    assert_eq!(detect_game(&root.path).unwrap(), Game::Re2);
    assert_eq!(detect_game(&install).unwrap(), Game::Re2);
    assert_eq!(detect_game(&install.join("DATA")).unwrap(), Game::Re2);
    assert_eq!(
        find_data_dir(&root.path).unwrap(),
        install.join("DATA"),
        "breadth-first, case-insensitive"
    );
    // Deeper than the search reaches.
    let deep = TempDir::new("detect-deep");
    write(&deep.path.join("a/b/c/d/Pl0/Rdt/ROOM1000.RDT"), &rdt(1));
    assert_eq!(detect_game(&deep.path).unwrap(), Game::Re1);
}

#[test]
fn converts_every_category() {
    let root = TempDir::new("convert");
    make_install(&root.path);
    let out = root.path.join("out.akpak");
    convert_game(&root.path, &out).unwrap();
    let pack = Pack::open(&out).unwrap();

    // Rooms, both discs and the letter stage; the 4-byte stub is skipped.
    for id in ["1000", "1001", "a010", "2000"] {
        assert_eq!(read(&pack, &format!("room/{id}/room.rdt")), rdt_for(id));
    }
    assert!(!pack.contains("room/7050/room.rdt"));

    // One decoded background shared by every cut that points at it; room
    // 200's pointer is past the end, so it has none.
    let cut = read(&pack, "room/1000/cut/00.bmp");
    let image = crate::bmp::decode(&cut).unwrap();
    assert_eq!((image.width, image.height), (320, 240));
    for entry in [
        "room/1000/cut/01.bmp",
        "room/1001/cut/00.bmp",
        "room/a010/cut/00.bmp",
    ] {
        assert_eq!(read(&pack, entry), cut, "{entry}");
    }
    assert!(!pack.contains("room/1000/cut/02.bmp"));
    assert!(!pack.contains("room/2000/cut/00.bmp"));

    // Enemies by the name's disc digit; the stray in the other disc's
    // folder yields to the file in its own.
    assert_eq!(read(&pack, "enemy/10/enemy_0.emd"), b"emd-010");
    assert_eq!(read(&pack, "enemy/10/enemy_0.tim"), b"tim-010");
    assert_eq!(read(&pack, "enemy/10/enemy_1.emd"), b"emd-110");
    assert_eq!(read(&pack, "enemy/3a/enemy_1.tim"), b"tim-13a");

    assert_eq!(read(&pack, "player/00/model_0.pld"), b"leon-0");
    assert_eq!(read(&pack, "player/00/model_1.pld"), b"leon-1");
    assert_eq!(read(&pack, "player/00/model_ch_0.pld"), b"leon-ch");
    assert_eq!(read(&pack, "player/0b/weapon/0a_0.plw"), b"weapon");

    // The `.sap` header is stripped, leaving the WAV.
    assert_eq!(read(&pack, "voice/0/01_0a.wav"), riff(b"WAVEv0"));
    assert_eq!(read(&pack, "voice/1/00_00.wav"), riff(b"WAVEv1"));
    assert_eq!(read(&pack, "bgm/main00.wav"), riff(b"WAVEbgm"));
    assert!(!pack.paths().any(|path| path.ends_with(".bgm")));

    assert_eq!(read(&pack, "door/1f.do2"), b"door");
    assert!(!pack.paths().any(|path| path.contains("vielogo")));

    assert_eq!(read(&pack, "movie/r109.mpg"), [0, 0, 1, 0xBA, 1]);
    assert_eq!(read(&pack, "movie/opn1stl.mpg"), [0, 0, 1, 0xBA, 2]);
    assert!(!pack.contains("movie/titlec.mpg"), "not an MPEG stream");

    // Demo reels interleave the characters; the alternate takes are not
    // packed.
    for (index, name) in ["00", "10", "01", "11", "02", "12"].iter().enumerate() {
        assert_eq!(
            read(&pack, &format!("demo/{index:02x}.dat")),
            format!("demo-{name}").as_bytes()
        );
    }
    assert!(!pack.paths().any(|path| read(&pack, path) == b"alternate"));

    let manifest = pack.manifest().unwrap();
    assert_eq!(manifest.id, "re2");
    assert_eq!(manifest.kind, manifest::PackKind::Base);
    assert_eq!(manifest.language, None);
    let text = String::from_utf8(read(&pack, manifest::ENTRY)).unwrap();
    assert!(!text.contains("rdt =") && !text.contains("scd ="), "{text}");

    // No RE1 content rides along.
    assert!(
        !pack
            .paths()
            .any(|path| path.ends_with(".lua") || path.starts_with("language/"))
    );
}

/// The RDT bytes [`make_install`] wrote for `id`.
fn rdt_for(id: &str) -> Vec<u8> {
    rdt(if matches!(id, "1000" | "1001") { 2 } else { 1 })
}

#[test]
fn a_missing_tree_fails_clearly_when_forced() {
    let root = TempDir::new("missing");
    let error = convert_game_as(
        &root.path,
        &root.path.join("out.akpak"),
        Some(Game::Re2),
        1,
        false,
    )
    .unwrap_err();
    assert!(
        format!("{error:#}").contains("no RE2 data directory"),
        "{error:#}"
    );
}

#[test]
fn a_rooms_only_install_still_converts() {
    let root = TempDir::new("sparse");
    write(&root.path.join("Pl0/Rdt/ROOM1000.RDT"), &rdt(1));
    let mut warnings = Vec::new();
    let layout = Layout::discover(&root.path).unwrap();
    let rooms = scan_rooms(&layout, &mut warnings).unwrap();
    assert_eq!(rooms.len(), 1);
    let out = root.path.join("out.akpak");
    convert_game(&root.path, &out).unwrap();
    let pack = Pack::open(&out).unwrap();
    assert!(pack.contains("room/1000/room.rdt"));
    assert!(pack.contains(manifest::ENTRY));
}

#[test]
fn over_long_cut_counts_stop_at_the_pointer_group() {
    let root = TempDir::new("cuts");
    let data = root.path.join("data");
    write(&data.join("Pl0/Rdt/ROOM30C0.RDT"), &rdt(17));
    let (bin, _) = roomcut(&[(2, 0x0C, 16)]);
    write(&data.join("Common/bin/roomcut.bin"), &bin);
    let out = root.path.join("out.akpak");
    convert_game(&root.path, &out).unwrap();
    let pack = Pack::open(&out).unwrap();
    assert!(pack.contains("room/30c0/cut/0f.bmp"));
    assert!(!pack.contains("room/30c0/cut/10.bmp"));
}

#[test]
fn file_names_must_be_ascii_hex_where_the_game_reads_hex() {
    // Multi-byte characters must not split a slice, and `+` is not a digit.
    assert!(Re2Room::parse("1€").is_none());
    assert!(Re2Room::parse("1+F0").is_none());
    assert_eq!(enemy_entry("EMé1", "EMD"), None);
    assert_eq!(enemy_entry("EM0+1", "EMD"), None);
    assert_eq!(enemy_entry("EM2+1", "EMD"), None);
    assert_eq!(player_entry("PLé", "PLD", 0), None);
    assert_eq!(player_entry("PL+1", "PLD", 0), None);
    assert_eq!(player_entry("PL00W+1", "PLW", 0), None);
    // Rooms past the 32 pointer slots a stage has.
    assert!(Re2Room::parse("11D0").unwrap().has_pointer_slot());
    assert!(!Re2Room::parse("1200").unwrap().has_pointer_slot());
}

/// The seven `STAGEn` folders RE1 discovery requires.
fn make_re1_install(root: &Path) {
    for stage in 1..=7 {
        fs::create_dir_all(root.join(format!("STAGE{stage}"))).unwrap();
    }
}

#[test]
fn a_folder_holding_both_games_needs_an_explicit_game() {
    let root = TempDir::new("both");
    make_re1_install(&root.path.join("re1"));
    write(&root.path.join("re2/data/Pl0/Rdt/ROOM1000.RDT"), &rdt(1));
    let error = detect_game(&root.path).unwrap_err();
    assert!(format!("{error:#}").contains("--game"), "{error:#}");
    let out = root.path.join("out.akpak");
    let error = convert_game(&root.path, &out).unwrap_err();
    assert!(format!("{error:#}").contains("--game"), "{error:#}");
    convert_game_as(&root.path, &out, Some(Game::Re2), 1, false).unwrap();
    assert!(Pack::open(&out).unwrap().contains("room/1000/room.rdt"));
}

#[test]
fn the_launcher_checks_an_install_for_the_game_it_builds() {
    let root = TempDir::new("launcher");
    write(&root.path.join("data/Pl0/Rdt/ROOM1000.RDT"), &rdt(1));
    let error = crate::convert::validate_install(&root.path).unwrap_err();
    assert!(
        format!("{error:#}").contains("Resident Evil 2"),
        "{error:#}"
    );
    crate::convert::validate_install_as(&root.path, Game::Re2).unwrap();
    // The launcher's conversion entry point builds the game it is given.
    let out = root.path.join("re1.akpak");
    assert!(
        crate::convert::convert_game_observed(&root.path, &out, Game::Re1, 1, false, None, None)
            .is_err()
    );
    assert!(!out.exists());
    let out = root.path.join("re2.akpak");
    crate::convert::convert_game_observed(&root.path, &out, Game::Re2, 1, false, None, None)
        .unwrap();
    assert!(Pack::open(&out).unwrap().contains("room/1000/room.rdt"));
    // A folder with both keeps working for either game.
    make_re1_install(&root.path);
    crate::convert::validate_install(&root.path).unwrap();
    crate::convert::validate_install_as(&root.path, Game::Re2).unwrap();
}

#[test]
fn an_re1_install_is_not_an_re2_install() {
    let root = TempDir::new("launcher-re1");
    make_re1_install(&root.path);
    let error = crate::convert::validate_install_as(&root.path, Game::Re2).unwrap_err();
    assert!(
        format!("{error:#}").contains("not Resident Evil 2"),
        "{error:#}"
    );
}

#[cfg(unix)]
#[test]
fn an_unreadable_folder_does_not_end_the_search() {
    use std::os::unix::fs::PermissionsExt;
    let root = TempDir::new("unreadable");
    let locked = root.path.join("aaa");
    fs::create_dir_all(&locked).unwrap();
    write(&root.path.join("re2/data/Pl0/Rdt/ROOM1000.RDT"), &rdt(1));
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let found = find_data_dir(&root.path);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(found.unwrap(), root.path.join("re2/data"));
}

#[test]
fn a_corrupt_stream_costs_only_its_own_cameras() {
    let root = TempDir::new("corrupt");
    let data = root.path.join("data");
    write(&data.join("Pl0/Rdt/ROOM1000.RDT"), &rdt(2));
    let (mut bin, _) = roomcut(&[(0, 0, 1)]);
    // Cut 1 points inside the table, where the bytes are no ADT stream.
    bin[4..8].copy_from_slice(&64u32.to_le_bytes());
    write(&data.join("Common/bin/roomcut.bin"), &bin);
    let out = root.path.join("out.akpak");
    convert_game(&root.path, &out).unwrap();
    let pack = Pack::open(&out).unwrap();
    assert!(pack.contains("room/1000/cut/00.bmp"));
    assert!(!pack.contains("room/1000/cut/01.bmp"));
}
