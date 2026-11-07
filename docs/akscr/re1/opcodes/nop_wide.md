# `nop_wide`

Does nothing, over four bytes.

| | |
| --- | --- |
| Opcode | `0x32` |
| Group | Flags and state |
| Kind | Command |
| Size | 4 bytes |

## Syntax

```c
nop_wide(byte1, byte2, byte3);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `byte1` | `u8` | Unused. |
| `byte2` | `u8` | Unused. |
| `byte3` | `u8` | Unused. |

## Behaviour

The instruction is skipped, whatever its three bytes hold. The shipped
scripts carry values in them (`nop_wide(0, 13, 0)`), which the engine never
reads; the decompiler keeps them so the bytecode round-trips. It is written
as raw bytes, so any value from -128 to 255 fits each argument.

## Example

```c
tw_end();
nop_wide(0, 13, 0);
evt_work_set(WK_OBJ, OBJ_00_WATER_0);
evt_tween_begin_alt();
```

Room 301 (`games/re1/room/301/room.akscr`): in an event, between two tween
sequences, as an inline command.

## See also

- [`nop`](nop.md): the 2-byte form.
