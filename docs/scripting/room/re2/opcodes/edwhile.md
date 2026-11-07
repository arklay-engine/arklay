# `edwhile`

Closes a `do` loop: tests the condition that follows it and jumps back to the body while it holds.

| | |
| --- | --- |
| Opcode | `0x12` |
| Group | Control flow |
| Kind | Block |
| Size | 2 bytes |
| Status | VM |

## Syntax

```c
do
{
    statements
}
while (condition);
```

The `while (condition);` of a `do` compiles to `edwhile` and the condition.
The raw form is `@edwhile(C);` followed by the condition's bytes.

## Parameters

The structured form takes the condition. The instruction's field, as the `@`
form writes it:

| Parameter | Width | Description |
| --- | --- | --- |
| `C` | `u8` | The byte length of the condition region that follows (at most 255). The compiler computes it. |

## Behaviour

`edwhile` evaluates the `C` bytes of conditions after it. If the condition
holds, the task jumps back to the start of the innermost loop's body, the
one its [`do`](do.md) opened, and the entry stays. If it fails, the entry is
removed and the task goes on after the condition region.

The region is the first condition, then a 16-bit join word before each
further one (0 for `&&`, 1 for `||`), evaluated left to right with no
precedence and no short cut. A failing condition here does not jump to an
enclosing `if`'s end. The decompiler writes a region with more than one
condition as `@edwhile(...)` followed by the decoded instructions (see
[several conditions in a loop](../index.md#several-conditions-in-a-loop)).

## Example

```c
do
{
    evt_next();
}
while (ck(5, 4, 1) || ck(5, 5, 1));
```

Illustrative: the task waits while either room bit 4 or room bit 5 is set.
This form decompiles as `@do`, `@edwhile` and the instructions of the
region, and compiles back to the same bytes.

## See also

- [`do`](do.md): opens the loop.
- [`while`](while.md): the loop that tests first.
