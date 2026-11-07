# `heal_partner`

By its name, heals the partner (not modelled).

| | |
| --- | --- |
| Opcode | `0x89` |
| Group | Items and the player's state |
| Kind | Instruction |
| Size | 1 byte |
| Status | no |

## Syntax

```c
heal_partner();
```

## Parameters

None.

## Behaviour

Not modelled: the instruction does nothing, and the log notes
`RE2 script: heal_partner is not modelled` once per run.

By its name, it restores the health of the partner (`splayer_work`); that
has not been checked against the original's handler.

## Example

```c
proc main_3B
{
    heal_partner();
    evt_end(0);
}
```

Illustrative: the instruction's form only.

## See also

- [`sce_heal`](sce_heal.md): the same for the player, also not modelled.
