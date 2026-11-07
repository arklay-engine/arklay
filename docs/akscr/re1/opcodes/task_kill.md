# `task_kill`

Stops the event running in an event slot.

| | |
| --- | --- |
| Opcode | `0x44` |
| Group | Events |
| Kind | Command |
| Size | 2 bytes |

## Syntax

```c
task_kill(slot);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `slot` | `u8` | The event slot, `TASK_00` to `TASK_07`. |

## Behaviour

`task_kill` marks the slot inactive: its event stops where it is and does
not run again. Nothing it started (a sound, an animation, a held camera) is
undone. A slot that is not running, or a slot past 7, is left alone.

The kill takes effect in program order with [`evt_exec`](evt_exec.md)
starts, exactly as the original's immediate write does: an event that kills
a later slot stops it in the same pass, and a kill followed by a start of
the same slot leaves the new event running.

It is written by field, with one argument.

## Example

```c
if (ck(FG_ROOM, F_ROOM_DROP_NOW, TRUE))
{
    set(FG_ROOM, F_ROOM_TYRANT_FIGHT, FALSE);
    task_kill(TASK_07);
    set(FG_ROOM, F_ROOM_DROP_PENDING, TRUE);
}
```

Room 303 (`games/re1/room/303/room.akscr`): during the Tyrant fight, the
main block stops the event in slot 7 once the room's drop flag is set.

## See also

- [`evt_exec`](evt_exec.md): starts an event in a slot.
- [`evt_disable`](evt_disable.md): the event VM's own form.
