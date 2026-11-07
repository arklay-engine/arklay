# `splc_sce`

By its handler's name, puts the partner into a scripted motion (not modelled).

| | |
| --- | --- |
| Opcode | `0x71` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 1 byte |
| Status | no |

## Syntax

```c
splc_sce();
```

## Parameters

None.

## Behaviour

Not modelled: the instruction does nothing, and the log notes
`RE2 script: splc_sce is not modelled` once per run. Its name is the original handler's, `Splc_sce`; by the name, it puts the
second player, the partner, into its scenario routine. That has not been
checked.

To steer the partner in the port, choose it with
[`work_set`](work_set.md)`(2, 0)` and use the `plc_*` instructions, which
are modelled for it.

## Example

```c
proc main_23
{
    work_set(2, 0);
    plc_dest(0, 4, 44, -3000, 2500);
    evt_end(0);
}
```

Illustrative: the modelled way to move the partner.

## See also

- [`splc_ret`](splc_ret.md): its companion, also not modelled.
- [`plc_motion`](plc_motion.md), [`plc_dest`](plc_dest.md): scripted motions on the task's work.
