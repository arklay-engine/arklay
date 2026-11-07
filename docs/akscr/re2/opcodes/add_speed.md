# `add_speed`

Moves and turns the task's work by the task's speeds, once.

| | |
| --- | --- |
| Opcode | `0x30` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 1 byte |
| Status | yes |

## Syntax

```c
add_speed();
```

## Parameters

None.

## Behaviour

Adds the running task's speeds (set with [`speed_set`](speed_set.md)) to its
work, chosen with [`work_set`](work_set.md). Each call is one step; to move
something steadily, call it once a tick in a loop.

- **A room object** (kind 4): speeds 0 to 2 are added to its x, y and z, and
  speeds 3 to 5 to its rotation about x, y and z.
- **A character** (kinds 2 and 3): speeds 0 to 2 are added to its position,
  and speed 4 to its facing. Speeds 3 and 5 are not used.
- **The player** (kind 1): as a character. The move does not happen at once:
  it is kept and applied after the tick's scripts have run, on top of any
  [`pos_set`](pos_set.md) or [`dir_set`](dir_set.md) earlier in the tick. The
  facing is kept within a turn (`& 0x0FFF`).
- **A work the engine does not model**: nothing happens.

The additions wrap around at 32 bits for a position and 16 bits for an angle.
Collision is not checked: the work goes wherever the speeds take it.

## Example

```c
proc main_06
{
    work_set(4, 0);
    speed_set(0, 16);            // x: 16 units a tick
    speed_set(4, 32);            // and turn about y
    for (64)
    {
        add_speed();
        evt_next();
    }
    evt_end(0);
}
```

Illustrative: room object 0 slides 1024 units along x over 64 ticks, turning
half a turn as it goes.

## See also

- [`speed_set`](speed_set.md): sets the speeds.
- [`add_aspeed`](add_aspeed.md): changes the speeds by the accelerations.
- [`pos_set`](pos_set.md): places the work outright.
