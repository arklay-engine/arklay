# `default`

Opens the `switch` section that runs when no `case` matched.

| | |
| --- | --- |
| Opcode | `0x15` |
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
    default
    {
        statements
    }
}
```

The raw form is `@default(0);`.

## Parameters

The structured form has none. The instruction's field, as the `@` form writes
it:

| Parameter | Width | Description |
| --- | --- | --- |
| `_` | `u8` | Not read. Write 0. |

## Behaviour

When the [`switch`](switch.md)'s search reaches a `default`, it stops there
and runs its body, whatever the variable's value. Its body runs to the
[`eswitch`](eswitch.md). The compiler requires `default` to be the last
section; a `case` after it is an error.

When the flow reaches the `default` instruction from the section before, it
does nothing and the task runs on into the `default` body.

## Example

```c
switch (29)
{
    case (0)
    {
        se_on(2, 1, 1, 0, 0, 0);
        break(0);
    }
    default
    {
        se_on(2, 2, 1, 0, 0, 0);
    }
}
```

Illustrative: after [`sce_rnd`](sce_rnd.md), variable 29 holds a random
number; the script plays room sound 1 when it is 0 and room sound 2 otherwise.

## See also

- [`switch`](switch.md), [`case`](case.md): the block and its other sections.
