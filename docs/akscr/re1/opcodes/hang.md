# `hang`

A dead opcode slot: the original's handler consumes nothing, so its interpreter spins.

| | |
| --- | --- |
| Opcode | `0x26`, `0x2E` |
| Group | Flow |
| Kind | Flow |
| Size | 0 bytes |

## Syntax

None. `hang` cannot be written: there is no statement for it, and neither
`.akscr` nor the `.s` assembler accepts the name.

## Parameters

None.

## Behaviour

In the original game the handlers of opcodes 0x26 and 0x2E return without
advancing the script, so the command interpreter reads the same opcode
again for ever and the game locks up. No shipped script contains either
byte as an instruction.

The port keeps the two slots as zero-width rows of the opcode table so the
table stays indexed by opcode. Its reader does not decode them: decoding of
the block stops at that byte, and the rest of the block is kept as
undecoded bytes. When the command VM reaches that point it finds no
instruction there and stops the whole block list for that pass, as a block
that runs off its end does, rather than hanging the game.

## Example

None: no script can contain it.

## See also

- [`end`](end.md): the way to end a block.
