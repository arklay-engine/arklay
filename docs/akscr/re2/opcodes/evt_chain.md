# `evt_chain`

Restarts the running task on another procedure of the same list.

| | |
| --- | --- |
| Opcode | `0x03` |
| Group | Control flow |
| Kind | Instruction |
| Size | 4 bytes |
| Status | VM |

## Syntax

```c
evt_chain(0, 0, proc);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `_` | `u8` | Not read. Write 0. |
| `_` | `u8` | Not read. Write 0. |
| `proc` | `u8` | The procedure to run: `init_NN` or `main_NN` with this number, from the list that is running. |

## Behaviour

The task drops what it was doing: its `gosub` calls, open `if` blocks and
loops are discarded. It then starts procedure `proc` from its beginning, in
the same task slot, and goes on running it in the same tick. Nothing comes
back to the instruction after the `evt_chain`.

The procedure is taken from the list that is running. In `init_00`, during
the room's load, that is the init list, so `evt_chain` is one of the two ways
(with [`gosub`](gosub.md)) to reach the other init procedures. Elsewhere it is
the main list. A `proc` past the end of the list stops the task.

## Example

```c
proc main_02
{
    if (ck(5, 6, 1))
    {
        evt_chain(0, 0, 3);      // the second visit runs main_03 instead
    }
    message_on(0, 2, 0, 0xFF, 0xFF);
    evt_end(0);
}
```

Illustrative: once room bit 6 is set, the area's procedure hands over to
`main_03` for good.

## See also

- [`evt_exec`](evt_exec.md): starts a procedure in another task.
- [`gosub`](gosub.md): runs a procedure and comes back.
- [What runs when](../index.md#what-runs-when): the init and main lists and the task slots.
