# `super_reset`

Gives the task's object back to the world, where it was last drawn, with a new rotation.

| | |
| --- | --- |
| Opcode | `0x49` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 8 bytes |
| Status | yes |

## Syntax

```c
super_reset(0, rx, ry, rz);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `_` | `u8` | Padding, not read. Write 0. |
| `rx`, `ry`, `rz` | `i16` | The object's new rotation, in the world. |

## Behaviour

The original handler is `Super_reset` (`0x4E5E30`). For the object chosen
with [`work_set`](work_set.md), it sets the parent pointer (`+0x80`) to the
identity matrix, copies the translation of the object's world matrix
(`+0x68..+0x70`) into its position (`+0x38..+0x40`), and sets its rotation
from the operands. That world matrix is the one
[`obj_model_set`](obj_model_set.md) composed when it built the object, and
then the one `Om_trans` composed on each frame it drew the object (the frame
before the script runs). An `Id` 2 box's is 900 higher than its position,
the lift `Om_trans` gives it.

So the object stays where it was seen, and a script can move it on from
there: room 205's falls (`main_07`, `main_09`, `main_0A`) run `add_speed`
from that position with no [`pos_set`](pos_set.md) between. Arklay keeps the
world matrix as the original does, except on a second player's or an enemy's
joint (see [`super_set`](super_set.md)): room 702's prop, hung on the second
player's joint 10, starts its fall (`main_0D`) from that character's matrix
rather than the hand, so the fall's loop and what waits on it end early.

On a work that isn't an object, nothing happens, and the log notes
`RE2 script: super_reset is not modelled` once per run.

## Example

```c
proc main_07
{
    work_set(4, 3);              // object 3, hung on object 2
    nop();
    super_reset(0, 1024, -256, 1024);
    evt_end(0);
}
```

After room 205's `main_07`: the object is in the world at the place it was
drawn, turned as its parent was.

## See also

- [`super_set`](super_set.md): hangs an object on a parent.
- [`super_on`](super_on.md): not modelled.
