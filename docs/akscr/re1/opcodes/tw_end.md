# `tw_end`

Returns from the tween state to the top state.

| | |
| --- | --- |
| Opcode | `0x01` |
| Group | Event: tween state |
| Kind | Event instruction |
| Size | 1 byte |
| Valid in | the tween state |

## Syntax

```c
tw_end();
```

## Parameters

None.

## Behaviour

Leaves the tween state and continues in the top state in the same frame.
It does not touch the selection: the position, rotation and steps stay as
the tween left them, and nothing moves further on its own (the steps are
only added by [`tw_pos_add`](tw_pos_add.md) and the like). An
[`evt_tween_begin`](evt_tween_begin.md)'s ignore byte stays set; an actor
sequence's [`act_reset`](act_reset.md) clears it.

## Example

```c
evt_work_set(WK_OBJ, OBJ_02_CLOCK);
evt_tween_begin_alt();
tw_set_pos(-20, 0, 0);
for (100)
{
    tw_pos_add();
    evt_next();
}
tw_nop();
tw_end();
evt_nop();
sleep(10);
```

Room 105 (`games/re1/room/105/room.akscr`): ends the clock's slide.

## See also

- [`evt_tween_begin_alt`](evt_tween_begin_alt.md): enters the tween state.
- [`evt_finish`](evt_finish.md): `tw_finish()` ends the event inside the
  tween state instead.
