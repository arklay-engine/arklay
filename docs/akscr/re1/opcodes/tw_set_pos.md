# `tw_set_pos`

Sets the selection's position step, which `tw_pos_add` adds.

| | |
| --- | --- |
| Opcode | `0x05` |
| Group | Event: tween state |
| Kind | Event instruction |
| Size | 4 bytes |
| Valid in | the tween state |

## Syntax

```c
tw_set_pos(dx, dy, dz);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `dx` | `i8` | The x step, -128 to 127. |
| `dy` | `i8` | The y step (negative is up). |
| `dz` | `i8` | The z step. |

## Behaviour

Stores the three steps; it does not move anything.
[`tw_pos_add`](tw_pos_add.md) and [`tw_pos_rot_add`](tw_pos_rot_add.md)
add them to the position. The steps stay until the next `tw_set_pos`, also
across tween sequences. On a character they are the entity's movement-step
words; on a room object or item model, the record's tween scratch. It does
not yield.

## Example

```c
evt_work_set(WK_OBJ, OBJ_00_FALLING_FRAME);
evt_tween_begin_alt();
tw_set_pos(0, 120, 0);
tw_pos_rot_add();
evt_next();
tw_set_pos(0, 100, 0);
```

Room 117 (`games/re1/room/117/room.akscr`), event `frame_falls`: the
falling frame's speed changes from one frame to the next.

## See also

- [`tw_pos_add`](tw_pos_add.md): adds the step.
- [`tw_set_rot`](tw_set_rot.md): the rotation step.
