# `cut_be_set`

Turns one of a camera's switch zones on or off.

| | |
| --- | --- |
| Opcode | `0x61` |
| Group | Camera, room and screen |
| Kind | Instruction |
| Size | 4 bytes |
| Status | yes |

## Syntax

```c
cut_be_set(cut, index, value);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `cut` | `u8` | The camera whose zones are changed. |
| `index` | `u8` | The zone, counted from the first record the room lists under camera `cut` (record 0). |
| `value` | `u8` | The zone's gate: `0` turns it off, anything else on. |

## Behaviour

Each camera has a list of switch zones in the room's camera-switch table
(`cameraSwitches` in `room.xml`). While camera `cut` is on screen, the
player stepping into one of its zones switches to the zone's camera, unless
the zone's gate is 0 or the zone is for another floor. `cut_be_set` writes
the gate of record `index` of camera `cut`'s list. Record 0 is the first one
listed under the camera, which the switch check does not test, so the zones
that switch are 1 and up.

The change is made after the tick's pass of the scripts, and lasts until the
room is loaded again. A camera with no records, or an `index` past the end
of the table, changes nothing.

## Example

```c
proc init_00
{
    if (ck(5, 10, 0))            // until the shutter is open
    {
        cut_be_set(2, 1, 0);     // camera 2's first zone does not switch
    }
    evt_end(0);
}
```

Illustrative: the room's init turns off a camera switch while a room flag is
clear.

## See also

- [`cut_replace`](cut_replace.md): swaps two cameras across the table.
- [`cut_chg`](cut_chg.md): switches the camera directly.
