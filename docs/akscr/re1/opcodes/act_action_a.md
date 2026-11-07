# `act_action_a`

Restarts the selected character's animation at a frame, remapping a monster's clip.

| | |
| --- | --- |
| Opcode | `0x89` |
| Group | Event: actor state |
| Kind | Event instruction |
| Size | 2 bytes |
| Valid in | the actor state |

## Syntax

```c
act_action_a(frame);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `frame` | `u8` | The frame to play from. |

## Behaviour

Sets the character's animation frame to `frame`, zeroes its frame timing
and walking speed, and starts a blend into the new pose over 7 steps (none
when entity flag `EF_0020` is set). Then it sets the action state:

- for a monster (an entity type below the scripted characters), the clip
  set by the last `act_anim_*` goes through the original's remap table:
  a clip below `0x10` becomes the table's clip and action state, a higher
  one parks in action state 3;
- for the player and the scripted characters, action state 1.

[`act_action_b`](act_action_b.md) is the same without the action state. It
usually follows an `act_anim_flags` to start its animation part way
through. It does not yield.

## Example

```c
evt_work_set(WK_ENEMY, ENEMY_00_NPC_ENRICO);
evt_actor_begin();
act_anim_flags(ANIM_00, F_ROOM_ANIM_DONE, 0);
act_flag_op(EF_SET, EF_0080);
act_action_a(30);
act_reset();
```

Room 30A (`games/re1/room/30a/room.akscr`): starts Enrico's animation at
frame 30.

## See also

- [`act_action_b`](act_action_b.md): the same without the remap.
- [`act_anim_flags`](act_anim_flags.md): sets the animation.
