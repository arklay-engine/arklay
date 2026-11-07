# `ck`

Tests one bit of a flag bank.

| | |
| --- | --- |
| Opcode | `0x21` |
| Group | Flags, variables and pad |
| Kind | Condition |
| Size | 4 bytes |
| Status | VM |

## Syntax

```c
ck(bank, index, value);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `bank` | `u8` | The flag bank, `0` to `31` (see [flags and variables](../index.md#flags-and-variables)). |
| `index` | `u8` | The bit in the bank, `0` to `255`. |
| `value` | `u8` | Non-zero passes when the bit is set; `0` passes when it is clear. |

## Behaviour

`ck` passes when the bit's state matches `value`, and fails otherwise. What a
failure does depends on where the `ck` is (see
[conditions](../index.md#conditions)): in an `if` head or body it jumps to the
end of the innermost `if`, in a `while` or `do` condition it makes the
condition false, and anywhere else it ends the task's tick, the task going
on after the `ck` at the next tick.

There are 32 banks of 256 bits. Bit `index` is `0x80000000 >> (index & 31)`
of the bank's 32-bit word `index >> 5`, so bits count from the most
significant bit of each word, as in the original's `Flg_addr`. A bank from 32
up reads as clear.

Bank 11 (`fg_message`) also reads the answer to the last question: bit 31 is
set while the answer was "no". Some other banks hold state the engine reads
and writes (the player's character and scenario in bank 1, the stops in bank
2, the room's bits in bank 5, the enemies' and items' flags in banks 6 to 8,
the camera mask groups in banks 13 to 28); the table in
[flags and variables](../index.md#flags-and-variables) lists them.

`value` reads the other way round from RE 1: in RE 2, `ck(bank, bit, 1)`
passes when the bit is set.

## Example

```c
if (ck(1, 0, 1))
{
    xa_on(0, 0x20);              // Claire's line
}
else
{
    xa_on(0, 0x21);              // Leon's line
}
```

Illustrative: bank 1 bit 0 is set when the player is Claire.

## See also

- [`set`](set.md): changes a flag bit.
- [`cmp`](cmp.md): compares a variable.
