//! The interactive screens (`check_and_display_interactive_screen`,
//! `InteractiveScreen.cpp`), run from the pack's `interactive/*.lua` scripts.
//!
//! The original has three screens the game loop keeps running underneath:
//! the passcode panel (room 4080), the lab computer (room 5060) and the slide
//! projector (room 4070). While scenario flag 0x20 is up, the dispatcher
//! calls the one a system flag selects once a frame, right after the room
//! scripts and before the camera switch and the entity update. The screens
//! themselves are pack Lua ([`SCREENS`]); this module is the engine half: the
//! dispatcher, the pad words the screens read, the texture slots they load,
//! the sprite primitives they draw through (`AddSpriteEx_Core`,
//! `PrintText8x8`, `PrintText8x14`, the perspective quad) and the game state
//! they touch.
//!
//! # State
//!
//! A screen script keeps no state in the VM: each call gets its state table
//! `s`, which the host keeps in [`GameState`] between calls as a
//! [`ScriptValue`] tree, so dropping the VM never changes behaviour and the
//! state clones and compares with the rest of the game. The first four fields
//! of every screen's table, `func`, `sub`, `sub2` and `sub3`, are the
//! original's shared state block, which the dispatcher clears when the gate
//! rises (and `func` on a room load).

use std::collections::BTreeMap;

use mlua::{Lua, Table, UserData, UserDataFields, UserDataMethods, Value};

use crate::game::{self, GameState};
use crate::pack::Pack;
use crate::player::PlayerState;
use crate::render::{EffectBlend, EffectQuad};
use crate::state::{Image, RoomState};

/// The screens by the system flag that selects them, in the dispatcher's
/// test order. A pack without a screen's script shows nothing for it.
pub const SCREENS: [(u8, &str); 3] = [
    (0x1d, "interactive/passcode_panel.lua"),
    (0x1e, "interactive/computer_lab.lua"),
    (0x1f, "interactive/slides.lua"),
];

/// The 8x8 font page `PrintText8x8` draws from.
pub const FONT_PAGE: &str = "ui/font.tim";

/// Scenario flag 0x20: an interactive screen is up.
const SCENARIO_INTERACTIVE: u8 = 0x20;
/// The game-space screen centre (`g_ScreenOffsetX/Y`).
const CENTRE: [i32; 2] = [160, 120];
/// The fov a camera-less room reports.
const DEFAULT_FOV: i32 = 589;
/// The deepest table a screen's state may nest.
const MAX_DEPTH: usize = 8;
/// The most values a screen's state may hold.
const MAX_VALUES: usize = 1 << 16;
/// The shared state block the dispatcher clears.
const BLOCK: [&str; 4] = ["func", "sub", "sub2", "sub3"];

// ---- the state tree --------------------------------------------------------

/// A table key of a screen's state.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ScriptKey {
    Int(i64),
    /// A string key, by its bytes.
    Str(Vec<u8>),
}

/// A value of a screen's state, as the host keeps it between calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptValue {
    Bool(bool),
    Int(i64),
    /// A float, by its bits.
    Float(u64),
    /// A string, by its bytes (a Lua string need not be UTF-8).
    Str(Vec<u8>),
    /// A table, sorted by key.
    Table(Vec<(ScriptKey, ScriptValue)>),
}

impl Default for ScriptValue {
    fn default() -> Self {
        ScriptValue::Table(Vec::new())
    }
}

impl ScriptValue {
    /// The field `key` of a table.
    pub fn field(&self, key: &str) -> Option<&ScriptValue> {
        self.get(&ScriptKey::Str(key.as_bytes().to_vec()))
    }

    /// The element `index` of a table.
    pub fn index(&self, index: i64) -> Option<&ScriptValue> {
        self.get(&ScriptKey::Int(index))
    }

    fn get(&self, key: &ScriptKey) -> Option<&ScriptValue> {
        let ScriptValue::Table(entries) = self else {
            return None;
        };
        entries
            .binary_search_by(|(k, _)| k.cmp(key))
            .ok()
            .map(|index| &entries[index].1)
    }

    /// The integer field `key` of a table.
    pub fn int(&self, key: &str) -> Option<i64> {
        match self.field(key)? {
            ScriptValue::Int(value) => Some(*value),
            _ => None,
        }
    }

    /// Set the integer field `key` of a table, adding it when absent.
    fn set_int(&mut self, key: &str, value: i64) {
        let ScriptValue::Table(entries) = self else {
            return;
        };
        let key = ScriptKey::Str(key.as_bytes().to_vec());
        match entries.binary_search_by(|(k, _)| k.cmp(&key)) {
            Ok(index) => entries[index].1 = ScriptValue::Int(value),
            Err(index) => entries.insert(index, (key, ScriptValue::Int(value))),
        }
    }
}

/// The Lua table of a state tree.
pub fn to_lua(lua: &Lua, value: &ScriptValue) -> mlua::Result<Table> {
    let ScriptValue::Table(entries) = value else {
        return Err(mlua::Error::runtime("a screen's state must be a table"));
    };
    let table = lua.create_table_with_capacity(0, entries.len())?;
    for (key, value) in entries {
        let value = match value {
            ScriptValue::Bool(b) => Value::Boolean(*b),
            ScriptValue::Int(i) => Value::Integer(*i),
            ScriptValue::Float(bits) => Value::Number(f64::from_bits(*bits)),
            ScriptValue::Str(s) => Value::String(lua.create_string(s)?),
            ScriptValue::Table(_) => Value::Table(to_lua(lua, value)?),
        };
        match key {
            ScriptKey::Int(i) => table.raw_set(*i, value)?,
            ScriptKey::Str(s) => table.raw_set(lua.create_string(s)?, value)?,
        }
    }
    Ok(table)
}

/// The state tree of a Lua table. A screen's state is plain data: a
/// function, a userdata, or a table that appears twice (shared, or a cycle)
/// is an error, since the next call would get separate copies.
pub fn from_lua(table: &Table) -> mlua::Result<ScriptValue> {
    let mut count = 0;
    let mut seen = std::collections::HashSet::new();
    from_lua_at(table, 0, &mut count, &mut seen)
}

fn from_lua_at(
    table: &Table,
    depth: usize,
    count: &mut usize,
    seen: &mut std::collections::HashSet<*const std::ffi::c_void>,
) -> mlua::Result<ScriptValue> {
    if depth >= MAX_DEPTH {
        return Err(mlua::Error::runtime("a screen's state nests too deep"));
    }
    if !seen.insert(table.to_pointer()) {
        return Err(mlua::Error::runtime(
            "a screen's state holds the same table twice",
        ));
    }
    let mut entries = Vec::new();
    for pair in table.pairs::<Value, Value>() {
        let (key, value) = pair?;
        *count += 1;
        if *count > MAX_VALUES {
            return Err(mlua::Error::runtime(
                "a screen's state holds too many values",
            ));
        }
        let key = match key {
            Value::Integer(i) => ScriptKey::Int(i),
            Value::String(s) => ScriptKey::Str(s.as_bytes().to_vec()),
            other => {
                return Err(mlua::Error::runtime(format!(
                    "a screen's state key must be an integer or a string, not {}",
                    other.type_name()
                )));
            }
        };
        let value = match value {
            Value::Boolean(b) => ScriptValue::Bool(b),
            Value::Integer(i) => ScriptValue::Int(i),
            Value::Number(n) => ScriptValue::Float(n.to_bits()),
            Value::String(s) => ScriptValue::Str(s.as_bytes().to_vec()),
            Value::Table(t) => from_lua_at(&t, depth + 1, count, seen)?,
            other => {
                return Err(mlua::Error::runtime(format!(
                    "a screen's state cannot hold a {}",
                    other.type_name()
                )));
            }
        };
        entries.push((key, value));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(ScriptValue::Table(entries))
}

// ---- the pad ---------------------------------------------------------------

/// The pad words a screen reads this frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Pad {
    /// `g_PlayerDpadPressed >> 8`: 0x40 confirm, 0x80 cancel (edges).
    pub dpad_pressed: u8,
    /// `g_PlayerDpadHeld >> 8`.
    pub dpad_held: u8,
    /// `g_PlayerPadHeld >> 8`, the raw pad's rising edges: 0x10 up, 0x40
    /// down.
    pub pad_edge: u8,
    /// `g_button_pressed_id & 0xF000`: the raw held D-pad (0x1000 up, 0x2000
    /// right, 0x4000 down, 0x8000 left).
    pub raw_held: u16,
    /// `g_PlayerPadHeld & 0xF000`: the raw D-pad's rising edges.
    pub raw_edge: u16,
}

