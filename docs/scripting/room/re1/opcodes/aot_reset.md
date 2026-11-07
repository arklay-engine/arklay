# `aot_reset`

Re-arms an AOT with a new type, trigger and data, keeping its zone.

| | |
| --- | --- |
| Opcode | `0x12` |
| Group | Room actions (AOTs) |
| Kind | Command |
| Size | 10 bytes |

## Syntax

```c
aot_reset(aot, type, trigger, w0, w1, w2);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `aot` | `u8` | The AOT slot, 0 to 31. |
| `type` | `u8` | The new type, `SCE_*`. |
| `trigger` | `u8` | The new trigger flags, `SAT_*`. |
| `w0` | `i16` | The first data word. |
| `w1` | `i16` | The second data word. |
| `w2` | `i16` | The third data word. |

## Behaviour

`aot_reset` rewrites the type, the trigger flags and the three data words of
an AOT that a `door_aot_set`, `aot_set` or `item_aot_set` set up. The zone
stays. The words mean what [`aot_set`](aot_set.md)'s data means for the new
type: for `SCE_EVENT`, the event slot (9 for any free slot), the event and
0; for `SCE_MESSAGE`, the message id and the pause word.

The item data of an `item_aot_set` survives, as in the original: an item
AOT re-armed as an event and later as an item again still names its item.
A door keeps its door record, so `aot_reset(.., SCE_DOOR, ..)` turns an AOT
that was a door back into that door.

Nothing happens when the slot is empty. A slot holding an ink ribbon that
the player's character does not need (the ink ribbon options) is not
re-armed as an item, so the ribbon stays hidden.

## Example

```c
if (ck(FG_ENEMY, F_ENEMY_101_HUNTER_0, TRUE) && ck(FG_ROOM, F_ROOM_C_MOURN_ON_KILL, TRUE))
{
    set(FG_ROOM, F_ROOM_C_MOURN_ON_KILL, FALSE);
    aot_reset(AOT_05_MESSAGE_REBECCA, SCE_EVENT, SAT_MANUAL | SAT_PLAYER, 9, mourn_rebecca, 0);
}
```

Room 101 (`games/re1/room/101/room.akscr`): once the Hunter is dead, talking
to Rebecca runs the `mourn_rebecca` event instead.

## See also

- [`aot_set`](aot_set.md): sets up an AOT and its zone.
- [`aot_delete`](aot_delete.md): changes only the type and trigger.
