# `sce_parts_down`

Makes the task's zombie drop one of its parts, which falls and comes to rest.

| | |
| --- | --- |
| Opcode | `0x7B` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 16 bytes |
| Status | yes |

## Syntax

```c
sce_parts_down(part, angle, along, up, gravity, 0, x, y, z);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `part` | `u8` | The part that drops. |
| `angle` | `i16` | The direction it is thrown in, 4096 to a turn. |
| `along` | `i16` | Its speed along that direction. |
| `up` | `i16` | Its speed upwards. |
| `gravity` | `u8` | The pull down on it each tick, read as a signed byte. |
| `_` | `u8` | Padding, not read. Write 0. |
| `x` | `i16` | The rotation it comes to rest with, about x. |
| `y` | `i16` | About y. |
| `z` | `i16` | About z. |

## Behaviour

On a character that the port runs as a zombie (see
[`sce_em_set`](sce_em_set.md)), the drop is queued for the zombie's next
tick, which starts it from where the part was on the last tick's pose: the
part leaves the body and is thrown, as the original's `Parts_down` throws it,
then falls with `gravity` and rests turned as given.

On any other work (the player, a scene character that is not a zombie, an
object or a work the engine does not model) it does nothing, silently.

## Example

```c
proc main_1D
{
    work_set(3, 1);
    sce_parts_down(4, 2048, 20, 40, 4, 0, 0, 1024, 0);
    evt_end(0);
}
```

Illustrative: the zombie in enemy slot 1 drops part 4, thrown back and up.
The values are placeholders.

## See also

- [`sce_parts_bomb`](sce_parts_bomb.md): a part bursts off.
- [`parts_set`](parts_set.md): shows and hides a part.
