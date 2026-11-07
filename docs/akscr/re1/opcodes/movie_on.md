# `movie_on`

Plays a film.

| | |
| --- | --- |
| Opcode | `0x29` |
| Group | Messages, voice and film |
| Kind | Command |
| Size | 2 bytes |

## Syntax

```c
movie_on(film);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `film` | `u8` | The film, a `MOVIE_*` name from `inc/movies.akinc`. |

## Behaviour

`movie_on` queues the film and raises the film-request bit of `FG_STATUS`.
The engine plays it on the next frame, then clears the bit and goes back to
the room. The script does not wait: the instructions after it run before
the film starts, so a cutscene puts `movie_on` at the end of a `run` block
and sleeps after it.

A film id the game does not have (past 28, or the unused id 10) is counted
and ignored, so a wrong id cannot hang the room.

## Example

```c
run
{
    bgm_stop_all(0);
    movie_on(MOVIE_03_FIRST_ZOMBIE);
}
sleep(1);
```

Room 104 (`games/re1/room/104/room.akscr`): the music stops and the first
zombie's film plays.

## See also

- [`bgm_stop_all`](bgm_stop_all.md): stops the music before a film.
