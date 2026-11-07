# `sce_espr_on`

Starts an effect sprite at a point, or at an offset from a body or one of its parts.

| | |
| --- | --- |
| Opcode | `0x3A` |
| Group | Effects |
| Kind | Instruction |
| Size | 16 bytes |
| Status | yes |

## Syntax

```c
sce_espr_on(0, id, matrix, size, x, y, z, dir);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `_` | `u8` | Not read. Write 0. |
| `id` | `u16` | Low byte: the effect's type in the room's effect bank. High byte: its row (the variant of that type). |
| `matrix` | `u16` | Where `x, y, z` count from (below). |
| `size` | `u16` | The sprite's size. |
| `x`, `y`, `z` | `i16` | The position: in the world, or an offset in the body's or part's own frame. |
| `dir` | `i16` | The effect's direction. |

## Behaviour

`sce_espr_on` is the original's `Sce_espr_on`, which calls `Esp_call` with the
matrix that `Get_matrix` picks from `matrix`. Its low byte selects the frame
and its high byte is an enemy slot where one is needed:

| Low byte | `x, y, z` count from |
|---|---|
| `0` | the world's origin |
| `1` | the player |
| `2` | the partner |
| `3` | the character in the enemy slot in the high byte |
| `0x80 + who * 32 + part` | part `part` of `who`: `0` the player, `1` the partner, `2` the character in the enemy slot in the high byte |

Any other value counts from the world's origin, which is also where the
original puts an effect on a room object's matrix.

An effect on the world's origin starts at once. One on a body starts after
the scripts have run for the tick, at the offset turned by the body's facing
(or by the part's own rotation) and added to its position. It is placed
there and does not follow the body afterwards. The port uses the player's
position and facing for a part of the player, and for a character that is
not there.

The effect's type must be in the room's effect bank; a type the room did not
load starts nothing. While bank 2 bit 3 is set, the effects are held where
they are. An effect from `sce_espr_on` has no group (group 0); use
[`sce_espr_on2`](sce_espr_on2.md) to end it with
[`sce_espr_kill2`](sce_espr_kill2.md).

## Example

```c
sce_espr_on(0, 0x0102, 0x0003, 4096, 0, -1600, 300, 0);
```

Illustrative: row 1 of effect type 2, 1600 units up and 300 forward from the
character in enemy slot 0 (the high byte of `matrix`). The type and row are
placeholders for one of the room's own effects.

## See also

- [`sce_espr_on2`](sce_espr_on2.md): the same in a group.
- [`sce_espr3d_on`](sce_espr3d_on.md): an effect turned in 3D.
- [`sce_espr_kill`](sce_espr_kill.md): ends effects by type and row.
