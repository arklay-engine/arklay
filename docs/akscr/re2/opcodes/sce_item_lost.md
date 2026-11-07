# `sce_item_lost`

Takes one of an item away from the player.

| | |
| --- | --- |
| Opcode | `0x62` |
| Group | Items and the player's state |
| Kind | Instruction |
| Size | 2 bytes |
| Status | yes |

## Syntax

```c
sce_item_lost(item);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `item` | `u8` | The item id. |

## Behaviour

Takes one off the first inventory stack of `item` that has a count above
0, and frees the slot when its count reaches 0. If no stack of `item` has a
count above 0 (including an item the player does not have) nothing changes.
For an item that does not stack, whose count is 1, that takes the item away.

The port takes one unit, as its other uses of an item do. Whether the
original's handler clears the whole slot instead has not been checked; the
difference shows only for a stack.

## Example

```c
proc main_35
{
    sce_item_lost(71);           // the Unicorn Medal is placed in the statue
    set(4, 72, 1);
    evt_end(0);
}
```

Illustrative, after room 200's statue.

## See also

- [`keep_item_ck`](keep_item_ck.md): tests for an item first.
- [`sce_item_get`](sce_item_get.md): gives one.
