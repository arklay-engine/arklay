# `do`

Runs its body once, then again for as long as its condition holds.

| | |
| --- | --- |
| Opcode | `0x11` |
| Group | Control flow |
| Kind | Block |
| Size | 4 bytes |
| Status | VM |

## Syntax

```c
do
{
    statements
}
while (condition);
```

The statement ends with `;`. The condition may join several conditions with
`&&` and `||`. The raw form is `@do(0, L);`.

## Parameters

The structured form takes the body and the condition. The instruction's own
fields, as the `@` form writes them:

| Parameter | Width | Description |
| --- | --- | --- |
| `_` | `u8` | Not read. Write 0. |
| `L` | `u16` | The block's length, from the end of the `do` to the end of the condition after the [`edwhile`](edwhile.md). The compiler computes it. |

## Behaviour

`do { body } while (cond);` compiles to `do(0, L)`, the body, `edwhile(C)`
and the condition region. The `do` pushes a loop entry and runs the body.
At the [`edwhile`](edwhile.md), the condition is evaluated: if it holds, the
task jumps back to the start of the body; if not, the entry is removed and
the task goes on after the condition. [`break`](break.md) jumps to the end
given by `L`.

A failing condition only makes the loop's condition false. Several
conditions join as in a [`while`](while.md): left to right, with a 16-bit
join word between them, no precedence and no short cut.

## Example

```c
work_set(3, 0);
plc_dest(0, 4, 8, -3000, 12000);
do
{
    evt_next();
}
while (ck(5, 8, 0));
```

Illustrative: it sends the character in enemy slot 0 walking and waits until
it arrives, which sets room bit 8.

## See also

- [`edwhile`](edwhile.md): closes the loop and holds its condition.
- [`while`](while.md): the loop that tests first.
