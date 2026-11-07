# `endif`

Closes an `if` body that has no `else`.

| | |
| --- | --- |
| Opcode | `0x03` |
| Group | Flow |
| Kind | Flow |
| Size | 2 bytes |

## Syntax

```c
if (cond)
{
    ...
}
```

`endif` is written by the closing brace of an `if` with no `else`, never as
a call.

## Parameters

None. The opcode carries one operand byte, which the compiler writes as 0
and the VM ignores.

## Behaviour

Reached at the end of the body, `endif` pops the `if`'s entry off the
branch stack and the flow goes on after it. A failed condition of the `if`
resumes at the byte after the `endif`, so it skips the `endif` and the pop
it would do (the failure has already popped the entry).

## Example

```c
proc main
{
    if (testpickup(IT_2F_KEY_INK_RIBBON))
    {
        aot_delete(AOT_05_MESSAGE_NOTHING, SCE_MESSAGE, SAT_MANUAL | SAT_PLAYER);
    }
}
```

Room 112 (`games/re1/room/112/room.akscr`): the closing brace of the `if`
compiles to `endif`.

## See also

- [`if`](if.md): opens the body.
- [`else`](else.md): ends a body that has an `else`.
