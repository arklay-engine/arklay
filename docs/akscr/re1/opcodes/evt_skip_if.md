# `evt_skip_if`

Waits while a voice line plays or a message is up; the second half of `wait_message();`.

| | |
| --- | --- |
| Opcode | `0xF7` |
| Group | Event: control |
| Kind | Event instruction |
| Size | 1 byte |
| Valid in | every state |

## Syntax

```c
wait_message();
```

## Parameters

None.

## Behaviour

Each frame it tests whether a voice line is playing (the voice-playing
status flag, `F_STATUS_VOICE_PLAYING`) or a message window is up (bit
`0x80` of the menu-choice byte, `B_MENU_CHOICE`). While either holds, the
slot yields and stays on `evt_skip_if`, to test again next frame.

When neither holds it releases the stack entry that
[`evt_push_cond`](evt_push_cond.md) took and moves past itself, but still
yields: the instruction after `wait_message();` runs in the next frame at
the earliest. A `wait_message()` therefore always costs at least one frame.

## Example

```c
sleep(30);
voice_play(VOICE_PLAY, VO_00C_01);
message(MSG_VO_00C_01, 0);
wait_message();
evt_work_set(WK_PLAYER, 0);
```

Room 10B (`games/re1/room/10b/room.akscr`), event `rebecca_radio_call`:
shows a line of the radio call and holds the event until its voice ends
and the message closes.

## See also

- [`evt_push_cond`](evt_push_cond.md): the first half.
- [`evt_do`](evt_do.md): a wait on any condition, written `do { evt_next(); } while (...)`.
