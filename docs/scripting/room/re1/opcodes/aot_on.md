# `aot_on`

Runs an AOT's handler now, as if the player had set it off.

| | |
| --- | --- |
| Opcode | `0x24` |
| Group | Room actions (AOTs) |
| Kind | Command |
| Size | 4 bytes |

## Syntax

```c
aot_on(aot, type, 0);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `aot` | `u8` | The AOT slot, 0 to 31. |
| `type` | `u8` | The handler to run, `SCE_*`: normally the AOT's own type. |
| *pad* | `u8` | Unused, written as 0. |

## Behaviour

`aot_on` runs the handler of `type` on AOT slot `aot` without the trigger
test: the player need not stand in the zone or press a button. The handler
is the one `type` names, not the one stored in the slot, so the script
chooses it. A door (`SCE_DOOR`) goes through the whole lock and key test of
[`door_aot_set`](door_aot_set.md) and, when it opens, starts the room
change. An item handler (`SCE_ITEM`) acts only on an item AOT and opens its
pick-up prompt. A message, event, item box or flag spot does what a trigger
would.

Nothing happens when the slot is empty or the type has no handler.

## Example

```c
setb(B_CHARACTER_MODEL, 0, 0);
aot_reset(AOT_01_DOOR_20E, SCE_DOOR, SAT_MANUAL | SAT_PLAYER, 1, 0, 0);
aot_on(AOT_01_DOOR_20E, SCE_DOOR, 0);
remove_item(IT_42_KEY_SERUM);
```

Room 20D (`games/re1/room/20d/room.akscr`): the script turns the AOT back
into its door and opens it at once.

## See also

- [`aot_set`](aot_set.md), [`door_aot_set`](door_aot_set.md): set up the AOT.
- [`give_item`](give_item.md): opens the got-item viewer for an item AOT.
