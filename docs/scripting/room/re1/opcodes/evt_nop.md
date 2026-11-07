# `evt_nop`

Does nothing in the top state of an event.

| | |
| --- | --- |
| Opcode | `0x00` |
| Group | Event: top state |
| Kind | Event instruction |
| Size | 1 byte |
| Valid in | the top state |

## Syntax

```c
evt_nop();
```

## Parameters

None.

## Behaviour

The event VM moves to the next instruction in the same frame. `evt_nop`
does not yield.

The byte `0x00` means a different instruction in each state: it is
[`act_nop`](act_nop.md) in the actor state and [`tw_nop`](tw_nop.md) in the
tween state, so `evt_nop()` is only accepted in the top state.

The original's scripts carry many of these, most often straight after an
actor or tween sequence returns to the top state (`act_reset(); evt_nop();`)
and after every `run` block. The one after a `run` block is not written:
the compiler emits it as the second byte of the block's terminator (see
[`evt_block`](evt_block.md)). Keep the ones the exporter writes if the
build must match the original byte for byte; otherwise they can go.

## Example

```c
evt_work_set(WK_ENEMY, ENEMY_00_BARRY);
evt_actor_begin();
act_anim_set(AB_07, ANIM_10, F_ROOM_J_BARRY_ANIM_DONE);
act_reset();
evt_nop();
wait_message();
```

Room 105 (`games/re1/room/105/room.akscr`), event
`barry_shoots_zombie_at_door`: the `evt_nop` after `act_reset` is the
original's.

## See also

- [`act_nop`](act_nop.md), [`tw_nop`](tw_nop.md): the same byte in the
  actor and tween states.
- [`evt_next`](evt_next.md): yields until the next frame.
