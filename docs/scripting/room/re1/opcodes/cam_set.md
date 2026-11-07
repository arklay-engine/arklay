# `cam_set`

Moves one of the room's cameras: its position, the point it looks at and its field of view.

| | |
| --- | --- |
| Opcode | `0x40` |
| Group | Camera, sprites and light |
| Kind | Command |
| Size | 16 bytes |

## Syntax

```c
cam_set(camera, x, y, z, look_x, look_y, look_z, fov);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `camera` | `u8` | The camera to change (`CAM_xx`, a room define). |
| `x` | `i16` | The camera's position, x. |
| `y` | `i16` | The camera's position, y (negative is up). |
| `z` | `i16` | The camera's position, z. |
| `look_x` | `i16` | The point the camera looks at, x. |
| `look_y` | `i16` | The point the camera looks at, y. |
| `look_z` | `i16` | The point the camera looks at, z. |
| `fov` | `i16` | The field of view, in the RDT camera's units. |

## Behaviour

`cam_set` overwrites the camera record's position, look-at point and field
of view. The camera keeps its background image, its roll and its sprite
table, so the new view should match the background it is drawn over.

The rewrite is queued and applied once the scripts of the frame have run.
It does not cut: follow it with [`cutnext`](cutnext.md) to show the camera,
or let the zones reach it. It lasts while the room stays loaded; the room's
cameras are read again from the RDT when it loads. A camera number past the
room's cameras is ignored.

The original's handler (`cmd_light_param_set`) is misnamed for what it does.

## Example

```c
if (ck(FG_COMMON, F_COMMON_CEILING_TRAP_SPRUNG, TRUE))
{
    set(FG_COMMON, F_COMMON_CEILING_TRAP_SPRUNG, FALSE);
    setb(B_ROOM, 19, 0);
    cam_set(CAM_00, 10818, -1116, 23508, 15570, -2304, 21852, 289);
    cutnext(CAM_00);
    effect(FX_0B, FX_ROW_0A, FX_AT_WORLD, 20152, -2600, 16284, 2624);
    dir_set(0, 0, 576, 0, 14933, 0, 22784);
}
```

Room 303 (`games/re1/room/303/room.akscr`) is the only user. In its init,
for each of the scenes that bring the player to it, it moves a camera
(the first two rewrites copy the cameras of `ROOM3110.RDT`), cuts to it,
starts an effect and places the player.

## See also

- [`cutnext`](cutnext.md): cuts to a camera.
- [`msg_list`](msg_list.md): rewrites the room's lights.
