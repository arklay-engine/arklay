# `spd_add`

Adds a signed step to the player's floor height.

| | |
| --- | --- |
| Opcode | `0x45` |
| Group | Enemies and characters |
| Kind | Command |
| Size | 2 bytes |

## Syntax

```c
spd_add(delta);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `delta` | `i8` | The step, -128 to 127 (negative is up). |

## Behaviour

`spd_add` adds `delta` to the player's floor height, the reference the
shadow and the floor checks use (see [`spd_set`](spd_set.md)). It does not
move the player's model: a script that moves the player, such as a lift
ride, moves the model with a tween and steps the floor height alongside.
The sum wraps at 16 bits.

## Example

```c
for (39)
{
    spd_add(25);
    evt_next();
}
timer_setup(LIGHT_FADE_VEIL, 2176, FADE_R | FADE_G | FADE_B);
for (15)
{
    spd_add(25);
    evt_next();
}
```

Room 300 (`games/re1/room/300/room.akscr`) lowers the floor height 25 units
a frame while the lift carries the player down.

## See also

- [`spd_set`](spd_set.md): sets the floor height.
