# `message_on`

Shows one of the room's messages.

| | |
| --- | --- |
| Opcode | `0x2B` |
| Group | Messages, sound and music |
| Kind | Instruction |
| Size | 6 bytes |
| Status | yes |

## Syntax

```c
message_on(0, message, 0, stop_lo, stop_hi);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `_` | `u8` | Not read. Write 0. |
| `message` | `u8` | The room message to show. |
| `_` | `u8` | Not read. Write 0. |
| `stop_lo`, `stop_hi` | `u8` | Together, a 16-bit word of `fg_stop` bits held while the message shows: `0xFF, 0xFF` for a check in play, `0, 0` in a scene. |

## Behaviour

`message_on` opens the message window with room message `message`, the
message of that number in the room's own text. The window stays until the
player dismisses it, and a question's window until the player answers; the
answer is then bank 11 bit 31, set for "no".

The stop word goes into the top half of `fg_stop` (bank 2's first word) while
the message shows, and the old value comes back when it closes. Its bit
`0x8000` is bank 2 bit 0, down to `0x0001` for bank 2 bit 15. `0xFFFF` holds
the player, the enemies, the player's input and the scripts themselves (bank
2 bit 6) until the message is dismissed, as a check in play does. In a scene
the script holds the player already, so the word is 0 and the task goes on;
it then usually waits with [`wsleep`](wsleep.md) and
[`wsleeping`](wsleeping.md). A stop word of 0, or one given while another
message's stop is still held, changes nothing.

`message_on` does not wait by itself: the task goes on with the next
instruction, unless the stop word stops the scripts.

## Example

```c
proc main_03
{
    // An event area on a statue, checked with the action button.
    if (ck(5, 11, 0))
    {
        message_on(0, 2, 0, 0xFF, 0xFF);
    }
    else
    {
        message_on(0, 3, 0, 0xFF, 0xFF);
    }
    evt_end(0);
}
```

Illustrative: a check in play that shows room message 2 or 3 depending on
room bit 11, holding the game while it shows.

## See also

- [`wsleep`](wsleep.md), [`wsleeping`](wsleeping.md): wait for the message to close.
- [`aot_set`](aot_set.md): a message area (kind 4) shows a message without a procedure.
- [`xa_on`](xa_on.md): plays a voice line.
