# `setw`

Writes one state word.

| | |
| --- | --- |
| Opcode | `0x31` |
| Group | Flags and state |
| Kind | Command |
| Size | 4 bytes |

## Syntax

```c
setw(word, value);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `word` | `u8` | The state word: a `W_*` name (`W_LIGHT_STATE`, `W_LIGHT_DELTA`, `W_RANDOM`, `W_DPAD_HELD`, `W_DPAD_PRESSED`) or a placeholder such as `W_04`. See [Engine names](../index.md#engine-names). |
| `value` | `i16` | The value, -32768 to 65535 (it is stored as 16 bits). |

## Behaviour

`setw` stores the value in one of the 16 state words, the words
[`cmpw`](cmpw.md) reads. It is written by field: the `.s` form prints the
word as bytes. An index of 16 or more writes the two state bytes at
`0x14 + 2 * word` and the byte after it, with the same aliasing into the
flag groups as [`setb`](setb.md).

Word 4 (`W_04`) is the seconds counter of the game's clock, which the
self-destruct countdown displays; the engine counts it up once a second, and
writing `0x7FFF` restarts it from 0.

## Example

```c
if (ck(FG_COMMON, F_COMMON_C_REBECCA_IN_SAVE_ROOM, TRUE))
{
    setw(W_04, 0);
    evt_exec(0, TASK_09, rebecca_radio_call);
}
```

Room 10A (`games/re1/room/10a/room.akscr`): the clock is reset as Rebecca's
radio call starts.

## See also

- [`cmpw`](cmpw.md): compares a state word.
- [`setb`](setb.md): writes a state byte.
