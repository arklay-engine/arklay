# `sce_key_ck`

Tests whether any of a set of pad buttons is held.

| | |
| --- | --- |
| Opcode | `0x4F` |
| Group | Flags, variables and pad |
| Kind | Condition |
| Size | 4 bytes |
| Status | yes |

## Syntax

```c
sce_key_ck(value, mask);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `value` | `u8` | Non-zero passes when any button in `mask` is held; `0` passes when none is. |
| `mask` | `u16` | The buttons, as bits of the pad word (below). |

## Behaviour

The scripts read the pad as the original's `Key` word, which the port builds
from the input each tick:

| Bit | Button |
|---|---|
| `0x0001` | forward (up), also `0x0010` |
| `0x0002` | turn right, also `0x0800` |
| `0x0004` | back (down), also `0x0020` |
| `0x0008` | turn left, also `0x0400` |
| `0x0040` | fire |
| `0x0080` | action (check) |
| `0x0100` | aim |
| `0x0200` | run, also `0x2000` (cancel) |
| `0x1000` | confirm: action or fire |

`sce_key_ck` passes when `(Key & mask) != 0` matches `value`, and fails
otherwise (see [conditions](../index.md#conditions) for what a failure does).

While the player's input is held (bank 2 bit 7, or the tick after a door),
`Key` keeps only `0x0400`, `0x0800`, `0x1000` and `0x2000`, so a script in a
scene can still read confirm and cancel.

## Example

```c
while (sce_key_ck(0, 0x1000))
{
    evt_next();
}
```

Illustrative: the task waits until confirm is held.

## See also

- [`sce_trg_ck`](sce_trg_ck.md): the buttons pressed this tick.
