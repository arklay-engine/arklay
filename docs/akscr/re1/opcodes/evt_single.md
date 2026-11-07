# `evt_single`

Runs one command from event code; written as the plain command, or `single(N)`.

| | |
| --- | --- |
| Opcode | `0x07` |
| Group | Event: top state |
| Kind | Event instruction |
| Size | 2 bytes plus the command (4 to 255 in all) |
| Valid in | the top state |

## Syntax

```c
name(args);
single(N) name(args);
```

## Parameters

None of its own: the arguments are the command's. The compiler fills in
the length byte.

| Parameter | Width | Description |
| --- | --- | --- |
| `length` | `u8` | Bytes from the opcode to the next event instruction: 2 plus the command's width, or `N` with `single(N)`. |

## Behaviour

Any statement in an event that is not an event instruction or one of the
event forms (`run`, `for`, `do`, `if`, `sleep`, `wait_message`, `raw`) is a
command, and the compiler wraps it in `evt_single`. A procedure call
(`PROC();`) is an `evt_single` holding a [`gosub`](gosub.md).

The VM runs the command and continues at `length` bytes past the opcode, in
the same frame. A condition command here has no effect; to test something,
use an event `if` or a `do`/`while`.

`single(N)` sets `length` to `N` instead of the command's natural width.
Some of the original's events have a length that cuts the command short;
the original's handler still reads the whole command, and the next event
instruction starts inside it. The exporter writes `single(N)` for these so
they build back the same. `N` must be at least 4 and no more than 2 plus
the command's width, and the bytes that follow must still hold the rest of
the command, since the handler reads it; the compiler checks that the
event decodes back. A new script never needs it.

`evt_single` is only valid in the top state. Commands that start or stop
events ([`evt_exec`](evt_exec.md), [`task_kill`](task_kill.md)) take effect
at once, before the next event slot runs.

## Example

```c
sleep(1);
set(FG_RUN, F_RUN_CONTROLS, FALSE);
sleep(100);
timer_setup(LIGHT_FADE_VEIL, 0, FADE_R | FADE_G | FADE_B);
```

Room 100 (`games/re1/room/100/room.akscr`), event `wake_up_after_poison`:
`set` and `timer_setup` each compile to an `evt_single`.

```c
event rebecca_hunter_attack_hit
{
    sleep(8);
    se_play_3d(SND_BANK_ROOM, SE_0B, 0, SND_AT_ENEMY, ENEMY_00_REBECCA, 0, 0);
    single(10) effect(FX_00, FX_ROW_00, FX_AT_WORLD, 13096, -1000, 0, 0);
    raw(0x48, 0x0B, 0x00, 0x04, 0xFF, 0x00);
}
```

Room 101 (`games/re1/room/101/room.akscr`): a shortened payload, whose
length 10 holds 8 bytes of the 12-byte `effect`. The `raw` bytes after it
supply the rest of the command, and the event has no `evt_finish`.

## See also

- [`evt_block`](evt_block.md): several commands, with flow.
- [Statements](../index.md#statements).
