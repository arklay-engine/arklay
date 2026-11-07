# `sce_fade_set`

Starts a fade of the screen to or from black or white.

| | |
| --- | --- |
| Opcode | `0x53` |
| Group | Camera, room and screen |
| Kind | Instruction |
| Size | 6 bytes |
| Status | yes |

## Syntax

```c
sce_fade_set(fade, kind, 0, add);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `fade` | `u8` | Which fade: `0` the screen's own (the one doors use), `1` to `3` the overlays drawn over it. Others do nothing. |
| `kind` | `u8` | The colour: `2` black, anything else white. |
| `_` | `u8` | Padding, not read. Write 0. |
| `add` | `i16` | The step a tick. Positive fades out to the colour, negative fades in from it. |

## Behaviour

A fade is a 16-bit accumulator: the colour is drawn over the picture with an
opacity of the accumulator divided by 128 (0 to 255), and each tick adds
`add` to it. Once the accumulator is negative it stops changing and nothing
is drawn. `sce_fade_set` starts it, as the original's `Sce_fade_set` does:

- **`add` positive**: the accumulator starts at 0, so the screen fades from
  clear towards the colour, fully covered after about `32640 / add` ticks.
  The accumulator then wraps past 32767 to negative, and the fade is gone.
  To hold a covered screen, start a fade with an `add` of 0, which does not
  step, and set its opacity with [`sce_fade_adjust`](sce_fade_adjust.md).
- **`add` negative**: the accumulator starts at `add + 32768` (its top bit
  set, read as the near-full value), so the screen starts covered and clears
  over about `32768 / -add` ticks, then the fade ends.

Fade 0 is the session's own fade, which doors also use; a script's write to
it takes effect at the end of the tick, after the scripts. Fades 1 to 3 are
separate overlays, changed at once. The original's other kinds of fade, if
any, are drawn as white.

## Example

```c
proc main_29
{
    sce_fade_set(1, 2, 0, 1024);   // to black over about 32 ticks
    sleep(10, 31);
    sce_fade_set(1, 2, 0, 0);      // stop stepping
    sce_fade_adjust(1, 32640);     // and hold it covered
    evt_end(0);
}
```

Illustrative: overlay 1 fades the screen to black in about a second and
holds it there until a later `sce_fade_adjust` with a negative value.

## See also

- [`sce_fade_adjust`](sce_fade_adjust.md): sets a fade's accumulator directly.
