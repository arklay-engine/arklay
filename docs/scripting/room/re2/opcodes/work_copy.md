# `work_copy`

Writes a script variable into the procedure's own bytes, to patch a later instruction's operand.

| | |
| --- | --- |
| Opcode | `0x1D` |
| Group | Control flow |
| Kind | Instruction |
| Size | 4 bytes |
| Status | VM |

## Syntax

```c
work_copy(variable, offset, wide);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `variable` | `u8` | The script variable to write. |
| `offset` | `u8` | Where to write it: this many bytes past the end of the `work_copy`. |
| `wide` | `u8` | `0` writes the variable's low byte; any other value writes both bytes, low byte first. |

## Behaviour

`work_copy` changes the running procedure's code. It writes the variable's
low byte, or its two bytes, at `offset` bytes past its own end, so the
instruction it lands in runs with the variable's value as an operand. Most
instructions take only constants, so this is how a script passes a computed
value to one.

The change is to the room's loaded copy of the procedure and stays until the
room's scripts are loaded again, at the next entry to the room. A write past
the end of the procedure is ignored. The compiled `.akscr` and the RDT are
not changed.

`offset` counts from the end of the `work_copy`, so `1` is the first operand
byte of an instruction that follows it directly. Work it out from the
[opcode table](../index.md#opcodes)'s widths.

## Example

```c
work_copy(16, 1, 0);
cut_chg(0);                      // runs as cut_chg(variable 16)
```

Illustrative: the `cut_chg` starts right after the `work_copy`, and its
operand is the byte after its opcode, at offset 1. The camera it switches to
is the value of variable 16.

## See also

- [`save`](save.md), [`calc`](calc.md): set the variable.
- [`member_copy`](member_copy.md): reads a field of the work into a variable.
