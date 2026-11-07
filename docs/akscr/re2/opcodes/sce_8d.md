# `sce_8d`

An unnamed 24-byte instruction (not modelled).

| | |
| --- | --- |
| Opcode | `0x8D` |
| Group | Unnamed and unused |
| Kind | Instruction |
| Size | 24 bytes |
| Status | no |

## Syntax

```c
sce_8d(f0, w0, w1, w2, w3, w4, w5, w6, w7, w8, f1, f2, f3, f4);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `f0` | `u8` | Not read by the port. Not known. |
| `w0` | `i16` | Not read by the port. Not known. |
| `w1` | `i16` | Not read by the port. Not known. |
| `w2` | `i16` | Not read by the port. Not known. |
| `w3` | `i16` | Not read by the port. Not known. |
| `w4` | `i16` | Not read by the port. Not known. |
| `w5` | `i16` | Not read by the port. Not known. |
| `w6` | `i16` | Not read by the port. Not known. |
| `w7` | `i16` | Not read by the port. Not known. |
| `w8` | `u16` | Not read by the port. Not known. |
| `f1` | `u8` | Not read by the port. Not known. |
| `f2` | `u8` | Not read by the port. Not known. |
| `f3` | `u8` | Not read by the port. Not known. |
| `f4` | `u8` | Not read by the port. Not known. |

## Behaviour

Not modelled: the instruction does nothing, and the log notes
`RE2 script: sce_8d is not modelled` once per run.

The opcode has no name in biohazard-utils and none from the original's
handler, so its mnemonic is its number. What the handler does has not been
looked at, and the operands' meanings are not known.

## Example

```c
proc main_44
{
    sce_8d(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);
    evt_end(0);
}
```

Illustrative: the instruction's form only.
