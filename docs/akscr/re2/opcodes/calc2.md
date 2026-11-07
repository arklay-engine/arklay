# `calc2`

Changes a script variable by the value of another.

| | |
| --- | --- |
| Opcode | `0x27` |
| Group | Flags, variables and pad |
| Kind | Instruction |
| Size | 4 bytes |
| Status | VM |

## Syntax

```c
calc2(operation, variable, source);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `operation` | `u8` | As for [`calc`](calc.md): `0` add, `1` subtract, `2` multiply, `3` divide, `4` remainder, `5` or, `6` and, `7` xor, `8` not, `9` shift left, `10` shift right, `11` arithmetic shift right. |
| `variable` | `u8` | The variable to change. |
| `source` | `u8` | The variable whose value is the operand. |

## Behaviour

`variable = variable OP source`, with the rules of [`calc`](calc.md): 16-bit
wrapping arithmetic, signed division and remainder that leave the variable as
it is for a zero divisor, and shifts by the low five bits of `source`. Note
that the operands come in a different order from `calc`'s: the operation
first, and no leading padding byte.

## Example

```c
copy(16, 26);                    // the camera on screen
calc2(0, 16, 17);                // plus the offset in variable 17
```

Illustrative: it adds variable 17 to a copy of the camera number.

## See also

- [`calc`](calc.md): the same with a constant.
- [`copy`](copy.md): copies a variable.
