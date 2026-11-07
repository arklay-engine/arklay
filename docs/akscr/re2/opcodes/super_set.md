# `super_set`

By its handler's name, attaches something to a parent body, a "super" (not modelled).

| | |
| --- | --- |
| Opcode | `0x48` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 16 bytes |
| Status | no |

## Syntax

```c
super_set(f0, f1, f2, w0, w1, w2, w3, w4, w5);
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
`RE2 script: super_set is not modelled` once per run. Its name is the original handler's, `Super_set`. In RE 2 a "super" is the
body that something else follows: an interaction area's `super` byte, for
instance, makes the area move with a character or an object (see
[`aot_set`](aot_set.md)). What this handler attaches, and what its operands
are, has not been looked at, so the operand names above are placeholders.

Leon's rooms do not use it. Claire's room 101 has three, which the port
skips.

## Example

```c
proc main_1E
{
    super_set(0, 0, 0, 0, 0, 0, 0, 0, 0);
    evt_end(0);
}
```

Illustrative: the instruction's form only; the operands are placeholders.

## See also

- [`super_reset`](super_reset.md), [`super_on`](super_on.md): its companions, also not modelled.
- [`aot_set`](aot_set.md): an area whose `super` byte makes it follow a body.
