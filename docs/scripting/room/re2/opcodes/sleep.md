# `sleep`

Waits a number of ticks.

| | |
| --- | --- |
| Opcode | `0x09` |
| Group | Control flow |
| Kind | Instruction |
| Size | 4 bytes |
| Status | VM |

## Syntax

```c
sleep(10, ticks);
```

## Parameters

| Parameter | Width | Description |
| --- | --- | --- |
| `10` | `u8` | The opcode of [`sleeping`](sleeping.md) (`0x0A`), which the VM runs next. Always write 10. |
| `ticks` | `u16` | How many ticks to wait. |

## Behaviour

`sleep` is two instructions in one: the `0x09` byte, then a
[`sleeping`](sleeping.md) instruction (`0A`, then the count) in its last three
bytes. `sleep` puts the count on the task's loop stack and steps one byte
forward, onto its own second byte, which the VM then runs as an opcode. With
10 there, `sleeping` counts the wait down. Any other first operand makes the
VM run that byte as an instruction instead, with the bytes after it as
operands, so always write 10.

`sleeping` gives up the tick each time it runs and takes one off the count.
The task therefore waits `ticks` ticks and goes on with the instruction after
the `sleep` at the tick after that: `sleep(10, 30);` holds the task for 30
ticks, one second at the game's 30 ticks a second. A count of 0 waits as 1
does.

In `init_00`, during the room's load, each pass counts as a tick, and the
init stops after 256 passes in any case.

The wait is an entry on the loop stack, so [`break`](break.md) counts it as
the innermost loop. A task that is sleeping runs no other instruction,
though, so nothing in the same procedure can break the wait in practice.

## Example

```c
proc main_02
{
    set(2, 7, 1);                // hold the player's input
    cut_chg(3);
    sleep(10, 45);               // a second and a half on camera 3
    cut_old();
    set(2, 7, 0);
    evt_end(0);
}
```

Illustrative: a short look through another camera while the player's input
is held.

## See also

- [`sleeping`](sleeping.md): the counting half of `sleep`.
- [`evt_next`](evt_next.md): waits one tick.
- [`wsleep`](wsleep.md): waits for a message or voice line instead of a count.
