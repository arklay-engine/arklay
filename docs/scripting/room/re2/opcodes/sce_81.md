# `sce_81`

An unnamed condition that always passes, because the port does not model it.

| | |
| --- | --- |
| Opcode | `0x81` |
| Group | Unnamed and unused |
| Kind | Condition |
| Size | 3 bytes |
| Status | no |

## Syntax

```c
sce_81(f0, f1);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `f0` | `u8` | Not read by the port. Not known. |
| `f1` | `u8` | Not read by the port. Not known. |

## Behaviour

Not modelled: the condition always passes, and the log notes
`RE2 script: sce_81 is not modelled` once per run.

The opcode has no name in biohazard-utils and none from the original's
handler, so its mnemonic is its number. What the handler does has not been
looked at, and the operands' meanings are not known.

The opcode table marks it a condition, so the original's handler can fail;
the port never fails it.

## Example

```c
proc main_40
{
    if (sce_81(0, 0))
    {
        set(5, 47, 1);
    }
    evt_end(0);
}
```

Illustrative: in the port the `if` body always runs.

## See also

- [`sce_85`](sce_85.md): the other unnamed condition.
