//! Game and room state.
//!
//! These are the engine's internal state after an RDT has been loaded. Source
//! file formats are not kept around during gameplay.

use crate::mask::MaskSprite;

/// A stage/room/player identity, e.g. stage 1, room 0x00, player 1 -> RDT 1001.
///
/// The identity is packed hex: the first digit is the stage, the middle two are
/// the room in hex, and the last is the player/flag digit. This matches the
/// shipped `ROOM####.RDT` names and the `RC` background names.
///
/// A generated game (door randomization) numbers its own stages and rooms:
/// any stage 1-255 and any room 0x00-0xFF. An id the short form cannot spell
/// (a stage other than 1-7 and `A`-`G`, or a room past 0x1F) takes the long
/// form: the stage and the room as two hex digits each, joined by `-`
/// (`09-40`), and the player digit after another `-` (`09-40-1`). The long
/// form is only for those ids, so every id has one spelling
/// ([`RoomId::has_short_form`]).
///
/// Such a room is usually a copy of an original room, its *origin*
/// ([`RoomOrigins`]): the engine behaviour, music and sound tables keyed to
/// a room follow the origin, not the id.
///
/// RE1's mansion (stages 1 and 2) has two visits: the second, after the
/// guardhouse sets scenario flag 0, plays the same rooms with their own
/// lines, lights and items. The visit is part of the identity
/// ([`RoomId::mansion2`]); the room is still stage 1 or 2. (The original
/// game numbered the second visit's rooms as stages 6 and 7, and its data
/// tables still do; [`RoomId::from_original`] and [`RoomId::table_row`] are
/// that numbering.)
///
/// The `(stage, room)` of the rooms only the second mansion visit has.
const SECOND_VISIT_ONLY: [(u8, u8); 14] = [
    (1, 0x10),
    (1, 0x19),
    (2, 0x00),
    (2, 0x0C),
    (2, 0x13),
    (2, 0x14),
    (2, 0x15),
    (2, 0x16),
    (2, 0x17),
    (2, 0x18),
    (2, 0x19),
    (2, 0x1A),
    (2, 0x1B),
    (2, 0x1C),
];

/// RE2 also ships letter stages `A`-`G` (`ROOMG010.RDT`), which are stages
/// 10-16 here: `A`-`F` are their own hex digits and `G` continues the run.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RoomId {
    /// 1-based stage number: 1-5 for RE1, 1-7 or 10-16 (the letter stages
    /// `A`-`G`) for RE2, any of 1-255 in a generated game.
    pub stage: u8,
    /// Room number within the stage: 0x00-0x1F in the original games, any
    /// byte in a generated game.
    pub room: u8,
    /// Player/flag digit (0 = Chris, 1 = Jill).
    pub player_flag: u8,
    /// The second visit of an RE1 mansion room (stage 1 or 2).
    pub mansion2: bool,
}

impl RoomId {
    /// RE1's stage count: the mansion (1 and 2), the guardhouse (3), the
    /// caves (4) and the laboratory (5).
    pub const RE1_STAGES: u8 = 5;
    /// Highest numeric stage digit an identity can name (RE2's last).
    pub const MAX_STAGE: u8 = 7;
    /// Highest stage an identity can name: the letter stage `G`.
    pub const MAX_STAGE_ID: u8 = 16;
    /// Highest room number the short form spells and the original's
    /// per-room tables hold. A generated game's rooms go up to 0xFF.
    pub const MAX_ROOM: u8 = 0x1F;
    /// The rows of the original game's per-room tables: one per stage, then
    /// one per mansion stage for the second visit ([`RoomId::table_row`]).
    pub const TABLE_ROWS: usize = 7;

    /// Parse a three- or four-digit RDT identity, e.g. `100`, `1001`, `11C1`,
    /// `G010`, or a long one, `09-40` or `09-40-1`.
    pub fn parse(text: &str) -> anyhow::Result<Self> {
        if text.contains('-') {
            return Self::parse_long(text, None);
        }
        let digits = Self::digits(text)
            .ok_or_else(|| anyhow::anyhow!("`{text}` is not a hex RDT identity"))?;
        match digits.as_slice() {
            [stage, hi, lo] => Self::from_digits(*stage, (*hi << 4) | *lo, 0, text),
            [stage, hi, lo, player] => Self::from_digits(*stage, (*hi << 4) | *lo, *player, text),
            _ => anyhow::bail!(
                "`{text}` must be three or four hex digits, e.g. 100 or 1001, or a long id such as 09-40"
            ),
        }
    }

    /// Parse a room id without its player digit (`100`, `11C`, `09-40`),
    /// with player digit `player`.
    pub fn from_room_and_player(room: &str, player: u8) -> anyhow::Result<Self> {
        if room.contains('-') {
            return Self::parse_long(room, Some(player));
        }
        let digits =
            Self::digits(room).ok_or_else(|| anyhow::anyhow!("`{room}` is not a hex room id"))?;
        match digits.as_slice() {
            [stage, hi, lo] => Self::from_digits(*stage, (*hi << 4) | *lo, player, room),
            _ => anyhow::bail!(
                "`{room}` must be exactly three hex digits, e.g. 100 or 11C, or a long id such as 09-40"
            ),
        }
    }

    /// Parse a room folder's name, the room id without a player digit
    /// (`100`, `09-40`), as player 0; `None` for anything else, an RDT
    /// identity (`1001`, `09-40-1`) included.
    pub fn parse_room(text: &str) -> Option<Self> {
        Self::from_room_and_player(text, 0).ok()
    }

    /// The long form: `SS-RR` (room, with player digit `player`) or
    /// `SS-RR-P` (RDT, when `player` is `None`), each part hex.
    fn parse_long(text: &str, player: Option<u8>) -> anyhow::Result<Self> {
        let parts: Vec<&str> = text.split('-').collect();
        let byte = |part: &str, what: &str| {
            (part.len() == 2 && part.bytes().all(|b| b.is_ascii_hexdigit()))
                .then(|| u8::from_str_radix(part, 16).ok())
                .flatten()
                .ok_or_else(|| anyhow::anyhow!("{what} in `{text}` must be two hex digits"))
        };
        let (stage, room, player_flag) = match (parts.as_slice(), player) {
            ([stage, room], Some(player)) => (byte(stage, "stage")?, byte(room, "room")?, player),
            ([stage, room, digit], None) => {
                let player = match digit.as_bytes() {
                    [b] if b.is_ascii_digit() => b - b'0',
                    _ => anyhow::bail!("player digit in `{text}` must be 0-9"),
                };
                (byte(stage, "stage")?, byte(room, "room")?, player)
            }
            (_, Some(_)) => anyhow::bail!("`{text}` is not a long room id such as 09-40"),
            (_, None) => anyhow::bail!("`{text}` is not a long RDT identity such as 09-40-1"),
        };
        if stage == 0 {
            anyhow::bail!("stage in `{text}` must be 01-FF");
        }
        if player_flag > 9 {
            anyhow::bail!("player digit in `{text}` must be 0-9");
        }
        let id = Self {
            stage,
            room,
            player_flag,
            mansion2: false,
        };
        if id.has_short_form() {
            anyhow::bail!(
                "`{text}` has a short form, `{}`; the long form is only for ids it cannot spell",
                if player.is_some() {
                    id.room3()
                } else {
                    id.code()
                }
            );
        }
        Ok(id)
    }

