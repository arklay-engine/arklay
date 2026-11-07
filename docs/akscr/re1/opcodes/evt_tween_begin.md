# `evt_tween_begin`

Resets the selected entity's ignore byte, behaviour and action state, then enters the tween state.

| | |
| --- | --- |
| Opcode | `0x02` |
| Group | Event: top state |
| Kind | Event instruction |
| Size | 1 byte |
| Valid in | the top state |

## Syntax

```c
evt_tween_begin();
```

## Parameters

None.

## Behaviour

Before it enters the tween state (state 2), `evt_tween_begin` writes the
selection's state block: the ignore byte (`TW_IGNORE`, the high byte of the
`+0x84` state word) becomes 2, and the action behaviour and action state
become 0. The state byte itself is left alone. On a room object or item
model these are the record's tween scratch bytes. Then it continues in the
same frame with the `tw_*` instructions.

[`evt_tween_begin_alt`](evt_tween_begin_alt.md) enters the same state
without the reset. The checked-in scripts use only the `_alt` form, so
nothing in the shipped rooms exercises this one.

In the tween state every byte from `0x00` to `0x0B` is a tween instruction,
so no top-state instruction can stand in a tween sequence; only the control
instructions and the `tw_*` set. [`tw_end`](tw_end.md) returns to the top
state.

In the port the reset goes to the entity the game state last had selected,
without first re-selecting this slot's own selection as the actor and tween
instructions do. Put `evt_work_set` straight before `evt_tween_begin` (with
no yield between them) so the two agree.

## Example

```c
evt_work_set(WK_ENEMY, ENEMY_00_BARRY);
evt_tween_begin();
tw_abs_pos(0, 18000, 0, 14300);
tw_end();
```

Illustrative, in room 105 (`games/re1/room/105/room.akscr`): puts Barry
at a position after clearing his behaviour, so his
own animation code stops driving him.

## See also

- [`evt_tween_begin_alt`](evt_tween_begin_alt.md): the same without the
  reset.
- [`tw_end`](tw_end.md): leaves the tween state.
- [`tw_set_field`](tw_set_field.md): writes one of the state bytes.
- [Actor and tween sequences](../index.md#actor-and-tween-sequences).
