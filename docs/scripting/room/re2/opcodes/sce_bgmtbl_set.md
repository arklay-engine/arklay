# `sce_bgmtbl_set`

Changes the music the room music table gives a room.

| | |
| --- | --- |
| Opcode | `0x57` |
| Group | Messages, sound and music |
| Kind | Instruction |
| Size | 8 bytes |
| Status | yes |

## Syntax

```c
sce_bgmtbl_set(0, room, stage, low, high);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `_` | `u8` | Not read. Write 0. |
| `room` | `u8` | The room. |
| `stage` | `u8` | The stage, counted from 1. |
| `low` | `u16` | Or'd with `high` to give the table word. |
| `high` | `u16` | Or'd with `low`. |

## Behaviour

The room music table (`bgm_tbl`) has a 16-bit word for each room of the
game. Its low byte is the main music and its high byte the sub sequence: the
low six bits of each are the id, and bits 6 and 7 flags. `sce_bgmtbl_set`
writes `low | high` as the word of `room` in `stage`. A stage of 0, or a room
outside the table, is ignored.

The table is read when the player goes through a door, so the change is heard
from the next entry to that room. Saves keep the table, so a change lasts for
the rest of the game.

## Example

```c
sce_bgmtbl_set(0, 0x0C, 2, 0x0005, 0x2300);
```

Illustrative: from now on, room `0x0C` of stage 2 plays main music 5 with sub
sequence `0x23`. The ids are placeholders.

## See also

- [`sce_bgm_control`](sce_bgm_control.md): controls the music that is loaded.