    /// Whether the short form spells this id: a stage of 1-7 or 10-16 (`A`-`G`)
    /// and a room up to [`RoomId::MAX_ROOM`]. Every id of the original games
    /// has one; the others take the long form (`09-40`).
    pub fn has_short_form(self) -> bool {
        ((1..=Self::MAX_STAGE).contains(&self.stage)
            || (10..=Self::MAX_STAGE_ID).contains(&self.stage))
            && self.room <= Self::MAX_ROOM
    }

    /// The room's `(stage, room)` in the original game's per-room tables:
    /// its [`RoomId::table_row`] and room, or `None` for a room no table
    /// row holds (a generated game's own stages and rooms past 0x1F).
    pub fn table_slot(self) -> Option<(usize, usize)> {
        let row = self.table_row();
        (row < Self::TABLE_ROWS && self.room <= Self::MAX_ROOM)
            .then_some((row, usize::from(self.room)))
    }

    /// The RE1 room the original game names RDT `number` (`0x6031`): its
    /// stages 6 and 7 are the second visit of the mansion's stages 1 and 2.
    /// `None` for a stage RE1 does not have.
    pub fn from_original(number: u16) -> Option<Self> {
        let stage = (number >> 12) as u8;
        let (stage, mansion2) = match stage {
            1..=5 => (stage, false),
            6 | 7 => (stage - 5, true),
            _ => return None,
        };
        Some(Self {
            stage,
            room: ((number >> 4) & 0xFF) as u8,
            player_flag: (number & 0xF) as u8,
            mansion2,
        })
    }

    /// The RDT number the original game gives this RE1 room, `0x6031` for
    /// room 103 played by Jill on the second mansion visit: the inverse of
    /// [`RoomId::from_original`], for the original's own files and traces.
    pub fn original_number(self) -> u16 {
        let stage = u16::from(self.stage) + if self.mansion2 { 5 } else { 0 };
        (stage << 12) | (u16::from(self.room) << 4) | u16::from(self.player_flag)
    }

    /// The row of the original game's per-room tables (music, effect
    /// sheets, camera lights, shadow and mask records) this room reads, 0-6:
    /// its 0-based stage, or for the second mansion visit the mansion
    /// stage's second-visit row (the original's stages 6 and 7).
    ///
    /// A room only the second visit has reads that row on any visit: the first
    /// visit's row is empty, and a generated game or the debug menu can enter
    /// the room without the scenario flag that picks the second visit.
    pub fn table_row(self) -> usize {
        let second = self.mansion2 || self.second_visit_only();
        usize::from(self.stage_index()) + if second { 5 } else { 0 }
    }

    /// The room of table row `row` and `room`, the inverse of
    /// [`RoomId::table_row`] for player 0. A row past the tables (a damaged
    /// save's) wraps rather than panicking.
    pub fn from_table_row(row: usize, room: u8) -> Self {
        let (stage, mansion2) = match row {
            5 | 6 => (row as u8 - 4, true),
            _ => ((row as u8).wrapping_add(1), false),
        };
        Self {
            stage,
            room,
            player_flag: 0,
            mansion2,
        }
    }

    /// This room on the second mansion visit when `mansion2` and it is a
    /// mansion room (stage 1 or 2); any other room has one visit.
    pub fn with_visit(self, mansion2: bool) -> Self {
        Self {
            mansion2: mansion2 && self.is_mansion(),
            ..self
        }
    }

    /// Whether only the second mansion visit has this room: its first visit's
    /// RDTs are stubs.
    pub fn second_visit_only(self) -> bool {
        SECOND_VISIT_ONLY.contains(&(self.stage, self.room))
    }

    /// Whether this is an RE1 mansion room (stage 1 or 2), which has two
    /// visits.
    pub fn is_mansion(self) -> bool {
        matches!(self.stage, 1 | 2)
    }

    /// The variant of room `origin` (`(stage, room)`) this id plays as: the
    /// origin's stage and room, this id's player, and the second visit when
    /// this id is on it and the origin is a mansion room.
    pub fn as_origin(self, (stage, room): (u8, u8)) -> Self {
        let origin = Self {
            stage,
            room,
            ..self
        };
        Self {
            mansion2: self.mansion2 && origin.is_mansion(),
            ..origin
        }
    }

    /// The digit values of `text`: hex, plus `G` (16) as the first one.
    fn digits(text: &str) -> Option<Vec<u8>> {
        text.chars()
            .enumerate()
            .map(|(index, c)| match c {
                'g' | 'G' if index == 0 => Some(Self::MAX_STAGE_ID),
                _ => c.to_digit(16).map(|d| d as u8),
            })
            .collect()
    }

    fn from_digits(stage: u8, room: u8, player_flag: u8, text: &str) -> anyhow::Result<Self> {
        if !(1..=Self::MAX_STAGE).contains(&stage) && !(10..=Self::MAX_STAGE_ID).contains(&stage) {
            anyhow::bail!("stage digit in `{text}` must be 1-7 or A-G");
        }
        if room > Self::MAX_ROOM {
            anyhow::bail!("room in `{text}` must be 00-1F");
        }
        if player_flag > 9 {
            anyhow::bail!("player digit in `{text}` must be 0-9");
        }
        Ok(Self {
            stage,
            room,
            player_flag,
            mansion2: false,
        })
    }

    /// Packed RDT number, e.g. 0x1001; the letter stage `G` is 0x10000 up.
    /// The visit is not part of it.
    pub fn rdt_number(self) -> u32 {
        (u32::from(self.stage) << 12) | (u32::from(self.room) << 4) | u32::from(self.player_flag)
    }

    /// The stage's digit: `1`-`7`, or the letter `a`-`g`. Only an id with a
    /// short form ([`RoomId::has_short_form`]) has one.
    pub fn stage_char(self) -> char {
        debug_assert!(self.stage <= Self::MAX_STAGE_ID, "stage {}", self.stage);
        match self.stage {
            0..=9 => char::from(b'0' + self.stage),
            stage => char::from(b'a' + (stage - 10).min(25)),
        }
    }

