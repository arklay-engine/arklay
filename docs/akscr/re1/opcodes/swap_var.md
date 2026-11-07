# `swap_var`

Moves a pickup quantity between an item AOT and a state byte.

| | |
| --- | --- |
| Opcode | `0x4C` |
| Group | Room actions (AOTs) |
| Kind | Command |
| Size | 4 bytes |

## Syntax

```c
swap_var(mode, aot, byte);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `mode` | `u8` | `XFER_SAVE` (0), `XFER_RESTORE` (1) or `XFER_HELD` (2). |
| `aot` | `u8` | The item AOT slot. |
| `byte` | `u8` | The state byte that keeps the quantity, a `B_xx` name (the shipped rooms use `B_0C` to `B_0E`). |

## Behaviour

An item placed back in a room (a weapon on its rack) keeps the quantity it
was left with in a state byte, since the item AOT is rebuilt on every room
load.

- `XFER_SAVE` copies the AOT's quantity into the state byte.
- `XFER_RESTORE` copies the state byte into the AOT's quantity.
- `XFER_HELD` looks the AOT's item up in the inventory and writes the
  quantity carried into both. It does nothing when the player carries none.

Nothing happens when the AOT slot is empty, the mode is unknown or the byte
is past the state block.

## Example

```c
item_aot_set(AOT_01_ITEM_WEAPON_SHOTGUN_0, 10200, 5700, 1400, 1400, IT_03_WEAPON_SHOTGUN, 7, IMDL_00_WEAPON_SHOTGUN, SCA_NONE, 10960, -3070, 6490, 3072, F_ITEM_116_WEAPON_SHOTGUN_0, SAT_MANUAL | SAT_PLAYER, 0);
swap_var(XFER_RESTORE, AOT_01_ITEM_WEAPON_SHOTGUN_0, B_0C);
```

Room 116 (`games/re1/room/116/room.akscr`): the shotgun on its rack keeps
the rounds it had when the player put it back, which the
`place_shotgun_on_rack` event saves with `XFER_HELD`.

## See also

- [`item_aot_set`](item_aot_set.md): sets up the item AOT.
- [`setb`](setb.md): writes a state byte.
