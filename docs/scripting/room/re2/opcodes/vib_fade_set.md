# `vib_fade_set`

By its name, fades a pad rumble on the console version (not modelled).

| | |
| --- | --- |
| Opcode | `0x8C` |
| Group | Items and the player's state |
| Kind | Instruction |
| Size | 8 bytes |
| Status | no |

## Syntax

```c
vib_fade_set(f0, w0, w1, w2);
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
`RE2 script: vib_fade_set is not modelled` once per run.

It belongs with [`vib_set0`](vib_set0.md) and [`vib_set1`](vib_set1.md),
whose PC handler does nothing. Whether the PC executable's `vib_fade_set`
handler does anything has not been checked, so the port logs it as not
modelled rather than treating it as a known no-op.

## Example

```c
proc main_3E
{
    vib_fade_set(0, 0, 60, 0);
    evt_end(0);
}
```

Illustrative: the instruction's form only.

## See also

- [`vib_set0`](vib_set0.md), [`vib_set1`](vib_set1.md): the rumbles, which do nothing on the PC.