    /// RDT identity as a per-RDT folder spells it, e.g. `1001` or `g010`,
    /// or the long form `09-40-1`.
    pub fn code(self) -> String {
        if !self.has_short_form() {
            return format!(
                "{:02x}-{:02x}-{:x}",
                self.stage, self.room, self.player_flag
            );
        }
        format!(
            "{}{:02x}{:x}",
            self.stage_char(),
            self.room,
            self.player_flag
        )
    }

    /// Pack directory of the room, every player and visit of it, e.g.
    /// `room/100`.
    pub fn dir(self) -> String {
        format!("room/{}", self.room3().to_ascii_lowercase())
    }

    /// Pack directory of one RDT, e.g. `room/1001`: the layout of an RE2
    /// pack, whose players' rooms are each their own.
    pub fn rdt_dir(self) -> String {
        format!("room/{}", self.code())
    }

    /// The name of the player, `chris` or `jill`.
    pub fn player_tag(self) -> &'static str {
        if self.player_flag & 1 == 0 {
            "chris"
        } else {
            "jill"
        }
    }

    /// The name of this variant of the room: `jill-mansion2` for a mansion
    /// room, `jill` for another.
    pub fn variant_tag(self) -> String {
        if self.is_mansion() {
            format!(
                "{}-mansion{}",
                self.player_tag(),
                if self.mansion2 { 2 } else { 1 }
            )
        } else {
            self.player_tag().to_string()
        }
    }

    /// Pack path of a raw RDT of this variant of the room: an RDT is one
    /// variant's, so it is named for it ([`RoomId::variant_tag`]), e.g.
    /// `room/100/jill-mansion2.rdt`.
    pub fn rdt_entry(self) -> String {
        format!("{}/{}.rdt", self.dir(), self.variant_tag())
    }

    /// Pack path of the room's scripts, e.g. `room/100/room.scd`.
    pub fn scd_entry(self) -> String {
        format!("{}/room.scd", self.dir())
    }

    /// The room id without its player digit, e.g. `100`, `11C`, or the long
    /// form `09-40`. Its lower case names the room's folder.
    pub fn room3(self) -> String {
        if !self.has_short_form() {
            return format!("{:02X}-{:02X}", self.stage, self.room);
        }
        format!(
            "{}{:02X}",
            self.stage_char().to_ascii_uppercase(),
            self.room
        )
    }

    /// Pack stem of file `index` in the room's `kind` subdirectory, e.g.
    /// `room/100/cut/03`.
    fn file_stem(self, kind: &str, index: usize) -> String {
        format!("{}/{kind}/{index:02x}", self.dir())
    }

    /// Pack path of a camera background, e.g. `room/100/cut/00.bmp`.
    pub fn cut_entry(self, camera: usize) -> String {
        format!("{}.bmp", self.cut_stem(camera))
    }

    /// Pack stem of a camera background, e.g. `room/100/cut/00`. Every
    /// variant of a room shares its backgrounds.
    ///
    /// [`crate::pack::Pack::read_asset`] resolves the stem across replacement
    /// formats; the shipped entry name is [`RoomId::cut_entry`].
    pub fn cut_stem(self, camera: usize) -> String {
        self.file_stem("cut", camera)
    }

    /// Pack path of a camera's mask page, e.g. `room/100/mask/00.bmp`.
    pub fn mask_entry(self, camera: usize) -> String {
        format!("{}.bmp", self.mask_stem(camera))
    }

    /// Pack stem of a camera's mask page, e.g. `room/100/mask/00`.
    pub fn mask_stem(self, camera: usize) -> String {
        self.file_stem("mask", camera)
    }

    /// Zero-based stage index for stage-indexed tables; stage 0 (no room yet,
    /// as in a fresh default state) reads as the first.
    pub fn stage_index(self) -> u8 {
        self.stage.saturating_sub(1)
    }
}

/// Where each room of a pack was made from: a room's *origin* is the
/// original room a generated game's room is a copy of (`<room origin="10b">`
/// in its `room.xml`, [`crate::room::xml`]). A room without one is its own
/// origin, so a pack with no origins (every original game) maps each room to
/// itself.
///
/// Engine behaviour keyed to an original room (its quirks, its music and
/// sound table rows, its effect and light tables) reads the origin
/// ([`RoomOrigins::of`]); the room's files, its door targets, the save and
/// the debug menu use the room's own id. The second mansion visit is the
/// origin's: a room plays it when its origin is a mansion room
/// ([`RoomOrigins::with_visit`]).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RoomOrigins(std::sync::Arc<std::collections::BTreeMap<(u8, u8), (u8, u8)>>);

impl RoomOrigins {
    /// The origins of `pairs`: `(room, origin)` each as `(stage, room)`. A
    /// room that is its own origin is left out.
    pub fn new(pairs: impl IntoIterator<Item = ((u8, u8), (u8, u8))>) -> Self {
        Self(std::sync::Arc::new(
            pairs
                .into_iter()
                .filter(|(room, origin)| room != origin)
                .collect(),
        ))
    }

    /// Whether every room is its own origin.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The `(stage, room)` room `id` was made from: its own when it has no
    /// origin.
    pub fn origin_room(&self, id: RoomId) -> (u8, u8) {
        let own = (id.stage, id.room);
        self.0.get(&own).copied().unwrap_or(own)
    }

    /// Room `id` as its origin plays it ([`RoomId::as_origin`]); `id` itself
    /// for a room without an origin.
    pub fn of(&self, id: RoomId) -> RoomId {
        id.as_origin(self.origin_room(id))
    }

    /// Room `id` on the second mansion visit when `mansion2` and its origin
    /// is a mansion room; [`RoomId::with_visit`] for a room without an
    /// origin.
    ///
    /// A copy of a room only the second visit has always plays it, on any
    /// visit: the origin's first visit is a stub RDT without the doors, so a
    /// generated game that places the copy before the scenario flag would
    /// otherwise be left without them. (A retail room follows the flag; the
    /// original never leads to its first visit.)
    pub fn with_visit(&self, id: RoomId, mansion2: bool) -> RoomId {
        let origin = self.of(RoomId {
            mansion2: true,
            ..id
        });
        let copy = self.origin_room(id) != (id.stage, id.room);
        RoomId {
            mansion2: (mansion2 || (copy && origin.second_visit_only())) && origin.mansion2,
            ..id
        }
    }

    /// Every `(room, origin)` pair, in room order.
    pub fn pairs(&self) -> impl Iterator<Item = ((u8, u8), (u8, u8))> + '_ {
        self.0.iter().map(|(room, origin)| (*room, *origin))
    }
}

