use super::*;
use std::collections::HashMap;

/// Files held in memory.
struct Fake(HashMap<String, Arc<[u8]>>);

impl Source for Fake {
    fn read(&self, entry: &str) -> Option<Arc<[u8]>> {
        self.0.get(entry).cloned()
    }
}

/// An RDT of `len` bytes (a counting pattern) whose section 2 is at `vb`, 0 for none, and whose
/// other header bytes are 0.
fn rdt(len: usize, vb: u32) -> Vec<u8> {
    let mut bytes: Vec<u8> = (0..len).map(|at| (at * 7) as u8).collect();
    bytes[..RDT_HEADER.min(len)].fill(0);
    bytes[16..20].copy_from_slice(&vb.to_le_bytes());
    bytes
}

/// An EMD of `len` bytes whose TMD is at `tmd`, `tmd_size` long, with `parts` parts, and whose first
/// sub-animation is at `sub0`; the rest is a pattern seeded by `seed`.
fn emd(len: usize, sub0: u32, tmd: u32, tmd_size: u32, parts: u32, seed: u8) -> Vec<u8> {
    let mut bytes: Vec<u8> = (0..len)
        .map(|at| (at as u8).wrapping_mul(3) ^ seed)
        .collect();
    let footer = len - 32;
    bytes[0..4].copy_from_slice(&(footer as u32).to_le_bytes());
    let sections = [8, 0xC, 0x20, sub0, 0x40, 0x50, 0x60, tmd];
    for (k, section) in sections.iter().enumerate() {
        bytes[footer + k * 4..footer + k * 4 + 4].copy_from_slice(&section.to_le_bytes());
    }
    let tmd = tmd as usize;
    bytes[tmd..tmd + 4].copy_from_slice(&tmd_size.to_le_bytes());
    bytes[tmd + 8..tmd + 12].copy_from_slice(&(parts * 2).to_le_bytes());
    bytes
}

fn files(entries: Vec<(String, Vec<u8>)>) -> Files {
    Files(Some(Arc::new(Fake(
        entries
            .into_iter()
            .map(|(entry, bytes)| (entry, Arc::from(bytes)))
            .collect(),
    ))))
}

fn word_at(bytes: &[u8], at: usize) -> [i16; 2] {
    [
        i16::from_le_bytes([bytes[at], bytes[at + 1]]),
        i16::from_le_bytes([bytes[at + 2], bytes[at + 3]]),
    ]
}

/// The music word of a room with no banks.
const NO_MUSIC: u16 = 0xFFFF;
/// A zombie's id.
const ZOMBIE: u8 = 0x1F;

/// The zombie's model: 0x4000 bytes, the TMD at 0x3000 (0x800 long), two parts. A work and its parts
/// take `WORK_SIZE + 2 * PART_SIZE` after the first; the first takes the model's 0x3800 too.
fn zombie_model() -> Vec<u8> {
    emd(0x4000, 0x1000, 0x3000, 0x800, 2, 0x5A)
}

/// The first work's model ends at `vb + WORK_SIZE + 0x3800`, then come its parts: the next work's
/// address.
fn second_work(vb: usize) -> usize {
    vb + WORK_SIZE + 0x3800 + 2 * PART_SIZE
}

#[test]
fn a_work_starts_with_the_rdt_bytes_under_it() {
    let room = rdt(0x2000, 0x100);
    let files = files(vec![
        (rdt_entry("1000"), room.clone()),
        (emd_entry(ZOMBIE, 0), zombie_model()),
    ]);
    let mut heap = Heap::default();
    heap.enter_room(&files, "1000", NO_MUSIC, false);
    let held = heap.em_set(&files, 0, ZOMBIE, 0, 0, false, true);
    assert_eq!(held, Some(word_at(&room, 0x100 + DEST)));
    // The next work of the same id shares the model: it sits on the model's bytes.
    let model = zombie_model();
    let held = heap.em_set(&files, 1, ZOMBIE, 0, 0, false, true);
    let at = second_work(0x100) - (0x100 + WORK_SIZE);
    assert_eq!(held, Some(word_at(&model, at + DEST)));
}

