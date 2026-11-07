# `movie_on`

Plays one of the game's full-motion movies (not modelled).

| | |
| --- | --- |
| Opcode | `0x6F` |
| Group | Camera, room and screen |
| Kind | Instruction |
| Size | 2 bytes |
| Status | no |

## Syntax

```c
movie_on(movie);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `movie` | `u8` | The movie's record in the original's movie table (`0x532C28`). Not read by the port. |

## Behaviour

Not modelled: the instruction does nothing, and the log notes
`RE2 script: movie_on is not modelled` once per run.

The original's handler (`0x4E8C40`) calls `0x4CAEC0` with the movie and
then `0x441870(2)`. Record 5 of its table, for instance, is
`zmovie\r200.bin`, the four-second clip of the statue in room 200 that the
Unicorn Medal starts (proc 8, after `cut_chg(14)`). The port has an MPEG-1
decoder (`--fmv`) but does not play movies from scripts, so the scene goes
on without the clip. How many ticks the original spends in the movie has not
been measured.

## Example

```c
proc main_08
{
    cut_chg(14);
    movie_on(5);
    evt_end(0);
}
```

Illustrative, after room 200's scene: in the port only the camera changes.

## See also

- [`cut_chg`](cut_chg.md): the camera the scene holds around the movie.
