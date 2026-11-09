# `super_set`

Hangs the task's object on a parent: the world, the player, a character, one of their joints, or another object.

| | |
| --- | --- |
| Opcode | `0x48` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 16 bytes |
| Status | partial |

## Syntax

```c
super_set(0, mode, joint, x, y, z, rx, ry, rz);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `_` | `u8` | Padding, not read. Write 0. |
| `mode` | `u8` | The parent, by its top two bits: `0x00` the world (the identity matrix); `0x40` the player; `0x80 \| n` work `n` (0 the player, 1 the second player, 2 + `n` enemy slot `n`); `0xC0 \| n` object `n`'s world matrix. |
| `joint` | `u8` | With bit 7 set, joint `joint & 0x7F` of the player (mode `0x40`) or of work `n` (mode `0x80`) instead of its own matrix. Not read for the world or an object. |
| `x`, `y`, `z` | `i16` | The object's position, now an offset on the parent's matrix. Negative y is up. |
| `rx`, `ry`, `rz` | `i16` | The object's rotation, on the parent's. |

## Behaviour

The original handler is `Super_set` (`0x4E59E0`). For the object chosen with
[`work_set`](work_set.md), it writes the parent pointer (`+0x80`, the one
[`obj_model_set`](obj_model_set.md)'s parent selector also writes), the
position (`+0x38`) and the rotation (`+0x74`). It doesn't compose anything:
the next frame's `Om_trans` draws the object on the parent's matrix, and
keeps that world matrix (`+0x54`) on the object.

Arklay does the same, and composes the world matrix each tick at the point
`Om_trans` runs, for an object in play (`Be_flg` bit 0, without `0x80000`),
in slot order. For the world, the player, the player's joint and another
object the matrix is the original's. For the second player's or an enemy's
work, joint or not, arklay uses the character's own matrix (its position and
angle), because it doesn't keep their joints between frames. An object hung
on a joint or a character's matrix isn't drawn yet.

On a work that isn't an object, nothing happens, and the log notes
`RE2 script: super_set is not modelled` once per run.

## Example

```c
proc main_0C
{
    work_set(4, 0);              // object 0
    nop();
    // In the second player's hand (work 1, joint 10), 600 below it.
    super_set(0, 129, 138, 0, 600, -50, -896, 0, 1024);
    evt_end(0);
}
```

From room 702's `main_0C`.

## See also

- [`super_reset`](super_reset.md): gives the object back to the world where it was drawn.
- [`super_on`](super_on.md): not modelled.
- [`obj_model_set`](obj_model_set.md): sets the same parent when it builds the object.
