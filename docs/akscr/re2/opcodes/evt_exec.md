# `evt_exec`

Starts a main procedure in a task slot.

| | |
| --- | --- |
| Opcode | `0x04` |
| Group | Control flow |
| Kind | Instruction |
| Size | 4 bytes |
| Status | VM |

## Syntax

```c
evt_exec(task, 0, proc);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `task` | `u8` | The task slot, `0` to `9`. From `10` up: the first free slot from 2 to 9. |
| `_` | `u8` | Not read. Write 0. |
| `proc` | `u8` | The procedure to start, by its number in the list that is running. |

## Behaviour

The scheduler has 14 task slots. `evt_exec` starts procedure `proc` from its
beginning in slot `task`, replacing whatever that slot was running, with no
`gosub` calls, blocks or loops left from it. A `task` of 10 or more picks the
first slot from 2 to 9 that is not running; if all eight are busy, nothing
starts. A `proc` past the end of the list leaves the slot stopped.

The running task goes on with the next instruction. The tasks run in slot
order within a tick, so a procedure started in a later slot runs in the same
tick and one started in an earlier slot first runs at the next tick. If
`task` is the running task's own slot, the new procedure replaces the
running one and starts at once.

A task that `init_00` starts during the room's load does not run during the
init. It first runs in the load's pass, alongside `main_00`, and it runs the
*main* procedure with that number (see
[what runs when](../index.md#what-runs-when)). Slot 0 is a poor choice
outside the load: the scheduler restarts `main_01` in task 0 at every tick.

## Example

```c
proc main_02
{
    evt_exec(10, 0, 4);          // main_04 in the first free slot
    evt_exec(10, 0, 5);          // main_05 in the next one
    evt_end(0);
}
```

Illustrative: an event area's procedure starts two procedures that run side
by side, for instance one moving a character and one moving the camera.

## See also

- [`evt_kill`](evt_kill.md): stops a task.
- [`evt_chain`](evt_chain.md): restarts the running task on another procedure.
- [`aot_set`](aot_set.md): an event area (kind 5) also starts a procedure in a task.
