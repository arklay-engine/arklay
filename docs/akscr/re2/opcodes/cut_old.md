# `cut_old`

Goes back to the camera `cut_chg` switched from, and releases the hold.

| | |
| --- | --- |
| Opcode | `0x2A` |
| Group | Camera, room and screen |
| Kind | Instruction |
| Size | 1 byte |
| Status | yes |

## Syntax

```c
cut_old();
```

## Parameters

None.

## Behaviour

Switches at once to the camera that was on screen at the last
[`cut_chg`](cut_chg.md), and lets the player's position switch the camera
again. If the player has moved into another camera's zone in the meantime,
the next camera check switches there.

Without an earlier `cut_chg`, the remembered camera is whatever the last one
left, which may be from another room: always pair it with a `cut_chg`.

## Example

```c
proc main_25
{
    cut_chg(5);
    message_on(0, 2, 0, 0, 0);
    wsleep();
    wsleeping();
    cut_old();
    evt_end(0);
}
```

Illustrative: a close-up on camera 5 while a message shows, then back.

## See also

- [`cut_chg`](cut_chg.md): switches and holds.
- [`cut_auto`](cut_auto.md): releases the hold without switching.
