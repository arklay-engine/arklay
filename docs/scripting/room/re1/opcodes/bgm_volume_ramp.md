# `bgm_volume_ramp`

Ramps a music channel's volume over a number of frames, then stops the channel.

| | |
| --- | --- |
| Opcode | `0x43` |
| Group | Sound and music |
| Kind | Command |
| Size | 4 bytes |

## Syntax

```c
bgm_volume_ramp(channel, volume, frames);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `channel` | `u8` | The music channel: `BGM_MAIN`, `BGM_SUB_1` or `BGM_SUB_2`. |
| `volume` | `i8` | The signed size of the ramp; negative fades out. |
| `frames` | `u8` | How long the ramp takes. 0 is refused. |

## Behaviour

`bgm_volume_ramp` arms the original's volume decay (`UpdateSoundDecay`).
It acts only on a channel that is enabled and has a track loaded; on any
other channel, or with `frames` 0 (which would divide by zero in the
original), it does nothing.

The ramp's rate is `volume / frames`, rounded towards zero, times 78
(`0x4E`), and it runs for `2 * frames` ticks. Each tick adds
`400 / (ticks left) * rate / 100` hundredths of a decibel to the channel's
volume, so the change speeds up towards the end. When the volume falls
below -10000 or the ticks run out, the ramp ends: the channel stops and its
volume is reset to full. A ramp that starts quiet music and raises it
therefore also ends with the channel stopped.

There is one ramp at a time: a second `bgm_volume_ramp` replaces the one
running, on whatever channel it was.

## Example

```c
if (ck(FG_ENEMY, F_ENEMY_101_HUNTER_0, TRUE) && ck(FG_ROOM, F_ROOM_C_STOP_MUSIC_ON_KILL, TRUE))
{
    set(FG_ROOM, F_ROOM_C_STOP_MUSIC_ON_KILL, FALSE);
    bgm_volume_ramp(BGM_MAIN, -96, 1);
    evt_exec(0, TASK_09, hunter_dead_stop_music);
}
```

Room 101 (`games/re1/room/101/room.akscr`) cuts the fight music off almost
at once when the Hunter dies. Room 210 fades its main channel over 30
frames with `bgm_volume_ramp(BGM_MAIN, -95, 30)`.

## See also

- [`snd_pan_vol_set`](snd_pan_vol_set.md): sets a channel's volume at once.
- [`snd_fade_set`](snd_fade_set.md): fades every channel out.
- [`bgm_stop`](bgm_stop.md): stops a channel.
