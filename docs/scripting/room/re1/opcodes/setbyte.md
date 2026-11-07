# `setbyte`

Writes an item model record's flag byte, which shows or hides the model.

| | |
| --- | --- |
| Opcode | `0x19` |
| Group | Room actions (AOTs) |
| Kind | Command |
| Size | 4 bytes |

## Syntax

```c
setbyte(model, value, 0);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `model` | `u8` | The item model slot (`IMDL_xx`). |
| `value` | `u8` | The new flag byte: 0 hides the model, 1 draws it. |
| *pad* | `u8` | Unused, written as 0. |

## Behaviour

`setbyte` replaces the first byte of item model record `model`, the byte
[`item_aot_set`](item_aot_set.md) sets to 1 for an item still in the room
and 0 for a taken one. Only that byte changes: the AOT, the item and its
`FG_ITEM` bit stay as they are. Hiding a model does not remove its pick-up
sparkle, which runs out on its own, as in the original.

The opcode table also calls the instruction `model_flag_set`; both names
compile to the same opcode, and the decompiler writes `setbyte`. Nothing
happens for a slot past the item model table.

## Example

```c
eml_rot(IMDL_01_AMMO_SHOTGUN, 0, -1100);
setbyte(1, 0, 0);
```

Room 102 (`games/re1/room/102/room.akscr`): the init tilts item model 1 and
hides it.

## See also

- [`item_aot_set`](item_aot_set.md): builds the item model.
- [`objtbl_b_set`](objtbl_b_set.md): writes an object's or item model's flags.
