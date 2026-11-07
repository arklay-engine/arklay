# `model_op`

Changes a model's colour in the original; does nothing in the port.

| | |
| --- | --- |
| Opcode | `0x34` |
| Group | Objects and models |
| Kind | Command |
| Size | 8 bytes |

## Syntax

```c
model_op(form, target, a, b, c, d, e);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `form` | `u8` | The form (`MODEL_OP_xx`). The shipped scripts use `MODEL_OP_00`. |
| `target` | `u8` | The model it acts on. |
| `a` to `e` | `u8` | Five form-specific bytes, such as colour steps. Their meaning is not documented. |

## Behaviour

In the original, form 0 steps the colour of a model in its texture queue,
and forms 1 and 2 retarget a queue entry. The form-0 handler first takes
`0x80` from the target and looks for a queue entry with that id; the queue
holds enemy model ids, which have bit 7 clear, while every target the
shipped scripts pass gives an id with bit 7 set. So no entry ever matches
and the original changes nothing. The port does not model the texture
queue and does nothing for every form, which gives the same result.

The `.s` assembler prints the instruction as raw bytes; `.akscr` takes one
argument per field.

## Example

```c
for (40)
{
    model_op(MODEL_OP_00, 0, 1, 1, 255, 255, 255);
    sleep(4);
}
```

Room 20b (`games/re1/room/20b/room.akscr`) steps a model's colour forty
times while the stove is lit; in practice nothing changes.

## See also

- [`objs_hide`](objs_hide.md): tints the player.
