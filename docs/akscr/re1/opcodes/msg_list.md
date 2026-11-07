# `msg_list`

Rewrites the room's three lights and its ambient colour.

| | |
| --- | --- |
| Opcode | `0x46` |
| Group | Camera, sprites and light |
| Kind | Command |
| Size | 44 bytes |

## Syntax

```c
msg_list(0,
         x1, y1, z1, r1, g1, b1, kind1, radius1,
         x2, y2, z2, r2, g2, b2, kind2, radius2,
         x3, y3, z3, r3, g3, b3, kind3, radius3,
         ambient_r, ambient_g, ambient_b);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| *pad* | `u8` | Unused; the shipped scripts write 0. |
| `x1` | `i16` | Light 1's position, x. |
| `y1` | `i16` | Light 1's position, y. |
| `z1` | `i16` | Light 1's position, z. |
| `r1` | `u8` | Light 1's colour, red. |
| `g1` | `u8` | Light 1's colour, green. |
| `b1` | `u8` | Light 1's colour, blue. |
| `kind1` | `u8` | Light 1's kind (`LIGHT_KIND_xx`; the shipped scripts write `LIGHT_KIND_00`). |
| `radius1` | `i16` | Light 1's radius. |
| `x2` .. `radius2` | as light 1 | Light 2, in the same eight fields. |
| `x3` .. `radius3` | as light 1 | Light 3, in the same eight fields. |
| `ambient_r` | `i16` | The ambient colour, red. |
| `ambient_g` | `i16` | The ambient colour, green. |
| `ambient_b` | `i16` | The ambient colour, blue. |

## Behaviour

`msg_list` replaces every light of the room at once: the three point
lights' positions, colours, kinds and radii, and the ambient colour. The
models are lit with the new values from then on. The rewrite is queued and
applied once the scripts of the frame have run, and it lasts while the
room stays loaded; the room's lights are read again from the RDT when the
room loads, so a room whose lights depend on a flag sets them again in its
init.

The original also repaints the background's clear colour from the ambient;
the port does not.

The name is the original tool's; the instruction has nothing to do with
messages.

## Example

```c
event lights_off
{
    run
    {
        set(FG_ROOM, F_ROOM_LIGHTS_OFF, TRUE);
        msg_list(0, 4640, -1650, 9280, 70, 70, 70, LIGHT_KIND_00, 9200,
                 6260, -2000, 9369, 70, 70, 70, LIGHT_KIND_00, 9200,
                 3120, -2000, 9200, 70, 70, 70, LIGHT_KIND_00, 9200,
                 800, 800, 800);
    }
    evt_chain(return_control);
    evt_finish();
}
```

Room 119 (`games/re1/room/119/room.akscr`, wrapped here) dims its three
lights to grey when the lights go out.

## See also

- [`timer_setup`](timer_setup.md): the full-screen light fade.
- [`cam_set`](cam_set.md): rewrites a camera.
