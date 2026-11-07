# `member_set`

Writes a constant into one field (member) of the task's work.

| | |
| --- | --- |
| Opcode | `0x34` |
| Group | Characters, objects and the work |
| Kind | Instruction |
| Size | 4 bytes |
| Status | yes |

## Syntax

```c
member_set(member, value);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `member` | `u8` | The field, by its number in the original's `Member_set_branch`. See [Members](#members). |
| `value` | `i16` | The value, sign-extended to the field's width. |

## Behaviour

Writes `value` into member `member` of the work chosen with
[`work_set`](work_set.md). A member the port has no field for, or a work the
engine does not model, is left alone: the instruction does nothing and logs
nothing.

Two writes have side effects:

- Writing a character's `Id` (member 6) to a zombie's id (`0x10` to `0x21`)
  turns it into a zombie that keeps its own model and animations, as Marvin
  does in room 202. A character that is already a zombie is not changed.
- Writing an object's `Be_flg` (members 0 and 1) also decides whether it is
  in play: it is when bit 0 is set and bit 19 (bit 3 of member 1) is clear.

The player's position (members 11 to 13) and facing (member 15) are kept and
applied after the tick's scripts, as [`pos_set`](pos_set.md)'s are. The other
writes are immediate.

### Members

The members the port models. Numbers are decimal, as a script writes them.

| Member | Field | Player | Character | Object |
| --- | --- | --- | --- | --- |
| 0 | `Be_flg`, low word (bit 0: in play) | yes | yes, the whole 32-bit word | yes |
| 1 | `Be_flg`, high word | yes | yes, a separate copy from member 0's | yes |
| 2 to 5 | `Routine_0` to `Routine_3`, the motion routine | yes | yes | |
| 6 | `Id`, the character's kind | | yes | |
| 7 | `Type`. On the player, reading gives the weapon in hand in the low 12 bits. | yes | yes | |
| 9 | The interaction area the character was last found in, 255 for none | | yes | |
| 11, 12, 13 | Position x, y, z | yes | yes | yes |
| 14, 15, 16 | Rotation about x, y, z | | | yes |
| 15 | Facing (`cdir.y`), 0 to 4095 | yes | yes | (rotation y) |
| 20, 21 | `Dest` x and z, a walk's target (also 25, 26) | yes | yes | |
| 22 | `Sce_flg`, the scene flags (see [`plc_flg`](plc_flg.md)) | yes | yes | |
| 23, 24 | `Sce_free0`, `Sce_free1` (see [`plc_rot`](plc_rot.md)) | yes | yes | |
| 27 | `Spd` x, low byte only | yes | yes | |
| 28, 29, 30 | `Spd` x, y, z, the motion's speed | yes | yes | |
| 31 | `Hokan`, the animation blend | yes | yes | |
| 42 | A zombie's state flags | | zombies only | |

## Example

```c
proc main_0A
{
    work_set(3, 2);
    member_set(6, 16);           // Id 0x10: the character becomes a zombie
    member_set(31, 0);
    evt_end(0);
}
```

Illustrative: it turns the character in enemy slot 2 into a zombie, as room
202 does with Marvin.

## See also

- [`member_set2`](member_set2.md): writes a member from a variable.
- [`member_copy`](member_copy.md): reads a member into a variable.
- [`member_cmp`](member_cmp.md): compares a member with a constant.
- [`member_calc`](member_calc.md): arithmetic on a member (not modelled).
