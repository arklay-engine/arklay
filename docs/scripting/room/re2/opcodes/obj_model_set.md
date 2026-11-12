# `obj_model_set`

Places a room object's model in its object slot, or takes it away again.

| | |
| --- | --- |
| Opcode | `0x2D` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 38 bytes |
| Status | yes |

## Syntax

```c
obj_model_set(slot, mode, f2, f3, f4, parent, f6, be_flg, attr, x, y, z, rx, ry, rz, ox, oy, oz, w, h_lo, h_hi, d_lo, d_hi);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `slot` | `u8` | The object slot, which is also the room's model pair it draws. |
| `mode` | `u8` | `2` draws a push box 900 units above its position; other values do not. |
| `f2` | `u8` | Not read by the port. |
| `f3` | `u8` | Not read by the port. |
| `f4` | `u8` | Not read by the port. |
| `parent` | `u16` | The high byte chooses what the object is attached to: with bits `0xC0` set, object `high & 0x3F`; with only `0x40`, the player; otherwise nothing. The low byte is not read. |
| `f6` | `u16` | Not read by the port. |
| `be_flg` | `i16` | The object's first `Be_flg`. Bit 0 is forced on (in play); bit 4 draws it outside its camera's zone too. |
| `attr` | `i16` | The attribute: bit 1 (2) blends the model's every packet, bit 4 (16) draws it unlit, bit 5 (32) leaves it out of play. |
| `x` | `i16` | The position's x. |
| `y` | `i16` | The position's y. |
| `z` | `i16` | The position's z. |
| `rx` | `i16` | The rotation about x. |
| `ry` | `i16` | The rotation about y. |
| `rz` | `i16` | The rotation about z. |
| `ox` | `i16` | The collision box's offset in x. |
| `oy` | `i16` | Its offset in y. |
| `oz` | `i16` | Its offset in z. |
| `w` | `i16` | The collision box's width. |
| `h_lo` | `u8` | Its height, low byte. |
| `h_hi` | `u8` | Its height, high byte. |
| `d_lo` | `u8` | Its depth, low byte. |
| `d_hi` | `u8` | Its depth, high byte. |

## Behaviour

Writes object record `slot` from the operands, as the original's
`Obj_model_set` does, keeping only what an item area has already put there.
If the room has no record `slot`, nothing happens (but a blend asked for with
`attr` bit 1 is still applied to the slot's model).
The blend is applied after the tick's pass of the scripts, with the cut
edits; a room's load applies its own passes' blends before anything moves.

A second `obj_model_set` on a slot that is in use takes the object away: its
`Be_flg` becomes 0, so it is out of play and not drawn, while the rest of the
record is written as usual. A third puts it back.

The object takes the slot's own model from the room. Once placed, it can be
moved with [`work_set`](work_set.md)`(4, slot)` and the position
instructions ([`pos_set`](pos_set.md), [`dir_set`](dir_set.md),
[`add_speed`](add_speed.md), [`member_set`](member_set.md)).

The operands are the original's object table fields in order; the port reads
those named above. The height and depth are two bytes each because the
format reads them as byte pairs.

## Example

```c
proc init_00
{
    obj_model_set(1, 0, 0, 0, 0, 0, 0, 0, 0, 3200, 0, -4800, 0, 1024, 0, 0, 0, 0, 600, 0x20, 0x03, 0x58, 0x02);
    evt_end(0);
}
```

Illustrative: the room's init places object 1 at `3200, 0, -4800`, turned a
quarter turn, with a collision box 600 wide, 800 (`0x320`) high and 600
(`0x258`) deep.

## See also

- [`work_set`](work_set.md): `work_set(4, slot)` acts on the object.
- [`member_set`](member_set.md): `Be_flg` (members 0 and 1) shows and hides it.
- [`item_aot_set`](item_aot_set.md): an item area, whose model is an object.
