# `else`

Ends an `if` body and opens the body that runs when the conditions failed.

| | |
| --- | --- |
| Opcode | `0x07` |
| Group | Control flow |
| Kind | Block |
| Size | 4 bytes |
| Status | VM |

## Syntax

```c
if (conditions)
{
    statements
}
else
{
    statements
}
```

The raw form is `@else(0, J);`.

## Parameters

The structured form has none of its own. The instruction's fields, as the `@`
form writes them:

| Parameter | Width | Description |
| --- | --- | --- |
| `_` | `u8` | Not read. Write 0. |
| `J` | `i16` | The jump, counted from the *start* of the `else` instruction to the end of the two `nop` bytes that close the `else` body. The compiler computes it. |

## Behaviour

The `if`'s length `L` runs to the end of the `else` instruction, so a failed
condition lands on the first instruction of the `else` body. When the `if`
body runs to its end instead, the flow reaches the `else` instruction, which
closes the `if` and jumps `J` bytes ahead, past the `else` body and its two
`nop` bytes.

The structured form writes the two [`nop`](nop.md) bytes after the `else`
body itself; they are part of the pattern the decompiler recognises, and the
original's scripts close an `else` the same way. There is no `endif`.

The port reads `J` as an unsigned forward distance, as the structured form
always produces one; a backward jump in an `@else` does not work.

`else if` is not accepted. Write `else { if (...) { ... } }`. The `else`
body of a long block is limited to 32,767 bytes by the signed jump.

## Example

```c
if (keep_item_ck(0x33))
{
    message_on(0, 4, 0, 0xFF, 0xFF);
}
else
{
    message_on(0, 5, 0, 0xFF, 0xFF);
}
```

Illustrative: it shows room message 4 when the player carries item `0x33`,
and message 5 otherwise.

## See also

- [`if`](if.md): opens the block.
- [`nop`](nop.md): the two bytes that close the `else` body.
