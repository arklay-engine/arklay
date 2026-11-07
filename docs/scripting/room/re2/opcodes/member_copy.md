# `member_copy`

Reads one field (member) of the task's work into a script variable.

| | |
| --- | --- |
| Opcode | `0x3D` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 3 bytes |
| Status | yes |

## Syntax

```c
member_copy(variable, member);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `variable` | `u8` | The script variable, `0` to `255`, that receives the value. |
| `member` | `u8` | The field. See [the members](member_set.md#members). |

## Behaviour

Reads member `member` of the work chosen with [`work_set`](work_set.md) and
stores its low 16 bits in script variable `variable`. A member the port has
no field for, or a work the engine does not model, gives 0.

The player's position and facing are read from where the player stands now:
a [`pos_set`](pos_set.md) earlier in the same tick has not been applied yet,
so it is not seen until the next tick. The facing reads within a turn
(`0` to `4095`).

## Example

```c
proc main_0C
{
    work_set(3, 0);
    member_copy(41, 15);         // variable 41 = the character's facing
    if (cmp(0, 41, 3, 2048))     // facing below half a turn
    {
        set(5, 40, 1);
    }
    evt_end(0);
}
```

Illustrative: it records in room flag 40 which half of the turn the character
in enemy slot 0 faces.

## See also

- [`member_cmp`](member_cmp.md): compares a member directly.
- [`member_set2`](member_set2.md): writes a variable into a member.
- [`cmp`](cmp.md): compares a variable.
