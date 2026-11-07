# `tw_pos_add`

Adds the position step to the selection's position.

| | |
| --- | --- |
| Opcode | `0x02` |
| Group | Event: tween state |
| Kind | Event instruction |
| Size | 1 byte |
| Valid in | the tween state |

## Syntax

```c
tw_pos_add();
```

## Parameters

None.

## Behaviour

Adds the step that [`tw_set_pos`](tw_set_pos.md) set to the x, y and z of
the selection's position, once. It does not yield: to move over time, put
it in a loop with `evt_next()`. It moves the selection without collision,
and works on a character, a room object or an item model.

## Example

```c
evt_work_set(WK_OBJ, OBJ_02_CLOCK);
evt_tween_begin_alt();
evt_next();
tw_nop();
tw_set_pos(-20, 0, 0);
for (100)
{
    tw_pos_add();
    evt_next();
}
```

Room 105 (`games/re1/room/105/room.akscr`): slides the clock 2000 units
along x over 100 frames.

## See also

- [`tw_set_pos`](tw_set_pos.md): sets the step.
- [`tw_pos_rot_add`](tw_pos_rot_add.md): steps the rotation too.
- [`tw_abs_pos`](tw_abs_pos.md): sets the position outright.
