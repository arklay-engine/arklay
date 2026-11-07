# `plw_anim`

Puts the player into a scripted animation.

| | |
| --- | --- |
| Opcode | `0x2B` |
| Group | The player |
| Kind | Command |
| Size | 4 bytes |

## Syntax

```c
plw_anim(state, animation, frame);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `state` | `u8` | The player's action state, less two (`PLW_xx`). |
| `animation` | `u8` | The animation clip (`ANIM_xx`). |
| `frame` | `u8` | The animation frame id to start from. |

## Behaviour

`plw_anim` switches the player entity to state 8, the scripted animation
state, and plays `animation` from `frame`. The action behaviour becomes 0
and the action state `state + 2` (wrapping at 8 bits): this is the
original's split of a signed behaviour word whose low byte is the opcode,
so `PLW_02` gives action state 4. The scripted clip index is recorded for
the player's scripted animation driver, and two internal player bytes
(`+0x8C` and `+0xBF` in the original's entity) are cleared.

The `.s` assembler prints the instruction as raw bytes; `.akscr` takes one
argument per field.

## Example

```c
cut_auto(CUT_LOCKED);
setb(B_CAMERA, CAM_04, 0);
plw_anim(PLW_02, ANIM_30, 0);
sys_multi(SYS_PLAYER_FLAGS, SYS_OR | PLF_04);
sys_multi(SYS_UNEQUIP);
evt_exec(0, TASK_09, wake_up_after_poison);
```

Room 100 (`games/re1/room/100/room.akscr`): when Chris wakes after the
Yawn's poison, the init puts him into clip `ANIM_30`, takes his weapon
away and starts the scene's event.

## See also

- [`sys_multi`](sys_multi.md): the player's other properties, `SYS_IDLE` to return to idle.
- [`dir_set`](dir_set.md): places and turns the player.
