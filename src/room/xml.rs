//! `room/<id>/room.xml` and `room/<id>/effect/<slot>.xml`: writing a
//! [`RoomSource`] or an [`EffectSlot`] and reading one back.
//!
//! Neither names another file: the room's images, models, effect sheets,
//! animation and scripts sit in its folder under conventional names, and the
//! room's id is its folder's name.
//!
//! One `room.xml` defines every variant of a room. An element only some
//! variants have carries `when="..."`: space-separated terms, each `CHRIS`,
//! `JILL`, `MANSION1` or `MANSION2` with an optional `!`, that must all hold
//! for the variant read ([`parse_for`]). A room read for no variant
//! ([`parse`]) may not have any.
//!
//! `<slots>` names the file each model, effect and animation slot reads
//! where the variants differ ([`Slots`]): `<item when="JILL">03 01 02</item>`
//! gives Jill's item slots 0-2 the files `item/03`, `item/01` and `item/02`
//! (`-` for an empty slot). The first `<item>` that holds for the variant
//! wins, so one without `when` after the others is the room's default; a
//! kind with none reads every file of its folder, slot `n` from file `n`.
//!
//! Every integer attribute is read as decimal or `0x` hex; the writer uses
//! hex for the few raw words and decimal for everything else. The packed
//! flag words of the RDT (collision boxes, walk zones, mask sprites) are
//! spelled out as one attribute per field. Unknown elements and attributes
//! are refused, so a typo in a hand-edited room fails loudly instead of
//! silently zeroing a field.

use std::fmt::Write as _;

use anyhow::{Context, Result, bail};
use roxmltree::{Document, Node};

use super::source::{
    Camera, EffectSlot, EffectSprite, MaskGroup, MaskSprite, RoomSource, SlotFiles, Slots,
};
use crate::budget;
use crate::effects::room::{FrameEntry, UvRecord};
use crate::format::Dialect;
use crate::language::XmlString;
use crate::state::{
    CameraLight, Collision, CollisionRect, FootstepZone, Light, RoomId, Sca, ScaRect, WalkZone,
    Zone, ZoneGate,
};

/// The format version `<room format="...">` declares.
pub const FORMAT: u32 = 3;

/// The format before [`FORMAT`]: the same but for a `<messages>` block of the
/// RDT's encoded message text, which is still read and ignored, since a
/// room's text is its `<strings>`.
const FORMAT_WITH_MESSAGES: u32 = 2;

/// Element writer with two-space indentation.
struct Writer {
    out: String,
    depth: usize,
}

/// One attribute value as written.
enum Value {
    Dec(i64),
    Hex(u32),
    Str(String),
    Bool(bool),
}

impl Value {
    fn render(&self) -> String {
        match self {
            Self::Dec(value) => value.to_string(),
            Self::Hex(value) => format!("0x{value:04X}"),
            Self::Str(text) => escape(text, true),
            Self::Bool(value) => value.to_string(),
        }
    }
}

fn dec(value: impl Into<i64>) -> Value {
    Value::Dec(value.into())
}

fn hex(value: impl Into<u32>) -> Value {
    Value::Hex(value.into())
}

fn text(value: &str) -> Value {
    Value::Str(value.to_string())
}

/// Escape text for element content or (with `quote`) a quoted attribute.
fn escape(text: &str, quote: bool) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if quote => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}

impl Writer {
    fn tag(&mut self, name: &str, attributes: &[(&str, Value)], close: &str) {
        let _ = write!(self.out, "{:indent$}<{name}", "", indent = self.depth * 2);
        for (key, value) in attributes {
            let _ = write!(self.out, " {key}=\"{}\"", value.render());
        }
        self.out.push_str(close);
    }

    fn empty(&mut self, name: &str, attributes: &[(&str, Value)]) {
        self.tag(name, attributes, "/>\n");
    }

    fn open(&mut self, name: &str, attributes: &[(&str, Value)]) {
        self.tag(name, attributes, ">\n");
        self.depth += 1;
    }

    fn close(&mut self, name: &str) {
        self.depth -= 1;
        let _ = writeln!(self.out, "{:indent$}</{name}>", "", indent = self.depth * 2);
    }
}

/// Lower-case hex of `bytes`.
fn hex_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
}

/// The XML declaration every written file starts with.
const DECLARATION: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n";

/// Render `room` as `room.xml`.
///
/// Fails when a collision box's floor step has no `step` spelling (see
/// [`CollisionRect`]); no shipped room has one.
pub fn write(room: &RoomSource) -> Result<String> {
    let mut w = Writer {
        out: String::from(DECLARATION),
        depth: 0,
    };
    let mut attributes = vec![("format", dec(FORMAT))];
    if room.dialect != Dialect::Re1 {
        attributes.push(("game", Value::Str(room.dialect.name().to_string())));
    }
    if let Some(slots) = room.object_slots {
        attributes.push(("objectSlots", dec(slots as i64)));
    }
    if let Some(slots) = room.item_slots {
        attributes.push(("itemSlots", dec(slots as i64)));
    }
    w.open("room", &attributes);
    let [r, g, b] = room.ambient;
    w.empty("ambient", &[("r", dec(r)), ("g", dec(g)), ("b", dec(b))]);
    w.open("lights", &[]);
    for light in &room.lights {
        w.empty("light", &light_attributes(light));
    }
    w.close("lights");

    w.open("cameras", &[]);
    for camera in &room.cameras {
        write_camera(&mut w, camera, room.dialect);
    }
    w.close("cameras");

    write_zones(&mut w, &room.zones, &room.zone_gates);

    w.open(
        "collision",
        &[
            ("cellX", dec(room.collision.cell_x)),
            ("cellZ", dec(room.collision.cell_z)),
        ],
    );
    for (index, quadrant) in room.collision.quadrants.iter().enumerate() {
        w.open("quadrant", &[("index", dec(index as i64))]);
        for rect in quadrant {
            w.empty("box", &box_attributes(rect)?);
        }
        w.close("quadrant");
    }
    w.close("collision");
    if let Some(sca) = &room.sca {
        write_sca(&mut w, sca);
    }

    w.open("walkZones", &[]);
    for zone in &room.walk_zones {
        w.empty(
            "walk",
            &[
                ("x1", dec(zone.x1)),
                ("z1", dec(zone.z1)),
                ("x2", dec(zone.x2)),
                ("z2", dec(zone.z2)),
                ("field8", hex(zone.field_08)),
                ("links", text(&bit_list(zone.flags))),
            ],
        );
    }
    w.close("walkZones");

    w.open("footsteps", &[("count", dec(room.footstep_count))]);
    for zone in &room.footstep_zones {
        let mut attributes = vec![
            ("x", dec(zone.base_x)),
            ("z", dec(zone.base_z)),
            ("width", dec(zone.width)),
            // The high byte is the surface type.
            ("height", hex(zone.height)),
            ("sound", hex(zone.sound_data)),
        ];
        // RE2's level and switch-off word; RE1 records have none.
        if zone.flags != 0 {
            attributes.push(("flags", hex(zone.flags)));
        }
        w.empty("footstep", &attributes);
    }
    w.close("footsteps");

    write_slots(&mut w, &room.slots);
    write_strings(&mut w, &room.strings);
    w.close("room");
    Ok(w.out)
}

