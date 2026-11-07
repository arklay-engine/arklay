# `timer_setup`

Starts or stops the special room light: a full-screen tint or veil that fades by a step each frame.

| | |
| --- | --- |
| Opcode | `0x1C` |
| Group | Camera, sprites and light |
| Kind | Command |
| Size | 6 bytes |

## Syntax

```c
timer_setup(mode, delta, channels);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `mode` | `u8` | How the light draws; its low two bits count. `LIGHT_FADE_VEIL` (2) draws a black veil, `LIGHT_FADE_HALF` (3) the colour at half strength, and 0 or 1 the colour at full strength. Stored in the state byte `B_LIGHT_R`. |
| `delta` | `i16` | The signed step added to the light's level each frame. 0 keeps the running fade as it is. Stored in the state word `W_LIGHT_DELTA`. |
| `channels` | `u16` | The colour channels the light saturates: `FADE_R`, `FADE_G` and `FADE_B` ORed together. 0 draws nothing. |

## Behaviour

The special room light is a rectangle drawn over the whole screen. Its
level is the state word `W_LIGHT_STATE`; each rendered frame adds
`W_LIGHT_DELTA` to it while it is not negative, and once it goes negative
the light rests and is not drawn.

A non-zero `delta` restarts the fade: at level 0 for a positive step (the
light fades up) or at `0x7FFF` for a negative one (it starts full and fades
out). A `delta` of 0 changes only the mode and channels and leaves the
level where it is.

The level's high byte is the light's strength. Each channel in `channels`
is drawn at full colour, and the strength is the alpha: a mode-2 veil is
black at that alpha, a mode-3 light is the channel colour at half of it.
Some rooms and cameras never draw the light, the courtyard fountain's
camera 3 draws it twice as strong and the guardhouse control room draws it
twice; these follow the original.

The opcode table's name is the original tool's; the original's handler is
`room_light_fade_set`.

## Example

```c
event lights_on
{
    set(FG_ROOM, F_ROOM_LIGHTS_OFF, FALSE);
    timer_setup(LIGHT_FADE_VEIL, 0, 0);
    sleep(1);
    timer_setup(LIGHT_FADE_VEIL, -1, FADE_R | FADE_G | FADE_B);
    sleep(1);
    timer_setup(LIGHT_FADE_VEIL, 0, 0);
    sleep(1);
    // ...
    run
    {
        timer_setup(LIGHT_FADE_VEIL, -10000, FADE_R | FADE_G | FADE_B);
        msg_list(0, 3680, -1650, 7160, 90, 90, 90, LIGHT_KIND_00, 6000,
                 3880, -2000, 7920, 130, 130, 130, LIGHT_KIND_00, 6639,
                 3360, -2000, 7920, 111, 111, 111, LIGHT_KIND_00, 5279,
                 1136, 1120, 1040);
    }
    evt_chain(return_control);
    evt_finish();
}
```

Room 119 (`games/re1/room/119/room.akscr`) flickers the lights back on: it
alternates frames with no light and frames with a full black veil, then
starts a veil that fades out in a few frames while it restores the room's
lights.

## See also

- [`msg_list`](msg_list.md): rewrites the room's lights.
- [`cmpw`](cmpw.md), [`setw`](setw.md): read and write `W_LIGHT_STATE` and `W_LIGHT_DELTA`.
