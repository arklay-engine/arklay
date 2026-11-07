# `pos_set`

Places the task's work at a point.

| | |
| --- | --- |
| Opcode | `0x32` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 8 bytes |
| Status | yes |

## Syntax

```c
pos_set(0, x, y, z);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `_` | `u8` | Padding, not read. Write 0. |
| `x` | `i16` | The x coordinate. |
| `y` | `i16` | The y coordinate. Negative y is up. |
| `z` | `i16` | The z coordinate. |

## Behaviour

Sets the position of the work chosen with [`work_set`](work_set.md):

- **A room object**: its position, at once.
- **A character**: its body's position, at once.
- **The player**: the position is kept and applied after the tick's scripts
  have run, together with any [`dir_set`](dir_set.md) or
  [`add_speed`](add_speed.md). Unlike a door's warp, it does not move the
  player's previous position, so the next push out of a wall starts from
  where the player stood last tick.
- **A work the engine does not model**: nothing happens, and the log notes
  `RE2 script: pos_set is not modelled` once per run.

Collision is not checked.

## Example

```c
proc main_08
{
    set(2, 7, 1);                // hold the player's input
    work_set(1, 0);
    pos_set(0, -1500, 0, 6200);
    dir_set(0, 0, 1024, 0);
    cut_chg(2);
    evt_end(0);
}
```

Illustrative: a scene's set-up places the player and turns them a quarter
turn, then holds camera 2.

## See also

- [`dir_set`](dir_set.md): turns the work.
- [`member_set`](member_set.md): writes one coordinate (members 11 to 13).
- [`work_set`](work_set.md): chooses the work.
