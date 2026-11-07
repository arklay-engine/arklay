# `sce_shake_on`

Offsets the whole picture by a few pixels, until the next `sce_shake_on`.

| | |
| --- | --- |
| Opcode | `0x5C` |
| Group | Camera, room and screen |
| Kind | Instruction |
| Size | 3 bytes |
| Status | yes |

## Syntax

```c
sce_shake_on(x, y);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `x` | `i8` | The sideways offset, in pixels. The picture moves by `-x`. |
| `y` | `i8` | The vertical offset. The picture moves by `-y`. |

## Behaviour

As the original's `Sce_shake_on`, which calls `SetGeomOffset`: the offset
applies to the 3D projection, the masks, the sprites and the background
alike, and holds until the next `sce_shake_on`. It does not shake on its
own: a script makes a shake by changing the offset every tick, and ends it
with `sce_shake_on(0, 0)`.

## Example

```c
proc main_2B
{
    for (8)
    {
        sce_shake_on(2, -1);
        evt_next();
        sce_shake_on(-2, 1);
        evt_next();
    }
    sce_shake_on(0, 0);
    evt_end(0);
}
```

Illustrative: the screen shakes for 16 ticks and settles.

## See also

- [`sce_scr_move`](sce_scr_move.md): scrolls only the background.
- [`vib_set0`](vib_set0.md): the console's pad rumble, which the PC version does not have.
