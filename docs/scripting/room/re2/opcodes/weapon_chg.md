# `weapon_chg`

Equips a weapon, or takes the one in hand away.

| | |
| --- | --- |
| Opcode | `0x5A` |
| Group | Items and the player's state |
| Kind | Instruction |
| Size | 2 bytes |
| Status | yes |

## Syntax

```c
weapon_chg(item);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `item` | `u8` | The weapon's item id. `0` equips nothing. |

## Behaviour

Makes `item` the equipped weapon (the player's `Type` member, read with
[`member_copy`](member_copy.md), shows it in its low bits from then on). As
the original's `Weapon_chg`, the player's model takes the change the next
time the tick looks (`Pl_weapon_ch`). The port does not check that the player
carries the item. With the infinite-ammunition cheat on, the new weapon is
filled.

## Example

```c
proc main_36
{
    weapon_chg(0);               // put the weapon away for the scene
    cut_chg(2);
    evt_end(0);
}
```

Illustrative: a scene's start puts the player's weapon away.

## See also

- [`keep_item_ck`](keep_item_ck.md): tests for an item.
