# `act_anim_seq`

Starts a scripted movement behaviour on the selected character, such as walking or running to a point, and names the flag it raises when done.

| | |
| --- | --- |
| Opcode | `0x83` |
| Group | Event: actor state |
| Kind | Event instruction |
| Size | 7 bytes |
| Valid in | the actor state |

## Syntax

```c
act_anim_seq(behaviour, x, z, done);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `behaviour` | `u8` | The action behaviour (`AB_xx`), optionally ORed with `AB_ADDITIVE` (`0x10`). |
| `x` | `i16` | The target's x. |
| `z` | `i16` | The target's z. |
| `done` | `u8` | The `FG_ROOM` flag (`F_ROOM_*`) the behaviour raises when it ends. |

## Behaviour

Puts the character in the scripted-action state (entity state 8), clears
its ignore byte and sets:

- the target point (`x`, `z`), the one [`tw_set_sel`](tw_set_sel.md) calls
  `TW_TARGET_X` and `TW_TARGET_Z`;
- the done flag;
- the turn step to `0x28` (see [`act_param_set`](act_param_set.md));
- the entity flags (`EF_*`) to 0.

From the next frame the engine's state-8 driver runs the behaviour; the
instruction itself does not wait. The behaviours that use the target:

| Behaviour | Does |
| --- | --- |
| `AB_02` | turns to face the target, then walks to it |
| `AB_03` | the same, running |
| `AB_04`, `AB_05` | walks backwards to the target, fast or slow |
| `AB_06` | turns in place |

Other behaviours play an animation, as with
[`act_anim_set`](act_anim_set.md). When the behaviour ends it sets the
`done` flag, which the event usually waits on with a `do`/`while`.

`behaviour` without `AB_ADDITIVE` starts afresh: the action state is reset
and the character's scripted lock (bit `0x80` of its collision flags) is
cleared. With `AB_ADDITIVE`, the low four bits are the behaviour and the
lock is set; the behaviour then keeps running when it ends. Given while
the lock is already set, `AB_ADDITIVE` changes the behaviour and target
without resetting the action state, so a walk already under way goes on
to the new point without restarting. On the player the lock is also the
arrival gate that hands control back to the pad.

The movement steps without room collision for characters other than the
player, as in the original; a path may cross geometry.

## Example

```c
evt_work_set(WK_ENEMY, ENEMY_00_BARRY);
evt_actor_begin();
act_anim_seq(AB_03, 17300, 12900, F_ROOM_J_BARRY_ANIM_DONE);
act_nop();
act_reset();
```

Room 105 (`games/re1/room/105/room.akscr`): Barry runs to a point; the
event then waits for `F_ROOM_J_BARRY_ANIM_DONE`.

## See also

- [`act_anim_set`](act_anim_set.md): plays an animation under a behaviour.
- [`act_anim_flags`](act_anim_flags.md): plays an animation.
- [`act_param_set`](act_param_set.md): sets the turn step.
