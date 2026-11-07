# `room_bgm_state_set`

Sets the music a room plays when it loads.

| | |
| --- | --- |
| Opcode | `0x37` |
| Group | Sound and music |
| Kind | Command |
| Size | 4 bytes |

## Syntax

```c
room_bgm_state_set(row, room, state);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `row` | `u8` | The row of the music table: the 0-based stage (0 to 4 for stages 1 to 5), or 5 and 6 for the second mansion visit of stages 1 and 2. |
| `room` | `u8` | The room within the stage, 0 to 31 (`ROOM_*`, the low five bits of a room id). |
| `state` | `u8` | The room's music state, below, or `BGM_OFF` (255) for no music. |

The state byte:

| Bits | Names | Meaning |
| --- | --- | --- |
| `0x07` | a number | The room's music group: which set of three tracks it loads. |
| `0x08` `0x10` `0x20` | `BGM_MAIN_ON` `BGM_SUB_1_ON` `BGM_SUB_2_ON` | The channels that start playing. |
| `0xC0` | `BGM_RELOAD` (`0x40`), `BGM_RESTART` (`0x80`) | How the room loads it: 0 loads and starts the tracks (keeping the playing ones when the group is the same); `BGM_RELOAD` always reloads them and starts nothing; `BGM_RESTART` always reloads them and starts them afresh. Both bits together is unused. |

## Behaviour

The save keeps a music state for every room, in a table of 7 rows of 32
rooms. `room_bgm_state_set` writes one entry. Nothing changes until that
room loads: on each room load or door transition the engine reads the
room's entry, fades out what plays when the group changes (or the state
says to reload), loads the group's tracks and starts the channels whose
bits are set. The room's init script runs before the entry is read, so a
room can set its own music in its init. A door transition reads the entry
once its animation is over, so the old room's music plays under the door.

The write is kept in the save, so it outlasts the room. Out-of-range rows
or rooms are ignored.

## Example

```c
room_bgm_state_set(5, 1, BGM_OFF);
// ...
if (ck(FG_COMMON, F_COMMON_C_REBECCA_KILLED, TRUE))
{
    room_bgm_state_set(5, 1, BGM_RELOAD);
    // ...
}
if (ck(FG_COMMON, F_COMMON_C_REBECCA_SAVED_TALK, TRUE))
{
    room_bgm_state_set(5, 1, BGM_RELOAD | 1);
    // ...
}
```

Room 101 (`games/re1/room/101/room.akscr`) sets its own second-visit music
(row 5, room 1) in its init, for the load under way: none, or group 0 or 1
loaded without starting, by how Rebecca's scene went.

## See also

- [`bgm_play`](bgm_play.md): starts a channel of the loaded tracks.
- [`bgm_stop`](bgm_stop.md): stops one.
