//! RE2 sound effects: the `.sap` sound banks and `bio2.exe`'s SE dispatch.
//!
//! The PC release keeps each bank's sounds as one `.sap` under
//! `Common/Sound` (see [`crate::sap`]: a WAV per mask bit, the bit being the
//! slot). `convert-game` writes every bank as one WAV per slot, named as the
//! executable formats the source file (`SsLoadSap`) but with the bank number
//! in hex, the way the rest of the pack numbers things:
//!
//! | source | entry | kind |
//! |---|---|---|
//! | `door/door%02d.sap` | `sound/door/<id:02x>/<slot:02x>.wav` | 0, 4 slots |
//! | `arms/weapon%02d.sap` | `sound/arms/<id:02x>/<slot:02x>.wav` | 1, 32 slots |
//! | `room/room%d%02x.sap` | `sound/room/<stage><room:02x>/<slot:02x>.wav` | 2, 48 slots |
//! | `room/fs%02d.sap` | `sound/step/<id:02x>/<slot:02x>.wav` | room slots 23-31 |
//! | `enemy/enemy%02d.sap` | `sound/enemy/<id:02x>/<slot:02x>.wav` | 3, 32 slots |
//! | `core/core%02d.sap` | `sound/core/<id:02x>/<slot:02x>.wav` | 4, 22 slots |
//! | `{arms,enemy,core}/*.EDH` | `sound/<kind>/<id:02x>/bank.edh` | |
//! | RDT sections 0 and 1 | `room/<id>/sound.edh` | |
//!
//! The `.sap` names count in decimal (`enemy37.sap`) and the `.EDH` names in
//! hex (`enem25.edh`) for the same bank, so both land in `sound/enemy/25/`.
//! The room bank's `<stage>` is the stage number (`1`-`7`); `room000` is a
//! shared bank some rooms add on top of their own.
//!
//! An EDH ([`Edh`]) is a bank's sound table (the EDT, four bytes per slot)
//! followed by a PlayStation VAB header (VH), whose tone attributes give each
//! slot's volume. A room carries its EDT and VH in its RDT's first two
//! sections; the converter stores them in the same layout.
//!
//! [`Banks`] mirrors the executable's buffer arrays (`BufferDoor`,
//! `BufferArms`, `BufferRoom`, `BufferEnemy`, `BufferCore`) as `SsLoadBanks`
//! fills them, and [`Banks::play`] resolves a `Snd_se_on` sound id to the
//! slot's pack stem and gain. The per-room special cases in `Snd_se_on` and
//! `SsPlay` (room 70A's slot shift, the forced volumes of a few rooms) and the
//! 3D panning are not modelled yet.

use anyhow::{Result, ensure};

use crate::pack::{AssetClass, Pack};

/// The pack folder every bank sits under.
pub const DIR: &str = "sound";

/// The footstep sets' folder under [`DIR`].
pub const STEP_FOLDER: &str = "step";

/// A bank's sound table and VAB header, beside its slots.
pub const EDH_NAME: &str = "bank.edh";

/// A room's sound table and VAB header, in its room folder.
pub const ROOM_EDH_NAME: &str = "sound.edh";

/// The slots a room bank's footstep sets fill: set `n` of the room's three
/// (`footstep_tbl`) starts at slot `23 + 3n` (`SsLoadSteps`).
pub const STEP_BASE: u8 = 0x17;

/// The shared core bank `SsLoadBanks` adds to every core bank but `0x15`.
const SHARED_CORE: u8 = 0x16;

/// The core bank whose load skips [`SHARED_CORE`].
const SOLO_CORE: u8 = 0x15;

/// The enemy banks a room with reverb byte 1 swaps for their echoing copy
/// `id + 0x3d` (`Snd_load_em`).
const ECHO_ENEMIES: [u8; 2] = [0x25, 0x26];

/// The offset of an echoing enemy bank.
const ECHO_OFFSET: u8 = 0x3d;

/// Rooms per stage in [`FOOTSTEPS`].
const FOOTSTEP_ROOMS: usize = 29;

/// The kind of a sound bank: the high byte of a sound id and the `type`
/// of the executable's `SsPlay`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    Door,
    Arms,
    Room,
    Enemy,
    Core,
}

impl Kind {
    pub const ALL: [Self; 5] = [Self::Door, Self::Arms, Self::Room, Self::Enemy, Self::Core];

