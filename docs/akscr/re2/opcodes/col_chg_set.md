# `col_chg_set`

A room-data change named after its handler, `Col_chg_set` (not modelled).

| | |
| --- | --- |
| Opcode | `0x45` |
| Group | Camera, room and screen |
| Kind | Instruction |
| Size | 5 bytes |
| Status | no |

## Syntax

```c
col_chg_set(f0, f1, f2, f3);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `f0` | `u8` | Not read by the port. Not known. |
| `f1` | `u8` | Not read by the port. Not known. |
| `f2` | `u8` | Not read by the port. Not known. |
| `f3` | `u8` | Not read by the port. Not known. |

## Behaviour

Not modelled: the instruction does nothing, and the log notes
`RE2 script: col_chg_set is not modelled` once per run. What the original's
handler changes has not been looked at.

## Example

```c
proc init_00
{
    col_chg_set(0, 0, 0, 0);
    evt_end(0);
}
```

Illustrative: the instruction's form only.

## See also

- [`sca_id_set`](sca_id_set.md): changes a collision record.
