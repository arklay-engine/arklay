# `while`

Runs its body for as long as its condition holds, testing before each pass.

| | |
| --- | --- |
| Opcode | `0x0F` |
| Group | Control flow |
| Kind | Block |
| Size | 4 bytes |
| Status | VM |

## Syntax

```c
while (condition)
{
    statements
}
```

The condition may join several conditions with `&&` and `||`. The raw form is
`@while(C, L);` followed by the condition's bytes.

## Parameters

The structured form takes the condition and the body. The instruction's own
fields, as the `@` form writes them:

| Parameter | Width | Description |
| --- | --- | --- |
| `C` | `u8` | The byte length of the condition region that follows the `while` (at most 255). The compiler computes it. |
| `L` | `u16` | The block's length, from the end of the `while` to the end of its [`ewhile`](ewhile.md). The compiler computes it. |

## Behaviour

`while (cond) { body }` compiles to `while(C, L)`, the condition region, the
body and `ewhile(0)`. Each time the `while` runs, it pushes a loop entry and
evaluates the condition region. If the condition fails, the entry is removed
and the task goes on past the `ewhile`. Otherwise the body runs, and
[`ewhile`](ewhile.md) jumps back to the `while`, which tests again.

A failing condition here only makes the loop's condition false: it does not
jump to an enclosing `if`'s end, and it does not end the tick.

With several conditions, the region is the first condition, then a 16-bit
join word before each further one (0 for `&&`, 1 for `||`). Every condition
runs, left to right, with no precedence and no short cut, so
`a || b && c` means `(a || b) && c`. The retail scripts never use this form,
so the decompiler writes it back as `@while(...)` and the instructions the
bytes decode to (see
[several conditions in a loop](../index.md#several-conditions-in-a-loop)).

A loop whose condition passes and whose body never gives up the tick runs
until the port's limit of 65,536 instructions per task per tick, then goes on
at the next tick. Put [`evt_next`](evt_next.md) in the body of a wait loop.

## Example

```c
while (ck(5, 3, 0) && cmp(0, 1, 5, 2))
{
    evt_next();
}
```

Illustrative: the task waits, one test a tick, until room bit 3 is set or the
player stands in interaction area 2 (variable 1).

## See also

- [`ewhile`](ewhile.md): closes the loop.
- [`do`](do.md): the loop that tests after each pass.
- [`break`](break.md): leaves the loop.