/// A door's destination given beside the room's scripts: `room.xml`'s
/// `<doors>` holds one `<door aot="1" target="09-40"/>` per door whose
/// destination the door's target byte cannot name (a generated game's
/// stages and rooms, [`RoomId::has_short_form`]).
///
/// The script's `door_aot_set` keeps its own bytes, target byte included,
/// so a retail room is unchanged and the door's animation still reads the
/// byte. When the room arms a door in AOT slot `aot` (and, if `next` is
/// given, with target byte `next`), the door leads to `target` instead of
/// where the byte points. The player digit is the player's and the visit
/// follows the scenario flag, as for any other door
/// ([`RoomOrigins::with_visit`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DoorTarget {
    /// The door's AOT slot, `door_aot_set`'s first operand.
    pub aot: u8,
    /// The target byte the script's door names, when only that door of the
    /// slot leads to `target` (a slot the script arms with different doors);
    /// `None` for any door in the slot.
    pub next: Option<u8>,
    /// The room the door leads to, as player 0 on the first visit.
    pub target: RoomId,
}

/// Raw RGBA8 image.
#[derive(Debug, Clone, Default)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Image {
    /// The frame `Bg_draw_2` draws: this picture moved down `offset` rows and
    /// `above` in the rows that leaves, ending on the row before this
    /// picture's first. A row neither covers is black.
    pub fn scrolled(&self, above: Option<&Image>, offset: i16) -> Image {
        let row_len = self.width as usize * 4;
        let height = self.height as i32;
        let mut rgba = vec![0; row_len * self.height as usize];
        for (row, out) in rgba.chunks_exact_mut(row_len).enumerate() {
            let from = row as i32 - i32::from(offset);
            let source = if from >= 0 { Some(self) } else { above };
            let from = if from >= 0 { from } else { from + height };
            if let Some(source) = source.filter(|_| (0..height).contains(&from)) {
                let start = from as usize * row_len;
                if let Some(pixels) = source.rgba.get(start..start + row_len) {
                    out.copy_from_slice(pixels);
                }
            }
            for alpha in out.iter_mut().skip(3).step_by(4) {
                *alpha = 255;
            }
        }
        Image {
            width: self.width,
            height: self.height,
            rgba,
        }
    }

    /// Re-interpret the linear pixel stream at a new width and height.
    ///
    /// The decoded background is a linear run of pixels; the TIM header's
    /// dimensions are not always the artwork's (the screen-border paks claim
    /// 320x240 for 316-wide rows), so a caller that knows the true geometry
    /// re-wraps the same stream.
    pub fn reslice(&mut self, width: u32, height: u32) {
        let pixels = (width as usize)
            .saturating_mul(height as usize)
            .min(self.rgba.len() / 4);
        self.rgba.truncate(pixels * 4);
        self.width = width;
        self.height = height;
    }
}

/// One room light. The RDT stores three of them.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Light {
    /// World-space position.
    pub pos: [i32; 3],
    /// Red, green and blue components.
    pub color: [u8; 3],
    /// Light type: 0 is a point light with radial falloff, anything else is
    /// directional.
    pub kind: u16,
    /// Falloff radius, used by point lights. The original consumes it as an
    /// unsigned 16-bit value, so `0xFFFF` is the widest radius, not -1.
    pub radius: u16,
    /// An RE2 light's factor on its direction (`Set_Light_data`), and that its
    /// point falloff is taken at `Set_light`'s sixteenth of the units.
    pub re2_gain: Option<u8>,
}

/// One camera's own lighting: an RE2 room lights each camera from its LIT
/// record instead of from the room-wide set. The values are on the RE1
/// room's scales (see [`crate::rdt2`]).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CameraLight {
    /// Ambient light colour, 12-bit per channel.
    pub ambient: [i16; 3],
    pub lights: [Light; 3],
}

/// One collision boundary rectangle, corners stored max-first.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CollisionRect {
    pub x_max: u16,
    pub z_max: u16,
    pub x_min: u16,
    pub z_min: u16,
    pub kind: u16,
    pub flags: u16,
}

/// Room collision boundaries, split into quadrants around `(cell_x, cell_z)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Collision {
    pub cell_x: i16,
    pub cell_z: i16,
    pub quadrants: [Vec<CollisionRect>; 4],
}

impl Default for Collision {
    fn default() -> Self {
        Self {
            cell_x: 0,
            cell_z: 0,
            quadrants: std::array::from_fn(|_| Vec::new()),
        }
    }
}

impl Collision {
    /// The records of the quadrant containing `(x, z)`.
    pub fn records(&self, x: i32, z: i32) -> &[CollisionRect] {
        let quadrant = (usize::from(z < i32::from(self.cell_z)) << 1)
            | usize::from(x < i32::from(self.cell_x));
        &self.quadrants[quadrant]
    }
}

/// RE2's room collision: the RDT's SCA section. RE2 records are signed
/// boxes with a shape, unlike RE1's quadrant-split rectangles, so they stay
/// in their own form; only the collision view reads them so far.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Sca {
    /// The header's centre point.
    pub cell_x: i16,
    pub cell_z: i16,
    /// The header's ceiling height.
    pub ceiling: i32,
    /// The header's last word (`0xC5C5C5C5` in every shipped room).
    pub tail: u32,
    pub records: Vec<ScaRect>,
}

/// One RE2 collision record: a box at `(x, z)` of `width` by `depth`, with
/// the raw shape/attribute words and the bit set of floors it applies on.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ScaRect {
    pub x: i16,
    pub z: i16,
    pub width: u16,
    pub depth: u16,
    /// Bits 0-3 are the shape ([`ScaRect::shape`]); the rest are raw.
    pub id: u16,
    pub kind: u16,
    pub floor: u32,
}

impl ScaRect {
    /// The record's shape: 0 a box, 1-4 the four right triangles, 5 a
    /// diamond, 6 a circle, 7 and up rounded boxes.
    pub fn shape(&self) -> u8 {
        (self.id & 0xF) as u8
    }

    /// `Sca_id_set`: a script gives the record new bits to block with, and the
    /// low seven, with its shape, stay.
    pub fn set_id(&mut self, id: u16) {
        self.id = (self.id & 0x7F) | (id & !0x7F);
    }
}

/// One of RE2's guide areas: a rectangle, and by bit those it opens onto.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Guide {
    pub x0: i16,
    pub z0: i16,
    pub x1: i16,
    pub z1: i16,
    pub links: u16,
}

/// One camera switch zone record. Every record is a zone; the first record of
/// each `cam_from` group is that group's header.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Zone {
    pub cam_to: i16,
    pub cam_from: i16,
    pub corners: [[i16; 2]; 4],
}

/// The gate on one RE2 camera switch zone: a zone only switches while its
/// flag is non-zero and the player is on its floor. Scripts rewrite both
/// (`cut_be_set`, `cut_replace`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZoneGate {
    pub flag: u8,
    /// The zone's floor; `0xFF` means every floor.
    pub floor: u8,
}

