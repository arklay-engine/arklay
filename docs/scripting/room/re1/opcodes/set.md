# `set`

Sets, clears or toggles one bit of a flag group.

| | |
| --- | --- |
| Opcode | `0x05` |
| Group | Flags and state |
| Kind | Command |
| Size | 4 bytes |

## Syntax

```c
set(group, bit, mode);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `group` | `u8` | The flag group: `FG_SCENARIO`, `FG_COMMON`, `FG_LOCK`, `FG_ENEMY`, `FG_ROOM`, `FG_STATUS`, `FG_RUN`, `FG_ITEM`, `FG_MAP` or `FG_9` (see [Values](../index.md#values)). |
| `bit` | `u8` | The bit within the group, usually an `F_<group>_*` name from `inc/flags.akinc` or a room's own `F_ROOM_*` define. |
| `mode` | `u8` | `TRUE` (0) sets the bit, `FALSE` (1) clears it, `TOGGLE` (2) flips it. |

## Behaviour

`set` changes the bit at once; a `ck` later in the same block sees the new
value. The modes are the engine's truth values, not C's: `TRUE` is 0.

A group is a run of 32-bit words, and `bit` selects a bit within it the way
the original does: the word is `bit / 32`, and the bit within the word
counts from the most significant end.

`FG_RUN` (group 6) is not a group of its own: it is the engine's run word
(the systems that update each frame, the `RUN_*` bits) and the word next to
it. Bits 16 to 31 address the run word, high bit first, and bits 0 to 15 the
word before it. A bit past 31 addresses the ordinary bank 6, as it does
for [`ck`](ck.md). `set(FG_RUN, F_RUN_CONTROLS,
FALSE)` is how a cutscene takes the controls away from the player.

A `mode` other than 0, 1 or 2, or a group the engine does not have, changes
nothing; the port counts the instruction as unhandled.

## Example

```c
if (ck(FG_ENEMY, F_ENEMY_101_HUNTER_0, TRUE) && ck(FG_ROOM, F_ROOM_C_SAVE_ON_KILL, TRUE))
{
    set(FG_ROOM, F_ROOM_C_SAVE_ON_KILL, FALSE);
    set(FG_COMMON, F_COMMON_C_REBECCA_UNDER_ATTACK, FALSE);
    set(FG_COMMON, F_COMMON_C_REBECCA_SAVED_TALK, TRUE);
}
```

Room 101 (`games/re1/room/101/room.akscr`): once the Hunter is dead, the
room clears its one-shot bit and records that Rebecca was saved.

## See also

- [`ck`](ck.md): tests a flag bit.
- [`setb`](setb.md) and [`setw`](setw.md): write a state byte or word.
