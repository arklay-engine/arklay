# `aot_reset`

Changes an existing area's kind, trigger and first three data words, keeping its box.

| | |
| --- | --- |
| Opcode | `0x46` |
| Group | Interaction areas and doors |
| Kind | Instruction |
| Size | 10 bytes |
| Status | yes |

## Syntax

```c
aot_reset(slot, sce, sat, d0, d1, d2, d3, d4, d5);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `slot` | `u8` | The area slot to change. |
| `sce` | `u8` | The new kind, as for [`aot_set`](aot_set.md); `0` turns the area off. |
| `sat` | `u8` | The new trigger byte, as for `aot_set`. |
| `d0` ... `d5` | `u8` | The new data: three 16-bit words, low byte first. |

## Behaviour

`aot_reset` rewrites the area in slot `slot` in place, as the original's
`Aot_reset` does: its kind, trigger and first three data words change, and its
box or corners, floor, attachment and the rest of its data stay. A slot with
no area is left as it is.

- For an item (`sce` 2), the three words are the item, the count and the
  flag, as in [`item_aot_set`](item_aot_set.md) (the port reads each word's
  low byte). The area keeps the object and action bytes it had, whether it
  was an item before or was one earlier and has been reset to something else
  since. So an item area turned off at the load and back on later is still
  picked up crouching if it was.
- For any other kind, the six bytes are the kind's data, as in `aot_set`.
- A door area keeps its door record, so `aot_reset(slot, 1, ...)` after
  turning a door off brings the same door back.

`aot_reset(slot, 0, 0, 0, 0, 0, 0, 0, 0)` is the usual way to switch an area
off once its event has run.

## Example

```c
proc main_02
{
    set(5, 4, 1);                // once only
    aot_reset(2, 0, 0, 0, 0, 0, 0, 0, 0);
    message_on(0, 3, 0, 0, 0);
    evt_end(0);
}
```

Illustrative: an event area's procedure switches its own area off and shows a
message.

## See also

- [`aot_set`](aot_set.md): sets an area up in full.
- [`aot_on`](aot_on.md): fires an area.