    /// The kind of `SsPlay` type `value`.
    pub fn from_type(value: u8) -> Option<Self> {
        Self::ALL.get(usize::from(value)).copied()
    }

    /// The `SsPlay` type.
    pub fn index(self) -> usize {
        self as usize
    }

    /// The bank folder under [`DIR`].
    pub fn folder(self) -> &'static str {
        match self {
            Self::Door => "door",
            Self::Arms => "arms",
            Self::Room => "room",
            Self::Enemy => "enemy",
            Self::Core => "core",
        }
    }

    /// The slots the kind's buffer array holds (`SsCreateBuffer` refuses
    /// any past them).
    pub fn slots(self) -> usize {
        match self {
            Self::Door => 4,
            Self::Arms | Self::Enemy => 32,
            Self::Room => 48,
            Self::Core => 22,
        }
    }
}

/// The pack folder of bank `id` in `folder` (a [`Kind::folder`] or
/// [`STEP_FOLDER`]).
pub fn bank_dir(folder: &str, id: &str) -> String {
    format!("{DIR}/{folder}/{id}")
}

/// The pack stem of `slot` in the bank folder `dir`.
pub fn slot_stem(dir: &str, slot: u8) -> String {
    format!("{dir}/{slot:02x}")
}

/// The room bank id of stage `stage` (1-7, or 0 for the shared `room000`)
/// and room `room`: `room%d%02x`'s digits.
pub fn room_bank(stage: u8, room: u8) -> String {
    format!("{stage}{room:02x}")
}

/// The bank folder (`folder`, id) of a `Common/Sound` `.sap` stem, matched
/// case-insensitively.
pub fn sap_bank(stem: &str) -> Option<(&'static str, String)> {
    let stem = stem.to_ascii_lowercase();
    let decimal = |prefix: &str| -> Option<String> {
        let digits = stem.strip_prefix(prefix)?;
        (digits.len() == 2 && digits.bytes().all(|b| b.is_ascii_digit()))
            .then(|| format!("{:02x}", digits.parse::<u8>().unwrap()))
    };
    if let Some(id) = decimal("door") {
        return Some(("door", id));
    }
    if let Some(id) = decimal("weapon") {
        return Some(("arms", id));
    }
    if let Some(id) = decimal("enemy") {
        return Some(("enemy", id));
    }
    if let Some(id) = decimal("core") {
        return Some(("core", id));
    }
    if let Some(id) = decimal("fs") {
        return Some((STEP_FOLDER, id));
    }
    let digits = stem.strip_prefix("room")?;
    let bytes = digits.as_bytes();
    (bytes.len() == 3 && bytes[0].is_ascii_digit() && u8::from_str_radix(&digits[1..], 16).is_ok())
        .then(|| ("room", digits.to_owned()))
}

/// The bank folder (`folder`, id) of a `Common/Sound` `.EDH` stem, matched
/// case-insensitively.
pub fn edh_bank(stem: &str) -> Option<(&'static str, String)> {
    let stem = stem.to_ascii_lowercase();
    let hex = |prefix: &str| -> Option<String> {
        let digits = stem.strip_prefix(prefix)?;
        (digits.len() == 2 && u8::from_str_radix(digits, 16).is_ok()).then(|| digits.to_owned())
    };
    if let Some(id) = hex("arms") {
        return Some(("arms", id));
    }
    if let Some(id) = hex("enem") {
        return Some(("enemy", id));
    }
    hex("core").map(|id| ("core", id))
}

/// A bank's sound table (EDT) and VAB header (VH).
///
/// The file is the EDT, the VH, then an 8-byte footer whose first `u32` is
/// the VH's offset (the executable reads it at `size - 8`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edh {
    table: Vec<u8>,
    header: Vec<u8>,
}

/// The VAB header's fixed part before the program table.
const VH_HEADER: usize = 0x20;
/// The program table: 128 programs of 16 bytes.
const VH_PROGRAMS: usize = 128 * 0x10;
/// One tone attribute record; 16 per program.
const VH_TONE: usize = 0x20;
/// A tone's volume byte.
const TONE_VOLUME: usize = 2;
/// The EDH footer after the VH.
const FOOTER: usize = 8;

