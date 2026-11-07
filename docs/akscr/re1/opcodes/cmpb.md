# `cmpb`

Compares a state byte with a value.

| | |
| --- | --- |
| Opcode | `0x06` |
| Group | Conditions |
| Kind | Condition |
| Size | 4 bytes |

## Syntax

```c
cmpb(byte, comparison, value);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `byte` | `u8` | The state byte (`B_*`: `B_ROOM`, `B_CAMERA`, `B_PREV_ROOM`, `B_MENU_CHOICE`, `B_USED_ITEM` and the others in [Engine names](../index.md#engine-names); unnamed ones as `B_xx`). |
| `comparison` | `u8` | `CMP_EQ`, `CMP_GT`, `CMP_GE`, `CMP_LT`, `CMP_LE` or `CMP_NE` (0 to 5). |
| `value` | `u8` | The value to compare with (0 to 255). |

## Behaviour

`cmpb` is a [condition](index.md#conditions): it stands in an `if` head,
a `require` or a `do`/`while` test.

It passes when `state comparison value` holds, the state byte on the left:
`cmpb(B_CAMERA, CMP_GT, 3)` passes when the camera is past 3. Both sides
are unsigned bytes. A comparison past 5 always fails.

The byte indexes the original's state block (the "BioCard" bytes, 52 of
them). As in the original, which does not check the index, an index past
the block reads the flag banks that follow it: 52 to 83 the `FG_COMMON`
bytes, 84 to 91 `FG_LOCK`, 92 to 123 `FG_ENEMY`, 124 to 155 `FG_ITEM`, 156
to 159 the examined-message bytes, 160 to 175 `FG_SCENARIO` and 176 to 195
`FG_MAP`. Anything further reads as 0.

## Example

```c
if (cmpb(B_CAMERA, CMP_EQ, CAM_04_KENNETH))
{
    evt_exec(0, TASK_09, first_zombie_film);
}
```

Room 104 (`games/re1/room/104/room.akscr`): the first zombie film starts
when the camera reaches Kenneth's body.

## See also

- [`setb`](setb.md): writes a state byte.
- [`cmpw`](cmpw.md): compares a state word.
