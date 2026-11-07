# `set`

Clears, sets or toggles one bit of a flag bank.

| | |
| --- | --- |
| Opcode | `0x22` |
| Group | Flags, variables and pad |
| Kind | Instruction |
| Size | 4 bytes |
| Status | VM |

## Syntax

```c
set(bank, index, op);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `bank` | `u8` | The flag bank, `0` to `31` (see [flags and variables](../index.md#flags-and-variables)). |
| `index` | `u8` | The bit in the bank, `0` to `255`. |
| `op` | `u8` | `0` clears the bit, `1` sets it, `7` toggles it. Any other value does nothing. |

## Behaviour

`set` changes the bit and goes on. A bank from 32 up ignores the write. The
bit numbering is the one [`ck`](ck.md) uses: bit `index` is
`0x80000000 >> (index & 31)` of word `index >> 5`.

The flags last: saves keep every bank. Some banks are the engine's own state,
and setting their bits has effects beyond the script:

- Bank 2 (`fg_stop`): bit 7 holds the player's input, bit 3 holds the effect
  sprites.
- Bank 5 (`fg_room`): the room's bits. The first 32 are cleared on every
  entry to the room.
- Banks 13 to 28: the camera mask groups, bank `13 + camera`, bit `group`.
  All are set on every room entry; clearing a bit hides that group.

The full list is in [flags and variables](../index.md#flags-and-variables).
`op` reads differently from RE 1, where `TOGGLE` is 2.

## Example

```c
set(2, 7, 1);                    // hold the player's input
message_on(0, 6, 0, 0, 0);
wsleep();
wsleeping();
set(2, 7, 0);                    // and let it go
```

Illustrative: a scene's message with the player held still.

## See also

- [`ck`](ck.md): tests a flag bit.
- [`save`](save.md): writes a variable.