/// The slot lists the room names, as a `<slots>` block; nothing when it
/// names none.
fn write_slots(w: &mut Writer, slots: &Slots) {
    if Slots::KINDS.iter().all(|kind| slots.get(kind).is_none()) {
        return;
    }
    w.open("slots", &[]);
    for kind in Slots::KINDS {
        let Some(files) = slots.get(kind) else {
            continue;
        };
        let list = slot_list(files);
        let indent = w.depth * 2;
        if list.is_empty() {
            let _ = writeln!(w.out, "{:indent$}<{kind}/>", "");
        } else {
            let _ = writeln!(w.out, "{:indent$}<{kind}>{list}</{kind}>", "");
        }
    }
    w.close("slots");
}

/// `files` spelled as a `<slots>` list: two hex digits per file, `-` for an
/// empty slot.
fn slot_list(files: &[Option<u8>]) -> String {
    files
        .iter()
        .map(|file| file.map_or_else(|| "-".to_string(), |file| format!("{file:02x}")))
        .collect::<Vec<_>>()
        .join(" ")
}

/// A `<slots>` block: for each kind, the list of the first element that
/// holds for the variant being read.
fn parse_slots(node: Node<'_, '_>) -> Result<Slots> {
    check_attributes(node, &[])?;
    let mut slots = Slots::default();
    for child in elements(node) {
        let kind = child.tag_name().name();
        let Some(list) = slots.get_mut(kind) else {
            bail!("<slots> may only contain <object>, <item>, <effect> or <anim>, found <{kind}>");
        };
        check_attributes(child, &[])?;
        if list.is_some() {
            continue;
        }
        let text = child.text().unwrap_or("");
        let files = text
            .split_whitespace()
            .map(|word| match word {
                "-" => Ok(None),
                word if word.len() == 2 => u8::from_str_radix(word, 16)
                    .map(Some)
                    .with_context(|| format!("<{kind}>: `{word}` is not two hex digits or `-`")),
                word => bail!("<{kind}>: `{word}` is not two hex digits or `-`"),
            })
            .collect::<Result<SlotFiles>>()?;
        let most = match kind {
            "effect" => crate::effects::room::EFFECT_SPRITE_SLOTS,
            "anim" => 1,
            _ => usize::from(u8::MAX),
        };
        if files.len() > most {
            bail!("<{kind}> lists {} slots, more than {most}", files.len());
        }
        *list = Some(files);
    }
    Ok(slots)
}

/// The room's display strings as a `<strings>` block, one `<language>` per
/// tag in the options menu's order ([`crate::language::menu_rank`], then by
/// tag); nothing when there are none.
fn write_strings(w: &mut Writer, strings: &[XmlString]) {
    if strings.is_empty() {
        return;
    }
    let mut tags: Vec<&str> = Vec::new();
    for (tag, _, _) in strings {
        if !tags.contains(&tag.as_str()) {
            tags.push(tag);
        }
    }
    tags.sort_by(|a, b| {
        crate::language::menu_rank(a)
            .cmp(&crate::language::menu_rank(b))
            .then_with(|| a.cmp(b))
    });
    w.open("strings", &[]);
    for tag in tags {
        w.open("language", &[("lang", text(tag))]);
        for (_, key, value) in strings.iter().filter(|(t, _, _)| t == tag) {
            w.tag("string", &[("key", text(key))], ">");
            let _ = writeln!(w.out, "{}</string>", escape(value, false));
        }
        w.close("language");
    }
    w.close("strings");
}

/// The language-file key suffix a `room.xml` string key reads as: `name`
/// and `save_name` are the room's labels, a decimal message index (`0` up to
/// [`crate::text::message_index`]`(0xFFFF)`) is that room message's text,
/// `TEXT_XX`, and `<id>_speaker` or
/// `<id>_speaker_<page>` names the message's speaker for a subtitle,
/// `TEXT_XX_SPEAKER[_<page>]`. Numbers have no leading zeros.
pub fn string_suffix(key: &str) -> Option<String> {
    /// `text` as a decimal number without leading zeros.
    fn number<T: std::str::FromStr + ToString>(text: &str) -> Option<T> {
        let value: T = text.parse().ok()?;
        (value.to_string() == text).then_some(value)
    }
    match key {
        "name" => Some("NAME".to_string()),
        "save_name" => Some("NAME_SAVE".to_string()),
        _ => {
            let (id, speaker) = match key.split_once("_speaker") {
                Some((id, page)) => (id, Some(page)),
                None => (key, None),
            };
            let index: u16 = number(id)?;
            if index > crate::text::message_index(0xFFFF) {
                return None;
            }
            let text = format!("TEXT_{index:02X}");
            match speaker {
                None => Some(text),
                Some("") => Some(format!("{text}_SPEAKER")),
                Some(page) => {
                    let page: u32 = number(page.strip_prefix('_')?)?;
                    Some(format!("{text}_SPEAKER_{page}"))
                }
            }
        }
    }
}

/// The collision box shape byte (the type word's low byte).
const SHAPE_MASK: u16 = 0x00FF;
/// The floor step's tens (the type word's bits 8-14).
const STEP_TENS_MASK: u16 = 0x7F00;
/// Set when the floor step goes down (the type word's bit 15).
const STEP_DOWN_BIT: u16 = 0x8000;
/// The floor step's units (the collision word's low byte).
const STEP_UNITS_MASK: u16 = 0x00FF;
/// The four collision bits `inst_cfg` writes (`CF_1`, `CF_2`, `CF_4`,
/// `CF_8`): the collision word's bits 8-11.
const CF_SHIFT: u16 = 8;
/// The collision word's bits no attribute names.
const BOX_OTHER_MASK: u16 = 0xF000;
/// The attribute names of the four `CF_` bits, lowest first.
const CF_NAMES: [&str; 4] = ["cf1", "cf2", "cf4", "cf8"];

