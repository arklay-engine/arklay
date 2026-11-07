# `light_color_set`

Sets the colour of one light of the camera on screen.

| | |
| --- | --- |
| Opcode | `0x7C` |
| Group | Camera, room and screen |
| Kind | Instruction |
| Size | 6 bytes |
| Status | yes |

## Syntax

```c
light_color_set(light, r, g, b, 0);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `light` | `u8` | The light, from 0. |
| `r` | `u8` | Red, 0 to 255. |
| `g` | `u8` | Green. |
| `b` | `u8` | Blue. |
| `_` | `u8` | Padding, not read. Write 0. |

## Behaviour

Writes the colour of light `light` of the light record of the camera on
screen when the instruction runs, converted as the room's own light colours
are when the room loads. The change is made after the tick's pass of the
scripts, and lasts until the room is loaded again. A light the record does
not have changes nothing.

To change another camera's light, use
[`light_color_set2`](light_color_set2.md).

## Example

```c
proc main_2E
{
    light_color_set(1, 255, 40, 20, 0);   // alarm red
    sleep(10, 20);
    light_color_set(1, 200, 200, 180, 0);
    evt_end(0);
}
```

Illustrative: light 1 of the camera on screen turns red for two thirds of a
second.

## See also

- [`light_color_set2`](light_color_set2.md): the same for a given camera.
- [`light_pos_set`](light_pos_set.md), [`light_kido_set`](light_kido_set.md): a light's position and strength.
