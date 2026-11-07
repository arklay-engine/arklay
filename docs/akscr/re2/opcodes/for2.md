# `for2`

Runs its body as many times as a script variable says.

| | |
| --- | --- |
| Opcode | `0x1B` |
| Group | Control flow |
| Kind | Block |
| Size | 6 bytes |
| Status | VM |

## Syntax

```c
for2 (variable)
{
    statements
}
```

The raw form is `@for2(0, L, 0, variable);`.

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `_` | `u8` | Not read. Write 0 (the structured form does). |
| `L` | `u16` | The block's length, from the end of the 6-byte `for2` to the end of its [`next`](next.md). The compiler computes it. |
| `_` | `u8` | Not read. Write 0 (the structured form does). |
| `variable` | `u8` | The script variable holding the count. The structured form's only operand. |

## Behaviour

`for2 (variable)` compiles to `for2(0, L, 0, variable)`, the body and
`next(0)`. It works as [`for`](for.md) does, except that the count is the
value of script variable `variable`, read once when the loop starts:
changing the variable in the body does not change the number of passes. A
value of 0 skips the body. The count is the variable's 16 bits taken as
unsigned, so a negative value gives a very long loop.

## Example

```c
save(16, 3);
for2 (16)
{
    se_on(2, 4, 1, 0, 0, 0);
    sleep(10, 10);
}
```

Illustrative: it plays sound 4 of the room's bank at the player three times,
a third of a second apart.

## See also

- [`for`](for.md): the same with a constant count.
- [`next`](next.md): closes the loop.
- [`save`](save.md), [`calc`](calc.md): set the variable.
