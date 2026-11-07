# `speed_set`

Sets one of the running task's twelve speeds, which `add_speed` and `add_aspeed` use.

| | |
| --- | --- |
| Opcode | `0x2F` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 4 bytes |
| Status | yes |

## Syntax

```c
speed_set(index, value);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `index` | `u8` | The speed, `0` to `11`: `0` to `2` move along x, y and z, `3` to `5` turn about x, y and z, and `6` to `11` are what [`add_aspeed`](add_aspeed.md) adds to `0` to `5`. |
| `value` | `i16` | The speed, in world units (or angle units, 4096 to a turn) per `add_speed`. |

## Behaviour

Each task slot keeps twelve signed 16-bit speeds. `speed_set` writes speed
`index` of the running task. An `index` from 12 up does nothing.
[`work_set`](work_set.md) sets all twelve back to 0, so set the speeds after
the `work_set` that chooses what they move.

Speeds do nothing on their own: [`add_speed`](add_speed.md) adds them to the
work's position and rotation, once per call.

## Example

```c
proc main_05
{
    work_set(4, 2);              // room object 2
    speed_set(1, -8);            // y: up 8 units a step
    speed_set(7, -1);            // and 1 faster each step
    for (40)
    {
        add_aspeed();
        add_speed();
        evt_next();
    }
    evt_end(0);
}
```

Illustrative: room object 2 rises for 40 ticks, faster each tick (negative y
is up).

## See also

- [`add_speed`](add_speed.md): moves the work by the speeds.
- [`add_aspeed`](add_aspeed.md): accelerates the speeds.
- [`work_set`](work_set.md): chooses the work and clears the speeds.
