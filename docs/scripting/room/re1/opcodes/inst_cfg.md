# `inst_cfg`

Rewrites one of the room's collision rectangles.

| | |
| --- | --- |
| Opcode | `0x30` |
| Group | Objects and models |
| Kind | Command |
| Size | 12 bytes |

## Syntax

```c
inst_cfg(list, record, flags, x_min, z_min, x_max, z_max);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `list` | `u8` | The collision list (`COL_LIST_xx`), 0 to 3: the room's collision is split into four lists, one per quadrant around a centre point. |
| `record` | `u8` | The rectangle in that list (`COL_xx`, a room define). |
| `flags` | `u8` | The rectangle's four collision flag bits (`CF_x`); 0 leaves its flags as they are. |
| `x_min` | `u16` | The rectangle's lower x. |
| `z_min` | `u16` | The rectangle's lower z. |
| `x_max` | `u16` | The rectangle's upper x. |
| `z_max` | `u16` | The rectangle's upper z. |

## Behaviour

`inst_cfg` writes the rectangle's corners, and when `flags` is not 0 puts
its low four bits in bits 8 to 11 of the rectangle's flags word, keeping
the rest. A list or record that does not exist is ignored. The flag bits
have no meaning-based names yet; the shipped scripts write `CF_2 | CF_1`.

The original writes the loaded room data in place. The port queues the
rewrite and applies it once the frame's scripts have run, before the
player's physics, so it takes effect on the same frame. The rewrite lasts
until the room is left: the next visit loads the room's own rectangles,
so a room re-applies it from its init or main when a flag says so.

The `.s` assembler prints the instruction as raw bytes; `.akscr` takes one
argument per field.

## Example

```c
if (ck(FG_COMMON, F_COMMON_CLOSET_ZOMBIE_RELEASED, TRUE) && ck(FG_ROOM, F_ROOM_CLOSET_OPEN_COLLISION, FALSE))
{
    set(FG_ROOM, F_ROOM_CLOSET_OPEN_COLLISION, TRUE);
    inst_cfg(COL_LIST_03, COL_08, CF_2 | CF_1, 1000, 5630, 5000, 7000);
    inst_cfg(COL_LIST_03, COL_09, CF_2 | CF_1, 1000, 100, 5000, 2400);
}
```

Room 10e (`games/re1/room/10e/room.akscr`): once the zombie has come out
of the closet, the main script reshapes two rectangles for the open
closet, once per visit.

## See also

- [`obj`](obj.md): sets up a room object and its collision box.
