# `bgm_play`

Starts one of the room's three music channels from the beginning.

| | |
| --- | --- |
| Opcode | `0x15` |
| Group | Sound and music |
| Kind | Command |
| Size | 2 bytes |

## Syntax

```c
bgm_play(channel);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `channel` | `u8` | The music channel: `BGM_MAIN` (0), `BGM_SUB_1` (1) or `BGM_SUB_2` (2). |

## Behaviour

A room loads up to three music tracks, one per channel, from its music
state (see [`room_bgm_state_set`](room_bgm_state_set.md)); some load
without starting. `bgm_play` sets the channel's enable bit in the live
music state and, when the channel has a track loaded, restarts it from its
start. A channel with no track loaded only gets its enable bit, and stays
silent. A channel past `BGM_SUB_2` has no track to play.

The change reaches the mixer after the scripts of the frame have run.

## Example

```c
if (ck(FG_ENEMY, F_ENEMY_108_CERBERUS_0, TRUE) && ck(FG_ROOM, F_ROOM_DOG_1_CHASE, TRUE))
{
    set(FG_ROOM, F_ROOM_DOG_1_CHASE, FALSE);
    if (ck(FG_ROOM, F_ROOM_DOG_2_CHASE, FALSE))
    {
        bgm_stop(BGM_SUB_2);
        bgm_play(BGM_MAIN);
    }
}
```

Room 108 (`games/re1/room/108/room.akscr`) swaps the chase music on its
second channel back for the room's main music once both dogs are dead.

## See also

- [`bgm_stop`](bgm_stop.md): stops a channel.
- [`bgm_stop_all`](bgm_stop_all.md), [`bgm_restore`](bgm_restore.md): stop every channel and bring them back.
- [`room_bgm_state_set`](room_bgm_state_set.md): picks a room's tracks.
