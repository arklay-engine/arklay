# `else_long`

The port's `else` with a two-byte skip, paired with `if_long`.

| | |
| --- | --- |
| Opcode | `0x52` |
| Group | Flow |
| Kind | Flow |
| Size | 3 bytes |

## Syntax

```c
if (cond)
{
    ...
}
else
{
    ...
}
```

`else_long` is never written by name. The compiler writes it for the `else`
of an `if` statement it compiles to [`if_long`](if_long.md).

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| target | `u16` | Written by the compiler: the forward distance from the start of this instruction to the end of the `else` body. |

## Behaviour

It behaves exactly as [`else`](else.md): reached at the end of the `if`
body, it pops the `if`'s branch-stack entry and jumps past the `else` body.
Only the operand differs, a word instead of a byte.

The opcode is the port's own, so a script that needs it runs only in
Arklay.

## Example

```c
if (ck(FG_ROOM, 2, FALSE))
{
    // a body long enough to need if_long
    set(FG_ROOM, 2, TRUE);
}
else
{
    set(FG_ROOM, 3, TRUE);
}
```

Illustrative: an ordinary `if`/`else`; the compiler chooses the long forms
when the bodies are too long for a byte.

## See also

- [`if_long`](if_long.md): the long `if`.
- [`else`](else.md): the original's short form.
