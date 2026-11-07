# `ck_anim`

Compares a room object's push counter with a count.

| | |
| --- | --- |
| Opcode | `0x36` |
| Group | Conditions |
| Kind | Condition |
| Size | 4 bytes |

## Syntax

```c
ck_anim(object, comparison, count);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `object` | `u8` | The room object (`OBJ_xx`, as `obj` set it up). |
| `comparison` | `u8` | `CMP_EQ`, `CMP_GT`, `CMP_GE`, `CMP_LT`, `CMP_LE` or `CMP_NE` (0 to 5), the counter on the left. |
| `count` | `u8` | The count to compare with. |

## Behaviour

`ck_anim` is a [condition](index.md#conditions): it stands in an `if`
head, a `require` or a `do`/`while` test.

Each room object keeps a push counter: the number of frames in a row the
player has walked into it within reach. It drops to 0 on a frame the player
does not, and on the ninth frame a pushable object starts to move (the
counter is then held at 8, or parked at 10 when the object is blocked). The test passes when the counter compares to `count`
as `comparison` says. A comparison past 5, or an object slot past the
table, fails. Despite the name, which is the original tool's, it reads no
animation.

## Example

```c
if (ck(FG_ROOM, F_ROOM_PLAYER_AT_AQUARIUM, TRUE) && ck(FG_ROOM, F_ROOM_TOO_HEAVY_RUNNING, FALSE) && ck_anim(OBJ_00_AQUARIUM, CMP_GE, 7))
{
    set(FG_ROOM, F_ROOM_TOO_HEAVY_RUNNING, TRUE);
    evt_exec(0, TASK_00, aquarium_too_heavy);
}
```

Room 20A (`games/re1/room/20a/room.akscr`): after seven frames of pushing
against the full aquarium the "too heavy" event runs.

## See also

- [`obj`](obj.md): sets up a room object.
