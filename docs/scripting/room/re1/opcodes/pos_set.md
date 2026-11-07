# `pos_set`

Places and turns an enemy.

| | |
| --- | --- |
| Opcode | `0x21` |
| Group | Enemies and characters |
| Kind | Command |
| Size | 14 bytes |

## Syntax

```c
pos_set(enemy, pitch, yaw, roll, x, y, z);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `enemy` | `u8` | The enemy slot (`ENEMY_xx`, a room define). |
| `pitch` | `i16` | The rotation about x. |
| `yaw` | `i16` | The facing angle; only the low 12 bits count (0 to 4095 for a full turn). |
| `roll` | `i16` | The rotation about z. |
| `x` | `i16` | The position, x. |
| `y` | `i16` | The position, y (negative is up). |
| `z` | `i16` | The position, z. |

## Behaviour

`pos_set` writes the entity's position (and its saved position), its
rotation, and clears two of its entity flags (`0x0004` and `0x0008`). It
does not change whether the slot is active: on a slot the room's
[`enemy`](enemy.md) records left empty (the enemy is dead) it moves an
inactive entity, and nothing appears. A slot past the entity table is
ignored.

The entity's own code carries on from the new place on its next update,
so a scene usually also sets its behaviour with
[`eml_state`](eml_state.md).

## Example

```c
run
{
    pos_set(ENEMY_00_HUNTER, 0, 914, 0, 2524, 0, 12163);
    eml_state(0, ENEMY_00_HUNTER, EML_BEHAVIOUR, EB_67);
    // ...
}
```

Room 10A (`games/re1/room/10a/room.akscr`) moves the Hunter to the door for
its scene and gives it the behaviour that runs the scene.

## See also

- [`enemy`](enemy.md): sets an enemy up.
- [`dir_set`](dir_set.md): places and turns the player.
- [`eml_state`](eml_state.md): changes an enemy's state.
