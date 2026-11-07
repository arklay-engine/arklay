# `effect_kill_b`

Removes the effects of one type and animation row byte.

| | |
| --- | --- |
| Opcode | `0x42` |
| Group | Effects |
| Kind | Command |
| Size | 4 bytes |

## Syntax

```c
effect_kill_b(type, row);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `type` | `u8` | The effect's sprite (`FX_xx`, a room define). |
| `row` | `u16` | The animation row byte (`FX_ROW_xx`) the effects were spawned with; only its low byte counts. |

## Behaviour

`effect_kill_b` frees every live effect slot whose type is `type` and
whose row byte, as [`effect`](effect.md) or
[`effect_tracked`](effect_tracked.md) gave it, equals the low byte of
`row`. The whole byte must match, tint level included: `FX_ROW_08` (row 0
at tint level 1) is not `FX_ROW_00`. The parent and position do not
matter.

The `.s` assembler prints the instruction as raw bytes; `.akscr` takes one
argument per field.

## Example

```c
if (ck(FG_ROOM, F_ROOM_CODE_1_LAMP_LIT, FALSE))
{
    set(FG_ROOM, F_ROOM_CODE_1_LAMP_LIT, TRUE);
    effect(FX_0A, FX_ROW_08, FX_AT_WORLD, 2230, -3000, 5300, 0);
    effect_kill_b(FX_0A, FX_ROW_00);
}
```

Room 508 (`games/re1/room/508/room.akscr`): when the first pass code is
entered, the script spawns the lit lamp (row 0, tint level 1) and removes
the unlit one (row 0, tint level 0) of the same sprite.

## See also

- [`effect`](effect.md): spawns an effect.
- [`effect_kill_a`](effect_kill_a.md): removes the tracked effect.
- [`effect_clear`](effect_clear.md): removes every effect.