impl Default for ZoneGate {
    fn default() -> Self {
        Self {
            flag: 1,
            floor: 0xFF,
        }
    }
}

impl Zone {
    /// [`Zone::contains`] for RE2 (`Hit_ck_point4`), whose coordinates are
    /// signed.
    pub fn contains_signed(&self, x: i32, z: i32) -> bool {
        let corner = |index: usize| {
            [
                i64::from(self.corners[index][0]),
                i64::from(self.corners[index][1]),
            ]
        };
        let [x0, z0] = corner(0);
        let [x1, z1] = corner(1);
        let [x2, z2] = corner(2);
        let [x3, z3] = corner(3);
        let (x, z) = (i64::from(x), i64::from(z));
        let (dx, dz) = (x - x0, z - z0);
        if (x1 - x0) * dz > (z1 - z0) * dx || (x3 - x0) * dz < (z3 - z0) * dx {
            return false;
        }
        let (dx, dz) = (x - x2, z - z2);
        (x3 - x2) * dz <= (z3 - z2) * dx && (x1 - x2) * dz >= (z1 - z2) * dx
    }

    /// Whether `(x, z)` lies inside the quad.
    ///
    /// Faithful to the original: every corner is zero-extended to unsigned
    /// before the four cross-product edge tests, pivoting on corners 0 and 2.
    pub fn contains(&self, x: i32, z: i32) -> bool {
        let corner = |index: usize| {
            [
                i64::from(self.corners[index][0] as u16),
                i64::from(self.corners[index][1] as u16),
            ]
        };
        let [x0, z0] = corner(0);
        let [x1, z1] = corner(1);
        let [x2, z2] = corner(2);
        let [x3, z3] = corner(3);
        let x = i64::from(x);
        let z = i64::from(z);

        let dx = x - x0;
        let dz = z - z0;
        if (x1 - x0) * dz > (z1 - z0) * dx {
            return false;
        }
        if (x3 - x0) * dz < (z3 - z0) * dx {
            return false;
        }
        let dx2 = x - x2;
        let dz2 = z - z2;
        if (x1 - x2) * dz2 < (z1 - z2) * dx2 {
            return false;
        }
        if (x3 - x2) * dz2 > (z3 - z2) * dx2 {
            return false;
        }
        true
    }
}

/// One footstep sound zone from the RDT's `.flr` table (RE1 slot 5, RE2
/// slot 11).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FootstepZone {
    pub base_x: u16,
    pub base_z: u16,
    pub width: u16,
    pub height: u16,
    pub sound_data: u16,
    /// RE2 only (`Floor_check`): the low 7 bits are the floor level the zone
    /// answers, bit 7 switches it off (`Flr_set`). RE1 records have no such
    /// word and keep it 0.
    pub flags: u16,
}

/// One walkable zone used by NPC navigation.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WalkZone {
    pub x1: i16,
    pub z1: i16,
    pub x2: i16,
    pub z2: i16,
    pub field_08: u16,
    pub flags: u16,
}

impl WalkZone {
    /// Half-open containment: `x in [x1, x2)` and `z in [z1, z2)`, compared
    /// with the original's wrapping unsigned-short arithmetic.
    pub fn contains(&self, x: i32, z: i32) -> bool {
        let wrap = |value: i64| value as u16;
        wrap(i64::from(x) - i64::from(self.x1)) < wrap(i64::from(self.x2) - i64::from(self.x1))
            && wrap(i64::from(z) - i64::from(self.z1))
                < wrap(i64::from(self.z2) - i64::from(self.z1))
    }
}

/// One camera background plus its camera transform.
#[derive(Debug, Default, Clone)]
pub struct Cut {
    pub index: usize,
    pub pos: [i32; 3],
    pub look_at: [i32; 3],
    pub roll: i32,
    pub fov: i32,
    pub background: Option<Image>,
    /// RE2: the picture above the background, which `sce_scr_move` scrolls
    /// into view (`Bg_loadPC`'s `rc2000_` and `rc2168_`).
    pub scroll: Option<Image>,
    /// Number of groups in the camera's mask table.
    pub mask_group_count: u8,
    /// One bit per one-based group id: set while the group is visible.
    pub mask_active: u32,
    /// The camera's parsed mask sprites in group order.
    pub masks: Vec<MaskSprite>,
    /// The game whose rules order the mask sprites: RE1's bias tables and
    /// overrides, or RE2's plain depth ([`crate::mask::re2_sprite_key`]).
    pub mask_dialect: crate::format::Dialect,
    /// The camera's own lighting, which replaces the room's while it is the
    /// current cut; `None` lights it with the room's (every RE1 room).
    pub light: Option<CameraLight>,
}

/// The loaded room: its contents and the currently displayed cut.
#[derive(Debug, Default, Clone)]
pub struct RoomState {
    pub stage: u8,
    pub room: u8,
    pub player_flag: u8,
    /// The second visit of a mansion room ([`RoomId::mansion2`]).
    pub mansion2: bool,
    /// The `(stage, room)` the room was made from, when it is not its own
    /// ([`RoomOrigins`], [`RoomState::origin`]).
    pub origin: Option<(u8, u8)>,
    /// Where the room's doors lead beyond the targets their scripts name
    /// (`room.xml`'s `<doors>`).
    pub door_targets: Vec<DoorTarget>,
    /// Declared omodel pair count from header byte `0x02`; also the number of
    /// runtime object records the room's init script may build.
    pub omodel_slot_count: u8,
    /// Declared item-model pair count from header byte `0x03`.
    pub item_count: u8,
    pub cuts: Vec<Cut>,
    pub current_cut: usize,
    /// The cut whose mask groups were last rebuilt for the screen. `None`
    /// until the room's first camera goes up.
    pub mask_table_cut: Option<usize>,
    /// The live room-SFX pair index (`g_nextRoomSfxId`): 0 at boot, reloaded
    /// from every door record's sfx byte. Bank-0 `se_play_3d` resolves through
    /// this pair of [`crate::sfx::room_sfx`] entries.
    pub room_sfx: u8,
    /// Ambient light color, 12-bit per channel.
    pub ambient: [i16; 3],
    /// The room's three lights.
    pub lights: [Light; 3],
    /// Collision boundary records.
    pub collision: Collision,
    /// RE2 collision records, present exactly for an RE2 room.
    pub sca: Option<Sca>,
    /// Camera switch zones in file order, group headers included.
    pub zones: Vec<Zone>,
    /// RE2's gates on `zones`, one per zone; empty for an RE1 room.
    pub zone_gates: Vec<ZoneGate>,
    /// Walkable zones for NPC navigation.
    pub walk_zones: Vec<WalkZone>,
    /// Footstep sound zones, in file order.
    pub footstep_zones: Vec<FootstepZone>,
    /// RE2's guide areas, `Root_ck`'s map of the room.
    pub guides: Vec<Guide>,
    /// Raw RDT message block: a `u16` offset table followed by the encoded
    /// message streams. Absent when the RDT has no message pointer.
    pub messages: Option<Vec<u8>>,
    /// The room's declared effect sprites, parsed from RDT header pointer
    /// slots 13/14/15. Empty when the room declares none.
    pub effects: crate::effects::RoomEffects,
    /// Declared omodel `{TMD, TIM}` pairs (header byte `0x02`), parsed from
    /// pointer slot 2. Malformed pairs are skipped; the source pair index is
    /// kept on each asset.
    pub object_models: Vec<crate::objects::ObjectAsset>,
    /// Declared item-model `{TMD, TIM}` pairs (header byte `0x03`), parsed
    /// from pointer slot 3. Item models stay unbuilt this milestone.
    pub item_models: Vec<crate::objects::ObjectAsset>,
    /// Non-fatal problems from parsing the embedded model pairs: one entry
    /// per malformed (never per null) pair.
    pub model_warnings: Vec<String>,
    /// The RDT-embedded room player-animation pair (pointer slots 9/10): the
    /// push/vault/ladder clips the action behaviours play. `None` when the RDT
    /// declares no pair or it fails to parse; behaviours then fall back to the
    /// no-op stance.
    pub room_anim: Option<crate::model::RoomAnim>,
    /// RE2's room animations of the enemy slots that have their own.
    pub character_anims: Vec<(u8, crate::model::RoomAnim)>,
}

