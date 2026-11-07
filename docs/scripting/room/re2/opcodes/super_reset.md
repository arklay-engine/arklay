# `super_reset`

By its handler's name, resets a parent-body ("super") link (not modelled).

| | |
| --- | --- |
| Opcode | `0x49` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 8 bytes |
| Status | no |

## Syntax

```c
super_reset(f0, w0, w1, w2);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `f0` | `u8` | Not read by the port. |
| `w0` | `i16` | Not read by the port. |
| `w1` | `i16` | Not read by the port. |
| `w2` | `i16` | Not read by the port. |

## Behaviour

Not modelled: the instruction does nothing, and the log notes
`RE2 script: super_reset is not modelled` once per run. Its name is the original handler's, `Super_reset`; it belongs with
[`super_set`](super_set.md). What it resets, and what its operands are, has
not been looked at.

## Example

```c
proc main_1F
{
    super_reset(0, 0, 0, 0);
    evt_end(0);
}
```

Illustrative: the instruction's form only; the operands are placeholders.

## See also

- [`super_set`](super_set.md), [`super_on`](super_on.md): its companions, also not modelled.
