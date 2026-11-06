//! RE 2's MAP page: which floor's map it shows and what it draws there
//! (`0x4FA990`), from the executable's floor and room tables.
//!
//! A floor (`0x691F7A`) is one map picture, `common\\file\\map<nn>.adt`, and
//! the rooms drawn on it. [`floor_of`] (`0x4FAF80`) finds the player's floor
//! from the stage and the room, and [`record_of`] (`0x4FB1C0`) the room's
//! record on a floor. A room is drawn once it is visited (bank 7, the
//! index [`visited_index`] gives, which the room load raises) or once the
//! player holds the floor's map (bank 31); a visited room gets a
//! translucent fill over its outline, in the colour of its state.
//!
//! The floor change (up and down, on a floor with others above or below it)
//! and the arrows that show it are not ported: no route floor has them.

use super::super::GameState;

/// The floors in the tables.
pub const FLOORS: usize = 20;
/// The rooms' visited bits (`0x98EC04`).
pub const VISITED_BANK: u8 = 7;
/// The bank of a room's [`MapRoom::fill`] bit (`0x98EC18`).
pub const FILL_BANK: u8 = 30;
/// The bank of a floor's map, [`Floor::held`] (`0x98EC1C`).
pub const HELD_BANK: u8 = 31;
/// The scripts' bank the floor test reads (`0x98EB4C`).
const ROOM_BANK: u8 = 2;

/// The texture slots the map's five pictures load into (`0x4FA240`'s sub
/// 0), in the order of the ADT's TIMs.
pub const OUTLINE: u8 = 0x1D;
pub const OUTLINE_LIT: u8 = 0x21;
pub const FILL: u8 = 0x20;
pub const SIGNS: u8 = 0x1E;
pub const ARROWS: u8 = 0x1F;

/// A room on a floor's map (16 bytes in the executable).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapRoom {
    /// Its rectangle in the map's pictures.
    pub u: u8,
    pub v: u8,
    pub w: u8,
    pub h: u8,
    /// Where it is drawn.
    pub x: i16,
    pub y: i16,
    /// The player's marker's origin in this room: the marker is drawn at
    /// this plus the player's position over 450.
    pub marker: [i16; 2],
    /// Its visited bit in [`VISITED_BANK`].
    pub visit: u8,
    /// Its bit in [`FILL_BANK`], which lights its outline and its fill.
    pub fill: u8,
    /// The scenario bits that hide it while unvisited (`+E` for the A
    /// scenario, `+F` for the B one): `1` outside, `2` inside the state
    /// `0x989ED0 & 0x40000000` names.
    pub scenario: [u8; 2],
}

/// A floor's map (`8` bytes a floor).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Floor {
    pub rooms: &'static [MapRoom],
    /// Its bit in [`HELD_BANK`]: the player holds the floor's map.
    pub held: u8,
    /// Its picture, `map<nn>`.
    pub map: u16,
}

#[allow(clippy::too_many_arguments)]
const fn room(
    u: u8,
    v: u8,
    w: u8,
    h: u8,
    x: i16,
    y: i16,
    mx: i16,
    my: i16,
    visit: u8,
    fill: u8,
    a: u8,
    b: u8,
) -> MapRoom {
    MapRoom {
        u,
        v,
        w,
        h,
        x,
        y,
        marker: [mx, my],
        visit,
        fill,
        scenario: [a, b],
    }
}

include!("map_data.rs");

/// The state the floor and room tests read that the port does not model:
/// the executable's `0x989FF6` and `0x989ED0`, the word at `0x98EB1A` and
/// the byte at `0x692FAC`. [`MapContext::default`] is a new game's.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct MapContext {
    /// `0x989FF6`.
    pub player: u8,
    /// `0x989ED0`: bit 31 picks the floor table, bit 30 the scenario
    /// rooms.
    pub status: u32,
    /// Bit 0 of `0x989EF8`: the B scenario's bits of [`MapRoom::scenario`].
    pub scenario_b: bool,
    /// `0x98EB1A`.
    pub word: u16,
    /// `0x692FAC`.
    pub byte: u8,
}

/// The floor tables `0x509CA0` picks between.
pub fn floors(context: &MapContext) -> &'static [Floor; FLOORS] {
    if context.status & 0x8000_0000 == 0 {
        &LEON_FLOORS
    } else {
        &OTHER_FLOORS
    }
}

