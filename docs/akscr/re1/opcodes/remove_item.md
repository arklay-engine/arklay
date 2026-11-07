# `remove_item`

Takes an item out of the inventory.

| | |
| --- | --- |
| Opcode | `0x2C` |
| Group | Room actions (AOTs) |
| Kind | Command |
| Size | 2 bytes |

## Syntax

```c
remove_item(item);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `item` | `u8` | The item, `IT_*`. |

## Behaviour

`remove_item` removes the first inventory slot holding `item`, the whole
stack whatever its quantity, and closes up the inventory. When the player
carries none, nothing happens.

The equipped weapon follows the slots as the original's does: the original
keeps the equipped slot's index and shifts it as the inventory closes up.
So removing an item before the equipped one keeps the same weapon equipped,
and removing the equipped item itself equips whatever moves into its slot
(nothing when it was the last).

`remove_item` is not a condition and cannot stand in an `if` head.

## Example

```c
aot_reset(AOT_01_DOOR_20E, SCE_DOOR, SAT_MANUAL | SAT_PLAYER, 1, 0, 0);
aot_on(AOT_01_DOOR_20E, SCE_DOOR, 0);
remove_item(IT_42_KEY_SERUM);
```

Room 20D (`games/re1/room/20d/room.akscr`): the serum is used up as the
player leaves with it.

## See also

- [`item_ck`](item_ck.md): tests whether the player carries an item.
- [`give_item`](give_item.md): gives an item through the got-item viewer.
