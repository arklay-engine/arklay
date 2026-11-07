# `scene_setup`

Sets up the room's mirror: its plane, the extent along it, and whether it is on.

| | |
| --- | --- |
| Opcode | `0x0F` |
| Group | Camera, sprites and light |
| Kind | Command |
| Size | 8 bytes |

## Syntax

```c
scene_setup(mode, extent_min, extent_max, plane);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `mode` | `u8` | Bit 0 switches the mirror on, bit 1 puts the mirror's plane at `X = plane` rather than `Z = plane`. |
| `extent_min` | `u16` | The low end of the mirror along the other axis (z for an X plane, x for a Z plane). |
| `extent_max` | `u16` | The high end of the mirror along the other axis. |
| `plane` | `u16` | The plane's coordinate. |

## Behaviour

A room mirror draws the characters a second time, reflected about a
vertical plane. `scene_setup` stores the plane and its extent, and writes
two status bits from `mode`: bit 0 sets or clears the mirror bit
(`F_STATUS_MIRROR_ENABLE`, bit `0x1F` of `FG_STATUS`), bit 1 the bit that
picks the X plane (bit `0x1E` of `FG_STATUS`). A joint shows in the mirror when the camera and the joint are on
the same side of the plane and the line between them crosses the plane
inside the extent.

Since the mirror bit is an ordinary status flag, a script can also switch
the mirror with [`set`](set.md). Of the shipped rooms, those that pass
`mode` 1 switch the mirror on with it, and those that pass 2 (an X plane,
mirror off) or 0 switch it on later with `set`.

## Example

```c
setbyte(0, 0, 0);
scene_setup(2, 4000, 6300, 12500);
set(FG_STATUS, F_STATUS_MIRROR_ENABLE, TRUE);
```

Room 111 (`games/re1/room/111/room.akscr`) puts its mirror on the plane
`X = 12500`, between `z` 4000 and 6300, in its init.

## See also

- [`set`](set.md): switches the mirror bit on its own.
