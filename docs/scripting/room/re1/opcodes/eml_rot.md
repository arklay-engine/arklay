# `eml_rot`

Turns an item model, or a room object, about its x and z axes.

| | |
| --- | --- |
| Opcode | `0x3B` |
| Group | Objects and models |
| Kind | Command |
| Size | 6 bytes |

## Syntax

```c
eml_rot(model, rotation_x, rotation_z);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `model` | `u8` | An item model (`IMDL_xx`), or `ROT_OBJ \| OBJ_xx` for a room object. |
| `rotation_x` | `i16` | The rotation about x, 4096 to a full turn. |
| `rotation_z` | `i16` | The rotation about z. |

## Behaviour

`eml_rot` writes the x and z rotation of the record and keeps its y
rotation, the yaw [`item_aot_set`](item_aot_set.md) or [`obj`](obj.md)
gave it. With `ROT_OBJ` (`0x80`) set, the low seven bits name a room
object; otherwise the byte names an item model.

The record must have been built and not be hidden: when its flags byte is
0 the instruction does nothing. So in an init it comes after the `obj` or
`item_aot_set` that builds the record, and on an item that is already
taken (built hidden) it has no effect.

The `.s` assembler prints the instruction as raw bytes; `.akscr` takes one
argument per field.

## Example

```c
item_aot_set(AOT_01_ITEM_KEY_BROKEN_SHOTGUN_0, 6500, 9100, 500, 900, IT_1C_KEY_BROKEN_SHOTGUN, 1, IMDL_00_KEY_BROKEN_SHOTGUN, SCA_NONE, 6840, -2250, 9670, -2000, F_ITEM_102_KEY_BROKEN_SHOTGUN_0, SAT_MANUAL | SAT_PLAYER, 0);
eml_rot(IMDL_00_KEY_BROKEN_SHOTGUN, -100, -1100);
```

```c
obj(OBJ_00_NOTE, OF_ACTIVE, SCA_NONE, 10720, -2350, 15120, 0, 0, 0, 0, 0, 0, 0, 0, 0);
eml_rot(ROT_OBJ | OBJ_00_NOTE, -2048, 1024);
```

Room 102 (`games/re1/room/102/room.akscr`) tilts the broken shotgun on its
rack; room 10b lays a note object flat.

## See also

- [`eml_pos`](eml_pos.md): places and turns a room object on every axis.
- [`obj`](obj.md), [`item_aot_set`](item_aot_set.md): build the records.
