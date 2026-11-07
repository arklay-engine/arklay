# `mizu_div_set`

A condition named for water (*mizu*) that always passes, because the port does not model it.

| | |
| --- | --- |
| Opcode | `0x5D` |
| Group | Camera, room and screen |
| Kind | Condition |
| Size | 2 bytes |
| Status | no |

## Syntax

```c
mizu_div_set(value);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `value` | `u8` | Not read by the port. Not known. |

## Behaviour

Not modelled: the condition always passes, and the log notes
`RE2 script: mizu_div_set is not modelled` once per run.

The opcode table marks it a condition, so the original's handler can fail
and stop the script where a failed condition stops it (see
[Conditions](../index.md#conditions)). Its name is the handler's, `Mizu_div_set`
(*mizu* is water); when it fails and what it sets has not been looked at.
In the port it never fails, so a script that waits on it moves on at once.

## Example

```c
proc main_2F
{
    if (mizu_div_set(0))
    {
        set(5, 46, 1);
    }
    evt_end(0);
}
```

Illustrative: in the port the `if` body always runs.

## See also

- [`aot_set`](aot_set.md): a water area is an interaction area of kind 7, which is modelled.
