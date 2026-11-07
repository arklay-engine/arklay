# `nop_63`

An opcode with no operands and no known effect, for which the port does nothing.

| | |
| --- | --- |
| Opcode | `0x63` |
| Group | Unnamed and unused |
| Kind | Instruction |
| Size | 1 byte |
| Status | no |

## Syntax

```c
nop_63();
```

## Parameters

None.

## Behaviour

Not modelled: the instruction does nothing, and the log notes
`RE2 script: nop_63 is not modelled` once per run.

The port handles it as it does `nop`, `nop_1c`, `nop_1e`, `nop_1f` and
`nop_20`: it does nothing and logs it. Its mnemonic records only its number
and that it has no operands; whether the original's handler does anything has
not been checked.

## Example

```c
proc main_3F
{
    nop_63();
    evt_end(0);
}
```

Illustrative: the instruction's form only.

## See also

- [`nop`](nop.md): the opcode that does nothing.
