# `ck_counter`

Passes when the player is within a distance of an enemy, a room object or an item model.

| | |
| --- | --- |
| Opcode | `0x3C` |
| Group | Conditions |
| Kind | Condition |
| Size | 6 bytes |

## Syntax

```c
ck_counter(0, kind, target, distance);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| *pad* | `u8` | Unused, written as 0. |
| `kind` | `u8` | What `target` is: `DIST_ENEMY` (0), `DIST_OBJ` (1) or `DIST_ITEM` (2). |
| `target` | `u8` | The enemy slot (`ENEMY_xx`), room object (`OBJ_xx`) or item model (`IMDL_xx`). |
| `distance` | `u16` | The largest distance that passes. |

## Behaviour

`ck_counter` is a [condition](index.md#conditions): it stands in an `if`
head, a `require` or a `do`/`while` test.

It measures the distance from the player to the target's position on the
floor plane (x and z only, height ignored), rounded down to a whole unit,
and passes when it is `distance` or less. Any other `kind`, or a slot past
its table, fails. The engine stores `kind` and `target` as one word, `kind`
in the low byte. Despite the name, which is the original tool's, there is
no counter.

## Example

```c
if (ck(FG_ROOM, F_ROOM_IN_BOULDER_PATH, TRUE) && ck_counter(0, DIST_OBJ, OBJ_00_BOULDER, 4500))
{
    aot_delete(AOT_07_USEITEM_BOULDER_PATH, SCE_NONE, SAT_UNDER | SAT_PLAYER);
    if (ck_tween(0, 1024, 3072))
    {
        evt_exec(0, TASK_04, crushed_by_boulder);
    }
    ...
}
```

Room 30B (`games/re1/room/30b/room.akscr`): the rolling boulder crushes the
player once it comes within 4500 units.

## See also

- [`ck_tween`](ck_tween.md): tests the player's facing.
