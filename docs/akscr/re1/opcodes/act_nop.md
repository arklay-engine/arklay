# `act_nop`

Does nothing in the actor state.

| | |
| --- | --- |
| Opcode | `0x00` |
| Group | Event: actor state |
| Kind | Event instruction |
| Size | 1 byte |
| Valid in | the actor state |

## Syntax

```c
act_nop();
```

## Parameters

None.

## Behaviour

Moves to the next instruction in the same frame and stays in the actor
state. It is the actor state's reading of the byte `0x00`, which is
[`evt_nop`](evt_nop.md) in the top state and [`tw_nop`](tw_nop.md) in the
tween state.

The original's scripts have many of these, often after a `sleep` or before
`act_reset`. They change nothing and can be dropped from a new script.

## Example

```c
evt_work_set(WK_PLAYER, 0);
evt_actor_begin();
act_motion_path(MOTION_TARGET, LOOK_AT_ENEMY, ENEMY_00_ZOMBIE_GREEN, 0, 16, 8);
sleep(30);
act_motion_bitclr();
act_nop();
```

Room 208 (`games/re1/room/208/room.akscr`).

## See also

- [`evt_nop`](evt_nop.md), [`tw_nop`](tw_nop.md): the same byte in the other
  states.
