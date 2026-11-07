# `sce_rnd`

Steps the script's random number and puts it in variable 29.

| | |
| --- | --- |
| Opcode | `0x28` |
| Group | Flags, variables and pad |
| Kind | Instruction |
| Size | 1 byte |
| Status | VM |

## Syntax

```c
sce_rnd();
```

## Parameters

None.

## Behaviour

`sce_rnd` runs the original's `Sce_rnd_set` once and writes the low 16 bits of
the new state to variable 29. The generator is a 32-bit state `r`, stepped as
`r = ((s ^ 2r) & 0xFFFF) ^ 2r`, where `s = ((r & 0x7FFFFFFF) >> 15) + r` and
the arithmetic wraps at 32 bits. Its state starts at
`0x138201C3`, and saves keep it, so the sequence is the same each time from a
given save.

The scheduler also steps the state once at the start of every tick, before
the tasks run, so variable 29 already changes from tick to tick; `sce_rnd`
gives a fresh number within the tick. The Extreme Battle branch of the
original's function is not ported.

To pick among `n` values, take the remainder: `calc(0, 4, 29, n)`. The
result can be negative, because the remainder is signed and the number is
read as a signed word; mask it with `calc(0, 6, 29, 0x7FFF)` first for a
value from 0.

## Example

```c
sce_rnd();
calc(0, 6, 29, 0x7FFF);
calc(0, 4, 29, 3);
switch (29)
{
    case (0)
    {
        xa_on(0, 0x30);
        break(0);
    }
    case (1)
    {
        xa_on(0, 0x31);
        break(0);
    }
    default
    {
        xa_on(0, 0x32);
    }
}
```

Illustrative: one of three voice lines of the stage, at random.

## See also

- [`calc`](calc.md): narrows the number to a range.
- [`switch`](switch.md): picks a section by the result.
