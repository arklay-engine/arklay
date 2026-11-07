# `evt_do`

Starts a loop that repeats its body while a condition passes; written as `do { ... } while (cond);`.

| | |
| --- | --- |
| Opcode | `0xFC` |
| Group | Event: control |
| Kind | Event instruction |
| Size | 2 bytes plus its condition command |
| Valid in | every state |

## Syntax

```c
do
{
    event statements...
}
while (cond);
```

## Parameters

The condition is written in the `while`. The compiler fills in the jump:

| Parameter | Width | Description |
| --- | --- | --- |
| `body` | `u8` | Bytes from the opcode to the start of the body, past the condition command. |

## Behaviour

`do { ... } while (cond);` compiles to `evt_do`, the condition command,
the body and [`evt_dountil`](evt_dountil.md). The condition is stored
inline after `evt_do`, which pushes an entry with its address and the
body's start on the slot's loop stack and jumps over it into the body. At
the end of the body, `evt_dountil` tests the condition: while it passes
the body runs again; when it fails the entry is popped and the event goes
on.

- The body runs once before the first test.
- The condition is one [condition command](index.md#conditions), with
  no `&&`. To wait on two things, nest two loops or use an event `if`
  inside.
- Neither instruction yields. A body without `evt_next()` (or another
  yield) runs again in the same frame. The port stops stepping a slot for
  the frame after 100,000 instructions, so such a wait spins every frame
  without the event getting any further until the condition fails.
- The loop takes one of the slot's four stack entries.

The usual wait on a flag is a `do` whose body is one `evt_next()`.

## Example

```c
evt_work_set(WK_PLAYER, 0);
evt_actor_begin();
act_anim_flags(ANIM_21, F_ROOM_J_PLAYER_ANIM_DONE, 0);
act_reset();
evt_nop();
do
{
    evt_next();
}
while (ck(FG_ROOM, F_ROOM_J_PLAYER_ANIM_DONE, FALSE));
```

Room 105 (`games/re1/room/105/room.akscr`): plays an animation on the
player and waits, a frame at a time, until it raises its done flag.

## See also

- [`evt_dountil`](evt_dountil.md): the end of the loop.
- [`evt_if`](evt_if.md): a condition without a loop.
- [`evt_for`](evt_for.md): a counted loop.
