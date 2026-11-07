# `evt_disable`

Stops the event running in an event slot.

| | |
| --- | --- |
| Opcode | `0x09` |
| Group | Event: top state |
| Kind | Event instruction |
| Size | 2 bytes |
| Valid in | the top state |

## Syntax

```c
evt_disable(slot);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `slot` | `u8` | The event slot, `TASK_00` to `TASK_07`. |

## Behaviour

Marks the slot inactive at once: the event in it runs no further, this
frame or later. A slot that is already stopped stays so, and a slot number
of 8 or more does nothing. The calling event continues in the same frame,
unless `slot` is its own, which stops it.

It is the event code's form of the command [`task_kill`](task_kill.md).

## Example

```c
do
{
    evt_next();
}
while (cmpb(B_MENU_CHOICE, CMP_GE, 128));
cutcurr(0);
evt_disable(TASK_06);
```

Room 105 (`games/re1/room/105/room.akscr`), event
`hall_door_not_done_or_barry_walks_in`: waits for the message to close and
stops the event in slot 6.

## See also

- [`task_kill`](task_kill.md): the same from command code.
- [`evt_fork`](evt_fork.md), [`evt_exec`](evt_exec.md): start an event.
- [`evt_finish`](evt_finish.md): ends the running event.
