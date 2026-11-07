# `effect_clear`

Removes every effect.

| | |
| --- | --- |
| Opcode | `0x48` |
| Group | Effects |
| Kind | Command |
| Size | 2 bytes |

## Syntax

```c
effect_clear(0);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| *pad* | `u8` | Unused; written as 0. |

## Behaviour

`effect_clear` frees all 64 slots of the effect pool at once, whatever
spawned them: the room script's effects, the weapons' and the enemies'.
It does not forget the last [`effect_tracked`](effect_tracked.md), so a
later [`effect_kill_a`](effect_kill_a.md) still matches its type and
parent. The pool is also cleared each time a room is entered.

## Example

```c
if (ck(FG_ENEMY, F_ENEMY_303_TYRANT_2_0, TRUE) && ck(FG_ROOM, F_ROOM_TYRANT_DEAD_SEEN, FALSE))
{
    set(FG_ROOM, F_ROOM_TYRANT_DEAD_SEEN, TRUE);
    bgm_stop(BGM_SUB_2);
    set(FG_STATUS, F_STATUS_COUNTDOWN_ACTIVE, FALSE);
    set(FG_STATUS, F_STATUS_ROOM_SPRITES_OFF, TRUE);
    effect_clear(0);
    // ...
}
```

Room 303 (`games/re1/room/303/room.akscr`): once the Tyrant is dead, the
script stops the countdown and removes every effect still on screen.

## See also

- [`effect_kill_a`](effect_kill_a.md), [`effect_kill_b`](effect_kill_b.md): remove some effects.
- [`mass_mask`](mass_mask.md): changes the flags of every live effect.
