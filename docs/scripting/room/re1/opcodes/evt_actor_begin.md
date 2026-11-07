# `evt_actor_begin`

Enters the actor state, where `act_*` instructions drive the selected character.

| | |
| --- | --- |
| Opcode | `0x01` |
| Group | Event: top state |
| Kind | Event instruction |
| Size | 1 byte |
| Valid in | the top state |

## Syntax

```c
evt_actor_begin();
```

## Parameters

None.

## Behaviour

Switches the event slot to the actor state (state 1) and continues in the
same frame. It touches no entity: the `act_*` instructions that follow act
on what the last [`evt_work_set`](evt_work_set.md) of this slot selected
(the player, if the event never selected anything).

In the actor state the event VM decodes bytes with the actor table:
[`act_nop`](act_nop.md) (`0x00`) and `0x80` to `0x8B`. The control
instructions ([`evt_next`](evt_next.md), `sleep`, `for`, `do`, `if`,
[`evt_finish`](evt_finish.md), written `act_finish()` here) work as in
every state. [`act_reset`](act_reset.md) or [`act_end`](act_end.md) returns
to the top state.

The actor instructions act on characters: the player and the enemy slots.
With a room object or item model selected (`WK_OBJ`, `WK_AOT`) they do
nothing; drive those in the tween state.

The compiler also accepts a top-state instruction whose byte is not an actor
opcode (`evt_tween_begin`, `evt_work_set`, a command, a `run` block) inside
an actor sequence, as an implicit return to the top state, because that is
how the reader decodes such a byte. The port's event VM does not do this:
it stops the event. Always end the sequence with `act_reset()` or
`act_end()` first.

## Example

```c
evt_work_set(WK_ENEMY, ENEMY_00_HUNTER);
evt_actor_begin();
act_anim_seq(AB_02, 3033, 8206, F_ROOM_ANIM_DONE);
act_finish();
```

Room 10A (`games/re1/room/10a/room.akscr`), event `hunter_film_hunter`:
sends the Hunter walking to a point and ends the event.

## See also

- [`evt_work_set`](evt_work_set.md): selects what the actor instructions act
  on.
- [`act_reset`](act_reset.md), [`act_end`](act_end.md): leave the actor
  state.
- [`evt_tween_begin_alt`](evt_tween_begin_alt.md): enters the tween state
  instead.
- [Actor and tween sequences](../index.md#actor-and-tween-sequences).