impl Edh {
    /// Read an EDH file.
    pub fn parse(data: &[u8]) -> Result<Self> {
        ensure!(data.len() >= FOOTER, "EDH is shorter than its footer");
        let body = data.len() - FOOTER;
        let offset = u32::from_le_bytes(data[body..body + 4].try_into().unwrap()) as usize;
        ensure!(
            offset <= body && offset.is_multiple_of(4),
            "EDH VH offset 0x{offset:x} is not inside its {body}-byte body"
        );
        Self::new(&data[..offset], &data[offset..body])
    }

    /// The pair from a sound table and a VAB header (an RDT's first two
    /// sections).
    pub fn new(table: &[u8], header: &[u8]) -> Result<Self> {
        ensure!(
            table.len().is_multiple_of(4),
            "EDT of {} bytes is not whole entries",
            table.len()
        );
        ensure!(
            header.len() >= VH_HEADER + VH_PROGRAMS && header.starts_with(b"pBAV"),
            "EDH has no VAB header"
        );
        Ok(Self {
            table: table.to_vec(),
            header: header.to_vec(),
        })
    }

    /// The EDH file bytes ([`Self::parse`] reads them back).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = self.table.clone();
        out.extend(&self.header);
        out.extend((self.table.len() as u32).to_le_bytes());
        out.extend(0u32.to_le_bytes());
        out
    }

    /// The table's entry for `slot`, if the table has one that is not the
    /// `ffffffff` filler.
    pub fn entry(&self, slot: u8) -> Option<[u8; 4]> {
        let at = usize::from(slot) * 4;
        let entry: [u8; 4] = self.table.get(at..at + 4)?.try_into().ok()?;
        (entry != [0xff; 4]).then_some(entry)
    }

    /// The volume (0-127) of `slot`'s tone: entry byte 1's low seven bits
    /// pick the program and byte 2's high nibble its tone (`Snd_se_on`).
    pub fn volume(&self, slot: u8) -> Option<u8> {
        let entry = self.entry(slot)?;
        let tone = usize::from(entry[1] & 0x7f) * 16 + usize::from(entry[2] >> 4);
        self.header
            .get(VH_HEADER + VH_PROGRAMS + tone * VH_TONE + TONE_VOLUME)
            .copied()
    }
}

/// The DirectSound attenuation in millibels of an SE volume (0-127), as
/// `SsSetVol` converts it: full scale at 127, -100 dB at 0.
pub fn volume_millibels(volume: i32) -> i32 {
    let millibels = if volume < 0x20 {
        volume * 0x103 - 10000
    } else {
        (volume * 9 - 0x477) * 2
    };
    millibels.clamp(-10000, 0)
}

/// The mixer gain of an SE volume (0-127).
pub fn volume_gain(volume: u8) -> f32 {
    crate::sfx::volume_gain(volume_millibels(i32::from(volume)))
}

/// A `Snd_se_on` sound id: the bank kind in the high byte, the slot in the
/// next, and the 3D flag (non-zero: pan from the source position) in the
/// low byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SoundId {
    pub kind: Kind,
    pub slot: u8,
    pub positional: bool,
}

impl SoundId {
    /// Split a sound id; one whose kind is not a bank is none.
    pub fn decode(id: u32) -> Option<Self> {
        Some(Self {
            kind: Kind::from_type((id >> 24) as u8)?,
            slot: (id >> 16) as u8,
            positional: id & 0xff != 0,
        })
    }
}

/// A resolved sound: the pack stem to play and its gain.
#[derive(Clone, Debug, PartialEq)]
pub struct Play<'a> {
    pub stem: &'a str,
    pub gain: f32,
}

/// The sounds loaded per kind, as `SsLoadBanks` leaves the buffer arrays.
#[derive(Clone, Debug, Default)]
pub struct Banks {
    slots: [Vec<Option<String>>; 5],
    tables: [Option<Edh>; 5],
}

impl Banks {
    pub fn new() -> Self {
        Self {
            slots: Kind::ALL.map(|kind| vec![None; kind.slots()]),
            tables: Default::default(),
        }
    }

    /// Drop `kind`'s sounds and table (`SsUnloadGroup`).
    pub fn unload(&mut self, kind: Kind) {
        self.slots[kind.index()].fill(None);
        self.tables[kind.index()] = None;
    }

