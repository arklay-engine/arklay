# `act_flag_op`

ORs, sets or toggles the selected character's entity flags.

| | |
| --- | --- |
| Opcode | `0x87` |
| Group | Event: actor state |
| Kind | Event instruction |
| Size | 4 bytes |
| Valid in | the actor state |

## Syntax

```c
act_flag_op(mode, flags);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `mode` | `u8` | `EF_OR` (0) to add the bits, `EF_SET` (1) to replace the word, `EF_XOR` (2) to toggle the bits. |
| `flags` | `u16` | The entity flags (`EF_xxxx`, one name per bit, ORed with `\|`). |

## Behaviour

Applies `flags` to the character's entity-flags word, the one the state-8
behaviours read. Another `mode` does nothing. It does not yield.

The known bits:

| Bit | Meaning |
| --- | --- |
| `EF_0001` | mirror the scripted animation |
| `EF_0010` | loop the scripted animation |
| `EF_0020` | [`act_action_a`](act_action_a.md) and [`act_action_b`](act_action_b.md) start without blending |

The other bits have only their placeholder names.
[`act_anim_seq`](act_anim_seq.md) and [`act_anim_set`](act_anim_set.md)
clear the word and [`act_anim_flags`](act_anim_flags.md) replaces it, so
`act_flag_op` goes after them.

## Example

```c
if (ck(FG_SCENARIO, F_SCENARIO_MANSION2, FALSE))
{
    evt_work_set(WK_ENEMY, ENEMY_00_BARRY);
}
else
{
    evt_work_set(WK_ENEMY, ENEMY_00_HUNTER);
}
evt_actor_begin();
act_motion_path(MOTION_TARGET, LOOK_AT_PLAYER, 0, 0, 150, 75);
act_anim_flags(ANIM_2A, F_ROOM_ANIM_DONE, 0);
act_flag_op(EF_SET, EF_0010);
act_reset();
```

Room 109 (`games/re1/room/109/room.akscr`), event
`barry_sandwich_talk_both_1`: Barry on the first visit, the Hunter on the
second, watches the player and loops an animation.

## See also

- [`act_anim_flags`](act_anim_flags.md): sets the flags with an animation.
