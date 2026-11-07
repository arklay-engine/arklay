# `ewhile`

Closes a `while` loop and jumps back to its test.

| | |
| --- | --- |
| Opcode | `0x10` |
| Group | Control flow |
| Kind | Block |
| Size | 2 bytes |
| Status | VM |

## Syntax

```c
while (condition)
{
    statements
}
```

The closing brace of a `while` compiles to `ewhile`. The raw form is
`@ewhile(0);`.

## Parameters

The structured form has none. The instruction's field, as the `@` form writes
it:

| Parameter | Width | Description |
| --- | --- | --- |
| `_` | `u8` | Not read. Write 0. |

## Behaviour

`ewhile` removes the innermost loop entry and jumps back to the `while` that
pushed it. The `while` then pushes a new entry and evaluates its condition
again, leaving the loop past the `ewhile` when it fails. With no loop open,
`ewhile` just goes on with the next instruction.

## Example

```c
while (sce_key_ck(0, 0x0080))
{
    evt_next();
}
```

Illustrative: the task waits until the action button is held; the closing
brace is the `ewhile(0)`.

## See also

- [`while`](while.md): opens the loop.