/// A collision box's attributes. The type word is the `shape` and the
/// floor step's tens and sign, the collision word the step's units, the
/// `CF_` bits and any bits left over. The step is written as the height the
/// engine moves by, `(tens * 10 + units) * 100`, signed by the down bit, so
/// it needs units below ten and no lone down bit.
fn box_attributes(rect: &CollisionRect) -> Result<Vec<(&'static str, Value)>> {
    let mut attributes = vec![
        ("xMax", dec(rect.x_max)),
        ("zMax", dec(rect.z_max)),
        ("xMin", dec(rect.x_min)),
        ("zMin", dec(rect.z_min)),
        ("shape", dec(rect.kind & SHAPE_MASK)),
    ];
    let tens = i64::from((rect.kind & STEP_TENS_MASK) >> 8);
    let units = i64::from(rect.flags & STEP_UNITS_MASK);
    let down = rect.kind & STEP_DOWN_BIT != 0;
    if units >= 10 || (down && tens == 0 && units == 0) {
        bail!(
            "collision box type 0x{:04X} and word 0x{:04X} have no `step` spelling",
            rect.kind,
            rect.flags
        );
    }
    let step = (tens * 10 + units) * 100;
    if step != 0 {
        attributes.push(("step", dec(if down { -step } else { step })));
    }
    for (bit, name) in CF_NAMES.iter().enumerate() {
        if rect.flags >> CF_SHIFT & (1 << bit) != 0 {
            attributes.push((name, Value::Bool(true)));
        }
    }
    if rect.flags & BOX_OTHER_MASK != 0 {
        attributes.push(("otherBits", hex(rect.flags & BOX_OTHER_MASK)));
    }
    Ok(attributes)
}

