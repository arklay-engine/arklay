# `bgm_restore`

Restarts the music channels `bgm_stop_all` stopped.

| | |
| --- | --- |
| Opcode | `0x4A` |
| Group | Sound and music |
| Kind | Command |
| Size | 2 bytes |

## Syntax

```c
bgm_restore(0);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| *pad* | `u8` | Unused; the shipped scripts write 0. |

## Behaviour

`bgm_restore` moves the channel bits [`bgm_stop_all`](bgm_stop_all.md)
saved in the high byte of the live music state back down, and restarts
each channel whose bit is set again and that has a track loaded, from the
start of its track. Without an earlier `bgm_stop_all` the high byte is
empty and every channel stops.

In a room whose music state is `BGM_OFF` (no music) it does nothing, as in
the original.

## Example

```c
movie_on(MOVIE_06_HUNTER_RUN);
sleep(1);
run
{
    set(FG_STATUS, F_STATUS_LETTERBOX, TRUE);
    bgm_restore(0);
    dir_set(0, 0, 304, 0, 2276, 0, 3481);
    set(FG_RUN, F_RUN_CONTROLS, FALSE);
    cutnext(CAM_02);
}
```

Room 10A (`games/re1/room/10a/room.akscr`) brings the music back after the
Hunter's film, which it stopped with `bgm_stop_all(0)`.

## See also

- [`bgm_stop_all`](bgm_stop_all.md): stops the music and saves which channels played.
- [`bgm_play`](bgm_play.md): starts one channel.
