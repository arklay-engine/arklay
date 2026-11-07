# `costume_set`

Picks which alternate costume the player wears.

| | |
| --- | --- |
| Opcode | `0x4F` |
| Group | The player |
| Kind | Command |
| Size | 2 bytes |

## Syntax

```c
costume_set(costume);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `costume` | `u8` | The alternate costume (`COSTUME_xx`); only its low bit is kept. |

## Behaviour

`costume_set` stores the low bit of `costume` as the costume bit, the
original's one-byte `g_bCostumeVariant`: `COSTUME_00` and `COSTUME_01`
pick the first and the second alternate outfit. It changes nothing on
screen by itself. The alternate model loads with the player's model on a
room load, and only while the player's alternate-costume status flag is
set: it is `EM1030` plus the character plus twice the bit (`EM1030` to
`EM1033`), with the character's own animations.

[`costume_ck`](costume_ck.md) reads the bit back.

## Example

```c
setb(B_CHARACTER_MODEL, 9, 0);
set(FG_SCENARIO, F_SCENARIO_ALT_OUTFIT, TRUE);
costume_set(COSTUME_01);
evt_exec(0, TASK_09, change_outfit);
```

Room 11c (`games/re1/room/11c/room.akscr`), the wardrobe: the player
changes into the second alternate outfit, and an event plays the change.

## See also

- [`costume_ck`](costume_ck.md): tests the costume bit.
