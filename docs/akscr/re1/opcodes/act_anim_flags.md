# `act_anim_flags`

Plays an animation on the selected character, sets its entity flags and names the flag it raises when done.

| | |
| --- | --- |
| Opcode | `0x84` |
| Group | Event: actor state |
| Kind | Event instruction |
| Size | 4 bytes |
| Valid in | the actor state |

## Syntax

```c
act_anim_flags(animation, done, flags);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `animation` | `u8` | The animation clip (`ANIM_xx`). |
| `done` | `u8` | The `FG_ROOM` flag (`F_ROOM_*`) raised when the animation ends. |
| `flags` | `u8` | Entity flags (`AFL_xx`), shifted down two bits; 0 for none. |

## Behaviour

Puts the character in the scripted-action state (entity state 8) with
action behaviour 1, the remapped animation playback, and sets:

- the animation to `animation`, from its first frame (action state 0);
- the done flag;
- the entity flags to `flags` shifted up two bits: `AFL_01` is entity flag
  `EF_0004`, `AFL_04` is `EF_0010` (loop the animation), and so on up to
  `AFL_80`, `EF_0200`;
- the turn step to 0.

It also clears the ignore byte. It does not wait: the engine plays the
animation from the next frame, and when it ends adds the turn step to the
character's facing and raises `done`. To turn the character as the
animation ends, follow with [`act_param_set`](act_param_set.md); to set
flags this form cannot reach (`EF_0001`, mirror), follow with
[`act_flag_op`](act_flag_op.md).

For the player, an animation below `0x10` selects its bank and action state
through the original's remap table, `0x10` to `0x3D` play from the room's
own animations, and `0x3E` and up are rebased by `0x3E`.

This is the commonest actor instruction in the shipped scripts.

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
player and waits for it to end.

## See also

- [`act_anim_set`](act_anim_set.md): the same with a chosen behaviour.
- [`act_anim_seq`](act_anim_seq.md): a movement behaviour.
- [`act_flag_op`](act_flag_op.md): changes the entity flags.
