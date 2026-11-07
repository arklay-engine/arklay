# `bgm_stop_all`

Stops all music and the voice line, saving which channels were playing.

| | |
| --- | --- |
| Opcode | `0x4B` |
| Group | Sound and music |
| Kind | Command |
| Size | 2 bytes |

## Syntax

```c
bgm_stop_all(0);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| *pad* | `u8` | Unused; the shipped scripts write 0. |

## Behaviour

`bgm_stop_all` moves the live music state's channel bits into its high
byte, which clears them, so all three channels stop and their tracks stay
loaded. It also stops the voice line playing. [`bgm_restore`](bgm_restore.md)
brings the saved bits back.

In a room whose music state is `BGM_OFF` (no music) it does nothing, as in
the original.

## Example

```c
event hunter_film
{
    run
    {
        aot_delete(AOT_05_EVENT_HUNTER_FILM, SCE_NONE, 0);
        // ...
        bgm_stop_all(0);
    }
    sleep(1);
    movie_on(MOVIE_06_HUNTER_RUN);
    sleep(1);
    run
    {
        set(FG_STATUS, F_STATUS_LETTERBOX, TRUE);
        bgm_restore(0);
        // ...
    }
    // ...
}
```

Room 10A (`games/re1/room/10a/room.akscr`) stops the music for a film and
restarts it afterwards.

## See also

- [`bgm_restore`](bgm_restore.md): restarts the channels this stopped.
- [`bgm_stop`](bgm_stop.md): stops one channel.
