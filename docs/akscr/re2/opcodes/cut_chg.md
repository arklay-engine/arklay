# `cut_chg`

Switches to a camera and holds it there.

| | |
| --- | --- |
| Opcode | `0x29` |
| Group | Camera, room and screen |
| Kind | Instruction |
| Size | 2 bytes |
| Status | yes |

## Syntax

```c
cut_chg(cut);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `cut` | `u8` | The camera (cut) of the room, from 0. Bit 7 is ignored: only `cut & 0x7F` is used. |

## Behaviour

Remembers the camera on screen for [`cut_old`](cut_old.md), switches to
camera `cut & 0x7F` at once (script variable 26 reads it from then on), and
holds the camera: the player's position no longer switches it until a
[`cut_old`](cut_old.md) or a [`cut_auto`](cut_auto.md) with a non-zero
operand. Only one camera is remembered, so a second `cut_chg` before the
`cut_old` replaces the camera it goes back to.

Entering a room releases the hold.

## Example

```c
proc main_24
{
    set(2, 7, 1);                // hold the player's input
    cut_chg(3);
    sleep(10, 60);
    cut_old();
    set(2, 7, 0);
    evt_end(0);
}
```

Illustrative: the scene cuts to camera 3 for two seconds and goes back to
the camera it was on, then gives the input back.

## See also

- [`cut_old`](cut_old.md): back to the remembered camera.
- [`cut_auto`](cut_auto.md): holds or releases the camera.
- [`cut_replace`](cut_replace.md), [`cut_be_set`](cut_be_set.md): change the room's camera switches.
