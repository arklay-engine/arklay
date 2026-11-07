# `door_aot_set_4p`

Puts a door with a four-cornered area in one of the room's area slots.

| | |
| --- | --- |
| Opcode | `0x68` |
| Group | Interaction areas and doors |
| Kind | Instruction |
| Size | 40 bytes |
| Status | yes |

## Syntax

```c
door_aot_set_4p(slot, sce, sat, floor, super, x0, z0, x1, z1, x2, z2, x3, z3,
                next_x, next_y, next_z, next_dir, next_stage, next_room, next_cut, 0,
                do2, door_type, sound, lock, key, 0);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `slot`, `sce`, `sat`, `floor`, `super` | `u8` | As for [`door_aot_set`](door_aot_set.md). |
| `x0`, `z0` ... `x3`, `z3` | `i16` | The four corners of the area, in order around it. |
| `next_x`, `next_y`, `next_z`, `next_dir` | `i16` | Where the player arrives and the facing, as for `door_aot_set`. |
| `next_stage`, `next_room`, `next_cut` | `u8` | The stage (from 0), room and camera to arrive at. |
| `_` | `u8` | Not read. Write 0. |
| `do2`, `door_type`, `sound` | `u8` | The door animation's DO2 file and type, and its sound. |
| `lock`, `key` | `u8` | The lock and the key, as for `door_aot_set`. |
| `_` | `u8` | Not read. Write 0. |

## Behaviour

`door_aot_set_4p` is [`door_aot_set`](door_aot_set.md) with a four-sided area
in place of the box, tested as [`aot_set_4p`](aot_set_4p.md)'s is. The door
record, the locks and what happens when it fires are the same.

## Example

```c
door_aot_set_4p(1, 1, 0x31, 0, 0, 2000, 5000, 3200, 5400, 3000, 6200, 1800, 5800,
                9800, 0, -2400, 3072, 1, 10, 0, 0,
                3, 0, 1, 0x85, 0x4C, 0);
```

Illustrative: a slanted door to room 10 of stage 2 (`next_stage` 1), locked by
unlocked-door bit 5 and opened with item `0x4C`. All the values are
placeholders.

## See also

- [`door_aot_set`](door_aot_set.md): the box form and the door's behaviour.
- [`aot_set_4p`](aot_set_4p.md): the four-corner area test.