/// The set bits of `word` as space-separated bit numbers, lowest first.
fn bit_list(word: u16) -> String {
    (0..16)
        .filter(|bit| word & (1 << bit) != 0)
        .map(|bit| bit.to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Render one effect sprite slot as `effect/<slot>.xml`.
pub fn write_effect(effect: &EffectSlot) -> String {
    let mut w = Writer {
        out: String::from(DECLARATION),
        depth: 0,
    };
    write_effect_element(&mut w, effect);
    w.out
}

fn light_attributes(light: &Light) -> Vec<(&'static str, Value)> {
    vec![
        ("x", dec(light.pos[0])),
        ("y", dec(light.pos[1])),
        ("z", dec(light.pos[2])),
        ("r", dec(light.color[0])),
        ("g", dec(light.color[1])),
        ("b", dec(light.color[2])),
        ("kind", dec(light.kind)),
        // Read unsigned: 65535 is the widest radius.
        ("radius", dec(light.radius)),
    ]
    .into_iter()
    .chain(light.re2_gain.map(|gain| ("gain", dec(gain))))
    .collect()
}

fn write_camera(w: &mut Writer, camera: &Camera, dialect: Dialect) {
    let attributes = vec![
        ("x", dec(camera.pos[0])),
        ("y", dec(camera.pos[1])),
        ("z", dec(camera.pos[2])),
        ("lookX", dec(camera.look_at[0])),
        ("lookY", dec(camera.look_at[1])),
        ("lookZ", dec(camera.look_at[2])),
        ("roll", dec(camera.roll)),
        ("fov", dec(camera.fov)),
    ];
    if camera.mask_groups.is_empty() && camera.light.is_none() {
        w.empty("camera", &attributes);
        return;
    }
    w.open("camera", &attributes);
    // An RE2 camera lights the room itself, in place of the room's lights.
    if let Some(light) = &camera.light {
        let [r, g, b] = light.ambient;
        w.open("lighting", &[("r", dec(r)), ("g", dec(g)), ("b", dec(b))]);
        for light in &light.lights {
            w.empty("light", &light_attributes(light));
        }
        w.close("lighting");
    }
    for group in &camera.mask_groups {
        w.open(
            "maskGroup",
            &[
                ("originX", dec(group.origin.0)),
                ("originY", dec(group.origin.1)),
                ("texBits", hex(group.tex_bits)),
            ],
        );
        for sprite in &group.sprites {
            let attributes = match dialect {
                Dialect::Re1 => sprite_attributes(sprite),
                Dialect::Re2 => re2_sprite_attributes(sprite),
            };
            w.empty("sprite", &attributes);
        }
        w.close("maskGroup");
    }
    w.close("camera");
}

/// The mask sprite word's texture page (bits 0-4).
const SPRITE_PAGE_MASK: u16 = 0x001F;
/// The mask sprite word's semi-transparency mode (bits 5-6).
const SPRITE_BLEND_SHIFT: u16 = 5;
/// The mask sprite word's texture colour mode (bits 7-8).
const SPRITE_COLOR_SHIFT: u16 = 7;
/// The mask sprite word's bits no attribute names (bits 9-11).
const SPRITE_OTHER_MASK: u16 = 0x0E00;
/// The mask sprite word's packed square size, in units of 8 (bits 12-15).
const SPRITE_SIZE_SHIFT: u16 = 12;

/// A mask sprite's attributes: the sprite word split into its texture
/// `page`, `blend` (semi-transparency) and `colorMode` fields, any other
/// bits, and the size as one square `size` when the word packs it or an
/// explicit `width` and `height` when it does not.
fn sprite_attributes(sprite: &MaskSprite) -> Vec<(&'static str, Value)> {
    let mut attributes = vec![
        ("u", dec(sprite.uv.0)),
        ("v", dec(sprite.uv.1)),
        ("x", dec(sprite.offset.0)),
        ("y", dec(sprite.offset.1)),
        ("depthData", dec(sprite.depth)),
    ];
    let page = sprite.flags & SPRITE_PAGE_MASK;
    if page != 0 {
        attributes.push(("page", dec(page)));
    }
    attributes.push(("blend", dec(sprite.flags >> SPRITE_BLEND_SHIFT & 3)));
    attributes.push(("colorMode", dec(sprite.flags >> SPRITE_COLOR_SHIFT & 3)));
    if sprite.flags & SPRITE_OTHER_MASK != 0 {
        attributes.push(("otherBits", hex(sprite.flags & SPRITE_OTHER_MASK)));
    }
    match sprite.size {
        Some((width, height)) => {
            attributes.push(("width", dec(width)));
            attributes.push(("height", dec(height)));
        }
        None => attributes.push(("size", dec((sprite.flags >> SPRITE_SIZE_SHIFT) * 8))),
    }
    attributes
}

/// An RE2 mask sprite's attributes: its size word is the side of a square
/// `size`, or 0 for an explicit `width` and `height`.
fn re2_sprite_attributes(sprite: &MaskSprite) -> Vec<(&'static str, Value)> {
    let mut attributes = vec![
        ("u", dec(sprite.uv.0)),
        ("v", dec(sprite.uv.1)),
        ("x", dec(sprite.offset.0)),
        ("y", dec(sprite.offset.1)),
        ("depthData", dec(sprite.depth)),
    ];
    match sprite.size {
        Some((width, height)) => {
            attributes.push(("width", dec(width)));
            attributes.push(("height", dec(height)));
        }
        None => attributes.push(("size", dec(sprite.flags))),
    }
    attributes
}

/// RE2's collision records, raw words in hex.
fn write_sca(w: &mut Writer, sca: &Sca) {
    w.open(
        "sca",
        &[
            ("cellX", dec(sca.cell_x)),
            ("cellZ", dec(sca.cell_z)),
            ("ceiling", dec(sca.ceiling)),
            ("tail", hex(sca.tail)),
        ],
    );
    for rect in &sca.records {
        w.empty(
            "record",
            &[
                ("x", dec(rect.x)),
                ("z", dec(rect.z)),
                ("width", dec(rect.width)),
                ("depth", dec(rect.depth)),
                ("id", hex(rect.id)),
                ("kind", hex(rect.kind)),
                ("floor", hex(rect.floor)),
            ],
        );
    }
    w.close("sca");
}

/// The switch zones grouped by source camera: each group's first record is
/// its `<view>` header (the culling quad), the rest are `<switch>` records.
/// Every record keeps its flat index, the one SCD `cut_zone_set` addresses.
fn write_zones(w: &mut Writer, zones: &[Zone], gates: &[ZoneGate]) {
    w.open("cameraSwitches", &[]);
    let mut index = 0;
    while index < zones.len() {
        let from = zones[index].cam_from;
        let end = zones[index..]
            .iter()
            .position(|zone| zone.cam_from != from)
            .map_or(zones.len(), |length| index + length);
        w.open("switches", &[("from", dec(from))]);
        for (offset, zone) in zones[index..end].iter().enumerate() {
            let mut attributes = vec![
                ("zoneIndex", dec((index + offset) as i64)),
                ("to", dec(zone.cam_to)),
            ];
            let names = [["x0", "z0"], ["x1", "z1"], ["x2", "z2"], ["x3", "z3"]];
            for (corner, [x, z]) in zone.corners.iter().zip(names) {
                // The corners are zero-extended when tested.
                attributes.push((x, dec(corner[0] as u16)));
                attributes.push((z, dec(corner[1] as u16)));
            }
            if let Some(gate) = gates.get(index + offset) {
                attributes.push(("flag", dec(gate.flag)));
                attributes.push(("floor", dec(gate.floor)));
            }
            w.empty(if offset == 0 { "view" } else { "switch" }, &attributes);
        }
        w.close("switches");
        index = end;
    }
    w.close("cameraSwitches");
}

fn write_effect_element(w: &mut Writer, effect: &EffectSlot) {
    let mut attributes = vec![("index", dec(effect.index))];
    let Some(sprite) = &effect.sprite else {
        w.empty("effect", &attributes);
        return;
    };
    attributes.push(("clut", hex(sprite.page_clut_word)));
    let table: Vec<String> = sprite.depth_table.iter().map(u8::to_string).collect();
    attributes.push(("depthTable", text(&table.join(" "))));
    w.open("effect", &attributes);
    for frame in &sprite.frames {
        w.empty(
            "frame",
            &[
                ("uv", dec(frame.uv_index)),
                ("delay", dec(frame.delay)),
                ("width", dec(frame.width)),
                ("height", dec(frame.height)),
            ],
        );
    }
    for uv in &sprite.uvs {
        w.empty(
            "uv",
            &[
                ("u", dec(uv.u)),
                ("v", dec(uv.v)),
                ("pivotX", dec(uv.pivot_x)),
                ("pivotY", dec(uv.pivot_y)),
            ],
        );
    }
    for (row, frames) in sprite.depth_frames.iter().enumerate() {
        if frames.is_empty() {
            continue;
        }
        w.open("depth", &[("row", dec(row as i64))]);
        for frame in frames {
            w.open("animFrame", &[]);
            for block in &frame.blocks {
                w.tag("block", &[], ">");
                w.out.push_str(&hex_bytes(&block.0));
                w.out.push_str("</block>\n");
            }
            w.close("animFrame");
        }
        w.close("depth");
    }
    w.close("effect");
}

/// The largest room XML read. The shipped rooms are well under 1 MiB.
const MAX_XML_BYTES: usize = 4 << 20;

thread_local! {
    /// The room variant a `when` condition is tested against while one
    /// room XML is read ([`parse_for`]).
    static VARIANT: std::cell::Cell<Option<RoomId>> = const { std::cell::Cell::new(None) };
}

/// Whether the terms of a `when` condition all hold for room variant `id`;
/// an error names a term that is not a variant.
pub fn condition_holds(condition: &str, id: RoomId) -> Result<bool> {
    let mut holds = true;
    for term in condition.split_whitespace() {
        let (negated, name) = match term.strip_prefix('!') {
            Some(name) => (true, name),
            None => (false, term),
        };
        let value = match name {
            "CHRIS" => id.player_flag & 1 == 0,
            "JILL" => id.player_flag & 1 == 1,
            "MANSION1" => id.is_mansion() && !id.mansion2,
            "MANSION2" => id.is_mansion() && id.mansion2,
            _ => bail!(
                "`when` term `{term}` is not CHRIS, JILL, MANSION1 or MANSION2 (or `!` before one)"
            ),
        };
        holds &= value != negated;
    }
    Ok(holds)
}

/// Whether `node` is in the variant being read: it has no `when`, or its
/// condition holds. (The conditions are checked before reading.)
fn active(node: &Node<'_, '_>) -> bool {
    match (node.attribute("when"), VARIANT.with(std::cell::Cell::get)) {
        (None, _) => true,
        (Some(condition), Some(id)) => condition_holds(condition, id).unwrap_or(false),
        (Some(_), None) => false,
    }
}

/// Check every `when` of `document`: a valid condition, and only where the
/// room is read for a variant.
fn check_conditions(document: &Document<'_>, id: Option<RoomId>) -> Result<()> {
    for node in document.descendants().filter(Node::is_element) {
        let Some(condition) = node.attribute("when") else {
            continue;
        };
        let name = node.tag_name().name();
        let Some(id) = id else {
            bail!("<{name} when=\"{condition}\"> needs a room variant to read for");
        };
        condition_holds(condition, id).with_context(|| format!("<{name}>"))?;
    }
    Ok(())
}

/// Parse `room.xml`, a room definition, for no particular variant: it may
/// not hold `when` conditions.
pub fn parse(text: &str) -> Result<RoomSource> {
    parse_variant(text, None)
}

/// Parse `room.xml`, a room definition, for room variant `id`: an element
/// whose `when` does not hold for it is left out.
pub fn parse_for(text: &str, id: RoomId) -> Result<RoomSource> {
    parse_variant(text, Some(id))
}

fn parse_variant(text: &str, id: Option<RoomId>) -> Result<RoomSource> {
    budget::check_len(text.len(), MAX_XML_BYTES, "room XML size")?;
    let document = Document::parse(text).context("room XML is not well-formed")?;
    check_conditions(&document, id)?;
    let previous = VARIANT.with(|variant| variant.replace(id));
    let parsed = parse_document(&document);
    VARIANT.with(|variant| variant.set(previous));
    parsed
}

fn parse_document(document: &Document<'_>) -> Result<RoomSource> {
    let root = document.root_element();
    if root.attribute("when").is_some() {
        bail!("<room> may not have a `when`");
    }
    if root.tag_name().name() != "room" {
        bail!(
            "room XML root is <{}>, expected <room>",
            root.tag_name().name()
        );
    }
    check_attributes(root, &["format", "game", "objectSlots", "itemSlots"])?;
    let format: u32 = attr_or(root, "format", FORMAT)?;
    if format != FORMAT && format != FORMAT_WITH_MESSAGES {
        bail!("room XML format {format} is not supported (expected {FORMAT})");
    }

    let mut room = RoomSource {
        object_slots: optional_attr::<u8>(root, "objectSlots")?.map(usize::from),
        item_slots: optional_attr::<u8>(root, "itemSlots")?.map(usize::from),
        dialect: match root.attribute("game") {
            None | Some("re1") => Dialect::Re1,
            Some("re2") => Dialect::Re2,
            Some(other) => bail!("<room game=\"{other}\"> is not `re1` or `re2`"),
        },
        ..RoomSource::default()
    };
    let mut seen = Vec::new();
    for node in elements(root) {
        let name = node.tag_name().name();
        if seen.contains(&name) {
            bail!("<room> has more than one <{name}>");
        }
        seen.push(name);
        match name {
            "ambient" => {
                check_attributes(node, &["r", "g", "b"])?;
                room.ambient = [attr(node, "r")?, attr(node, "g")?, attr(node, "b")?];
            }
            "lights" => {
                check_attributes(node, &[])?;
                let lights = children(node, "light")?
                    .map(parse_light)
                    .collect::<Result<Vec<_>>>()?;
                room.lights = lights.try_into().map_err(|lights: Vec<Light>| {
                    anyhow::anyhow!("<lights> has {} <light>, expected 3", lights.len())
                })?;
            }
            "cameras" => {
                check_attributes(node, &[])?;
                room.cameras = children(node, "camera")?
                    .map(|camera| parse_camera(camera, room.dialect))
                    .collect::<Result<_>>()?;
            }
            "cameraSwitches" => (room.zones, room.zone_gates) = parse_zones(node)?,
            "collision" => room.collision = parse_collision(node)?,
            "sca" => room.sca = Some(parse_sca(node)?),
            "walkZones" => {
                check_attributes(node, &[])?;
                room.walk_zones = children(node, "walk")?
                    .map(|walk| {
                        check_attributes(walk, &["x1", "z1", "x2", "z2", "field8", "links"])?;
                        Ok(WalkZone {
                            x1: attr(walk, "x1")?,
                            z1: attr(walk, "z1")?,
                            x2: attr(walk, "x2")?,
                            z2: attr(walk, "z2")?,
                            field_08: attr_or(walk, "field8", 0)?,
                            flags: parse_bit_list(walk, "links")?,
                        })
                    })
                    .collect::<Result<_>>()?;
            }
            "footsteps" => {
                check_attributes(node, &["count"])?;
                room.footstep_zones = children(node, "footstep")?
                    .map(|zone| {
                        check_attributes(zone, &["x", "z", "width", "height", "sound", "flags"])?;
                        Ok(FootstepZone {
                            base_x: attr(zone, "x")?,
                            base_z: attr(zone, "z")?,
                            width: attr(zone, "width")?,
                            height: attr(zone, "height")?,
                            sound_data: attr(zone, "sound")?,
                            flags: attr_or(zone, "flags", 0)?,
                        })
                    })
                    .collect::<Result<_>>()?;
                room.footstep_count = attr_or(node, "count", room.footstep_zones.len() as u16)?;
            }
            "slots" => room.slots = parse_slots(node)?,
            "strings" => room.strings = parse_strings(node)?,
            "messages" if format == FORMAT_WITH_MESSAGES => {}
            other => bail!("unknown room element <{other}>"),
        }
    }
    Ok(room)
}

/// A `<strings>` block: the room's `name`, `save_name`, message texts and
/// speaker names per language, for the variant being read: a string whose
/// `when` holds replaces the room's own.
pub(crate) fn parse_strings(node: Node<'_, '_>) -> Result<Vec<XmlString>> {
    let variant = VARIANT.with(std::cell::Cell::get);
    let strings = conditional_strings(node)?;
    // The most specific string of each key whose condition holds.
    let mut chosen: Vec<(XmlString, usize)> = Vec::new();
    for (tag, key, value, condition) in strings {
        let rank = match (&condition, variant) {
            (None, _) => 0,
            (Some(condition), Some(id)) if condition_holds(condition, id)? => {
                condition.split_whitespace().count()
            }
            _ => continue,
        };
        match chosen
            .iter_mut()
            .find(|((t, k, _), _)| *t == tag && *k == key)
        {
            Some(entry) if entry.1 <= rank => *entry = ((tag, key, value), rank),
            Some(_) => {}
            None => chosen.push(((tag, key, value), rank)),
        }
    }
    Ok(chosen.into_iter().map(|(string, _)| string).collect())
}

/// One `<string>` of a room, with its `when` condition if it has one.
pub type ConditionalString = crate::language::ConditionalXmlString;

/// Every `<string>` of a `<strings>` block, with its `when` condition.
pub(crate) fn conditional_strings(node: Node<'_, '_>) -> Result<Vec<ConditionalString>> {
    crate::language::parse_xml_strings_when(node, |key, condition| {
        let tag = match condition {
            None => String::new(),
            Some(condition) => format!("{}_", string_variant(condition)?),
        };
        Ok(string_suffix(key).map(|suffix| format!("ROOM_100_{tag}{suffix}")))
    })
    .context("<strings>")
}

/// The key tag a string's `when` gives its language key: the variant terms
/// in order, player first, joined by `_` (`JILL_MANSION2`). Only plain
/// terms name a string's variant.
pub fn string_variant(condition: &str) -> Result<String> {
    let mut terms: Vec<&str> = condition.split_whitespace().collect();
    for term in &terms {
        if !matches!(*term, "CHRIS" | "JILL" | "MANSION1" | "MANSION2") {
            bail!("a string's `when` term must be CHRIS, JILL, MANSION1 or MANSION2, not `{term}`");
        }
    }
    terms.sort_by_key(|term| term.starts_with("MANSION"));
    terms.dedup();
    if terms.is_empty() || terms.len() > 2 || (terms.len() == 2 && terms[0] == terms[1]) {
        bail!("a string's `when` names one player and one visit at most, not `{condition}`");
    }
    Ok(terms.join("_"))
}

/// The key tags of the strings room variant `id` reads, most specific
/// first, then the room's own (`""`): for a mansion room `JILL_MANSION2`,
/// `JILL`, `MANSION2`, for any other room `JILL`.
pub fn string_variants(id: RoomId) -> Vec<String> {
    let player = if id.player_flag & 1 == 0 {
        "CHRIS"
    } else {
        "JILL"
    };
    let mut tags = Vec::new();
    if id.is_mansion() {
        let visit = if id.mansion2 { "MANSION2" } else { "MANSION1" };
        tags.push(format!("{player}_{visit}"));
        tags.push(player.to_string());
        tags.push(visit.to_string());
    } else {
        tags.push(player.to_string());
    }
    tags.push(String::new());
    tags
}

/// The element children of `node` in the variant being read.
fn elements<'a, 'input>(node: Node<'a, 'input>) -> impl Iterator<Item = Node<'a, 'input>> {
    node.children()
        .filter(Node::is_element)
        .filter(|node| active(node))
}

/// The element children of `node`, all of which must be `<name>`.
fn children<'a, 'input>(
    node: Node<'a, 'input>,
    name: &'static str,
) -> Result<impl Iterator<Item = Node<'a, 'input>>> {
    if let Some(other) = elements(node).find(|child| child.tag_name().name() != name) {
        bail!(
            "<{}> may only contain <{name}>, found <{}>",
            node.tag_name().name(),
            other.tag_name().name()
        );
    }
    let count = elements(node).count();
    budget::check_len(count, budget::MAX_RECORDS, "room XML element count")?;
    Ok(elements(node))
}

/// Refuse any attribute of `node` not in `known` (or its `when`).
fn check_attributes(node: Node<'_, '_>, known: &[&str]) -> Result<()> {
    for attribute in node.attributes() {
        if !known.contains(&attribute.name()) && attribute.name() != "when" {
            bail!(
                "<{}> has an unknown attribute `{}`",
                node.tag_name().name(),
                attribute.name()
            );
        }
    }
    Ok(())
}

fn attr_text<'a>(node: Node<'a, '_>, name: &str) -> Result<&'a str> {
    node.attribute(name)
        .with_context(|| format!("<{}> needs a `{name}` attribute", node.tag_name().name()))
}

/// Parse a decimal or `0x` hex integer.
fn integer(text: &str) -> Option<i64> {
    let text = text.trim();
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let value = match digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        Some(hex) => i64::from_str_radix(hex, 16).ok()?,
        None => digits.parse().ok()?,
    };
    Some(if negative { -value } else { value })
}

fn attr<T: TryFrom<i64>>(node: Node<'_, '_>, name: &str) -> Result<T> {
    let raw = attr_text(node, name)?;
    integer(raw)
        .and_then(|value| T::try_from(value).ok())
        .with_context(|| {
            format!(
                "<{}> `{name}` value `{raw}` is not a valid {}",
                node.tag_name().name(),
                std::any::type_name::<T>()
            )
        })
}

fn attr_or<T: TryFrom<i64>>(node: Node<'_, '_>, name: &str, default: T) -> Result<T> {
    Ok(optional_attr(node, name)?.unwrap_or(default))
}

fn optional_attr<T: TryFrom<i64>>(node: Node<'_, '_>, name: &str) -> Result<Option<T>> {
    node.attribute(name).map(|_| attr(node, name)).transpose()
}

/// A `true`/`false` (or `1`/`0`) attribute, false when absent.
fn flag(node: Node<'_, '_>, name: &str) -> Result<bool> {
    match node.attribute(name) {
        None | Some("false" | "0") => Ok(false),
        Some("true" | "1") => Ok(true),
        Some(other) => bail!(
            "<{}> `{name}` value `{other}` is not true or false",
            node.tag_name().name()
        ),
    }
}

/// A word from a space-separated list of its set bit numbers (0-15).
fn parse_bit_list(node: Node<'_, '_>, name: &str) -> Result<u16> {
    let mut word = 0u16;
    for item in node.attribute(name).unwrap_or("").split_ascii_whitespace() {
        let bit = integer(item)
            .filter(|bit| (0..16).contains(bit))
            .with_context(|| {
                format!(
                    "<{}> `{name}` item `{item}` is not a number 0-15",
                    node.tag_name().name()
                )
            })?;
        word |= 1 << bit;
    }
    Ok(word)
}

fn parse_light(node: Node<'_, '_>) -> Result<Light> {
    check_attributes(
        node,
        &["x", "y", "z", "r", "g", "b", "kind", "radius", "gain"],
    )?;
    let radius: i64 = attr(node, "radius")?;
    if !(i64::from(i16::MIN)..=i64::from(u16::MAX)).contains(&radius) {
        bail!("<light> radius {radius} does not fit 16 bits");
    }
    Ok(Light {
        pos: [attr(node, "x")?, attr(node, "y")?, attr(node, "z")?],
        color: [attr(node, "r")?, attr(node, "g")?, attr(node, "b")?],
        kind: attr(node, "kind")?,
        radius: radius as u16,
        re2_gain: optional_attr(node, "gain")?,
    })
}

fn parse_camera(node: Node<'_, '_>, dialect: Dialect) -> Result<Camera> {
    check_attributes(
        node,
        &["x", "y", "z", "lookX", "lookY", "lookZ", "roll", "fov"],
    )?;
    let mut light = None;
    let mut groups = Vec::new();
    for child in elements(node) {
        match child.tag_name().name() {
            "lighting" if light.is_none() && groups.is_empty() => {
                light = Some(parse_camera_light(child)?)
            }
            "maskGroup" => groups.push(child),
            other => {
                bail!("<camera> may only contain one <lighting> then <maskGroup>, found <{other}>")
            }
        }
    }
    budget::check_len(groups.len(), budget::MAX_RECORDS, "room XML element count")?;
    let mask_groups = groups
        .into_iter()
        .map(|group| {
            check_attributes(group, &["originX", "originY", "texBits"])?;
            let sprites = children(group, "sprite")?
                .map(|sprite| match dialect {
                    Dialect::Re1 => parse_sprite(sprite),
                    Dialect::Re2 => parse_re2_sprite(sprite),
                })
                .collect::<Result<_>>()?;
            Ok(MaskGroup {
                tex_bits: attr_or(group, "texBits", 0)?,
                origin: (attr(group, "originX")?, attr(group, "originY")?),
                sprites,
            })
        })
        .collect::<Result<_>>()?;
    Ok(Camera {
        pos: [attr(node, "x")?, attr(node, "y")?, attr(node, "z")?],
        look_at: [
            attr(node, "lookX")?,
            attr(node, "lookY")?,
            attr(node, "lookZ")?,
        ],
        roll: attr_or(node, "roll", 0)?,
        fov: attr(node, "fov")?,
        mask_groups,
        light,
    })
}

/// A camera's `<lighting>`: its ambient and three lights.
fn parse_camera_light(node: Node<'_, '_>) -> Result<CameraLight> {
    check_attributes(node, &["r", "g", "b"])?;
    let lights = children(node, "light")?
        .map(parse_light)
        .collect::<Result<Vec<_>>>()?;
    Ok(CameraLight {
        ambient: [attr(node, "r")?, attr(node, "g")?, attr(node, "b")?],
        lights: lights.try_into().map_err(|lights: Vec<Light>| {
            anyhow::anyhow!("<lighting> has {} <light>, expected 3", lights.len())
        })?,
    })
}

fn parse_sprite(sprite: Node<'_, '_>) -> Result<MaskSprite> {
    check_attributes(
        sprite,
        &[
            "u",
            "v",
            "x",
            "y",
            "depthData",
            "page",
            "blend",
            "colorMode",
            "otherBits",
            "size",
            "width",
            "height",
        ],
    )?;
    let page: u16 = attr_or(sprite, "page", 0)?;
    let blend: u16 = attr_or(sprite, "blend", 0)?;
    let color: u16 = attr_or(sprite, "colorMode", 0)?;
    let other: u16 = attr_or(sprite, "otherBits", 0)?;
    if page > SPRITE_PAGE_MASK || blend > 3 || color > 3 || other & !SPRITE_OTHER_MASK != 0 {
        bail!(
            "<sprite> needs `page` 0-31, `blend` and `colorMode` 0-3 and `otherBits` \
             within 0x{SPRITE_OTHER_MASK:04X}"
        );
    }
    let mut flags = page | blend << SPRITE_BLEND_SHIFT | color << SPRITE_COLOR_SHIFT | other;
    let size = match (
        sprite.attribute("size"),
        sprite.attribute("width"),
        sprite.attribute("height"),
    ) {
        (Some(_), None, None) => {
            let size: u16 = attr(sprite, "size")?;
            if size == 0 || !size.is_multiple_of(8) || size > 15 * 8 {
                bail!("<sprite> size {size} is not a multiple of 8 from 8 to 120");
            }
            flags |= (size / 8) << SPRITE_SIZE_SHIFT;
            None
        }
        (None, Some(_), Some(_)) => Some((attr(sprite, "width")?, attr(sprite, "height")?)),
        _ => bail!("<sprite> needs a `size`, or a `width` and a `height`"),
    };
    Ok(MaskSprite {
        uv: (attr(sprite, "u")?, attr(sprite, "v")?),
        offset: (attr(sprite, "x")?, attr(sprite, "y")?),
        depth: attr(sprite, "depthData")?,
        flags,
        size,
    })
}

/// An RE2 `<sprite>` ([`re2_sprite_attributes`]).
fn parse_re2_sprite(sprite: Node<'_, '_>) -> Result<MaskSprite> {
    check_attributes(
        sprite,
        &["u", "v", "x", "y", "depthData", "size", "width", "height"],
    )?;
    let (flags, size) = match (
        sprite.attribute("size"),
        sprite.attribute("width"),
        sprite.attribute("height"),
    ) {
        (Some(_), None, None) => {
            let size: u16 = attr(sprite, "size")?;
            if size == 0 {
                bail!("<sprite> size must not be 0; give a `width` and a `height` instead");
            }
            (size, None)
        }
        (None, Some(_), Some(_)) => (0, Some((attr(sprite, "width")?, attr(sprite, "height")?))),
        _ => bail!("<sprite> needs a `size`, or a `width` and a `height`"),
    };
    Ok(MaskSprite {
        uv: (attr(sprite, "u")?, attr(sprite, "v")?),
        offset: (attr(sprite, "x")?, attr(sprite, "y")?),
        depth: attr(sprite, "depthData")?,
        flags,
        size,
    })
}

fn parse_zones(node: Node<'_, '_>) -> Result<(Vec<Zone>, Vec<ZoneGate>)> {
    check_attributes(node, &[])?;
    let mut zones: Vec<(usize, Zone)> = Vec::new();
    let mut gates: Vec<(usize, ZoneGate)> = Vec::new();
    for group in children(node, "switches")? {
        check_attributes(group, &["from"])?;
        let from: i16 = attr(group, "from")?;
        // The RDTs store the camera in a byte; RE2 rooms have up to 17.
        if !(0..=0xFF).contains(&from) {
            bail!("<switches> from camera {from} is not 0-255");
        }
        for (position, record) in elements(group).enumerate() {
            let expected = if position == 0 { "view" } else { "switch" };
            if record.tag_name().name() != expected {
                bail!(
                    "<switches from=\"{from}\"> record {position} is <{}>, expected <{expected}>",
                    record.tag_name().name()
                );
            }
            check_attributes(
                record,
                &[
                    "zoneIndex",
                    "to",
                    "x0",
                    "z0",
                    "x1",
                    "z1",
                    "x2",
                    "z2",
                    "x3",
                    "z3",
                    "flag",
                    "floor",
                ],
            )?;
            if record.has_attribute("flag") {
                gates.push((
                    attr::<usize>(record, "zoneIndex")?,
                    ZoneGate {
                        flag: attr(record, "flag")?,
                        floor: attr_or(record, "floor", 0xFF)?,
                    },
                ));
            }
            let corner = |x: &str, z: &str| -> Result<[i16; 2]> {
                Ok([
                    attr::<u16>(record, x)? as i16,
                    attr::<u16>(record, z)? as i16,
                ])
            };
            zones.push((
                attr(record, "zoneIndex")?,
                Zone {
                    cam_to: attr(record, "to")?,
                    cam_from: from,
                    corners: [
                        corner("x0", "z0")?,
                        corner("x1", "z1")?,
                        corner("x2", "z2")?,
                        corner("x3", "z3")?,
                    ],
                },
            ));
            budget::check_len(zones.len(), budget::MAX_RECORDS, "camera switch zone count")?;
        }
    }
    zones.sort_by_key(|(index, _)| *index);
    for (expected, (index, _)) in zones.iter().enumerate() {
        if *index != expected {
            bail!(
                "camera switch zoneIndex values must be 0..{} with no gaps or repeats",
                zones.len()
            );
        }
    }
    if !gates.is_empty() && gates.len() != zones.len() {
        bail!("every camera switch zone needs a gate, or none of them");
    }
    gates.sort_by_key(|(index, _)| *index);
    Ok((
        zones.into_iter().map(|(_, zone)| zone).collect(),
        gates.into_iter().map(|(_, gate)| gate).collect(),
    ))
}

fn parse_collision(node: Node<'_, '_>) -> Result<Collision> {
    check_attributes(node, &["cellX", "cellZ"])?;
    let mut collision = Collision {
        cell_x: attr(node, "cellX")?,
        cell_z: attr(node, "cellZ")?,
        ..Collision::default()
    };
    let mut seen = [false; 4];
    for quadrant in children(node, "quadrant")? {
        check_attributes(quadrant, &["index"])?;
        let index: usize = attr(quadrant, "index")?;
        if index >= 4 || std::mem::replace(&mut seen[index], true) {
            bail!("<quadrant> index {index} is out of range or repeated");
        }
        collision.quadrants[index] = children(quadrant, "box")?
            .map(parse_box)
            .collect::<Result<_>>()?;
    }
    Ok(collision)
}

fn parse_sca(node: Node<'_, '_>) -> Result<Sca> {
    check_attributes(node, &["cellX", "cellZ", "ceiling", "tail"])?;
    Ok(Sca {
        cell_x: attr(node, "cellX")?,
        cell_z: attr(node, "cellZ")?,
        ceiling: attr_or(node, "ceiling", 0)?,
        tail: attr_or(node, "tail", 0)?,
        records: children(node, "record")?
            .map(|rect| {
                check_attributes(rect, &["x", "z", "width", "depth", "id", "kind", "floor"])?;
                Ok(ScaRect {
                    x: attr(rect, "x")?,
                    z: attr(rect, "z")?,
                    width: attr(rect, "width")?,
                    depth: attr(rect, "depth")?,
                    id: attr_or(rect, "id", 0)?,
                    kind: attr_or(rect, "kind", 0)?,
                    floor: attr_or(rect, "floor", 0)?,
                })
            })
            .collect::<Result<_>>()?,
    })
}

fn parse_box(rect: Node<'_, '_>) -> Result<CollisionRect> {
    let mut known = vec!["xMax", "zMax", "xMin", "zMin", "shape", "step", "otherBits"];
    known.extend(CF_NAMES);
    check_attributes(rect, &known)?;
    let shape: u16 = attr_or(rect, "shape", 0)?;
    if shape > SHAPE_MASK {
        bail!("<box> shape {shape} does not fit a byte");
    }
    let step: i64 = attr_or(rect, "step", 0)?;
    let size = step.unsigned_abs();
    let (tens, units) = (size / 1000, size / 100 % 10);
    if !size.is_multiple_of(100) || tens > 0x7F {
        bail!("<box> step {step} is not a multiple of 100 within 127900 either way");
    }
    let mut kind = shape | (tens as u16) << 8;
    if step < 0 {
        kind |= STEP_DOWN_BIT;
    }
    let mut flags = units as u16;
    for (bit, name) in CF_NAMES.iter().enumerate() {
        if flag(rect, name)? {
            flags |= 1 << (CF_SHIFT + bit as u16);
        }
    }
    let other: u16 = attr_or(rect, "otherBits", 0)?;
    if other & !BOX_OTHER_MASK != 0 {
        bail!("<box> otherBits 0x{other:04X} is outside 0x{BOX_OTHER_MASK:04X}");
    }
    Ok(CollisionRect {
        x_max: attr(rect, "xMax")?,
        z_max: attr(rect, "zMax")?,
        x_min: attr(rect, "xMin")?,
        z_min: attr(rect, "zMin")?,
        kind,
        flags: flags | other,
    })
}

/// Decode a lower- or upper-case hex string.
fn unhex(text: &str) -> Result<Vec<u8>> {
    let digits: Vec<u8> = text
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    if !digits.iter().all(u8::is_ascii_hexdigit) {
        bail!("hex data has a non-hex digit");
    }
    if !digits.len().is_multiple_of(2) {
        bail!("hex data has an odd number of digits");
    }
    budget::check_len(
        digits.len() / 2,
        budget::MAX_DECODE_ALLOC,
        "room XML hex data",
    )?;
    let nibble = |digit: u8| (digit as char).to_digit(16).unwrap_or(0) as u8;
    Ok(digits
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&[high, low]| nibble(high) << 4 | nibble(low))
        .collect())
}

