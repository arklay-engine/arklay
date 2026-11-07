# `evt_for`

Starts a loop that runs its body a fixed number of times; written as `for (N) { ... }`.

| | |
| --- | --- |
| Opcode | `0xFA` |
| Group | Event: control |
| Kind | Event instruction |
| Size | 4 bytes |
| Valid in | every state |

## Syntax

```c
for (N)
{
    event statements...
}
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| *pad* | `u8` | Unused, written as 0. |
| `N` | `u16` | How many times the body runs. |

## Behaviour

`for (N) { ... }` compiles to `evt_for`, the body and
[`evt_fornext`](evt_fornext.md). `evt_for` pushes `N` on the slot's loop
stack, records where the body starts and goes on into it in the same frame.
At `evt_fornext` the count goes down by one: the body runs again until it
reaches zero, then the entry is popped.

- Neither instruction yields. A body that should take a frame per pass
  needs an `evt_next()` or a `sleep` in it.
- The body runs at least once. `for (0)` runs it 65536 times, since the
  first `evt_fornext` takes the count below zero and it wraps.
- The count is read as a signed word, and the port reads one above 32767
  as 0, so such a loop also runs 65536 times. Keep `N` at 32767 or below.
- The loop takes one of the slot's four stack entries. `for`, `do`, `sleep`
  and `wait_message` nested inside each other may be at most four deep; a
  fifth push is dropped, and the loops then count with the wrong entries.

`for` works in every state, so a body can be a run of actor or tween
instructions. A body should end in the state it began in, since each pass
runs the same bytes.

## Example

```c
evt_work_set(WK_OBJ, OBJ_02_CLOCK);
evt_tween_begin_alt();
tw_set_pos(-20, 0, 0);
for (100)
{
    tw_pos_add();
    evt_next();
}
```

Room 105 (`games/re1/room/105/room.akscr`): moves the clock 20 units along
x each frame for 100 frames.

## See also

- [`evt_fornext`](evt_fornext.md): the end of the loop.
- [`evt_do`](evt_do.md): a loop on a condition.
- [`evt_sleep`](evt_sleep.md): waits without a body.
