# `dir_set`

Places and turns the player.

| | |
| --- | --- |
| Opcode | `0x20` |
| Group | The player |
| Kind | Command |
| Size | 14 bytes |

## Syntax

```c
dir_set(0, pitch, yaw, speed_x, x, y, z);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| *pad* | `u8` | Unused; written as 0. |
| `pitch` | `i16` | The player's pitch. The shipped scripts write 0. |
| `yaw` | `i16` | The facing angle, 0 to 4095 for a full turn (only the low 12 bits are kept). |
| `speed_x` | `i16` | Written to the player's x move speed, not a rotation. The shipped scripts write 0. |
| `x` | `i16` | The position's x. |
| `y` | `i16` | The position's y (negative is up). |
| `z` | `i16` | The position's z. |

## Behaviour

`dir_set` writes the player entity's position, pitch and facing angle at
once, with no walk or turn. The dialect page calls the three words after
the pad "rotation x, y, z", but the port reads the third into the first
component of the player's move speed, as the original's record layout
does; every shipped script writes 0 there and in `pitch`.

It also clears the player's entity flag bits `0x0004` and `0x0008` and
marks the player entity active.

It does not change the floor height the player's shadow and floor checks
use. A script that moves the player to another floor follows it with
[`spd_set`](spd_set.md)`(ENT_PLAYER, y)`.

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

Room 110 (`games/re1/room/110/room.akscr`), as the player takes the west
stairs: the event puts the player on the upper floor facing angle 0, sets
the floor height to match, and hands over to the stairs AOT.

## See also

- [`spd_set`](spd_set.md): sets the player's floor height.
- [`pos_set`](pos_set.md): places an enemy.
- [`sys_multi`](sys_multi.md): `SYS_ANGLE` turns the player without moving it.
