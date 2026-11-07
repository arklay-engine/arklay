# `next`

Closes a `for` or `for2` loop: counts a pass and goes back to the body while passes remain.

| | |
| --- | --- |
| Opcode | `0x0E` |
| Group | Control flow |
| Kind | Block |
| Size | 2 bytes |
| Status | VM |

## Syntax

```c
for (count)
{
    statements
}
```

The closing brace of a `for` or `for2` compiles to `next`. The raw form is
`@next(0);`.

## Parameters

The structured form has none. The instruction's field, as the `@` form writes
it:

| Parameter | Width | Description |
| --- | --- | --- |
| `_` | `u8` | Not read. Write 0. |

## Behaviour

`next` looks at the innermost entry of the task's loop stack. If its count is
above 1, it takes one off and jumps back to the start of the loop's body.
Otherwise it removes the entry and goes on with the instruction after the
`next`.

It does not check that the innermost entry is a `for`: an `@next` reached
inside some other loop, or with no loop open, acts on what is there (or
simply goes on).

## Example

```c
for2 (16)
{
    calc(0, 0, 17, 100);
    evt_next();
}
```

Illustrative: the loop body runs as many times as variable 16 says, adding
100 to variable 17 once a tick; the closing brace is the `next(0)`.

## See also

- [`for`](for.md), [`for2`](for2.md): the loops it closes.
