# `light_pos_set`

Moves one light of the camera on screen along one axis.

| | |
| --- | --- |
| Opcode | `0x6A` |
| Group | Camera, room and screen |
| Kind | Instruction |
| Size | 6 bytes |
| Status | yes |

## Syntax

```c
light_pos_set(0, light, axis, value);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `_` | `u8` | Padding, not read. Write 0. |
| `light` | `u8` | The light of the camera's light record, from 0. |
| `axis` | `u8` | `11` x, `12` y, `13` z (the member numbers of a position). Any other value changes nothing. |
| `value` | `i16` | The new coordinate. |

## Behaviour

Each camera of an RE 2 room has its own light record, which lights the
models while that camera is on screen. `light_pos_set` writes one coordinate
of light `light` of the record of the camera on screen when the instruction
runs (script variable 26). The change is made after the tick's pass of the
scripts, and lasts until the room is loaded again. A light the record does
not have changes nothing.

To change another camera's light, use [`light_pos_set2`](light_pos_set2.md).

## Example

```c
proc main_2C
{
    for (30)
    {
        calc(0, 0, 47, 40);
        work_copy(47, 4, 1);     // patch variable 47 into the value below
        light_pos_set(0, 1, 11, 0);
        evt_next();
    }
    evt_end(0);
}
```

Illustrative: light 1 of the camera on screen moves along x by 40 a tick
for a second, from variable 47's starting value. Offset 4 skips the opcode
and the three operand bytes before `value`.

## See also

- [`light_pos_set2`](light_pos_set2.md): the same for a given camera.
- [`light_kido_set`](light_kido_set.md), [`light_color_set`](light_color_set.md): a light's strength and colour.
