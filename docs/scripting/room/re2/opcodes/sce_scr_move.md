# `sce_scr_move`

Scrolls the camera's background picture vertically.

| | |
| --- | --- |
| Opcode | `0x6D` |
| Group | Camera, room and screen |
| Kind | Instruction |
| Size | 4 bytes |
| Status | yes |

## Syntax

```c
sce_scr_move(0, y);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `_` | `u8` | Padding, not read. Write 0. |
| `y` | `i16` | How far the background is moved down, in pixels. `0` puts it back. |

## Behaviour

Sets the scroll, which holds until the next `sce_scr_move` or until a room
is entered (which sets it to 0). While it is not 0, the background is drawn
`y` pixel rows lower. The rows that open at the top come from the bottom of
the camera's second picture, the one above it, when the camera has one, and
are black otherwise. A negative `y` moves the picture up, and the rows that
open at the bottom are black.

Only the background moves: the 3D models, masks and sprites do not. To
scroll smoothly, change it a little each tick.

## Example

```c
proc main_28
{
    save(46, 0);
    for (60)
    {
        calc(0, 0, 46, 4);
        work_copy(46, 2, 1);     // write variable 46 into the next instruction's y
        sce_scr_move(0, 0);
        evt_next();
    }
    evt_end(0);
}
```

Illustrative: the background scrolls down 4 pixels a tick for 60 ticks.
[`work_copy`](work_copy.md) patches the variable into the `sce_scr_move`
that follows it: offset 2, counted from the end of the `work_copy`, skips
that instruction's opcode and padding byte to its `y`.

## See also

- [`sce_shake_on`](sce_shake_on.md): shakes the whole picture.
- [`work_copy`](work_copy.md): writes a variable into a later instruction.
