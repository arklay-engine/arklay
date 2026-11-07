# `dir_ck`

Tests whether a point lies within an angle of the way the task's work faces.

| | |
| --- | --- |
| Opcode | `0x39` |
| Group | Characters, objects and the work |
| Kind | Condition |
| Size | 8 bytes |
| Status | yes |

## Syntax

```c
dir_ck(0, x, z, range);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `_` | `u8` | Padding, not read. Write 0. |
| `x` | `i16` | The point's x. |
| `z` | `i16` | The point's z. |
| `range` | `i16` | Half the width of the cone, in angle units (4096 to a turn). |

## Behaviour

Works out the direction from the work's position (members 11 and 13) to the
point `x, z` on the ground plane, as the original's `Dir_ck` does, and passes
when that direction is within `range` either side of the work's facing
(member 15): when `(direction - facing + range) & 0x0FFF` is less than
`2 * range`. So `range` 1024 passes for anything in the front half, and
`range` 0 never passes. A failure works as for any
[condition](../index.md#conditions).

If the work has no position or facing the port can read (a work the engine
does not model), the condition passes.

## Example

```c
proc main_10
{
    work_set(1, 0);
    if (dir_ck(0, 4500, -2000, 256))
    {
        message_on(0, 3, 0, 255, 255);
    }
    evt_end(0);
}
```

Illustrative: the message shows only when the player faces the point
`4500, -2000`, to within 256 units (22.5 degrees) either side.

## See also

- [`dir_set`](dir_set.md): turns the work.
- [`member_cmp`](member_cmp.md): compares the facing (member 15) directly.
