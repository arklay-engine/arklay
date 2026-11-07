# `evt_sleep`

Waits a number of frames; written as `sleep(N);`.

| | |
| --- | --- |
| Opcode | `0xF8` |
| Group | Event: control |
| Kind | Event instruction |
| Size | 4 bytes |
| Valid in | every state |

## Syntax

```c
sleep(N);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| *tick* | `u8` | Always `0xF9`, the [`evt_sleep_tick`](evt_sleep_tick.md) byte; the compiler writes it. |
| `N` | `u16` | The number of frames to wait. |

## Behaviour

`sleep(N);` compiles to the bytes `F8 F9`, then `N` as a little-endian
word. The first time the slot reaches it, `evt_sleep` pushes `N` on the
slot's loop stack. Each frame it then counts down one and yields; when the
count reaches zero it pops the entry and moves on, and yields that frame
too. The instruction after `sleep(N);` therefore runs `N` frames later:
`sleep(1);` lets one frame end.

- `sleep(0);` does not wait or yield: the event goes straight on.
- The count is read as a signed word. The compiler takes `N` up to 65535,
  but in the port a count above 32767 is read as 0 and does not wait at
  all. Use two `sleep`s for a longer wait.
- The counter takes one of the slot's four stack entries while it runs,
  shared with `for`, `do` and `wait_message`.

`sleep` works in every state. In an actor sequence it holds the actor state
while the character's animation plays; in a tween sequence it holds the
selection still.

## Example

```c
evt_work_set(WK_PLAYER, 0);
evt_actor_begin();
act_motion_path(MOTION_13, 4900, -8000, 4900, 100, 100);
sleep(20);
act_motion(MOTION_10);
sleep(10);
act_end();
```

Room 115 (`games/re1/room/115/room.akscr`), event `jill_pushes_ceiling`:
the player looks up at a point for 20 frames, then straight ahead again,
and the actor sequence ends 10 frames later.

## See also

- [`evt_next`](evt_next.md): waits one frame.
- [`evt_sleep_tick`](evt_sleep_tick.md): the second byte of the `sleep`.
- [`evt_for`](evt_for.md): repeats a body.
