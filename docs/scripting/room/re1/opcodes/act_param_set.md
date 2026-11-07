# `act_param_set`

Sets the selected character's turn step, which its scripted behaviour turns by.

| | |
| --- | --- |
| Opcode | `0x88` |
| Group | Event: actor state |
| Kind | Event instruction |
| Size | 4 bytes |
| Valid in | the actor state |

## Syntax

```c
act_param_set(0, step);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| *pad* | `u8` | Unused, written as 0. |
| `step` | `u16` | The turn step, in 4096ths of a turn. A value above 32767 turns the other way (64512 is -1024, a quarter turn back). |

## Behaviour

Stores `step` in the character's turn-step word. It does not yield. What
the step does depends on the state-8 behaviour running:

- behaviour 1 ([`act_anim_flags`](act_anim_flags.md)) adds it to the facing
  once, when the animation ends;
- behaviour 7 adds it to the facing every frame;
- the turning and walking behaviours ([`act_anim_seq`](act_anim_seq.md))
  turn towards the target by at most this much a frame.

`act_anim_seq` sets the step to `0x28`, and `act_anim_flags` and
`act_anim_set` set it to 0, so `act_param_set` goes after them.

## Example

```c
evt_work_set(WK_ENEMY, ENEMY_00_BARRY);
evt_actor_begin();
act_motion(MOTION_10);
act_anim_flags(ANIM_2D, F_ROOM_J_BARRY_ANIM_DONE, 0);
act_param_set(0, 64512);
act_reset();
```

Room 105 (`games/re1/room/105/room.akscr`): Barry turns a quarter turn as
his animation ends.

## See also

- [`act_anim_flags`](act_anim_flags.md), [`act_anim_seq`](act_anim_seq.md):
  the behaviours that read it.
