# `mirror_set`

By its handler's name, `Mirror_set`, sets up a mirror's reflection (not modelled).

| | |
| --- | --- |
| Opcode | `0x73` |
| Group | Camera, room and screen |
| Kind | Instruction |
| Size | 8 bytes |
| Status | no |

## Syntax

```c
mirror_set(f0, w0, w1, w2);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `f0` | `u8` | Not read by the port. Not known. |
| `w0` | `u16` | Not read by the port. Not known. |
| `w1` | `u16` | Not read by the port. Not known. |
| `w2` | `u16` | Not read by the port. Not known. |

## Behaviour

Not modelled: the instruction does nothing, and the log notes
`RE2 script: mirror_set is not modelled` once per run.

Its name is the original handler's; what it sets, and what its operands
are, has not been looked at. A room that uses it shows no reflection in the
port.

## Example

```c
proc init_00
{
    mirror_set(0, 0, 0, 0);
    evt_end(0);
}
```

Illustrative: the instruction's form only.
