# `flr_set`

Sets or clears a flag of one of the room's floor (footstep) records (not modelled).

| | |
| --- | --- |
| Opcode | `0x38` |
| Group | Camera, room and screen |
| Kind | Instruction |
| Size | 3 bytes |
| Status | no |

## Syntax

```c
flr_set(index, on);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `index` | `u8` | Not read by the port. In the original, the floor record. |
| `on` | `u8` | Not read by the port. In the original, whether to set or clear the bit. |

## Behaviour

Not modelled: the instruction does nothing, and the log notes
`RE2 script: flr_set is not modelled` once per run.

The original's handler (`0x4E8BE0`) sets or clears bit `0x80` of a record in
the room's floor table, the zones that choose the sound of footsteps. The
port reads the floor table for footsteps, but scripts cannot change it, so a
room that turns a floor zone on or off keeps the sounds it loaded with. No
room on the route from 100 to 200 uses `flr_set`. Which operand is which, as
named above, has not been checked.

## Example

```c
proc init_00
{
    flr_set(2, 1);
    evt_end(0);
}
```

Illustrative: the instruction's form only.

## See also

- [`sca_id_set`](sca_id_set.md): changes a collision record, which is modelled.
