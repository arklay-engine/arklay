# `cut_auto`

Holds the camera where it is, or lets the player's position switch it again.

| | |
| --- | --- |
| Opcode | `0x3C` |
| Group | Camera, room and screen |
| Kind | Instruction |
| Size | 2 bytes |
| Status | yes |

## Syntax

```c
cut_auto(on);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `on` | `u8` | `0` holds the camera; any other value releases it. |

## Behaviour

Sets the camera hold that [`cut_chg`](cut_chg.md) sets, without switching
the camera. With `on` not 0 the hold is released and the camera on screen
stays until the player's position, checked against the room's camera
switches, chooses another. Unlike [`cut_old`](cut_old.md), it does not go back
to the remembered camera.

## Example

```c
proc main_26
{
    cut_auto(0);                 // keep this camera during the scene
    work_set(1, 0);
    plc_dest(0, 4, 45, 0, -5000);
    do
    {
        evt_next();
    }
    while (ck(5, 45, 0));
    plc_ret();
    cut_auto(1);
    evt_end(0);
}
```

Illustrative: the camera stays put while the player is walked across a
camera switch, and switching resumes afterwards.

## See also

- [`cut_chg`](cut_chg.md), [`cut_old`](cut_old.md): switch and hold, and go back.
