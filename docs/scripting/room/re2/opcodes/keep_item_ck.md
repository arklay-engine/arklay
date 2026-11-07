# `keep_item_ck`

Passes when the player carries an item.

| | |
| --- | --- |
| Opcode | `0x5E` |
| Group | Items and the player's state |
| Kind | Condition |
| Size | 2 bytes |
| Status | yes |

## Syntax

```c
keep_item_ck(item);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `item` | `u8` | The item id, as the inventory and the item areas number items (`0x1E` the ink ribbon, `0x47` the Unicorn Medal). |

## Behaviour

Passes when one of the player's inventory slots holds `item`, whatever its
count: an empty stack still counts while its slot exists, as the original's
slot scan does. It looks only at the inventory, not at the item box or the
eleventh slot that holds the lighter or the lockpick. A failure works as for
any [condition](../index.md#conditions).

## Example

```c
proc main_33
{
    if (keep_item_ck(71))        // the Unicorn Medal
    {
        sce_item_lost(71);
        set(4, 72, 1);
    }
    else
    {
        message_on(0, 4, 0, 255, 255);
    }
    evt_end(0);
}
```

Illustrative: an event area that takes the Unicorn Medal if the player has
it and otherwise shows a message.

## See also

- [`sce_item_lost`](sce_item_lost.md): takes an item away.
- [`sce_item_get`](sce_item_get.md): gives one.
- [`item_aot_set`](item_aot_set.md): an item to pick up.
