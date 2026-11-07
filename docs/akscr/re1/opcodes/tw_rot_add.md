# `tw_rot_add`

Adds the rotation step to the selection's rotation.

| | |
| --- | --- |
| Opcode | `0x03` |
| Group | Event: tween state |
| Kind | Event instruction |
| Size | 1 byte |
| Valid in | the tween state |

## Syntax

```c
tw_rot_add();
```

## Parameters

None.

## Behaviour

Adds the three steps that [`tw_set_rot`](tw_set_rot.md) set to the
selection's rotation about x, y and z, once, in 4096ths of a turn. It does
not yield. The z step is the selection's `+0x84` state word (see
`tw_set_rot`). It works on a character, a room object or an item model.

## Example

```c
evt_work_set(WK_OBJ, OBJ_00_STATUE);
evt_tween_begin_alt();
tw_set_pos(0, 0, 5);
tw_set_rot(0, 0, 0);
tw_rot_add();
evt_next();
tw_set_rot(0, 0, 1);
tw_rot_add();
evt_next();
tw_set_rot(0, 0, 3);
tw_rot_add();
evt_next();
```

Room 202 (`games/re1/room/202/room.akscr`), event `statue_falls`: the
statue tips over about z, faster each frame.

## See also

- [`tw_set_rot`](tw_set_rot.md): sets the steps.
- [`tw_pos_rot_add`](tw_pos_rot_add.md): steps the position too.
- [`tw_abs_rot`](tw_abs_rot.md): sets the rotation outright.