impl RoomState {
    /// `Hit_ck_point4` on the view quad `Ccut_searc` finds for the current cut.
    /// `None` if the cut has none.
    pub fn in_cut_view(&self, x: i32, z: i32) -> Option<bool> {
        let zone = self
            .zones
            .iter()
            .find(|zone| i32::from(zone.cam_from) == self.current_cut as i32)?;
        Some(zone.contains_signed(x, z))
    }

    /// The room's own id.
    pub fn id(&self) -> RoomId {
        RoomId {
            stage: self.stage,
            room: self.room,
            player_flag: self.player_flag,
            mansion2: self.mansion2,
        }
    }

    /// The room as its origin plays it ([`RoomOrigins::of`]): the variant
    /// of the original room it was made from, or its own id.
    pub fn origin(&self) -> RoomId {
        match self.origin {
            Some(origin) => self.id().as_origin(origin),
            None => self.id(),
        }
    }

    /// The row of the original's per-room tables the room reads
    /// ([`RoomId::table_row`]): its origin's.
    pub fn table_row(&self) -> usize {
        self.origin().table_row()
    }

    /// The packed footstep zone value for `(x, z)`: the surface type in the
    /// high byte and the sound offset for that surface in the low byte.
    ///
    /// The bounds test is the original's 32-bit unsigned subtraction: the
    /// sign-extended position minus the zero-extended base, compared against
    /// the width. It therefore does not wrap at 16 bits - a position below a
    /// base produces a huge value and falls through to the next zone. The scan
    /// stops after 256 entries; a room whose last zone is not a catch-all
    /// would otherwise run past the table, and returning `None` keeps that
    /// contained.
    pub fn footstep_zone(&self, x: i32, z: i32) -> Option<u16> {
        let x = x as u32;
        let z = z as u32;
        self.footstep_zones
            .iter()
            .take(256)
            .find(|zone| {
                x.wrapping_sub(u32::from(zone.base_x)) < u32::from(zone.width)
                    && z.wrapping_sub(u32::from(zone.base_z)) < u32::from(zone.height)
            })
            .map(|zone| ((zone.height >> 8) << 8) | (zone.sound_data & 0xFF))
    }
}

#[cfg(test)]
mod tests {
    fn column(rows: &[u8]) -> Image {
        Image {
            width: 1,
            height: rows.len() as u32,
            rgba: rows.iter().flat_map(|&row| [row, 0, 0, 255]).collect(),
        }
    }

    /// `Ccut_searc` stops at the first zone of the cut: its group's header.
    #[test]
    fn the_view_of_a_cut_is_the_header_of_its_group() {
        let zone = |cam_from, far| Zone {
            cam_to: 0,
            cam_from,
            corners: [[0, 0], [0, far], [far, far], [far, 0]],
        };
        let room = RoomState {
            zones: vec![zone(1, 10), zone(2, 100), zone(2, 1000)],
            current_cut: 2,
            ..Default::default()
        };
        assert_eq!(room.in_cut_view(50, 50), Some(true));
        assert_eq!(room.in_cut_view(500, 500), Some(false));
        let room = RoomState {
            current_cut: 3,
            ..room
        };
        assert_eq!(room.in_cut_view(50, 50), None);
    }

    #[test]
    fn a_script_changes_what_a_record_blocks_but_not_its_shape() {
        let mut record = ScaRect {
            x: 0,
            z: 0,
            width: 0,
            depth: 0,
            id: 0x0405,
            kind: 0,
            floor: 0,
        };
        record.set_id(0);
        assert_eq!(record.id, 5);
        record.set_id(0x1234);
        assert_eq!(record.id, 0x1205);
        assert_eq!(record.shape(), 5);
    }

    fn reds(image: &Image) -> Vec<u8> {
        image.rgba.iter().step_by(4).copied().collect()
    }

    #[test]
    fn a_scrolled_background_shows_the_picture_above_it() {
        let background = column(&[1, 2, 3]);
        let above = column(&[7, 8, 9]);
        assert_eq!(reds(&background.scrolled(Some(&above), 1)), [9, 1, 2]);
        assert_eq!(reds(&background.scrolled(Some(&above), 3)), [7, 8, 9]);
        // Nothing covers these rows.
        assert_eq!(reds(&background.scrolled(None, 2)), [0, 0, 1]);
        assert_eq!(reds(&background.scrolled(Some(&above), 5)), [0, 0, 7]);
    }

    use super::*;

    #[test]
    fn room_id_roundtrip() {
        let id = RoomId::from_room_and_player("100", 0).unwrap();
        assert_eq!(id.stage, 1);
        assert_eq!(id.room, 0x00);
        assert_eq!(id.rdt_number(), 0x1000);
        assert_eq!(id.dir(), "room/100");
        assert_eq!(id.rdt_dir(), "room/1000");
        assert_eq!(id.rdt_entry(), "room/100/chris-mansion1.rdt");
        assert_eq!(id.scd_entry(), "room/100/room.scd");
        assert_eq!(id.room3(), "100");
        assert_eq!(id.cut_entry(3), "room/100/cut/03.bmp");
        assert_eq!(id.mask_entry(12), "room/100/mask/0c.bmp");
        assert_eq!(id.stage_index(), 0);
        assert_eq!(id.table_row(), 0);

        let id = RoomId::from_room_and_player("205", 3).unwrap();
        assert_eq!(id.stage, 2);
        assert_eq!(id.room, 0x05);
        assert_eq!(id.rdt_number(), 0x2053);
        assert_eq!(id.room3(), "205");
    }

