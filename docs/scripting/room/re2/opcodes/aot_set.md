# `aot_set`

Puts an interaction area with a rectangular box in one of the room's 32 area slots.

| | |
| --- | --- |
| Opcode | `0x2C` |
| Group | Interaction areas and doors |
| Kind | Instruction |
| Size | 20 bytes |
| Status | yes |

## Syntax

```c
aot_set(slot, sce, sat, floor, super, x, z, w, d, d0, d1, d2, d3, d4, d5);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `slot` | `u8` | The area slot, `0` to `31`. A larger slot is refused with a warning. |
| `sce` | `u8` | The kind (below). |
| `sat` | `u8` | The trigger byte: who and what sets the area off (below). |
| `floor` | `u8` | The floor the area is on; `0x80` and up for every floor. |
| `super` | `u8` | With bit 7 set, the area moves with a body or an object (below); otherwise 0. |
| `x`, `z` | `i16` | The box's near corner. |
| `w`, `d` | `i16` | The box's width (along x) and depth (along z), read as unsigned. |
| `d0` ... `d5` | `u8` | Six bytes of data for the kind. |

## Behaviour

`aot_set` fills slot `slot` of the room's interaction-area table, replacing
whatever was there, door record included. The `aot_*` instructions share this
layout; [`aot_set_4p`](aot_set_4p.md) replaces the box with four corners.

**The probe.** Each tick, the engine tests the areas against the player. A
point is inside when `x <= px < x + w` and `z <= pz < z + d`, on the area's
floor. The area the probe finds in front of the player goes in variable 0,
and the one the player stands in goes in variable 1.

`sat` chooses the test:

- Bits 0 to 2 choose who sets the area off: `1` the player, `2` an enemy
  (which only records the area for the enemy; see
  [`member_cmp`](member_cmp.md) member 9), `4` a room object's pass.
- `0x10` makes it an action area: it fires only when the player presses the
  action button, and only the first such area that matches fires. An action
  area is for the player only, whatever bits 0 to 2 say. Otherwise the area fires on every tick the player is in it.
- `0x20` tests the point in front of the player (the reach) instead of the
  player's own position.

`super` with bit 7 set attaches the area: the box moves by the `x` and `z` of
what bits 0 to 6 name. With bit 6 set, the low six bits are `0` the player,
`1` the partner, or `n` the character in enemy slot `n - 2`; with bit 6
clear, they are a room object's slot.

**The kinds.** When the area fires, its kind decides what happens:

| `sce` | Kind | Data |
|---|---|---|
| 0 | none: never fires, records nothing | |
| 1 | door | use [`door_aot_set`](door_aot_set.md); an `aot_set` door has no record and does nothing |
| 2 | item | use [`item_aot_set`](item_aot_set.md) |
| 3 | normal: records the hit in variables 0 and 1 only | |
| 4 | message | `d0` the room message to show |
| 5 | event | `d0`, `d1` the task (a 16-bit word, as [`evt_exec`](evt_exec.md)'s), `d2` padding, `d3` the main procedure to start |
| 6 | flag change | three words: the bank (`d0`, `d1`), the bit (`d2`, `d3`), and whether to set (non-zero) or clear it (`d4`, `d5`) |
| 7 | water | `d0`, `d1` the water's level, which the player has while inside |
| 9 | typewriter (saving) | |
| 10 | item box | `d0` the box's object |
| 11 | damage | `d0` the kind of damage, `d2`, `d3` the amount |

An event area queues its procedure, which starts at the start of the next
tick, before the tasks run, in the task the data names (10 and up: the first
free slot from 2 to 9). Kinds the port does not give a game side (8, the
slopes, among them) only record their hit.

The areas belong to the room: its init scripts set them up again on each
entry.

## Example

```c
proc init_00
{
    if (ck(5, 4, 0))
    {
        aot_set(2, 5, 0x01, 0, 0, -1200, 4400, 1500, 1200, 10, 0, 0, 2, 0, 0);
    }
    aot_set(3, 4, 0x31, 0, 0, 2600, -800, 1000, 700, 9, 0, 0, 0, 0, 0);
    evt_end(0);
}
```

Illustrative: area 2 is an event area that runs `main_02` in the first free
task while the player walks through it, until room bit 4 is set. Area 3 shows
room message 9 when the player faces it and presses the action button. The
coordinates and trigger bytes are placeholders; copy real ones from the
room's own `aot_set`s.

## See also

- [`aot_set_4p`](aot_set_4p.md): the same with four corners.
- [`aot_reset`](aot_reset.md): changes an area's kind and data.
- [`aot_on`](aot_on.md): fires an area from the script.
- [`door_aot_set`](door_aot_set.md), [`item_aot_set`](item_aot_set.md): doors and items.
