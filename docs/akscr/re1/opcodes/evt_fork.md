# `evt_fork`

Starts an event in an event slot, from event code.

| | |
| --- | --- |
| Opcode | `0x05` |
| Group | Event: top state |
| Kind | Event instruction |
| Size | 4 bytes |
| Valid in | the top state |

## Syntax

```c
evt_fork(slot, event, 0);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `slot` | `u8` | The event slot (`TASK_00` to `TASK_07`), or 8 or more (the scripts use `TASK_09`) for the first free slot. |
| `event` | `u8` | The event to start, by name (or its number). |
| *pad* | `u8` | Unused, written as 0. |

## Behaviour

`evt_fork` is the event code's form of the command
[`evt_exec`](evt_exec.md). It resets the target slot and starts `event` at
its first instruction there, at once rather than at the end of the pass:

- A slot number of 8 or more picks the first slot that is not running, or
  slot 7 when all eight are busy.
- Whatever was running in the target slot is dropped.
- The new event starts in the top state, with empty loop stacks and the
  player selected.
- An event number past the room's last event leaves the slot stopped.
- Do not name the forking event's own slot (nor 8 or more when it runs in
  slot 7 and every slot is busy). The port resets the slot, then carries on
  with the old event's next instruction instead of starting the new one.

The VM steps the slots in order, 0 to 7, each frame. A new event in a slot
after the current one runs its first instructions in the same frame; one in
an earlier slot waits for the next frame. The forking event continues in
the same frame.

## Example

```c
sleep(1);
set(FG_RUN, F_RUN_CONTROLS, FALSE);
evt_fork(TASK_09, rebecca_hunter_attack_rebecca, 0);
evt_fork(TASK_09, rebecca_hunter_attack_hunter, 0);
evt_fork(TASK_09, rebecca_hunter_attack_footsteps, 0);
sleep(100);
```

Room 101 (`games/re1/room/101/room.akscr`), event `rebecca_hunter_attack`:
starts three events in free slots that run beside this one.

## See also

- [`evt_exec`](evt_exec.md): the same from command code (a `run` block or
  the main script).
- [`evt_chain`](evt_chain.md): replaces the current event in its own slot.
- [`evt_disable`](evt_disable.md), [`task_kill`](task_kill.md): stop a slot.
