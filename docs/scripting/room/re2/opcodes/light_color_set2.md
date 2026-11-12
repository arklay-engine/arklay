# `light_color_set2`

Sets the colour of one light of a given camera.

| | |
| --- | --- |
| Opcode | `0x7F` |
| Group | Camera, room and screen |
| Kind | Instruction |
| Size | 6 bytes |
| Status | yes |

## Syntax

```c
light_color_set2(cut, light, r, g, b);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `cut` | `u8` | The camera whose light record is changed. |
| `light` | `u8` | The light, from 0. |
| `r` | `u8` | Red, 0 to 255. |
| `g` | `u8` | Green. |
| `b` | `u8` | Blue. |

## Behaviour

As [`light_color_set`](light_color_set.md), on camera `cut`'s light record.
The change is made after the tick's pass of the scripts and lasts until the
room is loaded again. A camera or light the room does not have changes
nothing.
A room's load applies the changes its own passes made before anything moves, so a door's end and its last stopped tick see them, as the original's do.

## Example

```c
proc init_00
{
    if (ck(5, 15, 1))
    {
        light_color_set2(3, 0, 60, 60, 120);   // camera 3's light 0, dim blue
    }
    evt_end(0);
}
```

Illustrative: the room's init gives camera 3 a dim blue light when a room
flag is set.

## See also

- [`light_color_set`](light_color_set.md): the same on the camera on screen.
- [`light_pos_set2`](light_pos_set2.md), [`light_kido_set2`](light_kido_set2.md): a given camera's light position and strength.
