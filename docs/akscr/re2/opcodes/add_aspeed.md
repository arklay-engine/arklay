# `add_aspeed`

Accelerates the running task's speeds: adds speeds 6 to 11 to speeds 0 to 5.

| | |
| --- | --- |
| Opcode | `0x31` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 1 byte |
| Status | yes |

## Syntax

```c
add_aspeed();
```

## Parameters

None.

## Behaviour

For each `n` from 0 to 5, speed `n` of the running task becomes speed `n`
plus speed `n + 6`, wrapping at 16 bits. It changes only the speeds, not the
work: follow it with [`add_speed`](add_speed.md) to move. It works whatever
the task's work is, including one the engine does not model.

## Example

```c
proc main_07
{
    work_set(4, 1);
    speed_set(7, 2);             // gravity: y speed grows by 2 a tick
    while (cmp(0, 30, 3, 120))   // until variable 30 reaches 120
    {
        add_aspeed();
        add_speed();
        calc(0, 0, 30, 1);
        evt_next();
    }
    evt_end(0);
}
```

Illustrative: room object 1 falls (positive y is down) faster each tick for
120 ticks, counting in script variable 30, which the procedure assumes starts
at 0.

## See also

- [`speed_set`](speed_set.md): sets a speed or an acceleration.
- [`add_speed`](add_speed.md): moves the work by the speeds.
