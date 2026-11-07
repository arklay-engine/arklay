# `evt_end`

Ends the procedure: back to the `gosub` that called it, or the end of the task.

| | |
| --- | --- |
| Opcode | `0x01` |
| Group | Control flow |
| Kind | Instruction |
| Size | 2 bytes |
| Status | VM |

## Syntax

```c
evt_end(0);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `_` | `u8` | Not read. Write 0. |

## Behaviour

If the procedure was called with [`gosub`](gosub.md), `evt_end` returns to
the caller, which goes on in the same tick with the instruction after the
`gosub`, and with its own `if` and loop state as it was. Otherwise the task
ends: its slot is free, and nothing runs in it until an interaction area,
[`evt_exec`](evt_exec.md) or the scheduler starts another procedure there.
In the port, `evt_end` and [`return`](return.md) do the same thing.

The compiler adds no `evt_end`, so end every procedure with one yourself. The
last procedure of each list must end with an `evt_end` at its top level
(outside every block): a reader of the stored list ends that procedure at its
first such `evt_end`, so without one the list does not read back the same
from its bytes (see [a script's layout](../index.md#a-scripts-layout)).

A procedure that runs off its end without `evt_end` also stops the task,
because the port treats the end of the procedure's bytes, or a byte that is
not an opcode, as the task's end.

## Example

```c
proc main_02
{
    if (ck(5, 4, 0))
    {
        evt_end(0);              // nothing to do yet
    }
    set(5, 4, 0);
    evt_end(0);
}
```

Illustrative: the first `evt_end` leaves early when room bit 4 is clear; the
second closes the procedure.

## See also

- [`return`](return.md): the same, under its other name.
- [`gosub`](gosub.md): calls a procedure that `evt_end` returns from.
- [`evt_kill`](evt_kill.md): ends another task.
