# `evt_push_cond`

Opens a message wait; the first half of `wait_message();`.

| | |
| --- | --- |
| Opcode | `0xF6` |
| Group | Event: control |
| Kind | Event instruction |
| Size | 1 byte |
| Valid in | every state |

## Syntax

```c
wait_message();
```

## Parameters

None.

## Behaviour

`wait_message();` compiles to `evt_push_cond` and
[`evt_skip_if`](evt_skip_if.md), always together. `evt_push_cond` takes one
entry of the slot's four-deep loop stack and continues in the same frame;
`evt_skip_if` does the waiting and releases the entry.

Because of that entry, a `wait_message()` inside `for`, `do` or `sleep`
counts towards the four levels of nesting the stack allows.

Written alone, `evt_push_cond()` compiles, but it leaves an entry on the
stack that nothing pops.

## Example

```c
voice_play(VOICE_PLAY, VO_00C_11);
wait_message();
evt_fork(TASK_09, rebecca_hunter_attack_hunter_slash, 0);
```

Room 101 (`games/re1/room/101/room.akscr`), event `rebecca_hunter_attack`:
waits for the voice line to end before starting the next event.

## See also

- [`evt_skip_if`](evt_skip_if.md): the wait itself.
- [`voice_play`](voice_play.md), [`message`](message.md): what it waits for.
