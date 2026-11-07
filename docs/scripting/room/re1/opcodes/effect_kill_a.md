# `effect_kill_a`

Removes the effects of the last tracked effect's type and parent.

| | |
| --- | --- |
| Opcode | `0x3E` |
| Group | Effects |
| Kind | Command |
| Size | 2 bytes |

## Syntax

```c
effect_kill_a(0);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| *pad* | `u8` | Unused; written as 0. |

## Behaviour

`effect_kill_a` frees every live effect slot whose type and parent match
the last [`effect_tracked`](effect_tracked.md) in the room. It matches by
type and parent alone, so it also removes effects of the same type on the
same parent that a plain [`effect`](effect.md) or an effect's own
behaviour spawned. The record stays after the kill, so a second
`effect_kill_a` removes any newer effects of that pair too. With no
tracked effect since the room was entered, it does nothing.

## Example

```c
effect_tracked(FX_14, FX_ROW_00, FX_AT_WORLD, 6800, 60736, 14000, 2048);
sleep(55);
effect_kill_a(0);
effect_tracked(FX_14, FX_ROW_00, FX_AT_WORLD, 3000, 0, 11500, 0);
```

Room 40c (`games/re1/room/40c/room.akscr`) shows an effect for 55 frames,
removes it, and starts the same effect somewhere else.

## See also

- [`effect_tracked`](effect_tracked.md): spawns the effect this removes.
- [`effect_kill_b`](effect_kill_b.md): removes the effects of a type and row.
- [`effect_clear`](effect_clear.md): removes every effect.
