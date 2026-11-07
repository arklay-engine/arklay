# `case`

Opens a `switch` section for one value.

| | |
| --- | --- |
| Opcode | `0x14` |
| Group | Control flow |
| Kind | Block |
| Size | 6 bytes |
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

The raw form is `@case(0, L, value);`.

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `_` | `u8` | Not read. Write 0 (the structured form does). |
| `L` | `u16` | The section's length, from the end of the 6-byte `case` to the end of its body. The compiler computes it. |
| `value` | `u16` | The value of the `switch` variable that selects this section. The structured form's only operand. |

## Behaviour

The `case` instruction is read by the [`switch`](switch.md) as it looks for
the section to run: a matching `value` sends the task to the first
instruction of the body, and a different one steps the search `L` bytes on to
the next section. Because the search follows the lengths, every section must
end exactly where the next `case`, `default` or the `eswitch` begins.

When the flow reaches a `case` instruction from the section before (a body
that does not end in [`break`](break.md)), it does nothing, and the task runs
on into this section's body.

`case` cannot be written outside a `switch` except as `@case(...)`.

## Example

```c
switch (16)
{
    case (0)
    {
        message_on(0, 3, 0, 0xFF, 0xFF);
        break(0);
    }
    case (1)
    {
        message_on(0, 4, 0, 0xFF, 0xFF);
        break(0);
    }
}
```

Illustrative: room message 3 or 4 depending on variable 16; any other value
shows nothing.

## See also

- [`switch`](switch.md): the block and its search.
- [`default`](default.md): the section for any other value.
