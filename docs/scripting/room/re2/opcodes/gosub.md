# `gosub`

Calls another procedure of the running list and comes back.

| | |
| --- | --- |
| Opcode | `0x18` |
| Group | Control flow |
| Kind | Instruction |
| Size | 2 bytes |
| Status | VM |

## Syntax

```c
gosub(proc);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `proc` | `u8` | The procedure to call, by its number in the list that is running (`init_NN` during the init, `main_NN` otherwise). |

## Behaviour

`gosub` saves where the task is, with its open `if` blocks and loops, and
runs procedure `proc` from its beginning in the same task, with empty
block stacks of its own. When the called procedure reaches
[`evt_end`](evt_end.md) or [`return`](return.md), the task comes back to the
instruction after the `gosub`, in the same tick, with the caller's blocks as
they were. A called procedure may give up ticks; the call waits with it.

- Calls nest at most four deep. A `gosub` made when four calls are already
  open is skipped, and the task goes on after it.
- A `proc` past the end of the list stops the task.
- The procedure numbers are those of the list that is running. In
  `init_00`, `gosub` reaches the other init procedures, which run only when
  `init_00` calls or chains to them.
- The work chosen by [`work_set`](work_set.md) belongs to the task, so a
  called procedure acts on the caller's work and can change it.

## Example

```c
proc init_00
{
    gosub(1);
    if (ck(5, 4, 0))
    {
        gosub(2);
    }
    evt_end(0);
}

proc init_01
{
    aot_set(0, 4, 0x11, 0, 0, -2000, 3000, 1200, 800, 6, 0, 0, 0, 0, 0);
    return(0);
}

proc init_02
{
    item_aot_set(1, 2, 0x11, 0, 0, 1500, -600, 900, 900, 0x13, 1, 12, 0, 1);
    return(0);
}
```

Illustrative: the init sets up a message area always and an item area only
while room bit 4 is clear. The trigger bytes and coordinates are placeholders.

## See also

- [`return`](return.md), [`evt_end`](evt_end.md): come back from the call.
- [`evt_chain`](evt_chain.md): goes to another procedure without coming back.
- [`evt_exec`](evt_exec.md): runs a procedure in another task.
