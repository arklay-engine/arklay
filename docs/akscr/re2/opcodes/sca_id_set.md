# `sca_id_set`

Changes the blocking bits of one of the room's collision records.

| | |
| --- | --- |
| Opcode | `0x37` |
| Group | Camera, room and screen |
| Kind | Instruction |
| Size | 4 bytes |
| Status | yes |

## Syntax

```c
sca_id_set(index, id);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `index` | `u8` | The collision (SCA) record, from 0. |
| `id` | `u16` | The record's new id. Only bits 7 to 15 are used. |

## Behaviour

As the original's `Sca_id_set`: the record's id becomes
`(old & 0x7F) | (id & 0xFF80)`. The low seven bits, which give the record's
shape, stay; the upper bits, which say what the record blocks, are
replaced.

The change is made after the tick's pass of the scripts, and lasts until the
room is loaded again. An `index` past the room's records does nothing.

## Example

```c
proc main_27
{
    sca_id_set(6, 0);            // record 6 takes new blocking bits
    set(5, 12, 1);
    evt_end(0);
}
```

Illustrative: once something blocking a passage is moved, its collision
record is given other blocking bits (here none). Take the values from the
room's own scripts: which bits block what is not documented here.

## See also

- [`col_chg_set`](col_chg_set.md), [`flr_set`](flr_set.md): other room-data changes, not modelled.
