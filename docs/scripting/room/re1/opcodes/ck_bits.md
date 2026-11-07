# `ck_bits`

Tests whether any of a set of pad buttons is held.

| | |
| --- | --- |
| Opcode | `0x38` |
| Group | Conditions |
| Kind | Condition |
| Size | 4 bytes |

## Syntax

```c
ck_bits(state, mask);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `state` | `u8` | `TRUE` (0) passes when a button of the mask is held, anything else (`FALSE`) when none is. |
| `mask` | `u16` | The buttons, as bits of the held-buttons word: `PAD_xxxx` names ORed together (`PAD_0001 \| PAD_0040`). |

## Behaviour

`ck_bits` is a [condition](index.md#conditions): it stands in an `if`
head, a `require` or a `do`/`while` test.

It ANDs the mask with the held-buttons state word (`W_DPAD_HELD`, word 6).
With `state` 0 it passes when the result is non-zero; with any other
`state` it passes when the result is zero. The bits are the original's pad
word and have no names beyond their `PAD_` placeholders yet.

## Example

```c
do
{
    evt_next();
}
while (ck_bits(TRUE, PAD_0001));
```

Room 20A (`games/re1/room/20a/room.akscr`): the event waits for the button
to be released before handing control back.

## See also

- [`cmpw`](cmpw.md): compares a state word, `W_DPAD_HELD` or `W_DPAD_PRESSED` among them.
