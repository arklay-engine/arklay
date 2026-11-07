# `mass_mask`

Sets, clears or flips flags on every live effect.

| | |
| --- | --- |
| Opcode | `0x4E` |
| Group | Effects |
| Kind | Command |
| Size | 4 bytes |

## Syntax

```c
mass_mask(mode, flags);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `mode` | `u8` | `MASK_OR` (0) sets the bits, `MASK_CLEAR` (1) clears them, `MASK_XOR` (2) flips them. |
| `flags` | `u16` | The bits of the effect flags word (`FXF_xxxx`). |

## Behaviour

`mass_mask` applies `flags` to the flags word of every live effect slot,
the word in the effect's animation header. Effects spawned afterwards
start from their own data's flags. Any other mode does nothing.

The only bit the shipped scripts use is `FXF_8000`, the hidden bit: an
effect with it set goes on animating and moving but is not drawn. The
other bits drive the effects' own behaviours and have no meaning-based
names.

The `.s` assembler prints the instruction as raw bytes; `.akscr` takes one
argument per field.

## Example

```c
event rocket_launcher_drop
{
    sleep(10);
    run
    {
        mass_mask(MASK_OR, FXF_8000);
        set(FG_RUN, F_RUN_PLAYER, FALSE);
        set(FG_RUN, F_RUN_ENTITIES, FALSE);
        set(FG_RUN, F_RUN_MONSTERS, FALSE);
        // ...
    }
    // ...
}
```

Room 303 (`games/re1/room/303/room.akscr`) hides every effect for the
scene in which the rocket launcher is dropped, and later clears the bit
with `mass_mask(MASK_CLEAR, FXF_8000)` to show them again.

## See also

- [`effect`](effect.md): spawns an effect.
- [`effect_clear`](effect_clear.md): removes every effect.
