# `snd_pan_vol_set`

Sets a music channel's volume from a pair of level bytes.

| | |
| --- | --- |
| Opcode | `0x2F` |
| Group | Sound and music |
| Kind | Command |
| Size | 4 bytes |

## Syntax

```c
snd_pan_vol_set(channel, pan, volume);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `channel` | `u8` | The music channel: `BGM_MAIN`, `BGM_SUB_1` or `BGM_SUB_2`. |
| `pan` | `i8` | The first level of the pair (the original's left level). |
| `volume` | `i8` | The second level of the pair (the original's right level). |

## Behaviour

The two bytes are the original's pan pair, and the channel's volume comes
from their average, through the original's `CalcPanVolume`: below 32 the
volume is `average * 259 - 10000` hundredths of a decibel, from 32 up
`average * 18 - 2276`. The result is held between -9999 (silent) and -1
(full); `(40, 40)` gives -1556, about -15.6 dB.

The pair is stored on the channel whether or not the channel has a track,
but the volume is set only on a channel with a track loaded. The port uses
the pair for the volume alone. A negative level (written as -1 or 255) is
sign-extended, as the original's `short` cast does, and so pulls the
average down. The volume holds until a [`bgm_stop`](bgm_stop.md) resets
it, or a [`bgm_volume_ramp`](bgm_volume_ramp.md) or
[`snd_fade_set`](snd_fade_set.md) moves it.

## Example

```c
if (cmpb(B_PREV_ROOM, CMP_EQ, ROOM_103_F_PASSAGE))
{
    snd_pan_vol_set(BGM_MAIN, 40, 40);
    snd_pan_vol_set(BGM_SUB_1, 40, 40);
}
if (cmpb(B_PREV_ROOM, CMP_EQ, ROOM_105_DINING_ROOM))
{
    snd_pan_vol_set(BGM_MAIN, 30, 30);
    snd_pan_vol_set(BGM_SUB_1, 30, 30);
}
```

Room 104 (`games/re1/room/104/room.akscr`) sets the volume of the piano
music it hears from the next room by the room the player came from.

## See also

- [`bgm_volume_ramp`](bgm_volume_ramp.md): ramps a channel's volume over time.
- [`bgm_play`](bgm_play.md): starts a channel.
