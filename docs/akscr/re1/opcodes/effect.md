# `effect`

Spawns an effect, a sprite animation, at a point in the world or on the player, an enemy or an object.

| | |
| --- | --- |
| Opcode | `0x2A` |
| Group | Effects |
| Kind | Command |
| Size | 12 bytes |

## Syntax

```c
effect(type, row, parent, x, y, z, yaw);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `type` | `u8` | The effect's sprite (`FX_xx`, a room define). |
| `row` | `u8` | The animation row byte (`FX_ROW_xx`): the low three bits pick the row of the sprite's animation, the bits above are a tint level. |
| `parent` | `u8` | What it is attached to: `FX_AT_WORLD` (0), `FX_AT_PLAYER` (1), an enemy (`FX_AT_ENEMY_xx`, a room define: 2 plus the enemy slot), or an object (`FX_AT_OBJ \| OBJ_xx`). |
| `x`, `y`, `z` | `i16` | The position, relative to the parent (negative y is up). |
| `yaw` | `i16` | Added to the yaw the animation's own data gives. |

## Behaviour

`effect` looks the sprite up in the room's own sprite table first, then in
the game's weapon effects. A type neither declares is skipped, with one
warning per room (the original would read a null pointer here).

The effect takes one slot of the 64-slot effect pool for each frame of the
row, initialised from that frame's first behaviour block; when the pool
fills, the rest of the row is dropped. Each slot records the type, the
whole row byte, the parent and the position as a 16-bit offset, so the
effect follows a moving parent. Its brightness is the animation data's
own. What the effect then does (moves, loops, ends, spawns others) is the
behaviour its data names, run each frame by the effect system.

A room script's effect always plays, including the blood types the
no-blood option hides when the engine spawns them.

The `.s` assembler prints the instruction as raw bytes; `.akscr` takes one
argument per field.

## Example

```c
run
{
    effect(FX_08, FX_ROW_14, FX_AT_OBJ | OBJ_00_CEILING, -3060, 20, -3010, 0);
    effect(FX_08, FX_ROW_14, FX_AT_OBJ | OBJ_00_CEILING, -3060, 20, 3140, 0);
    effect(FX_08, FX_ROW_14, FX_AT_OBJ | OBJ_00_CEILING, 2940, 20, 3140, 0);
    effect(FX_08, FX_ROW_14, FX_AT_OBJ | OBJ_00_CEILING, 2940, 20, -3010, 0);
}
```

Room 115 (`games/re1/room/115/room.akscr`) spawns one effect at each
corner of the falling ceiling, attached to the ceiling object so they
move down with it.

## See also

- [`effect_tracked`](effect_tracked.md): the same, remembered for `effect_kill_a`.
- [`effect_kill_b`](effect_kill_b.md): removes the effects of a type and row.
- [`effect_clear`](effect_clear.md): removes every effect.
- [`mass_mask`](mass_mask.md): changes the flags of every live effect.
