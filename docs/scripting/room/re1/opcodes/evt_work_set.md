# `evt_work_set`

Selects the player, an enemy slot, a room object or an item model for the actor and tween instructions.

| | |
| --- | --- |
| Opcode | `0x04` |
| Group | Event: top state |
| Kind | Event instruction |
| Size | 3 bytes |
| Valid in | the top state |

## Syntax

```c
evt_work_set(kind, index);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `kind` | `u8` | What to select: `WK_PLAYER` (0), `WK_ENEMY` (1), `WK_OBJ` (2, a room object) or `WK_AOT` (3, an item model). |
| `index` | `u8` | Which one: 0 for the player, the enemy slot (`ENEMY_xx`), the room object (`OBJ_xx`) or the item model (`IMDL_xx`). |

## Behaviour

The selection belongs to the event slot: each slot keeps its own, and the
actor and tween instructions re-apply the slot's selection before each one
runs, so several events can drive different characters at once. The
instruction continues in the same frame.

- `WK_PLAYER` selects entity 0, whatever `index` is.
- `WK_ENEMY` selects entity slot `index + 1`. An index past the last slot
  selects nothing.
- `WK_OBJ` and `WK_AOT` select the room object or item model record. The
  tween instructions move these; the actor instructions do nothing on them.
- Any other `kind` selects nothing, and the instructions that follow have no
  effect.

A slot started by [`evt_fork`](evt_fork.md) or [`evt_exec`](evt_exec.md)
starts with the player selected. [`evt_chain`](evt_chain.md) keeps the
selection. The selection outlasts the actor or tween sequence, so an event
can begin a second sequence on the same entity without selecting it again.

## Example

```c
evt_work_set(WK_PLAYER, 0);
evt_actor_begin();
act_idle();
act_reset();
```

Room 100 (`games/re1/room/100/room.akscr`), event `rebecca_question_end`:
returns the player to the idle state.

## See also

- [`evt_actor_begin`](evt_actor_begin.md), [`evt_tween_begin_alt`](evt_tween_begin_alt.md):
  the sequences the selection is for.
- [`act_motion_path`](act_motion_path.md): `LOOK_AT_*` uses the same four
  kinds for a look-at target.
