# `sce_espr3d_on2`

Starts an effect drawn in 3D, in a group for `sce_espr_kill2` to end.

| | |
| --- | --- |
| Opcode | `0x75` |
| Group | Effects |
| Kind | Instruction |
| Size | 22 bytes |
| Status | yes |

## Syntax

```c
sce_espr3d_on2(group, id, matrix, size, x, y, z, rx, ry, rz, dir);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `group` | `u8` | The group the effect belongs to. |
| `id` | `u16` | Low byte: the effect's type. High byte: its row. |
| `matrix` | `u16` | Where `x, y, z` count from, as for [`sce_espr_on`](sce_espr_on.md). |
| `size` | `u16` | The sprite's size. |
| `x`, `y`, `z` | `i16` | The position or offset. |
| `rx`, `ry`, `rz` | `i16` | The effect's rotation about each axis. |
| `dir` | `u16` | The effect's direction. |

## Behaviour

`sce_espr3d_on2` is [`sce_espr3d_on`](sce_espr3d_on.md) with a group, as
[`sce_espr_on2`](sce_espr_on2.md) is `sce_espr_on` with one (the original's
`Esp_call3D2`). [`sce_espr_kill2`](sce_espr_kill2.md)`(group)` ends it.

## Example

```c
sce_espr3d_on2(5, 0x0107, 0x0000, 4096, 1200, 0, -3400, 1024, 0, 0, 0);
evt_next();
sce_espr_kill2(5);
```

Illustrative: a 3D effect at a fixed point, shown for a tick and ended.

## See also

- [`sce_espr3d_on`](sce_espr3d_on.md): the same without a group.
- [`sce_espr_kill2`](sce_espr_kill2.md): ends a group.
