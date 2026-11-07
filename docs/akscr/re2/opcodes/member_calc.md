# `member_calc`

Arithmetic on one field (member) of the task's work with a constant (not modelled).

| | |
| --- | --- |
| Opcode | `0x55` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 6 bytes |
| Status | no |

## Syntax

```c
member_calc(operation, member, value);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `operation` | `u8` | Not read by the port. By its name and its place beside [`calc`](calc.md), the operation. |
| `member` | `u16` | Not read by the port. By its name, the member to change. |
| `value` | `i16` | Not read by the port. By its name, the operand. |

## Behaviour

Not modelled: the instruction does nothing, and the log notes
`RE2 script: member_calc is not modelled` once per run. The field meanings
above follow the mnemonic, the operand widths of the opcode table and the
pattern of [`calc`](calc.md); they are not taken from the original's handler,
which has not been looked at.

To change a member, read it with [`member_copy`](member_copy.md), change the
variable with [`calc`](calc.md) and write it back with
[`member_set2`](member_set2.md).

## Example

```c
proc main_0E
{
    work_set(3, 0);
    member_copy(42, 12);         // in place of a member_calc on y
    calc(0, 1, 42, 100);         // y - 100
    member_set2(12, 42);
    evt_end(0);
}
```

Illustrative: what to write instead of a `member_calc` on the work's y.

## See also

- [`member_calc2`](member_calc2.md): the same with a variable, also not modelled.
- [`member_set`](member_set.md): lists the members.