/// `0x4E4040`: room `room`'s visited bit, from its stage (0-based).
pub fn visited_index(stage: u8, room: u8) -> u8 {
    const BASES: [u8; 7] = [0, 0x1E, 0x3A, 0x48, 0x59, 0x63, 0x7B];
    BASES
        .get(usize::from(stage))
        .map_or(stage, |&base| base)
        .wrapping_add(room)
}

/// `0x4FAF80`: the floor of room `room` of stage `stage` (0-based).
pub fn floor_of(stage: u8, room: u8, context: &MapContext, flag: impl Fn(u8, u8) -> bool) -> u8 {
    let player = context.player;
    let scenario = context.status & 0x4000_0000 != 0;
    match stage {
        0 => match room {
            0x11 | 0x17 => 4,
            0x12 if player == 3 => 4,
            0x12 => 3,
            0..=3 => 0,
            4..=7 => 1,
            8..=0x15 => 3,
            0x16 => 2 + u8::from(player == 4),
            _ => 0,
        },
        1 => match room {
            0x1B => 3,
            0..=0x10 => 2,
            _ => 5,
        },
        2 => {
            if room <= 1 || room == 8 {
                5
            } else {
                6
            }
        }
        3 => match room {
            0 | 1 | 3 => {
                if player < 4 {
                    8
                } else {
                    7
                }
            }
            2 | 9 | 0xB => 7,
            _ => 8,
        },
        4 => match room {
            0..=2 | 8 => 9,
            3 if scenario => {
                if flag(ROOM_BANK, 0x62) {
                    10
                } else {
                    11
                }
            }
            5 => {
                if flag(ROOM_BANK, 0xBE) {
                    if scenario { 0xD } else { 0x10 }
                } else if flag(ROOM_BANK, 0x63) {
                    0xC
                } else {
                    0xA
                }
            }
            0..=4 => 0xA,
            _ => 0xB + u8::from(room == 9),
        },
        5 => match room {
            1 => {
                if context.word == 0x602 {
                    0xD
                } else {
                    0x10
                }
            }
            3 => {
                if player >= 3 {
                    0xD
                } else {
                    0xE
                }
            }
            0xE if context.byte == 1 => 0x11,
            0xE => 0x10,
            0..=4 => 0xD,
            5 => 0xF,
            0x12.. => 0x11,
            _ => 0x10,
        },
        6 => 0x12 + u8::from(flag(ROOM_BANK, 0x89)),
        _ => 0,
    }
}

/// `0x4FB1C0`: room `room`'s record on floor `floor`, which may be past the
/// floor's rooms.
pub fn record_of(floor: u8, room: u8) -> u8 {
    let room = room as u32;
    let record = match floor {
        0 => {
            if room < 0x18 {
                room
            } else {
                room.wrapping_sub(0x14)
            }
        }
        1 => room.wrapping_sub(4),
        2 => {
            if room == 0x16 {
                0x11
            } else {
                room
            }
        }
        3 => match room {
            0x1B => 0xE,
            0x12.. => room - 9,
            _ => room.wrapping_sub(8),
        },
        4 => {
            if room == 0x17 {
                0
            } else {
                room.wrapping_sub(0x10)
            }
        }
        5 => match room {
            8 => 2,
            0x19.. => room - 0x10,
            0..=0x10 => room,
            _ => room - 0xE,
        },
        6 => {
            if room < 9 {
                room.wrapping_sub(2)
            } else {
                room - 3
            }
        }
        7 => match room {
            9 => 4,
            0xB => 5,
            _ => room,
        },
        8 => match room {
            0..=1 => room,
            2..=5 => room - 1,
            6..=8 => room - 2,
            9..=0xA => room - 3,
            0xB..=0x10 => room - 4,
            _ => room,
        },
        9 => {
            if room == 8 {
                3
            } else {
                room
            }
        }
        10 => room.wrapping_sub(3),
        11 => {
            if room == 3 {
                0
            } else {
                room.wrapping_sub(5)
            }
        }
        12 => u32::from(room != 9),
        13 | 18 => room,
        16 => match room {
            5 => 0xC,
            1 => 0xB,
            0x11 => 2,
            _ => room.wrapping_sub(6),
        },
        17 => {
            if room == 0xE {
                6
            } else {
                room.wrapping_sub(0x12)
            }
        }
        19 => u32::from(room != 3),
        _ => 0,
    };
    record as u8
}

