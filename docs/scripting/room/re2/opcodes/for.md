# `for`

Runs its body a fixed number of times.

| | |
| --- | --- |
| Opcode | `0x0D` |
| Group | Control flow |
| Kind | Block |
| Size | 6 bytes |
| Status | VM |

## Syntax

```c
for (count)
{
    statements
}
```

The raw form is `@for(0, L, count);`.

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `_` | `u8` | Not read. Write 0 (the structured form does). |
| `L` | `u16` | The block's length, from the end of the 6-byte `for` to the end of its [`next`](next.md). The compiler computes it. |
| `count` | `u16` | How many times the body runs. The structured form's only operand. |

## Behaviour

`for (count)` compiles to `for(0, L, count)`, the body and `next(0)`. When
the `for` runs, a `count` of 0 skips the body and goes on after the `next`.
Otherwise it pushes a loop entry with the count onto the task's loop stack
and runs the body. Each [`next`](next.md) takes one off the count and jumps
back to the start of the body while more than one pass remains, so the body
runs exactly `count` times.

[`break`](break.md) in the body leaves the loop at once, past the `next`.
A body may give up ticks ([`evt_next`](evt_next.md), [`sleep`](sleep.md)):
the count is kept with the task, and the loop goes on at the next tick.

The count is a 16-bit constant. For a count held in a variable, use
[`for2`](for2.md).

## Example

```c
for (30)
{
    sce_shake_on(4, 0);
    evt_next();
    sce_shake_on(-4, 0);
    evt_next();
}
sce_shake_on(0, 0);
```

Illustrative: it shakes the picture from side to side for 60 ticks, then
puts it back.

## See also

- [`next`](next.md): closes the loop.
- [`for2`](for2.md): the same with the count in a variable.
- [`while`](while.md), [`do`](do.md): loops on a condition.
