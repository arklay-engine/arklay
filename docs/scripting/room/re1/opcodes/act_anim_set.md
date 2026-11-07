# `act_anim_set`

Plays an animation on the selected character under a chosen action behaviour, and names the flag it raises when done.

| | |
| --- | --- |
| Opcode | `0x85` |
| Group | Event: actor state |
| Kind | Event instruction |
| Size | 4 bytes |
| Valid in | the actor state |

## Syntax

```c
act_anim_set(behaviour, animation, done);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `behaviour` | `u8` | The action behaviour (`AB_xx`). |
| `animation` | `u8` | The animation clip (`ANIM_xx`). |
| `done` | `u8` | The `FG_ROOM` flag (`F_ROOM_*`) raised when the behaviour ends. |

## Behaviour

Puts the character in the scripted-action state (entity state 8), clears
its ignore byte and sets the behaviour, the animation (from the start,
action state 0) and the done flag. The turn step and the entity flags
become 0. It does not wait.

[`act_anim_flags`](act_anim_flags.md) is the same with behaviour 1 and a
flags byte. The behaviours that play an animation, rather than walk to a
target (see [`act_anim_seq`](act_anim_seq.md)):

| Behaviour | Does |
| --- | --- |
| `AB_00`, `AB_01` | plays the animation (for the player, `AB_01` goes through the remap table, as with `act_anim_flags`) |
| `AB_07` | plays it once, turning by the turn step every frame |
| `AB_08` | plays it once without the turn; for an enemy-slot character, the weapon fire: muzzle flash, shell and sound on their frames |
| `AB_09` | plays it once (for an enemy-slot character, in reverse) |

## Example

```c
evt_work_set(WK_ENEMY, ENEMY_00_BARRY);
evt_actor_begin();
act_anim_set(AB_07, ANIM_10, F_ROOM_J_BARRY_ANIM_DONE);
act_reset();
evt_nop();
```

Room 105 (`games/re1/room/105/room.akscr`), event
`barry_shoots_zombie_at_door`.

## See also

- [`act_anim_flags`](act_anim_flags.md): plays an animation under
  behaviour 1.
- [`act_anim_seq`](act_anim_seq.md): a movement behaviour.
