# `member_set2`

Writes a script variable into one field (member) of the task's work.

| | |
| --- | --- |
| Opcode | `0x35` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 3 bytes |
| Status | yes |

## Syntax

```c
member_set2(member, variable);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `member` | `u8` | The field. See [the members](member_set.md#members). |
| `variable` | `u8` | The script variable, `0` to `255`, whose value is written. |

## Behaviour

As [`member_set`](member_set.md), with the value read from script variable
`variable`. The variable is read as a signed 16-bit number and sign-extended,
as the original's `Member_set2` does (`MOVSX`), so `-1` stays `-1` in a
32-bit field. The same members are modelled, with the same side effects.

## Example

```c
proc main_0B
{
    work_set(1, 0);
    member_copy(40, 11);         // variable 40 = the player's x
    calc(0, 0, 40, 500);         // + 500
    work_set(3, 0);
    member_set2(11, 40);         // the character's x = variable 40
    evt_end(0);
}
```

Illustrative: it moves the character in enemy slot 0 to 500 units to the
player's side along x.

## See also

- [`member_set`](member_set.md): writes a member from a constant, and lists the members.
- [`member_copy`](member_copy.md): reads a member into a variable.
- [`calc`](calc.md), [`calc2`](calc2.md): arithmetic on variables.
