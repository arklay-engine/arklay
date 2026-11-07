# `tw_pos_rot_add`

Adds both the position step and the rotation step to the selection.

| | |
| --- | --- |
| Opcode | `0x04` |
| Group | Event: tween state |
| Kind | Event instruction |
| Size | 1 byte |
| Valid in | the tween state |

## Syntax

```c
tw_pos_rot_add();
```

## Parameters

None.

## Behaviour

[`tw_pos_add`](tw_pos_add.md) and [`tw_rot_add`](tw_rot_add.md) in one:
adds the [`tw_set_pos`](tw_set_pos.md) step to the position and the
[`tw_set_rot`](tw_set_rot.md) steps to the rotation, once. It does not
yield.

## Example

```c
evt_work_set(WK_OBJ, OBJ_00_FALLING_FRAME);
evt_tween_begin_alt();
tw_set_pos(0, 40, 0);
tw_set_rot(80, 0, 0);
tw_pos_rot_add();
evt_next();
tw_set_rot(40, 0, 0);
tw_pos_rot_add();
evt_next();
```

Room 117 (`games/re1/room/117/room.akscr`), event `frame_falls`: the
picture frame drops and swings about x.

## See also

- [`tw_pos_add`](tw_pos_add.md), [`tw_rot_add`](tw_rot_add.md): one of the
  two.
