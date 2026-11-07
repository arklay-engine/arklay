# `voice_play`

Plays a voice line, or stops the one playing.

| | |
| --- | --- |
| Opcode | `0x1E` |
| Group | Messages, voice and film |
| Kind | Command |
| Size | 4 bytes |

## Syntax

```c
voice_play(mode, line);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `mode` | `u8` | `VOICE_PLAY` (1) or `VOICE_STOP` (2). |
| `line` | `u16` | The voice line, a `VO_xx_xx` define at the top of the room's script. Ignored by `VOICE_STOP`. |

## Behaviour

`VOICE_PLAY` queues line `line` of the current stage's voice pack and raises
the voice-playing bit of `FG_STATUS`; the engine clears the bit when the
line ends. A `message` shown while the bit is set is the line's subtitle,
and an event's `wait_message();` waits for the line too. Stage 1's line
`0x33` on the first visit plays 3 dB quieter, as in the original.

`VOICE_STOP` drops a queued line, stops the one playing and clears the bit.
No shipped script uses it. Any other mode does nothing.

`voice_play` is written by field: the `.s` form prints the line as two
bytes.

## Example

```c
voice_play(VOICE_PLAY, VO_00C_01);
message(MSG_VO_00C_01, 0);
wait_message();
```

Room 10B (`games/re1/room/10b/room.akscr`): a radio line, its subtitle, and
a wait until both are done.

## See also

- [`message`](message.md): shows the subtitle.
- [`bgm_stop_all`](bgm_stop_all.md): stops the music and the voice.
