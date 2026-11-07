# `splc_ret`

By its handler's name, hands the partner back from a scripted motion (not modelled).

| | |
| --- | --- |
| Opcode | `0x70` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 1 byte |
| Status | no |

## Syntax

```c
splc_ret();
```

## Parameters

None.

## Behaviour

Not modelled: the instruction does nothing, and the log notes
`RE2 script: splc_ret is not modelled` once per run. Its name is the original handler's, `Splc_ret`: by the pattern of
`Plc_ret`, the same for the second player, the partner. That has not been
checked.

To hand back the partner in the port, choose it with
[`work_set`](work_set.md)`(2, 0)` and use [`plc_ret`](plc_ret.md), which is
modelled for it.

## Example

```c
proc main_22
{
    work_set(2, 0);              // the partner
    plc_ret();                   // modelled, in place of splc_ret
    evt_end(0);
}
```

Illustrative: the modelled way to do what the name suggests.

## See also

- [`plc_ret`](plc_ret.md): ends a scripted motion on the task's work.
- [`splc_sce`](splc_sce.md): its companion, also not modelled.
