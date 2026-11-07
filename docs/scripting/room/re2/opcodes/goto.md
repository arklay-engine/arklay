# `goto`

Jumps by a byte distance, keeping a given number of open `if`s and loops.

| | |
| --- | --- |
| Opcode | `0x17` |
| Group | Control flow |
| Kind | Instruction |
| Size | 6 bytes |
| Status | VM |

## Syntax

```c
goto(if_depth, loop_depth, 0, jump);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `if_depth` | `u8` | The number of open `if` blocks at the target, less one: `0` keeps one, `255` keeps none. |
| `loop_depth` | `u8` | The number of open loops (and `switch` blocks and waits) at the target, less one: `255` keeps none. |
| `_` | `u8` | Not read. Write 0. |
| `jump` | `i16` | The jump in bytes, counted from the start of the `goto`. Negative jumps backwards. |

## Behaviour

`goto` cuts the task's `if` stack to `if_depth + 1` entries and its loop
stack to `loop_depth + 1` entries (each value wraps, so 255 means 0), then
jumps `jump` bytes from its own first byte. It never adds entries: a depth
above the current one keeps them all. The jump stays within the procedure
that is running.

There are no labels, so the distance must be worked out by hand from the
instruction widths, and nothing checks it. A jump that lands inside an
instruction, or the depths that do not match the target, give undefined
results. The decompiler writes `goto` as a plain instruction, and the
structured blocks, `break`, `gosub` and `evt_chain` cover most needs.

## Example

```c
goto(255, 255, 0, 10);           // skip the next instruction
set(5, 1, 1);
evt_end(0);
```

Illustrative: the `goto` is 6 bytes and the `set` 4, so a jump of 10 from the
start of the `goto` lands on the `evt_end`, with no blocks open.

## See also

- [`break`](break.md): leaves the innermost loop without counting bytes.
- [`gosub`](gosub.md): calls another procedure.
- [`break`, `goto`, `gosub`, `return`](../index.md#break-goto-gosub-return)
