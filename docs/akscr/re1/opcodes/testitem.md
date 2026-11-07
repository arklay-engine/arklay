# `testitem`

Passes when the item the player has just used is the one named.

| | |
| --- | --- |
| Opcode | `0x10` |
| Group | Conditions |
| Kind | Condition |
| Size | 2 bytes |

## Syntax

```c
testitem(item);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `item` | `u8` | The item id (`IT_xx_*` from `inc/items.akinc`). |

## Behaviour

`testitem` is a [condition](index.md#conditions): it stands in an `if`
head, a `require` or a `do`/`while` test.

It compares the used-item state byte (`B_USED_ITEM`) with `item`. Using an
item from the inventory writes that byte, and the
engine clears it after every frame's main scripts have run, so the test
passes on exactly one pass of `proc main`. As in the original, the test is a
plain byte compare: `testitem(IT_00_NONE)` passes on every frame no item
was used.

## Example

```c
if (testitem(IT_03_WEAPON_SHOTGUN))
{
    set(FG_ITEM, F_ITEM_116_WEAPON_SHOTGUN_0, TRUE);
    setbyte(0, 1, 0);
    evt_exec(0, TASK_09, rack_settles);
    ...
}
```

Room 116 (`games/re1/room/116/room.akscr`): putting the shotgun back on the
rack.

## See also

- [`testpickup`](testpickup.md): the item just picked up.
- [`item_ck`](item_ck.md): whether an item is carried.
