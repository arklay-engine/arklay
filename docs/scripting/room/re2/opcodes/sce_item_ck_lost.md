# `sce_item_ck_lost`

By its name, checks for an item and takes it away (not modelled).

| | |
| --- | --- |
| Opcode | `0x88` |
| Group | Items and the player's state |
| Kind | Instruction |
| Size | 3 bytes |
| Status | no |

## Syntax

```c
sce_item_ck_lost(f0, f1);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `f0` | `u8` | Not read by the port. Not known. |
| `f1` | `u8` | Not read by the port. Not known. |

## Behaviour

Not modelled: the instruction does nothing, and the log notes
`RE2 script: sce_item_ck_lost is not modelled` once per run.

Its name is the original handler's, `Sce_item_ck_lost`; by the name it
checks for an item and takes it away, but that and its operands have not been
looked at. It is not a condition in the opcode table.

Use [`keep_item_ck`](keep_item_ck.md) and [`sce_item_lost`](sce_item_lost.md)
instead.

## Example

```c
proc main_37
{
    if (keep_item_ck(71))        // in place of sce_item_ck_lost
    {
        sce_item_lost(71);
    }
    evt_end(0);
}
```

Illustrative: the modelled way to check for an item and take it.

## See also

- [`keep_item_ck`](keep_item_ck.md), [`sce_item_lost`](sce_item_lost.md): the modelled instructions.
