# `rbj_reset`

Resets something the original calls `rbj` (not modelled).

| | |
| --- | --- |
| Opcode | `0x6C` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 1 byte |
| Status | no |

## Syntax

```c
rbj_reset();
```

## Parameters

None.

## Behaviour

Not modelled: the instruction does nothing, and the log notes
`RE2 script: rbj_reset is not modelled` once per run. Its name is the original handler's, `Rbj_reset`; what it resets has not
been looked at.

## Example

```c
proc main_21
{
    rbj_reset();
    evt_end(0);
}
```

Illustrative: the instruction's form only.
