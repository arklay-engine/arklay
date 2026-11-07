# `cmpw`

Compares a state word with a value.

| | |
| --- | --- |
| Opcode | `0x07` |
| Group | Conditions |
| Kind | Condition |
| Size | 6 bytes |

## Syntax

```c
cmpw(0, word, comparison, value);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| *pad* | `u8` | Unused, written as 0. |
| `word` | `u8` | The state word (`W_LIGHT_STATE`, `W_LIGHT_DELTA`, `W_RANDOM`, `W_DPAD_HELD`, `W_DPAD_PRESSED`; unnamed ones as `W_xx`). |
| `comparison` | `u8` | `CMP_EQ`, `CMP_GT`, `CMP_GE`, `CMP_LT`, `CMP_LE` or `CMP_NE` (0 to 5). |
| `value` | `i16` | The value to compare with, written signed (-32768 to 32767) or as an unsigned word (up to 65535). |

## Behaviour

`cmpw` is a [condition](index.md#conditions): it stands in an `if` head,
a `require` or a `do`/`while` test.

It passes when `state comparison value` holds, the state word on the left.
The comparison is between unsigned words: a negative `value` stands for its
16-bit pattern, so `-1` is 65535 and `cmpw(0, W_xx, CMP_LT, -1)` passes for
every value but 65535. A comparison past 5 always fails.

Words 0 to 15 are the original's fading-state words. A higher index reads
two bytes of the state block at byte `0x14 + 2 * word`, as the original's
unchecked index does, which lands in the flag banks (see
[`cmpb`](cmpb.md)).

## Example

```c
if (cmpw(0, W_04, CMP_GT, 180))
{
    set(FG_COMMON, F_COMMON_C_REBECCA_CALL_TOO_LATE, TRUE);
}
```

Room 101 (`games/re1/room/101/room.akscr`): word 4 counts up while Rebecca
waits; past 180 the player is too late.

## See also

- [`setw`](setw.md): writes a state word.
- [`cmpb`](cmpb.md): compares a state byte.
