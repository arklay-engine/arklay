# `member_cmp`

Compares one field (member) of the task's work with a constant.

| | |
| --- | --- |
| Opcode | `0x3E` |
| Group | Characters, objects and the work |
| Kind | Condition |
| Size | 6 bytes |
| Status | yes |

## Syntax

```c
member_cmp(0, member, comparison, value);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `_` | `u8` | Padding, not read. Write 0. |
| `member` | `u8` | The field. See [the members](member_set.md#members). |
| `comparison` | `u8` | `0` `==`, `1` `>`, `2` `>=`, `3` `<`, `4` `<=`, `5` `!=`, `6` passes when `member & value` is not 0. Any other value fails. |
| `value` | `i16` | The constant, sign-extended. |

## Behaviour

Reads member `member` of the work chosen with [`work_set`](work_set.md) and
compares it, as a 32-bit signed number, with `value`: `member_cmp(0, 22, 1,
5)` passes when the member is greater than 5. A failure works as for any
[condition](../index.md#conditions).

The comparisons are those of [`cmp`](cmp.md), with one difference: a
comparison from 7 up always fails here, where `cmp` passes when the variable
is not 0.

A member the port has no field for, or a work the engine does not model,
fails the condition. That is deliberate: a script that waits in a loop for a
motion of something the port does not model (an enemy other than the ones
[`sce_em_set`](sce_em_set.md) lists, say) moves on rather than waiting for
ever.

Members are read as they stand now. The player's position and facing do not
yet show a [`pos_set`](pos_set.md) from the same tick.

## Example

```c
proc main_0D
{
    work_set(3, 0);
    plc_motion(1, 3, 0);
    do
    {
        evt_next();
    }
    while (member_cmp(0, 3, 0, 1));    // while Routine_1 is still 1
    evt_end(0);
}
```

Illustrative: it starts a scripted motion on the character in enemy slot 0
and waits until its routine moves on. The routine and clip numbers are
placeholders.

## See also

- [`member_copy`](member_copy.md): reads a member into a variable.
- [`member_set`](member_set.md): writes a member, and lists the members.
- [`cmp`](cmp.md): compares a script variable.
