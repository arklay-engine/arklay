# `evt_finish`

Ends the event and frees its slot; written `act_finish()` in the actor state and `tw_finish()` in the tween state.

| | |
| --- | --- |
| Opcode | `0xFF` |
| Group | Event: control |
| Kind | Event instruction |
| Size | 1 byte |
| Valid in | every state |

## Syntax

```c
evt_finish();
act_finish();
tw_finish();
```

## Parameters

None.

## Behaviour

Stops the slot: the event runs no further and the slot is free for
[`evt_exec`](evt_exec.md) or [`evt_fork`](evt_fork.md) to reuse. It is the
same instruction in every state, but the compiler takes the name that
matches the state the event is in: `evt_finish()` in the top state,
`act_finish()` in an actor sequence, `tw_finish()` in a tween sequence.
Another name there is an error ("not valid here").

Ending inside an actor or tween sequence does not return the entity to the
top state's handling: whatever the sequence set (state, behaviour, flags)
stays as it is. `act_finish()` after an `act_anim_seq`, for example, leaves
the character walking to its target with nothing waiting for it.

Every event ends in `evt_finish` or [`evt_chain`](evt_chain.md); one that
runs off its end stops when it reaches bytes that are not an instruction.
The compiler pads an event that ends in `evt_finish` with zeros to four
bytes, unless its header says `nopad` (see [Padding](../index.md#padding)).

## Example

```c
evt_work_set(WK_OBJ, OBJ_00_WATER);
evt_tween_begin_alt();
tw_abs_pos(0, 6280, 160, 7920);
tw_finish();
```

Room 113 (`games/re1/room/113/room.akscr`), event `place_water`: puts a
room object at a position and ends the event inside the tween sequence.

## See also

- [`evt_chain`](evt_chain.md): ends the event by starting another in its
  slot.
- [`evt_disable`](evt_disable.md), [`task_kill`](task_kill.md): stop
  another slot.
