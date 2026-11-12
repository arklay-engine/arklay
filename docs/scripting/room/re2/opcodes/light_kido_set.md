# `light_kido_set`

Sets the strength (`kido`) of one light of the camera on screen.

| | |
| --- | --- |
| Opcode | `0x6B` |
| Group | Camera, room and screen |
| Kind | Instruction |
| Size | 4 bytes |
| Status | yes |

## Syntax

```c
light_kido_set(light, radius);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `light` | `u8` | The light, from 0. |
| `radius` | `i16` | The light's `kido` (brightness), kept as the light's radius: how far it reaches. Read as unsigned. |

## Behaviour

Writes the `kido` of light `light` of the light record of the camera on
screen when the instruction runs. The port keeps it as the light's radius,
the distance over which the light falls off. The change is made after the
tick's pass of the scripts, and lasts until the room is loaded again. A light
the record does not have changes nothing.
A room's load applies the changes its own passes made before anything moves, so a door's end and its last stopped tick see them, as the original's do.

To change another camera's light, use [`light_kido_set2`](light_kido_set2.md).

## Example

```c
proc main_2D
{
    for (6)
    {
        light_kido_set(0, 0);        // off
        sleep(10, 3);
        light_kido_set(0, 6000);     // on
        sleep(10, 5);
    }
    evt_end(0);
}
```

Illustrative: light 0 of the camera on screen flickers for a while.

## See also

- [`light_kido_set2`](light_kido_set2.md): the same for a given camera.
- [`light_pos_set`](light_pos_set.md), [`light_color_set`](light_color_set.md): a light's position and colour.
