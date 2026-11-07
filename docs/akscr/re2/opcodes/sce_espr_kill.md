# `sce_espr_kill`

Ends every effect of one type and row.

| | |
| --- | --- |
| Opcode | `0x4C` |
| Group | Effects |
| Kind | Instruction |
| Size | 5 bytes |
| Status | yes |

## Syntax

```c
sce_espr_kill(type, row, a2, a3);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `type` | `u8` | The effect type, as the low byte of `sce_espr_on`'s `id`. |
| `row` | `u8` | The row, as the high byte of `id`. |
| `a2`, `a3` | `u8` | Not read by the port. |

## Behaviour

`sce_espr_kill` ends every live effect sprite whose type is `type` and whose
row is `row`, wherever it is and whoever started it: the room's own effects
of that kind too. The port does not read the last two bytes. The original's
`Esp_kill` can also take a matrix, to end only those at one place; the port's
scripts always end them all.

## Example

```c
sce_espr_on(0, 0x0102, 0x0000, 4096, 500, -1200, 800, 0);
sleep(10, 60);
sce_espr_kill(2, 1, 0, 0);
```

Illustrative: an effect of type 2, row 1, ended two seconds later.

## See also

- [`sce_espr_kill2`](sce_espr_kill2.md): ends effects by group.
- [`sce_espr_on`](sce_espr_on.md): starts an effect.