impl Pad {
    /// The pad words from arklay's remapped D-pad words
    /// ([`game::dpad_word`]: up 1, down 2, left 4, right 8, run/cancel 0x10,
    /// confirm 0x80).
    pub fn from_dpad(held: u16, pressed: u16) -> Self {
        let remapped = |word: u16| {
            let mut byte = 0u8;
            if word & 0x0080 != 0 {
                byte |= 0x40;
            }
            if word & 0x0010 != 0 {
                byte |= 0x80;
            }
            byte
        };
        let raw = |word: u16| {
            let mut bits = 0u16;
            if word & 0x0001 != 0 {
                bits |= 0x1000;
            }
            if word & 0x0008 != 0 {
                bits |= 0x2000;
            }
            if word & 0x0002 != 0 {
                bits |= 0x4000;
            }
            if word & 0x0004 != 0 {
                bits |= 0x8000;
            }
            bits
        };
        let raw_edge = raw(pressed);
        Self {
            dpad_pressed: remapped(pressed),
            dpad_held: remapped(held),
            pad_edge: (raw_edge >> 8) as u8,
            raw_held: raw(held),
            raw_edge,
        }
    }
}

// ---- textures and sprites --------------------------------------------------

/// A texture slot a screen loaded (`LoadTexturePage`).
#[derive(Debug, Clone, PartialEq, Eq)]
struct TextureSlot {
    /// The pack entry of the TIM.
    entry: String,
    /// The VRAM texture page it sits at.
    tpage: i32,
    /// Its width in 16-bit VRAM words.
    words: i32,
}

/// The width in 16-bit VRAM words of a TIM's image block.
fn tim_words(data: &[u8]) -> Option<i32> {
    let u32_at = |offset: usize| -> Option<u32> {
        Some(u32::from_le_bytes(
            data.get(offset..offset + 4)?.try_into().ok()?,
        ))
    };
    let flags = u32_at(4)?;
    let mut offset = 8;
    if flags & 8 != 0 {
        offset += u32_at(offset)? as usize;
    }
    let width = data.get(offset + 8..offset + 10)?;
    Some(i32::from(u16::from_le_bytes([width[0], width[1]])))
}

/// Decode one page to RGBA: every texel whose CLUT word is `0x0000` is
/// transparent, the PC renderer's colour key.
pub fn decode_page(data: &[u8]) -> Option<Image> {
    let texture = match data.get(4).map(|flags| flags & 7) {
        Some(0) => crate::tim::decode_4bpp(data).ok()?,
        Some(1) => crate::tim::decode_8bpp(data).ok()?,
        _ => return None,
    };
    let mut rgba = Vec::with_capacity(texture.indices.len() * 4);
    for &index in &texture.indices {
        let [r, g, b, _] = texture.palette(0, index);
        if [r, g, b] == [0, 0, 0] && !texture.palette_stp(0, index) {
            rgba.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            rgba.extend_from_slice(&[r, g, b, 255]);
        }
    }
    Some(Image {
        width: texture.width,
        height: texture.height,
        rgba,
    })
}

/// A sprite descriptor (`g_TextureDesc`), read from a screen's table. Each
/// field takes the C field's width, so an out-of-range value truncates like
/// the store would.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Desc {
    page: i16,
    pivot_x: i16,
    pivot_y: i16,
    screen_x: i16,
    screen_y: i16,
    tex_u: u8,
    tex_v: u8,
    width: u16,
    height: u16,
    flags: u32,
    colour: [u8; 3],
    scale_x: i16,
    scale_y: i16,
}

impl Desc {
    fn read(table: &Table) -> mlua::Result<Self> {
        let get = |name: &str| table.get::<i64>(name);
        Ok(Self {
            page: get("page")? as i16,
            pivot_x: get("pivot_x")? as i16,
            pivot_y: get("pivot_y")? as i16,
            screen_x: get("screen_x")? as i16,
            screen_y: get("screen_y")? as i16,
            tex_u: get("tex_u")? as u8,
            tex_v: get("tex_v")? as u8,
            width: get("width")? as u16,
            height: get("height")? as u16,
            flags: get("flags")? as u32,
            colour: [get("r")? as u8, get("g")? as u8, get("b")? as u8],
            scale_x: get("scale_x")? as i16,
            scale_y: get("scale_y")? as i16,
        })
    }
}

/// A `PrintText8x14` string a screen drew this frame, in the message font.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Text8x14 {
    /// Screen pixel x of the first glyph.
    pub x: i32,
    /// Screen pixel y.
    pub y: i32,
    /// The font CLUT row (the colour's low nibble).
    pub row: u8,
    /// The glyph bytes.
    pub bytes: Vec<u8>,
}

/// The interactive screens' engine state, kept in [`GameState`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InteractiveState {
    /// The cancel key's held level last frame, for its press edge.
    cancel_prev: bool,
    /// `g_labSlidesState`: the gate's value after the previous dispatch.
    was_up: bool,
    /// Each screen's state table, by script entry.
    states: BTreeMap<String, ScriptValue>,
    /// The loaded texture slots, by slot.
    textures: BTreeMap<u8, TextureSlot>,
    /// The pages this frame's sprites index, by pack entry.
    pages: Vec<String>,
    /// This frame's sprites, in submission order.
    sprites: Vec<EffectQuad>,
    /// This frame's 8x14 strings, in submission order.
    texts: Vec<Text8x14>,
    /// What the screen up now may change, as it was when the gate rose.
    entered: Option<Entered>,
}

/// The state a screen's API can change outside the screen's own, taken when
/// the gate rises, so a screen whose script fails can be backed out of.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entered {
    camera: usize,
    room_cut: usize,
    zone_group: Option<usize>,
    player_y: i32,
    lights: [crate::state::Light; 3],
    ambient: [i16; 3],
    message_flags: u16,
    enemy_flags: Vec<u8>,
}

impl Entered {
    fn take(game: &GameState, room: &RoomState, player: &PlayerState) -> Self {
        Entered {
            camera: game.camera.current_cut,
            room_cut: room.current_cut,
            zone_group: room.zone_group,
            player_y: player.pos[1],
            lights: room.lights,
            ambient: room.ambient,
            message_flags: game.message_flags,
            enemy_flags: game.entities.iter().map(|e| e.behavior_flags).collect(),
        }
    }

    fn restore(&self, game: &mut GameState, room: &mut RoomState, player: &mut PlayerState) {
        game.set_camera_cut(self.camera);
        room.current_cut = self.room_cut;
        room.zone_group = self.zone_group;
        player.pos[1] = self.player_y;
        game.sync_entity_from_player(player);
        room.lights = self.lights;
        room.ambient = self.ambient;
        game.message_flags = self.message_flags;
        for (entity, flags) in game.entities.iter_mut().zip(&self.enemy_flags) {
            entity.behavior_flags = *flags;
        }
    }
}

impl InteractiveState {
    /// This frame's pad from the game's remapped D-pad words, with the
    /// cancel key folded into the run bit (`0x10`): the original reads one
    /// raw bit for both. Call once per frame, a screen up or not, so the
    /// cancel edge stays current.
    pub fn pad_from_dpad(&mut self, held: u16, pressed: u16, cancel_held: bool) -> Pad {
        let cancel = if cancel_held { 0x10 } else { 0 };
        let cancel_edge = if cancel_held && !self.cancel_prev {
            0x10
        } else {
            0
        };
        self.cancel_prev = cancel_held;
        Pad::from_dpad(held | cancel, pressed | cancel_edge)
    }

    /// `lab_slides_reset` (0x0042a020), which `room_set` runs: a room load
    /// drops a screen left mid-sequence, so the next visit starts over at its
    /// init (the room's own textures replace the screen's).
    pub fn room_reset(&mut self) {
        for state in self.states.values_mut() {
            state.set_int("func", 0);
        }
        self.was_up = false;
        self.entered = None;
        self.textures.clear();
        self.pages.clear();
        self.sprites.clear();
        self.texts.clear();
    }

    /// This frame's sprites. Their `page` indexes [`Self::pages`].
    pub fn sprites(&self) -> &[EffectQuad] {
        &self.sprites
    }

    /// This frame's 8x14 strings, drawn over the scene in the message font.
    pub fn texts(&self) -> &[Text8x14] {
        &self.texts
    }

    /// The pack entries this frame's sprites draw from.
    pub fn pages(&self) -> &[String] {
        &self.pages
    }

    /// A screen's state table, for tests.
    pub fn state(&self, entry: &str) -> Option<&ScriptValue> {
        self.states.get(entry)
    }

