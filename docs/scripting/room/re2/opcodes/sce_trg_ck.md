# `sce_trg_ck`

Tests whether any of a set of pad buttons was pressed this tick.

| | |
| --- | --- |
| Opcode | `0x50` |
| Group | Flags, variables and pad |
| Kind | Condition |
| Size | 4 bytes |
| Status | yes |

## Syntax

```c
sce_trg_ck(value, mask);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `value` | `u8` | Non-zero passes when any button in `mask` went down this tick; `0` passes when none did. |
| `mask` | `u16` | The buttons, as bits of the pad word (see [`sce_key_ck`](sce_key_ck.md)). |

## Behaviour

As [`sce_key_ck`](sce_key_ck.md), on the original's `Key_trg`: the bits of
`Key` that are set this tick and were clear the tick before. A button held
down passes only on the tick it was pressed. On the tick the player's input
stops being held, no button counts as pressed, so a button held through a
scene does not fire as it ends.

## Example

```c
do
{
    evt_next();
}
while (sce_trg_ck(0, 0x3000));
```

Illustrative: the task waits for a press of confirm or cancel.

## See also

- [`sce_key_ck`](sce_key_ck.md): the buttons held.
