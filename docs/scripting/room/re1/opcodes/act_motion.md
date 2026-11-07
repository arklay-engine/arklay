# `act_motion`

Sets the selected character's look-at mode without a target, for example to turn its head back to straight ahead.

| | |
| --- | --- |
| Opcode | `0x81` |
| Group | Event: actor state |
| Kind | Event instruction |
| Size | 2 bytes |
| Valid in | the actor state |

## Syntax

```c
act_motion(mode);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `mode` | `u8` | The look-at flags (`MOTION_xx`). The low four bits must be 0. |

## Behaviour

Opcode `0x81` has two forms, told apart by the mode byte. With the low four
bits of `mode` clear it is this 2-byte `act_motion`; with any of them set
it is the 10-byte [`act_motion_path`](act_motion_path.md), which also
carries a target. The compiler checks that the mode matches the name and
says so when it does not.

`act_motion` stores `mode` as the character's look-at flags and changes
nothing else: the target and the turn steps stay as the last
`act_motion_path` left them. The look-at aims the model's tracking joint
(the head) and is applied each frame after the character's own animation;
it does not turn or move the body. The bits of `mode` that matter here:

| Bit | Meaning |
| --- | --- |
| `0x10` | the joint turns towards its target each frame |
| `0x20`, `0x40`, `0x80` | absolute angles, a glance and a live target: see [`act_motion_path`](act_motion_path.md) |

With the yaw and pitch bits clear, the joint's target is straight ahead,
so `act_motion(MOTION_10)` turns the head back to the front at the last
steps given. `act_motion(0)` drops the look-at altogether, and the joint
takes its animated pose at once.

The instruction does not yield and leaves the actor state as it is. On a
room object or item model selection it does nothing.

## Example

```c
evt_work_set(WK_ENEMY, ENEMY_00_BARRY);
evt_actor_begin();
act_motion(MOTION_10);
act_anim_flags(ANIM_2D, F_ROOM_J_BARRY_ANIM_DONE, 0);
act_param_set(0, 64512);
act_reset();
```

Room 105 (`games/re1/room/105/room.akscr`): Barry's head turns back to the
front as he plays an animation.

## See also

- [`act_motion_path`](act_motion_path.md): the same opcode with a target.
- [`act_motion_bitclr`](act_motion_bitclr.md): stops the head turning.
