//! `room/<id>.xml`: writing a [`RoomSource`] and reading one back.
//!
//! Every integer attribute is read as decimal or `0x` hex; the writer uses
//! hex for raw flag and shape words and decimal for coordinates and counts.
//! Unknown elements and attributes are refused, so a typo in a hand-edited
//! room fails loudly instead of silently zeroing a field.

use std::fmt::Write as _;

use anyhow::{Context, Result, bail};
use roxmltree::{Document, Node};

use super::markup::{self, Piece};
use super::source::{
    Camera, EffectSlot, EffectSprite, MaskGroup, MaskSprite, Messages, RoomSource,
};
use crate::budget;
use crate::charset::Script;
use crate::effects::room::{FrameEntry, UvRecord};
use crate::state::{Collision, CollisionRect, FootstepZone, Light, RoomId, WalkZone, Zone};

/// The format version `<room format="...">` declares.
pub const FORMAT: u32 = 1;

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
}

impl Value {
    fn render(&self) -> String {
        match self {
            Self::Dec(value) => value.to_string(),
            Self::Hex(value) => format!("0x{value:04X}"),
            Self::Str(text) => escape(text, true),
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

/// The script name a `<messages>` element declares.
fn script_name(script: Script) -> &'static str {
    match script {
        Script::Japanese => "japanese",
        Script::Latin => "latin",
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

/// Render `room` as `room/<id>.xml`.
pub fn write(room: &RoomSource, id: RoomId) -> String {
    let mut w = Writer {
        out: String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"),
        depth: 0,
    };
    w.open(
        "room",
        &[
            ("id", text(&format!("{:04X}", id.rdt_number()))),
            ("format", dec(FORMAT)),
        ],
    );
    let [r, g, b] = room.ambient;
    w.empty("ambient", &[("r", dec(r)), ("g", dec(g)), ("b", dec(b))]);
    w.open("lights", &[]);
    for light in &room.lights {
        w.empty(
            "light",
            &[
                ("x", dec(light.pos[0])),
                ("y", dec(light.pos[1])),
                ("z", dec(light.pos[2])),
                ("r", dec(light.color[0])),
                ("g", dec(light.color[1])),
                ("b", dec(light.color[2])),
                ("kind", dec(light.kind)),
                // Read unsigned: 65535 is the widest radius.
                ("radius", dec(light.radius)),
            ],
        );
    }
    w.close("lights");

    w.open("cameras", &[]);
    for camera in &room.cameras {
        write_camera(&mut w, camera);
    }
    w.close("cameras");

    write_zones(&mut w, &room.zones);

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
            w.empty(
                "box",
                &[
                    ("xMax", dec(rect.x_max)),
                    ("zMax", dec(rect.z_max)),
                    ("xMin", dec(rect.x_min)),
                    ("zMin", dec(rect.z_min)),
                    ("kind", hex(rect.kind)),
                    ("flags", hex(rect.flags)),
                ],
            );
        }
        w.close("quadrant");
    }
    w.close("collision");

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
                ("flags", hex(zone.flags)),
            ],
        );
    }
    w.close("walkZones");

    w.open("footsteps", &[("count", dec(room.footstep_count))]);
    for zone in &room.footstep_zones {
        w.empty(
            "footstep",
            &[
                ("x", dec(zone.base_x)),
                ("z", dec(zone.base_z)),
                ("width", dec(zone.width)),
                // The high byte is the surface type.
                ("height", hex(zone.height)),
                ("sound", hex(zone.sound_data)),
            ],
        );
    }
    w.close("footsteps");

    match &room.messages {
        None => {}
        Some(Messages::Raw(block)) => w.empty("messages", &[("raw", text(&hex_bytes(block)))]),
        Some(Messages::Streams { script, streams }) => {
            w.open("messages", &[("script", text(script_name(*script)))]);
            for (index, stream) in streams.iter().enumerate() {
                write_message(&mut w, index, stream, *script);
            }
            w.close("messages");
        }
    }

    w.open("effects", &[]);
    for (slot, effect) in room.effects.iter().enumerate() {
        write_effect(&mut w, slot, effect);
    }
    w.close("effects");

    for (table, element, slots) in [
        ("objectModels", "objectModel", &room.object_models),
        ("itemModels", "itemModel", &room.item_models),
    ] {
        w.open(table, &[]);
        for (slot, model) in slots.iter().enumerate() {
            let mut attributes = vec![("slot", dec(slot as i64))];
            if let Some(model) = model {
                attributes.push(("model", text(model)));
            }
            w.empty(element, &attributes);
        }
        w.close(table);
    }

    if let Some(stem) = &room.player_animation {
        w.empty("playerAnimation", &[("src", text(stem))]);
    }
    if let Some(entry) = &room.scripts {
        w.empty("scripts", &[("src", text(entry))]);
    }
    w.close("room");
    w.out
}

