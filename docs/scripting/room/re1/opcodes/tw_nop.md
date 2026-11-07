# `tw_nop`

Does nothing in the tween state.

| | |
| --- | --- |
| Opcode | `0x00` |
| Group | Event: tween state |
| Kind | Event instruction |
| Size | 1 byte |
| Valid in | the tween state |

## Syntax

```c
tw_nop();
```

## Parameters

None.

## Behaviour

Moves to the next instruction in the same frame and stays in the tween
state. It is the tween state's reading of the byte `0x00`, which is
[`evt_nop`](evt_nop.md) in the top state and [`act_nop`](act_nop.md) in the
actor state. The original's tween sequences often begin and end with one;
a new script can leave them out.

## Example

```c
evt_work_set(WK_OBJ, OBJ_02_CLOCK);
evt_tween_begin_alt();
evt_next();
tw_nop();
tw_set_pos(-20, 0, 0);
```

Room 105 (`games/re1/room/105/room.akscr`).

## See also

- [`evt_nop`](evt_nop.md), [`act_nop`](act_nop.md): the same byte in the
  other states.
