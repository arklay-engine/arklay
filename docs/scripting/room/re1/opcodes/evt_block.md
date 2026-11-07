# `evt_block`

Runs a block of command code to its end, inside an event; written as `run { ... }`.

| | |
| --- | --- |
| Opcode | `0x06` |
| Group | Event: top state |
| Kind | Event instruction |
| Size | 2 bytes plus the block, at most 255 bytes in all |
| Valid in | the top state |

## Syntax

```c
run
{
    commands...
}

run noreturn
{
    commands...
}
```

## Parameters

None written in `.akscr`. The compiler fills in the bytes:

| Parameter | Width | Description |
| --- | --- | --- |
| `length` | `u8` | Bytes from the opcode to the end of the chain, counting the first byte of its terminator. |
| sub-block size | `u16` | Bytes in the sub-block, counting this word: the command code that follows. |
| terminator | `u16` | 0. Its second byte is the `evt_nop` the VM runs next. |

## Behaviour

The body of a `run` is command code, as in a procedure: commands, `if`,
`else`, conditions, `return;`, `require(...)` and procedure calls. The
event VM runs all of it in one go, with the command VM's flow rules, and
then continues with the next event instruction in the same frame. A `run`
block never yields: nothing in it waits for a frame.

- A condition that fails skips to the end of its `if`, or, outside an
  `if`, ends the block.
- `return;` (the command `end`) ends the block at any depth.
- The block ends with an implicit `end`; `run noreturn` leaves it out, for
  the original's blocks that have none.
- A procedure call (`PROC();`, the command [`gosub`](gosub.md)) runs the
  procedure inside the block, up to 16 calls deep.

The bytes are `0x06`, the length, then a chain of sub-blocks, each a `u16`
size and its command code, closed by a `u16` 0. The length counts only the
first byte of that zero word: the second is an [`evt_nop`](evt_nop.md),
which the VM then runs. The compiler makes one sub-block per `run`; the
original's data sometimes has several, and the VM runs each in turn.

The whole chain must fit in 255 bytes; the compiler says so when a block is
longer. Split a long block into two `run` blocks. A command that needs no
flow can stand alone in the event instead: the compiler writes it as an
[`evt_single`](evt_single.md), which is smaller.

`run` is only valid in the top state. Commands started from it, such as
[`evt_exec`](evt_exec.md) or [`task_kill`](task_kill.md), take effect at
once, before the next event slot runs.

## Example

```c
event first_zombie_film
{
    run
    {
        bgm_stop_all(0);
        movie_on(MOVIE_03_FIRST_ZOMBIE);
    }
    sleep(1);
    run
    {
        room_bgm_state_set(0, ROOM_104_TEA_ROOM, BGM_MAIN_ON);
        set(FG_COMMON, F_COMMON_KENNETH_ZOMBIE_SEEN, TRUE);
    }
    evt_finish();
}
```

Room 104 (`games/re1/room/104/room.akscr`): plays the first zombie film,
then a frame later restores the music and records that it was seen.

## See also

- [`evt_single`](evt_single.md): one command, without a block.
- [Statements](../index.md#statements) and [Command code](../index.md#command-code).
