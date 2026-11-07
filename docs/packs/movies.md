# Movies

`movie/` holds the films: intros, cut scenes, endings, credits. Each film
is named by its number in two hex digits, and the room scripts start a film
by that number:

```text
movie/00.mp4
movie/01.avi
movie/17.mpg
```

Formats, highest first: `mp4`, `avi`, `mpg`.

| Format | Video | Sound |
|---|---|---|
| `.mp4` | H.264, any profile (High included) | AAC-LC, optional |
| `.avi` | H.264 or Cinepak | AAC-LC or PCM, optional |
| `.mpg` | MPEG-1 program stream | MP2 |

A film may be any size up to 4096x4096 and any frame rate. It plays at its
own rate, and its sound is resampled to the mixer's 22050 Hz. The GPU
renderer scales the film's frames to the window, letterboxed to their
aspect; the software renderer scales them to 320x240. Colour follows the
stream's own colour matrix (BT.709 or BT.601, limited or full range), and
BT.601 limited range when the stream says nothing.

A film that does not open logs a warning, and the film it replaces (in a
lower layer or a lower format) plays instead. A damaged frame later on holds the previous
picture.

To watch a film on its own:

```sh
arklay re1.akpak --fmv 0
```

H.264 and AAC need the default `hd` build feature. A build without it plays
only Cinepak and MPEG-1.
