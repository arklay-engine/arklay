# `parts_set`

Changes one field of one part of the task's character or the player: its colour, or whether it is drawn.

| | |
| --- | --- |
| Opcode | `0x6E` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 6 bytes |
| Status | yes |

## Syntax

```c
parts_set(0, part, case, value);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `_` | `u8` | Padding, not read. Write 0. |
| `part` | `i8` | The part (joint) of the model, from 0. The original reads it as a signed byte. |
| `case` | `u8` | Which field of the part's `PARTS_W` to write (below). |
| `value` | `i16` | The new value. |

## Behaviour

`Parts_set` (`0x4E5F60`) writes into the task's work's parts
(`pSin_parts_ptr`, `0xAC` bytes a part):

| Case | Field | In the port |
| --- | --- | --- |
| 0 | `Poly_rgb`'s first two bytes: red and green | the part's colour |
| 1 | `Poly_rgb`'s third byte: blue | the part's colour |
| 2 | `Be_flg`'s low word | bit 0: clear hides the part, set shows it |
| 3 | `Be_flg`'s high word | nothing: no draw reads it |
| 4, 5, 6 | `Cdir_x`, `Cdir_y`, `Cdir_z` | nothing: the animated path builds a part's matrix from its `Rot_x/y/z` (`J_seq2`) and never reads them |
| other | nothing | nothing |

**The colour.** Every part starts at `0x808080` (`PartsWork_link`). The draw
multiplies each vertex's light by the part's colour, `>> 8`, so `0x80` is a
channel at its usual light and `0` is black. In room 702 the original writes
`0x0505` for case 0 and `5` for case 1, whatever the value. Room 201's licker
fades to black this way, and rooms 106, 112, 60D and 702 use cases 0 and 1
too. No room script uses cases 3 to 6.

**Hiding.** A part whose `Be_flg` bit 0 is clear is neither drawn nor
transformed by `Joint_trans2`: its matrix stays where the last tick left it,
and so do the things that hang from it, such as a licker's shadow and drool.
The original writes the whole low word, which also clears the part's other
flags; the port keeps only bit 0.

It works on the player and on a character (kinds 1 to 3 of
[`work_set`](work_set.md)), for parts 0 to 31. A negative part, one past 31,
or any other work does nothing (the original writes out of bounds).

## Example

```c
proc main_1B
{
    work_set(3, 0);
    parts_set(0, 9, 0, 0x4040); // part 9's red and green at half light
    parts_set(0, 9, 1, 0x40);   // and its blue
    sleep(10, 30);
    parts_set(0, 9, 2, 0);      // hide part 9
    evt_end(0);
}
```

Illustrative: part 9 of the character in enemy slot 0 darkens, then
disappears a second later.

## See also

- [`sce_parts_bomb`](sce_parts_bomb.md): a part bursts off.
- [`sce_parts_down`](sce_parts_down.md): a zombie drops a part.
