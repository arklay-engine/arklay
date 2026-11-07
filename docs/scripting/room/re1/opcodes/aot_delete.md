# `aot_delete`

Changes only an AOT's type and trigger; `SCE_NONE` switches it off.

| | |
| --- | --- |
| Opcode | `0x13` |
| Group | Room actions (AOTs) |
| Kind | Command |
| Size | 4 bytes |

## Syntax

```c
aot_delete(aot, type, trigger);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `aot` | `u8` | The AOT slot, 0 to 31. |
| `type` | `u8` | The new type, `SCE_*`. `SCE_NONE` makes the zone inert. |
| `trigger` | `u8` | The new trigger flags, `SAT_*`. |

## Behaviour

Despite its name, `aot_delete` does not empty the slot: it sets the type and
the trigger flags and keeps the zone and the data. With `SCE_NONE` the zone
no longer does anything, and an [`aot_reset`](aot_reset.md) or another
`aot_delete` with the old type can switch it back on.

Nothing happens when the slot is empty. As with `aot_reset`, a slot holding
an ink ribbon the character does not need is not turned back into an item.

## Example

```c
aot_delete(AOT_05_EVENT_TO_WEST_STAIRS, SCE_NONE, SAT_UNDER | SAT_PLAYER);
cut_auto(CUT_LOCKED);
dir_set(0, 0, 0, 0, 11336, -4315, 10556);
```

Room 110 (`games/re1/room/110/room.akscr`): the event that walks the player
to the stairs first switches its own trigger zone off.

## See also

- [`aot_reset`](aot_reset.md): changes the type and the data.
- [`aot_set`](aot_set.md): sets up an AOT.
