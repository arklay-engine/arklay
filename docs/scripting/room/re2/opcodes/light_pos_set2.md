# `light_pos_set2`

Moves one light of a given camera along one axis.

| | |
| --- | --- |
| Opcode | `0x7D` |
| Group | Camera, room and screen |
| Kind | Instruction |
| Size | 6 bytes |
| Status | yes |

## Syntax

```c
light_pos_set2(cut, light, axis, value);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `cut` | `u8` | The camera whose light record is changed. |
| `light` | `u8` | The light, from 0. |
| `axis` | `u8` | `11` x, `12` y, `13` z. Any other value changes nothing. |
| `value` | `i16` | The new coordinate. |

## Behaviour

As [`light_pos_set`](light_pos_set.md), on camera `cut`'s light record
rather than the one on screen. The change is made after the tick's pass of
the scripts and lasts until the room is loaded again, so a room's init can
set up a camera that is not on screen yet. A camera or light the room does
not have changes nothing.

## Example

```c
proc init_00
{
    if (ck(5, 13, 1))            // the lamp has fallen
    {
        light_pos_set2(4, 0, 12, -200);
    }
    evt_end(0);
}
```

Illustrative: the room's init lowers light 0 of camera 4 when a room flag
is set.

## See also

- [`light_pos_set`](light_pos_set.md): the same on the camera on screen.
- [`light_kido_set2`](light_kido_set2.md), [`light_color_set2`](light_color_set2.md): a given camera's light strength and colour.
