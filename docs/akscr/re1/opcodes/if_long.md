# `if_long`

The port's `if` with a two-byte skip, for a body longer than `if`'s one byte reaches.

| | |
| --- | --- |
| Opcode | `0x51` |
| Group | Flow |
| Kind | Flow |
| Size | 3 bytes |

## Syntax

```c
if (cond)
{
    ... // a body of 256 bytes or more
}
```

`if_long` is never written by name. The compiler picks it for an `if`
statement whose skip does not fit a byte, and pairs it with
[`else_long`](else_long.md) when the statement has an `else`.

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| target | `u16` | Written by the compiler: the forward distance from the start of this instruction to where the flow resumes when a condition fails (the first byte of the `else` body, or the byte after the `endif`). |

## Behaviour

It behaves exactly as [`if`](if.md): it pushes its resume address onto the
branch stack, its conditions follow, a failed condition pops the stack and
resumes there. The only difference is the encoding: the operand is a word,
counted from the start of the instruction to the resume point itself rather
than to the `else` or `endif`.

The original game has no such opcode (it is past the original's 0x50), so a
script that needs one runs only in Arklay. The shipped scripts never need it.

## Example

```c
if (ck(FG_ROOM, 2, FALSE))
{
    // Enough commands here to pass 255 bytes, for instance a dozen
    // door_aot_set or item_aot_set calls.
    set(FG_ROOM, 2, TRUE);
}
```

Illustrative: the source is an ordinary `if`. Only the size of the compiled
body decides between `if` and `if_long`.

## See also

- [`if`](if.md): the original's short form.
- [`else_long`](else_long.md): the long `else`.
