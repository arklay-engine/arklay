# `super_on`

By its handler's name, turns on a parent-body ("super") link (not modelled).

| | |
| --- | --- |
| Opcode | `0x72` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 16 bytes |
| Status | no |

## Syntax

```c
super_on(f0, f1, f2, w0, w1, w2, w3, w4, w5);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `f0` | `u8` | Not read by the port. |
| `f1` | `u8` | Not read by the port. |
| `f2` | `u8` | Not read by the port. |
| `w0` | `i16` | Not read by the port. |
| `w1` | `i16` | Not read by the port. |
| `w2` | `i16` | Not read by the port. |
| `w3` | `i16` | Not read by the port. |
| `w4` | `i16` | Not read by the port. |
| `w5` | `i16` | Not read by the port. |

## Behaviour

Not modelled: the instruction does nothing, and the log notes
`RE2 script: super_on is not modelled` once per run. Its name is the original handler's, `Super_on`, and its layout is
[`super_set`](super_set.md)'s. What it does, and what its operands are, has
not been looked at.

## Example

```c
proc main_20
{
    super_on(0, 0, 0, 0, 0, 0, 0, 0, 0);
    evt_end(0);
}
```

Illustrative: the instruction's form only; the operands are placeholders.

## See also

- [`super_set`](super_set.md), [`super_reset`](super_reset.md): its companions, which are modelled.
