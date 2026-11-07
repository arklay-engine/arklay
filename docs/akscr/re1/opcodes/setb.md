# `setb`

Writes one state byte.

| | |
| --- | --- |
| Opcode | `0x08` |
| Group | Flags and state |
| Kind | Command |
| Size | 4 bytes |

## Syntax

```c
setb(byte, value, 0);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `byte` | `u8` | The state byte: a `B_*` name (`B_CAMERA`, `B_SELECTED_ITEM`, `B_CHARACTER_MODEL`, ...), or a placeholder such as `B_0C`. See [Engine names](../index.md#engine-names). |
| `value` | `u8` | The value to write. |
| *pad* | `u8` | Unused, written as 0. |

## Behaviour

The state bytes are the original's game-state block, the same bytes
[`cmpb`](cmpb.md) reads. `setb` stores the value, and a few bytes also move
the engine state that mirrors them:

- `B_CAMERA` (2) is the live camera: writing it makes that camera the
  current one. Unlike [`cutnext`](cutnext.md) it saves nothing to return to.
- `B_MENU_CHOICE` (5) is the message window's menu choice.
- `B_SELECTED_ITEM` (6) is the item the message text names; 0 clears it.
- `B_HEALTH_STATUS` (50) is mirrored into the player's health status.

An index past the 52-byte state block writes into the flag groups packed
after it, as the original's unchecked write does: 52 to 83 is `FG_COMMON`,
84 to 91 `FG_LOCK`, 92 to 123 `FG_ENEMY`, 124 to 155 `FG_ITEM`, 160 to 175
`FG_SCENARIO` and 176 to 195 `FG_MAP`. Some shipped rooms rely on this.

## Example

```c
evt_exec(0, TASK_09, rebecca_first_meeting);
cut_auto(CUT_LOCKED);
setb(B_CAMERA, CAM_02, 0);
```

Room 100 (`games/re1/room/100/room.akscr`): the init starts Rebecca's
scene, holds the camera and makes camera 2 the current one.

## See also

- [`cmpb`](cmpb.md): compares a state byte.
- [`setw`](setw.md): writes a state word.
- [`set`](set.md): changes a flag bit.
