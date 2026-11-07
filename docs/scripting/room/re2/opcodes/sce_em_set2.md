# `sce_em_set2`

A second form of `sce_em_set`, two bytes longer (not modelled).

| | |
| --- | --- |
| Opcode | `0x8E` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 24 bytes |
| Status | no |

## Syntax

```c
sce_em_set2(0, slot, id, type_lo, type_hi, floor, sound_type, model_type, dead_flag, x, y, z, dir, start, start_flags, extra0, extra1);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `_` | `u8` | Not read by the port. |
| `slot` | `u8` | Not read by the port. |
| `id` | `u8` | Not read by the port. |
| `type_lo` | `u8` | Not read by the port. |
| `type_hi` | `u8` | Not read by the port. |
| `floor` | `u8` | Not read by the port. |
| `sound_type` | `u8` | Not read by the port. |
| `model_type` | `u8` | Not read by the port. |
| `dead_flag` | `u8` | Not read by the port. |
| `x` | `i16` | Not read by the port. |
| `y` | `i16` | Not read by the port. |
| `z` | `i16` | Not read by the port. |
| `dir` | `i16` | Not read by the port. |
| `start` | `u16` | Not read by the port. |
| `start_flags` | `u16` | Not read by the port. |
| `extra0` | `u8` | Not read by the port. Not known. |
| `extra1` | `u8` | Not read by the port. Not known. |

## Behaviour

Not modelled: the instruction does nothing, and the log notes
`RE2 script: sce_em_set2 is not modelled` once per run, so the character it
would place is not there. Its layout is [`sce_em_set`](sce_em_set.md)'s with
two more bytes, and the names above assume the shared fields mean the same;
that, and what the two extra bytes do, has not been checked against the
original's handler.

## Example

```c
proc init_01
{
    sce_em_set2(0, 1, 16, 0, 0, 0, 1, 0, 13, 4200, 0, -2600, 0, 0, 0, 0, 0);
    evt_end(0);
}
```

Illustrative: the instruction's form only.

## See also

- [`sce_em_set`](sce_em_set.md): the modelled form.