#[test]
fn a_slots_dest_survives_a_load_into_a_room_whose_zombie_does_not_set_it() {
    let vb = 0x100;
    let second = second_work(vb);
    // Room 1000 has two zombies; room 1010's RDT ends where 1000's second work was and has no
    // section 2, so its first work goes there.
    let files = files(vec![
        (rdt_entry("1000"), rdt(0x2000, vb as u32)),
        (rdt_entry("1010"), rdt(second, 0)),
        (emd_entry(ZOMBIE, 0), zombie_model()),
        (emd_entry(0x13, 0), emd(0x400, 0x100, 0x200, 0x100, 1, 0x11)),
    ]);
    let mut heap = Heap::default();
    heap.enter_room(&files, "1000", NO_MUSIC, false);
    heap.em_set(&files, 0, ZOMBIE, 0, 0, false, true);
    heap.em_set(&files, 1, ZOMBIE, 0, 0, false, true);
    heap.after_load();
    heap.leave_room(|slot| Some([slot as i16 * 100 + 7, -300]));
    heap.enter_room(&files, "1010", NO_MUSIC, false);
    let held = heap.em_set(&files, 3, 0x13, 0, 0, false, true);
    assert_eq!(held, Some([107, -300]), "slot 1's last `Dest`");
}

#[test]
fn a_new_occupant_overwrites_what_its_load_writes() {
    let vb = 0x100;
    let second = second_work(vb);
    // Room 1010's first work goes 0x100 before 1000's second, so its model covers that work.
    let first = second - 0x100;
    let other = emd(0x400, 0x100, 0x200, 0x100, 1, 0x11);
    let files = files(vec![
        (rdt_entry("1000"), rdt(0x2000, vb as u32)),
        (rdt_entry("1010"), rdt(first, 0)),
        (emd_entry(ZOMBIE, 0), zombie_model()),
        (emd_entry(0x13, 0), other.clone()),
    ]);
    let mut heap = Heap::default();
    heap.enter_room(&files, "1000", NO_MUSIC, false);
    heap.em_set(&files, 0, ZOMBIE, 0, 0, false, true);
    heap.em_set(&files, 1, ZOMBIE, 0, 0, false, true);
    heap.leave_room(|_| Some([1, 2]));
    heap.enter_room(&files, "1010", NO_MUSIC, false);
    heap.em_set(&files, 0, 0x13, 0, 0, false, true);
    // 1000's second work's `Dest` now holds the model's bytes.
    let image = &heap.image;
    let at = second + DEST;
    let model_at = at - (first + WORK_SIZE);
    assert_eq!(word_at(&image.bytes, at), word_at(&other, model_at));
    // The new work's own bytes are left unknown.
    assert!(
        !image.known[first..first + WORK_SIZE]
            .iter()
            .any(|&known| known)
    );
}

#[test]
fn a_dest_never_given_stays_unknown() {
    let vb = 0x100;
    let second = second_work(vb);
    let files = files(vec![
        (rdt_entry("1000"), rdt(0x2000, vb as u32)),
        (rdt_entry("1010"), rdt(second, 0)),
        (emd_entry(ZOMBIE, 0), zombie_model()),
    ]);
    let mut heap = Heap::default();
    // Room 1000's second work sits past what this boot wrote: its `Dest` is unknown.
    heap.enter_room(&files, "1000", NO_MUSIC, false);
    heap.em_set(&files, 0, ZOMBIE, 0, 0, false, true);
    Arc::make_mut(&mut heap.image).forget(second, WORK_SIZE);
    assert_eq!(heap.em_set(&files, 1, ZOMBIE, 0, 0, false, true), None);
    // `Root_ck` never gave it one: still 0 when the room ends.
    heap.leave_room(|_| Some([0, 0]));
    heap.enter_room(&files, "1010", NO_MUSIC, false);
    assert_eq!(heap.em_set(&files, 0, ZOMBIE, 0, 0, false, true), None);
}

#[test]
fn the_first_model_of_a_room_keeps_its_tmd_where_the_file_has_it() {
    // `Em_kind_search` gives 0x80 for anything but id 0x10, so a room's first model (the last kind
    // is 0xFF) has its type's 0x80 cleared, and the next model of another id keeps it.
    let model = zombie_model();
    let files = files(vec![
        (rdt_entry("1000"), rdt(0x2000, 0x100)),
        (emd_entry(ZOMBIE, 0), model.clone()),
        (emd_entry(0x13, 0), model.clone()),
    ]);
    let mut heap = Heap::default();
    heap.enter_room(&files, "1000", NO_MUSIC, false);
    heap.em_set(&files, 0, ZOMBIE, 0x80, 0, false, true);
    let moved = second_work(0x100);
    assert_eq!(heap.top, Some(moved), "not moved");
    heap.em_set(&files, 1, 0x13, 0x80, 0, false, true);
    // Moved: the TMD goes to 0x1000, so the model ends at 0x1800.
    assert_eq!(heap.top, Some(moved + WORK_SIZE + 0x1800 + 2 * PART_SIZE));
    let buffer = moved + WORK_SIZE;
    assert_eq!(
        &heap.image.bytes[buffer + 0x1000..buffer + 0x1004],
        &model[0x3000..0x3004]
    );
}

