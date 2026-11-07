# `if`

Opens a conditional body: the conditions after it decide whether the body runs.

| | |
| --- | --- |
| Opcode | `0x01` |
| Group | Flow |
| Kind | Flow |
| Size | 2 bytes |

## Syntax

```c
if (cond && cond ...)
{
    ...
}
else if (cond)
{
    ...
}
else
{
    ...
}
```

`if` is written with the `if` statement, never as a call. The head is one or
more [condition commands](index.md#conditions) joined by `&&`; there is no
`||` or `!`.

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| skip | `u8` | Written by the compiler, not the script: the distance in bytes from this instruction to its `else` or `endif`. |

## Behaviour

The statement compiles to `if`, the head's condition commands, the body,
then [`else`](else.md) and the `else` body, or [`endif`](endif.md) when
there is no `else`. `else if` is an `if` nested in the `else` body.

At run time `if` pushes its resume address onto the command VM's branch
stack: the byte after its `else` or `endif`. Then the condition commands
run one after another. When one fails, the VM pops the stack and goes to
that address, so the flow enters the `else` body, or goes on past the
`endif`. When they all pass, the body runs. Its `else` pops the entry and
jumps over the `else` body; its `endif` just pops it.

A failed condition anywhere in the body, as in a
[`require`](../index.md#require), resumes the same way at the innermost open
`if`. With no `if` open, a failed condition ends the block, as
[`end`](end.md) does.

The command VM's branch stack is 16 entries deep, as the original's is. A
17th nested `if` is not pushed, so a failed condition in it resumes at the
16th `if`'s address: do not nest deeper than 16. The event VM's inline
command flow (`run { }` blocks) keeps an unbounded stack.

The skip is one byte, so a body (with its `else`) must stay under 256 bytes.
When it does not, the compiler emits [`if_long`](if_long.md) and
[`else_long`](else_long.md) instead, which behave the same. An `if` in an
event's own statements (outside `run { }`) compiles to the port's `evt_if`
instead; see [Event `if`](../index.md#event-if).

## Example

```c
if (testitem(IT_03_WEAPON_SHOTGUN))
{
    set(FG_ITEM, F_ITEM_116_WEAPON_SHOTGUN_0, TRUE);
    setbyte(0, 1, 0);
    evt_exec(0, TASK_09, rack_settles);
    ...
}
```

Room 116 (`games/re1/room/116/room.akscr`): the body runs on the frame the
player uses the shotgun on the rack.

## See also

- [`else`](else.md), [`endif`](endif.md): close the body.
- [`if_long`](if_long.md): the same with a two-byte skip.
- [`ck`](ck.md), [`cmpb`](cmpb.md): the commonest conditions.
- [`if`, `else` and `else if`](../index.md#if-else-and-else-if) in the language reference.
