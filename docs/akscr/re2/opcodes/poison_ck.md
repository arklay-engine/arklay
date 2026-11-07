# `poison_ck`

By its name, tests whether the player is poisoned; not modelled, it always passes.

| | |
| --- | --- |
| Opcode | `0x86` |
| Group | Items and the player's state |
| Kind | Condition |
| Size | 1 byte |
| Status | no |

## Syntax

```c
poison_ck();
```

## Parameters

None.

## Behaviour

Not modelled: the condition always passes, and the log notes
`RE2 script: poison_ck is not modelled` once per run.

Its name is the original handler's, `Poison_ck`. The port does not model
poison, so it cannot tell, and the condition passes as if the test held.
Which way round the original's test is (passing when poisoned, or when not)
has not been checked.

## Example

```c
proc main_38
{
    if (poison_ck())
    {
        poison_clr();
    }
    evt_end(0);
}
```

Illustrative: in the port the `if` body always runs, and `poison_clr` does
nothing.

## See also

- [`poison_clr`](poison_clr.md): cures it, also not modelled.
