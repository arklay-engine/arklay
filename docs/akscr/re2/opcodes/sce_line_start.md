# `sce_line_start`

By its handler's name, starts a scripted line effect (not modelled).

| | |
| --- | --- |
| Opcode | `0x77` |
| Group | Camera, room and screen |
| Kind | Instruction |
| Size | 4 bytes |
| Status | no |

## Syntax

```c
sce_line_start(f0, w0);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `f0` | `u8` | Not read by the port. Not known. |
| `w0` | `u16` | Not read by the port. Not known. |

## Behaviour

Not modelled: the instruction does nothing, and the log notes
`RE2 script: sce_line_start is not modelled` once per run.

Its name is the original handler's, `Sce_line_start`. With
[`sce_line_main`](sce_line_main.md) and [`sce_line_end`](sce_line_end.md) it
makes a group whose effect has not been looked at.

## Example

```c
proc main_30
{
    sce_line_start(0, 0);
    for (30)
    {
        sce_line_main(0, 0, 0);
        evt_next();
    }
    sce_line_end();
    evt_end(0);
}
```

Illustrative: the three instructions in the order their names suggest. The
operands are placeholders.

## See also

- [`sce_line_main`](sce_line_main.md), [`sce_line_end`](sce_line_end.md): the rest of the group.
