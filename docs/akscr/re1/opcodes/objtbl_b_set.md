# `objtbl_b_set`

Writes the flags byte of a room object or an item model.

| | |
| --- | --- |
| Opcode | `0x35` |
| Group | Objects and models |
| Kind | Command |
| Size | 4 bytes |

## Syntax

```c
objtbl_b_set(table, index, flags);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `table` | `u8` | `TBL_OBJ` (0) for a room object, `TBL_ITEM` (1) for an item model. |
| `index` | `u8` | The object (`OBJ_xx`) or the item model (`IMDL_xx`), room defines. |
| `flags` | `u8` | The new flags byte (`OF_*`). 0 hides the object or model; `OF_ACTIVE` draws it. |

## Behaviour

`objtbl_b_set` replaces the whole flags byte of the record, the byte
[`obj`](obj.md) or [`item_aot_set`](item_aot_set.md) set up: for an object
that is whether it is drawn and how it collides, for an item model only
bit 0, whether it is drawn, counts. A table other than 0 or 1, or an index
past the room's records, does nothing. It does not touch the record's AOT,
so a script that shows an item model again re-arms its pickup with
[`aot_reset`](aot_reset.md) too.

The original has one special case, which the port keeps: in the water
tank entry (stage 4, room `0x0D`), a write to object 5 clears its flags to
0 whatever the table and value.

The `.s` assembler prints the instruction as raw bytes; `.akscr` takes one
argument per field.

## Example

```c
if (cmpb(B_USED_ITEM, CMP_EQ, IT_22_KEY_JEWEL_RED))
{
    objtbl_b_set(TBL_ITEM, IMDL_01_WEAPON_MAGNUM, OF_ACTIVE);
    aot_reset(AOT_04_ITEM_WEAPON_MAGNUM_1, SCE_ITEM, SAT_MANUAL | SAT_PLAYER, 0, 1, 0);
    eml_pos(OBJ_02_RED_EYE, -80, 0, 230, 510, -2310, -110);
    // ...
}
```

Room 10d (`games/re1/room/10d/room.akscr`), the tiger statue: setting the
red jewel shows the magnum's model and arms its pickup.

## See also

- [`obj`](obj.md): sets up a room object.
- [`item_aot_set`](item_aot_set.md): sets up an item pickup and its model.
- [`setbyte`](setbyte.md): writes an item model's flags byte by its index alone.
