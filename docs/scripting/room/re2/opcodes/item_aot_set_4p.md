# `item_aot_set_4p`

Puts an item to pick up, with a four-cornered area, in one of the room's area slots.

| | |
| --- | --- |
| Opcode | `0x69` |
| Group | Interaction areas and doors |
| Kind | Instruction |
| Size | 30 bytes |
| Status | yes |

## Syntax

```c
item_aot_set_4p(slot, sce, sat, floor, super, x0, z0, x1, z1, x2, z2, x3, z3, item, count, flag, model, action);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `slot`, `sce`, `sat`, `floor`, `super` | `u8` | As for [`item_aot_set`](item_aot_set.md). |
| `x0`, `z0` ... `x3`, `z3` | `i16` | The four corners of the area, in order around it. |
| `item`, `count`, `flag` | `u16` | The item, how many, and its bit in bank 8, as for `item_aot_set`. |
| `model`, `action` | `u8` | The object that shows it and its action bits, as for `item_aot_set`. |

## Behaviour

`item_aot_set_4p` is [`item_aot_set`](item_aot_set.md) with a four-sided area
in place of the box, tested as [`aot_set_4p`](aot_set_4p.md)'s is. The item,
its flag, its object and the pick-up are the same.

## Example

```c
item_aot_set_4p(6, 2, 0x31, 0, 0, 100, 200, 900, 200, 900, 1000, 100, 1000, 0x47, 1, 41, 255, 0);
```

Illustrative: one of item `0x47`, with no object, recorded by bank 8 bit 41.
The values are placeholders.

## See also

- [`item_aot_set`](item_aot_set.md): the box form and the pick-up.
