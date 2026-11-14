# `door_aot_set`

Sets up a door: its zone, its animation, its lock and the room it leads to.

| | |
| --- | --- |
| Opcode | `0x0C` |
| Group | Room actions (AOTs) |
| Kind | Command |
| Size | 26 bytes |

## Syntax

```c
door_aot_set(aot, x, z, width, depth, direction, sound, animation, camera,
             lock, room, next_x, next_y, next_z, next_angle, key, trigger);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `aot` | `u8` | The AOT slot, 0 to 31 (the room's `AOT_xx_DOOR_*` defines). |
| `x` | `i16` | The zone's corner on the floor, x. |
| `z` | `i16` | The zone's corner, z. |
| `width` | `i16` | The zone's size along x. |
| `depth` | `i16` | The zone's size along z. |
| `direction` | `u8` | Which way the door opens: picks the arm of the door animation. |
| `sound` | `u8` | The door's sound pair, a `DOOR_SND_*` name from `inc/doors.akinc`. |
| `animation` | `u8` | The door animation (door, stairs, ladder, lift...), a `DOOR_*` name from `inc/doors.akinc`. |
| `camera` | `u8` | The low six bits are the animation's camera. Bit `0x80` makes a door that keeps the current room (a camera-only transition); bit `0x40` a silent one. |
| `lock` | `u8` | 0 for no lock, or `DOOR_LOCKED \| F_LOCK_*`: locked until that `FG_LOCK` bit is set. `DOOR_LOCK_40` marks a door for one character only. |
| `room` | `u8` | The room it leads to: `ROOM_*` for a room of this stage, `STAGE_n \| ROOM_*` for another stage, 255 for none. |
| `next_x` | `i16` | Where the player arrives, x (read unsigned). |
| `next_y` | `i16` | Where the player arrives, y (signed). |
| `next_z` | `i16` | Where the player arrives, z (read unsigned). |
| `next_angle` | `i16` | Which way the player faces on arrival, 0 to 4095. |
| `key` | `u8` | `UNLOCKED` (0), `LOCK` (254: opens from the other side), `LOCKED` (255: never opens from here), or the item (`IT_*`) that unlocks it. |
| `trigger` | `u8` | The trigger flags (`SAT_*`): usually `SAT_MANUAL \| SAT_PLAYER`, the player on the action button. |

## Behaviour

`door_aot_set` fills AOT slot `aot` with a door, replacing what the slot
held. When the player sets it off (or a script runs it with
[`aot_on`](aot_on.md)), the engine decides as the original does:

1. A lock with `DOOR_LOCK_40` refuses a character the door is not for and
   shows the wrong-character message. The two playable characters always
   pass this test; Rebecca, played after a script stores `B_CHARACTER_MODEL`
   3 (40C's Plant 42 scene), is refused, which keeps her out of 40C from
   408 until her V-JOLT is poured.
2. With no `DOOR_LOCKED` bit, or with its `FG_LOCK` bit already set, the
   door opens: the door animation plays and the player arrives in `room` at
   the given position and angle. A room of the first mansion stages leads
   to the second visit's version while `F_SCENARIO_MANSION2` is set.
3. Otherwise the key decides. `LOCK` plays the locked sound, shows "It's
   locked from the other side" and sets the `FG_LOCK` bit, so the next try
   opens it. `LOCKED` shows "It's locked". An item key that the player does
   not carry shows the key's own locked message; one the player carries is
   turned: the key-turn message shows, the `FG_LOCK` bit is set, and the key
   is used, as the menu's USE would, when the message closes. The door opens on the next try.

Jill opens the doors of the sword key with her lockpick once she has it.
Outside the authentic mode Chris may too, and Jill may turn a sword key she
carries.

When a door opens, the engine clears the run word (`FG_RUN`), so the player
stops updating for the rest of that frame, as in the original. The room load
restores it.

## Example

```c
door_aot_set(AOT_00_DOOR_103, 14800, 4100, 3000, 3100, 2, DOOR_SND_00_DR_WD, DOOR_01_DOOR01, 1, DOOR_LOCKED | F_LOCK_103_104, ROOM_103_F_PASSAGE, 3000, 0, 3000, 3072, LOCKED, SAT_MANUAL | SAT_PLAYER);
door_aot_set(AOT_01_DOOR_10F, 29800, 4000, 1700, 3500, 2, DOOR_SND_00_DR_WD, DOOR_1F_DOOR05K, 5, DOOR_LOCKED | F_LOCK_104_10F, ROOM_10F_BAR, 14000, 0, 3000, 3072, IT_33_KEY_MANSION_SWORD, SAT_MANUAL | SAT_PLAYER);
```

Room 104 (`games/re1/room/104/room.akscr`): the door to 103 stays locked
until its `FG_LOCK` bit is set from the other side, and the door to the bar
opens with the sword key.

## See also

- [`aot_set`](aot_set.md): sets up any other kind of AOT.
- [`aot_reset`](aot_reset.md), [`aot_delete`](aot_delete.md): change an AOT.
- [`aot_on`](aot_on.md): runs an AOT now.
