# `door_aot_set`

Puts a door in one of the room's area slots: its box, and the room, position and camera it leads to.

| | |
| --- | --- |
| Opcode | `0x3B` |
| Group | Interaction areas and doors |
| Kind | Instruction |
| Size | 32 bytes |
| Status | yes |

## Syntax

```c
door_aot_set(slot, sce, sat, floor, super, x, z, w, d,
             next_x, next_y, next_z, next_dir, next_stage, next_room, next_cut, 0,
             do2, door_type, sound, lock, key, 0);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `slot` | `u8` | The area slot, `0` to `31`. A larger slot is refused with a warning. |
| `sce` | `u8` | The kind: `1` (door). |
| `sat` | `u8` | The trigger byte, as for [`aot_set`](aot_set.md); a door is usually an action area (`0x10`). |
| `floor` | `u8` | The floor; `0x80` and up for every floor. |
| `super` | `u8` | The body or object the area moves with (bit 7 set), as for `aot_set`; otherwise 0. |
| `x`, `z`, `w`, `d` | `i16` | The box, as for `aot_set`. |
| `next_x`, `next_y`, `next_z` | `i16` | Where the player arrives. |
| `next_dir` | `i16` | The player's facing on arrival (low 12 bits; 4096 is a full turn). |
| `next_stage` | `u8` | The stage to go to, from 0. |
| `next_room` | `u8` | The room to go to. |
| `next_cut` | `u8` | The camera the player arrives under (low six bits). |
| `_` | `u8` | Not read. Write 0. |
| `do2` | `u8` | The DO2 file of the door animation. |
| `door_type` | `u8` | The door type, which the DO2 animation is given. |
| `sound` | `u8` | The door's sound. |
| `lock` | `u8` | `0` for an unlocked door. With bit 7 set, the door is locked, and the low six bits are its bit among the 64 unlocked-door bits. |
| `key` | `u8` | For a locked door: the item that opens it, `0xFE` for a door that unlocks from this side, or `0xFF` for one locked from the other side. |
| `_` | `u8` | Not read. Write 0. |

## Behaviour

`door_aot_set` fills slot `slot` with an area of kind door and a door record
that holds the destination. The area is tested as any other (see
[`aot_set`](aot_set.md)). When it fires, or when [`aot_on`](aot_on.md) fires
it:

- **An open door** (`lock` bit 7 clear, or its unlocked bit already set)
  plays the door animation and loads room `next_room` of stage `next_stage`,
  with the player at `next_x, next_y, next_z` facing `next_dir`, under
  camera `next_cut`.
- **A locked door** does not open. With `key` `0xFE`, it shows "You unlocked
  it." and sets its unlocked bit, so the next use opens it. With `0xFF`, it
  shows "It's locked from inside." and stays locked. Otherwise `key` is an
  item: without it, the door shows the message that names its lock (door
  message `key - 0x4C`, or "It's locked." for a key the table has no message
  for); with it, the door shows "You have used the item." and sets its
  unlocked bit, so the next use opens it.

The unlocked-door bits are the game's, not the room's: saves keep them, and a
door unlocked once stays unlocked on every later visit, and so does any other
door record that names the same bit.

## Example

```c
door_aot_set(0, 1, 0x31, 0, 0, -4600, -1200, 1800, 900,
             -15200, 0, 6800, 1024, 0, 3, 2, 0,
             1, 0, 2, 0, 0, 0);
```

Illustrative: an unlocked door to room 3 of the first stage, arriving under
camera 2. All the values are placeholders; the decompiled `room.akscr` of a
room has the real ones.

## See also

- [`door_aot_set_4p`](door_aot_set_4p.md): the same with four corners.
- [`aot_set`](aot_set.md): the area layout and the trigger byte.
- [`aot_on`](aot_on.md): opens the door from the script.
