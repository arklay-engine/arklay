# `sce_82`

An unnamed two-operand instruction (not modelled).

| | |
| --- | --- |
| Opcode | `0x82` |
| Group | Unnamed and unused |
| Kind | Instruction |
| Size | 3 bytes |
| Status | no |

## Syntax

```c
sce_82(f0, f1);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `f0` | `u8` | Not read by the port. Not known. |
| `f1` | `u8` | Not read by the port. Not known. |

## Behaviour

Not modelled: the instruction does nothing, and the log notes
`RE2 script: sce_82 is not modelled` once per run.

The opcode has no name in biohazard-utils and none from the original's
handler, so its mnemonic is its number. What the handler does has not been
looked at, and the operands' meanings are not known.

## Example

```c
proc main_41
{
    sce_82(0, 0);
    evt_end(0);
}
```

Illustrative: the instruction's form only.
