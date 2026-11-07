# `save`

Sets a script variable to a constant.

| | |
| --- | --- |
| Opcode | `0x24` |
| Group | Flags, variables and pad |
| Kind | Instruction |
| Size | 4 bytes |
| Status | VM |

## Syntax

```c
save(variable, value);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `variable` | `u8` | The script variable, `0` to `255`. |
| `value` | `i16` | The value. Any 16-bit spelling works: `-1` and `0xFFFF` are the same. |

## Behaviour

`variable = value`. There are 256 variables of 16 bits, and saves keep them
all. Variables 24, 25 and 26 (the stage, the room and the camera on screen)
read from the engine whatever is written to them, so a `save` to them has no
visible effect. Variables 0 and 1 are overwritten by the area probe, and 29
by the scheduler at every tick.

## Example

```c
save(16, 0);
while (cmp(0, 16, 3, 5))
{
    calc(0, 0, 16, 1);
    sleep(10, 10);
}
```

Illustrative: a counter from 0 to 5, one step every ten ticks.

## See also

- [`copy`](copy.md): copies one variable to another.
- [`calc`](calc.md): changes a variable by a constant.
- [`cmp`](cmp.md): tests a variable.
