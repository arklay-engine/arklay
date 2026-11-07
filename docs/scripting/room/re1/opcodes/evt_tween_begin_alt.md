# `evt_tween_begin_alt`

Enters the tween state, where `tw_*` instructions move the selection, without touching the selection.

| | |
| --- | --- |
| Opcode | `0x03` |
| Group | Event: top state |
| Kind | Event instruction |
| Size | 1 byte |
| Valid in | the top state |

## Syntax

```c
evt_tween_begin_alt();
```

## Parameters

None.

## Behaviour

Switches the event slot to the tween state (state 2) and continues in the
same frame. The `tw_*` instructions that follow act on what the last
[`evt_work_set`](evt_work_set.md) of this slot selected: the player, an
enemy slot, a room object or an item model. On a character they write the
entity; on a room object or item model they write its position and
rotation, and keep the other fields (steps, state bytes) in a scratch area
of the record.

In the tween state every byte from `0x00` to `0x0B` is a tween instruction,
so no top-state instruction can stand in a tween sequence; only the control
instructions ([`evt_next`](evt_next.md), `sleep`, `for`, `do`, `if`, and
[`evt_finish`](evt_finish.md), written `tw_finish()`) and the `tw_*` set.
[`tw_end`](tw_end.md) returns to the top state. No tween instruction
yields, so a movement over several frames needs `evt_next()` or `sleep`
between steps.

This is the form the shipped scripts use. [`evt_tween_begin`](evt_tween_begin.md)
also resets the selection's ignore byte, behaviour and action state first.

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
tw_nop();
tw_end();
evt_nop();
```

Room 105 (`games/re1/room/105/room.akscr`): slides the clock 20 units a
frame for 100 frames.

## See also

- [`evt_tween_begin`](evt_tween_begin.md): the same with a reset.
- [`tw_end`](tw_end.md): leaves the tween state.
- [`evt_actor_begin`](evt_actor_begin.md): enters the actor state instead.
- [Actor and tween sequences](../index.md#actor-and-tween-sequences).
