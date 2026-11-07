# `endif`

Closes an `if` block that has no `else`.

| | |
| --- | --- |
| Opcode | `0x08` |
| Group | Control flow |
| Kind | Block |
| Size | 2 bytes |
| Status | VM |

## Syntax

```c
if (conditions)
{
    statements
}
```

The closing brace of an `if` without `else` compiles to `endif`. The raw form
is `@endif(0);`.

## Parameters

The structured form has none. The instruction's field, as the `@` form writes
it:

| Parameter | Width | Description |
| --- | --- | --- |
| `_` | `u8` | Not read. Write 0. |

## Behaviour

`endif` closes the innermost open `if` and goes on with the next instruction.
The `if`'s length runs to the end of the `endif`, so a failed condition in the
block also lands just past it.

The opcode is two bytes wide, because its handler in the executable skips
two; biohazard-utils' table gives it another width. An `if` with an `else` has no `endif`; its
`else` body ends with two [`nop`](nop.md) bytes instead.

## Example

```c
if (ck(5, 2, 0))
{
    set(5, 2, 1);
    message_on(0, 1, 0, 0xFF, 0xFF);
}
evt_end(0);
```

Illustrative: the message shows the first time only; the closing brace
compiles to `endif(0)`.

## See also

- [`if`](if.md): opens the block.
- [`else`](else.md): closes the block's first body when it has two.
