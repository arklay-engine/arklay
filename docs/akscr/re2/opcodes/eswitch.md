# `eswitch`

Closes a `switch` block.

| | |
| --- | --- |
| Opcode | `0x16` |
| Group | Control flow |
| Kind | Block |
| Size | 2 bytes |
| Status | VM |

## Syntax

```c
switch (variable)
{
    case (value)
    {
        statements
    }
}
```

The closing brace of a `switch` compiles to `eswitch`. The raw form is
`@eswitch(0);`.

## Parameters

The structured form has none. The instruction's field, as the `@` form writes
it:

| Parameter | Width | Description |
| --- | --- | --- |
| `_` | `u8` | Not read. Write 0. |

## Behaviour

When the last section's body runs into it, `eswitch` removes the loop entry
the [`switch`](switch.md) pushed and goes on with the next instruction. A
[`break`](break.md) in a section jumps just past it, removing the entry
itself, and a `switch` whose search finds no section goes past it the same
way.

## Example

```c
switch (16)
{
    case (2)
    {
        set(5, 10, 1);
    }
}
evt_end(0);
```

Illustrative: room bit 10 is set when variable 16 is 2; the switch's closing
brace is the `eswitch(0)`.

## See also

- [`switch`](switch.md): opens the block.
