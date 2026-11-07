# `sce_85`

An unnamed condition that always passes, because the port does not model it.

| | |
| --- | --- |
| Opcode | `0x85` |
| Group | Unnamed and unused |
| Kind | Condition |
| Size | 6 bytes |
| Status | no |

## Syntax

```c
sce_85(f0, f1, f2, w0);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `f0` | `u8` | Not read by the port. Not known. |
| `f1` | `u8` | Not read by the port. Not known. |
| `f2` | `u8` | Not read by the port. Not known. |
| `w0` | `u16` | Not read by the port. Not known. |

## Behaviour

Not modelled: the condition always passes, and the log notes
`RE2 script: sce_85 is not modelled` once per run.

The opcode has no name in biohazard-utils and none from the original's
handler, so its mnemonic is its number. What the handler does has not been
looked at, and the operands' meanings are not known.

The opcode table marks it a condition, so the original's handler can fail;
the port never fails it.

## Example

```c
proc main_43
{
    while (sce_85(0, 0, 0, 0))
    {
        evt_next();
    }
    evt_end(0);
}
```

Illustrative: in the port this loop never ends by itself, since the
condition always holds, and the task waits a tick at a time for ever. Do not
wait on an unmodelled condition.

## See also

- [`sce_81`](sce_81.md): the other unnamed condition.
