# `act_motion_bitclr`

Stops the selected character's head turning, holding it where it is.

| | |
| --- | --- |
| Opcode | `0x82` |
| Group | Event: actor state |
| Kind | Event instruction |
| Size | 1 byte |
| Valid in | the actor state |

## Syntax

```c
act_motion_bitclr();
```

## Parameters

None.

## Behaviour

Clears bit `0x10` of the selected character's look-at flags, the bit that
turns the tracking joint towards its target each frame. The other bits stay,
so the joint keeps the aim it has reached rather than snapping back. When
no other bit is set (after `act_motion(MOTION_10)`, say) the flags become 0
and the look-at is dropped, as with `act_motion(0)`. It does not yield. To turn the head back to the front, use
`act_motion(MOTION_10)`; to drop the look-at, `act_motion(0)`.

## Example

```c
evt_work_set(WK_PLAYER, 0);
evt_actor_begin();
act_motion_path(MOTION_TARGET, LOOK_AT_ENEMY, ENEMY_00_ZOMBIE_GREEN, 0, 16, 8);
sleep(30);
act_motion_bitclr();
```

Room 208 (`games/re1/room/208/room.akscr`): the player watches the zombie
for 30 frames, then the head stays where it ended.

## See also

- [`act_motion_path`](act_motion_path.md), [`act_motion`](act_motion.md):
  set the look-at.
