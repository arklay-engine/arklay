# `sce_line_end`

By its handler's name, ends a scripted line effect (not modelled).

| | |
| --- | --- |
| Opcode | `0x79` |
| Group | Camera, room and screen |
| Kind | Instruction |
| Size | 1 byte |
| Status | no |

## Syntax

```c
sce_line_end();
```

## Parameters

None.

## Behaviour

Not modelled: the instruction does nothing, and the log notes
`RE2 script: sce_line_end is not modelled` once per run.

Its name is the original handler's, `Sce_line_end`, and it belongs with
[`sce_line_start`](sce_line_start.md) and [`sce_line_main`](sce_line_main.md).
What it does has not been looked at.

## Example

```c
proc main_32
{
    sce_line_end();
    evt_end(0);
}
```

Illustrative: the instruction's form only.

## See also

- [`sce_line_start`](sce_line_start.md), [`sce_line_main`](sce_line_main.md): the rest of the group.
