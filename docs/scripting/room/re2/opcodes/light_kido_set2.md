# `light_kido_set2`

Sets the strength (`kido`) of one light of a given camera.

| | |
| --- | --- |
| Opcode | `0x7E` |
| Group | Camera, room and screen |
| Kind | Instruction |
| Size | 6 bytes |
| Status | yes |

## Syntax

```c
light_kido_set2(0, cut, light, radius);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `_` | `u8` | Padding, not read. Write 0. |
| `cut` | `u8` | The camera whose light record is changed. |
| `light` | `u8` | The light, from 0. |
| `radius` | `u16` | The light's `kido`, kept as its radius. |

## Behaviour

As [`light_kido_set`](light_kido_set.md), on camera `cut`'s light record.
Unlike the other `*2` light instructions, the camera is the second operand,
after a padding byte. The change is made after the tick's pass of the
scripts and lasts until the room is loaded again. A camera or light the room
does not have changes nothing.

## Example

```c
proc init_00
{
    if (ck(5, 14, 1))            // the power is off
    {
        light_kido_set2(0, 1, 0, 0);
        light_kido_set2(0, 2, 0, 0);
    }
    evt_end(0);
}
```

Illustrative: the room's init turns down light 0 of cameras 1 and 2 when a
room flag is set.

## See also

- [`light_kido_set`](light_kido_set.md): the same on the camera on screen.
- [`light_pos_set2`](light_pos_set2.md), [`light_color_set2`](light_color_set2.md): a given camera's light position and colour.
