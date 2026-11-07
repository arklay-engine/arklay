# `evt_exec`

Starts an event in an event slot.

| | |
| --- | --- |
| Opcode | `0x14` |
| Group | Events |
| Kind | Command |
| Size | 4 bytes |

## Syntax

```c
evt_exec(0, slot, event);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| *pad* | `u8` | Unused, written as 0. |
| `slot` | `u8` | The event slot, `TASK_00` to `TASK_07`; `TASK_08` or more (the shipped scripts use `TASK_09`) takes the first free slot. |
| `event` | `u8` | The event, by its name (`event NAME { ... }`) or number. |

## Behaviour

The event VM has eight slots, each running one event a step at a time,
every frame. `evt_exec` starts `event` in `slot` from its first
instruction, replacing whatever ran there, with the slot's stacks and its
[`evt_work_set`](evt_work_set.md) selection cleared. A slot of 8 or more
takes the first slot not running an event, or slot 7 when all eight are
busy.

The request takes effect in program order with every other start and
[`task_kill`](task_kill.md), so a kill followed by a start of the same slot
leaves the new event running. A start from `init` or `main` lands before the
event slots run that frame. A start from inside an event (an inline command
in a `run` block) lands at once, so a later slot can run the new event in
the same pass. While the room's events are switched off, the starts still
land but no slot runs.

An event number the room does not have leaves the slot stopped.

## Example

```c
aot_set(AOT_01_EVENT_REBECCA_TALK, 6780, 3420, 1000, 1000, SCE_EVENT, SAT_MANUAL | SAT_PLAYER, 9, 0, rebecca_wonders_about_team, 0, 0, 0);
set(FG_COMMON, F_COMMON_C_REBECCA_IN_SAVE_ROOM, TRUE);
evt_exec(0, TASK_09, rebecca_first_meeting);
```

Room 100 (`games/re1/room/100/room.akscr`): the init starts Rebecca's first
meeting in any free slot.

## See also

- [`task_kill`](task_kill.md): stops the event in a slot.
- [`evt_fork`](evt_fork.md), [`evt_chain`](evt_chain.md),
  [`evt_disable`](evt_disable.md): the event VM's own start and stop.
- [Event scripts](../index.md#event-scripts).
