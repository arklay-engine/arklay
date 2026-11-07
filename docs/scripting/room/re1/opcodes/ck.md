# `ck`

Tests one bit of a flag group.

| | |
| --- | --- |
| Opcode | `0x04` |
| Group | Conditions |
| Kind | Condition |
| Size | 4 bytes |

## Syntax

```c
ck(group, bit, state);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `group` | `u8` | The flag group: `FG_SCENARIO`, `FG_COMMON`, `FG_LOCK`, `FG_ENEMY`, `FG_ROOM`, `FG_STATUS`, `FG_RUN`, `FG_ITEM`, `FG_MAP` or `FG_9` (0 to 9). |
| `bit` | `u8` | The bit in the group, usually an `F_<group>_*` name from `inc/flags.akinc` or one of the room's own `F_ROOM_*` defines. |
| `state` | `u8` | `TRUE` (0) passes when the bit is set, `FALSE` (1) when it is clear. |

## Behaviour

`ck` is a [condition](index.md#conditions): it stands in an `if` head, a
`require` or a `do`/`while` test, and nowhere else.

It passes when the bit is set and `state` is 0, or the bit is clear and
`state` is anything else (so `TOGGLE` tests as `FALSE`). Remember that the
engine's truth values are inverted from C's: `TRUE` is 0.

Each group is a bank of 32-bit words; bit `n` is bit `31 - (n & 31)` of word
`n >> 5`, as in the original. A group number past the last bank fails.
`FG_RUN` (6) is not a bank of its own in the original: bits 16 to 31 address
the word of systems that run each frame (the `RUN_*` bits, which `message`
pauses), and bits 0 to 15 the backup word beside it. Higher bits read the group's
ordinary bank.

A room variant build folds a `ck` that tells the variant (the player bit
`F_STATUS_PLAYER_JILL`, the release bits, and `F_SCENARIO_MANSION2` in a
mansion room) to its known result. See [Room variants](../index.md#room-variants).

## Example

```c
if (ck(FG_SCENARIO, F_SCENARIO_KENNETH_EXAMINED, TRUE))
{
    aot_delete(AOT_04_EVENT_KENNETH, SCE_NONE, SAT_PLAYER);
}
```

Room 104 (`games/re1/room/104/room.akscr`): once Kenneth's body has been
examined, his examine spot is switched off.

## See also

- [`set`](set.md): sets, clears or toggles a flag bit.
- [`cmpb`](cmpb.md): compares a state byte.
- [Include files](../index.md#include-files): the flag names.
