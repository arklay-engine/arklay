# `cutnext`

Cuts to a camera, saving the current one, and holds the camera there.

| | |
| --- | --- |
| Opcode | `0x09` |
| Group | Camera, sprites and light |
| Kind | Command |
| Size | 2 bytes |

## Syntax

```c
cutnext(camera);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `camera` | `u8` | The camera to cut to: one of the room's cameras, named `CAM_xx` by the room's own defines. |

## Behaviour

`cutnext` does four things at once:

- It saves the camera on screen, for [`cutcurr`](cutcurr.md), and writes
  its number to the state byte `B_CUT`.
- It makes `camera` the room camera, so the state byte `B_CAMERA` reads it
  from then on.
- It locks the camera: the player's camera switch zones no longer move it,
  as [`cut_auto`](cut_auto.md)`(CUT_LOCKED)` would. The lock stays until a
  `cutcurr` or a `cut_auto(CUT_FREE)`.
- It rebuilds the camera's sprite table with every sprite group shown. Any
  [`aot_switch`](aot_switch.md) toggles queued earlier in the same script
  pass are dropped with the old table, and a pending
  [`bank_bit`](bank_bit.md) mask is applied to the new one.

There is one saved camera, not a stack: a second `cutnext` overwrites the
first one's save, so a later `cutcurr` returns to the camera that was on
screen at the second `cutnext`.

A room whose script locks the camera during its init keeps the camera the
script chose, rather than the one the player's position would pick.

## Example

```c
event examine_kenneth
{
    run
    {
        cutnext(CAM_05_KENNETH_CLOSEUP);
        set(FG_RUN, F_RUN_CONTROLS, FALSE);
        set(FG_RUN, F_RUN_MONSTERS, FALSE);
        message(MSG_HE_IS_KENNETH_FROM_THE_STARS | MSG_FAST, 0);
    }
    // ...
    run
    {
        cutnext(CAM_04_KENNETH);
        aot_delete(AOT_04_EVENT_KENNETH, SCE_NONE, SAT_PLAYER);
        set(FG_RUN, F_RUN_CONTROLS, TRUE);
        set(FG_RUN, F_RUN_MONSTERS, TRUE);
        cut_auto(CUT_FREE);
    }
    // ...
}
```

Room 104 (`games/re1/room/104/room.akscr`) cuts to a close-up of Kenneth
while his message is open, cuts back to the room's camera on him, and then
gives the camera back to the switch zones.

## See also

- [`cutcurr`](cutcurr.md): cuts back to the saved camera and unlocks it.
- [`cut_auto`](cut_auto.md): locks or unlocks the camera without cutting.
- [`cam_set`](cam_set.md): moves a camera.
- [`cmpb`](cmpb.md): `cmpb(B_CAMERA, CMP_EQ, CAM_xx)` tests the camera on screen.
