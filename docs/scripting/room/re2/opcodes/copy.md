# `copy`

Copies one script variable to another.

| | |
| --- | --- |
| Opcode | `0x25` |
| Group | Flags, variables and pad |
| Kind | Instruction |
| Size | 3 bytes |
| Status | VM |

## Syntax

```c
copy(to, from);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `to` | `u8` | The variable to write. |
| `from` | `u8` | The variable to read. |

## Behaviour

`to = from`, all 16 bits. Reading variables 24, 25 or 26 gives the engine's
stage (from 0), room and camera on screen, so `copy` is how a script keeps
one of them: `copy(16, 26);` saves the camera on screen in variable 16.
Writing to 24, 25 or 26 has no visible effect.

## Example

```c
copy(17, 1);                     // the area the player stands in
if (cmp(0, 17, 0, 3))
{
    set(5, 15, 1);
}
```

Illustrative: it keeps the probe's area number before testing it.

## See also

- [`save`](save.md): sets a variable to a constant.
- [`calc2`](calc2.md): arithmetic with another variable.
