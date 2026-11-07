# `item_aot_set`

Puts an item to pick up in one of the room's area slots.

| | |
| --- | --- |
| Opcode | `0x4E` |
| Group | Interaction areas and doors |
| Kind | Instruction |
| Size | 22 bytes |
| Status | yes |

## Syntax

```c
item_aot_set(slot, sce, sat, floor, super, x, z, w, d, item, count, flag, model, action);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `slot` | `u8` | The area slot, `0` to `31`. A larger slot is refused with a warning. |
| `sce` | `u8` | The kind: `2` (item). |
| `sat` | `u8` | The trigger byte, as for [`aot_set`](aot_set.md); an item is usually an action area (`0x10`). |
| `floor` | `u8` | The floor; `0x80` and up for every floor. |
| `super` | `u8` | The body or object the area moves with (bit 7 set), as for `aot_set`; otherwise 0. |
| `x`, `z` | `i16` | The box's near corner. |
| `w`, `d` | `u16` | The box's width and depth. |
| `item` | `u16` | The item's id. The port reads the low byte. |
| `count` | `u16` | How many the player gets. The port reads the low byte. |
| `flag` | `u16` | The item's bit in bank 8, set once it is taken. The port reads the low byte. |
| `model` | `u8` | The room object (slot below 32) that shows the item, or a larger value for none. |
| `action` | `u8` | Bit 0: the player crouches to pick it up. Bit 7: the object twinkles, bits 5 and 6 giving the twinkle's size. Bit 1: the object is not shown. |

## Behaviour

`item_aot_set` fills slot `slot` with an item area. When bank 8 bit `flag` is
already set, the item has been taken: the area is set with kind 0, so it does
nothing, and the object `model` stays hidden whatever
[`obj_model_set`](obj_model_set.md) asks of it. Otherwise the object takes
`action` as its item flags, and is hidden if bit 1 is set.

When the area fires, or when [`aot_on`](aot_on.md) fires it, the get-item
window opens (after a crouch, if `action` bit 0 is set) and asks whether to
take the item. If the inventory has no room, it says so instead and the item
stays. On a yes, the item joins a stack of the same id that has room for all
of it (ammunition, ink ribbons and the small key stack), or the first free
slot. Its flag is set, its object and twinkle go, and the area turns to kind
0. A no leaves everything as it was. The window holds the rest of the game,
scripts included, while it is open.

The original changes some counts with the difficulty; the port does not.

## Example

```c
item_aot_set(5, 2, 0x31, 0, 0, -2400, 6100, 800, 800, 0x14, 15, 40, 3, 0x81);
```

Illustrative: 15 of item `0x14` on room object 3, which twinkles, taken
crouching; bank 8 bit 40 records it. The values are placeholders.

## See also

- [`item_aot_set_4p`](item_aot_set_4p.md): the same with four corners.
- [`aot_reset`](aot_reset.md): turns an area into an item or out of one.
- [`keep_item_ck`](keep_item_ck.md), [`sce_item_get`](sce_item_get.md): test for or give an item directly.
