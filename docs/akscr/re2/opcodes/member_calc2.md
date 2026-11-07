# `member_calc2`

Arithmetic on one field (member) of the task's work with a script variable (not modelled).

| | |
| --- | --- |
| Opcode | `0x56` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 4 bytes |
| Status | no |

## Syntax

```c
member_calc2(operation, member, variable);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `operation` | `u8` | Not read by the port. By its name and its place beside [`calc2`](calc2.md), the operation. |
| `member` | `u8` | Not read by the port. By its name, the member to change. |
| `variable` | `u8` | Not read by the port. By its name, the variable that is the operand. |

## Behaviour

Not modelled: the instruction does nothing, and the log notes
`RE2 script: member_calc2 is not modelled` once per run. The field meanings
above follow the mnemonic and the pattern of [`calc2`](calc2.md); they are not
taken from the original's handler.

Use [`member_copy`](member_copy.md), [`calc2`](calc2.md) and
[`member_set2`](member_set2.md) instead.

## Example

```c
proc main_0F
{
    work_set(4, 3);
    member_copy(43, 11);         // in place of a member_calc2 on x
    calc2(0, 43, 44);            // x + variable 44
    member_set2(11, 43);
    evt_end(0);
}
```

Illustrative: what to write instead of a `member_calc2` that adds variable 44
to room object 3's x.

## See also

- [`member_calc`](member_calc.md): the same with a constant, also not modelled.
- [`member_set`](member_set.md): lists the members.
