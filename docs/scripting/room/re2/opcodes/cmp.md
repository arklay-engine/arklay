# `cmp`

Compares a script variable with a constant.

| | |
| --- | --- |
| Opcode | `0x23` |
| Group | Flags, variables and pad |
| Kind | Condition |
| Size | 6 bytes |
| Status | VM |

## Syntax

```c
cmp(0, variable, comparison, value);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `_` | `u8` | Not read. Write 0. |
| `variable` | `u8` | The script variable, `0` to `255`. |
| `comparison` | `u8` | `0` `==`, `1` `>`, `2` `>=`, `3` `<`, `4` `<=`, `5` `!=`, `6` passes when `variable & value` is not 0; any other value passes when the variable is not 0. |
| `value` | `i16` | The constant to compare with. |

## Behaviour

`cmp` reads the variable as a signed 16-bit number and compares it with
`value` as `variable OP value`, so `cmp(0, 16, 1, 3)` passes when variable 16
is above 3. Comparison 6 tests the bits, with both sides as unsigned words.
A failure acts as any condition's does (see
[conditions](../index.md#conditions)).

Variables 0 and 1 are the interaction area in front of the player and the
area the player stands in, 24 to 26 are the stage (from 0), the room and the
camera on screen, read from the engine, and 29 is
[`sce_rnd`](sce_rnd.md)'s random number. The table in
[flags and variables](../index.md#flags-and-variables) lists them.

## Example

```c
if (cmp(0, 26, 0, 4) && ck(5, 12, 0))
{
    set(5, 12, 1);
    se_on(2, 6, 1, 0, 0, 0);
}
```

Illustrative: the first time the room's camera 4 is on screen, it plays room
sound 6 at the player.

## See also

- [`save`](save.md), [`calc`](calc.md): set and change a variable.
- [`switch`](switch.md): chooses among several values.
- [`member_cmp`](member_cmp.md): compares a field of the work.
