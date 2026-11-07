# `effect_tracked`

Spawns an effect, as `effect` does, and remembers its type and parent for `effect_kill_a`.

| | |
| --- | --- |
| Opcode | `0x3D` |
| Group | Effects |
| Kind | Command |
| Size | 12 bytes |

## Syntax

```c
effect_tracked(type, row, parent, x, y, z, yaw);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `type` | `u8` | The effect's sprite (`FX_xx`, a room define). |
| `row` | `u8` | The animation row byte (`FX_ROW_xx`): the low three bits pick the row, the bits above are a tint level. |
| `parent` | `u8` | What it is attached to: `FX_AT_WORLD`, `FX_AT_PLAYER`, `FX_AT_ENEMY_xx` or `FX_AT_OBJ \| OBJ_xx`, as for [`effect`](effect.md). |
| `x`, `y`, `z` | `u16` | The position, relative to the parent, read unsigned: 61536 is -4000. |
| `yaw` | `i16` | Added to the yaw the animation's own data gives. |

## Behaviour

`effect_tracked` spawns the effect exactly as [`effect`](effect.md) does,
then records its type and parent as the last tracked effect, which
[`effect_kill_a`](effect_kill_a.md) removes. Only one is remembered: a
second `effect_tracked` replaces the first. The record is cleared when
the room is left.

The original reads x, y and z unsigned here, where `effect` reads them
signed, so the shipped scripts write a height above the floor as, for
example, 61536. The effect is drawn at the 16-bit offset, which has the
same bits either way, so the two forms place the effect alike; the
unsigned value only reaches the full-precision spawn position the effect
code keeps alongside.

The `.s` assembler prints the instruction as raw bytes; `.akscr` takes one
argument per field.

## Example

```c
if (ck(FG_ITEM, F_ITEM_215_KEY_JEWEL_RED_2, TRUE))
{
    effect_tracked(FX_0C, FX_ROW_19, FX_AT_WORLD, 5200, 61536, 12100, 0);
}
```

```c
event lights_on
{
    run
    {
        if (ck(FG_ITEM, F_ITEM_215_KEY_JEWEL_RED_2, TRUE))
        {
            effect_kill_a(0);
        }
        set(FG_COMMON, F_COMMON_STUFFED_ROOM_LIGHTS_OFF, FALSE);
    }
    // ...
}
```

Room 215 (`games/re1/room/215/room.akscr`): with the lights off and the
red jewel still in place, the init spawns an effect on it (the
`lights_off` event does the same); when the lights come on, the event
removes it.

## See also

- [`effect_kill_a`](effect_kill_a.md): removes the tracked effect.
- [`effect`](effect.md): spawns an effect without tracking it.
