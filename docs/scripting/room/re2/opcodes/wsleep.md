# `wsleep`

Starts a wait for a message window or voice line to finish; follow it with `wsleeping`.

| | |
| --- | --- |
| Opcode | `0x0B` |
| Group | Control flow |
| Kind | Instruction |
| Size | 1 byte |
| Status | VM |

## Syntax

```c
wsleep();
wsleeping();
```

## Parameters

None.

## Behaviour

`wsleep` puts a wait entry on the task's loop stack and goes on with the next
instruction, which should be [`wsleeping`](wsleeping.md). `wsleeping` then
holds the task while a message window is open or a voice line is playing,
and removes the entry when neither is. Together they replace RE 1's
`wait_message()`.

The pair belongs together. A `wsleep` without its `wsleeping` leaves the
entry on the loop stack, where [`next`](next.md), [`ewhile`](ewhile.md) or
[`break`](break.md) would take it for the innermost loop.

## Example

```c
proc main_02
{
    message_on(0, 5, 0, 0, 0);
    wsleep();
    wsleeping();
    xa_on(0, 0x12);
    wsleep();
    wsleeping();
    evt_end(0);
}
```

Illustrative: it shows room message 5 and waits until the player dismisses
it, then plays a voice line of the stage and waits until it ends.

## See also

- [`wsleeping`](wsleeping.md): the instruction that waits.
- [`message_on`](message_on.md): shows a message.
- [`xa_on`](xa_on.md): plays a voice line.
