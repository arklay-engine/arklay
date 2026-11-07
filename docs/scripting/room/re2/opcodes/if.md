# `if`

Opens a block that runs only when its conditions pass.

| | |
| --- | --- |
| Opcode | `0x06` |
| Group | Control flow |
| Kind | Block |
| Size | 4 bytes |
| Status | VM |

## Syntax

```c
if (condition && condition ...)
{
    statements
}
else
{
    statements
}
```

The `else` part is optional. The raw form is `@if(0, L);`.

## Parameters

The structured form takes the conditions and the bodies. The instruction's
own fields, as the `@` form writes them:

| Parameter | Width | Description |
| --- | --- | --- |
| `_` | `u8` | Not read. Write 0. |
| `L` | `u16` | The block's length in bytes, from the end of the `if` to the end of its [`endif`](endif.md), or to the end of its [`else`](else.md) instruction. The compiler computes it. |

## Behaviour

`if` records where its block ends (`L` bytes past itself) and goes on with
the instruction that follows: the conditions, then the body. The conditions
are ordinary instructions placed first in the body. Only the
[condition opcodes](../index.md#conditions) can fail, and when one does,
the task jumps to the end of the innermost open `if`: into the `else` body if
there is one, otherwise past the `endif`. The remaining conditions are not
evaluated. The same holds for a condition anywhere in the body, so a
condition in an `if` body works like RE 1's `require(...)`.

Without an `else`, the block compiles to `if(0, L)`, the conditions, the body
and `endif(0)`. With one, it compiles to `if(0, L)`, the conditions, the body,
`else(0, J)`, the `else` body and two `nop()` bytes, with no `endif`
(see [`if` and `else`](../index.md#if-and-else)).

- Conditions join with `&&` only. `||` in an `if` is a compiler error, and
  there is no `!`: use the operand that selects the opposite test, such as
  `ck(.., .., 0)` for a clear bit or [`cmp`](cmp.md) with comparison 5.
- `if ()` with no condition is allowed, and its body always runs.
- There is no `else if`: nest the `if` in an `else` block.
- Blocks nest up to 256 deep in the compiler. The VM keeps one stack of open
  `if`s per `gosub` level. [`break`](break.md) drops the `if`s opened inside
  the loop it leaves, and [`goto`](goto.md) says how many to keep.

The decompiler writes a block as `@if(...)` when its bytes do not match the
structured pattern (a non-zero first byte, or a length that does not end at
its `endif` or `else`). When it does structure the block, the head is every
condition opcode at the start of the body, so a condition you put first in
the body comes back in the head. The bytes are the same.

## Example

```c
if (ck(1, 25, 0) && cmp(0, 24, 0, 3))
{
    set(5, 3, 1);
}
else
{
    gosub(2);
}
```

Illustrative: when bank 1 bit 25 is clear and the stage (variable 24) is 3,
it sets room bit 3, and otherwise calls procedure 2.

## See also

- [`else`](else.md), [`endif`](endif.md): the instructions that close the block.
- [`ck`](ck.md), [`cmp`](cmp.md): the usual conditions.
- [Conditions](../index.md#conditions): what a failing condition does in each place.
