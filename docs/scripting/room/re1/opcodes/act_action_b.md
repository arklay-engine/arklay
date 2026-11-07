# `act_action_b`

Restarts the selected character's animation at a frame.

| | |
| --- | --- |
| Opcode | `0x8A` |
| Group | Event: actor state |
| Kind | Event instruction |
| Size | 2 bytes |
| Valid in | the actor state |

## Syntax

```c
act_action_b(frame);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `frame` | `u8` | The frame to play from. |

## Behaviour

Sets the character's animation frame to `frame`, zeroes its frame timing
and walking speed, and starts a blend into the new pose over 7 steps (none
when entity flag `EF_0020` is set). Unlike [`act_action_a`](act_action_a.md)
it leaves the action state and the clip alone. It does not yield.

## Example

```c
evt_work_set(WK_PLAYER, 0);
evt_actor_begin();
act_anim_flags(ANIM_31, F_ROOM_PLAYER_ANIM_DONE, 0);
act_action_b(10);
sleep(7);
act_anim_flags(ANIM_30, F_ROOM_PLAYER_ANIM_DONE, 0);
act_action_b(10);
```

Room 20D (`games/re1/room/20d/room.akscr`), event `give_richard_serum`:
starts each animation at frame 10.

## See also

- [`act_action_a`](act_action_a.md): the same, with the remap for monsters.
