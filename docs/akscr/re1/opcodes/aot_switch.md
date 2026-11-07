# `aot_switch`

Shows or hides a group of the current camera's mask sprites.

| | |
| --- | --- |
| Opcode | `0x25` |
| Group | Camera, sprites and light |
| Kind | Command |
| Size | 4 bytes |

## Syntax

```c
aot_switch(0, group, state);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| *pad* | `u8` | Unused; the shipped scripts write 0. |
| `group` | `u8` | The sprite group of the current camera (`SPR_xx`, a room define). Groups count from 1. |
| `state` | `u8` | `SPR_ON` (0) shows the group; `SPR_OFF` (1), or any other non-zero value, hides it. |

## Behaviour

A camera's mask sprites are the pieces of its background drawn in front of
the models (pillars, door frames, furniture), split into numbered groups.
`aot_switch` queues a show or hide of one group, and the engine applies the
queue to the camera on screen after the scripts of the frame have run.

The change belongs to that camera's sprite table, which is rebuilt with
every group shown each time a camera goes on screen. So:

- a toggle lasts only until the camera changes, and a script that keeps a
  group hidden tests the camera (`cmpb(B_CAMERA, CMP_EQ, CAM_xx)`) in its
  `main` and toggles it again;
- a toggle queued in the same pass as a [`cutnext`](cutnext.md) or
  [`cutcurr`](cutcurr.md) is dropped with the table those replace;
- a toggle in a room's `init` is dropped, as the init runs before the
  room's first camera goes on screen. Use [`bank_bit`](bank_bit.md) for a
  group that must be hidden when a camera comes on.

Group 0 and groups past the camera's table do nothing. The name is the
original tool's; the instruction does not touch AOTs.

## Example

```c
if (ck(FG_COMMON, F_COMMON_L_PASSAGE_WINDOW_1_BROKEN, FALSE))
{
    if (cmpb(B_CAMERA, CMP_EQ, CAM_01))
    {
        aot_switch(0, SPR_01, SPR_OFF);
        aot_switch(0, SPR_02, SPR_OFF);
    }
}
else if (cmpb(B_CAMERA, CMP_EQ, CAM_01))
{
    aot_switch(0, SPR_01, SPR_ON);
    aot_switch(0, SPR_02, SPR_ON);
}
```

Room 108 (`games/re1/room/108/room.akscr`) shows the broken window's
sprites on camera 1 only once the window has been broken.

## See also

- [`bank_bit`](bank_bit.md): hides groups when the next camera comes on screen.