#[test]
fn a_parts_colour_is_known() {
    let files = files(vec![
        (rdt_entry("1000"), rdt(0x2000, 0x100)),
        (emd_entry(ZOMBIE, 0), zombie_model()),
    ]);
    let mut heap = Heap::default();
    heap.enter_room(&files, "1000", NO_MUSIC, false);
    heap.em_set(&files, 0, ZOMBIE, 0, 0, false, true);
    let parts = 0x100 + WORK_SIZE + 0x3800;
    assert_eq!(heap.image.word(parts + PART_POLY_RGB), Some(0x80_8080));
    assert_eq!(
        heap.image.word(parts + PART_SIZE + PART_POLY_RGB),
        Some(0x80_8080)
    );
    assert_eq!(heap.image.word(parts), None, "the parts' live fields");
}

#[test]
fn an_init_that_allocates_ends_what_is_known_after_the_load() {
    let files = files(vec![
        (rdt_entry("1000"), rdt(0x2000, 0x100)),
        (emd_entry(0x2A, 0), zombie_model()),
        (emd_entry(ZOMBIE, 0), zombie_model()),
    ]);
    let mut heap = Heap::default();
    heap.enter_room(&files, "1000", NO_MUSIC, false);
    heap.em_set(&files, 0, 0x2A, 0, 0, false, true);
    // Mr. X's joints come at his first move, after the load's sets.
    assert!(heap.em_set(&files, 1, ZOMBIE, 0, 0, false, true).is_some());
    heap.after_load();
    assert_eq!(heap.em_set(&files, 2, ZOMBIE, 0, 0, false, false), None);
}

#[test]
fn an_init_run_again_starts_from_the_loads_image() {
    let vb = 0x100;
    let second = second_work(vb);
    let files = files(vec![
        (rdt_entry("1000"), rdt(0x2000, vb as u32)),
        (rdt_entry("1010"), rdt(second, 0)),
        (emd_entry(ZOMBIE, 0), zombie_model()),
        (emd_entry(0x13, 0), emd(0x400, 0x100, 0x200, 0x100, 1, 0x11)),
    ]);
    let mut heap = Heap::default();
    heap.enter_room(&files, "1000", NO_MUSIC, false);
    heap.em_set(&files, 0, ZOMBIE, 0, 0, false, true);
    heap.em_set(&files, 1, ZOMBIE, 0, 0, false, true);
    heap.leave_room(|_| Some([5, 6]));
    heap.enter_room(&files, "1010", NO_MUSIC, false);
    let first = heap.em_set(&files, 0, 0x13, 0, 0, false, true);
    heap.after_load();
    heap.enter_room(&files, "1010", NO_MUSIC, true);
    assert_eq!(heap.em_set(&files, 0, 0x13, 0, 0, false, true), first);
    assert_eq!(first, Some([5, 6]));
}

#[test]
fn the_music_banks_load_when_their_ids_change() {
    let main: Vec<u8> = vec![0xAB; 0x3000];
    let sub: Vec<u8> = vec![0xCD; 0x2800];
    let files = files(vec![
        (rdt_entry("1000"), rdt(0x2000, 0)),
        (bgm_entry(false, 3), main),
        (bgm_entry(true, 5), sub),
    ]);
    let mut heap = Heap::default();
    // The first load can't tell whether the banks load: past the RDT nothing is known.
    heap.enter_room(&files, "1000", 0x0503, false);
    assert_eq!(heap.image.word(0x2000), None);
    heap.enter_room(&files, "1000", 0xFF04, false);
    heap.enter_room(&files, "1000", 0x0503, false);
    // The main loaded at 0x10 and the sub over it.
    assert_eq!(heap.image.word(0x2000), Some(0xCDCD_CDCD));
    assert_eq!(heap.image.word(0x2800), Some(0xCDCD_CDCD));
    assert_eq!(heap.image.word(0x2810), Some(0xABAB_ABAB));
    assert_eq!(heap.image.word(0x3010), None);
}

