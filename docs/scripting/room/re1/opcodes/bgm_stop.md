# `bgm_stop`

Stops one of the room's music channels.

| | |
| --- | --- |
| Opcode | `0x16` |
| Group | Sound and music |
| Kind | Command |
| Size | 2 bytes |

## Syntax

```c
bgm_stop(channel);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `channel` | `u8` | The music channel: `BGM_MAIN` (0), `BGM_SUB_1` (1) or `BGM_SUB_2` (2). |

## Behaviour

`bgm_stop` acts only on a channel whose enable bit is set in the live
music state, that is one that is playing or was started with
[`bgm_play`](bgm_play.md). It clears the bit, stops the channel and resets
its volume to full (-1 millibel), so a later `bgm_play` starts it at full
volume even after [`bgm_volume_ramp`](bgm_volume_ramp.md) or
[`snd_pan_vol_set`](snd_pan_vol_set.md) lowered it. The track stays loaded.
On a channel that is not enabled it does nothing, as in the original.

## Example

```c
if (ck(FG_ROOM, F_ROOM_DOG_2_CHASE, FALSE))
{
    bgm_stop(BGM_SUB_2);
    bgm_play(BGM_MAIN);
}
```

Room 108 (`games/re1/room/108/room.akscr`) stops the chase music on its
second sub channel when the last dog dies, and goes back to the room's
main music.

## See also

- [`bgm_play`](bgm_play.md): starts a channel.
- [`bgm_stop_all`](bgm_stop_all.md): stops every channel and saves which played.