/// Parse `effect/<slot>.xml`, one effect sprite slot.
pub fn parse_effect(text: &str) -> Result<EffectSlot> {
    budget::check_len(text.len(), MAX_XML_BYTES, "effect XML size")?;
    let document = Document::parse(text).context("effect XML is not well-formed")?;
    let effect = document.root_element();
    if effect.tag_name().name() != "effect" {
        bail!(
            "effect XML root is <{}>, expected <effect>",
            effect.tag_name().name()
        );
    }
    check_attributes(effect, &["index", "clut", "depthTable"])?;
    let index: u8 = attr(effect, "index")?;
    if index == 0xFF {
        bail!("<effect> index 255 marks a free slot and cannot be declared");
    }
    if effect.attribute("depthTable").is_none() && elements(effect).next().is_none() {
        return Ok(EffectSlot {
            index,
            sprite: None,
        });
    }
    let mut depth_table = [0u8; 8];
    let values: Vec<&str> = attr_text(effect, "depthTable")?
        .split_ascii_whitespace()
        .collect();
    if values.len() != 8 {
        bail!(
            "<effect> depthTable needs eight values, found {}",
            values.len()
        );
    }
    for (entry, value) in depth_table.iter_mut().zip(values) {
        *entry = integer(value)
            .and_then(|value| u8::try_from(value).ok())
            .with_context(|| format!("<effect> depthTable value `{value}` is not a byte"))?;
    }
    let mut sprite = EffectSprite {
        page_clut_word: attr_or(effect, "clut", 0)?,
        frames: Vec::new(),
        uvs: Vec::new(),
        depth_table,
        depth_frames: Default::default(),
    };
    let mut total_blocks = 0usize;
    for child in elements(effect) {
        match child.tag_name().name() {
            "frame" => {
                check_attributes(child, &["uv", "delay", "width", "height"])?;
                sprite.frames.push(FrameEntry {
                    uv_index: attr(child, "uv")?,
                    delay: attr(child, "delay")?,
                    width: attr(child, "width")?,
                    height: attr(child, "height")?,
                });
            }
            "uv" => {
                check_attributes(child, &["u", "v", "pivotX", "pivotY"])?;
                sprite.uvs.push(UvRecord {
                    u: attr(child, "u")?,
                    v: attr(child, "v")?,
                    pivot_x: attr(child, "pivotX")?,
                    pivot_y: attr(child, "pivotY")?,
                });
            }
            "depth" => {
                check_attributes(child, &["row"])?;
                let row: usize = attr(child, "row")?;
                if row >= 8 || !sprite.depth_frames[row].is_empty() {
                    bail!("<depth> row {row} is out of range or repeated");
                }
                for frame in children(child, "animFrame")? {
                    check_attributes(frame, &[])?;
                    let mut blocks = Vec::new();
                    for block in children(frame, "block")? {
                        let bytes = unhex(block.text().unwrap_or(""))?;
                        let block: [u8; 24] = bytes.try_into().map_err(|bytes: Vec<u8>| {
                            anyhow::anyhow!("<block> is {} bytes, expected 24", bytes.len())
                        })?;
                        blocks.push(block);
                    }
                    total_blocks += blocks.len();
                    budget::check_len(total_blocks, budget::MAX_RECORDS, "effect block total")?;
                    sprite.depth_frames[row].push(super::source::anim_frame(blocks));
                }
            }
            other => bail!("unknown <effect> element <{other}>"),
        }
    }
    if sprite.uvs.len() > usize::from(u8::MAX) || sprite.frames.len() > usize::from(u16::MAX) {
        bail!("<effect> has too many frames or UV records");
    }
    Ok(EffectSlot {
        index,
        sprite: Some(sprite),
    })
}
