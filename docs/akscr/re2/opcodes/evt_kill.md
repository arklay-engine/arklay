# `evt_kill`

Stops the task in a slot.

| | |
| --- | --- |
| Opcode | `0x05` |
| Group | Control flow |
| Kind | Instruction |
| Size | 2 bytes |
| Status | VM |

## Syntax

```c
evt_kill(task);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `task` | `u8` | The task slot to stop, `0` to `13`. A larger value does nothing. |

## Behaviour

The task in slot `task` stops where it is, whatever it was waiting for, and
its slot becomes free for [`evt_exec`](evt_exec.md). A slot that is not
running is left as it is. Stopping the running task's own slot ends it at
once, like [`evt_end`](evt_end.md) without the return to a caller.

Nothing is undone: flags, variables, areas and the work the stopped task had
set stay as they are, and a character it had sent walking keeps walking.

## Example

```c
proc main_06
{
    evt_kill(3);                 // stop the task that loops the alarm
    sce_bgm_control(1, 2, 0, 0, 0);
    evt_end(0);
}
```

Illustrative: one procedure stops a looping task in slot 3 and stops the
sub music it was playing.

## See also

- [`evt_exec`](evt_exec.md): starts a procedure in a slot.
- [`evt_end`](evt_end.md): ends the running task.
