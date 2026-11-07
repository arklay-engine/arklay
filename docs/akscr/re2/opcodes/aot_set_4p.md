# `aot_set_4p`

Puts an interaction area with four corners in one of the room's 32 area slots.

| | |
| --- | --- |
| Opcode | `0x67` |
| Group | Interaction areas and doors |
| Kind | Instruction |
| Size | 28 bytes |
| Status | yes |

## Syntax

```c
aot_set_4p(slot, sce, sat, floor, super, x0, z0, x1, z1, x2, z2, x3, z3, d0, d1, d2, d3, d4, d5);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `slot` | `u8` | The area slot, `0` to `31`. |
| `sce` | `u8` | The kind, as for [`aot_set`](aot_set.md). |
| `sat` | `u8` | The trigger byte, as for `aot_set`. |
| `floor` | `u8` | The floor; `0x80` and up for every floor. |
| `super` | `u8` | The body or object the area moves with (bit 7 set), as for `aot_set`; otherwise 0. |
| `x0`, `z0` ... `x3`, `z3` | `i16` | The four corners of the area, in order around it. |
| `d0` ... `d5` | `u8` | Six bytes of data for the kind, as for `aot_set`. |

## Behaviour

`aot_set_4p` is [`aot_set`](aot_set.md) with a four-sided area in place of the
box: a point is inside when it lies within the quadrilateral the corners
make, tested with the original's signed coordinates. Everything else (the
trigger, the floor, the attachment, the kinds and their data) is as for
`aot_set`.

For a damage area (kind 11), the first two corners stand where the box's
`x, z, w, d` would be in the record the damage reads.

## Example

```c
aot_set_4p(4, 5, 0x01, 0, 0, -3000, 1000, -1000, 1400, -1400, 3400, -3400, 3000, 11, 0, 0, 3, 0, 0);
```

Illustrative: a slanted event area that starts `main_03` in task 11 (the first
free slot) while the player is inside it. The corners and trigger byte are
placeholders.

## See also

- [`aot_set`](aot_set.md): the box form and the meaning of every field.
- [`door_aot_set_4p`](door_aot_set_4p.md), [`item_aot_set_4p`](item_aot_set_4p.md): the other four-corner areas.
