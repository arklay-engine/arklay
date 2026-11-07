# `poison_clr`

By its name, cures the player's poisoning (not modelled).

| | |
| --- | --- |
| Opcode | `0x87` |
| Group | Items and the player's state |
| Kind | Instruction |
| Size | 1 byte |
| Status | no |

## Syntax

```c
poison_clr();
```

## Parameters

None.

## Behaviour

Not modelled: the instruction does nothing, and the log notes
`RE2 script: poison_clr is not modelled` once per run.

Its name is the original handler's, `Poison_clr`. The port does not model
poison.

## Example

```c
proc main_39
{
    poison_clr();
    evt_end(0);
}
```

Illustrative: the instruction's form only.

## See also

- [`poison_ck`](poison_ck.md): tests for poison, also not modelled.
