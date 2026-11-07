# `sce_bgm_control`

Starts, stops, fades or changes a music sequence and its channels.

| | |
| --- | --- |
| Opcode | `0x51` |
| Group | Messages, sound and music |
| Kind | Instruction |
| Size | 6 bytes |
| Status | yes |

## Syntax

```c
sce_bgm_control(sequence, op, channel, volume, pan);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `sequence` | `u8` | `0` the main music, `1` or `2` a sub sequence (ambience and the like). A larger value does nothing. |
| `op` | `u8` | `1` play, `2` stop, `3` play (only the main loops), `4` mute and stop, `5` fade out. Any other value changes only the volume and pan. |
| `channel` | `u8` | `0` all of the sequence's channels, or `n` its channel `n`, for the volume and pan. |
| `volume` | `u8` | `0` leaves the volume; otherwise the channels' volume plus one. |
| `pan` | `u8` | `0` leaves the pan; otherwise the channels' pan plus one. |

## Behaviour

`sce_bgm_control` is the original's `Snd_bgm_ctr`. The music a room plays
comes from the room music table (see [`sce_bgmtbl_set`](sce_bgmtbl_set.md)),
loaded as the player enters the room. This instruction then controls what is
loaded:

- **Play** (1) starts the sequence with its own volume. It loops, except for
  the sequences a few rooms play once, as in the original.
- **Stop** (2) stops a sequence that is playing.
- **Play** (3) starts it again; only the main music loops.
- **Mute** (4) sets the volume to 0 and stops it.
- **Fade** (5) fades it out.

Then, whatever `op` was, a non-zero `volume` or `pan` sets the volume or pan
of `channel` (or all the sequence's channels) to that value less one. The
main music has three channels and each sub sequence its own.

A sequence with nothing loaded ignores the instruction. So does all music
while `fg_system`'s no-music bit is set. During a door's wait the instruction
is kept, and applied when the wait ends.

## Example

```c
sce_bgm_control(0, 5, 0, 0, 0);  // fade the main music out
sleep(10, 60);
sce_bgm_control(1, 1, 0, 0x60, 0); // start the sub sequence at volume 0x5F
```

Illustrative: a scene that fades the music and brings in the room's second
sequence.

## See also

- [`sce_bgmtbl_set`](sce_bgmtbl_set.md): the music a room loads.
- [`se_on`](se_on.md): a sound effect.
