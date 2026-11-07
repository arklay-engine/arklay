# `parts_set`

Changes one part of the task's character or the player, modelled only for case 2, which shows and hides the part.

| | |
| --- | --- |
| Opcode | `0x6E` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 6 bytes |
| Status | partial |

## Syntax

```c
parts_set(0, part, case, value);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `_` | `u8` | Padding, not read. Write 0. |
| `part` | `u8` | The part (joint) of the model, from 0. |
| `case` | `u8` | What to change. Only `2`, the part's `Be_flg`, is modelled. |
| `value` | `i16` | The new value. For case 2, bit 0 set draws the part and clear hides it. |

## Behaviour

**Case 2** writes the part's `Be_flg`. The port keeps only its bit 0: clear,
the part is not drawn; set, it is drawn again. It works on the player and on
a character (kinds 1 to 3 of [`work_set`](work_set.md)), for parts 0 to 31.
On any other work it does nothing, silently.

**Other cases** do nothing and log `RE2 script: parts_set is not modelled`
once per run. In the original they change a part's colour, attribute or
rotation, as rooms 106 and 112 do in loops; those rooms' effects are not
seen in the port.

## Example

```c
proc main_1B
{
    work_set(3, 0);
    parts_set(0, 9, 2, 0);       // hide part 9
    sleep(10, 30);
    parts_set(0, 9, 2, 1);       // and show it again
    evt_end(0);
}
```

Illustrative: part 9 of the character in enemy slot 0 disappears for a
second.

## See also

- [`sce_parts_bomb`](sce_parts_bomb.md): a part bursts off.
- [`sce_parts_down`](sce_parts_down.md): a zombie drops a part.
