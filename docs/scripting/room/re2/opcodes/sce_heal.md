# `sce_heal`

By its name, heals the player (not modelled).

| | |
| --- | --- |
| Opcode | `0x83` |
| Group | Items and the player's state |
| Kind | Instruction |
| Size | 1 byte |
| Status | no |

## Syntax

```c
sce_heal();
```

## Parameters

None.

## Behaviour

Not modelled: the instruction does nothing, and the log notes
`RE2 script: sce_heal is not modelled` once per run.

Its name is the original handler's, `Sce_heal`; by the name it restores
the player's health, but how much has not been looked at. The player's
health does not change in the port.

## Example

```c
proc main_3A
{
    sce_heal();
    evt_end(0);
}
```

Illustrative: the instruction's form only.

## See also

- [`heal_partner`](heal_partner.md): the same for the partner, also not modelled.