    #[test]
    fn the_second_mansion_visit_is_the_rooms_own_variant() {
        let id = RoomId::from_room_and_player("10b", 1)
            .unwrap()
            .with_visit(true);
        assert!(id.mansion2);
        assert_eq!(id.dir(), "room/10b");
        assert_eq!(id.variant_tag(), "jill-mansion2");
        assert_eq!(id.rdt_entry(), "room/10b/jill-mansion2.rdt");
        // The original's tables give it the second visit's row.
        assert_eq!(id.table_row(), 5);
        assert_eq!(id.original_number(), 0x60B1);
        assert_eq!(RoomId::from_original(0x60B1), Some(id));
        assert_eq!(RoomId::from_original(0x7031).unwrap().table_row(), 6);
        assert_eq!(
            RoomId::from_table_row(6, 3),
            RoomId::from_original(0x7030).unwrap()
        );
        assert_eq!(
            RoomId::from_table_row(2, 3),
            RoomId::from_original(0x3030).unwrap()
        );
        assert_eq!(RoomId::from_original(0x8000), None);
        // Only a mansion room has a second visit.
        let id = RoomId::from_room_and_player("305", 0).unwrap();
        assert!(!id.with_visit(true).mansion2);
        assert_eq!(id.rdt_entry(), "room/305/chris.rdt");
    }

    #[test]
    fn parses_four_digit_identities() {
        let id = RoomId::parse("1001").unwrap();
        assert_eq!(
            id,
            RoomId {
                stage: 1,
                room: 0x00,
                player_flag: 1,
                mansion2: false,
            }
        );
        assert_eq!(id.rdt_dir(), "room/1001");
        assert_eq!(id.room3(), "100");

        let id = RoomId::parse("11C0").unwrap();
        assert_eq!(
            id,
            RoomId {
                stage: 1,
                room: 0x1C,
                player_flag: 0,
                mansion2: false,
            }
        );
        assert_eq!(id.room3(), "11C");

        // A stage 7 room is RE2's own.
        let id = RoomId::parse("71A1").unwrap();
        assert_eq!(
            id,
            RoomId {
                stage: 7,
                room: 0x1A,
                player_flag: 1,
                mansion2: false,
            }
        );
        assert_eq!(id.rdt_dir(), "room/71a1");
        assert_eq!(id.room3(), "71A");
    }

    #[test]
    fn parses_letter_stages() {
        let id = RoomId::parse("G010").unwrap();
        assert_eq!((id.stage, id.room, id.player_flag), (16, 0x01, 0));
        assert_eq!(id.rdt_dir(), "room/g010");
        assert_eq!(id.dir(), "room/g01");
        assert_eq!(id.room3(), "G01");
        assert_eq!(id.code(), "g010");
        let id = RoomId::from_room_and_player("a1d", 1).unwrap();
        assert_eq!((id.stage, id.room, id.player_flag), (10, 0x1D, 1));
        assert_eq!(id.rdt_dir(), "room/a1d1");
        assert_eq!(RoomId::parse("G010").unwrap().rdt_number(), 0x1_0010);
        // Per-RDT folders keep their packed-number names.
        for stage in 1..=RoomId::MAX_STAGE {
            let id = RoomId {
                stage,
                room: 0x1C,
                player_flag: 1,
                mansion2: false,
            };
            assert_eq!(id.rdt_dir(), format!("room/{:04x}", id.rdt_number()));
            assert_eq!(id.code(), format!("{:04x}", id.rdt_number()));
        }
        // `G` is only a stage digit, and no stage lies past it.
        assert!(RoomId::parse("1G00").is_err());
        assert!(RoomId::parse("H000").is_err());
        assert!(RoomId::parse("0000").is_err());
    }

    #[test]
    fn every_shipped_room_id_roundtrips() {
        for stage in (1..=RoomId::MAX_STAGE).chain(10..=RoomId::MAX_STAGE_ID) {
            for room in 0..=RoomId::MAX_ROOM {
                for player in 0..=1u8 {
                    let id = RoomId {
                        stage,
                        room,
                        player_flag: player,
                        mansion2: false,
                    };
                    let text = id.code().to_ascii_uppercase();
                    assert_eq!(RoomId::parse(&text).unwrap(), id);
                    assert_eq!(
                        RoomId::from_room_and_player(&id.room3(), player).unwrap(),
                        id
                    );
                }
            }
        }
    }

    #[test]
    fn a_room_past_the_short_forms_has_the_long_form() {
        let id = RoomId {
            stage: 9,
            room: 0x40,
            player_flag: 1,
            mansion2: false,
        };
        assert!(!id.has_short_form());
        assert_eq!(id.code(), "09-40-1");
        assert_eq!(id.room3(), "09-40");
        assert_eq!(id.dir(), "room/09-40");
        assert_eq!(RoomId::parse("09-40-1").unwrap(), id);
        assert_eq!(
            RoomId::parse("09-40-1".to_ascii_uppercase().as_str()).unwrap(),
            id
        );
        assert_eq!(RoomId::from_room_and_player("09-40", 1).unwrap(), id);
        assert_eq!(
            RoomId::parse_room("09-40"),
            Some(RoomId {
                player_flag: 0,
                ..id
            })
        );
        // A short stage with a long room, and every byte.
        for (stage, room) in [(1, 0x20), (0x10, 0xFF), (0xFF, 0), (8, 0)] {
            let id = RoomId {
                stage,
                room,
                player_flag: 0,
                mansion2: false,
            };
            assert_eq!(RoomId::parse(&id.code()).unwrap(), id);
            assert_eq!(RoomId::parse(&id.code().to_ascii_uppercase()).unwrap(), id);
            assert_eq!(RoomId::from_room_and_player(&id.room3(), 0).unwrap(), id);
        }
        // The short forms are unchanged.
        assert_eq!(RoomId::parse("1061").unwrap().code(), "1061");
        assert_eq!(RoomId::parse("G1F0").unwrap().room3(), "G1F");
        assert_eq!(RoomId::parse_room("10B").unwrap().room3(), "10B");
        assert_eq!(RoomId::parse_room("1061"), None, "an RDT, not a room");
    }

