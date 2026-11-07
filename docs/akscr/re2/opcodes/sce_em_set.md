# `sce_em_set`

Places a character (an enemy, or someone a scene moves) in an enemy slot or the partner's slot.

| | |
| --- | --- |
| Opcode | `0x44` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 22 bytes |
| Status | yes |

## Syntax

```c
sce_em_set(0, slot, id, type_lo, type_hi, floor, sound_type, model_type, dead_flag, x, y, z, dir, start, start_flags);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `_` | `u8` | Padding, not read. Write 0. |
| `slot` | `u8` | The enemy slot, `0` to `31`, or `255` for the partner (`splayer_work`). Any other value places nothing. |
| `id` | `u8` | The character's `Id`: which enemy or person it is, and what runs it. |
| `type_lo` | `u8` | `Type`'s low byte: the variant, which the character's routines read. |
| `type_hi` | `u8` | `Type`'s high byte. |
| `floor` | `u8` | The floor it stands on. |
| `sound_type` | `u8` | Its sound type, which chooses the room's enemy sound bank. |
| `model_type` | `u8` | The model variant. |
| `dead_flag` | `u8` | Its bit in the enemy flag bank: a character whose bit is set is dead and is not placed. `255` for none. |
| `x` | `i16` | The position's x. |
| `y` | `i16` | The position's y. |
| `z` | `i16` | The position's z. |
| `dir` | `i16` | The facing, 4096 to a turn. |
| `start` | `u16` | For a character the room's load places: the routine and clip it starts in, as `(routine + 1) << 8 \| clip`. `0` to start standing. |
| `start_flags` | `u16` | The `Sce_flg` it starts with when `start` is not 0. |

## Behaviour

Builds the character in slot `slot`, replacing whatever was there:

- If `dead_flag` is not 255 and that bit of the enemy flag bank is set, the
  instruction does nothing. The bank is 6 for stages 1 to 3 unless bit 6 of
  bank 1 is set, and 7 otherwise (see
  [Flags and variables](../index.md#flags-and-variables)). A script that
  kills a character sets the bit, so it stays dead when the room is entered
  again.
- Otherwise the room's enemy count goes up (not for the partner), and the
  sound type is noted for the room's enemy sound bank.
- The ids the port models are the zombies (`0x10` to `0x21`, which takes in
  the dogs and crows), the lickers (`0x22`, `0x24`), the alligator (`0x23`),
  the spiders (`0x25`), the young (`0x26`), Mr. X (`0x2A`), the plants
  (`0x2E`, `0x2F`, `0x39`), G1 (`0x30`), G2 (`0x31`), and the scene
  characters `0x40` to `0x5B`, which the original's `_Spl` runs and the
  scripts steer. Any other id empties the slot: the enemy is not there in the
  port, and [`work_set`](work_set.md) on the slot selects nothing.
- A modelled character is placed at `x, y, z` facing `dir`, in play, with no
  area yet (member 9 is 255). A character placed while the room loads that
  has a `start` begins in that routine and clip: `start`'s high byte less
  one is `Routine_1`, its low byte the clip, as the original's
  `Em_init_move` does.

The `Type` word, the floor and the model type go to the character's own
routines; for the enemies, see the enemy scripts under `enemy/` and
[Modding](../../../modding.md).

## Example

```c
proc init_00
{
    if (ck(6, 12, 0))            // not yet killed
    {
        sce_em_set(0, 0, 16, 0, 0, 0, 1, 0, 12, -14200, 0, 3600, 1024, 0, 0);
    }
    evt_end(0);
}
```

Illustrative: the room's init places a zombie (`Id` 0x10) in enemy slot 0,
with dead flag 12. The test of bank 6 bit 12 is not needed, since
`sce_em_set` checks it itself, but the room's scripts often have one around
other set-up for the same enemy.

## See also

- [`work_set`](work_set.md): `work_set(3, slot)` acts on the character.
- [`sce_em_set2`](sce_em_set2.md): a second form, not modelled.
- [`kage_set`](kage_set.md): the character's shadow or pool.
- [`plc_dest`](plc_dest.md), [`plc_motion`](plc_motion.md): move it in a scene.
