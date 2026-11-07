# `plc_rot`

Writes one of the two scene words (`Sce_free0`, `Sce_free1`) of the task's character or the player.

| | |
| --- | --- |
| Opcode | `0x58` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 4 bytes |
| Status | yes |

## Syntax

```c
plc_rot(which, value);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `which` | `u8` | `0` writes `Sce_free0` (member 23), `1` writes `Sce_free1` (member 24). Anything else does nothing. |
| `value` | `u16` | The value, stored as a signed 16-bit word. |

## Behaviour

Sets the word at once. The scenario routine reads it: for the walks and
turns of [`plc_dest`](plc_dest.md), `Sce_free0` is the turn rate, which
`plc_dest` sets from the original's tables and `plc_rot` can then change.
What other routines read the words for is the routine's.

[`plc_motion`](plc_motion.md) clears both words, and `plc_dest` sets the
first and clears the second, so `plc_rot` comes after either.

It applies to the player and to a character. On a room object or a work the
engine does not model it does nothing, and the log notes
`RE2 script: plc_rot is not modelled` once per run.

## Example

```c
proc main_16
{
    work_set(3, 0);
    plc_dest(0, 9, 43, 1200, -800);   // turn on the spot to face the point
    plc_rot(0, 32);                   // turning 32 a tick
    evt_end(0);
}
```

Illustrative: the character in enemy slot 0 turns on the spot towards a
point at its own rate.

## See also

- [`plc_dest`](plc_dest.md): the walks that read `Sce_free0`.
- [`member_set`](member_set.md): `member_set(23, ...)` writes the same word.
