# `item_ck`

Passes when the player carries an item.

| | |
| --- | --- |
| Opcode | `0x1A` |
| Group | Conditions |
| Kind | Condition |
| Size | 2 bytes |

## Syntax

```c
item_ck(item);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `item` | `u8` | The item id (`IT_xx_*` from `inc/items.akinc`). |

## Behaviour

`item_ck` is a [condition](index.md#conditions): it stands in an `if`
head, a `require` or a `do`/`while` test.

It passes when any inventory slot holds `item`, whatever its quantity. As in
the original, which scans slot ids only, a stack used down to 0 (or the
starting knife, quantity 0) still counts while its slot exists. The item
box is not searched.

## Example

```c
if (item_ck(IT_13_CHEMICAL_EMPTY_BOTTLE))
{
    aot_reset(AOT_08_MESSAGE_FAUCET, SCE_EVENT, SAT_MANUAL | SAT_PLAYER, 9, fill_bottle_with_water, 0);
    ...
}
```

Room 409 (`games/re1/room/409/room.akscr`): with an empty bottle in hand,
the faucet and the chemical tanks become events that fill it.

## See also

- [`ck_item_count`](ck_item_count.md): the quantity of a group of items.
- [`ck_last_item`](ck_last_item.md): the equipped weapon.
- [`remove_item`](remove_item.md): takes an item out of the inventory.
