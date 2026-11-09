# `heal_partner`

Heals the partner: 200 of 200, and the poison goes.

| | |
| --- | --- |
| Opcode | `0x89` |
| Group | Items and the player's state |
| Kind | Instruction |
| Size | 1 byte |
| Status | yes |

## Syntax

```c
heal_partner();
```

## Parameters

None.

## Behaviour

As the original's `Sce_heal_partner` (`0x4e8d70`): the partner's poison is
cleared, her saved life (what a save keeps, and what [`splc_ret`](splc_ret.md)
gives her) becomes 200, and the partner in play, if there is one, has a life
and a maximum life of 200. It works on the partner whatever the task's work is.

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

- [`sce_heal`](sce_heal.md): the same for the player, not modelled.
