# `aot_set`

Sets up an AOT of any type: an examine spot, an event spot, a save point and
so on.

| | |
| --- | --- |
| Opcode | `0x0D` |
| Group | Room actions (AOTs) |
| Kind | Command |
| Size | 18 bytes |

## Syntax

```c
aot_set(aot, x, z, width, depth, type, trigger, d0, d1, d2, d3, d4, d5);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `aot` | `u8` | The AOT slot, 0 to 31. |
| `x` | `i16` | The zone's corner on the floor, x. |
| `z` | `i16` | The zone's corner, z. |
| `width` | `i16` | The zone's size along x. |
| `depth` | `i16` | The zone's size along z. |
| `type` | `u8` | The AOT type, `SCE_*` (see [Values](../index.md#values)). |
| `trigger` | `u8` | The trigger flags, `SAT_*`: who sets it off (`SAT_PLAYER`, `SAT_OBJECT`), where (`SAT_UNDER`: at the feet rather than the reach point) and how (`SAT_MANUAL`: on the action button). |
| `d0` ... `d5` | `u8` each | Six data bytes, read by the type as three little-endian words: `d0`/`d1`, `d2`/`d3`, `d4`/`d5`. |

## Behaviour

`aot_set` fills AOT slot `aot`, replacing what it held. The zone is a
rectangle on the floor, and the type decides what happens when the trigger
fires. The data's meaning depends on the type:

| Type | Data | Does |
| --- | --- | --- |
| `SCE_MESSAGE` | message id word, pause word | shows a message (see [Message ids](../index.md#message-ids)) |
| `SCE_EVENT` | event slot word, event word, *0* | starts the event in the slot; slot 9 (or any value of 8 and up) takes the first free slot |
| `SCE_USEITEM` | group word, bit word, set word | sets the bit of the group (`set` nonzero) or clears it |
| `SCE_ITEMBOX` | | opens the item box |
| `SCE_TYPEWRITER` | | the typewriter |
| `SCE_HIKIDASHI` | `FG_LOCK` bit word, item AOT word, camera word | a locked desk: unlocked with a key, it opens onto the item AOT and its close-up camera |
| `SCE_5`, `SCE_6` | | latch the side the player approaches a door from (split along x, or along z) |
| `SCE_B` | | an effect zone: shifts the footstep sounds |
| `SCE_C` | | a stairs zone |
| `SCE_NONE` | | nothing: an inert zone |

Any other type does nothing when it is set off.

An event spot's event word can be the event's name: in
`aot_set(.., SCE_EVENT, SAT_MANUAL | SAT_PLAYER, 9, 0, examine_kenneth, 0, 0, 0)`
the event is `d2`. An examine spot's message id word is `d0` and `d1`, the
low byte first, and its pause word `d2` and `d3`.

For doors use [`door_aot_set`](door_aot_set.md) and for items
[`item_aot_set`](item_aot_set.md), which carry the data those types need.

## Example

```c
aot_set(AOT_04_EVENT_KENNETH, 3300, 5600, 2000, 2000, SCE_EVENT, SAT_MANUAL | SAT_PLAYER, 9, 0, examine_kenneth, 0, 0, 0);
aot_set(AOT_06_MESSAGE_PAINTING, 3300, 7400, 2000, 1000, SCE_MESSAGE, SAT_MANUAL | SAT_PLAYER, 131, 0, 255, 0, 0, 0);
```

Room 104 (`games/re1/room/104/room.akscr`): examining Kenneth's body starts
the `examine_kenneth` event in a free slot, and the painting shows room
message 3 at once (131 is `0x83`, `MSG_FAST` and index 3), pausing the
systems of pause word 255.

## See also

- [`aot_reset`](aot_reset.md): re-arms an AOT with a new type and data.
- [`aot_delete`](aot_delete.md): changes only the type and trigger.
- [`aot_on`](aot_on.md): runs an AOT now.
- [`evt_exec`](evt_exec.md): starts an event from a script.
