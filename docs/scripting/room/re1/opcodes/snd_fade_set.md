# `snd_fade_set`

Fades the music out and then stops it, the voice line and every music channel.

| | |
| --- | --- |
| Opcode | `0x27` |
| Group | Sound and music |
| Kind | Command |
| Size | 2 bytes |

## Syntax

```c
snd_fade_set(steps);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `steps` | `u8` | Read as a signed byte: the volume change per frame, in units of 78 (`0x4E`) hundredths of a decibel. A fade out is negative, written as 256 minus the step (`251` for -5); the compiler takes no negative number here. |

## Behaviour

`snd_fade_set` arms the original's scripted sound fade (`BuildSndFadeTbl`).
For each music channel with a track loaded it works out how many frames
of `steps * 78` it takes to bring the channel from its volume to silence
(-10000); a channel with no track, or a step that does not move towards
silence (0 or positive), gets none. Then, once a frame:

1. Each channel with frames left moves by the step, and counts one down.
2. When no channel has frames left (at once for a step of 0 or more), a
   countdown of 30 frames starts. On its first frame every music channel
   stops; 25 frames later the voice line stops; near the end the channels'
   tracks are unloaded, so a [`bgm_play`](bgm_play.md) no longer brings
   them back.

So a negative step is a fade to silence followed by the stop, and a
positive one stops the music after the countdown without fading. A room
that wants music afterwards loads it again, through its music state on the
next room load.

The original also fades the voice, sound effect and enemy banks; the port
fades the three music channels only. A step of 0 would divide by zero in
the original; the port gives every channel no frames instead.

## Example

```c
if (ck(FG_COMMON, F_COMMON_J_PIT_SCENE_STARTED, TRUE))
{
    set(FG_COMMON, F_COMMON_J_PIT_SCENE_STARTED, FALSE);
    snd_fade_set(251);
}
```

Room 303 (`games/re1/room/303/room.akscr`) fades the music out by 5 steps
a frame in its init when the pit scene has begun.

## See also

- [`bgm_volume_ramp`](bgm_volume_ramp.md): ramps one channel's volume.
- [`bgm_stop_all`](bgm_stop_all.md): stops every channel at once.
