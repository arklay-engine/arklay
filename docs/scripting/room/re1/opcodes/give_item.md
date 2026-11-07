# `give_item`

Opens the got-item viewer for an item AOT; the item is awarded when the
viewer closes.

| | |
| --- | --- |
| Opcode | `0x2D` |
| Group | Room actions (AOTs) |
| Kind | Command |
| Size | 4 bytes |

## Syntax

```c
give_item(aot, handler, flags);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `aot` | `u8` | The item AOT slot, set up by [`item_aot_set`](item_aot_set.md). |
| `handler` | `u8` | The original's handler byte; every shipped use is 4 (the item handler). The port does not read it. |
| `flags` | `u8` | The original's flags byte; every shipped use is 129. The port does not read it. |

## Behaviour

`give_item` arms the got-item viewer for the AOT and raises the menu's
got-item and item-view bits. The viewer opens on the following frames. When
it closes, the item of the AOT is picked up as the yes answer of a pick-up
prompt would: it goes into the inventory (the radio raises its scenario
flag instead), its `FG_ITEM` bit is cleared and its model hidden.

Nothing is awarded when the instruction runs, so the script goes on at once;
an event that must wait for the viewer waits on its own.

## Example

```c
act_reset();
wait_message();
give_item(AOT_08_ITEM_AMMO_GRENADE_ACID, 4, 129);
set(FG_RUN, F_RUN_CONTROLS, TRUE);
```

Room 203 (`games/re1/room/203/room.akscr`): after the scene, the player is
given the acid rounds and the controls come back.

## See also

- [`item_aot_set`](item_aot_set.md): sets up the item AOT.
- [`aot_on`](aot_on.md): runs an AOT's handler, such as its pick-up prompt.
- [`remove_item`](remove_item.md): takes an item away.
