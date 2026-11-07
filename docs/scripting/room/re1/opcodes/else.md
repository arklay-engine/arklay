# `else`

Ends an `if` body and jumps over the `else` body that follows it.

| | |
| --- | --- |
| Opcode | `0x02` |
| Group | Flow |
| Kind | Flow |
| Size | 2 bytes |

## Syntax

```c
if (cond)
{
    ...
}
else
{
    ...
}
```

`else` is written with the `else` keyword of an `if` statement, never as a
call.

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| skip | `u8` | Written by the compiler: the distance in bytes from this instruction to the end of the `else` body. |

## Behaviour

The instruction stands between the two bodies. Reached at the end of the
`if` body (every condition passed), it pops the `if`'s entry off the branch
stack and jumps to the end of the `else` body. A failed condition of the
`if` never executes it: the `if`'s resume address is the byte after the
`else`, which is the first byte of the `else` body.

There is no `endif` after an `else` body. `else if` compiles to an `else`
whose body is another `if`.

When the bodies are too long for a one-byte skip, the compiler writes
[`else_long`](else_long.md) instead.

## Example

```c
if (cmpw(0, W_04, CMP_GT, 180))
{
    set(FG_COMMON, F_COMMON_C_REBECCA_CALL_TOO_LATE, TRUE);
}
else
{
    set(FG_COMMON, F_COMMON_C_REBECCA_CALL_IN_TIME, TRUE);
}
```

Room 101 (`games/re1/room/101/room.akscr`): one of the two flags is set,
depending on how long Rebecca's call took.

## See also

- [`if`](if.md): opens the body.
- [`endif`](endif.md): closes an `if` with no `else`.
- [`else_long`](else_long.md): the same with a two-byte skip.
