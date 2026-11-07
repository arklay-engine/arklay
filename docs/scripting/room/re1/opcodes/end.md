# `end`

Ends the running command block, at any `if` depth.

| | |
| --- | --- |
| Opcode | `0x00` |
| Group | Flow |
| Kind | Flow |
| Size | 2 bytes |

## Syntax

```c
return;
```

`end` is not called by name: `return;` compiles to it, and so does the
close of every `proc` section that is not `noreturn`. Calling `end(...)` is
an error.

## Parameters

None. The opcode carries one operand byte, which the compiler writes as 0
and the VM ignores.

## Behaviour

`end` stops the block at once, however many `if`s are open: the original's
handler zeroes the `if` nesting count, so the branch stack is dropped rather
than unwound. What runs next depends on the block:

- In a `proc init` or `proc main` block, the next block of the list runs.
- In a procedure (`proc NAME`), control returns to the instruction after
  the call (see [`gosub`](gosub.md)).
- In an event's `run { }` block or inline command, the command part ends and
  the event goes on with its next event statement.

A block that has no `end` (a `noreturn` section) runs off the end of its
bytecode. The port then stops the whole block list for that pass: the blocks
after it do not run. See [Section flags](../index.md#section-flags).

## Example

```c
proc main
{
    if (ck(FG_SCENARIO, F_SCENARIO_MANSION2, FALSE))
    {
        return;
    }
    else
    {
        if (ck(FG_SCENARIO, F_SCENARIO_TRAP_PASSAGE_HUNTER_AMBUSH, FALSE))
```

Room 10A (`games/re1/room/10a/room.akscr`): on the first mansion visit the
main block ends at once, from inside the `if`.

## See also

- [`if`](if.md): opens a conditional body.
- [`gosub`](gosub.md): calls a procedure, which `return;` leaves.
- [`return`](../index.md#return) in the language reference.
