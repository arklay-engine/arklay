# `plc_dest`

Walks the task's character or the player to a point, raising a room flag on arrival.

| | |
| --- | --- |
| Opcode | `0x40` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 8 bytes |
| Status | yes |

## Syntax

```c
plc_dest(0, routine, flag, x, z);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `_` | `u8` | Padding, not read. Write 0. |
| `routine` | `u8` | The scenario routine that makes the move: `4` or `18` walk, `5` run, `7` walk backwards, `8` run backwards, `9` turn on the spot. Others are routines with no turn rate of their own. |
| `flag` | `u8` | The bit of bank 5 (`fg_room`) raised on arrival. |
| `x` | `i16` | The destination's x. |
| `z` | `i16` | The destination's z. |

## Behaviour

On the player or a character (kinds 1 to 3 of [`work_set`](work_set.md)), as
the original's `Plc_dest`:

- If `Sce_flg` bit 2 (value 4) is clear or the work is in another routine,
  the routine restarts as `4, routine, 0, 0` (the scenario mode).
- For routines 4, 5, 7, 8, 9 and 18, `Sce_free0` becomes the turn rate the
  original's tables give that routine and character.
- `Sce_flg` and `Sce_free1` become 0, `Dest` becomes `x, z`, and the flag is
  kept for the arrival.

The routine then turns the work towards `Dest` and moves it there. On
arrival it sets bit `flag` of bank 5, and the work stands (routine 6) unless
`Sce_flg` bit 2 has been set in the meantime, in which case its last state
keeps running. The flag is raised even when it is 0.

The usual way to wait is a `do` loop on the flag. Bits 32 to 63 of bank 5
are cleared at the end of every pass of the scripts, so an arrival flag
there is seen for one pass only, which suits a wait and needs no clearing.
Bits 0 to 31 stay set until a script clears them or the room is entered
again.

On a work the engine does not model (an enemy that is not modelled, a room
object or none) the work arrives at once: the flag, if it is not 0, is
raised after this tick's pass of the scripts, for the next pass to see.
Nothing is logged.

## Example

```c
proc main_12
{
    work_set(3, 0);
    plc_dest(0, 4, 40, -9000, 1500);
    do
    {
        evt_next();
    }
    while (ck(5, 40, 0));        // until bit 40 of bank 5 is set
    plc_neck(5, 1, 0, 0, 16, 16);  // then look at the player
    evt_end(0);
}
```

Illustrative: the character in enemy slot 0 walks to `-9000, 1500` and, once
it is there, turns its head to the player. Bit 40 is in the second word of
bank 5, so it lasts the one pass that ends the loop.

## See also

- [`plc_motion`](plc_motion.md): a scripted motion in place.
- [`plc_rot`](plc_rot.md): changes `Sce_free0`, the walk's turn rate.
- [`plc_ret`](plc_ret.md): hands the work back.
- [`ck`](ck.md): tests the arrival flag.
