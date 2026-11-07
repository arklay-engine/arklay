# `xa_on`

Plays a voice line.

| | |
| --- | --- |
| Opcode | `0x59` |
| Group | Messages, sound and music |
| Kind | Instruction |
| Size | 4 bytes |
| Status | yes |

## Syntax

```c
xa_on(mode, line);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `mode` | `u8` | `0` for one of the stage's own lines, any other value for one of the common set (stage 0's). |
| `line` | `u16` | The line's number in that set. |

## Behaviour

`xa_on` starts the voice line and sets bank 1 bit 26, which stays set while
the line plays: [`wsleeping`](wsleeping.md) waits on it, and a script can test
it with [`ck`](ck.md). The line is that of the character being played (bank 1
bit 0: Leon or Claire), so the same script plays each character's own
recording. The port finds it in the pack as
`voice/<player>/<stage>_<line>.wav`, with the stage as two digits (00 for the
common set) and the line in hexadecimal.

The bit comes down when the line ends: after its length in ticks and three
more, or after the length a `--voice-ticks` list gives, when one is used to
replay a recording of the original.
A line missing from the pack holds the bit for those three ticks only. The
line plays at the volume [`xa_vol`](xa_vol.md) last set.

`xa_on` does not wait itself; follow it with `wsleep(); wsleeping();` to wait
for the line.

## Example

```c
xa_vol(110);
xa_on(0, 0x29);
wsleep();
wsleeping();
```

Illustrative: line `0x29` of the stage, a little quieter than the default,
and the task waits until it ends.

## See also

- [`xa_vol`](xa_vol.md): the voice volume.
- [`wsleep`](wsleep.md), [`wsleeping`](wsleeping.md): wait for the line.
- [`message_on`](message_on.md): the subtitles are messages.
