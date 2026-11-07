# `bank_bit`

Hides a mask sprite group when the next camera comes on screen; 255 clears the groups to hide.

| | |
| --- | --- |
| Opcode | `0x49` |
| Group | Camera, sprites and light |
| Kind | Command |
| Size | 2 bytes |

## Syntax

```c
bank_bit(group);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `group` | `u8` | The mask sprite group to hide (1 to 15), or 255 to clear the pending set. The compiler has no names for it; write the number. |

## Behaviour

`bank_bit` adds a group to a pending set of groups to hide. Nothing changes
on screen at once: the set is applied the next time a camera's sprite
table is rebuilt, that is when a camera goes on screen (a room load, a
switch zone, [`cutnext`](cutnext.md) or [`cutcurr`](cutcurr.md)). The new
table starts with every group shown, the groups in the set are hidden, and
the set is emptied.

`bank_bit(255)` empties the set, so scripts write it first to drop what an
earlier pass left. Only groups 1 to 15 take effect; 0 and 16 to 31 are
recorded but do nothing, and other values wrap to one of those (the low
five bits are used).

Because the set is applied when a camera comes on, it works from a room's
`init`, where [`aot_switch`](aot_switch.md) does not.

The original's handler is `room_sprite_hide`; `bank_bit` is the original
tool's name.

## Example

```c
if (cmpb(B_CAMERA, CMP_EQ, CAM_02) && ck(FG_SCENARIO, F_SCENARIO_LIBRARY_SWITCH_ON, FALSE))
{
    bank_bit(255);
    bank_bit(7);
}
if (cmpb(B_CAMERA, CMP_EQ, CAM_04_STATUE_SPOT) && ck(FG_SCENARIO, F_SCENARIO_LIBRARY_SWITCH_ON, FALSE))
{
    bank_bit(255);
    bank_bit(4);
}
```

Room 217 (`games/re1/room/217/room.akscr`) hides one group on each of two
cameras until the library's switch is pressed.

## See also

- [`aot_switch`](aot_switch.md): shows or hides a group on the camera on screen.
