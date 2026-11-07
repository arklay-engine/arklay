# `act_motion_path`

Aims the selected character's head at a point, at fixed angles, or at another character or model.

| | |
| --- | --- |
| Opcode | `0x81` |
| Group | Event: actor state |
| Kind | Event instruction |
| Size | 10 bytes |
| Valid in | the actor state |

## Syntax

```c
act_motion_path(mode, x, y, z, yaw_step, pitch_step);
act_motion_path(MOTION_TARGET, kind, index, 0, yaw_step, pitch_step);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `mode` | `u8` | The look-at flags (`MOTION_xx`, or `MOTION_TARGET`). At least one of the low four bits must be set. |
| `x` | `i16` | Target x, or the yaw with bit `0x20`. With `MOTION_TARGET`, the target kind: `LOOK_AT_PLAYER`, `LOOK_AT_ENEMY`, `LOOK_AT_OBJ` or `LOOK_AT_ITEM`. |
| `y` | `i16` | Target y, or the pitch with bit `0x20`. With `MOTION_TARGET`, the target's index (`ENEMY_xx`, `OBJ_xx`, `IMDL_xx`; 0 for the player). |
| `z` | `i16` | Target z, or the number of swings with bit `0x40`. Unused with `MOTION_TARGET` (0). |
| `yaw_step` | `u8` | How far the head turns sideways per frame, in 4096ths of a turn; 0 means `0xC0`. |
| `pitch_step` | `u8` | How far it tilts per frame; 0 means `0x40`. |

## Behaviour

This is the 10-byte form of opcode `0x81`; the 2-byte form, with the low
four bits of `mode` clear, is [`act_motion`](act_motion.md). The compiler
checks the name against the mode.

It sets the look-at of the selected character: the flags, the target and
the two steps. Each frame after its animation, the engine turns the
model's tracking joint (the head) towards the target by at most the step
on each axis, within the joint's limits. The body does not turn or move,
and the instruction does not wait: it returns at once, and the head keeps
turning while the event goes on. The `mode` bits:

| Bit | Meaning |
| --- | --- |
| `0x01` | turn the head sideways (yaw) |
| `0x02` | tilt the head (pitch) |
| `0x10` | turn towards the target each frame; [`act_motion_bitclr`](act_motion_bitclr.md) clears it |
| `0x20` | `x` and `y` are a yaw and a pitch relative to the body, not a point; a negative one is taken as 4096 more |
| `0x40` | a glance: the head swings from side to side `z` times, then settles |
| `0x80` | follow a live target |

`MOTION_TARGET` (`0x93`: follow, turn, yaw and pitch) is the one mode that
names a live target. The head then follows the target's position, less a
head height, every frame: the player, an enemy slot, a room object or an
item model. A mode with bit `0x80` but not exactly `0x93` keeps `x`, `y`,
`z` as a point and follows nothing.

The selection must be a character (`WK_PLAYER` or `WK_ENEMY`); on a room
object or item model the instruction does nothing.

## Example

```c
evt_work_set(WK_PLAYER, 0);
evt_actor_begin();
act_motion_path(MOTION_TARGET, LOOK_AT_ENEMY, ENEMY_00_ZOMBIE_GREEN, 0, 16, 8);
sleep(30);
act_motion_bitclr();
act_nop();
```

Room 208 (`games/re1/room/208/room.akscr`): the player's head follows the
zombie for 30 frames, slowly, then stops turning.

```c
evt_work_set(WK_PLAYER, 0);
evt_actor_begin();
act_motion_path(MOTION_32, 0, 250, 0, 0, 250);
act_reset();
```

Room 10B (`games/re1/room/10b/room.akscr`), event `rebecca_radio_call`:
the player's head tilts to a fixed pitch of 250 (`0x32`: follow, pitch,
absolute angles).

## See also

- [`act_motion`](act_motion.md): the 2-byte form, without a target.
- [`act_motion_bitclr`](act_motion_bitclr.md): stops the head turning.
- [`evt_work_set`](evt_work_set.md): the target kinds match its `WK_*`.
