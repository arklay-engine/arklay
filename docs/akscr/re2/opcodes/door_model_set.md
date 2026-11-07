# `door_model_set`

Not modelled: the original's door model set-up.

| | |
| --- | --- |
| Opcode | `0x4D` |
| Group | Interaction areas and doors |
| Kind | Instruction |
| Size | 22 bytes |
| Status | no |

## Syntax

```c
door_model_set(a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `a0` ... `a4` | `u8` | Five bytes. |
| `a5` | `u16` | A word. |
| `a6` ... `a9` | `i16` | Four signed words. |
| `a10`, `a11` | `u16` | Two words. |
| `a12`, `a13` | `u8` | Two bytes. |

The meaning of the fields is not known to the port; the widths follow
biohazard-utils' table, and every retail room script decodes with them.

## Behaviour

The port does not model `door_model_set`. It does nothing, and the first one
a run executes logs `RE2 script: door_model_set is not modelled`. The
instruction still compiles and round-trips exactly.

The name, from biohazard-utils, suggests that the original sets up the model
of a door that is drawn in the room (one that opens in place rather than
through the door animation); what the original does with each field is not
documented here.

## Example

```c
door_model_set(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);
```

Illustrative only: it shows the operand count. Copy real operands from the
decompiled `room.akscr` of a room that has one.

## See also

- [`door_aot_set`](door_aot_set.md): the doors the port models.
- [`obj_model_set`](obj_model_set.md): places a room object's model.
