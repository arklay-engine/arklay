# `wsleeping`

Gives up the tick while a message window is open or a voice line plays, at least once.

| | |
| --- | --- |
| Opcode | `0x0C` |
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

Each time it runs, `wsleeping` gives up the tick. If a message window is
open, or bank 1 bit 26 is set (a voice line from [`xa_on`](xa_on.md) is
playing), the task stays on it and tests again at the next tick. When
neither holds, it removes the innermost loop-stack entry (the one
[`wsleep`](wsleep.md) pushed) and the task goes on with the next instruction
at the following tick. So the task always waits at least one tick, even when
nothing is showing.

The wait ends on the message window as a whole: a question's window ends it
once the player has answered, and the answer is then in bank 11 (see
[flags and variables](../index.md#flags-and-variables)).

`wsleeping` without a `wsleep` before it removes whatever loop entry is on top
of the stack, so always write the two together.

## Example

```c
message_on(0, 7, 0, 0, 0);
wsleep();
wsleeping();
if (ck(11, 31, 0))
{
    set(5, 9, 1);
}
```

Illustrative: room message 7 is a yes/no question; once it closes, bank 11
bit 31 is clear for "yes", and the script sets room bit 9.

## See also

- [`wsleep`](wsleep.md): starts the wait.
- [`message_on`](message_on.md), [`xa_on`](xa_on.md): what it waits for.
