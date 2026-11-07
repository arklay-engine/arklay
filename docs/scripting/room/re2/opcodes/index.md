# RE 2 opcodes

Every RE 2 opcode, 0x00 to 0x8E, by purpose, with a page each. Every opcode
has a fixed width. The mnemonics are biohazard-utils' where it names the
opcode, and otherwise the name of the handler in the executable. On each
page, the parameter table gives each field's width (`u8`, `i8`, `u16`,
`i16`) and, where the engine reads it, its meaning; a field the engine does
not read is usually written as 0.

The status shows how far the port runs each opcode:

- **VM**: run by the script VM itself (control flow, flags, variables).
- **yes**: the engine models it.
- **partial**: the engine models only the cases noted. Other cases do
  nothing.
- **no**: not modelled yet. The instruction does nothing, a condition
  passes, and the log notes it once per run as `RE2 script: NAME is not
  modelled`. For the `nop` opcodes, doing nothing is the whole of their
  behaviour.

Every opcode compiles and round-trips, whatever its status.

{{#opcode-index Control flow | Flags, variables and pad | Interaction areas and doors | Characters, objects and the work | Camera, room and screen | Messages, sound and music | Effects | Items and the player's state | Unnamed and unused}}
