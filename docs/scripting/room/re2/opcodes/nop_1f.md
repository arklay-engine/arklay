# `nop_1f`

A one-byte opcode with no operation.

| | |
| --- | --- |
| Opcode | `0x1F` |
| Group | Control flow |
| Kind | Instruction |
| Size | 1 byte |
| Status | no |

## Syntax

```c
nop_1f();
```

## Parameters

None.

## Behaviour

The executable's handler for opcode 0x1F does nothing but step over the
byte, and the port does the same: the task goes on with the next instruction
in the same tick. As for any opcode the engine does not model, the first one a
run executes logs `RE2 script: nop_1f is not modelled`. Doing nothing is the
whole of its behaviour.

The retail scripts have no use for it. Prefer [`nop`](nop.md) if a filler byte
is ever needed.

## Example

```c
proc main_03
{
    nop_1f();
    evt_end(0);
}
```

Illustrative: the instruction compiles to the single byte 0x1F.

## See also

- [`nop`](nop.md): the no-op that closes an `else`.
