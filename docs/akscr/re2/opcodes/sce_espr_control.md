# `sce_espr_control`

Not modelled: a control of running effect sprites.

| | |
| --- | --- |
| Opcode | `0x52` |
| Group | Effects |
| Kind | Instruction |
| Size | 6 bytes |
| Status | no |

## Syntax

```c
sce_espr_control(a0, a1, a2, a3, a4);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `a0` ... `a4` | `u8` | Five bytes, whose meaning is not known to the port. |

## Behaviour

The port does not model `sce_espr_control`. It does nothing, and the first
one a run executes logs `RE2 script: sce_espr_control is not modelled`. The
instruction compiles and round-trips exactly. By its name, the original
changes something about effects already running; what each byte does is not
documented here.

## Example

```c
sce_espr_control(0, 0, 0, 0, 0);
```

Illustrative only: it shows the operand count. Copy real operands from the
decompiled `room.akscr` of a room that has one.

## See also

- [`sce_espr_on`](sce_espr_on.md): starts an effect.
- [`sce_espr_kill`](sce_espr_kill.md): ends effects.