/// The original's numbers, stated as the decompilation has them rather than through the constants:
/// the work's 0x248 bytes (`Mem_top + 0x248` is the model's buffer), the part's 0xAC, `Dest` at
/// 0x1C4, the part's `Poly_rgb` at 0x70, the banks at 0x10 and the 100-byte header.
#[test]
fn the_originals_numbers() {
    let mut room = rdt(0x2000, 0x100);
    room[0x2C4..0x2C8].copy_from_slice(&[0x34, 0x12, 0x78, 0x56]);
    let model = zombie_model();
    let files = files(vec![
        (rdt_entry("1000"), room),
        (emd_entry(ZOMBIE, 0), model.clone()),
        (bgm_entry(false, 1), vec![0xEE; 0x40]),
        (rdt_entry("1010"), rdt(0x20, 0)),
    ]);
    let mut heap = Heap::default();
    // The first work at section 2 (0x100): its `Dest` at 0x2C4.
    heap.enter_room(&files, "1000", NO_MUSIC, false);
    assert_eq!(
        heap.em_set(&files, 0, ZOMBIE, 0, 0, false, true),
        Some([0x1234, 0x5678])
    );
    // The model at 0x348; its TMD (0x3000, 0x800 long) ends at 0x3B48, then two parts of 0xAC, the
    // first's `Poly_rgb` at 0x3BB8; the next work at 0x3CA0 reads the model's byte 0x3B1C.
    assert_eq!(heap.image.word(0x3BB8), Some(0x80_8080));
    assert_eq!(heap.image.word(0x3BB8 + 0xAC), Some(0x80_8080));
    assert_eq!(heap.top, Some(0x3CA0));
    assert_eq!(
        heap.em_set(&files, 1, ZOMBIE, 0, 0, false, true),
        Some(word_at(&model, 0x3B1C))
    );
    // The header's section 2 offset is relocated to `Work_buffer`'s 0x8FF8A0.
    assert_eq!(heap.image.word(16), Some(0x8F_F8A0 + 0x100));
    // A main bank loads at 0x10; a 0x20-byte RDT goes over its start.
    heap.enter_room(&files, "1010", 0xFF00, false);
    heap.enter_room(&files, "1010", 0xFF01, false);
    assert_eq!(heap.image.word(0x20), Some(0xEEEE_EEEE));
    assert_eq!(heap.image.word(0x4C), Some(0xEEEE_EEEE));
    assert_eq!(heap.image.word(0x50), None);
}

#[test]
fn model_type_0x40_moves_what_follows_the_sub_animations_down() {
    let model = zombie_model();
    let files = files(vec![
        (rdt_entry("1000"), rdt(0x2000, 0x100)),
        (emd_entry(ZOMBIE, 0), model.clone()),
    ]);
    let mut heap = Heap::default();
    heap.enter_room(&files, "1000", NO_MUSIC, false);
    heap.em_set(&files, 0, ZOMBIE, 0x40, 0, false, true);
    // From the first sub-animation (0x1000) on, down to the main ones (0xC): the TMD is at 0x200C.
    let buffer = 0x100 + WORK_SIZE;
    assert_eq!(
        &heap.image.bytes[buffer + 0x200C..buffer + 0x2010],
        &model[0x3000..0x3004]
    );
    assert_eq!(heap.top, Some(buffer + 0x200C + 0x800 + 2 * PART_SIZE));
}

#[test]
fn the_tmds_pointers_are_mapped() {
    let model = zombie_model();
    let files = files(vec![
        (rdt_entry("1000"), rdt(0x2000, 0x100)),
        (emd_entry(ZOMBIE, 0), model.clone()),
    ]);
    let mut heap = Heap::default();
    heap.enter_room(&files, "1000", NO_MUSIC, false);
    heap.em_set(&files, 0, ZOMBIE, 0, 0, false, true);
    let tmd = 0x100 + WORK_SIZE + 0x3000;
    assert_eq!(heap.image.word(tmd + 4), Some(1));
    let table = tmd + 0xC;
    let by = 0x8F_F8A0 + table as u32;
    let file = |at: usize| u32::from_le_bytes(model[at..at + 4].try_into().unwrap());
    // Four objects (two parts); dwords 0, 2, 4 and 6 of each, and not 1.
    for object in 0..4 {
        let at = table + object * 0x1C;
        let in_file = 0x3000 + 0xC + object * 0x1C;
        assert_eq!(heap.image.word(at), Some(file(in_file).wrapping_add(by)));
        assert_eq!(
            heap.image.word(at + 24),
            Some(file(in_file + 24).wrapping_add(by))
        );
        assert_eq!(heap.image.word(at + 4), Some(file(in_file + 4)));
    }
}