    fn page_index(&mut self, entry: &str) -> usize {
        if let Some(index) = self.pages.iter().position(|page| page == entry) {
            return index;
        }
        self.pages.push(entry.to_string());
        self.pages.len() - 1
    }

    /// The page a draw resolves to: the original's search over the texture
    /// slots `slot .. slot + count`, and the u offset into it.
    fn resolve(&self, desc: &Desc, slot: u8, count: u8) -> Option<(String, i32)> {
        let scale = [4, 2, 1, 1][((desc.flags >> 24) & 3) as usize];
        let mut page = i32::from(desc.page);
        let mut v_add = 0;
        if page > 16 {
            v_add = 256;
            page -= 16;
        }
        let u_words = page * 0x40 + i32::from(desc.tex_u) / scale;
        let v_abs = v_add + i32::from(desc.tex_v);
        for index in 0..count {
            let Some(texture) = self.textures.get(&slot.wrapping_add(index)) else {
                continue;
            };
            let left = texture.tpage * 0x40;
            let right = left + texture.words;
            if left <= u_words
                && u_words + i32::from(desc.width) / scale <= right
                && v_abs + i32::from(desc.height) <= 256
            {
                let offset = (i32::from(desc.page) - texture.tpage) * scale * 0x40;
                return Some((texture.entry.clone(), offset));
            }
        }
        None
    }

    /// `AddSpriteEx_Core`: one sprite of `desc`. `scaled` sprites scale about
    /// their pivot; `effect` sprites sort among the scene's triangles at
    /// `key`, the others draw in the 2D pass after the scene.
    fn submit(&mut self, desc: &Desc, key: u32, slot: u8, count: u8, scaled: bool, effect: bool) {
        let Some((entry, offset)) = self.resolve(desc, slot, count) else {
            return;
        };
        let page = self.page_index(&entry);
        let scaled = scaled && (desc.scale_x != 0x1000 || desc.scale_y != 0x1000);
        let sx = i32::from(desc.screen_x) + CENTRE[0];
        let sy = i32::from(desc.screen_y) + CENTRE[1];
        let (w, h) = (i32::from(desc.width), i32::from(desc.height));
        let (px, py) = (i32::from(desc.pivot_x), i32::from(desc.pivot_y));
        let fix = |value: i32| (value + ((value >> 31) & 0xfff)) >> 12;
        let (x0, y0, x1, y1) = if scaled {
            let (scx, scy) = (i32::from(desc.scale_x), i32::from(desc.scale_y));
            (
                sx - (fix(px * scx) as i16 as i32),
                sy - (fix(py * scy) as i16 as i32),
                (fix((w - px) * scx) as i16 as i32) + sx - 1,
                (fix((h - py) * scy) as i16 as i32) + sy - 1,
            )
        } else {
            (sx - px, sy - py, (w - px) + sx - 1, (h - py) + sy - 1)
        };
        let (x0, y0, x1, y1) = (
            x0 as i16 as i32,
            y0 as i16 as i32,
            x1 as i16 as i32,
            y1 as i16 as i32,
        );
        let su0 = (i32::from(desc.tex_u) + offset).max(0);
        let sv0 = i32::from(desc.tex_v).max(0);
        let su1 = (i32::from(desc.tex_u) + offset + w - 1).max(0);
        let sv1 = (i32::from(desc.tex_v) + h - 1).max(0);
        let colour = desc.colour.map(|channel| {
            let value = f32::from(channel) * (2.0f32 / 255.0f32);
            ((value * 255.0f32) as i32).clamp(0, 255) as u8
        });
        let variant = if desc.flags & 0x4000_0000 != 0 {
            ((desc.flags & 0x3000_0000) >> 28) + 1
        } else {
            0
        };
        let blend = match variant {
            1 | 2 | 4 => 0x80,
            _ => 0,
        };
        let key = if effect {
            key.wrapping_mul(0x10).wrapping_add(500)
        } else {
            // A NORMAL-class sprite draws in the 2D pass after the scene:
            // keep its order but sort it in front of every scene item.
            key.wrapping_mul(0x10).wrapping_add(500) >> 6
        };
        let size = [(x1 + 1 - x0).max(1), (y1 + 1 - y0).max(1)];
        let source = [(su1 + 1 - su0).max(0), (sv1 + 1 - sv0).max(0)];
        let quad = EffectQuad {
            key,
            page,
            uv: [su0.min(255) as u8, sv0.min(255) as u8, 0, 0],
            source: [source[0].min(255) as u8, source[1].min(255) as u8],
            pos: [x0, y0],
            size,
            corners: None,
            tint: colour,
            blend,
            sheet: [0, 256],
            op: EffectBlend::Normal,
            mirror: [desc.flags & 0x0080_0000 != 0, desc.flags & 0x0040_0000 != 0],
        };
        // A full 256-texel row (the lab login window's title art) is wider
        // than a quad's byte-sized source: draw it as two 1:1 halves.
        if source[0] > 255 && size[0] == source[0] && !quad.mirror[0] {
            let half = source[0] / 2;
            self.sprites.push(EffectQuad {
                source: [half as u8, quad.source[1]],
                size: [half, size[1]],
                ..quad
            });
            self.sprites.push(EffectQuad {
                uv: [(su0 + half) as u8, quad.uv[1], 0, 0],
                source: [(source[0] - half) as u8, quad.source[1]],
                pos: [x0 + half, y0],
                size: [size[0] - half, size[1]],
                ..quad
            });
            return;
        }
        self.sprites.push(quad);
    }

    /// `PrintText8x8(text, x, y, colour, 0)`: the 8x8 font at screen pixel
    /// `(x, y)`, in the overlay pass after every sprite. The colour's low
    /// nibble is the CLUT tint (0 white, 1 green, 2 red, 3 grey, else
    /// yellow). The high nibble is the brightness, which the original's
    /// `AddTintSprite` (JPN 0x00441120) only turns into the sprite's ordering
    /// depth (`n * 16 + 0x1C2`): it never scales the colour, so the lab
    /// terminal's `0xF0` star is plain white. Every glyph here draws in the
    /// overlay pass, so the brightness is ignored.
    fn print8x8(&mut self, x: i32, y: i32, text: &[u8], colour: u8) {
        let page = self.page_index(FONT_PAGE);
        let tint: [u8; 3] = match colour & 0xf {
            0 => [255, 255, 255],
            1 => [0, 255, 0],
            2 => [255, 0, 0],
            3 => [204, 204, 204],
            _ => [255, 255, 0],
        };
        let mut x = x;
        for &ch in text {
            if ch != b' ' {
                self.sprites.push(EffectQuad {
                    key: 0,
                    page,
                    uv: [
                        ch.wrapping_mul(8),
                        (ch.wrapping_sub(0x20) & 0xe3) >> 2,
                        0,
                        0,
                    ],
                    source: [8, 8],
                    pos: [x, y],
                    size: [8, 8],
                    corners: None,
                    tint,
                    blend: 0,
                    sheet: [0, 256],
                    op: EffectBlend::Normal,
                    mirror: [false; 2],
                });
            }
            x += 8;
        }
    }
}

// ---- the dispatcher --------------------------------------------------------

/// What a frame of the screens needs from the session.
pub struct Host<'a> {
    pub game: &'a mut GameState,
    pub room: &'a mut RoomState,
    pub player: &'a mut PlayerState,
    pub pack: &'a Pack,
}

/// `check_and_display_interactive_screen` (0x0042a030): one frame of the
/// screen the system flags select, while the gate is up.
///
/// A screen whose script is missing, or fails, would leave the gate up with
/// nothing to lower it, and the player held. So the engine backs out of it
/// instead: it restores the camera, lights, message flags, player height and
/// enemy commands as they were when the gate rose, frees the screen's
/// textures and lowers the gate. The failure is logged; the screen's state
/// table keeps its last good value.
pub fn tick(host: &mut Host<'_>, lua: &mut crate::enemy::LuaEnemyHost, cancel_held: bool) {
    let game = &mut *host.game;
    let pad = game
        .interactive
        .pad_from_dpad(game.dpad_held, game.dpad_pressed, cancel_held);
    game.interactive.sprites.clear();
    game.interactive.pages.clear();
    game.interactive.texts.clear();
    if game.flag_test(game::BANK_SCENARIO, SCENARIO_INTERACTIVE, false) {
        if !game.interactive.was_up {
            for state in game.interactive.states.values_mut() {
                for field in BLOCK {
                    state.set_int(field, 0);
                }
            }
            game.interactive.entered = Some(Entered::take(game, host.room, host.player));
        }
        let selected = SCREENS
            .iter()
            .find(|(flag, _)| game.flag_test(game::BANK_SYSTEM, *flag, false));
        if let Some(&(_, entry)) = selected
            && !display(host, lua, entry, pad)
        {
            abort(host);
        }
    }
    let game = &mut *host.game;
    game.interactive.was_up = game.flag_test(game::BANK_SCENARIO, SCENARIO_INTERACTIVE, false);
}

