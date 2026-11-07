# `tw_abs_rot`

Sets the selection's rotation.

| | |
| --- | --- |
| Opcode | `0x0B` |
| Group | Event: tween state |
| Kind | Event instruction |
| Size | 8 bytes |
| Valid in | the tween state |

## Syntax

```c
tw_abs_rot(0, x, y, z);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| *pad* | `u8` | Unused, written as 0. |
| `x` | `i16` | The rotation about x (pitch), in 4096ths of a turn. |
| `y` | `i16` | The rotation about y (yaw, the facing). |
| `z` | `i16` | The rotation about z (roll). |

## Behaviour

Sets all three angles at once. It works on a character (for whom `y` is
the facing), a room object or an item model, and does not yield. The steps
are left as they are.

## Example

```c
evt_work_set(WK_OBJ, OBJ_00_FALLING_FRAME);
evt_tween_begin_alt();
sleep(1);
tw_abs_pos(0, 3440, 300, 3310);
tw_abs_rot(0, 0, -120, 1024);
tw_end();
```

Room 117 (`games/re1/room/117/room.akscr`), event `frame_falls`: the
fallen frame comes to rest on its side.

## See also

- [`tw_abs_pos`](tw_abs_pos.md): sets the position.
- [`tw_set_rot`](tw_set_rot.md): sets the rotation steps.
