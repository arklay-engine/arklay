# `cutcurr`

Cuts back to the camera `cutnext` saved and lets the camera follow the player again.

| | |
| --- | --- |
| Opcode | `0x0A` |
| Group | Camera, sprites and light |
| Kind | Command |
| Size | 2 bytes |

## Syntax

```c
cutcurr(0);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| *pad* | `u8` | Unused; the shipped scripts write 0. |

## Behaviour

When a [`cutnext`](cutnext.md) saved a camera, `cutcurr` makes it the room
camera again (the state bytes `B_CAMERA` and `B_CUT` both read it) and
forgets the save. It then unlocks the camera, so the player's camera
switch zones move it again, and rebuilds the camera's sprite table as a
`cutnext` does (toggles queued earlier in the same pass are dropped).

With no saved camera (no `cutnext` since the last `cutcurr`), it only
unlocks the camera.

## Example

```c
run
{
    set(FG_RUN, F_RUN_CONTROLS, TRUE);
    set(FG_RUN, F_RUN_MONSTERS, TRUE);
    set(FG_RUN, F_RUN_EFFECTS, TRUE);
    cutcurr(0);
    cut_auto(CUT_FREE);
}
evt_finish();
```

Room 117 (`games/re1/room/117/room.akscr`) ends a scene this way: it gives
the player back control and returns to the camera on screen before the
scene's `cutnext`.

## See also

- [`cutnext`](cutnext.md): cuts to a camera and saves the current one.
- [`cut_auto`](cut_auto.md): locks or unlocks the camera without cutting.
