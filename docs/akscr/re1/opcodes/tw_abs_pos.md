# `tw_abs_pos`

Moves the selection to a position.

| | |
| --- | --- |
| Opcode | `0x07` |
| Group | Event: tween state |
| Kind | Event instruction |
| Size | 8 bytes |
| Valid in | the tween state |

## Syntax

```c
tw_abs_pos(0, x, y, z);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| *pad* | `u8` | Unused, written as 0. |
| `x` | `i16` | The x position. |
| `y` | `i16` | The y position (negative is up). |
| `z` | `i16` | The z position. |

## Behaviour

Sets the selection's position at once, without collision. It works on a
character, a room object or an item model, and does not yield. The steps
are left as they are.

## Example

```c
evt_work_set(WK_OBJ, OBJ_00_WATER);
evt_tween_begin_alt();
tw_abs_pos(0, 6280, 160, 7920);
tw_finish();
```

Room 113 (`games/re1/room/113/room.akscr`), event `place_water`.

## See also

- [`tw_abs_rot`](tw_abs_rot.md): sets the rotation.
- [`tw_set_sel`](tw_set_sel.md): sets one axis.
- [`pos_set`](pos_set.md): places a character from command code.
