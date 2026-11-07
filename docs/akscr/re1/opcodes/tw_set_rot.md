# `tw_set_rot`

Sets the selection's rotation steps, which `tw_rot_add` adds.

| | |
| --- | --- |
| Opcode | `0x06` |
| Group | Event: tween state |
| Kind | Event instruction |
| Size | 4 bytes |
| Valid in | the tween state |

## Syntax

```c
tw_set_rot(dx, dy, dz);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `dx` | `i8` | The step about x (pitch), in 4096ths of a turn. |
| `dy` | `i8` | The step about y (yaw). |
| `dz` | `i8` | The step about z (roll). |

## Behaviour

Stores the three steps; it does not turn anything.
[`tw_rot_add`](tw_rot_add.md) and [`tw_pos_rot_add`](tw_pos_rot_add.md) add
them to the rotation. It does not yield.

The z step is not a field of its own: it is stored, sign-extended, in the
selection's `+0x84` state word, whose low byte is the state and high byte
the ignore byte. On a room object or item model that is only the record's
tween scratch. On a character, `tw_set_rot` therefore also sets its state
(to `dz`, and its ignore byte to 0, or to `0xFF` for a negative `dz`), and
an actor or [`tw_set_field`](tw_set_field.md) write to the state changes
the roll step. Use rotation steps on characters with care; on objects they
are safe.

## Example

```c
evt_work_set(WK_OBJ, OBJ_00_STATUE);
evt_tween_begin_alt();
tw_set_rot(0, 0, 9);
tw_rot_add();
evt_next();
tw_set_rot(0, 0, 11);
tw_rot_add();
evt_next();
```

Room 202 (`games/re1/room/202/room.akscr`), event `statue_falls`: the
statue's roll speeds up a little each frame.

## See also

- [`tw_rot_add`](tw_rot_add.md): adds the steps.
- [`tw_abs_rot`](tw_abs_rot.md): sets the rotation outright.
