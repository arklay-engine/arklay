# `enemy`

Puts an enemy or a character in one of the room's enemy slots.

| | |
| --- | --- |
| Opcode | `0x1B` |
| Group | Enemies and characters |
| Kind | Command |
| Size | 22 bytes |

## Syntax

```c
enemy(type, behaviour, guard, spawn, hit_records, pitch, yaw, roll, x, y, z, slot, animation, frame, variant);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `type` | `u8` | The enemy or character type (`EM_xx_*`, from `inc/enemies.akinc`). Types past `0x2E` are ignored. |
| `behaviour` | `u8` | The behaviour byte (`EB_xx`); its meaning depends on the type. |
| `guard` | `u8` | The `FG_ENEMY` bit that keeps the enemy away once set (`F_ENEMY_<room>_*`), or `NO_FLAG` (255). The bit is set when the enemy dies. |
| `spawn` | `u8` | `SPAWN_FORCE` (1, or any non-zero value) sets the enemy up from this record; `SPAWN_RESTORE` (0) restores it from a saved snapshot when there is one. |
| `hit_records` | `u8` | Not read by the port; the shipped scripts write 1 or 2. |
| `pitch` | `i16` | The rotation about x. |
| `yaw` | `i16` | The facing angle, 0 to 4095 for a full turn. |
| `roll` | `i16` | The rotation about z. |
| `x` | `u16` | The position, x. |
| `y` | `i16` | The position, y (negative is up). |
| `z` | `u16` | The position, z. |
| `slot` | `u8` | The enemy slot, 0 to 15 (`ENEMY_xx`, a room define); only the low four bits count. |
| `animation` | `u8` | The animation clip it starts in (`ANIM_xx`). |
| `frame` | `u8` | The frame of that clip it starts at. |
| `variant` | `u8` | The model variant (`EVAR_xx`); only the low four bits count. |

## Behaviour

When `guard` names a bit and that bit of `FG_ENEMY` is set (the enemy is
already dead), or `type` is past `0x2E`, `enemy` does nothing. Otherwise
it sets up the entity of slot `slot` (enemy slot 0 is the entity after the
player):

1. It writes the record's `y`, and a spawn key made of the slot, the
   variant and the `spawn` byte.
2. With `SPAWN_RESTORE`, a snapshot the game saved of an enemy of this room
   with the same key restores the enemy's status, behaviour flags, x and z
   (and y for some behaviours) and facing, and the record's position and
   angle are not used. Without a snapshot, or with `SPAWN_FORCE`, the
   record sets the type, behaviour, rotation, position, animation and
   frame, and the entity becomes active.
3. Either way, an active entity is then reset: its state, action and hit
   state are cleared, its type, guard bit, pitch and roll are taken from the
   record, and its collision volume is the default until the type's own
   code replaces it. The room's enemy count goes up by one when the slot
   was empty.

The enemy's own code (the native character drivers or the pack's Lua
enemy scripts) runs it from the next frame.

## Example

```c
enemy(EM_00_ZOMBIE_GROUNDSKEEPER, EB_00, F_ENEMY_103_ZOMBIE_GROUNDSKEEPER_0, SPAWN_FORCE, 2, 0, 1024, 0, 8380, 0, 25515, ENEMY_00_ZOMBIE_GROUNDSKEEPER, ANIM_00, 0, EVAR_00);
```

```c
enemy(EM_2C_NPC_REBECCA_2, EB_00, NO_FLAG, SPAWN_FORCE, 2, 0, 2576, 0, 13096, 0, 2888, ENEMY_00_REBECCA, ANIM_04, 29, EVAR_00);
```

Room 103 (`games/re1/room/103/room.akscr`) puts a zombie in slot 0, kept
away by its `FG_ENEMY` bit once it is dead. Room 101 places Rebecca's body,
which no flag keeps away, in animation 4 at frame 29.

## See also

- [`pos_set`](pos_set.md): moves an enemy.
- [`eml_state`](eml_state.md): changes an enemy's state.
- [`ck`](ck.md): `ck(FG_ENEMY, F_ENEMY_.., TRUE)` tests whether it is dead.
