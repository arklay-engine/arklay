# `item_aot_set`

Sets up an item pickup: its zone, the item, its model and the flag that
says it is still there.

| | |
| --- | --- |
| Opcode | `0x18` |
| Group | Room actions (AOTs) |
| Kind | Command |
| Size | 26 bytes |

## Syntax

```c
item_aot_set(aot, x, z, width, depth, item, quantity, model, parent,
             model_x, model_y, model_z, yaw, item_flag, trigger, flags);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `aot` | `u8` | The AOT slot, 0 to 31. `ITEM_ALT_ROT \| aot` seeds the model's alternate rotation. |
| `x` | `i16` | The zone's corner on the floor, x. |
| `z` | `i16` | The zone's corner, z. |
| `width` | `i16` | The zone's size along x. |
| `depth` | `i16` | The zone's size along z. |
| `item` | `u8` | The item, `IT_*` from `inc/items.akinc`. |
| `quantity` | `u8` | How many (rounds, uses); 0 for an item that does not stack. |
| `model` | `u8` | The item model slot, the room's `IMDL_xx` defines. |
| `parent` | `u8` | What the model is attached to: `SCA_NONE`, `SCA_PLAYER`, `SCA_ENEMY \| ENEMY_xx`, or an object (`OBJ_xx`). |
| `model_x` | `i16` | The model's position, x. |
| `model_y` | `i16` | The model's position, y. |
| `model_z` | `i16` | The model's position, z. |
| `yaw` | `i16` | The model's turn about the vertical axis, 0 to 4095. |
| `item_flag` | `u8` | The `FG_ITEM` bit that is set while the item is still in the room (`F_ITEM_<room>_*`). |
| `trigger` | `u8` | The trigger flags, `SAT_*`. |
| `flags` | `u16` | Pick-up flags: `ITEM_REACH` (the player reaches for it), `ITEM_SPARKLE` (the pick-up sparkle) with its effect `ITEM_FX_1` to `ITEM_FX_F`; bits `0x00F0` raise the sparkle (2 units per step, a number). |

## Behaviour

`item_aot_set` builds the item model record `model` and fills AOT slot
`aot`, replacing what it held. Its handler comes from the item: an ordinary
item gets the pick-up handler, a map (`0x4E` to `0x53`) the map pick-up and
a file its document handler.

When the `item_flag` bit of `FG_ITEM` is clear the item has been taken: the
model is built hidden and the AOT is inert, though a script can still
re-arm the zone. While the bit is set, the model is drawn and the zone opens
the item's pick-up prompt; picking the item up adds it to the inventory,
clears the bit and hides the model.

With `ITEM_REACH` the player plays the reach animation before the prompt.
With `ITEM_SPARKLE` the item gets the glint effect while it is there,
riding the model and its parent.

An ink ribbon is not placed for a character who does not need ribbons (the
ink ribbon options): its `item_flag` bit is cleared and its zone kept,
inert. The port keeps the zone, where the original leaves the slot unset,
so a room that turns it into a typewriter afterwards still has one.

Two rooms' models are adjusted as in the original: the second item built in
the second visit's trophy room sits 150 units lower, and the courtyard and
guardhouse maps darken their palette when the model built is not the one
built last.

## Example

```c
item_aot_set(AOT_03_ITEM_KEY_MANSION_SHIELD_0, 17400, 14500, 1000, 1000, IT_35_KEY_MANSION_SHIELD, 1, IMDL_00_KEY_MANSION_SHIELD, SCA_NONE, 18000, -2400, 14880, 0, F_ITEM_105_KEY_MANSION_SHIELD_0, SAT_MANUAL | SAT_PLAYER, ITEM_SPARKLE | ITEM_FX_6);
```

Room 105 (`games/re1/room/105/room.akscr`): the shield key, with its
sparkle, until it is taken.

## See also

- [`give_item`](give_item.md): awards an item AOT through the got-item viewer.
- [`setbyte`](setbyte.md): shows or hides an item model.
- [`swap_var`](swap_var.md): moves a pickup quantity in and out of a state byte.
- [`eml_rot`](eml_rot.md): turns an item model.
