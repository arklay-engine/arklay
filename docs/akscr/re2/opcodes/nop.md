# `nop`

Does nothing; two of them close every `else` block.

| | |
| --- | --- |
| Opcode | `0x00` |
| Group | Control flow |
| Kind | Instruction |
| Size | 1 byte |
| Status | no |

## Syntax

```c
nop();
```

## Parameters

None.

## Behaviour

`nop` has no effect: the task goes on with the next instruction in the same
tick. Doing nothing is the whole of its behaviour, but it reaches the engine
like any other unmodelled opcode, so the first one a run executes logs
`RE2 script: nop is not modelled`.

The structured `else` compiles to `else(0, J)`, the `else` body and two `nop`
bytes, which is how the original's scripts close an `else` (see
[`if` and `else`](../index.md#if-and-else)). You do not write those two
yourself. The `else` jump lands after them, so they run only when the `if`
body was skipped and the `else` body ran to its end.

The join word between two conditions of a `while` or `do` condition
decodes as `nop(); nop();` when it means `&&` (see
[several conditions in a loop](../index.md#several-conditions-in-a-loop)).

## Example

```c
proc main_03
{
    nop();
    evt_end(0);
}
```

Illustrative: a procedure that does nothing and ends.

## See also

- [`else`](else.md): the block whose end the two `nop` bytes mark.
- [`nop_1c`](nop_1c.md), [`nop_1e`](nop_1e.md), [`nop_1f`](nop_1f.md),
  [`nop_20`](nop_20.md): the other one-byte no-ops.
