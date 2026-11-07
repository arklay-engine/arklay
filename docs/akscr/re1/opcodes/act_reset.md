# `act_reset`

Clears the selected character's ignore byte and returns to the top state.

| | |
| --- | --- |
| Opcode | `0x80` |
| Group | Event: actor state |
| Kind | Event instruction |
| Size | 1 byte |
| Valid in | the actor state |

## Syntax

```c
act_reset();
```

## Parameters

None.

## Behaviour

Sets the selected character's ignore byte (the high byte of its `+0x84`
state word, `TW_IGNORE`) to 0, then leaves the actor state for the top
state and continues in the same frame. The entity's state, behaviour and
animation are left as the actor instructions set them, so an animation or
walk that `act_anim_*` started goes on after the sequence ends.

This is how the shipped scripts close almost every actor sequence; the
original's data usually follows it with an [`evt_nop`](evt_nop.md).
[`act_end`](act_end.md) returns to the top state without touching the
entity.

## Example

```c
evt_work_set(WK_ENEMY, ENEMY_00_BARRY);
evt_actor_begin();
act_anim_set(AB_07, ANIM_10, F_ROOM_J_BARRY_ANIM_DONE);
act_reset();
evt_nop();
```

Room 105 (`games/re1/room/105/room.akscr`), event
`barry_shoots_zombie_at_door`.

## See also

- [`act_end`](act_end.md): leaves the actor state without the reset.
- [`act_idle`](act_idle.md): returns the character to its idle state.
- [`evt_actor_begin`](evt_actor_begin.md): enters the actor state.
