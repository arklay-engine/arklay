# `eml_pos`

Places and turns a room object.

| | |
| --- | --- |
| Opcode | `0x47` |
| Group | Objects and models |
| Kind | Command |
| Size | 14 bytes |

## Syntax

```c
eml_pos(object, rotation_x, rotation_y, rotation_z, x, y, z);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `object` | `u8` | The room object (`OBJ_xx`, a room define). |
| `rotation_x`, `rotation_y`, `rotation_z` | `i16` | The rotation, 4096 to a full turn on each axis. |
| `x`, `y`, `z` | `i16` | The position (negative y is up). |

## Behaviour

`eml_pos` replaces the object's rotation and position, both its live
position and the 16-bit copy the push code commits, with no movement in
between. It works whether or not the object is drawn; an object past the
room's records is ignored. An object attached to a parent (see
[`obj`](obj.md)) keeps its parent, so the values are relative to it.

The `.s` assembler prints the instruction as raw bytes; `.akscr` takes one
argument per field.

## Example

```c
obj(OBJ_02_RED_EYE, OF_NOT_PUSHABLE | OF_10 | OF_NO_COLLISION | OF_NO_FLOOR_PROBE | OF_INTANGIBLE | OF_ACTIVE, OBJ_00_TIGER_STATUE, 510, -2310, -110, 0, 0, 0, 0, 0, 0, 0, 0, 0);
eml_rot(ROT_OBJ | OBJ_02_RED_EYE, -80, 230);
if (ck(FG_SCENARIO, F_SCENARIO_TIGER_MAGNUM_TAKEN, FALSE))
{
    if (ck(FG_SCENARIO, F_SCENARIO_TIGER_RED_JEWEL_SET, FALSE))
    {
        eml_pos(OBJ_02_RED_EYE, 0, 0, 0, 0, -30000, 0);
    }
}
```

```c
eml_pos(OBJ_02_RED_EYE, -80, 0, 230, 510, -2310, -110);
```

Room 10d (`games/re1/room/10d/room.akscr`), the tiger statue. The red eye
is attached to the statue. While the red jewel is not set, the init parks
the eye far out of sight (y -30000); when the player sets the jewel, the
main script puts it back in the statue's socket.

## See also

- [`eml_rot`](eml_rot.md): turns an object or item model about x and z.
- [`obj`](obj.md): sets up a room object.
