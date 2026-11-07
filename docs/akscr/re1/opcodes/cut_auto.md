# `cut_auto`

Lets the camera follow the player's camera zones, or holds it on the current camera.

| | |
| --- | --- |
| Opcode | `0x23` |
| Group | Camera, sprites and light |
| Kind | Command |
| Size | 2 bytes |

## Syntax

```c
cut_auto(mode);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `mode` | `u8` | `CUT_FREE` (0) lets the camera switch zones move the camera; `CUT_LOCKED` (1) holds it. Any value other than 0 locks. |

## Behaviour

`cut_auto` sets the camera lock and nothing else: it does not cut and does
not touch the camera [`cutnext`](cutnext.md) saved. While the camera is
locked, the room's switch zones do not change the camera as the player
walks; a script still can, with `cutnext` or by writing `B_CAMERA` with
[`setb`](setb.md). `cutnext` locks the camera itself and
[`cutcurr`](cutcurr.md) unlocks it, so `cut_auto(CUT_FREE)` mostly ends a
scene that cut with `cutnext` and does not cut back.

When a room loads with the camera unlocked, the room picks its camera
from the player's position; a camera locked by the room's init keeps the
camera the script chose.

## Example

```c
run
{
    aot_delete(AOT_05_EVENT_TO_WEST_STAIRS, SCE_NONE, SAT_UNDER | SAT_PLAYER);
    cut_auto(CUT_LOCKED);
    dir_set(0, 0, 0, 0, 11336, -4315, 10556);
    spd_set(ENT_PLAYER, -4315);
    aot_on(AOT_03_5_STAIRS_EAST, SCE_5, 0);
    cutnext(CAM_01_EAST);
    cut_auto(CUT_FREE);
}
```

Room 110 (`games/re1/room/110/room.akscr`) holds the camera while it moves
the player onto the stairs, cuts to the stairs' camera and then lets the
zones take over again.

## See also

- [`cutnext`](cutnext.md): cuts to a camera and locks it.
- [`cutcurr`](cutcurr.md): cuts back and unlocks.
- [`msgnode_set`](msgnode_set.md): rewrites a camera switch zone.
