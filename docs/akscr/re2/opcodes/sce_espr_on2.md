# `sce_espr_on2`

Starts an effect sprite in a group, for `sce_espr_kill2` to end.

| | |
| --- | --- |
| Opcode | `0x64` |
| Group | Effects |
| Kind | Instruction |
| Size | 16 bytes |
| Status | yes |

## Syntax

```c
sce_espr_on2(group, id, matrix, size, x, y, z, dir);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `group` | `u8` | The group the effect belongs to. |
| `id` | `u16` | Low byte: the effect's type. High byte: its row. |
| `matrix` | `u16` | Where `x, y, z` count from, as for [`sce_espr_on`](sce_espr_on.md). |
| `size` | `u16` | The sprite's size. |
| `x`, `y`, `z` | `i16` | The position or offset. |
| `dir` | `u16` | The effect's direction. |

## Behaviour

`sce_espr_on2` is [`sce_espr_on`](sce_espr_on.md) (the original's
`Esp_call2`) with a group: the effect and the sprites that go with it are
marked with `group`, and [`sce_espr_kill2`](sce_espr_kill2.md)`(group)` ends
them all together. Effects started without a group are in group 0.

## Example

```c
sce_espr_on2(3, 0x0004, 0x0000, 4096, -2200, -400, 5100, 0);
sce_espr_on2(3, 0x0004, 0x0000, 4096, -1900, -400, 5300, 0);
sleep(10, 90);
sce_espr_kill2(3);
```

Illustrative: two effects of type 4 at fixed points in the room, ended
together three seconds later.

## See also

- [`sce_espr_kill2`](sce_espr_kill2.md): ends a group.
- [`sce_espr3d_on2`](sce_espr3d_on2.md): the 3D form in a group.
