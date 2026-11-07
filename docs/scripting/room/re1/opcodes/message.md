# `message`

Shows a message in the message window.

| | |
| --- | --- |
| Opcode | `0x0B` |
| Group | Messages, voice and film |
| Kind | Command |
| Size | 4 bytes |

## Syntax

```c
message(id, pause);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `id` | `u8` (a `u16` word in `.akscr`) | The message id: a room message (`MSG_*` defined at the top of the room's script) or a global one (`MSG_*` from `inc/messages.akinc`), ORed with `MSG_FAST` to show the text at once. See [Message ids](../index.md#message-ids). |
| `pause` | `u16` | The `RUN_*` systems that stop while the window is open, ORed together; 0 stops none. |

## Behaviour

`message` asks the window to show the message. The id's low byte is the
original's: bit `0x80` (`MSG_FAST`) reveals the text at once instead of
letter by letter, bit `0x40` picks the global table, and the low six bits
are the index. A room message past 63 needs the id's high byte, so the
compiler writes [`message_long`](message_long.md) for an id word above 255
by itself; a global message has no index past 63 and such an id is an error.

A request while a message is already open is dropped, as in the original.
When the request is accepted, the bits of `pause` are cleared from the run
word (`FG_RUN`) until the window closes, and then the word is restored:
`RUN_PLAYER` freezes the player, `RUN_ENTITIES` the scripted characters,
`RUN_MONSTERS` the monsters, and so on. A message shown while a voice line
plays (a [`voice_play`](voice_play.md) just before) is marked as the line's
subtitle.

The instruction does not wait. An event that should wait for the window to
close follows it with `wait_message();`. A message with a yes/no choice
leaves the answer in the state byte `B_MENU_CHOICE`, for `cmpb`.

## Example

```c
cutnext(CAM_05_KENNETH_CLOSEUP);
set(FG_RUN, F_RUN_CONTROLS, FALSE);
set(FG_RUN, F_RUN_MONSTERS, FALSE);
message(MSG_HE_IS_KENNETH_FROM_THE_STARS | MSG_FAST, 0);
```

Room 104 (`games/re1/room/104/room.akscr`): the camera cuts to Kenneth's
body, the controls and monsters stop, and room message 0 is shown at once.

## See also

- [`message_long`](message_long.md): the form with an id word.
- [`voice_play`](voice_play.md): plays the voice line a message subtitles.
- [`cmpb`](cmpb.md): reads the menu choice.
