# `calc`

Changes a script variable by a constant.

| | |
| --- | --- |
| Opcode | `0x26` |
| Group | Flags, variables and pad |
| Kind | Instruction |
| Size | 6 bytes |
| Status | VM |

## Syntax

```c
calc(0, operation, variable, value);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `_` | `u8` | Not read. Write 0. |
| `operation` | `u8` | `0` add, `1` subtract, `2` multiply, `3` divide, `4` remainder, `5` or, `6` and, `7` xor, `8` not, `9` shift left, `10` shift right, `11` arithmetic shift right. Any other value leaves the variable as it is. |
| `variable` | `u8` | The variable to change. |
| `value` | `i16` | The constant. |

## Behaviour

`variable = variable OP value`, on 16 bits, following the original's
`Calc_branch`:

- Add, subtract and multiply wrap at 16 bits.
- Divide and remainder are signed. Dividing by 0 leaves the variable as it is.
- Or, and and xor work on the bits. Not (8) inverts the variable and ignores
  `value`.
- The shifts take the low five bits of `value` as the count. Shift right (10)
  fills with zeros, arithmetic shift right (11) with the sign bit.

## Example

```c
calc(0, 0, 16, 1);               // variable 16 += 1
calc(0, 4, 16, 3);               // variable 16 %= 3
```

Illustrative: a counter that cycles through 0, 1 and 2.

## See also

- [`calc2`](calc2.md): the same with another variable as the operand.
- [`save`](save.md): sets a variable.