/// Back out of a screen whose script is missing or failed (see [`tick`]).
fn abort(host: &mut Host<'_>) {
    let state = &mut host.game.interactive;
    if let Some(entered) = state.entered.take() {
        entered.restore(host.game, host.room, host.player);
    }
    let state = &mut host.game.interactive;
    state.textures.clear();
    state.sprites.clear();
    state.pages.clear();
    state.texts.clear();
    host.game
        .apply_flag(game::BANK_SCENARIO, SCENARIO_INTERACTIVE, 1);
}

/// One `display(api, s)` call of the screen script `entry`: whether it ran.
fn display(
    host: &mut Host<'_>,
    lua: &mut crate::enemy::LuaEnemyHost,
    entry: &str,
    pad: Pad,
) -> bool {
    let mut state = host
        .game
        .interactive
        .states
        .remove(entry)
        .unwrap_or_default();
    let mut api = ScreenApi {
        game: host.game as *mut GameState,
        room: host.room as *mut RoomState,
        player: host.player as *mut PlayerState,
        pack: host.pack as *const Pack,
        pad,
    };
    let ran = lua.call_screen(host.pack, entry, &mut state, &mut api);
    host.game
        .interactive
        .states
        .insert(entry.to_string(), state);
    ran
}

// ---- the script API --------------------------------------------------------

/// The `api` a screen's `display(api, s)` gets.
///
/// # Safety
///
/// The scoped userdata holds this struct by mutable reference for exactly one
/// call; `Lua::scope` seals it so the script cannot retain it. The pointers
/// borrow the caller's live data for that call, and no method stores them.
struct ScreenApi {
    game: *mut GameState,
    room: *mut RoomState,
    player: *mut PlayerState,
    pack: *const Pack,
    pad: Pad,
}

impl ScreenApi {
    fn game(&self) -> &GameState {
        // SAFETY: see the type-level comment.
        unsafe { &*self.game }
    }

    fn game_mut(&mut self) -> &mut GameState {
        // SAFETY: see the type-level comment.
        unsafe { &mut *self.game }
    }

    fn room(&self) -> &RoomState {
        // SAFETY: see the type-level comment.
        unsafe { &*self.room }
    }

    fn room_mut(&mut self) -> &mut RoomState {
        // SAFETY: see the type-level comment.
        unsafe { &mut *self.room }
    }

    fn player(&self) -> &PlayerState {
        // SAFETY: see the type-level comment.
        unsafe { &*self.player }
    }

    /// The game and the room at once; they are distinct objects.
    fn game_and_room(&mut self) -> (&mut GameState, &mut RoomState) {
        // SAFETY: see the type-level comment; the pointers never alias.
        unsafe { (&mut *self.game, &mut *self.room) }
    }

    /// The game and the player at once; they are distinct objects.
    fn game_and_player(&mut self) -> (&mut GameState, &mut PlayerState) {
        // SAFETY: see the type-level comment; the pointers never alias.
        unsafe { (&mut *self.game, &mut *self.player) }
    }

    fn pack(&self) -> &Pack {
        // SAFETY: see the type-level comment.
        unsafe { &*self.pack }
    }

    /// `g_EnemiesList[index]`: entity slot `index + 1`.
    fn enemy_slot(index: i64) -> mlua::Result<usize> {
        usize::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(1))
            .filter(|slot| *slot < crate::game::ENTITY_COUNT)
            .ok_or_else(|| mlua::Error::runtime(format!("no enemy slot {index}")))
    }

    fn light_index(index: i64) -> mlua::Result<usize> {
        usize::try_from(index)
            .ok()
            .filter(|index| *index < 3)
            .ok_or_else(|| mlua::Error::runtime(format!("no light {index}")))
    }

    /// `g_roomCameraId = camera; check_camera_switch(1)`: the forced cut.
    /// The zone pointer only follows a camera that has a switch-zone group,
    /// so a camera without one leaves it on the group it was on.
    fn switch_camera(&mut self, camera: usize) {
        let (game, room) = self.game_and_room();
        if camera >= room.cuts.len() {
            return;
        }
        let has_group = room
            .zones
            .iter()
            .any(|zone| zone.cam_from >= 0 && zone.cam_from as usize == camera);
        room.zone_group = if has_group {
            None
        } else {
            Some(room.zone_group.unwrap_or(room.current_cut))
        };
        game.set_camera_cut(camera);
        room.current_cut = camera;
    }
}