#[test]
fn the_rdts_relocated_entries_are_addresses() {
    let mut room = rdt(0x2000, 0x100);
    // Two camera entries (section 7 at 0x400) and one model pair (section 10 at 0x600).
    room[1] = 2;
    room[2] = 1;
    room[8 + 7 * 4..8 + 7 * 4 + 4].copy_from_slice(&0x400u32.to_le_bytes());
    room[8 + 10 * 4..8 + 10 * 4 + 4].copy_from_slice(&0x600u32.to_le_bytes());
    let word = |at: usize| u32::from_le_bytes(room[at..at + 4].try_into().unwrap());
    let expected = [0x41C, 0x43C, 0x600, 0x604].map(|at| (at, word(at).wrapping_add(0x8F_F8A0)));
    let files = files(vec![(rdt_entry("1000"), room.clone())]);
    let mut heap = Heap::default();
    heap.enter_room(&files, "1000", NO_MUSIC, false);
    for (at, value) in expected {
        assert_eq!(heap.image.word(at), Some(value), "{at:#x}");
    }
    assert_eq!(heap.image.word(0x420), Some(word(0x420)), "not relocated");
}

#[test]
fn the_zapping_models_a_mirror_and_an_em_set2_end_what_is_known() {
    let files = files(vec![
        (rdt_entry("1000"), rdt(0x2000, 0x100)),
        (emd_entry(ZOMBIE, 0), zombie_model()),
        (emd_entry(0x55, 0), zombie_model()),
    ]);
    let after = |first: &dyn Fn(&mut Heap)| {
        let mut heap = Heap::default();
        heap.enter_room(&files, "1000", NO_MUSIC, false);
        first(&mut heap);
        heap.em_set(&files, 1, ZOMBIE, 0, 0, false, true)
    };
    // `Emd_load` picks 0x50 to 0x5A's files by the zapping flags.
    for id in [0x50, 0x55, 0x5A] {
        assert_eq!(
            after(&|heap: &mut Heap| {
                assert!(heap.em_set(&files, 0, id, 0, 0, false, true).is_some());
            }),
            None,
            "{id:#x}"
        );
    }
    // `Mirror_model_cp`.
    assert_eq!(
        after(&|heap: &mut Heap| {
            heap.em_set(&files, 0, ZOMBIE, 0, 0, true, true);
        }),
        None
    );
    assert_eq!(after(&|heap: &mut Heap| heap.em_set2()), None);
    assert!(after(&|_: &mut Heap| {}).is_some());
}

#[test]
fn rbj_reset_forgets_what_it_moves() {
    let mut room = rdt(0x2000, 0x1000);
    room[8 + 22 * 4..8 + 22 * 4 + 4].copy_from_slice(&0x800u32.to_le_bytes());
    let files = files(vec![(rdt_entry("1000"), room)]);
    let mut heap = Heap::default();
    heap.enter_room(&files, "1000", NO_MUSIC, false);
    assert!(heap.image.word(0x900).is_some());
    heap.rbj_reset();
    assert_eq!(heap.image.word(0x800), None);
    assert_eq!(heap.image.word(0xFFC), None);
    assert!(heap.image.word(0x7FC).is_some());
    assert!(heap.image.word(0x1000).is_some());
    assert_eq!(heap.em_set(&files, 0, ZOMBIE, 0, 0, false, true), None);
}

#[test]
fn the_computers_work_is_unknown() {
    let files = files(vec![(rdt_entry("2000"), rdt(0x8000, 0x100))]);
    let mut heap = Heap::default();
    heap.enter_room(&files, "2000", NO_MUSIC, false);
    heap.after_load();
    heap.scratch(COMPUTER_SIZE);
    assert_eq!(heap.image.word(0x100), None);
    assert_eq!(heap.image.word(0x100 + 0x53DC), None);
    assert!(heap.image.word(0x100 + 0x53E0).is_some());
    // `Computer_exit` gives the memory back: the next work goes where the computer's was.
    assert_eq!(heap.top, Some(0x100));
}

#[test]
fn nothing_past_the_cap_is_kept() {
    let files = files(vec![
        (rdt_entry("1000"), rdt(0x100, 0x7FFF_FFF0)),
        (rdt_entry("1010"), rdt(0x2000, (CAP - 0x10) as u32)),
        (emd_entry(ZOMBIE, 0), zombie_model()),
    ]);
    let mut heap = Heap::default();
    heap.enter_room(&files, "1000", NO_MUSIC, false);
    assert_eq!(heap.em_set(&files, 0, ZOMBIE, 0, 0, false, true), None);
    heap.enter_room(&files, "1010", NO_MUSIC, false);
    heap.em_set(&files, 0, ZOMBIE, 0, 0, false, true);
    assert!(heap.image.bytes.len() <= CAP);
    assert_eq!(heap.top, None);
}
