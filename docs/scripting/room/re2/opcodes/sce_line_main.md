# `sce_line_main`

By its handler's name, steps a scripted line effect (not modelled).

| | |
| --- | --- |
| Opcode | `0x78` |
| Group | Camera, room and screen |
| Kind | Instruction |
| Size | 6 bytes |
| Status | no |

## Syntax

```c
sce_line_main(f0, w0, w1);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `f0` | `u8` | Not read by the port. Not known. |
| `w0` | `i16` | Not read by the port. Not known. |
| `w1` | `i16` | Not read by the port. Not known. |

## Behaviour

Not modelled: the instruction does nothing, and the log notes
`RE2 script: sce_line_main is not modelled` once per run.

Its name is the original handler's, `Sce_line_main`, and it belongs with
[`sce_line_start`](sce_line_start.md) and [`sce_line_end`](sce_line_end.md).
What it does has not been looked at.

## Example

```c
proc main_31
{
    sce_line_main(0, 0, 0);
    evt_end(0);
}
```

Illustrative: the instruction's form only.

## See also

- [`sce_line_start`](sce_line_start.md), [`sce_line_end`](sce_line_end.md): the rest of the group.