fn write_camera(w: &mut Writer, camera: &Camera) {
    let mut attributes = vec![
        ("x", dec(camera.pos[0])),
        ("y", dec(camera.pos[1])),
        ("z", dec(camera.pos[2])),
        ("lookX", dec(camera.look_at[0])),
        ("lookY", dec(camera.look_at[1])),
        ("lookZ", dec(camera.look_at[2])),
        ("roll", dec(camera.roll)),
        ("fov", dec(camera.fov)),
    ];
    if let Some(stem) = &camera.background {
        attributes.push(("background", text(stem)));
    }
    if let Some(stem) = &camera.mask {
        attributes.push(("mask", text(stem)));
    }
    if camera.mask_groups.is_empty() {
        w.empty("camera", &attributes);
        return;
    }
    w.open("camera", &attributes);
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
            let mut attributes = vec![
                ("u", dec(sprite.uv.0)),
                ("v", dec(sprite.uv.1)),
                ("x", dec(sprite.offset.0)),
                ("y", dec(sprite.offset.1)),
                ("depthData", dec(sprite.depth)),
                ("flags", hex(sprite.flags)),
            ];
            if let Some((width, height)) = sprite.size {
                attributes.push(("width", dec(width)));
                attributes.push(("height", dec(height)));
            }
            w.empty("sprite", &attributes);
        }
        w.close("maskGroup");
    }
    w.close("camera");
}

/// The switch zones grouped by source camera: each group's first record is
/// its `<view>` header (the culling quad), the rest are `<switch>` records.
/// Every record keeps its flat index, the one SCD `cut_zone_set` addresses.
fn write_zones(w: &mut Writer, zones: &[Zone]) {
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
            w.empty(if offset == 0 { "view" } else { "switch" }, &attributes);
        }
        w.close("switches");
        index = end;
    }
    w.close("cameraSwitches");
}

fn write_message(w: &mut Writer, index: usize, stream: &[u8], script: Script) {
    let Some(pieces) = markup::encode(stream, script) else {
        w.empty(
            "message",
            &[("id", dec(index as i64)), ("raw", text(&hex_bytes(stream)))],
        );
        return;
    };
    w.tag("message", &[("id", dec(index as i64))], ">");
    for piece in &pieces {
        match piece {
            Piece::Text(run) => w.out.push_str(&escape(run, false)),
            Piece::Tag(name, values) => {
                let _ = write!(w.out, "<{name}");
                for (key, value) in values {
                    if *name == "glyph" && *key == "page" || *name == "byte" {
                        let _ = write!(w.out, " {key}=\"0x{value:02X}\"");
                    } else {
                        let _ = write!(w.out, " {key}=\"{value}\"");
                    }
                }
                w.out.push_str("/>");
            }
        }
    }
    w.out.push_str("</message>\n");
}

