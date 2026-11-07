# `return`

Returns to the `gosub` that called the procedure, or ends the task.

| | |
| --- | --- |
| Opcode | `0x19` |
| Group | Control flow |
| Kind | Instruction |
| Size | 2 bytes |
| Status | VM |

## Syntax

```c
return(0);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `_` | `u8` | Not read. Write 0. |

## Behaviour

In the port, `return` does what [`evt_end`](evt_end.md) does. In a procedure
called with [`gosub`](gosub.md), the task goes back to the instruction after
the `gosub`, in the same tick, with the caller's blocks as they were. In a
procedure that was not called, the task ends.

`return` is an ordinary instruction, so it takes its operand: write
`return(0);`, not `return;`. It does not count as the closing `evt_end` that
the last procedure of a list needs (see
[a script's layout](../index.md#a-scripts-layout)).

## Example

```c
proc main_04
{
    if (ck(5, 7, 1))
    {
        return(0);               // already done
    }
    set(5, 7, 1);
    sce_item_lost(0x4A);
    evt_end(0);
}
```

Illustrative: a procedure called with `gosub(4)` that takes item `0x4A` from
the player once, and returns early on later calls.

## See also

- [`evt_end`](evt_end.md): the same, and the instruction that ends a list.
- [`gosub`](gosub.md): the call it returns from.
