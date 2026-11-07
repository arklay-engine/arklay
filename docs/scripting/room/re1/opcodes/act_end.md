# `act_end`

Returns from the actor state to the top state.

| | |
| --- | --- |
| Opcode | `0x8B` |
| Group | Event: actor state |
| Kind | Event instruction |
| Size | 1 byte |
| Valid in | the actor state |

## Syntax

```c
act_end();
```

## Parameters

None.

## Behaviour

Leaves the actor state and continues in the top state in the same frame.
It does not touch the selected entity. [`act_reset`](act_reset.md) does the
same and also clears the ignore byte; the shipped scripts nearly always use
that, and `act_end` appears once.

## Example

```c
evt_work_set(WK_PLAYER, 0);
evt_actor_begin();
act_motion_path(MOTION_13, 4900, -8000, 4900, 100, 100);
sleep(20);
act_motion(MOTION_10);
sleep(10);
act_end();
evt_nop();
evt_chain(jill_pushes_ceiling);
```

Room 115 (`games/re1/room/115/room.akscr`), event `jill_pushes_ceiling`.

## See also

- [`act_reset`](act_reset.md): the usual way out of the actor state.
