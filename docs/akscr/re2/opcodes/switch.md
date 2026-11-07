# `switch`

Jumps to the section whose value matches a script variable.

| | |
| --- | --- |
| Opcode | `0x13` |
| Group | Control flow |
| Kind | Block |
| Size | 4 bytes |
| Status | VM |

## Syntax

```c
switch (variable)
{
    case (value)
    {
        statements
    }
    default
    {
        statements
    }
}
```

The raw form is `@switch(variable, L);`.

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `variable` | `u8` | The script variable to test. The structured form's only operand. |
| `L` | `u16` | The block's length, from the end of the `switch` to the end of its [`eswitch`](eswitch.md). The compiler computes it. |

## Behaviour

`switch (variable)` compiles to `switch(variable, L)`, then each
`case (value)` as `case(0, Lc, value)` and its body, then `default(0)` and its
body if there is one, and `eswitch(0)`.

When the `switch` runs, it pushes a loop entry (so that
[`break`](break.md) leaves the block) and reads the variable's 16 bits. It
then walks the sections from the first: at each [`case`](case.md) whose
value equals the variable it jumps into that case's body; a `case` that does
not match is stepped over by its own length to the next section; a
[`default`](default.md) is taken whatever the value. When the walk reaches
the `eswitch`, or any instruction that is not a `case` or `default`, no
section matched: the entry is removed and the task goes on past the block.

As in C, a section runs on into the next one unless it ends in `break(0);`.
The `case` and `default` instructions do nothing when the flow reaches them.

- `default` must be the last section, and a switch may have no sections.
- The variable is compared as an unsigned 16-bit number with the case
  values, which are 16-bit too (`-1` and `0xFFFF` are the same value).
- The compiler takes the structured form only; the decompiler writes `@`
  instructions for a block whose bytes do not follow the pattern.

## Example

```c
switch (26)
{
    case (1)
    {
        cut_chg(2);
        break(0);
    }
    case (2)
    {
    }
    default
    {
        cut_chg(0);
    }
}
```

Illustrative: variable 26 is the camera on screen. On camera 1 the script
switches to camera 2; on camera 2 it falls through the empty case into
`default` and switches to camera 0, as it does on any other camera.

## See also

- [`case`](case.md), [`default`](default.md), [`eswitch`](eswitch.md): the parts of the block.
- [`break`](break.md): leaves the block.
- [`cmp`](cmp.md): a single comparison of a variable.