    #[test]
    fn a_long_form_is_rejected_where_a_short_form_exists() {
        let error = RoomId::parse("01-00-0").unwrap_err().to_string();
        assert!(error.contains("short form, `1000`"), "{error}");
        assert!(RoomId::from_room_and_player("0a-1f", 0).is_err());
        assert!(RoomId::from_room_and_player("00-40", 0).is_err(), "stage 0");
        assert!(RoomId::from_room_and_player("9-40", 0).is_err());
        assert!(RoomId::from_room_and_player("09-40-0", 0).is_err());
        assert!(RoomId::parse("09-40").is_err(), "an RDT needs its player");
        assert!(RoomId::parse("09-40-x").is_err());
        assert!(RoomId::parse("09-4g-0").is_err());
    }

    #[test]
    fn only_original_rooms_have_a_table_slot() {
        let slot = |text| RoomId::parse_room(text).unwrap().table_slot();
        assert_eq!(slot("10B"), Some((0, 0x0B)));
        assert_eq!(slot("51F"), Some((4, 0x1F)));
        assert_eq!(slot("09-40"), None);
        assert_eq!(slot("01-20"), None);
        assert_eq!(slot("08-00"), None);
        let mansion2 = RoomId::parse_room("203").unwrap().with_visit(true);
        assert_eq!(mansion2.table_slot(), Some((6, 3)));
    }

    #[test]
    fn a_copy_of_a_second_visit_only_room_always_plays_the_second_visit() {
        // Room 110 has no first visit: its RDT there is a stub without doors.
        let origins = RoomOrigins::new([((1, 5), (1, 0x10)), ((9, 0x40), (1, 0x06))]);
        let copy = RoomId::parse("1050").unwrap();
        assert!(origins.with_visit(copy, false).mansion2);
        assert!(origins.with_visit(copy, true).mansion2);
        assert_eq!(origins.of(origins.with_visit(copy, false)).room3(), "110");
        // The room's own id is untouched, and a copy of any other room still
        // follows the scenario.
        assert_eq!((copy.stage, copy.room), (1, 5));
        assert!(
            !origins
                .with_visit(RoomId::parse_room("09-40").unwrap(), false)
                .mansion2
        );
        // A retail room follows the scenario, whatever visits it has.
        let retail = RoomId::parse("1100").unwrap();
        assert!(!origins.with_visit(retail, false).mansion2);
        assert!(origins.with_visit(retail, true).mansion2);
        assert!(
            !RoomOrigins::default()
                .with_visit(RoomId::parse("1060").unwrap(), false)
                .mansion2
        );
    }

    #[test]
    fn a_room_plays_as_its_origin() {
        let origins = RoomOrigins::new([
            ((9, 0x40), (1, 0x06)),
            ((3, 3), (3, 3)),
            ((9, 0x41), (3, 0x05)),
        ]);
        assert_eq!(
            origins.pairs().count(),
            2,
            "a room that is its own origin is left out"
        );
        let high = RoomId::from_room_and_player("09-40", 1).unwrap();
        let origin = origins.of(high);
        assert_eq!(origin, RoomId::parse("1061").unwrap());
        // The second visit follows a mansion origin.
        let visit = origins.with_visit(high, true);
        assert!(visit.mansion2);
        assert_eq!(
            (visit.stage, visit.room),
            (9, 0x40),
            "the room keeps its own id"
        );
        assert!(origins.of(visit).mansion2);
        assert_eq!(origins.of(visit).table_row(), 5);
        // A non-mansion origin has none.
        let lab = RoomId::from_room_and_player("09-41", 0).unwrap();
        assert!(!origins.with_visit(lab, true).mansion2);
        assert_eq!(origins.of(lab).room3(), "305");
        // A room without an origin is its own.
        let plain = RoomId::parse("2030").unwrap();
        assert_eq!(origins.of(plain), plain);
        assert_eq!(origins.with_visit(plain, true), plain.with_visit(true));
        assert_eq!(RoomOrigins::default().of(high), high);
        assert!(!RoomOrigins::default().with_visit(high, true).mansion2);
        // The room's state names it through the origin too.
        let room = RoomState {
            stage: 9,
            room: 0x40,
            origin: Some((1, 6)),
            ..Default::default()
        };
        assert_eq!(room.origin().room3(), "106");
        assert_eq!(room.table_row(), 0);
    }

    #[test]
    fn rejects_bad_identities() {
        assert!(RoomId::parse("").is_err());
        assert!(RoomId::parse("10012").is_err());
        assert!(RoomId::parse("0A0").is_err());
        assert!(RoomId::parse("800").is_err());
        assert!(RoomId::parse("900").is_err());
        assert!(RoomId::parse("H00").is_err());
        assert!(RoomId::parse("1FF").is_err());
        assert!(RoomId::parse("1X0").is_err());
        assert!(RoomId::from_room_and_player("1001", 0).is_err());
    }

    #[test]
    fn footstep_zone_does_not_wrap_the_coordinate_space() {
        let room = RoomState {
            footstep_zones: vec![
                FootstepZone {
                    base_x: 0xFF00,
                    base_z: 0xFFF0,
                    width: 0x0200,
                    height: 0x0020,
                    sound_data: 0x2D45,
                    flags: 0,
                },
                FootstepZone {
                    base_x: 0,
                    base_z: 0,
                    width: 0x7FE4,
                    height: 0x7FEC,
                    sound_data: 0,
                    flags: 0,
                },
            ],
            ..RoomState::default()
        };

        // A position inside the first zone matches it; the packed result keeps
        // the height's high byte and the low sound byte.
        assert_eq!(room.footstep_zone(0xFF00, 0xFFF0), Some(0x45));
        assert_eq!(room.footstep_zone(0xFF10, 0xFFF5), Some(0x45));
        // 0x0010 - 0xFF00 is a huge unsigned 32-bit value, not a 16-bit wrap
        // back to 0x0110, so the catch-all zone wins instead.
        assert_eq!(room.footstep_zone(0x10, 0x08), Some(0x7F00));
        assert_eq!(room.footstep_zone(0, 0), Some(0x7F00));
        assert_eq!(room.footstep_zone(0x1000, 0x1000), Some(0x7F00));
        assert_eq!(room.footstep_zone(0x7FE3, 0x7FEB), Some(0x7F00));
    }

    #[test]
    fn footstep_zone_stops_after_256_entries() {
        let room = RoomState {
            footstep_zones: (0..300)
                .map(|index| FootstepZone {
                    base_x: 1000 + index,
                    base_z: 1000 + index,
                    width: 1,
                    height: 1,
                    sound_data: index,
                    flags: 0,
                })
                .collect(),
            ..RoomState::default()
        };

        assert_eq!(room.footstep_zone(1000, 1000), Some(0));
        assert_eq!(room.footstep_zone(1255, 1255), Some(255));
        assert_eq!(room.footstep_zone(1299, 1299), None);
        assert_eq!(RoomState::default().footstep_zone(0, 0), None);
    }
}
