# `obj`

Sets up a room object: its model, flags, attachment, position, floor probe and collision box.

| | |
| --- | --- |
| Opcode | `0x1F` |
| Group | Objects and models |
| Kind | Command |
| Size | 28 bytes |

## Syntax

```c
obj(slot, flags, parent, x, y, z, yaw,
    probe_x0, probe_z0, probe_x1, probe_z1,
    radius, height, half_x, half_z);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `slot` | `u8` | The object (`OBJ_xx`, a room define, in the low six bits), ORed with `OBJ_PUSH_GRUNT` (`0x40`) for the grunt a pushed object makes, or `OBJ_TEX_QUEUE` (`0x80`). |
| `flags` | `u8` | The object flags (`OF_*`): `OF_ACTIVE` draws it; `OF_INTANGIBLE`, `OF_NO_FLOOR_PROBE`, `OF_NO_COLLISION`, `OF_NOT_PUSHABLE`, `OF_CLIMBABLE`, and `OF_xx` for the bits not yet named. |
| `parent` | `u8` | What it is attached to: `SCA_NONE`, `SCA_PLAYER`, another object (`OBJ_xx`), or `SCA_ENEMY \| ENEMY_xx`. |
| `x`, `y`, `z` | `i16` | The position (negative y is up). |
| `yaw` | `i16` | The facing angle, 0 to 4095 for a full turn. |
| `probe_x0`, `probe_z0` | `i16` | The first end of the floor probe, relative to the object. |
| `probe_x1`, `probe_z1` | `i16` | The second end of the floor probe. |
| `radius` | `i16` | The radius the entities collide with. |
| `height` | `i16` | The collision box's half height. |
| `half_x`, `half_z` | `i16` | The collision box's half widths in x and z. |

## Behaviour

`obj` builds the object record of slot `slot & 0x3F` afresh: every field
of the record is rewritten, its rotation is `(0, yaw, 0)`, and its push
counter (see [`ck_anim`](ck_anim.md)) starts at 0. A slot past the number
of objects the room declares is ignored. Objects are built in the room's
init, and the model drawn is the room's model of the same slot.

An object attached to another object or to the player rides its parent's
position and rotation (the chain is followed up to 20 links). An enemy
parent is recorded but not modelled: the object then stands on its own.

The original special-cases a few rooms, and the port follows: it moves
some objects a little (slots in the front lesson room, the guardhouse
save room, the security room and Jill's dining room on the first visit),
greys or lifts the palette of the greenhouse and front lesson room models
when slot 0 is built, and draws the water tank's surface in the west-wing
study at a different opacity. `OBJ_TEX_QUEUE` asks the original for
texture processing the port has no counterpart for, so it has no effect.

The `.s` assembler prints the probe and box words as bytes; `.akscr` takes
one argument per field.

## Example

```c
obj(OBJ_PUSH_GRUNT | OBJ_00_STEP, OF_CLIMBABLE | OF_ACTIVE, SCA_NONE, 12500, 0, 4600, 0, 285, 0, -285, 0, 725, 900, 990, 800);
```

Room 107 (`games/re1/room/107/room.akscr`) sets up the step the player
pushes and climbs: drawn, climbable, with the push grunt.

## See also

- [`objtbl_b_set`](objtbl_b_set.md): rewrites an object's flags.
- [`eml_pos`](eml_pos.md): places and turns an object.
- [`eml_rot`](eml_rot.md): turns an object.
- [`ck_anim`](ck_anim.md): tests how long the player has pushed it.
