# `evt_chain`

Replaces the running event with another, in the same slot.

| | |
| --- | --- |
| Opcode | `0x08` |
| Group | Event: top state |
| Kind | Event instruction |
| Size | 2 bytes |
| Valid in | the top state |

## Syntax

```c
evt_chain(event);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `event` | `u8` | The event to run next, by name (or its number). |

## Behaviour

The slot restarts at the first instruction of `event`: top state, empty
loop stacks. Unlike [`evt_fork`](evt_fork.md), it keeps the slot's
selection, so the new event can carry on with the entity the old one
selected. The slot yields: the new event's first instruction runs in the
next frame.

Nothing after `evt_chain` in the old event runs, so it is the last
instruction of an event and needs no [`evt_finish`](evt_finish.md). An
event number past the room's last event stops the slot.

## Example

```c
pos_set(ENEMY_00_BARRY, 0, 1024, 0, 4300, 0, 8000);
do
{
    evt_next();
}
while (ck(FG_ROOM, F_ROOM_J_BARRY_TO_FIREPLACE, FALSE));
evt_chain(barry_shows_blood);
```

Room 105 (`games/re1/room/105/room.akscr`), event `barry_walks_in_barry`:
waits for Barry to reach the fireplace, then goes on with the next scene in
the same slot.

## See also

- [`evt_fork`](evt_fork.md): starts an event in another slot.
- [`evt_finish`](evt_finish.md): ends the event.
