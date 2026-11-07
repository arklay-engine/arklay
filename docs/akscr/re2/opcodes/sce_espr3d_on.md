# `sce_espr3d_on`

Starts an effect drawn in 3D, with its own rotation.

| | |
| --- | --- |
| Opcode | `0x54` |
| Group | Effects |
| Kind | Instruction |
| Size | 22 bytes |
| Status | yes |

## Syntax

```c
sce_espr3d_on(0, id, matrix, size, x, y, z, rx, ry, rz, dir);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `_` | `u8` | Not read. Write 0. |
| `id` | `u16` | Low byte: the effect's type. High byte: its row. |
| `matrix` | `u16` | Where `x, y, z` count from, as for [`sce_espr_on`](sce_espr_on.md). |
| `size` | `u16` | The sprite's size. |
| `x`, `y`, `z` | `i16` | The position or offset. |
| `rx`, `ry`, `rz` | `i16` | The effect's rotation about each axis (4096 is a full turn). |
| `dir` | `i16` | The effect's direction. |

## Behaviour

`sce_espr3d_on` is [`sce_espr_on`](sce_espr_on.md) for an effect drawn as a
turned plane rather than a sprite facing the camera (the original's
`Esp_call3D`): the effect and its sprites take the rotation `rx, ry, rz`.
The type, the placement by `matrix` and the rest are as for `sce_espr_on`.
The effect has no group (group 0).

## Example

```c
sce_espr3d_on(0, 0x0007, 0x0000, 4096, 1200, 0, -3400, 1024, 0, 0, 0);
```

Illustrative: an effect of type 7 lying flat on the floor (a quarter turn
about x) at a fixed point.

## See also

- [`sce_espr3d_on2`](sce_espr3d_on2.md): the same in a group.
- [`sce_espr_on`](sce_espr_on.md): the placement rules.
