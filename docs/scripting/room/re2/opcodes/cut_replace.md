# `cut_replace`

Swaps two camera numbers throughout the room's camera switches.

| | |
| --- | --- |
| Opcode | `0x4B` |
| Group | Camera, room and screen |
| Kind | Instruction |
| Size | 3 bytes |
| Status | yes |

## Syntax

```c
cut_replace(a, b);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `a` | `u8` | A camera. |
| `b` | `u8` | The camera it trades places with. |

## Behaviour

As the original's `Cut_replace`: in every record of the room's camera-switch
table, a camera `a` becomes `b` and a camera `b` becomes `a`, both in the
camera the record is listed under and in the camera it switches to. The
zones' gates (see [`cut_be_set`](cut_be_set.md)) do not move. The effect is
that wherever the room would have shown `a` it now shows `b`, and the other
way round. Doing it twice undoes it.

The change is made after the tick's pass of the scripts, and lasts until the
room is loaded again. A room's load applies what its own passes changed before anything moves, so a door's end and its last stopped tick see it, as the original's do (the original writes the room as the script runs).

The original's `Cut_replace` (`0x4E5090`) ends by switching the camera: when the zone the player stands in is listed under camera `a`, it calls `Cut_change(b)`. arklay doesn't yet, so the camera on screen changes only when the player next crosses a zone (`docs/internals/deviations/re2.md`).

## Example

```c
proc init_00
{
    if (ck(5, 11, 1))            // the room has changed
    {
        cut_replace(2, 4);       // show camera 4's picture in camera 2's place
    }
    evt_end(0);
}
```

Illustrative: a room with a second version of one camera's background
switches to it once a room flag is set.

## See also

- [`cut_be_set`](cut_be_set.md): turns one switch zone on or off.
