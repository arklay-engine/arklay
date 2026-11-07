# `spd_set`

Sets the player's floor height, or an enemy's height.

| | |
| --- | --- |
| Opcode | `0x41` |
| Group | Enemies and characters |
| Kind | Command |
| Size | 4 bytes |

## Syntax

```c
spd_set(entity, height);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `entity` | `u8` | The entity: `ENT_PLAYER` (0), or `ENT_xx` for entity `xx`, where the enemies follow the player (`ENT_01` is enemy slot 0). |
| `height` | `i16` | The height (negative is up). |

## Behaviour

For the player, `spd_set` writes the floor height the player stands on:
the reference the shadow is drawn at and the floor checks use, not the
model's position. A script that moves the player to another floor with
[`dir_set`](dir_set.md) writes the same y here, so the shadow and the
floor follow.

For any other entity it writes the entity's y position directly. Every
shipped use names the player. An entity past the table is ignored.

## Example

```c
run
{
    aot_delete(AOT_05_EVENT_TO_WEST_STAIRS, SCE_NONE, SAT_UNDER | SAT_PLAYER);
    cut_auto(CUT_LOCKED);
    dir_set(0, 0, 0, 0, 11336, -4315, 10556);
    spd_set(ENT_PLAYER, -4315);
    aot_on(AOT_03_5_STAIRS_EAST, SCE_5, 0);
    cutnext(CAM_01_EAST);
    cut_auto(CUT_FREE);
}
```

Room 110 (`games/re1/room/110/room.akscr`) puts the player on the stairs at
y -4315 and sets the floor height to match.

## See also

- [`spd_add`](spd_add.md): moves the player's floor height by a step.
- [`dir_set`](dir_set.md): places the player.
