# `sce_item_get`

Gives the player an item, a condition that always passes in the port.

| | |
| --- | --- |
| Opcode | `0x76` |
| Group | Items and the player's state |
| Kind | Condition |
| Size | 3 bytes |
| Status | yes |

## Syntax

```c
sce_item_get(item, count);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `item` | `u8` | The item id. |
| `count` | `u8` | How many: the rounds for ammunition, 1 for most items. |

## Behaviour

Adds `count` of `item` to the inventory at once, with the pick-up's rules:
an item that stacks (ids `0x14` to `0x1F`, the ammunition, the ink ribbon and
the small key) joins a stack of the same id while the total fits the item's
maximum; if it does not fit, the stack is filled and the rest takes a new
slot. Anything else takes a new slot. No window opens and no message shows:
a script that wants them shows them itself.

The port does not check that the inventory has room, and the condition never
fails. The opcode table marks it a condition, so the original's handler can
fail; when it does (a full inventory would be the obvious case) has not been
checked, so a script that relies on the failure behaves differently in the
port.

## Example

```c
proc main_34
{
    if (ck(8, 20, 0))
    {
        sce_item_get(20, 15);    // 15 handgun rounds
        set(8, 20, 1);
        message_on(0, 7, 0, 255, 255);
    }
    evt_end(0);
}
```

Illustrative: a one-off gift of ammunition, remembered in bank 8 (the items
picked up), with a message the room provides.

## See also

- [`keep_item_ck`](keep_item_ck.md): tests for an item.
- [`sce_item_lost`](sce_item_lost.md): takes one away.
- [`item_aot_set`](item_aot_set.md): an item to pick up, with the get-item window.