    /// Fill `kind`'s slots from bank folder `dir`, its slot `n` going to slot
    /// `base + n`; slots the bank leaves empty keep what they hold. Returns
    /// whether the folder held any sound.
    fn load_dir(&mut self, pack: &Pack, kind: Kind, dir: &str, base: u8, count: u8) -> bool {
        let mut any = false;
        for slot in 0..count {
            let target = usize::from(base) + usize::from(slot);
            if target >= kind.slots() {
                break;
            }
            let stem = slot_stem(dir, slot);
            if pack.has_asset(&stem, AssetClass::Audio) {
                self.slots[kind.index()][target] = Some(stem);
                any = true;
            }
        }
        any
    }

    /// Read `dir`'s [`EDH_NAME`] as `kind`'s table, if it has one that
    /// parses.
    fn load_table(&mut self, pack: &Pack, kind: Kind, dir: &str) {
        let entry = format!("{dir}/{EDH_NAME}");
        self.tables[kind.index()] = pack
            .read(&entry)
            .ok()
            .and_then(|data| Edh::parse(&data).ok());
    }

    /// Set `kind`'s sound table: the door kind's comes from a core EDH and
    /// the room's from the room folder's [`ROOM_EDH_NAME`].
    pub fn set_table(&mut self, kind: Kind, table: Option<Edh>) {
        self.tables[kind.index()] = table;
    }

    /// Load door bank `id` (`door%02d.sap`), replacing the door sounds.
    pub fn load_door(&mut self, pack: &Pack, id: u8) {
        self.slots[Kind::Door.index()].fill(None);
        self.load_dir(
            pack,
            Kind::Door,
            &bank_dir("door", &format!("{id:02x}")),
            0,
            48,
        );
    }

    /// Load weapon bank `id` (`weapon%02d.sap`, `ARMS%02X.EDH`), replacing
    /// the weapon sounds.
    pub fn load_arms(&mut self, pack: &Pack, id: u8) {
        self.unload(Kind::Arms);
        let dir = bank_dir("arms", &format!("{id:02x}"));
        self.load_dir(pack, Kind::Arms, &dir, 0, 48);
        self.load_table(pack, Kind::Arms, &dir);
    }

    /// Load the enemy bank of enemy `id` (`ENEM%02X.EDH` and
    /// `enemy%02d.sap`), replacing the enemy sounds. In a room whose RDT
    /// header byte 6 is 1, enemies `25` and `26` take their echoing sounds
    /// from bank `id + 0x3d` (`Snd_load_em`).
    pub fn load_enemy(&mut self, pack: &Pack, id: u8, reverb: u8) {
        self.unload(Kind::Enemy);
        let bank = if reverb == 1 && ECHO_ENEMIES.contains(&id) {
            id + ECHO_OFFSET
        } else {
            id
        };
        let dir = bank_dir("enemy", &format!("{bank:02x}"));
        self.load_dir(pack, Kind::Enemy, &dir, 0, 48);
        self.load_table(pack, Kind::Enemy, &bank_dir("enemy", &format!("{id:02x}")));
    }

    /// Load core bank `id` (`core%02d.sap`, `CORE%02X.EDH`) and, for every
    /// bank but `15`, the shared bank `16` over it, replacing the core
    /// sounds.
    pub fn load_core(&mut self, pack: &Pack, id: u8) {
        self.unload(Kind::Core);
        let dir = bank_dir("core", &format!("{id:02x}"));
        self.load_dir(pack, Kind::Core, &dir, 0, 48);
        if id != SOLO_CORE {
            let shared = bank_dir("core", &format!("{SHARED_CORE:02x}"));
            self.load_dir(pack, Kind::Core, &shared, 0, 48);
        }
        self.load_table(pack, Kind::Core, &dir);
    }