fn write_effect(w: &mut Writer, slot: usize, effect: &EffectSlot) {
    let mut attributes = vec![("slot", dec(slot as i64)), ("index", dec(effect.index))];
    let Some(sprite) = &effect.sprite else {
        w.empty("effect", &attributes);
        return;
    };
    if let Some(sheet) = &sprite.sheet {
        attributes.push(("sheet", text(sheet)));
    }
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

/// Parse `room/<id>.xml`, the definition of room `id`.
pub fn parse(text: &str, id: RoomId) -> Result<RoomSource> {
    budget::check_len(text.len(), MAX_XML_BYTES, "room XML size")?;
    let document = Document::parse(text).context("room XML is not well-formed")?;
    let root = document.root_element();
    if root.tag_name().name() != "room" {
        bail!(
            "room XML root is <{}>, expected <room>",
            root.tag_name().name()
        );
    }
    check_attributes(root, &["id", "format"])?;
    if let Some(named) = root.attribute("id")
        && RoomId::parse(named).ok() != Some(id)
    {
        bail!("<room> id `{named}` is not room {:04X}", id.rdt_number());
    }
    let format: u32 = attr_or(root, "format", FORMAT)?;
    if format != FORMAT {
        bail!("room XML format {format} is not supported (expected {FORMAT})");
    }

    let mut room = RoomSource::default();
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
                    .map(parse_camera)
                    .collect::<Result<_>>()?;
            }
            "cameraSwitches" => room.zones = parse_zones(node)?,
            "collision" => room.collision = parse_collision(node)?,
            "walkZones" => {
                check_attributes(node, &[])?;
                room.walk_zones = children(node, "walk")?
                    .map(|walk| {
                        check_attributes(walk, &["x1", "z1", "x2", "z2", "field8", "flags"])?;
                        Ok(WalkZone {
                            x1: attr(walk, "x1")?,
                            z1: attr(walk, "z1")?,
                            x2: attr(walk, "x2")?,
                            z2: attr(walk, "z2")?,
                            field_08: attr_or(walk, "field8", 0)?,
                            flags: attr_or(walk, "flags", 0)?,
                        })
                    })
                    .collect::<Result<_>>()?;
            }
            "footsteps" => {
                check_attributes(node, &["count"])?;
                room.footstep_zones = children(node, "footstep")?
                    .map(|zone| {
                        check_attributes(zone, &["x", "z", "width", "height", "sound"])?;
                        Ok(FootstepZone {
                            base_x: attr(zone, "x")?,
                            base_z: attr(zone, "z")?,
                            width: attr(zone, "width")?,
                            height: attr(zone, "height")?,
                            sound_data: attr(zone, "sound")?,
                        })
                    })
                    .collect::<Result<_>>()?;
                room.footstep_count = attr_or(node, "count", room.footstep_zones.len() as u16)?;
            }
            "messages" => room.messages = Some(parse_messages(node)?),
            "effects" => room.effects = parse_effects(node)?,
            "objectModels" => room.object_models = parse_models(node, "objectModel")?,
            "itemModels" => room.item_models = parse_models(node, "itemModel")?,
            "playerAnimation" => {
                check_attributes(node, &["src"])?;
                room.player_animation = Some(attr_text(node, "src")?.to_string());
            }
            "scripts" => {
                check_attributes(node, &["src"])?;
                room.scripts = Some(attr_text(node, "src")?.to_string());
            }
            other => bail!("unknown room element <{other}>"),
        }
    }
    Ok(room)
}

