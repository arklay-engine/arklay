# `message_long`

Shows a message by an id word, reaching the room messages past 63 (port
only).

| | |
| --- | --- |
| Opcode | `0x53` |
| Group | Messages, voice and film |
| Kind | Command |
| Size | 6 bytes |

## Syntax

```c
message_long(0, id, pause);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| *pad* | `u8` | Unused, written as 0. |
| `id` | `u16` | The message id word: the low byte as [`message`](message.md) takes it, and the high byte the index's higher bits (room message 70 is `0x106`). |
| `pause` | `u16` | The `RUN_*` systems that stop while the window is open. |

## Behaviour

`message_long` does exactly what `message` does, with the whole id word.
The original's `message` has one id byte, so a room has at most 64
messages; the port's own opcode lifts that limit. A script normally writes
`message`: the compiler picks this opcode when the id does not fit a byte.
It can also be written by name, and the original's engine does not have it,
so a script that uses it runs only in Arklay.

## Example

```c
message(0x106 | MSG_FAST, 0);
```

An illustrative snippet: room message 70, shown at once. The id does not
fit a byte, so this `message` compiles to `message_long(0, 0x186, 0)`.

## See also

- [`message`](message.md): the original's form.
- [Message ids](../index.md#message-ids).