/// One sprite of the map, from a picture's slot (see [`OUTLINE`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapSprite {
    pub texture: u8,
    pub u: u8,
    pub v: u8,
    pub clut: u8,
}

/// A room as the page draws it: its fill, translucent, then its outline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrawnRoom {
    pub x: i16,
    pub y: i16,
    pub w: u8,
    pub h: u8,
    pub fill: Option<MapSprite>,
    pub outline: MapSprite,
}

/// The player's marker: a 12x12 cell of texture 0x14, translucent, its
/// colour the page's blink plus 40.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Marker {
    pub x: i16,
    pub y: i16,
    pub u: u8,
    pub v: u8,
}

/// What the MAP page draws on floor `floor`: its rooms in the draw order
/// and the player's marker when the player is on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapView {
    pub floor: u8,
    pub map: u16,
    pub rooms: Vec<DrawnRoom>,
    pub marker: Option<Marker>,
}

impl GameState {
    /// The MAP page's view of the player's floor (`0x4FA990`).
    pub fn re2_map_view(&self) -> MapView {
        let context = MapContext {
            scenario_b: self.re2.scenario_b,
            ..MapContext::default()
        };
        let stage = self.id.stage.saturating_sub(1);
        let room = self.id.room;
        let flag = |bank: u8, index: u8| self.re2.flag(bank, index);
        let floor = floor_of(stage, room, &context, flag);
        let player = &self.entities[0];
        map_view(
            floor,
            floor,
            room,
            [player.pos[0], player.pos[2]],
            player.angle,
            &context,
            flag,
        )
    }
}

/// `0x4FA990`'s drawing of floor `floor` with the player in room `room` of
/// floor `player_floor`, at `(x, z)` facing `angle`.
pub fn map_view(
    floor: u8,
    player_floor: u8,
    room: u8,
    [x, z]: [i32; 2],
    angle: u16,
    context: &MapContext,
    flag: impl Fn(u8, u8) -> bool,
) -> MapView {
    let table = floors(context);
    let Some(entry) = table.get(usize::from(floor)) else {
        return MapView {
            floor,
            map: 0,
            rooms: Vec::new(),
            marker: None,
        };
    };
    let here = record_of(floor, room);
    let marker = (floor == player_floor)
        .then(|| entry.rooms.get(usize::from(here)))
        .flatten()
        .map(|record| {
            // `(v + 28000) / 450`, rounded toward zero, down the screen for z.
            let x = ((x + 28000) / 450) as i16;
            let y = (-(z + 28000) / 450) as i16;
            let direction = ((u32::from(angle) + 0x100) >> 9) & 7;
            let (u, v) = if direction < 4 {
                (12 * direction, 0)
            } else {
                (12 * direction - 48, 12)
            };
            Marker {
                x: x.wrapping_add(record.marker[0]),
                y: y.wrapping_add(record.marker[1]),
                u: u as u8,
                v,
            }
        });
    let held = flag(HELD_BANK, entry.held);
    let mut rooms = Vec::new();
    for (index, record) in entry.rooms.iter().enumerate() {
        let mut drawn = DrawnRoom {
            x: record.x,
            y: record.y,
            w: record.w,
            h: record.h,
            fill: None,
            outline: MapSprite {
                texture: OUTLINE,
                u: record.u,
                v: record.v,
                clut: 0,
            },
        };
        if floor == 2 && index == 0xE {
            if let Some(room) = special_room(record, room == 0xE, held, &flag) {
                rooms.push(room);
            }
            continue;
        }
        let lit = flag(FILL_BANK, record.fill);
        if lit {
            drawn.outline.texture = OUTLINE_LIT;
        }
        let visited = flag(VISITED_BANK, record.visit);
        if visited {
            let clut =
                2 * u8::from(lit) + u8::from(floor == player_floor && index == usize::from(here));
            drawn.fill = Some(MapSprite {
                texture: FILL,
                u: record.u,
                v: record.v,
                clut,
            });
        } else {
            if !held {
                continue;
            }
            let bits = record.scenario[usize::from(context.scenario_b)];
            let inside = context.status & 0x4000_0000 != 0;
            if (bits & 1 != 0 && !inside) || (bits & 2 != 0 && inside) {
                continue;
            }
        }
        rooms.push(drawn);
    }
    MapView {
        floor,
        map: entry.map,
        rooms,
        marker,
    }
}

