# `plc_gun`

A weapon instruction for the task's work (not modelled).

| | |
| --- | --- |
| Opcode | `0x4A` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 2 bytes |
| Status | no |

## Syntax

```c
plc_gun(value);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `value` | `u8` | Not read by the port. Its meaning is not known. |

## Behaviour

Not modelled: the instruction does nothing, and the log notes
`RE2 script: plc_gun is not modelled` once per run. Its name is the
original's handler, `Plc_gun`; what the handler does has not been looked at.

## Example

```c
proc main_19
{
    work_set(1, 0);
    plc_gun(0);
    evt_end(0);
}
```

Illustrative: the instruction's form only. The operand is a placeholder.

## See also

- [`weapon_chg`](weapon_chg.md): changes the player's weapon.
- [`plc_motion`](plc_motion.md): a scripted motion.
