# `dir_set`

Turns the task's work to a rotation.

| | |
| --- | --- |
| Opcode | `0x33` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 8 bytes |
| Status | yes |

## Syntax

```c
dir_set(0, x, y, z);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `_` | `u8` | Padding, not read. Write 0. |
| `x` | `i16` | The rotation about x. Read only for an object. |
| `y` | `i16` | The rotation about y, the facing: 4096 is a whole turn. |
| `z` | `i16` | The rotation about z. Read only for an object. |

## Behaviour

Sets the rotation of the work chosen with [`work_set`](work_set.md):

- **A room object**: all three rotations, at once.
- **A character**: its facing becomes `y`, at once. `x` and `z` are not
  read.
- **The player**: the facing `y` is kept and applied after the tick's scripts
  have run, as [`pos_set`](pos_set.md)'s position is.
- **A work the engine does not model**: nothing happens, and the log notes
  `RE2 script: dir_set is not modelled` once per run.

## Example

```c
proc main_09
{
    work_set(3, 1);
    dir_set(0, 0, 3072, 0);      // three quarters of a turn
    evt_end(0);
}
```

Illustrative: it turns the character in enemy slot 1.

## See also

- [`pos_set`](pos_set.md): places the work.
- [`dir_ck`](dir_ck.md): tests which way the work faces.
- [`plc_dest`](plc_dest.md): walks a character to a point, turning it as it goes.
