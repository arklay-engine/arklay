# `act_idle`

Returns the selected character to its idle state, ending any scripted behaviour.

| | |
| --- | --- |
| Opcode | `0x86` |
| Group | Event: actor state |
| Kind | Event instruction |
| Size | 1 byte |
| Valid in | the actor state |

## Syntax

```c
act_idle();
```

## Parameters

None.

## Behaviour

Writes the character's state block: state 1 (idle, or under the pad for
the player), ignore byte 0, action behaviour 0, action state 0, and the
byte at `+0x8A` 0. On the player that byte is the being-attacked flag, so
`act_idle` also releases a player an enemy's grab left held; on other
characters it is the hit state.

It stays in the actor state; follow it with
[`act_reset`](act_reset.md) or [`act_end`](act_end.md) to leave. It does
not yield.

## Example

```c
evt_work_set(WK_PLAYER, 0);
evt_actor_begin();
act_idle();
act_reset();
```

Room 100 (`games/re1/room/100/room.akscr`), event `rebecca_question_end`:
gives the player back after the scene.

## See also

- [`act_reset`](act_reset.md): leaves the actor state.
- [`tw_set_field`](tw_set_field.md): writes the state bytes one at a time.
