# `super_on`

Places the script's work at an offset from a parent body, once (not modelled).

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
| `f0` | `u8` | Not read by the original. Not read by the port. |
| `f1` | `u8` | The parent, as in [`super_set`](super_set.md): `0x00` the world, `0x40` the player, `0x80\|n` work n, `0xC0\|n` object n. Not read by the port. |
| `f2` | `u8` | The joint, as in [`super_set`](super_set.md): with bit 7 set, joint `f2 & 0x7F` of the player or work; otherwise its matrix. Not read by the port. |
| `w0` | `i16` | The offset's x. Not read by the port. |
| `w1` | `i16` | The offset's y. Not read by the port. |
| `w2` | `i16` | The offset's z. Not read by the port. |
| `w3` | `i16` | The offset's rotation x. Not read by the port. |
| `w4` | `i16` | The offset's rotation y. Not read by the port. |
| `w5` | `i16` | The offset's rotation z. Not read by the port. |

## Behaviour

Not modelled: the instruction does nothing, and the log notes
`RE2 script: super_on is not modelled` once per run.

The original's handler, `Super_on` (`0x4E5BC0`), takes the parent and joint
as [`super_set`](super_set.md) does, composes the offset and its rotation on
that parent's matrix, and writes the result's translation into the work's
position (`+0x38..+0x40`) once. It points the work's rotation (`+0x74..+0x78`)
along the composed matrix's x axis (the direction of (1000, 0, 0) through
it), adding `0x400` to the third component. Unlike `super_set` it doesn't
keep the parent, and it leaves the world matrix (`+0x54`, translation
`+0x68`) alone, so a [`super_reset`](super_reset.md) straight after it puts
the object back where it was last drawn (room 409's `main_04`).

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
