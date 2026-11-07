# `msgnode_set`

Rewrites the cameras of one of the room's camera switch zones.

| | |
| --- | --- |
| Opcode | `0x3A` |
| Group | Camera, sprites and light |
| Kind | Command |
| Size | 4 bytes |

## Syntax

```c
msgnode_set(zone, from, to);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `zone` | `u8` | The switch zone, by its index in the room's zone list (`ZONE_xx`, a room define). |
| `from` | `u8` | The camera the zone belongs to (`CAM_xx`): the zone only acts while this camera is on screen. |
| `to` | `u8` | The camera the zone switches to when the player steps into it (`CAM_xx`). |

## Behaviour

A camera switch zone is a floor area listed under one camera: while that
camera is on screen and the camera is not locked, the player walking into
the zone switches to its target camera. `msgnode_set` replaces the zone's
two cameras and keeps its area.

The rewrite is queued and applied once the scripts of the frame have run,
before the player moves and the camera is picked, so it takes effect the
same frame. It changes the loaded room only: the room's data is read again
from the RDT when the room loads, so a room that needs the change on every
visit makes it in its init, under the flag that calls for it. A zone index
past the room's zone list is ignored.

The name is the original tool's; the instruction has nothing to do with
messages.

## Example

```c
if (ck(FG_SCENARIO, F_SCENARIO_FOUNTAIN_OPEN, TRUE))
{
    msgnode_set(ZONE_04, CAM_01, CAM_02);
    msgnode_set(ZONE_09, CAM_02, CAM_04);
    msgnode_set(ZONE_0C, CAM_03, CAM_02);
    msgnode_set(ZONE_0D, CAM_03, CAM_04);
    msgnode_set(ZONE_0F, CAM_04, CAM_02);
}
```

Room 305 (`games/re1/room/305/room.akscr`) rewires its zones in its init
once the fountain has been drained, so the new path through the room gets
its own camera changes.

## See also

- [`cut_auto`](cut_auto.md): stops the zones from moving the camera.
- [`cam_set`](cam_set.md): moves a camera.
