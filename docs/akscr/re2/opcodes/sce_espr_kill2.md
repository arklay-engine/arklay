# `sce_espr_kill2`

Ends every effect in a group.

| | |
| --- | --- |
| Opcode | `0x65` |
| Group | Effects |
| Kind | Instruction |
| Size | 2 bytes |
| Status | yes |

## Syntax

```c
sce_espr_kill2(group);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `group` | `u8` | The group, as given to [`sce_espr_on2`](sce_espr_on2.md) or [`sce_espr3d_on2`](sce_espr3d_on2.md). |

## Behaviour

`sce_espr_kill2` is the original's `Esp_kill2`: every live effect sprite
marked with `group` ends. Every effect that was started without a group is in
group 0, so `sce_espr_kill2(0)` ends all of those, the room's own included.
Use groups from 1 up for a script's own effects.

## Example

```c
sce_espr_on2(2, 0x0003, 0x0001, 4096, 0, -900, 0, 0);
sleep(10, 30);
sce_espr_kill2(2);
```

Illustrative: an effect over the player for a second.

## See also

- [`sce_espr_on2`](sce_espr_on2.md), [`sce_espr3d_on2`](sce_espr3d_on2.md): start effects in a group.
- [`sce_espr_kill`](sce_espr_kill.md): ends effects by type and row.