    /// Load the room bank of `stage` (0-based, as the scripts number it) and
    /// `room`, replacing the room sounds: the room's own `.sap`, its three
    /// footstep sets at slots 23, 26 and 29 ([`footsteps`]), then the shared
    /// `room000` for the rooms [`shares_room000`] names. `table` is the
    /// room's [`ROOM_EDH_NAME`].
    pub fn load_room(&mut self, pack: &Pack, stage: u8, room: u8, table: Option<Edh>) {
        self.unload(Kind::Room);
        let dir = bank_dir("room", &room_bank(stage + 1, room));
        self.load_dir(pack, Kind::Room, &dir, 0, 48);
        for (set, step) in footsteps(stage, room).into_iter().enumerate() {
            if let Some(step) = step {
                let dir = bank_dir(STEP_FOLDER, &format!("{step:02x}"));
                self.load_dir(pack, Kind::Room, &dir, STEP_BASE + 3 * set as u8, 32);
            }
        }
        if shares_room000(stage, room) {
            self.load_dir(pack, Kind::Room, &bank_dir("room", &room_bank(0, 0)), 0, 48);
        }
        self.tables[Kind::Room.index()] = table;
    }

    /// The pack stem loaded in `kind`'s `slot`.
    pub fn stem(&self, kind: Kind, slot: u8) -> Option<&str> {
        self.slots[kind.index()].get(usize::from(slot))?.as_deref()
    }

    /// The sound `Snd_se_on` plays for `id`: the slot's stem, at the
    /// volume its table gives (full scale without a table entry). The 3D
    /// pan and volume of a positional sound are the caller's.
    pub fn play(&self, id: SoundId) -> Option<Play<'_>> {
        let stem = self.stem(id.kind, id.slot)?;
        let volume = self.tables[id.kind.index()]
            .as_ref()
            .and_then(|table| table.volume(id.slot))
            .unwrap_or(0x7f);
        Some(Play {
            stem,
            gain: volume_gain(volume),
        })
    }
}

/// The three footstep sets (`fs%02d`) a room bank loads, from `bio2.exe`'s
/// `footstep_tbl` (`stage * 29 + room`, stage 0-based).
pub fn footsteps(stage: u8, room: u8) -> [Option<u8>; 3] {
    let row = usize::from(stage) * FOOTSTEP_ROOMS + usize::from(room);
    let sets = if usize::from(room) < FOOTSTEP_ROOMS {
        FOOTSTEPS.get(row).copied().unwrap_or([-1; 3])
    } else {
        [-1; 3]
    };
    sets.map(|set| u8::try_from(set).ok())
}

/// Whether the room bank of `stage` (0-based) and `room` also loads the
/// shared `room000` (`Room_fs_ck`).
pub fn shares_room000(stage: u8, room: u8) -> bool {
    matches!(
        (stage, room),
        (0, 0x0f)
            | (1, 0x01 | 0x08)
            | (2, 0x0c | 0x0d)
            | (3, 0x01 | 0x03)
            | (4, 0x03)
            | (5, 0x03 | 0x07 | 0x12)
            | (6, 0x01 | 0x04)
    )
}

