# `evt_sleep_tick`

The counting byte of a `sleep`; not written on its own.

| | |
| --- | --- |
| Opcode | `0xF9` |
| Group | Event: control |
| Kind | Event instruction |
| Size | 1 byte |
| Valid in | every state |

## Syntax

```c
sleep(N);
```

## Parameters

None of its own. In a `sleep` it is followed by the `u16` count.

## Behaviour

A `sleep(N);` is the bytes `F8 F9` and the count. In the original,
[`evt_sleep`](evt_sleep.md) (`F8`) pushes the count and steps onto this
byte, and `evt_sleep_tick` counts down a frame at a time until it reaches
zero, then skips itself and the count word. The port reads the four bytes
as one [`evt_sleep`](evt_sleep.md) instruction that does both, so a
`sleep` never reaches `evt_sleep_tick` on its own.

When the port does run a lone `evt_sleep_tick` (a jump into the middle of
a sleep, or `evt_sleep_tick();` written by hand) it decrements the counter
on top of the slot's loop stack and yields. On zero it pops the entry and
skips 3 bytes: itself and the two after it. With an empty stack it just
moves on, without yielding. Do not write it; use `sleep(N);`.

## Example

```c
sleep(31);
```

Room 100 (`games/re1/room/100/room.akscr`), event `rebecca_question_end`:
compiles to `F8 F9 1F 00`, the `F9` being this byte.

## See also

- [`evt_sleep`](evt_sleep.md): the instruction it belongs to.
