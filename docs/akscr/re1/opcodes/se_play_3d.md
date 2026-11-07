# `se_play_3d`

Plays a sound effect at a point, on the player or on an enemy.

| | |
| --- | --- |
| Opcode | `0x17` |
| Group | Sound and music |
| Kind | Command |
| Size | 10 bytes; 6 when `where` is past `SND_NO_POS` |

## Syntax

```c
se_play_3d(bank, sound, volume, where, enemy, x, z);
se_play_3d(bank, sound, volume, where, enemy);   // where > SND_NO_POS
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `bank` | `u8` | The sound bank: `SND_BANK_DOOR` (0, the room's door sound pair), `SND_BANK_WEAPON` (1), `SND_BANK_ROOM` (2, the room's own table), `SND_BANK_PLAYER` (3, the character's) or `SND_BANK_BGM` (4). |
| `sound` | `u8` | The sound within the bank (`SE_xx`, a room define). |
| `volume` | `i8` | Carried, but neither the original's 3D dispatch nor the port reads it. Written as 0 to 255 (the compiler takes no negative number here). |
| `where` | `u8` | Where the sound plays: `SND_AT_POINT` (0), `SND_AT_PLAYER` (1), `SND_AT_ENEMY` (2) or `SND_NO_POS` (3). |
| `enemy` | `u8` | For `SND_AT_ENEMY`, the enemy slot (`ENEMY_xx`); otherwise unused, written as 0. |
| `x` | `i16` | For `SND_AT_POINT`, the point's x; otherwise unused. |
| `z` | `i16` | For `SND_AT_POINT`, the point's z; otherwise unused. |

## Behaviour

The instruction's length depends on `where`: with `where` 0 to 3 it is 10
bytes and takes all seven arguments, and past 3 it is 6 bytes and takes
the first five. The compiler checks the count against `where`.

The sound is queued and played after the scripts of the frame have run,
panned and attenuated from the camera on screen:

- `SND_AT_POINT` plays at (`x`, 0, `z`).
- `SND_AT_PLAYER` plays at the player's position.
- `SND_AT_ENEMY` plays at the position of the entity in enemy slot
  `enemy`. A slot past the entity table is dropped.
- `SND_NO_POS` plays with no position. The original passes the bank number
  as the sound too (`play_sfx(bank, bank)`) and ignores `sound`, and the
  port does the same; the port then places it on the player.
- A `where` past 3 has no case in the original: the instruction does
  nothing.

`SND_BANK_BGM` does not play a sound effect: it pans the main music channel
from the position and restarts it. A sound the bank does not hold plays
nothing.

## Example

```c
event rebecca_hunter_attack_footsteps
{
    for (5)
    {
        se_play_3d(SND_BANK_ROOM, SE_0F, 0, SND_AT_ENEMY, ENEMY_00_REBECCA, 0, 0);
        sleep(18);
        se_play_3d(SND_BANK_ROOM, SE_0F, 0, SND_AT_ENEMY, ENEMY_00_REBECCA, 0, 0);
        sleep(13);
    }
    // ...
}
```

```c
se_play_3d(SND_BANK_DOOR, SE_DOOR_00, 0, SND_AT_POINT, 0, 2468, 11628);
sleep(30);
se_play_3d(SND_BANK_DOOR, SE_DOOR_01, 0, SND_AT_POINT, 0, 2468, 11628);
```

Room 101 (`games/re1/room/101/room.akscr`) plays footsteps on the enemy in
slot 0, and room 10A (`games/re1/room/10a/room.akscr`) plays its door's
open and close sounds at the door.

## See also

- [`voice_play`](voice_play.md): plays a voice line.
- [`bgm_play`](bgm_play.md): starts a music channel.