/// `footstep_tbl` (`0x522208` in `bio2 1.10.exe`): each room's three
/// footstep sets, `-1` for none, 29 rooms per stage.
#[rustfmt::skip]
const FOOTSTEPS: [[i8; 3]; 7 * FOOTSTEP_ROOMS] = [
    // stage 1
    [0, 1, 2], [-1, -1, 3], [-1, 4, 2], [5, 6, 7], [-1, 1, 2], [-1, 8, 2],
    [-1, 9, 10], [-1, 11, 7], [-1, 12, 10], [-1, 14, 13], [-1, 3, 18], [-1, 15, 16],
    [17, 16, 18], [-1, 19, 3], [-1, 16, 3], [-1, -1, 16], [-1, -1, 20], [-1, 17, 21],
    [-1, 17, 18], [-1, -1, 22], [-1, 23, 21], [-1, 24, 25], [12, 11, 7], [-1, -1, 20],
    [26, 27, 28], [-1, -1, 2], [-1, 23, 21], [-1, 27, 2], [-1, 12, 10],
    // stage 2
    [20, 29, 29], [-1, -1, 25], [-1, 30, 25], [-1, 32, 31], [-1, 30, 25], [-1, 36, 33],
    [18, 37, 25], [-1, -1, 28], [-1, 31, 3], [-1, -1, 33], [-1, -1, 38], [-1, 12, 25],
    [-1, 15, 16], [39, 21, 10], [-1, -1, 40], [-1, 41, 40], [-1, -1, 40], [-1, -1, 28],
    [-1, -1, 28], [-1, 42, 28], [-1, -1, 28], [-1, -1, 28], [-1, -1, 28], [-1, -1, -1],
    [-1, -1, -1], [-1, -1, 28], [0, 3, 33], [-1, 16, 3], [-1, -1, -1],
    // stage 3
    [-1, -1, 28], [-1, -1, 28], [-1, 49, 28], [-1, 42, 28], [42, 49, 28], [7, 42, 28],
    [42, 50, 28], [42, 28, 6], [-1, -1, 28], [-1, -1, 26], [0, 49, 28], [-1, -1, 28],
    [-1, -1, 28], [-1, -1, 28], [-1, -1, -1], [-1, -1, -1], [-1, -1, -1], [-1, -1, -1],
    [-1, -1, -1], [-1, -1, -1], [-1, -1, -1], [-1, -1, -1], [-1, -1, -1], [-1, -1, -1],
    [-1, -1, -1], [-1, -1, -1], [-1, -1, -1], [-1, -1, -1], [-1, -1, -1],
    // stage 4
    [-1, 49, 28], [-1, 42, 28], [-1, 49, 28], [-1, 42, 28], [-1, 51, 42], [-1, 52, 28],
    [-1, -1, -1], [-1, 52, 28], [42, -1, 28], [42, -1, 28], [-1, 42, 28], [-1, -1, 28],
    [-1, 28, 42], [-1, 53, 26], [28, 53, 26], [-1, -1, 28], [-1, 52, 28], [-1, -1, -1],
    [-1, -1, -1], [-1, -1, -1], [-1, -1, -1], [-1, -1, -1], [-1, -1, -1], [-1, -1, -1],
    [-1, -1, -1], [-1, -1, -1], [-1, -1, -1], [-1, -1, -1], [-1, -1, -1],
    // stage 5
    [-1, -1, 34], [-1, -1, 34], [-1, 42, 34], [-1, -1, 35], [-1, 42, 34], [-1, -1, 42],
    [-1, 42, 34], [-1, 42, 26], [-1, 42, 34], [-1, -1, 42], [-1, -1, -1], [-1, -1, -1],
    [-1, -1, -1], [-1, -1, -1], [-1, -1, -1], [-1, -1, -1], [-1, -1, -1], [-1, -1, -1],
    [-1, -1, -1], [-1, -1, -1], [-1, -1, -1], [-1, -1, -1], [-1, -1, -1], [-1, -1, -1],
    [-1, -1, -1], [-1, -1, -1], [-1, -1, -1], [-1, -1, -1], [-1, -1, -1],
    // stage 6
    [-1, -1, 42], [-1, -1, 43], [26, 42, 28], [-1, -1, 42], [26, 44, -1], [26, 44, 42],
    [-1, 42, 34], [-1, -1, 35], [-1, 26, 45], [-1, -1, 43], [-1, -1, 43], [-1, -1, 28],
    [-1, 44, 28], [-1, -1, 46], [-1, 26, -1], [36, 0, 34], [-1, -1, 28], [-1, 26, 45],
    [-1, -1, 47], [-1, -1, 48], [26, 42, 28], [-1, 26, 28], [28, 42, 47], [-1, 26, 28],
    [-1, -1, -1], [-1, -1, -1], [-1, -1, -1], [-1, -1, -1], [-1, -1, -1],
    // stage 7
    [26, 42, 28], [12, 42, 28], [-1, 0, 28], [26, 42, 28], [-1, 42, 28], [-1, -1, -1],
    [-1, -1, -1], [-1, -1, -1], [-1, -1, -1], [-1, -1, -1], [-1, -1, -1], [-1, -1, -1],
    [-1, -1, -1], [-1, -1, -1], [-1, -1, -1], [-1, -1, -1], [-1, -1, -1], [-1, -1, -1],
    [-1, -1, -1], [-1, -1, -1], [-1, -1, -1], [-1, -1, -1], [-1, -1, -1], [-1, -1, -1],
    [-1, -1, -1], [-1, -1, -1], [-1, -1, -1], [-1, -1, -1], [-1, -1, -1],
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::PackWriter;

    /// A VAB header whose program 1, tone 2 has volume `volume`.
    fn vh(volume: u8) -> Vec<u8> {
        let mut out = vec![0; VH_HEADER + VH_PROGRAMS + 32 * VH_TONE];
        out[..4].copy_from_slice(b"pBAV");
        out[VH_HEADER + VH_PROGRAMS + 18 * VH_TONE + TONE_VOLUME] = volume;
        out
    }

    #[test]
    fn source_names_map_to_hex_bank_folders() {
        assert_eq!(sap_bank("door54"), Some(("door", "36".into())));
        assert_eq!(sap_bank("WEAPON19"), Some(("arms", "13".into())));
        assert_eq!(sap_bank("enemy37"), Some(("enemy", "25".into())));
        assert_eq!(sap_bank("core22"), Some(("core", "16".into())));
        assert_eq!(sap_bank("fs53"), Some(("step", "35".into())));
        assert_eq!(sap_bank("room10a"), Some(("room", "10a".into())));
        assert_eq!(sap_bank("room000"), Some(("room", "000".into())));
        assert_eq!(sap_bank("room1"), None);
        assert_eq!(sap_bank("door0a"), None);
        assert_eq!(sap_bank("main00"), None);
        assert_eq!(edh_bank("ENEM3A"), Some(("enemy", "3a".into())));
        assert_eq!(edh_bank("arms13"), Some(("arms", "13".into())));
        assert_eq!(edh_bank("CORE0B"), Some(("core", "0b".into())));
        assert_eq!(edh_bank("enemy00"), None);
    }

    #[test]
    fn an_edh_round_trips_and_gives_tone_volumes() {
        let mut table = vec![0xff; 12];
        table[4..8].copy_from_slice(&[0, 0x81, 0x23, 2]);
        let edh = Edh::new(&table, &vh(100)).unwrap();
        let bytes = edh.to_bytes();
        assert_eq!(bytes.len() - 8, 12 + vh(100).len());
        assert_eq!(&bytes[bytes.len() - 8..][..4], &12u32.to_le_bytes());
        let back = Edh::parse(&bytes).unwrap();
        assert_eq!(back, edh);
        assert_eq!(back.entry(0), None);
        assert_eq!(back.entry(1), Some([0, 0x81, 0x23, 2]));
        assert_eq!(back.volume(1), Some(100));
        assert_eq!(back.volume(2), None);
        assert_eq!(back.volume(9), None);
        assert!(Edh::parse(&bytes[..bytes.len() - 1]).is_err());
        assert!(Edh::new(&table, b"pBAV").is_err());
    }

    #[test]
    fn volumes_follow_the_directsound_curve() {
        assert_eq!(volume_millibels(127), 0);
        assert_eq!(volume_millibels(0), -10000);
        assert_eq!(volume_millibels(31), 31 * 0x103 - 10000);
        assert_eq!(volume_millibels(64), (64 * 9 - 0x477) * 2);
        assert_eq!(volume_millibels(200), 0);
        assert_eq!(volume_gain(127), 1.0);
        assert!(volume_gain(100) < 1.0);
    }

    #[test]
    fn sound_ids_split_into_kind_slot_and_flag() {
        assert_eq!(
            SoundId::decode(0x0214_0001),
            Some(SoundId {
                kind: Kind::Room,
                slot: 0x14,
                positional: true
            })
        );
        assert_eq!(SoundId::decode(0x0403_0000).unwrap().kind, Kind::Core);
        assert!(!SoundId::decode(0x0403_0000).unwrap().positional);
        assert_eq!(SoundId::decode(0x0500_0000), None);
    }

    #[test]
    fn footstep_sets_come_from_the_executable_table() {
        assert_eq!(footsteps(0, 0), [Some(0), Some(1), Some(2)]);
        assert_eq!(footsteps(0, 1), [None, None, Some(3)]);
        assert_eq!(footsteps(6, 1), [Some(12), Some(42), Some(28)]);
        assert_eq!(footsteps(6, 28), [None; 3]);
        assert_eq!(footsteps(0, 29), [None; 3]);
        assert_eq!(footsteps(9, 0), [None; 3]);
        assert!(shares_room000(0, 0x0f));
        assert!(!shares_room000(0, 0));
    }

    fn wav() -> Vec<u8> {
        b"RIFF\x04\0\0\0WAVE".to_vec()
    }

    #[test]
    fn a_room_bank_layers_its_footsteps_and_room000() {
        let mut writer = PackWriter::new();
        // Stage 1 room 0x0f (stage index 0): footstep sets [-1, -1, 16].
        for slot in [0u8, 23, 29, 30] {
            writer
                .add(&format!("sound/room/10f/{slot:02x}.wav"), wav())
                .unwrap();
        }
        writer.add("sound/step/10/00.wav", wav()).unwrap();
        writer.add("sound/step/10/02.wav", wav()).unwrap();
        writer.add("sound/room/000/00.wav", wav()).unwrap();
        let pack = Pack::from_bytes(writer.to_bytes().unwrap()).unwrap();

        let mut banks = Banks::new();
        let mut table = vec![0xff; 48 * 4];
        table[29 * 4..30 * 4].copy_from_slice(&[0, 1, 0x20, 0]);
        banks.load_room(&pack, 0, 0x0f, Some(Edh::new(&table, &vh(64)).unwrap()));
        assert_eq!(banks.stem(Kind::Room, 0), Some("sound/room/000/00"));
        assert_eq!(banks.stem(Kind::Room, 23), Some("sound/room/10f/17"));
        assert_eq!(banks.stem(Kind::Room, 29), Some("sound/step/10/00"));
        assert_eq!(banks.stem(Kind::Room, 30), Some("sound/room/10f/1e"));
        assert_eq!(banks.stem(Kind::Room, 31), Some("sound/step/10/02"));
        assert_eq!(banks.stem(Kind::Room, 1), None);

        let play = banks.play(SoundId::decode(0x021d_0000).unwrap()).unwrap();
        assert_eq!(play.stem, "sound/step/10/00");
        assert_eq!(play.gain, volume_gain(64));
        let play = banks.play(SoundId::decode(0x0200_0000).unwrap()).unwrap();
        assert_eq!(play.gain, 1.0);
        assert_eq!(banks.play(SoundId::decode(0x0201_0000).unwrap()), None);

        banks.unload(Kind::Room);
        assert_eq!(banks.stem(Kind::Room, 0), None);
    }

    #[test]
    fn core_enemy_and_door_banks_load_their_folders() {
        let mut writer = PackWriter::new();
        writer.add("sound/core/0b/00.wav", wav()).unwrap();
        writer.add("sound/core/0b/01.wav", wav()).unwrap();
        writer.add("sound/core/16/01.wav", wav()).unwrap();
        writer.add("sound/core/15/02.wav", wav()).unwrap();
        writer.add("sound/enemy/25/03.wav", wav()).unwrap();
        writer.add("sound/enemy/62/04.wav", wav()).unwrap();
        writer
            .add(
                "sound/enemy/25/bank.edh",
                Edh::new(&[0, 1, 0x20, 0], &vh(1)).unwrap().to_bytes(),
            )
            .unwrap();
        writer.add("sound/door/36/01.wav", wav()).unwrap();
        writer.add("sound/door/36/05.wav", wav()).unwrap();
        writer.add("sound/arms/13/1f.wav", wav()).unwrap();
        let pack = Pack::from_bytes(writer.to_bytes().unwrap()).unwrap();

        let mut banks = Banks::new();
        banks.load_core(&pack, 0x0b);
        assert_eq!(banks.stem(Kind::Core, 0), Some("sound/core/0b/00"));
        assert_eq!(banks.stem(Kind::Core, 1), Some("sound/core/16/01"));
        banks.load_core(&pack, 0x15);
        assert_eq!(banks.stem(Kind::Core, 0), None);
        assert_eq!(banks.stem(Kind::Core, 2), Some("sound/core/15/02"));

        banks.load_enemy(&pack, 0x25, 0);
        assert_eq!(banks.stem(Kind::Enemy, 3), Some("sound/enemy/25/03"));
        banks.load_enemy(&pack, 0x25, 1);
        assert_eq!(banks.stem(Kind::Enemy, 3), None);
        assert_eq!(banks.stem(Kind::Enemy, 4), Some("sound/enemy/62/04"));
        // The table stays the enemy's own.
        assert!(banks.tables[Kind::Enemy.index()].is_some());

        banks.load_door(&pack, 0x36);
        assert_eq!(banks.stem(Kind::Door, 1), Some("sound/door/36/01"));
        // The door array holds four slots.
        assert_eq!(banks.stem(Kind::Door, 5), None);

        banks.load_arms(&pack, 0x13);
        assert_eq!(banks.stem(Kind::Arms, 0x1f), Some("sound/arms/13/1f"));
    }
}
