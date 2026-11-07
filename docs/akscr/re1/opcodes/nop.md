# `nop`

Does nothing.

| | |
| --- | --- |
| Opcode | `0x0E` |
| Group | Flags and state |
| Kind | Command |
| Size | 2 bytes |

## Syntax

```c
nop(0);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| *pad* | `u8` | Unused, written as 0. |

## Behaviour

The instruction is skipped. It is not a condition, so it never ends a block
or leaves an `if` body. No shipped script uses it.

## Example

```c
proc init
{
    nop(0);
}
```

An illustrative snippet; no checked-in room uses `nop`.

## See also

- [`nop_wide`](nop_wide.md): the 4-byte form.
