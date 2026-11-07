# `evt_next`

Gives up the rest of the tick; the task goes on from the next instruction at the next tick.

| | |
| --- | --- |
| Opcode | `0x02` |
| Group | Control flow |
| Kind | Instruction |
| Size | 1 byte |
| Status | VM |

## Syntax

```c
evt_next();
```

## Parameters

None.

## Behaviour

The task stops for this tick, after the `evt_next`. At the next tick it
resumes with the following instruction, with its loops, `if` blocks and
`gosub` calls as they were. The other tasks run as usual.

`evt_next` is the usual body of a wait loop: `while (cond) { evt_next(); }`
tests the condition once a tick.

During the room's load, `init_00` runs in passes rather than ticks: an
`evt_next` there ends a pass, and the next pass starts at once (at most 256
passes; see [what runs when](../index.md#what-runs-when)). `main_00` gets one
pass during the load, up to its first `evt_next`, `sleep` or end. And
`main_01` restarts from its beginning at every tick, so an `evt_next` in it
ends that tick's run for good.

## Example

```c
proc main_02
{
    while (ck(5, 3, 0))
    {
        evt_next();
    }
    cut_chg(2);
    evt_end(0);
}
```

Illustrative: the task waits, one test per tick, until room bit 3 is set,
then switches to camera 2.

## See also

- [`sleep`](sleep.md): gives up a fixed number of ticks.
- [`wsleep`](wsleep.md): waits for a message or voice line.
- [`while`](while.md): the loop around a wait.
