# `sce_parts_bomb`

Bursts a part off the task's character, which the port models only by hiding the part.

| | |
| --- | --- |
| Opcode | `0x7A` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 16 bytes |
| Status | partial |

## Syntax

```c
sce_parts_bomb(0, part, f2, f3, f4, w0, w1, w2, w3, w4);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `_` | `u8` | Not read. Write 0. |
| `part` | `u8` | The part that bursts. |
| `f2` | `u8` | Not read by the port. |
| `f3` | `u8` | Not read by the port. |
| `f4` | `u8` | Not read by the port. |
| `w0` | `i16` | Not read by the port. |
| `w1` | `i16` | Not read by the port. |
| `w2` | `i16` | Not read by the port. |
| `w3` | `i16` | Not read by the port. |
| `w4` | `i16` | Not read by the port. |

## Behaviour

On a character (kinds 2 and 3 of [`work_set`](work_set.md)), part `part` is
hidden, as [`parts_set`](parts_set.md)`(0, part, 2, 0)` would hide it. On the
player, an object or a work the engine does not model it does nothing, and
nothing is logged.

The original's handler also starts `Parts_bomb`, which throws pieces of the
part about. The port does not draw them, and it does not read the operands
that shape them; their meanings are not known.

## Example

```c
proc main_1C
{
    work_set(3, 0);
    sce_parts_bomb(0, 1, 0, 0, 0, 0, 0, 0, 0, 0);
    evt_end(0);
}
```

Illustrative: part 1 (often the head) of the character in enemy slot 0
bursts. The other operands are placeholders.

## See also

- [`sce_parts_down`](sce_parts_down.md): a zombie drops a part, which falls and rests.
- [`parts_set`](parts_set.md): shows and hides a part.
