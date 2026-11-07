# `break`

Leaves the innermost loop, `switch` or wait.

| | |
| --- | --- |
| Opcode | `0x1A` |
| Group | Control flow |
| Kind | Instruction |
| Size | 2 bytes |
| Status | VM |

## Syntax

```c
break(0);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `_` | `u8` | Not read. Write 0. |

## Behaviour

`break` removes the innermost entry of the task's loop stack and jumps to its
end: past the `next` of a [`for`](for.md) or [`for2`](for2.md), past the
`ewhile` of a [`while`](while.md), past the condition of a [`do`](do.md), or
past the `eswitch` of a [`switch`](switch.md). Any `if` blocks opened inside
that loop are dropped with it, so a `break` inside an `if` in the loop works
as in C. With no loop open, `break` does nothing and the task goes on.

The waits of [`sleep`](sleep.md) and [`wsleep`](wsleep.md) are entries on the
same stack, so a `break` reached while one is open would end that wait
instead.

`break` is an ordinary instruction and takes its operand: write `break(0);`,
not `break;`.

## Example

```c
for (90)
{
    if (sce_trg_ck(1, 0x1000))
    {
        break(0);                // skipped by a press
    }
    evt_next();
}
```

Illustrative: the task waits three seconds, or until the confirm button is
pressed.

## See also

- [`goto`](goto.md): a jump that says how many blocks to keep.
- [`switch`](switch.md): where `break` ends a section.