impl UserData for ScreenApi {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("pad_pressed", |_, api| Ok(api.pad.dpad_pressed));
        fields.add_field_method_get("pad_held", |_, api| Ok(api.pad.dpad_held));
        fields.add_field_method_get("pad_edge", |_, api| Ok(api.pad.pad_edge));
        fields.add_field_method_get("raw_held", |_, api| Ok(api.pad.raw_held));
        fields.add_field_method_get("raw_edge", |_, api| Ok(api.pad.raw_edge));
        fields.add_field_method_get("player_character", |_, api| {
            Ok(api.game().id.player_flag & 1)
        });
        fields.add_field_method_get("player_id", |_, api| Ok(api.game().entities[0].id));
        fields.add_field_method_get("menu_choice", |_, api| {
            Ok(api.game().message.menu_choice_id())
        });
        fields.add_field_method_get("camera", |_, api| Ok(api.game().camera.current_cut as i64));
        fields.add_field_method_get("fov", |_, api| {
            let room = api.room();
            Ok(room
                .cuts
                .get(room.current_cut)
                .map_or(DEFAULT_FOV, |cut| cut.fov))
        });
        fields.add_field_method_get("message_flags", |_, api| Ok(api.game().message_flags));
        fields.add_field_method_set("message_flags", |_, api, value: i64| {
            api.game_mut().message_flags = value as u16;
            Ok(())
        });
        fields.add_field_method_get("player_y", |_, api| Ok(api.player().pos[1]));
        fields.add_field_method_set("player_y", |_, api, value: i64| {
            let (game, player) = api.game_and_player();
            player.pos[1] = value as i32;
            game.sync_entity_from_player(player);
            Ok(())
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("flag", |_, api, (bank, bit): (i64, i64)| {
            Ok(api.game().flag_test(bank as u8, bit as u8, false))
        });
        methods.add_method_mut("flag_on", |_, api, (bank, bit): (i64, i64)| {
            api.game_mut().apply_flag(bank as u8, bit as u8, 0);
            Ok(())
        });
        methods.add_method_mut("flag_off", |_, api, (bank, bit): (i64, i64)| {
            api.game_mut().apply_flag(bank as u8, bit as u8, 1);
            Ok(())
        });
        methods.add_method("enemy_flags", |_, api, index: i64| {
            Ok(api.game().entities[Self::enemy_slot(index)?].behavior_flags)
        });
        methods.add_method_mut("set_enemy_flags", |_, api, (index, value): (i64, i64)| {
            api.game_mut().entities[Self::enemy_slot(index)?].behavior_flags = value as u8;
            Ok(())
        });
        methods.add_method_mut("rand", |_, api, ()| {
            Ok(i64::from(game::platform_rand(
                &mut api.game_mut().rand_state,
            )))
        });
        methods.add_method_mut("sound", |_, api, (bank, id): (i64, i64)| {
            api.game_mut().snd3d_requests.push(game::Snd3dRequest {
                bank: bank as u8,
                id: id as u8,
                volume: 0,
                pos: game::Snd3dPos::None,
            });
            Ok(())
        });
        methods.add_method_mut("voice", |_, api, id: i64| {
            let game = api.game_mut();
            let row = game.id.table_row() as u8;
            game.voice.request = Some(game::VoiceRequest {
                path: crate::voice::pack_path(row, id as u16),
                volume: 0,
                pan: 0,
            });
            game.set_voice_playing();
            Ok(())
        });
        methods.add_method("voice_playing", |_, api, ()| Ok(api.game().voice_playing()));
        methods.add_method_mut("message", |_, api, (id, pause): (i64, i64)| {
            api.game_mut().show_message_word(id as u16, pause as u16);
            Ok(())
        });
        methods.add_method_mut("set_camera_fov", |_, api, (camera, fov): (i64, i64)| {
            if let Some(cut) = usize::try_from(camera)
                .ok()
                .and_then(|camera| api.room_mut().cuts.get_mut(camera))
            {
                cut.fov = fov as i32;
            }
            Ok(())
        });
        methods.add_method_mut("set_room_sprite", |_, api, (id, active): (i64, bool)| {
            api.game_mut().mask_toggles.push(game::MaskToggle {
                group: id as u8,
                active,
            });
            Ok(())
        });
        methods.add_method_mut("switch_camera", |_, api, camera: i64| {
            if let Ok(camera) = usize::try_from(camera) {
                api.switch_camera(camera);
            }
            Ok(())
        });
        methods.add_method("light", |_, api, index: i64| {
            let light = api.room().lights[Self::light_index(index)?];
            Ok((
                light.pos[0],
                light.pos[1],
                light.pos[2],
                light.color[0],
                light.color[1],
                light.color[2],
                light.kind,
                light.radius,
            ))
        });
        methods.add_method_mut(
            "set_light",
            |_, api, args: (i64, i64, i64, i64, i64, i64, i64, i64, i64)| {
                let (index, x, y, z, r, g, b, kind, radius) = args;
                let light = &mut api.room_mut().lights[Self::light_index(index)?];
                light.pos = [x as i32, y as i32, z as i32];
                light.color = [r as u8, g as u8, b as u8];
                light.kind = kind as u16;
                light.radius = radius as u16;
                Ok(())
            },
        );
        methods.add_method("ambient", |_, api, ()| {
            let [r, g, b] = api.room().ambient;
            Ok((r, g, b))
        });
        methods.add_method_mut("set_ambient", |_, api, (r, g, b): (i64, i64, i64)| {
            api.room_mut().ambient = [r as i16, g as i16, b as i16];
            Ok(())
        });
        methods.add_method_mut(
            "load_texture",
            |_, api, (slot, entry, tpage): (i64, String, i64)| {
                let words = api
                    .pack()
                    .read(&entry)
                    .ok()
                    .and_then(|data| tim_words(&data));
                let Some(words) = words else {
                    crate::warning!("[interactive] no TIM texture {entry}: slot {slot} left empty");
                    api.game_mut().interactive.textures.remove(&(slot as u8));
                    return Ok(false);
                };
                api.game_mut().interactive.textures.insert(
                    slot as u8,
                    TextureSlot {
                        entry,
                        tpage: tpage as i32,
                        words,
                    },
                );
                Ok(true)
            },
        );
        methods.add_method_mut("free_texture", |_, api, slot: i64| {
            api.game_mut().interactive.textures.remove(&(slot as u8));
            Ok(())
        });
        methods.add_method("texture_loaded", |_, api, slot: i64| {
            Ok(api.game().interactive.textures.contains_key(&(slot as u8)))
        });
        methods.add_method_mut(
            "effect_sprite",
            |_, api, (desc, key, slot, count): (Table, i64, i64, i64)| {
                let desc = Desc::read(&desc)?;
                api.game_mut().interactive.submit(
                    &desc,
                    key as u32,
                    slot as u8,
                    count as u8,
                    false,
                    true,
                );
                Ok(())
            },
        );
        methods.add_method_mut(
            "add_sprite",
            |_, api, (desc, depth, slot, count): (Table, i64, i64, i64)| {
                let desc = Desc::read(&desc)?;
                api.game_mut().interactive.submit(
                    &desc,
                    depth as u32,
                    slot as u8,
                    count as u8,
                    true,
                    false,
                );
                Ok(())
            },
        );
        methods.add_method_mut(
            "print8x8",
            |_, api, (x, y, text, colour): (i64, i64, mlua::String, i64)| {
                api.game_mut().interactive.print8x8(
                    x as i32,
                    y as i32,
                    &text.as_bytes(),
                    colour as u8,
                );
                Ok(())
            },
        );
        methods.add_method_mut(
            "print8x14",
            |_, api, (x, y, text, colour): (i64, i64, mlua::String, i64)| {
                api.game_mut().interactive.texts.push(Text8x14 {
                    x: x as i16 as i32,
                    y: y as i16 as i32,
                    row: colour as u8 & 0xf,
                    bytes: text.as_bytes().to_vec(),
                });
                Ok(())
            },
        );
        methods.add_method_mut("smooth_quad", |_, api, args: mlua::Variadic<i64>| {
            let [slot, key, u, v, w, h, corners @ ..] = args.as_slice() else {
                return Err(mlua::Error::runtime("smooth_quad takes 14 integers"));
            };
            let [x0, y0, x1, y1, x2, y2, x3, y3] = corners else {
                return Err(mlua::Error::runtime("smooth_quad takes 14 integers"));
            };
            let state = &mut api.game_mut().interactive;
            let Some(entry) = state.textures.get(&(*slot as u8)).map(|t| t.entry.clone()) else {
                return Ok(());
            };
            let page = state.page_index(&entry);
            let corner = |x: i64, y: i64| [x as i32, y as i32];
            state.sprites.push(EffectQuad {
                key: *key as u32,
                page,
                uv: [*u as u8, *v as u8, 0, 0],
                source: [*w as u8, *h as u8],
                pos: corner(*x0, *y0),
                size: [1, 1],
                corners: Some([
                    corner(*x0, *y0),
                    corner(*x1, *y1),
                    corner(*x2, *y2),
                    corner(*x3, *y3),
                ]),
                tint: [255; 3],
                blend: 0,
                sheet: [0, 256],
                op: EffectBlend::Smooth,
                mirror: [false; 2],
            });
            Ok(())
        });
        // Single-precision maths: the original computes in `float`, and a
        // Lua number is a double. Rounding each step through `f32` gives the
        // float result exactly for `+ - * /`.
        methods.add_method("f32", |_, _, value: f64| Ok(f64::from(value as f32)));
        methods.add_method("sinf", |_, _, value: f64| {
            Ok(f64::from((value as f32).sin()))
        });
        methods.add_method("cosf", |_, _, value: f64| {
            Ok(f64::from((value as f32).cos()))
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loaded() -> InteractiveState {
        let mut state = InteractiveState::default();
        for (slot, entry, tpage) in [
            (0x17, "ui/umb00.tim", 7),
            (0x18, "ui/umb01.tim", 9),
            (0x19, "ui/umb02.tim", 0xb),
        ] {
            state.textures.insert(
                slot,
                TextureSlot {
                    entry: entry.to_string(),
                    tpage,
                    words: 128,
                },
            );
        }
        state
    }

    fn desc(page: i16) -> Desc {
        Desc {
            page,
            width: 8,
            height: 8,
            flags: 0x0100_0000,
            colour: [0x80; 3],
            scale_x: 0x1000,
            scale_y: 0x1000,
            ..Desc::default()
        }
    }

    #[test]
    fn colour_bytes_follow_the_reference_scale() {
        let mut state = loaded();
        // `0x40 * (2/255f)` times 255 is exactly 128.0 in f32.
        for (input, expected) in [(0x80u8, 255u8), (0x40, 128), (0xff, 255), (0, 0)] {
            state.sprites.clear();
            let desc = Desc {
                colour: [input; 3],
                ..desc(7)
            };
            state.submit(&desc, 10, 0x17, 2, false, true);
            assert_eq!(state.sprites[0].tint[0], expected, "multiplier {input:#x}");
            assert_eq!(state.sprites[0].key, 10 * 16 + 500);
        }
    }

    #[test]
    fn a_page_outside_the_searched_slots_draws_nothing() {
        let mut state = loaded();
        state.submit(&desc(0), 10, 0x17, 2, false, true);
        assert!(state.sprites.is_empty());
        let desc = Desc {
            tex_u: 0x10,
            ..desc(10)
        };
        state.submit(&desc, 10, 0x17, 2, false, true);
        // tpage 10 is umb01's right half.
        assert_eq!(state.pages[state.sprites[0].page], "ui/umb01.tim");
        assert_eq!(state.sprites[0].uv[0], 0x90);
    }

    #[test]
    fn the_cancel_key_is_the_run_bit() {
        let mut state = InteractiveState::default();
        let pad = state.pad_from_dpad(0, 0, true);
        assert_eq!((pad.dpad_held, pad.dpad_pressed), (0x80, 0x80));
        // Held on: no second edge.
        let pad = state.pad_from_dpad(0, 0, true);
        assert_eq!((pad.dpad_held, pad.dpad_pressed), (0x80, 0));
        let pad = state.pad_from_dpad(0, 0, false);
        assert_eq!((pad.dpad_held, pad.dpad_pressed), (0, 0));
        // Run itself still reads as cancel.
        let pad = state.pad_from_dpad(0x10, 0x10, false);
        assert_eq!(pad.dpad_pressed, 0x80);
    }

    #[test]
    fn a_room_load_starts_the_screen_over() {
        let mut state = loaded();
        let mut tree = ScriptValue::default();
        tree.set_int("func", 1);
        tree.set_int("sub", 2);
        state.states.insert("interactive/x.lua".to_string(), tree);
        state.was_up = true;
        state.room_reset();
        let tree = &state.states["interactive/x.lua"];
        assert_eq!((tree.int("func"), tree.int("sub")), (Some(0), Some(2)));
        assert!(!state.was_up);
        assert!(state.textures.is_empty());
    }

    #[test]
    fn the_masked_star_is_white_whatever_its_brightness() {
        let mut state = InteractiveState::default();
        state.print8x8(10, 20, b"* *", 0xf0);
        assert_eq!(state.sprites.len(), 2);
        assert_eq!(state.sprites[0].tint, [255; 3]);
        assert_eq!(state.sprites[0].uv[..2], [80, 0]);
        assert_eq!(state.sprites[1].pos, [26, 20]);
    }

    /// Room 5060's terminal over stub pages: six cameras, the two arms, the
    /// shipped script and a TIM header per page (the tick only reads their
    /// widths).
    fn lab_scene() -> (GameState, RoomState, PlayerState, Pack) {
        let script = crate::enemy::ENEMY_SCRIPTS
            .iter()
            .find(|(entry, _)| *entry == "interactive/computer_lab.lua")
            .expect("the checked-in lab computer")
            .1;
        let tim = |words: u16| {
            let mut data = vec![0x10, 0, 0, 0, 1, 0, 0, 0];
            data.extend(12u32.to_le_bytes());
            data.extend([0, 0, 0, 0]);
            data.extend(words.to_le_bytes());
            data.extend(256u16.to_le_bytes());
            data
        };
        let mut writer = crate::pack::PackWriter::new();
        writer
            .add("interactive/computer_lab.lua", script.as_bytes().to_vec())
            .unwrap();
        for (entry, words) in [
            ("ui/umb00.tim", 128),
            ("ui/umb01.tim", 128),
            ("ui/umb02.tim", 128),
            ("ui/umbrella.tim", 64),
        ] {
            writer.add(entry, tim(words)).unwrap();
        }
        let pack = Pack::from_bytes(writer.to_bytes().unwrap()).unwrap();
        let room = RoomState {
            cuts: vec![crate::state::Cut::default(); 6],
            ..RoomState::default()
        };
        let player = crate::player::spawn(
            crate::state::RoomId {
                stage: 5,
                room: 6,
                ..Default::default()
            },
            &room,
        );
        let mut game = GameState::default();
        game.apply_flag(game::BANK_SCENARIO, SCENARIO_INTERACTIVE, 0);
        game.apply_flag(game::BANK_SYSTEM, 0x1e, 0);
        (game, room, player, pack)
    }

    /// One frame of the scene. The arms answer every command at once and the
    /// logo's voice line ends as soon as it starts, so the boot runs through.
    fn lab_frame(
        scene: &mut (GameState, RoomState, PlayerState, Pack),
        lua: &mut crate::enemy::LuaEnemyHost,
    ) {
        let (game, room, player, pack) = scene;
        for slot in [1, 2] {
            game.entities[slot].behavior_flags |= 0x20;
        }
        game.clear_voice_playing();
        tick(
            &mut Host {
                game,
                room,
                player,
                pack,
            },
            lua,
            false,
        );
    }

    #[test]
    fn the_lab_computer_boots_into_the_login() {
        let mut scene = lab_scene();
        let mut lua = crate::enemy::LuaEnemyHost::new();
        lab_frame(&mut scene, &mut lua);
        let (game, room, player, _) = &scene;
        // The first frame is `computer_lab_init`: the monitor camera, the
        // parked player and the lab lights.
        assert_eq!(room.current_cut, 4);
        assert_eq!(player.pos[1], -9000);
        assert_eq!(room.ambient, [0x708; 3]);
        assert_eq!(game.entities[1].behavior_flags & 0xc0, 0x40);
        for _ in 0..600 {
            lab_frame(&mut scene, &mut lua);
        }
        let (game, room, _, _) = &scene;
        let state = game
            .interactive
            .state("interactive/computer_lab.lua")
            .unwrap();
        assert_eq!(
            (state.int("func"), state.int("sub")),
            (Some(1), Some(1)),
            "{state:?}"
        );
        // The login's keyboard is up on the keyboard camera.
        assert_eq!(room.current_cut, 5);
        assert!(!game.interactive.sprites().is_empty());
        assert!(
            game.interactive
                .pages()
                .iter()
                .any(|page| page == "ui/umb01.tim")
        );
    }

    /// The statelessness contract the enemy VM keeps: a screen's state lives
    /// in the game state, so dropping the VM before every frame changes
    /// nothing.
    #[test]
    fn the_lab_computer_keeps_no_state_in_the_vm() {
        let mut kept = lab_scene();
        let mut dropped = lab_scene();
        let mut kept_lua = crate::enemy::LuaEnemyHost::new();
        let mut dropped_lua = crate::enemy::LuaEnemyHost::new();
        for frame in 0..400 {
            lab_frame(&mut kept, &mut kept_lua);
            dropped_lua.reset();
            lab_frame(&mut dropped, &mut dropped_lua);
            assert!(kept.0 == dropped.0, "frame {frame}");
        }
    }

    #[test]
    fn a_missing_screen_draws_nothing() {
        let (mut game, mut room, mut player, _) = lab_scene();
        let pack = Pack::from_bytes(crate::pack::PackWriter::new().to_bytes().unwrap()).unwrap();
        let mut lua = crate::enemy::LuaEnemyHost::new();
        tick(
            &mut Host {
                game: &mut game,
                room: &mut room,
                player: &mut player,
                pack: &pack,
            },
            &mut lua,
            false,
        );
        assert!(game.interactive.sprites().is_empty());
        assert_eq!(room.current_cut, 0);
        // Nothing would lower the gate: the engine does, so the game goes on.
        assert!(!game.flag_test(game::BANK_SCENARIO, SCENARIO_INTERACTIVE, false));
    }

    /// A script that errors partway through a screen is backed out of: what
    /// it changed through the API is restored and the gate lowered.
    #[test]
    fn a_failing_screen_is_backed_out_of() {
        let (mut game, mut room, mut player, _) = lab_scene();
        let script = "function display(api, s)\n\
                      s.n = (s.n or 0) + 1\n\
                      if s.n == 1 then\n\
                          api:switch_camera(4)\n\
                          api:set_ambient(1, 2, 3)\n\
                          api.player_y = -9000\n\
                          api:set_enemy_flags(0, 0x40)\n\
                          api:load_texture(0x17, 'ui/umb00.tim', 7)\n\
                      else\n\
                          api:switch_camera(5)\n\
                          error('broken')\n\
                      end\n\
                  end";
        let mut writer = crate::pack::PackWriter::new();
        writer
            .add("interactive/computer_lab.lua", script.as_bytes().to_vec())
            .unwrap();
        let mut tim = vec![0x10, 0, 0, 0, 1, 0, 0, 0];
        tim.extend(12u32.to_le_bytes());
        tim.extend([0, 0, 0, 0, 128, 0, 0, 1]);
        writer.add("ui/umb00.tim", tim).unwrap();
        let pack = Pack::from_bytes(writer.to_bytes().unwrap()).unwrap();
        let before = (room.ambient, player.pos[1], game.entities[1].behavior_flags);
        let mut lua = crate::enemy::LuaEnemyHost::new();
        let mut frame = |game: &mut GameState, room: &mut RoomState, player: &mut PlayerState| {
            tick(
                &mut Host {
                    game,
                    room,
                    player,
                    pack: &pack,
                },
                &mut lua,
                false,
            );
        };
        frame(&mut game, &mut room, &mut player);
        assert_eq!(room.current_cut, 4);
        assert!(game.interactive.textures.contains_key(&0x17));
        frame(&mut game, &mut room, &mut player);
        assert_eq!(room.current_cut, 0);
        assert_eq!(game.camera.current_cut, 0);
        assert_eq!(
            (room.ambient, player.pos[1], game.entities[1].behavior_flags),
            before
        );
        assert!(game.interactive.textures.is_empty());
        assert!(!game.flag_test(game::BANK_SCENARIO, SCENARIO_INTERACTIVE, false));
        // The state keeps its last good value.
        let state = game
            .interactive
            .state("interactive/computer_lab.lua")
            .unwrap();
        assert_eq!(state.int("n"), Some(1));
    }

    #[test]
    fn a_missing_texture_leaves_its_slot_empty() {
        let (mut game, mut room, mut player, _) = lab_scene();
        let script = "function display(api, s)\n\
                      s.loaded = api:load_texture(0x17, 'ui/none.tim', 7)\n\
                      s.slot = api:texture_loaded(0x17)\n\
                  end";
        let mut writer = crate::pack::PackWriter::new();
        writer
            .add("interactive/computer_lab.lua", script.as_bytes().to_vec())
            .unwrap();
        let pack = Pack::from_bytes(writer.to_bytes().unwrap()).unwrap();
        let mut lua = crate::enemy::LuaEnemyHost::new();
        tick(
            &mut Host {
                game: &mut game,
                room: &mut room,
                player: &mut player,
                pack: &pack,
            },
            &mut lua,
            false,
        );
        let state = game
            .interactive
            .state("interactive/computer_lab.lua")
            .unwrap();
        assert_eq!(state.field("loaded"), Some(&ScriptValue::Bool(false)));
        assert_eq!(state.field("slot"), Some(&ScriptValue::Bool(false)));
        // The screen keeps running.
        assert!(game.flag_test(game::BANK_SCENARIO, SCENARIO_INTERACTIVE, false));
    }

    /// The lab script's local functions, for testing them one at a time.
    fn lab_locals(lua: &Lua) -> Table {
        let script = crate::enemy::ENEMY_SCRIPTS
            .iter()
            .find(|(entry, _)| *entry == "interactive/computer_lab.lua")
            .expect("the checked-in lab computer")
            .1;
        let source = format!(
            "{script}\nreturn {{ keyboard_input = keyboard_input, \
             text_begin = text_begin, text_append = text_append }}"
        );
        lua.load(source).eval().unwrap()
    }

    #[test]
    fn the_keyboard_skips_the_covered_cells() {
        let lua = Lua::new();
        let locals = lab_locals(&lua);
        let keyboard_input: mlua::Function = locals.get("keyboard_input").unwrap();
        let press = |col: i64, row: i64, dpad: i64| -> (i64, i64, i64) {
            let s: Table = lua
                .load(format!(
                    "return {{ col = {col}, row = {row}, pad_repeat_latch = 0, \
                     pad_repeat_timer = 0, win = {{ [3] = {{ enabled = 0 }} }} }}"
                ))
                .eval()
                .unwrap();
            let api: Table = lua
                .load(format!(
                    "return {{ pad_pressed = 0, raw_held = {dpad}, raw_edge = {dpad} }}"
                ))
                .eval()
                .unwrap();
            let result: i64 = keyboard_input.call((s.clone(), api)).unwrap();
            (result, s.get("col").unwrap(), s.get("row").unwrap())
        };
        // Right from BACKSPACE skips its second cell onto column 7, which is
        // ENTER's covered lower half, so the row steps up onto ENTER.
        assert_eq!(press(5, 3, 0x2000), (0, 7, 2));
        // Down from ENTER stays on ENTER.
        assert_eq!(press(7, 2, 0x4000), (0, 7, 2));
    }

    #[test]
    fn the_eighth_character_overwrites_in_place() {
        let lua = Lua::new();
        let locals = lab_locals(&lua);
        let begin: mlua::Function = locals.get("text_begin").unwrap();
        let append: mlua::Function = locals.get("text_append").unwrap();
        let s: Table = lua
            .load("return { text = { [0] = { win = 0, x = 0, y = 0, text = {} } } }")
            .eval()
            .unwrap();
        begin.call::<()>((s.clone(), 0)).unwrap();
        let word = || -> Vec<i64> {
            let buffer: Table = s
                .get::<Table>("text")
                .unwrap()
                .get::<Table>(0)
                .unwrap()
                .get("text")
                .unwrap();
            (0..8)
                .map(|i| buffer.get::<i64>(i).unwrap())
                .filter(|key| *key != 0)
                .collect()
        };
        for key in 1..=9 {
            append.call::<()>((s.clone(), key, 0)).unwrap();
        }
        assert_eq!(word(), [1, 2, 3, 4, 5, 6, 7, 9]);
        append.call::<()>((s.clone(), -3, 0)).unwrap();
        assert_eq!(word(), [1, 2, 3, 4, 5, 6, 7]);
    }

    // ---- room 408's numeric panel ------------------------------------------

    const PANEL: &str = "interactive/passcode_panel.lua";

    /// Room 408 with the panel up: the shipped script, six cameras, the
    /// gate raised and system flag 0x1D selecting the panel.
    fn panel_scene() -> (GameState, RoomState, PlayerState, Pack) {
        let script = crate::enemy::ENEMY_SCRIPTS
            .iter()
            .find(|(entry, _)| *entry == PANEL)
            .expect("the checked-in panel")
            .1;
        let mut writer = crate::pack::PackWriter::new();
        writer.add(PANEL, script.as_bytes().to_vec()).unwrap();
        let pack = Pack::from_bytes(writer.to_bytes().unwrap()).unwrap();
        let mut room = RoomState {
            cuts: vec![crate::state::Cut::default(); 6],
            ..RoomState::default()
        };
        room.current_cut = 2;
        let player = crate::player::spawn(
            crate::state::RoomId {
                stage: 4,
                room: 8,
                ..Default::default()
            },
            &room,
        );
        let mut game = GameState::default();
        game.set_camera_cut(2);
        game.message_flags = 0xffff;
        game.apply_flag(game::BANK_SCENARIO, SCENARIO_INTERACTIVE, 0);
        game.apply_flag(game::BANK_SYSTEM, 0x1d, 0);
        (game, room, player, pack)
    }

    /// One frame of the panel with `pressed` (arklay's remapped D-pad word:
    /// up 1, down 2, left 4, right 8, confirm 0x80) and the cancel key.
    fn panel_frame(
        scene: &mut (GameState, RoomState, PlayerState, Pack),
        lua: &mut crate::enemy::LuaEnemyHost,
        pressed: u16,
        cancel: bool,
    ) {
        let (game, room, player, pack) = scene;
        game.dpad_held = pressed;
        game.dpad_pressed = pressed;
        game.mask_toggles.clear();
        game.snd3d_requests.clear();
        tick(
            &mut Host {
                game,
                room,
                player,
                pack,
            },
            lua,
            cancel,
        );
    }

    fn panel_state(game: &GameState) -> &ScriptValue {
        game.interactive.state(PANEL).expect("the panel ran")
    }

    /// The nine lamps as the frame's room-sprite toggles left them.
    fn lamps(game: &GameState) -> [bool; 9] {
        let mut lamps = [false; 9];
        assert_eq!(game.mask_toggles.len(), 9, "one toggle per lamp a frame");
        for toggle in &game.mask_toggles {
            lamps[usize::from(toggle.group) - 1] = toggle.active;
        }
        lamps
    }

    /// Open the panel and answer its prompt: `yes` or no.
    fn opened(
        yes: bool,
    ) -> (
        (GameState, RoomState, PlayerState, Pack),
        crate::enemy::LuaEnemyHost,
    ) {
        let mut scene = panel_scene();
        let mut lua = crate::enemy::LuaEnemyHost::new();
        panel_frame(&mut scene, &mut lua, 0, false);
        scene.0.message.set_menu_choice_id(if yes { 0 } else { 1 });
        panel_frame(&mut scene, &mut lua, 0, false);
        (scene, lua)
    }

    #[test]
    fn the_panel_cuts_to_the_pad_and_asks_its_question() {
        let mut scene = panel_scene();
        let mut lua = crate::enemy::LuaEnemyHost::new();
        panel_frame(&mut scene, &mut lua, 0, false);
        let (game, room, _, _) = &scene;
        assert_eq!(room.current_cut, 5);
        assert_eq!(game.camera.current_cut, 5);
        assert_eq!(room.cuts[5].fov, 0xC0);
        assert_eq!(game.message.id_word(), Some(0x0E), "room message 14");
        assert_ne!(game.message.menu_choice_id() & 0x80, 0);
        assert_eq!(game.message_flags & !0xFEB0, 0, "the player is held");
        assert_eq!(
            lamps(game),
            [false, false, false, false, false, false, false, false, true]
        );
        assert!(
            game.interactive.texts().is_empty(),
            "no labels before the answer"
        );
        // The pad waits while the prompt is up.
        panel_frame(&mut scene, &mut lua, 0x80, false);
        assert_eq!(panel_state(&scene.0).int("sub"), Some(0));
    }

    #[test]
    fn no_at_the_prompt_puts_the_camera_back() {
        let (mut scene, mut lua) = opened(false);
        assert_eq!(panel_state(&scene.0).int("func"), Some(2));
        panel_frame(&mut scene, &mut lua, 0, false);
        let (game, room, _, _) = &scene;
        assert_eq!(room.current_cut, 2);
        assert!(!game.flag_test(game::BANK_SCENARIO, SCENARIO_INTERACTIVE, false));
        assert_eq!(game.message_flags & 0x014F, 0x014F);
        assert!(!game.flag_test(game::BANK_SCENARIO, 0x21, false));
    }

    #[test]
    fn cancel_on_the_pad_closes_it() {
        let (mut scene, mut lua) = opened(true);
        panel_frame(&mut scene, &mut lua, 0, true);
        panel_frame(&mut scene, &mut lua, 0, false);
        assert_eq!(scene.1.current_cut, 2);
        assert!(
            !scene
                .0
                .flag_test(game::BANK_SCENARIO, SCENARIO_INTERACTIVE, false)
        );
    }

    #[test]
    fn the_pad_labels_follow_the_cursor() {
        let (mut scene, mut lua) = opened(true);
        let texts = scene.0.interactive.texts();
        assert_eq!(texts.len(), 2 + 3 * 5);
        let label = |texts: &[Text8x14], digit: u8| {
            texts
                .iter()
                .find(|t| t.bytes == [digit, b'$'])
                .map(|t| (t.x, t.y, t.row))
        };
        assert_eq!(label(texts, b'1'), Some((0x8B, 0x9A, 1)));
        assert_eq!(label(texts, b'9'), Some((0xAB, 0xB6, 3)));
        // Left and up clamp at the corner; right then down moves to key 5.
        for pressed in [4, 1, 8, 2] {
            panel_frame(&mut scene, &mut lua, pressed, false);
        }
        let texts = scene.0.interactive.texts();
        assert_eq!(label(texts, b'5'), Some((0x9B, 0xA8, 1)));
        assert_eq!(label(texts, b'1').map(|l| l.2), Some(3));
    }

    /// The key set that lights every lamp from `lamps`.
    fn solve(lamps: [bool; 9]) -> Vec<usize> {
        let flip = |state: &mut [bool; 9], key: usize| {
            let (row, column) = (key / 3, key % 3);
            state[key] ^= true;
            if row > 0 {
                state[key - 3] ^= true;
            }
            if row < 2 {
                state[key + 3] ^= true;
            }
            if column > 0 {
                state[key - 1] ^= true;
            }
            if column < 2 {
                state[key + 1] ^= true;
            }
        };
        (0u16..512)
            .map(|mask| {
                (0..9)
                    .filter(|key| mask & (1 << key) != 0)
                    .collect::<Vec<_>>()
            })
            .find(|keys| {
                let mut state = lamps;
                for &key in keys {
                    flip(&mut state, key);
                }
                state.iter().all(|&lit| lit)
            })
            .expect("a solution")
    }

    #[test]
    fn lighting_every_lamp_solves_the_panel() {
        let (mut scene, mut lua) = opened(true);
        let start = lamps(&scene.0);
        let mut cursor = (0usize, 0usize);
        for key in solve(start) {
            let target = (key % 3, key / 3);
            while cursor != target {
                let pressed = if cursor.0 < target.0 {
                    cursor.0 += 1;
                    8
                } else if cursor.0 > target.0 {
                    cursor.0 -= 1;
                    4
                } else if cursor.1 < target.1 {
                    cursor.1 += 1;
                    2
                } else {
                    cursor.1 -= 1;
                    1
                };
                panel_frame(&mut scene, &mut lua, pressed, false);
            }
            panel_frame(&mut scene, &mut lua, 0x80, false);
            panel_frame(&mut scene, &mut lua, 0, false);
            let sound = &scene.0.snd3d_requests;
            assert_eq!((sound[0].bank, sound[0].id), (2, 0x17), "the key's sound");
        }
        assert!(lamps(&scene.0).iter().all(|&lit| lit));
        assert!(scene.0.interactive.texts().is_empty(), "the labels go");
        let mut frames = 0;
        while !scene.0.flag_test(game::BANK_SCENARIO, 0x21, false) {
            panel_frame(&mut scene, &mut lua, 0, false);
            frames += 1;
            assert!(frames < 1000, "the animation never finished");
        }
        // 30 frames' wait, the first lamp, the nine lamps' delays and 30 more.
        assert_eq!(frames, 30 + 1 + 92 + 30);
        let sound = &scene.0.snd3d_requests;
        assert_eq!((sound[0].bank, sound[0].id), (2, 0x18), "the success sound");
        assert!(lamps(&scene.0).iter().all(|&lit| !lit));
        panel_frame(&mut scene, &mut lua, 0, false);
        assert_eq!(scene.1.current_cut, 2);
        assert!(
            !scene
                .0
                .flag_test(game::BANK_SCENARIO, SCENARIO_INTERACTIVE, false)
        );
    }

    #[test]
    fn the_initial_lamps_follow_the_variant_and_the_character() {
        for (variant, jill, expected) in [
            (None, true, [0, 0, 0, 0, 0, 0, 0, 0, 1]),
            (Some(0x1e), true, [0, 1, 0, 1, 1, 1, 0, 0, 1]),
            (Some(0x1e), false, [0, 0, 0, 0, 0, 0, 0, 0, 1]),
            (Some(0x1f), true, [1, 0, 1, 0, 1, 1, 0, 0, 1]),
            (Some(0x1f), false, [1, 0, 0, 1, 1, 0, 1, 0, 1]),
        ] {
            let mut scene = panel_scene();
            if let Some(flag) = variant {
                scene.0.apply_flag(game::BANK_SCENARIO, flag, 0);
            }
            scene.0.entities[0].id = u8::from(jill);
            let mut lua = crate::enemy::LuaEnemyHost::new();
            panel_frame(&mut scene, &mut lua, 0, false);
            assert_eq!(
                lamps(&scene.0),
                expected.map(|lamp| lamp == 1),
                "variant {variant:?}, jill {jill}"
            );
        }
    }

    /// The panel waits on room message 14's yes/no answer: every language's
    /// text for it must ask the question, or the pad opens unasked.
    #[test]
    fn every_language_asks_the_panel_question() {
        let xml = include_str!("../games/re1/room/408/room.xml");
        let languages: Vec<&str> = xml.split("<language lang=\"").skip(1).collect();
        assert_eq!(languages.len(), 8);
        for language in languages {
            let lang = &language[..language.find('"').unwrap()];
            let body = &language[..language.find("</language>").unwrap()];
            let start = body
                .find("<string key=\"14\">")
                .unwrap_or_else(|| panic!("{lang}: no message 14"));
            let text = &body[start..];
            let text = &text[..text.find("</string>").unwrap()];
            assert!(text.contains("{branch "), "{lang}: {text}");
        }
    }

    #[test]
    fn a_state_table_round_trips() {
        let lua = Lua::new();
        let table: Table = lua
            .load("return { func = 1, win = { [0] = { alpha = -3 } }, name = 'x', on = true, f = 0.5 }")
            .eval()
            .unwrap();
        let tree = from_lua(&table).unwrap();
        let back = from_lua(&to_lua(&lua, &tree).unwrap()).unwrap();
        assert_eq!(tree, back);
        assert_eq!(tree.int("func"), Some(1));
        let bad: Table = lua.load("return { f = function() end }").eval().unwrap();
        assert!(from_lua(&bad).is_err());
        // A table in two places would come back as two copies.
        let shared: Table = lua
            .load("local t = {} return { a = t, b = t }")
            .eval()
            .unwrap();
        assert!(from_lua(&shared).is_err());
        let cycle: Table = lua.load("local t = {} t.self = t return t").eval().unwrap();
        assert!(from_lua(&cycle).is_err());
        // A Lua string is bytes, UTF-8 or not.
        let bytes: Table = lua
            .load("return { ['\\xff'] = '\\x80\\x00' }")
            .eval()
            .unwrap();
        let tree = from_lua(&bytes).unwrap();
        assert_eq!(
            tree,
            ScriptValue::Table(vec![(
                ScriptKey::Str(vec![0xff]),
                ScriptValue::Str(vec![0x80, 0])
            )])
        );
        assert_eq!(from_lua(&to_lua(&lua, &tree).unwrap()).unwrap(), tree);
    }
}