/// Floor 2's room 0xE (`0x4FAC2C`), drawn from its own pieces by the
/// state of its four bits in [`FILL_BANK`]. Unverified: no recording
/// reaches it.
fn special_room(
    record: &MapRoom,
    inside: bool,
    held: bool,
    flag: &impl Fn(u8, u8) -> bool,
) -> Option<DrawnRoom> {
    let bit = |index: u8| flag(FILL_BANK, index);
    let u = record.u;
    let piece = |texture: u8, u: u8, v: u8, clut: u8| MapSprite {
        texture,
        u,
        v,
        clut,
    };
    let fill = if inside {
        // Without bit 8 or bit 9 the routine draws the primitive as it
        // finds it: the outline's picture and rectangle, its CLUT the
        // player's record.
        let mut fill = piece(OUTLINE, u, record.v, 0xE);
        if bit(8) {
            fill = if bit(0xC) {
                piece(FILL, u, 0x50, 2)
            } else {
                piece(ARROWS, 0x20, 0, 1)
            };
        }
        if bit(9) {
            fill = if bit(0xB) {
                piece(FILL, u, 0x98, 2)
            } else {
                piece(ARROWS, 0, 0, 1)
            };
        }
        fill
    } else {
        if flag(VISITED_BANK, 0x2C) && !held {
            return None;
        }
        if bit(0xB) {
            if bit(0xC) {
                piece(FILL, u, 0x98, 0)
            } else {
                piece(ARROWS, 0x20, 0, 1)
            }
        } else if bit(0xC) {
            piece(ARROWS, 0, 0, 1)
        } else {
            piece(OUTLINE, u, 0x50, 0)
        }
    };
    Some(DrawnRoom {
        x: record.x,
        y: record.y,
        w: record.w,
        h: record.h,
        fill: Some(fill),
        outline: piece(OUTLINE, u, 0x50, 0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_visited_bits_count_from_each_stages_base() {
        assert_eq!(visited_index(0, 2), 2);
        assert_eq!(visited_index(1, 0), 0x1E);
        assert_eq!(visited_index(6, 3), 0x7E);
    }

    #[test]
    fn the_route_rooms_are_on_the_citys_floor() {
        let none = |_: u8, _: u8| false;
        let context = MapContext::default();
        for room in 0..=3 {
            assert_eq!(floor_of(0, room, &context, none), 0, "room {room}");
            assert_eq!(record_of(0, room), room);
        }
        assert_eq!(floor_of(0, 0x11, &context, none), 4);
        assert_eq!(
            floor_of(4, 5, &context, |bank, index| (bank, index) == (2, 0x63)),
            0xC
        );
        assert_eq!(record_of(0, 0x18), 4);
        assert_eq!(record_of(1, 4), 0);
    }

    /// `status_map`: room 102, visited with room 100, the marker where the
    /// original drew it.
    #[test]
    fn room_102_fills_the_visited_rooms_and_marks_the_player() {
        let visited = |bank: u8, index: u8| bank == VISITED_BANK && matches!(index, 0 | 2);
        let view = map_view(
            0,
            0,
            2,
            [-11800, -4600],
            0x400,
            &MapContext::default(),
            visited,
        );
        assert_eq!(view.map, 0);
        // Nothing else is drawn: the city's map isn't held.
        assert_eq!(view.rooms.len(), 2);
        let here = view.rooms[1];
        assert_eq!((here.x, here.y), (81, 112));
        assert_eq!(here.fill.map(|fill| fill.clut), Some(1));
        assert_eq!(view.rooms[0].fill.map(|fill| fill.clut), Some(0));
        // Facing 0x400: direction 2, the first row's third cell.
        assert_eq!(
            view.marker,
            Some(Marker {
                x: 111,
                y: 164,
                u: 24,
                v: 0
            })
        );
        // On another floor there's no marker.
        assert_eq!(
            map_view(1, 0, 2, [0, 0], 0, &MapContext::default(), visited).marker,
            None
        );
    }
}