/// The element children of `node`.
fn elements<'a, 'input>(node: Node<'a, 'input>) -> impl Iterator<Item = Node<'a, 'input>> {
    node.children().filter(Node::is_element)
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

/// Refuse any attribute of `node` not in `known`.
fn check_attributes(node: Node<'_, '_>, known: &[&str]) -> Result<()> {
    for attribute in node.attributes() {
        if !known.contains(&attribute.name()) {
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
    if node.attribute(name).is_some() {
        attr(node, name)
    } else {
        Ok(default)
    }
}

fn parse_light(node: Node<'_, '_>) -> Result<Light> {
    check_attributes(node, &["x", "y", "z", "r", "g", "b", "kind", "radius"])?;
    let radius: i64 = attr(node, "radius")?;
    if !(i64::from(i16::MIN)..=i64::from(u16::MAX)).contains(&radius) {
        bail!("<light> radius {radius} does not fit 16 bits");
    }
    Ok(Light {
        pos: [attr(node, "x")?, attr(node, "y")?, attr(node, "z")?],
        color: [attr(node, "r")?, attr(node, "g")?, attr(node, "b")?],
        kind: attr(node, "kind")?,
        radius: radius as u16,
    })
}

fn parse_camera(node: Node<'_, '_>) -> Result<Camera> {
    check_attributes(
        node,
        &[
            "x",
            "y",
            "z",
            "lookX",
            "lookY",
            "lookZ",
            "roll",
            "fov",
            "background",
            "mask",
        ],
    )?;
    let mask_groups = children(node, "maskGroup")?
        .map(|group| {
            check_attributes(group, &["originX", "originY", "texBits"])?;
            let sprites = children(group, "sprite")?
                .map(|sprite| {
                    check_attributes(
                        sprite,
                        &["u", "v", "x", "y", "depthData", "flags", "width", "height"],
                    )?;
                    let size = match (sprite.attribute("width"), sprite.attribute("height")) {
                        (None, None) => None,
                        (Some(_), Some(_)) => {
                            Some((attr(sprite, "width")?, attr(sprite, "height")?))
                        }
                        _ => bail!("<sprite> needs both `width` and `height` or neither"),
                    };
                    Ok(MaskSprite {
                        uv: (attr(sprite, "u")?, attr(sprite, "v")?),
                        offset: (attr(sprite, "x")?, attr(sprite, "y")?),
                        depth: attr(sprite, "depthData")?,
                        flags: attr(sprite, "flags")?,
                        size,
                    })
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
        background: node.attribute("background").map(str::to_string),
        mask: node.attribute("mask").map(str::to_string),
        mask_groups,
    })
}

fn parse_zones(node: Node<'_, '_>) -> Result<Vec<Zone>> {
    check_attributes(node, &[])?;
    let mut zones: Vec<(usize, Zone)> = Vec::new();
    for group in children(node, "switches")? {
        check_attributes(group, &["from"])?;
        let from: i16 = attr(group, "from")?;
        if !(0..=7).contains(&from) {
            bail!("<switches> from camera {from} is not 0-7");
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
                ],
            )?;
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
    Ok(zones.into_iter().map(|(_, zone)| zone).collect())
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
            .map(|rect| {
                check_attributes(rect, &["xMax", "zMax", "xMin", "zMin", "kind", "flags"])?;
                Ok(CollisionRect {
                    x_max: attr(rect, "xMax")?,
                    z_max: attr(rect, "zMax")?,
                    x_min: attr(rect, "xMin")?,
                    z_min: attr(rect, "zMin")?,
                    kind: attr_or(rect, "kind", 0)?,
                    flags: attr_or(rect, "flags", 0)?,
                })
            })
            .collect::<Result<_>>()?;
    }
    Ok(collision)
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

fn parse_messages(node: Node<'_, '_>) -> Result<Messages> {
    check_attributes(node, &["script", "raw"])?;
    if let Some(raw) = node.attribute("raw") {
        if elements(node).next().is_some() {
            bail!("a raw <messages> block has no <message> children");
        }
        return Ok(Messages::Raw(unhex(raw)?));
    }
    let script = match node.attribute("script").unwrap_or("latin") {
        "latin" => Script::Latin,
        "japanese" => Script::Japanese,
        other => bail!("<messages> script `{other}` is not latin or japanese"),
    };
    let mut streams = Vec::new();
    for (index, message) in children(node, "message")?.enumerate() {
        check_attributes(message, &["id", "raw"])?;
        let id: usize = attr_or(message, "id", index)?;
        if id != index {
            bail!("<message> id {id} is out of order (expected {index})");
        }
        if let Some(raw) = message.attribute("raw") {
            streams.push(unhex(raw)?);
            continue;
        }
        let mut pieces = Vec::new();
        for child in message.children() {
            if child.is_text() {
                let text = child.text().unwrap_or("");
                // A character the font cannot spell would silently become
                // another glyph, so it is refused.
                let mut bytes = Vec::new();
                crate::charset::encode(text, script, &mut bytes);
                let mut spelled = String::new();
                crate::charset::decode(&bytes, script, &mut spelled);
                if spelled != text {
                    bail!("message {index}: the font cannot spell `{text}` exactly");
                }
                pieces.push(Piece::Text(text.to_string()));
            } else if child.is_element() {
                let name = child.tag_name().name();
                let Some(name) = markup::element(name) else {
                    bail!("message {index}: unknown element <{name}>");
                };
                let names = markup::attributes(name);
                check_attributes(child, names)?;
                let values = names
                    .iter()
                    .map(|key| {
                        let raw = attr_text(child, key)?;
                        let parsed = integer(raw).and_then(|value| u8::try_from(value).ok());
                        parsed.map(|value| (*key, value)).with_context(|| {
                            format!("message {index}: <{name}> `{key}` value `{raw}` is not a byte")
                        })
                    })
                    .collect::<Result<_>>()?;
                pieces.push(Piece::Tag(name, values));
            }
        }
        streams.push(markup::decode(&pieces, script).with_context(|| format!("message {index}"))?);
    }
    Ok(Messages::Streams { script, streams })
}

fn parse_effects(node: Node<'_, '_>) -> Result<Vec<EffectSlot>> {
    check_attributes(node, &[])?;
    let mut slots = Vec::new();
    for (position, effect) in children(node, "effect")?.enumerate() {
        check_attributes(effect, &["slot", "index", "sheet", "clut", "depthTable"])?;
        let slot: usize = attr_or(effect, "slot", position)?;
        if slot != position {
            bail!("<effect> slot {slot} is out of order (expected {position})");
        }
        let index: u8 = attr(effect, "index")?;
        if index == 0xFF {
            bail!("<effect> index 255 marks a free slot and cannot be declared");
        }
        if effect.attribute("depthTable").is_none() && elements(effect).next().is_none() {
            slots.push(EffectSlot {
                index,
                sprite: None,
            });
            continue;
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
            sheet: effect.attribute("sheet").map(str::to_string),
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
        slots.push(EffectSlot {
            index,
            sprite: Some(sprite),
        });
    }
    Ok(slots)
}

fn parse_models(node: Node<'_, '_>, element: &'static str) -> Result<Vec<Option<String>>> {
    check_attributes(node, &[])?;
    children(node, element)?
        .enumerate()
        .map(|(position, model)| {
            check_attributes(model, &["slot", "model"])?;
            let slot: usize = attr_or(model, "slot", position)?;
            if slot != position {
                bail!("<{element}> slot {slot} is out of order (expected {position})");
            }
            Ok(model.attribute("model").map(str::to_string))
        })
        .collect()
}
