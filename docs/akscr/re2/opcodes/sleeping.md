# `sleeping`

The counting half of `sleep`; not written on its own.

| | |
| --- | --- |
| Opcode | `0x0A` |
| Group | Control flow |
| Kind | Instruction |
| Size | 3 bytes |
| Status | VM |

## Syntax

```c
sleeping(ticks);
```

You do not write this: [`sleep`](sleep.md)`(10, ticks)` contains it.

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `ticks` | `u16` | Not read by `sleeping` itself: the count it decrements is the one `sleep` put on the loop stack from these same bytes. |

## Behaviour

Each time it runs, `sleeping` takes one off the count of the innermost entry
on the task's loop stack and gives up the tick. While the count is above
zero, the task stays on the `sleeping` and runs it again at the next tick.
When the count reaches zero, the entry is removed and the task goes on with
the next instruction at the following tick.

[`sleep`](sleep.md) is the byte `0x09` followed by a `sleeping`: `sleep`
pushes the count and steps onto its own second byte, which is this
instruction. The decompiler writes the pair as `sleep(10, ticks);`.

Written on its own, `sleeping` acts on whatever is on top of the loop stack.
In a [`for`](for.md) body it would count that loop down and remove it; with
no loop open it gives up one tick and goes on. Neither is useful, so write
`sleep`.

## Example

```c
sleep(10, 30);
```

Illustrative: this compiles to the bytes `09 0A 1E 00`, of which `0A 1E 00`
is a `sleeping(30)`.

## See also

- [`sleep`](sleep.md): the instruction to write.
